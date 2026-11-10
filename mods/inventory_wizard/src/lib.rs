//! Inventory Wizard for Kestrel: smart sorting by category, a
//! survival-friendly hotbar, three sort modes and lockable slots.
//! A behaviour port of InventoryWizard by VanishingTacos (MIT),
//! version 2.3.3, on the API 22 screen surface (buttons and the
//! mode button at the two title anchors) and the API 24
//! inventory-actions surface (every sort is a planned sequence of
//! ordinary left clicks, the game's own sort algorithm carrying
//! this mod's ranks and merge rules).
//!
//! Fidelity, against the registry entry: the category order
//! matches — stone, earth, wood, ores, blocks, tools, weapons,
//! armor, food, redstone, transportation, decoration — judged
//! from the item tags and components `recipes.items` lends; the
//! hotbar layout matches (weapons, tools, food, building blocks,
//! utility, the rest); the three modes match (Default,
//! Alphabetical, Stack-based); enchanted or named stacks are
//! never combined; the buttons, the mode button showing the
//! mode's letter, the R key, the unbound-by-default mode key and
//! the /iwiz command with its /inventorywizard alias all match,
//! as does the reach (inventory, hotbar, chests, barrels, shulker
//! boxes, ender chests, singleplayer and servers). Gaps, all
//! surface limits, documented rather than substituted: Alt+Click
//! locking and Shift+Right-Click sorting observe ordinary slot
//! clicks, which no SDK surface reports — locks live on the
//! settings page instead (choose a slot, flip Locked; they
//! persist and the planner honours them) and the sort gestures
//! are the buttons, the key and the command; keys and commands
//! fire only while playing with no screen open, so in a screen
//! the buttons carry the sorts and R sorts the whole inventory
//! rather than the part under the cursor. Vanilla stack sizes
//! cross no read, so the planner carries the game's table for
//! the sizes that differ from 64, as in the other sorting ports.

use std::collections::{HashMap, HashSet, VecDeque};

use horizon_mod_sdk::{
    Anchor, ClickKind, Mod, Registration, ScreenButton, ScreenEvent, input,
    inventory::{self, InvStack},
    inventory_actions, recipes, screen, storage,
    ui::{self, ButtonWidget, Widget},
};

#[cfg(test)]
const API_LEVEL: u32 = 24;

// Screen buttons (API 22).
const BTN_CONTAINER_SORT: u32 = 1;
const BTN_CONTAINER_MODE: u32 = 2;
const BTN_INVENTORY_SORT: u32 = 3;
const BTN_INVENTORY_MODE: u32 = 4;

// Settings buttons (Options > Mods > Inventory Wizard).
const SET_MODE: u32 = 101;
const SET_LOCK_TARGET: u32 = 102;
const SET_LOCK_TOGGLE: u32 = 103;
const SET_LOCK_CLEAR: u32 = 104;

const SORT_KEY: &str = "sort";
const MODE_KEY: &str = "mode";

const MODE_KEY_STORAGE: &str = "mode";
const LOCKS_KEY: &str = "locks";

/// The sort modes, by settings index; the mode button shows the
/// mode's letter.
const MODES: [&str; 3] = ["Default", "Alphabetical", "Stack-based"];
const MODE_LETTERS: [&str; 3] = ["D", "A", "S"];
const MODE_DEFAULT: usize = 0;
const MODE_ALPHABETICAL: usize = 1;
const MODE_STACK: usize = 2;

/// Clicks queued per tick while a plan plays out — the district's
/// per-call ceiling; the original has no speed setting.
const PACE: usize = 32;

// The category order, the original's own list.
const CAT_STONE: usize = 0;
const CAT_EARTH: usize = 1;
const CAT_WOOD: usize = 2;
const CAT_ORES: usize = 3;
const CAT_BLOCKS: usize = 4;
const CAT_TOOLS: usize = 5;
const CAT_WEAPONS: usize = 6;
const CAT_ARMOR: usize = 7;
const CAT_FOOD: usize = 8;
const CAT_REDSTONE: usize = 9;
const CAT_TRANSPORT: usize = 10;
const CAT_DECOR: usize = 11;
const CAT_OTHER: usize = 12;

// The survival hotbar's classes, in its order.
const HOT_WEAPON: usize = 0;
const HOT_TOOL: usize = 1;
const HOT_FOOD: usize = 2;
const HOT_BLOCK: usize = 3;
const HOT_UTILITY: usize = 4;
const HOT_REST: usize = 5;

// ---------------------------------------------------------------------------
// Items: categories and hotbar classes, judged from tags and id.
// ---------------------------------------------------------------------------

fn path_of(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}

fn has_tag(tags: &[String], name: &str) -> bool {
    tags.iter().any(|t| t == name || t.ends_with(&format!(":{name}")))
}

fn is_weapon(path: &str, tags: &[String]) -> bool {
    path.ends_with("_sword")
        || matches!(path, "bow" | "crossbow" | "trident" | "mace")
        || has_tag(tags, "swords")
}

fn is_tool(path: &str, tags: &[String]) -> bool {
    path.ends_with("_pickaxe")
        || path.ends_with("_axe")
        || path.ends_with("_shovel")
        || path.ends_with("_hoe")
        || matches!(path, "shears" | "fishing_rod" | "flint_and_steel" | "brush")
        || has_tag(tags, "pickaxes")
        || has_tag(tags, "axes")
        || has_tag(tags, "shovels")
        || has_tag(tags, "hoes")
}

fn is_armor(path: &str) -> bool {
    path.ends_with("_helmet")
        || path.ends_with("_chestplate")
        || path.ends_with("_leggings")
        || path.ends_with("_boots")
        || path.ends_with("_horse_armor")
        || matches!(path, "elytra" | "shield" | "turtle_helmet" | "wolf_armor")
}

/// The item's category, in the original's order of categories.
/// Checks run in an order that settles overlaps (a stone sword is
/// a weapon, redstone dust is redstone, an ore is an ore).
fn category(id: &str, tags: &[String]) -> usize {
    let path = path_of(id);
    if matches!(
        path,
        "redstone"
            | "redstone_torch"
            | "redstone_block"
            | "repeater"
            | "comparator"
            | "piston"
            | "sticky_piston"
            | "observer"
            | "dispenser"
            | "dropper"
            | "hopper"
            | "lever"
            | "target"
            | "daylight_detector"
            | "tripwire_hook"
            | "sculk_sensor"
            | "calibrated_sculk_sensor"
            | "lightning_rod"
            | "string"
    ) || path.ends_with("_pressure_plate")
        || path.ends_with("_button")
        || path == "tnt"
    {
        return CAT_REDSTONE;
    }
    if is_weapon(path, tags) {
        return CAT_WEAPONS;
    }
    if is_tool(path, tags) {
        return CAT_TOOLS;
    }
    if is_armor(path) {
        return CAT_ARMOR;
    }
    if path.contains("_ore")
        || matches!(
            path,
            "raw_iron"
                | "raw_gold"
                | "raw_copper"
                | "coal"
                | "diamond"
                | "emerald"
                | "lapis_lazuli"
                | "ancient_debris"
                | "netherite_scrap"
                | "nether_quartz"
                | "amethyst_shard"
        )
    {
        return CAT_ORES;
    }
    if path.contains("stone")
        || has_tag(tags, "stone_bricks")
        || has_tag(tags, "base_stone_overworld")
        || has_tag(tags, "base_stone_nether")
        || matches!(
            path,
            "obsidian" | "crying_obsidian" | "bedrock" | "netherrack" | "basalt"
                | "smooth_basalt" | "magma_block" | "tuff" | "calcite" | "dripstone_block"
        )
    {
        return CAT_STONE;
    }
    if matches!(
        path,
        "dirt"
            | "coarse_dirt"
            | "rooted_dirt"
            | "grass_block"
            | "podzol"
            | "mycelium"
            | "dirt_path"
            | "farmland"
            | "mud"
            | "packed_mud"
            | "sand"
            | "red_sand"
            | "gravel"
            | "clay"
            | "soul_sand"
            | "soul_soil"
            | "snow"
            | "snow_block"
            | "ice"
            | "packed_ice"
            | "blue_ice"
    ) || has_tag(tags, "sand")
        || has_tag(tags, "dirt")
    {
        return CAT_EARTH;
    }
    if has_tag(tags, "logs")
        || has_tag(tags, "planks")
        || has_tag(tags, "wooden_slabs")
        || has_tag(tags, "wooden_stairs")
        || has_tag(tags, "wooden_fences")
        || has_tag(tags, "wooden_doors")
        || has_tag(tags, "wooden_trapdoors")
        || path.contains("_log")
        || path.contains("_wood")
        || path.contains("_planks")
        || path.contains("_stem")
        || path.contains("_hyphae")
    {
        return CAT_WOOD;
    }
    if matches!(
        path,
        "apple"
            | "golden_apple"
            | "enchanted_golden_apple"
            | "bread"
            | "beef"
            | "cooked_beef"
            | "porkchop"
            | "cooked_porkchop"
            | "chicken"
            | "cooked_chicken"
            | "mutton"
            | "cooked_mutton"
            | "rabbit"
            | "cooked_rabbit"
            | "cod"
            | "cooked_cod"
            | "salmon"
            | "cooked_salmon"
            | "tropical_fish"
            | "pufferfish"
            | "potato"
            | "baked_potato"
            | "poisonous_potato"
            | "carrot"
            | "golden_carrot"
            | "beetroot"
            | "beetroot_soup"
            | "melon_slice"
            | "glistering_melon_slice"
            | "sweet_berries"
            | "glow_berries"
            | "chorus_fruit"
            | "dried_kelp"
            | "cookie"
            | "pumpkin_pie"
            | "rotten_flesh"
            | "spider_eye"
            | "honey_bottle"
            | "milk_bucket"
            | "mushroom_stew"
            | "rabbit_stew"
            | "suspicious_stew"
            | "cake"
    ) || has_tag(tags, "food")
    {
        return CAT_FOOD;
    }
    if path.ends_with("_boat")
        || path.ends_with("_raft")
        || path.contains("minecart")
        || matches!(path, "saddle" | "lead" | "carrot_on_a_stick" | "warped_fungus_on_a_stick")
    {
        return CAT_TRANSPORT;
    }
    if path.ends_with("_sapling")
        || path.ends_with("_leaves")
        || path.ends_with("_flower")
        || path.ends_with("_tulip")
        || path.ends_with("_carpet")
        || path.ends_with("_candle")
        || path.ends_with("_bed")
        || path.ends_with("_banner")
        || matches!(
            path,
            "poppy"
                | "dandelion"
                | "cornflower"
                | "azure_bluet"
                | "oxeye_daisy"
                | "lily_of_the_valley"
                | "wither_rose"
                | "sunflower"
                | "lilac"
                | "rose_bush"
                | "peony"
                | "pink_petals"
                | "torchflower"
                | "pitcher_plant"
                | "spore_blossom"
                | "painting"
                | "item_frame"
                | "glow_item_frame"
                | "flower_pot"
                | "lantern"
                | "soul_lantern"
                | "torch"
                | "soul_torch"
                | "end_rod"
                | "sea_pickle"
                | "lily_pad"
                | "vine"
                | "glow_lichen"
        )
    {
        return CAT_DECOR;
    }
    if path.ends_with("_block")
        || path.ends_with("_bricks")
        || path.ends_with("_slab")
        || path.ends_with("_stairs")
        || path.ends_with("_wall")
        || path.ends_with("_fence")
        || path.ends_with("_door")
        || path.ends_with("_trapdoor")
        || path.ends_with("_pane")
        || path.ends_with("_glass")
        || path == "glass"
        || path.contains("concrete")
        || path.contains("terracotta")
        || path.ends_with("_wool")
        || matches!(path, "bricks" | "prismarine" | "bookshelf" | "hay_block")
    {
        return CAT_BLOCKS;
    }
    CAT_OTHER
}

/// The hotbar's survival class for one item.
fn hotbar_class(id: &str, tags: &[String]) -> usize {
    let path = path_of(id);
    if is_weapon(path, tags) {
        return HOT_WEAPON;
    }
    if is_tool(path, tags) {
        return HOT_TOOL;
    }
    let cat = category(id, tags);
    if cat == CAT_FOOD {
        return HOT_FOOD;
    }
    if matches!(cat, CAT_STONE | CAT_EARTH | CAT_WOOD | CAT_BLOCKS) {
        return HOT_BLOCK;
    }
    if matches!(
        path,
        "torch"
            | "soul_torch"
            | "ender_pearl"
            | "bucket"
            | "water_bucket"
            | "lava_bucket"
            | "milk_bucket"
            | "compass"
            | "recovery_compass"
            | "clock"
            | "empty_map"
            | "map"
            | "name_tag"
            | "lead"
            | "firework_rocket"
            | "totem_of_undying"
            | "experience_bottle"
            | "spyglass"
            | "bundle"
            | "shield"
            | "snowball"
            | "egg"
            | "potion"
            | "splash_potion"
            | "lingering_potion"
            | "arrow"
            | "spectral_arrow"
            | "tipped_arrow"
            | "goat_horn"
            | "book"
            | "enchanted_book"
            | "writable_book"
    ) || path.starts_with("music_disc_")
    {
        return HOT_UTILITY;
    }
    HOT_REST
}

// ---------------------------------------------------------------------------
// The planner — the game's own sort algorithm with this mod's
// ranks and merge rules. Pure, so sequences are unit-tested.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct PStack {
    id: String,
    components: String,
    count: u32,
}

impl PStack {
    fn identity(&self) -> (String, String) {
        (self.id.clone(), self.components.clone())
    }

    /// Enchanted or named: the wizard never combines these.
    fn special(&self) -> bool {
        self.components.contains("enchantments") || self.components.contains("custom_name")
    }
}

#[derive(Clone, Debug)]
struct SlotView {
    slot: u32,
    stack: Option<PStack>,
}

fn view_of(stack: &InvStack) -> PStack {
    PStack {
        id: stack.id.clone(),
        components: stack.components.clone(),
        count: stack.count,
    }
}

fn range_views(range: std::ops::RangeInclusive<u32>, by_slot: &HashMap<u32, InvStack>) -> Vec<SlotView> {
    range
        .map(|slot| SlotView {
            slot,
            stack: by_slot.get(&slot).map(|s| view_of(s)),
        })
        .collect()
}

fn container_views(by_slot: &HashMap<u32, InvStack>) -> Option<Vec<SlotView>> {
    let max = by_slot.keys().copied().filter(|s| (300..500).contains(s)).max()?;
    Some(range_views(300..=max, by_slot))
}

fn stack_limit_base(path: &str) -> u32 {
    match path {
        "snowball" | "egg" | "ender_pearl" | "armor_stand" | "written_book" | "bucket" => 16,
        "honey_bottle" => 4,
        "potion" | "splash_potion" | "lingering_potion" | "enchanted_book" | "writable_book"
        | "totem_of_undying" | "goat_horn" | "saddle" | "cake" | "mushroom_stew"
        | "rabbit_stew" | "beetroot_soup" | "suspicious_stew" | "milk_bucket" | "water_bucket"
        | "lava_bucket" | "powder_snow_bucket" | "cod_bucket" | "salmon_bucket"
        | "tropical_fish_bucket" | "pufferfish_bucket" | "axolotl_bucket" | "tadpole_bucket"
        | "minecart" | "chest_minecart" | "furnace_minecart" | "hopper_minecart"
        | "tnt_minecart" | "command_block_minecart" | "leather_horse_armor"
        | "iron_horse_armor" | "golden_horse_armor" | "diamond_horse_armor"
        | "netherite_horse_armor" | "wolf_armor" | "bundle" | "shulker_box" => 1,
        _ => {
            if path.ends_with("_sign") || path.ends_with("_hanging_sign") || path.ends_with("_banner") {
                16
            } else if path.ends_with("_bed")
                || path.ends_with("_boat")
                || path.ends_with("_raft")
                || path.ends_with("_shulker_box")
                || path.ends_with("_bundle")
                || path.starts_with("music_disc_")
            {
                1
            } else {
                64
            }
        }
    }
}

fn stack_limit(id: &str, visible: &[&InvStack]) -> u32 {
    let mut limit = stack_limit_base(path_of(id));
    for stack in visible {
        if stack.id == id {
            if stack.max_damage > 0 {
                return 1;
            }
            limit = limit.max(stack.count);
        }
    }
    limit
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    a: u64,
    b: u64,
    s: String,
    t: String,
}

/// The item registry: order, localized names and tags.
#[derive(Default)]
struct Registry {
    order: HashMap<String, u32>,
    names: HashMap<String, String>,
    tags: HashMap<String, Vec<String>>,
}

impl Registry {
    fn load() -> Registry {
        let mut registry = Registry::default();
        for (i, item) in recipes::items().into_iter().enumerate() {
            registry.order.insert(item.id.clone(), i as u32);
            registry.names.insert(item.id.clone(), item.name);
            registry.tags.insert(item.id, item.tags);
        }
        registry
    }

    fn index(&self, id: &str) -> u64 {
        self.order.get(id).map_or(u64::MAX, |i| u64::from(*i))
    }

    fn name(&self, id: &str) -> String {
        self.names.get(id).cloned().unwrap_or_else(|| id.to_string())
    }

    fn tags(&self, id: &str) -> Vec<String> {
        self.tags.get(id).cloned().unwrap_or_default()
    }
}

/// One stack's key under the current mode, for an inventory or
/// container run (the hotbar's survival layout ranks separately).
fn mode_key(mode: usize, registry: &Registry, totals: &HashMap<(String, String), u32>, stack: &PStack) -> SortKey {
    match mode {
        MODE_ALPHABETICAL => SortKey {
            a: 0,
            b: 0,
            s: registry.name(&stack.id).to_lowercase(),
            t: stack.id.clone(),
        },
        MODE_STACK => SortKey {
            a: u64::from(u32::MAX - totals.get(&stack.identity()).copied().unwrap_or(0)),
            b: registry.index(&stack.id),
            s: stack.id.clone(),
            t: String::new(),
        },
        _ => SortKey {
            a: category(&stack.id, &registry.tags(&stack.id)) as u64,
            b: registry.index(&stack.id),
            s: stack.id.clone(),
            t: String::new(),
        },
    }
}

/// Whether two stacks may combine: same item, same components,
/// and neither enchanted nor named.
fn mergeable(a: &PStack, b: &PStack) -> bool {
    a.id == b.id && a.components == b.components && !a.special() && !b.special()
}

struct Sim<'a> {
    slots: Vec<Option<PStack>>,
    hand: Option<PStack>,
    numbers: Vec<u32>,
    limit: &'a dyn Fn(&str) -> u32,
}

impl Sim<'_> {
    fn click(&mut self, i: usize, out: &mut Vec<u32>) {
        out.push(self.numbers[i]);
        match (&mut self.hand, &mut self.slots[i]) {
            (None, slot @ Some(_)) => self.hand = slot.take(),
            (hand @ Some(_), None) => self.slots[i] = hand.take(),
            (Some(hand), Some(slot)) => {
                if hand.id == slot.id && hand.components == slot.components {
                    let max = (self.limit)(&slot.id);
                    if slot.count < max {
                        let moved = (max - slot.count).min(hand.count);
                        slot.count += moved;
                        hand.count -= moved;
                        if hand.count == 0 {
                            self.hand = None;
                        }
                    } else {
                        std::mem::swap(hand, slot);
                    }
                } else {
                    std::mem::swap(hand, slot);
                }
            }
            (None, None) => {}
        }
    }

    fn partial(&self, i: usize) -> bool {
        match &self.slots[i] {
            Some(stack) => stack.count < (self.limit)(&stack.id),
            None => false,
        }
    }
}

/// Plans a sort of one run (locked slots are removed by the
/// caller): merge, then permute into the ranked layout, biggest
/// stacks of one rank first. Empty when the layout is unreachable.
fn plan_sort(
    run: &[SlotView],
    limit: &dyn Fn(&str) -> u32,
    rank: &dyn Fn(&PStack) -> SortKey,
) -> Vec<u32> {
    let mut sim = Sim {
        slots: run.iter().map(|v| v.stack.clone()).collect(),
        hand: None,
        numbers: run.iter().map(|v| v.slot).collect(),
        limit,
    };
    let mut clicks = Vec::new();
    let n = sim.slots.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let mergeable = sim.partial(i)
                && matches!(&sim.slots[j], Some(b) if matches!(&sim.slots[i], Some(a) if mergeable(a, b)));
            if !mergeable {
                continue;
            }
            sim.click(j, &mut clicks);
            sim.click(i, &mut clicks);
            if sim.hand.is_some() {
                let back = j;
                sim.click(back, &mut clicks);
            }
            if !sim.partial(i) {
                break;
            }
        }
    }
    let mut stacks: Vec<PStack> = sim.slots.iter().flatten().cloned().collect();
    stacks.sort_by(|x, y| rank(x).cmp(&rank(y)).then(y.count.cmp(&x.count)));
    let mut target: Vec<Option<PStack>> = stacks.into_iter().map(Some).collect();
    target.resize(n, None);
    for i in 0..n {
        if sim.slots[i] == target[i] {
            continue;
        }
        let Some(j) = ((i + 1)..n).find(|&j| sim.slots[j] == target[i]) else {
            return Vec::new();
        };
        sim.click(j, &mut clicks);
        sim.click(i, &mut clicks);
        if sim.hand.is_some() {
            sim.click(j, &mut clicks);
        }
    }
    if sim.hand.is_some() {
        return Vec::new();
    }
    clicks
}

// ---------------------------------------------------------------------------
// The mod.
// ---------------------------------------------------------------------------

struct Pending {
    clicks: VecDeque<u32>,
    screen: String,
    container: Option<String>,
}

struct InventoryWizard {
    mode: usize,
    locks: HashSet<u32>,
    lock_target: u32,
    registry: Registry,
    registry_tried: bool,
    pending: Option<Pending>,
    declared: String,
}

impl InventoryWizard {
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: SET_MODE,
                label: format!("Mode: {}", MODES[self.mode]),
            }),
            Widget::Button(ButtonWidget {
                id: SET_LOCK_TARGET,
                label: format!("Lock slot: {}", slot_label(self.lock_target)),
            }),
            Widget::Button(ButtonWidget {
                id: SET_LOCK_TOGGLE,
                label: format!(
                    "{} {}: {}",
                    if self.locks.contains(&self.lock_target) {
                        "Unlock"
                    } else {
                        "Lock"
                    },
                    slot_label(self.lock_target),
                    if self.locks.contains(&self.lock_target) {
                        "locked"
                    } else {
                        "free"
                    }
                ),
            }),
            Widget::Button(ButtonWidget {
                id: SET_LOCK_CLEAR,
                label: format!("Clear all locks ({} locked)", self.locks.len()),
            }),
        ]);
    }

    fn save_locks(&self) {
        let mut slots: Vec<u32> = self.locks.iter().copied().collect();
        slots.sort_unstable();
        let text = slots
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let _ = storage::set(LOCKS_KEY, text.as_bytes());
    }

    fn cycle_mode(&mut self) {
        self.mode = (self.mode + 1) % MODES.len();
        let _ = storage::set(MODE_KEY_STORAGE, self.mode.to_string().as_bytes());
    }

    fn queue(&mut self, clicks: Vec<u32>, screen: &str, container: Option<String>) {
        if clicks.is_empty() {
            return;
        }
        self.pending = Some(Pending {
            clicks: clicks.into(),
            screen: screen.to_string(),
            container,
        });
    }

    /// Removes the locked slots from a player-side run.
    fn unlocked(&self, run: Vec<SlotView>) -> Vec<SlotView> {
        run.into_iter()
            .filter(|v| !self.locks.contains(&v.slot))
            .collect()
    }

    /// Sorts runs (each an ordered slot run) under rank functions
    /// chosen per run, into one queued plan.
    fn sort_runs(
        &mut self,
        runs: &[(Vec<SlotView>, bool)],
        by_slot: &HashMap<u32, InvStack>,
        screen: &str,
        container: Option<String>,
    ) {
        let visible: Vec<&InvStack> = by_slot.values().collect();
        let limit = |id: &str| stack_limit(id, &visible);
        let mut clicks = Vec::new();
        for (run, hotbar) in runs {
            let totals: HashMap<(String, String), u32> = run
                .iter()
                .filter_map(|v| v.stack.as_ref())
                .fold(HashMap::new(), |mut acc, s| {
                    *acc.entry(s.identity()).or_insert(0) += s.count;
                    acc
                });
            let registry = &self.registry;
            let mode = self.mode;
            let hotbar = *hotbar;
            let rank = |s: &PStack| {
                if hotbar {
                    SortKey {
                        a: hotbar_class(&s.id, &registry.tags(&s.id)) as u64,
                        b: registry.index(&s.id),
                        s: s.id.clone(),
                        t: String::new(),
                    }
                } else {
                    mode_key(mode, registry, &totals, s)
                }
            };
            clicks.extend(plan_sort(run, &limit, &rank));
        }
        self.queue(clicks, screen, container);
    }

    /// The full player sort: main inventory under the mode, the
    /// hotbar under the survival layout, locks honoured.
    fn sort_player(&mut self, what: &str, by_slot: &HashMap<u32, InvStack>, screen: &str, container: Option<String>) {
        let main = self.unlocked(range_views(9..=35, by_slot));
        let hotbar = self.unlocked(range_views(0..=8, by_slot));
        let runs: Vec<(Vec<SlotView>, bool)> = match what {
            "hotbar" => vec![(hotbar, true)],
            "inventory" => vec![(main, false)],
            _ => vec![(main, false), (hotbar, true)],
        };
        self.sort_runs(&runs, by_slot, screen, container);
    }

    fn sort_container(&mut self, by_slot: &HashMap<u32, InvStack>, screen: &str, container: Option<String>) {
        if let Some(group) = container_views(by_slot) {
            self.sort_runs(&[(group, false)], by_slot, screen, container);
        }
    }

    fn context(&self) -> (inventory::PlayerInventory, Option<inventory::Container>, HashMap<u32, InvStack>) {
        let inv = inventory::inventory();
        let container = inventory::open_container();
        let mut by_slot: HashMap<u32, InvStack> = inv.slots.iter().map(|s| (s.slot, s.clone())).collect();
        if let Some(container) = &container {
            for stack in &container.slots {
                by_slot.insert(stack.slot, stack.clone());
            }
        }
        (inv, container, by_slot)
    }

    fn on_button(&mut self, id: u32) {
        match id {
            BTN_CONTAINER_MODE | BTN_INVENTORY_MODE => self.cycle_mode(),
            BTN_CONTAINER_SORT | BTN_INVENTORY_SORT => {
                let state = screen::state();
                if state.id != "inventory" {
                    return;
                }
                let (inv, container, by_slot) = self.context();
                if inv.carried.is_some() {
                    return;
                }
                let kind = container.as_ref().map(|c| c.kind.clone());
                if id == BTN_CONTAINER_SORT {
                    self.sort_container(&by_slot, "inventory", kind);
                } else {
                    self.sort_player("all", &by_slot, "inventory", kind);
                }
            }
            _ => {}
        }
    }

    fn on_key(&mut self, key: &str) {
        match key {
            MODE_KEY => self.cycle_mode(),
            SORT_KEY => {
                let (inv, _, by_slot) = self.context();
                if inv.carried.is_some() {
                    return;
                }
                self.sort_player("all", &by_slot, "playing", None);
            }
            _ => {}
        }
    }

    fn drain(&mut self) {
        let Some(pending) = &self.pending else { return };
        let state = screen::state();
        let container = inventory::open_container();
        let kind = container.as_ref().map(|c| c.kind.clone());
        let carried = inventory::inventory().carried.is_some();
        if state.id != pending.screen || kind != pending.container || carried {
            self.pending = None;
            return;
        }
        let pending = self.pending.as_mut().expect("checked above");
        for _ in 0..PACE {
            let Some(slot) = pending.clicks.pop_front() else { break };
            let _ = inventory_actions::click(Some(slot), 0, ClickKind::Pickup);
        }
        if pending.clicks.is_empty() {
            self.pending = None;
        }
    }

    fn declare(&mut self) {
        let state = screen::state();
        let container_open = inventory::open_container().is_some();
        let mut buttons = Vec::new();
        if state.id == "inventory" {
            if container_open {
                buttons.push(ScreenButton {
                    id: BTN_CONTAINER_SORT,
                    screen: "inventory".into(),
                    menu: None,
                    anchor: Anchor::ContainerHead,
                    label: "Sort".into(),
                    icon: None,
                });
                buttons.push(ScreenButton {
                    id: BTN_CONTAINER_MODE,
                    screen: "inventory".into(),
                    menu: None,
                    anchor: Anchor::ContainerHead,
                    label: MODE_LETTERS[self.mode].into(),
                    icon: None,
                });
            }
            buttons.push(ScreenButton {
                id: BTN_INVENTORY_SORT,
                screen: "inventory".into(),
                menu: None,
                anchor: Anchor::InventoryHead,
                label: "Sort".into(),
                icon: None,
            });
            buttons.push(ScreenButton {
                id: BTN_INVENTORY_MODE,
                screen: "inventory".into(),
                menu: None,
                anchor: Anchor::InventoryHead,
                label: MODE_LETTERS[self.mode].into(),
                icon: None,
            });
        }
        let spelling = buttons
            .iter()
            .map(|b| format!("{}:{}", b.id, b.label))
            .collect::<Vec<_>>()
            .join("|");
        if spelling != self.declared {
            screen::set_buttons(&buttons);
            self.declared = spelling;
        }
    }
}

fn slot_label(slot: u32) -> String {
    if slot < 9 {
        format!("Hotbar {}", slot + 1)
    } else {
        format!("Inventory {}", slot - 8)
    }
}

fn read_index(key: &str, len: usize, default: usize) -> usize {
    let Some(bytes) = storage::get(key) else {
        return default;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return default;
    };
    match text.trim().parse::<usize>() {
        Ok(v) if v < len => v,
        _ => default,
    }
}

fn read_locks() -> HashSet<u32> {
    let Some(bytes) = storage::get(LOCKS_KEY) else {
        return HashSet::new();
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return HashSet::new();
    };
    text.split(',')
        .filter_map(|part| part.trim().parse::<u32>().ok())
        .filter(|slot| *slot < 36)
        .collect()
}

impl Mod for InventoryWizard {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(SORT_KEY, "Sort inventory", "key.keyboard.r")?;
        input::register_key(MODE_KEY, "Cycle sort mode", "key.keyboard.unknown")?;
        let state = InventoryWizard {
            mode: read_index(MODE_KEY_STORAGE, MODES.len(), MODE_DEFAULT),
            locks: read_locks(),
            lock_target: 0,
            registry: Registry::default(),
            registry_tried: false,
            pending: None,
            declared: String::new(),
        };
        Ok((
            state,
            Registration::new()
                .on_tick()
                .command("iwiz")
                .command("inventorywizard"),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        if !self.registry_tried {
            self.registry = Registry::load();
            self.registry_tried = !self.registry.order.is_empty();
        }
        for event in screen::poll() {
            if let ScreenEvent::Button(id) = event {
                self.on_button(id);
            }
        }
        for event in input::poll() {
            if let input::Event::KeyDown(key) = event {
                self.on_key(&key);
            }
        }
        self.drain();
        self.declare();
    }

    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        let arg = args.trim();
        match arg {
            "mode" => {
                self.cycle_mode();
                Ok(Some(format!("Inventory Wizard: mode {}", MODES[self.mode])))
            }
            "" | "all" | "hotbar" | "inventory" => {
                let (inv, _, by_slot) = self.context();
                if inv.carried.is_some() {
                    return Ok(Some(
                        "Inventory Wizard: put what you are holding down first".into(),
                    ));
                }
                let what = if arg.is_empty() { "all" } else { arg };
                self.sort_player(what, &by_slot, "playing", None);
                Ok(Some(format!("Inventory Wizard: sorting {what}")))
            }
            _ => Err("Usage: /iwiz [hotbar|inventory|all|mode]".into()),
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SET_MODE => self.cycle_mode(),
            SET_LOCK_TARGET => {
                self.lock_target = (self.lock_target + 1) % 36;
            }
            SET_LOCK_TOGGLE => {
                if !self.locks.remove(&self.lock_target) {
                    self.locks.insert(self.lock_target);
                }
                self.save_locks();
            }
            SET_LOCK_CLEAR => {
                self.locks.clear();
                self.save_locks();
            }
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(InventoryWizard);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_24() {
        assert_eq!(API_LEVEL, 24);
    }

    fn no_tags() -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn categories_follow_the_originals_order() {
        assert_eq!(category("minecraft:stone", &no_tags()), CAT_STONE);
        assert_eq!(category("minecraft:dirt", &no_tags()), CAT_EARTH);
        assert_eq!(category("minecraft:oak_log", &no_tags()), CAT_WOOD);
        assert_eq!(category("minecraft:iron_ore", &no_tags()), CAT_ORES);
        assert_eq!(category("minecraft:bricks", &no_tags()), CAT_BLOCKS);
        assert_eq!(category("minecraft:iron_pickaxe", &no_tags()), CAT_TOOLS);
        assert_eq!(category("minecraft:iron_sword", &no_tags()), CAT_WEAPONS);
        assert_eq!(category("minecraft:diamond_chestplate", &no_tags()), CAT_ARMOR);
        assert_eq!(category("minecraft:bread", &no_tags()), CAT_FOOD);
        assert_eq!(category("minecraft:redstone", &no_tags()), CAT_REDSTONE);
        assert_eq!(category("minecraft:oak_boat", &no_tags()), CAT_TRANSPORT);
        assert_eq!(category("minecraft:poppy", &no_tags()), CAT_DECOR);
        assert_eq!(category("minecraft:stick", &no_tags()), CAT_OTHER);
    }

    #[test]
    fn overlaps_settle_as_the_game_would() {
        // A stone sword is a weapon, sandstone is stone, a wooden
        // slab carried by its tag is wood, redstone ore is an ore.
        assert_eq!(category("minecraft:stone_sword", &no_tags()), CAT_WEAPONS);
        assert_eq!(category("minecraft:sandstone", &no_tags()), CAT_STONE);
        assert_eq!(
            category("minecraft:oak_slab", &["minecraft:wooden_slabs".to_string()]),
            CAT_WOOD
        );
        assert_eq!(category("minecraft:redstone_ore", &no_tags()), CAT_ORES);
        assert_eq!(category("minecraft:glass", &no_tags()), CAT_BLOCKS);
    }

    #[test]
    fn hotbar_classes_are_the_survival_layout() {
        assert_eq!(hotbar_class("minecraft:iron_sword", &no_tags()), HOT_WEAPON);
        assert_eq!(hotbar_class("minecraft:iron_pickaxe", &no_tags()), HOT_TOOL);
        assert_eq!(hotbar_class("minecraft:bread", &no_tags()), HOT_FOOD);
        assert_eq!(hotbar_class("minecraft:cobblestone", &no_tags()), HOT_BLOCK);
        assert_eq!(hotbar_class("minecraft:torch", &no_tags()), HOT_UTILITY);
        assert_eq!(hotbar_class("minecraft:stick", &no_tags()), HOT_REST);
    }

    fn stack(slot: u32, id: &str, count: u32) -> SlotView {
        SlotView {
            slot,
            stack: Some(PStack {
                id: id.into(),
                components: String::new(),
                count,
            }),
        }
    }

    fn limit64(_: &str) -> u32 {
        64
    }

    #[test]
    fn enchanted_and_named_stacks_never_merge() {
        let plain = PStack {
            id: "minecraft:iron_sword".into(),
            components: String::new(),
            count: 1,
        };
        let mut enchanted = plain.clone();
        enchanted.components = "[enchantments={levels:{sharpness:5}}]".into();
        assert!(mergeable(&plain, &plain));
        assert!(!mergeable(&enchanted, &enchanted));
        let mut named = plain.clone();
        named.components = "[custom_name=Excalibur]".into();
        assert!(!mergeable(&named, &named));
    }

    #[test]
    fn the_probe_sequence_is_exact() {
        // Dirt 20 + dirt 10 merge, then stone (stone category)
        // permutes ahead of the dirt (earth category), and the
        // merged dirt compacts into the second slot of the run.
        let registry = Registry::default();
        let totals: HashMap<(String, String), u32> = HashMap::new();
        let rank = |s: &PStack| mode_key(MODE_DEFAULT, &registry, &totals, s);
        let run = vec![
            stack(9, "minecraft:dirt", 20),
            stack(10, "minecraft:dirt", 10),
            stack(11, "minecraft:stone", 5),
        ];
        assert_eq!(plan_sort(&run, &limit64, &rank), vec![10, 9, 11, 9, 11, 11, 10]);
    }

    #[test]
    fn stack_based_ranks_by_the_runs_totals() {
        let registry = Registry::default();
        let mut totals = HashMap::new();
        totals.insert(("minecraft:dirt".to_string(), String::new()), 90);
        totals.insert(("minecraft:stone".to_string(), String::new()), 3);
        let dirt = PStack {
            id: "minecraft:dirt".into(),
            components: String::new(),
            count: 64,
        };
        let stone = PStack {
            id: "minecraft:stone".into(),
            components: String::new(),
            count: 3,
        };
        assert!(
            mode_key(MODE_STACK, &registry, &totals, &dirt)
                < mode_key(MODE_STACK, &registry, &totals, &stone)
        );
        // And in Default mode the stone category still wins.
        assert!(
            mode_key(MODE_DEFAULT, &registry, &totals, &stone)
                < mode_key(MODE_DEFAULT, &registry, &totals, &dirt)
        );
    }
}

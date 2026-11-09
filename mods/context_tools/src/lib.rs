//! Context Tool Swap for Kestrel: press middle click with a
//! tool in hand, or start breaking a block, and the right tool
//! for the block under the crosshair is swapped out of your
//! hotbar into your hand.
//!
//! A behaviour port of Context Tool Swap (MIT) by TTCjonasTTC
//! and Qwen. Its registry entry gives the classic trigger —
//! "just press middle click when you have a tool in the hand;
//! it will swap the right tool out" — and its repository's
//! README (<https://github.com/TTCjonasTTC/context_toolswap>)
//! gives the rest: the swap also happens when you start
//! breaking a block (or switch the block you're breaking), at
//! the start of a break, never mid-break; it picks the tool
//! correct for the block's drops, or any tool with mining speed
//! at least 2.0 (so swords and the bare hand are never
//! selected), ordering by (correct-for-drops, speed) when the
//! block requires a tool and by pure speed otherwise, ties
//! keeping your current slot; it skips unbreakable and
//! instant-break blocks and broken tools; and `/toolswap`
//! switches it on and off, on by default. This port implements
//! both triggers and that selection, against the Kestrel SDK
//! (the `actions` capability, API 21): the swap itself is one
//! `select-slot`. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/context_tools>.
//!
//! PARTIAL, as the ports roadmap flagged from the start: the
//! swap reaches the hotbar only. The original moves a tool out
//! of the whole inventory; moving a stack from the bag into the
//! hand is the inventory-actions district's (B6) verb, which
//! the SDK does not have yet — a tool that is only in the bag
//! is left there, and the README says so.
//!
//! Two more honest limits. The original asks the game for the
//! block's tool data — which class mines it, what drops need,
//! each tool's speed; the SDK exposes none of that, so the
//! table below renders vanilla's own mineable/tool data as the
//! port's data (classes by block family, tier speeds by
//! material), and a block the table doesn't know simply gets
//! no swap. And the original skips creative and spectator,
//! which a client mod cannot detect (the SDK shows no game
//! mode), so the port swaps there too; the middle click also
//! keeps its vanilla pick-block, because a registered key does
//! not consume the button — the swap lands a tick later and
//! wins.

use horizon_mod_sdk::{
    ActionTarget, Mod, Registration, actions, input,
    inventory::{self, InvStack},
    storage,
};

/// The newest API level this mod needs, asserted in the tests
/// below: `actions.target` and `actions.select-slot` are level
/// 21 (`crates/horizon-mods/src/api_level.rs`); everything else
/// here is older.
#[cfg(test)]
const API_LEVEL: u32 = 21;

/// The swap key: the original's middle click, and the name it
/// is registered under.
const SWAP_KEY: &str = "swap";
const DEFAULT_SWAP_KEY: &str = "key.mouse.middle";

/// The enabled switch's storage key.
const ENABLED_KEY: &str = "enabled";

/// The tool classes a block can call for and an item can be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ToolClass {
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
    Shears,
    Sword,
}

/// The class of an item, by its id: the way the game's own
/// item ids name them. `_pickaxe` is checked before `_axe`
/// because a pickaxe's id ends with it too.
fn class_of_item(id: &str) -> Option<ToolClass> {
    let path = id.rsplit(':').next().unwrap_or(id);
    if path.ends_with("_pickaxe") {
        Some(ToolClass::Pickaxe)
    } else if path.ends_with("_axe") {
        Some(ToolClass::Axe)
    } else if path.ends_with("_shovel") {
        Some(ToolClass::Shovel)
    } else if path.ends_with("_hoe") {
        Some(ToolClass::Hoe)
    } else if path == "shears" {
        Some(ToolClass::Shears)
    } else if path.ends_with("_sword") {
        Some(ToolClass::Sword)
    } else {
        None
    }
}

/// A tool material's mining speed, vanilla's numbers: wood 2,
/// stone 4, copper 5, iron 6, diamond 8, netherite 9, gold 12.
/// Anything else (a hand, shears, a non-tool) reads 1, the
/// bare-hand speed.
fn tier_speed(id: &str) -> f32 {
    let path = id.rsplit(':').next().unwrap_or(id);
    if path.contains("netherite") {
        9.0
    } else if path.contains("diamond") {
        8.0
    } else if path.contains("golden") {
        12.0
    } else if path.contains("iron") {
        6.0
    } else if path.contains("copper") {
        5.0
    } else if path.contains("stone") {
        4.0
    } else if path.contains("wooden") {
        2.0
    } else {
        1.0
    }
}

/// What a block calls for: the class whose tools mine it at
/// their tier speed, the class (and the least tier speed) a
/// drop requires — none when the block drops for any tool or
/// the bare hand, so ordering falls to pure speed — and the
/// special speeds of shears and the sword, which have no tier.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Rule {
    speed_class: Option<ToolClass>,
    drop_class: Option<ToolClass>,
    drop_alt: Option<ToolClass>,
    min_tier: f32,
    shears_speed: f32,
    sword_speed: f32,
}

/// A block any tool of the class mines faster, dropping for
/// anything: ordering is pure speed.
const fn speedy(class: ToolClass) -> Rule {
    Rule {
        speed_class: Some(class),
        drop_class: None,
        drop_alt: None,
        min_tier: 0.0,
        shears_speed: 0.0,
        sword_speed: 0.0,
    }
}

/// A block whose drops need the class at `min_tier` or better.
const fn needs(class: ToolClass, min_tier: f32) -> Rule {
    Rule {
        speed_class: Some(class),
        drop_class: Some(class),
        drop_alt: None,
        min_tier,
        shears_speed: 0.0,
        sword_speed: 0.0,
    }
}

/// The least tier speed an ore's drops require, by its id.
fn ore_min_tier(path: &str) -> f32 {
    if path.contains("diamond") || path.contains("emerald") || path.contains("redstone") {
        6.0 // iron or better
    } else if path.contains("gold") && !path.starts_with("nether") {
        6.0
    } else if path.contains("iron") || path.contains("lapis") || path.contains("copper") {
        4.0 // stone or better
    } else {
        2.0 // any pickaxe: coal, nether gold, nether quartz
    }
}

/// The right tool for a block, from its state string — the
/// port's rendering of vanilla's mineable and tool-tier data
/// (see the module docs). None for blocks no tool is right
/// for: the unbreakable, the instant-break (they are simply
/// not in the table), glass and the rest that every hand
/// mines alike.
fn tool_rule(state: &str) -> Option<Rule> {
    let id = state.split('[').next().unwrap_or(state);
    let path = id.rsplit(':').next().unwrap_or(id);
    // Not blocks a tool is ever swapped for.
    if matches!(
        path,
        "bedrock"
            | "barrier"
            | "end_portal_frame"
            | "reinforced_deepslate"
            | "command_block"
            | "chain_command_block"
            | "repeating_command_block"
            | "structure_block"
            | "jigsaw"
            | "nether_portal"
            | "end_portal"
            | "end_gateway"
            | "light"
    ) || path.contains("glass")
    {
        return None;
    }
    // Leaves: a hoe is fast, shears are fastest and are what
    // the drops want.
    if path.ends_with("_leaves") {
        return Some(Rule {
            speed_class: Some(ToolClass::Hoe),
            drop_class: Some(ToolClass::Shears),
            drop_alt: None,
            min_tier: 0.0,
            shears_speed: 15.0,
            sword_speed: 0.0,
        });
    }
    // Wool: shears are the tool, for speed and drops alike.
    if path == "wool" || path.ends_with("_wool") {
        return Some(Rule {
            speed_class: None,
            drop_class: Some(ToolClass::Shears),
            drop_alt: None,
            min_tier: 0.0,
            shears_speed: 5.0,
            sword_speed: 0.0,
        });
    }
    // Cobweb: shears or a sword, both fast, both drop its
    // string.
    if path == "cobweb" {
        return Some(Rule {
            speed_class: None,
            drop_class: Some(ToolClass::Shears),
            drop_alt: Some(ToolClass::Sword),
            min_tier: 0.0,
            shears_speed: 15.0,
            sword_speed: 15.0,
        });
    }
    // The hoe's ground: the sculk family, wart blocks, and the
    // soft grown things.
    if path.starts_with("sculk")
        || path.contains("_wart_block")
        || matches!(
            path,
            "shroomlight"
                | "hay_bale"
                | "target"
                | "dried_kelp_block"
                | "moss_block"
                | "moss_carpet"
                | "sponge"
                | "wet_sponge"
        )
    {
        return Some(speedy(ToolClass::Hoe));
    }
    // The shovel's ground.
    if matches!(
        path,
        "dirt"
            | "coarse_dirt"
            | "rooted_dirt"
            | "grass_block"
            | "mycelium"
            | "podzol"
            | "dirt_path"
            | "farmland"
            | "sand"
            | "red_sand"
            | "gravel"
            | "clay"
            | "soul_sand"
            | "soul_soil"
            | "snow"
            | "snow_block"
            | "powder_snow"
            | "mud"
            | "muddy_mangrove_roots"
            | "packed_mud"
            | "suspicious_sand"
            | "suspicious_gravel"
    ) || path.ends_with("_concrete_powder")
    {
        return Some(speedy(ToolClass::Shovel));
    }
    // The pickaxe's ground, before the wooden families below
    // can claim its stairs and slabs: the ores first, then the
    // stone and metal families by their names, then the named
    // blocks.
    if path.contains("_ore") {
        return Some(needs(ToolClass::Pickaxe, ore_min_tier(path)));
    }
    if matches!(
        path,
        "obsidian" | "crying_obsidian" | "respawn_anchor" | "netherite_block" | "ancient_debris"
    ) {
        return Some(needs(ToolClass::Pickaxe, 8.0));
    }
    if matches!(
        path,
        "gold_block" | "raw_gold_block" | "diamond_block" | "emerald_block"
    ) {
        return Some(needs(ToolClass::Pickaxe, 6.0));
    }
    if matches!(path, "lapis_block" | "raw_iron_block" | "raw_copper_block") {
        return Some(needs(ToolClass::Pickaxe, 4.0));
    }
    if path.contains("stone")
        || path.contains("deepslate")
        || path.contains("brick")
        || path.contains("copper")
        || path.contains("quartz")
        || path.contains("prismarine")
        || path.contains("purpur")
        || path.contains("amethyst")
        || path.contains("basalt")
        || path.contains("tuff")
        || path.contains("terracotta")
        || path.contains("lantern")
        || path.contains("_coral_block")
        || path.ends_with("_concrete")
        || path.ends_with("_ice")
        || path == "ice"
    {
        return Some(needs(ToolClass::Pickaxe, 2.0));
    }
    if matches!(
        path,
        "granite"
            | "polished_granite"
            | "diorite"
            | "polished_diorite"
            | "andesite"
            | "polished_andesite"
            | "netherrack"
            | "magma_block"
            | "bone_block"
            | "coal_block"
            | "iron_block"
            | "redstone_block"
            | "calcite"
            | "dripstone_block"
            | "pointed_dripstone"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "dispenser"
            | "dropper"
            | "hopper"
            | "brewing_stand"
            | "anvil"
            | "chipped_anvil"
            | "damaged_anvil"
            | "enchanting_table"
            | "ender_chest"
            | "beacon"
            | "conduit"
            | "spawner"
            | "vault"
            | "trial_spawner"
            | "heavy_core"
            | "iron_door"
            | "iron_trapdoor"
            | "iron_bars"
            | "chain"
            | "lightning_rod"
            | "cauldron"
            | "water_cauldron"
            | "lava_cauldron"
            | "powder_snow_cauldron"
            | "rail"
            | "powered_rail"
            | "detector_rail"
            | "activator_rail"
            | "light_weighted_pressure_plate"
            | "heavy_weighted_pressure_plate"
    ) || path.ends_with("_chain")
    {
        return Some(needs(ToolClass::Pickaxe, 2.0));
    }
    // The axe's ground: wood and everything made of it. The
    // stone and metal doors, buttons, plates and stairs were
    // claimed above; what reaches here in those shapes is
    // wooden.
    if path.contains("bookshelf")
        || path.ends_with("_log")
        || path.ends_with("_wood")
        || path.ends_with("_stem")
        || path.ends_with("_hyphae")
        || path.ends_with("_planks")
        || path.ends_with("_fence")
        || path.ends_with("_fence_gate")
        || path.ends_with("_door")
        || path.ends_with("_trapdoor")
        || path.ends_with("_button")
        || path.ends_with("_pressure_plate")
        || path.ends_with("_sign")
        || path.ends_with("_stairs")
        || path.ends_with("_slab")
        || path.ends_with("_shelf")
        || matches!(
            path,
            "crafting_table"
                | "smithing_table"
                | "cartography_table"
                | "fletching_table"
                | "loom"
                | "chest"
                | "trapped_chest"
                | "barrel"
                | "lectern"
                | "composter"
                | "note_block"
                | "jukebox"
                | "beehive"
                | "bee_nest"
                | "pumpkin"
                | "carved_pumpkin"
                | "jack_o_lantern"
                | "melon"
                | "campfire"
                | "soul_campfire"
                | "ladder"
                | "bamboo"
                | "honeycomb_block"
                | "creaking_heart"
        )
    {
        return Some(speedy(ToolClass::Axe));
    }
    None
}

/// A candidate's standing for a block: whether it is correct
/// for the drops, and its effective speed there.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Score {
    correct: bool,
    speed: f32,
}

/// A stack's standing, or none when it is no candidate: a
/// broken tool, or one whose speed on the block is under the
/// original's 2.0 line (which is what keeps swords and the
/// bare hand from ever being selected).
fn score_of(stack: &InvStack, rule: &Rule) -> Option<Score> {
    if stack.max_damage > 0 && stack.damage >= stack.max_damage {
        return None; // broken
    }
    let class = class_of_item(&stack.id);
    let speed = match class {
        Some(c) if Some(c) == rule.speed_class => tier_speed(&stack.id),
        Some(ToolClass::Shears) => rule.shears_speed,
        Some(ToolClass::Sword) => rule.sword_speed,
        _ => 1.0,
    };
    if speed < 2.0 {
        return None;
    }
    let drops = |want: Option<ToolClass>| {
        want.is_some_and(|dc| class == Some(dc) && tier_speed(&stack.id) >= rule.min_tier)
    };
    Some(Score {
        correct: drops(rule.drop_class) || drops(rule.drop_alt),
        speed,
    })
}

/// Better: correct-for-drops first, then faster — the
/// original's ordering.
fn better(a: Score, b: Score) -> bool {
    (a.correct, a.speed) > (b.correct, b.speed)
}

/// The hotbar slot to swap to for `rule`'s block, or none:
/// the best candidate carried on the bar, with ties keeping
/// the current slot — including when the best is already in
/// hand.
fn choose(slots: &[InvStack], selected: u32, rule: &Rule) -> Option<u32> {
    let mut best: Option<(u32, Score)> = None;
    let mut current: Option<Score> = None;
    for stack in slots.iter().filter(|s| s.slot < 9) {
        let score = score_of(stack, rule);
        if stack.slot == selected {
            current = score;
        }
        if let Some(score) = score
            && best.is_none_or(|(_, b)| better(score, b))
        {
            best = Some((stack.slot, score));
        }
    }
    let (slot, standing) = best?;
    if slot == selected || current == Some(standing) {
        return None;
    }
    Some(slot)
}

/// Which trigger is asking for a swap.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trigger {
    /// The middle-click key (the registry entry's trigger):
    /// only with a tool in hand, as the original says.
    Key,
    /// Starting to break a block (the repository README's
    /// trigger): with a tool in hand, or bare-handed — the
    /// hand is never a candidate, but a swap away from it is
    /// the mod's point. A non-tool item in hand is left alone:
    /// the player is not mining with it.
    Attack,
}

struct ContextTools {
    enabled: bool,
    /// The attack button last tick, and the block it was
    /// breaking: the swap fires at a break's start and when
    /// the block being broken changes.
    attack_down: bool,
    attack_target: Option<[i32; 3]>,
}

impl ContextTools {
    fn status(&self) -> String {
        format!("Tool Swap: {}", if self.enabled { "on" } else { "off" })
    }

    fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        let _ = storage::set(ENABLED_KEY, if on { b"true" } else { b"false" });
    }

    /// One swap attempt, for the block under the crosshair.
    fn try_swap(&self, trigger: Trigger) {
        if !self.enabled {
            return;
        }
        let Some(ActionTarget::Block(block)) = actions::target() else {
            return;
        };
        let Some(rule) = tool_rule(&block.block) else {
            return;
        };
        let held = input::held();
        match trigger {
            Trigger::Key => {
                if !held.is_some_and(|h| class_of_item(&h.id).is_some()) {
                    return;
                }
            }
            Trigger::Attack => {
                if held.is_some_and(|h| class_of_item(&h.id).is_none()) {
                    return;
                }
            }
        }
        let inv = inventory::inventory();
        if let Some(slot) = choose(&inv.slots, inv.selected, &rule) {
            let _ = actions::select_slot(slot);
        }
    }
}

impl Mod for ContextTools {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(SWAP_KEY, "Swap to the right tool", DEFAULT_SWAP_KEY)?;
        let enabled = match storage::get(ENABLED_KEY) {
            Some(bytes) => std::str::from_utf8(&bytes).ok().map(str::trim) != Some("false"),
            None => true,
        };
        let state = ContextTools {
            enabled,
            attack_down: false,
            attack_target: None,
        };
        Ok((state, Registration::new().on_tick().command("toolswap")))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            if let input::Event::KeyDown(k) = event
                && k == SWAP_KEY
            {
                self.try_swap(Trigger::Key);
            }
        }
        let attack = input::mapping("key.attack").is_some_and(|m| m.down);
        let target = match actions::target() {
            Some(ActionTarget::Block(block)) => Some([block.block_x, block.block_y, block.block_z]),
            _ => None,
        };
        if attack && (!self.attack_down || (target.is_some() && target != self.attack_target)) {
            self.try_swap(Trigger::Attack);
        }
        self.attack_down = attack;
        self.attack_target = if attack { target } else { None };
    }

    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        match args.trim() {
            "" | "status" => Ok(Some(self.status())),
            "on" => {
                self.set_enabled(true);
                Ok(Some(self.status()))
            }
            "off" => {
                self.set_enabled(false);
                Ok(Some(self.status()))
            }
            "toggle" => {
                self.set_enabled(!self.enabled);
                Ok(Some(self.status()))
            }
            _ => Err("Usage: /toolswap [on|off|toggle]".into()),
        }
    }
}

horizon_mod_sdk::export_mod!(ContextTools);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_21() {
        assert_eq!(API_LEVEL, 21);
    }

    fn stack(slot: u32, id: &str) -> InvStack {
        InvStack {
            slot,
            id: id.into(),
            count: 1,
            damage: 0,
            max_damage: 0,
            components: String::new(),
        }
    }

    fn tool(slot: u32, id: &str, max_damage: u32) -> InvStack {
        InvStack {
            damage: 0,
            max_damage,
            ..stack(slot, id)
        }
    }

    #[test]
    fn item_classes_and_tiers_read_their_ids() {
        assert_eq!(
            class_of_item("minecraft:iron_pickaxe"),
            Some(ToolClass::Pickaxe)
        );
        assert_eq!(class_of_item("minecraft:iron_axe"), Some(ToolClass::Axe));
        assert_eq!(
            class_of_item("minecraft:wooden_shovel"),
            Some(ToolClass::Shovel)
        );
        assert_eq!(class_of_item("minecraft:diamond_hoe"), Some(ToolClass::Hoe));
        assert_eq!(class_of_item("minecraft:shears"), Some(ToolClass::Shears));
        assert_eq!(
            class_of_item("minecraft:netherite_sword"),
            Some(ToolClass::Sword)
        );
        assert_eq!(class_of_item("minecraft:stone"), None);
        assert_eq!(tier_speed("minecraft:wooden_pickaxe"), 2.0);
        assert_eq!(tier_speed("minecraft:stone_pickaxe"), 4.0);
        assert_eq!(tier_speed("minecraft:copper_pickaxe"), 5.0);
        assert_eq!(tier_speed("minecraft:iron_pickaxe"), 6.0);
        assert_eq!(tier_speed("minecraft:diamond_pickaxe"), 8.0);
        assert_eq!(tier_speed("minecraft:netherite_pickaxe"), 9.0);
        assert_eq!(tier_speed("minecraft:golden_pickaxe"), 12.0);
    }

    #[test]
    fn block_rules_name_the_right_tool() {
        let pick = |min| Some(needs(ToolClass::Pickaxe, min));
        // Stone and its family: any pickaxe drops them.
        assert_eq!(tool_rule("minecraft:stone"), pick(2.0));
        assert_eq!(tool_rule("minecraft:cobblestone_stairs"), pick(2.0));
        assert_eq!(tool_rule("minecraft:deepslate_bricks"), pick(2.0));
        assert_eq!(tool_rule("minecraft:sandstone"), pick(2.0));
        // Ores by the tier their drops need.
        assert_eq!(tool_rule("minecraft:coal_ore"), pick(2.0));
        assert_eq!(tool_rule("minecraft:iron_ore"), pick(4.0));
        assert_eq!(tool_rule("minecraft:lapis_ore"), pick(4.0));
        assert_eq!(tool_rule("minecraft:gold_ore"), pick(6.0));
        assert_eq!(tool_rule("minecraft:diamond_ore"), pick(6.0));
        assert_eq!(tool_rule("minecraft:obsidian"), pick(8.0));
        assert_eq!(tool_rule("minecraft:ancient_debris"), pick(8.0));
        // Wood is the axe's, for speed only.
        assert_eq!(tool_rule("minecraft:oak_log"), Some(speedy(ToolClass::Axe)));
        assert_eq!(
            tool_rule("minecraft:oak_planks"),
            Some(speedy(ToolClass::Axe))
        );
        assert_eq!(
            tool_rule("minecraft:crafting_table"),
            Some(speedy(ToolClass::Axe))
        );
        assert_eq!(
            tool_rule("minecraft:smithing_table"),
            Some(speedy(ToolClass::Axe))
        );
        assert_eq!(tool_rule("minecraft:pumpkin"), Some(speedy(ToolClass::Axe)));
        // ...but a nether brick fence is stone, not wood, and a
        // stone button is not a wooden one.
        assert_eq!(tool_rule("minecraft:nether_brick_fence"), pick(2.0));
        assert_eq!(tool_rule("minecraft:stone_button"), pick(2.0));
        assert_eq!(
            tool_rule("minecraft:oak_button"),
            Some(speedy(ToolClass::Axe))
        );
        assert_eq!(tool_rule("minecraft:iron_door"), pick(2.0));
        assert_eq!(tool_rule("minecraft:copper_door"), pick(2.0));
        // Dirt and its kin are the shovel's.
        assert_eq!(tool_rule("minecraft:dirt"), Some(speedy(ToolClass::Shovel)));
        assert_eq!(
            tool_rule("minecraft:grass_block"),
            Some(speedy(ToolClass::Shovel))
        );
        assert_eq!(
            tool_rule("minecraft:gravel"),
            Some(speedy(ToolClass::Shovel))
        );
        // Sculk is the hoe's; leaves want shears for their
        // drops and a hoe (or shears) for speed.
        assert_eq!(tool_rule("minecraft:sculk"), Some(speedy(ToolClass::Hoe)));
        let leaves = tool_rule("minecraft:oak_leaves").unwrap();
        assert_eq!(leaves.drop_class, Some(ToolClass::Shears));
        assert_eq!(leaves.speed_class, Some(ToolClass::Hoe));
        let wool = tool_rule("minecraft:white_wool").unwrap();
        assert_eq!(wool.drop_class, Some(ToolClass::Shears));
        assert_eq!(wool.shears_speed, 5.0);
        // No swap: the unbreakable, glass, and the instant.
        assert_eq!(tool_rule("minecraft:bedrock"), None);
        assert_eq!(tool_rule("minecraft:glass"), None);
        assert_eq!(tool_rule("minecraft:reinforced_deepslate"), None);
        assert_eq!(tool_rule("minecraft:short_grass"), None);
        assert_eq!(tool_rule("minecraft:torch"), None);
        assert_eq!(tool_rule("minecraft:wheat[age=7]"), None);
    }

    #[test]
    fn choosing_orders_by_drops_then_speed_and_keeps_ties() {
        let stone = tool_rule("minecraft:stone").unwrap();
        let bar = [
            tool(0, "minecraft:wooden_pickaxe", 59),
            tool(1, "minecraft:diamond_pickaxe", 1561),
            stack(2, "minecraft:dirt"),
        ];
        // Holding the wooden pickaxe: the diamond is correct
        // too and faster.
        assert_eq!(choose(&bar, 0, &stone), Some(1));
        // Holding the diamond: nothing beats it.
        assert_eq!(choose(&bar, 1, &stone), None);
        // Two equal pickaxes: a tie keeps the current slot.
        let twins = [
            tool(0, "minecraft:iron_pickaxe", 250),
            tool(3, "minecraft:iron_pickaxe", 250),
        ];
        assert_eq!(choose(&twins, 0, &stone), None);
        assert_eq!(choose(&twins, 3, &stone), None);
        // Diamond ore needs iron or better for its drops: the
        // stone pickaxe is faster than nothing but not correct,
        // and correctness outranks the wooden one's... both
        // lose to the iron pickaxe.
        let diamond_ore = tool_rule("minecraft:diamond_ore").unwrap();
        let bar = [
            tool(0, "minecraft:stone_pickaxe", 131),
            tool(1, "minecraft:iron_pickaxe", 250),
        ];
        assert_eq!(choose(&bar, 0, &diamond_ore), Some(1));
        // A sword is never selected, even for want of anything
        // else.
        let only_a_sword = [
            tool(0, "minecraft:iron_axe", 250),
            tool(1, "minecraft:diamond_sword", 1561),
        ];
        assert_eq!(choose(&only_a_sword, 0, &stone), None);
        // A broken tool is skipped.
        let mut broken = tool(1, "minecraft:diamond_pickaxe", 1561);
        broken.damage = 1561;
        let bar = [tool(0, "minecraft:wooden_pickaxe", 59), broken];
        assert_eq!(choose(&bar, 0, &stone), None);
        // Shears beat a hoe to leaves: they are what the drops
        // want, and faster besides.
        let leaves = tool_rule("minecraft:oak_leaves").unwrap();
        let bar = [
            tool(0, "minecraft:iron_hoe", 250),
            tool(1, "minecraft:shears", 238),
        ];
        assert_eq!(choose(&bar, 0, &leaves), Some(1));
        // A tool in the bag is out of reach (the port's known
        // limit): no swap.
        let bag_only = [
            stack(0, "minecraft:dirt"),
            tool(20, "minecraft:diamond_pickaxe", 1561),
        ];
        assert_eq!(choose(&bag_only, 0, &stone), None);
    }
}

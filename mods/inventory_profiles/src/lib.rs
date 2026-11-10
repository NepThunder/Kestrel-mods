//! Inventory Profiles for Kestrel: four sort modes, auto-refill,
//! tool break protection and quick-stack. A behaviour port of
//! Inventory Profiles Next and its Inventory Sorter companion,
//! ported to Horizon by CayMac (MIT), on the API 22 screen
//! surface (sort and quick-stack buttons at the title anchors)
//! and the API 24 inventory-actions surface (sorts as planned
//! click sequences; auto-refill and tool protection as the one
//! Swap click each — bag slot × selected hotbar slot — that the
//! design prescribes; quick-stack as quick-move clicks).
//!
//! Fidelity, against the registry entry: the four modes match —
//! Category (Creative Tab), Alphabetical (A–Z), Stack Count
//! (High–Low), Item ID — as do the R key and the UI sort buttons,
//! auto-refill of a depleted hand stack, tool break protection
//! and the G key's quick-stack. Gaps, surface limits, documented
//! rather than substituted: the creative tabs' exact order
//! crosses no SDK read, so Category sorts by the game's item
//! registry order, which follows the tabs closely (the other
//! three modes are exact); keys fire only while playing with no
//! screen open, so in a screen the sort and quick-stack live on
//! their buttons; auto-refill and tool protection watch only
//! while playing, not while a screen is open, so organizing an
//! inventory never triggers them. Tool protection triggers when
//! the held tool has one use left — the last moment a swap can
//! still save it — and prefers a healthier tool of the same kind,
//! else parks the dying tool in a free inventory slot. Vanilla
//! stack sizes cross no read, so the planner carries the game's
//! table for the sizes that differ from 64, as in the other
//! sorting ports.

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
const BTN_QUICK_STACK: u32 = 5;

// Settings buttons (Options > Mods > Inventory Profiles).
const SET_MODE: u32 = 101;
const SET_REFILL: u32 = 102;
const SET_PROTECTION: u32 = 103;

const SORT_KEY: &str = "sort";
const QUICK_STACK_KEY: &str = "quick_stack";

const MODE_KEY: &str = "mode";
const REFILL_KEY: &str = "auto_refill";
const PROTECTION_KEY: &str = "tool_protection";

/// The four sort modes, by settings index, with the short labels
/// the mode button shows.
const MODES: [&str; 4] = ["Category", "Alphabetical", "Stack Count", "Item ID"];
const MODE_SHORT: [&str; 4] = ["Category", "A-Z", "Count", "ID"];
const MODE_CATEGORY: usize = 0;
const MODE_ALPHA: usize = 1;
const MODE_COUNT: usize = 2;
const MODE_ID: usize = 3;

/// Clicks queued per tick while a plan plays out — the district's
/// per-call ceiling; the original has no speed setting.
const PACE: usize = 32;

// ---------------------------------------------------------------------------
// The planner — the game's own sort algorithm with this mod's
// ranks and vanilla's merge rules. Pure, so sequences are
// unit-tested below.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct PStack {
    id: String,
    components: String,
    count: u32,
    damage: u32,
    max_damage: u32,
}

impl PStack {
    fn identity(&self) -> (String, String) {
        (self.id.clone(), self.components.clone())
    }

    /// Uses left before the item breaks; u32::MAX when the item
    /// takes no damage.
    fn remaining(&self) -> u32 {
        if self.max_damage == 0 {
            u32::MAX
        } else {
            self.max_damage.saturating_sub(self.damage)
        }
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
        damage: stack.damage,
        max_damage: stack.max_damage,
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
    let path = id.rsplit(':').next().unwrap_or(id);
    let mut limit = stack_limit_base(path);
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

#[derive(Default)]
struct Registry {
    order: HashMap<String, u32>,
    names: HashMap<String, String>,
}

impl Registry {
    fn load() -> Registry {
        let mut registry = Registry::default();
        for (i, item) in recipes::items().into_iter().enumerate() {
            registry.order.insert(item.id.clone(), i as u32);
            registry.names.insert(item.id.clone(), item.name);
        }
        registry
    }

    fn index(&self, id: &str) -> u64 {
        self.order.get(id).map_or(u64::MAX, |i| u64::from(*i))
    }

    fn name(&self, id: &str) -> String {
        self.names.get(id).cloned().unwrap_or_else(|| id.to_string())
    }
}

fn mode_key(mode: usize, registry: &Registry, totals: &HashMap<(String, String), u32>, stack: &PStack) -> SortKey {
    match mode {
        MODE_ALPHA => SortKey {
            a: 0,
            b: 0,
            s: registry.name(&stack.id).to_lowercase(),
            t: stack.id.clone(),
        },
        MODE_COUNT => SortKey {
            a: u64::from(u32::MAX - totals.get(&stack.identity()).copied().unwrap_or(0)),
            b: registry.index(&stack.id),
            s: stack.id.clone(),
            t: String::new(),
        },
        MODE_ID => SortKey {
            a: 0,
            b: 0,
            s: stack.id.clone(),
            t: String::new(),
        },
        _ => SortKey {
            a: registry.index(&stack.id),
            b: 0,
            s: stack.id.clone(),
            t: String::new(),
        },
    }
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
                && matches!(&sim.slots[j], Some(b) if matches!(&sim.slots[i], Some(a) if a.id == b.id && a.components == b.components));
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

/// Quick-stack: the player-side slots holding what the container
/// already holds.
fn plan_quick_stack(player: &[SlotView], container: &[SlotView]) -> Vec<u32> {
    let identities: HashSet<(String, String)> =
        container.iter().filter_map(|v| v.stack.as_ref().map(PStack::identity)).collect();
    player
        .iter()
        .filter(|v| v.stack.as_ref().is_some_and(|s| identities.contains(&s.identity())))
        .map(|v| v.slot)
        .collect()
}

/// Auto-refill: the selected slot held `prev` a tick ago and is
/// empty now — the bag slot (main inventory views, empties
/// included) to swap a fresh stack of the same item from.
fn refill_plan(prev: &PStack, bag: &[SlotView]) -> Option<u32> {
    bag.iter()
        .find(|v| v.stack.as_ref().is_some_and(|s| s.identity() == prev.identity() && s.count > 0))
        .map(|v| v.slot)
}

/// Tool break protection: the held stack crossed to one use left
/// since the previous tick — the bag slot to swap with: a
/// healthier stack of the same item first, else a free slot to
/// park the dying tool in.
fn protection_plan(prev: Option<&PStack>, held: &PStack, bag: &[SlotView]) -> Option<u32> {
    if held.max_damage == 0 || held.remaining() > 1 {
        return None;
    }
    // No fresh crossing, no swap: the stack was already dying a
    // tick ago (a swap already happened, or the player chose it).
    if let Some(prev) = prev
        && prev.id == held.id
        && prev.damage == held.damage
    {
        return None;
    }
    if let Some(prev) = prev
        && prev.id == held.id
        && prev.remaining() <= 1
    {
        return None;
    }
    let healthier = bag.iter().find(|v| {
        v.stack
            .as_ref()
            .is_some_and(|s| s.id == held.id && s.max_damage > 0 && s.remaining() > 1)
    });
    if let Some(view) = healthier {
        return Some(view.slot);
    }
    bag.iter().find(|v| v.stack.is_none()).map(|v| v.slot)
}

// ---------------------------------------------------------------------------
// The mod.
// ---------------------------------------------------------------------------

struct Pending {
    clicks: VecDeque<(u32, u32, ClickKind)>,
    screen: String,
    container: Option<String>,
}

struct InventoryProfiles {
    mode: usize,
    auto_refill: bool,
    protection: bool,
    registry: Registry,
    registry_tried: bool,
    pending: Option<Pending>,
    declared: String,
    /// The selected slot and its stack, last tick (the watchers).
    prev_selected: u32,
    prev_held: Option<PStack>,
}

impl InventoryProfiles {
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: SET_MODE,
                label: format!("Sort mode: {}", MODES[self.mode]),
            }),
            Widget::Button(ButtonWidget {
                id: SET_REFILL,
                label: format!("Auto-refill: {}", if self.auto_refill { "on" } else { "off" }),
            }),
            Widget::Button(ButtonWidget {
                id: SET_PROTECTION,
                label: format!(
                    "Tool break protection: {}",
                    if self.protection { "on" } else { "off" }
                ),
            }),
        ]);
    }

    fn cycle_mode(&mut self) {
        self.mode = (self.mode + 1) % MODES.len();
        let _ = storage::set(MODE_KEY, self.mode.to_string().as_bytes());
    }

    fn queue(&mut self, clicks: Vec<(u32, u32, ClickKind)>, screen: &str, container: Option<String>) {
        if clicks.is_empty() {
            return;
        }
        self.pending = Some(Pending {
            clicks: clicks.into(),
            screen: screen.to_string(),
            container,
        });
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

    fn sort_runs(&mut self, runs: &[Vec<SlotView>], by_slot: &HashMap<u32, InvStack>, screen: &str, container: Option<String>) {
        let visible: Vec<&InvStack> = by_slot.values().collect();
        let limit = |id: &str| stack_limit(id, &visible);
        let mut clicks = Vec::new();
        for run in runs {
            let totals: HashMap<(String, String), u32> = run
                .iter()
                .filter_map(|v| v.stack.as_ref())
                .fold(HashMap::new(), |mut acc, s| {
                    *acc.entry(s.identity()).or_insert(0) += s.count;
                    acc
                });
            let registry = &self.registry;
            let mode = self.mode;
            let rank = |s: &PStack| mode_key(mode, registry, &totals, s);
            clicks.extend(
                plan_sort(run, &limit, &rank)
                    .into_iter()
                    .map(|slot| (slot, 0, ClickKind::Pickup)),
            );
        }
        self.queue(clicks, screen, container);
    }

    fn sort_player(&mut self, by_slot: &HashMap<u32, InvStack>, screen: &str, container: Option<String>) {
        let runs = vec![range_views(9..=35, by_slot), range_views(0..=8, by_slot)];
        self.sort_runs(&runs, by_slot, screen, container);
    }

    fn sort_container(&mut self, by_slot: &HashMap<u32, InvStack>, screen: &str, container: Option<String>) {
        if let Some(group) = container_views(by_slot) {
            self.sort_runs(&[group], by_slot, screen, container);
        }
    }

    fn quick_stack(&mut self, by_slot: &HashMap<u32, InvStack>, screen: &str, container: Option<String>) {
        let Some(open) = inventory::open_container() else {
            return;
        };
        let player = range_views(0..=35, by_slot);
        let dest: Vec<SlotView> = open
            .slots
            .iter()
            .map(|s| SlotView {
                slot: s.slot,
                stack: Some(view_of(s)),
            })
            .collect();
        let clicks = plan_quick_stack(&player, &dest)
            .into_iter()
            .map(|slot| (slot, 0, ClickKind::QuickMove))
            .collect();
        self.queue(clicks, screen, container);
    }

    fn on_button(&mut self, id: u32) {
        match id {
            BTN_CONTAINER_MODE | BTN_INVENTORY_MODE => self.cycle_mode(),
            BTN_CONTAINER_SORT | BTN_INVENTORY_SORT | BTN_QUICK_STACK => {
                let state = screen::state();
                if state.id != "inventory" {
                    return;
                }
                let (inv, container, by_slot) = self.context();
                if inv.carried.is_some() {
                    return;
                }
                let kind = container.as_ref().map(|c| c.kind.clone());
                match id {
                    BTN_CONTAINER_SORT => self.sort_container(&by_slot, "inventory", kind),
                    BTN_INVENTORY_SORT => self.sort_player(&by_slot, "inventory", kind),
                    _ => self.quick_stack(&by_slot, "inventory", kind),
                }
            }
            _ => {}
        }
    }

    fn on_key(&mut self, key: &str) {
        let (inv, _, by_slot) = self.context();
        if inv.carried.is_some() {
            return;
        }
        match key {
            SORT_KEY => self.sort_player(&by_slot, "playing", None),
            QUICK_STACK_KEY => self.quick_stack(&by_slot, "playing", None),
            _ => {}
        }
    }

    /// Auto-refill and tool break protection, watching the
    /// selected slot while playing. Each fires at most one Swap
    /// click — bag slot, button the selected hotbar slot.
    fn watch(&mut self) {
        if screen::state().id != "playing" {
            self.prev_held = None;
            return;
        }
        let inv = inventory::inventory();
        let selected = inv.selected;
        let held = inv
            .slots
            .iter()
            .find(|s| s.slot == selected)
            .map(view_of);
        if selected != self.prev_selected {
            self.prev_selected = selected;
            self.prev_held = held;
            return;
        }
        let bag: Vec<SlotView> = {
            let by_slot: HashMap<u32, InvStack> =
                inv.slots.iter().map(|s| (s.slot, s.clone())).collect();
            range_views(9..=35, &by_slot)
        };
        if self.protection
            && let Some(held_now) = &held
            && let Some(target) = protection_plan(self.prev_held.as_ref(), held_now, &bag)
        {
            let _ = inventory_actions::click(Some(target), selected, ClickKind::Swap);
            self.prev_held = held;
            return;
        }
        if self.auto_refill
            && held.is_none()
            && let Some(prev) = &self.prev_held
            && prev.count > 0
            && let Some(source) = refill_plan(prev, &bag)
        {
            let _ = inventory_actions::click(Some(source), selected, ClickKind::Swap);
        }
        self.prev_held = held;
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
            let Some((slot, button, kind)) = pending.clicks.pop_front() else { break };
            let _ = inventory_actions::click(Some(slot), button, kind);
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
                    label: MODE_SHORT[self.mode].into(),
                    icon: None,
                });
                buttons.push(ScreenButton {
                    id: BTN_QUICK_STACK,
                    screen: "inventory".into(),
                    menu: None,
                    anchor: Anchor::ContainerHead,
                    label: "Quick-Stack".into(),
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
                label: MODE_SHORT[self.mode].into(),
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

fn read_bool(key: &str, default: bool) -> bool {
    match storage::get(key) {
        Some(bytes) => match std::str::from_utf8(&bytes).ok().map(str::trim) {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        },
        None => default,
    }
}

fn write_bool(key: &str, value: bool) {
    let _ = storage::set(key, if value { b"true" } else { b"false" });
}

impl Mod for InventoryProfiles {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(SORT_KEY, "Sort inventory", "key.keyboard.r")?;
        input::register_key(QUICK_STACK_KEY, "Quick-stack into container", "key.keyboard.g")?;
        let state = InventoryProfiles {
            mode: read_index(MODE_KEY, MODES.len(), MODE_CATEGORY),
            auto_refill: read_bool(REFILL_KEY, true),
            protection: read_bool(PROTECTION_KEY, true),
            registry: Registry::default(),
            registry_tried: false,
            pending: None,
            declared: String::new(),
            prev_selected: 0,
            prev_held: None,
        };
        Ok((state, Registration::new().on_tick()))
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
        self.watch();
        self.drain();
        self.declare();
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SET_MODE => self.cycle_mode(),
            SET_REFILL => {
                self.auto_refill = !self.auto_refill;
                write_bool(REFILL_KEY, self.auto_refill);
            }
            SET_PROTECTION => {
                self.protection = !self.protection;
                write_bool(PROTECTION_KEY, self.protection);
            }
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(InventoryProfiles);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_24() {
        assert_eq!(API_LEVEL, 24);
    }

    fn stack(slot: u32, id: &str, count: u32) -> SlotView {
        SlotView {
            slot,
            stack: Some(PStack {
                id: id.into(),
                components: String::new(),
                count,
                damage: 0,
                max_damage: 0,
            }),
        }
    }

    fn tool(slot: u32, id: &str, damage: u32, max_damage: u32) -> SlotView {
        SlotView {
            slot,
            stack: Some(PStack {
                id: id.into(),
                components: String::new(),
                count: 1,
                damage,
                max_damage,
            }),
        }
    }

    fn empty(slot: u32) -> SlotView {
        SlotView { slot, stack: None }
    }

    fn limit64(_: &str) -> u32 {
        64
    }

    #[test]
    fn the_planner_merges_like_the_other_ports() {
        let registry = Registry::default();
        let totals: HashMap<(String, String), u32> = HashMap::new();
        let rank = |s: &PStack| mode_key(MODE_ID, &registry, &totals, s);
        let run = vec![
            stack(9, "minecraft:dirt", 10),
            stack(10, "minecraft:stone", 5),
            stack(11, "minecraft:dirt", 20),
        ];
        assert_eq!(plan_sort(&run, &limit64, &rank), vec![11, 9]);
    }

    #[test]
    fn count_mode_outranks_category() {
        let mut registry = Registry::default();
        registry.order.insert("minecraft:stone".into(), 0);
        registry.order.insert("minecraft:dirt".into(), 1);
        let mut totals = HashMap::new();
        totals.insert(("minecraft:dirt".to_string(), String::new()), 90);
        totals.insert(("minecraft:stone".to_string(), String::new()), 3);
        let dirt = PStack {
            id: "minecraft:dirt".into(),
            components: String::new(),
            count: 64,
            damage: 0,
            max_damage: 0,
        };
        let stone = PStack {
            id: "minecraft:stone".into(),
            components: String::new(),
            count: 3,
            damage: 0,
            max_damage: 0,
        };
        assert!(mode_key(MODE_COUNT, &registry, &totals, &dirt) < mode_key(MODE_COUNT, &registry, &totals, &stone));
        assert!(mode_key(MODE_CATEGORY, &registry, &totals, &stone) < mode_key(MODE_CATEGORY, &registry, &totals, &dirt));
        assert!(mode_key(MODE_ID, &registry, &totals, &dirt) < mode_key(MODE_ID, &registry, &totals, &stone));
    }

    #[test]
    fn quick_stack_takes_only_what_the_container_holds() {
        let player = vec![
            stack(9, "minecraft:dirt", 20),
            stack(10, "minecraft:stone", 5),
            stack(11, "minecraft:dirt", 3),
        ];
        let container = vec![stack(300, "minecraft:dirt", 10)];
        assert_eq!(plan_quick_stack(&player, &container), vec![9, 11]);
    }

    #[test]
    fn refill_swaps_a_fresh_stack_in() {
        let prev = PStack {
            id: "minecraft:stone".into(),
            components: String::new(),
            count: 1,
            damage: 0,
            max_damage: 0,
        };
        let bag = vec![stack(9, "minecraft:dirt", 5), stack(10, "minecraft:stone", 30)];
        assert_eq!(refill_plan(&prev, &bag), Some(10));
        let no_match = vec![stack(9, "minecraft:dirt", 5)];
        assert_eq!(refill_plan(&prev, &no_match), None);
    }

    #[test]
    fn protection_swaps_on_the_crossing_only() {
        let worn = |damage: u32| PStack {
            id: "minecraft:iron_pickaxe".into(),
            components: String::new(),
            count: 1,
            damage,
            max_damage: 250,
        };
        let bag = vec![tool(10, "minecraft:iron_pickaxe", 0, 250), empty(11)];
        // Two uses left: nothing yet.
        assert_eq!(protection_plan(Some(&worn(247)), &worn(248), &bag), None);
        // The crossing to one use left: swap with the fresh pick.
        assert_eq!(protection_plan(Some(&worn(248)), &worn(249), &bag), Some(10));
        // Already dying a tick ago at the same damage: no repeat.
        assert_eq!(protection_plan(Some(&worn(249)), &worn(249), &bag), None);
        // No healthier pick: park the dying one in the free slot.
        let bag = vec![empty(11)];
        assert_eq!(protection_plan(Some(&worn(248)), &worn(249), &bag), Some(11));
        // No replacement and no free slot: leave it.
        let bag = vec![stack(10, "minecraft:dirt", 5)];
        assert_eq!(protection_plan(Some(&worn(248)), &worn(249), &bag), None);
    }
}

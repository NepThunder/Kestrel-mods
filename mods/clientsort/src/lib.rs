//! ClientSort for Kestrel: buttons beside the container and beside
//! your inventory — Sort, Restock, Transfer — a middle click that
//! sorts the part under the pointer, and keys for sort, restock,
//! transfer and fill stacks. A behaviour port of ClientSort by
//! TerminalMC (Apache-2.0), against the API 22 screen surface (the
//! buttons, the middle-click event, the game's own sort buttons
//! stepping aside for a served area) and the API 24 inventory-actions
//! surface (every operation is planned as ordinary container clicks
//! and played at the click-speed setting, under the district's caps).
//!
//! Fidelity, against the registry entry: the three operations on
//! both sides match — Restock moves in the items the side already
//! holds, Transfer moves as much as fits, container, main inventory
//! and hotbar sort on their own — as do the settings (buttons on/off,
//! default sort order, click speed, hotbar as its own group, restock
//! by item type) and the keys for every operation including Fill
//! Stacks. The order set matches: creative order by default, Shift
//! (the sneak key) for quantity, Ctrl (the sprint key) for name A–Z,
//! item ID. Gaps, all surface limits, never substitutions: the
//! creative inventory's exact order crosses no SDK read, so the
//! default order uses the game's item registry order, which follows
//! it closely (quantity, name and ID orders are exact); Alt has no
//! key mapping to read, so the ID order is chosen as the default
//! order in the settings instead of by a held modifier; trigger keys
//! fire only while playing with no screen open, so in a screen the
//! operations live on the buttons, restock and transfer keys have no
//! container to act on while playing, and Fill Stacks tops the
//! hotbar up from the main inventory; button tooltips are not
//! ported, a sandbox button carries a label only. Vanilla stack
//! sizes cross no read either, so the planner carries the game's
//! table for the sizes that differ from 64, and treats anything with
//! durability as unstackable, as the game does.

use std::collections::{HashMap, HashSet, VecDeque};

use horizon_mod_sdk::{
    Anchor, ClickKind, Mod, Registration, ScreenButton, ScreenEvent, input,
    inventory::{self, InvStack},
    inventory_actions, recipes, screen, storage,
    ui::{self, ButtonWidget, Widget},
};

#[cfg(test)]
const API_LEVEL: u32 = 24;

// Screen buttons (API 22). The game's own sort buttons step aside
// for any area a mod serves.
const BTN_CONTAINER_SORT: u32 = 1;
const BTN_CONTAINER_RESTOCK: u32 = 2;
const BTN_CONTAINER_TRANSFER: u32 = 3;
const BTN_INVENTORY_SORT: u32 = 4;
const BTN_INVENTORY_RESTOCK: u32 = 5;
const BTN_INVENTORY_TRANSFER: u32 = 6;

// Settings buttons (Options > Mods > ClientSort).
const SET_ORDER: u32 = 101;
const SET_SPEED: u32 = 102;
const SET_BUTTONS: u32 = 103;
const SET_HOTBAR_GROUP: u32 = 104;
const SET_RESTOCK_TYPE: u32 = 105;

// Registered keys; they ship unbound, as the registry names no
// defaults — bind them in Options > Mods > ClientSort.
const SORT_KEY: &str = "sort";
const RESTOCK_KEY: &str = "restock";
const TRANSFER_KEY: &str = "transfer";
const FILL_KEY: &str = "fill";

const ORDER_KEY: &str = "order";
const SPEED_KEY: &str = "speed";
const BUTTONS_KEY: &str = "buttons";
const HOTBAR_GROUP_KEY: &str = "hotbar_group";
const RESTOCK_TYPE_KEY: &str = "restock_type";

/// The sort orders, by settings index. The trigger modifiers pick
/// from the same set: sneak (Shift) is Quantity, sprint (Ctrl) Name.
const ORDERS: [&str; 4] = ["Default", "Quantity", "Name", "Item ID"];
const ORDER_DEFAULT: usize = 0;
const ORDER_QUANTITY: usize = 1;
const ORDER_NAME: usize = 2;
const ORDER_ID: usize = 3;

/// Clicks queued per tick, by settings index — the click speed.
/// 32 is the district's per-call ceiling, so the top step is
/// effectively instant.
const SPEEDS: [usize; 6] = [1, 2, 4, 8, 16, 32];
const SPEED_LABELS: [&str; 6] = ["1", "2", "4", "8", "16", "Max"];

// ---------------------------------------------------------------------------
// The planner. Pure: slot views in, click sequences out, so the exact
// sequences are unit-tested below. The simulation mirrors the game's
// own sort planner (merge pass, then permute into the ranked layout);
// a plan that cannot reach its layout yields no clicks at all.
// ---------------------------------------------------------------------------

/// A stack as the planner sees it.
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
}

/// One slot of a sort run: its number and what it holds.
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

/// The run for a slot range, empties synthesized — the inventory
/// reads report only the slots that hold something.
fn range_views(range: std::ops::RangeInclusive<u32>, by_slot: &HashMap<u32, &InvStack>) -> Vec<SlotView> {
    range
        .map(|slot| SlotView {
            slot,
            stack: by_slot.get(&slot).map(|s| view_of(s)),
        })
        .collect()
}

/// The container's own group: chest-like slots (300..500), anchored
/// at 300, running to the highest slot the container reports.
fn container_views(by_slot: &HashMap<u32, &InvStack>) -> Option<Vec<SlotView>> {
    let max = by_slot.keys().copied().filter(|s| (300..500).contains(s)).max()?;
    Some(range_views(300..=max, by_slot))
}

/// Vanilla stack sizes that differ from 64, by item id's path. No
/// SDK read exposes a stack's maximum, so the planner carries this
/// table; anything observed with durability is unstackable instead.
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

/// The stack limit for one item id, from everything on view: a
/// stack with durability makes the item unstackable; otherwise the
/// table value, raised by any larger stack actually observed.
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

/// A sort rank: compared field by field.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    a: u64,
    b: u64,
    s: String,
    t: String,
}

/// The item registry: order and localized names, from
/// `recipes.items()`. The default (creative) order stands on this
/// registry order, as documented in the manifest.
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

/// One stack's key under one order. `totals` is the run's count per
/// identity, for the quantity order.
fn sort_key(order: usize, registry: &Registry, totals: &HashMap<(String, String), u32>, stack: &PStack) -> SortKey {
    match order {
        ORDER_QUANTITY => SortKey {
            a: u64::from(u32::MAX - totals.get(&stack.identity()).copied().unwrap_or(0)),
            b: registry.index(&stack.id),
            s: stack.id.clone(),
            t: String::new(),
        },
        ORDER_NAME => SortKey {
            a: 0,
            b: 0,
            s: registry.name(&stack.id).to_lowercase(),
            t: stack.id.clone(),
        },
        ORDER_ID => SortKey {
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

/// A pickup-click simulation over one run of slots, hand starting
/// and ending empty — the game's own planner semantics.
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

/// Plans a sort of one run: merge partial stacks of one item, then
/// permute into the ranked layout, biggest stacks of one rank first.
/// Empty when the hand does not end empty — no clicks at all, as
/// the game's own planner bails.
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
    // Merge pass.
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
    // The layout: non-empty stacks ranked, fullest of a rank first.
    let mut stacks: Vec<PStack> = sim.slots.iter().flatten().cloned().collect();
    stacks.sort_by(|x, y| rank(x).cmp(&rank(y)).then(y.count.cmp(&x.count)));
    let mut target: Vec<Option<PStack>> = stacks.into_iter().map(Some).collect();
    target.resize(n, None);
    // Permute pass.
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

/// Plans fill-stacks over a combined run (destinations first, then
/// sources): top every partial destination stack up from the
/// sources holding the same item, click for click.
fn plan_fill(run: &[SlotView], destinations: usize, limit: &dyn Fn(&str) -> u32) -> Vec<u32> {
    let mut sim = Sim {
        slots: run.iter().map(|v| v.stack.clone()).collect(),
        hand: None,
        numbers: run.iter().map(|v| v.slot).collect(),
        limit,
    };
    let mut clicks = Vec::new();
    for d in 0..destinations {
        for s in destinations..sim.slots.len() {
            if !sim.partial(d) {
                break;
            }
            let same = matches!(&sim.slots[s], Some(src) if matches!(&sim.slots[d], Some(dst) if src.id == dst.id && src.components == dst.components));
            if !same {
                continue;
            }
            sim.click(s, &mut clicks);
            sim.click(d, &mut clicks);
            if sim.hand.is_some() {
                sim.click(s, &mut clicks);
            }
        }
    }
    clicks
}

/// Restock: the source slots holding what the destination side
/// already holds — by item type, or by exact stack identity when
/// the by-type setting is off.
fn plan_restock(
    source: &[SlotView],
    dest: &[SlotView],
    by_type: bool,
) -> Vec<u32> {
    let types: HashSet<&str> = dest.iter().filter_map(|v| v.stack.as_ref().map(|s| s.id.as_str())).collect();
    let identities: HashSet<(String, String)> =
        dest.iter().filter_map(|v| v.stack.as_ref().map(PStack::identity)).collect();
    source
        .iter()
        .filter(|v| {
            v.stack.as_ref().is_some_and(|s| {
                if by_type {
                    types.contains(s.id.as_str())
                } else {
                    identities.contains(&s.identity())
                }
            })
        })
        .map(|v| v.slot)
        .collect()
}

/// Transfer: every non-empty source slot, as much as fits being
/// the quick-move click's own semantics.
fn plan_transfer(source: &[SlotView]) -> Vec<u32> {
    source.iter().filter(|v| v.stack.is_some()).map(|v| v.slot).collect()
}

// ---------------------------------------------------------------------------
// The mod.
// ---------------------------------------------------------------------------

/// A planned run of clicks and the context it was planned in; the
/// plan is dropped if the screen or the open container changes
/// under it, or the player picks something up.
struct Pending {
    clicks: VecDeque<(u32, ClickKind)>,
    screen: String,
    container: Option<String>,
}

struct ClientSort {
    order: usize,
    speed: usize,
    buttons_on: bool,
    hotbar_group: bool,
    restock_type: bool,
    registry: Registry,
    registry_tried: bool,
    pending: Option<Pending>,
    declared: String,
}

impl ClientSort {
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: SET_ORDER,
                label: format!("Sort order: {}", ORDERS[self.order]),
            }),
            Widget::Button(ButtonWidget {
                id: SET_SPEED,
                label: format!("Click speed: {}", SPEED_LABELS[self.speed]),
            }),
            Widget::Button(ButtonWidget {
                id: SET_BUTTONS,
                label: format!("Buttons: {}", if self.buttons_on { "on" } else { "off" }),
            }),
            Widget::Button(ButtonWidget {
                id: SET_HOTBAR_GROUP,
                label: format!(
                    "Hotbar own group: {}",
                    if self.hotbar_group { "on" } else { "off" }
                ),
            }),
            Widget::Button(ButtonWidget {
                id: SET_RESTOCK_TYPE,
                label: format!(
                    "Restock by item type: {}",
                    if self.restock_type { "on" } else { "off" }
                ),
            }),
        ]);
    }

    /// The order a trigger uses right now: sneak held is quantity,
    /// sprint held is name, otherwise the settings' default order.
    fn trigger_order(&self) -> usize {
        if input::mapping("key.sneak").is_some_and(|m| m.down) {
            ORDER_QUANTITY
        } else if input::mapping("key.sprint").is_some_and(|m| m.down) {
            ORDER_NAME
        } else {
            self.order
        }
    }

    fn queue(&mut self, clicks: Vec<(u32, ClickKind)>, screen: &str, container: Option<String>) {
        if clicks.is_empty() {
            return;
        }
        self.pending = Some(Pending {
            clicks: clicks.into(),
            screen: screen.to_string(),
            container,
        });
    }

    /// Sorts the given groups (each an ordered run of slot views),
    /// one plan, queued together.
    fn sort_groups(
        &mut self,
        groups: &[Vec<SlotView>],
        order: usize,
        visible: &[&InvStack],
        screen: &str,
        container: Option<String>,
    ) {
        let limit = |id: &str| stack_limit(id, visible);
        let mut clicks = Vec::new();
        for group in groups {
            let totals: HashMap<(String, String), u32> = group
                .iter()
                .filter_map(|v| v.stack.as_ref())
                .fold(HashMap::new(), |mut acc, s| {
                    *acc.entry(s.identity()).or_insert(0) += s.count;
                    acc
                });
            let rank = |s: &PStack| sort_key(order, &self.registry, &totals, s);
            clicks.extend(plan_sort(group, &limit, &rank).into_iter().map(|slot| (slot, ClickKind::Pickup)));
        }
        self.queue(clicks, screen, container);
    }

    /// The player's own groups under the hotbar-group setting.
    fn player_groups(&self, by_slot: &HashMap<u32, &InvStack>) -> Vec<Vec<SlotView>> {
        if self.hotbar_group {
            vec![range_views(9..=35, by_slot), range_views(0..=8, by_slot)]
        } else {
            let mut run = range_views(9..=35, by_slot);
            run.extend(range_views(0..=8, by_slot));
            vec![run]
        }
    }

    fn sort_player(&mut self, order: usize, by_slot: &HashMap<u32, &InvStack>, visible: &[&InvStack], screen: &str, container: Option<String>) {
        let groups = self.player_groups(by_slot);
        self.sort_groups(&groups, order, visible, screen, container);
    }

    fn sort_container(&mut self, order: usize, by_slot: &HashMap<u32, &InvStack>, visible: &[&InvStack], screen: &str, container: Option<String>) {
        if let Some(group) = container_views(by_slot) {
            self.sort_groups(&[group], order, visible, screen, container);
        }
    }

    /// Restock or transfer into one side from the other.
    fn move_into(
        &mut self,
        source: &[SlotView],
        dest: &[SlotView],
        restock: bool,
        screen: &str,
        container: Option<String>,
    ) {
        let slots = if restock {
            plan_restock(source, dest, self.restock_type)
        } else {
            plan_transfer(source)
        };
        self.queue(
            slots.into_iter().map(|slot| (slot, ClickKind::QuickMove)).collect(),
            screen,
            container,
        );
    }

    fn on_button(&mut self, id: u32) {
        let state = screen::state();
        if state.id != "inventory" {
            return;
        }
        let inv = inventory::inventory();
        if inv.carried.is_some() {
            return;
        }
        let container = inventory::open_container();
        let kind = container.as_ref().map(|c| c.kind.clone());
        let mut by_slot: HashMap<u32, &InvStack> = inv.slots.iter().map(|s| (s.slot, s)).collect();
        if let Some(container) = &container {
            for stack in &container.slots {
                by_slot.insert(stack.slot, stack);
            }
        }
        let visible: Vec<&InvStack> = by_slot.values().copied().collect();
        let player_all = range_views(0..=35, &by_slot);
        // Restock and transfer see every slot the container reports
        // (a station's inputs included); sorting uses the
        // chest-like run only.
        let container_all: Vec<SlotView> = match &container {
            Some(container) => {
                let mut views: Vec<SlotView> = container
                    .slots
                    .iter()
                    .map(|s| SlotView {
                        slot: s.slot,
                        stack: Some(view_of(s)),
                    })
                    .collect();
                views.sort_by_key(|v| v.slot);
                views
            }
            None => Vec::new(),
        };
        let order = self.trigger_order();
        match id {
            BTN_CONTAINER_SORT => self.sort_container(order, &by_slot, &visible, "inventory", kind),
            BTN_INVENTORY_SORT => self.sort_player(order, &by_slot, &visible, "inventory", kind),
            BTN_CONTAINER_RESTOCK | BTN_CONTAINER_TRANSFER => {
                if container.is_some() {
                    self.move_into(&player_all, &container_all, id == BTN_CONTAINER_RESTOCK, "inventory", kind);
                }
            }
            BTN_INVENTORY_RESTOCK | BTN_INVENTORY_TRANSFER => {
                if container.is_some() {
                    self.move_into(&container_all, &player_all, id == BTN_INVENTORY_RESTOCK, "inventory", kind);
                }
            }
            _ => {}
        }
    }

    fn on_middle_click(&mut self, slot: Option<u32>) {
        let Some(slot) = slot else { return };
        let inv = inventory::inventory();
        if inv.carried.is_some() {
            return;
        }
        let container = inventory::open_container();
        let kind = container.as_ref().map(|c| c.kind.clone());
        let mut by_slot: HashMap<u32, &InvStack> = inv.slots.iter().map(|s| (s.slot, s)).collect();
        if let Some(container) = &container {
            for stack in &container.slots {
                by_slot.insert(stack.slot, stack);
            }
        }
        let visible: Vec<&InvStack> = by_slot.values().copied().collect();
        let order = self.trigger_order();
        if slot >= 300 {
            self.sort_container(order, &by_slot, &visible, "inventory", kind);
        } else if slot <= 35 {
            if self.hotbar_group {
                let group = if slot <= 8 {
                    range_views(0..=8, &by_slot)
                } else {
                    range_views(9..=35, &by_slot)
                };
                self.sort_groups(&[group], order, &visible, "inventory", kind);
            } else {
                self.sort_player(order, &by_slot, &visible, "inventory", kind);
            }
        }
    }

    fn on_key(&mut self, key: &str) {
        // Keys fire only while playing; the open container is the
        // player's own inventory menu there.
        let inv = inventory::inventory();
        if inv.carried.is_some() {
            return;
        }
        let by_slot: HashMap<u32, &InvStack> = inv.slots.iter().map(|s| (s.slot, s)).collect();
        let visible: Vec<&InvStack> = by_slot.values().copied().collect();
        match key {
            SORT_KEY => {
                let order = self.trigger_order();
                self.sort_player(order, &by_slot, &visible, "playing", None);
            }
            FILL_KEY => {
                // Fill stacks: the hotbar topped up from the main
                // inventory — the one fill the playing field offers.
                let mut run = range_views(0..=8, &by_slot);
                run.extend(range_views(9..=35, &by_slot));
                let limit = |id: &str| stack_limit(id, &visible);
                let clicks = plan_fill(&run, 9, &limit)
                    .into_iter()
                    .map(|slot| (slot, ClickKind::Pickup))
                    .collect();
                self.queue(clicks, "playing", None);
            }
            // Restock and transfer need a second inventory; while
            // playing there is none — the buttons carry them.
            _ => {}
        }
    }

    /// Plays the pending plan out at the click speed, while its
    /// context still holds and the hand stays empty.
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
        let speed = SPEEDS[self.speed];
        let pending = self.pending.as_mut().expect("checked above");
        for _ in 0..speed {
            let Some((slot, kind)) = pending.clicks.pop_front() else { break };
            let _ = inventory_actions::click(Some(slot), 0, kind);
        }
        if pending.clicks.is_empty() {
            self.pending = None;
        }
    }

    /// Declares the buttons for the open screen, again only when
    /// the set changed.
    fn declare(&mut self) {
        let state = screen::state();
        let container_open = inventory::open_container().is_some();
        let mut buttons: Vec<ScreenButton> = Vec::new();
        if self.buttons_on && state.id == "inventory" {
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
                    id: BTN_CONTAINER_RESTOCK,
                    screen: "inventory".into(),
                    menu: None,
                    anchor: Anchor::ContainerHead,
                    label: "Restock".into(),
                    icon: None,
                });
                buttons.push(ScreenButton {
                    id: BTN_CONTAINER_TRANSFER,
                    screen: "inventory".into(),
                    menu: None,
                    anchor: Anchor::ContainerHead,
                    label: "Transfer".into(),
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
                id: BTN_INVENTORY_RESTOCK,
                screen: "inventory".into(),
                menu: None,
                anchor: Anchor::InventoryHead,
                label: "Restock".into(),
                icon: None,
            });
            buttons.push(ScreenButton {
                id: BTN_INVENTORY_TRANSFER,
                screen: "inventory".into(),
                menu: None,
                anchor: Anchor::InventoryHead,
                label: "Transfer".into(),
                icon: None,
            });
        }
        let spelling = buttons
            .iter()
            .map(|b| format!("{}:{}:{:?}", b.id, b.label, b.anchor))
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

impl Mod for ClientSort {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(SORT_KEY, "Sort inventory", "key.keyboard.unknown")?;
        input::register_key(RESTOCK_KEY, "Restock", "key.keyboard.unknown")?;
        input::register_key(TRANSFER_KEY, "Transfer", "key.keyboard.unknown")?;
        input::register_key(FILL_KEY, "Fill stacks", "key.keyboard.unknown")?;
        let state = ClientSort {
            order: read_index(ORDER_KEY, ORDERS.len(), ORDER_DEFAULT),
            speed: read_index(SPEED_KEY, SPEEDS.len(), SPEEDS.len() - 1),
            buttons_on: read_bool(BUTTONS_KEY, true),
            hotbar_group: read_bool(HOTBAR_GROUP_KEY, true),
            restock_type: read_bool(RESTOCK_TYPE_KEY, true),
            registry: Registry::default(),
            registry_tried: false,
            pending: None,
            declared: String::new(),
        };
        Ok((state, Registration::new().on_tick()))
    }

    fn tick(&mut self, _tick: u64) {
        if !self.registry_tried {
            self.registry = Registry::load();
            self.registry_tried = !self.registry.order.is_empty();
        }
        for event in screen::poll() {
            match event {
                ScreenEvent::Button(id) => self.on_button(id),
                ScreenEvent::MiddleClick(slot) => self.on_middle_click(slot),
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

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SET_ORDER => {
                self.order = (self.order + 1) % ORDERS.len();
                let _ = storage::set(ORDER_KEY, self.order.to_string().as_bytes());
            }
            SET_SPEED => {
                self.speed = (self.speed + 1) % SPEEDS.len();
                let _ = storage::set(SPEED_KEY, self.speed.to_string().as_bytes());
            }
            SET_BUTTONS => {
                self.buttons_on = !self.buttons_on;
                write_bool(BUTTONS_KEY, self.buttons_on);
            }
            SET_HOTBAR_GROUP => {
                self.hotbar_group = !self.hotbar_group;
                write_bool(HOTBAR_GROUP_KEY, self.hotbar_group);
            }
            SET_RESTOCK_TYPE => {
                self.restock_type = !self.restock_type;
                write_bool(RESTOCK_TYPE_KEY, self.restock_type);
            }
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(ClientSort);

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
            }),
        }
    }

    fn empty(slot: u32) -> SlotView {
        SlotView { slot, stack: None }
    }

    fn limit64(_: &str) -> u32 {
        64
    }

    fn by_id(s: &PStack) -> SortKey {
        SortKey {
            a: 0,
            b: 0,
            s: s.id.clone(),
            t: String::new(),
        }
    }

    #[test]
    fn merge_clicks_are_exact() {
        // Dirt 10 at 9 and dirt 20 at 11 merge with two clicks;
        // stone stays where it is.
        let run = vec![
            stack(9, "minecraft:dirt", 10),
            stack(10, "minecraft:stone", 5),
            stack(11, "minecraft:dirt", 20),
        ];
        assert_eq!(plan_sort(&run, &limit64, &by_id), vec![11, 9]);
    }

    #[test]
    fn permute_clicks_are_exact() {
        // Stone before dirt, ranked dirt first: take dirt, place,
        // put the stone back — three clicks.
        let run = vec![stack(9, "minecraft:stone", 5), stack(10, "minecraft:dirt", 10)];
        assert_eq!(plan_sort(&run, &limit64, &by_id), vec![10, 9, 10]);
    }

    #[test]
    fn full_stacks_sort_before_partials() {
        // A full stack and a partial of one item: the plan tops the
        // partial up from the full stack, ending full-first.
        let run = vec![stack(9, "minecraft:dirt", 10), stack(10, "minecraft:dirt", 64)];
        assert_eq!(plan_sort(&run, &limit64, &by_id), vec![10, 9, 10]);
    }

    #[test]
    fn an_ordered_run_plans_nothing() {
        let run = vec![stack(9, "minecraft:dirt", 10), empty(10)];
        assert_eq!(plan_sort(&run, &limit64, &by_id), Vec::<u32>::new());
    }

    #[test]
    fn different_components_never_merge() {
        let mut a = stack(9, "minecraft:dirt", 10);
        a.stack.as_mut().expect("stack").components = "[custom_name=x]".into();
        let run = vec![a, stack(10, "minecraft:dirt", 20)];
        // No merge click sequence (which would start by taking the
        // 20 into the 10); the two stacks only swap places, the
        // bigger stack first, as different items would.
        assert_eq!(plan_sort(&run, &limit64, &by_id), vec![10, 9, 10]);
    }

    #[test]
    fn fill_clicks_are_exact() {
        // Hotbar dirt 10 topped up from two main-inventory stacks.
        let run = vec![
            stack(0, "minecraft:dirt", 10),
            stack(9, "minecraft:dirt", 30),
            stack(10, "minecraft:dirt", 5),
            stack(11, "minecraft:stone", 5),
        ];
        assert_eq!(plan_fill(&run, 1, &limit64), vec![9, 0, 10, 0]);
    }

    #[test]
    fn restock_matches_types_or_identities() {
        let source = vec![
            stack(9, "minecraft:dirt", 10),
            stack(10, "minecraft:stone", 5),
            stack(11, "minecraft:dirt", 3),
        ];
        let dest = vec![stack(300, "minecraft:dirt", 1)];
        assert_eq!(plan_restock(&source, &dest, true), vec![9, 11]);
        // By exact identity, a named dirt on the dest side matches
        // no plain dirt on the source side.
        let mut named = stack(300, "minecraft:dirt", 1);
        named.stack.as_mut().expect("stack").components = "[custom_name=x]".into();
        assert_eq!(plan_restock(&source, &[named], false), Vec::<u32>::new());
        // And plain dirt matches plain dirt.
        assert_eq!(plan_restock(&source, &dest, false), vec![9, 11]);
    }

    #[test]
    fn transfer_takes_every_non_empty_slot() {
        let source = vec![stack(9, "minecraft:dirt", 10), empty(10), stack(11, "minecraft:stone", 1)];
        assert_eq!(plan_transfer(&source), vec![9, 11]);
    }

    #[test]
    fn stack_limits_follow_the_table() {
        let inv = |id: &str, count: u32, max_damage: u32| InvStack {
            slot: 0,
            id: id.into(),
            count,
            damage: 0,
            max_damage,
            components: String::new(),
        };
        let snow = inv("minecraft:snowball", 16, 0);
        assert_eq!(stack_limit("minecraft:snowball", &[&snow]), 16);
        let pick = inv("minecraft:iron_pickaxe", 1, 250);
        assert_eq!(stack_limit("minecraft:iron_pickaxe", &[&pick]), 1);
        let dirt = inv("minecraft:dirt", 64, 0);
        assert_eq!(stack_limit("minecraft:dirt", &[&dirt]), 64);
        let honey = inv("minecraft:honey_bottle", 4, 0);
        assert_eq!(stack_limit("minecraft:honey_bottle", &[&honey]), 4);
        let sign = inv("minecraft:oak_sign", 16, 0);
        assert_eq!(stack_limit("minecraft:oak_sign", &[&sign]), 16);
    }

    #[test]
    fn quantity_order_outranks_registry_order() {
        let mut registry = Registry::default();
        registry.order.insert("minecraft:stone".into(), 0);
        registry.order.insert("minecraft:dirt".into(), 1);
        let mut totals = HashMap::new();
        totals.insert(("minecraft:dirt".to_string(), String::new()), 100);
        totals.insert(("minecraft:stone".to_string(), String::new()), 5);
        let dirt = PStack {
            id: "minecraft:dirt".into(),
            components: String::new(),
            count: 64,
        };
        let stone = PStack {
            id: "minecraft:stone".into(),
            components: String::new(),
            count: 5,
        };
        // Default order: stone (registry 0) first. Quantity: the
        // 100 dirt outrank the 5 stone.
        assert!(sort_key(ORDER_DEFAULT, &registry, &totals, &stone) < sort_key(ORDER_DEFAULT, &registry, &totals, &dirt));
        assert!(sort_key(ORDER_QUANTITY, &registry, &totals, &dirt) < sort_key(ORDER_QUANTITY, &registry, &totals, &stone));
    }
}

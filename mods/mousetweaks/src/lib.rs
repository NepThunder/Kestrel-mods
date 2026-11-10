//! Mouse Tweaks for Kestrel: right-button spread, left-button
//! gather and shift quick-move drags, and the wheel tweak. A
//! behaviour port of Mouse Tweaks by YaLTeR (Ivan Molodetskikh),
//! BSD-3-Clause, on the only observation surface a sandbox mod
//! gets — the API 22 screen read (the hovered slot), the input
//! mappings for the buttons, and the API 24 wheel events — with
//! every gesture answered in inventory-actions verbs: the spread
//! as one-slot right drags, the gather as the game's own
//! two-pickup sequence, the wheel as its three-verb push and pull
//! (pickup, one-slot right drag, put the rest back), computed on
//! a simulation of the open menu, as the API 24 design describes.
//!
//! Fidelity, against the registry entry: the three tweaks match
//! (RMB drag drops one item per slot passed; LMB drag gathers
//! the same items, with Shift quick-moving every slot passed;
//! LMB with an empty hand and Shift quick-moves every slot
//! passed; the wheel moves items one at a time between the
//! inventory and the container, crafting results included), as
//! do the settings the registry names — the three tweak toggles,
//! scroll direction (normal, inverted, position-aware), search
//! order and item scaling — in Options > Mods > Mouse Tweaks,
//! and it works on every container screen and the player's slots
//! (not the creative inventory, whose slots run on a different
//! protocol). While it is on, it takes the place of the game's
//! built-in Mouse Tweaks extra, which the player turns off in
//! Extras — two wheel tweaks both answering one turn would move
//! two items. Three frictions, all surface limits, stated
//! plainly in the manifest: no drag preview; hovered slots are
//! read once a tick, so a fast flick can skip a slot (and a slot
//! re-entered within one hold is served once); and the game's
//! own quick-craft drag still runs underneath — a sandbox mod
//! cannot consume the player's press — so on release the game
//! also distributes what is left in the hand over the slots it
//! tracked, which can add an item to a slot the tweak served.

use std::collections::{HashMap, HashSet};

use horizon_mod_sdk::{
    DragButton, Mod, Registration, input,
    inventory::{self, InvStack},
    inventory_actions, screen, storage,
    ui::{self, ButtonWidget, Widget},
};

#[cfg(test)]
const API_LEVEL: u32 = 24;

// Settings buttons (Options > Mods > Mouse Tweaks).
const SET_RMB: u32 = 101;
const SET_LMB: u32 = 102;
const SET_WHEEL: u32 = 103;
const SET_DIRECTION: u32 = 104;
const SET_SEARCH: u32 = 105;
const SET_SCALING: u32 = 106;

const RMB_KEY: &str = "rmb";
const LMB_KEY: &str = "lmb";
const WHEEL_KEY: &str = "wheel";
const DIRECTION_KEY: &str = "direction";
const SEARCH_KEY: &str = "search";
const SCALING_KEY: &str = "scaling";

const DIRECTIONS: [&str; 3] = ["Normal", "Inverted", "Position-aware"];
const DIR_NORMAL: usize = 0;
const DIR_INVERTED: usize = 1;
const DIR_POSITION: usize = 2;

const SEARCHES: [&str; 2] = ["First", "Last"];

/// Result slots: the crafting result and every station's result
/// (the merchant's included). Nothing is ever placed into them.
const RESULT_SLOTS: [u32; 2] = [200, 650];

/// Wheel verbs queued per tick at most — the district's ceiling,
/// with headroom.
const WHEEL_VERB_BUDGET: usize = 30;

// ---------------------------------------------------------------------------
// The wheel planner: a simulation of the open menu, and the
// game's own push and pull sequences over it. Pure, so the
// sequences are unit-tested below.
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

/// One planned wheel verb.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Verb {
    Click(u32),
    DragRight(u32),
    DragLeft(u32),
}

struct WheelSim {
    slots: Vec<SlotView>,
    hand: Option<PStack>,
    /// Stack limits for the ids on view, from the visible stacks.
    limits: HashMap<String, u32>,
    container_open: bool,
}

impl WheelSim {
    fn limit(&self, id: &str) -> u32 {
        self.limits
            .get(id)
            .copied()
            .unwrap_or_else(|| stack_limit_base(id.rsplit(':').next().unwrap_or(id)))
    }

    fn pos(&self, slot: u32) -> Option<usize> {
        self.slots.iter().position(|v| v.slot == slot)
    }

    fn stack_at(&self, slot: u32) -> Option<&PStack> {
        self.pos(slot).and_then(|i| self.slots[i].stack.as_ref())
    }

    /// A whole-stack pickup click, vanilla semantics.
    fn click_pickup(&mut self, slot: u32, out: &mut Vec<Verb>) {
        let Some(i) = self.pos(slot) else { return };
        out.push(Verb::Click(slot));
        let max = self.slots[i].stack.as_ref().map(|s| self.limit(&s.id));
        match (&mut self.hand, &mut self.slots[i].stack) {
            (None, stack @ Some(_)) => self.hand = stack.take(),
            (hand @ Some(_), None) => self.slots[i].stack = hand.take(),
            (Some(hand), Some(stack)) => {
                if hand.id == stack.id && hand.components == stack.components {
                    let max = max.unwrap_or(64);
                    if stack.count < max {
                        let moved = (max - stack.count).min(hand.count);
                        stack.count += moved;
                        hand.count -= moved;
                        if hand.count == 0 {
                            self.hand = None;
                        }
                    } else {
                        std::mem::swap(hand, stack);
                    }
                } else {
                    std::mem::swap(hand, stack);
                }
            }
            (None, None) => {}
        }
    }

    /// A one-slot drag: exactly one item with the right button,
    /// all that fits with the left. No verb when nothing can move.
    fn drag_one(&mut self, slot: u32, right: bool, out: &mut Vec<Verb>) {
        let Some(i) = self.pos(slot) else { return };
        let Some(hand) = &self.hand else { return };
        let moved = match &self.slots[i].stack {
            None => {
                if right {
                    1
                } else {
                    hand.count.min(self.limit(&hand.id))
                }
            }
            Some(stack) => {
                if stack.id == hand.id && stack.components == hand.components {
                    let room = self.limit(&stack.id).saturating_sub(stack.count);
                    if right {
                        room.min(1)
                    } else {
                        room.min(hand.count)
                    }
                } else {
                    0
                }
            }
        };
        if moved == 0 {
            return;
        }
        out.push(if right { Verb::DragRight(slot) } else { Verb::DragLeft(slot) });
        let mut hand = self.hand.take().expect("checked above");
        hand.count -= moved;
        match &mut self.slots[i].stack {
            Some(stack) => stack.count += moved,
            None => {
                let mut placed = hand.clone();
                placed.count = moved;
                self.slots[i].stack = Some(placed);
            }
        }
        if hand.count > 0 {
            self.hand = Some(hand);
        }
    }

    /// The side a quick move out of `from` lands on, in landing
    /// order: the container's items land in the main inventory,
    /// then the hotbar; the player's items land in the container
    /// (its inputs, never its result), or in the other player
    /// range when no container is open.
    fn landing_side(&self, from: u32) -> Vec<u32> {
        if from >= 300 || from == 200 || (100..109).contains(&from) {
            // The container's items, the crafting result and the
            // grid land in the main inventory, then the hotbar.
            return (9..=35).chain(0..=8).collect();
        }
        if self.container_open {
            let mut side: Vec<u32> = self
                .slots
                .iter()
                .map(|v| v.slot)
                .filter(|s| (300..500).contains(s))
                .collect();
            side.sort_unstable();
            let mut stations: Vec<u32> = self
                .slots
                .iter()
                .map(|v| v.slot)
                .filter(|s| (600..700).contains(s) && *s != 650)
                .collect();
            stations.sort_unstable();
            side.extend(stations);
            return side;
        }
        if from < 9 {
            (9..=35).collect()
        } else {
            (0..=8).collect()
        }
    }

    /// Quick-move landing targets for `stack` leaving `from`:
    /// slots holding the same item with room, in side order, then
    /// the side's empty slots, in side order.
    fn landing_targets(&self, from: u32, stack: &PStack) -> Vec<u32> {
        let side = self.landing_side(from);
        let mut targets: Vec<u32> = side
            .iter()
            .copied()
            .filter(|s| {
                self.stack_at(*s).is_some_and(|t| {
                    t.identity() == stack.identity() && t.count < self.limit(&t.id)
                })
            })
            .collect();
        targets.extend(
            side.iter()
                .copied()
                .filter(|s| self.pos(*s).is_some_and(|i| self.slots[i].stack.is_none())),
        );
        targets
    }

    /// Push: move one of the slot's items to where a quick move
    /// would send it — the game's own sequence (pickup, one-slot
    /// right drag into the landing slot, the rest back). A result
    /// slot's whole output is taken and put down the landing
    /// order instead.
    fn push(&mut self, slot: u32, out: &mut Vec<Verb>) {
        let Some(stack) = self.stack_at(slot).cloned() else {
            return;
        };
        if RESULT_SLOTS.contains(&slot) {
            let targets = self.landing_targets(slot, &stack);
            if targets.is_empty() {
                return;
            }
            self.click_pickup(slot, out);
            for target in targets {
                if self.hand.is_none() {
                    break;
                }
                self.drag_one(target, false, out);
            }
            return;
        }
        let Some(landing) = self.landing_targets(slot, &stack).first().copied() else {
            return;
        };
        let count = stack.count;
        self.click_pickup(slot, out);
        self.drag_one(landing, true, out);
        if count > 1 {
            self.drag_one(slot, false, out);
        }
    }

    /// The slots a pull into `target` searches, in the game's own
    /// order: for a container slot, the hotbar from the right,
    /// then the main inventory from the right; for a player slot,
    /// the container's slots in order, or the other player range
    /// when no container is open.
    fn sources(&self, target: u32) -> Vec<u32> {
        if target >= 300 {
            (0..=8).rev().chain((9..=35).rev()).collect()
        } else if self.container_open {
            let mut side: Vec<u32> = self.slots.iter().map(|v| v.slot).filter(|s| *s >= 300).collect();
            side.sort_unstable();
            side
        } else if target < 9 {
            (9..=35).collect()
        } else if target < 100 {
            (0..=8).collect()
        } else {
            (0..=35).collect()
        }
    }

    /// Pull: bring one of the hovered slot's item from the other
    /// side into it — the game's own sequence again (pickup the
    /// found stack, one-slot right drag into the target, the rest
    /// back). The search order setting scans the source list
    /// forwards or backwards.
    fn pull(&mut self, target: u32, search_last: bool, out: &mut Vec<Verb>) {
        if RESULT_SLOTS.contains(&target) {
            return;
        }
        let Some(stack) = self.stack_at(target).cloned() else {
            return;
        };
        if stack.count >= self.limit(&stack.id) {
            return;
        }
        let mut sources = self.sources(target);
        if search_last {
            sources.reverse();
        }
        let source = sources.iter().copied().find(|s| {
            *s != target
                && !RESULT_SLOTS.contains(s)
                && self
                    .stack_at(*s)
                    .is_some_and(|t| t.identity() == stack.identity())
        });
        let Some(source) = source else { return };
        let count = self.stack_at(source).map_or(0, |s| s.count);
        self.click_pickup(source, out);
        self.drag_one(target, true, out);
        if count > 1 {
            self.drag_one(source, false, out);
        }
    }
}

/// Whether a wheel turn over `slot` pushes (moves one of the
/// slot's items to the other side) or pulls, under the scroll
/// direction setting. Positive steps are the turn the game's own
/// wheel tweak calls down.
fn wheel_pushes(direction: usize, slot: u32, steps: i32) -> bool {
    match direction {
        DIR_INVERTED => steps < 0,
        DIR_POSITION => {
            if slot >= 300 {
                steps > 0
            } else {
                steps < 0
            }
        }
        _ => steps > 0,
    }
}

// ---------------------------------------------------------------------------
// The mod.
// ---------------------------------------------------------------------------

struct MouseTweaks {
    rmb_tweak: bool,
    lmb_tweak: bool,
    wheel_tweak: bool,
    direction: usize,
    search: usize,
    scaling: bool,
    // Gesture state, reset whenever the button comes up.
    rmb_carried: Option<(String, String)>,
    rmb_visited: HashSet<u32>,
    lmb_carried: Option<(String, String)>,
    lmb_visited: HashSet<u32>,
    empty_visited: HashSet<u32>,
}

impl MouseTweaks {
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: SET_RMB,
                label: format!("RMB tweak: {}", if self.rmb_tweak { "on" } else { "off" }),
            }),
            Widget::Button(ButtonWidget {
                id: SET_LMB,
                label: format!("LMB tweak: {}", if self.lmb_tweak { "on" } else { "off" }),
            }),
            Widget::Button(ButtonWidget {
                id: SET_WHEEL,
                label: format!("Wheel tweak: {}", if self.wheel_tweak { "on" } else { "off" }),
            }),
            Widget::Button(ButtonWidget {
                id: SET_DIRECTION,
                label: format!("Scroll direction: {}", DIRECTIONS[self.direction]),
            }),
            Widget::Button(ButtonWidget {
                id: SET_SEARCH,
                label: format!("Search order: {}", SEARCHES[self.search]),
            }),
            Widget::Button(ButtonWidget {
                id: SET_SCALING,
                label: format!("Item scaling: {}", if self.scaling { "on" } else { "off" }),
            }),
        ]);
    }

    /// The RMB spread: while the right button is held with a
    /// stack, one item into each slot newly passed over.
    fn rmb_drag(&mut self, hovered: Option<u32>, carried: Option<&PStack>) {
        let (Some(carried), Some(slot)) = (carried, hovered) else {
            return;
        };
        if self.rmb_carried.as_ref() != Some(&carried.identity()) {
            self.rmb_carried = Some(carried.identity());
            self.rmb_visited.clear();
        }
        if RESULT_SLOTS.contains(&slot) || self.rmb_visited.contains(&slot) {
            return;
        }
        self.rmb_visited.insert(slot);
        let _ = inventory_actions::drag(DragButton::Right, &[slot]);
    }

    /// The LMB gather — and, with Shift, the quick-move drags,
    /// with a stack in hand or without.
    fn lmb_drag(
        &mut self,
        hovered: Option<u32>,
        carried: Option<&PStack>,
        shift: bool,
        slots: &HashMap<u32, &InvStack>,
        limit: &dyn Fn(&str) -> u32,
    ) {
        let Some(slot) = hovered else { return };
        match carried {
            Some(carried) => {
                if self.lmb_carried.as_ref() != Some(&carried.identity()) {
                    self.lmb_carried = Some(carried.identity());
                    self.lmb_visited.clear();
                }
                if self.lmb_visited.contains(&slot) {
                    return;
                }
                let Some(target) = slots.get(&slot) else { return };
                if shift {
                    self.lmb_visited.insert(slot);
                    let _ = inventory_actions::click(Some(slot), 0, horizon_mod_sdk::ClickKind::QuickMove);
                } else if target.id == carried.id
                    && target.components == carried.components
                    && carried.count + target.count <= limit(&carried.id)
                {
                    self.lmb_visited.insert(slot);
                    // The game's own gather: put the hand stack
                    // in, take the combined stack back.
                    let _ = inventory_actions::click(Some(slot), 0, horizon_mod_sdk::ClickKind::Pickup);
                    let _ = inventory_actions::click(Some(slot), 0, horizon_mod_sdk::ClickKind::Pickup);
                }
            }
            None => {
                if !shift || self.empty_visited.contains(&slot) {
                    return;
                }
                if slots.contains_key(&slot) {
                    self.empty_visited.insert(slot);
                    let _ = inventory_actions::click(Some(slot), 0, horizon_mod_sdk::ClickKind::QuickMove);
                }
            }
        }
    }

    /// The wheel tweak over this tick's turns.
    fn wheel(&mut self, events: Vec<(u32, i32)>, inv: &inventory::PlayerInventory, container: &Option<inventory::Container>) {
        if !self.wheel_tweak || inv.carried.is_some() || events.is_empty() {
            return;
        }
        let mut by_slot: HashMap<u32, InvStack> = inv.slots.iter().map(|s| (s.slot, s.clone())).collect();
        if let Some(container) = container {
            for stack in &container.slots {
                by_slot.insert(stack.slot, stack.clone());
            }
        }
        let visible: Vec<&InvStack> = by_slot.values().collect();
        let mut slots: Vec<SlotView> = by_slot
            .values()
            .map(|s| SlotView {
                slot: s.slot,
                stack: Some(view_of(s)),
            })
            .collect();
        // The runs the wheel can land in: synthesize the player
        // side's empties (landing needs them) and the container's
        // reported range.
        let mut present: HashSet<u32> = slots.iter().map(|v| v.slot).collect();
        for slot in 0..=35 {
            if !present.contains(&slot) {
                slots.push(SlotView { slot, stack: None });
                present.insert(slot);
            }
        }
        if let Some(max) = present.iter().copied().filter(|s| (300..500).contains(s)).max() {
            for slot in 300..=max {
                if !present.contains(&slot) {
                    slots.push(SlotView { slot, stack: None });
                    present.insert(slot);
                }
            }
        }
        slots.sort_by_key(|v| v.slot);
        let limits: HashMap<String, u32> = visible
            .iter()
            .map(|s| s.id.clone())
            .collect::<HashSet<String>>()
            .into_iter()
            .map(|id| {
                let limit = stack_limit(&id, &visible);
                (id, limit)
            })
            .collect();
        let mut sim = WheelSim {
            slots,
            hand: None,
            limits,
            container_open: container.is_some(),
        };
        let mut verbs: Vec<Verb> = Vec::new();
        for (slot, steps) in events {
            if steps == 0 {
                continue;
            }
            let times = if self.scaling {
                steps.unsigned_abs()
            } else {
                1
            };
            for _ in 0..times {
                if verbs.len() > WHEEL_VERB_BUDGET {
                    break;
                }
                if wheel_pushes(self.direction, slot, steps) {
                    sim.push(slot, &mut verbs);
                } else {
                    sim.pull(slot, self.search == 1, &mut verbs);
                }
            }
            if verbs.len() > WHEEL_VERB_BUDGET {
                break;
            }
        }
        for verb in verbs {
            match verb {
                Verb::Click(slot) => {
                    let _ = inventory_actions::click(Some(slot), 0, horizon_mod_sdk::ClickKind::Pickup);
                }
                Verb::DragRight(slot) => {
                    let _ = inventory_actions::drag(DragButton::Right, &[slot]);
                }
                Verb::DragLeft(slot) => {
                    let _ = inventory_actions::drag(DragButton::Left, &[slot]);
                }
            }
        }
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

impl Mod for MouseTweaks {
    fn init() -> Result<(Self, Registration), String> {
        let state = MouseTweaks {
            rmb_tweak: read_bool(RMB_KEY, true),
            lmb_tweak: read_bool(LMB_KEY, true),
            wheel_tweak: read_bool(WHEEL_KEY, true),
            direction: read_index(DIRECTION_KEY, DIRECTIONS.len(), DIR_NORMAL),
            search: read_index(SEARCH_KEY, SEARCHES.len(), 0),
            scaling: read_bool(SCALING_KEY, true),
            rmb_carried: None,
            rmb_visited: HashSet::new(),
            lmb_carried: None,
            lmb_visited: HashSet::new(),
            empty_visited: HashSet::new(),
        };
        Ok((state, Registration::new().on_tick()))
    }

    fn tick(&mut self, _tick: u64) {
        let wheel_events: Vec<(u32, i32)> = inventory_actions::poll_wheel()
            .into_iter()
            .filter_map(|e| e.slot.map(|slot| (slot, e.steps)))
            .collect();
        let state = screen::state();
        if state.id != "inventory" {
            self.rmb_carried = None;
            self.rmb_visited.clear();
            self.lmb_carried = None;
            self.lmb_visited.clear();
            self.empty_visited.clear();
            return;
        }
        let inv = inventory::inventory();
        let container = inventory::open_container();
        let carried = inv.carried.as_ref().map(view_of);
        let lmb = input::mapping("key.attack").is_some_and(|m| m.down);
        let rmb = input::mapping("key.use").is_some_and(|m| m.down);
        let shift = input::mapping("key.sneak").is_some_and(|m| m.down);

        if self.rmb_tweak && rmb {
            self.rmb_drag(state.hovered_slot, carried.as_ref());
        } else {
            self.rmb_carried = None;
            self.rmb_visited.clear();
        }

        if self.lmb_tweak && lmb {
            let mut by_slot: HashMap<u32, &InvStack> = inv.slots.iter().map(|s| (s.slot, s)).collect();
            if let Some(container) = &container {
                for stack in &container.slots {
                    by_slot.insert(stack.slot, stack);
                }
            }
            let visible: Vec<&InvStack> = by_slot.values().copied().collect();
            let limit = |id: &str| stack_limit(id, &visible);
            self.lmb_drag(state.hovered_slot, carried.as_ref(), shift, &by_slot, &limit);
        } else {
            self.lmb_carried = None;
            self.lmb_visited.clear();
            self.empty_visited.clear();
        }

        self.wheel(wheel_events, &inv, &container);
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SET_RMB => {
                self.rmb_tweak = !self.rmb_tweak;
                write_bool(RMB_KEY, self.rmb_tweak);
            }
            SET_LMB => {
                self.lmb_tweak = !self.lmb_tweak;
                write_bool(LMB_KEY, self.lmb_tweak);
            }
            SET_WHEEL => {
                self.wheel_tweak = !self.wheel_tweak;
                write_bool(WHEEL_KEY, self.wheel_tweak);
            }
            SET_DIRECTION => {
                self.direction = (self.direction + 1) % DIRECTIONS.len();
                let _ = storage::set(DIRECTION_KEY, self.direction.to_string().as_bytes());
            }
            SET_SEARCH => {
                self.search = (self.search + 1) % SEARCHES.len();
                let _ = storage::set(SEARCH_KEY, self.search.to_string().as_bytes());
            }
            SET_SCALING => {
                self.scaling = !self.scaling;
                write_bool(SCALING_KEY, self.scaling);
            }
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(MouseTweaks);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_24() {
        assert_eq!(API_LEVEL, 24);
    }

    fn view(slot: u32, id: &str, count: u32) -> SlotView {
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

    fn make_sim(slots: Vec<SlotView>, container_open: bool) -> WheelSim {
        WheelSim {
            slots,
            hand: None,
            limits: HashMap::new(),
            container_open,
        }
    }

    #[test]
    fn push_is_the_games_three_verbs() {
        // One dirt out of the chest slot into the main inventory's
        // matching stack: pickup, one in, the rest back.
        let mut sim = make_sim(
            vec![view(300, "minecraft:dirt", 10), view(9, "minecraft:dirt", 30)],
            true,
        );
        let mut out = Vec::new();
        sim.push(300, &mut out);
        assert_eq!(
            out,
            vec![Verb::Click(300), Verb::DragRight(9), Verb::DragLeft(300)]
        );
    }

    #[test]
    fn push_of_a_single_item_needs_no_put_back() {
        let mut sim = make_sim(
            vec![view(300, "minecraft:dirt", 1), view(9, "minecraft:dirt", 30)],
            true,
        );
        let mut out = Vec::new();
        sim.push(300, &mut out);
        assert_eq!(out, vec![Verb::Click(300), Verb::DragRight(9)]);
    }

    #[test]
    fn push_without_a_landing_does_nothing() {
        // The player side is full of stone: nowhere for dirt to go.
        let mut slots = vec![view(300, "minecraft:dirt", 10)];
        for slot in (0..=35).filter(|s| *s != 300) {
            slots.push(view(slot, "minecraft:stone", 64));
        }
        let mut sim = make_sim(slots, true);
        let mut out = Vec::new();
        sim.push(300, &mut out);
        assert_eq!(out, Vec::<Verb>::new());
    }

    #[test]
    fn pull_searches_the_other_side_in_order() {
        // Into a container slot: the hotbar from the right first —
        // slot 7 before slot 3 before the main inventory's 20.
        let mut sim = make_sim(
            vec![
                view(300, "minecraft:dirt", 10),
                view(3, "minecraft:dirt", 5),
                view(7, "minecraft:dirt", 5),
                view(20, "minecraft:dirt", 5),
            ],
            true,
        );
        let mut out = Vec::new();
        sim.pull(300, false, &mut out);
        assert_eq!(
            out,
            vec![Verb::Click(7), Verb::DragRight(300), Verb::DragLeft(7)]
        );
        // Last: the scan runs backwards, the main inventory first.
        let mut sim = make_sim(
            vec![
                view(300, "minecraft:dirt", 10),
                view(3, "minecraft:dirt", 5),
                view(7, "minecraft:dirt", 5),
                view(20, "minecraft:dirt", 5),
            ],
            true,
        );
        let mut out = Vec::new();
        sim.pull(300, true, &mut out);
        assert_eq!(
            out,
            vec![Verb::Click(20), Verb::DragRight(300), Verb::DragLeft(20)]
        );
    }

    #[test]
    fn pull_into_a_full_slot_does_nothing() {
        let mut sim = make_sim(
            vec![view(300, "minecraft:dirt", 64), view(9, "minecraft:dirt", 5)],
            true,
        );
        let mut out = Vec::new();
        sim.pull(300, false, &mut out);
        assert_eq!(out, Vec::<Verb>::new());
    }

    #[test]
    fn a_result_push_takes_the_whole_output() {
        // The crafting result: pickup, then put down the landing
        // order — the matching stack first.
        let mut sim = make_sim(
            vec![
                view(200, "minecraft:bread", 4),
                view(9, "minecraft:bread", 60),
                empty(10),
            ],
            false,
        );
        let mut out = Vec::new();
        sim.push(200, &mut out);
        assert_eq!(out, vec![Verb::Click(200), Verb::DragLeft(9)]);
        assert_eq!(sim.stack_at(9).map(|s| s.count), Some(64));
        assert!(sim.hand.is_none());
    }

    #[test]
    fn directions_resolve_as_named() {
        assert!(wheel_pushes(DIR_NORMAL, 300, 1));
        assert!(!wheel_pushes(DIR_NORMAL, 300, -1));
        assert!(!wheel_pushes(DIR_INVERTED, 300, 1));
        assert!(wheel_pushes(DIR_INVERTED, 300, -1));
        // Position-aware: down over the container pushes down into
        // the inventory; down over the inventory pulls from the
        // container above it.
        assert!(wheel_pushes(DIR_POSITION, 300, 1));
        assert!(!wheel_pushes(DIR_POSITION, 9, 1));
        assert!(wheel_pushes(DIR_POSITION, 9, -1));
    }

    #[test]
    fn landing_prefers_the_matching_stack_then_an_empty() {
        let sim = make_sim(
            vec![
                view(300, "minecraft:dirt", 10),
                view(9, "minecraft:stone", 5),
                view(10, "minecraft:dirt", 30),
                empty(11),
            ],
            true,
        );
        let dirt = PStack {
            id: "minecraft:dirt".into(),
            components: String::new(),
            count: 10,
        };
        assert_eq!(sim.landing_targets(300, &dirt), vec![10, 11]);
    }
}

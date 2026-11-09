//! Item New for Kestrel: items you just got glow in your hotbar
//! and inventory until you look at them.
//!
//! Behaviour reimplemented from Item New by d3grando, MIT,
//! <https://github.com/d3grando/itemnew> (itself inspired by
//! Item Highlighter): this file shares no code with the
//! original — the original is a native Horizon mod, this is a
//! fresh implementation against the Kestrel SDK. This port's
//! home is <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/itemnew>.
//!
//! Fidelity, against the original's registry description:
//! items just got — picked up, crafted, taken from a chest,
//! given — get a soft glow in the hotbar and in inventory and
//! container screens, until the player points at them or closes
//! the screen. The styles are the original's three (glow, glow
//! with a star, by rarity colour); which screens show marks and
//! when a mark clears are settings; `/itemnew` shows or clears
//! the marks.
//!
//! Finding the new items is the mod's own work, as the API 23
//! design note has it: each tick the inventory snapshots
//! (`inventory`, API 9) are diffed against the last. A slot
//! becomes new when its stack's identity (item + components)
//! grew in total across the inventory it sits in — the player's
//! own, or the open container's — so a pickup, a craft and a
//! take from a chest mark, while dragging stacks around (gains
//! matched by losses) marks nothing. Marks are
//! `item-model.set-decorations` (API 23): a fill (and star) on
//! the new slots; clearing reads the hovered slot and the
//! screen id from `screen.state` (API 22). The first snapshot
//! in a world only sets the baseline — joining a world marks
//! nothing.
//!
//! Two differences the surface forces, documented rather than
//! hidden: a new item moved to another slot loses its mark
//! (the diff cannot follow a stack across slots), and the
//! by-rarity style declares host-matched stack marks keyed on
//! `minecraft:rarity` for the newly gained item ids — the host
//! colours them by their true rarity (which lives in the
//! item's defaults, where no SDK read can see it), but every
//! stack of such an item colours, not only the new one.

use horizon_mod_sdk::{
    ComponentMatch, Decoration, DecorationPlace, DecorationTarget, Mod, ModelMatch, Registration,
    inventory::{self, InvStack},
    item_model, screen,
    storage,
    ui::{self, ButtonWidget, Widget},
};
use std::collections::{BTreeMap, BTreeSet};

/// The newest API level this mod needs, asserted in the tests
/// below: `item-model.set-decorations` is level 23
/// (`crates/horizon-mods/src/api_level.rs`); `screen.state` is
/// 22 and `inventory` is 9.
#[cfg(test)]
const API_LEVEL: u32 = 23;

/// The three styles, as stored indices.
const STYLE_GLOW: usize = 0;
const STYLE_STAR: usize = 1;
const STYLE_RARITY: usize = 2;
const STYLES: usize = 3;

/// Where marks show, as stored indices.
const SHOW_BOTH: usize = 0;
const SHOW_HOTBAR: usize = 1;
const SHOW_SCREENS: usize = 2;
const SHOWS: usize = 3;

const STYLE_KEY: &str = "style";
const SHOW_KEY: &str = "show";
const HOVER_KEY: &str = "clear_hover";
const CLOSE_KEY: &str = "clear_close";

const STYLE_BUTTON: u32 = 0;
const SHOW_BUTTON: u32 = 1;
const HOVER_BUTTON: u32 = 2;
const CLOSE_BUTTON: u32 = 3;

/// The glow's colour: a soft warm white. The registry says
/// "a soft glow" and names no colour; this is the port's.
const GLOW: u32 = 0xFFF3B0;

/// The vanilla rarity colours, rarities in order: the fill of
/// the by-rarity style, matched host-side per stack.
const RARITIES: [(&str, u32); 4] = [
    ("common", 0xFFFFFF),
    ("uncommon", 0xFFFF55),
    ("rare", 0x55FFFF),
    ("epic", 0xFF55FF),
];

/// The surface's cap on one mod's decorations.
const MAX_DECORATIONS: usize = 256;

/// One slot's stack as the diff sees it: identity is the item
/// and its components (beyond the item's defaults); the count
/// rides along.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Seen {
    id: String,
    components: String,
    count: u32,
}

impl Seen {
    fn of(stack: &InvStack) -> Seen {
        Seen {
            id: stack.id.clone(),
            components: stack.components.clone(),
            count: stack.count,
        }
    }
    fn identity(&self) -> (&str, &str) {
        (&self.id, &self.components)
    }
}

/// Diffs one inventory (the player's, or the open container's)
/// tick by tick. The first snapshot after priming is a baseline
/// and marks nothing: joining a world, or opening a container,
/// makes nothing new by itself.
#[derive(Default)]
struct Tracker {
    prev: BTreeMap<u32, Seen>,
    primed: bool,
}

impl Tracker {
    fn reprime(&mut self) {
        self.prev.clear();
        self.primed = false;
    }

    /// Folds one snapshot in and returns the slots that became
    /// new: a slot whose stack grew, in an inventory where that
    /// stack's identity grew in total — gains matched by losses
    /// (a move, a swap, a split) mark nothing.
    fn observe(&mut self, stacks: &[InvStack]) -> Vec<u32> {
        let cur: BTreeMap<u32, Seen> = stacks
            .iter()
            .filter(|s| s.count > 0)
            .map(|s| (s.slot, Seen::of(s)))
            .collect();
        if !self.primed {
            self.prev = cur;
            self.primed = true;
            return Vec::new();
        }
        let before = totals(&self.prev);
        let now = totals(&cur);
        let grew = |seen: &Seen| {
            now.get(&seen.identity()).copied().unwrap_or(0)
                > before.get(&seen.identity()).copied().unwrap_or(0)
        };
        let mut marked = Vec::new();
        for (slot, seen) in &cur {
            let gained = match self.prev.get(slot) {
                Some(old) if old.identity() == seen.identity() => seen.count > old.count,
                _ => true,
            };
            if gained && grew(seen) {
                marked.push(*slot);
            }
        }
        self.prev = cur;
        marked
    }
}

/// Identity totals over a snapshot: how much of each identity
/// the inventory holds in all.
fn totals(map: &BTreeMap<u32, Seen>) -> BTreeMap<(&str, &str), u32> {
    let mut out: BTreeMap<(&str, &str), u32> = BTreeMap::new();
    for seen in map.values() {
        *out.entry(seen.identity()).or_default() += seen.count;
    }
    out
}

/// The places a slot's mark shows at, under the Show On
/// setting: hotbar slots live in both places (the hotbar, and
/// the screens that draw it), every other slot only on screens.
fn places_for(slot: u32, show: usize) -> Vec<DecorationPlace> {
    let mut places = Vec::new();
    if slot <= 8 && (show == SHOW_BOTH || show == SHOW_HOTBAR) {
        places.push(DecorationPlace::Hotbar);
    }
    if show == SHOW_BOTH || show == SHOW_SCREENS {
        places.push(DecorationPlace::Screens);
    }
    if slot > 8 && show == SHOW_HOTBAR {
        return Vec::new();
    }
    places
}

/// The decorations for the new set, in declaration order: one
/// slot mark each in the glow styles; in the rarity style, one
/// host-matched stack mark per rarity for each newly gained
/// item, coloured as vanilla colours rarities.
fn build_decorations(
    new: &BTreeSet<u32>,
    stacks: &BTreeMap<u32, &InvStack>,
    style: usize,
    show: usize,
) -> Vec<Decoration> {
    let mut out = Vec::new();
    if style == STYLE_RARITY {
        let mut by_item: BTreeMap<&str, Vec<DecorationPlace>> = BTreeMap::new();
        for slot in new {
            let Some(stack) = stacks.get(slot) else {
                continue;
            };
            let places = places_for(*slot, show);
            if places.is_empty() {
                continue;
            }
            let entry = by_item.entry(stack.id.as_str()).or_default();
            for place in places {
                if !entry.contains(&place) {
                    entry.push(place);
                }
            }
        }
        for (item, places) in by_item {
            for (rarity, colour) in RARITIES {
                if out.len() >= MAX_DECORATIONS {
                    return out;
                }
                out.push(Decoration {
                    target: DecorationTarget::Stack(ModelMatch {
                        item: item.to_string(),
                        components: vec![ComponentMatch {
                            name: "minecraft:rarity".into(),
                            value: rarity.into(),
                        }],
                    }),
                    places: places.clone(),
                    fill: Some(colour),
                    border: None,
                    star: false,
                });
            }
        }
        return out;
    }
    for slot in new {
        let places = places_for(*slot, show);
        if places.is_empty() {
            continue;
        }
        out.push(Decoration {
            target: DecorationTarget::Slot(*slot),
            places,
            fill: Some(GLOW),
            border: None,
            // The glow styles: a star only in Glow + Star.
            star: style == STYLE_STAR,
        });
    }
    out
}

/// A decoration list as a comparable string (the runtime's own
/// spelling), so an unchanged tick costs no call.
fn spell(decorations: &[Decoration]) -> String {
    let mut out = String::new();
    for d in decorations {
        let target = match &d.target {
            DecorationTarget::Slot(slot) => format!("slot {slot}"),
            DecorationTarget::Stack(m) => {
                let mut s = if m.item.is_empty() {
                    "stack *".to_string()
                } else {
                    format!("stack {}", m.item)
                };
                for c in &m.components {
                    s.push_str(&format!(" {}={}", c.name, c.value));
                }
                s
            }
        };
        let places: Vec<&str> = d
            .places
            .iter()
            .map(|p| match p {
                DecorationPlace::Hotbar => "hotbar",
                DecorationPlace::Screens => "screens",
            })
            .collect();
        out.push_str(&format!("{target} @ {}", places.join(",")));
        if let Some(fill) = d.fill {
            out.push_str(&format!(" fill {fill:06x}"));
        }
        if let Some(border) = d.border {
            out.push_str(&format!(" border {border:06x}"));
        }
        if d.star {
            out.push_str(" star");
        }
        out.push('\n');
    }
    out
}

/// A stored index into a cycle of `len`; `default` when
/// unreadable.
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

/// A stored on/off setting; `default` when unreadable.
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

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn style_label(style: usize) -> String {
    format!(
        "Style: {}",
        ["Glow", "Glow + Star", "By Rarity"][style.min(2)]
    )
}

fn show_label(show: usize) -> String {
    format!(
        "Show On: {}",
        ["Hotbar + Screens", "Hotbar", "Screens"][show.min(2)]
    )
}

fn hover_label(on: bool) -> String {
    format!("Clear When Pointed At: {}", on_off(on))
}

fn close_label(on: bool) -> String {
    format!("Clear When Screen Closes: {}", on_off(on))
}

struct ItemNew {
    style: usize,
    show: usize,
    clear_hover: bool,
    clear_close: bool,
    player: Tracker,
    container: Tracker,
    container_kind: Option<String>,
    new: BTreeSet<u32>,
    hidden: bool,
    last_screen: String,
    declared: String,
}

impl ItemNew {
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: STYLE_BUTTON,
                label: style_label(self.style),
            }),
            Widget::Button(ButtonWidget {
                id: SHOW_BUTTON,
                label: show_label(self.show),
            }),
            Widget::Button(ButtonWidget {
                id: HOVER_BUTTON,
                label: hover_label(self.clear_hover),
            }),
            Widget::Button(ButtonWidget {
                id: CLOSE_BUTTON,
                label: close_label(self.clear_close),
            }),
        ]);
    }

    /// Leaves the world state behind (the title screen): the
    /// next world's first snapshot is a fresh baseline.
    fn reset(&mut self) {
        self.player.reprime();
        self.container.reprime();
        self.container_kind = None;
        self.new.clear();
        self.hidden = false;
    }

    fn tick(&mut self) {
        let state = screen::state();
        if state.id == "title" {
            self.reset();
            self.last_screen = state.id.clone();
            self.declare(&BTreeMap::new());
            return;
        }
        // Pointing at a marked slot clears its mark.
        if self.clear_hover
            && let Some(slot) = state.hovered_slot
        {
            self.new.remove(&slot);
        }
        // Closing a screen clears every mark.
        if self.clear_close
            && self.last_screen != "playing"
            && self.last_screen != "title"
            && state.id == "playing"
        {
            self.new.clear();
        }
        self.last_screen = state.id.clone();

        let inv = inventory::inventory();
        for slot in self.player.observe(&inv.slots) {
            self.new.insert(slot);
        }
        let container = inventory::open_container();
        if let Some(container) = &container {
            if self.container_kind.as_deref() != Some(container.kind.as_str()) {
                self.container.reprime();
                self.container_kind = Some(container.kind.clone());
            }
            for slot in self.container.observe(&container.slots) {
                self.new.insert(slot);
            }
        } else {
            self.container_kind = None;
            self.container.reprime();
        }
        let mut stacks: BTreeMap<u32, &InvStack> =
            inv.slots.iter().map(|s| (s.slot, s)).collect();
        if let Some(container) = &container {
            for stack in &container.slots {
                stacks.insert(stack.slot, stack);
            }
        }
        // A mark lives only while its slot holds a stack.
        self.new.retain(|slot| stacks.contains_key(slot));
        self.declare(&stacks);
    }

    /// Replaces the declared decorations when they changed —
    /// nothing while the marks are hidden.
    fn declare(&mut self, stacks: &BTreeMap<u32, &InvStack>) {
        let decorations = if self.hidden {
            Vec::new()
        } else {
            build_decorations(&self.new, stacks, self.style, self.show)
        };
        let spelling = spell(&decorations);
        if spelling != self.declared {
            item_model::set_decorations(&decorations);
            self.declared = spelling;
        }
    }
}

impl Mod for ItemNew {
    fn init() -> Result<(Self, Registration), String> {
        let state = ItemNew {
            style: read_index(STYLE_KEY, STYLES, STYLE_GLOW),
            show: read_index(SHOW_KEY, SHOWS, SHOW_BOTH),
            clear_hover: read_bool(HOVER_KEY, true),
            clear_close: read_bool(CLOSE_KEY, true),
            player: Tracker::default(),
            container: Tracker::default(),
            container_kind: None,
            new: BTreeSet::new(),
            hidden: false,
            last_screen: "title".into(),
            declared: String::new(),
        };
        Ok((state, Registration::new().command("itemnew").on_tick()))
    }

    fn tick(&mut self, _tick: u64) {
        self.tick();
    }

    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        Ok(Some(match args.trim() {
            "clear" => {
                self.new.clear();
                self.hidden = false;
                "Item New: marks cleared".to_string()
            }
            "" => {
                if self.new.is_empty() {
                    "Item New: nothing new".to_string()
                } else if self.hidden {
                    self.hidden = false;
                    "Item New: marks shown".to_string()
                } else {
                    self.hidden = true;
                    "Item New: marks hidden".to_string()
                }
            }
            _ => "Item New: /itemnew hides or shows the marks, /itemnew clear forgets them"
                .to_string(),
        }))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            STYLE_BUTTON => {
                self.style = (self.style + 1) % STYLES;
                let _ = storage::set(STYLE_KEY, self.style.to_string().as_bytes());
            }
            SHOW_BUTTON => {
                self.show = (self.show + 1) % SHOWS;
                let _ = storage::set(SHOW_KEY, self.show.to_string().as_bytes());
            }
            HOVER_BUTTON => {
                self.clear_hover = !self.clear_hover;
                let _ = storage::set(
                    HOVER_KEY,
                    if self.clear_hover { b"true" } else { b"false" },
                );
            }
            CLOSE_BUTTON => {
                self.clear_close = !self.clear_close;
                let _ = storage::set(
                    CLOSE_KEY,
                    if self.clear_close { b"true" } else { b"false" },
                );
            }
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(ItemNew);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_23() {
        assert_eq!(API_LEVEL, 23);
    }

    fn stack(slot: u32, id: &str, count: u32) -> InvStack {
        InvStack {
            slot,
            id: id.into(),
            count,
            damage: 0,
            max_damage: 0,
            components: String::new(),
        }
    }

    fn named(slot: u32, id: &str, count: u32, name: &str) -> InvStack {
        let mut stack = stack(slot, id, count);
        stack.components = format!("[custom_name={name}]");
        stack
    }

    #[test]
    fn the_first_snapshot_is_a_baseline() {
        let mut tracker = Tracker::default();
        assert!(tracker.observe(&[stack(0, "minecraft:apple", 3)]).is_empty());
        // And an empty world primes the same way.
        let mut tracker = Tracker::default();
        assert!(tracker.observe(&[]).is_empty());
        assert_eq!(
            tracker.observe(&[stack(0, "minecraft:apple", 3)]),
            vec![0]
        );
    }

    #[test]
    fn pickups_crafts_and_growth_mark() {
        let mut tracker = Tracker::default();
        tracker.observe(&[stack(9, "minecraft:dirt", 10)]);
        // A new stack appears.
        assert_eq!(
            tracker.observe(&[stack(9, "minecraft:dirt", 10), stack(0, "minecraft:apple", 1)]),
            vec![0]
        );
        // A held stack grows (a merge pickup).
        assert_eq!(
            tracker.observe(&[stack(9, "minecraft:dirt", 10), stack(0, "minecraft:apple", 3)]),
            vec![0]
        );
        // Nothing changes: nothing marks.
        assert!(
            tracker
                .observe(&[stack(9, "minecraft:dirt", 10), stack(0, "minecraft:apple", 3)])
                .is_empty()
        );
        // A stack only shrinks: nothing marks.
        assert!(
            tracker
                .observe(&[stack(9, "minecraft:dirt", 4), stack(0, "minecraft:apple", 3)])
                .is_empty()
        );
    }

    #[test]
    fn moves_swaps_and_splits_mark_nothing() {
        let mut tracker = Tracker::default();
        tracker.observe(&[stack(0, "minecraft:apple", 5), stack(9, "minecraft:dirt", 7)]);
        // A drag from the hotbar into the bag.
        assert!(
            tracker
                .observe(&[stack(10, "minecraft:apple", 5), stack(9, "minecraft:dirt", 7)])
                .is_empty()
        );
        // A swap of the two stacks.
        assert!(
            tracker
                .observe(&[stack(10, "minecraft:dirt", 7), stack(9, "minecraft:apple", 5)])
                .is_empty()
        );
        // A split: half stays, half moves.
        assert!(
            tracker
                .observe(&[
                    stack(10, "minecraft:dirt", 7),
                    stack(9, "minecraft:apple", 2),
                    stack(11, "minecraft:apple", 3),
                ])
                .is_empty()
        );
    }

    #[test]
    fn identity_is_item_and_components() {
        let mut tracker = Tracker::default();
        tracker.observe(&[stack(0, "minecraft:apple", 2)]);
        // A different item in the same slot is a gain of it.
        assert_eq!(tracker.observe(&[stack(0, "minecraft:pear", 2)]), vec![0]);
        // A renamed stack is a new identity, likewise.
        let mut tracker = Tracker::default();
        tracker.observe(&[stack(0, "minecraft:apple", 2)]);
        assert_eq!(
            tracker.observe(&[named(0, "minecraft:apple", 2, "Snack")]),
            vec![0]
        );
    }

    #[test]
    fn places_follow_the_slot_and_the_setting() {
        assert_eq!(
            places_for(0, SHOW_BOTH),
            vec![DecorationPlace::Hotbar, DecorationPlace::Screens]
        );
        assert_eq!(places_for(0, SHOW_HOTBAR), vec![DecorationPlace::Hotbar]);
        assert_eq!(places_for(0, SHOW_SCREENS), vec![DecorationPlace::Screens]);
        assert_eq!(places_for(20, SHOW_BOTH), vec![DecorationPlace::Screens]);
        assert_eq!(places_for(300, SHOW_SCREENS), vec![DecorationPlace::Screens]);
        assert!(places_for(20, SHOW_HOTBAR).is_empty());
        assert!(places_for(300, SHOW_HOTBAR).is_empty());
    }

    #[test]
    fn glow_decorations_mark_the_new_slots() {
        let a = stack(0, "minecraft:apple", 1);
        let b = stack(300, "minecraft:dirt", 4);
        let stacks: BTreeMap<u32, &InvStack> = [(0, &a), (300, &b)].into_iter().collect();
        let new: BTreeSet<u32> = [0, 300].into_iter().collect();
        let glow = build_decorations(&new, &stacks, STYLE_GLOW, SHOW_BOTH);
        assert_eq!(
            spell(&glow),
            "slot 0 @ hotbar,screens fill fff3b0\nslot 300 @ screens fill fff3b0\n"
        );
        let star = build_decorations(&new, &stacks, STYLE_STAR, SHOW_BOTH);
        assert!(spell(&star).contains("slot 0 @ hotbar,screens fill fff3b0 star"));
        // Hotbar-only showing drops the container slot entirely.
        let hotbar = build_decorations(&new, &stacks, STYLE_GLOW, SHOW_HOTBAR);
        assert_eq!(spell(&hotbar), "slot 0 @ hotbar fill fff3b0\n");
    }

    #[test]
    fn rarity_decorations_match_the_item_by_rarity() {
        let a = stack(0, "minecraft:dragon_egg", 1);
        let stacks: BTreeMap<u32, &InvStack> = [(0, &a)].into_iter().collect();
        let new: BTreeSet<u32> = [0].into_iter().collect();
        let decorations = build_decorations(&new, &stacks, STYLE_RARITY, SHOW_BOTH);
        // One mark per rarity, in vanilla's rarity order, each
        // keyed on the item and that rarity; exactly one can
        // hold against the stack's merged components.
        assert_eq!(
            spell(&decorations),
            "stack minecraft:dragon_egg minecraft:rarity=common @ hotbar,screens fill ffffff\n\
             stack minecraft:dragon_egg minecraft:rarity=uncommon @ hotbar,screens fill ffff55\n\
             stack minecraft:dragon_egg minecraft:rarity=rare @ hotbar,screens fill 55ffff\n\
             stack minecraft:dragon_egg minecraft:rarity=epic @ hotbar,screens fill ff55ff\n"
        );
    }

    #[test]
    fn settings_labels_name_the_originals_choices() {
        assert_eq!(style_label(0), "Style: Glow");
        assert_eq!(style_label(1), "Style: Glow + Star");
        assert_eq!(style_label(2), "Style: By Rarity");
        assert_eq!(show_label(0), "Show On: Hotbar + Screens");
        assert_eq!(show_label(1), "Show On: Hotbar");
        assert_eq!(show_label(2), "Show On: Screens");
        assert_eq!(hover_label(true), "Clear When Pointed At: On");
        assert_eq!(close_label(false), "Clear When Screen Closes: Off");
    }
}

//! Right-Click Crops for Kestrel: right-click a mature plant to
//! harvest it — with the game's own loot, tool and Fortune
//! effects, because the harvest is the game's own dig — and
//! replant it for exactly one matching planting item.
//!
//! A behaviour port of Right-Click Crops (MIT) by AyanoKaito,
//! following its registry description and its harvesting guide
//! (<https://github.com/AyanoKaito/horizon-right-click-crops>):
//! sixteen plant families in three behaviour groups — harvest
//! and replant; harvest the upper growth and retain the base;
//! harvest the fruit and retain the stem — per-family settings,
//! and its leave-alone rules. This is a fresh implementation
//! against the Kestrel SDK (the `actions` capability, API 21):
//! the harvest is a synthesized `attack` click on the crop, the
//! replant a `use-on-block` of the planting item on the soil,
//! exactly the clicks a player would make. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/right_click_crops>.
//!
//! How a click flows: the mod watches the use button and the
//! crosshair (the original hooks the interaction itself). A
//! right-click — or, holding the button, arriving at a new plant,
//! as the original handles successive targets — on a mature,
//! enabled plant arms a harvest; the attack fires a tick later,
//! once the aim is seen to still rest on the plant (an `attack`
//! click has no target of its own, so the port checks before
//! swinging). The port then watches for the plant to be gone —
//! if it is still there, the world refused the dig and nothing
//! is replanted into it — and pays one planting item: the
//! harvest's own loot first as it lands in the inventory, then
//! the inventory itself. What it can reach is the hand, the
//! hotbar (chosen with `select-slot`, and the player's own slot
//! chosen back afterwards), the offhand, and the bag — a stack
//! there is brought into the hand with one Swap click
//! (`inventory-actions`, API 24), trading places with the
//! stack in the selected slot.
//!
//! Where the SDK cannot carry the original, the port says so
//! rather than imitating (see the README): the
//! original's survival-only gate needs a game-mode read the
//! client SDK does not have; and the checks that read the
//! world's blocks (a tall plant's base, the torchflower's
//! farmland, a fruit's attached stem) work where client world
//! reads answer — single player in v1 — and hold those families
//! back where they don't. (Paying from the bag was on this
//! list until 0.2.0: the inventory-actions district's Swap
//! click — B6, API 24 — closed it.)

use horizon_mod_sdk::{
    ActionHand, ActionTarget, BlockTarget, ClickKind, Face, Mod, Registration, actions, game,
    input,
    inventory::{self, InvStack},
    inventory_actions, storage,
    ui::{self, ButtonWidget, Widget},
    world,
};

/// The newest API level this mod needs, asserted in the tests
/// below: `inventory-actions.click` is level 24
/// (`crates/horizon-mods/src/api_level.rs`); `actions.target`,
/// `actions.attack`, `actions.use-on-block` and
/// `actions.select-slot` are level 21, and everything else
/// here is older.
#[cfg(test)]
const API_LEVEL: u32 = 24;

/// Ticks to wait for a queued dig to show before giving the
/// harvest up (the world refused it: adventure restrictions,
/// spawn protection...). The dig performs a tick after it is
/// queued and a hardness-0 crop breaks at once, so this is
/// generous.
const GONE_LIMIT: u32 = 10;

/// Ticks after the harvest to keep looking for a planting item
/// before harvesting without replanting: the window in which
/// the harvest's own drops reach the inventory, which is how
/// "generated loot pays first" is honoured.
const ITEM_LIMIT: u32 = 8;

/// The sixteen families, in the original's own order (its
/// behaviour groups: replanted, base kept, stem kept).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    Wheat,
    Carrots,
    Potatoes,
    Beetroot,
    NetherWart,
    Cocoa,
    Torchflower,
    PitcherCrop,
    SugarCane,
    Cactus,
    Bamboo,
    Kelp,
    TwistingVines,
    WeepingVines,
    Pumpkin,
    Melon,
}

const FAMILIES: [Family; 16] = [
    Family::Wheat,
    Family::Carrots,
    Family::Potatoes,
    Family::Beetroot,
    Family::NetherWart,
    Family::Cocoa,
    Family::Torchflower,
    Family::PitcherCrop,
    Family::SugarCane,
    Family::Cactus,
    Family::Bamboo,
    Family::Kelp,
    Family::TwistingVines,
    Family::WeepingVines,
    Family::Pumpkin,
    Family::Melon,
];

impl Family {
    /// The settings page's name for the family.
    fn label(self) -> &'static str {
        match self {
            Family::Wheat => "Wheat",
            Family::Carrots => "Carrots",
            Family::Potatoes => "Potatoes",
            Family::Beetroot => "Beetroot",
            Family::NetherWart => "Nether Wart",
            Family::Cocoa => "Cocoa",
            Family::Torchflower => "Torchflower",
            Family::PitcherCrop => "Pitcher Crop",
            Family::SugarCane => "Sugar Cane",
            Family::Cactus => "Cactus",
            Family::Bamboo => "Bamboo",
            Family::Kelp => "Kelp",
            Family::TwistingVines => "Twisting Vines",
            Family::WeepingVines => "Weeping Vines",
            Family::Pumpkin => "Pumpkin",
            Family::Melon => "Melon",
        }
    }

    /// The family's storage key and settings button.
    fn key(self) -> &'static str {
        match self {
            Family::Wheat => "wheat",
            Family::Carrots => "carrots",
            Family::Potatoes => "potatoes",
            Family::Beetroot => "beetroot",
            Family::NetherWart => "nether_wart",
            Family::Cocoa => "cocoa",
            Family::Torchflower => "torchflower",
            Family::PitcherCrop => "pitcher_crop",
            Family::SugarCane => "sugar_cane",
            Family::Cactus => "cactus",
            Family::Bamboo => "bamboo",
            Family::Kelp => "kelp",
            Family::TwistingVines => "twisting_vines",
            Family::WeepingVines => "weeping_vines",
            Family::Pumpkin => "pumpkin",
            Family::Melon => "melon",
        }
    }

    fn button(self) -> u32 {
        FAMILIES.iter().position(|&f| f == self).unwrap() as u32
    }
}

/// What harvesting a family does, once its plant is targeted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// Harvest, then replant with the planting item.
    Replant { item: &'static str },
    /// A column growing up: harvest growth, keep the lowest
    /// block.
    TallUp,
    /// A column hanging down: harvest growth, keep the top
    /// block (its anchor).
    TallDown,
    /// A fruit: harvest it; its attached stem stays.
    Fruit,
}

/// The check a harvest must pass against the world's blocks
/// before the attack is armed — the original's own conditions,
/// read off the blocks around the plant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Gate {
    /// Nothing around the plant matters.
    None,
    /// The block below must be farmland (a decorative
    /// torchflower away from farmland is not managed).
    FarmlandBelow,
    /// The block below must be the same plant: the target is
    /// growth, not the retained base.
    GrowthAboveBase,
    /// The block above must be the same plant: the target is
    /// hanging growth, not the retained anchor.
    GrowthBelowAnchor,
    /// An attached stem beside the fruit must point at it —
    /// where the world's blocks can be read at all.
    StemBeside,
}

/// A harvest worked out for one targeted plant. (The family,
/// kind and gate are spent working it out; what the flight
/// needs is where the plant stands and what follows.)
#[derive(Clone, Debug)]
struct Plan {
    /// The plant the player clicked.
    clicked: [i32; 3],
    /// The block that must stop being the crop for the harvest
    /// to have happened (for the pitcher crop, its lower half:
    /// either half clicked performs one harvest).
    watch: [i32; 3],
    /// The crop's block id, to recognise it by.
    crop_id: String,
    /// Where the replant goes, for the replanting families.
    replant: Option<Replant>,
}

/// A replant: the item it costs and the target it is used on.
#[derive(Clone, Debug)]
struct Replant {
    item: &'static str,
    target: BlockTarget,
}

/// The id of a block state string: `minecraft:wheat[age=7]` is
/// `minecraft:wheat`.
fn base_id(state: &str) -> &str {
    state.split('[').next().unwrap_or(state)
}

/// One property of a block state string: `age` in
/// `minecraft:cocoa[age=2,facing=north]`.
fn prop<'a>(state: &'a str, name: &str) -> Option<&'a str> {
    let props = state.split('[').nth(1)?.trim_end_matches(']');
    for part in props.split(',') {
        let (key, value) = part.trim().split_once('=')?;
        if key == name {
            return Some(value);
        }
    }
    None
}

fn age(state: &str) -> Option<u32> {
    prop(state, "age")?.parse().ok()
}

/// A horizontal facing as (the `Face`, its unit vector).
fn facing_of(state: &str) -> Option<(Face, [i32; 3])> {
    match prop(state, "facing")? {
        "north" => Some((Face::North, [0, 0, -1])),
        "south" => Some((Face::South, [0, 0, 1])),
        "west" => Some((Face::West, [-1, 0, 0])),
        "east" => Some((Face::East, [1, 0, 0])),
        _ => None,
    }
}

/// What a block state is to this mod: the family it belongs to,
/// what harvesting it does, and the gate it must pass — or none,
/// for anything the mod does not manage (immature plants
/// included: a family's mature age is its own maximum).
///
/// The replant's target is filled in by `plan` below, which
/// knows where the plant stands.
fn classify(state: &str) -> Option<(Family, Kind, Gate)> {
    let id = base_id(state);
    let replant = |item| Kind::Replant { item };
    Some(match id {
        "minecraft:wheat" if age(state) == Some(7) => {
            (Family::Wheat, replant("minecraft:wheat_seeds"), Gate::None)
        }
        "minecraft:carrots" if age(state) == Some(7) => {
            (Family::Carrots, replant("minecraft:carrot"), Gate::None)
        }
        "minecraft:potatoes" if age(state) == Some(7) => {
            (Family::Potatoes, replant("minecraft:potato"), Gate::None)
        }
        "minecraft:beetroots" if age(state) == Some(3) => (
            Family::Beetroot,
            replant("minecraft:beetroot_seeds"),
            Gate::None,
        ),
        "minecraft:nether_wart" if age(state) == Some(3) => (
            Family::NetherWart,
            replant("minecraft:nether_wart"),
            Gate::None,
        ),
        "minecraft:cocoa" if age(state) == Some(2) => {
            (Family::Cocoa, replant("minecraft:cocoa_beans"), Gate::None)
        }
        "minecraft:torchflower_crop" if age(state) == Some(2) => (
            Family::Torchflower,
            replant("minecraft:torchflower_seeds"),
            Gate::FarmlandBelow,
        ),
        "minecraft:torchflower" => (
            Family::Torchflower,
            replant("minecraft:torchflower_seeds"),
            Gate::FarmlandBelow,
        ),
        "minecraft:pitcher_crop" if age(state) == Some(4) => (
            Family::PitcherCrop,
            replant("minecraft:pitcher_pod"),
            Gate::None,
        ),
        "minecraft:sugar_cane" => (Family::SugarCane, Kind::TallUp, Gate::GrowthAboveBase),
        "minecraft:cactus" => (Family::Cactus, Kind::TallUp, Gate::GrowthAboveBase),
        "minecraft:bamboo" => (Family::Bamboo, Kind::TallUp, Gate::GrowthAboveBase),
        "minecraft:kelp" | "minecraft:kelp_plant" => {
            (Family::Kelp, Kind::TallUp, Gate::GrowthAboveBase)
        }
        "minecraft:twisting_vines" | "minecraft:twisting_vines_plant" => {
            (Family::TwistingVines, Kind::TallUp, Gate::GrowthAboveBase)
        }
        "minecraft:weeping_vines" | "minecraft:weeping_vines_plant" => (
            Family::WeepingVines,
            Kind::TallDown,
            Gate::GrowthBelowAnchor,
        ),
        "minecraft:pumpkin" => (Family::Pumpkin, Kind::Fruit, Gate::StemBeside),
        "minecraft:melon" => (Family::Melon, Kind::Fruit, Gate::StemBeside),
        _ => return None,
    })
}

/// Whether a block id is the same plant as a tall family — body
/// and tip, and for bamboo the sapling it starts as.
fn same_plant(family: Family, id: &str) -> bool {
    match family {
        Family::SugarCane => id == "minecraft:sugar_cane",
        Family::Cactus => id == "minecraft:cactus",
        Family::Bamboo => id == "minecraft:bamboo" || id == "minecraft:bamboo_sapling",
        Family::Kelp => id == "minecraft:kelp" || id == "minecraft:kelp_plant",
        Family::TwistingVines => {
            id == "minecraft:twisting_vines" || id == "minecraft:twisting_vines_plant"
        }
        Family::WeepingVines => {
            id == "minecraft:weeping_vines" || id == "minecraft:weeping_vines_plant"
        }
        _ => false,
    }
}

/// The gate's verdict, against a world read (`None` where the
/// read does not answer — a remote server, in v1): the families
/// whose condition is a world's block hold back there, except
/// the fruit's stem check, which is simply unverifiable and so
/// does not hold the harvest back.
fn gate_passes(
    gate: Gate,
    family: Family,
    pos: [i32; 3],
    read: &dyn Fn([i32; 3]) -> Option<String>,
) -> bool {
    match gate {
        Gate::None => true,
        Gate::FarmlandBelow => match read([pos[0], pos[1] - 1, pos[2]]) {
            Some(state) => base_id(&state) == "minecraft:farmland",
            None => false,
        },
        Gate::GrowthAboveBase => match read([pos[0], pos[1] - 1, pos[2]]) {
            Some(state) => same_plant(family, base_id(&state)),
            None => false,
        },
        Gate::GrowthBelowAnchor => match read([pos[0], pos[1] + 1, pos[2]]) {
            Some(state) => same_plant(family, base_id(&state)),
            None => false,
        },
        Gate::StemBeside => {
            let stem = match family {
                Family::Pumpkin => "minecraft:attached_pumpkin_stem",
                _ => "minecraft:attached_melon_stem",
            };
            // (the neighbour's offset, the facing that points
            // from it at the fruit)
            let sides = [
                ([-1, 0, 0], "east"),
                ([1, 0, 0], "west"),
                ([0, 0, -1], "south"),
                ([0, 0, 1], "north"),
            ];
            let mut any_read = false;
            for (off, facing) in sides {
                let Some(state) = read([pos[0] + off[0], pos[1] + off[1], pos[2] + off[2]]) else {
                    continue;
                };
                any_read = true;
                if base_id(&state) == stem && prop(&state, "facing") == Some(facing) {
                    return true;
                }
            }
            // No read answered: the check cannot be made, and
            // the harvest goes ahead without it.
            !any_read
        }
    }
}

/// The original's leave-alone rules, against the click's
/// context: bone meal in either hand fertilises instead; a
/// crouch with an item in either hand is the player's own item
/// use; shears on a pumpkin carve it. (Its fourth rule, that
/// the actor is in survival, needs a game-mode read the client
/// SDK does not expose — see the README.)
fn leave_alone(family: Family, main: Option<&str>, off: Option<&str>, sneaking: bool) -> bool {
    if main == Some("minecraft:bone_meal") || off == Some("minecraft:bone_meal") {
        return true;
    }
    if sneaking && (main.is_some() || off.is_some()) {
        return true;
    }
    if family == Family::Pumpkin
        && (main == Some("minecraft:shears") || off == Some("minecraft:shears"))
    {
        return true;
    }
    false
}

/// The replant's target: the soil under a crop (its top face),
/// or the log a cocoa pod hung on (the face it faced). `soil` /
/// `log` is the block's state as read, when a read answered —
/// the host re-picks on position and face alone; the string is
/// the port's honest name for what stands there, falling back
/// to the family's usual ground.
fn replant_target(
    kind: Kind,
    clicked_state: &str,
    clicked_pos: [i32; 3],
    cursor: [f32; 3],
    ground_state: Option<&str>,
) -> Option<(BlockTarget, [i32; 3])> {
    let Kind::Replant { .. } = kind else {
        return None;
    };
    let id = base_id(clicked_state);
    if id == "minecraft:cocoa" {
        let (face, vec) = facing_of(clicked_state)?;
        let log = [
            clicked_pos[0] - vec[0],
            clicked_pos[1] - vec[1],
            clicked_pos[2] - vec[2],
        ];
        // The cursor on the log's face: the clicked point's
        // tangential parts, on the face's plane.
        let c = match face {
            Face::North => [cursor[0], cursor[1], 0.0],
            Face::South => [cursor[0], cursor[1], 1.0],
            Face::West => [0.0, cursor[1], cursor[2]],
            Face::East => [1.0, cursor[1], cursor[2]],
            _ => cursor,
        };
        return Some((
            BlockTarget {
                block_x: log[0],
                block_y: log[1],
                block_z: log[2],
                block: ground_state.unwrap_or("minecraft:jungle_log").to_string(),
                face,
                cursor_x: c[0],
                cursor_y: c[1],
                cursor_z: c[2],
            },
            log,
        ));
    }
    // The lower half is where a pitcher crop stands: clicking
    // its upper half replants at the lower one's soil.
    let standing = if id == "minecraft:pitcher_crop" && prop(clicked_state, "half") == Some("upper")
    {
        [clicked_pos[0], clicked_pos[1] - 1, clicked_pos[2]]
    } else {
        clicked_pos
    };
    let soil = [standing[0], standing[1] - 1, standing[2]];
    let fallback = if id == "minecraft:nether_wart" {
        "minecraft:soul_sand"
    } else {
        "minecraft:farmland"
    };
    Some((
        BlockTarget {
            block_x: soil[0],
            block_y: soil[1],
            block_z: soil[2],
            block: ground_state.unwrap_or(fallback).to_string(),
            face: Face::Up,
            cursor_x: cursor[0],
            cursor_y: 1.0,
            cursor_z: cursor[2],
        },
        standing,
    ))
}

/// Where a planting item can be paid from, in the order the
/// port looks: the original pays from the generated loot first,
/// then the inventory — the loot lands in the inventory, so the
/// port looks at the hand the player is already holding (where
/// loot stacks onto a matching pile), the rest of the hotbar,
/// the offhand, and last the bag (9–35): a stack there is
/// brought into the hand with one Swap click, the
/// inventory-actions district's verb (API 24), so its slot
/// rides along.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Payment {
    Selected,
    Hotbar(u32),
    Offhand,
    Bag(u32),
    None,
}

fn find_payment(slots: &[InvStack], selected: u32, item: &str) -> Payment {
    let has = |slot: u32| slots.iter().any(|s| s.slot == slot && s.id == item);
    if has(selected) {
        return Payment::Selected;
    }
    for slot in 0..9u32 {
        if slot != selected && has(slot) {
            return Payment::Hotbar(slot);
        }
    }
    if has(504) {
        return Payment::Offhand;
    }
    for slot in 9..36u32 {
        if has(slot) {
            return Payment::Bag(slot);
        }
    }
    Payment::None
}

/// A stored family switch ("true" / "false"); on when
/// unreadable — the original's families are all on until
/// switched off.
fn read_switch(key: &str) -> bool {
    match storage::get(key) {
        Some(bytes) => match std::str::from_utf8(&bytes).ok().map(str::trim) {
            Some("false") => false,
            _ => true,
        },
        None => true,
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

/// A harvest in flight.
struct Pending {
    plan: Plan,
    /// The attack has been queued (it performs a tick after it
    /// is queued; the plan is armed for one tick first, while
    /// the aim is checked — see the module docs).
    fired: bool,
    /// Ticks since the attack was queued.
    ticks: u32,
    /// Ticks since the crop was first seen gone.
    gone_ticks: Option<u32>,
    /// A bag payment's Swap click has been queued: it performs
    /// a tick later, so the payment search must not queue a
    /// second one — a swap is symmetric, and a second click on
    /// the same slot would trade the item straight back into
    /// the bag.
    bag_swap_queued: bool,
}

struct RightClickCrops {
    /// The sixteen family switches, by settings button.
    enabled: [bool; 16],
    pending: Option<Pending>,
    /// The plant last handled, so holding the button across one
    /// plant handles it once; arriving at a new one handles it
    /// in turn.
    last_handled: Option<[i32; 3]>,
    /// After a replant paid from another hotbar slot: (the
    /// player's own slot, the slot chosen for the payment), to
    /// choose the player's slot back next tick.
    restore: Option<(u32, u32)>,
}

impl RightClickCrops {
    fn refresh_settings(&self) {
        let widgets: Vec<Widget> = FAMILIES
            .iter()
            .map(|family| {
                Widget::Button(ButtonWidget {
                    id: family.button(),
                    label: format!(
                        "{}: {}",
                        family.label(),
                        on_off(self.enabled[family.button() as usize])
                    ),
                })
            })
            .collect();
        ui::set_settings(&widgets);
    }

    /// Whether the crop of `pending` has gone: read from the
    /// world where reads answer; elsewhere, from the crosshair
    /// no longer resting on it.
    fn crop_gone(&self, plan: &Plan, dimension: &str) -> bool {
        match world::get_block(dimension, plan.watch[0], plan.watch[1], plan.watch[2]) {
            Some(state) => base_id(&state) != plan.crop_id,
            None => !matches!(
                actions::target(),
                Some(ActionTarget::Block(b))
                    if [b.block_x, b.block_y, b.block_z] == plan.clicked
                        && base_id(&b.block) == plan.crop_id.as_str()
            ),
        }
    }

    /// Advances a harvest in flight; clears `self.pending` when
    /// it resolves, one way or the other.
    fn advance(&mut self, dimension: &str) {
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        let plan = &pending.plan;
        if !pending.fired {
            // The aim check: the attack click has no target of
            // its own, so it is only swung once the crosshair is
            // seen to still rest on the plant.
            let still = matches!(
                actions::target(),
                Some(ActionTarget::Block(b))
                    if [b.block_x, b.block_y, b.block_z] == plan.clicked
                        && base_id(&b.block) == plan.crop_id.as_str()
            );
            if still {
                actions::attack();
                pending.fired = true;
                self.pending = Some(pending);
            }
            // Not still there: the swing is never made, and the
            // click is simply left alone after all.
            return;
        }
        pending.ticks += 1;
        if pending.gone_ticks.is_none() {
            if self.crop_gone(plan, dimension) {
                pending.gone_ticks = Some(0);
            } else if pending.ticks > GONE_LIMIT {
                // Still standing: the world refused the dig.
                // Nothing is replanted into a standing crop.
                return;
            } else {
                self.pending = Some(pending);
                return;
            }
        }
        // The crop is gone. Tall plants and fruit have nothing
        // after this; a replanting family pays and replants.
        let Some(replant) = &plan.replant else {
            return;
        };
        let gone = pending.gone_ticks.unwrap() + 1;
        pending.gone_ticks = Some(gone);
        let inv = inventory::inventory();
        match find_payment(&inv.slots, inv.selected, replant.item) {
            Payment::Selected => {
                let _ = actions::use_on_block(&replant.target, ActionHand::Main);
            }
            Payment::Offhand => {
                let _ = actions::use_on_block(&replant.target, ActionHand::Off);
            }
            Payment::Hotbar(slot) => {
                let _ = actions::select_slot(slot);
                let _ = actions::use_on_block(&replant.target, ActionHand::Main);
                self.restore = Some((inv.selected, slot));
            }
            Payment::Bag(slot) => {
                // The bag pays last: one Swap click trades the
                // planting item's stack with the selected
                // slot's, into the hand; once it lands there,
                // the Selected arm above replants with it on a
                // following tick. Queued once — until it lands,
                // the search keeps finding the item in the bag.
                if !pending.bag_swap_queued {
                    let _ = inventory_actions::click(Some(slot), inv.selected, ClickKind::Swap);
                    pending.bag_swap_queued = true;
                }
                if gone <= ITEM_LIMIT {
                    self.pending = Some(pending);
                }
                // Past the window with the click unlanded (the
                // game dropped it — another screen in the way):
                // harvested, not replanted.
            }
            Payment::None => {
                if gone <= ITEM_LIMIT {
                    // The harvest's drops may still be on their
                    // way into the inventory: keep looking.
                    self.pending = Some(pending);
                }
                // Past the window: harvested, not replanted —
                // the original's answer when no planting item
                // can be had.
            }
        }
    }

    /// Considers the plant under the crosshair for a harvest:
    /// the trigger — a click, or holding the button and arriving
    /// at a new plant.
    fn consider(&mut self, player: &game::Player) {
        let Some(ActionTarget::Block(block)) = actions::target() else {
            return;
        };
        let pos = [block.block_x, block.block_y, block.block_z];
        if self.last_handled == Some(pos) {
            return;
        }
        let Some((family, kind, gate)) = classify(&block.block) else {
            return;
        };
        if !self.enabled[family.button() as usize] {
            return;
        }
        let inv = inventory::inventory();
        let main = inv
            .slots
            .iter()
            .find(|s| s.slot == inv.selected)
            .map(|s| s.id.as_str());
        let off = inv
            .slots
            .iter()
            .find(|s| s.slot == 504)
            .map(|s| s.id.as_str());
        let sneaking = game::get_self_state().is_some_and(|s| s.sneaking);
        if leave_alone(family, main, off, sneaking) {
            return;
        }
        let dimension = player.dimension.clone();
        let read = |p: [i32; 3]| world::get_block(&dimension, p[0], p[1], p[2]);
        if !gate_passes(gate, family, pos, &read) {
            return;
        }
        // The harvest is under way: this plant is handled, and
        // no other is until it resolves.
        self.last_handled = Some(pos);
        let (replant, watch) = match kind {
            Kind::Replant { item } => {
                let ground_pos = if base_id(&block.block) == "minecraft:cocoa" {
                    facing_of(&block.block)
                        .map(|(_, v)| [pos[0] - v[0], pos[1] - v[1], pos[2] - v[2]])
                } else {
                    Some([pos[0], pos[1] - 1, pos[2]])
                };
                let ground = ground_pos.and_then(|p| read(p));
                match replant_target(
                    kind,
                    &block.block,
                    pos,
                    [block.cursor_x, block.cursor_y, block.cursor_z],
                    ground.as_deref(),
                ) {
                    Some((target, standing)) => (
                        Some(Replant { item, target }),
                        if base_id(&block.block) == "minecraft:pitcher_crop" {
                            standing
                        } else {
                            pos
                        },
                    ),
                    None => (None, pos),
                }
            }
            _ => (None, pos),
        };
        self.pending = Some(Pending {
            plan: Plan {
                clicked: pos,
                watch,
                crop_id: base_id(&block.block).to_string(),
                replant,
            },
            fired: false,
            ticks: 0,
            gone_ticks: None,
            bag_swap_queued: false,
        });
    }
}

impl Mod for RightClickCrops {
    fn init() -> Result<(Self, Registration), String> {
        let mut enabled = [true; 16];
        for family in FAMILIES {
            enabled[family.button() as usize] = read_switch(family.key());
        }
        let state = RightClickCrops {
            enabled,
            pending: None,
            last_handled: None,
            restore: None,
        };
        Ok((state, Registration::new().on_tick()))
    }

    fn tick(&mut self, _tick: u64) {
        // Choose the player's own slot back after a replant
        // paid from another one — unless the player has chosen
        // a slot themselves since.
        if let Some((back, chosen)) = self.restore.take() {
            let inv = inventory::inventory();
            if inv.selected == chosen {
                let _ = actions::select_slot(back);
            }
        }
        let Some(player) = game::get_player() else {
            self.pending = None;
            self.last_handled = None;
            return;
        };
        if self.pending.is_some() {
            self.advance(&player.dimension.clone());
        }
        let use_held = input::mapping("key.use").is_some_and(|m| m.down);
        if !use_held {
            self.last_handled = None;
            return;
        }
        if self.pending.is_none() {
            self.consider(&player);
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        let Some(family) = FAMILIES.get(id as usize) else {
            return;
        };
        let on = !self.enabled[id as usize];
        self.enabled[id as usize] = on;
        let _ = storage::set(family.key(), if on { b"true" } else { b"false" });
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(RightClickCrops);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_24() {
        assert_eq!(API_LEVEL, 24);
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

    #[test]
    fn every_family_classifies_at_its_mature_age_only() {
        let mature = [
            ("minecraft:wheat[age=7]", Family::Wheat),
            ("minecraft:carrots[age=7]", Family::Carrots),
            ("minecraft:potatoes[age=7]", Family::Potatoes),
            ("minecraft:beetroots[age=3]", Family::Beetroot),
            ("minecraft:nether_wart[age=3]", Family::NetherWart),
            ("minecraft:cocoa[age=2,facing=north]", Family::Cocoa),
            ("minecraft:torchflower_crop[age=2]", Family::Torchflower),
            ("minecraft:torchflower", Family::Torchflower),
            (
                "minecraft:pitcher_crop[age=4,half=lower]",
                Family::PitcherCrop,
            ),
            ("minecraft:sugar_cane", Family::SugarCane),
            ("minecraft:cactus", Family::Cactus),
            ("minecraft:bamboo", Family::Bamboo),
            ("minecraft:kelp[age=12]", Family::Kelp),
            ("minecraft:kelp_plant", Family::Kelp),
            ("minecraft:twisting_vines[age=3]", Family::TwistingVines),
            ("minecraft:twisting_vines_plant", Family::TwistingVines),
            ("minecraft:weeping_vines[age=3]", Family::WeepingVines),
            ("minecraft:weeping_vines_plant", Family::WeepingVines),
            ("minecraft:pumpkin", Family::Pumpkin),
            ("minecraft:melon", Family::Melon),
        ];
        for (state, family) in mature {
            assert_eq!(classify(state).map(|c| c.0), Some(family), "{state}");
        }
        // One step short of mature is not managed.
        for state in [
            "minecraft:wheat[age=6]",
            "minecraft:carrots[age=0]",
            "minecraft:beetroots[age=2]",
            "minecraft:nether_wart[age=2]",
            "minecraft:cocoa[age=1,facing=east]",
            "minecraft:torchflower_crop[age=1]",
            "minecraft:pitcher_crop[age=3,half=upper]",
        ] {
            assert_eq!(classify(state), None, "{state}");
        }
        // Not plants the mod manages, berries above all: they
        // keep their normal behaviour.
        for state in [
            "minecraft:sweet_berry_bush[age=3]",
            "minecraft:glow_berries",
            "minecraft:cave_vines[age=4]",
            "minecraft:bamboo_sapling",
            "minecraft:stone",
            "minecraft:oak_log",
            "minecraft:pumpkin_stem",
            "minecraft:attached_pumpkin_stem[facing=east]",
        ] {
            assert_eq!(classify(state), None, "{state}");
        }
    }

    #[test]
    fn planting_items_are_the_originals() {
        let cases = [
            ("minecraft:wheat[age=7]", "minecraft:wheat_seeds"),
            ("minecraft:carrots[age=7]", "minecraft:carrot"),
            ("minecraft:potatoes[age=7]", "minecraft:potato"),
            ("minecraft:beetroots[age=3]", "minecraft:beetroot_seeds"),
            ("minecraft:nether_wart[age=3]", "minecraft:nether_wart"),
            (
                "minecraft:cocoa[age=2,facing=south]",
                "minecraft:cocoa_beans",
            ),
            (
                "minecraft:torchflower_crop[age=2]",
                "minecraft:torchflower_seeds",
            ),
            ("minecraft:torchflower", "minecraft:torchflower_seeds"),
            (
                "minecraft:pitcher_crop[age=4,half=upper]",
                "minecraft:pitcher_pod",
            ),
        ];
        for (state, item) in cases {
            match classify(state).map(|c| c.1) {
                Some(Kind::Replant { item: got }) => assert_eq!(got, item, "{state}"),
                other => panic!("{state}: expected a replant, got {other:?}"),
            }
        }
    }

    #[test]
    fn gates_read_the_blocks_around_the_plant() {
        use std::collections::HashMap;
        let world: HashMap<[i32; 3], &str> = HashMap::from([
            ([0, 63, 0], "minecraft:farmland"),
            ([1, 63, 0], "minecraft:dirt"),
            ([0, 64, 0], "minecraft:sugar_cane"),
            ([5, 63, 5], "minecraft:sand"),
            ([5, 64, 5], "minecraft:cactus"),
            ([5, 65, 5], "minecraft:cactus"),
            ([9, 70, 9], "minecraft:weeping_vines_plant"),
            ([9, 69, 9], "minecraft:weeping_vines"),
            ([3, 63, 3], "minecraft:pumpkin"),
            ([2, 63, 3], "minecraft:attached_pumpkin_stem[facing=east]"),
        ]);
        let read = |p: [i32; 3]| world.get(&p).map(|s| s.to_string());
        // Torchflower on farmland is managed; on dirt it is a
        // decoration and is left alone.
        assert!(gate_passes(
            Gate::FarmlandBelow,
            Family::Torchflower,
            [0, 64, 0],
            &read
        ));
        assert!(!gate_passes(
            Gate::FarmlandBelow,
            Family::Torchflower,
            [1, 64, 0],
            &read
        ));
        // A lone cane base is not growth; the block above its
        // base is.
        assert!(!gate_passes(
            Gate::GrowthAboveBase,
            Family::SugarCane,
            [0, 64, 0],
            &read
        ));
        assert!(!gate_passes(
            Gate::GrowthAboveBase,
            Family::Cactus,
            [5, 64, 5],
            &read
        ));
        assert!(gate_passes(
            Gate::GrowthAboveBase,
            Family::Cactus,
            [5, 65, 5],
            &read
        ));
        // Weeping vines: the anchor has no vine above it.
        assert!(!gate_passes(
            Gate::GrowthBelowAnchor,
            Family::WeepingVines,
            [9, 70, 9],
            &read
        ));
        assert!(gate_passes(
            Gate::GrowthBelowAnchor,
            Family::WeepingVines,
            [9, 69, 9],
            &read
        ));
        // A pumpkin with its stem pointing at it; without the
        // stem read, no harvest — and with no reads at all, the
        // harvest goes ahead unchecked.
        assert!(gate_passes(
            Gate::StemBeside,
            Family::Pumpkin,
            [3, 63, 3],
            &read
        ));
        assert!(!gate_passes(
            Gate::StemBeside,
            Family::Melon,
            [3, 63, 3],
            &read
        ));
        let no_reads = |_: [i32; 3]| None;
        assert!(gate_passes(
            Gate::StemBeside,
            Family::Pumpkin,
            [3, 63, 3],
            &no_reads
        ));
        assert!(!gate_passes(
            Gate::GrowthAboveBase,
            Family::Cactus,
            [5, 65, 5],
            &no_reads
        ));
    }

    #[test]
    fn leave_alone_rules_are_the_originals() {
        // Bone meal, in either hand.
        assert!(leave_alone(
            Family::Wheat,
            Some("minecraft:bone_meal"),
            None,
            false
        ));
        assert!(leave_alone(
            Family::Wheat,
            None,
            Some("minecraft:bone_meal"),
            false
        ));
        // Crouching with an item; crouching empty-handed still
        // harvests.
        assert!(leave_alone(
            Family::Wheat,
            Some("minecraft:stone"),
            None,
            true
        ));
        assert!(leave_alone(
            Family::Wheat,
            None,
            Some("minecraft:stone"),
            true
        ));
        assert!(!leave_alone(Family::Wheat, None, None, true));
        // Shears carve a pumpkin; on a melon they mean nothing.
        assert!(leave_alone(
            Family::Pumpkin,
            Some("minecraft:shears"),
            None,
            false
        ));
        assert!(!leave_alone(
            Family::Melon,
            Some("minecraft:shears"),
            None,
            false
        ));
        // An ordinary click harvests.
        assert!(!leave_alone(
            Family::Wheat,
            Some("minecraft:wheat_seeds"),
            None,
            false
        ));
        assert!(!leave_alone(Family::Wheat, None, None, false));
    }

    #[test]
    fn payment_looks_at_the_hand_the_bar_the_offhand_then_the_bag() {
        let item = "minecraft:wheat_seeds";
        // Already in hand.
        assert_eq!(find_payment(&[stack(2, item)], 2, item), Payment::Selected);
        // Elsewhere on the bar.
        assert_eq!(
            find_payment(&[stack(0, "minecraft:dirt"), stack(5, item)], 0, item),
            Payment::Hotbar(5)
        );
        // In the offhand.
        assert_eq!(find_payment(&[stack(504, item)], 0, item), Payment::Offhand);
        // Only in the bag: paid last, with its slot (0.2.0) —
        // the first stack of it, in slot order.
        assert_eq!(find_payment(&[stack(20, item)], 0, item), Payment::Bag(20));
        assert_eq!(
            find_payment(&[stack(30, item), stack(12, item)], 0, item),
            Payment::Bag(12)
        );
        // The offhand still outranks the bag, and the bar
        // outranks both.
        assert_eq!(
            find_payment(&[stack(504, item), stack(20, item)], 0, item),
            Payment::Offhand
        );
        assert_eq!(
            find_payment(&[stack(5, item), stack(20, item)], 0, item),
            Payment::Hotbar(5)
        );
        assert_eq!(
            find_payment(&[stack(0, "minecraft:dirt")], 0, item),
            Payment::None
        );
        assert_eq!(find_payment(&[], 0, item), Payment::None);
    }

    #[test]
    fn replant_targets_the_soil_or_the_log() {
        // Wheat: the farmland below, its top face, the clicked
        // point's x/z.
        let (target, standing) = replant_target(
            Kind::Replant {
                item: "minecraft:wheat_seeds",
            },
            "minecraft:wheat[age=7]",
            [4, 64, 4],
            [0.25, 0.9, 0.75],
            Some("minecraft:farmland"),
        )
        .unwrap();
        assert_eq!(standing, [4, 64, 4]);
        assert_eq!((target.block_x, target.block_y, target.block_z), (4, 63, 4));
        assert_eq!(target.face, Face::Up);
        assert_eq!(
            (target.cursor_x, target.cursor_y, target.cursor_z),
            (0.25, 1.0, 0.75)
        );
        // Nether wart falls back to soul sand when no read named
        // the ground.
        let (target, _) = replant_target(
            Kind::Replant {
                item: "minecraft:nether_wart",
            },
            "minecraft:nether_wart[age=3]",
            [0, 64, 0],
            [0.5, 0.5, 0.5],
            None,
        )
        .unwrap();
        assert_eq!(target.block, "minecraft:soul_sand");
        // A pitcher crop clicked on its upper half replants
        // below its lower half.
        let (target, standing) = replant_target(
            Kind::Replant {
                item: "minecraft:pitcher_pod",
            },
            "minecraft:pitcher_crop[age=4,half=upper]",
            [1, 65, 1],
            [0.5, 0.5, 0.5],
            Some("minecraft:farmland"),
        )
        .unwrap();
        assert_eq!(standing, [1, 64, 1]);
        assert_eq!((target.block_x, target.block_y, target.block_z), (1, 63, 1));
        // Cocoa: the log behind the pod, the face it faced.
        let (target, _) = replant_target(
            Kind::Replant {
                item: "minecraft:cocoa_beans",
            },
            "minecraft:cocoa[age=2,facing=north]",
            [2, 65, 2],
            [0.3, 0.4, 0.9],
            Some("minecraft:jungle_log[axis=y]"),
        )
        .unwrap();
        assert_eq!((target.block_x, target.block_y, target.block_z), (2, 65, 3));
        assert_eq!(target.face, Face::North);
        assert_eq!(
            (target.cursor_x, target.cursor_y, target.cursor_z),
            (0.3, 0.4, 0.0)
        );
        // A non-replanting kind has no target.
        assert!(
            replant_target(Kind::TallUp, "minecraft:cactus", [0, 64, 0], [0.5; 3], None).is_none()
        );
    }
}

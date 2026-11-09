//! Accurate Block Placement for Kestrel: hold right click with a
//! block and every *new* spot you look at gets a block at once,
//! instead of vanilla's one block every four ticks (which skips
//! spots when you move fast and stacks extra blocks when you move
//! slowly).
//!
//! A behaviour port of Accurate Block Placement: Reborn (MIT) by
//! Clayborn, Flourick, schwar and KadTheHunter, through Lutziano888's
//! Horizon port — its rules file (`mixins/accurate_block_placement/
//! runtime.rs`) is the spec this file follows: the freshness test,
//! the four-tick wait after a press and the 10-degree turn that ends
//! it, building backwards, at most four placements a tick. The rules
//! below are that file's, reimplemented against the Kestrel SDK
//! (tick-domain sampling of `actions.target`, placement through
//! `actions.use-on-block`); this file shares no code with the
//! original beyond the rules themselves, which are its author's.
//! This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/accurate_block_placement>.
//!
//! Where the SDK cannot carry the original, the port says so
//! rather than imitating:
//!
//! - The original switches vanilla's own held-repeat off while it
//!   works. No SDK surface can subtract a click, only add one, so
//!   while the player holds still on one spot, vanilla's four-tick
//!   repeat still runs (the original places nothing more there).
//!   The fresh-target acceleration — the mod's point — is exact.
//! - The original asks the game whether the held item is a block
//!   item. The SDK has no such query, so the port learns it from
//!   the press itself: vanilla places the first block, and if the
//!   block that appears where it should is the held item, the item
//!   is a block item and the rest of the hold is accelerated. A
//!   hold that starts looking at air is accelerated from the first
//!   placement vanilla's own repeat makes. Nothing is ever fired
//!   for an item the game has not itself placed as a block.
//! - Sampling is per tick, not per frame: a target crossed and
//!   left inside one tick is not seen. Placements the mod queues
//!   are performed in the next tick's input step (the `actions`
//!   bargain), at most four a tick, as the original caps them.
//! - "Blocks that react to a click get vanilla's click" is, in
//!   the original, the game's own interaction flag. The SDK does
//!   not expose it, so the port keeps the list below — vanilla's
//!   doors, trapdoors, gates, buttons, chests, workstations and
//!   the rest whose own use would double up with the player's
//!   physical click. Scaffolding is on it, as the original names
//!   it: scaffolding on scaffolding climbs as in vanilla.
//! - "Allowed on servers" is enforced the way this catalog's
//!   other ports read the world: the SDK's world reads answer in
//!   your own worlds and not on a remote server, so where they
//!   stay silent the port treats the world as a server.

use horizon_mod_sdk::{
    ActionHand, ActionTarget, BlockTarget, Face, Mod, Registration, actions, chat, game, input,
    storage,
    ui::{self, ButtonWidget, Widget},
    world,
};

/// The newest API level this mod needs, asserted in the tests
/// below: `actions.target` and `actions.use-on-block` are level
/// 21 (`crates/horizon-mods/src/api_level.rs`); everything else
/// here is older.
#[cfg(test)]
const API_LEVEL: u32 = 21;

/// The settings' storage keys and the settings page's buttons —
/// the original's own key names.
const ENABLED_KEY: &str = "enabled";
const SERVERS_KEY: &str = "servers";
const ENABLED_BUTTON: u32 = 0;
const SERVERS_BUTTON: u32 = 1;

/// The toggle key: the original's name, unbound by default, as
/// in the original.
const TOGGLE_KEY: &str = "toggle";
const DEFAULT_TOGGLE_KEY: &str = "key.keyboard.unknown";

/// Said in chat when the toggle key turns it on or off: the
/// original's own words.
const ON: &str = "Accurate Block Placement: ON";
const OFF: &str = "Accurate Block Placement: OFF";

/// Vanilla's delay between repeated uses, in ticks
/// (`Minecraft.rightClickDelay`).
const COOLDOWN: u8 = 4;

/// Turning the view this far (degrees) since the first click
/// ends the wait.
const TURN: f32 = 10.0;

/// At most this many blocks go out in one tick, however many
/// fresh targets were seen.
const MAX_PER_TICK: usize = 4;

/// A stored flag ("true" / "false"); `default` when unreadable —
/// the original's storage format.
fn read_flag(key: &str, default: bool) -> bool {
    match storage::get(key) {
        Some(bytes) => match std::str::from_utf8(&bytes).ok().map(str::trim) {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        },
        None => default,
    }
}

fn store_flag(key: &str, value: bool) {
    let _ = storage::set(key, if value { b"true" } else { b"false" });
}

fn on_off(value: bool) -> &'static str {
    if value { "ON" } else { "OFF" }
}

/// The settings page's labels: the original's.
fn enabled_label(enabled: bool) -> String {
    format!("Accurate placement: {}", on_off(enabled))
}

fn servers_label(servers: bool) -> String {
    format!("Allowed on servers: {}", on_off(servers))
}

/// The id of a block state string: `minecraft:wheat[age=7]` is
/// `minecraft:wheat`.
fn base_id(state: &str) -> &str {
    state.split('[').next().unwrap_or(state)
}

/// The path of an item or block id: `minecraft:oak_door` is
/// `oak_door`.
fn path_of(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}

/// Whether a block reacts to a click, so the player's own click
/// is all it gets (see the module docs: the port's list standing
/// in for the game's interaction flag). Matched on the id's path,
/// so coloured and wooden/stone variants come with their family.
fn reacts_to_click(id: &str) -> bool {
    let path = path_of(id);
    if path == "scaffolding" || path == "shulker_box" {
        return true;
    }
    if path.ends_with("_door")
        || path.ends_with("_trapdoor")
        || path.ends_with("_fence_gate")
        || path.ends_with("_button")
        || path.ends_with("_bed")
        || path.ends_with("_sign")
        || path.ends_with("_shulker_box")
        || path.ends_with("_candle_cake")
        || path.starts_with("potted_")
    {
        return true;
    }
    matches!(
        path,
        "lever"
            | "note_block"
            | "jukebox"
            | "bell"
            | "beacon"
            | "enchanting_table"
            | "brewing_stand"
            | "crafting_table"
            | "cartography_table"
            | "loom"
            | "stonecutter"
            | "grindstone"
            | "smithing_table"
            | "anvil"
            | "chipped_anvil"
            | "damaged_anvil"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "chest"
            | "ender_chest"
            | "trapped_chest"
            | "barrel"
            | "dispenser"
            | "dropper"
            | "hopper"
            | "crafter"
            | "lectern"
            | "composter"
            | "flower_pot"
            | "cake"
            | "dragon_egg"
            | "chiseled_bookshelf"
            | "decorated_pot"
            | "respawn_anchor"
            | "command_block"
            | "chain_command_block"
            | "repeating_command_block"
            | "structure_block"
            | "jigsaw"
            | "daylight_detector"
            | "redstone_comparator"
            | "redstone_repeater"
    )
}

/// Whether a block placed on this target takes the target's own
/// place instead of going beside it: the grasses, ferns, seagrass
/// and veins vanilla lets a placement replace, and snow layers
/// thinner than a full block. Read from the state string, as the
/// original's hook read it from the game.
fn replaceable(state: &str) -> bool {
    let path = path_of(base_id(state));
    if matches!(
        path,
        "short_grass"
            | "tall_grass"
            | "fern"
            | "large_fern"
            | "dead_bush"
            | "seagrass"
            | "tall_seagrass"
            | "vine"
            | "glow_lichen"
            | "sculk_vein"
            | "hanging_roots"
            | "kelp"
            | "kelp_plant"
    ) {
        return true;
    }
    if path == "snow" {
        // `minecraft:snow[layers=3]`: layers 1 to 7 are replaced,
        // 8 is a full block and is not.
        if let Some(props) = state.split('[').nth(1) {
            for part in props.trim_end_matches(']').split(',') {
                if let Some(value) = part.trim().strip_prefix("layers=") {
                    return value.parse::<u32>().is_ok_and(|layers| layers < 8);
                }
            }
        }
        return true; // a bare `minecraft:snow` is one layer
    }
    false
}

/// A face as the unit vector the original's samples carry.
fn face_vec(face: Face) -> [i32; 3] {
    match face {
        Face::Down => [0, -1, 0],
        Face::Up => [0, 1, 0],
        Face::North => [0, 0, -1],
        Face::South => [0, 0, 1],
        Face::West => [-1, 0, 0],
        Face::East => [1, 0, 0],
    }
}

/// What the crosshair hits, in the original's sample shape, plus
/// the SDK target it came from (what `use-on-block` fires with).
#[derive(Clone, Debug)]
struct Sample {
    pos: [i32; 3],
    face: [i32; 3],
    replaceable: bool,
    target: BlockTarget,
}

/// Two samples are the same spot the same way when their block,
/// face and replaceability agree (the SDK's target record has no
/// equality of its own to derive from).
impl PartialEq for Sample {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos && self.face == other.face && self.replaceable == other.replaceable
    }
}

impl Sample {
    /// Where a block placed on this target goes.
    fn placement(&self) -> [i32; 3] {
        if self.replaceable {
            self.pos
        } else {
            [
                self.pos[0] + self.face[0],
                self.pos[1] + self.face[1],
                self.pos[2] + self.face[2],
            ]
        }
    }

    /// The axis the face points along (0 x, 1 y, 2 z).
    fn axis(&self) -> usize {
        self.face.iter().position(|&f| f != 0).unwrap_or(1)
    }
}

/// A crosshair target as a sample, or none when there is nothing
/// to place on: no block, an entity, or a block that reacts to a
/// click and so keeps vanilla's click alone.
fn sample_of(target: &ActionTarget) -> Option<Sample> {
    let ActionTarget::Block(block) = target else {
        return None;
    };
    if reacts_to_click(&block.block) {
        return None;
    }
    Some(Sample {
        pos: [block.block_x, block.block_y, block.block_z],
        face: face_vec(block.face),
        replaceable: replaceable(&block.block),
        target: block.clone(),
    })
}

/// Whether `t` is a fresh target for the machine, the player
/// standing at `player`: a block you weren't already looking at
/// and didn't just place — or the just-placed block, building
/// backwards (below).
fn fresh(m: &Machine, t: &Sample, player: [f64; 3]) -> bool {
    let unseen = m.last_seen != Some(t.pos) && m.last_placed != Some(t.pos);
    unseen || backwards(m, t, player)
}

/// Building toward yourself: the target is the block you just
/// placed, and you have stepped a full block back from where you
/// placed it.
fn backwards(m: &Machine, t: &Sample, player: [f64; 3]) -> bool {
    let (Some(placed), Some(then)) = (m.last_placed, m.last_player) else {
        return false;
    };
    if placed != t.pos {
        return false;
    }
    let a = t.axis();
    // The face's plane.
    let plane = f64::from(t.pos[a] + i32::from(t.face[a] > 0));
    (player[a] - then[a]).abs() >= 0.99 && (then[a] - plane).abs() < (player[a] - plane).abs()
}

/// `d` wrapped into -180..180 degrees.
fn angle(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

/// The placement rules, as a state machine apart from the SDK:
/// the wrapper feeds it one tick at a time and fires what `due`
/// returns. The original samples every frame; the port samples
/// every tick, the snapshot's own rate.
#[derive(Default)]
struct Machine {
    /// Whether a hold is under way.
    held: bool,
    /// The block looked at last sample while the key was held.
    last_seen: Option<[i32; 3]>,
    /// Where the last block went.
    last_placed: Option<[i32; 3]>,
    /// The player's position when it went (stepped one block
    /// along the face at each later placement, so rounding never
    /// drifts).
    last_player: Option<[f64; 3]>,
    /// Ticks left of the wait after the first click.
    wait: u8,
    /// The view (yaw, pitch) at the first click.
    press_view: (f32, f32),
    /// Fresh targets seen since the last tick, in order.
    queue: Vec<Sample>,
}

impl Machine {
    /// Forgets everything about the current hold.
    fn reset(&mut self) {
        *self = Machine::default();
    }

    /// One sample: `held` is whether the use key is held with a
    /// block to place (and the mod on); `target` what the
    /// crosshair hits; `player` the player's position and `view`
    /// its yaw and pitch.
    fn frame(&mut self, held: bool, target: Option<Sample>, player: [f64; 3], view: (f32, f32)) {
        if !held {
            self.held = false;
            self.queue.clear();
            return;
        }
        if !self.held {
            // A fresh press: vanilla places this one itself.
            self.reset();
            self.held = true;
            self.wait = COOLDOWN;
            self.press_view = view;
            if let Some(t) = target {
                self.last_seen = Some(t.pos);
                self.last_placed = Some(t.placement());
                self.last_player = Some(player);
            }
            return;
        }
        let Some(t) = target else {
            return;
        };
        if fresh(self, &t, player)
            && !self
                .queue
                .iter()
                .any(|q| q.pos == t.pos && q.face == t.face)
        {
            self.queue.push(t.clone());
        }
        self.last_seen = Some(t.pos);
    }

    /// One tick: the fresh targets to place now, oldest first.
    /// Empty while the first click's wait runs and the view
    /// hasn't turned; those wait for later.
    fn due(&mut self, view: (f32, f32)) -> Vec<Sample> {
        if !self.held {
            self.queue.clear();
            return Vec::new();
        }
        if self.wait > 0 {
            let (dy, dp) = (
                angle(view.0 - self.press_view.0).abs(),
                (view.1 - self.press_view.1).abs(),
            );
            if dy.hypot(dp) < TURN {
                self.wait -= 1;
                return Vec::new();
            }
            self.wait = 0;
        }
        let n = self.queue.len().min(MAX_PER_TICK);
        self.queue.drain(..n).collect()
    }

    /// Whether `t`, due this tick, is still fresh after the
    /// blocks placed before it this tick.
    fn still_fresh(&self, t: &Sample, player: [f64; 3]) -> bool {
        self.last_placed != Some(t.pos) || backwards(self, t, player)
    }

    /// A block went on `t`, at `at`, the player standing at
    /// `player`.
    fn placed(&mut self, t: &Sample, at: [i32; 3], player: [f64; 3]) {
        self.last_player = Some(match self.last_player {
            None => player,
            Some(then) => {
                // One block further along the face, from where the
                // last one went; the other axes where the player
                // is now.
                let a = t.axis();
                let mut p = player;
                p[a] = then[a] + f64::from(t.face[a]);
                p
            }
        });
        self.last_placed = Some(at);
    }
}

/// Whether the game has shown the held item to be a block item:
/// a block with the item's id now stands where the press's block
/// was to go — read from the world where reads answer, and from
/// the crosshair's target (which names the block it rests on)
/// anywhere.
fn placed_block_seen(
    dimension: &str,
    placed: [i32; 3],
    item_id: &str,
    target: Option<&ActionTarget>,
) -> bool {
    if let Some(state) = world::get_block(dimension, placed[0], placed[1], placed[2])
        && base_id(&state) == item_id
    {
        return true;
    }
    match target {
        Some(ActionTarget::Block(block)) => {
            [block.block_x, block.block_y, block.block_z] == placed
                && base_id(&block.block) == item_id
        }
        _ => false,
    }
}

/// What the port knows about the held item this hold: still
/// learning, shown to be a block item, or shown not to be (the
/// press's wait passed with nothing placed).
#[derive(Clone, Copy, PartialEq)]
enum Confirm {
    Unknown,
    BlockItem,
    NotBlockItem,
}

struct AccurateBlockPlacement {
    enabled: bool,
    servers: bool,
    machine: Machine,
    /// The (item id, slot) the current hold began with; a change
    /// restarts the hold, as vanilla restarts its delay.
    hold_item: Option<(String, u32)>,
    confirm: Confirm,
    /// The tick the current hold began on (the learning window).
    press_tick: u64,
}

impl AccurateBlockPlacement {
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: ENABLED_BUTTON,
                label: enabled_label(self.enabled),
            }),
            Widget::Button(ButtonWidget {
                id: SERVERS_BUTTON,
                label: servers_label(self.servers),
            }),
        ]);
    }

    /// Whether the mod may work in this world under the
    /// "Allowed on servers" setting: always in your own worlds —
    /// read the way this catalog's ports read them, by whether
    /// the SDK's world reads answer at the player's own feet.
    fn server_allowed(&self, dimension: &str, player: [f64; 3]) -> bool {
        if self.servers {
            return true;
        }
        world::get_block(
            dimension,
            player[0].floor() as i32,
            player[1].floor() as i32,
            player[2].floor() as i32,
        )
        .is_some()
    }
}

impl Mod for AccurateBlockPlacement {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(
            TOGGLE_KEY,
            "Toggle Accurate Block Placement",
            DEFAULT_TOGGLE_KEY,
        )?;
        let state = AccurateBlockPlacement {
            enabled: read_flag(ENABLED_KEY, true),
            servers: read_flag(SERVERS_KEY, true),
            machine: Machine::default(),
            hold_item: None,
            confirm: Confirm::Unknown,
            press_tick: 0,
        };
        Ok((state, Registration::new().on_tick()))
    }

    fn tick(&mut self, tick: u64) {
        for event in input::poll() {
            if let input::Event::KeyDown(k) = event
                && k == TOGGLE_KEY
            {
                self.enabled = !self.enabled;
                store_flag(ENABLED_KEY, self.enabled);
                chat::show(if self.enabled { ON } else { OFF });
            }
        }
        let Some(player) = game::get_player() else {
            self.machine.frame(false, None, [0.0; 3], (0.0, 0.0));
            self.hold_item = None;
            return;
        };
        let pos = [player.x, player.y, player.z];
        let view = (player.yaw, player.pitch);
        let use_held = input::mapping("key.use").is_some_and(|m| m.down);
        let held = input::held();
        let active = self.enabled
            && use_held
            && held.is_some()
            && self.server_allowed(&player.dimension, pos);
        // A new hold, or the held item changed under the button:
        // the machine starts over, as vanilla's delay does.
        let item_now = held.as_ref().map(|h| (h.id.clone(), h.slot));
        if active && self.hold_item != item_now {
            self.machine.reset();
            self.confirm = Confirm::Unknown;
            self.press_tick = tick;
        }
        if !active {
            self.hold_item = None;
            self.confirm = Confirm::Unknown;
        } else {
            self.hold_item = item_now;
        }
        let target = actions::target();
        let sample = target.as_ref().and_then(sample_of);
        self.machine.frame(active, sample, pos, view);
        if !active {
            return;
        }
        // Learning whether the held item is a block item, from
        // the press's own placement (see the module docs).
        if self.confirm == Confirm::Unknown {
            let learned = match (self.machine.last_placed, &self.hold_item) {
                (Some(placed), Some((id, _))) => {
                    placed_block_seen(&player.dimension, placed, id, target.as_ref())
                }
                _ => false,
            };
            if learned {
                self.confirm = Confirm::BlockItem;
            } else if tick > self.press_tick + u64::from(COOLDOWN) {
                self.confirm = Confirm::NotBlockItem;
            }
        }
        let due = self.machine.due(view);
        if self.confirm != Confirm::BlockItem {
            // Not (yet) known to be a block item: vanilla's own
            // click and repeat are all that happen, as the
            // original does for a non-block item.
            return;
        }
        for t in due {
            if self.machine.still_fresh(&t, pos) {
                let at = t.placement();
                let _ = actions::use_on_block(&t.target, ActionHand::Main);
                self.machine.placed(&t, at, pos);
            }
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            ENABLED_BUTTON => {
                self.enabled = !self.enabled;
                store_flag(ENABLED_KEY, self.enabled);
            }
            SERVERS_BUTTON => {
                self.servers = !self.servers;
                store_flag(SERVERS_KEY, self.servers);
            }
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(AccurateBlockPlacement);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_21() {
        assert_eq!(API_LEVEL, 21);
    }

    /// A sample on the top of the block at `pos`.
    fn top(pos: [i32; 3]) -> Sample {
        Sample {
            pos,
            face: [0, 1, 0],
            replaceable: false,
            target: BlockTarget {
                block_x: pos[0],
                block_y: pos[1],
                block_z: pos[2],
                block: "minecraft:stone".into(),
                face: Face::Up,
                cursor_x: 0.5,
                cursor_y: 1.0,
                cursor_z: 0.5,
            },
        }
    }

    const STILL: (f32, f32) = (0.0, 60.0);
    const AT: [f64; 3] = [0.5, 64.0, 0.5];

    /// Ticks until the first click's wait is over.
    fn wait_out(m: &mut Machine) {
        for _ in 0..COOLDOWN {
            assert!(m.due(STILL).is_empty());
        }
    }

    #[test]
    fn settings_labels_are_the_originals() {
        assert_eq!(enabled_label(true), "Accurate placement: ON");
        assert_eq!(enabled_label(false), "Accurate placement: OFF");
        assert_eq!(servers_label(true), "Allowed on servers: ON");
        assert_eq!(servers_label(false), "Allowed on servers: OFF");
    }

    #[test]
    fn placement_goes_beside_or_replaces() {
        assert_eq!(top([1, 2, 3]).placement(), [1, 3, 3]);
        let grass = Sample {
            replaceable: true,
            ..top([1, 2, 3])
        };
        assert_eq!(grass.placement(), [1, 2, 3]);
    }

    #[test]
    fn one_block_per_fresh_target_none_while_staring() {
        let mut m = Machine::default();
        // The press: vanilla places on [0,63,0]'s top.
        m.frame(true, Some(top([0, 63, 0])), AT, STILL);
        // Staring at the same spot: nothing, during and after
        // the wait.
        for _ in 0..10 {
            m.frame(true, Some(top([0, 63, 0])), AT, STILL);
            assert!(m.due(STILL).is_empty());
        }
        // Looking at the block vanilla just placed isn't fresh
        // either.
        m.frame(true, Some(top([0, 64, 0])), AT, STILL);
        assert!(m.due(STILL).is_empty());
        // A new spot is placed at once, and only once.
        m.frame(true, Some(top([1, 63, 0])), AT, STILL);
        m.frame(true, Some(top([1, 63, 0])), AT, STILL);
        let now = m.due(STILL);
        assert_eq!(now, vec![top([1, 63, 0])]);
        assert!(m.still_fresh(&now[0], AT));
        m.placed(&now[0], now[0].placement(), AT);
        m.frame(true, Some(top([1, 63, 0])), AT, STILL);
        assert!(m.due(STILL).is_empty());
        // Letting go forgets the hold.
        m.frame(false, Some(top([2, 63, 0])), AT, STILL);
        assert!(m.due(STILL).is_empty());
    }

    #[test]
    fn a_fast_sweep_loses_no_spot() {
        let mut m = Machine::default();
        m.frame(true, Some(top([0, 63, 0])), AT, STILL);
        wait_out(&mut m);
        // Five spots between two ticks.
        for x in 1..=5 {
            m.frame(true, Some(top([x, 63, 0])), AT, STILL);
        }
        let first = m.due(STILL);
        assert_eq!(first.len(), MAX_PER_TICK);
        assert_eq!(first[0].pos, [1, 63, 0]);
        // The rest goes on the next tick.
        assert_eq!(
            m.due(STILL).iter().map(|t| t.pos).collect::<Vec<_>>(),
            [[5, 63, 0]]
        );
    }

    #[test]
    fn the_wait_keeps_a_click_single_unless_the_view_turns() {
        let mut m = Machine::default();
        m.frame(true, Some(top([0, 63, 0])), AT, STILL);
        m.frame(true, Some(top([1, 63, 0])), AT, STILL);
        // Waiting: nothing yet, but it isn't forgotten.
        for _ in 0..COOLDOWN {
            assert!(m.due(STILL).is_empty());
        }
        assert_eq!(m.due(STILL).len(), 1, "placed once the wait is over");
        // Turning the view ends the wait at once.
        let mut m = Machine::default();
        m.frame(true, Some(top([0, 63, 0])), AT, STILL);
        m.frame(true, Some(top([1, 63, 0])), AT, (15.0, 60.0));
        assert_eq!(m.due((15.0, 60.0)).len(), 1);
    }

    #[test]
    fn building_backwards_needs_a_full_step_back() {
        let mut m = Machine::default();
        // Facing the east side (+x face) of the block at x 0,
        // standing east of it.
        let side = |x: i32| Sample {
            pos: [x, 63, 0],
            face: [1, 0, 0],
            replaceable: false,
            target: BlockTarget {
                block_x: x,
                block_y: 63,
                block_z: 0,
                block: "minecraft:stone".into(),
                face: Face::East,
                cursor_x: 1.0,
                cursor_y: 0.5,
                cursor_z: 0.5,
            },
        };
        m.frame(true, Some(side(0)), [2.5, 64.0, 0.5], STILL);
        wait_out(&mut m);
        // The press placed at x 1; now the crosshair is on it.
        m.frame(true, Some(side(1)), [2.6, 64.0, 0.5], STILL);
        assert!(m.due(STILL).is_empty(), "not before a step back");
        // A full block further east: fresh again.
        m.frame(true, Some(side(1)), [3.55, 64.0, 0.5], STILL);
        let now = m.due(STILL);
        assert_eq!(now, vec![side(1)]);
        m.placed(&now[0], now[0].placement(), [3.55, 64.0, 0.5]);
        // And the next one a block further still.
        m.frame(true, Some(side(2)), [3.9, 64.0, 0.5], STILL);
        assert!(m.due(STILL).is_empty());
        m.frame(true, Some(side(2)), [4.6, 64.0, 0.5], STILL);
        assert_eq!(m.due(STILL), vec![side(2)]);
    }

    #[test]
    fn a_target_used_this_tick_is_not_used_again() {
        let mut m = Machine::default();
        m.frame(true, Some(top([0, 63, 0])), AT, STILL);
        wait_out(&mut m);
        m.frame(true, Some(top([1, 63, 0])), AT, STILL);
        let side = Sample {
            face: [0, 0, 1],
            ..top([1, 63, 0])
        };
        m.frame(true, Some(side), AT, STILL);
        // Two faces of one block were seen; the first placement
        // makes the second stale only if it went there.
        let now = m.due(STILL);
        assert_eq!(now.len(), 1, "one entry per block looked at: {now:?}");
        m.placed(&now[0], [1, 63, 0], AT);
        assert!(!m.still_fresh(&top([1, 63, 0]), AT));
    }

    #[test]
    fn angles_wrap() {
        assert_eq!(angle(350.0), -10.0);
        assert_eq!(angle(-350.0), 10.0);
    }

    #[test]
    fn reacting_blocks_keep_vanillas_click() {
        for id in [
            "minecraft:oak_door",
            "minecraft:iron_trapdoor",
            "minecraft:oak_fence_gate",
            "minecraft:stone_button",
            "minecraft:chest",
            "minecraft:crafting_table",
            "minecraft:white_shulker_box",
            "minecraft:shulker_box",
            "minecraft:scaffolding",
            "minecraft:lever",
            "minecraft:potted_oak_sapling",
            "minecraft:white_bed",
            "minecraft:oak_hanging_sign",
            "minecraft:decorated_pot",
            "minecraft:composter",
        ] {
            assert!(reacts_to_click(id), "{id} should react");
        }
        for id in [
            "minecraft:stone",
            "minecraft:dirt",
            "minecraft:oak_planks",
            "minecraft:glass",
            "minecraft:tnt",
            "minecraft:cauldron",
            "minecraft:farmland",
            "minecraft:wheat",
        ] {
            assert!(!reacts_to_click(id), "{id} should not react");
        }
    }

    #[test]
    fn replaceable_blocks_take_the_targets_place() {
        for state in [
            "minecraft:short_grass",
            "minecraft:tall_grass",
            "minecraft:fern",
            "minecraft:seagrass",
            "minecraft:vine",
            "minecraft:glow_lichen",
            "minecraft:snow[layers=1]",
            "minecraft:snow[layers=7]",
            "minecraft:snow",
        ] {
            assert!(replaceable(state), "{state} should be replaceable");
        }
        for state in [
            "minecraft:snow[layers=8]",
            "minecraft:stone",
            "minecraft:oak_leaves",
        ] {
            assert!(!replaceable(state), "{state} should not be replaceable");
        }
    }
}

//! SmartF3 for Kestrel: a tidier, configurable F3 debug screen — the same
//! information grouped into modules you can turn off, in one panel in the
//! top-left corner. `/smartf3` hides or shows the panel, and Options > Mods
//! > SmartF3 has a switch per module; the game keeps the choices in the
//! mod's storage.
//!
//! Behaviour reimplemented from SmartF3 by d3grando, MIT, source
//! <https://github.com/d3grando/smartf3>; this file shares no code with
//! the original. The original groups the same information into 14 modules
//! (FPS, server, coordinates, chunk, facing, world, biome, light, chart
//! keys, system, rendering, targeted block, fluid, entity); this port
//! shows the six the SDK exposes — FPS (`render.frame-stats`), the
//! coordinates, facing and world (dimension and clock) from `game-read`,
//! the targeted block from `game.raycast` and the targeted entity from
//! `entity.targeted` — plus the player's health and food, which
//! `game-read` exposes alongside the position — and lists the rest as
//! missing in the README, with the reason each one is out of reach. This port's
//! home is <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/smartf3>.
use horizon_mod_sdk::{
    Mod, Registration,
    entity::{self, Target},
    game::{self, Clock, Hit, Player},
    hud::{self, Corner, Line},
    recipes, render,
    render::FrameRate,
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `entity.targeted` is level 10, the newest function it imports
/// (`recipes.items` is level 5, `game.raycast` level 2,
/// `render.frame-stats` level 0; `crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 10;

/// The storage key for whether the panel shows.
const SHOWN_KEY: &str = "shown";

/// The storage keys for the seven module switches.
const MOD_FPS_KEY: &str = "mod_fps";
const MOD_COORDS_KEY: &str = "mod_coords";
const MOD_FACING_KEY: &str = "mod_facing";
const MOD_WORLD_KEY: &str = "mod_world";
const MOD_PLAYER_KEY: &str = "mod_player";
const MOD_BLOCK_KEY: &str = "mod_block";
const MOD_ENTITY_KEY: &str = "mod_entity";

/// The settings page's buttons: show the panel, then one per module.
const SHOWN_BUTTON: u32 = 0;
const FPS_BUTTON: u32 = 1;
const COORDS_BUTTON: u32 = 2;
const FACING_BUTTON: u32 = 3;
const WORLD_BUTTON: u32 = 4;
const PLAYER_BUTTON: u32 = 5;
const BLOCK_BUTTON: u32 = 6;
const ENTITY_BUTTON: u32 = 7;

/// How far the crosshair reaches for the targeted block and entity, in
/// blocks: far enough to name what the player looks at, not just what
/// they can touch.
const REACH: f32 = 64.0;

/// Which modules the panel shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Modules {
    fps: bool,
    coords: bool,
    facing: bool,
    world: bool,
    player: bool,
    block: bool,
    entity: bool,
}

impl Modules {
    #[cfg(test)]
    fn all_on() -> Self {
        Modules {
            fps: true,
            coords: true,
            facing: true,
            world: true,
            player: true,
            block: true,
            entity: true,
        }
    }
}

struct SmartF3 {
    shown: bool,
    modules: Modules,
    /// The item table's id-to-name pairs, loaded once on the first tick
    /// with a hit (`recipes.items` in one call, as JEI loads it) and kept.
    names: Option<Vec<(String, String)>>,
    /// The panel last sent, so an unchanged one costs no call.
    last: Vec<String>,
}

/// Direction.fromYRot: yaw 0 faces south, 90 west.
fn facing(yaw: f32) -> &'static str {
    let i = (yaw / 90.0 + 0.5).floor() as i32 & 3;
    ["South (+Z)", "West (-X)", "North (-Z)", "East (+X)"][i as usize]
}

/// `Day 3, 14:05`: day time 0 is 6:00 on day 1.
fn time_of_day(clock: &Clock) -> String {
    let day = clock.day_time / 24000 + 1;
    let ticks = clock.day_time % 24000;
    let hours = (ticks / 1000 + 6) % 24;
    let minutes = ticks % 1000 * 60 / 1000;
    format!("Day {day}, {hours:02}:{minutes:02}")
}

/// The block id with the words capitalised: `minecraft:grass_block` is
/// "Grass Block", `minecraft:water` is "Water".
fn pretty_id(id: &str) -> String {
    let path = id.split(':').next_back().unwrap_or(id);
    let mut out = String::with_capacity(path.len());
    for (i, word) in path.split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        match chars.next() {
            Some(first) => {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
            None => out.push_str(word),
        }
    }
    out
}

/// The distance from the player's feet to the point the crosshair meets
/// the block, as Blockscope measures it.
fn distance_from_feet(player: &Player, hit: &Hit) -> f64 {
    let (dx, dy, dz) = (hit.x - player.x, hit.y - player.y, hit.z - player.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// The FPS module's line: frames in the last second, the slowest frame
/// and the 1% low of the last 5 seconds, as the FPS counter shows them.
fn fps_line(frame: &FrameRate) -> String {
    format!("{} fps  min {}  1% {}", frame.fps, frame.min, frame.low)
}

/// The targeted block's lines: the name, the id, and the distance with
/// the block's coordinates; one line when nothing is in reach.
fn block_lines(name: &str, hit: Option<&Hit>, player: &Player) -> Vec<String> {
    match hit {
        Some(hit) => vec![
            name.to_string(),
            hit.block.clone(),
            format!(
                "Distance {:.1}  at {}, {}, {}",
                distance_from_feet(player, hit),
                hit.block_x,
                hit.block_y,
                hit.block_z
            ),
        ],
        None => vec!["Target block: none".to_string()],
    }
}

/// The targeted entity's line: name, kind, distance and health — or, in
/// multiplayer, where health reads as none, name, kind and distance with
/// no health at all; one line when nothing is under the crosshair.
fn entity_line(target: Option<&Target>) -> String {
    match target {
        Some(t) => match t.entity.health {
            Some(health) => format!(
                "Target: {} ({}) {:.1} blocks {:.1}/{:.1}",
                t.entity.name, t.entity.kind, t.distance, health, t.entity.max_health
            ),
            None => format!(
                "Target: {} ({}) {:.1} blocks",
                t.entity.name, t.entity.kind, t.distance
            ),
        },
        None => "Target: none".to_string(),
    }
}

/// The panel's lines for one tick's readings, in module order.
#[allow(clippy::too_many_arguments)]
fn build_lines(
    player: &Player,
    clock: Option<&Clock>,
    frame: &FrameRate,
    hit: Option<&Hit>,
    block_name: &str,
    target: Option<&Target>,
    modules: Modules,
) -> Vec<String> {
    let mut lines = Vec::new();
    if modules.fps {
        lines.push(fps_line(frame));
    }
    if modules.coords {
        lines.push(format!(
            "XYZ {:.1} / {:.1} / {:.1}",
            player.x, player.y, player.z
        ));
    }
    if modules.facing {
        lines.push(format!(
            "Facing {}  yaw {:.1}  pitch {:.1}",
            facing(player.yaw),
            player.yaw,
            player.pitch
        ));
    }
    if modules.world {
        lines.push(format!("Dimension {}", player.dimension));
        if let Some(clock) = clock {
            lines.push(time_of_day(clock));
        }
    }
    if modules.player {
        lines.push(format!("Health {:.1}/20.0", player.health));
        lines.push(format!("Food {}", player.food));
    }
    if modules.block {
        lines.extend(block_lines(block_name, hit, player));
    }
    if modules.entity {
        lines.push(entity_line(target));
    }
    lines
}

fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

fn read_modules() -> Modules {
    Modules {
        fps: read_bool(storage::get(MOD_FPS_KEY), true),
        coords: read_bool(storage::get(MOD_COORDS_KEY), true),
        facing: read_bool(storage::get(MOD_FACING_KEY), true),
        world: read_bool(storage::get(MOD_WORLD_KEY), true),
        player: read_bool(storage::get(MOD_PLAYER_KEY), true),
        block: read_bool(storage::get(MOD_BLOCK_KEY), true),
        entity: read_bool(storage::get(MOD_ENTITY_KEY), true),
    }
}

fn shown_label(shown: bool) -> String {
    format!("Shown: {}", if shown { "ON" } else { "OFF" })
}

fn module_label(name: &str, on: bool) -> String {
    format!("{name}: {}", if on { "ON" } else { "OFF" })
}

impl SmartF3 {
    fn name_for(&self, block: &str) -> String {
        self.names
            .as_ref()
            .and_then(|names| {
                names
                    .iter()
                    .find(|(id, _)| id == block)
                    .map(|(_, name)| name.clone())
            })
            .unwrap_or_else(|| pretty_id(block))
    }

    /// Loads the item table once, on the first tick with a hit.
    fn load_names(&mut self) {
        if self.names.is_none() {
            self.names = Some(
                recipes::items()
                    .into_iter()
                    .map(|i| (i.id, i.name))
                    .collect(),
            );
        }
    }

    fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
        let _ = storage::set(SHOWN_KEY, if shown { b"true" } else { b"false" });
        self.refresh_settings();
    }

    fn set_module(&mut self, key: &str, on: bool) {
        match key {
            MOD_FPS_KEY => self.modules.fps = on,
            MOD_COORDS_KEY => self.modules.coords = on,
            MOD_FACING_KEY => self.modules.facing = on,
            MOD_WORLD_KEY => self.modules.world = on,
            MOD_PLAYER_KEY => self.modules.player = on,
            MOD_BLOCK_KEY => self.modules.block = on,
            MOD_ENTITY_KEY => self.modules.entity = on,
            _ => {}
        }
        let _ = storage::set(key, if on { b"true" } else { b"false" });
        self.refresh_settings();
    }

    /// The settings page: one button for the panel, one per module.
    fn refresh_settings(&self) {
        let m = self.modules;
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: SHOWN_BUTTON,
                label: shown_label(self.shown),
            }),
            Widget::Button(ButtonWidget {
                id: FPS_BUTTON,
                label: module_label("FPS", m.fps),
            }),
            Widget::Button(ButtonWidget {
                id: COORDS_BUTTON,
                label: module_label("Coordinates", m.coords),
            }),
            Widget::Button(ButtonWidget {
                id: FACING_BUTTON,
                label: module_label("Facing", m.facing),
            }),
            Widget::Button(ButtonWidget {
                id: WORLD_BUTTON,
                label: module_label("World", m.world),
            }),
            Widget::Button(ButtonWidget {
                id: PLAYER_BUTTON,
                label: module_label("Player", m.player),
            }),
            Widget::Button(ButtonWidget {
                id: BLOCK_BUTTON,
                label: module_label("Target block", m.block),
            }),
            Widget::Button(ButtonWidget {
                id: ENTITY_BUTTON,
                label: module_label("Target entity", m.entity),
            }),
        ]);
    }

    fn clear_panel(&mut self) {
        if !self.last.is_empty() {
            hud::clear();
            self.last.clear();
        }
    }
}

impl Mod for SmartF3 {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("smartf3").on_tick();
        let state = SmartF3 {
            shown: read_bool(storage::get(SHOWN_KEY), true),
            modules: read_modules(),
            names: None,
            last: Vec::new(),
        };
        Ok((state, hooks))
    }

    fn tick(&mut self, _tick: u64) {
        let Some(p) = game::get_player().filter(|_| self.shown) else {
            self.clear_panel();
            return;
        };
        let hit = game::raycast(REACH);
        if hit.is_some() {
            self.load_names();
        }
        let block_name = hit
            .as_ref()
            .map(|h| self.name_for(&h.block))
            .unwrap_or_default();
        let now = build_lines(
            &p,
            game::get_clock().as_ref(),
            &render::frame_stats(),
            hit.as_ref(),
            &block_name,
            entity::targeted(REACH).as_ref(),
            self.modules,
        );
        if now != self.last {
            let panel: Vec<Line> = now
                .iter()
                .map(|text| Line {
                    text: text.clone(),
                    color: 0xFFFFFF,
                })
                .collect();
            hud::set_panel(Corner::TopLeft, &panel);
            self.last = now;
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "smartf3" {
            return Err("This command does nothing".into());
        }
        self.set_shown(!self.shown);
        let state = if self.shown { "shown" } else { "hidden" };
        Ok(Some(format!("SmartF3 panel {state}")))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SHOWN_BUTTON => self.set_shown(!self.shown),
            FPS_BUTTON => self.set_module(MOD_FPS_KEY, !self.modules.fps),
            COORDS_BUTTON => self.set_module(MOD_COORDS_KEY, !self.modules.coords),
            FACING_BUTTON => self.set_module(MOD_FACING_KEY, !self.modules.facing),
            WORLD_BUTTON => self.set_module(MOD_WORLD_KEY, !self.modules.world),
            PLAYER_BUTTON => self.set_module(MOD_PLAYER_KEY, !self.modules.player),
            BLOCK_BUTTON => self.set_module(MOD_BLOCK_KEY, !self.modules.block),
            ENTITY_BUTTON => self.set_module(MOD_ENTITY_KEY, !self.modules.entity),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(SmartF3);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::entity::Entity;

    fn player() -> Player {
        Player {
            x: 0.5,
            y: 64.0,
            z: 0.5,
            yaw: 0.0,
            pitch: 0.0,
            health: 20.0,
            food: 20,
            dimension: "minecraft:overworld".into(),
        }
    }

    fn clock(day_time: u64) -> Clock {
        Clock {
            game_time: 0,
            day_time,
        }
    }

    fn frame() -> FrameRate {
        FrameRate {
            fps: 120,
            min: 41,
            low: 52,
        }
    }

    fn hit() -> Hit {
        Hit {
            block_x: 12,
            block_y: 63,
            block_z: -44,
            x: 12.5,
            y: 64.0,
            z: -43.75,
            block: "minecraft:grass_block".into(),
        }
    }

    fn mob(name: &str, health: Option<f32>) -> Entity {
        Entity {
            id: 1,
            kind: "minecraft:zombie".into(),
            name: name.into(),
            x: 0.5,
            y: 64.0,
            z: 5.5,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            yaw: 0.0,
            health,
            max_health: 20.0,
            width: 0.6,
            height: 1.95,
            on_ground: true,
            custom_name: None,
            baby: false,
        }
    }

    fn target(health: Option<f32>) -> Target {
        Target {
            entity: mob("Zombie", health),
            distance: 4.7,
        }
    }

    #[test]
    fn api_level_is_ten() {
        // Newest imported function is `entity.targeted` (level 10);
        // `recipes.items` is level 5, `game.raycast` level 2,
        // `render.frame-stats` level 0, the rest level 0.
        assert_eq!(API_LEVEL, 10);
    }

    #[test]
    fn facing_follows_vanilla_yaw() {
        assert_eq!(facing(0.0), "South (+Z)");
        assert_eq!(facing(90.0), "West (-X)");
        assert_eq!(facing(-180.0), "North (-Z)");
        assert_eq!(facing(-90.0), "East (+X)");
        assert_eq!(facing(314.0), "East (+X)");
    }

    #[test]
    fn day_time_reads_as_a_clock() {
        assert_eq!(time_of_day(&clock(0)), "Day 1, 06:00");
        assert_eq!(time_of_day(&clock(18000)), "Day 1, 00:00");
        assert_eq!(time_of_day(&clock(24000 * 2 + 8083)), "Day 3, 14:04");
    }

    #[test]
    fn fps_line_shows_fps_min_and_low() {
        assert_eq!(fps_line(&frame()), "120 fps  min 41  1% 52");
        assert_eq!(
            fps_line(&FrameRate {
                fps: 0,
                min: 0,
                low: 0
            }),
            "0 fps  min 0  1% 0"
        );
    }

    #[test]
    fn id_prettifies_to_the_english_name() {
        assert_eq!(pretty_id("minecraft:grass_block"), "Grass Block");
        assert_eq!(pretty_id("minecraft:water"), "Water");
        assert_eq!(pretty_id("minecraft:oak_planks"), "Oak Planks");
        assert_eq!(pretty_id("somemod:cool_block"), "Cool Block");
        assert_eq!(pretty_id("stone"), "Stone");
    }

    #[test]
    fn distance_is_measured_from_the_feet() {
        // The far hit is 40.5 straight along -Z from the feet.
        let p = Player {
            x: 12.5,
            y: 64.0,
            z: -3.25,
            ..player()
        };
        assert!((distance_from_feet(&p, &hit()) - 40.5).abs() < 0.001);
    }

    #[test]
    fn block_lines_show_name_id_distance_and_coords() {
        // The player stands 40.5 blocks north of the hit, as in the
        // distance test above.
        let p = Player {
            x: 12.5,
            y: 64.0,
            z: -3.25,
            ..player()
        };
        assert_eq!(
            block_lines("Grass Block", Some(&hit()), &p),
            [
                "Grass Block",
                "minecraft:grass_block",
                "Distance 40.5  at 12, 63, -44",
            ]
        );
        assert_eq!(block_lines("", None, &p), ["Target block: none"]);
    }

    #[test]
    fn entity_line_shows_health_when_the_client_can_vouch_for_it() {
        assert_eq!(
            entity_line(Some(&target(Some(14.0)))),
            "Target: Zombie (minecraft:zombie) 4.7 blocks 14.0/20.0"
        );
        // Multiplayer: health reads as none, so the line carries no
        // health at all — not even a `?`.
        assert_eq!(
            entity_line(Some(&target(None))),
            "Target: Zombie (minecraft:zombie) 4.7 blocks"
        );
        assert_eq!(entity_line(None), "Target: none");
    }

    #[test]
    fn all_modules_on_build_the_full_panel() {
        let lines = build_lines(
            &player(),
            Some(&clock(0)),
            &frame(),
            Some(&hit()),
            "Grass Block",
            Some(&target(Some(14.0))),
            Modules::all_on(),
        );
        assert_eq!(
            lines,
            [
                "120 fps  min 41  1% 52",
                "XYZ 0.5 / 64.0 / 0.5",
                "Facing South (+Z)  yaw 0.0  pitch 0.0",
                "Dimension minecraft:overworld",
                "Day 1, 06:00",
                "Health 20.0/20.0",
                "Food 20",
                "Grass Block",
                "minecraft:grass_block",
                "Distance 45.8  at 12, 63, -44",
                "Target: Zombie (minecraft:zombie) 4.7 blocks 14.0/20.0",
            ]
        );
        // Eleven lines: inside the HUD panel's 16-line cap.
        assert!(lines.len() <= 16);
    }

    #[test]
    fn modules_off_leave_their_lines_out() {
        let off = Modules {
            fps: false,
            coords: false,
            facing: false,
            world: false,
            player: false,
            block: false,
            entity: false,
        };
        assert!(
            build_lines(
                &player(),
                Some(&clock(0)),
                &frame(),
                Some(&hit()),
                "Grass Block",
                Some(&target(Some(14.0))),
                off,
            )
            .is_empty()
        );
        // Only FPS and the targeted entity: two lines, in module order.
        let some = Modules {
            fps: true,
            coords: false,
            facing: false,
            world: false,
            player: false,
            block: false,
            entity: true,
        };
        assert_eq!(
            build_lines(
                &player(),
                Some(&clock(0)),
                &frame(),
                None,
                "",
                Some(&target(None)),
                some,
            ),
            [
                "120 fps  min 41  1% 52",
                "Target: Zombie (minecraft:zombie) 4.7 blocks",
            ]
        );
        // No clock (title screen edge): the world module shows the
        // dimension but no clock line.
        let world_only = Modules { world: true, ..off };
        assert_eq!(
            build_lines(&player(), None, &frame(), None, "", None, world_only),
            ["Dimension minecraft:overworld"]
        );
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert!(read_bool(Some(b"true".to_vec()), false));
        assert_eq!(shown_label(true), "Shown: ON");
        assert_eq!(shown_label(false), "Shown: OFF");
        assert_eq!(module_label("FPS", true), "FPS: ON");
        assert_eq!(module_label("Target entity", false), "Target entity: OFF");
    }
}

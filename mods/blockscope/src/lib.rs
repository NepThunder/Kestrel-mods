//! Blockscope for Kestrel: the block under the crosshair — its name in your
//! language, its block id, its distance and its coordinates — in a small box
//! in the top-right corner. Nothing shows when you look at no block within
//! reach. `/blockscope` hides or shows the box, and Options > Mods >
//! Blockscope has the same switch, a coordinates switch and a reach switch
//! (5, 20, 64 or 128 blocks); the game keeps the choices in the mod's storage.
//!
//! Behaviour reimplemented from Blockscope by Domanffe (Alexander), MIT,
//! Copyright (c) 2026 Alexander (Domanffe); this file shares no code with
//! the original. The name comes from the game's item table
//! (`recipes.items`), as the original reads it from the game's own tables,
//! and a block with no item falls back to its id with the words capitalised
//! ("Water"), as the original does. The distance is measured from the
//! player's feet to the point the crosshair meets the block, as the
//! original measures it. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/blockscope>.
//!
//! What the SDK cannot do, the original's native build can: hardness and
//! the tool it wants, block states, crop growth, a container's slots and a
//! creature's name and health are not exposed to a sandboxed mod, so this
//! port shows none of them. The box is a text panel anchored to a corner —
//! it cannot draw the block's icon or sit beside the crosshair — so it
//! lives in the top-right corner, where the original's screenshot puts it.
use horizon_mod_sdk::{
    Mod, Registration, game,
    game::Hit,
    hud::{self, Corner, Line},
    recipes, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `recipes.items` is level 5, the newest function it imports
/// (`game.raycast` is level 2, the rest level 0;
/// `crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 5;

/// The storage keys for the three settings.
const SHOWN_KEY: &str = "shown";
const COORDS_KEY: &str = "coords";
const REACH_KEY: &str = "reach";

/// The settings page's buttons: show the box, show coordinates, reach.
const SHOWN_BUTTON: u32 = 0;
const COORDS_BUTTON: u32 = 1;
const REACH_BUTTON: u32 = 2;

/// The reaches the switch cycles through, in blocks; 5 is vanilla reach.
const REACHES: [f32; 4] = [5.0, 20.0, 64.0, 128.0];

struct Blockscope {
    shown: bool,
    show_coords: bool,
    reach: f32,
    /// The item table's id-to-name pairs, loaded once on the first tick
    /// with a hit (`recipes.items` in one call, as JEI loads it) and kept.
    names: Option<Vec<(String, String)>>,
    /// The panel last sent, so an unchanged one costs no call.
    last: Vec<String>,
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

/// The distance from the player's feet to the point the crosshair meets the
/// block, as the original measures it.
fn distance_from_feet(player: &game::Player, hit: &Hit) -> f64 {
    let (dx, dy, dz) = (hit.x - player.x, hit.y - player.y, hit.z - player.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// The box's lines for a hit: the name, the id, the distance, and the
/// coordinates when the switch is on.
fn lines(name: &str, hit: &Hit, distance: f64, show_coords: bool) -> Vec<String> {
    let mut lines = vec![
        name.to_string(),
        hit.block.clone(),
        format!("Distance {distance:.1}"),
    ];
    if show_coords {
        lines.push(format!("{}, {}, {}", hit.block_x, hit.block_y, hit.block_z));
    }
    lines
}

fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

fn read_reach(stored: Option<Vec<u8>>) -> f32 {
    let wanted = stored
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(REACHES[0]);
    if REACHES.contains(&wanted) {
        wanted
    } else {
        REACHES[0]
    }
}

fn shown_label(shown: bool) -> String {
    format!("Shown: {}", if shown { "ON" } else { "OFF" })
}

fn coords_label(show_coords: bool) -> String {
    format!("Coordinates: {}", if show_coords { "ON" } else { "OFF" })
}

fn reach_label(reach: f32) -> String {
    format!("Reach: {} blocks", reach as u32)
}

/// The next reach in the switch's cycle, wrapping back to the first.
fn next_reach(reach: f32) -> f32 {
    let i = REACHES.iter().position(|r| *r == reach).unwrap_or(0);
    REACHES[(i + 1) % REACHES.len()]
}

impl Blockscope {
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

    fn set_coords(&mut self, show_coords: bool) {
        self.show_coords = show_coords;
        let _ = storage::set(COORDS_KEY, if show_coords { b"true" } else { b"false" });
        self.refresh_settings();
    }

    fn cycle_reach(&mut self) {
        self.reach = next_reach(self.reach);
        let _ = storage::set(REACH_KEY, self.reach.to_string().as_bytes());
        self.refresh_settings();
    }

    /// The settings page: one button per setting, cycling its label.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: SHOWN_BUTTON,
                label: shown_label(self.shown),
            }),
            Widget::Button(ButtonWidget {
                id: COORDS_BUTTON,
                label: coords_label(self.show_coords),
            }),
            Widget::Button(ButtonWidget {
                id: REACH_BUTTON,
                label: reach_label(self.reach),
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

impl Mod for Blockscope {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("blockscope").on_tick();
        let state = Blockscope {
            shown: read_bool(storage::get(SHOWN_KEY), true),
            show_coords: read_bool(storage::get(COORDS_KEY), true),
            reach: read_reach(storage::get(REACH_KEY)),
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
        let Some(hit) = game::raycast(self.reach) else {
            self.clear_panel();
            return;
        };
        self.load_names();
        let name = self.name_for(&hit.block);
        let now = lines(&name, &hit, distance_from_feet(&p, &hit), self.show_coords);
        if now != self.last {
            let panel: Vec<Line> = now
                .iter()
                .map(|text| Line {
                    text: text.clone(),
                    color: 0xFFFFFF,
                })
                .collect();
            hud::set_panel(Corner::TopRight, &panel);
            self.last = now;
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "blockscope" {
            return Err("This command does nothing".into());
        }
        self.set_shown(!self.shown);
        let state = if self.shown { "shown" } else { "hidden" };
        Ok(Some(format!("Blockscope panel {state}")))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SHOWN_BUTTON => self.set_shown(!self.shown),
            COORDS_BUTTON => self.set_coords(!self.show_coords),
            REACH_BUTTON => self.cycle_reach(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(Blockscope);

#[cfg(test)]
mod tests {
    use super::*;

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

    fn player() -> game::Player {
        game::Player {
            x: 12.5,
            y: 64.0,
            z: -3.25,
            yaw: 180.0,
            pitch: 0.0,
            health: 20.0,
            food: 20,
            dimension: "minecraft:overworld".into(),
        }
    }

    #[test]
    fn api_level_is_five() {
        // Newest imported function is `recipes.items` (level 5);
        // `game.raycast` is level 2, the rest level 0.
        assert_eq!(API_LEVEL, 5);
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
        assert!((distance_from_feet(&player(), &hit()) - 40.5).abs() < 0.001);
        // Half a block down and half a block along -Z: sqrt(0.5^2 + 0.5^2).
        let diagonal = Hit {
            block_x: 12,
            block_y: 63,
            block_z: -4,
            x: 12.5,
            y: 63.5,
            z: -3.75,
            block: "minecraft:grass_block".into(),
        };
        assert!(
            (distance_from_feet(&player(), &diagonal) - std::f64::consts::FRAC_1_SQRT_2).abs()
                < 0.001
        );
        // Straight down at the feet: the block face 2.0 below.
        let straight_down = Hit {
            x: 12.5,
            y: 62.0,
            z: -3.25,
            ..hit()
        };
        assert!((distance_from_feet(&player(), &straight_down) - 2.0).abs() < 0.001);
    }

    #[test]
    fn lines_show_name_id_distance_and_coords() {
        assert_eq!(
            lines("Grass Block", &hit(), 40.5, true),
            [
                "Grass Block",
                "minecraft:grass_block",
                "Distance 40.5",
                "12, 63, -44",
            ]
        );
        assert_eq!(
            lines("Grass Block", &hit(), 40.5, false),
            ["Grass Block", "minecraft:grass_block", "Distance 40.5"]
        );
    }

    #[test]
    fn translated_name_wins_over_the_pretty_id() {
        let state = Blockscope {
            shown: true,
            show_coords: true,
            reach: 5.0,
            names: Some(vec![(
                "minecraft:grass_block".into(),
                "Bloc d'herbe".into(),
            )]),
            last: Vec::new(),
        };
        assert_eq!(state.name_for("minecraft:grass_block"), "Bloc d'herbe");
        // A block with no item in the table falls back to its id.
        assert_eq!(state.name_for("minecraft:water"), "Water");
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert!(read_bool(Some(b"true".to_vec()), false));
        assert_eq!(read_reach(None), 5.0);
        assert_eq!(read_reach(Some(b"20".to_vec())), 20.0);
        assert_eq!(read_reach(Some(b"64".to_vec())), 64.0);
        assert_eq!(read_reach(Some(b"128".to_vec())), 128.0);
        // A stored reach the switch never offers falls back to 5.
        assert_eq!(read_reach(Some(b"1024".to_vec())), 5.0);
        assert_eq!(shown_label(false), "Shown: OFF");
        assert_eq!(coords_label(true), "Coordinates: ON");
        assert_eq!(reach_label(20.0), "Reach: 20 blocks");
        // The cycle steps through all four and wraps back to 5.
        assert_eq!(next_reach(5.0), 20.0);
        assert_eq!(next_reach(64.0), 128.0);
        assert_eq!(next_reach(128.0), 5.0);
    }
}

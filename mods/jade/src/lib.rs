//! Jade for Kestrel: the look-at tooltip, blocks and entities.
//!
//! Look at a block or a mob and a panel at the top-left shows what it is:
//! a block's name, its mod, its position and how far away it is; a mob's
//! name, its kind, its health and how far away it is. `/jade` hides or
//! shows the panel, and Options > Mods > Jade has the same switch; the
//! game keeps the choice in the mod's storage.
//!
//! A behaviour port of Jade by Snownee (https://github.com/Snownee/Jade)
//! and Jade for Horizon by Joeypc6 (https://github.com/Joeypc6/Jade-Horizon),
//! rewritten in Rust against the Kestrel SDK — no original code runs.
//! Not made or endorsed by Snownee: report problems to the Kestrel-mods
//! project, not to them. Licence: CC BY-NC-SA 4.0, noncommercial use only,
//! the same licence as Jade.
//!
//! What the SDK exposes, and what this port therefore shows, is the block
//! the player's view hits (`game.raycast`, out to the reach switch: 6, 20
//! or 64 blocks, 6 by default) and its translated name (`recipes.items`),
//! and the mob under the crosshair (`entity.targeted`, out to the same
//! reach; health reads only in singleplayer — in multiplayer the panel
//! shows the mob with no health line). What Jade also shows
//! that this port cannot: block states and progress (crops, furnaces,
//! machines), provider plugins, themes, config screens beyond the on/off
//! and reach switches, and the `/jadec` command. The panel sits at the
//! top-left because the SDK's HUD only offers corners; Jade's own overlay
//! is centred at the top.
use std::collections::HashMap;

use horizon_mod_sdk::{
    Mod, Registration,
    entity::{self, Target},
    game::{self, Hit, Player},
    hud::{self, Corner, Line},
    recipes, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The reaches the switch cycles through, in blocks; 6 is the player's
/// reach (survival 4.5, creative 5), with a little slack.
const REACHES: [f32; 3] = [6.0, 20.0, 64.0];
/// The storage key for the reach choice.
const REACH_KEY: &str = "reach";
/// The storage key for whether the panel shows.
const SHOWN_KEY: &str = "shown";
/// The settings page's buttons: show or hide the panel, reach.
const SHOWN_BUTTON: u32 = 0;
const REACH_BUTTON: u32 = 1;

struct Jade {
    shown: bool,
    reach: f32,
    /// The panel last sent, so an unchanged one costs no call.
    last: Vec<String>,
    /// Block id -> translated name, loaded once from `recipes.items` on
    /// the first tick that actually hits a block.
    names: Option<HashMap<String, String>>,
}

/// `minecraft:oak_log` -> `Oak Log`; a bare id keeps its own path.
fn prettify(id: &str) -> String {
    let path = id.split(':').nth(1).unwrap_or(id);
    path.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The item whose translated name a block goes by. Most blocks share their
/// item's id; the wall-mounted ones don't, and a potted plant has no item
/// of its own — the pot is what the player would hold.
fn item_id_for_block(block: &str) -> String {
    let (ns, path) = match block.split_once(':') {
        Some((ns, path)) => (ns, path),
        None => ("minecraft", block),
    };
    let path = path.split('[').next().unwrap_or(path);
    let item = if let Some(base) = path.strip_suffix("_wall_sign") {
        format!("{base}_sign")
    } else if let Some(base) = path.strip_suffix("_wall_banner") {
        format!("{base}_banner")
    } else if let Some(base) = path.strip_suffix("_wall_head") {
        format!("{base}_head")
    } else if let Some(base) = path.strip_suffix("_wall_torch") {
        format!("{base}_torch")
    } else if path == "wall_torch" {
        "torch".to_string()
    } else if path.starts_with("potted_") {
        "flower_pot".to_string()
    } else {
        path.to_string()
    };
    format!("{ns}:{item}")
}

/// `minecraft` -> `Minecraft`; any other namespace title-cased.
fn mod_name(id: &str) -> String {
    let ns = id.split(':').next().unwrap_or("minecraft");
    if ns == "minecraft" {
        return "Minecraft".to_string();
    }
    prettify(ns)
}

/// How far the hit point is from the player's feet, in blocks. The player
/// record holds the feet position, so this reads a touch long against a
/// block at eye level — the eye is ~1.62 above the feet.
fn distance(p: &Player, hit: &Hit) -> f64 {
    let (dx, dy, dz) = (hit.x - p.x, hit.y - p.y, hit.z - p.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// The tooltip's lines for a hit: the block's name, its mod, and its
/// position with the distance.
fn tooltip_lines(p: &Player, hit: &Hit, names: &HashMap<String, String>) -> Vec<String> {
    let block = hit.block.split('[').next().unwrap_or(&hit.block);
    let item = item_id_for_block(block);
    let name = names.get(&item).cloned().unwrap_or_else(|| prettify(block));
    vec![
        name,
        mod_name(block),
        format!(
            "{} {} {} · {:.1}m",
            hit.block_x,
            hit.block_y,
            hit.block_z,
            distance(p, hit)
        ),
    ]
}

/// The tooltip's lines for the mob under the crosshair: its name, its
/// kind, its health out of its maximum, and how far away it is — the
/// distance the SDK reports, eye to hit point. Health is `None` where the
/// client cannot vouch for it (multiplayer in v1): then the panel shows
/// the mob with no health line at all, never a fake or zero one.
fn entity_tooltip_lines(target: &Target) -> Vec<String> {
    let e = &target.entity;
    let mut lines = vec![e.name.clone(), e.kind.clone()];
    if let Some(health) = e.health {
        lines.push(format!("Health {health:.1}/{:.1}", e.max_health));
    }
    lines.push(format!("{:.1}m", target.distance));
    lines
}

/// The stored `shown` value: shown unless it says `false`.
fn read_shown(stored: Option<Vec<u8>>) -> bool {
    stored.is_none_or(|v| v != b"false")
}

fn shown_label(shown: bool) -> String {
    format!("Shown: {}", if shown { "ON" } else { "OFF" })
}

/// The stored reach: one of the switch's values, else the default.
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

/// The next reach in the switch's cycle, wrapping back to the first.
fn next_reach(reach: f32) -> f32 {
    let i = REACHES.iter().position(|r| *r == reach).unwrap_or(0);
    REACHES[(i + 1) % REACHES.len()]
}

fn reach_label(reach: f32) -> String {
    format!("Reach: {} blocks", reach as u32)
}

impl Jade {
    /// Hides or shows the panel, keeps the choice in storage, and refreshes
    /// the settings page when it is open.
    fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
        let _ = storage::set(SHOWN_KEY, if shown { b"true" } else { b"false" });
        self.refresh_settings();
    }

    /// Steps the reach to the next value, keeps the choice in storage, and
    /// refreshes the settings page when it is open.
    fn cycle_reach(&mut self) {
        self.reach = next_reach(self.reach);
        let _ = storage::set(REACH_KEY, self.reach.to_string().as_bytes());
        self.refresh_settings();
    }

    /// The settings page: the panel shown or hidden, and the reach.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: SHOWN_BUTTON,
                label: shown_label(self.shown),
            }),
            Widget::Button(ButtonWidget {
                id: REACH_BUTTON,
                label: reach_label(self.reach),
            }),
        ]);
    }

    /// The block name for this tick's hit, loading the translated names
    /// once (they don't change while a world is open).
    fn names(&mut self) -> &HashMap<String, String> {
        if self.names.is_none() {
            let names = recipes::items()
                .into_iter()
                .map(|item| (item.id, item.name))
                .collect();
            self.names = Some(names);
        }
        self.names.as_ref().expect("names just loaded")
    }
}

impl Mod for Jade {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("jade").on_tick();
        let state = Jade {
            shown: read_shown(storage::get(SHOWN_KEY)),
            reach: read_reach(storage::get(REACH_KEY)),
            last: Vec::new(),
            names: None,
        };
        Ok((state, hooks))
    }

    fn tick(&mut self, _tick: u64) {
        let now = match game::get_player().filter(|_| self.shown) {
            Some(p) => match entity::targeted(self.reach) {
                // A mob under the crosshair comes first: the SDK's
                // targeted hit is before any block, and out to the same
                // reach as the block half.
                Some(target) => entity_tooltip_lines(&target),
                None => match game::raycast(self.reach) {
                    Some(hit) => {
                        let names = self.names();
                        tooltip_lines(&p, &hit, names)
                    }
                    None => Vec::new(),
                },
            },
            None => Vec::new(),
        };
        if now != self.last {
            if now.is_empty() {
                hud::clear();
            } else {
                let panel: Vec<Line> = now
                    .iter()
                    .map(|text| Line {
                        text: text.clone(),
                        color: 0xFFFFFF,
                    })
                    .collect();
                hud::set_panel(Corner::TopLeft, &panel);
            }
            self.last = now;
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "jade" {
            return Err("This command does nothing".into());
        }
        self.set_shown(!self.shown);
        let state = if self.shown { "shown" } else { "hidden" };
        Ok(Some(format!("Jade tooltip {state}")))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SHOWN_BUTTON => self.set_shown(!self.shown),
            REACH_BUTTON => self.cycle_reach(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(Jade);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::{
        entity::{Entity, Target},
        game::Hit,
    };

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

    fn hit() -> Hit {
        Hit {
            block_x: 12,
            block_y: 63,
            block_z: -44,
            x: 12.5,
            y: 64.0,
            z: -43.8,
            block: "minecraft:grass_block".into(),
        }
    }

    #[test]
    fn prettify_turns_ids_into_names() {
        assert_eq!(prettify("minecraft:oak_log"), "Oak Log");
        assert_eq!(prettify("minecraft:grass_block"), "Grass Block");
        assert_eq!(
            prettify("minecraft:deepslate_diamond_ore"),
            "Deepslate Diamond Ore"
        );
        assert_eq!(prettify("stone"), "Stone");
    }

    #[test]
    fn wall_blocks_map_to_their_items() {
        assert_eq!(
            item_id_for_block("minecraft:oak_wall_sign"),
            "minecraft:oak_sign"
        );
        assert_eq!(
            item_id_for_block("minecraft:red_wall_banner"),
            "minecraft:red_banner"
        );
        assert_eq!(
            item_id_for_block("minecraft:creeper_wall_head"),
            "minecraft:creeper_head"
        );
        assert_eq!(
            item_id_for_block("minecraft:redstone_wall_torch"),
            "minecraft:redstone_torch"
        );
        assert_eq!(item_id_for_block("minecraft:wall_torch"), "minecraft:torch");
        assert_eq!(
            item_id_for_block("minecraft:potted_oak_sapling"),
            "minecraft:flower_pot"
        );
        assert_eq!(
            item_id_for_block("minecraft:oak_log[axis=y]"),
            "minecraft:oak_log"
        );
        assert_eq!(item_id_for_block("minecraft:stone"), "minecraft:stone");
    }

    #[test]
    fn mod_name_is_minecraft_for_vanilla() {
        assert_eq!(mod_name("minecraft:stone"), "Minecraft");
        assert_eq!(mod_name("some_mod:thing"), "Some Mod");
    }

    #[test]
    fn distance_is_euclidean_from_the_feet() {
        let p = Player {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            ..player()
        };
        let h = Hit {
            x: 3.0,
            y: 4.0,
            z: 0.0,
            ..hit()
        };
        assert_eq!(distance(&p, &h), 5.0);
    }

    #[test]
    fn tooltip_shows_name_mod_position_and_distance() {
        // 12 across, 44.3 back: sqrt(12^2 + 44.3^2) = 45.9, checked
        // independently of this code.
        let names = HashMap::from([(
            "minecraft:grass_block".to_string(),
            "Grass Block".to_string(),
        )]);
        assert_eq!(
            tooltip_lines(&player(), &hit(), &names),
            ["Grass Block", "Minecraft", "12 63 -44 · 45.9m",]
        );
    }

    #[test]
    fn tooltip_falls_back_to_the_prettified_id_without_a_name_table() {
        let names = HashMap::new();
        assert_eq!(tooltip_lines(&player(), &hit(), &names)[0], "Grass Block");
        let wall = Hit {
            block: "minecraft:oak_wall_sign".into(),
            ..hit()
        };
        assert_eq!(tooltip_lines(&player(), &wall, &names)[0], "Oak Wall Sign");
    }

    #[test]
    fn shown_unless_stored_false() {
        assert!(read_shown(None));
        assert!(read_shown(Some(b"true".to_vec())));
        assert!(!read_shown(Some(b"false".to_vec())));
        assert_eq!(shown_label(false), "Shown: OFF");
    }

    #[test]
    fn reach_defaults_to_six_and_cycles_back() {
        assert_eq!(read_reach(None), 6.0);
        assert_eq!(read_reach(Some(b"20".to_vec())), 20.0);
        assert_eq!(read_reach(Some(b"64".to_vec())), 64.0);
        // A stored reach the switch never offers falls back to 6.
        assert_eq!(read_reach(Some(b"128".to_vec())), 6.0);
        assert_eq!(reach_label(6.0), "Reach: 6 blocks");
        assert_eq!(next_reach(6.0), 20.0);
        assert_eq!(next_reach(20.0), 64.0);
        assert_eq!(next_reach(64.0), 6.0);
    }

    fn targeted_mob(
        name: &str,
        kind: &str,
        health: Option<f32>,
        max_health: f32,
        distance: f32,
    ) -> Target {
        Target {
            entity: Entity {
                id: 1,
                kind: kind.into(),
                name: name.into(),
                x: 0.5,
                y: 64.0,
                z: 5.5,
                vx: 0.0,
                vy: 0.0,
                vz: 0.0,
                yaw: 0.0,
                health,
                max_health,
                width: 0.6,
                height: 1.95,
                on_ground: true,
                custom_name: None,
                baby: false,
            },
            distance,
        }
    }

    #[test]
    fn entity_tooltip_shows_name_kind_health_and_distance() {
        assert_eq!(
            entity_tooltip_lines(&targeted_mob(
                "Zombie",
                "minecraft:zombie",
                Some(14.0),
                20.0,
                4.7
            )),
            ["Zombie", "minecraft:zombie", "Health 14.0/20.0", "4.7m",]
        );
    }

    #[test]
    fn entity_tooltip_without_health_has_no_health_line() {
        // Multiplayer in v1, and a player under the crosshair: health
        // reads None, so the panel shows name, kind and distance only —
        // never a fake or zero health.
        assert_eq!(
            entity_tooltip_lines(&targeted_mob("Zombie", "minecraft:zombie", None, 20.0, 4.7)),
            ["Zombie", "minecraft:zombie", "4.7m",]
        );
        assert_eq!(
            entity_tooltip_lines(&targeted_mob("Steve", "minecraft:player", None, 20.0, 2.7)),
            ["Steve", "minecraft:player", "2.7m",]
        );
    }

    #[test]
    fn entity_tooltip_uses_the_display_name_not_the_kind() {
        // A named sheep shows its custom name first, then its kind.
        assert_eq!(
            entity_tooltip_lines(&targeted_mob(
                "Dolly",
                "minecraft:sheep",
                Some(8.0),
                8.0,
                5.0
            )),
            ["Dolly", "minecraft:sheep", "Health 8.0/8.0", "5.0m",]
        );
    }
}

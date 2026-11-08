//! LookAt for Kestrel: the block or mob under the crosshair as a tooltip
//! at the top of the screen — its name in your language, the namespace it
//! comes from, its position and distance, and for a mob its health as
//! hearts and numbers. `/lookat` hides or shows it, and Options > Mods >
//! LookAt has the same switch plus range (6, 20, 64, 256 or 1024 blocks),
//! position (top, top left, top right), health and entities switches; the
//! game keeps the choices in the mod's storage.
//!
//! Behaviour reimplemented from LookAt by d3grando (MIT,
//! <https://github.com/d3grando/lookat>); this file shares no code with
//! the original. The block's name comes from the game's item table
//! (`recipes.items`), as the original reads it from the game's own
//! tables, and a block with no item falls back to its id with the words
//! capitalised ("Water"). The distance is measured from the player's eye
//! to the point the crosshair meets the block or mob. A mob wins over a
//! block when it is the nearer of the two under the crosshair; a mob
//! behind a wall is never shown, because `entity.targeted` never returns
//! it. Health is `Some` in singleplayer and `None` in multiplayer in v1:
//! with no health to show, the tooltip shows the mob's name, kind and
//! distance and no health line. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/lookat>.
//!
//! What the SDK cannot do, the original's native build can: the item's
//! icon (the panel is text, so there is no icon to draw), the harvest
//! tool a block needs (the held item is readable but the harvest rule is
//! not exposed), and non-living entities — boats, minecarts, armor
//! stands, paintings and item frames — because the v1 entity API covers
//! living mobs only. The panel also anchors to a corner, not the top
//! centre: the SDK offers no top-centre corner, so Top and Top Left both
//! anchor top-left in v1.
use std::collections::HashMap;

use horizon_mod_sdk::{
    Mod, Registration,
    entity::{self, Target},
    game::{self, Hit, Player},
    hud::{self, Corner, Line},
    recipes, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `entity.targeted` is level 10, `recipes.items` is level 5,
/// `game.raycast` is level 2, the rest level 0
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 10;

/// The player's eye above the feet, as the game's crosshair raycast
/// measures it.
const EYE_HEIGHT: f64 = 1.62;

/// The storage keys for the five settings.
const SHOWN_KEY: &str = "shown";
const RANGE_KEY: &str = "range";
const POSITION_KEY: &str = "position";
const HEALTH_KEY: &str = "health";
const ENTITIES_KEY: &str = "entities";

/// The settings page's buttons: show the tooltip, range, position,
/// health, entities.
const SHOWN_BUTTON: u32 = 0;
const RANGE_BUTTON: u32 = 1;
const POSITION_BUTTON: u32 = 2;
const HEALTH_BUTTON: u32 = 3;
const ENTITIES_BUTTON: u32 = 4;

/// The ranges the switch cycles through, in blocks; 64 is the default —
/// far past vanilla reach, as the original looks across distant terrain,
/// and 1024 is the SDK's raycast cap (`RAYCAST_MAX`).
const RANGES: [f32; 5] = [6.0, 20.0, 64.0, 256.0, 1024.0];

/// Where the tooltip sits. The SDK's HUD offers corners only, no
/// top-centre, so Top anchors top-left in v1, the same as Top Left.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Position {
    Top,
    TopLeft,
    TopRight,
}

impl Position {
    fn corner(self) -> Corner {
        match self {
            Position::Top | Position::TopLeft => Corner::TopLeft,
            Position::TopRight => Corner::TopRight,
        }
    }
}

struct LookAt {
    shown: bool,
    range: f32,
    position: Position,
    show_health: bool,
    show_entities: bool,
    /// Block id -> translated name, loaded once from `recipes.items` on
    /// the first tick that actually hits a block.
    names: Option<HashMap<String, String>>,
    /// The panel last sent, so an unchanged one costs no call.
    last: Vec<String>,
    last_corner: Option<Corner>,
}

/// `minecraft:grass_block` -> "Grass Block"; a bare id keeps its path.
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

/// `minecraft` -> "Minecraft"; any other namespace title-cased.
fn namespace_name(id: &str) -> String {
    let ns = id.split(':').next().unwrap_or("minecraft");
    if ns == "minecraft" {
        return "Minecraft".to_string();
    }
    pretty_id(ns)
}

/// How far the hit point is from the player's eye, in blocks.
fn distance_from_eye(player: &Player, x: f64, y: f64, z: f64) -> f64 {
    let (dx, dy, dz) = (x - player.x, y - (player.y + EYE_HEIGHT), z - player.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// The tooltip's lines for a block hit: the name, the id, the namespace
/// it comes from, and its position with the distance.
fn block_lines(player: &Player, hit: &Hit, names: &HashMap<String, String>) -> Vec<String> {
    let block = hit.block.split('[').next().unwrap_or(&hit.block);
    let item = item_id_for_block(block);
    let name = names
        .get(&item)
        .cloned()
        .unwrap_or_else(|| pretty_id(block));
    vec![
        name,
        block.to_string(),
        namespace_name(block),
        format!(
            "{} {} {} · {:.1}m",
            hit.block_x,
            hit.block_y,
            hit.block_z,
            distance_from_eye(player, hit.x, hit.y, hit.z)
        ),
    ]
}

/// A mob's health as hearts and numbers: one heart per two hit points,
/// full hearts filled, the rest empty, with the numbers beside them so a
/// half heart never hides the exact value. `14.0/20.0` is seven full
/// hearts and three empty ones.
fn health_hearts(health: f32, max_health: f32) -> String {
    let total = (max_health / 2.0).ceil().max(1.0) as usize;
    let filled = ((health / 2.0).round() as usize).min(total);
    let mut hearts = String::with_capacity(total);
    for _ in 0..filled {
        hearts.push('♥');
    }
    for _ in filled..total {
        hearts.push('♡');
    }
    format!("{hearts} {health:.1}/{max_health:.1}")
}

/// The tooltip's lines for the mob under the crosshair: the name, the
/// kind, the health when the switch is on and the game vouches for it,
/// and the distance. With no health (multiplayer in v1) there is no
/// health line at all.
fn entity_lines(target: &Target, show_health: bool) -> Vec<String> {
    let entity = &target.entity;
    let mut lines = vec![entity.name.clone(), entity.kind.clone()];
    if show_health && let Some(health) = entity.health {
        lines.push(health_hearts(health, entity.max_health));
    }
    lines.push(format!("Distance {:.1}", target.distance));
    lines
}

/// Whether the mob under the crosshair wins over the block hit: it does
/// when there is no block, or when it is the nearer of the two from the
/// eye. A mob behind a wall never reaches here — `targeted` returns
/// `None` for it — so this only settles a mob standing in front of a
/// farther block.
fn entity_wins(player: &Player, target: &Target, hit: Option<&Hit>) -> bool {
    match hit {
        None => true,
        Some(hit) => f64::from(target.distance) <= distance_from_eye(player, hit.x, hit.y, hit.z),
    }
}

/// The stored `shown` value: shown unless it says `false`.
fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

/// The stored range: one of the switch's values, else the default.
fn read_range(stored: Option<Vec<u8>>) -> f32 {
    let wanted = stored
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(RANGES[2]);
    if RANGES.contains(&wanted) {
        wanted
    } else {
        RANGES[2]
    }
}

/// The stored position: `top`, `top-left` or `top-right`, else Top.
fn read_position(stored: Option<Vec<u8>>) -> Position {
    match stored.and_then(|v| String::from_utf8(v).ok()).as_deref() {
        Some("top-left") => Position::TopLeft,
        Some("top-right") => Position::TopRight,
        _ => Position::Top,
    }
}

/// The next range in the switch's cycle, wrapping back to the first.
fn next_range(range: f32) -> f32 {
    let i = RANGES.iter().position(|r| *r == range).unwrap_or(0);
    RANGES[(i + 1) % RANGES.len()]
}

/// The next position in the switch's cycle, wrapping back to the first.
fn next_position(position: Position) -> Position {
    match position {
        Position::Top => Position::TopLeft,
        Position::TopLeft => Position::TopRight,
        Position::TopRight => Position::Top,
    }
}

fn shown_label(shown: bool) -> String {
    format!("Shown: {}", if shown { "ON" } else { "OFF" })
}

fn range_label(range: f32) -> String {
    format!("Range: {} blocks", range as u32)
}

fn position_label(position: Position) -> String {
    format!(
        "Position: {}",
        match position {
            Position::Top => "Top",
            Position::TopLeft => "Top Left",
            Position::TopRight => "Top Right",
        }
    )
}

fn health_label(show_health: bool) -> String {
    format!("Health: {}", if show_health { "ON" } else { "OFF" })
}

fn entities_label(show_entities: bool) -> String {
    format!("Entities: {}", if show_entities { "ON" } else { "OFF" })
}

impl LookAt {
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

    fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
        let _ = storage::set(SHOWN_KEY, if shown { b"true" } else { b"false" });
        self.refresh_settings();
    }

    fn cycle_range(&mut self) {
        self.range = next_range(self.range);
        let _ = storage::set(RANGE_KEY, self.range.to_string().as_bytes());
        self.refresh_settings();
    }

    fn cycle_position(&mut self) {
        self.position = next_position(self.position);
        let value = match self.position {
            Position::Top => "top",
            Position::TopLeft => "top-left",
            Position::TopRight => "top-right",
        };
        let _ = storage::set(POSITION_KEY, value.as_bytes());
        self.refresh_settings();
    }

    fn toggle_health(&mut self) {
        self.show_health = !self.show_health;
        let _ = storage::set(
            HEALTH_KEY,
            if self.show_health { b"true" } else { b"false" },
        );
        self.refresh_settings();
    }

    fn toggle_entities(&mut self) {
        self.show_entities = !self.show_entities;
        let _ = storage::set(
            ENTITIES_KEY,
            if self.show_entities {
                b"true"
            } else {
                b"false"
            },
        );
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
                id: RANGE_BUTTON,
                label: range_label(self.range),
            }),
            Widget::Button(ButtonWidget {
                id: POSITION_BUTTON,
                label: position_label(self.position),
            }),
            Widget::Button(ButtonWidget {
                id: HEALTH_BUTTON,
                label: health_label(self.show_health),
            }),
            Widget::Button(ButtonWidget {
                id: ENTITIES_BUTTON,
                label: entities_label(self.show_entities),
            }),
        ]);
    }

    fn clear_panel(&mut self) {
        if !self.last.is_empty() {
            hud::clear();
            self.last.clear();
            self.last_corner = None;
        }
    }
}

impl Mod for LookAt {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("lookat").on_tick();
        let state = LookAt {
            shown: read_bool(storage::get(SHOWN_KEY), true),
            range: read_range(storage::get(RANGE_KEY)),
            position: read_position(storage::get(POSITION_KEY)),
            show_health: read_bool(storage::get(HEALTH_KEY), true),
            show_entities: read_bool(storage::get(ENTITIES_KEY), true),
            names: None,
            last: Vec::new(),
            last_corner: None,
        };
        Ok((state, hooks))
    }

    fn tick(&mut self, _tick: u64) {
        let Some(player) = game::get_player().filter(|_| self.shown) else {
            self.clear_panel();
            return;
        };
        // The mob under the crosshair, when the entities switch is on:
        // `targeted` caps at 128 blocks, so a longer range only reaches
        // farther blocks.
        let target = self
            .show_entities
            .then(|| entity::targeted(self.range.min(128.0)))
            .flatten();
        let hit = game::raycast(self.range);
        let now = match (&target, &hit) {
            (Some(target), _) if entity_wins(&player, target, hit.as_ref()) => {
                entity_lines(target, self.show_health)
            }
            (_, Some(hit)) => {
                let names = self.names();
                block_lines(&player, hit, names)
            }
            _ => Vec::new(),
        };
        let corner = self.position.corner();
        if now != self.last || Some(corner) != self.last_corner {
            if now.is_empty() {
                self.clear_panel();
            } else {
                let panel: Vec<Line> = now
                    .iter()
                    .map(|text| Line {
                        text: text.clone(),
                        color: 0xFFFFFF,
                    })
                    .collect();
                hud::set_panel(corner, &panel);
                self.last = now;
                self.last_corner = Some(corner);
            }
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "lookat" {
            return Err("This command does nothing".into());
        }
        self.set_shown(!self.shown);
        let state = if self.shown { "shown" } else { "hidden" };
        Ok(Some(format!("LookAt tooltip {state}")))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SHOWN_BUTTON => self.set_shown(!self.shown),
            RANGE_BUTTON => self.cycle_range(),
            POSITION_BUTTON => self.cycle_position(),
            HEALTH_BUTTON => self.toggle_health(),
            ENTITIES_BUTTON => self.toggle_entities(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(LookAt);

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

    fn mob(health: Option<f32>) -> Entity {
        Entity {
            id: 1,
            kind: "minecraft:zombie".into(),
            name: "Zombie".into(),
            x: 0.5,
            y: 64.0,
            z: 5.5,
            vx: 0.0,
            vy: -0.1,
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

    fn target(health: Option<f32>, distance: f32) -> Target {
        Target {
            entity: mob(health),
            distance,
        }
    }

    #[test]
    fn api_level_is_ten() {
        // Newest imported function is `entity.targeted` (level 10);
        // `recipes.items` is level 5, `game.raycast` level 2.
        assert_eq!(API_LEVEL, 10);
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
    fn namespace_name_is_minecraft_for_vanilla() {
        assert_eq!(namespace_name("minecraft:stone"), "Minecraft");
        assert_eq!(namespace_name("some_mod:thing"), "Some Mod");
    }

    #[test]
    fn distance_is_measured_from_the_eye() {
        let p = Player {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            ..player()
        };
        // The hit point 3 across, 4 up from the eye (1.62 up): 3-4-5
        // with the eye offset folded in.
        let h = Hit {
            x: 3.0,
            y: 1.62 + 4.0,
            z: 0.0,
            ..hit()
        };
        assert!((distance_from_eye(&p, h.x, h.y, h.z) - 5.0).abs() < 0.001);
    }

    #[test]
    fn block_lines_show_name_id_namespace_position_and_distance() {
        // 12 across, 44.3 back from the eye: sqrt(12^2 + 44.3^2 + 1.62^2)
        // = 45.9, checked independently of this code.
        let names = HashMap::from([(
            "minecraft:grass_block".to_string(),
            "Bloc d'herbe".to_string(),
        )]);
        assert_eq!(
            block_lines(&player(), &hit(), &names),
            [
                "Bloc d'herbe",
                "minecraft:grass_block",
                "Minecraft",
                "12 63 -44 · 45.9m",
            ]
        );
    }

    #[test]
    fn block_lines_fall_back_to_the_prettified_id_without_a_name_table() {
        let names = HashMap::new();
        assert_eq!(block_lines(&player(), &hit(), &names)[0], "Grass Block");
        let wall = Hit {
            block: "minecraft:oak_wall_sign".into(),
            ..hit()
        };
        assert_eq!(block_lines(&player(), &wall, &names)[0], "Oak Wall Sign");
    }

    #[test]
    fn health_hearts_fill_one_heart_per_two_hit_points() {
        assert_eq!(health_hearts(20.0, 20.0), "♥♥♥♥♥♥♥♥♥♥ 20.0/20.0");
        assert_eq!(health_hearts(14.0, 20.0), "♥♥♥♥♥♥♥♡♡♡ 14.0/20.0");
        // 15 hit points rounds to eight hearts; the numbers stay exact.
        assert_eq!(health_hearts(15.0, 20.0), "♥♥♥♥♥♥♥♥♡♡ 15.0/20.0");
        assert_eq!(health_hearts(1.0, 20.0), "♥♡♡♡♡♡♡♡♡♡ 1.0/20.0");
        assert_eq!(health_hearts(0.0, 20.0), "♡♡♡♡♡♡♡♡♡♡ 0.0/20.0");
        // A tougher mob gets more hearts: 40 max health is twenty.
        assert_eq!(health_hearts(30.0, 40.0), "♥♥♥♥♥♥♥♥♥♥♥♥♥♥♥♡♡♡♡♡ 30.0/40.0");
    }

    #[test]
    fn entity_lines_show_name_kind_health_and_distance() {
        assert_eq!(
            entity_lines(&target(Some(14.0), 4.7), true),
            [
                "Zombie",
                "minecraft:zombie",
                "♥♥♥♥♥♥♥♡♡♡ 14.0/20.0",
                "Distance 4.7",
            ]
        );
    }

    #[test]
    fn entity_lines_omit_health_when_it_is_unknown_or_off() {
        // Multiplayer in v1: health is None, so no health line at all.
        assert_eq!(
            entity_lines(&target(None, 4.7), true),
            ["Zombie", "minecraft:zombie", "Distance 4.7",]
        );
        // The health switch off: the same, even with health known.
        assert_eq!(
            entity_lines(&target(Some(14.0), 4.7), false),
            ["Zombie", "minecraft:zombie", "Distance 4.7",]
        );
    }

    #[test]
    fn nearer_of_mob_and_block_wins() {
        let p = player();
        // The mob 4.7 away beats the block ~45.9 away.
        assert!(entity_wins(&p, &target(Some(20.0), 4.7), Some(&hit())));
        // The block wins when the mob is past it.
        assert!(!entity_wins(&p, &target(Some(20.0), 60.0), Some(&hit())));
        // With no block at all the mob always wins.
        assert!(entity_wins(&p, &target(Some(20.0), 4.7), None));
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert!(read_bool(Some(b"true".to_vec()), false));
        assert_eq!(read_range(None), 64.0);
        assert_eq!(read_range(Some(b"6".to_vec())), 6.0);
        assert_eq!(read_range(Some(b"20".to_vec())), 20.0);
        assert_eq!(read_range(Some(b"256".to_vec())), 256.0);
        assert_eq!(read_range(Some(b"1024".to_vec())), 1024.0);
        // A stored range the switch never offers falls back to 64.
        assert_eq!(read_range(Some(b"128".to_vec())), 64.0);
        assert_eq!(read_position(None), Position::Top);
        assert_eq!(read_position(Some(b"top-left".to_vec())), Position::TopLeft);
        assert_eq!(
            read_position(Some(b"top-right".to_vec())),
            Position::TopRight
        );
        assert_eq!(read_position(Some(b"centre".to_vec())), Position::Top);
        assert_eq!(shown_label(false), "Shown: OFF");
        assert_eq!(range_label(1024.0), "Range: 1024 blocks");
        assert_eq!(position_label(Position::Top), "Position: Top");
        assert_eq!(position_label(Position::TopLeft), "Position: Top Left");
        assert_eq!(position_label(Position::TopRight), "Position: Top Right");
        assert_eq!(health_label(true), "Health: ON");
        assert_eq!(entities_label(false), "Entities: OFF");
        // The cycles step through every value and wrap back.
        assert_eq!(next_range(6.0), 20.0);
        assert_eq!(next_range(64.0), 256.0);
        assert_eq!(next_range(256.0), 1024.0);
        assert_eq!(next_range(1024.0), 6.0);
        assert_eq!(next_position(Position::Top), Position::TopLeft);
        assert_eq!(next_position(Position::TopLeft), Position::TopRight);
        assert_eq!(next_position(Position::TopRight), Position::Top);
        // Top and Top Left anchor the same corner in v1: the SDK has no
        // top-centre corner.
        assert_eq!(Position::Top.corner(), Corner::TopLeft);
        assert_eq!(Position::TopLeft.corner(), Corner::TopLeft);
        assert_eq!(Position::TopRight.corner(), Corner::TopRight);
    }
}

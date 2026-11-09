//! Rider HUD for Kestrel: while riding, your hunger stays on
//! screen — a row of the game's own food sprites directly above
//! the mount's heart rows, so hunger and mount health show at
//! the same time, in the familiar vanilla style.
//!
//! Behaviour reimplemented from Rider HUD by Xariami (MIT,
//! <https://github.com/maliferen/horizon-rider-hud>); this file
//! shares no code with the original. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/rider_hud>.
//!
//! Vanilla's bargain while riding is that the mount's health
//! takes the hunger row's place: the game draws the vehicle
//! hearts (its `extractVehicleHealth`) and the food row is not
//! drawn at all. The mount's hearts therefore need no help
//! from this mod — the game already draws them where vanilla
//! draws them. What the mod adds back is the hunger: API 18's
//! self read (`game.get-self-state`, capability `game-read`)
//! names the mount, `game.get-player` carries the food level,
//! and the row is drawn with the game's own food sprites in
//! the game's own food layout, one row above however many rows
//! the mount's hearts take — the arrangement the original's
//! screenshots show. While the Hunger effect lasts the row
//! wears the game's hunger-variant sprites, as the game's own
//! food row does (the effect read is API 18's other half).
//!
//! Fidelity notes (the README has the full accounting): the
//! original also keeps the experience bar visible while
//! riding and switches it with the jump bar as a jump charges
//! ("dynamic experience and jump bar switching"). The SDK
//! exposes no experience read and no jump-charge read, so
//! neither bar can be drawn honestly; both are documented
//! gaps, and their settings toggles go with them — the one
//! setting here is the hunger row's.

use horizon_mod_sdk::{
    Mod, Registration,
    game::{self, Mount},
    render::{self, Point, Rect, Shape, SpriteShape},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `game.get-self-state` and `game.get-effects` are
/// level 18 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 18;

/// The storage key and settings button for the one setting.
const HUNGER_KEY: &str = "hunger";
const HUNGER_BUTTON: u32 = 0;

/// The HUD's own metrics, the game's (its `Hud` layout): the
/// status rows' right edge sits 91 GUI pixels right of the
/// screen's centre, their base row's top is 39 above the
/// bottom, and icons step 8 pixels, one row every 10.
const X_RIGHT_OFFSET: f32 = 91.0;
const Y_BASE_OFFSET: f32 = 39.0;
const ICON_STEP: f32 = 8.0;
const ROW_STEP: f32 = 10.0;

/// How many hearts the mount's health takes, the game's own
/// count (`Hud.getVehicleMaxHearts`): half the max health,
/// rounded, capped at 30 — 0 for a mount that shows no health,
/// in which case the game draws the food row itself and this
/// mod has nothing to add.
fn vehicle_max_hearts(max_health: f32) -> i32 {
    (((max_health + 0.5) as i32) / 2).min(30)
}

/// How many rows the mount's hearts occupy: ten to a row,
/// growing upward from the base row.
fn mount_rows(max_health: f32) -> i32 {
    let hearts = vehicle_max_hearts(max_health);
    if hearts <= 0 {
        0
    } else {
        (hearts + 9) / 10
    }
}

/// One food icon's fill, the game's own rule for the icon at
/// position `n` (0 is the rightmost, as the row fills from the
/// right): the icon is full when the food level covers both
/// its halves, half when it covers one, and empty otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FoodIcon {
    Full,
    Half,
    Empty,
}

fn food_icon(n: u32, food: u32) -> FoodIcon {
    if n * 2 + 1 < food {
        FoodIcon::Full
    } else if n * 2 + 1 == food {
        FoodIcon::Half
    } else {
        FoodIcon::Empty
    }
}

/// The food row's sprites for a food level: per icon the empty
/// container, with the full or half sprite laid over it when
/// the level reaches it — the game's own food drawing, in the
/// hunger variant while the Hunger effect lasts. `x_right` and
/// `y` are the row's right edge and top, in GUI pixels.
fn food_shapes(food: u32, hunger_effect: bool, x_right: f32, y: f32) -> Vec<Shape> {
    let (empty, half, full) = if hunger_effect {
        (
            "hud/food_empty_hunger",
            "hud/food_half_hunger",
            "hud/food_full_hunger",
        )
    } else {
        ("hud/food_empty", "hud/food_half", "hud/food_full")
    };
    let mut shapes = Vec::new();
    let mut sprite = |name: &str, x: f32| {
        shapes.push(Shape::Sprite(SpriteShape {
            sprite: name.to_string(),
            rect: Rect {
                x,
                y,
                w: 9.0,
                h: 9.0,
            },
            color: 0xFFFF_FFFF,
        }));
    };
    for n in 0..10u32 {
        let x = x_right - n as f32 * ICON_STEP - 9.0;
        sprite(empty, x);
        match food_icon(n, food) {
            FoodIcon::Full => sprite(full, x),
            FoodIcon::Half => sprite(half, x),
            FoodIcon::Empty => {}
        }
    }
    shapes
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

fn hunger_label(on: bool) -> String {
    format!("Hunger: {}", on_off(on))
}

struct RiderHud {
    hunger: bool,
    /// What the shapes on screen were built from — food level,
    /// hunger variant, mount rows, screen size — so an
    /// unchanged tick costs no call; `None` while nothing is
    /// drawn.
    drawn: Option<(u32, bool, i32, u32, u32)>,
}

impl RiderHud {
    fn refresh(&mut self) {
        let key = self.current_key();
        if key == self.drawn {
            return;
        }
        match key {
            Some((food, hunger_effect, rows, w, h)) => {
                let x_right = w as f32 / 2.0 + X_RIGHT_OFFSET;
                let y = h as f32 - Y_BASE_OFFSET - rows as f32 * ROW_STEP;
                render::set_hud(&food_shapes(food, hunger_effect, x_right, y));
            }
            None => render::set_hud(&[]),
        }
        self.drawn = key;
    }

    /// This tick's draw key, or none when there is nothing to
    /// add: on foot, on a mount that shows no health (the game
    /// draws the food row itself then), or with the hunger
    /// setting off.
    fn current_key(&self) -> Option<(u32, bool, i32, u32, u32)> {
        if !self.hunger {
            return None;
        }
        let state = game::get_self_state()?;
        let mount: &Mount = state.mount.as_ref()?;
        let rows = mount_rows(mount.max_health);
        if rows == 0 {
            return None;
        }
        let player = game::get_player()?;
        let hunger_effect = game::get_effects()
            .iter()
            .any(|e| e.id.split(':').next_back() == Some("hunger"));
        let (w, h) = render::gui_size();
        Some((player.food, hunger_effect, rows, w as u32, h as u32))
    }

    /// The settings page in Options > Mods: the hunger row's
    /// toggle (the original's experience and jump toggles
    /// belong to features this port documents as gaps).
    fn refresh_settings(&self) {
        ui::set_settings(&[Widget::Button(ButtonWidget {
            id: HUNGER_BUTTON,
            label: hunger_label(self.hunger),
        })]);
    }
}

impl Mod for RiderHud {
    fn init() -> Result<(Self, Registration), String> {
        let state = RiderHud {
            hunger: read_bool(HUNGER_KEY, true),
            drawn: None,
        };
        Ok((state, Registration::new().on_tick()))
    }

    fn tick(&mut self, _tick: u64) {
        self.refresh();
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        if id == HUNGER_BUTTON {
            self.hunger = !self.hunger;
            let _ = storage::set(
                HUNGER_KEY,
                if self.hunger { b"true" } else { b"false" },
            );
            self.drawn = None;
            self.refresh_settings();
        }
    }
}

horizon_mod_sdk::export_mod!(RiderHud);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_18() {
        assert_eq!(API_LEVEL, 18);
    }

    #[test]
    fn mount_hearts_follow_the_games_count() {
        // `getVehicleMaxHearts`: half the max health rounded,
        // capped at 30, in rows of ten.
        assert_eq!(vehicle_max_hearts(30.0), 15);
        assert_eq!(mount_rows(30.0), 2);
        assert_eq!(vehicle_max_hearts(15.0), 7);
        assert_eq!(mount_rows(15.0), 1);
        assert_eq!(vehicle_max_hearts(20.0), 10);
        assert_eq!(mount_rows(20.0), 1);
        assert_eq!(vehicle_max_hearts(100.0), 30);
        assert_eq!(mount_rows(100.0), 3);
        assert_eq!(mount_rows(0.0), 0);
    }

    #[test]
    fn food_icons_fill_from_the_right() {
        // A full bar: every icon full.
        assert_eq!(food_icon(0, 20), FoodIcon::Full);
        assert_eq!(food_icon(9, 20), FoodIcon::Full);
        // 19: the leftmost icon is half, the rest full.
        assert_eq!(food_icon(9, 19), FoodIcon::Half);
        assert_eq!(food_icon(8, 19), FoodIcon::Full);
        // 7: three full, one half, the rest empty.
        assert_eq!(food_icon(0, 7), FoodIcon::Full);
        assert_eq!(food_icon(2, 7), FoodIcon::Full);
        assert_eq!(food_icon(3, 7), FoodIcon::Half);
        assert_eq!(food_icon(4, 7), FoodIcon::Empty);
        // 0: every icon empty.
        assert_eq!(food_icon(0, 0), FoodIcon::Empty);
    }

    #[test]
    fn the_row_draws_containers_and_fills() {
        // Full hunger at the right edge x=1000, top y=100: ten
        // containers and ten fills, rightmost first.
        let shapes = food_shapes(20, false, 1000.0, 100.0);
        assert_eq!(shapes.len(), 20);
        let Shape::Sprite(first) = &shapes[0] else {
            panic!("the row opens with a container: {:?}", shapes[0]);
        };
        assert_eq!(first.sprite, "hud/food_empty");
        assert_eq!((first.rect.x, first.rect.y), (991.0, 100.0));
        let Shape::Sprite(second) = &shapes[1] else {
            panic!("a fill follows its container: {:?}", shapes[1]);
        };
        assert_eq!(second.sprite, "hud/food_full");
        // The leftmost icon sits nine steps left.
        let Shape::Sprite(last_container) = &shapes[18] else {
            panic!("the last container: {:?}", shapes[18]);
        };
        assert_eq!(last_container.rect.x, 1000.0 - 9.0 * 8.0 - 9.0);
        // The hunger variant wears the hunger's own sprites,
        // and an empty bar draws containers only.
        let shapes = food_shapes(0, true, 1000.0, 100.0);
        assert_eq!(shapes.len(), 10);
        let Shape::Sprite(first) = &shapes[0] else {
            panic!("hunger container: {:?}", shapes[0]);
        };
        assert_eq!(first.sprite, "hud/food_empty_hunger");
    }

    #[test]
    fn the_setting_labels_itself() {
        assert_eq!(hunger_label(true), "Hunger: On");
        assert_eq!(hunger_label(false), "Hunger: Off");
    }
}

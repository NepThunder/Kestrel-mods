//! Status Effect HUD for Kestrel: every active effect listed on
//! the HUD with the game's own mob_effect sprite, its name and
//! Roman numeral level, a colour-coded mm:ss countdown and a
//! smooth duration bar in the effect's vanilla potion colour.
//!
//! Behaviour reimplemented from Status Effect HUD by CayMac
//! (MIT, <https://github.com/CayMac97/status-effect-hud>), itself
//! a port of Status Effect Bars and Inventory HUD+ (Potion HUD);
//! this file shares no code with the original. The list is fed by
//! the SDK's self reads (API 18, `game.get-effects`, capability
//! `game-read`): the local player's active effects with their
//! amplifiers and the ticks left on them. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/status_effect_hud>.
//!
//! Fidelity notes (the README has the full accounting): the
//! countdown colours are the original's — white over 30 s,
//! yellow from 30 s down to 10 s, flashing red at 10 s and
//! under, aqua ∞ for a permanent effect. The bar's colour is the
//! effect's authentic vanilla potion colour, from the same
//! `MobEffects` table the game itself carries. A duration bar
//! needs the effect's full length and the SDK reports only the
//! time left, so — the way the original genre does it — the bar
//! is drawn against the longest remaining time seen for that
//! effect since it appeared. Effect names are read from the
//! effect's id: the SDK has no translation lookup, so the
//! translated names the original shows are the id's own words,
//! which are the same words in English.

use horizon_mod_sdk::{
    Mod, Registration,
    game::{self, Effect},
    render::{self, FillShape, Point, Rect, Shape, SpriteShape, TextShape},
};
use std::collections::HashMap;

/// The newest API level this mod needs, asserted in the tests
/// below: `game.get-effects` is level 18
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 18;

/// The list's layout, in GUI pixels: the rows start at the top
/// left, each row is the game's own 24-pixel effect background
/// with a 2-pixel gap, the texts sit right of the icon, and the
/// duration bar runs under the whole row.
const LEFT: f32 = 2.0;
const TOP: f32 = 2.0;
const ROW_PITCH: f32 = 26.0;
const ICON_SIZE: f32 = 18.0;
const TEXT_X: f32 = 29.0;
const BAR_WIDTH: f32 = 100.0;
const BAR_Y: f32 = 23.0;

/// The countdown colours, the original's: plain white, yellow,
/// the flashing red's bright and dark halves, and aqua for a
/// permanent effect. Text colours are 0xRRGGBB.
const COLOR_WHITE: u32 = 0xFF_FFFF;
const COLOR_YELLOW: u32 = 0xFF_FF55;
const COLOR_RED: u32 = 0xFF_5555;
const COLOR_RED_DARK: u32 = 0xAA_0000;
const COLOR_AQUA: u32 = 0x55_FFFF;
/// The duration bar's backing and its colour for an effect the
/// vanilla table does not know (a content mod's own effect).
const BAR_BACKING: u32 = 0xFF26_2626;
const COLOR_UNKNOWN_EFFECT: u32 = 0xC0_C0C0;

/// Vanilla's `MobEffects` colours — each effect's authentic
/// potion colour — in registration order, as the game's own
/// effect table lists them (`horizon_client_core::potions`):
/// the bar wears the colour the game itself assigns the effect.
/// Matched by the id's path, so a content effect reusing a
/// vanilla path wears that colour too.
const EFFECT_COLORS: &[(&str, u32)] = &[
    ("speed", 3402751),
    ("slowness", 9154528),
    ("haste", 14270531),
    ("mining_fatigue", 4866583),
    ("strength", 16762624),
    ("instant_health", 16262179),
    ("instant_damage", 11101546),
    ("jump_boost", 16646020),
    ("nausea", 5578058),
    ("regeneration", 13458603),
    ("resistance", 9520880),
    ("fire_resistance", 16750848),
    ("water_breathing", 10017472),
    ("invisibility", 16185078),
    ("blindness", 2039587),
    ("night_vision", 12779366),
    ("hunger", 5797459),
    ("weakness", 4738376),
    ("poison", 8889187),
    ("wither", 7561558),
    ("health_boost", 16284963),
    ("absorption", 2445989),
    ("saturation", 16262179),
    ("glowing", 9740385),
    ("levitation", 13565951),
    ("luck", 5882118),
    ("unluck", 12624973),
    ("slow_falling", 15978425),
    ("conduit_power", 1950417),
    ("dolphins_grace", 8954814),
    ("bad_omen", 745784),
    ("hero_of_the_village", 4521796),
    ("darkness", 2696993),
    ("trial_omen", 1484454),
    ("raid_omen", 14565464),
    ("wind_charged", 12438015),
    ("weaving", 7891290),
    ("oozing", 10092451),
    ("infested", 9214860),
    ("breath_of_the_nautilus", 65518),
];

/// An effect id's path: `minecraft:fire_resistance` is
/// `fire_resistance`, and so is a bare `fire_resistance`.
fn id_path(id: &str) -> &str {
    id.split(':').next_back().unwrap_or(id)
}

/// The effect's display name from its id — `Fire Resistance` for
/// `minecraft:fire_resistance` — the id's own words capitalised,
/// standing in for the translated name (see the crate docs).
fn effect_name(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for (i, word) in id_path(id).split('_').enumerate() {
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

/// The game's own sprite for an effect, named the way the game
/// names it (`potions::push_sprite`): a vanilla effect is
/// `mob_effect/<path>`; a content mod's keeps its namespace in
/// front, `<ns>:mob_effect/<path>`.
fn effect_sprite(id: &str) -> String {
    match id.split_once(':') {
        None => format!("mob_effect/{id}"),
        Some(("minecraft", path)) => format!("mob_effect/{path}"),
        Some((namespace, path)) => format!("{namespace}:mob_effect/{path}"),
    }
}

/// A level in Roman numerals: 1 is I, 2 is II, 4 is IV, 9 is IX.
/// Levels past I are the ones a HUD prints; 0 and anything past
/// 3999 (no real amplifier) print as the plain number's fallback
/// of an empty string for 0 — callers only ask for level >= 1.
fn roman(mut n: u32) -> String {
    if n == 0 || n > 3999 {
        return String::new();
    }
    const TABLE: &[(u32, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, glyph) in TABLE {
        while n >= *value {
            out.push_str(glyph);
            n -= value;
        }
    }
    out
}

/// The name line's text: the effect's name, with its Roman
/// numeral level when it is past I (`Speed II`); level I wears
/// no numeral, as the original shows it.
fn name_line(effect: &Effect) -> String {
    let name = effect_name(&effect.id);
    if effect.amplifier == 0 {
        name
    } else {
        format!("{name} {}", roman(effect.amplifier + 1))
    }
}

/// The whole seconds left in a remaining tick count (the client
/// counts ticks down; the HUD counts seconds).
fn seconds_left(duration_ticks: i32) -> i64 {
    (duration_ticks / 20) as i64
}

/// The countdown's text: `m:ss` with the minutes unbounded
/// (`1:01`, `90:00`), or the infinity symbol for a permanent
/// effect (duration -1), as the original draws it.
fn duration_text(duration_ticks: i32) -> String {
    if duration_ticks < 0 {
        return "∞".to_string();
    }
    let seconds = seconds_left(duration_ticks);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The countdown's colour, the original's bands: white while
/// more than 30 s remain, yellow from 30 s down past 10 s,
/// flashing red at 10 s and under (bright and dark halves half
/// a second apart), and aqua for a permanent effect.
fn duration_color(duration_ticks: i32, flash_on: bool) -> u32 {
    if duration_ticks < 0 {
        return COLOR_AQUA;
    }
    let seconds = seconds_left(duration_ticks);
    if seconds > 30 {
        COLOR_WHITE
    } else if seconds > 10 {
        COLOR_YELLOW
    } else if flash_on {
        COLOR_RED
    } else {
        COLOR_RED_DARK
    }
}

/// The effect's bar colour: its authentic vanilla potion
/// colour, or the neutral silver for an effect the vanilla
/// table does not know.
fn effect_color(id: &str) -> u32 {
    let path = id_path(id);
    EFFECT_COLORS
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, color)| *color)
        .unwrap_or(COLOR_UNKNOWN_EFFECT)
}

/// How much of the bar is left: the remaining time against the
/// longest remaining time seen for the effect since it appeared
/// (its peak), clamped to the bar; a permanent effect's bar is
/// full, and an effect with no time left is empty.
fn bar_fraction(duration_ticks: i32, peak_ticks: i32) -> f32 {
    if duration_ticks < 0 {
        return 1.0;
    }
    if peak_ticks <= 0 {
        return 0.0;
    }
    (duration_ticks as f32 / peak_ticks as f32).clamp(0.0, 1.0)
}

struct StatusEffectHud {
    /// The longest remaining time seen per active effect, by
    /// (id, amplifier): the bar's full length. An entry lives
    /// only while its effect does.
    peaks: HashMap<(String, u32), i32>,
    /// Shapes are on screen: an empty list is sent once when the
    /// last effect goes, not every tick.
    drawn: bool,
}

impl StatusEffectHud {
    /// The rows' shapes for this tick's effects, in draw order:
    /// per effect its background (the ambient frame for an
    /// ambient effect, as the game's own HUD frames them), its
    /// icon, its name, its countdown, and its bar.
    fn shapes(&self, effects: &[Effect], flash_on: bool) -> Vec<Shape> {
        let mut shapes = Vec::new();
        for (i, effect) in effects.iter().enumerate() {
            let y = TOP + i as f32 * ROW_PITCH;
            let background = if effect.ambient {
                "hud/effect_background_ambient"
            } else {
                "hud/effect_background"
            };
            shapes.push(Shape::Sprite(SpriteShape {
                sprite: background.to_string(),
                rect: Rect {
                    x: LEFT,
                    y,
                    w: 24.0,
                    h: 24.0,
                },
                color: 0xFFFF_FFFF,
            }));
            shapes.push(Shape::Sprite(SpriteShape {
                sprite: effect_sprite(&effect.id),
                rect: Rect {
                    x: LEFT + 3.0,
                    y: y + 3.0,
                    w: ICON_SIZE,
                    h: ICON_SIZE,
                },
                color: 0xFFFF_FFFF,
            }));
            shapes.push(Shape::Text(TextShape {
                text: name_line(effect),
                at: Point {
                    x: LEFT + TEXT_X,
                    y: y + 2.0,
                },
                color: COLOR_WHITE,
                centered: false,
                shadow: true,
            }));
            shapes.push(Shape::Text(TextShape {
                text: duration_text(effect.duration),
                at: Point {
                    x: LEFT + TEXT_X,
                    y: y + 12.0,
                },
                color: duration_color(effect.duration, flash_on),
                centered: false,
                shadow: true,
            }));
            let peak = self
                .peaks
                .get(&(effect.id.clone(), effect.amplifier))
                .copied()
                .unwrap_or(effect.duration);
            shapes.push(Shape::Fill(FillShape {
                rect: Rect {
                    x: LEFT,
                    y: y + BAR_Y,
                    w: BAR_WIDTH,
                    h: 2.0,
                },
                color: BAR_BACKING,
            }));
            shapes.push(Shape::Fill(FillShape {
                rect: Rect {
                    x: LEFT,
                    y: y + BAR_Y,
                    w: BAR_WIDTH * bar_fraction(effect.duration, peak),
                    h: 2.0,
                },
                color: 0xFF00_0000 | effect_color(&effect.id),
            }));
        }
        shapes
    }
}

impl Mod for StatusEffectHud {
    fn init() -> Result<(Self, Registration), String> {
        Ok((
            StatusEffectHud {
                peaks: HashMap::new(),
                drawn: false,
            },
            Registration::new().on_tick(),
        ))
    }

    fn tick(&mut self, tick: u64) {
        let effects = game::get_effects();
        if effects.is_empty() {
            self.peaks.clear();
            if self.drawn {
                render::set_hud(&[]);
                self.drawn = false;
            }
            return;
        }
        // The bar's full length is the longest time left seen
        // for the effect since it appeared; a refresh to a
        // longer time raises it, and an effect that is gone
        // takes its peak with it.
        let mut live: Vec<(String, u32)> = Vec::with_capacity(effects.len());
        for effect in &effects {
            let key = (effect.id.clone(), effect.amplifier);
            if effect.duration > 0 {
                let peak = self.peaks.entry(key.clone()).or_insert(effect.duration);
                *peak = (*peak).max(effect.duration);
            }
            live.push(key);
        }
        self.peaks.retain(|key, _| live.contains(key));
        // The urgent flash: half a second bright, half a second
        // dark, from the tick count.
        let flash_on = (tick / 10) % 2 == 0;
        render::set_hud(&self.shapes(&effects, flash_on));
        self.drawn = true;
    }
}

horizon_mod_sdk::export_mod!(StatusEffectHud);

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(id: &str, amplifier: u32, duration: i32) -> Effect {
        Effect {
            id: id.into(),
            amplifier,
            duration,
            ambient: false,
            show_particles: true,
            show_icon: true,
        }
    }

    #[test]
    fn api_level_is_18() {
        assert_eq!(API_LEVEL, 18);
    }

    #[test]
    fn roman_numerals_read_as_vanilla_levels() {
        assert_eq!(roman(1), "I");
        assert_eq!(roman(2), "II");
        assert_eq!(roman(3), "III");
        assert_eq!(roman(4), "IV");
        assert_eq!(roman(5), "V");
        assert_eq!(roman(9), "IX");
        assert_eq!(roman(10), "X");
        assert_eq!(roman(14), "XIV");
        assert_eq!(roman(40), "XL");
        assert_eq!(roman(0), "");
    }

    #[test]
    fn names_come_from_the_ids_words() {
        assert_eq!(effect_name("minecraft:speed"), "Speed");
        assert_eq!(effect_name("minecraft:fire_resistance"), "Fire Resistance");
        assert_eq!(effect_name("minecraft:breath_of_the_nautilus"), "Breath Of The Nautilus");
        assert_eq!(effect_name("speed"), "Speed");
        assert_eq!(name_line(&effect("minecraft:speed", 0, 100)), "Speed");
        assert_eq!(name_line(&effect("minecraft:speed", 1, 100)), "Speed II");
        assert_eq!(name_line(&effect("minecraft:strength", 3, 100)), "Strength IV");
    }

    #[test]
    fn sprites_are_named_as_the_game_names_them() {
        assert_eq!(effect_sprite("minecraft:speed"), "mob_effect/speed");
        assert_eq!(effect_sprite("speed"), "mob_effect/speed");
        assert_eq!(
            effect_sprite("some_mod:glow"),
            "some_mod:mob_effect/glow"
        );
    }

    #[test]
    fn durations_format_as_mm_ss_and_infinity() {
        assert_eq!(duration_text(-1), "∞");
        assert_eq!(duration_text(0), "0:00");
        assert_eq!(duration_text(19), "0:00");
        assert_eq!(duration_text(20), "0:01");
        assert_eq!(duration_text(1234), "1:01");
        assert_eq!(duration_text(20 * 90 * 60), "90:00");
    }

    #[test]
    fn countdown_colours_follow_the_originals_bands() {
        // Over 30 s: white. 30 s exactly and down past 10 s: yellow.
        assert_eq!(duration_color(31 * 20, true), COLOR_WHITE);
        assert_eq!(duration_color(30 * 20, true), COLOR_YELLOW);
        assert_eq!(duration_color(11 * 20, true), COLOR_YELLOW);
        // 10 s and under: flashing red, bright then dark.
        assert_eq!(duration_color(10 * 20, true), COLOR_RED);
        assert_eq!(duration_color(10 * 20, false), COLOR_RED_DARK);
        assert_eq!(duration_color(1, false), COLOR_RED_DARK);
        // Permanent: aqua, whichever way the flash sits.
        assert_eq!(duration_color(-1, true), COLOR_AQUA);
        assert_eq!(duration_color(-1, false), COLOR_AQUA);
    }

    #[test]
    fn bar_colours_are_the_vanilla_potion_colours() {
        assert_eq!(effect_color("minecraft:speed"), 3402751);
        assert_eq!(effect_color("minecraft:poison"), 8889187);
        assert_eq!(effect_color("minecraft:breath_of_the_nautilus"), 65518);
        assert_eq!(effect_color("minecraft:fire_resistance"), 16750848);
        // A content effect nobody knows: the neutral silver.
        assert_eq!(effect_color("some_mod:glow"), COLOR_UNKNOWN_EFFECT);
        // Every vanilla effect in the table is a distinct row.
        assert_eq!(EFFECT_COLORS.len(), 40);
    }

    #[test]
    fn bar_fraction_runs_from_full_to_empty() {
        assert_eq!(bar_fraction(1200, 1200), 1.0);
        assert_eq!(bar_fraction(600, 1200), 0.5);
        assert_eq!(bar_fraction(0, 1200), 0.0);
        assert_eq!(bar_fraction(-1, 0), 1.0);
        // A refresh past the peak clamps to full.
        assert_eq!(bar_fraction(2400, 1200), 1.0);
    }

    #[test]
    fn shapes_draw_one_row_per_effect_in_order() {
        let hud = StatusEffectHud {
            peaks: HashMap::from([(("minecraft:speed".to_string(), 1), 2400)]),
            drawn: false,
        };
        let effects = vec![
            effect("minecraft:speed", 1, 1234),
            effect("minecraft:regeneration", 0, -1),
        ];
        let shapes = hud.shapes(&effects, true);
        // Six shapes per row: background, icon, name, countdown,
        // bar backing, bar.
        assert_eq!(shapes.len(), 12);
        let texts: Vec<&str> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["Speed II", "1:01", "Regeneration", "∞"]);
        // The second row sits one pitch below the first.
        let Shape::Sprite(second_bg) = &shapes[6] else {
            panic!("second row opens with its background: {:?}", shapes[6]);
        };
        assert_eq!(second_bg.rect.y, TOP + ROW_PITCH);
        assert_eq!(second_bg.sprite, "hud/effect_background");
        // The bar's fill is the effect's colour at the fraction.
        let Shape::Fill(bar) = &shapes[5] else {
            panic!("first row ends with its bar: {:?}", shapes[5]);
        };
        assert_eq!(bar.color, 0xFF00_0000 | 3402751);
        assert!((bar.rect.w - BAR_WIDTH * (1234.0 / 2400.0)).abs() < 1e-3);
    }
}

//! Armor HUD for Kestrel: your worn armor and its durability in a column
//! at the bottom-left of the screen. Press K (rebindable in Options > Mods)
//! to show or hide the durability numbers; a notice in chat says which. A
//! piece about to break flashes. Both switches are also on the mod's
//! settings page (Options > Mods > Armor HUD), and the game keeps them in
//! the mod's storage.
//!
//! Behaviour reimplemented from Armor HUD by Thomas9950 / Thomas-Slot (MIT
//! original); this file shares no code with the original. Credit to Thomas9950
//! for the original mod. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/armor-hud>.
//!
//! Placement approximation: the SDK's `hud` panels are text lines anchored
//! to a screen corner — they cannot draw item icons or sit at the pixel
//! position left of the off-hand slot — so the column is a four-line text
//! panel anchored to the bottom-left corner, in the original's
//! top-to-bottom order (helmet, chestplate, leggings, boots), with a text
//! durability bar where the original draws one. The pulse is the low
//! piece's line flashing red-white every 0.8 s, the original's pulse
//! period; the line's text stays the same, only its colour alternates.
use horizon_mod_sdk::{
    Mod, Registration, chat, game,
    hud::{self, Corner, Line},
    input::{self, Event},
    inventory, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The key's name, as `input.poll` reports it.
const KEY_NAME: &str = "numbers";
/// The default key, as options.txt writes it.
const DEFAULT_KEY: &str = "key.keyboard.k";

/// Storage keys for the two switches.
const NUMBERS_KEY: &str = "numbers";
const ALERTS_KEY: &str = "alerts";

/// The settings page's buttons: show the numbers, flash low pieces.
const NUMBERS_BUTTON: u32 = 0;
const ALERTS_BUTTON: u32 = 1;

/// An armour slot's number in the inventory snapshot, and its line's label:
/// the original's column order, helmet at the top, boots at the bottom.
const ARMOUR: [(u32, &str); 4] = [
    (500, "Helmet"),
    (501, "Chestplate"),
    (502, "Leggings"),
    (503, "Boots"),
];

/// Below this share of durability left, a piece counts as low (10%), as in
/// the original.
const LOW_FRACTION: f32 = 0.10;

/// The text durability bar's length, in characters.
const BAR_LEN: usize = 10;

/// Ticks per pulse phase: 8 ticks is 0.4 s, a full flash 0.8 s, the
/// original's pulse period.
const PULSE_TICKS: u64 = 8;

/// A low piece's line colour at the pulse's bright phase.
const PULSE_BRIGHT: u32 = 0xFF3030;

/// The colour of an empty slot's line, and of a piece at full durability.
const PLAIN: u32 = 0xFFFFFF;
/// An empty slot's line, and a low piece at the pulse's dim phase.
const DIM: u32 = 0xAAAAAA;

/// A stored switch: `true` or `false`, its default when never saved.
fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    match stored.as_deref() {
        Some(b"true") => true,
        Some(b"false") => false,
        _ => default,
    }
}

fn numbers_label(on: bool) -> String {
    format!("Durability numbers: {}", if on { "ON" } else { "OFF" })
}

fn alerts_label(on: bool) -> String {
    format!("Low-durability alerts: {}", if on { "ON" } else { "OFF" })
}

/// The notice the key shows in chat: the original's text, above the hotbar
/// there, in chat here (the SDK has no fading notice).
fn notice_text(on: bool) -> String {
    numbers_label(on)
}

/// Whether the pulse is in its bright phase on this tick.
fn pulse_bright(tick: u64) -> bool {
    (tick / PULSE_TICKS).is_multiple_of(2)
}

// ---- durability, ported from the original's `armor` module ----

/// A piece's durability: damage taken and its maximum, as the inventory
/// snapshot reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Durability {
    damage: u32,
    max_damage: u32,
}

impl Durability {
    /// None for an item that cannot be damaged (max durability 0).
    fn new(damage: u32, max_damage: u32) -> Option<Durability> {
        if max_damage == 0 {
            None
        } else {
            Some(Durability {
                damage: damage.min(max_damage),
                max_damage,
            })
        }
    }

    fn remaining(self) -> u32 {
        self.max_damage - self.damage
    }

    /// Nothing used yet: the bar stays hidden, as vanilla's does.
    fn is_full(self) -> bool {
        self.damage == 0
    }

    /// 1.0 is new, 0.0 is about to break.
    fn fraction(self) -> f32 {
        self.remaining() as f32 / self.max_damage as f32
    }

    /// The bar's filled length in characters, rounded as vanilla's item bar
    /// does.
    fn bar_filled(self) -> usize {
        (BAR_LEN as f32 * self.fraction()).round() as usize
    }

    fn color(self) -> u32 {
        bar_color(self.fraction())
    }

    fn is_low(self) -> bool {
        self.fraction() < LOW_FRACTION
    }

    /// The number the numeric overlay shows: durability left.
    fn numbers(self) -> String {
        self.remaining().to_string()
    }
}

/// Green (full) through yellow to red (empty): the hue runs from 120 to 0
/// degrees as the fraction falls, as vanilla's item bar does. 0xRRGGBB.
fn bar_color(fraction: f32) -> u32 {
    hsv_to_rgb(fraction.clamp(0.0, 1.0) / 3.0, 1.0, 1.0)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
    let h6 = h.clamp(0.0, 1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as u32) % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let byte = |c: f32| (c * 255.0 + 0.5) as u32;
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// `minecraft:diamond_chestplate` reads as `Diamond Chestplate`.
fn display_name(id: &str) -> String {
    let path = id.split(':').next_back().unwrap_or(id);
    path.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One armour slot's line: the label, the piece's name, its durability bar
/// once it has been used, and the remaining durability when the numbers are
/// on. `stack` is the slot's (item id, damage, max damage), or nothing when
/// the slot is empty.
fn piece_line(
    label: &str,
    stack: Option<(&str, u32, u32)>,
    numbers: bool,
    alerts: bool,
    tick: u64,
) -> Line {
    let Some((id, damage, max_damage)) = stack else {
        return Line {
            text: format!("{label}: (empty)"),
            color: DIM,
        };
    };
    let name = display_name(id);
    let Some(d) = Durability::new(damage, max_damage) else {
        return Line {
            text: format!("{label}: {name}"),
            color: PLAIN,
        };
    };
    let mut text = format!("{label}: {name}");
    if numbers {
        text.push(' ');
        text.push_str(&d.numbers());
    }
    if !d.is_full() {
        let filled = d.bar_filled();
        text.push(' ');
        text.push('[');
        text.push_str(&"#".repeat(filled));
        text.push_str(&"-".repeat(BAR_LEN - filled));
        text.push(']');
    }
    let color = if alerts && d.is_low() {
        if pulse_bright(tick) {
            PULSE_BRIGHT
        } else {
            PLAIN
        }
    } else if d.is_full() {
        PLAIN
    } else {
        d.color()
    };
    Line { text, color }
}

struct ArmorHud {
    numbers: bool,
    alerts: bool,
    /// The panel last sent, as (text, colour) pairs — `Line` has no
    /// `PartialEq` — so an unchanged one costs no call.
    last: Vec<(String, u32)>,
}

impl ArmorHud {
    /// Keeps a switch in storage and refreshes the settings page when it
    /// is open.
    fn set_numbers(&mut self, on: bool) {
        self.numbers = on;
        let _ = storage::set(NUMBERS_KEY, if on { b"true" } else { b"false" });
        self.refresh_settings();
    }

    fn set_alerts(&mut self, on: bool) {
        self.alerts = on;
        let _ = storage::set(ALERTS_KEY, if on { b"true" } else { b"false" });
        self.refresh_settings();
    }

    /// The settings page: one button per switch, as the original's.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: NUMBERS_BUTTON,
                label: numbers_label(self.numbers),
            }),
            Widget::Button(ButtonWidget {
                id: ALERTS_BUTTON,
                label: alerts_label(self.alerts),
            }),
        ]);
    }
}

impl Mod for ArmorHud {
    fn init() -> Result<(Self, Registration), String> {
        let numbers = read_bool(storage::get(NUMBERS_KEY), false);
        let alerts = read_bool(storage::get(ALERTS_KEY), true);
        if let Err(e) = input::register_key(KEY_NAME, "Toggle durability numbers", DEFAULT_KEY) {
            // The key is a convenience: the settings page still toggles the
            // numbers without it, so a failed registration only warns.
            horizon_mod_sdk::warn(&format!(
                "Armor HUD: could not register the numbers key ({e}); use Options > Mods > Armor HUD instead"
            ));
        }
        Ok((
            ArmorHud {
                numbers,
                alerts,
                last: Vec::new(),
            },
            Registration::new().on_tick(),
        ))
    }

    fn tick(&mut self, tick: u64) {
        for event in input::poll() {
            if let Event::KeyDown(name) = event
                && name == KEY_NAME
            {
                let on = !self.numbers;
                self.set_numbers(on);
                chat::show(&notice_text(on));
            }
        }
        let Some(_) = game::get_player() else {
            if !self.last.is_empty() {
                hud::clear();
                self.last.clear();
            }
            return;
        };
        let inv = inventory::inventory();
        let now: Vec<Line> = ARMOUR
            .iter()
            .map(|(slot, label)| {
                let stack = inv
                    .slots
                    .iter()
                    .find(|s| s.slot == *slot)
                    .map(|s| (s.id.as_str(), s.damage, s.max_damage));
                piece_line(label, stack, self.numbers, self.alerts, tick)
            })
            .collect();
        let now_key: Vec<(String, u32)> = now.iter().map(|l| (l.text.clone(), l.color)).collect();
        if now_key != self.last {
            hud::set_panel(Corner::BottomLeft, &now);
            self.last = now_key;
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            NUMBERS_BUTTON => self.set_numbers(!self.numbers),
            ALERTS_BUTTON => self.set_alerts(!self.alerts),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(ArmorHud);

#[cfg(test)]
mod tests {
    use super::*;

    /// Compare 0xRRGGBB colours channel by channel, allowing 1 off for rounding.
    fn near(got: u32, want: u32) -> bool {
        (0..3).all(|i| {
            let shift = i * 8;
            let a = ((got >> shift) & 0xFF) as i32;
            let b = ((want >> shift) & 0xFF) as i32;
            (a - b).abs() <= 1
        })
    }

    #[test]
    fn switches_use_their_defaults() {
        assert!(!read_bool(None, false));
        assert!(read_bool(None, true));
        assert!(read_bool(Some(b"true".to_vec()), false));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert_eq!(numbers_label(false), "Durability numbers: OFF");
        assert_eq!(alerts_label(true), "Low-durability alerts: ON");
        assert_eq!(notice_text(true), "Durability numbers: ON");
        assert_eq!(notice_text(false), "Durability numbers: OFF");
    }

    #[test]
    fn unbreakable_items_have_no_durability() {
        assert_eq!(Durability::new(0, 0), None);
    }

    #[test]
    fn damage_is_clamped_to_the_maximum() {
        let d = Durability::new(150, 100).unwrap();
        assert_eq!(d.damage, 100);
        assert_eq!(d.remaining(), 0);
    }

    #[test]
    fn remaining_and_numbers() {
        let d = Durability::new(10, 100).unwrap();
        assert_eq!(d.remaining(), 90);
        assert_eq!(d.numbers(), "90");
    }

    #[test]
    fn only_undamaged_items_are_full() {
        assert!(Durability::new(0, 80).unwrap().is_full());
        assert!(!Durability::new(1, 80).unwrap().is_full());
    }

    #[test]
    fn low_means_under_ten_percent() {
        assert!(!Durability::new(0, 100).unwrap().is_low());
        assert!(!Durability::new(90, 100).unwrap().is_low()); // exactly 10%
        assert!(Durability::new(91, 100).unwrap().is_low());
        assert!(Durability::new(100, 100).unwrap().is_low());
        assert!(Durability::new(79, 80).unwrap().is_low()); // a leather tunic at 1/80
    }

    #[test]
    fn bar_fills_round_like_vanilla() {
        assert_eq!(Durability::new(0, 100).unwrap().bar_filled(), 10);
        assert_eq!(Durability::new(50, 100).unwrap().bar_filled(), 5);
        assert_eq!(Durability::new(95, 100).unwrap().bar_filled(), 1); // 0.5 rounds up
        assert_eq!(Durability::new(100, 100).unwrap().bar_filled(), 0);
    }

    #[test]
    fn colour_runs_green_yellow_red() {
        assert!(near(bar_color(1.0), 0x00FF00));
        assert!(near(bar_color(0.5), 0xFFFF00));
        assert!(near(bar_color(0.0), 0xFF0000));
        assert!(near(bar_color(0.25), 0xFF8000));
        // Out of range inputs are clamped, not wrapped.
        assert!(near(bar_color(2.0), 0x00FF00));
        assert!(near(bar_color(-1.0), 0xFF0000));
    }

    #[test]
    fn names_drop_the_namespace_and_title_case() {
        assert_eq!(
            display_name("minecraft:diamond_chestplate"),
            "Diamond Chestplate"
        );
        assert_eq!(
            display_name("minecraft:netherite_helmet"),
            "Netherite Helmet"
        );
        assert_eq!(display_name("minecraft:leather_boots"), "Leather Boots");
    }

    #[test]
    fn pulse_flips_every_eight_ticks() {
        assert!(pulse_bright(0));
        assert!(pulse_bright(7));
        assert!(!pulse_bright(8));
        assert!(!pulse_bright(15));
        assert!(pulse_bright(16));
    }

    #[test]
    fn an_empty_slot_is_dim() {
        let line = piece_line("Helmet", None, false, true, 1);
        assert_eq!(line.text, "Helmet: (empty)");
        assert_eq!(line.color, DIM);
    }

    #[test]
    fn a_full_piece_shows_no_bar() {
        let line = piece_line(
            "Boots",
            Some(("minecraft:diamond_boots", 0, 429)),
            false,
            true,
            1,
        );
        assert_eq!(line.text, "Boots: Diamond Boots");
        assert_eq!(line.color, PLAIN);
        let numbered = piece_line(
            "Boots",
            Some(("minecraft:diamond_boots", 0, 429)),
            true,
            true,
            1,
        );
        assert_eq!(numbered.text, "Boots: Diamond Boots 429");
    }

    #[test]
    fn a_damaged_piece_shows_its_bar_and_number() {
        let stack = Some(("minecraft:diamond_helmet", 10, 363));
        let line = piece_line("Helmet", stack, false, true, 1);
        assert_eq!(line.text, "Helmet: Diamond Helmet [##########]");
        let numbered = piece_line("Helmet", stack, true, true, 1);
        assert_eq!(numbered.text, "Helmet: Diamond Helmet 353 [##########]");
        let half = piece_line(
            "Helmet",
            Some(("minecraft:diamond_helmet", 181, 363)),
            true,
            true,
            1,
        );
        assert_eq!(half.text, "Helmet: Diamond Helmet 182 [#####-----]");
    }

    #[test]
    fn an_undamageable_piece_shows_no_bar() {
        let line = piece_line(
            "Helmet",
            Some(("minecraft:player_head", 0, 0)),
            true,
            true,
            1,
        );
        assert_eq!(line.text, "Helmet: Player Head");
        assert_eq!(line.color, PLAIN);
    }

    #[test]
    fn a_low_piece_flashes_and_alerts_can_turn_it_off() {
        let stack = Some(("minecraft:diamond_chestplate", 350, 363));
        let bright = piece_line("Chestplate", stack, false, true, 1);
        assert_eq!(bright.text, "Chestplate: Diamond Chestplate [----------]");
        assert_eq!(bright.color, PULSE_BRIGHT);
        let dim = piece_line("Chestplate", stack, false, true, 8);
        assert_eq!(dim.color, PLAIN);
        let quiet = piece_line("Chestplate", stack, false, false, 1);
        // Alerts off: the ordinary durability colour, nearly red this low.
        assert_eq!(quiet.color, 0xFF1200);
    }
}

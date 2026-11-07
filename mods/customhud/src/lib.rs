//! Beautiful Day Counter for Kestrel: a port of SCARABOCCHIO100 /
//! sonoFrangu's CustomHUD (the Fabric/NeoForge CustomHUD as a native
//! Horizon mod), reimplemented here as a Kestrel `.kmod` on the SDK
//! (`docs/mods.md`). The port lives at
//! `https://github.com/NepThunder/Kestrel-mods/tree/main/mods/customhud`.
//!
//! What it shows, every tick, in a corner panel the player picks
//! (top-left by default, as the original sits at the top):
//!
//! * `Day: N` — days elapsed since the world was created, exactly as the
//!   original counts them (`world.day_time / 24000`, so the first day reads
//!   `Day: 0`, the same as vanilla `/time query day`).
//! * The in-game clock, 24-hour by default or 12-hour (`7:30 PM`).
//! * A day/night bar that follows the cycle: 20 cells, filled in proportion
//!   to the time since midnight, coloured by the original's four themes
//!   (Classic, Pastel, Colorblind, Mono) with the same colour stops and
//!   interpolation; optional night marks at the sleep window's ends.
//! * The moon phase (`days % 8`, 0 is the full moon, as in vanilla).
//! * A countdown to night (`Night in M:SS`) or to day (`Day in M:SS`).
//! * Milestone days (50, 100, 365, every 100): the day line turns rainbow
//!   and names the original's icon — a golden apple under 500 days, a totem
//!   of undying from 500 on.
//!
//! Deliberate gaps against the original, all forced by what the SDK offers:
//!
//! * No milestone sound. The SDK has no audio capability, so the level-up
//!   sound the original plays on a milestone day cannot be reproduced; the
//!   rainbow badge is the whole celebration. The settings page says so.
//! * No game-mode icon. The SDK's `game-read` player record carries no game
//!   mode (and no hardcore flag), so the heart / grass block / sword /
//!   ender eye next to the day counter cannot be shown.
//! * No bed icon. The SDK exposes no weather, so "can sleep" is judged by
//!   the sleep window alone (thunder is not known); when the sleep icon is
//!   on and the window is open the clock line reads `[Bed] HH:MM`.
//! * No pixel layout, scale, opacity or offsets. The SDK's `hud` panels are
//!   text lines in a screen corner; the original's centred bar, sprites and
//!   drag-to-place simple mode have no equivalent. Simple mode here only
//!   shows the day line on its own.
//! * English only. The SDK exposes no locale, so the original's six
//!   languages collapse to English.
//!
//! Settings live on the mod's page in Options > Mods (capability `ui`) and
//! persist in `storage` under the key `config`, in the original's
//! `key=value` format with the original's field names, so a config file
//! copied from the native mod still loads. `/daycounter` prints the same
//! summary in chat.

use horizon_mod_sdk::{
    Mod, Registration,
    game::{self, Clock, Player},
    hud::{self, Corner, Line},
    storage,
    ui::{self, ButtonWidget, ListWidget, TextWidget, Widget},
};

/// Ticks of the day in which sleeping is possible with clear weather, as
/// in vanilla (and the original: `SLEEP_START` / `SLEEP_END`).
const SLEEP_START: u64 = 12542;
const SLEEP_END: u64 = 23460;

/// Colour stops of the day/night bar, in ticks since midnight, with the
/// intermediate stops (dawn, morning sun) that keep the RGB interpolation
/// from passing through grey or brown. Copied from the original.
const COLOR_TICKS: [u64; 7] = [0, 4500, 6000, 7500, 12000, 18000, 24000];
const CLASSIC: [u32; 7] = [
    0x6633CC, 0xFF7A4D, 0xFFE633, 0xFFF5C0, 0x3399FF, 0xFF4D80, 0x6633CC,
];
const PASTEL: [u32; 7] = [
    0x9F8FD9, 0xFFB59E, 0xFFF0A0, 0xFFFBE0, 0xA8D8FF, 0xFFB3C8, 0x9F8FD9,
];
/// Okabe-Ito palette: distinguishable with red/green colour blindness.
const COLORBLIND: [u32; 7] = [
    0x2B3A8C, 0xE69F00, 0xF0E442, 0xF5EEA0, 0x56B4E9, 0xD55E00, 0x2B3A8C,
];
const MONO: [u32; 7] = [
    0x3A3A3A, 0x8A8A8A, 0xD0D0D0, 0xEEEEEE, 0xFFFFFF, 0x9A9A9A, 0x3A3A3A,
];

/// Cells of the text bar. The original draws a 182 px boss-bar sprite;
/// the SDK's `hud` panels are text, so the bar is 20 characters.
const BAR_CELLS: usize = 20;

/// Storage key for the settings, as the original names it.
const CONFIG_KEY: &str = "config";

/// The bar's colour theme, in the original's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarTheme {
    Classic,
    Pastel,
    Colorblind,
    Mono,
}

impl BarTheme {
    fn from_name(value: &str) -> Self {
        match value {
            "PASTEL" => BarTheme::Pastel,
            "COLORBLIND" => BarTheme::Colorblind,
            "MONO" => BarTheme::Mono,
            _ => BarTheme::Classic,
        }
    }
    fn table(self) -> &'static [u32; 7] {
        match self {
            BarTheme::Classic => &CLASSIC,
            BarTheme::Pastel => &PASTEL,
            BarTheme::Colorblind => &COLORBLIND,
            BarTheme::Mono => &MONO,
        }
    }
}

/// What to show outside the Overworld, where the clock has no meaning.
/// The original's `OtherDimensions`: `Show` is its default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtherDimensions {
    Show,
    Unknown,
    Hide,
}

impl OtherDimensions {
    fn from_name(value: &str) -> Self {
        match value {
            "UNKNOWN" => OtherDimensions::Unknown,
            "HIDE" => OtherDimensions::Hide,
            _ => OtherDimensions::Show,
        }
    }
    fn name(self) -> &'static str {
        match self {
            OtherDimensions::Show => "SHOW",
            OtherDimensions::Unknown => "UNKNOWN",
            OtherDimensions::Hide => "HIDE",
        }
    }
}

/// The settings, with the original's defaults. Scale, opacity, offsets and
/// background have no meaning for the SDK's corner text panels and are not
/// stored; the parser still accepts (and ignores) them, so a config file
/// copied from the native mod loads.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub show_day_counter: bool,
    pub show_clock: bool,
    pub show_bossbar: bool,
    pub show_night_ticks: bool,
    pub show_sleep_icon: bool,
    pub show_moon_phase: bool,
    pub show_night_countdown: bool,
    pub bar_theme: BarTheme,
    pub use_12_hour_format: bool,
    pub corner: Corner,
    pub other_dimensions: OtherDimensions,
    pub milestones: bool,
    pub simple_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_day_counter: true,
            show_clock: true,
            show_bossbar: true,
            show_night_ticks: false,
            show_sleep_icon: false,
            show_moon_phase: false,
            show_night_countdown: false,
            bar_theme: BarTheme::Classic,
            use_12_hour_format: false,
            corner: Corner::TopLeft,
            other_dimensions: OtherDimensions::Show,
            milestones: true,
            simple_mode: false,
        }
    }
}

impl Config {
    fn corner_name(corner: Corner) -> &'static str {
        match corner {
            Corner::TopLeft => "TOP_LEFT",
            Corner::TopRight => "TOP_RIGHT",
            Corner::BottomLeft => "BOTTOM_LEFT",
            Corner::BottomRight => "BOTTOM_RIGHT",
        }
    }

    fn corner_from_name(value: &str) -> Corner {
        match value {
            "TOP_RIGHT" => Corner::TopRight,
            "BOTTOM_LEFT" => Corner::BottomLeft,
            "BOTTOM_RIGHT" => Corner::BottomRight,
            _ => Corner::TopLeft,
        }
    }

    /// Parses the original's `key=value` format; unknown keys and invalid
    /// values are ignored, so a hand-edited file can never break the mod.
    pub fn parse(text: &str) -> Self {
        let mut c = Config::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            let flag = value == "true";
            match key {
                "showDayCounter" => c.show_day_counter = flag,
                "showClock" => c.show_clock = flag,
                "showBossbar" => c.show_bossbar = flag,
                "showNightTicks" => c.show_night_ticks = flag,
                "showSleepIcon" => c.show_sleep_icon = flag,
                "showMoonPhase" => c.show_moon_phase = flag,
                "showNightCountdown" => c.show_night_countdown = flag,
                "use12HourFormat" => c.use_12_hour_format = flag,
                "milestones" => c.milestones = flag,
                "simpleMode" => c.simple_mode = flag,
                "barTheme" => c.bar_theme = BarTheme::from_name(value),
                "otherDimensions" => c.other_dimensions = OtherDimensions::from_name(value),
                "corner" => c.corner = Self::corner_from_name(value),
                // The original's top/bottom flag, kept for old configs.
                "positionBottom" => {
                    c.corner = if flag {
                        Corner::BottomLeft
                    } else {
                        Corner::TopLeft
                    }
                }
                _ => {}
            }
        }
        c
    }

    /// The original's serialisation, with `corner` in place of its
    /// `positionBottom` (which `parse` still accepts).
    pub fn serialize(&self) -> String {
        let theme = match self.bar_theme {
            BarTheme::Classic => "CLASSIC",
            BarTheme::Pastel => "PASTEL",
            BarTheme::Colorblind => "COLORBLIND",
            BarTheme::Mono => "MONO",
        };
        [
            format!("showDayCounter={}", self.show_day_counter),
            format!("showClock={}", self.show_clock),
            format!("showBossbar={}", self.show_bossbar),
            format!("showNightTicks={}", self.show_night_ticks),
            format!("showSleepIcon={}", self.show_sleep_icon),
            format!("showMoonPhase={}", self.show_moon_phase),
            format!("showNightCountdown={}", self.show_night_countdown),
            format!("barTheme={theme}"),
            format!("use12HourFormat={}", self.use_12_hour_format),
            format!("corner={}", Self::corner_name(self.corner)),
            format!("otherDimensions={}", self.other_dimensions.name()),
            format!("milestones={}", self.milestones),
            format!("simpleMode={}", self.simple_mode),
        ]
        .join("\n")
            + "\n"
    }
}

/// Days elapsed since the world was created: the original's
/// `day_time.div_euclid(24000)`, so the first day is day 0.
pub fn days_elapsed(day_time: u64) -> u64 {
    day_time / 24000
}

/// Ticks within the current day, 0 at 6:00.
pub fn time_of_day(day_time: u64) -> u64 {
    day_time % 24000
}

/// `06:00`, or `6:00 AM` in 12-hour mode. The original's `time_text`.
pub fn time_text(tod: u64, use_12h: bool) -> String {
    let hours24 = (tod / 1000 + 6) % 24;
    let minutes = tod % 1000 * 60 / 1000;
    if use_12h {
        let h = if hours24.is_multiple_of(12) {
            12
        } else {
            hours24 % 12
        };
        let suffix = if hours24 < 12 { "AM" } else { "PM" };
        format!("{h}:{minutes:02} {suffix}")
    } else {
        format!("{hours24:02}:{minutes:02}")
    }
}

/// `Night in M:SS` by day, `Day in M:SS` by night, in real time. The
/// original's `countdown_text`.
pub fn countdown_text(tod: u64) -> String {
    let night = (SLEEP_START..SLEEP_END).contains(&tod);
    let ticks_left = if night {
        SLEEP_END - tod
    } else {
        (SLEEP_START - tod).rem_euclid(24000)
    };
    let seconds = ticks_left / 20;
    let left = format!("{}:{:02}", seconds / 60, seconds % 60);
    if night {
        format!("Day in {left}")
    } else {
        format!("Night in {left}")
    }
}

/// Milestone days: 50, 100, 365, and every 100. The original's
/// `is_milestone`.
pub fn is_milestone(days: u64) -> bool {
    days > 0 && (days == 50 || days == 365 || days.is_multiple_of(100))
}

/// The item the original draws beside the day counter on a milestone day:
/// a golden apple under 500 days, a totem of undying from 500 on.
pub fn milestone_item(days: u64) -> &'static str {
    if days >= 500 {
        "totem of undying"
    } else {
        "golden apple"
    }
}

/// Whether the player could sleep now. The original also requires clear
/// weather; the SDK exposes no weather, so thunder is not known and the
/// window alone decides (see the module docs).
pub fn can_sleep(overworld: bool, tod: u64) -> bool {
    overworld && (SLEEP_START..SLEEP_END).contains(&tod)
}

/// The bar's colour at `ticks` since midnight, interpolating between the
/// theme's stops. The original's `bar_color`.
pub fn bar_color(theme: BarTheme, ticks: u64) -> u32 {
    let colors = theme.table();
    let mut i = 1;
    while i < COLOR_TICKS.len() - 1 && ticks >= COLOR_TICKS[i] {
        i += 1;
    }
    let t = (ticks - COLOR_TICKS[i - 1]) as f32 / (COLOR_TICKS[i] - COLOR_TICKS[i - 1]) as f32;
    lerp_rgb(colors[i - 1], colors[i], t)
}

fn lerp_rgb(a: u32, b: u32, t: f32) -> u32 {
    let ch = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xFF) as f32, ((b >> shift) & 0xFF) as f32);
        (x + t * (y - x)) as u32 & 0xFF
    };
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// `Mth.hsvToRgb`, for the milestone rainbow. The original cycles the hue
/// over 4 seconds of wall time; here the game tick does it (80 ticks).
fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
    let h = (hue * 6.0) as i32 % 6;
    let f = hue * 6.0 - h as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - f * saturation);
    let t = value * (1.0 - (1.0 - f) * saturation);
    let (r, g, b) = match h {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    };
    let c = |x: f32| ((x * 255.0) as i32).clamp(0, 255) as u32;
    (c(r) << 16) | (c(g) << 8) | c(b)
}

/// The moon phase, 0 to 7: 0 is the full moon, 4 the new moon, as in
/// vanilla (and the original: `days.rem_euclid(8)`).
pub fn moon_phase(days: u64) -> u64 {
    days % 8
}

pub fn moon_phase_name(phase: u64) -> &'static str {
    match phase {
        0 => "Full Moon",
        1 => "Waning Gibbous",
        2 => "Last Quarter",
        3 => "Waning Crescent",
        4 => "New Moon",
        5 => "Waxing Crescent",
        6 => "First Quarter",
        _ => "Waxing Gibbous",
    }
}

/// The day/night bar as text: `=` for the time since midnight, `-` for the
/// time left, `|` at the sleep window's ends when night ticks are on. The
/// original draws a 182 px boss-bar sprite with the same progress and the
/// same two tick marks.
pub fn bar_text(tod: u64, night_ticks: bool) -> String {
    let since_midnight = (tod + 6000) % 24000;
    let filled = (BAR_CELLS as u64 * since_midnight / 24000) as usize;
    let mut cells = vec!['-'; BAR_CELLS];
    for cell in cells.iter_mut().take(filled) {
        *cell = '=';
    }
    if night_ticks {
        for tick in [SLEEP_START, SLEEP_END] {
            let at = (BAR_CELLS as u64 * ((tick + 6000) % 24000) / 24000) as usize;
            if at < BAR_CELLS {
                cells[at] = '|';
            }
        }
    }
    cells.into_iter().collect()
}

fn is_overworld(player: &Player) -> bool {
    player.dimension == "minecraft:overworld"
}

/// The panel for this tick: its corner and lines, or `None` when nothing
/// is visible. `tick` drives the milestone rainbow (80 ticks a cycle).
pub fn panel_lines(
    config: &Config,
    clock: &Clock,
    player: &Player,
    tick: u64,
) -> Option<(Corner, Vec<Line>)> {
    let days = days_elapsed(clock.day_time);
    let tod = time_of_day(clock.day_time);
    let overworld = is_overworld(player);
    let hide_time = !overworld && config.other_dimensions == OtherDimensions::Hide;
    let unknown_time = !overworld && config.other_dimensions == OtherDimensions::Unknown;
    let time_extras = !hide_time && !unknown_time;

    let milestone = config.milestones && is_milestone(days);
    let day_line = |color: u32| {
        let text = if milestone {
            format!("Day: {days} (Milestone: {})", milestone_item(days))
        } else {
            format!("Day: {days}")
        };
        Line { text, color }
    };

    if config.simple_mode {
        // Simple mode: only the day counter, as the original's 1.7.0 mode.
        let color = if milestone {
            hsv_to_rgb((tick % 80) as f32 / 80.0, 0.7, 1.0)
        } else {
            0xFFFFFF
        };
        return Some((config.corner, vec![day_line(color)]));
    }

    let show_days = config.show_day_counter;
    let show_clock = config.show_clock && !hide_time;
    let show_bar = config.show_bossbar && !hide_time;
    let show_countdown = config.show_night_countdown && time_extras;
    let show_moon = config.show_moon_phase && time_extras;
    if !show_days && !show_clock && !show_bar {
        return None;
    }

    let mut lines = Vec::new();
    if show_days {
        let color = if milestone {
            hsv_to_rgb((tick % 80) as f32 / 80.0, 0.7, 1.0)
        } else {
            0xFFFFFF
        };
        lines.push(day_line(color));
    }
    if show_clock {
        let time = if unknown_time {
            "??:??".to_string()
        } else {
            time_text(tod, config.use_12_hour_format)
        };
        let text = if config.show_sleep_icon && time_extras && can_sleep(overworld, tod) {
            format!("[Bed] {time}")
        } else {
            time
        };
        lines.push(Line {
            text,
            color: 0xFFFFFF,
        });
    }
    if show_bar {
        let text = if unknown_time {
            "-".repeat(BAR_CELLS)
        } else {
            bar_text(tod, config.show_night_ticks)
        };
        let color = if unknown_time {
            0xBBBBBB
        } else {
            bar_color(config.bar_theme, (tod + 6000) % 24000)
        };
        lines.push(Line { text, color });
    }
    if show_countdown {
        lines.push(Line {
            text: countdown_text(tod),
            color: 0xBBBBBB,
        });
    }
    if show_moon {
        lines.push(Line {
            text: format!("Moon: {}", moon_phase_name(moon_phase(days))),
            color: 0xFFFFFF,
        });
    }
    Some((config.corner, lines))
}

/// The `/daycounter` summary: day, clock, moon and countdown together,
/// whatever the panel itself shows.
fn summary(config: &Config, clock: &Clock, player: &Player) -> String {
    let days = days_elapsed(clock.day_time);
    let tod = time_of_day(clock.day_time);
    let overworld = is_overworld(player);
    let time = match config.other_dimensions {
        _ if overworld => time_text(tod, config.use_12_hour_format),
        OtherDimensions::Unknown => "??:??".to_string(),
        _ => time_text(tod, config.use_12_hour_format),
    };
    let mut parts = vec![
        format!("Day: {days}"),
        time,
        format!("Moon: {}", moon_phase_name(moon_phase(days))),
        countdown_text(tod),
    ];
    if config.milestones && is_milestone(days) {
        parts.push(format!("Milestone: {}", milestone_item(days)));
    }
    parts.join(", ")
}

// Widget ids on the settings page (Options > Mods).
const W_THEME: u32 = 1;
const W_CLOCK_FORMAT: u32 = 2;
const W_POSITION: u32 = 3;
const W_DIMENSIONS: u32 = 4;
const W_DAY: u32 = 10;
const W_CLOCK: u32 = 11;
const W_BAR: u32 = 12;
const W_NIGHT_TICKS: u32 = 13;
const W_SLEEP_ICON: u32 = 14;
const W_MOON: u32 = 15;
const W_COUNTDOWN: u32 = 16;
const W_MILESTONES: u32 = 17;
const W_SIMPLE: u32 = 18;

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

fn corner_index(corner: Corner) -> u32 {
    match corner {
        Corner::TopLeft => 0,
        Corner::TopRight => 1,
        Corner::BottomLeft => 2,
        Corner::BottomRight => 3,
    }
}

/// The settings page: lists for the choices, buttons that cycle the
/// on/off options. A button's click comes back as `select(id, 0)`.
fn settings_widgets(config: &Config) -> Vec<Widget> {
    let button = |id: u32, label: &str, on: bool| {
        Widget::Button(ButtonWidget {
            id,
            label: format!("{label}: {}", on_off(on)),
        })
    };
    let label = |id: u32, text: &str| {
        Widget::Text(TextWidget {
            id,
            text: text.to_string(),
            color: 0,
        })
    };
    vec![
        label(
            0,
            "Beautiful Day Counter — a day counter, clock and day/night bar.",
        ),
        label(30, "Bar colours"),
        Widget::List(ListWidget {
            id: W_THEME,
            rows: ["Classic", "Pastel", "Colorblind", "Mono"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            selected: Some(match config.bar_theme {
                BarTheme::Classic => 0,
                BarTheme::Pastel => 1,
                BarTheme::Colorblind => 2,
                BarTheme::Mono => 3,
            }),
        }),
        label(31, "Clock"),
        Widget::List(ListWidget {
            id: W_CLOCK_FORMAT,
            rows: vec!["24-hour".to_string(), "12-hour".to_string()],
            selected: Some(u32::from(config.use_12_hour_format)),
        }),
        label(32, "Position"),
        Widget::List(ListWidget {
            id: W_POSITION,
            rows: vec![
                "Top left".to_string(),
                "Top right".to_string(),
                "Bottom left".to_string(),
                "Bottom right".to_string(),
            ],
            selected: Some(corner_index(config.corner)),
        }),
        label(33, "Outside the Overworld"),
        Widget::List(ListWidget {
            id: W_DIMENSIONS,
            rows: vec![
                "Show the time".to_string(),
                "Show ??:??".to_string(),
                "Hide clock and bar".to_string(),
            ],
            selected: Some(match config.other_dimensions {
                OtherDimensions::Show => 0,
                OtherDimensions::Unknown => 1,
                OtherDimensions::Hide => 2,
            }),
        }),
        button(W_DAY, "Day counter", config.show_day_counter),
        button(W_CLOCK, "Clock", config.show_clock),
        button(W_BAR, "Day/night bar", config.show_bossbar),
        button(
            W_NIGHT_TICKS,
            "Night marks on the bar",
            config.show_night_ticks,
        ),
        button(
            W_SLEEP_ICON,
            "Bed icon when you can sleep",
            config.show_sleep_icon,
        ),
        button(W_MOON, "Moon phase", config.show_moon_phase),
        button(
            W_COUNTDOWN,
            "Countdown to night/day",
            config.show_night_countdown,
        ),
        button(W_MILESTONES, "Milestone days", config.milestones),
        button(W_SIMPLE, "Simple mode (day only)", config.simple_mode),
        label(
            20,
            "Milestone days show a rainbow badge. No sound plays: the SDK has no audio capability.",
        ),
    ]
}

/// Applies one settings-page pick to the config; `true` when it changed
/// anything. Pure, so the unit tests can drive the whole page.
fn apply_select(config: &mut Config, id: u32, row: u32) -> bool {
    match id {
        W_THEME => {
            let theme = match row {
                1 => BarTheme::Pastel,
                2 => BarTheme::Colorblind,
                3 => BarTheme::Mono,
                _ => BarTheme::Classic,
            };
            if theme != config.bar_theme {
                config.bar_theme = theme;
                return true;
            }
        }
        W_CLOCK_FORMAT => {
            let use_12h = row == 1;
            if use_12h != config.use_12_hour_format {
                config.use_12_hour_format = use_12h;
                return true;
            }
        }
        W_POSITION => {
            let corner = match row {
                1 => Corner::TopRight,
                2 => Corner::BottomLeft,
                3 => Corner::BottomRight,
                _ => Corner::TopLeft,
            };
            if corner != config.corner {
                config.corner = corner;
                return true;
            }
        }
        W_DIMENSIONS => {
            let dims = match row {
                1 => OtherDimensions::Unknown,
                2 => OtherDimensions::Hide,
                _ => OtherDimensions::Show,
            };
            if dims != config.other_dimensions {
                config.other_dimensions = dims;
                return true;
            }
        }
        W_DAY => config.show_day_counter = !config.show_day_counter,
        W_CLOCK => config.show_clock = !config.show_clock,
        W_BAR => config.show_bossbar = !config.show_bossbar,
        W_NIGHT_TICKS => config.show_night_ticks = !config.show_night_ticks,
        W_SLEEP_ICON => config.show_sleep_icon = !config.show_sleep_icon,
        W_MOON => config.show_moon_phase = !config.show_moon_phase,
        W_COUNTDOWN => config.show_night_countdown = !config.show_night_countdown,
        W_MILESTONES => config.milestones = !config.milestones,
        W_SIMPLE => config.simple_mode = !config.simple_mode,
        _ => return false,
    }
    true
}

struct CustomHud {
    config: Config,
    /// The panel last sent (corner, then each line's text and colour), so
    /// an unchanged one costs no call.
    last: Option<(u32, Vec<(String, u32)>)>,
}

fn load_config() -> Config {
    match storage::get(CONFIG_KEY) {
        Some(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Config::parse(&text),
            Err(_) => Config::default(),
        },
        None => Config::default(),
    }
}

impl CustomHud {
    fn save(&self) {
        if let Err(e) = storage::set(CONFIG_KEY, Config::serialize(&self.config).as_bytes()) {
            horizon_mod_sdk::warn(&format!("Could not save settings: {e}"));
        }
    }

    fn push_settings(&self) {
        ui::set_settings(&settings_widgets(&self.config));
    }

    fn refresh(&mut self, tick: u64) {
        let Some(player) = game::get_player() else {
            if self.last.is_some() {
                hud::clear();
                self.last = None;
            }
            return;
        };
        let Some(clock) = game::get_clock() else {
            if self.last.is_some() {
                hud::clear();
                self.last = None;
            }
            return;
        };
        let panel = panel_lines(&self.config, &clock, &player, tick);
        let now = panel.as_ref().map(|(corner, lines)| {
            (
                corner_index(*corner),
                lines
                    .iter()
                    .map(|l| (l.text.clone(), l.color))
                    .collect::<Vec<_>>(),
            )
        });
        if now != self.last {
            match panel {
                Some((corner, lines)) => hud::set_panel(corner, &lines),
                None => hud::clear(),
            }
            self.last = now;
        }
    }
}

impl Mod for CustomHud {
    fn init() -> Result<(Self, Registration), String> {
        let config = load_config();
        let hooks = Registration::new().command("daycounter").on_tick();
        let state = CustomHud { config, last: None };
        state.push_settings();
        Ok((state, hooks))
    }

    fn tick(&mut self, tick: u64) {
        self.refresh(tick);
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "daycounter" {
            return Err("This command does nothing".into());
        }
        let player = game::get_player().ok_or("Not in a world")?;
        let clock = game::get_clock().ok_or("Not in a world")?;
        Ok(Some(summary(&self.config, &clock, &player)))
    }

    fn ui_select(&mut self, id: u32, row: u32) {
        if apply_select(&mut self.config, id, row) {
            self.save();
            self.push_settings();
        }
    }

    fn ui_settings_opened(&mut self) {
        self.push_settings();
    }
}

horizon_mod_sdk::export_mod!(CustomHud);

#[cfg(test)]
mod tests {
    use super::*;

    fn player_in(dimension: &str) -> Player {
        Player {
            x: 0.0,
            y: 64.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            health: 20.0,
            food: 20,
            dimension: dimension.to_string(),
        }
    }

    fn clock(day_time: u64) -> Clock {
        Clock {
            game_time: day_time,
            day_time,
        }
    }

    fn texts(panel: &Option<(Corner, Vec<Line>)>) -> Vec<String> {
        panel
            .as_ref()
            .map(|(_, lines)| lines.iter().map(|l| l.text.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn clock_matches_vanilla_time() {
        // The original's own cases.
        assert_eq!(time_text(0, false), "06:00");
        assert_eq!(time_text(6000, false), "12:00");
        assert_eq!(time_text(18000, false), "00:00");
        assert_eq!(time_text(18000, true), "12:00 AM");
        assert_eq!(time_text(13500, true), "7:30 PM");
        assert_eq!(time_text(0, true), "6:00 AM");
    }

    #[test]
    fn days_count_elapsed_days_like_the_original() {
        assert_eq!(days_elapsed(1000), 0);
        assert_eq!(days_elapsed(24000 * 5 + 6000), 5);
        assert_eq!(days_elapsed(24000 * 100), 100);
        let panel = panel_lines(
            &Config::default(),
            &clock(1000),
            &player_in("minecraft:overworld"),
            0,
        );
        assert_eq!(texts(&panel)[0], "Day: 0");
    }

    #[test]
    fn countdown_to_night_and_day() {
        assert_eq!(countdown_text(12542 - 200), "Night in 0:10");
        assert_eq!(countdown_text(23460 - 400), "Day in 0:20");
        assert_eq!(countdown_text(0), "Night in 10:27");
        assert_eq!(countdown_text(18000), "Day in 4:33");
    }

    #[test]
    fn milestones_are_50_100_365_and_every_100() {
        for d in [50, 100, 200, 365, 500, 1000] {
            assert!(is_milestone(d), "{d}");
        }
        for d in [0, 1, 49, 51, 99, 101, 364, 366] {
            assert!(!is_milestone(d), "{d}");
        }
        assert_eq!(milestone_item(100), "golden apple");
        assert_eq!(milestone_item(499), "golden apple");
        assert_eq!(milestone_item(500), "totem of undying");
    }

    #[test]
    fn bar_colors_hit_the_original_stops() {
        assert_eq!(bar_color(BarTheme::Classic, 0), 0x6633CC);
        assert_eq!(bar_color(BarTheme::Classic, 6000), 0xFFE633);
        assert_eq!(bar_color(BarTheme::Classic, 12000), 0x3399FF);
        assert_eq!(bar_color(BarTheme::Pastel, 0), 0x9F8FD9);
        assert_eq!(bar_color(BarTheme::Colorblind, 6000), 0xF0E442);
        assert_eq!(bar_color(BarTheme::Mono, 23999) >> 16, 0x3A);
    }

    #[test]
    fn bar_follows_the_cycle() {
        assert_eq!(bar_text(0, false), "=====---------------");
        assert_eq!(bar_text(6000, false), "==========----------");
        assert_eq!(bar_text(12000, false), "===============-----");
        assert_eq!(bar_text(18000, false), "--------------------");
        // Night marks at the sleep window's ends (12542 -> cell 15,
        // 23460 -> cell 4, after the +6000 midnight shift).
        assert_eq!(bar_text(6000, true), "====|=====-----|----");
    }

    #[test]
    fn moon_phases_follow_vanilla() {
        assert_eq!(moon_phase(0), 0);
        assert_eq!(moon_phase_name(0), "Full Moon");
        assert_eq!(moon_phase_name(4), "New Moon");
        assert_eq!(moon_phase(100), 4);
        assert_eq!(moon_phase_name(moon_phase(100)), "New Moon");
    }

    #[test]
    fn default_panel_is_day_clock_and_bar() {
        let panel = panel_lines(
            &Config::default(),
            &clock(24000 * 5 + 6000),
            &player_in("minecraft:overworld"),
            0,
        );
        assert_eq!(texts(&panel), ["Day: 5", "12:00", "==========----------"]);
        let (_, lines) = panel.unwrap();
        assert_eq!(lines[2].color, 0x3399FF);
    }

    #[test]
    fn milestone_day_is_badged_and_rainbow() {
        let panel = panel_lines(
            &Config::default(),
            &clock(24000 * 100),
            &player_in("minecraft:overworld"),
            0,
        );
        assert_eq!(texts(&panel)[0], "Day: 100 (Milestone: golden apple)");
        let (_, lines) = panel.unwrap();
        assert_ne!(lines[0].color, 0xFFFFFF);
        // Day 600: a totem, not an apple.
        let panel = panel_lines(
            &Config::default(),
            &clock(24000 * 600),
            &player_in("minecraft:overworld"),
            0,
        );
        assert_eq!(texts(&panel)[0], "Day: 600 (Milestone: totem of undying)");
    }

    #[test]
    fn bed_prefix_in_the_sleep_window_when_enabled() {
        let config = Config {
            show_sleep_icon: true,
            ..Config::default()
        };
        let panel = panel_lines(&config, &clock(13000), &player_in("minecraft:overworld"), 0);
        assert_eq!(texts(&panel)[1], "[Bed] 19:00");
        let panel = panel_lines(&config, &clock(1000), &player_in("minecraft:overworld"), 0);
        assert_eq!(texts(&panel)[1], "07:00");
    }

    #[test]
    fn in_nether_follows_setting() {
        let mut config = Config::default();
        let player = player_in("minecraft:the_nether");
        // Default: show the Overworld time.
        let panel = panel_lines(&config, &clock(1000), &player, 0);
        assert_eq!(texts(&panel), ["Day: 0", "07:00", "=====---------------"]);
        config.other_dimensions = OtherDimensions::Hide;
        let panel = panel_lines(&config, &clock(1000), &player, 0);
        assert_eq!(texts(&panel), ["Day: 0"]);
        config.other_dimensions = OtherDimensions::Unknown;
        let panel = panel_lines(&config, &clock(1000), &player, 0);
        assert_eq!(texts(&panel), ["Day: 0", "??:??", "--------------------"]);
    }

    #[test]
    fn nothing_when_everything_is_off() {
        let config = Config {
            show_day_counter: false,
            show_clock: false,
            show_bossbar: false,
            ..Config::default()
        };
        assert!(panel_lines(&config, &clock(0), &player_in("minecraft:overworld"), 0).is_none());
    }

    #[test]
    fn simple_mode_shows_only_the_day() {
        let config = Config {
            simple_mode: true,
            ..Config::default()
        };
        let panel = panel_lines(
            &config,
            &clock(24000 * 7 + 1000),
            &player_in("minecraft:overworld"),
            0,
        );
        assert_eq!(texts(&panel), ["Day: 7"]);
    }

    #[test]
    fn countdown_and_moon_lines_appear_when_on() {
        let config = Config {
            show_night_countdown: true,
            show_moon_phase: true,
            ..Config::default()
        };
        let panel = panel_lines(
            &config,
            &clock(24000 * 5 + 6000),
            &player_in("minecraft:overworld"),
            0,
        );
        assert_eq!(
            texts(&panel),
            [
                "Day: 5",
                "12:00",
                "==========----------",
                "Night in 5:27",
                "Moon: Waxing Crescent"
            ]
        );
    }

    #[test]
    fn config_round_trips_in_the_original_format() {
        let config = Config {
            bar_theme: BarTheme::Mono,
            use_12_hour_format: true,
            corner: Corner::BottomRight,
            other_dimensions: OtherDimensions::Unknown,
            show_moon_phase: true,
            ..Config::default()
        };
        let text = config.serialize();
        assert!(text.contains("barTheme=MONO\n"));
        assert!(text.contains("corner=BOTTOM_RIGHT\n"));
        assert_eq!(Config::parse(&text), config);
        // The original's keys load too, including its position flag and
        // keys this port ignores (scale, opacity, sound).
        let old = "showDayCounter=true\nshowClock=false\nbarTheme=PASTEL\npositionBottom=true\nscale=120\nmilestoneSound=true\nbogusKey=1\n";
        let parsed = Config::parse(old);
        assert_eq!(parsed.bar_theme, BarTheme::Pastel);
        assert!(!parsed.show_clock);
        assert_eq!(parsed.corner, Corner::BottomLeft);
    }

    #[test]
    fn settings_page_drives_the_config() {
        let mut config = Config::default();
        assert!(apply_select(&mut config, W_THEME, 2));
        assert_eq!(config.bar_theme, BarTheme::Colorblind);
        assert!(apply_select(&mut config, W_CLOCK_FORMAT, 1));
        assert!(config.use_12_hour_format);
        assert!(apply_select(&mut config, W_POSITION, 3));
        assert_eq!(config.corner, Corner::BottomRight);
        assert!(apply_select(&mut config, W_DIMENSIONS, 1));
        assert_eq!(config.other_dimensions, OtherDimensions::Unknown);
        assert!(apply_select(&mut config, W_MOON, 0));
        assert!(config.show_moon_phase);
        assert!(apply_select(&mut config, W_MOON, 0));
        assert!(!config.show_moon_phase);
        assert!(!apply_select(&mut config, 99, 0));
        // The widgets reflect the config.
        let widgets = settings_widgets(&config);
        assert!(widgets.len() > 10);
    }
}

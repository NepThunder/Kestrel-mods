//! The Basic HUD for Kestrel: a corner panel with coordinates, FPS, the
//! world clock, speed, CPS, player health, effect timers, held-item and
//! armour durability and recent pickups. `/basichud` hides or shows the
//! panel, and Options > Mods > The Basic HUD has a switch per widget, the
//! corner, and how long pickups stay up; the game keeps the choices in
//! the mod's storage.
//!
//! Behaviour reimplemented from The Basic HUD by repositorykreml1n, MIT,
//! source <https://github.com/repositorykreml1n/the-basic-hud>; this file
//! shares no code with the original. The original is a visual HUD editor
//! with 14 movable widgets; this port keeps the 10 the SDK can feed —
//! coordinates and clock from `game-read`, FPS from
//! `render.frame-stats`, CPS from `input.clicks-per-second`, health from
//! `game-read`, effect timers from `game.get-effects` (API 18's self
//! reads: the real durations and amplifiers, where 0.1.0 had to leave
//! the widget out for want of an effect read), durability from the
//! `%inventory` snapshot and pickups from diffing that snapshot once per
//! tick — and lists the rest as missing in the README, with the reason
//! each one is out of reach. This port's home
//! is <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/basic-hud>.
use horizon_mod_sdk::{
    Mod, Registration,
    game::{self, Clock, Effect, Player},
    hud::{self, Corner, Line},
    input,
    inventory::{self, InvStack, PlayerInventory},
    recipes, render,
    render::FrameRate,
    storage,
    ui::{self, ButtonWidget, Widget},
};
use std::collections::{BTreeMap, VecDeque};

/// The newest API level this mod needs, asserted in the tests below:
/// `game.get-effects` is level 18, the newest function it imports
/// (`inventory.inventory` is level 9, `input.clicks-per-second` and
/// `render.frame-stats` are level 0, `recipes.items` level 5;
/// `crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 18;

/// Ticks per second: a pickup shown for N seconds expires N * 20 ticks
/// after it was last refreshed.
const TICKS_PER_SECOND: u64 = 20;

/// How many per-tick distances the speed average keeps: one second.
const SPEED_WINDOW: usize = 20;

/// How many pickup entries the panel shows; a further entry is folded
/// into a `+N more` line.
const MAX_PICKUP_LINES: usize = 4;

/// How many pickup entries are kept at all; older ones drop off the far end.
const MAX_PICKUPS: usize = 8;

/// The Java durability colours, as Armor Status uses them: green above
/// 50%, yellow from 25%, orange from 10%, red below 10%.
const COLOR_GREEN: u32 = 0x55_FF55;
const COLOR_YELLOW: u32 = 0xFF_FF55;
const COLOR_ORANGE: u32 = 0xFF_AA00;
const COLOR_RED: u32 = 0xFF_5555;
const COLOR_TEXT: u32 = 0xFF_FFFF;

/// The corners the panel can sit in, in settings order.
const CORNERS: [&str; 4] = ["Top left", "Top right", "Bottom left", "Bottom right"];

/// The pickup display times the settings page cycles through, in seconds:
/// the original lets the player pick any 1-60 seconds; the settings page
/// is buttons, so the choice is these presets, 5 seconds by default.
const PICKUP_SECONDS: [u64; 8] = [1, 2, 3, 5, 10, 15, 30, 60];

/// Armour slots in the `%inventory` numbering: head, chest, legs, feet.
const ARMOUR_SLOTS: [u32; 4] = [500, 501, 502, 503];

/// Storage keys for the settings.
const SHOWN_KEY: &str = "shown";
const CORNER_KEY: &str = "corner";
const COORDS_KEY: &str = "widget_coords";
const FPS_KEY: &str = "widget_fps";
const CLOCK_KEY: &str = "widget_clock";
const SPEED_KEY: &str = "widget_speed";
const CPS_KEY: &str = "widget_cps";
const HEALTH_KEY: &str = "widget_health";
const EFFECTS_KEY: &str = "widget_effects";
const DURABILITY_KEY: &str = "widget_durability";
const ARMOUR_KEY: &str = "widget_armour";
const PICKUPS_KEY: &str = "widget_pickups";
const PICKUP_SECS_KEY: &str = "pickup_secs";

/// The settings page's buttons, in the order they are shown.
const SHOWN_BUTTON: u32 = 0;
const CORNER_BUTTON: u32 = 1;
const COORDS_BUTTON: u32 = 2;
const FPS_BUTTON: u32 = 3;
const CLOCK_BUTTON: u32 = 4;
const SPEED_BUTTON: u32 = 5;
const CPS_BUTTON: u32 = 6;
const HEALTH_BUTTON: u32 = 7;
const DURABILITY_BUTTON: u32 = 8;
const ARMOUR_BUTTON: u32 = 9;
const PICKUPS_BUTTON: u32 = 10;
const PICKUP_SECS_BUTTON: u32 = 11;
/// The effect widget's button, after the shipped page's twelve:
/// the buttons the shipped mod had keep their ids.
const EFFECTS_BUTTON: u32 = 12;

/// Which widgets the panel shows, and where it sits.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Settings {
    shown: bool,
    /// Index in `CORNERS`. Default: 0 (Top left).
    corner: usize,
    coords: bool,
    fps: bool,
    clock: bool,
    speed: bool,
    cps: bool,
    health: bool,
    effects: bool,
    durability: bool,
    armour: bool,
    pickups: bool,
    /// Index in `PICKUP_SECONDS`. Default: 3 (5 seconds).
    pickup_secs: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shown: true,
            corner: 0,
            coords: true,
            fps: true,
            clock: true,
            speed: true,
            cps: true,
            health: true,
            effects: true,
            durability: true,
            armour: true,
            pickups: true,
            pickup_secs: 3,
        }
    }
}

impl Settings {
    /// Reads the stored settings: an absent or unparseable value keeps its
    /// default, and an out-of-range index wraps, as Armor Status does.
    fn read(get: impl Fn(&str) -> Option<Vec<u8>>) -> Self {
        let flag = |key: &str, def: bool| read_bool(get(key), def);
        let index = |key: &str, len: usize, def: usize| {
            get(key)
                .and_then(|v| String::from_utf8(v).ok())
                .and_then(|s| s.trim().parse::<usize>().ok())
                .map_or(def, |i| i % len)
        };
        Self {
            shown: flag(SHOWN_KEY, true),
            corner: index(CORNER_KEY, CORNERS.len(), 0),
            coords: flag(COORDS_KEY, true),
            fps: flag(FPS_KEY, true),
            clock: flag(CLOCK_KEY, true),
            speed: flag(SPEED_KEY, true),
            cps: flag(CPS_KEY, true),
            health: flag(HEALTH_KEY, true),
            effects: flag(EFFECTS_KEY, true),
            durability: flag(DURABILITY_KEY, true),
            armour: flag(ARMOUR_KEY, true),
            pickups: flag(PICKUPS_KEY, true),
            pickup_secs: index(PICKUP_SECS_KEY, PICKUP_SECONDS.len(), 3),
        }
    }

    fn corner(&self) -> Corner {
        match self.corner {
            1 => Corner::TopRight,
            2 => Corner::BottomLeft,
            3 => Corner::BottomRight,
            _ => Corner::TopLeft,
        }
    }

    fn pickup_seconds(&self) -> u64 {
        PICKUP_SECONDS[self.pickup_secs]
    }

    fn labels(&self) -> Vec<String> {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("Shown: {}", on(self.shown)),
            format!("Corner: {}", CORNERS[self.corner]),
            format!("Coordinates: {}", on(self.coords)),
            format!("FPS: {}", on(self.fps)),
            format!("Clock: {}", on(self.clock)),
            format!("Speed: {}", on(self.speed)),
            format!("CPS: {}", on(self.cps)),
            format!("Health: {}", on(self.health)),
            format!("Durability: {}", on(self.durability)),
            format!("Armour: {}", on(self.armour)),
            format!("Pickups: {}", on(self.pickups)),
            format!("Pickup time: {}s", self.pickup_seconds()),
            format!("Effects: {}", on(self.effects)),
        ]
    }
}

/// One recent pickup: the item, how many arrived in total, and the tick
/// the entry stops showing at.
#[derive(Clone, Debug, PartialEq)]
struct Pickup {
    id: String,
    count: u32,
    expires: u64,
}

struct BasicHud {
    settings: Settings,
    /// The item table's id-to-name pairs, loaded once on the first tick
    /// that needs a name (`recipes.items` in one call, as JEI loads it)
    /// and kept.
    names: Option<Vec<(String, String)>>,
    /// The panel last sent, and its corner, so an unchanged one costs no call.
    /// Texts only: a durability line's colour is a pure function of the
    /// numbers in its text, so equal texts mean an equal panel.
    last: Vec<String>,
    last_corner: Option<Corner>,
    /// The feet position on the last tick, for the speed average.
    prev_pos: Option<(f64, f64)>,
    /// The last second's per-tick horizontal distances, oldest first.
    speeds: VecDeque<f64>,
    /// The last tick's per-item totals over the hotbar and main
    /// inventory, for the pickup diff; None until the first tick with a
    /// player, so joining a world never logs the whole inventory.
    prev_totals: Option<BTreeMap<String, u32>>,
    /// Recent pickups, newest first.
    pickups: Vec<Pickup>,
}

/// `Day 3, 14:05`: day time 0 is 6:00 on day 1.
fn time_of_day(clock: &Clock) -> String {
    let day = clock.day_time / 24000 + 1;
    let ticks = clock.day_time % 24000;
    let hours = (ticks / 1000 + 6) % 24;
    let minutes = ticks % 1000 * 60 / 1000;
    format!("Day {day}, {hours:02}:{minutes:02}")
}

/// The item id with the words capitalised: `minecraft:grass_block` is
/// "Grass Block", `minecraft:torch` is "Torch". The fallback when the
/// item table has no name for the id.
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

/// The FPS widget's line: frames in the last second, the slowest frame
/// and the 1% low of the last 5 seconds, as the FPS counter shows them.
fn fps_line(frame: &FrameRate) -> String {
    format!("{} fps  min {}  1% {}", frame.fps, frame.min, frame.low)
}

/// The CPS widget's line: attack clicks, then use clicks, in the last second.
fn cps_line(cps: (u32, u32)) -> String {
    format!("CPS {} | {}", cps.0, cps.1)
}

/// The speed widget's line: blocks per second over the last second.
fn speed_line(speed: f64) -> String {
    format!("Speed {speed:.1} b/s")
}

/// A level in Roman numerals: 1 is I, 2 is II, 4 is IV, 9 is IX.
/// The effect widget prints levels past I; 0 (and anything past
/// 3999, no real amplifier) prints nothing.
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

/// The effect widget's line for one active effect — `Speed II 1:01`:
/// the name (the id's words, as the panel names items it has no
/// table entry for), the Roman numeral level past I, and the time
/// left as m:ss from the real remaining ticks `game.get-effects`
/// reports, or `inf` for a permanent effect (duration -1).
fn effect_line(effect: &Effect) -> String {
    let name = pretty_id(&effect.id);
    let level = if effect.amplifier == 0 {
        String::new()
    } else {
        format!(" {}", roman(effect.amplifier + 1))
    };
    let time = if effect.duration < 0 {
        "inf".to_string()
    } else {
        let seconds = effect.duration / 20;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    };
    format!("{name}{level} {time}")
}

/// The effect widget's lines, in the game's own order.
fn effect_lines(effects: &[Effect]) -> Vec<String> {
    effects.iter().map(effect_line).collect()
}

/// The durability colour for the fraction remaining, as Armor Status
/// colours it.
fn durability_color(remaining: u32, max: u32) -> u32 {
    if max == 0 {
        return COLOR_TEXT;
    }
    let pct = remaining * 100 / max;
    if pct > 50 {
        COLOR_GREEN
    } else if pct >= 25 {
        COLOR_YELLOW
    } else if pct >= 10 {
        COLOR_ORANGE
    } else {
        COLOR_RED
    }
}

/// A damageable stack's line — `Diamond Sword 1461/1561` — or nothing
/// when the stack cannot wear out.
fn durability_line(name: &str, stack: &InvStack) -> Option<Line> {
    if stack.max_damage == 0 {
        return None;
    }
    let remaining = stack.max_damage.saturating_sub(stack.damage);
    Some(Line {
        text: format!("{name} {remaining}/{}", stack.max_damage),
        color: durability_color(remaining, stack.max_damage),
    })
}

/// The per-item totals over the hotbar and main inventory (slots 0-35):
/// what the pickup diff compares. Armour, the offhand, the crafting
/// grid and the carried stack are not pickups' business.
fn totals_of(inv: &PlayerInventory) -> BTreeMap<String, u32> {
    let mut totals = BTreeMap::new();
    for stack in &inv.slots {
        if stack.slot <= 35 {
            *totals.entry(stack.id.clone()).or_insert(0) += stack.count;
        }
    }
    totals
}

/// The pickup widget's lines, newest first: `+6 Torch`, then a `+N more`
/// line when more entries are kept than the panel shows.
fn pickup_lines(pickups: &[Pickup], names: &[(String, String)]) -> Vec<String> {
    let mut lines = Vec::new();
    for pickup in pickups.iter().take(MAX_PICKUP_LINES) {
        lines.push(format!("+{} {}", pickup.count, name_for(names, &pickup.id)));
    }
    if pickups.len() > MAX_PICKUP_LINES {
        lines.push(format!("+{} more", pickups.len() - MAX_PICKUP_LINES));
    }
    lines
}

fn name_for(names: &[(String, String)], id: &str) -> String {
    names
        .iter()
        .find(|(item_id, _)| item_id == id)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| pretty_id(id))
}

/// The panel's line cap: the game's HUD panel holds at most 16
/// lines, so the built panel is cut there — the widgets at the
/// end of the order are the ones that give way.
const PANEL_MAX_LINES: usize = 16;

/// The panel's lines for one tick's readings, in widget order. Durability
/// and pickup lines are built by the caller, which knows the item names.
#[allow(clippy::too_many_arguments)]
fn build_lines(
    player: &Player,
    clock: Option<&Clock>,
    frame: &FrameRate,
    cps: (u32, u32),
    speed: f64,
    effects: &[String],
    durability: &[Line],
    armour: &[Line],
    pickups: &[String],
    settings: Settings,
) -> Vec<Line> {
    let mut lines = Vec::new();
    let push = |lines: &mut Vec<Line>, text: String| {
        lines.push(Line {
            text,
            color: COLOR_TEXT,
        });
    };
    if settings.fps {
        push(&mut lines, fps_line(frame));
    }
    if settings.coords {
        push(
            &mut lines,
            format!("XYZ {:.1} / {:.1} / {:.1}", player.x, player.y, player.z),
        );
    }
    if settings.clock
        && let Some(clock) = clock
    {
        push(&mut lines, time_of_day(clock));
    }
    if settings.speed {
        push(&mut lines, speed_line(speed));
    }
    if settings.cps {
        push(&mut lines, cps_line(cps));
    }
    if settings.health {
        push(&mut lines, format!("Health {:.1}/20.0", player.health));
    }
    if settings.effects {
        for text in effects {
            push(&mut lines, text.clone());
        }
    }
    if settings.durability {
        lines.extend(durability.iter().cloned());
    }
    if settings.armour {
        lines.extend(armour.iter().cloned());
    }
    if settings.pickups {
        for text in pickups {
            push(&mut lines, text.clone());
        }
    }
    lines.truncate(PANEL_MAX_LINES);
    lines
}

fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

impl BasicHud {
    /// Loads the item table once, on the first tick that needs a name.
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

    fn name_for(&self, id: &str) -> String {
        match &self.names {
            Some(names) => name_for(names, id),
            None => pretty_id(id),
        }
    }

    /// The speed over the last second: the mean per-tick horizontal
    /// distance, times 20 ticks a second; 0 before any movement is seen.
    fn speed(&self) -> f64 {
        if self.speeds.is_empty() {
            0.0
        } else {
            self.speeds.iter().sum::<f64>() / self.speeds.len() as f64 * 20.0
        }
    }

    /// Records this tick's horizontal movement. The first tick with a
    /// player only sets the baseline; a teleport reads as one fast tick,
    /// the way a rolling average does.
    fn sample_speed(&mut self, player: &Player) {
        if let Some((px, pz)) = self.prev_pos {
            let dx = player.x - px;
            let dz = player.z - pz;
            self.speeds.push_back((dx * dx + dz * dz).sqrt());
            while self.speeds.len() > SPEED_WINDOW {
                self.speeds.pop_front();
            }
        }
        self.prev_pos = Some((player.x, player.z));
    }

    /// Folds this tick's inventory totals into the pickup list: a rise in
    /// an item's total is a pickup, aggregated with an entry for the same
    /// item and refreshing its timer, as the original aggregates them.
    /// A rise in the same tick as any fall is not a pickup — that is
    /// crafting or consuming, which the original does not log — and
    /// rearrangement (the same totals) logs nothing. A `/give` looks
    /// like a pickup to a snapshot diff; the original could tell because
    /// it saw the pickup event, and this port cannot.
    fn track_pickups(&mut self, totals: &BTreeMap<String, u32>, tick: u64) {
        // Cloned: the diff below mutates the pickup list.
        let prev = self.prev_totals.clone();
        match &prev {
            None => {}
            Some(prev) => {
                let any_fall = prev
                    .iter()
                    .any(|(id, &was)| totals.get(id).copied().unwrap_or(0) < was);
                if !any_fall {
                    for (id, &now) in totals {
                        let was = prev.get(id).copied().unwrap_or(0);
                        if now > was {
                            self.add_pickup(id.clone(), now - was, tick);
                        }
                    }
                }
            }
        }
        self.prev_totals = Some(totals.clone());
        self.pickups.retain(|p| tick < p.expires);
    }

    fn add_pickup(&mut self, id: String, count: u32, tick: u64) {
        let expires = tick + self.settings.pickup_seconds() * TICKS_PER_SECOND;
        if let Some(i) = self.pickups.iter().position(|p| p.id == id) {
            let mut entry = self.pickups.remove(i);
            entry.count += count;
            entry.expires = expires;
            self.pickups.insert(0, entry);
        } else {
            self.pickups.insert(0, Pickup { id, count, expires });
        }
        while self.pickups.len() > MAX_PICKUPS {
            self.pickups.pop();
        }
    }

    fn set_shown(&mut self, shown: bool) {
        self.settings.shown = shown;
        let _ = storage::set(SHOWN_KEY, if shown { b"true" } else { b"false" });
        self.refresh_settings();
    }

    fn set_corner(&mut self) {
        self.settings.corner = (self.settings.corner + 1) % CORNERS.len();
        let _ = storage::set(CORNER_KEY, self.settings.corner.to_string().as_bytes());
        self.refresh_settings();
    }

    fn set_widget(&mut self, key: &str, on: bool) {
        match key {
            COORDS_KEY => self.settings.coords = on,
            FPS_KEY => self.settings.fps = on,
            CLOCK_KEY => self.settings.clock = on,
            SPEED_KEY => self.settings.speed = on,
            CPS_KEY => self.settings.cps = on,
            HEALTH_KEY => self.settings.health = on,
            EFFECTS_KEY => self.settings.effects = on,
            DURABILITY_KEY => self.settings.durability = on,
            ARMOUR_KEY => self.settings.armour = on,
            PICKUPS_KEY => self.settings.pickups = on,
            _ => {}
        }
        let _ = storage::set(key, if on { b"true" } else { b"false" });
        self.refresh_settings();
    }

    fn cycle_pickup_secs(&mut self) {
        self.settings.pickup_secs = (self.settings.pickup_secs + 1) % PICKUP_SECONDS.len();
        let _ = storage::set(
            PICKUP_SECS_KEY,
            self.settings.pickup_secs.to_string().as_bytes(),
        );
        self.refresh_settings();
    }

    /// The settings page: show, corner, one button per widget, pickup time.
    fn refresh_settings(&self) {
        let labels = self.settings.labels();
        let widgets: Vec<Widget> = labels
            .into_iter()
            .enumerate()
            .map(|(i, label)| {
                Widget::Button(ButtonWidget {
                    id: i as u32,
                    label,
                })
            })
            .collect();
        ui::set_settings(&widgets);
    }

    fn clear_panel(&mut self) {
        if !self.last.is_empty() {
            hud::clear();
            self.last.clear();
            self.last_corner = None;
        }
    }
}

impl Mod for BasicHud {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("basichud").on_tick();
        let state = BasicHud {
            settings: Settings::read(storage::get),
            names: None,
            last: Vec::new(),
            last_corner: None,
            prev_pos: None,
            speeds: VecDeque::new(),
            prev_totals: None,
            pickups: Vec::new(),
        };
        Ok((state, hooks))
    }

    fn tick(&mut self, tick: u64) {
        let Some(p) = game::get_player() else {
            // Off the world: forget the baselines, so rejoining never
            // logs the whole inventory as pickups, and drop the panel.
            self.prev_pos = None;
            self.prev_totals = None;
            self.pickups.clear();
            self.speeds.clear();
            self.clear_panel();
            return;
        };
        self.sample_speed(&p);
        let inv = inventory::inventory();
        let totals = totals_of(&inv);
        self.track_pickups(&totals, tick);
        if !self.settings.shown {
            self.clear_panel();
            return;
        }
        let needs_names = (self.settings.durability
            && inv
                .slots
                .iter()
                .any(|s| s.slot == inv.selected && s.max_damage > 0))
            || (self.settings.armour
                && inv
                    .slots
                    .iter()
                    .any(|s| ARMOUR_SLOTS.contains(&s.slot) && s.max_damage > 0))
            || (self.settings.pickups && !self.pickups.is_empty());
        if needs_names {
            self.load_names();
        }
        let durability: Vec<Line> = match inv.slots.iter().find(|s| s.slot == inv.selected) {
            Some(stack) => match durability_line(&self.name_for(&stack.id), stack) {
                Some(line) => vec![line],
                None => Vec::new(),
            },
            None => Vec::new(),
        };
        let armour: Vec<Line> = ARMOUR_SLOTS
            .iter()
            .filter_map(|slot| {
                inv.slots
                    .iter()
                    .find(|s| s.slot == *slot)
                    .and_then(|stack| durability_line(&self.name_for(&stack.id), stack))
            })
            .collect();
        let empty_names: Vec<(String, String)> = Vec::new();
        let names = self.names.as_ref().unwrap_or(&empty_names);
        let pickups = pickup_lines(&self.pickups, names);
        let effects = if self.settings.effects {
            effect_lines(&game::get_effects())
        } else {
            Vec::new()
        };
        let now = build_lines(
            &p,
            game::get_clock().as_ref(),
            &render::frame_stats(),
            input::clicks_per_second(),
            self.speed(),
            &effects,
            &durability,
            &armour,
            &pickups,
            self.settings,
        );
        let corner = self.settings.corner();
        let texts: Vec<String> = now.iter().map(|l| l.text.clone()).collect();
        if texts != self.last || Some(corner) != self.last_corner {
            hud::set_panel(corner, &now);
            self.last = texts;
            self.last_corner = Some(corner);
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "basichud" {
            return Err("This command does nothing".into());
        }
        self.set_shown(!self.settings.shown);
        let state = if self.settings.shown {
            "shown"
        } else {
            "hidden"
        };
        Ok(Some(format!("Basic HUD panel {state}")))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SHOWN_BUTTON => self.set_shown(!self.settings.shown),
            CORNER_BUTTON => self.set_corner(),
            COORDS_BUTTON => self.set_widget(COORDS_KEY, !self.settings.coords),
            FPS_BUTTON => self.set_widget(FPS_KEY, !self.settings.fps),
            CLOCK_BUTTON => self.set_widget(CLOCK_KEY, !self.settings.clock),
            SPEED_BUTTON => self.set_widget(SPEED_KEY, !self.settings.speed),
            CPS_BUTTON => self.set_widget(CPS_KEY, !self.settings.cps),
            HEALTH_BUTTON => self.set_widget(HEALTH_KEY, !self.settings.health),
            EFFECTS_BUTTON => self.set_widget(EFFECTS_KEY, !self.settings.effects),
            DURABILITY_BUTTON => self.set_widget(DURABILITY_KEY, !self.settings.durability),
            ARMOUR_BUTTON => self.set_widget(ARMOUR_KEY, !self.settings.armour),
            PICKUPS_BUTTON => self.set_widget(PICKUPS_KEY, !self.settings.pickups),
            PICKUP_SECS_BUTTON => self.cycle_pickup_secs(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(BasicHud);

#[cfg(test)]
mod tests {
    use super::*;

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

    fn stack(slot: u32, id: &str, count: u32, damage: u32, max_damage: u32) -> InvStack {
        InvStack {
            slot,
            id: id.into(),
            count,
            damage,
            max_damage,
            components: String::new(),
        }
    }

    fn inventory(slots: Vec<InvStack>) -> PlayerInventory {
        PlayerInventory {
            selected: 0,
            saturation: 5.0,
            slots,
            carried: None,
        }
    }

    fn names() -> Vec<(String, String)> {
        vec![
            ("minecraft:torch".into(), "Torch".into()),
            ("minecraft:diamond_sword".into(), "Diamond Sword".into()),
            ("minecraft:diamond_helmet".into(), "Diamond Helmet".into()),
            ("minecraft:oak_planks".into(), "Oak Planks".into()),
            ("minecraft:stick".into(), "Stick".into()),
        ]
    }

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.text.clone()).collect()
    }

    #[test]
    fn api_level_is_eighteen() {
        // Newest imported function is `game.get-effects` (level 18);
        // `inventory.inventory` is level 9, `recipes.items` is
        // level 5, `input.clicks-per-second` and
        // `render.frame-stats` level 0, the rest level 0.
        assert_eq!(API_LEVEL, 18);
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
    fn cps_line_shows_attack_then_use() {
        assert_eq!(cps_line((7, 2)), "CPS 7 | 2");
        assert_eq!(cps_line((0, 0)), "CPS 0 | 0");
    }

    #[test]
    fn speed_line_is_blocks_per_second() {
        assert_eq!(speed_line(4.317), "Speed 4.3 b/s");
        assert_eq!(speed_line(0.0), "Speed 0.0 b/s");
    }

    #[test]
    fn id_prettifies_to_the_english_name() {
        assert_eq!(pretty_id("minecraft:grass_block"), "Grass Block");
        assert_eq!(pretty_id("minecraft:torch"), "Torch");
        assert_eq!(pretty_id("stone"), "Stone");
        assert_eq!(name_for(&names(), "minecraft:torch"), "Torch");
        assert_eq!(
            name_for(&names(), "minecraft:unknown_thing"),
            "Unknown Thing"
        );
    }

    #[test]
    fn durability_colours_follow_the_java_bands() {
        assert_eq!(durability_color(1561, 1561), COLOR_GREEN);
        assert_eq!(durability_color(800, 1561), COLOR_GREEN);
        assert_eq!(durability_color(780, 1561), COLOR_YELLOW);
        assert_eq!(durability_color(391, 1561), COLOR_YELLOW);
        assert_eq!(durability_color(200, 1561), COLOR_ORANGE);
        assert_eq!(durability_color(156, 1561), COLOR_RED);
        assert_eq!(durability_color(0, 1561), COLOR_RED);
    }

    #[test]
    fn durability_line_is_remaining_over_max_or_nothing() {
        let line = durability_line(
            "Diamond Sword",
            &stack(0, "minecraft:diamond_sword", 1, 100, 1561),
        )
        .unwrap();
        assert_eq!(line.text, "Diamond Sword 1461/1561");
        assert_eq!(line.color, COLOR_GREEN);
        let worn = durability_line(
            "Diamond Sword",
            &stack(0, "minecraft:diamond_sword", 1, 1500, 1561),
        )
        .unwrap();
        assert_eq!(worn.text, "Diamond Sword 61/1561");
        assert_eq!(worn.color, COLOR_RED);
        assert!(durability_line("Torch", &stack(0, "minecraft:torch", 64, 0, 0)).is_none());
    }

    #[test]
    fn totals_count_the_hotbar_and_main_inventory_only() {
        let inv = inventory(vec![
            stack(0, "minecraft:torch", 4, 0, 0),
            stack(9, "minecraft:torch", 2, 0, 0),
            stack(500, "minecraft:diamond_helmet", 1, 10, 363),
            stack(504, "minecraft:torch", 32, 0, 0),
        ]);
        // The offhand's 32 torches and the helmet do not count.
        assert_eq!(
            totals_of(&inv),
            BTreeMap::from([("minecraft:torch".to_string(), 6)])
        );
    }

    #[test]
    fn pickups_aggregate_and_refresh_but_rearranging_logs_nothing() {
        let mut hud = BasicHud {
            settings: Settings::default(),
            names: None,
            last: Vec::new(),
            last_corner: None,
            prev_pos: None,
            speeds: VecDeque::new(),
            prev_totals: None,
            pickups: Vec::new(),
        };
        // The first sighting is the baseline: no entries.
        hud.track_pickups(&BTreeMap::from([("minecraft:torch".to_string(), 2)]), 10);
        assert!(hud.pickups.is_empty());
        // Four more torches: one entry, expiring 5s after tick 11.
        hud.track_pickups(&BTreeMap::from([("minecraft:torch".to_string(), 6)]), 11);
        assert_eq!(
            hud.pickups,
            [Pickup {
                id: "minecraft:torch".into(),
                count: 4,
                expires: 11 + 5 * 20,
            }]
        );
        // Two more before expiry: x6, timer refreshed to tick 12 + 100.
        hud.track_pickups(&BTreeMap::from([("minecraft:torch".to_string(), 8)]), 12);
        assert_eq!(hud.pickups[0].count, 6);
        assert_eq!(hud.pickups[0].expires, 12 + 5 * 20);
        // Rearranging (same totals) logs nothing and keeps the timer.
        hud.track_pickups(&BTreeMap::from([("minecraft:torch".to_string(), 8)]), 13);
        assert_eq!(hud.pickups.len(), 1);
        assert_eq!(hud.pickups[0].count, 6);
        // A fall alone logs nothing.
        hud.track_pickups(&BTreeMap::from([("minecraft:torch".to_string(), 5)]), 14);
        assert_eq!(hud.pickups[0].count, 6);
        // Still showing on the last tick before expiry, gone on it.
        hud.track_pickups(
            &BTreeMap::from([("minecraft:torch".to_string(), 5)]),
            12 + 5 * 20 - 1,
        );
        assert_eq!(hud.pickups.len(), 1);
        hud.track_pickups(
            &BTreeMap::from([("minecraft:torch".to_string(), 5)]),
            12 + 5 * 20,
        );
        assert!(hud.pickups.is_empty());
    }

    #[test]
    fn crafting_suppresses_the_pickup_its_result_would_log() {
        let mut hud = BasicHud {
            settings: Settings::default(),
            names: None,
            last: Vec::new(),
            last_corner: None,
            prev_pos: None,
            speeds: VecDeque::new(),
            prev_totals: None,
            pickups: Vec::new(),
        };
        hud.track_pickups(
            &BTreeMap::from([("minecraft:oak_planks".to_string(), 6)]),
            10,
        );
        // Two planks became four sticks: the planks fell, so the sticks
        // are not logged as a pickup.
        hud.track_pickups(
            &BTreeMap::from([
                ("minecraft:oak_planks".to_string(), 4),
                ("minecraft:stick".to_string(), 4),
            ]),
            11,
        );
        assert!(hud.pickups.is_empty());
    }

    #[test]
    fn pickup_lines_show_the_newest_four_then_a_more_line() {
        let entries: Vec<Pickup> = (0..6)
            .map(|i| Pickup {
                id: format!("minecraft:item_{i}"),
                count: i + 1,
                expires: 1000,
            })
            .collect();
        assert_eq!(
            pickup_lines(&entries, &names()),
            [
                "+1 Item 0",
                "+2 Item 1",
                "+3 Item 2",
                "+4 Item 3",
                "+2 more",
            ]
        );
        assert_eq!(
            pickup_lines(&entries[..2], &names()),
            ["+1 Item 0", "+2 Item 1"]
        );
        assert!(pickup_lines(&[], &names()).is_empty());
    }

    #[test]
    fn speed_averages_the_last_second_of_movement() {
        let mut hud = BasicHud {
            settings: Settings::default(),
            names: None,
            last: Vec::new(),
            last_corner: None,
            prev_pos: None,
            speeds: VecDeque::new(),
            prev_totals: None,
            pickups: Vec::new(),
        };
        assert_eq!(hud.speed(), 0.0);
        let mut p = player();
        hud.sample_speed(&p);
        assert_eq!(hud.speed(), 0.0);
        // One block in one tick: 20 blocks a second.
        p.x = 1.5;
        hud.sample_speed(&p);
        assert!((hud.speed() - 20.0).abs() < 0.001);
        // Nineteen still ticks: the block spread over a full second.
        for _ in 0..19 {
            hud.sample_speed(&p);
        }
        assert!((hud.speed() - 1.0).abs() < 0.001);
        // Twenty more still ticks: the movement has left the window.
        for _ in 0..20 {
            hud.sample_speed(&p);
        }
        assert_eq!(hud.speed(), 0.0);
    }

    #[test]
    fn all_widgets_on_build_the_full_panel_within_the_line_cap() {
        let held = durability_line(
            "Diamond Sword",
            &stack(0, "minecraft:diamond_sword", 1, 100, 1561),
        )
        .into_iter()
        .collect::<Vec<_>>();
        let armour = [
            durability_line(
                "Diamond Helmet",
                &stack(500, "minecraft:diamond_helmet", 1, 10, 363),
            ),
            durability_line(
                "Diamond Chestplate",
                &stack(501, "minecraft:diamond_chestplate", 1, 20, 528),
            ),
            durability_line(
                "Diamond Leggings",
                &stack(502, "minecraft:diamond_leggings", 1, 30, 495),
            ),
            durability_line(
                "Diamond Boots",
                &stack(503, "minecraft:diamond_boots", 1, 0, 429),
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        let pickups = pickup_lines(
            &[
                Pickup {
                    id: "minecraft:torch".into(),
                    count: 6,
                    expires: 1000,
                },
                Pickup {
                    id: "minecraft:oak_planks".into(),
                    count: 6,
                    expires: 1000,
                },
                Pickup {
                    id: "minecraft:stick".into(),
                    count: 4,
                    expires: 1000,
                },
                Pickup {
                    id: "minecraft:item_3".into(),
                    count: 1,
                    expires: 1000,
                },
                Pickup {
                    id: "minecraft:item_4".into(),
                    count: 1,
                    expires: 1000,
                },
            ],
            &names(),
        );
        let lines = build_lines(
            &player(),
            Some(&clock(0)),
            &frame(),
            (7, 2),
            4.3,
            &[],
            &held,
            &armour,
            &pickups,
            Settings::default(),
        );
        assert_eq!(
            texts(&lines),
            [
                "120 fps  min 41  1% 52",
                "XYZ 0.5 / 64.0 / 0.5",
                "Day 1, 06:00",
                "Speed 4.3 b/s",
                "CPS 7 | 2",
                "Health 20.0/20.0",
                "Diamond Sword 1461/1561",
                "Diamond Helmet 353/363",
                "Diamond Chestplate 508/528",
                "Diamond Leggings 465/495",
                "Diamond Boots 429/429",
                "+6 Torch",
                "+6 Oak Planks",
                "+4 Stick",
                "+1 Item 3",
                "+1 more",
            ]
        );
        // Sixteen lines: exactly the HUD panel's cap, never over it.
        assert_eq!(lines.len(), 16);
    }

    #[test]
    fn widgets_off_leave_their_lines_out() {
        let off = Settings {
            coords: false,
            fps: false,
            clock: false,
            speed: false,
            cps: false,
            health: false,
            effects: false,
            durability: false,
            armour: false,
            pickups: false,
            ..Settings::default()
        };
        let held = durability_line(
            "Diamond Sword",
            &stack(0, "minecraft:diamond_sword", 1, 100, 1561),
        )
        .into_iter()
        .collect::<Vec<_>>();
        assert!(
            build_lines(
                &player(),
                Some(&clock(0)),
                &frame(),
                (7, 2),
                4.3,
                &["Speed II 1:01".to_string()],
                &held,
                &[],
                &["+6 Torch".to_string()],
                off,
            )
            .is_empty()
        );
        // No clock (title screen edge): the clock widget shows nothing,
        // the rest still shows.
        let clock_only = Settings { clock: true, ..off };
        assert!(
            build_lines(
                &player(),
                None,
                &frame(),
                (0, 0),
                0.0,
                &[],
                &[],
                &[],
                &[],
                clock_only,
            )
            .is_empty()
        );
    }

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
    fn effect_lines_carry_the_real_level_and_time_left() {
        assert_eq!(effect_line(&effect("minecraft:speed", 0, 400)), "Speed 0:20");
        assert_eq!(
            effect_line(&effect("minecraft:speed", 1, 1234)),
            "Speed II 1:01"
        );
        assert_eq!(
            effect_line(&effect("minecraft:fire_resistance", 0, 20 * 90 * 60)),
            "Fire Resistance 90:00"
        );
        // A permanent effect reads inf, at any level.
        assert_eq!(
            effect_line(&effect("minecraft:regeneration", 0, -1)),
            "Regeneration inf"
        );
        assert_eq!(
            effect_line(&effect("minecraft:strength", 3, -1)),
            "Strength IV inf"
        );
        // The widget keeps the game's order.
        assert_eq!(
            effect_lines(&[
                effect("minecraft:poison", 0, 100),
                effect("minecraft:speed", 1, 1234),
            ]),
            ["Poison 0:05", "Speed II 1:01"]
        );
    }

    #[test]
    fn effect_lines_sit_after_health_and_the_panel_caps_at_sixteen() {
        let effects = effect_lines(&[
            effect("minecraft:speed", 1, 1234),
            effect("minecraft:regeneration", 0, -1),
        ]);
        let lines = build_lines(
            &player(),
            Some(&clock(0)),
            &frame(),
            (7, 2),
            4.3,
            &effects,
            &[],
            &[],
            &[],
            Settings::default(),
        );
        assert_eq!(
            texts(&lines),
            [
                "120 fps  min 41  1% 52",
                "XYZ 0.5 / 64.0 / 0.5",
                "Day 1, 06:00",
                "Speed 4.3 b/s",
                "CPS 7 | 2",
                "Health 20.0/20.0",
                "Speed II 1:01",
                "Regeneration inf",
            ]
        );
        // With every widget fed at once the panel stops at the
        // game's 16-line cap instead of running past it.
        let many: Vec<String> = (0..10)
            .map(|i| format!("Effect {i} 1:00"))
            .collect();
        let lines = build_lines(
            &player(),
            Some(&clock(0)),
            &frame(),
            (7, 2),
            4.3,
            &many,
            &[],
            &[],
            &["+6 Torch".to_string()],
            Settings::default(),
        );
        assert_eq!(lines.len(), PANEL_MAX_LINES);
        assert_eq!(texts(&lines)[6], "Effect 0 1:00");
    }

    #[test]
    fn settings_read_back_with_defaults() {
        let stored = Settings::read(|_| None);
        assert_eq!(stored, Settings::default());
        assert_eq!(stored.corner(), Corner::TopLeft);
        assert_eq!(stored.pickup_seconds(), 5);
        let get = |key: &str| match key {
            "shown" => Some(b"false".to_vec()),
            "corner" => Some(b"2".to_vec()),
            "pickup_secs" => Some(b"6".to_vec()),
            "widget_fps" => Some(b"false".to_vec()),
            _ => None,
        };
        let read = Settings::read(get);
        assert!(!read.shown);
        assert_eq!(read.corner(), Corner::BottomLeft);
        assert_eq!(read.pickup_seconds(), 30);
        assert!(!read.fps);
        assert!(read.coords);
        // An out-of-range index wraps instead of breaking the settings.
        let wrapped = Settings::read(|key| match key {
            "corner" => Some(b"9".to_vec()),
            _ => None,
        });
        assert_eq!(wrapped.corner(), Corner::TopRight);
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
    }

    #[test]
    fn settings_labels_name_every_switch() {
        let labels = Settings::default().labels();
        assert_eq!(
            labels,
            [
                "Shown: ON",
                "Corner: Top left",
                "Coordinates: ON",
                "FPS: ON",
                "Clock: ON",
                "Speed: ON",
                "CPS: ON",
                "Health: ON",
                "Durability: ON",
                "Armour: ON",
                "Pickups: ON",
                "Pickup time: 5s",
                "Effects: ON",
            ]
        );
    }
}

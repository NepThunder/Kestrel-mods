//! BetterF3 for Kestrel: a colour-coded, two-column replacement
//! for the game's F3 debug screen.
//!
//! Behaviour reimplemented from BetterF3 by cominixo and
//! TreyRuffy, MIT, <https://github.com/TreyRuffy/BetterF3>, via
//! DeportedToHell's Horizon port; this file shares no code with
//! either — this is a fresh implementation against the Kestrel
//! SDK. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/betterf3>.
//!
//! Fidelity, against the original's registry description: the
//! takeover is `screen.claim-debug` (API 22) — while this mod
//! runs, the game does not draw its own debug entries, and the
//! mod draws its modules with `render` shapes when
//! `screen.state` says the debug overlay is open. Of the
//! original's thirteen modules, the eight the SDK can feed are
//! here — Minecraft, FPS, Coordinates, Chunks, Location, Help,
//! Misc, Targeted — each in its own colour, in two columns,
//! every module on or off, in either column, its colour cycling
//! a palette, with line backgrounds, text shadow, the sliding
//! animation and Auto Start, all on the settings page in
//! Options > Mods (and on the same page the mod's own config
//! key opens in game; the key starts unbound, as the platform's
//! own mods do). NOT ported, and named in the README rather
//! than imitated: the Graphics, Server, Entities, Sounds and
//! System modules and the memory line (the SDK has no read for
//! what they show), the slime-chunk line (no world-seed read),
//! the font scale (the SDK's text has no scale), hiding the
//! boss bar and scoreboard while F3 is open (no SDK surface
//! hides just those), module reordering and add/delete (the
//! settings page is buttons; the order here is the registry's
//! own module order), and the original's Cloth Config screens.
//! F3+M and F3+F stay the game's own debug keys.
//!
//! One layering limit, from the API 22 design note: the game's
//! own overlay draws over open screens; this replacement draws
//! in the HUD layer, under a screen that is open over the game.
//!
//! Reads happen in the tick hook (some are tick-only) and are
//! kept as [`Facts`]; the columns are rebuilt from them there,
//! and the frame hook only lays the shapes out and runs the
//! slide, so the text and geometry below are pure and tested.

use horizon_mod_sdk::{
    Mod, Registration, entity, game, input,
    input::Event,
    render::{self, FillShape, Point, Rect, Shape, TextShape},
    screen, storage,
    ui::{self, ButtonWidget, Widget},
    world,
};

/// The newest API level this mod needs, asserted in the tests
/// below: `screen.state` and `screen.claim-debug` are level 22
/// (`crates/horizon-mods/src/api_level.rs`); everything else is
/// older (`game.get-effects` 18, `world.get-lights` 12,
/// `entity.targeted` 10, `world.get-biome` 4, `game.raycast` 2,
/// the rest 0).
#[cfg(test)]
const API_LEVEL: u32 = 22;

/// The game this client is pinned to, as the Minecraft module
/// names it: the platform's one version.
const GAME_VERSION: &str = "26.3";

/// The registered key that opens the config screen in game. It
/// starts unbound (`key.keyboard.unknown`), like First Person
/// Body's toggle: the player binds it on the mod's page in
/// Options > Mods. F3+M is the game's chord, not the mod's to
/// take.
const CONFIG_KEY: &str = "config";
const CONFIG_KEY_DEFAULT: &str = "key.keyboard.unknown";

/// The distance the targeted block and entity are read over:
/// five blocks, the longest of vanilla's targeting reaches
/// (creative's), so the lines name what F3 would name.
const TARGET_DISTANCE: f32 = 5.0;

/// The layout, in GUI pixels: the columns' distance from the
/// screen's edges, the line pitch, the extra space before a
/// module's first line, and the background's padding around a
/// line's text.
const MARGIN: f32 = 2.0;
const LINE_HEIGHT: f32 = 10.0;
const MODULE_GAP: f32 = 4.0;
const BACKGROUND_PAD: f32 = 2.0;
/// The line background: black at about 40% opacity, as the
/// compass bar's backing is.
const BACKGROUND_COLOR: u32 = 0x6600_0000;

/// The value colour and the dim colour every module's lines
/// share: the label wears the module's colour, the values are
/// white, the asides grey — BetterF3's label/value split.
const VALUE_COLOR: u32 = 0xFF_FFFF;
const DIM_COLOR: u32 = 0xAA_AAAA;

/// The colours a module's colour button cycles through, in
/// order: the vanilla palette the settings page can name.
const PALETTE: [(&str, u32); 9] = [
    ("White", 0xFF_FFFF),
    ("Red", 0xFF_5555),
    ("Gold", 0xFF_AA00),
    ("Yellow", 0xFF_FF55),
    ("Green", 0x55_FF55),
    ("Aqua", 0x55_FFFF),
    ("Blue", 0x55_55FF),
    ("Light Purple", 0xFF_55FF),
    ("Gray", 0xAA_AAAA),
];

/// The modules, in the registry description's order, with the
/// five the SDK cannot feed left out (Graphics, Server,
/// Entities, Sounds, System — see the header). Each: its name,
/// its default colour's index in [`PALETTE`], and its default
/// column (true: right).
const MODULES: [(&str, usize, bool); 8] = [
    ("Minecraft", 2, false),
    ("FPS", 4, false),
    ("Coordinates", 5, false),
    ("Chunks", 3, false),
    ("Location", 7, false),
    ("Help", 0, true),
    ("Misc", 8, false),
    ("Targeted", 1, true),
];
const MINECRAFT: usize = 0;
const FPS: usize = 1;
const COORDINATES: usize = 2;
const CHUNKS: usize = 3;
const LOCATION: usize = 4;
const HELP: usize = 5;
const MISC: usize = 6;
const TARGETED: usize = 7;

/// The general settings page buttons.
const ENABLED_BUTTON: u32 = 0;
const BACKGROUNDS_BUTTON: u32 = 1;
const SHADOW_BUTTON: u32 = 2;
const ANIMATION_BUTTON: u32 = 3;
const AUTO_START_BUTTON: u32 = 4;
/// Module buttons start here: three per module (on/off, column,
/// colour), in module order.
const MODULE_BUTTON_BASE: u32 = 10;

/// Storage keys for the general settings.
const ENABLED_KEY: &str = "enabled";
const BACKGROUNDS_KEY: &str = "backgrounds";
const SHADOW_KEY: &str = "shadow";
const ANIMATION_KEY: &str = "animation";
const AUTO_START_KEY: &str = "auto_start";

/// One module's choices.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ModuleSetting {
    on: bool,
    right: bool,
    color: usize,
}

/// The mod's settings, as the page and the storage spell them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Settings {
    enabled: bool,
    backgrounds: bool,
    shadow: bool,
    animation: bool,
    auto_start: bool,
    modules: [ModuleSetting; 8],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            backgrounds: true,
            shadow: true,
            animation: true,
            // The original ships Auto Start as an option; its
            // own default is not in the registry text, so the
            // port starts it off — F3 behaves as the game's
            // until the player asks otherwise.
            auto_start: false,
            modules: std::array::from_fn(|i| ModuleSetting {
                on: true,
                right: MODULES[i].2,
                color: MODULES[i].1,
            }),
        }
    }
}

/// A stored on/off setting ("1" / "0"); `default` when
/// unreadable.
fn read_switch(key: &str, default: bool) -> bool {
    match storage::get(key) {
        Some(bytes) => match std::str::from_utf8(&bytes) {
            Ok("1") => true,
            Ok("0") => false,
            _ => default,
        },
        None => default,
    }
}

/// A stored palette index; `default` when unreadable.
fn read_color(key: &str, default: usize) -> usize {
    let Some(bytes) = storage::get(key) else {
        return default;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return default;
    };
    match text.trim().parse::<usize>() {
        Ok(v) if v < PALETTE.len() => v,
        _ => default,
    }
}

fn store_switch(key: &str, on: bool) {
    let _ = storage::set(key, if on { b"1" } else { b"0" });
}

impl Settings {
    fn read() -> Self {
        let mut settings = Settings::default();
        settings.enabled = read_switch(ENABLED_KEY, settings.enabled);
        settings.backgrounds = read_switch(BACKGROUNDS_KEY, settings.backgrounds);
        settings.shadow = read_switch(SHADOW_KEY, settings.shadow);
        settings.animation = read_switch(ANIMATION_KEY, settings.animation);
        settings.auto_start = read_switch(AUTO_START_KEY, settings.auto_start);
        for (i, module) in settings.modules.iter_mut().enumerate() {
            module.on = read_switch(&format!("module_{i}_on"), module.on);
            module.right = read_switch(&format!("module_{i}_right"), module.right);
            module.color = read_color(&format!("module_{i}_color"), module.color);
        }
        settings
    }

    /// The settings page's (and the config screen's) widgets:
    /// the five general switches, then three buttons per module.
    fn widgets(&self) -> Vec<Widget> {
        let on_off = |b: bool| if b { "On" } else { "Off" };
        let mut widgets = vec![
            button(
                ENABLED_BUTTON,
                format!("BetterF3: {}", on_off(self.enabled)),
            ),
            button(
                BACKGROUNDS_BUTTON,
                format!("Line Backgrounds: {}", on_off(self.backgrounds)),
            ),
            button(
                SHADOW_BUTTON,
                format!("Text Shadow: {}", on_off(self.shadow)),
            ),
            button(
                ANIMATION_BUTTON,
                format!("Sliding Animation: {}", on_off(self.animation)),
            ),
            button(
                AUTO_START_BUTTON,
                format!("Auto Start: {}", on_off(self.auto_start)),
            ),
        ];
        for (i, module) in self.modules.iter().enumerate() {
            let base = MODULE_BUTTON_BASE + (i as u32) * 3;
            let name = MODULES[i].0;
            widgets.push(button(base, format!("{name}: {}", on_off(module.on))));
            widgets.push(button(
                base + 1,
                format!(
                    "{name} Column: {}",
                    if module.right { "Right" } else { "Left" }
                ),
            ));
            widgets.push(button(
                base + 2,
                format!("{name} Color: {}", PALETTE[module.color].0),
            ));
        }
        widgets
    }
}

fn button(id: u32, label: String) -> Widget {
    Widget::Button(ButtonWidget { id, label })
}

/// Everything the overlay's text is built from, gathered in one
/// tick: plain values, so [`build_columns`] is pure.
#[derive(Clone, Debug, PartialEq)]
struct Facts {
    fps: u32,
    fps_min: u32,
    fps_low: u32,
    x: f64,
    y: f64,
    z: f64,
    yaw: f32,
    pitch: f32,
    /// Blocks per second, from the last two ticks' positions.
    velocity: f32,
    dimension: String,
    biome: Option<String>,
    /// (sky, block) light at the player's feet.
    light: Option<(u8, u8)>,
    /// The current day, from 1.
    day: u64,
    /// Whole days the world has run.
    days_played: u64,
    /// The in-game clock, (hour, minute).
    time: (u32, u32),
    /// The targeted block: its position and id.
    block: Option<(i32, i32, i32, String)>,
    /// The targeted entity: its display name, its kind's id,
    /// and its health where the client can vouch for it.
    entity: Option<(String, String, Option<f32>, f32)>,
}

/// One run of text on a line, in its colour (0xRRGGBB).
type Segment = (String, u32);

/// One drawn line: its segments, and whether a module gap
/// belongs before it.
#[derive(Clone, Debug, PartialEq)]
struct OutLine {
    segments: Vec<Segment>,
    gap_before: bool,
}

/// The block position of a point: the block it stands in.
fn block_of(v: f64) -> i32 {
    v.floor() as i32
}

/// The chunk a block coordinate belongs to: the shift keeps
/// the sign, as the game's own chunk maths does (block -1 is
/// chunk -1, place 15 within it).
fn chunk_of(block: i32) -> i32 {
    block >> 4
}

fn in_chunk(block: i32) -> i32 {
    block & 15
}

/// The speed between two positions a tick apart, in blocks per
/// second.
fn velocity_between(prev: (f64, f64, f64), cur: (f64, f64, f64)) -> f32 {
    let dx = cur.0 - prev.0;
    let dy = cur.1 - prev.1;
    let dz = cur.2 - prev.2;
    ((dx * dx + dy * dy + dz * dz).sqrt() * 20.0) as f32
}

/// The cardinal the yaw faces, and the axis text vanilla F3
/// pairs with it. The game's yaw is 0 facing south and turns
/// clockwise (west at 90, north at 180, east at 270); each
/// cardinal owns the 90 degrees centred on it.
fn facing(yaw: f32) -> (&'static str, &'static str) {
    let yaw = yaw.rem_euclid(360.0);
    if !(45.0..315.0).contains(&yaw) {
        ("south", "Towards positive Z")
    } else if yaw < 135.0 {
        ("west", "Towards negative X")
    } else if yaw < 225.0 {
        ("north", "Towards negative Z")
    } else {
        ("east", "Towards positive X")
    }
}

/// The yaw as F3 prints it: -180 to 180.
fn facing_yaw(yaw: f32) -> f32 {
    let yaw = yaw.rem_euclid(360.0);
    if yaw > 180.0 { yaw - 360.0 } else { yaw }
}

/// The in-game clock from the day time: day time 0 is 06:00,
/// 1000 ticks an hour.
fn clock_of(day_time: u64) -> (u32, u32) {
    let hour = ((day_time / 1000) + 6) % 24;
    let minute = ((day_time % 1000) * 60) / 1000;
    (hour as u32, minute as u32)
}

/// The colour the FPS value wears: green at 60 and over,
/// yellow from 30, red below — the bands the port reads
/// "coloured FPS" with (the registry names the colouring, not
/// its thresholds).
fn fps_color(fps: u32) -> u32 {
    if fps >= 60 {
        0x55_FF55
    } else if fps >= 30 {
        0xFF_FF55
    } else {
        0xFF_5555
    }
}

/// A registry id as the overlay prints it: a `minecraft:` id
/// loses its namespace and reads title-cased ("Plains",
/// "The Nether"); any other namespace's id is data and stays
/// as it is.
fn pretty_id(id: &str) -> String {
    let Some(path) = id.strip_prefix("minecraft:") else {
        return id.to_string();
    };
    path.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One module's lines for these facts: the caller adds the
/// module gap. An empty list means the module shows nothing
/// this tick (Targeted with nothing targeted).
fn module_lines(module: usize, color: u32, f: &Facts) -> Vec<Vec<Segment>> {
    let label = |text: &str| (text.to_string(), color);
    let value = |text: String| (text, VALUE_COLOR);
    let dim = |text: String| (text, DIM_COLOR);
    match module {
        MINECRAFT => vec![vec![label("Minecraft"), value(format!(" {GAME_VERSION}"))]],
        FPS => vec![
            vec![label("FPS: "), (f.fps.to_string(), fps_color(f.fps))],
            vec![
                label("Min: "),
                value(f.fps_min.to_string()),
                dim("  1% Low: ".to_string()),
                value(f.fps_low.to_string()),
            ],
        ],
        COORDINATES => {
            let (bx, by, bz) = (block_of(f.x), block_of(f.y), block_of(f.z));
            let (name, axis) = facing(f.yaw);
            vec![
                vec![
                    label("XYZ: "),
                    value(format!("{:.2} / {:.2} / {:.2}", f.x, f.y, f.z)),
                ],
                vec![label("Block: "), value(format!("{bx} {by} {bz}"))],
                vec![
                    label("Facing: "),
                    value(format!("{name} ({axis})")),
                    dim(format!(" ({:.1} / {:.1})", facing_yaw(f.yaw), f.pitch)),
                ],
                vec![label("Velocity: "), value(format!("{:.2} b/s", f.velocity))],
            ]
        }
        CHUNKS => {
            let (bx, by, bz) = (block_of(f.x), block_of(f.y), block_of(f.z));
            vec![
                vec![
                    label("Chunk: "),
                    value(format!(
                        "{} {} {}",
                        chunk_of(bx),
                        chunk_of(by),
                        chunk_of(bz)
                    )),
                ],
                vec![
                    label("In Chunk: "),
                    value(format!(
                        "{} {} {}",
                        in_chunk(bx),
                        in_chunk(by),
                        in_chunk(bz)
                    )),
                ],
            ]
        }
        LOCATION => {
            let mut lines = vec![vec![label("Dimension: "), value(pretty_id(&f.dimension))]];
            if let Some(biome) = &f.biome {
                lines.push(vec![label("Biome: "), value(pretty_id(biome))]);
            }
            if let Some((sky, block)) = f.light {
                lines.push(vec![
                    label("Light: "),
                    value(format!("{} ({sky} sky, {block} block)", sky.max(block))),
                ]);
            }
            lines
        }
        HELP => vec![
            vec![dim("Config: Options > Mods > BetterF3".to_string())],
            vec![dim("Config Key: bind it on that page".to_string())],
        ],
        MISC => vec![
            vec![label("Day: "), value(f.day.to_string())],
            vec![
                label("Time: "),
                value(format!("{:02}:{:02}", f.time.0, f.time.1)),
            ],
            vec![label("Days Played: "), value(f.days_played.to_string())],
        ],
        TARGETED => {
            let mut lines = Vec::new();
            if let Some((bx, by, bz, id)) = &f.block {
                lines.push(vec![
                    label("Targeted Block: "),
                    value(format!("{bx}, {by}, {bz}")),
                ]);
                lines.push(vec![dim(id.clone())]);
            }
            if let Some((name, kind, health, max_health)) = &f.entity {
                lines.push(vec![label("Targeted Entity: "), value(name.clone())]);
                lines.push(vec![dim(kind.clone())]);
                if let Some(health) = health {
                    lines.push(vec![
                        label("Health: "),
                        value(format!("{health:.1} / {max_health:.1}")),
                    ]);
                }
            }
            lines
        }
        _ => Vec::new(),
    }
}

/// The two columns for these settings and facts: module order
/// within a column is the registry's order, a module's first
/// line carries the gap that separates it from the one above.
fn build_columns(settings: &Settings, facts: Option<&Facts>) -> [Vec<OutLine>; 2] {
    let mut columns: [Vec<OutLine>; 2] = [Vec::new(), Vec::new()];
    let Some(facts) = facts else { return columns };
    for (i, module) in settings.modules.iter().enumerate() {
        if !module.on {
            continue;
        }
        let color = PALETTE[module.color].1;
        let lines = module_lines(i, color, facts);
        let column = &mut columns[module.right as usize];
        for (j, segments) in lines.into_iter().enumerate() {
            column.push(OutLine {
                segments,
                gap_before: j == 0 && !column.is_empty(),
            });
        }
    }
    columns
}

/// A line's width in GUI pixels: its segments end to end.
fn line_width(line: &OutLine, measure: &dyn Fn(&str) -> f32) -> f32 {
    line.segments.iter().map(|(text, _)| measure(text)).sum()
}

/// The eased slide: quick off the mark, settling into place.
fn ease_out(phase: f32) -> f32 {
    let rest = 1.0 - phase.clamp(0.0, 1.0);
    1.0 - rest * rest * rest
}

/// The overlay's shapes for two columns at a slide `phase`
/// (0: off screen, 1: at rest): a line's background first, then
/// its segments in order; the left column slides in from the
/// left edge, the right from the right. Pure but for the
/// injected text measure, so the geometry is testable.
fn overlay_shapes(
    columns: &[Vec<OutLine>; 2],
    gui_width: f32,
    phase: f32,
    backgrounds: bool,
    shadow: bool,
    measure: &dyn Fn(&str) -> f32,
) -> Vec<Shape> {
    let eased = ease_out(phase);
    let mut shapes = Vec::new();
    for (side, column) in columns.iter().enumerate() {
        if column.is_empty() {
            continue;
        }
        let widths: Vec<f32> = column.iter().map(|l| line_width(l, measure)).collect();
        let column_width = widths.iter().copied().fold(0.0f32, f32::max);
        // The column's left edge at rest, and how far the slide
        // holds it off screen at this phase.
        let (rest_x, off) = if side == 0 {
            (
                MARGIN,
                -(column_width + MARGIN + BACKGROUND_PAD) * (1.0 - eased),
            )
        } else {
            (
                gui_width - MARGIN - column_width,
                (column_width + MARGIN + BACKGROUND_PAD) * (1.0 - eased),
            )
        };
        let base_x = rest_x + off;
        let mut y = MARGIN;
        for (line, width) in column.iter().zip(&widths) {
            if line.gap_before {
                y += MODULE_GAP;
            }
            // A right column's lines right-align to its edge.
            let line_x = if side == 0 {
                base_x
            } else {
                base_x + (column_width - width)
            };
            if backgrounds {
                shapes.push(Shape::Fill(FillShape {
                    rect: Rect {
                        x: line_x - BACKGROUND_PAD,
                        y: y - 1.0,
                        w: width + BACKGROUND_PAD * 2.0,
                        h: LINE_HEIGHT,
                    },
                    color: BACKGROUND_COLOR,
                }));
            }
            let mut x = line_x;
            for (text, color) in &line.segments {
                shapes.push(Shape::Text(TextShape {
                    at: Point { x, y },
                    color: *color,
                    text: text.clone(),
                    centered: false,
                    shadow,
                }));
                x += measure(text);
            }
            y += LINE_HEIGHT;
        }
    }
    shapes
}

struct BetterF3 {
    settings: Settings,
    /// The last tick's facts; none off world (the title).
    facts: Option<Facts>,
    /// The columns built from them, rebuilt each tick.
    columns: [Vec<OutLine>; 2],
    /// The previous tick's position and dimension, for the
    /// velocity: a dimension change restarts it at rest.
    prev_pos: Option<(f64, f64, f64, String)>,
    /// The debug overlay should show: the game's F3 state, or
    /// Auto Start's hold on a fresh world.
    shown: bool,
    /// Auto Start is holding the overlay open on this world
    /// entry; any F3 press hands control to the game's state.
    auto_engaged: bool,
    /// Last tick's debug-open, to see F3 move at all.
    prev_debug_open: bool,
    /// Last tick's in-world state, to see a world entry.
    was_in_world: bool,
    /// The slide's phase, 0 (off screen) to 1 (at rest).
    phase: f32,
    /// The key of the shapes currently on screen, so an
    /// unchanged frame costs no call; `None` while nothing is
    /// drawn.
    drawn_key: Option<String>,
    /// The mod's own config screen is open: selects from it
    /// re-open it, selects from the settings page don't.
    config_open: bool,
}

impl BetterF3 {
    /// Reads one tick's facts into place and rebuilds the
    /// columns; the reads that are tick-only live here.
    fn gather(&mut self) {
        let stats = render::frame_stats();
        let Some(player) = game::get_player() else {
            self.facts = None;
            self.prev_pos = None;
            self.columns = build_columns(&self.settings, None);
            return;
        };
        let velocity = match &self.prev_pos {
            Some((px, py, pz, dim)) if *dim == player.dimension => {
                velocity_between((*px, *py, *pz), (player.x, player.y, player.z))
            }
            _ => 0.0,
        };
        self.prev_pos = Some((player.x, player.y, player.z, player.dimension.clone()));
        let (bx, by, bz) = (block_of(player.x), block_of(player.y), block_of(player.z));
        let biome = world::get_biome(&player.dimension, bx, by, bz);
        let light = world::get_lights(
            &player.dimension,
            &[world::BlockPos {
                x: bx,
                y: by,
                z: bz,
            }],
        )
        .first()
        .and_then(|result| result.light)
        .map(|light| (light.sky, light.block));
        let clock = game::get_clock();
        let (day, days_played, time) = match &clock {
            Some(c) => (
                c.day_time / 24000 + 1,
                c.game_time / 24000,
                clock_of(c.day_time),
            ),
            None => (1, 0, (6, 0)),
        };
        let block = game::raycast(TARGET_DISTANCE)
            .map(|hit| (hit.block_x, hit.block_y, hit.block_z, hit.block));
        let entity = entity::targeted(TARGET_DISTANCE).map(|target| {
            (
                target.entity.name,
                target.entity.kind,
                target.entity.health,
                target.entity.max_health,
            )
        });
        self.facts = Some(Facts {
            fps: stats.fps,
            fps_min: stats.min,
            fps_low: stats.low,
            x: player.x,
            y: player.y,
            z: player.z,
            yaw: player.yaw,
            pitch: player.pitch,
            velocity,
            dimension: player.dimension,
            biome,
            light,
            day,
            days_played,
            time,
            block,
            entity,
        });
        self.columns = build_columns(&self.settings, self.facts.as_ref());
    }

    /// Works out whether the overlay shows this tick: F3's own
    /// state, with Auto Start holding it open from a world
    /// entry until F3 is pressed either way.
    fn update_shown(&mut self) {
        let state = screen::state();
        let in_world = game::get_player().is_some();
        if in_world && !self.was_in_world {
            self.auto_engaged = self.settings.auto_start && self.settings.enabled;
        }
        if !in_world || state.debug_open != self.prev_debug_open {
            self.auto_engaged = false;
        }
        self.prev_debug_open = state.debug_open;
        self.was_in_world = in_world;
        self.shown = self.settings.enabled && in_world && (state.debug_open || self.auto_engaged);
    }

    /// The shapes' identity: everything that would change them
    /// at rest, so a still frame skips the call.
    fn shapes_key(&self, gui_width: f32) -> String {
        let mut key = format!(
            "{}x{}",
            gui_width as i32,
            self.settings.backgrounds as u8 + self.settings.shadow as u8 * 2
        );
        for column in &self.columns {
            for line in column {
                key.push('|');
                for (text, color) in &line.segments {
                    key.push_str(&format!("{text}:{color:x};"));
                }
            }
        }
        key
    }

    /// Opens the config screen with the settings widgets: the
    /// same page Options > Mods shows, in game.
    fn open_config(&mut self) {
        self.config_open = true;
        ui::open_screen(&ui::Screen {
            title: "BetterF3".to_string(),
            widgets: self.settings.widgets(),
        });
    }

    /// Sends the overlay's shapes for this frame, or takes them
    /// down; a no-op when nothing changed and no slide runs.
    fn draw(&mut self, seconds: f32) {
        let target = if self.shown { 1.0 } else { 0.0 };
        if self.settings.animation {
            let step = seconds.max(0.0) / 0.3;
            if self.phase < target {
                self.phase = (self.phase + step).min(target);
            } else if self.phase > target {
                self.phase = (self.phase - step).max(target);
            }
        } else {
            self.phase = target;
        }
        if self.phase <= 0.0 {
            if self.drawn_key.is_some() {
                render::set_hud(&[]);
                self.drawn_key = None;
            }
            return;
        }
        let (width, _) = render::gui_size();
        let animating = self.phase != target;
        let key = self.shapes_key(width);
        if !animating && self.drawn_key.as_deref() == Some(key.as_str()) {
            return;
        }
        let shapes = overlay_shapes(
            &self.columns,
            width,
            self.phase,
            self.settings.backgrounds,
            self.settings.shadow,
            &|text| render::text_width(text),
        );
        render::set_hud(&shapes);
        self.drawn_key = Some(key);
    }
}

impl Mod for BetterF3 {
    fn init() -> Result<(Self, Registration), String> {
        let settings = Settings::read();
        input::register_key(CONFIG_KEY, "Open BetterF3 Config", CONFIG_KEY_DEFAULT)?;
        screen::claim_debug(settings.enabled);
        Ok((
            BetterF3 {
                settings,
                facts: None,
                columns: [Vec::new(), Vec::new()],
                prev_pos: None,
                shown: false,
                auto_engaged: false,
                prev_debug_open: false,
                was_in_world: false,
                phase: 0.0,
                drawn_key: None,
                config_open: false,
            },
            Registration::new().on_tick().on_frame(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        self.gather();
        self.update_shown();
        for event in input::poll() {
            if let Event::KeyDown(name) = event
                && name == CONFIG_KEY
            {
                self.open_config();
            }
        }
    }

    fn frame(&mut self, seconds: f32) {
        self.draw(seconds);
    }

    fn ui_settings_opened(&mut self) {
        ui::set_settings(&self.settings.widgets());
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        let settings = &mut self.settings;
        match id {
            ENABLED_BUTTON => {
                settings.enabled = !settings.enabled;
                store_switch(ENABLED_KEY, settings.enabled);
                screen::claim_debug(settings.enabled);
            }
            BACKGROUNDS_BUTTON => {
                settings.backgrounds = !settings.backgrounds;
                store_switch(BACKGROUNDS_KEY, settings.backgrounds);
            }
            SHADOW_BUTTON => {
                settings.shadow = !settings.shadow;
                store_switch(SHADOW_KEY, settings.shadow);
            }
            ANIMATION_BUTTON => {
                settings.animation = !settings.animation;
                store_switch(ANIMATION_KEY, settings.animation);
            }
            AUTO_START_BUTTON => {
                settings.auto_start = !settings.auto_start;
                store_switch(AUTO_START_KEY, settings.auto_start);
            }
            id if id >= MODULE_BUTTON_BASE => {
                let offset = id - MODULE_BUTTON_BASE;
                let (index, part) = ((offset / 3) as usize, offset % 3);
                let Some(module) = settings.modules.get_mut(index) else {
                    return;
                };
                match part {
                    0 => {
                        module.on = !module.on;
                        store_switch(&format!("module_{index}_on"), module.on);
                    }
                    1 => {
                        module.right = !module.right;
                        store_switch(&format!("module_{index}_right"), module.right);
                    }
                    _ => {
                        module.color = (module.color + 1) % PALETTE.len();
                        let _ = storage::set(
                            &format!("module_{index}_color"),
                            module.color.to_string().as_bytes(),
                        );
                    }
                }
            }
            _ => return,
        }
        self.columns = build_columns(&self.settings, self.facts.as_ref());
        if self.config_open {
            self.open_config();
        } else {
            ui::set_settings(&self.settings.widgets());
        }
    }

    fn ui_closed(&mut self) {
        self.config_open = false;
    }
}

horizon_mod_sdk::export_mod!(BetterF3);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_22() {
        assert_eq!(API_LEVEL, 22);
    }

    /// A standing player in the plains at noon on day 2,
    /// looking north at a stone block.
    fn facts() -> Facts {
        Facts {
            fps: 120,
            fps_min: 90,
            fps_low: 75,
            x: 100.25,
            y: 64.0,
            z: -200.5,
            yaw: 180.0,
            pitch: 10.0,
            velocity: 0.0,
            dimension: "minecraft:overworld".into(),
            biome: Some("minecraft:plains".into()),
            light: Some((15, 0)),
            day: 2,
            days_played: 1,
            time: (12, 0),
            block: Some((100, 63, -201, "minecraft:stone".into())),
            entity: None,
        }
    }

    fn texts(lines: &[Vec<Segment>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.iter().map(|(t, _)| t.as_str()).collect::<String>())
            .collect()
    }

    #[test]
    fn chunk_maths_keeps_the_sign() {
        assert_eq!((chunk_of(0), in_chunk(0)), (0, 0));
        assert_eq!((chunk_of(15), in_chunk(15)), (0, 15));
        assert_eq!((chunk_of(16), in_chunk(16)), (1, 0));
        assert_eq!((chunk_of(-1), in_chunk(-1)), (-1, 15));
        assert_eq!((chunk_of(-16), in_chunk(-16)), (-1, 0));
        assert_eq!((chunk_of(-17), in_chunk(-17)), (-2, 15));
        assert_eq!(block_of(-0.5), -1);
        assert_eq!(block_of(64.0), 64);
    }

    #[test]
    fn facing_names_the_cardinals() {
        assert_eq!(facing(0.0), ("south", "Towards positive Z"));
        assert_eq!(facing(44.9).0, "south");
        assert_eq!(facing(45.0), ("west", "Towards negative X"));
        assert_eq!(facing(90.0).0, "west");
        assert_eq!(facing(135.0), ("north", "Towards negative Z"));
        assert_eq!(facing(180.0).0, "north");
        assert_eq!(facing(225.0), ("east", "Towards positive X"));
        assert_eq!(facing(314.9).0, "east");
        assert_eq!(facing(315.0).0, "south");
        assert_eq!(facing(-90.0).0, "east"); // wraps to 270
        assert_eq!(facing_yaw(180.0), 180.0);
        assert_eq!(facing_yaw(270.0), -90.0);
        assert_eq!(facing_yaw(-45.0), -45.0);
    }

    #[test]
    fn the_clock_reads_the_day() {
        assert_eq!(clock_of(0), (6, 0));
        assert_eq!(clock_of(1000), (7, 0));
        assert_eq!(clock_of(6000), (12, 0));
        assert_eq!(clock_of(12500), (18, 30));
        assert_eq!(clock_of(18000), (0, 0));
        assert_eq!(clock_of(23999), (5, 59));
    }

    #[test]
    fn fps_wears_its_band() {
        assert_eq!(fps_color(240), 0x55_FF55);
        assert_eq!(fps_color(60), 0x55_FF55);
        assert_eq!(fps_color(59), 0xFF_FF55);
        assert_eq!(fps_color(30), 0xFF_FF55);
        assert_eq!(fps_color(29), 0xFF_5555);
        assert_eq!(fps_color(0), 0xFF_5555);
    }

    #[test]
    fn ids_read_pretty() {
        assert_eq!(pretty_id("minecraft:plains"), "Plains");
        assert_eq!(pretty_id("minecraft:the_nether"), "The Nether");
        assert_eq!(pretty_id("minecraft:the_end"), "The End");
        assert_eq!(pretty_id("minecraft:overworld"), "Overworld");
        assert_eq!(pretty_id("example:deep_dark"), "example:deep_dark");
    }

    #[test]
    fn velocity_is_blocks_per_second() {
        assert_eq!(velocity_between((0.0, 64.0, 0.0), (0.0, 64.0, 0.0)), 0.0);
        // A block a tick sideways is 20 blocks a second.
        assert_eq!(velocity_between((0.0, 64.0, 0.0), (1.0, 64.0, 0.0)), 20.0);
        // 3-4-5: five blocks a tick diagonally.
        assert_eq!(velocity_between((0.0, 0.0, 0.0), (0.15, 0.0, 0.2)), 5.0);
    }

    #[test]
    fn columns_carry_the_modules_in_order() {
        let settings = Settings::default();
        let columns = build_columns(&settings, Some(&facts()));
        let left_texts: Vec<String> = columns[0]
            .iter()
            .map(|l| l.segments.iter().map(|(t, _)| t.as_str()).collect())
            .collect();
        assert_eq!(
            left_texts,
            [
                "Minecraft 26.3",
                "FPS: 120",
                "Min: 90  1% Low: 75",
                "XYZ: 100.25 / 64.00 / -200.50",
                "Block: 100 64 -201",
                "Facing: north (Towards negative Z) (180.0 / 10.0)",
                "Velocity: 0.00 b/s",
                "Chunk: 6 4 -13",
                "In Chunk: 4 0 7",
                "Dimension: Overworld",
                "Biome: Plains",
                "Light: 15 (15 sky, 0 block)",
                "Day: 2",
                "Time: 12:00",
                "Days Played: 1",
            ]
        );
        let right_texts: Vec<String> = columns[1]
            .iter()
            .map(|l| l.segments.iter().map(|(t, _)| t.as_str()).collect())
            .collect();
        assert_eq!(
            right_texts,
            [
                "Config: Options > Mods > BetterF3",
                "Config Key: bind it on that page",
                "Targeted Block: 100, 63, -201",
                "minecraft:stone",
            ]
        );
        // The gaps: only the first line of each column has none.
        assert!(!columns[0][0].gap_before);
        assert!(columns[0][1].gap_before); // FPS after Minecraft
        assert!(columns[0][3].gap_before); // Coordinates after FPS
        assert!(!columns[0][4].gap_before); // within a module
        assert!(columns[1][2].gap_before); // Targeted after Help
        // Labels wear their module's colour, values white.
        assert_eq!(
            columns[0][3].segments[0].1,
            PALETTE[MODULES[COORDINATES].1].1
        );
        assert_eq!(columns[0][3].segments[1].1, VALUE_COLOR);
        assert_eq!(columns[0][1].segments[1], ("120".to_string(), 0x55_FF55));
    }

    #[test]
    fn modules_switch_off_and_change_sides() {
        let mut settings = Settings::default();
        settings.modules[FPS].on = false;
        settings.modules[TARGETED].right = false;
        let columns = build_columns(&settings, Some(&facts()));
        let left: Vec<String> = columns[0]
            .iter()
            .map(|l| l.segments.iter().map(|(t, _)| t.as_str()).collect())
            .collect();
        assert!(!left.iter().any(|l| l.starts_with("FPS:")));
        assert!(left.iter().any(|l| l.starts_with("Targeted Block:")));
        let right: Vec<String> = columns[1]
            .iter()
            .map(|l| l.segments.iter().map(|(t, _)| t.as_str()).collect())
            .collect();
        assert_eq!(right.len(), 2); // Help alone remains
        // No facts: no columns at all.
        let empty = build_columns(&settings, None);
        assert!(empty[0].is_empty() && empty[1].is_empty());
    }

    #[test]
    fn targeted_needs_a_target() {
        let mut f = facts();
        f.block = None;
        let settings = Settings::default();
        let columns = build_columns(&settings, Some(&f));
        assert_eq!(columns[1].len(), 2, "Help alone on the right");
        // An entity with a vouched health adds its lines.
        f.entity = Some(("Zombie".into(), "minecraft:zombie".into(), Some(14.0), 20.0));
        let lines = module_lines(TARGETED, 0xFF_5555, &f);
        assert_eq!(
            texts(&lines),
            [
                "Targeted Entity: Zombie",
                "minecraft:zombie",
                "Health: 14.0 / 20.0"
            ]
        );
        // Without a health the line goes, the rest stays.
        f.entity = Some(("Cow".into(), "minecraft:cow".into(), None, 10.0));
        let lines = module_lines(TARGETED, 0xFF_5555, &f);
        assert_eq!(texts(&lines), ["Targeted Entity: Cow", "minecraft:cow"]);
        // Missing biome and light lines drop out of Location.
        f.biome = None;
        f.light = None;
        let lines = module_lines(LOCATION, 0xFF_55FF, &f);
        assert_eq!(texts(&lines), ["Dimension: Overworld"]);
    }

    #[test]
    fn shapes_slide_in_from_both_edges() {
        let measure = |s: &str| s.len() as f32 * 6.0;
        let settings = Settings::default();
        let columns = build_columns(&settings, Some(&facts()));
        // At rest: the left column starts at the margin, the
        // right column ends at the far margin, every line has a
        // background when they are on.
        let shapes = overlay_shapes(&columns, 854.0, 1.0, true, true, &measure);
        let texts: Vec<&TextShape> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        let fills = shapes
            .iter()
            .filter(|s| matches!(s, Shape::Fill(_)))
            .count();
        assert_eq!(fills, columns[0].len() + columns[1].len());
        let first = texts.first().unwrap();
        assert_eq!((first.text.as_str(), first.at.x), ("Minecraft", MARGIN));
        // The widest right-column line ends at the far margin.
        let right_end = texts
            .iter()
            .map(|t| t.at.x + measure(&t.text))
            .fold(0.0f32, f32::max);
        assert!(
            (right_end - (854.0 - MARGIN)).abs() < 0.01,
            "right end {right_end}"
        );
        // Mid-slide (phase 0 = fully out, eased): the left
        // column is off the left edge, the right off the right.
        let out = overlay_shapes(&columns, 854.0, 0.0, true, true, &measure);
        let out_texts: Vec<&TextShape> = out
            .iter()
            .filter_map(|s| match s {
                Shape::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        let min_x = out_texts.iter().map(|t| t.at.x).fold(f32::MAX, f32::min);
        assert!(min_x < 0.0, "left column out: {min_x}");
        // No backgrounds: no fills at all.
        let bare = overlay_shapes(&columns, 854.0, 1.0, false, false, &measure);
        assert!(bare.iter().all(|s| matches!(s, Shape::Text(_))));
        // The shadow flag reaches the text shapes.
        assert!(texts.iter().all(|t| t.shadow));
        assert!(bare.iter().all(|s| match s {
            Shape::Text(t) => !t.shadow,
            _ => true,
        }));
    }

    #[test]
    fn settings_labels_name_the_choices() {
        let widgets = Settings::default().widgets();
        assert_eq!(widgets.len(), 5 + 8 * 3);
        let labels: Vec<String> = widgets
            .iter()
            .map(|w| match w {
                Widget::Button(b) => b.label.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(labels[0], "BetterF3: On");
        assert_eq!(labels[1], "Line Backgrounds: On");
        assert_eq!(labels[4], "Auto Start: Off");
        assert_eq!(labels[5], "Minecraft: On");
        assert_eq!(labels[6], "Minecraft Column: Left");
        assert_eq!(labels[7], "Minecraft Color: Gold");
        assert_eq!(labels[26], "Targeted: On");
        assert_eq!(labels[27], "Targeted Column: Right");
        assert_eq!(labels[28], "Targeted Color: Red");
    }
}

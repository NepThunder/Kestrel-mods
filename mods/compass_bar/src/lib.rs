//! Compass Bar for Kestrel: a compass bar at the top of the screen
//! that shows where you are facing, in degrees, and slides smoothly
//! as you turn.
//!
//! Behaviour reimplemented from Compass Enhanced by Isaac
//! (Fernandes585), MIT,
//! <https://github.com/Fernandes585/Compass-Enhanced>: this file
//! shares no code with the original — the original is a sandboxed
//! `.hmod` for Horizon, this is a fresh implementation against the
//! Kestrel SDK. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/compass_bar>.
//!
//! Fidelity, against the original's registry description: the bar
//! and its settings match — the eight compass letters in one colour
//! the player picks, the degree scale, the optional background box,
//! the four sizes (75% to 150%), all in Options > Mods, and, as the
//! original says, no commands. The death points arrived in 0.2.0
//! with the SDK's map-data district (API 25): a point where you
//! died, marked on the bar at its bearing with its distance, an
//! arrow at the bar's edge when it is out of view, the marker
//! turning into an X inside the arrival distance and going after
//! the X time, a dismiss key (X), and the three death-point
//! settings — arrival distance, X time, and how many points are
//! kept. The points are not saved: they vanish when the game
//! closes, as the original's do. Two readings are the port's, both
//! stated in the README: a point is also seeded from the game's
//! own last-death record when a world is entered (the standing
//! read the district prescribes; the original, itself a mod, only
//! ever sees deaths it was there for), and the arrival countdown
//! runs while you stay in range — step out and the X lifts.
//!
//! The bar itself is drawn with the SDK's HUD shapes
//! (`render.set-hud`) around the top centre of the screen
//! (`render.gui-size`), from `game.get-player`'s yaw. One shape
//! list is kept per yaw quantum and size: the game replaces shapes
//! wholesale, so an unchanged list costs no call.

use horizon_mod_sdk::{
    Mod, Registration, game, input,
    input::Event,
    map,
    render::{self, FillShape, Point, Rect, Shape, TextShape},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: the death points read the map surface — `map.context`,
/// `map.last-death` and `map.poll-events` are level 25
/// (`crates/horizon-mods/src/api_level.rs`); the bar's own reads
/// are level 0.
#[cfg(test)]
const API_LEVEL: u32 = 25;

/// The storage keys for the settings.
const ENABLED_KEY: &str = "enabled";
const BACKGROUND_KEY: &str = "background";
const SCALE_KEY: &str = "scale";
const COLOR_KEY: &str = "color";
const ARRIVAL_KEY: &str = "arrival";
const XTIME_KEY: &str = "xtime";
const KEPT_KEY: &str = "kept";

/// The settings page's buttons, one per setting.
const ENABLED_BUTTON: u32 = 0;
const BACKGROUND_BUTTON: u32 = 1;
const SCALE_BUTTON: u32 = 2;
const COLOR_BUTTON: u32 = 3;
const ARRIVAL_BUTTON: u32 = 4;
const XTIME_BUTTON: u32 = 5;
const KEPT_BUTTON: u32 = 6;

/// The dismiss key's registration name; the original's default
/// is X, and the registration is what Options > Mods rebinds.
const DISMISS_KEY: &str = "dismiss";

/// The death-point settings' cycles. The registry names the
/// three settings but no values or defaults; these cycles and
/// their defaults (5 blocks, 10 seconds, 3 kept) are the
/// port's choices, stated in the README the way 0.1.0 stated
/// its letter-colour palette.
const ARRIVALS: [u32; 6] = [2, 3, 5, 8, 12, 20];
const ARRIVAL_DEFAULT: usize = 2;
const X_TIMES: [u32; 5] = [5, 10, 15, 30, 60];
const X_TIME_DEFAULT: usize = 1;
const KEPTS: [usize; 5] = [1, 2, 3, 5, 10];
const KEPT_DEFAULT: usize = 2;

/// The death marker's colour: the red of a death point.
const DEATH_COLOR: u32 = 0xFFFF_5555;
const DEATH_TEXT_COLOR: u32 = 0x00FF_5555;

/// The bar's vertical layout: its top edge below the screen's top,
/// its height, the centre marker's height, and the gap between the
/// letters' baseline and the degree numbers' baseline. Text is
/// placed by its top, so the offsets below are worked additions.
const BAR_TOP: f32 = 6.0;
const BAR_HEIGHT: f32 = 22.0;
const MARKER_HEIGHT: f32 = 5.0;
const LETTER_LIFT: f32 = 2.0;
const DEGREE_BASE: f32 = 11.0;

/// The angular spacing of the scale: a numbered tick every 30
/// degrees, a short tick every 15, the smallest tick every 5. The
/// bar shows 120 degrees across its width at 100% size.
const VIEW_DEGREES: f32 = 120.0;
const NUMBER_EVERY: u32 = 30;
const MID_EVERY: u32 = 15;
const TICK_EVERY: u32 = 5;

/// The background box's colour: black at about 40% opacity.
const BACKGROUND_COLOR: u32 = 0x6600_0000;
/// The centre marker and the tick marks: plain white.
const MARK_COLOR: u32 = 0xFFFF_FFFF;
/// The degree numbers: a light grey, readable over the world.
const DEGREE_COLOR: u32 = 0x00E0_E0E0;
/// The letter colours the colour setting cycles through — the
/// colour packs a compass of this kind ships. The registry says
/// only "a color you choose"; these six are the port's palette.
const LETTER_COLORS: [u32; 6] = [
    0xFFFF_FFFF, // white
    0xFFFF_5555, // red
    0xFFFF_AA00, // gold
    0xFF55_FFFF, // aqua
    0xFF55_FF55, // green
    0xFFFF_99CC, // pink
];
const COLOR_NAMES: [&str; 6] = ["White", "Red", "Gold", "Aqua", "Green", "Pink"];

/// The four sizes of the original's setting, as scale factors.
const SCALES: [f32; 4] = [0.75, 1.0, 1.25, 1.5];

/// The eight compass points: label, heading in degrees (0 is
/// north, clockwise, as the scale reads) and yaw offset from south
/// (the game's yaw for the same facing).
const POINTS: [(&str, u32, f32); 8] = [
    ("S", 180, 0.0),
    ("SW", 225, 45.0),
    ("W", 270, 90.0),
    ("NW", 315, 135.0),
    ("N", 0, 180.0),
    ("NE", 45, 225.0),
    ("E", 90, 270.0),
    ("SE", 135, 315.0),
];

/// The player's heading in degrees: 0 north, 90 east, 180 south,
/// 270 west, always in [0, 360). The game's yaw is 0 facing south
/// and turns clockwise (west at 90, north at 180, east at 270), so
/// the heading is the yaw shifted by half a turn.
fn heading(yaw: f32) -> f32 {
    (yaw + 180.0).rem_euclid(360.0)
}

/// The signed smallest turn from `from` to `to`, in (-180, 180]:
/// how far a mark at heading `to` sits from the player's heading,
/// positive to the right of the view's centre.
fn angle_delta(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

/// GUI pixels per degree at a scale: the bar shows `VIEW_DEGREES`
/// across a base width the size scales — half the screen's width
/// at 100% — so a bigger size magnifies the strip and shows less
/// of the circle, the way the game's own zoom of a bar reads.
fn px_per_degree(width: f32, scale: f32) -> f32 {
    (width / 2.0) * scale / VIEW_DEGREES
}

/// The bar's own width at a scale, in GUI pixels.
fn bar_width(width: f32, scale: f32) -> f32 {
    VIEW_DEGREES * px_per_degree(width, scale)
}

/// Is a mark at this centre offset inside the bar (with a margin
/// for its own width)?
fn visible(offset: f32, half: f32) -> bool {
    offset.abs() <= half
}

/// One death point, in memory only: the original does not
/// save its points, and neither does the port — the list is
/// empty every time the game starts, filled by the deaths
/// the mod sees and by the one seed from the game's own
/// last-death record on entering a world.
#[derive(Clone, PartialEq, Debug)]
struct DeathPoint {
    x: i32,
    y: i32,
    z: i32,
    dimension: String,
    /// The tick the player first came within the arrival
    /// distance; the X shows while it is set, and the point
    /// goes once the X time has run from it. Stepping out of
    /// range clears it — the countdown is time in range.
    arrived_tick: Option<u64>,
}

/// The heading from the player to a point, in the compass's
/// degrees: 0 north (−z), 90 east (+x), always in [0, 360).
fn bearing(px: f64, pz: f64, x: i32, z: i32) -> f32 {
    let dx = f64::from(x) + 0.5 - px;
    let dz = f64::from(z) + 0.5 - pz;
    (dx.atan2(-dz).to_degrees() + 360.0).rem_euclid(360.0) as f32
}

/// The 3D distance from the player to a point's centre.
fn point_distance(p: (f64, f64, f64), x: i32, y: i32, z: i32) -> f64 {
    let dx = f64::from(x) + 0.5 - p.0;
    let dy = f64::from(y) - p.1;
    let dz = f64::from(z) + 0.5 - p.2;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// A death point as the bar draws it: where it sits against
/// the player's heading and how far it is.
#[derive(Clone, Copy, PartialEq, Debug)]
struct PointMark {
    /// Signed degrees from the heading, positive to the right.
    delta: f32,
    distance: f64,
    arrived: bool,
}

/// A degree tick to draw.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Tick {
    /// The heading it marks, normalized to [0, 360).
    degree: u32,
    /// Its centre offset from the bar's centre, in GUI pixels.
    offset: f32,
    /// Long (30 degrees), short (15) or smallest (5).
    kind: u32,
}

/// The degree ticks visible at a heading, nearest first: one every
/// `TICK_EVERY` degrees whose offset lands in the bar. The visible
/// span is walked rather than the whole circle, so the count is
/// the bar's, not the compass's.
fn visible_ticks(head: f32, half: f32, px: f32) -> Vec<Tick> {
    let span = half / px;
    let first = ((head - span) / TICK_EVERY as f32).floor() as i32;
    let last = ((head + span) / TICK_EVERY as f32).ceil() as i32;
    let mut ticks = Vec::new();
    for step in first..=last {
        let degree = (step * TICK_EVERY as i32).rem_euclid(360) as u32;
        let offset = angle_delta(head, degree as f32) * px;
        if !visible(offset, half) {
            continue;
        }
        let kind = if degree % NUMBER_EVERY == 0 {
            2
        } else if degree % MID_EVERY == 0 {
            1
        } else {
            0
        };
        ticks.push(Tick { degree, offset, kind });
    }
    ticks.sort_by(|a, b| a.offset.abs().partial_cmp(&b.offset.abs()).unwrap());
    ticks
}

/// A compass letter to draw: its text and centre offset.
#[derive(Clone, PartialEq, Debug)]
struct Letter {
    text: &'static str,
    offset: f32,
}

/// The compass letters visible at a heading, nearest first.
fn visible_letters(head: f32, half: f32, px: f32) -> Vec<Letter> {
    let mut letters: Vec<Letter> = POINTS
        .iter()
        .map(|(label, degree, _)| Letter {
            text: label,
            offset: angle_delta(head, *degree as f32) * px,
        })
        .filter(|l| visible(l.offset, half))
        .collect();
    letters.sort_by(|a, b| a.offset.abs().partial_cmp(&b.offset.abs()).unwrap());
    letters
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

/// A stored index into a cycle of `len`; 0 when unreadable.
fn read_index(key: &str, len: usize) -> usize {
    read_index_default(key, len, 0)
}

/// A stored index into a cycle of `len`; `default` when
/// unreadable.
fn read_index_default(key: &str, len: usize, default: usize) -> usize {
    let Some(bytes) = storage::get(key) else { return default };
    let Ok(text) = std::str::from_utf8(&bytes) else { return default };
    match text.trim().parse::<usize>() {
        Ok(v) if v < len => v,
        _ => default,
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn enabled_label(enabled: bool) -> String {
    format!("Compass Bar: {}", on_off(enabled))
}

fn background_label(background: bool) -> String {
    format!("Background: {}", on_off(background))
}

fn scale_label(index: usize) -> String {
    format!("Size: {}%", (SCALES[index] * 100.0) as u32)
}

fn color_label(index: usize) -> String {
    format!("Letter Color: {}", COLOR_NAMES[index])
}

fn arrival_label(index: usize) -> String {
    format!("Arrival Distance: {}", ARRIVALS[index])
}

fn xtime_label(index: usize) -> String {
    format!("X Time: {}s", X_TIMES[index])
}

fn kept_label(index: usize) -> String {
    format!("Death Points Kept: {}", KEPTS[index])
}

struct CompassBar {
    enabled: bool,
    background: bool,
    scale_index: usize,
    color_index: usize,
    arrival_index: usize,
    xtime_index: usize,
    kept_index: usize,
    /// The death points, oldest first; session-only, as the
    /// original's are.
    points: Vec<DeathPoint>,
    /// The context (world + dimension) the last-death seed
    /// has already run for, so entering a world seeds once.
    seeded_for: Option<String>,
    /// The signature of the shapes on screen — the yaw quantum
    /// (tenths of a degree), the size, the player's block and
    /// the marks — so an unchanged frame costs no call; `None`
    /// until the first draw, `Some` while showing.
    drawn: Option<String>,
}

impl CompassBar {
    /// Adds a death point, honouring how many are kept: the
    /// oldest goes first. A seed (from the game's record) is
    /// skipped when the point is already listed.
    fn add_point(&mut self, x: i32, y: i32, z: i32, dimension: &str, seed: bool) {
        if seed
            && self
                .points
                .iter()
                .any(|p| p.x == x && p.y == y && p.z == z && p.dimension == dimension)
        {
            return;
        }
        let kept = KEPTS[self.kept_index];
        while self.points.len() >= kept {
            self.points.remove(0);
        }
        self.points.push(DeathPoint {
            x,
            y,
            z,
            dimension: dimension.to_string(),
            arrived_tick: None,
        });
        self.drawn = None;
    }

    /// The arrival bookkeeping: a point in range starts its
    /// countdown; out of range the X lifts; a point whose X
    /// time has run in range goes.
    fn update_arrivals(&mut self, tick: u64) {
        let Some(player) = game::get_player() else {
            return;
        };
        let arrival = f64::from(ARRIVALS[self.arrival_index]);
        let limit = u64::from(X_TIMES[self.xtime_index]) * 20;
        for p in &mut self.points {
            if p.dimension != player.dimension {
                continue;
            }
            let d = point_distance((player.x, player.y, player.z), p.x, p.y, p.z);
            if d <= arrival {
                if p.arrived_tick.is_none() {
                    p.arrived_tick = Some(tick);
                    self.drawn = None;
                }
            } else if p.arrived_tick.is_some() {
                p.arrived_tick = None;
                self.drawn = None;
            }
        }
        let before = self.points.len();
        self.points.retain(|p| match p.arrived_tick {
            Some(t0) => tick.saturating_sub(t0) < limit,
            None => true,
        });
        if self.points.len() != before {
            self.drawn = None;
        }
    }

    /// The dismiss key: the point nearest the way you are
    /// looking, within 30 degrees, goes.
    fn dismiss_nearest(&mut self) {
        let Some(player) = game::get_player() else {
            return;
        };
        let head = heading(player.yaw);
        let mut best: Option<(usize, f32)> = None;
        for (i, p) in self.points.iter().enumerate() {
            if p.dimension != player.dimension {
                continue;
            }
            let d = angle_delta(head, bearing(player.x, player.z, p.x, p.z)).abs();
            if d <= 30.0 && best.map(|(_, b)| d < b).unwrap_or(true) {
                best = Some((i, d));
            }
        }
        if let Some((i, _)) = best {
            self.points.remove(i);
            self.drawn = None;
        }
    }

    /// Draws the bar for the player's facing, or clears it off the
    /// title screen; a no-op when nothing changed.
    fn refresh(&mut self) {
        if !self.enabled {
            return;
        }
        let Some(player) = game::get_player() else {
            if self.drawn.is_some() {
                render::set_hud(&[]);
                self.drawn = None;
            }
            return;
        };
        let head = heading(player.yaw);
        let marks: Vec<PointMark> = self
            .points
            .iter()
            .filter(|p| p.dimension == player.dimension)
            .map(|p| PointMark {
                delta: angle_delta(head, bearing(player.x, player.z, p.x, p.z)),
                distance: point_distance((player.x, player.y, player.z), p.x, p.y, p.z),
                arrived: p.arrived_tick.is_some(),
            })
            .collect();
        let mut sig = format!(
            "{}|{}|{}|{}|{}",
            (head * 10.0).round() as i32,
            self.scale_index,
            player.x.floor() as i32,
            player.y.floor() as i32,
            player.z.floor() as i32
        );
        for m in &marks {
            sig.push_str(&format!(
                ";{:.0},{}",
                m.distance,
                m.arrived as u8
            ));
        }
        if self.drawn.as_deref() == Some(sig.as_str()) {
            return;
        }
        let (w, _h) = render::gui_size();
        let scale = SCALES[self.scale_index];
        let shapes = bar_shapes(
            head,
            w,
            scale,
            self.background,
            LETTER_COLORS[self.color_index],
            &marks,
        );
        render::set_hud(&shapes);
        self.drawn = Some(sig);
    }

    /// Takes the bar off the screen at once (it was switched off);
    /// the next tick redraws it if it comes back on.
    fn hide_now(&mut self) {
        if self.drawn.is_some() {
            render::set_hud(&[]);
            self.drawn = None;
        }
    }

    /// The settings page in Options > Mods: one button per setting,
    /// cycling its label — the original's four bar settings and
    /// its three death-point settings.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: ENABLED_BUTTON,
                label: enabled_label(self.enabled),
            }),
            Widget::Button(ButtonWidget {
                id: BACKGROUND_BUTTON,
                label: background_label(self.background),
            }),
            Widget::Button(ButtonWidget {
                id: SCALE_BUTTON,
                label: scale_label(self.scale_index),
            }),
            Widget::Button(ButtonWidget {
                id: COLOR_BUTTON,
                label: color_label(self.color_index),
            }),
            Widget::Button(ButtonWidget {
                id: ARRIVAL_BUTTON,
                label: arrival_label(self.arrival_index),
            }),
            Widget::Button(ButtonWidget {
                id: XTIME_BUTTON,
                label: xtime_label(self.xtime_index),
            }),
            Widget::Button(ButtonWidget {
                id: KEPT_BUTTON,
                label: kept_label(self.kept_index),
            }),
        ]);
    }
}

/// The bar's shapes for one facing, in draw order: the background,
/// the ticks, the letters, the degree numbers, the centre marker.
/// Drawn with `render.set-hud` in GUI pixels from the top left.
/// `measure` is the SDK's text measure, injected so the geometry
/// is testable on the host.
fn bar_shapes_measured(
    head: f32,
    width: f32,
    scale: f32,
    background: bool,
    letter_color: u32,
    marks: &[PointMark],
    measure: &dyn Fn(&str) -> f32,
) -> Vec<Shape> {
    let px = px_per_degree(width, scale);
    let half = bar_width(width, scale) / 2.0;
    let centre = width / 2.0;
    let mut shapes = Vec::new();
    if background {
        shapes.push(Shape::Fill(FillShape {
            rect: Rect {
                x: centre - half,
                y: BAR_TOP,
                w: half * 2.0,
                h: BAR_HEIGHT,
            },
            color: BACKGROUND_COLOR,
        }));
    }
    for tick in visible_ticks(head, half, px) {
        let (len, thick) = match tick.kind {
            2 => (5.0, 1.0),
            1 => (3.5, 1.0),
            _ => (2.0, 0.5),
        };
        shapes.push(Shape::Fill(FillShape {
            rect: Rect {
                x: centre + tick.offset - thick / 2.0,
                y: BAR_TOP,
                w: thick,
                h: len,
            },
            color: MARK_COLOR,
        }));
        if tick.kind == 2 {
            let label = tick.degree.to_string();
            shapes.push(Shape::Text(TextShape {
                at: Point {
                    x: centre + tick.offset,
                    y: BAR_TOP + DEGREE_BASE,
                },
                color: DEGREE_COLOR,
                text: label,
                centered: true,
                shadow: true,
            }));
        }
    }
    for letter in visible_letters(head, half, px) {
        shapes.push(Shape::Text(TextShape {
            at: Point {
                x: centre + letter.offset,
                y: BAR_TOP + LETTER_LIFT,
            },
            color: letter_color,
            text: letter.text.to_string(),
            centered: true,
            shadow: true,
        }));
    }
    for mark in marks {
        let offset = mark.delta * px;
        let dist = format!("{:.0}", mark.distance);
        if visible(offset, half) {
            // On the bar: a red mark at its bearing — an X
            // once arrived — with the distance under it.
            if mark.arrived {
                shapes.push(Shape::Text(TextShape {
                    at: Point {
                        x: centre + offset,
                        y: BAR_TOP + 6.0,
                    },
                    color: DEATH_TEXT_COLOR,
                    text: "X".to_string(),
                    centered: true,
                    shadow: true,
                }));
            } else {
                shapes.push(Shape::Fill(FillShape {
                    rect: Rect {
                        x: centre + offset - 2.0,
                        y: BAR_TOP + 7.0,
                        w: 4.0,
                        h: 4.0,
                    },
                    color: DEATH_COLOR,
                }));
            }
            shapes.push(Shape::Text(TextShape {
                at: Point {
                    x: centre + offset,
                    y: BAR_TOP + 14.0,
                },
                color: DEGREE_COLOR,
                text: dist,
                centered: true,
                shadow: true,
            }));
        } else {
            // Out of view: an arrow at the bar's edge, pointing
            // the way to turn, with the distance under it.
            let edge = if mark.delta > 0.0 {
                half - 6.0
            } else {
                -(half - 6.0)
            };
            shapes.push(Shape::Text(TextShape {
                at: Point {
                    x: centre + edge,
                    y: BAR_TOP + 6.0,
                },
                color: DEATH_TEXT_COLOR,
                text: if mark.delta > 0.0 {
                    ">".to_string()
                } else {
                    "<".to_string()
                },
                centered: true,
                shadow: true,
            }));
            shapes.push(Shape::Text(TextShape {
                at: Point {
                    x: centre + edge,
                    y: BAR_TOP + 14.0,
                },
                color: DEGREE_COLOR,
                text: dist,
                centered: true,
                shadow: true,
            }));
        }
    }
    let _ = measure; // texts centre on their marks; no measure needed
    shapes.push(Shape::Fill(FillShape {
        rect: Rect {
            x: centre - 0.5,
            y: BAR_TOP,
            w: 1.0,
            h: MARKER_HEIGHT,
        },
        color: MARK_COLOR,
    }));
    shapes
}

/// The bar's shapes, measured with the SDK's own text measure.
fn bar_shapes(
    head: f32,
    width: f32,
    scale: f32,
    background: bool,
    letter_color: u32,
    marks: &[PointMark],
) -> Vec<Shape> {
    bar_shapes_measured(head, width, scale, background, letter_color, marks, &|s| {
        render::text_width(s)
    })
}

impl Mod for CompassBar {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(DISMISS_KEY, "Dismiss Death Point", "key.keyboard.x")?;
        let state = CompassBar {
            enabled: read_bool(ENABLED_KEY, true),
            background: read_bool(BACKGROUND_KEY, true),
            scale_index: read_index(SCALE_KEY, SCALES.len()),
            color_index: read_index(COLOR_KEY, LETTER_COLORS.len()),
            arrival_index: read_index_default(ARRIVAL_KEY, ARRIVALS.len(), ARRIVAL_DEFAULT),
            xtime_index: read_index_default(XTIME_KEY, X_TIMES.len(), X_TIME_DEFAULT),
            kept_index: read_index_default(KEPT_KEY, KEPTS.len(), KEPT_DEFAULT),
            points: Vec::new(),
            seeded_for: None,
            drawn: None,
        };
        Ok((state, Registration::new().on_tick()))
    }

    fn tick(&mut self, tick: u64) {
        // Entering a world seeds the one point the game itself
        // remembers; deaths while playing append theirs.
        if let Some(ctx) = map::context() {
            let key = format!("{}\u{1}{}", ctx.world, ctx.dimension);
            if self.seeded_for.as_deref() != Some(key.as_str()) {
                self.seeded_for = Some(key);
                if let Some(d) = map::last_death()
                    && d.dimension == ctx.dimension
                {
                    self.add_point(d.x, d.y, d.z, &d.dimension, true);
                }
            }
        }
        for event in map::poll_events() {
            if let horizon_mod_sdk::PositionEvent::Died(pos) = event {
                self.add_point(pos.x, pos.y, pos.z, &pos.dimension, false);
            }
        }
        for event in input::poll() {
            if let Event::KeyDown(k) = event
                && k == DISMISS_KEY
            {
                self.dismiss_nearest();
            }
        }
        self.update_arrivals(tick);
        self.refresh();
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            ENABLED_BUTTON => {
                self.enabled = !self.enabled;
                let _ = storage::set(
                    ENABLED_KEY,
                    if self.enabled { b"true" } else { b"false" },
                );
                if !self.enabled {
                    self.hide_now();
                }
            }
            BACKGROUND_BUTTON => {
                self.background = !self.background;
                let _ = storage::set(
                    BACKGROUND_KEY,
                    if self.background { b"true" } else { b"false" },
                );
                self.drawn = None;
            }
            SCALE_BUTTON => {
                self.scale_index = (self.scale_index + 1) % SCALES.len();
                let _ = storage::set(SCALE_KEY, self.scale_index.to_string().as_bytes());
                self.drawn = None;
            }
            COLOR_BUTTON => {
                self.color_index = (self.color_index + 1) % LETTER_COLORS.len();
                let _ = storage::set(COLOR_KEY, self.color_index.to_string().as_bytes());
                self.drawn = None;
            }
            ARRIVAL_BUTTON => {
                self.arrival_index = (self.arrival_index + 1) % ARRIVALS.len();
                let _ = storage::set(ARRIVAL_KEY, self.arrival_index.to_string().as_bytes());
                self.drawn = None;
            }
            XTIME_BUTTON => {
                self.xtime_index = (self.xtime_index + 1) % X_TIMES.len();
                let _ = storage::set(XTIME_KEY, self.xtime_index.to_string().as_bytes());
                self.drawn = None;
            }
            KEPT_BUTTON => {
                self.kept_index = (self.kept_index + 1) % KEPTS.len();
                let _ = storage::set(KEPT_KEY, self.kept_index.to_string().as_bytes());
                let kept = KEPTS[self.kept_index];
                while self.points.len() > kept {
                    self.points.remove(0);
                }
                self.drawn = None;
            }
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(CompassBar);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_25() {
        assert_eq!(API_LEVEL, 25);
    }

    #[test]
    fn heading_reads_the_compass() {
        assert_eq!(heading(0.0), 180.0); // yaw south reads S
        assert_eq!(heading(180.0), 0.0); // yaw north reads N
        assert_eq!(heading(90.0), 270.0); // yaw west reads W
        assert_eq!(heading(270.0), 90.0); // yaw east reads E
        assert_eq!(heading(-90.0), 90.0); // negative yaw wraps
        assert_eq!(heading(540.0), 0.0); // so does a spun yaw
        assert!((heading(179.0) - 359.0).abs() < 1e-4);
    }

    #[test]
    fn deltas_take_the_short_way_round() {
        assert_eq!(angle_delta(0.0, 90.0), 90.0);
        assert_eq!(angle_delta(0.0, 270.0), -90.0);
        assert_eq!(angle_delta(350.0, 10.0), 20.0);
        assert_eq!(angle_delta(10.0, 350.0), -20.0);
        assert_eq!(angle_delta(0.0, 180.0), 180.0);
        assert_eq!(angle_delta(180.0, 180.0), 0.0);
    }

    #[test]
    fn sizes_scale_the_strip() {
        // 854 GUI px wide at 100%: half the screen, 120 degrees.
        assert!((px_per_degree(854.0, 1.0) - 427.0 / 120.0).abs() < 1e-4);
        assert!((bar_width(854.0, 1.0) - 427.0).abs() < 1e-4);
        assert!((bar_width(854.0, 0.75) - 320.25).abs() < 1e-4);
        assert!((bar_width(854.0, 1.5) - 640.5).abs() < 1e-4);
    }

    #[test]
    fn the_faced_point_sits_at_the_centre() {
        // Facing north: N is centred, NE and NW sit inside the
        // ±60-degree view, and E, W and S are nowhere in the bar.
        let px = px_per_degree(854.0, 1.0);
        let half = bar_width(854.0, 1.0) / 2.0;
        let letters = visible_letters(0.0, half, px);
        let at = |label: &str| letters.iter().find(|l| l.text == label).map(|l| l.offset);
        assert_eq!(at("N"), Some(0.0));
        assert!((at("NE").unwrap() - 45.0 * px).abs() < 1e-3);
        assert!((at("NW").unwrap() + 45.0 * px).abs() < 1e-3);
        assert_eq!(at("E"), None);
        assert_eq!(at("W"), None);
        assert_eq!(at("S"), None);
        // Facing east: E centres, SE is to the right, NE to the
        // left.
        let letters = visible_letters(90.0, half, px);
        let at = |label: &str| letters.iter().find(|l| l.text == label).map(|l| l.offset);
        assert_eq!(at("E"), Some(0.0));
        assert!(at("SE").unwrap() > 0.0);
        assert!(at("NE").unwrap() < 0.0);
        // Across the wrap: facing 350, N sits 10 degrees right.
        let letters = visible_letters(350.0, half, px);
        let n = letters.iter().find(|l| l.text == "N").unwrap();
        assert!((n.offset - 10.0 * px).abs() < 1e-3);
    }

    #[test]
    fn ticks_land_on_their_degrees() {
        let px = px_per_degree(854.0, 1.0);
        let half = bar_width(854.0, 1.0) / 2.0;
        let ticks = visible_ticks(0.0, half, px);
        // Every 5 degrees across the ±60-degree view: -60..=60
        // step 5, wrapped — 25 ticks, the faced 0 dead centre,
        // numbered every 30.
        assert_eq!(ticks.len(), 25);
        let centre = ticks.iter().find(|t| t.offset.abs() < 1e-4).unwrap();
        assert_eq!((centre.degree, centre.kind), (0, 2));
        let thirty = ticks.iter().find(|t| t.degree == 30).unwrap();
        assert!((thirty.offset - 30.0 * px).abs() < 1e-3);
        assert_eq!(thirty.kind, 2);
        let fifteen = ticks.iter().find(|t| t.degree == 15).unwrap();
        assert_eq!(fifteen.kind, 1);
        let five = ticks.iter().find(|t| t.degree == 5).unwrap();
        assert_eq!(five.kind, 0);
        // Offsets grow with the turn, both ways.
        let west = ticks.iter().find(|t| t.degree == 330).unwrap();
        assert!((west.offset + 30.0 * px).abs() < 1e-3);
        // Sorted nearest first.
        assert!(ticks.windows(2).all(|w| w[0].offset.abs() <= w[1].offset.abs()));
    }

    #[test]
    fn shapes_fill_the_bar() {
        let shapes = bar_shapes_measured(0.0, 854.0, 1.0, true, 0xFFFF_FFFF, &[], &|s| {
            s.len() as f32 * 6.0
        });
        // Background first, centre marker last.
        assert!(matches!(shapes.first(), Some(Shape::Fill(_))));
        assert!(matches!(shapes.last(), Some(Shape::Fill(_))));
        let texts: Vec<&str> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        for want in ["N", "NE", "NW", "0", "30", "330"] {
            assert!(texts.contains(&want), "missing {want}: {texts:?}");
        }
        // Without the background there is no backing fill: the
        // first shape is the faced tick.
        let bare = bar_shapes_measured(0.0, 854.0, 1.0, false, 0xFFFF_FFFF, &[], &|s| {
            s.len() as f32 * 6.0
        });
        assert_eq!(bare.len(), shapes.len() - 1);
    }

    #[test]
    fn settings_labels_name_the_originals_choices() {
        assert_eq!(enabled_label(true), "Compass Bar: On");
        assert_eq!(background_label(false), "Background: Off");
        assert_eq!(scale_label(0), "Size: 75%");
        assert_eq!(scale_label(1), "Size: 100%");
        assert_eq!(scale_label(3), "Size: 150%");
        assert_eq!(color_label(0), "Letter Color: White");
        // The death-point settings and the port's defaults.
        assert_eq!(arrival_label(ARRIVAL_DEFAULT), "Arrival Distance: 5");
        assert_eq!(xtime_label(X_TIME_DEFAULT), "X Time: 10s");
        assert_eq!(kept_label(KEPT_DEFAULT), "Death Points Kept: 3");
    }

    #[test]
    fn bearings_read_the_compass() {
        // From (0.5, 0.5): north is −z, east is +x.
        assert!((bearing(0.5, 0.5, 0, -10) - 0.0).abs() < 1e-4);
        assert!((bearing(0.5, 0.5, 10, 0) - 90.0).abs() < 1e-4);
        assert!((bearing(0.5, 0.5, 0, 10) - 180.0).abs() < 1e-4);
        assert!((bearing(0.5, 0.5, -10, 0) - 270.0).abs() < 1e-4);
        // A point dead ahead while facing it centres: heading
        // east (90) to a point east reads delta 0.
        let head = 90.0;
        let b = bearing(0.5, 0.5, 10, 0);
        assert!((angle_delta(head, b)).abs() < 1e-4);
        // Distances are 3D, to the block's centre.
        assert!((point_distance((0.5, 64.0, 0.5), 0, 64, -10) - 10.0).abs() < 1e-9);
        assert!((point_distance((0.5, 64.0, 0.5), 3, 64, -4) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn death_marks_draw_on_and_off_the_bar() {
        let texts_of = |shapes: &[Shape]| -> Vec<String> {
            shapes
                .iter()
                .filter_map(|s| match s {
                    Shape::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect()
        };
        // Ahead: a red fill and the plain-integer distance.
        let ahead = [PointMark {
            delta: 0.0,
            distance: 40.4,
            arrived: false,
        }];
        let shapes = bar_shapes_measured(0.0, 854.0, 1.0, true, 0xFFFF_FFFF, &ahead, &|s| {
            s.len() as f32 * 6.0
        });
        assert!(texts_of(&shapes).contains(&"40".to_string()));
        assert!(shapes.iter().any(|s| matches!(s, Shape::Fill(f) if f.color == DEATH_COLOR)));
        // Arrived: the mark is an X.
        let arrived = [PointMark {
            delta: 10.0,
            distance: 3.0,
            arrived: true,
        }];
        let shapes = bar_shapes_measured(0.0, 854.0, 1.0, true, 0xFFFF_FFFF, &arrived, &|s| {
            s.len() as f32 * 6.0
        });
        assert!(texts_of(&shapes).contains(&"X".to_string()));
        assert!(!shapes.iter().any(|s| matches!(s, Shape::Fill(f) if f.color == DEATH_COLOR)));
        // Behind, to the left: a "<" at the left edge; to the
        // right-behind: a ">".
        let behind_left = [PointMark {
            delta: -170.0,
            distance: 12.0,
            arrived: false,
        }];
        let shapes = bar_shapes_measured(
            0.0,
            854.0,
            1.0,
            true,
            0xFFFF_FFFF,
            &behind_left,
            &|s| s.len() as f32 * 6.0,
        );
        assert!(texts_of(&shapes).contains(&"<".to_string()));
        assert!(texts_of(&shapes).contains(&"12".to_string()));
        let behind_right = [PointMark {
            delta: 170.0,
            distance: 12.0,
            arrived: false,
        }];
        let shapes = bar_shapes_measured(
            0.0,
            854.0,
            1.0,
            true,
            0xFFFF_FFFF,
            &behind_right,
            &|s| s.len() as f32 * 6.0,
        );
        assert!(texts_of(&shapes).contains(&">".to_string()));
    }
}

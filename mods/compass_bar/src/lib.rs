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
//! original says, no commands. What is NOT ported is the death
//! points (their markers, distances, edge arrows, arrival countdown
//! and dismiss key, and the death-point settings): the client SDK
//! exposes no death event, so there is nothing to hang a death on.
//! They arrive with the SDK's position-events work; nothing here
//! stands in for them.
//!
//! The bar itself is drawn with the SDK's HUD shapes
//! (`render.set-hud`) around the top centre of the screen
//! (`render.gui-size`), from `game.get-player`'s yaw. One shape
//! list is kept per yaw quantum and size: the game replaces shapes
//! wholesale, so an unchanged list costs no call.

use horizon_mod_sdk::{
    Mod, Registration, game,
    render::{self, FillShape, Point, Rect, Shape, TextShape},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `game.get-player` is level 0, as are `render.set-hud`,
/// `render.gui-size` and `render.text-width`
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 0;

/// The storage keys for the four settings.
const ENABLED_KEY: &str = "enabled";
const BACKGROUND_KEY: &str = "background";
const SCALE_KEY: &str = "scale";
const COLOR_KEY: &str = "color";

/// The settings page's buttons, one per setting.
const ENABLED_BUTTON: u32 = 0;
const BACKGROUND_BUTTON: u32 = 1;
const SCALE_BUTTON: u32 = 2;
const COLOR_BUTTON: u32 = 3;

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
    let Some(bytes) = storage::get(key) else { return 0 };
    let Ok(text) = std::str::from_utf8(&bytes) else { return 0 };
    match text.trim().parse::<usize>() {
        Ok(v) if v < len => v,
        _ => 0,
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

struct CompassBar {
    enabled: bool,
    background: bool,
    scale_index: usize,
    color_index: usize,
    /// The yaw quantum and size the shapes on screen were built for
    /// (the yaw in tenths of a degree), so an unchanged frame costs
    /// no call; `None` until the first draw, `Some` while showing.
    drawn: Option<(i32, usize)>,
}

impl CompassBar {
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
        let quantum = (heading(player.yaw) * 10.0).round() as i32;
        if self.drawn == Some((quantum, self.scale_index)) {
            return;
        }
        let (w, _h) = render::gui_size();
        let scale = SCALES[self.scale_index];
        let shapes = bar_shapes(
            heading(player.yaw),
            w,
            scale,
            self.background,
            LETTER_COLORS[self.color_index],
        );
        render::set_hud(&shapes);
        self.drawn = Some((quantum, self.scale_index));
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
    /// cycling its label — the original's four ported settings.
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
) -> Vec<Shape> {
    bar_shapes_measured(head, width, scale, background, letter_color, &|s| {
        render::text_width(s)
    })
}

impl Mod for CompassBar {
    fn init() -> Result<(Self, Registration), String> {
        let state = CompassBar {
            enabled: read_bool(ENABLED_KEY, true),
            background: read_bool(BACKGROUND_KEY, true),
            scale_index: read_index(SCALE_KEY, SCALES.len()),
            color_index: read_index(COLOR_KEY, LETTER_COLORS.len()),
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
    fn api_level_is_0() {
        assert_eq!(API_LEVEL, 0);
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
        let shapes = bar_shapes_measured(0.0, 854.0, 1.0, true, 0xFFFF_FFFF, &|s| {
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
        let bare = bar_shapes_measured(0.0, 854.0, 1.0, false, 0xFFFF_FFFF, &|s| {
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
    }
}

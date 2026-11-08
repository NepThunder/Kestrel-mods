//! Crosshair Indicator for Kestrel: your crosshair tells you when the mob
//! or player you aim at is within attack reach (3 blocks). Four styles —
//! corner brackets around the crosshair (default), a centre dot, diamond
//! pips, a chevron above it — in five static colours: classic white
//! (default), combat red, pink, diamond cyan, gold. `/crosshair` turns it
//! on or off, as does the key on the mod's page in Options > Mods (unbound
//! by default) and the switch there; the page also cycles the style and
//! the theme, and the game keeps the choices.
//!
//! Behaviour reimplemented from the public description of Crosshair
//! Indicator by CayMac (CayMac97), MIT, a port of the Java Crosshair
//! Indicator mod by AmerDz01: this file shares no code with either
//! original. The indicator is drawn with the SDK's HUD shapes
//! (`render.set-hud`) around the screen's centre (`render.gui-size`), so
//! it sits on the crosshair itself instead of in a corner panel. The
//! target comes from `entity.targeted(3.0)`, which works in singleplayer
//! and multiplayer and is hidden by a wall between you and the mob, as
//! the original's raycast is. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/crosshair-indicator>.
//!
//! What the SDK cannot do, the original's native build can: the thin
//! weapon-cooldown bar below the crosshair and the dynamic theme's charge
//! colours (green when the weapon is fully charged, orange while it
//! charges) both need the weapon's attack strength, which the SDK does
//! not expose, so this port has neither — the themes here are static
//! colours only.
use horizon_mod_sdk::{
    Mod, Registration, entity,
    input::{self, Event},
    render::{self, FillShape, Point, QuadShape, Rect, Shape},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `entity.targeted` is level 10; `render.set-hud` and `render.gui-size`
/// are level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 10;

/// Attack reach, in blocks: the indicator shows only while the target is
/// this close or closer.
const REACH: f32 = 3.0;

/// The storage keys for the three settings.
const ENABLED_KEY: &str = "enabled";
const STYLE_KEY: &str = "style";
const THEME_KEY: &str = "theme";

/// The settings page's buttons: on/off, style, theme.
const ENABLED_BUTTON: u32 = 0;
const STYLE_BUTTON: u32 = 1;
const THEME_BUTTON: u32 = 2;

/// The toggle key's name for `register-key`, unbound by default.
const TOGGLE_KEY: &str = "toggle";

/// How far from the screen's centre the brackets' inner edges sit, in GUI
/// pixels; how long each bracket arm is; how thick the drawn lines are.
const GAP: f32 = 7.0;
const ARM: f32 = 6.0;
const THICK: f32 = 1.0;

/// How far from the centre the diamond pips sit, and their half-size.
const PIP_DISTANCE: f32 = 11.0;
const PIP_HALF: f32 = 2.5;

/// The chevron's apex above the centre, its tips, and its stroke width.
const CHEVRON_APEX: f32 = 14.0;
const CHEVRON_TIP: f32 = 7.0;
const CHEVRON_HALF_SPREAD: f32 = 7.0;
const CHEVRON_WIDTH: f32 = 2.0;

/// The indicator's style.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Style {
    Brackets,
    Dot,
    Diamonds,
    Chevron,
}

/// The indicator's colour theme (static colours only — see the module docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Theme {
    Classic,
    Combat,
    Pink,
    Diamond,
    Gold,
}

impl Style {
    /// The style's name, as the settings button shows it.
    fn name(self) -> &'static str {
        match self {
            Style::Brackets => "Brackets",
            Style::Dot => "Dot",
            Style::Diamonds => "Diamonds",
            Style::Chevron => "Chevron",
        }
    }
}

impl Theme {
    /// The theme's name, as the settings button shows it.
    fn name(self) -> &'static str {
        match self {
            Theme::Classic => "Classic",
            Theme::Combat => "Combat",
            Theme::Pink => "Pink",
            Theme::Diamond => "Diamond",
            Theme::Gold => "Gold",
        }
    }

    /// The theme's colour, as the HUD shapes' 0xAARRGGBB.
    fn color(self) -> u32 {
        match self {
            Theme::Classic => 0xFFFFFFFF,
            Theme::Combat => 0xFFFF5555,
            Theme::Pink => 0xFFFF69B4,
            Theme::Diamond => 0xFF4AEDD9,
            Theme::Gold => 0xFFFFAA00,
        }
    }
}

const STYLES: [Style; 4] = [Style::Brackets, Style::Dot, Style::Diamonds, Style::Chevron];

const THEMES: [Theme; 5] = [
    Theme::Classic,
    Theme::Combat,
    Theme::Pink,
    Theme::Diamond,
    Theme::Gold,
];

/// A filled rectangle, as one HUD shape.
fn fill(x: f32, y: f32, w: f32, h: f32, color: u32) -> Shape {
    Shape::Fill(FillShape {
        rect: Rect { x, y, w, h },
        color,
    })
}

/// A four-cornered shape from its corners, in order round it.
fn quad(a: Point, b: Point, c: Point, d: Point, color: u32) -> Shape {
    Shape::Quad(QuadShape {
        corners: (a, b, c, d),
        color,
    })
}

fn point(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// A stroke from `a` to `b`, `width` thick: a quad whose long sides run
/// along the stroke and whose short sides are perpendicular to it.
fn stroke(a: Point, b: Point, width: f32, color: u32) -> Shape {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    // A zero-length stroke draws nothing useful; fall back to a dot.
    if len == 0.0 {
        return fill(a.x - width / 2.0, a.y - width / 2.0, width, width, color);
    }
    let (nx, ny) = (-dy / len * width, dx / len * width);
    quad(
        a,
        b,
        point(b.x + nx, b.y + ny),
        point(a.x + nx, a.y + ny),
        color,
    )
}

/// The corner brackets around (`cx`, `cy`): eight thin rectangles, two
/// per corner, their inner edges `GAP` from the centre.
fn brackets(cx: f32, cy: f32, color: u32) -> Vec<Shape> {
    vec![
        // Top left.
        fill(cx - GAP - ARM, cy - GAP - ARM, ARM, THICK, color),
        fill(cx - GAP - ARM, cy - GAP - ARM, THICK, ARM, color),
        // Top right.
        fill(cx + GAP, cy - GAP - ARM, ARM, THICK, color),
        fill(cx + GAP + ARM - THICK, cy - GAP - ARM, THICK, ARM, color),
        // Bottom left.
        fill(cx - GAP - ARM, cy + GAP + ARM - THICK, ARM, THICK, color),
        fill(cx - GAP - ARM, cy + GAP, THICK, ARM, color),
        // Bottom right.
        fill(cx + GAP, cy + GAP + ARM - THICK, ARM, THICK, color),
        fill(cx + GAP + ARM - THICK, cy + GAP, THICK, ARM, color),
    ]
}

/// The centre dot: one 2x2 square on (`cx`, `cy`).
fn dot(cx: f32, cy: f32, color: u32) -> Vec<Shape> {
    vec![fill(cx - 1.0, cy - 1.0, 2.0, 2.0, color)]
}

/// The diamond pips: one small diamond each side of the centre.
fn diamonds(cx: f32, cy: f32, color: u32) -> Vec<Shape> {
    let pip = |px: f32, py: f32| {
        quad(
            point(px, py - PIP_HALF),
            point(px + PIP_HALF, py),
            point(px, py + PIP_HALF),
            point(px - PIP_HALF, py),
            color,
        )
    };
    vec![
        pip(cx, cy - PIP_DISTANCE),
        pip(cx + PIP_DISTANCE, cy),
        pip(cx, cy + PIP_DISTANCE),
        pip(cx - PIP_DISTANCE, cy),
    ]
}

/// The chevron above the centre: two strokes meeting at an apex.
fn chevron(cx: f32, cy: f32, color: u32) -> Vec<Shape> {
    let apex = point(cx, cy - CHEVRON_APEX);
    vec![
        stroke(
            apex,
            point(cx - CHEVRON_HALF_SPREAD, cy - CHEVRON_TIP),
            CHEVRON_WIDTH,
            color,
        ),
        stroke(
            apex,
            point(cx + CHEVRON_HALF_SPREAD, cy - CHEVRON_TIP),
            CHEVRON_WIDTH,
            color,
        ),
    ]
}

/// The indicator's shapes for a style and theme at the screen's centre.
fn shapes(style: Style, theme: Theme, cx: f32, cy: f32) -> Vec<Shape> {
    let color = theme.color();
    match style {
        Style::Brackets => brackets(cx, cy, color),
        Style::Dot => dot(cx, cy, color),
        Style::Diamonds => diamonds(cx, cy, color),
        Style::Chevron => chevron(cx, cy, color),
    }
}

/// Whether the crosshair target is within attack reach: `targeted` only
/// answers inside the reach it is asked for, so any answer counts.
fn in_reach() -> bool {
    entity::targeted(REACH).is_some()
}

fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

fn read_style(stored: Option<Vec<u8>>) -> Style {
    match stored.and_then(|v| String::from_utf8(v).ok()).as_deref() {
        Some("dot") => Style::Dot,
        Some("diamonds") => Style::Diamonds,
        Some("chevron") => Style::Chevron,
        _ => Style::Brackets,
    }
}

fn read_theme(stored: Option<Vec<u8>>) -> Theme {
    match stored.and_then(|v| String::from_utf8(v).ok()).as_deref() {
        Some("combat") => Theme::Combat,
        Some("pink") => Theme::Pink,
        Some("diamond") => Theme::Diamond,
        Some("gold") => Theme::Gold,
        _ => Theme::Classic,
    }
}

fn style_value(style: Style) -> &'static str {
    match style {
        Style::Brackets => "brackets",
        Style::Dot => "dot",
        Style::Diamonds => "diamonds",
        Style::Chevron => "chevron",
    }
}

fn theme_value(theme: Theme) -> &'static str {
    match theme {
        Theme::Classic => "classic",
        Theme::Combat => "combat",
        Theme::Pink => "pink",
        Theme::Diamond => "diamond",
        Theme::Gold => "gold",
    }
}

fn enabled_label(enabled: bool) -> String {
    format!("Enabled: {}", if enabled { "ON" } else { "OFF" })
}

fn style_label(style: Style) -> String {
    format!("Style: {}", style.name())
}

fn theme_label(theme: Theme) -> String {
    format!("Theme: {}", theme.name())
}

/// The next style in the switch's cycle, wrapping back to the first.
fn next_style(style: Style) -> Style {
    let i = STYLES.iter().position(|s| *s == style).unwrap_or(0);
    STYLES[(i + 1) % STYLES.len()]
}

/// The next theme in the switch's cycle, wrapping back to the first.
fn next_theme(theme: Theme) -> Theme {
    let i = THEMES.iter().position(|t| *t == theme).unwrap_or(0);
    THEMES[(i + 1) % THEMES.len()]
}

struct CrosshairIndicator {
    enabled: bool,
    style: Style,
    theme: Theme,
    /// Whether the shapes are on screen now, so an unchanged tick costs
    /// no call.
    showing: bool,
}

impl CrosshairIndicator {
    /// Draws the indicator when a target is within reach, clears it
    /// otherwise; a no-op when nothing changed.
    fn refresh(&mut self) {
        let show = self.enabled && in_reach();
        if show == self.showing {
            return;
        }
        self.showing = show;
        if show {
            let (w, h) = render::gui_size();
            render::set_hud(&shapes(self.style, self.theme, w / 2.0, h / 2.0));
        } else {
            render::set_hud(&[]);
        }
    }

    /// Hides the shapes at once (the indicator is off); the next tick
    /// redraws them if a target is still in reach.
    fn hide_now(&mut self) {
        if self.showing {
            render::set_hud(&[]);
            self.showing = false;
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        let _ = storage::set(ENABLED_KEY, if enabled { b"true" } else { b"false" });
        if !enabled {
            self.hide_now();
        }
        self.refresh_settings();
    }

    fn cycle_style(&mut self) {
        self.style = next_style(self.style);
        let _ = storage::set(STYLE_KEY, style_value(self.style).as_bytes());
        self.refresh_settings();
    }

    fn cycle_theme(&mut self) {
        self.theme = next_theme(self.theme);
        let _ = storage::set(THEME_KEY, theme_value(self.theme).as_bytes());
        self.refresh_settings();
    }

    /// The settings page: one button per setting, cycling its label.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: ENABLED_BUTTON,
                label: enabled_label(self.enabled),
            }),
            Widget::Button(ButtonWidget {
                id: STYLE_BUTTON,
                label: style_label(self.style),
            }),
            Widget::Button(ButtonWidget {
                id: THEME_BUTTON,
                label: theme_label(self.theme),
            }),
        ]);
    }
}

impl Mod for CrosshairIndicator {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(
            TOGGLE_KEY,
            "Toggle Crosshair Indicator",
            "key.keyboard.unknown",
        )?;
        let hooks = Registration::new()
            .command("crosshair")
            .on_tick()
            .on_frame();
        let state = CrosshairIndicator {
            enabled: read_bool(storage::get(ENABLED_KEY), true),
            style: read_style(storage::get(STYLE_KEY)),
            theme: read_theme(storage::get(THEME_KEY)),
            showing: false,
        };
        Ok((state, hooks))
    }

    fn tick(&mut self, _tick: u64) {
        self.refresh();
    }

    fn frame(&mut self, _seconds: f32) {
        for event in input::poll() {
            if let Event::KeyDown(k) = event
                && k == TOGGLE_KEY
            {
                self.set_enabled(!self.enabled);
            }
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "crosshair" {
            return Err("This command does nothing".into());
        }
        self.set_enabled(!self.enabled);
        let state = if self.enabled { "shown" } else { "hidden" };
        Ok(Some(format!("Crosshair Indicator {state}")))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            ENABLED_BUTTON => self.set_enabled(!self.enabled),
            STYLE_BUTTON => self.cycle_style(),
            THEME_BUTTON => self.cycle_theme(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(CrosshairIndicator);

#[cfg(test)]
mod tests {
    use super::*;

    fn fill_rect(shape: &Shape) -> Rect {
        let Shape::Fill(f) = shape else {
            panic!("not a fill: {shape:?}");
        };
        f.rect
    }

    fn quad_corners(shape: &Shape) -> (Point, Point, Point, Point) {
        let Shape::Quad(q) = shape else {
            panic!("not a quad: {shape:?}");
        };
        q.corners
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn api_level_is_ten() {
        // Newest imported function is `entity.targeted` (level 10);
        // `render.set-hud` and `render.gui-size` are level 0.
        assert_eq!(API_LEVEL, 10);
    }

    #[test]
    fn brackets_are_eight_thin_rects_around_the_centre() {
        // Screen centre (213.5, 120): the vanilla crosshair's arms end
        // about 5px out, and the brackets sit just past them.
        let made = brackets(213.5, 120.0, 0xFFFFFFFF);
        assert_eq!(made.len(), 8);
        // Top-left horizontal arm: 6 long, 1 thick, inner edge 7 out.
        let r = fill_rect(&made[0]);
        assert!(close(r.x, 213.5 - 13.0) && close(r.y, 120.0 - 13.0));
        assert!(close(r.w, 6.0) && close(r.h, 1.0));
        // Top-left vertical arm: same corner, running down.
        let r = fill_rect(&made[1]);
        assert!(close(r.x, 213.5 - 13.0) && close(r.y, 120.0 - 13.0));
        assert!(close(r.w, 1.0) && close(r.h, 6.0));
        // Top-right horizontal arm ends 13 out on the other side.
        let r = fill_rect(&made[2]);
        assert!(close(r.x, 213.5 + 7.0) && close(r.x + r.w, 213.5 + 13.0));
        // Bottom-right vertical arm ends 13 below the centre.
        let r = fill_rect(&made[7]);
        assert!(close(r.x + r.w, 213.5 + 13.0) && close(r.y + r.h, 120.0 + 13.0));
        // Every bracket stays outside the crosshair's own arms.
        for s in &made {
            let r = fill_rect(s);
            assert!(
                r.x + r.w <= 213.5 - GAP
                    || r.x >= 213.5 + GAP
                    || r.y + r.h <= 120.0 - GAP
                    || r.y >= 120.0 + GAP,
                "bracket overlaps the crosshair: {r:?}"
            );
        }
    }

    #[test]
    fn dot_is_one_two_pixel_square_on_the_centre() {
        let made = dot(100.0, 50.0, 0xFFFFFFFF);
        assert_eq!(made.len(), 1);
        let r = fill_rect(&made[0]);
        assert!(close(r.x, 99.0) && close(r.y, 49.0));
        assert!(close(r.w, 2.0) && close(r.h, 2.0));
    }

    #[test]
    fn diamonds_are_four_pips_round_the_centre() {
        let made = diamonds(100.0, 50.0, 0xFFFFFFFF);
        assert_eq!(made.len(), 4);
        // North pip: centred 11 above, 2.5 half-size each way.
        let (a, b, c, d) = quad_corners(&made[0]);
        assert!(close(a.x, 100.0) && close(a.y, 50.0 - 13.5));
        assert!(close(b.x, 102.5) && close(b.y, 50.0 - 11.0));
        assert!(close(c.x, 100.0) && close(c.y, 50.0 - 8.5));
        assert!(close(d.x, 97.5) && close(d.y, 50.0 - 11.0));
        // East pip: the same diamond, 11 to the right.
        let (a, _, c, _) = quad_corners(&made[1]);
        assert!(close(a.x, 111.0) && close(a.y, 50.0 - 2.5));
        assert!(close(c.x, 111.0) && close(c.y, 50.0 + 2.5));
    }

    #[test]
    fn chevron_is_two_strokes_meeting_above_the_centre() {
        let made = chevron(100.0, 50.0, 0xFFFFFFFF);
        assert_eq!(made.len(), 2);
        // Both strokes start at the apex, 14 above the centre.
        for s in &made {
            let (a, _, _, _) = quad_corners(s);
            assert!(close(a.x, 100.0) && close(a.y, 36.0));
        }
        // Left stroke ends 7 left and 7 down from the apex; right mirrors it.
        let (_, b, _, _) = quad_corners(&made[0]);
        assert!(close(b.x, 93.0) && close(b.y, 43.0));
        let (_, b, _, _) = quad_corners(&made[1]);
        assert!(close(b.x, 107.0) && close(b.y, 43.0));
        // Each stroke is 2 thick: the quad's short side is ~2 long.
        for s in &made {
            let (_, b, c, _) = quad_corners(s);
            let w = ((c.x - b.x).powi(2) + (c.y - b.y).powi(2)).sqrt();
            assert!(close(w, 2.0), "stroke width {w}");
        }
    }

    #[test]
    fn shapes_pick_the_style_in_the_theme_colour() {
        let made = shapes(Style::Brackets, Theme::Combat, 100.0, 50.0);
        assert_eq!(made.len(), 8);
        let Shape::Fill(f) = &made[0] else { panic!() };
        assert_eq!(f.color, 0xFFFF5555);
        assert_eq!(shapes(Style::Dot, Theme::Diamond, 100.0, 50.0).len(), 1);
        assert_eq!(shapes(Style::Diamonds, Theme::Gold, 100.0, 50.0).len(), 4);
        assert_eq!(shapes(Style::Chevron, Theme::Pink, 100.0, 50.0).len(), 2);
        assert_eq!(Theme::Classic.color(), 0xFFFFFFFF);
        assert_eq!(Theme::Pink.color(), 0xFFFF69B4);
        assert_eq!(Theme::Diamond.color(), 0xFF4AEDD9);
        assert_eq!(Theme::Gold.color(), 0xFFFFAA00);
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert!(read_bool(Some(b"true".to_vec()), false));
        assert_eq!(read_style(None), Style::Brackets);
        assert_eq!(read_style(Some(b"dot".to_vec())), Style::Dot);
        assert_eq!(read_style(Some(b"diamonds".to_vec())), Style::Diamonds);
        assert_eq!(read_style(Some(b"chevron".to_vec())), Style::Chevron);
        // A stored style the switch never offers falls back to Brackets.
        assert_eq!(read_style(Some(b"cross".to_vec())), Style::Brackets);
        assert_eq!(read_theme(None), Theme::Classic);
        assert_eq!(read_theme(Some(b"combat".to_vec())), Theme::Combat);
        assert_eq!(read_theme(Some(b"pink".to_vec())), Theme::Pink);
        assert_eq!(read_theme(Some(b"diamond".to_vec())), Theme::Diamond);
        assert_eq!(read_theme(Some(b"gold".to_vec())), Theme::Gold);
        assert_eq!(read_theme(Some(b"dynamic".to_vec())), Theme::Classic);
        assert_eq!(style_value(Style::Diamonds), "diamonds");
        assert_eq!(theme_value(Theme::Gold), "gold");
        assert_eq!(enabled_label(false), "Enabled: OFF");
        assert_eq!(style_label(Style::Chevron), "Style: Chevron");
        assert_eq!(theme_label(Theme::Pink), "Theme: Pink");
        // Both cycles step through every choice and wrap back.
        assert_eq!(next_style(Style::Brackets), Style::Dot);
        assert_eq!(next_style(Style::Diamonds), Style::Chevron);
        assert_eq!(next_style(Style::Chevron), Style::Brackets);
        assert_eq!(next_theme(Theme::Classic), Theme::Combat);
        assert_eq!(next_theme(Theme::Diamond), Theme::Gold);
        assert_eq!(next_theme(Theme::Gold), Theme::Classic);
    }
}

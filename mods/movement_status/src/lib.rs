//! Movement Status for Kestrel: the live movement state in the
//! bottom-left corner — `Sprint [Held]` / `Sprint [Toggled]`,
//! `Sneak [Held]` / `Sneak [Toggled]`, `Flying [Creative]` /
//! `Flying [Elytra]` — several lines at once when several apply.
//!
//! Behaviour reimplemented from Movement Status by UniDex101
//! (MIT, <https://github.com/UniDex101/movement-status>); this
//! file shares no code with the original. The state is API 18's
//! self read (`game.get-self-state`, capability `game-read`):
//! the movement flags and the game's own Toggle Sprint /
//! Toggle Crouch options, which are what tell a held state from
//! a toggled one. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/movement_status>.
//!
//! Fidelity notes (the README has the full accounting): the
//! labels, the corner and the one setting (the background box)
//! are the original's. One label is coarser than the original's:
//! the original reads the physical key, so with a toggle option
//! on it can still say [Held] while the key is down; the SDK
//! exposes the option and the state, not the physical key, so
//! here the label follows the option. The original's own notes
//! carry the mirror-image caveat (custom keybinds could show
//! the wrong label); this port's labels follow the bound
//! options, never a hardcoded key.

use horizon_mod_sdk::{
    Mod, Registration,
    game::{self, SelfState},
    render::{self, Corner, FillShape, Point, Rect, Shape, TextShape},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `game.get-self-state` is level 18
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 18;

/// The storage key and settings button for the one setting.
const BACKGROUND_KEY: &str = "background";
const BACKGROUND_BUTTON: u32 = 0;

/// The box's drawing, in GUI pixels: a line pitch of 11, a
/// 3-pixel pad around the text, and the semi-transparent black
/// of the original's background box.
const LINE_PITCH: f32 = 11.0;
const PAD: f32 = 3.0;
const BACKGROUND_COLOR: u32 = 0x6600_0000;
const TEXT_COLOR: u32 = 0xFF_FFFF;

/// The status lines for one self-state, in the original's
/// order — sprint, sneak, flying — one per state that applies.
/// A state the game has latched through its Toggle Sprint /
/// Toggle Crouch option reads [Toggled]; one without the
/// option is the key being held, [Held]. Gliding is its own
/// kind of flying: `Flying [Elytra]`, not [Creative].
fn status_lines(state: &SelfState) -> Vec<&'static str> {
    let mut lines = Vec::new();
    if state.sprinting {
        lines.push(if state.sprint_toggled {
            "Sprint [Toggled]"
        } else {
            "Sprint [Held]"
        });
    }
    if state.sneaking {
        lines.push(if state.sneak_toggled {
            "Sneak [Toggled]"
        } else {
            "Sneak [Held]"
        });
    }
    if state.gliding {
        lines.push("Flying [Elytra]");
    } else if state.flying {
        lines.push("Flying [Creative]");
    }
    lines
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

fn background_label(background: bool) -> String {
    format!("Background: {}", on_off(background))
}

struct MovementStatus {
    background: bool,
    /// The lines and background state the corner box on screen
    /// was built from, so an unchanged tick costs no call; `None`
    /// while nothing is drawn.
    drawn: Option<(Vec<&'static str>, bool)>,
}

impl MovementStatus {
    /// The corner box's shapes for these lines, measured with
    /// the SDK's own text measure: the background box first
    /// (when the setting is on), then the lines top to bottom.
    fn shapes(&self, lines: &[&'static str], background: bool) -> Vec<Shape> {
        let width = lines
            .iter()
            .map(|line| render::text_width(line))
            .fold(0.0f32, f32::max)
            + PAD * 2.0;
        let height = lines.len() as f32 * LINE_PITCH + PAD;
        let mut shapes = Vec::new();
        if background {
            shapes.push(Shape::Fill(FillShape {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: width,
                    h: height,
                },
                color: BACKGROUND_COLOR,
            }));
        }
        for (i, line) in lines.iter().enumerate() {
            shapes.push(Shape::Text(TextShape {
                text: line.to_string(),
                at: Point {
                    x: PAD,
                    y: PAD + i as f32 * LINE_PITCH,
                },
                color: TEXT_COLOR,
                centered: false,
                shadow: true,
            }));
        }
        shapes
    }

    fn refresh(&mut self) {
        let lines = game::get_self_state().map(|state| status_lines(&state));
        let key = lines.map(|lines| (lines, self.background));
        if key == self.drawn {
            return;
        }
        match &key {
            Some((lines, _)) if !lines.is_empty() => {
                let shapes = self.shapes(lines, self.background);
                let width = lines
                    .iter()
                    .map(|line| render::text_width(line))
                    .fold(0.0f32, f32::max)
                    + PAD * 2.0;
                let height = lines.len() as f32 * LINE_PITCH + PAD;
                render::set_corner(Corner::BottomLeft, width, height, &shapes);
            }
            _ => render::clear_corner(),
        }
        self.drawn = key;
    }
}

impl Mod for MovementStatus {
    fn init() -> Result<(Self, Registration), String> {
        let state = MovementStatus {
            background: read_bool(BACKGROUND_KEY, true),
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
        if id == BACKGROUND_BUTTON {
            self.background = !self.background;
            let _ = storage::set(
                BACKGROUND_KEY,
                if self.background { b"true" } else { b"false" },
            );
            // The next tick rebuilds the box without (or with)
            // its background.
            self.drawn = None;
            self.refresh_settings();
        }
    }
}

impl MovementStatus {
    /// The settings page in Options > Mods: the original's
    /// single toggle, the background box.
    fn refresh_settings(&self) {
        ui::set_settings(&[Widget::Button(ButtonWidget {
            id: BACKGROUND_BUTTON,
            label: background_label(self.background),
        })]);
    }
}

horizon_mod_sdk::export_mod!(MovementStatus);

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> SelfState {
        SelfState {
            sprinting: false,
            sneaking: false,
            flying: false,
            gliding: false,
            mount: None,
            sprint_toggled: false,
            sneak_toggled: false,
        }
    }

    #[test]
    fn api_level_is_18() {
        assert_eq!(API_LEVEL, 18);
    }

    #[test]
    fn standing_shows_nothing() {
        assert!(status_lines(&state()).is_empty());
    }

    #[test]
    fn held_and_toggled_read_off_the_options() {
        let mut s = state();
        s.sprinting = true;
        assert_eq!(status_lines(&s), ["Sprint [Held]"]);
        s.sprint_toggled = true;
        assert_eq!(status_lines(&s), ["Sprint [Toggled]"]);
        let mut s = state();
        s.sneaking = true;
        assert_eq!(status_lines(&s), ["Sneak [Held]"]);
        s.sneak_toggled = true;
        assert_eq!(status_lines(&s), ["Sneak [Toggled]"]);
        // An option on its own, with no state, shows nothing.
        let mut s = state();
        s.sprint_toggled = true;
        s.sneak_toggled = true;
        assert!(status_lines(&s).is_empty());
    }

    #[test]
    fn flying_names_its_kind() {
        let mut s = state();
        s.flying = true;
        assert_eq!(status_lines(&s), ["Flying [Creative]"]);
        let mut s = state();
        s.gliding = true;
        assert_eq!(status_lines(&s), ["Flying [Elytra]"]);
    }

    #[test]
    fn states_stack_in_the_originals_order() {
        let mut s = state();
        s.sprinting = true;
        s.sneaking = true;
        s.sneak_toggled = true;
        s.gliding = true;
        assert_eq!(
            status_lines(&s),
            ["Sprint [Held]", "Sneak [Toggled]", "Flying [Elytra]"]
        );
    }

    #[test]
    fn the_background_setting_labels_itself() {
        assert_eq!(background_label(true), "Background: On");
        assert_eq!(background_label(false), "Background: Off");
    }
}

//! Zoomify for Kestrel: a zoom key with smooth transitions,
//! scroll zoom, a secondary zoom and a settings page; the
//! hand's FOV goes with the view, and the zoom stands down
//! while you scope with a spyglass.
//!
//! Behaviour reimplemented from Zoomify 2.2.1+26.3-fabric by
//! isXander (LGPL-3.0-or-later,
//! <https://modrinth.com/mod/zoomify/versions>), ported to the
//! Horizon registry by DeportedToHell; this file shares no code
//! with the original — the original is a Fabric mod, this is a
//! fresh implementation against the Kestrel SDK. The original's
//! defaults and behaviour are kept wherever the SDK can express
//! them: C zooms (hold or toggle), the primary zoom starts at
//! 4x, scroll multiplies it in and out with no distance limit
//! of its own (the game caps any mod zoom at 250x), F6 is the
//! secondary zoom (50x by default) with a slow cinematic
//! transition that hides the HUD while it lasts, the hand's
//! FOV follows the view's, sensitivity follows the zoom
//! ("relative sensitivity") and the transitions ease
//! cinematically. The
//! original's own settings screen is a YACL page; here the
//! settings live on the mod's page in Options > Mods, the only
//! settings surface a mod has in this SDK — the same options,
//! the same defaults. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/zoomify>.
//!
//! Two of the original's behaviours stay gaps, named in the
//! README rather than papered over: cycling the secondary
//! zoom's level with the mouse buttons (this SDK gives mods no
//! mouse-button events, so the level lives on the settings
//! page), and the spyglass rules beyond standing down — a mod
//! cannot scope for you on a zoom press, combine its zoom with
//! the scope's view, or offer the original's spyglass-behaviour
//! options; while you scope, this zoom stands down and the
//! game's own spyglass view wins, as `set-zoom` documents.
use horizon_mod_sdk::{
    Mod, Registration,
    input::{self, Event},
    render::{self, HandFov, HandFovMode, Zoom as ZoomView},
    storage,
    ui::{self, ButtonWidget, ListWidget, Widget},
};

/// The zoom key's and the cycling keys' names for `register-key`,
/// with the original's defaults: C zooms (and the original
/// defaults it to hold), F6 is the secondary zoom, F7 the hand
/// FOV toggle.
const ZOOM_KEY: &str = "zoom";
const DEFAULT_ZOOM_KEY: &str = "key.keyboard.c";
const SECONDARY_KEY: &str = "secondary_zoom";
const DEFAULT_SECONDARY_KEY: &str = "key.keyboard.f6";
const HAND_FOV_KEY: &str = "hand_fov";
const DEFAULT_HAND_FOV_KEY: &str = "key.keyboard.f7";

/// The zoom levels the mouse buttons cycle the secondary zoom
/// through, as the original.
const SECONDARY_LEVELS: [f32; 3] = [10.0, 50.0, 200.0];
/// The primary's starting zoom, and the secondary's default, as
/// the original.
const DEFAULT_PRIMARY: f32 = 4.0;
const DEFAULT_SECONDARY: f32 = 50.0;

/// The most the game lets any mod zoom divide the view by
/// (`render.set-zoom`'s cap). Scroll zoom has no limit of its
/// own in the original; it stops here, as the game's does.
const MAX_ZOOM: f32 = 250.0;

/// The hand's own field of view in vanilla, the base the hand
/// FOV modes adjust (the original's setting is the mode alone
/// over this base).
const VANILLA_HAND_FOV: f32 = 70.0;

/// How fast the eased transitions approach their target, per
/// second — the turn-time of an exponential ease: a bigger
/// number snaps sooner. The primary's default is the original's
/// 10%; the secondary's slow cinematic transition uses a quarter
/// of it, chosen here (the original exposes no number for it).
const TRANSITION_RATE: f32 = 6.0;
const SECONDARY_RATE: f32 = 1.5;

/// The settings page's buttons, in the original's option order.
const BEHAVIOUR_BUTTON: u32 = 0; // hold / toggle
const PRIMARY_BUTTON: u32 = 1; // initial zoom list
const HAND_TOGGLE_BUTTON: u32 = 2; // hand FOV follows
const HAND_LIST_BUTTON: u32 = 3; // hand FOV behaviour list
const RELATIVE_BUTTON: u32 = 4; // relative sensitivity
const CINEMATIC_BUTTON: u32 = 5; // cinematic camera
const SECONDARY_LIST_BUTTON: u32 = 6; // secondary initial zoom

/// The primary's starting zooms the settings page cycles
/// through (the original's list contents, read off its labels).
const PRIMARY_LEVELS: [f32; 3] = [2.0, 4.0, 8.0];
/// The hand FOV behaviours the settings page cycles through.
const HAND_BEHAVIOURS: [&str; 3] = ["multiply", "add", "keep"];

/// The storage keys: the original's option names.
const BEHAVIOUR_KEY: &str = "zoom_key_behaviour";
const PRIMARY_KEY: &str = "initial_zoom";
const SECONDARY_INITIAL_KEY: &str = "secondary_initial_zoom";
const HAND_TOGGLE_KEY: &str = "hand_fov";
const HAND_BEHAVIOUR_KEY: &str = "hand_fov_behaviour";
const RELATIVE_KEY: &str = "relative_sensitivity";
const CINEMATIC_KEY: &str = "cinematic_camera";

/// One easing step of `current` toward `target` over `dt`
/// seconds at `rate`: an exponential approach, so the factor
/// moves briskly at first and settles — the spyglass feel.
/// `dt` of 0 changes nothing; the step never overshoots.
fn approach(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return current;
    }
    let k = (-rate * dt).exp();
    target + (current - target) * k
}

/// The turning scale for a zoom factor: relative sensitivity
/// keeps the angle per mouse count steady, so the view turns
/// slower in step with the zoom; without it, full speed.
fn turn_scale(factor: f32, relative: bool) -> f32 {
    if relative && factor > 1.0 {
        1.0 / factor
    } else {
        1.0
    }
}

/// Scroll zoom: a step multiplies the target in or out, with
/// the original's lack of a distance limit — the game's zoom
/// cap is the only stop, and the factor never goes below 1.
fn scrolled(target: f32, steps: f32) -> f32 {
    (target * 1.25f32.powf(steps)).clamp(1.0, MAX_ZOOM)
}

/// The hand FOV the settings ask for: none while the hand FOV
/// setting is off (vanilla's fixed hand), else the chosen
/// behaviour over the vanilla base.
fn hand_fov_setting(settings: &Settings) -> Option<HandFov> {
    if !settings.hand {
        return None;
    }
    let mode = match settings.hand_behaviour {
        1 => HandFovMode::Add,
        2 => HandFovMode::Keep,
        _ => HandFovMode::Multiply,
    };
    Some(HandFov {
        mode,
        base: VANILLA_HAND_FOV,
    })
}

/// The stored float under `key`, or `default` when missing or
/// not a number — the original reads its values the same way.
fn stored_f32(stored: Option<Vec<u8>>, default: f32) -> f32 {
    stored
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

/// A stored flag: off only when its bytes say "false".
fn stored_flag(stored: Option<Vec<u8>>) -> bool {
    stored.is_none_or(|v| v != b"false")
}

/// The next value in a cycle after `current` (an unknown current
/// counts as the first entry).
fn next_in<T: PartialEq + Copy>(cycle: &[T], current: T) -> T {
    let at = cycle.iter().position(|v| *v == current).unwrap_or(0);
    cycle[(at + 1) % cycle.len()]
}

/// Which zoom is driving the view.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Active {
    None,
    Primary,
    Secondary,
}

/// The settings, with the original's defaults.
#[derive(Clone, Copy, PartialEq)]
struct Settings {
    /// Hold (default) or toggle.
    toggle: bool,
    /// The primary's starting zoom.
    primary: f32,
    /// The secondary's starting zoom (the level it returns to
    /// after the mouse-button cycling steps away from it).
    secondary: f32,
    /// The hand FOV follows the view FOV (default on).
    hand: bool,
    /// How the hand FOV follows: "multiply" (default), "add" or
    /// "keep".
    hand_behaviour: usize,
    /// Sensitivity follows the zoom (default on).
    relative: bool,
    /// The camera eases cinematically while zoomed (default on).
    cinematic: bool,
}

impl Settings {
    fn read() -> Settings {
        Settings {
            toggle: storage::get(BEHAVIOUR_KEY).is_some_and(|v| v == b"toggle"),
            primary: stored_f32(storage::get(PRIMARY_KEY), DEFAULT_PRIMARY),
            secondary: stored_f32(storage::get(SECONDARY_INITIAL_KEY), DEFAULT_SECONDARY),
            hand: stored_flag(storage::get(HAND_TOGGLE_KEY)),
            hand_behaviour: stored_f32(storage::get(HAND_BEHAVIOUR_KEY), 0.0) as usize % 3,
            relative: stored_flag(storage::get(RELATIVE_KEY)),
            cinematic: stored_flag(storage::get(CINEMATIC_KEY)),
        }
    }

    fn save(&self) {
        let _ = storage::set(
            BEHAVIOUR_KEY,
            if self.toggle { b"toggle" } else { b"hold" },
        );
        let _ = storage::set(PRIMARY_KEY, self.primary.to_string().as_bytes());
        let _ = storage::set(SECONDARY_INITIAL_KEY, self.secondary.to_string().as_bytes());
        let _ = storage::set(HAND_TOGGLE_KEY, flag_text(self.hand));
        let _ = storage::set(
            HAND_BEHAVIOUR_KEY,
            (self.hand_behaviour as f32).to_string().as_bytes(),
        );
        let _ = storage::set(RELATIVE_KEY, flag_text(self.relative));
        let _ = storage::set(CINEMATIC_KEY, flag_text(self.cinematic));
    }

    /// The settings page's buttons, in the original's option order.
    fn labels(&self) -> [String; 7] {
        [
            format!(
                "Zoom key behaviour: {}",
                if self.toggle { "Toggle" } else { "Hold" }
            ),
            format!("Initial zoom: {}x", trim(self.primary)),
            format!("Hand FOV: {}", on_off(self.hand)),
            format!(
                "Hand FOV behaviour: {}",
                HAND_BEHAVIOURS[self.hand_behaviour]
            ),
            format!("Relative sensitivity: {}", on_off(self.relative)),
            format!("Cinematic camera: {}", on_off(self.cinematic)),
            format!("Secondary initial zoom: {}x", trim(self.secondary)),
        ]
    }

    /// A click on one of the page's buttons.
    fn click(&mut self, id: u32) {
        match id {
            BEHAVIOUR_BUTTON => self.toggle = !self.toggle,
            PRIMARY_BUTTON => self.primary = next_in(&PRIMARY_LEVELS, self.primary),
            HAND_TOGGLE_BUTTON => self.hand = !self.hand,
            HAND_LIST_BUTTON => self.hand_behaviour = (self.hand_behaviour + 1) % 3,
            RELATIVE_BUTTON => self.relative = !self.relative,
            CINEMATIC_BUTTON => self.cinematic = !self.cinematic,
            SECONDARY_LIST_BUTTON => {
                self.secondary = next_in(&SECONDARY_LEVELS, self.secondary)
            }
            _ => {}
        }
    }
}

fn flag_text(on: bool) -> &'static [u8] {
    if on { b"true" } else { b"false" }
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

/// A zoom factor without its trailing ".0": 4, not 4.0.
fn trim(v: f32) -> String {
    if v == v.trunc() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

struct Zoomify {
    settings: Settings,
    active: Active,
    /// The factor the view is at now (eased toward the target).
    factor: f32,
    /// The factor the active zoom is heading for (scroll moves
    /// it; the spyglass clamps it).
    target: f32,
    /// The secondary's current level: cycling the mouse buttons
    /// steps it, and engaging the secondary starts from its
    /// initial zoom when cycled away from — the level persists
    /// between engagements until cycled.
    secondary_level: f32,
    /// The factor the secondary was engaged at, when it took
    /// over from a running primary: releasing returns there.
    resume: Option<f32>,
    /// The zoom last told to the game, as a plain tuple (factor,
    /// smooth, turn), so `set-zoom` is only called when something
    /// changed.
    shown: Option<(f32, bool, f32)>,
    /// Whether the HUD is currently hidden for the secondary
    /// zoom, so `set-hide-hud` is only called on a change.
    hud_hidden: bool,
}

impl Zoomify {
    /// Engage a zoom: the view eases from the factor it is at to
    /// the zoom's starting factor.
    fn engage(&mut self, which: Active) {
        match which {
            Active::Primary => {
                self.active = Active::Primary;
                self.target = self.settings.primary.max(1.0);
            }
            Active::Secondary => {
                // Taking over from a running primary: remember
                // where to return to on release.
                self.resume = if self.active == Active::Primary {
                    Some(self.target)
                } else {
                    None
                };
                self.active = Active::Secondary;
                self.target = self.secondary_level.max(1.0);
            }
            Active::None => {}
        }
    }

    /// Release the active zoom. The secondary falls back to the
    /// primary it took over from, if there was one; anything else
    /// ends the zoom.
    fn release(&mut self) {
        match self.active {
            Active::Secondary => {
                if let Some(back) = self.resume.take() {
                    self.active = Active::Primary;
                    self.target = back;
                } else {
                    self.active = Active::None;
                    self.target = 1.0;
                }
            }
            Active::Primary => {
                self.active = Active::None;
                self.target = 1.0;
            }
            Active::None => {}
        }
    }

    /// The settings page: the original's options as buttons and
    /// lists (a `list` shows the cycle the button walks through).
    fn refresh_settings(&self) {
        let labels = self.settings.labels();
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: BEHAVIOUR_BUTTON,
                label: labels[0].clone(),
            }),
            Widget::List(ListWidget {
                id: PRIMARY_BUTTON,
                rows: vec![
                    "Initial zoom:".into(),
                    "2x".into(),
                    "4x".into(),
                    "8x".into(),
                ],
                selected: Some(1 + PRIMARY_LEVELS.iter().position(|v| *v == self.settings.primary).unwrap_or(1) as u32),
            }),
            Widget::Button(ButtonWidget {
                id: HAND_TOGGLE_BUTTON,
                label: labels[2].clone(),
            }),
            Widget::List(ListWidget {
                id: HAND_LIST_BUTTON,
                rows: vec![
                    "Hand FOV behaviour:".into(),
                    "multiply".into(),
                    "add".into(),
                    "keep".into(),
                ],
                selected: Some(1 + self.settings.hand_behaviour as u32),
            }),
            Widget::Button(ButtonWidget {
                id: RELATIVE_BUTTON,
                label: labels[4].clone(),
            }),
            Widget::Button(ButtonWidget {
                id: CINEMATIC_BUTTON,
                label: labels[5].clone(),
            }),
            Widget::List(ListWidget {
                id: SECONDARY_LIST_BUTTON,
                rows: vec![
                    "Secondary initial zoom:".into(),
                    "10x".into(),
                    "50x".into(),
                    "200x".into(),
                ],
                selected: Some(1 + SECONDARY_LEVELS.iter().position(|v| *v == self.settings.secondary).unwrap_or(1) as u32),
            }),
        ]);
    }

    /// Push the hand FOV the settings ask for to the game.
    fn apply_hand_fov(&self) {
        render::set_hand_fov(hand_fov_setting(&self.settings));
    }

    /// One frame: poll the keys and wheel, ease the factor, and
    /// tell the game the zoom in effect when it changed.
    fn step(&mut self, seconds: f32) {
        // The spyglass owns the view while the player scopes:
        // this zoom stands down there — a press does not engage
        // and the wheel does not turn — and the scope's view
        // shows instead, as `can-zoom` reports. Engaging is
        // gated on `can-zoom` itself, so a press with a screen
        // open starts nothing either.
        let can_zoom = render::can_zoom();
        let scoping = render::get_spyglass_zoom().is_some();
        let mut wheeled = false;
        for event in input::poll() {
            match event {
                Event::KeyDown(k) if k == ZOOM_KEY => {
                    if !can_zoom {
                        // Standing down: the press is not ours.
                    } else if self.settings.toggle {
                        if self.active == Active::Primary {
                            self.release();
                        } else if self.active == Active::None {
                            self.engage(Active::Primary);
                            input::claim_wheel(true);
                        }
                    } else if self.active == Active::None {
                        self.engage(Active::Primary);
                        input::claim_wheel(true);
                    }
                }
                Event::KeyUp(k) if k == ZOOM_KEY => {
                    if !self.settings.toggle && self.active == Active::Primary {
                        self.release();
                    }
                }
                Event::KeyDown(k) if k == SECONDARY_KEY => {
                    if self.active == Active::Secondary {
                        self.release();
                    } else if can_zoom {
                        self.engage(Active::Secondary);
                        input::claim_wheel(true);
                    }
                }
                Event::KeyDown(k) if k == HAND_FOV_KEY => {
                    self.settings.hand = !self.settings.hand;
                    self.settings.save();
                    self.apply_hand_fov();
                }
                Event::Wheel(steps) => {
                    if self.active != Active::None && !scoping {
                        self.target = scrolled(self.target, steps);
                        wheeled = true;
                    }
                }
                _ => {}
            }
        }
        if self.active == Active::None && !wheeled {
            input::claim_wheel(false);
        }

        let rate = match self.active {
            Active::Secondary => SECONDARY_RATE,
            _ => TRANSITION_RATE,
        };

        self.factor = approach(self.factor, self.target, rate, seconds);
        if self.active == Active::None && (self.factor - 1.0).abs() < 0.01 {
            self.factor = 1.0;
        }

        let turn = turn_scale(self.factor, self.settings.relative);
        let key = (self.factor, true, turn);
        if self.shown != Some(key) {
            render::set_zoom(ZoomView {
                factor: self.factor,
                smooth: true,
                turn,
            });
            self.shown = Some(key);
        }
        if self.factor == 1.0 && self.active == Active::None {
            self.shown = None;
        }

        // The secondary zoom hides the HUD while it is active,
        // as the original's does.
        let hide = self.active == Active::Secondary;
        if hide != self.hud_hidden {
            render::set_hide_hud(hide);
            self.hud_hidden = hide;
        }
    }
}

impl Mod for Zoomify {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(ZOOM_KEY, "Zoom", DEFAULT_ZOOM_KEY)?;
        input::register_key(SECONDARY_KEY, "Secondary Zoom", DEFAULT_SECONDARY_KEY)?;
        input::register_key(HAND_FOV_KEY, "Toggle Hand FOV", DEFAULT_HAND_FOV_KEY)?;
        let settings = Settings::read();
        render::set_hand_fov(hand_fov_setting(&settings));
        let state = Zoomify {
            settings,
            active: Active::None,
            factor: 1.0,
            target: 1.0,
            secondary_level: settings.secondary,
            resume: None,
            shown: None,
            hud_hidden: false,
        };
        Ok((state, Registration::new().on_frame()))
    }

    fn frame(&mut self, seconds: f32) {
        // A hitched frame must not jump the transition: the step
        // the maths sees is capped at a twentieth of a second.
        self.step(seconds.clamp(0.0, 0.05));
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, row: u32) {
        // A row picked on a list jumps straight to that entry
        // (row 0 is the list's header row and changes nothing);
        // the matching button ids cycle instead (below).
        let pick = row.saturating_sub(1) as usize;
        match id {
            PRIMARY_BUTTON if row > 0 => {
                self.settings.primary = PRIMARY_LEVELS[pick.min(2)];
            }
            HAND_LIST_BUTTON if row > 0 => {
                self.settings.hand_behaviour = pick.min(2);
            }
            SECONDARY_LIST_BUTTON if row > 0 => {
                self.settings.secondary = SECONDARY_LEVELS[pick.min(2)];
                self.secondary_level = self.settings.secondary;
            }
            PRIMARY_BUTTON | HAND_LIST_BUTTON | SECONDARY_LIST_BUTTON => {}
            _ => self.settings.click(id),
        }
        self.settings.save();
        self.apply_hand_fov();
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(Zoomify);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approach_eases_without_overshoot() {
        // Half a second at the primary's rate gets most of the way.
        let v = approach(1.0, 4.0, TRANSITION_RATE, 0.5);
        assert!(v > 3.5 && v < 4.0, "got {v}");
        // No step passes the target, in either direction.
        assert!(approach(1.0, 4.0, TRANSITION_RATE, 5.0) <= 4.0);
        assert!(approach(4.0, 1.0, TRANSITION_RATE, 5.0) >= 1.0);
        // Zero time changes nothing; the secondary's slow rate
        // lags the primary's, as its cinematic transition should.
        assert_eq!(approach(2.0, 9.0, TRANSITION_RATE, 0.0), 2.0);
        let slow = approach(1.0, 50.0, SECONDARY_RATE, 0.5);
        let fast = approach(1.0, 50.0, TRANSITION_RATE, 0.5);
        assert!(slow < fast);
    }

    #[test]
    fn turn_scale_follows_relative_sensitivity() {
        assert_eq!(turn_scale(4.0, true), 0.25);
        assert_eq!(turn_scale(4.0, false), 1.0);
        assert_eq!(turn_scale(1.0, true), 1.0);
        assert_eq!(turn_scale(0.5, true), 1.0);
    }

    #[test]
    fn cycles_step_and_wrap() {
        assert_eq!(next_in(&PRIMARY_LEVELS, 4.0), 8.0);
        assert_eq!(next_in(&PRIMARY_LEVELS, 8.0), 2.0);
        assert_eq!(next_in(&SECONDARY_LEVELS, 50.0), 200.0);
        assert_eq!(next_in(&SECONDARY_LEVELS, 200.0), 10.0);
        // An unknown current counts as the first entry.
        assert_eq!(next_in(&PRIMARY_LEVELS, 5.0), 4.0);
    }

    #[test]
    fn stored_values_read_with_defaults() {
        assert_eq!(stored_f32(None, 4.0), 4.0);
        assert_eq!(stored_f32(Some(b"8".to_vec()), 4.0), 8.0);
        assert_eq!(stored_f32(Some(b"junk".to_vec()), 4.0), 4.0);
        assert!(stored_flag(None));
        assert!(!stored_flag(Some(b"false".to_vec())));
        assert!(stored_flag(Some(b"true".to_vec())));
        assert_eq!(trim(4.0), "4");
        assert_eq!(trim(2.5), "2.5");
    }

    #[test]
    fn engage_and_release_follow_the_original() {
        let settings = Settings {
            toggle: false,
            primary: 4.0,
            secondary: 50.0,
            hand: true,
            hand_behaviour: 0,
            relative: true,
            cinematic: true,
        };
        let mut z = Zoomify {
            settings,
            active: Active::None,
            factor: 1.0,
            target: 1.0,
            secondary_level: settings.secondary,
            resume: None,
            shown: None,
            hud_hidden: false,
        };
        z.engage(Active::Primary);
        assert_eq!((z.active, z.target), (Active::Primary, 4.0));
        // The secondary takes over and remembers where to return.
        z.engage(Active::Secondary);
        assert_eq!((z.active, z.target), (Active::Secondary, 50.0));
        assert_eq!(z.resume, Some(4.0));
        z.release();
        assert_eq!((z.active, z.target), (Active::Primary, 4.0));
        z.release();
        assert_eq!((z.active, z.target), (Active::None, 1.0));
        // The secondary from rest ends the zoom outright.
        z.engage(Active::Secondary);
        z.release();
        assert_eq!((z.active, z.target), (Active::None, 1.0));
    }

    #[test]
    fn scroll_zoom_stops_only_at_the_games_cap() {
        assert_eq!(scrolled(4.0, 1.0), 5.0);
        assert_eq!(scrolled(4.0, -1.0), 3.2);
        // Past the old 10x cap without a stop, in both ranges.
        assert_eq!(scrolled(50.0, 1.0), 62.5);
        assert_eq!(scrolled(200.0, 1.0), 250.0);
        assert_eq!(scrolled(200.0, 10.0), 250.0);
        assert_eq!(scrolled(2.0, -10.0), 1.0);
        // Every secondary level is inside the cap now.
        assert!(SECONDARY_LEVELS.iter().all(|l| *l <= MAX_ZOOM));
        assert_eq!(MAX_ZOOM, 250.0);
    }

    #[test]
    fn hand_fov_setting_follows_the_settings() {
        let mut s = Settings {
            toggle: false,
            primary: 4.0,
            secondary: 50.0,
            hand: true,
            hand_behaviour: 0,
            relative: true,
            cinematic: true,
        };
        let hf = hand_fov_setting(&s).expect("on means a setting");
        assert_eq!(hf.mode, HandFovMode::Multiply);
        assert_eq!(hf.base, 70.0);
        s.hand_behaviour = 1;
        assert_eq!(hand_fov_setting(&s).map(|h| h.mode), Some(HandFovMode::Add));
        s.hand_behaviour = 2;
        assert_eq!(
            hand_fov_setting(&s).map(|h| h.mode),
            Some(HandFovMode::Keep)
        );
        // Off restores vanilla's fixed hand: no setting at all.
        s.hand = false;
        assert!(hand_fov_setting(&s).is_none());
    }

    #[test]
    fn settings_labels_name_the_defaults() {
        let s = Settings {
            toggle: false,
            primary: 4.0,
            secondary: 50.0,
            hand: true,
            hand_behaviour: 0,
            relative: true,
            cinematic: true,
        };
        let labels = s.labels();
        assert_eq!(labels[0], "Zoom key behaviour: Hold");
        assert_eq!(labels[1], "Initial zoom: 4x");
        assert_eq!(labels[3], "Hand FOV behaviour: multiply");
        assert_eq!(labels[6], "Secondary initial zoom: 50x");
    }
}

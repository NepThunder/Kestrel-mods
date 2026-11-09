//! Zoom for Kestrel: hold the key (Z by default) to zoom in at the
//! starting zoom; let go and the view comes back. While the key is
//! held, the mouse wheel zooms further in or out (up to 10x) and the
//! hotbar stays put. Smooth zoom eases in and out like the spyglass;
//! slower turning keeps aiming steady.
//!
//! Behaviour reimplemented from Zoom 2.0.0 by Horizon (MIT,
//! <https://github.com/horizon-rust/horizon-mods/tree/main/native/zoom>),
//! Horizon's own zoom, formerly an extra in Options > Horizon
//! Extras; this file shares no code with the original — the original
//! is a native mod, this is a fresh implementation against the
//! Kestrel SDK. Settings and defaults are the original's: the key is
//! Z, the starting zoom is 4x (the settings page cycles 2, 3, 4, 6,
//! 8, 10), smooth zoom and slower turning default on, and the wheel
//! moves in x1.25 steps between 1.25x and 10x. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/zoom>.
//!
//! The shape follows the SDK's own route for a zoom mod: a frame
//! hook polls the input events, claims the wheel while the key is
//! held, and tells the game the zoom in effect only when it changes
//! (`set-zoom` lasts until the next call, and a factor of 1 ends
//! it). One difference from the original: it starts the zoom at the
//! clamped starting level even when that is under the wheel's 1.25x
//! floor — the lowest starting level is 2x, so the clamp never
//! bites there.
use horizon_mod_sdk::{
    Mod, Registration,
    input::{self, Event},
    render::{self, Zoom as ZoomView},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The key's name for `register-key`, and its default: the original's.
const KEY: &str = "zoom";
const DEFAULT_KEY: &str = "key.keyboard.z";
/// The starting zooms the settings page cycles through, as the original.
const LEVELS: [u8; 6] = [2, 3, 4, 6, 8, 10];
/// The wheel's limits and its step, as the original.
const MIN: f32 = 1.25;
const MAX: f32 = 10.0;
const STEP: f32 = 1.25;

/// The storage keys, the original's own file names.
const LEVEL_KEY: &str = "level";
const SMOOTH_KEY: &str = "smooth";
const SENSITIVITY_KEY: &str = "sensitivity";

/// The settings page's buttons, in the original's label order.
const LEVEL_BUTTON: u32 = 0;
const SMOOTH_BUTTON: u32 = 1;
const SENSITIVITY_BUTTON: u32 = 2;

/// The settings, as the original keeps them: the starting zoom, and
/// the smooth-zoom and slower-turning switches.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Settings {
    level: u8,
    smooth: bool,
    sensitivity: bool,
}

impl Settings {
    /// The original's defaults: 4x, smooth on, slower turning on. A
    /// stored level outside the cycle still reads, clamped to
    /// 2..=10 as the original clamps it; a stored flag is off only
    /// when its file says "false".
    fn read(level: Option<Vec<u8>>, smooth: Option<Vec<u8>>, sensitivity: Option<Vec<u8>>) -> Self {
        let level = level
            .and_then(|v| String::from_utf8(v).ok())
            .and_then(|s| s.trim().parse::<u8>().ok())
            .map_or(4, |v| v.clamp(2, MAX as u8));
        let flag = |stored: Option<Vec<u8>>| stored.is_none_or(|v| v != b"false");
        Settings {
            level,
            smooth: flag(smooth),
            sensitivity: flag(sensitivity),
        }
    }

    /// The settings page's buttons, as the original labels them.
    fn labels(&self) -> [String; 3] {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        [
            format!("Starting zoom: {}x", self.level),
            format!("Smooth zoom: {}", on(self.smooth)),
            format!("Slower turning: {}", on(self.sensitivity)),
        ]
    }

    /// A click on one of the page's buttons: the level cycles
    /// through the original's list (an unknown level counts as the
    /// first), the switches flip.
    fn click(&mut self, id: u32) {
        match id {
            LEVEL_BUTTON => {
                let at = LEVELS.iter().position(|l| *l == self.level).unwrap_or(0);
                self.level = LEVELS[(at + 1) % LEVELS.len()];
            }
            SMOOTH_BUTTON => self.smooth = !self.smooth,
            SENSITIVITY_BUTTON => self.sensitivity = !self.sensitivity,
            _ => {}
        }
    }
}

/// One stored flag the way the original writes it.
fn flag_text(on: bool) -> &'static [u8] {
    if on { b"true" } else { b"false" }
}

struct Zoom {
    settings: Settings,
    /// The zoom while the key is held; None when it is up.
    held: Option<f32>,
}

impl Zoom {
    /// The zoom key went down: start at the starting zoom.
    fn press(&mut self) {
        let start = f32::from(self.settings.level);
        self.held = Some(start.clamp(MIN, MAX));
    }

    /// The key came up: no zoom.
    fn release(&mut self) {
        self.held = None;
    }

    /// Wheel steps while the key is held: true when they zoom,
    /// false when the key is up and the wheel stays the hotbar's.
    fn wheel(&mut self, steps: f32) -> bool {
        let Some(z) = &mut self.held else {
            return false;
        };
        *z = (*z * STEP.powf(steps)).clamp(MIN, MAX);
        true
    }

    /// The zoom in effect: (factor, smooth, turning scale) — the
    /// factor is 1 when the key is up, turning slows in step with
    /// the zoom when slower turning is on.
    fn view(&self) -> (f32, bool, f32) {
        let factor = self.held.unwrap_or(1.0);
        let turn = if self.settings.sensitivity {
            1.0 / factor
        } else {
            1.0
        };
        (factor, self.settings.smooth, turn)
    }

    /// Tells the game the zoom in effect (a factor of 1 ends it).
    fn show(&self) {
        let (factor, smooth, turn) = self.view();
        render::set_zoom(ZoomView {
            factor,
            smooth,
            turn,
        });
    }

    /// The settings page: the original's three buttons.
    fn refresh_settings(&self) {
        let labels = self.settings.labels();
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: LEVEL_BUTTON,
                label: labels[0].clone(),
            }),
            Widget::Button(ButtonWidget {
                id: SMOOTH_BUTTON,
                label: labels[1].clone(),
            }),
            Widget::Button(ButtonWidget {
                id: SENSITIVITY_BUTTON,
                label: labels[2].clone(),
            }),
        ]);
    }

    /// Store the settings under the original's file names.
    fn save(&self) {
        let _ = storage::set(LEVEL_KEY, self.settings.level.to_string().as_bytes());
        let _ = storage::set(SMOOTH_KEY, flag_text(self.settings.smooth));
        let _ = storage::set(SENSITIVITY_KEY, flag_text(self.settings.sensitivity));
    }
}

impl Mod for Zoom {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(KEY, "Zoom", DEFAULT_KEY)?;
        let state = Zoom {
            settings: Settings::read(
                storage::get(LEVEL_KEY),
                storage::get(SMOOTH_KEY),
                storage::get(SENSITIVITY_KEY),
            ),
            held: None,
        };
        Ok((state, Registration::new().on_frame()))
    }

    fn frame(&mut self, _seconds: f32) {
        let mut changed = false;
        for event in input::poll() {
            match event {
                Event::KeyDown(k) if k == KEY => {
                    self.press();
                    // While held, the wheel zooms instead of moving the hotbar.
                    input::claim_wheel(true);
                }
                Event::KeyUp(k) if k == KEY => {
                    self.release();
                    input::claim_wheel(false);
                }
                Event::Wheel(steps) => {
                    if !self.wheel(steps) {
                        continue;
                    }
                }
                _ => continue,
            }
            changed = true;
        }
        if changed {
            self.show();
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        self.settings.click(id);
        self.save();
        self.refresh_settings();
        // A switch flipped mid-hold changes the zoom in effect now:
        // the turning scale and smoothness follow the new settings.
        if self.held.is_some() {
            self.show();
        }
    }
}

horizon_mod_sdk::export_mod!(Zoom);

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn defaults_are_the_originals() {
        let s = Settings::read(None, None, None);
        assert_eq!(
            s,
            Settings {
                level: 4,
                smooth: true,
                sensitivity: true,
            }
        );
        let s = Settings::read(
            Some(b"8".to_vec()),
            Some(b"false".to_vec()),
            Some(b"true".to_vec()),
        );
        assert_eq!(
            s,
            Settings {
                level: 8,
                smooth: false,
                sensitivity: true,
            }
        );
        // A stored level past the wheel's top clamps to 10; junk
        // reads as the default 4.
        assert_eq!(Settings::read(Some(b"99".to_vec()), None, None).level, 10);
        assert_eq!(Settings::read(Some(b"nope".to_vec()), None, None).level, 4);
        // Trailing whitespace in the stored file is fine.
        assert_eq!(Settings::read(Some(b"8\n".to_vec()), None, None).level, 8);
    }

    #[test]
    fn holds_at_four_and_lets_go() {
        let mut zoom = Zoom {
            settings: Settings::read(None, None, None),
            held: None,
        };
        assert_eq!(zoom.view(), (1.0, true, 1.0));
        assert!(
            !zoom.wheel(1.0),
            "the wheel is the hotbar's while the key is up"
        );
        zoom.press();
        assert_eq!(zoom.view(), (4.0, true, 0.25));
        zoom.release();
        assert_eq!(zoom.view(), (1.0, true, 1.0));
        assert!(!zoom.wheel(1.0));
    }

    #[test]
    fn the_wheel_zooms_by_a_quarter_within_bounds() {
        let mut zoom = Zoom {
            settings: Settings::read(None, None, None),
            held: None,
        };
        zoom.press();
        assert!(zoom.wheel(1.0));
        assert!(close(zoom.view().0, 5.0));
        // The wheel's floor is 1.25x, not the starting level.
        assert!(zoom.wheel(-100.0));
        assert_eq!(zoom.view().0, 1.25);
        assert!(zoom.wheel(100.0));
        assert_eq!(zoom.view().0, 10.0);
        // Pressing again starts over at the starting zoom.
        zoom.release();
        zoom.press();
        assert_eq!(zoom.view().0, 4.0);
    }

    #[test]
    fn buttons_cycle_as_the_original_did() {
        let mut s = Settings {
            level: 10,
            smooth: true,
            sensitivity: false,
        };
        s.click(LEVEL_BUTTON);
        assert_eq!(s.level, 2);
        // An unknown level counts as the first when cycling.
        s.level = 5;
        s.click(LEVEL_BUTTON);
        assert_eq!(s.level, 3);
        s.click(SMOOTH_BUTTON);
        s.click(SENSITIVITY_BUTTON);
        assert_eq!(
            s.labels(),
            [
                "Starting zoom: 3x",
                "Smooth zoom: OFF",
                "Slower turning: ON",
            ]
        );
        s.click(99);
        assert_eq!(s.level, 3);
    }

    #[test]
    fn slower_turning_off_keeps_full_turn() {
        let mut zoom = Zoom {
            settings: Settings {
                level: 4,
                smooth: false,
                sensitivity: false,
            },
            held: None,
        };
        zoom.press();
        assert_eq!(zoom.view(), (4.0, false, 1.0));
    }
}

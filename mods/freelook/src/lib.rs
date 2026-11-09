//! Freelook for Kestrel: hold X to look around freely while your
//! character keeps walking and facing the way it was. Let go and the
//! camera snaps back behind the player's look.
//!
//! Behaviour reimplemented from Freelook 0.3.1 by Krzysix17 (MIT,
//! <https://github.com/Krzysix17/horizon-freelook>); this file shares no
//! code with the original — the original is a native mod, this is a fresh
//! implementation against the Kestrel SDK. The shape follows the SDK's
//! own camera probe (`crates/horizon-mod-sdk/examples/camera-probe/`):
//! while the key is held the mod calls `render.set-camera` once, with
//! `position: none` and the player's own yaw and pitch, and the host
//! routes the mouse to the camera instead of the player — so the player
//! keeps walking, aiming and mining in the direction they faced, and the
//! server's picture of the player never changes. The call happens only
//! when the look changes: calling `set-camera` every tick with fixed
//! angles would override the mouse and kill the look. Releasing the key
//! (or `/freelook` off, or leaving the world) calls `set-camera(None)`
//! and the camera snaps back.
//!
//! What the SDK cannot do, the original's native build can: the
//! third-person orbit — the original pulls the camera out behind the
//! player and scrolls it from 1 to 24 blocks — because this port keeps
//! the camera at the eye and only the look decouples (the freelook the
//! camera spike describes). An orbit would be a `position: some` call
//! with the position computed from the camera's yaw and pitch; it is a
//! follow-up, not this port. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/freelook>.
use horizon_mod_sdk::{
    Mod, Registration, game,
    hud::{self, Corner, Line},
    input::{self, Event},
    render::{self, CameraOverride},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `render.get-camera` / `render.set-camera` are level 14, the rest
/// level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 14;

/// The key's name for `register-key`, and its default: X, as the
/// original binds it. No other Kestrel mod uses X (V Classic Zoom, R/U
/// JEI, K Armor HUD, H Hitbox+, N/M Waypoints, F7 Light Overlay, J
/// Fullbright, B Fullbright & NoFog), and X is unbound in vanilla.
const KEY: &str = "freelook";
const DEFAULT_KEY: &str = "key.keyboard.x";

/// The storage key for the hold/toggle mode.
const MODE_KEY: &str = "mode";

/// The settings page's button: the hold/toggle mode.
const MODE_BUTTON: u32 = 0;

/// The indicator while freelook is active.
const INDICATOR: &str = "[Freelook]";

/// Hold the key, or press it to toggle: the mode the settings page
/// picks, kept in storage.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Hold,
    Toggle,
}

/// The stored mode: `toggle` when it says so, else hold.
fn read_mode(stored: Option<Vec<u8>>) -> Mode {
    if stored.is_some_and(|v| v == b"toggle") {
        Mode::Toggle
    } else {
        Mode::Hold
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Hold => "Hold",
        Mode::Toggle => "Toggle",
    }
}

struct Freelook {
    mode: Mode,
    /// The key is physically down (hold mode only).
    held: bool,
    /// Latched on by the key (toggle mode) or `/freelook`.
    latched: bool,
    /// Whether this mod currently has the camera detached.
    active: bool,
    /// Whether the indicator panel is currently shown.
    shown: bool,
}

impl Freelook {
    /// Whether the camera should be detached right now.
    fn wants(&self) -> bool {
        self.held || self.latched
    }

    /// The key went down: hold it in hold mode, flip the latch in
    /// toggle mode.
    fn key_down(&mut self) {
        match self.mode {
            Mode::Hold => self.held = true,
            Mode::Toggle => self.latched = !self.latched,
        }
    }

    /// The key came up: let go in hold mode; nothing in toggle mode.
    fn key_up(&mut self) {
        if self.mode == Mode::Hold {
            self.held = false;
        }
    }

    /// `/freelook`: flip the latch in either mode.
    fn toggle(&mut self) {
        self.latched = !self.latched;
    }

    /// The settings page picked a mode: store it, and drop a stale hold
    /// (a key held across the switch must not stick the camera on).
    /// The storage write and the settings refresh live in `ui_select`,
    /// so this stays a pure state change the unit tests can drive.
    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.held = false;
    }

    /// Leaving the world: let go of everything, so the camera and the
    /// indicator release with it. The caller runs `apply` after this.
    fn leave_world(&mut self) {
        self.held = false;
        self.latched = false;
    }

    /// The settings page: one button, the hold/toggle mode.
    fn refresh_settings(&self) {
        ui::set_settings(&[Widget::Button(ButtonWidget {
            id: MODE_BUTTON,
            label: format!("Mode: {}", mode_name(self.mode)),
        })]);
    }

    /// Brings the camera and the indicator in line with what the keys
    /// want — and only then: `set-camera` is called on change, never
    /// every frame, or the mouse look dies (the camera spike's §8.1).
    /// Activating takes the player's own yaw and pitch as the look the
    /// mouse then turns from; with no player there is nothing to
    /// detach from, so the mod stays off.
    fn apply(&mut self) {
        if self.wants() && !self.active {
            let Some(player) = game::get_player() else {
                return;
            };
            render::set_camera(Some(CameraOverride {
                position: None,
                yaw: player.yaw,
                pitch: player.pitch,
                roll: 0.0,
            }));
            self.active = true;
        } else if !self.wants() && self.active {
            render::set_camera(None);
            self.active = false;
        }
        if self.active && !self.shown {
            hud::set_panel(
                Corner::TopRight,
                &[Line {
                    text: INDICATOR.into(),
                    color: 0xFFFFFF,
                }],
            );
            self.shown = true;
        } else if !self.active && self.shown {
            hud::clear();
            self.shown = false;
        }
    }
}

impl Mod for Freelook {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(KEY, "Freelook", DEFAULT_KEY)?;
        Ok((
            Freelook {
                mode: read_mode(storage::get(MODE_KEY)),
                held: false,
                latched: false,
                active: false,
                shown: false,
            },
            Registration::new().command("freelook").on_frame(),
        ))
    }

    fn frame(&mut self, _seconds: f32) {
        if game::get_player().is_none() {
            self.leave_world();
            self.apply();
            return;
        }
        let mut changed = false;
        for event in input::poll() {
            match event {
                Event::KeyDown(k) if k == KEY => {
                    self.key_down();
                    changed = true;
                }
                Event::KeyUp(k) if k == KEY => {
                    self.key_up();
                    changed = true;
                }
                _ => continue,
            }
        }
        if changed {
            self.apply();
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "freelook" {
            return Err("This command does nothing".into());
        }
        let _ = game::get_player().ok_or("Not in a world")?;
        self.toggle();
        self.apply();
        Ok(Some(format!(
            "Freelook {}",
            if self.active { "enabled" } else { "disabled" }
        )))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        if id == MODE_BUTTON {
            let mode = match self.mode {
                Mode::Hold => Mode::Toggle,
                Mode::Toggle => Mode::Hold,
            };
            self.set_mode(mode);
            let _ = storage::set(
                MODE_KEY,
                match mode {
                    Mode::Hold => b"hold".as_slice(),
                    Mode::Toggle => b"toggle".as_slice(),
                },
            );
            self.refresh_settings();
            self.apply();
        }
    }
}

horizon_mod_sdk::export_mod!(Freelook);

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Freelook {
        Freelook {
            mode: Mode::Hold,
            held: false,
            latched: false,
            active: false,
            shown: false,
        }
    }

    #[test]
    fn api_level_is_fourteen() {
        // Newest imported functions are `render.get-camera` /
        // `render.set-camera` (level 14); the rest level 0.
        assert_eq!(API_LEVEL, 14);
    }

    #[test]
    fn hold_mode_holds_while_the_key_is_down() {
        let mut m = fresh();
        assert!(!m.wants());
        m.key_down();
        assert!(m.wants());
        // A second press while held changes nothing.
        m.key_down();
        assert!(m.wants());
        m.key_up();
        assert!(!m.wants());
        // Releasing again changes nothing.
        m.key_up();
        assert!(!m.wants());
    }

    #[test]
    fn toggle_mode_flips_on_press_and_ignores_release() {
        let mut m = fresh();
        m.mode = Mode::Toggle;
        m.key_down();
        assert!(m.wants());
        m.key_up();
        assert!(m.wants(), "release does not unlatch in toggle mode");
        m.key_down();
        assert!(!m.wants());
        m.key_up();
        assert!(!m.wants());
    }

    #[test]
    fn the_command_latches_in_either_mode() {
        let mut m = fresh();
        m.toggle();
        assert!(m.wants());
        m.toggle();
        assert!(!m.wants());
        // And in toggle mode too: the command and the key share the latch.
        m.mode = Mode::Toggle;
        m.toggle();
        assert!(m.wants());
        m.key_down();
        assert!(!m.wants(), "the key flips the same latch");
    }

    #[test]
    fn hold_and_latch_combine() {
        // The command latches it on; letting go of the key must not turn
        // it off while the latch holds.
        let mut m = fresh();
        m.toggle();
        m.key_down();
        m.key_up();
        assert!(m.wants());
        m.toggle();
        assert!(!m.wants());
    }

    #[test]
    fn switching_mode_drops_a_stale_hold() {
        let mut m = fresh();
        m.key_down();
        assert!(m.wants());
        m.set_mode(Mode::Toggle);
        assert!(!m.wants(), "a hold must not stick across the switch");
        assert_eq!(m.mode, Mode::Toggle);
        // The latch survives the switch back.
        m.key_down();
        assert!(m.wants());
        m.set_mode(Mode::Hold);
        assert!(m.wants(), "the latch is not a hold");
        m.key_up();
        assert!(m.wants(), "release does not clear the latch");
        m.toggle();
        assert!(!m.wants());
    }

    #[test]
    fn leaving_the_world_lets_go_of_everything() {
        let mut m = fresh();
        m.key_down();
        m.toggle();
        assert!(m.wants());
        m.leave_world();
        assert!(!m.wants());
        assert!(!m.held);
        assert!(!m.latched);
    }

    #[test]
    fn mode_reads_back_with_defaults() {
        assert_eq!(read_mode(None), Mode::Hold);
        assert_eq!(read_mode(Some(b"hold".to_vec())), Mode::Hold);
        assert_eq!(read_mode(Some(b"toggle".to_vec())), Mode::Toggle);
        assert_eq!(read_mode(Some(b"nope".to_vec())), Mode::Hold);
        assert_eq!(mode_name(Mode::Hold), "Hold");
        assert_eq!(mode_name(Mode::Toggle), "Toggle");
    }
}

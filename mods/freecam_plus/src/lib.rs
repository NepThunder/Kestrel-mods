//! Freecam+ for Kestrel: press U and the camera detaches — fly
//! it anywhere while the player stays perfectly still.
//! WASD/Space/Shift and the mouse still work, but they drive
//! the detached camera; the player stands where they were
//! until U releases them. Fly speed is the catalog freecam's
//! switch (2 / 5 / 10 / 20 / 30 / 40 / 60 blocks/s, default 10),
//! cycled with the comma key and kept between sessions.
//!
//! Behaviour reimplemented from Freecam+ by _idom_ (MIT,
//! <https://www.curseforge.com/minecraft/mc-mods/freecamplus>);
//! this file shares no code with the original — the original
//! is a native mod, this is a fresh implementation against the
//! Kestrel SDK. The original states no controls or speeds of
//! its own beyond U and the stillness, so the flight here uses
//! the established Kestrel freecam idiom: dedicated registered
//! fly keys (the only way a mod in this SDK can fly a camera
//! without the movement keys) at the catalog freecam's speeds.
//! This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/freecam_plus>.
//!
//! The stillness, honestly scoped: while a camera override is
//! active the mouse turns the camera, not the player — the
//! player's yaw and pitch do not move, so the player keeps
//! walking, aiming and mining in the direction they faced.
//! That is the freeze this SDK's detach gives. The player's
//! body is NOT frozen beyond it: WASD still walks the player
//! while the camera flies, because the SDK has no way for a
//! mod to claim the movement keys — the same named gap as
//! every freecam in this catalog. "Stays perfectly still"
//! holds for the player's look and for a player who is not
//! walking; it does not hold against held movement keys.
use horizon_mod_sdk::{
    Mod, Registration, chat, game,
    input::{self, Event},
    render::{self, CameraOverride, WorldPoint},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `render.get-camera` / `render.set-camera` are level
/// 14, the rest level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 14;

/// The player's eye above their feet, as the game puts the camera.
const EYE: f64 = 1.62;
/// Blocks per second the camera flies, and the speeds the
/// switch offers: the catalog freecam's set, 10 the default.
const SPEEDS: [f32; 7] = [2.0, 5.0, 10.0, 20.0, 30.0, 40.0, 60.0];
const DEFAULT_SPEED: f32 = 10.0;

/// The toggle key's name for `register-key`, and its default:
/// the original's U.
const TOGGLE_KEY: &str = "toggle";
const DEFAULT_TOGGLE_KEY: &str = "key.keyboard.u";
/// The movement keys' names for `register-key`, and their
/// defaults: the catalog freecam's cluster (I forward, comma
/// back, semicolon left, apostrophe right, equal up, minus
/// down) — free keys around the right hand, none a vanilla
/// binding or another mod's key, every one rebindable.
const FORWARD_KEY: &str = "forward";
const DEFAULT_FORWARD_KEY: &str = "key.keyboard.i";
const BACK_KEY: &str = "back";
const DEFAULT_BACK_KEY: &str = "key.keyboard.comma";
const LEFT_KEY: &str = "left";
const DEFAULT_LEFT_KEY: &str = "key.keyboard.semicolon";
const RIGHT_KEY: &str = "right";
const DEFAULT_RIGHT_KEY: &str = "key.keyboard.apostrophe";
const UP_KEY: &str = "up";
const DEFAULT_UP_KEY: &str = "key.keyboard.equal";
const DOWN_KEY: &str = "down";
const DEFAULT_DOWN_KEY: &str = "key.keyboard.minus";

/// The storage key for the fly speed.
const SPEED_KEY: &str = "speed";

/// The settings page's buttons: the freecam switch, the fly speed.
const TOGGLE_BUTTON: u32 = 0;
const SPEED_BUTTON: u32 = 1;

/// Which movement keys are held down right now.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
struct Held {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

impl Held {
    fn press(&mut self, key: &str) -> bool {
        match key {
            FORWARD_KEY => self.forward = true,
            BACK_KEY => self.back = true,
            LEFT_KEY => self.left = true,
            RIGHT_KEY => self.right = true,
            UP_KEY => self.up = true,
            DOWN_KEY => self.down = true,
            _ => return false,
        }
        true
    }

    fn release(&mut self, key: &str) {
        match key {
            FORWARD_KEY => self.forward = false,
            BACK_KEY => self.back = false,
            LEFT_KEY => self.left = false,
            RIGHT_KEY => self.right = false,
            UP_KEY => self.up = false,
            DOWN_KEY => self.down = false,
            _ => {}
        }
    }

    fn clear(&mut self) {
        *self = Held::default();
    }

    fn any(&self) -> bool {
        self.forward || self.back || self.left || self.right || self.up || self.down
    }
}

/// The look direction for a yaw/pitch in degrees, as F3 shows
/// them: yaw 0 faces south (+z), pitch -90 straight up.
fn forward(yaw: f32, pitch: f32) -> [f64; 3] {
    let (yaw, pitch) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ]
}

/// The horizontal direction to the camera's right for a yaw.
fn right(yaw: f32) -> [f64; 3] {
    let yaw = f64::from(yaw).to_radians();
    [-yaw.cos(), 0.0, -yaw.sin()]
}

/// How far the camera moves this tick from the held keys, the
/// look, and the fly speed: forward/back along the look (so
/// flying forward while looking down descends), left/right
/// along the horizontal right, up/down straight. A diagonal
/// moves at the same speed as a straight line; opposite keys
/// cancel. Twenty ticks a second, so one tick is speed / 20.
fn fly_delta(held: &Held, yaw: f32, pitch: f32, speed: f32) -> [f64; 3] {
    let f = forward(yaw, pitch);
    let r = right(yaw);
    let along = (held.forward as i32 - held.back as i32) as f64;
    let strafe = (held.right as i32 - held.left as i32) as f64;
    let vertical = (held.up as i32 - held.down as i32) as f64;
    let mut delta = [0.0; 3];
    for i in 0..3 {
        delta[i] = f[i] * along + r[i] * strafe;
    }
    delta[1] += vertical;
    let len = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if len > 0.0 {
        let step = f64::from(speed) / 20.0 / len;
        for d in &mut delta {
            *d *= step;
        }
    }
    delta
}

/// The stored fly speed: the stored number when it is one the
/// switch offers, else the default 10.
fn read_speed(stored: Option<Vec<u8>>) -> f32 {
    let wanted = stored
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(DEFAULT_SPEED);
    if SPEEDS.contains(&wanted) {
        wanted
    } else {
        DEFAULT_SPEED
    }
}

/// The next speed in the switch's cycle, wrapping to the first.
fn next_speed(speed: f32) -> f32 {
    let i = SPEEDS.iter().position(|s| *s == speed).unwrap_or(2);
    SPEEDS[(i + 1) % SPEEDS.len()]
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

struct FreecamPlus {
    detached: bool,
    held: Held,
    speed: f32,
}

impl FreecamPlus {
    /// The effective camera pose, one decimal a field, for the
    /// toggle's feedback.
    fn pose_feedback(on: bool) -> String {
        let p = render::get_camera();
        format!(
            "Freecam+ {} — camera x={:.1} y={:.1} z={:.1} yaw={:.1} pitch={:.1}",
            if on { "on" } else { "off" },
            p.x, p.y, p.z, p.yaw, p.pitch
        )
    }

    /// The settings page: the freecam switch and the fly speed.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: TOGGLE_BUTTON,
                label: format!("Freecam+: {}", on_off(self.detached)),
            }),
            Widget::Button(ButtonWidget {
                id: SPEED_BUTTON,
                label: format!("Fly speed: {} blocks/s", self.speed),
            }),
        ]);
    }

    /// Detach the camera at the player's eye, looking where
    /// they look, or release it back to the player. Releasing
    /// also forgets any held movement keys.
    fn set_detached(&mut self, on: bool) {
        if on {
            let Some(player) = game::get_player() else {
                return;
            };
            self.detached = true;
            self.held.clear();
            render::set_camera(Some(CameraOverride {
                position: Some(WorldPoint {
                    x: player.x,
                    y: player.y + EYE,
                    z: player.z,
                }),
                yaw: player.yaw,
                pitch: player.pitch,
                roll: 0.0,
            }));
        } else {
            self.detached = false;
            self.held.clear();
            render::set_camera(None);
        }
        self.refresh_settings();
    }

    fn cycle_speed(&mut self) {
        self.speed = next_speed(self.speed);
        let _ = storage::set(SPEED_KEY, self.speed.to_string().as_bytes());
        self.refresh_settings();
    }
}

impl Mod for FreecamPlus {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(TOGGLE_KEY, "Toggle Freecam+", DEFAULT_TOGGLE_KEY)?;
        input::register_key(FORWARD_KEY, "Freecam+: fly forward", DEFAULT_FORWARD_KEY)?;
        input::register_key(BACK_KEY, "Freecam+: fly back", DEFAULT_BACK_KEY)?;
        input::register_key(LEFT_KEY, "Freecam+: fly left", DEFAULT_LEFT_KEY)?;
        input::register_key(RIGHT_KEY, "Freecam+: fly right", DEFAULT_RIGHT_KEY)?;
        input::register_key(UP_KEY, "Freecam+: fly up", DEFAULT_UP_KEY)?;
        input::register_key(DOWN_KEY, "Freecam+: fly down", DEFAULT_DOWN_KEY)?;
        let state = FreecamPlus {
            detached: false,
            held: Held::default(),
            speed: read_speed(storage::get(SPEED_KEY)),
        };
        Ok((
            state,
            Registration::new().command("freecam_plus").on_tick(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            match event {
                Event::KeyDown(k) if k == TOGGLE_KEY => {
                    let on = !self.detached;
                    self.set_detached(on);
                    chat::show(if on { "Freecam+ on" } else { "Freecam+ off" });
                }
                Event::KeyDown(k) => {
                    self.held.press(&k);
                }
                Event::KeyUp(k) => {
                    self.held.release(&k);
                }
                _ => {}
            }
        }
        if game::get_player().is_none() {
            if self.detached {
                self.set_detached(false);
            }
            return;
        }
        if !self.detached || !self.held.any() {
            // No movement this tick: do not call set-camera at
            // all, so the mouse keeps turning the camera.
            return;
        }
        // Move from the live pose and write it back with that
        // look, so mouse look survives.
        let pose = render::get_camera();
        let delta = fly_delta(&self.held, pose.yaw, pose.pitch, self.speed);
        render::set_camera(Some(CameraOverride {
            position: Some(WorldPoint {
                x: pose.x + delta[0],
                y: pose.y + delta[1],
                z: pose.z + delta[2],
            }),
            yaw: pose.yaw,
            pitch: pose.pitch,
            roll: 0.0,
        }));
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "freecam_plus" {
            return Err("This command does nothing".into());
        }
        if !self.detached && game::get_player().is_none() {
            return Ok(Some("freecam_plus: no player".into()));
        }
        let on = !self.detached;
        self.set_detached(on);
        Ok(Some(Self::pose_feedback(on)))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            TOGGLE_BUTTON => {
                let on = !self.detached;
                self.set_detached(on);
            }
            SPEED_BUTTON => self.cycle_speed(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(FreecamPlus);

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn close3(a: [f64; 3], b: [f64; 3]) -> bool {
        close(a[0], b[0]) && close(a[1], b[1]) && close(a[2], b[2])
    }

    #[test]
    fn api_level_is_fourteen() {
        assert_eq!(API_LEVEL, 14);
    }

    #[test]
    fn forward_and_right_follow_the_look() {
        assert!(close3(forward(0.0, 0.0), [0.0, 0.0, 1.0]));
        assert!(close3(forward(90.0, 0.0), [-1.0, 0.0, 0.0]));
        assert!(close3(forward(0.0, -90.0), [0.0, 1.0, 0.0]));
        assert!(close3(forward(0.0, 90.0), [0.0, -1.0, 0.0]));
        assert!(close3(right(0.0), [-1.0, 0.0, 0.0]));
        assert!(close3(right(270.0), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn flight_moves_at_the_speed_along_the_look() {
        let mut held = Held::default();
        held.press(FORWARD_KEY);
        // Looking down 45 facing south: forward and down equally,
        // half a block a tick at 10 blocks/s in total.
        let d = fly_delta(&held, 0.0, 45.0, 10.0);
        let s = std::f64::consts::FRAC_1_SQRT_2 * 0.5;
        assert!(close3(d, [0.0, -s, s]));
        // A diagonal is the same speed, not faster.
        held.press(RIGHT_KEY);
        let d = fly_delta(&held, 0.0, 0.0, 10.0);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        assert!(close(len, 0.5));
        // Opposite keys cancel; releasing clears the hold.
        held.press(BACK_KEY);
        held.press(LEFT_KEY);
        let d = fly_delta(&held, 0.0, 0.0, 10.0);
        assert!(close3(d, [0.0, 0.0, 0.0]));
        held.release(FORWARD_KEY);
        assert!(!held.forward && held.back);
        held.clear();
        assert!(!held.any());
    }

    #[test]
    fn speeds_cycle_and_read_back() {
        assert_eq!(next_speed(10.0), 20.0);
        assert_eq!(next_speed(60.0), 2.0);
        assert_eq!(read_speed(Some(b"30".to_vec())), 30.0);
        assert_eq!(read_speed(Some(b"7".to_vec())), 10.0);
        assert_eq!(read_speed(None), 10.0);
    }

    #[test]
    fn held_keys_press_and_release() {
        let mut held = Held::default();
        assert!(held.press(UP_KEY));
        assert!(held.press(DOWN_KEY));
        assert!(!held.press("nope"));
        held.release(UP_KEY);
        assert!(!held.up && held.down);
    }
}

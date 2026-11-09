//! Freecam for Kestrel: detach the camera from the player and fly it
//! around on dedicated keys while the player stays where they were, and
//! the mouse looks around. Y toggles it (rebindable on the mod's page in
//! Options > Mods), `/freecam` does the same, and that page also has the
//! same switch plus the fly-speed switch; the game keeps the speed.
//!
//! Behaviour reimplemented from Freecam 0.1.0 by hakergeniusz (MIT,
//! <https://github.com/hakergeniusz/freecam>), itself a rewrite for
//! Horizon of Freecam by the Freecam authors (MIT,
//! <https://github.com/MinecraftFreecam/Freecam>); this file shares no
//! code with either — the originals are native mods, this is a fresh
//! implementation against the Kestrel SDK. The original detaches the
//! camera on F4, flies it with the movement keys, and offers tripod
//! cameras, creative-style flight, and settings for the speeds, where
//! the camera starts, and what happens on damage; this port detaches
//! on Y, flies on its own keys (see below), and keeps the speed as its
//! one setting. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/freecam>.
//!
//! The camera rides the level-14 camera API (`render.get-camera` /
//! `render.set-camera`, `crates/horizon-mods/src/api_level.rs`): while
//! detached the mod reads the live camera pose back every tick — the
//! look the mouse just turned — moves only the position from the held
//! keys, and writes the pose back with that live look. It never calls
//! `set-camera` with a fixed look, which would freeze mouse look (the
//! call pattern the camera spike warns about), and it never calls it
//! at all on a tick when no movement key is held, so the mouse turns
//! the camera freely between moves. The host wall-clamps the camera
//! and caps it at 64 blocks from the player's feet, so there is no
//! seeing through walls in v1 and no scouting across the map.
//!
//! What the SDK cannot do, the original's native build can: the
//! movement keys cannot fly the camera — `input` has no movement-key
//! claim, so WASD still walks the player while the camera is detached
//! — and the camera cannot pass through walls. Those are the two
//! named follow-ups in the camera spike (a free-clip policy
//! relaxation and an `input.claim-keys` capability), not oversights
//! here. The crosshair, mining, and the player's aim all stay the
//! player's while detached, and the player can still be hurt; the
//! camera is a view, not a body.
use horizon_mod_sdk::{
    Mod, Registration, chat, game,
    hud::{self, Corner, Line},
    input::{self, Event},
    render::{self, CameraOverride, WorldPoint},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `render.get-camera` / `render.set-camera` are level 14, the rest
/// level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 14;

/// The player's eye above their feet, as the game puts the camera.
const EYE: f64 = 1.62;
/// Blocks per second the camera flies, and the speeds the switch
/// offers; 10 is the original's middle ground and Fair Freecam's.
const SPEEDS: [f32; 5] = [2.0, 5.0, 10.0, 20.0, 40.0];
const DEFAULT_SPEED: f32 = 10.0;

/// The toggle key's name for `register-key`, and its default: Y. The
/// original binds F4, but F4 is vanilla's spectator-shader toggle in
/// Horizon, and Z (the nearest free letter) is the Extras zoom key —
/// Y is the nearest free key with no vanilla or Kestrel use.
const TOGGLE_KEY: &str = "toggle";
const DEFAULT_TOGGLE_KEY: &str = "key.keyboard.y";
/// The movement keys' names for `register-key`, and their defaults.
/// No letter cluster is free — J and K are Fullbright and Armor HUD,
/// L is vanilla Advancements, WASD walks the player — so the fly keys
/// are the free keys around the right hand: I forward, comma back,
/// semicolon left, apostrophe right, equal up, minus down. None is a
/// vanilla binding or another Kestrel mod's key (I is only an F3
/// debug combo), and every one is rebindable on the mod's page.
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
    /// A movement key went down: true when it is one of ours.
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

    /// A movement key came up.
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

/// The look direction for a yaw/pitch in degrees, as F3 shows them:
/// yaw 0 faces south (+z), yaw 90 west (-x), pitch -90 straight up.
/// The same forward the camera probe's anchor uses.
fn forward(yaw: f32, pitch: f32) -> [f64; 3] {
    let (yaw, pitch) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ]
}

/// The horizontal direction to the camera's right for a yaw: facing
/// south it is west (-x), facing west it is north (-z).
fn right(yaw: f32) -> [f64; 3] {
    let yaw = f64::from(yaw).to_radians();
    [-yaw.cos(), 0.0, -yaw.sin()]
}

/// How far the camera moves this tick from the held keys, the look,
/// and the fly speed in blocks per second: forward/back along the
/// look (so flying forward while looking down descends), left/right
/// along the horizontal right, up/down straight up and down. A
/// diagonal moves at the same speed as a straight line, and opposite
/// keys cancel. Twenty ticks a second, so one tick is speed / 20.
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

/// The stored fly speed: the stored number when it is one the switch
/// offers, else the default 10.
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

/// The next speed in the switch's cycle, wrapping back to the first.
fn next_speed(speed: f32) -> f32 {
    let i = SPEEDS.iter().position(|s| *s == speed).unwrap_or(2);
    SPEEDS[(i + 1) % SPEEDS.len()]
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

struct Freecam {
    detached: bool,
    held: Held,
    speed: f32,
    last_panel: Vec<String>,
}

impl Freecam {
    /// The effective camera pose, one decimal a field, for the toggle's
    /// feedback: it proves the camera call took, in the probe and to
    /// the player.
    fn pose_feedback(on: bool) -> String {
        let p = render::get_camera();
        format!(
            "Freecam {} — camera x={:.1} y={:.1} z={:.1} yaw={:.1} pitch={:.1}",
            if on { "on" } else { "off" },
            p.x,
            p.y,
            p.z,
            p.yaw,
            p.pitch
        )
    }

    /// The notice while detached: the toggle key and the fly speed.
    fn panel_lines(&self) -> Vec<String> {
        vec![
            "Freecam on — Y to return".to_string(),
            format!("Speed: {} blocks/s", self.speed),
        ]
    }

    fn show_panel(&mut self) {
        let lines = self.panel_lines();
        if lines != self.last_panel {
            self.last_panel = lines.clone();
            hud::set_panel(
                Corner::TopLeft,
                &lines
                    .iter()
                    .map(|text| Line {
                        text: text.clone(),
                        color: 0xFFFFFF,
                    })
                    .collect::<Vec<_>>(),
            );
        }
    }

    fn clear_panel(&mut self) {
        if !self.last_panel.is_empty() {
            hud::clear();
            self.last_panel.clear();
        }
    }

    /// The settings page: the freecam switch and the fly speed.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: TOGGLE_BUTTON,
                label: format!("Freecam: {}", on_off(self.detached)),
            }),
            Widget::Button(ButtonWidget {
                id: SPEED_BUTTON,
                label: format!("Fly speed: {} blocks/s", self.speed),
            }),
        ]);
    }

    /// Detach the camera at the player's eye, looking where they look,
    /// or release it back to the player. Releasing also clears the
    /// notice and forgets any held movement keys.
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
            self.show_panel();
        } else {
            self.detached = false;
            self.held.clear();
            render::set_camera(None);
            self.clear_panel();
        }
        self.refresh_settings();
    }

    fn cycle_speed(&mut self) {
        self.speed = next_speed(self.speed);
        let _ = storage::set(SPEED_KEY, self.speed.to_string().as_bytes());
        self.refresh_settings();
        if self.detached {
            self.show_panel();
        }
    }
}

impl Mod for Freecam {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(TOGGLE_KEY, "Toggle Freecam", DEFAULT_TOGGLE_KEY)?;
        input::register_key(FORWARD_KEY, "Freecam: fly forward", DEFAULT_FORWARD_KEY)?;
        input::register_key(BACK_KEY, "Freecam: fly back", DEFAULT_BACK_KEY)?;
        input::register_key(LEFT_KEY, "Freecam: fly left", DEFAULT_LEFT_KEY)?;
        input::register_key(RIGHT_KEY, "Freecam: fly right", DEFAULT_RIGHT_KEY)?;
        input::register_key(UP_KEY, "Freecam: fly up", DEFAULT_UP_KEY)?;
        input::register_key(DOWN_KEY, "Freecam: fly down", DEFAULT_DOWN_KEY)?;
        let state = Freecam {
            detached: false,
            held: Held::default(),
            speed: read_speed(storage::get(SPEED_KEY)),
            last_panel: Vec::new(),
        };
        Ok((state, Registration::new().command("freecam").on_tick()))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            match event {
                Event::KeyDown(k) if k == TOGGLE_KEY => {
                    let on = !self.detached;
                    self.set_detached(on);
                    chat::show(if on { "Freecam on" } else { "Freecam off" });
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
            // Leaving the world releases the camera and clears the
            // notice; the host restores the player's camera anyway.
            if self.detached {
                self.set_detached(false);
            }
            return;
        }
        if !self.detached || !self.held.any() {
            // No movement this tick: do not call set-camera at all, so
            // the mouse keeps turning the camera between moves.
            return;
        }
        // Move from the live pose — the look the mouse just turned —
        // and write it back with that look, so mouse look survives.
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
        if name != "freecam" {
            return Err("This command does nothing".into());
        }
        if !self.detached && game::get_player().is_none() {
            return Ok(Some("freecam: no player".into()));
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

horizon_mod_sdk::export_mod!(Freecam);

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
        // Newest imported functions are `render.get-camera` and
        // `render.set-camera` (level 14); the rest level 0.
        assert_eq!(API_LEVEL, 14);
    }

    #[test]
    fn forward_follows_yaw_and_pitch() {
        // Facing south, level: +z.
        assert!(close3(forward(0.0, 0.0), [0.0, 0.0, 1.0]));
        // Facing west, level: -x.
        assert!(close3(forward(90.0, 0.0), [-1.0, 0.0, 0.0]));
        // Facing north, level: -z.
        assert!(close3(forward(180.0, 0.0), [0.0, 0.0, -1.0]));
        // Facing east, level: +x.
        assert!(close3(forward(270.0, 0.0), [1.0, 0.0, 0.0]));
        // Straight up and straight down, whatever the yaw.
        assert!(close3(forward(0.0, -90.0), [0.0, 1.0, 0.0]));
        assert!(close3(forward(90.0, 90.0), [0.0, -1.0, 0.0]));
        // Looking down 45° while facing south: forward and down equally.
        let s = std::f64::consts::FRAC_1_SQRT_2;
        assert!(close3(forward(0.0, 45.0), [0.0, -s, s]));
    }

    #[test]
    fn right_is_west_when_facing_south() {
        assert!(close3(right(0.0), [-1.0, 0.0, 0.0]));
        assert!(close3(right(90.0), [0.0, 0.0, -1.0]));
        assert!(close3(right(180.0), [1.0, 0.0, 0.0]));
        assert!(close3(right(270.0), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn fly_delta_moves_one_tick_at_the_fly_speed() {
        // Speed 10 is half a block a tick, facing south.
        let held = Held {
            forward: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 10.0), [0.0, 0.0, 0.5]));
        // Back is the opposite; up and down are straight.
        let held = Held {
            back: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 10.0), [0.0, 0.0, -0.5]));
        let held = Held {
            up: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 10.0), [0.0, 0.5, 0.0]));
        let held = Held {
            down: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 10.0), [0.0, -0.5, 0.0]));
        // Left is east when facing south; right is west.
        let held = Held {
            left: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 10.0), [0.5, 0.0, 0.0]));
        let held = Held {
            right: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 10.0), [-0.5, 0.0, 0.0]));
        // Speed 2 is a tenth of a block a tick.
        let held = Held {
            forward: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 2.0), [0.0, 0.0, 0.1]));
        // Nothing held: no movement.
        assert!(close3(
            fly_delta(&Held::default(), 0.0, 0.0, 10.0),
            [0.0, 0.0, 0.0]
        ));
    }

    #[test]
    fn fly_delta_diagonals_keep_the_speed_and_opposites_cancel() {
        // Forward + right at speed 10: still half a block a tick,
        // split between the two directions.
        let held = Held {
            forward: true,
            right: true,
            ..Held::default()
        };
        let d = fly_delta(&held, 0.0, 0.0, 10.0);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        assert!(close(len, 0.5));
        assert!(close3(d, [-0.3535533905932738, 0.0, 0.3535533905932738]));
        // Forward + back cancel; left + right cancel; up + down cancel.
        let held = Held {
            forward: true,
            back: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 10.0), [0.0, 0.0, 0.0]));
        let held = Held {
            left: true,
            right: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 10.0), [0.0, 0.0, 0.0]));
        let held = Held {
            up: true,
            down: true,
            ..Held::default()
        };
        assert!(close3(fly_delta(&held, 0.0, 0.0, 10.0), [0.0, 0.0, 0.0]));
    }

    #[test]
    fn held_tracks_press_and_release_by_name() {
        let mut held = Held::default();
        assert!(!held.any());
        assert!(held.press(FORWARD_KEY));
        assert!(held.press(UP_KEY));
        assert!(held.any());
        assert!(held.forward);
        assert!(held.up);
        // An unknown key is not ours.
        assert!(!held.press("toggle"));
        assert!(!held.press("nope"));
        held.release(FORWARD_KEY);
        assert!(!held.forward);
        assert!(held.up);
        held.release(UP_KEY);
        assert!(!held.any());
        // Releasing a key that was never pressed is harmless.
        held.release(BACK_KEY);
        assert!(!held.any());
        // Clear forgets everything held.
        held.press(LEFT_KEY);
        held.press(DOWN_KEY);
        held.clear();
        assert!(!held.any());
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert_eq!(read_speed(None), 10.0);
        assert_eq!(read_speed(Some(b"2".to_vec())), 2.0);
        assert_eq!(read_speed(Some(b"5".to_vec())), 5.0);
        assert_eq!(read_speed(Some(b"20".to_vec())), 20.0);
        assert_eq!(read_speed(Some(b"40".to_vec())), 40.0);
        // A stored speed the switch never offers falls back to 10.
        assert_eq!(read_speed(Some(b"99".to_vec())), 10.0);
        assert_eq!(read_speed(Some(b"nope".to_vec())), 10.0);
        // The cycle steps through all five and wraps back to 2.
        assert_eq!(next_speed(2.0), 5.0);
        assert_eq!(next_speed(5.0), 10.0);
        assert_eq!(next_speed(10.0), 20.0);
        assert_eq!(next_speed(20.0), 40.0);
        assert_eq!(next_speed(40.0), 2.0);
        assert_eq!(on_off(true), "ON");
        assert_eq!(on_off(false), "OFF");
    }

    #[test]
    fn panel_shows_the_toggle_key_and_the_speed() {
        let freecam = Freecam {
            detached: true,
            held: Held::default(),
            speed: 10.0,
            last_panel: Vec::new(),
        };
        assert_eq!(
            freecam.panel_lines(),
            ["Freecam on — Y to return", "Speed: 10 blocks/s"]
        );
        let freecam = Freecam {
            speed: 40.0,
            ..freecam
        };
        assert_eq!(
            freecam.panel_lines(),
            ["Freecam on — Y to return", "Speed: 40 blocks/s"]
        );
    }
}

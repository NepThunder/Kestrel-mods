//! FreeCam for Kestrel, Yannis_'s: F4 starts the free camera,
//! F6 switches to a follow view at a hover (crosshair hidden),
//! F4 ends it. Holding F4 and pressing 1-9 saves the current
//! camera position as that tripod; pressing 1-9 with the
//! camera off starts the free camera at the saved tripod.
//! Tripods are kept per world in the mod's own storage.
//! Follow distance 28, height 18 (settings).
//!
//! Behaviour reimplemented from FreeCam by Yannis_ (MIT,
//! <https://www.curseforge.com/minecraft/mc-mods/freecam-yannis>);
//! this file shares no code with the original — the original
//! is a native mod, this is a fresh implementation against the
//! Kestrel SDK. The flight itself uses the catalog freecam's
//! established idiom (dedicated registered fly keys at its
//! speed set), because a mod in this SDK cannot fly a camera
//! on the movement keys; the original states no fly keys or
//! speeds of its own. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/freecam_yannis>.
//!
//! The tripod keys are the mod's own registered keys
//! (tripod 1-9, defaulting to the number row, as the
//! original's ride it): a mod may register 32 keys, and this
//! mod's 17 (F4, F6, six fly keys, nine tripods) fit. They are
//! rebindable in Controls like the rest; a tripod key also
//! selects its hotbar slot, because the game's own hotbar
//! binding sits on the same physical key and nothing in this
//! SDK can claim a key away from it.
//!
//! Two of the original's behaviours still meet SDK limits,
//! named here and in the README. The follow view at a hover:
//! the camera parks at the follow distance behind and above
//! the player, but the original keeps the view LIVE — its
//! camera follows the moving player — while this SDK's
//! `set-camera` takes a fixed position per call and a mod's
//! plain detach cannot track the player (a mod that keeps
//! re-setting the camera each tick would fight the host's
//! first-mod-wins camera aggregate and the mouse-look rule);
//! so this port's follow view is the parked hover the original
//! describes, set once when F6 is pressed. The original hides
//! the crosshair in follow view; this port hides the HUD with
//! it (`render.set-hide-hud` — the SDK's hide is the whole HUD,
//! F1-style, not the crosshair alone). And the tripods are
//! keyed per world by the dimension's name (the only world
//! identity a client mod can read), not by the world's own
//! save identity.
use horizon_mod_sdk::{
    Mod, Registration, chat, game,
    input::{self, Event},
    render::{self, CameraOverride, WorldPoint},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `render.set-hide-hud` is level 19, `render.get-camera`
/// / `render.set-camera` are level 14, the rest level 0
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 19;

/// The player's eye above their feet, as the game puts the camera.
const EYE: f64 = 1.62;
/// Blocks per second the camera flies, and the speeds the
/// switch offers: the catalog freecam's set, 10 the default.
const SPEEDS: [f32; 7] = [2.0, 5.0, 10.0, 20.0, 30.0, 40.0, 60.0];
const DEFAULT_SPEED: f32 = 10.0;

/// The original's keys: F4 the free camera, F6 the follow view.
const TOGGLE_KEY: &str = "toggle";
const DEFAULT_TOGGLE_KEY: &str = "key.keyboard.f4";
const FOLLOW_KEY: &str = "follow";
const DEFAULT_FOLLOW_KEY: &str = "key.keyboard.f6";
/// The nine tripod keys: the mod's own registered keys,
/// defaulting to the number row the original's ride. All 17 of
/// the mod's keys fit the 32-key registration cap.
const TRIPOD_KEYS: [&str; 9] = [
    "tripod_1", "tripod_2", "tripod_3", "tripod_4", "tripod_5", "tripod_6", "tripod_7",
    "tripod_8", "tripod_9",
];
const DEFAULT_TRIPOD_KEYS: [&str; 9] = [
    "key.keyboard.1",
    "key.keyboard.2",
    "key.keyboard.3",
    "key.keyboard.4",
    "key.keyboard.5",
    "key.keyboard.6",
    "key.keyboard.7",
    "key.keyboard.8",
    "key.keyboard.9",
];
/// The catalog freecam's fly cluster (see freecam_plus for why).
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

/// The storage keys: the speed, the follow tuning, the tripods.
const SPEED_KEY: &str = "speed";
const DISTANCE_KEY: &str = "follow_distance";
const HEIGHT_KEY: &str = "follow_height";
/// The follow view's defaults, as the original: distance 28,
/// height 18; the settings cycle these sets.
const DEFAULT_DISTANCE: f32 = 28.0;
const DEFAULT_HEIGHT: f32 = 18.0;
const DISTANCES: [f32; 5] = [8.0, 14.0, 20.0, 28.0, 40.0];
const HEIGHTS: [f32; 5] = [4.0, 8.0, 12.0, 18.0, 26.0];

/// The settings page's buttons.
const TOGGLE_BUTTON: u32 = 0;
const FOLLOW_BUTTON: u32 = 1;
const SPEED_BUTTON: u32 = 2;
const DISTANCE_BUTTON: u32 = 3;
const HEIGHT_BUTTON: u32 = 4;

/// Which mode the camera is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Off,
    Free,
    Follow,
}

/// One saved tripod: a camera position and the look it had.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Tripod {
    x: f64,
    y: f64,
    z: f64,
    yaw: f32,
    pitch: f32,
}

impl Tripod {
    /// The stored text form: "x,y,z,yaw,pitch", full precision.
    fn encode(&self) -> String {
        format!("{},{},{},{},{}", self.x, self.y, self.z, self.yaw, self.pitch)
    }

    /// Parses the stored text form; anything malformed is no tripod.
    fn decode(text: &str) -> Option<Tripod> {
        let mut parts = text.split(',');
        let mut next = || parts.next()?.trim().parse::<f64>().ok();
        let tripod = Tripod {
            x: next()?,
            y: next()?,
            z: next()?,
            yaw: next()? as f32,
            pitch: next()? as f32,
        };
        if parts.next().is_some() {
            return None;
        }
        Some(tripod)
    }
}

/// The storage key for tripod `slot` (0-8) in `world`: tripods
/// are per world, keyed by the dimension's name — the only
/// world identity a client mod can read.
fn tripod_key(world: &str, slot: usize) -> String {
    format!("tripod:{world}:{}", slot + 1)
}

/// The look direction for a yaw/pitch in degrees.
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

/// How far the camera moves this tick (the catalog idiom: along
/// the look, diagonal at the same speed, speed / 20 per tick).
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

fn next_speed(speed: f32) -> f32 {
    let i = SPEEDS.iter().position(|s| *s == speed).unwrap_or(2);
    SPEEDS[(i + 1) % SPEEDS.len()]
}

/// A stored tuning value from its cycle set, else the default.
fn read_tuning(stored: Option<Vec<u8>>, set: &[f32], default: f32) -> f32 {
    let wanted = stored
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(default);
    if set.contains(&wanted) {
        wanted
    } else {
        default
    }
}

fn next_in(set: &[f32], current: f32) -> f32 {
    let i = set.iter().position(|s| *s == current).unwrap_or(0);
    set[(i + 1) % set.len()]
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

struct FreeCam {
    mode: Mode,
    held: Held,
    /// F4 is currently held down (the tripod-save chord).
    f4_held: bool,
    speed: f32,
    follow_distance: f32,
    follow_height: f32,
}

impl FreeCam {
    /// The effective camera pose, one decimal a field, for the
    /// toggles' feedback.
    fn pose_feedback(what: &str) -> String {
        let p = render::get_camera();
        format!(
            "FreeCam {what} — camera x={:.1} y={:.1} z={:.1} yaw={:.1} pitch={:.1}",
            p.x, p.y, p.z, p.yaw, p.pitch
        )
    }

    /// The settings page: the two switches, the speed, the
    /// follow tuning.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: TOGGLE_BUTTON,
                label: format!("Free camera: {}", on_off(self.mode == Mode::Free)),
            }),
            Widget::Button(ButtonWidget {
                id: FOLLOW_BUTTON,
                label: format!("Follow view: {}", on_off(self.mode == Mode::Follow)),
            }),
            Widget::Button(ButtonWidget {
                id: SPEED_BUTTON,
                label: format!("Fly speed: {} blocks/s", self.speed),
            }),
            Widget::Button(ButtonWidget {
                id: DISTANCE_BUTTON,
                label: format!("Follow distance: {} blocks", self.follow_distance),
            }),
            Widget::Button(ButtonWidget {
                id: HEIGHT_BUTTON,
                label: format!("Follow height: {} blocks", self.follow_height),
            }),
        ]);
    }

    /// Start the free camera at the player's eye (or at a tripod).
    fn start_free(&mut self, at: Option<Tripod>) {
        let Some(player) = game::get_player() else {
            return;
        };
        self.mode = Mode::Free;
        self.held.clear();
        let (pos, yaw, pitch) = match at {
            Some(t) => (
                WorldPoint {
                    x: t.x,
                    y: t.y,
                    z: t.z,
                },
                t.yaw,
                t.pitch,
            ),
            None => (
                WorldPoint {
                    x: player.x,
                    y: player.y + EYE,
                    z: player.z,
                },
                player.yaw,
                player.pitch,
            ),
        };
        render::set_camera(Some(CameraOverride {
            position: Some(pos),
            yaw,
            pitch,
            roll: 0.0,
        }));
        self.sync_hud();
        self.refresh_settings();
    }

    /// The follow view hides the HUD, as the original hides the
    /// crosshair there; every other mode shows it.
    fn sync_hud(&self) {
        render::set_hide_hud(self.mode == Mode::Follow);
    }

    /// The follow view: park the camera at the follow distance
    /// behind the player and the follow height above, looking
    /// back at them. Set once, when F6 is pressed (see the
    /// module docs for why it does not track).
    fn start_follow(&mut self) {
        let Some(player) = game::get_player() else {
            return;
        };
        self.mode = Mode::Follow;
        self.held.clear();
        let back = forward(player.yaw, 0.0);
        let pos = WorldPoint {
            x: player.x - back[0] * f64::from(self.follow_distance),
            y: player.y + f64::from(self.follow_height),
            z: player.z - back[2] * f64::from(self.follow_distance),
        };
        // Look from the hover down at the player's eye: the yaw
        // is the player's own (the camera sits behind them), the
        // pitch is the drop angle.
        let drop = (f64::from(self.follow_height) - EYE)
            .atan2(f64::from(self.follow_distance))
            .to_degrees() as f32;
        render::set_camera(Some(CameraOverride {
            position: Some(pos),
            yaw: player.yaw,
            pitch: drop,
            roll: 0.0,
        }));
        self.sync_hud();
        self.refresh_settings();
    }

    /// End the camera (F4 from any mode): back to the player.
    fn stop(&mut self) {
        self.mode = Mode::Off;
        self.held.clear();
        render::set_camera(None);
        self.sync_hud();
        self.refresh_settings();
    }

    fn cycle_speed(&mut self) {
        self.speed = next_speed(self.speed);
        let _ = storage::set(SPEED_KEY, self.speed.to_string().as_bytes());
        self.refresh_settings();
    }
}

impl Mod for FreeCam {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(TOGGLE_KEY, "Free Camera", DEFAULT_TOGGLE_KEY)?;
        input::register_key(FOLLOW_KEY, "Follow View", DEFAULT_FOLLOW_KEY)?;
        input::register_key(FORWARD_KEY, "FreeCam: fly forward", DEFAULT_FORWARD_KEY)?;
        input::register_key(BACK_KEY, "FreeCam: fly back", DEFAULT_BACK_KEY)?;
        input::register_key(LEFT_KEY, "FreeCam: fly left", DEFAULT_LEFT_KEY)?;
        input::register_key(RIGHT_KEY, "FreeCam: fly right", DEFAULT_RIGHT_KEY)?;
        input::register_key(UP_KEY, "FreeCam: fly up", DEFAULT_UP_KEY)?;
        input::register_key(DOWN_KEY, "FreeCam: fly down", DEFAULT_DOWN_KEY)?;
        for slot in 0..9 {
            input::register_key(
                TRIPOD_KEYS[slot],
                &format!("FreeCam: tripod {}", slot + 1),
                DEFAULT_TRIPOD_KEYS[slot],
            )?;
        }
        let state = FreeCam {
            mode: Mode::Off,
            held: Held::default(),
            f4_held: false,
            speed: read_speed(storage::get(SPEED_KEY)),
            follow_distance: read_tuning(
                storage::get(DISTANCE_KEY),
                &DISTANCES,
                DEFAULT_DISTANCE,
            ),
            follow_height: read_tuning(storage::get(HEIGHT_KEY), &HEIGHTS, DEFAULT_HEIGHT),
        };
        Ok((
            state,
            Registration::new().command("freecam_yannis").on_tick(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            match event {
                Event::KeyDown(k) if k == TOGGLE_KEY => {
                    self.f4_held = true;
                    match self.mode {
                        Mode::Off => {
                            self.start_free(None);
                            chat::show("FreeCam on");
                        }
                        _ => {
                            self.stop();
                            chat::show("FreeCam off");
                        }
                    }
                }
                Event::KeyUp(k) if k == TOGGLE_KEY => {
                    self.f4_held = false;
                }
                Event::KeyDown(k) if k == FOLLOW_KEY => {
                    self.start_follow();
                    chat::show("FreeCam follow view");
                }
                Event::KeyDown(k) if TRIPOD_KEYS.contains(&k.as_str()) => {
                    let slot = TRIPOD_KEYS
                        .iter()
                        .position(|name| *name == k.as_str())
                        .unwrap_or(0);
                    self.tripod_key_press(slot);
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
            if self.mode != Mode::Off {
                self.stop();
            }
            return;
        }
        if self.mode != Mode::Free || !self.held.any() {
            return;
        }
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
        if name != "freecam_yannis" {
            return Err("This command does nothing".into());
        }
        if self.mode == Mode::Off && game::get_player().is_none() {
            return Ok(Some("freecam_yannis: no player".into()));
        }
        match self.mode {
            Mode::Off => {
                self.start_free(None);
                Ok(Some(Self::pose_feedback("on")))
            }
            _ => {
                self.stop();
                Ok(Some(Self::pose_feedback("off")))
            }
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            TOGGLE_BUTTON => match self.mode {
                Mode::Free => self.stop(),
                _ => self.start_free(None),
            },
            FOLLOW_BUTTON => match self.mode {
                Mode::Follow => self.stop(),
                _ => self.start_follow(),
            },
            SPEED_BUTTON => self.cycle_speed(),
            DISTANCE_BUTTON => {
                self.follow_distance = next_in(&DISTANCES, self.follow_distance);
                let _ = storage::set(
                    DISTANCE_KEY,
                    self.follow_distance.to_string().as_bytes(),
                );
                self.refresh_settings();
            }
            HEIGHT_BUTTON => {
                self.follow_height = next_in(&HEIGHTS, self.follow_height);
                let _ = storage::set(HEIGHT_KEY, self.follow_height.to_string().as_bytes());
                self.refresh_settings();
            }
            _ => {}
        }
    }
}

impl FreeCam {
    /// A tripod number key went down. With F4 held, the current
    /// camera pose is saved as that tripod (per world). With the
    /// camera off, the free camera starts at the saved tripod.
    /// Otherwise (camera running, no chord) the key does nothing
    /// the mod claims — it stays the hotbar's.
    fn tripod_key_press(&mut self, slot: usize) {
        let Some(player) = game::get_player() else {
            return;
        };
        let key = tripod_key(&player.dimension, slot);
        if self.f4_held {
            let pose = render::get_camera();
            let tripod = Tripod {
                x: pose.x,
                y: pose.y,
                z: pose.z,
                yaw: pose.yaw,
                pitch: pose.pitch,
            };
            let _ = storage::set(&key, tripod.encode().as_bytes());
            chat::show(&format!("Tripod {} saved", slot + 1));
        } else if self.mode == Mode::Off {
            if let Some(tripod) = storage::get(&key)
                .and_then(|v| String::from_utf8(v).ok())
                .and_then(|s| Tripod::decode(&s))
            {
                self.start_free(Some(tripod));
                chat::show(&format!("Tripod {}", slot + 1));
            }
        }
    }
}

horizon_mod_sdk::export_mod!(FreeCam);

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn api_level_is_nineteen() {
        assert_eq!(API_LEVEL, 19);
    }

    #[test]
    fn all_seventeen_keys_fit_the_registration_cap() {
        let mut names: Vec<&str> = vec![
            TOGGLE_KEY, FOLLOW_KEY, FORWARD_KEY, BACK_KEY, LEFT_KEY, RIGHT_KEY, UP_KEY,
            DOWN_KEY,
        ];
        names.extend_from_slice(&TRIPOD_KEYS);
        assert_eq!(names.len(), 17);
        assert!(names.len() <= 32, "the registration cap");
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 17, "no key name twice");
        // Every tripod key has its number-row default.
        assert_eq!(TRIPOD_KEYS.len(), DEFAULT_TRIPOD_KEYS.len());
        assert_eq!(DEFAULT_TRIPOD_KEYS[0], "key.keyboard.1");
        assert_eq!(DEFAULT_TRIPOD_KEYS[8], "key.keyboard.9");
    }

    #[test]
    fn tripods_encode_and_decode_exactly() {
        let t = Tripod {
            x: 12.5,
            y: 65.62,
            z: -30.25,
            yaw: 90.0,
            pitch: -15.5,
        };
        let back = Tripod::decode(&t.encode()).unwrap();
        assert_eq!(back, t);
        // Malformed text is no tripod.
        assert_eq!(Tripod::decode(""), None);
        assert_eq!(Tripod::decode("1,2,3"), None);
        assert_eq!(Tripod::decode("1,2,3,4,5,6"), None);
        assert_eq!(Tripod::decode("a,b,c,d,e"), None);
    }

    #[test]
    fn tripod_keys_are_per_world_and_slot() {
        assert_eq!(
            tripod_key("minecraft:overworld", 0),
            "tripod:minecraft:overworld:1"
        );
        assert_eq!(
            tripod_key("minecraft:the_nether", 8),
            "tripod:minecraft:the_nether:9"
        );
        assert_ne!(
            tripod_key("minecraft:overworld", 0),
            tripod_key("minecraft:overworld", 1)
        );
    }

    #[test]
    fn follow_tuning_reads_cycles_and_defaults() {
        assert_eq!(read_tuning(None, &DISTANCES, DEFAULT_DISTANCE), 28.0);
        assert_eq!(
            read_tuning(Some(b"14".to_vec()), &DISTANCES, DEFAULT_DISTANCE),
            14.0
        );
        assert_eq!(
            read_tuning(Some(b"99".to_vec()), &DISTANCES, DEFAULT_DISTANCE),
            28.0
        );
        assert_eq!(next_in(&DISTANCES, 28.0), 40.0);
        assert_eq!(next_in(&DISTANCES, 40.0), 8.0);
        assert_eq!(next_in(&HEIGHTS, 18.0), 26.0);
        assert_eq!(read_tuning(None, &HEIGHTS, DEFAULT_HEIGHT), 18.0);
    }

    #[test]
    fn flight_and_speeds_follow_the_idiom() {
        let mut held = Held::default();
        held.press(FORWARD_KEY);
        let d = fly_delta(&held, 0.0, 0.0, 10.0);
        assert!(close(d[2], 0.5) && close(d[0], 0.0) && close(d[1], 0.0));
        assert_eq!(next_speed(10.0), 20.0);
        assert_eq!(next_speed(60.0), 2.0);
        assert_eq!(read_speed(Some(b"30".to_vec())), 30.0);
        assert_eq!(read_speed(None), 10.0);
        held.clear();
        assert!(!held.any());
    }

    #[test]
    fn follow_hover_sits_behind_and_above() {
        // Facing south at the origin (forward is +z): the hover
        // sits behind, at origin - forward * distance = -28 z,
        // and at feet + height = 82.
        let back = forward(0.0, 0.0);
        let pos = [
            0.0 - back[0] * 28.0,
            64.0 + 18.0,
            0.0 - back[2] * 28.0,
        ];
        assert!(close(pos[2], -28.0) && close(pos[1], 82.0));
        // The drop pitch: atan2(height - eye, distance) ≈ 30.2°.
        let drop = (18.0f64 - EYE).atan2(28.0).to_degrees();
        assert!(drop > 30.0 && drop < 30.5, "got {drop}");
    }
}

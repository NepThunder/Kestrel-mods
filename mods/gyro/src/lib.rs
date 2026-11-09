//! Gyro for Kestrel: the camera half of Gyro — while you glide, the view
//! banks and rolls with your look (`render.set-camera`, API level 14).
//! Y toggles it (rebindable), `/gyro` shows the roll and speed, `/gyro on`
//! and `/gyro off` set it; the game keeps the choice. While it is on and
//! you are gliding, a HUD line shows the roll and speed.
//!
//! Behaviour reimplemented from Gyro 1.4.1 by CGRLabs (MIT,
//! <https://github.com/CGRLabs/gyro>), inspired by the Java mod Do a
//! Barrel Roll; this file shares no code with the original — the original
//! is a native mod, this is a fresh implementation against the Kestrel
//! SDK. The original has no keybinds and no config, and neither does this
//! port beyond the toggle: no settings page, the key is rebindable in the
//! controls as usual. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/gyro>.
//!
//! What drives the roll — the SDK exposes no velocity and no fall-flying
//! flag (`game.get-player` gives feet, yaw, pitch, health, food and
//! dimension), so both drivers are derived from what it does expose:
//!
//! * The look bank (the live driver): how far the camera's yaw has moved
//!   from the player's heading, read back through `render.get-camera`.
//!   While a camera is active the mouse turns the camera, not the player
//!   (docs/mods.md, "Camera"), so moving the mouse banks the view — one
//!   degree of roll per degree of look, up to a full 180° inversion when
//!   you look right round. That is the original's "mouse left / right is
//!   roll", expressed through the look the SDK can see.
//! * The slip bank (the transient driver): the angle between the
//!   horizontal velocity — feet positions across ticks — and the heading,
//!   halved and capped at 30°. Toggle it on mid-turn and the view banks
//!   into the turn while the velocity catches up with the heading, then
//!   settles as the glide straightens.
//!
//! The roll eases toward its target 10° a tick and eases back to level
//! when you stop gliding or toggle off, as the original does. The glide
//! gate is a speed heuristic, because the SDK cannot see the elytra:
//! engage after 3 ticks above 8 m/s horizontal, release after 2 ticks
//! below 5 m/s. A sprint (5.6 m/s) never engages; a fast horse or a boat
//! on ice can, and banks the same way — the toggle is the control, and
//! the gate only stops the mod taking your mouse on foot.
//!
//! The one sharp edge, stated plainly: while the gyro camera is held,
//! the mouse turns the camera, not the player, so your glide holds the
//! heading you had when it engaged — you cannot steer with the mouse
//! until you toggle it off (or land, which releases it). That is the
//! freelook contract the camera API is built on, and it is why this port
//! is camera-only: the flight steering of the original — roll steering
//! the glide — needs a movement/physics capability the SDK does not have
//! (`travel_fall_flying` steers by the player's pitch/yaw with no roll
//! in it). No 6DOF flight steering is attempted here, and none is
//! smuggled in: this mod never writes the player's yaw, pitch or
//! position, and never touches movement input.
use horizon_mod_sdk::{
    Mod, Registration, chat, game,
    hud::{self, Corner, Line},
    input::{self, Event},
    render::{self, CameraOverride},
    storage,
};

/// The newest API level this mod needs, asserted in the tests below:
/// `render.get-camera` / `render.set-camera` are level 14, the rest
/// level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 14;

/// The key's name for `register-key`, and its default: Y. The original
/// has no key, and the mnemonic G is vanilla Quick Actions
/// (`key.quickActions`); Y is free — no vanilla binding and no Kestrel
/// mod uses it.
const KEY: &str = "toggle";
const DEFAULT_KEY: &str = "key.keyboard.y";

/// The storage key for the toggle.
const ENABLED_KEY: &str = "enabled";

/// Engage after this many consecutive ticks at or above [`ENGAGE_SPEED`].
const ENGAGE_TICKS: u32 = 3;
/// Release after this many consecutive ticks below [`RELEASE_SPEED`].
const RELEASE_TICKS: u32 = 2;
/// The horizontal speed, in m/s, that counts as gliding: above a sprint
/// (5.6 m/s), below an ordinary elytra glide.
const ENGAGE_SPEED: f32 = 8.0;
/// The horizontal speed, in m/s, that counts as no longer gliding.
const RELEASE_SPEED: f32 = 5.0;
/// A bigger jump than this in one tick is a teleport, not a velocity:
/// forget the jump instead of banking on it.
const TELEPORT_JUMP: f64 = 32.0;
/// Degrees of roll per degree of look offset from the heading.
const LOOK_GAIN: f32 = 1.0;
/// The slip angle's share of the roll, before the cap.
const SLIP_GAIN: f32 = 0.5;
/// The slip bank never contributes more than this, in degrees.
const SLIP_MAX: f32 = 30.0;
/// The roll never exceeds this, in degrees — the host clamps it too.
const MAX_ROLL: f32 = 180.0;
/// The roll moves toward its target by at most this, in degrees a tick,
/// so it lands exactly and eases instead of snapping.
const EASE_STEP: f32 = 10.0;

/// Wraps degrees to [-180, 180).
fn wrap_degrees(degrees: f32) -> f32 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

/// The yaw whose forward is the horizontal velocity `(vx, vz)`, in
/// degrees: the forward of a yaw is `(-sin, cos)`, so the yaw of a
/// direction is `atan2(-vx, vz)`.
fn velocity_yaw(vx: f64, vz: f64) -> f32 {
    (-vx).atan2(vz).to_degrees() as f32
}

/// The roll from the look: how far the camera's yaw sits from the
/// player's heading, wrapped, at [`LOOK_GAIN`], clamped to a full turn.
fn look_bank(camera_yaw: f32, player_yaw: f32) -> f32 {
    (wrap_degrees(camera_yaw - player_yaw) * LOOK_GAIN).clamp(-MAX_ROLL, MAX_ROLL)
}

/// The roll from the slip: the signed angle from the horizontal velocity
/// to the heading — positive when the heading is to the right of the
/// velocity, a right turn — at [`SLIP_GAIN`], capped at [`SLIP_MAX`].
/// Still air reads 0.
fn slip_bank(vel: [f64; 2], yaw: f32) -> f32 {
    if vel[0].hypot(vel[1]) < 1e-9 {
        return 0.0;
    }
    (wrap_degrees(yaw - velocity_yaw(vel[0], vel[1])) * SLIP_GAIN).clamp(-SLIP_MAX, SLIP_MAX)
}

/// The roll the camera wants this tick, from the look and the slip.
fn roll_target(camera_yaw: f32, player_yaw: f32, vel: [f64; 2]) -> f32 {
    (look_bank(camera_yaw, player_yaw) + slip_bank(vel, player_yaw)).clamp(-MAX_ROLL, MAX_ROLL)
}

/// Moves `current` toward `target` by at most `step`, landing exactly.
fn ease_toward(current: f32, target: f32, step: f32) -> f32 {
    if (target - current).abs() <= step {
        target
    } else if target > current {
        current + step
    } else {
        current - step
    }
}

fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

/// What the camera should do this tick: hold an override with this roll,
/// release the override, or leave it alone.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CameraAction {
    Hold { roll: f32 },
    Release,
    Idle,
}

/// The feet positions across ticks, as a horizontal speed and velocity.
#[derive(Default)]
struct Motion {
    prev: Option<[f64; 3]>,
    /// Horizontal speed this tick, in m/s.
    speed: f32,
    /// Horizontal velocity this tick, in blocks a tick.
    vel: [f64; 2],
    /// Consecutive ticks at or above [`ENGAGE_SPEED`].
    fast_ticks: u32,
    /// Consecutive ticks below [`RELEASE_SPEED`].
    slow_ticks: u32,
}

impl Motion {
    fn reset(&mut self) {
        *self = Motion::default();
    }

    /// Folds one tick's feet position in. A mid-band speed (between the
    /// engage and release speeds) breaks both streaks: only clear
    /// evidence counts either way.
    fn observe(&mut self, feet: [f64; 3]) {
        if let Some(prev) = self.prev {
            let (dx, dy, dz) = (feet[0] - prev[0], feet[1] - prev[1], feet[2] - prev[2]);
            if dx.abs() > TELEPORT_JUMP || dy.abs() > TELEPORT_JUMP || dz.abs() > TELEPORT_JUMP {
                self.speed = 0.0;
                self.vel = [0.0, 0.0];
                self.fast_ticks = 0;
                self.slow_ticks += 1;
            } else {
                self.vel = [dx, dz];
                self.speed = (dx.hypot(dz) * 20.0) as f32;
                if self.speed >= ENGAGE_SPEED {
                    self.fast_ticks += 1;
                    self.slow_ticks = 0;
                } else if self.speed < RELEASE_SPEED {
                    self.slow_ticks += 1;
                    self.fast_ticks = 0;
                } else {
                    self.fast_ticks = 0;
                    self.slow_ticks = 0;
                }
            }
        }
        self.prev = Some(feet);
    }
}

struct Gyro {
    enabled: bool,
    /// Whether this mod currently holds a camera override.
    engaged: bool,
    /// The eased roll now in effect, in degrees.
    roll: f32,
    motion: Motion,
    /// The HUD line currently shown, if any.
    last_panel: Option<String>,
}

impl Gyro {
    /// The camera's next move, from this tick's feet, the player's yaw
    /// and the camera's yaw. It calls no host function, so the tests
    /// drive it without the game: engage on a sustained fast glide,
    /// bank on the look and the slip, ease out to level and release on
    /// toggle-off or on landing.
    fn step(&mut self, feet: [f64; 3], player_yaw: f32, camera_yaw: f32) -> CameraAction {
        self.motion.observe(feet);
        if !self.enabled {
            return self.ease_out();
        }
        if !self.engaged {
            if self.motion.fast_ticks >= ENGAGE_TICKS {
                self.engaged = true;
            } else {
                return CameraAction::Idle;
            }
        }
        if self.motion.slow_ticks >= RELEASE_TICKS {
            return self.ease_out();
        }
        let target = roll_target(camera_yaw, player_yaw, self.motion.vel);
        self.roll = ease_toward(self.roll, target, EASE_STEP);
        CameraAction::Hold { roll: self.roll }
    }

    /// Eases the roll back to level and releases the camera once it is
    /// there; idles once released.
    fn ease_out(&mut self) -> CameraAction {
        if !self.engaged {
            return CameraAction::Idle;
        }
        self.roll = ease_toward(self.roll, 0.0, EASE_STEP);
        if self.roll == 0.0 {
            self.engaged = false;
            CameraAction::Release
        } else {
            CameraAction::Hold { roll: self.roll }
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        let _ = storage::set(ENABLED_KEY, if enabled { b"true" } else { b"false" });
    }

    fn status(&self) -> String {
        if self.enabled {
            format!(
                "Gyro enabled - roll {:+.0}°, {:.0} m/s",
                self.roll, self.motion.speed
            )
        } else {
            "Gyro disabled".to_string()
        }
    }

    /// The HUD line while the camera is held; nothing once it is not.
    /// Set only when the line changes.
    fn sync_panel(&mut self) {
        if self.engaged {
            let line = format!(
                "Gyro: roll {:+.0}°, {:.0} m/s",
                self.roll, self.motion.speed
            );
            if self.last_panel.as_deref() != Some(line.as_str()) {
                hud::set_panel(
                    Corner::TopLeft,
                    &[Line {
                        text: line.clone(),
                        color: 0xFFFFFF,
                    }],
                );
                self.last_panel = Some(line);
            }
        } else if self.last_panel.is_some() {
            hud::clear();
            self.last_panel = None;
        }
    }

    fn clear_panel(&mut self) {
        if self.last_panel.is_some() {
            hud::clear();
            self.last_panel = None;
        }
    }
}

impl Mod for Gyro {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(KEY, "Toggle Gyro", DEFAULT_KEY)?;
        let state = Gyro {
            enabled: read_bool(storage::get(ENABLED_KEY), false),
            engaged: false,
            roll: 0.0,
            motion: Motion::default(),
            last_panel: None,
        };
        Ok((state, Registration::new().command("gyro").on_tick()))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            if let Event::KeyDown(k) = event
                && k == KEY
            {
                let enabled = !self.enabled;
                self.set_enabled(enabled);
                chat::show(&format!("Gyro {}", if enabled { "on" } else { "off" }));
            }
        }
        let Some(p) = game::get_player() else {
            // No world open (title screen): drop the camera, the panel
            // and the motion, and start clean on re-entry. The toggle
            // itself survives — it is the stored choice.
            if self.engaged {
                render::set_camera(None);
            }
            self.engaged = false;
            self.roll = 0.0;
            self.motion.reset();
            self.clear_panel();
            return;
        };
        // Read the camera's look back before re-applying: the mouse
        // turns it between ticks, and feeding the current look back
        // keeps the mouse look alive while only the roll changes.
        let pose = render::get_camera();
        match self.step([p.x, p.y, p.z], p.yaw, pose.yaw) {
            CameraAction::Hold { roll } => {
                render::set_camera(Some(CameraOverride {
                    position: None,
                    yaw: pose.yaw,
                    pitch: pose.pitch,
                    roll,
                }));
            }
            CameraAction::Release => render::set_camera(None),
            CameraAction::Idle => {}
        }
        self.sync_panel();
    }

    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        if name != "gyro" {
            return Err("This command does nothing".into());
        }
        match args.trim() {
            "" => Ok(Some(self.status())),
            "on" => {
                self.set_enabled(true);
                Ok(Some("Gyro enabled".into()))
            }
            "off" => {
                self.set_enabled(false);
                Ok(Some("Gyro disabled".into()))
            }
            other => Err(format!(
                "Unknown option {other}: use /gyro, /gyro on, /gyro off"
            )),
        }
    }
}

horizon_mod_sdk::export_mod!(Gyro);

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn api_level_is_fourteen() {
        // Newest imported functions are `render.get-camera` /
        // `render.set-camera` (level 14); the rest level 0.
        assert_eq!(API_LEVEL, 14);
    }

    #[test]
    fn wrap_degrees_wraps_both_ways() {
        assert_eq!(wrap_degrees(270.0), -90.0);
        assert_eq!(wrap_degrees(-270.0), 90.0);
        assert_eq!(wrap_degrees(180.0), -180.0);
        assert_eq!(wrap_degrees(540.0), -180.0);
        assert_eq!(wrap_degrees(0.0), 0.0);
        assert_eq!(wrap_degrees(45.0), 45.0);
    }

    #[test]
    fn velocity_yaw_matches_the_view_vector() {
        // Forward of a yaw is (-sin, cos): south is 0, west is 90,
        // east is -90, north is 180 (read as -180).
        assert!(close(velocity_yaw(0.0, 1.0), 0.0));
        assert!(close(velocity_yaw(-1.0, 0.0), 90.0));
        assert!(close(velocity_yaw(1.0, 0.0), -90.0));
        // Due north reads -180: atan2 keeps the sign of the zero in -vx.
        assert!(close(velocity_yaw(0.0, -1.0), -180.0));
        assert!(close(velocity_yaw(1.0, 1.0), -45.0));
    }

    #[test]
    fn look_bank_follows_the_offset_and_wraps() {
        assert_eq!(look_bank(45.0, 0.0), 45.0);
        assert_eq!(look_bank(-45.0, 0.0), -45.0);
        // 200° round is 160° the other way: the bank follows the short way.
        assert_eq!(look_bank(200.0, 0.0), -160.0);
        assert_eq!(look_bank(-200.0, 0.0), 160.0);
        // Looking right round inverts the view.
        assert_eq!(look_bank(180.0, 0.0), -180.0);
        // No offset, no bank — whatever the heading is.
        assert_eq!(look_bank(90.0, 90.0), 0.0);
    }

    #[test]
    fn slip_bank_banks_into_the_turn_and_caps() {
        // Facing south, velocity due east: the heading is 90° right of
        // the velocity, a right turn — the bank caps at +30°.
        assert_eq!(slip_bank([1.0, 0.0], 0.0), 30.0);
        // Facing east, velocity due south: the heading is 90° left of
        // the velocity, a left turn.
        assert_eq!(slip_bank([0.0, 1.0], -90.0), -30.0);
        // Velocity along the heading: no slip, no bank.
        assert_eq!(slip_bank([0.0, 1.0], 0.0), 0.0);
        // Still air: no slip, no bank.
        assert_eq!(slip_bank([0.0, 0.0], 0.0), 0.0);
        // A small slip banks by half of it: 20° off, 10° of bank.
        assert!(close(slip_bank([0.3420201433, 0.9396926208], 0.0), 10.0));
    }

    #[test]
    fn ease_toward_steps_and_lands_exactly() {
        assert_eq!(ease_toward(0.0, 30.0, 10.0), 10.0);
        assert_eq!(ease_toward(25.0, 30.0, 10.0), 30.0);
        assert_eq!(ease_toward(30.0, 0.0, 10.0), 20.0);
        assert_eq!(ease_toward(-5.0, 5.0, 10.0), 5.0);
        assert_eq!(ease_toward(30.0, 30.0, 10.0), 30.0);
    }

    fn gliding() -> Gyro {
        Gyro {
            enabled: true,
            engaged: false,
            roll: 0.0,
            motion: Motion::default(),
            last_panel: None,
        }
    }

    /// Feet stepping east 0.5 blocks a tick (10 m/s), facing south.
    fn east_step(g: &mut Gyro, x: f64) -> CameraAction {
        g.step([x, 64.0, 0.5], 0.0, 0.0)
    }

    #[test]
    fn engages_after_three_fast_ticks_then_banks_to_the_cap() {
        let mut g = gliding();
        // The first tick only primes the motion: no velocity yet.
        assert_eq!(east_step(&mut g, 0.5), CameraAction::Idle);
        assert_eq!(east_step(&mut g, 1.0), CameraAction::Idle);
        assert_eq!(east_step(&mut g, 1.5), CameraAction::Idle);
        // Third fast tick: engaged, easing 10° a tick toward +30°.
        assert_eq!(east_step(&mut g, 2.0), CameraAction::Hold { roll: 10.0 });
        assert_eq!(east_step(&mut g, 2.5), CameraAction::Hold { roll: 20.0 });
        assert_eq!(east_step(&mut g, 3.0), CameraAction::Hold { roll: 30.0 });
        assert_eq!(east_step(&mut g, 3.5), CameraAction::Hold { roll: 30.0 });
    }

    #[test]
    fn a_sprint_never_engages() {
        let mut g = gliding();
        // 0.28 blocks a tick is 5.6 m/s, a sprint: below the gate.
        assert_eq!(g.step([0.5, 64.0, 0.5], 0.0, 0.0), CameraAction::Idle);
        for i in 1..6 {
            assert_eq!(
                g.step([0.5 + 0.28 * i as f64, 64.0, 0.5], 0.0, 0.0),
                CameraAction::Idle
            );
        }
    }

    #[test]
    fn landing_levels_the_view_then_releases() {
        let mut g = gliding();
        east_step(&mut g, 0.5);
        east_step(&mut g, 1.0);
        east_step(&mut g, 1.5);
        assert_eq!(east_step(&mut g, 2.0), CameraAction::Hold { roll: 10.0 });
        assert_eq!(east_step(&mut g, 2.5), CameraAction::Hold { roll: 20.0 });
        assert_eq!(east_step(&mut g, 3.0), CameraAction::Hold { roll: 30.0 });
        // Stopped: the first slow tick levels toward 0, the second
        // starts the release, the third lets go.
        assert_eq!(
            g.step([3.0, 64.0, 0.5], 0.0, 0.0),
            CameraAction::Hold { roll: 20.0 }
        );
        assert_eq!(
            g.step([3.0, 64.0, 0.5], 0.0, 0.0),
            CameraAction::Hold { roll: 10.0 }
        );
        assert_eq!(g.step([3.0, 64.0, 0.5], 0.0, 0.0), CameraAction::Release);
        assert_eq!(g.step([3.0, 64.0, 0.5], 0.0, 0.0), CameraAction::Idle);
    }

    #[test]
    fn toggle_off_eases_out_then_releases() {
        let mut g = gliding();
        east_step(&mut g, 0.5);
        east_step(&mut g, 1.0);
        east_step(&mut g, 1.5);
        assert_eq!(east_step(&mut g, 2.0), CameraAction::Hold { roll: 10.0 });
        assert_eq!(east_step(&mut g, 2.5), CameraAction::Hold { roll: 20.0 });
        assert_eq!(east_step(&mut g, 3.0), CameraAction::Hold { roll: 30.0 });
        g.enabled = false;
        assert_eq!(east_step(&mut g, 3.5), CameraAction::Hold { roll: 20.0 });
        assert_eq!(east_step(&mut g, 4.0), CameraAction::Hold { roll: 10.0 });
        assert_eq!(east_step(&mut g, 4.5), CameraAction::Release);
        assert_eq!(east_step(&mut g, 5.0), CameraAction::Idle);
    }

    #[test]
    fn disabled_never_holds_a_camera() {
        let mut g = Gyro {
            enabled: false,
            engaged: false,
            roll: 0.0,
            motion: Motion::default(),
            last_panel: None,
        };
        for i in 0..6 {
            assert_eq!(
                g.step([0.5 + 0.5 * i as f64, 64.0, 0.5], 0.0, 0.0),
                CameraAction::Idle
            );
        }
    }

    #[test]
    fn a_teleport_is_not_a_velocity() {
        let mut g = gliding();
        assert_eq!(g.step([0.5, 64.0, 0.5], 0.0, 0.0), CameraAction::Idle);
        // 40 blocks in a tick: forgotten, not banked on.
        assert_eq!(g.step([40.5, 64.0, 0.5], 0.0, 0.0), CameraAction::Idle);
        assert_eq!(g.motion.speed, 0.0);
        assert_eq!(g.motion.fast_ticks, 0);
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert!(!read_bool(None, false));
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert!(read_bool(Some(b"true".to_vec()), false));
    }
}

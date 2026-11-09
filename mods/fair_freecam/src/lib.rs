//! Fair Freecam for Kestrel: press F4 and fly the camera while
//! the player stays put. WASD moves forward/left/back/right,
//! Space up, Shift down; hold Ctrl for 2.5× speed; the mouse
//! wheel steps the speed through 2 / 5 / 10 / 20 / 40 / 80
//! blocks/s. In single player the camera is blocked by solid
//! blocks (Fair Play mode, on by default): glass, fences and
//! doors block; small plants and flowers do not. A setting
//! labels it allowed on servers.
//!
//! Behaviour reimplemented from Fair Freecam by Lutziano888
//! (MIT, <https://github.com/Lutziano888/fair-freecam>); this
//! file shares no code with the original — the original is a
//! native mod, this is a fresh implementation against the
//! Kestrel SDK. The original's movement keys are the player's
//! own Controls bindings; this port reads them through
//! `input.mapping`, the SDK's window onto those same bindings,
//! so a player who rebinds WASD flies with their own keys. The
//! original's 2.5× Ctrl boost and its wheel-stepped speeds are
//! kept exactly. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/fair_freecam>.
//!
//! Two platform facts shape the port. While a camera override
//! is active the mouse turns the camera, not the player — so
//! the player keeps walking in the direction they faced; the
//! original's implied stillness is what detaching gives on
//! this SDK, no more (WASD still walks the player: the SDK has
//! no way to claim the movement keys — named gap). And the
//! host wall-clamps the camera and caps it at 64 blocks from
//! the player's feet, so no camera in this SDK goes through
//! walls or scouts far. What Fair Play adds on top is the
//! original's own block rule, enforced by the mod from world
//! reads: a move whose destination block is solid does not
//! happen — but the SDK's world reads are single-player only
//! in v1, so on a server the mod cannot check blocks and the
//! host clamp is all that bounds the camera there; the
//! "allowed on servers" setting is kept as the label the
//! original gives it.
use horizon_mod_sdk::{
    Mod, Registration, chat, game,
    input::{self, Event},
    render::{self, CameraOverride, WorldPoint},
    storage, ui,
    world::{self, BlockPos},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `render.get-camera` / `render.set-camera` are level
/// 14, the rest level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 14;

/// The player's eye above their feet, as the game puts the camera.
const EYE: f64 = 1.62;

/// The toggle key's name for `register-key`, and its default:
/// the original's F4. (In Horizon F4 is also vanilla's
/// spectator-shader toggle; the original binds F4 and the
/// fidelity rule for this port keeps it — the game's own
/// mapping on the key still runs, as registered keys do.)
const TOGGLE_KEY: &str = "toggle";
const DEFAULT_TOGGLE_KEY: &str = "key.keyboard.f4";

/// The fly speeds the wheel steps through, in blocks per
/// second, as the original: 2 / 5 / 10 / 20 / 40 / 80, starting
/// at 10.
const SPEEDS: [f32; 6] = [2.0, 5.0, 10.0, 20.0, 40.0, 80.0];
const DEFAULT_SPEED: f32 = 10.0;
/// Holding Ctrl multiplies the speed by this, as the original.
const BOOST: f32 = 2.5;

/// The storage keys: the speed, the served labels and switches.
const SPEED_KEY: &str = "speed";
const FAIR_KEY: &str = "fair_play";
const SERVERS_KEY: &str = "allowed_on_servers";

/// The settings page's buttons.
const TOGGLE_BUTTON: u32 = 0;
const SPEED_BUTTON: u32 = 1;
const FAIR_BUTTON: u32 = 2;
const SERVERS_BUTTON: u32 = 3;

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

/// How far the camera moves this tick: forward/back along the
/// horizontal look (the original flies level, not along the
/// pitch), left/right along the horizontal right, up/down
/// straight. A diagonal moves at the same speed as a straight
/// line; opposite keys cancel. `boost` multiplies the speed.
fn fly_delta(
    fwd: bool,
    back: bool,
    left: bool,
    right_held: bool,
    up: bool,
    down: bool,
    yaw: f32,
    speed: f32,
    boost: bool,
) -> [f64; 3] {
    let flat = forward(yaw, 0.0);
    let f = [flat[0], 0.0, flat[2]];
    let r = right(yaw);
    let along = (fwd as i32 - back as i32) as f64;
    let strafe = (right_held as i32 - left as i32) as f64;
    let vertical = (up as i32 - down as i32) as f64;
    let mut delta = [0.0; 3];
    for i in 0..3 {
        delta[i] = f[i] * along + r[i] * strafe;
    }
    delta[1] += vertical;
    let len = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if len > 0.0 {
        let step = f64::from(speed) * if boost { f64::from(BOOST) } else { 1.0 } / 20.0 / len;
        for d in &mut delta {
            *d *= step;
        }
    }
    delta
}

/// The stored fly speed: the stored number when it is one the
/// wheel offers, else the default 10.
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

/// One wheel step: up a level (clamped at the ends, as the
/// original clamps its speed steps).
fn step_speed(speed: f32, steps: f32) -> f32 {
    let at = SPEEDS.iter().position(|s| *s == speed).unwrap_or(2);
    let next = (at as i32 + steps.signum() as i32).clamp(0, SPEEDS.len() as i32 - 1);
    SPEEDS[next as usize]
}

/// Whether a block state blocks the camera in Fair Play mode:
/// the original's rule — solid blocks block; glass, fences and
/// doors block too; small plants and flowers do not. Read from
/// the state string the world read returns: air and the
/// pass-through plants are named; everything else with a block
/// there blocks. `None` (no chunk loaded, or not single player)
/// does not block: the host's own clamp still bounds the camera.
fn blocks_camera(state: Option<&str>) -> bool {
    let Some(state) = state else {
        return false;
    };
    let id = state.split(['[', ',']).next().unwrap_or(state);
    !matches!(
        id,
        "minecraft:air"
            | "minecraft:cave_air"
            | "minecraft:void_air"
            | "minecraft:short_grass"
            | "minecraft:tall_grass"
            | "minecraft:fern"
            | "minecraft:large_fern"
            | "minecraft:dead_bush"
            | "minecraft:dandelion"
            | "minecraft:poppy"
            | "minecraft:blue_orchid"
            | "minecraft:allium"
            | "minecraft:azure_bluet"
            | "minecraft:red_tulip"
            | "minecraft:orange_tulip"
            | "minecraft:white_tulip"
            | "minecraft:pink_tulip"
            | "minecraft:oxeye_daisy"
            | "minecraft:cornflower"
            | "minecraft:lily_of_the_valley"
            | "minecraft:wither_rose"
            | "minecraft:torchflower"
            | "minecraft:pitcher_plant"
            | "minecraft:pink_petals"
            | "minecraft:sunflower"
            | "minecraft:lilac"
            | "minecraft:rose_bush"
            | "minecraft:peony"
            | "minecraft:water"
            | "minecraft:lava"
    )
}

fn read_flag(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

struct FairFreecam {
    detached: bool,
    speed: f32,
    fair_play: bool,
    allowed_on_servers: bool,
}

impl FairFreecam {
    /// The effective camera pose, one decimal a field, for the
    /// toggle's feedback.
    fn pose_feedback(on: bool) -> String {
        let p = render::get_camera();
        format!(
            "Fair Freecam {} — camera x={:.1} y={:.1} z={:.1} yaw={:.1} pitch={:.1}",
            if on { "on" } else { "off" },
            p.x, p.y, p.z, p.yaw, p.pitch
        )
    }

    /// The settings page: the switch, the speed, Fair Play and
    /// the allowed-on-servers label.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            ui::Widget::Button(ui::ButtonWidget {
                id: TOGGLE_BUTTON,
                label: format!("Fair Freecam: {}", on_off(self.detached)),
            }),
            ui::Widget::Button(ui::ButtonWidget {
                id: SPEED_BUTTON,
                label: format!("Fly speed: {} blocks/s", self.speed),
            }),
            ui::Widget::Button(ui::ButtonWidget {
                id: FAIR_BUTTON,
                label: format!("Fair Play mode: {}", on_off(self.fair_play)),
            }),
            ui::Widget::Button(ui::ButtonWidget {
                id: SERVERS_BUTTON,
                label: format!("Allowed on servers: {}", on_off(self.allowed_on_servers)),
            }),
        ]);
    }

    /// Detach the camera at the player's eye, or release it.
    fn set_detached(&mut self, on: bool) {
        if on {
            let Some(player) = game::get_player() else {
                return;
            };
            self.detached = true;
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
            render::set_camera(None);
        }
        self.refresh_settings();
    }

    fn save_speed(&self) {
        let _ = storage::set(SPEED_KEY, self.speed.to_string().as_bytes());
    }
}

impl Mod for FairFreecam {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(TOGGLE_KEY, "Toggle Fair Freecam", DEFAULT_TOGGLE_KEY)?;
        let state = FairFreecam {
            detached: false,
            speed: read_speed(storage::get(SPEED_KEY)),
            fair_play: read_flag(storage::get(FAIR_KEY), true),
            allowed_on_servers: read_flag(storage::get(SERVERS_KEY), false),
        };
        Ok((
            state,
            Registration::new().command("fair_freecam").on_tick(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            match event {
                Event::KeyDown(k) if k == TOGGLE_KEY => {
                    let on = !self.detached;
                    self.set_detached(on);
                    chat::show(if on {
                        "Fair Freecam on"
                    } else {
                        "Fair Freecam off"
                    });
                }
                Event::Wheel(steps) if self.detached => {
                    self.speed = step_speed(self.speed, steps);
                    self.save_speed();
                }
                _ => {}
            }
        }
        let Some(player) = game::get_player() else {
            if self.detached {
                self.set_detached(false);
            }
            return;
        };
        if !self.detached {
            return;
        }
        // The wheel is ours only while detached.
        input::claim_wheel(true);
        // The player's own movement bindings drive the camera.
        let down = |name: &str| input::mapping(name).is_some_and(|m| m.down);
        let (fwd, back, left, right_h) = (
            down("key.forward"),
            down("key.back"),
            down("key.left"),
            down("key.right"),
        );
        let (up, dn) = (down("key.jump"), down("key.sneak"));
        let boost = down("key.sprint");
        if !(fwd || back || left || right_h || up || dn) {
            // No movement this tick: do not call set-camera at
            // all, so the mouse keeps turning the camera.
            return;
        }
        let pose = render::get_camera();
        let delta = fly_delta(
            fwd, back, left, right_h, up, dn, pose.yaw, self.speed, boost,
        );
        let target = [pose.x + delta[0], pose.y + delta[1], pose.z + delta[2]];
        if self.fair_play {
            // Fair Play: a move into a blocking block does not
            // happen. The destination block only — the host
            // clamps walls along the way.
            let dest = BlockPos {
                x: target[0].floor() as i32,
                y: target[1].floor() as i32,
                z: target[2].floor() as i32,
            };
            let state = world::get_blocks(&player.dimension, &[dest])
                .first()
                .and_then(|r| r.state.clone());
            if blocks_camera(state.as_deref()) {
                return;
            }
        }
        render::set_camera(Some(CameraOverride {
            position: Some(WorldPoint {
                x: target[0],
                y: target[1],
                z: target[2],
            }),
            yaw: pose.yaw,
            pitch: pose.pitch,
            roll: 0.0,
        }));
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "fair_freecam" {
            return Err("This command does nothing".into());
        }
        if !self.detached && game::get_player().is_none() {
            return Ok(Some("fair_freecam: no player".into()));
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
            SPEED_BUTTON => {
                self.speed = step_speed(self.speed, 1.0);
                self.save_speed();
                self.refresh_settings();
            }
            FAIR_BUTTON => {
                self.fair_play = !self.fair_play;
                let _ = storage::set(FAIR_KEY, if self.fair_play { b"true" } else { b"false" });
                self.refresh_settings();
            }
            SERVERS_BUTTON => {
                self.allowed_on_servers = !self.allowed_on_servers;
                let _ = storage::set(
                    SERVERS_KEY,
                    if self.allowed_on_servers {
                        b"true"
                    } else {
                        b"false"
                    },
                );
                self.refresh_settings();
            }
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(FairFreecam);

#[cfg(test)]
mod tests {
    use super::*;

    fn close3(a: [f64; 3], b: [f64; 3]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9 && (a[2] - b[2]).abs() < 1e-9
    }

    #[test]
    fn api_level_is_fourteen() {
        assert_eq!(API_LEVEL, 14);
    }

    #[test]
    fn flies_level_at_the_speed() {
        // Facing south, forward at 10 blocks/s: half a block +z
        // per tick, level (pitch does not tilt the flight).
        let d = fly_delta(true, false, false, false, false, false, 0.0, 10.0, false);
        assert!(close3(d, [0.0, 0.0, 0.5]));
        // Space rises straight up; Shift sinks.
        let d = fly_delta(false, false, false, false, true, false, 0.0, 10.0, false);
        assert!(close3(d, [0.0, 0.5, 0.0]));
        let d = fly_delta(false, false, false, false, false, true, 0.0, 10.0, false);
        assert!(close3(d, [0.0, -0.5, 0.0]));
        // A diagonal is the same speed, not faster.
        let d = fly_delta(true, false, false, true, false, false, 0.0, 10.0, false);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        assert!((len - 0.5).abs() < 1e-9);
        // Opposite keys cancel; nothing held moves nothing.
        let d = fly_delta(true, true, false, false, false, false, 0.0, 10.0, false);
        assert!(close3(d, [0.0, 0.0, 0.0]));
    }

    #[test]
    fn ctrl_boosts_two_and_a_half_times() {
        let plain = fly_delta(true, false, false, false, false, false, 0.0, 10.0, false);
        let boosted = fly_delta(true, false, false, false, false, false, 0.0, 10.0, true);
        assert!(close3(boosted, [plain[0] * 2.5, plain[1] * 2.5, plain[2] * 2.5]));
    }

    #[test]
    fn wheel_steps_the_speeds_and_clamps() {
        assert_eq!(step_speed(10.0, 1.0), 20.0);
        assert_eq!(step_speed(10.0, -1.0), 5.0);
        assert_eq!(step_speed(80.0, 1.0), 80.0);
        assert_eq!(step_speed(2.0, -1.0), 2.0);
        assert_eq!(read_speed(Some(b"40".to_vec())), 40.0);
        assert_eq!(read_speed(Some(b"7".to_vec())), 10.0);
        assert_eq!(read_speed(None), 10.0);
    }

    #[test]
    fn fair_play_block_rule_matches_the_original() {
        // Solid blocks block — and so do glass, fences and doors.
        assert!(blocks_camera(Some("minecraft:stone")));
        assert!(blocks_camera(Some("minecraft:glass")));
        assert!(blocks_camera(Some("minecraft:oak_fence[waterlogged=false]")));
        assert!(blocks_camera(Some("minecraft:oak_door[facing=north]")));
        assert!(blocks_camera(Some("minecraft:dirt")));
        // Small plants and flowers do not; neither does air,
        // water, or an unloaded/unreadable position.
        assert!(!blocks_camera(Some("minecraft:short_grass")));
        assert!(!blocks_camera(Some("minecraft:dandelion")));
        assert!(!blocks_camera(Some("minecraft:poppy")));
        assert!(!blocks_camera(Some("minecraft:air")));
        assert!(!blocks_camera(Some("minecraft:water[level=0]")));
        assert!(!blocks_camera(None));
    }

    #[test]
    fn forward_and_right_match_the_camera_convention() {
        assert!(close3(forward(0.0, 0.0), [0.0, 0.0, 1.0]));
        assert!(close3(forward(90.0, -90.0), [0.0, 1.0, 0.0]));
        assert!(close3(right(0.0), [-1.0, 0.0, 0.0]));
        assert!(close3(right(180.0), [1.0, 0.0, 0.0]));
    }
}

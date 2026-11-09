//! Punchy for Kestrel: the first-person hand lags behind your
//! look on a spring, follows through on your attacks, and bobs
//! with more weight.
//!
//! Behaviour reimplemented from Punchy! by PunchyDevGuy
//! (All Rights Reserved,
//! <https://www.curseforge.com/minecraft/mc-mods/punchy>): the
//! original is a closed-source Java mod and resource pack; this
//! file shares no code or assets with it — the feel is rebuilt
//! against the Kestrel SDK's hand API (API 16, capability
//! `render`): every frame the game hands the mod the hand's state
//! (look, swing, bob), and the mod hands back the one transform
//! the game multiplies into the arm's and the item's poses. The
//! original's resource-pack look and its third-person changes are
//! out of scope here. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/punchy>.
//!
//! The shapes (a spring, a sine follow-through, vanilla's own bob
//! cycle) are the port; the weights were first tuned without a
//! device, then retuned after on-device testing (0.1.1: the bob
//! emphasis had run ten times too weak — the game's bob amount is
//! vanilla's 0..0.1 scale, not 0..1 — and the follow-through was
//! lost inside the vanilla swing).

use horizon_mod_sdk::{
    HandTransform, Mod, Registration, SwingKind, hand, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs: `hand` is level 16
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 16;

/// The setting steps the intensity switch offers, as percents.
const STEPS: [u32; 5] = [0, 25, 50, 75, 100];
const INTENSITY_KEY: &str = "intensity";
const SWAY_KEY: &str = "sway";
const ATTACKS_KEY: &str = "attacks";
const BOB_KEY: &str = "bob";
const INTENSITY_BUTTON: u32 = 1;
const SWAY_BUTTON: u32 = 2;
const ATTACKS_BUTTON: u32 = 3;
const BOB_BUTTON: u32 = 4;

/// The sway spring (per second): stiff enough to follow a flick,
/// damped under 1 so the hand overshoots a touch and settles, the
/// weight the mod exists for.
const SPRING_STIFFNESS: f32 = 170.0;
const SPRING_DAMPING: f32 = 15.0;
/// How much of the lag turns the hand, per axis (the rest is the
/// spring's shape); the sideways slide and roll per degree of lag.
const SWAY_TURN: f32 = 0.6;
const SWAY_SLIDE: f32 = 0.0035;
const SWAY_ROLL: f32 = 0.12;
/// A lag past this (a flick across the whole view) is capped, so
/// the hand never wraps.
const LAG_MAX: f32 = 45.0;

/// A damped spring pulled back to rest: a look turn drags it by
/// the turn's size, then it springs back, overshooting a little.
#[derive(Clone, Copy, Default)]
struct Spring {
    pos: f32,
    vel: f32,
}
impl Spring {
    /// Drag by `impulse` (the view's turn this frame, degrees),
    /// integrate one frame of `dt` seconds, and read the lag.
    fn step(&mut self, impulse: f32, dt: f32) -> f32 {
        self.pos = (self.pos + impulse).clamp(-LAG_MAX, LAG_MAX);
        let accel = -SPRING_STIFFNESS * self.pos - SPRING_DAMPING * self.vel;
        self.vel += accel * dt;
        self.pos += self.vel * dt;
        self.pos
    }
}

/// The follow-through of an attack at swing progress `p` (0..1):
/// nothing at the ends, full at the middle — the hand drives
/// forward and dips as the swing passes its centre.
fn follow_through(p: f32) -> f32 {
    (p.clamp(0.0, 1.0) * std::f32::consts::PI).sin()
}

/// The hand's extra bob for a frame, from the game's own bob
/// cycle — sin and cos of `phase` * pi, as the camera's bob reads
/// them — weighted by `amount`: a sideways sway in blocks, a dip
/// in blocks, and a roll in degrees. Copying the game's formulas
/// keeps the hand's bob in step with the camera's.
///
/// The game's `amount` is vanilla's bob value: it ramps toward the
/// horizontal speed capped at **0.1**, so 0.1 is a full walk or
/// sprint, not 1. It is normalised to 0..1 before weighting —
/// 0.1.0 weighted the raw value and the emphasis ran ten times too
/// weak to see.
fn bob_emphasis(phase: f32, amount: f32) -> (f32, f32, f32) {
    let s = (phase * std::f32::consts::PI).sin();
    let c = (phase * std::f32::consts::PI).cos();
    let a = (amount * 10.0).clamp(0.0, 1.0);
    (s * 0.03 * a, -c.abs() * 0.06 * a, s * 4.0 * a)
}

/// The next intensity step, wrapping from 100 back to 0.
fn next_step(percent: u32) -> u32 {
    match STEPS.iter().position(|&s| s == percent) {
        Some(i) => STEPS[(i + 1) % STEPS.len()],
        None => 50,
    }
}

/// A stored percent, snapped to the steps; `default` when unreadable.
fn read_percent(key: &str, default: u32) -> u32 {
    let Some(bytes) = storage::get(key) else { return default };
    let Ok(text) = std::str::from_utf8(&bytes) else { return default };
    match text.trim().parse::<u32>() {
        Ok(v) if STEPS.contains(&v) => v,
        _ => default,
    }
}

/// A stored switch ("1" / "0"); `default` when unreadable.
fn read_switch(key: &str, default: bool) -> bool {
    match storage::get(key) {
        Some(bytes) => match std::str::from_utf8(&bytes) {
            Ok("1") => true,
            Ok("0") => false,
            _ => default,
        },
        None => default,
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// The identity offset: what the hand wears at rest.
const REST: HandTransform = HandTransform {
    x: 0.0,
    y: 0.0,
    z: 0.0,
    pitch: 0.0,
    yaw: 0.0,
    roll: 0.0,
};

struct Punchy {
    /// The whole effect's strength, as a percent.
    intensity: u32,
    sway: bool,
    attacks: bool,
    bob: bool,
    /// The springs the look drags: yaw and pitch lag, in degrees.
    lag_yaw: Spring,
    lag_pitch: Spring,
    /// An offset is outstanding with the game: `clear-offset` is
    /// only sent while one is, so rest frames stay silent.
    offset_set: bool,
}

impl Punchy {
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: INTENSITY_BUTTON,
                label: format!("Intensity: {}%", self.intensity),
            }),
            Widget::Button(ButtonWidget {
                id: SWAY_BUTTON,
                label: format!("Sway: {}", on_off(self.sway)),
            }),
            Widget::Button(ButtonWidget {
                id: ATTACKS_BUTTON,
                label: format!("Attacks: {}", on_off(self.attacks)),
            }),
            Widget::Button(ButtonWidget {
                id: BOB_BUTTON,
                label: format!("Bob: {}", on_off(self.bob)),
            }),
        ]);
    }

    fn status(&self) -> String {
        format!(
            "Punchy: intensity {}%, sway {}, attacks {}, bob {}",
            self.intensity,
            on_off(self.sway),
            on_off(self.attacks),
            on_off(self.bob)
        )
    }

    /// Take the offset back and rest the springs, so nothing stale
    /// lands when the effect resumes.
    fn release(&mut self) {
        if self.offset_set {
            hand::clear_offset();
            self.offset_set = false;
        }
        self.lag_yaw = Spring::default();
        self.lag_pitch = Spring::default();
    }
}

impl Mod for Punchy {
    fn init() -> Result<(Self, Registration), String> {
        let state = Punchy {
            intensity: read_percent(INTENSITY_KEY, 75),
            sway: read_switch(SWAY_KEY, true),
            attacks: read_switch(ATTACKS_KEY, true),
            bob: read_switch(BOB_KEY, true),
            lag_yaw: Spring::default(),
            lag_pitch: Spring::default(),
            offset_set: false,
        };
        Ok((state, Registration::new().command("punchy").on_frame()))
    }

    fn frame(&mut self, seconds: f32) {
        // A hitched frame must not fling the springs: the step the
        // maths sees is capped at a twentieth of a second.
        let dt = seconds.clamp(0.0, 0.05);
        if self.intensity == 0 {
            self.release();
            return;
        }
        let state = hand::get_state();
        // A two-handed map's pose is its own: the game skips the
        // offset anyway, and the springs rest meanwhile.
        if state.two_handed {
            self.release();
            return;
        }
        let mut t = REST;
        if self.sway {
            let lag_yaw = self.lag_yaw.step(state.look_delta_yaw, dt);
            let lag_pitch = self.lag_pitch.step(state.look_delta_pitch, dt);
            // The hand trails the turn: turning right (yaw up), it
            // is left pointing left of the view and slid left, its
            // top tipped left; looking up (pitch down), it hangs
            // pitched down into the view.
            t.yaw += lag_yaw * SWAY_TURN;
            t.x -= lag_yaw * SWAY_SLIDE;
            t.roll += lag_yaw * SWAY_ROLL;
            t.pitch -= lag_pitch * SWAY_TURN;
        }
        if self.attacks && state.swinging && state.swing_kind == SwingKind::Attack {
            let f = follow_through(state.swing_progress);
            // Forward (view space looks along -z), down, pitched
            // over the swing, and a touch in toward the centre.
            // Weighted to read as a distinct thrust past the vanilla
            // swing: 0.1.0's gentler numbers (0.16 forward, 16
            // degrees) blended into the swing and went unseen on
            // device.
            t.z -= 0.30 * f;
            t.y -= 0.08 * f;
            t.pitch += 28.0 * f;
            t.yaw -= 8.0 * f;
        }
        if self.bob {
            let (x, y, roll) = bob_emphasis(state.bob_phase, state.bob_amount);
            t.x += x;
            t.y += y;
            t.roll += roll;
        }
        let scale = self.intensity as f32 / 100.0;
        // The game's own clamps (±1 block, ±60 degrees) as the last
        // word locally too, so what is set is what is applied.
        let clamp = |v: f32, max: f32| (v * scale).clamp(-max, max);
        t = HandTransform {
            x: clamp(t.x, 1.0),
            y: clamp(t.y, 1.0),
            z: clamp(t.z, 1.0),
            pitch: clamp(t.pitch, 60.0),
            yaw: clamp(t.yaw, 60.0),
            roll: clamp(t.roll, 60.0),
        };
        hand::set_offset(t);
        self.offset_set = true;
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        match name {
            "punchy" => Ok(Some(self.status())),
            _ => Err("This command does nothing".into()),
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            INTENSITY_BUTTON => {
                self.intensity = next_step(self.intensity);
                let _ = storage::set(INTENSITY_KEY, self.intensity.to_string().as_bytes());
            }
            SWAY_BUTTON => {
                self.sway = !self.sway;
                let _ = storage::set(SWAY_KEY, if self.sway { &b"1"[..] } else { &b"0"[..] });
            }
            ATTACKS_BUTTON => {
                self.attacks = !self.attacks;
                let _ = storage::set(ATTACKS_KEY, if self.attacks { &b"1"[..] } else { &b"0"[..] });
            }
            BOB_BUTTON => {
                self.bob = !self.bob;
                let _ = storage::set(BOB_KEY, if self.bob { &b"1"[..] } else { &b"0"[..] });
            }
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(Punchy);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_16() {
        assert_eq!(API_LEVEL, 16);
    }

    #[test]
    fn a_dragged_spring_overshoots_and_settles() {
        let mut spring = Spring::default();
        // A 12-degree flick in one frame, then two seconds of rest.
        let mut lag = spring.step(12.0, 1.0 / 60.0);
        assert!(lag > 11.0, "the drag lands at once: {lag}");
        let mut min = lag;
        for _ in 0..120 {
            lag = spring.step(0.0, 1.0 / 60.0);
            min = min.min(lag);
        }
        assert!(min < 0.0, "damped under 1, it overshoots: min {min}");
        assert!(lag.abs() < 0.05, "and settles to rest: {lag}");
    }

    #[test]
    fn the_spring_caps_a_huge_drag() {
        // A wild flick clamps the drag to LAG_MAX before the spring
        // integrates, so the step's answer sits just inside the cap,
        // on the dragged side — never past it.
        let mut spring = Spring::default();
        let lag = spring.step(720.0, 1.0 / 60.0);
        assert!(lag <= LAG_MAX && lag > LAG_MAX * 0.9, "capped: {lag}");
        let lag = spring.step(-1440.0, 1.0 / 60.0);
        assert!(lag >= -LAG_MAX && lag < -LAG_MAX * 0.9, "capped: {lag}");
    }

    #[test]
    fn follow_through_peaks_mid_swing() {
        assert_eq!(follow_through(0.0), 0.0);
        assert!((follow_through(0.5) - 1.0).abs() < 1e-6);
        assert!(follow_through(1.0).abs() < 1e-6);
        // Symmetric, and clamped outside the swing.
        assert!((follow_through(0.25) - follow_through(0.75)).abs() < 1e-6);
        assert_eq!(follow_through(-1.0), 0.0);
        assert!(follow_through(2.0).abs() < 1e-6);
    }

    #[test]
    fn bob_emphasis_follows_the_games_cycle() {
        // The game's amount is vanilla's 0..0.1 scale: 0.1 is a
        // full walk. Phase 0: the dip is at its deepest, no sway
        // or roll.
        let (x, y, roll) = bob_emphasis(0.0, 0.1);
        assert_eq!((x, roll), (0.0, 0.0));
        assert!((y + 0.06).abs() < 1e-6);
        // A quarter cycle on: full sway and roll, no dip.
        let (x, y, roll) = bob_emphasis(0.5, 0.1);
        assert!((x - 0.03).abs() < 1e-6);
        assert!(y.abs() < 1e-6);
        assert!((roll - 4.0).abs() < 1e-6);
        // Half the walk speed, half the emphasis.
        let (x, _, _) = bob_emphasis(0.5, 0.05);
        assert!((x - 0.015).abs() < 1e-6);
        // No bob amount, no emphasis.
        assert_eq!(bob_emphasis(0.5, 0.0), (0.0, 0.0, 0.0));
    }

    #[test]
    fn steps_cycle_and_wrap() {
        assert_eq!(next_step(0), 25);
        assert_eq!(next_step(75), 100);
        assert_eq!(next_step(100), 0);
        assert_eq!(next_step(33), 50);
    }
}

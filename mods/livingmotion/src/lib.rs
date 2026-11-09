//! Living Motion for Kestrel: third-person players feel alive,
//! on top of the vanilla animations.
//!
//! Behaviour reimplemented from LivingMotion by bart, MIT,
//! <https://github.com/bartekchomik26-dotcom/Living-motion>: this
//! file shares no code with the original — the original is a
//! native `.hnmod` for Horizon, this is a fresh implementation
//! against the Kestrel SDK (`entity-pose`, API 20). This port's
//! home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/livingmotion>.
//!
//! Fidelity, against the original's README (its registry
//! description is one line; the README is the full spec): each
//! player gets a few springs, stepped once a frame from how they
//! move, and the results are added to the finished vanilla pose
//! as per-part offsets — the mod layers on vanilla exactly as the
//! original does, so the pose scale is never touched. Lean into
//! runs, bank into turns, stride bounce, landing squash (and a
//! take-off stretch), the jump pose, a fuller arm swing trailing
//! the lean, idle breathing, a steady head, elytra flight with
//! twirls on `,`/`.`, swimming roll and flutter, item poses,
//! swings that follow through, and Inspect on I. Riding and
//! sleeping fade back to vanilla, as the original says, and the
//! swinging or item-using arm keeps its vanilla motion — the
//! other effects never fight it.
//!
//! What is NOT ported, and why (nothing stands in for any of it):
//! the elytra contrails (fading translucent trails are a particle
//! surface the SDK does not expose yet — they arrive with the
//! particles district, presets and all); climbing poses (the
//! pushed pose state carries no climbing signal); Riptide and
//! dying fades (the state exposes neither — riding and sleeping,
//! the two it does expose, do fade); the strafe half of banking
//! (no velocity direction is pushed, so banking reads the turn
//! from the head-yaw velocity — see `Rig`); landing depth and the
//! jump's rise (no vertical velocity is pushed, so airtime stands
//! in for both); and the first-person hand half (the hand
//! district's domain — the catalog's Punchy is that genre's
//! port). The pose state also does not say which hand is the main
//! hand, so item poses assume the vanilla default: main hand on
//! the right.

use horizon_mod_sdk::{
    Mod, PartOffset, PoseOffset, PoseTarget, Registration,
    entity_pose::{self, ArmPose, EntityPoseState},
    input::{self, Event},
    storage,
    ui::{self, ButtonWidget, TextWidget, Widget},
};
use std::collections::{HashMap, HashSet};

/// The newest API level this mod needs: `entity-pose` is level
/// 20 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 20;

/// The registered keys, the original's bindings: Y toggles the
/// mod, I inspects, `,` and `.` twirl while gliding. (The
/// original changes them on its settings page; a `.kmod`'s keys
/// are rebound in Controls instead — the page names them.)
const TOGGLE_KEY: &str = "toggle";
const DEFAULT_TOGGLE_KEY: &str = "key.keyboard.y";
const INSPECT_KEY: &str = "inspect";
const DEFAULT_INSPECT_KEY: &str = "key.keyboard.i";
const TWIRL_LEFT_KEY: &str = "twirl-left";
const DEFAULT_TWIRL_LEFT_KEY: &str = "key.keyboard.comma";
const TWIRL_RIGHT_KEY: &str = "twirl-right";
const DEFAULT_TWIRL_RIGHT_KEY: &str = "key.keyboard.period";

/// The effects the settings page steps from 0% to 200%, in page
/// order — the original's list, in its README's order.
const FX_LEAN: usize = 0;
const FX_BANK: usize = 1;
const FX_BOUNCE: usize = 2;
const FX_SQUASH: usize = 3;
const FX_JUMP: usize = 4;
const FX_ARM_SWING: usize = 5;
const FX_BREATHING: usize = 6;
const FX_ELYTRA: usize = 7;
const FX_SWIMMING: usize = 8;
const FX_ITEM_POSES: usize = 9;
const FX_SWINGS: usize = 10;
const FX_COUNT: usize = 11;
const FX_NAMES: [&str; FX_COUNT] = [
    "Lean",
    "Bank",
    "Stride Bounce",
    "Landing Squash",
    "Jump Pose",
    "Arm Swing",
    "Idle Breathing",
    "Elytra Flight",
    "Swimming",
    "Item Poses",
    "Swing Follow-through",
];
/// The strength steps the page cycles through, the original's:
/// 0% to 200% by 25%.
const FX_STEPS: [u32; 9] = [0, 25, 50, 75, 100, 125, 150, 175, 200];

/// The settings page's widget ids.
const ENABLED_BUTTON: u32 = 0;
const OTHERS_BUTTON: u32 = 1;
const STEADY_BUTTON: u32 = 2;
const FX_BUTTON_BASE: u32 = 10;
const RESET_BUTTON: u32 = 90;
const KEYS_TEXT: u32 = 200;

/// The storage keys.
const ENABLED_KEY: &str = "enabled";
const OTHERS_KEY: &str = "others";
const STEADY_KEY: &str = "steady_head";

/// Vanilla's own stride frequency: the leg swing reads
/// `cos(walk_pos * 0.6662)`, so every phase in this file does.
const STRIDE: f32 = 0.6662;

/// A damped spring, stepped once a frame — the original's whole
/// rig is a few of these per player. Under-damped where the
/// motion wants overshoot (the lean rocks back when you stop),
/// near-critical where it should just arrive.
#[derive(Clone, Copy, Default)]
struct Spring {
    pos: f32,
    vel: f32,
}

impl Spring {
    fn step(&mut self, target: f32, stiffness: f32, damping: f32, dt: f32) -> f32 {
        let accel = stiffness * (target - self.pos) - damping * self.vel;
        self.vel += accel * dt;
        self.pos += self.vel * dt;
        self.pos
    }

    fn kick(&mut self, impulse: f32) {
        self.vel += impulse;
    }
}

/// The signed smallest turn from `from` to `to`, in (-180, 180].
fn angle_delta(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

/// An angle wrapped into (-180, 180] — a barrel roll passes
/// through the wrap, and the host clamps a part's roll to
/// ±180° anyway.
fn wrap_180(angle: f32) -> f32 {
    angle_delta(0.0, angle)
}

/// A smoothstep of `t` (0..1): flat at both ends.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Is this swing one of a run of swings — the player still
/// swinging shortly after the last swing ended (held attack,
/// mining)? Punchy's window, the house number for the question.
fn chained(elapsed: f32) -> bool {
    elapsed < 0.45
}

/// What an item in use asks of its arm, from the pose code's own
/// arm classification and the item: a bow drawn, a shield up, a
/// trident charged and the rest keep their vanilla arm outright;
/// of what is left, the drinks tip back and everything else is
/// eaten with a chew. The drink set is vanilla's own drinkables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UseKind {
    Eat,
    Drink,
    Vanilla,
}

fn classify_use(arm_pose: ArmPose, item: &str) -> UseKind {
    match arm_pose {
        ArmPose::Bow
        | ArmPose::Block
        | ArmPose::CrossbowHold
        | ArmPose::CrossbowCharge
        | ArmPose::ThrowTrident
        | ArmPose::TootHorn
        | ArmPose::Spear
        | ArmPose::Brush => UseKind::Vanilla,
        ArmPose::Empty | ArmPose::Item => {
            let base = item.rsplit(':').next().unwrap_or(item);
            if matches!(base, "potion" | "milk_bucket" | "honey_bottle") {
                UseKind::Drink
            } else {
                UseKind::Eat
            }
        }
    }
}

/// What a held item asks of the arms while it is just held: a
/// map in both hands with the head down, a compass or clock held
/// up to read, a torch or lantern held out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeldKind {
    Map,
    Read,
    Light,
    None,
}

fn classify_held(item: Option<&str>) -> HeldKind {
    let Some(id) = item else {
        return HeldKind::None;
    };
    match id.rsplit(':').next().unwrap_or(id) {
        "filled_map" | "map" => HeldKind::Map,
        "compass" | "clock" | "recovery_compass" => HeldKind::Read,
        "torch" | "soul_torch" | "lantern" | "soul_lantern" => HeldKind::Light,
        _ => HeldKind::None,
    }
}

/// The settings, as the page and the storage keep them.
#[derive(Clone)]
struct Settings {
    enabled: bool,
    others: bool,
    steady_head: bool,
    /// Each effect's strength, a percent in `FX_STEPS`.
    strength: [u32; FX_COUNT],
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            others: true,
            steady_head: true,
            strength: [100; FX_COUNT],
        }
    }
}

impl Settings {
    /// An effect's strength as a factor (100% is 1.0).
    fn factor(&self, fx: usize) -> f32 {
        self.strength[fx] as f32 / 100.0
    }

    fn save(&self) {
        let put = |key: &str, value: String| {
            let _ = storage::set(key, value.as_bytes());
        };
        put(ENABLED_KEY, on_off_flag(self.enabled).into());
        put(OTHERS_KEY, on_off_flag(self.others).into());
        put(STEADY_KEY, on_off_flag(self.steady_head).into());
        for (i, s) in self.strength.iter().enumerate() {
            put(&fx_key(i), s.to_string());
        }
    }
}

fn fx_key(fx: usize) -> String {
    format!("fx{fx}")
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

fn on_off_flag(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

/// A stored effect strength, snapped to the steps; 100 when
/// unreadable.
fn read_strength(fx: usize) -> u32 {
    let Some(bytes) = storage::get(&fx_key(fx)) else {
        return 100;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return 100;
    };
    match text.trim().parse::<u32>() {
        Ok(v) if FX_STEPS.contains(&v) => v,
        _ => 100,
    }
}

/// The next strength step, wrapping from 200 back to 0.
fn next_strength(percent: u32) -> u32 {
    match FX_STEPS.iter().position(|&s| s == percent) {
        Some(i) => FX_STEPS[(i + 1) % FX_STEPS.len()],
        None => 100,
    }
}

fn enabled_label(enabled: bool) -> String {
    format!("Living Motion: {}", on_off(enabled))
}

fn others_label(others: bool) -> String {
    format!("Other Players: {}", on_off(others))
}

fn steady_label(steady: bool) -> String {
    format!("Steady Head: {}", on_off(steady))
}

fn strength_label(fx: usize, percent: u32) -> String {
    format!("{}: {}%", FX_NAMES[fx], percent)
}

/// One player's rig: the springs and the bookkeeping they are
/// stepped from. The turn a bank answers is read from the head:
/// the pushed state carries the head's yaw relative to the body,
/// not the body's own turning, so a player looking into a turn
/// (and the body following, as it does) is the signal there is.
/// Airtime stands in for the two velocities the state does not
/// push — how long the fall was, for the landing's depth, and
/// the first moments of it, for the jump's rise.
#[derive(Default)]
struct Rig {
    lean: Spring,
    bank: Spring,
    squash: Spring,
    /// The last head yaw seen, for the turn rate.
    prev_head_yaw: Option<f32>,
    /// The smoothed head-yaw velocity, degrees per second.
    yaw_rate: f32,
    /// Seconds since this player left the ground.
    air_time: f32,
    was_on_ground: bool,
    was_swinging: bool,
    /// Seconds since the last swing ended (f32::MAX at rest).
    since_swing: f32,
    /// This rig's own clock, for the breathing and the flutter.
    time: f32,
    /// The twirl's progress, 0..1, while one runs (self only).
    twirl_t: Option<f32>,
    twirl_dir: f32,
    /// Seconds since Inspect was pressed (self only).
    inspect_t: Option<f32>,
}

impl Rig {
    /// Queue a barrel roll in `dir`'s direction (-1 left, +1
    /// right); it runs while the player glides.
    fn start_twirl(&mut self, dir: f32) {
        if self.twirl_t.is_none() {
            self.twirl_t = Some(0.0);
            self.twirl_dir = dir;
        }
    }

    fn start_inspect(&mut self) {
        self.inspect_t = Some(0.0);
    }
}

/// A pose being built: the contributions, merged per part, in
/// the offset's units (blocks, degrees). Positive pitch pitches
/// a part forward/down, positive roll rolls it to its left, a
/// pivot's y down — the model's own axes, as the host folds
/// them in.
#[derive(Default)]
struct PoseBuilder {
    parts: Vec<PartOffset>,
}

impl PoseBuilder {
    fn add(&mut self, part: &str, x: f32, y: f32, z: f32, pitch: f32, yaw: f32, roll: f32) {
        if let Some(p) = self.parts.iter_mut().find(|p| p.part == part) {
            p.x += x;
            p.y += y;
            p.z += z;
            p.pitch += pitch;
            p.yaw += yaw;
            p.roll += roll;
        } else {
            self.parts.push(PartOffset {
                part: part.into(),
                x,
                y,
                z,
                pitch,
                yaw,
                roll,
            });
        }
    }

    fn build(self) -> Option<PoseOffset> {
        let any = self.parts.iter().any(|p| {
            p.x != 0.0
                || p.y != 0.0
                || p.z != 0.0
                || p.pitch != 0.0
                || p.yaw != 0.0
                || p.roll != 0.0
        });
        if !any {
            return None;
        }
        Some(PoseOffset {
            parts: self.parts,
            hidden: Vec::new(),
        })
    }
}

/// Is the arm on `right` the one vanilla owns this frame — the
/// swinging arm or the item-using arm? The other effects never
/// fight it (the original: attacks and item poses keep their
/// vanilla arm).
fn arm_exempt(state: &EntityPoseState, right: bool) -> bool {
    let swinging = state.swing_progress > 0.0 && (state.swing_off_hand != right);
    let using = state.using.as_ref().is_some_and(|u| u.off_hand != right);
    swinging || using
}

/// One player's offset for this frame, or None to leave them
/// vanilla's. Riding and sleeping fade back to vanilla whole —
/// the rig's springs hold still for them and resume after.
fn rig_offset(
    state: &EntityPoseState,
    rig: &mut Rig,
    settings: &Settings,
    dt: f32,
) -> Option<PoseOffset> {
    if state.riding || state.sleeping {
        rig.prev_head_yaw = Some(state.head_yaw);
        return None;
    }
    let dt = dt.clamp(0.0, 0.05);
    rig.time += dt;
    let mut pose = PoseBuilder::default();

    let speed = state.walk_speed.clamp(0.0, 1.0);
    let phase = state.walk_pos * STRIDE;
    let swimming = state.swim_amount > 0.02 && !state.gliding;
    let airborne = !state.on_ground && !state.gliding && !swimming;

    // The turn rate, from the head (see the Rig note).
    if let Some(prev) = rig.prev_head_yaw
        && dt > 0.0
    {
        let raw = angle_delta(prev, state.head_yaw) / dt;
        let blend = 1.0 - (-dt * 7.0).exp();
        rig.yaw_rate += (raw.clamp(-300.0, 300.0) - rig.yaw_rate) * blend;
    }
    rig.prev_head_yaw = Some(state.head_yaw);

    // Lean into runs: forward with speed, more at a sprint, less
    // sneaking; the spring overshoots setting off and rocks back
    // stopping, by its damping.
    let lean_target = if state.gliding {
        0.0
    } else {
        speed
            * 10.0
            * if state.sprinting { 1.35 } else { 1.0 }
            * if state.crouching { 0.45 } else { 1.0 }
            * settings.factor(FX_LEAN)
    };
    let lean = rig.lean.step(lean_target, 110.0, 12.0, dt);

    // Bank into turns: toward the inside of the turn, more the
    // faster you go; gliding banks harder (its own strength).
    let (bank_gain, bank_cap, bank_factor) = if state.gliding {
        (0.09, 26.0, settings.factor(FX_ELYTRA))
    } else {
        (0.05, 16.0, settings.factor(FX_BANK) * (0.3 + 0.7 * speed))
    };
    let bank_target = (-rig.yaw_rate * bank_gain).clamp(-bank_cap, bank_cap) * bank_factor;
    let bank = rig.bank.step(bank_target, 80.0, 11.0, dt);

    // Landing squash and take-off stretch: the ground edges kick
    // the spring, deeper the longer the fall (airtime its measure).
    if state.on_ground {
        if !rig.was_on_ground {
            let depth = (rig.air_time / 1.2).clamp(0.0, 1.0);
            rig.squash.kick(9.0 * depth * settings.factor(FX_SQUASH));
        }
        rig.air_time = 0.0;
    } else {
        if rig.was_on_ground {
            rig.squash.kick(-3.0 * settings.factor(FX_SQUASH));
        }
        rig.air_time += dt;
    }
    rig.was_on_ground = state.on_ground;
    let squash = rig.squash.step(0.0, 130.0, 10.0, dt);

    // The body wears the lean, the bank and the squash.
    pose.add(
        "body",
        0.0,
        squash * 0.07,
        0.0,
        lean + squash * 2.5,
        0.0,
        bank,
    );
    if squash.abs() > 0.01 {
        let bend = -squash * 8.0;
        pose.add("right_leg", 0.0, 0.0, 0.0, bend, 0.0, 0.0);
        pose.add("left_leg", 0.0, 0.0, 0.0, bend, 0.0, 0.0);
    }

    // Stride bounce: the body rises as the legs pass each other
    // and dips at full stride — twice the stride's frequency.
    if state.on_ground && speed > 0.02 && !swimming {
        let bounce = (1.0 - (2.0 * phase).cos()) / 2.0;
        pose.add(
            "body",
            0.0,
            -bounce * 0.03 * speed * settings.factor(FX_BOUNCE),
            0.0,
            0.0,
            0.0,
            0.0,
        );
    }

    // The fuller arm swing, trailing behind the lean. The vanilla
    // arm (swinging or using) is exempt.
    if !swimming && !state.gliding {
        let amp =
            9.0 * speed * if state.sprinting { 1.3 } else { 1.0 } * settings.factor(FX_ARM_SWING);
        let trail = lean * 0.55 * settings.factor(FX_ARM_SWING);
        if !arm_exempt(state, true) {
            pose.add(
                "right_arm",
                0.0,
                0.0,
                0.0,
                phase.cos() * amp + trail,
                0.0,
                0.0,
            );
        }
        if !arm_exempt(state, false) {
            pose.add(
                "left_arm",
                0.0,
                0.0,
                0.0,
                -phase.cos() * amp + trail,
                0.0,
                0.0,
            );
        }
    }

    // The jump pose: arms out to the sides (and up while rising —
    // airtime's first moments stand in for the rise), one knee up.
    if airborne && rig.air_time > 0.03 {
        let f = settings.factor(FX_JUMP);
        let out = (38.0 + if rig.air_time < 0.28 { 22.0 } else { 0.0 }) * f;
        if !arm_exempt(state, true) {
            pose.add("right_arm", 0.0, 0.0, 0.0, -6.0 * f, 0.0, out);
        }
        if !arm_exempt(state, false) {
            pose.add("left_arm", 0.0, 0.0, 0.0, -6.0 * f, 0.0, -out);
        }
        pose.add("right_leg", 0.0, 0.0, 0.0, -32.0 * f, 0.0, 0.0);
        pose.add("left_leg", 0.0, 0.0, 0.0, 9.0 * f, 0.0, 0.0);
    }

    // Idle breathing: a slow arm sway while standing.
    if speed <= 0.02 && state.on_ground && !swimming && !state.gliding {
        let f = settings.factor(FX_BREATHING);
        let sway = (rig.time * 1.7).sin() * 1.4 * f;
        if !arm_exempt(state, true) {
            pose.add("right_arm", 0.0, 0.0, 0.0, 0.0, 0.0, sway);
        }
        if !arm_exempt(state, false) {
            pose.add("left_arm", 0.0, 0.0, 0.0, 0.0, 0.0, -sway);
        }
        let breath = (0.5 + 0.5 * (rig.time * 1.7 - 1.2).sin()) * 0.004 * f;
        pose.add("body", 0.0, -breath, 0.0, 0.0, 0.0, 0.0);
    }

    // Elytra flight: arms spread like wings, swept back into a
    // tuck as you dive, the outside arm of a turn reaching
    // further, legs apart with a slight flutter. The bank above
    // already rolls the body into the turn.
    if state.gliding {
        let f = settings.factor(FX_ELYTRA);
        let diving = state.head_pitch > 25.0;
        let (spread, sweep) = if diving { (34.0, -40.0) } else { (72.0, -6.0) };
        pose.add("right_arm", 0.0, 0.0, 0.0, sweep * f, 0.0, spread * f);
        pose.add("left_arm", 0.0, 0.0, 0.0, sweep * f, 0.0, -spread * f);
        if bank > 3.0 {
            pose.add("left_arm", 0.0, 0.0, 0.0, 0.0, 0.0, -12.0 * f);
        } else if bank < -3.0 {
            pose.add("right_arm", 0.0, 0.0, 0.0, 0.0, 0.0, 12.0 * f);
        }
        let flutter = (rig.time * 12.0).sin() * 2.0 * f;
        pose.add("right_leg", 0.0, 0.0, 0.0, flutter, 0.0, 7.0 * f);
        pose.add("left_leg", 0.0, 0.0, 0.0, -flutter, 0.0, -7.0 * f);
        // The twirl: a full barrel roll, eased, through the wrap.
        if let Some(t) = rig.twirl_t {
            let t = t + dt / 0.9;
            if t >= 1.0 {
                rig.twirl_t = None;
            } else {
                rig.twirl_t = Some(t);
                let angle = rig.twirl_dir * 360.0 * smoothstep(t);
                pose.add("body", 0.0, 0.0, 0.0, 0.0, 0.0, wrap_180(angle) * f);
            }
        }
    } else if rig.twirl_t.is_some() {
        rig.twirl_t = None;
    }

    // Swimming: the body rolls with each stroke, and a flutter
    // kick follows your speed. Vanilla swims the arms and legs;
    // this rides on top, scaled by the stroke's own amount.
    if swimming {
        let f = settings.factor(FX_SWIMMING) * state.swim_amount;
        let stroke = if state.walk_speed > 0.05 {
            phase.sin()
        } else {
            (rig.time * 2.1).sin()
        };
        pose.add("body", 0.0, 0.0, 0.0, 0.0, 0.0, stroke * 5.0 * f);
        let kick = (rig.time * (7.0 + 5.0 * speed)).sin() * 11.0 * f;
        pose.add("right_leg", 0.0, 0.0, 0.0, kick, 0.0, 0.0);
        pose.add("left_leg", 0.0, 0.0, 0.0, -kick, 0.0, 0.0);
    }

    // Swings that follow through: the shoulders turn into the
    // attack and the head stays on target; a run of swings
    // (mining) leans the body in.
    let swinging = state.swing_progress > 0.0;
    // A run's membership is read before the bookkeeping resets
    // it: the gap since the last swing ended, as this one began.
    let in_run = chained(rig.since_swing);
    if swinging {
        rig.was_swinging = true;
        rig.since_swing = 0.0;
    } else {
        if rig.was_swinging {
            rig.was_swinging = false;
            rig.since_swing = 0.0;
        } else {
            rig.since_swing += dt;
        }
    }
    if swinging {
        let f = settings.factor(FX_SWINGS);
        let env = (state.swing_progress * std::f32::consts::PI).sin();
        let dir = if state.swing_off_hand { -1.0 } else { 1.0 };
        pose.add("body", 0.0, 0.0, 0.0, 0.0, -dir * 8.0 * env * f, 0.0);
        pose.add("head", 0.0, 0.0, 0.0, 0.0, dir * 8.0 * env * f, 0.0);
        if in_run {
            pose.add("body", 0.0, 0.0, 0.0, 3.5 * env * f, 0.0, 0.0);
        }
    }

    // Item poses. In use first: eating brings the food to the
    // mouth with a chew, drinking tips the bottle (and the head)
    // back — small additions, since vanilla already raises the
    // arm; the classified vanilla uses are left alone entirely.
    let f_items = settings.factor(FX_ITEM_POSES);
    if let Some(u) = &state.using {
        let arm_pose = if u.off_hand {
            state.off_arm_pose
        } else {
            state.main_arm_pose
        };
        let arm = if u.off_hand { "left_arm" } else { "right_arm" };
        match classify_use(arm_pose, &u.item) {
            UseKind::Eat => {
                pose.add(arm, 0.0, 0.0, 0.0, -10.0 * f_items, 0.0, 0.0);
                let chew = (u.ticks as f32 * 0.85).sin() * 2.2 * f_items;
                pose.add("head", 0.0, 0.0, 0.0, chew, 0.0, 0.0);
            }
            UseKind::Drink => {
                pose.add(arm, 0.0, 0.0, 0.0, -8.0 * f_items, 0.0, 0.0);
                pose.add("head", 0.0, 0.0, 0.0, -7.0 * f_items, 0.0, 0.0);
            }
            UseKind::Vanilla => {}
        }
    } else if !swinging {
        // Just held: a map in both hands with the head down, a
        // compass or clock up to read, a torch or lantern out.
        match classify_held(state.main_hand.as_deref()) {
            HeldKind::Map => {
                pose.add("right_arm", 0.0, 0.0, 0.0, -16.0 * f_items, 0.0, 0.0);
                pose.add("left_arm", 0.0, 0.0, 0.0, -40.0 * f_items, 0.0, 0.0);
                pose.add("head", 0.0, 0.0, 0.0, 12.0 * f_items, 0.0, 0.0);
            }
            HeldKind::Read => {
                pose.add("right_arm", 0.0, 0.0, 0.0, -16.0 * f_items, 0.0, 0.0);
            }
            HeldKind::Light => {
                pose.add(
                    "right_arm",
                    0.0,
                    0.0,
                    0.0,
                    -13.0 * f_items,
                    0.0,
                    6.0 * f_items,
                );
            }
            HeldKind::None => {}
        }
    }

    // Inspect (self only; the key handler arms it): the held
    // item comes up and turns over, then settles back.
    if let Some(t) = rig.inspect_t {
        let t = t + dt;
        if t >= 1.4 {
            rig.inspect_t = None;
        } else {
            rig.inspect_t = Some(t);
            if !swinging && state.using.is_none() {
                let env = ((t / 1.4) * std::f32::consts::PI).sin();
                let turn = (t / 1.4) * std::f32::consts::TAU;
                pose.add(
                    "right_arm",
                    0.0,
                    0.0,
                    0.0,
                    -52.0 * env * f_items,
                    turn.sin() * 22.0 * env,
                    (turn + 1.0).sin() * 14.0 * env,
                );
            }
        }
    }

    // The steady head: level against the body's lean and bank.
    if settings.steady_head {
        pose.add("head", 0.0, 0.0, 0.0, -lean, 0.0, -bank);
    }

    pose.build()
}

struct LivingMotion {
    settings: Settings,
    /// One rig per drawn player, by entity id.
    rigs: HashMap<u32, Rig>,
    /// The targets this mod currently holds an offset on, by
    /// entity id, so a fade or a disappearance clears it once.
    written: HashMap<u32, PoseTarget>,
    /// Inspect was pressed; the frame loop starts it on the
    /// self rig.
    pending_inspect: bool,
    /// A twirl was pressed (its direction); the frame loop
    /// starts it on the self rig.
    pending_twirl: Option<f32>,
}

impl LivingMotion {
    fn refresh_settings(&self) {
        let mut widgets: Vec<Widget> = vec![
            Widget::Button(ButtonWidget {
                id: ENABLED_BUTTON,
                label: enabled_label(self.settings.enabled),
            }),
            Widget::Button(ButtonWidget {
                id: OTHERS_BUTTON,
                label: others_label(self.settings.others),
            }),
            Widget::Button(ButtonWidget {
                id: STEADY_BUTTON,
                label: steady_label(self.settings.steady_head),
            }),
        ];
        for fx in 0..FX_COUNT {
            widgets.push(Widget::Button(ButtonWidget {
                id: FX_BUTTON_BASE + fx as u32,
                label: strength_label(fx, self.settings.strength[fx]),
            }));
        }
        widgets.push(Widget::Button(ButtonWidget {
            id: RESET_BUTTON,
            label: "Reset to Defaults".into(),
        }));
        widgets.push(Widget::Text(TextWidget {
            id: KEYS_TEXT,
            text: "Keys: Y toggles, I inspects, , and . twirl while gliding — rebind them in Controls.".into(),
            color: 0,
        }));
        ui::set_settings(&widgets);
    }

    /// Hand every written target back to vanilla, once each.
    fn clear_all(&mut self) {
        for (_, target) in self.written.drain() {
            entity_pose::clear_pose_offset(target);
        }
    }
}

impl Mod for LivingMotion {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(TOGGLE_KEY, "Toggle Living Motion", DEFAULT_TOGGLE_KEY)?;
        input::register_key(INSPECT_KEY, "Inspect Item", DEFAULT_INSPECT_KEY)?;
        input::register_key(TWIRL_LEFT_KEY, "Elytra Twirl Left", DEFAULT_TWIRL_LEFT_KEY)?;
        input::register_key(
            TWIRL_RIGHT_KEY,
            "Elytra Twirl Right",
            DEFAULT_TWIRL_RIGHT_KEY,
        )?;
        let mut settings = Settings {
            enabled: read_switch(ENABLED_KEY, true),
            others: read_switch(OTHERS_KEY, true),
            steady_head: read_switch(STEADY_KEY, true),
            ..Settings::default()
        };
        for fx in 0..FX_COUNT {
            settings.strength[fx] = read_strength(fx);
        }
        Ok((
            LivingMotion {
                settings,
                rigs: HashMap::new(),
                written: HashMap::new(),
                pending_inspect: false,
                pending_twirl: None,
            },
            Registration::new().on_tick().on_frame(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            let Event::KeyDown(key) = event else {
                continue;
            };
            if key == TOGGLE_KEY {
                self.settings.enabled = !self.settings.enabled;
                self.settings.save();
            } else if self.settings.enabled && key == INSPECT_KEY {
                // Armed here, started by the frame loop on the
                // self rig (the tick does not know its id yet).
                self.pending_inspect = true;
            } else if self.settings.enabled && key == TWIRL_LEFT_KEY {
                self.pending_twirl = Some(-1.0);
            } else if self.settings.enabled && key == TWIRL_RIGHT_KEY {
                self.pending_twirl = Some(1.0);
            }
        }
    }

    fn frame(&mut self, seconds: f32) {
        if !self.settings.enabled {
            self.clear_all();
            self.rigs.clear();
            return;
        }
        let states = entity_pose::get_pose_states();
        let mut seen: HashSet<u32> = HashSet::new();
        for state in &states {
            seen.insert(state.id);
            let target = if state.self_ {
                PoseTarget::SelfPlayer
            } else {
                PoseTarget::Entity(state.id)
            };
            if !state.self_ && !self.settings.others {
                if self.written.remove(&state.id).is_some() {
                    entity_pose::clear_pose_offset(target);
                }
                self.rigs.remove(&state.id);
                continue;
            }
            let rig = self.rigs.entry(state.id).or_default();
            if state.self_ {
                if self.pending_inspect {
                    rig.start_inspect();
                    self.pending_inspect = false;
                }
                if let Some(dir) = self.pending_twirl.take() {
                    rig.start_twirl(dir);
                }
            }
            match rig_offset(state, rig, &self.settings, seconds) {
                Some(offset) => {
                    entity_pose::set_pose_offset(target, &offset);
                    self.written.insert(state.id, target);
                }
                None => {
                    if self.written.remove(&state.id).is_some() {
                        entity_pose::clear_pose_offset(target);
                    }
                }
            }
        }
        // Players who left the drawn set hand their pose back.
        let gone: Vec<u32> = self
            .written
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(target) = self.written.remove(&id) {
                entity_pose::clear_pose_offset(target);
            }
        }
        self.rigs.retain(|id, _| seen.contains(id));
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            ENABLED_BUTTON => self.settings.enabled = !self.settings.enabled,
            OTHERS_BUTTON => self.settings.others = !self.settings.others,
            STEADY_BUTTON => self.settings.steady_head = !self.settings.steady_head,
            RESET_BUTTON => {
                self.settings = Settings::default();
                self.settings.save();
                self.refresh_settings();
                return;
            }
            _ if (FX_BUTTON_BASE..FX_BUTTON_BASE + FX_COUNT as u32).contains(&id) => {
                let fx = (id - FX_BUTTON_BASE) as usize;
                self.settings.strength[fx] = next_strength(self.settings.strength[fx]);
            }
            _ => return,
        }
        self.settings.save();
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(LivingMotion);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::entity_pose::{ItemUse, SwingAnim};

    #[test]
    fn api_level_is_20() {
        assert_eq!(API_LEVEL, 20);
    }

    /// A standing, idle player; tests move the fields they need.
    fn state() -> EntityPoseState {
        EntityPoseState {
            id: 1,
            kind: "minecraft:player".into(),
            self_: true,
            walk_pos: 0.0,
            walk_speed: 0.0,
            head_yaw: 0.0,
            head_pitch: 0.0,
            swing_progress: 0.0,
            swing_anim: SwingAnim::None,
            swing_off_hand: false,
            main_arm_pose: ArmPose::Empty,
            off_arm_pose: ArmPose::Empty,
            main_hand: None,
            off_hand: None,
            using: None,
            crouching: false,
            riding: false,
            swim_amount: 0.0,
            gliding: false,
            sleeping: false,
            sprinting: false,
            on_ground: true,
        }
    }

    fn part(offset: &PoseOffset, name: &str) -> PartOffset {
        offset
            .parts
            .iter()
            .find(|p| p.part == name)
            .map(|p| PartOffset {
                part: p.part.clone(),
                x: p.x,
                y: p.y,
                z: p.z,
                pitch: p.pitch,
                yaw: p.yaw,
                roll: p.roll,
            })
            .unwrap_or_else(|| panic!("no part {name} in {offset:?}"))
    }

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn deltas_and_wraps_take_the_short_way() {
        assert_eq!(angle_delta(350.0, 10.0), 20.0);
        assert_eq!(angle_delta(10.0, 350.0), -20.0);
        assert_eq!(wrap_180(270.0), -90.0);
        assert_eq!(wrap_180(-270.0), 90.0);
        assert_eq!(wrap_180(360.0), 0.0);
        assert_eq!(wrap_180(180.0), 180.0);
    }

    #[test]
    fn the_lean_spring_overshoots_and_settles() {
        // The run lean's own numbers: pulled to 8 it should pass
        // 8 (the set-off overshoot) and come back to it.
        let mut spring = Spring::default();
        let mut peak = 0.0f32;
        for _ in 0..240 {
            peak = peak.max(spring.step(8.0, 110.0, 12.0, DT));
        }
        assert!(peak > 8.0, "no overshoot: peak {peak}");
        assert!((spring.pos - 8.0).abs() < 0.05, "settled at {}", spring.pos);
    }

    #[test]
    fn uses_classify_by_arm_and_item() {
        assert_eq!(
            classify_use(ArmPose::Bow, "minecraft:potion"),
            UseKind::Vanilla
        );
        assert_eq!(
            classify_use(ArmPose::Block, "minecraft:apple"),
            UseKind::Vanilla
        );
        assert_eq!(
            classify_use(ArmPose::Item, "minecraft:potion"),
            UseKind::Drink
        );
        assert_eq!(
            classify_use(ArmPose::Empty, "minecraft:milk_bucket"),
            UseKind::Drink
        );
        assert_eq!(
            classify_use(ArmPose::Item, "minecraft:honey_bottle"),
            UseKind::Drink
        );
        assert_eq!(
            classify_use(ArmPose::Item, "minecraft:cooked_beef"),
            UseKind::Eat
        );
        assert_eq!(classify_use(ArmPose::Empty, "apple"), UseKind::Eat);
    }

    #[test]
    fn held_items_classify() {
        assert_eq!(classify_held(None), HeldKind::None);
        assert_eq!(classify_held(Some("minecraft:filled_map")), HeldKind::Map);
        assert_eq!(classify_held(Some("minecraft:compass")), HeldKind::Read);
        assert_eq!(classify_held(Some("minecraft:clock")), HeldKind::Read);
        assert_eq!(classify_held(Some("minecraft:lantern")), HeldKind::Light);
        assert_eq!(classify_held(Some("minecraft:iron_sword")), HeldKind::None);
    }

    #[test]
    fn strength_steps_wrap_and_labels_name_the_effects() {
        assert_eq!(next_strength(100), 125);
        assert_eq!(next_strength(200), 0);
        assert_eq!(strength_label(FX_LEAN, 125), "Lean: 125%");
        assert_eq!(enabled_label(false), "Living Motion: Off");
        assert_eq!(others_label(true), "Other Players: On");
        assert_eq!(steady_label(true), "Steady Head: On");
    }

    #[test]
    fn the_vanilla_arm_is_exempt() {
        let mut s = state();
        s.swing_progress = 0.5;
        assert!(arm_exempt(&s, true));
        assert!(!arm_exempt(&s, false));
        s.swing_off_hand = true;
        assert!(!arm_exempt(&s, true));
        assert!(arm_exempt(&s, false));
        s.swing_progress = 0.0;
        s.using = Some(ItemUse {
            off_hand: true,
            item: "minecraft:apple".into(),
            ticks: 3,
        });
        assert!(arm_exempt(&s, false));
        assert!(!arm_exempt(&s, true));
    }

    #[test]
    fn riding_and_sleeping_fade_to_vanilla() {
        let settings = Settings::default();
        let mut rig = Rig::default();
        let mut s = state();
        s.walk_speed = 0.8;
        s.riding = true;
        assert!(rig_offset(&s, &mut rig, &settings, DT).is_none());
        s.riding = false;
        s.sleeping = true;
        assert!(rig_offset(&s, &mut rig, &settings, DT).is_none());
    }

    #[test]
    fn walking_leans_the_body_forward() {
        let settings = Settings::default();
        let mut rig = Rig::default();
        let mut s = state();
        s.walk_speed = 0.8;
        s.walk_pos = 2.0;
        let mut out = None;
        for _ in 0..120 {
            out = rig_offset(&s, &mut rig, &settings, DT);
            s.walk_pos += 0.14;
        }
        let body = part(&out.unwrap(), "body");
        // Target lean is 0.8 * 10 = 8 degrees.
        assert!(body.pitch > 5.0 && body.pitch < 11.0, "lean {}", body.pitch);
    }

    #[test]
    fn landing_squashes_down_and_recovers() {
        let settings = Settings::default();
        let mut rig = Rig::default();
        let mut s = state();
        s.on_ground = false;
        // Fall for half a second (the jump pose rides along, but
        // the body only bounces on the landing itself).
        for _ in 0..30 {
            rig_offset(&s, &mut rig, &settings, DT);
        }
        s.on_ground = true;
        let mut dipped = false;
        for _ in 0..20 {
            if let Some(o) = rig_offset(&s, &mut rig, &settings, DT) {
                if let Some(body) = o.parts.iter().find(|p| p.part == "body") {
                    dipped |= body.y > 0.008;
                }
            }
        }
        assert!(dipped, "the body never squashed down on landing");
    }

    #[test]
    fn the_jump_pose_puts_the_arms_out() {
        let settings = Settings::default();
        let mut rig = Rig::default();
        let mut s = state();
        s.on_ground = false;
        let mut out = None;
        for _ in 0..10 {
            out = rig_offset(&s, &mut rig, &settings, DT);
        }
        let arm = part(&out.unwrap(), "right_arm");
        assert!(arm.roll > 40.0, "arm roll {}", arm.roll);
    }

    #[test]
    fn a_swing_turns_the_shoulders_and_the_head_stays() {
        let settings = Settings::default();
        let mut rig = Rig::default();
        let mut s = state();
        s.swing_progress = 0.5;
        s.main_hand = Some("minecraft:iron_sword".into());
        let out = rig_offset(&s, &mut rig, &settings, DT).unwrap();
        let body = part(&out, "body");
        let head = part(&out, "head");
        assert!(body.yaw < -6.0, "body yaw {}", body.yaw);
        assert!(
            (head.yaw + body.yaw).abs() < 0.01,
            "head {} does not counter body {}",
            head.yaw,
            body.yaw
        );
    }

    #[test]
    fn eating_raises_the_arm_and_chews() {
        let settings = Settings::default();
        let mut rig = Rig::default();
        let mut s = state();
        s.main_arm_pose = ArmPose::Item;
        s.main_hand = Some("minecraft:cooked_beef".into());
        s.using = Some(ItemUse {
            off_hand: false,
            item: "minecraft:cooked_beef".into(),
            ticks: 2,
        });
        let out = rig_offset(&s, &mut rig, &settings, DT).unwrap();
        let arm = part(&out, "right_arm");
        assert!(arm.pitch < -8.0, "eat raise {}", arm.pitch);
    }

    #[test]
    fn a_held_map_brings_up_both_hands() {
        let settings = Settings::default();
        let mut rig = Rig::default();
        let mut s = state();
        s.main_hand = Some("minecraft:filled_map".into());
        let out = rig_offset(&s, &mut rig, &settings, DT).unwrap();
        assert!(part(&out, "left_arm").pitch < -30.0);
        assert!(part(&out, "head").pitch > 8.0);
    }

    #[test]
    fn gliding_spreads_the_arms_and_a_twirl_rolls_through() {
        let settings = Settings::default();
        let mut rig = Rig::default();
        let mut s = state();
        s.gliding = true;
        s.on_ground = false;
        let out = rig_offset(&s, &mut rig, &settings, DT).unwrap();
        assert!(part(&out, "right_arm").roll > 50.0);
        rig.start_twirl(1.0);
        let mut rolled_far = false;
        for _ in 0..80 {
            if let Some(o) = rig_offset(&s, &mut rig, &settings, DT) {
                if let Some(body) = o.parts.iter().find(|p| p.part == "body") {
                    rolled_far |= body.roll.abs() > 90.0;
                }
            }
        }
        assert!(rolled_far, "the twirl never rolled past 90 degrees");
        assert!(rig.twirl_t.is_none(), "the twirl finished");
    }

    #[test]
    fn zero_strength_is_vanilla() {
        let mut settings = Settings::default();
        settings.strength = [0; FX_COUNT];
        settings.steady_head = false;
        let mut rig = Rig::default();
        let mut s = state();
        s.walk_speed = 0.9;
        s.sprinting = true;
        let mut any = false;
        for _ in 0..60 {
            any |= rig_offset(&s, &mut rig, &settings, DT).is_some();
            s.walk_pos += 0.2;
        }
        assert!(!any, "at 0% every effect together is no offset at all");
    }
}

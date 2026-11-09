//! Punchy for Kestrel: the first-person hand lags behind your
//! look on a spring, follows through on your attacks, and bobs
//! with more weight.
//!
//! Behaviour reimplemented from Punchy! by PunchyDevGuy
//! (All Rights Reserved,
//! <https://www.curseforge.com/minecraft/mc-mods/punchy>): the
//! original is a closed-source Java mod and resource pack; this
//! file shares no code or assets with it — the feel is rebuilt
//! against the Kestrel SDK's hand API (capability `render`, API
//! 16; the swing terms are API 17): every frame the game hands
//! the mod the hand's state (look, swing, bob), and the mod hands
//! back the one transform the game multiplies into the arm's and
//! the item's poses. The original's resource-pack look and its
//! third-person changes are out of scope here. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/punchy>.
//!
//! The shapes (a spring, a sine follow-through, vanilla's own bob
//! cycle) are the port; the weights were first tuned without a
//! device, then retuned after on-device testing (0.1.1: the bob
//! emphasis had run ten times too weak — the game's bob amount is
//! vanilla's 0..0.1 scale, not 0..1 — and the follow-through was
//! lost inside the vanilla swing).
//!
//! 0.2.0's attacks are authored, not added: on API 17 the mod
//! reads which swing animation is playing and scales the vanilla
//! swing pose itself. A sword's whack is replaced outright (scale
//! 0) by the slash curve below and a spear's stab by the jab —
//! the original's headline per-weapon animations, rebuilt — while
//! any other whack keeps part of the vanilla swing with the
//! 0.1.1 follow-through on top as its weight. Sway and bob still
//! add over the top; the claim replaces only the swing.
//!
//! 0.2.1 retunes from the owner's device pass on 0.2.0: the slash
//! read as a different animation from vanilla, not a heavier one
//! — its wind-up is shorter (0.15 of the swing) and its keyframes
//! sit roughly halfway back toward vanilla's own sweep — while
//! the tools' attacks and the bob read as plain vanilla, so the
//! tools keep more of the vanilla swing under the same bump
//! (half instead of a third) and the bob weights rise by about
//! two fifths. The target everywhere: vanilla's shape, more
//! weight.
//!
//! 0.3.0 goes back to the original's own page and loops, after
//! two feel rounds still missed what the owner saw there. What
//! the page actually promises: sword *combos*, per-tool digging
//! ("a shovel digs, a pickaxe swings forward and an axe chops
//! from the side — each one has a start, a loop and an end"),
//! and held items that "swing and settle". So: every attack
//! recovery now settles with a damped overshoot instead of
//! easing stiffly home; chained sword swings alternate the
//! slash's direction (the combo); pickaxes, axes and shovels
//! each get their own authored stroke at scale 0; and a chained
//! shovel run holds its depth between strokes (the loop). The
//! frame loops on the page show how far the original commits —
//! the shovel is planted overhead between strokes while digging
//! — which is the reference these curves were shaped against.

use horizon_mod_sdk::{
    HandTransform, Mod, Registration, SwingAnim, hand, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs: `hand` is level 16 and
/// its swing terms (`get-swing-anim`, `set-swing-scale`) level 17
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 17;

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

/// A smoothstep of `t` (0..1): flat at both ends — the easing the
/// slash's wind-up and recovery ride.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far an attack pose has settled toward rest, at swing
/// progress `p`: the pose holds in full until 0.55, then travels
/// home along a flat-topped arch — most of the way back by 0.8 —
/// crosses rest at about 0.82, overshoots to about a tenth past
/// it, and lands on rest exactly at 1. This is the "swing and
/// settle" the original mod's page promises: hand-authored
/// keyframes end stiff, and two rounds of on-device testing
/// said so. The overshoot's sign follows the pose it scales (a
/// pose value times a negative settle tips past rest the other
/// way). The arch is |cos| flipped at 0.6 under a squared decay:
/// plain damped cosines either came home too slowly or rang
/// past rest far too hard mid-swing.
fn settle(p: f32) -> f32 {
    let t = ((p - 0.55) / 0.45).clamp(0.0, 1.0);
    if t <= 0.0 {
        return 1.0;
    }
    if t >= 1.0 {
        return 0.0;
    }
    let decay = 1.0 - t;
    let arch = (std::f32::consts::PI * (0.5 + t)).cos().abs();
    let signed = if t < 0.6 { arch } else { -arch };
    decay * decay * signed * (1.0 + 3.0 * t)
}

/// The authored sword slash at swing progress `p` (0..1), in the
/// offset's units (blocks, degrees): the whole animation, worn
/// while the vanilla swing is scaled to nothing. Three phases
/// between two keyframes — the wind-up's peak and the slash's
/// end: the wind-up (p < 0.15) lifts the hand a little up and
/// back, pitched up and yawed in; the slash (0.15..0.55) cuts
/// across — the yaw sweeps out, pitch and roll drop, the hand
/// drives forward — on a fast-in ease, so the cut accelerates;
/// the recovery (0.55..1) settles home with a small overshoot
/// (see `settle`). Zero at both ends. 0.2.1 pulled the keyframes
/// roughly halfway back toward vanilla's sweep and shortened
/// the wind-up: on device, 0.2.0's slash read as a different
/// animation, not a heavier vanilla one.
fn slash_curve(p: f32) -> HandTransform {
    let p = p.clamp(0.0, 1.0);
    // [x, y, z, pitch, yaw, roll]
    const WIND: [f32; 6] = [0.03, 0.035, 0.03, 8.0, -6.0, 0.0];
    const CUT: [f32; 6] = [0.03, -0.025, -0.14, -18.0, 20.0, -12.0];
    let pose: [f32; 6] = if p < 0.15 {
        let t = smoothstep(p / 0.15);
        std::array::from_fn(|i| WIND[i] * t)
    } else if p < 0.55 {
        let t = (p - 0.15) / 0.4;
        let t = t * t;
        std::array::from_fn(|i| WIND[i] + (CUT[i] - WIND[i]) * t)
    } else {
        let t = settle(p);
        std::array::from_fn(|i| CUT[i] * t)
    };
    HandTransform {
        x: pose[0],
        y: pose[1],
        z: pose[2],
        pitch: pose[3],
        yaw: pose[4],
        roll: pose[5],
    }
}

/// The authored spear jab at swing progress `p` (0..1): a straight
/// thrust — forward on a sharpened sine (the follow-through's
/// sine squared, so the peak is brief and the ends linger),
/// pitched a touch down. Zero at both ends.
fn jab_curve(p: f32) -> HandTransform {
    let s = follow_through(p);
    HandTransform {
        z: -0.38 * s * s,
        pitch: -6.0 * s,
        ..REST
    }
}

/// The tool families the attack shapes distinguish, by held item
/// id: the original mod gives each tool its own motion — a
/// pickaxe swings forward, an axe chops from the side, a shovel
/// digs. Anything else (a hoe, a fist, a block) is `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolKind {
    Pickaxe,
    Axe,
    Shovel,
    Other,
}

fn tool_kind(held_item: Option<&str>) -> ToolKind {
    let Some(id) = held_item else {
        return ToolKind::Other;
    };
    if id.ends_with("_pickaxe") {
        ToolKind::Pickaxe
    } else if id.ends_with("_axe") {
        ToolKind::Axe
    } else if id.ends_with("_shovel") {
        ToolKind::Shovel
    } else {
        ToolKind::Other
    }
}

/// An authored tool stroke at swing progress `p` (0..1): a short
/// raise, the stroke itself on a fast-in ease, then a settle home
/// with the shared `settle` overshoot. Zero at both ends except
/// while digging (below). `key` is the stroke's end pose as
/// [x, y, z, pitch, yaw, roll].
fn tool_curve(p: f32, key: [f32; 6]) -> HandTransform {
    let p = p.clamp(0.0, 1.0);
    let pose: [f32; 6] = if p < 0.2 {
        let t = smoothstep(p / 0.2);
        std::array::from_fn(|i| -0.3 * key[i] * t)
    } else if p < 0.55 {
        let t = (p - 0.2) / 0.35;
        let t = t * t;
        std::array::from_fn(|i| key[i] * (-0.3 + 1.3 * t))
    } else {
        let t = settle(p);
        std::array::from_fn(|i| key[i] * t)
    };
    HandTransform {
        x: pose[0],
        y: pose[1],
        z: pose[2],
        pitch: pose[3],
        yaw: pose[4],
        roll: pose[5],
    }
}

/// Is this swing one of a run of swings — the player still
/// swinging shortly after the last swing ended (held attack,
/// mining)? A combo or digging loop flows from it; a lone swing
/// plays on its own.
fn chained(elapsed: f32) -> bool {
    elapsed < 0.45
}

/// A sword, for the slash: the held item's id ends `_sword`, the
/// way the game's own item ids name them.
fn is_sword(held_item: Option<&str>) -> bool {
    held_item.is_some_and(|id| id.ends_with("_sword"))
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
    (s * 0.04 * a, -c.abs() * 0.085 * a, s * 5.5 * a)
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
    /// A swing-scale claim is outstanding with the game (API 17):
    /// this mod holds the vanilla swing below 1.0, and hands it
    /// back (`set-swing-scale(1.0)`) when the swing ends, the
    /// attacks switch off, or the effect rests.
    swing_claimed: bool,
    /// Whether the previous frame sat inside a swing: a swing
    /// that starts within `chained`'s window of the last one's
    /// end continues a run — a sword combo, a digging loop.
    was_swinging: bool,
    /// Seconds since the last swing ended (age::MAX at rest).
    since_swing: f32,
    /// The sword combo step: chained sword swings alternate the
    /// slash's direction, forehand then backhand, like the
    /// original's sword combos.
    combo_mirror: bool,
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

    /// Take the offset back, hand the swing back, and rest the
    /// springs, so nothing stale lands when the effect resumes.
    fn release(&mut self) {
        if self.swing_claimed {
            hand::set_swing_scale(1.0);
            self.swing_claimed = false;
        }
        if self.offset_set {
            hand::clear_offset();
            self.offset_set = false;
        }
        self.lag_yaw = Spring::default();
        self.lag_pitch = Spring::default();
        self.was_swinging = false;
        self.since_swing = f32::MAX;
        self.combo_mirror = false;
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
            swing_claimed: false,
            was_swinging: false,
            since_swing: f32::MAX,
            combo_mirror: false,
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
        // Track the swing run: a swing starting within the
        // chained window of the last one's end continues the run
        // (a sword combo, held digging); a late swing starts over.
        let swinging_now = self.attacks && state.swinging;
        if swinging_now && !self.was_swinging {
            if !chained(self.since_swing) {
                self.combo_mirror = false;
            }
            self.since_swing = 0.0;
        } else if !swinging_now {
            self.since_swing = (self.since_swing + dt).min(3600.0);
        }
        let in_combo = chained(self.since_swing) && self.combo_mirror;
        self.was_swinging = swinging_now;
        if swinging_now {
            // API 17: which animation is playing decides the claim
            // on the vanilla swing and the offset worn over it.
            // The claim itself is not intensity-scaled — a replaced
            // swing is replaced; the authored offsets below scale
            // with the rest at the end of the frame.
            let p = state.swing_progress;
            match hand::get_swing_anim() {
                SwingAnim::Whack if is_sword(state.held_item.as_deref()) => {
                    hand::set_swing_scale(0.0);
                    self.swing_claimed = true;
                    let mut c = slash_curve(p);
                    if in_combo {
                        // The combo's backhand: the sweep mirrors.
                        c.x = -c.x;
                        c.yaw = -c.yaw;
                        c.roll = -c.roll;
                    }
                    if p >= 0.98 {
                        // The swing is spent: the next chained
                        // swing answers with the other hand side.
                        self.combo_mirror = !self.combo_mirror;
                    }
                    t.x += c.x;
                    t.y += c.y;
                    t.z += c.z;
                    t.pitch += c.pitch;
                    t.yaw += c.yaw;
                    t.roll += c.roll;
                }
                SwingAnim::Whack => {
                    let kind = tool_kind(state.held_item.as_deref());
                    if kind == ToolKind::Other {
                        hand::set_swing_scale(0.5);
                        self.swing_claimed = true;
                        let f = follow_through(p);
                        // Forward (view space looks along -z), down,
                        // pitched over the swing, and a touch in
                        // toward the centre: the 0.1.1 follow-
                        // through, the weight on half the vanilla
                        // swing instead of a layer over all of it.
                        t.z -= 0.30 * f;
                        t.y -= 0.08 * f;
                        t.pitch += 28.0 * f;
                        t.yaw -= 8.0 * f;
                    } else {
                        // A tool's own stroke, the original's way:
                        // each family moves differently — a
                        // pickaxe swings forward, an axe chops in
                        // from the side, a shovel scoops down and
                        // forward. The vanilla swing goes off; a
                        // chained swing plays its stroke from a
                        // held depth instead of rest (the digging
                        // loop), a lone one from rest.
                        hand::set_swing_scale(0.0);
                        self.swing_claimed = true;
                        let key: [f32; 6] = match kind {
                            ToolKind::Pickaxe => [0.0, -0.05, -0.18, -34.0, 0.0, 0.0],
                            ToolKind::Axe => [0.10, -0.04, -0.14, -22.0, 18.0, -14.0],
                            ToolKind::Shovel => [0.0, -0.10, -0.24, -26.0, 0.0, 0.0],
                            ToolKind::Other => unreachable!(),
                        };
                        let mut c = tool_curve(p, key);
                        if kind == ToolKind::Shovel && in_combo && p < 0.55 {
                            // Digging flows: between strokes of a
                            // run the shovel stays dug in — its
                            // pose blends toward the stroke's end
                            // pose instead of travelling home.
                            let depth: [f32; 6] =
                                [0.0, -0.05, -0.12, -13.0, 0.0, 0.0];
                            let w = if p < 0.2 {
                                0.6
                            } else {
                                0.6 * (1.0 - (p - 0.2) / 0.35)
                            };
                            let parts = [
                                c.x, c.y, c.z, c.pitch, c.yaw, c.roll,
                            ];
                            let mixed: [f32; 6] = std::array::from_fn(|i| {
                                parts[i] * (1.0 - w) + depth[i] * w
                            });
                            c = HandTransform {
                                x: mixed[0],
                                y: mixed[1],
                                z: mixed[2],
                                pitch: mixed[3],
                                yaw: mixed[4],
                                roll: mixed[5],
                            };
                        }
                        t.x += c.x;
                        t.y += c.y;
                        t.z += c.z;
                        t.pitch += c.pitch;
                        t.yaw += c.yaw;
                        t.roll += c.roll;
                    }
                }
                SwingAnim::Stab => {
                    hand::set_swing_scale(0.0);
                    self.swing_claimed = true;
                    let c = jab_curve(p);
                    t.z += c.z;
                    t.pitch += c.pitch;
                }
                SwingAnim::None => {
                    // Swinging with no animation playing (an item
                    // use, say): hold no claim this frame.
                    if self.swing_claimed {
                        hand::set_swing_scale(1.0);
                        self.swing_claimed = false;
                    }
                }
            }
        } else if self.swing_claimed {
            hand::set_swing_scale(1.0);
            self.swing_claimed = false;
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
    fn api_level_is_17() {
        assert_eq!(API_LEVEL, 17);
    }

    #[test]
    fn slash_curve_rests_at_the_ends_and_cuts_mid_swing() {
        let parts = |t: HandTransform| [t.x, t.y, t.z, t.pitch, t.yaw, t.roll];
        assert_eq!(parts(slash_curve(0.0)), [0.0; 6]);
        assert_eq!(parts(slash_curve(1.0)), [0.0; 6]);
        // Mid wind-up: pitched up, yawed in.
        let wind = slash_curve(0.075);
        assert!(wind.pitch > 0.0 && wind.yaw < 0.0, "wind-up: {wind:?}");
        // The slash's end pose, exactly: yaw swept out, pitch and
        // roll dropped, driven forward.
        let cut = slash_curve(0.55);
        assert!((cut.yaw - 20.0).abs() < 1e-4, "yaw {}", cut.yaw);
        assert!((cut.pitch + 18.0).abs() < 1e-4, "pitch {}", cut.pitch);
        assert!((cut.roll + 12.0).abs() < 1e-4, "roll {}", cut.roll);
        assert!((cut.z + 0.14).abs() < 1e-4, "z {}", cut.z);
        // The recovery settles home: part-way back by p = 0.8,
        // and tipped just past rest near the end (the overshoot).
        let back = slash_curve(0.8);
        assert!(back.yaw > 0.0 && back.yaw < cut.yaw, "recovering: {back:?}");
        assert!(slash_curve(0.9).yaw < 0.0, "overshoot: {:?}", slash_curve(0.9));
    }

    #[test]
    fn settle_holds_then_overshoots_and_lands() {
        assert_eq!(settle(0.3), 1.0);
        assert_eq!(settle(0.55), 1.0);
        assert_eq!(settle(1.0), 0.0);
        // Past rest in the back third: the overshoot.
        assert!(settle(0.85) < 0.0, "overshoot {}", settle(0.85));
        // And nearly home by the very end.
        assert!(settle(0.97).abs() < 0.1, "landing {}", settle(0.97));
    }

    #[test]
    fn tools_have_their_own_strokes() {
        let parts = |t: HandTransform| [t.x, t.y, t.z, t.pitch, t.yaw, t.roll];
        const PICK: [f32; 6] = [0.0, -0.05, -0.18, -34.0, 0.0, 0.0];
        const AXE: [f32; 6] = [0.10, -0.04, -0.14, -22.0, 18.0, -14.0];
        const SHOVEL: [f32; 6] = [0.0, -0.10, -0.24, -26.0, 0.0, 0.0];
        for key in [PICK, AXE, SHOVEL] {
            assert_eq!(parts(tool_curve(0.0, key)), [0.0; 6]);
            assert_eq!(parts(tool_curve(1.0, key)), [0.0; 6]);
            // The stroke's end pose is the keyframe, in full.
            let end = parts(tool_curve(0.55, key));
            for i in 0..6 {
                assert!((end[i] - key[i]).abs() < 1e-4, "key {key:?} part {i}");
            }
        }
        // Forward for the pickaxe, in from the side for the axe,
        // deepest for the shovel.
        assert_eq!(tool_kind(Some("minecraft:diamond_pickaxe")), ToolKind::Pickaxe);
        assert_eq!(tool_kind(Some("minecraft:iron_axe")), ToolKind::Axe);
        assert_eq!(tool_kind(Some("minecraft:wooden_shovel")), ToolKind::Shovel);
        assert_eq!(tool_kind(Some("minecraft:golden_hoe")), ToolKind::Other);
        assert_eq!(tool_kind(None), ToolKind::Other);
    }

    #[test]
    fn chained_swings_continue_a_run() {
        assert!(chained(0.0));
        assert!(chained(0.44));
        assert!(!chained(0.45));
        assert!(!chained(2.0));
    }

    #[test]
    fn jab_curve_peaks_mid_swing() {
        let parts = |t: HandTransform| [t.x, t.y, t.z, t.pitch, t.yaw, t.roll];
        assert_eq!(parts(jab_curve(0.0)), [0.0; 6]);
        // At p = 1 the sine's tail is float dust, not exactly zero
        // (the follow-through test above reads it the same way).
        assert!(
            parts(jab_curve(1.0)).iter().all(|v| v.abs() < 1e-6),
            "rests at the end: {:?}",
            parts(jab_curve(1.0))
        );
        let mid = jab_curve(0.5);
        assert!((mid.z + 0.38).abs() < 1e-6, "thrust {}", mid.z);
        assert!((mid.pitch + 6.0).abs() < 1e-6, "pitch {}", mid.pitch);
        // Sharpened: a quarter in, the thrust is well under what a
        // plain sine would give (-0.27), at -0.19.
        assert!(jab_curve(0.25).z > -0.20, "z {}", jab_curve(0.25).z);
    }

    #[test]
    fn swords_are_the_items_whose_ids_end_so() {
        assert!(is_sword(Some("minecraft:diamond_sword")));
        assert!(is_sword(Some("minecraft:wooden_sword")));
        assert!(!is_sword(Some("minecraft:iron_axe")));
        assert!(!is_sword(Some("minecraft:trident")));
        assert!(!is_sword(None));
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
        assert!((y + 0.085).abs() < 1e-6);
        // A quarter cycle on: full sway and roll, no dip.
        let (x, y, roll) = bob_emphasis(0.5, 0.1);
        assert!((x - 0.04).abs() < 1e-6);
        assert!(y.abs() < 1e-6);
        assert!((roll - 5.5).abs() < 1e-6);
        // Half the walk speed, half the emphasis.
        let (x, _, _) = bob_emphasis(0.5, 0.05);
        assert!((x - 0.02).abs() < 1e-6);
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


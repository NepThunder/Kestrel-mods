//! Blood Particles for Kestrel: when a mob is hit, it bleeds —
//! droplets that are crumbs of a block, a mist that dries
//! darker, and a cloud of spray on a heavy blow.
//!
//! Behaviour reimplemented from Blood Particles 1.2.0 by
//! Bogdan_Tokarev, MIT,
//! <https://github.com/bogdan1tokarev1/Blood-Particles-Rust>:
//! this file shares no code with the original — the original is
//! a native mod for Horizon, this is a fresh implementation
//! against the Kestrel SDK (API 26's `particles` surface under
//! `render`). This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/blood_particles>.
//!
//! Fidelity, against the original: the settings are the
//! original's twelve, with its value cycles and defaults, on
//! the page in Options > Mods and through `/bloodparticles`
//! (and `/bp`), persisted as the original's own one-line
//! encoding under the storage key `config`. The five bloods
//! and their colours, blocks and mob table are the original's,
//! as are the counts: a hit of the reference damage throws the
//! configured numbers, scaled by the damage under the scaling
//! setting, capped at the original's ceilings, and multiplied
//! on the blow that kills. Two readings are the port's, both
//! forced by the surface and stated in the README: the trigger
//! is the health the client is told (the SDK's entity read
//! carries health, not the hurt flash the original watches —
//! and health is single-player data in the SDK's first
//! version, so on a server the mod stays quiet), and a wound's
//! spray is the SDK's burst — a gaussian cloud around the
//! wound — where the original throws each droplet on its own
//! arc away from the player who dealt the blow.

use std::collections::HashMap;

use horizon_mod_sdk::{
    Burst, Mod, ParticleParams, Registration, WorldPoint, chat, entity, game, particles, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `particles.emit` is level 26
/// (`crates/horizon-mods/src/api_level.rs`); the entity,
/// game, storage and ui reads are older.
#[cfg(test)]
const API_LEVEL: u32 = 26;

/// The original's name and version, as its chat lines carry
/// them: `/bloodparticles` answers are prefixed with both.
const NAME: &str = "Blood Particles";
const ORIGINAL_VERSION: &str = "1.2.0";

/// What `/bloodparticles help` says, in one line: every
/// setting of the page and the command, and what kind of
/// value each takes. The original's text.
const HELP: &str = "droplets, mist, clouds, scaling (off/soft/full), death, spread, arc, \
colours (on/off), crumbs (on/off), reach, wounds, particles: /bloodparticles <setting> <value>, \
or reset.";

/// A hit of this much damage (three hearts) throws the
/// configured number of particles; a lighter one throws
/// fewer, a heavier one more.
const REFERENCE_DAMAGE: f32 = 6.0;
/// The least and the most a hit's damage may scale a burst
/// by, however light or heavy it was.
const DAMAGE_MIN_FACTOR: f32 = 0.35;
const DAMAGE_MAX_FACTOR: f32 = 2.5;
/// The blow that kills multiplies every count by this, by
/// default.
const DEATH_MULTIPLIER: f32 = 3.0;
/// A wound throws wider on the blow that kills (the
/// original's death spread factor).
const DEATH_SPREAD: f32 = 1.4;
/// Two bursts for one mob closer together than this are one
/// blow heard twice (the original's dedupe window).
const BLED_WINDOW: u64 = 3;
/// Mobs remembered at once; past it the memory is cleared
/// whole, as the original clears its own.
const REMEMBERED_MAX: usize = 4096;
/// Ceilings nothing can raise a burst past, whatever the
/// settings say.
const DROPLET_CAP: usize = 64;
const MIST_CAP: usize = 48;
const CLOUD_CAP: usize = 12;
/// Where in the body the wound sits: this fraction of the
/// mob's height, the original's wound point.
const WOUND_HEIGHT: f64 = 0.62;

/// The storage key the settings line lives under — the
/// original keeps the same line as its one native setting,
/// under the same name.
const CONFIG_KEY: &str = "config";

// ------------------------------------------------------------------ settings

/// How a hit's damage changes the size of its burst.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scaling {
    /// Every hit throws the same amount.
    Off,
    /// Half as much of the difference.
    Soft,
    /// The whole difference.
    Full,
}

impl Scaling {
    const ALL: [Scaling; 3] = [Scaling::Off, Scaling::Soft, Scaling::Full];

    fn label(self) -> &'static str {
        match self {
            Scaling::Off => "off",
            Scaling::Soft => "soft",
            Scaling::Full => "full",
        }
    }

    fn from_label(text: &str) -> Option<Scaling> {
        Scaling::ALL.into_iter().find(|s| s.label() == text)
    }

    /// How much a hit of `damage` multiplies the counts by
    /// (1.0 for a hit that names no damage).
    fn factor(self, damage: Option<f32>) -> f32 {
        let Some(damage) = damage.filter(|d| d.is_finite() && *d > 0.0) else {
            return 1.0;
        };
        let raw = match self {
            Scaling::Off => return 1.0,
            Scaling::Soft => 1.0 + (damage / REFERENCE_DAMAGE - 1.0) * 0.5,
            Scaling::Full => damage / REFERENCE_DAMAGE,
        };
        raw.clamp(DAMAGE_MIN_FACTOR, DAMAGE_MAX_FACTOR)
    }
}

/// Everything the player can change, from the mod's page in
/// Options > Mods or with `/bloodparticles`. The original's
/// fields, in its order.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Config {
    /// Droplets thrown by a hit of [`REFERENCE_DAMAGE`].
    droplets: usize,
    /// Puffs of mist around it.
    mist: usize,
    /// Clouds of spray around it.
    clouds: usize,
    /// Whether a hit's damage sizes the burst.
    scaling: Scaling,
    /// What the blow that kills multiplies every count by.
    death: f32,
    /// How wide the spray fans out, 0.4 (a jet) to 1.6 (a splash).
    spread: f32,
    /// How much of the spray goes up rather than out.
    arc: f32,
    /// Each mob's own blood colour, or red for all.
    colours: bool,
    /// Droplets as crumbs of a block (on) or mist alone (off).
    crumbs: bool,
    /// Hits farther than this from the player are not drawn.
    reach: f64,
    /// Wounds drawn in one tick at most.
    wounds: usize,
    /// Particles one tick may ask for at most.
    particles: usize,
}

/// The value lists the settings page cycles through, and
/// the defaults.
const DROPLET_STEPS: [usize; 8] = [0, 4, 6, 8, 12, 16, 20, 26];
const MIST_STEPS: [usize; 6] = [0, 2, 4, 6, 8, 12];
const CLOUD_STEPS: [usize; 5] = [0, 1, 2, 3, 4];
const DEATH_STEPS: [f32; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];
const SPREAD_STEPS: [f32; 3] = [0.4, 1.0, 1.6];
const ARC_STEPS: [f32; 3] = [0.3, 0.75, 1.2];
const REACH_STEPS: [f64; 5] = [16.0, 24.0, 32.0, 40.0, 64.0];
const WOUND_STEPS: [usize; 5] = [2, 4, 8, 12, 16];
const PARTICLE_STEPS: [usize; 5] = [32, 64, 112, 160, 256];

/// What the settings page and `/bloodparticles` call the
/// settings.
const KEYS: [&str; 12] = [
    "droplets",
    "mist",
    "clouds",
    "scaling",
    "death",
    "spread",
    "arc",
    "colours",
    "crumbs",
    "reach",
    "wounds",
    "particles",
];

fn step_usize(steps: &[usize], value: usize) -> usize {
    *steps
        .iter()
        .min_by_key(|s| s.abs_diff(value))
        .unwrap_or(&value)
}

fn step_f32(steps: &[f32], value: f32) -> f32 {
    *steps
        .iter()
        .min_by(|a, b| {
            (*a - value)
                .abs()
                .partial_cmp(&(*b - value).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(&value)
}

fn step_f64(steps: &[f64], value: f64) -> f64 {
    *steps
        .iter()
        .min_by(|a, b| {
            (*a - value)
                .abs()
                .partial_cmp(&(*b - value).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(&value)
}

fn next<T: PartialEq + Copy>(steps: &[T], value: T) -> T {
    let at = steps.iter().position(|s| *s == value).unwrap_or(0);
    steps[(at + 1) % steps.len()]
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

/// A number the way the page shows it: whole numbers without
/// a fraction, the rest as they are.
fn trim(value: f32) -> String {
    if value == value.trunc() && value.is_finite() {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn spread_label(value: f32) -> &'static str {
    match value {
        v if v < 0.7 => "jet",
        v if v > 1.3 => "splash",
        _ => "normal",
    }
}

fn arc_label(value: f32) -> &'static str {
    match value {
        v if v < 0.5 => "low",
        v if v > 1.0 => "high",
        _ => "normal",
    }
}

impl Config {
    const DEFAULT: Self = Self {
        droplets: 8,
        mist: 6,
        clouds: 2,
        scaling: Scaling::Full,
        death: DEATH_MULTIPLIER,
        spread: 1.0,
        arc: 0.75,
        colours: true,
        crumbs: true,
        reach: 40.0,
        wounds: 8,
        particles: 112,
    };

    /// The settings as one line, which is what storage keeps
    /// for the mod — the original's own encoding.
    fn encode(&self) -> String {
        format!(
            "droplets={};mist={};clouds={};scaling={};death={};spread={};arc={};colours={};\
             crumbs={};reach={};wounds={};particles={}",
            self.droplets,
            self.mist,
            self.clouds,
            self.scaling.label(),
            self.death,
            self.spread,
            self.arc,
            self.colours,
            self.crumbs,
            self.reach,
            self.wounds,
            self.particles
        )
    }

    /// Reads [`Config::encode`]'s line. Anything missing,
    /// unknown or impossible keeps its default, so a damaged
    /// line still loads.
    fn decode(text: &str) -> Self {
        let mut config = Self::DEFAULT;
        for field in text.split(';') {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "droplets" => {
                    if let Ok(v) = value.parse() {
                        config.droplets = step_usize(&DROPLET_STEPS, v);
                    }
                }
                "mist" => {
                    if let Ok(v) = value.parse() {
                        config.mist = step_usize(&MIST_STEPS, v);
                    }
                }
                "clouds" => {
                    if let Ok(v) = value.parse() {
                        config.clouds = step_usize(&CLOUD_STEPS, v);
                    }
                }
                "scaling" => {
                    if let Some(s) = Scaling::from_label(value) {
                        config.scaling = s;
                    }
                }
                "death" => {
                    if let Ok(v) = value.parse() {
                        config.death = step_f32(&DEATH_STEPS, v);
                    }
                }
                "spread" => {
                    if let Ok(v) = value.parse() {
                        config.spread = step_f32(&SPREAD_STEPS, v);
                    }
                }
                "arc" => {
                    if let Ok(v) = value.parse() {
                        config.arc = step_f32(&ARC_STEPS, v);
                    }
                }
                "colours" => config.colours = value != "false",
                "crumbs" => config.crumbs = value != "false",
                "reach" => {
                    if let Ok(v) = value.parse() {
                        config.reach = step_f64(&REACH_STEPS, v);
                    }
                }
                "wounds" => {
                    if let Ok(v) = value.parse() {
                        config.wounds = step_usize(&WOUND_STEPS, v);
                    }
                }
                "particles" => {
                    if let Ok(v) = value.parse() {
                        config.particles = step_usize(&PARTICLE_STEPS, v);
                    }
                }
                _ => {}
            }
        }
        config
    }

    /// The labels of the settings page, in the order
    /// [`Config::cycle`] takes them. The last is the reset.
    fn labels(&self) -> Vec<String> {
        vec![
            format!("Droplets a hit: {}", self.droplets),
            format!("Mist a hit: {}", self.mist),
            format!("Clouds a hit: {}", self.clouds),
            format!("Damage sizes the burst: {}", self.scaling.label()),
            format!("The killing blow throws: x{}", trim(self.death)),
            format!("Spread: {}", spread_label(self.spread)),
            format!("Arc: {}", arc_label(self.arc)),
            format!("Each mob's own colour: {}", on_off(self.colours)),
            format!("Droplets are block crumbs: {}", on_off(self.crumbs)),
            format!("Drawn within: {} blocks", trim(self.reach as f32)),
            format!("Wounds a tick: {}", self.wounds),
            format!("Particles a tick: {}", self.particles),
            "Reset to defaults".to_string(),
        ]
    }

    /// The settings in three lines, for `/bloodparticles`
    /// with no arguments.
    fn describe(&self) -> Vec<String> {
        let labels = self.labels();
        vec![
            labels[0..4].join(", "),
            labels[4..8].join(", "),
            labels[8..12].join(", "),
        ]
    }

    /// What button `button` of the page does: moves its
    /// setting to its next value, or resets everything on
    /// the last one. Returns the line the page now shows for
    /// it (the reset's own line for the reset).
    fn cycle(&mut self, button: usize) -> String {
        match button {
            0 => self.droplets = next(&DROPLET_STEPS, self.droplets),
            1 => self.mist = next(&MIST_STEPS, self.mist),
            2 => self.clouds = next(&CLOUD_STEPS, self.clouds),
            3 => self.scaling = next(&Scaling::ALL, self.scaling),
            4 => self.death = next(&DEATH_STEPS, self.death),
            5 => self.spread = next(&SPREAD_STEPS, self.spread),
            6 => self.arc = next(&ARC_STEPS, self.arc),
            7 => self.colours = !self.colours,
            8 => self.crumbs = !self.crumbs,
            9 => self.reach = next(&REACH_STEPS, self.reach),
            10 => self.wounds = next(&WOUND_STEPS, self.wounds),
            11 => self.particles = next(&PARTICLE_STEPS, self.particles),
            _ => {
                *self = Self::DEFAULT;
                return "everything back to its defaults".to_string();
            }
        }
        self.labels().get(button).cloned().unwrap_or_default()
    }

    /// Changes one setting by name, the way
    /// `/bloodparticles <key> <value>` does. Values snap to
    /// the nearest one the page can show.
    fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        let number = |text: &str| {
            text.parse::<f64>()
                .map_err(|_| format!("'{text}' is not a number"))
        };
        let yes_no = |text: &str| match text {
            "on" | "true" | "yes" | "1" => Ok(true),
            "off" | "false" | "no" | "0" => Ok(false),
            other => Err(format!("'{other}' is not on or off")),
        };
        match key {
            "droplets" => self.droplets = step_usize(&DROPLET_STEPS, number(value)? as usize),
            "mist" => self.mist = step_usize(&MIST_STEPS, number(value)? as usize),
            "clouds" => self.clouds = step_usize(&CLOUD_STEPS, number(value)? as usize),
            "scaling" => {
                self.scaling = Scaling::from_label(value)
                    .ok_or_else(|| format!("'{value}' is not off, soft or full"))?
            }
            "death" => self.death = step_f32(&DEATH_STEPS, number(value)? as f32),
            "spread" => self.spread = step_f32(&SPREAD_STEPS, number(value)? as f32),
            "arc" => self.arc = step_f32(&ARC_STEPS, number(value)? as f32),
            "colours" | "colors" => self.colours = yes_no(value)?,
            "crumbs" => self.crumbs = yes_no(value)?,
            "reach" => self.reach = step_f64(&REACH_STEPS, number(value)?),
            "wounds" => self.wounds = step_usize(&WOUND_STEPS, number(value)? as usize),
            "particles" => {
                self.particles = step_usize(&PARTICLE_STEPS, number(value)? as usize)
            }
            other => return Err(format!("there is no setting called '{other}'")),
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ bloods

/// A kind of blood a mob can bleed. The five are the
/// original's, with its mob table, colours and blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
enum BloodKind {
    Red,
    Slime,
    Fire,
    Ichor,
    Bone,
}

struct Blood {
    /// The colour of the mist as it leaves the wound.
    mist: [f32; 3],
    /// The colour it dries to.
    mist_to: [f32; 3],
    /// The colour of the clouds: the mist's, brighter.
    cloud: [f32; 3],
    /// The block the droplets are crumbs of, and the brighter
    /// block every fourth droplet is.
    crumbs: (&'static str, &'static str),
}

/// 0xAARRGGBB for a dust colour: the engine's dust particles
/// take their tint from `ParticleParams.color`.
fn argb(rgb: [f32; 3]) -> u32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    0xFF00_0000 | (c(rgb[0]) << 16) | (c(rgb[1]) << 8) | c(rgb[2])
}

impl BloodKind {
    /// What a mob of `kind` bleeds. `kind` is the SDK's
    /// entity kind (`minecraft:zombie` and the like);
    /// anything unlisted bleeds red.
    fn of(kind: &str) -> BloodKind {
        match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "slime" => BloodKind::Slime,
            "blaze" | "magma_cube" | "strider" => BloodKind::Fire,
            "enderman" | "endermite" | "shulker" | "ender_dragon" => BloodKind::Ichor,
            "skeleton" | "stray" | "bogged" | "wither_skeleton" | "skeleton_horse"
            | "zombified_piglin" => BloodKind::Bone,
            _ => BloodKind::Red,
        }
    }

    fn blood(self) -> Blood {
        let (mist, mist_to, crumbs): ([f32; 3], [f32; 3], (&'static str, &'static str)) =
            match self {
                BloodKind::Red => (
                    [0.42, 0.02, 0.02],
                    [0.13, 0.01, 0.01],
                    ("minecraft:nether_wart_block", "minecraft:redstone_block"),
                ),
                BloodKind::Slime => (
                    [0.28, 0.52, 0.16],
                    [0.11, 0.22, 0.05],
                    ("minecraft:slime_block", "minecraft:lime_concrete"),
                ),
                BloodKind::Fire => (
                    [0.86, 0.34, 0.04],
                    [0.32, 0.09, 0.01],
                    ("minecraft:magma_block", "minecraft:orange_concrete"),
                ),
                BloodKind::Ichor => (
                    [0.38, 0.08, 0.52],
                    [0.14, 0.02, 0.20],
                    ("minecraft:purpur_block", "minecraft:purple_concrete"),
                ),
                BloodKind::Bone => (
                    [0.78, 0.76, 0.68],
                    [0.33, 0.31, 0.27],
                    ("minecraft:bone_block", "minecraft:quartz_block"),
                ),
            };
        Blood {
            mist,
            mist_to,
            cloud: [
                (mist[0] * 1.35).min(1.0),
                (mist[1] * 1.35).min(1.0),
                (mist[2] * 1.35).min(1.0),
            ],
            crumbs,
        }
    }
}

// ------------------------------------------------------------------ wounds

/// How many particles one hit throws, before the tick's caps
/// are applied: the configured counts by the damage factor
/// and, on the blow that kills, by the death multiplier,
/// under the original's ceilings.
struct WoundCounts {
    droplets: usize,
    mist: usize,
    clouds: usize,
}

fn wound_counts(config: &Config, damage: Option<f32>, death: bool) -> WoundCounts {
    // The blow that kills is sized by the multiplier alone:
    // the original seeds a death wound's remembered health
    // as 0, so no damage is ever read for it.
    let factor = if death {
        1.0
    } else {
        config.scaling.factor(damage)
    };
    let multiplier = factor as f64 * if death { config.death as f64 } else { 1.0 };
    WoundCounts {
        droplets: ((config.droplets as f64 * multiplier).round() as usize).min(DROPLET_CAP),
        mist: ((config.mist as f64 * multiplier).round() as usize).min(MIST_CAP),
        clouds: ((config.clouds as f64 * multiplier).round() as usize).min(CLOUD_CAP),
    }
}

/// One planned wound: everything the emitter needs.
struct Wound {
    /// The mob's kind, for the blood.
    kind: String,
    /// The wound point: feet + [`WOUND_HEIGHT`] of the height.
    at: [f64; 3],
    counts: WoundCounts,
    death: bool,
}

/// The bursts for one wound, in emission order: the dark
/// crumbs, every fourth droplet's bright block, the mist,
/// the cloud. The burst shape is the SDK's: a gaussian cloud
/// around the wound, its width and speed scaled by the
/// spread setting (and [`DEATH_SPREAD`] on the killing
/// blow), its height by the arc setting. The original throws
/// each droplet on its own directed arc; the SDK's burst is
/// what this one surface offers, so the spray is a cloud
/// around the wound rather than a thrown jet — the counts,
/// colours, blocks and scales are the original's.
fn plan_bursts(config: &Config, wound: &Wound) -> Vec<Burst> {
    let blood = BloodKind::of(&wound.kind).blood();
    let spread = config.spread as f64 * if wound.death { DEATH_SPREAD as f64 } else { 1.0 };
    let arc = config.arc as f64;
    let at = WorldPoint {
        x: wound.at[0],
        y: wound.at[1],
        z: wound.at[2],
    };
    let burst = |particle: &str, count: usize, spread_xyz: [f64; 3], speed: f64, params| Burst {
        particle: particle.to_string(),
        params,
        at,
        spread: WorldPoint {
            x: spread_xyz[0],
            y: spread_xyz[1],
            z: spread_xyz[2],
        },
        speed,
        count: count as u32,
    };
    let plain = || ParticleParams {
        color: None,
        to_color: None,
        scale: None,
        block: None,
        target: None,
        duration: None,
    };
    let mut out = Vec::new();
    if config.crumbs && wound.counts.droplets > 0 {
        let bright = wound.counts.droplets / 4;
        let dark = wound.counts.droplets - bright;
        let crumb_params = |block: &'static str| {
            let mut p = plain();
            p.block = Some(block.to_string());
            p
        };
        if dark > 0 {
            out.push(burst(
                "minecraft:block",
                dark,
                [0.15 * spread, 0.12 * arc, 0.15 * spread],
                0.22 * spread,
                crumb_params(blood.crumbs.0),
            ));
        }
        if bright > 0 {
            out.push(burst(
                "minecraft:block",
                bright,
                [0.15 * spread, 0.12 * arc, 0.15 * spread],
                0.22 * spread,
                crumb_params(blood.crumbs.1),
            ));
        }
    }
    if wound.counts.mist > 0 {
        let mut p = plain();
        p.color = Some(argb(blood.mist));
        p.to_color = Some(argb(blood.mist_to));
        p.scale = Some(0.85);
        out.push(burst(
            "minecraft:dust_color_transition",
            wound.counts.mist,
            [0.12 * spread, 0.10 * arc, 0.12 * spread],
            0.06 * spread,
            p,
        ));
    }
    if wound.counts.clouds > 0 {
        let mut p = plain();
        p.color = Some(argb(blood.cloud));
        p.scale = Some(1.6);
        out.push(burst(
            "minecraft:dust",
            wound.counts.clouds,
            [0.08 * spread, 0.06 * arc, 0.08 * spread],
            0.045 * spread,
            p,
        ));
    }
    out
}

/// Positions the host will not draw (past the edge of the
/// world) get no spray — the original's `drawable` check.
fn drawable(at: [f64; 3]) -> bool {
    at.iter().all(|v| v.is_finite())
        && at[0].abs() <= 3.0e7
        && at[2].abs() <= 3.0e7
        && at[1].abs() <= 1.0e6
}

// ------------------------------------------------------------------ the mod

/// One tick's fresh wounds, in the order `entity.entities`
/// reports mobs (nearest first).
fn detect_wounds(
    config: &Config,
    prev: &mut HashMap<u32, f32>,
    last_bled: &mut HashMap<u32, u64>,
    tick: u64,
    player: [f64; 3],
) -> Vec<Wound> {
    let mut wounds = Vec::new();
    for e in entity::entities() {
        let Some(health) = e.health else { continue };
        if !health.is_finite() || e.height < 0.0 || e.height > 512.0 {
            continue;
        }
        let at = [e.x, e.y + WOUND_HEIGHT * e.height as f64, e.z];
        let dx = e.x - player[0];
        let dy = e.y - player[1];
        let dz = e.z - player[2];
        if (dx * dx + dy * dy + dz * dz).sqrt() > config.reach {
            continue; // out of reach: never tracked, as the original tracks
        }
        let previous = prev.insert(e.id, health);
        let Some(before) = previous else { continue };
        if health >= before {
            continue; // a heal, or nothing: no wound either way
        }
        if let Some(&bled) = last_bled.get(&e.id)
            && tick.wrapping_sub(bled) < BLED_WINDOW
        {
            continue; // one blow heard twice: swallowed
        }
        if !drawable(at) {
            continue;
        }
        last_bled.insert(e.id, tick);
        let death = health <= 0.0;
        let damage = Some(before - health);
        let kind = if config.colours {
            e.kind.clone()
        } else {
            String::new() // falls to Red in BloodKind::of, as the original's does
        };
        wounds.push(Wound {
            kind,
            at,
            counts: wound_counts(config, damage, death),
            death,
        });
        if prev.len() > REMEMBERED_MAX {
            prev.clear();
        }
    }
    wounds
}

struct BloodParticles {
    config: Config,
    /// The last health seen per mob id: the wound watch.
    prev_health: HashMap<u32, f32>,
    /// The tick each mob last bled on: the dedupe window.
    last_bled: HashMap<u32, u64>,
}

impl BloodParticles {
    fn persist(&self) {
        let _ = storage::set(CONFIG_KEY, self.config.encode().as_bytes());
    }

    fn say(&self, line: &str) {
        let _ = chat::show(&format!("{NAME} {ORIGINAL_VERSION}: {line}"));
    }
}

impl Mod for BloodParticles {
    fn init() -> Result<(Self, Registration), String> {
        let config = storage::get(CONFIG_KEY)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|text| Config::decode(&text))
            .unwrap_or(Config::DEFAULT);
        Ok((
            Self {
                config,
                prev_health: HashMap::new(),
                last_bled: HashMap::new(),
            },
            Registration::new()
                .on_tick()
                .command("bloodparticles")
                .command("bp"),
        ))
    }

    fn tick(&mut self, tick: u64) {
        let Some(player) = game::get_player() else {
            return; // off the title screen there is nothing to bleed
        };
        let player_at = [player.x, player.y, player.z];
        let wounds = detect_wounds(
            &self.config,
            &mut self.prev_health,
            &mut self.last_bled,
            tick,
            player_at,
        );
        let mut budget = self.config.particles;
        for wound in wounds.iter().take(self.config.wounds.max(1)) {
            if budget == 0 {
                break;
            }
            for mut burst in plan_bursts(&self.config, wound) {
                if budget == 0 {
                    break;
                }
                if burst.count as usize > budget {
                    burst.count = budget as u32;
                }
                if burst.count == 0 {
                    continue;
                }
                budget -= burst.count as usize;
                particles::emit(&burst);
            }
        }
    }

    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        let words: Vec<&str> = args.split_whitespace().collect();
        match words.as_slice() {
            [] => {
                for line in self.config.describe() {
                    self.say(&line);
                }
                self.say("/bloodparticles help lists the settings.");
            }
            ["help"] => self.say(HELP),
            ["reset"] => {
                self.config = Config::DEFAULT;
                self.persist();
                self.say("everything back to its defaults.");
            }
            [key, value] => match self.config.set(key, value) {
                Ok(()) => {
                    self.persist();
                    let at = KEYS
                        .iter()
                        .position(|k| *k == *key || (*key == "colors" && *k == "colours"));
                    if let Some(at) = at {
                        let label = self.config.labels()[at].clone();
                        self.say(&label);
                    }
                }
                Err(why) => self.say(&format!("{why}. Try /bloodparticles help.")),
            },
            _ => self.say("/bloodparticles <setting> <value>, or help."),
        }
        Ok(None)
    }

    fn ui_settings_opened(&mut self) {
        let widgets: Vec<Widget> = self
            .config
            .labels()
            .into_iter()
            .enumerate()
            .map(|(id, label)| {
                Widget::Button(ButtonWidget {
                    id: id as u32,
                    label,
                })
            })
            .collect();
        ui::set_settings(&widgets);
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        let _ = self.config.cycle(id as usize);
        self.persist();
        self.ui_settings_opened();
    }
}

horizon_mod_sdk::export_mod!(BloodParticles);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_26() {
        assert_eq!(API_LEVEL, 26);
    }

    #[test]
    fn defaults_match_the_original() {
        let c = Config::DEFAULT;
        assert_eq!(c.droplets, 8);
        assert_eq!(c.mist, 6);
        assert_eq!(c.clouds, 2);
        assert_eq!(c.scaling, Scaling::Full);
        assert_eq!(c.death, 3.0);
        assert_eq!(c.spread, 1.0);
        assert_eq!(c.arc, 0.75);
        assert!(c.colours && c.crumbs);
        assert_eq!(c.reach, 40.0);
        assert_eq!(c.wounds, 8);
        assert_eq!(c.particles, 112);
    }

    #[test]
    fn encode_round_trips_and_snaps() {
        let c = Config::DEFAULT;
        assert_eq!(Config::decode(&c.encode()), c);
        // Values off the page's steps snap to the nearest step
        // (an exact tie keeps the lower step).
        let snapped = Config::decode("droplets=10;death=2.4;reach=30");
        assert_eq!(snapped.droplets, 8);
        assert_eq!(snapped.death, 2.0);
        assert_eq!(snapped.reach, 32.0);
    }

    #[test]
    fn describe_lines_match_the_original_page() {
        let lines = Config::DEFAULT.describe();
        assert_eq!(
            lines,
            vec![
                "Droplets a hit: 8, Mist a hit: 6, Clouds a hit: 2, Damage sizes the burst: full",
                "The killing blow throws: x3, Spread: normal, Arc: normal, Each mob's own colour: on",
                "Droplets are block crumbs: on, Drawn within: 40 blocks, Wounds a tick: 8, Particles a tick: 112",
            ]
        );
    }

    #[test]
    fn counts_scale_and_cap_like_the_original() {
        let c = Config::DEFAULT;
        // The reference damage throws the configured numbers.
        let w = wound_counts(&c, Some(6.0), false);
        assert_eq!((w.droplets, w.mist, w.clouds), (8, 6, 2));
        // Half the damage under Full scaling: half the burst.
        let w = wound_counts(&c, Some(3.0), false);
        assert_eq!((w.droplets, w.mist, w.clouds), (4, 3, 1));
        // The killing blow: the counts times three, no damage read.
        let w = wound_counts(&c, None, true);
        assert_eq!((w.droplets, w.mist, w.clouds), (24, 18, 6));
        // A huge hit caps at the ceilings.
        let w = wound_counts(&c, Some(600.0), false);
        assert_eq!((w.droplets, w.mist, w.clouds), (20, 15, 5));
    }

    #[test]
    fn bloods_match_the_original_table() {
        assert_eq!(BloodKind::of("minecraft:zombie"), BloodKind::Red);
        assert_eq!(BloodKind::of("minecraft:slime"), BloodKind::Slime);
        assert_eq!(BloodKind::of("minecraft:blaze"), BloodKind::Fire);
        assert_eq!(BloodKind::of("minecraft:enderman"), BloodKind::Ichor);
        assert_eq!(
            BloodKind::of("minecraft:wither_skeleton"),
            BloodKind::Bone
        );
        assert_eq!(
            BloodKind::of("minecraft:slime").blood().crumbs.0,
            "minecraft:slime_block"
        );
        assert_eq!(argb([0.42, 0.02, 0.02]), 0xFF6B_0505);
    }

    #[test]
    fn a_hit_plans_four_bursts_at_the_wound() {
        let c = Config::DEFAULT;
        let wound = Wound {
            kind: "minecraft:zombie".to_string(),
            at: [2.5, 65.24, 0.5],
            counts: wound_counts(&c, Some(6.0), false),
            death: false,
        };
        let bursts = plan_bursts(&c, &wound);
        assert_eq!(bursts.len(), 4);
        assert_eq!(bursts[0].particle, "minecraft:block");
        assert_eq!(bursts[0].count, 6);
        assert_eq!(
            bursts[0].params.block.as_deref(),
            Some("minecraft:nether_wart_block")
        );
        assert_eq!(bursts[1].count, 2);
        assert_eq!(
            bursts[1].params.block.as_deref(),
            Some("minecraft:redstone_block")
        );
        assert_eq!(bursts[2].particle, "minecraft:dust_color_transition");
        assert_eq!(bursts[2].count, 6);
        assert_eq!(bursts[2].params.color, Some(0xFF6B_0505));
        assert_eq!(bursts[2].params.scale, Some(0.85));
        assert_eq!(bursts[3].particle, "minecraft:dust");
        assert_eq!(bursts[3].count, 2);
        assert_eq!(bursts[3].params.scale, Some(1.6));
        assert_eq!(bursts[0].at.y, 65.24);
    }

    #[test]
    fn command_set_snaps_and_reports() {
        let mut c = Config::DEFAULT;
        c.set("droplets", "16").unwrap();
        assert_eq!(c.droplets, 16);
        c.set("scaling", "soft").unwrap();
        assert_eq!(c.scaling, Scaling::Soft);
        c.set("colors", "off").unwrap();
        assert!(!c.colours);
        assert!(c.set("scaling", "loud").is_err());
        assert!(c.set("nope", "1").is_err());
    }
}

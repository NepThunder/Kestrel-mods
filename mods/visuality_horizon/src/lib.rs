//! Visuality for Kestrel: hit fragments, slime blobs, soul
//! wisps, amethyst sparkles and ore glints.
//!
//! Behaviour reimplemented from Visuality 1.0.2 by BenjiSyS,
//! MIT, <https://github.com/BenjiSyS/visuality-horizon> — an
//! original Rust implementation of PinkGoosik's Visuality
//! (also MIT) for Horizon. This port shares no code or art
//! with either: the original's Blender-made sprites are
//! replaced by this port's own pixel art (see
//! `generate_sprites.py`), drawn to the same sprite-set
//! shapes, and the engine is rebuilt against the Kestrel SDK
//! (API 26's `particles` surface under `render`, plus the
//! content layering of an api "0.2" package for the sprite
//! sets). This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/visuality_horizon>.
//!
//! Fidelity, against the original: the settings are the
//! original's — the same four pages (Main, Effects, one page
//! per family, Advanced limits) with the same labels, value
//! cycles, presets (Low/Balanced/High with the same numbers)
//! and family defaults; the same hit rules, slime size
//! classes, soul/amethyst/ore odds shapes and per-effect
//! sizes, re-derived per effect in the code below. The
//! readings the port stands on, and the five effects it
//! cannot run, are the design note's: hit fragments key off
//! the health the client is told (the SDK carries health,
//! not the server damage event with an attacker the original
//! hooks); the ambient families sample the world on a scan
//! and the soul wisps ride position-anchored emitters rather
//! than the game's own ambient visits; and charged-creeper
//! sparks, armor glints and rain ripples wait on reads the
//! SDK does not have (an entity charged flag, entity
//! equipment, the weather) — they are documented gaps in the
//! README, with their settings kept, never approximations.

use std::collections::HashMap;

use horizon_mod_sdk::{
    Burst, Emitter, EmitterAnchor, Mod, ParticleParams, Registration, WorldPoint, entity, game,
    particles, storage,
    ui::{self, ButtonWidget, Widget},
    world::{self, BlockPos},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `particles.emit`/`set-emitters` are level 26
/// (`crates/horizon-mods/src/api_level.rs`); the entity,
/// world, game, storage and ui reads are older.
#[cfg(test)]
const API_LEVEL: u32 = 26;

/// The storage key the settings line lives under. The
/// original keeps its settings in
/// `config/visuality_horizon.toml`; the port keeps its own
/// one-line encoding in the mod's storage instead (there is
/// no file surface), under this key.
const CONFIG_KEY: &str = "config";

// ------------------------------------------------------------------ random

/// The mod's own random stream (SplitMix64) — the original's
/// algorithm and seed, so the port's odds run on the same
/// kind of stream. It never touches the game's world, server
/// or vanilla particle generators.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in 0..1.
    fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Uniform in 0..n (0 when n is 0).
    fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        ((self.next_u32() as u64 * n as u64) >> 32) as u32
    }
}

/// The original seeds its engine stream with the constant
/// 0x5649_5355 ("VISU").
const RNG_SEED: u64 = 0x5649_5355;

// ------------------------------------------------------------------ families

/// The eight effect families, in the original's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Hit,
    Slime,
    Charge,
    Amethyst,
    Armor,
    Ores,
    Soul,
    Water,
}

impl Family {
    const ALL: [Family; 8] = [
        Family::Hit,
        Family::Slime,
        Family::Charge,
        Family::Amethyst,
        Family::Armor,
        Family::Ores,
        Family::Soul,
        Family::Water,
    ];

    fn label(self) -> &'static str {
        match self {
            Family::Hit => "Hit fragments",
            Family::Slime => "Slime blobs",
            Family::Charge => "Charged creeper sparks",
            Family::Amethyst => "Amethyst sparkles",
            Family::Armor => "Armor glints",
            Family::Ores => "Ore glints",
            Family::Soul => "Soul wisps",
            Family::Water => "Rain ripples",
        }
    }

    /// The family's own active-particle ceiling at the Low
    /// preset (its default); presets scale it.
    const fn base_max_active(self) -> u32 {
        match self {
            Family::Hit => 48,
            Family::Slime => 24,
            Family::Charge => 12,
            Family::Amethyst => 16,
            Family::Armor => 16,
            Family::Ores => 16,
            Family::Soul => 24,
            Family::Water => 24,
        }
    }

    fn index(self) -> usize {
        Family::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// One family's settings: whether it runs, how much of it
/// there is, and its own ceiling.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FamilyCfg {
    enabled: bool,
    intensity: f32,
    max_active: u32,
}

impl FamilyCfg {
    const DEFAULT: Self = Self {
        enabled: true,
        intensity: 1.0,
        max_active: 0, // filled from Family::base_max_active
    };
}

/// Quality presets, plus Custom for "the numbers no longer
/// match one".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Preset {
    Low,
    Balanced,
    High,
    Custom,
}

impl Preset {
    fn label(self) -> &'static str {
        match self {
            Preset::Low => "Low",
            Preset::Balanced => "Balanced",
            Preset::High => "High",
            Preset::Custom => "Custom",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Preset::Low => "low",
            Preset::Balanced => "balanced",
            Preset::High => "high",
            Preset::Custom => "custom",
        }
    }

    fn from_key(key: &str) -> Option<Preset> {
        match key {
            "low" => Some(Preset::Low),
            "balanced" => Some(Preset::Balanced),
            "high" => Some(Preset::High),
            "custom" => Some(Preset::Custom),
            _ => None,
        }
    }

    /// (max_active, spawns per tick, distance, water radius,
    /// water samples, family scale) — the original's table.
    fn values(self) -> Option<(u32, u32, f64, u32, u32, f32)> {
        match self {
            Preset::Low => Some((128, 24, 24.0, 8, 10, 1.0)),
            Preset::Balanced => Some((256, 48, 32.0, 12, 16, 2.0)),
            Preset::High => Some((512, 80, 40.0, 16, 24, 4.0)),
            Preset::Custom => None,
        }
    }

    fn next(self) -> Preset {
        match self {
            Preset::Low => Preset::Balanced,
            Preset::Balanced => Preset::High,
            Preset::High | Preset::Custom => Preset::Low,
        }
    }
}

/// What the mod does when game's Particles option is Minimal
/// (and the follow setting is on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MinimalMode {
    /// Nothing extra at all.
    NoExtraParticles,
    /// Only the feedback families (hits and slimes) still run.
    FeedbackOnly,
}

impl MinimalMode {
    fn key(self) -> &'static str {
        match self {
            MinimalMode::NoExtraParticles => "no_extra",
            MinimalMode::FeedbackOnly => "feedback",
        }
    }

    fn label(self) -> &'static str {
        match self {
            MinimalMode::NoExtraParticles => "no extra particles",
            MinimalMode::FeedbackOnly => "hits and slimes only",
        }
    }
}

// The original's hard clamps.
const HARD_MAX_ACTIVE: u32 = 1024;
const HARD_MAX_SPAWNS_PER_TICK: u32 = 256;
const MIN_DISTANCE: f64 = 4.0;
const MAX_DISTANCE: f64 = 64.0;
const MAX_INTENSITY: f32 = 2.0;
const MAX_WATER_RADIUS: u32 = 32;
const MAX_WATER_SAMPLES: u32 = 64;
/// File-only in the original (no page shows them); the port
/// keeps them as the constants they effectively are without
/// a settings file.
const HIT_COOLDOWN_TICKS: u64 = 10;
const HIT_MAX_FRAGMENTS: u32 = 10;

/// Everything the player can change, matching the original's
/// config fields. Fields the original only exposes in its
/// settings file (hit cooldown and max fragments are the
/// constants above; the pending-requests cap, water
/// source/rain-only switches, adaptive density and the
/// frame trace have no surface here and stay at their
/// defaults in effect) are each called out in the README.
#[derive(Clone, Debug, PartialEq)]
struct Config {
    enabled: bool,
    preset: Preset,
    reduced_motion: bool,
    // quality
    respect_game_particle_setting: bool,
    minimal_mode: MinimalMode,
    global_intensity: f32,
    emission_distance_blocks: f64,
    max_active_particles: u32,
    max_spawns_per_tick: u32,
    // families
    families: [FamilyCfg; 8],
    // family extras
    slime_color: u32,
    amethyst_full_clusters_only: bool,
    armor_show_local_first_person: bool,
    ores_exposed_faces_only: bool,
    soul_ambient: bool,
    soul_sprinting: bool,
    // water (a gap family's page, kept whole)
    water_radius: u32,
    water_samples: u32,
    water_biome_tint: bool,
    // diagnostics
    show_counters: bool,
}

impl Config {
    const fn families_default() -> [FamilyCfg; 8] {
        let mut out = [FamilyCfg::DEFAULT; 8];
        let mut i = 0;
        while i < 8 {
            out[i].max_active = Family::ALL[i].base_max_active();
            i += 1;
        }
        out
    }

    const DEFAULT: Self = Self {
        enabled: true,
        preset: Preset::Low,
        reduced_motion: false,
        respect_game_particle_setting: true,
        minimal_mode: MinimalMode::NoExtraParticles,
        global_intensity: 1.0,
        emission_distance_blocks: 24.0,
        max_active_particles: 128,
        max_spawns_per_tick: 24,
        families: Self::families_default(),
        slime_color: 0x88FF79,
        amethyst_full_clusters_only: true,
        armor_show_local_first_person: false,
        ores_exposed_faces_only: true,
        soul_ambient: true,
        soul_sprinting: true,
        water_radius: 8,
        water_samples: 10,
        water_biome_tint: true,
        show_counters: false,
    };

    fn family(&self, f: Family) -> &FamilyCfg {
        &self.families[f.index()]
    }

    fn family_mut(&mut self, f: Family) -> &mut FamilyCfg {
        &mut self.families[f.index()]
    }

    /// The original's preset application: the quality
    /// numbers, the water search, and every family's ceiling
    /// scaled.
    fn apply_preset(&mut self, preset: Preset) {
        if let Some((max_active, spawns, distance, radius, samples, scale)) = preset.values() {
            self.max_active_particles = max_active;
            self.max_spawns_per_tick = spawns;
            self.emission_distance_blocks = distance;
            self.water_radius = radius;
            self.water_samples = samples;
            for f in Family::ALL {
                self.family_mut(f).max_active =
                    (f.base_max_active() as f32 * scale).round() as u32;
            }
        }
        self.preset = preset;
    }

    /// Which preset the current numbers match, if any — the
    /// original re-derives this after a hand change to a
    /// preset-managed number.
    fn detect_preset(&self) -> Preset {
        for preset in [Preset::Low, Preset::Balanced, Preset::High] {
            let (max_active, spawns, distance, radius, samples, scale) =
                preset.values().unwrap_or_default();
            if self.max_active_particles == max_active
                && self.max_spawns_per_tick == spawns
                && self.emission_distance_blocks == distance
                && self.water_radius == radius
                && self.water_samples == samples
                && Family::ALL.iter().all(|f| {
                    self.family(*f).max_active
                        == (f.base_max_active() as f32 * scale).round() as u32
                })
            {
                return preset;
            }
        }
        Preset::Custom
    }

    fn refresh_preset(&mut self) {
        self.preset = self.detect_preset();
    }

    /// The scale an effect of `family` is emitted at: the
    /// family's intensity by the global one. (The original
    /// also folds the game's Particles option in here when
    /// the follow setting is on; the SDK cannot read that
    /// option — the engine itself thins mod particles by it,
    /// so the term is the engine's to apply. See the README.)
    fn scale(&self, family: Family) -> f32 {
        self.family(family).intensity * self.global_intensity
    }

    /// The original's clamps, applied after any load.
    fn clamp(&mut self) {
        self.global_intensity = clamp_f(self.global_intensity, 0.0, MAX_INTENSITY, 1.0);
        self.emission_distance_blocks = clamp_f64(
            self.emission_distance_blocks,
            MIN_DISTANCE,
            MAX_DISTANCE,
            24.0,
        );
        self.max_active_particles = self.max_active_particles.clamp(1, HARD_MAX_ACTIVE);
        self.max_spawns_per_tick = self
            .max_spawns_per_tick
            .clamp(1, HARD_MAX_SPAWNS_PER_TICK);
        for f in &mut self.families {
            f.intensity = clamp_f(f.intensity, 0.0, MAX_INTENSITY, 1.0);
            f.max_active = f.max_active.min(HARD_MAX_ACTIVE);
        }
        self.slime_color &= 0xFF_FFFF;
        self.water_radius = self.water_radius.clamp(1, MAX_WATER_RADIUS);
        self.water_samples = self.water_samples.clamp(1, MAX_WATER_SAMPLES);
    }

    /// The settings as one line, which is what storage keeps.
    fn encode(&self) -> String {
        let mut s = format!(
            "enabled={};preset={};reduced={};respect={};minimal={};gint={};dist={};maxact={};\
             spawns={};slime={:06X};amfull={};armor1p={};oresexp={};soulamb={};soulspr={};\
             wradius={};wsamples={};wtint={};counters={}",
            self.enabled,
            self.preset.key(),
            self.reduced_motion,
            self.respect_game_particle_setting,
            self.minimal_mode.key(),
            self.global_intensity,
            self.emission_distance_blocks,
            self.max_active_particles,
            self.max_spawns_per_tick,
            self.slime_color,
            self.amethyst_full_clusters_only,
            self.armor_show_local_first_person,
            self.ores_exposed_faces_only,
            self.soul_ambient,
            self.soul_sprinting,
            self.water_radius,
            self.water_samples,
            self.water_biome_tint,
            self.show_counters,
        );
        for (i, f) in self.families.iter().enumerate() {
            s += &format!(";fam{i}={},{},{}", f.enabled, f.intensity, f.max_active);
        }
        s
    }

    /// Reads [`Config::encode`]'s line. Anything missing or
    /// impossible keeps its default, then the clamps run.
    fn decode(text: &str) -> Self {
        let mut config = Self::DEFAULT;
        for field in text.split(';') {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "enabled" => config.enabled = value != "false",
                "preset" => {
                    if let Some(p) = Preset::from_key(value) {
                        config.preset = p;
                    }
                }
                "reduced" => config.reduced_motion = value == "true",
                "respect" => config.respect_game_particle_setting = value != "false",
                "minimal" => {
                    config.minimal_mode = if value == "feedback" {
                        MinimalMode::FeedbackOnly
                    } else {
                        MinimalMode::NoExtraParticles
                    }
                }
                "gint" => {
                    if let Ok(v) = value.parse() {
                        config.global_intensity = v;
                    }
                }
                "dist" => {
                    if let Ok(v) = value.parse() {
                        config.emission_distance_blocks = v;
                    }
                }
                "maxact" => {
                    if let Ok(v) = value.parse() {
                        config.max_active_particles = v;
                    }
                }
                "spawns" => {
                    if let Ok(v) = value.parse() {
                        config.max_spawns_per_tick = v;
                    }
                }
                "slime" => {
                    if let Ok(v) = u32::from_str_radix(value, 16) {
                        config.slime_color = v;
                    }
                }
                "amfull" => config.amethyst_full_clusters_only = value != "false",
                "armor1p" => config.armor_show_local_first_person = value == "true",
                "oresexp" => config.ores_exposed_faces_only = value != "false",
                "soulamb" => config.soul_ambient = value != "false",
                "soulspr" => config.soul_sprinting = value != "false",
                "wradius" => {
                    if let Ok(v) = value.parse() {
                        config.water_radius = v;
                    }
                }
                "wsamples" => {
                    if let Ok(v) = value.parse() {
                        config.water_samples = v;
                    }
                }
                "wtint" => config.water_biome_tint = value != "false",
                "counters" => config.show_counters = value == "true",
                key if key.starts_with("fam") => {
                    if let Ok(i) = key[3..].parse::<usize>()
                        && i < 8
                    {
                        let mut parts = value.split(',');
                        if let (Some(e), Some(inten), Some(max)) =
                            (parts.next(), parts.next(), parts.next())
                        {
                            config.families[i].enabled = e != "false" && e != "0";
                            if let Ok(v) = inten.parse() {
                                config.families[i].intensity = v;
                            }
                            if let Ok(v) = max.parse() {
                                config.families[i].max_active = v;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        config.clamp();
        config
    }
}

fn clamp_f(value: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        default
    }
}

fn clamp_f64(value: f64, lo: f64, hi: f64, default: f64) -> f64 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        default
    }
}

// ------------------------------------------------------------------ the pages

/// The value cycles the pages step through — the original's
/// lists.
const AMOUNTS: [f32; 7] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
const DISTANCES: [f64; 7] = [12.0, 16.0, 24.0, 32.0, 40.0, 48.0, 64.0];
const BUDGETS: [u32; 6] = [64, 128, 256, 384, 512, 1024];
const SPAWNS: [u32; 5] = [12, 24, 48, 80, 128];
const RADII: [u32; 5] = [4, 8, 12, 16, 24];
const SAMPLES: [u32; 5] = [5, 10, 16, 24, 32];

/// The next of a page cycle: the first entry above the
/// current value, wrapping to the first (the original's
/// `next_in`).
fn next_in<T: PartialOrd + Copy>(steps: &[T], value: T) -> T {
    steps
        .iter()
        .copied()
        .find(|s| *s > value)
        .unwrap_or(steps[0])
}

/// The slime colour choices of the Slime page: names and
/// values, the original's list.
const SLIME_COLORS: [(&str, u32); 6] = [
    ("Classic green", 0x88FF79),
    ("Lime", 0xB8FF5A),
    ("Mint", 0x7AFFC4),
    ("Sky", 0x7AC8FF),
    ("Pink", 0xFF8FD0),
    ("Amber", 0xFFC86A),
];

/// The sprite-set page a slime colour draws with: named
/// particles cannot be tinted, so each page colour is its own
/// set (see `generate_sprites.py`). An unlisted (custom)
/// colour keeps the classic art.
fn slime_page(color: u32) -> &'static str {
    match color {
        0x88FF79 => "classic",
        0xB8FF5A => "lime",
        0x7AFFC4 => "mint",
        0x7AC8FF => "sky",
        0xFF8FD0 => "pink",
        0xFFC86A => "amber",
        _ => "classic",
    }
}

fn slime_color_name(color: u32) -> &'static str {
    SLIME_COLORS
        .iter()
        .find(|(_, v)| *v == color)
        .map(|(name, _)| *name)
        .unwrap_or("Custom")
}

/// The next slime colour on the page: the entry after the
/// current one, or the first when the current is unlisted.
fn next_slime_color(color: u32) -> u32 {
    let at = SLIME_COLORS.iter().position(|(_, v)| *v == color);
    match at {
        Some(i) => SLIME_COLORS[(i + 1) % SLIME_COLORS.len()].1,
        None => SLIME_COLORS[0].1,
    }
}

fn on(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn pct(value: f32) -> String {
    format!("{}%", (value * 100.0).round() as i64)
}

/// Which page the settings menu shows.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Page {
    Main,
    Effects,
    Family(Family),
    Advanced,
}

/// The menu state: the page, and whether the reset row is
/// armed (the original's two-click reset).
#[derive(Clone, Copy, Debug)]
struct Menu {
    page: Page,
    confirm_reset: bool,
}

impl Menu {
    const START: Self = Self {
        page: Page::Main,
        confirm_reset: false,
    };

    /// The rows of the current page, labelled exactly as the
    /// original labels them. One documented difference: the
    /// original's Hit/Armor/Ores pages carry a row pointing
    /// at its settings file and a reload button; the port has
    /// no file, so the row says the lists are built in and
    /// there is no reload row. (The original also shows a
    /// "Note:" row on Main when its file has problems — with
    /// no file there is nothing to note.)
    fn labels(&self, c: &Config) -> Vec<String> {
        match self.page {
            Page::Main => vec![
                format!("Visuality: {}", on(c.enabled)),
                format!("Quality preset: {}", c.preset.label()),
                format!("Particle amount: {}", pct(c.global_intensity)),
                format!(
                    "Effect distance: {} blocks",
                    trim_f64(c.emission_distance_blocks)
                ),
                format!("Reduced motion and flashes: {}", on(c.reduced_motion)),
                "Effects...".to_string(),
                "Advanced limits...".to_string(),
                if self.confirm_reset {
                    "Click again to reset everything".to_string()
                } else {
                    "Reset to defaults".to_string()
                },
            ],
            Page::Effects => {
                let mut rows: Vec<String> = Family::ALL
                    .iter()
                    .map(|f| format!("{}: {}...", f.label(), on(c.family(*f).enabled)))
                    .collect();
                rows.push("Back".to_string());
                rows
            }
            Page::Family(f) => {
                let fam = c.family(f);
                let mut rows = vec![
                    format!("{}: {}", f.label(), on(fam.enabled)),
                    format!("Amount: {}", pct(fam.intensity)),
                ];
                match f {
                    Family::Slime => rows.push(format!(
                        "Colour: {}",
                        slime_color_name(c.slime_color)
                    )),
                    Family::Amethyst => rows.push(format!(
                        "Only full-grown clusters: {}",
                        on(c.amethyst_full_clusters_only)
                    )),
                    Family::Armor => rows.push(format!(
                        "Own armor in first person: {}",
                        on(c.armor_show_local_first_person)
                    )),
                    Family::Ores => rows.push(format!(
                        "Only uncovered faces: {}",
                        on(c.ores_exposed_faces_only)
                    )),
                    Family::Soul => {
                        rows.push(format!(
                            "Wisps above soul blocks: {}",
                            on(c.soul_ambient)
                        ));
                        rows.push(format!(
                            "Wisps when sprinting: {}",
                            on(c.soul_sprinting)
                        ));
                    }
                    Family::Water => {
                        rows.push(format!("Search radius: {} blocks", c.water_radius));
                        rows.push(format!("Ripples per tick: up to {}", c.water_samples));
                        rows.push(format!(
                            "Biome water colour: {}",
                            on(c.water_biome_tint)
                        ));
                    }
                    Family::Hit | Family::Charge => {}
                }
                if matches!(f, Family::Hit | Family::Armor | Family::Ores) {
                    rows.push("Lists: the original's defaults, built in".to_string());
                }
                rows.push("Back".to_string());
                rows
            }
            Page::Advanced => vec![
                format!("Particle budget: {}", c.max_active_particles),
                format!("New particles per tick: {}", c.max_spawns_per_tick),
                format!(
                    "Follow the game's Particles option: {}",
                    on(c.respect_game_particle_setting)
                ),
                format!(
                    "With Minimal particles: {}",
                    c.minimal_mode.label()
                ),
                format!("Diagnostics counters: {}", on(c.show_counters)),
                "Back".to_string(),
            ],
        }
    }

    /// What row `row` of the current page does — the
    /// original's click table. Anything that changes a
    /// preset-managed number re-derives the preset.
    fn click(&mut self, c: &mut Config, row: usize) {
        // The reset's arming survives only its own row.
        if !(self.page == Page::Main && row == 7) {
            self.confirm_reset = false;
        }
        match self.page {
            Page::Main => match row {
                0 => c.enabled = !c.enabled,
                1 => {
                    let next = c.preset.next();
                    c.apply_preset(next);
                }
                2 => c.global_intensity = next_in(&AMOUNTS, c.global_intensity),
                3 => {
                    c.emission_distance_blocks =
                        next_in(&DISTANCES, c.emission_distance_blocks);
                    c.refresh_preset();
                }
                4 => c.reduced_motion = !c.reduced_motion,
                5 => self.page = Page::Effects,
                6 => self.page = Page::Advanced,
                7 => {
                    if self.confirm_reset {
                        *c = Config::DEFAULT;
                        self.confirm_reset = false;
                    } else {
                        self.confirm_reset = true;
                    }
                }
                _ => {}
            },
            Page::Effects => match row {
                0..=7 => self.page = Page::Family(Family::ALL[row]),
                8 => self.page = Page::Main,
                _ => {}
            },
            Page::Family(f) => {
                let back = self.family_rows(c, f);
                if row == back {
                    self.page = Page::Effects;
                    return;
                }
                match row {
                    0 => c.family_mut(f).enabled = !c.family(f).enabled,
                    1 => {
                        let v = next_in(&AMOUNTS, c.family(f).intensity);
                        c.family_mut(f).intensity = v;
                    }
                    _ => self.family_extra_click(c, f, row),
                }
            }
            Page::Advanced => match row {
                0 => {
                    c.max_active_particles = next_in(&BUDGETS, c.max_active_particles);
                    c.refresh_preset();
                }
                1 => {
                    c.max_spawns_per_tick = next_in(&SPAWNS, c.max_spawns_per_tick);
                    c.refresh_preset();
                }
                2 => {
                    c.respect_game_particle_setting = !c.respect_game_particle_setting
                }
                3 => {
                    c.minimal_mode = match c.minimal_mode {
                        MinimalMode::NoExtraParticles => MinimalMode::FeedbackOnly,
                        MinimalMode::FeedbackOnly => MinimalMode::NoExtraParticles,
                    }
                }
                4 => c.show_counters = !c.show_counters,
                5 => self.page = Page::Main,
                _ => {}
            },
        }
    }

    /// The index of the "Back" row on a family page.
    fn family_rows(&self, c: &Config, f: Family) -> usize {
        // Labels minus the Back row itself.
        let _ = c;
        let base = 2;
        let extras = match f {
            Family::Slime
            | Family::Amethyst
            | Family::Armor
            | Family::Ores => 1,
            Family::Soul => 2,
            Family::Water => 3,
            Family::Hit | Family::Charge => 0,
        };
        let lists = if matches!(f, Family::Hit | Family::Armor | Family::Ores) {
            1
        } else {
            0
        };
        base + extras + lists
    }

    fn family_extra_click(&mut self, c: &mut Config, f: Family, row: usize) {
        match f {
            Family::Slime => {
                if row == 2 {
                    c.slime_color = next_slime_color(c.slime_color);
                }
            }
            Family::Amethyst => {
                if row == 2 {
                    c.amethyst_full_clusters_only = !c.amethyst_full_clusters_only;
                }
            }
            Family::Armor => {
                if row == 2 {
                    c.armor_show_local_first_person = !c.armor_show_local_first_person;
                }
            }
            Family::Ores => {
                if row == 2 {
                    c.ores_exposed_faces_only = !c.ores_exposed_faces_only;
                }
            }
            Family::Soul => match row {
                2 => c.soul_ambient = !c.soul_ambient,
                3 => c.soul_sprinting = !c.soul_sprinting,
                _ => {}
            },
            Family::Water => match row {
                2 => {
                    c.water_radius = next_in(&RADII, c.water_radius);
                    c.refresh_preset();
                }
                3 => {
                    c.water_samples = next_in(&SAMPLES, c.water_samples);
                    c.refresh_preset();
                }
                4 => c.water_biome_tint = !c.water_biome_tint,
                _ => {}
            },
            Family::Hit | Family::Charge => {}
        }
    }
}

/// A whole number without its fraction, the way the
/// original's distance row shows 24 rather than 24.0.
fn trim_f64(value: f64) -> String {
    if value == value.trunc() && value.is_finite() {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

// ------------------------------------------------------------------ hit fragments

/// What a mob sheds when hit — the original's hit effects.
#[derive(Clone, Copy, Debug, PartialEq)]
enum HitEffect {
    Bone,
    WitherBone,
    Feather,
    Emerald,
}

/// The original's default hit rules (its built-in list):
/// these mobs shed these fragments. Anything else sheds
/// nothing.
fn hit_rule(kind: &str) -> Option<HitEffect> {
    match kind.strip_prefix("minecraft:").unwrap_or(kind) {
        "skeleton" | "skeleton_horse" | "stray" => Some(HitEffect::Bone),
        "wither_skeleton" => Some(HitEffect::WitherBone),
        // The original's FeatherByVariant picks a warm or cold
        // feather by the chicken's variant; the SDK has no
        // variant read, so the port sheds the plain feather
        // (see the README).
        "chicken" => Some(HitEffect::Feather),
        "villager" => Some(HitEffect::Emerald),
        _ => None,
    }
}

impl HitEffect {
    /// The fragment's sprite set: bone and wither bone have
    /// two drawings each, picked per fragment, as the
    /// original's art does.
    fn set(self, rng: &mut Rng) -> &'static str {
        match self {
            HitEffect::Bone => {
                if rng.below(2) == 0 {
                    "visuality_horizon:bone_a"
                } else {
                    "visuality_horizon:bone_b"
                }
            }
            HitEffect::WitherBone => {
                if rng.below(2) == 0 {
                    "visuality_horizon:wither_bone_a"
                } else {
                    "visuality_horizon:wither_bone_b"
                }
            }
            HitEffect::Feather => "visuality_horizon:feather",
            HitEffect::Emerald => "visuality_horizon:emerald",
        }
    }

    /// The fragment's size range (blocks across) — the
    /// original's per-effect sizes. Named particles draw at
    /// the engine's own base size times the burst's scale, so
    /// the port asks for size / [`NAMED_BASE`].
    fn size(self, rng: &mut Rng) -> f32 {
        match self {
            HitEffect::Bone | HitEffect::WitherBone => rng.range(0.18, 0.28),
            HitEffect::Emerald => rng.range(0.16, 0.24),
            HitEffect::Feather => rng.range(0.22, 0.34),
        }
    }
}

/// The engine's base size for a named (mod) particle: the
/// smoke sprite's quad, which `particles` scales by the
/// burst's `params.scale`. Sizes in the original are blocks
/// across; the port divides by this to ask for them.
const NAMED_BASE: f32 = 0.3;

// ------------------------------------------------------------------ slime blobs

/// A slime's size class, from its body width (a slime's
/// width grows with its size: ~0.5, ~1.0, ~2.0 blocks).
#[derive(Clone, Copy, Debug, PartialEq)]
enum SlimeSize {
    Small,
    Medium,
    Large,
}

fn slime_size(width: f32) -> SlimeSize {
    if width < 0.8 {
        SlimeSize::Small
    } else if width < 1.6 {
        SlimeSize::Medium
    } else {
        SlimeSize::Large
    }
}

impl SlimeSize {
    /// Blobs a landing throws before scaling (the original's
    /// per-class counts).
    fn base(self) -> u32 {
        match self {
            SlimeSize::Small => 2,
            SlimeSize::Medium => 3,
            SlimeSize::Large => 4,
        }
    }

    /// The blob's size across, in blocks.
    fn quad(self) -> f32 {
        match self {
            SlimeSize::Small => 0.18,
            SlimeSize::Medium => 0.30,
            SlimeSize::Large => 0.42,
        }
    }

    fn key(self) -> &'static str {
        match self {
            SlimeSize::Small => "small",
            SlimeSize::Medium => "medium",
            SlimeSize::Large => "large",
        }
    }
}

// ------------------------------------------------------------------ the scan

/// How often the world around the player is re-read for the
/// ambient families (ticks). The original hears about blocks
/// from the game's own ambient tick; the port polls.
const SCAN_EVERY: u64 = 10;
/// The scan's reach: ±8 in x/z, -4..+4 in y — 2601 blocks,
/// under the bulk read's 4096 cap.
const SCAN_R: i32 = 8;
const SCAN_Y: i32 = 4;

/// One ore block worth glinting, with the faces that show.
#[derive(Clone, Debug, PartialEq)]
struct OreSpot {
    at: [i32; 3],
    /// Bit i set when the face along [`DIRS`][i] is uncovered.
    faces: u8,
    /// The glint's sprite set (the material's own art, since
    /// named particles cannot be tinted).
    set: &'static str,
}

/// What one scan found.
#[derive(Clone, Default, Debug)]
struct Scan {
    /// Soul blocks with air above: wisp patches.
    soul: Vec<[i32; 3]>,
    /// Amethyst blocks that may sparkle.
    amethyst: Vec<[i32; 3]>,
    /// Ore blocks with at least one glinting face.
    ores: Vec<OreSpot>,
}

const DIRS: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

/// A block state's id (the part before any `[properties]`).
fn block_id(state: &str) -> &str {
    state.split('[').next().unwrap_or(state)
}

fn is_air(id: &str) -> bool {
    matches!(id, "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
}

fn is_soul(id: &str) -> bool {
    matches!(id, "minecraft:soul_sand" | "minecraft:soul_soil")
}

/// The glint set for an ore block id, from the original's
/// default ore list (the gold, diamond and emerald ore
/// tags): gold, diamond and emerald ores, in their stone,
/// deepslate and (for gold) nether forms.
fn ore_set(id: &str) -> Option<&'static str> {
    match id {
        "minecraft:gold_ore" | "minecraft:deepslate_gold_ore" | "minecraft:nether_gold_ore" => {
            Some("visuality_horizon:sparkle_gold")
        }
        "minecraft:diamond_ore" | "minecraft:deepslate_diamond_ore" => {
            Some("visuality_horizon:sparkle_diamond")
        }
        "minecraft:emerald_ore" | "minecraft:deepslate_emerald_ore" => {
            Some("visuality_horizon:sparkle_emerald")
        }
        _ => None,
    }
}

/// Classifies one scan's blocks. `blocks` maps position to
/// state string for every non-air answer; a position inside
/// the scan with no entry read as air, one outside the scan
/// as unknown (never air, never exposed — the original
/// counts an unloaded neighbour as covering a face).
fn classify(
    blocks: &HashMap<[i32; 3], String>,
    centre: [i32; 3],
    full_clusters_only: bool,
    exposed_faces_only: bool,
) -> Scan {
    let inside = |p: [i32; 3]| {
        (p[0] - centre[0]).abs() <= SCAN_R
            && (p[1] - centre[1]).abs() <= SCAN_Y
            && (p[2] - centre[2]).abs() <= SCAN_R
    };
    let id_at = |p: [i32; 3]| -> Option<&str> {
        if !inside(p) {
            return None; // outside the scan: unknown
        }
        Some(blocks.get(&p).map(|s| block_id(s)).unwrap_or("minecraft:air"))
    };
    let mut scan = Scan::default();
    for (pos, state) in blocks {
        let id = block_id(state);
        if is_soul(id) {
            // A wisp patch needs air above the soul block.
            if let Some(above) = id_at([pos[0], pos[1] + 1, pos[2]])
                && is_air(above)
            {
                scan.soul.push(*pos);
            }
        }
        if id == "minecraft:amethyst_cluster"
            || (!full_clusters_only && id.ends_with("_amethyst_bud"))
        {
            scan.amethyst.push(*pos);
        }
        if let Some(set) = ore_set(id) {
            let mut faces = 0u8;
            for (i, d) in DIRS.iter().enumerate() {
                let n = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
                let exposed = if !exposed_faces_only {
                    true
                } else {
                    match id_at(n) {
                        Some(nid) => {
                            is_air(nid)
                                || nid == "minecraft:water"
                                || nid == "minecraft:lava"
                        }
                        None => false, // outside the scan counts as covering
                    }
                };
                if exposed {
                    faces |= 1 << i;
                }
            }
            if faces != 0 {
                scan.ores.push(OreSpot {
                    at: *pos,
                    faces,
                    set,
                });
            }
        }
    }
    scan
}

// ------------------------------------------------------------------ bursts

fn plain_params() -> ParticleParams {
    ParticleParams {
        color: None,
        to_color: None,
        scale: None,
        block: None,
        target: None,
        duration: None,
    }
}

/// One particle of `set` at `at`, drawn at `size` blocks
/// across: a count-1 burst with almost no spread. (The
/// original places every particle itself; the port does the
/// same, one burst per particle, so positions and sizes are
/// the plan's own rather than a gaussian's.)
fn single(set: &str, at: [f64; 3], size: f32) -> Burst {
    let mut params = plain_params();
    params.scale = Some(size / NAMED_BASE);
    Burst {
        particle: set.to_string(),
        params,
        at: WorldPoint {
            x: at[0],
            y: at[1],
            z: at[2],
        },
        spread: WorldPoint {
            x: 0.02,
            y: 0.02,
            z: 0.02,
        },
        speed: 0.0,
        count: 1,
    }
}

/// One wisp of `set` at `at`, rising: a count-0 burst is the
/// surface's directional single — one particle whose
/// velocity is the spread vector, here straight up at the
/// original's rise speed.
fn wisp(set: &str, at: [f64; 3]) -> Burst {
    Burst {
        particle: set.to_string(),
        params: plain_params(),
        at: WorldPoint {
            x: at[0],
            y: at[1],
            z: at[2],
        },
        spread: WorldPoint {
            x: 0.0,
            y: 0.03,
            z: 0.0,
        },
        speed: 1.0,
        count: 0,
    }
}

// ------------------------------------------------------------------ the mod

struct Visuality {
    config: Config,
    menu: Menu,
    rng: Rng,
    /// The last health seen per mob id: the hit watch.
    prev_health: HashMap<u32, f32>,
    /// The tick each mob last shed fragments on.
    hit_last: HashMap<u32, u64>,
    /// Was the slime on the ground last tick, per mob id.
    prev_ground: HashMap<u32, bool>,
    /// The latest world scan.
    scan: Scan,
    last_scan: Option<u64>,
    /// The emitter set currently lent to the engine, as
    /// (block, rate bits) — compared before re-lending.
    emitter_key: Vec<([i32; 3], u32)>,
    /// Particles asked for this tick (the spawns budget).
    spawns: u32,
}

impl Visuality {
    fn persist(&self) {
        let _ = storage::set(CONFIG_KEY, self.config.encode().as_bytes());
    }

    /// The per-tick spawn budget: the original admits at
    /// most `max_spawns_per_tick` new particles a tick, and
    /// so does the port. (Its `max_active_particles` ceiling
    /// cannot be kept here: the surface lends no count of
    /// live particles. See the README.)
    fn gate(&mut self) -> bool {
        if self.spawns >= self.config.max_spawns_per_tick {
            return false;
        }
        self.spawns += 1;
        true
    }

    fn near(&self, player: [f64; 3], at: [f64; 3]) -> bool {
        let d = self.config.emission_distance_blocks;
        let dx = at[0] - player[0];
        let dy = at[1] - player[1];
        let dz = at[2] - player[2];
        dx * dx + dy * dy + dz * dz <= d * d
    }

    // -- hit fragments -------------------------------------------------

    fn run_hits(&mut self, tick: u64, player: [f64; 3], entities: &[entity::Entity]) {
        let enabled = self.config.family(Family::Hit).enabled;
        let s = self.config.scale(Family::Hit);
        for e in entities {
            let Some(health) = e.health else { continue };
            if !health.is_finite() {
                continue;
            }
            let previous = self.prev_health.insert(e.id, health);
            if !enabled {
                continue;
            }
            let Some(before) = previous else { continue };
            if health >= before {
                continue;
            }
            if !self.near(player, [e.x, e.y, e.z]) {
                continue;
            }
            if let Some(&last) = self.hit_last.get(&e.id)
                && tick.wrapping_sub(last) < HIT_COOLDOWN_TICKS
            {
                continue;
            }
            let Some(effect) = hit_rule(&e.kind) else {
                continue;
            };
            self.hit_last.insert(e.id, tick);
            // The original's count: (2 + a roll of 3) by the
            // scale, at least 1, at most the fragment cap.
            let base = 2 + self.rng.below(3);
            let n = ((base as f32 * s).round() as u32).clamp(1, HIT_MAX_FRAGMENTS);
            for _ in 0..n {
                if !self.gate() {
                    break;
                }
                let set = effect.set(&mut self.rng);
                let size = effect.size(&mut self.rng);
                let at = [
                    e.x + self.rng.range(-0.3, 0.3) as f64 * e.width as f64,
                    e.y + e.height as f64 * self.rng.range(0.25, 0.85) as f64,
                    e.z + self.rng.range(-0.3, 0.3) as f64 * e.width as f64,
                ];
                particles::emit(&single(set, at, size));
            }
        }
    }

    // -- slime blobs ---------------------------------------------------

    fn run_slimes(&mut self, player: [f64; 3], entities: &[entity::Entity]) {
        let enabled = self.config.family(Family::Slime).enabled;
        let s = self.config.scale(Family::Slime);
        for e in entities {
            if block_id(&e.kind) != "minecraft:slime" {
                continue;
            }
            let was = self.prev_ground.insert(e.id, e.on_ground);
            if !enabled || was != Some(false) || !e.on_ground {
                continue;
            }
            if !self.near(player, [e.x, e.y, e.z]) {
                continue;
            }
            let size = slime_size(e.width);
            let base = size.base() + self.rng.below(2);
            let n = ((base as f32 * s).round() as u32).clamp(1, 8);
            let set = format!(
                "visuality_horizon:slime_{}_{}",
                size.key(),
                slime_page(self.config.slime_color)
            );
            for _ in 0..n {
                if !self.gate() {
                    break;
                }
                let angle = self.rng.range(0.0, std::f32::consts::TAU) as f64;
                let radius = e.width as f64 * 0.5 * self.rng.range(0.5, 1.0) as f64;
                let at = [
                    e.x + angle.cos() * radius,
                    e.y + 0.05,
                    e.z + angle.sin() * radius,
                ];
                let blob_size = size.quad() * self.rng.range(0.85, 1.15);
                particles::emit(&single(&set, at, blob_size));
            }
        }
    }

    // -- the scan --------------------------------------------------------

    fn rescan(&mut self, player: &game::Player) {
        let centre = [
            player.x.floor() as i32,
            player.y.floor() as i32,
            player.z.floor() as i32,
        ];
        let mut positions = Vec::with_capacity(
            ((2 * SCAN_R + 1) * (2 * SCAN_Y + 1) * (2 * SCAN_R + 1)) as usize,
        );
        for dy in -SCAN_Y..=SCAN_Y {
            for dx in -SCAN_R..=SCAN_R {
                for dz in -SCAN_R..=SCAN_R {
                    positions.push(BlockPos {
                        x: centre[0] + dx,
                        y: centre[1] + dy,
                        z: centre[2] + dz,
                    });
                }
            }
        }
        let results = world::get_blocks(&player.dimension, &positions);
        let mut blocks: HashMap<[i32; 3], String> = HashMap::new();
        for (pos, result) in positions.iter().zip(results.iter()) {
            if let Some(state) = &result.state {
                if !is_air(block_id(state)) {
                    blocks.insert([pos.x, pos.y, pos.z], state.clone());
                }
            }
        }
        self.scan = classify(
            &blocks,
            centre,
            self.config.amethyst_full_clusters_only,
            self.config.ores_exposed_faces_only,
        );
    }

    // -- soul wisps ------------------------------------------------------

    /// Works out the wisp emitters the scan calls for and
    /// lends them when they changed: the nearest patches, up
    /// to the emitter cap, each a slow stream of wisps at
    /// the original's per-patch rate scaled by the family
    /// scale (0.4/s at scale 1 — its ~0.5%-a-visit over the
    /// game's ambient visits, re-based as a steady rate).
    fn run_soul_emitters(&mut self, player: [f64; 3]) {
        let on = self.config.family(Family::Soul).enabled && self.config.soul_ambient;
        let rate = 0.4 * self.config.scale(Family::Soul);
        let mut key: Vec<([i32; 3], u32)> = Vec::new();
        if on && rate > 0.0 {
            let mut patches = self.scan.soul.clone();
            patches.sort_by(|a, b| {
                let d = |p: &[i32; 3]| {
                    let dx = p[0] as f64 + 0.5 - player[0];
                    let dy = p[1] as f64 + 0.5 - player[1];
                    let dz = p[2] as f64 + 0.5 - player[2];
                    dx * dx + dy * dy + dz * dz
                };
                d(a).partial_cmp(&d(b)).unwrap_or(std::cmp::Ordering::Equal)
            });
            for p in patches.into_iter().take(8) {
                key.push((p, rate.to_bits()));
            }
        }
        if key != self.emitter_key {
            self.emitter_key = key;
            if self.emitter_key.is_empty() {
                particles::set_emitters(None);
            } else {
                let emitters: Vec<Emitter> = self
                    .emitter_key
                    .iter()
                    .map(|(p, bits)| Emitter {
                        anchor: EmitterAnchor::Position(WorldPoint {
                            x: p[0] as f64 + 0.5,
                            y: p[1] as f64 + 1.1,
                            z: p[2] as f64 + 0.5,
                        }),
                        offset: WorldPoint {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        },
                        burst: Burst {
                            particle: "visuality_horizon:soul_ambient".to_string(),
                            params: plain_params(),
                            at: WorldPoint {
                                x: 0.0,
                                y: 0.0,
                                z: 0.0,
                            },
                            spread: WorldPoint {
                                x: 0.0,
                                y: 0.03,
                                z: 0.0,
                            },
                            speed: 1.0,
                            count: 0,
                        },
                        rate: f32::from_bits(*bits),
                    })
                    .collect();
                particles::set_emitters(Some(&emitters));
            }
        }
    }

    fn run_soul_sprint(&mut self, player: &game::Player, player_at: [f64; 3]) {
        if !self.config.family(Family::Soul).enabled || !self.config.soul_sprinting {
            return;
        }
        let Some(state) = game::get_self_state() else {
            return;
        };
        if !state.sprinting {
            return;
        }
        let under = world::get_block(
            &player.dimension,
            player.x.floor() as i32,
            player.y.floor() as i32 - 1,
            player.z.floor() as i32,
        );
        let Some(state_str) = under else { return };
        if !is_soul(block_id(&state_str)) {
            return;
        }
        // The original's per-tick chance: a roll of 5 against
        // the scale.
        let s = self.config.scale(Family::Soul);
        if self.rng.f32() * 5.0 >= s {
            return;
        }
        if !self.gate() {
            return;
        }
        let at = [
            player_at[0] + self.rng.range(-0.15, 0.15) as f64,
            player_at[1] + 0.1,
            player_at[2] + self.rng.range(-0.15, 0.15) as f64,
        ];
        particles::emit(&wisp("visuality_horizon:soul_sprint", at));
    }

    // -- amethyst and ore sparkles ----------------------------------------

    /// Per-tick rolls over the scan's candidates. The
    /// original rolls when the game's ambient tick visits a
    /// block (amethyst: a roll of 2 against the scale; ores:
    /// a roll of 5 per exposed face); visits are rare and
    /// unobservable here, so the port rolls every candidate
    /// every tick at visit-shaped odds — 1/200 a tick for
    /// amethyst, 1/500 a face for ores, each by the family
    /// scale — which lands at the same sparse density.
    fn run_sparkles(&mut self) {
        if self.config.family(Family::Amethyst).enabled && !self.scan.amethyst.is_empty() {
            let s = self.config.scale(Family::Amethyst);
            let chance = s / 200.0;
            for i in 0..self.scan.amethyst.len() {
                if self.rng.f32() >= chance {
                    continue;
                }
                if !self.gate() {
                    return;
                }
                let p = self.scan.amethyst[i];
                let at = [
                    p[0] as f64 + self.rng.range(0.2, 0.8) as f64,
                    p[1] as f64 + self.rng.range(0.2, 0.8) as f64,
                    p[2] as f64 + self.rng.range(0.2, 0.8) as f64,
                ];
                let size = self.rng.range(0.22, 0.32);
                particles::emit(&single("visuality_horizon:sparkle_amethyst", at, size));
            }
        }
        if self.config.family(Family::Ores).enabled && !self.scan.ores.is_empty() {
            let s = self.config.scale(Family::Ores);
            let chance = s / 500.0;
            for i in 0..self.scan.ores.len() {
                let spot = self.scan.ores[i].clone();
                for (fi, d) in DIRS.iter().enumerate() {
                    if spot.faces & (1 << fi) == 0 {
                        continue;
                    }
                    if self.rng.f32() >= chance {
                        continue;
                    }
                    if !self.gate() {
                        return;
                    }
                    // The face's centre, pushed out of the
                    // block, with the original's in-plane
                    // scatter.
                    let centre = [
                        spot.at[0] as f64 + 0.5 + d[0] as f64 * 0.5625,
                        spot.at[1] as f64 + 0.5 + d[1] as f64 * 0.5625,
                        spot.at[2] as f64 + 0.5 + d[2] as f64 * 0.5625,
                    ];
                    let mut at = centre;
                    for axis in 0..3 {
                        if d[axis] == 0 {
                            at[axis] += self.rng.range(-0.3, 0.3) as f64;
                        }
                    }
                    let size = self.rng.range(0.2, 0.3);
                    particles::emit(&single(spot.set, at, size));
                }
            }
        }
    }
}

impl Mod for Visuality {
    fn init() -> Result<(Self, Registration), String> {
        let config = storage::get(CONFIG_KEY)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|text| Config::decode(&text))
            .unwrap_or(Config::DEFAULT);
        Ok((
            Self {
                config,
                menu: Menu::START,
                rng: Rng::new(RNG_SEED),
                prev_health: HashMap::new(),
                hit_last: HashMap::new(),
                prev_ground: HashMap::new(),
                scan: Scan::default(),
                last_scan: None,
                emitter_key: Vec::new(),
                spawns: 0,
            },
            Registration::new().on_tick(),
        ))
    }

    fn tick(&mut self, tick: u64) {
        self.spawns = 0;
        let Some(player) = game::get_player() else {
            return;
        };
        let player_at = [player.x, player.y, player.z];
        if !self.config.enabled {
            if !self.emitter_key.is_empty() {
                self.emitter_key = Vec::new();
                particles::set_emitters(None);
            }
            return;
        }
        let entities = entity::entities();
        self.run_hits(tick, player_at, &entities);
        self.run_slimes(player_at, &entities);
        let want_scan = (self.config.family(Family::Soul).enabled && self.config.soul_ambient)
            || self.config.family(Family::Amethyst).enabled
            || self.config.family(Family::Ores).enabled;
        if want_scan
            && self
                .last_scan
                .map(|t| tick.wrapping_sub(t) >= SCAN_EVERY)
                .unwrap_or(true)
        {
            self.last_scan = Some(tick);
            self.rescan(&player);
        }
        self.run_soul_emitters(player_at);
        self.run_soul_sprint(&player, player_at);
        self.run_sparkles();
    }

    fn ui_settings_opened(&mut self) {
        let widgets: Vec<Widget> = self
            .menu
            .labels(&self.config)
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
        self.menu.click(&mut self.config, id as usize);
        self.persist();
        self.ui_settings_opened();
    }
}

horizon_mod_sdk::export_mod!(Visuality);

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
        assert!(c.enabled);
        assert_eq!(c.preset, Preset::Low);
        assert_eq!(c.max_active_particles, 128);
        assert_eq!(c.max_spawns_per_tick, 24);
        assert_eq!(c.emission_distance_blocks, 24.0);
        assert_eq!(c.water_radius, 8);
        assert_eq!(c.water_samples, 10);
        assert_eq!(c.slime_color, 0x88FF79);
        assert_eq!(c.family(Family::Hit).max_active, 48);
        assert_eq!(c.family(Family::Soul).max_active, 24);
        assert!(c.amethyst_full_clusters_only && c.ores_exposed_faces_only);
        assert!(c.soul_ambient && c.soul_sprinting);
    }

    #[test]
    fn presets_apply_and_detect() {
        let mut c = Config::DEFAULT;
        c.apply_preset(Preset::Balanced);
        assert_eq!(c.max_active_particles, 256);
        assert_eq!(c.max_spawns_per_tick, 48);
        assert_eq!(c.emission_distance_blocks, 32.0);
        assert_eq!(c.family(Family::Hit).max_active, 96);
        assert_eq!(c.detect_preset(), Preset::Balanced);
        c.max_spawns_per_tick = 24;
        assert_eq!(c.detect_preset(), Preset::Custom);
    }

    #[test]
    fn encode_round_trips() {
        let mut c = Config::DEFAULT;
        c.apply_preset(Preset::High);
        c.slime_color = 0xFF8FD0;
        c.family_mut(Family::Ores).intensity = 1.5;
        assert_eq!(Config::decode(&c.encode()), c);
    }

    #[test]
    fn main_page_labels_match_the_original() {
        let menu = Menu::START;
        assert_eq!(
            menu.labels(&Config::DEFAULT),
            vec![
                "Visuality: On",
                "Quality preset: Low",
                "Particle amount: 100%",
                "Effect distance: 24 blocks",
                "Reduced motion and flashes: Off",
                "Effects...",
                "Advanced limits...",
                "Reset to defaults",
            ]
        );
    }

    #[test]
    fn menu_walks_pages_and_the_reset_takes_two_clicks() {
        let mut c = Config::DEFAULT;
        let mut menu = Menu::START;
        menu.click(&mut c, 5); // Effects...
        assert_eq!(menu.page, Page::Effects);
        menu.click(&mut c, 6); // Soul wisps page
        assert_eq!(menu.page, Page::Family(Family::Soul));
        let labels = menu.labels(&c);
        assert_eq!(labels.len(), 5);
        assert_eq!(labels[2], "Wisps above soul blocks: On");
        menu.click(&mut c, 4); // Back
        assert_eq!(menu.page, Page::Effects);
        menu.click(&mut c, 8); // Back
        assert_eq!(menu.page, Page::Main);
        menu.click(&mut c, 2); // amount 100% -> 125%
        assert_eq!(c.global_intensity, 1.25);
        c.global_intensity = 1.75;
        menu.click(&mut c, 7); // arm the reset
        assert!(menu.confirm_reset);
        assert_eq!(c.global_intensity, 1.75);
        menu.click(&mut c, 7); // fire it
        assert_eq!(c, Config::DEFAULT);
    }

    #[test]
    fn hit_rules_match_the_original_list() {
        assert_eq!(hit_rule("minecraft:skeleton"), Some(HitEffect::Bone));
        assert_eq!(
            hit_rule("minecraft:wither_skeleton"),
            Some(HitEffect::WitherBone)
        );
        assert_eq!(hit_rule("minecraft:chicken"), Some(HitEffect::Feather));
        assert_eq!(hit_rule("minecraft:villager"), Some(HitEffect::Emerald));
        assert_eq!(hit_rule("minecraft:zombie"), None);
    }

    #[test]
    fn slime_classes_follow_the_width() {
        assert_eq!(slime_size(0.51), SlimeSize::Small);
        assert_eq!(slime_size(1.02), SlimeSize::Medium);
        assert_eq!(slime_size(2.04), SlimeSize::Large);
        assert_eq!(slime_page(0x88FF79), "classic");
        assert_eq!(slime_color_name(0x7AC8FF), "Sky");
    }

    #[test]
    fn classify_finds_patches_clusters_and_faces() {
        let mut blocks: HashMap<[i32; 3], String> = HashMap::new();
        let centre = [0, 64, 0];
        blocks.insert([0, 63, 0], "minecraft:soul_sand".to_string());
        blocks.insert([3, 64, 3], "minecraft:amethyst_cluster".to_string());
        blocks.insert([5, 64, 0], "minecraft:small_amethyst_bud".to_string());
        blocks.insert([-2, 64, 0], "minecraft:gold_ore".to_string());
        blocks.insert([-3, 64, 0], "minecraft:stone".to_string());
        let scan = classify(&blocks, centre, true, true);
        assert_eq!(scan.soul, vec![[0, 63, 0]]);
        assert_eq!(scan.amethyst, vec![[3, 64, 3]]); // bud excluded: full only
        assert_eq!(scan.ores.len(), 1);
        // The -x face (bit 1) is covered by the stone; the
        // other five show.
        assert_eq!(scan.ores[0].faces, 0b111101);
        assert_eq!(scan.ores[0].set, "visuality_horizon:sparkle_gold");
        let scan = classify(&blocks, centre, false, true);
        assert_eq!(scan.amethyst.len(), 2); // the bud joins in
    }

    #[test]
    fn rng_stays_in_range() {
        let mut r = Rng::new(RNG_SEED);
        for _ in 0..1000 {
            assert!((0.0..1.0).contains(&r.f32()));
            assert!(r.below(5) < 5);
        }
        assert_eq!(r.below(0), 0);
    }
}

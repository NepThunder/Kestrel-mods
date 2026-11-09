//! Sound Physics for Kestrel: sounds are quietened and muffled by
//! the blocks between you and them, and enclosed spaces — caves,
//! rooms, tunnels — give sounds reverb.
//!
//! Behaviour reimplemented from Sound Physics Remastered by
//! henkelmax (GPL-3.0, <https://github.com/henkelmax/sound-physics-remastered>),
//! itself in the lineage of Sonic Ether's Sound Physics through
//! vlad2305m's Fabric port; this file shares no code with any of
//! them — the originals are Java mods, this is a fresh implementation
//! against the Kestrel SDK. The original raycasts from every sound
//! to the listener for occlusion and estimates the environment's
//! reflectivity for its OpenAL EFX reverb; this port samples blocks
//! along the same line for occlusion and probes the blocks around
//! the listener for enclosure, and the game applies the muffle and
//! the reverb in its own mixer (API 15, capability `audio`). This
//! port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/sound_physics>.
//!
//! Approximations, against the original: every solid block absorbs
//! alike (no per-material table), the reverb's character is one
//! global estimate around the listener rather than per-sound, and
//! world reads are single-player only in v1 of the SDK's `world`
//! capability, so on a multiplayer server the mod leaves sounds
//! alone. Sounds whose listener is underwater are also left alone:
//! vanilla already muffles those itself.

use horizon_mod_sdk::{
    Mod, Registration, SoundAdjustment, SoundQuery, audio, game, storage,
    ui::{self, ButtonWidget, Widget},
    world::{self, BlockPos},
};

/// The newest API level this mod needs: `audio` is level 15
/// (`crates/horizon-mods/src/api_level.rs`); `world.get-blocks` is 8.
#[cfg(test)]
const API_LEVEL: u32 = 15;

/// The player's eye above their feet, as the game puts the camera.
const EYE: f64 = 1.62;
/// The setting steps both switches offer, as percents.
const STEPS: [u32; 5] = [0, 25, 50, 75, 100];
const REVERB_KEY: &str = "reverb";
const OCCLUSION_KEY: &str = "occlusion";
const REVERB_BUTTON: u32 = 1;
const OCCLUSION_BUTTON: u32 = 2;
/// Occlusion samples along a long sound's line, at most.
const MAX_SAMPLES: usize = 32;
/// The 26 directions around the listener the enclosure probe casts:
/// the 6 axes, the 12 edge diagonals and the 8 corners of a cube.
const PROBE_DIRS: [(i32, i32, i32); 26] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
    (1, 1, 0),
    (1, -1, 0),
    (-1, 1, 0),
    (-1, -1, 0),
    (1, 0, 1),
    (1, 0, -1),
    (-1, 0, 1),
    (-1, 0, -1),
    (0, 1, 1),
    (0, 1, -1),
    (0, -1, 1),
    (0, -1, -1),
    (1, 1, 1),
    (1, 1, -1),
    (1, -1, 1),
    (1, -1, -1),
    (-1, 1, 1),
    (-1, 1, -1),
    (-1, -1, 1),
    (-1, -1, -1),
];
/// The distances the enclosure probe checks in each direction.
const PROBE_RADII: [i32; 2] = [5, 11];

/// No change to a sound.
const NO_CHANGE: SoundAdjustment = SoundAdjustment { gain: 1.0, lowpass: 0.0, reverb: 0.0 };

/// The block id inside a block state: `minecraft:oak_log[axis=y]`
/// reads as `oak_log`.
fn block_id(state: &str) -> &str {
    let id = state.split('[').next().unwrap_or(state);
    id.rsplit(':').next().unwrap_or(id)
}

/// Whether a block state stops sound. Air and the see-through,
/// walk-through clutter do not; everything else does, water
/// included. A `None` state (no chunk loaded there) does not.
fn is_solid(state: Option<&str>) -> bool {
    let Some(state) = state else { return false };
    let id = block_id(state);
    if id == "air" || id.ends_with("_air") {
        return false;
    }
    !matches!(
        id,
        "short_grass"
            | "tall_grass"
            | "fern"
            | "large_fern"
            | "dead_bush"
            | "seagrass"
            | "tall_seagrass"
            | "kelp"
            | "kelp_plant"
            | "torch"
            | "wall_torch"
            | "soul_torch"
            | "soul_wall_torch"
            | "redstone_torch"
            | "redstone_wall_torch"
            | "rail"
            | "powered_rail"
            | "detector_rail"
            | "activator_rail"
            | "ladder"
            | "vine"
            | "glow_lichen"
            | "snow"
            | "sugar_cane"
            | "bamboo"
            | "cobweb"
            | "fire"
            | "soul_fire"
            | "nether_sprouts"
            | "crimson_roots"
            | "warped_roots"
            | "hanging_roots"
            | "spore_blossom"
            | "small_dripleaf"
            | "big_dripleaf"
    )
}

/// The fraction of the line from source to listener that runs
/// through solid blocks.
fn occlusion(solids: u32, samples: u32) -> f32 {
    if samples == 0 {
        return 0.0;
    }
    solids as f32 / samples as f32
}

/// A sound's adjustment from the two estimates and the settings:
/// occlusion quiets and muffles, enclosure sends to the reverb.
fn compose(occlusion: f32, enclosure: f32, strength: f32, reverb_setting: f32) -> SoundAdjustment {
    let occ = (occlusion * strength).clamp(0.0, 1.0);
    SoundAdjustment {
        gain: 1.0 - 0.92 * occ,
        lowpass: occ,
        reverb: (enclosure * reverb_setting).clamp(0.0, 1.0),
    }
}

/// The shared reverb's character for an enclosure estimate: the wet
/// gain and the decay in seconds, or dry (None) in the open.
fn environment(enclosure: f32, reverb_setting: f32) -> Option<(f32, f32)> {
    if enclosure < 0.04 || reverb_setting <= 0.0 {
        return None;
    }
    let wet = (enclosure * reverb_setting * 0.8).clamp(0.0, 0.8);
    let decay = 0.5 + 3.0 * enclosure;
    Some((wet, decay))
}

/// The next setting step, wrapping from 100 back to 0.
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

struct SoundPhysics {
    /// The reverb amount the player chose, as a percent.
    reverb: u32,
    /// The occlusion strength the player chose, as a percent.
    occlusion: u32,
    /// The smoothed enclosure estimate, 0 in the open to 1 in a box.
    enclosure: f32,
    /// The environment last handed to the game, to avoid re-sending.
    applied_env: Option<(f32, f32)>,
    /// Whether the applied state was "dry" (clear sent / never set).
    applied_dry: bool,
}

impl SoundPhysics {
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: REVERB_BUTTON,
                label: format!("Reverb: {}%", self.reverb),
            }),
            Widget::Button(ButtonWidget {
                id: OCCLUSION_BUTTON,
                label: format!("Occlusion: {}%", self.occlusion),
            }),
        ]);
    }

    /// Hand the game the reverb character for the current estimate,
    /// when it changed enough to matter.
    fn apply_environment(&mut self) {
        let env = environment(self.enclosure, self.reverb as f32 / 100.0);
        match env {
            Some((wet, decay)) => {
                let changed = self.applied_dry
                    || self.applied_env.is_none_or(|(w, d)| {
                        (w - wet).abs() > 0.02 || (d - decay).abs() > 0.1
                    });
                if changed {
                    audio::set_environment(wet, decay);
                    self.applied_env = Some((wet, decay));
                    self.applied_dry = false;
                }
            }
            None => {
                if !self.applied_dry {
                    audio::clear_environment();
                    self.applied_env = None;
                    self.applied_dry = true;
                }
            }
        }
    }

    /// Re-estimate the enclosure from the blocks around the player
    /// and smooth it towards the reading.
    fn probe_enclosure(&mut self, player: &game::Player) {
        let (ex, ey, ez) = (player.x.floor() as i32, (player.y + EYE).floor() as i32, player.z.floor() as i32);
        let mut positions = Vec::with_capacity(PROBE_DIRS.len() * PROBE_RADII.len());
        for r in PROBE_RADII {
            for (dx, dy, dz) in PROBE_DIRS {
                positions.push(BlockPos { x: ex + dx * r, y: ey + dy * r, z: ez + dz * r });
            }
        }
        let results = world::get_blocks(&player.dimension, &positions);
        let solids = results.iter().filter(|b| is_solid(b.state.as_deref())).count();
        let target = occlusion(solids as u32, results.len() as u32);
        self.enclosure += (target - self.enclosure) * 0.35;
        if self.enclosure < 0.005 {
            self.enclosure = 0.0;
        }
    }
}

impl Mod for SoundPhysics {
    fn init() -> Result<(Self, Registration), String> {
        let state = SoundPhysics {
            reverb: read_percent(REVERB_KEY, 50),
            occlusion: read_percent(OCCLUSION_KEY, 100),
            enclosure: 0.0,
            applied_env: None,
            applied_dry: true,
        };
        Ok((state, Registration::new().command("soundphysics").on_tick()))
    }

    fn tick(&mut self, tick: u64) {
        if tick % 10 != 0 {
            return;
        }
        let Some(player) = game::get_player() else { return };
        self.probe_enclosure(&player);
        self.apply_environment();
    }

    fn sound(&mut self, query: &SoundQuery) -> SoundAdjustment {
        let Some(player) = game::get_player() else { return NO_CHANGE };
        let ear = (player.x, player.y + EYE, player.z);
        // Vanilla muffles underwater sounds itself; leave them to it.
        let eye_block = world::get_block(
            &player.dimension,
            ear.0.floor() as i32,
            ear.1.floor() as i32,
            ear.2.floor() as i32,
        );
        if eye_block.as_deref().is_some_and(|s| block_id(s) == "water") {
            return NO_CHANGE;
        }
        let src = (query.position.x, query.position.y, query.position.z);
        let (dx, dy, dz) = (src.0 - ear.0, src.1 - ear.1, src.2 - ear.2);
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();
        let strength = self.occlusion as f32 / 100.0;
        let reverb_setting = self.reverb as f32 / 100.0;
        if distance < 2.0 || strength <= 0.0 {
            // The player's own sounds: no occlusion, the room still echoes.
            return compose(0.0, self.enclosure, strength, reverb_setting);
        }
        let samples = (distance.ceil() as usize).clamp(6, MAX_SAMPLES);
        let mut positions = Vec::with_capacity(samples);
        for i in 0..samples {
            let t = (i as f64 + 0.5) / samples as f64;
            positions.push(BlockPos {
                x: (ear.0 + dx * t).floor() as i32,
                y: (ear.1 + dy * t).floor() as i32,
                z: (ear.2 + dz * t).floor() as i32,
            });
        }
        let results = world::get_blocks(&player.dimension, &positions);
        let solids = results.iter().filter(|b| is_solid(b.state.as_deref())).count();
        let occ = occlusion(solids as u32, results.len() as u32);
        compose(occ, self.enclosure, strength, reverb_setting)
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        match name {
            "soundphysics" => Ok(Some(format!(
                "Sound Physics: enclosure {}%, reverb {}%, occlusion {}%",
                (self.enclosure * 100.0).round() as u32,
                self.reverb,
                self.occlusion
            ))),
            _ => Err("This command does nothing".into()),
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            REVERB_BUTTON => {
                self.reverb = next_step(self.reverb);
                let _ = storage::set(REVERB_KEY, self.reverb.to_string().as_bytes());
                self.apply_environment();
            }
            OCCLUSION_BUTTON => {
                self.occlusion = next_step(self.occlusion);
                let _ = storage::set(OCCLUSION_KEY, self.occlusion.to_string().as_bytes());
            }
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(SoundPhysics);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_15() {
        assert_eq!(API_LEVEL, 15);
    }

    #[test]
    fn solidity_reads_states() {
        assert!(is_solid(Some("minecraft:stone")));
        assert!(is_solid(Some("minecraft:oak_log[axis=y]")));
        assert!(is_solid(Some("minecraft:water[level=0]")));
        assert!(!is_solid(Some("minecraft:air")));
        assert!(!is_solid(Some("minecraft:cave_air")));
        assert!(!is_solid(Some("minecraft:short_grass")));
        assert!(!is_solid(Some("minecraft:torch")));
        assert!(!is_solid(None));
    }

    #[test]
    fn open_line_is_unoccluded_and_full_wall_is_not() {
        assert_eq!(occlusion(0, 16), 0.0);
        assert_eq!(occlusion(16, 16), 1.0);
        let open = compose(0.0, 0.0, 1.0, 0.5);
        assert_eq!((open.gain, open.lowpass, open.reverb), (1.0, 0.0, 0.0));
        let walled = compose(1.0, 0.0, 1.0, 0.5);
        assert!((walled.gain - 0.08).abs() < 1e-6);
        assert_eq!(walled.lowpass, 1.0);
        // Half strength halves the muffle.
        let half = compose(1.0, 0.0, 0.5, 0.5);
        assert!((half.gain - 0.54).abs() < 1e-6);
        assert_eq!(half.lowpass, 0.5);
    }

    #[test]
    fn enclosure_drives_the_reverb() {
        assert_eq!(environment(0.0, 0.5), None);
        assert_eq!(environment(0.5, 0.0), None);
        let (wet, decay) = environment(1.0, 1.0).expect("a box echoes");
        assert!((wet - 0.8).abs() < 1e-6);
        assert!((decay - 3.5).abs() < 1e-6);
        // A cave at the default reverb setting sends half its echo.
        let send = compose(0.0, 0.6, 1.0, 0.5);
        assert!((send.reverb - 0.3).abs() < 1e-6);
    }

    #[test]
    fn steps_cycle_and_wrap() {
        assert_eq!(next_step(0), 25);
        assert_eq!(next_step(50), 75);
        assert_eq!(next_step(100), 0);
        assert_eq!(next_step(33), 50);
    }
}

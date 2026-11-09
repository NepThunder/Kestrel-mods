//! Stuck? for Kestrel: the original's nearby-safe-spot search and
//! its cooldown, with the move run as a teleport through the
//! player — the one mechanism a client mod has.
//!
//! Behaviour reimplemented from Stuck? by adasjusk (CC-BY-SA-4.0,
//! <https://github.com/adasjusk/Stuck>), as described by the
//! Horizon registry entry for the port by Vakarux12; this file
//! shares no code with either. The original searches random
//! columns near the player for solid ground with two blocks of
//! air above, skips lava, fire, magma, cactus and other hazards
//! (and leaves and snow), brings a player on the Nether's
//! bedrock roof back under it, and keeps a cooldown between
//! uses — 30 seconds by the default `config/stuck.properties`.
//! Only loaded chunks are searched. This port keeps the search
//! rules, the roof rule and the 30-second default; what differs
//! is forced by the SDK and stated in the README: the search
//! reads the world through the client `world` capability
//! (single-player only in v1, where the plugin read the server's
//! world anywhere), and the move is the vanilla `tp` command run
//! as the player, so the player's own permissions decide where
//! the plugin simply moved them. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/stuck>.

use horizon_mod_sdk::{
    Mod, Registration, command, game,
    world::{self, BlockPos},
};

/// The cooldown between uses, in ticks: the original's default
/// of 30 seconds at 20 ticks per second.
const COOLDOWN_TICKS: u64 = 600;
/// Candidate columns tried per use.
const TRIES: usize = 24;
/// How far around the player columns are drawn from: the
/// original searches "near you"; its properties file's radius
/// is not in the registry description, so the search uses 32
/// blocks and the README says the number is this port's.
const SEARCH_RADIUS: i32 = 32;
/// The downward scan for a column's ground starts at the
/// player's own height plus this much (and never below it).
const SCAN_ABOVE: i32 = 16;
/// The Nether's bedrock roof: the ceiling layer sits at y=127,
/// so a player at 128 or above in the Nether is on the roof.
const NETHER_ROOF_Y: i32 = 128;

/// SplitMix64: a small deterministic PRNG. The mod has no clock
/// or entropy surface, so searches seed it from the player's
/// position and the world clock — plumbing, not a feature.
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A column offset inside the search radius, from a random draw.
fn pick_offset(state: &mut u64) -> (i32, i32) {
    let span = (2 * SEARCH_RADIUS + 1) as u64;
    let dx = (splitmix(state) % span) as i32 - SEARCH_RADIUS;
    let dz = (splitmix(state) % span) as i32 - SEARCH_RADIUS;
    (dx, dz)
}

/// The block id inside a block state: `minecraft:oak_log[axis=y]`
/// reads as `oak_log`.
fn block_id(state: &str) -> &str {
    let id = state.split('[').next().unwrap_or(state);
    id.rsplit(':').next().unwrap_or(id)
}

/// Whether a block state is air.
fn is_air(state: Option<&str>) -> bool {
    state.is_some_and(|s| {
        let id = block_id(s);
        id == "air" || id.ends_with("_air")
    })
}

/// Whether a block state is ground the original refuses to land
/// on: lava, fire, magma and cactus, the fluids the plugin's
/// hazard rule covers (a landing spot has air above, so its
/// ground is what can hurt), and the leaves and snow the
/// original skips as surfaces.
fn is_bad_ground(state: Option<&str>) -> bool {
    let Some(state) = state else { return true };
    let id = block_id(state);
    matches!(
        id,
        "lava"
            | "fire"
            | "soul_fire"
            | "magma_block"
            | "cactus"
            | "water"
            | "powder_snow"
            | "snow"
            | "snow_block"
    ) || id.ends_with("_leaves")
}

/// Whether a column read (top-down from `top`, one state per y)
/// has a landing spot the original accepts: solid ground that is
/// not a hazard, with two blocks of air above it. Returns the
/// feet y to stand at (one above the ground).
fn landing_in_column(states: &[Option<&str>], top: i32) -> Option<i32> {
    // i is the ground's index: the two above it must be air, and
    // the ground itself solid and not a hazard.
    for i in 2..states.len() {
        if is_air(states[i - 1].as_deref())
            && is_air(states[i - 2].as_deref())
            && !is_air(states[i].as_deref())
            && !is_bad_ground(states[i].as_deref())
        {
            return Some(top - i as i32 + 1);
        }
    }
    None
}

/// Whether the player stands on the Nether's bedrock roof: in
/// the Nether at or above the ceiling layer.
fn on_nether_roof(dimension: &str, y: f64) -> bool {
    dimension == "minecraft:the_nether" && y >= NETHER_ROOF_Y as f64
}

struct Stuck {
    /// The tick of the last use (a use spends the cooldown
    /// whether it found a spot or not, as the original's does —
    /// the command ran).
    last_use: Option<u64>,
    /// The current tick, kept from the tick hook for the command
    /// hook, which is not handed one.
    now: u64,
}

impl Stuck {
    /// Search for a landing spot the original accepts, around
    /// the player. Roof mode searches *down from under the
    /// ceiling* instead of from above the player.
    fn find_spot(&mut self, player: &game::Player) -> Option<(i32, i32, i32)> {
        let roof = on_nether_roof(&player.dimension, player.y);
        let clock = game::get_clock().map(|c| c.game_time).unwrap_or(0);
        let mut state = (player.x.to_bits() as u64)
            ^ (player.z.to_bits() as u64).rotate_left(32)
            ^ clock.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let (px, pz) = (player.x.floor() as i32, player.z.floor() as i32);
        // The scan window: from above the player down past the
        // plausible ground. On the roof the window is the same
        // height, hung under the ceiling.
        let top = if roof {
            NETHER_ROOF_Y - 1
        } else {
            player.y.floor() as i32 + SCAN_ABOVE
        };
        let depth = 128;
        for _ in 0..TRIES {
            let (dx, dz) = pick_offset(&mut state);
            let (x, z) = (px + dx, pz + dz);
            let positions: Vec<BlockPos> =
                (0..depth).map(|i| BlockPos { x, y: top - i, z }).collect();
            let results = world::get_blocks(&player.dimension, &positions);
            if results.iter().all(|b| b.state.is_none()) {
                continue; // unloaded column: the original skips it too
            }
            let states: Vec<Option<&str>> =
                results.iter().map(|b| b.state.as_deref()).collect();
            if let Some(y) = landing_in_column(&states, top) {
                return Some((x, y, z));
            }
        }
        None
    }

    fn use_once(&mut self) -> Result<Option<String>, String> {
        if let Some(last) = self.last_use {
            let elapsed = self.now.saturating_sub(last);
            if elapsed < COOLDOWN_TICKS {
                let left = (COOLDOWN_TICKS - elapsed).div_ceil(20);
                return Ok(Some(format!("Stuck?: cooldown, {left}s left")));
            }
        }
        let Some(player) = game::get_player() else {
            return Ok(Some("Stuck?: not in a world".to_string()));
        };
        self.last_use = Some(self.now);
        match self.find_spot(&player) {
            Some((x, y, z)) => {
                command::run(&format!("tp @s {x} {y} {z}"));
                Ok(Some(format!("Stuck?: moved to {x}, {y}, {z}")))
            }
            None => Ok(Some(
                "Stuck?: no safe spot found nearby (single player only in v1)".to_string(),
            )),
        }
    }
}

impl Mod for Stuck {
    fn init() -> Result<(Self, Registration), String> {
        let state = Stuck { last_use: None, now: 0 };
        Ok((state, Registration::new().command("stuck").command("escape").on_tick()))
    }

    fn tick(&mut self, tick: u64) {
        self.now = tick;
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        match name {
            "stuck" | "escape" => self.use_once(),
            _ => Err("This command does nothing".into()),
        }
    }
}

horizon_mod_sdk::export_mod!(Stuck);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landing_needs_solid_safe_ground_and_two_air() {
        let air = Some("minecraft:air");
        let stone = Some("minecraft:stone");
        // Top-down from y=50: air air air stone → feet at 48.
        assert_eq!(landing_in_column(&[air, air, air, stone], 50), Some(48));
        // Only one air above the ground: not a landing.
        assert_eq!(landing_in_column(&[stone, air, stone], 50), None);
        // All air: no ground.
        assert_eq!(landing_in_column(&[air, air, air], 50), None);
    }

    #[test]
    fn hazards_and_skips_are_not_ground() {
        let air = Some("minecraft:air");
        for bad in [
            "minecraft:lava",
            "minecraft:fire",
            "minecraft:magma_block",
            "minecraft:cactus",
            "minecraft:oak_leaves",
            "minecraft:snow",
            "minecraft:water",
        ] {
            assert_eq!(
                landing_in_column(&[air, air, air, Some(bad)], 50),
                None,
                "{bad} must not be a landing"
            );
        }
    }

    #[test]
    fn roof_rule_is_nether_and_high() {
        assert!(on_nether_roof("minecraft:the_nether", 130.0));
        assert!(on_nether_roof("minecraft:the_nether", 128.0));
        assert!(!on_nether_roof("minecraft:the_nether", 64.0));
        assert!(!on_nether_roof("minecraft:overworld", 130.0));
    }

    #[test]
    fn offsets_stay_inside_the_radius() {
        let mut state = 7u64;
        for _ in 0..200 {
            let (dx, dz) = pick_offset(&mut state);
            assert!((-SEARCH_RADIUS..=SEARCH_RADIUS).contains(&dx));
            assert!((-SEARCH_RADIUS..=SEARCH_RADIUS).contains(&dz));
        }
    }

    #[test]
    fn cooldown_default_is_the_originals() {
        assert_eq!(COOLDOWN_TICKS, 30 * 20);
    }
}

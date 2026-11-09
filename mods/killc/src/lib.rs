//! KillC for Kestrel: the original's three kill commands and its
//! config, over the one mechanism a client mod has — running
//! the vanilla commands as the player.
//!
//! Behaviour reimplemented from KillC by adasjusk (CC-BY-SA-4.0,
//! <https://github.com/adasjusk/KillC>), as described by the
//! Horizon registry entry for the port by Vakarux12; this file
//! shares no code with either. The original is a server plugin:
//! `/killc` and `/suicide` kill you, `/killc <player>` kills
//! another player (operator level 2), and after a death in the
//! Overworld the player respawns at a random surface spot within
//! `randomSpawnRadius` of (0, 0); `config/killc.json` holds
//! `enabled`, `useRandomSpawn` and `randomSpawnRadius`. This
//! port keeps the commands, the config (same keys and defaults,
//! in the mod's storage, set with `/killc config` /
//! `/killc radius`) and the random spot — found with the SDK's
//! world reads — but the mechanism differs where the SDK forces
//! it, and the README says so plainly: the kill is the vanilla
//! `kill` command run as the player (`command.run`), so the
//! player's own permissions decide, where the plugin simply did
//! it; and the "respawn" is a teleport issued right after the
//! kill command rather than a hook into the respawn itself (the
//! SDK has no death event), so it lands at spawn an instant
//! later — and runs, harmlessly and wrongly, even if the kill
//! was refused (the teleport asks with the same permissions).
//! This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/killc>.

use horizon_mod_sdk::{
    Mod, Registration, command, game, storage,
    world::{self, BlockPos},
};

/// The original's config defaults: enabled, random respawn on,
/// radius 100 blocks around (0, 0) (KillC's `config/killc.json`).
const DEFAULT_ENABLED: bool = true;
const DEFAULT_RANDOM: bool = true;
const DEFAULT_RADIUS: i32 = 100;
const ENABLED_KEY: &str = "enabled";
const RANDOM_KEY: &str = "random";
const RADIUS_KEY: &str = "radius";
/// How far down from the build ceiling the surface search looks:
/// every column's surface in a loaded chunk is found well inside
/// this many steps of 16.
const SURFACE_STEPS: i32 = 24;

/// A stored boolean; `default` when unreadable.
fn read_bool(key: &str, default: bool) -> bool {
    let Some(bytes) = storage::get(key) else { return default };
    match std::str::from_utf8(&bytes).map(str::trim) {
        Ok("true") => true,
        Ok("false") => false,
        _ => default,
    }
}

/// A stored radius; the default when unreadable or not positive.
fn read_radius(key: &str) -> i32 {
    let Some(bytes) = storage::get(key) else { return DEFAULT_RADIUS };
    let Ok(text) = std::str::from_utf8(&bytes) else { return DEFAULT_RADIUS };
    match text.trim().parse::<i32>() {
        Ok(v) if v > 0 => v,
        _ => DEFAULT_RADIUS,
    }
}

/// SplitMix64: a small deterministic PRNG. The mod has no clock
/// or entropy surface, so command runs seed it from the player's
/// position and the world clock — plumbing for the original's
/// "random spot", not a feature.
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A column inside the radius of (0, 0), from a random draw.
fn pick_column(state: &mut u64, radius: i32) -> (i32, i32) {
    let span = (2 * radius + 1) as u64;
    let x = (splitmix(state) % span) as i32 - radius;
    let z = (splitmix(state) % span) as i32 - radius;
    (x, z)
}

/// The block id inside a block state: `minecraft:oak_log[axis=y]`
/// reads as `oak_log`.
fn block_id(state: &str) -> &str {
    let id = state.split('[').next().unwrap_or(state);
    id.rsplit(':').next().unwrap_or(id)
}

/// Whether a block state is air, the surface search's "empty".
fn is_air(state: Option<&str>) -> bool {
    state.is_some_and(|s| {
        let id = block_id(s);
        id == "air" || id.ends_with("_air")
    })
}

/// The y to stand on in a column whose blocks were read top-down
/// starting at `top`: the first solid block with air above it,
/// as a feet position (one above the block). None when the
/// column is all air or all solid in the read range.
fn surface_y(states: &[Option<&str>], top: i32) -> Option<i32> {
    for (i, state) in states.iter().enumerate() {
        if !is_air(state.as_deref()) {
            return if i == 0 { None } else { Some(top - i as i32 + 1) };
        }
    }
    None
}

/// The vanilla command a kill runs, with or without a target.
fn kill_line(target: Option<&str>) -> String {
    match target {
        Some(name) => format!("kill {name}"),
        None => "kill @s".to_string(),
    }
}

/// A valid player name for `/killc <player>`: vanilla's name
/// alphabet, 3–16 of them — anything else could smuggle a second
/// command into the line the mod runs.
fn valid_name(name: &str) -> bool {
    (3..=16).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

struct Killc {
    enabled: bool,
    random: bool,
    radius: i32,
}

impl Killc {
    fn status(&self) -> String {
        format!(
            "KillC: {}, random respawn {}, radius {}",
            if self.enabled { "enabled" } else { "disabled" },
            if self.random { "on" } else { "off" },
            self.radius
        )
    }

    /// Issue the original's random respawn as this port can: a
    /// teleport to a random surface spot within the radius of
    /// (0, 0), run as the player. World reads are single-player
    /// only in v1; elsewhere no spot is found and nothing runs.
    fn random_respawn(&self) {
        let Some(player) = game::get_player() else { return };
        if player.dimension != "minecraft:overworld" {
            return;
        }
        let clock = game::get_clock().map(|c| c.game_time).unwrap_or(0);
        let mut state = (player.x.to_bits() as u64)
            ^ (player.z.to_bits() as u64).rotate_left(32)
            ^ clock.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        for _ in 0..16 {
            let (x, z) = pick_column(&mut state, self.radius);
            let top = 320;
            let positions: Vec<BlockPos> = (0..SURFACE_STEPS)
                .map(|i| BlockPos { x, y: top - i * 16, z })
                .collect();
            let results = world::get_blocks(&player.dimension, &positions);
            let coarse: Vec<Option<&str>> =
                results.iter().map(|b| b.state.as_deref()).collect();
            let Some(band) = coarse.iter().position(|s| !is_air(s.as_deref())) else {
                continue;
            };
            if band == 0 {
                continue;
            }
            let band_top = top - (band as i32 - 1) * 16;
            let positions: Vec<BlockPos> =
                (0..16).map(|i| BlockPos { x, y: band_top - i, z }).collect();
            let results = world::get_blocks(&player.dimension, &positions);
            let fine: Vec<Option<&str>> =
                results.iter().map(|b| b.state.as_deref()).collect();
            if let Some(y) = surface_y(&fine, band_top) {
                command::run(&format!("tp @s {x} {y} {z}"));
                return;
            }
        }
    }
}

impl Mod for Killc {
    fn init() -> Result<(Self, Registration), String> {
        let state = Killc {
            enabled: read_bool(ENABLED_KEY, DEFAULT_ENABLED),
            random: read_bool(RANDOM_KEY, DEFAULT_RANDOM),
            radius: read_radius(RADIUS_KEY),
        };
        Ok((state, Registration::new().command("killc").command("suicide")))
    }

    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        let args = args.trim();
        match name {
            "suicide" => {
                if !args.is_empty() {
                    return Err("Usage: /suicide".into());
                }
                if !self.enabled {
                    return Ok(Some("KillC is disabled".to_string()));
                }
                command::run(&kill_line(None));
                if self.random {
                    self.random_respawn();
                }
                Ok(None)
            }
            "killc" => {
                if args.is_empty() {
                    // The original's bare /killc kills; the
                    // config lives at /killc config.
                    if !self.enabled {
                        return Ok(Some("KillC is disabled".to_string()));
                    }
                    command::run(&kill_line(None));
                    if self.random {
                        self.random_respawn();
                    }
                    return Ok(None);
                }
                if args == "config" {
                    return Ok(Some(self.status()));
                }
                if let Some(rest) = args.strip_prefix("config ") {
                    return match rest {
                        "on" => {
                            self.enabled = true;
                            let _ = storage::set(ENABLED_KEY, b"true");
                            Ok(Some("KillC: enabled".to_string()))
                        }
                        "off" => {
                            self.enabled = false;
                            let _ = storage::set(ENABLED_KEY, b"false");
                            Ok(Some("KillC: disabled".to_string()))
                        }
                        "random on" => {
                            self.random = true;
                            let _ = storage::set(RANDOM_KEY, b"true");
                            Ok(Some("KillC: random respawn on".to_string()))
                        }
                        "random off" => {
                            self.random = false;
                            let _ = storage::set(RANDOM_KEY, b"false");
                            Ok(Some("KillC: random respawn off".to_string()))
                        }
                        _ => Err(
                            "Usage: /killc config, /killc config on|off, /killc config random on|off"
                                .into(),
                        ),
                    };
                }
                if let Some(rest) = args.strip_prefix("radius ") {
                    return match rest.trim().parse::<i32>() {
                        Ok(v) if v > 0 => {
                            self.radius = v;
                            let _ = storage::set(RADIUS_KEY, v.to_string().as_bytes());
                            Ok(Some(format!("KillC: radius {v}")))
                        }
                        _ => Err("Usage: /killc radius <blocks>".into()),
                    };
                }
                if valid_name(args) {
                    if !self.enabled {
                        return Ok(Some("KillC is disabled".to_string()));
                    }
                    // The original asks operator level 2 of the
                    // typist; here the kill runs as the player,
                    // and the server applies its own rules.
                    command::run(&kill_line(Some(args)));
                    return Ok(None);
                }
                Err("Usage: /killc, /killc <player>, /killc config, /killc radius <blocks>".into())
            }
            _ => Err("This command does nothing".into()),
        }
    }
}

horizon_mod_sdk::export_mod!(Killc);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_lines_match_vanilla() {
        assert_eq!(kill_line(None), "kill @s");
        assert_eq!(kill_line(Some("Steve")), "kill Steve");
    }

    #[test]
    fn names_are_vanilla_names() {
        assert!(valid_name("Steve"));
        assert!(valid_name("xX_player_99"));
        assert!(!valid_name("ab"));
        assert!(!valid_name("a name"));
        assert!(!valid_name("a;kill"));
        assert!(!valid_name("@a"));
        assert!(!valid_name("seventeen_chars_x"));
    }

    #[test]
    fn columns_stay_inside_the_radius() {
        let mut state = 42u64;
        for _ in 0..200 {
            let (x, z) = pick_column(&mut state, 100);
            assert!((-100..=100).contains(&x));
            assert!((-100..=100).contains(&z));
        }
        let (x, z) = pick_column(&mut state, 1);
        assert!((-1..=1).contains(&x) && (-1..=1).contains(&z));
    }

    #[test]
    fn surface_is_the_first_solid_with_air_above() {
        let air = Some("minecraft:air");
        let stone = Some("minecraft:stone");
        // Read top-down from y=100: air, air, stone → stand on 99.
        assert_eq!(surface_y(&[air, air, stone], 100), Some(99));
        assert_eq!(surface_y(&[air, air], 100), None);
        assert_eq!(surface_y(&[stone, stone], 100), None);
        assert_eq!(surface_y(&[None, stone], 100), None);
    }

    #[test]
    fn config_defaults_are_the_originals() {
        assert!(DEFAULT_ENABLED);
        assert!(DEFAULT_RANDOM);
        assert_eq!(DEFAULT_RADIUS, 100);
    }
}

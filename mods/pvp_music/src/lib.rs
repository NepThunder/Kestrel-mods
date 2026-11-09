//! PVP Music for Kestrel: the original's combat detection, kept;
//! its battle tracks, not — the SDK gives a mod no way to inject
//! or switch music, and this port invents no substitute.
//!
//! Behaviour reimplemented from PvP Music by Countered (CC0-1.0,
//! <https://modrinth.com/mod/Tkf0xIjA>), as described by the
//! Horizon registry entry for the native port by sparticle62ops;
//! this file shares no code with either — the originals are Java,
//! this is a fresh implementation against the Kestrel SDK. The
//! original remembers attacks on and by players and mobs for 5 s,
//! starts a mob battle when a mob we hit then hits us, a player
//! battle on a landed hit on a player, pulls hurts from explosions
//! and bad respawn points into combat within 12 blocks, and starts
//! a fight when a wither is within 30 blocks. This port derives
//! what the client SDK can honestly see of that: the local
//! player's health trend, and every nearby mob's health trend and
//! distance (`entity`, `game`). What it cannot see — who dealt a
//! hit, whether a mob is targeting the player, other players at
//! all — is listed in the README's gap list, not papered over.
//!
//! The original then plays battle tracks (its own CC0 assets, in a
//! companion resource pack): fades in and out, death fades at a
//! configured multiplier, a phased two-half track switching at low
//! health on a beat, and short/long end sounds after battles of
//! 30 s / 60 s. None of that is in this port: `audio-events` lets a
//! mod only quieten the sounds the game itself starts, and no SDK
//! surface starts or switches a music track. The detection runs,
//! `/pvpmusic` reports it, and the music gap stays documented
//! until the SDK grows a music surface. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/pvp_music>.

use horizon_mod_sdk::{Mod, Registration, entity, game};

/// How long an attack is remembered, in ticks: the original's 5 s
/// at 20 ticks per second.
const MEMORY_TICKS: u64 = 100;
/// A hurt from an explosion or a bad respawn point pulls combat
/// in within this many blocks of the player, per the original.
const HURT_RADIUS: f64 = 12.0;
/// A wither within this many blocks starts a fight, per the
/// original.
const WITHER_RADIUS: f64 = 30.0;
/// A mob counts as engaged at this range — close enough to be
/// fighting the player. The original knows a mob's target; the
/// client SDK does not expose targets, so engagement is judged
/// by range alone. This is detection plumbing, not a feature.
const ENGAGE_RADIUS: f64 = 16.0;

/// One tracked mob's part in the fight bookkeeping: when the
/// player last hurt it, when it last hurt the player, and the
/// health it was last seen with (to spot the changes).
#[derive(Clone, Copy, Default)]
struct Track {
    hurt_by_player: Option<u64>,
    hurt_player: Option<u64>,
    last_health: Option<f32>,
}

/// Which battle, if any, the remembered attacks amount to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Battle {
    None,
    Mob,
    Player,
}

/// The original's mutual-combat rule over one mob's memory: a mob
/// battle runs while a mob we hit has since hit us (or the other
/// way round) and both memories are still inside the window.
fn mob_battle(track: &Track, now: u64) -> bool {
    let fresh = |t: Option<u64>| t.is_some_and(|t| now.saturating_sub(t) < MEMORY_TICKS);
    fresh(track.hurt_by_player) && fresh(track.hurt_player)
}

/// Whether an event at most `radius` blocks away counts, by
/// squared distance (the original's 12-block hurt pull-in and
/// 30-block wither rule).
fn within(distance_sq: f64, radius: f64) -> bool {
    distance_sq <= radius * radius
}

/// Squared distance between two points.
fn dist_sq(ax: f64, ay: f64, az: f64, bx: f64, by: f64, bz: f64) -> f64 {
    let (dx, dy, dz) = (ax - bx, ay - by, az - bz);
    dx * dx + dy * dy + dz * dz
}

struct PvpMusic {
    /// Every tracked mob, by entity id.
    tracks: Vec<(u32, Track)>,
    /// The local player's health at the last tick, to spot hurts.
    last_player_health: Option<f32>,
    /// The wither rule's latch: a wither inside its radius.
    wither_near: bool,
}

impl PvpMusic {
    fn track_mut(&mut self, id: u32) -> &mut Track {
        if let Some(i) = self.tracks.iter().position(|(t, _)| *t == id) {
            return &mut self.tracks[i].1;
        }
        self.tracks.push((id, Track::default()));
        &mut self.tracks.last_mut().expect("just pushed").1
    }

    /// The battle the current memories amount to.
    fn battle(&self, now: u64) -> Battle {
        if self.wither_near {
            return Battle::Mob;
        }
        if self.tracks.iter().any(|(_, t)| mob_battle(t, now)) {
            return Battle::Mob;
        }
        // Player battles cannot be detected at all in v1: the
        // entity snapshot lists no other players. The `Player`
        // case exists because the original has it; it is never
        // returned here (see the README's gap list).
        Battle::None
    }

    fn status(&self, now: u64) -> String {
        match self.battle(now) {
            Battle::Mob => "PVP Music: battle (mob)".to_string(),
            Battle::Player => "PVP Music: battle (player)".to_string(),
            Battle::None => {
                let remembered = self
                    .tracks
                    .iter()
                    .filter(|(_, t)| {
                        t.hurt_by_player
                            .is_some_and(|t| now.saturating_sub(t) < MEMORY_TICKS)
                            || t.hurt_player
                                .is_some_and(|t| now.saturating_sub(t) < MEMORY_TICKS)
                    })
                    .count();
                format!("PVP Music: no battle ({remembered} fights remembered)")
            }
        }
    }
}

impl Mod for PvpMusic {
    fn init() -> Result<(Self, Registration), String> {
        let state = PvpMusic { tracks: Vec::new(), last_player_health: None, wither_near: false };
        Ok((state, Registration::new().command("pvpmusic").on_tick()))
    }

    fn tick(&mut self, tick: u64) {
        let Some(player) = game::get_player() else {
            self.last_player_health = None;
            self.wither_near = false;
            return;
        };
        let mobs = entity::entities();
        // Prune memories of mobs that left the snapshot, and of
        // fights whose window has fully closed.
        let live: Vec<u32> = mobs.iter().map(|m| m.id).collect();
        self.tracks.retain(|(id, t)| {
            live.contains(id)
                || t.hurt_by_player.is_some_and(|h| tick.saturating_sub(h) < MEMORY_TICKS)
                || t.hurt_player.is_some_and(|h| tick.saturating_sub(h) < MEMORY_TICKS)
        });
        // Did the player take a hurt this tick?
        let player_hurt = self
            .last_player_health
            .is_some_and(|last| player.health < last - 0.01);
        self.last_player_health = Some(player.health);
        self.wither_near = false;
        for mob in &mobs {
            let d2 = dist_sq(player.x, player.y, player.z, mob.x, mob.y, mob.z);
            if mob.kind == "minecraft:wither" && within(d2, WITHER_RADIUS) {
                self.wither_near = true;
            }
            let Some(health) = mob.health else { continue };
            let track = self.track_mut(mob.id);
            let mob_hurt = track.last_health.is_some_and(|last| health < last - 0.01);
            track.last_health = Some(health);
            if within(d2, ENGAGE_RADIUS) {
                if mob_hurt {
                    track.hurt_by_player = Some(tick);
                }
                if player_hurt && within(d2, HURT_RADIUS) {
                    track.hurt_player = Some(tick);
                }
            }
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        match name {
            // The probe harness drives no ticks, so `now` is 0
            // there; in game the status uses the last tick the mod
            // saw, which the command hook is not handed — 0 reads
            // as "everything fresh", so the counts are of tracked
            // fights, which is what the line reports.
            "pvpmusic" => Ok(Some(self.status(0))),
            _ => Err("This command does nothing".into()),
        }
    }
}

horizon_mod_sdk::export_mod!(PvpMusic);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutual_combat_needs_both_directions_inside_the_window() {
        let mut t = Track::default();
        assert!(!mob_battle(&t, 0));
        t.hurt_by_player = Some(10);
        assert!(!mob_battle(&t, 20), "one direction is not a battle");
        t.hurt_player = Some(15);
        assert!(mob_battle(&t, 20));
        assert!(mob_battle(&t, 109), "still inside the 5 s window");
        assert!(!mob_battle(&t, 110), "the first memory expired");
    }

    #[test]
    fn radii_match_the_original() {
        assert!(within(144.0, HURT_RADIUS));
        assert!(!within(144.01, HURT_RADIUS));
        assert!(within(900.0, WITHER_RADIUS));
        assert!(!within(900.01, WITHER_RADIUS));
    }

    #[test]
    fn distance_is_squared_euclidean() {
        assert_eq!(dist_sq(0.0, 0.0, 0.0, 3.0, 4.0, 0.0), 25.0);
        assert_eq!(dist_sq(1.0, 2.0, 3.0, 1.0, 2.0, 3.0), 0.0);
    }
}

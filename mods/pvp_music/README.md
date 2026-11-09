# PVP Music

The combat detection of PvP Music, ported; its battle tracks are a documented gap (below), not a substitute feature.

A behaviour port of PvP Music by Countered (CC0-1.0), following the description of the Horizon registry port by sparticle62ops. It shares no code or assets with either: the originals are Java, and this is a fresh implementation against the Kestrel SDK (capabilities `entity`, `game-read`, `audio`).

## What the original does

- Remembers attacks on and by players and mobs for 5 seconds; a mob we hit that then hits us starts a mob battle, and a landed hit on a player starts a player battle. Hurts from explosions and bad respawn points pull in combat within 12 blocks, and a wither within 30 blocks starts a fight.
- Plays battle tracks from its own CC0 sound pack: the track fades in and out, death fades run at a configured multiplier, the phased two-half track switches at low health on a beat, and battles of 30 s or more end with the short end sound, of 60 s or more with the long end sound.
- `/pvpmusic` drives every setting.

## What this port does

- Runs the detection it can honestly derive on a client: the local player's health trend, plus every nearby mob's health trend and distance, tracked per mob with the original's 5-second mutual-combat window, the 12-block hurt rule and the 30-block wither rule.
- `/pvpmusic` reports the state: in a mob battle, or how many fights are still remembered.

## Gaps — features the SDK cannot express yet

- **No battle music.** A mod can only *quieten* the sounds the game itself starts (`audio-events`); nothing in the SDK starts, injects or switches a music track, and the original's tracks are separate sound assets a `.kmod` cannot carry. The fades, the death fades, the two-half low-health switch and the end sounds all depend on that, so none of them are here. This is the mod's headline feature and it is missing in full — the port ships the detection, not a reimagining of the music.
- **No player battles.** The client entity snapshot lists no other players, and nothing exposes another player's health or hits, so the player-battle half of the detection cannot run. Mob detection in multiplayer can't run either: the entity list is single-player only in v1 of the SDK.
- **No hit attribution.** The original knows who dealt each hit; the client sees only health changes. A mob hurt by anything (a fall, another mob, the player) while close counts as hurt-by-player, and a player hurt with several mobs near credits them all within 12 blocks. A mob's *target* is likewise not exposed, so engagement is judged by range (16 blocks) instead.
- **No settings page.** The original's `/pvpmusic` drives its music settings; with the music unportable there are no settings to drive, so the command reports state instead of opening configuration.

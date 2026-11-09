# Stuck?

/stuck and /escape: a search of random nearby columns for solid ground with two blocks of air above, and a move there — off the Nether roof too, with the original's 30-second cooldown.

A behaviour port of Stuck? by adasjusk (CC-BY-SA-4.0), following the description of the Horizon registry port by Vakarux12. It shares no code with either: the originals are Java, and this is a fresh implementation against the Kestrel SDK (capabilities `command`, `game-read`, `world`).

## What the original does

- `/stuck` and `/escape` search random columns near you for solid ground with two blocks of air above, and teleport you there.
- Skips lava, fire, magma, cactus and other hazards, and leaves and snow.
- A player on the Nether's bedrock roof is brought back under it.
- A cooldown between uses, 30 seconds by default; only loaded chunks are searched.
- It is a server plugin: it reads the server's world and moves the player itself.

## What this port does

- The same commands, search rules and 30-second cooldown: random columns around you are read top-down; the first with air, air, then solid non-hazard ground wins, and you are moved to stand on it. Hazards skipped are lava, fire and soul fire, magma, cactus, water and powder snow; leaves and snow are skipped as surfaces, as the original skips them.
- On the Nether roof (in the Nether at y 128 or above) the search hangs its window under the ceiling instead, bringing you back into the cave space below.
- Unloaded columns are skipped, as the original skips them.

## Gaps — where the SDK changes the mechanism

- **The move runs with your permissions.** The original moves the player because it is the server. This port issues the vanilla `tp @s …` command as you, so a server that denies you teleports — most multiplayer servers, for non-operators — refuses the move (the search still runs and reports the spot it found). In single player and LAN worlds you host, your permissions are the world's.
- **The search is single-player only.** It reads the world through the client `world` capability, which the SDK serves only in single player in v1. In multiplayer the command finds nothing and says so.
- **The search radius is this port's number.** The original searches "near you"; its properties file's radius is not in the registry description this port was written from, so the port searches within 32 blocks and says so here rather than inventing a claim about the original. Its config file (`config/stuck.properties`) likewise does not carry over: the cooldown is the original's default 30 seconds, fixed, with no config surface.

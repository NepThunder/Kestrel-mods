# KillC

/killc, /killc <player> and /suicide, with the original's config and its random respawn — run as vanilla commands through the player, which is the one mechanism a client mod has.

A behaviour port of KillC by adasjusk (CC-BY-SA-4.0), following the description of the Horizon registry port by Vakarux12. It shares no code with either: the originals are Java, and this is a fresh implementation against the Kestrel SDK (capabilities `command`, `game-read`, `world`, `storage`).

## What the original does

- `/killc` and `/suicide` kill you; `/killc <player>` kills another player (operator level 2). It is a server plugin: its word is law.
- After a death in the Overworld, players respawn at a random surface spot within `randomSpawnRadius` of (0, 0).
- Config in `config/killc.json`: `enabled`, `useRandomSpawn`, `randomSpawnRadius`.

## What this port does

- The same three commands, as wrappers: each issues the vanilla `kill` command as you (`kill @s`, `kill <player>`), through the SDK's command surface.
- The config with the original's keys and defaults — enabled, random respawn on, radius 100 — held in the mod's storage and set in game: `/killc config` reads it, `/killc config on|off`, `/killc config random on|off`, `/killc radius <blocks>`.
- The random respawn: after a self-kill with random respawn on, a random surface spot within the radius of (0, 0) is found with the SDK's world reads and you are teleported there (`tp @s …`, also as you).

## Gaps — where the SDK changes the mechanism

- **Permissions are the server's, not the plugin's.** The original kills because it runs on the server. This port asks: the kill (and the respawn teleport) run as the player, so a server that denies you `kill`/`tp` — most multiplayer servers, for non-operators — refuses, and the command does nothing. In single player and LAN worlds you host, your own permissions are the world's.
- **The random respawn is a teleport after the kill, not a respawn hook.** The SDK has no death event, so the port cannot hook the respawn itself: it issues the teleport right after the kill command, the commands run in order, and you land at the random spot instead of world spawn. Two consequences: if the kill is refused but the teleport is allowed, you are moved without dying; and the original applies random respawn after *every* Overworld death, while this port applies it only after its own `/killc` and `/suicide` — deaths from falling, lava or mobs respawn at world spawn as usual.
- **Random respawn is single-player only.** It needs world reads, which the SDK serves only in single player in v1; in multiplayer no spot is found and no teleport is issued.
- **`/killc <player>` has no random-respawn follow-up** — the original respawns the *victim* randomly, and a client mod cannot observe or move another player.

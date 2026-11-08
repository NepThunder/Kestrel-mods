# Entity Probe

Prototype for client entity reads: shows the living mobs around you and
the mob under your crosshair, in a HUD panel at the top-left corner.

## The panel

- First line: the crosshair target —
  `Target: Creeper (minecraft:creeper) 2.2 blocks 20.0/20.0` — or
  `Target: none` when nothing is under the crosshair.
- Then at most the **five nearest mobs**, one line each: name, kind,
  feet distance, health, hitbox size, velocity, yaw, and the baby and
  on-ground flags. When more mobs are around, the last line counts them
  instead of listing them: `+10 more nearby`.
- `no mobs` when nothing is in range.

## Requirements

Needs a Kestrel build with the SDK entity interface (API level 10,
build 9+). `entities()` is singleplayer-only; the crosshair target
works in multiplayer too, where health reads as `?`.

A diagnostic probe, not a player-facing mod: it exists to prove the
entity data the SDK exposes, and to show what a mod can do with it.

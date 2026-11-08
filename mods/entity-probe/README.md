# Entity Probe

Prototype for client entity reads: shows the living mobs around you and
the mob under your crosshair, in a HUD panel at the top-left corner.

## The panel

- First line: the crosshair target —
  `Target: Creeper (minecraft:creeper) 2.2 blocks 20.0/20.0` — or
  `Target: none` when nothing is under the crosshair. The target line
  is the same in both detail modes.
- Then the nearest mobs, one line each. Two settings in
  Options > Mods > Entity Probe control the list, and the game keeps
  them in the mod's storage:
  - **Mobs shown** cycles 5 → 10 → 15 → All → 5 (default 5). When
    more mobs are around than the list shows, the last line counts
    them instead of listing them: `+10 more nearby`. All lists every
    mob, with no count line.
  - **Detail** toggles Short / Long (default Short).
    - Short — only the important facts: name, kind, feet distance,
      health, and `baby` when the mob is a baby:
      `Zombie (minecraft:zombie) 5.0 14.0/20.0`.
    - Long — the full line: name, kind, feet distance, health,
      hitbox size, velocity, yaw, and the baby and on-ground flags:
      `Zombie (minecraft:zombie) 5.0 14.0/20.0 0.60x1.95 v=0.0,-0.1,0.0 yaw=0.0 ground`.
- `no mobs` when nothing is in range.

## Requirements

Needs a Kestrel build with the SDK entity interface (API level 10,
build 9+). `entities()` is singleplayer-only; the crosshair target
works in multiplayer too, where health reads as `?`.

A diagnostic probe, not a player-facing mod: it exists to prove the
entity data the SDK exposes, and to show what a mod can do with it.

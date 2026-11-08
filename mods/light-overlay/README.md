# Light Overlay

A port of [Light Overlay](https://github.com/CayMac97/light-overlay)
by CayMac, MIT — MiniHUD's F7 light level overlay for Horizon:
scan the block tops around you and mark where hostile mobs can
spawn. Rewritten in Rust against the Kestrel SDK; no original code
runs. Not made or endorsed by CayMac, so report problems to the
Kestrel-mods project, not to them.

## What was ported

- **Spawnable-block crosses in world space**: a red cross where
  hostile mobs can spawn at any time (caves, indoors, anywhere the
  day light is 7 or less), a yellow cross where they can spawn at
  night only. The crosses are depth-tested, so a wall hides them,
  and they hide with the HUD (F1).
- **The Modern spawn rule (default)**: the vanilla rule since 1.18 —
  block light must be 0, then the effective light
  `max(block, sky - 11)` at night decides. The Legacy rule
  (pre-1.18, the effective light alone) is on the settings page.
- **The Nether rule**: no sky light and no day or night there, so a
  surface with block light 7 or less draws red and there is no
  yellow. In the End light does not gate endermen, so nothing draws.
- **Safe blocks (off by default)**: green crosses where mobs cannot
  spawn, on the settings page.
- **The scan radius switch**: 8, 12, 16 (default) or 20 blocks.
- **The on/off switch three ways**: F7 (rebindable on the mod's page
  in Options > Mods), `/lightoverlay`, and the switch on that page.
  The page also cycles the radius, the safe-blocks switch and the
  spawn rule; the game keeps the choices.
- **The counts panel**: red and yellow counts (plus safe when the
  safe switch is on), the radius and the rule, bottom-left.

## What is missing

The SDK does not expose these, so this port has neither:

- **The light level as a number on the block top** — there is no
  world-text capability yet, so this port draws colour-coded crosses
  only. The original draws the 0–15 number, or crosses and numbers
  together; the counts live in the corner panel instead.
- **The Green-marked safe blocks by default** — the original offers
  them as an option too; here the switch is off by default because
  every safe surface in a 16-block radius would fill the 512-cross
  budget before the red and yellow ones draw.

Two limits come from the scan itself, not the API:

- **A surface is only marked when the two blocks above it are air.**
  A mob needs head room, so a surface under leaves, a torch or tall
  grass gets no cross — the original, which reads the game's own
  spawn check, agrees.
- **The scan window is 16 blocks above to 32 below your feet, and a
  cold scan takes about seven ticks.** The scan runs at 2 Hz and is
  spread across ticks (one tick probes the cached surfaces, then two
  four-level bands a tick), so a surface outside the window is not
  marked until you move nearer to its level. Slabs, stairs and other
  partial blocks are treated as solid, except a bottom slab.
- **At most 512 crosses draw.** Red first, then yellow, then green,
  each nearest first.
- **Multiplayer shows nothing.** World reads are singleplayer-only
  in v1, so on a server the scan finds no surfaces.

## Needs

API level 12 (`world.get-lights`) — Kestrel build 18 or later.

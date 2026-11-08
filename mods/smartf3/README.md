# SmartF3

A tidier, configurable F3 debug screen: the same information grouped into
modules you can turn off, in one panel in the top-left corner. A port of
[SmartF3](https://github.com/d3grando/smartf3) by d3grando (MIT), ported
for Kestrel by NepThunder. Inspired by BetterF3; not affiliated.

## The panel

One line per module, in this order, when the module is on (all are on by
default):

- **FPS** — `120 fps  min 41  1% 52`: frames in the last second, the
  slowest frame and the 1% low of the last 5 seconds, from
  `render.frame-stats`, as the FPS counter shows them.
- **Coordinates** — `XYZ 0.5 / 64.0 / 0.5`: the feet position.
- **Facing** — `Facing South (+Z)  yaw 0.0  pitch 0.0`: the direction
  name and the raw yaw and pitch, as F3 shows them.
- **World** — `Dimension minecraft:overworld` and `Day 1, 06:00`: the
  dimension and the world clock.
- **Player** — `Health 20.0/20.0` and `Food 20`.
- **Target block** — the block under the crosshair: its name in your
  language, its id, and its distance with its coordinates
  (`Distance 40.5  at 12, 63, -44`); `Target block: none` when nothing
  is in reach (64 blocks).
- **Target entity** — the mob under the crosshair:
  `Target: Zombie (minecraft:zombie) 4.7 blocks 14.0/20.0`; `Target: none`
  when nothing is under it. In multiplayer the client cannot vouch for
  the mob's health, so the line carries no health at all:
  `Target: Zombie (minecraft:zombie) 4.7 blocks`.

`/smartf3` hides or shows the panel, and Options > Mods > SmartF3 has a
switch for the panel and one per module; the game keeps the choices in
the mod's storage.

## Requirements

Needs a Kestrel build with the SDK entity interface (API level 10,
build 9+). The targeted-entity module works in multiplayer too; the
health line is singleplayer-only, as above.

## What is missing, and why

The original has 14 modules. Six of them are in this port — FPS,
coordinates, facing, world, targeted block and targeted entity — plus
the player's health and food, which `game-read` exposes alongside the
position. The other eight cannot be built on the SDK as it stands:

- **Server** — no server information is exposed to a client mod: no
  server address, no ping, no player count. The `players` capability is
  server-mods only, and a client mod has no interface that names the
  server it is connected to.
- **Chunk** — the chunk the player stands in and its loading state are
  not exposed. (Chunk coordinates are arithmetic on the position, but
  the module is the chunk's load state as well, which the SDK does not
  expose, so the module is left out whole rather than half-built.)
- **Biome** — `world.get-biome` exists, but only for a client mod in
  singleplayer and only where a chunk is loaded, and the port does not
  take the `world` capability for one line; the biome module stays out
  until a world-reading module needs that capability anyway.
- **Light** — block light and sky light at the player's position are
  not exposed by any SDK interface.
- **System** — CPU, GPU, memory and Java version are not exposed by
  any SDK interface.
- **Rendering** — render distance, chunk counts, entity counts and the
  graphics pipeline state are not exposed by any SDK interface.
- **Fluid** — the targeted fluid is not exposed: `game.raycast` passes
  through fluids, so looking at water or lava reports the block behind
  it, or nothing.
- **Chart keys** — the F3 charts (pie chart, frame-time graph) and
  their keys are the vanilla debug screen's own UI, which a sandboxed
  mod cannot draw or drive.

Also deliberately not ported: per-module label and value colours, left
or right column choice, module reordering, text shadow, background
styles (vanilla, dark, none), the gap between modules and the compact
layout. The SDK's HUD panel is one panel of plain white text lines in a
corner — a mod says what the lines hold, never where they sit or how
they look — so none of that styling survives. The panel is always
top-left, always the same order, always plain text.

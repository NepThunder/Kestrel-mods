# Waypoints

A port of [Waypoints](https://github.com/Nirokx/horizon-waypoints) by
Nirokx — place waypoints, manage them, and find your way back to them.
Rewritten in Rust against the Kestrel SDK; no original code runs, though
the storage format, the 16-colour palette, the distance wording and the
death-waypoint rules are the original's, adapted to the SDK's storage.
Not made or endorsed by Nirokx, so report problems to the Kestrel-mods
project, not to them. Licence: MIT, the same licence as Waypoints.

## What was ported

- **N creates a waypoint where you stand**: auto-named ("Waypoint 1",
  "Waypoint 2", …), coloured from the palette in order, in your current
  dimension, visible. Chat confirms it, e.g. `Waypoint "Home" created
  at 0 64 0 (Overworld)`.
- **M lists them**: the waypoint list with All / Overworld / Nether /
  End filters. Each row shows its number, name, coordinates, dimension
  and distance — or "other dimension" when you are elsewhere. Pick a row
  for Hide/Show, Colour (cycles the palette), Move here and Delete
  (click twice to confirm).
- **In the world**: each visible waypoint of your dimension shows a
  block-sized box in its colour plus a beam 32 blocks above it, drawn
  with the new `render.set-world` (API level 11). The markers are
  depth-tested, so a wall hides them, and they hide with the HUD (F1).
  Each one also carries an in-world label — its name and distance as
  HUD text at its projected position (`render.project`, API level 14),
  e.g. `Base Camp · 12 m`, floating just above its box, nearest
  sixteen at most. A label has no occlusion test, so it shows through
  walls, as the panel's distance already does; a waypoint behind the
  camera gets no label. Labels follow the world-markers switch:
  turning markers off hides them too.
- **HUD distance panel**: the top-left panel lists each visible
  waypoint of your dimension with its name and distance, nearest first,
  e.g. `Base Camp · 12 m` — `1.2 km` from a kilometre on.
- **Dying leaves a waypoint**: a red `Death 1`, `Death 2`, … waypoint
  marks the block you died on. The newest 5 stay, and the oldest goes
  when you die again; rename one to keep it for good.
- **`/wp` commands**: `add [name]`, `list`, `rename <n> <name>`,
  `delete <n>`, `hide <n>`, `show <n>`, `color <n> <colour>`,
  `move <n>`, `dim <n> <overworld|nether|end>`, `help`. Colours take a
  palette name (`red`, `dark-blue`, …) or a hex (`#FF55FF`).
- **Options > Mods > Waypoints**: both keys are rebindable there, and
  the page has switches for the world markers, the beams, the distance
  panel and the death waypoints. The game keeps your choices.

## What is missing

- **The editor with text fields** — the original's create/edit form
  (name, X/Y/Z, colour swatches, dimension, visible) needs text input,
  which the SDK's widgets do not offer. N creates a waypoint at once
  with a suggested name; `/wp rename`, `move`, `color` and `dim`
  and the list's Colour and Move here buttons do the editing instead.
- **Per-world storage** — the original keeps a file per single-player
  world folder and per server address. The SDK exposes no world folder
  or server address, so every world shares one list, filtered by
  dimension: a waypoint made in one world shows in another world's list
  when the dimension matches.
- **Death waypoints are inferred, not told** — the SDK exposes no death
  event, so a death is inferred from the player's health reaching zero
  and the waypoint marks where the player last stood alive. If the game
  ever hides the player record while the death screen is up, no death
  waypoint is made.
- **Minimap dots** — the minimap API has no waypoint overlay, so there
  are none.
- **The 2D screen marker dot** — replaced by the 3D box and beam plus
  the projected in-world label.

## Use

Download `waypoints.kmod`, import it in Kestrel (Mods → Import file),
enable it. It needs SDK level 14 (`render.project`, Kestrel build
24+).

## Build

Built like the other mods in this repo: copy this folder to
`crates/horizon-mod-sdk/examples/waypoints/` in the Kestrel tree, add
it as a workspace member, and run `cargo horizon-mod test` then
`cargo horizon-mod build` there; the pack tool writes
`waypoints.kmod`.

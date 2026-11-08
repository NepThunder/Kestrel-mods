# Hitbox+

A port of [Hitbox+](https://github.com/taxtii/hitbox_plus) by taxtii and
itzglecni, MIT — the game's F3+B hitboxes as a customizable PvP aid.
Rewritten in Rust against the Kestrel SDK; no original code runs. Not
made or endorsed by taxtii, so report problems to the Kestrel-mods
project, not to them.

## What was ported

- **Entity hitboxes in world space**: a box round every living mob
  within 128 blocks (nearest first), from the mob's position and
  hitbox width/height. The boxes are depth-tested, so a wall hides
  them, and they hide with the HUD (F1).
- **In-reach colour**: the box round the mob under your crosshair takes
  bright green while it is within attack reach — 3 blocks for swords,
  axes, maces and fists, 4.5 blocks for a spear (the held item decides).
  It works in singleplayer and multiplayer, and on players as well as
  mobs.
- **Four hitbox styles**: Neon Cyan (default), Gold Yellow, Crimson
  Red, White.
- **Three thicknesses**: Thin (1.5px), Normal (2.5px, default), Thick
  (4.0px).
- **Eye rays**: a red band at eye level and a blue look-direction
  arrow, two blocks long, per mob — both on by default, switchable.
- **Sprint HUD**: `[Sprinting (Key)]` shows in the corner you pick
  (top-left by default) while the sprint key is held, `[Sneaking
  (Key)]` while the sneak key is held.
- **The on/off switch three ways**: H (rebindable on the mod's page in
  Options > Mods), `/hitbox`, and the switch on that page. The page
  also cycles the colour, thickness, eye ray, sprint display and sprint
  position; the game keeps the choices. (The original keeps its settings
  only until the game closes; this port stores them.)

## What is missing

The SDK does not expose these, so this port has neither:

- **Boxes round non-living entities** — dropped items, paintings, item
  frames, boats, minecarts, armor stands. The v1 entity API lists
  living mobs only; the original draws them all, since it hooks the
  game's own hitbox renderer.
- **The exact eye height per mob** — the API gives width and height
  only, so the red band sits at 0.85 of the mob's height. That is the
  middle of the vanilla range (about 0.74 for a creeper, 1.0 for a
  spider), so the band can sit a little high or low on some mobs.
- **The mob's true look direction** — the API exposes the mob's yaw
  (movement facing) but no pitch, so the blue arrow runs horizontally
  along its yaw. A mob looking up or down still gets a level arrow.
- **Creative-mode reach** — 5 blocks, 6.5 for a spear. The game mode is
  not exposed, so this port always uses the survival reach (3 blocks,
  4.5 for a spear).
- **The true sprint/swim/sneak state** — the original reads whether the
  player is actually sprinting, swimming or crouching, and says whether
  the toggle or the key is behind it. The API exposes no such state,
  only whether a key is held, so the indicator lights while the sprint
  or sneak key is held (double-tap-W sprint and toggle sprint do not
  light it) and there is no swimming line.
- **The eleven reach colours and the rainbow style** — the registry
  blurb mentions them, but the shipped 1.3.0 source has one in-reach
  colour (bright green) and four hitbox styles, and this port follows
  the source.

Two limits come from the SDK itself, not the API:

- **At most 1024 world lines per mod.** A mob costs 21 lines with the
  eye ray on (12 for the box, 4 for the eye band, 5 for the arrow), 16
  with it off, so the nearest 48 mobs draw with the eye ray on (64
  with it off). The mob under the crosshair always draws first, so it
  is never crowded out.
- **Multiplayer shows only the crosshair target.** The mob list is
  singleplayer-only in v1, so on a server only the mob under your
  crosshair gets a box; every other mob draws in singleplayer.

## Needs

API level 11 (`render.set-world`) — Kestrel build 17 or later.

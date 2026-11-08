# LookAt

A port of [LookAt](https://github.com/d3grando/lookat) by d3grando, MIT,
for Kestrel. The block or mob under your crosshair as a tooltip at the
top of the screen: its name in your language, the namespace it comes
from, its position and distance — and for a mob, its health as hearts
and numbers.

## What was ported

- The block tooltip: the translated name (from the game's item table, so
  it is in your language), the block id, the namespace it comes from
  (`Minecraft` for vanilla blocks), and its position with the distance
  from your eye. A block with no item falls back to its id with the words
  capitalised ("Water"), as the original does.
- The mob tooltip: the mob's name, its kind, its health as hearts plus
  the exact numbers (`♥♥♥♥♥♥♥♡♡♡ 14.0/20.0`), and its distance. A mob
  wins over a block when it is the nearer of the two under the crosshair;
  a mob behind a wall is never shown. Players under the crosshair are
  named too, with no health line — the game never vouches for a player's
  health.
- `/lookat` hides or shows the tooltip. Options > Mods > LookAt has the
  same switch plus range (6, 20, 64, 256 or 1024 blocks; 64 by default),
  position (top, top left, top right), health and entities switches; the
  game keeps the choices.
- In multiplayer, the mob under the crosshair still answers, but its
  health reads as unknown, so the tooltip shows the name, kind and
  distance with no health line.

## What is missing

The SDK does not expose these, so this port shows none of them:

- The item's icon: the tooltip is a text panel, so it cannot draw icons.
- The harvest tool: whether what you hold harvests the block, and the
  check or cross with the tool the block needs. The held item is readable
  but the harvest rule is not exposed to a sandboxed mod.
- Non-living entities: boats, minecarts, armor stands, paintings and
  item frames (with what they hold). The v1 entity API covers living mobs
  only.
- Size and background opacity settings: the panel is the SDK's plain text
  panel, with no size or opacity control.
- The top-centre position: the SDK's HUD offers corners only, so Top and
  Top Left both anchor top-left in this version.

## How to use it

Import `lookat.kmod` in Kestrel and enable it. Look at a block or a mob:
the tooltip shows what it is, where it comes from and how far away it
is. `/lookat` hides it; `/lookat` again shows it.

Needs SDK API level 10 (`entity.targeted` for the mob under the
crosshair; `recipes.items` for the translated names; `game.raycast` for
the block under the crosshair). Runs on Kestrel build 9+.

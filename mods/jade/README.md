# Jade

A port of [Jade](https://github.com/Snownee/Jade) by Snownee and
[Jade for Horizon](https://github.com/Joeypc6/Jade-Horizon) by Joeypc6 —
the look-at tooltip: what you're pointing at. Rewritten in Rust against
the Kestrel SDK; no original code runs. Not made or endorsed by Snownee,
so report problems to the Kestrel-mods project, not to them. Licence:
CC BY-NC-SA 4.0, noncommercial use only, the same licence as Jade.

## What was ported (block identification only)

Look at a block and a panel at the top-left
shows three lines:

1. The block's name, in your language (from the recipes table; a block
   with no item falls back to its registry id, prettified — "Oak Log").
2. Its mod — "Minecraft" for vanilla blocks.
3. Its position and how far away it is, e.g. `12 63 -44 · 3.2m`.

`/jade` hides or shows the tooltip, and Options > Mods > Jade has the
same switch plus a reach switch (6, 20 or 64 blocks; 6 by default, the
player's reach); the game keeps your choices.

## What is missing

- **Entities and their health** — the SDK exposes no entity data.
- **Block states and progress** (crops, furnaces, machines) — the raycast
  gives the block id only, no state.
- **Provider plugins** — other mods cannot register Jade providers with
  this port.
- **Themes, config screens, drag-to-place, profiles** — beyond the on/off
  and reach switches above.
- **`/jade` and `/jadec` commands from the original** — only the `/jade`
  on/off toggle is ported; there is no `/jadec`.
- **Overlay position** — Jade's own overlay is centred at the top of the
  screen; this port sits at the top-left, because the SDK's HUD only
  offers corners.
- **Distance** is measured from the player's feet, so it reads a touch
  long against a block at eye level (the eye is ~1.62 blocks above the
  feet).

## Use

Download `jade.kmod`, import it in Kestrel (Mods → Import file), enable
it. It needs SDK level 5 (`game-read` raycast, `recipes.items`,
`hud`) — any current Kestrel build covers it.

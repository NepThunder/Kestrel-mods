# Blockscope

A port of [Blockscope](https://github.com/Domanffe/blockscope) by Domanffe
(Alexander), MIT, for Kestrel. The block under your crosshair — its name in
your language, its block id, its distance and its coordinates — in a small
box in the top-right corner.

## What was ported

- The name, id, distance and coordinates box. The name comes from the
  game's item table, so it is translated to your language; a block with no
  item falls back to its id with the words capitalised ("Water"), as the
  original does. The distance is measured from your feet to the point the
  crosshair meets the block, as the original measures it.
- Nothing shows when you look at no block within reach.
- `/blockscope` hides or shows the box. Options > Mods > Blockscope has
  the same switch, a coordinates switch and a reach switch (5, 20 or
  64 blocks); the game keeps the choices.

## What is missing

The SDK does not expose these, so this port shows none of them:

- Hardness and the tool the block wants (and whether what you hold
  harvests it).
- Block state properties and crop growth.
- A container's slots.
- A creature's name and health when looking at one.
- The block's icon: the box is a text panel, so it cannot draw icons, and
  it sits in the top-right corner instead of beside the crosshair.

## How to use it

Import `blockscope.kmod` in Kestrel and enable it. Look at a block: the
box shows its name, its id (for example `minecraft:grass_block`), how far
away it is and which block it is. `/blockscope` hides the box;
`/blockscope` again shows it.

Needs SDK API level 5 (`recipes.items` for the translated names;
`game.raycast` for the block under the crosshair).

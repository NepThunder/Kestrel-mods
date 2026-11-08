# AppleSkin

A port of AppleSkin by soufi
([AppleSkin-Horizon](https://github.com/soufied/AppleSkin-Horizon), MIT),
itself a port of AppleSkin by squeek502. Ported for Kestrel by NepThunder.
This port's home is
<https://github.com/NepThunder/Kestrel-mods/tree/main/mods/appleskin>.

## What it does

Shows your saturation and what the food in your hand would restore, as a
text panel (bottom-right by default):

- **Saturation line** — your current saturation (0–20) as a bar plus the
  number, e.g. `Saturation [######----] 12.4`.
- **Held-food preview** — while you hold food, what eating it would
  restore and the totals after eating, e.g.
  `Cooked Beef +8 hunger, +12.8 sat -> 20 hunger, 20 sat`. At full hunger
  it says `full hunger` instead — except for foods that can always be
  eaten (golden apples, enchanted golden apples, chorus fruit, honey
  bottles, suspicious stew), which still show the saturation gain.

The food values are vanilla 26.3's (hunger + saturation per food, from
`minecraft:food`), and the totals follow vanilla's rule that saturation
can never exceed the hunger bar.

## What is missing

- **Exhaustion strip — left out.** Exhaustion is not on the client at
  all and cannot be exposed by the SDK, so there is no value to draw.
- **Icons over the food bar — approximated.** The SDK's HUD panels are
  text only, so saturation is a text bar in a corner panel instead of
  food icons drawn over the hunger bar.
- **Animation (fade/pulse) — left out.** The text panel has nothing to
  fade or pulse; the preview appears the tick you hold food and leaves
  the tick you stop.
- **Healing estimate — left out.** The original estimates the health the
  meal would restore; this port shows only hunger and saturation.
- **Off-hand food — not previewed.** Only the selected hotbar slot is
  checked, as in the original's held-item read.

## Use

`/appleskin` hides or shows the panel. Options > Mods > AppleSkin has
the saturation line, the held-food preview, and the panel position; the
game keeps those choices.

Needs SDK API level 9 (inventory read): the saturation and the held item
come from the read-only `%inventory` interface, the food level from
`game-read`.

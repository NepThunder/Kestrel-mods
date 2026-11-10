# Blood Particles

When a mob is hit, it bleeds: droplets that are crumbs of a block, a mist that
dries darker, and a cloud of spray on a heavy blow — three times as much on
the blow that kills. A port for Kestrel of
[Blood Particles](https://github.com/bogdan1tokarev1/Blood-Particles-Rust)
1.2.0 by Bogdan_Tokarev, MIT. This is a behaviour reimplementation against
the Kestrel SDK — no code or assets from the original; the settings,
defaults, value cycles, blood table, counts and chat strings are the
original's.

## What it does

Every tick the mod compares each nearby mob's health with the last health it
saw for it. A drop is a hit: the wound sits at 62% of the mob's height, and
the burst is sized by the damage — a hit of the reference damage (6, three
hearts) throws the configured numbers, lighter hits less, heavier hits more
(clamped to ×0.35–×2.5), with ceilings of 64 droplets, 48 mist and 12 clouds
a wound. The blow that kills multiplies every count by the death setting
(×3 by default) and throws wider. Two drops for one mob within three ticks
are one blow heard twice and draw once. Each mob bleeds its own kind, as in
the original:

| Mobs | Mist | Crumbs |
| --- | --- | --- |
| most mobs | dark red → darker | nether wart block / redstone block |
| slime | green | slime block / lime concrete |
| blaze, magma cube, strider | orange | magma block / orange concrete |
| enderman, endermite, shulker, ender dragon | violet | purpur block / purple concrete |
| skeleton, stray, bogged, wither skeleton, skeleton horse, zombified piglin | pale bone | bone block / quartz block |

Droplets are `minecraft:block` crumbs (every fourth droplet is the brighter
block), the mist is `minecraft:dust_color_transition` drying from the blood's
colour to its darker shade at scale 0.85, and the cloud is `minecraft:dust`
in the mist's colour brightened, at scale 1.6.

## Settings

Options > Mods > Blood Particles, or `/bloodparticles` (alias `/bp`):
`/bloodparticles` alone describes the current settings, `help` lists them,
`reset` restores the defaults, and `/bloodparticles <setting> <value>`
changes one (values snap to the nearest value the page can show). The twelve
settings, with the original's defaults: droplets a hit 8 (0–26 by its steps),
mist a hit 6, clouds a hit 2, damage scaling full (off/soft/full), the
killing blow ×3 (×1–×5), spread normal (jet/normal/splash), arc normal
(low/normal/high), each mob's own colour on, droplets are block crumbs on,
drawn within 40 blocks, wounds a tick 8, particles a tick 112. Settings
persist under the original's own one-line encoding.

## Reading notes — where the port differs, and why

- **The trigger is health, not the hurt flash.** The original watches the
  mob's hurt animation restarting; the SDK's entity read carries health, not
  the hurt flash, so the port diffs health instead, as the SDK's particles
  design note prescribes. A consequence the original does not have: damage
  the client is never told about draws nothing, and a hit that lands between
  two ticks is sized by the total drop seen. The three-tick dedupe window is
  the original's.
- **Single-player scope.** In the SDK's first version, entity health is
  data the client has in single-player; on a server the client is not given
  mob health, the entity list reads empty of health, and the mod stays quiet
  rather than guessing. This is the surface's scope, stated in the design
  note — not a defect in the port.
- **The spray is a burst, not a thrown jet.** The original throws each
  droplet on its own arc away from the player who dealt the blow, with
  gravity; the SDK's burst is a gaussian cloud of particles around a point,
  with a spread and a speed, and that is the shape this port emits — the
  spread setting widens it, the arc setting lifts it, and the killing blow
  widens it by the original's ×1.4. Counts, colours, blocks and scales match
  the original; the per-droplet ballistics do not exist on this surface.
- **The attacker is unknown.** Nothing in the port needs it: the original
  used it only for the spray direction, which the burst shape makes moot.

## Testing

`tests/probe.toml` walks the mod through a command summary, a seeded
zombie, a reference-damage hit (four bursts at the wound point), the
killing blow at ×3, and a command-line setting change. Host unit tests in
`src/lib.rs` pin the defaults, the encoding, the describe lines, the
scaling/clamping arithmetic, the blood table and the burst plan.

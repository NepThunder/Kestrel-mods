# Visuality

Pretty particles for the world's small moments: bones and feathers burst
from mobs when you hit them, slime blobs scatter where a slime lands, wisps
rise from soul blocks, amethyst clusters twinkle, and exposed ores glint. A
port for Kestrel of [Visuality](https://github.com/BenjiSyS/visuality-horizon)
1.0.2 by BenjiSyS, MIT — itself an original Rust implementation of
PinkGoosik's Visuality (also MIT). This is a behaviour reimplementation
against the Kestrel SDK — no code and no art from the original; the settings
pages, presets, defaults, effect rules and odds are the original's, and the
sprites are this port's own pixel art (`generate_sprites.py`), drawn to the
same sprite-set shapes.

## What runs

Five of the eight families, on the port's own sprite sets (below):

- **Hit fragments** — skeletons shed bone shards, wither skeletons dark
  bone, chickens feathers, villagers emeralds, on a 10-tick per-mob
  cooldown, (2 + a roll of 3) fragments by the family scale, at the
  original's sizes, placed through the mob's body box.
- **Slime blobs** — when a slime lands (it was airborne, now it is not),
  blobs of its size class scatter around its feet: small/medium/large by
  body width, base 2/3/4 blobs by the scale, in the page's slime colour.
- **Soul wisps** — a periodic scan of the blocks around the player finds
  soul sand/soul soil with air above; the nearest eight patches lend the
  engine a slow wisp emitter each (0.4 wisps a second at scale 1, the
  original's ~0.5%-a-visit re-based as a steady rate). Sprinting on a soul
  block sheds sprint wisps at the original's 1-in-5-a-tick odds.
- **Amethyst sparkles** — clusters found by the scan twinkle (buds too,
  when "only full-grown clusters" is off).
- **Ore glints** — gold, diamond and emerald ores found by the scan glint
  on their uncovered faces, in the material's own art.

## Settings

Options > Mods > Visuality — the original's four pages with its labels and
value cycles: Main (on/off, quality preset, particle amount, effect
distance, reduced motion, two-click reset), Effects (a page per family:
its toggle, its amount, and its extras — slime colour, full clusters only,
own armor in first person, uncovered faces only, the two soul switches,
the water search numbers), and Advanced limits (particle budget, spawns a
tick, follow the game's Particles option, the Minimal behaviour, the
diagnostics toggle). Presets are the original's: Low (128 active, 24
spawns a tick, 24 blocks, family ceilings ×1), Balanced (256/48/32, ×2),
High (512/80/40, ×4); changing a preset-managed number by hand re-derives
the preset, landing on Custom when nothing matches. Defaults are Low,
everything on, slime colour classic green. Settings persist in the mod's
storage.

## Sprite pack inventory

Thirty sets, 151 frames of 16×16 pixel art, carried in this same package
(api "0.2" content, the eatinganimation precedent) and registered as
`visuality_horizon:<set>`:

| Family | Sets |
| --- | --- |
| Hit | `bone_a`, `bone_b`, `wither_bone_a`, `wither_bone_b`, `feather`, `emerald` (1 frame each) |
| Slime | `slime_{small,medium,large}_{classic,lime,mint,sky,pink,amber}` (18 sets, 6 squash frames each) |
| Amethyst | `sparkle_amethyst` (6 frames) |
| Ores | `sparkle_gold`, `sparkle_diamond`, `sparkle_emerald` (6 frames each) |
| Soul | `soul_ambient` (8 frames), `soul_sprint` (5 frames) |

Not shipped: `charge_a/b/c` and `water_ring` (their families are gaps,
below), `feather_warm`/`feather_cold` (the variant gap, below), and the
plain `sparkle` (only the armor family used it).

## Reading notes — the gaps, each with its reason

- **Charged creeper sparks do not run.** The SDK's entity read carries no
  charged/power flag. The family's page and settings are kept whole; it
  waits on an entity-flag read.
- **Armor glints do not run.** There is no read for what a mob is wearing.
  The page (toggle, amount, first-person switch, built-in list note) is
  kept; it waits on an entity-equipment read.
- **Rain ripples do not run.** There is no weather read (rain level, or
  whether a column is rained on). The Water page is kept whole — radius,
  samples per tick, biome tint — and takes effect the day the read
  exists.
- **Chicken feathers are the plain feather.** The original picks a warm
  or cold feather set by the chicken's variant; there is no variant read,
  so the warm/cold sets are not shipped and every chicken sheds the plain
  feather.
- **The hit trigger is health, not the damage event.** The original hooks
  the server-sent damage event (and requires an attacker); the SDK
  carries health, not that event, so the port diffs health — the design
  note's prescribed reading. Consequences: fragments also follow damage
  with no attacker, and, health being single-player data in the SDK's
  first version, the family is quiet on servers. The world reads behind
  the ambient families are likewise single-player in v1.
- **Slime blobs do not replace the vanilla crumbs.** The original
  suppresses the slime's own crumb particles; the SDK has no
  particle-suppression surface, so the crumbs still fall and the blobs
  land alongside them.
- **Named-particle physics.** A named (mod) particle is a plain billboard:
  no tint (colours are baked into separate sprite sets instead — the six
  slime colours, the three ore materials), no gravity or flutter, a life
  of 20–40 ticks where the original's fragments live 5–20, and the
  engine's own base quad size, which the port's scales divide into. The
  original's per-particle arcs, spins and falls do not exist on this
  surface; positions, counts, sizes and odds are the port's own plan,
  one count-1 burst per particle.
- **Ambient sampling is a scan, not the game's visits.** The original
  rolls its odds when the game's ambient tick visits a block; visits are
  unobservable from the SDK, so the port re-reads the 17×9×17 blocks
  around the player every 10 ticks and re-bases the odds (amethyst
  1/200 a tick a block, ores 1/500 a tick a face, each by the family
  scale), landing at the same sparse density. A face counts as uncovered
  when its neighbour's id is air, water or lava — block state strings
  carry no shape, so a slab or fence neighbour can differ from the
  original's shape test.
- **Budgets, partially.** Spawns a tick is enforced exactly (the gate
  drops the rest, as the original's pool admission does). The active
  ceilings (global and per family) are stored and shown but cannot be
  enforced: the surface lends no count of live particles.
- **The game's Particles option.** The follow setting and the Minimal
  behaviour are kept as settings, but the SDK cannot read the option;
  the engine itself already thins mod particles by it, so the port does
  not apply the original's own ×2/3-at-Decreased or Minimal gating on
  top.
- **Reduced motion** is kept as a setting; the calmer spin and flutter it
  produces in the original ride on per-particle motion named particles
  do not have.
- **Diagnostics counters** — the toggle is kept; the counter readout the
  original appends to its Advanced page is not drawn, as the port keeps
  no particle pool to count.
- **No settings file.** The original's lists (hit rules, armor list, ore
  tags) and its file-only settings live in `config/visuality_horizon.toml`
  with a Reload button; the port has no file surface, so the lists are
  the original's built-in defaults, the file-only numbers stay at their
  defaults in effect (hit cooldown 10 ticks and max fragments 10 are
  constants in the code), and the lists row on the Hit/Armor/Ores pages
  says so instead of pointing at a file. There is no reload row and no
  file-problem note row.

## Testing

`tests/probe.toml` pins what a script can pin exactly: a quiet world, the
soul emitter a soul-sand block lends (and its withdrawal when the block
goes). The rng-shaped planners — hit rules and counts, slime classes,
menu labels and navigation, presets, the scan classification — are pinned
by the host unit tests in `src/lib.rs`.

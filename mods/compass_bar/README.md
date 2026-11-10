# Compass Bar

A compass bar at the top of your screen that shows where you are facing, in degrees, and slides smoothly as you turn — with death points on the bar.

A behaviour port of [Compass Enhanced](https://github.com/Fernandes585/Compass-Enhanced) by Isaac (Fernandes585), MIT. It shares no code with the original: the original is a sandboxed `.hmod` for Horizon, this is a fresh implementation against the Kestrel SDK (`game-read` for the facing, `render` for the bar, `map` for the death points, `input` for the dismiss key, `ui` for the settings page, `storage` for the choices).

## What matches the original

- The bar itself: the eight compass letters (N, NE, E, SE, S, SW, W, NW) in one chosen colour, and the degree scale under them, across the top of the screen, sliding as you turn. The heading reads as F3 does — 0° north, 90° east, 180° south, 270° west.
- The settings, in Options > Mods > Compass Bar: the bar on/off, the background box, the size (75%, 100%, 125%, 150%) and the letter colour. Choices are kept between sessions.
- **Death points (new in 0.2.0).** Where you died is marked on the bar at its bearing, with the distance under the mark; a point out of view sits at the bar's edge as an arrow pointing the way to turn, with its distance; inside the arrival distance the marker becomes an X, and after the X time in range the point goes; looking at a point (within 30° of your heading) and pressing **X** dismisses it. The three death-point settings are on the same page: Arrival Distance, X Time, and Death Points Kept (the oldest goes first past the kept number). The points are not saved — they vanish when the game closes, as the original's do.
- No commands — the original has none, so this port adds none.

## Readings and choices, stated plainly

- The registry names the three death-point settings but no values or defaults, so the cycles and defaults are the port's: Arrival Distance 2/3/**5**/8/12/20 blocks, X Time 5/**10**/15/30/60 seconds, Death Points Kept 1/2/**3**/5/10 — the same kind of stated choice as the letter-colour palette below.
- A point is also seeded from the game's own last-death record when you enter a world in that death's dimension (the standing read the SDK's map district prescribes). The original, itself a mod, only ever sees deaths it was there for; the seed means a death from before the mod was installed — or from a session where it was off — still gets its point, once, when you come back to that world. Dismissing it and changing dimension away and back re-seeds it, since the game still remembers the death.
- The arrival countdown is read as time spent in range: step back out of the arrival distance and the X lifts, the count forgotten. The registry says only "arrival countdown"; this is the port's reading.
- The registry description names no default letter colour, so the port's colour cycle starts at white; the six colours offered are the port's palette for the original's "a color you choose".

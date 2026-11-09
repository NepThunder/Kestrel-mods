# Compass Bar

A compass bar at the top of your screen that shows where you are facing, in degrees, and slides smoothly as you turn.

A behaviour port of [Compass Enhanced](https://github.com/Fernandes585/Compass-Enhanced) by Isaac (Fernandes585), MIT. It shares no code with the original: the original is a sandboxed `.hmod` for Horizon, this is a fresh implementation against the Kestrel SDK (`game-read` for the facing, `render` for the bar, `ui` for the settings page, `storage` for the choices).

## What matches the original

- The bar itself: the eight compass letters (N, NE, E, SE, S, SW, W, NW) in one chosen colour, and the degree scale under them, across the top of the screen, sliding as you turn. The heading reads as F3 does — 0° north, 90° east, 180° south, 270° west.
- The settings, in Options > Mods > Compass Bar: the bar on/off, the background box, the size (75%, 100%, 125%, 150%) and the letter colour. Choices are kept between sessions.
- No commands — the original has none, so this port adds none.

## What's missing, and why

- **Death points.** The original marks where you died on the bar, with the distance, arrows at the bar's edge for points out of view, an arrival countdown that turns the marker into an X, and a key to dismiss it. The Kestrel client SDK exposes no death event, so a port has nothing to hang a death on. The death points — and their settings (arrival distance, X time, how many are kept) — are left out entirely rather than imitated, and arrive when the SDK gains a death/position event.
- The registry description names no default letter colour, so the port's colour cycle starts at white; the six colours offered are the port's palette for the original's "a color you choose".

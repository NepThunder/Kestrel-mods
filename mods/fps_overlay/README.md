# FPS Counter

Frames per second in a corner of the screen, with the minimum and the 1% low.

A port of Horizon's own FPS Counter (MIT), formerly a built-in extra in Options > Horizon Extras, and listed in the Horizon registry as a native mod (2.0.0) built from the same behaviour as the 1.0.0 WebAssembly mod. This port follows the original exactly: same box, same wording, same settings, same defaults, same storage keys. The original is MIT-licensed; this port is a fresh crate in the Kestrel catalog layout, implementing the original's behaviour against the Kestrel SDK (capabilities `render`, `ui`, `storage`).

## What it shows

The game's own frame counter (`render.frame-stats`, recomputed twice a second) in a small translucent-black box (0x78000000) with white shadowed text:

- Always: `120 fps` — frames drawn in the last second.
- With the lows on: `120 fps  min 58  1% 41` — adding the slowest frame's rate and the 1% low over the last 5 seconds.

The box is drawn from the tick hook, as the original draws it, and only redrawn when the text changes.

## Settings

Options > Mods > FPS Counter — the original's two settings, nothing added:

- **Corner** (Top left / Top right / Bottom left / Bottom right) — default **Top left**.
- **Minimum and 1% low** (ON / OFF) — one combined toggle, as in the original — default **OFF**.

They are stored under the original's keys (`corner`, `lows`) — the same storage the game itself writes a player's old extra settings into when it installs the mod for them, so those carry over.

## Fidelity

Matches the original: the feature set above, the text wording and spacing, the box's size rule (text width + 4, 12 high) and colours, the two settings and their defaults, the storage keys and value encodings, and redraw-only-on-change from the tick hook.

Missing: nothing found. The original has no chat command and no further options, so this port has none either.

# Keystrokes

Your movement keys, mouse buttons and space bar in a corner, lit while held, with clicks per second.

A port of Horizon's own Keystrokes (MIT), formerly a built-in extra in Options > Horizon Extras, and listed in the Horizon registry as a native mod (2.0.0) built from the same behaviour as the 1.0.0 WebAssembly mod. This port follows the original exactly: same overlay, same settings, same defaults, same storage keys. The original is MIT-licensed; this port is a fresh crate in the Kestrel catalog layout, implementing the original's behaviour against the Kestrel SDK (capabilities `render`, `input`, `ui`, `storage`).

## What it shows

- Forward, left, back and right **as you bound them** — each box shows the bound key's own label and lights while the key is held.
- The two mouse buttons (LMB / RMB), lit while held, with **clicks per second** under each label.
- The space bar as a wide short key with a line in it, lit while jump is held.

Boxes are 22 px keys on a 70 × 82 panel: translucent black (0x78000000) at rest with white shadowed text; a held key fills light (0xB0FFFFFF) with dark unshadowed text — the extra's own colours and geometry.

## Settings

Options > Mods > Keystrokes — the original's two settings, nothing added:

- **Corner** (Top left / Top right / Bottom left / Bottom right) — default **Top left**.
- **Clicks per second** (ON / OFF) — default **ON**.

They are stored under the original's keys (`corner`, `cps`), so a player's settings from the original mod carry over.

## Fidelity

Matches the original: the feature set above, the two settings and their defaults, the storage keys and value encodings, the layout, colours and text treatment, redraw-only-on-change in the frame hook, and bound-key labels read live from the game's own mappings (`input.mapping`), so rebinding a key in Controls changes the overlay exactly as it did in the original.

Missing: nothing found. The original has no chat command and no further options, so this port has none either.

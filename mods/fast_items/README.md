# Fast Items

Dropped items, drawn cheaper: flat items (potions, tools, food) lie as one flat face turned to face you — the sprite's edges and hidden back face are not drawn — and dropped block items keep their 3D model but face the camera instead of spinning, as in Lunar Client and Badlion.

A behaviour port of [Fast Items for Horizon](https://github.com/Lyvt-Dev/FastItems-for-Horizon) by Lyvt, MIT, itself inspired by Noryea's Fast Items and independently written, as this port is. It is a fresh implementation against the Kestrel SDK: API 23's `item-model.set-dropped-display` (under `render`) is this mod's whole surface, with `storage` and `ui` for the settings.

## What matches the original

- Flat items are one flat quad facing the camera; multi-layer textures, tint, lighting and glint stay (the game's flat bake keeps each layer's front face).
- Block items billboard: the whole 3D model, turned to face you instead of spinning.
- Item bobbing, stacks (clusters by count), the pickup animation, culling and glow outlines work as before — the engine's parts, untouched by the policy.
- Dropped items' blob shadows can be turned off.
- Only dropped items change: inventory icons, held items, item frames and servers stay as they are — the original says the same of itself.

## What's different, and why

- **Settings live on the standard settings page** (Options > Mods > Fast Items: Flat Items, Block Items, Dropped Shadows) and apply at once. The original keeps its settings in `config/fast-items.json` in the game folder and asks for a restart after editing it — the settings-page compromise every port here makes, named in the API 23 design note.
- One policy applies at a time across all running mods (the host's newest-writer rule for this surface), where the native original simply owns the render path. Writing the all-vanilla record hands the display back, and a stopped mod's policy goes with it.

# Rarity Borders

Adds clean, vanilla-style coloured borders to inventory items based on their rarity: yellow for Uncommon, aqua for Rare, light purple for Epic — a square outline with a highlighted bottom edge, on inventory and container slots.

A behaviour port of [Rarity Borders](https://github.com/maliferen/horizon-rarity-borders) by Xariami (maliferen), MIT. The original is a native Horizon mod; this is a fresh implementation against the Kestrel SDK (API 23's `item-model` decorations under `render`, `storage` and `ui` for the settings).

## What matches the original

- The borders: one per rarity, in the vanilla rarity colours, on inventory and container slots.
- The optional features, as settings in Options > Mods > Rarity Borders, both off until turned on: borders for **Common** items, and borders on the **hotbar**.
- Settings are kept between sessions (the original's 0.1.1 fixed exactly that, and added its regression tests; the declarations and options are pinned by this port's unit tests the same way).

## What's different, and why

- **The port never reads a rarity — the game matches it.** A stack's rarity usually lives in its item's *default* components, and the SDK's `inventory` read spells only the components a stack carries *beyond* its defaults, so a mod cannot learn most slots' rarity. API 23 was designed around exactly this: the port declares one border per rarity as a *stack-match* decoration keyed on the `minecraft:rarity` component, and the host evaluates the match against each stack's merged components (defaults included) — the same set the item resolver itself reads. Nothing about the port reads rarity itself, because nothing in the SDK honestly can.
- The border's exact pixels are the game's host-drawn border hand (a square outline, bottom edge lit), as every sandboxed mark is; the mod chooses the colours, places and which rarities border. The original draws its own pixel-perfect outline natively.

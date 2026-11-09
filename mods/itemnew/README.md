# Item New

Items you just got — picked up, crafted, taken from a chest, given — get a soft glow in the hotbar and in inventory and container screens, until you point at them or close the screen.

A behaviour port of [Item New](https://github.com/d3grando/itemnew) by d3grando, MIT (inspired by Item Highlighter; no code from either). The original is a native Horizon mod; this is a fresh implementation against the Kestrel SDK (`inventory` for the snapshots, `screen` for the hover and screen reads, API 23's `item-model` decorations under `render`, `storage` and `ui` for the settings, `command` for `/itemnew`).

## What matches the original

- The marks: a soft glow (a translucent fill under the stack) on newly gained items, in the hotbar and on inventory and container screens.
- The three styles: **Glow**, **Glow + Star** (a star at the slot's corner), and **By Rarity** (the fill takes the item's vanilla rarity colour).
- The clearing: a mark goes when you point at its slot, and closing the screen clears the marks — each is a setting, as "when a mark clears" is in the original.
- Which screens show marks is a setting: hotbar + screens, hotbar only, or screens only.
- `/itemnew` hides the marks or shows them again; `/itemnew clear` forgets the new items entirely.

## How newness is found

The SDK lends snapshots, not events, so the port diffs them: each tick, a slot counts as new when its stack's identity (item + components) grew in total across the inventory it sits in — the player's own inventory, or the open container's — compared with the tick before. Pickups (including merges onto a held stack), crafting results and taking from a chest all grow a total; dragging a stack to another slot, swapping two slots, splitting a stack and swapping hands do not, and mark nothing. Opening a container and joining a world only set the baseline.

## What's different, and why

- **Moving a new item loses its mark.** The diff cannot follow a stack across slots, so a marked item dragged elsewhere arrives unmarked. The original, running natively, can track the item itself.
- **The by-rarity style colours every stack of a newly gained item.** A stack's rarity usually lives in its item's *default* components, which no SDK read exposes (the `inventory` components string is the patch only), so — as the API 23 design prescribes — the style is declared as host-matched stack decorations keyed on `minecraft:rarity`: the host finds the stacks of that item and colours each in its true rarity colour. If you gain a second dragon egg while one already sits in a chest screen, both colour until the mark clears. The glow styles mark only the new slots themselves.
- The glow's colour is the port's soft warm white; the original's registry text names no colour.

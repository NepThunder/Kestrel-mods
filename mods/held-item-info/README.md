# Held Item Info

A port of [Held Item Info](https://github.com/AltMugen/horizon-held-item-info)
by AltMugen (MIT) for Kestrel. Ported for Kestrel by NepThunder. This port's
home is
<https://github.com/NepThunder/Kestrel-mods/tree/main/mods/held-item-info>.

## What it does

One text line for the item in your selected hotbar slot, in the bottom-left
corner:

- remaining / maximum durability, e.g. `Diamond Sword · 1500 / 1561`
- stack count when it stacks, e.g. `Torch · ×64`
- up to six enchantment names, e.g. `Diamond Sword · 1500 / 1561 · Sharpness 5 · Unbreaking 3`

The line only shows while the held item has something to say: a single
undamageable, unenchanted item (an apple, a stick) shows nothing, exactly
as in the original.

## Use

`/helditem` hides or shows the line. Options > Mods > Held Item Info has
the same switch, and the game remembers your choice.

## What was ported

- Durability (`remaining / maximum`) from the inventory snapshot's
  `damage` / `max-damage` — the audit said this was not exposed, but the
  Kestrel inventory API (level 9) does expose it.
- Stack count (`×64`) when the count is greater than one.
- Up to six enchantment names, parsed from the snapshot's `components`
  string (`[enchantments={levels:{...}}]`), with the level after the name,
  as in the original.

## What is missing (gaps)

- **Position.** The original draws its line centred above the hotbar; the
  SDK's text HUD only offers the four screen corners, so this port uses
  the bottom-left corner.
- **Language.** The original uses the game's translations for enchantment
  names. The SDK has no translation lookup, so this port uses a hardcoded
  table of the vanilla enchantment names in English. An enchantment id the
  table does not know falls back to its title-cased id, so a modded
  enchantment still shows something.
- **Item name.** The original does not draw the item's name — it sits
  directly under the game's own selected-item name. This port puts the
  name in front of the line, because in a corner the line has no such
  context.
- **Shortening.** The original shortens its text to the exact GUI width.
  The SDK gives the mod no GUI width, so this port shortens the line only
  to the panel's 250-character limit, with an ellipsis.
- **Not implemented in the original either:** potion details, layout
  options and a fade timer.

## Build

Built like the other mods in this repo: copy this folder to
`crates/horizon-mod-sdk/examples/held-item-info/` in the Kestrel tree,
add it as a workspace member, and run `cargo horizon-mod build` there;
the pack tool writes `held_item_info.kmod`.

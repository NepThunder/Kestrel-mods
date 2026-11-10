# Inventory Wizard

A behaviour port of [InventoryWizard](https://github.com/VanishingTacos/InventoryWizard)
by VanishingTacos (MIT, v2.3.3) for Kestrel. Smart sorting: items sort by
category — stone, earth, wood, ores, blocks, tools, weapons, armor, food,
redstone, transportation, decoration — judged from item tags and
components, and the hotbar is laid out survival-style: weapons, tools,
food, building blocks, utility items, then the rest. Partial stacks merge,
but enchanted or named items are never combined.

Three modes — **Default**, **Alphabetical**, **Stack-based** — with Sort
buttons and a mode button (showing the mode's letter) beside the inventory
and container titles, an **R** key, and `/iwiz [hotbar|inventory|all|mode]`
(also `/inventorywizard`). Slots can be locked so sorting never touches
them; locks persist. Works on the inventory, the hotbar, chests, barrels,
shulker boxes and ender chests, in singleplayer and on servers — every
sort is ordinary left clicks.

## Fidelity

Matches the original: the category order and the survival hotbar, the
three modes, the never-combine rule for enchanted or named items, the
buttons and the mode button's letter, the R key, the mode key unbound by
default, both command names and their arguments, locked slots, and the
all-left-clicks implementation.

Gaps, all limits of the sandbox surface, documented rather than filled
with inventions:

- **Alt+Click locking does not port**: no SDK surface observes ordinary
  slot clicks. Locks live on the mod's page in Options > Mods > Inventory
  Wizard instead — pick a slot, flip it locked; the feature (locked
  slots the sorter never touches, remembered between sessions) is the
  same, the gesture is the gap. Shift+Right-Click sorting is the same
  story: the sort gestures are the buttons, the R key and the command.
- Keys and commands fire only while playing with no screen open, so in a
  screen the buttons carry the sorts, and R sorts the whole inventory
  rather than the part under the cursor.
- Categories are judged from the tags and ids the SDK lends, not from a
  creative-tab read; borderline items can land a category off from the
  original's own table.
- Vanilla stack sizes cross no read; the planner carries the game's
  table for the sizes that differ from 64, as in the other sorting
  ports.

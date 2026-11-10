# ClientSort

A behaviour port of [ClientSort](https://github.com/TerminalMC/ClientSort) by
TerminalMC (Apache-2.0; Horizon port by DeportedToHell) for Kestrel. Buttons
beside the container and beside your inventory — **Sort**, **Restock**,
**Transfer** — a middle click that sorts the part under the pointer, and keys
for sort, restock, transfer and fill stacks. Every action is an ordinary
container click, so it works the same on servers.

- **Sort** orders one side on its own: the container, the main inventory and
  the hotbar are separate groups (the hotbar group can be merged into the
  inventory in the settings).
- **Restock** moves in the items the side already holds. **Transfer** moves
  as much as fits.
- Sort order is the creative inventory's order by default; hold **Shift**
  (your sneak key) while triggering for quantity, **Ctrl** (your sprint key)
  for name A–Z. The default order, click speed, hotbar grouping and
  restock-by-type are settings in Options > Mods > ClientSort.
- While ClientSort's buttons are shown, the game's own sort buttons step
  aside (the API 22 contract).

## Fidelity

Matches the original: the three operations on both sides, middle-click
sort, the four orders and their modifiers, the settings list (buttons
on/off, default order, click speed, hotbar as its own group, restock by
item type), keys for every operation including Fill Stacks, and the
all-clicks implementation.

Gaps, all limits of the sandbox surface, documented rather than filled
with inventions:

- The creative inventory's exact order crosses no SDK read, so the
  default order sorts by the game's item registry order, which follows
  it closely. The quantity, name and ID orders are exact.
- The original's Alt modifier (item ID order) has no key mapping to
  read; the ID order is chosen as the default order in the settings.
- Trigger keys fire only while playing with no screen open, so in a
  screen the operations live on the buttons; restock and transfer keys
  have no second inventory to act on while playing, and Fill Stacks
  tops the hotbar up from the main inventory. The keys ship unbound
  (the registry names no defaults); bind them in Options > Mods.
- Button tooltips are not ported: a sandbox button carries a label
  only, so the original's tooltips setting has no counterpart.
- Vanilla stack sizes cross no read; the planner carries the game's
  table for the sizes that differ from 64 and treats anything with
  durability as unstackable, as the game does.

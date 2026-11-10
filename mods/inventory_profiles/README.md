# Inventory Profiles

A behaviour port of Inventory Profiles Next and its Inventory Sorter
companion (Horizon port by CayMac, MIT) for Kestrel. Press **R** or click
the sort buttons to merge partial stacks and sort a chest or your
inventory in four modes — **Category** (Creative Tab), **Alphabetical**
(A–Z), **Stack Count** (High–Low), **Item ID** — switchable on the
settings page or with the mode button beside the sort buttons.

- **Auto-Refill**: when the stack in your hand runs out (the last block
  placed, the last food eaten), a fresh stack of the same item is swapped
  in from your inventory.
- **Tool Break Protection**: when the tool in your hand has one use
  left, it is swapped for a healthier one of the same kind — or parked
  in a free inventory slot — before the use that would break it.
- **Quick-Stack** (G key, or the button beside the container): moves
  every stack the open container already holds into it.

## Fidelity

Matches the original: the four modes, the R key and the UI sort
buttons, auto-refill, tool break protection and the G key quick-stack,
each performed as ordinary container clicks.

Gaps, all limits of the sandbox surface, documented rather than filled
with inventions:

- The creative tabs' exact order crosses no SDK read, so Category mode
  sorts by the game's item registry order, which follows the tabs
  closely. The other three modes are exact.
- Keys fire only while playing with no screen open, so in a screen the
  sort and quick-stack live on their buttons.
- Auto-refill and tool protection watch only while you play, not while
  a screen is open — organizing an inventory never triggers them.
- Tool protection triggers at one use left (the last moment a swap can
  save the tool); the registry names no threshold number.
- Vanilla stack sizes cross no read; the planner carries the game's
  table for the sizes that differ from 64, as in the other sorting
  ports.

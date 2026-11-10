# Mouse Tweaks

A behaviour port of [Mouse Tweaks](https://github.com/YaLTeR/MouseTweaks)
by YaLTeR (Ivan Molodetskikh, BSD-3-Clause; Horizon port by
DeportedToHell) for Kestrel.

- **RMB drag**: hold the right button with a stack and drag over slots
  to drop one item into each.
- **LMB drag**: with an item on the cursor, drag to gather the same
  items into your hand. With **Shift**, quick-move every slot you pass
  instead — and the same with an empty hand.
- **Wheel tweak**: turn the wheel over a slot to move items one at a
  time between the inventory and the container, crafting results
  included.

Search order, scroll direction (normal, inverted, position-aware) and
item scaling are settings in Options > Mods > Mouse Tweaks, together
with the three tweak toggles. It works on every container screen and
the player's slots in the inventory screen. While it is on, turn the
game's built-in Mouse Tweaks extra off (Extras) — it takes that
extra's place, and two wheel tweaks answering one turn would move two
items.

## Fidelity

Matches the original: the three tweaks and their gestures, the wheel's
push and pull sequences (the same clicks the game's own tweak makes),
and the settings the registry names — the toggles, scroll direction in
its three forms, search order, item scaling.

Frictions, all limits of the sandbox, stated rather than hidden:

- **No drag preview**: the drag ghost is drawn for the game's own
  drag, not a mod's.
- **Tick-grain hover tracking**: the hovered slot is read once a tick,
  so a fast flick can skip a slot, and a slot re-entered within one
  hold is served once.
- **The game's own quick-craft drag still runs underneath**: a mod
  cannot consume the player's press, so on release the game also
  distributes what is left in your hand over the slots it tracked,
  which can add an item to a slot the tweak already served.
- The creative inventory screen is not served: its slots run on a
  different protocol.
- Vanilla stack sizes cross no read; the wheel planner carries the
  game's table for the sizes that differ from 64, as in the sorting
  ports.

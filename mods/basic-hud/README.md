# The Basic HUD

A corner HUD panel: coordinates, FPS, the world clock, speed, CPS,
player health, effect timers, held-item and armour durability, and
recent pickups. A
port of [The Basic HUD](https://github.com/repositorykreml1n/the-basic-hud)
by repositorykreml1n (MIT), ported for Kestrel by NepThunder.

## The panel

One line per widget, in this order, when the widget is on (all are on by
default), in the top-left corner:

- **FPS** — `120 fps  min 41  1% 52`: frames in the last second, the
  slowest frame and the 1% low of the last 5 seconds, from
  `render.frame-stats`, as the FPS counter shows them.
- **Coordinates** — `XYZ 0.5 / 64.0 / 0.5`: the feet position.
- **Clock** — `Day 1, 06:00`: the world clock, day time 0 is 6:00 on day 1.
- **Speed** — `Speed 4.3 b/s`: horizontal blocks per second, averaged
  over the last second of movement (20 ticks).
- **CPS** — `CPS 7 | 2`: attack clicks, then use clicks, in the last
  second, from `input.clicks-per-second`.
- **Health** — `Health 20.0/20.0`: the player's own health.
- **Effects** — `Speed II 1:01`: one line per active effect, in the
  game's own order — the name, the Roman numeral level past I, and
  the time left as m:ss from the effect's real remaining ticks
  (`game.get-effects`); a permanent effect reads `inf`. New in
  0.1.1: 0.1.0 left this widget out because no SDK interface
  exposed an effect list; API 18's self reads do.
- **Durability** — `Diamond Sword 1461/1561`: the held item's remaining
  over maximum durability, from the `%inventory` snapshot; nothing when
  the held item cannot wear out. Coloured the Java way, as Armor Status
  colours it: green above 50%, yellow from 25%, orange from 10%, red
  below 10%.
- **Armour** — one line per worn piece, same format and colours as the
  held item: `Diamond Helmet 353/363`.
- **Pickups** — `+6 Torch`: items that arrived in the inventory since the
  last tick, newest first, at most four lines and then a `+N more` line.
  Matching items aggregate and refresh their timer, as the original
  does: 4 torches, then 2 more before expiry, shows x6. Entries expire
  after the pickup display time, 5 seconds by default.

`/basichud` hides or shows the panel, and Options > Mods > The Basic HUD
has a switch per widget, the corner (top left, top right, bottom left,
bottom right) and the pickup display time (1, 2, 3, 5, 10, 15, 30 or 60
seconds); the game keeps the choices in the mod's storage.

## Requirements

Needs a Kestrel build with the SDK self reads (API level 18). The
effect list is level 18, the newest function this mod imports; the
inventory snapshot is level 9, and everything else it reads — the
player and clock, frame stats, clicks per second, item names — is
level 5 or below.

## How pickups are found

The SDK has no pickup event, so the mod diffs the `%inventory` snapshot
once per tick: a rise in an item's total over the hotbar and main
inventory is a pickup. That is a snapshot, not an event, so it has
edges the original does not:

- **Rearrangement logs nothing**: the same totals mean no pickup, so
  moving stacks around creates no entries.
- **Crafting and consuming log nothing**: a rise in the same tick as any
  fall is crafting or consuming, not a pickup — two planks becoming four
  sticks shows no entry.
- **A `/give` looks like a pickup**: giving has no fall in the same tick,
  so the mod cannot tell it from picking the item up. The original
  could, because it saw the pickup event.
- **Only the hotbar and main inventory count**: armour, the offhand, the
  crafting grid and the carried stack are not diffed.
- **Joining a world logs nothing**: the first tick with a player sets
  the baseline instead of logging the whole inventory.

## What is missing, and why

The original is a visual HUD editor with 14 movable widgets. Ten of
them are in this port. The rest cannot be built on the SDK as it stands:

- **Item cooldowns** — server-plugin-only in the original, and no
  cooldown state is exposed to a client mod.
- **Ping** — no client player list is exposed, so there is no server
  latency to show.
- **The Tab list** — no client player list is exposed: no players, no
  ping bars, no sorting.
- **The drag-and-drop HUD editor** — the SDK `ui` has no drag
  infrastructure: no dragging, no per-widget scale, colours, opacity,
  padding or shadows. Position is chosen on the settings page instead
  (the corner), and the panel is always one panel of plain text lines in
  that corner, in the same order.
- **Totem-use counting** — unobserved totem uses cannot be counted: the
  client only sees its own inventory, and a totem consumed while the
  panel is off or before the mod loads leaves no trace.
- **Ten interface languages** — English only in v1: the SDK has no
  translation lookup, so names come from the item table and labels are
  hardcoded English.
- **Consumable counts** — the original's consumable-count widget has no
  separate line here: the counts it would show are the inventory totals
  the pickup diff already reads, and a count widget would duplicate the
  held-item line.

Also deliberately not ported: per-widget position (one panel, one
corner), per-widget scale and colours (durability lines excepted),
text shadows and background styles. The SDK's HUD panel is one panel of
plain text lines in a corner — a mod says what the lines hold, never
where each one sits or how it looks — so none of that styling survives.

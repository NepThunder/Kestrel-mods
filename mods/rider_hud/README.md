# Rider HUD

While riding, your hunger stays visible next to your mount's health: a hunger row in the vanilla style, directly above the mount's heart rows, with the experience and jump bars left as documented gaps.

A behaviour port of Rider HUD by Xariami (MIT, https://github.com/maliferen/horizon-rider-hud). It shares no code with the original: this is a fresh implementation against the Kestrel SDK, built on API 18's self read (`game.get-self-state`, capability `game-read`) — the district that exists because the ridden mount was not readable before it.

## How it works

Vanilla's bargain while riding is that the mount's health takes the hunger row's place: the game draws the vehicle hearts and does not draw the food row at all. So the mount's health half of the original needs no help — the game already draws the mount's hearts where vanilla draws them, and this mod does not draw them twice.

What the mod adds back is the hunger, arranged the way the original's own screenshots show it: a row of the game's food sprites in the game's own food layout (containers with full and half overlays, filling from the right), sitting directly above however many rows the mount's hearts occupy — two rows up for a 30-health horse, one for a pig. While the Hunger effect lasts, the row wears the game's hunger-variant sprites, as the game's own food row does. The row shows only while riding a mount whose health the game displays; on foot, or on a mount with no health display, the game's own food row is already there and the mod draws nothing.

Options > Mods > Rider HUD holds the hunger row's toggle.

## Fidelity

Matches the original's registry description:

- Player hunger and mount health displayed at the same time while riding.
- The familiar vanilla Minecraft HUD style: the game's own sprites, the game's own layout metrics, nothing redrawn that the game already draws.
- In-game settings to toggle the feature.

Documented gaps — omitted, never faked:

- **The experience bar while riding.** The original keeps the XP bar (and its level) visible while riding, switching dynamically with the jump bar as a jump charges. The SDK exposes no experience level or progress read, so the bar cannot be drawn honestly; it arrives when the SDK gains an experience read, and its settings toggle comes with it.
- **The jump bar.** The original shows the jump bar when charging a jump. The SDK exposes no jump-charge read (the self read names the mount, not its charge), so the bar and the dynamic XP/jump switching are out with it.
- **Boats and minecarts.** The self read's mount is a living mount, as the original's health display is; riding a boat or minecart reads as no mount, and the game draws its own food row there anyway.
- **No command** — the original has none, so the port adds none.

## Requirements

Needs a Kestrel build with the SDK self reads (API level 18): `game.get-self-state` and `game.get-effects` are the functions this mod imports beyond the HUD shapes.

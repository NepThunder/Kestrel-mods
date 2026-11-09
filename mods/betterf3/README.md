# BetterF3

A colour-coded, customizable replacement for the F3 debug screen: modules in two columns, each in its own colours, sliding in when F3 opens.

A behaviour port of [BetterF3](https://github.com/TreyRuffy/BetterF3) by cominixo and TreyRuffy, MIT, via DeportedToHell's Horizon port. It shares no code with the original: this is a fresh implementation against the Kestrel SDK (`screen` for the debug claim and the open state, `game-read` for the player, clock and targets, `world` for biome and light, `entity` for the targeted mob, `render` for the drawing, `input` for the config key, `storage` and `ui` for the settings).

## What matches the original

- The takeover: while the mod runs, the game's own F3 entries stand down (`screen.claim-debug`) and BetterF3's screen appears in their place when F3 is open, in the HUD layer.
- The modules the SDK can feed, in the registry's order, each in its own colour, labels coloured and values white: **Minecraft** (the version), **FPS** (the rate, coloured green/yellow/red by its value, with the minimum and 1% low), **Coordinates** (XYZ, block, facing with vanilla's axis text, and velocity — one of BetterF3's extra lines), **Chunks** (the chunk and the place in it), **Location** (dimension, biome, light), **Help**, **Misc** (day, the in-game time and days played — the other extra lines), **Targeted** (the block and entity under the crosshair, with health where the client can vouch for it).
- The customization the settings surface can hold, in Options > Mods > BetterF3 and on the same page in game (the mod registers a config key, which starts unbound — bind it on that page): every module on or off, in the left or right column, its colour cycling a nine-colour palette; line backgrounds, text shadow, the sliding animation, and Auto Start (the overlay opens with the world, until F3 is pressed) each on or off; and a master switch that hands F3 back to the game. Choices are kept between sessions.
- No commands — the original has none, so this port adds none. F3+M and F3+F stay the game's own debug keys.

## What's missing, and why

- **Five modules: Graphics, Server, Entities, Sounds, System — and the memory line.** The client SDK has no read for a renderer or GPU, a server's address or brand, an entity or chunk count, a playing-sound count, or memory and system figures. They are left out entirely rather than filled with invented numbers; each arrives when the SDK gains its read.
- **The slime-chunk line** (a Coordinates extra in the original): slime chunks are decided by the world seed, and the SDK exposes no seed.
- **The font scale**: the SDK's text shapes have no scale to set.
- **Hide the boss bar and scoreboard while F3 is open**: no SDK surface hides just those two; the only HUD switch there is hides everything, the mod's own drawing included.
- **Module order, add and delete modules**: the settings page is a stack of buttons, not the original's drag-and-drop Cloth Config screens (themselves not portable — a mod's config here is the standard page). The order is the registry's module order; columns and colours are the arrangement choices the page can hold.
- **Per-line switches and free colour choice**: lines belong to their module's switch, and colours cycle the palette rather than opening a colour picker, for the same reason.
- **The layering**: the game's own debug overlay draws over open screens; this replacement draws in the HUD layer, under a screen that is open over the game — the API 22 design's named v1 limit.

# Accurate Block Placement

Hold right click with a block and every new spot you look at gets a block at once, however fast you move.

A behaviour port of [Accurate Block Placement: Reborn](https://modrinth.com/mod/accurate-block-placement-reborn) (MIT) by Clayborn, Flourick, schwar and KadTheHunter, following Lutziano888's [Horizon port](https://github.com/Lutziano888/horizon-accurate-block-placement) — its rules file is the spec for the placement logic here. This is a fresh implementation against the Kestrel SDK (the `actions` capability, API 21); the original is a native mod.

## What matches the original

- **A new spot, a block**: while use is held with a block item, each fresh target — a block you weren't already looking at and didn't just place — gets one placement at once, at most four a tick, oldest first. Bridging, walls, floors and sweeps lose no spot.
- **Staring at one spot adds nothing of the mod's**: the mod never fires twice on the spot it just filled.
- **A single click stays a single block**: after a press, the mod waits out vanilla's four ticks before placing what you looked at meanwhile — unless you turn the view by 10 degrees, which ends the wait at once.
- **Building backwards**: the block you just placed counts as fresh again once you have stepped a full block back from where you placed it.
- **Blocks that react to a click get vanilla's click alone**: doors, chests, buttons, levers and the rest are never given a second, synthesized use, and scaffolding on scaffolding climbs as in vanilla.
- **Settings** (Options > Mods > Accurate Block Placement), with the original's labels and defaults: **Accurate placement** on/off and **Allowed on servers** on/off (in your own worlds it always works), stored under the original's keys. A **toggle key**, unbound by default as in the original, flips the mod and says `Accurate Block Placement: ON`/`OFF` in chat, the original's words.

## What's different, and why

- **Vanilla's held-repeat stays on.** The original switches vanilla's own four-tick repeat off while it works, so holding still on one spot places exactly one block. A Kestrel client mod can add clicks but never take one away — there is no SDK surface for it — so in this port, holding still keeps vanilla's cadence (one block every four ticks). The fresh-target acceleration, the mod's actual feature, is unaffected.
- **The block-item test is learned, not asked.** The original asks the game whether the held item is a block item; the client SDK has no such query. The port instead watches the press: vanilla places the first block itself, and if the block that appears is the held item, the item is a block item and the rest of the hold is accelerated. A hold that begins looking at air is accelerated from the first block vanilla's own repeat places. An item the game has not placed as a block is never fired with.
- **Sampling is per tick, not per frame.** The original samples the crosshair every frame and places every tick; the SDK's crosshair read is a per-tick snapshot, so a target crossed and left within a single tick is not seen, and queued placements are performed in the next tick's input step (the `actions` bargain, at most ~50 ms later).
- **The "reacts to a click" list is the port's.** In the original it is the game's own interaction flag; the SDK doesn't expose one, so the port lists vanilla's interactive blocks itself (suffix families — doors, trapdoors, fence gates, buttons, beds, signs, shulker boxes — plus the named workstations, chests, redstone devices and storage blocks, and scaffolding). Anything missed still behaves exactly as vanilla; it just also gets the mod's timing.
- **"Allowed on servers" reads the world the way this catalog's ports do**: the SDK's world reads answer in your own worlds and stay silent on a remote server, so where they don't answer, the port treats the world as a server. A LAN host whose client reads don't answer is treated as a server too.

# Fair Freecam

A freecam that's safe to use anywhere: spectate your builds, scout a base or frame a screenshot — without ever seeing through a wall.

A behaviour port of [Fair Freecam](https://github.com/Lutziano888/fair-freecam) by Lutziano888 (MIT). It shares no code with the original: the original is a native mod, this is a fresh implementation against the Kestrel SDK (`render.set-camera`, API 14, plus world reads for Fair Play).

## How it works

- Press **F4** and fly the camera while the player stays put. Release with F4 again. `/fair_freecam` toggles it too, and Options > Mods > Fair Freecam has the same switch.
- **WASD** (your own Controls bindings) moves forward/left/back/right, **Space** up, **Shift** down. Hold **Ctrl** for 2.5× speed. The **mouse wheel** steps the speed: 2 / 5 / 10 / 20 / 40 / 80 blocks/s, starting at 10; the game keeps your speed between sessions.
- **Fair Play mode** (on by default): in single player the camera may not enter a solid block. Glass, fences and doors block; small plants and flowers do not.

## Differences from the original

- Fair Play's block check reads the destination block through the SDK's world reads, which are single-player only in this SDK — so on a multiplayer server the mod cannot check blocks. There, the game's own camera rules still bound every freecam: the host wall-clamps the camera and caps it at 64 blocks from the player. The "Allowed on servers" setting is kept as the label the original gives it.
- While the camera is detached, the mouse turns the camera, not the player — so the player keeps walking in the direction they faced, and WASD (which flies the camera here) also still walks the player: the SDK has no way for a mod to claim the movement keys. That is the one place this port is less "still" than the original implies.
- The original's movement keys are the player's Controls bindings; this port reads those same bindings through the SDK's mapping window, so rebinding WASD in Controls rebinds the flight too.

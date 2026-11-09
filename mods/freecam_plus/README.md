# Freecam+

Press U and fly the camera anywhere while the player stays perfectly still.

A behaviour port of [Freecam+](https://www.curseforge.com/minecraft/mc-mods/freecamplus) by _idom_ (MIT). It shares no code with the original: the original is a native mod, this is a fresh implementation against the Kestrel SDK (`render.set-camera`, API 14).

## How it works

- Press **U** (or `/freecam_plus`) and the camera detaches at your eye. WASD/Space/Shift and the mouse still work, but they drive the detached camera; press U again to return. Options > Mods > Freecam+ has the same switch.
- The camera flies on dedicated keys — **I** forward, **comma** back, **semicolon** left, **apostrophe** right, **equal** up, **minus** down (all rebindable) — the catalog freecam's cluster, because a mod in this SDK cannot fly a camera on the movement keys themselves.
- Fly speed: 2 / 5 / 10 / 20 / 30 / 40 / 60 blocks/s, default 10, switched on the settings page; the game keeps your speed between sessions.

## The stillness, honestly scoped

While the camera is detached, the mouse turns the camera, not the player — the player's yaw and pitch do not move, so the player keeps aiming in the direction they faced. That is the freeze this SDK's detach gives, and for a player who is standing still it is the original's promise exactly.

The one gap: the player's body is not frozen against held movement keys. WASD still walks the player while the camera flies, because the SDK has no way for a mod to claim the movement keys — the same named gap as every freecam in this catalog. "Stays perfectly still" holds for the player's look and for a player not walking; it does not hold against held movement keys.

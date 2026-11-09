# Freelook

Hold **X** (rebindable in Options > Mods) to look around freely while
your character keeps walking and facing the way it was. Let go and the
camera snaps back behind the player's look. A `[Freelook]` indicator
shows top-right while it is active.

`/freelook` does the same without the key — it latches freelook on and
off, which also covers touch players with no keyboard. Options > Mods >
Freelook switches the key between **Hold** (the default) and
**Toggle**; the choice is kept.

A port of [Freelook 0.3.1 by Krzysix17](https://github.com/Krzysix17/horizon-freelook)
(MIT). Ported for Kestrel by NepThunder. This port's home is
<https://github.com/NepThunder/Kestrel-mods/tree/main/mods/freelook>.

## How it works

While the key is held the mod calls `render.set-camera` once, with
`position: none` and the player's own yaw and pitch, and the host
routes the mouse to the camera instead of the player. The player keeps
walking, aiming and mining in the direction they faced, and the
server's picture of the player never changes — the camera is a view,
not a movement. The call happens only when the look changes: calling
`set-camera` every tick with fixed angles would override the mouse
and kill the look. Releasing the key (or `/freelook` off, or leaving
the world) calls `set-camera(None)` and the camera snaps back.

Needs Kestrel API level 14 (the camera functions
`render.get-camera` / `render.set-camera`, build 24 or later). It
works in singleplayer and multiplayer alike, like third person and
`render.shake` already do.

## Ported vs missing

Ported:

- Hold a key to decouple the look from the movement; release to snap back.
- The key is rebindable, and the original's default (X) is kept — no
  other Kestrel mod uses X, and X is unbound in vanilla.
- A HUD indicator while freelook is active.
- Hold/toggle as a setting, kept across restarts; `/freelook` to latch
  it on and off.

Missing — what the SDK cannot do, the original's native build can:

- **The third-person orbit.** The original pulls the camera out
  behind the player and scrolls it from 1 to 24 blocks; this port
  keeps the camera at the eye and only the look decouples (the
  freelook the camera design describes). An orbit would be a
  `position: some` call with the position computed from the camera's
  yaw and pitch; it is a follow-up, not this port.
- **Scroll zoom while held.** Same reason — it belongs to the orbit.

## What to test on device

On a 64-bit build (API 14 or later), in a singleplayer world:

1. Hold X and move the mouse: the view turns, the player model keeps
   facing the way it was, and walking forward (W) keeps going in the
   original direction.
2. Release X: the camera snaps back behind the player's look, and the
   mouse turns the player again.
3. While holding X, mine a block: the crosshair and the break follow
   the player's aim, not the camera — the player mines where *they*
   look.
4. `/freelook` on, put the phone down, walk around: the view stays
   decoupled until `/freelook` again.
5. Switch Mode to Toggle in Options > Mods > Freelook: one press of X
   latches it on, the next latches it off.
6. In multiplayer: other players still see you facing your movement
   direction, not the camera direction.

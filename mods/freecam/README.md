# Freecam

A port of [Freecam](https://github.com/hakergeniusz/freecam) by
hakergeniusz (MIT), itself a rewrite for Horizon of
[Freecam](https://github.com/MinecraftFreecam/Freecam) by the
Freecam authors (MIT) — detach the camera and fly it around while
your player stays where they were. Rewritten in Rust against the
Kestrel SDK; no original code runs. Not made or endorsed by
hakergeniusz or the Freecam authors, so report problems to the
Kestrel-mods project, not to them.

## What was ported

- **The toggle, three ways**: Y (rebindable on the mod's page in
  Options > Mods), `/freecam`, and the switch on that page. The
  camera detaches at your eye, looking where you look; toggling
  again snaps it back behind your own eyes.
- **Flying the camera**: while detached, I flies forward where the
  camera looks, comma flies back, semicolon left, apostrophe right,
  equal up, minus down — all rebindable on the mod's page. The
  mouse looks around while you fly. A diagonal flies at the same
  speed as a straight line, and opposite keys cancel.
- **The fly speed**: the settings page cycles 2, 5, 10, 20 or 40
  blocks per second (10 by default); the game keeps your choice.
- **The HUD notice**: "Freecam on — Y to return" and the speed sit
  in the top-left corner while detached, and the toggle answers in
  chat ("Freecam on" / "Freecam off"). It clears when you toggle
  off, leave the world, or unload the mod.
- **Your setting is never changed**: the mod only ever holds its
  own camera in the game, never writes your yaw, pitch, or
  perspective. Disabling or unloading the mod restores your own
  camera within a frame.

## What is missing

- **The original's F4 key.** F4 is vanilla's spectator-shader
  toggle in Horizon, so this port binds Y instead — the nearest
  free key with no vanilla or Kestrel use (Z is the Extras zoom
  key; V, R/U, K, H, N/M, F7, J and B are other Kestrel mods').
  Rebind it on the mod's page if Y suits you worse.
- **The movement keys cannot fly the camera.** The SDK has no
  movement-key claim, so WASD still walks the player while the
  camera is detached — the player stands where they were only in
  the sense that this mod never moves them. That needs a future
  `input.claim-keys` capability; until then the camera flies on
  its own dedicated keys, and walking the player while detached
  walks them in the direction they faced (the camera does not
  follow them past the 64-block cap — see below).
- **No wall-clip in v1.** The camera stops at the first wall it
  would pass through, as third person does; it cannot be parked
  inside a wall, a vault, or another player's base to see through
  it. Free-clip is a policy decision for later, not a bug here.
- **No tripod cameras.** The original's F4 + 1–9 saved camera
  spots are not ported in v1; they are mod-side storage, so they
  can come later with no SDK work.
- **No speed modifier key.** The original's Left Ctrl speeds the
  camera up, but Ctrl is vanilla Sprint and would sprint the
  player too; use the fly-speed switch instead.
- **The crosshair stays the player's.** Mining, attacking, and
  the crosshair target all follow where the *player* looks, not
  where the detached camera looks — the camera is a view, not an
  interaction. The player model shows from the detached camera
  (as in third person) and the hand hides; name tags may read
  edge-on under a detached camera, as they face the player's yaw
  rather than the camera's.

Two limits come from the camera API itself, not this port:

- **The camera stops 64 blocks from the player's feet.** Flying
  further out stops at that sphere; it does not snap back. A
  larger cap, or none in singleplayer, is a policy change.
- **The first loaded camera mod wins.** Another mod holding the
  camera (Freelook, a scripted camera) drives it instead of this
  one until it releases; this mod's notice may say on while the
  view is the other mod's. Disable the other mod to fly this one.
- **Multiplayer works.** The camera is render-only, like third
  person and shake, so it detaches on a server exactly as in
  singleplayer. Nothing is sent to the server, and other players
  see you facing the direction you faced — the server never learns
  where your camera is.

## Needs

API level 14 (`render.get-camera` / `render.set-camera`) — the
Kestrel build after 23.

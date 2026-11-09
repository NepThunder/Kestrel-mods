# Gyro

A port of [Gyro](https://github.com/CGRLabs/gyro)
by CGRLabs, MIT — full 6DOF elytra flight with camera roll/pitch/yaw,
inspired by the Java mod Do a Barrel Roll. Rewritten in Rust against
the Kestrel SDK; no original code runs. Not made or endorsed by
CGRLabs, so report problems to the Kestrel-mods project, not to them.

**Camera only.** This port moves the view while you glide. It does
not — and cannot, on this SDK — steer the flight. See [What is
missing](#what-is-missing).

## What was ported

- **The banked, rolling view while gliding**: while the gyro camera is
  on and you are gliding, the view banks and rolls with your look. Move
  the mouse and the horizon tilts — one degree of roll per degree you
  look away from your heading — up to a full 180° inversion when you
  look right round. That is the original's "mouse left / right is
  roll", expressed through the look the SDK can see.
- **The bank into a turn**: toggle it on mid-turn and the view banks
  into the turn while your velocity catches up with your heading, then
  settles as the glide straightens. The bank is the angle between your
  velocity and your heading, halved and capped at 30°.
- **The ease back to level**: the roll eases toward its target 10° a
  tick, and eases back to level when you stop gliding or toggle off,
  as the original does.
- **The toggle**: Y (rebindable in the controls as usual), `/gyro on`
  and `/gyro off`. The original has no keybinds and no config, and
  neither does this port beyond the toggle — there is no settings page.
  The game keeps the choice across reloads. `/gyro` on its own reports
  the roll and speed.
- **The HUD line**: "Gyro: roll +30°, 10 m/s" sits in the top-left
  corner while the camera is held.
- **Your settings are never changed**: the mod only ever holds its own
  camera override, never writes your yaw, pitch or any setting.
  Disabling or unloading the mod restores your own camera within a
  frame.

## How the roll is driven

The SDK exposes no velocity and no fall-flying flag — `game.get-player`
gives feet, yaw, pitch, health, food and dimension — so both drivers
are derived from that:

- **Look bank** (the live driver): the camera's yaw minus your
  heading, read back through `render.get-camera` each tick. While a
  camera is active the mouse turns the camera, not the player, so the
  mouse is the roll control.
- **Slip bank** (the transient driver): your horizontal velocity from
  feet positions across ticks, against your heading.

The glide gate is a speed heuristic, because the SDK cannot see the
elytra: the camera engages after 3 ticks above 8 m/s horizontal and
releases after 2 ticks below 5 m/s. A sprint (5.6 m/s) never engages;
a fast horse or a boat on ice can, and banks the same way — the toggle
is the control, and the gate only stops the mod taking your mouse on
foot.

## What is missing

- **No 6DOF flight steering — the whole flight half of the original.**
  In the original, rolling the view banks the glide: pull through the
  bank and you carve a turn, push past vertical and you loop. That
  needs the flight itself to steer by the roll, and the SDK has no
  movement or physics capability: `travel_fall_flying` steers the
  velocity by the player's pitch/yaw with no roll in it, and a camera
  mod cannot write the player's yaw or pitch. Steering is a separate
  capability with its own spike and policy call, not part of this port,
  and none of it is smuggled in here.
- **While the gyro camera is on, the mouse turns the camera, not you.**
  Your glide holds the heading you had when it engaged — you cannot
  steer with the mouse until you toggle it off (or land, which
  releases it). That is the freelook contract the camera API is built
  on: the camera is a view, not a control. Toggle off to steer, on to
  look around with a banked view.
- **No mouse up / down pitch steering.** The original pitches through
  the bank to carve a turn; here the mouse pitches the camera only,
  and the glide ignores it.
- **No third-person pinned model.** The original pins your avatar,
  elytra and held item to the camera in third person while the world
  rolls. This port keeps the camera at the eye (freelook), so there is
  no detached view to pin a model to.
- **The original's G key.** G is vanilla Quick Actions, so this port
  binds Y instead — the nearest free key with no vanilla or Kestrel
  use. The original binds nothing; the key exists here because the
  camera cannot be always-on while gliding without taking your mouse
  steering (see above), so the toggle is the control.
- **No settings page.** One toggle needs none; the key is rebindable
  in the controls, and `/gyro` covers the rest. A maximum-roll cap is
  a possible follow-up if the full 180° inversion proves too much.

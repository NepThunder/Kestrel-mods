# Speedrun Timer

An RTA speedrun timer in a corner of the screen: `HH:MM:SS.mmm` with
live milliseconds. A port of [Speedrun Timer](https://github.com/Demerobit/Speedrun-Timer-Horizon-Rust-Minecraft)
by Demerobit (MIT), ported for Kestrel by NepThunder. Rewritten in Rust
against the Kestrel SDK; no original code runs. Not made or endorsed by
Demerobit, so report problems to the Kestrel-mods project, not to them.

## What it does

- **Starts** on your first move after a world loads (armed at zero
  while you stand still — looking around does not start it).
- **Stops** when you take the exit portal back to the Overworld after
  the End. Dying is not a finish: the run keeps counting through the
  death screen and the respawn.
- **Resets** on its own when you leave a world for about a second,
  ready for the next run.
- **Saves** every finished run and your personal best in the mod's
  storage (the newest 500 runs); the settings page has a button to view
  them, newest first, with the personal best marked. A run stopped by
  hand with `/speedrun stop` is saved too, but never counts as a
  personal best.
- `/speedrun` says where the timer is; `/speedrun start`,
  `/speedrun stop` and `/speedrun reset` drive it by hand.
- Options > Mods > Speedrun Timer has the corner (top left, top right,
  bottom left, bottom right), the shown and personal-best switches, the
  saved-runs view and a reset.
- It hides with F1 like the rest of the HUD, and the top-left corner
  gives way to the F3 debug screen — both are the game's own rules for
  HUD panels, not something the mod does itself.

## Requirements

Runs on Kestrel build 9+ (API level 10). The interfaces it uses are all
level 0 (`game-read`, `chat`, `hud`, `storage`, `ui`), so it needs
nothing newer than that.

## What is missing, and why

The original is a native mod: it reads the game's own hooks for world
loads and portal exits and its own wall clock. The SDK exposes no
world-load event, no portal event and no wall clock, so this port
infers the same events from what the SDK does expose — the player's
position, dimension and health from `game-read`, and the frame hook's
delta for the clock. The inference is faithful to the original's rules
(move more than 0.05 blocks to start; End → Overworld after having
been in the End to finish; a second out of a world to reset), but the
edge cases the original gets from real events can misfire here:

- **Starting late or early.** Loading into a world already moving (falling,
  or walking on join) starts the run on the first frame that sees the
  movement, and a world that loads with the player mid-air arms at the
  landing spot only after the first still frame — the original arms at
  the exact load position from its world-load hook.
- **Finishing without the portal.** Any End → Overworld change after
  the run visited the End finishes it, not just the exit portal: dying
  in the End and respawning is caught (the death suspends the reading
  until the player is alive again), but a respawn anchor in the Nether
  or a command teleport out of the End would finish it too.
- **The clock is the frame delta.** The timer advances with rendered
  frames, so the milliseconds step by frame intervals (about 17 ms at
  60 fps, 8 ms at 120 fps) rather than continuously, and it does not
  advance while the game renders no frames at all.
- **No folder button.** The original's settings page opens the timer's
  folder in the file manager; a sandboxed mod has no file manager, so
  the button opens the saved runs in-game instead.
- **No box, no per-phase layout.** The original draws its own
  translucent box with gold/green/grey text. The SDK's HUD panel is
  one panel of plain text lines in a corner, so the timer is one line
  in the phase's colour (gold running, green done, grey waiting) with
  the personal best beside it when the switch is on.
- **An in-progress run does not survive a reload.** Only finished runs
  and the personal best are stored; restarting the game or the mod
  mid-run arms the timer fresh, as the original's in-memory run does.

Two small fixes against the original, both in the same spirit:
`/speedrun start` after a finished run no longer finishes the new run
at once (the finish line is taken from the dimension the player is in
now), and a hand-stopped run is saved without touching the personal
best, which the original's stop path also does.

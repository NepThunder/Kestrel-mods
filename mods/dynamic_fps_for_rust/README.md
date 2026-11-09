# Dynamic FPS

Caps the frame rate after a minute without input — the one tier of the original this SDK can express, with the original's own delay and rate.

A behaviour port of Dynamic FPS for Rust by Lyvt (MIT), following its Horizon registry entry. It shares no code with the original: this is a fresh implementation against the Kestrel SDK (capabilities `render` + `input`, API 19).

## What the original does

Caps the frame rate by window state:

- 30 FPS while another window hovers over the game,
- 10 FPS when unfocused, after a 0.5 s grace,
- 1 FPS when minimised,
- 20 FPS after 1 minute without input,
- and a reduced 30 FPS while on battery power,

with an option to lower the game's volume in the background. Configuration lives in a file; there is no settings screen.

## What this port does

- Watches the game's own inactivity clock (`input.last-input-ms` — the same clock the game's idle framerate limit reads, counting every key, click, mouse or touch movement, and gamepad or touch-control action).
- After 60 seconds without input, caps the frame rate at 20 FPS through the game's frame limiter (`render.set-fps-cap`); the next input lifts the cap at once. The world still ticks at full rate — only frames are paced, as in the original.

That is the original's idle tier, whole: its delay, its rate, its restore.

## Gaps — features the SDK cannot express yet

- **The window-state caps (30 hovered / 10 unfocused / 1 minimised).** The engine consumes focus and iconification inside its own limiter and reports neither state to a mod (on Android the surface stops entirely when the app is backgrounded, which no mod code runs through). The engine's own limiter already acts on those states with vanilla's numbers; the original's per-state values have no mod surface.
- **The battery cap.** No SDK surface reports the power source. The game's own battery limit already runs inside the same frame limiter, ahead of any mod cap.
- **No volume lowering.** The original's background volume option adjusts the game's master volume; the SDK's audio surface only lets a mod quieten individual world sounds, never the master or music volume, so the option is absent rather than approximated.
- **No config file.** The original's per-state FPS values live in `config/dynamic-fps-for-rust.json` in the game folder; a mod cannot read the game's config folder, so the one expressible tier uses the original's defaults — 60 seconds, 20 FPS — fixed. There is no settings screen, as in the original.

# MoreGamma

A port of [MoreGamma](https://github.com/bezi5kt/gamma-rust)
by bezi5kt, MIT — set the brightness (gamma) far past the video
settings' 100% "Bright" limit, with a chat command, and switch full
brightness on and off. Rewritten in Rust against the Kestrel SDK; no
original code runs. Not made or endorsed by bezi5kt, so report
problems to the Kestrel-mods project, not to them.

## What was ported

- **`/gamma <value>`**: your own gamma. `1` is vanilla's "Bright",
  `10` is fullbright, the brightest the SDK allows. The host clamps
  every value to 0.0..=10.0, and the command prints the gamma in
  effect back in chat, so `/gamma 15` answers `gamma 10`.
- **`/gamma` on its own**: shows the gamma in effect — your
  Brightness setting when the mod holds nothing, the mod's value
  when it does.
- **`/gamma 1500%`**: percentages read against vanilla's "Bright",
  as in the original — `100%` is 1, `1500%` is 15, clamped to 10.
- **`/gamma reset` (or `off`)**: goes back to your Brightness
  setting — the one in effect when the mod first set a gamma.
- **`/gamma on`**: turns your last custom gamma on again (the value
  as read back after the host clamped it).
- **`/fullbright`**: toggles full brightness (gamma 10) on and off,
  answering `fullbright on: gamma 10` and
  `fullbright off: gamma 0.5`.
- **Your Brightness setting is never changed.** The mod only holds
  its own gamma while it runs: disabling or unloading it restores
  your setting within a frame, and `options.txt` is untouched.
  Gamma works in singleplayer and multiplayer — it only changes
  what your client renders, like the Extras fullbright switch.

## What is missing

The SDK does not expose these, so this port has neither:

- **Values past 10.0** — the original goes to 100 and calls 15
  fullbright. The SDK clamps host-side to 0.0..=10.0, so 10.0 is
  this port's fullbright and anything higher reads back as 10.
  Past 1.0 the lightmap saturates toward white, so colours wash out
  as the value grows; 10.0 is near-white in the dark, not the
  original's uniformly full lightmap.
- **The Options > Mods page** — the original has an on/off button,
  a button cycling 100/200/500/1000/1500%, and a toggle key you
  can bind. This port is commands only: `/gamma` and `/fullbright`.
- **Remembering your gamma between sessions** — the original keeps
  its settings. This port keeps the last gamma in memory only:
  reloading the mod restores your Brightness setting, and
  `/gamma on` then answers that no custom gamma is set until you
  set a value again.

Two limits come from the gamma itself, not the API:

- **The Darkness effect subtracts from gamma first.** A gamma of
  10 still beats it; a gamma of 0.5 does not.
- **While the mod holds a gamma, the Brightness slider goes dead.**
  The slider still writes your setting, but the screen does not
  change until the mod releases it (`/gamma reset`, `/fullbright`
  off, or disabling the mod) — then the new setting applies.

## Needs

API level 13 (`render.get-gamma`, `render.set-gamma`) — Kestrel build 19 or later.

# Fullbright & NoFog

A port of [Fullbright & NoFog](https://github.com/Nirokx/horizon-fullbright-nofog)
by Nirokx, MIT — press a key to turn maximum brightness and fog
removal on together. Rewritten in Rust against the Kestrel SDK; no
original code runs. Not made or endorsed by Nirokx, so report
problems to the Kestrel-mods project, not to them.

## What was ported

- **The combined toggle**: B (rebindable on the mod's page in
  Options > Mods), `/fullbright-nofog`, or the switch on that page
  turns the effect on or off. The original has two separate toggles
  (G for fullbright, N for fog); this port is one toggle for both,
  because the two effects are one feature here and one key is easier
  to reach mid-game.
- **The fade**: turning it on fades the gamma from the player's own
  Brightness up to 10.0 — the 1000% Fullbright value — over about 20
  ticks (1 second); turning it off fades it back to the Brightness
  the player had before. The fade switch on the settings page turns
  the fade off, so the gamma jumps straight to its target instead.
- **The fog removal**: distance fog goes off while the effect is on
  and comes back when it goes off. The Nether's 96-block fog wall
  goes with it, and so does the Overworld's render-distance haze.
- **The HUD notice**: a corner panel says "Fullbright & NoFog on"
  while the effect is on, and clears when it goes off.
- **The remembered Brightness**: the player's Brightness slider and
  `options.txt` are never changed; turning the effect off gives the
  player's own brightness back, and disabling or unloading the mod
  restores it too.
- **Multiplayer**: works on servers as well as in singleplayer —
  gamma and fog change only what the local client renders, the same
  effect the Extras fullbright switch and the Fog graphics option
  already grant in multiplayer.

## What is missing

The SDK does not expose these, so this port has neither:

- **`/nofog fluids`** — the original's separate fog command also
  clears water, lava and powder-snow fog. `set-fog(false)` removes
  distance fog only (see below), and there is no fluid-fog call.
- **Two separate toggles** — the original's G and N are one B here
  (see the key note below).

Two limits come from the rendering itself, not the API:

- **The fade is tick-stepped, about 20 ticks, not frame-smooth.**
  There is no per-frame hook for mods, so the gamma moves once a
  tick; the original's 250 ms smooth fade is 20 visible steps
  instead of a continuous ramp.
- **Gamma 10.0 saturates toward white.** The lightmap lerps past
  full correction and clips, so colours wash out as the value grows
  — it is not the Extras fullbright flag, which fills the lightmap
  uniformly and keeps colours. At 10.0 the picture is close to the
  flag's, but the darkest texels may read grey rather than white.

## The fog limits — read this before reporting fog as a bug

`set-fog(false)` removes **distance fog only**: the atmospheric and
render-distance fog, including the Nether's 96-block fog wall. The
following fog **stays**, by design of the engine call this reuses:

- the fog of water, lava and powder snow when the camera is in one;
- blindness fog, darkness fog, and a boss's world fog.

Swimming still looks like swimming, and the Darkness effect still
darkens. That is the same removal the Fog setting in the graphics
options does — this mod just drives it from a key.

## Key and command choice

B is the default key because G — the original's fullbright key — is
not free in practice: the parallel Fullbright port uses J (verified
in its checkout while this port was written), and N — the original's
fog key — is Waypoints' create key. B is free among the Kestrel mods
and unbound in vanilla. The command is `/fullbright-nofog` (with `on`,
`off`, or `status`, which reads the effective gamma and fog back)
rather than `/fullbright`, so this mod and the Fullbright port never
fight over a command name when both are installed.

## Known limits

- **The Brightness slider looks dead while the mod is loaded.**
  The SDK has no call for a mod to release its gamma — once set, a
  mod holds a value until it unloads — so after the first toggle the
  mod holds the remembered Brightness even while the effect is off,
  and a slider change does not show until the mod is disabled or
  unloaded. This is an API v1 limit shared by every gamma mod, not a
  bug in this port; the remembered value itself is correct, and
  unloading the mod restores the slider at once.
- **Two gamma mods fight by maximum.** The strongest gamma any
  running mod sets wins, so another mod holding 10.0 keeps the
  screen bright while this one is off, and vice versa. Disabling a
  mod is the escape hatch.

## Needs

API level 13 (`render.get-gamma`, `render.set-gamma`,
`render.get-fog`, `render.set-fog`) — Kestrel build 19 or later.

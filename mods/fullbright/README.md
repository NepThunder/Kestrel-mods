# Fullbright

A port of [Fullbright](https://github.com/CayMac97/fullbright)
by CayMac, MIT — toggle maximum brightness (1000% gamma) with a
fade and a HUD notice. Rewritten in Rust against the Kestrel SDK;
no original code runs. Not made or endorsed by CayMac, so report
problems to the Kestrel-mods project, not to them.

## What was ported

- **The toggle, three ways**: J (rebindable on the mod's page in
  Options > Mods), `/fullbright`, and the switch on that page.
  The page also has the fade switch; the game keeps both choices.
- **Maximum brightness**: gamma 10.0, the original's 1000%.
- **The fade**: on toggle-on the gamma animates from your current
  brightness to 10.0, and on toggle-off it animates back to the
  brightness you had. About a second either way (see below).
- **The HUD notice**: "Fullbright on" sits in the top-left corner
  while it is on, and the toggle answers in chat ("Fullbright on" /
  "Fullbright off"). It clears when you toggle off or leave the
  world.
- **Your setting is never changed**: the mod only ever holds its
  own gamma in the game, never writes your Brightness setting.
  Disabling or unloading the mod restores your setting within a
  frame.

## What is missing

- **The fade is ~20 ticks, not the original's 250 ms.** A mod can
  only step gamma once a tick (20 steps at 20 tps is about a
  second); there is no frame hook to fade on. Turn the fade off on
  the settings page for an instant toggle.
- **Gamma 10.0 saturates toward white.** The lightmap lerp
  extrapolates past 1.0 and the pixels clip, so colours wash out
  next to the Extras fullbright flag, which fills the lightmap
  uniformly and keeps them. If 10.0 looks materially worse than
  the flag on device, that is a case for exposing the flag, not
  for a bigger gamma.
- **The original's G key.** G is vanilla Quick Actions, so this
  port binds J instead — the nearest free key with no vanilla or
  Kestrel use (Z is the Extras zoom key; H, K, N/M, V, R/U are
  other Kestrel mods'). Rebind it on the mod's page if J suits
  you worse.

Two limits come from the gamma API itself, not this port:

- **While the mod is loaded, your Brightness slider is shadowed.**
  `set-gamma` has no unset: once the mod sets a gamma it holds
  that value until disabled or unloaded, so after toggling off it
  keeps holding the brightness you had when you first turned it
  on. Changing Brightness in Options while the mod is loaded has
  no visible effect until you disable or unload the mod — then
  your setting applies again within a frame.
- **The strongest gamma any running mod sets wins.** Another mod
  holding more than 10.0 would win over this one; one holding
  less never dims it.
- **Multiplayer works.** Gamma is render-only, like the Extras
  fullbright, so it brightens your screen on a server exactly as
  in singleplayer. Nothing is sent to the server.

## Needs

API level 13 (`render.get-gamma` / `render.set-gamma`) — Kestrel
build 19 or later.

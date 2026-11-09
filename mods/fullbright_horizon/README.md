# Fullbright

Toggle maximum brightness with a smooth fade — caves and nights become fully visible while the world keeps its shapes and colours.

A behaviour port of [Fullbright](https://github.com/CayMac97/fullbright) by CayMac (MIT), the registry's `fullbright_horizon`. It shares no code with the original: the original is a native mod, this is a fresh implementation against the Kestrel SDK (`render.set-gamma`, API 13).

## How it works

- Press G to toggle maximum brightness (1000% gamma); a notice on the HUD says which way it went. `/fullbright_horizon` toggles it too, and Options > Mods > Fullbright has the same switch. The game keeps your choice between sessions.
- The brightness fades, it does not snap: on, the light rises smoothly to full; off, it sinks back to the brightness you had. Pressing the key mid-fade reverses the fade from where it is. (The fade is the original's 250 ms, eased in and out.)
- Client-side only: it changes your lightmap, nothing sent to a server, no server can ban it. It works on any server.

## Notes

- Gamma 10 saturates the lightmap toward white, so colours wash out a little compared to a uniformly filled lightmap.
- While the mod is loaded it holds the brightness it last set (the SDK's gamma has no unset), so the Options Brightness slider is shadowed until the mod is disabled or unloaded; your own setting itself is never changed.
- This is the registry's `fullbright_horizon`. The catalog also carries CayMac's other Fullbright (a port of the same author's original mod) as `fullbright` on J with a tick-stepped fade; this one keeps the original's G binding and a per-frame fade.

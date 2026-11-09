# Zoom

Hold a key to zoom in; the mouse wheel zooms further while you hold it.

A behaviour port of [Zoom](https://github.com/horizon-rust/horizon-mods/tree/main/native/zoom) by Horizon (MIT), Horizon's own zoom, formerly an extra in Options > Horizon Extras. It shares no code with the original: the original is a native mod, this is a fresh implementation against the Kestrel SDK (`render.set-zoom` + `input`).

## How it works

- Hold the key (Z by default) to zoom in at the starting zoom; let go and the view comes back.
- While you hold it, the mouse wheel zooms further in or out (up to 10x) and the hotbar stays put.
- Smooth zoom eases in and out like the spyglass; slower turning keeps aiming steady.

## Settings

Options > Mods > Zoom — the original's three, with the original's defaults: starting zoom 4x (the button cycles 2, 3, 4, 6, 8, 10), smooth zoom on, slower turning on. The key itself is rebindable on the same page. The game keeps the choices between sessions.

## Differences from the original

None known in behaviour. The original's settings lived in its own files (`level`, `smooth`, `sensitivity`); this port keeps the same file names and values in the game's per-mod storage, so the choices mean the same thing in both.

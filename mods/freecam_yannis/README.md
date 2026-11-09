# FreeCam

Record videos of your world — or just watch yourself play from the outside.

A behaviour port of [FreeCam](https://www.curseforge.com/minecraft/mc-mods/freecam-yannis) by Yannis_ (MIT). It shares no code with the original: the original is a native mod, this is a fresh implementation against the Kestrel SDK (`render.set-camera`, API 14; `render.set-hide-hud`, API 19).

## How it works

- **F4** starts the free camera; fly it on **I** / **comma** / **semicolon** / **apostrophe** / **equal** / **minus** (the catalog freecam's cluster, all rebindable) at 2–60 blocks/s. **F4** again ends it. `/freecam_yannis` toggles it too.
- **F6** switches to a follow view: the camera parks behind and above you — follow distance 28 blocks, height 18 (both adjustable in Options > Mods > FreeCam) — and the HUD hides while you follow, as the original hides the crosshair there.
- **Tripods:** hold **F4** and press **1–9** to save the current camera position as that tripod. Press **1–9** with the camera off to start the free camera at a saved tripod. Tripods are kept per world.

## Differences from the original

- The original's follow view is live: its camera follows the moving player. Here F6 parks the camera at the hover — behind and above where you stood, looking back at you — and it stays parked; it does not track you as you move. A mod camera in this SDK is a position set per call, and a mod cannot keep re-aiming it at a moving player without fighting the camera rules every other mod plays by.
- The original hides only the crosshair in follow view; the SDK's HUD hide is all-or-nothing (what F1 hides), so the whole HUD goes while you follow.
- The tripod keys are the mod's own Controls bindings, defaulting to the number row: they can be rebound separately from the hotbar, and rebinding a hotbar slot does not move them. Pressing one still selects its hotbar slot too — the game's own binding sits on the same physical key, and no SDK surface can claim a key away from the game's own mapping.
- Tripods are "per world" keyed by the dimension's name — the only world identity a client mod can read — so two different saves share tripods per dimension rather than keeping separate sets per save.
- The flight keys and speeds are the catalog freecam's idiom: the original states no fly keys or speeds of its own, and a mod in this SDK cannot fly a camera on the movement keys themselves.

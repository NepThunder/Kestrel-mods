# Zoomify

A zoom key with smooth transitions, scroll zoom, a secondary zoom level and a settings page.

A behaviour port of [Zoomify](https://modrinth.com/mod/zoomify) by isXander (LGPL-3.0-or-later), the Fabric mod, ported to the Horizon registry by DeportedToHell. It shares no code with the original: the original is a Fabric mod, this is a fresh implementation against the Kestrel SDK (`render.set-zoom` + `input`).

## How it works

- **C** zooms (hold by default; the settings can make it a toggle). The view eases to the initial zoom (4x) and back, spyglass-style.
- **Scroll** while zoomed to zoom further in or out. The original sets no distance limit; the game caps any mod zoom at 250x, so the factor stops there.
- **F6** is the secondary zoom: a slow, cinematic transition to its level — 50x by default, with 10x/50x/200x on the settings page — and the HUD hides while it is active, as in the original.
- The hand's field of view follows the view's while zoomed, in the settings page's behaviour (multiply, add or keep); **F7** toggles it, as in the original.
- While you scope with a spyglass, the zoom stands down and the spyglass's own view shows; the zoom returns when you lower it.
- Sensitivity follows the zoom (relative sensitivity), and the transitions ease cinematically while zoomed.

## Settings

Options > Mods > Zoomify (the original's own settings screen is a YACL page; mods in this game get the standard settings page, with the same options and defaults): zoom key behaviour (hold/toggle), initial zoom, hand FOV on/off and its behaviour (multiply/add/keep), relative sensitivity, cinematic camera, secondary initial zoom. All three keys are rebindable on the same page.

## Differences from the original

- The secondary zoom's cycling in the original steps through 10x/50x/200x with the mouse buttons while it is active. This SDK has no mouse-button events for mods, so the cycling lives on the settings page instead (Secondary initial zoom), and the level it sets is where F6 goes.
- The spyglass rules are only partly a mod's to give. In the original, the zoom key can scope for you while you hold a spyglass, and its spyglass-behaviour options combine the two zooms or let one override the other. A mod cannot scope, and the host keeps a mod zoom out of the scope's view entirely — so this port stands its zoom down while you scope and has no spyglass-behaviour setting. The spyglass's overlay and sounds are the game's own, as they are under the original.
- The original's transition speed setting (a percentage, default 10%) has no exact equivalent here: the port eases exponentially at a fixed rate chosen to feel like the original's default.
- "Cinematic camera" eases the camera while zoomed in the original's engine; here the zoom's own eased transition carries that feel, and the setting is kept for parity.

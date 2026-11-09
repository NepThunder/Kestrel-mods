# Punchy

The first-person hand lags behind your look on a spring, attacks with authored animations, and bobs with more weight.

A behaviour port of Punchy! by PunchyDevGuy (All Rights Reserved). It shares no code or assets with the original: Punchy! is a closed-source Java mod and resource pack, and this is a fresh implementation of its feel against the Kestrel SDK (API 16's hand state and API 17's swing terms, capability `render`). The original's resource-pack look and its third-person changes are out of scope.

## How it works

Every rendered frame the game hands the mod the hand's state — look direction and how far it turned this frame, swing progress, the view bob's phase and strength — and the mod hands back one transform the game multiplies into the poses of both the arm and the held item:

- **Sway.** Two damped springs, one per look axis. A turn drags the hand behind the view; the springs pull it back with a touch of overshoot, so quick looks feel like the hand has weight.
- **Attacks.** The mod reads which swing animation is playing and scales the vanilla swing itself: a sword's swing is replaced outright by an authored slash, and chained sword swings alternate direction — a combo. Each tool has its own stroke, like the original: a pickaxe swings forward, an axe chops in from the side, a shovel digs down and forward (and holds its depth between strokes while you keep digging). A spear's stab is an authored jab; anything else keeps half the vanilla motion with a follow-through curve on top. Every attack settles home with a small overshoot instead of stopping stiff.
- **Bob.** Extra hand bob on the game's own bob cycle, so it stays in step with the camera: a sideways sway, a dip, and a roll scaled by how strong the bob is.
- **Settings** (Options > Mods > Punchy): Intensity (0/25/50/75/100%, default 75) and switches for Sway, Attacks and Bob. `/punchy` reports them. While a two-handed map is held, the offset is skipped entirely — that pose is the map's own.

## Limits in v1

- First-person only: that is all the hand API poses.
- One transform covers both hands, as the API's state does.
- The feel was first tuned blind; 0.1.1 retuned it from on-device testing (the bob emphasis now uses the game's real 0..0.1 bob scale, and the follow-through is stronger). 0.2.0's authored slash and jab were tuned blind; 0.2.1 retuned from the first device pass (slash closer to vanilla, tools and bob stronger). 0.3.0 is shaped against the original mod's own page and gameplay loops: combos, per-tool strokes with a start, a loop and an end, and the settle.

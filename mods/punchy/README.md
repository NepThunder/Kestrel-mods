# Punchy

The first-person hand lags behind your look on a spring, attacks with authored animations, and bobs with more weight.

A behaviour port of Punchy! by PunchyDevGuy (All Rights Reserved). It shares no code or assets with the original: Punchy! is a closed-source Java mod and resource pack, and this is a fresh implementation of its feel against the Kestrel SDK (API 16's hand state and API 17's swing terms, capability `render`). The original's resource-pack look and its third-person changes are out of scope.

## How it works

Every rendered frame the game hands the mod the hand's state — look direction and how far it turned this frame, swing progress, the view bob's phase and strength — and the mod hands back one transform the game multiplies into the poses of both the arm and the held item:

- **Sway.** Two damped springs, one per look axis. A turn drags the hand behind the view; the springs pull it back with a touch of overshoot, so quick looks feel like the hand has weight.
- **Attacks.** The mod reads which swing animation is playing and scales the vanilla swing itself: a sword's swing is replaced outright by an authored slash (a wind-up, a fast cut across, a recovery), a spear's stab by an authored jab, and any other swing keeps half of the vanilla motion with a follow-through curve on top — forward, dipped and pitched over the swing.
- **Bob.** Extra hand bob on the game's own bob cycle, so it stays in step with the camera: a sideways sway, a dip, and a roll scaled by how strong the bob is.
- **Settings** (Options > Mods > Punchy): Intensity (0/25/50/75/100%, default 75) and switches for Sway, Attacks and Bob. `/punchy` reports them. While a two-handed map is held, the offset is skipped entirely — that pose is the map's own.

## Limits in v1

- First-person only: that is all the hand API poses.
- One transform covers both hands, as the API's state does.
- The feel was first tuned blind; 0.1.1 retuned it from on-device testing (the bob emphasis now uses the game's real 0..0.1 bob scale, and the follow-through is stronger). 0.2.0's authored slash and jab were tuned blind; 0.2.1 retunes them from the owner's device pass — the slash sits closer to vanilla (shorter wind-up, keyframes roughly halfway back), the tools keep more vanilla swing under the follow-through, and the bob weights rise.

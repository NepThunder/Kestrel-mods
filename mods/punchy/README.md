# Punchy

The first-person hand lags behind your look on a spring, follows through on your attacks, and bobs with more weight.

A behaviour port of Punchy! by PunchyDevGuy (All Rights Reserved). It shares no code or assets with the original: Punchy! is a closed-source Java mod and resource pack, and this is a fresh implementation of its feel against the Kestrel SDK (API 16, capability `render`). The original's resource-pack look and its third-person changes are out of scope.

## How it works

Every rendered frame the game hands the mod the hand's state — look direction and how far it turned this frame, swing progress, the view bob's phase and strength — and the mod hands back one transform the game multiplies into the poses of both the arm and the held item:

- **Sway.** Two damped springs, one per look axis. A turn drags the hand behind the view; the springs pull it back with a touch of overshoot, so quick looks feel like the hand has weight.
- **Attacks.** While an attack swing runs, a follow-through curve peaking mid-swing pushes the hand forward, dips it and pitches it over the swing.
- **Bob.** Extra hand bob on the game's own bob cycle, so it stays in step with the camera: a sideways sway, a dip, and a roll scaled by how strong the bob is.
- **Settings** (Options > Mods > Punchy): Intensity (0/25/50/75/100%, default 75) and switches for Sway, Attacks and Bob. `/punchy` reports them. While a two-handed map is held, the offset is skipped entirely — that pose is the map's own.

## Limits in v1

- First-person only: that is all the hand API poses.
- One transform covers both hands, as the API's state does.
- The feel is tuned blind — the spring weights and follow-through sizes are a first pass and want on-device iteration.

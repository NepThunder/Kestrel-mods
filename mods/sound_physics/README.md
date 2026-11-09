# Sound Physics

Sounds are quietened and muffled by the blocks between you and them, and enclosed spaces — caves, rooms, tunnels — give them reverb.

A behaviour port of [Sound Physics Remastered](https://github.com/henkelmax/sound-physics-remastered) by henkelmax (GPL-3.0), in the lineage of Sonic Ether's Sound Physics through vlad2305m's Fabric port. It shares no code with the originals: they are Java mods, this is a fresh implementation against the Kestrel SDK (API 15, capability `audio`).

## How it works

- **Occlusion.** When a world sound starts, the mod samples the blocks along the line from the sound to your ears. The fraction of the line inside solid blocks sets how much the sound is quietened (down to 8% through a fully solid line) and muffled.
- **Reverb.** Ten times a second the mod probes the blocks around you in 17 directions at and above eye level (the ground below is not counted, so open sky reads as open). The enclosed fraction drives a shared reverb in the game's mixer: open sky is dry, a cave rings for seconds.
- **Settings** (Options > Mods > Sound Physics): Reverb (0/25/50/75/100%) and Occlusion (same steps). `/soundphysics` reports the current enclosure estimate and the settings.

## Limits in v1

- Every solid block absorbs alike (no per-material table yet).
- The reverb's character is one global estimate around you, not per-sound.
- Single-player only: the SDK's world reads are, in v1. On a multiplayer server the mod leaves sounds alone.
- Underwater sounds are left to vanilla, which already muffles them.

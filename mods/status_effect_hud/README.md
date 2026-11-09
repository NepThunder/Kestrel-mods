# Status Effect HUD

Every active effect on the HUD: the game's own icon, the effect's name and level, a colour-coded countdown, and a duration bar in the effect's vanilla potion colour.

A behaviour port of Status Effect HUD by CayMac (MIT, https://github.com/CayMac97/status-effect-hud), itself a port of Status Effect Bars and Inventory HUD+ (Potion HUD). It shares no code with the original: this is a fresh implementation against the Kestrel SDK. The port is fed by API 18's self reads (`game.get-effects`, capability `game-read`) — the district that exists because this genre had no way to read the local player's own effects before it.

## What it draws

One row per active effect, in the game's own order, at the top left of the HUD:

- The effect's official `mob_effect` sprite, framed by the game's own effect background (the ambient frame for beacon and conduit effects, as the game's HUD frames them).
- The effect's name with its Roman numeral level past I: `Speed II`, `Strength IV`.
- The time left as mm:ss, colour-coded exactly as the original: **white** while more than 30 seconds remain, **yellow** from 30 down past 10 seconds, **flashing red** at 10 seconds and under, and an **aqua ∞** for a permanent effect.
- A smooth duration bar under the row, coloured in the effect's authentic vanilla potion colour — the same `MobEffects` colour table the game itself carries.

The rows are drawn with the SDK's HUD shapes (`render.set-hud`), rebuilt each tick so the bars run smoothly and the urgent flash flashes.

## Fidelity

Matches the original's registry description:

- All active potion and beacon effects, on the HUD during gameplay.
- The game's own mob_effect sprites.
- Roman numeral amplifiers (II, III, IV — level I wears none).
- The colour-coded mm:ss countdowns, with the original's exact bands and the aqua infinity symbol.
- Smooth duration bars in each effect's authentic vanilla potion colour.

Documented gaps — nothing substituted, nothing invented:

- **Translated effect names.** The original prints the game's translated effect names; the SDK exposes no translation lookup, so the name shown is the effect id's own words (`Fire Resistance` for `minecraft:fire_resistance`). In English those are the same words; in another language they stay English where the original would translate.
- **The bar's full length.** A progress bar needs the effect's total duration; the SDK reports only the time left. Like the original genre, the bar is drawn against the longest remaining time seen for that effect (at its current level) since it appeared — a mid-flight refresh to a longer time raises the bar's full length, exactly as it does in the originals.
- **No settings page and no command** — the original's registry entry describes neither, so the port adds neither.

## Requirements

Needs a Kestrel build with the SDK self reads (API level 18): `game.get-effects` is the only function this mod imports beyond the HUD shapes.

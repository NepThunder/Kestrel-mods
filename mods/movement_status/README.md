# Movement Status

Your current movement state in the bottom-left corner: `Sprint [Held]` / `Sprint [Toggled]`, `Sneak [Held]` / `Sneak [Toggled]`, `Flying [Creative]` / `Flying [Elytra]`.

A behaviour port of Movement Status by UniDex101 (MIT, https://github.com/UniDex101/movement-status). It shares no code with the original: this is a fresh implementation against the Kestrel SDK, reading API 18's self state (`game.get-self-state`, capability `game-read`) — the district that exists because the local player's movement state and toggle options were not readable before it.

## What it does

Each tick the mod reads how the player is moving and draws the states that apply in the bottom-left corner, in the original's order — sprint, sneak, flying — several lines at once when several apply (sprint + sneak, and so on). The [Held] / [Toggled] distinction follows the game's own Toggle Sprint and Toggle Crouch options, exactly as the original respects them: a state the option has latched reads [Toggled]; one without the option is the key being held. Gliding on an elytra reads `Flying [Elytra]`, distinct from creative flight.

Options > Mods > Movement Status holds the original's single setting: the semi-transparent background box behind the lines, on by default.

This is a status display only. It changes no movement, no keybinds, no gameplay — it reports what the game is already doing, as the original says of itself.

## Fidelity

Matches the original's registry description:

- Live movement status in the bottom-left corner, with the original's exact labels.
- Multiple states shown at once.
- The optional semi-transparent background box, controlled by the one setting.
- Vanilla's Toggle Sprint and Toggle Crouch settings respected.

Documented gaps — nothing substituted, nothing invented:

- **Held vs toggled while both are true.** The original checks the physical key, so with a toggle option on it can still say [Held] while the key is down. The SDK exposes the option and the movement state, not the physical key, so this port's label follows the option: with the option on, a sprint reads [Toggled] even while the key is also held. (The original carries the mirror-image caveat — it reads the vanilla default keys, so custom keybinds could show the wrong label; this port never looks at a key at all, so its labels follow the bound options.)
- **Forced crouch.** The self read's sneaking includes a crouch forced by a low ceiling; with no key down and no option on, that reads `Sneak [Held]`. The SDK does not separate the two, and the line is the original's nearest label for the state.
- **No command** — the original has none, so the port adds none.

## Requirements

Needs a Kestrel build with the SDK self reads (API level 18): `game.get-self-state` is the only function this mod imports beyond the corner shapes and the settings page.

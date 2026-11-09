# Toggle Sprint

Sprint and sneak toggle instead of holding the key down, with the classic status HUD: `[Sprinting (Toggled)]`, `[Sprinting (Key Held)]`, `[Sneaking (Toggled)]`, `[Sneaking (Key Held)]`, `[Flying]`, `[Gliding (Elytra)]`, `[Riding]`.

A behaviour port of Toggle Sprint by CayMac (MIT, https://github.com/CayMac97/toggle-sprint), itself a port of Better Sprinting and the Lunar/Badlion ToggleSprint HUD. It shares no code with the original: this is a fresh implementation against the Kestrel SDK, built on API 18's movement-toggle write (`game.set-movement-toggles`, capability `game-read`) — the district that exists because a settings-grade movement write was not possible before it.

## How the toggle works here

The original implements its own latching sprint/sneak. This game already has the same behaviour as an option — vanilla's Toggle Sprint and Toggle Crouch, which latch the sprint and sneak keys — so the port drives the option itself, through the SDK's settings-grade write:

- **V** toggles the sprint option, **C** toggles the sneak option (the original's keys; both rebindable on the mod's page in Options > Mods).
- Options > Mods > Toggle Sprint flips the same two options from its settings page.
- The write is saved to options.txt, exactly as a change on the game's own options page is.
- A toggled sneak pauses inside inventory and chest screens so Shift-clicking works normally — that is the game's own toggle machinery handling screens, the same end the original implements by hand.
- Nothing forces movement: the write changes the option, never the player's state, and turning an option off does not un-latch a sprint already latched until the key is next pressed — again, the options page's own behaviour.

The HUD sits in the bottom-left corner and shows one line per state that applies, in the original's order.

## Fidelity

Matches the original's registry description:

- Toggle continuous sprint and toggle sneak without holding keys down, on the original's V and C keys.
- Toggled sneak pausing inside inventory and chest screens.
- The classic movement status HUD with the original's exact texts: `[Sprinting (Toggled)]`, `[Sprinting (Key Held)]`, `[Sneaking (Toggled)]`, `[Sneaking (Key Held)]`, `[Gliding (Elytra)]`, `[Riding]`.

Documented gaps — omitted, never faked:

- **Creative Fly Speed Boost.** The original's configurable 1.0x–4.0x multiplier has no SDK counterpart: there is no fly-speed write. The multiplier setting is omitted, and the flying line reads plain `[Flying]` instead of `[Flying (2.0x Boost)]` — the suffix would claim a boost that is not happening. Both return when the SDK gains a fly-speed write.
- **[Swimming].** The self read carries no in-water state, so the swimming line cannot be told apart and is not shown.
- **No command** — the original has none, so the port adds none.

## Requirements

Needs a Kestrel build with the SDK self reads and the movement-toggle write (API level 18).

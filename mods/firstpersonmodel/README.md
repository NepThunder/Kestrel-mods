# First-person Model

See your own third-person body in first person: armour, cape, elytra, held items and all, the head hidden, with offsets for every pose and the original's hands modes.

A behaviour port of [First-person Model](https://github.com/tr7zw/FirstPersonModel) by tr7zw, MIT, in the shape of its Horizon port by DeportedToHell (MIT). It shares no code with either: the originals are a Java mod and a native `.hnmod`, this is a fresh implementation against the Kestrel SDK (`entity-pose`, API 20, under the `render` capability).

## What matches the original

- **Your body in first person**, head and hat hidden by the host, with your armour, cape, elytra, held items, parrots, arrows and fire — everything the game's own first-person body path draws.
- **Drawn behind the camera**: 0.25 blocks back along the body's yaw standing, the Horizon port's number.
- **Offsets for every pose**: standing, sneaking, sitting (boats, minecarts, mounts), crawling and swimming. The standing, sneaking and sitting offsets can be changed (0.00 to 0.50 in 0.05 steps, on the settings page); swimming and crawling follow the head — the body pitches with your look and rides up or down with your pitch.
- **Steps aside** while sleeping, gliding with an elytra, half swimming, and scoping with a spyglass.
- **Vanilla Hands Mode** — Off, Always, Always Both Hands, Item In Hand: the model's arms hide so the vanilla hands show instead (Always the main arm, Both both arms, Item In Hand the arm of each hand holding an item).
- **Dynamic Mode**: looking down shows the body's arms and lowers the vanilla hands (through the hand district's offset, API 16).
- **Disable Vanilla Hands while Swimming**: swimming suspends the hands modes, the body's arms show.
- **Show Layers**: jacket, sleeves, pants and cape can each be hidden.
- **F6** turns it on and off (rebound in Options > Controls).
- **Settings** are kept between sessions.

## What's missing, and why

- **The four-tab config screen** (Settings / Auto Vanilla Hands / Disable Items / Show Layers, with sliders, search and item icons). The SDK's settings surface is one page of buttons in Options > Mods, so the port's settings live there; the tabs, sliders, tooltips and the English/Russian texts go with the screen.
- **Auto Vanilla Hands and Disable Items** — the per-item lists (items that switch to vanilla hands while held, items that turn the mod off while held). A list of items cannot be expressed on the settings page; both features wait on a settings surface with item lists, not on anything about poses. The mod therefore never turns itself off for an item.
- **The rest of Show Layers**: armour, held items, arrows, ears, head items, wings, parrots, Riptide and bee stingers — and the **Render Arrows & Stings** switch. Those are separate draws, not named parts of the posed model, so pose offsets cannot hide them. The four toggles above are the layers that are parts.
- **Hiding the vanilla hands themselves.** In the original, Off means only the body — the vanilla first-person hands are switched off. The SDK exposes no hand-hide: the vanilla hands still draw over the body's arms in the Off modes, and only Dynamic Mode's lowering moves them (down, not away). Always Both Hands likewise cannot make an *empty* vanilla off hand draw; it hides both model arms, and the off hand's vanilla arm appears when it holds something.
- **The Riptide step-aside.** The original also steps aside for a Riptide spin; the pose state exposes no spin signal to read it from.
- **The fishing line** leaving the body's hand: a render detail of the game's own line drawing, not something a pose can move.
- **`config/firstperson.json`.** The original keeps the original's config file in its format; this port keeps its choices in the mod's own SDK storage instead.
- **Handedness.** The pose state does not say which hand is your main hand, so the hands modes assume the vanilla default (main hand on the right). On a left-handed player the modes hide the wrong arm.
- Defaults the sources do not state are the port's: sneaking 0.35 and sitting 0.30 (the description numbers only standing's 0.25), hands mode Off, Dynamic Mode off.

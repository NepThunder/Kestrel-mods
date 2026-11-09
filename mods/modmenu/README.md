# Mod Menu

A Mods button on the title screen and in the pause menu, opening the list of your mods.

A behaviour port of [Mod Menu](https://github.com/TerraformersMC/ModMenu) by Prospector and TerraformersMC, MIT, via DeportedToHell's Horizon port. It shares no code with the original: this is a fresh implementation against the Kestrel SDK (`screen` for the buttons and their presses, `ui` for the mods screen, `input` for the open key, `storage` for the switches). **This port is partial, by the platform's limits** — see below.

## What matches the original

- **The Mods buttons**: one in the title screen's row of buttons and one in the pause menu's column, labelled "Mods", placed by the game where its own extra buttons go (the API 22 screen surface places a mod's buttons for it; the original's icon-only button becomes a labelled one, since a sandboxed button cannot carry the original's texture).
- **The Open Mod Menu key**: registered like the original's, starting unbound — bind it in Options > Mods > Mod Menu. Registered keys fire while playing, so the key opens the screen in a world; the buttons carry the title and pause screens.
- **The mods screen itself**, in the game's stacked layout every `ui` screen uses (not Mod Menu's two-pane layout): picking the one listed row opens that mod's details page — name, summary, credits, licence — with a Back button, as a click on a row does in the original.
- **The buttons' switches**: Options > Mods > Mod Menu can show or hide each button — the part of the original's options that governs what this port places.

## What's missing, and why

- **The list's data.** The screen lists exactly one mod: Mod Menu itself. No SDK interface lets a sandboxed mod enumerate the installed mods — the runtime's mod entries serve the game's own Mods screens and are not mod-facing — so the other mods' names, versions, summaries, icons and badges cannot be read, and the screen says so on its face rather than inventing a list. The small mod-list read that unlocks this is named in the API 22 design note but deliberately not bundled into it; the full list arrives with it.
- Everything that hangs off that data goes with it: the **mod count** after the version on the title screen, the **update indicator** and its flashing, **search**, **sorting**, the **library filter**, the other mods' **details, credits and links**, **Configure** buttons and **Open Mods Folder**.
- **The rest of Mod Menu's 21 options**: they configure the list's presentation (sorting, count position, library handling, button styles for placements this port doesn't have); with no list to present they would be switches that do nothing, so only the two button switches ship.

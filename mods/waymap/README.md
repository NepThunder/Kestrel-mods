# Waymap

Waypoints with a rotating minimap: the map north follows you.

A behaviour port of [Waymap](https://github.com/Lauch2/horizon-waymap-mod) by Aron (Lauch2), MIT. It shares no code with the original: the original is a native mod for Horizon, this is a fresh implementation against the Kestrel SDK (`map` for the game's tiles, the minimap image and the death events; `render` for the in-world markers; `hud` for the panel; `input` for the keys; `ui` for the list and the editor; `storage` for the waypoints and settings; `game-read` for the player).

## What matches the original

- **Per-world store.** Waypoints are kept per world or server (the port keys its storage by the world or server name the game reports, exactly the original's context), up to 512, each with its own dimension and colour, a hidden flag, and the death flag, in the mod's storage.
- **The rotating minimap.** The terrain round you at one block per pixel, turning with you — the dots rotate too, which the original added in its 0.2.0 — with waypoints as dots in their colours and a death as a red X. The terrain is the game's own map tiles (API 25), shaded with the map item's rule, held in memory only: the original keeps no map, and neither does the port.
- **Markers in the world** — the waypoint's letter (its initial; a red X for a death) over its place — and with labels on, the name and the distance under it.
- **The HUD panel** in the bottom-left: the nearest waypoints with their distances.
- **The list**, filtered by dimension and by deaths (All / Overworld / Nether / End / Deaths), with hide/show, edit and delete per waypoint.
- **The editor**: position with steppers and a Use My Position button, dimension, sixteen colours, visibility.
- **Death waypoints**: left where you died, named Death 1, Death 2, ... in order. At the 512 cap a new death makes room by removing the oldest death waypoint (the port's documented choice where the original is silent).
- **Keys**: Create on **N**, the list on **B** or **M** — registered with the platform, so they rebind on the mod's page in Options > Mods, where the three settings also live, with the original's names: Markers, Labels, HUD Panel.
- **`/wp` commands**: add, del, list, rename, color (a colour name or a `#rrggbb`, stored as the ARGB the original keeps), clear, edit (shows a waypoint's information), hide, show, help.

## What's missing, or different, and why

- **Minimap size and position settings.** The original names none in its registry description; the port fixes the minimap at 100 texels in the top-right corner rather than inventing settings.
- **The original's own list and editor screens.** The platform gives mods standard screens, so navigation is by row clicks and buttons instead of the original's keyboard-driven list, and the key that opens the list cannot also close it (key presses do not reach a mod while its own screen is open; Esc closes). This is the platform's screen pattern, the same one the container-screen ports took.
- **Renaming in the editor.** The widget surface has no text field, so the editor shows the name and `/wp rename` changes it.
- **The original's file format and folder.** A sandboxed mod's only store is its key-value storage; the waypoints live there in the same per-world shape (name, whole-number position, dimension, ARGB colour, flags, the death counter) instead of in `<game dir>/waymap/<context>.json`. Nothing of the original's data is lost in the move; the file itself is.
- **Atomic writes.** The original writes its file atomically; the storage surface has no rename, so a save is one whole-blob write, as near atomic as the surface allows.

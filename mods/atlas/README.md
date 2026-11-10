# Atlas

A clone of Xaero's Minimap and World Map: a sharp minimap, a world map of everywhere you've been, and waypoints.

A behaviour port of [Atlas](https://github.com/MrCloudy2/horizon-atlas) by MrCloudy2, MIT. It shares no code with the original: the original is a native mod for Horizon, this is a fresh implementation against the Kestrel SDK (`map` for the game's tiles, the map screen, the minimap image and the death events; `entity` for the minimap's mob dots; `render` for the in-world labels; `input` for the keys; `ui` for the screens; `storage` for the maps, waypoints and settings; `command`, `chat`, `game-read` and `world` for the sends and the biome readout).

## What matches the original

- **One map, kept per world, server and dimension.** The minimap and the world map share one accumulated map, as the original's do. The accumulation is the game's own map tiles (API 25's `map.tiles`), shaded with the map item's own rule — vanilla's colours, brighter facing a lower north neighbour, water by depth, cave walls brightest — and darkened by the time of day on the surface, as the original's 0.2.0 shading pass describes. Map modes: Auto (the surface in the open, the cave around you under a roof), Cave and Layer.
- **Minimap (own composition).** Turns with you (rotation can be switched off), with entity dots that fade with height and waypoint dots; deathpoints draw as a red X. Size, corner, shape (square/round) and zoom are settings.
- **World map (M).** The host pans and zooms the mod's composed picture; a left click starts a waypoint there, a right click reads the coordinates and the biome under the pointer. Waypoints of the active set are the map's marks.
- **Waypoints (U, B, H).** Sets (the list's Set button switches, and one stop shows all), the list with distances, New at your position (B) or at a map click, Teleport (a `/tp` run as you, for operators — the original's own send), Share (a chat message with the waypoint — the original's other send), Delete, and labels in the world with name and distance (H toggles them). Deathpoints appear where you died; temporary waypoints delete themselves when you reach them.
- **Overlays.** Chunk grid and slime chunks on the world map. Slime chunks are computed from the seed you give `/atlas seed`, exactly as the original takes it (even the native mod cannot read a server's seed).
- **Settings (Y, and Options > Mods > Atlas).** The settings above plus entity dots and deathpoints switches; `/atlas set <setting> <value>` sets exact values, as the original's `/atlas` does. Waypoints are renamed with `/atlas rename <old> | <new>`, and a waypoint another player shares in chat is added with `/atlas addshared`.

## What's missing, and why

- **Mob and player faces, and item icons, on the minimap.** Hold-Tab faces need skin textures; no SDK read exposes a skin, a face, or a dropped item (the entity read lists living mobs only), so dots stand in for all three and the port does not pretend otherwise. Other players are dots in the original; here they cannot even be dotted — the SDK's pose states carry no positions, and the entity read does not list players — so multiplayer minimaps show mobs (single-player) and waypoints, not players.
- **The light overlay, and block light at night.** The game's tiles carry colour, height and water depth — no light. A bulk light read exists but is capped far below map scale, so lit blocks do not stay lit on this port's night map; the day/night darkening is a daylight factor over the whole surface instead.
- **Cave layers at heights you never visited.** The host scans the one cave layer at your head; the port's cave and Layer modes show the visited cut nearest your head (or the Layer Y setting), and a stop you never stood at has no tiles. Layered views over visited heights are exact.
- **The map's size in storage.** The original writes map files freely; a sandboxed mod's storage is capped (4 MiB, 64 keys). Explored ground is kept in 8×8-chunk regions under that cap; past it, the farthest regions are evicted first. Most worlds never reach it.
- **Text fields.** The widget surface has no text input, so waypoint names are made by the editor's default ("Waypoint N") and changed with `/atlas rename`, rather than typed into the editor as in the original.
- **Settings search and pages.** The original's searchable settings pages land on the platform's one standard settings page — the settings themselves are all here, the search box is not.

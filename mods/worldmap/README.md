# World Map

A full-screen, self-writing world map with a minimap, waypoints (groups, import and export), day and night, cave layers, and teleport.

A behaviour port of [World Map](https://github.com/Px228-Da-Da/horizon-worldmap) by Px228-Da-Da, MIT. It shares no code with the original: the original is a native mod for Horizon, this is a fresh implementation against the Kestrel SDK (`map` for the game's tiles, the map screen, the minimap image and the death events; `render` for the in-world markers; `input` for the keys; `ui` for the screens; `storage` for the maps, waypoints and settings; `command`, `chat`, `game-read` and `world` for the sends and the biome readout).

## What matches the original

- **Self-writing map.** The terrain you have seen stays on the map after you leave, per world, dimension and cave layer, in the vanilla map colours — here, an accumulation of the game's own map tiles (API 25's `map.tiles`) in the mod's storage, shaded with the map item's own rule. Open it with **M**; the host fields the drag and the zoom toward the pointer, and clicks are the mod's: a left click reads the coordinates and the biome under the pointer, a right click opens the menu.
- **Minimap.** In a corner (setting), turning with you or north-up (setting), with the compass letters on its rim, your waypoints as dots, and the floor at your head height in caves. **N** toggles it, **Z** changes its scale.
- **Cave layers.** Under a roof, in a cave or in the Nether the map shows the floor round you, layered (the cut at your head) or full (the cut at the Top Y setting), over the heights you have visited.
- **Day and night.** The surface darkens at night by the time of day.
- **Waypoints.** The right-click menu: add here, rename by `/wp rename`, 16 colours, groups (created in the menu's editor flow or with `/wp group`), hide per waypoint or per group from the list, teleport (a `/tp` run as you), share, delete; markers in the world with name and distance; a waypoint where you die. Groups hide and show from the list's group buttons, as the original's list headings do.
- **Share.** Through chat (a green [Add] in the original; here the line is read by other clients running the port, who are told `/wp addshared` adds it), or `/wp share <number> <player>` as a private message, `/wp share <number> all` to the whole chat.
- **Import and export of waypoint text** — also Xaero's format on import, and the export writes that shape. Export prints the lines in chat and keeps the full text in the mod's storage.
- **`/wp` commands:** add, del, rename, list, clear, share — plus goto (centre the map) and group.
- Keys are registered with the platform, so they rebind on the mod's page in Options > Mods, as the original's do; the rest of the settings live there too.

## What's missing, and why

- **Cave layers at an arbitrary Top Y.** The host scans the one cave layer at your head; the port's layers are the heights you have visited, and a Top Y stop you never stood at has no tiles. Layered and full modes over visited heights are exact.
- **Glowing blocks stay lit, and smooth zoom.** The game's tiles carry colour, height and water depth — no light — so lit blocks do not stay lit on the night map; the darkening is a daylight factor instead. And the map screen's pan and zoom are the host's over the mod's still picture (the design's bargain): no glide physics, and a zoomed view is the host's sampling of the composed texels until the view settles and the mod recomposes — the original's own "drawn from its texels" at a distance, without its glide.
- **PNG export and waypoint files.** No surface writes a file for the user: storage is the mod's own key-value store. Export therefore prints its text in chat (and keeps it in storage) instead of writing `exports/waypoints.txt`; import takes one line per `/wp import` call instead of reading a file.
- **Other players on the minimap.** No SDK read gives another player's position (the pose states carry no positions, and the entity read lists mobs, single-player only), so the minimap dots are waypoints only.
- **The list's search, and text fields generally.** The widget surface has no text input: the waypoint list filters by group instead of searching, and names are made by the editor's default and changed with `/wp rename` rather than typed into a field.
- **Waypoint shapes.** The host's map marks are squares with a letter; the in-world marker is the name in its colour. The original's shape choice does not survive the surface.
- **The map's size in storage.** The original writes map files freely; a sandboxed mod's storage is capped (4 MiB, 64 keys). Explored ground is kept in 8×8-chunk regions under that cap; past it, the farthest regions are evicted first.
- **The Russian interface.** The port's strings are English; the original's second language pack is not carried.

# RMotd

The MOTD list, its commands and its 10-second rotation — but not the server-list MOTD itself, which no client SDK surface can write (see Gaps).

A behaviour port of RMotd by adasjusk (CC-BY-SA-4.0), following the description of the Horizon registry port by Vakarux12. It shares no code with either: the originals are Java, and this is a fresh implementation against the Kestrel SDK (capabilities `storage`, `chat`).

## What the original does

- Rotates the MOTD of a world opened to LAN every 10 seconds, through the list in `config/rmotd.json`; the server list ping shows the rotation.
- `/randommotd` turns rotation on and off.
- `/motd create <name> <motd>` adds an MOTD; `/motd reload` reads the file again (operator level 2).
- MOTDs take `&` colour codes.

## What this port does

- Keeps the list: one MOTD per line in the file `motds` in the mod's own storage folder (the counterpart of `config/rmotd.json`).
- `/randommotd` toggles rotation (on by default, as in the original); `/motd create <name> <motd>` appends an MOTD to the list and saves it; `/motd reload` re-reads the file; bare `/motd` reports the current entry and the list size.
- Rotation runs on the original's 10-second beat, and the current MOTD is shown in your chat each time it changes.

## Gaps — features the SDK cannot express yet

- **The MOTD is never set.** The original's whole point is writing the LAN server's MOTD; the Horizon port does it by writing a private field of the LAN server. The client SDK exposes neither the LAN server nor any MOTD, so this port's rotation is visible only in your own chat. There is no server-list effect at all.
- **No operator gate.** `/motd reload` (and `create`) are operator-level-2 in the original; a client mod's own commands carry no permission levels, so anyone running the mod can use them. They only ever touch the mod's own list.
- **Colour codes print literally.** MOTDs are stored verbatim, `&` codes included, but the chat lines a mod shows are plain text, so a coded MOTD reads with its codes visible in this port's chat announcements.
- **Entry names are not kept.** The original's JSON labels each MOTD with a name; this port's list is lines of a file, so `create` accepts the name (the command's shape matches) but stores only the MOTD text.

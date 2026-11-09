# Flesh to Leather — Kestrel catalog staging (Tier C1 pilot)

A **data-only content pack** for the Kestrel catalog: one smelting recipe,
rotten flesh → leather (200 ticks, 0.35 XP), plus its recipe-unlock lang
entry. No code, no new blocks or items — vanilla items only.

## Credit

- **Original mod:** Flesh to Leather 1.0.0 by **CGRLabs**, MIT licence.
- **Upstream source:** https://github.com/CGRLabs/flesh-to-leather
- **Upstream registry entry:** `horizon-rust/horizon-mods`,
  `mods/flesh_to_leather.toml` (published 2026-11-09, game 26.3).

## What this staging is

A **repack, not a rebuild**. `.hmod` and `.kmod` are the same zip format —
the runtime's `Package::read` never looks at the file name — so the package
here is **byte-identical** to the upstream release asset, renamed:

| | |
| --- | --- |
| Upstream file | `flesh_to_leather-1.0.0.hmod` (horizon-rust/horizon-mods release `flesh_to_leather-1.0.0`) |
| Staged file | `flesh_to_leather.kmod` (same bytes) |
| Size | 812 bytes |
| SHA-256 | `2caac39306a051b53ae489137fa2a3a204fc8f156860fcbc4568275392a7e5c6` — matches the registry entry's pin and the upstream index |

Package contents (3 files):

- `horizon-mod.toml` — manifest: api `0.2`, side `both`, capabilities `["registries"]`
- `data/flesh_to_leather/recipe/zombie_flesh_to_leather.json` — the smelting recipe
- `assets/flesh_to_leather/lang/en_us.json` — recipe name

Validated with this tree's own checker (`cargo horizon-mod check`, which
runs `horizon_mods::index::inspect` = `Package::read` + `Content::read` —
the exact code the launcher runs at install time): accepted as a
content-only mod. The same bytes under the `.hmod` name are refused only
because the checker cannot infer the kind from that extension.

**Signing:** the upstream entry is minisign-signed by its author. Kestrel's
catalog is SHA-256-pinned and unsigned for every existing entry, so this
repack publishes **unsigned**, like the rest of the catalog. The hash above
is the trust anchor.

**Source manifest:** `mods/flesh_to_leather/horizon-mod.toml` is a verbatim
transcription of the manifest embedded in the package (byte-identical,
208 B), which is what `tools/build_index.py` cross-checks the release asset
against (id, version, api, side, capabilities). The builder takes the
licence from `Cargo.toml` when present and defaults to MIT otherwise —
correct here, as the original is MIT. Ship note: `flesh_to_leather` must be
added to `ALLOW_IDS` in `tools/build_index.py`.

## Behaviour players will see

- **Restart:** content packs load at game start, not when toggled. Enable
  the pack in Options → Mods, then **restart the game** for the recipe to
  appear. Disabling likewise takes effect on the next restart.
- **Server parity:** the pack is `side = "both"` — registry content follows
  the server. On a multiplayer server or LAN world, **the server and every
  joining player need the same pack**; Kestrel peers exchange a
  content-registry hash when connecting and a mismatch disconnects with
  "Content registry mismatch". A LAN host's guests need the pack too.
  Vanilla and plugin servers are unaffected (a content-free game sends no
  identity).
- **Removal is safe for this pack:** it adds no blocks or items, so a world
  keeps nothing that depends on it — leather already smelted stays.

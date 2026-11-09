# Eating Animation

While you eat food, or drink a potion or milk, the item in your hand shows its bitten frames as you go.

A behaviour port of [Eating Animation](https://github.com/Theoness1/EatingAnimation) by Theoness1, MIT, via soufied's Horizon port. It shares no code with the original: the original is a Fabric mod whose frames are its own drawn textures; this is a fresh implementation against the Kestrel SDK (API 23's `item-model` surface under `render`, plus this package's own content under `registries`).

## How it ships — one self-contained .kmod

The original's Horizon port is one package with its frames compiled in as a content mod, and so is this port. The game's package format carries a component and content files together: `cargo horizon-mod build` packs the crate's component AND the folder's `assets/`/`data/` files into one `.kmod` (`Package::write_files` takes both), the manifest rule is only that content needs `registries`, which needs api "0.2" — "0.1 plus content" — and at load the game layers the content of every enabled package declaring `registries` into the resource root, whether or not the package also carries code. The item renderer reads its definitions from that same root (`assets/<ns>/items/`), so this package's `assets/eatinganimation/items/*.json` resolve as the `eatinganimation:<item>` definitions its rules name. Nothing else to install, no resource pack needed — install this one mod and restart, so its content joins the resources.

## What matches the original

- The behaviour: while a covered item is being eaten or drunk, the item in the hand steps through bitten frames as the use goes on; at rest it is the item itself. First and third person both (another player eating animates on your screen too).
- The covered set: every vanilla food, the potion (one item — its variants are components, as the design note says), the milk bucket, the honey bottle and the ominous bottle. 43 items, one rule each.
- The pacing: frames step at the same fractions (35% / 60% / 82%) of each item's own use time — dried kelp's quick 16 ticks, the honey bottle's slow 40, everything else's 32 — because each item's definition dispatches on the resolver's `use_duration` with that item's own thresholds. No per-frame mod code runs; there are no settings, as the original has none.

## What's different, and why

- **The frame art is this port's own.** The original's frames are hand-drawn bitten textures (its author's art, not copied here, and no texture files ship in this package at all). Each frame here is a flat cuboid model sampling a cropped rectangle of the item's *own vanilla texture*: foods lose a growing bite from the top-right corner (5×5, 8×8, 11×11, 13×13 pixels over the four frames), drinks empty from the top down. The progression and timing match; the pictures are plainer than the original's drawings. `generate_frames.py` in this folder authors the whole set from the item table, and the unit tests pin the table, thresholds and files together.
- The potion's frames keep vanilla's two layers (bottle, then the liquid tinted by the stack's potion contents, as the vanilla potion definition tints it) — the only two-layer item in the set.
- The GUI (inventory) display never swaps: the rules name the hand contexts only, as the original's frames are a hand display.

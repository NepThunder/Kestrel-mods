# Context Tool Swap

Start breaking a block, and the right tool for it is already in your hand.

A behaviour port of [Context Tool Swap](https://github.com/TTCjonasTTC/context_toolswap) (MIT) by TTCjonasTTC and Qwen, following its registry description and its repository README. This is a fresh implementation against the Kestrel SDK (the `actions` capability, API 21, and `inventory-actions`, API 24): on the hotbar the swap is a slot change, from the bag it is one swap click, and everything after either is the game's ordinary mining.

## What matches the original

- **Both of the original's triggers.** The registry entry's: press middle click (the port's one registered key, "Swap to the right tool", rebindable) while you have a tool in hand. And the repository README's: simply start breaking a block — the swap happens at the start of the break, or when you switch to a different block mid-break, never half-way through breaking one.
- **The same choice of tool**: the one correct for the block's drops when the block requires a tool — an iron pickaxe for diamond ore, shears for leaves and wool — otherwise the fastest, with tools ranked by their material speeds (wood 2, stone 4, copper 5, iron 6, diamond 8, netherite 9, gold 12). Candidates must reach a mining speed of 2.0 on the block, which is why swords and the bare hand are never selected. Ties keep the slot you're already on.
- **The whole inventory is searched**, hotbar and bag alike, as the original searches it. A tool that is only in your bag is brought into your hand with one swap click (the `inventory-actions` capability, API 24): the tool's stack trades places with the stack in your selected slot, exactly the player's own number-key swap over a slot.
- **The same skips**: unbreakable blocks, instant-break blocks (they simply have no tool to swap to), and tools that are already broken.
- **`/toolswap`** turns the mod on and off — bare `/toolswap` reports the state ("Tool Swap: on"), `on`, `off` and `toggle` change it. On by default, as in the original.

This is still a *partial* port — the established pattern in this catalog (rmotd and pvp_music shipped partial the same way) — but by one rule only, the creative and spectator skip below. The bag half that this section used to lead with is missing no more: it was the inventory-actions district's (B6) verb, and 0.2.0 completes the port with it now that the SDK has one (API 24). What is still missing, and why:

## What's missing, and why

- **Creative and spectator skip.** The original does not swap there. The client SDK shows a mod no game-mode read (it is only used inside the host, to gate `recipes.give`), so the port cannot tell and swaps in creative too.
- **The middle click also does vanilla pick-block.** A registered key does not consume the button, so the click keeps its vanilla meaning as well: in survival it may first select the targeted block's slot, and the swap lands a tick later and wins; in creative it can also put the block in your hand. The break-start trigger has no such shadow.
- **The tool table is the port's own rendering of vanilla's data.** The original asks the game which tool class mines a block, what its drops require, and each tool's speed; the SDK exposes none of that, so the port carries the same facts as a table keyed by block family. A block the table doesn't know gets no swap — the safe failure — rather than a wrong one.
- **State is per-mod, not per-world.** The original keeps its on/off per world; the SDK's storage can't key by world, so the port keeps one switch for everywhere.

## Files

`src/lib.rs` — everything: the two triggers, the tool table, the choice. The decisions are pinned by unit tests in the same file; `tests/probe.toml` covers the command and calm ticking under the harness.

# Crosshair Indicator

A port of [Crosshair Indicator](https://github.com/CayMac97/crosshair-indicator)
by CayMac (CayMac97), MIT — itself a port of the Java Crosshair Indicator mod
by AmerDz01 — for Kestrel. Your crosshair tells you when the mob or player
you aim at is within attack reach (3 blocks).

## What was ported

- The indicator itself: it appears only while the entity under your crosshair
  is within 3 blocks, and disappears when you look away, look past 3 blocks,
  or a wall stands between you and it. It works in singleplayer and
  multiplayer, and on players as well as mobs.
- All four styles: corner brackets around the crosshair (default), a centre
  dot, diamond pips, and a chevron above the crosshair. The indicator is
  drawn with the SDK's HUD shapes around the screen's centre, so it sits on
  the crosshair itself.
- The static colour themes: classic white (default), combat red, pink,
  diamond cyan, gold.
- The on/off switch three ways: `/crosshair`, a key on the mod's page in
  Options > Mods (unbound by default — bind it there), and the switch on
  that page. The page also cycles the style and the theme; the game keeps
  the choices.

## What is missing

The SDK does not expose these, so this port has neither:

- The thin weapon-cooldown bar below the crosshair, and the dynamic theme's
  charge colours (green when the weapon is fully charged, orange while it
  charges). Both need the weapon's attack strength / cooldown, which the SDK
  does not expose to a sandboxed mod. The themes here are static colours
  only.

## How to use it

Import `crosshair_indicator.kmod` in Kestrel and enable it. Aim at a mob or
player within 3 blocks: the brackets (or your chosen style) appear around
the crosshair in your chosen colour. `/crosshair` hides it; `/crosshair`
again shows it.

Needs SDK API level 10 (`entity.targeted` for the entity under the
crosshair; the HUD shapes are level 0). Runs on Kestrel build 9+.

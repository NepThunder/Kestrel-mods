# FastTrading

A behaviour port of FastTrading by bendy1234 (MIT; Horizon port by
sparticle62ops) for Kestrel. A **Speed Trade** button on the merchant screen
repeats the selected trade as fast as the speed setting allows, until the
trade locks, the payments run out, the result has nowhere to go, or you
press the button again.

Each round picks the trade for you — the game stages the payments and the
result, exactly as your own click on the trade list would — and takes the
result out of the merchant's result slot with one quick-move click, which
keeps your hand empty for the next round. The repeat speed is a setting in
Options > Mods > FastTrading, and `/fasttrading` opens the same controls
in game.

## Fidelity

Matches the original: the speed-trade button on the merchant screen, the
repeat loop and its stop conditions, the speed setting, and the command
that opens the controls. The registry entry names no numbers for the
speed, so the setting steps over idle ticks between rounds — Fastest
(the default) adds none — and the port says so rather than inventing a
number in the original's name.

One gap, a limit of the surface: the loop runs on the game's ticks, not
on frames, so the fastest cadence is one round per two ticks — pick on
one tick, take on the next — where the original could repeat within a
frame.

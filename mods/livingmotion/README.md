# Living Motion

Third-person players feel alive, on top of the vanilla animations: lean into runs, bank into turns, stride bounce, landing squash, and more, each player on their own springs.

A behaviour port of [LivingMotion](https://github.com/bartekchomik26-dotcom/Living-motion) by bart, MIT. It shares no code with the original: the original is a native `.hnmod` for Horizon, this is a fresh implementation against the Kestrel SDK (`entity-pose`, API 20, under the `render` capability). The original's own animations are original to it — no Punchy, Punchy Moves or Not Enough Animations code or assets — and likewise this port authors its curves here.

## What matches the original

- **Springs over vanilla.** Each drawn player gets a few springs, stepped once a frame from how they move, and the results are added to the finished vanilla pose. The vanilla animation is never replaced or scaled — the port layers on it exactly as the original does.
- **Lean into runs** — forward with speed, more at a sprint, less sneaking, overshooting when you set off and rocking back when you stop.
- **Bank into turns** — tilt toward the inside of a turn, more the faster you go.
- **Stride bounce** — the body rises as the legs pass each other and dips at full stride.
- **Landing squash** — deeper after a longer fall, springing back with a bounce; a quick stretch on take-off.
- **Jump pose** — arms out to the sides (and up while rising), one knee raised.
- **Arm swing** — a slightly fuller swing, more at a sprint, trailing behind the lean.
- **Idle breathing** — a slow arm sway while standing.
- **Steady head** — the head stays level while the body leans and banks (a setting, on by default).
- **Elytra flight** — roll into turns like a glider, arms spread like wings and swept back into a tuck as you dive, the outside arm of a turn reaching further, legs apart with a slight flutter; landing from a glide squashes too.
- **Elytra twirls** — `,` and `.` do a full barrel roll left or right while gliding.
- **Swimming** — the body rolls with each stroke and into turns, with a flutter kick that follows your speed.
- **Item poses** — eating brings the food to the mouth with a chew, drinking tips the bottle back, a map is held in both hands with the head down, a compass or clock is held up to read, a torch or lantern is held out.
- **Swings that follow through** — the shoulders turn into an attack and the head stays on target; mining raises the arm between strokes (the vanilla swing itself) and leans the body in.
- **Inspect** — I brings the held item up and turns it over.
- **Fades** — riding and sleeping fade back to vanilla; attacks and item uses keep their vanilla arm (the other effects never fight the swinging or using arm).
- **Settings**, in Options > Mods > Living Motion: on/off, other players on/off, each effect (Lean, Bank, Stride Bounce, Landing Squash, Jump Pose, Arm Swing, Idle Breathing, Elytra Flight, Swimming, Item Poses, Swing Follow-through) from 0% to 200%, stepping by 25%, steady head, and reset. Y toggles the mod, I inspects, `,`/`.` twirl. Choices are kept between sessions.

## What's missing, and why

- **Elytra trails.** The original's contrails (7 presets, 13 colours, 18 striped flags, width, duration, minimum speed, glow, fade — ported there from Elytra Contrails, MPL-2.0) are fading translucent geometry that lives for seconds. The SDK's render surface draws HUD shapes and depth-tested world lines/boxes; it has no particle or fading-trail surface, so the trails are left out whole rather than imitated with lines. They arrive with the particles district.
- **Climbing.** Hand over hand up ladders and vines needs a climbing signal; the pushed pose state has none (no on-ladder field), so there is nothing honest to hang the pose on.
- **The strafe half of banking, and exact turn rate.** The pose state carries the head's yaw relative to the body, not the body's own velocity or heading, so banking reads the turn from the head-yaw velocity — you look into a turn and the body follows, which is the signal available — and cannot separate a strafe from a turn.
- **Fall depth and the jump's rise.** No vertical velocity is pushed, so airtime stands in for both: a longer fall (more airtime) squashes deeper, and the jump pose's "up while rising" reads as the first moments of airtime.
- **Riptide and dying fades.** The original also fades for Riptide spins and death; neither is exposed in the pose state (riding and sleeping, which are, do fade).
- **The first-person hand half.** The original also animates the first-person hands from the same rig. That is the hand district's domain (API 16/17), not this one's, and the catalog's Punchy is that genre's port; this port is the third-person body only.
- **Handedness.** The pose state does not say which hand is a player's main hand, so item poses and Inspect assume the vanilla default (main hand on the right). A left-handed player's poses land on the wrong arm.
- **Key rebinding on the settings page.** The original changes its keys on its own page; a `.kmod`'s registered keys are rebound in Options > Controls instead. The page names the keys and their defaults.

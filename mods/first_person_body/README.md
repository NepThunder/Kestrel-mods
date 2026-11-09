# First Person Body

See your own body in first person. Look down and your torso, legs and feet are there, moving as you walk, crouch and fight, instead of the floating hand alone.

A behaviour port of [First Person Body](https://github.com/selfmonoraque5/first-person-body) by selfmonoraque5, MIT. It shares no code with the original: the original is a native `.hnmod` for Horizon, this is a fresh implementation against the Kestrel SDK (`entity-pose`, API 20, under the `render` capability). The original was inspired by tr7zw's First-person Model and contains none of its code or assets; likewise here.

## What matches the original

- **Your own model in first person**, with your skin, armour and cape.
- **Head, helmet and head items hidden** — the host never draws the head or hat on a mod-shown first-person body, so the camera never looks out of your skull.
- **The body sits slightly behind the camera**, a little more when you look down or crouch, so you never see inside your chest.
- **The vanilla first-person hand by default** (the model's arms hide under it), or the model's own arms.
- **Steps aside** while swimming, crawling, gliding with an elytra, and sleeping.
- **The settings, exactly the original's table**, in Options > Mods > First Person Body — each button steps through its values:
  - First Person Body: OFF, ON — default ON
  - Arms: Vanilla hand, Model arms — default Vanilla hand
  - Body offset: None, Small, Normal, Large — default Normal
  - While riding: OFF, ON — default ON
  - Lean back looking down: OFF, ON — default ON
  - Reset to defaults
- **A toggle key** that turns it on and off in game, starting **unbound** as the original's does — bind it in Options > Controls.
- Client side only: works in single player and on any server. Choices are kept between sessions.

## What's missing, and why

- **The Riptide and spectator step-asides.** The original also steps aside for a Riptide spin and for spectators. The pose state the SDK pushes exposes neither — no spin signal, no spectator flag — so those two fades cannot be read honestly, and the body stays shown during them.
- **Cancelling the vanilla hand.** With Model arms chosen, the original cancels the vanilla hand's draw so only the model's arms show. The SDK exposes no hand-hide, so here the vanilla hand still draws over the model's arms.
- **The offset distances.** The original names its steps (None, Small, Normal, Large) without numbering them; this port reads them as 0, 0.06, 0.12 and 0.20 blocks behind the camera, with up to 0.10 more looking straight down and 0.06 more crouching.

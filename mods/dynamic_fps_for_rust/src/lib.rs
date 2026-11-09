//! Dynamic FPS for Kestrel: after the original's minute
//! without input, the frame rate is capped at the original's
//! idle rate (20 FPS) until the next input, which lifts the
//! cap at once. The idle clock is the game's own
//! (`input.last-input-ms` — the same clock the game's
//! inactivity limiter reads) and the cap goes through the
//! game's own frame limiter (`render.set-fps-cap`, which can
//! only ever lower the rate), so the port's one expressible
//! tier is the original's exactly.
//!
//! Behaviour reimplemented from Dynamic FPS for Rust by Lyvt
//! (MIT, <https://github.com/Lyvt/dynamic-fps>), as described
//! by its Horizon registry entry; this file shares no code
//! with it. The original caps the frame rate by window state:
//! 30 FPS while another window hovers over the game, 10 FPS
//! unfocused (after a 0.5 s grace), 1 FPS minimised, 20 FPS
//! after a minute without input, and a reduced 30 FPS on
//! battery power; it can also lower the game's volume in the
//! background.
//!
//! Of those, only the idle tier has an SDK surface, and it is
//! implemented here in full. The window states are the
//! engine's own business — it consumes focus and
//! iconification inside its limiter and reports neither to a
//! mod — and on Android the surface stops when the app is
//! backgrounded, so no mod code runs there anyway. Battery
//! behaviour already lives in the same limiter (the game's own
//! battery cap applies ahead of any mod cap), and no SDK
//! surface reports the power source. The volume option would
//! need the master volume, which the audio surface does not
//! give a mod. Those gaps are stated in the README, not
//! approximated. The original keeps its values in a config
//! file; a mod cannot read the game's config folder, so the
//! values here are the original's defaults, fixed — and like
//! the original, there is no settings screen. This port's home
//! is <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/dynamic_fps_for_rust>.

use horizon_mod_sdk::{Mod, Registration, input, render};

/// The original's idle delay: a minute without input, in the
/// milliseconds `last-input-ms` reports.
const IDLE_MS: u64 = 60_000;

/// The original's idle cap.
const IDLE_CAP: u32 = 20;

/// The cap for a given time since the player's last input: the
/// original's idle tier past its delay, no cap before it.
fn cap_for_idle(idle_ms: u64) -> Option<u32> {
    if idle_ms >= IDLE_MS {
        Some(IDLE_CAP)
    } else {
        None
    }
}

/// The call to make, if any: `Some(new)` when the cap in effect
/// should change to `new` (a cap to set, or none to lift it),
/// `None` when it already matches — the limiter holds its
/// state, so only transitions are pushed.
fn cap_change(applied: Option<u32>, idle_ms: u64) -> Option<Option<u32>> {
    let want = cap_for_idle(idle_ms);
    if want != applied {
        Some(want)
    } else {
        None
    }
}

struct DynamicFps {
    /// The cap currently in effect (what this mod last set).
    cap: Option<u32>,
}

impl Mod for DynamicFps {
    fn init() -> Result<(Self, Registration), String> {
        Ok((DynamicFps { cap: None }, Registration::new().on_frame()))
    }

    fn frame(&mut self, _seconds: f32) {
        if let Some(new) = cap_change(self.cap, input::last_input_ms()) {
            render::set_fps_cap(new);
            self.cap = new;
        }
    }
}

horizon_mod_sdk::export_mod!(DynamicFps);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_begins_at_the_originals_minute() {
        assert_eq!(cap_for_idle(0), None);
        assert_eq!(cap_for_idle(IDLE_MS - 1), None);
        assert_eq!(cap_for_idle(IDLE_MS), Some(IDLE_CAP));
        assert_eq!(cap_for_idle(IDLE_MS * 10), Some(IDLE_CAP));
    }

    #[test]
    fn the_cap_applies_once_and_lifts_on_input() {
        // Crossing the delay sets the cap, once.
        assert_eq!(cap_change(None, IDLE_MS), Some(Some(IDLE_CAP)));
        assert_eq!(cap_change(Some(IDLE_CAP), IDLE_MS + 1), None);
        // Fresh input lifts it, once.
        assert_eq!(cap_change(Some(IDLE_CAP), 0), Some(None));
        assert_eq!(cap_change(None, 0), None);
        assert_eq!(cap_change(None, IDLE_MS - 1), None);
    }

    #[test]
    fn the_numbers_are_the_originals() {
        assert_eq!(IDLE_MS, 60_000);
        assert_eq!(IDLE_CAP, 20);
    }
}

//! Fullbright for Kestrel: maximum brightness (1000% gamma,
//! `render.set-gamma` 10.0, API level 13) toggled on G, with the
//! original's smooth fade, and a notice on the HUD saying which
//! way it went. `/fullbright_horizon` toggles it too, and the
//! mod's page in Options > Mods has the same switch; the game
//! keeps the choice.
//!
//! Behaviour reimplemented from Fullbright 1.0.0 by CayMac (MIT,
//! <https://github.com/CayMac97/fullbright>), the registry's
//! `fullbright_horizon`; this file shares no code with the
//! original — the original is a native mod, this is a fresh
//! implementation against the Kestrel SDK. The original binds G,
//! fades over 250 ms and shows a HUD notice. This port binds G
//! too, and fades over the original's 250 ms with an ease in and
//! out. The frame hook is what makes the fade per-frame smooth
//! rather than tick-stepped; gamma
//! still only takes effect when the game renders, so nothing is
//! lost by stepping it there. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/fullbright_horizon>.
//!
//! The one sharp edge, as with any gamma mod on this SDK:
//! `set-gamma` has no unset, so once this mod sets a gamma it
//! holds that value until it is disabled or unloaded. Turning
//! off therefore fades back to the brightness the player had
//! when the mod first turned on — the mod remembers that value
//! itself — and keeps holding it. The player's own Brightness
//! setting is never changed, but while the mod is loaded the
//! Options slider is shadowed by the held value: changing it
//! has no visible effect until the mod is disabled or unloaded,
//! which restores the setting within a frame. The same holds
//! while the mod is on, by design: the strongest gamma any
//! running mod sets wins.
//!
//! Gamma 10.0 saturates the lightmap toward white, so colours
//! wash out compared to a uniformly filled lightmap. The effect
//! is render-only, so it works in multiplayer exactly as in
//! single player — nothing is sent to a server.
use horizon_mod_sdk::{
    Mod, Registration, game,
    hud::{self, Corner, Line},
    input::{self, Event},
    render, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `render.get-gamma` / `render.set-gamma` are level 13,
/// the rest level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 13;

/// The key's name for `register-key`, and its default: the
/// original's G. (The catalog's other Fullbright binds J because
/// it treats G as vanilla Quick Actions; the original binds G,
/// and the fidelity rule for this port keeps the original's
/// binding.)
const KEY: &str = "toggle";
const DEFAULT_KEY: &str = "key.keyboard.g";

/// Maximum brightness: 1000% gamma, the value the original toggles to.
const FULLBRIGHT_GAMMA: f32 = 10.0;
/// The fade's length in seconds: the original's 250 ms.
const FADE_SECONDS: f32 = 0.25;

/// The storage key for the switch.
const ENABLED_KEY: &str = "enabled";

/// The settings page's one button: the toggle.
const ENABLED_BUTTON: u32 = 0;

/// The smoothstep ease the fade rides: slow at both ends, so the
/// brightness glides rather than ramps.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The gamma `elapsed` seconds into a fade from `from` to `to`:
/// eased over [`FADE_SECONDS`], exactly `to` once the fade is
/// done. A fade whose ends are equal is done at once.
fn fade_value(from: f32, to: f32, elapsed: f32) -> f32 {
    if from == to {
        return to;
    }
    from + (to - from) * ease(elapsed / FADE_SECONDS)
}

/// One fade in progress: `from` to `to`, `elapsed` seconds in.
struct Anim {
    from: f32,
    to: f32,
    elapsed: f32,
}

impl Anim {
    /// Advances the fade by `seconds` and returns the gamma to
    /// set now, or `None` once the fade has finished (the last
    /// returned value is exactly the target).
    fn advance(&mut self, seconds: f32) -> Option<f32> {
        if self.elapsed >= FADE_SECONDS {
            return None;
        }
        self.elapsed += seconds;
        Some(fade_value(self.from, self.to, self.elapsed))
    }
}

fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

struct Fullbright {
    enabled: bool,
    /// The brightness the player had when the mod first turned
    /// on (or when it resumed after a reload): toggle-off fades
    /// back to this, and it is only re-captured while the mod
    /// holds nothing, so a mid-fade re-toggle never overwrites
    /// it with a half-faded value.
    base: f32,
    /// The last gamma this mod set, while it holds one.
    current: f32,
    /// Whether this mod currently holds a gamma at all (nothing
    /// is held before the first toggle, or after a reload, until
    /// the fade starts).
    holding: bool,
    anim: Option<Anim>,
    /// The HUD notice's text while one shows.
    notice: Option<String>,
}

impl Fullbright {
    fn toggle(&mut self) {
        let enabled = !self.enabled;
        self.apply(enabled);
    }

    fn apply(&mut self, enabled: bool) {
        self.enabled = enabled;
        let _ = storage::set(ENABLED_KEY, if enabled { b"true" } else { b"false" });
        if enabled {
            self.begin_on();
        } else {
            self.begin_off();
        }
        self.show_notice(if enabled {
            "Fullbright on"
        } else {
            "Fullbright off"
        });
        self.refresh_settings();
    }

    /// Start turning on, from whatever gamma is in effect now.
    fn begin_on(&mut self) {
        let from = if self.holding {
            self.current
        } else {
            render::get_gamma()
        };
        if !self.holding {
            self.base = from;
        }
        // Interruptible: a re-toggle mid-fade starts the new fade
        // from the gamma in effect at this moment.
        self.anim = Some(Anim {
            from,
            to: FULLBRIGHT_GAMMA,
            elapsed: 0.0,
        });
    }

    /// Start turning off, back to the brightness captured at turn-on.
    fn begin_off(&mut self) {
        let from = if self.holding {
            self.current
        } else {
            render::get_gamma()
        };
        if self.holding {
            self.anim = Some(Anim {
                from,
                to: self.base,
                elapsed: 0.0,
            });
        }
    }

    /// The settings page: the toggle.
    fn refresh_settings(&self) {
        ui::set_settings(&[Widget::Button(ButtonWidget {
            id: ENABLED_BUTTON,
            label: format!("Fullbright: {}", on_off(self.enabled)),
        })]);
    }

    /// The original's HUD notice: a line naming which way it went.
    fn show_notice(&mut self, text: &str) {
        hud::set_panel(
            Corner::TopLeft,
            &[Line {
                text: text.to_string(),
                color: 0xFFFFFF,
            }],
        );
        self.notice = Some(text.to_string());
    }

    fn clear_notice(&mut self) {
        if self.notice.is_some() {
            hud::clear();
            self.notice = None;
        }
    }
}

impl Mod for Fullbright {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(KEY, "Toggle Fullbright", DEFAULT_KEY)?;
        let state = Fullbright {
            enabled: read_bool(storage::get(ENABLED_KEY), false),
            base: 0.5,
            current: 0.5,
            holding: false,
            anim: None,
            notice: None,
        };
        Ok((
            state,
            Registration::new().command("fullbright_horizon").on_frame(),
        ))
    }

    fn frame(&mut self, seconds: f32) {
        // A hitched frame must not jump the fade: the step the
        // maths sees is capped at a twentieth of a second.
        let dt = seconds.clamp(0.0, 0.05);
        for event in input::poll() {
            if let Event::KeyDown(k) = event
                && k == KEY
            {
                self.toggle();
            }
        }
        if game::get_player().is_none() {
            // No world open (title screen): the notice goes, and
            // the fade waits — it resumes where it left off on
            // re-entry.
            self.clear_notice();
            return;
        }
        if self.enabled && !self.holding && self.anim.is_none() {
            // Resumed after a reload while still on: capture the
            // player's brightness now (nothing is held) and fade up.
            self.begin_on();
        }
        if let Some(anim) = &mut self.anim {
            if let Some(value) = anim.advance(dt) {
                render::set_gamma(value);
                self.current = value;
                self.holding = true;
            } else {
                self.anim = None;
            }
        }
        // The notice names the toggle, then goes: once the fade
        // has finished and nothing is fading, the panel clears.
        if self.anim.is_none() && self.notice.is_some() {
            self.clear_notice();
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "fullbright_horizon" {
            return Err("This command does nothing".into());
        }
        self.toggle();
        Ok(Some(format!(
            "Fullbright {}",
            if self.enabled { "on" } else { "off" }
        )))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        if id == ENABLED_BUTTON {
            self.toggle();
        }
    }
}

horizon_mod_sdk::export_mod!(Fullbright);

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn api_level_is_thirteen() {
        // Newest imported functions are `render.get-gamma` /
        // `render.set-gamma` (level 13); the rest level 0.
        assert_eq!(API_LEVEL, 13);
    }

    #[test]
    fn ease_is_smooth_and_lands() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!(close(ease(0.5), 0.5));
        // Slow at the ends: a tenth in moves less than a tenth.
        assert!(ease(0.1) < 0.1);
        assert!(ease(0.9) > 0.9);
        // Monotone throughout.
        let mut prev = ease(0.0);
        for i in 1..=20 {
            let v = ease(i as f32 / 20.0);
            assert!(v >= prev, "ease fell at {i}");
            prev = v;
        }
        // Clamped outside the range.
        assert_eq!(ease(-1.0), 0.0);
        assert_eq!(ease(2.0), 1.0);
    }

    #[test]
    fn fade_value_eases_and_lands_exactly() {
        // 0.5 to 10.0 over 0.25 s: nothing at the start, halfway is
        // the eased middle (5.25), the end is exactly 10.0.
        assert_eq!(fade_value(0.5, 10.0, 0.0), 0.5);
        assert!(close(fade_value(0.5, 10.0, 0.125), 5.25));
        assert_eq!(fade_value(0.5, 10.0, 0.25), 10.0);
        assert_eq!(fade_value(0.5, 10.0, 3.0), 10.0);
        // Back down the same way.
        assert!(close(fade_value(10.0, 0.5, 0.125), 5.25));
        assert_eq!(fade_value(10.0, 0.5, 0.25), 0.5);
        // Equal ends: done at once.
        assert_eq!(fade_value(3.0, 3.0, 0.0), 3.0);
    }

    #[test]
    fn anim_finishes_in_its_time_and_lands_on_target() {
        let mut anim = Anim {
            from: 0.5,
            to: 10.0,
            elapsed: 0.0,
        };
        let mut values = Vec::new();
        while let Some(v) = anim.advance(0.05) {
            values.push(v);
        }
        // Five 0.05 s steps make the 0.25 s fade; the last is the
        // target exactly, and the steps rise throughout.
        assert_eq!(values.len(), 5);
        assert_eq!(values[4], 10.0);
        assert!(values.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(anim.advance(0.05), None);
    }

    #[test]
    fn anim_reversal_starts_from_the_current_value() {
        // Mid-fade the value is somewhere between the ends; a new
        // fade built from it (what a re-toggle does) starts there
        // and still lands on its target.
        let mut anim = Anim {
            from: 0.5,
            to: 10.0,
            elapsed: 0.0,
        };
        let mid = anim.advance(0.1).unwrap();
        assert!(mid > 0.5 && mid < 10.0);
        let mut back = Anim {
            from: mid,
            to: 0.5,
            elapsed: 0.0,
        };
        assert_eq!(back.advance(0.0), Some(mid));
        let mut last = mid;
        while let Some(v) = back.advance(0.05) {
            last = v;
        }
        assert_eq!(last, 0.5);
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert!(!read_bool(None, false));
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert!(read_bool(Some(b"true".to_vec()), false));
        assert_eq!(on_off(true), "ON");
        assert_eq!(on_off(false), "OFF");
    }
}

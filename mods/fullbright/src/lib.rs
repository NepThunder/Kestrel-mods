//! Fullbright for Kestrel: maximum brightness (1000% gamma, `render.set-gamma`
//! 10.0, API level 13) on a toggle, with a one-second fade. J toggles it
//! (rebindable on the mod's page in Options > Mods), `/fullbright` does the
//! same, and that page also has the fade switch; the game keeps the choices.
//! While it is on, a "Fullbright on" notice sits in the top-left corner and
//! the toggle answers in chat.
//!
//! Behaviour reimplemented from Fullbright 1.0.0 by CayMac (MIT,
//! <https://github.com/CayMac97/fullbright>); this file shares no code with
//! the original — the original is a native mod, this is a fresh
//! implementation against the Kestrel SDK. The original binds G, fades over
//! 250 ms and shows a HUD notice; this port binds J instead, because G is
//! vanilla Quick Actions, and fades over ~20 ticks (about a second), because
//! a mod can only step gamma once a tick — there is no frame hook to fade
//! on. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/fullbright>.
//!
//! How the toggle works, and the one sharp edge: `set-gamma` has no unset,
//! so once this mod sets a gamma it holds that value until it is disabled
//! or unloaded. Turning off therefore fades back to the brightness the
//! player had when the mod first turned on — the mod remembers that value
//! itself — and keeps holding it. The player's own Brightness setting is
//! never changed, but while the mod is loaded the Options slider is
//! shadowed by the held value: changing it has no visible effect until
//! the mod is disabled or unloaded, which restores the setting within a
//! frame. The same holds while the mod is on, by design: the strongest
//! gamma any running mod sets wins, so another mod holding more than 10.0
//! would win, and one holding less never dims this one.
//!
//! Gamma 10.0 saturates the lightmap toward white (the `update_lightmap`
//! lerp extrapolates past 1.0 and the pixels clip), so colours wash out
//! next to the Extras fullbright flag, which fills the lightmap uniformly
//! and keeps them. The fade is tick-stepped, not frame-smooth. The effect
//! is render-only, so it works in multiplayer exactly as the Extras
//! fullbright already does — nothing is sent to a server.
use horizon_mod_sdk::{
    Mod, Registration, chat, game,
    hud::{self, Corner, Line},
    input::{self, Event},
    render, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `render.get-gamma` / `render.set-gamma` are level 13, the rest level 0
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 13;

/// The key's name for `register-key`, and its default: J. The original
/// binds G, but G is vanilla Quick Actions (`key.quickActions`), and the
/// other free letters are taken — Z is the Extras zoom key, H is Hitbox+,
/// K is Armor HUD, N/M are Waypoints, V is Classic Zoom, R/U are JEI —
/// so J is the nearest free key with no vanilla or Kestrel use.
const KEY: &str = "toggle";
const DEFAULT_KEY: &str = "key.keyboard.j";

/// Maximum brightness: 1000% gamma, the value the original toggles to.
const FULLBRIGHT_GAMMA: f32 = 10.0;
/// The fade's length in ticks: 20 steps at 20 tps is about a second, the
/// original's 250 ms stretched to what a per-tick fade can do.
const FADE_TICKS: u32 = 20;

/// The storage keys for the two settings.
const ENABLED_KEY: &str = "enabled";
const FADE_KEY: &str = "fade";

/// The settings page's buttons: the toggle, and the fade switch.
const ENABLED_BUTTON: u32 = 0;
const FADE_BUTTON: u32 = 1;

/// The gamma after `step` of `total` steps from `from` to `to`, linear;
/// the last step (and any step past it) is exactly `to`.
fn fade_step(from: f32, to: f32, step: u32, total: u32) -> f32 {
    if step >= total {
        to
    } else {
        from + (to - from) * (step as f32 / total as f32)
    }
}

/// One fade in progress: `from` to `to` over [`FADE_TICKS`] ticks.
struct Anim {
    from: f32,
    to: f32,
    step: u32,
}

impl Anim {
    /// The next step's gamma, or `None` once the fade has run its steps.
    fn next_value(&mut self) -> Option<f32> {
        if self.step >= FADE_TICKS {
            return None;
        }
        self.step += 1;
        Some(fade_step(self.from, self.to, self.step, FADE_TICKS))
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
    fade: bool,
    /// The brightness the player had when the mod first turned on (or
    /// when it resumed after a reload): toggle-off fades back to this,
    /// and it is only re-captured while the mod holds nothing, so a
    /// mid-fade re-toggle never overwrites it with a half-faded value.
    base: f32,
    /// The last gamma this mod set, while it holds one.
    current: f32,
    /// Whether this mod currently holds a gamma at all (nothing is held
    /// before the first toggle, or after a reload, until the fade starts).
    holding: bool,
    anim: Option<Anim>,
    /// Whether the "Fullbright on" notice is currently shown.
    panel_shown: bool,
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
        if self.fade {
            self.anim = Some(Anim {
                from,
                to: FULLBRIGHT_GAMMA,
                step: 0,
            });
        } else {
            render::set_gamma(FULLBRIGHT_GAMMA);
            self.current = FULLBRIGHT_GAMMA;
            self.holding = true;
        }
        self.show_notice();
    }

    /// Start turning off, back to the brightness captured at turn-on.
    fn begin_off(&mut self) {
        let from = if self.holding {
            self.current
        } else {
            render::get_gamma()
        };
        if self.fade && self.holding {
            self.anim = Some(Anim {
                from,
                to: self.base,
                step: 0,
            });
        } else if self.holding {
            render::set_gamma(self.base);
            self.current = self.base;
        }
        self.clear_notice();
    }

    fn toggle_fade(&mut self) {
        self.fade = !self.fade;
        let _ = storage::set(FADE_KEY, if self.fade { b"true" } else { b"false" });
        if !self.fade {
            // Fading is off mid-fade: jump straight to the fade's end.
            if let Some(anim) = self.anim.take() {
                render::set_gamma(anim.to);
                self.current = anim.to;
                self.holding = true;
            }
        }
        self.refresh_settings();
    }

    /// The settings page: the toggle, and the fade switch.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: ENABLED_BUTTON,
                label: format!("Fullbright: {}", on_off(self.enabled)),
            }),
            Widget::Button(ButtonWidget {
                id: FADE_BUTTON,
                label: format!("Fade: {}", on_off(self.fade)),
            }),
        ]);
    }

    fn show_notice(&mut self) {
        hud::set_panel(
            Corner::TopLeft,
            &[Line {
                text: "Fullbright on".to_string(),
                color: 0xFFFFFF,
            }],
        );
        self.panel_shown = true;
    }

    fn clear_notice(&mut self) {
        if self.panel_shown {
            hud::clear();
            self.panel_shown = false;
        }
    }
}

impl Mod for Fullbright {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(KEY, "Toggle Fullbright", DEFAULT_KEY)?;
        let state = Fullbright {
            enabled: read_bool(storage::get(ENABLED_KEY), false),
            fade: read_bool(storage::get(FADE_KEY), true),
            base: 0.5,
            current: 0.5,
            holding: false,
            anim: None,
            panel_shown: false,
        };
        Ok((state, Registration::new().command("fullbright").on_tick()))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            if let Event::KeyDown(k) = event
                && k == KEY
            {
                self.toggle();
                chat::show(&format!(
                    "Fullbright {}",
                    if self.enabled { "on" } else { "off" }
                ));
            }
        }
        if game::get_player().is_none() {
            // No world open (title screen): the notice goes, and the
            // fade waits — it resumes where it left off on re-entry.
            self.clear_notice();
            return;
        }
        if self.enabled {
            if !self.panel_shown {
                self.show_notice();
            }
            if !self.holding && self.anim.is_none() {
                // Resumed after a reload while still on: capture the
                // player's brightness now (nothing is held) and fade up.
                self.begin_on();
            }
        }
        if let Some(anim) = &mut self.anim {
            if let Some(value) = anim.next_value() {
                render::set_gamma(value);
                self.current = value;
                self.holding = true;
            } else {
                self.anim = None;
            }
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "fullbright" {
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
        match id {
            ENABLED_BUTTON => self.toggle(),
            FADE_BUTTON => self.toggle_fade(),
            _ => {}
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
    fn fade_step_is_linear_and_lands_exactly() {
        // 0.5 to 10.0 over 20 ticks: a quarter in is 2.875, halfway is
        // 5.25, and the last step is exactly 10.0.
        assert!(close(fade_step(0.5, 10.0, 0, 20), 0.5));
        assert!(close(fade_step(0.5, 10.0, 1, 20), 0.975));
        assert!(close(fade_step(0.5, 10.0, 5, 20), 2.875));
        assert!(close(fade_step(0.5, 10.0, 10, 20), 5.25));
        assert!(close(fade_step(0.5, 10.0, 19, 20), 9.525));
        assert_eq!(fade_step(0.5, 10.0, 20, 20), 10.0);
        assert_eq!(fade_step(0.5, 10.0, 21, 20), 10.0);
        // And back down: halfway is 5.25 again, the last step 0.5.
        assert!(close(fade_step(10.0, 0.5, 10, 20), 5.25));
        assert!(close(fade_step(10.0, 0.5, 1, 20), 9.525));
        assert_eq!(fade_step(10.0, 0.5, 20, 20), 0.5);
    }

    #[test]
    fn anim_runs_twenty_steps_then_stops() {
        let mut anim = Anim {
            from: 0.5,
            to: 10.0,
            step: 0,
        };
        let mut values = Vec::new();
        while let Some(v) = anim.next_value() {
            values.push(v);
        }
        assert_eq!(values.len(), 20);
        assert!(close(values[0], 0.975));
        assert!(close(values[9], 5.25));
        assert_eq!(values[19], 10.0);
        assert!(values.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(anim.next_value(), None);
    }

    #[test]
    fn anim_down_runs_twenty_steps_then_stops() {
        let mut anim = Anim {
            from: 10.0,
            to: 0.5,
            step: 0,
        };
        let mut values = Vec::new();
        while let Some(v) = anim.next_value() {
            values.push(v);
        }
        assert_eq!(values.len(), 20);
        assert!(close(values[0], 9.525));
        assert!(close(values[9], 5.25));
        assert_eq!(values[19], 0.5);
        assert!(values.windows(2).all(|w| w[0] > w[1]));
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

//! Fullbright & NoFog for Kestrel: maximum brightness (gamma 10.0, the
//! 1000% Fullbright value) with distance fog removed, on one toggle.
//! Press B (rebindable on the mod's page in Options > Mods), or
//! `/fullbright-nofog`, to turn the effect on or off. Turning it on
//! fades the gamma from the player's own Brightness up to 10.0 over
//! about 20 ticks (1 second) and turns distance fog off at once;
//! turning it off fades the gamma back to the Brightness the player
//! had before and turns distance fog back on. A corner panel says
//! "Fullbright & NoFog on" while the effect is on. The settings page
//! has the same switch plus a fade switch; the game keeps the choices.
//!
//! Behaviour reimplemented from Fullbright & NoFog 1.0.0 by Nirokx
//! (MIT, <https://github.com/Nirokx/horizon-fullbright-nofog>); this
//! file shares no code with the original — the original is a native
//! mod, this is a fresh implementation against the Kestrel SDK. The
//! original binds G for fullbright and N for fog, has separate
//! `/fullbright` and `/nofog` commands (plus `/nofog fluids`, which
//! also clears water, lava and powder-snow fog), and is silent on
//! toggle. This port is one combined toggle instead: B, because the
//! parallel Fullbright port uses J (verified in its checkout while
//! this port was written) and N is Waypoints' create key — and one
//! `/fullbright-nofog` command, so the two mods never fight over a
//! key or a command. The player's Brightness slider and
//! `options.txt` are never changed; turning the effect off gives the
//! player's own brightness back — as far as the SDK allows (see the
//! README's known limits: the mod holds the remembered value while
//! it is loaded, so the slider looks dead until the mod is disabled
//! or unloaded). This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/fullbright-nofog>.
//!
//! What the SDK cannot do, the original's native build can: `/nofog
//! fluids` — water, lava and powder-snow fog stay, because
//! `set-fog(false)` removes distance fog only (see the README). The
//! fade is tick-stepped, about 20 ticks, not frame-smooth, and gamma
//! 10.0 saturates toward white rather than the Extras fullbright
//! flag's uniform full light. Works in singleplayer and multiplayer:
//! gamma and fog change only what the local client renders, the same
//! effect the Extras fullbright switch and the Fog graphics option
//! already grant in multiplayer.
use horizon_mod_sdk::{
    Mod, Registration, chat, game,
    hud::{self, Corner, Line},
    input::{self, Event},
    render, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `render.get-gamma`, `render.set-gamma`, `render.get-fog` and
/// `render.set-fog` are level 13, the rest level 0
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 13;

/// The key's name for `register-key`, and its default: B, for
/// "bright". G is the original's fullbright key, but the parallel
/// Fullbright port uses J (verified in its checkout while this port
/// was written), and N (the original's fog key) is Waypoints' create
/// key — B is free among the Kestrel mods and unbound in vanilla.
const KEY: &str = "toggle";
const DEFAULT_KEY: &str = "key.keyboard.b";

/// The gamma the effect fades to: 10.0 is the 1000% Fullbright value
/// (`render.set-gamma` clamps host-side to 0.0..=10.0).
const FULLBRIGHT_GAMMA: f32 = 10.0;
/// The fade's length in ticks: 20 ticks at 20 tps is about a second,
/// the original Fullbright's smooth fade in tick steps.
const FADE_TICKS: u32 = 20;

/// The storage keys for the toggle, the fade switch and the
/// remembered Brightness.
const ENABLED_KEY: &str = "enabled";
const FADE_KEY: &str = "fade";
const BASE_GAMMA_KEY: &str = "base-gamma";

/// The settings page's buttons: the toggle, then the fade switch.
const TOGGLE_BUTTON: u32 = 0;
const FADE_BUTTON: u32 = 1;

/// The HUD notice while the effect is on.
const NOTICE: &str = "Fullbright & NoFog on";

/// A fade in progress: the gamma at its start, the gamma at its end,
/// and how many of its [`FADE_TICKS`] ticks have run.
struct Fade {
    from: f32,
    to: f32,
    step: u32,
}

/// The gamma a fade is at after `step` of its [`FADE_TICKS`] ticks,
/// linearly stepped; past the end it sits at the target.
fn fade_value(from: f32, to: f32, step: u32) -> f32 {
    let t = (step.min(FADE_TICKS) as f32) / (FADE_TICKS as f32);
    from + (to - from) * t
}

fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

/// The stored Brightness: the stored number when it parses and lies
/// in the gamma range the host accepts, else none.
fn read_base_gamma(stored: Option<Vec<u8>>) -> Option<f32> {
    let value = stored
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|s| s.parse::<f32>().ok())?;
    if value.is_finite() && (0.0..=10.0).contains(&value) {
        Some(value)
    } else {
        None
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

struct FullbrightNoFog {
    enabled: bool,
    fade: bool,
    /// The player's gamma before the first override, remembered so
    /// toggle-off returns to it. Read from `get-gamma` at toggle-on,
    /// when no override of ours is in effect.
    base_gamma: Option<f32>,
    /// A fade in progress, if any.
    fade_state: Option<Fade>,
    /// Whether the HUD notice is currently shown.
    panel_shown: bool,
    /// Whether the player was in a world on the last tick, so a
    /// world re-entry re-asserts the effect.
    in_world: bool,
}

impl FullbrightNoFog {
    /// Turn the effect on or off: remember the player's gamma, fade
    /// it to 10.0 and fog off — or fade it back and fog on — and
    /// persist the choice. With the fade switch off the gamma jumps
    /// straight to its target instead.
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        let _ = storage::set(ENABLED_KEY, if enabled { b"true" } else { b"false" });
        if enabled {
            let base = render::get_gamma();
            self.base_gamma = Some(base);
            let _ = storage::set(BASE_GAMMA_KEY, base.to_string().as_bytes());
            render::set_fog(false);
            if self.fade {
                self.fade_state = Some(Fade {
                    from: base,
                    to: FULLBRIGHT_GAMMA,
                    step: 0,
                });
            } else {
                render::set_gamma(FULLBRIGHT_GAMMA);
            }
        } else {
            // The remembered Brightness, or the vanilla default when
            // it was never stored: the host keeps no per-mod history,
            // so the mod itself is the record.
            let base = self.base_gamma.unwrap_or(0.5);
            render::set_fog(true);
            if self.fade {
                self.fade_state = Some(Fade {
                    from: render::get_gamma(),
                    to: base,
                    step: 0,
                });
            } else {
                render::set_gamma(base);
            }
        }
        self.refresh_settings();
    }

    fn toggle_fade(&mut self) {
        self.fade = !self.fade;
        let _ = storage::set(FADE_KEY, if self.fade { b"true" } else { b"false" });
        self.refresh_settings();
    }

    /// The settings page: the toggle, then the fade switch.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: TOGGLE_BUTTON,
                label: format!("Fullbright & NoFog: {}", on_off(self.enabled)),
            }),
            Widget::Button(ButtonWidget {
                id: FADE_BUTTON,
                label: format!("Fade: {}", on_off(self.fade)),
            }),
        ]);
    }

    /// Advance a fade one tick: the next stepped gamma, the exact
    /// target on the last step.
    fn advance_fade(&mut self) {
        let Some(fade) = &mut self.fade_state else {
            return;
        };
        fade.step += 1;
        if fade.step >= FADE_TICKS {
            let target = fade.to;
            self.fade_state = None;
            render::set_gamma(target);
        } else {
            render::set_gamma(fade_value(fade.from, fade.to, fade.step));
        }
    }

    /// The HUD notice follows the toggle: shown while the effect is
    /// on in a world, cleared otherwise.
    fn refresh_panel(&mut self, in_world: bool) {
        let want = self.enabled && in_world;
        if want && !self.panel_shown {
            hud::set_panel(
                Corner::BottomLeft,
                &[Line {
                    text: NOTICE.to_string(),
                    color: 0xFFFFFF,
                }],
            );
            self.panel_shown = true;
        } else if !want && self.panel_shown {
            hud::clear();
            self.panel_shown = false;
        }
    }
}

impl Mod for FullbrightNoFog {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(KEY, "Toggle Fullbright & NoFog", DEFAULT_KEY)?;
        let state = FullbrightNoFog {
            enabled: read_bool(storage::get(ENABLED_KEY), false),
            fade: read_bool(storage::get(FADE_KEY), true),
            base_gamma: read_base_gamma(storage::get(BASE_GAMMA_KEY)),
            fade_state: None,
            panel_shown: false,
            in_world: false,
        };
        Ok((
            state,
            Registration::new().command("fullbright-nofog").on_tick(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            if let Event::KeyDown(k) = event
                && k == KEY
            {
                let enabled = !self.enabled;
                self.set_enabled(enabled);
                chat::show(&format!(
                    "Fullbright & NoFog {}",
                    if enabled { "on" } else { "off" }
                ));
            }
        }
        let in_world = game::get_player().is_some();
        if !in_world {
            // At the title screen there is no renderer to brighten:
            // the notice goes, the fade waits, the toggle waits.
            self.fade_state = None;
            self.in_world = false;
            self.refresh_panel(false);
            return;
        }
        if !self.in_world {
            // A world re-entry: the effect re-asserts itself from the
            // Brightness in force now, fog off, fading as usual.
            self.in_world = true;
            if self.enabled {
                render::set_fog(false);
                if self.fade {
                    self.fade_state = Some(Fade {
                        from: render::get_gamma(),
                        to: FULLBRIGHT_GAMMA,
                        step: 0,
                    });
                } else {
                    render::set_gamma(FULLBRIGHT_GAMMA);
                }
            }
        }
        self.advance_fade();
        self.refresh_panel(true);
    }

    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        if name != "fullbright-nofog" {
            return Err("This command does nothing".into());
        }
        match args.trim() {
            "" => {
                let enabled = !self.enabled;
                self.set_enabled(enabled);
                Ok(Some(format!(
                    "Fullbright & NoFog {}",
                    if enabled { "on" } else { "off" }
                )))
            }
            "on" => {
                if !self.enabled {
                    self.set_enabled(true);
                }
                Ok(Some("Fullbright & NoFog on".to_string()))
            }
            "off" => {
                if self.enabled {
                    self.set_enabled(false);
                }
                Ok(Some("Fullbright & NoFog off".to_string()))
            }
            "status" => Ok(Some(format!(
                "gamma {} fog {}",
                render::get_gamma(),
                if render::get_fog() { "on" } else { "off" }
            ))),
            _ => Err("Usage: /fullbright-nofog [on|off|status]".into()),
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            TOGGLE_BUTTON => {
                let enabled = !self.enabled;
                self.set_enabled(enabled);
            }
            FADE_BUTTON => self.toggle_fade(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(FullbrightNoFog);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_thirteen() {
        // Newest imported functions are `render.get-gamma`,
        // `render.set-gamma`, `render.get-fog` and `render.set-fog`
        // (level 13); the rest are level 0.
        assert_eq!(API_LEVEL, 13);
    }

    #[test]
    fn fade_steps_linearly_over_twenty_ticks() {
        // 0.5 to 10.0: the first step, the midpoint and the end.
        assert_eq!(fade_value(0.5, 10.0, 0), 0.5);
        assert_eq!(fade_value(0.5, 10.0, 1), 0.975);
        assert_eq!(fade_value(0.5, 10.0, 10), 5.25);
        assert_eq!(fade_value(0.5, 10.0, 19), 9.525);
        assert_eq!(fade_value(0.5, 10.0, 20), 10.0);
        // Past the end it sits at the target.
        assert_eq!(fade_value(0.5, 10.0, 21), 10.0);
        assert_eq!(fade_value(0.5, 10.0, 100), 10.0);
        // And back down: 10.0 to 0.5 mirrors it.
        assert_eq!(fade_value(10.0, 0.5, 0), 10.0);
        assert_eq!(fade_value(10.0, 0.5, 10), 5.25);
        assert_eq!(fade_value(10.0, 0.5, 20), 0.5);
        // A no-op fade never moves.
        assert_eq!(fade_value(1.0, 1.0, 7), 1.0);
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert!(!read_bool(None, false));
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert!(read_bool(Some(b"true".to_vec()), false));
        assert_eq!(read_base_gamma(None), None);
        assert_eq!(read_base_gamma(Some(b"0.5".to_vec())), Some(0.5));
        assert_eq!(read_base_gamma(Some(b"1".to_vec())), Some(1.0));
        assert_eq!(read_base_gamma(Some(b"10".to_vec())), Some(10.0));
        assert_eq!(read_base_gamma(Some(b"0".to_vec())), Some(0.0));
        // Outside the range the host accepts, or not a number, it is
        // no remembered Brightness at all.
        assert_eq!(read_base_gamma(Some(b"11".to_vec())), None);
        assert_eq!(read_base_gamma(Some(b"-1".to_vec())), None);
        assert_eq!(read_base_gamma(Some(b"nope".to_vec())), None);
        assert_eq!(read_base_gamma(Some(b"NaN".to_vec())), None);
        assert_eq!(read_base_gamma(Some(b"inf".to_vec())), None);
        assert_eq!(on_off(true), "ON");
        assert_eq!(on_off(false), "OFF");
    }
}

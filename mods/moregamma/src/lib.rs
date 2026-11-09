//! MoreGamma for Kestrel: set the brightness (gamma) far past the video
//! settings' 100% "Bright" limit with `/gamma <value>`, or toggle full
//! brightness with `/fullbright`.
//!
//! Behaviour ported from MoreGamma by bezi5kt (MIT,
//! <https://github.com/bezi5kt/gamma-rust>): `/gamma <value>` sets your
//! own gamma, `/gamma` on its own shows the gamma in effect, `/gamma
//! reset` (or `off`) goes back to your Brightness setting, `/gamma on`
//! turns your last custom gamma on again, `/gamma 1500%` reads as a
//! percentage of vanilla's "Bright", and `/fullbright` toggles full
//! brightness. Rewritten in Rust against the Kestrel SDK; no original
//! code runs. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/moregamma>.
//!
//! What the SDK cannot do, the original's native build can: values past
//! 10.0 — the original goes to 100 and calls 15 fullbright, but the SDK
//! clamps host-side to 0.0..=10.0, so 10.0 is this port's fullbright and
//! anything higher reads back as 10; the Options > Mods page with its
//! on/off button, 100/200/500/1000/1500% cycle button and toggle key
//! (this port is commands only); and remembering your gamma between
//! sessions (this port keeps the last gamma in memory only — reloading
//! the mod restores your Brightness setting, and `/gamma on` then has
//! nothing to turn on until you set a value again).
use horizon_mod_sdk::{Mod, Registration, render};

/// The newest API level this mod needs, asserted in the tests below:
/// `render.get-gamma` and `render.set-gamma` are level 13, the rest
/// level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 13;

/// The fullbright gamma: the brightest the SDK allows, and the value
/// `/fullbright` sets. The original's fullbright is 15; the host clamps
/// it to this.
const FULLBRIGHT_GAMMA: f32 = 10.0;

/// What `/gamma` was asked to do, parsed without touching the game.
#[derive(Debug, PartialEq)]
enum GammaArg {
    /// `/gamma`: print the gamma in effect.
    Read,
    /// `/gamma reset` or `/gamma off`: go back to the Brightness setting.
    Reset,
    /// `/gamma on`: turn the last custom gamma on again.
    On,
    /// `/gamma <value>`: set this gamma. A trailing `%` reads as a
    /// percentage of vanilla's "Bright": `1500%` is 15, which the host
    /// clamps to 10.
    Set(f32),
}

/// Parse one `/gamma` argument: empty reads, `reset`/`off` restores,
/// `on` re-applies, anything else must be a number, with an optional
/// trailing `%`.
fn parse_gamma_arg(args: &str) -> Result<GammaArg, String> {
    let arg = args.trim();
    if arg.is_empty() {
        return Ok(GammaArg::Read);
    }
    match arg.to_ascii_lowercase().as_str() {
        "reset" | "off" => return Ok(GammaArg::Reset),
        "on" => return Ok(GammaArg::On),
        _ => {}
    }
    let (number, percent) = match arg.strip_suffix('%') {
        Some(number) => (number.trim(), true),
        None => (arg, false),
    };
    let value: f32 = number
        .parse()
        .map_err(|_| "gamma: give a number".to_string())?;
    Ok(GammaArg::Set(if percent { value / 100.0 } else { value }))
}

struct MoreGamma {
    /// The gamma in effect the first time this mod overrode one: the
    /// player's own Brightness setting when no other mod holds a gamma,
    /// which is what a reset (and `/fullbright` off) goes back to.
    base: Option<f32>,
    /// The last custom gamma this mod set, as read back after the host
    /// clamped it, so `/gamma on` re-applies the value that was really
    /// in effect.
    last: Option<f32>,
    /// Whether full brightness is on: set by `/fullbright`, and by any
    /// `/gamma` value that lands on the fullbright gamma.
    fullbright: bool,
}

impl MoreGamma {
    /// Remember the gamma in effect now, the first time this mod
    /// overrides one; later overrides keep the first one, so a reset
    /// always goes back to the setting the player had before the mod
    /// touched anything.
    fn remember_base(&mut self) {
        if self.base.is_none() {
            self.base = Some(render::get_gamma());
        }
    }

    /// Go back to the Brightness setting this mod first saw, if it ever
    /// overrode one. With nothing to go back to there is nothing to do,
    /// and the gamma in effect is already the player's own.
    fn restore(&mut self) {
        if let Some(base) = self.base {
            render::set_gamma(base);
        }
        self.fullbright = false;
    }

    fn gamma_command(&mut self, args: &str) -> Result<Option<String>, String> {
        match parse_gamma_arg(args)? {
            GammaArg::Read => Ok(Some(format!("gamma {}", render::get_gamma()))),
            GammaArg::Reset => {
                self.restore();
                Ok(Some(format!("gamma {}", render::get_gamma())))
            }
            GammaArg::On => {
                let Some(last) = self.last else {
                    return Err(
                        "gamma: no custom gamma set yet — use /gamma <value> first".to_string()
                    );
                };
                self.remember_base();
                render::set_gamma(last);
                self.fullbright = last >= FULLBRIGHT_GAMMA;
                Ok(Some(format!("gamma {}", render::get_gamma())))
            }
            GammaArg::Set(value) => {
                self.remember_base();
                render::set_gamma(value);
                let effective = render::get_gamma();
                self.last = Some(effective);
                self.fullbright = effective >= FULLBRIGHT_GAMMA;
                Ok(Some(format!("gamma {effective}")))
            }
        }
    }

    fn toggle_fullbright(&mut self) -> String {
        if self.fullbright {
            self.restore();
            format!("fullbright off: gamma {}", render::get_gamma())
        } else {
            self.remember_base();
            render::set_gamma(FULLBRIGHT_GAMMA);
            let effective = render::get_gamma();
            self.last = Some(effective);
            self.fullbright = true;
            format!("fullbright on: gamma {effective}")
        }
    }
}

impl Mod for MoreGamma {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("gamma").command("fullbright");
        Ok((
            MoreGamma {
                base: None,
                last: None,
                fullbright: false,
            },
            hooks,
        ))
    }

    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        match name {
            "gamma" => self.gamma_command(args),
            "fullbright" => Ok(Some(self.toggle_fullbright())),
            _ => Err("This command does nothing".into()),
        }
    }
}

horizon_mod_sdk::export_mod!(MoreGamma);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_thirteen() {
        // Newest imported functions are `render.get-gamma` and
        // `render.set-gamma` (level 13); the rest level 0.
        assert_eq!(API_LEVEL, 13);
    }

    #[test]
    fn gamma_arg_parses() {
        assert_eq!(parse_gamma_arg(""), Ok(GammaArg::Read));
        assert_eq!(parse_gamma_arg("   "), Ok(GammaArg::Read));
        assert_eq!(parse_gamma_arg("reset"), Ok(GammaArg::Reset));
        assert_eq!(parse_gamma_arg("OFF"), Ok(GammaArg::Reset));
        assert_eq!(parse_gamma_arg("On"), Ok(GammaArg::On));
        assert_eq!(parse_gamma_arg("1.5"), Ok(GammaArg::Set(1.5)));
        assert_eq!(parse_gamma_arg("3"), Ok(GammaArg::Set(3.0)));
        assert_eq!(parse_gamma_arg("10"), Ok(GammaArg::Set(10.0)));
        assert_eq!(parse_gamma_arg("15"), Ok(GammaArg::Set(15.0)));
        assert_eq!(parse_gamma_arg("0.5"), Ok(GammaArg::Set(0.5)));
        assert_eq!(parse_gamma_arg("-1"), Ok(GammaArg::Set(-1.0)));
        assert_eq!(parse_gamma_arg("1500%"), Ok(GammaArg::Set(15.0)));
        assert_eq!(parse_gamma_arg("100%"), Ok(GammaArg::Set(1.0)));
        assert_eq!(parse_gamma_arg("200%"), Ok(GammaArg::Set(2.0)));
        assert_eq!(
            parse_gamma_arg("frobnicate"),
            Err("gamma: give a number".to_string())
        );
        assert_eq!(
            parse_gamma_arg("1.5x"),
            Err("gamma: give a number".to_string())
        );
        assert_eq!(
            parse_gamma_arg("1 2"),
            Err("gamma: give a number".to_string())
        );
    }
}

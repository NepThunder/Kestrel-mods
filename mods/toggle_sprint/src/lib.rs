//! Toggle Sprint for Kestrel: sprint and sneak toggle instead
//! of holding the key down, with the classic status HUD —
//! `[Sprinting (Toggled)]`, `[Sprinting (Key Held)]`,
//! `[Sneaking (Toggled)]`, `[Sneaking (Key Held)]`, `[Flying]`,
//! `[Gliding (Elytra)]`, `[Riding]` — in the bottom-left corner.
//!
//! Behaviour reimplemented from Toggle Sprint by CayMac (MIT,
//! <https://github.com/CayMac97/toggle-sprint>), itself a port
//! of Better Sprinting and the Lunar/Badlion ToggleSprint HUD;
//! this file shares no code with the original. This port's home
//! is <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/toggle_sprint>.
//!
//! The toggle behaviour is the game's own: API 18's
//! `game.set-movement-toggles` (capability `game-read`) flips
//! the Toggle Sprint / Toggle Crouch *options* and saves them,
//! exactly as flipping them on the options page does — the
//! sprint and sneak keys then latch instead of holding, and a
//! toggled sneak pauses inside inventory and chest screens the
//! way the game's own option machinery handles screens, which
//! is the behaviour the original implements for the same
//! reason (so Shift-clicking items works normally). Nothing
//! here forces the player to move: turning an option off does
//! not un-latch a sprint already latched, as with the options
//! page. The mod's own keys are the original's — V toggles
//! sprint, C toggles sneak — and Options > Mods flips the same
//! two options from its settings page.
//!
//! Fidelity notes (the README has the full accounting): two
//! pieces of the original are not portable on today's SDK and
//! are omitted, not faked — the Creative Fly Speed Boost (no
//! fly-speed write exists; its multiplier setting and the
//! HUD's `(Nx Boost)` suffix go with it, so flying reads plain
//! `[Flying]`) and the `[Swimming]` line (the self read has no
//! in-water state).

use horizon_mod_sdk::{
    Mod, Registration,
    game::{self, SelfState},
    hud::{self, Corner, Line},
    input::{self, Event},
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `game.get-self-state` and `game.set-movement-toggles`
/// are level 18 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 18;

/// The mod's own keys, the original's: V toggles sprint, C
/// toggles sneak (the player can rebind both on the mod's page
/// in Options > Mods, and that binding wins).
const SPRINT_KEY: &str = "toggle_sprint";
const SNEAK_KEY: &str = "toggle_sneak";
const DEFAULT_SPRINT_KEY: &str = "key.keyboard.v";
const DEFAULT_SNEAK_KEY: &str = "key.keyboard.c";

/// The settings page's buttons, one per option.
const SPRINT_BUTTON: u32 = 0;
const SNEAK_BUTTON: u32 = 1;

/// The HUD's text colour: plain white, as the classic HUDs
/// draw it.
const TEXT_COLOR: u32 = 0xFF_FFFF;

/// The HUD's lines for one self-state, in the original's
/// order: sprinting, sneaking, flying, gliding, riding — one
/// per state that applies. A state latched by its toggle
/// option reads (Toggled); one without the option is the key
/// itself, (Key Held). The original's [Swimming] line has no
/// entry here: the self read carries no in-water state.
fn hud_lines(state: &SelfState) -> Vec<&'static str> {
    let mut lines = Vec::new();
    if state.sprinting {
        lines.push(if state.sprint_toggled {
            "[Sprinting (Toggled)]"
        } else {
            "[Sprinting (Key Held)]"
        });
    }
    if state.sneaking {
        lines.push(if state.sneak_toggled {
            "[Sneaking (Toggled)]"
        } else {
            "[Sneaking (Key Held)]"
        });
    }
    if state.flying {
        lines.push("[Flying]");
    }
    if state.gliding {
        lines.push("[Gliding (Elytra)]");
    }
    if state.mount.is_some() {
        lines.push("[Riding]");
    }
    lines
}

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn sprint_label(on: bool) -> String {
    format!("Toggle Sprint: {}", on_off(on))
}

fn sneak_label(on: bool) -> String {
    format!("Toggle Sneak: {}", on_off(on))
}

struct ToggleSprint {
    /// The two options as last read from the self-state (or as
    /// last flipped by this mod): what the settings page shows
    /// and what a key press flips from.
    sprint_option: bool,
    sneak_option: bool,
    /// The panel lines on screen, so an unchanged tick costs no
    /// call.
    drawn: Vec<&'static str>,
}

impl ToggleSprint {
    /// Flips the sprint option through the game's own settings
    /// write, and remembers the flip.
    fn flip_sprint(&mut self) {
        self.sprint_option = !self.sprint_option;
        game::set_movement_toggles(Some(self.sprint_option), None);
    }

    /// Flips the sneak option the same way.
    fn flip_sneak(&mut self) {
        self.sneak_option = !self.sneak_option;
        game::set_movement_toggles(None, Some(self.sneak_option));
    }

    /// The settings page in Options > Mods: one button per
    /// option, flipping the game's own option.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: SPRINT_BUTTON,
                label: sprint_label(self.sprint_option),
            }),
            Widget::Button(ButtonWidget {
                id: SNEAK_BUTTON,
                label: sneak_label(self.sneak_option),
            }),
        ]);
    }
}

impl Mod for ToggleSprint {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(SPRINT_KEY, "Toggle Sprint", DEFAULT_SPRINT_KEY)?;
        input::register_key(SNEAK_KEY, "Toggle Sneak", DEFAULT_SNEAK_KEY)?;
        Ok((
            ToggleSprint {
                sprint_option: false,
                sneak_option: false,
                drawn: Vec::new(),
            },
            Registration::new().on_tick(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        // The options as the game holds them now: a flip made
        // on the options page is picked up here too.
        if let Some(state) = game::get_self_state() {
            self.sprint_option = state.sprint_toggled;
            self.sneak_option = state.sneak_toggled;
            let lines = hud_lines(&state);
            if lines != self.drawn {
                if lines.is_empty() {
                    hud::clear();
                } else {
                    let panel: Vec<Line> = lines
                        .iter()
                        .map(|text| Line {
                            text: text.to_string(),
                            color: TEXT_COLOR,
                        })
                        .collect();
                    hud::set_panel(Corner::BottomLeft, &panel);
                }
                self.drawn = lines;
            }
        } else if !self.drawn.is_empty() {
            hud::clear();
            self.drawn = Vec::new();
        }
        // The mod's own keys: V and C flip the options. The
        // flips write the options the game just reported, so a
        // press is never applied twice from a stale read.
        for event in input::poll() {
            match event {
                Event::KeyDown(name) if name == SPRINT_KEY => self.flip_sprint(),
                Event::KeyDown(name) if name == SNEAK_KEY => self.flip_sneak(),
                _ => {}
            }
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SPRINT_BUTTON => self.flip_sprint(),
            SNEAK_BUTTON => self.flip_sneak(),
            _ => return,
        }
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(ToggleSprint);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::game::Mount;

    fn state() -> SelfState {
        SelfState {
            sprinting: false,
            sneaking: false,
            flying: false,
            gliding: false,
            mount: None,
            sprint_toggled: false,
            sneak_toggled: false,
        }
    }

    fn mount() -> Mount {
        Mount {
            id: 42,
            kind: "minecraft:horse".into(),
            health: 24.5,
            max_health: 30.0,
        }
    }

    #[test]
    fn api_level_is_18() {
        assert_eq!(API_LEVEL, 18);
    }

    #[test]
    fn standing_shows_nothing() {
        assert!(hud_lines(&state()).is_empty());
        // The options on their own are not a state either.
        let mut s = state();
        s.sprint_toggled = true;
        s.sneak_toggled = true;
        assert!(hud_lines(&s).is_empty());
    }

    #[test]
    fn sprint_and_sneak_name_how_they_are_held() {
        let mut s = state();
        s.sprinting = true;
        assert_eq!(hud_lines(&s), ["[Sprinting (Key Held)]"]);
        s.sprint_toggled = true;
        assert_eq!(hud_lines(&s), ["[Sprinting (Toggled)]"]);
        let mut s = state();
        s.sneaking = true;
        assert_eq!(hud_lines(&s), ["[Sneaking (Key Held)]"]);
        s.sneak_toggled = true;
        assert_eq!(hud_lines(&s), ["[Sneaking (Toggled)]"]);
    }

    #[test]
    fn flying_gliding_and_riding_have_their_lines() {
        let mut s = state();
        s.flying = true;
        assert_eq!(hud_lines(&s), ["[Flying]"]);
        let mut s = state();
        s.gliding = true;
        assert_eq!(hud_lines(&s), ["[Gliding (Elytra)]"]);
        let mut s = state();
        s.mount = Some(mount());
        assert_eq!(hud_lines(&s), ["[Riding]"]);
    }

    #[test]
    fn states_stack_in_the_originals_order() {
        let mut s = state();
        s.sprinting = true;
        s.sprint_toggled = true;
        s.mount = Some(mount());
        assert_eq!(hud_lines(&s), ["[Sprinting (Toggled)]", "[Riding]"]);
    }

    #[test]
    fn the_settings_labels_name_the_options() {
        assert_eq!(sprint_label(true), "Toggle Sprint: On");
        assert_eq!(sprint_label(false), "Toggle Sprint: Off");
        assert_eq!(sneak_label(true), "Toggle Sneak: On");
        assert_eq!(sneak_label(false), "Toggle Sneak: Off");
    }
}

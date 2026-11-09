//! Mod Menu for Kestrel: a Mods button on the title screen and
//! in the pause menu, opening the list of your mods.
//!
//! Behaviour reimplemented from Mod Menu by Prospector and
//! TerraformersMC, MIT, <https://github.com/TerraformersMC/ModMenu>,
//! via DeportedToHell's Horizon port; this file shares no code
//! with either — this is a fresh implementation against the
//! Kestrel SDK. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/modmenu>.
//!
//! Fidelity, against the original's registry description: the
//! Mods buttons land with the API 22 screen district — the
//! title screen's (`screen.set-buttons`, `title-row` anchor)
//! and the pause menu's (`menu-column` anchor) — and the Open
//! Mod Menu key is here (registered, starting unbound, as the
//! platform's own key mods do). What is NOT here is the list's
//! data, and the port says so everywhere rather than inventing
//! it: no SDK interface lets a sandboxed mod enumerate the
//! installed mods, so the screen the buttons open lists the
//! one mod this mod can honestly name — itself, with its own
//! details page — and states the gap on its face. The mod
//! count after the version, the update indicator, search,
//! sorting, the library filter, the other mods' details,
//! credits, links and Configure buttons all wait on that
//! mod-list read; they arrive with it, as the API 22 design
//! note schedules. The buttons' own visibility switches are
//! the part of Mod Menu's options that applies to what this
//! port has; the rest of its 21 options configure a list that
//! cannot be read yet.
//!
//! Presses are heard through `screen.poll`, drained in both
//! the tick and frame hooks: in a world the tick answers, and
//! on the title screen — which has no tick — the frame does,
//! exactly the split the design note names.

use horizon_mod_sdk::{
    Mod, Registration, input,
    input::Event,
    screen::{self, Anchor, ScreenButton},
    storage,
    ui::{self, ButtonWidget, ListWidget, TextWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `screen.set-buttons` and `screen.poll` are level 22
/// (`crates/horizon-mods/src/api_level.rs`); the rest is older.
#[cfg(test)]
const API_LEVEL: u32 = 22;

/// The buttons the mod declares on the game's screens.
const TITLE_BUTTON: u32 = 1;
const PAUSE_BUTTON: u32 = 2;

/// The list screen's widgets and the details screen's.
const LIST_WIDGET: u32 = 1;
const BACK_BUTTON: u32 = 2;

/// The settings page's buttons.
const TITLE_SETTING: u32 = 10;
const PAUSE_SETTING: u32 = 11;

/// The registered key that opens the mods screen, Mod Menu's
/// own "Open Mod Menu" key. It starts unbound
/// (`key.keyboard.unknown`): the player binds it on the mod's
/// page in Options > Mods. Registered keys fire while playing,
/// so the key works in a world; the buttons carry the title
/// and pause screens.
const OPEN_KEY: &str = "open";
const OPEN_KEY_DEFAULT: &str = "key.keyboard.unknown";

/// Storage keys for the two button switches.
const TITLE_KEY: &str = "title_button";
const PAUSE_KEY: &str = "pause_button";

/// This mod's own facts, from its manifest: the one entry the
/// list can honestly show until the SDK can read the game's
/// mod list.
const OWN_NAME: &str = "Mod Menu";
const OWN_ROW: &str = "Mod Menu 0.1.0";
const OWN_SUMMARY: &str =
    "A Mods button on the title screen and pause menu, opening the list of your mods.";
const OWN_CREDITS: &str = "By Prospector and TerraformersMC; ported for Kestrel by NepThunder.";
const OWN_LICENCE: &str = "Licence: MIT";
/// What the list says about everything it cannot show.
const LIST_GAP: &str = "Mod Menu can see only itself: the game does not let mods read the installed mod list yet, so your other mods are not listed here.";

struct ModMenu {
    title_button: bool,
    pause_button: bool,
}

/// A stored on/off setting ("1" / "0"); `default` when
/// unreadable.
fn read_switch(key: &str, default: bool) -> bool {
    match storage::get(key) {
        Some(bytes) => match std::str::from_utf8(&bytes) {
            Ok("1") => true,
            Ok("0") => false,
            _ => default,
        },
        None => default,
    }
}

fn store_switch(key: &str, on: bool) {
    let _ = storage::set(key, if on { b"1" } else { b"0" });
}

/// The buttons for these switches, in declaration order: the
/// title screen's row and the pause menu's column, each
/// labelled as the original labels its button.
fn buttons(title: bool, pause: bool) -> Vec<ScreenButton> {
    let mut out = Vec::new();
    if title {
        out.push(ScreenButton {
            id: TITLE_BUTTON,
            screen: "title".to_string(),
            menu: None,
            anchor: Anchor::TitleRow,
            label: "Mods".to_string(),
            icon: None,
        });
    }
    if pause {
        out.push(ScreenButton {
            id: PAUSE_BUTTON,
            screen: "paused".to_string(),
            menu: None,
            anchor: Anchor::MenuColumn,
            label: "Mods".to_string(),
            icon: None,
        });
    }
    out
}

/// The mods screen's widgets: the gap stated plainly, then the
/// one mod the list can name. Picking its row opens its
/// details, as a click on a row does in the original.
fn list_widgets() -> Vec<Widget> {
    vec![
        Widget::Text(TextWidget {
            id: 0,
            text: LIST_GAP.to_string(),
            color: 0,
        }),
        Widget::List(ListWidget {
            id: LIST_WIDGET,
            rows: vec![OWN_ROW.to_string()],
            selected: None,
        }),
    ]
}

/// The one details page the port can fill truthfully: Mod
/// Menu's own.
fn detail_widgets() -> Vec<Widget> {
    vec![
        Widget::Text(TextWidget {
            id: 0,
            text: OWN_NAME.to_string(),
            color: 0,
        }),
        Widget::Text(TextWidget {
            id: 0,
            text: OWN_SUMMARY.to_string(),
            color: 0,
        }),
        Widget::Text(TextWidget {
            id: 0,
            text: OWN_CREDITS.to_string(),
            color: 0,
        }),
        Widget::Text(TextWidget {
            id: 0,
            text: OWN_LICENCE.to_string(),
            color: 0,
        }),
        Widget::Button(ButtonWidget {
            id: BACK_BUTTON,
            label: "Back".to_string(),
        }),
    ]
}

fn shown(b: bool) -> &'static str {
    if b { "Shown" } else { "Hidden" }
}

/// The settings page: the two buttons' switches, the part of
/// Mod Menu's options that governs what this port places.
fn settings_widgets(title: bool, pause: bool) -> Vec<Widget> {
    vec![
        Widget::Button(ButtonWidget {
            id: TITLE_SETTING,
            label: format!("Title Screen Button: {}", shown(title)),
        }),
        Widget::Button(ButtonWidget {
            id: PAUSE_SETTING,
            label: format!("Pause Menu Button: {}", shown(pause)),
        }),
    ]
}

impl ModMenu {
    /// Declares the buttons for the current switches.
    fn declare(&self) {
        screen::set_buttons(&buttons(self.title_button, self.pause_button));
    }

    /// Opens the mods screen.
    fn open_list(&self) {
        ui::open_screen(&ui::Screen {
            title: "Mods".to_string(),
            widgets: list_widgets(),
        });
    }

    /// Hears the screens' presses and the open key: a press on
    /// either Mods button opens the list.
    fn drain(&mut self) {
        for event in screen::poll() {
            if let screen::ScreenEvent::Button(TITLE_BUTTON | PAUSE_BUTTON) = event {
                self.open_list();
            }
        }
        for event in input::poll() {
            if let Event::KeyDown(name) = event
                && name == OPEN_KEY
            {
                self.open_list();
            }
        }
    }
}

impl Mod for ModMenu {
    fn init() -> Result<(Self, Registration), String> {
        let menu = ModMenu {
            title_button: read_switch(TITLE_KEY, true),
            pause_button: read_switch(PAUSE_KEY, true),
        };
        input::register_key(OPEN_KEY, "Open Mod Menu", OPEN_KEY_DEFAULT)?;
        menu.declare();
        Ok((menu, Registration::new().on_tick().on_frame()))
    }

    fn tick(&mut self, _tick: u64) {
        self.drain();
    }

    fn frame(&mut self, _seconds: f32) {
        self.drain();
    }

    fn ui_settings_opened(&mut self) {
        ui::set_settings(&settings_widgets(self.title_button, self.pause_button));
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            TITLE_SETTING => {
                self.title_button = !self.title_button;
                store_switch(TITLE_KEY, self.title_button);
                self.declare();
                ui::set_settings(&settings_widgets(self.title_button, self.pause_button));
            }
            PAUSE_SETTING => {
                self.pause_button = !self.pause_button;
                store_switch(PAUSE_KEY, self.pause_button);
                self.declare();
                ui::set_settings(&settings_widgets(self.title_button, self.pause_button));
            }
            LIST_WIDGET => {
                ui::open_screen(&ui::Screen {
                    title: OWN_NAME.to_string(),
                    widgets: detail_widgets(),
                });
            }
            BACK_BUTTON => self.open_list(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(ModMenu);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_22() {
        assert_eq!(API_LEVEL, 22);
    }

    #[test]
    fn buttons_follow_the_switches() {
        let both = buttons(true, true);
        assert_eq!(both.len(), 2);
        assert_eq!(both[0].id, TITLE_BUTTON);
        assert_eq!(both[0].screen, "title");
        assert_eq!(both[0].anchor, Anchor::TitleRow);
        assert_eq!(both[0].label, "Mods");
        assert_eq!(both[0].menu, None);
        assert_eq!(both[0].icon, None);
        assert_eq!(both[1].id, PAUSE_BUTTON);
        assert_eq!(both[1].screen, "paused");
        assert_eq!(both[1].anchor, Anchor::MenuColumn);
        assert_eq!(buttons(true, false).len(), 1);
        assert_eq!(buttons(true, false)[0].id, TITLE_BUTTON);
        assert_eq!(buttons(false, true).len(), 1);
        assert_eq!(buttons(false, true)[0].id, PAUSE_BUTTON);
        assert!(buttons(false, false).is_empty());
    }

    #[test]
    fn the_list_names_only_what_it_knows() {
        let widgets = list_widgets();
        assert_eq!(widgets.len(), 2);
        // The gap is stated on the screen's face, not hidden.
        match &widgets[0] {
            Widget::Text(text) => assert!(text.text.contains("only itself")),
            _ => panic!("the first widget states the gap"),
        }
        match &widgets[1] {
            Widget::List(list) => {
                assert_eq!(list.id, LIST_WIDGET);
                assert_eq!(list.rows, [OWN_ROW]);
            }
            _ => panic!("the second widget is the list"),
        }
    }

    #[test]
    fn the_details_are_its_own() {
        let widgets = detail_widgets();
        let texts: Vec<&str> = widgets
            .iter()
            .filter_map(|w| match w {
                Widget::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, [OWN_NAME, OWN_SUMMARY, OWN_CREDITS, OWN_LICENCE]);
        assert!(texts.iter().any(|t| t.contains("MIT")));
        match widgets.last() {
            Some(Widget::Button(b)) => {
                assert_eq!(b.id, BACK_BUTTON);
                assert_eq!(b.label, "Back");
            }
            _ => panic!("the details end with a back button"),
        }
    }

    #[test]
    fn settings_labels_name_the_switches() {
        let labels: Vec<String> = settings_widgets(true, false)
            .iter()
            .map(|w| match w {
                Widget::Button(b) => b.label.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(
            labels,
            ["Title Screen Button: Shown", "Pause Menu Button: Hidden"]
        );
    }
}

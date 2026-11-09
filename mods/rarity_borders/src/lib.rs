//! Rarity Borders for Kestrel: clean, vanilla-style coloured
//! borders on inventory items by rarity.
//!
//! Behaviour reimplemented from Rarity Borders by Xariami
//! (maliferen), MIT,
//! <https://github.com/maliferen/horizon-rarity-borders>: this
//! file shares no code with the original — the original is a
//! native Horizon mod, this is a fresh implementation against
//! the Kestrel SDK. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/rarity_borders>.
//!
//! Fidelity, against the original's registry description:
//! borders for Uncommon, Rare and Epic items in the vanilla
//! rarity colours, as a clean square outline with a highlighted
//! bottom edge, on inventory and container slots; borders for
//! Common items and for the hotbar are the original's optional
//! features, off until turned on; the choices are settings, and
//! they are kept between sessions (the original's 0.1.1 fixed
//! exactly that).
//!
//! The one thing the port cannot do is the original's first
//! step — reading a slot's rarity. A stack's rarity usually
//! lives in its item's *default* components, and the SDK's
//! `inventory` read spells only the components beyond the
//! defaults, so most stacks show no rarity to a mod at all.
//! API 23's answer, designed for this mod, is the host-matched
//! stack decoration: the port declares one border per rarity
//! keyed on a `minecraft:rarity` component match, and the game
//! evaluates it against each stack's merged components — the
//! resolver's own set, defaults included. The border's look is
//! the game's own border hand, as every sandboxed mark is; the
//! mod chooses the colours, the places and the tenancy, and
//! never learns any slot's rarity itself.

use horizon_mod_sdk::{
    ComponentMatch, Decoration, DecorationPlace, DecorationTarget, Mod, ModelMatch, Registration,
    item_model, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `item-model.set-decorations` is level 23
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 23;

const COMMON_KEY: &str = "common";
const HOTBAR_KEY: &str = "hotbar";

const COMMON_BUTTON: u32 = 0;
const HOTBAR_BUTTON: u32 = 1;

/// The rarities in order, with the colours vanilla names them
/// by (its rarity text colours): white, yellow, aqua, light
/// purple. Common is the optional one, first when it is on.
const RARITIES: [(&str, u32); 4] = [
    ("common", 0xFFFFFF),
    ("uncommon", 0xFFFF55),
    ("rare", 0x55FFFF),
    ("epic", 0xFF55FF),
];

/// The borders for the current settings: one host-matched
/// stack decoration per rarity (Common only when its option
/// is on), on screens, and on the hotbar when its option is
/// on. The match names no item — an empty item in a stack
/// match is every item — only the rarity component.
fn build_decorations(common: bool, hotbar: bool) -> Vec<Decoration> {
    let mut places = vec![DecorationPlace::Screens];
    if hotbar {
        places.push(DecorationPlace::Hotbar);
    }
    RARITIES
        .iter()
        .filter(|(rarity, _)| common || *rarity != "common")
        .map(|(rarity, colour)| Decoration {
            target: DecorationTarget::Stack(ModelMatch {
                item: String::new(),
                components: vec![ComponentMatch {
                    name: "minecraft:rarity".into(),
                    value: (*rarity).into(),
                }],
            }),
            places: places.clone(),
            fill: None,
            border: Some(*colour),
            star: false,
        })
        .collect()
}

/// A stored on/off setting; `default` when unreadable.
fn read_bool(key: &str, default: bool) -> bool {
    match storage::get(key) {
        Some(bytes) => match std::str::from_utf8(&bytes).ok().map(str::trim) {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        },
        None => default,
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn common_label(on: bool) -> String {
    format!("Common Items: {}", on_off(on))
}

fn hotbar_label(on: bool) -> String {
    format!("Hotbar: {}", on_off(on))
}

struct RarityBorders {
    common: bool,
    hotbar: bool,
}

impl RarityBorders {
    fn declare(&self) {
        item_model::set_decorations(&build_decorations(self.common, self.hotbar));
    }

    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: COMMON_BUTTON,
                label: common_label(self.common),
            }),
            Widget::Button(ButtonWidget {
                id: HOTBAR_BUTTON,
                label: hotbar_label(self.hotbar),
            }),
        ]);
    }
}

impl Mod for RarityBorders {
    fn init() -> Result<(Self, Registration), String> {
        let state = RarityBorders {
            common: read_bool(COMMON_KEY, false),
            hotbar: read_bool(HOTBAR_KEY, false),
        };
        state.declare();
        Ok((state, Registration::new()))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            COMMON_BUTTON => {
                self.common = !self.common;
                let _ = storage::set(COMMON_KEY, if self.common { b"true" } else { b"false" });
            }
            HOTBAR_BUTTON => {
                self.hotbar = !self.hotbar;
                let _ = storage::set(HOTBAR_KEY, if self.hotbar { b"true" } else { b"false" });
            }
            _ => return,
        }
        self.declare();
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(RarityBorders);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_23() {
        assert_eq!(API_LEVEL, 23);
    }

    /// A decoration as the tests name it: target, places,
    /// border — mirroring the runtime's spelling.
    fn spell(d: &Decoration) -> String {
        let DecorationTarget::Stack(m) = &d.target else {
            panic!("a slot target");
        };
        let item = if m.item.is_empty() { "*" } else { &m.item };
        let places: Vec<&str> = d
            .places
            .iter()
            .map(|p| match p {
                DecorationPlace::Hotbar => "hotbar",
                DecorationPlace::Screens => "screens",
            })
            .collect();
        format!(
            "stack {item} {}={} @ {} border {:06x}",
            m.components[0].name,
            m.components[0].value,
            places.join(","),
            d.border.unwrap()
        )
    }

    #[test]
    fn defaults_border_the_three_rarities_on_screens() {
        let decorations = build_decorations(false, false);
        let spelled: Vec<String> = decorations.iter().map(spell).collect();
        assert_eq!(
            spelled,
            [
                "stack * minecraft:rarity=uncommon @ screens border ffff55",
                "stack * minecraft:rarity=rare @ screens border 55ffff",
                "stack * minecraft:rarity=epic @ screens border ff55ff",
            ]
        );
        // Borders only: no fill, no star.
        assert!(
            decorations
                .iter()
                .all(|d| d.fill.is_none() && !d.star && d.border.is_some())
        );
    }

    #[test]
    fn the_options_add_common_and_the_hotbar() {
        let decorations = build_decorations(true, true);
        let spelled: Vec<String> = decorations.iter().map(spell).collect();
        assert_eq!(spelled.len(), 4);
        assert_eq!(
            spelled[0],
            "stack * minecraft:rarity=common @ screens,hotbar border ffffff"
        );
        assert!(
            spelled[1..]
                .iter()
                .all(|s| s.contains("@ screens,hotbar"))
        );
    }

    #[test]
    fn settings_labels_name_the_options() {
        assert_eq!(common_label(false), "Common Items: Off");
        assert_eq!(common_label(true), "Common Items: On");
        assert_eq!(hotbar_label(false), "Hotbar: Off");
        assert_eq!(hotbar_label(true), "Hotbar: On");
    }
}

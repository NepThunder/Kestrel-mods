//! Fast Items for Kestrel: dropped items lie flat and
//! camera-facing, so the game draws less of them.
//!
//! Behaviour reimplemented from Fast Items for Horizon by Lyvt,
//! MIT, <https://github.com/Lyvt-Dev/FastItems-for-Horizon>,
//! itself inspired by Noryea's Fast Items and independently
//! written, as this is: a fresh implementation against the
//! Kestrel SDK. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/fast_items>.
//!
//! Fidelity, against the original's registry description: flat
//! dropped items (potions, tools...) draw as one flat face
//! turned to the player — the sprite's edges and hidden back
//! face left undrawn, multi-layer textures, tint, lighting and
//! glint intact — and dropped block items keep their 3D model
//! and face the camera instead of spinning, as in Lunar Client
//! and Badlion. Bobbing, stack clusters, the pickup animation,
//! culling and glow outlines are the engine's and stay, as the
//! original promises of itself. Turning off dropped items'
//! blob shadows is its option, here a setting. Only item
//! entities change: inventory icons, held items, item frames
//! and servers stay as they are — the original says the same
//! of itself.
//!
//! The whole port is one record: API 23's
//! `item-model.set-dropped-display` (under `render`) was
//! designed as this mod's surface, so the settings map onto it
//! field for field and are re-declared when they change. The
//! original reads `config/fast-items.json` and asks for a
//! restart after editing it; the port's settings live on the
//! standard settings page and apply at once — the compromise
//! every port here makes, named in the district's design note.

use horizon_mod_sdk::{
    BlockDisplay, DroppedDisplay, FlatDisplay, Mod, Registration, item_model, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests
/// below: `item-model.set-dropped-display` is level 23
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 23;

const FLAT_KEY: &str = "flat";
const BLOCKS_KEY: &str = "blocks";
const SHADOWS_KEY: &str = "shadows";

const FLAT_BUTTON: u32 = 0;
const BLOCKS_BUTTON: u32 = 1;
const SHADOWS_BUTTON: u32 = 2;

/// The settings as stored indices: flat is Sprite (0) or
/// Vanilla (1); blocks is Billboard (0) or Vanilla (1). The
/// defaults are the original's described behaviour.
fn flat_display(index: usize) -> FlatDisplay {
    if index == 1 {
        FlatDisplay::Vanilla
    } else {
        FlatDisplay::Sprite
    }
}

fn block_display(index: usize) -> BlockDisplay {
    if index == 1 {
        BlockDisplay::Vanilla
    } else {
        BlockDisplay::Billboard
    }
}

/// The dropped-display record the settings name.
fn display(flat: usize, blocks: usize, shadows: bool) -> DroppedDisplay {
    DroppedDisplay {
        flat: flat_display(flat),
        blocks: block_display(blocks),
        shadows,
    }
}

/// A stored index in {0, 1}; 0 when unreadable.
fn read_index(key: &str) -> usize {
    let Some(bytes) = storage::get(key) else {
        return 0;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return 0;
    };
    match text.trim().parse::<usize>() {
        Ok(v) if v <= 1 => v,
        _ => 0,
    }
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

fn flat_label(index: usize) -> String {
    format!("Flat Items: {}", ["Sprite", "Vanilla"][index.min(1)])
}

fn blocks_label(index: usize) -> String {
    format!("Block Items: {}", ["Billboard", "Vanilla"][index.min(1)])
}

fn shadows_label(on: bool) -> String {
    format!("Dropped Shadows: {}", on_off(on))
}

struct FastItems {
    flat: usize,
    blocks: usize,
    shadows: bool,
}

impl FastItems {
    fn apply(&self) {
        item_model::set_dropped_display(display(self.flat, self.blocks, self.shadows));
    }

    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: FLAT_BUTTON,
                label: flat_label(self.flat),
            }),
            Widget::Button(ButtonWidget {
                id: BLOCKS_BUTTON,
                label: blocks_label(self.blocks),
            }),
            Widget::Button(ButtonWidget {
                id: SHADOWS_BUTTON,
                label: shadows_label(self.shadows),
            }),
        ]);
    }
}

impl Mod for FastItems {
    fn init() -> Result<(Self, Registration), String> {
        let state = FastItems {
            flat: read_index(FLAT_KEY),
            blocks: read_index(BLOCKS_KEY),
            shadows: read_bool(SHADOWS_KEY, true),
        };
        state.apply();
        Ok((state, Registration::new()))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            FLAT_BUTTON => {
                self.flat = (self.flat + 1) % 2;
                let _ = storage::set(FLAT_KEY, self.flat.to_string().as_bytes());
            }
            BLOCKS_BUTTON => {
                self.blocks = (self.blocks + 1) % 2;
                let _ = storage::set(BLOCKS_KEY, self.blocks.to_string().as_bytes());
            }
            SHADOWS_BUTTON => {
                self.shadows = !self.shadows;
                let _ = storage::set(SHADOWS_KEY, if self.shadows { b"true" } else { b"false" });
            }
            _ => return,
        }
        self.apply();
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(FastItems);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_23() {
        assert_eq!(API_LEVEL, 23);
    }

    #[test]
    fn the_defaults_are_the_originals_behaviour() {
        // Flat items as sprites, block items billboarded,
        // shadows on (turning them off is the option).
        let d = display(0, 0, true);
        assert!(matches!(d.flat, FlatDisplay::Sprite));
        assert!(matches!(d.blocks, BlockDisplay::Billboard));
        assert!(d.shadows);
    }

    #[test]
    fn each_setting_maps_to_its_field() {
        assert!(matches!(display(1, 0, true).flat, FlatDisplay::Vanilla));
        assert!(matches!(
            display(0, 1, true).blocks,
            BlockDisplay::Vanilla
        ));
        assert!(!display(0, 0, false).shadows);
        // Handing the display back whole is the all-vanilla
        // record with shadows on.
        let d = display(1, 1, true);
        assert!(matches!(d.flat, FlatDisplay::Vanilla));
        assert!(matches!(d.blocks, BlockDisplay::Vanilla));
        assert!(d.shadows);
    }

    #[test]
    fn settings_labels_name_the_choices() {
        assert_eq!(flat_label(0), "Flat Items: Sprite");
        assert_eq!(flat_label(1), "Flat Items: Vanilla");
        assert_eq!(blocks_label(0), "Block Items: Billboard");
        assert_eq!(blocks_label(1), "Block Items: Vanilla");
        assert_eq!(shadows_label(true), "Dropped Shadows: On");
        assert_eq!(shadows_label(false), "Dropped Shadows: Off");
    }
}

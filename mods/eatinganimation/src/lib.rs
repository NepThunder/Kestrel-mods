//! Eating Animation for Kestrel: while you eat food, or drink a
//! potion or milk, the item in your hand shows its bitten frames
//! as you go.
//!
//! Behaviour reimplemented from Eating Animation by Theoness1,
//! MIT, <https://github.com/Theoness1/EatingAnimation>, via
//! soufied's Horizon port: this file shares no code with either —
//! the original is a Fabric mod with a resource pack, this is a
//! fresh implementation against the Kestrel SDK. This port's home
//! is <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/eatinganimation>.
//!
//! The port is one package in the original's own split — its
//! Horizon port likewise carries its frames compiled in as
//! content: the code here registers one item-model rule per
//! covered item (`item-model.set-model-rules`, API 23, under the
//! `render` capability), and the frame definitions ship beside
//! it under `assets/eatinganimation/` in the same .kmod
//! (capability `registries`, so the manifest is api 0.2: a
//! package may carry content and code together, and the game
//! layers the content of every enabled package that declares
//! `registries`, component or not). A definition animates
//! itself: `condition(using_item)` -> `range_dispatch` on
//! `use_duration` over four frame models, falling back to the
//! item's vanilla model when not in use — the game's resolver
//! steps the frames, so no code runs per frame here and there
//! are no settings, as the original has none.
//!
//! Fidelity, against the original's registry description: the
//! covered set is the original's — every food, the potion (one
//! item; its variants are components), the milk bucket, the
//! honey and ominous bottles — and the frames show in the hand,
//! first and third person. The frame ART is this port's own:
//! the original draws bitten textures of its own, and none are
//! copied; here each frame is a flat model sampling a cropped
//! rectangle of the item's own vanilla texture (a bite out of
//! the top-right corner for food, the top emptied away for
//! drinks), authored by `generate_frames.py` in this folder.
//! Interfaces are not eaten in the hand and show no frames, as
//! in the original; the inventory (GUI) display never swaps.

use horizon_mod_sdk::{
    DisplayContext, Mod, ModelMatch, ModelRule, Registration, item_model,
};

/// The newest API level this mod needs, asserted in the tests
/// below: `item-model.set-model-rules` is level 23
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 23;

/// The covered items and each one's use duration in ticks —
/// vanilla's own numbers (foods 32, dried kelp 16, the honey
/// bottle 40): a definition's frame thresholds are fractions of
/// its item's duration, so the bites land at the same point of
/// every item's eat or drink. The frame assets are generated
/// from this same table by `generate_frames.py`; the tests pin
/// the two together.
const FOODS: [(&str, u32); 43] = [
    ("apple", 32),
    ("golden_apple", 32),
    ("enchanted_golden_apple", 32),
    ("melon_slice", 32),
    ("sweet_berries", 32),
    ("glow_berries", 32),
    ("chorus_fruit", 32),
    ("carrot", 32),
    ("golden_carrot", 32),
    ("potato", 32),
    ("baked_potato", 32),
    ("poisonous_potato", 32),
    ("beetroot", 32),
    ("dried_kelp", 16),
    ("beef", 32),
    ("cooked_beef", 32),
    ("porkchop", 32),
    ("cooked_porkchop", 32),
    ("mutton", 32),
    ("cooked_mutton", 32),
    ("chicken", 32),
    ("cooked_chicken", 32),
    ("rabbit", 32),
    ("cooked_rabbit", 32),
    ("cod", 32),
    ("cooked_cod", 32),
    ("salmon", 32),
    ("cooked_salmon", 32),
    ("tropical_fish", 32),
    ("pufferfish", 32),
    ("bread", 32),
    ("cookie", 32),
    ("pumpkin_pie", 32),
    ("rotten_flesh", 32),
    ("spider_eye", 32),
    ("mushroom_stew", 32),
    ("rabbit_stew", 32),
    ("beetroot_soup", 32),
    ("suspicious_stew", 32),
    ("milk_bucket", 32),
    ("honey_bottle", 40),
    ("ominous_bottle", 32),
    ("potion", 32),
];

/// The stage boundaries inside a use, as fractions of its
/// duration: frame 1 from the first tick, frames 2-4 from these.
/// `generate_frames.py` holds the same fractions; the tests pin
/// both against the generated definitions. (Test-only here: the
/// thresholds live in the shipped definitions, not in code.)
#[cfg(test)]
const STAGE_FRACTIONS: [f32; 3] = [0.35, 0.60, 0.82];

/// The elapsed ticks at which frames 2-4 begin for a use of
/// `duration` ticks.
#[cfg(test)]
fn thresholds(duration: u32) -> [u32; 3] {
    STAGE_FRACTIONS.map(|f| (f * duration as f32).round() as u32)
}

/// The rule for one covered item: in the hand (first and third
/// person) it displays as this package's own definition for it,
/// which does the stepping. An item the definitions do not
/// cover would simply display as itself.
fn rule(item: &str) -> ModelRule {
    ModelRule {
        match_: ModelMatch {
            item: format!("minecraft:{item}"),
            components: Vec::new(),
        },
        contexts: vec![DisplayContext::FirstPerson, DisplayContext::ThirdPerson],
        model: format!("eatinganimation:{item}"),
    }
}

/// Every rule the mod declares, in table order.
fn rules() -> Vec<ModelRule> {
    FOODS.iter().map(|(item, _)| rule(item)).collect()
}

struct EatingAnimation;

impl Mod for EatingAnimation {
    fn init() -> Result<(Self, Registration), String> {
        item_model::set_model_rules(&rules());
        Ok((EatingAnimation, Registration::new()))
    }
}

horizon_mod_sdk::export_mod!(EatingAnimation);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    #[test]
    fn api_level_is_23() {
        assert_eq!(API_LEVEL, 23);
    }

    #[test]
    fn the_table_is_the_covered_set() {
        assert_eq!(FOODS.len(), 43);
        let ids: BTreeSet<&str> = FOODS.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), FOODS.len(), "an item is listed twice");
        // The drinks the registry names, and the potion as one
        // item (its variants are components, not items).
        for want in ["potion", "milk_bucket", "honey_bottle", "ominous_bottle"] {
            assert!(ids.contains(want), "missing {want}");
        }
        // Durations are vanilla's: only kelp eats fast and only
        // the honey bottle drinks slow.
        for (id, duration) in FOODS {
            let want = match id {
                "dried_kelp" => 16,
                "honey_bottle" => 40,
                _ => 32,
            };
            assert_eq!(duration, want, "{id}");
        }
    }

    #[test]
    fn thresholds_are_fractions_of_the_duration() {
        assert_eq!(thresholds(32), [11, 19, 26]);
        assert_eq!(thresholds(16), [6, 10, 13]);
        assert_eq!(thresholds(40), [14, 24, 33]);
    }

    #[test]
    fn rules_cover_each_item_once_in_the_hand_only() {
        let rules = rules();
        assert_eq!(rules.len(), FOODS.len());
        assert!(rules.len() <= 64, "the surface caps rules at 64");
        for (rule, (item, _)) in rules.iter().zip(FOODS.iter()) {
            assert_eq!(rule.match_.item, format!("minecraft:{item}"));
            assert!(rule.match_.components.is_empty());
            assert_eq!(rule.model, format!("eatinganimation:{item}"));
            assert_eq!(
                rule.contexts,
                vec![DisplayContext::FirstPerson, DisplayContext::ThirdPerson]
            );
        }
    }

    /// The generated assets on disk, beside this crate: one
    /// definition per table entry, stepping at the table's
    /// thresholds over four frame models that exist.
    #[test]
    fn assets_match_the_table() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/eatinganimation");
        for (item, duration) in FOODS {
            let text = std::fs::read_to_string(root.join(format!("items/{item}.json")))
                .unwrap_or_else(|e| panic!("items/{item}.json: {e}"));
            assert!(text.contains("\"minecraft:using_item\""), "{item}");
            assert!(text.contains("\"minecraft:use_duration\""), "{item}");
            for (threshold, stage) in thresholds(duration).iter().zip(2..=4) {
                assert!(
                    text.contains(&format!("\"threshold\": {threshold}")),
                    "{item}: no threshold {threshold} in {text}"
                );
                assert!(
                    text.contains(&format!("eatinganimation:item/{item}_{stage}")),
                    "{item}: no stage {stage}"
                );
            }
            assert!(
                text.contains(&format!("\"model\": \"minecraft:item/{item}\"")),
                "{item}: the not-using fallback is not the item itself"
            );
            for stage in 1..=4 {
                let model = root.join(format!("models/item/{item}_{stage}.json"));
                assert!(model.is_file(), "{}", model.display());
            }
        }
        // The two special textures: the enchanted golden apple
        // wears the golden apple's sprite, and the potion's
        // frames carry the liquid layer, tinted.
        let enchanted =
            std::fs::read_to_string(root.join("models/item/enchanted_golden_apple_1.json"))
                .unwrap();
        assert!(enchanted.contains("minecraft:item/golden_apple"));
        let potion = std::fs::read_to_string(root.join("models/item/potion_1.json")).unwrap();
        assert!(potion.contains("minecraft:item/potion_overlay"));
        assert!(potion.contains("\"tintindex\": 0"));
        let potion_def = std::fs::read_to_string(root.join("items/potion.json")).unwrap();
        assert!(potion_def.contains("\"minecraft:potion\""));
    }
}

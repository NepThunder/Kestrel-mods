//! Held Item Info, ported from AltMugen's Held Item Info (MIT, original
//! native implementation for Horizon 26.3; inspired by A5b84/held-item-info,
//! no LGPL source was read or included): one text line for the selected
//! item — its remaining/maximum durability, its stack count when it stacks,
//! and up to six enchantment names — shown while the held item has anything
//! to say, and nothing when it doesn't (a single undamageable, unenchanted
//! item). `/helditem` hides or shows the line, and Options > Mods >
//! Held Item Info has the same switch; the game keeps the choice in the
//! mod's storage.
//!
//! The data comes from the read-only `%inventory` interface (API level 9):
//! the selected hotbar slot's damage and maximum damage, and its
//! `components` string in vanilla item syntax, which carries the
//! enchantments (`[enchantments={levels:{"minecraft:sharpness":5}}]`).
//! The SDK has no translation lookup, so enchantment names come from a
//! hardcoded table of the vanilla enchantments in English; an id the table
//! does not know falls back to its title-cased id, so a modded enchantment
//! still shows something.
//!
//! What the SDK's text HUD cannot do, the original's native HUD could:
//! draw the line centred above the hotbar, in the game's own language, and
//! shorten it to the exact GUI width. This port shows the line in the
//! bottom-left corner instead, in English, with the item's name in front
//! (the original sat under the game's own selected-item name, so it did not
//! need one), and shortens it only to the panel's 250-character limit.

use horizon_mod_sdk::{
    Mod, Registration,
    hud::{self, Corner, Line},
    inventory::{self, InvStack, PlayerInventory},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The original's text colour, light grey.
pub const COLOR_TEXT: u32 = 0xE0_E0E0;

/// How many enchantments the line shows, as in the original.
pub const MAX_ENCHANTMENTS: usize = 6;

/// The panel line's character limit: the HUD's lines hold 256 characters,
/// so the line is shortened a little short of that, with an ellipsis.
pub const MAX_LINE_CHARS: usize = 250;

/// The storage key for whether the line shows.
const SHOWN_KEY: &str = "shown";
/// The settings page's button: show or hide the line.
const SHOWN_BUTTON: u32 = 0;

/// The vanilla enchantments' English names, by their registry id without
/// the `minecraft:` prefix. The two curses are listed under both their
/// registry ids and their long names, in case a components string uses
/// either.
pub fn enchantment_name(id: &str) -> String {
    let name = match id {
        "aqua_affinity" => "Aqua Affinity",
        "bane_of_arthropods" => "Bane of Arthropods",
        "binding_curse" | "curse_of_binding" => "Curse of Binding",
        "blast_protection" => "Blast Protection",
        "breach" => "Breach",
        "channeling" => "Channeling",
        "density" => "Density",
        "depth_strider" => "Depth Strider",
        "efficiency" => "Efficiency",
        "feather_falling" => "Feather Falling",
        "fire_aspect" => "Fire Aspect",
        "fire_protection" => "Fire Protection",
        "flame" => "Flame",
        "fortune" => "Fortune",
        "frost_walker" => "Frost Walker",
        "impaling" => "Impaling",
        "infinity" => "Infinity",
        "knockback" => "Knockback",
        "looting" => "Looting",
        "loyalty" => "Loyalty",
        "luck_of_the_sea" => "Luck of the Sea",
        "lure" => "Lure",
        "mending" => "Mending",
        "multishot" => "Multishot",
        "piercing" => "Piercing",
        "power" => "Power",
        "projectile_protection" => "Projectile Protection",
        "protection" => "Protection",
        "punch" => "Punch",
        "quick_charge" => "Quick Charge",
        "respiration" => "Respiration",
        "riptide" => "Riptide",
        "sharpness" => "Sharpness",
        "silk_touch" => "Silk Touch",
        "smite" => "Smite",
        "soul_speed" => "Soul Speed",
        "sweeping_edge" => "Sweeping Edge",
        "swift_sneak" => "Swift Sneak",
        "thorns" => "Thorns",
        "unbreaking" => "Unbreaking",
        "vanishing_curse" | "curse_of_vanishing" => "Curse of Vanishing",
        "wind_burst" => "Wind Burst",
        _ => return display_name(id),
    };
    name.to_string()
}

/// `minecraft:diamond_sword` → `Diamond Sword`: the name the HUD line
/// shows, since the text panel has no item icon to speak for the stack.
pub fn display_name(id: &str) -> String {
    let path = id.split(':').next_back().unwrap_or(id);
    let mut name = String::new();
    for (i, word) in path.split('_').enumerate() {
        if i > 0 {
            name.push(' ');
        }
        let mut chars = word.chars();
        match chars.next() {
            Some(first) => {
                name.extend(first.to_uppercase());
                name.push_str(chars.as_str());
            }
            None => name.push(' '),
        }
    }
    name
}

/// The enchantments in a stack's `components` string, in the order they
/// appear: `(id without the namespace, level)`. Reads the
/// `enchantments={levels:{...}}` part in vanilla item syntax, quoted
/// (`{"minecraft:sharpness":5}`) or unquoted (`{minecraft:sharpness:5}`),
/// and ignores everything else in the string.
pub fn parse_enchantments(components: &str) -> Vec<(String, u32)> {
    let Some(start) = components.find("enchantments") else {
        return Vec::new();
    };
    let rest = &components[start..];
    let Some(open) = rest.find('{') else {
        return Vec::new();
    };
    // The enchantments object ends at the brace matching its opening one.
    let mut depth = 0i32;
    let mut end = rest.len();
    for (i, c) in rest[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &rest[open..end];
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        // Find a `minecraft:<id>` at this position, quoted or not.
        if body[i..].starts_with("\"minecraft:") || body[i..].starts_with("minecraft:") {
            let quoted = body[i..].starts_with('"');
            // `"minecraft:` is 11 characters, `minecraft:` is 10.
            let id_start = i + if quoted { 11 } else { 10 };
            let mut id_end = id_start;
            while id_end < bytes.len()
                && (bytes[id_end].is_ascii_lowercase()
                    || bytes[id_end].is_ascii_digit()
                    || bytes[id_end] == b'_')
            {
                id_end += 1;
            }
            if id_end > id_start {
                let mut j = id_end;
                if quoted && j < bytes.len() && bytes[j] == b'"' {
                    j += 1;
                }
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b':' {
                    j += 1;
                    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    let level_start = j;
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j > level_start {
                        let id = &body[id_start..id_end];
                        let level: u32 = body[level_start..j].parse().unwrap_or(0);
                        out.push((id.to_string(), level));
                        i = j;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

/// The line for a held stack: its name, then its durability, count and
/// enchantments, joined with ` · ` as in the original. `None` when the
/// stack has nothing to say — a single item that doesn't wear out and
/// carries no enchantments — so the panel stays empty.
pub fn line_for(stack: &InvStack) -> Option<Line> {
    let mut details: Vec<String> = Vec::new();
    if stack.max_damage > 0 {
        let remaining = stack.max_damage.saturating_sub(stack.damage);
        details.push(format!("{remaining} / {}", stack.max_damage));
    }
    if stack.count > 1 {
        details.push(format!("×{}", stack.count));
    }
    for (id, level) in parse_enchantments(&stack.components)
        .iter()
        .take(MAX_ENCHANTMENTS)
    {
        details.push(format!("{} {level}", enchantment_name(id)));
    }
    if details.is_empty() {
        return None;
    }
    let text = format!("{} · {}", display_name(&stack.id), details.join(" · "));
    Some(Line {
        text: shorten(&text),
        color: COLOR_TEXT,
    })
}

/// Shortens a line to [`MAX_LINE_CHARS`] characters, with an ellipsis, as
/// the original shortened its text to the GUI width.
pub fn shorten(text: &str) -> String {
    if text.chars().count() <= MAX_LINE_CHARS {
        return text.to_string();
    }
    let mut short: String = text.chars().take(MAX_LINE_CHARS - 1).collect();
    short.push('…');
    short
}

/// The panel's line for an inventory snapshot: the selected hotbar slot's
/// stack, if it has anything to say. Other slots never show.
pub fn panel_line(inv: &PlayerInventory) -> Option<Line> {
    let stack = inv.slots.iter().find(|s| s.slot == inv.selected)?;
    line_for(stack)
}

/// The stored `shown` value: shown unless it says `false`.
fn read_shown(stored: Option<Vec<u8>>) -> bool {
    stored.is_none_or(|v| v != b"false")
}

fn shown_label(shown: bool) -> String {
    format!("Shown: {}", if shown { "ON" } else { "OFF" })
}

struct HeldItemInfo {
    shown: bool,
    /// The panel line last sent, so an unchanged one costs no call.
    last: Option<(String, u32)>,
}

impl HeldItemInfo {
    /// Hides or shows the line, keeps the choice in storage, and refreshes
    /// the settings page when it is open.
    fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
        let _ = storage::set(SHOWN_KEY, if shown { b"true" } else { b"false" });
        self.refresh_settings();
    }

    /// The settings page: one button, the line shown or hidden.
    fn refresh_settings(&self) {
        ui::set_settings(&[Widget::Button(ButtonWidget {
            id: SHOWN_BUTTON,
            label: shown_label(self.shown),
        })]);
    }
}

impl Mod for HeldItemInfo {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("helditem").on_tick();
        let state = HeldItemInfo {
            shown: read_shown(storage::get(SHOWN_KEY)),
            last: None,
        };
        Ok((state, hooks))
    }

    fn tick(&mut self, _tick: u64) {
        if !self.shown {
            if self.last.is_some() {
                hud::clear();
                self.last = None;
            }
            return;
        }
        let inv = inventory::inventory();
        match panel_line(&inv) {
            Some(line) => {
                let key = (line.text.clone(), line.color);
                if self.last.as_ref() != Some(&key) {
                    hud::set_panel(Corner::BottomLeft, std::slice::from_ref(&line));
                    self.last = Some(key);
                }
            }
            None => {
                if self.last.is_some() {
                    hud::clear();
                    self.last = None;
                }
            }
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        match name {
            "helditem" => {
                self.set_shown(!self.shown);
                let state = if self.shown { "shown" } else { "hidden" };
                Ok(Some(format!("Held Item Info panel {state}")))
            }
            _ => Err("This command does nothing".into()),
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        if id == SHOWN_BUTTON {
            self.set_shown(!self.shown);
            // A settings change redraws the panel on the next tick; clear
            // the remembered line so the change is not skipped as
            // unchanged.
            self.last = None;
        }
    }
}

horizon_mod_sdk::export_mod!(HeldItemInfo);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::inventory::InvStack;

    fn stack(id: &str, count: u32, damage: u32, max_damage: u32, components: &str) -> InvStack {
        InvStack {
            slot: 0,
            id: id.to_string(),
            count,
            damage,
            max_damage,
            components: components.to_string(),
        }
    }

    fn inventory(selected: u32, slots: Vec<InvStack>) -> PlayerInventory {
        PlayerInventory {
            selected,
            saturation: 5.0,
            slots,
            carried: None,
        }
    }

    #[test]
    fn display_names_come_from_item_ids() {
        assert_eq!(display_name("minecraft:diamond_sword"), "Diamond Sword");
        assert_eq!(display_name("minecraft:cooked_beef"), "Cooked Beef");
        assert_eq!(display_name("minecraft:torch"), "Torch");
        assert_eq!(display_name("minecraft:enchanted_book"), "Enchanted Book");
    }

    #[test]
    fn enchantment_names_cover_the_vanilla_set() {
        assert_eq!(enchantment_name("sharpness"), "Sharpness");
        assert_eq!(enchantment_name("unbreaking"), "Unbreaking");
        assert_eq!(enchantment_name("binding_curse"), "Curse of Binding");
        assert_eq!(enchantment_name("vanishing_curse"), "Curse of Vanishing");
        assert_eq!(enchantment_name("wind_burst"), "Wind Burst");
        assert_eq!(enchantment_name("density"), "Density");
        assert_eq!(enchantment_name("breach"), "Breach");
        assert_eq!(enchantment_name("mending"), "Mending");
        assert_eq!(enchantment_name("swift_sneak"), "Swift Sneak");
    }

    #[test]
    fn an_unknown_enchantment_falls_back_to_its_id() {
        assert_eq!(enchantment_name("soulbound"), "Soulbound");
        assert_eq!(enchantment_name("arcane_shield"), "Arcane Shield");
    }

    #[test]
    fn enchantments_parse_from_the_vanilla_components_string() {
        let parsed = parse_enchantments(
            "[enchantments={levels:{\"minecraft:sharpness\":5,\"minecraft:unbreaking\":3}}]",
        );
        assert_eq!(
            parsed,
            vec![("sharpness".to_string(), 5), ("unbreaking".to_string(), 3),]
        );
    }

    #[test]
    fn enchantments_parse_unquoted_and_ignore_other_components() {
        let parsed = parse_enchantments(
            "[minecraft:enchantments={levels:{minecraft:fire_aspect:2}},minecraft:custom_name='\"Hot\"']",
        );
        assert_eq!(parsed, vec![("fire_aspect".to_string(), 2)]);
    }

    #[test]
    fn enchantments_parse_nothing_without_enchantments() {
        assert!(parse_enchantments("").is_empty());
        assert!(parse_enchantments("[minecraft:custom_name='\"Plain\"']").is_empty());
        assert!(parse_enchantments("[enchantments={}]").is_empty());
    }

    #[test]
    fn a_damaged_enchanted_sword_shows_everything_the_original_did() {
        let line = line_for(&stack(
            "minecraft:diamond_sword",
            1,
            61,
            1561,
            "[enchantments={levels:{\"minecraft:sharpness\":5,\"minecraft:unbreaking\":3}}]",
        ))
        .unwrap();
        assert_eq!(
            line.text,
            "Diamond Sword · 1500 / 1561 · Sharpness 5 · Unbreaking 3"
        );
        assert_eq!(line.color, COLOR_TEXT);
    }

    #[test]
    fn a_stack_shows_its_count_with_a_cross() {
        let line = line_for(&stack("minecraft:torch", 64, 0, 0, "")).unwrap();
        assert_eq!(line.text, "Torch · ×64");
    }

    #[test]
    fn an_undamaged_bow_shows_full_durability() {
        let line = line_for(&stack("minecraft:bow", 1, 0, 385, "")).unwrap();
        assert_eq!(line.text, "Bow · 385 / 385");
    }

    #[test]
    fn an_enchanted_book_shows_only_its_enchantments() {
        let line = line_for(&stack(
            "minecraft:enchanted_book",
            1,
            0,
            0,
            "[enchantments={levels:{\"minecraft:mending\":1}}]",
        ))
        .unwrap();
        assert_eq!(line.text, "Enchanted Book · Mending 1");
    }

    #[test]
    fn a_plain_single_item_shows_nothing() {
        assert!(line_for(&stack("minecraft:apple", 1, 0, 0, "")).is_none());
        assert!(line_for(&stack("minecraft:stick", 1, 0, 0, "")).is_none());
    }

    #[test]
    fn only_six_enchantments_show() {
        let components = "[enchantments={levels:{\"minecraft:sharpness\":5,\"minecraft:smite\":5,\"minecraft:bane_of_arthropods\":5,\"minecraft:knockback\":2,\"minecraft:fire_aspect\":2,\"minecraft:looting\":3,\"minecraft:unbreaking\":3}}]";
        let line = line_for(&stack("minecraft:diamond_sword", 1, 0, 1561, components)).unwrap();
        assert_eq!(
            line.text,
            "Diamond Sword · 1561 / 1561 · Sharpness 5 · Smite 5 · Bane of Arthropods 5 · Knockback 2 · Fire Aspect 2 · Looting 3"
        );
        assert!(!line.text.contains("Unbreaking"));
    }

    #[test]
    fn a_long_line_is_shortened_with_an_ellipsis() {
        let long = "x".repeat(300);
        let short = shorten(&long);
        assert_eq!(short.chars().count(), MAX_LINE_CHARS);
        assert!(short.ends_with('…'));
        assert_eq!(shorten("short"), "short");
    }

    #[test]
    fn only_the_selected_slot_shows() {
        // Slot 0 holds the sword, slot 1 the torches; neither is selected.
        let sword = InvStack {
            slot: 0,
            ..stack("minecraft:diamond_sword", 1, 61, 1561, "")
        };
        let torches = InvStack {
            slot: 1,
            ..stack("minecraft:torch", 64, 0, 0, "")
        };
        let inv = inventory(2, vec![sword.clone(), torches.clone()]);
        assert!(panel_line(&inv).is_none());
        let inv = inventory(0, vec![sword.clone(), torches.clone()]);
        assert_eq!(
            panel_line(&inv).unwrap().text,
            "Diamond Sword · 1500 / 1561"
        );
        let inv = inventory(1, vec![sword, torches]);
        assert_eq!(panel_line(&inv).unwrap().text, "Torch · ×64");
    }

    #[test]
    fn an_empty_hand_shows_nothing() {
        let inv = inventory(0, vec![]);
        assert!(panel_line(&inv).is_none());
    }

    #[test]
    fn shown_unless_stored_false() {
        assert!(read_shown(None));
        assert!(read_shown(Some(b"true".to_vec())));
        assert!(!read_shown(Some(b"false".to_vec())));
        assert_eq!(shown_label(false), "Shown: OFF");
    }
}

//! Armor Status, ported from CayMac97's Armor Status (MIT, Java
//! ArmorStatusHUD-style): the four equipped armour pieces plus the main hand
//! and the off hand on the HUD, with Java Edition durability colours — green
//! above 50%, yellow from 25%, orange from 10%, red below 10% — and stack
//! counts for held items that don't wear out (food, blocks, torches).
//!
//! The data comes from the read-only `%inventory` interface (API level 9):
//! armour slots 500-503, offhand 504, and the selected hotbar slot for the
//! main hand, each with its damage and maximum damage. `/armor` hides or
//! shows the panel, and Options > Mods > Armor Status cycles the position,
//! the durability format, which hands show, and whether stack counts show.
//! The game keeps those choices in the mod's storage.
//!
//! What the SDK's text HUD cannot do, the original's native HUD could: draw
//! the item icons beside the numbers, sit beside the hotbar, or draw a
//! translucent background box behind the rows. This port shows one text line
//! per item — its name and its durability or count, in the Java colour — in
//! one of the four screen corners instead.
use horizon_mod_sdk::{
    Mod, Registration,
    hud::{self, Corner, Line},
    inventory::{self, InvStack, PlayerInventory},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// Original Java ArmorStatusHUD colour constants.
pub const COLOR_GREEN: u32 = 0x55_FF55; // > 50% durability
pub const COLOR_YELLOW: u32 = 0xFF_FF55; // 25% - 50% durability
pub const COLOR_ORANGE: u32 = 0xFF_AA00; // 10% - 25% durability
pub const COLOR_RED: u32 = 0xFF_5555; // < 10% durability
pub const COLOR_COUNT: u32 = 0xFF_FFFF; // Non-damageable item stack count

/// Corners the SDK's HUD panel supports, in the original's position order.
pub const POSITIONS: [&str; 4] = ["Top left", "Top right", "Bottom left", "Bottom right"];

/// Durability display formats, as in the original.
pub const FORMATS: [&str; 3] = ["Remaining (1520)", "Percent (97%)", "Fraction (1520/1561)"];

/// Which hand items show, as in the original.
pub const HANDS: [&str; 4] = [
    "Main & off hand",
    "Main hand only",
    "Off hand only",
    "Off (Armor only)",
];

/// Armour slots in the `%inventory` numbering: head, chest, legs, feet.
const ARMOUR_SLOTS: [u32; 4] = [500, 501, 502, 503];
/// The offhand slot in the `%inventory` numbering.
const OFFHAND_SLOT: u32 = 504;

/// Storage keys for the settings.
const SHOWN_KEY: &str = "shown";
const POSITION_KEY: &str = "position";
const FORMAT_KEY: &str = "format";
const HANDS_KEY: &str = "hands";
const SHOW_COUNT_KEY: &str = "show_count";

/// The settings page's buttons, in the order they are shown: position,
/// format, hands, show count, shown.
const SETTING_BUTTONS: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Index in `POSITIONS`. Default: 3 (Bottom right).
    pub position: usize,
    /// Index in `FORMATS`. Default: 0 (Remaining value).
    pub format: usize,
    /// Index in `HANDS`. Default: 0 (Both hands).
    pub hands: usize,
    /// Show stack counts for items without durability.
    pub show_count: bool,
    /// Show the panel at all (`/armor` and the settings page toggle this).
    pub shown: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            position: 3, // Bottom right, as in the original.
            format: 0,
            hands: 0,
            show_count: true,
            shown: true,
        }
    }
}

impl Settings {
    /// Reads the stored settings: an absent or unparseable value keeps its
    /// default, and an out-of-range index wraps, as in the original.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let index = |key: &str, len: usize, def: usize| {
            get(key)
                .and_then(|v| v.trim().parse::<usize>().ok())
                .map_or(def, |i| i % len)
        };
        let flag = |key: &str, def: bool| get(key).map_or(def, |v| v.trim() != "false");
        Self {
            position: index(POSITION_KEY, POSITIONS.len(), 3),
            format: index(FORMAT_KEY, FORMATS.len(), 0),
            hands: index(HANDS_KEY, HANDS.len(), 0),
            show_count: flag(SHOW_COUNT_KEY, true),
            shown: flag(SHOWN_KEY, true),
        }
    }

    pub fn labels(&self) -> Vec<String> {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("Position: {}", POSITIONS[self.position]),
            format!("Durability: {}", FORMATS[self.format]),
            format!("Hand items: {}", HANDS[self.hands]),
            format!("Show stack count: {}", on(self.show_count)),
            format!("Shown: {}", on(self.shown)),
        ]
    }

    /// Cycles the setting the button at `i` stands for, as in the original.
    pub fn click(&mut self, i: usize) {
        match i {
            0 => self.position = (self.position + 1) % POSITIONS.len(),
            1 => self.format = (self.format + 1) % FORMATS.len(),
            2 => self.hands = (self.hands + 1) % HANDS.len(),
            3 => self.show_count = !self.show_count,
            4 => self.shown = !self.shown,
            _ => {}
        }
    }

    pub fn stored(&self) -> [(&'static str, String); 5] {
        let flag = |b: bool| if b { "true" } else { "false" }.to_owned();
        [
            (POSITION_KEY, self.position.to_string()),
            (FORMAT_KEY, self.format.to_string()),
            (HANDS_KEY, self.hands.to_string()),
            (SHOW_COUNT_KEY, flag(self.show_count)),
            (SHOWN_KEY, flag(self.shown)),
        ]
    }

    fn corner(&self) -> Corner {
        match self.position {
            0 => Corner::TopLeft,
            1 => Corner::TopRight,
            2 => Corner::BottomLeft,
            _ => Corner::BottomRight,
        }
    }
}

/// The exact 1:1 Java ArmorStatusHUD colour for a durability ratio.
pub fn durability_color(ratio: f32) -> u32 {
    if !ratio.is_finite() || ratio < 0.10 {
        COLOR_RED
    } else if ratio < 0.25 {
        COLOR_ORANGE
    } else if ratio <= 0.50 {
        COLOR_YELLOW
    } else {
        COLOR_GREEN
    }
}

/// Formats the durability text in the selected format.
pub fn format_durability(remaining: u32, max_damage: u32, format: usize) -> String {
    match format {
        1 => {
            let pct = if max_damage > 0 {
                ((remaining as f32 / max_damage as f32) * 100.0).round() as u32
            } else {
                100
            };
            format!("{pct}%")
        }
        2 => format!("{remaining}/{max_damage}"),
        _ => remaining.to_string(),
    }
}

/// An item stack's display text and colour: its durability in the Java
/// colour when it wears out, else its stack count in white when it stacks
/// and counts are shown, else nothing to show beside it.
pub fn format_item(
    count: u32,
    damage: u32,
    max_damage: u32,
    format: usize,
    show_count: bool,
) -> Option<(String, u32)> {
    if max_damage > 0 {
        let remaining = max_damage.saturating_sub(damage);
        let ratio = remaining as f32 / max_damage as f32;
        let color = durability_color(ratio);
        let text = format_durability(remaining, max_damage, format);
        Some((text, color))
    } else if show_count && count > 1 {
        Some((format!("x{count}"), COLOR_COUNT))
    } else {
        None
    }
}

/// `minecraft:diamond_helmet` → `Diamond Helmet`: the name the HUD line
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

/// One HUD line for a stack: its name and its durability or count.
/// `None` when the stack shows nothing beside its name — a single item that
/// doesn't wear out — so the line is just its name.
fn line_for(stack: &InvStack, settings: &Settings) -> Line {
    let name = display_name(&stack.id);
    match format_item(
        stack.count,
        stack.damage,
        stack.max_damage,
        settings.format,
        settings.show_count,
    ) {
        Some((text, color)) => Line {
            text: format!("{name} {text}"),
            color,
        },
        None => Line {
            text: name,
            color: COLOR_COUNT,
        },
    }
}

/// The panel's lines for an inventory snapshot: armour head to feet, then
/// the hands the settings ask for. Empty slots show nothing, as in the
/// original.
pub fn panel_lines(inv: &PlayerInventory, settings: &Settings) -> Vec<Line> {
    let mut lines = Vec::with_capacity(6);
    for slot in ARMOUR_SLOTS {
        if let Some(stack) = inv.slots.iter().find(|s| s.slot == slot) {
            lines.push(line_for(stack, settings));
        }
    }
    let show_main = settings.hands == 0 || settings.hands == 1;
    let show_off = settings.hands == 0 || settings.hands == 2;
    if show_main && let Some(stack) = inv.slots.iter().find(|s| s.slot == inv.selected) {
        lines.push(line_for(stack, settings));
    }
    if show_off && let Some(stack) = inv.slots.iter().find(|s| s.slot == OFFHAND_SLOT) {
        lines.push(line_for(stack, settings));
    }
    lines
}

struct ArmorStatus {
    settings: Settings,
    /// The panel last sent, as its lines' text and colour — `Line` has no
    /// `PartialEq` — so an unchanged one costs no call.
    last: Vec<(String, u32)>,
}

impl ArmorStatus {
    /// Writes the settings to storage and refreshes the settings page.
    fn store_settings(&self) {
        for (key, value) in self.settings.stored() {
            let _ = storage::set(key, value.as_bytes());
        }
        self.refresh_settings();
    }

    /// The settings page: one button per setting, cycling its label.
    fn refresh_settings(&self) {
        let labels = self.settings.labels();
        let widgets: Vec<Widget> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                Widget::Button(ButtonWidget {
                    id: i as u32,
                    label: label.clone(),
                })
            })
            .collect();
        ui::set_settings(&widgets);
    }
}

impl Mod for ArmorStatus {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("armor").on_tick();
        let settings = Settings::read(|key| {
            storage::get(key).map(|v| String::from_utf8_lossy(&v).into_owned())
        });
        Ok((
            ArmorStatus {
                settings,
                last: Vec::new(),
            },
            hooks,
        ))
    }

    fn tick(&mut self, _tick: u64) {
        if !self.settings.shown {
            if !self.last.is_empty() {
                hud::clear();
                self.last.clear();
            }
            return;
        }
        let inv = inventory::inventory();
        let now = panel_lines(&inv, &self.settings);
        if now.is_empty() {
            if !self.last.is_empty() {
                hud::clear();
                self.last.clear();
            }
            return;
        }
        let now_key: Vec<(String, u32)> = now.iter().map(|l| (l.text.clone(), l.color)).collect();
        if now_key != self.last {
            hud::set_panel(self.settings.corner(), &now);
            self.last = now_key;
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        match name {
            "armor" => {
                self.settings.shown = !self.settings.shown;
                self.store_settings();
                let state = if self.settings.shown {
                    "shown"
                } else {
                    "hidden"
                };
                Ok(Some(format!("Armor Status panel {state}")))
            }
            _ => Err("This command does nothing".into()),
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        if id < SETTING_BUTTONS {
            self.settings.click(id as usize);
            self.store_settings();
            // A settings change redraws the panel on the next tick; clear the
            // remembered panel so the corner change is not skipped as
            // unchanged.
            self.last.clear();
        }
    }
}

horizon_mod_sdk::export_mod!(ArmorStatus);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::inventory::InvStack;

    fn stored<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |key| pairs.iter().find(|p| p.0 == key).map(|p| p.1.to_owned())
    }

    fn stack(slot: u32, id: &str, count: u32, damage: u32, max_damage: u32) -> InvStack {
        InvStack {
            slot,
            id: id.into(),
            count,
            damage,
            max_damage,
            components: String::new(),
        }
    }

    #[test]
    fn reads_and_writes_settings_correctly() {
        let defaults = Settings::read(stored(&[]));
        assert_eq!(defaults, Settings::default());
        assert_eq!(defaults.position, 3); // Bottom right

        let custom = Settings::read(stored(&[
            ("position", "1"),
            ("format", "2"),
            ("hands", "3"),
            ("show_count", "false"),
            ("shown", "false"),
        ]));
        assert_eq!(
            custom,
            Settings {
                position: 1,
                format: 2,
                hands: 3,
                show_count: false,
                shown: false,
            }
        );

        let pairs = custom.stored();
        let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(Settings::read(stored(&refs)), custom);
    }

    #[test]
    fn button_clicks_cycle_options() {
        let mut s = Settings::default();
        assert_eq!(s.position, 3); // Default is Bottom right
        s.click(0); // position cycles -> Top left (0)
        assert_eq!(s.position, 0);
        s.click(1); // format -> Percent
        assert_eq!(s.format, 1);
        s.click(2); // hands -> Main hand only
        assert_eq!(s.hands, 1);
        s.click(3); // show_count -> false
        assert!(!s.show_count);
        s.click(4); // shown -> false
        assert!(!s.shown);
    }

    #[test]
    fn original_java_color_thresholds_match_parity() {
        // > 50%: Green
        assert_eq!(durability_color(1.0), COLOR_GREEN);
        assert_eq!(durability_color(0.51), COLOR_GREEN);

        // 25% - 50%: Yellow
        assert_eq!(durability_color(0.50), COLOR_YELLOW);
        assert_eq!(durability_color(0.25), COLOR_YELLOW);

        // 10% - 25%: Orange
        assert_eq!(durability_color(0.24), COLOR_ORANGE);
        assert_eq!(durability_color(0.10), COLOR_ORANGE);

        // < 10%: Red
        assert_eq!(durability_color(0.09), COLOR_RED);
        assert_eq!(durability_color(0.01), COLOR_RED);
        assert_eq!(durability_color(0.00), COLOR_RED);
    }

    #[test]
    fn formats_durability_styles() {
        assert_eq!(format_durability(1500, 1561, 0), "1500");
        assert_eq!(format_durability(1500, 1561, 1), "96%");
        assert_eq!(format_durability(1500, 1561, 2), "1500/1561");
    }

    #[test]
    fn item_formatting_handles_durability_and_stacks() {
        // Diamond sword at 1500/1561 remaining.
        let sword = format_item(1, 61, 1561, 0, true);
        assert_eq!(sword, Some(("1500".into(), COLOR_GREEN)));

        // Critical diamond sword at 50/1561 remaining.
        let low_sword = format_item(1, 1511, 1561, 0, true);
        assert_eq!(low_sword, Some(("50".into(), COLOR_RED)));

        // Non-damageable stack (64 torches).
        let stack = format_item(64, 0, 0, 0, true);
        assert_eq!(stack, Some(("x64".into(), COLOR_COUNT)));

        // Single item without damage -> no text beside it.
        let single = format_item(1, 0, 0, 0, true);
        assert_eq!(single, None);
    }

    #[test]
    fn display_names_come_from_item_ids() {
        assert_eq!(display_name("minecraft:diamond_helmet"), "Diamond Helmet");
        assert_eq!(display_name("minecraft:cooked_beef"), "Cooked Beef");
        assert_eq!(display_name("minecraft:torch"), "Torch");
    }

    #[test]
    fn panel_lists_armour_then_hands_in_order() {
        let inv = PlayerInventory {
            selected: 0,
            saturation: 5.0,
            slots: vec![
                stack(0, "minecraft:diamond_sword", 1, 61, 1561),
                stack(500, "minecraft:diamond_helmet", 1, 10, 363),
                stack(503, "minecraft:diamond_boots", 1, 0, 429),
                stack(504, "minecraft:torch", 64, 0, 0),
            ],
            carried: None,
        };
        let lines = panel_lines(&inv, &Settings::default());
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Diamond Helmet 353",
                "Diamond Boots 429",
                "Diamond Sword 1500",
                "Torch x64",
            ]
        );
        assert_eq!(lines[0].color, COLOR_GREEN);
        assert_eq!(lines[3].color, COLOR_COUNT);

        // Armour only drops both hands.
        let armour_only = Settings {
            hands: 3,
            ..Settings::default()
        };
        let armour_lines = panel_lines(&inv, &armour_only);
        let texts: Vec<&str> = armour_lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["Diamond Helmet 353", "Diamond Boots 429"]);
    }

    #[test]
    fn empty_inventory_shows_no_lines() {
        let inv = PlayerInventory {
            selected: 0,
            saturation: 5.0,
            slots: vec![],
            carried: None,
        };
        assert!(panel_lines(&inv, &Settings::default()).is_empty());
    }
}

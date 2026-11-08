//! AppleSkin, ported from soufi's AppleSkin-Horizon (MIT, Copyright (c)
//! 2026 @soufied), itself a port of AppleSkin by squeek502 (MIT). This file
//! shares no code with either original; the behaviour is reimplemented
//! against the Kestrel SDK, and the food values below are vanilla 26.3's
//! (`minecraft:food` nutrition/saturation), checked against the Minecraft
//! Wiki's food table. Credit to soufied and squeek502 for the original mod.
//! This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/appleskin>.
//!
//! What the original draws over the food bar as icons, this port shows as
//! text lines in a HUD panel, because the SDK's `hud` panels are text only:
//! a saturation bar (`Saturation [######----] 12.4`), and, while the player
//! holds food, what eating it would restore and the totals after eating
//! (`Cooked Beef +8 hunger, +12.8 sat -> 20 hunger, 20 sat`). The food
//! level comes from `game-read`, the saturation and the held item from the
//! read-only `%inventory` interface (API level 9): saturation lives on the
//! `player-inventory` record, not on `game.player`.
//!
//! What the SDK cannot do, the original's native HUD could: draw the
//! saturation as food icons over the food bar, dim or pulse the preview, or
//! estimate the healing the meal would give. The exhaustion strip is left
//! out entirely: exhaustion is not on the client at all and cannot be
//! exposed, so there is no value to draw. `/appleskin` hides or shows the
//! panel, and Options > Mods > AppleSkin cycles the saturation line, the
//! held-food preview, and the position. The game keeps those choices in the
//! mod's storage.
use horizon_mod_sdk::{
    Mod, Registration, game,
    hud::{self, Corner, Line},
    inventory::{self, InvStack, PlayerInventory},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// Full hunger and full saturation, as vanilla counts them.
pub const MAX_FOOD: u32 = 20;
/// The saturation bar's length, in characters, as in Armor HUD.
const BAR_LEN: usize = 10;

/// The saturation line's colour: the gold of the original's icons.
pub const COLOR_SATURATION: u32 = 0xFF_FF55;
/// The held-food preview's colour.
pub const COLOR_PREVIEW: u32 = 0xFF_FFFF;
/// The "full hunger" note's colour.
pub const COLOR_DIM: u32 = 0xAA_AAAA;

/// Corners the SDK's HUD panel supports, in the original's position order.
pub const POSITIONS: [&str; 4] = ["Top left", "Top right", "Bottom left", "Bottom right"];

/// One vanilla food: its item id, the hunger it restores, the saturation it
/// restores, and whether it can be eaten at full hunger. Values are vanilla
/// 26.3 (`minecraft:food` nutrition/saturation, `can_always_eat`): chorus
/// fruit, golden apples, enchanted golden apples, honey bottles and
/// suspicious stew are the always-eatable ones.
pub const FOOD_TABLE: [(&str, u32, f32, bool); 39] = [
    ("minecraft:apple", 4, 2.4, false),
    ("minecraft:baked_potato", 5, 6.0, false),
    ("minecraft:beef", 3, 1.8, false),
    ("minecraft:beetroot", 1, 1.2, false),
    ("minecraft:beetroot_soup", 6, 7.2, false),
    ("minecraft:bread", 5, 6.0, false),
    ("minecraft:carrot", 3, 3.6, false),
    ("minecraft:chicken", 2, 1.2, false),
    ("minecraft:chorus_fruit", 4, 2.4, true),
    ("minecraft:cod", 2, 0.4, false),
    ("minecraft:cooked_beef", 8, 12.8, false),
    ("minecraft:cooked_chicken", 6, 7.2, false),
    ("minecraft:cooked_cod", 5, 6.0, false),
    ("minecraft:cooked_mutton", 6, 9.6, false),
    ("minecraft:cooked_porkchop", 8, 12.8, false),
    ("minecraft:cooked_rabbit", 5, 6.0, false),
    ("minecraft:cooked_salmon", 6, 9.6, false),
    ("minecraft:cookie", 2, 0.4, false),
    ("minecraft:dried_kelp", 1, 0.6, false),
    ("minecraft:enchanted_golden_apple", 4, 9.6, true),
    ("minecraft:glow_berries", 2, 1.2, false),
    ("minecraft:golden_apple", 4, 9.6, true),
    ("minecraft:golden_carrot", 6, 14.4, false),
    ("minecraft:honey_bottle", 6, 1.2, true),
    ("minecraft:melon_slice", 2, 1.2, false),
    ("minecraft:mushroom_stew", 6, 7.2, false),
    ("minecraft:mutton", 2, 1.2, false),
    ("minecraft:poisonous_potato", 2, 1.2, false),
    ("minecraft:porkchop", 3, 1.8, false),
    ("minecraft:potato", 1, 0.6, false),
    ("minecraft:pufferfish", 1, 0.2, false),
    ("minecraft:pumpkin_pie", 8, 4.8, false),
    ("minecraft:rabbit", 3, 1.8, false),
    ("minecraft:rabbit_stew", 10, 12.0, false),
    ("minecraft:rotten_flesh", 4, 0.8, false),
    ("minecraft:salmon", 2, 0.4, false),
    ("minecraft:spider_eye", 2, 3.2, false),
    ("minecraft:suspicious_stew", 6, 7.2, true),
    ("minecraft:tropical_fish", 1, 0.2, false),
];

/// A food's (hunger, saturation, can-always-eat) by item id; `None` when
/// the item is not food. The table is sorted by id, so this is a binary
/// search.
pub fn food_value(id: &str) -> Option<(u32, f32, bool)> {
    FOOD_TABLE
        .binary_search_by(|(item, _, _, _)| item.cmp(&id))
        .ok()
        .map(|i| {
            let (_, hunger, saturation, always) = FOOD_TABLE[i];
            (hunger, saturation, always)
        })
}

/// The food and saturation after eating, as the original computes them:
/// food clamps at 20, and saturation clamps at the new food level —
// vanilla never lets saturation exceed the hunger bar.
pub fn eaten(food: u32, saturation: f32, nutrition: u32, added: f32) -> (u32, f32) {
    let food = (food + nutrition).min(MAX_FOOD);
    (food, (saturation + added).clamp(0.0, food as f32))
}

/// A saturation value with no trailing `.0`: `6`, not `6.0`; `12.8` stays.
pub fn fmt_saturation(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as u32)
    } else {
        format!("{value:.1}")
    }
}

/// The saturation bar: ten `#`/`-` characters, filled in proportion, as
/// Armor HUD draws a durability bar.
pub fn saturation_bar(saturation: f32) -> String {
    let filled = ((saturation.clamp(0.0, MAX_FOOD as f32) / MAX_FOOD as f32) * BAR_LEN as f32)
        .round() as usize;
    let mut bar = String::with_capacity(BAR_LEN + 2);
    bar.push('[');
    bar.push_str(&"#".repeat(filled));
    bar.push_str(&"-".repeat(BAR_LEN - filled));
    bar.push(']');
    bar
}

/// `minecraft:cooked_beef` → `Cooked Beef`: the name the preview line
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

/// Storage keys for the settings.
const SHOW_SATURATION_KEY: &str = "show_saturation";
const SHOW_PREVIEW_KEY: &str = "show_preview";
const POSITION_KEY: &str = "position";
const SHOWN_KEY: &str = "shown";

/// The settings page's buttons, in the order they are shown: saturation,
/// preview, position, shown.
const SETTING_BUTTONS: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Show the saturation line. Default: true.
    pub show_saturation: bool,
    /// Show the held-food preview line. Default: true.
    pub show_preview: bool,
    /// Index in `POSITIONS`. Default: 3 (Bottom right).
    pub position: usize,
    /// Show the panel at all (`/appleskin` and the settings page toggle this).
    pub shown: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_saturation: true,
            show_preview: true,
            position: 3, // Bottom right, as in Armor Status.
            shown: true,
        }
    }
}

impl Settings {
    /// Reads the stored settings: an absent or unparseable value keeps its
    /// default, and an out-of-range index wraps, as in Armor Status.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let index = |key: &str, len: usize, def: usize| {
            get(key)
                .and_then(|v| v.trim().parse::<usize>().ok())
                .map_or(def, |i| i % len)
        };
        let flag = |key: &str, def: bool| get(key).map_or(def, |v| v.trim() != "false");
        Self {
            show_saturation: flag(SHOW_SATURATION_KEY, true),
            show_preview: flag(SHOW_PREVIEW_KEY, true),
            position: index(POSITION_KEY, POSITIONS.len(), 3),
            shown: flag(SHOWN_KEY, true),
        }
    }

    pub fn labels(&self) -> Vec<String> {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("Saturation: {}", on(self.show_saturation)),
            format!("Held-food preview: {}", on(self.show_preview)),
            format!("Position: {}", POSITIONS[self.position]),
            format!("Shown: {}", on(self.shown)),
        ]
    }

    /// Cycles the setting the button at `i` stands for, as in Armor Status.
    pub fn click(&mut self, i: usize) {
        match i {
            0 => self.show_saturation = !self.show_saturation,
            1 => self.show_preview = !self.show_preview,
            2 => self.position = (self.position + 1) % POSITIONS.len(),
            3 => self.shown = !self.shown,
            _ => {}
        }
    }

    pub fn stored(&self) -> [(&'static str, String); 4] {
        let flag = |b: bool| if b { "true" } else { "false" }.to_owned();
        [
            (SHOW_SATURATION_KEY, flag(self.show_saturation)),
            (SHOW_PREVIEW_KEY, flag(self.show_preview)),
            (POSITION_KEY, self.position.to_string()),
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

/// The held-food preview line for a stack: what eating it would restore
/// and the totals after eating, or a "full hunger" note when the player
/// cannot eat it yet. `None` when the stack is not food.
pub fn preview_line(stack: &InvStack, food: u32, saturation: f32) -> Option<Line> {
    let (nutrition, added, always) = food_value(&stack.id)?;
    let name = display_name(&stack.id);
    if food >= MAX_FOOD && !always {
        return Some(Line {
            text: format!("{name}: full hunger"),
            color: COLOR_DIM,
        });
    }
    let (new_food, new_saturation) = eaten(food, saturation, nutrition, added);
    Some(Line {
        text: format!(
            "{name} +{nutrition} hunger, +{} sat -> {new_food} hunger, {} sat",
            fmt_saturation(added),
            fmt_saturation(new_saturation),
        ),
        color: COLOR_PREVIEW,
    })
}

/// The panel's lines for a tick: the saturation line, then the held-food
/// preview when the selected slot holds food. Empty when the player is not
/// in a world or the panel is hidden.
pub fn panel_lines(food: u32, inv: &PlayerInventory, settings: &Settings) -> Vec<Line> {
    let mut lines = Vec::with_capacity(2);
    if settings.show_saturation {
        lines.push(Line {
            text: format!(
                "Saturation {} {}",
                saturation_bar(inv.saturation),
                fmt_saturation(inv.saturation.clamp(0.0, MAX_FOOD as f32)),
            ),
            color: COLOR_SATURATION,
        });
    }
    if settings.show_preview
        && let Some(stack) = inv.slots.iter().find(|s| s.slot == inv.selected)
        && let Some(line) = preview_line(stack, food, inv.saturation)
    {
        lines.push(line);
    }
    lines
}

struct AppleSkin {
    settings: Settings,
    /// The panel last sent, as its lines' text and colour — `Line` has no
    /// `PartialEq` — so an unchanged one costs no call.
    last: Vec<(String, u32)>,
}

impl AppleSkin {
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

impl Mod for AppleSkin {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("appleskin").on_tick();
        let settings = Settings::read(|key| {
            storage::get(key).map(|v| String::from_utf8_lossy(&v).into_owned())
        });
        Ok((
            AppleSkin {
                settings,
                last: Vec::new(),
            },
            hooks,
        ))
    }

    fn tick(&mut self, _tick: u64) {
        let Some(player) = game::get_player() else {
            if !self.last.is_empty() {
                hud::clear();
                self.last.clear();
            }
            return;
        };
        if !self.settings.shown {
            if !self.last.is_empty() {
                hud::clear();
                self.last.clear();
            }
            return;
        }
        let inv = inventory::inventory();
        let now = panel_lines(player.food, &inv, &self.settings);
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
            "appleskin" => {
                self.settings.shown = !self.settings.shown;
                self.store_settings();
                let state = if self.settings.shown {
                    "shown"
                } else {
                    "hidden"
                };
                Ok(Some(format!("AppleSkin panel {state}")))
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

horizon_mod_sdk::export_mod!(AppleSkin);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::inventory::InvStack;

    fn stored<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |key| pairs.iter().find(|p| p.0 == key).map(|p| p.1.to_owned())
    }

    fn stack(slot: u32, id: &str) -> InvStack {
        InvStack {
            slot,
            id: id.into(),
            count: 1,
            damage: 0,
            max_damage: 0,
            components: String::new(),
        }
    }

    fn inv(selected: u32, saturation: f32, slots: Vec<InvStack>) -> PlayerInventory {
        PlayerInventory {
            selected,
            saturation,
            slots,
            carried: None,
        }
    }

    #[test]
    fn food_table_is_sorted_for_binary_search() {
        let mut ids: Vec<&str> = FOOD_TABLE.iter().map(|(id, _, _, _)| *id).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            FOOD_TABLE
                .iter()
                .map(|(id, _, _, _)| *id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn food_values_match_vanilla() {
        // The best saturation in the game, and the biggest meal.
        assert_eq!(
            food_value("minecraft:golden_carrot"),
            Some((6, 14.4, false))
        );
        assert_eq!(food_value("minecraft:rabbit_stew"), Some((10, 12.0, false)));
        assert_eq!(food_value("minecraft:cooked_beef"), Some((8, 12.8, false)));
        // Staples and the worst foods.
        assert_eq!(food_value("minecraft:bread"), Some((5, 6.0, false)));
        assert_eq!(food_value("minecraft:cookie"), Some((2, 0.4, false)));
        assert_eq!(food_value("minecraft:pufferfish"), Some((1, 0.2, false)));
        assert_eq!(food_value("minecraft:rotten_flesh"), Some((4, 0.8, false)));
        assert_eq!(food_value("minecraft:spider_eye"), Some((2, 3.2, false)));
        // The always-eatable foods.
        assert_eq!(food_value("minecraft:chorus_fruit"), Some((4, 2.4, true)));
        assert_eq!(food_value("minecraft:golden_apple"), Some((4, 9.6, true)));
        assert_eq!(
            food_value("minecraft:enchanted_golden_apple"),
            Some((4, 9.6, true))
        );
        assert_eq!(food_value("minecraft:honey_bottle"), Some((6, 1.2, true)));
        assert_eq!(
            food_value("minecraft:suspicious_stew"),
            Some((6, 7.2, true))
        );
        // Not food, and raw fish is barely food.
        assert_eq!(food_value("minecraft:diamond_sword"), None);
        assert_eq!(food_value("minecraft:torch"), None);
        assert_eq!(food_value("minecraft:cod"), Some((2, 0.4, false)));
        assert_eq!(food_value("minecraft:apple"), Some((4, 2.4, false)));
    }

    #[test]
    fn eaten_clamps_food_and_saturation_like_the_original() {
        // A steak at 14 hunger, 3 saturation: food fills to 20, saturation
        // to 15.8 — under the new food level, so no second clamp bites.
        assert_eq!(eaten(14, 3.0, 8, 12.8), (20, 15.8));
        // Saturation can never exceed the hunger bar: a golden carrot at
        // full hunger would take saturation to 20, not past it.
        assert_eq!(eaten(20, 18.0, 6, 14.4), (20, 20.0));
        // A small snack at low hunger.
        assert_eq!(eaten(3, 0.5, 4, 2.4), (7, 2.9));
    }

    #[test]
    fn saturation_bar_fills_in_proportion() {
        assert_eq!(saturation_bar(20.0), "[##########]");
        assert_eq!(saturation_bar(10.0), "[#####-----]");
        assert_eq!(saturation_bar(0.0), "[----------]");
        assert_eq!(saturation_bar(12.4), "[######----]");
        assert_eq!(saturation_bar(1.0), "[#---------]"); // 0.5 rounds up
    }

    #[test]
    fn saturation_formats_without_trailing_zero() {
        assert_eq!(fmt_saturation(6.0), "6");
        assert_eq!(fmt_saturation(12.8), "12.8");
        assert_eq!(fmt_saturation(0.0), "0");
    }

    #[test]
    fn preview_line_shows_gain_and_totals() {
        let beef = stack(0, "minecraft:cooked_beef");
        assert_eq!(
            preview_line(&beef, 14, 3.0).unwrap().text,
            "Cooked Beef +8 hunger, +12.8 sat -> 20 hunger, 15.8 sat"
        );
        assert_eq!(preview_line(&beef, 14, 3.0).unwrap().color, COLOR_PREVIEW);
        // A golden carrot at full hunger still previews: it is not
        // always-eatable, so at 20 hunger it cannot be eaten at all.
        let carrot = stack(0, "minecraft:golden_carrot");
        assert_eq!(
            preview_line(&carrot, 20, 5.0).unwrap().text,
            "Golden Carrot: full hunger"
        );
        assert_eq!(preview_line(&carrot, 20, 5.0).unwrap().color, COLOR_DIM);
        // A golden apple at full hunger can still be eaten for saturation.
        let apple = stack(0, "minecraft:golden_apple");
        assert_eq!(
            preview_line(&apple, 20, 5.0).unwrap().text,
            "Golden Apple +4 hunger, +9.6 sat -> 20 hunger, 14.6 sat"
        );
        // A sword is not food: no preview line.
        assert!(preview_line(&stack(0, "minecraft:diamond_sword"), 14, 3.0).is_none());
    }

    #[test]
    fn panel_shows_saturation_and_preview() {
        let inventory = inv(0, 12.4, vec![stack(0, "minecraft:cooked_beef")]);
        let lines = panel_lines(14, &inventory, &Settings::default());
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Saturation [######----] 12.4",
                "Cooked Beef +8 hunger, +12.8 sat -> 20 hunger, 20 sat",
            ]
        );
        assert_eq!(lines[0].color, COLOR_SATURATION);
        // No food in hand: the saturation line alone.
        let empty_hand = inv(0, 5.0, vec![stack(0, "minecraft:torch")]);
        let lines = panel_lines(18, &empty_hand, &Settings::default());
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["Saturation [###-------] 5"]);
        // Both lines off: no panel.
        let off = Settings {
            show_saturation: false,
            show_preview: false,
            ..Settings::default()
        };
        assert!(panel_lines(14, &inventory, &off).is_empty());
    }

    #[test]
    fn reads_and_writes_settings_correctly() {
        let defaults = Settings::read(stored(&[]));
        assert_eq!(defaults, Settings::default());
        assert_eq!(defaults.position, 3); // Bottom right

        let custom = Settings::read(stored(&[
            ("show_saturation", "false"),
            ("show_preview", "false"),
            ("position", "1"),
            ("shown", "false"),
        ]));
        assert_eq!(
            custom,
            Settings {
                show_saturation: false,
                show_preview: false,
                position: 1,
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
        s.click(0); // show_saturation -> false
        assert!(!s.show_saturation);
        s.click(1); // show_preview -> false
        assert!(!s.show_preview);
        s.click(2); // position cycles -> Top left (0)
        assert_eq!(s.position, 0);
        s.click(3); // shown -> false
        assert!(!s.shown);
    }

    #[test]
    fn display_names_come_from_item_ids() {
        assert_eq!(display_name("minecraft:cooked_beef"), "Cooked Beef");
        assert_eq!(
            display_name("minecraft:enchanted_golden_apple"),
            "Enchanted Golden Apple"
        );
        assert_eq!(display_name("minecraft:torch"), "Torch");
    }
}

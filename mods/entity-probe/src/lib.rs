//! Prototype probe for client entity reads (`docs/mods.md`, "Client
//! entities"): each tick it reads the living mobs around the player and
//! the mob under the crosshair, and shows them in a HUD panel — the
//! crosshair target first, then the nearest mobs, with a count line for
//! the rest. Options > Mods > Entity Probe has the settings: how many
//! mobs the panel lists (5, 10, 15 or All) and how much detail each line
//! carries (Short: name, kind, distance and health; Long: the full line
//! with hitbox, velocity, yaw and the baby and on-ground flags).
use horizon_mod_sdk::{
    Mod, Registration,
    entity::{self, Entity},
    game,
    hud::{self, Corner, Line},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The storage keys for the two settings.
const MOBS_SHOWN_KEY: &str = "mobs_shown";
const DETAIL_KEY: &str = "detail";

/// The settings page's buttons: how many mobs to list, how much detail.
const MOBS_SHOWN_BUTTON: u32 = 0;
const DETAIL_BUTTON: u32 = 1;

/// The mobs-shown switch's cycle: 5, 10, 15, then All; 5 is the default.
const COUNTS: [Option<usize>; 4] = [Some(5), Some(10), Some(15), None];

/// How much detail a mob's line carries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Detail {
    Short,
    Long,
}

struct EntityProbe {
    /// How many mob lines the panel shows (`None` lists them all).
    mobs_shown: Option<usize>,
    detail: Detail,
    last_panel: Vec<String>,
}

impl EntityProbe {
    /// One mob's line in the panel, short form: the name as the game
    /// shows it, the kind, the feet distance from the player, health,
    /// and the baby flag — only the important facts.
    fn short_line(m: &Entity, player: &game::Player) -> String {
        let distance =
            ((m.x - player.x).powi(2) + (m.y - player.y).powi(2) + (m.z - player.z).powi(2)).sqrt();
        let health = m.health.map_or("?".into(), |h| format!("{h:.1}"));
        let mut line = format!(
            "{} ({}) {:.1} {health}/{:.1}",
            m.name, m.kind, distance, m.max_health
        );
        if m.baby {
            line.push_str(" baby");
        }
        line
    }

    /// One mob's line in the panel, long form: the name as the game shows
    /// it, the kind, the feet distance from the player, health, hitbox
    /// size, velocity, yaw, and the baby and on-ground flags.
    fn long_line(m: &Entity, player: &game::Player) -> String {
        let distance =
            ((m.x - player.x).powi(2) + (m.y - player.y).powi(2) + (m.z - player.z).powi(2)).sqrt();
        let health = m.health.map_or("?".into(), |h| format!("{h:.1}"));
        let mut line = format!(
            "{} ({}) {:.1} {health}/{:.1} {:.2}x{:.2} v={:.1},{:.1},{:.1} yaw={:.1}",
            m.name, m.kind, distance, m.max_health, m.width, m.height, m.vx, m.vy, m.vz, m.yaw
        );
        if m.baby {
            line.push_str(" baby");
        }
        line.push_str(if m.on_ground { " ground" } else { " air" });
        line
    }

    /// The mob lines for a panel: one line per mob (nearest first),
    /// capped at `mobs_shown` when set, with the dropped mobs counted
    /// in one final line instead of listed — a crowd used to cover most
    /// of the screen.
    fn mob_lines(
        entities: &[Entity],
        player: &game::Player,
        mobs_shown: Option<usize>,
        detail: Detail,
    ) -> Vec<String> {
        let lines: Vec<String> = entities
            .iter()
            .map(|m| match detail {
                Detail::Short => Self::short_line(m, player),
                Detail::Long => Self::long_line(m, player),
            })
            .collect();
        match mobs_shown {
            None => lines,
            Some(n) => {
                let total = lines.len();
                if total <= n {
                    return lines;
                }
                let mut capped: Vec<String> = lines.into_iter().take(n).collect();
                capped.push(format!("+{} more nearby", total - n));
                capped
            }
        }
    }

    /// The settings page: one button per setting, cycling its label.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: MOBS_SHOWN_BUTTON,
                label: count_label(self.mobs_shown),
            }),
            Widget::Button(ButtonWidget {
                id: DETAIL_BUTTON,
                label: detail_label(self.detail),
            }),
        ]);
    }

    fn cycle_mobs_shown(&mut self) {
        self.mobs_shown = next_count(self.mobs_shown);
        let value = match self.mobs_shown {
            Some(n) => n.to_string(),
            None => "all".to_string(),
        };
        let _ = storage::set(MOBS_SHOWN_KEY, value.as_bytes());
        self.refresh_settings();
    }

    fn toggle_detail(&mut self) {
        self.detail = if self.detail == Detail::Long {
            Detail::Short
        } else {
            Detail::Long
        };
        let _ = storage::set(
            DETAIL_KEY,
            if self.detail == Detail::Long {
                b"long"
            } else {
                b"short"
            },
        );
        self.refresh_settings();
    }
}

/// How many mobs the panel shows, read back from storage: 5, 10, 15 or
/// All; anything else (including nothing stored) falls back to 5.
fn read_count(stored: Option<Vec<u8>>) -> Option<usize> {
    match stored.and_then(|v| String::from_utf8(v).ok()).as_deref() {
        Some("10") => Some(10),
        Some("15") => Some(15),
        Some("all") => None,
        _ => Some(5),
    }
}

/// The detail mode, read back from storage: Long only when "long" was
/// stored, Short otherwise (including nothing stored).
fn read_detail(stored: Option<Vec<u8>>) -> Detail {
    match stored.and_then(|v| String::from_utf8(v).ok()).as_deref() {
        Some("long") => Detail::Long,
        _ => Detail::Short,
    }
}

fn count_label(count: Option<usize>) -> String {
    match count {
        Some(n) => format!("Mobs shown: {n}"),
        None => "Mobs shown: All".to_string(),
    }
}

fn detail_label(detail: Detail) -> String {
    format!(
        "Detail: {}",
        if detail == Detail::Long {
            "Long"
        } else {
            "Short"
        }
    )
}

/// The next count in the switch's cycle, wrapping back to the first.
fn next_count(count: Option<usize>) -> Option<usize> {
    let i = COUNTS.iter().position(|c| *c == count).unwrap_or(0);
    COUNTS[(i + 1) % COUNTS.len()]
}

impl Mod for EntityProbe {
    fn init() -> Result<(Self, Registration), String> {
        Ok((
            EntityProbe {
                mobs_shown: read_count(storage::get(MOBS_SHOWN_KEY)),
                detail: read_detail(storage::get(DETAIL_KEY)),
                last_panel: Vec::new(),
            },
            Registration::new().on_tick(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        let player = game::get_player();
        let entities = entity::entities();
        let mut panel: Vec<String> = match &player {
            Some(p) => Self::mob_lines(&entities, p, self.mobs_shown, self.detail),
            None => Vec::new(),
        };
        if panel.is_empty() {
            panel.push("no mobs".to_string());
        }
        // The crosshair target is the panel's first line, not a chat
        // message: chat draws over the same corner as the panel, so the
        // target lines landed on top of the list and neither was readable.
        let target = entity::targeted(128.0).map(|t| {
            let health = t.entity.health.map_or("?".into(), |h| format!("{h:.1}"));
            format!(
                "Target: {} ({}) {:.1} blocks {health}/{:.1}",
                t.entity.name, t.entity.kind, t.distance, t.entity.max_health
            )
        });
        panel.insert(0, target.unwrap_or_else(|| "Target: none".to_string()));
        if panel != self.last_panel {
            let lines: Vec<Line> = panel
                .iter()
                .map(|text| Line {
                    text: text.clone(),
                    color: 0xFFFFFF,
                })
                .collect();
            hud::set_panel(Corner::TopLeft, &lines);
            self.last_panel = panel;
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            MOBS_SHOWN_BUTTON => self.cycle_mobs_shown(),
            DETAIL_BUTTON => self.toggle_detail(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(EntityProbe);

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> game::Player {
        game::Player {
            x: 0.5,
            y: 64.0,
            z: 0.5,
            yaw: 0.0,
            pitch: 0.0,
            health: 20.0,
            food: 20,
            dimension: "minecraft:overworld".into(),
        }
    }

    fn mob(name: &str, x: f64, z: f64, health: Option<f32>, baby: bool) -> Entity {
        Entity {
            id: 1,
            kind: "minecraft:zombie".into(),
            name: name.into(),
            x,
            y: 64.0,
            z,
            vx: 0.0,
            vy: -0.1,
            vz: 0.0,
            yaw: 0.0,
            health,
            max_health: 20.0,
            width: 0.6,
            height: 1.95,
            on_ground: true,
            custom_name: None,
            baby,
        }
    }

    #[test]
    fn short_line_has_only_the_important_facts() {
        assert_eq!(
            EntityProbe::short_line(&mob("Zombie", 0.5, 5.5, Some(14.0), false), &player()),
            "Zombie (minecraft:zombie) 5.0 14.0/20.0"
        );
        // A baby mob is marked; unknown health reads as `?`.
        assert_eq!(
            EntityProbe::short_line(&mob("Zombie", -0.5, 3.5, Some(20.0), true), &player()),
            "Zombie (minecraft:zombie) 3.2 20.0/20.0 baby"
        );
        assert_eq!(
            EntityProbe::short_line(&mob("Zombie", 0.5, 5.5, None, false), &player()),
            "Zombie (minecraft:zombie) 5.0 ?/20.0"
        );
    }

    #[test]
    fn long_line_is_the_full_line_unchanged() {
        assert_eq!(
            EntityProbe::long_line(&mob("Zombie", 0.5, 5.5, Some(14.0), false), &player()),
            "Zombie (minecraft:zombie) 5.0 14.0/20.0 0.60x1.95 v=0.0,-0.1,0.0 yaw=0.0 ground"
        );
        assert_eq!(
            EntityProbe::long_line(&mob("Zombie", -0.5, 3.5, Some(20.0), true), &player()),
            "Zombie (minecraft:zombie) 3.2 20.0/20.0 0.60x1.95 v=0.0,-0.1,0.0 yaw=0.0 baby ground"
        );
    }

    #[test]
    fn mob_lines_cap_at_five_ten_fifteen_or_all() {
        let mobs: Vec<Entity> = (0..16)
            .map(|i| mob("Zombie", 0.5, 5.5 + i as f64, Some(20.0), false))
            .collect();
        // No mobs: no lines at all (the panel's `no mobs` is added by tick).
        assert!(EntityProbe::mob_lines(&[], &player(), Some(5), Detail::Short).is_empty());
        // Five: the five nearest, then the count of the other eleven.
        let five = EntityProbe::mob_lines(&mobs, &player(), Some(5), Detail::Short);
        assert_eq!(five.len(), 6);
        assert_eq!(five[5], "+11 more nearby");
        // Ten: ten lines, then the count of the other six.
        let ten = EntityProbe::mob_lines(&mobs, &player(), Some(10), Detail::Short);
        assert_eq!(ten.len(), 11);
        assert_eq!(ten[10], "+6 more nearby");
        // Fifteen: fifteen lines, then the one left over.
        let fifteen = EntityProbe::mob_lines(&mobs, &player(), Some(15), Detail::Short);
        assert_eq!(fifteen.len(), 16);
        assert_eq!(fifteen[15], "+1 more nearby");
        // All: every line, no count line at all.
        let all = EntityProbe::mob_lines(&mobs, &player(), None, Detail::Short);
        assert_eq!(all.len(), 16);
        assert!(!all.iter().any(|l| l.contains("more nearby")));
        // Fewer mobs than the cap: no count line either.
        let few = EntityProbe::mob_lines(&mobs[..3], &player(), Some(5), Detail::Short);
        assert_eq!(few.len(), 3);
        // Long lines cap the same way.
        let long = EntityProbe::mob_lines(&mobs, &player(), Some(5), Detail::Long);
        assert_eq!(long.len(), 6);
        assert_eq!(long[5], "+11 more nearby");
        assert!(long[0].contains("0.60x1.95"));
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert_eq!(read_count(None), Some(5));
        assert_eq!(read_count(Some(b"5".to_vec())), Some(5));
        assert_eq!(read_count(Some(b"10".to_vec())), Some(10));
        assert_eq!(read_count(Some(b"15".to_vec())), Some(15));
        assert_eq!(read_count(Some(b"all".to_vec())), None);
        // A stored count the switch never offers falls back to 5.
        assert_eq!(read_count(Some(b"7".to_vec())), Some(5));
        assert_eq!(read_detail(None), Detail::Short);
        assert_eq!(read_detail(Some(b"short".to_vec())), Detail::Short);
        assert_eq!(read_detail(Some(b"long".to_vec())), Detail::Long);
        assert_eq!(read_detail(Some(b"full".to_vec())), Detail::Short);
        assert_eq!(count_label(Some(5)), "Mobs shown: 5");
        assert_eq!(count_label(Some(10)), "Mobs shown: 10");
        assert_eq!(count_label(None), "Mobs shown: All");
        assert_eq!(detail_label(Detail::Short), "Detail: Short");
        assert_eq!(detail_label(Detail::Long), "Detail: Long");
        // The cycle steps through all four and wraps back to 5.
        assert_eq!(next_count(Some(5)), Some(10));
        assert_eq!(next_count(Some(10)), Some(15));
        assert_eq!(next_count(Some(15)), None);
        assert_eq!(next_count(None), Some(5));
    }
}

//! Prototype probe for client entity reads (`docs/mods.md`, "Client
//! entities"): each tick it reads the living mobs around the player and
//! the mob under the crosshair, and shows them in a HUD panel — the
//! crosshair target first, then every mob nearest first, with kind,
//! distance, health, hitbox, velocity, yaw and the baby flag.
use horizon_mod_sdk::{
    Mod, Registration,
    entity::{self, Entity},
    game,
    hud::{self, Corner, Line},
};

struct EntityProbe {
    last_panel: Vec<String>,
}

impl EntityProbe {
    /// One mob's line in the panel: the name as the game shows it, the
    /// kind, the feet distance from the player, health, hitbox size,
    /// velocity, yaw, and the baby and on-ground flags.
    fn line(m: &Entity, player: &game::Player) -> String {
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
}

impl Mod for EntityProbe {
    fn init() -> Result<(Self, Registration), String> {
        Ok((
            EntityProbe {
                last_panel: Vec::new(),
            },
            Registration::new().on_tick(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        let player = game::get_player();
        let entities = entity::entities();
        let mut panel: Vec<String> = match &player {
            Some(p) => entities.iter().map(|m| Self::line(m, p)).collect(),
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
}

horizon_mod_sdk::export_mod!(EntityProbe);

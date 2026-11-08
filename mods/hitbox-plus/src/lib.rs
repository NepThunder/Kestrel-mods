//! Hitbox+ for Kestrel: the game's F3+B hitboxes as a customizable PvP
//! aid — a box round every living mob, bright green while your crosshair
//! is on it within attack reach, in your choice of colour and thickness,
//! with a red eye-level band and a blue look-direction arrow per mob, plus
//! a sprint indicator in the corner you pick. H toggles the boxes
//! (rebindable on the mod's page in Options > Mods), `/hitbox` does the
//! same, and that page also cycles the colour, thickness, eye ray, sprint
//! display and sprint position; the game keeps the choices.
//!
//! Behaviour reimplemented from Hitbox+ 1.3.0 by taxtii and itzglecni
//! (MIT, <https://github.com/taxtii/hitbox_plus>); this file shares no
//! code with the original — the original is a native mixin mod, this is a
//! fresh implementation against the Kestrel SDK. The original's registry
//! blurb mentions eleven reach colours and a rainbow style; the shipped
//! 1.3.0 source has one in-reach colour (bright green) and four hitbox
//! styles, so this port has those. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/hitbox-plus>.
//!
//! What the SDK cannot do, the original's native build can: boxes round
//! non-living entities — dropped items, paintings, item frames, boats,
//! minecarts, armor stands — because the v1 entity API lists living mobs
//! only; the exact eye height per mob (the band sits at 0.85 of the
//! height, an approximation); the mob's true look direction (the arrow
//! runs horizontally along its movement yaw — the API exposes no pitch);
//! creative-mode reach (5 blocks, 6.5 for a spear — the game mode is not
//! exposed, so this port always uses the survival reach, 3 blocks and 4.5
//! for a spear); the sprint state itself (the indicator lights while the
//! sprint key is held, so double-tap-W sprint and toggle sprint do not
//! light it, and there is no swimming line — the API exposes no sprinting,
//! swimming or crouch state, only whether a key is held). In multiplayer
//! the mob list is empty in v1, so only the mob under the crosshair gets
//! a box there. The boxes are depth-tested, so a wall hides them, and
//! they hide with the HUD (F1). The SDK caps a mod at 1024 world lines,
//! so with the eye ray on the nearest 48 mobs draw (64 with it off), the
//! crosshair target always first.
use horizon_mod_sdk::{
    Mod, Registration, chat,
    entity::{self, Entity, Target},
    game,
    hud::{self, Corner, Line},
    input::{self, Event},
    render::{self, WorldBox, WorldLine, WorldPoint, WorldShape},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `render.set-world` is level 11, `entity.targeted` is level 10, the
/// rest level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 11;

/// The key's name for `register-key`, and its default: H, as the
/// original binds it.
const KEY: &str = "toggle";
const DEFAULT_KEY: &str = "key.keyboard.h";

/// Bright green while the crosshair is on a mob within attack reach.
const IN_REACH_COLOR: u32 = 0xFF00_E676;
/// The four hitbox styles: name and 0xAARRGGBB colour, as the original
/// lists them (Neon Cyan first).
const STYLES: [(&str, u32); 4] = [
    ("Neon Cyan", 0xFF00_E5FF),
    ("Gold Yellow", 0xFFFF_D600),
    ("Crimson Red", 0xFFFF_1744),
    ("White", 0xFFFF_FFFF),
];
/// The three thicknesses: width in pixels and the label the settings
/// page shows.
const THICKNESSES: [(f32, &str); 3] = [
    (1.5, "Thin (1.5px)"),
    (2.5, "Normal (2.5px)"),
    (4.0, "Thick (4.0px)"),
];
/// The eye-level band's red and the look arrow's blue, as the game's own
/// gizmos draw them.
const EYE_RED: u32 = 0xFFFF_0000;
const RAY_BLUE: u32 = 0xFF00_00FF;
/// Where the eye band sits: the API gives no eye height per mob, so the
/// band is this fraction of the mob's height — 0.85 is the middle of the
/// vanilla range (about 0.74 for a creeper, 1.0 for a spider).
const EYE_FRACTION: f64 = 0.85;
/// How long the look arrow runs from the mob's eye, as the original's
/// two blocks.
const RAY_LENGTH: f64 = 2.0;
/// The per-mod world-line budget (`render.set-world`): a box counts 12.
const MAX_LINES: usize = 1024;
/// Attack reach in survival, as the original measures it: 3 blocks for
/// swords, axes and fists, 4.5 for a spear. Creative reach (5 and 6.5)
/// is not used — the game mode is not exposed.
const MELEE_REACH: f32 = 3.0;
const SPEAR_REACH: f32 = 4.5;

/// The storage keys for the six settings.
const ENABLED_KEY: &str = "enabled";
const STYLE_KEY: &str = "style";
const THICKNESS_KEY: &str = "thickness";
const EYE_RAY_KEY: &str = "eye_ray";
const SPRINT_DISPLAY_KEY: &str = "sprint_display";
const SPRINT_CORNER_KEY: &str = "sprint_corner";

/// The settings page's buttons: show the boxes, colour, thickness, eye
/// ray, sprint display, sprint position.
const ENABLED_BUTTON: u32 = 0;
const STYLE_BUTTON: u32 = 1;
const THICKNESS_BUTTON: u32 = 2;
const EYE_RAY_BUTTON: u32 = 3;
const SPRINT_DISPLAY_BUTTON: u32 = 4;
const SPRINT_CORNER_BUTTON: u32 = 5;

/// The sprint position's corners, in the order the switch cycles them.
const CORNERS: [Corner; 4] = [
    Corner::TopLeft,
    Corner::TopRight,
    Corner::BottomLeft,
    Corner::BottomRight,
];
const CORNER_NAMES: [&str; 4] = ["Top-Left", "Top-Right", "Bottom-Left", "Bottom-Right"];

/// The mob's hitbox as a world box: its feet, its width either side, its
/// height up.
fn mob_box(entity: &Entity, color: u32, width: f32) -> WorldShape {
    let half = f64::from(entity.width) / 2.0;
    WorldShape::Box(WorldBox {
        min: WorldPoint {
            x: entity.x - half,
            y: entity.y,
            z: entity.z - half,
        },
        max: WorldPoint {
            x: entity.x + half,
            y: entity.y + f64::from(entity.height),
            z: entity.z + half,
        },
        color,
        width,
    })
}

/// The mob's eye height above its feet: the API gives no eye height, so
/// this fraction of the height.
fn eye_height(entity: &Entity) -> f64 {
    f64::from(entity.height) * EYE_FRACTION
}

/// The red eye-level band: the rectangle round the box at the eye height.
/// The original draws it as a 0.02-block thin cuboid (twelve edges); the
/// four edges of its top face draw the same band, and the eight saved
/// lines buy more mobs under the 1024-line cap.
fn eye_band(entity: &Entity, width: f32) -> [WorldShape; 4] {
    let half = f64::from(entity.width) / 2.0;
    let eye = entity.y + eye_height(entity);
    let (x0, x1) = (entity.x - half, entity.x + half);
    let (z0, z1) = (entity.z - half, entity.z + half);
    let line = |ax: f64, az: f64, bx: f64, bz: f64| {
        WorldShape::Line(WorldLine {
            start: WorldPoint {
                x: ax,
                y: eye,
                z: az,
            },
            end: WorldPoint {
                x: bx,
                y: eye,
                z: bz,
            },
            color: EYE_RED,
            width,
        })
    };
    [
        line(x0, z0, x1, z0),
        line(x1, z0, x1, z1),
        line(x1, z1, x0, z1),
        line(x0, z1, x0, z0),
    ]
}

/// The mob's horizontal look direction from its yaw, as the game's view
/// vector with no pitch: (-sin yaw, 0, cos yaw).
fn view_dir(yaw: f32) -> (f64, f64, f64) {
    let rad = f64::from(yaw).to_radians();
    (-rad.sin(), 0.0, rad.cos())
}

/// The blue look-direction arrow: the shaft from the mob's eye, two
/// blocks along its yaw, and the four tips the game's gizmo arrow draws
/// — each `clamp(length * 0.1, 0.1, 1.0)` off the end, one up, one down,
/// one either side.
fn look_arrow(entity: &Entity, width: f32) -> [WorldShape; 5] {
    let eye = WorldPoint {
        x: entity.x,
        y: entity.y + eye_height(entity),
        z: entity.z,
    };
    let (dx, _, dz) = view_dir(entity.yaw);
    let end = WorldPoint {
        x: eye.x + dx * RAY_LENGTH,
        y: eye.y,
        z: eye.z + dz * RAY_LENGTH,
    };
    let len = (RAY_LENGTH * 0.1).clamp(0.1, 1.0);
    // The end pulled back along the shaft, the foot of the four tips.
    let base = (end.x - dx * len, end.y, end.z - dz * len);
    let up = (0.0, len, 0.0);
    let side = (-dz * len, 0.0, dx * len);
    let tip = |ox: f64, oy: f64, oz: f64| {
        WorldShape::Line(WorldLine {
            start: WorldPoint {
                x: base.0 + ox,
                y: base.1 + oy,
                z: base.2 + oz,
            },
            end,
            color: RAY_BLUE,
            width,
        })
    };
    [
        WorldShape::Line(WorldLine {
            start: eye,
            end,
            color: RAY_BLUE,
            width,
        }),
        tip(0.0, up.1, 0.0),
        tip(0.0, -up.1, 0.0),
        tip(side.0, 0.0, side.2),
        tip(-side.0, 0.0, -side.2),
    ]
}

/// One mob's shapes: its box, its eye band, and its look arrow when the
/// eye ray is on — 21 lines with it, 16 without.
fn mob_shapes(entity: &Entity, color: u32, width: f32, eye_ray: bool) -> Vec<WorldShape> {
    let mut shapes = Vec::with_capacity(if eye_ray { 21 } else { 16 });
    shapes.push(mob_box(entity, color, width));
    shapes.extend(eye_band(entity, width));
    if eye_ray {
        shapes.extend(look_arrow(entity, width));
    }
    shapes
}

/// The lines one mob costs under the cap.
fn lines_per_mob(eye_ray: bool) -> usize {
    if eye_ray { 21 } else { 16 }
}

/// The shapes for this tick: the crosshair target first, in the in-reach
/// colour, so it is never crowded out by nearer mobs, then the rest
/// nearest first in the style colour, stopping inside the 1024-line cap.
/// The target may be a mob the list does not carry — a player, or in
/// multiplayer, where the list is empty — so it is drawn from the target
/// itself, not looked up in the list.
fn build_shapes(
    entities: &[Entity],
    target: Option<&Target>,
    color: u32,
    width: f32,
    eye_ray: bool,
) -> Vec<WorldShape> {
    let max_mobs = MAX_LINES / lines_per_mob(eye_ray);
    let mut shapes = Vec::new();
    let mut drawn = 0;
    if let Some(target) = target {
        shapes.extend(mob_shapes(&target.entity, IN_REACH_COLOR, width, eye_ray));
        drawn += 1;
    }
    for entity in entities {
        if drawn >= max_mobs {
            break;
        }
        if target.is_some_and(|t| t.entity.id == entity.id) {
            continue;
        }
        shapes.extend(mob_shapes(entity, color, width, eye_ray));
        drawn += 1;
    }
    shapes
}

/// Attack reach for the held item: a spear reaches 4.5 blocks, anything
/// else 3.0 — the original's survival reach, since the game mode is not
/// exposed.
fn reach_for(held: Option<input::HeldItem>) -> f32 {
    if held.is_some_and(|h| h.id.contains("spear")) {
        SPEAR_REACH
    } else {
        MELEE_REACH
    }
}

/// The sprint indicator's line: the original shows the player's real
/// sprinting, swimming and crouch state, which the API does not expose,
/// so this port shows what the keys say — the sprint key held, else the
/// sneak key held — and no swimming line.
fn sprint_text(sprint_down: bool, sneak_down: bool) -> Option<&'static str> {
    if sprint_down {
        Some("[Sprinting (Key)]")
    } else if sneak_down {
        Some("[Sneaking (Key)]")
    } else {
        None
    }
}

/// The stored boolean: `false` only when it says so, else the default.
fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

/// The stored index into a switch: the stored number when it is in
/// range, else the default.
fn read_index(stored: Option<Vec<u8>>, count: usize, default: usize) -> usize {
    stored
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|i| *i < count)
        .unwrap_or(default)
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

struct HitboxPlus {
    enabled: bool,
    style: usize,
    thickness: usize,
    eye_ray: bool,
    sprint_display: bool,
    sprint_corner: usize,
    /// The sprint line and corner last sent, so an unchanged one costs
    /// no call.
    last_sprint: Option<(String, Corner)>,
}

impl HitboxPlus {
    fn color(&self) -> u32 {
        STYLES[self.style].1
    }

    fn thickness(&self) -> f32 {
        THICKNESSES[self.thickness].0
    }

    fn corner(&self) -> Corner {
        CORNERS[self.sprint_corner]
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        let _ = storage::set(ENABLED_KEY, if enabled { b"true" } else { b"false" });
        self.refresh_settings();
    }

    fn cycle_style(&mut self) {
        self.style = (self.style + 1) % STYLES.len();
        let _ = storage::set(STYLE_KEY, self.style.to_string().as_bytes());
        self.refresh_settings();
    }

    fn cycle_thickness(&mut self) {
        self.thickness = (self.thickness + 1) % THICKNESSES.len();
        let _ = storage::set(THICKNESS_KEY, self.thickness.to_string().as_bytes());
        self.refresh_settings();
    }

    fn toggle_eye_ray(&mut self) {
        self.eye_ray = !self.eye_ray;
        let _ = storage::set(EYE_RAY_KEY, if self.eye_ray { b"true" } else { b"false" });
        self.refresh_settings();
    }

    fn toggle_sprint_display(&mut self) {
        self.sprint_display = !self.sprint_display;
        let _ = storage::set(
            SPRINT_DISPLAY_KEY,
            if self.sprint_display {
                b"true"
            } else {
                b"false"
            },
        );
        self.refresh_settings();
    }

    fn cycle_sprint_corner(&mut self) {
        self.sprint_corner = (self.sprint_corner + 1) % CORNERS.len();
        let _ = storage::set(SPRINT_CORNER_KEY, self.sprint_corner.to_string().as_bytes());
        self.refresh_settings();
    }

    /// The settings page: one button per setting, cycling its label.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: ENABLED_BUTTON,
                label: format!("Hitbox+: {}", on_off(self.enabled)),
            }),
            Widget::Button(ButtonWidget {
                id: STYLE_BUTTON,
                label: format!("Color: {}", STYLES[self.style].0),
            }),
            Widget::Button(ButtonWidget {
                id: THICKNESS_BUTTON,
                label: format!("Thickness: {}", THICKNESSES[self.thickness].1),
            }),
            Widget::Button(ButtonWidget {
                id: EYE_RAY_BUTTON,
                label: format!("Eye Ray: {}", on_off(self.eye_ray)),
            }),
            Widget::Button(ButtonWidget {
                id: SPRINT_DISPLAY_BUTTON,
                label: format!("Sprint Display: {}", on_off(self.sprint_display)),
            }),
            Widget::Button(ButtonWidget {
                id: SPRINT_CORNER_BUTTON,
                label: format!("Sprint Pos: {}", CORNER_NAMES[self.sprint_corner]),
            }),
        ]);
    }

    fn clear_panel(&mut self) {
        if self.last_sprint.is_some() {
            hud::clear();
            self.last_sprint = None;
        }
    }

    /// The sprint indicator: shown while a sprint or sneak key is held,
    /// in the corner the setting picks, cleared otherwise.
    fn show_sprint(&mut self) {
        let text = if !self.sprint_display {
            None
        } else {
            let sprint = input::mapping("key.sprint").is_some_and(|m| m.down);
            let sneak = input::mapping("key.sneak").is_some_and(|m| m.down);
            sprint_text(sprint, sneak)
        };
        let now = text.map(|t| (t.to_string(), self.corner()));
        if now != self.last_sprint {
            match &now {
                Some((text, corner)) => {
                    hud::set_panel(
                        *corner,
                        &[Line {
                            text: text.clone(),
                            color: 0xFFFFFF,
                        }],
                    );
                }
                None => hud::clear(),
            }
            self.last_sprint = now;
        }
    }
}

impl Mod for HitboxPlus {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(KEY, "Toggle Hitbox+", DEFAULT_KEY)?;
        let state = HitboxPlus {
            enabled: read_bool(storage::get(ENABLED_KEY), true),
            style: read_index(storage::get(STYLE_KEY), STYLES.len(), 0),
            thickness: read_index(storage::get(THICKNESS_KEY), THICKNESSES.len(), 1),
            eye_ray: read_bool(storage::get(EYE_RAY_KEY), true),
            sprint_display: read_bool(storage::get(SPRINT_DISPLAY_KEY), true),
            sprint_corner: read_index(storage::get(SPRINT_CORNER_KEY), CORNERS.len(), 0),
            last_sprint: None,
        };
        Ok((state, Registration::new().command("hitbox").on_tick()))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            if let Event::KeyDown(k) = event
                && k == KEY
            {
                let enabled = !self.enabled;
                self.set_enabled(enabled);
                chat::show(&format!(
                    "HitBox+ {}",
                    if enabled { "Enabled" } else { "Disabled" }
                ));
            }
        }
        let Some(_player) = game::get_player() else {
            render::set_world(&[]);
            self.clear_panel();
            return;
        };
        if self.enabled {
            let reach = reach_for(input::held());
            let target = entity::targeted(reach);
            let entities = entity::entities();
            let shapes = build_shapes(
                &entities,
                target.as_ref(),
                self.color(),
                self.thickness(),
                self.eye_ray,
            );
            render::set_world(&shapes);
        } else {
            render::set_world(&[]);
        }
        self.show_sprint();
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "hitbox" {
            return Err("This command does nothing".into());
        }
        let enabled = !self.enabled;
        self.set_enabled(enabled);
        Ok(Some(format!(
            "HitBox+ {}",
            if enabled { "enabled" } else { "disabled" }
        )))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            ENABLED_BUTTON => {
                let enabled = !self.enabled;
                self.set_enabled(enabled);
            }
            STYLE_BUTTON => self.cycle_style(),
            THICKNESS_BUTTON => self.cycle_thickness(),
            EYE_RAY_BUTTON => self.toggle_eye_ray(),
            SPRINT_DISPLAY_BUTTON => self.toggle_sprint_display(),
            SPRINT_CORNER_BUTTON => self.cycle_sprint_corner(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(HitboxPlus);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::entity::Entity;

    fn mob(id: u32, x: f64, y: f64, z: f64) -> Entity {
        Entity {
            id,
            kind: "minecraft:zombie".into(),
            name: "Zombie".into(),
            x,
            y,
            z,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            yaw: 0.0,
            health: Some(20.0),
            max_health: 20.0,
            width: 0.6,
            height: 1.95,
            on_ground: true,
            custom_name: None,
            baby: false,
        }
    }

    fn target(entity: Entity, distance: f32) -> Target {
        Target { entity, distance }
    }

    /// How many world lines a list of shapes draws as: a box is twelve.
    fn line_count(shapes: &[WorldShape]) -> usize {
        shapes
            .iter()
            .map(|s| match s {
                WorldShape::Box(_) => 12,
                WorldShape::Line(_) => 1,
            })
            .sum()
    }

    /// A line's ends, colour and width, for exact comparisons.
    fn ends(shape: &WorldShape) -> ([f64; 3], [f64; 3], u32, f32) {
        match shape {
            WorldShape::Line(l) => (
                [l.start.x, l.start.y, l.start.z],
                [l.end.x, l.end.y, l.end.z],
                l.color,
                l.width,
            ),
            WorldShape::Box(_) => panic!("expected a line"),
        }
    }

    #[test]
    fn api_level_is_eleven() {
        // Newest imported function is `render.set-world` (level 11);
        // `entity.targeted` is level 10, the rest level 0.
        assert_eq!(API_LEVEL, 11);
    }

    #[test]
    fn box_is_the_mob_s_hitbox() {
        // A mob 0.5 wide, 2.0 tall at (0.5, 64, 5.5): the box is its feet
        // out half a width either side, its height up.
        let mut m = mob(1, 0.5, 64.0, 5.5);
        m.width = 0.5;
        m.height = 2.0;
        let shape = mob_box(&m, 0xFF00_E5FF, 2.5);
        match shape {
            WorldShape::Box(b) => {
                assert_eq!([b.min.x, b.min.y, b.min.z], [0.25, 64.0, 5.25]);
                assert_eq!([b.max.x, b.max.y, b.max.z], [0.75, 66.0, 5.75]);
                assert_eq!(b.color, 0xFF00_E5FF);
                assert_eq!(b.width, 2.5);
            }
            WorldShape::Line(_) => panic!("expected a box"),
        }
    }

    #[test]
    fn eye_band_sits_at_85_percent_of_the_height() {
        // The 2.0-tall mob's eye is 64 + 0.85 * 2.0 = 65.7.
        let mut m = mob(1, 0.5, 64.0, 5.5);
        m.width = 0.5;
        m.height = 2.0;
        let band = eye_band(&m, 2.5);
        assert_eq!(band.len(), 4);
        for shape in &band {
            let (start, end, color, width) = ends(shape);
            assert_eq!(start[1], 65.7);
            assert_eq!(end[1], 65.7);
            assert_eq!(color, EYE_RED);
            assert_eq!(width, 2.5);
        }
        // The four lines are the rectangle round the box, in order.
        assert_eq!(ends(&band[0]).0, [0.25, 65.7, 5.25]);
        assert_eq!(ends(&band[0]).1, [0.75, 65.7, 5.25]);
        assert_eq!(ends(&band[1]).1, [0.75, 65.7, 5.75]);
        assert_eq!(ends(&band[2]).1, [0.25, 65.7, 5.75]);
        assert_eq!(ends(&band[3]).1, [0.25, 65.7, 5.25]);
    }

    #[test]
    fn eye_ray_runs_two_blocks_along_the_yaw() {
        // Yaw 0 faces south (+z): the shaft runs from the eye (0.5,
        // 65.7, 5.5) to (0.5, 65.7, 7.5), and the four tips sit 0.2 back
        // from the end — up, down, and either side.
        let mut m = mob(1, 0.5, 64.0, 5.5);
        m.width = 0.5;
        m.height = 2.0;
        let ray = look_arrow(&m, 2.5);
        assert_eq!(ray.len(), 5);
        let (start, end, color, width) = ends(&ray[0]);
        assert_eq!(start, [0.5, 65.7, 5.5]);
        assert_eq!(end, [0.5, 65.7, 7.5]);
        assert_eq!(color, RAY_BLUE);
        assert_eq!(width, 2.5);
        let tips: Vec<([f64; 3], [f64; 3])> =
            ray[1..].iter().map(|s| (ends(s).0, ends(s).1)).collect();
        assert_eq!(
            tips,
            [
                ([0.5, 65.9, 7.3], [0.5, 65.7, 7.5]),
                ([0.5, 65.5, 7.3], [0.5, 65.7, 7.5]),
                ([0.3, 65.7, 7.3], [0.5, 65.7, 7.5]),
                ([0.7, 65.7, 7.3], [0.5, 65.7, 7.5]),
            ]
        );
    }

    #[test]
    fn eye_ray_turns_with_the_yaw() {
        // Yaw 90 faces west (-x): the shaft runs to (0.5 - 2, 65.7,
        // 5.5), and the side tips spread along z instead of x.
        let mut m = mob(1, 0.5, 64.0, 5.5);
        m.width = 0.5;
        m.height = 2.0;
        m.yaw = 90.0;
        let ray = look_arrow(&m, 2.5);
        let (start, end, _, _) = ends(&ray[0]);
        assert_eq!(start, [0.5, 65.7, 5.5]);
        assert_eq!(end, [-1.5, 65.7, 5.5]);
        let tips: Vec<[f64; 3]> = ray[1..].iter().map(|s| ends(s).0).collect();
        assert_eq!(
            tips,
            [
                [-1.3, 65.9, 5.5],
                [-1.3, 65.5, 5.5],
                [-1.3, 65.7, 5.3],
                [-1.3, 65.7, 5.7],
            ]
        );
    }

    #[test]
    fn reach_is_4_5_for_a_spear_and_3_otherwise() {
        assert_eq!(
            reach_for(Some(input::HeldItem {
                id: "minecraft:spear".into(),
                count: 1,
                slot: 0,
            })),
            4.5
        );
        assert_eq!(
            reach_for(Some(input::HeldItem {
                id: "minecraft:netherite_sword".into(),
                count: 1,
                slot: 0,
            })),
            3.0
        );
        assert_eq!(reach_for(None), 3.0);
    }

    #[test]
    fn target_draws_first_in_the_in_reach_colour() {
        // The target is second in the list, past reach for the others —
        // it still draws first, green, and once.
        let mut near = mob(1, 0.5, 64.0, 2.5);
        near.width = 0.5;
        near.height = 2.0;
        let mut far = mob(2, 3.5, 64.0, 8.5);
        far.width = 0.5;
        far.height = 2.0;
        let shapes = build_shapes(
            &[near.clone(), far.clone()],
            Some(&target(far.clone(), 5.0)),
            0xFF00_E5FF,
            2.5,
            true,
        );
        // Two mobs at 10 shapes each (one box, four band lines, five
        // arrow lines), 42 lines in all.
        assert_eq!(shapes.len(), 20);
        assert_eq!(line_count(&shapes), 42);
        match &shapes[0] {
            WorldShape::Box(b) => {
                assert_eq!(b.color, IN_REACH_COLOR);
                assert_eq!(b.min.x, 3.25);
            }
            WorldShape::Line(_) => panic!("expected a box"),
        }
        // The near mob's box is the second box, in the style colour.
        match &shapes[10] {
            WorldShape::Box(b) => {
                assert_eq!(b.color, 0xFF00_E5FF);
                assert_eq!(b.min.x, 0.25);
            }
            WorldShape::Line(_) => panic!("expected a box"),
        }
    }

    #[test]
    fn the_line_cap_bounds_how_many_mobs_draw() {
        // 300 mobs, eye ray on: 21 lines each, so 48 draw (48 * 21 =
        // 1008, one more would pass 1024). With it off: 16 each, 64
        // draw (64 * 16 = 1024 exactly).
        let mobs: Vec<Entity> = (0..300).map(|i| mob(i, f64::from(i), 64.0, 5.5)).collect();
        let on = build_shapes(&mobs, None, 0xFF00_E5FF, 2.5, true);
        assert_eq!(on.len(), 48 * 10);
        assert_eq!(line_count(&on), 48 * 21);
        let off = build_shapes(&mobs, None, 0xFF00_E5FF, 2.5, false);
        assert_eq!(off.len(), 64 * 5);
        assert_eq!(line_count(&off), 64 * 16);
        // The target still draws first even past the cap: it is the
        // first box, green, ahead of the nearest mobs.
        let capped = build_shapes(
            &mobs,
            Some(&target(mob(999, 100.5, 64.0, 5.5), 2.0)),
            0xFF00_E5FF,
            2.5,
            true,
        );
        assert_eq!(capped.len(), 48 * 10);
        assert_eq!(line_count(&capped), 48 * 21);
        match &capped[0] {
            WorldShape::Box(b) => assert_eq!(b.color, IN_REACH_COLOR),
            WorldShape::Line(_) => panic!("expected a box"),
        }
    }

    #[test]
    fn sprint_text_follows_the_keys_held() {
        assert_eq!(sprint_text(true, false), Some("[Sprinting (Key)]"));
        assert_eq!(sprint_text(false, true), Some("[Sneaking (Key)]"));
        assert_eq!(sprint_text(true, true), Some("[Sprinting (Key)]"));
        assert_eq!(sprint_text(false, false), None);
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert!(read_bool(Some(b"true".to_vec()), false));
        assert_eq!(read_index(None, 4, 0), 0);
        assert_eq!(read_index(Some(b"2".to_vec()), 4, 0), 2);
        assert_eq!(read_index(Some(b"1".to_vec()), 3, 1), 1);
        // Out of range falls back to the default.
        assert_eq!(read_index(Some(b"9".to_vec()), 4, 0), 0);
        assert_eq!(read_index(Some(b"nope".to_vec()), 3, 1), 1);
        assert_eq!(STYLES[0].0, "Neon Cyan");
        assert_eq!(STYLES[0].1, 0xFF00_E5FF);
        assert_eq!(STYLES[3].1, 0xFFFF_FFFF);
        assert_eq!(THICKNESSES[1], (2.5, "Normal (2.5px)"));
        assert_eq!(on_off(true), "ON");
        assert_eq!(on_off(false), "OFF");
        assert_eq!(
            CORNER_NAMES,
            ["Top-Left", "Top-Right", "Bottom-Left", "Bottom-Right"]
        );
        assert_eq!(CORNERS[0], Corner::TopLeft);
        assert_eq!(CORNERS[3], Corner::BottomRight);
    }
}

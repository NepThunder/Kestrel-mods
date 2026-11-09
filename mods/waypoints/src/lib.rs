//! Waypoints for Kestrel: N creates a waypoint where you stand, M lists
//! them, and every visible waypoint of your dimension shows a box and a
//! beam in the world, its name and distance on the HUD panel, and an
//! in-world label with its name and distance at its position. Dying can
//! leave a red Death waypoint where it happened.
//!
//! Behaviour ported from Waypoints by Nirokx (MIT, Copyright (c) 2026
//! Nirokx, <https://github.com/Nirokx/horizon-waypoints>): the storage
//! format, the 16-colour palette, the distance wording, the death
//! waypoint rules (newest five stay, renaming keeps one for good) and the
//! list's All / Overworld / Nether / End filters are the original's,
//! adapted to the SDK's storage. The rest is reimplemented against the
//! Kestrel SDK; no original code runs. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/waypoints>.
//!
//! What the SDK cannot do, the original's native build can: the editor
//! with its text fields (the SDK has no text input widget, so N creates
//! a waypoint at once with a suggested name and `/wp` renames,
//! moves and recolours) and per-world storage (the SDK exposes no world
//! folder or server address, so every world shares one list, filtered by
//! dimension). In-world name/distance labels are HUD text placed by
//! `render.project` (API level 14): there is no world-text capability,
//! so a label shows through walls, as the panel's distance already does.
//! Death waypoints are inferred from the player's health reaching zero,
//! not from a death event, which the SDK does not expose.
use horizon_mod_sdk::{
    Mod, Registration, chat,
    game::{self, Player},
    hud::{self, Corner, Line},
    input::{self, Event},
    render::{
        self, Point, ScreenPoint, Shape, TextShape, WorldBox, WorldLine, WorldPoint, WorldShape,
    },
    storage,
    ui::{self, ButtonWidget, ListWidget, Screen, TextWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below:
/// `render.project` is level 14, `render.set-world` level 11,
/// `input.register-key` level 6 (see
/// `crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 14;

/// The keys' names for `register-key`, and their defaults.
const CREATE_KEY: &str = "create";
const LIST_KEY: &str = "list";
const DEFAULT_CREATE: &str = "key.keyboard.n";
const DEFAULT_LIST: &str = "key.keyboard.m";

const OVERWORLD: &str = "minecraft:overworld";
const NETHER: &str = "minecraft:the_nether";
const END: &str = "minecraft:the_end";

/// The colours a waypoint can have: Minecraft's text colours and a brown,
/// as the original's palette.
const PALETTE: [u32; 16] = [
    0xFF5555, 0xFFAA00, 0xFFFF55, 0x55FF55, 0x00AA00, 0x55FFFF, 0x00AAAA, 0x5555FF, 0x0000AA,
    0xAA00AA, 0xFF55FF, 0xFFFFFF, 0xAAAAAA, 0x555555, 0xAA5500, 0x000000,
];

/// The palette's names, in the same order, for `/wp color`.
const PALETTE_NAMES: [&str; 16] = [
    "red",
    "orange",
    "yellow",
    "lime",
    "green",
    "cyan",
    "aqua",
    "blue",
    "dark-blue",
    "purple",
    "magenta",
    "white",
    "grey",
    "dark-grey",
    "brown",
    "black",
];

/// A death waypoint's colour, and how many the mod keeps: the oldest goes.
const DEATH_COLOR: u32 = 0xFF5555;
const DEATH_MAX: usize = 5;

const NAME_MAX: usize = 32;
/// The list is one storage value under a 4 MiB quota; 512 waypoints is
/// far under it and more than a list screen can show usefully.
const MAX_WAYPOINTS: usize = 512;
/// Each marker is 13 lines (a 12-edge box plus a beam) against the
/// 1024-line per-mod budget: 78 markers is 1014 lines.
const MAX_MARKERS: usize = 78;
/// How far a waypoint's beam reaches above its block, in blocks.
const BEAM_HEIGHT: f64 = 32.0;
/// How many in-world labels at most: the same nearest sixteen the HUD
/// panel lists, so a tick projects at most sixteen points.
const MAX_LABELS: usize = 16;
/// How far above a waypoint's block centre its label floats, in blocks:
/// just above its box.
const LABEL_RAISE: f64 = 1.0;
const BOX_WIDTH: f32 = 2.0;
const BEAM_WIDTH: f32 = 2.0;

/// The storage keys. Keys are `[a-z0-9_.-]{1,64}`.
const WAYPOINTS_KEY: &str = "waypoints";
const MARKERS_KEY: &str = "markers";
const BEAM_KEY: &str = "beam";
const PANEL_KEY: &str = "panel";
const DEATHS_KEY: &str = "deaths";

/// The list widget, and the buttons of the list screen.
const LIST_WIDGET: u32 = 1;
const FILTER_ALL: u32 = 10;
const FILTER_OVERWORLD: u32 = 11;
const FILTER_NETHER: u32 = 12;
const FILTER_END: u32 = 13;
const NEW_BUTTON: u32 = 20;
const HIDE_BUTTON: u32 = 30;
const COLOUR_BUTTON: u32 = 31;
const MOVE_BUTTON: u32 = 32;
const DELETE_BUTTON: u32 = 33;

/// The settings page's buttons, on their own ids so a click there can
/// never be mistaken for a click on the list screen.
const MARKERS_BUTTON: u32 = 1000;
const BEAM_BUTTON: u32 = 1001;
const PANEL_BUTTON: u32 = 1002;
const DEATHS_BUTTON: u32 = 1003;

/// One waypoint. `pos` is a block position; `color` is `0xRRGGBB`.
#[derive(Clone, Debug, PartialEq)]
pub struct Waypoint {
    pub name: String,
    pub pos: [i32; 3],
    pub dimension: String,
    pub color: u32,
    pub hidden: bool,
    /// Made by a death: the oldest of these go, unless renamed.
    pub death: bool,
}

/// A dimension and a block, as a death is remembered.
pub type Spot = (String, [i32; 3]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Overworld,
    Nether,
    End,
}

impl Filter {
    pub const ALL: [Filter; 4] = [Filter::All, Filter::Overworld, Filter::Nether, Filter::End];

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Overworld => "Overworld",
            Filter::Nether => "Nether",
            Filter::End => "End",
        }
    }

    pub fn matches(self, dimension: &str) -> bool {
        match self {
            Filter::All => true,
            Filter::Overworld => dimension == OVERWORLD,
            Filter::Nether => dimension == NETHER,
            Filter::End => dimension == END,
        }
    }
}

/// What `/wp ...` asks for, parsed from the line after the name.
#[derive(Clone, Debug, PartialEq)]
enum Command {
    Add(Option<String>),
    List,
    Rename(usize, String),
    Delete(usize),
    Hide(usize),
    Show(usize),
    Color(usize, u32),
    Move(usize),
    Dim(usize, String),
    Help,
}

/// What a command did: its feedback, and whether the list screen opens.
struct Outcome {
    feedback: String,
    open_list: bool,
}

/// What a widget click did: push the list screen again, refresh the
/// settings page, or neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Select {
    Screen,
    Settings,
    None,
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// One line per waypoint: name, x, y, z, dimension, colour, hidden, death.
/// The original's format, so a file it wrote would read back here.
pub fn encode(list: &[Waypoint]) -> String {
    let mut out = String::from("# waypoints 1\n");
    for w in list {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{:06X}\t{}\t{}\n",
            escape(&w.name),
            w.pos[0],
            w.pos[1],
            w.pos[2],
            w.dimension,
            w.color & 0xFF_FFFF,
            u8::from(w.hidden),
            u8::from(w.death)
        ));
    }
    out
}

/// What `encode` wrote; broken lines are skipped.
pub fn decode(text: &str) -> Vec<Waypoint> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 7 {
                return None;
            }
            Some(Waypoint {
                name: unescape(f[0]),
                pos: [f[1].parse().ok()?, f[2].parse().ok()?, f[3].parse().ok()?],
                dimension: f[4].to_owned(),
                color: u32::from_str_radix(f[5], 16).ok()? & 0xFF_FFFF,
                hidden: f[6].trim() == "1",
                death: f.get(7).is_some_and(|v| v.trim() == "1"),
            })
        })
        .collect()
}

pub fn dimension_label(id: &str) -> String {
    match id {
        OVERWORLD => "Overworld".into(),
        NETHER => "Nether".into(),
        END => "The End".into(),
        other => other.rsplit(':').next().unwrap_or(other).replace('_', " "),
    }
}

pub fn dimension_color(id: &str) -> u32 {
    match id {
        OVERWORLD => 0x55FF55,
        NETHER => 0xFF5555,
        END => 0xD27BFF,
        _ => 0xAAAAAA,
    }
}

/// `12 m`, then `1.2 km` from a kilometre on.
pub fn distance_text(d: f64) -> String {
    if d < 999.5 {
        format!("{} m", d.round() as i64)
    } else {
        format!("{:.1} km", d / 1000.0)
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The waypoint's block centre.
fn centre(w: &Waypoint) -> [f64; 3] {
    [
        f64::from(w.pos[0]) + 0.5,
        f64::from(w.pos[1]) + 0.5,
        f64::from(w.pos[2]) + 0.5,
    ]
}

/// The player's feet, as distances are measured from them.
fn feet(player: &Player) -> [f64; 3] {
    [player.x, player.y, player.z]
}

/// The palette colour a name or hex spells, if it spells one.
pub fn parse_color(s: &str) -> Option<u32> {
    let lower = s.to_ascii_lowercase();
    if lower == "gray" {
        return Some(0xAAAAAA);
    }
    if let Some(i) = PALETTE_NAMES.iter().position(|n| *n == lower) {
        return Some(PALETTE[i]);
    }
    let hex = lower.strip_prefix('#').unwrap_or(&lower);
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        None
    }
}

/// The palette name of a colour, or its hex.
pub fn color_name(color: u32) -> String {
    match PALETTE.iter().position(|c| *c == color) {
        Some(i) => PALETTE_NAMES[i].to_string(),
        None => format!("#{:06X}", color & 0xFF_FFFF),
    }
}

/// The palette colour after this one, wrapping round.
pub fn next_color(color: u32) -> u32 {
    match PALETTE.iter().position(|c| *c == color) {
        Some(i) => PALETTE[(i + 1) % PALETTE.len()],
        None => PALETTE[0],
    }
}

/// A stored on/off value: on unless it says `false`.
fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

fn on_label(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

struct Waypoints {
    waypoints: Vec<Waypoint>,
    filter: Filter,
    selected: Option<usize>,
    /// The row whose Delete was clicked once and asks to be clicked again.
    confirm_delete: Option<usize>,
    markers: bool,
    beam: bool,
    panel: bool,
    deaths: bool,
    /// Where the player last stood alive: a death marks this block.
    last_alive: Option<Spot>,
    /// A death has been marked for this fall to zero health already.
    death_recorded: bool,
    /// The panel's lines last sent (text and colour), so an unchanged one
    /// costs no call.
    last_panel: Vec<(String, u32)>,
    /// The labels last sent (text, colour, x, y), so an unchanged set
    /// costs no call.
    last_labels: Vec<(String, u32, f32, f32)>,
}

impl Waypoints {
    fn default_name(&self) -> String {
        let mut n = self.waypoints.len() + 1;
        while self
            .waypoints
            .iter()
            .any(|w| w.name == format!("Waypoint {n}"))
        {
            n += 1;
        }
        format!("Waypoint {n}")
    }

    /// The next `Death n`, counting on from the newest kept.
    fn death_name(&self) -> String {
        let last = self
            .waypoints
            .iter()
            .filter(|w| w.death)
            .filter_map(|w| w.name.strip_prefix("Death ")?.parse::<u32>().ok())
            .max();
        format!("Death {}", last.unwrap_or(0) + 1)
    }

    /// A waypoint at `spot`, unless one of the deaths is already there; the
    /// oldest go past `DEATH_MAX`. Says whether it made one.
    fn add_death(&mut self, on: bool, (dimension, pos): Spot) -> bool {
        let same = |w: &Waypoint| w.death && w.pos == pos && w.dimension == dimension;
        if !on || self.waypoints.iter().any(same) {
            return false;
        }
        self.waypoints.push(Waypoint {
            name: self.death_name(),
            pos,
            dimension,
            color: DEATH_COLOR,
            hidden: false,
            death: true,
        });
        let deaths = self.waypoints.iter().filter(|w| w.death).count();
        let mut extra = deaths.saturating_sub(DEATH_MAX);
        self.waypoints.retain(|w| {
            let gone = w.death && extra > 0;
            extra -= usize::from(gone);
            !gone
        });
        // The rows moved: a form or a Delete open on one would hit another.
        self.confirm_delete = None;
        self.selected = None;
        true
    }

    /// Creates a waypoint; `name` is the suggested name when absent.
    /// Errors when the list is full.
    fn create(
        &mut self,
        name: Option<String>,
        pos: [i32; 3],
        dimension: String,
    ) -> Result<usize, String> {
        if self.waypoints.len() >= MAX_WAYPOINTS {
            return Err(format!(
                "Waypoints are full ({MAX_WAYPOINTS}): delete one first"
            ));
        }
        let name = match name {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => self.default_name(),
        };
        let name = name.chars().take(NAME_MAX).collect();
        let color = PALETTE[self.waypoints.len() % PALETTE.len()];
        self.waypoints.push(Waypoint {
            name,
            pos,
            dimension,
            color,
            hidden: false,
            death: false,
        });
        Ok(self.waypoints.len() - 1)
    }

    /// The 1-based number a command or the list shows, as an index.
    fn index(number: usize) -> Result<usize, String> {
        number
            .checked_sub(1)
            .ok_or_else(|| "Waypoint numbers start at 1".to_string())
    }

    fn get(&self, number: usize) -> Result<usize, String> {
        let i = Self::index(number)?;
        if i < self.waypoints.len() {
            Ok(i)
        } else {
            Err(format!("No waypoint {number}"))
        }
    }

    fn rename(&mut self, number: usize, name: String) -> Result<(), String> {
        let i = self.get(number)?;
        let name = name.trim().chars().take(NAME_MAX).collect::<String>();
        if name.is_empty() {
            return Err("A waypoint needs a name".into());
        }
        // Renaming a death waypoint keeps it for good.
        let was_death = self.waypoints[i].death;
        self.waypoints[i].name = name;
        if was_death {
            self.waypoints[i].death = false;
        }
        Ok(())
    }

    fn remove(&mut self, number: usize) -> Result<Waypoint, String> {
        let i = self.get(number)?;
        self.confirm_delete = None;
        // The rows moved: a selection open on one would hit another.
        self.selected = None;
        Ok(self.waypoints.remove(i))
    }

    fn set_hidden(&mut self, number: usize, hidden: bool) -> Result<(), String> {
        let i = self.get(number)?;
        self.waypoints[i].hidden = hidden;
        Ok(())
    }

    fn set_color(&mut self, number: usize, color: u32) -> Result<(), String> {
        let i = self.get(number)?;
        self.waypoints[i].color = color & 0xFF_FFFF;
        Ok(())
    }

    fn move_to(&mut self, number: usize, pos: [i32; 3], dimension: String) -> Result<(), String> {
        let i = self.get(number)?;
        self.waypoints[i].pos = pos;
        self.waypoints[i].dimension = dimension;
        Ok(())
    }

    fn set_dimension(&mut self, number: usize, dimension: String) -> Result<(), String> {
        let i = self.get(number)?;
        self.waypoints[i].dimension = dimension;
        Ok(())
    }

    /// The visible waypoints of this dimension, nearest first, as
    /// (distance, index).
    fn visible(&self, player: &Player) -> Vec<(f64, usize)> {
        let here = feet(player);
        let mut shown: Vec<(f64, usize)> = self
            .waypoints
            .iter()
            .enumerate()
            .filter(|(_, w)| !w.hidden && w.dimension == player.dimension)
            .map(|(i, w)| (distance(here, centre(w)), i))
            .collect();
        shown.sort_by(|a, b| a.0.total_cmp(&b.0));
        shown
    }

    /// The HUD panel's lines: name and distance, nearest first, at most
    /// the panel's 16 lines.
    fn panel_lines(&self, player: &Player) -> Vec<Line> {
        self.visible(player)
            .into_iter()
            .take(16)
            .map(|(d, i)| {
                let w = &self.waypoints[i];
                Line {
                    text: format!("{} · {}", w.name, distance_text(d)),
                    color: w.color,
                }
            })
            .collect()
    }

    /// The world markers: a block-sized box at each visible waypoint of
    /// this dimension, nearest first, plus a beam above it when beams are
    /// on. At most `MAX_MARKERS` fit the 1024-line budget.
    fn marker_shapes(&self, dimension: &str, player: &Player) -> Vec<WorldShape> {
        let here = feet(player);
        let mut shown: Vec<(f64, &Waypoint)> = self
            .waypoints
            .iter()
            .filter(|w| !w.hidden && w.dimension == dimension)
            .map(|w| (distance(here, centre(w)), w))
            .collect();
        shown.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut shapes = Vec::new();
        for (_, w) in shown.into_iter().take(MAX_MARKERS) {
            let argb = 0xFF00_0000 | w.color;
            let min = WorldPoint {
                x: f64::from(w.pos[0]),
                y: f64::from(w.pos[1]),
                z: f64::from(w.pos[2]),
            };
            let max = WorldPoint {
                x: f64::from(w.pos[0] + 1),
                y: f64::from(w.pos[1] + 1),
                z: f64::from(w.pos[2] + 1),
            };
            shapes.push(WorldShape::Box(WorldBox {
                min,
                max,
                color: argb,
                width: BOX_WIDTH,
            }));
            if self.beam {
                let cx = f64::from(w.pos[0]) + 0.5;
                let cz = f64::from(w.pos[2]) + 0.5;
                shapes.push(WorldShape::Line(WorldLine {
                    start: WorldPoint {
                        x: cx,
                        y: f64::from(w.pos[1]),
                        z: cz,
                    },
                    end: WorldPoint {
                        x: cx,
                        y: f64::from(w.pos[1]) + BEAM_HEIGHT,
                        z: cz,
                    },
                    color: argb,
                    width: BEAM_WIDTH,
                }));
            }
        }
        shapes
    }

    /// The label's anchor: the block centre raised above its box.
    fn label_point(w: &Waypoint) -> WorldPoint {
        let c = centre(w);
        WorldPoint {
            x: c[0],
            y: c[1] + LABEL_RAISE,
            z: c[2],
        }
    }

    /// The in-world labels: name and distance as HUD text at the
    /// projected position of each visible waypoint of this dimension,
    /// nearest first, at most `MAX_LABELS`. A waypoint behind the camera
    /// projects with `visible` false and gets no label. There is no
    /// occlusion test: a label shows through walls, as the panel's
    /// distance already does.
    fn label_shapes(
        &self,
        player: &Player,
        project: impl Fn(WorldPoint) -> ScreenPoint,
    ) -> Vec<Shape> {
        self.visible(player)
            .into_iter()
            .take(MAX_LABELS)
            .filter_map(|(d, i)| {
                let w = &self.waypoints[i];
                let s = project(Self::label_point(w));
                if !s.visible {
                    return None;
                }
                Some(Shape::Text(TextShape {
                    text: format!("{} · {}", w.name, distance_text(d)),
                    at: Point { x: s.x, y: s.y },
                    color: w.color,
                    centered: true,
                    shadow: true,
                }))
            })
            .collect()
    }

    /// The labels as (text, colour, x, y), for change detection.
    fn label_state(shapes: &[Shape]) -> Vec<(String, u32, f32, f32)> {
        shapes
            .iter()
            .map(|s| match s {
                Shape::Text(t) => (t.text.clone(), t.color, t.at.x, t.at.y),
                _ => unreachable!("labels are text shapes"),
            })
            .collect()
    }

    /// The filtered rows, as waypoint indices in list order.
    fn filtered(&self) -> Vec<usize> {
        self.waypoints
            .iter()
            .enumerate()
            .filter(|(_, w)| self.filter.matches(&w.dimension))
            .map(|(i, _)| i)
            .collect()
    }

    /// One row of the list: number, name, coordinates, dimension and
    /// distance, or "other dimension" when the player is elsewhere.
    fn row(&self, index: usize, player: Option<&Player>) -> String {
        let w = &self.waypoints[index];
        let dist = match player {
            Some(p) if p.dimension == w.dimension => distance_text(distance(feet(p), centre(w))),
            _ => "other dimension".into(),
        };
        let hidden = if w.hidden { " [hidden]" } else { "" };
        format!(
            "{}. {}{} · {} {} {} · {} · {}",
            index + 1,
            w.name,
            hidden,
            w.pos[0],
            w.pos[1],
            w.pos[2],
            dimension_label(&w.dimension),
            dist
        )
    }

    /// The list screen: filter buttons, the rows, and — for the selected
    /// row — Hide/Show, colour, Move here and Delete.
    fn screen(&self, player: Option<&Player>) -> Screen {
        let mut widgets = Vec::new();
        for (n, filter) in Filter::ALL.iter().enumerate() {
            let on = self.filter == *filter;
            widgets.push(Widget::Button(ButtonWidget {
                id: FILTER_ALL + n as u32,
                label: if on {
                    format!("✓ {}", filter.label())
                } else {
                    filter.label().to_string()
                },
            }));
        }
        let rows = self.filtered();
        if rows.is_empty() {
            let msg = if self.waypoints.is_empty() {
                "No waypoints yet. Press N in game or click + New."
            } else {
                "No waypoints in this dimension."
            };
            widgets.push(Widget::Text(TextWidget {
                id: 0,
                text: msg.to_string(),
                color: 0xAAAAAA,
            }));
        } else {
            let selected = self
                .selected
                .and_then(|i| rows.iter().position(|r| *r == i));
            widgets.push(Widget::List(ListWidget {
                id: LIST_WIDGET,
                rows: rows.iter().map(|&i| self.row(i, player)).collect(),
                selected: selected.map(|p| p as u32),
            }));
        }
        if let Some(i) = self.selected.filter(|&i| i < self.waypoints.len()) {
            let w = &self.waypoints[i];
            let detail = match player {
                Some(p) if p.dimension == w.dimension => {
                    format!(
                        "{} · {}",
                        dimension_label(&w.dimension),
                        distance_text(distance(feet(p), centre(w)))
                    )
                }
                _ => dimension_label(&w.dimension),
            };
            widgets.push(Widget::Text(TextWidget {
                id: 2,
                text: format!(
                    "{} — {} {} {} · {detail}",
                    w.name, w.pos[0], w.pos[1], w.pos[2]
                ),
                color: w.color,
            }));
            widgets.push(Widget::Button(ButtonWidget {
                id: HIDE_BUTTON,
                label: if w.hidden {
                    "Show".into()
                } else {
                    "Hide".into()
                },
            }));
            widgets.push(Widget::Button(ButtonWidget {
                id: COLOUR_BUTTON,
                label: format!("Colour: {}", color_name(w.color)),
            }));
            widgets.push(Widget::Button(ButtonWidget {
                id: MOVE_BUTTON,
                label: "Move here".into(),
            }));
            let delete = if self.confirm_delete == Some(i) {
                "Sure?"
            } else {
                "Delete"
            };
            widgets.push(Widget::Button(ButtonWidget {
                id: DELETE_BUTTON,
                label: delete.into(),
            }));
            widgets.push(Widget::Text(TextWidget {
                id: 3,
                text: format!("Rename: /wp rename {} <name>", i + 1),
                color: 0xAAAAAA,
            }));
        }
        widgets.push(Widget::Button(ButtonWidget {
            id: NEW_BUTTON,
            label: "+ New".into(),
        }));
        Screen {
            title: format!("Waypoints ({})", self.waypoints.len()),
            widgets,
        }
    }

    /// The settings page's buttons.
    fn settings_widgets(&self) -> Vec<Widget> {
        [
            (
                MARKERS_BUTTON,
                format!("Markers in the world: {}", on_label(self.markers)),
            ),
            (BEAM_BUTTON, format!("Beams: {}", on_label(self.beam))),
            (
                PANEL_BUTTON,
                format!("Distance panel: {}", on_label(self.panel)),
            ),
            (
                DEATHS_BUTTON,
                format!("Waypoint on death: {}", on_label(self.deaths)),
            ),
        ]
        .map(|(id, label)| Widget::Button(ButtonWidget { id, label }))
        .to_vec()
    }

    /// The death check for one tick. The SDK exposes no death event, so a
    /// death is inferred from the player's health reaching zero: the
    /// waypoint marks where the player last stood alive. Says whether a
    /// waypoint was added.
    fn track_death(&mut self, player: &Player) -> bool {
        if player.health > 0.0 {
            self.last_alive = Some((
                player.dimension.clone(),
                [
                    player.x.floor() as i32,
                    player.y.floor() as i32,
                    player.z.floor() as i32,
                ],
            ));
            self.death_recorded = false;
            false
        } else if !self.death_recorded {
            self.death_recorded = true;
            match &self.last_alive {
                Some(spot) => self.add_death(self.deaths, spot.clone()),
                None => false,
            }
        } else {
            false
        }
    }

    /// Parses the line after `/wp`.
    fn parse_command(args: &str) -> Result<Command, String> {
        let mut parts = args.split_whitespace();
        let verb = parts.next().unwrap_or("help");
        let rest: Vec<&str> = parts.collect();
        let number = |at: usize| -> Result<usize, String> {
            rest.get(at)
                .ok_or_else(|| "A waypoint number is needed".to_string())
                .and_then(|s| {
                    s.parse::<usize>()
                        .map_err(|_| format!("{s} is not a waypoint number"))
                })
        };
        match verb {
            "add" => Ok(Command::Add({
                let name = rest.join(" ");
                if name.is_empty() { None } else { Some(name) }
            })),
            "list" => Ok(Command::List),
            "rename" => {
                let n = number(0)?;
                let name = rest[1..].join(" ");
                if name.is_empty() {
                    return Err("A new name is needed".into());
                }
                Ok(Command::Rename(n, name))
            }
            "delete" => Ok(Command::Delete(number(0)?)),
            "hide" => Ok(Command::Hide(number(0)?)),
            "show" => Ok(Command::Show(number(0)?)),
            "color" => {
                let n = number(0)?;
                let colour = rest
                    .get(1)
                    .ok_or_else(|| "A colour is needed".to_string())
                    .and_then(|s| parse_color(s).ok_or_else(|| format!("Unknown colour {s}")))?;
                Ok(Command::Color(n, colour))
            }
            "move" => Ok(Command::Move(number(0)?)),
            "dim" => {
                let n = number(0)?;
                let dim = match rest.get(1).copied() {
                    Some("overworld") => OVERWORLD,
                    Some("nether") => NETHER,
                    Some("end") => END,
                    Some(other) => return Err(format!("Unknown dimension {other}")),
                    None => return Err("A dimension is needed: overworld, nether or end".into()),
                };
                Ok(Command::Dim(n, dim.to_string()))
            }
            "help" => Ok(Command::Help),
            other => Err(format!(
                "Unknown waypoint command {other}: add, list, rename, delete, hide, show, color, move, dim"
            )),
        }
    }

    /// Runs a parsed command. The player is needed for `add`, `move` and
    /// `list`; the rest work from the list alone.
    fn apply_command(
        &mut self,
        command: Command,
        player: Option<&Player>,
    ) -> Result<Outcome, String> {
        let done = |feedback: String| Outcome {
            feedback,
            open_list: false,
        };
        match command {
            Command::Help => Ok(done(
                "Waypoints: add [name] | list | rename <n> <name> | delete <n> | hide <n> | show <n> | color <n> <colour> | move <n> | dim <n> <overworld|nether|end>".into(),
            )),
            Command::Add(name) => {
                let Some(p) = player else {
                    return Err("No world is open".into());
                };
                let pos = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
                let i = self.create(name, pos, p.dimension.clone())?;
                let w = &self.waypoints[i];
                Ok(done(format!(
                    "Waypoint \"{}\" created at {} {} {} ({})",
                    w.name,
                    w.pos[0],
                    w.pos[1],
                    w.pos[2],
                    dimension_label(&w.dimension)
                )))
            }
            Command::List => Ok(Outcome {
                feedback: String::new(),
                open_list: true,
            }),
            Command::Rename(n, name) => {
                self.rename(n, name)?;
                Ok(done(format!("Waypoint {n} renamed to \"{}\"", self.waypoints[Self::index(n)?].name)))
            }
            Command::Delete(n) => {
                let w = self.remove(n)?;
                Ok(done(format!("Waypoint \"{}\" deleted", w.name)))
            }
            Command::Hide(n) => {
                self.set_hidden(n, true)?;
                Ok(done(format!("Waypoint {n} hidden")))
            }
            Command::Show(n) => {
                self.set_hidden(n, false)?;
                Ok(done(format!("Waypoint {n} shown")))
            }
            Command::Color(n, color) => {
                self.set_color(n, color)?;
                Ok(done(format!("Waypoint {n} is now {}", color_name(color))))
            }
            Command::Move(n) => {
                let Some(p) = player else {
                    return Err("No world is open".into());
                };
                let pos = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
                self.move_to(n, pos, p.dimension.clone())?;
                Ok(done(format!("Waypoint {n} moved here")))
            }
            Command::Dim(n, dimension) => {
                self.set_dimension(n, dimension.clone())?;
                Ok(done(format!(
                    "Waypoint {n} is now in {}",
                    dimension_label(&dimension)
                )))
            }
        }
    }

    /// A widget click, on the list screen or the settings page. Says what
    /// the shell should do next.
    fn select(&mut self, id: u32, row: u32, player: Option<&Player>) -> Select {
        match id {
            MARKERS_BUTTON => {
                self.markers = !self.markers;
                Select::Settings
            }
            BEAM_BUTTON => {
                self.beam = !self.beam;
                Select::Settings
            }
            PANEL_BUTTON => {
                self.panel = !self.panel;
                Select::Settings
            }
            DEATHS_BUTTON => {
                self.deaths = !self.deaths;
                Select::Settings
            }
            FILTER_ALL | FILTER_OVERWORLD | FILTER_NETHER | FILTER_END => {
                self.filter = Filter::ALL[(id - FILTER_ALL) as usize];
                self.selected = None;
                self.confirm_delete = None;
                Select::Screen
            }
            LIST_WIDGET => {
                let rows = self.filtered();
                if let Some(&i) = rows.get(row as usize) {
                    self.selected = Some(i);
                    self.confirm_delete = None;
                }
                Select::Screen
            }
            NEW_BUTTON => {
                if let Some(p) = player
                    && let Ok(i) = self.create(
                        None,
                        [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32],
                        p.dimension.clone(),
                    )
                {
                    self.selected = Some(i);
                }
                Select::Screen
            }
            HIDE_BUTTON => {
                if let Some(i) = self.selected
                    && let Some(w) = self.waypoints.get_mut(i)
                {
                    w.hidden = !w.hidden;
                }
                Select::Screen
            }
            COLOUR_BUTTON => {
                if let Some(i) = self.selected
                    && let Some(w) = self.waypoints.get_mut(i)
                {
                    w.color = next_color(w.color);
                }
                Select::Screen
            }
            MOVE_BUTTON => {
                if let (Some(i), Some(p)) = (self.selected, player) {
                    let pos = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
                    if let Some(w) = self.waypoints.get_mut(i) {
                        w.pos = pos;
                        w.dimension = p.dimension.clone();
                    }
                }
                Select::Screen
            }
            DELETE_BUTTON => {
                if let Some(i) = self.selected {
                    if self.confirm_delete == Some(i) {
                        let _ = self.remove(i + 1);
                    } else {
                        self.confirm_delete = Some(i);
                    }
                }
                Select::Screen
            }
            _ => Select::None,
        }
    }

    /// The panel's lines as (text, colour), for change detection.
    fn panel_state(&self, player: &Player) -> Vec<(String, u32)> {
        self.panel_lines(player)
            .into_iter()
            .map(|l| (l.text, l.color))
            .collect()
    }
}

impl Waypoints {
    /// Writes the list and the four settings to storage.
    fn persist(&self) {
        let _ = storage::set(WAYPOINTS_KEY, encode(&self.waypoints).as_bytes());
        let _ = storage::set(MARKERS_KEY, if self.markers { b"true" } else { b"false" });
        let _ = storage::set(BEAM_KEY, if self.beam { b"true" } else { b"false" });
        let _ = storage::set(PANEL_KEY, if self.panel { b"true" } else { b"false" });
        let _ = storage::set(DEATHS_KEY, if self.deaths { b"true" } else { b"false" });
    }

    fn push_screen(&self) {
        ui::open_screen(&self.screen(game::get_player().as_ref()));
    }

    fn refresh_settings(&self) {
        ui::set_settings(&self.settings_widgets());
    }

    /// Creates a waypoint where the player stands and says so in chat.
    fn create_here(&mut self) {
        let Some(p) = game::get_player() else {
            return;
        };
        let pos = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
        match self.create(None, pos, p.dimension.clone()) {
            Ok(i) => {
                let w = &self.waypoints[i];
                chat::show(&format!(
                    "Waypoint \"{}\" created at {} {} {} ({})",
                    w.name,
                    w.pos[0],
                    w.pos[1],
                    w.pos[2],
                    dimension_label(&w.dimension)
                ));
                self.persist();
            }
            Err(e) => chat::show(&e),
        }
    }
}

impl Mod for Waypoints {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(CREATE_KEY, "Create waypoint", DEFAULT_CREATE)?;
        input::register_key(LIST_KEY, "Waypoint list", DEFAULT_LIST)?;
        let waypoints = storage::get(WAYPOINTS_KEY)
            .and_then(|v| String::from_utf8(v).ok())
            .map(|t| decode(&t))
            .unwrap_or_default();
        let state = Waypoints {
            waypoints,
            filter: Filter::All,
            selected: None,
            confirm_delete: None,
            markers: read_bool(storage::get(MARKERS_KEY), true),
            beam: read_bool(storage::get(BEAM_KEY), true),
            panel: read_bool(storage::get(PANEL_KEY), true),
            deaths: read_bool(storage::get(DEATHS_KEY), true),
            last_alive: None,
            death_recorded: false,
            last_panel: Vec::new(),
            last_labels: Vec::new(),
        };
        let hooks = Registration::new().command("wp").on_tick().on_frame();
        Ok((state, hooks))
    }

    fn tick(&mut self, _tick: u64) {
        let Some(player) = game::get_player() else {
            if !self.last_panel.is_empty() {
                hud::clear();
                self.last_panel.clear();
            }
            if !self.last_labels.is_empty() {
                render::set_hud(&[]);
                self.last_labels.clear();
            }
            render::set_world(&[]);
            self.last_alive = None;
            return;
        };
        if self.track_death(&player) {
            self.persist();
        }
        // The HUD panel: name and distance, nearest first.
        if self.panel {
            let now = self.panel_state(&player);
            if now != self.last_panel {
                if now.is_empty() {
                    hud::clear();
                } else {
                    let lines: Vec<Line> = now
                        .iter()
                        .map(|(text, color)| Line {
                            text: text.clone(),
                            color: *color,
                        })
                        .collect();
                    hud::set_panel(Corner::TopLeft, &lines);
                }
                self.last_panel = now;
            }
        } else if !self.last_panel.is_empty() {
            hud::clear();
            self.last_panel.clear();
        }
        // The world markers, submitted every tick and drawn every frame
        // until replaced.
        let shapes = if self.markers {
            self.marker_shapes(&player.dimension, &player)
        } else {
            Vec::new()
        };
        render::set_world(&shapes);
        // The in-world labels, with the markers: name and distance at
        // each visible waypoint's projected position. The projection
        // reads the last frame's camera, so a label lags the camera by
        // a frame.
        if self.markers {
            let labels = self.label_shapes(&player, render::project);
            let now = Self::label_state(&labels);
            if now != self.last_labels {
                self.last_labels = now;
                render::set_hud(&labels);
            }
        } else if !self.last_labels.is_empty() {
            self.last_labels.clear();
            render::set_hud(&[]);
        }
    }

    fn frame(&mut self, _seconds: f32) {
        for event in input::poll() {
            if let Event::KeyDown(k) = event {
                if k == CREATE_KEY {
                    self.create_here();
                } else if k == LIST_KEY {
                    self.push_screen();
                }
            }
        }
    }

    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        if name != "wp" {
            return Err("This command does nothing".into());
        }
        let command = Waypoints::parse_command(args)?;
        let player = game::get_player();
        let outcome = self.apply_command(command, player.as_ref())?;
        self.persist();
        if outcome.open_list {
            self.push_screen();
            Ok(None)
        } else if outcome.feedback.is_empty() {
            Ok(None)
        } else {
            Ok(Some(outcome.feedback))
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, row: u32) {
        let player = game::get_player();
        match self.select(id, row, player.as_ref()) {
            Select::Screen => {
                self.persist();
                self.push_screen();
            }
            Select::Settings => {
                self.persist();
                self.refresh_settings();
            }
            Select::None => {}
        }
    }

    fn ui_closed(&mut self) {
        self.selected = None;
        self.confirm_delete = None;
    }
}

horizon_mod_sdk::export_mod!(Waypoints);

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> Player {
        Player {
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

    fn state() -> Waypoints {
        Waypoints {
            waypoints: Vec::new(),
            filter: Filter::All,
            selected: None,
            confirm_delete: None,
            markers: true,
            beam: true,
            panel: true,
            deaths: true,
            last_alive: None,
            death_recorded: false,
            last_panel: Vec::new(),
            last_labels: Vec::new(),
        }
    }

    fn waypoint(name: &str, pos: [i32; 3], dimension: &str, color: u32) -> Waypoint {
        Waypoint {
            name: name.into(),
            pos,
            dimension: dimension.into(),
            color,
            hidden: false,
            death: false,
        }
    }

    #[test]
    fn api_level_is_fourteen() {
        // Newest imported function is `render.project` (level 14);
        // `render.set-world` is level 11, `input.register-key` level 6,
        // the rest level 0.
        assert_eq!(API_LEVEL, 14);
    }

    #[test]
    fn storage_round_trips_odd_names() {
        let list = vec![
            Waypoint {
                name: "Base\twith\\tab".into(),
                pos: [-12, 70, 300],
                dimension: NETHER.into(),
                color: 0xFF5555,
                hidden: true,
                death: false,
            },
            Waypoint {
                name: "Ünïcode ⛏".into(),
                pos: [0, -64, 0],
                dimension: OVERWORLD.into(),
                color: 0x00AA00,
                hidden: false,
                death: true,
            },
        ];
        assert_eq!(decode(&encode(&list)), list);
        assert!(decode("garbage\n1\t2").is_empty());
        // Files of 1.0 have no death column.
        let old = "# waypoints 1\nHome\t1\t2\t3\tminecraft:overworld\tFF5555\t0\n";
        assert!(!decode(old)[0].death);
    }

    #[test]
    fn words() {
        assert_eq!(dimension_label(NETHER), "Nether");
        assert_eq!(dimension_label(END), "The End");
        assert_eq!(dimension_label("mymod:deep_dark"), "deep dark");
        assert_eq!(dimension_color(OVERWORLD), 0x55FF55);
        assert_eq!(distance_text(4.4), "4 m");
        assert_eq!(distance_text(0.5), "1 m");
        assert_eq!(distance_text(1530.0), "1.5 km");
        assert!(Filter::All.matches(END));
        assert!(!Filter::Overworld.matches(NETHER));
        assert_eq!(Filter::Nether.label(), "Nether");
    }

    #[test]
    fn colours_parse_name_and_cycle() {
        assert_eq!(parse_color("red"), Some(0xFF5555));
        assert_eq!(parse_color("dark-blue"), Some(0x0000AA));
        assert_eq!(parse_color("gray"), Some(0xAAAAAA));
        assert_eq!(parse_color("#ff55ff"), Some(0xFF55FF));
        assert_eq!(parse_color("FF55FF"), Some(0xFF55FF));
        assert_eq!(parse_color("pink"), None);
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(color_name(0xFF5555), "red");
        assert_eq!(color_name(0x123456), "#123456");
        assert_eq!(next_color(0xFF5555), 0xFFAA00);
        assert_eq!(next_color(0x000000), 0xFF5555);
        assert_eq!(next_color(0x123456), 0xFF5555);
    }

    #[test]
    fn create_names_number_and_colour_in_palette_order() {
        let mut s = state();
        let i = s.create(None, [10, 64, -4], OVERWORLD.into()).unwrap();
        assert_eq!(s.waypoints[i].name, "Waypoint 1");
        assert_eq!(s.waypoints[i].color, PALETTE[0]);
        let j = s
            .create(Some("Home".into()), [0, 64, 0], OVERWORLD.into())
            .unwrap();
        assert_eq!(s.waypoints[j].name, "Home");
        assert_eq!(s.waypoints[j].color, PALETTE[1]);
        // A taken suggested name is skipped.
        let k = s.create(None, [1, 64, 1], OVERWORLD.into()).unwrap();
        assert_eq!(s.waypoints[k].name, "Waypoint 3");
        // A blank name falls back to the suggestion, cut to 32 chars.
        let l = s
            .create(Some("   ".into()), [2, 64, 2], OVERWORLD.into())
            .unwrap();
        assert_eq!(s.waypoints[l].name, "Waypoint 4");
        let long = "x".repeat(40);
        let m = s.create(Some(long), [3, 64, 3], OVERWORLD.into()).unwrap();
        assert_eq!(s.waypoints[m].name.chars().count(), 32);
    }

    #[test]
    fn rename_hide_show_delete_and_move() {
        let mut s = state();
        s.waypoints
            .push(waypoint("Home", [0, 64, 0], OVERWORLD, 0xFF5555));
        s.waypoints
            .push(waypoint("Death 1", [5, 64, 5], OVERWORLD, DEATH_COLOR));
        s.waypoints[1].death = true;
        assert_eq!(s.rename(2, "My stuff".into()), Ok(()));
        assert_eq!(s.waypoints[1].name, "My stuff");
        assert!(
            !s.waypoints[1].death,
            "renaming keeps a death waypoint for good"
        );
        assert_eq!(s.rename(9, "Nope".into()), Err("No waypoint 9".into()));
        assert_eq!(
            s.rename(1, "   ".into()),
            Err("A waypoint needs a name".into())
        );
        assert_eq!(
            s.rename(0, "Zero".into()),
            Err("Waypoint numbers start at 1".into())
        );
        assert_eq!(s.set_hidden(1, true), Ok(()));
        assert!(s.waypoints[0].hidden);
        assert_eq!(s.set_color(1, 0x55FFFF), Ok(()));
        assert_eq!(s.waypoints[0].color, 0x55FFFF);
        assert_eq!(s.set_color(1, 0x1FF5555), Ok(()));
        assert_eq!(s.waypoints[0].color, 0xFF5555, "colour is masked to RGB");
        assert_eq!(s.move_to(1, [9, 70, 9], NETHER.into()), Ok(()));
        assert_eq!(s.waypoints[0].pos, [9, 70, 9]);
        assert_eq!(s.waypoints[0].dimension, NETHER);
        assert_eq!(s.set_dimension(1, END.into()), Ok(()));
        assert_eq!(s.waypoints[0].dimension, END);
        let gone = s.remove(1).unwrap();
        assert_eq!(gone.name, "Home");
        assert_eq!(s.waypoints.len(), 1);
        assert_eq!(s.remove(5), Err("No waypoint 5".into()));
    }

    #[test]
    fn death_waypoints_keep_the_newest_five() {
        let mut s = state();
        s.waypoints
            .push(waypoint("Home", [0, 64, 0], OVERWORLD, 0x55FF55));
        for x in 1..=6 {
            assert!(s.add_death(true, (OVERWORLD.into(), [x, 64, 0])));
        }
        let deaths: Vec<&str> = s
            .waypoints
            .iter()
            .filter(|w| w.death)
            .map(|w| w.name.as_str())
            .collect();
        assert_eq!(
            deaths,
            ["Death 2", "Death 3", "Death 4", "Death 5", "Death 6"]
        );
        assert_eq!(s.waypoints[0].name, "Home");
        // The same block twice is one death, and off means none.
        assert!(!s.add_death(true, (OVERWORLD.into(), [6, 64, 0])));
        assert!(!s.add_death(false, (OVERWORLD.into(), [7, 64, 0])));
        assert_eq!(s.death_name(), "Death 7");
    }

    #[test]
    fn health_reaching_zero_marks_where_the_player_last_stood() {
        let mut s = state();
        let alive = player();
        assert!(!s.track_death(&alive));
        assert_eq!(s.last_alive, Some((OVERWORLD.into(), [0, 64, 0])));
        // Dying marks it, once per fall to zero.
        let dead = Player {
            health: 0.0,
            ..player()
        };
        assert!(s.track_death(&dead));
        assert_eq!(s.waypoints.len(), 1);
        assert_eq!(s.waypoints[0].name, "Death 1");
        assert_eq!(s.waypoints[0].pos, [0, 64, 0]);
        assert!(!s.track_death(&dead), "the same death is not marked twice");
        assert_eq!(s.waypoints.len(), 1);
        // Back alive resets it; the next death, elsewhere, is a new waypoint.
        let elsewhere = Player {
            x: 10.5,
            z: 10.5,
            ..player()
        };
        assert!(!s.track_death(&elsewhere));
        assert!(s.track_death(&dead));
        assert_eq!(s.waypoints.len(), 2);
        assert_eq!(s.waypoints[1].name, "Death 2");
        assert_eq!(s.waypoints[1].pos, [10, 64, 10]);
        // Turned off, a death leaves nothing.
        s.deaths = false;
        assert!(!s.track_death(&alive));
        assert!(!s.track_death(&dead));
        assert_eq!(s.waypoints.len(), 2);
    }

    #[test]
    fn panel_shows_visible_waypoints_of_this_dimension_nearest_first() {
        let mut s = state();
        s.waypoints
            .push(waypoint("Far", [100, 64, 0], OVERWORLD, 0xFF5555));
        s.waypoints
            .push(waypoint("Near", [2, 64, 0], OVERWORLD, 0x55FF55));
        s.waypoints
            .push(waypoint("Hidden", [1, 64, 0], OVERWORLD, 0xFFFF55));
        s.waypoints[2].hidden = true;
        s.waypoints
            .push(waypoint("Nether base", [2, 64, 0], NETHER, 0xFFAA00));
        let lines = s.panel_lines(&player());
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        // Feet (0.5, 64, 0.5) to Near's centre (2.5, 64.5, 0.5): 2.06 -> 2 m.
        // To Far's centre (100.5, 64.5, 0.5): 100.0 -> 100 m.
        assert_eq!(texts, ["Near · 2 m", "Far · 100 m"]);
        assert_eq!(lines[0].color, 0x55FF55);
        assert_eq!(lines[1].color, 0xFF5555);
        // Sixteen lines at most.
        for i in 0..20 {
            s.waypoints
                .push(waypoint(&format!("W{i}"), [i, 64, 0], OVERWORLD, 0xFFFFFF));
        }
        assert_eq!(s.panel_lines(&player()).len(), 16);
    }

    #[test]
    fn markers_are_a_box_and_a_beam_in_the_waypoints_colour() {
        let mut s = state();
        s.waypoints
            .push(waypoint("Home", [12, 64, -44], OVERWORLD, 0xFF5555));
        s.waypoints
            .push(waypoint("Hidden", [1, 64, 1], OVERWORLD, 0xFFFF55));
        s.waypoints[1].hidden = true;
        s.waypoints
            .push(waypoint("Nether base", [1, 64, 1], NETHER, 0xFFAA00));
        let shapes = s.marker_shapes(OVERWORLD, &player());
        assert_eq!(
            shapes.len(),
            2,
            "box plus beam for the one visible waypoint"
        );
        match &shapes[0] {
            WorldShape::Box(b) => {
                assert_eq!((b.min.x, b.min.y, b.min.z), (12.0, 64.0, -44.0));
                assert_eq!((b.max.x, b.max.y, b.max.z), (13.0, 65.0, -43.0));
                assert_eq!(b.color, 0xFFFF5555);
                assert_eq!(b.width, 2.0);
            }
            other => panic!("expected a box, got {other:?}"),
        }
        match &shapes[1] {
            WorldShape::Line(l) => {
                assert_eq!((l.start.x, l.start.y, l.start.z), (12.5, 64.0, -43.5));
                assert_eq!((l.end.x, l.end.y, l.end.z), (12.5, 96.0, -43.5));
                assert_eq!(l.color, 0xFFFF5555);
                assert_eq!(l.width, 2.0);
            }
            other => panic!("expected a line, got {other:?}"),
        }
        // Beams off leaves the box alone; the budget caps the markers.
        s.beam = false;
        assert_eq!(s.marker_shapes(OVERWORLD, &player()).len(), 1);
        s.beam = true;
        for i in 0..100 {
            s.waypoints
                .push(waypoint(&format!("W{i}"), [i, 64, 0], OVERWORLD, 0xFFFFFF));
        }
        assert_eq!(s.marker_shapes(OVERWORLD, &player()).len(), MAX_MARKERS * 2);
    }

    #[test]
    fn labels_draw_name_and_distance_at_the_projected_position() {
        let mut s = state();
        s.waypoints
            .push(waypoint("Home", [0, 64, 20], OVERWORLD, 0xFF5555));
        // The projection records what it was asked to project.
        let asked = std::cell::RefCell::new(Vec::new());
        let shapes = s.label_shapes(&player(), |p| {
            asked.borrow_mut().push(p);
            ScreenPoint {
                x: 213.5,
                y: 121.0,
                visible: true,
                depth: 20.0,
            }
        });
        // The anchor is the block centre raised above its box.
        assert_eq!(asked.borrow().len(), 1);
        assert_eq!(
            (
                asked.borrow()[0].x,
                asked.borrow()[0].y,
                asked.borrow()[0].z
            ),
            (0.5, 65.5, 20.5)
        );
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            Shape::Text(t) => {
                assert_eq!(t.text, "Home · 20 m");
                assert_eq!((t.at.x, t.at.y), (213.5, 121.0));
                assert_eq!(t.color, 0xFF5555);
                assert!(t.centered && t.shadow);
            }
            other => panic!("expected a text label, got {other:?}"),
        }
    }

    #[test]
    fn labels_skip_waypoints_behind_the_camera_hidden_ones_and_other_dimensions() {
        let mut s = state();
        s.waypoints
            .push(waypoint("Home", [0, 64, 20], OVERWORLD, 0xFF5555));
        s.waypoints
            .push(waypoint("Hidden", [0, 64, 10], OVERWORLD, 0xFFFF55));
        s.waypoints[1].hidden = true;
        s.waypoints
            .push(waypoint("Nether base", [0, 64, 10], NETHER, 0xFFAA00));
        // Behind the camera: nothing is drawn.
        let behind = s.label_shapes(&player(), |_| ScreenPoint {
            x: 0.0,
            y: 0.0,
            visible: false,
            depth: -10.0,
        });
        assert!(behind.is_empty());
        // Ahead: only the visible waypoint of this dimension.
        let ahead = s.label_shapes(&player(), |_| ScreenPoint {
            x: 100.0,
            y: 50.0,
            visible: true,
            depth: 10.0,
        });
        assert_eq!(ahead.len(), 1);
        match &ahead[0] {
            Shape::Text(t) => assert_eq!(t.text, "Home · 20 m"),
            other => panic!("expected a text label, got {other:?}"),
        }
        // Sixteen labels at most, nearest first.
        for i in 0..20 {
            s.waypoints
                .push(waypoint(&format!("W{i}"), [i, 64, 20], OVERWORLD, 0xFFFFFF));
        }
        assert_eq!(
            s.label_shapes(&player(), |_| ScreenPoint {
                x: 0.0,
                y: 0.0,
                visible: true,
                depth: 1.0,
            })
            .len(),
            MAX_LABELS
        );
    }

    #[test]
    fn commands_parse_and_apply() {
        let mut s = state();
        let p = player();
        let out = s
            .apply_command(Command::Add(Some("Home".into())), Some(&p))
            .unwrap();
        assert!(
            out.feedback
                .contains("\"Home\" created at 0 64 0 (Overworld)")
        );
        assert!(!out.open_list);
        assert_eq!(s.waypoints.len(), 1);
        let out = s
            .apply_command(
                Waypoints::parse_command("rename 1 Base Camp").unwrap(),
                Some(&p),
            )
            .unwrap();
        assert_eq!(out.feedback, "Waypoint 1 renamed to \"Base Camp\"");
        let out = s
            .apply_command(Waypoints::parse_command("color 1 cyan").unwrap(), Some(&p))
            .unwrap();
        assert_eq!(out.feedback, "Waypoint 1 is now cyan");
        assert_eq!(s.waypoints[0].color, 0x55FFFF);
        let out = s
            .apply_command(Waypoints::parse_command("dim 1 nether").unwrap(), Some(&p))
            .unwrap();
        assert_eq!(out.feedback, "Waypoint 1 is now in Nether");
        let out = s
            .apply_command(Waypoints::parse_command("move 1").unwrap(), Some(&p))
            .unwrap();
        assert_eq!(out.feedback, "Waypoint 1 moved here");
        assert_eq!(s.waypoints[0].dimension, OVERWORLD);
        let out = s
            .apply_command(Waypoints::parse_command("hide 1").unwrap(), Some(&p))
            .unwrap();
        assert_eq!(out.feedback, "Waypoint 1 hidden");
        assert!(s.waypoints[0].hidden);
        let out = s
            .apply_command(Waypoints::parse_command("show 1").unwrap(), Some(&p))
            .unwrap();
        assert_eq!(out.feedback, "Waypoint 1 shown");
        let out = s
            .apply_command(Waypoints::parse_command("list").unwrap(), Some(&p))
            .unwrap();
        assert!(out.open_list);
        let out = s
            .apply_command(Waypoints::parse_command("delete 1").unwrap(), Some(&p))
            .unwrap();
        assert_eq!(out.feedback, "Waypoint \"Base Camp\" deleted");
        assert!(s.waypoints.is_empty());
        // Errors.
        assert!(
            s.apply_command(Waypoints::parse_command("rename 1 X").unwrap(), Some(&p))
                .is_err()
        );
        assert!(
            s.apply_command(Waypoints::parse_command("add").unwrap(), None)
                .is_err()
        );
        assert!(Waypoints::parse_command("frobnicate").is_err());
        assert!(Waypoints::parse_command("color 1 pink").is_err());
        assert!(Waypoints::parse_command("color 1").is_err());
        assert!(Waypoints::parse_command("dim 1 moon").is_err());
        assert!(Waypoints::parse_command("rename 1").is_err());
        assert!(Waypoints::parse_command("rename x Name").is_err());
        assert_eq!(Waypoints::parse_command("help").unwrap(), Command::Help,);
        assert_eq!(Waypoints::parse_command("").unwrap(), Command::Help,);
        assert_eq!(Waypoints::parse_command("add").unwrap(), Command::Add(None),);
        assert_eq!(
            Waypoints::parse_command("add My Base").unwrap(),
            Command::Add(Some("My Base".into())),
        );
    }

    #[test]
    fn list_screen_filters_selects_and_confirms_delete() {
        let mut s = state();
        s.waypoints
            .push(waypoint("Home", [0, 64, 0], OVERWORLD, 0xFF5555));
        s.waypoints
            .push(waypoint("Nether base", [2, 64, 0], NETHER, 0xFFAA00));
        let p = player();
        let screen = s.screen(Some(&p));
        assert_eq!(screen.title, "Waypoints (2)");
        let rows = match screen.widgets.iter().find_map(|w| match w {
            Widget::List(l) => Some(l),
            _ => None,
        }) {
            Some(l) => &l.rows,
            None => panic!("no list widget"),
        };
        assert_eq!(rows.len(), 2);
        assert!(rows[0].starts_with("1. Home"));
        assert!(!rows[0].contains("other dimension"));
        // Filter to the Nether: one row, and the selection clears.
        assert_eq!(s.select(FILTER_NETHER, 0, Some(&p)), Select::Screen);
        assert_eq!(s.filtered(), vec![1]);
        assert_eq!(s.screen(Some(&p)).title, "Waypoints (2)");
        // Pick the row, hide it, recolour it, delete it in two steps.
        assert_eq!(s.select(LIST_WIDGET, 0, Some(&p)), Select::Screen);
        assert_eq!(s.selected, Some(1));
        assert_eq!(s.select(HIDE_BUTTON, 0, Some(&p)), Select::Screen);
        assert!(s.waypoints[1].hidden);
        assert_eq!(s.select(COLOUR_BUTTON, 0, Some(&p)), Select::Screen);
        assert_eq!(s.waypoints[1].color, 0xFFFF55);
        assert_eq!(s.select(DELETE_BUTTON, 0, Some(&p)), Select::Screen);
        assert_eq!(s.waypoints.len(), 2, "asks first");
        assert_eq!(s.select(DELETE_BUTTON, 0, Some(&p)), Select::Screen);
        assert_eq!(s.waypoints.len(), 1);
        assert_eq!(s.selected, None);
        // The settings buttons toggle and refresh the page, not the screen.
        assert_eq!(s.select(MARKERS_BUTTON, 0, Some(&p)), Select::Settings);
        assert!(!s.markers);
        assert_eq!(s.select(BEAM_BUTTON, 0, Some(&p)), Select::Settings);
        assert!(!s.beam);
        assert_eq!(s.select(PANEL_BUTTON, 0, Some(&p)), Select::Settings);
        assert!(!s.panel);
        assert_eq!(s.select(DEATHS_BUTTON, 0, Some(&p)), Select::Settings);
        assert!(!s.deaths);
        assert_eq!(s.select(999, 0, Some(&p)), Select::None);
    }

    #[test]
    fn screen_rows_show_distance_or_other_dimension() {
        let mut s = state();
        s.waypoints
            .push(waypoint("Home", [0, 64, 0], OVERWORLD, 0xFF5555));
        s.waypoints
            .push(waypoint("Nether base", [2, 64, 0], NETHER, 0xFFAA00));
        let p = player();
        assert_eq!(s.row(0, Some(&p)), "1. Home · 0 64 0 · Overworld · 1 m");
        assert_eq!(
            s.row(1, Some(&p)),
            "2. Nether base · 2 64 0 · Nether · other dimension"
        );
        let nether = Player {
            dimension: NETHER.into(),
            ..player()
        };
        assert!(s.row(1, Some(&nether)).contains("2 m"));
        assert_eq!(
            s.row(0, None),
            "1. Home · 0 64 0 · Overworld · other dimension"
        );
    }
}

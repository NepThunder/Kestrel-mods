//! Waymap: a behaviour port of Waymap by Aron (Lauch2),
//! MIT: https://github.com/Lauch2/horizon-waymap-mod
//!
//! A waypoint mod with a rotating minimap. The original
//! stores its waypoints per world or server and shows them
//! on a rotated minimap (the map north follows the player),
//! in the world as markers, and in a HUD panel of the
//! nearest ones; the list filters by dimension and by
//! deaths; its editor changes name, position, dimension,
//! colour and visibility; a death waypoint is left where
//! you died; and it has the `/wp` commands.
//!
//! The port: the waypoints per world in the mod's storage,
//! same shape (a dimension and a colour of its own, a
//! hidden flag, the death flag, the 512 cap); the minimap
//! composed from the game's own map tiles (API 25), turning
//! with the player at blocks-per-pixel 1; markers drawn
//! through render projection; the HUD panel through the
//! hud surface; the list and the editor as standard
//! screens; the death left from the map surface's death
//! events. The live tiles are memory only — the original
//! keeps no map at all, and its minimap shows the terrain
//! round you, from the tiles the game has already made.
//!
//! Standing choices, all documented in the README: the
//! settings page carries the three toggles (markers,
//! labels, HUD panel) and the create/list keys rebind
//! there, as the original's settings work; names are
//! changed with `/wp rename`, since the widget surface has
//! no text field; and the minimap's size (100, top-right)
//! is the port's fixed choice, the original naming none.

use horizon_mod_sdk::{
    MapImage, MapLayer, MapTile, MinimapImage, Mod, Registration, chat, game, hud, input,
    input::Event,
    map,
    render::{self, Point, Shape, TextShape, WorldPoint},
    storage, ui,
    ui::{ButtonWidget, ListWidget, TextWidget, Widget},
};

fn button(id: u32, label: String) -> Widget {
    Widget::Button(ButtonWidget { id, label })
}

fn text(id: u32, text: String) -> Widget {
    Widget::Text(TextWidget { id, text, color: 0 })
}
use std::collections::{HashMap, HashSet, VecDeque};

const API_LEVEL: u32 = 25;

const CREATE_KEY: &str = "create";
const LIST_KEY: &str = "list";
const LIST_KEY_M: &str = "list_m";

const LIST_WIDGET: u32 = 10;
const LIST_NEW_BUTTON: u32 = 11;
const LIST_EDIT_BUTTON: u32 = 12;
const LIST_HIDE_BUTTON: u32 = 13;
const LIST_DELETE_BUTTON: u32 = 14;
const FILTER_BASE: u32 = 20;
const EDITOR_STEP_BUTTON: u32 = 30;
const EDITOR_XM: u32 = 31;
const EDITOR_XP: u32 = 32;
const EDITOR_YM: u32 = 33;
const EDITOR_YP: u32 = 34;
const EDITOR_ZM: u32 = 35;
const EDITOR_ZP: u32 = 36;
const EDITOR_POS_BUTTON: u32 = 37;
const EDITOR_DIM_BUTTON: u32 = 38;
const EDITOR_COLOR_BUTTON: u32 = 39;
const EDITOR_HIDDEN_BUTTON: u32 = 40;
const EDITOR_SAVE_BUTTON: u32 = 41;
const EDITOR_CANCEL_BUTTON: u32 = 42;

const STEPS: [i32; 3] = [1, 10, 100];
const CAP: usize = 512;
const MINIMAP_SIZE: u32 = 100;
const MINIMAP_BPP: f64 = 1.0;
const FEED_RADIUS: i32 = 6;

// The waypoint colour names and ARGB values, the
// original's palette: sixteen colours from black to
// white, used by the editor and by `/wp color <name>`.
const PALETTE: [(&str, u32); 16] = [
    ("Black", 0xFF00_0000),
    ("Dark Blue", 0xFF00_00AA),
    ("Dark Green", 0xFF00_AA00),
    ("Dark Aqua", 0xFF00_AAAA),
    ("Dark Red", 0xFFAA_0000),
    ("Dark Purple", 0xFFAA_00AA),
    ("Gold", 0xFFFF_AA00),
    ("Gray", 0xFFAA_AAAA),
    ("Dark Gray", 0xFF55_5555),
    ("Blue", 0xFF55_55FF),
    ("Green", 0xFF55_FF55),
    ("Aqua", 0xFF55_FFFF),
    ("Red", 0xFFFF_5555),
    ("Light Purple", 0xFFFF_55FF),
    ("Yellow", 0xFFFF_FF55),
    ("White", 0xFFFF_FFFF),
];
const DEATH_COLOR: u32 = 0xFFFF_0000;

// ----------------------------------------------------------------
// Map shading and tiles (the map item's rule, as in the
// other two map ports; the store is memory only).

const WALL: u8 = 0x80;
const WATER: u8 = 12;
const UNKNOWN: u32 = 0xC814_1414;

const MAP_COLORS: [u32; 62] = [
    0, 8368696, 16247203, 13092807, 16711680, 10526975, 10987431, 31744, 16777215, 10791096,
    9923917, 7368816, 4210943, 9402184, 16776437, 14188339, 11685080, 6724056, 15066419, 8375321,
    15892389, 5000268, 10066329, 5013401, 8339378, 3361970, 6704179, 6717235, 10040115, 1644825,
    16445005, 6085589, 4882687, 55610, 8476209, 7340544, 13742497, 10441252, 9787244, 7367818,
    12223780, 6780213, 10505550, 3746083, 8874850, 5725276, 8014168, 4996700, 4993571, 5001770,
    9321518, 2430480, 12398641, 9715553, 6035741, 1474182, 3837580, 5647422, 
    1356933, 6579300, 14200723, 8365974,
];

fn brightness(color: u8, height: i16, north: Option<i16>, depth: u8, x: i32, z: i32) -> u8 {
    if color & WALL != 0 {
        return 3;
    }
    let v = if color == WATER {
        f64::from(depth) * 0.1 + f64::from((x + z) & 1) * 0.2
    } else {
        let n = north.unwrap_or(height);
        f64::from(height - n) * 0.8 + (f64::from((x + z) & 1) - 0.5) * 0.4
    };
    if color == WATER {
        if v < 0.5 {
            2
        } else if v > 0.9 {
            0
        } else {
            1
        }
    } else if v > 0.6 {
        2
    } else if v < -0.6 {
        0
    } else {
        1
    }
}

fn shade(color: u8, height: i16, north: Option<i16>, depth: u8, gx: i32, gz: i32) -> u32 {
    let base = color & !WALL;
    let Some(&packed) = MAP_COLORS.get(base as usize) else {
        return UNKNOWN;
    };
    if packed == 0 {
        return UNKNOWN;
    }
    let m = [180u32, 220, 255, 135][brightness(color, height, north, depth, gx, gz) as usize];
    let ch = |v: u32| (v * m / 255).min(255);
    0xFF00_0000 | (ch((packed >> 16) & 0xFF) << 16) | (ch((packed >> 8) & 0xFF) << 8) | ch(packed & 0xFF)
}

#[derive(Clone)]
struct Tile {
    layer: MapLayer,
    version: u64,
    colors: [u8; 256],
    heights: [i16; 256],
    depths: [u8; 256],
}

impl Tile {
    fn from_sdk(t: &MapTile) -> Option<Tile> {
        if t.colors.len() != 256 || t.heights.len() != 256 || t.depths.len() != 256 {
            return None;
        }
        let mut tile = Tile {
            layer: t.layer,
            version: t.version,
            colors: [0; 256],
            heights: [0; 256],
            depths: [0; 256],
        };
        tile.colors.copy_from_slice(&t.colors);
        tile.heights.copy_from_slice(&t.heights);
        tile.depths.copy_from_slice(&t.depths);
        Some(tile)
    }
}

/// The live tiles: surface tiles by chunk, and the cave
/// tiles per cut. The minimap shows the surface, or the
/// cut nearest the head underground.
#[derive(Default)]
struct TileStore {
    surface: HashMap<(i32, i32), Tile>,
    cave: HashMap<(i32, i32), Vec<Tile>>,
}

impl TileStore {
    fn put(&mut self, at: (i32, i32), t: Tile) {
        match t.layer {
            MapLayer::Surface => {
                self.surface.insert(at, t);
            }
            MapLayer::Cave => {
                let cuts = self.cave.entry(at).or_default();
                match cuts.iter_mut().find(|c| c.version == t.version) {
                    Some(slot) => *slot = t,
                    None => cuts.push(t),
                }
            }
        }
    }

    /// The tile the minimap shows for a chunk at head height.
    fn display(&self, at: (i32, i32), head_y: i32) -> Option<&Tile> {
        match self.cave.get(&at) {
            Some(cuts) if !cuts.is_empty() => cuts.iter().min_by_key(|c| {
                let cut = c
                    .heights
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(i16::MIN) as i32;
                (cut - head_y).abs()
            }),
            _ => self.surface.get(&at),
        }
    }

    /// The shaded colour of one world column at head height.
    fn color_at(&self, x: i32, z: i32, head_y: i32) -> u32 {
        let at = (x.div_euclid(16), z.div_euclid(16));
        let Some(t) = self.display(at, head_y) else {
            return UNKNOWN;
        };
        let (lx, lz) = (x.rem_euclid(16), z.rem_euclid(16));
        let i = (lz * 16 + lx) as usize;
        let raw = t.colors[i];
        if raw & !WALL == 0 {
            return UNKNOWN;
        }
        let north = self
            .display((at.0, at.1 - 1), head_y)
            .filter(|_| lz == 0)
            .map(|n| n.heights[15 * 16 + lx as usize])
            .or_else(|| (lz > 0).then(|| t.heights[((lz - 1) * 16 + lx) as usize]));
        shade(raw, t.heights[i], north, t.depths[i], x, z)
    }
}

// ----------------------------------------------------------------
// Waypoints.

#[derive(Clone, Debug, PartialEq)]
struct Waypoint {
    name: String,
    x: i32,
    y: i32,
    z: i32,
    dimension: String,
    /// ARGB, the colour as the original keeps it.
    color: u32,
    hidden: bool,
    death: bool,
}

/// One world's waypoints, plus how many deaths the player
/// has had here (the names run Death 1, Death 2, ...).
#[derive(Default, Clone)]
struct Store {
    waypoints: Vec<Waypoint>,
    deaths: u32,
}

/// The storage key for a world's waypoints: the context is
/// the world or server alone (waypoints carry their own
/// dimension), as the original keys its store.
fn world_hash(world: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in world.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

fn waypoints_key(hash: &str) -> String {
    format!("w.{hash}")
}

fn encode_store(st: &Store) -> Vec<u8> {
    let mut out = format!("#deaths {}\n", st.deaths);
    for w in &st.waypoints {
        let flags = format!("{}{}", if w.hidden { "h" } else { "" }, if w.death { "d" } else { "" });
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{:08x}\t{}\n",
            w.name.replace(['\t', '\n'], " "),
            w.x,
            w.y,
            w.z,
            w.dimension,
            w.color,
            flags
        ));
    }
    out.into_bytes()
}

fn decode_store(bytes: &[u8]) -> Store {
    let text = String::from_utf8_lossy(bytes);
    let mut st = Store::default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("#deaths ") {
            st.deaths = rest.trim().parse().unwrap_or(0);
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 {
            continue;
        }
        let (Ok(x), Ok(y), Ok(z)) = (f[1].parse(), f[2].parse(), f[3].parse()) else {
            continue;
        };
        st.waypoints.push(Waypoint {
            name: f[0].to_string(),
            x,
            y,
            z,
            dimension: f[4].to_string(),
            color: u32::from_str_radix(f[5], 16).unwrap_or(0xFFFF_FFFF),
            hidden: f[6].contains('h'),
            death: f[6].contains('d'),
        });
    }
    st
}

fn clean_name(raw: &str) -> String {
    raw.trim().replace(['\t', '\n'], " ")
}

fn distance(p: (f64, f64, f64), w: &Waypoint) -> f64 {
    let dx = f64::from(w.x) + 0.5 - p.0;
    let dy = f64::from(w.y) - p.1;
    let dz = f64::from(w.z) + 0.5 - p.2;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// A colour from `/wp color`: a palette name (one or two
/// words) or a `#rrggbb`.
fn parse_color(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return u32::from_str_radix(hex, 16).ok().map(|v| 0xFF00_0000 | v);
    }
    PALETTE
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(s))
        .map(|(_, v)| *v)
}

/// Split `/wp color`'s arguments into (waypoint name,
/// colour): the colour is the last token if it is a
/// `#rrggbb`, else a trailing palette name.
fn split_color_args(rest: &str) -> Option<(String, u32)> {
    let rest = rest.trim();
    if let Some((name, last)) = rest.rsplit_once(' ')
        && last.starts_with('#')
        && let Some(c) = parse_color(last)
    {
        return Some((clean_name(name), c));
    }
    for words in [2usize, 1] {
        let parts: Vec<&str> = rest.rsplitn(words + 1, ' ').collect();
        if parts.len() == words + 1 {
            let candidate = parts[..words].iter().rev().cloned().collect::<Vec<_>>().join(" ");
            if let Some(c) = parse_color(&candidate) {
                return Some((clean_name(parts[words]), c));
            }
        }
    }
    None
}

// ----------------------------------------------------------------
// Settings: the original's three toggles.

#[derive(Clone)]
struct Settings {
    markers: bool,
    labels: bool,
    hud_panel: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            markers: true,
            labels: true,
            hud_panel: true,
        }
    }
}

impl Settings {
    fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let on = v == "1";
            match k {
                "markers" => s.markers = on,
                "labels" => s.labels = on,
                "hud_panel" => s.hud_panel = on,
                _ => {}
            }
        }
        s
    }

    fn encode(&self) -> Vec<u8> {
        format!(
            "markers={}\nlabels={}\nhud_panel={}\n",
            self.markers as u8, self.labels as u8, self.hud_panel as u8
        )
        .into_bytes()
    }
}

fn yn(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

// ----------------------------------------------------------------
// The minimap: the terrain round the player at bpp 1,
// turning with the player (the map north follows the
// look), the shown waypoints as dots, a death as a red X,
// the player as a white mark at the centre.

#[allow(clippy::too_many_arguments)]
fn compose_minimap(
    store: &TileStore,
    px: f64,
    pz: f64,
    yaw: f32,
    head_y: i32,
    dots: &[(i32, i32, u32, bool)],
) -> MinimapImage {
    let size = MINIMAP_SIZE;
    let mut palette: Vec<u32> = vec![0];
    let mut palette_index: HashMap<u32, u8> = HashMap::new();
    let mut pixels = vec![0u8; (size * size) as usize];
    let idx_of = |palette: &mut Vec<u32>, index: &mut HashMap<u32, u8>, c: u32| -> u8 {
        if let Some(&i) = index.get(&c) {
            return i;
        }
        if palette.len() >= 256 {
            return 0;
        }
        let i = palette.len() as u8;
        palette.push(c);
        index.insert(c, i);
        i
    };
    // Seed the dot colours so terrain cannot starve them.
    for &(_, _, c, _) in dots {
        idx_of(&mut palette, &mut palette_index, c);
    }
    let white = idx_of(&mut palette, &mut palette_index, 0xFFFF_FFFF);
    let rim = idx_of(&mut palette, &mut palette_index, 0xFF10_1010);
    let red = idx_of(&mut palette, &mut palette_index, 0xFFFF_0000);
    let (cx, cz) = (px.floor() as i32, pz.floor() as i32);
    // The heading direction in world axes, and the right
    // vector, for the rotating map.
    let yr = f64::from(yaw).to_radians();
    let fwd = (-yr.sin(), -yr.cos());
    let right = (fwd.1, -fwd.0);
    let half = f64::from(size) / 2.0;
    for v in 0..size {
        for u in 0..size {
            let (du, dv) = (f64::from(u) + 0.5 - half, f64::from(v) + 0.5 - half);
            let wx = f64::from(cx) + 0.5 + (right.0 * du + fwd.0 * dv) * MINIMAP_BPP;
            let wz = f64::from(cz) + 0.5 + (right.1 * du + fwd.1 * dv) * MINIMAP_BPP;
            let c = store.color_at(wx.floor() as i32, wz.floor() as i32, head_y);
            pixels[(v * size + u) as usize] = idx_of(&mut palette, &mut palette_index, c);
        }
    }
    // Waypoint dots, in view space: deaths as a red X.
    for &(wx, wz, color, death) in dots {
        let rel = (
            f64::from(wx) + 0.5 - (f64::from(cx) + 0.5),
            f64::from(wz) + 0.5 - (f64::from(cz) + 0.5),
        );
        let u = (rel.0 * right.0 + rel.1 * right.1) / MINIMAP_BPP + half;
        let v = (rel.0 * fwd.0 + rel.1 * fwd.1) / MINIMAP_BPP + half;
        let (ui, vi) = (u.round() as i32, v.round() as i32);
        if death {
            for d in -2..=2i32 {
                for (x, y) in [(ui + d, vi + d), (ui + d, vi - d)] {
                    if x >= 0 && y >= 0 && x < size as i32 && y < size as i32 {
                        pixels[(y as u32 * size + x as u32) as usize] = red;
                    }
                }
            }
        } else {
            let dot = idx_of(&mut palette, &mut palette_index, color);
            for dy in -1..=1i32 {
                for dx in -1..=1i32 {
                    let (x, y) = (ui + dx, vi + dy);
                    if x >= 0 && y >= 0 && x < size as i32 && y < size as i32 {
                        pixels[(y as u32 * size + x as u32) as usize] = dot;
                    }
                }
            }
        }
    }
    // The player: a white mark with a dark rim.
    let c = size as i32 / 2;
    for dy in -2..=2i32 {
        for dx in -2..=2i32 {
            let fill = dx.abs() + dy.abs() <= 1;
            pixels[((c + dy) as u32 * size + (c + dx) as u32) as usize] =
                if fill { white } else { rim };
        }
    }
    MinimapImage {
        corner: horizon_mod_sdk::Corner::TopRight,
        size: size as f32,
        round: false,
        image: MapImage {
            center_x: cx,
            center_z: cz,
            blocks_per_pixel: MINIMAP_BPP as f32,
            width: size,
            height: size,
            palette,
            pixels,
        },
    }
}

// ----------------------------------------------------------------
// Screens: the list with its dimension filters, and the
// editor.

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Overworld,
    Nether,
    End,
    Deaths,
}

impl Filter {
    fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Overworld => "Overworld",
            Filter::Nether => "Nether",
            Filter::End => "End",
            Filter::Deaths => "Deaths",
        }
    }

    fn matches(self, w: &Waypoint) -> bool {
        match self {
            Filter::All => true,
            Filter::Overworld => w.dimension.ends_with("overworld"),
            Filter::Nether => w.dimension.ends_with("the_nether"),
            Filter::End => w.dimension.ends_with("the_end"),
            Filter::Deaths => w.death,
        }
    }
}

const FILTERS: [Filter; 5] = [
    Filter::All,
    Filter::Overworld,
    Filter::Nether,
    Filter::End,
    Filter::Deaths,
];

fn dim_short(dimension: &str) -> &str {
    dimension.rsplit(':').next().unwrap_or(dimension)
}

#[derive(Clone)]
struct Editor {
    index: Option<usize>,
    name: String,
    x: i32,
    y: i32,
    z: i32,
    dimension: String,
    color: u32,
    hidden: bool,
    step: usize,
    from_list: bool,
}

struct Waymap {
    settings: Settings,
    world: Option<String>,
    hash: String,
    store_wps: Store,
    wps_dirty: bool,
    tiles: TileStore,
    known: HashMap<(i32, i32), u64>,
    verified: HashSet<(i32, i32)>,
    fetch: VecDeque<(i32, i32)>,
    queued: HashSet<(i32, i32)>,
    feed_dim: Option<String>,
    feed_cave: bool,
    filter: Filter,
    selected: Option<usize>,
    list_open: bool,
    editor: Option<Editor>,
    last_labels: String,
    last_panel: String,
}

impl Store {
    /// Add a waypoint under the 512 cap: at the cap the
    /// oldest death waypoint goes first; without one the
    /// add fails.
    fn push_capped(&mut self, w: Waypoint) -> bool {
        if self.waypoints.len() >= CAP {
            match self.waypoints.iter().position(|o| o.death) {
                Some(i) => {
                    self.waypoints.remove(i);
                }
                None => return false,
            }
        }
        self.waypoints.push(w);
        true
    }
}

impl Waymap {
    fn save_wps(&mut self) {
        if self.wps_dirty && !self.hash.is_empty() {
            let _ = storage::set(&waypoints_key(&self.hash), &encode_store(&self.store_wps));
        }
        self.wps_dirty = false;
    }

    fn save_settings(&self) {
        let _ = storage::set("settings", &self.settings.encode());
    }

    fn load_world(&mut self, world: &str) {
        self.hash = world_hash(world);
        self.world = Some(world.to_string());
        self.store_wps = storage::get(&waypoints_key(&self.hash))
            .map(|b| decode_store(&b))
            .unwrap_or_default();
        self.wps_dirty = false;
        self.selected = None;
    }

    /// The tile feed: watch the chunks round the player and
    /// fetch what is new or changed, into memory only.
    fn sweep(&mut self, pc: (i32, i32)) {
        let mut cells: Vec<(i32, (i32, i32))> = Vec::new();
        for dz in -FEED_RADIUS..=FEED_RADIUS {
            for dx in -FEED_RADIUS..=FEED_RADIUS {
                cells.push((dx.abs().max(dz.abs()), (pc.0 + dx, pc.1 + dz)));
            }
        }
        cells.sort();
        let cells: Vec<(i32, i32)> = cells.into_iter().map(|(_, c)| c).collect();
        for batch in cells.chunks(256) {
            let ask: Vec<horizon_mod_sdk::ChunkPos> = batch
                .iter()
                .map(|c| horizon_mod_sdk::ChunkPos { x: c.0, z: c.1 })
                .collect();
            for v in map::tile_versions(&ask) {
                let at = (v.chunk_x, v.chunk_z);
                let stale = !self.verified.contains(&at)
                    || self.known.get(&at).copied() != Some(v.version);
                if stale && self.queued.insert(at) {
                    self.fetch.push_back(at);
                }
            }
        }
    }

    fn drain_fetch(&mut self, pc: (i32, i32)) {
        for _ in 0..2 {
            if self.fetch.is_empty() {
                return;
            }
            let mut near: Vec<(i32, i32)> = Vec::new();
            let mut rest: Vec<(i32, i32)> = Vec::new();
            for at in self.fetch.drain(..) {
                if (at.0 - pc.0).abs() <= FEED_RADIUS + 2 && (at.1 - pc.1).abs() <= FEED_RADIUS + 2
                {
                    near.push(at);
                } else {
                    self.queued.remove(&at);
                }
            }
            near.sort_by_key(|a| (a.0 - pc.0).abs().max((a.1 - pc.1).abs()));
            let take = near.len().min(64);
            rest.extend(near.drain(take..));
            for at in &near {
                self.queued.remove(at);
            }
            let ask: Vec<horizon_mod_sdk::ChunkPos> = near
                .iter()
                .map(|c| horizon_mod_sdk::ChunkPos { x: c.0, z: c.1 })
                .collect();
            for tile in map::tiles(&ask) {
                let at = (tile.chunk_x, tile.chunk_z);
                self.known.insert(at, tile.version);
                self.verified.insert(at);
                if tile.layer == MapLayer::Cave {
                    self.feed_cave = true;
                }
                if let Some(t) = Tile::from_sdk(&tile) {
                    self.tiles.put(at, t);
                }
            }
            for at in rest {
                self.fetch.push_back(at);
                self.queued.insert(at);
            }
        }
    }

    fn list_indices(&self) -> Vec<usize> {
        self.store_wps
            .waypoints
            .iter()
            .enumerate()
            .filter(|(_, w)| self.filter.matches(w))
            .map(|(i, _)| i)
            .collect()
    }

    fn selected_waypoint(&self) -> Option<(usize, Waypoint)> {
        let row = self.selected?;
        let idxs = self.list_indices();
        idxs.get(row)
            .map(|&i| (i, self.store_wps.waypoints[i].clone()))
    }

    fn open_list(&mut self) {
        let rows: Vec<String> = self
            .list_indices()
            .iter()
            .map(|&i| {
                let w = &self.store_wps.waypoints[i];
                let hidden = if w.hidden { " (hidden)" } else { "" };
                let death = if w.death { " ✕" } else { "" };
                format!(
                    "{} ({}, {}, {}) [{}]{hidden}{death}",
                    w.name,
                    w.x,
                    w.y,
                    w.z,
                    dim_short(&w.dimension)
                )
            })
            .collect();
        let mut widgets: Vec<Widget> = vec![
            text(0, format!("Showing: {}", self.filter.label())),
            Widget::List(ListWidget {
                id: LIST_WIDGET,
                rows,
                selected: self.selected.map(|s| s as u32),
            }),
            button(LIST_NEW_BUTTON, "+ New".to_string()),
            button(LIST_EDIT_BUTTON, "Edit".to_string()),
            button(LIST_HIDE_BUTTON, "Hide/Show".to_string()),
            button(LIST_DELETE_BUTTON, "Delete".to_string()),
        ];
        for (i, f) in FILTERS.iter().enumerate() {
            widgets.push(button(FILTER_BASE + i as u32, f.label().to_string()));
        }
        ui::open_screen(&ui::Screen {
            title: "Waypoints".to_string(),
            widgets,
        });
        self.list_open = true;
    }

    #[allow(clippy::too_many_arguments)]
    fn open_editor(
        &mut self,
        index: Option<usize>,
        name: String,
        x: i32,
        y: i32,
        z: i32,
        dimension: String,
        color: u32,
        hidden: bool,
        from_list: bool,
    ) {
        self.editor = Some(Editor {
            index,
            name,
            x,
            y,
            z,
            dimension,
            color,
            hidden,
            step: 1,
            from_list,
        });
        self.show_editor();
    }

    fn show_editor(&self) {
        let Some(e) = &self.editor else { return };
        let color_label = PALETTE
            .iter()
            .find(|(_, v)| *v == e.color)
            .map(|(name, _)| name.to_string())
            .unwrap_or_else(|| format!("#{:06X}", e.color & 0xFF_FFFF));
        ui::open_screen(&ui::Screen {
            title: "Waypoint".to_string(),
            widgets: vec![
                text(0, format!("Name: {}", e.name)),
                text(1, format!("Position: {}, {}, {}", e.x, e.y, e.z)),
                button(EDITOR_STEP_BUTTON, format!("Step: ±{}", STEPS[e.step])),
                button(EDITOR_XM, "X −".to_string()),
                button(EDITOR_XP, "X +".to_string()),
                button(EDITOR_YM, "Y −".to_string()),
                button(EDITOR_YP, "Y +".to_string()),
                button(EDITOR_ZM, "Z −".to_string()),
                button(EDITOR_ZP, "Z +".to_string()),
                button(EDITOR_POS_BUTTON, "Use My Position".to_string()),
                button(
                    EDITOR_DIM_BUTTON,
                    format!("Dimension: {}", dim_short(&e.dimension)),
                ),
                button(EDITOR_COLOR_BUTTON, format!("Colour: {color_label}")),
                button(
                    EDITOR_HIDDEN_BUTTON,
                    format!("Visibility: {}", if e.hidden { "Hidden" } else { "Shown" }),
                ),
                button(EDITOR_SAVE_BUTTON, "Save".to_string()),
                button(EDITOR_CANCEL_BUTTON, "Cancel".to_string()),
            ],
        });
    }

    fn leave_editor(&mut self) {
        let from_list = self.editor.as_ref().map(|e| e.from_list).unwrap_or(false);
        self.editor = None;
        if from_list {
            self.open_list();
        } else {
            ui::close_screen();
            self.list_open = false;
        }
    }

    /// The `/wp` commands: add, del, list, rename, color,
    /// clear, edit (show), hide, show, help.
    fn wp_command(&mut self, args: &str) -> Result<Option<String>, String> {
        let args = args.trim();
        if args.is_empty() || args == "help" {
            return Ok(Some(
                "Waymap: /wp add [name] · /wp del <name> · /wp list · /wp rename <old> <new> · /wp color <name> <colour|#rrggbb> · /wp clear · /wp edit <name> · /wp hide <name> · /wp show <name>"
                    .to_string(),
            ));
        }
        let (sub, rest) = match args.split_once(' ') {
            Some((a, b)) => (a, b.trim()),
            None => (args, ""),
        };
        match sub {
            "add" => {
                let Some(p) = game::get_player() else {
                    return Err("Waymap: not in a world".to_string());
                };
                if self.world.is_none() {
                    return Err("Waymap: not in a world".to_string());
                }
                let name = if rest.is_empty() {
                    format!("Waypoint {}", self.store_wps.waypoints.len() + 1)
                } else {
                    clean_name(rest)
                };
                let w = Waypoint {
                    name: name.clone(),
                    x: p.x.floor() as i32,
                    y: p.y.floor() as i32,
                    z: p.z.floor() as i32,
                    dimension: p.dimension.clone(),
                    color: PALETTE[self.store_wps.waypoints.len() % 16].1,
                    hidden: false,
                    death: false,
                };
                let (x, y, z) = (w.x, w.y, w.z);
                if !self.store_wps.push_capped(w) {
                    return Err(format!(
                        "Waymap: the list is full ({CAP} waypoints) and no death waypoint can go"
                    ));
                }
                self.wps_dirty = true;
                Ok(Some(format!("Waymap: added '{name}' at {x}, {y}, {z}")))
            }
            "del" => {
                let name = clean_name(rest);
                match self.store_wps.waypoints.iter().position(|w| w.name == name) {
                    Some(i) => {
                        self.store_wps.waypoints.remove(i);
                        self.wps_dirty = true;
                        Ok(Some(format!("Waymap: deleted '{name}'")))
                    }
                    None => Err(format!("Waymap: no waypoint called '{name}' here")),
                }
            }
            "list" => {
                if self.store_wps.waypoints.is_empty() {
                    return Ok(Some("Waymap: no waypoints in this world".to_string()));
                }
                let p = game::get_player();
                let lines: Vec<String> = self
                    .store_wps
                    .waypoints
                    .iter()
                    .enumerate()
                    .map(|(i, w)| {
                        let dist = match &p {
                            Some(p) if p.dimension == w.dimension => {
                                format!(" — {:.0}m", distance((p.x, p.y, p.z), w))
                            }
                            _ => String::new(),
                        };
                        let hidden = if w.hidden { " (hidden)" } else { "" };
                        let death = if w.death { " ✕" } else { "" };
                        format!(
                            "{}. {} ({}, {}, {}) [{}]{dist}{hidden}{death}",
                            i + 1,
                            w.name,
                            w.x,
                            w.y,
                            w.z,
                            dim_short(&w.dimension)
                        )
                    })
                    .collect();
                Ok(Some(lines.join("\n")))
            }
            "rename" => {
                // The old name is whatever leading part of
                // the arguments names a waypoint here.
                let target = self
                    .store_wps
                    .waypoints
                    .iter()
                    .find(|w| rest.starts_with(&w.name) && rest.len() > w.name.len())
                    .map(|w| w.name.clone());
                let Some(old) = target else {
                    return Err("Waymap: /wp rename <old> <new>".to_string());
                };
                let new = clean_name(rest[old.len()..].trim());
                if new.is_empty() {
                    return Err("Waymap: the new name is empty".to_string());
                }
                if let Some(w) = self.store_wps.waypoints.iter_mut().find(|w| w.name == old) {
                    w.name = new.clone();
                }
                self.wps_dirty = true;
                Ok(Some(format!("Waymap: '{old}' is now '{new}'")))
            }
            "color" => {
                let Some((name, color)) = split_color_args(rest) else {
                    return Err(
                        "Waymap: /wp color <name> <colour name or #rrggbb>".to_string(),
                    );
                };
                match self.store_wps.waypoints.iter_mut().find(|w| w.name == name) {
                    Some(w) => {
                        w.color = color;
                        self.wps_dirty = true;
                        Ok(Some(format!(
                            "Waymap: '{name}' is now #{:06X}",
                            color & 0xFF_FFFF
                        )))
                    }
                    None => Err(format!("Waymap: no waypoint called '{name}' here")),
                }
            }
            "clear" => {
                let n = self.store_wps.waypoints.len();
                self.store_wps.waypoints.clear();
                self.wps_dirty = true;
                Ok(Some(format!("Waymap: cleared {n} waypoints in this world")))
            }
            "edit" => {
                let name = clean_name(rest);
                match self.store_wps.waypoints.iter().find(|w| w.name == name) {
                    Some(w) => Ok(Some(format!(
                        "Waymap: {} — {}, {}, {} in {}, colour #{:06X}{}",
                        w.name,
                        w.x,
                        w.y,
                        w.z,
                        dim_short(&w.dimension),
                        w.color & 0xFF_FFFF,
                        if w.hidden { ", hidden" } else { "" }
                    ))),
                    None => Err(format!("Waymap: no waypoint called '{name}' here")),
                }
            }
            "hide" | "show" => {
                let name = clean_name(rest);
                let hidden = sub == "hide";
                match self.store_wps.waypoints.iter_mut().find(|w| w.name == name) {
                    Some(w) => {
                        w.hidden = hidden;
                        self.wps_dirty = true;
                        Ok(Some(format!(
                            "Waymap: '{name}' is {}",
                            if hidden { "hidden" } else { "shown" }
                        )))
                    }
                    None => Err(format!("Waymap: no waypoint called '{name}' here")),
                }
            }
            _ => Err("Waymap: unknown — /wp help".to_string()),
        }
    }
}

impl Mod for Waymap {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(CREATE_KEY, "Create Waypoint", "key.keyboard.n")?;
        input::register_key(LIST_KEY, "Waypoint List", "key.keyboard.b")?;
        input::register_key(LIST_KEY_M, "Waypoint List", "key.keyboard.m")?;
        let settings = storage::get("settings")
            .and_then(|b| String::from_utf8(b).ok())
            .map(|t| Settings::parse(&t))
            .unwrap_or_default();
        Ok((
            Waymap {
                settings,
                world: None,
                hash: String::new(),
                store_wps: Store::default(),
                wps_dirty: false,
                tiles: TileStore::default(),
                known: HashMap::new(),
                verified: HashSet::new(),
                fetch: VecDeque::new(),
                queued: HashSet::new(),
                feed_dim: None,
                feed_cave: false,
                filter: Filter::All,
                selected: None,
                list_open: false,
                editor: None,
                last_labels: String::new(),
                last_panel: String::new(),
            },
            Registration::new().on_tick().on_frame().command("wp"),
        ))
    }

    fn tick(&mut self, tick: u64) {
        match map::context() {
            Some(c) => {
                if self.world.as_deref() != Some(c.world.as_str()) {
                    self.save_wps();
                    self.load_world(&c.world);
                }
            }
            None => {
                if self.world.is_some() {
                    self.save_wps();
                    self.world = None;
                    self.hash.clear();
                    map::set_minimap_image(None);
                    hud::clear();
                    self.last_panel.clear();
                }
            }
        }
        for event in input::poll() {
            let Event::KeyDown(k) = event else { continue };
            match k.as_str() {
                CREATE_KEY => {
                    if let Some(p) = game::get_player() {
                        let n = self.store_wps.waypoints.len();
                        self.open_editor(
                            None,
                            format!("Waypoint {}", n + 1),
                            p.x.floor() as i32,
                            p.y.floor() as i32,
                            p.z.floor() as i32,
                            p.dimension.clone(),
                            PALETTE[n % 16].1,
                            false,
                            false,
                        );
                    }
                }
                LIST_KEY | LIST_KEY_M => {
                    self.selected = None;
                    self.open_list();
                }
                _ => {}
            }
        }
        let Some(player) = game::get_player() else {
            return;
        };
        if self.world.is_none() {
            return;
        }
        if self.feed_dim.as_deref() != Some(player.dimension.as_str()) {
            self.tiles = TileStore::default();
            self.known.clear();
            self.verified.clear();
            self.fetch.clear();
            self.queued.clear();
            self.feed_dim = Some(player.dimension.clone());
            self.feed_cave = false;
        }
        for event in map::poll_events() {
            if let horizon_mod_sdk::PositionEvent::Died(pos) = event {
                self.store_wps.deaths += 1;
                let n = self.store_wps.deaths;
                let w = Waypoint {
                    name: format!("Death {n}"),
                    x: pos.x,
                    y: pos.y,
                    z: pos.z,
                    dimension: pos.dimension.clone(),
                    color: DEATH_COLOR,
                    hidden: false,
                    death: true,
                };
                if self.store_wps.push_capped(w) {
                    self.wps_dirty = true;
                }
            }
        }
        let pc = (
            (player.x.floor() as i32) >> 4,
            (player.z.floor() as i32) >> 4,
        );
        if tick % 5 == 0 {
            self.sweep(pc);
            let dots: Vec<(i32, i32, u32, bool)> = self
                .store_wps
                .waypoints
                .iter()
                .filter(|w| !w.hidden && w.dimension == player.dimension)
                .map(|w| (w.x, w.z, w.color, w.death))
                .collect();
            let img = compose_minimap(
                &self.tiles,
                player.x,
                player.z,
                player.yaw,
                player.y as i32,
                &dots,
            );
            map::set_minimap_image(Some(&img));
        }
        self.drain_fetch(pc);
        if self.wps_dirty {
            self.save_wps();
        }
        if tick % 10 == 0 {
            // The HUD panel: the nearest waypoints with
            // their distances, in the bottom-left.
            let mut sig = String::new();
            if self.settings.hud_panel {
                let ppos = (player.x, player.y, player.z);
                let mut near: Vec<&Waypoint> = self
                    .store_wps
                    .waypoints
                    .iter()
                    .filter(|w| !w.hidden && w.dimension == player.dimension)
                    .collect();
                near.sort_by(|a, b| {
                    distance(ppos, a)
                        .partial_cmp(&distance(ppos, b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let lines: Vec<hud::Line> = near
                    .iter()
                    .take(8)
                    .map(|w| {
                        let d = distance(ppos, w);
                        sig.push_str(&format!("{}:{:.0};", w.name, d));
                        hud::Line {
                            text: format!("{} — {d:.0}m", w.name),
                            color: w.color & 0x00FF_FFFF,
                        }
                    })
                    .collect();
                if sig != self.last_panel {
                    if lines.is_empty() {
                        hud::clear();
                    } else {
                        hud::set_panel(hud::Corner::BottomLeft, &lines);
                    }
                    self.last_panel = sig;
                }
            } else if !self.last_panel.is_empty() {
                hud::clear();
                self.last_panel.clear();
            }
        }
    }

    fn frame(&mut self, _seconds: f32) {
        // In-world markers: the waypoint's letter (its
        // initial, a red X for a death) over its place, and
        // with labels on, its name and distance under it.
        if !self.settings.markers || self.world.is_none() {
            if !self.last_labels.is_empty() {
                render::set_hud(&[]);
                self.last_labels.clear();
            }
            return;
        }
        let Some(player) = game::get_player() else {
            if !self.last_labels.is_empty() {
                render::set_hud(&[]);
                self.last_labels.clear();
            }
            return;
        };
        let ppos = (player.x, player.y, player.z);
        let mut near: Vec<&Waypoint> = self
            .store_wps
            .waypoints
            .iter()
            .filter(|w| !w.hidden && w.dimension == player.dimension)
            .collect();
        near.sort_by(|a, b| {
            distance(ppos, a)
                .partial_cmp(&distance(ppos, b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut shapes: Vec<Shape> = Vec::new();
        let mut sig = String::new();
        for w in near.iter().take(24) {
            let sp = render::project(WorldPoint {
                x: f64::from(w.x) + 0.5,
                y: f64::from(w.y) + 2.0,
                z: f64::from(w.z) + 0.5,
            });
            if !sp.visible {
                continue;
            }
            let d = distance(ppos, w);
            sig.push_str(&format!("{}@{:.0},{:.0};", w.name, sp.x, sp.y));
            let (mark, color) = if w.death {
                ("✕".to_string(), 0xFF_0000u32)
            } else {
                (
                    w.name
                        .chars()
                        .next()
                        .map(|c| c.to_uppercase().collect::<String>())
                        .unwrap_or_else(|| "?".to_string()),
                    w.color & 0x00FF_FFFF,
                )
            };
            shapes.push(Shape::Text(TextShape {
                text: mark,
                at: Point { x: sp.x, y: sp.y },
                color,
                centered: true,
                shadow: true,
            }));
            if self.settings.labels {
                shapes.push(Shape::Text(TextShape {
                    text: w.name.clone(),
                    at: Point {
                        x: sp.x,
                        y: sp.y + 9.0,
                    },
                    color: 0xFF_FFFF,
                    centered: true,
                    shadow: true,
                }));
                shapes.push(Shape::Text(TextShape {
                    text: format!("{d:.0}m"),
                    at: Point {
                        x: sp.x,
                        y: sp.y + 18.0,
                    },
                    color: 0xFF_FFFF,
                    centered: true,
                    shadow: true,
                }));
            }
        }
        if sig != self.last_labels {
            render::set_hud(&shapes);
            self.last_labels = sig;
        }
    }

    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        self.wp_command(args)
    }

    fn ui_settings_opened(&mut self) {
        ui::set_settings(&[
            button(0, format!("Markers: {}", yn(self.settings.markers))),
            button(1, format!("Labels: {}", yn(self.settings.labels))),
            button(2, format!("HUD Panel: {}", yn(self.settings.hud_panel))),
        ]);
    }

    fn ui_closed(&mut self) {
        self.list_open = false;
        self.editor = None;
    }

    fn ui_select(&mut self, id: u32, row: u32) {
        match id {
            0 | 1 | 2 => {
                match id {
                    0 => self.settings.markers = !self.settings.markers,
                    1 => self.settings.labels = !self.settings.labels,
                    _ => self.settings.hud_panel = !self.settings.hud_panel,
                }
                self.save_settings();
                self.ui_settings_opened();
            }
            LIST_WIDGET => {
                self.selected = Some(row as usize);
            }
            id if (FILTER_BASE..FILTER_BASE + 5).contains(&id) => {
                self.filter = FILTERS[(id - FILTER_BASE) as usize];
                self.selected = None;
                self.open_list();
            }
            LIST_NEW_BUTTON => {
                if let Some(p) = game::get_player() {
                    let n = self.store_wps.waypoints.len();
                    self.open_editor(
                        None,
                        format!("Waypoint {}", n + 1),
                        p.x.floor() as i32,
                        p.y.floor() as i32,
                        p.z.floor() as i32,
                        p.dimension.clone(),
                        PALETTE[n % 16].1,
                        false,
                        true,
                    );
                }
            }
            LIST_EDIT_BUTTON => {
                if let Some((real, w)) = self.selected_waypoint() {
                    self.open_editor(
                        Some(real),
                        w.name,
                        w.x,
                        w.y,
                        w.z,
                        w.dimension,
                        w.color,
                        w.hidden,
                        true,
                    );
                }
            }
            LIST_HIDE_BUTTON => {
                if let Some((real, _)) = self.selected_waypoint() {
                    let w = &mut self.store_wps.waypoints[real];
                    w.hidden = !w.hidden;
                    self.wps_dirty = true;
                    self.open_list();
                }
            }
            LIST_DELETE_BUTTON => {
                if let Some((real, _)) = self.selected_waypoint() {
                    self.store_wps.waypoints.remove(real);
                    self.wps_dirty = true;
                    self.selected = None;
                    self.open_list();
                }
            }
            EDITOR_STEP_BUTTON => {
                if let Some(e) = &mut self.editor {
                    e.step = (e.step + 1) % STEPS.len();
                }
                self.show_editor();
            }
            EDITOR_XM | EDITOR_XP | EDITOR_YM | EDITOR_YP | EDITOR_ZM | EDITOR_ZP => {
                if let Some(e) = &mut self.editor {
                    let d = STEPS[e.step];
                    match id {
                        EDITOR_XM => e.x -= d,
                        EDITOR_XP => e.x += d,
                        EDITOR_YM => e.y -= d,
                        EDITOR_YP => e.y += d,
                        EDITOR_ZM => e.z -= d,
                        _ => e.z += d,
                    }
                }
                self.show_editor();
            }
            EDITOR_POS_BUTTON => {
                if let Some(p) = game::get_player()
                    && let Some(e) = &mut self.editor
                {
                    e.x = p.x.floor() as i32;
                    e.y = p.y.floor() as i32;
                    e.z = p.z.floor() as i32;
                    e.dimension = p.dimension.clone();
                }
                self.show_editor();
            }
            EDITOR_DIM_BUTTON => {
                if let Some(e) = &mut self.editor {
                    let mut stops: Vec<String> = vec![
                        "minecraft:overworld".to_string(),
                        "minecraft:the_nether".to_string(),
                        "minecraft:the_end".to_string(),
                    ];
                    if !stops.contains(&e.dimension) {
                        stops.push(e.dimension.clone());
                    }
                    let cur = stops.iter().position(|d| *d == e.dimension).unwrap_or(0);
                    e.dimension = stops[(cur + 1) % stops.len()].clone();
                }
                self.show_editor();
            }
            EDITOR_COLOR_BUTTON => {
                if let Some(e) = &mut self.editor {
                    let cur = PALETTE.iter().position(|(_, v)| *v == e.color);
                    let next = cur.map(|i| (i + 1) % 16).unwrap_or(0);
                    e.color = PALETTE[next].1;
                }
                self.show_editor();
            }
            EDITOR_HIDDEN_BUTTON => {
                if let Some(e) = &mut self.editor {
                    e.hidden = !e.hidden;
                }
                self.show_editor();
            }
            EDITOR_SAVE_BUTTON => {
                let Some(e) = self.editor.clone() else { return };
                let w = Waypoint {
                    name: if e.name.trim().is_empty() {
                        "Waypoint".to_string()
                    } else {
                        clean_name(&e.name)
                    },
                    x: e.x,
                    y: e.y,
                    z: e.z,
                    dimension: e.dimension.clone(),
                    color: e.color,
                    hidden: e.hidden,
                    death: false,
                };
                match e.index {
                    Some(i) if i < self.store_wps.waypoints.len() => {
                        let death = self.store_wps.waypoints[i].death;
                        self.store_wps.waypoints[i] = Waypoint { death, ..w };
                        self.wps_dirty = true;
                        self.leave_editor();
                    }
                    _ => {
                        if self.store_wps.push_capped(w) {
                            self.wps_dirty = true;
                            self.leave_editor();
                        } else {
                            chat::show(&format!(
                                "Waymap: the list is full ({CAP} waypoints) and no death waypoint can go"
                            ));
                        }
                    }
                }
            }
            EDITOR_CANCEL_BUTTON => {
                self.leave_editor();
            }
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(Waymap);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_25() {
        assert_eq!(API_LEVEL, 25);
    }

    #[test]
    fn shading_matches_the_map_rule() {
        assert_eq!(brightness(7 | WALL, 64, Some(70), 0, 0, 0), 3);
        assert_eq!(shade(2, 70, Some(60), 0, 4, 4), 0xFFF7_E9A3);
        assert_eq!(shade(0, 70, Some(60), 0, 0, 0), UNKNOWN);
    }

    #[test]
    fn store_blob_roundtrips() {
        let mut st = Store::default();
        st.deaths = 3;
        st.waypoints = vec![
            Waypoint {
                name: "Home".into(),
                x: 1,
                y: 2,
                z: -3,
                dimension: "minecraft:overworld".into(),
                color: 0xFF55_FF55,
                hidden: false,
                death: false,
            },
            Waypoint {
                name: "Death 2".into(),
                x: 9,
                y: 8,
                z: 7,
                dimension: "minecraft:the_nether".into(),
                color: DEATH_COLOR,
                hidden: true,
                death: true,
            },
        ];
        let back = decode_store(&encode_store(&st));
        assert_eq!(back.deaths, 3);
        assert_eq!(back.waypoints, st.waypoints);
    }

    #[test]
    fn colors_parse() {
        assert_eq!(parse_color("dark blue"), Some(0xFF00_00AA));
        assert_eq!(parse_color("#12ab34"), Some(0xFF12_AB34));
        assert_eq!(parse_color("pink"), None);
        let (name, c) = split_color_args("My Base Dark Blue").unwrap();
        assert_eq!(name, "My Base");
        assert_eq!(c, 0xFF00_00AA);
        let (name, c) = split_color_args("Home #ff0000").unwrap();
        assert_eq!(name, "Home");
        assert_eq!(c, 0xFFFF_0000);
        assert!(split_color_args("Home").is_none());
    }

    #[test]
    fn the_cap_evicts_the_oldest_death() {
        let mut st = Store::default();
        for i in 0..CAP {
            st.waypoints.push(Waypoint {
                name: format!("W{i}"),
                x: 0,
                y: 0,
                z: 0,
                dimension: "minecraft:overworld".into(),
                color: 0,
                hidden: false,
                death: i == 3,
            });
        }
        let extra = Waypoint {
            name: "New".into(),
            x: 1,
            y: 1,
            z: 1,
            dimension: "minecraft:overworld".into(),
            color: 0,
            hidden: false,
            death: false,
        };
        assert!(st.push_capped(extra));
        assert_eq!(st.waypoints.len(), CAP);
        assert!(!st.waypoints.iter().any(|w| w.name == "W3"));
        let mut full = Store::default();
        for i in 0..CAP {
            full.waypoints.push(Waypoint {
                name: format!("W{i}"),
                x: 0,
                y: 0,
                z: 0,
                dimension: "minecraft:overworld".into(),
                color: 0,
                hidden: false,
                death: false,
            });
        }
        let extra = Waypoint {
            name: "New".into(),
            x: 1,
            y: 1,
            z: 1,
            dimension: "minecraft:overworld".into(),
            color: 0,
            hidden: false,
            death: false,
        };
        assert!(!full.push_capped(extra));
    }

    #[test]
    fn filters_match_dimensions_and_deaths() {
        let w = Waypoint {
            name: "A".into(),
            x: 0,
            y: 0,
            z: 0,
            dimension: "minecraft:the_nether".into(),
            color: 0,
            hidden: false,
            death: true,
        };
        assert!(Filter::All.matches(&w));
        assert!(Filter::Nether.matches(&w));
        assert!(!Filter::Overworld.matches(&w));
        assert!(Filter::Deaths.matches(&w));
    }

    #[test]
    fn the_minimap_composes() {
        let mut store = TileStore::default();
        store.put(
            (0, 0),
            Tile {
                layer: MapLayer::Surface,
                version: 1,
                colors: [2; 256],
                heights: [64; 256],
                depths: [0; 256],
            },
        );
        let img = compose_minimap(&store, 8.0, 8.0, 0.0, 64, &[(24, 8, 0xFFFF_0000, true)]);
        assert_eq!(img.image.width, MINIMAP_SIZE);
        assert_eq!(img.image.pixels.len(), (MINIMAP_SIZE * MINIMAP_SIZE) as usize);
        // The centre is the player's white mark.
        let c = (MINIMAP_SIZE / 2 * MINIMAP_SIZE + MINIMAP_SIZE / 2) as usize;
        assert_eq!(img.image.palette[img.image.pixels[c] as usize], 0xFFFF_FFFF);
    }
}

//! World Map for Kestrel: a full-screen map that writes
//! itself as you explore, a minimap, and waypoints with
//! groups, import and export, and the `/wp` commands.
//!
//! Behaviour reimplemented from World Map by Px228-Da-Da,
//! MIT, <https://github.com/Px228-Da-Da/horizon-worldmap>:
//! this file shares no code with the original — the original
//! is a native mod for Horizon, this is a fresh
//! implementation against the Kestrel SDK (API 25,
//! capability `map`). This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/worldmap>.
//!
//! Fidelity, against the original's registry description: the
//! terrain you have seen stays on the map after you leave,
//! per world, dimension and cave layer, in the vanilla map
//! colours — here, an accumulation of the game's own map
//! tiles (`map.tiles`) in the mod's `storage`, keyed by
//! `map.context`, shaded by the map item's own rule and
//! darkened at night. Waypoints have groups, hide, share and
//! teleport; the `/wp` commands are the original's (add, del,
//! rename, list, clear, share); a waypoint is made where you
//! die. What is NOT ported is listed in the README: cave
//! layers at a Top Y you never stood at (the host scans the
//! layer at your head, so layers are the visited heights),
//! PNG export and waypoint files (no surface writes a file
//! for the user; export prints its text in chat instead),
//! block light at night (the tiles carry no light), other
//! players on the minimap (no SDK read gives their
//! positions), and the Russian interface.

use std::collections::{HashMap, HashSet, VecDeque};

use horizon_mod_sdk::{
    ChunkPos, Corner as MapCorner, MapImage, MapLayer, MapMarker, MapScreen, Mod, Registration,
    chat, command, game, input,
    input::Event,
    map,
    render::{self, Point, Shape, TextShape, WorldPoint},
    storage, ui,
    ui::{ButtonWidget, ListWidget, TextWidget, Widget},
    world,
};

/// The newest API level this mod needs, asserted in the tests
/// below: the whole `map` interface is level 25
/// (`crates/horizon-mods/src/api_level.rs`); the rest is older.
#[cfg(test)]
const API_LEVEL: u32 = 25;

// ----------------------------------------------------------------
// The map item's colours and shading, restated: the tiles a
// mod reads are colour ids, and turning them into a picture
// is the vanilla rule — `MapColor.col` by id, shaded by the
// brightness the map item computes (brighter facing a lower
// north neighbour, darker facing a higher one, dithered;
// water by depth; a cut wall brightest). The game applies
// the same rule in `horizon_client::minimap`; this is that
// rule, so the port's map matches the game's own.

/// `MapColor.col` by id (0 is none), as packed 0xRRGGBB.
const MAP_COLORS: [u32; 62] = [
    0, 8368696, 16247203, 13092807, 16711680, 10526975, 10987431, 31744, 16777215, 10791096,
    9923917, 7368816, 4210943, 9402184, 16776437, 14188339, 11685080, 6724056, 15066419, 8375321,
    15892389, 5000268, 10066329, 5013401, 8339378, 3361970, 6704179, 6717235, 10040115, 1644825,
    16445005, 6085589, 4882687, 55610, 8476209, 7340544, 13742497, 10441252, 9787244, 7367818,
    12223780, 6780213, 10505550, 3746083, 8874850, 5725276, 8014168, 4996700, 4993571, 5001770,
    9321518, 2430480, 12398641, 9715553, 6035741, 1474182, 3837580, 5647422, 1356933, 6579300,
    14200723, 8365974,
];
/// The wall mark on a cave tile's colours.
const WALL: u8 = 0x80;
/// The water colour id: shaded by depth, not by slope.
const WATER: u8 = 12;
/// Unexplored ground, as the game's own map draws it.
const UNKNOWN: u32 = 0xC814_1414;

/// The map item's brightness for one column: 0..=3.
fn brightness(color: u8, height: i16, north: Option<i16>, depth: u8, parity: i32) -> u8 {
    if color & WALL != 0 {
        return 3;
    }
    let parity = f64::from(parity & 1);
    if color == WATER {
        let diff = f64::from(depth) * 0.1 + parity * 0.2;
        return if diff < 0.5 {
            2
        } else if diff > 0.9 {
            0
        } else {
            1
        };
    }
    let north = north.unwrap_or(height);
    let diff = f64::from(height - north) * 4.0 / 5.0 + (parity - 0.5) * 0.4;
    if diff > 0.6 {
        2
    } else if diff < -0.6 {
        0
    } else {
        1
    }
}

/// One column as a 0xAARRGGBB picture colour, at a daylight
/// factor (1.0 by day; the night darkening scales the
/// channels, never the alpha). Colour id 0 (or past the
/// table) reads as the game's unexplored colour.
fn shade(color: u8, height: i16, north: Option<i16>, depth: u8, x: i32, z: i32, day: f32) -> u32 {
    let base = color & !WALL;
    if base == 0 || (base as usize) >= MAP_COLORS.len() {
        return UNKNOWN;
    }
    let b = brightness(color, height, north, depth, x + z);
    let modifier = [180u32, 220, 255, 135][(b & 3) as usize];
    let col = MAP_COLORS[base as usize];
    let ch = |s: u32| {
        let v = (col >> s & 255) * modifier / 255;
        (v as f32 * day) as u32
    };
    0xFF00_0000 | ch(16) << 16 | ch(8) << 8 | ch(0)
}

/// The daylight factor for a day-time: 1.0 at midday, 0.35
/// at midnight, a cosine between — the port's curve for the
/// original's day-and-night map (its per-column block light
/// is the documented gap: the tiles carry no light).
fn daylight(day_time: u64) -> f32 {
    let t = (day_time % 24000) as f64;
    (0.675 + 0.325 * (std::f64::consts::TAU * (t - 6000.0) / 24000.0).cos()) as f32
}

// ----------------------------------------------------------------
// Tiles: what the game holds, kept by the mod.

/// One chunk of the game's map, as the mod keeps it.
#[derive(Clone)]
struct Tile {
    layer: MapLayer,
    cut_y: Option<i32>,
    version: u64,
    colors: [u8; 256],
    heights: [i16; 256],
    depths: [u8; 256],
}

impl Tile {
    fn from_sdk(t: &horizon_mod_sdk::MapTile) -> Option<Tile> {
        if t.colors.len() != 256 || t.heights.len() != 256 || t.depths.len() != 256 {
            return None;
        }
        let mut tile = Tile {
            layer: t.layer,
            cut_y: t.cut_y,
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
    /// The column at local (x, z) of this chunk.
    fn column(&self, x: i32, z: i32) -> (u8, i16, u8) {
        let i = (z * 16 + x) as usize;
        (self.colors[i], self.heights[i], self.depths[i])
    }
}

/// The mod's accumulated map of one context: surface tiles
/// by chunk, cave tiles by chunk and the head height they
/// were scanned at.
#[derive(Default)]
struct TileStore {
    surface: HashMap<(i32, i32), Tile>,
    cave: HashMap<(i32, i32), Vec<Tile>>,
}

/// How a composition picks its tiles.
#[derive(Clone, Copy, PartialEq)]
struct Display {
    /// 0 Auto (the game's own layer), 1 Surface, 2 Cave.
    mode: u8,
    /// In a cave display: the full style picks the cut at
    /// the Top Y setting; the layered style the cut at the
    /// player's head.
    full: bool,
    top_y: i32,
}

impl TileStore {
    fn put(&mut self, chunk: (i32, i32), tile: Tile) {
        match tile.layer {
            MapLayer::Surface => {
                self.surface.insert(chunk, tile);
            }
            MapLayer::Cave => {
                let cuts = self.cave.entry(chunk).or_default();
                let cut = tile.cut_y;
                if let Some(slot) = cuts.iter_mut().find(|t| t.cut_y == cut) {
                    *slot = tile;
                } else {
                    cuts.push(tile);
                }
            }
        }
    }
    fn display(&self, chunk: (i32, i32), d: Display, feed_cave: bool, head_y: i32) -> Option<&Tile> {
        let cave_at = |y: i32| {
            self.cave.get(&chunk).and_then(|cuts| {
                cuts.iter().min_by_key(|t| {
                    let c = t.cut_y.unwrap_or(y);
                    ((c - y).abs(), if c <= y { 0 } else { 1 })
                })
            })
        };
        match d.mode {
            1 => self.surface.get(&chunk),
            2 => {
                let y = if d.full { d.top_y } else { head_y };
                cave_at(y).or_else(|| self.surface.get(&chunk))
            }
            _ => {
                if feed_cave {
                    let y = if d.full { d.top_y } else { head_y };
                    cave_at(y).or_else(|| self.surface.get(&chunk))
                } else {
                    self.surface.get(&chunk)
                }
            }
        }
    }
    /// The shaded colour of one world column, or None where
    /// the store holds no tile for it.
    fn sample(
        &self,
        wx: i32,
        wz: i32,
        d: Display,
        feed_cave: bool,
        head_y: i32,
        day: f32,
    ) -> Option<u32> {
        let chunk = (wx >> 4, wz >> 4);
        let tile = self.display(chunk, d, feed_cave, head_y)?;
        let (color, height, depth) = tile.column(wx & 15, wz & 15);
        let north = if (wz & 15) == 0 {
            self.display((chunk.0, chunk.1 - 1), d, feed_cave, head_y)
                .map(|t| t.column(wx & 15, 15).1)
        } else {
            Some(tile.column(wx & 15, (wz & 15) - 1).1)
        };
        Some(shade(color, height, north, depth, wx, wz, day))
    }
}

// ----------------------------------------------------------------
// Persistence: regions of 8x8 chunks under the mod's storage,
// a run-length codec over the tiles' arrays — the same
// machinery as Atlas's port, since both stand on the one
// tile feed. (Each mod is its own crate; the codec is
// carried, not shared.)

/// How many chunks a stored region spans, each way.
const REGION: i32 = 8;

/// A short stable hash of a context's names (FNV-1a):
/// storage keys are `[a-z0-9_.-]`, world and server names
/// are not.
fn context_hash(world: &str, dimension: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in world.bytes().chain([1]).chain(dimension.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn region_key(ctx: &str, rx: i32, rz: i32) -> String {
    format!("t.{ctx}.{rx}.{rz}")
}
fn index_key(ctx: &str) -> String {
    format!("i.{ctx}")
}
fn waypoints_key(ctx: &str) -> String {
    format!("w.{ctx}")
}

/// Adds a tile to a region's encoding list when its chunk
/// belongs to the region, under its slot in the region.
fn push_region_tile<'a>(
    tiles: &mut Vec<(u8, &'a Tile)>,
    region: (i32, i32),
    chunk: (i32, i32),
    tile: &'a Tile,
) {
    if (chunk.0.div_euclid(REGION), chunk.1.div_euclid(REGION)) == region {
        let slot = ((chunk.1.rem_euclid(REGION)) * REGION + chunk.0.rem_euclid(REGION)) as u8;
        tiles.push((slot, tile));
    }
}

/// Run-length encodes bytes as (value, count) pairs, count
/// 1..=255.
fn rle_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let v = data[i];
        let mut n = 1;
        while i + n < data.len() && data[i + n] == v && n < 255 {
            n += 1;
        }
        out.push(v);
        out.push(n as u8);
        i += n;
    }
    out
}

/// Decodes (value, count) pairs into exactly `want` bytes.
fn rle_decode(data: &[u8], want: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(want);
    let mut i = 0;
    while out.len() < want {
        let (v, n) = (*data.get(i)?, *data.get(i + 1)? as usize);
        i += 2;
        if out.len() + n > want {
            return None;
        }
        out.extend(std::iter::repeat_n(v, n));
    }
    if out.len() == want { Some(out) } else { None }
}

/// Writes a zigzag varint.
fn put_varint(out: &mut Vec<u8>, v: i64) {
    let mut u = ((v << 1) ^ (v >> 63)) as u64;
    loop {
        let b = (u & 0x7f) as u8;
        u >>= 7;
        if u == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Reads a zigzag varint; advances `at`.
fn get_varint(data: &[u8], at: &mut usize) -> Option<i64> {
    let mut u: u64 = 0;
    let mut shift = 0;
    loop {
        let b = *data.get(*at)?;
        *at += 1;
        u |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
    Some(((u >> 1) as i64) ^ -((u & 1) as i64))
}

/// One region's tiles as bytes.
fn encode_region(tiles: &[(u8, &Tile)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(tiles.len() as u16).to_le_bytes());
    for (slot, tile) in tiles {
        out.push(*slot);
        out.push(match tile.layer {
            MapLayer::Surface => 0,
            MapLayer::Cave => 1,
        });
        out.extend_from_slice(&tile.cut_y.unwrap_or(0).to_le_bytes());
        out.extend_from_slice(&tile.version.to_le_bytes());
        out.extend_from_slice(&rle_encode(&tile.colors));
        let mut prev: i64 = 0;
        let mut deltas = Vec::with_capacity(512);
        for h in tile.heights {
            let h = i64::from(h);
            put_varint(&mut deltas, h - prev);
            prev = h;
        }
        out.extend_from_slice(&(deltas.len() as u16).to_le_bytes());
        out.extend_from_slice(&deltas);
        out.extend_from_slice(&rle_encode(&tile.depths));
    }
    out
}

/// A region blob back into (chunk, tile) pairs; a blob that
/// does not parse reads as an empty region.
fn decode_region(rx: i32, rz: i32, data: &[u8]) -> Vec<((i32, i32), Tile)> {
    let mut out = Vec::new();
    if data.len() < 2 {
        return out;
    }
    let count = u16::from_le_bytes([data[0], data[1]]) as usize;
    let mut at = 2;
    for _ in 0..count {
        let Some(&slot) = data.get(at) else { return out };
        let Some(&layer_b) = data.get(at + 1) else {
            return out;
        };
        at += 2;
        let Some(cut_b) = data.get(at..at + 4) else {
            return out;
        };
        let cut = i32::from_le_bytes(cut_b.try_into().unwrap());
        at += 4;
        let Some(ver_b) = data.get(at..at + 8) else {
            return out;
        };
        let version = u64::from_le_bytes(ver_b.try_into().unwrap());
        at += 8;
        let start = at;
        let mut total = 0usize;
        while total < 256 {
            let Some(&n) = data.get(at + 1) else { return out };
            total += n as usize;
            at += 2;
        }
        let Some(colors_v) = rle_decode(&data[start..at], 256) else {
            return out;
        };
        let Some(len_b) = data.get(at..at + 2) else {
            return out;
        };
        let hlen = u16::from_le_bytes(len_b.try_into().unwrap()) as usize;
        at += 2;
        let Some(hdata) = data.get(at..at + hlen) else {
            return out;
        };
        at += hlen;
        let mut heights = [0i16; 256];
        let mut hat = 0usize;
        let mut prev: i64 = 0;
        let mut ok = true;
        for h in heights.iter_mut() {
            match get_varint(hdata, &mut hat) {
                Some(d) => {
                    prev += d;
                    *h = prev as i16;
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            return out;
        }
        let start = at;
        let mut total = 0usize;
        while total < 256 {
            let Some(&n) = data.get(at + 1) else { return out };
            total += n as usize;
            at += 2;
        }
        let Some(depths_v) = rle_decode(&data[start..at], 256) else {
            return out;
        };
        let mut tile = Tile {
            layer: if layer_b == 1 {
                MapLayer::Cave
            } else {
                MapLayer::Surface
            },
            cut_y: if layer_b == 1 { Some(cut) } else { None },
            version,
            colors: [0; 256],
            heights,
            depths: [0; 256],
        };
        tile.colors.copy_from_slice(&colors_v);
        tile.depths.copy_from_slice(&depths_v);
        let chunk = (rx * REGION + i32::from(slot % 8), rz * REGION + i32::from(slot / 8));
        out.push((chunk, tile));
    }
    out
}

// ----------------------------------------------------------------
// Waypoints: sixteen colours, groups, and the store.

/// The sixteen waypoint colours.
const WP_COLORS: [(&str, u32); 16] = [
    ("Black", 0xFF20_2020),
    ("Dark Blue", 0xFF00_00AA),
    ("Dark Green", 0xFF00_AA00),
    ("Dark Aqua", 0xFF00_AAAA),
    ("Dark Red", 0xFFAA_0000),
    ("Purple", 0xFFAA_00AA),
    ("Gold", 0xFFFF_AA00),
    ("Gray", 0xFFAA_AAAA),
    ("Dark Gray", 0xFF55_5555),
    ("Blue", 0xFF55_55FF),
    ("Green", 0xFF55_FF55),
    ("Aqua", 0xFF55_FFFF),
    ("Red", 0xFFFF_5555),
    ("Pink", 0xFFFF_55FF),
    ("Yellow", 0xFFFF_FF55),
    ("White", 0xFFFF_FFFF),
];
const RED: usize = 12;

/// One waypoint. Its dimension is its context's; its group
/// may be empty (no group).
#[derive(Clone, PartialEq, Debug)]
struct Waypoint {
    name: String,
    x: i32,
    y: i32,
    z: i32,
    color: usize,
    group: String,
    hidden: bool,
    death: bool,
}

/// A context's whole waypoint state: the waypoints, the
/// group names (empty groups included), and the groups
/// hidden from the world, the minimap and the map.
#[derive(Clone, Default)]
struct WpState {
    waypoints: Vec<Waypoint>,
    groups: Vec<String>,
    hidden_groups: HashSet<String>,
}

/// A name storage can carry inside a line: tabs and
/// newlines become spaces (the format's separators); export
/// and share lines also bar ':' and ';'.
fn clean_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c == '\t' || c == '\n' || c == '\r' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// The waypoint blob: `#group` and `#gh` header lines, then
/// one tab-separated waypoint a line.
fn encode_waypoints(st: &WpState) -> Vec<u8> {
    let mut out = String::new();
    for g in &st.groups {
        out.push_str(&format!("#group\t{}\n", clean_name(g)));
    }
    for g in &st.hidden_groups {
        out.push_str(&format!("#gh\t{}\n", clean_name(g)));
    }
    for w in &st.waypoints {
        let mut flags = String::new();
        if w.hidden {
            flags.push('h');
        }
        if w.death {
            flags.push('d');
        }
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            clean_name(&w.name),
            w.x,
            w.y,
            w.z,
            w.color,
            clean_name(&w.group),
            flags
        ));
    }
    out.into_bytes()
}

fn decode_waypoints(data: &[u8]) -> WpState {
    let mut st = WpState::default();
    let Ok(text) = std::str::from_utf8(data) else {
        return st;
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() == 2 && f[0] == "#group" {
            if !f[1].is_empty() && !st.groups.iter().any(|g| g == f[1]) {
                st.groups.push(f[1].to_string());
            }
            continue;
        }
        if f.len() == 2 && f[0] == "#gh" {
            st.hidden_groups.insert(f[1].to_string());
            continue;
        }
        if f.len() != 7 {
            continue;
        }
        let (Ok(x), Ok(y), Ok(z), Ok(color)) = (
            f[1].parse::<i32>(),
            f[2].parse::<i32>(),
            f[3].parse::<i32>(),
            f[4].parse::<usize>(),
        ) else {
            continue;
        };
        let group = f[5].to_string();
        if !group.is_empty() && !st.groups.contains(&group) {
            st.groups.push(group.clone());
        }
        st.waypoints.push(Waypoint {
            name: f[0].to_string(),
            x,
            y,
            z,
            color: color.min(15),
            group,
            hidden: f[6].contains('h'),
            death: f[6].contains('d'),
        });
    }
    st
}

impl WpState {
    /// Is this waypoint shown in the world, on the minimap
    /// and on the map? Its own hide, or its group's.
    fn shown(&self, w: &Waypoint) -> bool {
        !w.hidden && (w.group.is_empty() || !self.hidden_groups.contains(&w.group))
    }
}

/// The 3D distance from the player to a waypoint, in blocks.
fn distance(p: (f64, f64, f64), w: &Waypoint) -> f64 {
    let dx = p.0 - (f64::from(w.x) + 0.5);
    let dy = p.1 - f64::from(w.y);
    let dz = p.2 - (f64::from(w.z) + 0.5);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// The share line a waypoint travels as, in chat: the
/// original sends a chat line with the waypoint, and a
/// client running the port reads it back out of the line.
fn share_line(w: &Waypoint, dimension: &str) -> String {
    format!(
        "worldmap-waypoint:{}:{}:{}:{}:{}:{}",
        clean_name(&w.name).replace(':', " "),
        w.x,
        w.y,
        w.z,
        w.color,
        dimension
    )
}

/// Reads a shared waypoint and its dimension out of a chat
/// line, if it carries one.
fn parse_share(line: &str) -> Option<(Waypoint, String)> {
    let at = line.find("worldmap-waypoint:")? + "worldmap-waypoint:".len();
    let f: Vec<&str> = line[at..].split(':').collect();
    if f.len() < 6 {
        return None;
    }
    Some((
        Waypoint {
            name: f[0].to_string(),
            x: f[1].parse().ok()?,
            y: f[2].parse().ok()?,
            z: f[3].parse().ok()?,
            color: f[4].parse::<usize>().ok()?.min(15),
            group: String::new(),
            hidden: false,
            death: false,
        },
        f[5..].join(":"),
    ))
}

/// One waypoint as an export line, in the shape Xaero's
/// waypoint files use (the original reads that format on
/// import, and the port's export writes it, so the two
/// exchange).
fn export_line(w: &Waypoint, dimension: &str) -> String {
    let dim = dimension.rsplit(':').next().unwrap_or(dimension);
    format!(
        "waypoint:{}:{}:{}:{}:{}:{}:0:Internal:{}",
        clean_name(&w.name).replace(':', " "),
        w.x,
        w.y,
        w.z,
        w.color,
        w.hidden,
        dim
    )
}

/// Reads an imported waypoint line: Xaero's shape
/// (`waypoint:name:x:y:z:color:...`, as the original's
/// import also reads), for the current dimension.
fn parse_import(line: &str) -> Option<Waypoint> {
    let rest = line.trim().strip_prefix("waypoint:")?;
    let f: Vec<&str> = rest.split(':').collect();
    if f.len() < 5 {
        return None;
    }
    Some(Waypoint {
        name: f[0].to_string(),
        x: f[1].parse().ok()?,
        y: f[2].parse().ok()?,
        z: f[3].parse().ok()?,
        color: f[4].parse::<usize>().ok()?.min(15),
        group: String::new(),
        hidden: f.get(5).map(|s| *s == "true").unwrap_or(false),
        death: false,
    })
}

// ----------------------------------------------------------------
// Settings: one storage blob of `key=value` lines (the tile
// regions need the storage key budget).

#[derive(Clone)]
struct Settings {
    minimap: bool,
    corner: usize,
    size: usize,
    rotate: bool,
    zoom: usize,
    mode: usize,
    cave_full: bool,
    top_y: usize,
    grid: bool,
    labels: bool,
    death_waypoint: bool,
}

const CORNERS: [MapCorner; 4] = [
    MapCorner::TopRight,
    MapCorner::TopLeft,
    MapCorner::BottomRight,
    MapCorner::BottomLeft,
];
const CORNER_NAMES: [&str; 4] = ["Top Right", "Top Left", "Bottom Right", "Bottom Left"];
const SIZES: [u32; 4] = [100, 128, 160, 200];
const ZOOMS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];
const MODES: [&str; 3] = ["Auto", "Surface", "Cave"];
const TOP_YS: [i32; 9] = [-64, 0, 32, 64, 96, 128, 192, 256, 320];

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            minimap: true,
            corner: 0,
            size: 1,
            rotate: true,
            zoom: 1,
            mode: 0,
            cave_full: false,
            top_y: 3,
            grid: false,
            labels: true,
            death_waypoint: true,
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
            let b = v == "1";
            let n = v.parse::<usize>().unwrap_or(0);
            match k {
                "minimap" => s.minimap = b,
                "corner" => s.corner = n.min(3),
                "size" => s.size = n.min(3),
                "rotate" => s.rotate = b,
                "zoom" => s.zoom = n.min(3),
                "mode" => s.mode = n.min(2),
                "cave_full" => s.cave_full = b,
                "top_y" => s.top_y = n.min(8),
                "grid" => s.grid = b,
                "labels" => s.labels = b,
                "death_waypoint" => s.death_waypoint = b,
                _ => {}
            }
        }
        s
    }
    fn encode(&self) -> Vec<u8> {
        format!(
            "minimap={}\ncorner={}\nsize={}\nrotate={}\nzoom={}\nmode={}\ncave_full={}\ntop_y={}\ngrid={}\nlabels={}\ndeath_waypoint={}\n",
            self.minimap as u8,
            self.corner,
            self.size,
            self.rotate as u8,
            self.zoom,
            self.mode,
            self.cave_full as u8,
            self.top_y,
            self.grid as u8,
            self.labels as u8,
            self.death_waypoint as u8,
        )
        .into_bytes()
    }
    fn display(&self) -> Display {
        Display {
            mode: self.mode as u8,
            full: self.cave_full,
            top_y: TOP_YS[self.top_y],
        }
    }
}

// ----------------------------------------------------------------
// Composition: a palette-indexed picture of the store.

/// A palette under construction: index 0 is always clear.
struct Palette {
    colors: Vec<u32>,
    index: HashMap<u32, u8>,
}

impl Palette {
    fn new() -> Palette {
        let mut p = Palette {
            colors: vec![0],
            index: HashMap::new(),
        };
        p.index.insert(0, 0);
        p
    }
    fn get(&mut self, color: u32) -> u8 {
        if let Some(&i) = self.index.get(&color) {
            return i;
        }
        if self.colors.len() >= 256 {
            return 0;
        }
        let i = self.colors.len() as u8;
        self.colors.push(color);
        self.index.insert(color, i);
        i
    }
}

/// Composes a picture of `w` x `h` pixels centred on the
/// world block (`center_x`, `center_z`) at
/// `blocks_per_pixel`. With `rotate` (the minimap), the view
/// turns so the player's facing is up. `dots` are (world x,
/// world z, colour, is-death) marks drawn over the terrain.
#[allow(clippy::too_many_arguments)]
fn compose(
    store: &TileStore,
    center_x: f64,
    center_z: f64,
    bpp: f32,
    w: u32,
    h: u32,
    display: Display,
    feed_cave: bool,
    head_y: i32,
    day: f32,
    rotate: Option<f32>,
    grid: bool,
    dots: &[(f64, f64, u32, bool)],
) -> MapImage {
    let mut palette = Palette::new();
    let grid_i = palette.get(0x6600_0000);
    let dot_idx: Vec<u8> = dots.iter().map(|d| palette.get(d.2)).collect();
    let bpp = f64::from(bpp);
    let (hw, hh) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
    let (r, f) = match rotate {
        Some(yaw) => {
            let (s, c) = f64::from(yaw).to_radians().sin_cos();
            ((-c, -s), (-s, c))
        }
        None => ((1.0, 0.0), (0.0, -1.0)),
    };
    let mut pixels = vec![0u8; (w * h) as usize];
    for j in 0..h {
        for i in 0..w {
            let (u, v) = (f64::from(i) + 0.5 - hw, f64::from(j) + 0.5 - hh);
            let wx = center_x + bpp * (u * r.0 - v * f.0);
            let wz = center_z + bpp * (u * r.1 - v * f.1);
            let (bx, bz) = (wx.floor() as i32, wz.floor() as i32);
            let mut idx = match store.sample(bx, bz, display, feed_cave, head_y, day) {
                Some(argb) => palette.get(argb),
                None => palette.get(UNKNOWN),
            };
            if grid && bpp <= 2.0 && ((bx & 15) == 0 || (bz & 15) == 0) {
                idx = grid_i;
            }
            pixels[(j * w + i) as usize] = idx;
        }
    }
    for (n, &(dx, dz, _, death)) in dots.iter().enumerate() {
        let rel_x = dx - center_x;
        let rel_z = dz - center_z;
        let u = (rel_x * r.0 + rel_z * r.1) / bpp;
        let v = -(rel_x * f.0 + rel_z * f.1) / bpp;
        let (px, py) = ((u + hw) as i32, (v + hh) as i32);
        let idx = dot_idx[n];
        if death {
            for (ox, oy) in [(-2, -2), (-1, -1), (0, 0), (1, 1), (2, 2), (-2, 2), (-1, 1), (1, -1), (2, -2)] {
                let (x, y) = (px + ox, py + oy);
                if x >= 0 && y >= 0 && x < w as i32 && y < h as i32 {
                    pixels[(y * w as i32 + x) as usize] = idx;
                }
            }
        } else {
            for oy in -1..=1 {
                for ox in -1..=1 {
                    let (x, y) = (px + ox, py + oy);
                    if x >= 0 && y >= 0 && x < w as i32 && y < h as i32 {
                        pixels[(y * w as i32 + x) as usize] = idx;
                    }
                }
            }
        }
    }
    MapImage {
        center_x: center_x as i32,
        center_z: center_z as i32,
        blocks_per_pixel: bpp as f32,
        width: w,
        height: h,
        palette: palette.colors,
        pixels,
    }
}

/// Sets one pixel of a composed image by offset from its
/// centre, adding the colour to the palette while there is
/// room.
fn poke(image: &mut MapImage, dx: i32, dy: i32, argb: u32) {
    let (cx, cy) = (image.width as i32 / 2, image.height as i32 / 2);
    let (x, y) = (cx + dx, cy + dy);
    if x < 0 || y < 0 || x >= image.width as i32 || y >= image.height as i32 {
        return;
    }
    let idx = match image.palette.iter().position(|c| *c == argb) {
        Some(i) => i as u8,
        None => {
            if image.palette.len() >= 256 {
                return;
            }
            image.palette.push(argb);
            (image.palette.len() - 1) as u8
        }
    };
    image.pixels[(y * image.width as i32 + x) as usize] = idx;
}

/// The compass letters for the minimap's rim, as 3x5 pixel
/// glyphs — the mod's own drawing in its composed picture
/// (the image surface has no text; the letters are pixels).
fn glyph(letter: char) -> [[bool; 3]; 5] {
    let rows: [[bool; 3]; 5] = match letter {
        'N' => [
            [true, false, true],
            [true, true, true],
            [true, true, true],
            [true, false, true],
            [true, false, true],
        ],
        'E' => [
            [true, true, true],
            [true, false, false],
            [true, true, false],
            [true, false, false],
            [true, true, true],
        ],
        'S' => [
            [false, true, true],
            [true, false, false],
            [false, true, false],
            [false, false, true],
            [true, true, false],
        ],
        'W' => [
            [true, false, true],
            [true, false, true],
            [true, true, true],
            [true, true, true],
            [true, false, true],
        ],
        _ => [[false; 3]; 5],
    };
    rows
}

/// Draws the four compass letters on a minimap image's rim,
/// at the screen directions the world's cardinals fall in
/// this view (rotated views turn the letters with the map,
/// as the original's turning minimap does).
fn draw_letters(image: &mut MapImage, rotate: Option<f32>) {
    let half = image.width as i32 / 2;
    let rim = half - 7;
    // World directions of the cardinals, as (x, z).
    let cardinals = [('N', 0.0, -1.0), ('E', 1.0, 0.0), ('S', 0.0, 1.0), ('W', -1.0, 0.0)];
    let (r, f) = match rotate {
        Some(yaw) => {
            let (s, c) = f64::from(yaw).to_radians().sin_cos();
            ((-c, -s), (-s, c))
        }
        None => ((1.0, 0.0), (0.0, -1.0)),
    };
    for (letter, dx, dz) in cardinals {
        // Screen offset of the world direction (unit bpp).
        let u = dx * r.0 + dz * r.1;
        let v = -(dx * f.0 + dz * f.1);
        let (ox, oy) = ((u * f64::from(rim)) as i32, (v * f64::from(rim)) as i32);
        let g = glyph(letter);
        for (row, line) in g.iter().enumerate() {
            for (col, on) in line.iter().enumerate() {
                if *on {
                    poke(image, ox + col as i32 - 1, oy + row as i32 - 2, 0xFFFF_FFFF);
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// The mod.

/// Which context's data is loaded: a world (or server) and
/// a dimension, as `map.context` names them.
#[derive(Clone)]
struct Ctx {
    hash: String,
    world: String,
    dimension: String,
}

/// A stored context being viewed on the map (the dimensions
/// button): its tiles and waypoints, loaded read-mostly —
/// the feed keeps writing to the live context, never here.
struct ViewState {
    ctx: Ctx,
    store: TileStore,
    wp: WpState,
    wp_dirty: bool,
}

/// Which of the mod's own screens is open.
#[derive(Clone, Copy, PartialEq)]
enum OpenScreen {
    None,
    Settings,
    List,
    Editor,
    Menu,
}

/// A waypoint being made or edited.
struct Editor {
    /// The waypoint's index in the active state, when
    /// editing an existing one.
    index: Option<usize>,
    name: String,
    x: i32,
    y: i32,
    z: i32,
    color: usize,
    group: String,
    hidden: bool,
    step: usize,
    /// The edit lands in the viewed context, not the live
    /// one (the map was showing another dimension).
    viewing: bool,
}

const STEPS: [i32; 3] = [1, 10, 100];

/// The widget ids of the mod's screens.
const SETTING_BUTTONS: std::ops::Range<u32> = 0..11;
const LIST_WIDGET: u32 = 20;
const LIST_FILTER_BUTTON: u32 = 21;
const LIST_NEW_BUTTON: u32 = 22;
const LIST_EDIT_BUTTON: u32 = 23;
const LIST_TP_BUTTON: u32 = 24;
const LIST_SHARE_BUTTON: u32 = 25;
const LIST_DELETE_BUTTON: u32 = 26;
const LIST_GROUP_BASE: u32 = 40;
const EDITOR_STEP_BUTTON: u32 = 60;
const EDITOR_XM: u32 = 61;
const EDITOR_XP: u32 = 62;
const EDITOR_YM: u32 = 63;
const EDITOR_YP: u32 = 64;
const EDITOR_ZM: u32 = 65;
const EDITOR_ZP: u32 = 66;
const EDITOR_COLOR_BUTTON: u32 = 67;
const EDITOR_GROUP_BUTTON: u32 = 68;
const EDITOR_HIDDEN_BUTTON: u32 = 69;
const EDITOR_SAVE_BUTTON: u32 = 70;
const EDITOR_CANCEL_BUTTON: u32 = 71;
const MENU_ADD_BUTTON: u32 = 80;
const MENU_TP_BUTTON: u32 = 81;
const MENU_LIST_BUTTON: u32 = 82;
const MENU_DIM_BUTTON: u32 = 83;

/// Key names, as registered.
const MAP_KEY: &str = "map";
const MINIMAP_KEY: &str = "minimap";
const ZOOM_KEY: &str = "zoom";

/// How far around the player the tile feed sweeps, in
/// chunks.
const FEED_RADIUS: i32 = 10;
/// The map screen's picture size.
const MAP_W: u32 = 1024;
const MAP_H: u32 = 1024;

struct WorldMap {
    settings: Settings,
    ctx: Option<Ctx>,
    store: TileStore,
    known: HashMap<(i32, i32), u64>,
    verified: HashSet<(i32, i32)>,
    fetch: VecDeque<(i32, i32)>,
    queued: HashSet<(i32, i32)>,
    regions: HashSet<(i32, i32)>,
    dirty: HashSet<(i32, i32)>,
    store_full: bool,
    feed_cave: bool,
    wp: WpState,
    wp_dirty: bool,
    viewing: Option<ViewState>,
    map_open: bool,
    view: (f64, f64, f32),
    map_image: (f64, f64, f32),
    map_gen: u64,
    shown_gen: u64,
    open_screen: OpenScreen,
    selected: Option<usize>,
    /// The list screen's group filter: None shows all.
    group_filter: Option<String>,
    /// The list screen shows the viewed context, not the
    /// live one.
    list_viewing: bool,
    editor: Option<Editor>,
    /// Where the open editor was opened from.
    editor_from: EditorFrom,
    /// Where the map's right-click fell: (x, y, z).
    menu_at: Option<(i32, i32, i32)>,
    /// Closing the open screen returns to the map (menus
    /// and the list opened from one are the map's children).
    return_to_map: bool,
    pending_share: Option<(Waypoint, String)>,
    last_labels: String,
}

/// Loads a context's tiles and index from storage into a
/// fresh store, with the chunk versions it carries.
fn load_store(hash: &str) -> (TileStore, HashMap<(i32, i32), u64>, HashSet<(i32, i32)>) {
    let mut store = TileStore::default();
    let mut known = HashMap::new();
    let mut regions = HashSet::new();
    if let Some(data) = storage::get(&index_key(hash)) {
        if let Ok(text) = std::str::from_utf8(&data) {
            for pair in text.trim().split(';') {
                let Some((a, b)) = pair.split_once(',') else {
                    continue;
                };
                if let (Ok(rx), Ok(rz)) = (a.parse::<i32>(), b.parse::<i32>()) {
                    regions.insert((rx, rz));
                }
            }
        }
    }
    for (rx, rz) in &regions {
        if let Some(data) = storage::get(&region_key(hash, *rx, *rz)) {
            for (chunk, tile) in decode_region(*rx, *rz, &data) {
                known.insert(chunk, tile.version);
                store.put(chunk, tile);
            }
        }
    }
    (store, known, regions)
}

fn load_wp(hash: &str) -> WpState {
    storage::get(&waypoints_key(hash))
        .map(|d| decode_waypoints(&d))
        .unwrap_or_default()
}

impl WorldMap {
    fn save_settings(&self) {
        let _ = storage::set("settings", &self.settings.encode());
    }

    /// Registers a context in the contexts list (the
    /// dimensions button cycles the ones of this world).
    fn note_context(&self, ctx: &Ctx) {
        let mut entries: Vec<(String, String, String)> = Vec::new();
        if let Some(data) = storage::get("contexts") {
            if let Ok(text) = std::str::from_utf8(&data) {
                for line in text.lines() {
                    let f: Vec<&str> = line.split('\t').collect();
                    if f.len() == 3 {
                        entries.push((f[0].to_string(), f[1].to_string(), f[2].to_string()));
                    }
                }
            }
        }
        if entries.iter().any(|e| e.0 == ctx.hash) {
            return;
        }
        entries.push((ctx.hash.clone(), ctx.world.clone(), ctx.dimension.clone()));
        let text = entries
            .iter()
            .map(|e| format!("{}\t{}\t{}", e.0, e.1, e.2))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = storage::set("contexts", text.as_bytes());
    }

    /// The dimensions of this world the mod holds data for,
    /// in registry order (the live one included).
    fn world_dimensions(&self, world: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(data) = storage::get("contexts") {
            if let Ok(text) = std::str::from_utf8(&data) {
                for line in text.lines() {
                    let f: Vec<&str> = line.split('\t').collect();
                    if f.len() == 3 && f[1] == world && !out.contains(&f[2].to_string()) {
                        out.push(f[2].to_string());
                    }
                }
            }
        }
        out
    }

    fn load_context(&mut self, ctx: Ctx) {
        self.note_context(&ctx);
        let (store, known, regions) = load_store(&ctx.hash);
        self.store = store;
        self.known = known;
        self.regions = regions;
        self.verified.clear();
        self.fetch.clear();
        self.queued.clear();
        self.dirty.clear();
        self.store_full = false;
        self.feed_cave = false;
        self.wp = load_wp(&ctx.hash);
        self.wp_dirty = false;
        self.viewing = None;
        self.ctx = Some(ctx);
    }

    fn save_context(&mut self) {
        let Some(ctx) = &self.ctx else { return };
        let hash = ctx.hash.clone();
        let dirty: Vec<(i32, i32)> = self.dirty.iter().copied().collect();
        for r in dirty {
            self.save_region(&hash, r);
        }
        self.save_index(&hash);
        if self.wp_dirty {
            let _ = storage::set(&waypoints_key(&hash), &encode_waypoints(&self.wp));
            self.wp_dirty = false;
        }
        self.save_view();
    }

    fn save_index(&self, hash: &str) {
        let mut list: Vec<(i32, i32)> = self.regions.iter().copied().collect();
        list.sort();
        let text = list
            .iter()
            .map(|(a, b)| format!("{a},{b}"))
            .collect::<Vec<_>>()
            .join(";");
        let _ = storage::set(&index_key(hash), text.as_bytes());
    }

    /// Writes one region from the live store; on a quota
    /// refusal the farthest stored region is evicted and
    /// the write retried (the platform's storage is capped
    /// where the original writes files freely).
    fn save_region(&mut self, hash: &str, region: (i32, i32)) {
        let (rx, rz) = region;
        let mut tiles: Vec<(u8, &Tile)> = Vec::new();
        for (chunk, tile) in &self.store.surface {
            push_region_tile(&mut tiles, region, *chunk, tile);
        }
        for (chunk, cuts) in &self.store.cave {
            for tile in cuts {
                push_region_tile(&mut tiles, region, *chunk, tile);
            }
        }
        let blob = encode_region(&tiles);
        let key = region_key(hash, rx, rz);
        if storage::set(&key, &blob).is_ok() {
            self.regions.insert(region);
            self.dirty.remove(&region);
            return;
        }
        for _ in 0..2 {
            let Some(&far) = self
                .regions
                .iter()
                .filter(|r| **r != region)
                .max_by_key(|r| (r.0 - rx).abs() + (r.1 - rz).abs())
            else {
                break;
            };
            storage::delete(&region_key(hash, far.0, far.1));
            self.regions.remove(&far);
            self.dirty.remove(&far);
            self.evict_memory(far);
            if storage::set(&key, &blob).is_ok() {
                self.regions.insert(region);
                self.dirty.remove(&region);
                return;
            }
        }
        self.store_full = true;
    }

    fn evict_memory(&mut self, region: (i32, i32)) {
        let inside = |c: &(i32, i32)| (c.0.div_euclid(REGION), c.1.div_euclid(REGION)) == region;
        self.store.surface.retain(|c, _| !inside(c));
        self.store.cave.retain(|c, _| !inside(c));
        self.known.retain(|c, _| !inside(c));
        self.verified.retain(|c| !inside(c));
    }

    // ---- dimension viewing ----

    /// The dimension the map shows: the viewed one, or the
    /// live one.
    fn view_dimension(&self) -> Option<String> {
        match &self.viewing {
            Some(v) => Some(v.ctx.dimension.clone()),
            None => self.ctx.as_ref().map(|c| c.dimension.clone()),
        }
    }

    /// The store and waypoints a map composition reads.
    fn view_data(&self) -> Option<(&TileStore, &WpState)> {
        match &self.viewing {
            Some(v) => Some((&v.store, &v.wp)),
            None => self.ctx.as_ref().map(|_| (&self.store, &self.wp)),
        }
    }

    fn save_view(&mut self) {
        if let Some(v) = &self.viewing {
            if v.wp_dirty {
                let _ = storage::set(&waypoints_key(&v.ctx.hash), &encode_waypoints(&v.wp));
            }
        }
        if let Some(v) = &mut self.viewing {
            v.wp_dirty = false;
        }
    }

    /// Switches the map's viewed dimension (the dimensions
    /// button): the live dimension closes the view, another
    /// loads its stored context read-mostly.
    fn set_view_dimension(&mut self, dimension: &str) {
        let Some(ctx) = &self.ctx else { return };
        let world = ctx.world.clone();
        let live = ctx.dimension.clone();
        self.save_view();
        if dimension == live {
            self.viewing = None;
            return;
        }
        let hash = context_hash(&world, dimension);
        let (store, _known, _regions) = load_store(&hash);
        let wp = load_wp(&hash);
        self.viewing = Some(ViewState {
            ctx: Ctx {
                hash,
                world,
                dimension: dimension.to_string(),
            },
            store,
            wp,
            wp_dirty: false,
        });
    }

    // ---- waypoints in the active (live or viewed) state ----

    /// The waypoint state an edit lands in.
    fn active_wp_mut(&mut self, viewing: bool) -> &mut WpState {
        if viewing {
            &mut self.viewing.as_mut().expect("viewing state").wp
        } else {
            &mut self.wp
        }
    }

    fn mark_wp_dirty(&mut self, viewing: bool) {
        if viewing {
            if let Some(v) = &mut self.viewing {
                v.wp_dirty = true;
            }
        } else {
            self.wp_dirty = true;
        }
    }

    /// A death waypoint in the live context.
    fn add_death_waypoint(&mut self, x: i32, y: i32, z: i32) {
        if self
            .wp
            .waypoints
            .iter()
            .any(|w| w.death && w.x == x && w.y == y && w.z == z)
        {
            return;
        }
        let n = self.wp.waypoints.iter().filter(|w| w.death).count();
        let name = if n == 0 {
            "Death Point".to_string()
        } else {
            format!("Death Point ({})", n + 1)
        };
        self.wp.waypoints.push(Waypoint {
            name,
            x,
            y,
            z,
            color: RED,
            group: String::new(),
            hidden: false,
            death: true,
        });
        self.wp_dirty = true;
    }

    // ---- the tile feed (the live context only) ----

    fn sweep(&mut self, pc: (i32, i32)) {
        let mut chunks: Vec<(i32, i32)> = Vec::new();
        for dz in -FEED_RADIUS..=FEED_RADIUS {
            for dx in -FEED_RADIUS..=FEED_RADIUS {
                chunks.push((pc.0 + dx, pc.1 + dz));
            }
        }
        chunks.sort_by_key(|c| (c.0 - pc.0).abs().max((c.1 - pc.1).abs()));
        for batch in chunks.chunks(256) {
            let ask: Vec<ChunkPos> = batch.iter().map(|c| ChunkPos { x: c.0, z: c.1 }).collect();
            for v in map::tile_versions(&ask) {
                let chunk = (v.chunk_x, v.chunk_z);
                let stale = !self.verified.contains(&chunk)
                    || self.known.get(&chunk) != Some(&v.version);
                if stale && !self.queued.contains(&chunk) {
                    self.fetch.push_back(chunk);
                    self.queued.insert(chunk);
                }
            }
        }
    }

    fn drain_fetch(&mut self, pc: (i32, i32)) {
        for _ in 0..3 {
            if self.fetch.is_empty() {
                return;
            }
            let batch: Vec<ChunkPos> = self
                .fetch
                .drain(..self.fetch.len().min(64))
                .map(|c| {
                    self.queued.remove(&c);
                    ChunkPos { x: c.0, z: c.1 }
                })
                .collect();
            for t in map::tiles(&batch) {
                let chunk = (t.chunk_x, t.chunk_z);
                self.verified.insert(chunk);
                self.known.insert(chunk, t.version);
                if chunk == pc {
                    self.feed_cave = t.layer == MapLayer::Cave;
                }
                if let Some(tile) = Tile::from_sdk(&t) {
                    let (vx, vz, vb) = self.view;
                    let extent = vb as f64 * MAP_W as f64 / 2.0;
                    if (f64::from(chunk.0 * 16) - vx).abs() < extent + 16.0
                        && (f64::from(chunk.1 * 16) - vz).abs() < extent + 16.0
                    {
                        self.map_gen += 1;
                    }
                    self.store.put(chunk, tile);
                    self.dirty
                        .insert((chunk.0.div_euclid(REGION), chunk.1.div_euclid(REGION)));
                }
            }
        }
    }

    /// The store's height of a column, for placing a
    /// waypoint made from a map click.
    fn ground_y(store: &TileStore, x: i32, z: i32) -> Option<i32> {
        let chunk = (x >> 4, z >> 4);
        if let Some(t) = store.surface.get(&chunk) {
            return Some(i32::from(t.column(x & 15, z & 15).1));
        }
        store
            .cave
            .get(&chunk)
            .and_then(|cuts| cuts.first())
            .map(|t| i32::from(t.column(x & 15, z & 15).1))
    }
}

// ----------------------------------------------------------------
// Composition drivers and screens.

fn button(id: u32, label: String) -> Widget {
    Widget::Button(ButtonWidget { id, label })
}

fn on_off(b: bool) -> &'static str {
    if b { "On" } else { "Off" }
}

fn short_dim(dimension: &str) -> &str {
    dimension.rsplit(':').next().unwrap_or(dimension)
}

/// The settings page and screen: one cycling button per
/// setting.
fn settings_widgets(s: &Settings) -> Vec<Widget> {
    vec![
        button(0, format!("Minimap: {}", on_off(s.minimap))),
        button(1, format!("Minimap Corner: {}", CORNER_NAMES[s.corner])),
        button(2, format!("Minimap Size: {}", SIZES[s.size])),
        button(3, format!("Minimap Rotation: {}", on_off(s.rotate))),
        button(
            4,
            format!("Minimap Scale: {}", ["2x", "1x", "0.5x", "0.25x"][s.zoom]),
        ),
        button(5, format!("Map Mode: {}", MODES[s.mode])),
        button(
            6,
            format!(
                "Cave Style: {}",
                if s.cave_full { "Full" } else { "Layered" }
            ),
        ),
        button(7, format!("Cave Top Y: {}", TOP_YS[s.top_y])),
        button(8, format!("Chunk Grid: {}", on_off(s.grid))),
        button(9, format!("Waypoint Labels: {}", on_off(s.labels))),
        button(10, format!("Death Waypoint: {}", on_off(s.death_waypoint))),
    ]
}

/// Where an editor was opened from.
#[derive(Clone, Copy, PartialEq)]
enum EditorFrom {
    Map,
    List,
}

impl WorldMap {
    /// The daylight factor for the next composition.
    fn day_factor(&self, dimension: &str) -> f32 {
        let cave_display = self.settings.mode == 2 || (self.settings.mode == 0 && self.feed_cave);
        if dimension != "minecraft:overworld" || cave_display {
            return 1.0;
        }
        match game::get_clock() {
            Some(clock) => daylight(clock.day_time),
            None => 1.0,
        }
    }

    /// Composes and pushes the minimap (the live context's:
    /// waypoints as dots, the compass letters on the rim).
    fn push_minimap(&mut self, px: f64, pz: f64, yaw: f32, player_y: i32, dimension: &str) {
        if !self.settings.minimap {
            map::set_minimap_image(None);
            return;
        }
        let size = SIZES[self.settings.size];
        let bpp = ZOOMS[self.settings.zoom];
        let dots: Vec<(f64, f64, u32, bool)> = self
            .wp
            .waypoints
            .iter()
            .filter(|w| self.wp.shown(w))
            .map(|w| {
                (
                    f64::from(w.x) + 0.5,
                    f64::from(w.z) + 0.5,
                    WP_COLORS[w.color].1,
                    w.death,
                )
            })
            .collect();
        let rotate = if self.settings.rotate {
            Some(yaw)
        } else {
            None
        };
        let mut image = compose(
            &self.store,
            px,
            pz,
            bpp,
            size,
            size,
            self.settings.display(),
            self.feed_cave,
            player_y + 1,
            self.day_factor(dimension),
            rotate,
            false,
            &dots,
        );
        draw_letters(&mut image, rotate);
        for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
            poke(&mut image, dx, dy, 0xFF00_0000);
        }
        poke(&mut image, 0, 0, 0xFFFF_FFFF);
        if self.settings.rotate {
            poke(&mut image, 0, -1, 0xFFFF_FFFF);
        } else {
            let head = (f64::from(yaw) + 180.0).to_radians();
            let (nx, ny) = (head.sin().round() as i32, (-head.cos()).round() as i32);
            poke(&mut image, nx, ny, 0xFFFF_FFFF);
        }
        map::set_minimap_image(Some(&horizon_mod_sdk::MinimapImage {
            corner: CORNERS[self.settings.corner],
            size: size as f32,
            round: false,
            image,
        }));
    }

    /// Composes and pushes the map screen at the current
    /// view, from the viewed (or live) context's data, with
    /// its shown waypoints as the marks.
    fn push_map_screen(&mut self, player: (f64, f64, f64)) {
        let Some((store, wp)) = self.view_data() else {
            return;
        };
        let (cx, cz, bpp) = self.view;
        let dimension = self.view_dimension().unwrap_or_default();
        let image = compose(
            store,
            cx,
            cz,
            bpp,
            MAP_W,
            MAP_H,
            self.settings.display(),
            self.feed_cave,
            player.1 as i32 + 1,
            self.day_factor(&dimension),
            None,
            self.settings.grid,
            &[],
        );
        let mut marks: Vec<&Waypoint> =
            wp.waypoints.iter().filter(|w| wp.shown(w)).collect();
        marks.sort_by_key(|w| {
            let dx = f64::from(w.x) - cx;
            let dz = f64::from(w.z) - cz;
            (dx * dx + dz * dz) as i64
        });
        let markers: Vec<MapMarker> = marks
            .iter()
            .take(128)
            .map(|w| MapMarker {
                x: w.x,
                z: w.z,
                color: WP_COLORS[w.color].1,
                label: w.name.clone(),
            })
            .collect();
        map::set_map_screen(Some(&MapScreen { image, markers }));
        self.map_image = self.view;
        self.shown_gen = self.map_gen;
    }

    /// Opens the map screen centred on the player, on the
    /// live dimension (closing any dimension view).
    fn open_map(&mut self, player: (f64, f64, f64)) {
        if self.viewing.is_some() {
            self.save_view();
            self.viewing = None;
        }
        self.view = (player.0, player.2, 1.0);
        self.push_map_screen(player);
        self.map_open = true;
    }

    /// The store an editor or menu acts on: the viewed one
    /// while viewing, else the live one.
    fn active_store(&self) -> &TileStore {
        match &self.viewing {
            Some(v) => &v.store,
            None => &self.store,
        }
    }

    // ---- screens ----

    fn open_settings_screen(&mut self) {
        self.open_screen = OpenScreen::Settings;
        ui::open_screen(&ui::Screen {
            title: "World Map".to_string(),
            widgets: settings_widgets(&self.settings),
        });
    }

    /// The waypoint indices the list shows, in storage
    /// order, under the group filter.
    fn list_indices(&self, st: &WpState) -> Vec<usize> {
        st.waypoints
            .iter()
            .enumerate()
            .filter(|(_, w)| match &self.group_filter {
                None => true,
                Some(g) => &w.group == g,
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn open_list(&mut self, viewing: bool) {
        let st: &WpState = if viewing {
            match &self.viewing {
                Some(v) => &v.wp,
                None => &self.wp,
            }
        } else {
            &self.wp
        };
        let dim = if viewing {
            self.view_dimension().unwrap_or_default()
        } else {
            self.ctx
                .as_ref()
                .map(|c| c.dimension.clone())
                .unwrap_or_default()
        };
        let player = game::get_player();
        let rows: Vec<String> = self
            .list_indices(st)
            .iter()
            .map(|&i| {
                let w = &st.waypoints[i];
                if viewing {
                    format!("{} ({}, {}, {})", w.name, w.x, w.y, w.z)
                } else if let Some(p) = &player {
                    format!("{} — {:.0}m", w.name, distance((p.x, p.y, p.z), w))
                } else {
                    format!("{} ({}, {}, {})", w.name, w.x, w.y, w.z)
                }
            })
            .collect();
        let mut widgets = vec![
            Widget::Text(TextWidget {
                id: 0,
                text: format!(
                    "{} — {} waypoints",
                    short_dim(&dim),
                    st.waypoints.len()
                ),
                color: 0,
            }),
            Widget::List(ListWidget {
                id: LIST_WIDGET,
                rows,
                selected: self.selected.map(|s| s as u32),
            }),
            button(
                LIST_FILTER_BUTTON,
                format!(
                    "Group: {}",
                    self.group_filter.as_deref().unwrap_or("All")
                ),
            ),
            button(LIST_NEW_BUTTON, "New Waypoint".to_string()),
            button(LIST_EDIT_BUTTON, "Edit".to_string()),
            button(LIST_TP_BUTTON, "Teleport".to_string()),
            button(LIST_SHARE_BUTTON, "Share".to_string()),
            button(LIST_DELETE_BUTTON, "Delete".to_string()),
        ];
        // The groups' headings: one button a group hides or
        // shows them all, as the original's list does.
        for (i, g) in st.groups.iter().take(10).enumerate() {
            widgets.push(button(
                LIST_GROUP_BASE + i as u32,
                format!(
                    "Group {g}: {}",
                    if st.hidden_groups.contains(g) {
                        "Hidden"
                    } else {
                        "Shown"
                    }
                ),
            ));
        }
        self.open_screen = OpenScreen::List;
        self.list_viewing = viewing;
        ui::open_screen(&ui::Screen {
            title: "Waypoints".to_string(),
            widgets,
        });
    }

    fn open_editor(
        &mut self,
        index: Option<usize>,
        name: String,
        x: i32,
        y: i32,
        z: i32,
        color: usize,
        group: String,
        hidden: bool,
        viewing: bool,
        from: EditorFrom,
    ) {
        self.editor = Some(Editor {
            index,
            name,
            x,
            y,
            z,
            color,
            group,
            hidden,
            step: 1,
            viewing,
        });
        self.editor_from = from;
        self.open_screen = OpenScreen::Editor;
        self.show_editor();
    }

    fn show_editor(&self) {
        let Some(e) = &self.editor else { return };
        ui::open_screen(&ui::Screen {
            title: if e.index.is_some() {
                "Edit Waypoint".to_string()
            } else {
                "New Waypoint".to_string()
            },
            widgets: vec![
                Widget::Text(TextWidget {
                    id: 0,
                    text: e.name.clone(),
                    color: 0,
                }),
                Widget::Text(TextWidget {
                    id: 0,
                    text: format!("{}, {}, {}", e.x, e.y, e.z),
                    color: 0,
                }),
                button(
                    EDITOR_STEP_BUTTON,
                    format!("Step: ±{}", STEPS[e.step]),
                ),
                button(EDITOR_XM, "X −".to_string()),
                button(EDITOR_XP, "X +".to_string()),
                button(EDITOR_YM, "Y −".to_string()),
                button(EDITOR_YP, "Y +".to_string()),
                button(EDITOR_ZM, "Z −".to_string()),
                button(EDITOR_ZP, "Z +".to_string()),
                button(
                    EDITOR_COLOR_BUTTON,
                    format!("Colour: {}", WP_COLORS[e.color].0),
                ),
                button(
                    EDITOR_GROUP_BUTTON,
                    format!(
                        "Group: {}",
                        if e.group.is_empty() {
                            "None"
                        } else {
                            &e.group
                        }
                    ),
                ),
                button(
                    EDITOR_HIDDEN_BUTTON,
                    format!("Hidden: {}", on_off(e.hidden)),
                ),
                button(EDITOR_SAVE_BUTTON, "Save".to_string()),
                button(EDITOR_CANCEL_BUTTON, "Cancel".to_string()),
            ],
        });
    }

    /// After the editor: back to the map or the list it came
    /// from.
    fn leave_editor(&mut self) {
        let from = self.editor_from;
        self.editor = None;
        self.open_screen = OpenScreen::None;
        match from {
            EditorFrom::Map => {
                if let Some(p) = game::get_player() {
                    self.push_map_screen((p.x, p.y, p.z));
                    self.map_open = true;
                }
            }
            EditorFrom::List => {
                let viewing = self.list_viewing;
                self.open_list(viewing);
            }
        }
    }

    /// The map's right-click menu: what the original puts at
    /// the pointer — make a waypoint here, teleport, the
    /// list, the dimensions.
    fn open_menu(&mut self, x: i32, y: i32, z: i32) {
        self.menu_at = Some((x, y, z));
        self.return_to_map = true;
        let dim = self.view_dimension().unwrap_or_default();
        let biome = world::get_biome(&dim, x, y + 1, z)
            .map(|b| format!(" — {}", b.rsplit(':').next().unwrap_or(&b)))
            .unwrap_or_default();
        self.open_screen = OpenScreen::Menu;
        ui::open_screen(&ui::Screen {
            title: format!("{x}, {z}"),
            widgets: vec![
                Widget::Text(TextWidget {
                    id: 0,
                    text: format!("{}{biome}", short_dim(&dim)),
                    color: 0,
                }),
                button(MENU_ADD_BUTTON, "Add Waypoint Here".to_string()),
                button(MENU_TP_BUTTON, "Teleport Here".to_string()),
                button(MENU_LIST_BUTTON, "Waypoint List".to_string()),
                button(
                    MENU_DIM_BUTTON,
                    format!("Dimension: {}", short_dim(&dim)),
                ),
            ],
        });
    }

    fn cycle_setting(&mut self, id: u32) {
        let s = &mut self.settings;
        match id {
            0 => s.minimap = !s.minimap,
            1 => s.corner = (s.corner + 1) % CORNERS.len(),
            2 => s.size = (s.size + 1) % SIZES.len(),
            3 => s.rotate = !s.rotate,
            4 => s.zoom = (s.zoom + 1) % ZOOMS.len(),
            5 => s.mode = (s.mode + 1) % MODES.len(),
            6 => s.cave_full = !s.cave_full,
            7 => s.top_y = (s.top_y + 1) % TOP_YS.len(),
            8 => s.grid = !s.grid,
            9 => s.labels = !s.labels,
            10 => s.death_waypoint = !s.death_waypoint,
            _ => return,
        }
        self.save_settings();
        ui::set_settings(&settings_widgets(&self.settings));
        if self.open_screen == OpenScreen::Settings {
            self.open_settings_screen();
        }
        if self.map_open {
            self.shown_gen = self.map_gen.wrapping_sub(1);
        }
    }
}

// ----------------------------------------------------------------
// The Mod impl.

impl WorldMap {
    /// A waypoint state by its viewing flag, for reading.
    fn wp_ref(&self, viewing: bool) -> &WpState {
        if viewing {
            match &self.viewing {
                Some(v) => &v.wp,
                None => &self.wp,
            }
        } else {
            &self.wp
        }
    }

    /// The waypoint the list's selection names.
    fn selected_waypoint(&self) -> Option<(usize, Waypoint)> {
        let row = self.selected?;
        let st = self.wp_ref(self.list_viewing);
        let idxs = self.list_indices(st);
        idxs.get(row).map(|&i| (i, st.waypoints[i].clone()))
    }

    /// The `/wp` commands, the original's suite: add, del,
    /// rename, list, clear, share — plus the port's
    /// stand-ins for the file and click flows (addshared,
    /// import, export, goto, group), each named in the
    /// README. All act on the live context.
    fn wp_command(&mut self, args: &str) -> Result<Option<String>, String> {
        let args = args.trim();
        if args.is_empty() {
            return Ok(Some(
                "World Map: /wp add [name] · /wp del <name> · /wp rename <old> | <new> · /wp list · /wp clear · /wp share <number> [player|all]"
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
                    return Err("World Map: not in a world".to_string());
                };
                if self.ctx.is_none() {
                    return Err("World Map: not in a world".to_string());
                }
                let name = if rest.is_empty() {
                    format!("Waypoint {}", self.wp.waypoints.len() + 1)
                } else {
                    clean_name(rest)
                };
                let w = Waypoint {
                    name: name.clone(),
                    x: p.x.floor() as i32,
                    y: p.y.floor() as i32,
                    z: p.z.floor() as i32,
                    color: self.wp.waypoints.len() % 16,
                    group: String::new(),
                    hidden: false,
                    death: false,
                };
                let (x, y, z) = (w.x, w.y, w.z);
                self.wp.waypoints.push(w);
                self.wp_dirty = true;
                Ok(Some(format!("World Map: added '{name}' at {x}, {y}, {z}")))
            }
            "del" => {
                let name = clean_name(rest);
                match self.wp.waypoints.iter().position(|w| w.name == name) {
                    Some(i) => {
                        self.wp.waypoints.remove(i);
                        self.wp_dirty = true;
                        Ok(Some(format!("World Map: deleted '{name}'")))
                    }
                    None => Err(format!("World Map: no waypoint called '{name}' here")),
                }
            }
            "rename" => {
                let (old, new) = if let Some((o, n)) = rest.split_once('|') {
                    (o.trim(), n.trim())
                } else if let Some(stripped) = rest.strip_prefix('"') {
                    match stripped.split_once('"') {
                        Some((o, n)) => (o, n.trim()),
                        None => return Err("World Map: /wp rename <old> | <new>".to_string()),
                    }
                } else {
                    match rest.split_once(' ') {
                        Some((o, n)) => (o, n.trim()),
                        None => return Err("World Map: /wp rename <old> | <new>".to_string()),
                    }
                };
                let new = clean_name(new);
                if new.is_empty() {
                    return Err("World Map: the new name is empty".to_string());
                }
                match self.wp.waypoints.iter_mut().find(|w| w.name == old) {
                    Some(w) => {
                        let old = w.name.clone();
                        w.name = new.clone();
                        self.wp_dirty = true;
                        Ok(Some(format!("World Map: '{old}' is now '{new}'")))
                    }
                    None => Err(format!("World Map: no waypoint called '{old}' here")),
                }
            }
            "list" => {
                if self.wp.waypoints.is_empty() {
                    return Ok(Some("World Map: no waypoints here".to_string()));
                }
                let ppos = game::get_player().map(|p| (p.x, p.y, p.z));
                let lines: Vec<String> = self
                    .wp
                    .waypoints
                    .iter()
                    .enumerate()
                    .map(|(i, w)| {
                        let dist = ppos
                            .map(|pp| format!(" — {:.0}m", distance(pp, w)))
                            .unwrap_or_default();
                        let group = if w.group.is_empty() {
                            String::new()
                        } else {
                            format!(" [{}]", w.group)
                        };
                        let hidden = if w.hidden { " (hidden)" } else { "" };
                        format!(
                            "{}. {} ({}, {}, {}){dist}{group}{hidden}",
                            i + 1,
                            w.name,
                            w.x,
                            w.y,
                            w.z
                        )
                    })
                    .collect();
                Ok(Some(lines.join("\n")))
            }
            "clear" => {
                let n = self.wp.waypoints.len();
                self.wp.waypoints.clear();
                self.wp_dirty = true;
                Ok(Some(format!("World Map: cleared {n} waypoints here")))
            }
            "share" => {
                let mut parts = rest.split_whitespace();
                let Some(n) = parts.next().and_then(|s| s.parse::<usize>().ok()) else {
                    return Err("World Map: /wp share <number> [player|all]".to_string());
                };
                let Some(w) = self.wp.waypoints.get(n.wrapping_sub(1)) else {
                    return Err(format!("World Map: no waypoint number {n} here"));
                };
                let Some(ctx) = &self.ctx else {
                    return Err("World Map: not in a world".to_string());
                };
                let line = share_line(w, &ctx.dimension);
                match parts.next() {
                    None | Some("all") => command::run(&format!("say {line}")),
                    Some(player) => command::run(&format!("msg {player} {line}")),
                }
                Ok(Some(format!("World Map: shared '{}'", w.name)))
            }
            "addshared" => {
                let Some((mut w, dim)) = self.pending_share.take() else {
                    return Err("World Map: no shared waypoint is waiting".to_string());
                };
                let Some(ctx) = &self.ctx else {
                    return Err("World Map: not in a world".to_string());
                };
                let (world_name, here) = (ctx.world.clone(), ctx.dimension.clone());
                let name = w.name.clone();
                if dim == here {
                    self.wp.waypoints.push(w);
                    self.wp_dirty = true;
                } else {
                    let h = context_hash(&world_name, &dim);
                    let mut st = load_wp(&h);
                    w.group = String::new();
                    st.waypoints.push(w);
                    let _ = storage::set(&waypoints_key(&h), &encode_waypoints(&st));
                }
                Ok(Some(format!("World Map: added shared waypoint '{name}'")))
            }
            "import" => match parse_import(rest) {
                Some(w) => {
                    let name = w.name.clone();
                    self.wp.waypoints.push(w);
                    self.wp_dirty = true;
                    Ok(Some(format!("World Map: imported '{name}'")))
                }
                None => Err(
                    "World Map: that line is not a waypoint — /wp import waypoint:<name>:<x>:<y>:<z>:<colour>:..."
                        .to_string(),
                ),
            },
            "export" => {
                let Some(ctx) = &self.ctx else {
                    return Err("World Map: not in a world".to_string());
                };
                let dim = ctx.dimension.clone();
                let lines: Vec<String> = self
                    .wp
                    .waypoints
                    .iter()
                    .map(|w| export_line(w, &dim))
                    .collect();
                let full = lines.join("\n");
                let _ = storage::set("export", full.as_bytes());
                for line in lines.iter().take(40) {
                    chat::show(line);
                }
                Ok(Some(format!(
                    "World Map: exported {} waypoints (the full text is also kept in the mod's storage)",
                    lines.len()
                )))
            }
            "goto" => {
                let mut parts = rest.split_whitespace();
                let (Some(Ok(x)), Some(Ok(z))) = (
                    parts.next().map(|s| s.parse::<i32>()),
                    parts.next().map(|s| s.parse::<i32>()),
                ) else {
                    return Err("World Map: /wp goto <x> <z>".to_string());
                };
                let Some(p) = game::get_player() else {
                    return Err("World Map: not in a world".to_string());
                };
                self.view = (f64::from(x), f64::from(z), 1.0);
                self.push_map_screen((p.x, p.y, p.z));
                self.map_open = true;
                Ok(Some(format!("World Map: map centred on {x}, {z}")))
            }
            "group" => {
                let name = clean_name(rest);
                if name.is_empty() {
                    return Err("World Map: /wp group <name>".to_string());
                }
                if self.wp.groups.contains(&name) {
                    return Ok(Some(format!("World Map: group '{name}' already exists")));
                }
                self.wp.groups.push(name.clone());
                self.wp_dirty = true;
                Ok(Some(format!("World Map: group '{name}' created")))
            }
            _ => Err("World Map: unknown — /wp for help".to_string()),
        }
    }
}

impl Mod for WorldMap {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(MAP_KEY, "Open World Map", "key.keyboard.m")?;
        input::register_key(MINIMAP_KEY, "Minimap On/Off", "key.keyboard.n")?;
        input::register_key(ZOOM_KEY, "Minimap Scale", "key.keyboard.z")?;
        let settings = storage::get("settings")
            .and_then(|b| String::from_utf8(b).ok())
            .map(|t| Settings::parse(&t))
            .unwrap_or_default();
        Ok((
            WorldMap {
                settings,
                ctx: None,
                store: TileStore::default(),
                known: HashMap::new(),
                verified: HashSet::new(),
                fetch: VecDeque::new(),
                queued: HashSet::new(),
                regions: HashSet::new(),
                dirty: HashSet::new(),
                store_full: false,
                feed_cave: false,
                wp: WpState::default(),
                wp_dirty: false,
                viewing: None,
                map_open: false,
                view: (0.0, 0.0, 1.0),
                map_image: (0.0, 0.0, 1.0),
                map_gen: 0,
                shown_gen: 0,
                open_screen: OpenScreen::None,
                selected: None,
                group_filter: None,
                list_viewing: false,
                editor: None,
                editor_from: EditorFrom::List,
                menu_at: None,
                return_to_map: false,
                pending_share: None,
                last_labels: String::new(),
            },
            Registration::new()
                .on_tick()
                .on_frame()
                .on_chat()
                .command("wp"),
        ))
    }

    fn tick(&mut self, tick: u64) {
        match map::context() {
            Some(c) => {
                let hash = context_hash(&c.world, &c.dimension);
                if self.ctx.as_ref().map(|k| &k.hash) != Some(&hash) {
                    self.save_context();
                    self.load_context(Ctx {
                        hash,
                        world: c.world,
                        dimension: c.dimension,
                    });
                }
            }
            None => {
                if self.ctx.is_some() {
                    self.save_context();
                    self.ctx = None;
                    self.map_open = false;
                    map::set_minimap_image(None);
                }
            }
        }
        for event in input::poll() {
            let Event::KeyDown(k) = event else { continue };
            match k.as_str() {
                MAP_KEY => {
                    if let Some(p) = game::get_player() {
                        self.open_map((p.x, p.y, p.z));
                    }
                }
                MINIMAP_KEY => {
                    self.settings.minimap = !self.settings.minimap;
                    self.save_settings();
                    if !self.settings.minimap {
                        map::set_minimap_image(None);
                    }
                }
                ZOOM_KEY => {
                    self.settings.zoom = (self.settings.zoom + 1) % ZOOMS.len();
                    self.save_settings();
                }
                _ => {}
            }
        }
        let Some(player) = game::get_player() else {
            return;
        };
        if self.ctx.is_none() {
            return;
        }
        let dim = player.dimension.clone();
        let ppos = (player.x, player.y, player.z);
        let pc = (
            (player.x.floor() as i32) >> 4,
            (player.z.floor() as i32) >> 4,
        );
        for event in map::poll_events() {
            if let horizon_mod_sdk::PositionEvent::Died(pos) = event
                && self.settings.death_waypoint
            {
                self.add_death_waypoint(pos.x, pos.y, pos.z);
            }
        }
        if tick % 5 == 0 {
            self.sweep(pc);
        }
        self.drain_fetch(pc);
        if let Some(&region) = self.dirty.iter().next() {
            if let Some(ctx) = &self.ctx {
                let hash = ctx.hash.clone();
                self.save_region(&hash, region);
                self.save_index(&hash);
            }
        }
        if self.wp_dirty {
            if let Some(ctx) = &self.ctx {
                let hash = ctx.hash.clone();
                let _ = storage::set(&waypoints_key(&hash), &encode_waypoints(&self.wp));
            }
            self.wp_dirty = false;
        }
        for event in map::poll_map() {
            match event {
                horizon_mod_sdk::MapEvent::Clicked(c) => {
                    if !self.map_open {
                        continue;
                    }
                    let (x, z) = (c.x.floor() as i32, c.z.floor() as i32);
                    let store = self.active_store();
                    let y = Self::ground_y(store, x, z).unwrap_or(player.y.floor() as i32);
                    if c.button == 0 {
                        let vdim = self.view_dimension().unwrap_or_default();
                        let biome = world::get_biome(&vdim, x, y + 1, z);
                        let name = biome
                            .map(|b| format!(" — {}", b.rsplit(':').next().unwrap_or(&b)))
                            .unwrap_or_default();
                        chat::show(&format!("World Map: {x}, {z}{name}"));
                    } else if c.button == 1 {
                        self.open_menu(x, y, z);
                    }
                }
                horizon_mod_sdk::MapEvent::View(v) => {
                    self.view = (v.center_x, v.center_z, v.blocks_per_pixel);
                }
                horizon_mod_sdk::MapEvent::Closed => {
                    self.map_open = false;
                }
            }
        }
        if tick % 5 == 0 {
            self.push_minimap(player.x, player.z, player.yaw, player.y as i32, &dim);
        }
        if self.map_open && tick % 20 == 0 {
            let (vx, vz, vb) = self.view;
            let (ix, iz, ib) = self.map_image;
            let extent = f64::from(ib) * f64::from(MAP_W) / 2.0;
            let drifted = (vx - ix).abs() > extent * 0.25 || (vz - iz).abs() > extent * 0.25;
            let zoomed = vb / ib > 3.0 || vb / ib < 0.34;
            let fresh = self.viewing.is_none() && self.shown_gen != self.map_gen;
            if drifted || zoomed || fresh {
                self.push_map_screen(ppos);
            }
        }
    }

    fn frame(&mut self, _seconds: f32) {
        // In-world markers: the waypoint's name and distance
        // over its place, as the original draws them (its
        // coloured square is the map screen's mark; in the
        // world the name carries its colour).
        if !self.settings.labels || self.ctx.is_none() || self.map_open {
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
            .wp
            .waypoints
            .iter()
            .filter(|w| self.wp.shown(w))
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
            shapes.push(Shape::Text(TextShape {
                text: w.name.clone(),
                at: Point { x: sp.x, y: sp.y },
                color: WP_COLORS[w.color].1 & 0x00FF_FFFF,
                centered: true,
                shadow: true,
            }));
            shapes.push(Shape::Text(TextShape {
                text: format!("{d:.0}m"),
                at: Point {
                    x: sp.x,
                    y: sp.y + 9.0,
                },
                color: 0xFF_FFFF,
                centered: true,
                shadow: true,
            }));
        }
        if sig != self.last_labels {
            render::set_hud(&shapes);
            self.last_labels = sig;
        }
    }

    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        self.wp_command(args)
    }

    fn chat(&mut self, text: &str) {
        if let Some((w, dim)) = parse_share(text) {
            chat::show(&format!(
                "World Map: shared waypoint '{}' at {}, {}, {} — /wp addshared to add it",
                w.name, w.x, w.y, w.z
            ));
            self.pending_share = Some((w, dim));
        }
    }

    fn ui_settings_opened(&mut self) {
        ui::set_settings(&settings_widgets(&self.settings));
    }

    fn ui_closed(&mut self) {
        if self.return_to_map && matches!(self.open_screen, OpenScreen::Menu | OpenScreen::List) {
            if let Some(p) = game::get_player() {
                self.push_map_screen((p.x, p.y, p.z));
                self.map_open = true;
            }
        }
        self.return_to_map = false;
        self.open_screen = OpenScreen::None;
    }

    fn ui_select(&mut self, id: u32, row: u32) {
        if SETTING_BUTTONS.contains(&id) {
            self.cycle_setting(id);
            return;
        }
        match id {
            LIST_WIDGET => {
                self.selected = Some(row as usize);
            }
            LIST_FILTER_BUTTON => {
                let viewing = self.list_viewing;
                let groups = self.wp_ref(viewing).groups.clone();
                let stops: Vec<Option<String>> =
                    std::iter::once(None).chain(groups.into_iter().map(Some)).collect();
                let cur = stops
                    .iter()
                    .position(|s| s == &self.group_filter)
                    .unwrap_or(0);
                self.group_filter = stops[(cur + 1) % stops.len()].clone();
                self.selected = None;
                self.open_list(viewing);
            }
            LIST_NEW_BUTTON => {
                let viewing = self.list_viewing;
                let (x, y, z) = if viewing {
                    let (vx, vz, _) = self.view;
                    let store = self.active_store();
                    let (x, z) = (vx as i32, vz as i32);
                    let y = Self::ground_y(store, x, z).unwrap_or(64);
                    (x, y, z)
                } else if let Some(p) = game::get_player() {
                    (
                        p.x.floor() as i32,
                        p.y.floor() as i32,
                        p.z.floor() as i32,
                    )
                } else {
                    return;
                };
                let n = self.wp_ref(viewing).waypoints.len();
                self.open_editor(
                    None,
                    format!("Waypoint {}", n + 1),
                    x,
                    y,
                    z,
                    n % 16,
                    self.group_filter.clone().unwrap_or_default(),
                    false,
                    viewing,
                    EditorFrom::List,
                );
            }
            LIST_EDIT_BUTTON => {
                let viewing = self.list_viewing;
                if let Some((real, w)) = self.selected_waypoint() {
                    self.open_editor(
                        Some(real),
                        w.name,
                        w.x,
                        w.y,
                        w.z,
                        w.color,
                        w.group,
                        w.hidden,
                        viewing,
                        EditorFrom::List,
                    );
                }
            }
            LIST_TP_BUTTON => {
                if let Some((_, w)) = self.selected_waypoint() {
                    command::run(&format!("tp @s {} {} {}", w.x, w.y, w.z));
                    self.return_to_map = false;
                    ui::close_screen();
                    self.open_screen = OpenScreen::None;
                }
            }
            LIST_SHARE_BUTTON => {
                if let Some((_, w)) = self.selected_waypoint() {
                    let dim = if self.list_viewing {
                        self.view_dimension().unwrap_or_default()
                    } else {
                        self.ctx
                            .as_ref()
                            .map(|c| c.dimension.clone())
                            .unwrap_or_default()
                    };
                    command::run(&format!("say {}", share_line(&w, &dim)));
                }
            }
            LIST_DELETE_BUTTON => {
                let viewing = self.list_viewing;
                if let Some((real, _)) = self.selected_waypoint() {
                    self.active_wp_mut(viewing).waypoints.remove(real);
                    self.mark_wp_dirty(viewing);
                    self.selected = None;
                    self.open_list(viewing);
                }
            }
            id if (LIST_GROUP_BASE..LIST_GROUP_BASE + 10).contains(&id) => {
                let viewing = self.list_viewing;
                let i = (id - LIST_GROUP_BASE) as usize;
                let group = self.wp_ref(viewing).groups.get(i).cloned();
                if let Some(g) = group {
                    let st = self.active_wp_mut(viewing);
                    if !st.hidden_groups.remove(&g) {
                        st.hidden_groups.insert(g);
                    }
                    self.mark_wp_dirty(viewing);
                    self.open_list(viewing);
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
            EDITOR_COLOR_BUTTON => {
                if let Some(e) = &mut self.editor {
                    e.color = (e.color + 1) % 16;
                }
                self.show_editor();
            }
            EDITOR_GROUP_BUTTON => {
                let viewing = self.editor.as_ref().map(|e| e.viewing).unwrap_or(false);
                let groups = self.wp_ref(viewing).groups.clone();
                if let Some(e) = &mut self.editor {
                    let stops: Vec<String> =
                        std::iter::once(String::new()).chain(groups).collect();
                    let cur = stops.iter().position(|g| *g == e.group).unwrap_or(0);
                    e.group = stops[(cur + 1) % stops.len()].clone();
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
                let Some(e) = self.editor.as_ref() else { return };
                let w = Waypoint {
                    name: if e.name.trim().is_empty() {
                        "Waypoint".to_string()
                    } else {
                        clean_name(&e.name)
                    },
                    x: e.x,
                    y: e.y,
                    z: e.z,
                    color: e.color,
                    group: e.group.clone(),
                    hidden: e.hidden,
                    death: false,
                };
                let (index, viewing) = (e.index, e.viewing);
                let st = self.active_wp_mut(viewing);
                match index {
                    Some(i) if i < st.waypoints.len() => {
                        let death = st.waypoints[i].death;
                        st.waypoints[i] = Waypoint { death, ..w };
                    }
                    _ => st.waypoints.push(w),
                }
                self.mark_wp_dirty(viewing);
                self.leave_editor();
            }
            EDITOR_CANCEL_BUTTON => {
                self.leave_editor();
            }
            MENU_ADD_BUTTON => {
                let Some((x, y, z)) = self.menu_at else { return };
                let viewing = self.viewing.is_some();
                let n = self.wp_ref(viewing).waypoints.len();
                self.open_editor(
                    None,
                    format!("Waypoint {}", n + 1),
                    x,
                    y,
                    z,
                    n % 16,
                    String::new(),
                    false,
                    viewing,
                    EditorFrom::Map,
                );
            }
            MENU_TP_BUTTON => {
                if let Some((x, y, z)) = self.menu_at {
                    command::run(&format!("tp @s {x} {y} {z}"));
                    self.return_to_map = false;
                    ui::close_screen();
                    self.open_screen = OpenScreen::None;
                }
            }
            MENU_LIST_BUTTON => {
                let viewing = self.viewing.is_some();
                self.selected = None;
                self.open_list(viewing);
            }
            MENU_DIM_BUTTON => {
                let Some(ctx) = &self.ctx else { return };
                let world = ctx.world.clone();
                let dims = self.world_dimensions(&world);
                if dims.len() > 1 {
                    let cur = self.view_dimension().unwrap_or_default();
                    let at = dims.iter().position(|d| *d == cur).unwrap_or(0);
                    let next = dims[(at + 1) % dims.len()].clone();
                    self.set_view_dimension(&next);
                }
                self.return_to_map = false;
                ui::close_screen();
                self.open_screen = OpenScreen::None;
                if let Some(p) = game::get_player() {
                    self.push_map_screen((p.x, p.y, p.z));
                    self.map_open = true;
                }
            }
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(WorldMap);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_25() {
        assert_eq!(API_LEVEL, 25);
    }

    #[test]
    fn shading_is_the_map_items_rule() {
        assert_eq!(brightness(7 | WALL, 64, Some(70), 0, 0), 3);
        assert_eq!(brightness(2, 70, Some(60), 0, 0), 2);
        assert_eq!(brightness(WATER, 60, None, 15, 0), 0);
        assert_eq!(shade(2, 70, Some(60), 0, 4, 4, 1.0), 0xFFF7_E9A3);
        assert_eq!(shade(0, 70, Some(60), 0, 0, 0, 1.0), UNKNOWN);
        assert!((daylight(6000) - 1.0).abs() < 1e-6);
        assert!((daylight(18000) - 0.35).abs() < 1e-6);
    }

    #[test]
    fn region_codec_roundtrips() {
        let mut tile = Tile {
            layer: MapLayer::Surface,
            cut_y: None,
            version: 3,
            colors: [0; 256],
            heights: [0; 256],
            depths: [0; 256],
        };
        for i in 0..256 {
            tile.colors[i] = (i % 9) as u8;
            tile.heights[i] = 100 - (i as i16 % 7);
        }
        let blob = encode_region(&[(9, &tile)]);
        let back = decode_region(-1, 3, &blob);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].0, (-8 + 1, 24 + 1));
        assert_eq!(back[0].1.colors, tile.colors);
        assert_eq!(back[0].1.heights, tile.heights);
        assert!(decode_region(0, 0, &[9, 9]).is_empty());
    }

    #[test]
    fn waypoint_blob_keeps_groups() {
        let mut st = WpState::default();
        st.groups = vec!["Bases".to_string(), "Mines".to_string()];
        st.hidden_groups.insert("Mines".to_string());
        st.waypoints = vec![
            Waypoint {
                name: "Home".into(),
                x: 1,
                y: 2,
                z: 3,
                color: 4,
                group: "Bases".into(),
                hidden: false,
                death: false,
            },
            Waypoint {
                name: "Death Point".into(),
                x: 9,
                y: 9,
                z: 9,
                color: RED,
                group: String::new(),
                hidden: false,
                death: true,
            },
        ];
        let back = decode_waypoints(&encode_waypoints(&st));
        assert_eq!(back.groups, st.groups);
        assert!(back.hidden_groups.contains("Mines"));
        assert_eq!(back.waypoints, st.waypoints);
        assert!(back.shown(&back.waypoints[0]));
        let mut st2 = back.clone();
        st2.hidden_groups.insert("Bases".to_string());
        assert!(!st2.shown(&st2.waypoints[0]));
    }

    #[test]
    fn share_and_import_formats() {
        let w = Waypoint {
            name: "Home".into(),
            x: 12,
            y: 70,
            z: -34,
            color: 5,
            group: "Bases".into(),
            hidden: false,
            death: false,
        };
        let line = format!("<Steve> {}", share_line(&w, "minecraft:the_nether"));
        let (back, dim) = parse_share(&line).unwrap();
        assert_eq!(back.name, "Home");
        assert_eq!(dim, "minecraft:the_nether");
        // Export writes the Xaero shape the import reads.
        let exported = export_line(&w, "minecraft:overworld");
        assert!(exported.starts_with("waypoint:Home:12:70:-34:5:false:0:Internal:overworld"));
        let imported = parse_import(&exported).unwrap();
        assert_eq!(
            (imported.x, imported.y, imported.z, imported.color),
            (12, 70, -34, 5)
        );
        assert!(parse_import("not a waypoint").is_none());
    }

    #[test]
    fn settings_roundtrip() {
        let mut s = Settings::default();
        s.minimap = false;
        s.mode = 2;
        s.cave_full = true;
        let back = Settings::parse(std::str::from_utf8(&s.encode()).unwrap());
        assert!(!back.minimap);
        assert_eq!(back.mode, 2);
        assert!(back.cave_full);
        assert_eq!(back.display().mode, 2);
    }

    #[test]
    fn letters_have_pixels() {
        for c in ['N', 'E', 'S', 'W'] {
            let g = glyph(c);
            assert!(g.iter().flatten().any(|p| *p), "{c} is blank");
        }
        assert_ne!(glyph('N'), glyph('W'));
    }

    #[test]
    fn compose_reads_the_store() {
        let mut store = TileStore::default();
        let mut tile = Tile {
            layer: MapLayer::Surface,
            cut_y: None,
            version: 1,
            colors: [2; 256],
            heights: [64; 256],
            depths: [0; 256],
        };
        tile.colors = [2; 256];
        store.put((0, 0), tile);
        let d = Display {
            mode: 0,
            full: false,
            top_y: 64,
        };
        let img = compose(&store, 8.0, 8.0, 1.0, 16, 16, d, false, 65, 1.0, None, false, &[]);
        let centre = img.pixels[8 * 16 + 8];
        assert_eq!(img.palette[centre as usize], shade(2, 64, Some(64), 0, 8, 8, 1.0));
    }
}

//! Atlas for Kestrel: a minimap that turns with you, a
//! full-screen world map of everywhere you have been, and
//! waypoints — the Xaero-style pair.
//!
//! Behaviour reimplemented from Atlas by MrCloudy2, MIT,
//! <https://github.com/MrCloudy2/horizon-atlas>: this file
//! shares no code with the original — the original is a native
//! mod for Horizon, this is a fresh implementation against the
//! Kestrel SDK (API 25, capability `map`). This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/atlas>.
//!
//! Fidelity, against the original's registry description: the
//! minimap and the world map share one map of the world, kept
//! per world, server and dimension — here, an accumulation of
//! the game's own map tiles (`map.tiles`) in the mod's
//! `storage`, keyed by `map.context`. The map colours and slope
//! shading are the map item's own (the game's rule, restated
//! below), darkened by the time of day as the original's 0.2.0
//! shading pass describes. Waypoints have sets, Share and
//! Teleport run through `command.run` exactly as the original
//! admits to sending them, deathpoints come from
//! `map.poll-events`, and `/atlas seed` takes the seed as the
//! original takes it. What is NOT ported is listed in the
//! README: faces and item icons on the minimap (no skin or
//! dropped-item read exists), the light overlay and block
//! light at night (the tiles carry no light), and text-field
//! editing (names go through `/atlas rename`).

use std::collections::{HashMap, HashSet, VecDeque};

use horizon_mod_sdk::{
    ChunkPos, Corner as MapCorner, MapImage, MapLayer, MapMarker, MapScreen, Mod, Registration,
    chat, command, entity, game, input,
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
// The map item's colours and shading, restated: the tiles a mod
// reads are colour ids, and turning them into a picture is the
// vanilla rule — `MapColor.col` by id, shaded by the brightness
// the map item computes (brighter facing a lower north
// neighbour, darker facing a higher one, dithered; water by
// depth; a cut wall brightest). The game applies the same rule
// in `horizon_client::minimap` (`brightness` / `packed_rgba`);
// this is that rule, so the port's map matches the game's own.

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
/// factor (1.0 by day; the night darkening scales the channels,
/// never the alpha). Colour id 0 (or past the table) reads as
/// the game's unexplored colour.
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

/// The daylight factor for a day-time: 1.0 at midday, 0.35 at
/// midnight, a cosine between — the port's curve for the
/// original's "the map darkens with the time of day". Only the
/// Overworld has a day; callers pass 1.0 elsewhere.
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
    /// The column at local (x, z) of this chunk: colour id,
    /// height, fluid depth.
    fn column(&self, x: i32, z: i32) -> (u8, i16, u8) {
        let i = (z * 16 + x) as usize;
        (self.colors[i], self.heights[i], self.depths[i])
    }
}

/// The mod's accumulated map of one context: surface tiles by
/// chunk, and cave tiles by chunk and the head height they
/// were scanned at — a cave and a surface tile of one chunk
/// are different maps, and the store keeps both.
#[derive(Default)]
struct TileStore {
    surface: HashMap<(i32, i32), Tile>,
    cave: HashMap<(i32, i32), Vec<Tile>>,
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
    /// The tile a display mode shows for one chunk. `head_y`
    /// picks the cave cut nearest the player's head; mode 0
    /// (Auto) follows the game's own current layer (`feed`),
    /// 1 (Cave) is the cave at the head, 2 (Layer) the cave at
    /// the layer setting's height.
    fn display(
        &self,
        chunk: (i32, i32),
        mode: u8,
        feed_cave: bool,
        head_y: i32,
        layer_y: i32,
    ) -> Option<&Tile> {
        let cave_at = |y: i32| {
            self.cave.get(&chunk).and_then(|cuts| {
                cuts.iter().min_by_key(|t| {
                    let c = t.cut_y.unwrap_or(y);
                    ((c - y).abs(), if c <= y { 0 } else { 1 })
                })
            })
        };
        match mode {
            1 => cave_at(head_y).or_else(|| self.surface.get(&chunk)),
            2 => cave_at(layer_y).or_else(|| self.surface.get(&chunk)),
            _ => {
                if feed_cave {
                    cave_at(head_y).or_else(|| self.surface.get(&chunk))
                } else {
                    self.surface.get(&chunk)
                }
            }
        }
    }
    /// The shaded colour of one world column under a display
    /// mode, or None where the store holds no tile for it.
    fn sample(
        &self,
        wx: i32,
        wz: i32,
        mode: u8,
        feed_cave: bool,
        head_y: i32,
        layer_y: i32,
        day: f32,
    ) -> Option<u32> {
        let chunk = (wx >> 4, wz >> 4);
        let tile = self.display(chunk, mode, feed_cave, head_y, layer_y)?;
        let (color, height, depth) = tile.column(wx & 15, wz & 15);
        // The north neighbour shades this column; it may live
        // in the chunk above.
        let north = if (wz & 15) == 0 {
            self.display((chunk.0, chunk.1 - 1), mode, feed_cave, head_y, layer_y)
                .map(|t| t.column(wx & 15, 15).1)
        } else {
            Some(tile.column(wx & 15, (wz & 15) - 1).1)
        };
        Some(shade(color, height, north, depth, wx, wz, day))
    }
}

// ----------------------------------------------------------------
// Persistence: regions of 8x8 chunks under the mod's storage,
// a run-length codec over the tiles' arrays. All keys are
// per context (a hash of the world and dimension names).

/// How many chunks a stored region spans, each way.
const REGION: i32 = 8;

/// A short stable hash of a context's names (FNV-1a): storage
/// keys are `[a-z0-9_.-]`, world and server names are not.
fn context_hash(world: &str, dimension: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in world.bytes().chain([1]).chain(dimension.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
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

fn region_key(ctx: &str, rx: i32, rz: i32) -> String {
    format!("t.{ctx}.{rx}.{rz}")
}
fn index_key(ctx: &str) -> String {
    format!("i.{ctx}")
}
fn waypoints_key(ctx: &str) -> String {
    format!("w.{ctx}")
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

/// Decodes (value, count) pairs into exactly `want` bytes;
/// None when the stream runs out early or overruns.
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

/// One region's tiles as bytes: a count, then per tile its
/// slot in the region, layer, cut, version, and the three
/// arrays (colours and depths run-length, heights as deltas).
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
/// does not parse reads as an empty region (the feed refills
/// it), never as an error.
fn decode_region(rx: i32, rz: i32, data: &[u8]) -> Vec<((i32, i32), Tile)> {
    let mut out = Vec::new();
    if data.len() < 2 {
        return out;
    }
    let count = u16::from_le_bytes([data[0], data[1]]) as usize;
    let mut at = 2;
    // RLE streams are self-delimiting only against the 256
    // target, so decode tile by tile with a cursor over a
    // working slice: colours first (its byte length is found
    // by walking the pairs), then heights, then depths.
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
        // Colours: walk pairs until 256 values are accounted.
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
// Waypoints: the sixteen colours, and the store.

/// The sixteen waypoint colours, named as the colour setting
/// names them (the dye colours, as Xaero's palette names
/// them).
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

/// One waypoint in a context's active set system.
#[derive(Clone, PartialEq, Debug)]
struct Waypoint {
    name: String,
    x: i32,
    y: i32,
    z: i32,
    color: usize,
    set: String,
    hidden: bool,
    temporary: bool,
    death: bool,
}

/// A name storage can carry inside a line: tabs and newlines
/// become spaces (they are the format's separators).
fn clean_name(name: &str) -> String {
    name.chars()
        .map(|c| if c == '\t' || c == '\n' || c == '\r' { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// The waypoint blob: a `#set` header naming the active set,
/// then one tab-separated waypoint a line.
fn encode_waypoints(active_set: &str, wps: &[Waypoint]) -> Vec<u8> {
    let mut out = format!("#set\t{}\n", clean_name(active_set));
    for w in wps {
        let mut flags = String::new();
        if w.hidden {
            flags.push('h');
        }
        if w.temporary {
            flags.push('t');
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
            clean_name(&w.set),
            flags
        ));
    }
    out.into_bytes()
}

fn decode_waypoints(data: &[u8]) -> (String, Vec<Waypoint>) {
    let mut active = "Default".to_string();
    let mut out = Vec::new();
    let Ok(text) = std::str::from_utf8(data) else {
        return (active, out);
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() == 2 && f[0] == "#set" {
            active = f[1].to_string();
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
        out.push(Waypoint {
            name: f[0].to_string(),
            x,
            y,
            z,
            color: color.min(15),
            set: if f[5].is_empty() {
                "Default".to_string()
            } else {
                f[5].to_string()
            },
            hidden: f[6].contains('h'),
            temporary: f[6].contains('t'),
            death: f[6].contains('d'),
        });
    }
    (active, out)
}

/// The set names in use, "Default" first, the active one
/// included even when it holds no waypoint yet.
fn set_names<'a>(active: &'a str, wps: &'a [Waypoint]) -> Vec<String> {
    let mut out = vec!["Default".to_string()];
    for w in wps {
        if !out.contains(&w.set) {
            out.push(w.set.clone());
        }
    }
    if !out.iter().any(|s| s == active) {
        out.push(active.to_string());
    }
    out
}

/// The 3D distance from the player to a waypoint, in blocks.
fn distance(p: (f64, f64, f64), w: &Waypoint) -> f64 {
    let dx = p.0 - (f64::from(w.x) + 0.5);
    let dy = p.1 - f64::from(w.y);
    let dz = p.2 - (f64::from(w.z) + 0.5);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

// ----------------------------------------------------------------
// Slime chunks: the game's own rule over `java.util.Random`,
// reimplemented (the 48-bit LCG, with its `nextInt` rejection)
// so the overlay can mark them from the seed the player types
// to `/atlas seed`, as the original takes it.

struct JavaRandom {
    seed: i64,
}

impl JavaRandom {
    fn new(seed: i64) -> JavaRandom {
        JavaRandom {
            seed: (seed ^ 0x5DEE_CE66D) & ((1 << 48) - 1),
        }
    }
    fn next(&mut self, bits: u32) -> i64 {
        self.seed = (self.seed.wrapping_mul(0x5DEE_CE66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        self.seed >> (48 - bits)
    }
    fn next_int(&mut self, bound: i64) -> i64 {
        if bound & (bound - 1) == 0 {
            return (bound * self.next(31)) >> 31;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            // Java's rejection test, in its overflowing ints;
            // done in i64 here it never triggers spuriously for
            // bound 10, and is kept for exactness.
            if bits - val + (bound - 1) >= 0 {
                return val;
            }
        }
    }
}

/// Is this chunk a slime chunk under `seed`? The game's
/// formula, in Java's wrapping int arithmetic.
fn slime_chunk(seed: i64, cx: i32, cz: i32) -> bool {
    let (x, z) = (i64::from(cx), i64::from(cz));
    let s = seed
        .wrapping_add(x.wrapping_mul(x).wrapping_mul(4987142))
        .wrapping_add(x.wrapping_mul(5947611))
        .wrapping_add(z.wrapping_mul(z).wrapping_mul(4392871))
        .wrapping_add(z.wrapping_mul(389711))
        ^ 987234911;
    JavaRandom::new(s).next_int(10) == 0
}

// ----------------------------------------------------------------
// Settings: one storage blob of `key=value` lines (the map
// mods' tile regions need the storage key budget, so settings
// share one key — the house style of one key per setting
// would starve the map).

#[derive(Clone)]
struct Settings {
    minimap: bool,
    corner: usize,
    size: usize,
    round: bool,
    rotate: bool,
    zoom: usize,
    mode: usize,
    layer_y: usize,
    grid: bool,
    slime: bool,
    entity_dots: bool,
    deathpoints: bool,
    labels: bool,
}

const CORNERS: [MapCorner; 4] = [
    MapCorner::TopRight,
    MapCorner::TopLeft,
    MapCorner::BottomRight,
    MapCorner::BottomLeft,
];
const CORNER_NAMES: [&str; 4] = ["Top Right", "Top Left", "Bottom Right", "Bottom Left"];
const SIZES: [u32; 4] = [100, 128, 160, 200];
const ZOOMS: [f32; 3] = [0.5, 1.0, 2.0];
const ZOOM_NAMES: [&str; 3] = ["2x", "1x", "0.5x"];
const MODES: [&str; 3] = ["Auto", "Cave", "Layer"];
const LAYER_YS: [i32; 9] = [-64, 0, 32, 64, 96, 128, 192, 256, 320];

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            minimap: true,
            corner: 0,
            size: 1,
            round: false,
            rotate: true,
            zoom: 1,
            mode: 0,
            layer_y: 3,
            grid: false,
            slime: false,
            entity_dots: true,
            deathpoints: true,
            labels: true,
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
                "round" => s.round = b,
                "rotate" => s.rotate = b,
                "zoom" => s.zoom = n.min(2),
                "mode" => s.mode = n.min(2),
                "layer_y" => s.layer_y = n.min(8),
                "grid" => s.grid = b,
                "slime" => s.slime = b,
                "entity_dots" => s.entity_dots = b,
                "deathpoints" => s.deathpoints = b,
                "labels" => s.labels = b,
                _ => {}
            }
        }
        s
    }
    fn encode(&self) -> Vec<u8> {
        format!(
            "minimap={}\ncorner={}\nsize={}\nround={}\nrotate={}\nzoom={}\nmode={}\nlayer_y={}\ngrid={}\nslime={}\nentity_dots={}\ndeathpoints={}\nlabels={}\n",
            self.minimap as u8,
            self.corner,
            self.size,
            self.round as u8,
            self.rotate as u8,
            self.zoom,
            self.mode,
            self.layer_y,
            self.grid as u8,
            self.slime as u8,
            self.entity_dots as u8,
            self.deathpoints as u8,
            self.labels as u8,
        )
        .into_bytes()
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
    /// The index of a colour, adding it while there is room;
    /// past 256 colours, a shaded variant stands in (a full
    /// palette can only happen on a screen showing nearly
    /// every map colour at every brightness at once).
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

/// What a composition draws besides the terrain.
struct Overlays<'a> {
    /// Chunk-boundary lines (the chunk grid overlay).
    grid: bool,
    /// The world seed, when the slime overlay is on and a seed
    /// is set for this context.
    slime_seed: Option<i64>,
    /// Dots: (world x, world z, colour, is-death) — waypoints
    /// and entities on the minimap; the map screen's marks
    /// are the host's `MapMarker`s instead.
    dots: &'a [(f64, f64, u32, bool)],
}

/// Composes a north-up picture of `w` x `h` pixels centred on
/// the world block (`center_x`, `center_z`) at
/// `blocks_per_pixel`. With `rotate` (the minimap), the view
/// turns so the player's facing is up: `yaw` is the game's
/// yaw, and the sampling rotates around the centre.
#[allow(clippy::too_many_arguments)]
fn compose(
    store: &TileStore,
    center_x: f64,
    center_z: f64,
    bpp: f32,
    w: u32,
    h: u32,
    mode: u8,
    feed_cave: bool,
    head_y: i32,
    layer_y: i32,
    day: f32,
    rotate: Option<f32>,
    overlays: &Overlays,
) -> MapImage {
    let mut palette = Palette::new();
    // Reserve the overlay colours first, so dots never lose
    // their colour to a full terrain palette.
    let grid_i = palette.get(0x6600_0000);
    let slime_i = palette.get(0x5533_FF33);
    let dot_idx: Vec<u8> = overlays.dots.iter().map(|d| palette.get(d.2)).collect();
    let bpp = f64::from(bpp);
    let (hw, hh) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
    // The view's axes: world (right, forward) per screen
    // (right, up). North-up is east right and north forward;
    // a turning view takes the game's own axes for its yaw.
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
            let mut idx = match store.sample(bx, bz, mode, feed_cave, head_y, layer_y, day) {
                Some(argb) => palette.get(argb),
                None => palette.get(UNKNOWN),
            };
            if overlays.grid && bpp <= 2.0 && ((bx & 15) == 0 || (bz & 15) == 0) {
                // The chunk grid: the first block column of
                // each chunk, both ways.
                idx = grid_i;
            }
            if let Some(seed) = overlays.slime_seed
                && slime_chunk(seed, bx >> 4, bz >> 4)
            {
                idx = slime_i;
            }
            pixels[(j * w + i) as usize] = idx;
        }
    }
    // Dots last, over the terrain: a filled 3x3, or a 5x5 X
    // for a death.
    for (n, &(dx, dz, _, death)) in overlays.dots.iter().enumerate() {
        let rel_x = dx - center_x;
        let rel_z = dz - center_z;
        // Screen offset: solve rel = bpp*(u*r - v*f) for (u,v)
        // (r and f are orthonormal).
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

// ----------------------------------------------------------------
// The mod.

/// Which context's data is loaded: a world (or server) and a
/// dimension, as `map.context` names them.
struct Ctx {
    hash: String,
    world: String,
    dimension: String,
}

/// Which of the mod's own screens is open.
#[derive(Clone, Copy, PartialEq)]
enum OpenScreen {
    None,
    Settings,
    List,
    Editor,
}

/// A waypoint being made on the editor screen.
struct Editor {
    name: String,
    x: i32,
    y: i32,
    z: i32,
    color: usize,
    set: String,
    temporary: bool,
    from_map: bool,
}

/// How far around the player the tile feed sweeps, in chunks:
/// the game's cache reaches the loaded chunks, and this
/// covers a minimap's reach and a good walk beyond it.
const FEED_RADIUS: i32 = 10;

/// The map screen's picture size; the host scales it to fill
/// the screen and fields pan and zoom over it.
const MAP_W: u32 = 1024;
const MAP_H: u32 = 1024;

struct Atlas {
    settings: Settings,
    ctx: Option<Ctx>,
    store: TileStore,
    /// The host version last seen per chunk, and the chunks
    /// this session has verified against the live cache (a
    /// stored tile's version is from an older cache: its
    /// chunk is re-read once before versions are trusted).
    known: HashMap<(i32, i32), u64>,
    verified: HashSet<(i32, i32)>,
    fetch: VecDeque<(i32, i32)>,
    queued: HashSet<(i32, i32)>,
    /// Regions in storage for this context (the index), and
    /// the ones whose stored copy is stale.
    regions: HashSet<(i32, i32)>,
    dirty: HashSet<(i32, i32)>,
    seed: Option<i64>,
    store_full: bool,
    /// The game's current layer at the player's chunk, from
    /// the last tile fetched there.
    feed_cave: bool,
    waypoints: Vec<Waypoint>,
    active_set: String,
    waypoints_dirty: bool,
    /// The map screen's state: open, and the view the host
    /// last reported (centre x, centre z, blocks per pixel).
    map_open: bool,
    view: (f64, f64, f32),
    /// The centre and scale of the picture last pushed to
    /// the map screen; the view drifting from it, or the
    /// ground under it changing, recomposes.
    map_image: (f64, f64, f32),
    /// Bumped when a tile inside the map view is stored, so
    /// the open map refreshes as its ground arrives.
    map_gen: u64,
    shown_gen: u64,
    open_screen: OpenScreen,
    selected: Option<usize>,
    /// The list screen's filter: the active set only, or all.
    list_all: bool,
    editor: Option<Editor>,
    /// A waypoint another player shared in chat, and the
    /// dimension it belongs to, waiting for `/atlas addshared`.
    pending_share: Option<(Waypoint, String)>,
    last_labels: String,
}

impl Atlas {
    // ---- settings persistence ----

    fn save_settings(&self) {
        let _ = storage::set("settings", &self.settings.encode());
    }

    // ---- context load and save ----

    /// Loads a context's index, regions and waypoints. The
    /// caller has already saved the previous context.
    fn load_context(&mut self, ctx: Ctx) {
        self.store = TileStore::default();
        self.known.clear();
        self.verified.clear();
        self.fetch.clear();
        self.queued.clear();
        self.regions.clear();
        self.dirty.clear();
        self.seed = None;
        self.store_full = false;
        self.feed_cave = false;
        if let Some(data) = storage::get(&index_key(&ctx.hash)) {
            if let Ok(text) = std::str::from_utf8(&data) {
                let mut lines = text.lines();
                if let Some(first) = lines.next() {
                    for pair in first.split(';') {
                        let Some((a, b)) = pair.split_once(',') else {
                            continue;
                        };
                        if let (Ok(rx), Ok(rz)) = (a.parse::<i32>(), b.parse::<i32>()) {
                            self.regions.insert((rx, rz));
                        }
                    }
                }
                for line in lines {
                    if let Some(v) = line.strip_prefix("seed ") {
                        self.seed = v.trim().parse::<i64>().ok();
                    }
                }
            }
        }
        let regions: Vec<(i32, i32)> = self.regions.iter().copied().collect();
        for (rx, rz) in regions {
            if let Some(data) = storage::get(&region_key(&ctx.hash, rx, rz)) {
                for (chunk, tile) in decode_region(rx, rz, &data) {
                    self.known.insert(chunk, tile.version);
                    self.store.put(chunk, tile);
                }
            }
        }
        let (active, wps) = match storage::get(&waypoints_key(&ctx.hash)) {
            Some(data) => decode_waypoints(&data),
            None => ("Default".to_string(), Vec::new()),
        };
        self.active_set = active;
        self.waypoints = wps;
        self.waypoints_dirty = false;
        self.ctx = Some(ctx);
        // The game's own last death seeds a deathpoint, as the
        // original keeps deathpoints between sessions.
        if self.settings.deathpoints {
            if let Some(pos) = map::last_death() {
                let dim = self.ctx.as_ref().map(|c| c.dimension.clone());
                if dim.as_deref() == Some(pos.dimension.as_str()) {
                    self.add_deathpoint(pos.x, pos.y, pos.z, false);
                }
            }
        }
    }

    /// Saves everything outstanding for the loaded context.
    fn save_context(&mut self) {
        let Some(ctx) = &self.ctx else { return };
        let hash = ctx.hash.clone();
        let dirty: Vec<(i32, i32)> = self.dirty.iter().copied().collect();
        for r in dirty {
            self.save_region(&hash, r);
        }
        self.save_index(&hash);
        if self.waypoints_dirty {
            let _ = storage::set(
                &waypoints_key(&hash),
                &encode_waypoints(&self.active_set, &self.waypoints),
            );
            self.waypoints_dirty = false;
        }
    }

    fn save_index(&self, hash: &str) {
        let mut list: Vec<(i32, i32)> = self.regions.iter().copied().collect();
        list.sort();
        let mut text = list
            .iter()
            .map(|(a, b)| format!("{a},{b}"))
            .collect::<Vec<_>>()
            .join(";");
        if let Some(seed) = self.seed {
            text.push_str(&format!("\nseed {seed}"));
        }
        let _ = storage::set(&index_key(hash), text.as_bytes());
    }

    /// Writes one region from the store. On a quota refusal,
    /// the farthest stored region is evicted and the write
    /// retried — the platform's storage is capped (4 MiB,
    /// 64 keys) where the original writes files freely; the
    /// README names it.
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
        // Evict the farthest other region and retry, twice.
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

    /// Drops a region's tiles from the in-memory store (its
    /// stored copy has been evicted).
    fn evict_memory(&mut self, region: (i32, i32)) {
        let inside = |c: &(i32, i32)| (c.0.div_euclid(REGION), c.1.div_euclid(REGION)) == region;
        self.store.surface.retain(|c, _| !inside(c));
        self.store.cave.retain(|c, _| !inside(c));
        self.known.retain(|c, _| !inside(c));
        self.verified.retain(|c| !inside(c));
    }

    // ---- waypoints ----

    /// Adds a deathpoint for a death at (x, y, z), unless one
    /// already stands there. `announce` is for a death that
    /// just happened under the running mod.
    fn add_deathpoint(&mut self, x: i32, y: i32, z: i32, _announce: bool) {
        if self
            .waypoints
            .iter()
            .any(|w| w.death && w.x == x && w.y == y && w.z == z)
        {
            return;
        }
        let n = self.waypoints.iter().filter(|w| w.death).count();
        let name = if n == 0 {
            "Deathpoint".to_string()
        } else {
            format!("Deathpoint ({})", n + 1)
        };
        self.waypoints.push(Waypoint {
            name,
            x,
            y,
            z,
            color: RED,
            set: self.active_set.clone(),
            hidden: false,
            temporary: false,
            death: true,
        });
        self.waypoints_dirty = true;
    }

    // ---- the tile feed ----

    /// One sweep: versions over the chunks around the player,
    /// queueing fetches for chunks that are new, changed, or
    /// not yet verified this session.
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

    /// Fetches queued tiles, a few calls a tick, into the
    /// store.
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
                    // A tile inside the open map's view: the
                    // map refreshes.
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
}

// ----------------------------------------------------------------
// Composition drivers: the minimap and the map screen.

/// The widget ids of the mod's screens.
const SETTING_BUTTONS: std::ops::Range<u32> = 0..13;
const LIST_WIDGET: u32 = 20;
const LIST_SET_BUTTON: u32 = 21;
const LIST_NEW_BUTTON: u32 = 22;
const LIST_TP_BUTTON: u32 = 23;
const LIST_SHARE_BUTTON: u32 = 24;
const LIST_DELETE_BUTTON: u32 = 25;
const EDITOR_COLOR_BUTTON: u32 = 30;
const EDITOR_SET_BUTTON: u32 = 31;
const EDITOR_TEMP_BUTTON: u32 = 32;
const EDITOR_CREATE_BUTTON: u32 = 33;
const EDITOR_CANCEL_BUTTON: u32 = 34;

/// Key names, as registered.
const MAP_KEY: &str = "map";
const LIST_KEY: &str = "list";
const NEW_KEY: &str = "new";
const LABELS_KEY: &str = "labels";
const SETTINGS_KEY: &str = "settings";

/// Mob kinds drawn as red dots on the minimap; everything
/// else living is a green one. (The original fades dots with
/// height and colours them by kind, as Xaero does.)
const HOSTILE: &[&str] = &[
    "zombie", "skeleton", "creeper", "spider", "cave_spider", "enderman", "witch", "pillager",
    "vindicator", "evoker", "ravager", "phantom", "drowned", "husk", "stray", "blaze",
    "magma_cube", "ghast", "slime", "guardian", "elder_guardian", "shulker", "hoglin", "zoglin",
    "piglin_brute", "warden", "breeze", "bogged", "silverfish", "endermite", "vex", "wither",
    "zombie_villager", "skeleton_horse", "zombie_horse", "illusioner", "giant",
];

/// A dot's colour, faded toward clear as the mob's height
/// leaves the player's: full within 8 blocks, a quarter at
/// 32 and beyond.
fn entity_dot(kind: &str, dy: f64) -> u32 {
    let base = if HOSTILE.contains(&kind) {
        0xFF40_40u32
    } else {
        0x9ACD_32u32
    };
    let a = dy.abs();
    let fade = if a <= 8.0 {
        1.0
    } else if a >= 32.0 {
        0.25
    } else {
        1.0 - (a - 8.0) / 24.0 * 0.75
    };
    (((255.0 * fade) as u32) << 24) | base
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

/// The share line a waypoint travels as, in chat: the
/// original sends a chat message with the waypoint, and a
/// client running the port reads it back out of the line.
fn share_line(w: &Waypoint, dimension: &str) -> String {
    format!(
        "atlas-waypoint:{}:{}:{}:{}:{}:{}",
        clean_name(&w.name).replace(':', " "),
        w.x,
        w.y,
        w.z,
        w.color,
        dimension
    )
}

/// Reads a share line out of a chat line, if it carries one.
fn parse_share(line: &str) -> Option<Waypoint> {
    let at = line.find("atlas-waypoint:")? + "atlas-waypoint:".len();
    let f: Vec<&str> = line[at..].split(':').collect();
    if f.len() < 6 {
        return None;
    }
    Some(Waypoint {
        name: f[0].to_string(),
        x: f[1].parse().ok()?,
        y: f[2].parse().ok()?,
        z: f[3].parse().ok()?,
        color: f[4].parse::<usize>().ok()?.min(15),
        set: "Default".to_string(),
        hidden: false,
        temporary: false,
        death: false,
    })
}

/// The dimension a share line names (its last field).
fn share_dimension(line: &str) -> Option<String> {
    let at = line.find("atlas-waypoint:")? + "atlas-waypoint:".len();
    let f: Vec<&str> = line[at..].split(':').collect();
    if f.len() < 6 {
        return None;
    }
    // The dimension itself contains a ':' (minecraft:...), so
    // everything from field 5 on is the dimension.
    Some(f[5..].join(":"))
}

impl Atlas {
    /// The daylight factor for the next composition: the
    /// Overworld's surface darkens at night; cave views and
    /// other dimensions keep 1.0.
    fn day_factor(&self, dimension: &str) -> f32 {
        let cave_display = self.settings.mode == 1
            || self.settings.mode == 2
            || (self.settings.mode == 0 && self.feed_cave);
        if dimension != "minecraft:overworld" || cave_display {
            return 1.0;
        }
        match game::get_clock() {
            Some(clock) => daylight(clock.day_time),
            None => 1.0,
        }
    }

    /// The store's height of a column, for placing a waypoint
    /// made from a map click: the surface tile's scanned
    /// height, else the cave tile's.
    fn ground_y(&self, x: i32, z: i32) -> Option<i32> {
        let chunk = (x >> 4, z >> 4);
        if let Some(t) = self.store.surface.get(&chunk) {
            return Some(i32::from(t.column(x & 15, z & 15).1));
        }
        self.store
            .cave
            .get(&chunk)
            .and_then(|cuts| cuts.first())
            .map(|t| i32::from(t.column(x & 15, z & 15).1))
    }

    /// Composes and pushes the minimap image for the player
    /// at (px, pz) facing `yaw`.
    fn push_minimap(&mut self, px: f64, pz: f64, yaw: f32, player_y: f64, dimension: &str) {
        if !self.settings.minimap {
            map::set_minimap_image(None);
            return;
        }
        let size = SIZES[self.settings.size];
        let bpp = ZOOMS[self.settings.zoom];
        let mut dots: Vec<(f64, f64, u32, bool)> = Vec::new();
        for w in &self.waypoints {
            if w.set == self.active_set && !w.hidden {
                dots.push((
                    f64::from(w.x) + 0.5,
                    f64::from(w.z) + 0.5,
                    WP_COLORS[w.color].1,
                    w.death,
                ));
            }
        }
        if self.settings.entity_dots {
            for e in entity::entities() {
                let kind = e.kind.rsplit(':').next().unwrap_or(&e.kind);
                dots.push((e.x, e.z, entity_dot(kind, e.y - player_y), false));
            }
        }
        let overlays = Overlays {
            grid: false,
            slime_seed: None,
            dots: &dots,
        };
        let mut image = compose(
            &self.store,
            px,
            pz,
            bpp,
            size,
            size,
            self.settings.mode as u8,
            self.feed_cave,
            player_y as i32 + 1,
            LAYER_YS[self.settings.layer_y],
            self.day_factor(dimension),
            if self.settings.rotate {
                Some(yaw)
            } else {
                None
            },
            &overlays,
        );
        if self.settings.round {
            let half = size as i32 / 2;
            for j in 0..size as i32 {
                for i in 0..size as i32 {
                    let (dx, dy) = (i - half, j - half);
                    if dx * dx + dy * dy > half * half {
                        image.pixels[(j * size as i32 + i) as usize] = 0;
                    }
                }
            }
        }
        // The player: a white dot with a dark rim, and its
        // nose the way they face (up on a turning map).
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
            round: self.settings.round,
            image,
        }));
    }

    /// Composes and pushes the map screen at the current
    /// view, with the active set's waypoints as its marks.
    fn push_map_screen(&mut self, player: (f64, f64, f64), dimension: &str) {
        let (cx, cz, bpp) = self.view;
        let overlays = Overlays {
            grid: self.settings.grid,
            slime_seed: if self.settings.slime { self.seed } else { None },
            dots: &[],
        };
        let image = compose(
            &self.store,
            cx,
            cz,
            bpp,
            MAP_W,
            MAP_H,
            self.settings.mode as u8,
            self.feed_cave,
            player.1 as i32 + 1,
            LAYER_YS[self.settings.layer_y],
            self.day_factor(dimension),
            None,
            &overlays,
        );
        let mut marks: Vec<&Waypoint> = self
            .waypoints
            .iter()
            .filter(|w| w.set == self.active_set && !w.hidden)
            .collect();
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

    /// Opens the map screen centred on the player. (The
    /// editor's return path reopens it at the kept view by
    /// calling `push_map_screen` instead.)
    fn open_map(&mut self, player: (f64, f64, f64), dimension: &str) {
        self.view = (player.0, player.2, 1.0);
        self.push_map_screen(player, dimension);
        self.map_open = true;
    }
}

// ----------------------------------------------------------------
// Screens: settings, the waypoint list, the editor.

fn button(id: u32, label: String) -> Widget {
    Widget::Button(ButtonWidget { id, label })
}

fn on_off(b: bool) -> &'static str {
    if b { "On" } else { "Off" }
}

/// The settings page and screen: one cycling button per
/// setting, in the order the original's pages present them.
fn settings_widgets(s: &Settings) -> Vec<Widget> {
    vec![
        button(0, format!("Minimap: {}", on_off(s.minimap))),
        button(1, format!("Minimap Corner: {}", CORNER_NAMES[s.corner])),
        button(2, format!("Minimap Size: {}", SIZES[s.size])),
        button(
            3,
            format!(
                "Minimap Shape: {}",
                if s.round { "Round" } else { "Square" }
            ),
        ),
        button(4, format!("Minimap Rotation: {}", on_off(s.rotate))),
        button(5, format!("Minimap Zoom: {}", ZOOM_NAMES[s.zoom])),
        button(6, format!("Map Mode: {}", MODES[s.mode])),
        button(7, format!("Layer Y: {}", LAYER_YS[s.layer_y])),
        button(8, format!("Chunk Grid: {}", on_off(s.grid))),
        button(9, format!("Slime Chunks: {}", on_off(s.slime))),
        button(10, format!("Entity Dots: {}", on_off(s.entity_dots))),
        button(11, format!("Deathpoints: {}", on_off(s.deathpoints))),
        button(12, format!("Waypoint Labels: {}", on_off(s.labels))),
    ]
}

impl Atlas {
    /// The visible waypoints' indices into `self.waypoints`,
    /// in storage order.
    fn visible_indices(&self) -> Vec<usize> {
        self.waypoints
            .iter()
            .enumerate()
            .filter(|(_, w)| self.list_all || w.set == self.active_set)
            .map(|(i, _)| i)
            .collect()
    }

    fn open_settings_screen(&mut self) {
        self.open_screen = OpenScreen::Settings;
        ui::open_screen(&ui::Screen {
            title: "Atlas".to_string(),
            widgets: settings_widgets(&self.settings),
        });
    }

    fn open_list(&mut self) {
        let Some(player) = game::get_player() else {
            return;
        };
        let p = (player.x, player.y, player.z);
        let rows: Vec<String> = self
            .visible_indices()
            .iter()
            .map(|&i| {
                let w = &self.waypoints[i];
                format!("{} — {:.0}m", w.name, distance(p, w))
            })
            .collect();
        let set_label = if self.list_all {
            "All Sets".to_string()
        } else {
            self.active_set.clone()
        };
        self.open_screen = OpenScreen::List;
        ui::open_screen(&ui::Screen {
            title: "Waypoints".to_string(),
            widgets: vec![
                Widget::List(ListWidget {
                    id: LIST_WIDGET,
                    rows,
                    selected: self.selected.map(|s| s as u32),
                }),
                button(LIST_SET_BUTTON, format!("Set: {set_label}")),
                button(LIST_NEW_BUTTON, "New Waypoint".to_string()),
                button(LIST_TP_BUTTON, "Teleport".to_string()),
                button(LIST_SHARE_BUTTON, "Share".to_string()),
                button(LIST_DELETE_BUTTON, "Delete".to_string()),
            ],
        });
    }

    fn open_editor(&mut self, x: i32, y: i32, z: i32, from_map: bool) {
        self.editor = Some(Editor {
            name: format!("Waypoint {}", self.waypoints.len() + 1),
            x,
            y,
            z,
            color: self.waypoints.len() % 16,
            set: self.active_set.clone(),
            temporary: false,
            from_map,
        });
        self.open_screen = OpenScreen::Editor;
        self.show_editor();
    }

    fn show_editor(&self) {
        let Some(e) = &self.editor else { return };
        ui::open_screen(&ui::Screen {
            title: "New Waypoint".to_string(),
            widgets: vec![
                Widget::Text(TextWidget {
                    id: 0,
                    text: format!("{} at {}, {}, {}", e.name, e.x, e.y, e.z),
                    color: 0,
                }),
                button(
                    EDITOR_COLOR_BUTTON,
                    format!("Colour: {}", WP_COLORS[e.color].0),
                ),
                button(EDITOR_SET_BUTTON, format!("Set: {}", e.set)),
                button(
                    EDITOR_TEMP_BUTTON,
                    format!("Temporary: {}", on_off(e.temporary)),
                ),
                button(EDITOR_CREATE_BUTTON, "Create Waypoint".to_string()),
                button(EDITOR_CANCEL_BUTTON, "Cancel".to_string()),
            ],
        });
    }

    /// After the editor: back to the map it came from, or
    /// closed.
    fn leave_editor(&mut self) {
        let from_map = self.editor.take().is_some_and(|e| e.from_map);
        self.open_screen = OpenScreen::None;
        if from_map {
            if let Some(player) = game::get_player() {
                let dim = player.dimension.clone();
                self.push_map_screen((player.x, player.y, player.z), &dim);
                self.map_open = true;
            }
        } else {
            ui::close_screen();
        }
    }

    /// Cycles one settings button, saves, and refreshes the
    /// settings surfaces.
    fn cycle_setting(&mut self, id: u32) {
        let s = &mut self.settings;
        match id {
            0 => s.minimap = !s.minimap,
            1 => s.corner = (s.corner + 1) % CORNERS.len(),
            2 => s.size = (s.size + 1) % SIZES.len(),
            3 => s.round = !s.round,
            4 => s.rotate = !s.rotate,
            5 => s.zoom = (s.zoom + 1) % ZOOMS.len(),
            6 => s.mode = (s.mode + 1) % MODES.len(),
            7 => s.layer_y = (s.layer_y + 1) % LAYER_YS.len(),
            8 => s.grid = !s.grid,
            9 => s.slime = !s.slime,
            10 => s.entity_dots = !s.entity_dots,
            11 => s.deathpoints = !s.deathpoints,
            12 => s.labels = !s.labels,
            _ => return,
        }
        self.save_settings();
        ui::set_settings(&settings_widgets(&self.settings));
        if self.open_screen == OpenScreen::Settings {
            self.open_settings_screen();
        }
        // An open map recomposes with the new overlay/mode.
        if self.map_open {
            self.shown_gen = self.map_gen.wrapping_sub(1);
        }
    }

    /// `/atlas set <name> <value>`: the settings by name, as
    /// the original's command sets exact values.
    fn set_by_name(&mut self, name: &str, value: &str) -> Result<String, String> {
        let boolv = |v: &str| -> Option<bool> {
            match v {
                "on" | "true" | "1" => Some(true),
                "off" | "false" | "0" => Some(false),
                _ => None,
            }
        };
        let s = &mut self.settings;
        let shown = match name {
            "minimap" => {
                s.minimap = boolv(value).ok_or("on or off")?;
                format!("Minimap: {}", on_off(s.minimap))
            }
            "corner" => {
                s.corner = CORNER_NAMES
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(value))
                    .or_else(|| value.parse::<usize>().ok().filter(|n| *n < 4))
                    .ok_or("Top Right, Top Left, Bottom Right or Bottom Left")?;
                format!("Minimap Corner: {}", CORNER_NAMES[s.corner])
            }
            "size" => {
                let v: u32 = value.parse().map_err(|_| "100, 128, 160 or 200")?;
                s.size = SIZES
                    .iter()
                    .position(|z| *z == v)
                    .ok_or("100, 128, 160 or 200")?;
                format!("Minimap Size: {}", SIZES[s.size])
            }
            "round" => {
                s.round = boolv(value).ok_or("on or off")?;
                format!("Minimap Shape: {}", if s.round { "Round" } else { "Square" })
            }
            "rotate" => {
                s.rotate = boolv(value).ok_or("on or off")?;
                format!("Minimap Rotation: {}", on_off(s.rotate))
            }
            "zoom" => {
                s.zoom = ZOOM_NAMES
                    .iter()
                    .position(|n| *n == value)
                    .ok_or("2x, 1x or 0.5x")?;
                format!("Minimap Zoom: {}", ZOOM_NAMES[s.zoom])
            }
            "mode" => {
                s.mode = MODES
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(value))
                    .ok_or("Auto, Cave or Layer")?;
                format!("Map Mode: {}", MODES[s.mode])
            }
            "layer" => {
                let v: i32 = value.parse().map_err(|_| "a height, -64 to 320")?;
                s.layer_y = LAYER_YS
                    .iter()
                    .position(|y| *y == v)
                    .ok_or("one of -64, 0, 32, 64, 96, 128, 192, 256, 320")?;
                format!("Layer Y: {}", LAYER_YS[s.layer_y])
            }
            "grid" => {
                s.grid = boolv(value).ok_or("on or off")?;
                format!("Chunk Grid: {}", on_off(s.grid))
            }
            "slime" => {
                s.slime = boolv(value).ok_or("on or off")?;
                format!("Slime Chunks: {}", on_off(s.slime))
            }
            "dots" => {
                s.entity_dots = boolv(value).ok_or("on or off")?;
                format!("Entity Dots: {}", on_off(s.entity_dots))
            }
            "deathpoints" => {
                s.deathpoints = boolv(value).ok_or("on or off")?;
                format!("Deathpoints: {}", on_off(s.deathpoints))
            }
            "labels" => {
                s.labels = boolv(value).ok_or("on or off")?;
                format!("Waypoint Labels: {}", on_off(s.labels))
            }
            _ => return Err(format!("Atlas: no setting called {name}")),
        };
        self.save_settings();
        Ok(format!("Atlas: {shown}"))
    }
}

// ----------------------------------------------------------------
// The Mod impl.

impl Atlas {
    /// The waypoint the list's selection names, if it still
    /// exists.
    fn selected_waypoint(&self) -> Option<Waypoint> {
        let row = self.selected?;
        let idxs = self.visible_indices();
        idxs.get(row).map(|&i| self.waypoints[i].clone())
    }
}

impl Mod for Atlas {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(MAP_KEY, "Open World Map", "key.keyboard.m")?;
        input::register_key(LIST_KEY, "Waypoint List", "key.keyboard.u")?;
        input::register_key(NEW_KEY, "New Waypoint", "key.keyboard.b")?;
        input::register_key(LABELS_KEY, "Toggle Waypoint Labels", "key.keyboard.h")?;
        input::register_key(SETTINGS_KEY, "Atlas Settings", "key.keyboard.y")?;
        let settings = storage::get("settings")
            .and_then(|b| String::from_utf8(b).ok())
            .map(|t| Settings::parse(&t))
            .unwrap_or_default();
        Ok((
            Atlas {
                settings,
                ctx: None,
                store: TileStore::default(),
                known: HashMap::new(),
                verified: HashSet::new(),
                fetch: VecDeque::new(),
                queued: HashSet::new(),
                regions: HashSet::new(),
                dirty: HashSet::new(),
                seed: None,
                store_full: false,
                feed_cave: false,
                waypoints: Vec::new(),
                active_set: "Default".to_string(),
                waypoints_dirty: false,
                map_open: false,
                view: (0.0, 0.0, 1.0),
                map_image: (0.0, 0.0, 1.0),
                map_gen: 0,
                shown_gen: 0,
                open_screen: OpenScreen::None,
                selected: None,
                list_all: false,
                editor: None,
                pending_share: None,
                last_labels: String::new(),
            },
            Registration::new()
                .on_tick()
                .on_frame()
                .on_chat()
                .command("atlas"),
        ))
    }

    fn tick(&mut self, tick: u64) {
        // Context: the world and dimension the player is in.
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
        // Keys.
        for event in input::poll() {
            let Event::KeyDown(k) = event else { continue };
            match k.as_str() {
                MAP_KEY => {
                    if let Some(p) = game::get_player() {
                        let dim = p.dimension.clone();
                        self.open_map((p.x, p.y, p.z), &dim);
                    }
                }
                LIST_KEY => self.open_list(),
                NEW_KEY => {
                    if let Some(p) = game::get_player() {
                        self.open_editor(
                            p.x.floor() as i32,
                            p.y.floor() as i32,
                            p.z.floor() as i32,
                            false,
                        );
                    }
                }
                LABELS_KEY => {
                    self.settings.labels = !self.settings.labels;
                    self.save_settings();
                }
                SETTINGS_KEY => self.open_settings_screen(),
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
        // Deaths become deathpoints.
        for event in map::poll_events() {
            if let horizon_mod_sdk::PositionEvent::Died(pos) = event
                && self.settings.deathpoints
            {
                self.add_deathpoint(pos.x, pos.y, pos.z, true);
            }
        }
        // The tile feed and its persistence.
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
        if self.waypoints_dirty {
            if let Some(ctx) = &self.ctx {
                let hash = ctx.hash.clone();
                let _ = storage::set(
                    &waypoints_key(&hash),
                    &encode_waypoints(&self.active_set, &self.waypoints),
                );
            }
            self.waypoints_dirty = false;
        }
        // Temporary waypoints delete themselves on arrival.
        if tick % 20 == 0 {
            let before = self.waypoints.len();
            self.waypoints
                .retain(|w| !w.temporary || distance(ppos, w) > 5.0);
            if self.waypoints.len() != before {
                self.waypoints_dirty = true;
            }
        }
        // The map screen's events.
        for event in map::poll_map() {
            match event {
                horizon_mod_sdk::MapEvent::Clicked(c) => {
                    if !self.map_open {
                        continue;
                    }
                    let (x, z) = (c.x.floor() as i32, c.z.floor() as i32);
                    if c.button == 0 {
                        let y = self.ground_y(x, z).unwrap_or(player.y.floor() as i32);
                        self.open_editor(x, y, z, true);
                    } else if c.button == 1 {
                        let y = self.ground_y(x, z).unwrap_or(player.y.floor() as i32);
                        let biome = world::get_biome(&dim, x, y + 1, z);
                        let name = biome
                            .map(|b| {
                                format!(" — {}", b.rsplit(':').next().unwrap_or(&b))
                            })
                            .unwrap_or_default();
                        chat::show(&format!("Atlas: {x}, {z}{name}"));
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
        // The minimap.
        if tick % 5 == 0 {
            self.push_minimap(player.x, player.z, player.yaw, player.y, &dim);
        }
        // An open map recomposes when its view drifts from
        // the pushed picture, its zoom runs away from it, or
        // fresh ground lands under it.
        if self.map_open && tick % 20 == 0 {
            let (vx, vz, vb) = self.view;
            let (ix, iz, ib) = self.map_image;
            let extent = f64::from(ib) * f64::from(MAP_W) / 2.0;
            let drifted =
                (vx - ix).abs() > extent * 0.25 || (vz - iz).abs() > extent * 0.25;
            let zoomed = vb / ib > 3.0 || vb / ib < 0.34;
            if drifted || zoomed || self.shown_gen != self.map_gen {
                self.push_map_screen(ppos, &dim);
            }
        }
    }

    fn frame(&mut self, _seconds: f32) {
        // In-world waypoint labels: the name and the distance,
        // projected over the waypoint, as the original draws
        // them.
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
            .waypoints
            .iter()
            .filter(|w| w.set == self.active_set && !w.hidden)
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
        let args = args.trim();
        if args.is_empty() {
            return Ok(Some(
                "Atlas: /atlas seed <seed> · /atlas set <setting> <value> · /atlas rename <old> | <new> · /atlas addshared"
                    .to_string(),
            ));
        }
        let (sub, rest) = match args.split_once(' ') {
            Some((a, b)) => (a, b.trim()),
            None => (args, ""),
        };
        match sub {
            "seed" => {
                let Some(ctx) = &self.ctx else {
                    return Err("Atlas: not in a world".to_string());
                };
                let hash = ctx.hash.clone();
                if rest.is_empty() {
                    Ok(Some(match self.seed {
                        Some(s) => format!("Atlas: seed {s} for this world"),
                        None => {
                            "Atlas: no seed set for this world — /atlas seed <seed>".to_string()
                        }
                    }))
                } else if rest == "clear" {
                    self.seed = None;
                    self.save_index(&hash);
                    Ok(Some("Atlas: seed cleared".to_string()))
                } else {
                    match rest.parse::<i64>() {
                        Ok(v) => {
                            self.seed = Some(v);
                            self.save_index(&hash);
                            Ok(Some(format!("Atlas: seed set to {v} for this world")))
                        }
                        Err(_) => Err("Atlas: the seed is a whole number".to_string()),
                    }
                }
            }
            "set" => {
                let Some((k, v)) = rest.split_once(' ') else {
                    return Err("Atlas: /atlas set <setting> <value>".to_string());
                };
                match self.set_by_name(k, v.trim()) {
                    Ok(msg) => Ok(Some(msg)),
                    Err(hint) => Err(format!("Atlas: {hint}")),
                }
            }
            "rename" => {
                let (old, new) = if let Some((o, n)) = rest.split_once('|') {
                    (o.trim(), n.trim())
                } else if let Some(stripped) = rest.strip_prefix('"') {
                    match stripped.split_once('"') {
                        Some((o, n)) => (o, n.trim()),
                        None => return Err("Atlas: /atlas rename <old> | <new>".to_string()),
                    }
                } else {
                    match rest.split_once(' ') {
                        Some((o, n)) => (o, n.trim()),
                        None => return Err("Atlas: /atlas rename <old> | <new>".to_string()),
                    }
                };
                let new = clean_name(new);
                if new.is_empty() {
                    return Err("Atlas: the new name is empty".to_string());
                }
                match self.waypoints.iter_mut().find(|w| w.name == old) {
                    Some(w) => {
                        let old = w.name.clone();
                        w.name = new.clone();
                        self.waypoints_dirty = true;
                        Ok(Some(format!("Atlas: '{old}' is now '{new}'")))
                    }
                    None => Err(format!("Atlas: no waypoint called '{old}' here")),
                }
            }
            "addshared" => {
                let Some((mut w, dim)) = self.pending_share.take() else {
                    return Err("Atlas: no shared waypoint is waiting".to_string());
                };
                let Some(ctx) = &self.ctx else {
                    return Err("Atlas: not in a world".to_string());
                };
                let world_name = ctx.world.clone();
                let here = ctx.dimension.clone();
                let name = w.name.clone();
                if dim == here {
                    w.set = self.active_set.clone();
                    self.waypoints.push(w);
                    self.waypoints_dirty = true;
                } else {
                    let h = context_hash(&world_name, &dim);
                    let (active, mut wps) = storage::get(&waypoints_key(&h))
                        .map(|d| decode_waypoints(&d))
                        .unwrap_or_else(|| ("Default".to_string(), Vec::new()));
                    w.set = active.clone();
                    wps.push(w);
                    let _ = storage::set(&waypoints_key(&h), &encode_waypoints(&active, &wps));
                }
                Ok(Some(format!("Atlas: added shared waypoint '{name}'")))
            }
            _ => Err("Atlas: unknown — /atlas for help".to_string()),
        }
    }

    fn chat(&mut self, text: &str) {
        if let Some(w) = parse_share(text)
            && let Some(dim) = share_dimension(text)
        {
            chat::show(&format!(
                "Atlas: shared waypoint '{}' at {}, {}, {} — /atlas addshared to add it",
                w.name, w.x, w.y, w.z
            ));
            let _ = dim;
            self.pending_share = Some((w, dim));
        }
    }

    fn ui_settings_opened(&mut self) {
        ui::set_settings(&settings_widgets(&self.settings));
    }

    fn ui_closed(&mut self) {
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
            LIST_SET_BUTTON => {
                let names = set_names(&self.active_set, &self.waypoints);
                // Stops: each set, then All.
                let stops: Vec<Option<String>> =
                    names.iter().cloned().map(Some).chain([None]).collect();
                let cur = if self.list_all {
                    stops.len() - 1
                } else {
                    stops
                        .iter()
                        .position(|s| s.as_deref() == Some(self.active_set.as_str()))
                        .unwrap_or(0)
                };
                match &stops[(cur + 1) % stops.len()] {
                    Some(name) => {
                        self.list_all = false;
                        if *name != self.active_set {
                            self.active_set = name.clone();
                            self.waypoints_dirty = true;
                        }
                    }
                    None => {
                        self.list_all = true;
                    }
                }
                self.selected = None;
                self.open_list();
            }
            LIST_NEW_BUTTON => {
                if let Some(p) = game::get_player() {
                    self.open_editor(
                        p.x.floor() as i32,
                        p.y.floor() as i32,
                        p.z.floor() as i32,
                        false,
                    );
                }
            }
            LIST_TP_BUTTON => {
                if let Some(w) = self.selected_waypoint() {
                    command::run(&format!("tp @s {} {} {}", w.x, w.y, w.z));
                    ui::close_screen();
                    self.open_screen = OpenScreen::None;
                }
            }
            LIST_SHARE_BUTTON => {
                if let Some(w) = self.selected_waypoint() {
                    if let Some(ctx) = &self.ctx {
                        let line = share_line(&w, &ctx.dimension);
                        command::run(&format!("say {line}"));
                    }
                }
            }
            LIST_DELETE_BUTTON => {
                if let Some(row) = self.selected {
                    let idxs = self.visible_indices();
                    if let Some(&real) = idxs.get(row) {
                        self.waypoints.remove(real);
                        self.waypoints_dirty = true;
                        self.selected = None;
                        self.open_list();
                    }
                }
            }
            EDITOR_COLOR_BUTTON => {
                if let Some(e) = &mut self.editor {
                    e.color = (e.color + 1) % 16;
                }
                self.show_editor();
            }
            EDITOR_SET_BUTTON => {
                let names = set_names(&self.active_set, &self.waypoints);
                if let Some(e) = &mut self.editor {
                    let cur = names.iter().position(|n| *n == e.set).unwrap_or(0);
                    e.set = names[(cur + 1) % names.len()].clone();
                }
                self.show_editor();
            }
            EDITOR_TEMP_BUTTON => {
                if let Some(e) = &mut self.editor {
                    e.temporary = !e.temporary;
                }
                self.show_editor();
            }
            EDITOR_CREATE_BUTTON => {
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
                    set: e.set.clone(),
                    hidden: false,
                    temporary: e.temporary,
                    death: false,
                };
                self.waypoints.push(w);
                self.waypoints_dirty = true;
                self.leave_editor();
            }
            EDITOR_CANCEL_BUTTON => {
                self.leave_editor();
            }
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(Atlas);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_level_is_25() {
        assert_eq!(API_LEVEL, 25);
    }

    #[test]
    fn shading_is_the_map_items_rule() {
        // A wall reads brightest (brightness 3, modifier 135).
        assert_eq!(brightness(7 | WALL, 64, Some(70), 0, 0), 3);
        // Higher than the north neighbour: brightness 2, the
        // full colour (id 2 = 16247203 = 0xF7EBC3... checked
        // through shade below).
        assert_eq!(brightness(2, 70, Some(60), 0, 0), 2);
        // Lower: brightness 0.
        assert_eq!(brightness(2, 50, Some(60), 0, 0), 0);
        // Level: brightness 1 (parity 0 gives the -0.2 dither,
        // inside the level band).
        assert_eq!(brightness(2, 60, Some(60), 0, 0), 1);
        // Water: shallow reads 2, deep reads 0.
        assert_eq!(brightness(WATER, 60, None, 0, 0), 2);
        assert_eq!(brightness(WATER, 60, None, 15, 0), 0);
        // Shade at brightness 2 is the unmodified colour with
        // full alpha: MAP_COLORS[2] = 16247203 = 0x00F7E9A3.
        assert_eq!(shade(2, 70, Some(60), 0, 4, 4, 1.0), 0xFFF7_E9A3);
        // None colour and unknown ids read as explored-less.
        assert_eq!(shade(0, 70, Some(60), 0, 0, 0, 1.0), UNKNOWN);
        // Night scales the channels, not the alpha.
        let night = shade(2, 70, Some(60), 0, 4, 4, 0.5);
        assert_eq!(night >> 24, 0xFF);
        assert!(night & 0xFF < 0xC3);
    }

    #[test]
    fn daylight_curve() {
        assert!((daylight(6000) - 1.0).abs() < 1e-6);
        assert!((daylight(18000) - 0.35).abs() < 1e-6);
    }

    #[test]
    fn rle_roundtrips() {
        let data: Vec<u8> = (0..256).map(|i| (i / 7) as u8).collect();
        let enc = rle_encode(&data);
        assert_eq!(rle_decode(&enc, 256).unwrap(), data);
        assert!(rle_decode(&enc, 255).is_none());
        let flat = vec![9u8; 256];
        assert_eq!(rle_encode(&flat).len(), 4);
    }

    #[test]
    fn region_codec_roundtrips() {
        let mut tile = Tile {
            layer: MapLayer::Cave,
            cut_y: Some(40),
            version: 7,
            colors: [0; 256],
            heights: [0; 256],
            depths: [0; 256],
        };
        for i in 0..256 {
            tile.colors[i] = (i % 13) as u8;
            tile.heights[i] = 60 + (i as i16 % 5);
            tile.depths[i] = (i % 3) as u8;
        }
        let blob = encode_region(&[(37, &tile)]);
        let back = decode_region(2, -1, &blob);
        assert_eq!(back.len(), 1);
        let (chunk, t) = &back[0];
        assert_eq!(*chunk, (2 * 8 + 5, -8 + 4));
        assert_eq!(t.layer, MapLayer::Cave);
        assert_eq!(t.cut_y, Some(40));
        assert_eq!(t.version, 7);
        assert_eq!(t.colors, tile.colors);
        assert_eq!(t.heights, tile.heights);
        assert_eq!(t.depths, tile.depths);
        // Garbage reads as an empty region, never a panic.
        assert!(decode_region(0, 0, &[1, 2, 3]).is_empty());
    }

    #[test]
    fn waypoint_blob_roundtrips() {
        let wps = vec![
            Waypoint {
                name: "Home Base".into(),
                x: 100,
                y: 64,
                z: -200,
                color: 11,
                set: "Default".into(),
                hidden: false,
                temporary: false,
                death: false,
            },
            Waypoint {
                name: "Deathpoint".into(),
                x: 1,
                y: 2,
                z: 3,
                color: RED,
                set: "Trips".into(),
                hidden: true,
                temporary: true,
                death: true,
            },
        ];
        let (active, back) = decode_waypoints(&encode_waypoints("Trips", &wps));
        assert_eq!(active, "Trips");
        assert_eq!(back, wps);
        assert_eq!(set_names("Trips", &wps), ["Default", "Trips"]);
    }

    #[test]
    fn settings_roundtrip() {
        let mut s = Settings::default();
        s.minimap = false;
        s.mode = 2;
        s.layer_y = 5;
        let back = Settings::parse(std::str::from_utf8(&s.encode()).unwrap());
        assert!(!back.minimap);
        assert_eq!(back.mode, 2);
        assert_eq!(back.layer_y, 5);
        assert!(back.rotate);
    }

    #[test]
    fn java_random_matches_java() {
        // Known values: new Random(42).nextInt() and
        // new Random(0).nextInt() in Java.
        assert_eq!(JavaRandom::new(42).next(32) as i32, -1170105035);
        assert_eq!(JavaRandom::new(0).next(32) as i32, -1155484576);
        // About a tenth of chunks are slime chunks.
        let mut n = 0;
        for cx in -50..50 {
            for cz in -50..50 {
                if slime_chunk(12345, cx, cz) {
                    n += 1;
                }
            }
        }
        assert!((700..=1300).contains(&n), "slime count {n}");
    }

    #[test]
    fn share_lines_roundtrip() {
        let w = Waypoint {
            name: "Home".into(),
            x: 12,
            y: 70,
            z: -34,
            color: 5,
            set: "Default".into(),
            hidden: false,
            temporary: false,
            death: false,
        };
        let line = format!("<Steve> {}", share_line(&w, "minecraft:overworld"));
        let back = parse_share(&line).unwrap();
        assert_eq!(back.name, "Home");
        assert_eq!((back.x, back.y, back.z, back.color), (12, 70, -34, 5));
        assert_eq!(share_dimension(&line).unwrap(), "minecraft:overworld");
        assert!(parse_share("just chat").is_none());
    }

    #[test]
    fn compose_reads_the_store() {
        let mut store = TileStore::default();
        let mut tile = Tile {
            layer: MapLayer::Surface,
            cut_y: None,
            version: 1,
            colors: [0; 256],
            heights: [64; 256],
            depths: [0; 256],
        };
        tile.colors = [2; 256];
        store.put((0, 0), tile);
        let overlays = Overlays {
            grid: false,
            slime_seed: None,
            dots: &[],
        };
        let img = compose(&store, 8.0, 8.0, 1.0, 16, 16, 0, false, 65, 64, 1.0, None, &overlays);
        // The centre pixel is the tile's colour at brightness
        // 1 (level ground, parity varies): some index whose
        // palette colour is MAP_COLORS[2] shaded by 220.
        let centre = img.pixels[8 * 16 + 8];
        assert_eq!(img.palette[centre as usize], shade(2, 64, Some(64), 0, 8, 8, 1.0));
        // Outside the one stored chunk: the unknown colour.
        let far = compose(&store, 500.0, 500.0, 1.0, 4, 4, 0, false, 65, 64, 1.0, None, &overlays);
        assert!(far.palette.contains(&UNKNOWN));
    }

    #[test]
    fn context_hash_is_stable_and_key_safe() {
        let h = context_hash("My World", "minecraft:overworld");
        assert_eq!(h.len(), 16);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(h, context_hash("My World", "minecraft:the_nether"));
        assert_ne!(h, context_hash("Other", "minecraft:overworld"));
    }
}

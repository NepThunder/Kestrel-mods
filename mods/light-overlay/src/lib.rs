//! Light Overlay for Kestrel: the block tops around the player where
//! hostile mobs can spawn, marked with depth-tested 3D crosses — red
//! where they can spawn at any time, yellow where they can spawn at
//! night, and green where they cannot (off by default). F7 toggles it
//! (rebindable on the mod's page in Options > Mods), `/lightoverlay`
//! does the same, and that page also cycles the scan radius (8, 12, 16
//! or 20 blocks), the safe-blocks switch and the spawn rule (Modern or
//! Legacy); the game keeps the choices.
//!
//! Behaviour reimplemented from Light Overlay 1.0.0 by CayMac (MIT,
//! <https://github.com/CayMac97/light-overlay>); this file shares no
//! code with the original — the original is a native mod, this is a
//! fresh implementation against the Kestrel SDK. The original scans
//! nearby surfaces and draws crosses or 3D light-level numbers on them;
//! the scan here is a batched `world.get-lights` read over the air
//! block above each nearby surface (API level 12), and the markers are
//! `render.set-world` crosses (API level 11). This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/light-overlay>.
//!
//! The spawn verdict is the night rule from `docs/mods.md`: the
//! effective light is `max(block, sky - 11)` at night and
//! `max(block, sky)` by day. Modern (the vanilla rule since 1.18, the
//! original's default) needs block light 0: red when the day value is
//! 7 or less, yellow when only the night value is. Legacy (the
//! original's pre-1.18 rule) drops the block-light-0 requirement and
//! tests the effective value alone. In the Nether the sky is always 0
//! and there is no day or night, so red is block light 7 or less and
//! there is no yellow. In the End light does not gate endermen, so
//! nothing is marked there.
//!
//! A scan runs at 2 Hz and is spread across ticks, so no one tick
//! carries it whole: one tick probes the cached surfaces, then up to
//! two four-level bands a tick walk the rest top-down, and the light
//! read and the drawing happen when the bands finish. A cold scan
//! over flat ground takes about seven ticks; the markers appear when
//! it finishes. The window is 16 blocks above to 32 below the
//! player's feet, so a surface outside it is not marked until the
//! player moves nearer to its level; and a surface is only marked
//! when the two blocks above it are air, as a mob needs head room —
//! a surface under leaves, a torch or tall grass gets no cross. A
//! column with no surface (open water, the void, ground below the
//! window) is remembered and only re-scanned every sixth rescan.
//!
//! What the SDK cannot do, the original's native build can: the light
//! level as a number on the block top — there is no world-text
//! capability yet, so this port draws colour-coded crosses only, and
//! the counts live in the corner panel instead. In multiplayer every
//! world read is empty in v1, so nothing draws there.
use horizon_mod_sdk::{
    Mod, Registration, chat, game,
    hud::{self, Corner, Line},
    input::{self, Event},
    render::{self, WorldLine, WorldPoint, WorldShape},
    storage,
    ui::{self, ButtonWidget, Widget},
    world::{self, BlockPos},
};
use std::collections::{HashMap, HashSet};

/// The newest API level this mod needs, asserted in the tests below:
/// `world.get-lights` is level 12, `render.set-world` is level 11,
/// the rest level 0 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 12;

/// The key's name for `register-key`, and its default: F7, as the
/// original binds it.
const KEY: &str = "toggle";
const DEFAULT_KEY: &str = "key.keyboard.f7";

/// How often a scan starts: every 10 ticks is 2 Hz, as the light
/// spike suggests (2–4 Hz, not every tick).
const RESCAN_TICKS: u64 = 10;
/// Bands of the top-down scan a rescan walks at most: 12 bands of 4
/// levels cover the whole window (16 above to 32 below the feet).
const MAX_BANDS_PER_SCAN: usize = 12;
/// Bands walked per tick while a scan runs: two bands are about 6400
/// block reads at radius 16, well inside one tick's fuel; a cold scan
/// over flat ground finishes in about seven ticks.
const BANDS_PER_TICK: usize = 2;
/// Levels per band call: 4 levels times the columns in a radius-20
/// circle (1257) is 5028 positions, split into two calls under the
/// 4096 cap by the batching below.
const BAND_LEVELS: i32 = 4;
/// The scan window, relative to the player's feet block: surfaces up
/// to 16 above and 32 below it are found.
const SCAN_ABOVE: i32 = 16;
const SCAN_BELOW: i32 = 32;
/// The most a `get-blocks` / `get-lights` call takes, as the WIT says.
const POSITIONS_PER_CALL: usize = 4096;
/// The per-mod world-line budget (`render.set-world`): a cross is two
/// lines, so at most 512 crosses draw, red first, then yellow, then
/// green.
const MAX_CROSSES: usize = 512;
/// The cross floats this far above the surface block's top face, so it
/// never z-fights with it.
const CROSS_LIFT: f64 = 0.02;
/// How far the cross's ends sit in from the block's edges.
const CROSS_INSET: f64 = 0.1;
/// The cross's width in pixels.
const CROSS_WIDTH: f32 = 2.0;
/// Red: can spawn at any time. Yellow: can spawn at night. Green: safe.
const RED: u32 = 0xFFFF_0000;
const YELLOW: u32 = 0xFFFF_FF00;
const GREEN: u32 = 0xFF00_FF00;

/// The storage keys for the four settings.
const ENABLED_KEY: &str = "enabled";
const RADIUS_KEY: &str = "radius";
const SAFE_KEY: &str = "safe";
const RULE_KEY: &str = "rule";

/// The settings page's buttons: show the overlay, scan radius, safe
/// blocks, spawn rule.
const ENABLED_BUTTON: u32 = 0;
const RADIUS_BUTTON: u32 = 1;
const SAFE_BUTTON: u32 = 2;
const RULE_BUTTON: u32 = 3;

/// The radii the switch cycles through, in blocks; 16 is the
/// original's middle and the spike's example.
const RADII: [i32; 4] = [8, 12, 16, 20];

/// Blocks that are not a surface to stand on: air, fluids, plants,
/// torches, rails, carpets and the rest a mob cannot spawn on top of.
/// Anything else counts as solid — leaves and glass included, as
/// vanilla spawns on them. A bottom slab is not solid either (its
/// state says `type=bottom`); a top or double slab is.
const NON_SOLID: &[&str] = &[
    "short_grass",
    "tall_grass",
    "grass",
    "fern",
    "large_fern",
    "dead_bush",
    "dandelion",
    "poppy",
    "blue_orchid",
    "allium",
    "azure_bluet",
    "red_tulip",
    "orange_tulip",
    "white_tulip",
    "pink_tulip",
    "oxeye_daisy",
    "cornflower",
    "lily_of_the_valley",
    "wither_rose",
    "sunflower",
    "lilac",
    "rose_bush",
    "peony",
    "torchflower",
    "pitcher_plant",
    "pink_petals",
    "torch",
    "wall_torch",
    "soul_torch",
    "soul_wall_torch",
    "redstone_torch",
    "redstone_wall_torch",
    "snow",
    "powder_snow",
    "rail",
    "powered_rail",
    "detector_rail",
    "activator_rail",
    "ladder",
    "vine",
    "sugar_cane",
    "wheat",
    "carrots",
    "potatoes",
    "beetroots",
    "nether_wart",
    "cocoa",
    "sweet_berry_bush",
    "bamboo",
    "cactus",
    "lever",
    "redstone_wire",
    "repeater",
    "comparator",
    "tripwire",
    "tripwire_hook",
    "string",
    "scaffolding",
    "sea_pickle",
    "turtle_egg",
    "flower_pot",
    "torchflower_crop",
    "pitcher_crop",
    "kelp",
    "kelp_plant",
    "seagrass",
    "tall_seagrass",
    "glow_lichen",
    "hanging_roots",
    "moss_carpet",
    "pointed_dripstone",
    "lantern",
    "soul_lantern",
    "chain",
];

/// What the light at a surface says about spawning there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verdict {
    /// Spawnable at any time.
    Red,
    /// Spawnable at night only.
    Yellow,
    /// Not spawnable.
    Safe,
    /// No verdict (the End, where light does not gate spawning).
    None,
}

impl Verdict {
    /// The draw order: red first, then yellow, then green.
    fn rank(self) -> u8 {
        match self {
            Verdict::Red => 0,
            Verdict::Yellow => 1,
            Verdict::Safe => 2,
            Verdict::None => 3,
        }
    }

    fn color(self) -> u32 {
        match self {
            Verdict::Red => RED,
            Verdict::Yellow => YELLOW,
            Verdict::Safe => GREEN,
            Verdict::None => 0,
        }
    }
}

/// One marked surface: the block it stands on, its verdict, and the
/// squared distance from the player's feet to the marker block's
/// centre, for nearest-first drawing.
#[derive(Clone, Copy, Debug)]
struct Marker {
    x: i32,
    surface_y: i32,
    z: i32,
    verdict: Verdict,
    dist2: f64,
}

/// What one block read means for the surface walk: only whether it
/// is air, solid, something else, or unread. The read's state string
/// is classified where it arrives and never stored — a scan reads
/// tens of thousands of states, and keeping the strings would cost
/// a tick's fuel on its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cell {
    Air,
    Solid,
    /// A block that is neither air nor a surface (torch, water, ...).
    Other,
    /// No read (an unloaded chunk, or a level never queried).
    Unknown,
}

fn classify(state: Option<&str>) -> Cell {
    if is_air(state) {
        Cell::Air
    } else if is_solid(state) {
        Cell::Solid
    } else if state.is_some() {
        Cell::Other
    } else {
        Cell::Unknown
    }
}

/// One column's walk down the scan, top-down: the two cells above
/// the next level to check. A level is a surface when it is solid
/// and both cells above it are air.
struct Walk {
    above1: Cell,
    above2: Cell,
}

impl Walk {
    fn new() -> Self {
        Walk {
            above1: Cell::Unknown,
            above2: Cell::Unknown,
        }
    }

    /// Feed the cell at level `y`, walking top-down. Returns the
    /// surface when `y` is solid with air in the two blocks above it.
    fn step(&mut self, y: i32, cell: Cell) -> Option<i32> {
        let found = if cell == Cell::Solid && self.above1 == Cell::Air && self.above2 == Cell::Air {
            Some(y)
        } else {
            None
        };
        self.above2 = self.above1;
        self.above1 = cell;
        found
    }
}

/// A scan in progress, spread across ticks: the probe runs when it
/// starts, then up to [`BANDS_PER_TICK`] bands a tick walk the rest
/// top-down, and the light read and the drawing happen when it
/// finishes.
struct ScanJob {
    dimension: String,
    columns: Vec<(i32, i32)>,
    /// Columns with no surface yet, in scan order.
    dirty: Vec<(i32, i32)>,
    /// One walk per dirty column, carried across bands.
    walks: HashMap<(i32, i32), Walk>,
    band: usize,
    hi: i32,
    lo: i32,
    resolved: HashMap<(i32, i32), i32>,
}

/// The block id of a state string: `minecraft:oak_log[axis=y]` is
/// `oak_log`.
fn base_id(state: &str) -> &str {
    let id = state.split('[').next().unwrap_or(state);
    id.split(':').next_back().unwrap_or(id)
}

/// Air, in any of its three ids. A missing read (`None`, an unloaded
/// chunk) is not air — it is unknown, and an unknown block is never a
/// surface or a free space above one.
fn is_air(state: Option<&str>) -> bool {
    matches!(
        state.map(base_id),
        Some("air") | Some("cave_air") | Some("void_air")
    )
}

/// A block a mob can spawn on top of: not air, not a fluid, not in
/// `NON_SOLID`, not a button, pressure plate or carpet, and not a
/// bottom slab. Unknown (`None`) is not solid.
fn is_solid(state: Option<&str>) -> bool {
    let Some(full) = state else {
        return false;
    };
    let id = base_id(full);
    if matches!(
        id,
        "air" | "cave_air" | "void_air" | "water" | "lava" | "bubble_column"
    ) {
        return false;
    }
    if id.ends_with("_slab") {
        // Only a bottom slab fails: its top is half a block down.
        return !full.contains("type=bottom");
    }
    if id.ends_with("_button") || id.ends_with("_pressure_plate") || id.ends_with("_carpet") {
        return false;
    }
    if id.starts_with("potted_") {
        return false;
    }
    !NON_SOLID.contains(&id)
}

/// The spawn verdict for one light read, in a dimension, under a
/// spawn rule. `legacy` is the original's pre-1.18 rule (the
/// effective light alone); Modern needs block light 0 as well.
fn verdict(block: u8, sky: u8, dimension: &str, legacy: bool) -> Verdict {
    match dimension {
        "minecraft:the_end" => Verdict::None,
        "minecraft:the_nether" => {
            // No sky light and no day or night: the raw brightness is
            // the block light, and 7 or less spawns.
            if block <= 7 {
                Verdict::Red
            } else {
                Verdict::Safe
            }
        }
        _ => {
            let night = i32::from(block).max(i32::from(sky) - 11);
            let day = i32::from(block).max(i32::from(sky));
            if legacy {
                if day <= 7 {
                    Verdict::Red
                } else if night <= 7 {
                    Verdict::Yellow
                } else {
                    Verdict::Safe
                }
            } else if block == 0 && day <= 7 {
                Verdict::Red
            } else if block == 0 && night <= 7 {
                Verdict::Yellow
            } else {
                Verdict::Safe
            }
        }
    }
}

/// The columns in a radius around the feet block, nearest first
/// (ties broken by x, then z, so the order is deterministic).
fn columns_around(fx: i32, fz: i32, radius: i32) -> Vec<(i32, i32)> {
    let mut cols = Vec::new();
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if dx * dx + dz * dz <= radius * radius {
                cols.push((fx + dx, fz + dz));
            }
        }
    }
    cols.sort_by_key(|&(x, z)| ((x - fx).pow(2) + (z - fz).pow(2), x, z));
    cols
}

/// The four levels of one band of the top-down scan: band 0 starts
/// two levels above the highest surface to consider, so the level
/// above every candidate surface is always read too.
fn band_levels(hi: i32, band: usize) -> [i32; 4] {
    let top = hi + 2 - BAND_LEVELS * band as i32;
    [top, top - 1, top - 2, top - 3]
}

/// A cross on a surface block's top face: the two diagonals, inset
/// from the edges and lifted just clear of the block.
fn cross_at(x: i32, surface_y: i32, z: i32, color: u32, width: f32) -> [WorldShape; 2] {
    let y = surface_y as f64 + 1.0 + CROSS_LIFT;
    let (x0, x1) = (x as f64 + CROSS_INSET, x as f64 + 1.0 - CROSS_INSET);
    let (z0, z1) = (z as f64 + CROSS_INSET, z as f64 + 1.0 - CROSS_INSET);
    let line = |ax: f64, az: f64, bx: f64, bz: f64| {
        WorldShape::Line(WorldLine {
            start: WorldPoint { x: ax, y, z: az },
            end: WorldPoint { x: bx, y, z: bz },
            color,
            width,
        })
    };
    [line(x0, z0, x1, z1), line(x1, z0, x0, z1)]
}

/// The shapes to draw for a scan's markers: red first, then yellow,
/// then green (only when safe blocks are shown), each nearest first,
/// inside the 1024-line cap. Returns the shapes and the drawn counts
/// (red, yellow, safe).
fn draw_markers(markers: &[Marker], show_safe: bool) -> (Vec<WorldShape>, usize, usize, usize) {
    let mut ordered: Vec<&Marker> = markers.iter().collect();
    ordered.sort_by(|a, b| {
        a.verdict.rank().cmp(&b.verdict.rank()).then(
            a.dist2
                .partial_cmp(&b.dist2)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    let mut shapes = Vec::new();
    let (mut red, mut yellow, mut safe) = (0, 0, 0);
    for marker in ordered {
        if shapes.len() / 2 >= MAX_CROSSES {
            break;
        }
        match marker.verdict {
            Verdict::None => continue,
            Verdict::Safe if !show_safe => continue,
            Verdict::Red => red += 1,
            Verdict::Yellow => yellow += 1,
            Verdict::Safe => safe += 1,
        }
        shapes.extend(cross_at(
            marker.x,
            marker.surface_y,
            marker.z,
            marker.verdict.color(),
            CROSS_WIDTH,
        ));
    }
    (shapes, red, yellow, safe)
}

/// `get-blocks` in calls of at most 4096 positions, results in
/// request order.
fn get_blocks_batched(dimension: &str, positions: &[BlockPos]) -> Vec<world::BlockResult> {
    let mut out = Vec::with_capacity(positions.len());
    for chunk in positions.chunks(POSITIONS_PER_CALL) {
        out.extend(world::get_blocks(dimension, chunk));
    }
    out
}

/// `get-lights` in calls of at most 4096 positions, results in
/// request order.
fn get_lights_batched(dimension: &str, positions: &[BlockPos]) -> Vec<world::LightResult> {
    let mut out = Vec::with_capacity(positions.len());
    for chunk in positions.chunks(POSITIONS_PER_CALL) {
        out.extend(world::get_lights(dimension, chunk));
    }
    out
}

fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored.map_or(default, |v| v != b"false")
}

/// The stored radius: the stored number when it is one the switch
/// offers, else the default 16.
fn read_radius(stored: Option<Vec<u8>>) -> i32 {
    let wanted = stored
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(RADII[2]);
    if RADII.contains(&wanted) {
        wanted
    } else {
        RADII[2]
    }
}

/// The stored spawn rule: `true` for Legacy, `false` for Modern.
fn read_legacy(stored: Option<Vec<u8>>) -> bool {
    stored.is_some_and(|v| v == b"legacy")
}

/// The next radius in the switch's cycle, wrapping back to the first.
fn next_radius(radius: i32) -> i32 {
    let i = RADII.iter().position(|r| *r == radius).unwrap_or(2);
    RADII[(i + 1) % RADII.len()]
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

struct LightOverlay {
    enabled: bool,
    radius: i32,
    show_safe: bool,
    legacy: bool,
    /// The top solid block per column, from the last scan; a column
    /// whose surface moved or unloaded is re-found by the band scan.
    surfaces: HashMap<(i32, i32), i32>,
    /// Columns the last scan found no surface in (open water, the
    /// void, ground below the scan window): the band scan skips them
    /// until a retry rescan, so they cost nothing at 2 Hz.
    empty: HashSet<(i32, i32)>,
    /// Rescans so far, for the empty-column retry.
    scan_count: u64,
    /// The dimension the cached surfaces belong to.
    dimension: String,
    last_scan: Option<u64>,
    rescan_now: bool,
    scan: Option<ScanJob>,
    /// Whether crosses and the panel are currently shown, so they are
    /// cleared exactly once when the mod turns off or the world closes.
    drawn: bool,
    last_panel: Vec<String>,
}

impl LightOverlay {
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        let _ = storage::set(ENABLED_KEY, if enabled { b"true" } else { b"false" });
        self.scan = None;
        self.rescan_now = true;
        self.refresh_settings();
    }

    fn cycle_radius(&mut self) {
        self.radius = next_radius(self.radius);
        let _ = storage::set(RADIUS_KEY, self.radius.to_string().as_bytes());
        self.scan = None;
        self.rescan_now = true;
        self.refresh_settings();
    }

    fn toggle_safe(&mut self) {
        self.show_safe = !self.show_safe;
        let _ = storage::set(SAFE_KEY, if self.show_safe { b"true" } else { b"false" });
        self.scan = None;
        self.rescan_now = true;
        self.refresh_settings();
    }

    fn toggle_rule(&mut self) {
        self.legacy = !self.legacy;
        let _ = storage::set(RULE_KEY, if self.legacy { b"legacy" } else { b"modern" });
        self.scan = None;
        self.rescan_now = true;
        self.refresh_settings();
    }

    /// The settings page: one button per setting, cycling its label.
    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: ENABLED_BUTTON,
                label: format!("Light Overlay: {}", on_off(self.enabled)),
            }),
            Widget::Button(ButtonWidget {
                id: RADIUS_BUTTON,
                label: format!("Radius: {} blocks", self.radius),
            }),
            Widget::Button(ButtonWidget {
                id: SAFE_BUTTON,
                label: format!("Safe blocks: {}", on_off(self.show_safe)),
            }),
            Widget::Button(ButtonWidget {
                id: RULE_BUTTON,
                label: format!(
                    "Spawn rule: {}",
                    if self.legacy { "Legacy" } else { "Modern" }
                ),
            }),
        ]);
    }

    fn clear_drawn(&mut self) {
        if self.drawn {
            render::set_world(&[]);
            hud::clear();
            self.drawn = false;
            self.last_panel.clear();
        }
    }

    /// Start a scan: probe the cached surfaces in one call, and queue
    /// the rest for the band scan. A column whose surface still stands
    /// keeps it; any other is dirty.
    fn start_scan(&mut self, player: &game::Player) {
        let dimension = player.dimension.clone();
        if dimension != self.dimension {
            // Surfaces belong to one dimension; a portal trip drops them.
            self.surfaces.clear();
            self.empty.clear();
            self.dimension = dimension.clone();
        }
        self.scan_count += 1;
        // Empty columns are retried every sixth rescan only.
        let retry_empty = self.scan_count % 6 == 1;
        let (fx, fy, fz) = (
            player.x.floor() as i32,
            player.y.floor() as i32,
            player.z.floor() as i32,
        );
        let columns = columns_around(fx, fz, self.radius);
        let mut resolved: HashMap<(i32, i32), i32> = HashMap::new();

        let cached: Vec<((i32, i32), i32)> = columns
            .iter()
            .filter_map(|c| self.surfaces.get(c).map(|&s| (*c, s)))
            .collect();
        if !cached.is_empty() {
            // Three positions per column, top-down: the two blocks
            // above the surface, then the surface itself.
            let mut positions = Vec::with_capacity(cached.len() * 3);
            for &((x, z), s) in &cached {
                positions.push(BlockPos { x, y: s + 2, z });
                positions.push(BlockPos { x, y: s + 1, z });
                positions.push(BlockPos { x, y: s, z });
            }
            let results = get_blocks_batched(&dimension, &positions);
            for (ci, &((x, z), s)) in cached.iter().enumerate() {
                let mut walk = Walk::new();
                let mut found = None;
                for (k, y) in [s + 2, s + 1, s].into_iter().enumerate() {
                    let cell = results
                        .get(ci * 3 + k)
                        .map(|r| classify(r.state.as_deref()))
                        .unwrap_or(Cell::Unknown);
                    if found.is_none() {
                        found = walk.step(y, cell);
                    }
                }
                if let Some(y) = found {
                    resolved.insert((x, z), y);
                }
            }
        }

        let max_cached = cached.iter().map(|&(_, s)| s).max();
        let min_cached = cached.iter().map(|&(_, s)| s).min();
        let hi = (fy + SCAN_ABOVE)
            .max(max_cached.map_or(fy + SCAN_ABOVE, |m| m + 4))
            .min(319);
        let lo = (fy - SCAN_BELOW)
            .min(min_cached.map_or(fy - SCAN_BELOW, |m| m - 4))
            .max(-64);
        let dirty: Vec<(i32, i32)> = columns
            .iter()
            .copied()
            .filter(|c| !resolved.contains_key(c))
            .filter(|c| retry_empty || !self.empty.contains(c))
            .collect();
        self.scan = Some(ScanJob {
            dimension,
            columns,
            dirty,
            walks: HashMap::new(),
            band: 0,
            hi,
            lo,
            resolved,
        });
    }

    /// Walk up to [`BANDS_PER_TICK`] bands of the scan in progress,
    /// then finish it when nothing is left to walk.
    fn advance_scan(&mut self, player: &game::Player) {
        let done = {
            let Some(job) = &mut self.scan else {
                return;
            };
            for _ in 0..BANDS_PER_TICK {
                if job.dirty.is_empty() {
                    break;
                }
                let levels: Vec<i32> = band_levels(job.hi, job.band)
                    .into_iter()
                    .filter(|&y| y >= job.lo)
                    .collect();
                if levels.is_empty() {
                    break;
                }
                job.band += 1;
                let ncols = job.dirty.len();
                let mut positions = Vec::with_capacity(levels.len() * ncols);
                for &y in &levels {
                    for &(x, z) in &job.dirty {
                        positions.push(BlockPos { x, y, z });
                    }
                }
                let results = get_blocks_batched(&job.dimension, &positions);
                let mut still_dirty = Vec::with_capacity(ncols);
                for (ci, &(x, z)) in job.dirty.iter().enumerate() {
                    let walk = job.walks.entry((x, z)).or_insert_with(Walk::new);
                    let mut found = None;
                    for (li, &y) in levels.iter().enumerate() {
                        let cell = results
                            .get(li * ncols + ci)
                            .map(|r| classify(r.state.as_deref()))
                            .unwrap_or(Cell::Unknown);
                        if found.is_none() {
                            found = walk.step(y, cell);
                        }
                    }
                    match found {
                        Some(y) => {
                            job.resolved.insert((x, z), y);
                        }
                        None => still_dirty.push((x, z)),
                    }
                }
                job.dirty = still_dirty;
            }
            job.dirty.is_empty()
                || job.band >= MAX_BANDS_PER_SCAN
                || band_levels(job.hi, job.band).iter().all(|&y| y < job.lo)
        };
        if done {
            self.finish_scan(player);
        }
    }

    /// The scan's last step: read the light above each surface, and
    /// draw the crosses for the verdicts.
    fn finish_scan(&mut self, player: &game::Player) {
        let Some(job) = self.scan.take() else {
            return;
        };
        if player.dimension != job.dimension {
            // A portal trip mid-scan: this scan belongs to the old
            // dimension, so drop it without drawing.
            self.surfaces.clear();
            self.empty.clear();
            self.dimension = player.dimension.clone();
            self.clear_drawn();
            return;
        }
        let column_set: HashSet<(i32, i32)> = job.columns.iter().copied().collect();
        // The cache is this scan's surfaces only: a column the scan
        // could not resolve has no surface worth keeping. Columns
        // still dirty after the band budget join the empty set, and
        // resolved ones leave it.
        self.surfaces = job.resolved.clone();
        for c in &job.dirty {
            self.empty.insert(*c);
        }
        for c in self.surfaces.keys() {
            self.empty.remove(c);
        }
        self.empty.retain(|c| column_set.contains(c));

        // One light read over the air block above each surface.
        let dimension = job.dimension.clone();
        let mut light_positions = Vec::with_capacity(self.surfaces.len());
        for (&(x, z), &s) in &self.surfaces {
            light_positions.push(BlockPos { x, y: s + 1, z });
        }
        let mut markers = Vec::with_capacity(light_positions.len());
        for res in get_lights_batched(&dimension, &light_positions) {
            let Some(light) = res.light else {
                continue;
            };
            let surface_y = res.y - 1;
            let v = verdict(light.block, light.sky, &dimension, self.legacy);
            if v == Verdict::None {
                continue;
            }
            let (dx, dy, dz) = (
                res.x as f64 + 0.5 - player.x,
                surface_y as f64 + 1.5 - player.y,
                res.z as f64 + 0.5 - player.z,
            );
            markers.push(Marker {
                x: res.x,
                surface_y,
                z: res.z,
                verdict: v,
                dist2: dx * dx + dy * dy + dz * dz,
            });
        }

        let (shapes, red, yellow, safe) = draw_markers(&markers, self.show_safe);
        render::set_world(&shapes);
        let mut lines = vec![format!("Red: {red}  Yellow: {yellow}")];
        if self.show_safe {
            lines[0].push_str(&format!("  Safe: {safe}"));
        }
        lines.push(format!(
            "Radius: {}  Rule: {}",
            self.radius,
            if self.legacy { "Legacy" } else { "Modern" }
        ));
        if lines != self.last_panel {
            let panel: Vec<Line> = lines
                .iter()
                .map(|text| Line {
                    text: text.clone(),
                    color: 0xFFFFFF,
                })
                .collect();
            hud::set_panel(Corner::BottomLeft, &panel);
            self.last_panel = lines;
        }
        self.drawn = true;
    }
}

impl Mod for LightOverlay {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(KEY, "Toggle Light Overlay", DEFAULT_KEY)?;
        let state = LightOverlay {
            enabled: read_bool(storage::get(ENABLED_KEY), true),
            radius: read_radius(storage::get(RADIUS_KEY)),
            show_safe: read_bool(storage::get(SAFE_KEY), false),
            legacy: read_legacy(storage::get(RULE_KEY)),
            surfaces: HashMap::new(),
            empty: HashSet::new(),
            scan_count: 0,
            dimension: String::new(),
            last_scan: None,
            rescan_now: false,
            scan: None,
            drawn: false,
            last_panel: Vec::new(),
        };
        Ok((state, Registration::new().command("lightoverlay").on_tick()))
    }

    fn tick(&mut self, tick: u64) {
        for event in input::poll() {
            if let Event::KeyDown(k) = event
                && k == KEY
            {
                let enabled = !self.enabled;
                self.set_enabled(enabled);
                chat::show(&format!(
                    "Light Overlay {}",
                    if enabled { "enabled" } else { "disabled" }
                ));
            }
        }
        let Some(player) = game::get_player().filter(|_| self.enabled) else {
            self.scan = None;
            self.clear_drawn();
            return;
        };
        if self.scan.is_some() {
            self.advance_scan(&player);
            return;
        }
        let due = self.rescan_now
            || self
                .last_scan
                .is_none_or(|t| tick.wrapping_sub(t) >= RESCAN_TICKS);
        if due {
            self.rescan_now = false;
            self.last_scan = Some(tick);
            self.start_scan(&player);
        }
    }

    fn command(&mut self, name: &str, _args: &str) -> Result<Option<String>, String> {
        if name != "lightoverlay" {
            return Err("This command does nothing".into());
        }
        let enabled = !self.enabled;
        self.set_enabled(enabled);
        Ok(Some(format!(
            "Light Overlay {}",
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
            RADIUS_BUTTON => self.cycle_radius(),
            SAFE_BUTTON => self.toggle_safe(),
            RULE_BUTTON => self.toggle_rule(),
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(LightOverlay);

#[cfg(test)]
mod tests {
    use super::*;

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
    fn api_level_is_twelve() {
        // Newest imported function is `world.get-lights` (level 12);
        // `render.set-world` is level 11, the rest level 0.
        assert_eq!(API_LEVEL, 12);
    }

    #[test]
    fn verdict_modern_needs_block_light_zero() {
        // Open sky at night: block 0, sky 15 — night value 4, so yellow.
        assert_eq!(
            verdict(0, 15, "minecraft:overworld", false),
            Verdict::Yellow
        );
        // No sky at all: red, spawnable even by day.
        assert_eq!(verdict(0, 0, "minecraft:overworld", false), Verdict::Red);
        // Sky 7 by day: day value 7, red.
        assert_eq!(verdict(0, 7, "minecraft:overworld", false), Verdict::Red);
        // Sky 8 by day: day value 8, but night value 0 — yellow.
        assert_eq!(verdict(0, 8, "minecraft:overworld", false), Verdict::Yellow);
        // Any block light at all: safe, however dark the sky is.
        assert_eq!(verdict(1, 0, "minecraft:overworld", false), Verdict::Safe);
        assert_eq!(verdict(14, 15, "minecraft:overworld", false), Verdict::Safe);
    }

    #[test]
    fn verdict_legacy_tests_the_effective_light_alone() {
        // Day 7: red without needing block light 0.
        assert_eq!(verdict(7, 7, "minecraft:overworld", true), Verdict::Red);
        // Block 7 under open sky: night value 7, yellow.
        assert_eq!(verdict(7, 15, "minecraft:overworld", true), Verdict::Yellow);
        // Block 8: night value 8, safe.
        assert_eq!(verdict(8, 15, "minecraft:overworld", true), Verdict::Safe);
        // No light at all: red.
        assert_eq!(verdict(0, 0, "minecraft:overworld", true), Verdict::Red);
    }

    #[test]
    fn verdict_in_other_dimensions() {
        // The Nether has no day or night: block 7 or less is red.
        assert_eq!(verdict(7, 0, "minecraft:the_nether", false), Verdict::Red);
        assert_eq!(verdict(0, 0, "minecraft:the_nether", false), Verdict::Red);
        assert_eq!(verdict(8, 0, "minecraft:the_nether", false), Verdict::Safe);
        // The End: light never gates endermen, so no verdict at all.
        assert_eq!(verdict(0, 0, "minecraft:the_end", false), Verdict::None);
        assert_eq!(verdict(15, 15, "minecraft:the_end", true), Verdict::None);
    }

    #[test]
    fn air_and_solid_follow_the_block_id() {
        assert!(is_air(Some("minecraft:air")));
        assert!(is_air(Some("minecraft:cave_air")));
        assert!(is_air(Some("minecraft:void_air")));
        assert!(!is_air(Some("minecraft:grass_block")));
        assert!(!is_air(None));
        assert!(is_solid(Some("minecraft:grass_block")));
        assert!(is_solid(Some("minecraft:grass_block[snowy=true]")));
        assert!(is_solid(Some("minecraft:oak_leaves")));
        assert!(is_solid(Some("minecraft:glass")));
        assert!(is_solid(Some("minecraft:stone")));
        assert!(!is_solid(Some("minecraft:air")));
        assert!(!is_solid(Some("minecraft:water")));
        assert!(!is_solid(Some("minecraft:lava")));
        assert!(!is_solid(Some("minecraft:bubble_column")));
        assert!(!is_solid(None));
        // Plants, torches and snow are not a surface.
        assert!(!is_solid(Some("minecraft:tall_grass")));
        assert!(!is_solid(Some("minecraft:short_grass")));
        assert!(!is_solid(Some("minecraft:torch")));
        assert!(!is_solid(Some("minecraft:wall_torch")));
        assert!(!is_solid(Some("minecraft:snow")));
        assert!(!is_solid(Some("minecraft:sugar_cane")));
        assert!(!is_solid(Some("minecraft:stone_button")));
        assert!(!is_solid(Some("minecraft:oak_pressure_plate")));
        assert!(!is_solid(Some("minecraft:white_carpet")));
        assert!(!is_solid(Some("minecraft:potted_dandelion")));
        // A bottom slab is not solid; a top or double slab is.
        assert!(!is_solid(Some("minecraft:oak_slab[type=bottom]")));
        assert!(is_solid(Some("minecraft:oak_slab[type=top]")));
        assert!(is_solid(Some("minecraft:oak_slab[type=double]")));
    }

    #[test]
    fn classify_sorts_states() {
        assert_eq!(classify(Some("minecraft:air")), Cell::Air);
        assert_eq!(classify(Some("minecraft:cave_air")), Cell::Air);
        assert_eq!(classify(Some("minecraft:grass_block")), Cell::Solid);
        assert_eq!(classify(Some("minecraft:torch")), Cell::Other);
        assert_eq!(classify(Some("minecraft:water")), Cell::Other);
        assert_eq!(classify(None), Cell::Unknown);
    }

    #[test]
    fn walk_finds_the_surface_top_down() {
        // Grass at 63, air at 64 and 65: stepping 65, 64, 63 finds 63.
        let mut walk = Walk::new();
        assert_eq!(walk.step(65, Cell::Air), None);
        assert_eq!(walk.step(64, Cell::Air), None);
        assert_eq!(walk.step(63, Cell::Solid), Some(63));
    }

    #[test]
    fn walk_needs_two_air_above() {
        // A torch at 64 blocks the spawn position.
        let mut walk = Walk::new();
        assert_eq!(walk.step(65, Cell::Air), None);
        assert_eq!(walk.step(64, Cell::Other), None);
        assert_eq!(walk.step(63, Cell::Solid), None);
        // Leaves at 65 block the head room.
        let mut walk = Walk::new();
        assert_eq!(walk.step(65, Cell::Solid), None);
        assert_eq!(walk.step(64, Cell::Air), None);
        assert_eq!(walk.step(63, Cell::Solid), None);
        // An unknown level above blocks it as well.
        let mut walk = Walk::new();
        assert_eq!(walk.step(65, Cell::Unknown), None);
        assert_eq!(walk.step(64, Cell::Air), None);
        assert_eq!(walk.step(63, Cell::Solid), None);
        // Two stacked solids: the top one wins.
        let mut walk = Walk::new();
        assert_eq!(walk.step(65, Cell::Air), None);
        assert_eq!(walk.step(64, Cell::Air), None);
        assert_eq!(walk.step(63, Cell::Solid), Some(63));
        assert_eq!(walk.step(62, Cell::Solid), None);
    }

    #[test]
    fn columns_are_a_circle_nearest_first() {
        assert_eq!(columns_around(0, 0, 0), [(0, 0)]);
        assert_eq!(
            columns_around(0, 0, 1),
            [(0, 0), (-1, 0), (0, -1), (0, 1), (1, 0)]
        );
        // 197 columns in a radius-8 circle, 441 in radius 12, 797 in
        // radius 16, 1257 in radius 20.
        assert_eq!(columns_around(0, 0, 8).len(), 197);
        assert_eq!(columns_around(0, 0, 12).len(), 441);
        assert_eq!(columns_around(0, 0, 16).len(), 797);
        assert_eq!(columns_around(0, 0, 20).len(), 1257);
        // Nearest first, and the centre is the player's own column.
        let cols = columns_around(10, -3, 16);
        assert_eq!(cols[0], (10, -3));
        let dist2 = |c: (i32, i32)| (c.0 - 10).pow(2) + (c.1 + 3).pow(2);
        assert!(cols.windows(2).all(|w| dist2(w[0]) <= dist2(w[1])));
    }

    #[test]
    fn bands_step_down_four_levels_from_above_the_top() {
        // hi 80: band 0 reads 82 down to 79, band 1 78 down to 75.
        assert_eq!(band_levels(80, 0), [82, 81, 80, 79]);
        assert_eq!(band_levels(80, 1), [78, 77, 76, 75]);
        assert_eq!(band_levels(80, 2), [74, 73, 72, 71]);
    }

    #[test]
    fn cross_is_two_diagonals_on_the_surface_top() {
        // A grass block at (0, 63, 0): the cross sits at 64.02, inset
        // 0.1 from the block's edges, yellow, 2.0 wide.
        let cross = cross_at(0, 63, 0, YELLOW, 2.0);
        assert_eq!(cross.len(), 2);
        assert_eq!(
            ends(&cross[0]),
            ([0.1, 64.02, 0.1], [0.9, 64.02, 0.9], YELLOW, 2.0)
        );
        assert_eq!(
            ends(&cross[1]),
            ([0.9, 64.02, 0.1], [0.1, 64.02, 0.9], YELLOW, 2.0)
        );
    }

    #[test]
    fn markers_draw_red_then_yellow_then_green_nearest_first() {
        let marker = |x: i32, z: i32, verdict: Verdict, dist2: f64| Marker {
            x,
            surface_y: 63,
            z,
            verdict,
            dist2,
        };
        let markers = vec![
            marker(5, 0, Verdict::Yellow, 25.0),
            marker(1, 0, Verdict::Red, 1.0),
            marker(0, 0, Verdict::Yellow, 0.25),
            marker(2, 0, Verdict::Safe, 4.0),
            marker(3, 0, Verdict::Red, 9.0),
        ];
        // Safe markers stay out unless the switch is on.
        let (shapes, red, yellow, safe) = draw_markers(&markers, false);
        assert_eq!((red, yellow, safe), (2, 2, 0));
        assert_eq!(shapes.len(), 8);
        // Red first, nearest first: the (1, 0) cross, then (3, 0).
        assert_eq!(ends(&shapes[0]).0, [1.1, 64.02, 0.1]);
        assert_eq!(ends(&shapes[2]).0, [3.1, 64.02, 0.1]);
        // Then yellow, nearest first: (0, 0), then (5, 0).
        assert_eq!(ends(&shapes[4]).0, [0.1, 64.02, 0.1]);
        assert_eq!(ends(&shapes[6]).0, [5.1, 64.02, 0.1]);
        // Colours follow the verdict.
        assert_eq!(ends(&shapes[0]).2, RED);
        assert_eq!(ends(&shapes[4]).2, YELLOW);
        // With safe blocks on, the safe cross draws last, green.
        let (shapes, red, yellow, safe) = draw_markers(&markers, true);
        assert_eq!((red, yellow, safe), (2, 2, 1));
        assert_eq!(shapes.len(), 10);
        assert_eq!(ends(&shapes[8]).0, [2.1, 64.02, 0.1]);
        assert_eq!(ends(&shapes[8]).2, GREEN);
    }

    #[test]
    fn the_line_cap_bounds_how_many_crosses_draw() {
        // 600 red markers: 512 crosses draw, 1024 lines in all.
        let markers: Vec<Marker> = (0..600)
            .map(|i| Marker {
                x: i,
                surface_y: 63,
                z: 0,
                verdict: Verdict::Red,
                dist2: f64::from(i),
            })
            .collect();
        let (shapes, red, yellow, safe) = draw_markers(&markers, false);
        assert_eq!((red, yellow, safe), (512, 0, 0));
        assert_eq!(shapes.len(), 1024);
    }

    #[test]
    fn settings_read_back_with_defaults() {
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert!(read_bool(Some(b"true".to_vec()), false));
        assert_eq!(read_radius(None), 16);
        assert_eq!(read_radius(Some(b"8".to_vec())), 8);
        assert_eq!(read_radius(Some(b"12".to_vec())), 12);
        assert_eq!(read_radius(Some(b"20".to_vec())), 20);
        // A stored radius the switch never offers falls back to 16.
        assert_eq!(read_radius(Some(b"99".to_vec())), 16);
        assert_eq!(read_radius(Some(b"nope".to_vec())), 16);
        assert!(!read_legacy(None));
        assert!(read_legacy(Some(b"legacy".to_vec())));
        assert!(!read_legacy(Some(b"modern".to_vec())));
        // The cycle steps through all four and wraps back to 8.
        assert_eq!(next_radius(8), 12);
        assert_eq!(next_radius(12), 16);
        assert_eq!(next_radius(16), 20);
        assert_eq!(next_radius(20), 8);
        assert_eq!(on_off(true), "ON");
        assert_eq!(on_off(false), "OFF");
    }
}

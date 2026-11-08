//! Speedrun Timer for Kestrel: an RTA timer in a corner of the screen —
//! `HH:MM:SS.mmm` with live milliseconds. It arms when the player is in a
//! world, starts on the first movement after that, and stops when the
//! player leaves the End for the overworld through the exit portal. Dying
//! is not a finish: the run keeps counting. Leaving a world for about a
//! second resets it, ready for the next run. Every finished run and the
//! personal best are saved in the mod's storage; Options > Mods >
//! Speedrun Timer has the corner, the shown and personal-best switches, a
//! button to view the saved runs, and a reset. `/speedrun` says where the
//! timer is; `/speedrun start`, `/speedrun stop` and `/speedrun reset`
//! drive it by hand.
//!
//! Behaviour reimplemented from Speedrun Timer by Demerobit (MIT,
//! <https://github.com/Demerobit/Speedrun-Timer-Horizon-Rust-Minecraft>);
//! this file shares no code with the original. The original is a native
//! mod: it reads the game's own hooks for world loads and portal exits
//! and its own wall clock. This port infers the same events from what the
//! SDK exposes — `game-read` for the player's position, dimension and
//! health, and the frame hook's delta for the clock — so the auto
//! start/stop edge cases are inferred, not observed: see the README.
//! This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/speedrun-timer>.
use horizon_mod_sdk::{
    Mod, Registration, chat,
    game::{self, Player},
    hud::{self, Corner, Line},
    storage,
    ui::{self, ButtonWidget, ListWidget, Screen, TextWidget, Widget},
};

/// The newest API level this mod needs, asserted in the tests below: every
/// function it imports is level 0 (`game.get-player`, `chat.show`,
/// `hud.set-panel`, `hud.clear`, `storage.get`, `storage.set`,
/// `ui.set-settings`, `ui.open-screen` —
/// `crates/horizon-mods/src/api_level.rs`), so it runs on any Kestrel
/// build with the SDK, including build 9 (API level 10).
#[cfg(test)]
const API_LEVEL: u32 = 0;

/// The End and the overworld, as `Player::dimension` names them.
const THE_END: &str = "minecraft:the_end";
const OVERWORLD: &str = "minecraft:overworld";

/// How far the player has to move (in blocks, on any axis) for the run to
/// start: enough that standing still never starts it.
const START_EPSILON: f64 = 0.05;

/// How many frames out of a world (about a second) before the next world
/// counts as a new session and the timer starts again from zero. A
/// dimension change or a death is far shorter than this, so neither
/// resets it.
const NEW_WORLD_FRAMES: u32 = 60;

/// How many finished runs the storage keeps: the newest ones, oldest
/// dropped first. One key holds them all as comma-separated milliseconds.
const MAX_RUNS: usize = 500;

/// The corners, as the storage names them and the settings page labels them.
const CORNERS: [(&str, &str, Corner); 4] = [
    ("top_left", "Top left", Corner::TopLeft),
    ("top_right", "Top right", Corner::TopRight),
    ("bottom_left", "Bottom left", Corner::BottomLeft),
    ("bottom_right", "Bottom right", Corner::BottomRight),
];

/// The panel's text colour while it runs, once it has stopped, and while
/// it waits: gold, green and grey, as the original draws them.
const GOLD: u32 = 0xFF_FF55;
const GREEN: u32 = 0x55_FF55;
const DIM: u32 = 0xAA_AAAA;

/// The storage keys for the settings, the personal best and the runs.
const CORNER_KEY: &str = "corner";
const SHOWN_KEY: &str = "shown";
const BEST_KEY: &str = "best";
const BEST_MS_KEY: &str = "best_ms";
const RUNS_KEY: &str = "runs";

/// The settings page's buttons: corner, shown, personal best, view the
/// saved runs, reset the timer.
const CORNER_BUTTON: u32 = 0;
const SHOWN_BUTTON: u32 = 1;
const BEST_BUTTON: u32 = 2;
const RUNS_BUTTON: u32 = 3;
const RESET_BUTTON: u32 = 4;
/// The saved-runs screen's list: picking a row does nothing.
const RUNS_LIST: u32 = 5;

/// Where the run is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// No world yet, or the run was reset.
    Idle,
    /// In a world, waiting for the first movement.
    Armed,
    /// Counting.
    Running,
    /// Stopped; the time is final.
    Done,
}

/// What one frame did, for the frame hook to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    /// Nothing to do.
    Nothing,
    /// The run just started.
    Started,
    /// The run just stopped: its final time in milliseconds.
    Finished(u64),
}

/// The run itself: no host calls, so the unit tests drive it directly.
#[derive(Clone, Debug)]
struct Run {
    phase: Phase,
    armed_at: Option<(f64, f64, f64)>,
    in_end: bool,
    /// Dead (health 0) on the last frame: the dimension is not read
    /// until the player is alive again, so dying in the End and
    /// respawning in the overworld is not a finish.
    suspended: bool,
    /// Frames spent out of a world.
    away_frames: u32,
    /// Milliseconds counted while running, from the frame deltas.
    elapsed_ms: f64,
    /// The final time once done, in milliseconds.
    last_ms: u64,
}

impl Run {
    fn new() -> Self {
        Self {
            phase: Phase::Idle,
            armed_at: None,
            in_end: false,
            suspended: false,
            away_frames: 0,
            elapsed_ms: 0.0,
            last_ms: 0,
        }
    }

    /// The time a run shows: live while it runs, final once done.
    fn reading(&self) -> u64 {
        match self.phase {
            Phase::Running => self.elapsed_ms as u64,
            Phase::Done => self.last_ms,
            _ => 0,
        }
    }

    /// One frame: arm in a world, start on the first movement, finish
    /// when the End is left for the overworld by the exit portal. Death
    /// is not a finish: a dead player suspends the reading, and a
    /// respawn in the overworld leaves the run counting.
    fn step(&mut self, player: Option<&Player>, dt_ms: f64) -> Event {
        let Some(p) = player else {
            // Out of a world: a short gap (a dimension change, a death)
            // keeps the run; a long one (the title screen) is a new
            // session.
            self.away_frames = self.away_frames.saturating_add(1);
            if self.away_frames >= NEW_WORLD_FRAMES {
                *self = Self::new();
            }
            return Event::Nothing;
        };
        self.away_frames = 0;
        match self.phase {
            Phase::Idle => {
                // A world: armed at zero, waiting for the first movement,
                // even when the last one had finished.
                self.phase = Phase::Armed;
                self.armed_at = Some((p.x, p.y, p.z));
                self.in_end = p.dimension == THE_END;
                self.suspended = false;
                self.elapsed_ms = 0.0;
                self.last_ms = 0;
                Event::Nothing
            }
            Phase::Armed => match self.armed_at {
                Some(from) if moved(from, p) => {
                    self.phase = Phase::Running;
                    self.elapsed_ms = 0.0;
                    Event::Started
                }
                _ => Event::Nothing,
            },
            Phase::Running => {
                // Real time keeps counting through the death screen and
                // the pause menu, as an RTA timer should.
                self.elapsed_ms += dt_ms;
                if p.health <= 0.0 {
                    // Dead: do not read where the player is until alive
                    // again.
                    self.suspended = true;
                    return Event::Nothing;
                }
                if self.suspended {
                    self.suspended = false;
                    if self.in_end && p.dimension != THE_END {
                        // Died in the End and came back: not a finish.
                        self.in_end = false;
                        return Event::Nothing;
                    }
                }
                if p.dimension == THE_END {
                    self.in_end = true;
                } else if self.in_end && p.dimension == OVERWORLD {
                    self.last_ms = self.elapsed_ms as u64;
                    self.phase = Phase::Done;
                    return Event::Finished(self.last_ms);
                }
                Event::Nothing
            }
            Phase::Done => Event::Nothing,
        }
    }

    /// Starts the run now, whatever the player is doing. The finish line
    /// is taken from the dimension the player is in now, so starting
    /// after a finished run does not finish the new one at once.
    fn force_start(&mut self, in_end: bool) {
        self.phase = Phase::Running;
        self.elapsed_ms = 0.0;
        self.in_end = in_end;
        self.suspended = false;
    }

    /// Stops a running run and gives its final time.
    fn force_stop(&mut self) -> Option<u64> {
        if self.phase != Phase::Running {
            return None;
        }
        self.last_ms = self.elapsed_ms as u64;
        self.phase = Phase::Done;
        Some(self.last_ms)
    }

    /// Forgets the run: back to waiting for a world.
    fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Milliseconds as `HH:MM:SS.mmm`: the hours are padded to two digits
/// and then grow as far as they need.
fn format_ms(ms: u64) -> String {
    let (h, m, s, milli) = (
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000,
    );
    format!("{h:02}:{m:02}:{s:02}.{milli:03}")
}

/// Whether the player left the spot the run was armed at.
fn moved(from: (f64, f64, f64), to: &Player) -> bool {
    (to.x - from.0).abs() > START_EPSILON
        || (to.y - from.1).abs() > START_EPSILON
        || (to.z - from.2).abs() > START_EPSILON
}

/// The stored `shown` / `best` value: shown unless it says `false`.
fn read_bool(stored: Option<Vec<u8>>, default: bool) -> bool {
    stored
        .and_then(|v| String::from_utf8(v).ok())
        .map_or(default, |v| v.trim() != "false")
}

/// The stored corner: one of the settings page's names, else the first.
fn read_corner(stored: Option<Vec<u8>>) -> usize {
    let wanted = stored.and_then(|v| String::from_utf8(v).ok());
    wanted
        .as_deref()
        .and_then(|c| CORNERS.iter().position(|k| k.0 == c.trim()))
        .unwrap_or(0)
}

/// The stored personal best in milliseconds (0: none yet).
fn read_best_ms(stored: Option<Vec<u8>>) -> u64 {
    stored
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// The stored runs: comma-separated milliseconds, oldest first, capped
/// at the newest `MAX_RUNS`.
fn read_runs(stored: Option<Vec<u8>>) -> Vec<u64> {
    let text = stored.and_then(|v| String::from_utf8(v).ok());
    let mut runs: Vec<u64> = text
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    if runs.len() > MAX_RUNS {
        runs.drain(..runs.len() - MAX_RUNS);
    }
    runs
}

/// The runs as the storage keeps them.
fn store_runs(runs: &[u64]) -> String {
    runs.iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// The saved-runs screen's rows, newest first: the personal best is
/// marked, so a run that ties it reads as one too.
fn runs_rows(runs: &[u64], best_ms: u64) -> Vec<String> {
    runs.iter()
        .rev()
        .enumerate()
        .map(|(i, ms)| {
            let time = format_ms(*ms);
            if best_ms > 0 && *ms == best_ms {
                format!("{}. {time} (personal best)", i + 1)
            } else {
                format!("{}. {time}", i + 1)
            }
        })
        .collect()
}

fn corner_label(corner: usize) -> String {
    format!("Corner: {}", CORNERS[corner].1)
}

fn shown_label(shown: bool) -> String {
    format!("Timer shown: {}", if shown { "ON" } else { "OFF" })
}

fn best_label(show_best: bool) -> String {
    format!("Personal best: {}", if show_best { "ON" } else { "OFF" })
}

fn runs_label(count: usize) -> String {
    format!("View saved runs ({count})")
}

struct SpeedrunTimer {
    run: Run,
    corner: usize,
    shown: bool,
    show_best: bool,
    best_ms: u64,
    runs: Vec<u64>,
    /// The panel last sent, so an unchanged one costs no call. While the
    /// run counts, the milliseconds change every frame, so it is sent
    /// every frame then.
    last_text: String,
    last_color: u32,
    last_corner: Option<Corner>,
}

impl SpeedrunTimer {
    /// The panel's line and colour for the run as it stands now; `None`
    /// when there is no world, so the title screen stays clean.
    fn panel(&self) -> Option<(String, u32)> {
        if !self.shown {
            return None;
        }
        let (mut text, color) = match self.run.phase {
            Phase::Idle => return None,
            Phase::Armed => (format_ms(0), DIM),
            Phase::Running => (format_ms(self.run.reading()), GOLD),
            Phase::Done => (format!("{}  done", format_ms(self.run.last_ms)), GREEN),
        };
        if self.show_best && self.best_ms > 0 {
            text.push_str(&format!("   PB {}", format_ms(self.best_ms)));
        }
        Some((text, color))
    }

    fn refresh_panel(&mut self) {
        let corner = CORNERS[self.corner].2;
        match self.panel() {
            Some((text, color))
                if text != self.last_text
                    || color != self.last_color
                    || Some(corner) != self.last_corner =>
            {
                hud::set_panel(
                    corner,
                    &[Line {
                        text: text.clone(),
                        color,
                    }],
                );
                self.last_text = text;
                self.last_color = color;
                self.last_corner = Some(corner);
            }
            Some(_) => {}
            None => self.clear_panel(),
        }
    }

    fn clear_panel(&mut self) {
        if !self.last_text.is_empty() || self.last_corner.is_some() {
            hud::clear();
            self.last_text.clear();
            self.last_color = 0;
            self.last_corner = None;
        }
    }

    /// A finished run is saved and the personal best kept: the run goes
    /// in the list either way, the best only when it beats it.
    fn on_finished(&mut self, ms: u64) -> bool {
        self.runs.push(ms);
        if self.runs.len() > MAX_RUNS {
            self.runs.drain(..self.runs.len() - MAX_RUNS);
        }
        let _ = storage::set(RUNS_KEY, store_runs(&self.runs).as_bytes());
        let best = self.best_ms == 0 || ms < self.best_ms;
        if best {
            self.best_ms = ms;
            let _ = storage::set(BEST_MS_KEY, ms.to_string().as_bytes());
        }
        best
    }

    /// A run stopped by hand is saved like a finished one, but it never
    /// counts as a personal best: the original saves it without keeping
    /// the best.
    fn on_stopped(&mut self, ms: u64) {
        self.runs.push(ms);
        if self.runs.len() > MAX_RUNS {
            self.runs.drain(..self.runs.len() - MAX_RUNS);
        }
        let _ = storage::set(RUNS_KEY, store_runs(&self.runs).as_bytes());
    }

    fn refresh_settings(&self) {
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: CORNER_BUTTON,
                label: corner_label(self.corner),
            }),
            Widget::Button(ButtonWidget {
                id: SHOWN_BUTTON,
                label: shown_label(self.shown),
            }),
            Widget::Button(ButtonWidget {
                id: BEST_BUTTON,
                label: best_label(self.show_best),
            }),
            Widget::Button(ButtonWidget {
                id: RUNS_BUTTON,
                label: runs_label(self.runs.len()),
            }),
            Widget::Button(ButtonWidget {
                id: RESET_BUTTON,
                label: "Reset timer".to_string(),
            }),
        ]);
    }

    /// The saved runs, newest first, over the game.
    fn open_runs_screen(&self) {
        if self.runs.is_empty() {
            ui::open_screen(&Screen {
                title: "Speedrun Timer: saved runs".to_string(),
                widgets: vec![Widget::Text(TextWidget {
                    id: RUNS_LIST,
                    text: "No finished runs yet.".to_string(),
                    color: 0,
                })],
            });
            return;
        }
        let mut widgets = vec![Widget::Text(TextWidget {
            id: RUNS_LIST,
            text: format!(
                "{} finished run{} — personal best {}",
                self.runs.len(),
                if self.runs.len() == 1 { "" } else { "s" },
                if self.best_ms > 0 {
                    format_ms(self.best_ms)
                } else {
                    "none yet".to_string()
                },
            ),
            color: 0,
        })];
        widgets.push(Widget::List(ListWidget {
            id: RUNS_LIST,
            rows: runs_rows(&self.runs, self.best_ms),
            selected: None,
        }));
        ui::open_screen(&Screen {
            title: "Speedrun Timer: saved runs".to_string(),
            widgets,
        });
    }

    fn cycle_corner(&mut self) {
        self.corner = (self.corner + 1) % CORNERS.len();
        let _ = storage::set(CORNER_KEY, CORNERS[self.corner].0.as_bytes());
        self.refresh_settings();
        self.refresh_panel();
    }

    fn toggle_shown(&mut self) {
        self.shown = !self.shown;
        let _ = storage::set(SHOWN_KEY, if self.shown { b"true" } else { b"false" });
        self.refresh_settings();
        self.refresh_panel();
    }

    fn toggle_best(&mut self) {
        self.show_best = !self.show_best;
        let _ = storage::set(BEST_KEY, if self.show_best { b"true" } else { b"false" });
        self.refresh_settings();
        self.refresh_panel();
    }

    fn status(&self) -> String {
        match self.run.phase {
            Phase::Idle => "Speedrun timer: waiting for a world".to_string(),
            Phase::Armed => "Speedrun timer: armed, move to start".to_string(),
            Phase::Running => format!("Speedrun timer: running, {}", format_ms(self.run.reading())),
            Phase::Done => format!("Speedrun timer: finished, {}", format_ms(self.run.last_ms)),
        }
    }
}

impl Mod for SpeedrunTimer {
    fn init() -> Result<(Self, Registration), String> {
        let hooks = Registration::new().command("speedrun").on_frame();
        let state = SpeedrunTimer {
            run: Run::new(),
            corner: read_corner(storage::get(CORNER_KEY)),
            shown: read_bool(storage::get(SHOWN_KEY), true),
            show_best: read_bool(storage::get(BEST_KEY), true),
            best_ms: read_best_ms(storage::get(BEST_MS_KEY)),
            runs: read_runs(storage::get(RUNS_KEY)),
            last_text: String::new(),
            last_color: 0,
            last_corner: None,
        };
        Ok((state, hooks))
    }

    fn frame(&mut self, seconds: f32) {
        // The frame delta is the clock: real time, with live
        // milliseconds, as the original's wall clock is.
        let dt_ms = (f64::from(seconds) * 1000.0).max(0.0);
        let player = game::get_player();
        match self.run.step(player.as_ref(), dt_ms) {
            Event::Nothing => {}
            Event::Started => chat::show("Speedrun timer started"),
            Event::Finished(ms) => {
                let best = self.on_finished(ms);
                let line = format!(
                    "Run finished: {}{}",
                    format_ms(ms),
                    if best { " — personal best" } else { "" }
                );
                chat::show(&line);
            }
        }
        self.refresh_panel();
    }

    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        if name != "speedrun" {
            return Err("This command does nothing".into());
        }
        let mut parts = args.split_whitespace();
        match parts.next().unwrap_or("") {
            "" => Ok(Some(self.status())),
            "start" => {
                let in_end = game::get_player().is_some_and(|p| p.dimension == THE_END);
                self.run.force_start(in_end);
                self.refresh_panel();
                Ok(Some("Speedrun timer started".to_string()))
            }
            "stop" => match self.run.force_stop() {
                Some(ms) => {
                    self.on_stopped(ms);
                    self.refresh_panel();
                    Ok(Some(format!("Speedrun timer stopped: {}", format_ms(ms))))
                }
                None => Ok(Some("Speedrun timer is not running".to_string())),
            },
            "reset" => {
                self.run.reset();
                self.refresh_panel();
                Ok(Some("Speedrun timer reset".to_string()))
            }
            other => Err(format!(
                "Unknown option {other}: use /speedrun, /speedrun start, /speedrun stop, /speedrun reset"
            )),
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            CORNER_BUTTON => self.cycle_corner(),
            SHOWN_BUTTON => self.toggle_shown(),
            BEST_BUTTON => self.toggle_best(),
            RUNS_BUTTON => self.open_runs_screen(),
            RESET_BUTTON => {
                self.run.reset();
                chat::show("Speedrun timer reset");
                self.refresh_panel();
            }
            _ => {}
        }
    }
}

horizon_mod_sdk::export_mod!(SpeedrunTimer);

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

    fn at(x: f64, y: f64, z: f64) -> Player {
        Player {
            x,
            y,
            z,
            ..player()
        }
    }

    fn end() -> Player {
        Player {
            dimension: THE_END.into(),
            ..player()
        }
    }

    /// One frame, 1/60 s, with the player where the test puts them.
    fn frame(run: &mut Run, player: Option<&Player>) -> Event {
        run.step(player, 1000.0 / 60.0)
    }

    #[test]
    fn api_level_is_zero() {
        // Every imported function is level 0 (see the `API_LEVEL` note
        // above); nothing in this mod needs a newer Horizon.
        assert_eq!(API_LEVEL, 0);
    }

    #[test]
    fn formats_hours_minutes_seconds_and_milliseconds() {
        assert_eq!(format_ms(0), "00:00:00.000");
        assert_eq!(format_ms(999), "00:00:00.999");
        assert_eq!(format_ms(1000), "00:00:01.000");
        // 1h 02m 03.456s
        assert_eq!(format_ms(3_723_456), "01:02:03.456");
        // Hours go on for ever: 100h, 1000h, 10 000h.
        assert_eq!(format_ms(360_000_000), "100:00:00.000");
        assert_eq!(format_ms(3_600_000_000), "1000:00:00.000");
        assert_eq!(format_ms(36_000_000_000), "10000:00:00.000");
    }

    #[test]
    fn movement_starts_and_standing_still_does_not() {
        assert!(!moved((0.0, 64.0, 0.0), &at(0.0, 64.0, 0.0)));
        assert!(!moved((0.0, 64.0, 0.0), &at(0.01, 64.0, 0.0)));
        assert!(moved((0.0, 64.0, 0.0), &at(0.2, 64.0, 0.0)));
        assert!(moved((0.0, 64.0, 0.0), &at(0.0, 63.0, 0.0)));
        // Looking around is not movement.
        let looking = Player {
            yaw: 180.0,
            pitch: 45.0,
            ..at(0.0, 64.0, 0.0)
        };
        assert!(!moved((0.0, 64.0, 0.0), &looking));
    }

    #[test]
    fn arms_in_a_world_starts_on_movement_and_finishes_leaving_the_end() {
        let mut run = Run::new();
        // No world: nothing.
        assert_eq!(run.step(None, 1000.0 / 60.0), Event::Nothing);
        assert_eq!(run.phase, Phase::Idle);
        // A world: armed, not counting, and it shows zero.
        let p = at(0.0, 64.0, 0.0);
        assert_eq!(frame(&mut run, Some(&p)), Event::Nothing);
        assert_eq!((run.phase, run.reading()), (Phase::Armed, 0));
        // Still standing: no start.
        assert_eq!(frame(&mut run, Some(&p)), Event::Nothing);
        assert_eq!(run.phase, Phase::Armed);
        // Movement: the run starts.
        assert_eq!(frame(&mut run, Some(&at(0.5, 64.0, 0.0))), Event::Started);
        assert_eq!(run.phase, Phase::Running);
        // Walking around in the overworld does not finish it.
        run.step(Some(&at(40.0, 64.0, 9.0)), 5000.0);
        assert_eq!(run.phase, Phase::Running);
        // The End arms the finish line...
        assert_eq!(frame(&mut run, Some(&end())), Event::Nothing);
        assert_eq!(run.phase, Phase::Running);
        // ...and the overworld crosses it, after the time that passed:
        // 5000 ms walking, one 1/60 s frame in the End, 2000 ms more.
        run.step(Some(&at(0.0, 64.0, 0.0)), 2000.0);
        assert_eq!(run.phase, Phase::Done);
        assert_eq!(run.last_ms, 7016);
        // A finished run does not move again.
        assert_eq!(frame(&mut run, Some(&at(1.0, 64.0, 1.0))), Event::Nothing);
        assert_eq!(run.reading(), 7016);
    }

    #[test]
    fn leaving_a_world_before_moving_waits_for_the_next_one() {
        let mut run = Run::new();
        let p = at(0.0, 64.0, 0.0);
        frame(&mut run, Some(&p));
        // A single frame out of the world (a dimension change, a death)
        // keeps the run armed where it was.
        assert_eq!(run.step(None, 1000.0 / 60.0), Event::Nothing);
        assert_eq!(run.phase, Phase::Armed);
        // A whole second out of it is a new session.
        for _ in 0..NEW_WORLD_FRAMES {
            run.step(None, 1000.0 / 60.0);
        }
        assert_eq!(run.phase, Phase::Idle);
    }

    #[test]
    fn a_new_world_starts_again_from_zero() {
        let mut run = Run::new();
        frame(&mut run, Some(&at(0.0, 64.0, 0.0)));
        frame(&mut run, Some(&at(1.0, 64.0, 0.0)));
        assert_eq!(run.phase, Phase::Running);
        // Back to the title long enough to be a new session.
        for _ in 0..NEW_WORLD_FRAMES {
            run.step(None, 1000.0 / 60.0);
        }
        assert_eq!(run.phase, Phase::Idle);
        // The new world: armed at zero, waiting for the first movement.
        assert_eq!(
            frame(&mut run, Some(&at(100.0, 70.0, 100.0))),
            Event::Nothing
        );
        assert_eq!((run.phase, run.reading()), (Phase::Armed, 0));
    }

    #[test]
    fn dying_in_the_end_is_not_a_finish() {
        let mut run = Run::new();
        frame(&mut run, Some(&at(0.0, 64.0, 0.0)));
        frame(&mut run, Some(&at(1.0, 64.0, 0.0)));
        frame(&mut run, Some(&end()));
        // The death screen comes up: health 0 suspends the reading.
        let dead = Player {
            health: 0.0,
            ..end()
        };
        assert_eq!(frame(&mut run, Some(&dead)), Event::Nothing);
        assert_eq!(run.phase, Phase::Running);
        // The respawn puts the player in the overworld: the run goes on,
        // and the time kept counting through the death.
        assert_eq!(frame(&mut run, Some(&at(0.5, 64.0, 0.5))), Event::Nothing);
        assert_eq!(run.phase, Phase::Running);
        assert!(run.reading() > 0);
    }

    #[test]
    fn dying_in_the_overworld_keeps_the_run_counting() {
        let mut run = Run::new();
        frame(&mut run, Some(&at(0.0, 64.0, 0.0)));
        frame(&mut run, Some(&at(1.0, 64.0, 0.0)));
        run.step(Some(&at(1.0, 64.0, 0.0)), 5000.0);
        let dead = Player {
            health: 0.0,
            ..at(1.0, 64.0, 0.0)
        };
        frame(&mut run, Some(&dead));
        frame(&mut run, Some(&at(0.5, 64.0, 0.5)));
        assert_eq!(run.phase, Phase::Running);
        assert!(run.reading() >= 5000);
    }

    #[test]
    fn a_menu_in_the_end_keeps_the_finish_working() {
        let mut run = Run::new();
        frame(&mut run, Some(&at(0.0, 64.0, 0.0)));
        frame(&mut run, Some(&at(1.0, 64.0, 0.0)));
        frame(&mut run, Some(&end()));
        // Dead in the End, then alive again still in the End: the exit
        // portal still finishes the run.
        let dead = Player {
            health: 0.0,
            ..end()
        };
        frame(&mut run, Some(&dead));
        frame(&mut run, Some(&end()));
        assert_eq!(run.phase, Phase::Running);
        run.step(Some(&at(0.0, 64.0, 0.0)), 2000.0);
        assert_eq!(run.phase, Phase::Done);
    }

    #[test]
    fn force_start_and_stop_work_from_the_command() {
        let mut run = Run::new();
        run.force_start(false);
        assert_eq!(run.phase, Phase::Running);
        run.step(Some(&at(0.0, 64.0, 0.0)), 1500.0);
        assert_eq!(run.force_stop(), Some(1500));
        assert_eq!(run.force_stop(), None);
        // Starting after a finish, in the overworld, does not finish at
        // once: the finish line is taken from where the player is now.
        run.force_start(false);
        assert_eq!(frame(&mut run, Some(&at(0.0, 64.0, 0.0))), Event::Nothing);
        assert_eq!(run.phase, Phase::Running);
    }

    #[test]
    fn settings_default_cycle_and_store() {
        assert_eq!(read_corner(None), 0);
        assert!(read_bool(None, true));
        assert!(!read_bool(Some(b"false".to_vec()), true));
        assert_eq!(read_best_ms(None), 0);
        assert_eq!(read_best_ms(Some(b"83456".to_vec())), 83_456);
        // Stored values come back, trailing newline and all.
        assert_eq!(read_corner(Some(b"bottom_left\n".to_vec())), 2);
        assert_eq!(read_corner(Some(b"nowhere".to_vec())), 0);
        assert_eq!(corner_label(0), "Corner: Top left");
        assert_eq!(corner_label(3), "Corner: Bottom right");
        assert_eq!(shown_label(false), "Timer shown: OFF");
        assert_eq!(best_label(true), "Personal best: ON");
        assert_eq!(runs_label(0), "View saved runs (0)");
        assert_eq!(runs_label(12), "View saved runs (12)");
    }

    #[test]
    fn runs_round_trip_and_cap_at_the_newest() {
        assert!(read_runs(None).is_empty());
        assert!(read_runs(Some(b"".to_vec())).is_empty());
        let runs = read_runs(Some(b"12000,83456, 999".to_vec()));
        assert_eq!(runs, vec![12000, 83_456, 999]);
        assert_eq!(store_runs(&runs), "12000,83456,999");
        // Junk entries are dropped, not kept as zeroes.
        assert_eq!(read_runs(Some(b"12x,,42".to_vec())), vec![42]);
        // Past the cap only the newest stay.
        let many: Vec<u64> = (0..MAX_RUNS as u64 + 10).collect();
        let kept = read_runs(Some(store_runs(&many).into_bytes()));
        assert_eq!(kept.len(), MAX_RUNS);
        assert_eq!(kept[0], 10);
        assert_eq!(*kept.last().unwrap(), MAX_RUNS as u64 + 9);
    }

    #[test]
    fn saved_runs_list_newest_first_with_the_best_marked() {
        assert!(runs_rows(&[], 0).is_empty());
        assert_eq!(
            runs_rows(&[12_000, 83_456, 999], 999),
            vec![
                "1. 00:00:00.999 (personal best)",
                "2. 00:01:23.456",
                "3. 00:00:12.000",
            ]
        );
        // No best yet: no row is marked.
        assert_eq!(runs_rows(&[12_000], 0), vec!["1. 00:00:12.000"]);
    }
}

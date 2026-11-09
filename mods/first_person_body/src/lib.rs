//! First Person Body for Kestrel: see your own body in first
//! person — look down to see your legs and feet.
//!
//! Behaviour reimplemented from First Person Body by
//! selfmonoraque5, MIT,
//! <https://github.com/selfmonoraque5/first-person-body>: this
//! file shares no code with the original — the original is a
//! native `.hnmod` for Horizon (inspired by tr7zw's
//! First-person Model, and containing none of its code), this
//! is a fresh implementation against the Kestrel SDK
//! (`entity-pose`, API 20). This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/first_person_body>.
//!
//! Fidelity, against the original's registry description and
//! README (which tabulates its settings and defaults — this
//! port's settings are that table): the body is shown in first
//! person by `entity-pose.set-first-person-body` (the host
//! hides the head and hat), sits a little behind the camera —
//! a little more when you look down or crouch — keeps the
//! vanilla hand by default (the model's arms hidden under it)
//! or shows the model's own arms, and steps aside while
//! swimming, crawling, gliding and sleeping. The toggle key
//! starts unbound, as the original's does: it is registered
//! with the game's own unbound key name, and binding it is done
//! in Options > Controls.
//!
//! What is NOT ported, and why (nothing stands in for any of
//! it): the step-asides for a Riptide spin and for spectators —
//! the pose state exposes neither a spin signal nor a spectator
//! flag. And with the model's own arms chosen, the original
//! cancels the vanilla hand's draw; the SDK exposes no
//! hand-hide, so the vanilla hand still draws over the model's
//! arms here. The body-offset steps are named in the original
//! (None, Small, Normal, Large) but not numbered; the distances
//! below are the port's reading of "a little behind".

use horizon_mod_sdk::{
    Mod, PartOffset, PoseOffset, PoseTarget, Registration,
    entity_pose::{self, EntityPoseState},
    input::{self, Event},
    storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs: `entity-pose` is level
/// 20 (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 20;

/// The toggle key: registered, and starting unbound — the
/// game's own name for a key with nothing on it, the original's
/// default.
const TOGGLE_KEY: &str = "toggle";
const DEFAULT_TOGGLE_KEY: &str = "key.keyboard.unknown";

/// The arms setting's two values, the original's words.
const ARMS_VANILLA: u32 = 0;
const ARMS_MODEL: u32 = 1;
const ARMS_NAMES: [&str; 2] = ["Vanilla hand", "Model arms"];

/// The body offset setting's steps, the original's names; the
/// distances (blocks behind the camera) are the port's reading.
const OFFSET_NAMES: [&str; 4] = ["None", "Small", "Normal", "Large"];
const OFFSET_STEPS: [f32; 4] = [0.0, 0.06, 0.12, 0.20];
const DEFAULT_OFFSET: u32 = 2; // Normal
/// Looking down adds up to this much more (Lean back looking
/// down), and crouching adds this much.
const LEAN_EXTRA: f32 = 0.10;
const CROUCH_EXTRA: f32 = 0.06;

/// The settings page's widget ids.
const ENABLED_BUTTON: u32 = 0;
const ARMS_BUTTON: u32 = 1;
const OFFSET_BUTTON: u32 = 2;
const RIDING_BUTTON: u32 = 3;
const LEAN_BUTTON: u32 = 4;
const RESET_BUTTON: u32 = 5;

/// The storage keys.
const ENABLED_KEY: &str = "enabled";
const ARMS_KEY: &str = "arms";
const OFFSET_KEY: &str = "offset";
const RIDING_KEY: &str = "while_riding";
const LEAN_KEY: &str = "lean_back";

/// The settings, the original's table.
#[derive(Clone)]
struct Settings {
    enabled: bool,
    arms: u32,
    offset: u32,
    while_riding: bool,
    lean_back: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            arms: ARMS_VANILLA,
            offset: DEFAULT_OFFSET,
            while_riding: true,
            lean_back: true,
        }
    }
}

impl Settings {
    fn save(&self) {
        let put = |key: &str, value: String| {
            let _ = storage::set(key, value.as_bytes());
        };
        put(ENABLED_KEY, flag(self.enabled).into());
        put(ARMS_KEY, self.arms.to_string());
        put(OFFSET_KEY, self.offset.to_string());
        put(RIDING_KEY, flag(self.while_riding).into());
        put(LEAN_KEY, flag(self.lean_back).into());
    }
}

fn flag(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

/// A stored switch ("1" / "0"); `default` when unreadable.
fn read_switch(key: &str, default: bool) -> bool {
    match storage::get(key) {
        Some(bytes) => match std::str::from_utf8(&bytes) {
            Ok("1") => true,
            Ok("0") => false,
            _ => default,
        },
        None => default,
    }
}

/// A stored index up to `max`; `default` when unreadable.
fn read_index(key: &str, max: u32, default: u32) -> u32 {
    let Some(bytes) = storage::get(key) else {
        return default;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return default;
    };
    match text.trim().parse::<u32>() {
        Ok(v) if v <= max => v,
        _ => default,
    }
}

fn enabled_label(enabled: bool) -> String {
    format!("First Person Body: {}", on_off(enabled))
}

fn arms_label(arms: u32) -> String {
    format!("Arms: {}", ARMS_NAMES[arms as usize])
}

fn offset_label(offset: u32) -> String {
    format!("Body offset: {}", OFFSET_NAMES[offset as usize])
}

fn riding_label(on: bool) -> String {
    format!("While riding: {}", on_off(on))
}

fn lean_label(on: bool) -> String {
    format!("Lean back looking down: {}", on_off(on))
}

/// Steps aside: the body is not shown while swimming or
/// crawling (the swim stroke's amount is up — the crawl reads
/// as the swim pose), while gliding, or while sleeping; and
/// while riding when the While riding setting is off.
fn stepped_aside(settings: &Settings, state: &EntityPoseState) -> bool {
    state.swim_amount > 0.02
        || state.gliding
        || state.sleeping
        || (state.riding && !settings.while_riding)
}

/// How far behind the camera the body sits, in blocks: the
/// setting's step, a little more looking down (the full extra
/// at straight down, when Lean back looking down is on), a
/// little more crouching. The offset moves the `body` part back
/// — a negative z in the model's own space, whose front is +z.
fn body_offset(settings: &Settings, state: &EntityPoseState) -> f32 {
    let mut back = OFFSET_STEPS[settings.offset as usize];
    if settings.lean_back {
        back += (state.head_pitch.clamp(0.0, 90.0) / 90.0) * LEAN_EXTRA;
    }
    if state.crouching {
        back += CROUCH_EXTRA;
    }
    back
}

/// The parts the body does not draw: with the vanilla hand, the
/// model's arms hide under it; with the model's own arms,
/// nothing hides.
fn hidden_parts(settings: &Settings) -> Vec<String> {
    if settings.arms == ARMS_VANILLA {
        vec![
            "right_arm".into(),
            "right_sleeve".into(),
            "left_arm".into(),
            "left_sleeve".into(),
        ]
    } else {
        Vec::new()
    }
}

struct FirstPersonBody {
    settings: Settings,
    /// The body is currently asked for.
    shown: bool,
    /// A pose offset is outstanding on the self target.
    offset_written: bool,
}

impl FirstPersonBody {
    fn refresh_settings(&self) {
        let s = &self.settings;
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: ENABLED_BUTTON,
                label: enabled_label(s.enabled),
            }),
            Widget::Button(ButtonWidget {
                id: ARMS_BUTTON,
                label: arms_label(s.arms),
            }),
            Widget::Button(ButtonWidget {
                id: OFFSET_BUTTON,
                label: offset_label(s.offset),
            }),
            Widget::Button(ButtonWidget {
                id: RIDING_BUTTON,
                label: riding_label(s.while_riding),
            }),
            Widget::Button(ButtonWidget {
                id: LEAN_BUTTON,
                label: lean_label(s.lean_back),
            }),
            Widget::Button(ButtonWidget {
                id: RESET_BUTTON,
                label: "Reset to defaults".into(),
            }),
        ]);
    }

    /// Withdraw everything the mod has asked the game for.
    fn withdraw(&mut self) {
        if self.shown {
            entity_pose::set_first_person_body(false);
            self.shown = false;
        }
        if self.offset_written {
            entity_pose::clear_pose_offset(PoseTarget::SelfPlayer);
            self.offset_written = false;
        }
    }
}

impl Mod for FirstPersonBody {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(TOGGLE_KEY, "Toggle First Person Body", DEFAULT_TOGGLE_KEY)?;
        let d = Settings::default();
        let settings = Settings {
            enabled: read_switch(ENABLED_KEY, d.enabled),
            arms: read_index(ARMS_KEY, ARMS_MODEL, d.arms),
            offset: read_index(OFFSET_KEY, OFFSET_NAMES.len() as u32 - 1, d.offset),
            while_riding: read_switch(RIDING_KEY, d.while_riding),
            lean_back: read_switch(LEAN_KEY, d.lean_back),
        };
        Ok((
            FirstPersonBody {
                settings,
                shown: false,
                offset_written: false,
            },
            Registration::new().on_tick().on_frame(),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        for event in input::poll() {
            if let Event::KeyDown(key) = event
                && key == TOGGLE_KEY
            {
                self.settings.enabled = !self.settings.enabled;
                self.settings.save();
            }
        }
    }

    fn frame(&mut self, _seconds: f32) {
        let states = entity_pose::get_pose_states();
        let me = states.iter().find(|s| s.self_);
        let want = self.settings.enabled && me.is_some_and(|s| !stepped_aside(&self.settings, s));
        if !want {
            self.withdraw();
            return;
        }
        let state = me.unwrap();
        if !self.shown {
            entity_pose::set_first_person_body(true);
            self.shown = true;
        }
        let back = body_offset(&self.settings, state);
        let hidden = hidden_parts(&self.settings);
        if back == 0.0 && hidden.is_empty() {
            if self.offset_written {
                entity_pose::clear_pose_offset(PoseTarget::SelfPlayer);
                self.offset_written = false;
            }
            return;
        }
        entity_pose::set_pose_offset(
            PoseTarget::SelfPlayer,
            &PoseOffset {
                parts: vec![PartOffset {
                    part: "body".into(),
                    x: 0.0,
                    y: 0.0,
                    z: -back,
                    pitch: 0.0,
                    yaw: 0.0,
                    roll: 0.0,
                }],
                hidden,
            },
        );
        self.offset_written = true;
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        let s = &mut self.settings;
        match id {
            ENABLED_BUTTON => s.enabled = !s.enabled,
            ARMS_BUTTON => s.arms = (s.arms + 1) % ARMS_NAMES.len() as u32,
            OFFSET_BUTTON => s.offset = (s.offset + 1) % OFFSET_NAMES.len() as u32,
            RIDING_BUTTON => s.while_riding = !s.while_riding,
            LEAN_BUTTON => s.lean_back = !s.lean_back,
            RESET_BUTTON => {
                self.settings = Settings::default();
                self.settings.save();
                self.refresh_settings();
                return;
            }
            _ => return,
        }
        self.settings.save();
        self.refresh_settings();
    }
}

horizon_mod_sdk::export_mod!(FirstPersonBody);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::entity_pose::{ArmPose, SwingAnim};

    #[test]
    fn api_level_is_20() {
        assert_eq!(API_LEVEL, 20);
    }

    /// A standing, idle player; tests move the fields they need.
    fn state() -> EntityPoseState {
        EntityPoseState {
            id: 1,
            kind: "minecraft:player".into(),
            self_: true,
            walk_pos: 0.0,
            walk_speed: 0.0,
            head_yaw: 0.0,
            head_pitch: 0.0,
            swing_progress: 0.0,
            swing_anim: SwingAnim::None,
            swing_off_hand: false,
            main_arm_pose: ArmPose::Empty,
            off_arm_pose: ArmPose::Empty,
            main_hand: None,
            off_hand: None,
            using: None,
            crouching: false,
            riding: false,
            swim_amount: 0.0,
            gliding: false,
            sleeping: false,
            sprinting: false,
            on_ground: true,
        }
    }

    #[test]
    fn the_defaults_are_the_originals_table() {
        let s = Settings::default();
        assert!(s.enabled);
        assert_eq!(s.arms, ARMS_VANILLA);
        assert_eq!(OFFSET_NAMES[s.offset as usize], "Normal");
        assert!(s.while_riding);
        assert!(s.lean_back);
    }

    #[test]
    fn step_asides_match_the_originals_list() {
        let settings = Settings::default();
        let mut s = state();
        assert!(!stepped_aside(&settings, &s));
        s.swim_amount = 1.0;
        assert!(stepped_aside(&settings, &s), "swimming");
        s.swim_amount = 0.5;
        assert!(
            stepped_aside(&settings, &s),
            "crawling reads as the swim pose"
        );
        s.swim_amount = 0.0;
        s.gliding = true;
        assert!(stepped_aside(&settings, &s), "gliding");
        s.gliding = false;
        s.sleeping = true;
        assert!(stepped_aside(&settings, &s), "sleeping");
        s.sleeping = false;
        s.riding = true;
        assert!(!stepped_aside(&settings, &s), "riding shows, by default");
        let mut off = Settings::default();
        off.while_riding = false;
        assert!(stepped_aside(&off, &s), "riding steps aside when set off");
    }

    #[test]
    fn the_body_sits_further_back_looking_down_or_crouching() {
        let settings = Settings::default();
        let mut s = state();
        let base = body_offset(&settings, &s);
        assert!((base - 0.12).abs() < 1e-6, "Normal is {base}");
        s.head_pitch = 90.0;
        let down = body_offset(&settings, &s);
        assert!(
            (down - (0.12 + LEAN_EXTRA)).abs() < 1e-6,
            "full lean {down}"
        );
        s.head_pitch = 45.0;
        let half = body_offset(&settings, &s);
        assert!((half - (0.12 + LEAN_EXTRA / 2.0)).abs() < 1e-6);
        s.head_pitch = -30.0;
        assert!(
            (body_offset(&settings, &s) - 0.12).abs() < 1e-6,
            "looking up adds nothing"
        );
        s.crouching = true;
        let crouch = body_offset(&settings, &s);
        assert!((crouch - (0.12 + CROUCH_EXTRA)).abs() < 1e-6);
        // With the lean off, looking down adds nothing.
        let mut no_lean = Settings::default();
        no_lean.lean_back = false;
        s.head_pitch = 90.0;
        s.crouching = false;
        assert!((body_offset(&no_lean, &s) - 0.12).abs() < 1e-6);
        // The named steps.
        let mut none = Settings::default();
        none.offset = 0;
        assert_eq!(body_offset(&none, &state()), 0.0);
        none.offset = 3;
        assert!((body_offset(&none, &state()) - 0.20).abs() < 1e-6);
    }

    #[test]
    fn the_vanilla_hand_hides_the_models_arms() {
        let settings = Settings::default();
        assert_eq!(
            hidden_parts(&settings),
            ["right_arm", "right_sleeve", "left_arm", "left_sleeve"]
        );
        let mut model = Settings::default();
        model.arms = ARMS_MODEL;
        assert!(hidden_parts(&model).is_empty());
    }

    #[test]
    fn labels_are_the_originals_words() {
        assert_eq!(enabled_label(true), "First Person Body: ON");
        assert_eq!(enabled_label(false), "First Person Body: OFF");
        assert_eq!(arms_label(ARMS_VANILLA), "Arms: Vanilla hand");
        assert_eq!(arms_label(ARMS_MODEL), "Arms: Model arms");
        assert_eq!(offset_label(2), "Body offset: Normal");
        assert_eq!(riding_label(true), "While riding: ON");
        assert_eq!(lean_label(false), "Lean back looking down: OFF");
    }
}

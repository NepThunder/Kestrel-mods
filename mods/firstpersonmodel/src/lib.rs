//! First-person Model for Kestrel: see your own third-person
//! body in first person.
//!
//! Behaviour reimplemented from First-person Model by tr7zw,
//! MIT, <https://github.com/tr7zw/FirstPersonModel>, in the
//! shape of its Horizon port by DeportedToHell (MIT): this file
//! shares no code with either — a fresh implementation against
//! the Kestrel SDK (`entity-pose`, API 20). This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/firstpersonmodel>.
//!
//! Fidelity, against the Horizon port's registry description
//! and README: the body is shown in first person by
//! `entity-pose.set-first-person-body` (the host hides the head
//! and hat), drawn 0.25 blocks back along the body's yaw when
//! standing, with per-pose offsets for sneaking, sitting,
//! crawling and swimming — the standing, sneaking and sitting
//! ones changeable — and steps aside while sleeping, gliding,
//! half swimming and scoping with a spyglass. Vanilla Hands
//! Mode, Dynamic Mode, Disable Vanilla Hands while Swimming
//! and the Show Layers toggles live on the standard settings
//! page (see `hidden_parts` for exactly what each does here).
//!
//! What is NOT ported, and why (nothing stands in for any of
//! it): the four-tab config screen and its searchable item
//! lists — Auto Vanilla Hands and Disable Items are per-item
//! lists, and the SDK's settings page is rows of buttons; those
//! features wait on a settings surface, not on poses. The layer
//! toggles for armour, held items, arrows, ears, wings,
//! parrots, stingers and head items — those are separate draws,
//! not parts of the posed model, so pose offsets cannot hide
//! them (the jacket, sleeves, pants and cape toggles below are
//! the parts the model does have). Hiding the vanilla
//! first-person hands themselves: the SDK has no hand-hide — in
//! the Off hands modes the vanilla hands still draw over the
//! body's arms, and only Dynamic Mode's lowering (the hand
//! district) moves them. The Riptide step-aside: the pose state
//! exposes no spin signal. The Russian texts and the
//! `config/firstperson.json` file format go with the config
//! screen; the choices here are kept in the mod's own storage.
//! The pose state also does not say which hand is the main
//! hand, so the hands modes assume the vanilla default: main
//! hand on the right.

use horizon_mod_sdk::{
    HandTransform, Mod, PartOffset, PoseOffset, PoseTarget, Registration,
    entity_pose::{self, EntityPoseState},
    hand,
    input::{self, Event},
    render, storage,
    ui::{self, ButtonWidget, Widget},
};

/// The newest API level this mod needs: `entity-pose` is level
/// 20, `render.get-spyglass-zoom` level 19, `hand` level 16
/// (`crates/horizon-mods/src/api_level.rs`).
#[cfg(test)]
const API_LEVEL: u32 = 20;

/// F6 turns it on and off, the original's binding.
const TOGGLE_KEY: &str = "toggle";
const DEFAULT_TOGGLE_KEY: &str = "key.keyboard.f6";

/// The Vanilla Hands modes, in the original's order.
const HANDS_OFF: u32 = 0;
const HANDS_ALWAYS: u32 = 1;
const HANDS_BOTH: u32 = 2;
const HANDS_ITEM: u32 = 3;
const HANDS_NAMES: [&str; 4] = ["Off", "Always", "Always Both Hands", "Item In Hand"];

/// The per-pose offsets are distances back, in blocks, stepped
/// by 0.05 from 0 to 0.5. The defaults are indices into that
/// ladder: standing 0.25 (the port README's number), sneaking
/// 0.35 and sitting 0.30 (the description gives no numbers for
/// those two; these sit a step or two further back, where a
/// crouched or seated chest rides closer to the camera).
const OFFSET_STEP: f32 = 0.05;
const OFFSET_MAX_INDEX: u32 = 10;
const DEFAULT_STANDING: u32 = 5;
const DEFAULT_SNEAKING: u32 = 7;
const DEFAULT_SITTING: u32 = 6;
/// The swimming and crawling backs are the original's own, not
/// settings: a fixed step behind the standing offset's family.
const SWIM_BACK: f32 = 0.20;

/// Looking down past this, Dynamic Mode is engaged.
const DYNAMIC_PITCH: f32 = 25.0;

/// The settings page's widget ids.
const ENABLED_BUTTON: u32 = 0;
const HANDS_BUTTON: u32 = 1;
const DYNAMIC_BUTTON: u32 = 2;
const NO_HANDS_SWIM_BUTTON: u32 = 3;
const OFF_STANDING_BUTTON: u32 = 4;
const OFF_SNEAKING_BUTTON: u32 = 5;
const OFF_SITTING_BUTTON: u32 = 6;
const JACKET_BUTTON: u32 = 7;
const SLEEVES_BUTTON: u32 = 8;
const PANTS_BUTTON: u32 = 9;
const CAPE_BUTTON: u32 = 10;
const RESET_BUTTON: u32 = 11;

/// The storage keys.
const ENABLED_KEY: &str = "enabled";
const HANDS_KEY: &str = "hands";
const DYNAMIC_KEY: &str = "dynamic";
const NO_HANDS_SWIM_KEY: &str = "no_hands_swim";
const OFF_STANDING_KEY: &str = "off_standing";
const OFF_SNEAKING_KEY: &str = "off_sneaking";
const OFF_SITTING_KEY: &str = "off_sitting";
const JACKET_KEY: &str = "layer_jacket";
const SLEEVES_KEY: &str = "layer_sleeves";
const PANTS_KEY: &str = "layer_pants";
const CAPE_KEY: &str = "layer_cape";

/// Which pose the body is in, for the offsets — or None where
/// the mod steps aside and the body is not shown at all:
/// sleeping and gliding (the description's list), and half
/// swimming — the stroke's amount between strokes at the
/// surface, read here as a partial amount while not standing on
/// anything. A full amount standing on the ground is crawling
/// (the swim pose in a one-block gap); a full amount off the
/// ground is swimming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PoseClass {
    Standing,
    Sneaking,
    Sitting,
    Crawling,
    Swimming,
}

fn pose_class(state: &EntityPoseState) -> Option<PoseClass> {
    if state.sleeping || state.gliding {
        return None;
    }
    if state.swim_amount >= 0.9 {
        return Some(if state.on_ground {
            PoseClass::Crawling
        } else {
            PoseClass::Swimming
        });
    }
    if state.swim_amount > 0.1 && !state.on_ground {
        return None; // half swimming
    }
    if state.crouching {
        return Some(PoseClass::Sneaking);
    }
    if state.riding {
        return Some(PoseClass::Sitting);
    }
    Some(PoseClass::Standing)
}

/// The settings, as the page and the storage keep them.
#[derive(Clone)]
struct Settings {
    enabled: bool,
    hands_mode: u32,
    dynamic: bool,
    no_hands_swim: bool,
    off_standing: u32,
    off_sneaking: u32,
    off_sitting: u32,
    show_jacket: bool,
    show_sleeves: bool,
    show_pants: bool,
    show_cape: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            hands_mode: HANDS_OFF,
            dynamic: false,
            no_hands_swim: false,
            off_standing: DEFAULT_STANDING,
            off_sneaking: DEFAULT_SNEAKING,
            off_sitting: DEFAULT_SITTING,
            show_jacket: true,
            show_sleeves: true,
            show_pants: true,
            show_cape: true,
        }
    }
}

impl Settings {
    /// A changeable offset's distance back, in blocks.
    fn offset(&self, which: u32) -> f32 {
        let index = match which {
            OFF_STANDING_BUTTON => self.off_standing,
            OFF_SNEAKING_BUTTON => self.off_sneaking,
            _ => self.off_sitting,
        };
        index as f32 * OFFSET_STEP
    }

    fn save(&self) {
        let put = |key: &str, value: String| {
            let _ = storage::set(key, value.as_bytes());
        };
        put(ENABLED_KEY, flag(self.enabled).into());
        put(HANDS_KEY, self.hands_mode.to_string());
        put(DYNAMIC_KEY, flag(self.dynamic).into());
        put(NO_HANDS_SWIM_KEY, flag(self.no_hands_swim).into());
        put(OFF_STANDING_KEY, self.off_standing.to_string());
        put(OFF_SNEAKING_KEY, self.off_sneaking.to_string());
        put(OFF_SITTING_KEY, self.off_sitting.to_string());
        put(JACKET_KEY, flag(self.show_jacket).into());
        put(SLEEVES_KEY, flag(self.show_sleeves).into());
        put(PANTS_KEY, flag(self.show_pants).into());
        put(CAPE_KEY, flag(self.show_cape).into());
    }
}

fn flag(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
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
    format!("First-person Model: {}", on_off(enabled))
}

fn hands_label(mode: u32) -> String {
    format!("Vanilla Hands: {}", HANDS_NAMES[mode as usize])
}

fn dynamic_label(dynamic: bool) -> String {
    format!("Dynamic Mode: {}", on_off(dynamic))
}

fn no_hands_swim_label(on: bool) -> String {
    format!("Disable Vanilla Hands while Swimming: {}", on_off(on))
}

fn offset_label(name: &str, index: u32) -> String {
    format!("{} Offset: {:.2}", name, index as f32 * OFFSET_STEP)
}

fn layer_label(name: &str, shown: bool) -> String {
    format!("Show {}: {}", name, on_off(shown))
}

/// The body's offset for a pose: drawn back along its yaw (a
/// negative z in the model's own space, whose front is +z), and
/// while crawling or swimming the body follows the head — it
/// pitches with the look and rides up or down with the pitch,
/// as the original describes.
fn body_offset(settings: &Settings, class: PoseClass, state: &EntityPoseState) -> PartOffset {
    let back = match class {
        PoseClass::Standing => settings.offset(OFF_STANDING_BUTTON),
        PoseClass::Sneaking => settings.offset(OFF_SNEAKING_BUTTON),
        PoseClass::Sitting => settings.offset(OFF_SITTING_BUTTON),
        PoseClass::Crawling => settings.offset(OFF_STANDING_BUTTON),
        PoseClass::Swimming => SWIM_BACK,
    };
    let (pitch, y) = match class {
        PoseClass::Swimming => (
            state.head_pitch * 0.85,
            state.head_pitch.to_radians().sin() * 0.22,
        ),
        PoseClass::Crawling => (
            state.head_pitch * 0.7,
            state.head_pitch.to_radians().sin() * 0.15,
        ),
        _ => (0.0, 0.0),
    };
    PartOffset {
        part: "body".into(),
        x: 0.0,
        y,
        z: -back,
        pitch,
        yaw: 0.0,
        roll: 0.0,
    }
}

/// Dynamic Mode engaged: the player is looking down.
fn dynamic_down(settings: &Settings, state: &EntityPoseState) -> bool {
    settings.dynamic && state.head_pitch > DYNAMIC_PITCH
}

/// What Dynamic Mode does to the vanilla hands while looking
/// down: lowers them out of the body's way (the hand district's
/// offset), so the body's own arms read as the hands.
fn dynamic_hand(settings: &Settings, state: &EntityPoseState) -> Option<HandTransform> {
    if dynamic_down(settings, state) {
        Some(HandTransform {
            x: 0.0,
            y: -0.38,
            z: 0.06,
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        })
    } else {
        None
    }
}

/// The parts the body does not draw: the hands modes' arms and
/// the hidden layers, in the model's own part names. Vanilla
/// Hands hides the *model's* arms so the vanilla hands show
/// instead — Always the main arm, Always Both Hands both arms,
/// Item In Hand the arm of each hand holding an item. Dynamic
/// Mode looking down, and Disable Vanilla Hands while Swimming,
/// both suspend the modes: the body's arms show. The main hand
/// is the right arm (see the header note on handedness).
fn hidden_parts(settings: &Settings, class: PoseClass, state: &EntityPoseState) -> Vec<String> {
    let mut hidden: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        if !hidden.iter().any(|h| h == name) {
            hidden.push(name.into());
        }
    };
    let suspended =
        dynamic_down(settings, state) || (settings.no_hands_swim && class == PoseClass::Swimming);
    if !suspended {
        match settings.hands_mode {
            HANDS_ALWAYS => {
                push("right_arm");
                push("right_sleeve");
            }
            HANDS_BOTH => {
                push("right_arm");
                push("right_sleeve");
                push("left_arm");
                push("left_sleeve");
            }
            HANDS_ITEM => {
                if state.main_hand.is_some() {
                    push("right_arm");
                    push("right_sleeve");
                }
                if state.off_hand.is_some() {
                    push("left_arm");
                    push("left_sleeve");
                }
            }
            _ => {}
        }
    }
    if !settings.show_jacket {
        push("jacket");
    }
    if !settings.show_sleeves {
        push("right_sleeve");
        push("left_sleeve");
    }
    if !settings.show_pants {
        push("right_pants");
        push("left_pants");
    }
    if !settings.show_cape {
        push("cape");
    }
    hidden
}

struct FirstPersonModel {
    settings: Settings,
    /// The body is currently asked for (`set-first-person-body`
    /// sent and not yet withdrawn).
    shown: bool,
    /// A pose offset is outstanding on the self target.
    offset_written: bool,
    /// The vanilla hands are currently lowered (Dynamic Mode).
    hand_lowered: bool,
}

impl FirstPersonModel {
    fn refresh_settings(&self) {
        let s = &self.settings;
        ui::set_settings(&[
            Widget::Button(ButtonWidget {
                id: ENABLED_BUTTON,
                label: enabled_label(s.enabled),
            }),
            Widget::Button(ButtonWidget {
                id: HANDS_BUTTON,
                label: hands_label(s.hands_mode),
            }),
            Widget::Button(ButtonWidget {
                id: DYNAMIC_BUTTON,
                label: dynamic_label(s.dynamic),
            }),
            Widget::Button(ButtonWidget {
                id: NO_HANDS_SWIM_BUTTON,
                label: no_hands_swim_label(s.no_hands_swim),
            }),
            Widget::Button(ButtonWidget {
                id: OFF_STANDING_BUTTON,
                label: offset_label("Standing", s.off_standing),
            }),
            Widget::Button(ButtonWidget {
                id: OFF_SNEAKING_BUTTON,
                label: offset_label("Sneaking", s.off_sneaking),
            }),
            Widget::Button(ButtonWidget {
                id: OFF_SITTING_BUTTON,
                label: offset_label("Sitting", s.off_sitting),
            }),
            Widget::Button(ButtonWidget {
                id: JACKET_BUTTON,
                label: layer_label("Jacket", s.show_jacket),
            }),
            Widget::Button(ButtonWidget {
                id: SLEEVES_BUTTON,
                label: layer_label("Sleeves", s.show_sleeves),
            }),
            Widget::Button(ButtonWidget {
                id: PANTS_BUTTON,
                label: layer_label("Pants", s.show_pants),
            }),
            Widget::Button(ButtonWidget {
                id: CAPE_BUTTON,
                label: layer_label("Cape", s.show_cape),
            }),
            Widget::Button(ButtonWidget {
                id: RESET_BUTTON,
                label: "Reset to Defaults".into(),
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
        if self.hand_lowered {
            hand::clear_offset();
            self.hand_lowered = false;
        }
    }
}

impl Mod for FirstPersonModel {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(TOGGLE_KEY, "Toggle First-person Model", DEFAULT_TOGGLE_KEY)?;
        let d = Settings::default();
        let settings = Settings {
            enabled: read_switch(ENABLED_KEY, d.enabled),
            hands_mode: read_index(HANDS_KEY, HANDS_ITEM, d.hands_mode),
            dynamic: read_switch(DYNAMIC_KEY, d.dynamic),
            no_hands_swim: read_switch(NO_HANDS_SWIM_KEY, d.no_hands_swim),
            off_standing: read_index(OFF_STANDING_KEY, OFFSET_MAX_INDEX, d.off_standing),
            off_sneaking: read_index(OFF_SNEAKING_KEY, OFFSET_MAX_INDEX, d.off_sneaking),
            off_sitting: read_index(OFF_SITTING_KEY, OFFSET_MAX_INDEX, d.off_sitting),
            show_jacket: read_switch(JACKET_KEY, d.show_jacket),
            show_sleeves: read_switch(SLEEVES_KEY, d.show_sleeves),
            show_pants: read_switch(PANTS_KEY, d.show_pants),
            show_cape: read_switch(CAPE_KEY, d.show_cape),
        };
        Ok((
            FirstPersonModel {
                settings,
                shown: false,
                offset_written: false,
                hand_lowered: false,
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
        let class = me.and_then(pose_class);
        let scoping = render::get_spyglass_zoom().is_some();
        let want = self.settings.enabled && class.is_some() && !scoping;
        if !want {
            self.withdraw();
            return;
        }
        let (state, class) = (me.unwrap(), class.unwrap());
        if !self.shown {
            entity_pose::set_first_person_body(true);
            self.shown = true;
        }
        let offset = PoseOffset {
            parts: vec![body_offset(&self.settings, class, state)],
            hidden: hidden_parts(&self.settings, class, state),
        };
        entity_pose::set_pose_offset(PoseTarget::SelfPlayer, &offset);
        self.offset_written = true;
        match dynamic_hand(&self.settings, state) {
            Some(t) => {
                hand::set_offset(t);
                self.hand_lowered = true;
            }
            None => {
                if self.hand_lowered {
                    hand::clear_offset();
                    self.hand_lowered = false;
                }
            }
        }
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        let s = &mut self.settings;
        match id {
            ENABLED_BUTTON => s.enabled = !s.enabled,
            HANDS_BUTTON => s.hands_mode = (s.hands_mode + 1) % HANDS_NAMES.len() as u32,
            DYNAMIC_BUTTON => s.dynamic = !s.dynamic,
            NO_HANDS_SWIM_BUTTON => s.no_hands_swim = !s.no_hands_swim,
            OFF_STANDING_BUTTON => s.off_standing = (s.off_standing + 1) % (OFFSET_MAX_INDEX + 1),
            OFF_SNEAKING_BUTTON => s.off_sneaking = (s.off_sneaking + 1) % (OFFSET_MAX_INDEX + 1),
            OFF_SITTING_BUTTON => s.off_sitting = (s.off_sitting + 1) % (OFFSET_MAX_INDEX + 1),
            JACKET_BUTTON => s.show_jacket = !s.show_jacket,
            SLEEVES_BUTTON => s.show_sleeves = !s.show_sleeves,
            PANTS_BUTTON => s.show_pants = !s.show_pants,
            CAPE_BUTTON => s.show_cape = !s.show_cape,
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

horizon_mod_sdk::export_mod!(FirstPersonModel);

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
    fn poses_classify_and_step_asides_step_aside() {
        let mut s = state();
        assert_eq!(pose_class(&s), Some(PoseClass::Standing));
        s.crouching = true;
        assert_eq!(pose_class(&s), Some(PoseClass::Sneaking));
        s.crouching = false;
        s.riding = true;
        assert_eq!(pose_class(&s), Some(PoseClass::Sitting));
        s.riding = false;
        s.swim_amount = 1.0;
        assert_eq!(pose_class(&s), Some(PoseClass::Crawling));
        s.on_ground = false;
        assert_eq!(pose_class(&s), Some(PoseClass::Swimming));
        s.swim_amount = 0.5;
        assert_eq!(pose_class(&s), None, "half swimming steps aside");
        s.swim_amount = 0.0;
        s.gliding = true;
        assert_eq!(pose_class(&s), None, "gliding steps aside");
        s.gliding = false;
        s.sleeping = true;
        assert_eq!(pose_class(&s), None, "sleeping steps aside");
    }

    #[test]
    fn the_standing_offset_is_the_originals_quarter_block() {
        let settings = Settings::default();
        let s = state();
        let body = body_offset(&settings, PoseClass::Standing, &s);
        assert_eq!(body.part, "body");
        assert!((body.z + 0.25).abs() < 1e-6, "standing back {}", body.z);
        let sneak = body_offset(&settings, PoseClass::Sneaking, &s);
        assert!((sneak.z + 0.35).abs() < 1e-6);
        let sit = body_offset(&settings, PoseClass::Sitting, &s);
        assert!((sit.z + 0.30).abs() < 1e-6);
    }

    #[test]
    fn swimming_follows_the_head() {
        let settings = Settings::default();
        let mut s = state();
        s.head_pitch = 40.0;
        let body = body_offset(&settings, PoseClass::Swimming, &s);
        assert!((body.pitch - 34.0).abs() < 0.01, "pitch {}", body.pitch);
        assert!(body.y > 0.1, "looking down rides the body down: {}", body.y);
        s.head_pitch = -40.0;
        let body = body_offset(&settings, PoseClass::Swimming, &s);
        assert!(body.y < -0.1, "looking up rides it up: {}", body.y);
    }

    #[test]
    fn hands_modes_hide_the_models_arms() {
        let mut settings = Settings::default();
        let s = state();
        assert!(hidden_parts(&settings, PoseClass::Standing, &s).is_empty());
        settings.hands_mode = HANDS_ALWAYS;
        assert_eq!(
            hidden_parts(&settings, PoseClass::Standing, &s),
            ["right_arm", "right_sleeve"]
        );
        settings.hands_mode = HANDS_BOTH;
        assert_eq!(hidden_parts(&settings, PoseClass::Standing, &s).len(), 4);
        settings.hands_mode = HANDS_ITEM;
        assert!(hidden_parts(&settings, PoseClass::Standing, &s).is_empty());
        let mut s = state();
        s.main_hand = Some("minecraft:iron_sword".into());
        assert_eq!(
            hidden_parts(&settings, PoseClass::Standing, &s),
            ["right_arm", "right_sleeve"]
        );
        s.off_hand = Some("minecraft:shield".into());
        assert_eq!(hidden_parts(&settings, PoseClass::Standing, &s).len(), 4);
    }

    #[test]
    fn swimming_and_dynamic_suspend_the_hands_modes() {
        let mut settings = Settings::default();
        settings.hands_mode = HANDS_BOTH;
        settings.no_hands_swim = true;
        let s = state();
        assert!(
            hidden_parts(&settings, PoseClass::Swimming, &s).is_empty(),
            "no vanilla hands while swimming"
        );
        settings.dynamic = true;
        let mut s = state();
        s.head_pitch = 40.0;
        assert!(
            hidden_parts(&settings, PoseClass::Standing, &s).is_empty(),
            "dynamic looking down shows the body's arms"
        );
        assert!(dynamic_hand(&settings, &s).is_some());
        s.head_pitch = 10.0;
        assert!(dynamic_hand(&settings, &s).is_none());
        assert_eq!(hidden_parts(&settings, PoseClass::Standing, &s).len(), 4);
    }

    #[test]
    fn layer_toggles_hide_their_parts_once() {
        let mut settings = Settings::default();
        settings.hands_mode = HANDS_ALWAYS;
        settings.show_sleeves = false;
        settings.show_jacket = false;
        settings.show_pants = false;
        settings.show_cape = false;
        let hidden = hidden_parts(&settings, PoseClass::Standing, &state());
        assert_eq!(
            hidden,
            [
                "right_arm",
                "right_sleeve",
                "jacket",
                "left_sleeve",
                "right_pants",
                "left_pants",
                "cape"
            ]
        );
    }

    #[test]
    fn labels_name_the_originals_choices() {
        assert_eq!(enabled_label(true), "First-person Model: On");
        assert_eq!(hands_label(HANDS_BOTH), "Vanilla Hands: Always Both Hands");
        assert_eq!(hands_label(HANDS_ITEM), "Vanilla Hands: Item In Hand");
        assert_eq!(offset_label("Standing", 5), "Standing Offset: 0.25");
        assert_eq!(offset_label("Sitting", 0), "Sitting Offset: 0.00");
        assert_eq!(
            no_hands_swim_label(false),
            "Disable Vanilla Hands while Swimming: Off"
        );
    }
}

//! Classic Zoom for Kestrel: hold V to zoom 3x, scroll while holding to
//! zoom 1x-10x (x1.1 per wheel step), release to snap back instantly.
//! While V is held the mouse wheel zooms instead of switching hotbar slots,
//! and turning slows down in step with the zoom so aiming stays steady.
//! The key is rebindable on the mod's page in Options > Mods (the game
//! lists every key a mod registers there).
//!
//! Behaviour reimplemented from the public description of Classic Zoom by
//! SU4G3 (GPL-3.0-or-later original): this file shares no code with the
//! original. Credit to SU4G3 for the original mod. This port's home is
//! <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/classic-zoom>. The shape follows Horizon's official zoom mod (the SDK's
//! own template for `render.set-zoom` + `input`): a frame hook polls the
//! input events, claims the wheel while the key is held, and tells the
//! game the zoom in effect only when it changes (`set-zoom` lasts until
//! the next call, and a factor of 1 ends it).
use horizon_mod_sdk::{
    Mod, Registration,
    input::{self, Event},
    render::{self, Zoom as ZoomView},
};

/// The key's name for `register-key`, and its default.
const KEY: &str = "zoom";
const DEFAULT_KEY: &str = "key.keyboard.v";
/// The zoom the key starts at.
const START: f32 = 3.0;
/// The wheel's limits: no zoom, and Horizon's maximum.
const MIN: f32 = 1.0;
const MAX: f32 = 10.0;
/// Each wheel step multiplies the zoom by this.
const STEP: f32 = 1.1;

struct ClassicZoom {
    /// The zoom while the key is held; None when it is up.
    held: Option<f32>,
}

impl ClassicZoom {
    /// The key went down: start at 3x.
    fn press(&mut self) {
        self.held = Some(START);
    }

    /// The key came up: no zoom.
    fn release(&mut self) {
        self.held = None;
    }

    /// Wheel steps while the key is held: true when they zoom, false when
    /// the key is up and the wheel stays the hotbar's.
    fn wheel(&mut self, steps: f32) -> bool {
        let Some(z) = &mut self.held else {
            return false;
        };
        *z = (*z * STEP.powf(steps)).clamp(MIN, MAX);
        true
    }

    /// The zoom in effect: (factor, smooth, turning scale). Instant, never
    /// smooth, and turning slows in step with the zoom.
    fn view(&self) -> (f32, bool, f32) {
        let factor = self.held.unwrap_or(1.0);
        (factor, false, 1.0 / factor)
    }

    /// Tells the game the zoom in effect (a factor of 1 ends it).
    fn show(&self) {
        let (factor, smooth, turn) = self.view();
        render::set_zoom(ZoomView {
            factor,
            smooth,
            turn,
        });
    }
}

impl Mod for ClassicZoom {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key(KEY, "Zoom key", DEFAULT_KEY)?;
        Ok((ClassicZoom { held: None }, Registration::new().on_frame()))
    }

    fn frame(&mut self, _seconds: f32) {
        let mut changed = false;
        for event in input::poll() {
            match event {
                Event::KeyDown(k) if k == KEY => {
                    self.press();
                    // While held, the wheel zooms instead of moving the hotbar.
                    input::claim_wheel(true);
                }
                Event::KeyUp(k) if k == KEY => {
                    self.release();
                    input::claim_wheel(false);
                }
                Event::Wheel(steps) => {
                    if !self.wheel(steps) {
                        continue;
                    }
                }
                _ => continue,
            }
            changed = true;
        }
        if changed {
            self.show();
        }
    }
}

horizon_mod_sdk::export_mod!(ClassicZoom);

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn holds_at_three_and_lets_go() {
        let mut zoom = ClassicZoom { held: None };
        assert_eq!(zoom.view(), (1.0, false, 1.0));
        assert!(
            !zoom.wheel(1.0),
            "the wheel is the hotbar's while the key is up"
        );
        zoom.press();
        assert_eq!(zoom.view(), (3.0, false, 1.0 / 3.0));
        zoom.release();
        assert_eq!(zoom.view(), (1.0, false, 1.0));
        assert!(!zoom.wheel(1.0));
    }

    #[test]
    fn the_wheel_zooms_by_a_tenth_within_one_and_ten() {
        let mut zoom = ClassicZoom { held: None };
        zoom.press();
        assert!(zoom.wheel(1.0));
        assert!(close(zoom.view().0, 3.3));
        assert!(zoom.wheel(-2.0));
        assert!(close(zoom.view().0, 3.3 / 1.21));
        assert!(zoom.wheel(100.0));
        assert_eq!(zoom.view().0, 10.0);
        assert!(zoom.wheel(-100.0));
        assert_eq!(zoom.view().0, 1.0);
        // Pressing again starts over at 3x.
        zoom.release();
        zoom.press();
        assert_eq!(zoom.view().0, 3.0);
    }
}

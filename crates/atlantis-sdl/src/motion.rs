//! The gamepad's motion maths: the sticks' radial dead zone and curve, the
//! pointer's speed, the D-pad's ramp and the panoramic view's turn.
//!
//! Adapted from Klaas Van den Broeck (@Klaasvdb),
//! github.com/Klaasvdb/atlantis-native-handheld (GPL-3.0-or-later), with
//! these changes: the speeds are per second and applied once per engine
//! frame with that frame's duration (the motion no longer depends on how
//! often the main loop turns); the dead zone and the curve are radial (on
//! the stick's tilt, not on each axis); the D-pad starts slow and speeds
//! up while held.
//!
//! Nothing here knows SDL: the front end gives the sticks' raw values and
//! the buttons held, [`Motion::step`] gives the pointer's new position or
//! the view's turn.

use atlantis_engine::warp::{SCREEN_H, SCREEN_W};

/// The speeds and ramps (`controls.ini` scales the two base speeds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// The pointer at full tilt, in screen pixels per second.
    pub pointer_speed: f32,
    /// The view's turn at full tilt (before the boosts), in mouse pixels
    /// per second.
    pub look_speed: f32,
    /// The right stick's share of the speed (fine aim).
    pub fine: f32,
    /// The speed with L1 held, with R1 held.
    pub slow: f32,
    pub fast: f32,
    /// The D-pad's speed when pressed, as a share of full tilt, and the
    /// seconds it takes to reach full tilt.
    pub dpad_start: f32,
    pub dpad_ramp: f32,
    /// The view: beyond half tilt the gain grows to `look_tilt_max` at
    /// full tilt; held near full tilt for `look_delay` seconds, it grows
    /// over `look_ramp` seconds to `look_max` times.
    pub look_tilt_max: f32,
    pub look_delay: f32,
    pub look_ramp: f32,
    pub look_max: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            // (Klaas's 7 and 10 pixels per frame at 60 frames a second)
            pointer_speed: 420.0,
            look_speed: 600.0,
            fine: 0.35,
            slow: 0.35,
            fast: 2.0,
            dpad_start: 0.3,
            dpad_ramp: 0.5,
            look_tilt_max: 2.0,
            look_delay: 1.0 / 3.0,
            look_ramp: 1.5,
            look_max: 3.0,
        }
    }
}

/// A stick's raw axes (-32768..32767) as a vector of length 0..1: 0 inside
/// the dead zone (a circle of radius `deadzone`), then the tilt beyond it
/// squared (small tilts slow, for aiming), in the stick's direction.
pub fn radial(x: i16, y: i16, deadzone: i16) -> (f32, f32) {
    let (vx, vy) = (f32::from(x) / 32767.0, f32::from(y) / 32767.0);
    let m = vx.hypot(vy);
    let dz = f32::from(deadzone.max(0)) / 32767.0;
    if m <= dz || m == 0.0 {
        return (0.0, 0.0);
    }
    let t = ((m - dz) / (1.0 - dz).max(f32::EPSILON)).min(1.0);
    let k = t * t / m;
    (vx * k, vy * k)
}

/// What pushes the pointer this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Inputs {
    /// The sticks after [`radial`].
    pub left: (f32, f32),
    pub right: (f32, f32),
    /// The D-pad held: -1, 0 or 1 on each axis (x right, y down).
    pub dpad: (i8, i8),
    /// L1, R1 held.
    pub slow: bool,
    pub fast: bool,
}

/// What a frame's motion does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    None,
    /// The pointer moved there (the mouse free).
    Pointer(i32, i32),
    /// The view turns by this mouse motion (the mouse locked).
    Look(i32, i32),
}

/// The pointer's state between frames.
#[derive(Clone, Debug)]
pub struct Motion {
    pub tuning: Tuning,
    /// The pointer, in screen pixels (fractions kept).
    pointer: (f32, f32),
    /// The view's motion not sent yet (fractions kept).
    look_acc: (f32, f32),
    /// For how long the view has turned near full tilt, in seconds.
    look_held: f32,
    /// For how long the D-pad has been held, in seconds.
    dpad_held: f32,
}

impl Motion {
    pub fn new(tuning: Tuning) -> Motion {
        Motion {
            tuning,
            pointer: (SCREEN_W as f32 / 2.0, SCREEN_H as f32 / 2.0),
            look_acc: (0.0, 0.0),
            look_held: 0.0,
            dpad_held: 0.0,
        }
    }

    /// The pointer moved by the engine (or the mouse lock taken or let
    /// go): the pad goes on from there.
    pub fn set_pointer(&mut self, (x, y): (i32, i32)) {
        self.pointer = (x as f32, y as f32);
        self.look_acc = (0.0, 0.0);
    }

    /// The direction and speed asked for, as a share of full tilt on each
    /// axis, the D-pad's ramp advanced by `dt` seconds; `None` when nothing
    /// pushes.
    fn push(&mut self, inp: &Inputs, dt: f32) -> Option<(f32, f32)> {
        let t = &self.tuning;
        let dpad = if inp.dpad != (0, 0) {
            let share = t.dpad_start
                + (1.0 - t.dpad_start) * (self.dpad_held / t.dpad_ramp.max(f32::EPSILON)).min(1.0);
            self.dpad_held += dt;
            let (x, y) = (f32::from(inp.dpad.0), f32::from(inp.dpad.1));
            // (a diagonal as fast as a straight line)
            let n = x.hypot(y);
            (x / n * share, y / n * share)
        } else {
            self.dpad_held = 0.0;
            (0.0, 0.0)
        };
        let x = inp.left.0 + dpad.0 + inp.right.0 * t.fine;
        let y = inp.left.1 + dpad.1 + inp.right.1 * t.fine;
        if x == 0.0 && y == 0.0 {
            return None;
        }
        let k = if inp.slow {
            t.slow
        } else if inp.fast {
            t.fast
        } else {
            1.0
        };
        Some((x * k, y * k))
    }

    /// One engine frame of `dt` seconds: the pointer moves (`locked`
    /// false) or the view turns (`locked` true).
    pub fn step(&mut self, inp: &Inputs, dt: f32, locked: bool) -> Step {
        let dt = dt.clamp(0.0, 0.1);
        let Some(push) = self.push(inp, dt) else {
            self.look_held = 0.0;
            return Step::None;
        };
        if locked {
            let (dx, dy) = self.look(push, dt);
            if dx == 0 && dy == 0 {
                Step::None
            } else {
                Step::Look(dx, dy)
            }
        } else {
            self.look_held = 0.0;
            let before = (self.pointer.0 as i32, self.pointer.1 as i32);
            let v = self.tuning.pointer_speed * dt;
            self.pointer.0 = (self.pointer.0 + push.0 * v).clamp(0.0, SCREEN_W as f32 - 1.0);
            self.pointer.1 = (self.pointer.1 + push.1 * v).clamp(0.0, SCREEN_H as f32 - 1.0);
            let after = (self.pointer.0 as i32, self.pointer.1 as i32);
            if after == before {
                Step::None
            } else {
                Step::Pointer(after.0, after.1)
            }
        }
    }

    /// The mouse motion that turns the view by `push` for `dt` seconds.
    ///
    /// The tilt sets the gain radially: beyond half tilt it grows to
    /// `look_tilt_max` at full tilt. Held near full tilt, the turn also
    /// speeds up with time (after `look_delay`, over `look_ramp`, to
    /// `look_max` times); a smaller tilt (aiming) keeps its speed.
    fn look(&mut self, (mx, my): (f32, f32), dt: f32) -> (i32, i32) {
        let t = self.tuning;
        let tilt = mx.hypot(my);
        let r = ((tilt - 0.5) / 0.5).clamp(0.0, 1.0);
        let radial = 1.0 + r * r * (t.look_tilt_max - 1.0);
        let ramp =
            ((self.look_held - t.look_delay) / t.look_ramp.max(f32::EPSILON)).clamp(0.0, 1.0);
        if tilt >= 0.7 {
            self.look_held += dt;
        } else {
            self.look_held = 0.0;
        }
        let gain = t.look_speed * dt * radial * (1.0 + ramp * (t.look_max - 1.0));
        self.look_acc.0 += mx * gain;
        self.look_acc.1 += my * gain;
        let (dx, dy) = (self.look_acc.0.trunc(), self.look_acc.1.trunc());
        self.look_acc.0 -= dx;
        self.look_acc.1 -= dy;
        (dx as i32, dy as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DZ: i16 = 6000;

    /// Moves the pointer for `secs` seconds in steps of `1/rate` s.
    fn run_pointer(inp: Inputs, rate: u32, secs: f32) -> (i32, i32) {
        let mut m = Motion::new(Tuning::default());
        m.set_pointer((0, 0));
        let mut at = (0, 0);
        for _ in 0..(secs * rate as f32).round() as u32 {
            if let Step::Pointer(x, y) = m.step(&inp, 1.0 / rate as f32, false) {
                at = (x, y);
            }
        }
        at
    }

    /// Turns the view for `secs` seconds in steps of `1/rate` s.
    fn run_look(inp: Inputs, rate: u32, secs: f32) -> (i32, i32) {
        let mut m = Motion::new(Tuning::default());
        let mut sum = (0, 0);
        for _ in 0..(secs * rate as f32).round() as u32 {
            if let Step::Look(dx, dy) = m.step(&inp, 1.0 / rate as f32, true) {
                sum.0 += dx;
                sum.1 += dy;
            }
        }
        sum
    }

    #[test]
    fn the_dead_zone_is_a_circle() {
        // Inside on each axis but outside as a whole: it moves (a per-axis
        // dead zone would not).
        let a = (DZ as f32 * 0.8) as i16;
        let (x, y) = radial(a, a, DZ);
        assert!(x > 0.0 && y > 0.0);
        assert!((x - y).abs() < 1e-6, "the direction kept");
        // Inside the circle: nothing.
        assert_eq!(radial(DZ - 1, 0, DZ), (0.0, 0.0));
        assert_eq!(radial(0, -(DZ - 1), DZ), (0.0, 0.0));
        let b = (DZ as f32 * 0.7) as i16;
        assert_eq!(radial(b, b, DZ), (0.0, 0.0));
        // Full tilt: length 1, whatever the direction (a square gate's
        // corner too).
        let (x, y) = radial(32767, 0, DZ);
        assert!((x - 1.0).abs() < 1e-6 && y == 0.0);
        let (x, y) = radial(-32768, 32767, DZ);
        assert!((x.hypot(y) - 1.0).abs() < 1e-5);
        assert!(x < 0.0 && y > 0.0);
        let d = (32767.0 / 2f32.sqrt()) as i16;
        let (x, y) = radial(d, d, DZ);
        assert!((x.hypot(y) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn the_curve_keeps_small_tilts_slow() {
        let half = radial(DZ + (32767 - DZ) / 2, 0, DZ).0;
        assert!((half - 0.25).abs() < 1e-3, "{half}");
        // No dead zone at all is allowed.
        assert_eq!(radial(0, 0, 0), (0.0, 0.0));
        assert!((radial(16384, 0, 0).0 - 0.25).abs() < 1e-3);
    }

    #[test]
    fn the_pointer_speed_does_not_depend_on_the_loop_rate() {
        let inp = Inputs {
            left: (0.5, 0.25),
            ..Inputs::default()
        };
        let at60 = run_pointer(inp, 60, 0.5);
        let at250 = run_pointer(inp, 250, 0.5);
        let at30 = run_pointer(inp, 30, 0.5);
        // 420 px/s * 0.5 s * 0.5 = 105, * 0.25 = 52.5
        assert!(
            (at60.0 - 105).abs() <= 1 && (at60.1 - 52).abs() <= 1,
            "{at60:?}"
        );
        for at in [at250, at30] {
            assert!(
                (at.0 - at60.0).abs() <= 1 && (at.1 - at60.1).abs() <= 1,
                "{at:?} {at60:?}"
            );
        }
    }

    #[test]
    fn the_view_turn_does_not_depend_on_the_loop_rate() {
        for left in [(0.4, 0.0), (1.0, 0.0), (0.0, -0.9)] {
            let inp = Inputs {
                left,
                ..Inputs::default()
            };
            // Over the ramp too (2 s: delay 1/3 s + ramp 1.5 s).
            let a = run_look(inp, 60, 2.0);
            let b = run_look(inp, 240, 2.0);
            let tol = |v: i32| (v.abs() / 50).max(2);
            assert!((a.0 - b.0).abs() <= tol(a.0), "{a:?} {b:?}");
            assert!((a.1 - b.1).abs() <= tol(a.1), "{a:?} {b:?}");
        }
        // 0.4 tilt: no boost, 600 px/s.
        let a = run_look(
            Inputs {
                left: (0.4, 0.0),
                ..Inputs::default()
            },
            60,
            1.0,
        );
        assert!((a.0 - 240).abs() <= 1, "{a:?}");
    }

    #[test]
    fn the_view_speeds_up_held_at_full_tilt() {
        let inp = Inputs {
            left: (1.0, 0.0),
            ..Inputs::default()
        };
        let mut m = Motion::new(Tuning::default());
        let per_frame = |m: &mut Motion| match m.step(&inp, 1.0 / 60.0, true) {
            Step::Look(dx, _) => dx,
            _ => 0,
        };
        // Full tilt: 600 px/s * 2 = 20 px a frame at first.
        let first = per_frame(&mut m);
        assert_eq!(first, 20);
        for _ in 0..120 {
            per_frame(&mut m);
        }
        // After 2 s: three times as fast.
        let later = per_frame(&mut m);
        assert_eq!(later, 60);
        // Let go: the next turn starts slow again.
        assert_eq!(m.step(&Inputs::default(), 1.0 / 60.0, true), Step::None);
        assert_eq!(per_frame(&mut m), 20);
    }

    #[test]
    fn the_dpad_starts_slow_and_ramps_to_full_speed() {
        let inp = Inputs {
            dpad: (1, 0),
            ..Inputs::default()
        };
        let mut m = Motion::new(Tuning::default());
        m.set_pointer((0, 0));
        // The first frame: 30 % of 420 px/s = 2.1 px.
        assert_eq!(m.step(&inp, 1.0 / 60.0, false), Step::Pointer(2, 0));
        let mut x = 2;
        let mut last = 0;
        for _ in 0..60 {
            if let Step::Pointer(nx, _) = m.step(&inp, 1.0 / 60.0, false) {
                last = nx - x;
                x = nx;
            }
        }
        // After the ramp: 7 px a frame (420 / 60).
        assert_eq!(last, 7);
        // The ramp is in seconds: at 240 frames a second it is as far
        // after the same time.
        let a = run_pointer(inp, 60, 1.0);
        let b = run_pointer(inp, 240, 1.0);
        assert!((a.0 - b.0).abs() <= 2, "{a:?} {b:?}");
        // Released: the next press starts slow again.
        m.step(&Inputs::default(), 1.0 / 60.0, false);
        let before = x;
        if let Step::Pointer(nx, _) = m.step(&inp, 1.0 / 60.0, false) {
            assert!(nx - before <= 3);
        }
    }

    #[test]
    fn a_dpad_diagonal_is_no_faster() {
        let straight = run_pointer(
            Inputs {
                dpad: (1, 0),
                ..Inputs::default()
            },
            60,
            0.5,
        );
        let diag = run_pointer(
            Inputs {
                dpad: (1, 1),
                ..Inputs::default()
            },
            60,
            0.5,
        );
        let d = ((diag.0 * diag.0 + diag.1 * diag.1) as f32).sqrt();
        assert!(
            (d - straight.0 as f32).abs() <= 2.0,
            "{diag:?} {straight:?}"
        );
    }

    #[test]
    fn the_shoulders_and_the_fine_stick_scale_the_speed() {
        let base = Inputs {
            left: (1.0, 0.0),
            ..Inputs::default()
        };
        let normal = run_pointer(base, 60, 0.5).0;
        let slow = run_pointer(Inputs { slow: true, ..base }, 60, 0.5).0;
        let fast = run_pointer(Inputs { fast: true, ..base }, 60, 0.5).0;
        assert_eq!(normal, 210);
        assert!((slow - 73).abs() <= 1, "{slow}");
        assert!((fast - 420).abs() <= 1, "{fast}");
        let fine = run_pointer(
            Inputs {
                right: (1.0, 0.0),
                ..Inputs::default()
            },
            60,
            0.5,
        )
        .0;
        assert!((fine - 73).abs() <= 1, "{fine}");
    }

    #[test]
    fn the_pointer_stays_on_the_screen_and_a_long_frame_is_capped() {
        let mut m = Motion::new(Tuning::default());
        let inp = Inputs {
            left: (-1.0, 1.0),
            ..Inputs::default()
        };
        for _ in 0..200 {
            m.step(&inp, 1.0 / 60.0, false);
        }
        m.set_pointer((0, SCREEN_H as i32 - 1));
        assert_eq!(m.step(&inp, 1.0 / 60.0, false), Step::None);
        // A 2 s hitch moves as far as 0.1 s.
        m.set_pointer((0, 0));
        let r = Inputs {
            left: (1.0, 0.0),
            ..Inputs::default()
        };
        assert_eq!(m.step(&r, 2.0, false), Step::Pointer(42, 0));
    }
}

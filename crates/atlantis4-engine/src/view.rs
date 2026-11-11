//! `GameWarp`'s view angles: yaw and pitch in degrees with four
//! "remaining range" limits (`GameWarp` = `Application+0x39d8`, fields
//! `+0x35e8..+0x35fc`, atlantis4-gog-en-2363910).
//!
//! - Setters `setYaw` @ 0x00521e30 and `setPitch` @ 0x00521d70: the change
//!   `d = target − current` is clamped to `[−minus, +plus]`, then `plus −=
//!   d`, `minus += d`; the yaw is then wrapped (`yaw − 360·trunc(yaw /
//!   360)`, 0 unless within (−360, 360)), the pitch clamped to [−55, 45]
//!   (the ranges are not adjusted by that clamp).
//! - Limits reset by every `ChangeWarp` (@ 0x00526ff8, @ 0x00527d55; the
//!   function @ 0x00521c60): yaw ±FLT_MAX, pitch `plus = 45 − θ`, `minus =
//!   θ + 55`. `SetViewAngleXLimits(a, b)` / `SetViewAngleYLimits(a, b)` write
//!   ranges relative to the current angles.
//! - `LookAt(x, y)`: yaw `180 − x`, pitch `−(y ≥ 90 ? y − 360 : y)`
//!   through the setters.
//! - Mouse look (`0x0052ab40`, dragging): `Δ` = the pointer's move in input
//!   pixels, y up; `ψ −= 0.5·Δx`, `θ += 0.5·Δy` (signs flipped with
//!   `InverseLook`).
//!
//! The setters run on the x87 with float fields: here each operation is in
//! double precision and each store rounds to `f32`.

/// The pitch's bounds (@ 0x00521df2..0x00521e26).
pub const PITCH_MIN: f32 = -55.0;
pub const PITCH_MAX: f32 = 45.0;

/// Degrees of view per input pixel of drag (@ 0x0052ad42..0x0052adcf).
pub const DRAG_DEGREES_PER_PIXEL: f32 = 0.5;

/// The view angles and their remaining ranges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub yaw: f32,
    pub pitch: f32,
    pub yaw_plus: f32,
    pub yaw_minus: f32,
    pub pitch_plus: f32,
    pub pitch_minus: f32,
}

impl Default for View {
    /// A new game (@ 0x005296bf): yaw = pitch = 0, the limits reset.
    fn default() -> View {
        let mut v = View {
            yaw: 0.0,
            pitch: 0.0,
            yaw_plus: 0.0,
            yaw_minus: 0.0,
            pitch_plus: 0.0,
            pitch_minus: 0.0,
        };
        v.reset_limits();
        v
    }
}

/// The clamped target of a setter and the ranges after it.
fn step(cur: f32, target: f32, plus: &mut f32, minus: &mut f32) -> f32 {
    let (c, p, m) = (f64::from(cur), f64::from(*plus), f64::from(*minus));
    let mut v = f64::from(target);
    let d = v - c;
    if d > p {
        v = (c + p) as f32 as f64;
    }
    if d + m <= 0.0 {
        v = (v - (d + m)) as f32 as f64;
    }
    let moved = v - c;
    *plus = (p - moved) as f32;
    *minus = (m + moved) as f32;
    v as f32
}

impl View {
    /// `0x00521c60`: no yaw limit, the pitch free within [−55, 45].
    pub fn reset_limits(&mut self) {
        self.yaw_plus = f32::MAX;
        self.yaw_minus = f32::MAX;
        self.pitch_plus = (f64::from(PITCH_MAX) - f64::from(self.pitch)) as f32;
        self.pitch_minus = (f64::from(self.pitch) - f64::from(PITCH_MIN)) as f32;
    }

    /// `setYaw` (`0x00521e30`).
    pub fn set_yaw(&mut self, target: f32) {
        let v = step(self.yaw, target, &mut self.yaw_plus, &mut self.yaw_minus);
        let v = f64::from(v);
        // `_ftol` truncates towards zero.
        let turns = (v / 360.0).trunc();
        let w = (v - 360.0 * turns) as f32;
        self.yaw = if w > -360.0 && w < 360.0 { w } else { 0.0 };
    }

    /// `setPitch` (`0x00521d70`).
    pub fn set_pitch(&mut self, target: f32) {
        let v = step(
            self.pitch,
            target,
            &mut self.pitch_plus,
            &mut self.pitch_minus,
        );
        self.pitch = v.clamp(PITCH_MIN, PITCH_MAX);
    }

    /// `SetViewAngleXLimits(a, b)`: the yaw may grow by `a` and shrink by
    /// `b` from here.
    pub fn set_yaw_limits(&mut self, plus: f32, minus: f32) {
        self.yaw_plus = plus;
        self.yaw_minus = minus;
    }

    /// `SetViewAngleYLimits(a, b)`: the pitch may shrink by `a` and grow
    /// by `b` from here.
    pub fn set_pitch_limits(&mut self, minus: f32, plus: f32) {
        self.pitch_minus = minus;
        self.pitch_plus = plus;
    }

    /// `LookAt(x, y)` of the scripts: script x = 0 looks at +Z, script
    /// y > 0 looks down.
    pub fn look_at(&mut self, x: f32, y: f32) {
        self.set_yaw((180.0 - f64::from(x)) as f32);
        let p = if y >= 90.0 {
            (f64::from(y) - 360.0) as f32
        } else {
            y
        };
        self.set_pitch(-p);
    }

    /// A drag of `dx`, `dy_up` input pixels (y up) in a frame.
    pub fn drag(&mut self, dx: f32, dy_up: f32, inverse_look: bool) {
        let k = f64::from(DRAG_DEGREES_PER_PIXEL);
        let s = if inverse_look { -1.0 } else { 1.0 };
        let yaw = (f64::from(self.yaw) - s * k * f64::from(dx)) as f32;
        self.set_yaw(yaw);
        let pitch = (f64::from(self.pitch) + s * k * f64::from(dy_up)) as f32;
        self.set_pitch(pitch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_game_looks_ahead_with_the_pitch_limits() {
        let v = View::default();
        assert_eq!(
            (v.yaw, v.pitch, v.pitch_plus, v.pitch_minus),
            (0.0, 0.0, 45.0, 55.0)
        );
    }

    #[test]
    fn half_a_degree_per_pixel() {
        let mut v = View::default();
        v.drag(10.0, 4.0, false);
        assert_eq!((v.yaw, v.pitch), (-5.0, 2.0));
        v.drag(0.0, 200.0, false);
        assert_eq!(v.pitch, 45.0);
        v.drag(10.0, 0.0, true);
        assert_eq!(v.yaw, 0.0);
    }

    #[test]
    fn look_at_turns_script_angles_into_view_angles() {
        let mut v = View::default();
        v.look_at(0.0, 0.0);
        assert_eq!((v.yaw, v.pitch), (180.0, 0.0));
        v.look_at(90.0, 350.0);
        assert_eq!((v.yaw, v.pitch), (90.0, 10.0));
    }
}

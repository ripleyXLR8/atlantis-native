//! The turn of a move: before the player walks to another view, the view
//! turns to the move's start angles in a straight line in (yaw, pitch),
//! accelerating then braking, one step per rendered frame (set-up
//! @ 0x0045b690, step @ 0x0045b910).
//!
//! The step is float code: each value is transcribed with the x87
//! operations and float stores of the original, under the control word the
//! game runs with ([`Fpu`]). The zoom (`ZoomStart`) and the roll
//! (`ZoomPsychoStart`) that can ride on the same step are not ported:
//! Tibet uses neither.

use crate::camera::Camera;
use crate::fpu::Fpu;

/// π as the float @ 0x005c678c.
const PI: f32 = f32::from_bits(0x4049_0fdb);
/// 1/1800 @ 0x005c6358 (a double): π × it is 0.1°, the snap distance.
const TENTH_DEGREE: f64 = 0.000_555_555_555_555_555_6;
/// 1/12 @ 0x005c6354: π × it is 15°, the braking distance.
const BRAKE: f32 = f32::from_bits(0x3daa_aaab);
/// 1/90 @ 0x005c6350: π × it is 2°, the top speed test.
const TOP: f32 = f32::from_bits(0x3c36_0b61);
/// 1/18 @ 0x005c634c: π × it is 10°.
const RAMP: f32 = f32::from_bits(0x3d63_8e39);

#[inline(always)]
fn d(x: f32) -> f64 {
    f64::from(x)
}

/// A turn in progress (the globals @ 0x006614d0..0x00661660).
#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    /// Δpitch, Δyaw (wrapped into [−π, π]) and the distance D, radians
    /// (@ 0x006614d4, 0x006614dc, 0x006614d8).
    pub d_pitch: f32,
    pub d_yaw: f32,
    pub dist: f32,
    /// The distance done s (@ 0x00661634) and the speed v (@ 0x00661660).
    pub done: f32,
    pub speed: f32,
    /// The acceleration, 0.2° (@ 0x006614d0).
    pub accel: f32,
    /// D − 0.1°: past it the turn snaps to its end (@ 0x00661630).
    pub snap: f32,
    /// Where the braking starts (@ 0x006615a8).
    pub brake: f32,
}

impl Turn {
    /// FUN_0045b690: a turn from the camera's direction to (yaw, pitch);
    /// `None` when the distance is under 0.1° (the move then waits for
    /// nothing, see the caller). No zoom is ever pending here.
    pub fn new(cam: &Camera, yaw: f32, pitch: f32) -> (Turn, bool) {
        let fpu = cam.fpu;
        let (yaw0, pitch0) = cam.angles();
        let d_pitch = fpu.f32(fpu.sub(d(pitch), d(pitch0)));
        let mut d_yaw = fpu.f32(fpu.sub(d(yaw), d(yaw0)));
        let two_pi = d(PI) + d(PI); // `fadd st0, st0`: exact
        if -d(PI) > d(d_yaw) {
            d_yaw = fpu.f32(fpu.add(two_pi, d(d_yaw)));
        }
        if d(d_yaw) > d(PI) {
            d_yaw = fpu.f32(fpu.sub(d(d_yaw), two_pi));
        }
        let dist = fpu
            .f32(fpu.sqrt(fpu.add(fpu.mul(d(d_yaw), d(d_yaw)), fpu.mul(d(d_pitch), d(d_pitch)))));
        let tenth = fpu.mul(d(PI), TENTH_DEGREE);
        let snap = fpu.f32(fpu.sub(d(dist), tenth));
        let q = fpu.f32(fpu.mul(d(PI), d(BRAKE)));
        let mut brake = fpu.f32(fpu.sub(d(dist), d(q)));
        let a = fpu.mul(d(PI), d(TOP));
        let b = fpu.f32(fpu.mul(d(PI), d(RAMP)));
        let accel = fpu.f32(fpu.div(fpu.mul(a, a), d(b) + d(b)));
        let turning = d(dist) >= tenth;
        if turning {
            // Under 25° the braking starts at 15 / 25 of the way.
            let bq = fpu.add(d(b), d(q));
            if bq > d(dist) {
                brake = fpu.f32(fpu.mul(fpu.div(d(q), bq), d(dist)));
            }
        }
        (
            Turn {
                d_pitch,
                d_yaw,
                dist,
                done: 0.0,
                speed: 0.0,
                accel,
                snap,
                brake,
            },
            turning,
        )
    }

    /// FUN_0045b910: one step; turns the camera (yaw about the world's Y
    /// axis, pitch about its own X axis, as the mouse does) and returns
    /// whether the turn goes on.
    pub fn step(&mut self, cam: &mut Camera) -> bool {
        let fpu: Fpu = cam.fpu;
        let prev = self.done;
        if self.done >= self.brake {
            let rem = fpu.sub(d(self.dist), d(self.done));
            let t = fpu.f32(fpu.div(fpu.mul(d(self.speed), d(self.speed)), rem + rem));
            let v = fpu.sub(d(self.speed), d(t));
            self.speed = fpu.f32(v);
            if v < 0.0 {
                self.done = fpu.f32(fpu.sub(d(self.dist), d(self.speed)));
            }
        } else {
            let top = fpu.mul(d(PI), d(TOP));
            if top > d(self.speed) {
                self.speed = fpu.f32(fpu.add(d(self.accel), d(self.speed)));
            }
        }
        let s = fpu.add(d(self.speed), d(self.done));
        self.done = fpu.f32(s);
        if s < 0.0 || self.done > self.snap {
            self.done = self.dist;
        }
        let f = fpu.div(fpu.sub(d(self.done), d(prev)), d(self.dist));
        let yaw = fpu.f32(fpu.mul(f, d(self.d_yaw)));
        let pitch = fpu.f32(fpu.mul(f, d(self.d_pitch)));
        cam.yaw(-yaw);
        cam.pitch(-pitch);
        self.done != self.dist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        let cam = Camera::new(Fpu::Double, 640, 480);
        let (t, turning) = Turn::new(&cam, (90f32 + 10.0).to_radians(), 0.0);
        assert!(turning);
        assert!((t.accel.to_degrees() - 0.2).abs() < 1e-5);
        // A 10° turn brakes from 6°.
        assert!((t.brake.to_degrees() - 6.0).abs() < 1e-4);
        // Under 0.1°: no turn.
        let (_, turning) = Turn::new(&cam, (90f32 + 0.05).to_radians(), 0.0);
        assert!(!turning);
    }

    #[test]
    fn a_ten_degree_turn() {
        // Golden case D: view 3 at (80, 0) to (90, 0) takes 14 frames,
        // 0.2°, 0.4° … then braking.
        let mut cam = Camera::new(Fpu::Chop, 640, 480);
        cam.enter(80f32.to_radians(), 0.0);
        let (mut t, _) = Turn::new(&cam, 90f32.to_radians(), 0.0);
        let mut n = 0;
        while t.step(&mut cam) {
            n += 1;
        }
        assert_eq!(n + 1, 14);
        let (y, _) = cam.angles();
        assert!((y.to_degrees() - 90.0).abs() < 1e-3);
    }
}

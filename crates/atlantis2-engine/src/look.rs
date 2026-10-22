//! Turning the view: the mouse gives the camera two speeds, which decay
//! every frame, and the angles are kept within the script's limits.

use crate::camera::Camera;
use crate::fpu::{f, Fpu};

/// The mouse sensitivity of the data section (@ 0x005da268): a mouse move
/// of `d` mickeys adds `d / 2000` radians to a turning speed.
pub const DEFAULT_SENSITIVITY: i32 = 2000;

/// `LIMITEANGLEWARPY` at start-up and after every view change: ±70°.
pub const DEFAULT_LIMIT_Y: i32 = 70;

/// No limit (`LIMITEANGLEWARPX` at start-up; `999` in a script).
pub const NO_LIMIT: i32 = -1;

/// The turning state (@ 0x006569d0 yaw speed, @ 0x006569d4 pitch speed).
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub yaw_speed: f32,
    pub pitch_speed: f32,
    /// The divisor of the mouse moves (@ 0x005da268).
    pub sensitivity: i32,
    /// "Invert Y" (@ 0x006569cc).
    pub invert_y: bool,
    /// `LIMITEANGLEWARPX` / `LIMITEANGLEWARPY` in degrees, −1 for none.
    pub limit_x: i32,
    pub limit_y: i32,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            yaw_speed: 0.0,
            pitch_speed: 0.0,
            sensitivity: DEFAULT_SENSITIVITY,
            invert_y: false,
            limit_x: NO_LIMIT,
            limit_y: DEFAULT_LIMIT_Y,
        }
    }
}

impl Look {
    /// A mouse move (DirectInput callback @ 0x0040c420): the yaw speed
    /// gains dx / S, the pitch speed loses dy / S (gains it with "invert
    /// Y"). Moving right turns right, moving down looks down.
    pub fn mouse(&mut self, fpu: Fpu, dx: i32, dy: i32) {
        let r = |x: f64| fpu.r(x);
        let s = f64::from(self.sensitivity);
        let dxf = f(f64::from(dx));
        self.yaw_speed = r(r(dxf / s) + f64::from(self.yaw_speed)) as f32;
        let q = r(f64::from(dy) / s);
        self.pitch_speed = if self.invert_y {
            r(q + f64::from(self.pitch_speed))
        } else {
            r(f64::from(self.pitch_speed) - q)
        } as f32;
    }

    /// Stops turning (@ 0x0040c680, at every view entry).
    pub fn stop(&mut self) {
        self.yaw_speed = 0.0;
        self.pitch_speed = 0.0;
    }

    /// One warp frame (@ 0x0040c6a0, when no move is turning the view):
    /// both speeds × 0.8, the camera turned by them (yaw about the world's
    /// Y axis, pitch about its own X axis), then the angles read back and
    /// brought within the limits, which also stop that turn.
    pub fn frame(&mut self, cam: &mut Camera) {
        let fpu = cam.fpu;
        let r = |x: f64| fpu.r(x);
        let decay = f64::from(f32::from_bits(0x3f4c_cccd)); // 0.8 @ 0x005c62bc
        self.yaw_speed = r(f64::from(self.yaw_speed) * decay) as f32;
        self.pitch_speed = r(f64::from(self.pitch_speed) * decay) as f32;
        cam.yaw(self.yaw_speed);
        cam.pitch(self.pitch_speed);
        // limit · π · (1/180) with the floats @ 0x005c678c, 0x005c62b8.
        let pi = f64::from(f32::from_bits(0x4049_0fdb));
        let k = f64::from(f32::from_bits(0x3bb6_0b61));
        let rad = |deg: i32| f(r(r(f64::from(deg) * pi) * k)) as f32;
        let (lim_y, lim_x) = (rad(self.limit_y), rad(self.limit_x));
        let (yaw, pitch) = cam.angles();
        if self.limit_y != NO_LIMIT {
            if let Some(turn) = over(fpu, pitch, lim_y) {
                cam.pitch(turn);
                self.pitch_speed = 0.0;
            }
        }
        if self.limit_x != NO_LIMIT {
            if let Some(turn) = over(fpu, yaw, lim_x) {
                cam.yaw(turn);
                self.yaw_speed = 0.0;
            }
        }
    }
}

/// The turn that brings `angle` back within ±`limit`, if it is outside
/// (@ 0x0040c773..0x0040c7fb): angle − limit above, angle + limit below.
fn over(fpu: Fpu, angle: f32, limit: f32) -> Option<f32> {
    let (a, l) = (f64::from(angle), f64::from(limit));
    if a > l {
        Some(fpu.r(a - l) as f32)
    } else if -l > a {
        Some(fpu.r(a + l) as f32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::deg_to_rad;

    #[test]
    fn turns_and_stops_at_the_limit() {
        let fpu = Fpu::Double;
        let mut cam = Camera::new(fpu, 640, 480);
        cam.enter(deg_to_rad(fpu, -90), 0.0);
        let mut look = Look::default();
        // Right: the yaw decreases.
        look.mouse(fpu, 100, 0);
        assert_eq!(look.yaw_speed, 0.05);
        look.frame(&mut cam);
        let (yaw, _) = cam.angles();
        assert!((yaw - (-90f32.to_radians() - 0.04)).abs() < 1e-5, "{yaw}");
        // Down a long way: the pitch stops at 70°.
        for _ in 0..50 {
            look.mouse(fpu, 0, 200);
            look.frame(&mut cam);
        }
        let (_, pitch) = cam.angles();
        assert!(
            (pitch.to_degrees() - 70.0).abs() < 1e-3,
            "{}",
            pitch.to_degrees()
        );
        assert_eq!(look.pitch_speed, 0.0);
        // Free yaw by default; a limit holds it.
        look.limit_x = 30;
        look.frame(&mut cam);
        let (yaw, _) = cam.angles();
        assert!(yaw.to_degrees().abs() <= 30.001, "{}", yaw.to_degrees());
    }
}

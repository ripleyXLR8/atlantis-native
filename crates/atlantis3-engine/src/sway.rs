//! The interdimension's swaying camera (atlantis3-gog-fr-3473408; the CD
//! 2001 build's step 0x00431190 and mouse callback are the same bytes).
//!
//! The place hook 0x004c2010 sets the byte 0x007592ac to 1 in
//! `interdim.wam` (0 elsewhere, @ 0x004c2568..0x004c25c7). With it set, the
//! 25 Hz warp step 0x004317a0, on a tick that does not turn, replaces the
//! 76° field of view by the sway branch (@ 0x004319b7..0x00431a8e), after
//! the look's turns and limits:
//!
//! - the amplitude 0x007592c4 × 0.95 (float @ 0x00700478);
//! - the phase 0x007592c8 + 0.08 (@ 0x00700474), less 2π (the float π
//!   @ 0x00700720 doubled on the x87 stack) when it is above 2π;
//! - the field of view `0x004409f0(130 + sin(phase) × amplitude × 0.0015)`
//!   in degrees (@ 0x0070046c, 0x00700470);
//! - a pitch turn 0x004407c0(sin(phase) × 0.00017 + amplitude × 1e-4 ×
//!   sin(phase) × 0.01683) (@ 0x00700460, 0x00700468, 0x00700464);
//! - a yaw turn 0x00440760 by the same with the cosine.
//!
//! A turning tick does not set the 76° field of view in that mode
//! (@ 0x00431845). The mouse callback 0x004313a0 adds min(dx², 1000) to the
//! amplitude in loop mode 3, turning or not, and caps it at 10000
//! (@ 0x004314a7..0x004314f1), whatever the place: the amplitude and the
//! phase are globals that nothing else writes, zero at start-up.
//! Golden: `tests/golden/a3_sway.txt` (atlantis3-re `tools/emu/a3_sway.py`).

use atlantis2_engine::camera::CameraOps;
use atlantis2_engine::Fpu;

/// A float constant of the data section.
fn c(bits: u32) -> f64 {
    f64::from(f32::from_bits(bits))
}

/// The amplitude (0x007592c4) and the phase (0x007592c8).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sway {
    pub amp: f32,
    pub phase: f32,
}

impl Sway {
    /// The mouse callback's part (@ 0x004314b1..0x004314f1): dx as a float
    /// squared, at most 1000, added; the sum at most 10000.
    pub fn mouse(&mut self, fpu: Fpu, dx: i32) {
        let d = fpu.f(f64::from(dx));
        let sq = fpu.mul(d, d);
        let add = if sq > 1000.0 { 1000.0 } else { sq };
        self.amp = fpu.f32(fpu.add(f64::from(self.amp), add));
        if self.amp > 10000.0 {
            self.amp = 10000.0;
        }
    }

    /// One tick of the sway branch: the new field of view in degrees for
    /// 0x004409f0, after the pitch and yaw turns given to the camera
    /// (none under the camera lock: `turn` false).
    pub fn tick<C: CameraOps>(&mut self, cam: &mut C, turn: bool) -> f32 {
        let fpu = cam.fpu();
        self.amp = fpu.f32(fpu.mul(f64::from(self.amp), c(0x3f73_3333)));
        self.phase = fpu.f32(fpu.add(f64::from(self.phase), c(0x3da3_d70a)));
        let pi = c(0x4049_0fdb);
        let two_pi = pi + pi;
        if f64::from(self.phase) > two_pi {
            self.phase = fpu.f32(fpu.sub(f64::from(self.phase), two_pi));
        }
        let phase = f64::from(self.phase);
        let amp = f64::from(self.amp);
        let s = phase.sin();
        let fov = fpu.f32(fpu.add(fpu.mul(fpu.mul(s, amp), c(0x3ac4_9ba6)), c(0x4302_0000)));
        // st0 = amp · 1e-4, · sin, · 0.01683; plus sin · 0.00017.
        let wobble = |t: f64| {
            let a = fpu.mul(fpu.mul(fpu.mul(amp, c(0x38d1_b717)), t), c(0x3c89_df12));
            fpu.f32(fpu.add(fpu.mul(t, c(0x3932_4207)), a))
        };
        let pitch = wobble(s);
        if turn {
            cam.pitch(pitch);
        }
        let yaw = wobble(phase.cos());
        if turn {
            cam.yaw(yaw);
        }
        fov
    }
}

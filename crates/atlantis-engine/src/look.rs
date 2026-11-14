//! Turning the panoramic view with the mouse: the momentum, the view-angle
//! variables ANGLEWARPX / ANGLEWARPY and the pointer the warp loop
//! hit-tests.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`warp_momentum.h`,
//! `warp_pointer.h`; GPLv3, the ScummVM team), which reproduces the warp
//! loop `FUN_0041bfa0` of `atlantis.exe`.
//!
//! The x87 runs at 53-bit precision: the arithmetic below is in `f64` with
//! a rounding to `f32` wherever the original stores a float.

use crate::warp::wrap_angles;

/// The view momentum, `FUN_0041bfa0` step 4 (0x0041c213..0x0041c2b5): with
/// the mouse deltas dx = prevX - x and dy = prevY - y (0x006ce05c /
/// 0x006ce060, floats),
///
/// ```text
/// vx = vx + dx / 3000.0; alpha += vx; vx *= 0.8
/// vy = vy + dy / 4000.0; beta  -= vy; vy *= 0.8
/// ```
///
/// in doubles (0x004bc2f7 = 3000.0, 0x004bc307 = 4000.0, 0x004bc2ff = 0.8)
/// stored back to the floats 0x00496984 / 0x00496988 and the angles
/// 0x00bcb59c / 0x00bcb598. A delta d turns the view by d / 600 (d / 800)
/// in total, spread over the following frames. Nothing resets vx / vy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WarpMomentum {
    pub vx: f32,
    pub vy: f32,
}

impl WarpMomentum {
    pub fn step(&mut self, dx: f32, dy: f32, alpha: &mut f32, beta: &mut f32) {
        self.vx = (f64::from(dx) / 3000.0 + f64::from(self.vx)) as f32;
        *alpha += self.vx;
        self.vx = (f64::from(self.vx) * 0.8) as f32;
        self.vy = (f64::from(dy) / 4000.0 + f64::from(self.vy)) as f32;
        *beta -= self.vy;
        self.vy = (f64::from(self.vy) * 0.8) as f32;
    }
}

/// ANGLEWARPX / ANGLEWARPY of a view angle (`FUN_0041bfa0` @
/// 0x0041c08b..0x0041c10a): angle / pi * 180 at 53-bit precision with the
/// float pi of 0x00bcb594 and the float 180.0 of 0x004bc2f3, stored to a
/// float, then truncated (`FUN_00451d6e`, FISTP).
pub fn angle_warp_degrees(angle: f32, pi: f32) -> i32 {
    let t = (f64::from(angle) / f64::from(pi) * 180.0) as f32;
    t as i32
}

/// One frame of `FUN_0041bfa0` steps 2..5 (see [`warp_look_frame`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WarpLookFrame {
    pub angle_warp_x: i32,
    pub angle_warp_y: i32,
    /// The angles before `FUN_00451915`'s wrap (the recentre test).
    pub new_alpha: f32,
    pub new_beta: f32,
    pub moved: bool,
}

/// `FUN_0041bfa0` steps 2..5 on the stored view angles (0x00bcb59c /
/// 0x00bcb598): ANGLEWARPX / ANGLEWARPY from the angles as the frame
/// starts, then the mouse deltas of the pointer motion (x right, y down;
/// [`WarpPointer::motion`]):
/// dx = -motion x; dy = -motion y in OMNI 3D MODE 1 (0x004960ac == 0, the
/// default), +motion y in MODE 2 (0x0041c1c3..0x0041c20d); the momentum;
/// and, when the new angles differ from the previous frame's,
/// `FUN_00451915`'s wrap and clamp (0x0041c2bb..0x0041c2e9). `alpha` /
/// `beta` are updated in place.
#[allow(clippy::too_many_arguments)]
pub fn warp_look_frame(
    m: &mut WarpMomentum,
    alpha: &mut f32,
    beta: &mut f32,
    motion_x: i32,
    motion_y: i32,
    mode2: bool,
    prev_alpha: f32,
    prev_beta: f32,
    pi: f32,
    two_pi: f32,
    beta_max: f64,
) -> WarpLookFrame {
    let angle_warp_x = angle_warp_degrees(*alpha, pi);
    let angle_warp_y = angle_warp_degrees(*beta, pi);
    let dy = if mode2 { motion_y } else { -motion_y };
    m.step(-motion_x as f32, dy as f32, alpha, beta);
    let (new_alpha, new_beta) = (*alpha, *beta);
    let moved = *alpha != prev_alpha || *beta != prev_beta;
    if moved {
        wrap_angles(alpha, beta, two_pi, beta_max);
    }
    WarpLookFrame {
        angle_warp_x,
        angle_warp_y,
        new_alpha,
        new_beta,
        moved,
    }
}

/// The pointer that the warp loop hit-tests at its exit (`FUN_00422bb0`
/// reads 0x004960ec / 0x004960f0), the previous point the look delta is
/// taken from (0x006ce074 / 0x006ce02c) and the previous frame's view
/// angles it compares with (0x0049698c / 0x00496990). They live across
/// warp loops:
///
/// - the loop does not set the pointer at entry (0x0041bfa0 entry block),
///   it sets the previous point to it (0x0041c003 / 0x0041c00d);
/// - every frame, step 3 takes the delta previous point minus pointer
///   (0x0041c1c3..0x0041c20d, [`WarpPointer::motion`]); step 5 recentres
///   the pointer at (320, 240) when alpha or beta differs from the stored
///   angles (0x0041c2bb..0x0041c301), then sets the previous point to the
///   pointer (0x0041c32c..0x0041c34c) and stores the angles (0x0041c357 /
///   0x0041c363). The angles start at -1.0 in the image, so the first loop
///   of a session always recentres;
/// - the previous point has no other writer than the bar close
///   (`FUN_0042c098` @ 0x0042c28e / 0x0042c298, with the pointer at
///   (320, 240) @ 0x0042c275): the motion of the mouse during a timer
///   section's lines or the in-game menu (which write neither) is in the
///   next frame's delta;
/// - the panorama load `FUN_0044a424` leaves the pointer at (0x140, 0x140)
///   (0x0044a70b / 0x0044a715, no SetCursorPos);
/// - the WM_MOUSEMOVE handler (0x00419c14) sets it to the mouse position,
///   so after a dialog or a video during which the mouse moved it is the
///   last mouse position.
///
/// The pointer is (0, 0) in the image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WarpPointer {
    pub pos: (i32, i32),
    /// 0x006ce074 / 0x006ce02c.
    pub prev: (i32, i32),
    pub prev_alpha: f32,
    pub prev_beta: f32,
}

impl Default for WarpPointer {
    fn default() -> Self {
        WarpPointer {
            pos: (0, 0),
            prev: (0, 0),
            prev_alpha: -1.0,
            prev_beta: -1.0,
        }
    }
}

impl WarpPointer {
    /// `FUN_0044a424` @ 0x0044a70b / 0x0044a715.
    pub fn panorama_loaded(&mut self) {
        self.pos = (0x140, 0x140);
    }

    /// WM_MOUSEMOVE (0x00419c14).
    pub fn mouse_moved(&mut self, p: (i32, i32)) {
        self.pos = p;
    }

    /// The step-5 recentre.
    pub fn recentre(&mut self) {
        self.pos = (320, 240);
    }

    /// The warp loop's entry (0x0041c003 / 0x0041c00d): the previous point
    /// is the pointer.
    pub fn loop_entry(&mut self) {
        self.prev = self.pos;
    }

    /// The bar close `FUN_0042c098`: the pointer at (320, 240) (@
    /// 0x0042c275) and the previous point with it (@ 0x0042c28e /
    /// 0x0042c298).
    pub fn bar_closed(&mut self) {
        self.recentre();
        self.prev = self.pos;
    }

    /// The relative motion of the mouse (x right, y down) as WM_MOUSEMOVE
    /// sets the pointer, clamped to the screen (the original takes the
    /// Windows cursor's position, which Windows keeps on the 640x480
    /// screen in fullscreen [INFERRED: the system's rule; the game
    /// imports no ClipCursor]).
    pub fn follow(&mut self, dx: i32, dy: i32) {
        self.pos.0 = i32::from(self.pos.0.saturating_add(dx).clamp(0, 639) as i16);
        self.pos.1 = i32::from(self.pos.1.saturating_add(dy).clamp(0, 479) as i16);
    }

    /// The motion step 3 reads (x right, y down): the pointer minus the
    /// previous point, so dx = prevX - x is its negation
    /// (0x0041c1c3..0x0041c20d; [`warp_look_frame`]).
    pub fn motion(&self) -> (i32, i32) {
        (self.pos.0 - self.prev.0, self.pos.1 - self.prev.1)
    }

    /// Step 5 of one warp frame: the pointer recentred when the new
    /// angles differ from the stored ones, the previous point set to the
    /// pointer either way, and the stored angles those the frame ends with
    /// (after the clamp `FUN_00451915`, @ 0x0041c351..0x0041c363). True
    /// when recentred.
    pub fn frame(&mut self, new_alpha: f32, new_beta: f32, end_alpha: f32, end_beta: f32) -> bool {
        let moved = new_alpha != self.prev_alpha || new_beta != self.prev_beta;
        if moved {
            self.recentre();
        }
        self.prev = self.pos;
        self.prev_alpha = end_alpha;
        self.prev_beta = end_beta;
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // FUN_0041bfa0 step 4 (STATIC @ 0x0041c213..0x0041c2b5): one delta of
    // 30 px, then none. Expected values: the same float / double sequence
    // computed outside the port: alpha 1.01, 1.018, 1.0244, 1.02952,
    // 1.033616 and 1.05 (30 / 600) in the limit.
    #[test]
    fn momentum() {
        let mut m = WarpMomentum::default();
        let (mut alpha, mut beta) = (1.0f32, 0.2f32);
        let want = [1.01f32, 1.018, 1.0244, 1.02952, 1.033616];
        for i in 0..400 {
            let (dx, dy) = if i == 0 { (30.0, -40.0) } else { (0.0, 0.0) };
            m.step(dx, dy, &mut alpha, &mut beta);
            if let Some(w) = want.get(i) {
                assert!((alpha - w).abs() < 1e-6, "frame {i}: {alpha}");
            }
        }
        assert!((alpha - 1.049_999_8).abs() < 1e-6);
        assert!((beta - 0.25).abs() < 1e-6);
    }

    // The warp pointer across loops: the image values (0, 0) and -1.0
    // (0x004960ec, 0x0049698c), (0x140, 0x140) after a panorama load
    // (FUN_0044a424 @ 0x0044a70b / 0x0044a715), the step-5 recentre only
    // when the angles change (0x0041c2bb..0x0041c363).
    #[test]
    #[allow(clippy::approx_constant)]
    fn pointer() {
        let mut p = WarpPointer::default();
        assert_eq!(p.pos, (0, 0));
        assert_eq!((p.prev_alpha, p.prev_beta), (-1.0, -1.0));
        // The first loop of a session: any view differs from -1.0.
        p.panorama_loaded();
        assert_eq!(p.pos, (320, 320));
        assert!(p.frame(0.0, 0.0, 0.0, 0.0));
        assert_eq!(p.pos, (320, 240));
        // A motion that does not turn the view moves the pointer.
        p.follow(15, -7);
        assert!(!p.frame(0.0, 0.0, 0.0, 0.0));
        assert_eq!(p.pos, (335, 233));
        // A checkpoint reload at the same angles: the loop keeps (320, 320).
        p.panorama_loaded();
        assert!(!p.frame(0.0, 0.0, 0.0, 0.0));
        assert_eq!(p.pos, (320, 320));
        // After a dialog the last mouse position, kept while the view rests.
        p.mouse_moved((500, 400));
        assert!(!p.frame(0.0, 0.0, 0.0, 0.0));
        assert_eq!(p.pos, (500, 400));
        // The pointer is clamped to the screen.
        p.follow(400, 400);
        assert!(!p.frame(0.0, 0.0, 0.0, 0.0));
        assert_eq!(p.pos, (639, 479));
        // Momentum moves the view: recentred; the stored angles are the
        // frame's end ones (after the pitch clamp), the test uses the new.
        assert!(p.frame(0.01, 0.9, 0.01, 0.785398));
        assert_eq!(p.pos, (320, 240));
        assert_eq!(p.prev_beta, 0.785398);
        p.mouse_moved((10, 20));
        assert!(p.frame(0.01, 0.9, 0.01, 0.785398));
        // The bar close recentres, the previous point with it.
        p.mouse_moved((10, 20));
        p.bar_closed();
        assert_eq!((p.pos, p.prev), ((320, 240), (320, 240)));
    }

    // The look delta is previous point minus pointer (0x0041c1c3..
    // 0x0041c20d), the previous point set at the loop entry, at step 5 and
    // by the bar close only: a mouse motion during a timer section's lines
    // or the in-game menu (the WM_MOUSEMOVE syncs) is in the next frame's
    // delta, and a motion past the screen's edge is not (N7).
    #[test]
    fn the_look_delta_is_taken_from_the_previous_point() {
        let mut p = WarpPointer {
            prev_alpha: 0.0,
            prev_beta: 0.0,
            ..WarpPointer::default()
        };
        p.mouse_moved((320, 240));
        p.loop_entry();
        p.follow(5, -3);
        assert_eq!(p.motion(), (5, -3));
        assert!(!p.frame(0.0, 0.0, 0.0, 0.0));
        assert_eq!(p.motion(), (0, 0));
        // A menu or a timer dialog during which the mouse went to (600, 40).
        p.mouse_moved((600, 40));
        assert_eq!(p.motion(), (600 - 325, 40 - 237));
        assert!(!p.frame(0.0, 0.0, 0.0, 0.0));
        assert_eq!(p.motion(), (0, 0));
        // At the right edge a motion further right gives no delta.
        p.mouse_moved((639, 240));
        p.loop_entry();
        p.follow(100, 0);
        assert_eq!(p.motion(), (0, 0));
    }
}

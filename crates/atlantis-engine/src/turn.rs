//! The turn before a transition video, `atlantis.exe` `FUN_004212cc`.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (`transition_turn.h`; GPLv3, the ScummVM team).
//!
//! Before a transition video the view turns to the transition's source
//! angles (`src_alpha`, `src_beta` of the WAM record), zooming in by 3
//! degrees a frame when the CON variable ZOOM is set, then jumps to the
//! destination angles. The x87 runs at 53-bit precision and every
//! intermediate the original stores as a float is stored as one here, so
//! the angles match the original's bit for bit.
//!
//! Phase 1 runs while the stop tests fail: |alpha - src_alpha| and
//! |beta - src_beta| both below 0.05, or one of two stall counters reaching
//! 5. A counter goes up on a frame whose truncated |difference| times 1000
//! equals the value kept from an earlier frame (the truncated value times
//! 100), which only happens when the truncated difference is 0
//! (`FUN_00451d6e` rounds toward zero). The kept values start uninitialised
//! in the original (stack left-overs); they are taken here as matching
//! nothing on the first frame. Phase 2 is five more frames. Phase 1 runs
//! while 0x00496124 is set ([`TransitionTurn`]).

/// The FOV `FUN_004212cc` starts its zoom from and sets at its end
/// (0x42965fbe).
pub const TURN_END_FOV_DEG: f32 = 75.187;

/// One frame of the turn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnFrame {
    pub alpha: f32,
    pub beta: f32,
    /// `FUN_0045179a(fov, 50)` before this frame.
    pub fov_set: bool,
    pub fov: f32,
}

/// The frames of the turn from (alpha, beta) to (src_alpha, src_beta) and
/// the angles it reaches, 0x00496124 set all along. `pi` is the float
/// `FUN_0045179a` stores at 0x00bcb594.
pub fn transition_turn(
    pi: f32,
    alpha: f32,
    beta: f32,
    src_alpha: f32,
    src_beta: f32,
    zoom: bool,
) -> (Vec<TurnFrame>, f32, f32) {
    let mut turn = TransitionTurn::new(pi, alpha, beta, src_alpha, src_beta, zoom);
    let mut frames = Vec::new();
    while let Some(f) = turn.next_frame(true) {
        frames.push(f);
    }
    (frames, turn.alpha, turn.beta)
}

/// The turn frame by frame, for a caller whose input poll can clear
/// 0x00496124 between two frames: phase 1 runs only while it is set (its
/// test at the head of each pass, 0x00421376), so an Escape there ends
/// phase 1 and the five frames of phase 2 follow from where the view is.
#[derive(Clone, Debug)]
pub struct TransitionTurn {
    pi: f32,
    two_pi: f32,
    pub alpha: f32,
    pub beta: f32,
    src_alpha: f32,
    src_beta: f32,
    zoom: bool,
    fov: f32,
    speed_x: f32, // 0x006ce05c
    speed_y: f32, // 0x006ce060
    // [EBP-0x18], [EBP-0x14]: never r * 1000 on the first frame.
    kept_a: i32,
    kept_b: i32,
    stall_a: u8, // [EBP-0x4]
    stall_b: u8, // [EBP-0xc]
    phase: u8,
    n: u8,
}

impl TransitionTurn {
    pub fn new(
        pi: f32,
        alpha: f32,
        beta: f32,
        src_alpha: f32,
        src_beta: f32,
        zoom: bool,
    ) -> TransitionTurn {
        TransitionTurn {
            pi,
            two_pi: (f64::from(pi) * 2.0) as f32,
            alpha,
            beta,
            src_alpha,
            src_beta,
            zoom,
            fov: TURN_END_FOV_DEG,
            speed_x: 0.0,
            speed_y: 0.0,
            kept_a: -1,
            kept_b: -1,
            stall_a: 0,
            stall_b: 0,
            phase: 1,
            n: 0,
        }
    }

    /// The next frame, `running` the flag 0x00496124 as the input poll
    /// after the previous frame left it; `None` once the turn is over.
    pub fn next_frame(&mut self, running: bool) -> Option<TurnFrame> {
        if self.phase == 1 && !running {
            self.phase = 2;
        }
        if self.phase == 2 {
            if self.n >= 5 {
                return None;
            }
            self.n += 1;
        }
        step(
            self.pi,
            self.two_pi,
            &mut self.alpha,
            &mut self.beta,
            self.src_alpha,
            self.src_beta,
            &mut self.speed_x,
            &mut self.speed_y,
        );
        if self.zoom {
            self.fov = (f64::from(self.fov) + f64::from(-3.0f32)) as f32;
        }
        let frame = TurnFrame {
            alpha: self.alpha,
            beta: self.beta,
            fov_set: self.zoom,
            fov: self.fov,
        };
        if self.phase == 1 && self.phase1_ends() {
            self.phase = 2;
        }
        Some(frame)
    }

    /// Phase 1's stop tests after a frame.
    fn phase1_ends(&mut self) -> bool {
        let mut da = (f64::from(self.alpha) - f64::from(self.src_alpha)) as f32;
        if 0.0 > da {
            da = -da;
        }
        let mut db = (f64::from(self.beta) - f64::from(self.src_beta)) as f32;
        if 0.0 > db {
            db = -db;
        }
        // FUN_00451d6e: FRNDINT with RC = chop.
        let (ra, rb) = (da as i32, db as i32);
        if ra.wrapping_mul(1000) == self.kept_a {
            self.stall_a = self.stall_a.wrapping_add(1);
        } else {
            self.kept_a = ra.wrapping_mul(100);
        }
        if rb.wrapping_mul(1000) == self.kept_b {
            self.stall_b = self.stall_b.wrapping_add(1);
        } else {
            self.kept_b = rb.wrapping_mul(100);
        }
        if f64::from(da) < 0.05 && f64::from(db) < 0.05 {
            return true;
        }
        self.stall_a == 5 || self.stall_b == 5
    }
}

/// One step, 0x00421383..0x00421514 (the negated compares keep the
/// original's NaN behaviour).
#[allow(clippy::too_many_arguments, clippy::neg_cmp_op_on_partial_ord)]
fn step(
    pi: f32,
    two_pi: f32,
    alpha: &mut f32,
    beta: &mut f32,
    src_alpha: f32,
    src_beta: f32,
    speed_x: &mut f32,
    speed_y: &mut f32,
) {
    let (pi, two_pi) = (f64::from(pi), f64::from(two_pi));
    // 0x006ce03c
    let mut d = ((f64::from(*alpha) + two_pi) - f64::from(src_alpha)) as f32;
    if f64::from(d) >= two_pi || d.is_nan() {
        d = (f64::from(d) - two_pi) as f32;
    }
    if 0.0 > d {
        d = (f64::from(d) + two_pi) as f32;
    }
    let mut t = (f64::from(d) * 600.0) as f32;
    if 0.0 > t {
        t = -t;
    }
    if f64::from(d) < pi {
        *speed_x = -t;
    }
    t = ((two_pi - f64::from(d)) * 600.0) as f32;
    if 0.0 > t {
        t = -t;
    }
    if !(f64::from(d) < pi) {
        *speed_x = t;
    }
    if d == 0.0 {
        *speed_x = 0.0;
    }
    let e = (f64::from(src_beta) - f64::from(*beta)) as f32; // 0x006ce040
    t = ((f64::from(*beta) - f64::from(src_beta)) * 600.0) as f32;
    if 0.0 > t {
        t = -t;
    }
    if 0.0 > e {
        *speed_y = t;
    }
    t = ((f64::from(*beta) - f64::from(src_beta)) * 600.0) as f32;
    if 0.0 > t {
        t = -t;
    }
    if !(0.0 > e) {
        *speed_y = -t;
    }
    *alpha = (f64::from(*speed_x) / 3000.0 + f64::from(*alpha)) as f32;
    *beta = (f64::from(*beta) - f64::from(*speed_y) / 4000.0) as f32;
}

/// The double-click flag 0x00496118 that a move tests before its video:
/// `FUN_00423ad8` @ 0x00423f8a jumps past the HNM open `FUN_00448c70`, the
/// SPF sprites `FUN_0041b81c`, the ambient glide `FUN_0043259c` and the
/// play loop `FUN_0041bc74` when it is set, to the flag reset @ 0x00424013;
/// the place then changes at once. The window procedure sets it on
/// WM_LBUTTONDBLCLK (0x00419b49 -> 0x00419c79..0x00419c8b) when the cursor
/// state 0x004968ac is 5 (`FUN_00422fcc`'s navigate cursor, zone 1..511) or
/// the menu request 0x00496138 is 1; the window class has CS_DBLCLKS
/// (style 0xb @ 0x0042e4eb). The turn `FUN_004212cc` pumps the messages on
/// every frame (`FUN_0041a298` @ 0x00421670 / 0x00421a5a) and does not call
/// `FUN_00422fcc` (0x0049616c = 1), so the navigate state of the clicked
/// zone holds through it: a double click whose second press comes during
/// the turn skips the video.
///
/// When Windows sends WM_LBUTTONDBLCLK is the system's rule, not the
/// binary's: a second press within the double-click time of the first,
/// inside a rectangle centred on it (GetDoubleClickTime, SM_CXDOUBLECLK /
/// SM_CYDOUBLECLK), 500 ms and 4 x 4 pixels by default (inferred: the
/// Windows defaults; the player's setting is not known).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnDoubleClick {
    /// 0x00496118.
    pub flag: bool,
    /// A first press is kept.
    armed: bool,
    first_ms: u64,
    first: (i32, i32),
}

impl TurnDoubleClick {
    /// GetDoubleClickTime()'s default.
    pub const TIME_MS: u64 = 500;
    /// SM_CXDOUBLECLK / SM_CYDOUBLECLK's default.
    pub const RECT_SIZE: i32 = 4;

    /// A left press that is not the second of a pair (the click that ends
    /// the warp loop).
    pub fn first_press(&mut self, ms: u64, x: i32, y: i32) {
        self.armed = true;
        self.first_ms = ms;
        self.first = (x, y);
    }

    /// A later left press; `cursor_mode` is 0x004968ac, `menu_request`
    /// 0x00496138. True when it is a double click (whether or not it sets
    /// the flag); the press after a double click starts a new pair.
    pub fn press(&mut self, ms: u64, x: i32, y: i32, cursor_mode: u8, menu_request: bool) -> bool {
        let (dx, dy) = (x - self.first.0, y - self.first.1);
        let half = Self::RECT_SIZE / 2;
        let double = self.armed
            && ms.wrapping_sub(self.first_ms) < Self::TIME_MS
            && (-half..=half).contains(&dx)
            && (-half..=half).contains(&dy);
        if double {
            self.armed = false;
            // 0x00419c79: CMP byte [0x004968ac],5 / CMP [0x00496138],1
            if cursor_mode == 5 || menu_request {
                self.flag = true;
            }
        } else {
            self.first_press(ms, x, y);
        }
        double
    }
}

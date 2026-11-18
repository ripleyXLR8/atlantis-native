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

/// The clamped target of a setter and the ranges after it
/// (@ 0x00521d73..0x00521de6, 0x00521e33..0x00521ea6): `d = float(target
/// − current)`; when `plus − d < 0` the target becomes `float(target +
/// (plus − d))`; then when `d + minus < 0` it becomes `float(target − (d +
/// minus))`; the ranges move by `target − current`.
fn step(cur: f32, target: f32, plus: &mut f32, minus: &mut f32) -> f32 {
    let (c, p, m) = (f64::from(cur), f64::from(*plus), f64::from(*minus));
    let t = f64::from(target);
    let d = f64::from((t - c) as f32);
    let mut v = t;
    if p - d < 0.0 {
        v = f64::from((t + (p - d)) as f32);
    }
    if d + m < 0.0 {
        v = f64::from((v - (d + m)) as f32);
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

/// The pointer moves the input manager keeps for its speed
/// (`0x0050a7e0` @ 0x0050a837..0x0050a89e).
pub const MOVES: usize = 20;

/// The time over which the speed at a drag's release is measured, s
/// (`0.05f` @ 0x005bee90, passed to `0x0050a240`).
pub const SPEED_WINDOW: f32 = 0.05;

/// The velocity curve's keys, fractions of the release velocity (doubles
/// @ 0x005bed48..0x005bed28, `0x005beac8`).
pub const COAST_KEYS: [f64; 7] = [1.0, 0.65, 0.32, 0.15, 0.07, 0.03, 0.0];

/// The velocity curve's duration, ms (`400.0f` @ 0x005bed20).
pub const COAST_MS: f32 = 400.0;

/// The velocity setters' bound, degrees per second (`0x00521d20`,
/// `0x00521cd0`: `±10000.0f` @ 0x005beb60, 0x005beb5c).
pub const VELOCITY_LIMIT: f32 = 10000.0;

/// The input manager's last pointer moves (`input+0x338`, 20 entries of
/// `{dt: f32 s, dx, dy}`, write index `+0x428`, timer `+0x430`).
///
/// - Every change of the pointer's position records the time since the
///   previous change (the timer read and restarted, µs / 1e6 rounded to a
///   float) and the move in pixels (@ 0x0050a837..0x0050a89e).
/// - The speed (`0x0050a240(0.05f)`): `t` = the time since the last move;
///   while `t < 0.05` and for at most 20 entries, newest first, `t += dt`
///   (rounded to a float) and the move is summed; then the sum divided by
///   `t`, truncated (`0x004659f0`), or zero when `t ≤ 0`.
///
/// The moves are kept in the drag's convention (x right, y up), so the
/// speed has the sign a drag of the same move has.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveHistory {
    dt: [f32; MOVES],
    dx: [i32; MOVES],
    dy: [i32; MOVES],
    next: usize,
    /// The last position recorded.
    last: Option<(i32, i32)>,
    /// The timer's start, µs of the engine clock.
    since_us: u64,
}

impl Default for MoveHistory {
    fn default() -> MoveHistory {
        MoveHistory {
            dt: [0.0; MOVES],
            dx: [0; MOVES],
            dy: [0; MOVES],
            next: 0,
            last: None,
            since_us: 0,
        }
    }
}

/// `0x004fb170` / `0x004fb0e0` divided by 1e6 and stored as a float.
fn seconds(us: u64) -> f32 {
    (us as f64 / 1e6) as f32
}

impl MoveHistory {
    /// The pointer at pixel (`x`, `y_up`) (y growing upwards) at `now_us`:
    /// a move is recorded when the position changed.
    pub fn moved(&mut self, x: i32, y_up: i32, now_us: u64) {
        let Some((lx, ly)) = self.last.replace((x, y_up)) else {
            self.since_us = now_us;
            return;
        };
        if (lx, ly) == (x, y_up) {
            return;
        }
        let dt = seconds(now_us.saturating_sub(self.since_us));
        self.since_us = now_us;
        self.push(dt, x.wrapping_sub(lx), y_up.wrapping_sub(ly));
    }

    /// Records a move of (`dx`, `dy`) pixels `dt` s after the previous one.
    pub fn push(&mut self, dt: f32, dx: i32, dy: i32) {
        self.dt[self.next] = dt;
        self.dx[self.next] = dx;
        self.dy[self.next] = dy;
        self.next = (self.next + 1) % MOVES;
    }

    /// The pointer's speed at `now_us`, pixels per second.
    pub fn speed(&self, now_us: u64) -> (i32, i32) {
        self.speed_after(seconds(now_us.saturating_sub(self.since_us)))
    }

    /// The speed `since_last` s after the last move (`0x0050a240`).
    pub fn speed_after(&self, since_last: f32) -> (i32, i32) {
        let mut t = since_last;
        let (mut sx, mut sy) = (0i32, 0i32);
        for k in 0..MOVES {
            // `fcom`: anything but "t below the window" stops.
            if t.partial_cmp(&SPEED_WINDOW) != Some(std::cmp::Ordering::Less) {
                break;
            }
            let i = (self.next + 2 * MOVES - 1 - k) % MOVES;
            t = (f64::from(t) + f64::from(self.dt[i])) as f32;
            sx = sx.wrapping_add(self.dx[i]);
            sy = sy.wrapping_add(self.dy[i]);
        }
        if t.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return (0, 0);
        }
        let t = f64::from(t);
        let div = |s: i32| (f64::from(s) / t).trunc() as i32;
        (div(sx), div(sy))
    }
}

/// `0x00521d20` / `0x00521cd0`: a velocity stored, then bounded to
/// ±10000.
fn bound_velocity(v: f32) -> f32 {
    // A NaN stays (the comparisons fail), as `clamp` keeps it.
    v.clamp(-VELOCITY_LIMIT, VELOCITY_LIMIT)
}

/// The two velocity curves (`GameWarp+0x3678` yaw, `+0x36e0` pitch),
/// started together.
#[derive(Clone, Debug, PartialEq)]
struct CoastCurves {
    /// `play()`'s clock, ms (`+0x28`).
    start_ms: f64,
    yaw: [f64; 7],
    pitch: [f64; 7],
}

/// The view's inertia after a drag (`GameWarp+0x3610..+0x3618`, the
/// curves `+0x3678`, `+0x36e0`, the timer `+0x3620`), atlantis4-gog-en-2363910.
///
/// - Drag start (`0x0052a950(1)`, from the warp's press handler
///   `0x00521f20`): the curves stop; the velocities stay but are not used
///   while dragging.
/// - Drag end (`0x0052a950(0)`, from the mouse-up `0x00523a80` and from
///   the per-frame check @ 0x0052ac0e when the button is up): the timer
///   restarts; `speed` = the pointer's speed ([`MoveHistory`]), negated
///   with `InverseLook`; `v_yaw = 0.5 sx`, `v_pitch = 0.5 sy` (each
///   bounded to ±10000); then `0x005224a0` starts two curves of 400 ms
///   with keys `v·{1, 0.65, 0.32, 0.15, 0.07, 0.03, 0}` that write the
///   velocities back through the same setters.
/// - The curves run in the global animation update (`0x00408f70`, before
///   the view update): `e = float(now − start)`, `v = float(curve(keys,
///   e, 400))`, stopped when `e ≥ 400`.
/// - Each frame of the view update (`0x0052ae0a..0x0052ae62`, not while
///   dragging, not during a movie): `dt` = the timer read and restarted
///   (s, a float); `ψ = setYaw(ψ − v_yaw·dt)`, `θ = setPitch(θ +
///   v_pitch·dt)`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inertia {
    /// `+0x3610`: a drag is in progress.
    pub dragging: bool,
    /// `+0x3614`, degrees per second.
    pub v_yaw: f32,
    /// `+0x3618`, degrees per second.
    pub v_pitch: f32,
    curves: Option<CoastCurves>,
    /// The timer's start, µs of the engine clock.
    timer_us: u64,
}

impl Inertia {
    /// A press that reaches the view: a drag starts (`0x0052a950(1)`).
    pub fn press(&mut self) {
        if self.dragging {
            return;
        }
        self.dragging = true;
        self.curves = None;
    }

    /// The drag's end at `now_us` with the pointer's `speed`
    /// (`0x0052a950(0)`); nothing when no drag is in progress.
    pub fn release(&mut self, speed: (i32, i32), inverse_look: bool, now_us: u64) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        self.timer_us = now_us;
        let (sx, sy) = if inverse_look {
            (speed.0.wrapping_neg(), speed.1.wrapping_neg())
        } else {
            speed
        };
        let k = f64::from(DRAG_DEGREES_PER_PIXEL);
        self.v_yaw = bound_velocity((f64::from(sx as f32) * k) as f32);
        self.v_pitch = bound_velocity((f64::from(sy as f32) * k) as f32);
        let keys = |v: f32| COAST_KEYS.map(|c| f64::from(v) * c);
        self.curves = Some(CoastCurves {
            start_ms: now_us as f64 / 1000.0,
            yaw: keys(self.v_yaw),
            pitch: keys(self.v_pitch),
        });
        // `play()` updates at once (@ 0x004f5946).
        self.update_curves(now_us as f64 / 1000.0);
    }

    /// The curves' update at `now_ms` (the global animation update).
    pub fn update_curves(&mut self, now_ms: f64) {
        let Some(c) = &self.curves else { return };
        let (v_yaw, v_pitch, done) = coast_sample(c, now_ms);
        self.v_yaw = v_yaw;
        self.v_pitch = v_pitch;
        if done {
            self.curves = None;
        }
    }

    /// Whether the velocity curves are running.
    pub fn curves_running(&self) -> bool {
        self.curves.is_some()
    }

    /// The view update's step at `now_us` (`0x0052ae0a..0x0052ae62`).
    pub fn coast(&mut self, view: &mut View, now_us: u64) {
        if self.dragging {
            return;
        }
        let dt = seconds(now_us.saturating_sub(self.timer_us));
        self.timer_us = now_us;
        let dt = f64::from(dt);
        view.set_yaw((f64::from(view.yaw) - f64::from(self.v_yaw) * dt) as f32);
        view.set_pitch((f64::from(self.v_pitch) * dt + f64::from(view.pitch)) as f32);
    }
}

/// One update of both curves (`0x00408f70` each): the velocities and
/// whether they ended.
fn coast_sample(c: &CoastCurves, now_ms: f64) -> (f32, f32, bool) {
    // The global update gives an animation `max(now, start)`.
    let t = now_ms.max(c.start_ms);
    let e = (t - c.start_ms) as f32;
    let d = f64::from(COAST_MS);
    let yaw = bound_velocity(crate::gui::anim::curve(&c.yaw, f64::from(e), d) as f32);
    let pitch = bound_velocity(crate::gui::anim::curve(&c.pitch, f64::from(e), d) as f32);
    (yaw, pitch, e >= COAST_MS)
}

/// The velocity curve's value `t_ms` after its start with release
/// velocity `v` (one update of `0x00408f70`), and whether it ended.
pub fn coast_velocity(v: f32, t_ms: f64) -> (f32, bool) {
    let keys = COAST_KEYS.map(|c| f64::from(v) * c);
    let c = CoastCurves {
        start_ms: 1000.0,
        yaw: keys,
        pitch: keys,
    };
    let (y, _, done) = coast_sample(&c, 1000.0 + t_ms);
    (y, done)
}

/// The velocity setters (`0x00521d20`, `0x00521cd0`).
pub fn set_velocity(v: f32) -> f32 {
    bound_velocity(v)
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

    #[test]
    fn a_release_while_moving_coasts_for_400_ms() {
        let mut m = MoveHistory::default();
        // 10 px right every 10 ms: 1000 px/s.
        for k in 0..=10u64 {
            m.moved(100 + 10 * k as i32, -300, 10_000 * k);
        }
        assert_eq!(m.speed(100_000), (1000, 0));
        let mut i = Inertia::default();
        i.press();
        i.release(m.speed(100_000), false, 100_000);
        assert_eq!((i.v_yaw, i.v_pitch), (500.0, 0.0));
        let mut v = View::default();
        let mut t = 100_000;
        while i.curves_running() {
            t += 16_667;
            i.update_curves(t as f64 / 1000.0);
            i.coast(&mut v, t);
        }
        assert!(v.yaw < -50.0 && v.yaw > -70.0, "{}", v.yaw);
        // At rest for 50 ms before the release: no inertia.
        assert_eq!(m.speed(150_000), (0, 0));
        // A press stops the curves; the view does not move while dragging.
        i.press();
        i.release((2000, 0), true, 200_000);
        assert_eq!(i.v_yaw, -1000.0);
        i.press();
        assert!(!i.curves_running());
        let before = v;
        i.coast(&mut v, 300_000);
        assert_eq!(v, before);
    }
}

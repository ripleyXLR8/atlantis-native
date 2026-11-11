//! The browser's pace and pointer motion, as plain functions (tested on
//! the desktop too).
//!
//! A browser asks for one redraw per screen refresh
//! (`requestAnimationFrame`): 60 a second on most screens, 120 or 144 on
//! a fast one, 30 under a power saver. The games' steps keep the
//! desktop's fixed pace whatever that rate: a redraw runs the steps that
//! fell due since the last one ([`steps_due`]).

use web_time::{Duration, Instant};

/// The most steps one redraw runs to catch up: four frames, so a screen
/// refreshed down to 15 times a second still gets every step. Further
/// behind (a slow computer, the browser busy), the backlog is dropped, as
/// the desktop's gate drops it ("fell behind: do not catch up").
pub const MAX_STEPS: u32 = 4;

/// A step is run when it is due within this: a redraw a moment early (the
/// refresh's own jitter, a 59.94 Hz pace on a 60 Hz screen) does not wait
/// a whole refresh and run two steps at the next one.
pub const SLACK: Duration = Duration::from_millis(4);

/// The steps of `frame` due at a redraw at `now` (the slack added by the
/// caller), the next being due at `next`: their number and the time the
/// first was due (the others follow it a `frame` apart, the last at most
/// `now`); `next` is moved past them. None due: (0, `now`).
pub fn steps_due(next: &mut Instant, now: Instant, frame: Duration) -> (u32, Instant) {
    if now < *next || frame.is_zero() {
        return (0, now);
    }
    let behind = now.duration_since(*next);
    let due = behind.as_nanos() / frame.as_nanos() + 1;
    if due > u128::from(MAX_STEPS) {
        // Fell behind: the last MAX_STEPS frames only, ending now.
        *next = now + frame;
        return (MAX_STEPS, now - frame * (MAX_STEPS - 1));
    }
    let n = due as u32;
    let first = *next;
    *next += frame * n;
    (n, first)
}

/// The factor that brings the pointer-lock motion winit reports back to
/// the browser's own `movementX` / `movementY`: winit takes Chromium's as
/// device pixels and multiplies the other engines' (Firefox's, Safari's)
/// by `devicePixelRatio`, so that the same mouse turned the view about
/// twice as fast in Firefox or Safari as in Chrome on a HiDPI screen.
pub fn motion_scale(chromium: bool, device_pixel_ratio: f64) -> f64 {
    if chromium || !(device_pixel_ratio.is_finite() && device_pixel_ratio > 0.0) {
        1.0
    } else {
        1.0 / device_pixel_ratio
    }
}

/// Adds the motion (`dx`, `dy`) to `acc` and takes its whole counts out:
/// the fractions wait for the next motion instead of being lost.
pub fn whole_motion(acc: &mut (f64, f64), dx: f64, dy: f64) -> (i32, i32) {
    acc.0 += dx;
    acc.1 += dy;
    let (ix, iy) = (acc.0.trunc(), acc.1.trunc());
    acc.0 -= ix;
    acc.1 -= iy;
    (ix as i32, iy as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_micros(16_667);

    /// The steps run by redraws every `period` for `secs` seconds.
    fn steps_in(period: Duration, secs: u32, slack: Duration) -> u32 {
        let t0 = Instant::now();
        let mut next = t0;
        let mut total = 0;
        let mut t = t0;
        let end = t0 + Duration::from_secs(u64::from(secs));
        let mut last_first = None;
        while t < end {
            let (n, first) = steps_due(&mut next, t + slack, FRAME);
            if n > 0 {
                // The steps' times go forward and end by the redraw.
                assert!(first + FRAME * (n - 1) <= t + slack);
                if let Some(l) = last_first {
                    assert!(first > l);
                }
                last_first = Some(first);
            }
            total += n;
            t += period;
        }
        total
    }

    #[test]
    fn sixty_steps_a_second_at_any_refresh() {
        for hz in [30u32, 50, 60, 75, 120, 144, 165, 240] {
            let period = Duration::from_nanos(1_000_000_000 / u64::from(hz));
            for slack in [Duration::ZERO, SLACK] {
                let n = steps_in(period, 10, slack);
                // 600 in 10 s, give or take the last redraw's.
                assert!((598..=602).contains(&n), "{hz} Hz: {n} steps");
            }
        }
    }

    #[test]
    fn at_most_one_step_a_refresh_from_60_hz_up() {
        let t0 = Instant::now();
        let mut next = t0;
        let period = Duration::from_nanos(1_000_000_000 / 144);
        for k in 0..1000u32 {
            let (n, _) = steps_due(&mut next, t0 + period * k + SLACK, FRAME);
            assert!(n <= 1, "redraw {k}: {n}");
        }
    }

    #[test]
    fn thirty_hz_runs_two_steps_a_redraw() {
        let t0 = Instant::now();
        let mut next = t0;
        let period = Duration::from_nanos(1_000_000_000 / 30);
        assert_eq!(steps_due(&mut next, t0, FRAME).0, 1);
        for k in 1..100u32 {
            let (n, _) = steps_due(&mut next, t0 + period * k + SLACK, FRAME);
            assert_eq!(n, 2, "redraw {k}");
        }
    }

    #[test]
    fn a_long_stall_drops_the_backlog() {
        let t0 = Instant::now();
        let mut next = t0;
        let now = t0 + Duration::from_millis(200);
        let (n, first) = steps_due(&mut next, now, FRAME);
        assert_eq!(n, MAX_STEPS);
        assert_eq!(first + FRAME * (MAX_STEPS - 1), now);
        assert_eq!(next, now + FRAME);
        // Not due again before the next frame.
        assert_eq!(steps_due(&mut next, now + FRAME / 2, FRAME).0, 0);
    }

    #[test]
    fn early_redraw_runs_nothing() {
        let t0 = Instant::now();
        let mut next = t0 + FRAME;
        assert_eq!(steps_due(&mut next, t0, FRAME), (0, t0));
        assert_eq!(next, t0 + FRAME);
    }

    #[test]
    fn motion_scale_undoes_the_device_pixel_ratio_outside_chromium() {
        assert_eq!(motion_scale(true, 2.0), 1.0);
        assert_eq!(motion_scale(false, 2.0), 0.5);
        assert_eq!(motion_scale(false, 1.0), 1.0);
        assert!((motion_scale(false, 1.25) - 0.8).abs() < 1e-12);
        assert_eq!(motion_scale(false, 0.0), 1.0);
        assert_eq!(motion_scale(false, f64::NAN), 1.0);
        // Firefox at a DPR of 2: winit reports 2 per count; back to 1.
        let s = motion_scale(false, 2.0);
        let mut acc = (0.0, 0.0);
        assert_eq!(whole_motion(&mut acc, 2.0 * s, -6.0 * s), (1, -3));
    }

    #[test]
    fn fractions_are_kept_not_dropped() {
        // A DPR of 1.5 in Firefox: one count reported as 1.5, then
        // brought back to 1 (0.6667 x 1.5); slow moves of 1.25 lose
        // nothing over time.
        let mut acc = (0.0, 0.0);
        let mut sum = (0, 0);
        for _ in 0..100 {
            let (x, y) = whole_motion(&mut acc, 1.25, -0.25);
            sum.0 += x;
            sum.1 += y;
        }
        assert_eq!(sum, (125, -25));
        // Below one count a motion: nothing lost either.
        let mut acc = (0.0, 0.0);
        let total: i32 = (0..10).map(|_| whole_motion(&mut acc, 0.5, 0.0).0).sum();
        assert_eq!(total, 5);
    }
}

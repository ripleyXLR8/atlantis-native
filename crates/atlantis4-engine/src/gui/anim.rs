//! The layouts' linear animations (`TeLayoutAnchorLinearAnimation` and its
//! three siblings), as atlantis4-gog-en-2363910 runs them.
//!
//! - **Clock**: wall-clock milliseconds (the global timer's µs / 1000).
//!   `play()` (@ 0x004f58f0) sets `start = now` and updates at once, so the
//!   first value is `startValue`; the per-frame update (@ 0x004f5950) gives
//!   every playing animation the same `max(now, start)`.
//! - **Update** (@ 0x00436e50): `e = float(t − start)`; `f = float(curve(e,
//!   duration))`; each component is `float(start × float(1 − f)) +
//!   float(end × f)`, stored as a float; when `e ≥ duration` the animation
//!   stops, then emits `onFinished`.
//! - **Curve** (@ 0x005525c0, samples as doubles): no sample → 0; `e ≥
//!   duration` → the last sample; else `x = e (n − 1) / duration`, `i =
//!   trunc(x)` (indices past the end read the last sample), `s[i] (i + 1 −
//!   x) + s[i + 1] (x − i)`: piecewise linear over equal steps, in doubles
//!   (the x87 at 53 bits).

/// `TeCurveAnim2`'s sample at `e` ms of `duration` (@ 0x005525c0).
pub fn curve(samples: &[f64], e: f64, duration: f64) -> f64 {
    let n = samples.len();
    if n == 0 {
        return 0.0;
    }
    let last = samples[n - 1];
    // `fcom`: anything but "e below duration" (NaN included) takes the
    // last sample.
    if e.partial_cmp(&duration) != Some(std::cmp::Ordering::Less) {
        return last;
    }
    let x = e * (n - 1) as f64 / duration;
    // `fistp` with the chop rounding mode, as a 64-bit integer whose low
    // word is kept (a negative `x` gives a huge unsigned index: the last
    // sample).
    let i = x.trunc() as i64 as u32 as usize;
    let s0 = if i < n { samples[i] } else { last };
    let s1 = if i + 1 < n { samples[i + 1] } else { last };
    // `cvttsd2si` (@ 0x004659f0) of the same `x`.
    let k = x.trunc() as i32;
    s0 * (f64::from(k.wrapping_add(1)) - x) + s1 * (x - f64::from(k))
}

/// One linear animation of a layout property (`x, y, z`, or a colour's
/// four bytes in the first three plus alpha for the colour class).
#[derive(Clone, Debug, PartialEq)]
pub struct LinearAnim {
    /// `duration`, ms (`+0x48`).
    pub duration: f32,
    pub start_value: [f32; 4],
    pub end_value: [f32; 4],
    /// `curve`, as the doubles Lua gave.
    pub curve: Vec<f64>,
    pub playing: bool,
    /// The clock at `play()`, ms (`+0x28`).
    pub start_ms: f64,
    /// The last value set on the layout.
    pub value: [f32; 4],
}

/// What one update did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Step {
    /// The value set on the layout, if the animation ran.
    pub value: Option<[f32; 4]>,
    /// The animation reached its duration (`onFinished` emitted).
    pub finished: bool,
}

impl LinearAnim {
    pub fn new(duration: f32, start: [f32; 4], end: [f32; 4], curve: Vec<f64>) -> LinearAnim {
        LinearAnim {
            duration,
            start_value: start,
            end_value: end,
            curve,
            playing: false,
            start_ms: 0.0,
            value: start,
        }
    }

    /// The value at `t` ms with the animation started at `start_ms`
    /// (@ 0x00436e50), and whether it reached its end.
    pub fn value_at(&self, t: f64) -> ([f32; 4], bool) {
        let e = (t - self.start_ms) as f32;
        let f = curve(&self.curve, f64::from(e), f64::from(self.duration)) as f32;
        let g = (1.0 - f64::from(f)) as f32;
        let v = std::array::from_fn(|k| {
            let a = (f64::from(self.end_value[k]) * f64::from(f)) as f32;
            let b = (f64::from(self.start_value[k]) * f64::from(g)) as f32;
            (f64::from(b) + f64::from(a)) as f32
        });
        (v, e >= self.duration)
    }

    /// `play()` at `now` (@ 0x004f58f0): a playing animation goes on; a
    /// stopped one starts at `now` and is updated at once.
    pub fn play(&mut self, now: f64) -> Step {
        if self.playing {
            return Step::default();
        }
        self.playing = true;
        self.start_ms = now;
        self.update(now)
    }

    /// `stop()`: `true` if it was playing.
    pub fn stop(&mut self) -> bool {
        std::mem::replace(&mut self.playing, false)
    }

    /// The frame's update at `now` (`max(now, start)`, @ 0x004f5950).
    pub fn update(&mut self, now: f64) -> Step {
        if !self.playing {
            return Step::default();
        }
        let (v, end) = self.value_at(now.max(self.start_ms));
        self.value = v;
        if end {
            self.playing = false;
        }
        Step {
            value: Some(v),
            finished: end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EASE: [f64; 7] = [0.0, 0.35, 0.68, 0.85, 0.93, 0.97, 1.0];

    #[test]
    fn the_curve_is_piecewise_linear_over_equal_steps() {
        assert_eq!(curve(&EASE, 25.0, 500.0), 0.105);
        assert_eq!(curve(&EASE, 500.0, 500.0), 1.0);
        assert_eq!(curve(&[], 10.0, 500.0), 0.0);
        assert_eq!(curve(&EASE, 0.0, 0.0), 1.0);
    }

    #[test]
    fn play_shows_the_start_value_and_the_end_is_exact() {
        let mut a = LinearAnim::new(
            500.0,
            [0.5, -0.1, 0.5, 0.0],
            [0.5, 1.0, 0.5, 0.0],
            EASE.to_vec(),
        );
        let s = a.play(1000.0);
        assert_eq!(s.value.unwrap()[1], -0.1);
        assert!(!s.finished);
        let s = a.update(1500.0);
        assert_eq!(s.value.unwrap()[1], 1.0);
        assert!(s.finished && !a.playing);
    }
}

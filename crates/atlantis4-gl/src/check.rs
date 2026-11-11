//! The check mode: one frame drawn by the processor (the exact model) and
//! by the card, compared channel by channel.
//!
//! # The tolerance
//!
//! A card computes `GL_LINEAR` with the fractions of the texel coordinate
//! in fixed point, 8 bits on today's GPUs and in Mesa's llvmpipe (the
//! specification leaves the precision to the implementation), while the
//! processor's model takes them exact. A fraction off by at most `2⁻⁸`
//! per axis moves an interpolated channel by at most `255 · (2⁻⁸ + 2⁻⁸) <
//! 2` levels between two texels as far apart as black and white; the two
//! sides then round independently (½ each), the processor rounds the
//! sampled texel to 8 bits before blending and the card does not (½
//! more), and the blend `Cs·As + Cd·(1 − As)` is rounded once more on
//! each side: at most **4** levels where a picture has a hard edge, far
//! less (a fraction of a level on average) on photographs. [`TOLERANCE`]
//! is that bound.
//!
//! The rasterisation of a pixel whose centre lies exactly on an edge
//! (two background cells, the rim of a sprite at a half-integer corner,
//! a tile's triangles) is the driver's too (GL's invariance rules are
//! recommendations); such pixels can take the neighbour cell's or
//! picture's texels and differ by more. [`Diff::over`] counts the pixels
//! over the tolerance and [`EDGE_SHARE`] bounds their share.

/// The largest channel difference expected from the filtering arithmetic
/// (see the module).
pub const TOLERANCE: u8 = 4;

/// The largest share of the pixels allowed over [`TOLERANCE`]: those on
/// an edge whose rasterisation is the driver's.
pub const EDGE_SHARE: f64 = 0.002;

/// The largest mean channel difference expected (most pixels sample
/// smooth pictures where the weights' precision moves nothing).
pub const MEAN: f64 = 0.5;

/// How two frames differ.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Diff {
    pub pixels: usize,
    /// Pixels with any channel different.
    pub differ: usize,
    /// Pixels with a channel more than [`TOLERANCE`] apart.
    pub over: usize,
    /// The largest channel difference.
    pub max: u8,
    /// The mean channel difference over every channel of every pixel.
    pub mean: f64,
    /// The first pixel over the tolerance: x, y, processor, card.
    pub first_over: Option<(usize, usize, u32, u32)>,
}

impl Diff {
    /// Whether the difference stays within [`TOLERANCE`], [`EDGE_SHARE`]
    /// and [`MEAN`].
    pub fn within(&self) -> bool {
        (self.over as f64) <= EDGE_SHARE * self.pixels as f64 && self.mean <= MEAN
    }
}

impl std::fmt::Display for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} of {} pixels differ, max {} levels, mean {:.4}, {} over {}",
            self.differ, self.pixels, self.max, self.mean, self.over, TOLERANCE
        )?;
        if let Some((x, y, a, b)) = self.first_over {
            write!(f, " (first at {x},{y}: processor {a:06x}, card {b:06x})")?;
        }
        Ok(())
    }
}

/// Compares two XRGB frames `width` pixels wide (rows top-down).
pub fn compare(cpu: &[u32], gpu: &[u32], width: usize) -> Diff {
    let mut d = Diff {
        pixels: cpu.len().min(gpu.len()),
        ..Diff::default()
    };
    let mut sum = 0u64;
    for (i, (&a, &b)) in cpu.iter().zip(gpu).enumerate() {
        if (a ^ b) & 0xff_ffff == 0 {
            continue;
        }
        d.differ += 1;
        let mut worst = 0u8;
        for s in [16, 8, 0] {
            let x = ((a >> s) & 0xff) as u8;
            let y = ((b >> s) & 0xff) as u8;
            let e = x.abs_diff(y);
            sum += u64::from(e);
            worst = worst.max(e);
        }
        d.max = d.max.max(worst);
        if worst > TOLERANCE {
            d.over += 1;
            if d.first_over.is_none() {
                d.first_over = Some((i % width.max(1), i / width.max(1), a, b));
            }
        }
    }
    d.mean = sum as f64 / (3 * d.pixels.max(1)) as f64;
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_differences_are_counted_per_channel() {
        let a = [0x00_10_20_30u32, 0, 0x00ff_ffff, 5];
        let b = [0x00_10_20_32u32, 0, 0x0000_0000, 5];
        let d = compare(&a, &b, 2);
        assert_eq!((d.differ, d.over, d.max), (2, 1, 255));
        assert_eq!(d.first_over, Some((0, 1, 0x00ff_ffff, 0)));
        assert!((d.mean - (2.0 + 765.0) / 12.0).abs() < 1e-9);
    }
}

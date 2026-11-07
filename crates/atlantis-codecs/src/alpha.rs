//! Atlantis II's dialog videos: the `AL` chunk, an alpha mask drawn with
//! each frame over the picture behind the character.
//!
//! Comments cite addresses in Atlantis II's `atlantis2.exe` (the French CD
//! build of 1999, 2199552 bytes). The mask is not part of the frame: the
//! frame decoder ignores it, and the dialog video stream (`HnmStream`)
//! blends the decoded frame into the back buffer with it
//! (@ 0x0046b790 → @ 0x0046b9b0).

use crate::DecodeError;

/// The blend tables of `HnmStream` (@ 0x0046b850): for each alpha a and
/// each byte of a background pixel, that byte's share of the pixel scaled
/// by (31 − a) / 32. The video frames are premultiplied: the character is
/// drawn over black, and the mask tells how much of the background shows
/// through.
pub struct AlphaTables {
    /// Indexed by `a * 256 + high byte` (0x00669984 in the original).
    hi: Vec<u16>,
    /// Indexed by `a * 256 + low byte` (0x00661984).
    lo: Vec<u16>,
}

impl AlphaTables {
    /// The tables for an RGB565 screen, or RGB555 when `rgb555` (`this +
    /// 0x18` = 1, the screen format test at 0x006d85c8).
    pub fn new(rgb555: bool) -> AlphaTables {
        let mut hi = vec![0u16; 32 * 256];
        let mut lo = vec![0u16; 32 * 256];
        for a in 0..32u32 {
            let f = 31 - a;
            for b in 0..256u32 {
                // High byte: red and the high bits of green; the green part
                // is rounded half up and clamped.
                let (red, gh, gmax, rshift) = if rgb555 {
                    ((b >> 2) & 31, b & 3, 31, 10)
                } else {
                    (b >> 3, b & 7, 63, 11)
                };
                let g = gh * 8 * f;
                let g = ((g >> 5) + ((g >> 4) & 1)).min(gmax);
                hi[(a * 256 + b) as usize] = ((((red * f) >> 5) << rshift) | (g << 5)) as u16;
                // Low byte: the low bits of green, and blue, truncated.
                let (gl, blue) = (b >> 5, b & 31);
                lo[(a * 256 + b) as usize] = ((((gl * f) >> 5) << 5) + ((blue * f) >> 5)) as u16;
            }
        }
        AlphaTables { hi, lo }
    }

    /// Draws a `width` x `height` video frame into `dst` (rows of `pitch`
    /// pixels) through the `AL` payload `runs`, as @ 0x0046b9b0 does: pairs
    /// (n, a) of n + 1 pixels of alpha a (0..31), read until exactly
    /// `width * height` pixels are covered (a padding pair after that is
    /// never read). Alpha 0 leaves the background, 31 copies the video,
    /// and in between `dst = video + HI[a][dst >> 8] + LO[a][dst & 0xff]`,
    /// a 16-bit add that wraps. A run may cross a row end.
    ///
    /// The original runs until its pixel counter is exactly 0; a mask that
    /// overshoots or ends early is an error here, after drawing what it
    /// covers.
    pub fn blend(
        &self,
        video: &[u16],
        dst: &mut [u16],
        runs: &[u8],
        width: usize,
        height: usize,
        pitch: usize,
    ) -> Result<(), DecodeError> {
        let total = width * height;
        let needed = height.checked_sub(1).map_or(0, |rows| rows * pitch + width);
        if video.len() < total || pitch < width || dst.len() < needed {
            return Err(DecodeError::Bitstream("alpha: buffer too small"));
        }
        let mut p = 0usize;
        let mut pairs = runs.as_chunks::<2>().0.iter();
        while p < total {
            let Some(pair) = pairs.next() else {
                return Err(DecodeError::Bitstream("alpha: mask ends early"));
            };
            let (n, a) = (usize::from(pair[0]) + 1, usize::from(pair[1]));
            if a > 31 || p + n > total {
                return Err(DecodeError::Bitstream("alpha: bad run"));
            }
            for q in p..p + n {
                let d = &mut dst[(q / width) * pitch + q % width];
                match a {
                    0 => {}
                    31 => *d = video[q],
                    _ => {
                        let bg = usize::from(*d);
                        *d = video[q]
                            .wrapping_add(self.hi[a * 256 + (bg >> 8)])
                            .wrapping_add(self.lo[a * 256 + (bg & 0xff)]);
                    }
                }
            }
            p += n;
        }
        Ok(())
    }
}

/// *Atlantis III*'s `AL` blend on a 32-bit display (`Game.exe`
/// @ 0x006a2031, chosen by @ 0x006a1e14 for 4 bytes a pixel): the same
/// runs as [`AlphaTables::blend`], but in between each byte of the pixel
/// (x included) becomes `(d·256 + (s − d)·8a) >> 8` (`punpcklbw`, `psubw`,
/// `psllw 8`, `pmullw` by 8a, `paddw`, `psrlw 8`): a mix of the video and
/// the background by a / 32, not the 16-bit tables' sum. A run that crosses
/// a row end continues, after the row's last pixel, at the next pixel of
/// `dst` without the row skip when it blends (the copy and skip runs take
/// it, @ 0x006a20e9 / 0x006a2100; the blend's second part @ 0x006a215a
/// does not): when `pitch` > `width` those pixels land on the same row.
pub fn blend_x888(
    video: &[u32],
    dst: &mut [u32],
    runs: &[u8],
    width: usize,
    height: usize,
    pitch: usize,
) -> Result<(), DecodeError> {
    let total = width * height;
    if video.len() < total || pitch < width {
        return Err(DecodeError::Bitstream("alpha: buffer too small"));
    }
    let skip = pitch - width;
    let mix = |s: u32, d: u32, a: u32| -> u32 {
        let mut out = 0;
        for lane in 0..4 {
            let (sb, db) = ((s >> (8 * lane)) & 0xff, (d >> (8 * lane)) & 0xff);
            let v = ((db << 8) as u16).wrapping_add(
                (sb as u16)
                    .wrapping_sub(db as u16)
                    .wrapping_mul((8 * a) as u16),
            ) >> 8;
            out |= u32::from(v) << (8 * lane);
        }
        out
    };
    let (mut left, mut col) = (total as i64, width as i64);
    let (mut si, mut di) = (0usize, 0usize);
    let mut pairs = runs.as_chunks::<2>().0.iter();
    let outside = DecodeError::Bitstream("alpha: outside the target");
    while left > 0 {
        if col == 0 {
            di += skip;
            col = width as i64;
        }
        let Some(pair) = pairs.next() else {
            return Err(DecodeError::Bitstream("alpha: mask ends early"));
        };
        let (n, a) = (i64::from(pair[0]) + 1, u32::from(pair[1]));
        if a > 31 {
            return Err(DecodeError::Bitstream("alpha: bad run"));
        }
        left -= n;
        col -= n;
        let (first, second, row_skip) = if col < 0 {
            (n + col, -col, a == 0 || a == 31)
        } else {
            (n, 0, false)
        };
        for part in 0..2 {
            let k = if part == 0 { first } else { second };
            if part == 1 {
                if second == 0 {
                    break;
                }
                if row_skip {
                    di += skip;
                }
            }
            for _ in 0..k {
                if a != 0 {
                    let d = dst.get_mut(di).ok_or(outside.clone())?;
                    *d = if a == 31 {
                        video[si]
                    } else {
                        mix(video[si], *d, a)
                    };
                }
                si += 1;
                di += 1;
            }
        }
        if col < 0 {
            col += width as i64;
        }
        if si > total {
            return Err(DecodeError::Bitstream("alpha: bad run"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_transparent_and_half() {
        let t = AlphaTables::new(false);
        let video = [0x1234u16; 8];
        let mut dst = [0xffffu16; 8];
        // 2 pixels a = 0, 3 pixels a = 31, 3 pixels a = 15.
        t.blend(&video, &mut dst, &[1, 0, 2, 31, 2, 15, 0, 0], 4, 2, 4)
            .unwrap();
        assert_eq!(&dst[..2], &[0xffff, 0xffff]);
        assert_eq!(&dst[2..5], &[0x1234; 3]);
        // White scaled by 16/32: red 15, blue 15, green 28 from the high
        // bits (rounded) + 3 from the low bits (truncated).
        let half = (15 << 11) | (31 << 5) | 15;
        assert_eq!(dst[5], 0x1234u16.wrapping_add(half));
        // a = 1 keeps 30/32 of the background, not all of it.
        let mut one = [0xffffu16];
        t.blend(&[0], &mut one, &[0, 1], 1, 1, 1).unwrap();
        assert_eq!(one[0], (29 << 11) | (59 << 5) | 29);
    }

    #[test]
    fn rows_of_a_wider_target() {
        let t = AlphaTables::new(false);
        let video = [7u16; 6];
        let mut dst = [0u16; 10];
        // One run of 6 opaque pixels over two rows of 3 in a pitch of 5.
        t.blend(&video, &mut dst, &[5, 31], 3, 2, 5).unwrap();
        assert_eq!(dst, [7, 7, 7, 0, 0, 7, 7, 7, 0, 0]);
        assert!(t.blend(&video, &mut dst, &[6, 31], 3, 2, 5).is_err());
        assert!(t.blend(&video, &mut dst, &[2, 31], 3, 2, 5).is_err());
        // An empty frame (height 0, as a bad header could give) is no
        // underflow.
        assert!(t.blend(&[], &mut [], &[], 3, 0, 5).is_ok());
    }

    #[test]
    fn rgb555_differs_in_the_high_byte_only() {
        let (a, b) = (AlphaTables::new(false), AlphaTables::new(true));
        assert_eq!(a.lo, b.lo);
        assert_ne!(a.hi, b.hi);
        // 555 white at a = 1 (f = 30): red and blue 29, green 23 + 6.
        let mut w = [0x7fffu16];
        b.blend(&[0], &mut w, &[0, 1], 1, 1, 1).unwrap();
        assert_eq!(w[0], (29 << 10) | (29 << 5) | 29);
    }
}

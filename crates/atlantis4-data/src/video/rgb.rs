//! Y'CbCr → RGBA as the original does it, and the alpha of its paired
//! streams and pictures.
//!
//! **Colour** (`TeTheora`'s upload, atlantis4-gog-en-2363910 @ 0x004cc920)
//! is Robin Watts' packed-table `yuv420 → RGB` code (BSD, "yuv2rgb"),
//! linked as RGBA32 @ 0x004ae5e0 and RGB24 @ 0x004aeb60, which give the
//! same R, G, B: BT.601 studio range with Y clamped to 16..239, nearest
//! chroma (each 2 × 2 luma block shares one Cb/Cr pair), the 4th byte 0.
//! Its 768-dword table (`.rdata` @ 0x005accf0) has no generator in the
//! executable and no formula gives it (the fields carry into each other:
//! B is off a clean formula by up to 255 on 1,202,182 of the 2^24
//! triples), so [`RgbTable::from_exe`] reads it from the player's own
//! executable and checks its MD5; it is not in this program.
//!
//! **Alpha** (`TeTheoraWithAlpha`, @ 0x004d17b0): the colour stream's pass
//! writes R, G, B and A = 0, then the alpha stream's pass writes A only:
//! the alpha stream's Y as is when its comments say `ColorRange=FullRange`
//! (@ 0x004ccbd4; the 742 alpha entries of the warp packages), otherwise
//! the red channel of an integer BT.601 conversion of the tables the
//! builder @ 0x004cb5a0 fills, clamped to 0..255 (@ 0x004ccb01; the 182
//! alpha `.ogv` files).
//!
//! **Pictures** `X.rgb.jpg` + `X.alpha.jpg` (`TeVideoWithAlpha`,
//! @ 0x004cee70): R, G, B of the colour picture and A = byte 0 of the
//! alpha picture's pixel.

use md5::{Digest, Md5};

use super::theora::Picture;
use super::VideoError;
use crate::image::Image;

/// Where a known build keeps the table: its id, the table's file offset,
/// and the MD5 of its 3,072 bytes.
pub struct TableLocation {
    pub build: &'static str,
    pub file_offset: usize,
    pub md5: [u8; 16],
}

/// The table in each known build.
pub const TABLE_LOCATIONS: [TableLocation; 1] = [TableLocation {
    build: "atlantis4-gog-en-2363910",
    // VA 0x005accf0 in `.rdata` (VA 0x5a4000 = file 0x1a3200).
    file_offset: 0x1a_bef0,
    md5: [
        0x88, 0x54, 0x5d, 0x9d, 0xed, 0x73, 0x02, 0xcd, 0x5e, 0x50, 0xb6, 0xf9, 0xee, 0x25, 0x3b,
        0x4f,
    ],
}];

/// The colour conversion's 3 × 256 packed dwords: luma at 0, Cb at 256,
/// Cr at 512.
#[derive(Clone)]
pub struct RgbTable {
    t: Box<[u32; 768]>,
}

impl std::fmt::Debug for RgbTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RgbTable")
    }
}

/// The field flags of the packed value: R's overflow bit 8, B's 19, G's 30.
const FLAGS: u32 = 0x4008_0100;

impl RgbTable {
    /// The table of the player's executable: the build identified, the
    /// table read at that build's place and its MD5 checked. An unknown
    /// build is refused.
    pub fn from_exe(exe: &[u8]) -> Result<RgbTable, VideoError> {
        let build = crate::exe::identify(exe).map_err(VideoError::Exe)?;
        let loc = TABLE_LOCATIONS
            .iter()
            .find(|l| l.build == build.id)
            .ok_or(VideoError::NoTable(build.id))?;
        let raw = exe
            .get(loc.file_offset..loc.file_offset + 3072)
            .ok_or(VideoError::NoTable(build.id))?;
        let md5: [u8; 16] = Md5::digest(raw).into();
        if md5 != loc.md5 {
            return Err(VideoError::NoTable(build.id));
        }
        RgbTable::from_bytes(raw)
    }

    /// A table from 3,072 little-endian bytes, unchecked (tests build
    /// their own).
    pub fn from_bytes(raw: &[u8]) -> Result<RgbTable, VideoError> {
        if raw.len() != 3072 {
            return Err(VideoError::NoTable("?"));
        }
        let mut t = Box::new([0u32; 768]);
        for (d, b) in t.iter_mut().zip(raw.as_chunks::<4>().0) {
            *d = u32::from_le_bytes(*b);
        }
        Ok(RgbTable { t })
    }

    /// Cb and Cr's part of a 2 × 2 block, added once per block.
    #[inline]
    fn uv(&self, cb: u8, cr: u8) -> u32 {
        self.t[256 + usize::from(cb)].wrapping_add(self.t[512 + usize::from(cr)])
    }

    /// One pixel: the luma entry added, `FIXUP` (overflow → 255,
    /// underflow → 0 per field), the bytes taken out.
    #[inline]
    fn pixel(&self, y: u8, uv: u32) -> [u8; 3] {
        let mut v = uv.wrapping_add(self.t[usize::from(y)]);
        let mut t = v & FLAGS;
        if t != 0 {
            t = t.wrapping_sub(t >> 8);
            v |= t;
            t = FLAGS & !(v >> 1);
            v = v.wrapping_add(t >> 8);
        }
        [v as u8, (v >> 22) as u8, (v >> 11) as u8]
    }

    /// R, G, B of one (Y, Cb, Cr) triple.
    pub fn rgb(&self, y: u8, cb: u8, cr: u8) -> [u8; 3] {
        self.pixel(y, self.uv(cb, cr))
    }

    /// A picture to RGBA (`out`: width × height × 4 bytes, rows top first),
    /// the 4th byte 0, in the order of @ 0x004ae5e0: row pairs, then the
    /// trailing row of an odd height (with the next chroma row). In the
    /// trailing column of an odd width the code swaps the two rows' luma,
    /// as the public C does; no picture of the data has an odd size.
    ///
    /// # Panics
    ///
    /// When `out` is shorter than the picture.
    pub fn to_rgba(&self, pic: &Picture<'_>, out: &mut [u8]) {
        let (w, h) = (pic.width, pic.height);
        let (cw, _) = pic.chroma_size();
        let out = &mut out[..w * h * 4];
        let put = |out: &mut [u8], row: usize, x: usize, c: [u8; 3]| {
            let o = (row * w + x) * 4;
            out[o..o + 4].copy_from_slice(&[c[0], c[1], c[2], 0]);
        };
        let mut row = 0;
        while row + 1 < h {
            let (cb, cr) = (pic.cb.row(row >> 1, cw), pic.cr.row(row >> 1, cw));
            let (y0, y1) = (pic.y.row(row, w), pic.y.row(row + 1, w));
            for cx in 0..w >> 1 {
                let uv = self.uv(cb[cx], cr[cx]);
                for x in [2 * cx, 2 * cx + 1] {
                    put(out, row + 1, x, self.pixel(y1[x], uv));
                    put(out, row, x, self.pixel(y0[x], uv));
                }
            }
            if w & 1 != 0 {
                let (cx, x) = (w >> 1, w - 1);
                let uv = self.uv(cb[cx], cr[cx]);
                put(out, row + 1, x, self.pixel(y0[x], uv));
                put(out, row, x, self.pixel(y1[x], uv));
            }
            row += 2;
        }
        if row + 1 == h {
            let (cb, cr) = (pic.cb.row(row >> 1, cw), pic.cr.row(row >> 1, cw));
            let y0 = pic.y.row(row, w);
            for x in 0..w {
                put(
                    out,
                    row,
                    x,
                    self.pixel(y0[x], self.uv(cb[x >> 1], cr[x >> 1])),
                );
            }
        }
    }
}

/// `(298·Y − 4640) >> 8` (@ .data 0x00601958; the rounding +128 folded
/// in), arithmetic shift.
const fn y_298(y: u8) -> i32 {
    (298 * y as i32 - 4640) >> 8
}

/// `(409·(Cr − 128)) >> 8` (@ .data 0x00604b68), arithmetic shift.
const fn cr_409(cr: u8) -> i32 {
    (409 * (cr as i32 - 128)) >> 8
}

/// The alpha of one pixel of a stream without `FullRange`.
pub fn limited_alpha(y: u8, cr: u8) -> u8 {
    (y_298(y) + cr_409(cr)).clamp(0, 255) as u8
}

/// The alpha stream's pass over an RGBA picture: byte 3 of each pixel.
/// With `full_range` it is the Y byte as is (every column); otherwise
/// [`limited_alpha`] of Y and the block's Cr, the pixels taken in pairs
/// (`width >> 1`), so an odd width's last column keeps its A.
///
/// # Panics
///
/// When `out` is shorter than the picture.
pub fn apply_alpha(pic: &Picture<'_>, full_range: bool, out: &mut [u8]) {
    let (w, h) = (pic.width, pic.height);
    let (cw, _) = pic.chroma_size();
    for row in 0..h {
        let ys = pic.y.row(row, w);
        let line = &mut out[row * w * 4..(row + 1) * w * 4];
        if full_range {
            for (px, &y) in line.as_chunks_mut::<4>().0.iter_mut().zip(ys) {
                px[3] = y;
            }
            continue;
        }
        let cr = pic.cr.row(row >> 1, cw);
        for x in 0..(w >> 1) * 2 {
            line[x * 4 + 3] = limited_alpha(ys[x], cr[x >> 1]);
        }
    }
}

/// An `X.rgb.jpg` + `X.alpha.jpg` pair as RGBA: R, G, B of the colour
/// picture (3 channels) and A = the first byte of the alpha picture's
/// pixel (1 or 3 channels; all 345 alpha pictures of the data are grey).
pub fn merge_jpeg_alpha(rgb: &Image, alpha: &Image) -> Result<Vec<u8>, VideoError> {
    if rgb.channels != 3
        || !matches!(alpha.channels, 1 | 3)
        || (rgb.width, rgb.height) != (alpha.width, alpha.height)
    {
        return Err(VideoError::Pair);
    }
    let step = usize::from(alpha.channels);
    let mut out = Vec::with_capacity(rgb.pixels.len() / 3 * 4);
    for (c, a) in rgb
        .pixels
        .as_chunks::<3>()
        .0
        .iter()
        .zip(alpha.pixels.chunks_exact(step))
    {
        out.extend_from_slice(&[c[0], c[1], c[2], a[0]]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::theora::Plane;
    use super::*;

    /// A table whose fields carry nothing: R = Cr, B = Cb, G = Y, each
    /// in its field (R bit 0, B × 2 at bit 10, G × 4 at bit 20).
    fn plain_table() -> RgbTable {
        let mut raw = Vec::new();
        for part in 0..3u32 {
            for i in 0..256u32 {
                let v = match part {
                    0 => (i << 22) & 0x3fc0_0000,
                    1 => i << 11,
                    _ => i,
                };
                raw.extend(v.to_le_bytes());
            }
        }
        RgbTable::from_bytes(&raw).unwrap()
    }

    #[test]
    fn fixup_follows_the_flags() {
        let t = plain_table();
        assert_eq!(t.rgb(10, 20, 30), [30, 10, 20]);
        // R's bit 8 alone gives 0, bits 8 and 9 give 255; the flags of
        // the other fields, clear, add 1 to them (the model's values).
        assert_eq!(t.pixel(0, 0x100), [0, 1, 1]);
        assert_eq!(t.pixel(0, 0x300), [255, 1, 1]);
        assert_eq!(t.pixel(0, 0x200), [0, 0, 0]);
    }

    #[test]
    fn the_odd_sizes_follow_the_code() {
        let t = plain_table();
        // 3 × 3 picture: Y = 10 × row + column; chroma 2 × 2.
        let y: Vec<u8> = (0..9).map(|i| (i / 3 * 10 + i % 3) as u8).collect();
        let cb = [1, 2, 3, 4];
        let cr = [5, 6, 7, 8];
        let pic = Picture {
            width: 3,
            height: 3,
            y: Plane::new(&y, 0, 3),
            cb: Plane::new(&cb, 0, 2),
            cr: Plane::new(&cr, 0, 2),
        };
        let mut out = vec![9; 36];
        t.to_rgba(&pic, &mut out);
        let g = |r: usize, x: usize| out[(r * 3 + x) * 4 + 1];
        // Row pair 0/1: the last column's luma is swapped.
        assert_eq!([g(0, 0), g(0, 1), g(0, 2)], [0, 1, 12]);
        assert_eq!([g(1, 0), g(1, 1), g(1, 2)], [10, 11, 2]);
        // The trailing row uses chroma row 1.
        assert_eq!([g(2, 0), g(2, 1), g(2, 2)], [20, 21, 22]);
        assert_eq!(out[(2 * 3 + 2) * 4], 8); // R = Cr of block (1, 1)
        assert_eq!(out[(2 * 3) * 4 + 2], 3); // B = Cb of block (1, 0)
        assert!(out.chunks(4).all(|p| p[3] == 0));
    }

    #[test]
    fn limited_alpha_is_the_red_channel_clamped() {
        // Values of the reference model (the executable's alpha loop run in an
        // emulator gives the same for every (Y, Cr)).
        let v = [
            ((0, 128), 0),
            ((16, 128), 0),
            ((17, 128), 1),
            ((128, 128), 130),
            ((235, 128), 255),
            ((255, 0), 73),
            ((100, 255), 255),
            ((60, 60), 0),
            ((200, 100), 169),
            ((50, 128), 40),
        ];
        for ((y, cr), a) in v {
            assert_eq!(limited_alpha(y, cr), a, "{y} {cr}");
        }
        // Odd width: the last column keeps its A; full range copies Y.
        let y = [16u8, 128, 235, 50, 60, 70];
        let cr = [128u8, 128];
        let pic = Picture {
            width: 3,
            height: 2,
            y: Plane::new(&y, 0, 3),
            cb: Plane::new(&cr, 0, 2),
            cr: Plane::new(&cr, 0, 2),
        };
        let mut out = vec![7; 24];
        apply_alpha(&pic, false, &mut out);
        let a: Vec<u8> = out.chunks(4).map(|p| p[3]).collect();
        assert_eq!(a, [0, 130, 7, 40, 51, 7]);
        apply_alpha(&pic, true, &mut out);
        let a: Vec<u8> = out.chunks(4).map(|p| p[3]).collect();
        assert_eq!(a, y);
        assert!(out.chunks(4).all(|p| p[..3] == [7, 7, 7]));
    }

    #[test]
    fn a_jpeg_pair_merges_byte_by_byte() {
        let rgb = Image {
            width: 2,
            height: 1,
            channels: 3,
            pixels: vec![1, 2, 3, 4, 5, 6],
        };
        let grey = Image {
            width: 2,
            height: 1,
            channels: 1,
            pixels: vec![7, 8],
        };
        assert_eq!(
            merge_jpeg_alpha(&rgb, &grey).unwrap(),
            [1, 2, 3, 7, 4, 5, 6, 8]
        );
        let red = Image {
            channels: 3,
            pixels: vec![9, 0, 0, 10, 0, 0],
            ..grey.clone()
        };
        assert_eq!(
            merge_jpeg_alpha(&rgb, &red).unwrap(),
            [1, 2, 3, 9, 4, 5, 6, 10]
        );
        assert!(merge_jpeg_alpha(&grey, &grey).is_err());
    }

    #[test]
    fn an_unknown_executable_has_no_table() {
        assert!(matches!(
            RgbTable::from_exe(b"not an executable"),
            Err(VideoError::Exe(_))
        ));
        assert!(RgbTable::from_bytes(&[0; 12]).is_err());
    }
}

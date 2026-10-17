//! HNM6 frame codec: the warp (panorama) keyframe decoder and the IX video
//! keyframe / interframe decoder.
//!
//! Translated from ScummVM's `Image::HNM6Decoder` (image/codecs/hnm.cpp,
//! GPLv3, the ScummVM team), in the bit-exact RGB565 mode the Atlantis
//! engine selects for both chunk types: IW keyframes as the original warp
//! decoder FUN_0045084c decodes them, IX frames as the original video
//! decoder FUN_004506ec does (including the read / write order of its
//! motion-copy routines, see `motion_order`).

use crate::motion_order;
use crate::DecodeError;

/// Size of the frame header: quality and five region offsets.
const HEADER_SIZE: usize = 6 * 4;

/// Classic JPEG quantization tables.
#[rustfmt::skip]
const LUMA_QUANT_TABLE: [i32; 64] = [
    16,  11,  10,  16,  24,  40,  51,  61,
    12,  12,  14,  19,  26,  58,  60,  55,
    14,  13,  16,  24,  40,  57,  69,  56,
    14,  17,  22,  29,  51,  87,  80,  62,
    18,  22,  37,  56,  68, 109, 103,  77,
    24,  35,  55,  64,  81, 104, 113,  92,
    49,  64,  78,  87, 103, 121, 120, 101,
    72,  92,  95,  98, 112, 100, 103,  99,
];

#[rustfmt::skip]
const CHROMA_QUANT_TABLE: [i32; 64] = [
    17,  18,  24,  47,  99,  99,  99,  99,
    18,  21,  26,  66,  99,  99,  99,  99,
    24,  26,  56,  99,  99,  99,  99,  99,
    47,  66,  99,  99,  99,  99,  99,  99,
    99,  99,  99,  99,  99,  99,  99,  99,
    99,  99,  99,  99,  99,  99,  99,  99,
    99,  99,  99,  99,  99,  99,  99,  99,
    99,  99,  99,  99,  99,  99,  99,  99,
];

/// Factors for the Arai, Agui & Nakajima IDCT.
#[rustfmt::skip]
const AAN_FACTORS: [i32; 64] = [
    16384, 22725, 21407, 19266, 16384, 12873,  8867,  4520,
    22725, 31521, 29692, 26722, 22725, 17855, 12299,  6270,
    21407, 29692, 27969, 25172, 21407, 16819, 11585,  5906,
    19266, 26722, 25172, 22654, 19266, 15137, 10426,  5315,
    16384, 22725, 21407, 19266, 16384, 12873,  8867,  4520,
    12873, 17855, 16819, 15137, 12873, 10114,  6967,  3552,
     8867, 12299, 11585, 10426,  8867,  6967,  4799,  2446,
     4520,  6270,  5906,  5315,  4520,  3552,  2446,  1247,
];

/// Classic JPEG zig-zag order.
#[rustfmt::skip]
const ZIGZAG: [u8; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// Which rows (high byte) and columns (low byte) hold a coefficient once the
/// coefficient of zig-zag index `i` is set; lets the IDCT skip empty ones.
#[rustfmt::skip]
const MASKS: [u16; 64] = [
    0x0101, 0x0102, 0x0201, 0x0401, 0x0202, 0x0104, 0x0108, 0x0204,
    0x0402, 0x0801, 0x1001, 0x0802, 0x0404, 0x0208, 0x0110, 0x0120,
    0x0210, 0x0408, 0x0804, 0x1002, 0x2001, 0x4001, 0x2002, 0x1004,
    0x0808, 0x0410, 0x0220, 0x0140, 0x0180, 0x0240, 0x0420, 0x0810,
    0x1008, 0x2004, 0x4002, 0x8001, 0x8002, 0x4004, 0x2008, 0x1010,
    0x0820, 0x0440, 0x0280, 0x0480, 0x0840, 0x1020, 0x2010, 0x4008,
    0x8004, 0x8008, 0x4010, 0x2020, 0x1040, 0x0880, 0x1080, 0x2040,
    0x4020, 0x8010, 0x8020, 0x4040, 0x2080, 0x4080, 0x8040, 0x8080,
];

#[rustfmt::skip]
const S2: [i8; 32] = [
     1,  1,  1,  2,  1, -2,  1, -1,
     2,  1,  2,  2,  2, -2,  2, -1,
    -2,  1, -2,  2, -2, -2, -2, -1,
    -1,  1, -1,  2, -1, -2, -1, -1,
];

#[rustfmt::skip]
const S4: [i8; 16] = [
     1,  2,  3,  4,  5,  6,  7,  8,
    -8, -7, -6, -5, -4, -3, -2, -1,
];

fn le32(d: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([d[pos], d[pos + 1], d[pos + 2], d[pos + 3]])
}

fn le16(d: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([d[pos], d[pos + 1]])
}

/// Block-type bits, read MSB first from little-endian 32-bit words.
struct BitBuffer<'a> {
    data: &'a [u8],
    pos: usize,
    reg: u32,
    bits: u32,
}

impl<'a> BitBuffer<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitBuffer {
            data,
            pos: 0,
            reg: 0,
            bits: 0,
        }
    }

    fn next(&mut self) -> Result<u8, DecodeError> {
        if self.bits == 0 {
            if self.pos + 4 > self.data.len() {
                return Err(DecodeError::Bitstream("bit buffer exhausted"));
            }
            self.reg = le32(self.data, self.pos);
            self.pos += 4;
            self.bits = 32;
        }
        let ret = (self.reg >> 31) as u8;
        self.reg <<= 1;
        self.bits -= 1;
        Ok(ret)
    }
}

/// 16-bit motion words.
struct MotionBuffer<'a> {
    data: &'a [u8],
    pos: usize,
}

impl MotionBuffer<'_> {
    fn next(&mut self) -> Result<u16, DecodeError> {
        if self.pos + 2 > self.data.len() {
            return Err(DecodeError::Bitstream("motion buffer exhausted"));
        }
        let ret = le16(self.data, self.pos);
        self.pos += 2;
        Ok(ret)
    }
}

/// 12-bit short motion words, packed two per three bytes.
struct ShortMotionBuffer<'a> {
    data: &'a [u8],
    pos: usize,
    // 0x8000 marks a pending second word
    other: u16,
}

impl ShortMotionBuffer<'_> {
    fn next(&mut self) -> Result<u16, DecodeError> {
        if self.other != 0 {
            let ret = self.other & 0xfff;
            self.other = 0;
            return Ok(ret);
        }
        let size = self.data.len();
        if self.pos + 2 > size {
            return Err(DecodeError::Bitstream("short motion buffer exhausted"));
        }
        if self.pos + 3 > size {
            // At the end: the last 2 bytes hold a last word
            let ret = le16(self.data, self.pos);
            self.pos += 2;
            return Ok(ret & 0xfff);
        }
        let full = u32::from(le16(self.data, self.pos)) | u32::from(self.data[self.pos + 2]) << 16;
        self.pos += 3;
        self.other = ((full >> 12) & 0xfff) as u16 | 0x8000;
        Ok((full & 0xfff) as u16)
    }
}

/// 4-bit coefficient codes, low nibble first.
struct JpegBuffer<'a> {
    /// The frame buffer from the start of the region: ScummVM checks
    /// `pos > size`, which lets one byte past the region be read (whatever
    /// the frame buffer holds there; 0 past its end).
    data: &'a [u8],
    size: usize,
    pos: usize,
    // 0x80 marks a pending high nibble
    other: u8,
}

impl JpegBuffer<'_> {
    fn next(&mut self) -> Result<u8, DecodeError> {
        if self.other != 0 {
            let ret = self.other & 0xf;
            self.other = 0;
            return Ok(ret);
        }
        if self.pos > self.size {
            return Err(DecodeError::Bitstream("coefficient buffer exhausted"));
        }
        let full = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        self.other = (full >> 4) | 0x80;
        Ok(full & 0xf)
    }
}

/// One pass of the optimized Arai, Agui & Nakajima IDCT over the rows or
/// columns whose bit is set in `mask` (stopping after the last one).
fn aan_stride(inout: &mut [i32; 64], stride: usize, inc: usize, mut mask: u8) {
    let mut base = 0;
    let mut i = 0;
    while i < 8 && mask != 0 {
        let x = |k: usize| inout[base + k * stride];
        let x2n6 = x(2).wrapping_sub(x(6));
        let x2p6 = x(2).wrapping_add(x(6));
        let x0n4 = x(0).wrapping_sub(x(4));
        let x0p4 = x(0).wrapping_add(x(4));

        let x5n3 = x(5).wrapping_sub(x(3));
        let x3p5 = x(3).wrapping_add(x(5));
        let x1n7 = x(1).wrapping_sub(x(7));
        let x1p7 = x(1).wrapping_add(x(7));

        let tmp0 = (3i32.wrapping_mul(x2n6) >> 1).wrapping_sub(x2p6);
        let tmp1 = 3i32.wrapping_mul(x1p7.wrapping_sub(x3p5)) >> 1;
        let tmp2 = 30i32.wrapping_mul(x5n3.wrapping_add(x1n7));
        let tmp3 = 17i32.wrapping_mul(x1n7).wrapping_sub(tmp2) >> 4;
        let tmp4 = tmp2.wrapping_sub(40i32.wrapping_mul(x5n3)) >> 4;
        let tmp5 = tmp4.wrapping_sub(x3p5).wrapping_sub(x1p7);

        let out = [
            x1p7.wrapping_add(x3p5)
                .wrapping_add(x0p4)
                .wrapping_add(x2p6),
            tmp0.wrapping_add(x0n4)
                .wrapping_add(tmp4)
                .wrapping_sub(x3p5)
                .wrapping_sub(x1p7),
            x0n4.wrapping_sub(tmp0)
                .wrapping_add(tmp1)
                .wrapping_sub(tmp5),
            x0p4.wrapping_sub(x2p6)
                .wrapping_sub(tmp1)
                .wrapping_add(tmp5)
                .wrapping_sub(tmp3),
            x0p4.wrapping_sub(x2p6)
                .wrapping_add(tmp1)
                .wrapping_sub(tmp5)
                .wrapping_add(tmp3),
            x0n4.wrapping_sub(tmp0)
                .wrapping_sub(tmp1)
                .wrapping_add(tmp5),
            tmp0.wrapping_add(x0n4)
                .wrapping_sub(tmp4)
                .wrapping_add(x3p5)
                .wrapping_add(x1p7),
            x0p4.wrapping_add(x2p6)
                .wrapping_sub(x1p7)
                .wrapping_sub(x3p5),
        ];
        for (k, v) in out.into_iter().enumerate() {
            inout[base + k * stride] = v;
        }

        i += 1;
        base += inc;
        mask >>= 1;
    }
}

/// Inverse DCT of one plane; `mask` tells which rows (high byte) and
/// columns (low byte) hold coefficients.
fn aan_idct(inout: &mut [i32; 64], mask: u16) {
    if mask == 0x0101 {
        // One coefficient in the top left corner: a constant block
        let dc = inout[0];
        inout.fill(dc);
        return;
    }

    let rowmask = (mask >> 8) as u8;
    let colmask = mask as u8;

    // The original decoder compares the masks signed, so 0x80 < 0x01 and
    // the pass order is not always the optimal one; this changes rounding.
    let row_first = (rowmask as i8) < (colmask as i8);
    if row_first {
        // Inside row
        aan_stride(inout, 1, 8, rowmask);
        if rowmask == 0x01 {
            // Only one line filled: copy it to the others
            let (first, rest) = inout.split_at_mut(8);
            for line in rest.as_chunks_mut::<8>().0 {
                line.copy_from_slice(first);
            }
        } else {
            aan_stride(inout, 8, 1, 0xff);
        }
    } else {
        // Inside column
        aan_stride(inout, 8, 1, colmask);
        if colmask == 0x01 {
            // Only one column filled: copy it to the others
            for line in inout.as_chunks_mut::<8>().0 {
                let v = line[0];
                line.fill(v);
            }
        } else {
            aan_stride(inout, 1, 8, 0xff);
        }
    }
}

/// The frame's quantization tables for `quality`, in zig-zag order and
/// premultiplied by the AAN factors.
fn quant_tables(quality: i32) -> ([i32; 64], [i32; 64]) {
    let quality = quality.unsigned_abs().clamp(1, 100);
    let qf = if quality >= 50 {
        200 - 2 * quality
    } else {
        5000 / quality
    };

    let make = |base: &[i32; 64]| {
        std::array::from_fn(|i| {
            let z = usize::from(ZIGZAG[i]);
            let q = ((base[z] as u32 * qf + 50) / 100) as i32;
            (q.clamp(8, 255) * AAN_FACTORS[z]) >> 13
        })
    };
    (make(&LUMA_QUANT_TABLE), make(&CHROMA_QUANT_TABLE))
}

/// The source pixel (y * 8 + x in the block read) of destination pixel
/// (x, y) of an `sx` x `sy` copy under transform `xform`.
fn motion_source(xform: u8, sx: usize, sy: usize, x: usize, y: usize) -> usize {
    match xform {
        0 => y * 8 + x,
        1 => y * 8 + (sx - 1 - x),
        2 => (sy - 1 - y) * 8 + x,
        3 => (sy - 1 - y) * 8 + (sx - 1 - x),
        4 => (sx - 1 - x) * 8 + (sy - 1 - y),
        5 => (sx - 1 - x) * 8 + y,
        6 => x * 8 + (sy - 1 - y),
        _ => x * 8 + y,
    }
}

/// The read / write order table of the original's IX copy routine for an
/// `sx` x `sy` block under transform `xform`.
fn motion_order(xform: u8, sx: usize, sy: usize) -> &'static [u8] {
    let x = usize::from(xform);
    match (sx, sy) {
        (8, 8) => &motion_order::ORDER_8X8[x],
        (4, 8) => &motion_order::ORDER_4X8[x],
        (8, 4) => &motion_order::ORDER_8X4[x],
        (4, 4) => &motion_order::ORDER_4X4[x],
        (2, 4) => &motion_order::ORDER_2X4[x],
        (4, 2) => &motion_order::ORDER_4X2[x],
        _ => &motion_order::ORDER_2X2[x],
    }
}

/// The frame buffers: RGB565, one `u16` each, rows contiguous. `cur` and
/// `old` are the offsets of the frame being decoded and of the previous one
/// in `px` (both 0 for warp frames, which have no previous frame).
struct Planes<'a> {
    px: &'a mut [u16],
    w: usize,
    h: usize,
    cur: usize,
    old: usize,
}

impl Planes<'_> {
    /// Index in `px` of pixel (x, y) of the frame at `base`. Coordinates
    /// may leave the line: rows are contiguous, so a block that starts left
    /// of x = 0 ends on the previous row, as in the original.
    fn index(&self, base: usize, x: isize, y: isize) -> Result<usize, DecodeError> {
        let i = y * self.w as isize + x;
        if i < 0 || i as usize >= self.w * self.h {
            return Err(DecodeError::Bitstream("motion source outside the frame"));
        }
        Ok(base + i as usize)
    }

    /// Copies an `sx` x `sy` block from (srx, sry) of the frame at `src` to
    /// (dx, dy) of the current frame, under transform `xform`, in the copy
    /// order of ScummVM's `doMotion` (with `src` the current frame, the
    /// source may overlap pixels already written by this copy).
    fn motion(
        &mut self,
        xform: u8,
        (sx, sy): (isize, isize),
        src: usize,
        (srx, sry): (isize, isize),
        (dx, dy): (usize, usize),
    ) -> Result<(), DecodeError> {
        let (dx, dy) = (dx as isize, dy as isize);
        match xform {
            // Copy, vertical flip: one memmove per row
            0 | 2 => {
                for y in 0..sy {
                    let src_y = if xform == 0 {
                        sry + y
                    } else {
                        sry + sy - 1 - y
                    };
                    let s = self.index(src, srx, src_y)?;
                    self.index(src, srx + sx - 1, src_y)?;
                    let d = self.index(self.cur, dx, dy + y)?;
                    self.px.copy_within(s..s + sx as usize, d);
                }
            }
            // Pixel by pixel, row by row
            1 | 3..=7 => {
                for y in 0..sy {
                    for x in 0..sx {
                        let (cx, cy) = match xform {
                            1 => (sx - 1 - x, y),          // horizontal flip
                            3 => (sx - 1 - x, sy - 1 - y), // cross flip
                            4 => (sy - 1 - y, sx - 1 - x), // forward flip
                            5 => (y, sx - 1 - x),          // forward rotate
                            6 => (sy - 1 - y, x),          // backward rotate
                            _ => (y, x),                   // backward flip
                        };
                        let s = self.index(src, srx + cx, sry + cy)?;
                        let d = self.index(self.cur, dx + x, dy + y)?;
                        self.px[d] = self.px[s];
                    }
                }
            }
            _ => unreachable!("motion transforms are 3 bits"),
        }
        Ok(())
    }

    /// The IX motion copy in the order of atlantis.exe's routines: when the
    /// source lies in the frame being decoded and overlaps the destination,
    /// a pixel takes the value it had when the original read it.
    fn motion_ordered(
        &mut self,
        xform: u8,
        (sx, sy): (usize, usize),
        src: usize,
        (srx, sry): (isize, isize),
        (dx, dy): (usize, usize),
    ) -> Result<(), DecodeError> {
        let order = motion_order(xform, sx, sy);
        let mut tmp = [0u16; 64];
        for &ev in &order[..2 * sx * sy] {
            let x = usize::from(ev & 7);
            let y = usize::from((ev >> 3) & 7);
            if ev & 0x80 != 0 {
                let d = self.index(self.cur, (dx + x) as isize, (dy + y) as isize)?;
                self.px[d] = tmp[motion_source(xform, sx, sy, x, y)];
            } else {
                let s = self.index(src, srx + x as isize, sry + y as isize)?;
                tmp[y * 8 + x] = self.px[s];
            }
        }
        Ok(())
    }

    /// Converts an 8x8 block of Y, U, V coefficients to RGB565 at (x, y) of
    /// the current frame, with the original's green clip and channel
    /// truncation.
    fn yuv_to_rgb(&mut self, x: usize, y: usize, coeffs: &[i32; 192]) {
        let cr_of = |v: i32| (16 * (v - 256) + 8) / 10;
        let cb_of = |v: i32| (v - 256) / 3;
        for ry in 0..8 {
            let line = self.cur + (y + ry) * self.w + x;
            for rx in 0..8 {
                let c = ry * 8 + rx;
                let cy = coeffs[c] >> 4;
                let cu = coeffs[64 + c];
                let cv = coeffs[128 + c];

                let cr = cr_of((cv >> 4) + 256);
                let cb = cb_of((cu >> 4) + 256);

                // The original clips G a little more and keeps 5/6/5 bits
                let r = (cy + 128 + cr).clamp(0, 255) as u16 & 0xf8;
                let g = (cy + 128 - (cr >> 1) - cb).clamp(0, 251) as u16 & 0xfc;
                let b = (cy + 128 + (cu >> 3)).clamp(0, 255) as u16 & 0xf8;

                self.px[line + rx] = (r << 8) | (g << 3) | (b >> 3);
            }
        }
    }
}

/// Coordinate of the 8x8 block holding `v`.
fn balign(v: usize) -> isize {
    (v & !7) as isize
}

/// Wraps a motion source around the line horizontally.
fn wrap_line(x: isize, w: usize) -> isize {
    let w = w as isize;
    if x < 0 {
        x + w
    } else if x >= w {
        x - w
    } else {
        x
    }
}

/// The state of one frame's decoding.
struct FrameDecoder<'a> {
    planes: Planes<'a>,
    bitbuf: BitBuffer<'a>,
    motion: MotionBuffer<'a>,
    shortmo: ShortMotionBuffer<'a>,
    jpeg: JpegBuffer<'a>,
    coeffs: [i32; 192],
    luma_quant: [i32; 64],
    chroma_quant: [i32; 64],
}

impl FrameDecoder<'_> {
    fn bit(&mut self) -> Result<bool, DecodeError> {
        Ok(self.bitbuf.next()? != 0)
    }

    /// Three transform bits from the bit buffer, or two and the motion
    /// word's top bit.
    fn xform_bits(&mut self, low: Option<u16>) -> Result<u8, DecodeError> {
        let hi = self.bitbuf.next()? << 2;
        let mid = self.bitbuf.next()? << 1;
        let lo = match low {
            Some(moword) => (moword >> 15) as u8,
            None => self.bitbuf.next()?,
        };
        Ok(hi | mid | lo)
    }

    fn blocks(
        &mut self,
        mut f: impl FnMut(&mut Self, usize, usize) -> Result<(), DecodeError>,
    ) -> Result<(), DecodeError> {
        for y in (0..self.planes.h).step_by(8) {
            for x in (0..self.planes.w).step_by(8) {
                f(self, x, y)?;
            }
        }
        Ok(())
    }

    // ----- IW: warp keyframes (panoramas) -----

    fn decode_iw_kf88(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            if self.bit()? {
                // 11: keyblock
                self.render_keyblock(x, y)
            } else {
                // 10: motion
                self.render_iw_motion(8, x, y)
            }
        } else {
            // 0: cross-cut
            self.decode_iw_kf44(x, y)?;
            self.decode_iw_kf44(x + 4, y)?;
            self.decode_iw_kf44(x, y + 4)?;
            self.decode_iw_kf44(x + 4, y + 4)
        }
    }

    fn decode_iw_kf44(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            // 1: double cross-cut
            self.render_iw_shortmo(x, y)?;
            self.render_iw_shortmo(x + 2, y)?;
            self.render_iw_shortmo(x, y + 2)?;
            self.render_iw_shortmo(x + 2, y + 2)
        } else {
            // 0: motion
            self.render_iw_motion(4, x, y)
        }
    }

    // ----- IX keyframes -----

    fn decode_ix_kf88(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            if self.bit()? {
                if self.bit()? {
                    // 111: motion
                    self.render_ix_kf_motion((8, 8), false, x, y)
                } else {
                    // 110: keyblock
                    self.render_keyblock(x, y)
                }
            } else {
                // 10: cross-cut
                self.decode_ix_kf44(x, y)?;
                self.decode_ix_kf44(x + 4, y)?;
                self.decode_ix_kf44(x, y + 4)?;
                self.decode_ix_kf44(x + 4, y + 4)
            }
        } else if self.bit()? {
            // 01: V-cut
            self.decode_ix_kf48(x, y)?;
            self.decode_ix_kf48(x + 4, y)
        } else {
            // 00: H-cut
            self.decode_ix_kf84(x, y)?;
            self.decode_ix_kf84(x, y + 4)
        }
    }

    fn decode_ix_kf48(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            // 1: motion
            self.render_ix_kf_motion((4, 8), false, x, y)
        } else {
            // 0: cross-cut
            self.decode_ix_kf44(x, y)?;
            self.decode_ix_kf44(x, y + 4)
        }
    }

    fn decode_ix_kf84(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            // 1: motion
            self.render_ix_kf_motion((8, 4), false, x, y)
        } else {
            // 0: cross-cut
            self.decode_ix_kf44(x, y)?;
            self.decode_ix_kf44(x + 4, y)
        }
    }

    fn decode_ix_kf44(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            if self.bit()? {
                // 11: double cross-cut; 2x2 blocks are always small motions
                self.render_ix_kf_motion((2, 2), true, x, y)?;
                self.render_ix_kf_motion((2, 2), true, x + 2, y)?;
                self.render_ix_kf_motion((2, 2), true, x, y + 2)?;
                self.render_ix_kf_motion((2, 2), true, x + 2, y + 2)
            } else {
                // 10: DV-cut
                self.decode_ix_kf24(x, y)?;
                self.decode_ix_kf24(x + 2, y)
            }
        } else if self.bit()? {
            // 01: DH-cut
            self.decode_ix_kf42(x, y)?;
            self.decode_ix_kf42(x, y + 2)
        } else {
            // 00: motion
            self.render_ix_kf_motion((4, 4), false, x, y)
        }
    }

    fn decode_ix_kf24(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            // 1: double cross-cut
            self.render_ix_kf_motion((2, 2), true, x, y)?;
            self.render_ix_kf_motion((2, 2), true, x, y + 2)
        } else {
            // 0: small motion
            self.render_ix_kf_motion((2, 4), true, x, y)
        }
    }

    fn decode_ix_kf42(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            // 1: double cross-cut
            self.render_ix_kf_motion((2, 2), true, x, y)?;
            self.render_ix_kf_motion((2, 2), true, x + 2, y)
        } else {
            // 0: small motion
            self.render_ix_kf_motion((4, 2), true, x, y)
        }
    }

    // ----- IX interframes -----

    fn decode_ix_if88(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            if self.bit()? {
                if self.bit()? {
                    if self.bit()? {
                        // 1111: not a valid code
                        Err(DecodeError::Bitstream("unknown 8x8 interframe block code"))
                    } else {
                        // 1110: cross-cut
                        self.decode_ix_if44(x, y)?;
                        self.decode_ix_if44(x + 4, y)?;
                        self.decode_ix_if44(x, y + 4)?;
                        self.decode_ix_if44(x + 4, y + 4)
                    }
                } else {
                    // 110: skip
                    self.render_skip((8, 8), x, y)
                }
            } else if self.bit()? {
                // 101: V-cut
                self.decode_ix_if48(x, y)?;
                self.decode_ix_if48(x + 4, y)
            } else {
                // 100: H-cut
                self.decode_ix_if84(x, y)?;
                self.decode_ix_if84(x, y + 4)
            }
        } else if self.bit()? {
            if self.bit()? {
                // 011: keyblock
                self.render_keyblock(x, y)
            } else {
                // 010: motion
                self.render_ix_if_motion((8, 8), false, x, y)
            }
        } else {
            // 00: short motion
            self.render_ix_if_shortmo((8, 8), x, y)
        }
    }

    /// The 4x8 and 8x4 halves of a cut 8x8 block: `vertical` for 4x8.
    fn decode_ix_if_half(&mut self, vertical: bool, x: usize, y: usize) -> Result<(), DecodeError> {
        let size = if vertical { (4, 8) } else { (8, 4) };
        if self.bit()? {
            if self.bit()? {
                // 11: skip
                self.render_skip(size, x, y)
            } else if vertical {
                // 10: cross-cut
                self.decode_ix_if44(x, y)?;
                self.decode_ix_if44(x, y + 4)
            } else {
                // 10: cross-cut
                self.decode_ix_if44(x, y)?;
                self.decode_ix_if44(x + 4, y)
            }
        } else if self.bit()? {
            // 01: motion
            self.render_ix_if_motion(size, false, x, y)
        } else {
            // 00: short motion
            self.render_ix_if_shortmo(size, x, y)
        }
    }

    fn decode_ix_if48(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        self.decode_ix_if_half(true, x, y)
    }

    fn decode_ix_if84(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        self.decode_ix_if_half(false, x, y)
    }

    fn decode_ix_if44(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            if self.bit()? {
                if self.bit()? {
                    // 111: double cross-cut
                    self.decode_ix_if22(x, y)?;
                    self.decode_ix_if22(x + 2, y)?;
                    self.decode_ix_if22(x, y + 2)?;
                    self.decode_ix_if22(x + 2, y + 2)
                } else {
                    // 110: DV-cut
                    self.decode_ix_if_small_half(true, x, y)?;
                    self.decode_ix_if_small_half(true, x + 2, y)
                }
            } else if self.bit()? {
                // 101: DH-cut
                self.decode_ix_if_small_half(false, x, y)?;
                self.decode_ix_if_small_half(false, x, y + 2)
            } else {
                // 100: skip
                self.render_skip((4, 4), x, y)
            }
        } else if self.bit()? {
            // 01: motion
            self.render_ix_if_motion((4, 4), false, x, y)
        } else {
            // 00: short motion
            self.render_ix_if_shortmo((4, 4), x, y)
        }
    }

    /// The 2x4 and 4x2 halves of a cut 4x4 block: `vertical` for 2x4.
    fn decode_ix_if_small_half(
        &mut self,
        vertical: bool,
        x: usize,
        y: usize,
    ) -> Result<(), DecodeError> {
        let size = if vertical { (2, 4) } else { (4, 2) };
        if self.bit()? {
            if self.bit()? {
                if self.bit()? {
                    // 111: double cross-cut
                    self.decode_ix_if22(x, y)?;
                    if vertical {
                        self.decode_ix_if22(x, y + 2)
                    } else {
                        self.decode_ix_if22(x + 2, y)
                    }
                } else {
                    // 110: skip
                    self.render_skip(size, x, y)
                }
            } else {
                // 10: small motion
                self.render_ix_if_motion(size, true, x, y)
            }
        } else {
            // 0: short motion
            self.render_ix_if_shortmo(size, x, y)
        }
    }

    fn decode_ix_if22(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        if self.bit()? {
            if self.bit()? {
                // 11: skip
                self.render_skip((2, 2), x, y)
            } else {
                // 10: small motion
                self.render_ix_if_motion((2, 2), true, x, y)
            }
        } else {
            // 0: short motion
            self.render_ix_if_shortmo((2, 2), x, y)
        }
    }

    // ----- Block renderers -----

    fn render_keyblock(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        let mut plane = [0i32; 64];
        for p in 0..3 {
            let quant = if p == 0 {
                self.luma_quant
            } else {
                self.chroma_quant
            };
            self.render_plane(&mut plane, &quant)?;
            self.coeffs[p * 64..(p + 1) * 64].copy_from_slice(&plane);
        }
        self.planes.yuv_to_rgb(x, y, &self.coeffs);
        Ok(())
    }

    /// Reads one plane's coefficients and runs the IDCT on them.
    fn render_plane(&mut self, dst: &mut [i32; 64], quant: &[i32; 64]) -> Result<(), DecodeError> {
        dst.fill(0);
        let mut i = 0usize;
        // Rows and columns holding values, to shorten the IDCT
        let mut mask = 0u16;
        let mut write = |i: &mut usize, v: i32| {
            if *i >= 64 {
                return Err(DecodeError::Bitstream("too many coefficients in a block"));
            }
            dst[usize::from(ZIGZAG[*i])] = v * quant[*i];
            mask |= MASKS[*i];
            *i += 1;
            Ok(())
        };
        let jpeg = &mut self.jpeg;
        let byte = |jpeg: &mut JpegBuffer| -> Result<i32, DecodeError> {
            let hi = jpeg.next()?;
            Ok(i32::from(((hi << 4) | jpeg.next()?) as i8))
        };

        let dc = byte(jpeg)?;
        write(&mut i, dc)?;
        while i < 64 {
            match jpeg.next()? {
                // Finish with zeros
                0 => i = 64,
                1 => i += 5,
                2 => {
                    i += 1;
                    write(&mut i, 1)?;
                }
                3 => {
                    i += 1;
                    write(&mut i, -1)?;
                }
                4 => {
                    i += 2;
                    write(&mut i, 1)?;
                }
                5 => {
                    i += 2;
                    write(&mut i, -1)?;
                }
                6 => {
                    i += 3;
                    write(&mut i, 1)?;
                }
                7 => {
                    i += 3;
                    write(&mut i, -1)?;
                }
                8 => {
                    let b = jpeg.next()?;
                    let z = ((b >> 2) & 0x3) + 1;
                    let mut t = (b & 0x3) + 2;
                    i += usize::from(z);
                    while t > 1 {
                        let p = 2 * usize::from(jpeg.next()?);
                        write(&mut i, S2[p].into())?;
                        write(&mut i, S2[p + 1].into())?;
                        t -= 2;
                    }
                    if t != 0 {
                        let p = 2 * usize::from(jpeg.next()?);
                        write(&mut i, S2[p].into())?;
                    }
                }
                9 => {
                    let b = jpeg.next()?;
                    let z = ((b >> 2) & 0x3) + 1;
                    let t = (b & 0x3) + 1;
                    i += usize::from(z);
                    for _ in 0..t {
                        let s = S4[usize::from(jpeg.next()?)];
                        write(&mut i, s.into())?;
                    }
                }
                10 => {
                    let p = 2 * usize::from(jpeg.next()?);
                    write(&mut i, S2[p].into())?;
                    write(&mut i, S2[p + 1].into())?;
                }
                // 13, 12 and 11: three, two or one 4-bit values
                n @ 11..=13 => {
                    for _ in 0..(n - 10) {
                        let s = S4[usize::from(jpeg.next()?)];
                        write(&mut i, s.into())?;
                    }
                }
                14 => i += 1,
                _ => {
                    let v = byte(jpeg)?;
                    write(&mut i, v)?;
                }
            }
        }

        aan_idct(dst, mask);
        Ok(())
    }

    /// IW motion: an `size` x `size` copy from earlier in the panorama,
    /// wrapping around the line horizontally.
    fn render_iw_motion(&mut self, size: isize, x: usize, y: usize) -> Result<(), DecodeError> {
        let moword = self.motion.next()?;
        let xform = self.xform_bits(Some(moword))?;

        let offy = if size == 8 { 0 } else { 4 };
        let srx = balign(x) + 128 - ((moword >> 7) & 0xff) as isize;
        let sry = balign(y) + offy - (moword & 0x7f) as isize;
        let srx = wrap_line(srx, self.planes.w);
        let cur = self.planes.cur;
        self.planes
            .motion(xform, (size, size), cur, (srx, sry), (x, y))
    }

    /// IW short motion: a 2x2 copy from close by.
    fn render_iw_shortmo(&mut self, x: usize, y: usize) -> Result<(), DecodeError> {
        let moword = self.shortmo.next()?;
        let xform = ((moword >> 1) & 7) as u8;

        let mut index = ((moword & 0x1) << 8 | (moword & 0xf0) | (moword >> 8)) as isize;
        let (xmotion, ymotion);
        if index < 12 * 8 {
            xmotion = -2 - index % 12;
            ymotion = 6 - index / 12;
        } else {
            index -= 12 * 8;
            xmotion = 19 - index % 32;
            let y = -2 - index / 32;
            ymotion = if y <= -8 { y - 1 } else { y };
        }

        // A source beyond the line ends up on the previous or next line:
        // this is expected (rows are contiguous).
        let srx = balign(x) + xmotion;
        let sry = balign(y) + ymotion;
        let cur = self.planes.cur;
        self.planes.motion(xform, (2, 2), cur, (srx, sry), (x, y))
    }

    /// IX keyframe motion: a copy from earlier in the frame being decoded.
    fn render_ix_kf_motion(
        &mut self,
        size: (usize, usize),
        small: bool,
        x: usize,
        y: usize,
    ) -> Result<(), DecodeError> {
        let moword = self.motion.next()?;
        let (xform, srx, sry);
        if small {
            xform = ((moword >> 12) & 7) as u8;
            // A source beyond the line ends up on the previous or next line
            srx = balign(x) + 63 - (moword & 0x7f) as isize;
            sry = balign(y) + 6 - ((moword >> 7) & 0x1f) as isize;
        } else {
            xform = self.xform_bits(Some(moword))?;
            let offy = if size == (8, 8) { 0 } else { 4 };
            let rx = balign(x) + 128 - ((moword >> 7) & 0xff) as isize;
            srx = wrap_line(rx, self.planes.w);
            sry = balign(y) + offy - (moword & 0x7f) as isize;
        }
        let cur = self.planes.cur;
        self.planes
            .motion_ordered(xform, size, cur, (srx, sry), (x, y))
    }

    /// IX skip: the block of the previous frame, unchanged.
    fn render_skip(
        &mut self,
        (sx, sy): (usize, usize),
        x: usize,
        y: usize,
    ) -> Result<(), DecodeError> {
        let old = self.planes.old;
        let pos = (x as isize, y as isize);
        self.planes
            .motion(0, (sx as isize, sy as isize), old, pos, (x, y))
    }

    /// IX interframe motion: a copy from the frame being decoded or from
    /// the previous one.
    fn render_ix_if_motion(
        &mut self,
        size: (usize, usize),
        small: bool,
        x: usize,
        y: usize,
    ) -> Result<(), DecodeError> {
        let moword = self.motion.next()?;
        let use_current = moword & 0x8000 != 0;
        let (xform, srx, sry);
        if small {
            xform = ((moword >> 12) & 7) as u8;
            // A source beyond the line ends up on the previous or next line
            if use_current {
                srx = balign(x) + 63 - (moword & 0x7f) as isize;
                sry = balign(y) + 6 - ((moword >> 7) & 0x1f) as isize;
            } else {
                srx = balign(x) + 31 - (moword & 0x3f) as isize;
                sry = balign(y) + 31 - ((moword >> 6) & 0x3f) as isize;
            }
        } else {
            xform = self.xform_bits(None)?;
            let offy = match (use_current, size) {
                (true, (8, 8)) => 0,
                (true, _) => 4,
                (false, _) => 64,
            };
            let rx = balign(x) + 128 - ((moword >> 7) & 0xff) as isize;
            srx = wrap_line(rx, self.planes.w);
            sry = balign(y) + offy - (moword & 0x7f) as isize;
        }
        let src = if use_current {
            self.planes.cur
        } else {
            self.planes.old
        };
        self.planes
            .motion_ordered(xform, size, src, (srx, sry), (x, y))
    }

    /// IX interframe short motion: the previous frame's pixels a little
    /// away (on square rings around the block, not block aligned).
    fn render_ix_if_shortmo(
        &mut self,
        (sx, sy): (usize, usize),
        x: usize,
        y: usize,
    ) -> Result<(), DecodeError> {
        let mut somoword = self.shortmo.next()? as isize;
        let (ox, oy);
        if somoword == 0 {
            ox = 1;
            oy = 0;
        } else {
            somoword -= 1;
            let mut span = 1;
            while somoword >= 8 * span {
                somoword -= 8 * span;
                span += 1;
            }
            let base = -span + 1;
            let side_sz = (span * 8) / 4;
            let side = somoword / side_sz;
            let side_pos = somoword % side_sz;
            let (off_x, off_y) = match side {
                0 => (0, side_pos),
                1 => (side_pos + 1, side_sz - 1),
                2 => (side_sz, side_sz - 1 - side_pos - 1),
                _ => (side_sz - 1 - side_pos, -1),
            };
            ox = base + off_x;
            oy = base + off_y;
        }

        // Not block aligned; a source beyond the line ends up on the
        // previous or next line.
        let src = (x as isize + ox, y as isize + oy);
        let old = self.planes.old;
        self.planes
            .motion(0, (sx as isize, sy as isize), old, src, (x, y))
    }
}

/// Decodes HNM6 frames into one or two frame buffers.
pub(crate) struct Hnm6Decoder {
    w: usize,
    h: usize,
    /// The file header's largest frame size.
    buffer_size: usize,
    /// The frame data, kept from one frame to the next as ScummVM's (the
    /// coefficient reader may look one byte past a frame's data).
    buffer: Vec<u8>,
    /// The pixels: one frame (warp) or two frame buffers (video).
    block: Vec<u16>,
    video_mode: bool,
    cur: usize,
    old: usize,
}

impl Hnm6Decoder {
    /// A decoder of `width` x `height` frames of at most `buffer_size`
    /// bytes. Video mode keeps two frame buffers in `block`, at offsets 0
    /// and `width * height` (the block grows with zeros to hold them);
    /// warp mode one at offset 0.
    pub(crate) fn new(
        width: usize,
        height: usize,
        buffer_size: usize,
        video_mode: bool,
        mut block: Vec<u16>,
    ) -> Self {
        let frame = width * height;
        let need = if video_mode { 2 * frame } else { frame };
        if block.len() < need {
            block.resize(need, 0);
        }
        Hnm6Decoder {
            w: width,
            h: height,
            buffer_size,
            buffer: vec![0; buffer_size],
            block,
            video_mode,
            cur: 0,
            old: if video_mode { frame } else { 0 },
        }
    }

    /// The last decoded frame.
    pub(crate) fn frame(&self) -> &[u16] {
        &self.block[self.cur..self.cur + self.w * self.h]
    }

    pub(crate) fn into_block(self) -> Vec<u16> {
        self.block
    }

    /// Decodes one IW (`warp`) or IX chunk.
    pub(crate) fn decode_frame(&mut self, chunk: &[u8], warp: bool) -> Result<(), DecodeError> {
        // Video mode switches both buffers first: the frame lands in the
        // other one, the one just shown becomes the previous frame.
        if self.video_mode {
            std::mem::swap(&mut self.cur, &mut self.old);
        }

        if chunk.len() < HEADER_SIZE {
            return Err(DecodeError::InvalidFrameHeader);
        }
        let field = |n: usize| le32(chunk, 4 * n);
        let quality = field(0) as i32;
        let offsets: [usize; 5] = std::array::from_fn(|n| field(n + 1) as usize);
        let [bit_start, motion_start, shortmo_start, jpeg_start, end] = offsets;

        if offsets.iter().any(|&o| o > end) || end > self.buffer_size || end < HEADER_SIZE {
            return Err(DecodeError::InvalidFrameHeader);
        }
        if end > chunk.len() {
            return Err(DecodeError::Truncated);
        }
        // ScummVM reads regions of `next - start` bytes, unsigned; well
        // formed frames have them in order.
        if bit_start > motion_start || motion_start > shortmo_start || shortmo_start > jpeg_start {
            return Err(DecodeError::InvalidFrameHeader);
        }
        // Warp frames always carry a positive quality; a negative one marks
        // an IX keyframe.
        if warp && quality <= 0 {
            return Err(DecodeError::InvalidFrameHeader);
        }
        if warp == self.video_mode {
            return Err(DecodeError::Bitstream(if warp {
                "IW frame in a video file"
            } else {
                "IX frame in a warp file"
            }));
        }
        let keyframe = quality < 0;
        self.buffer[..end].copy_from_slice(&chunk[..end]);
        let data = &self.buffer[..];
        let (luma_quant, chroma_quant) = quant_tables(quality);

        let mut dec = FrameDecoder {
            planes: Planes {
                px: &mut self.block,
                w: self.w,
                h: self.h,
                cur: self.cur,
                old: self.old,
            },
            bitbuf: BitBuffer::new(&data[bit_start..motion_start]),
            motion: MotionBuffer {
                data: &data[motion_start..shortmo_start],
                pos: 0,
            },
            shortmo: ShortMotionBuffer {
                data: &data[shortmo_start..jpeg_start],
                pos: 0,
                other: 0,
            },
            jpeg: JpegBuffer {
                data: &data[jpeg_start..],
                size: end - jpeg_start,
                pos: 0,
                other: 0,
            },
            coeffs: [0; 192],
            luma_quant,
            chroma_quant,
        };
        if warp {
            dec.blocks(FrameDecoder::decode_iw_kf88)
        } else if keyframe {
            dec.blocks(FrameDecoder::decode_ix_kf88)
        } else {
            dec.blocks(FrameDecoder::decode_ix_if88)
        }
    }
}

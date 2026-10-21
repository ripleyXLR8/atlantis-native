//! 2D sprites: SPR banks (SPRITE\\2D\\*.SPR: cursors, fonts, objects,
//! menus, puzzles), SPW panorama sprites (SPRITE\\CYCLO\\*.SPW), SPF
//! transition sprites (SPRITE\\UBB\\*.SPF) and the 15-bit TGA pictures, and
//! their blitters onto RGB565 buffers.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (engines/cryomni3d/atlantis/sprite_decode.cpp and sprite_renderer.cpp,
//! GPLv3, the ScummVM team).
//!
//! SPR layout:
//!
//! ```text
//! u16 flags          bit 2: a 256-entry RGB555 colour table follows,
//!                    bit 3: a second 256-entry table follows (unused)
//! u16 table[256]     (bit 2)
//! u16 count
//! u32 offsets[count] relative to the position of the first offset
//! per sprite: u16 w, h; i16 xoff, yoff; u32 rowOffsets[h]; the RLE rows
//! ```
//!
//! Each row (FUN_0041047f) is a series of runs: a control byte whose top 2
//! bits are the run type and low 6 bits its length; 0 = transparent skip,
//! 1 = `len` colour indices, 2 and 3 = `len` (factor, index) edge pairs.
//!
//! SPW / SPF layout: u32 file size, u32 frame count, u32 format, then at
//! 0x14 the frame offsets (low 2 bits are flags). A frame: bounding box
//! (x1 u16 at +0, y_bottom u32 at +4, x2 u32 at +8, y_top u16 at +0xc),
//! format, at +0x14 the offset of the pixel stream, the RGB565 palette from
//! +0x18 up to it, then the pixel stream (see [`decode_spw_pixels`]).

use crate::reader::Reader;
use crate::sprite_blend::{blend_spr_pixel565, blend_spw_pixel565, SPR_NO_BLEND, SPR_TRANSPARENT};

/// Size limits the engine's SPR loaders pass to [`read_spr_file`].
pub mod limits {
    /// Most banks (SUJETS, FONTMAX/FONTMAX2/FONTMIN/FONTMIN2, CREDBLAN /
    /// CREDBLEU, SPRMENU, OBJETS1, puzzles).
    pub const DEFAULT: (u16, u16) = (640, 480);
    /// CURSEURS.SPR.
    pub const CURSORS: (u16, u16) = (64, 64);
    /// OBJETS0.SPR, the ~599x82 inventory bar background.
    pub const INVENTORY_BAR: (u16, u16) = (1024, 256);
}

/// An RGB565 picture, row by row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Image565 {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u16>,
}

/// One SPR sprite: RGB565 pixels and a parallel blend array
/// ([`SPR_NO_BLEND`] opaque, [`SPR_TRANSPARENT`] transparent, otherwise an
/// edge coverage factor 0..31).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SprSprite {
    pub w: u16,
    pub h: u16,
    /// The hotspot: the sprite is drawn at (x - xoff, y - yoff).
    pub xoff: i16,
    pub yoff: i16,
    pub pixels: Vec<u16>,
    pub blend: Vec<u8>,
}

impl SprSprite {
    /// Draws the sprite with its hotspot at (x, y) onto an RGB565 buffer of
    /// `width` x `height`, clipped, as FUN_00410430 / FUN_0041047f do.
    pub fn draw(&self, dst: &mut [u16], width: usize, height: usize, x: i32, y: i32) {
        if self.pixels.is_empty() {
            return;
        }
        let base_x = x - i32::from(self.xoff);
        let base_y = y - i32::from(self.yoff);
        let (w, h) = (usize::from(self.w), usize::from(self.h));
        for row in 0..h {
            let dy = base_y + row as i32;
            if dy < 0 || dy as usize >= height {
                continue;
            }
            for col in 0..w {
                let dx = base_x + col as i32;
                if dx < 0 || dx as usize >= width {
                    continue;
                }
                let s = row * w + col;
                let f = self.blend[s];
                if f == SPR_TRANSPARENT {
                    continue;
                }
                let Some(d) = dst.get_mut(dy as usize * width + dx as usize) else {
                    continue;
                };
                *d = if f == SPR_NO_BLEND {
                    self.pixels[s]
                } else {
                    blend_spr_pixel565(self.pixels[s], *d, f)
                };
            }
        }
    }
}

/// Decodes one sprite's rows (FUN_0041047f); `s` is at the row offset
/// table, `spr.w` / `spr.h` are set; `colors` maps the indices to RGB565.
fn decode_spr_rows(s: &mut Reader, spr: &mut SprSprite, colors: &[u16; 256]) {
    let (w, h) = (usize::from(spr.w), usize::from(spr.h));
    let row_off: Vec<u32> = (0..h).map(|_| s.u32()).collect();
    let pix_base = s.pos();
    spr.pixels = vec![0; w * h];
    spr.blend = vec![SPR_TRANSPARENT; w * h];
    for (ry, &off) in row_off.iter().enumerate() {
        s.seek(pix_base.saturating_add(off as usize));
        let mut rx = 0usize;
        while rx < w && !s.eos() {
            let b = s.u8();
            let cnt = usize::from(b & 0x3f);
            match b >> 6 {
                0 => rx += cnt,
                1 => {
                    let mut i = 0;
                    while i < cnt && rx < w {
                        spr.pixels[ry * w + rx] = colors[usize::from(s.u8())];
                        spr.blend[ry * w + rx] = SPR_NO_BLEND;
                        i += 1;
                        rx += 1;
                    }
                }
                _ => {
                    // (factor, index) pairs: the colour is premultiplied by
                    // factor/32; drawing adds the framebuffer's share.
                    let mut i = 0;
                    while i < cnt && rx < w {
                        let factor = s.u8() & 0x1f;
                        spr.pixels[ry * w + rx] = colors[usize::from(s.u8())];
                        spr.blend[ry * w + rx] = factor;
                        i += 1;
                        rx += 1;
                    }
                }
            }
        }
    }
}

/// Reads a whole SPR bank. A sprite of size 0 or larger than
/// `max_w` x `max_h` keeps its header and no pixels. `None` for a count of 0
/// or more than 256.
pub fn read_spr_file(data: &[u8], max_w: u16, max_h: u16) -> Option<Vec<SprSprite>> {
    let mut s = Reader::new(data);
    let flags = s.u16();
    let mut colors = [0u16; 256];
    if flags & 0x4 != 0 {
        // RGB555 to RGB565 as FUN_00410782 does: r << 11 | g << 6 | b.
        for c in colors.iter_mut() {
            let v = s.u16();
            let (r, g, b) = ((v >> 10) & 0x1f, (v >> 5) & 0x1f, v & 0x1f);
            *c = (r << 11) | (g << 6) | b;
        }
    }
    if flags & 0x8 != 0 {
        s.skip(256 * 2);
    }
    let count = s.u16();
    if count == 0 || count > 256 {
        return None;
    }
    let base = s.pos();
    let offsets: Vec<u32> = (0..count).map(|_| s.u32()).collect();
    let mut out = vec![SprSprite::default(); usize::from(count)];
    for (spr, &off) in out.iter_mut().zip(&offsets) {
        s.seek(base.saturating_add(off as usize));
        spr.w = s.u16();
        spr.h = s.u16();
        spr.xoff = s.i16();
        spr.yoff = s.i16();
        if spr.w == 0 || spr.h == 0 || spr.w > max_w || spr.h > max_h {
            continue;
        }
        decode_spr_rows(&mut s, spr, &colors);
    }
    Some(out)
}

/// Reads a game TGA: type 2, 15-bit, rows bottom-up, converted to RGB565
/// (FUN_00449600: ((v & 0x7fe0) << 1) | (v & 0x1f), then FUN_004174a0's
/// bottom-up copy; also FUN_0041b654). `None` for another format.
pub fn read_tga15(data: &[u8]) -> Option<Image565> {
    let mut s = Reader::new(data);
    let id_len = s.u8();
    s.u8(); // colour map type
    let img_type = s.u8();
    s.skip(9); // colour map spec (5) + x/y origin (4)
    let w = usize::from(s.u16());
    let h = usize::from(s.u16());
    let depth = s.u8();
    s.u8(); // image descriptor
    s.skip(usize::from(id_len));
    if img_type != 2 || depth != 15 || w == 0 || h == 0 {
        return None;
    }
    let mut pixels = vec![0u16; w * h];
    for y in 0..h {
        let row = &mut pixels[(h - 1 - y) * w..(h - y) * w];
        for p in row.iter_mut() {
            let v = s.u16();
            *p = ((v & 0x7fe0) << 1) | (v & 0x1f);
        }
    }
    Some(Image565 {
        width: w,
        height: h,
        pixels,
    })
}

/// The offset of frame `frame` of an SPW / SPF (low 2 bits masked).
pub fn spw_frame_offset(data: &[u8], frame: usize) -> u32 {
    let mut s = Reader::new(data);
    s.seek(0x14 + frame * 4);
    s.u32() & !3
}

/// The frame count of an SPW / SPF (header dword +4).
pub fn spw_frame_count(data: &[u8]) -> u32 {
    let mut s = Reader::new(data);
    s.seek(4);
    s.u32()
}

/// A frame's bounding box: (x1, y_top, x2, y_bottom).
pub fn spw_frame_bounds(data: &[u8], frame: usize) -> Option<(i16, i16, i16, i16)> {
    let off = spw_frame_offset(data, frame) as usize;
    if off == 0 || off + 0x0e > data.len() {
        return None;
    }
    let mut s = Reader::new(data);
    s.seek(off);
    let x1 = s.u16() as i16;
    s.u16();
    let y2 = s.u32() as i16;
    let x2 = s.u32() as i16;
    let y1 = s.u16() as i16;
    Some((x1, y1, x2, y2))
}

/// Draws frame `frame` of an SPW (or SPF) onto an RGB565 buffer of
/// `width` x `height`, as FUN_00452bde does.
///
/// The pixel stream writes through a flat cursor starting at pixel 0 of the
/// buffer; skips move it by any count, across rows. u16 opcodes: 0x8000
/// end; 0xc000 | hi then a u16 lo: skip hi * 65536 + lo; 0x8000 | n: skip
/// n; 0x4000 | n: n (index, factor) edge pixels; n (1..0x3fff): n palette
/// indices; 0: nothing. A skip that leaves the buffer ends the frame.
pub fn decode_spw_pixels(data: &[u8], dst: &mut [u16], width: usize, height: usize, frame: usize) {
    let frame_off = spw_frame_offset(data, frame) as usize;
    if frame_off == 0 {
        return;
    }
    let mut s = Reader::new(data);
    s.seek(frame_off + 0x14);
    let pix_off = s.u32() as usize;
    if pix_off < 0x18 {
        return;
    }
    let pal_count = ((pix_off - 0x18) / 2).min(256);
    let mut palette = [0u16; 256];
    s.seek(frame_off + 0x18);
    for p in palette.iter_mut().take(pal_count) {
        *p = s.u16();
    }
    s.seek(frame_off + pix_off);

    let color = |idx: u8| -> u16 {
        if usize::from(idx) < pal_count {
            palette[usize::from(idx)]
        } else {
            0
        }
    };
    let area = (width * height).min(dst.len()) as u64;
    let mut cur: u64 = 0;
    while !s.eos() {
        let ctrl = s.u16();
        if ctrl == 0x8000 {
            break;
        }
        if ctrl >= 0xc000 {
            let extra = s.u16();
            cur += u64::from(ctrl & 0x3fff) * 65536 + u64::from(extra);
            if cur >= area {
                break;
            }
        } else if ctrl >= 0x8000 {
            cur += u64::from(ctrl & 0x7fff);
            if cur >= area {
                break;
            }
        } else if ctrl >= 0x4000 {
            let mut i = 0;
            while i < (ctrl & 0x3fff) && !s.eos() {
                let idx = s.u8();
                let factor = s.u8();
                if cur < area {
                    let d = &mut dst[cur as usize];
                    *d = blend_spw_pixel565(color(idx), *d, factor);
                }
                cur += 1;
                i += 1;
            }
        } else if ctrl > 0 {
            let mut i = 0;
            while i < ctrl && !s.eos() {
                let idx = s.u8();
                if cur < area {
                    dst[cur as usize] = color(idx);
                }
                cur += 1;
                i += 1;
            }
        }
    }
}

/// Draws frame `frame` of an SPF onto `dst`. Like FUN_004256e8 (test at
/// 0x004257b6), nothing is drawn once `frame` reaches the frame count, of
/// which it reads only the low byte (byte load @ 0x00425764); no shipped
/// SPF has more than 139 frames.
pub fn composite_spf_frame(
    data: &[u8],
    frame: usize,
    dst: &mut [u16],
    width: usize,
    height: usize,
) {
    let count = data.get(4).copied().unwrap_or(0);
    if frame >= usize::from(count) {
        return;
    }
    decode_spw_pixels(data, dst, width, height, frame);
}

#[cfg(test)]
mod tests {
    use super::*;

    const FB_W: usize = 640;
    const FB_H: usize = 480;
    const CYC_W: usize = 2048;
    const CYC_H: usize = 768;

    /// The background of the original's runs: pixel i = (i * 2654435761) >> 16.
    fn background(n: usize) -> Vec<u16> {
        (0..n as u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 16) as u16)
            .collect()
    }

    fn md5_le(v: &[u16]) -> String {
        use md5::{Digest, Md5};
        let bytes: Vec<u8> = v.iter().flat_map(|p| p.to_le_bytes()).collect();
        Md5::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// A synthetic 8x3 SPR, hotspot (2, 1).
    fn synthetic_spr() -> Vec<u8> {
        let table: [u16; 6] = [0x0000, 0x0001, 0x7fff, 0x03e0, 0x001f, 0x8001];
        let row0: &[u8] = &[0x42, 1, 2, 0x02, 0x84, 16, 3, 31, 4, 0, 2, 8, 1];
        let row1: &[u8] = &[0x48, 0, 1, 2, 3, 4, 5, 1, 2];
        let row2: &[u8] = &[0x08];
        let mut d = Vec::new();
        let u16 = |d: &mut Vec<u8>, v: u16| d.extend_from_slice(&v.to_le_bytes());
        let u32 = |d: &mut Vec<u8>, v: u32| d.extend_from_slice(&v.to_le_bytes());
        u16(&mut d, 4);
        for i in 0..256 {
            u16(&mut d, table.get(i).copied().unwrap_or(0));
        }
        u16(&mut d, 1);
        u32(&mut d, 4);
        for v in [8, 3, 2, 1] {
            u16(&mut d, v);
        }
        u32(&mut d, 0);
        u32(&mut d, row0.len() as u32);
        u32(&mut d, (row0.len() + row1.len()) as u32);
        d.extend_from_slice(row0);
        d.extend_from_slice(row1);
        d.extend_from_slice(row2);
        d
    }

    /// A synthetic SPW: header and one format-9 frame.
    fn synthetic_spw() -> Vec<u8> {
        let mut f = vec![0u8; 0x18];
        f[0x10] = 9;
        f[0x14] = 0x18 + 12;
        for p in [0x0000u16, 0x0001, 0xffff, 0x07e0, 0xf800, 0x1234] {
            f.extend_from_slice(&p.to_le_bytes());
        }
        f.extend_from_slice(&[
            0x85, 0x82, // skip 645
            0x03, 0x00, 2, 3, 5, // draw 3
            0x04, 0x40, 4, 16, 2, 31, 1, 0, 5, 8, // blend 4 (index, factor)
            0x01, 0xc0, 0x02, 0x00, // long skip 65538
            0x02, 0x00, 4, 3, // draw 2
            0x00, 0x80, // end
        ]);
        let mut d = vec![0u8; 0x18];
        let size = (0x18 + f.len()) as u32;
        d[..4].copy_from_slice(&size.to_le_bytes());
        d[4] = 1;
        d[8] = 9;
        d[0x14] = 0x18;
        d.extend_from_slice(&f);
        d
    }

    // The synthetic SPR blitted by the original FUN_00410430 at hotspot
    // (12, 6). Rows 0 and 1 use table entry 1 (RGB555 0x0001 -> 0x0001).
    #[test]
    fn spr_synthetic_window() {
        let expected: [[u16; 8]; 3] = [
            [
                0x0001, 0xffdf, 0x200a, 0xbe42, 0x31ec, 0x001f, 0x98c7, 0x2541,
            ],
            [
                0x0000, 0x0001, 0xffdf, 0x07c0, 0x001f, 0x0001, 0x0001, 0xffdf,
            ],
            [
                0xf8fc, 0x9733, 0x356b, 0xd3a2, 0x71da, 0x1011, 0xae49, 0x4c80,
            ],
        ];
        let sprites = read_spr_file(&synthetic_spr(), 640, 480).unwrap();
        assert_eq!(sprites.len(), 1);
        let mut fb = background(FB_W * FB_H);
        sprites[0].draw(&mut fb, FB_W, FB_H, 12, 6);
        for (r, row) in expected.iter().enumerate() {
            for (c, &want) in row.iter().enumerate() {
                assert_eq!(fb[(5 + r) * FB_W + 10 + c], want, "pixel ({c},{r})");
            }
        }
        assert_eq!(md5_le(&fb), "7cd5c6039580578a54b1ec3d6a17e213");
    }

    #[test]
    fn spr_synthetic_clipping() {
        let sprites = read_spr_file(&synthetic_spr(), 640, 480).unwrap();
        let mut fb = background(FB_W * FB_H);
        sprites[0].draw(&mut fb, FB_W, FB_H, 641, 481);
        assert_eq!(md5_le(&fb), "3b034feb5c09d0701829bb2efbef37d8");
        let mut fb = background(FB_W * FB_H);
        sprites[0].draw(&mut fb, FB_W, FB_H, 1, 0);
        assert_eq!(md5_le(&fb), "2d3428ecd8ec318e1a6a5aa9f023e0b2");
    }

    // The synthetic SPW frame drawn by the original FUN_00452bde into a
    // 2048x768 buffer.
    #[test]
    fn spw_synthetic() {
        let d = synthetic_spw();
        let mut p = background(CYC_W * CYC_H);
        composite_spf_frame(&d, 0, &mut p, CYC_W, CYC_H);
        let at = [645, 646, 647, 648, 649, 650, 651, 66190, 66191];
        let val = [
            0xffff, 0x07e0, 0x1234, 0x3206, 0xffff, 0xb8db, 0x4f81, 0xf800, 0x07e0,
        ];
        for (a, v) in at.iter().zip(val) {
            assert_eq!(p[*a], v, "pixel {a}");
        }
        assert_eq!(md5_le(&p), "8db0dea5bf37a5fbe798b92e8b5f9949");
    }

    // FUN_004256e8 @ 0x004257b6..0x004257be draws an SPF only while the
    // frame counter is below the frame count (static reading).
    #[test]
    fn spf_index_past_end() {
        let d = synthetic_spw();
        let mut p = background(CYC_W * CYC_H);
        let before = md5_le(&p);
        composite_spf_frame(&d, 1, &mut p, CYC_W, CYC_H);
        assert_eq!(md5_le(&p), before);
    }

    // The frame count is the low byte of the header dword (0x00425764): a
    // count of 0x100 draws nothing, a count of 0x101 draws frame 0.
    #[test]
    fn spf_count_low_byte() {
        let mut d = synthetic_spw();
        d[4..8].copy_from_slice(&0x100u32.to_le_bytes());
        let mut p = background(CYC_W * CYC_H);
        let before = md5_le(&p);
        composite_spf_frame(&d, 0, &mut p, CYC_W, CYC_H);
        assert_eq!(md5_le(&p), before);
        d[4..8].copy_from_slice(&0x101u32.to_le_bytes());
        composite_spf_frame(&d, 0, &mut p, CYC_W, CYC_H);
        assert_eq!(md5_le(&p), "8db0dea5bf37a5fbe798b92e8b5f9949");
    }

    #[test]
    fn spr_count_limits() {
        assert!(read_spr_file(&[0, 0, 0, 0], 640, 480).is_none());
        assert!(read_spr_file(&[0, 0, 0x01, 0x01], 640, 480).is_none());
    }
}

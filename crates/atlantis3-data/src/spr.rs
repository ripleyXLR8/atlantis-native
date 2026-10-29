//! `SPRCRYO` / `SPRCRYO1` sprite banks and `SPWCRYO` place sprites, as the
//! game's 2001 `spr_p5.dll` / `spr_p6.dll` draw them.
//!
//! The executable reads a whole file after its 8-byte magic (`SPRCRYO ` or
//! `SPRCRYO1` @ 0x0069d220, `SPWCRYO ` @ 0x0069d4b0) and prepares it for
//! the display: flags & 0x80 on a 16-bit 565 display → `CONV_SPR32_64`
//! (the bank's direct colours from 555 to 565, [`conv_spr32_64`]); flags &
//! 4 → `SPR_CONVPAL32` (an RGB555 table of 256 u16); flags & 8 →
//! `SPR_CONVPAL16` (a table of 256 B, G, R, x), on a 16-bit display only:
//! a 32-bit display uses the table as it is.
//!
//! Layout after the magic, as in *Atlantis II* (`atlantis2_data::spr`): u32
//! flags; the colour table; u32 count; u32 offsets from the start of that
//! array (0 = empty slot); per sprite the default position (u16 x, y) when
//! flags & 0x800, u16 w, h, s16 hot spot x, y, u32\[h\] row offsets from
//! the end of that array, the rows. An `SPWCRYO` file (flags & 1) holds
//! frames instead of sprites: u32 offsets of the frames, per frame six u32
//! offsets of the cube faces from the frame's start (0 = no sprite on that
//! face), each face a sprite drawn with its hot spot at (0, 0).
//!
//! `AFF_SPR` (spr_p5 @ 0x100022b0) and `AFF_SPW` (@ 0x100023f0) choose a
//! blitter from `(flags & 0x3ff) >> 1` in one of two tables, for a 16-bit
//! display (@ 0x10002ca3) or a 32-bit one (@ 0x10002e83, descriptor byte 0
//! bit 1):
//!
//! | Flags & 0x3fe | 16 bits | 32 bits | Rows |
//! |---|---|---|---|
//! | 0x74 | @ 0x10007875 | — | palette, S runs with a factor byte |
//! | 0x78 (0x878) | @ 0x10006e75 | @ 0x1000ed80 | the same, B, G, R, x table |
//! | 0x274 | @ 0x10007d98 | — | palette, S runs at 50 % |
//! | 0x278 (0xa78) | @ 0x10007398 | @ 0x1000f2ba | the same, B, G, R, x table |
//! | 0xb0 | @ 0x10008275 | — | direct colour, two streams |
//! | 0x130 | — | @ 0x100122eb | direct 32-bit colour, two streams |
//! | 0xa0 | @ 0x100091f8 | — | direct colour, skip and copy runs only |
//!
//! Any other kind has no blitter (the call draws nothing) or one that no
//! shipped file uses (0x24, 0x28, 0x280 in 16 bits; 0x28, 0x120, 0x300 in
//! 32 bits): those are not here.
//!
//! The palette rows use *Atlantis II*'s run grammar
//! ([`atlantis2_data::spr::next_run`]): K skip, R palette index, P pair
//! (factor f, index): `pal[i] + shade(dst, f)`, S shade: `shade(dst, f)`
//! with one factor byte for the run, or `half(dst)` in the 0x200 family.
//! The direct-colour rows start with a u32 offset (from that u32) of their
//! control bytes; the pixels (u16, or u32 for 0x130) follow the u32, and P
//! runs take their factors, S runs their one factor, from the control
//! stream.

use std::fmt;

use atlantis2_data::spr::{half as half565, next_run, shade as shade565, to565, Run};

/// The sprite library: `spr_p5.dll` for a CPU class below 4, `spr_p6.dll`
/// from 4 (@ 0x0069c500; the classes as in *Atlantis II*). They draw
/// alike with `AFF_SPR` and `AFF_SPW` but in [`Pixel::shade_p5_right_clip`].
pub use atlantis2_data::spr::SprDll;

/// Why a file is not a sprite bank.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SprError {
    Truncated,
    /// Not `SPRCRYO `, `SPRCRYO1` or `SPWCRYO ` (error 4 of the loaders).
    NotSpr,
}

impl fmt::Display for SprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SprError::Truncated => write!(f, "truncated sprite bank"),
            SprError::NotSpr => write!(f, "not an SPRCRYO or SPWCRYO sprite bank"),
        }
    }
}

impl std::error::Error for SprError {}

/// The display the bank is prepared for (`PixelFormat` of `Atlantis3.cfg`:
/// `16bits` / `r5g6b5`, the default, or `32bits` / `x8r8g8b8`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Depth {
    /// 16-bit RGB565, descriptor `00 0b 03 05 02 00 03`.
    #[default]
    Rgb565,
    /// 32-bit x8r8g8b8, descriptor `02 10 00 08 00 00 00`.
    Xrgb8888,
}

/// A pixel of a display depth and the blitters' arithmetic on it.
pub trait Pixel: Copy {
    /// The display depth.
    const DEPTH: Depth;
    /// A colour-table entry as the blitters store it.
    fn from_entry(e: u32) -> Self;
    /// A direct colour of a 0xb0 / 0xa0 (u16) or 0x130 (u32) row.
    fn from_direct(d: &[u8], at: usize) -> Option<Self>;
    /// `shade(dst, f)`: the destination darkened by the factor f.
    fn shade(self, f: u8) -> Self;
    /// `c + shade(dst, f)` of a pair run.
    fn add_shaded(c: Self, dst: Self, f: u8) -> Self;
    /// The 50 % of the 0x200 family's S runs.
    fn half(self) -> Self;
    /// The S runs of `spr_p5.dll`'s 32-bit 0x130 blitter in its right-clip
    /// path (@ 0x1001269a): its MMX factor is unpacked in the wrong order
    /// (`punpckldq` before `punpcklwd`, @ 0x100126a5), so that only B and
    /// G are shaded and R and x become 0. `spr_p6.dll` and every other
    /// path shade the four bytes.
    fn shade_p5_right_clip(self, f: u8) -> Self {
        self.shade(f)
    }
    /// The translucent pixel of `AFF_SPRTRS` at level `l`: colour `c` over
    /// `dst`, the destination's factor `g` (`l`, or `F[l][f]` in a pair).
    fn trs(c: Self, dst: Self, l: u8, g: u8) -> Self;
    /// The translucent shade run: `dst` with factor `g = F[l][f]`.
    fn trs_shade(self, g: u8) -> Self;
}

/// `AFF_SPRTRS`'s factor table, built by `SprDll_Init` (spr_p6 @ 0x100018a0):
/// `F[a][b] = a × (b & 31) / 31`, integer.
pub fn trs_factor(a: u8, b: u8) -> u8 {
    ((u32::from(a) * u32::from(b & 31)) / 31) as u8
}

/// One byte lane of the 32-bit translucent blitters (spr_p6 @ 0x10010c5b):
/// `((s − d) × g + (d << 5)) >> 5` in 16-bit lanes (`psubw`, `pmullw` low
/// word, `paddw`, `psrlw 5`), saturated by `packuswb`.
fn mmx_trs(c: u32, dst: u32, g: u16) -> u32 {
    let mut out = 0u32;
    for lane in 0..4 {
        let s = ((c >> (8 * lane)) & 0xff) as u16;
        let d = ((dst >> (8 * lane)) & 0xff) as u16;
        let v = s.wrapping_sub(d).wrapping_mul(g).wrapping_add(d << 5) >> 5;
        out |= u32::from((v as i16).clamp(0, 255) as u8) << (8 * lane);
    }
    out
}

impl Pixel for u16 {
    const DEPTH: Depth = Depth::Rgb565;

    fn from_entry(e: u32) -> u16 {
        e as u16
    }

    fn from_direct(d: &[u8], at: usize) -> Option<u16> {
        Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
    }

    /// `HI[f][c >> 8] + LO[f][c & 0xff]` (tables @ 0x1002d600 and
    /// @ 0x100255e0, the same bytes as *Atlantis II*'s).
    fn shade(self, f: u8) -> u16 {
        shade565(self, f)
    }

    fn add_shaded(c: u16, dst: u16, f: u8) -> u16 {
        c.wrapping_add(shade565(dst, f))
    }

    /// `(c & 0xf7de) >> 1` (mask @ 0x1001b58c).
    fn half(self) -> u16 {
        half565(self)
    }

    /// `shade(c, l ^ 31) + shade(dst, g)` (spr_p6 @ 0x1000aad9).
    fn trs(c: u16, dst: u16, l: u8, g: u8) -> u16 {
        shade565(c, l ^ 31).wrapping_add(shade565(dst, g))
    }

    fn trs_shade(self, g: u8) -> u16 {
        shade565(self, g)
    }
}

/// The MMX shade of the 32-bit blitters (@ 0x1000ee80..0x1000eeaf): each
/// byte of `dst`, x included, times `f ^ 0x1f` (`pmullw` of 16-bit lanes),
/// shifted right by 5, plus the byte of `c` (`paddw`), saturated to 0..255
/// (`packuswb`).
fn mmx_shade(c: u32, dst: u32, f: u8) -> u32 {
    let k = u16::from(f ^ 0x1f);
    let mut out = 0u32;
    for lane in 0..4 {
        let d = ((dst >> (8 * lane)) & 0xff) as u16;
        let b = ((c >> (8 * lane)) & 0xff) as u16;
        let s = b.wrapping_add(d.wrapping_mul(k) >> 5) as i16;
        out |= u32::from(s.clamp(0, 255) as u8) << (8 * lane);
    }
    out
}

impl Pixel for u32 {
    const DEPTH: Depth = Depth::Xrgb8888;

    fn from_entry(e: u32) -> u32 {
        e
    }

    fn from_direct(d: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
    }

    fn shade(self, f: u8) -> u32 {
        mmx_shade(0, self, f)
    }

    fn add_shaded(c: u32, dst: u32, f: u8) -> u32 {
        mmx_shade(c, dst, f)
    }

    /// `(c & 0x00fefefe) >> 1`: the 32-bit mask @ 0x1001b58c clears x.
    fn half(self) -> u32 {
        (self & 0x00fe_fefe) >> 1
    }

    fn shade_p5_right_clip(self, f: u8) -> u32 {
        mmx_shade(0, self, f) & 0xffff
    }

    fn trs(c: u32, dst: u32, _l: u8, g: u8) -> u32 {
        mmx_trs(c, dst, u16::from(g))
    }

    /// `(d × k) >> 5` per byte, k = `g ^ 0x1f`.
    fn trs_shade(self, g: u8) -> u32 {
        let k = u32::from(g ^ 0x1f);
        let mut out = 0;
        for lane in 0..4 {
            let d = (self >> (8 * lane)) & 0xff;
            out |= ((d * k) >> 5).min(255) << (8 * lane);
        }
        out
    }
}

/// A drawing target: pixels with a pitch and the clip of the game's
/// descriptor (six dwords: pixels, pitch, `ymin`, `ymax`, `xmin`, `xmax`;
/// the upper bounds excluded by the blitters).
pub struct Target<'a, P> {
    pub pixels: &'a mut [P],
    pub pitch: usize,
    pub ymin: i32,
    pub ymax: i32,
    pub xmin: i32,
    pub xmax: i32,
}

impl<'a, P> Target<'a, P> {
    /// A whole buffer with the clip maxima at width − 1 and height − 1, as
    /// *Atlantis II*'s descriptor has them: its last row and column are
    /// never drawn.
    pub fn screen(pixels: &'a mut [P], width: usize, height: usize) -> Target<'a, P> {
        Target {
            pixels,
            pitch: width,
            ymin: 0,
            ymax: height as i32 - 1,
            xmin: 0,
            xmax: width as i32 - 1,
        }
    }
}

/// One sprite's header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteInfo {
    pub w: u16,
    pub h: u16,
    /// The hot spot, subtracted from the draw position.
    pub xoff: i16,
    pub yoff: i16,
    /// The default position (flags & 0x800).
    pub default_pos: Option<(u16, u16)>,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn put_u16(b: &mut [u8], at: usize, v: u16) -> Option<()> {
    b.get_mut(at..at + 2)?.copy_from_slice(&v.to_le_bytes());
    Some(())
}

/// `CONV_SPR32_64` (spr_p5 @ 0x10001e34), which the loaders call on a bank
/// with flags & 0x80 for a 565 display: every direct colour of every row,
/// last sprite first, from 555 to 565 (`c & 0x1f | (c & !0x1f) << 1`).
/// Only for flags & 0x20; the count is read as a u16 at +4 and the offsets
/// from +8 (no colour table). It walks the rows as the blitters do: the
/// pixels of R and P runs are converted, a P run's factors and an S run's
/// factor byte (none with flags & 0x200: the stack slot read at
/// @ 0x10001f73 is the flag `(flags & 0x200) == 0` set @ 0x10001e67) are
/// skipped. Ends silently on data that run out.
pub fn conv_spr32_64(buf: &mut [u8]) {
    let Some(flags) = u32_at(buf, 0) else { return };
    if flags & 0x20 == 0 {
        return;
    }
    let Some(count) = u16_at(buf, 4) else { return };
    let skip_pos = if flags & 0x800 != 0 { 4 } else { 0 };
    let skip_s = usize::from(flags & 0x200 == 0);
    let shade_runs = flags & 0x10 != 0;
    for index in (0..usize::from(count)).rev() {
        let Some(off) = u32_at(buf, 8 + 4 * index) else {
            return;
        };
        if off == 0 {
            continue;
        }
        let sprite = 8 + off as usize + skip_pos;
        let Some(h) = u16_at(buf, sprite + 2) else {
            return;
        };
        let mut p = sprite + 8 + 4 * usize::from(h);
        for _ in 0..h {
            let Some(rel) = u32_at(buf, p) else { return };
            let mut c = p + rel as usize;
            p += 4;
            if shade_runs {
                let mut prev = Run::Start;
                loop {
                    let Some(&b) = buf.get(c) else { return };
                    c += 1;
                    let (run, n) = next_run(prev, b);
                    prev = run;
                    match run {
                        Run::End | Run::Start => break,
                        Run::Skip => {}
                        Run::Pal => {
                            if convert(buf, &mut p, n).is_none() {
                                return;
                            }
                        }
                        Run::Pair => {
                            c += n;
                            if convert(buf, &mut p, n).is_none() {
                                return;
                            }
                        }
                        Run::Shade => c += skip_s,
                    }
                }
            } else {
                let mut prev = Simple::Start;
                loop {
                    let Some(&b) = buf.get(c) else { return };
                    c += 1;
                    let (run, n) = next_simple(prev, b);
                    prev = run;
                    match run {
                        Simple::End | Simple::Start => break,
                        Simple::Skip => {}
                        Simple::Copy => {
                            if convert(buf, &mut p, n).is_none() {
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// `n` direct colours at `p` from 555 to 565 (@ 0x10001ea4).
fn convert(buf: &mut [u8], p: &mut usize, n: usize) -> Option<()> {
    for _ in 0..n {
        let v = u16_at(buf, *p)?;
        put_u16(buf, *p, (v & 0x1f) | ((v & !0x1f) << 1))?;
        *p += 2;
    }
    Some(())
}

/// The runs of the 0xa0 rows: skip, copy, end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Simple {
    Start,
    Skip,
    Copy,
    End,
}

/// The next run of a 0xa0 row (spr_p5 @ 0x100092e0..0x10009334): at the
/// start of a row and after a copy run, bit 0 set → (copy after a copy, skip
/// at the start) `b >> 1`, else bits 1..0 = 10 → (skip after a copy, copy at
/// the start) `b >> 2`, else the end; after a skip run, bit 0 clear → skip,
/// set → copy, `b >> 1`, and no end.
fn next_simple(prev: Simple, b: u8) -> (Simple, usize) {
    let n1 = usize::from(b >> 1) + 1;
    let n2 = usize::from(b >> 2) + 1;
    match prev {
        Simple::Start | Simple::End => {
            if b & 1 == 1 {
                (Simple::Skip, n1)
            } else if b & 2 == 2 {
                (Simple::Copy, n2)
            } else {
                (Simple::End, 0)
            }
        }
        Simple::Copy => {
            if b & 1 == 1 {
                (Simple::Copy, n1)
            } else if b & 2 == 2 {
                (Simple::Skip, n2)
            } else {
                (Simple::End, 0)
            }
        }
        Simple::Skip => {
            if b & 1 == 1 {
                (Simple::Copy, n1)
            } else {
                (Simple::Skip, n1)
            }
        }
    }
}

/// What a family's rows hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rows {
    /// Palette rows (one stream); S runs with a factor byte, or at 50 %.
    Palette { half_shade: bool },
    /// Direct colours and a control stream (0xb0, 0x130).
    Direct,
    /// Direct colours, skip and copy runs only (0xa0).
    DirectCopy,
}

/// What `AFF_SPR` / `AFF_SPW` do with a family on a display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blitter {
    Draw(Rows),
    /// The no-op entry of the jump table (@ 0x10003063): nothing drawn.
    Nothing,
    /// A blitter no shipped file uses, not transcribed.
    Unsupported,
}

fn blitter(flags: u32, depth: Depth) -> Blitter {
    use Blitter::*;
    match (depth, flags & 0x3fe) {
        (Depth::Rgb565, 0x74 | 0x78) | (Depth::Xrgb8888, 0x78) => {
            Draw(Rows::Palette { half_shade: false })
        }
        (Depth::Rgb565, 0x274 | 0x278) | (Depth::Xrgb8888, 0x278) => {
            Draw(Rows::Palette { half_shade: true })
        }
        (Depth::Rgb565, 0xb0) | (Depth::Xrgb8888, 0x130) => Draw(Rows::Direct),
        (Depth::Rgb565, 0xa0) => Draw(Rows::DirectCopy),
        (Depth::Rgb565, 0x24 | 0x28 | 0x280) | (Depth::Xrgb8888, 0x28 | 0x120 | 0x300) => {
            Unsupported
        }
        _ => Nothing,
    }
}

/// A sprite bank or a place-sprite file, prepared for one display depth.
#[derive(Clone, Debug)]
pub struct Bank {
    /// The file after its magic, after `CONV_SPR32_64`.
    data: Vec<u8>,
    pub flags: u32,
    pub depth: Depth,
    /// The colour table as the blitters read it.
    pal: Vec<u32>,
    offsets_at: usize,
    count: u32,
}

impl Bank {
    /// Reads a whole file for a display of `depth`, as the loaders
    /// (@ 0x0069d220, @ 0x0069d4b0) prepare it.
    pub fn parse(file: &[u8], depth: Depth) -> Result<Bank, SprError> {
        let magic = file.get(..8).ok_or(SprError::Truncated)?;
        if !matches!(magic, b"SPRCRYO " | b"SPRCRYO1" | b"SPWCRYO ") {
            return Err(SprError::NotSpr);
        }
        let mut data = file[8..].to_vec();
        let flags = u32_at(&data, 0).ok_or(SprError::Truncated)?;
        if flags & 0x80 != 0 && depth == Depth::Rgb565 {
            conv_spr32_64(&mut data);
        }
        let mut at = 4;
        let mut pal = vec![0u32; 256];
        if flags & 4 != 0 {
            // SPR_CONVPAL32 (@ 0x100013a0): RGB555 to the display; only a
            // 16-bit display has a blitter for this family.
            for (i, p) in pal.iter_mut().enumerate() {
                *p = u32::from(to565(u16_at(&data, at + 2 * i).ok_or(SprError::Truncated)?));
            }
            at += 0x200;
        }
        if flags & 8 != 0 {
            for (i, p) in pal.iter_mut().enumerate() {
                let e = data
                    .get(at + 4 * i..at + 4 * i + 4)
                    .ok_or(SprError::Truncated)?;
                let (b, g, r) = (u32::from(e[0]), u32::from(e[1]), u32::from(e[2]));
                *p = match depth {
                    // SPR_CONVPAL16 (@ 0x10001dbe) for RGB565: each channel
                    // shifted right by its loss and left into place, the
                    // colour in both halves. Atlantis II's library dropped
                    // the green's low bit; this one keeps all six.
                    Depth::Rgb565 => {
                        let c = (r >> 3) << 11 | (g >> 2) << 5 | (b >> 3);
                        c | c << 16
                    }
                    Depth::Xrgb8888 => u32::from_le_bytes([e[0], e[1], e[2], e[3]]),
                };
            }
            at += 0x400;
        }
        let count = u32_at(&data, at).ok_or(SprError::Truncated)?;
        Ok(Bank {
            data,
            flags,
            depth,
            pal,
            offsets_at: at + 4,
            count,
        })
    }

    /// Whether this is an `SPWCRYO` file (flags & 1).
    pub fn is_spw(&self) -> bool {
        self.flags & 1 != 0
    }

    /// The number of slots, or of frames of a place-sprite file
    /// (`GET_NBSPR`).
    pub fn len(&self) -> usize {
        self.count as usize
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Where sprite `index` starts (its size field), none for an empty
    /// slot.
    fn sprite_at(&self, index: usize) -> Option<usize> {
        if index >= self.len() {
            return None;
        }
        let off = u32_at(&self.data, self.offsets_at + 4 * index)? as usize;
        if off == 0 {
            return None;
        }
        let at = self.offsets_at + off;
        Some(if self.flags & 0x800 != 0 { at + 4 } else { at })
    }

    /// Where face `face` of frame `frame` starts (`AFF_SPW` @ 0x1000242e):
    /// none for an empty face.
    fn face_at(&self, frame: usize, face: usize) -> Option<usize> {
        if frame >= self.len() || face >= 6 {
            return None;
        }
        let fo = self.offsets_at + u32_at(&self.data, self.offsets_at + 4 * frame)? as usize;
        let off = u32_at(&self.data, fo + 4 * face)? as usize;
        (off != 0).then_some(fo + off)
    }

    fn info_at(&self, at: usize) -> Option<SpriteInfo> {
        let d = &self.data;
        Some(SpriteInfo {
            w: u16_at(d, at)?,
            h: u16_at(d, at + 2)?,
            xoff: u16_at(d, at + 4)? as i16,
            yoff: u16_at(d, at + 6)? as i16,
            default_pos: if self.flags & 0x800 != 0 && !self.is_spw() {
                Some((u16_at(d, at - 4)?, u16_at(d, at - 2)?))
            } else {
                None
            },
        })
    }

    /// A sprite's header; none for an empty slot.
    pub fn info(&self, index: usize) -> Option<SpriteInfo> {
        if self.is_spw() {
            return None;
        }
        self.info_at(self.sprite_at(index)?)
    }

    /// A face's header; none for an empty face.
    pub fn face_info(&self, frame: usize, face: usize) -> Option<SpriteInfo> {
        if !self.is_spw() {
            return None;
        }
        self.info_at(self.face_at(frame, face)?)
    }

    /// `AFF_SPR` (spr_p5 @ 0x100022b0) of sprite `index` with its hot spot
    /// at (x, y); `None` for x = the default position (0x80000000). The
    /// target's pixel type must be the bank's depth. Returns false when
    /// nothing was drawn because this port lacks the blitter (or the call
    /// does not apply: no such sprite, a place-sprite file, another
    /// depth); a family without a blitter in the original draws nothing
    /// and returns true.
    pub fn draw<P: Pixel>(&self, t: &mut Target<P>, x: Option<i32>, y: i32, index: usize) -> bool {
        self.draw_with(t, x, y, index, SprDll::default())
    }

    /// [`Bank::draw`] with the library `dll`.
    pub fn draw_with<P: Pixel>(
        &self,
        t: &mut Target<P>,
        x: Option<i32>,
        y: i32,
        index: usize,
        dll: SprDll,
    ) -> bool {
        if P::DEPTH != self.depth || self.is_spw() {
            return false;
        }
        let Some(at) = self.sprite_at(index) else {
            return false;
        };
        let Some(info) = self.info_at(at) else {
            return false;
        };
        let (x, y) = match (x, info.default_pos) {
            (None, Some((dx, dy))) => (i32::from(dx), i32::from(dy)),
            (None, None) => (i32::MIN, y),
            (Some(x), _) => (x, y),
        };
        self.blit(t, at, info, x, y, dll)
    }

    /// `AFF_SPRTRS` (spr_p6 @ 0x10001210 → 0x100025db, spr_p5 @ 0x10001190):
    /// sprite `index` drawn translucent at `level` (the byte `& 0x1f`; the
    /// game passes `ftol(alpha × 31)`). Families 0x78 and 0x278 (and 0x274
    /// on a 16-bit display); any other draws nothing. In 32 bits this is
    /// `spr_p6.dll`'s blitter without its left-clip path (which shades
    /// pixels the unclipped path skips; not transcribed, the port draws in
    /// 16 bits); `spr_p5.dll`'s reads its factors from the 16-bit table,
    /// left zero on a 32-bit display (not transcribed: the game picks
    /// `spr_p6.dll` from CPU class 4). False as [`Bank::draw`].
    pub fn draw_trs<P: Pixel>(
        &self,
        t: &mut Target<P>,
        x: Option<i32>,
        y: i32,
        index: usize,
        level: u8,
    ) -> bool {
        if P::DEPTH != self.depth || self.is_spw() {
            return false;
        }
        let Some(at) = self.sprite_at(index) else {
            return false;
        };
        let Some(info) = self.info_at(at) else {
            return false;
        };
        let (x, y) = match (x, info.default_pos) {
            (None, Some((dx, dy))) => (i32::from(dx), i32::from(dy)),
            (None, None) => (i32::MIN, y),
            (Some(x), _) => (x, y),
        };
        let half_shade = match (self.depth, self.flags & 0x3fe) {
            (_, 0x78) | (Depth::Rgb565, 0x74) => false,
            (_, 0x278) | (Depth::Rgb565, 0x274) => true,
            _ => return true,
        };
        let level = level & 0x1f;
        let x0 = i32::from((x as i16).wrapping_sub(info.xoff));
        let y0 = i32::from((y as i16).wrapping_sub(info.yoff));
        let (w, h) = (i32::from(info.w), i32::from(info.h));
        let d = &self.data;
        let table = at + 8;
        let first = table + 4 * usize::from(info.h);
        for row in 0..h {
            let sy = y0 + row;
            if sy < t.ymin || sy >= t.ymax {
                continue;
            }
            let Some(off) = u32_at(d, table + 4 * row as usize) else {
                break;
            };
            let line = sy as usize * t.pitch;
            if self
                .trs_row(t, line, first + off as usize, x0, w, half_shade, level)
                .is_none()
            {
                break;
            }
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn trs_row<P: Pixel>(
        &self,
        t: &mut Target<P>,
        line: usize,
        mut p: usize,
        x0: i32,
        w: i32,
        half_shade: bool,
        level: u8,
    ) -> Option<()> {
        let d = &self.data;
        let (xmin, xmax) = (t.xmin, t.xmax);
        let inside = |sx: i32| sx >= xmin && sx < xmax;
        let mut sx = x0;
        let mut prev = Run::Start;
        while sx < x0 + w {
            let b = *d.get(p)?;
            p += 1;
            let (run, n) = next_run(prev, b);
            prev = run;
            match run {
                Run::End | Run::Start => break,
                Run::Skip => sx += n as i32,
                Run::Pal => {
                    for _ in 0..n {
                        let i = *d.get(p)?;
                        p += 1;
                        if inside(sx) {
                            let px = &mut t.pixels[line + sx as usize];
                            *px =
                                P::trs(P::from_entry(self.pal[usize::from(i)]), *px, level, level);
                        }
                        sx += 1;
                    }
                }
                Run::Pair => {
                    for _ in 0..n {
                        let (f, i) = (*d.get(p)?, *d.get(p + 1)?);
                        p += 2;
                        if inside(sx) {
                            let px = &mut t.pixels[line + sx as usize];
                            *px = P::trs(
                                P::from_entry(self.pal[usize::from(i)]),
                                *px,
                                level,
                                trs_factor(level, f),
                            );
                        }
                        sx += 1;
                    }
                }
                Run::Shade => {
                    let f = if half_shade {
                        15
                    } else {
                        p += 1;
                        *d.get(p - 1)?
                    };
                    for _ in 0..n {
                        if inside(sx) {
                            let px = &mut t.pixels[line + sx as usize];
                            *px = px.trs_shade(trs_factor(level, f));
                        }
                        sx += 1;
                    }
                }
            }
        }
        Some(())
    }

    /// `SPR_COLLISION` (spr_p6 @ 0x10001240 → @ 0x100029ae; the 0x278
    /// rows @ 0x100031f2): whether sprite `index`, drawn as
    /// [`Bank::draw`] would at (x, y), covers (px, py). The pixel's row is
    /// walked run by run with the distance d = px − run start: the run
    /// with d ≤ its length decides (so a run's end pixel belongs to it):
    /// a colour or pair run hits, a skip or shade run misses. Palette
    /// families only.
    pub fn opaque_at(&self, index: usize, x: Option<i32>, y: i32, px: i32, py: i32) -> bool {
        if self.is_spw() {
            return false;
        }
        let Some(at) = self.sprite_at(index) else {
            return false;
        };
        let Some(info) = self.info_at(at) else {
            return false;
        };
        let (x, y) = match (x, info.default_pos) {
            (None, Some((dx, dy))) => (i32::from(dx), i32::from(dy)),
            (None, None) => return false,
            (Some(x), _) => (x, y),
        };
        if !matches!(
            blitter(self.flags, self.depth),
            Blitter::Draw(Rows::Palette { .. })
        ) {
            return false;
        }
        let half_shade = self.flags & 0x200 != 0;
        let x0 = i32::from((x as i16).wrapping_sub(info.xoff));
        let y0 = i32::from((y as i16).wrapping_sub(info.yoff));
        let row = py - y0;
        if row < 0 || row >= i32::from(info.h) || px < x0 || px >= x0 + i32::from(info.w) {
            return false;
        }
        let d = &self.data;
        let table = at + 8;
        let first = table + 4 * usize::from(info.h);
        let Some(off) = u32_at(d, table + 4 * row as usize) else {
            return false;
        };
        let mut p = first + off as usize;
        let mut sx = x0;
        let mut prev = Run::Start;
        loop {
            let Some(&b) = d.get(p) else { return false };
            p += 1;
            let (run, n) = next_run(prev, b);
            prev = run;
            let n = n as i32;
            let inside = px - sx <= n;
            match run {
                Run::End | Run::Start => return false,
                Run::Skip => {}
                Run::Pal => {
                    if inside {
                        return true;
                    }
                    p += n as usize;
                }
                Run::Pair => {
                    if inside {
                        return true;
                    }
                    p += 2 * n as usize;
                }
                Run::Shade => {
                    if !half_shade {
                        p += 1;
                    }
                }
            }
            if inside {
                return false;
            }
            sx += n;
        }
    }

    /// `AFF_SPW` (spr_p5 @ 0x100023f0): face `face` of frame `frame` of a
    /// place-sprite file, its hot spot at (0, 0). False as [`Bank::draw`].
    pub fn draw_face<P: Pixel>(&self, t: &mut Target<P>, frame: usize, face: usize) -> bool {
        self.draw_face_with(t, frame, face, SprDll::default())
    }

    /// [`Bank::draw_face`] with the library `dll`.
    pub fn draw_face_with<P: Pixel>(
        &self,
        t: &mut Target<P>,
        frame: usize,
        face: usize,
        dll: SprDll,
    ) -> bool {
        if P::DEPTH != self.depth || !self.is_spw() {
            return false;
        }
        let Some(at) = self.face_at(frame, face) else {
            return false;
        };
        let Some(info) = self.info_at(at) else {
            return false;
        };
        self.blit(t, at, info, 0, 0, dll)
    }

    fn blit<P: Pixel>(
        &self,
        t: &mut Target<P>,
        at: usize,
        info: SpriteInfo,
        x: i32,
        y: i32,
        dll: SprDll,
    ) -> bool {
        let rows = match blitter(self.flags, self.depth) {
            Blitter::Draw(rows) => rows,
            Blitter::Nothing => return true,
            Blitter::Unsupported => return false,
        };
        // The blitters compute the corner in 16 bits (`mov bx, [ebp+0x10];
        // sub bx, [esi+4]; movsx`).
        let x0 = i32::from((x as i16).wrapping_sub(info.xoff));
        let y0 = i32::from((y as i16).wrapping_sub(info.yoff));
        let (w, h) = (i32::from(info.w), i32::from(info.h));
        // The right-clip path (`[esp+0x51] & 2` without & 1): the sprite
        // crosses `xmax` and not `xmin`.
        let p5_right = dll == SprDll::P5 && x0 >= t.xmin && x0 + w > t.xmax;
        let d = &self.data;
        let table = at + 8;
        let first = table + 4 * usize::from(info.h);
        for row in 0..h {
            let sy = y0 + row;
            if sy < t.ymin || sy >= t.ymax {
                continue;
            }
            let Some(off) = u32_at(d, table + 4 * row as usize) else {
                break;
            };
            let p = first + off as usize;
            let line = sy as usize * t.pitch;
            let (xmin, xmax) = (t.xmin, t.xmax);
            let inside = move |sx: i32| sx >= xmin && sx < xmax;
            let ok = match rows {
                Rows::Palette { half_shade } => {
                    self.palette_row(t, line, &inside, p, x0, w, half_shade)
                }
                Rows::Direct => {
                    let Some(rel) = u32_at(d, p) else { break };
                    let c = p + rel as usize;
                    direct_row(d, t, line, &inside, p + 4, c, x0, w, p5_right)
                }
                Rows::DirectCopy => {
                    let Some(rel) = u32_at(d, p) else { break };
                    copy_row(d, t, line, &inside, p + 4, p + rel as usize, x0, w)
                }
            };
            if ok.is_none() {
                break;
            }
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn palette_row<P: Pixel>(
        &self,
        t: &mut Target<P>,
        line: usize,
        inside: &dyn Fn(i32) -> bool,
        mut p: usize,
        x0: i32,
        w: i32,
        half_shade: bool,
    ) -> Option<()> {
        let d = &self.data;
        let mut sx = x0;
        let mut prev = Run::Start;
        while sx < x0 + w {
            let b = *d.get(p)?;
            p += 1;
            let (run, n) = next_run(prev, b);
            prev = run;
            match run {
                Run::End | Run::Start => break,
                Run::Skip => sx += n as i32,
                Run::Pal => {
                    for _ in 0..n {
                        let i = *d.get(p)?;
                        p += 1;
                        if inside(sx) {
                            t.pixels[line + sx as usize] = P::from_entry(self.pal[usize::from(i)]);
                        }
                        sx += 1;
                    }
                }
                Run::Pair => {
                    for _ in 0..n {
                        let (f, i) = (*d.get(p)?, *d.get(p + 1)?);
                        p += 2;
                        if inside(sx) {
                            let px = &mut t.pixels[line + sx as usize];
                            *px = P::add_shaded(P::from_entry(self.pal[usize::from(i)]), *px, f);
                        }
                        sx += 1;
                    }
                }
                Run::Shade => {
                    let f = if half_shade {
                        None
                    } else {
                        p += 1;
                        Some(*d.get(p - 1)?)
                    };
                    for _ in 0..n {
                        if inside(sx) {
                            let px = &mut t.pixels[line + sx as usize];
                            *px = match f {
                                Some(f) => px.shade(f),
                                None => px.half(),
                            };
                        }
                        sx += 1;
                    }
                }
            }
        }
        Some(())
    }
}

/// The size of a direct colour of depth `P`.
fn direct_size<P: Pixel>() -> usize {
    match P::DEPTH {
        Depth::Rgb565 => 2,
        Depth::Xrgb8888 => 4,
    }
}

/// A 0xb0 / 0x130 row (16 bits @ 0x10008360..0x10008463, 32 bits
/// @ 0x100123e0..0x100124f6): pixels at `p`, control bytes at `c`.
#[allow(clippy::too_many_arguments)]
fn direct_row<P: Pixel>(
    d: &[u8],
    t: &mut Target<P>,
    line: usize,
    inside: &dyn Fn(i32) -> bool,
    mut p: usize,
    mut c: usize,
    x0: i32,
    w: i32,
    p5_right: bool,
) -> Option<()> {
    let size = direct_size::<P>();
    let mut sx = x0;
    let mut prev = Run::Start;
    while sx < x0 + w {
        let b = *d.get(c)?;
        c += 1;
        let (run, n) = next_run(prev, b);
        prev = run;
        match run {
            Run::End | Run::Start => break,
            Run::Skip => sx += n as i32,
            Run::Pal => {
                for _ in 0..n {
                    let v = P::from_direct(d, p)?;
                    p += size;
                    if inside(sx) {
                        t.pixels[line + sx as usize] = v;
                    }
                    sx += 1;
                }
            }
            Run::Pair => {
                for _ in 0..n {
                    let f = *d.get(c)?;
                    c += 1;
                    let v = P::from_direct(d, p)?;
                    p += size;
                    if inside(sx) {
                        let px = &mut t.pixels[line + sx as usize];
                        *px = P::add_shaded(v, *px, f);
                    }
                    sx += 1;
                }
            }
            Run::Shade => {
                let f = *d.get(c)?;
                c += 1;
                for _ in 0..n {
                    if inside(sx) {
                        let px = &mut t.pixels[line + sx as usize];
                        *px = if p5_right {
                            px.shade_p5_right_clip(f)
                        } else {
                            px.shade(f)
                        };
                    }
                    sx += 1;
                }
            }
        }
    }
    Some(())
}

/// A 0xa0 row (@ 0x100092e0..0x10009334): pixels at `p`, control bytes at
/// `c`, skip and copy runs.
#[allow(clippy::too_many_arguments)]
fn copy_row<P: Pixel>(
    d: &[u8],
    t: &mut Target<P>,
    line: usize,
    inside: &dyn Fn(i32) -> bool,
    mut p: usize,
    mut c: usize,
    x0: i32,
    w: i32,
) -> Option<()> {
    let size = direct_size::<P>();
    let mut sx = x0;
    let mut prev = Simple::Start;
    while sx < x0 + w {
        let b = *d.get(c)?;
        c += 1;
        let (run, n) = next_simple(prev, b);
        prev = run;
        match run {
            Simple::End | Simple::Start => break,
            Simple::Skip => sx += n as i32,
            Simple::Copy => {
                for _ in 0..n {
                    let v = P::from_direct(d, p)?;
                    p += size;
                    if inside(sx) {
                        t.pixels[line + sx as usize] = v;
                    }
                    sx += 1;
                }
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-sprite bank of `flags` with an empty colour table of `pal`
    /// bytes, sprite `w` x 1 with `row` as its only row.
    fn bank(flags: u32, pal: usize, w: u16, row: &[u8]) -> Vec<u8> {
        let mut d = b"SPRCRYO ".to_vec();
        d.extend(flags.to_le_bytes());
        d.extend(vec![0u8; pal]);
        d.extend(1u32.to_le_bytes());
        d.extend(4u32.to_le_bytes());
        for v in [w, 1, 0, 0] {
            d.extend(v.to_le_bytes());
        }
        d.extend(0u32.to_le_bytes());
        d.extend(row);
        d
    }

    #[test]
    fn half_shade_family_draws_without_a_factor_byte() {
        // 0x278: B, G, R, x table; a row "R 1 (index 0), S 2, E".
        let mut d = bank(0x278, 0x400, 3, &[0x04, 0, 0x0a, 0x02]);
        // Entry 0 = white.
        d[12..16].copy_from_slice(&[0xff, 0xff, 0xff, 0]);
        let b = Bank::parse(&d, Depth::Rgb565).unwrap();
        let mut px = vec![0x1234u16; 8 * 3];
        let mut t = Target::screen(&mut px, 8, 3);
        assert!(b.draw(&mut t, Some(1), 1, 0));
        assert_eq!(&px[9..13], &[0xffff, 0x090a, 0x090a, 0x1234]);
        let b = Bank::parse(&d, Depth::Xrgb8888).unwrap();
        let mut px = vec![0xffff_ffffu32; 8 * 3];
        let mut t = Target::screen(&mut px, 8, 3);
        assert!(b.draw(&mut t, Some(1), 1, 0));
        assert_eq!(&px[9..12], &[0x00ff_ffff, 0x007f_7f7f, 0x007f_7f7f]);
    }

    #[test]
    fn mmx_shade_saturates() {
        assert_eq!(mmx_shade(0, 0xffff_ffff, 0), 0xf7f7_f7f7);
        assert_eq!(mmx_shade(0xff, 0xff, 0), 0xff);
        assert_eq!(mmx_shade(0, 0x40, 31), 0);
    }

    #[test]
    fn simple_grammar() {
        assert_eq!(next_simple(Simple::Start, 0b11), (Simple::Skip, 2));
        assert_eq!(next_simple(Simple::Start, 0b110), (Simple::Copy, 2));
        assert_eq!(next_simple(Simple::Start, 0), (Simple::End, 0));
        assert_eq!(next_simple(Simple::Skip, 0b10), (Simple::Skip, 2));
        assert_eq!(next_simple(Simple::Copy, 0b100), (Simple::End, 0));
    }

    #[test]
    fn conversion_to_565() {
        // A 0xa0 bank: count 1 (u16), one sprite 2 x 1, row: u32 offset of
        // the control bytes, two pixels, control "copy 2", end.
        let mut d = 0xa0u32.to_le_bytes().to_vec();
        d.extend([1u8, 0, 0, 0]);
        d.extend(4u32.to_le_bytes());
        for v in [2u16, 1, 0, 0] {
            d.extend(v.to_le_bytes());
        }
        d.extend(0u32.to_le_bytes());
        d.extend(8u32.to_le_bytes());
        d.extend(0x7fffu16.to_le_bytes());
        d.extend(0x001fu16.to_le_bytes());
        d.extend([0b110, 0]);
        conv_spr32_64(&mut d);
        assert_eq!(u16_at(&d, 28), Some(0xffdf));
        assert_eq!(u16_at(&d, 30), Some(0x001f));
    }

    #[test]
    fn rejections() {
        assert_eq!(
            Bank::parse(b"SPRCRYOX0000", Depth::Rgb565).unwrap_err(),
            SprError::NotSpr
        );
        assert!(Bank::parse(b"SPRCRYO", Depth::Rgb565).is_err());
    }
}

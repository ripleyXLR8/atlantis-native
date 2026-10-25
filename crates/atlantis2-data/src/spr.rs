//! `SPRCRYO` / `SPRCRYO1` sprite banks (`spr\*.spr`, `spr\menu\*.spr`,
//! `datas_a2\spf\*.spr`): the cursors, the bag, the menus' buttons.
//!
//! The executable loads them (@ 0x0059d9b2) and `spr_p5.dll` draws them;
//! the game calls three of its modes through `AtlSpr_Aff` (@ 0x0047bc40):
//! 0 `AFF_SPR`, 1 `AFF_SPRTRS50`, 2 `AFF_SPRECL` (5, 0, 0). `AFF_SPR` and
//! `AFF_SPRTRS50` are here, for an RGB565 screen (the cursors, the bag, the
//! subject menu, whose subjects already asked are drawn at 50 %).
//!
//! Layout after the 8-byte magic: u32 flags; the RGB555 colour table
//! (0x200 bytes) when flags & 4, or a table of 256 (B, G, R, x) bytes
//! (0x400 bytes) when flags & 8 (`Ponpirog.SPR`); u32 count; u32 offsets from the start of
//! that array (0 = empty slot); per sprite: a default position (u16 x, y)
//! when flags & 0x800, u16 w, h, s16 hot spot x, y, u32\[h\] row offsets
//! from the end of that array, the rows.

use std::fmt;

/// Why a file is not a sprite bank.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SprError {
    Truncated,
    /// Not `SPRCRYO ` or `SPRCRYO1` (error 4 of the loader).
    NotSpr,
}

impl fmt::Display for SprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SprError::Truncated => write!(f, "truncated sprite bank"),
            SprError::NotSpr => write!(f, "not an SPRCRYO sprite bank"),
        }
    }
}

impl std::error::Error for SprError {}

/// A drawing target: 16-bit pixels with a pitch and the clip of the
/// game's descriptor (@ 0x00673f70: rows `ymin..ymax`, columns
/// `xmin..xmax`, the upper bounds excluded by the blitters).
pub struct Target<'a> {
    pub pixels: &'a mut [u16],
    pub pitch: usize,
    pub ymin: i32,
    pub ymax: i32,
    pub xmin: i32,
    pub xmax: i32,
}

impl<'a> Target<'a> {
    /// A whole screen as `Spr_Init` (@ 0x0047b980) describes it: ymax =
    /// height − 1, xmax = width − 1, so its last row and column are never
    /// drawn.
    pub fn screen(pixels: &'a mut [u16], width: usize, height: usize) -> Target<'a> {
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

/// A sprite bank, its colours converted for an RGB565 screen.
#[derive(Clone, Debug)]
pub struct SprBank {
    data: Vec<u8>,
    pub flags: u32,
    pal: Vec<u16>,
    offsets_at: usize,
    count: u32,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// `SPR_CONVPAL32` (spr_p5.dll @ 0x10002428) for RGB565: B, G, R of an
/// RGB555 entry shifted to `R << 11 | G << 6 | B`.
fn to565(v: u16) -> u16 {
    let (b, g, r) = (v & 31, (v >> 5) & 31, (v >> 10) & 31);
    (r << 11) | (g << 6) | b
}

/// The shade of a pixel by a factor f (0..31), `HI[f][c >> 8] + LO[f][c &
/// 0xff]` of `SprDll_Init` (spr_p5.dll @ 0x100018e0) for RGB565: about
/// `c × (31 − f) / 32` per channel.
fn shade(c: u16, f: u8) -> u16 {
    let k = 31 - u32::from(f & 31);
    let hb = u32::from(c >> 8);
    let lb = u32::from(c & 0xff);
    let g1 = (((hb & 7) << 3) * k) >> 4;
    let g = ((g1 >> 1) + (g1 & 1)).min(0x3f);
    let hi = ((((hb >> 3) * k) >> 5) << 11) | (g << 5);
    let lo = ((((lb >> 5) * k) >> 5) << 5) | (((lb & 0x1f) * k) >> 5);
    (hi + lo) as u16
}

/// `half(c)` with the 565 mask 0xf7de (spr_p5.dll @ 0x1003e124).
fn half(c: u16) -> u16 {
    (c & 0xf7de) >> 1
}

/// The sprite library the game loads (@ 0x0059cb93): `spr_p6.dll` for the
/// CPU classes from 4 of FUN_0059f670 (CPUID family 6: a Pentium Pro, a
/// Pentium II and the later family-6 CPUs), `spr_p5.dll` for the others
/// (family 4 or 5, and families above 6, whose class is −1). The two draw
/// alike but in `AFF_SPRTRS50`'s shade runs (and in `AFF_SPRALPHA`, which
/// the game never calls). The default is the Pentium II's, the port's
/// reference machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SprDll {
    /// `spr_p5.dll`: shade runs at 50 % `shade(dst, f >> 1)` (0x74 family,
    /// @ 0x10005b58) or `half(dst) + half(half(dst))` (0x274 family,
    /// @ 0x10006773).
    P5,
    /// `spr_p6.dll`: shade runs at 50 % `shade(dst, f)` (0x74 family,
    /// @ 0x10005deb; 0x78 @ 0x1000586b) or `half(dst)` (0x274 family,
    /// @ 0x100068c7).
    #[default]
    P6,
}

/// How `AtlSpr_Aff` (@ 0x0047bc40) blends a sprite: its fifth argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// 0, `AFF_SPR` (spr_p5.dll @ 0x100028e5).
    Opaque,
    /// 1, `AFF_SPRTRS50` (spr_p5.dll @ 0x100029e8, spr_p6.dll
    /// @ 0x10002a63): the sprite at 50 % over the screen. Palette runs
    /// `half(c) + half(dst)`, pair runs `half(c) + shade(dst, f >> 1)`,
    /// shade runs as the library draws them ([`SprDll`]).
    Half,
}

#[derive(Clone, Copy, PartialEq)]
enum Run {
    Start,
    Skip,
    Pal,
    Pair,
    Shade,
    End,
}

/// The next run of a row (spr_p5.dll @ 0x10003ac0..0x10003bbf,
/// 0x10003fe0..0x100040c8): the meaning of the control byte's low bits
/// depends on the previous run.
fn next_run(prev: Run, b: u8) -> (Run, usize) {
    use Run::*;
    let n1 = usize::from(b >> 1) + 1;
    let n2 = usize::from(b >> 2) + 1;
    let n3 = usize::from(b >> 3) + 1;
    let bit = |k: u8| b >> k & 1 == 1;
    let low1 = bit(0);
    let low2 = b & 3 == 2;
    let low3 = b & 7 == 4;
    match prev {
        Start | End => {
            if low1 {
                (Skip, n1)
            } else if low2 {
                (if bit(2) { Shade } else { Pair }, n3)
            } else if low3 {
                (Pal, n3)
            } else {
                (End, 0)
            }
        }
        Pair => {
            if low1 {
                (if bit(1) { Pal } else { Skip }, n2)
            } else if low2 {
                (End, 0)
            } else if low3 {
                (Pair, n3)
            } else {
                (Shade, n3)
            }
        }
        Pal => {
            if low1 {
                (Pair, n1)
            } else if low2 {
                (if bit(2) { Pal } else { Shade }, n3)
            } else if low3 {
                (Skip, n3)
            } else {
                (End, 0)
            }
        }
        Shade => {
            if low1 {
                (if bit(1) { Skip } else { Pair }, n2)
            } else if low2 {
                (End, 0)
            } else if low3 {
                (Shade, n3)
            } else {
                (Pal, n3)
            }
        }
        Skip => {
            if low1 {
                (Pair, n1)
            } else if low2 {
                (Shade, n2)
            } else if low3 {
                (Skip, n3)
            } else {
                (Pal, n3)
            }
        }
    }
}

impl SprBank {
    /// Reads a whole file (@ 0x0059d9b2).
    pub fn parse(data: &[u8]) -> Result<SprBank, SprError> {
        let magic = data.get(..8).ok_or(SprError::Truncated)?;
        if magic != b"SPRCRYO " && magic != b"SPRCRYO1" {
            return Err(SprError::NotSpr);
        }
        let data = data[8..].to_vec();
        let flags = u32_at(&data, 0).ok_or(SprError::Truncated)?;
        let mut at = 4;
        let mut pal = vec![0u16; 256];
        if flags & 4 != 0 {
            for (i, p) in pal.iter_mut().enumerate() {
                *p = to565(u16_at(&data, at + 2 * i).ok_or(SprError::Truncated)?);
            }
            at += 0x200;
        }
        if flags & 8 != 0 {
            // SPR_CONVPAL16 (spr_p5.dll @ 0x1000249e) for RGB565: the
            // green's low bit dropped.
            for (i, p) in pal.iter_mut().enumerate() {
                let e = data
                    .get(at + 4 * i..at + 4 * i + 3)
                    .ok_or(SprError::Truncated)?;
                let (b, g, r) = (u16::from(e[0]), u16::from(e[1]), u16::from(e[2]));
                *p = (r >> 3) << 11 | ((g >> 2) & 0xfe) << 5 | (b >> 3);
            }
            at += 0x400;
        }
        let count = u32_at(&data, at).ok_or(SprError::Truncated)?;
        Ok(SprBank {
            data,
            flags,
            pal,
            offsets_at: at + 4,
            count,
        })
    }

    /// The number of slots (`GET_NBSPR`).
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

    /// A sprite's header; none for an empty slot.
    pub fn info(&self, index: usize) -> Option<SpriteInfo> {
        let at = self.sprite_at(index)?;
        let d = &self.data;
        Some(SpriteInfo {
            w: u16_at(d, at)?,
            h: u16_at(d, at + 2)?,
            xoff: u16_at(d, at + 4)? as i16,
            yoff: u16_at(d, at + 6)? as i16,
            default_pos: if self.flags & 0x800 != 0 {
                Some((u16_at(d, at - 4)?, u16_at(d, at - 2)?))
            } else {
                None
            },
        })
    }

    /// `AFF_SPR` (spr_p5.dll @ 0x100028e5) of sprite `index` with its hot
    /// spot at (x, y); `None` for x = the default position. Returns false
    /// when nothing could be drawn (no such sprite, a family the blitters
    /// of this port do not have).
    pub fn draw(&self, t: &mut Target, x: Option<i32>, y: i32, index: usize) -> bool {
        self.draw_mode(t, x, y, index, Mode::Opaque)
    }

    /// `AFF_SPR` or `AFF_SPRTRS50` of sprite `index`, as [`SprBank::draw`],
    /// with the default library ([`SprDll::P6`]).
    pub fn draw_mode(
        &self,
        t: &mut Target,
        x: Option<i32>,
        y: i32,
        index: usize,
        mode: Mode,
    ) -> bool {
        self.draw_with(t, x, y, index, mode, SprDll::default())
    }

    /// [`SprBank::draw_mode`] with the library `dll`. The original's
    /// `AFF_SPRTRS50` also has a blitter for the 0x278 family
    /// (spr_p5.dll @ 0x100060b8), which no shipped bank uses: not here.
    pub fn draw_with(
        &self,
        t: &mut Target,
        x: Option<i32>,
        y: i32,
        index: usize,
        mode: Mode,
        dll: SprDll,
    ) -> bool {
        // The two families of the shipped banks: 0x74 / 0x874 / 0x878 (a
        // factor byte in shade runs, @ 0x100039d5 and, for 0x878, its own
        // blitter @ 0x10002fc3, which draws every sprite of the one 0x878
        // bank as the first does) and 0x274 / 0xa74 (none, `half`,
        // @ 0x10003ef8).
        let kind = (self.flags & 0x3ff) >> 1;
        let half_shade = match (kind, mode) {
            (0x3a | 0x3c, _) => false,
            (0x13a, _) => true,
            _ => return false,
        };
        let half_mode = mode == Mode::Half;
        let Some(info) = self.info(index) else {
            return false;
        };
        let Some(at) = self.sprite_at(index) else {
            return false;
        };
        let (x, y) = match (x, info.default_pos) {
            (None, Some((dx, dy))) => (i32::from(dx), i32::from(dy)),
            (None, None) => (0, y),
            (Some(x), _) => (x, y),
        };
        let x0 = x - i32::from(info.xoff);
        let y0 = y - i32::from(info.yoff);
        let (w, h) = (i32::from(info.w), i32::from(info.h));
        let d = &self.data;
        let rows = at + 8 + 4 * info.h as usize;
        for row in 0..h {
            let sy = y0 + row;
            if sy < t.ymin || sy >= t.ymax {
                continue;
            }
            let Some(off) = u32_at(d, at + 8 + 4 * row as usize) else {
                break;
            };
            let mut p = rows + off as usize;
            let line = sy as usize * t.pitch;
            let mut sx = x0;
            let mut prev = Run::Start;
            while sx < x0 + w {
                let Some(&b) = d.get(p) else { break };
                p += 1;
                let (run, n) = next_run(prev, b);
                prev = run;
                let inside = |sx: i32| sx >= t.xmin && sx < t.xmax;
                match run {
                    Run::End => break,
                    Run::Start => unreachable!(),
                    Run::Skip => sx += n as i32,
                    Run::Pal => {
                        for _ in 0..n {
                            let Some(&i) = d.get(p) else { return true };
                            p += 1;
                            if inside(sx) {
                                let px = &mut t.pixels[line + sx as usize];
                                let c = self.pal[usize::from(i)];
                                *px = if half_mode {
                                    half(c).wrapping_add(half(*px))
                                } else {
                                    c
                                };
                            }
                            sx += 1;
                        }
                    }
                    Run::Pair => {
                        for _ in 0..n {
                            let (Some(&f), Some(&i)) = (d.get(p), d.get(p + 1)) else {
                                return true;
                            };
                            p += 2;
                            if inside(sx) {
                                let px = &mut t.pixels[line + sx as usize];
                                let c = self.pal[usize::from(i)];
                                *px = if half_mode {
                                    half(c).wrapping_add(shade(*px, f >> 1))
                                } else {
                                    c.wrapping_add(shade(*px, f))
                                };
                            }
                            sx += 1;
                        }
                    }
                    Run::Shade => {
                        let f = if half_shade {
                            None
                        } else {
                            let Some(&f) = d.get(p) else { return true };
                            p += 1;
                            Some(f)
                        };
                        for _ in 0..n {
                            if inside(sx) {
                                let px = &mut t.pixels[line + sx as usize];
                                *px = match (f, half_mode, dll) {
                                    (Some(f), false, _) | (Some(f), true, SprDll::P6) => {
                                        shade(*px, f)
                                    }
                                    (Some(f), true, SprDll::P5) => shade(*px, f >> 1),
                                    (None, false, _) | (None, true, SprDll::P6) => half(*px),
                                    // spr_p5.dll @ 0x10006773: half(dst) +
                                    // half(half(dst)).
                                    (None, true, SprDll::P5) => {
                                        half(*px).wrapping_add(half(half(*px)))
                                    }
                                };
                            }
                            sx += 1;
                        }
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_ends() {
        // f = 0 keeps (almost) the colour, f = 31 gives black.
        assert_eq!(shade(0xffff, 31), 0);
        assert_eq!(shade(0, 0), 0);
        let c = shade(0xffff, 0);
        assert!(c >> 11 >= 29 && c & 31 >= 29, "{c:#x}");
        assert_eq!(half(0xffff), 0x7bef);
    }

    #[test]
    fn rows_of_both_families() {
        // A 4 x 1 sprite: two palette pixels, an end; then 0x274's shade
        // run without a byte.
        let mut d = b"SPRCRYO ".to_vec();
        d.extend(0x74u32.to_le_bytes());
        let mut pal = vec![0u8; 0x200];
        pal[2..4].copy_from_slice(&0x001fu16.to_le_bytes());
        d.extend(&pal);
        d.extend(1u32.to_le_bytes());
        d.extend(4u32.to_le_bytes());
        for v in [4u16, 1, 0, 0] {
            d.extend(v.to_le_bytes());
        }
        d.extend(0u32.to_le_bytes());
        // R run of 2 (b = (1 << 3) | 4), indices 1, 1; after R "000" ends.
        d.extend([0x0c, 1, 1, 0x00]);
        let bank = SprBank::parse(&d).unwrap();
        let mut px = vec![0xffffu16; 8 * 4];
        let mut t = Target::screen(&mut px, 8, 4);
        assert!(bank.draw(&mut t, Some(1), 1, 0));
        assert_eq!(&px[8 + 1..8 + 5], &[0x1f, 0x1f, 0xffff, 0xffff]);
        assert!(SprBank::parse(b"SPRCRYO").is_err());
        assert_eq!(
            SprBank::parse(b"SPRCRYOX0000").unwrap_err(),
            SprError::NotSpr
        );
    }
}

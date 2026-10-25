//! `datas_a2\spw\*.SPW`: the place animations drawn into the faces of the
//! warp cube.
//!
//! Unlike the first game's SPW, a file is a bank of frames, each holding up
//! to six sprites, one per cube face, in the first game's sprite row
//! grammar. The executable draws them itself (`spr_p5.dll` is not used):
//! the blitter @ 0x0049e14c, called by @ 0x0047cf00 into the 512 x 512
//! face surfaces.
//!
//! Layout (@ 0x0047cb30 `Get_ZoneSpw`, the blitter @ 0x0049ea8a):
//!
//! | Offset | Content |
//! |---|---|
//! | 0 | u16 flags: 0x474 (a colour table) or 0x4b0 (direct colours) |
//! | 2 | 0x400 bytes when flags & 8 (no shipped file) |
//! | | u16\[256\] RGB555 colour table when flags & 4 |
//! | | s16 frame count, then u32 frame offsets from the byte after the count |
//! | frame | u32\[6\] face offsets from the frame start, 0 = no sprite on that face |
//! | face | u16 w, h, x, y (top-left in the face), u32\[h\] row offsets, rows |

use std::fmt;

/// The width and height of a cube face.
pub const FACE: usize = 512;

/// Why a file is not an SPW bank.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpwError {
    Truncated,
    /// The magic of the `SPWCRYO` variant (@ 0x0059dc8b), which no shipped
    /// file has.
    Cryo,
}

impl fmt::Display for SpwError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpwError::Truncated => write!(f, "truncated SPW file"),
            SpwError::Cryo => write!(f, "SPWCRYO files are not supported"),
        }
    }
}

impl std::error::Error for SpwError {}

/// A sprite's place on a face: size and top-left texel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FaceBox {
    pub w: u16,
    pub h: u16,
    pub x: u16,
    pub y: u16,
}

/// A clip rectangle on a face: rows `y0..y1`, columns `x0..x1` (upper
/// bounds exclusive, as the blitter's globals @ 0x0067da40..0x0067da4c).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clip {
    pub y0: i32,
    pub y1: i32,
    pub x0: i32,
    pub x1: i32,
}

impl Clip {
    /// The whole face.
    pub const FACE: Clip = Clip {
        y0: 0,
        y1: FACE as i32,
        x0: 0,
        x1: FACE as i32,
    };
}

/// The blend of a pair-run pixel over the face (@ 0x0047d200 for an
/// RGB565 screen): per channel `dst × (32 − f) / 32`, truncated, the green
/// as `g × (64 − 2f) / 64`. Three tables of 32 x 256 dwords, as built.
#[derive(Clone)]
pub struct Blend {
    b: Vec<u32>,
    g: Vec<u32>,
    r: Vec<u32>,
}

impl fmt::Debug for Blend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Blend(RGB565)")
    }
}

impl Default for Blend {
    fn default() -> Blend {
        Blend::rgb565()
    }
}

impl Blend {
    /// The tables of an RGB565 screen: B @ 0x006b2df0 indexed by the low
    /// byte, R @ 0x00673fac by the high byte, G @ 0x00685a8c by
    /// `(dst >> 5) & 0xff`.
    pub fn rgb565() -> Blend {
        let mut t = Blend {
            b: vec![0; 32 * 256],
            g: vec![0; 32 * 256],
            r: vec![0; 32 * 256],
        };
        for f in 0..32u32 {
            for v in 0..256u32 {
                let i = (f * 256 + v) as usize;
                t.b[i] = (v & 31) * (32 - f) / 32;
                t.g[i] = ((v & 63) * (64 - 2 * f) / 64) << 5;
                t.r[i] = (((v >> 3) & 31) * (32 - f) / 32) << 11;
            }
        }
        t
    }

    /// `c + dst × (32 − f) / 32` (the factor taken modulo 32 as the table
    /// index; the shipped files never exceed 31).
    #[inline]
    fn pair(&self, c: u16, dst: u16, f: u8) -> u16 {
        let f = usize::from(f & 31) * 256;
        let d = usize::from(dst);
        (u32::from(c)
            + self.b[f + (d & 0xff)]
            + self.r[f + (d >> 8)]
            + self.g[f + ((d >> 5) & 0xff)]) as u16
    }
}

/// An SPW bank, its colours converted for an RGB565 screen as the loader
/// does (@ 0x0047caa0).
#[derive(Clone, Debug)]
pub struct Spw {
    data: Vec<u8>,
    /// The flags (0x474 or 0x4b0).
    pub flags: u16,
    /// The colour table, RGB565 (flags & 4).
    pal: Vec<u16>,
    /// Where the frame count is.
    count_at: usize,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// RGB555 to RGB565 as @ 0x0049dd60 (table) and @ 0x0049de3e (pixels) do
/// it: `(v >> 5) << 6 | v & 0x1f`.
fn to565(v: u16) -> u16 {
    ((v >> 5) << 6) | (v & 0x1f)
}

impl Spw {
    /// Reads a whole file.
    pub fn parse(mut data: Vec<u8>) -> Result<Spw, SpwError> {
        if data.starts_with(b"SPWCRYO") {
            return Err(SpwError::Cryo);
        }
        let flags = u16_at(&data, 0).ok_or(SpwError::Truncated)?;
        let mut at = 2;
        if flags & 8 != 0 {
            at += 0x400;
        }
        let mut pal = Vec::new();
        if flags & 4 != 0 {
            pal = (0..256)
                .map(|i| u16_at(&data, at + 2 * i).map(to565))
                .collect::<Option<Vec<u16>>>()
                .ok_or(SpwError::Truncated)?;
            at += 0x200;
        }
        u16_at(&data, at).ok_or(SpwError::Truncated)?;
        let mut spw = Spw {
            data: Vec::new(),
            flags,
            pal,
            count_at: at,
        };
        // Direct colours (0x10 / 0x20 without a table, @ 0x0049de3e): the
        // u16 colours of every row converted in place.
        if flags & 0x40 == 0 && flags & 0x30 != 0 {
            let frames = spw.frames_in(&data);
            for frame in 0..frames {
                for face in 0..6 {
                    for (pos, _) in spw.colour_spans(&data, frame, face) {
                        if let Some(v) = u16_at(&data, pos) {
                            data[pos..pos + 2].copy_from_slice(&to565(v).to_le_bytes());
                        }
                    }
                }
            }
        }
        spw.data = data;
        Ok(spw)
    }

    fn frames_in(&self, data: &[u8]) -> usize {
        u16_at(data, self.count_at).map_or(0, |c| (c as i16).max(0) as usize)
    }

    /// The frame count (read signed by @ 0x0047cb30).
    pub fn frame_count(&self) -> i32 {
        i32::from(u16_at(&self.data, self.count_at).unwrap_or(0) as i16)
    }

    /// The start of face `face` of frame `frame`, if it has a sprite.
    fn face_at(&self, data: &[u8], frame: usize, face: usize) -> Option<usize> {
        let base = self.count_at + 2;
        let frame_at = base + u32_at(data, base + 4 * frame)? as usize;
        let off = u32_at(data, frame_at + 4 * face)? as usize;
        (off != 0).then_some(frame_at + off)
    }

    /// The box of the sprite on face `face` of frame `frame` (@ 0x0047cb30:
    /// none for frame −1 or a frame past the count).
    pub fn face_box(&self, frame: i32, face: usize) -> Option<FaceBox> {
        if frame < 0 || frame > self.frame_count() || face > 5 {
            return None;
        }
        let at = self.face_at(&self.data, frame as usize, face)?;
        Some(FaceBox {
            w: u16_at(&self.data, at)?,
            h: u16_at(&self.data, at + 2)?,
            x: u16_at(&self.data, at + 4)?,
            y: u16_at(&self.data, at + 6)?,
        })
    }

    /// The byte positions of the u16 colours of a direct-colour sprite
    /// (for the conversion at load), with their pixel count.
    fn colour_spans(&self, data: &[u8], frame: usize, face: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let Some(at) = self.face_at(data, frame, face) else {
            return out;
        };
        let (Some(w), Some(h)) = (u16_at(data, at), u16_at(data, at + 2)) else {
            return out;
        };
        let rows = at + 8 + 4 * usize::from(h);
        for row in 0..usize::from(h) {
            let Some(off) = u32_at(data, at + 8 + 4 * row) else {
                return out;
            };
            let mut p = rows + off as usize;
            let mut x = 0;
            while x < usize::from(w) {
                let Some(&b) = data.get(p) else { return out };
                p += 1;
                let n = usize::from(b & 0x3f);
                if b & 0x80 != 0 {
                    for k in 0..n {
                        out.push((p + 3 * k + 1, 1));
                    }
                    p += 3 * n;
                    x += n;
                } else if b & 0x40 != 0 {
                    for k in 0..n {
                        out.push((p + 2 * k, 1));
                    }
                    p += 2 * n;
                    x += n;
                } else {
                    x += usize::from(b);
                    if b == 0 {
                        break;
                    }
                }
            }
        }
        out
    }

    /// Draws face `face` of frame `frame` into `dst` (a 512 x 512 face,
    /// pitch 512) as @ 0x0049e14c does: nothing when the flags have 0x200,
    /// when the frame (as u16) is not below the count, or when the face has
    /// no sprite; pixels outside `clip` are left. Returns whether it drew.
    pub fn draw(
        &self,
        frame: i32,
        face: usize,
        dst: &mut [u16],
        clip: Clip,
        blend: &Blend,
    ) -> bool {
        if self.flags & 0x200 != 0
            || (frame as u16) >= (self.frame_count() as u16)
            || face > 5
            || dst.len() < FACE * FACE
        {
            return false;
        }
        let data = &self.data;
        let Some(at) = self.face_at(data, frame as usize, face) else {
            return false;
        };
        let (Some(w), Some(h), Some(x0), Some(y0)) = (
            u16_at(data, at),
            u16_at(data, at + 2),
            u16_at(data, at + 4),
            u16_at(data, at + 6),
        ) else {
            return false;
        };
        let direct = self.flags & 0x40 == 0;
        let rows = at + 8 + 4 * usize::from(h);
        for row in 0..i32::from(h) {
            let y = i32::from(y0) + row;
            if y < clip.y0 || y >= clip.y1 {
                continue;
            }
            let Some(off) = u32_at(data, at + 8 + 4 * row as usize) else {
                break;
            };
            let mut p = rows + off as usize;
            let line = y as usize * FACE;
            let mut x = 0i32;
            let put = |x: i32, dst: &mut [u16], c: &dyn Fn(u16) -> u16| {
                let sx = i32::from(x0) + x;
                if sx >= clip.x0 && sx < clip.x1 {
                    let d = &mut dst[line + sx as usize];
                    *d = c(*d);
                }
            };
            while x < i32::from(w) {
                let Some(&b) = data.get(p) else { break };
                p += 1;
                let n = i32::from(b & 0x3f);
                if b & 0x80 != 0 {
                    // Pair run: factor, colour.
                    for _ in 0..n {
                        let Some(&f) = data.get(p) else { return true };
                        let c = if direct {
                            let Some(c) = u16_at(data, p + 1) else {
                                return true;
                            };
                            p += 3;
                            c
                        } else {
                            let Some(&i) = data.get(p + 1) else {
                                return true;
                            };
                            p += 2;
                            self.pal[usize::from(i)]
                        };
                        put(x, dst, &|d| blend.pair(c, d, f));
                        x += 1;
                    }
                } else if b & 0x40 != 0 {
                    // Colour run.
                    for _ in 0..n {
                        let c = if direct {
                            let Some(c) = u16_at(data, p) else {
                                return true;
                            };
                            p += 2;
                            c
                        } else {
                            let Some(&i) = data.get(p) else { return true };
                            p += 1;
                            self.pal[usize::from(i)]
                        };
                        put(x, dst, &|_| c);
                        x += 1;
                    }
                } else {
                    // Skip; a zero skip would never end the row.
                    if b == 0 {
                        break;
                    }
                    x += i32::from(b);
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame, one sprite of 3 x 1 on face 2 at (5, 7): a colour pixel,
    /// a skip, a pair pixel.
    fn bank() -> Vec<u8> {
        let mut d = 0x474u16.to_le_bytes().to_vec();
        let mut pal = vec![0u8; 0x200];
        pal[2..4].copy_from_slice(&0x7fffu16.to_le_bytes()); // index 1: white
        pal[4..6].copy_from_slice(&0x0400u16.to_le_bytes()); // index 2: R 1
        d.extend(pal);
        d.extend(1i16.to_le_bytes());
        d.extend(4u32.to_le_bytes()); // frame 0 right after the table
        let mut frame = vec![0u8; 24];
        frame[8..12].copy_from_slice(&24u32.to_le_bytes());
        for v in [3u16, 1, 5, 7] {
            frame.extend(v.to_le_bytes());
        }
        frame.extend(0u32.to_le_bytes());
        frame.extend([0x41, 1, 0x01, 0x81, 16, 2]);
        d.extend(frame);
        d
    }

    #[test]
    fn draws_runs_and_blends() {
        let spw = Spw::parse(bank()).unwrap();
        assert_eq!(spw.frame_count(), 1);
        assert_eq!(
            spw.face_box(0, 2),
            Some(FaceBox {
                w: 3,
                h: 1,
                x: 5,
                y: 7
            })
        );
        assert_eq!(spw.face_box(0, 1), None);
        let mut face = vec![0xffffu16; FACE * FACE];
        let blend = Blend::rgb565();
        assert!(spw.draw(0, 2, &mut face, Clip::FACE, &blend));
        let row = &face[7 * FACE + 5..7 * FACE + 8];
        // White in 565; the skip; R 1 + white × 16/32 per channel.
        assert_eq!(row[0], 0xffdf);
        assert_eq!(row[1], 0xffff);
        assert_eq!(row[2], (1 << 11) + ((15 << 11) | (31 << 5) | 15));
        // Frame −1 and a frame past the end draw nothing.
        assert!(!spw.draw(-1, 2, &mut face, Clip::FACE, &blend));
        assert!(!spw.draw(1, 2, &mut face, Clip::FACE, &blend));
        // Clipped away.
        let mut face2 = vec![0u16; FACE * FACE];
        let clip = Clip {
            y0: 0,
            y1: 7,
            x0: 0,
            x1: 512,
        };
        spw.draw(0, 2, &mut face2, clip, &blend);
        assert!(face2.iter().all(|&p| p == 0));
    }

    #[test]
    fn short_files() {
        assert_eq!(Spw::parse(vec![0x74]).unwrap_err(), SpwError::Truncated);
        assert_eq!(
            Spw::parse(b"SPWCRYO ".to_vec()).unwrap_err(),
            SpwError::Cryo
        );
        let mut d = bank();
        d.truncate(d.len() - 3);
        let spw = Spw::parse(d).unwrap();
        let mut face = vec![0u16; FACE * FACE];
        spw.draw(0, 2, &mut face, Clip::FACE, &Blend::rgb565());
    }
}

//! The dialog subtitles (@ 0x004611e0): the line breaking and placement of
//! the original, transcribed byte for byte, and the text drawn with the
//! game's own TrueType font, `loc\Atl_Font.ttf` (`AtlFT_Dialog` of
//! `loc\Font.txt`: Gill Alt One MT, 12 points).
//!
//! The original draws with GDI on the back buffer's DC (`TextOutA`), the
//! font created with `CreateFontA(−MulDiv(12, 96, 72) = −16, ...)`: 16
//! pixels per em at 96 DPI. Two parts of what GDI does are reproduced from
//! the font's own tables:
//!
//! - the widths `GetTextExtentPoint32A` measures, and the advance of each
//!   character: the `hdmx` record of 16 pixels per em (the hinted advance
//!   widths the font stores for GDI), no kerning;
//! - the ascent: the `VDMX` record of 16 pixels (yMax 18, yMin −5), which
//!   GDI takes for the text metrics (top of a 23-pixel cell; the baseline 18
//!   pixels below the y `TextOutA` is given).
//!
//! The glyph shapes are the outlines scaled to 16 pixels per em and filled
//! where a pixel is at least half covered (monochrome, as GDI draws on a
//! 16-bit surface without smoothing). GDI also runs the font's hinting
//! program, which moves the outlines to the pixel grid; that is not done
//! here, so a few pixels of each glyph differ from the original's: the
//! one known limit of the subtitles (the first game's port has the same one).
//!
//! The breaking rules (`FUN_004611e0`, checked against the original under
//! emulation: `subtitles.txt`):
//!
//! - characters are added to the line while the line's width stays under
//!   W − 20; a byte with the high bit set is taken with the next byte as
//!   one character (the code is written for double-byte code pages: in
//!   Windows-1252 an accented letter is glued to the character after it,
//!   so `é ` is not a break);
//! - the break characters are `Font.txt`'s `[PartitionCode]` (0x20); the
//!   line ends after the last one met (the break character stays at the end
//!   of the line), and the next line starts after it;
//! - a line that overflows without a break is cut before its last
//!   character and the rest of the text is dropped;
//! - at most (H − 20) / h lines, h the extent's height (23); the line
//!   started when that count is reached is drawn with whatever bytes the
//!   buffer holds there;
//! - line j of n (from 1) is drawn at x = 10, y = H − 12 (n − j + 1) − 10
//!   in white, after a black copy at (+1, +1): the lines are 12 pixels
//!   apart, `Font.txt`'s height in points (@ 0x00404c50), so they overlap.

use ab_glyph::{point, Font, FontVec, Glyph, PxScale};
use atlantis_data::TextCodePage;

use atlantis2_data::dialog::FontTxt;

/// One `TextOutA` call: position, colour (0xRRGGBB) and bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextOut {
    pub x: i32,
    pub y: i32,
    pub colour: u32,
    pub bytes: Vec<u8>,
}

/// Whether the character at `s` (one byte, or a lead byte and the next)
/// is a break character (@ 0x00404840).
fn is_break(breaks: &[u16], s: &[u8]) -> bool {
    let b0 = s.first().copied().unwrap_or(0);
    if b0 & 0x80 == 0 {
        breaks.iter().any(|&c| c & 0x8000 == 0 && b0 == c as u8)
    } else {
        let b1 = s.get(1).copied().unwrap_or(0);
        breaks
            .iter()
            .any(|&c| (c >> 8) & 0x80 != 0 && b0 == (c >> 8) as u8 && b1 == c as u8)
    }
}

/// The subtitle layout of `text` (@ 0x004611e0) on a `screen_w` x
/// `screen_h` screen: `measure` gives the width and height of a run of
/// bytes (`GetTextExtentPoint32A`), `line_step` is the font's height in
/// points. Returns the `TextOutA` calls in order.
pub fn layout(
    text: &[u8],
    measure: impl Fn(&[u8]) -> (i32, i32),
    line_step: i32,
    breaks: &[u16],
    screen_w: i32,
    screen_h: i32,
) -> Vec<TextOut> {
    // The copy of the text (`_mbscpy` into a 1024-byte buffer, then a
    // second NUL after the first) and the zeroed output buffer.
    let mut src = vec![0u8; 1026];
    let n = text
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(text.len())
        .min(1023);
    src[..n].copy_from_slice(&text[..n]);
    // (1024 bytes in the original, the stack after it; the lines' NULs
    // need at most 20 bytes more than the text.)
    let mut out = vec![0u8; 2048];
    let mut lines = vec![0usize];
    let (mut j, mut i, mut saved_i, mut next_start) = (0usize, 0usize, 0usize, 0usize);
    let mut found = false;
    loop {
        let start = *lines.last().unwrap();
        let c = src[i];
        if c == 0 {
            out[start + j] = 0;
            break;
        }
        let brk = is_break(breaks, &src[i..]);
        out[start + j] = c;
        if c & 0x80 != 0 {
            // A lead byte: the next byte goes with it.
            out[start + j + 1] = src[i + 1];
            j += 2;
            i += 2;
        } else {
            j += 1;
            i += 1;
        }
        if brk {
            saved_i = i;
            next_start = start + j;
            found = true;
        }
        let (cx, cy) = measure(&out[start..start + j]);
        if cx < screen_w - 20 {
            continue;
        }
        if !found {
            out[start + j - 1] = 0;
            out[start + j] = 0;
            break;
        }
        out[next_start] = 0;
        lines.push(next_start + 1);
        j = 0;
        i = saved_i;
        found = false;
        let max = if cy > 0 {
            ((screen_h - 20) as u32 / cy as u32) as usize
        } else {
            usize::MAX
        };
        if lines.len() >= max {
            break;
        }
    }
    let count = lines.len() as i32;
    let mut calls = Vec::new();
    for (m, &s) in lines.iter().enumerate() {
        let len = out[s..].iter().position(|&b| b == 0).unwrap_or(0);
        let bytes = out[s..s + len].to_vec();
        let y = screen_h - line_step * (count - m as i32);
        calls.push(TextOut {
            x: 11,
            y: y - 9,
            colour: 0,
            bytes: bytes.clone(),
        });
        calls.push(TextOut {
            x: 10,
            y: y - 10,
            colour: 0xffffff,
            bytes,
        });
    }
    calls
}

/// The `hdmx` advance widths of `ppem` pixels per em, by glyph.
fn hdmx_widths(ttf: &[u8], ppem: u8) -> Option<Vec<u8>> {
    let (off, len) = table(ttf, b"hdmx")?;
    let t = ttf.get(off..off + len)?;
    let n = i16::from_be_bytes([*t.get(2)?, *t.get(3)?]);
    let size = u32::from_be_bytes(t.get(4..8)?.try_into().ok()?) as usize;
    for r in 0..n.max(0) as usize {
        let rec = t.get(8 + r * size..8 + (r + 1) * size)?;
        if rec[0] == ppem {
            return Some(rec[2..].to_vec());
        }
    }
    None
}

/// The `VDMX` yMax and yMin of `ppem` (the first ratio group that fits
/// any ratio, or 1:1).
fn vdmx(ttf: &[u8], ppem: u16) -> Option<(i32, i32)> {
    let (off, len) = table(ttf, b"VDMX")?;
    let t = ttf.get(off..off + len)?;
    let be16 = |p: usize| t.get(p..p + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let nratios = usize::from(be16(4)?);
    for k in 0..nratios {
        let r = t.get(6 + 4 * k..10 + 4 * k)?;
        let any = r[1] == 0 && r[2] == 0 && r[3] == 0;
        let square = r[1] == 1 && r[2] <= 1 && r[3] >= 1;
        if !(any || square) {
            continue;
        }
        let g = usize::from(be16(6 + 4 * nratios + 2 * k)?);
        let recs = usize::from(be16(g)?);
        for e in 0..recs {
            let p = g + 4 + 6 * e;
            if be16(p)? == ppem {
                return Some((be16(p + 2)? as i16 as i32, be16(p + 4)? as i16 as i32));
            }
        }
    }
    None
}

/// The offset and length of a table of a TrueType file.
fn table(ttf: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    let n = usize::from(u16::from_be_bytes([*ttf.get(4)?, *ttf.get(5)?]));
    for i in 0..n {
        let rec = ttf.get(12 + 16 * i..28 + 16 * i)?;
        if &rec[0..4] == tag {
            let off = u32::from_be_bytes(rec[8..12].try_into().ok()?) as usize;
            let len = u32::from_be_bytes(rec[12..16].try_into().ok()?) as usize;
            return Some((off, len));
        }
    }
    None
}

/// The subtitle font: `AtlFT_Dialog` of `Font.txt` in the game's TrueType
/// file, at the size GDI makes of its height.
pub struct SubtitleFont {
    font: FontVec,
    /// Pixels per em: MulDiv(points, 96, 72).
    ppem: u16,
    widths: Option<Vec<u8>>,
    ascent: i32,
    /// The extent's height (`tmHeight`).
    cell: i32,
    /// `Font.txt`'s height in points: the step between lines.
    line_step: i32,
    breaks: Vec<u16>,
}

impl SubtitleFont {
    /// The dialog font of `font_txt` from `ttf`, the bytes of its file.
    pub fn new(ttf: Vec<u8>, font_txt: &FontTxt) -> Option<SubtitleFont> {
        SubtitleFont::named(ttf, font_txt, "AtlFT_Dialog")
    }

    /// Font `id` of `font_txt` (`AtlFT_POPUP` for the menus' texts and the
    /// bag's names, 10 points: 13 pixels per em) from `ttf`.
    pub fn named(ttf: Vec<u8>, font_txt: &FontTxt, id: &str) -> Option<SubtitleFont> {
        let entry = font_txt.font(id)?;
        // CreateFontA(−MulDiv(height, LOGPIXELSY, 72)) at 96 DPI.
        let ppem = ((i64::from(entry.height) * 96 + 36) / 72) as u16;
        let widths = hdmx_widths(&ttf, ppem as u8);
        let font = FontVec::try_from_vec(ttf.clone()).ok()?;
        let upem = font.units_per_em()?;
        let (ascent, descent) = match vdmx(&ttf, ppem) {
            Some(a) => a,
            None => {
                let s = f32::from(ppem) / upem;
                (
                    (font.ascent_unscaled() * s).round() as i32,
                    (font.descent_unscaled() * s).round() as i32,
                )
            }
        };
        Some(SubtitleFont {
            font,
            ppem,
            widths,
            ascent,
            cell: ascent - descent,
            line_step: entry.height,
            breaks: font_txt.breaks.clone(),
        })
    }

    fn glyph(&self, b: u8) -> ab_glyph::GlyphId {
        self.font.glyph_id(TextCodePage::Windows1252.char_of(b))
    }

    /// The advance of byte `b`, in pixels.
    fn advance(&self, b: u8) -> i32 {
        let g = self.glyph(b);
        match &self.widths {
            Some(w) => i32::from(w.get(usize::from(g.0)).copied().unwrap_or(0)),
            None => {
                let s = f32::from(self.ppem) / self.font.units_per_em().unwrap_or(2048.0);
                (self.font.h_advance_unscaled(g) * s).round() as i32
            }
        }
    }

    /// `GetTextExtentPoint32A`: width and height of `bytes`.
    pub fn extent(&self, bytes: &[u8]) -> (i32, i32) {
        (bytes.iter().map(|&b| self.advance(b)).sum(), self.cell)
    }

    /// The layout of `text` on a `w` x `h` screen.
    pub fn layout(&self, text: &[u8], w: i32, h: i32) -> Vec<TextOut> {
        layout(text, |b| self.extent(b), self.line_step, &self.breaks, w, h)
    }

    /// Draws `calls` into `out` (`w` x `h` RGB565).
    pub fn draw(&self, out: &mut [u16], w: usize, h: usize, calls: &[TextOut]) {
        self.draw_with(calls, |px, py, colour| {
            if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                out[py as usize * w + px as usize] = rgb565(colour);
            }
        });
    }

    /// The pixels of `calls`: `put(x, y, colour)` for each pixel set.
    pub fn draw_with(&self, calls: &[TextOut], mut put: impl FnMut(i32, i32, u32)) {
        let upem = self.font.units_per_em().unwrap_or(2048.0);
        // ab_glyph's scale is the unscaled height (ascent − descent) in
        // pixels: one em is `ppem` pixels.
        let scale = PxScale::from(f32::from(self.ppem) * self.font.height_unscaled() / upem);
        for call in calls {
            let mut pen = call.x;
            let base = (call.y + self.ascent) as f32;
            for &b in &call.bytes {
                let g = Glyph {
                    id: self.glyph(b),
                    scale,
                    position: point(pen as f32, base),
                };
                if let Some(o) = self.font.outline_glyph(g) {
                    let bb = o.px_bounds();
                    o.draw(|x, y, c| {
                        if c < 0.5 {
                            return;
                        }
                        put(
                            bb.min.x as i32 + x as i32,
                            bb.min.y as i32 + y as i32,
                            call.colour,
                        );
                    });
                }
                pen += self.advance(b);
            }
        }
    }

    /// `Font.txt`'s height in points (@ 0x00404c50: the font record's
    /// +0x44), the step between the lines of the subtitles and the credits.
    pub fn line_step(&self) -> i32 {
        self.line_step
    }
}

/// A GDI colour (0xRRGGBB, red in the low byte for `COLORREF`; the two the
/// subtitles use are black and white) on a 16-bit 565 surface.
fn rgb565(c: u32) -> u16 {
    let (r, g, b) = (c & 0xff, (c >> 8) & 0xff, (c >> 16) & 0xff);
    (((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths(b: &[u8]) -> (i32, i32) {
        (b.len() as i32 * 10, 23)
    }

    #[test]
    fn short_text_one_line() {
        let c = layout(b"Hello", widths, 12, &[0x20], 640, 480);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].x, c[0].y, c[0].colour), (11, 459, 0));
        assert_eq!((c[1].x, c[1].y, c[1].colour), (10, 458, 0xffffff));
        assert_eq!(c[1].bytes, b"Hello");
    }

    #[test]
    fn breaks_after_the_last_space_and_glues_accents() {
        // 10 pixels a byte, 620 available: 62 bytes fit.
        let mut t = b"word ".repeat(13);
        t.extend_from_slice(b"end");
        let c = layout(&t, widths, 12, &[0x20], 640, 480);
        assert_eq!(c.len(), 4);
        assert_eq!(c[1].bytes.len(), 60);
        assert!(c[1].bytes.ends_with(b" "));
        assert_eq!(c[3].bytes, b"word end");
        assert_eq!((c[1].y, c[3].y), (446, 458));
        // `\xe9 ` is one character: no break there.
        let mut t = b"x".repeat(55);
        t.extend_from_slice(b"\xe9 yyyyyyyyyyyy");
        let c = layout(&t, widths, 12, &[0x20], 640, 480);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].bytes.len(), 61);
    }

    #[test]
    fn colours() {
        assert_eq!(rgb565(0xffffff), 0xffff);
        assert_eq!(rgb565(0), 0);
    }
}

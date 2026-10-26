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
//! The glyph shapes are drawn as GDI draws them in monochrome (a 16-bit
//! surface without font smoothing, Windows 9x's default; GOG's quality
//! `NONANTIALIASED_QUALITY` everywhere): the font's hinting program run
//! at the size (the TrueType interpreter, for a monochrome target), then
//! the pixels whose centre is inside the hinted outline, and a pixel for
//! each dropout (a run of a row or a column between two crossings that
//! holds no pixel centre: the one nearest its middle). Measured against
//! GDI (`tests/gdi_glyphs.rs`: every byte at every size the game asks, on
//! Windows 98 SE and on Windows 11), 70 to 80 % of the glyphs are GDI's
//! pixel for pixel and the others differ by a pixel or two at a diagonal's
//! step: GDI's own dropout and stub rules are not all known, and Windows
//! 98 and 11 differ from each other on about one glyph in twenty. A font
//! with no hinting program is filled where a pixel is at least half
//! covered. On a Windows with font smoothing on, 1.00 and 1.01
//! (`DEFAULT_QUALITY`) draw smoothed glyphs; the port does not.
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
//!
//! The 1.01 patch (and GOG's `Game.exe`, built on it) changes three things
//! (`atlantis2-gog-fr-2183168` @ 0x0045a2f0, 0x0046ec70, 0x0046f140,
//! 0x0046f0c0; checked under emulation of the 1.01 executable:
//! `subtitles_101_*.txt`):
//!
//! - a byte with the high bit set is a lead byte only when `Font.txt`'s
//!   `[CharacterSet]` is `MBCS`; with `SBCS` or no such section (the
//!   French files) every byte is a character of its own, and a break
//!   character is any table entry whose low byte is that byte: French
//!   lines break at the space after an accented letter;
//! - `Height` is a cell height: `CreateFontA(+Height)` (through `DPtoLP`,
//!   the identity of `MM_TEXT`), so GDI takes the largest size whose
//!   `VDMX` height (yMax − yMin) fits; GOG's `Font.txt` asks 22 for the
//!   dialogs (15 pixels per em, a 21-pixel cell) and 16 for the popups
//!   (11 pixels per em). A height below the font's smallest `VDMX` cell
//!   (the CD's `Font.txt`, 12 and 10) gets the size under that record, 7
//!   pixels per em, in a cell of the height asked. These sizes are what
//!   GDI gives on Windows (measured on Windows 11 and Windows 98 SE for
//!   both `Atl_Font.ttf` files, every height from 2 to 40; Windows 98
//!   differs only below the smallest `VDMX` cell, where its cell is the
//!   size's own: the port follows Windows 11, where the GOG edition runs);
//! - the lines are `tmHeight` + 1 apart (`GetTextMetricsA`, through
//!   `LPtoDP`): 22 pixels with GOG's files, no longer overlapping.
//!
//! GOG's own patch draws the fonts without antialiasing
//! (`NONANTIALIASED_QUALITY`), as the glyphs are drawn here in both
//! releases. A font file that is missing is replaced by a system font
//! (1.01 logs `Font %s not found, using system font`; 1.00's
//! `AddFontResourceA` fails and GDI substitutes a face the same way): here
//! the port's Liberation Sans, which has Arial's metrics.

use ab_glyph::{point, Font, FontVec, Glyph, PxScale};
use atlantis_data::TextCodePage;

use atlantis2_data::dialog::FontTxt;
use atlantis2_data::exe::Release;
use atlantis2_data::{A2Files, Folder};

/// One `TextOutA` call: position, colour (0xRRGGBB) and bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextOut {
    pub x: i32,
    pub y: i32,
    pub colour: u32,
    pub bytes: Vec<u8>,
}

/// Whether the character at `s` (one byte, or a lead byte and the next)
/// is a break character (@ 0x00404840). `mbcs`: a byte with the high bit
/// set is a lead byte (always in 1.00; in 1.01 with `[CharacterSet] MBCS`
/// only, else every byte is compared with the entries' low bytes,
/// `atlantis2-gog-fr-2183168` @ 0x0046ec70).
fn is_break(breaks: &[u16], s: &[u8], mbcs: bool) -> bool {
    let b0 = s.first().copied().unwrap_or(0);
    if !mbcs {
        return breaks.iter().any(|&c| b0 == c as u8);
    }
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
/// bytes (`GetTextExtentPoint32A`), `line_step` is the step between the
/// lines (1.00: the font's height in points; 1.01: `tmHeight` + 1),
/// `mbcs` whether a byte with the high bit set takes the next byte with it
/// (always in 1.00). Returns the `TextOutA` calls in order.
pub fn layout(
    text: &[u8],
    measure: impl Fn(&[u8]) -> (i32, i32),
    line_step: i32,
    breaks: &[u16],
    mbcs: bool,
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
        let brk = is_break(breaks, &src[i..], mbcs);
        out[start + j] = c;
        if mbcs && c & 0x80 != 0 {
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

/// GDI's size for a cell height `height` (`CreateFontA` with a positive
/// height, 1.01) in a font with a `VDMX` table: in the group of the font's
/// ratio, the first record whose yMax − yMin equals `height`, else the one
/// before the first that is taller. When even the first record is taller,
/// the size is the smaller of MulDiv(unitsPerEm, height, winAscent +
/// winDescent) and the first record's size − 1, the cell is `height` and
/// the ascent MulDiv(height, winAscent, winAscent + winDescent). `None`
/// without a `VDMX` group for the ratio, or past its last record. Returns
/// ppem, ascent, −descent.
///
/// Measured on Windows 11 for both `Atl_Font.ttf` files at every height
/// from 2 to 40 (`CreateFontA` with the arguments of 1.01's
/// `atlantis2-gog-fr-2183168` @ 0x0046f140, `GetTextMetricsA`): the rule
/// gives Windows' tmHeight, tmAscent and size at each of them. Windows 98
/// SE agrees down to the first record; below it, its cell is the size's
/// own (9 pixels at 7 per em), not `height`.
fn vdmx_for_height(ttf: &[u8], height: i32) -> Option<(u16, i32, i32)> {
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
        let rec = |e: usize| -> Option<(u16, i32, i32)> {
            let p = g + 4 + 6 * e;
            Some((
                be16(p)?,
                be16(p + 2)? as i16 as i32,
                be16(p + 4)? as i16 as i32,
            ))
        };
        for e in 0..recs {
            let (ppem, ymax, ymin) = rec(e)?;
            if ymax - ymin == height {
                return Some((ppem, ymax, ymin));
            }
            if ymax - ymin > height {
                if e > 0 {
                    return rec(e - 1);
                }
                return below_vdmx(ttf, height, ppem);
            }
        }
        return None;
    }
    None
}

/// GDI's font of a cell height below the first `VDMX` record (whose size
/// is `first_ppem`), see [`vdmx_for_height`].
fn below_vdmx(ttf: &[u8], height: i32, first_ppem: u16) -> Option<(u16, i32, i32)> {
    let (wa, wd) = (i64::from(os2_u16(ttf, 74)?), i64::from(os2_u16(ttf, 76)?));
    let upem = i64::from(head_upem(ttf)?);
    if wa + wd == 0 || height < 1 {
        return None;
    }
    let muldiv = |a: i64, b: i64, c: i64| (a * b + c / 2) / c;
    let h = i64::from(height);
    let ppem = muldiv(upem, h, wa + wd)
        .min(i64::from(first_ppem) - 1)
        .max(1);
    let ascent = muldiv(h, wa, wa + wd) as i32;
    Some((ppem as u16, ascent, ascent - height))
}

/// The font's units per em (`head`).
fn head_upem(ttf: &[u8]) -> Option<u16> {
    let (off, len) = table(ttf, b"head")?;
    if len < 20 {
        return None;
    }
    Some(u16::from_be_bytes([
        *ttf.get(off + 18)?,
        *ttf.get(off + 19)?,
    ]))
}

/// A `u16` of the font's `OS/2` table at `offset`.
fn os2_u16(ttf: &[u8], offset: usize) -> Option<u16> {
    let (off, len) = table(ttf, b"OS/2")?;
    if offset + 2 > len {
        return None;
    }
    Some(u16::from_be_bytes([
        *ttf.get(off + offset)?,
        *ttf.get(off + offset + 1)?,
    ]))
}

/// The size and the ascent and descent GDI gives a cell height of a font
/// without a `VDMX` table (the port's system font): ppem =
/// MulDiv(unitsPerEm, height, usWinAscent + usWinDescent), the ascent and
/// descent the `OS/2` win values at that size, rounded as 26.6 fixed-point
/// values. Windows 11 gives these for Liberation Sans within a pixel; its
/// hinted widths, which the port does not compute, differ more.
fn cell_from_win_metrics(font: &FontVec, ttf: &[u8], height: i32) -> (u16, i32, i32) {
    let upem = font.units_per_em().unwrap_or(2048.0) as i64;
    let (wa, wd) = match (os2_u16(ttf, 74), os2_u16(ttf, 76)) {
        (Some(a), Some(d)) if a as i64 + d as i64 > 0 => (a as i64, d as i64),
        _ => (
            font.ascent_unscaled().round() as i64,
            (-font.descent_unscaled()).round() as i64,
        ),
    };
    let ppem = ((upem * i64::from(height) + (wa + wd) / 2) / (wa + wd)).max(1);
    let px26 = |v: i64| (v * ppem * 64 + upem / 2) / upem;
    let ascent = ((px26(wa) + 32) >> 6) as i32;
    let descent = ((px26(wd) + 32) >> 6) as i32;
    (ppem as u16, ascent, -descent)
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
    /// Each byte's glyph as GDI draws it in monochrome ([`gdi_glyph`]);
    /// `None` for the whole table when the font cannot be hinted (its
    /// unhinted outline is then filled where a pixel is half covered).
    glyphs: Option<Vec<Option<GlyphBits>>>,
    /// Pixels per em: MulDiv(points, 96, 72).
    ppem: u16,
    widths: Option<Vec<u8>>,
    ascent: i32,
    /// The extent's height (`tmHeight`).
    cell: i32,
    /// The step between lines: `Font.txt`'s height in points (1.00), or
    /// `tmHeight` + 1 (1.01).
    line_step: i32,
    breaks: Vec<u16>,
    /// A byte with the high bit set is a lead byte (1.00; 1.01 with
    /// `[CharacterSet] MBCS`).
    mbcs: bool,
}

impl SubtitleFont {
    /// The dialog font of `font_txt` from `ttf`, the bytes of its file.
    pub fn new(ttf: Vec<u8>, font_txt: &FontTxt) -> Option<SubtitleFont> {
        SubtitleFont::named(ttf, font_txt, "AtlFT_Dialog")
    }

    /// Font `id` of `font_txt` (`AtlFT_POPUP` for the menus' texts and the
    /// bag's names, 10 points: 13 pixels per em) from `ttf`, as 1.00 makes
    /// it.
    pub fn named(ttf: Vec<u8>, font_txt: &FontTxt, id: &str) -> Option<SubtitleFont> {
        SubtitleFont::named_for(ttf, font_txt, id, Release::V100)
    }

    /// Font `id` of `font_txt` from `ttf` as `release` makes it (see the
    /// module's notes).
    pub fn named_for(
        ttf: Vec<u8>,
        font_txt: &FontTxt,
        id: &str,
        release: Release,
    ) -> Option<SubtitleFont> {
        let entry = font_txt.font(id)?;
        let font = FontVec::try_from_vec(ttf.clone()).ok()?;
        let upem = font.units_per_em()?;
        let (ppem, ascent, descent, line_step, mbcs) = match release {
            Release::V100 => {
                // CreateFontA(−MulDiv(height, LOGPIXELSY, 72)) at 96 DPI.
                let ppem = ((i64::from(entry.height) * 96 + 36) / 72) as u16;
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
                (ppem, ascent, descent, entry.height, true)
            }
            Release::V101 => {
                // CreateFontA(+height): a cell height.
                let (ppem, ascent, descent) = match vdmx_for_height(&ttf, entry.height) {
                    Some(r) => r,
                    None => cell_from_win_metrics(&font, &ttf, entry.height),
                };
                // GetTextMetricsA: tmHeight + 1.
                let line_step = ascent - descent + 1;
                (
                    ppem,
                    ascent,
                    descent,
                    line_step,
                    font_txt.mbcs == Some(true),
                )
            }
        };
        let widths = hdmx_widths(&ttf, ppem as u8);
        let glyphs = gdi_glyphs(&ttf, ppem, &font);
        Some(SubtitleFont {
            font,
            glyphs,
            ppem,
            widths,
            ascent,
            cell: ascent - descent,
            line_step,
            breaks: font_txt.breaks.clone(),
            mbcs,
        })
    }

    /// Font `id` of `font_txt` for the game in `files`: its file in
    /// `loc\`, made as the executable's release makes it; a system font
    /// (the port's Liberation Sans) when the file is missing.
    pub fn load(files: &A2Files, font_txt: &FontTxt, id: &str) -> Option<SubtitleFont> {
        let release = files.build().release;
        let file = &font_txt.font(id)?.file;
        let ttf = match files.read(Folder::Loc, file) {
            Some(t) if !file.is_empty() => t,
            _ => {
                match release {
                    Release::V100 => log::warn!("add_font() ERREUR: {file} ({id}): system font"),
                    Release::V101 => log::warn!("Font {file} not found, using system font"),
                }
                atlantis_engine::subtitles::system_font_data().to_vec()
            }
        };
        SubtitleFont::named_for(ttf, font_txt, id, release)
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
        layout(
            text,
            |b| self.extent(b),
            self.line_step,
            &self.breaks,
            self.mbcs,
            w,
            h,
        )
    }

    /// Pixels per em, the ascent and the cell height (`tmHeight`).
    pub fn metrics(&self) -> (u16, i32, i32) {
        (self.ppem, self.ascent, self.cell)
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
                if let Some(table) = &self.glyphs {
                    if let Some(g) = &table[usize::from(b)] {
                        for (k, &on) in g.bits.iter().enumerate() {
                            if on {
                                let (x, y) = ((k % g.w) as i32, (k / g.w) as i32);
                                put(
                                    pen + g.x0 + x,
                                    call.y + self.ascent - g.top + y,
                                    call.colour,
                                );
                            }
                        }
                    }
                    pen += self.advance(b);
                    continue;
                }
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

    /// The HD mode (a port addition, the first game's "Sharp subtitles"):
    /// `call` drawn again at a larger scale, the 640x480 screen mapped to
    /// the `dw` x `dh` pixels at (`ox`, `oy`) of `out` (XRGB8888, `stride`
    /// pixels per row). The layout is the original's: each glyph starts at
    /// its pen position of [`SubtitleFont::draw`] (the `hdmx` advances)
    /// and the baseline is the `VDMX` ascent below the call's y, both
    /// scaled; the outline is rasterised at `ppem` times the scale and
    /// blended by its coverage (smoothed), times `alpha` / 255, in the
    /// call's colour. Nothing is drawn outside those pixels.
    pub fn draw_hd(
        &self,
        out: &mut [u32],
        stride: usize,
        (ox, oy, dw, dh): (usize, usize, usize, usize),
        call: &TextOut,
        alpha: u8,
    ) {
        let upem = self.font.units_per_em().unwrap_or(2048.0);
        let (sx, sy) = (dw as f32 / 640.0, dh as f32 / 480.0);
        let scale = PxScale::from(f32::from(self.ppem) * sy * self.font.height_unscaled() / upem);
        let base = oy as f32 + (call.y + self.ascent) as f32 * sy;
        let c = call.colour;
        let colour = (c & 0xff) << 16 | (c & 0xff00) | (c >> 16) & 0xff;
        let (x1, y1) = ((ox + dw) as i32, (oy + dh) as i32);
        let mut pen = call.x;
        for &b in &call.bytes {
            let g = Glyph {
                id: self.glyph(b),
                scale,
                position: point(ox as f32 + pen as f32 * sx, base),
            };
            if let Some(o) = self.font.outline_glyph(g) {
                let bb = o.px_bounds();
                let (bx, by) = (bb.min.x as i32, bb.min.y as i32);
                o.draw(|gx, gy, cov| {
                    let (px, py) = (bx + gx as i32, by + gy as i32);
                    if px < ox as i32 || py < oy as i32 || px >= x1 || py >= y1 {
                        return;
                    }
                    let a = (cov.clamp(0.0, 1.0) * 256.0) as u32 * u32::from(alpha) / 255;
                    if a == 0 {
                        return;
                    }
                    let d = &mut out[py as usize * stride + px as usize];
                    *d = atlantis_engine::upscale::lerp8888(*d, colour, a.min(256));
                });
            }
            pen += self.advance(b);
        }
    }

    /// The step between the lines of the subtitles and the credits:
    /// `Font.txt`'s height in points (@ 0x00404c50: the font record's
    /// +0x44) in 1.00, `tmHeight` + 1 in 1.01
    /// (`atlantis2-gog-fr-2183168` @ 0x0046f0c0).
    pub fn line_step(&self) -> i32 {
        self.line_step
    }
}

/// A GDI colour (0xRRGGBB, red in the low byte for `COLORREF`; the two the
/// subtitles use are black and white) on a 16-bit 565 surface.
pub(crate) fn rgb565(c: u32) -> u16 {
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
        let c = layout(b"Hello", widths, 12, &[0x20], true, 640, 480);
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
        let c = layout(&t, widths, 12, &[0x20], true, 640, 480);
        assert_eq!(c.len(), 4);
        assert_eq!(c[1].bytes.len(), 60);
        assert!(c[1].bytes.ends_with(b" "));
        assert_eq!(c[3].bytes, b"word end");
        assert_eq!((c[1].y, c[3].y), (446, 458));
        // `\xe9 ` is one character: no break there.
        let mut t = b"x".repeat(55);
        t.extend_from_slice(b"\xe9 yyyyyyyyyyyy");
        let c = layout(&t, widths, 12, &[0x20], true, 640, 480);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].bytes.len(), 61);
    }

    #[test]
    fn single_byte_characters_in_1_01() {
        // `\xe9 ` breaks: the space after the accent is a break.
        let mut t = b"x".repeat(55);
        t.extend_from_slice(b"\xe9 yyyyyyyyyyyy");
        let c = layout(&t, widths, 22, &[0x20], false, 640, 480);
        assert_eq!(c.len(), 4);
        assert_eq!(c[1].bytes.len(), 57);
        assert_eq!(c[3].bytes, b"yyyyyyyyyyyy");
        assert_eq!((c[1].y, c[3].y), (480 - 44 - 10, 480 - 22 - 10));
        // A table entry with the lead flag matches its low byte alone.
        assert!(is_break(&[0x8120], b" ", false));
        assert!(!is_break(&[0x8120], b" ", true));
    }

    #[test]
    fn colours() {
        assert_eq!(rgb565(0xffffff), 0xffff);
        assert_eq!(rgb565(0), 0);
    }
}

/// A glyph's pixels as GDI lights them: `w` x `h` from the column `x0`
/// right of the pen and the row `top` above the baseline.
#[derive(Clone, Debug)]
struct GlyphBits {
    x0: i32,
    top: i32,
    w: usize,
    bits: Vec<bool>,
}

/// Every byte's glyph ([`gdi_glyph`]) at `ppem`, or `None` when the font
/// has no hinting program to run.
fn gdi_glyphs(ttf: &[u8], ppem: u16, font: &FontVec) -> Option<Vec<Option<GlyphBits>>> {
    use skrifa::instance::{LocationRef, Size};
    use skrifa::outline::{Engine, HintingInstance, HintingOptions, Target};
    use skrifa::{FontRef, MetadataProvider};
    let f = FontRef::new(ttf).ok()?;
    let outlines = f.outline_glyphs();
    let options = HintingOptions {
        engine: Engine::Interpreter,
        target: Target::Mono,
    };
    let hinting = HintingInstance::new(
        &outlines,
        Size::new(f32::from(ppem)),
        LocationRef::default(),
        options,
    )
    .ok()?;
    Some(
        (0..=255u8)
            .map(|b| {
                let id = font.glyph_id(TextCodePage::Windows1252.char_of(b));
                let g = outlines.get(skrifa::GlyphId::new(u32::from(id.0)))?;
                gdi_glyph(&g, &hinting)
            })
            .collect(),
    )
}

/// The outline as the font's hinting program leaves it at that size
/// (the TrueType interpreter for a monochrome target) and GDI's scan
/// conversion of it: a pixel lit when its centre is inside (non-zero
/// winding), then the dropouts: a run between two crossings of a row's
/// or a column's centre line that holds no pixel centre lights the pixel
/// nearest its middle. GDI's own rules for the dropouts (and the stubs it
/// leaves out) are not all known: measured against Windows 98 SE and 11
/// (`tests/gdi_glyphs.rs`).
fn gdi_glyph(
    g: &skrifa::outline::OutlineGlyph,
    hinting: &skrifa::outline::HintingInstance,
) -> Option<GlyphBits> {
    let mut pen = Flatten::default();
    g.draw(
        skrifa::outline::DrawSettings::hinted(hinting, false),
        &mut pen,
    )
    .ok()?;
    pen.end();
    let cs = &pen.contours;
    let pts = cs.iter().flatten();
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &(x, y) in pts {
        xmin = xmin.min(x);
        xmax = xmax.max(x);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    if xmin > xmax {
        return None;
    }
    // The pixels (columns x0.., rows from `top` above the baseline down)
    // that may hold a centre or a dropout, with a margin of one.
    let x0 = xmin.floor() as i32 - 1;
    let top = ymax.ceil() as i32 + 1;
    let w = (xmax.ceil() as i32 + 1 - x0) as usize;
    let h = (top - (ymin.floor() as i32 - 1)) as usize;
    let mut bits = vec![false; w * h];
    // Pixel (i, j) has its centre at x = x0 + i + 0.5, y = top − j − 0.5.
    for j in 0..h {
        let cy = f64::from(top) - j as f64 - 0.5;
        for i in 0..w {
            let cx = f64::from(x0) + i as f64 + 0.5;
            bits[j * w + i] = winding(cs, cx, cy) != 0;
        }
    }
    // Dropouts along the rows, then the columns.
    for j in 0..h {
        let cy = f64::from(top) - j as f64 - 0.5;
        for (l, r) in runs(cs, cy, false) {
            let (l, r) = (l - f64::from(x0), r - f64::from(x0));
            if (l - 0.5).ceil() > (r - 0.5).floor() {
                let m = ((l + r) / 2.0).floor();
                if m >= 0.0 && (m as usize) < w {
                    bits[j * w + m as usize] = true;
                }
            }
        }
    }
    for i in 0..w {
        let cx = f64::from(x0) + i as f64 + 0.5;
        for (lo, hi) in runs(cs, cx, true) {
            // Rows down from `top`.
            let (a, b) = (f64::from(top) - hi, f64::from(top) - lo);
            if (a - 0.5).ceil() > (b - 0.5).floor() {
                let m = ((a + b) / 2.0).floor();
                if m >= 0.0 && (m as usize) < h {
                    bits[m as usize * w + i] = true;
                }
            }
        }
    }
    Some(GlyphBits { x0, top, w, bits })
}

/// Non-zero winding number of the polygons `cs` at (x, y).
fn winding(cs: &[Vec<(f64, f64)>], x: f64, y: f64) -> i32 {
    let mut n = 0;
    for c in cs {
        for k in 0..c.len() {
            let (x0, y0) = c[k];
            let (x1, y1) = c[(k + 1) % c.len()];
            let side = (x1 - x0) * (y - y0) - (x - x0) * (y1 - y0);
            if y0 <= y {
                if y1 > y && side > 0.0 {
                    n += 1;
                }
            } else if y1 <= y && side < 0.0 {
                n -= 1;
            }
        }
    }
    n
}

/// The inside runs (from, to) of the line y = `at` (`vertical`: x = `at`,
/// the runs along y), from the crossings and their windings.
fn runs(cs: &[Vec<(f64, f64)>], at: f64, vertical: bool) -> Vec<(f64, f64)> {
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for c in cs {
        for k in 0..c.len() {
            let (mut a, mut b) = (c[k], c[(k + 1) % c.len()]);
            if vertical {
                a = (a.1, a.0);
                b = (b.1, b.0);
            }
            if (a.1 <= at && b.1 > at) || (b.1 <= at && a.1 > at) {
                let t = (at - a.1) / (b.1 - a.1);
                xs.push((a.0 + t * (b.0 - a.0), if b.1 > a.1 { 1 } else { -1 }));
            }
        }
    }
    xs.sort_by(|p, q| p.0.total_cmp(&q.0));
    let mut out = Vec::new();
    let mut n = 0;
    let mut from = 0.0;
    for (x, d) in xs {
        let before = n;
        n += d;
        if before == 0 && n != 0 {
            from = x;
        } else if before != 0 && n == 0 {
            out.push((from, x));
        }
    }
    out
}

/// The hinted outline as polygons (each quadratic curve in 64 steps).
#[derive(Default)]
struct Flatten {
    contours: Vec<Vec<(f64, f64)>>,
    cur: Vec<(f64, f64)>,
}

impl Flatten {
    fn end(&mut self) {
        if !self.cur.is_empty() {
            self.contours.push(std::mem::take(&mut self.cur));
        }
    }
}

impl skrifa::outline::OutlinePen for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.end();
        self.cur.push((f64::from(x), f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cur.push((f64::from(x), f64::from(y)));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (x0, y0) = self.cur.last().copied().unwrap_or_default();
        let (cx, cy, x, y) = (f64::from(cx), f64::from(cy), f64::from(x), f64::from(y));
        for k in 1..=64 {
            let t = f64::from(k) / 64.0;
            let u = 1.0 - t;
            self.cur.push((
                u * u * x0 + 2.0 * u * t * cx + t * t * x,
                u * u * y0 + 2.0 * u * t * cy + t * t * y,
            ));
        }
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (x0, y0) = self.cur.last().copied().unwrap_or_default();
        let p = [
            (x0, y0),
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(cx1), f64::from(cy1)),
            (f64::from(x), f64::from(y)),
        ];
        for k in 1..=64 {
            let t = f64::from(k) / 64.0;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.cur.push((
                a * p[0].0 + b * p[1].0 + c * p[2].0 + d * p[3].0,
                a * p[0].1 + b * p[1].1 + c * p[2].1 + d * p[3].1,
            ));
        }
    }
    fn close(&mut self) {
        self.end();
    }
}

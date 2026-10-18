//! The dialog subtitles: the line breaks of `FUN_0042ffe8`, the layout of
//! `FUN_00430178` and the font; for the 1997 build (atlantis-cd-en-688128),
//! its own: the 8x8 bitmap font `DIALOG\FONTS.BIN`, the breaks of
//! `FUN_0042e970` and the layout of `FUN_0042eb3c` ([`BitmapFont`]).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (`subtitle_layout.cpp`, `engine.cpp` `subtitleFont`, the subtitle draw of
//! `dialog.cpp`; GPLv3, the ScummVM team).
//!
//! The original draws the subtitles with the system's Arial
//! (`FUN_0041ae20`: CreateFontA with a cell height of 16, which Windows 98
//! renders at 13 pixels per em, without smoothing). The port draws them
//! with Liberation Sans, which has the metrics of Arial, at 13 pixels per
//! em, monochrome, as the ScummVM port does when the player has no
//! arial.ttf. The font is bundled (`assets/fonts`, SIL Open Font Licence
//! 1.1) and rasterised by `ab_glyph`, so every target, the web included,
//! draws the same pixels.

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};

/// The subtitle font, Liberation Sans Regular.
static FONT_DATA: &[u8] = include_bytes!("../../../assets/fonts/LiberationSans-Regular.ttf");

/// The pixels per em of the subtitle font.
pub const SUBTITLE_EM_PX: f32 = 13.0;

/// `FUN_00430178`: the x of every subtitle line.
pub const SUBTITLE_X: i32 = 5;

/// The width a subtitle line may take: the screen less 5-pixel margins.
pub const SUBTITLE_WIDTH: i32 = 640 - 5 - 5;

/// Text colour and shadow (white, black shadow at +1, +1).
pub const SUBTITLE_COLOR: u16 = 0xffff;
pub const SUBTITLE_SHADOW: u16 = 0x0000;

/// `FUN_0042ffe8` on the subtitle buffer `FUN_0042844c` fills: a space (the
/// byte after ']') then the text. Under 101 bytes it is one line; otherwise
/// each line ends before the last space within the next 100 bytes and the
/// following line starts at that space, until the remaining length is 0.
///
/// A window of 100 bytes with no space makes the original write a NUL every
/// 0x6e bytes past its line buffer until a write faults; no shipped text
/// has one. The port cuts such a window at 100 bytes.
pub fn split_subtitle(text: &[u8]) -> Vec<Vec<u8>> {
    let mut b = Vec::with_capacity(text.len() + 1);
    b.push(b' ');
    b.extend_from_slice(text);
    let mut lines = Vec::new();
    let mut remaining = b.len() as i64;
    if remaining < 0x65 {
        lines.push(b);
        return lines;
    }
    let mut start = 0usize;
    let mut p = 100usize;
    loop {
        if remaining < 0x65 {
            lines.push(b[start.min(b.len())..].to_vec());
            break;
        }
        // Back from p to the first space (a CR also stops it; the buffer
        // holds none).
        let mut back = 0usize;
        while back < 100 && b.get(p - back).copied() != Some(b' ') {
            back += 1;
        }
        if back == 100 {
            back = 0;
        }
        let end = (start + 100 - back).min(b.len());
        lines.push(b[start.min(end)..end].to_vec());
        remaining -= (100 - back) as i64;
        start = p - back;
        p = start + 100;
        if remaining <= 0 {
            break;
        }
    }
    lines
}

/// `FUN_00430178`: line `i` of `n` is drawn at y = 0x1ce - (n - i + 1) * 14.
pub fn subtitle_line_y(i: usize, n: usize) -> i32 {
    0x1ce - (n as i32 - i as i32 + 1) * 14
}

/// Windows-1252 for the bytes 0x80..0x9f (the original's GDI draws the
/// text in the ANSI code page); the other bytes are Latin-1.
fn cp1252(b: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
        0x0152, 0, 0x017d, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc,
        0x2122, 0x0161, 0x203a, 0x0153, 0, 0x017e, 0x0178,
    ];
    match b {
        0x80..=0x9f => char::from_u32(u32::from(HIGH[usize::from(b - 0x80)])).unwrap_or('\0'),
        _ => char::from(b),
    }
}

/// One rasterised glyph: its coverage thresholded to on / off.
#[derive(Clone, Debug, Default)]
struct GlyphBits {
    /// The pen advance, rounded to whole pixels.
    advance: i32,
    /// The bitmap's offset from the pen (x) and the baseline (y).
    left: i32,
    top: i32,
    w: i32,
    h: i32,
    bits: Vec<bool>,
}

/// The subtitle font, rasterised once for the 256 byte values.
pub struct SubtitleFont {
    glyphs: Vec<GlyphBits>,
    /// The ascent in whole pixels (rounded up), from the line's top to the
    /// baseline.
    ascent: i32,
}

impl Default for SubtitleFont {
    fn default() -> Self {
        Self::new()
    }
}

impl SubtitleFont {
    pub fn new() -> SubtitleFont {
        let font = FontRef::try_from_slice(FONT_DATA).expect("bundled subtitle font");
        let upem = font.units_per_em().unwrap_or(2048.0);
        // ab_glyph scales by the line height (ascent - descent); the size is
        // given per em.
        let scale = PxScale::from(SUBTITLE_EM_PX * font.height_unscaled() / upem);
        let scaled = font.as_scaled(scale);
        let ascent = scaled.ascent().ceil() as i32;
        let glyphs = (0..=255u8)
            .map(|b| {
                let c = cp1252(b);
                if c < ' ' {
                    return GlyphBits::default();
                }
                let id = font.glyph_id(c);
                if id.0 == 0 {
                    return GlyphBits::default();
                }
                let advance = scaled.h_advance(id).round() as i32;
                let glyph = id.with_scale_and_position(scale, point(0.0, 0.0));
                let Some(outline) = font.outline_glyph(glyph) else {
                    return GlyphBits {
                        advance,
                        ..GlyphBits::default()
                    };
                };
                let bounds = outline.px_bounds();
                let (w, h) = (bounds.width() as i32, bounds.height() as i32);
                let mut bits = vec![false; (w * h).max(0) as usize];
                outline.draw(|x, y, c| {
                    if c >= 0.5 {
                        if let Some(p) = bits.get_mut(y as usize * w as usize + x as usize) {
                            *p = true;
                        }
                    }
                });
                GlyphBits {
                    advance,
                    left: bounds.min.x as i32,
                    top: bounds.min.y as i32,
                    w,
                    h,
                    bits,
                }
            })
            .collect();
        SubtitleFont { glyphs, ascent }
    }

    /// The width of `text` in pixels.
    pub fn width(&self, text: &[u8]) -> i32 {
        text.iter()
            .map(|&b| self.glyphs[usize::from(b)].advance)
            .sum()
    }

    /// Draws `text` with the top of its line at (x, y) on an RGB565 buffer
    /// of `width` x `height`, as ScummVM's `Font::drawString` with a width
    /// limit of `max_w`: a glyph that would end past x + max_w + 1 stops
    /// the line.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        dst: &mut [u16],
        width: usize,
        height: usize,
        text: &[u8],
        x: i32,
        y: i32,
        max_w: i32,
        color: u16,
    ) {
        let right = x + max_w + 1;
        let mut pen = x;
        let base = y + self.ascent;
        for &b in text {
            let g = &self.glyphs[usize::from(b)];
            if pen + g.left + g.w > right {
                break;
            }
            for gy in 0..g.h {
                let dy = base + g.top + gy;
                if dy < 0 || dy as usize >= height {
                    continue;
                }
                for gx in 0..g.w {
                    if !g.bits[(gy * g.w + gx) as usize] {
                        continue;
                    }
                    let dx = pen + g.left + gx;
                    if dx < 0 || dx as usize >= width {
                        continue;
                    }
                    dst[dy as usize * width + dx as usize] = color;
                }
            }
            pen += g.advance;
        }
    }

    /// The glyphs [`SubtitleFont::draw`] draws for `text` at x = `x`
    /// with the width limit `max_w`: each byte with its pen position.
    fn layout(&self, text: &[u8], x: i32, max_w: i32) -> Vec<(u8, i32)> {
        let right = x + max_w + 1;
        let mut pen = x;
        let mut out = Vec::with_capacity(text.len());
        for &b in text {
            let g = &self.glyphs[usize::from(b)];
            if pen + g.left + g.w > right {
                break;
            }
            out.push((b, pen));
            pen += g.advance;
        }
        out
    }

    /// [`SubtitleFont::draw`] at a larger scale, for the HD mode: the same
    /// glyphs at the same places of the 640x480 screen, the screen mapped
    /// to the `dw` x `dh` pixels at (`ox`, `oy`) of `out` (XRGB8888,
    /// `stride` pixels per row), rasterised from the outlines at that
    /// size and blended by their coverage (smoothed), in `color`
    /// (XRGB8888). Nothing is drawn outside those pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_hd(
        &self,
        out: &mut [u32],
        stride: usize,
        (ox, oy, dw, dh): (usize, usize, usize, usize),
        text: &[u8],
        x: i32,
        y: i32,
        max_w: i32,
        color: u32,
    ) {
        let font = FontRef::try_from_slice(FONT_DATA).expect("bundled subtitle font");
        let (sx, sy) = (dw as f32 / 640.0, dh as f32 / 480.0);
        let upem = font.units_per_em().unwrap_or(2048.0);
        let scale = PxScale::from(SUBTITLE_EM_PX * sy * font.height_unscaled() / upem);
        let base = oy as f32 + (y + self.ascent) as f32 * sy;
        let (x1, y1) = ((ox + dw) as i32, (oy + dh) as i32);
        for (b, pen) in self.layout(text, x, max_w) {
            let c = cp1252(b);
            let id = font.glyph_id(c);
            if c < ' ' || id.0 == 0 {
                continue;
            }
            let pos = point(ox as f32 + pen as f32 * sx, base);
            let Some(outline) = font.outline_glyph(id.with_scale_and_position(scale, pos)) else {
                continue;
            };
            let bounds = outline.px_bounds();
            let (bx, by) = (bounds.min.x as i32, bounds.min.y as i32);
            outline.draw(|gx, gy, cov| {
                let (px, py) = (bx + gx as i32, by + gy as i32);
                if px < ox as i32 || py < oy as i32 || px >= x1 || py >= y1 {
                    return;
                }
                let a = (cov.clamp(0.0, 1.0) * 256.0) as u32;
                if a == 0 {
                    return;
                }
                let d = &mut out[py as usize * stride + px as usize];
                *d = crate::upscale::lerp888(*d, color, a.min(256));
            });
        }
    }

    /// The subtitle lines over a 640x480 RGB565 picture (`FUN_00430178`):
    /// line i of n at (5, [`subtitle_line_y`]) in white over its black
    /// shadow at (+1, +1). `y_offset` moves them down (a picture taller
    /// than the screen).
    pub fn draw_subtitles(
        &self,
        dst: &mut [u16],
        width: usize,
        height: usize,
        lines: &[Vec<u8>],
        y_offset: i32,
    ) {
        for (i, line) in lines.iter().enumerate() {
            let y = subtitle_line_y(i, lines.len()) + y_offset;
            self.draw(
                dst,
                width,
                height,
                line,
                SUBTITLE_X + 1,
                y + 1,
                SUBTITLE_WIDTH,
                SUBTITLE_SHADOW,
            );
            self.draw(
                dst,
                width,
                height,
                line,
                SUBTITLE_X,
                y,
                SUBTITLE_WIDTH,
                SUBTITLE_COLOR,
            );
        }
    }
}

/// The 1997 build's subtitle font: `DIALOG\FONTS.BIN` of the archives,
/// 256 glyphs of 8 x 8 bytes, glyph = the raw byte, a non-zero byte a
/// pixel (atlantis-cd-en-688128 loads it at start-up, 0x004191c2..
/// 0x004191f9, and draws with FUN_00410800). The shapes are the IBM PC
/// set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitmapFont {
    glyphs: Vec<u8>,
}

/// The size of `DIALOG\FONTS.BIN`.
pub const BITMAP_FONT_SIZE: usize = 256 * 64;

/// The row height and the x of the 1997 layout (atlantis-cd-en-688128
/// FUN_0042eb3c).
pub const BITMAP_ROW_H: i32 = 10;

/// FUN_0042e970's window: a text of fewer than 0x4c bytes (its leading
/// space included) is one row; else rows of a 75-byte window.
const BITMAP_WINDOW: usize = 0x4b;

impl BitmapFont {
    /// The font from the file's bytes, `None` for another size.
    pub fn new(data: &[u8]) -> Option<BitmapFont> {
        (data.len() == BITMAP_FONT_SIZE).then(|| BitmapFont {
            glyphs: data.to_vec(),
        })
    }

    /// Whether pixel (`col`, `row`) of glyph `b` is set.
    fn on(&self, b: u8, col: usize, row: usize) -> bool {
        self.glyphs[usize::from(b) * 64 + row * 8 + col] != 0
    }

    /// FUN_00410800: `text` at (x, y) on a buffer of `width` pixels per
    /// row, 8 pixels per byte, the set pixels in `color`. There is no
    /// clipping: the pixels are addressed as y * width + x, so a glyph
    /// past the right edge goes on at the left of the next pixel row (the
    /// port drops what falls outside the buffer).
    pub fn draw(&self, dst: &mut [u16], width: usize, text: &[u8], x: i32, y: i32, color: u16) {
        self.for_each_pixel(width, text, x, y, |i| {
            if let Some(p) = usize::try_from(i).ok().and_then(|i| dst.get_mut(i)) {
                *p = color;
            }
        });
    }

    /// The linear index (y * width + x) of each pixel FUN_00410800 sets.
    fn for_each_pixel(&self, width: usize, text: &[u8], x: i32, y: i32, mut f: impl FnMut(i64)) {
        let w = width as i64;
        let mut base = i64::from(y) * w + i64::from(x);
        for &b in text {
            for row in 0..8 {
                for col in 0..8 {
                    if self.on(b, col, row) {
                        f(base + row as i64 * w + col as i64);
                    }
                }
            }
            base += 8;
        }
    }

    /// The subtitle rows over a picture of `width` x `height`
    /// (atlantis-cd-en-688128 FUN_0042eb3c): row i of n, black at (6,
    /// y + 1) then white at (5, y), y = 0x1ce - (n - i + 1) * 10.
    /// `y_offset` moves them down (a picture taller than the screen).
    pub fn draw_subtitles(&self, dst: &mut [u16], width: usize, lines: &[Vec<u8>], y_offset: i32) {
        for (i, line) in lines.iter().enumerate() {
            let y = bitmap_line_y(i, lines.len()) + y_offset;
            self.draw(dst, width, line, SUBTITLE_X + 1, y + 1, SUBTITLE_SHADOW);
            self.draw(dst, width, line, SUBTITLE_X, y, SUBTITLE_COLOR);
        }
    }

    /// [`BitmapFont::draw`] for the HD mode: each font pixel of the
    /// 640x480 screen a sharp block of the `dw` x `dh` pixels at (`ox`,
    /// `oy`) of `out` (XRGB8888, `stride` pixels per row), in `color`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_hd(
        &self,
        out: &mut [u32],
        stride: usize,
        (ox, oy, dw, dh): (usize, usize, usize, usize),
        text: &[u8],
        x: i32,
        y: i32,
        color: u32,
    ) {
        let (sw, sh) = (crate::warp::SCREEN_W, crate::warp::SCREEN_H);
        self.for_each_pixel(sw, text, x, y, |i| {
            let Ok(i) = usize::try_from(i) else { return };
            let (px, py) = (i % sw, i / sw);
            if py >= sh {
                return;
            }
            let (x0, x1) = (ox + px * dw / sw, ox + (px + 1) * dw / sw);
            let (y0, y1) = (oy + py * dh / sh, oy + (py + 1) * dh / sh);
            for row in y0..y1 {
                if let Some(r) = out.get_mut(row * stride + x0..row * stride + x1) {
                    r.fill(color);
                }
            }
        });
    }
}

/// atlantis-cd-en-688128 FUN_0042eb3c: row `i` of `n` at y = 0x1ce -
/// (n - i + 1) * 10.
pub fn bitmap_line_y(i: usize, n: usize) -> i32 {
    0x1ce - (n as i32 - i as i32 + 1) * BITMAP_ROW_H
}

/// atlantis-cd-en-688128 FUN_0042e970 on the subtitle buffer (a space, the
/// byte after ']', then the text): fewer than 0x4c bytes make one row;
/// else, with p the end of a 75-byte window from the row's start, the
/// scans forward (f) and back (b) from p to a space (a CR stops a scan
/// one byte past it); if f < b the row runs to the forward space, else to
/// the back one, and that space starts the next row; until the length
/// left (less 75 + f, or 75 - b, per row) is not positive or under 0x4c.
///
/// The bytes after the text's NUL are what earlier lines left in the
/// buffer; the port takes a CR there, as the emulation of the original
/// (forward scans that reach the end of the text stop 2 bytes past it).
/// A 75-byte window without a space would loop for ever in the original;
/// no shipped text has one, and the port cuts it at 75 bytes.
pub fn split_subtitle_1997(text: &[u8]) -> Vec<Vec<u8>> {
    let mut b = Vec::with_capacity(text.len() + 3);
    b.push(b' ');
    b.extend_from_slice(text);
    let len = b.len();
    if len < BITMAP_WINDOW + 1 {
        return vec![b];
    }
    b.extend_from_slice(b"\0\r");
    // The byte at i (NUL past the buffer: the scans stop at the CR).
    let at = |i: usize| b.get(i).copied().unwrap_or(0);
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut remaining = len as i64;
    loop {
        if remaining < (BITMAP_WINDOW + 1) as i64 {
            lines.push(b[start.min(len)..len].to_vec());
            break;
        }
        let p = start + BITMAP_WINDOW;
        let mut f = 0usize;
        loop {
            let c = at(p + f);
            if c == b' ' {
                break;
            }
            f += 1;
            if c == b'\r' || p + f >= b.len() {
                break;
            }
        }
        let mut back = 0usize;
        loop {
            let c = at(p - back);
            if c == b' ' {
                break;
            }
            back += 1;
            if c == b'\r' || back > p {
                break;
            }
        }
        let (end, step) = if f < back {
            (p + f, BITMAP_WINDOW + f)
        } else if p - back > start {
            (p - back, BITMAP_WINDOW - back)
        } else {
            (p, BITMAP_WINDOW) // no space in the window: cut
        };
        lines.push(b[start..end.min(len)].to_vec());
        remaining -= step as i64;
        start = end;
        if remaining <= 0 {
            break;
        }
    }
    lines
}

/// The subtitles of a build: the reference's GDI Arial layout, or the 1997
/// build's bitmap font and layout.
#[derive(Clone)]
pub enum Subtitler {
    Gdi(std::rc::Rc<SubtitleFont>),
    Bitmap(std::sync::Arc<BitmapFont>),
}

impl Subtitler {
    /// The rows of a subtitle text.
    pub fn split(&self, text: &[u8]) -> Vec<Vec<u8>> {
        match self {
            Subtitler::Gdi(_) => split_subtitle(text),
            Subtitler::Bitmap(_) => split_subtitle_1997(text),
        }
    }

    /// The rows over a picture of `width` x `height` RGB565 pixels.
    pub fn draw_subtitles(
        &self,
        dst: &mut [u16],
        width: usize,
        height: usize,
        lines: &[Vec<u8>],
        y_offset: i32,
    ) {
        match self {
            Subtitler::Gdi(f) => f.draw_subtitles(dst, width, height, lines, y_offset),
            Subtitler::Bitmap(f) => f.draw_subtitles(dst, width, lines, y_offset),
        }
    }

    /// The text items for the HD mode (see [`subtitle_items`]).
    pub fn items(&self, lines: &[Vec<u8>], dx: i32, dy: i32) -> Vec<crate::hd::TextItem> {
        match self {
            Subtitler::Gdi(_) => subtitle_items(lines, dx, dy),
            Subtitler::Bitmap(f) => {
                let mut items = Vec::with_capacity(2 * lines.len());
                for (i, line) in lines.iter().enumerate() {
                    let y = bitmap_line_y(i, lines.len()) + dy;
                    for (o, color) in [(1, SUBTITLE_SHADOW), (0, SUBTITLE_COLOR)] {
                        items.push(crate::hd::TextItem {
                            x: SUBTITLE_X + dx + o,
                            y: y + o,
                            max_w: i32::MAX,
                            color,
                            text: line.clone(),
                            bitmap: Some(f.clone()),
                        });
                    }
                }
                items
            }
        }
    }
}

/// The text items of [`SubtitleFont::draw_subtitles`] for the HD mode
/// (see [`crate::hd::HdText`]): the shadow then the text of each line,
/// moved by (`dx`, `dy`) on the screen.
pub fn subtitle_items(lines: &[Vec<u8>], dx: i32, dy: i32) -> Vec<crate::hd::TextItem> {
    let mut items = Vec::with_capacity(2 * lines.len());
    for (i, line) in lines.iter().enumerate() {
        let y = subtitle_line_y(i, lines.len()) + dy;
        for (o, color) in [(1, SUBTITLE_SHADOW), (0, SUBTITLE_COLOR)] {
            items.push(crate::hd::TextItem {
                x: SUBTITLE_X + dx + o,
                y: y + o,
                max_w: SUBTITLE_WIDTH,
                color,
                text: line.clone(),
                bitmap: None,
            });
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line breaks of the original `FUN_0042ffe8` on these inputs, run
    /// under emulation (ScummVM's `video_dialog.h` cases).
    #[test]
    fn line_breaks_match_original() {
        let s = |t: &str| t.as_bytes().to_vec();
        assert_eq!(split_subtitle(b"Short line."), vec![s(" Short line.")]);
        assert_eq!(
            split_subtitle(
                b"The quick brown fox jumps over the lazy dog, then the lazy dog wakes up and \
                  chases the fox around the field."
            ),
            vec![
                s(" The quick brown fox jumps over the lazy dog, then the lazy dog wakes up and chases the fox around"),
                s(" the field."),
            ]
        );
        let words = ["word"; 30].join(" ");
        assert_eq!(
            split_subtitle(words.as_bytes()),
            vec![
                s(&format!(" {}", ["word"; 20].join(" "))),
                s(&format!(" {}", ["word"; 10].join(" "))),
            ]
        );
    }

    /// A window of 100 bytes without a space: the original faults (it
    /// writes NULs past its line buffer); the port cuts at 100 bytes.
    #[test]
    fn no_space_is_cut() {
        let x = vec![b'x'; 150];
        let lines = split_subtitle(&x);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 100);
        assert_eq!(lines.concat().len(), 151);
    }

    /// Line i of n at y = 0x1ce - (n - i + 1) * 14: one line at 434, lines
    /// 14 px apart.
    #[test]
    fn line_y_matches_original() {
        assert_eq!(subtitle_line_y(0, 1), 0x1ce - 2 * 14);
        assert_eq!(subtitle_line_y(1, 2) - subtitle_line_y(0, 2), 14);
        assert_eq!(subtitle_line_y(1, 2), 0x1ce - 2 * 14);
    }

    #[test]
    fn font_draws_white_over_black_shadow() {
        let f = SubtitleFont::new();
        assert_eq!(f.ascent, 12);
        assert!(f.width(b"Seth") > 20 && f.width(b"Seth") < 30);
        let mut dst = vec![0x1234u16; 640 * 480];
        f.draw_subtitles(&mut dst, 640, 480, &[b" Hello, Seth!".to_vec()], 0);
        let white = dst.iter().filter(|&&p| p == SUBTITLE_COLOR).count();
        let black = dst.iter().filter(|&&p| p == SUBTITLE_SHADOW).count();
        assert!(white > 50, "{white} white pixels");
        assert!(black > 10, "{black} shadow pixels");
        // Everything inside the line's band.
        for (i, &p) in dst.iter().enumerate() {
            if p != 0x1234 {
                let y = (i / 640) as i32;
                assert!((434..434 + 16).contains(&y), "pixel at row {y}");
            }
        }
    }

    #[test]
    fn hd_text_draws_the_same_glyphs_larger() {
        let f = SubtitleFont::new();
        let line = b" Hello, Seth! This line is long enough".to_vec();
        // At scale 1 the smoothed glyphs fall where the 640 ones are.
        let mut lo = vec![0u16; 640 * 480];
        f.draw(&mut lo, 640, 480, &line, 5, 434, SUBTITLE_WIDTH, 0xffff);
        let (w, h) = (1920, 1440);
        let mut hd = vec![0u32; w * h];
        f.draw_hd(
            &mut hd,
            w,
            (0, 0, w, h),
            &line,
            5,
            434,
            SUBTITLE_WIDTH,
            0x00ff_ffff,
        );
        let lit: Vec<usize> = (0..w * h).filter(|&i| hd[i] & 0xff > 0x80).collect();
        assert!(lit.len() > 9 * 200, "{} lit pixels", lit.len());
        // Most of the HD text lies over the 640 text's pixels (the
        // outlines are the same, at 3 times the size).
        let over = lit
            .iter()
            .filter(|&&i| {
                let (x, y) = (i % w / 3, i / w / 3);
                (y.saturating_sub(1)..y + 2)
                    .any(|yy| (x.saturating_sub(1)..x + 2).any(|xx| lo[yy * 640 + xx] != 0))
            })
            .count();
        assert!(over * 100 >= lit.len() * 98, "{over} of {}", lit.len());
        // Smoothed: partly covered pixels.
        assert!(hd.iter().any(|&p| p & 0xff > 0 && p & 0xff < 0xff));
        // Clipped to the picture.
        let mut small = vec![0u32; 100 * 100];
        f.draw_hd(
            &mut small,
            100,
            (10, 10, 80, 60),
            &line,
            5,
            0,
            SUBTITLE_WIDTH,
            0x00ff_ffff,
        );
        for (i, &p) in small.iter().enumerate() {
            if p != 0 {
                let (x, y) = (i % 100, i / 100);
                assert!((10..90).contains(&x) && (10..70).contains(&y));
            }
        }
        let items = subtitle_items(&[line.clone(), line], 0, 0);
        assert_eq!(items.len(), 4);
        assert_eq!(
            (items[0].x, items[0].y, items[0].color),
            (6, subtitle_line_y(0, 2) + 1, 0)
        );
        assert_eq!((items[3].x, items[3].y), (5, subtitle_line_y(1, 2)));
    }

    /// HD: each pixel of the 8x8 font a sharp block at the output's
    /// scale, where the 640x480 pixel is.
    #[test]
    fn bitmap_hd_draws_blocks() {
        let mut data = vec![0u8; BITMAP_FONT_SIZE];
        data[usize::from(b'A') * 64] = 1; // glyph 'A': its top-left pixel
        let f = std::sync::Arc::new(BitmapFont::new(&data).unwrap());
        assert!(BitmapFont::new(&data[1..]).is_none());
        let (w, h) = (1920, 1440);
        let mut hd = vec![0u32; w * h];
        f.draw_hd(&mut hd, w, (0, 0, w, h), b" A", 5, 100, 0xffffff);
        let lit: Vec<usize> = (0..w * h).filter(|&i| hd[i] != 0).collect();
        assert_eq!(lit.len(), 9);
        assert_eq!((lit[0] % w, lit[0] / w), (13 * 3, 100 * 3));
        let s = Subtitler::Bitmap(f);
        let items = s.items(&[b" A".to_vec()], 0, 0);
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].x, items[0].y), (6, bitmap_line_y(0, 1) + 1));
        assert!(items[1].bitmap.is_some());
    }

    #[test]
    fn width_limit_stops_the_line() {
        let f = SubtitleFont::new();
        let long = vec![b'W'; 100];
        let mut dst = vec![0u16; 640 * 20];
        f.draw(&mut dst, 640, 20, &long, 5, 0, SUBTITLE_WIDTH, 0xffff);
        for y in 0..20 {
            for x in 637..640 {
                assert_eq!(dst[y * 640 + x], 0);
            }
        }
    }
}

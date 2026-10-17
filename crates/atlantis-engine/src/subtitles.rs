//! The dialog subtitles: the line breaks of `FUN_0042ffe8`, the layout of
//! `FUN_00430178` and the font.
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

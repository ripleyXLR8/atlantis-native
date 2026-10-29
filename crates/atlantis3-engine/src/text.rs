//! Text on screen: the dialog subtitles, the cinematics' captions and cue
//! lines, the `PLAYVOICE` lines — every one drawn by the flip
//! (@ 0x0040fdf0) with GDI on the back buffer through the linked *Cryo
//! Text Library 2.57*: a layout (four margins and two alignments,
//! `AtlFont_SetLayout` @ 0x0041e190) and a string (`AtlFont_Print`
//! @ 0x0041e2c0) broken into lines inside the margins (@ 0x006b4a20, with
//! a French rule: no break before `?`, `!` or `:`), stacked from the
//! bottom, each line drawn with `ExtTextOutA` and an advance per byte from
//! `GetCharABCWidthsA`.
//!
//! The fonts are `font\font.txt`'s GDI `Arial` (`font\Atlantis.ttf` is
//! Arial 2.76, registered by the game) at heights `ftol(height × f)`, f
//! the float 0.9 / 1.0 / 1.1 for 640 / 800 / 1024 pixels wide. The widths
//! GDI gives for them (`tmHeight`, `tmAscent`, `tmAveCharWidth`, the ABC
//! widths of the 256 bytes) are the table measured with GDI on the game's
//! font (`data/arial_gdi.txt`); the glyphs are drawn here from the game's
//! font file, antialiased (`ANTIALIASED_QUALITY`; GDI's own grey levels
//! are not reproduced).

use std::collections::HashMap;
use std::sync::OnceLock;

/// GDI's measurements of `Arial` at one requested height.
#[derive(Clone, Debug)]
pub struct GdiFont {
    pub height: i32,
    /// `tmHeight`, the line step.
    pub tm_height: i32,
    pub ascent: i32,
    pub ave_char_width: i32,
    /// A, B, C of each byte.
    pub abc: Vec<(i32, i32, i32)>,
}

const TABLE: &str = include_str!("data/arial_gdi.txt");

fn table() -> &'static HashMap<i32, GdiFont> {
    static T: OnceLock<HashMap<i32, GdiFont>> = OnceLock::new();
    T.get_or_init(|| {
        let mut out = HashMap::new();
        let mut cur: Option<GdiFont> = None;
        for l in TABLE.lines() {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w.first() {
                Some(&"FONT") => {
                    if let Some(f) = cur.take() {
                        out.insert(f.height, f);
                    }
                    let tm = w.iter().position(|x| *x == "tm").unwrap_or(0);
                    let n = |i: usize| w.get(i).and_then(|v| v.parse().ok()).unwrap_or(0);
                    cur = Some(GdiFont {
                        height: n(2),
                        tm_height: n(tm + 1),
                        ascent: n(tm + 2),
                        ave_char_width: n(tm + 6),
                        abc: vec![(0, 0, 0); 256],
                    });
                }
                Some(&"ABC") => {
                    if let Some(f) = cur.as_mut() {
                        let n = |i: usize| w.get(i).and_then(|v| v.parse().ok()).unwrap_or(0);
                        let b: usize = n(1) as usize;
                        if b < 256 {
                            f.abc[b] = (n(2), n(3), n(4));
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(f) = cur.take() {
            out.insert(f.height, f);
        }
        out
    })
}

impl GdiFont {
    /// GDI's `Arial` at a requested height, when measured.
    pub fn arial(height: i32) -> Option<&'static GdiFont> {
        table().get(&height)
    }

    /// The advances `ExtTextOutA` is given (@ 0x006b6880): A + B + C per
    /// byte, the last byte's negative C and the first byte's positive A
    /// taken off.
    pub fn dx(&self, s: &[u8]) -> Vec<i32> {
        let n = s.len();
        s.iter()
            .enumerate()
            .map(|(i, &c)| {
                let (a, b, cc) = self.abc[usize::from(c)];
                let mut d = a + b + cc;
                if i + 1 == n && cc < 0 {
                    d -= cc;
                }
                if i == 0 && a > 0 {
                    d -= a;
                }
                d
            })
            .collect()
    }

    pub fn width(&self, s: &[u8]) -> i32 {
        self.dx(s).iter().sum()
    }
}

/// The text ids of `font.txt` the code asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontId {
    Default,
    Bulle,
    Special,
    Titre,
    Credit,
}

impl FontId {
    pub fn name(self) -> &'static str {
        match self {
            FontId::Default => "AtlFont_Default",
            FontId::Bulle => "AtlFont_Bulle",
            FontId::Special => "AtlFont_Special",
            FontId::Titre => "AtlFont_Titre",
            FontId::Credit => "AtlFont_Credit",
        }
    }
}

/// A layout (`AtlFont_SetLayout` @ 0x0041e190): the margins and the
/// alignments (horizontal 0 left, 1 right, 2 centre; vertical 0 top, 1
/// bottom, 2 middle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    pub ml: i32,
    pub mr: i32,
    pub mt: i32,
    pub mb: i32,
    pub halign: i32,
    pub valign: i32,
}

impl Zone {
    pub const fn new(ml: i32, mr: i32, mt: i32, mb: i32, halign: i32, valign: i32) -> Zone {
        Zone {
            ml,
            mr,
            mt,
            mb,
            halign,
            valign,
        }
    }
}

/// The dialog line's layouts (`CDialogue` draw @ 0x0046a0e0): the black
/// shadow, then the white text.
pub const DIALOG_SHADOW: Zone = Zone::new(9, 11, 21, 19, 0, 1);
pub const DIALOG_TEXT: Zone = Zone::new(10, 10, 20, 20, 0, 1);
/// A caption (`CineNNN.txt`, @ 0x0040de60, flag 1): centred, no shadow.
pub const CAPTION: Zone = Zone::new(10, 10, 35, 25, 2, 1);
/// A cue line of `hnm\cineNNN.txt` (flag 0) and the voice slot k (bottom
/// margin + 15 k): shadow, then text.
pub const CUE_SHADOW: Zone = Zone::new(9, 11, 36, 4, 0, 1);
pub const CUE_TEXT: Zone = Zone::new(10, 10, 35, 5, 0, 1);

/// One line drawn: `ExtTextOutA(x, y, bytes, dx)`, (x, y) the top-left of
/// the text cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub x: i32,
    pub y: i32,
    pub bytes: Vec<u8>,
    pub dx: Vec<i32>,
}

fn is_blank(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

/// Where to break a paragraph overflowing at `e` (@ 0x006b7920): the
/// start of the last run of blanks before it, and that run's length;
/// the run just before a `?`, `!` or `:` (the byte at the overflow, past
/// blanks) is not a break (@ 0x006b78d0). (0, 0) without a break.
fn break_before(p: &[u8], e: usize) -> (usize, usize) {
    if e == 0 {
        return (0, 0);
    }
    let mut i = e - 1;
    let mut j = i;
    while j < p.len() && is_blank(p[j]) {
        j += 1;
    }
    let mut fr = j < p.len() && matches!(p[j], b'?' | b'!' | b':');
    loop {
        if is_blank(p[i]) {
            let top = i;
            while i > 0 && is_blank(p[i - 1]) {
                i -= 1;
            }
            if !fr {
                return (i, top - i + 1);
            }
            fr = false;
        }
        if i == 0 {
            return (0, 0);
        }
        i -= 1;
    }
}

/// The lines of `text` (Windows-1252 bytes) for a `w` x `h` screen
/// (@ 0x006b4450, @ 0x006b51b0, @ 0x006b5370).
pub fn layout(font: &GdiFont, text: &[u8], w: i32, h: i32, z: Zone) -> Vec<Line> {
    let avail = (w - 1) - z.mr - z.ml;
    let limit = w - 1 - z.mr;
    let over = |s: &[u8]| z.ml + font.width(s) > limit;
    let mut lines: Vec<Vec<u8>> = Vec::new();
    for p in text.split(|&c| c == b'\n') {
        let mut s = 0usize;
        loop {
            let chars = if font.ave_char_width > 0 {
                (avail / font.ave_char_width).max(0) as usize
            } else {
                0
            };
            let mut e = s + chars.min(p.len() - s);
            let overflow;
            if over(&p[s..e]) {
                loop {
                    e -= 1;
                    if !over(&p[s..e]) {
                        break;
                    }
                }
                e += 1;
                overflow = true;
            } else {
                loop {
                    if e == p.len() {
                        overflow = false;
                        break;
                    }
                    e += 1;
                    if over(&p[s..e]) {
                        overflow = true;
                        break;
                    }
                }
            }
            if !overflow {
                lines.push(p[s..e].to_vec());
                break;
            }
            let (b, n) = break_before(p, e);
            if b <= s {
                // The original never returns (a word wider than the zone;
                // no shipped text has one): the port cuts the word.
                let cut = (e - 1).max(s + 1);
                lines.push(p[s..cut].to_vec());
                s = cut;
                continue;
            }
            lines.push(p[s..b].to_vec());
            s = b + n;
            while s < p.len() && p[s] == b' ' {
                s += 1;
            }
        }
    }
    let th = font.tm_height;
    let n = lines.len() as i32;
    let zone = (h - 1) - z.mb - z.mt;
    let y0 = match z.valign {
        1 => zone - n * th,
        2 => (zone - n * th) / 2,
        _ => 0,
    };
    lines
        .into_iter()
        .enumerate()
        .filter_map(|(j, bytes)| {
            let y = z.mt + y0 + j as i32 * th;
            if y >= (h - 1) - z.mb {
                return None;
            }
            let dx = font.dx(&bytes);
            let wd: i32 = dx.iter().sum();
            let x = match z.halign {
                1 => z.ml + avail - wd,
                2 => z.ml + (avail - wd) / 2,
                _ => z.ml,
            };
            Some(Line { x, y, bytes, dx })
        })
        .collect()
}

/// Draws lines of a font into an RGB565 screen: each glyph at its pen
/// position, the baseline `tmAscent` below the line's y, its coverage
/// blended in `rgb` (0xRRGGBB).
pub struct Glyphs {
    font: ab_glyph::FontVec,
}

impl Glyphs {
    /// The game's `font\Atlantis.ttf` (Arial).
    pub fn new(ttf: Vec<u8>) -> Option<Glyphs> {
        ab_glyph::FontVec::try_from_vec(ttf)
            .ok()
            .map(|font| Glyphs { font })
    }

    pub fn draw(
        &self,
        gdi: &GdiFont,
        lines: &[Line],
        rgb: u32,
        out: &mut [u16],
        w: usize,
        h: usize,
    ) {
        use ab_glyph::{Font, ScaleFont};
        let scale = ab_glyph::PxScale::from(gdi.tm_height as f32);
        let sf = self.font.as_scaled(scale);
        let (r, g, b) = ((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
        for l in lines {
            let mut pen = l.x as f32;
            let base = (l.y + gdi.ascent) as f32;
            for (&c, &d) in l.bytes.iter().zip(&l.dx) {
                let ch = cp1252(c);
                let id = sf.glyph_id(ch);
                let glyph = id.with_scale_and_position(scale, ab_glyph::point(pen, base));
                if let Some(og) = self.font.outline_glyph(glyph) {
                    let bb = og.px_bounds();
                    og.draw(|gx, gy, cov| {
                        let x = bb.min.x as i32 + gx as i32;
                        let y = bb.min.y as i32 + gy as i32;
                        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 || cov <= 0.0 {
                            return;
                        }
                        let px = &mut out[y as usize * w + x as usize];
                        let a = cov.min(1.0);
                        let (dr, dg, db) = (
                            u32::from(*px >> 11) << 3,
                            u32::from((*px >> 5) & 0x3f) << 2,
                            u32::from(*px & 0x1f) << 3,
                        );
                        let mix = |s: u32, d: u32| (s as f32 * a + d as f32 * (1.0 - a)) as u32;
                        let (nr, ng, nb) = (mix(r, dr), mix(g, dg), mix(b, db));
                        *px = ((nr >> 3) << 11 | (ng >> 2) << 5 | nb >> 3) as u16;
                    });
                }
                pen += d as f32;
            }
        }
    }
}

/// A Windows-1252 byte as a character.
pub fn cp1252(c: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160, 0x2039,
        0x152, 0x8d, 0x17d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
        0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
    ];
    if (0x80..0xa0).contains(&c) {
        char::from_u32(u32::from(HIGH[usize::from(c - 0x80)])).unwrap_or('?')
    } else {
        char::from(c)
    }
}

/// A character as a Windows-1252 byte (`?` when it has none).
pub fn to_cp1252(ch: char) -> u8 {
    let u = ch as u32;
    if u < 0x80 || (0xa0..0x100).contains(&u) {
        return u as u8;
    }
    (0x80u8..0xa0).find(|&b| cp1252(b) == ch).unwrap_or(b'?')
}

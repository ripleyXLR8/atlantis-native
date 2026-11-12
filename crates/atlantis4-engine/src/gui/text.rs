//! Text on the GUI layer: `TESF` fonts, the markup, the line builder, the
//! block layout and the glyph quads, as atlantis4-gog-en-2363910's
//! `TeTextBase2` does them.
//!
//! - **Markup** (the walker @ 0x004beab0 that every layout text uses): the
//!   text is wrapped in `<document>…</document>`; text nodes append their
//!   bytes; `<br/>` records a break at the current offset (several at one
//!   offset count several times); `<font file/>`, `<color r g b [a]/>`
//!   (alpha 255 by default) and `<section style/>` (`left` 0, `right` 1,
//!   `justify` 2, `center` 3) take effect from the current offset.
//! - **Line builder** (@ 0x004a1840, one call per line; transcribed below
//!   in [`build_line`]): a glyph whose ink would end past the box's
//!   integer width (`f2 + w + f0 + kern > W`, the equal case fits) ends the
//!   line at the last space, tab, CR or LF, or, when the line has none,
//!   after the whole word (a word longer than the box overflows it); a
//!   `<br/>` ends the line at its offset and returns **the number of
//!   `<br/>` there**; the other exits return 1 (0 when a cut word ends
//!   the text).
//! - **Block** (@ 0x004a3270): the first baseline is at the ascent of the
//!   font at offset 0; each next baseline is `(previous return) × (line
//!   height)` lower, so `a<br/><br/>b` leaves an empty line; the block is
//!   `ceil(max width)` wide and `ceil(interLine + last baseline +
//!   descent)` high.
//! - **Quads** (line drawer @ 0x004a2020): leading spaces skipped; the
//!   pen starts at the alignment offset (`(W − L) / 2` centred, `W − L`
//!   right); kerning of the previous drawn byte (0 at the line's start)
//!   moves the pen; corners `floor(pen.x + f2)`, `floor(pen.x + f2 +
//!   f0)`, `floor(pen.y − f3)`, `floor(pen.y − f3 + f1)`; texture from
//!   `(f8, f9)` with `f9` the glyph's bottom edge counted from the atlas
//!   bottom; `GL_LINEAR` and `GL_BLEND`: the colour is the vertices', the
//!   atlas gives the coverage.
//!
//! Every intermediate stored as a `float` is rounded to `f32` here at the
//! same place; the sums between stores are done in `f64` (the x87 at 53
//! bits).

use std::collections::HashMap;
use std::rc::Rc;

use atlantis4_data::font::Font as TefFont;
use atlantis4_data::image::decode_png;
use atlantis4_data::A4Files;

/// `TeTextBase2`'s alignment (`+0x288`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Right,
    Justify,
    Center,
}

/// A font ready to draw: its metrics and its atlas's coverage.
#[derive(Debug)]
pub struct Font {
    /// Header `+0x54`, `+0x58`, `+0x5c`.
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
    pub glyphs: Vec<atlantis4_data::font::Glyph>,
    /// Kerning pairs `(c1, c2) → (dx, dy)` (the first of a repeated pair,
    /// as a `std::map` insert keeps it).
    kerning: HashMap<(u32, u32), (f32, f32)>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    /// The atlas's alpha, rows top-down.
    pub alpha: Vec<u8>,
}

impl Font {
    /// A `.tef` file read whole.
    pub fn parse(data: &[u8]) -> Option<Font> {
        let f = TefFont::parse(data).ok()?;
        let img = decode_png(f.atlas(data)).ok()?;
        let n = (img.width * img.height) as usize;
        let alpha = match img.channels {
            4 => img.pixels.iter().skip(3).step_by(4).copied().collect(),
            2 => img.pixels.iter().skip(1).step_by(2).copied().collect(),
            _ => vec![255; n],
        };
        let mut kerning = HashMap::new();
        for k in &f.kerning {
            kerning
                .entry((k.left, k.right))
                .or_insert((k.offset[0], k.offset[1]));
        }
        Some(Font {
            ascent: f.metrics[0],
            descent: f.metrics[1],
            line_height: f.metrics[2],
            glyphs: f.glyphs,
            kerning,
            atlas_width: img.width,
            atlas_height: img.height,
            alpha,
        })
    }

    fn glyph(&self, c: u8) -> Glyph {
        self.glyphs
            .get(usize::from(c))
            .map(|g| Glyph {
                f0: g.size[0],
                f1: g.size[1],
                f2: g.offset[0],
                f3: g.offset[1],
                f4: g.advance,
                f8: g.atlas[0],
                f9: g.atlas[1],
            })
            .unwrap_or_default()
    }

    /// The kerning of the pair (`a`, `b`), zero when absent.
    pub fn kern(&self, a: u32, b: u32) -> (f32, f32) {
        self.kerning.get(&(a, b)).copied().unwrap_or((0.0, 0.0))
    }

    /// The atlas's coverage at (`x`, `y`), clamped to its edge.
    fn coverage(&self, x: i64, y: i64) -> u8 {
        if self.atlas_width == 0 || self.atlas_height == 0 {
            return 0;
        }
        let x = x.clamp(0, i64::from(self.atlas_width) - 1) as usize;
        let y = y.clamp(0, i64::from(self.atlas_height) - 1) as usize;
        self.alpha[y * self.atlas_width as usize + x]
    }
}

/// The fields of a glyph record the builder and the drawer read.
#[derive(Clone, Copy, Debug, Default)]
struct Glyph {
    f0: f32,
    f1: f32,
    f2: f32,
    f3: f32,
    f4: f32,
    f8: f32,
    f9: f32,
}

/// Fonts by path, read once.
#[derive(Default)]
pub struct FontCache {
    fonts: HashMap<String, Option<Rc<Font>>>,
}

impl FontCache {
    pub fn get(&mut self, files: &A4Files, path: &str) -> Option<Rc<Font>> {
        self.fonts
            .entry(path.to_string())
            .or_insert_with(|| {
                let f = files.read(path).and_then(|d| Font::parse(&d));
                if f.is_none() {
                    log::warn!("TeFont2::load: can't load {path}");
                }
                f.map(Rc::new)
            })
            .clone()
    }
}

/// A markup string walked: the bytes to draw and what applies where.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Markup {
    pub bytes: Vec<u8>,
    pub align: Align,
    /// Colours from an offset on (`r, g, b, a`).
    pub colours: Vec<(usize, [u8; 4])>,
    /// Fonts from an offset on.
    pub fonts: Vec<(usize, String)>,
    /// `<br/>` offsets, one entry per tag.
    pub breaks: Vec<usize>,
}

impl Markup {
    /// The colour in effect at `i` (opaque black before any `<color/>`,
    /// the line drawer's `0xff000000`).
    pub fn colour_at(&self, i: usize) -> [u8; 4] {
        self.colours
            .iter()
            .rev()
            .find(|(o, _)| *o <= i)
            .map_or([0, 0, 0, 255], |(_, c)| *c)
    }

    fn breaks_at(&self, i: usize) -> u32 {
        self.breaks.iter().filter(|&&b| b == i).count() as u32
    }
}

fn entity(name: &[u8]) -> Option<Vec<u8>> {
    Some(match name {
        b"amp" => b"&".to_vec(),
        b"lt" => b"<".to_vec(),
        b"gt" => b">".to_vec(),
        b"quot" => b"\"".to_vec(),
        b"apos" => b"'".to_vec(),
        _ => {
            let n = name.strip_prefix(b"#")?;
            let v = match n.strip_prefix(b"x") {
                Some(h) => u32::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()?,
                None => std::str::from_utf8(n).ok()?.parse().ok()?,
            };
            vec![u8::try_from(v).ok()?]
        }
    })
}

fn unescape(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'&' {
            if let Some(end) = s[i..].iter().position(|&c| c == b';') {
                if let Some(e) = entity(&s[i + 1..i + end]) {
                    out.extend(e);
                    i += end + 1;
                    continue;
                }
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

/// The attributes of a tag's inside (`name a="v" …`).
/// A tag's attributes: names and unescaped values.
type Attributes = Vec<(Vec<u8>, Vec<u8>)>;

fn attributes(s: &[u8]) -> (Vec<u8>, Attributes) {
    let mut i = 0;
    let name_end = s
        .iter()
        .position(|c| c.is_ascii_whitespace() || *c == b'/')
        .unwrap_or(s.len());
    let name = s[..name_end].to_ascii_lowercase();
    i += name_end;
    let mut attrs = Vec::new();
    loop {
        while i < s.len() && (s[i].is_ascii_whitespace() || s[i] == b'/') {
            i += 1;
        }
        let Some(eq) = s[i..].iter().position(|&c| c == b'=') else {
            break;
        };
        let key = s[i..i + eq].trim_ascii().to_vec();
        i += eq + 1;
        while i < s.len() && s[i].is_ascii_whitespace() {
            i += 1;
        }
        let Some(&q) = s.get(i) else { break };
        if q != b'"' && q != b'\'' {
            break;
        }
        let Some(end) = s[i + 1..].iter().position(|&c| c == q) else {
            break;
        };
        attrs.push((key, unescape(&s[i + 1..i + 1 + end])));
        i += end + 2;
    }
    (name, attrs)
}

/// `sscanf("%d")` of an attribute's value (0 when it is not a number).
fn atoi(v: &[u8]) -> i64 {
    let s = std::str::from_utf8(v).unwrap_or("").trim_start();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let n: i64 = s.bytes().take_while(u8::is_ascii_digit).fold(0i64, |a, d| {
        a.saturating_mul(10).saturating_add(i64::from(d - b'0'))
    });
    if neg {
        -n
    } else {
        n
    }
}

/// The walker @ 0x004beab0 over `markup` (the text without its
/// `<document>` wrapper). Text nodes are kept byte for byte: whether the
/// engine's XML reader condenses white space is `[UNKNOWN]` (no line of
/// the English table starts or ends with a space; one has a double space).
pub fn parse_markup(markup: &[u8]) -> Markup {
    let mut m = Markup::default();
    let mut i = 0;
    while i < markup.len() {
        if markup[i] == b'<' {
            let Some(end) = markup[i..].iter().position(|&c| c == b'>') else {
                break;
            };
            let inside = &markup[i + 1..i + end];
            i += end + 1;
            if inside.starts_with(b"/") || inside.starts_with(b"?") || inside.starts_with(b"!") {
                continue;
            }
            let (name, attrs) = attributes(inside);
            let get = |k: &[u8]| {
                attrs
                    .iter()
                    .find(|(a, _)| a.as_slice() == k)
                    .map(|(_, v)| v.as_slice())
            };
            let at = m.bytes.len();
            match name.as_slice() {
                b"br" => m.breaks.push(at),
                b"font" => {
                    if let Some(f) = get(b"file") {
                        m.fonts.push((at, String::from_utf8_lossy(f).into_owned()));
                    }
                }
                b"color" => {
                    let c = |k: &[u8], d: i64| get(k).map_or(d, atoi) as u8;
                    m.colours
                        .push((at, [c(b"r", 0), c(b"g", 0), c(b"b", 0), c(b"a", 255)]));
                }
                b"section" => {
                    m.align = match get(b"style") {
                        Some(b"right") => Align::Right,
                        Some(b"justify") => Align::Justify,
                        Some(b"center") => Align::Center,
                        _ => Align::Left,
                    };
                }
                _ => {}
            }
        } else {
            let end = markup[i..]
                .iter()
                .position(|&c| c == b'<')
                .map_or(markup.len(), |e| i + e);
            m.bytes.extend(unescape(&markup[i..end]));
            i = end;
        }
    }
    m
}

/// `sprintf(textFormat, line)` (@ 0x005067f0 called as `(dest, format,
/// line)`): the line is the argument, so a `%` in it is copied as is.
/// Only `%s` and `%%` occur in the data's formats.
pub fn format(text_format: &[u8], line: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text_format.len() {
        match (text_format[i], text_format.get(i + 1)) {
            (b'%', Some(b's')) => {
                out.extend_from_slice(line);
                i += 2;
            }
            (b'%', Some(b'%')) => {
                out.push(b'%');
                i += 2;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

#[inline]
fn f(x: f64) -> f32 {
    x as f32
}

#[inline]
fn d(x: f32) -> f64 {
    f64::from(x)
}

/// One line as the builder leaves its record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Line {
    pub start: usize,
    /// The line's end (exclusive): `rec+0`.
    pub end: usize,
    /// Where the next line starts: `rec+4`.
    pub next: usize,
    /// The tallest glyph: `rec+8`.
    pub glyph_height: f32,
    /// The line height, `font+0x5c + interLine` (`rec+0x10`; the record is
    /// reused from line to line, so it only grows).
    pub height: f32,
    /// The line's width `L`: `rec+0x18`.
    pub width: f32,
    /// The builder's return: the number of `<br/>` that ended the line, 1
    /// for the other ends, 0 for a cut word that ends the text.
    pub ret: u32,
    /// The line's baseline (the block's pen y when it is drawn).
    pub baseline: f32,
}

/// What a text is laid out in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The box's width as the builder reads it (`TeTextBase2+8`, the
    /// layout's width through `_ftol`).
    pub width: i32,
    /// `wrapMode = SingleLine` (`+0x2b8`): no fit test.
    pub single_line: bool,
    /// `interLine` (`+0x2c`).
    pub inter_line: f32,
}

/// The line builder @ 0x004a1840 from `start`, `prev` the record of the
/// previous line (its height carries over).
fn build_line(font: &Font, m: &Markup, start: usize, frame: Frame, prev_height: f32) -> Line {
    let b = &m.bytes;
    let len = b.len();
    let byte = |i: usize| b.get(i).copied().unwrap_or(0);
    let mut out = Line {
        start,
        end: start,
        next: start,
        height: prev_height,
        ..Line::default()
    };
    let mut w = 0f32;
    let mut i = start;
    while i < len {
        if i != start {
            let n = m.breaks_at(i);
            if n > 0 {
                out.end = i;
                out.next = i;
                out.width = f(d(w) - d(font.glyph(byte(i.wrapping_sub(1))).f2));
                out.ret = n;
                return out;
            }
        }
        let c = byte(i);
        let g = font.glyph(c);
        if !frame.single_line && i != 0 && m.breaks_at(i) == 0 {
            let k = font.kern(u32::from(byte(i - 1)), u32::from(c)).0;
            let sum = d(g.f2) + d(w) + d(g.f0) + d(k);
            if sum > f64::from(frame.width) {
                return wrap(font, m, start, i, w, out);
            }
        }
        if is_space(c) {
            out.end = i;
            out.width = f(d(w) - d(font.glyph(byte(i.wrapping_sub(1))).f2));
        }
        let k = if i > 0 {
            font.kern(u32::from(byte(i - 1)), u32::from(c)).0
        } else {
            font.kern(0, u32::from(c)).0
        };
        w = f(d(k) + d(w));
        if out.glyph_height < g.f1 {
            out.glyph_height = g.f1;
        }
        let h = f(d(font.line_height) + d(frame.inter_line));
        if out.height < h {
            out.height = h;
        }
        w = f(d(g.f4) + d(w));
        i += 1;
    }
    out.end = len;
    out.next = len;
    out.width = if i != 0 {
        f(d(w) - d(font.glyph(byte(i - 1)).f2))
    } else {
        w
    };
    out.ret = 1;
    out
}

/// The builder's exit for a glyph that does not fit (@ 0x004a1dfd).
fn wrap(font: &Font, m: &Markup, start: usize, i: usize, mut w: f32, mut out: Line) -> Line {
    let b = &m.bytes;
    let len = b.len();
    let byte = |i: usize| b.get(i).copied().unwrap_or(0);
    let mut at = i;
    if out.end == start {
        // No break candidate: the whole word stays on the line
        // (@ 0x004a15e0: up to a `<br/>`, a space or the end).
        let mut e = i;
        while e < len && m.breaks_at(e) == 0 && !is_space(byte(e)) {
            e += 1;
        }
        out.end = e;
        while at < e {
            w = f(d(font.glyph(byte(at)).f4) + d(w));
            out.width = f(d(w) - d(font.glyph(byte(at.wrapping_sub(1))).f2));
            at += 1;
        }
    }
    // The next line starts after the spaces (@ 0x004a16c0; spaces that
    // run to the end leave the last one, an offset past the end gives
    // the one before it).
    let mut p = out.end;
    out.next = loop {
        if p >= len {
            break p.saturating_sub(1);
        }
        if !is_space(byte(p)) {
            break p;
        }
        p += 1;
    };
    if at == len {
        out.next = at;
        out.ret = 0;
    } else {
        out.ret = 1;
    }
    out
}

/// A text laid out: its lines and its block size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    pub markup: Markup,
    pub lines: Vec<Line>,
    /// `ceil(max L)` (`+0x10`).
    pub width: i32,
    /// `ceil(interLine + last baseline + descent)` (`+0x14`).
    pub height: i32,
    /// The box width it was built for.
    pub frame_width: i32,
}

/// `TeTextBase2::build` @ 0x004a3270 for `m` in `frame`.
pub fn build(font: &Font, m: Markup, frame: Frame) -> Block {
    let len = m.bytes.len();
    let mut lines = Vec::new();
    let mut y = font.ascent;
    let mut ret_prev = 0u32;
    let mut height = 0f32;
    let mut width = 0i32;
    let mut start = 0;
    while start < len {
        let mut l = build_line(font, &m, start, frame, height);
        height = l.height;
        let adv = f(f64::from(ret_prev) * d(height));
        y = f(d(y) + d(adv));
        l.baseline = y;
        if (width as f32) < l.width {
            width = d(l.width).ceil() as f32 as i32;
        }
        ret_prev = l.ret;
        // The original loops for ever on a line that does not move on (a
        // `<br/>` under a word cut at the line's start); never in the data.
        start = l.next.max(start + 1);
        lines.push(l);
    }
    let h = f(d(frame.inter_line) + d(y) + d(font.descent));
    Block {
        markup: m,
        lines,
        width,
        height: d(h).ceil() as f32 as i32,
        frame_width: frame.width,
    }
}

/// A glyph's quad in the text's space (origin at the layout's top-left,
/// y down) and where it reads the atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub byte: u8,
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    /// Atlas rectangle: left, bottom edge counted from the atlas bottom,
    /// width, height (`f8, f9, f0, f1`).
    pub atlas: [f32; 4],
    pub colour: [u8; 4],
}

/// The quads of one line (@ 0x004a2020), pen at (0, `line.baseline`).
pub fn line_quads(font: &Font, block: &Block, line: &Line) -> Vec<Quad> {
    let m = &block.markup;
    let b = &m.bytes;
    let byte = |i: usize| b.get(i).copied().unwrap_or(0);
    let w = block.frame_width as f32;
    let mut x = 0f32;
    let mut y = line.baseline;
    match m.align {
        Align::Center => x = f(d(x) + (d(w) - d(line.width)) * 0.5),
        Align::Right => x = f(d(w) - d(line.width) + d(x)),
        // Justify spreads `W − L` over the spaces (@ 0x004a1760); no
        // text of the data asks for it, so it is drawn as left
        // [UNKNOWN: the spread's rounding].
        Align::Left | Align::Justify => {}
    }
    let mut i = line.start;
    while i < line.end && is_space(byte(i)) {
        i += 1;
    }
    let mut prev = 0u32;
    let mut out = Vec::new();
    while i < line.end {
        let c = byte(i);
        let (dx, dy) = font.kern(prev, u32::from(c));
        x = f(d(x) + d(dx));
        y = f(d(y) + d(dy));
        let g = font.glyph(c);
        let gx = f(d(x) + d(g.f2));
        let t = f(d(y) - d(g.f3));
        let bot = f(d(t) + d(g.f1));
        let gx1 = f(d(gx) + d(g.f0));
        out.push(Quad {
            byte: c,
            x0: gx.floor() as i32,
            y0: t.floor() as i32,
            x1: gx1.floor() as i32,
            y1: bot.floor() as i32,
            atlas: [g.f8, g.f9, g.f0, g.f1],
            colour: m.colour_at(i),
        });
        x = f(d(x) + d(g.f4));
        prev = u32::from(c);
        i += 1;
    }
    out
}

/// A clip rectangle in window pixels: columns `x0..x1`, rows `y0..y1`.
pub type Clip = (i64, i64, i64, i64);

/// Draws `block` with its top-left at the window pixel (`ox`, `oy`) into
/// `out` (`width × height` XRGB): each quad's pixels take the vertex
/// colour (the markup's times the layout's `tint`) with the atlas's coverage (`GL_BLEND` texture environment,
/// `GL_LINEAR`), blended over the window.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    out: &mut [u32],
    width: u32,
    height: u32,
    font: &Font,
    block: &Block,
    ox: i64,
    oy: i64,
    clip: Option<Clip>,
    tint: [u8; 4],
) {
    let (cx0, cx1, cy0, cy1) = clip.unwrap_or((0, i64::from(width), 0, i64::from(height)));
    let ah = f64::from(font.atlas_height);
    for line in &block.lines {
        for mut q in line_quads(font, block, line) {
            q.colour = super::modulate(q.colour, tint);
            let (qw, qh) = (q.x1 - q.x0, q.y1 - q.y0);
            if qw <= 0 || qh <= 0 {
                continue;
            }
            let [f8, f9, f0, f1] = q.atlas.map(f64::from);
            let (sx, sy) = (f0 / f64::from(qw), f1 / f64::from(qh));
            // The atlas row (from its top) of the quad's top edge.
            let top = ah - (f9 + f1);
            let i0 = (ox + i64::from(q.x0)).max(cx0).max(0);
            let i1 = (ox + i64::from(q.x1)).min(cx1).min(i64::from(width));
            let j0 = (oy + i64::from(q.y0)).max(cy0).max(0);
            let j1 = (oy + i64::from(q.y1)).min(cy1).min(i64::from(height));
            for j in j0..j1 {
                let ty = top + ((j - oy - i64::from(q.y0)) as f64 + 0.5) * sy - 0.5;
                let (fy, b) = (ty.floor(), ty - ty.floor());
                for i in i0..i1 {
                    let tx = f8 + ((i - ox - i64::from(q.x0)) as f64 + 0.5) * sx - 0.5;
                    let (fx, a) = (tx.floor(), tx - tx.floor());
                    let (x0, y0) = (fx as i64, fy as i64);
                    let cov = f64::from(font.coverage(x0, y0)) * (1.0 - a) * (1.0 - b)
                        + f64::from(font.coverage(x0 + 1, y0)) * a * (1.0 - b)
                        + f64::from(font.coverage(x0, y0 + 1)) * (1.0 - a) * b
                        + f64::from(font.coverage(x0 + 1, y0 + 1)) * a * b;
                    let cov = (cov + 0.5).floor().clamp(0.0, 255.0);
                    let alpha = (f64::from(q.colour[3]) * cov / 255.0 + 0.5).floor() as u8;
                    if alpha == 0 {
                        continue;
                    }
                    let px = &mut out[j as usize * width as usize + i as usize];
                    *px = crate::sprite::blend(*px, [q.colour[0], q.colour[1], q.colour[2], alpha]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font of fixed-width glyphs: every glyph 8 wide, 10 high, advance
    /// 10, bearing 1, top 10; ascent 10, descent 3, line 14; one kerning
    /// pair (`A`, `V`) of −2.
    fn mono() -> Font {
        let g = atlantis4_data::font::Glyph {
            size: [8.0, 10.0],
            offset: [1.0, 10.0],
            advance: 10.0,
            unknown: [0.0; 3],
            atlas: [0.0, 0.0],
            code: 0,
        };
        let mut kerning = HashMap::new();
        kerning.insert((u32::from(b'A'), u32::from(b'V')), (-2.0, 0.0));
        Font {
            ascent: 10.0,
            descent: 3.0,
            line_height: 14.0,
            glyphs: vec![g; 256],
            kerning,
            atlas_width: 16,
            atlas_height: 16,
            alpha: vec![255; 256],
        }
    }

    fn frame(width: i32) -> Frame {
        Frame {
            width,
            single_line: false,
            inter_line: 0.0,
        }
    }

    #[test]
    fn markup_gives_bytes_colours_alignment_and_breaks() {
        let m = parse_markup(
            b"<section style=\"center\" /><color r=\"200\" g=\"200\" b=\"200\"/><font file=\"F.tef\" />a&amp;b<br/><br/>c",
        );
        assert_eq!(m.bytes, b"a&bc");
        assert_eq!(m.align, Align::Center);
        assert_eq!(m.colours, [(0, [200, 200, 200, 255])]);
        assert_eq!(m.fonts, [(0, "F.tef".to_string())]);
        assert_eq!(m.breaks, [3, 3]);
        assert_eq!(m.colour_at(0), [200, 200, 200, 255]);
        assert_eq!(parse_markup(b"x").colour_at(0), [0, 0, 0, 255]);
    }

    #[test]
    fn the_line_is_the_argument_of_the_format() {
        assert_eq!(format(b"<b/>%s", b"100% sure"), b"<b/>100% sure");
    }

    #[test]
    fn two_breaks_at_one_offset_leave_an_empty_line() {
        // `a<br/><br/>b`: the first line returns 2, so the second baseline
        // is two line heights below the first (@ 0x004a1d98, 0x004a33ee).
        let font = mono();
        let b = build(&font, parse_markup(b"a<br/><br/>b"), frame(100));
        assert_eq!(b.lines.len(), 2);
        assert_eq!(b.lines[0].ret, 2);
        assert_eq!(b.lines[0].baseline, 10.0);
        assert_eq!(b.lines[1].baseline, 38.0);
        // ceil(0 + 38 + 3).
        assert_eq!(b.height, 41);
        // Width: advance 10 minus the bearing 1.
        assert_eq!(b.width, 9);
    }

    #[test]
    fn lines_wrap_at_the_last_space_and_long_words_overflow() {
        let font = mono();
        // "aa bb": "aa " is 30 wide; `b` at 30 ends its ink at 1 + 30 + 8
        // = 39 > 35: the line ends at the space.
        let b = build(&font, parse_markup(b"aa bb"), frame(35));
        let spans: Vec<_> = b.lines.iter().map(|l| (l.start, l.end, l.next)).collect();
        assert_eq!(spans, [(0, 2, 3), (3, 5, 5)]);
        assert_eq!(b.lines[0].width, 19.0);
        // The equal case fits: 1 + 30 + 8 = 39 ≤ 39.
        assert_eq!(
            build(&font, parse_markup(b"aa b"), frame(39)).lines.len(),
            1
        );
        assert_eq!(
            build(&font, parse_markup(b"aa b"), frame(38)).lines.len(),
            2
        );
        // A word wider than the box stays whole.
        let b = build(&font, parse_markup(b"abcdef"), frame(25));
        assert_eq!(b.lines.len(), 1);
    }

    #[test]
    fn quads_are_floored_and_kerned_from_the_line_start() {
        let font = mono();
        let b = build(&font, parse_markup(b"AV"), frame(100));
        let q = line_quads(&font, &b, &b.lines[0]);
        assert_eq!((q[0].x0, q[0].y0, q[0].x1, q[0].y1), (1, 0, 9, 10));
        // Pen 10, kerning −2: 8 + 1.
        assert_eq!(q[1].x0, 9);
        // Centred in 100: L = 8 + 10 − 1 = 17 (the builder's kerning of
        // byte 0 with the byte before it is none), offset 41.5.
        let mut m = parse_markup(b"AV");
        m.align = Align::Center;
        let b = build(&font, m, frame(100));
        assert_eq!(b.lines[0].width, 17.0);
        let q = line_quads(&font, &b, &b.lines[0]);
        assert_eq!(q[0].x0, 42);
    }
}

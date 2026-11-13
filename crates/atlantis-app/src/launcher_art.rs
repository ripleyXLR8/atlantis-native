//! The game library's look, in the manner of the web page's
//! (`web/index.html`, itself after the game's own start screen): a painted
//! night on parchment, a silvery italic logo with its glow, embossed
//! orange-gold capitals, golden crescent-and-spear ornaments and a sun
//! disc for the pointer. None of the games' pictures are used.
//!
//! The screen is laid out on the 640x480 screen. Its background is a
//! 640x480 RGB565 picture; its text and ornaments are [`Item`]s, drawn
//! from their outlines on a [`Canvas`]: the 640x480 frame itself, or the
//! window at its own resolution ([`Window`]), so that they stay smooth in a
//! large window.

use std::sync::OnceLock;

use ab_glyph::{point, Font, FontRef, OutlinedGlyph, PxScale, ScaleFont};
use atlantis_engine::warp::{SCREEN_H, SCREEN_W};

/// P052, URW's Palatino-style face (the web page's serif).
static ROMAN: &[u8] = include_bytes!("../../../assets/fonts/P052-Roman.otf");
static ITALIC: &[u8] = include_bytes!("../../../assets/fonts/P052-Italic.otf");

/// An 8-bit colour.
pub type Rgb = (u8, u8, u8);

/// The web page's colours.
pub const TEXT: Rgb = (205, 214, 234);
pub const MUTED: Rgb = (127, 143, 181);
pub const EM: Rgb = (245, 211, 140);
pub const LINK: Rgb = (233, 184, 106);
pub const ERROR: Rgb = (255, 154, 138);
pub const GOLD: Rgb = (227, 154, 58);
pub const TABLET: Rgb = (4, 12, 34);

/// The embossed gold of the capitals (top to bottom).
const GOLD_STOPS: [(f32, Rgb); 4] = [
    (0.0, (255, 226, 160)),
    (0.38, (255, 191, 85)),
    (0.55, (227, 154, 58)),
    (1.0, (122, 63, 12)),
];
/// The gold of the ornaments.
const ORNAMENT_STOPS: [(f32, Rgb); 3] = [
    (0.0, (255, 233, 176)),
    (0.45, (232, 165, 72)),
    (1.0, (110, 56, 10)),
];
/// The logo's silver.
const SILVER_STOPS: [(f32, Rgb); 4] = [
    (0.0, (255, 255, 255)),
    (0.45, (230, 238, 255)),
    (0.7, (159, 180, 220)),
    (1.0, (244, 248, 255)),
];
const EMBOSS: Rgb = (42, 18, 0);
const GLOW: Rgb = (150, 190, 255);

/// How a text is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    Solid(Rgb),
    /// The gold of the capitals, embossed (a dark edge below right).
    Gold,
    /// The logo's silver, in its blue glow.
    Silver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Roman,
    Italic,
}

/// A text's face, size (pixels per em on the 640x480 screen), capitals,
/// letter spacing (in ems) and paint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub face: Face,
    pub size: f32,
    pub caps: bool,
    pub tracking: f32,
    pub paint: Paint,
}

impl Style {
    /// Running text.
    pub const fn body(size: f32, color: Rgb) -> Style {
        Style {
            face: Face::Roman,
            size,
            caps: false,
            tracking: 0.0,
            paint: Paint::Solid(color),
        }
    }

    pub const fn italic(size: f32, color: Rgb) -> Style {
        Style {
            face: Face::Italic,
            ..Style::body(size, color)
        }
    }

    /// The game's gold capitals.
    pub const fn caps(size: f32) -> Style {
        Style {
            face: Face::Roman,
            size,
            caps: true,
            tracking: 0.12,
            paint: Paint::Gold,
        }
    }

    /// Capitals in a plain colour (a disabled button, a missing game).
    pub const fn caps_in(size: f32, color: Rgb) -> Style {
        Style {
            paint: Paint::Solid(color),
            ..Style::caps(size)
        }
    }

    /// The silvery logo.
    pub const fn logo(size: f32) -> Style {
        Style {
            face: Face::Italic,
            size,
            caps: false,
            tracking: 0.04,
            paint: Paint::Silver,
        }
    }
}

/// A text or an ornament, in 640x480 screen units.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A text from `x`, on the baseline `baseline`.
    Text {
        style: Style,
        text: String,
        x: f32,
        baseline: f32,
    },
    /// The sun disc of the pointer, centred, `r` its rays' radius.
    Sun { cx: f32, cy: f32, r: f32 },
    /// A golden crescent with a spear through it, `w` wide (a third of it
    /// high) from (x, y), the point to the left (to the right: `flip`).
    Spear { x: f32, y: f32, w: f32, flip: bool },
}

impl Item {
    pub fn text(style: Style, text: &str, x: f32, baseline: f32) -> Item {
        Item::Text {
            style,
            text: text.to_string(),
            x,
            baseline,
        }
    }
}

fn faces() -> &'static (FontRef<'static>, FontRef<'static>) {
    static FACES: OnceLock<(FontRef<'static>, FontRef<'static>)> = OnceLock::new();
    FACES.get_or_init(|| {
        (
            FontRef::try_from_slice(ROMAN).expect("P052 Roman"),
            FontRef::try_from_slice(ITALIC).expect("P052 Italic"),
        )
    })
}

fn font(face: Face) -> &'static FontRef<'static> {
    match face {
        Face::Roman => &faces().0,
        Face::Italic => &faces().1,
    }
}

/// The scale of `font` at `em_px` pixels per em (ab_glyph scales by the
/// line height).
fn px_scale(font: &FontRef<'_>, em_px: f32) -> PxScale {
    let upem = font.units_per_em().unwrap_or(1000.0);
    PxScale::from(em_px * font.height_unscaled() / upem)
}

/// The text as drawn: in capitals for a capitals style, a character the
/// face lacks as '?'.
fn shown(style: &Style, text: &str) -> Vec<char> {
    let f = font(style.face);
    let chars: Vec<char> = if style.caps {
        text.to_uppercase().chars().collect()
    } else {
        text.chars().collect()
    };
    chars
        .into_iter()
        .map(|c| if f.glyph_id(c).0 == 0 { '?' } else { c })
        .collect()
}

/// The pen position of each character of `chars` from 0, and the width.
fn layout(style: &Style, chars: &[char]) -> (Vec<f32>, f32) {
    let f = font(style.face);
    let sf = f.as_scaled(px_scale(f, style.size));
    let track = style.tracking * style.size;
    let mut pen = 0.0;
    let mut out = Vec::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        out.push(pen);
        pen += sf.h_advance(f.glyph_id(c));
        if i + 1 < chars.len() {
            pen += track;
        }
    }
    (out, pen)
}

/// The width of `text` in `style`, in screen units.
pub fn width(style: &Style, text: &str) -> f32 {
    layout(style, &shown(style, text)).1
}

/// The height of capitals above the baseline (for centring).
pub fn cap_height(style: &Style) -> f32 {
    style.size * 0.69
}

/// `text` cut on the right with "…" to fit `max_w` (a label keeps its
/// start).
pub fn fit_right(style: &Style, text: &str, max_w: f32) -> String {
    if width(style, text) <= max_w {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let t: String = chars.iter().collect::<String>().trim_end().to_string() + "\u{2026}";
        if width(style, &t) <= max_w {
            return t;
        }
    }
    String::new()
}

/// `text` cut on the left with "…" to fit `max_w` (a path keeps its end,
/// the game's folder).
pub fn fit_left(style: &Style, text: &str, max_w: f32) -> String {
    if width(style, text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for start in 1..chars.len() {
        let t = format!("\u{2026}{}", chars[start..].iter().collect::<String>());
        if width(style, &t) <= max_w {
            return t;
        }
    }
    String::new()
}

/// Where items are drawn: a picture, in pixels, `scale` pixels to a
/// screen unit from `origin`.
pub trait Canvas {
    /// The pixels per screen unit, across and down.
    fn scale(&self) -> (f32, f32);
    /// The pixel of the screen's top left.
    fn origin(&self) -> (f32, f32);
    /// Blends `c` at `a` (0..1) over pixel (x, y); nothing outside the
    /// screen's pixels.
    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32);
}

fn mix(d: Rgb, c: Rgb, a: f32) -> Rgb {
    let m = |d: u8, c: u8| {
        (d as f32 + (c as f32 - d as f32) * a)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (m(d.0, c.0), m(d.1, c.1), m(d.2, c.2))
}

pub fn rgb565(c: Rgb) -> u16 {
    ((c.0 as u16 >> 3) << 11) | ((c.1 as u16 >> 2) << 5) | (c.2 as u16 >> 3)
}

fn from565(p: u16) -> Rgb {
    let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
    (
        ((r << 3) | (r >> 2)) as u8,
        ((g << 2) | (g >> 4)) as u8,
        ((b << 3) | (b >> 2)) as u8,
    )
}

/// The 640x480 RGB565 frame.
pub struct Frame565<'a>(pub &'a mut [u16]);

impl Canvas for Frame565<'_> {
    fn scale(&self) -> (f32, f32) {
        (1.0, 1.0)
    }
    fn origin(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if x < 0 || y < 0 || x >= SCREEN_W as i32 || y >= SCREEN_H as i32 || a <= 0.0 {
            return;
        }
        let p = &mut self.0[y as usize * SCREEN_W + x as usize];
        *p = rgb565(mix(from565(*p), c, a.min(1.0)));
    }
}

/// The window's XRGB8888 pixels (`stride` a row), the screen mapped to
/// the `dw` x `dh` pixels at (`ox`, `oy`).
pub struct Window<'a> {
    pub px: &'a mut [u32],
    pub stride: usize,
    pub rect: (usize, usize, usize, usize),
}

impl Canvas for Window<'_> {
    fn scale(&self) -> (f32, f32) {
        (
            self.rect.2 as f32 / SCREEN_W as f32,
            self.rect.3 as f32 / SCREEN_H as f32,
        )
    }
    fn origin(&self) -> (f32, f32) {
        (self.rect.0 as f32, self.rect.1 as f32)
    }
    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        let (ox, oy, dw, dh) = self.rect;
        if x < ox as i32 || y < oy as i32 || x >= (ox + dw) as i32 || y >= (oy + dh) as i32 {
            return;
        }
        if a <= 0.0 {
            return;
        }
        let p = &mut self.px[y as usize * self.stride + x as usize];
        let d = ((*p >> 16) as u8, (*p >> 8) as u8, *p as u8);
        let (r, g, b) = mix(d, c, a.min(1.0));
        *p = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
    }
}

fn gradient(stops: &[(f32, Rgb)], t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (w[0], w[1]);
        if t <= t1 {
            return mix(c0, c1, (t - t0) / (t1 - t0).max(1e-6));
        }
    }
    stops[stops.len() - 1].1
}

/// Draws `item` on `canvas`.
pub fn draw(canvas: &mut impl Canvas, item: &Item) {
    match item {
        Item::Text {
            style,
            text,
            x,
            baseline,
        } => draw_text(canvas, style, text, *x, *baseline),
        Item::Sun { cx, cy, r } => draw_sun(canvas, *cx, *cy, *r),
        Item::Spear { x, y, w, flip } => draw_spear(canvas, *x, *y, *w, *flip),
    }
}

fn draw_text(canvas: &mut impl Canvas, style: &Style, text: &str, x: f32, baseline: f32) {
    let chars = shown(style, text);
    if chars.is_empty() {
        return;
    }
    let f = font(style.face);
    let ((sx, sy), (ox, oy)) = (canvas.scale(), canvas.origin());
    let scale = px_scale(f, style.size * sy);
    let (pens, _) = layout(style, &chars);
    let base = oy + baseline * sy;
    let glyphs: Vec<OutlinedGlyph> = chars
        .iter()
        .zip(&pens)
        .filter(|(c, _)| **c > ' ')
        .filter_map(|(&c, &pen)| {
            let pos = point(ox + (x + pen) * sx, base);
            f.outline_glyph(f.glyph_id(c).with_scale_and_position(scale, pos))
        })
        .collect();
    // The gradient runs over the capitals' height (the logo's: its
    // ascent).
    let (top, span) = match style.paint {
        Paint::Silver => {
            let a = f.as_scaled(scale).ascent();
            (base - a * 0.8, a * 0.8)
        }
        _ => {
            let h = cap_height(style) * sy;
            (base - h, h)
        }
    };
    let each = |canvas: &mut dyn FnMut(i32, i32, f32)| {
        for g in &glyphs {
            let b = g.px_bounds();
            let (bx, by) = (b.min.x as i32, b.min.y as i32);
            g.draw(|gx, gy, cov| canvas(bx + gx as i32, by + gy as i32, cov));
        }
    };
    match style.paint {
        Paint::Solid(c) => each(&mut |px, py, cov| canvas.blend(px, py, c, cov)),
        Paint::Gold => {
            // The emboss: the letters' dark edge, a pixel below right.
            let d = (sy * 0.9).round().max(1.0) as i32;
            each(&mut |px, py, cov| canvas.blend(px + d, py + d, EMBOSS, cov * 0.9));
            each(&mut |px, py, cov| {
                let t = (py as f32 - top) / span;
                canvas.blend(px, py, gradient(&GOLD_STOPS, t), cov)
            });
        }
        Paint::Silver => {
            glow(canvas, &glyphs, (10.0 * sy).round().max(2.0) as i32);
            each(&mut |px, py, cov| {
                let t = (py as f32 - top) / span;
                canvas.blend(px, py, gradient(&SILVER_STOPS, t), cov)
            });
        }
    }
}

/// The logo's blue glow: its letters' coverage blurred over `r` pixels.
fn glow(canvas: &mut impl Canvas, glyphs: &[OutlinedGlyph], r: i32) {
    let Some(first) = glyphs.first() else {
        return;
    };
    let mut b = first.px_bounds();
    for g in glyphs {
        let gb = g.px_bounds();
        b.min.x = b.min.x.min(gb.min.x);
        b.min.y = b.min.y.min(gb.min.y);
        b.max.x = b.max.x.max(gb.max.x);
        b.max.y = b.max.y.max(gb.max.y);
    }
    let (x0, y0) = (b.min.x as i32 - 2 * r, b.min.y as i32 - 2 * r);
    let (w, h) = (
        (b.max.x - b.min.x) as usize + 4 * r as usize + 1,
        (b.max.y - b.min.y) as usize + 4 * r as usize + 1,
    );
    let mut a = vec![0f32; w * h];
    for g in glyphs {
        let gb = g.px_bounds();
        let (bx, by) = (gb.min.x as i32 - x0, gb.min.y as i32 - y0);
        g.draw(|gx, gy, cov| {
            let (x, y) = (bx + gx as i32, by + gy as i32);
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                let p = &mut a[y as usize * w + x as usize];
                *p = (*p + cov).min(1.0);
            }
        });
    }
    // Three box blurs, a Gaussian's likeness.
    let rad = (r / 2).max(1) as usize;
    let mut tmp = vec![0f32; w * h];
    for _ in 0..3 {
        box_blur(&a, &mut tmp, w, h, rad, true);
        box_blur(&tmp, &mut a, w, h, rad, false);
    }
    for y in 0..h {
        for x in 0..w {
            let v = a[y * w + x];
            if v > 0.003 {
                canvas.blend(x0 + x as i32, y0 + y as i32, GLOW, (v * 1.6).min(0.8));
            }
        }
    }
}

fn box_blur(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, across: bool) {
    let (n, lines) = if across { (w, h) } else { (h, w) };
    let at = |line: usize, i: usize| if across { line * w + i } else { i * w + line };
    let k = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut sum = 0.0;
        for i in 0..=r.min(n - 1) {
            sum += src[at(line, i)];
        }
        for i in 0..n {
            dst[at(line, i)] = sum * k;
            if i + r + 1 < n {
                sum += src[at(line, i + r + 1)];
            }
            if i >= r {
                sum -= src[at(line, i - r)];
            }
        }
    }
}

/// Fills a shape given as `inside(u, v)` over its box (`ux`, `uy`: the
/// box's size in its own units) mapped to the screen rectangle (x, y, w,
/// h), 4x4 samples a pixel, painted with `color(v)`.
#[allow(clippy::too_many_arguments)]
fn fill_shape(
    canvas: &mut impl Canvas,
    (x, y, w, h): (f32, f32, f32, f32),
    (ux, uy): (f32, f32),
    inside: impl Fn(f32, f32) -> bool,
    color: impl Fn(f32) -> Rgb,
    alpha: f32,
) {
    let ((sx, sy), (ox, oy)) = (canvas.scale(), canvas.origin());
    let (px0, py0) = (ox + x * sx, oy + y * sy);
    let (pw, ph) = (w * sx, h * sy);
    for py in py0.floor() as i32..(py0 + ph).ceil() as i32 {
        for px in px0.floor() as i32..(px0 + pw).ceil() as i32 {
            let mut hits = 0;
            let mut vsum = 0.0;
            for j in 0..4 {
                for i in 0..4 {
                    let u = (px as f32 + (i as f32 + 0.5) / 4.0 - px0) / pw * ux;
                    let v = (py as f32 + (j as f32 + 0.5) / 4.0 - py0) / ph * uy;
                    if inside(u, v) {
                        hits += 1;
                        vsum += v;
                    }
                }
            }
            if hits > 0 {
                let c = color(vsum / hits as f32 / uy);
                canvas.blend(px, py, c, alpha * hits as f32 / 16.0);
            }
        }
    }
}

fn in_triangle((px, py): (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let s = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| {
        (p.0 - r.0) * (q.1 - r.1) - (q.0 - r.0) * (p.1 - r.1)
    };
    let (d1, d2, d3) = (s((px, py), a, b), s((px, py), b, c), s((px, py), c, a));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

/// The web page's sun (a 40x40 drawing): a disc and eight rays.
fn draw_sun(canvas: &mut impl Canvas, cx: f32, cy: f32, r: f32) {
    const RAYS: [[(f32, f32); 3]; 8] = [
        [(20.0, 1.0), (22.4, 9.0), (17.6, 9.0)],
        [(20.0, 39.0), (22.4, 31.0), (17.6, 31.0)],
        [(1.0, 20.0), (9.0, 22.4), (9.0, 17.6)],
        [(39.0, 20.0), (31.0, 22.4), (31.0, 17.6)],
        [(6.6, 6.6), (13.6, 11.0), (11.0, 13.6)],
        [(33.4, 33.4), (26.4, 29.0), (29.0, 26.4)],
        [(33.4, 6.6), (29.0, 13.6), (26.4, 11.0)],
        [(6.6, 33.4), (11.0, 26.4), (13.6, 29.0)],
    ];
    let rect = (cx - r, cy - r, 2.0 * r, 2.0 * r);
    let inside = |u: f32, v: f32| {
        (u - 20.0).hypot(v - 20.0) <= 8.5
            || RAYS.iter().any(|t| in_triangle((u, v), t[0], t[1], t[2]))
    };
    fill_shape(
        canvas,
        rect,
        (40.0, 40.0),
        inside,
        |t| gradient(&ORNAMENT_STOPS, t),
        1.0,
    );
    fill_shape(
        canvas,
        rect,
        (40.0, 40.0),
        |u, v| (u - 20.0).hypot(v - 20.0) <= 4.0,
        |_| (255, 242, 200),
        0.7,
    );
}

/// The web page's spear (a 120x40 drawing): its point, its shaft, four
/// bands and a crescent at the end.
fn draw_spear(canvas: &mut impl Canvas, x: f32, y: f32, w: f32, flip: bool) {
    let rects: [(f32, f32, f32, f32); 5] = [
        (8.0, 18.6, 96.0, 2.8),
        (70.0, 14.0, 2.0, 12.0),
        (76.0, 15.0, 2.0, 10.0),
        (82.0, 16.0, 2.0, 8.0),
        (88.0, 16.5, 2.0, 7.0),
    ];
    let inside = move |u: f32, v: f32| {
        let u = if flip { 120.0 - u } else { u };
        let crescent = (u - 100.0).hypot(v - 20.0) <= 16.0 && (u - 106.0).hypot(v - 20.0) > 13.5;
        crescent
            || in_triangle((u, v), (0.0, 20.0), (12.0, 14.0), (12.0, 26.0))
            || rects
                .iter()
                .any(|&(rx, ry, rw, rh)| u >= rx && u <= rx + rw && v >= ry && v <= ry + rh)
    };
    fill_shape(
        canvas,
        (x, y, w, w / 3.0),
        (120.0, 40.0),
        inside,
        |t| gradient(&ORNAMENT_STOPS, t),
        1.0,
    );
}

/// A rectangle of `c` at `a` over the frame.
pub fn shade(frame: &mut [u16], (x, y, w, h): (i32, i32, i32, i32), c: Rgb, a: f32) {
    let mut f = Frame565(frame);
    for yy in y..y + h {
        for xx in x..x + w {
            f.blend(xx, yy, c, a);
        }
    }
}

/// The background: the web page's night (its gradient, the moonlight
/// above right, the sea's light below), the parchment's mottling and
/// grain, warm age stains and the worn, darker edges; dithered to RGB565.
pub fn background() -> &'static [u16] {
    static BG: OnceLock<Vec<u16>> = OnceLock::new();
    BG.get_or_init(|| {
        let (w, h) = (SCREEN_W, SCREEN_H);
        let mut out = vec![0u16; w * h];
        const BAYER: [[f32; 4]; 4] = [
            [0.0, 8.0, 2.0, 10.0],
            [12.0, 4.0, 14.0, 6.0],
            [3.0, 11.0, 1.0, 9.0],
            [15.0, 7.0, 13.0, 5.0],
        ];
        let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };
        let night = [3.0, 8.0, 23.0];
        let blue = [11.0, 31.0, 74.0];
        let sea = [16.0, 64.0, 106.0];
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
                let mut c = if v < 0.55 {
                    lerp3(night, blue, v / 0.55)
                } else {
                    lerp3(blue, sea, (v - 0.55) / 0.45)
                };
                // the moonlight and the sea's light
                let d1 = ((u - 0.7) * 1.0).hypot((v - 0.18) * 0.75) / 0.5;
                c = lerp3(c, [120.0, 160.0, 255.0], 0.22 * (1.0 - d1).max(0.0));
                let d2 = (u - 0.5).hypot((v - 1.2) * 0.9) / 0.8;
                c = lerp3(c, [40.0, 120.0, 190.0], 0.55 * (1.0 - d2).max(0.0));
                // the parchment: a mottled skin and fine fibres
                let m = fbm(u * 3.2, v * 2.4, 5);
                let grain = hash(x as i32, y as i32);
                let k = 1.0 + 0.55 * (m - 0.5) + 0.10 * (grain - 0.5);
                c = [c[0] * k, c[1] * k, c[2] * k];
                // warm age stains
                let s = fbm(u * 2.0 + 11.0, v * 1.6 + 5.0, 4);
                let stain = ((s - 0.55) * 4.0).clamp(0.0, 1.0) * 0.35;
                c = lerp3(c, [c[0] * 1.25 + 8.0, c[1] * 1.05 + 4.0, c[2] * 0.7], stain);
                // the worn edges
                let e = (u - 0.5).hypot(v - 0.45) / 0.72;
                let t = ((e - 0.45) / 0.55).clamp(0.0, 1.0);
                let t = t * t * (3.0 - 2.0 * t) * 0.78;
                c = lerp3(
                    c,
                    [20.0 * c[0] / 255.0, 10.0 * c[1] / 255.0, 8.0 * c[2] / 255.0],
                    t,
                );
                let b = BAYER[y & 3][x & 3] / 16.0;
                let q = |v: f32, step: f32| ((v / step + b).floor() * step).clamp(0.0, 255.0) as u8;
                out[y * w + x] = rgb565((q(c[0], 8.0), q(c[1], 4.0), q(c[2], 8.0)));
            }
        }
        out
    })
}

fn hash(x: i32, y: i32) -> f32 {
    let mut n = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
}

fn value_noise(x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (xf, yf) = (x - xi as f32, y - yi as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(xf), s(yf));
    let a = hash(xi, yi) + (hash(xi + 1, yi) - hash(xi, yi)) * sx;
    let b = hash(xi, yi + 1) + (hash(xi + 1, yi + 1) - hash(xi, yi + 1)) * sx;
    a + (b - a) * sy
}

fn fbm(x: f32, y: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut f, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for _ in 0..octaves {
        sum += amp * value_noise(x * f, y * f);
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_fits() {
        let s = Style::body(13.0, TEXT);
        assert!(width(&s, "Atlantis") > 30.0);
        assert!(width(&Style::caps(13.0), "Atlantis") > width(&s, "Atlantis"));
        let long = format!("/home/player/{}/Atlantis", "very-long-folder".repeat(20));
        let t = fit_left(&s, &long, 300.0);
        assert!(width(&s, &t) <= 300.0 && t.starts_with('\u{2026}') && t.ends_with("/Atlantis"));
        let label = "Atlantis: The Lost Tales, GOG.com, French (1999 build), from disc images";
        let t = fit_right(&s, label, 250.0);
        assert!(
            width(&s, &t) <= 250.0 && t.starts_with("Atlantis: The") && t.ends_with('\u{2026}')
        );
        assert_eq!(fit_right(&s, "Atlantis II", 300.0), "Atlantis II");
        // A character the face lacks draws as '?'.
        assert_eq!(shown(&s, "a\u{4e2d}"), vec!['a', '?']);
    }

    #[test]
    fn items_draw_on_both_canvases() {
        let items = [
            Item::text(Style::logo(60.0), "Atlantis", 200.0, 70.0),
            Item::text(Style::caps(13.0), "Choose a game", 250.0, 95.0),
            Item::Sun {
                cx: 100.0,
                cy: 200.0,
                r: 9.0,
            },
            Item::Spear {
                x: 120.0,
                y: 85.0,
                w: 54.0,
                flip: true,
            },
        ];
        let mut frame = background().to_vec();
        for i in &items {
            draw(&mut Frame565(&mut frame), i);
        }
        assert_ne!(frame, background());
        let (w, h) = (1280, 960);
        let mut px = vec![0u32; w * h];
        let mut win = Window {
            px: &mut px,
            stride: w,
            rect: (0, 0, w, h),
        };
        for i in &items {
            draw(&mut win, i);
        }
        assert!(px.iter().filter(|&&p| p != 0).count() > 5000);
    }
}

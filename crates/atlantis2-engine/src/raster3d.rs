//! The polygon rasteriser of the 3D heads: the C routine
//! `Zi32Texture1516FastPerspective1516C` of the game's `rasterizerTR`
//! table (record @ 0x005e54d8: polygon set-up @ 0x00570710, edge set-up
//! @ 0x00570520, scan @ 0x00570190, span @ 0x00565100), the one a head is
//! drawn with on the C path: textured with the nearest texel,
//! perspective-correct every 32 pixels, with a 32-bit Z buffer, no
//! lighting and no transparency.
//!
//! Transcribed operation by operation so that it draws the original's
//! pixels at either x87 precision ([`Fpu`]): `f()` marks a value the
//! original stores as a float, `r()` the result of an x87 operation.

use crate::fpu::{f, ftol, Fpu};

/// The dialog's frame and Z buffer: 640 x 480 whatever the screen mode
/// (the viewport set by the dialog frame @ 0x00461430).
pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;

/// The viewport (@ 0x004ce450 with the dialog's rectangle 0, 479, 639, 0):
/// first row, last row + 1, first column, last column + 1.
const VP_TOP: i32 = 0;
const VP_BOTTOM1: i32 = HEIGHT as i32;
const VP_LEFT: f64 = 0.0;
const VP_RIGHT1: i32 = WIDTH as i32;

/// 2³¹, the Z scale (@ 0x005c67b8).
const Z_SCALE: f64 = 2147483648.0;

/// A vertex record of the polygon buffer (@ 0x006e1520, 9 floats each;
/// the head routine reads five): screen x and y, w = 1/z, texture u, v.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub u: f64,
    pub v: f64,
}

/// A texture: `width` x `height` RGB565 texels, top row first; both sizes
/// powers of two (others are rounded up by the binding, as in the
/// original).
#[derive(Clone, Copy, Debug)]
pub struct Texture<'a> {
    pub pixels: &'a [u16],
    pub width: usize,
    pub height: usize,
}

/// The texture binding (`rasterizerTR::SetTexture` @ 0x004ce7b0): the
/// scales from (u, v) to fixed point and the masks that turn
/// `(u << 6, v << 6)` into a texel index.
#[derive(Clone, Copy, Debug)]
struct Binding {
    scale_u: f64,
    scale_v: f64,
    mask_u: u32,
    mask_v: u32,
    shift: u32,
}

/// log2 of `n` rounded up (the bit count loop @ 0x004ce7fb).
fn log2_up(n: usize) -> u32 {
    let bits = usize::BITS - n.leading_zeros();
    let l = bits.saturating_sub(1);
    if 1usize << l == n {
        l
    } else {
        l + 1
    }
}

impl Binding {
    fn new(t: &Texture) -> Binding {
        let lw = log2_up(t.width);
        let lh = log2_up(t.height);
        let mask_u = u32::MAX.checked_shr(lh).unwrap_or(0);
        Binding {
            scale_v: f(t.height as f64 * (1u64 << (26 - lh)) as f64),
            scale_u: f((1u64 << (26 - lh - lw)) as f64 * t.width as f64),
            mask_u,
            mask_v: !mask_u,
            shift: 32 - lh - lw,
        }
    }
}

/// An edge of a triangle (@ 0x00570520's output): its rows, x and the
/// left edge's attributes (W = w, U = u·w, V = v·w) at the centre of its
/// first row, and their steps per row.
#[derive(Clone, Copy, Debug, Default)]
struct Edge {
    y0: i32,
    y1: i32,
    x: f64,
    w: f64,
    dxdy: f64,
    dw: f64,
    u: f64,
    v: f64,
    du: f64,
    dv: f64,
}

/// The per-triangle x gradients (@ 0x00570190).
#[derive(Clone, Copy, Debug, Default)]
struct Gradients {
    dwdx: f64,
    dw32: f64,
    dz: i32,
    dudx: f64,
    dvdx: f64,
    du32: f64,
    dv32: f64,
}

/// The rasteriser's destination: a 640 x 480 RGB565 frame and its Z
/// buffer (32 bits, 0 = infinitely far).
pub struct Target<'a> {
    pub pixels: &'a mut [u16],
    pub zbuf: &'a mut [u32],
}

/// Draws one polygon (a fan of triangles v0, v(k-1), v(k)) with the
/// texture, as the polygon set-up @ 0x00570710 with the head's flags
/// 0x0020010e (textured, perspective, Z buffer).
pub fn draw_polygon(fpu: Fpu, poly: &mut [Vertex], tex: &Texture, out: &mut Target) {
    if poly.len() < 3 {
        return;
    }
    let b = Binding::new(tex);
    let r = |x: f64| fpu.r(x);
    // Every record: x and y moved to the pixel centres, u and v scaled,
    // then multiplied by w (the perspective flags 0x200002).
    for p in poly.iter_mut() {
        p.x = f(r(p.x + 0.5));
        p.y = f(r(p.y + 0.5));
        p.u = f(r(b.scale_u * p.u));
        p.v = f(r(b.scale_v * p.v));
        p.u = f(r(p.u * p.w));
        p.v = f(r(p.v * p.w));
    }
    for k in 2..poly.len() {
        triangle(fpu, &b, [poly[0], poly[k - 1], poly[k]], tex, out);
    }
}

/// One triangle (a, p = previous, c = current), @ 0x00570b93..0x00570dcb.
fn triangle(fpu: Fpu, b: &Binding, tri: [Vertex; 3], tex: &Texture, out: &mut Target) {
    let [a, p, c] = tri;
    // Sort by y: top, middle, bottom (the compare tree @ 0x00570b93).
    let (mut top, mut mid, mut bot);
    if a.y > p.y {
        if p.y > c.y {
            (top, mid, bot) = (c, p, a);
        } else {
            top = p;
            if c.y <= a.y {
                (mid, bot) = (c, a);
            } else {
                (mid, bot) = (a, c);
            }
        }
    } else if p.y < c.y {
        (top, mid, bot) = (a, p, c);
    } else {
        bot = p;
        if c.y < a.y {
            (top, mid) = (c, a);
        } else {
            (top, mid) = (a, c);
        }
    }
    // Ties: the top pair by x (smaller first), the bottom pair with the
    // middle at the larger x.
    if mid.y == top.y && mid.x < top.x {
        std::mem::swap(&mut top, &mut mid);
    }
    if mid.y == bot.y && mid.x < bot.x {
        std::mem::swap(&mut bot, &mut mid);
    }
    // Edges: 0 the long edge, 1 the top short edge, 2 the bottom one.
    let mut edges = [Edge::default(); 3];
    let Some(long) = edge(fpu, &top, &bot, true) else {
        return;
    };
    edges[0] = long;
    let (left, right, lower);
    match edge(fpu, &top, &mid, true) {
        None => {
            // Flat top: the bottom short edge and the long edge.
            edges[2] = edge(fpu, &mid, &bot, true).unwrap_or(Edge {
                dxdy: f64::from(f32::MAX),
                ..Edge::default()
            });
            if edges[0].x < edges[2].x {
                (left, right) = (0, 2);
            } else {
                (left, right) = (2, 0);
            }
            lower = None;
        }
        Some(e) => {
            edges[1] = e;
            if edges[0].dxdy < edges[1].dxdy {
                (left, right) = (0, 1);
            } else {
                (left, right) = (1, 0);
            }
            // The bottom short edge has no attributes when the middle
            // vertex lies right of both others (it is then a right edge).
            let attrs = !(top.x < mid.x && bot.x < mid.x);
            lower = edge(fpu, &mid, &bot, attrs).map(|e| {
                edges[2] = e;
                if edges[left].y1 > edges[right].y1 {
                    (left, 2)
                } else {
                    (2, right)
                }
            });
        }
    }
    scan(fpu, b, &mut edges, (left, right), lower, tex, out);
}

/// The edge set-up @ 0x00570520; `None` for an empty edge (the original
/// marks it with dx/dy = FLT_MAX).
fn edge(fpu: Fpu, a: &Vertex, b: &Vertex, attrs: bool) -> Option<Edge> {
    let r = |x: f64| fpu.r(x);
    if ftol(a.y) == ftol(b.y) {
        return None;
    }
    let (top, bot) = if b.y < a.y { (b, a) } else { (a, b) };
    let ya = ftol(top.y);
    if ya >= VP_BOTTOM1 - 1 {
        return None;
    }
    let yb = ftol(bot.y);
    if yb <= VP_TOP {
        return None;
    }
    let dx = r(bot.x - top.x);
    let inv = f(r(1.0 / r(bot.y - top.y)));
    let y1 = if yb < VP_BOTTOM1 - 1 { yb } else { VP_BOTTOM1 };
    let y0 = ya.max(VP_TOP);
    // The centre of row y0 (y already holds the +0.5).
    let t = f(r(f64::from(y0 + 1) - top.y));
    let dxdy = r(dx * inv);
    let mut e = Edge {
        y0,
        y1,
        x: f(r(r(dxdy * t) + top.x)),
        dxdy: f(dxdy),
        ..Edge::default()
    };
    if attrs {
        let attr = |from: f64, to: f64| -> (f64, f64) {
            let d = r(r(to - from) * inv);
            (f(r(r(d * t) + from)), f(d))
        };
        (e.w, e.dw) = attr(top.w, bot.w);
        (e.u, e.du) = attr(top.u, bot.u);
        (e.v, e.dv) = attr(top.v, bot.v);
    }
    Some(e)
}

/// The scan @ 0x00570190: the x gradients from the two edges' slopes,
/// then the rows of the upper pair and of the lower pair.
fn scan(
    fpu: Fpu,
    b: &Binding,
    edges: &mut [Edge; 3],
    upper: (usize, usize),
    lower: Option<(usize, usize)>,
    tex: &Texture,
    out: &mut Target,
) {
    let r = |x: f64| fpu.r(x);
    let (l, rt) = (edges[upper.0], edges[upper.1]);
    let k = r(1.0 / r(rt.dxdy - l.dxdy));
    let gw = r(r(rt.dw - l.dw) * k);
    let dwdx = f(gw);
    let g = Gradients {
        dwdx,
        dw32: f(r(gw * 32.0)),
        dz: ftol(r(dwdx * Z_SCALE)),
        dudx: f(r(r(rt.du - l.du) * k)),
        dvdx: f(r(r(rt.dv - l.dv) * k)),
        du32: 0.0,
        dv32: 0.0,
    };
    let g = Gradients {
        du32: f(r(g.dudx * 32.0)),
        dv32: f(r(g.dvdx * 32.0)),
        ..g
    };
    let rows =
        |edges: &mut [Edge; 3], li: usize, ri: usize, y: &mut i32, n: i32, out: &mut Target| {
            for _ in 0..n {
                let xs = ftol(VP_LEFT).max(ftol(edges[li].x));
                let xe = ftol(edges[ri].x).min(VP_RIGHT1);
                if xs < xe {
                    span(fpu, b, &g, &edges[li], *y, xs, xe, tex, out);
                }
                *y += 1;
                let rr = &mut edges[ri];
                rr.x = f(r(rr.dxdy + rr.x));
                let ll = &mut edges[li];
                ll.x = f(r(ll.dxdy + ll.x));
                ll.w = f(r(ll.dw + ll.w));
                ll.u = f(r(ll.du + ll.u));
                ll.v = f(r(ll.dv + ll.v));
            }
        };
    let end = edges[upper.1].y1.min(edges[upper.0].y1);
    let mut y = edges[upper.0].y0;
    if y < end {
        let n = end - y;
        rows(edges, upper.0, upper.1, &mut y, n, out);
    }
    if let Some((li, ri)) = lower {
        let mut y = end;
        if y < edges[li].y1 {
            let n = edges[li].y1 - y;
            rows(edges, li, ri, &mut y, n, out);
        }
    }
}

/// The span @ 0x00565100: pixels xs..xe of row y, the perspective
/// division every 32 pixels, the texel and Z steps linear in between.
#[allow(clippy::too_many_arguments)]
fn span(
    fpu: Fpu,
    b: &Binding,
    g: &Gradients,
    l: &Edge,
    y: i32,
    xs: i32,
    xe: i32,
    tex: &Texture,
    out: &mut Target,
) {
    let r = |x: f64| fpu.r(x);
    let row = y as usize * WIDTH;
    // The centre of pixel xs, from the left edge.
    let d = r(f64::from(xs + 1) - l.x);
    let df = f(d);
    let mut uu = r(r(d * g.dudx) + l.u);
    let vf = f(r(r(df * g.dvdx) + l.v));
    let mut ww = r(r(df * g.dwdx) + l.w);
    let mut z = ftol(r(ww * Z_SCALE)) as u32;
    let rc = r(1.0 / ww);
    let mut u = ftol(r(rc * uu));
    let mut v = ftol(r(rc * vf));
    let n = xe - xs;
    let mut x = xs as usize;
    let mut plot = |x: usize, z: u32, a: i32, bb: i32| {
        if z >= out.zbuf[row + x] {
            out.zbuf[row + x] = z;
            let i = ((a as u32 & b.mask_u) | (bb as u32 & b.mask_v)) >> b.shift;
            out.pixels[row + x] = tex.pixels[i as usize];
        }
    };
    let mut vv = vf;
    if n >> 5 > 0 {
        for _ in 0..n >> 5 {
            ww = r(ww + g.dw32);
            uu = r(uu + g.du32);
            vv = r(vv + g.dv32);
            let rc = r(1.0 / ww);
            let u2 = ftol(r(rc * uu));
            let v2 = ftol(r(rc * vv));
            let du = (u2.wrapping_sub(u) >> 5).wrapping_shl(6);
            let dv = (v2.wrapping_sub(v) >> 5).wrapping_shl(6);
            let mut a = u.wrapping_shl(6);
            let mut bb = v.wrapping_shl(6);
            for _ in 0..32 {
                plot(x, z, a, bb);
                x += 1;
                z = z.wrapping_add(g.dz as u32);
                a = a.wrapping_add(du);
                bb = bb.wrapping_add(dv);
            }
            u = u2;
            v = v2;
        }
        vv = f(vv);
    }
    let rem = n & 31;
    if rem > 0 {
        let nf = f64::from(rem);
        let rr = f(r(1.0 / r(r(nf * g.dwdx) + ww)));
        let inv_n = f(r(1.0 / nf));
        let u2 = ftol(r(r(r(nf * g.dudx) + uu) * rr));
        let du = ftol(r(f64::from(u2.wrapping_sub(u)) * inv_n));
        let v2 = ftol(r(r(r(nf * g.dvdx) + vv) * rr));
        let dv = ftol(r(f64::from(v2.wrapping_sub(v)) * inv_n));
        let du = du.wrapping_shl(6);
        let dv = dv.wrapping_shl(6);
        let mut a = u.wrapping_shl(6);
        let mut bb = v.wrapping_shl(6);
        for _ in 0..rem {
            plot(x, z, a, bb);
            x += 1;
            z = z.wrapping_add(g.dz as u32);
            a = a.wrapping_add(du);
            bb = bb.wrapping_add(dv);
        }
    }
}

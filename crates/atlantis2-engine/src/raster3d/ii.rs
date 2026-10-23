//! `Zi32Texture16FastPerspective1516II`, the head routine of a Pentium II
//! (the `rasterizerTR` record @ 0x005e65d0, class 8): polygon set-up
//! @ 0x0051ca00, scan @ 0x0051ee00, MMX span @ 0x004ff6c0 (shared with
//! the `MX` routine). Textured with the nearest texel, perspective-correct
//! every 32 pixels, 32-bit Z buffer, as the C routine, but its arithmetic
//! is its own: rows chosen from the vertices' integer rows, the texel steps
//! of a 32-pixel block from the shifted end points, the last block's steps
//! from a float difference.
//!
//! The set-up loads the x87 control word 0x0e7f (@ 0x0051ca01: 53-bit
//! precision, rounding toward zero) at each polygon and never restores it,
//! so every operation here is [`Fpu::Chop`], whatever the caller ran under.
//! The MMX span's integer lanes are written as plain `u32` operations.

use super::{Binding, Target, Texture, Vertex};
use crate::fpu::{f, Fpu};

/// The word the set-up loads (@ 0x005edc78).
const C: Fpu = Fpu::Chop;

/// 0.5, added to every screen x and y (@ 0x005edbc0).
const HALF: f64 = 0.5;

/// The viewport as the rasteriser holds it for the dialog (read under
/// emulation after `SetViewport` @ 0x004ce450 with 0, 479, 639, 0): first
/// row (@ 0x006e13d8) and the span's left clip as a float (@ 0x006e1418);
/// the last row + 1 (@ 0x006e1440, 480) and the right clip (@ 0x006e1410,
/// 640) are the target's height and width ([`Target`]: larger in the HD
/// mode only).
const VP_TOP: i32 = 0;
const CLIP_LEFT: f64 = 0.0;

/// 32, the block length (@ 0x005edc20), and 2³¹, the Z scale
/// (@ 0x005edbe0).
const BLOCK: f64 = 32.0;
const Z_SCALE: f64 = 2147483648.0;

/// An edge record (0x48 bytes @ 0x005edcf0, 0x005edd38, 0x005edd80): first
/// row, end row, x at the centre of the first row, its step, and for an
/// edge set up with attributes W, U, V there and their steps.
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

/// The edge set-up, one macro the set-up repeats for every case: from
/// `top` to `bot`, first row `y0`, end row `y1`. The slope and the
/// attribute steps are stored as floats, x and the attributes are computed
/// from the unrounded ones at the centre of row `y0`.
fn edge(top: &Vertex, bot: &Vertex, y0: i32, y1: i32, attrs: bool) -> Edge {
    let inv = C.div(1.0, C.sub(bot.y, top.y));
    let t = C.sub(f64::from(y0.wrapping_add(1)), top.y);
    let dxdy = C.mul(C.sub(bot.x, top.x), inv);
    let mut e = Edge {
        y0,
        y1,
        x: C.f(C.add(top.x, C.mul(dxdy, t))),
        dxdy: C.f(dxdy),
        ..Edge::default()
    };
    if attrs {
        let dw = C.mul(inv, C.sub(bot.w, top.w));
        e.w = C.f(C.add(top.w, C.mul(t, dw)));
        e.dw = C.f(dw);
        let du = C.mul(C.sub(bot.u, top.u), inv);
        e.u = C.f(C.add(top.u, C.mul(du, t)));
        e.du = C.f(du);
        let dv = C.mul(C.sub(bot.v, top.v), inv);
        e.v = C.f(C.add(top.v, C.mul(dv, t)));
        e.dv = C.f(dv);
    }
    e
}

/// `a < b` as `fcomi` / `jb` take it: true when unordered too.
fn below(a: f64, b: f64) -> bool {
    !matches!(
        a.partial_cmp(&b),
        Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
    )
}

/// Draws one polygon (a fan v0, v(k-1), v(k)) as the set-up @ 0x0051ca00
/// does: each vertex record gets +0.5 on x and y, its integer row
/// (`fistp` toward zero) and u, v scaled and multiplied by w, once.
pub fn draw_polygon(poly: &mut [Vertex], tex: &Texture, out: &mut Target) {
    if poly.len() < 3 {
        return;
    }
    let b = Binding::new(tex);
    let mut rows = Vec::with_capacity(poly.len());
    for p in poly.iter_mut() {
        p.x = C.f(C.add(HALF, p.x));
        let y = C.add(HALF, p.y);
        p.y = C.f(y);
        rows.push(C.fistp(y));
        p.u = C.f(C.mul(C.mul(b.scale_u, p.u), p.w));
        p.v = C.f(C.mul(C.mul(b.scale_v, p.v), p.w));
    }
    for k in 2..poly.len() {
        triangle(
            &b,
            [&poly[0], &poly[k - 1], &poly[k]],
            [rows[0], rows[k - 1], rows[k]],
            tex,
            out,
        );
    }
}

/// The case tree @ 0x0051cadf on the integer rows of v0, v1, v2.
fn triangle(b: &Binding, v: [&Vertex; 3], i: [i32; 3], tex: &Texture, out: &mut Target) {
    let [v0, v1, v2] = v;
    let [i0, i1, i2] = i;
    let mut e = Scan {
        b,
        tex,
        edges: [Edge::default(); 3],
    };
    if i0 < i1 {
        if i1 < i2 {
            e.general([v0, v1, v2], [i0, i1, i2], out);
        } else if i1 == i2 {
            // @ 0x0051e5xx: v1 left when its x is below v2's.
            let (l, r) = if below(v1.x, v2.x) {
                (v1, v2)
            } else {
                (v2, v1)
            };
            e.flat_bottom(v0, l, r, i0, i1, out);
        } else if i0 < i2 {
            e.general([v0, v2, v1], [i0, i2, i1], out);
        } else if i0 == i2 {
            let (l, r) = if below(v2.x, v0.x) {
                (v2, v0)
            } else {
                (v0, v2)
            };
            e.flat_top(l, r, v1, i0, i1, out);
        } else {
            e.general([v2, v0, v1], [i2, i0, i1], out);
        }
    } else if i0 == i1 {
        if i1 < i2 {
            let (l, r) = if below(v1.x, v0.x) {
                (v1, v0)
            } else {
                (v0, v1)
            };
            e.flat_top(l, r, v2, i0, i2, out);
        } else if i1 > i2 {
            let (l, r) = if below(v1.x, v0.x) {
                (v1, v0)
            } else {
                (v0, v1)
            };
            e.flat_bottom(v2, l, r, i2, i0, out);
        }
        // All three on one row: nothing.
    } else if i1 < i2 {
        if i0 < i2 {
            e.general([v1, v0, v2], [i1, i0, i2], out);
        } else if i0 == i2 {
            let (l, r) = if below(v0.x, v2.x) {
                (v0, v2)
            } else {
                (v2, v0)
            };
            e.flat_bottom(v1, l, r, i1, i0, out);
        } else {
            e.general([v1, v2, v0], [i1, i2, i0], out);
        }
    } else if i1 == i2 {
        let (l, r) = if below(v2.x, v1.x) {
            (v2, v1)
        } else {
            (v1, v2)
        };
        e.flat_top(l, r, v0, i1, i0, out);
    } else {
        e.general([v2, v1, v0], [i2, i1, i0], out);
    }
}

/// The per-triangle state: the three edge records and the bound texture.
struct Scan<'a> {
    b: &'a Binding,
    tex: &'a Texture<'a>,
    edges: [Edge; 3],
}

/// The x gradients of a triangle (@ 0x0051ee00): per pixel, as floats, and
/// per 32 pixels from the unrounded ones; the Z step.
struct Gradients {
    dwdx: f64,
    dudx: f64,
    dvdx: f64,
    dw32: f64,
    du32: f64,
    dv32: f64,
    dz: i32,
}

impl Scan<'_> {
    /// Three distinct rows, top `t`, middle `m`, bottom `b` (every
    /// ordering of the case tree shares this shape).
    fn general(&mut self, v: [&Vertex; 3], i: [i32; 3], out: &mut Target) {
        let [t, m, b] = v;
        let [it, im, ib] = i;
        let vp_bottom = out.height as i32;
        if it >= vp_bottom || ib <= VP_TOP {
            return;
        }
        let yt = it.max(VP_TOP);
        let yb = ib.min(vp_bottom);
        let ym = im.min(vp_bottom);
        let e = &mut self.edges;
        e[0] = edge(t, b, yt, yb, true);
        if ym <= VP_TOP {
            // The upper half is above the viewport: the lower short edge
            // and the long edge, left by their slopes (`jbe`: left is the
            // long edge also when unordered).
            e[1] = edge(m, b, ym.max(VP_TOP), yb, true);
            let (l, r) = if e[1].dxdy > e[0].dxdy {
                (1, 0)
            } else {
                (0, 1)
            };
            let end = e[0].y1;
            self.scan(l, r, end, None, out);
            return;
        }
        e[1] = edge(t, m, yt, ym, true);
        let end = e[1].y1;
        if e[1].dxdy > e[0].dxdy {
            // The middle vertex on the right: the long edge is the left
            // one for both halves; the lower right edge needs no
            // attributes.
            if ym == vp_bottom {
                self.scan(0, 1, end, None, out);
            } else {
                self.edges[2] = edge(m, b, ym, yb, false);
                self.scan(0, 1, end, Some((0, 2)), out);
            }
        } else if ym == vp_bottom {
            self.scan(1, 0, end, None, out);
        } else {
            self.edges[2] = edge(m, b, ym, yb, true);
            self.scan(1, 0, end, Some((2, 0)), out);
        }
    }

    /// Two vertices on the top row (`l` left of `r`), `b` below.
    fn flat_top(
        &mut self,
        l: &Vertex,
        r: &Vertex,
        b: &Vertex,
        itop: i32,
        ib: i32,
        out: &mut Target,
    ) {
        let vp_bottom = out.height as i32;
        if itop >= vp_bottom || ib <= VP_TOP {
            return;
        }
        let y0 = itop.max(VP_TOP);
        let y1 = ib.min(vp_bottom);
        self.edges[0] = edge(l, b, y0, y1, true);
        self.edges[1] = edge(r, b, y0, y1, true);
        self.scan(0, 1, y1, None, out);
    }

    /// One vertex `t` on top, two on the bottom row (`l` left of `r`).
    fn flat_bottom(
        &mut self,
        t: &Vertex,
        l: &Vertex,
        r: &Vertex,
        it: i32,
        ibot: i32,
        out: &mut Target,
    ) {
        let vp_bottom = out.height as i32;
        if it >= vp_bottom || ibot <= VP_TOP {
            return;
        }
        let y0 = it.max(VP_TOP);
        let y1 = ibot.min(vp_bottom);
        self.edges[0] = edge(t, l, y0, y1, true);
        self.edges[1] = edge(t, r, y0, y1, true);
        self.scan(0, 1, y1, None, out);
    }

    /// The scan @ 0x0051ee00: the gradients from the upper pair, the rows
    /// of the upper pair down to `end`, then those of the lower pair.
    fn scan(
        &mut self,
        li: usize,
        ri: usize,
        end: i32,
        lower: Option<(usize, usize)>,
        out: &mut Target,
    ) {
        let (l, r) = (self.edges[li], self.edges[ri]);
        let k = C.div(1.0, C.sub(r.dxdy, l.dxdy));
        let dvdx = C.mul(C.sub(r.dv, l.dv), k);
        let dwdx = C.mul(C.sub(r.dw, l.dw), k);
        let dudx = C.mul(k, C.sub(r.du, l.du));
        let g = Gradients {
            dvdx: C.f(dvdx),
            dv32: C.f(C.mul(dvdx, BLOCK)),
            dwdx: C.f(dwdx),
            dz: C.fistp(C.mul(Z_SCALE, dwdx)),
            dw32: C.f(C.mul(dwdx, BLOCK)),
            dudx: C.f(dudx),
            du32: C.f(C.mul(BLOCK, dudx)),
        };
        let mut y = self.edges[li].y0;
        let (mut li, mut ri) = (li, ri);
        let mut count = end.wrapping_sub(y);
        let mut lower = lower;
        loop {
            // The rows (@ 0x0051eed7): `count` of them, at least one (the
            // set-up's cases give a non-empty half).
            if count <= 0 {
                break;
            }
            loop {
                self.row(li, ri, y, &g, out);
                y += 1;
                count -= 1;
                if count == 0 {
                    break;
                }
                self.step_right(ri);
                self.step_left(li);
            }
            // @ 0x0051ef36: the right edge steps once more; the left one
            // too when it goes on below.
            self.step_right(ri);
            let Some((ll, lr)) = lower.take() else { break };
            if ll == li {
                self.step_left(li);
            }
            li = ll;
            ri = lr;
            count = self.edges[ri].y1.wrapping_sub(end);
        }
    }

    fn step_right(&mut self, i: usize) {
        let e = &mut self.edges[i];
        e.x = C.f(C.add(e.dxdy, e.x));
    }

    fn step_left(&mut self, i: usize) {
        let e = &mut self.edges[i];
        e.v = C.f(C.add(e.dv, e.v));
        e.u = C.f(C.add(e.du, e.u));
        e.w = C.f(C.add(e.dw, e.w));
        e.x = C.f(C.add(e.dxdy, e.x));
    }

    /// One row: the span from the left edge's x (clipped at the left
    /// clip, `fcmovb`) to the right edge's (clipped at the right one),
    /// both by `fistp` toward zero.
    fn row(&self, li: usize, ri: usize, y: i32, g: &Gradients, out: &mut Target) {
        let (l, r) = (&self.edges[li], &self.edges[ri]);
        let xr = C.fistp(r.x);
        let xl = C.fistp(if below(CLIP_LEFT, l.x) {
            l.x
        } else {
            CLIP_LEFT
        });
        let clip_right = out.width as i32;
        let xe = if clip_right >= xr { xr } else { clip_right };
        // The original tests only `xl < xe` (@ 0x0051ef0a). xl is negative
        // only as the integer indefinite (a NaN or out-of-range left x, the
        // left clip being 0): with an in-range right x the original then
        // spans from x = -2^31, a write outside the picture; nothing is
        // drawn here (docs/TECHNICAL.md).
        if 0 <= xl && xl < xe {
            self.span(l, g, y, xl, xe, out);
        }
    }

    /// The span @ 0x004ff6c0: the texel coordinates at the first pixel and
    /// every 32 pixels by a division (a table @ 0x005ed7c0), stepped in
    /// between in fixed point two pixels at a time (MMX); the last block's
    /// step from the float difference to its last pixel.
    #[allow(clippy::too_many_arguments)]
    fn span(&self, l: &Edge, g: &Gradients, y: i32, xl: i32, xe: i32, out: &mut Target) {
        let n = xe - xl;
        let d = C.sub(f64::from(xl + 1), l.x);
        let w = C.add(l.w, C.mul(g.dwdx, d));
        let rc = C.div(1.0, w);
        let v = C.add(l.v, C.mul(g.dvdx, d));
        let u = C.add(C.mul(d, g.dudx), l.u);
        let wf = C.f32(w);
        let mut last_u = C.f(C.mul(rc, u));
        let mut last_v = C.f(C.mul(rc, v));
        let mut table = vec![(C.fistp(C.mul(rc, u)), C.fistp(C.mul(rc, v)))];
        let (mut w, mut u, mut v) = (w, u, v);
        let (blocks, rem) = if n > 32 {
            let mut blocks = n >> 5;
            let mut rem = n & 31;
            if rem == 0 {
                blocks -= 1;
                rem = 32;
            }
            for _ in 0..blocks {
                w = C.add(w, g.dw32);
                let rc = C.div(1.0, w);
                v = C.add(v, g.dv32);
                u = C.add(u, g.du32);
                table.push((C.fistp(C.mul(rc, u)), C.fistp(C.mul(v, rc))));
                last_u = C.f(C.mul(rc, u));
                last_v = C.f(C.mul(v, rc));
            }
            (blocks as usize, rem)
        } else {
            (0, n)
        };
        // The last block's end point: pixel rem - 1 of it (the tables
        // @ 0x005ede18 hold k - 1 and @ 0x005ee018 1 / (k - 1) as floats,
        // the latter 0 for k = 1).
        let steps = f64::from(rem - 1);
        let inv = if rem > 1 { f(1.0 / steps) } else { 0.0 };
        let rr = C.div(1.0, C.add(C.mul(g.dwdx, steps), w));
        let ve = C.add(v, C.mul(steps, g.dvdx));
        let ue = C.add(u, C.mul(g.dudx, steps));
        let dv_last = C.fistp(C.mul(C.sub(C.mul(ve, rr), last_v), inv));
        let du_last = C.fistp(C.mul(C.sub(C.mul(ue, rr), last_u), inv));
        // Z at the first pixel: the float W with 31 added to its exponent
        // (@ 0x004ff862), then `fistp`.
        let mut z = C.fistp(f64::from(f32::from_bits(
            wf.to_bits().wrapping_add(0x0f80_0000),
        ))) as u32;
        let dz = g.dz as u32;
        let row = y as usize * out.width;
        let mut x = xl as usize;
        let b = self.b;
        let tex = self.tex;
        let mut plot = |x: usize, z: u32, uu: u32, vv: u32| {
            if z >= out.zbuf[row + x] {
                out.zbuf[row + x] = z;
                let i = ((uu & b.mask_u) | (vv & b.mask_v)) >> b.shift;
                out.pixels[row + x] = tex.pixels[i as usize];
            }
        };
        for k in 0..blocks {
            let (u0, v0) = table[k];
            let (u1, v1) = table[k + 1];
            // `psrld` 5 of both ends, their difference, `pslld` 6.
            let du = ((u1 as u32 >> 5).wrapping_sub(u0 as u32 >> 5)) << 6;
            let dv = ((v1 as u32 >> 5).wrapping_sub(v0 as u32 >> 5)) << 6;
            let (mut uu, mut vv) = ((u0 as u32) << 6, (v0 as u32) << 6);
            for _ in 0..32 {
                plot(x, z, uu, vv);
                x += 1;
                z = z.wrapping_add(dz);
                uu = uu.wrapping_add(du);
                vv = vv.wrapping_add(dv);
            }
        }
        let (u0, v0) = table[blocks];
        let (mut uu, mut vv) = ((u0 as u32) << 6, (v0 as u32) << 6);
        let (du, dv) = ((du_last as u32) << 6, (dv_last as u32) << 6);
        for _ in 0..rem {
            plot(x, z, uu, vv);
            x += 1;
            z = z.wrapping_add(dz);
            uu = uu.wrapping_add(du);
            vv = vv.wrapping_add(dv);
        }
    }
}

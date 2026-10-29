//! `Zi32Texture16Linear1516II`, the head routine of Atlantis III on a
//! Pentium II or an SSE CPU (atlantis3-gog-fr-3473408: `rasterizerTR`
//! record @ 0x00725fe0, mode 0x10e, class mask 0x628): polygon set-up
//! @ 0x005d7000, scan @ 0x005d9100, MMX span @ 0x0058e000. Textured with
//! the nearest texel, **affine** (u and v linear in screen space, no
//! division per pixel or block), 32-bit Z buffer, no lighting, no
//! transparency.
//!
//! Same frame as [`super::ii`] (Atlantis II's `FastPerspective` II): the
//! vertex pass, the integer rows, the edge macro and the case tree are the
//! same instructions (73 paths of the case tree compared symbolically,
//! atlantis3-re tools/emu/a3_x87sym.py), but for:
//! - the vertex pass: u and v are scaled but not multiplied by w; w gets
//!   31 added to its exponent (`add [v+8], 0x0f800000`: w · 2³¹, the Z
//!   scale, applied once to the vertex instead of in the span);
//! - the three flat-top cases (two vertices on the top row): both edges
//!   are set up first and the left one is the one whose x at the first row
//!   centre is smaller ([`Scan::flat_top`]); A2 compares the vertices' x;
//! - the scan: the x gradients of u, v, Z are turned once per triangle into
//!   integer steps per pixel (`fistp` of the unrounded gradient);
//! - the span: u, v, Z at the first pixel by one `fistp` each, then integer
//!   steps; no division.
//!
//! The set-up loads the x87 control word 0x0e7f (@ 0x00730e38: 53-bit
//! precision, rounding toward zero) and never restores it: every operation
//! is [`Fpu::Chop`]. Checked against evidence/golden/a3_raster_linear.txt
//! (the original routine alone under Unicorn: 480 polygons, 640 x 480 and
//! 800 x 600, 512² and 1024² textures).

use super::{Binding, Target, Texture, Vertex};
use crate::fpu::Fpu;

/// The word the set-up loads (@ 0x00730e38).
const C: Fpu = Fpu::Chop;

/// 0.5, added to every screen x and y (@ 0x00730d80).
const HALF: f64 = 0.5;

/// Added to the bits of every vertex w (@ 0x005d7067, 0x005d70b7): 31 to
/// its exponent, w · 2³¹ for a normal float.
const Z_EXP: u32 = 0x0f80_0000;

/// The viewport as SetViewport (@ 0x004feb10) stores it for (0, H - 1,
/// W - 1, 0): first row (@ 0x007944b0) and the span's left clip as a
/// float (@ 0x00794540); the last row + 1 (@ 0x00794580) and the right
/// clip (@ 0x007944f8) are the target's height and width.
const VP_TOP: i32 = 0;
const CLIP_LEFT: f64 = 0.0;

/// An edge record (0x48 bytes @ 0x00730eb0, 0x00730ef8, 0x00730f40):
/// +0 first row, +4 end row, +8 x at the centre of the first row, +0xc Z,
/// +0x10 dx/dy, +0x14 dZ, +0x18 U, +0x1c V, +0x20 dU, +0x24 dV.
#[derive(Clone, Copy, Debug, Default)]
struct Edge {
    y0: i32,
    y1: i32,
    x: f64,
    z: f64,
    dxdy: f64,
    dz: f64,
    u: f64,
    v: f64,
    du: f64,
    dv: f64,
}

/// The edge macro the set-up repeats for every case (e.g. @ 0x005d7137..
/// 0x005d71b4): the same expressions as `ii::edge`, with Z for W.
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
        let dv = C.mul(C.sub(bot.v, top.v), inv);
        e.dv = C.f(dv);
        let du = C.mul(C.sub(bot.u, top.u), inv);
        e.du = C.f(du);
        e.v = C.f(C.add(top.v, C.mul(dv, t)));
        e.u = C.f(C.add(top.u, C.mul(du, t)));
        let dz = C.mul(inv, C.sub(bot.w, top.w));
        e.dz = C.f(dz);
        e.z = C.f(C.add(top.w, C.mul(t, dz)));
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

/// Draws one polygon (a fan v0, v(k-1), v(k): the vertex records
/// @ 0x00794678, 0x24 bytes each, up to the end pointer @ 0x00794f80) as
/// the set-up @ 0x005d7000 does: each record once gets +0.5 on x and y
/// (stored as floats), its integer row (`fistp` of the unrounded y + 0.5,
/// toward zero), u and v multiplied by the binding's scales, and w's
/// exponent raised by 31.
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
        p.u = C.f(C.mul(b.scale_u, p.u));
        p.v = C.f(C.mul(b.scale_v, p.v));
        p.w = f64::from(f32::from_bits((p.w as f32).to_bits().wrapping_add(Z_EXP)));
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

/// The case tree @ 0x005d70dc on the integer rows of v0, v1, v2: as
/// `ii::triangle` but for the flat-top cases.
fn triangle(b: &Binding, v: [&Vertex; 3], i: [i32; 3], tex: &Texture, out: &mut Target) {
    let [v0, v1, v2] = v;
    let [i0, i1, i2] = i;
    let mut e = Scan {
        b,
        tex,
        edges: [Edge::default(); 3],
        g: Gradients::default(),
    };
    if i0 < i1 {
        if i1 < i2 {
            e.general([v0, v1, v2], [i0, i1, i2], out);
        } else if i1 == i2 {
            let (l, r) = if below(v1.x, v2.x) {
                (v1, v2)
            } else {
                (v2, v1)
            };
            e.flat_bottom(v0, l, r, i0, i1, out);
        } else if i0 < i2 {
            e.general([v0, v2, v1], [i0, i2, i1], out);
        } else if i0 == i2 {
            e.flat_top(v0, v2, v1, i0, i1, out);
        } else {
            e.general([v2, v0, v1], [i2, i0, i1], out);
        }
    } else if i0 == i1 {
        if i1 < i2 {
            e.flat_top(v0, v1, v2, i0, i2, out);
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
        e.flat_top(v1, v2, v0, i1, i0, out);
    } else {
        e.general([v2, v1, v0], [i2, i1, i0], out);
    }
}

/// The x gradients of a triangle (@ 0x005d9100): the float ones the span
/// starts from (@ 0x007945f8 u, 0x007945fc v, 0x00794660 Z) and the
/// integer steps per pixel (`fistp` of the unrounded gradients; u and v
/// shifted left by 6, @ 0x00730fa0 / 0x00730fa4; Z @ 0x00730fc8).
#[derive(Clone, Copy, Default)]
struct Gradients {
    gu: f64,
    gv: f64,
    gz: f64,
    du: u32,
    dv: u32,
    dz: u32,
}

struct Scan<'a> {
    b: &'a Binding,
    tex: &'a Texture<'a>,
    edges: [Edge; 3],
    g: Gradients,
}

impl Scan<'_> {
    /// Three distinct rows, top `t`, middle `m`, bottom `b`: `ii::Scan::general`.
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

    /// Two vertices `a`, `b` on the top row (`a` the lower-numbered of the
    /// pair: (v0, v2), (v0, v1) or (v1, v2)), `c` below. Edge 1 from `a`,
    /// edge 0 from `b`, both with attributes; then `fcompi` of the stored x
    /// (@ 0x005d9037 and its twins): `a`'s edge is the right one only when
    /// its x is greater (`ja`), the left one on ties and NaN.
    fn flat_top(
        &mut self,
        a: &Vertex,
        b: &Vertex,
        c: &Vertex,
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
        self.edges[1] = edge(a, c, y0, y1, true);
        self.edges[0] = edge(b, c, y0, y1, true);
        if self.edges[1].x > self.edges[0].x {
            self.scan(0, 1, y1, None, out);
        } else {
            self.scan(1, 0, y1, None, out);
        }
    }

    /// One vertex `t` on top, two on the bottom row (`l` left of `r` by the
    /// vertices' x, as in A2).
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

    /// The scan @ 0x005d9100: the gradients from the upper pair (left
    /// `li`, right `ri`), then the rows of the upper pair down to `end`,
    /// then those of the lower pair (@ 0x00730f88 / 0x00730f8c). There is
    /// no test of an empty half: the set-up's cases never give one (a
    /// count of 0 would run 2³² rows).
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
        let dud = C.sub(r.du, l.du);
        let dvdx = C.mul(C.sub(r.dv, l.dv), k);
        let dzdx = C.mul(C.sub(r.dz, l.dz), k);
        let dudx = C.mul(k, dud);
        self.g = Gradients {
            gv: C.f(dvdx),
            gz: C.f(dzdx),
            gu: C.f(dudx),
            dz: C.fistp(dzdx) as u32,
            dv: (C.fistp(dvdx) as u32) << 6,
            du: (C.fistp(dudx) as u32) << 6,
        };
        let mut y = self.edges[li].y0;
        let (mut li, mut ri) = (li, ri);
        let mut count = end.wrapping_sub(y);
        let mut lower = lower;
        loop {
            loop {
                self.row(li, ri, y, out);
                y += 1;
                count -= 1;
                if count == 0 {
                    break;
                }
                self.step_right(ri);
                self.step_left(li);
            }
            // @ 0x005d9256: the right edge steps once more; the left one
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
        e.x = C.f(C.add(e.x, e.dxdy));
    }

    fn step_left(&mut self, i: usize) {
        let e = &mut self.edges[i];
        e.v = C.f(C.add(e.v, e.dv));
        e.u = C.f(C.add(e.u, e.du));
        e.z = C.f(C.add(e.z, e.dz));
        e.x = C.f(C.add(e.x, e.dxdy));
    }

    /// One row (@ 0x005d91f7): from the left edge's x (clipped at the left
    /// clip, `fcmovb`) to the right edge's (clipped at the right one,
    /// `cmovge`), both by `fistp` toward zero; the span when xl < xe.
    fn row(&self, li: usize, ri: usize, y: i32, out: &mut Target) {
        let (l, r) = (&self.edges[li], &self.edges[ri]);
        let xr = C.fistp(r.x);
        let xl = C.fistp(if below(CLIP_LEFT, l.x) {
            l.x
        } else {
            CLIP_LEFT
        });
        let clip_right = out.width as i32;
        let xe = if clip_right >= xr { xr } else { clip_right };
        // As in `ii`: xl < 0 only as the integer indefinite (NaN left x),
        // a write outside the picture in the original; nothing here.
        if 0 <= xl && xl < xe {
            self.span(l, y, xl, xe, out);
        }
    }

    /// The span @ 0x0058e000: u, v, Z at the first pixel's centre from the
    /// left edge by one `fistp` each, then the triangle's integer steps.
    /// MMX: u in two lanes (pixels 2k, 2k + 1), v in two, stepped by 2 per
    /// pair; a texel index ((u & mask_u) | (v & mask_v)) >> shift (the
    /// binding's two-lane masks @ 0x00794640 / 0x00794648, shift
    /// @ 0x007945c8); the odd last pixel and a 1-pixel span by the same
    /// formula. Z written with the texel when Z >= the buffer's word
    /// (unsigned, `jb` skips); Z steps for every pixel.
    fn span(&self, l: &Edge, y: i32, xl: i32, xe: i32, out: &mut Target) {
        let g = &self.g;
        let n = (xe - xl) as usize;
        let d = C.sub(f64::from(xl + 1), l.x);
        let mut z = C.fistp(C.add(C.mul(g.gz, d), l.z)) as u32;
        let u0 = C.fistp(C.add(l.u, C.mul(g.gu, d))) as u32;
        let v0 = C.fistp(C.add(l.v, C.mul(d, g.gv))) as u32;
        let (mut u, mut v) = (u0 << 6, v0 << 6);
        let b = self.b;
        let tex = self.tex;
        let row = y as usize * out.width + xl as usize;
        for i in row..row + n {
            if z >= out.zbuf[i] {
                out.zbuf[i] = z;
                let t = ((u & b.mask_u) | (v & b.mask_v)) >> b.shift;
                out.pixels[i] = tex.pixels[t as usize];
            }
            z = z.wrapping_add(g.dz);
            u = u.wrapping_add(g.du);
            v = v.wrapping_add(g.dv);
        }
    }
}

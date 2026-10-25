//! The warp: a view's panorama, a cube of six 512 x 512 RGB565 pictures,
//! drawn every frame in software as six textured quads (`FUN_0045c030` on
//! the cube @ 0x00661540) through the game's polygon rasteriser.
//!
//! The rasteriser depends on the CPU (`FUN_004d8ed0`): C, P5, MMX, PII and
//! K6-3D routines that are not bit-identical. Two are transcribed operation
//! by operation ([`Variant`]):
//!
//! - `warpPII` (polygon set-up @ 0x0057b3c0, scan conversion @ 0x0057d7c0,
//!   span @ 0x0057dd00, shared with `warpMMX`), which every Intel CPU since
//!   the Pentium II draws with: the reference, and the default. It loads
//!   the x87 control word 0x0e7f (53-bit precision, rounding toward zero)
//!   and leaves it, so the renderer's [`Renderer::fpu`] becomes
//!   [`Fpu::Chop`] at the first polygon it draws, and the rest of the
//!   game's float code (the camera) runs under it from then on;
//! - `warpC` (polygon set-up @ 0x00573650, scan conversion @ 0x00573210,
//!   span @ 0x00573a50), which the original uses on AMD processors and on
//!   CPUs older than the Pentium. It sets no control word and draws the
//!   same pixels as the original under each one ([`Fpu`]). A later option.
//!
//! Their frames differ by one texel in about 4 % of the pixels. Sprites
//! drawn into the faces (SPW) are not drawn yet.

use crate::fpu::{f, ftol, Fpu};

mod pii;

pub use pii::MAX_WIDTH as PII_MAX_WIDTH;

/// The rasteriser: which CPU's routines draw the cube.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variant {
    /// `warpPII`: Intel Pentium II and every later Intel CPU (MMX, the
    /// control word 0x0e7f). The reference.
    #[default]
    PII,
    /// `warpC`: AMD CPUs, CPUs before the Pentium.
    C,
}

/// The width and height of a face.
pub const FACE_SIZE: usize = 512;

/// The cube's corners (FUN_0045c790(512)): ±10 on each axis.
const VERTS: [[f64; 3]; 8] = [
    [-10.0, 10.0, 10.0],
    [10.0, 10.0, 10.0],
    [10.0, -10.0, 10.0],
    [-10.0, -10.0, 10.0],
    [-10.0, 10.0, -10.0],
    [10.0, 10.0, -10.0],
    [10.0, -10.0, -10.0],
    [-10.0, -10.0, -10.0],
];

/// Each face's corners: faces 0 `ft` (z = +10), 1 `bk`, 2 `up` (y = −10),
/// 3 `dn`, 4 `lf` (x = −10), 5 `rt`.
const INDICES: [[usize; 4]; 6] = [
    [0, 1, 2, 3],
    [7, 6, 5, 4],
    [3, 2, 6, 7],
    [4, 5, 1, 0],
    [3, 7, 4, 0],
    [6, 2, 1, 5],
];

/// The faces' texture coordinates: the corners map to 1/512 and 511/512,
/// so a face's 90° spans texels 1..511 (a picture of 2·atan(256/255) =
/// 90.22°). Faces 0, 2, 3 use the first set, 1, 4, 5 the second.
const UV_SET: [usize; 6] = [0, 1, 0, 0, 1, 1];

fn uvs(set: usize) -> [[f64; 2]; 4] {
    let a = f(1.0 / 512.0);
    let b = f(1.0 - a);
    if set == 0 {
        [[a, b], [b, b], [b, a], [a, a]]
    } else {
        [[b, a], [a, a], [a, b], [b, b]]
    }
}

/// u, v → 24.8 texel units (the C variant's scale, FUN_004d8d40).
const TEX_SCALE: f64 = 512.0 * 256.0;

/// The HNM frame k of a view's warp file goes into face `FRAME_TO_FACE[k]`
/// (@ 0x0040cf20): the frames are `bk dn ft lf rt up`.
pub const FRAME_TO_FACE: [usize; 6] = [1, 3, 0, 4, 5, 2];

/// A polygon vertex: screen x, y, 1/z, u, v (floats as stored).
pub(crate) type Vertex = [f64; 5];

/// An edge of a triangle (@ 0x00573510).
#[derive(Clone, Copy, Debug, Default)]
struct Edge {
    ytop: i32,
    ybot: i32,
    x: f64,
    w: f64,
    dx: f64,
    dw: f64,
    u: f64,
    v: f64,
    du: f64,
    dv: f64,
}

/// The cube renderer for one screen size.
pub struct Renderer {
    pub variant: Variant,
    /// The x87 control word: the process's when the frame starts; `warpPII`
    /// sets [`Fpu::Chop`] at its first polygon and leaves it.
    pub fpu: Fpu,
    width: usize,
    height: usize,
    /// `warpC`: spans whose texel index left the face (the original reads
    /// the memory beside it; here the nearest texel). None in any frame
    /// the original was checked on. `warpPII` wraps its texel coordinates
    /// within the face and never leaves it.
    pub out_of_face: u64,
}

/// The x / y / z of a corner times the matrix (FUN_0045c6d0).
fn transform(fpu: Fpu, m: &[f64; 9], v: [f64; 3]) -> [f64; 3] {
    let (add, mul) = (|a, b| fpu.add(a, b), |a, b| fpu.mul(a, b));
    let [x, y, z] = v;
    [
        fpu.f(add(add(mul(z, m[6]), mul(y, m[3])), mul(x, m[0]))),
        fpu.f(add(add(mul(x, m[1]), mul(z, m[7])), mul(y, m[4]))),
        fpu.f(add(add(mul(x, m[2]), mul(z, m[8])), mul(y, m[5]))),
    ]
}

/// A corner projected once per frame (@ 0x0045c1c1, flag 0x80 of its
/// state): w = 1/z, sy = fy·y·w + cy, sx = x·fx·w + cx, each stored.
fn project(
    fpu: Fpu,
    cache: &mut [Option<[f64; 3]>; 8],
    cam: &[[f64; 3]],
    proj: [f64; 4],
    i: usize,
) -> [f64; 3] {
    *cache[i].get_or_insert_with(|| {
        let (add, mul) = (|a, b| fpu.add(a, b), |a, b| fpu.mul(a, b));
        let [p0, p1, p2, p3] = proj;
        let [x, y, z] = cam[i];
        let rz = fpu.f(fpu.div(1.0, z));
        let sy = fpu.f(add(mul(mul(p1, y), rz), p3));
        let sx = fpu.f(add(mul(mul(x, p0), rz), p2));
        [sx, sy, rz]
    })
}

impl Renderer {
    /// A renderer of `variant` for a `width` x `height` screen, the process
    /// running under the control word `fpu`. `warpPII` draws screens up to
    /// [`PII_MAX_WIDTH`] pixels wide, as the original's span buffer does.
    pub fn new(variant: Variant, fpu: Fpu, width: usize, height: usize) -> Renderer {
        Renderer {
            variant,
            fpu,
            width,
            height,
            out_of_face: 0,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    /// The screen size changed (the 800 x 600 mode's surfaces,
    /// `crate::display`).
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Draws the cube: `faces` are the six faces in cube order (0 `ft`,
    /// 1 `bk`, 2 `up`, 3 `dn`, 4 `lf`, 5 `rt`), `matrix` the camera's
    /// ([`crate::camera::Camera::matrix`]), `proj` fx, fy, cx, cy. Every
    /// pixel of `out` (width x height) is written.
    pub fn render(
        &mut self,
        faces: &[Vec<u16>; 6],
        matrix: &[f32; 9],
        proj: [f32; 4],
        out: &mut [u16],
    ) {
        assert!(out.len() >= self.width * self.height);
        assert!(faces.iter().all(|f| f.len() == FACE_SIZE * FACE_SIZE));
        assert!(
            self.variant != Variant::PII || self.width <= pii::MAX_WIDTH,
            "warpPII draws at most {} pixels across",
            pii::MAX_WIDTH
        );
        self.cube(faces, matrix, proj, out);
    }

    /// FUN_0045c030: transform the 8 corners, clip at z = 2, project, one
    /// polygon per face. The control word is read again for each face: the
    /// first polygon `warpPII` draws changes it for the rest of the frame.
    fn cube(&mut self, faces: &[Vec<u16>; 6], matrix: &[f32; 9], proj: [f32; 4], out: &mut [u16]) {
        let m = matrix.map(f64::from);
        let proj = proj.map(f64::from);
        let [p0, p1, p2, p3] = proj;
        let cam: Vec<[f64; 3]> = VERTS.iter().map(|&v| transform(self.fpu, &m, v)).collect();
        let behind: Vec<bool> = cam.iter().map(|c| c[2] < 2.0).collect();
        let mut cache: [Option<[f64; 3]>; 8] = [None; 8];
        for face in 0..6 {
            let fpu = self.fpu;
            let (add, sub, mul) = (
                |a, b| fpu.add(a, b),
                |a, b| fpu.sub(a, b),
                |a, b| fpu.mul(a, b),
            );
            let st = |x| fpu.f(x);
            let idx = INDICES[face];
            let uv = uvs(UV_SET[face]);
            if idx.iter().all(|&i| behind[i]) {
                continue;
            }
            let mut poly: Vec<Vertex> = Vec::with_capacity(8);
            if !idx.iter().any(|&i| behind[i]) {
                for k in 0..4 {
                    let [sx, sy, rz] = project(fpu, &mut cache, &cam, proj, idx[k]);
                    poly.push([sx, sy, rz, uv[k][0], uv[k][1]]);
                }
            } else {
                let (mut prev, mut pk) = (idx[3], 3);
                for k in 0..4 {
                    let cur = idx[k];
                    if !behind[cur] {
                        project(fpu, &mut cache, &cam, proj, cur);
                    }
                    let (pp, cc) = (cam[prev], cam[cur]);
                    let ([up, vp], [uc, vc]) = (uv[pk], uv[k]);
                    if behind[cur] {
                        if !behind[prev] {
                            // @ 0x0045c3fb: previous in front, current behind.
                            let t = fpu.div(sub(2.0, pp[2]), sub(cc[2], pp[2]));
                            let x = add(mul(sub(cc[0], pp[0]), t), pp[0]);
                            let y = st(add(mul(sub(cc[1], pp[1]), t), pp[1]));
                            let sx = st(add(mul(mul(x, p0), 0.5), p2));
                            let sy = st(add(mul(mul(p1, y), 0.5), p3));
                            let dv = st(sub(vc, vp));
                            let du_t = st(mul(sub(uc, up), t));
                            let u = st(add(du_t, up));
                            let v = st(add(mul(t, dv), vp));
                            poly.push([sx, sy, 0.5, u, v]);
                        }
                    } else {
                        if behind[prev] {
                            // @ 0x0045c4ec: previous behind, current in front.
                            let t = fpu.div(sub(2.0, cc[2]), sub(pp[2], cc[2]));
                            let x = add(mul(sub(pp[0], cc[0]), t), cc[0]);
                            let y = st(add(mul(sub(pp[1], cc[1]), t), cc[1]));
                            let sx = st(add(mul(mul(x, p0), 0.5), p2));
                            let sy = st(add(mul(mul(p1, y), 0.5), p3));
                            let dv = st(sub(vp, vc));
                            let du_t = st(mul(sub(up, uc), t));
                            let u = st(add(du_t, uc));
                            let v = st(add(mul(t, dv), vc));
                            poly.push([sx, sy, 0.5, u, v]);
                        }
                        let [sx, sy, rz] = project(fpu, &mut cache, &cam, proj, cur);
                        poly.push([sx, sy, rz, uc, vc]);
                    }
                    prev = cur;
                    pk = k;
                }
            }
            // @ 0x0045c66f: the routine is called for 3 vertices or more.
            if poly.len() > 2 {
                match self.variant {
                    Variant::C => self.polygon(&mut poly, &faces[face], out),
                    Variant::PII => self.polygon_pii(&mut poly, &faces[face], out),
                }
            }
        }
    }

    /// 0x00573650: the polygon as a fan (v₀, vₖ₋₁, vₖ), each triangle
    /// sorted by y (ties: the smaller x first).
    fn polygon(&mut self, poly: &mut [Vertex], tex: &[u16], out: &mut [u16]) {
        let fpu = self.fpu;
        let (add, mul, st) = (|a, b| fpu.add(a, b), |a, b| fpu.mul(a, b), |x| fpu.f(x));
        let prep = |v: &mut Vertex| {
            v[0] = st(add(v[0], 0.5));
            v[1] = st(add(v[1], 0.5));
            v[3] = st(mul(st(mul(v[3], TEX_SCALE)), v[2]));
            v[4] = st(mul(st(mul(v[4], TEX_SCALE)), v[2]));
        };
        prep(&mut poly[0]);
        prep(&mut poly[1]);
        let v0 = poly[0];
        let mut prev = poly[1];
        for vertex in &mut poly[2..] {
            prep(vertex);
            let cur = *vertex;
            // lo (top), hi (bottom), mid.
            let (hi, lo, mid);
            if v0[1] <= prev[1] {
                if cur[1] <= prev[1] {
                    hi = prev;
                    if v0[1] <= cur[1] {
                        (lo, mid) = (v0, cur);
                    } else {
                        (lo, mid) = (cur, v0);
                    }
                } else {
                    (hi, lo, mid) = (cur, v0, prev);
                }
            } else if prev[1] <= cur[1] {
                lo = prev;
                if cur[1] <= v0[1] {
                    (hi, mid) = (v0, cur);
                } else {
                    (hi, mid) = (cur, v0);
                }
            } else {
                (hi, lo, mid) = (v0, cur, prev);
            }
            let (mut top, mut mid) = (lo, mid);
            if mid[1] == lo[1] && mid[0] < lo[0] {
                (top, mid) = (mid, lo);
            }
            let mut bot = hi;
            if mid[1] == hi[1] && mid[0] < hi[0] {
                (bot, mid) = (mid, hi);
            }
            self.triangle(&top, &mid, &bot, tex, out);
            prev = cur;
        }
    }

    /// 0x00573650 (continued): the long edge top→bottom, the upper edge
    /// top→middle, the lower middle→bottom. The left edge is the one with
    /// the smaller dx/dy (flat top: the smaller start x); only the left
    /// edge's w, u, v are stepped.
    fn triangle(&mut self, top: &Vertex, mid: &Vertex, bot: &Vertex, tex: &[u16], out: &mut [u16]) {
        let Some(a) = self.edge(top, bot, false) else {
            return;
        };
        let mut edges = [a, Edge::default(), Edge::default()];
        const A: usize = 0;
        const B: usize = 1;
        const C: usize = 2;
        let (l, rr, second);
        match self.edge(top, mid, false) {
            None => {
                // A flat top: the lower edge and the long one. An empty
                // lower edge with a non-empty long one cannot happen.
                let Some(c) = self.edge(mid, bot, false) else {
                    return;
                };
                edges[C] = c;
                second = None;
                (l, rr) = if edges[C].x <= edges[A].x {
                    (C, A)
                } else {
                    (A, C)
                };
            }
            Some(b) => {
                edges[B] = b;
                (l, rr) = if edges[B].dx <= edges[A].dx {
                    (B, A)
                } else {
                    (A, B)
                };
                let xonly = !(mid[0] <= top[0] || mid[0] <= bot[0]);
                second = match self.edge(mid, bot, xonly) {
                    None => None,
                    Some(c) => {
                        edges[C] = c;
                        if edges[rr].ybot < edges[l].ybot {
                            Some((l, C))
                        } else {
                            Some((C, rr))
                        }
                    }
                };
            }
        }
        self.scan(&mut edges, l, rr, second, tex, out);
    }

    /// 0x00573510: an edge from `a` to `b`, `None` when it covers no row.
    /// Rows run from max(0, trunc(y_top)) to trunc(y_bottom) − 1 (to the
    /// screen's end once at its last row); the start values are taken at
    /// the row's pixel centre.
    fn edge(&self, a: &Vertex, b: &Vertex, xonly: bool) -> Option<Edge> {
        let fpu = self.fpu;
        let (add, sub, mul, f) = (
            |a, b| fpu.add(a, b),
            |a, b| fpu.sub(a, b),
            |a, b| fpu.mul(a, b),
            |x| fpu.f(x),
        );
        if ftol(a[1]) == ftol(b[1]) {
            return None;
        }
        let (a, b) = if b[1] < a[1] { (b, a) } else { (a, b) };
        let mut ytop = ftol(a[1]);
        if ytop >= self.height as i32 - 1 {
            return None;
        }
        let mut ybot = ftol(b[1]);
        if ybot <= 0 {
            return None;
        }
        let inv = f(fpu.div(1.0, sub(b[1], a[1])));
        if ybot >= self.height as i32 - 1 {
            ybot = self.height as i32;
        }
        if ytop < 0 {
            ytop = 0;
        }
        let fy = f(sub(f64::from(ytop + 1), a[1]));
        let sx = mul(sub(b[0], a[0]), inv);
        let mut e = Edge {
            ytop,
            ybot,
            x: f(add(mul(sx, fy), a[0])),
            dx: f(sx),
            ..Edge::default()
        };
        if !xonly {
            let step = |i: usize| {
                let d = mul(sub(b[i], a[i]), inv);
                (f(add(mul(d, fy), a[i])), f(d))
            };
            (e.w, e.dw) = step(2);
            (e.u, e.du) = step(3);
            (e.v, e.dv) = step(4);
        }
        Some(e)
    }

    /// 0x00573210: the gradients across the triangle, from the two edges of
    /// its upper part, then the rows of both parts.
    fn scan(
        &mut self,
        e: &mut [Edge; 3],
        l: usize,
        rr: usize,
        second: Option<(usize, usize)>,
        tex: &[u16],
        out: &mut [u16],
    ) {
        let fpu = self.fpu;
        let (sub, mul, f) = (|a, b| fpu.sub(a, b), |a, b| fpu.mul(a, b), |x| fpu.f(x));
        let inv = fpu.div(1.0, sub(e[rr].dx, e[l].dx));
        let dwdx_d = mul(sub(e[rr].dw, e[l].dw), inv);
        let dwdx = f(dwdx_d);
        let dwdx32 = f(mul(dwdx_d, 32.0));
        let dudx = f(mul(sub(e[rr].du, e[l].du), inv));
        let dvdx = f(mul(sub(e[rr].dv, e[l].dv), inv));
        let g = Gradients {
            dwdx,
            dwdx32,
            dudx,
            dvdx,
            dudx32: f(mul(dudx, 32.0)),
            dvdx32: f(mul(dvdx, 32.0)),
        };
        let mut y = e[l].ytop;
        let mut yend = e[l].ybot.min(e[rr].ybot);
        while y < yend {
            self.row(y, &e[l], &e[rr], &g, tex, out);
            self.step(e, l, rr);
            y += 1;
        }
        let Some((l2, r2)) = second else {
            return;
        };
        while yend < e[l2].ybot {
            self.row(yend, &e[l2], &e[r2], &g, tex, out);
            self.step(e, l2, r2);
            yend += 1;
        }
    }

    fn step(&self, e: &mut [Edge; 3], l: usize, rr: usize) {
        let fpu = self.fpu;
        let add = |a, b| fpu.f(fpu.add(a, b));
        e[rr].x = add(e[rr].dx, e[rr].x);
        let le = &mut e[l];
        le.x = add(le.dx, le.x);
        le.w = add(le.w, le.dw);
        le.u = add(le.u, le.du);
        le.v = add(le.dv, le.v);
    }

    fn row(&mut self, y: i32, l: &Edge, rr: &Edge, g: &Gradients, tex: &[u16], out: &mut [u16]) {
        let xl = ftol(l.x).max(0);
        let xr = ftol(rr.x).min(self.width as i32);
        if xl < xr {
            self.span(y, xl, xr, l, g, tex, out);
        }
    }

    /// 0x00573a50: perspective-correct every 32 pixels, 24.8 fixed-point
    /// steps in between, nearest texel, no clamp.
    #[allow(clippy::too_many_arguments)]
    fn span(
        &mut self,
        y: i32,
        xl: i32,
        xr: i32,
        l: &Edge,
        g: &Gradients,
        tex: &[u16],
        out: &mut [u16],
    ) {
        let fpu = self.fpu;
        let (add, sub, mul, div, f) = (
            |a, b| fpu.add(a, b),
            |a, b| fpu.sub(a, b),
            |a, b| fpu.mul(a, b),
            |a, b| fpu.div(a, b),
            |x| fpu.f(x),
        );
        let mut base = y as usize * self.width + xl as usize;
        let dx0 = sub(f64::from(xl + 1), l.x);
        let dx0f = f(dx0);
        let mut uu = add(mul(dx0, g.dudx), l.u);
        let mut vf = f(add(mul(dx0f, g.dvdx), l.v));
        let mut ww = add(mul(dx0f, g.dwdx), l.w);
        let rw = div(1.0, ww);
        let mut u0 = ftol(mul(rw, uu));
        let mut v0 = ftol(mul(rw, vf));
        let n = xr - xl;
        for _ in 0..n >> 5 {
            ww = add(ww, g.dwdx32);
            uu = add(uu, g.dudx32);
            vf = f(add(g.dvdx32, vf));
            let rw = div(1.0, ww);
            let u1 = ftol(mul(rw, uu));
            let v1 = ftol(mul(rw, vf));
            let du = u1.wrapping_sub(u0) >> 5;
            let dv = v1.wrapping_sub(v0) >> 5;
            self.run(tex, out, base, 32, u0, v0, du, dv);
            base += 32;
            (u0, v0) = (u1, v1);
        }
        let rem = n & 31;
        if rem != 0 {
            let remf = f64::from(rem);
            let invwe = f(div(1.0, add(mul(remf, g.dwdx), ww)));
            let invrem = f(div(1.0, remf));
            let ue = ftol(mul(add(mul(remf, g.dudx), uu), invwe));
            let du = ftol(mul(f64::from(ue.wrapping_sub(u0)), invrem));
            let ve = ftol(mul(add(mul(remf, g.dvdx), vf), invwe));
            let dv = ftol(mul(f64::from(ve.wrapping_sub(v0)), invrem));
            self.run(tex, out, base, rem as usize, u0, v0, du, dv);
        }
    }

    /// `n` pixels from texel (u, v) in 24.8 steps (du, dv):
    /// `face[(v >> 8) * 512 + (u >> 8)]`.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn run(
        &mut self,
        tex: &[u16],
        out: &mut [u16],
        base: usize,
        n: usize,
        u0: i32,
        v0: i32,
        du: i32,
        dv: i32,
    ) {
        let last_u = u0.wrapping_add(du.wrapping_mul(n as i32 - 1));
        let last_v = v0.wrapping_add(dv.wrapping_mul(n as i32 - 1));
        let inside = |u: i32, v: i32| {
            (0..FACE_SIZE as i32).contains(&(u >> 8)) && (0..FACE_SIZE as i32).contains(&(v >> 8))
        };
        let (mut u, mut v) = (u0, v0);
        if inside(u0, v0) && inside(last_u, last_v) {
            for px in &mut out[base..base + n] {
                *px = tex[(v >> 8) as usize * FACE_SIZE + (u >> 8) as usize];
                v = v.wrapping_add(dv);
                u = u.wrapping_add(du);
            }
        } else {
            self.out_of_face += 1;
            for px in &mut out[base..base + n] {
                let tu = (u >> 8).clamp(0, FACE_SIZE as i32 - 1) as usize;
                let tv = (v >> 8).clamp(0, FACE_SIZE as i32 - 1) as usize;
                *px = tex[tv * FACE_SIZE + tu];
                v = v.wrapping_add(dv);
                u = u.wrapping_add(du);
            }
        }
    }
}

/// The plane gradients of a triangle (`warpC`).
struct Gradients {
    dwdx: f64,
    dwdx32: f64,
    dudx: f64,
    dvdx: f64,
    dudx32: f64,
    dvdx32: f64,
}

/// A view's six faces in cube order from the six frames of its warp file
/// (HNM6, 512 x 512), decoded by the shared HNM6 decoder: frame k goes to
/// face [`FRAME_TO_FACE`]\[k\].
pub fn faces_from_hnm(hnm: &[u8]) -> Result<[Vec<u16>; 6], atlantis_codecs::DecodeError> {
    use atlantis_codecs::{DecodeError, HnmVideo};
    let mut video = HnmVideo::new(hnm)?;
    if video.width() != FACE_SIZE || video.height() != FACE_SIZE {
        return Err(DecodeError::InvalidFileHeader);
    }
    let mut faces: [Vec<u16>; 6] = Default::default();
    for &face in &FRAME_TO_FACE {
        let frame = video.next_frame().ok_or(DecodeError::NoVideo)??;
        faces[face] = frame.to_vec();
    }
    Ok(faces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{deg_to_rad, Camera};

    /// Faces whose texel tells the face and where: the face number in the
    /// top 3 bits, then v >> 4 and u >> 4.
    fn tagged() -> [Vec<u16>; 6] {
        std::array::from_fn(|face| {
            (0..FACE_SIZE * FACE_SIZE)
                .map(|i| {
                    let (u, v) = (i % FACE_SIZE, i / FACE_SIZE);
                    ((face as u16) << 13) | (((v >> 4) as u16) << 5) | (u >> 4) as u16
                })
                .collect()
        })
    }

    #[test]
    fn the_centre_looks_at_the_right_face() {
        let faces = tagged();
        let mut out = vec![0u16; 640 * 480];
        // yaw 0 rt, 90 ft, 180 lf, 270 bk; pitch -89 up, 89 dn.
        for variant in [Variant::C, Variant::PII] {
            for (yaw, pitch, face) in [
                (0, 0, 5),
                (90, 0, 0),
                (180, 0, 4),
                (270, 0, 1),
                (90, -89, 2),
                (90, 89, 3),
            ] {
                let mut cam = Camera::new(Fpu::Double, 640, 480);
                cam.enter(deg_to_rad(Fpu::Double, yaw), deg_to_rad(Fpu::Double, pitch));
                let mut r = Renderer::new(variant, Fpu::Double, 640, 480);
                r.render(&faces, &cam.matrix(), cam.projection(), &mut out);
                let centre = out[240 * 640 + 320];
                assert_eq!(usize::from(centre >> 13), face, "yaw {yaw} pitch {pitch}");
                // Upright, the face's centre at the screen's centre.
                if pitch == 0 {
                    assert_eq!((centre >> 5) & 31, 16, "yaw {yaw}");
                    assert_eq!(centre & 31, 16, "yaw {yaw}");
                }
                assert_eq!(r.out_of_face, 0);
            }
        }
    }
}

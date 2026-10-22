//! The warp: a view's panorama, a cube of six 512 x 512 RGB565 pictures,
//! drawn every frame in software as six textured quads (`FUN_0045c030` on
//! the cube @ 0x00661540) through the game's polygon rasteriser.
//!
//! The rasteriser depends on the CPU (`FUN_004d8ed0`): C, P5, MMX, PII and
//! K6-3D routines that are not bit-identical. This is the C one, `warpC`
//! (polygon set-up @ 0x00573650, scan conversion @ 0x00573210, span
//! @ 0x00573a50), which the original uses on AMD processors and on CPUs
//! older than the Pentium; transcribed operation by operation so that it
//! draws the same pixels at either x87 precision ([`Fpu`]).
//!
//! It is the interim renderer of M0, not the reference: the reference is
//! `warpPII` (atlantis2-re decision 0003), which every Intel CPU since the
//! Pentium II draws with, under the control word 0x0e7f it loads (53-bit
//! precision, rounding toward zero). `warpPII` is not transcribed yet; its
//! frames differ from these by one texel in about 4 % of the pixels. Sprites
//! drawn into the faces (SPW) are not drawn yet either.

use crate::fpu::{f, ftol, Fpu};

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
type Vertex = [f64; 5];

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
    pub fpu: Fpu,
    width: usize,
    height: usize,
    /// Spans whose texel index left the face (the original reads the
    /// memory beside it; here the nearest texel). None in any frame the
    /// original was checked on.
    pub out_of_face: u64,
}

/// The x / y / z of a corner times the matrix (FUN_0045c6d0).
fn transform(fpu: Fpu, m: &[f64; 9], v: [f64; 3]) -> [f64; 3] {
    let r = |x: f64| fpu.r(x);
    let [x, y, z] = v;
    [
        f(r(r(r(z * m[6]) + r(y * m[3])) + r(x * m[0]))),
        f(r(r(r(x * m[1]) + r(z * m[7])) + r(y * m[4]))),
        f(r(r(r(x * m[2]) + r(z * m[8])) + r(y * m[5]))),
    ]
}

impl Renderer {
    pub fn new(fpu: Fpu, width: usize, height: usize) -> Renderer {
        Renderer {
            fpu,
            width,
            height,
            out_of_face: 0,
        }
    }

    pub fn width(&self) -> usize {
        self.width
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
        self.cube(faces, matrix, proj, out);
    }

    /// FUN_0045c030: transform the 8 corners, clip at z = 2, project, one
    /// polygon per face.
    fn cube(&mut self, faces: &[Vec<u16>; 6], matrix: &[f32; 9], proj: [f32; 4], out: &mut [u16]) {
        let fpu = self.fpu;
        let r = |x: f64| fpu.r(x);
        let m = matrix.map(f64::from);
        let [p0, p1, p2, p3] = proj.map(f64::from);
        let cam: Vec<[f64; 3]> = VERTS.iter().map(|&v| transform(fpu, &m, v)).collect();
        let behind: Vec<bool> = cam.iter().map(|c| c[2] < 2.0).collect();
        let mut cache: [Option<[f64; 3]>; 8] = [None; 8];
        let mut project = |i: usize| -> [f64; 3] {
            *cache[i].get_or_insert_with(|| {
                let [x, y, z] = cam[i];
                let rz = f(r(1.0 / z));
                let sy = f(r(r(r(p1 * y) * rz) + p3));
                let sx = f(r(r(r(x * p0) * rz) + p2));
                [sx, sy, rz]
            })
        };
        for face in 0..6 {
            let idx = INDICES[face];
            let uv = uvs(UV_SET[face]);
            if idx.iter().all(|&i| behind[i]) {
                continue;
            }
            let mut poly: Vec<Vertex> = Vec::with_capacity(8);
            if !idx.iter().any(|&i| behind[i]) {
                for k in 0..4 {
                    let [sx, sy, rz] = project(idx[k]);
                    poly.push([sx, sy, rz, uv[k][0], uv[k][1]]);
                }
            } else {
                let (mut prev, mut pk) = (idx[3], 3);
                for k in 0..4 {
                    let cur = idx[k];
                    if !behind[cur] {
                        project(cur);
                    }
                    let (pp, cc) = (cam[prev], cam[cur]);
                    let ([up, vp], [uc, vc]) = (uv[pk], uv[k]);
                    if behind[cur] {
                        if !behind[prev] {
                            // @ 0x0045c3fb: previous in front, current behind.
                            let t = r(r(2.0 - pp[2]) / r(cc[2] - pp[2]));
                            let x = r(r(r(cc[0] - pp[0]) * t) + pp[0]);
                            let y = f(r(r(r(cc[1] - pp[1]) * t) + pp[1]));
                            let sx = f(r(r(r(x * p0) * 0.5) + p2));
                            let sy = f(r(r(r(p1 * y) * 0.5) + p3));
                            let dv = f(r(vc - vp));
                            let du_t = f(r(r(uc - up) * t));
                            let u = f(r(du_t + up));
                            let v = f(r(r(t * dv) + vp));
                            poly.push([sx, sy, 0.5, u, v]);
                        }
                    } else {
                        if behind[prev] {
                            // @ 0x0045c4ec: previous behind, current in front.
                            let t = r(r(2.0 - cc[2]) / r(pp[2] - cc[2]));
                            let x = r(r(r(pp[0] - cc[0]) * t) + cc[0]);
                            let y = f(r(r(r(pp[1] - cc[1]) * t) + cc[1]));
                            let sx = f(r(r(r(x * p0) * 0.5) + p2));
                            let sy = f(r(r(r(p1 * y) * 0.5) + p3));
                            let dv = f(r(vp - vc));
                            let du_t = f(r(r(up - uc) * t));
                            let u = f(r(du_t + uc));
                            let v = f(r(r(t * dv) + vc));
                            poly.push([sx, sy, 0.5, u, v]);
                        }
                        let [sx, sy, rz] = project(cur);
                        poly.push([sx, sy, rz, uc, vc]);
                    }
                    prev = cur;
                    pk = k;
                }
            }
            if poly.len() > 2 {
                self.polygon(&mut poly, &faces[face], out);
            }
        }
    }

    /// 0x00573650: the polygon as a fan (v₀, vₖ₋₁, vₖ), each triangle
    /// sorted by y (ties: the smaller x first).
    fn polygon(&mut self, poly: &mut [Vertex], tex: &[u16], out: &mut [u16]) {
        let fpu = self.fpu;
        let r = |x: f64| fpu.r(x);
        let prep = |v: &mut Vertex| {
            v[0] = f(r(v[0] + 0.5));
            v[1] = f(r(v[1] + 0.5));
            v[3] = f(r(f(r(v[3] * TEX_SCALE)) * v[2]));
            v[4] = f(r(f(r(v[4] * TEX_SCALE)) * v[2]));
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
        let r = |x: f64| self.fpu.r(x);
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
        let inv = f(r(1.0 / r(b[1] - a[1])));
        if ybot >= self.height as i32 - 1 {
            ybot = self.height as i32;
        }
        if ytop < 0 {
            ytop = 0;
        }
        let fy = f(r(f64::from(ytop + 1) - a[1]));
        let sx = r(r(b[0] - a[0]) * inv);
        let mut e = Edge {
            ytop,
            ybot,
            x: f(r(r(sx * fy) + a[0])),
            dx: f(sx),
            ..Edge::default()
        };
        if !xonly {
            let step = |i: usize| {
                let d = r(r(b[i] - a[i]) * inv);
                (f(r(r(d * fy) + a[i])), f(d))
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
        let r = |x: f64| self.fpu.r(x);
        let inv = r(1.0 / r(e[rr].dx - e[l].dx));
        let dwdx_d = r(r(e[rr].dw - e[l].dw) * inv);
        let dwdx = f(dwdx_d);
        let dwdx32 = f(r(dwdx_d * 32.0));
        let dudx = f(r(r(e[rr].du - e[l].du) * inv));
        let dvdx = f(r(r(e[rr].dv - e[l].dv) * inv));
        let g = Gradients {
            dwdx,
            dwdx32,
            dudx,
            dvdx,
            dudx32: f(r(dudx * 32.0)),
            dvdx32: f(r(dvdx * 32.0)),
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
        let r = |x: f64| self.fpu.r(x);
        e[rr].x = f(r(e[rr].dx + e[rr].x));
        let le = &mut e[l];
        le.x = f(r(le.dx + le.x));
        le.w = f(r(le.w + le.dw));
        le.u = f(r(le.u + le.du));
        le.v = f(r(le.dv + le.v));
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
        let r = |x: f64| fpu.r(x);
        let mut base = y as usize * self.width + xl as usize;
        let dx0 = r(f64::from(xl + 1) - l.x);
        let dx0f = f(dx0);
        let mut uu = r(r(dx0 * g.dudx) + l.u);
        let mut vf = f(r(r(dx0f * g.dvdx) + l.v));
        let mut ww = r(r(dx0f * g.dwdx) + l.w);
        let rw = r(1.0 / ww);
        let mut u0 = ftol(r(rw * uu));
        let mut v0 = ftol(r(rw * vf));
        let n = xr - xl;
        for _ in 0..n >> 5 {
            ww = r(ww + g.dwdx32);
            uu = r(uu + g.dudx32);
            vf = f(r(g.dvdx32 + vf));
            let rw = r(1.0 / ww);
            let u1 = ftol(r(rw * uu));
            let v1 = ftol(r(rw * vf));
            let du = u1.wrapping_sub(u0) >> 5;
            let dv = v1.wrapping_sub(v0) >> 5;
            self.run(tex, out, base, 32, u0, v0, du, dv);
            base += 32;
            (u0, v0) = (u1, v1);
        }
        let rem = n & 31;
        if rem != 0 {
            let remf = f64::from(rem);
            let invwe = f(r(1.0 / r(r(remf * g.dwdx) + ww)));
            let invrem = f(r(1.0 / remf));
            let ue = ftol(r(r(r(remf * g.dudx) + uu) * invwe));
            let du = ftol(r(f64::from(ue.wrapping_sub(u0)) * invrem));
            let ve = ftol(r(r(r(remf * g.dvdx) + vf) * invwe));
            let dv = ftol(r(f64::from(ve.wrapping_sub(v0)) * invrem));
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

/// The plane gradients of a triangle.
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
            let mut r = Renderer::new(Fpu::Double, 640, 480);
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

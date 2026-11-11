//! The warp drawn on the processor: what the original's OpenGL 1.x draws
//! for the background cube, modelled from the specification.
//!
//! The original (`TeWarp3::draw` @ 0x004f0b50, atlantis4-gog-en-2363910)
//! draws each cell as a textured quad through the camera, depth test off,
//! textures `GL_LINEAR` for both filters with no mipmaps and
//! `GL_CLAMP_TO_EDGE` (@ 0x004ea728..0x004ea776), blending `GL_SRC_ALPHA,
//! GL_ONE_MINUS_SRC_ALPHA` (opaque JPEG texels replace what is behind).
//! So a pixel shows the texture sampled at the point of the cube its
//! centre's ray meets: a planar quad with affine texture coordinates,
//! interpolated perspective-correctly, gives exactly the coordinates of
//! that point. The sample is GL's `GL_LINEAR`: the texel coordinate `c −
//! ½`, its floor and fraction, the four taps weighted by the fractions,
//! the sum rounded to 8 bits. The fraction's precision is the driver's
//! (decision 0002 § 4): [`Weights::Exact`] is the specification's real
//! number, [`Weights::Bits`] a GPU's fixed point.
//!
//! Over the background, [`draw_mesh`] draws an object's or an animation's
//! tile range: the original's `TeMesh::draw` of its triangles (material
//! mode 1, `GL_MODULATE` with white, alpha blended, as a GL trace of the
//! original shows), here per pixel: the ray of the pixel's centre meets
//! the triangle's plane at a point whose barycentric coordinates interpolate
//! the UVs (GL's perspective-correct interpolation), the picture is
//! sampled `GL_LINEAR` at `(u·w, (1 − v)·h)` (texel centres at half
//! integers) and blended over the frame.

use atlantis4_data::anim::TileRange;

use crate::camera::{Camera, WorldDirs};
use crate::cube::{cell_texel, face_point, FACE_PICTURE};
use crate::package::Picture;
use crate::sprite::blend;

/// The precision of the bilinear weights.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weights {
    /// The specification's weights (real numbers, here `f64`).
    Exact,
    /// Weights truncated to `n` fraction bits, as fixed-point hardware.
    Bits(u32),
}

/// GL's `GL_LINEAR` sample of a `FACE_PICTURE` square XRGB picture at
/// (`x`, `y`) (texel centres at integers), clamped to the edge.
pub fn bilinear(pic: &[u32], x: f64, y: f64, weights: Weights) -> u32 {
    let n = FACE_PICTURE as i64;
    let (fx, fy) = (x.floor(), y.floor());
    let (mut a, mut b) = (x - fx, y - fy);
    if let Weights::Bits(bits) = weights {
        let k = f64::from(1u32 << bits);
        a = (a * k).floor() / k;
        b = (b * k).floor() / k;
    }
    let (x0, y0) = (fx as i64, fy as i64);
    let t = if x0 >= 0 && y0 >= 0 && x0 + 1 < n && y0 + 1 < n {
        let o = (y0 * n + x0) as usize;
        let u = n as usize;
        [pic[o], pic[o + 1], pic[o + u], pic[o + u + 1]]
    } else {
        let at = |x: i64, y: i64| pic[(y.clamp(0, n - 1) * n + x.clamp(0, n - 1)) as usize];
        [
            at(x0, y0),
            at(x0 + 1, y0),
            at(x0, y0 + 1),
            at(x0 + 1, y0 + 1),
        ]
    };
    let w = [(1.0 - a) * (1.0 - b), a * (1.0 - b), (1.0 - a) * b, a * b];
    let mut out = 0u32;
    for shift in [16, 8, 0] {
        let ch = |p: u32| f64::from((p >> shift) & 0xff);
        let v = ch(t[0]) * w[0] + ch(t[1]) * w[1] + ch(t[2]) * w[2] + ch(t[3]) * w[3];
        // `(v + ½)` floored and clamped to 0..=255: the saturating cast
        // (NaN to 0 included).
        out |= u32::from((v + 0.5) as u8) << shift;
    }
    out
}

/// The colour of the background at the world direction `d`.
pub fn sample_background(faces: &[Vec<u32>; 6], d: [f64; 3], weights: Weights) -> u32 {
    match face_point(d) {
        Some((f, s, t)) => {
            let (x, y) = cell_texel(s, t);
            bilinear(&faces[f], x, y, weights)
        }
        None => 0,
    }
}

/// The view of `cam` into `out` (`width × height` XRGB8888, rows
/// top-down): each pixel's centre `((i + ½)/W·2 − 1, 1 − (j + ½)/H·2)` in
/// normalised device coordinates, its ray, the background there.
pub fn render(cam: &Camera, faces: &[Vec<u32>; 6], out: &mut [u32], weights: Weights) {
    let (w, h) = (cam.width as usize, cam.height as usize);
    assert!(out.len() >= w * h, "output smaller than {w}x{h}");
    let dirs = cam.world_dirs();
    let cols = columns(&dirs, w);
    let cz = dirs.cam_z();
    rows(&mut out[..w * h], w, h, |j, line| {
        let cy = dirs.cam_y(1.0 - (j as f64 + 0.5) / h as f64 * 2.0);
        for (px, &cx) in line.iter_mut().zip(&cols) {
            *px = sample_background(faces, dirs.turn([cx, cy, cz]), weights);
        }
    });
}

/// Each column's camera-space x, `nx = (i + ½)/W·2 − 1` through
/// [`WorldDirs::cam_x`].
fn columns(dirs: &WorldDirs, w: usize) -> Vec<f64> {
    (0..w)
        .map(|i| dirs.cam_x((i as f64 + 0.5) / w as f64 * 2.0 - 1.0))
        .collect()
}

/// Runs `row(j, line)` for each row of `out` (`w × h`, rows top-down),
/// in bands of rows on the processor's threads when the picture is large
/// (each row's result does not depend on the order).
pub fn rows<T: Send>(out: &mut [T], w: usize, h: usize, row: impl Fn(usize, &mut [T]) + Sync) {
    rows_of(out, w, h, w * h, row);
}

/// [`rows`] with the work counted as `pixels` pixels.
fn rows_of<T: Send>(
    out: &mut [T],
    w: usize,
    h: usize,
    pixels: usize,
    row: impl Fn(usize, &mut [T]) + Sync,
) {
    let out = &mut out[..w * h];
    let threads = threads(pixels);
    if threads <= 1 || h == 0 || w == 0 {
        for (j, line) in out.chunks_mut(w.max(1)).enumerate() {
            row(j, line);
        }
        return;
    }
    let per = h.div_ceil(threads);
    std::thread::scope(|s| {
        for (k, band) in out.chunks_mut(per * w).enumerate() {
            let row = &row;
            s.spawn(move || {
                for (i, line) in band.chunks_mut(w).enumerate() {
                    row(k * per + i, line);
                }
            });
        }
    });
}

/// GL's `GL_LINEAR` sample of an RGBA picture at (`x`, `y`) (texel
/// centres at integers), clamped to the edge, rounded to 8 bits.
pub fn bilinear_rgba(pic: &Picture, x: f64, y: f64, weights: Weights) -> [u8; 4] {
    let (fx, fy) = (x.floor(), y.floor());
    let (mut a, mut b) = (x - fx, y - fy);
    if let Weights::Bits(bits) = weights {
        let k = f64::from(1u32 << bits);
        a = (a * k).floor() / k;
        b = (b * k).floor() / k;
    }
    let (x0, y0) = (fx as i64, fy as i64);
    let t = [
        pic.texel_clamped(x0, y0),
        pic.texel_clamped(x0 + 1, y0),
        pic.texel_clamped(x0, y0 + 1),
        pic.texel_clamped(x0 + 1, y0 + 1),
    ];
    let w = [(1.0 - a) * (1.0 - b), a * (1.0 - b), (1.0 - a) * b, a * b];
    std::array::from_fn(|c| {
        let v = f64::from(t[0][c]) * w[0]
            + f64::from(t[1][c]) * w[1]
            + f64::from(t[2][c]) * w[2]
            + f64::from(t[3][c]) * w[3];
        (v + 0.5) as u8
    })
}

type D3 = [f64; 3];

fn sub3(a: D3, b: D3) -> D3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot3(a: D3, b: D3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: D3, b: D3) -> D3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Per-pixel stamps: a pixel is drawn once per mesh even where two of its
/// triangles share an edge (GL's rasterisation rule covers it once).
pub struct Coverage {
    stamp: Vec<u32>,
    current: u32,
}

impl Coverage {
    pub fn new() -> Coverage {
        Coverage {
            stamp: Vec::new(),
            current: 0,
        }
    }

    fn next(&mut self, pixels: usize) {
        if self.stamp.len() != pixels {
            self.stamp = vec![0; pixels];
            self.current = 0;
        }
        self.current = self.current.wrapping_add(1);
        if self.current == 0 {
            self.stamp.fill(0);
            self.current = 1;
        }
    }
}

impl Default for Coverage {
    fn default() -> Self {
        Self::new()
    }
}

/// The window rectangle (`x0..x1`, `y0..y1`, rows top-down) the triangle
/// `v` may cover, `None` when it is off screen; a triangle with a vertex
/// behind the camera may cover any pixel.
fn triangle_box(cam: &Camera, v: &[[f32; 3]; 3]) -> Option<(i64, i64, i64, i64)> {
    let (w, h) = (i64::from(cam.width), i64::from(cam.height));
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in v {
        let s = cam.project(*p);
        if !(s[2] <= 1.0 && s.iter().all(|c| c.is_finite())) {
            return Some((0, w, 0, h));
        }
        let (x, y) = (f64::from(s[0]), f64::from(cam.height) - f64::from(s[1]));
        lo = [lo[0].min(x), lo[1].min(y)];
        hi = [hi[0].max(x), hi[1].max(y)];
    }
    let x0 = (lo[0].floor() as i64 - 1).max(0);
    let x1 = (hi[0].ceil() as i64 + 1).min(w);
    let y0 = (lo[1].floor() as i64 - 1).max(0);
    let y1 = (hi[1].ceil() as i64 + 1).min(h);
    (x0 < x1 && y0 < y1).then_some((x0, x1, y0, y1))
}

/// Whether any triangle of `range` is on screen (the original decodes and
/// draws a tile only when a triangle of its range is in the frustum,
/// `0x004bb2a0` @ 0x004f0366).
pub fn range_visible(cam: &Camera, range: &TileRange) -> bool {
    let v = |i: u32| range.vertices.get(i as usize).copied().unwrap_or([0.0; 3]);
    range
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .any(|t| triangle_box(cam, &[v(t[0]), v(t[1]), v(t[2])]).is_some())
}

/// A triangle of a mesh, prepared for [`draw_mesh`].
struct Tri {
    bbox: (i64, i64, i64, i64),
    p0: D3,
    e1: D3,
    e2: D3,
    n: D3,
    np0: f64,
    d11: f64,
    d12: f64,
    d22: f64,
    det: f64,
    uvs: [[f64; 2]; 3],
}

/// Draws the triangles of `range` textured with `pic` over `out` (the
/// view of `cam`, XRGB, rows top-down).
pub fn draw_mesh(
    cam: &Camera,
    range: &TileRange,
    pic: &Picture,
    out: &mut [u32],
    cover: &mut Coverage,
    weights: Weights,
) {
    let (w, h) = (cam.width as usize, cam.height as usize);
    cover.next(w * h);
    let (pw, ph) = (f64::from(pic.width), f64::from(pic.height));
    let vert = |i: u32| range.vertices.get(i as usize).copied().unwrap_or([0.0; 3]);
    let uv = |i: u32| range.uvs.get(i as usize).copied().unwrap_or([0.0; 2]);
    let mut tris = Vec::new();
    for t in range.indices.as_chunks::<3>().0 {
        let v = [vert(t[0]), vert(t[1]), vert(t[2])];
        let Some(bbox) = triangle_box(cam, &v) else {
            continue;
        };
        let p: [D3; 3] = v.map(|p| p.map(f64::from));
        let uvs = [uv(t[0]), uv(t[1]), uv(t[2])].map(|q| q.map(f64::from));
        let (e1, e2) = (sub3(p[1], p[0]), sub3(p[2], p[0]));
        let n = cross3(e1, e2);
        let (d11, d12, d22) = (dot3(e1, e1), dot3(e1, e2), dot3(e2, e2));
        let det = d11 * d22 - d12 * d12;
        if det == 0.0 {
            continue;
        }
        tris.push(Tri {
            bbox,
            p0: p[0],
            e1,
            e2,
            n,
            np0: dot3(n, p[0]),
            d11,
            d12,
            d22,
            det,
            uvs,
        });
    }
    if tris.is_empty() {
        return;
    }
    let (y_lo, y_hi) = tris.iter().fold((i64::MAX, i64::MIN), |(lo, hi), t| {
        (lo.min(t.bbox.2), hi.max(t.bbox.3))
    });
    let current = cover.current;
    let dirs = cam.world_dirs();
    let cols = columns(&dirs, w);
    let cz = dirs.cam_z();
    // Rows are independent: each runs every triangle in order over its
    // own pixels and stamps.
    let (y_lo, y_hi) = (y_lo as usize, y_hi as usize);
    let mut lines: Vec<(&mut [u32], &mut [u32])> = out[y_lo * w..y_hi * w]
        .chunks_mut(w)
        .zip(cover.stamp[y_lo * w..y_hi * w].chunks_mut(w))
        .collect();
    let n_rows = lines.len();
    rows_of(&mut lines, 1, n_rows, n_rows * w, |r, slot| {
        let (line, stamp) = &mut slot[0];
        let j = (y_lo + r) as i64;
        for t in &tris {
            let (x0, x1, y0, y1) = t.bbox;
            if j < y0 || j >= y1 {
                continue;
            }
            let cy = dirs.cam_y(1.0 - (j as f64 + 0.5) / h as f64 * 2.0);
            for i in x0..x1 {
                let k = i as usize;
                if stamp[k] == current {
                    continue;
                }
                let d = dirs.turn([cols[k], cy, cz]);
                let den = dot3(t.n, d);
                if den == 0.0 {
                    continue;
                }
                let s = t.np0 / den;
                if s <= 0.0 {
                    continue;
                }
                let q = sub3([d[0] * s, d[1] * s, d[2] * s], t.p0);
                let (q1, q2) = (dot3(q, t.e1), dot3(q, t.e2));
                let b1 = (t.d22 * q1 - t.d12 * q2) / t.det;
                let b2 = (t.d11 * q2 - t.d12 * q1) / t.det;
                let b0 = 1.0 - b1 - b2;
                if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                    continue;
                }
                stamp[k] = current;
                let uvs = &t.uvs;
                let u = b0 * uvs[0][0] + b1 * uvs[1][0] + b2 * uvs[2][0];
                let vv = b0 * uvs[0][1] + b1 * uvs[1][1] + b2 * uvs[2][1];
                let texel = bilinear_rgba(pic, u * pw - 0.5, (1.0 - vv) * ph - 0.5, weights);
                line[k] = blend(line[k], texel);
            }
        }
    });
}

/// How many threads draw a picture of `pixels` pixels.
pub(crate) fn threads(pixels: usize) -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = pixels;
        1
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if pixels < 100_000 {
            return 1;
        }
        std::thread::available_parallelism().map_or(1, |n| n.get().min(8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texel_centres_return_the_texel_and_halfway_rounds_up() {
        let mut pic = vec![0u32; FACE_PICTURE * FACE_PICTURE];
        pic[0] = 0x00ff_ff01;
        pic[1] = 0x0000_0000;
        assert_eq!(bilinear(&pic, 0.0, 0.0, Weights::Exact), 0x00ff_ff01);
        // Halfway between 255 and 0 is 127.5: rounded to 128; 0.5 to 1.
        assert_eq!(bilinear(&pic, 0.5, 0.0, Weights::Exact), 0x0080_8001);
        // Clamped to the edge.
        assert_eq!(bilinear(&pic, -3.0, -3.0, Weights::Exact), 0x00ff_ff01);
        // 8-bit weights: a quarter is exact; 0.999 truncated to 255/256
        // leaves 1/256 of the first texel.
        assert_eq!(bilinear(&pic, 0.25, 0.0, Weights::Bits(8)), 0x00bf_bf01);
        assert_eq!(bilinear(&pic, 0.999, 0.0, Weights::Exact), 0);
        assert_eq!(bilinear(&pic, 0.999, 0.0, Weights::Bits(8)), 0x0001_0100);
    }
}

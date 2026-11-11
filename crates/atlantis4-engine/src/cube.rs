//! The cube a node is drawn on: side 1000 around the camera, six faces
//! numbered as the XML `block face` (the block code @ 0x004ef630,
//! atlantis4-gog-en-2363910), each a frame `P = C + s·U + t·V`, `s, t ∈
//! [0, 1]`:
//!
//! | face | plane | C | U | V |
//! |---|---|---|---|---|
//! | 0 | y = +500 | (−500, 500, −500) | (+1000, 0, 0) | (0, 0, +1000) |
//! | 1 | y = −500 | (−500, −500, 500) | (+1000, 0, 0) | (0, 0, −1000) |
//! | 2 | z = −500 | (−500, −500, −500) | (+1000, 0, 0) | (0, +1000, 0) |
//! | 3 | z = +500 | (500, −500, 500) | (−1000, 0, 0) | (0, +1000, 0) |
//! | 4 | x = −500 | (−500, −500, 500) | (0, 0, −1000) | (0, +1000, 0) |
//! | 5 | x = +500 | (500, −500, −500) | (0, 0, +1000) | (0, +1000, 0) |
//!
//! The background is 13 × 13 cells per face, each one quad of `anim.anim`
//! textured from a 64 × 64 slot of one of the package's JPEG atlases: 60 ×
//! 60 texels at (2, 2) of the slot, the texture's `v` counted from the
//! bottom row of the picture (the JPEG decoders store the rows bottom-up,
//! @ 0x004b1d5a, and the texture matrix built @ 0x004ea9b0..0x004eab6b
//! only scales the picture's part of its power-of-two texture). `u` grows
//! with `s` and `v` with `t` on every face. Objects replace whole cells;
//! animations use a 14 × 14 grid (not drawn yet).
//!
//! The clicks use a third grid: 16 × 16 blocks per face.

use atlantis4_data::anim::Anim;

/// A face's frame: corner `c`, edges `u` (s) and `v` (t).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub c: [f32; 3],
    pub u: [f32; 3],
    pub v: [f32; 3],
}

/// The six faces (`0x004ef630`'s jump table @ 0x004ef920).
pub const FACES: [Face; 6] = [
    Face {
        c: [-500.0, 500.0, -500.0],
        u: [1000.0, 0.0, 0.0],
        v: [0.0, 0.0, 1000.0],
    },
    Face {
        c: [-500.0, -500.0, 500.0],
        u: [1000.0, 0.0, 0.0],
        v: [0.0, 0.0, -1000.0],
    },
    Face {
        c: [-500.0, -500.0, -500.0],
        u: [1000.0, 0.0, 0.0],
        v: [0.0, 1000.0, 0.0],
    },
    Face {
        c: [500.0, -500.0, 500.0],
        u: [-1000.0, 0.0, 0.0],
        v: [0.0, 1000.0, 0.0],
    },
    Face {
        c: [-500.0, -500.0, 500.0],
        u: [0.0, 0.0, -1000.0],
        v: [0.0, 1000.0, 0.0],
    },
    Face {
        c: [500.0, -500.0, -500.0],
        u: [0.0, 0.0, 1000.0],
        v: [0.0, 1000.0, 0.0],
    },
];

/// The background's cells per face side.
pub const CELLS: usize = 13;
/// A cell's slot in its atlas, and its texels inside the slot.
pub const SLOT: usize = 64;
pub const CELL_TEXELS: usize = 60;
pub const CELL_GUTTER: usize = 2;
/// A face picture: the 13 × 13 slots, gutters included.
pub const FACE_PICTURE: usize = CELLS * SLOT;

/// The face a direction (or point) meets, by its largest component.
pub fn face_of(d: [f64; 3]) -> usize {
    let a = d.map(f64::abs);
    if a[1] >= a[0] && a[1] >= a[2] {
        if d[1] > 0.0 {
            0
        } else {
            1
        }
    } else if a[2] >= a[0] {
        if d[2] < 0.0 {
            2
        } else {
            3
        }
    } else if d[0] < 0.0 {
        4
    } else {
        5
    }
}

/// The face a direction meets and its `(s, t)` there, or `None` for no
/// direction (zero, not finite).
pub fn face_point(d: [f64; 3]) -> Option<(usize, f64, f64)> {
    let f = face_of(d);
    let axis = match f {
        0 | 1 => 1,
        2 | 3 => 2,
        _ => 0,
    };
    let k = 500.0 / d[axis].abs();
    if !k.is_finite() {
        return None;
    }
    let p = d.map(|c| c * k);
    let face = &FACES[f];
    let st = |e: [f32; 3]| {
        let mut s = 0.0;
        for i in 0..3 {
            s += (p[i] - f64::from(face.c[i])) * f64::from(e[i]);
        }
        s / 1e6
    };
    let (s, t) = (st(face.u), st(face.v));
    (s.is_finite() && t.is_finite()).then_some((f, s, t))
}

/// Where a background cell's texels are: atlas (tile index), slot corner
/// in the picture's top-down rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSlot {
    pub atlas: u16,
    pub x0: u16,
    pub y0: u16,
}

/// The background of a node: every cell's slot.
#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    /// `[face][t cell][s cell]`.
    pub cells: [[[CellSlot; CELLS]; CELLS]; 6],
}

/// Why an `anim.anim` is not a background.
#[derive(Clone, Debug, PartialEq)]
pub enum CubeError {
    /// A quad not on a face of the cube, or not one cell.
    Quad(String),
    /// A cell's texels not 60 × 60 at (2, 2) of a 64-aligned slot.
    Slot(String),
    /// Not 1014 distinct cells.
    Cells(usize),
}

impl std::fmt::Display for CubeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CubeError::Quad(s) => write!(f, "quad not a cube cell: {s}"),
            CubeError::Slot(s) => write!(f, "cell texels not a slot: {s}"),
            CubeError::Cells(n) => write!(f, "{n} distinct cells, not 1014"),
        }
    }
}

impl std::error::Error for CubeError {}

/// The face whose plane holds every point of `v`.
fn plane_face(v: &[[f32; 3]]) -> Option<usize> {
    for axis in 0..3 {
        let c = v[0][axis];
        if c.abs() == 500.0 && v.iter().all(|p| p[axis] == c) {
            return Some(match (axis, c > 0.0) {
                (1, true) => 0,
                (1, false) => 1,
                (2, false) => 2,
                (2, true) => 3,
                (0, false) => 4,
                _ => 5,
            });
        }
    }
    None
}

/// `(s, t)` of a point of face `f`.
pub fn st_of(f: usize, p: [f32; 3]) -> (f64, f64) {
    let face = &FACES[f];
    let dot = |e: [f32; 3]| {
        (0..3)
            .map(|i| (f64::from(p[i]) - f64::from(face.c[i])) * f64::from(e[i]))
            .sum::<f64>()
            / 1e6
    };
    (dot(face.u), dot(face.v))
}

impl Background {
    /// The cells of a background's `anim.anim`, whose tile `i` has the
    /// picture size `sizes[i]` (width, height).
    pub fn from_anim(anim: &Anim, sizes: &[(u32, u32)]) -> Result<Background, CubeError> {
        let none = CellSlot {
            atlas: u16::MAX,
            x0: 0,
            y0: 0,
        };
        let mut cells = [[[none; CELLS]; CELLS]; 6];
        let mut count = 0;
        for (ti, tile) in anim.tiles.iter().enumerate() {
            let (w, h) = sizes
                .get(ti)
                .map(|&(w, h)| (f64::from(w), f64::from(h)))
                .ok_or_else(|| CubeError::Slot(format!("tile {ti} has no picture")))?;
            for r in &tile.ranges {
                for q in r.indices.chunks(6) {
                    let mut idx: Vec<usize> = q.iter().map(|&i| i as usize).collect();
                    idx.sort_unstable();
                    idx.dedup();
                    let vs: Vec<[f32; 3]> = idx.iter().map(|&i| r.vertices[i]).collect();
                    let uvs: Vec<[f32; 2]> = idx.iter().map(|&i| r.uvs[i]).collect();
                    let f = plane_face(&vs)
                        .ok_or_else(|| CubeError::Quad(format!("{} {vs:?}", tile.name)))?;
                    let sts: Vec<(f64, f64)> = vs.iter().map(|&p| st_of(f, p)).collect();
                    let (smin, smax) = minmax(sts.iter().map(|p| p.0));
                    let (tmin, tmax) = minmax(sts.iter().map(|p| p.1));
                    let n = CELLS as f64;
                    let (ci, cj) = ((smin * n).round(), (tmin * n).round());
                    let one = |a: f64, b: f64, c: f64| ((b - a) * n - 1.0).abs() < 1e-4 && c >= 0.0;
                    if !one(smin, smax, ci) || !one(tmin, tmax, cj) || ci >= n || cj >= n {
                        return Err(CubeError::Quad(format!("{} {vs:?}", tile.name)));
                    }
                    // The UVs grow with s and t.
                    for (k, uv) in uvs.iter().enumerate() {
                        let ds = sts[k].0 - smin;
                        let dt = sts[k].1 - tmin;
                        let (u0, v0) = uvs
                            .iter()
                            .zip(&sts)
                            .find(|(_, st)| st.0 == smin && st.1 == tmin)
                            .map(|(uv, _)| (uv[0], uv[1]))
                            .unwrap_or((uv[0], uv[1]));
                        let du = (f64::from(uv[0]) - f64::from(u0)) * w;
                        let dv = (f64::from(uv[1]) - f64::from(v0)) * h;
                        let want = |d: f64| d * n * CELL_TEXELS as f64;
                        if (du - want(ds)).abs() > 1e-3 || (dv - want(dt)).abs() > 1e-3 {
                            return Err(CubeError::Slot(format!("{} orientation", tile.name)));
                        }
                    }
                    let (umin, _) = minmax(uvs.iter().map(|uv| f64::from(uv[0]) * w));
                    let (_, vmax) = minmax(uvs.iter().map(|uv| f64::from(uv[1]) * h));
                    let x0 = umin - CELL_GUTTER as f64;
                    let y0 = h - vmax - CELL_GUTTER as f64;
                    let slot = |x: f64| {
                        let r = x.round();
                        ((x - r).abs() < 1e-3 && r >= 0.0 && (r as usize).is_multiple_of(SLOT))
                            .then_some(r as u16)
                    };
                    let (Some(x0), Some(y0)) = (slot(x0), slot(y0)) else {
                        return Err(CubeError::Slot(format!("{} at {x0} {y0}", tile.name)));
                    };
                    let cell = &mut cells[f][cj as usize][ci as usize];
                    if cell.atlas == u16::MAX {
                        count += 1;
                    }
                    *cell = CellSlot {
                        atlas: ti as u16,
                        x0,
                        y0,
                    };
                }
            }
        }
        if count != 6 * CELLS * CELLS {
            return Err(CubeError::Cells(count));
        }
        Ok(Background { cells })
    }
}

fn minmax(it: impl Iterator<Item = f64>) -> (f64, f64) {
    it.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
        (a.min(x), b.max(x))
    })
}

/// A decoded atlas: RGB, rows top-down.
pub struct Atlas<'a> {
    pub width: usize,
    pub height: usize,
    pub rgb: &'a [u8],
}

/// The six face pictures, `FACE_PICTURE` square each, XRGB8888, rows
/// top-down: the slot of cell `(ci, cj)` at `(64·ci, 64·(12 − cj))`.
pub fn face_pictures(bg: &Background, atlases: &[Atlas<'_>]) -> [Vec<u32>; 6] {
    std::array::from_fn(|f| {
        let mut out = vec![0u32; FACE_PICTURE * FACE_PICTURE];
        for cj in 0..CELLS {
            for ci in 0..CELLS {
                let slot = bg.cells[f][cj][ci];
                let a = &atlases[slot.atlas as usize];
                for y in 0..SLOT {
                    let sy = slot.y0 as usize + y;
                    let dy = SLOT * (CELLS - 1 - cj) + y;
                    for x in 0..SLOT {
                        let sx = slot.x0 as usize + x;
                        let px = if sx < a.width && sy < a.height {
                            let o = 3 * (sy * a.width + sx);
                            u32::from(a.rgb[o]) << 16
                                | u32::from(a.rgb[o + 1]) << 8
                                | u32::from(a.rgb[o + 2])
                        } else {
                            0
                        };
                        out[dy * FACE_PICTURE + SLOT * ci + x] = px;
                    }
                }
            }
        }
        out
    })
}

/// The point of the face picture (texel centres at integers, rows
/// top-down) that a GL quad of cell `(⌊13 s⌋, ⌊13 t⌋)` samples at `(s,
/// t)`: `(64·ci + 1.5 + 60·a', 64·(12 − cj) + 1.5 + 60·(1 − b'))` with
/// `a' = 13 s − ci`, `b' = 13 t − cj`.
pub fn cell_texel(s: f64, t: f64) -> (f64, f64) {
    let n = CELLS as f64;
    let ci = (s * n).floor().clamp(0.0, n - 1.0);
    let cj = (t * n).floor().clamp(0.0, n - 1.0);
    let (a, b) = (s * n - ci, t * n - cj);
    let g = CELL_GUTTER as f64 - 0.5;
    let k = CELL_TEXELS as f64;
    let slot = SLOT as f64;
    (
        slot * ci + g + k * a,
        slot * (n - 1.0 - cj) + g + k * (1.0 - b),
    )
}

/// The exits' click blocks per face side (`nbWarpBlock`, 16 in the data).
pub const BLOCKS: u32 = 16;

/// The two triangles of click block (`ox`, `oy`) of face `f` on a grid of
/// `nb` (`0x004ef630`): `P = C + ((ox + i)/nb)·U + ((oy + j)/nb)·V` at
/// `(0,0), (1,0), (0,1)` and `(1,0), (1,1), (0,1)`.
pub fn block_triangles(f: usize, ox: u32, oy: u32, nb: u32) -> [[[f32; 3]; 3]; 2] {
    let face = &FACES[f];
    let step = 1000.0 / nb as f32;
    let p = |i: u32, j: u32| -> [f32; 3] {
        let (a, b) = ((ox + i) as f32 * step, (oy + j) as f32 * step);
        std::array::from_fn(|k| face.c[k] + a * face.u[k] / 1000.0 + b * face.v[k] / 1000.0)
    };
    [[p(0, 0), p(1, 0), p(0, 1)], [p(1, 0), p(1, 1), p(0, 1)]]
}

/// The click block of a point (or direction) of the cube.
pub fn block_of(d: [f64; 3], nb: u32) -> Option<(usize, u32, u32)> {
    let (f, s, t) = face_point(d)?;
    let n = f64::from(nb);
    let c = |x: f64| ((x * n).floor().max(0.0) as u32).min(nb - 1);
    Some((f, c(s), c(t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_face_centre_is_its_middle() {
        for (f, d) in [
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 1.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ]
        .into_iter()
        .enumerate()
        {
            let (g, s, t) = face_point(d).unwrap();
            assert_eq!((g, s, t), (f, 0.5, 0.5));
        }
        assert_eq!(face_point([0.0; 3]), None);
    }

    #[test]
    fn a_cell_samples_inside_its_slot() {
        // The corners of cell (0, 0): texel 1.5 .. 61.5 of the bottom-left
        // slot (rows 768 .. 831).
        assert_eq!(cell_texel(0.0, 0.0), (1.5, 768.0 + 61.5));
        let (x, y) = cell_texel(1.0 / 13.0 - 1e-12, 1.0 / 13.0 - 1e-12);
        assert!((x - 61.5).abs() < 1e-6 && (y - 768.0 - 1.5).abs() < 1e-6);
        assert_eq!(cell_texel(1.0, 1.0), (12.0 * 64.0 + 61.5, 1.5));
    }
}

//! Clicks on the warp (atlantis4-gog-en-2363910).
//!
//! - A click is a mouse-up **less than 20 pixels** from its mouse-down
//!   (`GameWarp` mouse-up @ 0x00523a80, `fcomp 20.0` @ 0x00523b04;
//!   `TeWarp3`'s click @ 0x004f3bb0 repeats the test with ≤ 20).
//! - Its ray (`getRay` of the mouse-up point, y from the bottom: `H −
//!   y_window`, @ 0x0050acd7) is tested first against the clickable
//!   animations **from the last in the draw list to the first**, each with
//!   the pick mesh of its current frame; the first hit wins
//!   (@ 0x004f3d90..0x004f4045).
//! - Else the exits' block triangles, in XML order; the **last** exit hit
//!   with `0 ≤ t < FLT_MAX` wins (`0x00552400`, no nearest-hit sort).
//! - The triangle test is `0x005520f0` (plane hit then barycentric
//!   coordinates, edges inclusive), in floats.

use atlantis4_data::scene::Exit;

use crate::camera::Camera;
use crate::cube::{block_triangles, BLOCKS};

/// The pointer's largest move between mouse-down and mouse-up for a
/// click, exclusive (@ 0x00523b04).
pub const CLICK_DISTANCE: f32 = 20.0;

type V3 = [f32; 3];

// The x87 code computes each value in extended precision and stores it as
// a float: here in double precision, each store rounded to `f32`.
fn sub(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| (f64::from(a[i]) - f64::from(b[i])) as f32)
}

fn dot(a: V3, b: V3) -> f32 {
    let m = |i: usize| f64::from(a[i]) * f64::from(b[i]);
    (m(0) + m(1) + m(2)) as f32
}

fn cross(a: V3, b: V3) -> V3 {
    let m = |i: usize, j: usize| f64::from(a[i]) * f64::from(b[j]);
    [
        (m(1, 2) - m(2, 1)) as f32,
        (m(2, 0) - m(0, 2)) as f32,
        (m(0, 1) - m(1, 0)) as f32,
    ]
}

/// What `0x005520f0` returns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// −1: the triangle has no area.
    Degenerate,
    /// 0: missed, or behind the origin.
    Miss,
    /// 1: hit at ray parameter `t`.
    Hit(f32),
    /// 2: the ray lies in the triangle's plane.
    InPlane,
}

/// `0x005520f0`: the ray `o + t·d` against the triangle `v`.
pub fn ray_triangle(o: V3, d: V3, v: &[V3; 3]) -> Hit {
    let u = sub(v[1], v[0]);
    let w = sub(v[2], v[0]);
    let n = cross(u, w);
    if n == [0.0; 3] {
        return Hit::Degenerate;
    }
    let w0 = sub(o, v[0]);
    let a = -dot(n, w0);
    let b = dot(n, d);
    // 1e-9 as a float (@ 0x005bec58).
    if b.abs() < 1e-9_f32 {
        return if a == 0.0 { Hit::InPlane } else { Hit::Miss };
    }
    let r = (f64::from(a) / f64::from(b)) as f32;
    if r < 0.0 {
        return Hit::Miss;
    }
    let rd: V3 = std::array::from_fn(|i| (f64::from(r) * f64::from(d[i])) as f32);
    let p: V3 = std::array::from_fn(|i| (f64::from(o[i]) + f64::from(rd[i])) as f32);
    let (uu, uv, vv) = (dot(u, u), dot(u, w), dot(w, w));
    let q = sub(p, v[0]);
    let (qu, qv) = (dot(q, u), dot(q, w));
    let f = f64::from;
    let den = f(uv) * f(uv) - f(uu) * f(vv);
    let den = den as f32;
    let s = ratio(f(uv) * f(qv), f(vv) * f(qu), den);
    if !(0.0..=1.0).contains(&s) {
        return Hit::Miss;
    }
    let t = ratio(f(uv) * f(qu), f(uu) * f(qv), den);
    if t < 0.0 || f(s) + f(t) > 1.0 {
        return Hit::Miss;
    }
    Hit::Hit(r)
}

/// `(a − b) / den` stored as a float, `a` and `b` exact products of
/// floats: the x87's extended difference is kept exact here (the error of
/// the double difference added back) before the division.
fn ratio(a: f64, b: f64, den: f32) -> f32 {
    let d = a - b;
    let bb = d - a;
    let err = (a - (d - bb)) + (-b - bb);
    let den = f64::from(den);
    (d / den + err / den) as f32
}

/// An exit's click mesh (`0x004f5340`): named with its `linkedWarp`, two
/// triangles per block.
#[derive(Clone, Debug, PartialEq)]
pub struct ExitMesh {
    pub linked_warp: String,
    pub triangles: Vec<[V3; 3]>,
}

impl ExitMesh {
    pub fn new(exit: &Exit) -> ExitMesh {
        let nb = if exit.nb_warp_block == 0 {
            BLOCKS
        } else {
            exit.nb_warp_block
        };
        let triangles = exit
            .blocks
            .iter()
            .filter(|b| b.face < 6)
            .flat_map(|b| block_triangles(b.face as usize, b.x, b.y, nb))
            .collect();
        ExitMesh {
            linked_warp: exit.linked_warp.clone(),
            triangles,
        }
    }

    /// Whether a triangle of the mesh is hit with `0 ≤ t < FLT_MAX`.
    pub fn hit(&self, o: V3, d: V3) -> bool {
        self.triangles
            .iter()
            .any(|t| matches!(ray_triangle(o, d, t), Hit::Hit(r) if (0.0..f32::MAX).contains(&r)))
    }
}

/// A clickable animation's pick mesh for its current frame (the meshes of
/// `anim.anim`; the test `0x0053d790` is taken to be `0x005520f0` over its
/// triangles, not walked yet).
#[derive(Clone, Debug, PartialEq)]
pub struct AnimPick {
    pub name: String,
    pub clickable: bool,
    pub triangles: Vec<[V3; 3]>,
}

/// What a click hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// An animation or object, by name (`OnWarpObjectHit`).
    Anim(String),
    /// An exit, by its `linkedWarp`.
    Exit(String),
}

/// Whether a mouse-up at `up` after a mouse-down at `down` (pixels) is a
/// click.
pub fn is_click(down: (f32, f32), up: (f32, f32)) -> bool {
    let (dx, dy) = (up.0 - down.0, up.1 - down.1);
    (dx * dx + dy * dy).sqrt() < CLICK_DISTANCE
}

/// The click at window pixel (`x`, `y`) (y from the top), with the draw
/// list's animations `anims` (draw order; `background` is skipped) and the
/// exits in XML order.
pub fn click(
    cam: &Camera,
    x: i32,
    y: i32,
    anims: &[AnimPick],
    exits: &[ExitMesh],
) -> Option<Target> {
    let d = cam.ray(x, cam.height as i32 - y);
    let o = [0.0f32; 3];
    for a in anims.iter().rev() {
        if a.name == "background" || !a.clickable {
            continue;
        }
        if a.triangles
            .iter()
            .any(|t| matches!(ray_triangle(o, d, t), Hit::Hit(_)))
        {
            return Some(Target::Anim(a.name.clone()));
        }
    }
    // The last exit hit wins.
    exits
        .iter()
        .rev()
        .find(|e| e.hit(o, d))
        .map(|e| Target::Exit(e.linked_warp.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_are_inside() {
        let t = [[0.0, 0.0, -1.0], [1.0, 0.0, -1.0], [0.0, 1.0, -1.0]];
        assert_eq!(ray_triangle([0.0; 3], [0.0, 0.0, -1.0], &t), Hit::Hit(1.0));
        assert_eq!(ray_triangle([0.0; 3], [0.5, 0.5, -1.0], &t), Hit::Hit(1.0));
        assert_eq!(ray_triangle([0.0; 3], [0.0, 0.0, 1.0], &t), Hit::Miss);
        assert_eq!(ray_triangle([0.0; 3], [1.0, 0.0, 0.0], &t), Hit::Miss);
        let flat = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        assert_eq!(
            ray_triangle([0.0; 3], [0.0, 0.0, 1.0], &flat),
            Hit::Degenerate
        );
    }

    #[test]
    fn twenty_pixels_is_a_drag() {
        assert!(is_click((0.0, 0.0), (12.0, 15.9)));
        assert!(!is_click((0.0, 0.0), (12.0, 16.0)));
    }
}

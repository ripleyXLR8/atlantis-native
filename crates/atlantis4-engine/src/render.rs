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
//! Not drawn yet: objects and animations over the background, the
//! markers' sprites (placed by [`crate::markers`]).

use crate::camera::Camera;
use crate::cube::{cell_texel, face_point, FACE_PICTURE};

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
    let at = |x: i64, y: i64| pic[(y.clamp(0, n - 1) * n + x.clamp(0, n - 1)) as usize];
    let t = [
        at(x0, y0),
        at(x0 + 1, y0),
        at(x0, y0 + 1),
        at(x0 + 1, y0 + 1),
    ];
    let w = [(1.0 - a) * (1.0 - b), a * (1.0 - b), (1.0 - a) * b, a * b];
    let mut out = 0u32;
    for shift in [16, 8, 0] {
        let v: f64 = t
            .iter()
            .zip(&w)
            .map(|(&p, &w)| f64::from((p >> shift) & 0xff) * w)
            .sum();
        let c = (v + 0.5).floor().clamp(0.0, 255.0) as u32;
        out |= c << shift;
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
    let row = |j: usize, line: &mut [u32]| {
        let ny = 1.0 - (j as f64 + 0.5) / h as f64 * 2.0;
        for (i, px) in line.iter_mut().enumerate() {
            let nx = (i as f64 + 0.5) / w as f64 * 2.0 - 1.0;
            let d = cam.to_world(cam.camera_dir(nx, ny));
            *px = sample_background(faces, d, weights);
        }
    };
    let out = &mut out[..w * h];
    let threads = threads(w * h);
    if threads <= 1 || h == 0 {
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

fn threads(pixels: usize) -> usize {
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

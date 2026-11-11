//! 2-D sprites over the warp: the markers (the 480 × 320 layer, origin
//! bottom-left) and the cursor (the 1365 × 768 GUI layer, origin at the
//! window centre, y down), drawn after the warp in that order
//! `[CONFIRMED-DYN]` (atlantis4-re experiment 05: warp → markers → GUI →
//! cursor), each in material mode 1 (`GL_MODULATE`, white) with alpha
//! blending `GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA` and `GL_LINEAR`
//! textures clamped to the edge.
//!
//! Both layers are stretched to the window, and both sprites keep their
//! own pixel size, so a sprite is an axis-aligned `w × h` quad in window
//! pixels at a possibly fractional corner; each window pixel whose centre
//! falls inside samples the picture at its centre (texel centres at
//! integers), bilinear.
//!
//! The picture sits in a power-of-two texture **bottom row first**, as the
//! background atlases (warp.md § 5): beyond its left column and its bottom
//! row the taps clamp to the edge; beyond its right column and its top row
//! they reach the texture's padding, taken here as transparent black
//! (`[UNKNOWN]`: what the padding holds is not read).

use crate::package::Picture;

/// The cursor picture (`mouseCursor`, string @ 0x005b9920 pushed
/// @ 0x0057ba0e, atlantis4-gog-en-2363910).
pub const CURSOR_PATH: &str = "2D/arrow6.png";

/// The cursor layout's anchor: `setAnchor(0.3, 0.1, 0)` (slot `+0x38`,
/// floats @ 0x005bee8c and 0x005bec60, called @ 0x0057baf0): the point
/// of the arrow (0 = left / top) put at the cursor's position.
pub const CURSOR_ANCHOR: (f32, f32) = (0.3, 0.1);

/// A tap of `pic` with the padding rule above (`x` from the left, `y`
/// from the top, texel centres at integers).
#[inline]
fn tap(pic: &Picture, x: i64, y: i64) -> [u8; 4] {
    let (w, h) = (i64::from(pic.width), i64::from(pic.height));
    if x >= w || y < 0 {
        return [0; 4];
    }
    pic.texel(x.max(0), y.min(h - 1))
}

/// GL's `GL_LINEAR` sample of `pic` at (`x`, `y`), each channel rounded
/// to 8 bits. `band` limits nothing: the taps may cross into the next
/// band of a strip, as the original's do.
pub fn sample(pic: &Picture, x: f64, y: f64) -> [u8; 4] {
    let (fx, fy) = (x.floor(), y.floor());
    let (a, b) = (x - fx, y - fy);
    let (x0, y0) = (fx as i64, fy as i64);
    let t = [
        tap(pic, x0, y0),
        tap(pic, x0 + 1, y0),
        tap(pic, x0, y0 + 1),
        tap(pic, x0 + 1, y0 + 1),
    ];
    let w = [(1.0 - a) * (1.0 - b), a * (1.0 - b), (1.0 - a) * b, a * b];
    std::array::from_fn(|c| {
        let v: f64 = t.iter().zip(&w).map(|(p, w)| f64::from(p[c]) * w).sum();
        (v + 0.5).floor().clamp(0.0, 255.0) as u8
    })
}

/// `GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA` of an RGBA source over an XRGB
/// pixel, rounded to 8 bits.
#[inline]
pub fn blend(dst: u32, src: [u8; 4]) -> u32 {
    let a = f64::from(src[3]) / 255.0;
    let mut out = 0u32;
    for (i, shift) in [16u32, 8, 0].into_iter().enumerate() {
        let d = f64::from((dst >> shift) & 0xff);
        let v = f64::from(src[i]) * a + d * (1.0 - a);
        out |= ((v + 0.5).floor().clamp(0.0, 255.0) as u32) << shift;
    }
    out
}

/// Draws the `src_w × src_h` part of `pic` whose top-left texel is
/// (`src_x`, `src_y`) at window position (`left`, `top`) (pixels, y down)
/// into `out` (`width × height` XRGB, rows top-down).
#[allow(clippy::too_many_arguments)]
pub fn draw(
    out: &mut [u32],
    width: u32,
    height: u32,
    pic: &Picture,
    (src_x, src_y, src_w, src_h): (u32, u32, u32, u32),
    left: f64,
    top: f64,
) {
    let (w, h) = (f64::from(src_w), f64::from(src_h));
    // Pixels whose centre is inside [left, left + w) × [top, top + h).
    let i0 = (left - 0.5).ceil().max(0.0) as i64;
    let i1 = ((left + w - 0.5).ceil() as i64).min(i64::from(width));
    let j0 = (top - 0.5).ceil().max(0.0) as i64;
    let j1 = ((top + h - 0.5).ceil() as i64).min(i64::from(height));
    for j in j0..j1 {
        let ty = f64::from(src_y) + (j as f64 + 0.5 - top) - 0.5;
        for i in i0..i1 {
            let tx = f64::from(src_x) + (i as f64 + 0.5 - left) - 0.5;
            let px = &mut out[j as usize * width as usize + i as usize];
            *px = blend(*px, sample(pic, tx, ty));
        }
    }
}

/// The cursor's top-left corner in window pixels for a pointer at
/// (`x`, `y`) (window pixels, y down): the anchor point on the pointer.
/// How the pointer reaches the cursor layout's position, and whether that
/// position is snapped, are not read (`[UNKNOWN]`).
pub fn cursor_corner(pic: &Picture, x: f64, y: f64) -> (f64, f64) {
    (
        x - f64::from(CURSOR_ANCHOR.0) * f64::from(pic.width),
        y - f64::from(CURSOR_ANCHOR.1) * f64::from(pic.height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Picture {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend(f(x, y));
            }
        }
        Picture {
            width: w,
            height: h,
            rgba,
        }
    }

    #[test]
    fn whole_pixel_sprites_copy_their_texels() {
        let p = pic(2, 2, |x, y| [(x * 100) as u8, (y * 100) as u8, 7, 255]);
        let mut out = vec![0u32; 16];
        draw(&mut out, 4, 4, &p, (0, 0, 2, 2), 1.0, 2.0);
        assert_eq!(out[2 * 4 + 1], 0x0000_0007);
        assert_eq!(out[2 * 4 + 2], 0x0064_0007);
        assert_eq!(out[3 * 4 + 1], 0x0000_6407);
        assert_eq!(out[0], 0);
    }

    #[test]
    fn half_alpha_blends_and_padding_is_transparent() {
        assert_eq!(blend(0x0000_0000, [255, 255, 255, 128]), 0x0080_8080);
        let p = pic(1, 1, |_, _| [255, 0, 0, 255]);
        // Right of the picture: padding; left of it: clamped.
        assert_eq!(sample(&p, 0.5, 0.0), [128, 0, 0, 128]);
        assert_eq!(sample(&p, -0.5, 0.0), [255, 0, 0, 255]);
        // Above: padding; below: clamped.
        assert_eq!(sample(&p, 0.0, -0.5), [128, 0, 0, 128]);
        assert_eq!(sample(&p, 0.0, 0.5), [255, 0, 0, 255]);
    }

    #[test]
    fn the_cursor_point_is_30_and_10_percent_in() {
        let p = pic(38, 40, |_, _| [0; 4]);
        let (l, t) = cursor_corner(&p, 100.0, 50.0);
        assert!((l - (100.0 - 0.3f32 as f64 * 38.0)).abs() < 1e-9);
        assert!((t - (50.0 - 0.1f32 as f64 * 40.0)).abs() < 1e-9);
    }
}

//! The image filters of the HD mode (port additions, see [`crate::hd`]):
//!
//! - [`catmull_rom_weights`]: the bicubic weights of the panorama's
//!   sampling ([`Warp::render_bicubic`]);
//! - [`sharpen_panorama`]: a mild unsharp mask of the panorama, limited to
//!   each pixel's neighbourhood so that it makes no halos;
//! - [`xbr2x`] and [`InterfaceScaler`]: a pixel-art upscaler of the xBR
//!   family for the 640x480 art drawn over the HD picture (the cursor, the
//!   bars, the menus, the puzzles), run twice for 4x;
//! - [`Axis`] and [`sample_bilinear`]: the final resampling to the
//!   window's size.
//!
//! Everything is integer arithmetic on XRGB8888, so every target draws the
//! same pixels.
//!
//! [`Warp::render_bicubic`]: crate::warp::Warp::render_bicubic

use std::sync::OnceLock;

use crate::warp::{SCREEN_H, SCREEN_W};

/// Runs `f(first_row, rows)` over bands of `out` (`row_len` items per
/// row), on several threads where the platform has them and the picture
/// is large.
pub(crate) fn for_each_band<T: Send>(
    out: &mut [T],
    row_len: usize,
    f: impl Fn(usize, &mut [T]) + Sync,
) {
    let rows = out.len().checked_div(row_len).unwrap_or(0);
    let threads = band_threads(out.len());
    if threads <= 1 || rows < 2 * threads {
        f(0, out);
        return;
    }
    let per = rows.div_ceil(threads);
    std::thread::scope(|s| {
        for (k, chunk) in out.chunks_mut(per * row_len).enumerate() {
            let f = &f;
            s.spawn(move || f(k * per, chunk));
        }
    });
}

/// The threads for a picture of `pixels` items: one below 250000, one per
/// core up to 8 above, one on the web.
pub(crate) fn band_threads(pixels: usize) -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = pixels;
        1
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if pixels < 250_000 {
            return 1;
        }
        std::thread::available_parallelism().map_or(1, |n| n.get().min(8))
    }
}

#[inline]
fn ch(p: u32, shift: u32) -> i32 {
    ((p >> shift) & 0xff) as i32
}

#[inline]
fn pack(r: i32, g: i32, b: i32) -> u32 {
    (r.clamp(0, 255) as u32) << 16 | (g.clamp(0, 255) as u32) << 8 | b.clamp(0, 255) as u32
}

/// The Catmull-Rom weights of the four taps around a sample point, for
/// the fraction `t` = i / 256 between the second and the third: in 1/128,
/// summing to 128 (a constant picture stays constant).
pub fn catmull_rom_weights() -> &'static [[i32; 4]; 256] {
    static W: OnceLock<[[i32; 4]; 256]> = OnceLock::new();
    W.get_or_init(|| {
        let mut w = [[0i32; 4]; 256];
        for (i, wi) in w.iter_mut().enumerate() {
            let t = i as f64 / 256.0;
            let (t2, t3) = (t * t, t * t * t);
            let k = [
                (-t3 + 2.0 * t2 - t) / 2.0,
                (3.0 * t3 - 5.0 * t2 + 2.0) / 2.0,
                (-3.0 * t3 + 4.0 * t2 + t) / 2.0,
                (t3 - t2) / 2.0,
            ];
            // Rounded as running sums, so that a step stays monotonic.
            let r = |v: f64| (v * 128.0).round() as i32;
            let (c1, c2, c3) = (r(k[0]), r(k[0] + k[1]), r(k[0] + k[1] + k[2]));
            *wi = [c1, c2 - c1, c3 - c2, 128 - c3];
        }
        w
    })
}

/// The bicubic (Catmull-Rom) value of 4x4 texels `t` (rows of four) with
/// the weights `wx`, `wy`, kept within the range of the four central
/// texels per channel (no ringing at hard edges).
#[inline]
pub(crate) fn bicubic16(t: &[[u32; 4]; 4], wx: &[i32; 4], wy: &[i32; 4]) -> u32 {
    let mut acc = [0i32; 3];
    for (row, &wyj) in t.iter().zip(wy) {
        let mut h = [0i32; 3];
        for (&p, &w) in row.iter().zip(wx) {
            h[0] += w * ch(p, 16);
            h[1] += w * ch(p, 8);
            h[2] += w * ch(p, 0);
        }
        for k in 0..3 {
            acc[k] += h[k] * wyj;
        }
    }
    let c = [t[1][1], t[1][2], t[2][1], t[2][2]];
    let mut v = [0i32; 3];
    for (k, shift) in [16u32, 8, 0].into_iter().enumerate() {
        let lo = c.iter().map(|&p| ch(p, shift)).min().unwrap_or(0);
        let hi = c.iter().map(|&p| ch(p, shift)).max().unwrap_or(0);
        v[k] = ((acc[k] + (1 << 13)) >> 14).clamp(lo, hi);
    }
    pack(v[0], v[1], v[2])
}

/// The strength of [`sharpen_panorama`]: the detail (the pixel less its
/// 3x3 blur) is added at 5/8.
pub const SHARPEN_AMOUNT: i32 = 160;

/// A mild unsharp mask of a panorama (`w` pixels per row, the rows
/// wrapping horizontally as the panorama does, clamped vertically) into
/// `dst`: each channel plus `amount` / 256 of its difference from the
/// 3x3 binomial blur, kept within the 3x3 neighbourhood's range so that
/// edges get no bright or dark halo.
pub fn sharpen_panorama(src: &[u32], w: usize, amount: i32, dst: &mut Vec<u32>) {
    dst.clear();
    dst.resize(src.len(), 0);
    if w == 0 {
        return;
    }
    let h = src.len() / w;
    for_each_band(&mut dst[..w * h], w, |first, band| {
        for (i, out) in band.chunks_mut(w).enumerate() {
            let y = first + i;
            let up = &src[y.saturating_sub(1) * w..][..w];
            let mid = &src[y * w..][..w];
            let down = &src[(y + 1).min(h - 1) * w..][..w];
            for (x, o) in out.iter_mut().enumerate() {
                let (xl, xr) = ((x + w - 1) % w, (x + 1) % w);
                let n = [
                    up[xl], up[x], up[xr], mid[xl], mid[x], mid[xr], down[xl], down[x], down[xr],
                ];
                const K: [i32; 9] = [1, 2, 1, 2, 4, 2, 1, 2, 1];
                let mut v = [0i32; 3];
                for (k, shift) in [16u32, 8, 0].into_iter().enumerate() {
                    let mut blur = 0;
                    let (mut lo, mut hi) = (255, 0);
                    for (&p, &kk) in n.iter().zip(&K) {
                        let c = ch(p, shift);
                        blur += kk * c;
                        lo = lo.min(c);
                        hi = hi.max(c);
                    }
                    let c = ch(n[4], shift);
                    let d = c * 16 - blur; // 16 x (pixel - blur)
                    v[k] = (c + (d * amount + (1 << 11)).div_euclid(1 << 12)).clamp(lo, hi);
                }
                *o = pack(v[0], v[1], v[2]);
            }
        }
    });
}

/// The panorama the HD view samples (see [`crate::hd`]), from the
/// sprite-composited panorama `surface` (RGB565, `w` pixels per row):
/// XRGB8888, sharpened with [`SHARPEN_AMOUNT`] when `sharpen`, and
/// upsampled to twice its size with Catmull-Rom (kept within each pair of
/// central texels, so without ringing) when `double`: the view then
/// samples that bilinearly, which costs what a bilinear view costs and
/// looks as a bicubic one (the view magnifies the panorama 1.5 to 6
/// times). This is the reference, on whole channel planes;
/// [`PanoramaPrep`] gives the same pixels tile by tile, again only where
/// the panorama changed.
pub fn prepare_panorama(
    surface: &[u16],
    w: usize,
    sharpen: bool,
    double: bool,
    out: &mut Vec<u32>,
) {
    let lut = crate::hd::rgb565_to_888();
    let h = surface.len().checked_div(w).unwrap_or(0);
    let mut planes: Vec<Vec<u8>> = [16u32, 8, 0]
        .iter()
        .map(|&sh| {
            surface[..w * h]
                .iter()
                .map(|&p| (lut[usize::from(p)] >> sh) as u8)
                .collect()
        })
        .collect();
    if sharpen {
        for p in &mut planes {
            *p = sharpen_plane(p, w, h, SHARPEN_AMOUNT);
        }
    }
    let (ow, oh) = if double {
        for p in &mut planes {
            let horizontal = double_rows(p, w, h);
            *p = double_columns(&horizontal, 2 * w, h);
        }
        (2 * w, 2 * h)
    } else {
        (w, h)
    };
    out.clear();
    out.resize(ow * oh, 0);
    let (r, g, b) = (&planes[0], &planes[1], &planes[2]);
    for_each_band(out, ow.max(1), |first, band| {
        let o = first * ow;
        for (i, d) in band.iter_mut().enumerate() {
            *d = u32::from(r[o + i]) << 16 | u32::from(g[o + i]) << 8 | u32::from(b[o + i]);
        }
    });
}

/// A row of a plane with `pad` pixels of the row's other end on each
/// side (the panorama wraps around).
fn wrapped_row(row: &[u8], pad: usize, out: &mut Vec<u8>) {
    let w = row.len();
    out.clear();
    out.extend_from_slice(&row[w - pad..]);
    out.extend_from_slice(row);
    out.extend_from_slice(&row[..pad]);
}

/// [`sharpen_panorama`] on one channel plane: the same arithmetic (the
/// 3x3 binomial blur and range made of their 3-tap rows and columns).
fn sharpen_plane(p: &[u8], w: usize, h: usize, amount: i32) -> Vec<u8> {
    // Per row: the horizontal [1 2 1] sum (bits 0..15), the 3-pixel min
    // (16..23) and max (24..31), wrapping around the panorama.
    let mut rows = vec![0u32; w * h];
    for_each_band(&mut rows, w.max(1), |first, band| {
        let mut pr = Vec::with_capacity(w + 2);
        for (i, o) in band.chunks_mut(w).enumerate() {
            wrapped_row(&p[(first + i) * w..][..w], 1, &mut pr);
            for (d, t) in o.iter_mut().zip(pr.windows(3)) {
                let (a, b, c) = (t[0], t[1], t[2]);
                let sum = u32::from(a) + 2 * u32::from(b) + u32::from(c);
                *d = sum | u32::from(a.min(b).min(c)) << 16 | u32::from(a.max(b).max(c)) << 24;
            }
        }
    });
    let mut out = vec![0u8; w * h];
    for_each_band(&mut out, w.max(1), |first, band| {
        for (i, o) in band.chunks_mut(w).enumerate() {
            let y = first + i;
            let (u, d) = (y.saturating_sub(1) * w, (y + 1).min(h - 1) * w);
            let m = y * w;
            let (ru, rm, rd) = (&rows[u..][..w], &rows[m..][..w], &rows[d..][..w]);
            let c = &p[m..][..w];
            for x in 0..w {
                let (a, b, e) = (ru[x], rm[x], rd[x]);
                let blur = ((a & 0xffff) + 2 * (b & 0xffff) + (e & 0xffff)) as i32;
                let lo = ((a >> 16) & 0xff)
                    .min((b >> 16) & 0xff)
                    .min((e >> 16) & 0xff);
                let hi = (a >> 24).max(b >> 24).max(e >> 24);
                let cv = i32::from(c[x]);
                let v = cv + (((cv * 16 - blur) * amount + (1 << 11)) >> 12);
                o[x] = v.clamp(lo as i32, hi as i32) as u8;
            }
        }
    });
    out
}

/// The 2x Catmull-Rom weights: an output pixel's centre is a quarter of
/// a texel before (even outputs: 3/4 of the way from the texel before)
/// or after (odd: 1/4 of the way to the next) a texel's centre.
fn double_weights() -> ([i32; 4], [i32; 4]) {
    let wt = catmull_rom_weights();
    (wt[192], wt[64])
}

/// The 2x tap: `t` the four texels, clamped to the central two.
#[inline]
fn tap4(w: &[i32; 4], a: u8, b: u8, c: u8, d: u8) -> u8 {
    let v = (w[0] * i32::from(a)
        + w[1] * i32::from(b)
        + w[2] * i32::from(c)
        + w[3] * i32::from(d)
        + 64)
        >> 7;
    v.clamp(i32::from(b.min(c)), i32::from(b.max(c))) as u8
}

/// A plane twice as wide, rows wrapping around the panorama.
fn double_rows(p: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (we, wo) = double_weights();
    let mut out = vec![0u8; 2 * w * h];
    for_each_band(&mut out, (2 * w).max(1), |first, band| {
        let mut pr = Vec::with_capacity(w + 4);
        for (i, o) in band.chunks_mut(2 * w).enumerate() {
            wrapped_row(&p[(first + i) * w..][..w], 2, &mut pr);
            for (x, pair) in o.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                let t = &pr[x..x + 5];
                pair[0] = tap4(&we, t[0], t[1], t[2], t[3]);
                pair[1] = tap4(&wo, t[1], t[2], t[3], t[4]);
            }
        }
    });
    out
}

/// A plane twice as tall, the first and last rows repeated past the edges.
fn double_columns(p: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (we, wo) = double_weights();
    let mut out = vec![0u8; 2 * w * h];
    for_each_band(&mut out, w.max(1), |first, band| {
        for (i, o) in band.chunks_mut(w).enumerate() {
            let oy = first + i;
            let y = oy / 2;
            let r = |k: isize| &p[(y as isize + k).clamp(0, h as isize - 1) as usize * w..][..w];
            let (wt, rs) = if oy % 2 == 0 {
                (&we, [r(-2), r(-1), r(0), r(1)])
            } else {
                (&wo, [r(-1), r(0), r(1), r(2)])
            };
            for (x, d) in o.iter_mut().enumerate() {
                *d = tap4(wt, rs[0][x], rs[1][x], rs[2][x], rs[3][x]);
            }
        }
    });
    out
}

/// The tiles [`PanoramaPrep`] works in, and the source pixels around a
/// tile its pixels depend on (1 for the sharpening, 2 for the doubling).
const PT: usize = 64;
const PM: usize = 3;

/// [`prepare_panorama`] kept up to date: the panorama is compared with
/// the last one tile by tile, and only the tiles whose pixels (or the
/// pixels around them that they depend on) changed are computed again;
/// a sprite's animation costs its few tiles, a new place all of them, on
/// all the cores.
#[derive(Default)]
pub struct PanoramaPrep {
    w: usize,
    h: usize,
    sharpen: bool,
    double: bool,
    prev: Vec<u16>,
    out: Vec<u32>,
}

impl PanoramaPrep {
    pub fn new() -> PanoramaPrep {
        PanoramaPrep::default()
    }

    /// The prepared panorama: `w` x `h`, or twice that when doubled.
    pub fn pixels(&self) -> &[u32] {
        &self.out
    }

    /// Brings the prepared panorama up to date with `surface` (`w`
    /// pixels per row) and the options; the number of tiles computed.
    pub fn update(&mut self, surface: &[u16], w: usize, sharpen: bool, double: bool) -> usize {
        let h = surface.len().checked_div(w).unwrap_or(0);
        if w == 0 || h == 0 {
            self.out.clear();
            return 0;
        }
        let (tw, th) = (w.div_ceil(PT), h.div_ceil(PT));
        let mut dirty = vec![false; tw * th];
        let fresh = (w, h, sharpen, double) != (self.w, self.h, self.sharpen, self.double)
            || self.prev.len() != w * h;
        if fresh {
            dirty.fill(true);
            let s = if double { 2 } else { 1 };
            self.out = vec![0; s * s * w * h];
            (self.w, self.h, self.sharpen, self.double) = (w, h, sharpen, double);
        } else if surface[..w * h] != self.prev[..] {
            // Each changed pixel dirties its tile and the tiles within
            // PM pixels of it (wrapping horizontally).
            for y in 0..h {
                let (a, b) = (&surface[y * w..][..w], &self.prev[y * w..][..w]);
                if a == b {
                    continue;
                }
                let tys = (y.saturating_sub(PM) / PT)..=((y + PM).min(h - 1) / PT);
                for tx in 0..tw {
                    let (x0, x1) = (tx * PT, ((tx + 1) * PT).min(w));
                    if a[x0..x1] == b[x0..x1] {
                        continue;
                    }
                    let first = (x0..x1).find(|&x| a[x] != b[x]).unwrap_or(x0);
                    let last = (x0..x1).rev().find(|&x| a[x] != b[x]).unwrap_or(x0);
                    let left = (first + w - PM) % w / PT;
                    let right = (last + PM) % w / PT;
                    for ty in tys.clone() {
                        for t in [left, tx, right] {
                            dirty[ty * tw + t] = true;
                        }
                    }
                }
            }
        }
        self.prev.clear();
        self.prev.extend_from_slice(&surface[..w * h]);
        let todo: Vec<Vec<usize>> = (0..th)
            .map(|ty| (0..tw).filter(|&tx| dirty[ty * tw + tx]).collect())
            .collect();
        let count = todo.iter().map(Vec::len).sum();
        if count == 0 {
            return 0;
        }
        let s = if double { 2 } else { 1 };
        let ow = s * w;
        let band = PT * s * ow;
        let src = &self.prev;
        let run = |first: usize, bands: &mut [u32]| {
            for (i, b) in bands.chunks_mut(band).enumerate() {
                for &tx in &todo[first + i] {
                    prep_tile(src, w, h, tx, first + i, sharpen, double, b);
                }
            }
        };
        let threads = if count >= 4 {
            band_threads(count * PT * PT * s * s)
        } else {
            1
        };
        if threads <= 1 {
            run(0, &mut self.out);
        } else {
            let per = th.div_ceil(threads);
            std::thread::scope(|sc| {
                for (k, chunk) in self.out.chunks_mut(per * band).enumerate() {
                    let run = &run;
                    sc.spawn(move || run(k * per, chunk));
                }
            });
        }
        count
    }
}

/// One tile of [`PanoramaPrep`]: the source tile (`tx`, `ty`) and the
/// pixels around it into `band`, the tile row's part of the output.
#[allow(clippy::too_many_arguments)]
fn prep_tile(
    src: &[u16],
    w: usize,
    h: usize,
    tx: usize,
    ty: usize,
    sharpen: bool,
    double: bool,
    band: &mut [u32],
) {
    let (x0, y0) = (tx * PT, ty * PT);
    let (tw, th) = (PT.min(w - x0), PT.min(h - y0));
    let (lw, lh) = (tw + 2 * PM, th + 2 * PM);
    // The region in planes: columns wrapping, rows clamped.
    let mut planes = [vec![0u8; lw * lh], vec![0u8; lw * lh], vec![0u8; lw * lh]];
    for ly in 0..lh {
        let sy = (y0 as isize + ly as isize - PM as isize).clamp(0, h as isize - 1) as usize;
        let row = &src[sy * w..][..w];
        for lx in 0..lw {
            let sx = (x0 + w + lx - PM) % w;
            let p = u32::from(row[sx]);
            let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
            let o = ly * lw + lx;
            planes[0][o] = (r << 3 | r >> 2) as u8;
            planes[1][o] = (g << 2 | g >> 4) as u8;
            planes[2][o] = (b << 3 | b >> 2) as u8;
        }
    }
    if sharpen {
        // Valid one pixel in from the region's edges (which the doubling
        // does not read). At the panorama's top and bottom the clamped
        // rows repeat the edge row, as the whole-plane version.
        for p in &mut planes {
            let mut o = p.clone();
            for y in 1..lh - 1 {
                for x in 1..lw - 1 {
                    let mut blur = 0;
                    let (mut lo, mut hi) = (255u8, 0u8);
                    for (dy, kr) in [(y - 1, 1), (y, 2), (y + 1, 1)] {
                        let r = &p[dy * lw..][..lw];
                        blur +=
                            kr * (i32::from(r[x - 1]) + 2 * i32::from(r[x]) + i32::from(r[x + 1]));
                        lo = lo.min(r[x - 1]).min(r[x]).min(r[x + 1]);
                        hi = hi.max(r[x - 1]).max(r[x]).max(r[x + 1]);
                    }
                    let c = i32::from(p[y * lw + x]);
                    let v = c + (((c * 16 - blur) * SHARPEN_AMOUNT + (1 << 11)) >> 12);
                    o[y * lw + x] = v.clamp(i32::from(lo), i32::from(hi)) as u8;
                }
            }
            // Past the top and bottom, the edge row as sharpened (the
            // whole-plane doubling repeats it).
            for ly in 0..lh {
                let sy = y0 as isize + ly as isize - PM as isize;
                let c = sy.clamp(0, h as isize - 1);
                if c != sy {
                    let from = (c - y0 as isize + PM as isize) as usize;
                    o.copy_within(from * lw..(from + 1) * lw, ly * lw);
                }
            }
            *p = o;
        }
    }
    let s = if double { 2 } else { 1 };
    let ow = s * w;
    let mut tile = vec![0u32; s * tw * s * th];
    let mut put = |plane: usize, x: usize, y: usize, v: u8| {
        tile[y * s * tw + x] |= u32::from(v) << (16 - 8 * plane);
    };
    if double {
        let (we, wo) = double_weights();
        for (k, p) in planes.iter().enumerate() {
            // Twice as wide: the rows the vertical pass reads (2 above
            // and below the tile).
            let hw = 2 * tw;
            let rows = th + 4;
            let mut hz = vec![0u8; hw * rows];
            for r in 0..rows {
                let row = &p[(r + PM - 2) * lw..][..lw];
                for x in 0..tw {
                    let t = &row[x + PM - 2..x + PM + 3];
                    hz[r * hw + 2 * x] = tap4(&we, t[0], t[1], t[2], t[3]);
                    hz[r * hw + 2 * x + 1] = tap4(&wo, t[1], t[2], t[3], t[4]);
                }
            }
            for oy in 0..2 * th {
                let y = oy / 2 + 2; // the row in hz
                let (wt, r0) = if oy % 2 == 0 {
                    (&we, y - 2)
                } else {
                    (&wo, y - 1)
                };
                for x in 0..hw {
                    let v = tap4(
                        wt,
                        hz[r0 * hw + x],
                        hz[(r0 + 1) * hw + x],
                        hz[(r0 + 2) * hw + x],
                        hz[(r0 + 3) * hw + x],
                    );
                    put(k, x, oy, v);
                }
            }
        }
    } else {
        for (k, p) in planes.iter().enumerate() {
            for y in 0..th {
                for x in 0..tw {
                    put(k, x, y, p[(y + PM) * lw + x + PM]);
                }
            }
        }
    }
    for y in 0..s * th {
        band[y * ow + s * x0..][..s * tw].copy_from_slice(&tile[y * s * tw..][..s * tw]);
    }
}

/// A pixel's luma and chroma for the xBR distance (YUV, in 1/256).
#[inline]
fn yuv(p: u32) -> [i32; 3] {
    let (r, g, b) = (ch(p, 16), ch(p, 8), ch(p, 0));
    [
        77 * r + 150 * g + 29 * b,
        -43 * r - 85 * g + 128 * b,
        128 * r - 107 * g - 21 * b,
    ]
}

/// The xBR colour distance: luma weighted most (the weights 48, 7, 6 of
/// Hyllian's xBR).
#[inline]
fn dist(a: &[i32; 3], b: &[i32; 3]) -> i32 {
    (48 * (a[0] - b[0]).abs() + 7 * (a[1] - b[1]).abs() + 6 * (a[2] - b[2]).abs()) >> 8
}

/// `dst` moved toward `src` by `w` / 256, per channel.
#[inline]
fn blend(dst: &mut u32, src: u32, w: i32) {
    let d = *dst;
    let mix = |s: u32| ch(d, s) + ((ch(src, s) - ch(d, s)) * w + 128).div_euclid(256);
    *dst = pack(mix(16), mix(8), mix(0));
}

/// The neighbourhood offsets one corner reads, in the order of
/// [`xbr_corner`]'s `n`: E, H, F, I, G, C, D, B, F4, I4, H5, I5 (E the
/// pixel, H below, F right, I below right, F4 two right, H5 two below...).
const CORNER_TAPS: [(i32, i32); 12] = [
    (0, 0),
    (0, 1),
    (1, 0),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, 0),
    (0, -1),
    (2, 0),
    (2, 1),
    (0, 2),
    (1, 2),
];

/// One corner of an xBR pixel: the 2x2 output `e` of the source pixel,
/// its neighbourhood `px` / `yv` (pixels and their YUV in the order of
/// [`CORNER_TAPS`]) turned so that the corner is the bottom-right one;
/// `n3`, `n2`, `n1` are the output pixels at the corner, beside it on the
/// bottom row and beside it in the right column.
#[inline]
fn xbr_corner(e: &mut [u32; 4], px: &[u32; 12], yv: &[[i32; 3]; 12], n: (usize, usize, usize)) {
    let [pe, ph, pf, _, pg, pc, pd, pb, ..] = *px;
    if pe == ph || pe == pf {
        return;
    }
    let [e_, h, f, i, g, c, d, b, f4, i4, h5, i5] = yv;
    let df = dist;
    let eq = |a: &[i32; 3], b: &[i32; 3]| dist(a, b) < 48;
    let edge = df(e_, c) + df(e_, g) + df(i, h5) + df(i, f4) + 4 * df(h, f);
    let across = df(h, d) + df(h, i5) + df(f, i4) + df(f, b) + 4 * df(e_, i);
    if edge > across {
        return;
    }
    let new = if df(e_, f) <= df(e_, h) { pf } else { ph };
    let (n3, n2, n1) = n;
    let corner = (!eq(f, b) && !eq(h, d))
        || (eq(e_, i) && !eq(f, i4) && !eq(h, i5))
        || eq(e_, g)
        || eq(e_, c);
    if edge < across && corner {
        let ke = df(f, g);
        let ki = df(h, c);
        let ex2 = pe != pc && pb != pc;
        let ex3 = pe != pg && pd != pg;
        let shallow = 2 * ke <= ki && ex3;
        let steep = ke >= 2 * ki && ex2;
        if shallow && steep {
            blend(&mut e[n3], new, 224);
            blend(&mut e[n2], new, 64);
            blend(&mut e[n1], new, 64);
        } else if shallow {
            blend(&mut e[n3], new, 192);
            blend(&mut e[n2], new, 64);
        } else if steep {
            blend(&mut e[n3], new, 192);
            blend(&mut e[n1], new, 64);
        } else {
            blend(&mut e[n3], new, 128);
        }
    } else {
        blend(&mut e[n3], new, 128);
    }
}

/// The four corners: the neighbourhood turned by 0, 90, 180 and 270
/// degrees ((x, y) -> (-y, x)), with the output pixels (top-left 0,
/// top-right 1, bottom-left 2, bottom-right 3) at its corner, bottom row
/// and right column.
/// A turn of the neighbourhood, and the output pixels of its corner.
type Rotation = (fn(i32, i32) -> (i32, i32), (usize, usize, usize));

const ROTATIONS: [Rotation; 4] = [
    (|x, y| (x, y), (3, 2, 1)),
    (|x, y| (-y, x), (2, 0, 3)),
    (|x, y| (-x, -y), (0, 1, 2)),
    (|x, y| (y, -x), (1, 3, 0)),
];

/// xBR at 2x (after Hyllian's 2xBR, level 1): the source pixels in the
/// rectangle `x0..x1`, `y0..y1` of `src` (`sw` x `sh`) become 2x2 blocks
/// of `dst` (`2 sw` pixels per row, its first row being the output row
/// `dst_row0`). The edges between two colours get the sub-pixel blends
/// that turn their steps into lines; a flat area stays flat. The source
/// is read with its edge pixels repeated.
#[allow(clippy::too_many_arguments)]
pub fn xbr2x(
    src: &[u32],
    sw: usize,
    sh: usize,
    (x0, y0, x1, y1): (usize, usize, usize, usize),
    dst: &mut [u32],
    dst_row0: usize,
) {
    let (x1, y1) = (x1.min(sw), y1.min(sh));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    // The rectangle and 2 pixels around, with their YUV.
    let (rw, rh) = (x1 - x0 + 4, y1 - y0 + 4);
    let mut px = vec![0u32; rw * rh];
    let mut yv = vec![[0i32; 3]; rw * rh];
    for ry in 0..rh {
        let sy = (y0 as isize + ry as isize - 2).clamp(0, sh as isize - 1) as usize;
        for rx in 0..rw {
            let sx = (x0 as isize + rx as isize - 2).clamp(0, sw as isize - 1) as usize;
            let p = src[sy * sw + sx];
            px[ry * rw + rx] = p;
            yv[ry * rw + rx] = yuv(p);
        }
    }
    // The taps of each rotation as offsets in the region.
    let taps: [[isize; 12]; 4] = ROTATIONS.map(|(rot, _)| {
        CORNER_TAPS.map(|(dx, dy)| {
            let (x, y) = rot(dx, dy);
            y as isize * rw as isize + x as isize
        })
    });
    let dw = 2 * sw;
    for y in y0..y1 {
        for x in x0..x1 {
            let c = ((y - y0 + 2) * rw + (x - x0 + 2)) as isize;
            let pe = px[c as usize];
            let mut e = [pe; 4];
            for (t, &(_, n)) in taps.iter().zip(&ROTATIONS) {
                let p = t.map(|o| px[(c + o) as usize]);
                // Most pixels are inside a flat area: no corner to blend.
                if p[0] == p[1] || p[0] == p[2] {
                    continue;
                }
                let v = t.map(|o| yv[(c + o) as usize]);
                xbr_corner(&mut e, &p, &v, n);
            }
            let o = (2 * y - dst_row0) * dw + 2 * x;
            dst[o] = e[0];
            dst[o + 1] = e[1];
            dst[o + dw] = e[2];
            dst[o + dw + 1] = e[3];
        }
    }
}

/// The 640x480 blocks [`InterfaceScaler`] works in.
const BLOCK: usize = 16;
const BW: usize = SCREEN_W / BLOCK;
const BH: usize = SCREEN_H / BLOCK;

/// The 640x480 art upscaled with xBR at 2x or 4x (twice 2x), block by
/// block and only where needed: the blocks a caller asks for, computed
/// again only when the picture changed around them (a still menu costs
/// nothing after its first frame, a moving cursor its few blocks).
#[derive(Default)]
pub struct InterfaceScaler {
    factor: usize,
    /// The picture of the last update, RGB565.
    prev: Vec<u16>,
    src: Vec<u32>,
    /// The 2x picture (4x only).
    mid: Vec<u32>,
    /// The `factor` x picture.
    out: Vec<u32>,
    mid_valid: Vec<bool>,
    out_valid: Vec<bool>,
}

/// Runs `f(by, bx, band, band_row0)` for the blocks `todo[by]` of each
/// row of blocks `by`, the rows of blocks split over the cores; `band` is
/// that row of blocks' part of `dst` (`dst_w` pixels per row, `scale`
/// output rows per source row), its first row being `band_row0`.
fn run_blocks(
    dst: &mut [u32],
    dst_w: usize,
    scale: usize,
    todo: &[Vec<usize>],
    f: impl Fn(usize, usize, &mut [u32], usize) + Sync,
) {
    let band_len = BLOCK * scale * dst_w;
    let work: usize = todo.iter().map(Vec::len).sum();
    if work == 0 {
        return;
    }
    let threads = if work >= 16 {
        band_threads(work * BLOCK * BLOCK * scale * scale * 4)
    } else {
        1
    };
    let run = |first: usize, bands: &mut [u32]| {
        for (i, band) in bands.chunks_mut(band_len).enumerate() {
            let by = first + i;
            for &bx in &todo[by] {
                f(by, bx, band, by * BLOCK * scale);
            }
        }
    };
    if threads <= 1 {
        run(0, dst);
        return;
    }
    let per = BH.div_ceil(threads);
    std::thread::scope(|s| {
        for (k, chunk) in dst.chunks_mut(per * band_len).enumerate() {
            let run = &run;
            s.spawn(move || run(k * per, chunk));
        }
    });
}

impl InterfaceScaler {
    pub fn new() -> InterfaceScaler {
        InterfaceScaler::default()
    }

    /// The scale factor of the last update (2 or 4).
    pub fn factor(&self) -> usize {
        self.factor
    }

    /// The upscaled picture: `640 factor` x `480 factor`, valid in the
    /// blocks the last update asked for.
    pub fn pixels(&self) -> &[u32] {
        &self.out
    }

    /// Brings the blocks `need(bx, by)` of the `factor` x picture of
    /// `screen` (640x480 RGB565) up to date.
    pub fn update(&mut self, screen: &[u16], factor: usize, need: impl Fn(usize, usize) -> bool) {
        let factor = if factor >= 4 { 4 } else { 2 };
        let lut = crate::hd::rgb565_to_888();
        if self.factor != factor || self.prev.len() != screen.len() {
            self.factor = factor;
            self.prev = screen.to_vec();
            self.src = screen.iter().map(|&p| lut[usize::from(p)]).collect();
            self.mid = if factor == 4 {
                vec![0; 4 * SCREEN_W * SCREEN_H]
            } else {
                Vec::new()
            };
            self.out = vec![0; factor * factor * SCREEN_W * SCREEN_H];
            self.mid_valid = vec![false; BW * BH];
            self.out_valid = vec![false; BW * BH];
        } else if screen != self.prev.as_slice() {
            // The blocks that changed invalidate themselves and their
            // neighbours (xBR reads 2 source pixels around, 3 at 4x).
            let mut changed = vec![false; BW * BH];
            for by in 0..BH {
                for y in by * BLOCK..(by + 1) * BLOCK {
                    let r = y * SCREEN_W;
                    if screen[r..r + SCREEN_W] == self.prev[r..r + SCREEN_W] {
                        continue;
                    }
                    for bx in 0..BW {
                        let b = r + bx * BLOCK;
                        if screen[b..b + BLOCK] != self.prev[b..b + BLOCK] {
                            changed[by * BW + bx] = true;
                        }
                    }
                    self.prev[r..r + SCREEN_W].copy_from_slice(&screen[r..r + SCREEN_W]);
                    for (d, &p) in self.src[r..r + SCREEN_W].iter_mut().zip(&screen[r..]) {
                        *d = lut[usize::from(p)];
                    }
                }
            }
            for by in 0..BH {
                for bx in 0..BW {
                    if changed[by * BW + bx] {
                        for (nx, ny) in neighbours(bx, by) {
                            self.mid_valid[ny * BW + nx] = false;
                            self.out_valid[ny * BW + nx] = false;
                        }
                    }
                }
            }
        }
        // The blocks to compute.
        let mut out_todo = vec![Vec::new(); BH];
        let mut mid_todo = vec![Vec::new(); BH];
        let mut mid_mark = vec![false; BW * BH];
        for (by, todo) in out_todo.iter_mut().enumerate() {
            for bx in 0..BW {
                if self.out_valid[by * BW + bx] || !need(bx, by) {
                    continue;
                }
                todo.push(bx);
                if factor == 4 {
                    for (nx, ny) in neighbours(bx, by) {
                        let k = ny * BW + nx;
                        if !self.mid_valid[k] && !mid_mark[k] {
                            mid_mark[k] = true;
                            mid_todo[ny].push(nx);
                        }
                    }
                }
            }
        }
        let src = &self.src;
        let rect = |bx: usize, by: usize, s: usize| {
            let (x0, y0) = (bx * BLOCK * s, by * BLOCK * s);
            (x0, y0, x0 + BLOCK * s, y0 + BLOCK * s)
        };
        if factor == 2 {
            run_blocks(
                &mut self.out,
                2 * SCREEN_W,
                2,
                &out_todo,
                |by, bx, band, r0| {
                    xbr2x(src, SCREEN_W, SCREEN_H, rect(bx, by, 1), band, r0);
                },
            );
        } else {
            run_blocks(
                &mut self.mid,
                2 * SCREEN_W,
                2,
                &mid_todo,
                |by, bx, band, r0| {
                    xbr2x(src, SCREEN_W, SCREEN_H, rect(bx, by, 1), band, r0);
                },
            );
            for (k, m) in mid_mark.iter().enumerate() {
                if *m {
                    self.mid_valid[k] = true;
                }
            }
            let mid = &self.mid;
            run_blocks(
                &mut self.out,
                4 * SCREEN_W,
                4,
                &out_todo,
                |by, bx, band, r0| {
                    xbr2x(mid, 2 * SCREEN_W, 2 * SCREEN_H, rect(bx, by, 2), band, r0);
                },
            );
        }
        for (by, row) in out_todo.iter().enumerate() {
            for &bx in row {
                self.out_valid[by * BW + bx] = true;
            }
        }
    }
}

/// A block and its eight neighbours inside the 40x30 grid.
fn neighbours(bx: usize, by: usize) -> impl Iterator<Item = (usize, usize)> {
    let xs = bx.saturating_sub(1)..(bx + 2).min(BW);
    (by.saturating_sub(1)..(by + 2).min(BH)).flat_map(move |y| xs.clone().map(move |x| (x, y)))
}

/// The resampling of one axis of `n` source pixels to `m` output pixels:
/// for each output pixel, its nearest source pixel and the two source
/// pixels and the weight (in 1/256) of the linear interpolation at its
/// centre.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Axis {
    pub nearest: Vec<usize>,
    pub lin: Vec<(usize, usize, u32)>,
}

impl Axis {
    pub fn new(n: usize, m: usize) -> Axis {
        let nearest = (0..m).map(|i| i * n / m.max(1)).collect();
        let lin = (0..m)
            .map(|i| {
                // Centre of the output pixel in source pixels, less half.
                let c = ((2 * i + 1) * n * 256 / (2 * m.max(1))) as i64 - 128;
                let c = c.max(0);
                let i0 = ((c >> 8) as usize).min(n - 1);
                let i1 = (i0 + 1).min(n - 1);
                (i0, i1, (c & 0xff) as u32)
            })
            .collect();
        Axis { nearest, lin }
    }
}

/// `a` to `b` at `w` / 256, per channel of XRGB8888.
#[inline]
pub(crate) fn lerp888(a: u32, b: u32, w: u32) -> u32 {
    let iw = 256 - w;
    let rb = (((a & 0xff_00ff) * iw + (b & 0xff_00ff) * w) >> 8) & 0xff_00ff;
    let g = (((a & 0xff00) * iw + (b & 0xff00) * w) >> 8) & 0xff00;
    rb | g
}

/// The bilinear value of `src` (`sw` pixels per row) at the output pixel
/// of the column map `cx` and the row map `cy` entries.
#[inline]
pub fn sample_bilinear(
    src: &[u32],
    sw: usize,
    cx: (usize, usize, u32),
    cy: (usize, usize, u32),
) -> u32 {
    let (r0, r1) = (cy.0 * sw, cy.1 * sw);
    let top = lerp888(src[r0 + cx.0], src[r0 + cx.1], cx.2);
    let bottom = lerp888(src[r1 + cx.0], src[r1 + cx.1], cx.2);
    lerp888(top, bottom, cy.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catmull_rom_sums_to_one() {
        let w = catmull_rom_weights();
        assert!(w.iter().all(|k| k.iter().sum::<i32>() == 128));
        assert_eq!(w[0], [0, 128, 0, 0]);
    }

    #[test]
    fn bicubic_of_a_constant_is_constant() {
        let w = catmull_rom_weights();
        let t = [[0x0040_80c0u32; 4]; 4];
        for i in (0..256).step_by(17) {
            for j in (0..256).step_by(13) {
                assert_eq!(bicubic16(&t, &w[i], &w[j]), 0x0040_80c0);
            }
        }
    }

    #[test]
    fn bicubic_does_not_ring() {
        let w = catmull_rom_weights();
        let white = 0x00ff_ffffu32;
        // Between two white texels next to a black one: white, where the
        // plain Catmull-Rom sum overshoots.
        let t = [[0, white, white, white]; 4];
        for wx in w.iter() {
            assert_eq!(bicubic16(&t, wx, &w[0]), white);
        }
        // Across a hard edge the values rise (to the weights' rounding).
        let t = [[0, 0, white, white]; 4];
        let mut last = 0;
        for wx in w.iter() {
            let v = bicubic16(&t, wx, &w[0]) & 0xff;
            assert!(v + 1 >= last, "monotonic across the edge: {v} after {last}");
            last = v;
        }
    }

    #[test]
    fn sharpen_keeps_a_constant_and_stays_in_range() {
        let w = 64;
        let src = vec![0x0012_3456u32; w * 8];
        let mut dst = Vec::new();
        sharpen_panorama(&src, w, SHARPEN_AMOUNT, &mut dst);
        assert_eq!(dst, src);
        // A soft edge (0, 64, 192, 255) gets steeper, without overshoot:
        // monotonic, its feet kept.
        let src: Vec<u32> = (0..w * 8)
            .map(|i| {
                let v = match i % w {
                    0..=19 => 0,
                    20 => 64,
                    21 => 192,
                    _ => 255,
                };
                v << 16 | v << 8 | v
            })
            .collect();
        sharpen_panorama(&src, w, SHARPEN_AMOUNT, &mut dst);
        let row = &dst[3 * w..4 * w];
        for x in 1..w - 1 {
            assert!(row[x] & 0xff >= row[x - 1] & 0xff, "monotonic at {x}");
        }
        assert!(row[20] & 0xff < 64 && row[21] & 0xff > 192);
        assert_eq!((row[19] & 0xff, row[22] & 0xff), (0, 255));
        // A one-pixel line gets no halo: its neighbours' range bounds it.
        let mut src = vec![0x0080_8080u32; w * 8];
        for y in 0..8 {
            src[y * w + 10] = 0x00ff_ffff;
        }
        sharpen_panorama(&src, w, SHARPEN_AMOUNT, &mut dst);
        assert!(dst.iter().all(|&p| p & 0xff >= 0x80));
        assert_eq!(dst[3 * w + 10], 0x00ff_ffff);
    }

    #[test]
    fn prepared_panorama_matches_the_reference_filters() {
        let (w, h) = (64usize, 24usize);
        let surface: Vec<u16> = (0..w * h)
            .map(|i| ((i * 2_654_435_761usize) >> 7) as u16)
            .collect();
        let lut = crate::hd::rgb565_to_888();
        let rgb: Vec<u32> = surface.iter().map(|&p| lut[usize::from(p)]).collect();
        let mut out = Vec::new();
        prepare_panorama(&surface, w, false, false, &mut out);
        assert_eq!(out, rgb);
        // The planar sharpening is sharpen_panorama's.
        let mut want = Vec::new();
        sharpen_panorama(&rgb, w, SHARPEN_AMOUNT, &mut want);
        prepare_panorama(&surface, w, true, false, &mut out);
        assert_eq!(out, want);
        // Doubled: twice the size, deterministic; a constant stays constant.
        prepare_panorama(&surface, w, true, true, &mut out);
        assert_eq!(out.len(), 4 * w * h);
        let mut again = Vec::new();
        prepare_panorama(&surface, w, true, true, &mut again);
        assert_eq!(out, again);
        prepare_panorama(&vec![0x7bef; w * h], w, true, true, &mut out);
        assert!(out.iter().all(|&p| p == lut[0x7bef]));
        // Each doubled pixel lies within the range of the texels around it.
        prepare_panorama(&surface, w, false, true, &mut out);
        for y in 0..2 * h {
            for x in 0..2 * w {
                let (sx, sy) = (x / 2, y / 2);
                let near = |dx: isize, dy: isize| {
                    let xx = (sx as isize + dx).rem_euclid(w as isize) as usize;
                    let yy = (sy as isize + dy).clamp(0, h as isize - 1) as usize;
                    rgb[yy * w + xx]
                };
                for shift in [16, 8, 0] {
                    let v = (out[y * 2 * w + x] >> shift) & 0xff;
                    let vals: Vec<u32> = (-1..=1)
                        .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                        .map(|(dx, dy)| (near(dx, dy) >> shift) & 0xff)
                        .collect();
                    let (lo, hi) = (*vals.iter().min().unwrap(), *vals.iter().max().unwrap());
                    assert!((lo..=hi).contains(&v));
                }
            }
        }
    }

    #[test]
    fn panorama_prep_matches_the_reference_and_updates_tiles() {
        let (w, h) = (256usize, 160usize); // a partial row of tiles
        let mut surface: Vec<u16> = (0..w * h)
            .map(|i| ((i * 2_654_435_761usize) >> 9) as u16)
            .collect();
        for (sharpen, double) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut prep = PanoramaPrep::new();
            assert_eq!(prep.update(&surface, w, sharpen, double), 4 * 3);
            let mut want = Vec::new();
            prepare_panorama(&surface, w, sharpen, double, &mut want);
            assert!(prep.pixels() == want.as_slice(), "{sharpen} {double}");
            assert_eq!(prep.update(&surface, w, sharpen, double), 0);
            // A change at a tile's left edge, by the panorama's seam, and
            // one in the middle of a tile.
            surface[70 * w] ^= 0xffff;
            surface[100 * w + 130] ^= 0x1234;
            let n = prep.update(&surface, w, sharpen, double);
            assert!(n > 0 && n < 12, "{n} tiles");
            prepare_panorama(&surface, w, sharpen, double, &mut want);
            assert!(
                prep.pixels() == want.as_slice(),
                "{sharpen} {double} updated"
            );
        }
    }

    fn up2(src: &[u32], w: usize, h: usize) -> Vec<u32> {
        let mut d = vec![0; 4 * w * h];
        xbr2x(src, w, h, (0, 0, w, h), &mut d, 0);
        d
    }

    #[test]
    fn xbr_keeps_flat_colours_flat() {
        let (w, h) = (12, 9);
        let src = vec![0x0033_6699u32; w * h];
        assert!(up2(&src, w, h).iter().all(|&p| p == 0x0033_6699));
        // Two flat halves split by a straight vertical line stay two
        // flat halves with a straight edge.
        let src: Vec<u32> = (0..w * h)
            .map(|i| if i % w < 6 { 0 } else { 0x00ff_ffff })
            .collect();
        let d = up2(&src, w, h);
        for y in 0..2 * h {
            for x in 0..2 * w {
                let want = if x < 12 { 0 } else { 0x00ff_ffff };
                assert_eq!(d[y * 2 * w + x], want, "({x}, {y})");
            }
        }
    }

    /// A staircase (the pixels with x + y < 12, whose ideal edge is the
    /// line x + y = 12.5) comes out closer to the
    /// ideal straight edge than nearest, with fewer grey pixels than
    /// bilinear: xBR smooths the steps and keeps the edge sharp.
    #[test]
    fn xbr_straightens_a_diagonal() {
        let (w, h) = (32, 16);
        let white = 0x00ff_ffffu32;
        let src: Vec<u32> = (0..w * h)
            .map(|i| if (i % w) + 2 * (i / w) < 24 { 0 } else { white })
            .collect();
        let d = up2(&src, w, h);
        let (dw, dh) = (2 * w, 2 * h);
        let (mut err_xbr, mut err_nearest, mut grey_xbr, mut grey_bilinear) = (0, 0, 0, 0);
        let ax = Axis::new(w, dw);
        for y in 4..dh - 4 {
            for x in 4..dw - 4 {
                // The ideal: the sub-pixel's centre against the line.
                let cx = (x as f64 + 0.5) / 2.0;
                let cy = (y as f64 + 0.5) / 2.0;
                let ideal: i32 = if cx + 2.0 * cy < 25.0 { 0 } else { 255 };
                let v = (d[y * dw + x] & 0xff) as i32;
                let n = (src[(y / 2) * w + x / 2] & 0xff) as i32;
                err_xbr += (v - ideal).abs();
                err_nearest += (n - ideal).abs();
                if v != 0 && v != 255 {
                    grey_xbr += 1;
                }
                let b = sample_bilinear(&src, w, ax.lin[x], ax.lin[y]) & 0xff;
                if b != 0 && b != 255 {
                    grey_bilinear += 1;
                }
            }
        }
        assert!(err_xbr < err_nearest, "{err_xbr} vs nearest {err_nearest}");
        assert!(
            grey_xbr < grey_bilinear,
            "{grey_xbr} vs bilinear {grey_bilinear}"
        );
    }

    #[test]
    fn interface_scaler_is_deterministic_and_incremental() {
        let mut screen: Vec<u16> = (0..SCREEN_W * SCREEN_H)
            .map(|i| {
                if (i % SCREEN_W) / 7 % 2 == (i / SCREEN_W) / 5 % 2 {
                    0xf800
                } else {
                    0x07e0
                }
            })
            .collect();
        for factor in [2, 4] {
            let mut a = InterfaceScaler::new();
            a.update(&screen, factor, |_, _| true);
            assert_eq!(a.factor(), factor);
            assert_eq!(a.pixels().len(), factor * factor * SCREEN_W * SCREEN_H);
            // A change, updated incrementally, matches a fresh scaler.
            screen[100 * SCREEN_W + 100] = 0xffff;
            a.update(&screen, factor, |_, _| true);
            let mut b = InterfaceScaler::new();
            b.update(&screen, factor, |_, _| true);
            assert!(a.pixels() == b.pixels(), "factor {factor}");
        }
    }

    #[test]
    fn axis_maps_centres() {
        let a = Axis::new(640, 1280);
        assert_eq!(a.nearest[1279], 639);
        assert_eq!(a.lin[0], (0, 1, 0));
        assert_eq!(a.lin[1].0, 0);
        assert_eq!(a.lin[1279], (639, 639, 64));
        let same = Axis::new(640, 640);
        assert!(same
            .lin
            .iter()
            .enumerate()
            .all(|(i, &(a, _, w))| a == i && w == 0));
    }
}

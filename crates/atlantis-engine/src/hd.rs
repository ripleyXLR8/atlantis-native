//! The port's HD mode: the panoramic view and the talking heads drawn at
//! the output's resolution, everything else as the original's 640x480 art.
//!
//! The engine keeps drawing the original's 640x480 frame ([`Sys::screen`]):
//! the scripts, the hit tests and the tests see the same pixels with or
//! without HD. When a frame shows the panorama, the engine also draws the
//! [`HdLayer`]: the same view rendered at the HD size with bilinear
//! sampling ([`Warp::render_bilinear`]), with, in a dialog line, the UBB
//! body upscaled and the 3D head drawn at that size
//! ([`DialogHead::scaled`]), together with `base`, the 640x480 picture the
//! layer is the HD version of. What is drawn over it afterwards (the
//! cursor, the inventory bar, the guests, the subtitles, the menus) changes
//! the screen's pixels only: the front end shows the layer where the screen
//! still equals `base`, and the screen upscaled elsewhere ([`compose`]).
//!
//! The layer is valid for one frame: a frame that does not mark it
//! ([`Sys::hd_mark`]) is shown as the original's (videos, menus, puzzles).
//!
//! [`DialogHead::scaled`]: crate::head::DialogHead::scaled
//! [`Warp::render_bilinear`]: crate::warp::Warp::render_bilinear
//! [`Sys::screen`]: crate::sys::Sys::screen
//! [`Sys::hd_mark`]: crate::sys::Sys::hd_mark

use std::sync::OnceLock;

use crate::engine::Engine;
use crate::warp::{hd_size, SCREEN_H, SCREEN_W};

/// The largest HD tile size: 4 times 640x480.
pub const MAX_TILE: usize = 64;

/// The HD tile size for an output `width` x `height` pixels wide (the
/// 4:3 picture fitted in it), at most `max_tile`: the smallest tile whose
/// HD size covers the picture, `None` when the picture is not larger than
/// 640x480.
pub fn tile_for_output(width: usize, height: usize, max_tile: usize) -> Option<usize> {
    let pw = width.min(height * 4 / 3);
    let tile = pw.div_ceil(SCREEN_W / 16).min(max_tile.min(MAX_TILE));
    (tile > 16).then_some(tile)
}

/// The HD picture of a frame.
#[derive(Clone, Debug, Default)]
pub struct HdLayer {
    /// The tile size it was drawn at (16 = 640x480).
    pub tile: usize,
    pub width: usize,
    pub height: usize,
    /// XRGB8888, `width` x `height`.
    pub pixels: Vec<u32>,
    /// RGB565 640x480: the picture `pixels` stands for.
    pub base: Vec<u16>,
}

impl HdLayer {
    /// Sizes the layer for `tile` (its content is then undefined).
    pub fn resize(&mut self, tile: usize) {
        let (w, h) = hd_size(tile);
        self.tile = tile;
        self.width = w;
        self.height = h;
        self.pixels.resize(w * h, 0);
        self.base.resize(SCREEN_W * SCREEN_H, 0);
    }
}

/// RGB565 to XRGB8888, the channels widened by repeating their top bits.
pub fn rgb565_to_888() -> &'static [u32] {
    static LUT: OnceLock<Vec<u32>> = OnceLock::new();
    LUT.get_or_init(|| {
        (0..=u16::MAX as u32)
            .map(|p| {
                let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
                (r << 3 | r >> 2) << 16 | (g << 2 | g >> 4) << 8 | (b << 3 | b >> 2)
            })
            .collect()
    })
}

/// The front end's picture: the 640x480 `screen` scaled (nearest) to
/// `dw` x `dh` pixels at (`ox`, `oy`) of `out` (`stride` pixels per row,
/// XRGB8888), with the HD `layer`'s pixels where the screen still shows
/// the layer's `base`.
#[allow(clippy::too_many_arguments)]
pub fn compose(
    screen: &[u16],
    layer: &HdLayer,
    out: &mut [u32],
    stride: usize,
    ox: usize,
    oy: usize,
    dw: usize,
    dh: usize,
) {
    let lut = rgb565_to_888();
    let (w, h) = (SCREEN_W, SCREEN_H);
    let src_col: Vec<usize> = (0..dw).map(|x| x * w / dw).collect();
    let hd_col: Vec<usize> = (0..dw).map(|x| x * layer.width / dw).collect();
    for y in 0..dh {
        let sy = y * h / dh;
        let hy = y * layer.height / dh;
        let src = &screen[sy * w..][..w];
        let base = &layer.base[sy * w..][..w];
        let hd = &layer.pixels[hy * layer.width..][..layer.width];
        let row = &mut out[(oy + y) * stride + ox..][..dw];
        for ((d, &sx), &hx) in row.iter_mut().zip(&src_col).zip(&hd_col) {
            let p = src[sx];
            *d = if p == base[sx] {
                hd[hx]
            } else {
                lut[usize::from(p)]
            };
        }
    }
}

/// The 640x480 pixel under each HD column (or row) of a tile size.
fn low_res_map(n: usize, tile: usize) -> Vec<usize> {
    (0..n).map(|i| i * 16 / tile).collect()
}

/// A dialog line's HD picture into `layer` (sized for `tile`): the HD view
/// `view` where the 640x480 composite (`composite`: the UBB body and the
/// head over the view) shows the panorama (`pano`, the 640x480 view), the
/// composite upscaled elsewhere, then the HD head over it (`head` pixels
/// where `mask` is set, both at the HD size). Under the HD head's edges
/// is the original-size head, not the empty mouth of the video. The HD
/// head is kept to the original-size head's footprint (where `pre_head`,
/// the composite before the head, differs from `composite`) and the
/// pixels next to it: a sliver face that only the larger raster draws
/// stays out.
#[allow(clippy::too_many_arguments)]
pub fn compose_dialog(
    layer: &mut HdLayer,
    tile: usize,
    view: &[u32],
    pano: &[u16],
    pre_head: &[u16],
    composite: &[u16],
    head: Option<(&[u16], &[u8])>,
) {
    layer.resize(tile);
    let lut = rgb565_to_888();
    let (w, h) = (layer.width, layer.height);
    let cols = low_res_map(w, tile);
    let footprint = head.map(|_| head_footprint(pre_head, composite));
    for (hy, sy) in low_res_map(h, tile).into_iter().enumerate() {
        let p = &pano[sy * SCREEN_W..][..SCREEN_W];
        let c = &composite[sy * SCREEN_W..][..SCREEN_W];
        let v = &view[hy * w..][..w];
        let out = &mut layer.pixels[hy * w..][..w];
        for (x, (d, &sx)) in out.iter_mut().zip(&cols).enumerate() {
            *d = if c[sx] == p[sx] {
                v[x]
            } else {
                lut[usize::from(c[sx])]
            };
        }
        if let (Some((hp, hm)), Some(f)) = (head, &footprint) {
            let hp = &hp[hy * w..][..w];
            let hm = &hm[hy * w..][..w];
            let f = &f[sy * SCREEN_W..][..SCREEN_W];
            for x in 0..w {
                if hm[x] != 0 && f[cols[x]] != 0 {
                    out[x] = lut[usize::from(hp[x])];
                }
            }
        }
    }
}

/// The 640x480 pixels where the head was drawn (`before` and `after`
/// differ) and their eight neighbours.
fn head_footprint(before: &[u16], after: &[u16]) -> Vec<u8> {
    let (w, h) = (SCREEN_W, SCREEN_H);
    let mut f = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            if before[y * w + x] != after[y * w + x] {
                for yy in y.saturating_sub(1)..(y + 2).min(h) {
                    for xx in x.saturating_sub(1)..(x + 2).min(w) {
                        f[yy * w + xx] = 1;
                    }
                }
            }
        }
    }
    f
}

/// What a rendered HD view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewKey {
    alpha: u32,
    beta: u32,
    hfov: u32,
    surface: u64,
    tile: usize,
}

/// The engine's HD state.
#[derive(Default)]
pub(crate) struct HdState {
    /// Bumped at every change of the sprite-composited panorama.
    pub(crate) surface_gen: u64,
    /// That panorama in XRGB8888, and the generation it is of.
    pano: Vec<u32>,
    pano_gen: Option<u64>,
    /// The HD view and what it shows.
    pub(crate) view: Vec<u32>,
    view_key: Option<ViewKey>,
    /// The view [`Sys::hd`](crate::sys::Sys::hd) holds (`None`: something
    /// else, or nothing).
    layer_key: Option<ViewKey>,
}

impl Engine {
    /// The sprite-composited panorama changed.
    pub(crate) fn hd_surface_changed(&mut self) {
        self.hd.surface_gen = self.hd.surface_gen.wrapping_add(1);
    }

    /// The HD view of the current warp angles and FOV at `tile` into
    /// `self.hd.view`, rendered again only when it changed.
    pub(crate) fn hd_render_view(&mut self, tile: usize) {
        let key = ViewKey {
            alpha: self.warp.alpha().to_bits(),
            beta: self.warp.beta().to_bits(),
            hfov: self.warp.hfov().to_bits(),
            surface: self.hd.surface_gen,
            tile,
        };
        if self.hd.view_key == Some(key) {
            return;
        }
        let t0 = web_time_now();
        if self.hd.pano_gen != Some(self.hd.surface_gen) {
            let lut = rgb565_to_888();
            self.hd.pano.clear();
            self.hd
                .pano
                .extend(self.warp_surface.iter().map(|&p| lut[usize::from(p)]));
            self.hd.pano_gen = Some(self.hd.surface_gen);
        }
        let (w, h) = hd_size(tile);
        self.hd.view.resize(w * h, 0);
        self.warp
            .render_bilinear(&self.hd.pano, tile, &mut self.hd.view);
        // The render above wrapped the angles as the 640x480 one does.
        self.hd.view_key = Some(ViewKey {
            alpha: self.warp.alpha().to_bits(),
            beta: self.warp.beta().to_bits(),
            ..key
        });
        if let Some(t0) = t0 {
            log::trace!(
                "HD view {w}x{h}: {:.2} ms",
                t0.elapsed().as_secs_f64() * 1e3
            );
        }
    }

    /// The frame being drawn shows the view as [`Warp::render`] has just
    /// drawn it on the screen: the HD view becomes its HD layer, the screen
    /// its base. Nothing when HD is off or no panorama is loaded.
    ///
    /// [`Warp::render`]: crate::warp::Warp::render
    pub(crate) fn hd_show_view(&mut self) {
        self.hd_show_view_over(None);
    }

    /// [`Engine::hd_show_view`] with the view drawn into `base` rather than
    /// on the screen.
    pub(crate) fn hd_show_view_over(&mut self, base: Option<&[u16]>) {
        let Some(tile) = self.sys.hd_tile() else {
            return;
        };
        if !self.warp_loaded {
            return;
        }
        self.hd_render_view(tile);
        let mut layer = self.sys.hd.borrow_mut();
        if self.hd.layer_key != self.hd.view_key || layer.tile != tile {
            layer.resize(tile);
            layer.pixels.copy_from_slice(&self.hd.view);
            self.hd.layer_key = self.hd.view_key;
        }
        match base {
            Some(b) => layer.base.copy_from_slice(b),
            None => layer.base.copy_from_slice(&self.sys.screen.borrow()),
        }
        drop(layer);
        self.sys.hd_mark();
    }

    /// The HD layer was drawn over with something else than the view.
    pub(crate) fn hd_layer_replaced(&mut self) {
        self.hd.layer_key = None;
    }
}

/// The clock for the HD timings in the log (none on the web).
fn web_time_now() -> Option<std::time::Instant> {
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        log::log_enabled!(log::Level::Trace).then(std::time::Instant::now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_from_the_output_size() {
        assert_eq!(tile_for_output(640, 480, MAX_TILE), None);
        assert_eq!(tile_for_output(960, 720, MAX_TILE), Some(24));
        // 1920x1080: the 1440x1080 picture.
        assert_eq!(tile_for_output(1920, 1080, MAX_TILE), Some(36));
        assert_eq!(tile_for_output(1000, 750, MAX_TILE), Some(25));
        assert_eq!(tile_for_output(3840, 2160, MAX_TILE), Some(64));
        assert_eq!(tile_for_output(3840, 2160, 32), Some(32));
    }

    #[test]
    fn compose_takes_the_layer_where_the_screen_is_its_base() {
        let mut layer = HdLayer::default();
        layer.resize(32);
        layer.pixels.fill(0x0012_3456);
        layer.base.fill(7);
        let mut screen = vec![7u16; SCREEN_W * SCREEN_H];
        screen[0] = 0xffff; // drawn over: the screen's pixel
        let mut out = vec![0u32; 1280 * 960];
        compose(&screen, &layer, &mut out, 1280, 0, 0, 1280, 960);
        assert_eq!(out[0], 0x00ff_ffff);
        assert_eq!(out[1], 0x00ff_ffff);
        assert_eq!(out[1280], 0x00ff_ffff);
        assert_eq!(out[2], 0x0012_3456);
        assert!(out[1281 * 2..].iter().all(|&p| p == 0x0012_3456));
    }

    #[test]
    fn dialog_layer_keeps_the_body_and_draws_the_head() {
        let tile = 32;
        let (w, h) = hd_size(tile);
        let view = vec![0x0011_1111u32; w * h];
        let pano = vec![1u16; SCREEN_W * SCREEN_H];
        let mut pre = pano.clone();
        pre[SCREEN_W + 1] = 0xf800; // a body pixel at (1, 1)
        let mut post = pre.clone();
        post[2 * SCREEN_W + 3] = 0x07e0; // the original-size head at (3, 2)
        let head = vec![0x001fu16; w * h];
        let mut mask = vec![0u8; w * h];
        let mut layer = HdLayer::default();
        mask[5 * w + 6] = 1; // inside the footprint
        mask[40 * w + 40] = 1; // outside
        compose_dialog(
            &mut layer,
            tile,
            &view,
            &pano,
            &pre,
            &post,
            Some((&head, &mask)),
        );
        assert_eq!((layer.width, layer.height), (w, h));
        assert_eq!(layer.pixels[0], 0x0011_1111);
        assert_eq!(layer.pixels[2 * w + 2], 0x00ff_0000);
        assert_eq!(layer.pixels[3 * w + 3], 0x00ff_0000);
        assert_eq!(layer.pixels[4 * w + 4], 0x0011_1111);
        assert_eq!(layer.pixels[5 * w + 6], 0x0000_00ff);
        assert_eq!(layer.pixels[40 * w + 40], 0x0011_1111);
    }
}

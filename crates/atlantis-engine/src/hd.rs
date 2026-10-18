//! The port's HD mode: the panoramic view and the talking heads drawn at
//! the output's resolution, everything else as the original's 640x480 art.
//!
//! The engine keeps drawing the original's 640x480 frame ([`Sys::screen`]):
//! the scripts, the hit tests and the tests see the same pixels with or
//! without HD. When a frame shows the panorama, the engine also draws the
//! [`HdLayer`]: the same view rendered at the HD size (bilinear sampling,
//! [`Warp::render_bilinear`], or of the panorama doubled with Catmull-Rom
//! and sharpened, see [`HdOptions`]), with, in a dialog line, the UBB
//! body upscaled and the 3D head drawn at that size
//! ([`DialogHead::scaled`]), together with `base`, the 640x480 picture the
//! layer is the HD version of. What is drawn over it afterwards (the
//! cursor, the inventory bar, the guests, the subtitles, the menus) changes
//! the screen's pixels only: the front end shows the layer where the screen
//! still equals `base`, and the screen upscaled elsewhere ([`compose`]).
//!
//! The layer is valid for one frame: a frame that does not mark it
//! ([`Sys::hd_mark`]) has no HD layer (videos, menus, puzzles). The front
//! end's [`HdCompositor`] upscales what is not the layer (xBR for the art,
//! bilinear for the videos' frames) and draws the frame's text
//! ([`HdText`]: the subtitles, the options menu) at the output's
//! resolution.
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

/// How the HD view samples the panorama.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanoFilter {
    /// Bilinear: 2x2 texels.
    Bilinear,
    /// Bicubic: the panorama upsampled to twice its size with Catmull-Rom
    /// (without ringing), then sampled bilinearly: sharper.
    #[default]
    Bicubic,
}

impl PanoFilter {
    /// The value in `settings.ini`.
    pub fn name(self) -> &'static str {
        match self {
            PanoFilter::Bilinear => "bilinear",
            PanoFilter::Bicubic => "bicubic",
        }
    }

    pub fn from_name(s: &str) -> Option<PanoFilter> {
        match s.to_ascii_lowercase().as_str() {
            "bilinear" => Some(PanoFilter::Bilinear),
            "bicubic" => Some(PanoFilter::Bicubic),
            _ => None,
        }
    }
}

/// The HD mode's picture options (options menu, F10; `settings.ini`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HdOptions {
    /// The panorama's sampling.
    pub panorama: PanoFilter,
    /// A mild unsharp mask of the panorama before it is sampled
    /// ([`sharpen_panorama`](crate::upscale::sharpen_panorama)).
    pub sharpen: bool,
    /// The 640x480 art drawn over the HD picture (the cursor, the bars,
    /// the menus, the puzzles) upscaled with xBR, the videos' frames
    /// bilinearly; else both as their pixels (nearest).
    pub smooth_interface: bool,
    /// The subtitles (and the options menu's text) drawn at the output's
    /// resolution from the font's outlines.
    pub sharp_subtitles: bool,
    /// Widescreen: in a window wider than 4:3, the panoramic view fills
    /// the sides (the 4:3 picture, its interface and hit areas unchanged
    /// in the middle; black sides where there is no panorama).
    pub widescreen: bool,
}

impl Default for HdOptions {
    fn default() -> Self {
        HdOptions {
            panorama: PanoFilter::Bicubic,
            sharpen: true,
            smooth_interface: true,
            sharp_subtitles: true,
            widescreen: false,
        }
    }
}

/// A line of text drawn on the 640x480 screen with
/// [`SubtitleFont::draw`](crate::subtitles::SubtitleFont::draw): the
/// top-left of its line, its width limit, its colour (RGB565) and its
/// bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextItem {
    pub x: i32,
    pub y: i32,
    pub max_w: i32,
    pub color: u16,
    pub text: Vec<u8>,
    /// The 1997 build's bitmap font (its pixels drawn as sharp blocks),
    /// instead of the outline font.
    pub bitmap: Option<std::sync::Arc<crate::subtitles::BitmapFont>>,
}

/// The text of a frame, for the HD mode to draw at the output's
/// resolution ("Sharp subtitles"): the lines drawn, with the 640x480
/// picture before (`under`) and after (`after`) they were drawn. The
/// compositor shows `under` where the frame still shows the text's own
/// pixels (the frame equals `after` there and `after` differs from
/// `under`), so that the upscaled 640x480 glyphs do not show around the
/// sharp ones; what was drawn over the text afterwards (the cursor) stays.
#[derive(Clone, Debug, Default)]
pub struct HdText {
    pub items: Vec<TextItem>,
    pub under: Vec<u16>,
    pub after: Vec<u16>,
}

impl HdText {
    /// The text `items` drawn over `under`, giving `after` (both
    /// 640x480).
    pub fn set(&mut self, items: Vec<TextItem>, under: &[u16], after: &[u16]) {
        self.items = items;
        self.under.clear();
        self.under.extend_from_slice(under);
        self.after.clear();
        self.after.extend_from_slice(after);
    }

    /// The text's pixels in `frame` (640x480) put back to what was under
    /// them.
    pub fn uncover(&self, frame: &mut [u16]) {
        if self.under.len() != frame.len() || self.after.len() != frame.len() {
            return;
        }
        for ((p, &u), &a) in frame.iter_mut().zip(&self.under).zip(&self.after) {
            if *p == a && a != u {
                *p = u;
            }
        }
    }
}

/// What the front end shows of a frame in the HD mode.
#[derive(Clone, Copy)]
pub struct HdFrame<'a> {
    /// The 640x480 frame (with the options menu over it when open).
    pub screen: &'a [u16],
    /// The frame's HD layer, when it has one.
    pub layer: Option<&'a HdLayer>,
    /// The frame's text, drawn sharp (in this order).
    pub texts: &'a [&'a HdText],
    /// A video's frame (photographic: smoothed bilinearly rather than as
    /// pixel art).
    pub photo: bool,
}

/// The HD mode's composition of the front end's picture, with the state
/// it keeps between frames (the upscaled interface art, the font).
#[derive(Default)]
pub struct HdCompositor {
    scaler: crate::upscale::InterfaceScaler,
    frame: Vec<u16>,
    rgb: Vec<u32>,
    font: Option<crate::subtitles::SubtitleFont>,
    /// The axis maps for the current output size and factor.
    axes: Option<Axes>,
}

struct Axes {
    key: (usize, usize, usize, usize, usize, usize, usize, usize),
    /// 640x480 to the output.
    x: crate::upscale::Axis,
    y: crate::upscale::Axis,
    /// The `factor` x interface picture to the output.
    ux: crate::upscale::Axis,
    uy: crate::upscale::Axis,
    /// The layer's column of each output column.
    lx: Vec<usize>,
    ly: Vec<usize>,
    /// Widescreen: the layer's column of each side column (the left
    /// side's, then the right side's).
    side_lx: Vec<usize>,
}

/// The xBR factor for an output `dw` pixels wide: 2x up to 1600 pixels,
/// 4x above (then resampled to the size).
pub fn interface_factor(dw: usize) -> usize {
    if dw <= 1600 {
        2
    } else {
        4
    }
}

impl HdCompositor {
    pub fn new() -> HdCompositor {
        HdCompositor::default()
    }

    /// The picture of `f` scaled to `dw` x `dh` pixels at (`ox`, `oy`) of
    /// `out` (`stride` pixels per row, XRGB8888): the HD layer where the
    /// frame still shows its base; elsewhere the frame's pixels, upscaled
    /// with xBR (or bilinearly for a video) when `opts.smooth_interface`,
    /// as pixels otherwise; then the text drawn sharp when
    /// `opts.sharp_subtitles`.
    #[allow(clippy::too_many_arguments)]
    pub fn compose(
        &mut self,
        f: HdFrame<'_>,
        opts: HdOptions,
        out: &mut [u32],
        stride: usize,
        ox: usize,
        oy: usize,
        dw: usize,
        dh: usize,
    ) {
        use crate::upscale::{for_each_band, sample_bilinear, Axis};
        if dw == 0 || dh == 0 {
            return;
        }
        let (w, h) = (SCREEN_W, SCREEN_H);
        self.frame.clear();
        self.frame.extend_from_slice(f.screen);
        if opts.sharp_subtitles {
            for t in f.texts {
                t.uncover(&mut self.frame);
            }
        }
        let factor = interface_factor(dw);
        let (lw, lh, side) = f.layer.map_or((1, 1, 0), |l| (l.width, l.height, l.side));
        let cw = lw - 2 * side;
        // Widescreen: the output columns of each side the layer covers.
        let sd = if side > 0 { side * dw / cw } else { 0 };
        let left = ox.min(sd);
        let right = (stride - ox - dw).min(sd);
        let key = (dw, dh, factor, lw, lh, side, left, right);
        if self.axes.as_ref().is_none_or(|a| a.key != key) {
            self.axes = Some(Axes {
                key,
                x: Axis::new(w, dw),
                y: Axis::new(h, dh),
                ux: Axis::new(factor * w, dw),
                uy: Axis::new(factor * h, dh),
                lx: (0..dw).map(|x| side + x * cw / dw).collect(),
                side_lx: (0..left + right)
                    .map(|i| {
                        // Output column relative to the picture's left edge.
                        let rel = if i < left {
                            i as isize - left as isize
                        } else {
                            (dw + i - left) as isize
                        };
                        let c = side as isize + (rel * cw as isize).div_euclid(dw as isize);
                        c.clamp(0, lw as isize - 1) as usize
                    })
                    .collect(),
                ly: (0..dh).map(|y| y * lh / dh).collect(),
            });
        }
        let lut = rgb565_to_888();
        let frame = &self.frame;
        // How the pixels that are not the layer's are scaled.
        #[derive(Clone, Copy, PartialEq)]
        enum Ui {
            Nearest,
            Bilinear,
            Xbr,
        }
        let ui = match (opts.smooth_interface, f.photo) {
            (false, _) => Ui::Nearest,
            (true, true) => Ui::Bilinear,
            (true, false) => Ui::Xbr,
        };
        if ui == Ui::Bilinear {
            self.rgb.clear();
            self.rgb.extend(frame.iter().map(|&p| lut[usize::from(p)]));
        }
        if ui == Ui::Xbr {
            // With a layer, only the blocks that show something else
            // (and their neighbours, which the resampling reads).
            let need = f.layer.map(|l| {
                let mut b = vec![false; (w / 16) * (h / 16)];
                for y in 0..h {
                    for x in 0..w {
                        if frame[y * w + x] != l.base[y * w + x] {
                            b[(y / 16) * (w / 16) + x / 16] = true;
                        }
                    }
                }
                b
            });
            self.scaler.update(frame, factor, |bx, by| match &need {
                None => true,
                Some(b) => (by.saturating_sub(1)..(by + 2).min(h / 16)).any(|y| {
                    (bx.saturating_sub(1)..(bx + 2).min(w / 16)).any(|x| b[y * (w / 16) + x])
                }),
            });
        }
        let axes = self.axes.as_ref().expect("axes");
        let up = self.scaler.pixels();
        let rgb = &self.rgb;
        let layer = f.layer;
        let uw = factor * w;
        // The upscaled art at the output's size: copied.
        let exact = uw == dw && factor * h == dh;
        let rows = &mut out[oy * stride..(oy + dh) * stride];
        for_each_band(rows, stride, |first, band| {
            for (i, row) in band.chunks_mut(stride).enumerate() {
                let y = first + i;
                let sy = axes.y.nearest[y];
                let src = &frame[sy * w..][..w];
                if let (Some(l), false) = (layer, axes.side_lx.is_empty()) {
                    let hd = &l.pixels[axes.ly[y] * l.width..][..l.width];
                    let (lft, _) = (key.6, key.7);
                    for (i, &c) in axes.side_lx.iter().enumerate() {
                        let x = if i < lft {
                            ox - lft + i
                        } else {
                            ox + dw + i - lft
                        };
                        row[x] = hd[c];
                    }
                }
                let row = &mut row[ox..ox + dw];
                let base = layer.map(|l| &l.base[sy * w..][..w]);
                let hd = layer.map(|l| &l.pixels[axes.ly[y] * l.width..][..l.width]);
                for (x, d) in row.iter_mut().enumerate() {
                    let sx = axes.x.nearest[x];
                    let p = src[sx];
                    if let (Some(b), Some(hd)) = (base, hd) {
                        if p == b[sx] {
                            *d = hd[axes.lx[x]];
                            continue;
                        }
                    }
                    *d = match ui {
                        Ui::Nearest => lut[usize::from(p)],
                        Ui::Bilinear => sample_bilinear(rgb, w, axes.x.lin[x], axes.y.lin[y]),
                        Ui::Xbr if exact => up[y * uw + x],
                        Ui::Xbr => sample_bilinear(up, uw, axes.ux.lin[x], axes.uy.lin[y]),
                    };
                }
            }
        });
        if opts.sharp_subtitles && f.texts.iter().any(|t| !t.items.is_empty()) {
            let font = self
                .font
                .get_or_insert_with(crate::subtitles::SubtitleFont::new);
            for t in f.texts {
                for it in &t.items {
                    if let Some(b) = &it.bitmap {
                        b.draw_hd(
                            out,
                            stride,
                            (ox, oy, dw, dh),
                            &it.text,
                            it.x,
                            it.y,
                            lut[usize::from(it.color)],
                        );
                        continue;
                    }
                    font.draw_hd(
                        out,
                        stride,
                        (ox, oy, dw, dh),
                        &it.text,
                        it.x,
                        it.y,
                        it.max_w,
                        lut[usize::from(it.color)],
                    );
                }
            }
        }
    }
}

/// The HD picture of a frame.
#[derive(Clone, Debug, Default)]
pub struct HdLayer {
    /// The tile size it was drawn at (16 = 640x480).
    pub tile: usize,
    /// The whole width, sides included, and the height.
    pub width: usize,
    pub height: usize,
    /// Widescreen: the columns of panorama on each side of the 4:3
    /// picture (0 without), which is `width - 2 * side` wide.
    pub side: usize,
    /// XRGB8888, `width` x `height`.
    pub pixels: Vec<u32>,
    /// RGB565 640x480: the picture `pixels` stands for.
    pub base: Vec<u16>,
}

impl HdLayer {
    /// Sizes the layer for `tile`, with `side_tiles` tile columns on each
    /// side (its content is then undefined).
    pub fn resize(&mut self, tile: usize, side_tiles: usize) {
        let (w, h) = hd_size(tile);
        self.tile = tile;
        self.side = side_tiles * tile;
        self.width = w + 2 * self.side;
        self.height = h;
        self.pixels.resize(self.width * h, 0);
        self.base.resize(SCREEN_W * SCREEN_H, 0);
    }

    /// The width of the 4:3 picture, without the sides.
    pub fn centre_width(&self) -> usize {
        self.width - 2 * self.side
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
    let hd_col: Vec<usize> = (0..dw)
        .map(|x| layer.side + x * layer.centre_width() / dw)
        .collect();
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
    side_tiles: usize,
    view: &[u32],
    pano: &[u16],
    pre_head: &[u16],
    composite: &[u16],
    head: Option<(&[u16], &[u8])>,
    smooth: bool,
) {
    use crate::upscale::{sample_bilinear, Axis};
    layer.resize(tile, side_tiles);
    let lut = rgb565_to_888();
    // The 4:3 picture; the view (and the layer) has `side` more columns
    // on each side (widescreen), which show the view.
    let (w, h) = (layer.centre_width(), layer.height);
    let (side, lw) = (layer.side, layer.width);
    let cols = low_res_map(w, tile);
    let footprint = head.map(|_| head_footprint(pre_head, composite));
    // The body (a video) upscaled bilinearly with a smooth interface.
    let smooth = smooth.then(|| {
        let rgb: Vec<u32> = composite.iter().map(|&p| lut[usize::from(p)]).collect();
        (rgb, Axis::new(SCREEN_W, w), Axis::new(SCREEN_H, h))
    });
    for (hy, sy) in low_res_map(h, tile).into_iter().enumerate() {
        let p = &pano[sy * SCREEN_W..][..SCREEN_W];
        let c = &composite[sy * SCREEN_W..][..SCREEN_W];
        let full = &view[hy * lw..][..lw];
        let line = &mut layer.pixels[hy * lw..][..lw];
        line[..side].copy_from_slice(&full[..side]);
        line[side + w..].copy_from_slice(&full[side + w..]);
        let v = &full[side..side + w];
        let out = &mut line[side..side + w];
        for (x, (d, &sx)) in out.iter_mut().zip(&cols).enumerate() {
            *d = if c[sx] == p[sx] {
                v[x]
            } else if let Some((rgb, ax, ay)) = &smooth {
                sample_bilinear(rgb, SCREEN_W, ax.lin[x], ay.lin[hy])
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
    side: usize,
    filter: PanoFilter,
    sharpen: bool,
}

/// The engine's HD state.
#[derive(Default)]
pub(crate) struct HdState {
    /// Bumped at every change of the sprite-composited panorama.
    pub(crate) surface_gen: u64,
    /// That panorama as the view samples it (see
    /// [`PanoramaPrep`](crate::upscale::PanoramaPrep)), and the
    /// generation, sharpening and filter it is of.
    pano: crate::upscale::PanoramaPrep,
    pano_gen: Option<(u64, bool, PanoFilter)>,
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
        let opts = self.sys.hd_options();
        let key = ViewKey {
            alpha: self.warp.alpha().to_bits(),
            beta: self.warp.beta().to_bits(),
            hfov: self.warp.hfov().to_bits(),
            surface: self.hd.surface_gen,
            tile,
            side: self.sys.hd_side_tiles(),
            filter: opts.panorama,
            sharpen: opts.sharpen,
        };
        if self.hd.view_key == Some(key) {
            return;
        }
        let t0 = web_time_now();
        let gen = (self.hd.surface_gen, opts.sharpen, opts.panorama);
        if self.hd.pano_gen != Some(gen) {
            // (again only in the tiles that changed)
            self.hd.pano.update(
                &self.warp_surface,
                crate::warp::PANO_W,
                opts.sharpen,
                opts.panorama == PanoFilter::Bicubic,
            );
            self.hd.pano_gen = Some(gen);
        }
        self.warp.set_side_tiles(self.sys.hd_side_tiles());
        let (w, h) = self.warp.tiles_size(tile);
        self.hd.view.resize(w * h, 0);
        match opts.panorama {
            PanoFilter::Bilinear => {
                self.warp
                    .render_bilinear(self.hd.pano.pixels(), tile, &mut self.hd.view)
            }
            PanoFilter::Bicubic => {
                self.warp
                    .render_bilinear_2x(self.hd.pano.pixels(), tile, &mut self.hd.view)
            }
        }
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
        let side = self.warp.side_tiles();
        if self.hd.layer_key != self.hd.view_key || layer.tile != tile || layer.side != side * tile
        {
            layer.resize(tile, side);
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
        layer.resize(32, 0);
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

    fn pixels_opts() -> HdOptions {
        HdOptions {
            smooth_interface: false,
            sharp_subtitles: false,
            widescreen: false,
            ..HdOptions::default()
        }
    }

    #[test]
    fn compositor_with_pixels_is_the_plain_composition() {
        let mut layer = HdLayer::default();
        layer.resize(32, 0);
        for (i, p) in layer.pixels.iter_mut().enumerate() {
            *p = (i as u32).wrapping_mul(2_654_435_761) >> 8;
        }
        let screen: Vec<u16> = (0..SCREEN_W * SCREEN_H).map(|i| (i % 7) as u16).collect();
        layer.base = screen.iter().map(|&p| if p == 3 { 9 } else { p }).collect();
        for (dw, dh) in [(1280, 960), (1000, 750)] {
            let mut want = vec![0u32; dw * dh];
            compose(&screen, &layer, &mut want, dw, 0, 0, dw, dh);
            let mut got = vec![0u32; dw * dh];
            let f = HdFrame {
                screen: &screen,
                layer: Some(&layer),
                texts: &[],
                photo: false,
            };
            HdCompositor::new().compose(f, pixels_opts(), &mut got, dw, 0, 0, dw, dh);
            assert!(got == want, "{dw}x{dh}");
        }
    }

    /// Widescreen: the layer's sides fill the bars beside the 4:3
    /// picture; the picture itself is composed as without sides.
    #[test]
    fn compositor_fills_the_bars_with_the_sides() {
        let side_tiles = 4;
        let mut wide = HdLayer::default();
        wide.resize(32, side_tiles);
        assert_eq!(wide.side, 128);
        assert_eq!(wide.width, 1280 + 256);
        let mut narrow = HdLayer::default();
        narrow.resize(32, 0);
        for y in 0..wide.height {
            for x in 0..wide.width {
                let v = ((x as u32) << 12) | y as u32;
                wide.pixels[y * wide.width + x] = v;
                if (wide.side..wide.side + 1280).contains(&x) {
                    narrow.pixels[y * 1280 + x - wide.side] = v;
                }
            }
        }
        let screen: Vec<u16> = (0..SCREEN_W * SCREEN_H).map(|i| (i % 5) as u16).collect();
        wide.base = screen.iter().map(|&p| if p == 2 { 9 } else { p }).collect();
        narrow.base = wide.base.clone();
        // A 16:9 window: 1280x960 picture with 213-pixel bars... at 1706.
        let (sw, sh, ox) = (1706, 960, 213);
        let frame = |l| HdFrame {
            screen: &screen,
            layer: Some(l),
            texts: &[],
            photo: false,
        };
        let mut got = vec![0u32; sw * sh];
        HdCompositor::new().compose(frame(&wide), pixels_opts(), &mut got, sw, ox, 0, 1280, 960);
        let mut want = vec![0u32; sw * sh];
        HdCompositor::new().compose(
            frame(&narrow),
            pixels_opts(),
            &mut want,
            sw,
            ox,
            0,
            1280,
            960,
        );
        for y in 0..sh {
            assert_eq!(
                got[y * sw + ox..][..1280],
                want[y * sw + ox..][..1280],
                "row {y}"
            );
            // The 128 side columns fill the bar next to the picture,
            // continuing the layer's columns; past them, black.
            assert_eq!(got[y * sw + ox - 1], wide.pixels[y * wide.width + 127]);
            assert_eq!(got[y * sw + ox - 128], wide.pixels[y * wide.width]);
            assert_eq!(got[y * sw + ox - 129], 0);
            assert_eq!(
                got[y * sw + ox + 1280],
                wide.pixels[y * wide.width + 128 + 1280]
            );
            assert_eq!(
                got[y * sw + ox + 1280 + 127],
                wide.pixels[y * wide.width + wide.width - 1]
            );
            assert_eq!(got[y * sw + ox + 1280 + 128], 0);
        }
    }

    #[test]
    fn compositor_smooths_flat_art_flat_and_keeps_the_layer() {
        let mut layer = HdLayer::default();
        layer.resize(36, 0);
        layer.pixels.fill(0x0012_3456);
        let mut screen = vec![0x1234u16; SCREEN_W * SCREEN_H];
        layer.base = screen.clone();
        // A flat bar drawn over the view.
        for y in 400..480 {
            for x in 0..SCREEN_W {
                screen[y * SCREEN_W + x] = 0xf800;
            }
        }
        let (dw, dh) = (1440, 1080);
        let mut out = vec![0u32; dw * dh];
        let f = HdFrame {
            screen: &screen,
            layer: Some(&layer),
            texts: &[],
            photo: false,
        };
        let mut c = HdCompositor::new();
        c.compose(f, HdOptions::default(), &mut out, dw, 0, 0, dw, dh);
        assert_eq!(out[0], 0x0012_3456);
        assert_eq!(out[1000 * dw + 700], 0x00ff_0000);
        // Again (incremental): the same picture.
        let mut again = vec![0u32; dw * dh];
        c.compose(f, HdOptions::default(), &mut again, dw, 0, 0, dw, dh);
        assert!(again == out);
        // A video frame of a constant colour stays that colour.
        let flat = vec![0x7bef_u16; SCREEN_W * SCREEN_H];
        let v = HdFrame {
            screen: &flat,
            layer: None,
            texts: &[],
            photo: true,
        };
        c.compose(v, HdOptions::default(), &mut out, dw, 0, 0, dw, dh);
        let want = rgb565_to_888()[0x7bef];
        assert!(out.iter().all(|&p| p == want));
    }

    #[test]
    fn sharp_text_replaces_the_low_res_glyphs() {
        let font = crate::subtitles::SubtitleFont::new();
        let mut layer = HdLayer::default();
        layer.resize(48, 0);
        layer.pixels.fill(0x0020_4060);
        let under = vec![0x2104u16; SCREEN_W * SCREEN_H];
        layer.base = under.clone();
        let lines = vec![b" Sharp words".to_vec()];
        let mut after = under.clone();
        font.draw_subtitles(&mut after, SCREEN_W, SCREEN_H, &lines, 0);
        let mut text = HdText::default();
        text.set(
            crate::subtitles::subtitle_items(&lines, 0, 0),
            &under,
            &after,
        );
        // The cursor drawn over the text afterwards stays.
        let mut screen = after.clone();
        let y = crate::subtitles::subtitle_line_y(0, 1) as usize + 5;
        screen[y * SCREEN_W + 10] = 0x07e0;
        let mut u = screen.clone();
        text.uncover(&mut u);
        assert_eq!(u[y * SCREEN_W + 10], 0x07e0);
        assert!(u
            .iter()
            .enumerate()
            .all(|(i, &p)| p == under[i] || i == y * SCREEN_W + 10));
        let (dw, dh) = (1920, 1440);
        let mut sharp = vec![0u32; dw * dh];
        let mut plain = vec![0u32; dw * dh];
        let texts = [&text];
        let f = HdFrame {
            screen: &screen,
            layer: Some(&layer),
            texts: &texts,
            photo: false,
        };
        let mut c = HdCompositor::new();
        c.compose(f, HdOptions::default(), &mut sharp, dw, 0, 0, dw, dh);
        c.compose(f, pixels_opts(), &mut plain, dw, 0, 0, dw, dh);
        // Plain: white blocks of 3x3; sharp: smoothed glyphs (greys) over
        // the layer, and no pixel of the 640 text's shadow left as a block.
        let grey = |o: &[u32]| {
            o.iter()
                .filter(|&&p| p != 0x0020_4060 && p != 0x00ff_ffff && p != 0)
                .count()
        };
        assert!(grey(&sharp) > grey(&plain));
        assert!(sharp.contains(&0x00ff_ffff));
        assert_eq!(sharp[0], 0x0020_4060);
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
            0,
            &view,
            &pano,
            &pre,
            &post,
            Some((&head, &mask)),
            false,
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

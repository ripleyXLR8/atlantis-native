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
/// Widescreen: at most this many tile columns of panorama on each side of
/// the 4:3 picture (a 2.4:1 picture); a video's sides end there too.
pub const MAX_SIDE_TILES: usize = 16;

/// Widescreen: how far the sides reach from a picture `dw` pixels wide
/// (its `MAX_SIDE_TILES` tile columns, a tile being `dw / 40` pixels).
pub fn side_reach(dw: usize) -> usize {
    dw * MAX_SIDE_TILES / (SCREEN_W / 16)
}

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

/// Widescreen: what fills the bars beside a video's 4:3 frame (the
/// transition videos and the cinematics have no picture past their edges).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VideoSides {
    /// Black bars, as without widescreen.
    #[default]
    Black,
    /// The frame's edges, mirrored, blurred and darkened.
    Blur,
    /// The frame enlarged to the window's width, its top and bottom cut
    /// (the subtitles stay where the 4:3 picture has them).
    Zoom,
    /// A move's video: the panorama of the place left on the sides,
    /// crossfading to the destination's, with the blurred edges in the
    /// middle of the video; the cinematics get the blurred edges.
    Panorama,
}

impl VideoSides {
    /// The value in `settings.ini`.
    pub fn name(self) -> &'static str {
        match self {
            VideoSides::Black => "black",
            VideoSides::Blur => "blur",
            VideoSides::Zoom => "zoom",
            VideoSides::Panorama => "panorama",
        }
    }

    pub fn from_name(s: &str) -> Option<VideoSides> {
        match s.to_ascii_lowercase().as_str() {
            "black" => Some(VideoSides::Black),
            "blur" => Some(VideoSides::Blur),
            "zoom" => Some(VideoSides::Zoom),
            "panorama" => Some(VideoSides::Panorama),
            _ => None,
        }
    }

    /// The next value (the options menu's change).
    pub fn next(self) -> VideoSides {
        match self {
            VideoSides::Black => VideoSides::Blur,
            VideoSides::Blur => VideoSides::Zoom,
            VideoSides::Zoom => VideoSides::Panorama,
            VideoSides::Panorama => VideoSides::Black,
        }
    }
}

/// The panorama beside a move's video ([`VideoSides::Panorama`]): the
/// side columns of the HD view of the place left (`start`) and of the
/// destination (`end`, empty when unknown), each the left side then the
/// right side, `width` x `height` pixels per side, and the video's
/// progress `t` (0..1).
#[derive(Clone, Debug, Default)]
pub struct VideoPanorama {
    pub active: bool,
    /// The HD tile size the sides were rendered at.
    pub tile: usize,
    pub width: usize,
    pub height: usize,
    pub start: Vec<u32>,
    pub end: Vec<u32>,
    pub t: f32,
}

impl VideoPanorama {
    /// The sides of a wide view `view` (`vw` x `vh`, `side` columns on
    /// each side): left then right, row by row.
    pub fn sides_of(view: &[u32], vw: usize, vh: usize, side: usize) -> Vec<u32> {
        let mut out = Vec::with_capacity(2 * side * vh);
        for s in 0..2 {
            for y in 0..vh {
                let x0 = if s == 0 { 0 } else { vw - side };
                out.extend_from_slice(&view[y * vw + x0..][..side]);
            }
        }
        out
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
    /// Widescreen: the bars beside a video.
    pub video_sides: VideoSides,
}

impl Default for HdOptions {
    fn default() -> Self {
        HdOptions {
            panorama: PanoFilter::Bicubic,
            sharpen: true,
            smooth_interface: true,
            sharp_subtitles: true,
            widescreen: false,
            video_sides: VideoSides::Black,
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
    /// How the outline font reads the bytes (the release's code page).
    pub code_page: atlantis_data::TextCodePage,
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
    /// Widescreen, a move's video: the panorama beside it.
    pub video_pano: Option<&'a VideoPanorama>,
}

/// The HD mode's composition of the front end's picture, with the state
/// it keeps between frames (the upscaled interface art, the font).
#[derive(Default)]
pub struct HdCompositor {
    scaler: crate::upscale::InterfaceScaler,
    frame: Vec<u16>,
    rgb: Vec<u32>,
    /// The outline fonts, one per code page met.
    fonts: Vec<crate::subtitles::SubtitleFont>,
    /// The axis maps for the current output size and factor, and how
    /// many times they were made (see [`GpuPlan::axes_gen`]).
    axes: Option<Axes>,
    axes_gen: u64,
}

/// How the pixels that are not the HD layer's are scaled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiScale {
    /// As blocks.
    Nearest,
    /// Bilinearly (a video's frame).
    Bilinear,
    /// The xBR-upscaled art, resampled bilinearly to the output.
    Xbr,
}

/// What [`HdCompositor::prepare`] decided for a frame.
struct Prep {
    video_bars: Option<VideoSides>,
    ui: UiScale,
    factor: usize,
}

/// A frame's composition as [`HdCompositor::compose`] does it, for a
/// renderer elsewhere (the browser's graphics card) to do the same: the
/// frame (its text uncovered), how its pixels are scaled, the upscaled
/// interface art and its changed blocks, the maps from the output's
/// columns and rows to the sources, and the layer.
pub struct GpuPlan<'a> {
    pub frame: &'a [u16],
    pub ui: UiScale,
    pub factor: usize,
    /// The `factor` x interface art, and its blocks computed since the
    /// last plan (of `up_tile` pixels).
    pub up: &'a [u32],
    pub up_changes: crate::upscale::PanoChanges,
    pub up_tile: usize,
    pub x: &'a crate::upscale::Axis,
    pub y: &'a crate::upscale::Axis,
    pub ux: &'a crate::upscale::Axis,
    pub uy: &'a crate::upscale::Axis,
    pub lx: &'a [usize],
    pub ly: &'a [usize],
    /// Widescreen: the layer's column of each side column, the left
    /// side's (`left` of them) then the right side's.
    pub side_lx: &'a [usize],
    pub left: usize,
    pub right: usize,
    /// Changes when the maps above do.
    pub axes_gen: u64,
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

    /// The frame's text drawn at the output's resolution: a bitmap font's
    /// glyphs as sharp blocks, an outline font's lines rasterised at that
    /// size in the item's code page.
    fn draw_text_items(
        &mut self,
        f: &HdFrame<'_>,
        out: &mut [u32],
        stride: usize,
        area: (usize, usize, usize, usize),
        lut: &[u32],
        opaque: u32,
    ) {
        for t in f.texts {
            for it in &t.items {
                let color = opaque | lut[usize::from(it.color)];
                if let Some(b) = &it.bitmap {
                    b.draw_hd(out, stride, area, &it.text, it.x, it.y, color);
                    continue;
                }
                let i = match self
                    .fonts
                    .iter()
                    .position(|g| g.code_page() == it.code_page)
                {
                    Some(i) => i,
                    None => {
                        self.fonts
                            .push(crate::subtitles::SubtitleFont::with_code_page(it.code_page));
                        self.fonts.len() - 1
                    }
                };
                self.fonts[i].draw_hd(out, stride, area, &it.text, it.x, it.y, it.max_w, color);
            }
        }
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
        self.compose_with(f, opts, out, stride, (ox, oy, dw, dh), true);
    }

    /// [`HdCompositor::compose`], the text drawn only with `texts`.
    pub fn compose_with(
        &mut self,
        f: HdFrame<'_>,
        opts: HdOptions,
        out: &mut [u32],
        stride: usize,
        (ox, oy, dw, dh): (usize, usize, usize, usize),
        texts: bool,
    ) {
        use crate::upscale::{for_each_band, sample_bilinear};
        if dw == 0 || dh == 0 {
            return;
        }
        let (w, h) = (SCREEN_W, SCREEN_H);
        let Prep {
            video_bars,
            ui,
            factor,
        } = self.prepare(&f, opts, stride, ox, dw, dh);
        if video_bars == Some(VideoSides::Zoom) {
            self.compose_zoom(f, out, stride, (ox, oy, dw, dh));
            return;
        }
        let key = self.axes.as_ref().expect("axes").key;
        let lut = rgb565_to_888();
        let frame = &self.frame;
        let axes = self.axes.as_ref().expect("axes");
        // The view on the graphics card (`HdLayer::on_card`): its pixels
        // are left transparent (0) and the rest is opaque (alpha 0xff), a
        // premultiplied overlay the front end draws over its view.
        let holes = layer_on_card(f.layer);
        let opaque = if holes { 0xff00_0000 } else { 0 };
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
                        row[x] = if holes { 0 } else { hd[c] };
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
                            *d = if holes { 0 } else { hd[axes.lx[x]] };
                            continue;
                        }
                    }
                    *d = opaque
                        | match ui {
                            UiScale::Nearest => lut[usize::from(p)],
                            UiScale::Bilinear => {
                                sample_bilinear(rgb, w, axes.x.lin[x], axes.y.lin[y])
                            }
                            UiScale::Xbr if exact => up[y * uw + x],
                            UiScale::Xbr => sample_bilinear(up, uw, axes.ux.lin[x], axes.uy.lin[y]),
                        };
                }
            }
        });
        if let Some(v) = video_bars {
            self.fill_video_bars(v, f.video_pano, out, stride, (ox, oy, dw, dh));
        }
        if opts.sharp_subtitles && texts {
            self.draw_text_items(&f, out, stride, (ox, oy, dw, dh), lut, opaque);
        }
    }

    /// [`HdCompositor::compose`]'s work for a renderer elsewhere: the
    /// frame prepared as `compose` prepares it, or None for a frame it
    /// composes otherwise (a video's widescreen sides).
    #[allow(clippy::too_many_arguments)]
    pub fn gpu_plan<'a>(
        &'a mut self,
        f: &HdFrame<'_>,
        opts: HdOptions,
        stride: usize,
        ox: usize,
        dw: usize,
        dh: usize,
    ) -> Option<GpuPlan<'a>> {
        if dw == 0 || dh == 0 {
            return None;
        }
        self.scaler.track = true;
        let prep = self.prepare(f, opts, stride, ox, dw, dh);
        if prep.video_bars.is_some() {
            return None;
        }
        let up_changes = self.scaler.take_changes();
        let a = self.axes.as_ref()?;
        Some(GpuPlan {
            frame: &self.frame,
            ui: prep.ui,
            factor: prep.factor,
            up: self.scaler.pixels(),
            up_changes,
            up_tile: self.scaler.block_size(),
            x: &a.x,
            y: &a.y,
            ux: &a.ux,
            uy: &a.uy,
            lx: &a.lx,
            ly: &a.ly,
            side_lx: &a.side_lx,
            left: a.key.6,
            right: a.key.7,
            axes_gen: self.axes_gen,
        })
    }

    /// The frame's sharp text alone into `out` (`stride` pixels per row,
    /// cleared by the caller) as a premultiplied overlay (alpha in the top
    /// byte), drawn where [`HdCompositor::compose`] draws it; false when
    /// there is none.
    pub fn draw_texts_overlay(
        &mut self,
        f: &HdFrame<'_>,
        opts: HdOptions,
        out: &mut [u32],
        stride: usize,
        area: (usize, usize, usize, usize),
    ) -> bool {
        if !opts.sharp_subtitles || f.texts.iter().all(|t| t.items.is_empty()) {
            return false;
        }
        self.draw_text_items(f, out, stride, area, rgb565_to_888(), 0xff00_0000);
        true
    }

    /// The frame and the maps of a composition: the frame with its text
    /// uncovered, the video's sides, how the pixels are scaled, the axes,
    /// the upscaled art brought up to date.
    fn prepare(
        &mut self,
        f: &HdFrame<'_>,
        opts: HdOptions,
        stride: usize,
        ox: usize,
        dw: usize,
        dh: usize,
    ) -> Prep {
        use crate::upscale::Axis;
        let (w, h) = (SCREEN_W, SCREEN_H);
        self.frame.clear();
        self.frame.extend_from_slice(f.screen);
        // Widescreen sides of a video (a frame without an HD layer).
        let video_bars = (f.photo && f.layer.is_none() && opts.widescreen && ox > 0)
            .then_some(opts.video_sides)
            .filter(|&v| v != VideoSides::Black);
        if opts.sharp_subtitles || video_bars == Some(VideoSides::Zoom) {
            for t in f.texts {
                t.uncover(&mut self.frame);
            }
        }
        if video_bars == Some(VideoSides::Zoom) {
            return Prep {
                video_bars,
                ui: UiScale::Nearest,
                factor: 2,
            };
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
            self.axes_gen = self.axes_gen.wrapping_add(1);
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
        let ui = match (opts.smooth_interface, f.photo) {
            (false, _) => UiScale::Nearest,
            (true, true) => UiScale::Bilinear,
            (true, false) => UiScale::Xbr,
        };
        if ui == UiScale::Bilinear {
            self.rgb.clear();
            self.rgb.extend(frame.iter().map(|&p| lut[usize::from(p)]));
        }
        if ui == UiScale::Xbr {
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
        Prep {
            video_bars,
            ui,
            factor,
        }
    }
}

/// Whether `layer`'s view is on the graphics card (see [`HdLayer::on_card`]).
fn layer_on_card(layer: Option<&HdLayer>) -> bool {
    layer.is_some_and(|l| l.on_card)
}

impl HdCompositor {
    /// The frame's edges, blurred: the 640x480 frame averaged in 20x20
    /// blocks (32x24), then a 3x3 box blur of that.
    fn edge_blur(&self) -> Vec<[f32; 3]> {
        let lut = rgb565_to_888();
        let (bw, bh) = (SCREEN_W / 20, SCREEN_H / 20);
        let mut small = vec![[0f32; 3]; bw * bh];
        for by in 0..bh {
            for bx in 0..bw {
                let mut acc = [0u32; 3];
                for y in by * 20..by * 20 + 20 {
                    for x in bx * 20..bx * 20 + 20 {
                        let p = lut[usize::from(self.frame[y * SCREEN_W + x])];
                        acc[0] += (p >> 16) & 0xff;
                        acc[1] += (p >> 8) & 0xff;
                        acc[2] += p & 0xff;
                    }
                }
                small[by * bw + bx] = acc.map(|c| c as f32 / 400.0);
            }
        }
        let mut out = vec![[0f32; 3]; bw * bh];
        for y in 0..bh as isize {
            for x in 0..bw as isize {
                let mut acc = [0f32; 3];
                let mut n = 0.0;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (xx, yy) = (x + dx, y + dy);
                        if (0..bw as isize).contains(&xx) && (0..bh as isize).contains(&yy) {
                            let p = small[yy as usize * bw + xx as usize];
                            for c in 0..3 {
                                acc[c] += p[c];
                            }
                            n += 1.0;
                        }
                    }
                }
                out[y as usize * bw + x as usize] = acc.map(|c| c / n);
            }
        }
        out
    }

    /// Widescreen: the bars beside a video's frame drawn at (`ox`, `oy`)
    /// `dw` x `dh` of `out`.
    fn fill_video_bars(
        &self,
        v: VideoSides,
        pano: Option<&VideoPanorama>,
        out: &mut [u32],
        stride: usize,
        (ox, oy, dw, dh): (usize, usize, usize, usize),
    ) {
        let blur = self.edge_blur();
        let (bw, bh) = (SCREEN_W / 20, SCREEN_H / 20);
        // A bar shows the frame's outer quarter, mirrored at its edge.
        let reach = (bw / 4) as f32;
        let sample = |fx: f32, fy: f32| -> [f32; 3] {
            let fx = fx.clamp(0.0, (bw - 1) as f32);
            let fy = fy.clamp(0.0, (bh - 1) as f32);
            let (x0, y0) = (fx as usize, fy as usize);
            let (x1, y1) = ((x0 + 1).min(bw - 1), (y0 + 1).min(bh - 1));
            let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
            let p = |x: usize, y: usize| blur[y * bw + x];
            [0, 1, 2].map(|c| {
                let top = p(x0, y0)[c] * (1.0 - ax) + p(x1, y0)[c] * ax;
                let bottom = p(x0, y1)[c] * (1.0 - ax) + p(x1, y1)[c] * ax;
                top * (1.0 - ay) + bottom * ay
            })
        };
        let pack = |c: [f32; 3]| -> u32 {
            let q = |v: f32| v.round().clamp(0.0, 255.0) as u32;
            q(c[0]) << 16 | q(c[1]) << 8 | q(c[2])
        };
        let unpack = |p: u32| {
            [
                ((p >> 16) & 0xff) as f32,
                ((p >> 8) & 0xff) as f32,
                (p & 0xff) as f32,
            ]
        };
        // The panorama's share: all of it at the video's ends, the blur's
        // in its middle.
        let pano = pano.filter(|p| v == VideoSides::Panorama && p.active && p.width > 0);
        let (pano_w, blur_w) = match pano {
            Some(p) => {
                let m = 4.0 * p.t * (1.0 - p.t);
                (1.0 - m, m)
            }
            None => (0.0, 1.0),
        };
        // (no further than the panorama's sides reach)
        let reach_px = side_reach(dw);
        let (left_w, right_w) = (ox.min(reach_px), (stride - ox - dw).min(reach_px));
        for y in 0..dh {
            let fy = (y as f32 + 0.5) * bh as f32 / dh as f32 - 0.5;
            let row = &mut out[(oy + y) * stride..][..stride];
            for side in 0..2 {
                let bar = if side == 0 { left_w } else { right_w };
                for d in 0..bar {
                    // d: the distance from the picture's edge.
                    let x = if side == 0 { ox - 1 - d } else { ox + dw + d };
                    let u = (d as f32 + 0.5) / bar as f32 * reach;
                    let fx = if side == 0 {
                        u - 0.5
                    } else {
                        bw as f32 - u - 0.5
                    };
                    let b = sample(fx, fy);
                    // Darker away from the picture.
                    let k = 0.62 - 0.22 * (d as f32 / bar as f32);
                    let mut c = b.map(|v| v * k);
                    if let Some(p) = pano {
                        // The side picture's column at this distance: the
                        // sides have the picture's scale, its 40 * tile
                        // HD columns over `dw` output pixels.
                        let col = d * 40 * p.tile / dw.max(1);
                        if col < p.width {
                            let hy = (y * p.height / dh).min(p.height - 1);
                            let sx = if side == 0 { p.width - 1 - col } else { col };
                            let at = |v: &[u32]| v[side * p.width * p.height + hy * p.width + sx];
                            let s0 = unpack(at(&p.start));
                            let s = if p.end.len() == p.start.len() {
                                let e = unpack(at(&p.end));
                                [0, 1, 2].map(|i| s0[i] * (1.0 - p.t) + e[i] * p.t)
                            } else {
                                s0
                            };
                            c = [0, 1, 2].map(|i| s[i] * pano_w + c[i] * blur_w);
                        }
                    }
                    row[x] = pack(c);
                }
            }
        }
    }

    /// Widescreen, [`VideoSides::Zoom`]: the frame enlarged to the width
    /// the panorama's sides reach (the window's, up to a 2.4:1 picture;
    /// bilinear), its top and bottom cut; the frame's text is then drawn
    /// where the 4:3 picture has it.
    fn compose_zoom(
        &mut self,
        f: HdFrame<'_>,
        out: &mut [u32],
        stride: usize,
        (ox, oy, dw, dh): (usize, usize, usize, usize),
    ) {
        use crate::upscale::{sample_bilinear, Axis};
        let lut = rgb565_to_888();
        let rgb: Vec<u32> = self.frame.iter().map(|&p| lut[usize::from(p)]).collect();
        let sh = out.len() / stride;
        // The 4:3 picture at the width the sides reach.
        let reach_px = side_reach(dw);
        let x0 = ox - ox.min(reach_px);
        let zw = dw + ox.min(reach_px) + (stride - ox - dw).min(reach_px);
        let zh = zw * SCREEN_H / SCREEN_W;
        let ax = Axis::new(SCREEN_W, zw);
        let ay = Axis::new(SCREEN_H, zh);
        let top = zh.saturating_sub(sh) / 2;
        let pad = sh.saturating_sub(zh) / 2;
        for y in 0..sh.min(zh) {
            let zy = top + y;
            let row = &mut out[(pad + y) * stride + x0..][..zw];
            for (x, d) in row.iter_mut().enumerate() {
                *d = sample_bilinear(&rgb, SCREEN_W, ax.lin[x], ay.lin[zy]);
            }
        }
        self.draw_text_items(&f, out, stride, (ox, oy, dw, dh), lut, 0);
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
    /// The view is on the front end's renderer (the graphics card), as
    /// its drawing `card_gen`: `pixels` are not drawn then, and the front
    /// end composes the picture over its own copy (see
    /// [`ViewRenderer`]).
    pub on_card: bool,
    pub card_gen: u64,
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
    layer.on_card = false;
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

/// What a renderer of the HD view outside the engine needs: the view is
/// that of [`Warp::render_bilinear_2x`] (`doubled`) or
/// [`Warp::render_bilinear`] of `pano` with the grid of [`Warp::grid`],
/// `width` x `height` pixels.
///
/// [`Warp::render_bilinear_2x`]: crate::warp::Warp::render_bilinear_2x
/// [`Warp::render_bilinear`]: crate::warp::Warp::render_bilinear
/// [`Warp::grid`]: crate::warp::Warp::grid
pub struct ViewJob<'a> {
    pub grid: &'a [i32],
    pub cols: usize,
    pub tile: usize,
    pub width: usize,
    pub height: usize,
    pub doubled: bool,
    /// The prepared panorama (XRGB8888), its size, and the side of the
    /// tiles `changes` counts in.
    pub pano: &'a [u32],
    pub pano_size: (usize, usize),
    pub pano_tile: usize,
    /// The panorama's parts changed since the last job.
    pub changes: crate::upscale::PanoChanges,
    /// The drawing's number, which [`HdLayer::card_gen`] names.
    pub gen: u64,
}

/// A renderer of the HD view outside the engine (see [`Sys::view_renderer`]).
///
/// [`Sys::view_renderer`]: crate::sys::Sys::view_renderer
pub trait ViewRenderer {
    /// Draws the view of `job`, kept for the front end's picture, and
    /// into `out` (`width` x `height`) as well when given (read back,
    /// which waits for the renderer); false when it cannot, the engine
    /// then drawing it (the next job brings all of the panorama again).
    fn render(&mut self, job: ViewJob<'_>, out: Option<&mut [u32]>) -> bool;
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
    /// The HD view and what it shows; `view_on_cpu` false when only the
    /// outside renderer drew it (`card_gen`, its drawing's number).
    pub(crate) view: Vec<u32>,
    view_on_cpu: bool,
    card_gen: u64,
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
        let outside = self.sys.view_renderer.borrow().is_some();
        if outside != self.hd.pano.track {
            // A renderer appeared or went: it gets all of the panorama.
            self.hd.pano_gen = None;
            self.hd.pano = crate::upscale::PanoramaPrep::new();
            self.hd.pano.track = outside;
        }
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
        let doubled = opts.panorama == PanoFilter::Bicubic;
        let mut drawn = false;
        let check = self.sys.view_check.get();
        if let Some(r) = self.sys.view_renderer.borrow_mut().as_mut() {
            self.hd.card_gen = self.hd.card_gen.wrapping_add(1);
            let changes = self.hd.pano.take_changes();
            let (grid, cols) = self.warp.grid();
            let job = ViewJob {
                grid,
                cols,
                tile,
                width: w,
                height: h,
                doubled,
                pano: self.hd.pano.pixels(),
                pano_size: self.hd.pano.size(),
                pano_tile: self.hd.pano.tile_size(),
                changes,
                gen: self.hd.card_gen,
            };
            drawn = r.render(job, check.then_some(&mut self.hd.view[..]));
            if !drawn {
                // (the renderer is sent all of the panorama next time)
                log::warn!("HD view: the outside renderer failed, drawn by the engine");
            }
        }
        self.hd.view_on_cpu = !drawn || check;
        if drawn && check {
            let mut check = vec![0u32; w * h];
            match opts.panorama {
                PanoFilter::Bilinear => {
                    self.warp
                        .render_bilinear(self.hd.pano.pixels(), tile, &mut check)
                }
                PanoFilter::Bicubic => {
                    self.warp
                        .render_bilinear_2x(self.hd.pano.pixels(), tile, &mut check)
                }
            }
            let diff = check
                .iter()
                .zip(&self.hd.view)
                .filter(|(a, b)| a != b)
                .count();
            let first = check.iter().zip(&self.hd.view).position(|(a, b)| a != b);
            log::info!(
                "HD view check {w}x{h} tile {tile}: {diff} pixels differ{}",
                first.map_or(String::new(), |i| format!(
                    " (first at {},{}: engine {:06x}, outside {:06x})",
                    i % w,
                    i / w,
                    check[i],
                    self.hd.view[i]
                ))
            );
        }
        if !drawn {
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

    /// The HD view's pixels on the processor (after [`Engine::hd_render_view`],
    /// for the uses other than the front end's picture), drawn now when
    /// only the outside renderer drew it.
    pub(crate) fn hd_view_on_cpu(&mut self, tile: usize) {
        if self.hd.view_on_cpu {
            return;
        }
        let pano = self.hd.pano.pixels();
        match self.sys.hd_options().panorama {
            PanoFilter::Bilinear => self.warp.render_bilinear(pano, tile, &mut self.hd.view),
            PanoFilter::Bicubic => self.warp.render_bilinear_2x(pano, tile, &mut self.hd.view),
        }
        self.hd.view_on_cpu = true;
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
            layer.on_card = !self.hd.view_on_cpu;
            layer.card_gen = self.hd.card_gen;
            if self.hd.view_on_cpu {
                layer.pixels.copy_from_slice(&self.hd.view);
            }
            self.hd.layer_key = self.hd.view_key;
        }
        match base {
            Some(b) => layer.base.copy_from_slice(b),
            None => layer.base.copy_from_slice(&self.sys.screen.borrow()),
        }
        drop(layer);
        self.sys.hd_mark();
    }

    /// Widescreen, [`VideoSides::Panorama`]: before a move's video, the
    /// sides of the view now shown (the turn's last frame) and of the
    /// destination's panorama (`to`: its CYCLO name and the destination
    /// angles; `None` when the move changes area, whose panorama is not
    /// known yet). Without a destination the sides fade to the blur.
    pub(crate) async fn hd_video_panorama_start(&mut self, to: Option<(String, f32, f32)>) {
        self.sys.hd_video.borrow_mut().active = false;
        let opts = self.sys.hd_options();
        let side_tiles = self.sys.hd_side_tiles();
        let Some(tile) = self.sys.hd_tile() else {
            return;
        };
        if !opts.widescreen || opts.video_sides != VideoSides::Panorama || side_tiles == 0 {
            return;
        }
        if !self.warp_loaded {
            return;
        }
        self.hd_render_view(tile);
        self.hd_view_on_cpu(tile);
        let (vw, vh) = self.warp.tiles_size(tile);
        if self.hd.view.len() != vw * vh {
            return;
        }
        let side = side_tiles * tile;
        let start = VideoPanorama::sides_of(&self.hd.view, vw, vh, side);
        let mut end = Vec::new();
        if let Some((cyclo, alpha, beta)) = to {
            // As `load_cyclo`: a .tga name stands for the .hnm.
            let mut path = format!("CYCLO\\{cyclo}");
            if !cyclo.to_ascii_lowercase().ends_with(".hnm") && !self.game.big.contains(&path) {
                let stem = cyclo.rsplit_once('.').map_or(cyclo.as_str(), |(s, _)| s);
                path = format!("CYCLO\\{stem}.hnm");
            }
            if let Some(data) = self.read(&path).await {
                if let Ok(p) = atlantis_codecs::decode_panorama(&data) {
                    if p.width == crate::warp::PANO_W {
                        let lut = rgb565_to_888();
                        let pano: Vec<u32> =
                            p.pixels.iter().map(|&c| lut[usize::from(c)]).collect();
                        let mut w = crate::warp::Warp::new();
                        w.set_alpha(alpha);
                        w.set_beta(beta);
                        w.set_side_tiles(side_tiles);
                        let mut view = vec![0u32; vw * vh];
                        w.render_bilinear(&pano, tile, &mut view);
                        end = VideoPanorama::sides_of(&view, vw, vh, side);
                    }
                }
            }
        }
        *self.sys.hd_video.borrow_mut() = VideoPanorama {
            active: true,
            tile,
            width: side,
            height: vh,
            start,
            end,
            t: 0.0,
        };
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
            video_sides: VideoSides::Black,
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
                video_pano: None,
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
            video_pano: None,
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

    fn video_opts(v: VideoSides) -> HdOptions {
        HdOptions {
            widescreen: true,
            video_sides: v,
            sharp_subtitles: false,
            ..HdOptions::default()
        }
    }

    /// A video frame in a 16:9 window: 1280x960 picture at x = 213.
    fn video_compose(v: VideoSides, pano: Option<&VideoPanorama>, wide: bool) -> Vec<u32> {
        // Left half red, right half blue.
        let screen: Vec<u16> = (0..SCREEN_W * SCREEN_H)
            .map(|i| if i % SCREEN_W < 320 { 0xf800 } else { 0x001f })
            .collect();
        let f = HdFrame {
            screen: &screen,
            layer: None,
            texts: &[],
            photo: true,
            video_pano: pano,
        };
        let mut o = video_opts(v);
        o.widescreen = wide;
        let mut out = vec![0u32; 1706 * 960];
        HdCompositor::new().compose(f, o, &mut out, 1706, 213, 0, 1280, 960);
        out
    }

    #[test]
    fn video_bars_black_blurred_or_off() {
        let black = video_compose(VideoSides::Black, None, true);
        let off = video_compose(VideoSides::Blur, None, false);
        let blur = video_compose(VideoSides::Blur, None, true);
        for y in [0, 480, 959] {
            assert_eq!(black[y * 1706 + 100], 0);
            assert_eq!(off[y * 1706 + 100], 0);
            // The bars take the frame's edge colours, darkened.
            let l = blur[y * 1706 + 212];
            let r = blur[y * 1706 + 1706 - 213];
            assert!(l >> 16 > 0x60 && l & 0xff == 0, "left {l:06x}");
            assert!(r & 0xff > 0x60 && r >> 16 == 0, "right {r:06x}");
            assert!(l >> 16 < 0xff);
            // The picture itself is unchanged.
            assert_eq!(
                blur[y * 1706 + 213..][..1280],
                black[y * 1706 + 213..][..1280]
            );
        }
    }

    #[test]
    fn video_zoom_fills_the_width() {
        let z = video_compose(VideoSides::Zoom, None, true);
        // The 4:3 frame at 1706 wide is 1279 tall: the rows are cut, every
        // pixel is the frame's.
        for y in [0, 480, 959] {
            assert_eq!(z[y * 1706] >> 16, 0xff, "left edge red");
            assert_eq!(z[y * 1706 + 1705] & 0xff, 0xff, "right edge blue");
        }
        // The halves meet at the window's middle.
        assert_eq!(z[480 * 1706 + 850] >> 16, 0xff);
        assert_eq!(z[480 * 1706 + 856] & 0xff, 0xff);
    }

    /// A video frame in a 32:9 window (5120x1440): 1920x1440 picture at
    /// x = 1600, wider than the 2.4:1 the panorama's sides reach.
    fn video_compose_ultrawide(v: VideoSides) -> Vec<u32> {
        let screen: Vec<u16> = (0..SCREEN_W * SCREEN_H)
            .map(|i| if i % SCREEN_W < 320 { 0xf800 } else { 0x001f })
            .collect();
        let f = HdFrame {
            screen: &screen,
            layer: None,
            texts: &[],
            photo: true,
            video_pano: None,
        };
        let mut out = vec![0u32; 5120 * 1440];
        HdCompositor::new().compose(f, video_opts(v), &mut out, 5120, 1600, 0, 1920, 1440);
        out
    }

    #[test]
    fn video_sides_end_where_the_panorama_does() {
        // The sides reach 16 tiles of 48 pixels: 768 pixels each side,
        // columns 832..4288.
        let (a, b) = (1600 - 768, 1600 + 1920 + 768);
        for v in [VideoSides::Zoom, VideoSides::Blur] {
            let o = video_compose_ultrawide(v);
            for y in [0, 720, 1439] {
                let row = &o[y * 5120..][..5120];
                assert_eq!(row[a - 1], 0, "{v:?}: black left of the reach");
                assert_eq!(row[b], 0, "{v:?}: black right of the reach");
                assert!(row[..a].iter().chain(&row[b..]).all(|&p| p == 0));
                assert_ne!(row[a], 0, "{v:?}: drawn at the reach's left edge");
                assert_ne!(row[b - 1], 0, "{v:?}: drawn at the reach's right edge");
            }
        }
        // Zoom: the 4:3 frame at 3456 wide, its halves meeting in the
        // middle.
        let z = video_compose_ultrawide(VideoSides::Zoom);
        assert_eq!(z[720 * 5120 + a] >> 16, 0xff, "left edge red");
        assert_eq!(z[720 * 5120 + b - 1] & 0xff, 0xff, "right edge blue");
        assert_eq!(z[720 * 5120 + 2550] >> 16, 0xff);
        assert_eq!(z[720 * 5120 + 2570] & 0xff, 0xff);
    }

    #[test]
    fn video_panorama_crossfades_its_sides() {
        let (side, h) = (128, 960);
        let mut p = VideoPanorama {
            active: true,
            tile: 32,
            width: side,
            height: h,
            start: vec![0x0000_ff00; 2 * side * h],
            end: vec![0x00ff_ffff; 2 * side * h],
            t: 0.0,
        };
        let at0 = video_compose(VideoSides::Panorama, Some(&p), true);
        p.t = 1.0;
        let at1 = video_compose(VideoSides::Panorama, Some(&p), true);
        p.t = 0.5;
        let mid = video_compose(VideoSides::Panorama, Some(&p), true);
        let y = 480;
        // The side columns next to the picture: the start, then the end.
        assert_eq!(at0[y * 1706 + 212], 0x0000_ff00);
        assert_eq!(at1[y * 1706 + 212], 0x00ff_ffff);
        assert_eq!(at0[y * 1706 + 1706 - 213], 0x0000_ff00);
        // Past the side columns (128 HD columns = 128 output pixels at
        // this scale), the blur.
        assert_ne!(at0[y * 1706 + 40], 0x0000_ff00);
        // In the middle, the blur only.
        let blur = video_compose(VideoSides::Blur, None, true);
        assert_eq!(mid[y * 1706 + 212], blur[y * 1706 + 212]);
        // A cinematic (no panorama): the blur.
        let cine = video_compose(VideoSides::Panorama, None, true);
        assert_eq!(cine, blur);
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
            video_pano: None,
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
            video_pano: None,
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
            crate::subtitles::subtitle_items(
                &lines,
                0,
                0,
                atlantis_data::TextCodePage::Windows1252,
            ),
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
            video_pano: None,
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

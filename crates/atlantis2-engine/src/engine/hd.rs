//! The HD mode's part in the engine ([`crate::hd`]): after the cube is
//! drawn at 640 x 480 (the original's pixels, unchanged), the same view
//! is drawn at the front end's HD size from the same faces (their sprites
//! in) and the same camera, with the 640x480 cube it stands for
//! (`HdLayer::base`). In a dialog line the layer is the first game's
//! dialog picture (`atlantis_engine::hd::compose_dialog`): the view, the
//! body video upscaled over it, and the head drawn again at the HD size by
//! the same pipeline ([`crate::head::Head::draw_scaled`]). On the spider's
//! web the creatures are drawn again over the HD view, and the layer stands
//! for the 640x480 frame with them. The texts of a
//! frame (the subtitles, the tooltips, the bag's names, the credits) are
//! recorded for the front end to draw again at its size
//! ([`crate::hd::HdTexts`]). The engine does nothing of this with the HD
//! mode off, or without an HD size from the front end.

use super::Engine;
use crate::hd::{DialogHd, HdLayer, HdPanorama, HdProjection, HdTexts, TextFont};

/// The engine's HD state.
#[derive(Default)]
pub(crate) struct HdState {
    /// The HD tile size (the picture is 40 x 30 tiles; 16 is 640 x 480)
    /// and the widescreen tile columns on each side, from the front end.
    tile: Option<usize>,
    side_tiles: usize,
    layer: HdLayer,
    pano: HdPanorama,
    /// The frame on screen shows the layer's view (where it still shows
    /// its base), a video's frame (photographic).
    shown: bool,
    photo: bool,
    /// What the current pass drew last: the cube, a video's frame.
    drew_scene: bool,
    drew_video: bool,
    /// The screen at the end of the last pass that drew: a pass that
    /// leaves it as it was keeps what it shows.
    after: Vec<u16>,
    /// The time the last HD view took (the faces prepared and the view
    /// drawn, and in a dialog the head and the composition), in
    /// milliseconds.
    view_ms: f64,
    /// Nested passes (a render inside a pass): only the outer one starts
    /// and ends the frame.
    depth: u32,
    /// The texts the current pass draws, and those of the frame on
    /// screen.
    texts: HdTexts,
    shown_texts: HdTexts,
    /// A dialog frame's pictures, and the view under it (the layer then
    /// holds the dialog's picture).
    dialog: DialogHd,
    view: Vec<u32>,
    /// What the last HD view drawn shows (the projection's floats, the
    /// sizes, the options), and whether it is in the layer (else in
    /// `view`, the layer holding a dialog's picture): a frame with the
    /// same view and unchanged faces is not drawn again.
    view_key: Option<ViewKey>,
    view_in_layer: bool,
    /// The web's creatures at the HD size and their Z buffer.
    web_pixels: Vec<u16>,
    web_z: Vec<u32>,
}

/// What an HD view shows, besides the faces.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ViewKey {
    matrix: [u32; 9],
    proj: [u32; 4],
    tile: usize,
    side: usize,
    options: crate::hd::HdOptions,
}

impl HdState {
    /// The text recorder of the current pass.
    pub(super) fn texts_mut(&mut self) -> &mut HdTexts {
        &mut self.texts
    }

    /// What a dialog frame draws with: the dialog's capture, when the HD
    /// view of the frame was drawn (`active`: the HD mode draws), and the
    /// text recorder.
    pub(super) fn frame_parts(&mut self, active: bool) -> (Option<&mut DialogHd>, &mut HdTexts) {
        let dialog = match self.tile {
            Some(tile) if active && self.drew_scene => {
                self.dialog.tile = tile;
                Some(&mut self.dialog)
            }
            _ => None,
        };
        (dialog, &mut self.texts)
    }
}

impl Engine {
    /// The HD picture the front end shows: `tile` (`None`: no HD
    /// picture; see `crate::hd::tile_for_output`) and the widescreen tile
    /// columns on each side (`crate::hd::side_tiles`). Used only with
    /// [`crate::Enhancements::hd`] and a 640 x 480 screen.
    pub fn set_hd_output(&mut self, tile: Option<usize>, side_tiles: usize) {
        self.hd.tile = tile.filter(|&t| t > 0);
        self.hd.side_tiles = side_tiles;
    }

    /// Whether the frames are drawn in HD too.
    pub fn hd_active(&self) -> bool {
        self.enhancements.hd
            && self.hd.tile.is_some()
            && self.renderer.width() == crate::hd::SCREEN_W
            && self.renderer.height() == crate::hd::SCREEN_H
    }

    /// The HD layer of the frame on screen: the view at the HD size and
    /// the 640x480 cube it stands for; `None` when the frame shows no
    /// cube (a video, a menu page) or the HD mode is off.
    pub fn hd_layer(&self) -> Option<&HdLayer> {
        (self.hd_active() && self.hd.shown).then_some(&self.hd.layer)
    }

    /// Whether the frame on screen is a video's (photographic: the front
    /// end smooths it rather than upscaling it as pixel art).
    pub fn hd_photo(&self) -> bool {
        self.hd_active() && self.hd.photo
    }

    /// The time the last HD view took, in milliseconds.
    pub fn hd_view_ms(&self) -> f64 {
        self.hd.view_ms
    }

    /// The last dialog frame's HD capture (for tests and tools).
    #[doc(hidden)]
    pub fn hd_dialog_capture(&self) -> &DialogHd {
        &self.hd.dialog
    }

    /// The web's last Z buffers, the 640x480 frame's and the HD view's
    /// (non-zero where the creatures were drawn; for tests and tools).
    #[doc(hidden)]
    pub fn hd_web_capture(&self) -> (&[u32], &[u32]) {
        (&self.web_z, &self.hd.web_z)
    }

    /// The texts of the frame on screen, to draw at the HD size (their
    /// 640x480 glyphs uncovered by the compositor: `HdTexts::text`).
    pub fn hd_texts(&self) -> Option<&HdTexts> {
        self.hd_active().then_some(&self.hd.shown_texts)
    }

    /// The texts of the frame on screen drawn at the output's resolution
    /// into `out` (XRGB8888, `stride` pixels per row), the 640x480 screen
    /// being the `area` (x, y, width, height) of it
    /// ([`crate::subtitle::SubtitleFont::draw_hd`]).
    pub fn draw_hd_texts(
        &self,
        out: &mut [u32],
        stride: usize,
        area: (usize, usize, usize, usize),
    ) {
        let Some(t) = self.hd_texts() else { return };
        let popup = self
            .popup_font
            .as_ref()
            .or_else(|| self.menu.as_ref().and_then(|m| m.font()));
        for r in &t.runs {
            let font = match r.font {
                TextFont::Dialog => self.font.as_ref(),
                TextFont::Popup => popup,
            };
            if let Some(f) = font {
                f.draw_hd(out, stride, area, &r.call, r.alpha);
            }
        }
    }

    /// A pass (or a render) starts: nothing drawn yet.
    pub(super) fn hd_pass_start(&mut self) {
        self.hd.depth += 1;
        if self.hd.depth > 1 {
            return;
        }
        self.hd.drew_scene = false;
        self.hd.drew_video = false;
        let on = self.hd_active();
        self.hd.texts.begin(on);
    }

    /// The dialog frame was drawn (its pictures in the dialog's capture):
    /// the HD layer becomes the dialog's picture (the first game's
    /// `compose_dialog`) standing for the 640x480 frame before the
    /// subtitles.
    pub(super) fn hd_dialog(&mut self) {
        let hd = &mut self.hd;
        if !hd.dialog.ready || !hd.drew_scene {
            return;
        }
        hd.dialog.ready = false;
        let Some(tile) = hd.tile else { return };
        let t0 = now();
        if hd.view_in_layer {
            std::mem::swap(&mut hd.view, &mut hd.layer.pixels);
            hd.view_in_layer = false;
        }
        let d = &hd.dialog;
        let head = d.drawn.then_some((&d.head[..], &d.mask[..]));
        atlantis_engine::hd::compose_dialog(
            &mut hd.layer,
            tile,
            hd.side_tiles,
            &hd.view,
            &d.pano,
            &d.pre_head,
            &d.composite,
            &d.composite,
            &d.body,
            head,
            self.enhancements.hd_options.smooth_interface,
        );
        hd.layer.base.copy_from_slice(&d.composite);
        if let Some(t0) = t0 {
            hd.view_ms += t0.elapsed().as_secs_f64() * 1e3;
        }
    }

    /// The cube was just drawn into `out` (640 x 480) with the camera's
    /// `matrix` and `proj`, from the faces as they are now (the sprites
    /// in): the same view at the HD size.
    pub(super) fn hd_scene(&mut self, out: &[u16], matrix: &[f32; 9], proj: [f32; 4]) {
        if !self.hd_active() {
            return;
        }
        let Some(tile) = self.hd.tile else { return };
        let t0 = now();
        let hd = &mut self.hd;
        hd.layer.resize(tile, hd.side_tiles);
        hd.layer
            .base
            .copy_from_slice(&out[..crate::hd::SCREEN_W * crate::hd::SCREEN_H]);
        let options = self.enhancements.hd_options;
        let changed = hd.pano.update(&self.work, &options);
        let key = ViewKey {
            matrix: matrix.map(f32::to_bits),
            proj: proj.map(f32::to_bits),
            tile,
            side: hd.side_tiles,
            options,
        };
        if changed == 0 && hd.view_key == Some(key) {
            // The same view (a dialog's, a still picture): drawn already.
            if !hd.view_in_layer {
                std::mem::swap(&mut hd.view, &mut hd.layer.pixels);
                hd.view_in_layer = true;
            }
        } else {
            let p = HdProjection::new(matrix, proj, tile, hd.layer.side);
            let (w, h) = (hd.layer.width, hd.layer.height);
            hd.pano.render(&p, w, h, &mut hd.layer.pixels);
            hd.view_key = Some(key);
            hd.view_in_layer = true;
        }
        hd.drew_scene = true;
        hd.drew_video = false;
        if let Some(t0) = t0 {
            hd.view_ms = t0.elapsed().as_secs_f64() * 1e3;
        }
    }

    /// The web's creatures were just drawn over the cube with `view`
    /// (`engine/web.rs`) into `frame`: drawn again over the HD view, at
    /// its size, by the same pipeline with the camera scaled (as the
    /// dialog heads). The layer then stands for `frame` (its base), so
    /// that the front end shows it where the screen still shows the cube
    /// and the creatures.
    pub(super) fn hd_web(&mut self, view: &crate::atl3d::View, frame: &[u16]) {
        if !self.hd_active() || !self.hd.drew_scene {
            return;
        }
        let Some(tile) = self.hd.tile else { return };
        let t0 = now();
        let k = self.camera.fpu;
        let (w, h, side) = (
            self.hd.layer.width,
            self.hd.layer.height,
            self.hd.layer.side,
        );
        let v = view.scaled(k, tile, side, w, h);
        let mut pixels = std::mem::take(&mut self.hd.web_pixels);
        let mut zbuf = std::mem::take(&mut self.hd.web_z);
        pixels.clear();
        pixels.resize(w * h, 0);
        zbuf.clear();
        zbuf.resize(w * h, 0);
        {
            let mut t = crate::raster3d::Target {
                pixels: &mut pixels,
                zbuf: &mut zbuf,
                width: w,
                height: h,
            };
            let player = self.view as i32;
            self.web.redraw(k, player, &v, &mut t);
        }
        let lut = crate::hd::rgb565_to_888();
        let hd = &mut self.hd;
        for (i, &z) in zbuf.iter().enumerate() {
            if z != 0 {
                hd.layer.pixels[i] = lut[usize::from(pixels[i])];
            }
        }
        hd.layer
            .base
            .copy_from_slice(&frame[..crate::hd::SCREEN_W * crate::hd::SCREEN_H]);
        // The layer no longer shows the view alone: the next frame draws
        // it again.
        hd.view_key = None;
        hd.web_pixels = pixels;
        hd.web_z = zbuf;
        if let Some(t0) = t0 {
            hd.view_ms += t0.elapsed().as_secs_f64() * 1e3;
        }
    }

    /// A video's frame was just drawn over the screen.
    pub(super) fn hd_video(&mut self) {
        self.hd.drew_video = true;
        self.hd.drew_scene = false;
    }

    /// A pass (or a render) ends, having drawn into `out` when given:
    /// what the frame shows now.
    pub(super) fn hd_pass_end(&mut self, out: Option<&[u16]>) {
        self.hd.depth = self.hd.depth.saturating_sub(1);
        // Nothing drawn (a pass without pixels): the screen shows what it
        // showed.
        let (0, Some(out)) = (self.hd.depth, out) else {
            return;
        };
        if !self.hd_active() {
            self.hd.shown = false;
            self.hd.photo = false;
            return;
        }
        let hd = &mut self.hd;
        if hd.drew_scene || hd.drew_video {
            hd.shown = hd.drew_scene;
            hd.photo = hd.drew_video;
        } else if hd.after[..] != out[..] {
            // Something else was drawn (a menu page).
            hd.shown = false;
            hd.photo = false;
        } else {
            // The screen left as it was: it shows what it showed (and
            // its texts).
            return;
        }
        // The frame's texts are those this pass drew.
        std::mem::swap(&mut hd.texts, &mut hd.shown_texts);
        hd.after.clear();
        hd.after.extend_from_slice(out);
    }
}

/// The clock of the HD timings (none on the web, which has no
/// `Instant`).
fn now() -> Option<std::time::Instant> {
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Some(std::time::Instant::now())
    }
}

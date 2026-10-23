//! The HD mode's part in the engine ([`crate::hd`]): after the cube is
//! drawn at 640 x 480 (the original's pixels, unchanged), the same view
//! is drawn at the front end's HD size from the same faces (their sprites
//! in) and the same camera, with the 640x480 cube it stands for
//! (`HdLayer::base`). The engine does nothing of this with the HD mode
//! off, or without an HD size from the front end.

use super::Engine;
use crate::hd::{HdLayer, HdPanorama, HdProjection};

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
    /// drawn), in milliseconds.
    view_ms: f64,
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

    /// A pass (or a render) starts: nothing drawn yet.
    pub(super) fn hd_pass_start(&mut self) {
        self.hd.drew_scene = false;
        self.hd.drew_video = false;
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
        hd.pano.update(&self.work, &self.enhancements.hd_options);
        let p = HdProjection::new(matrix, proj, tile, hd.layer.side);
        let (w, h) = (hd.layer.width, hd.layer.height);
        hd.pano.render(&p, w, h, &mut hd.layer.pixels);
        hd.drew_scene = true;
        hd.drew_video = false;
        if let Some(t0) = t0 {
            hd.view_ms = t0.elapsed().as_secs_f64() * 1e3;
        }
    }

    /// A video's frame was just drawn over the screen.
    pub(super) fn hd_video(&mut self) {
        self.hd.drew_video = true;
        self.hd.drew_scene = false;
    }

    /// A pass (or a render) drew into `out`: what the frame shows now.
    pub(super) fn hd_pass_end(&mut self, out: &[u16]) {
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
            // The screen left as it was: it shows what it showed.
            return;
        }
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

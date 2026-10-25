//! The engine's part of the 800 x 600 mode ([`crate::display`]): the
//! screen size from `atlant2.cfg`, the mode of each loop type, the frame
//! drawn on the 640 x 480 surface and enlarged. At 640 x 480 nothing of
//! this changes what the engine does (not even the camera reset the
//! original does at each loop type change: see `apply_mode`).

use super::{Engine, LoopType};
use crate::camera::WARP_FOV;
use crate::display::{blit_640_to_800, native_dialog, screen_for_cfg, SMALL_H, SMALL_W};

impl Engine {
    /// The size of the frames [`Engine::pass`] and [`Engine::render`]
    /// draw (the screen's: 640 x 480, or 800 x 600 with the options page's
    /// resolution, `atlant2.cfg`). It changes at the start of a pass, after
    /// the options page's button or a new `atlant2.cfg`; the HD mode keeps
    /// 640 x 480 (its picture is drawn from the 640 x 480 screen).
    pub fn screen_size(&self) -> (usize, usize) {
        if self.enhancements.hd {
            (SMALL_W, SMALL_H)
        } else {
            screen_for_cfg(self.cfg_extra.width)
        }
    }

    /// The original's screen option (its options page's button, action
    /// 14), from elsewhere: the port's options menu or the command line.
    /// Kept in `atlant2.cfg`; the screen changes at the next pass (not in
    /// the HD mode, whose screen stays 640 x 480).
    pub fn set_screen_800(&mut self, on: bool) {
        let (w, h) = if on { (800, 600) } else { (640, 480) };
        self.cfg_extra.width = w;
        self.cfg_extra.height = h;
        if let Some(m) = self.menu.as_mut() {
            m.screen_changed(w);
        }
    }

    /// Whether the screen is 800 x 600 (the screen's width read by
    /// FUN_00402dd0).
    pub(crate) fn display_large(&self) -> bool {
        self.display.large()
    }

    /// The screen size the dialogs' fields of view are computed with
    /// (@ 0x004803e5, 0x0048096e: the screen's, not the logical one).
    pub(crate) fn fov_screen(&self) -> (u32, u32) {
        (self.display.width as u32, self.display.height as u32)
    }

    /// `AtlMain_SetLoopType` (FUN_0049da90): the loop type, and the mode
    /// it draws in (FUN_00402960: native for the panorama).
    pub(crate) fn set_loop_type(&mut self, t: LoopType) {
        self.loop_type = t;
        self.apply_mode(t == LoopType::Warp);
    }

    /// `AtlDial_start` (FUN_0047ff60): after the loop type 2, dialog
    /// `number` drawn natively when FUN_00480110 lists it.
    pub(crate) fn dialog_mode(&mut self, number: u32) {
        if native_dialog(number) {
            self.apply_mode(true);
        }
    }

    /// FUN_00402960: the frame drawn on the screen itself (`native`) or on
    /// the 640 x 480 surface; the warp camera set for the logical size:
    /// centre (W / 2, H / 2) (FUN_004a48f0), 76° across (FUN_004a4950 with
    /// the float π times the double 76 / 180, the constant `WARP_FOV`).
    /// The original sets the camera at every loop type change, at 640 x
    /// 480 too; the port does it only at 800 x 600 or when the size
    /// changes, so that the 640 x 480 game stays as it was (a possible
    /// difference after a dialog ending on another speaker's line with a
    /// `[Focale]` field of view, not checked yet).
    fn apply_mode(&mut self, native: bool) {
        self.display.native = native;
        let logical = self.display.logical_size();
        if !self.display.large() && self.display.logical == logical {
            return;
        }
        self.display.logical = logical;
        let (w, h) = (logical.0 as u32, logical.1 as u32);
        self.camera.cx = (w / 2) as f32;
        self.camera.cy = (h / 2) as f32;
        self.camera.set_fov(WARP_FOV, w, h);
    }

    /// `AtlDisplay_SetNewScreenSize` (FUN_00402de0) when the screen size
    /// asked for ([`Engine::screen_size`]) is not the screen's: the
    /// screen made again, the 640 x 480 surface at 800 x 600
    /// (FUN_00402720), then the loop type set again (its mode). Then the
    /// surface the frame is drawn on takes the logical size (the
    /// renderer's), which changes only between passes.
    pub(crate) fn sync_surface(&mut self) {
        let want = self.screen_size();
        if want != (self.display.width, self.display.height) {
            log::info!("screen {} x {}", want.0, want.1);
            self.display.width = want.0;
            self.display.height = want.1;
            if self.display.large() && self.display.small.is_empty() {
                self.display.small = vec![0; SMALL_W * SMALL_H];
            }
            self.set_loop_type(self.loop_type);
            if self.loop_type == LoopType::Dialog {
                if let Some(n) = self.dialog.as_ref().map(|d| d.number) {
                    self.dialog_mode(n);
                }
            }
        }
        let (w, h) = self.display.logical;
        if (self.renderer.width(), self.renderer.height()) != (w, h) {
            self.renderer.resize(w, h);
        }
    }

    /// Whether this frame is drawn on the 640 x 480 surface and enlarged.
    fn on_small_surface(&self) -> bool {
        self.display.large() && self.renderer.width() == SMALL_W
    }

    /// One pass of the frame dispatcher (@ 0x0049ce20), drawing the frame
    /// into `out` ([`Engine::screen_size`], RGB565) when given. Without
    /// `out` everything but the pixels is done. At 800 x 600 the loop
    /// types other than the panorama draw on the 640 x 480 surface, which
    /// `AtlDisplay_AutoBlit` (FUN_00402c30, FUN_00402bd0) then enlarges
    /// onto the screen if the frame is still not native (a menu closed in
    /// the pass leaves the screen as it was). Then the 15 Hz task when it
    /// is due in this pass ([`Engine::set_clock`]).
    pub fn pass(&mut self, out: Option<&mut [u16]>) {
        self.frame_pass(out);
        self.run_due_tick();
    }

    fn frame_pass(&mut self, out: Option<&mut [u16]>) {
        self.sync_surface();
        let (w, h) = (self.display.width, self.display.height);
        let out = match out {
            Some(o) if o.len() < w * h => {
                log::warn!(
                    "a frame of {} pixels for a {w} x {h} screen: not drawn",
                    o.len()
                );
                None
            }
            o => o,
        };
        let Some(out) = out else {
            self.pass_surface(None);
            return;
        };
        if self.on_small_surface() {
            let mut small = std::mem::take(&mut self.display.small);
            self.pass_surface(Some(&mut small));
            if !self.display.native {
                blit_640_to_800(&small, out, w);
            }
            self.display.small = small;
            return;
        }
        self.pass_surface(Some(&mut out[..w * h]));
        if self.display.large() && !self.display.native && self.video.is_some() {
            // A video started in a panorama frame (AtlHnm_start shows its
            // first frame at once, on the 640 x 480 surface now).
            self.sync_surface();
            let mut small = std::mem::take(&mut self.display.small);
            self.draw_video(&mut small);
            blit_640_to_800(&small, out, w);
            self.display.small = small;
        }
    }

    /// Draws the current screen into `out` ([`Engine::screen_size`])
    /// without running a pass (see [`Engine::render_surface`]), enlarged
    /// from the 640 x 480 surface when the frame is drawn there.
    pub fn render(&mut self, out: &mut [u16]) {
        self.sync_surface();
        let (w, h) = (self.display.width, self.display.height);
        if out.len() < w * h {
            return;
        }
        if self.on_small_surface() {
            let mut small = std::mem::take(&mut self.display.small);
            self.render_surface(&mut small);
            blit_640_to_800(&small, out, w);
            self.display.small = small;
        } else {
            self.render_surface(&mut out[..w * h]);
        }
    }
}

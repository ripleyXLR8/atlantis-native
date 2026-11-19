//! A game's picture on Android at the game's own size: the window's
//! buffers get the picture's proportions with the screen's black bars
//! added (`ANativeWindow_setBuffersGeometry`), and the system's composer
//! scales them to the screen on the graphics chip. softbuffer's own path
//! makes a buffer at the screen's size every frame (2340 x 1080 on a
//! phone), the game scales into it pixel by pixel and softbuffer then
//! copies it byte by byte into the window: about 13 ms a frame for
//! Atlantis III's 800 x 600, most of the frame. Here only the picture's
//! own pixels are written, once.

use ndk::hardware_buffer_format::HardwareBufferFormat;
use ndk::native_window::NativeWindow;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

pub(crate) struct Frame {
    window: NativeWindow,
    /// The buffers' size last given (width, height).
    geometry: (i32, i32),
}

/// A pixel as the window's R8G8B8X8 buffer keeps it (the bytes R, G, B,
/// unused): from 0x00RRGGBB.
#[inline]
pub(crate) fn from_xrgb(p: u32) -> u32 {
    ((p >> 16) & 0xff) | (p & 0xff00) | ((p & 0xff) << 16)
}

/// Every RGB565 value as the window's pixel (the colours of
/// [`atlantis2_engine::png::rgb565_to_xrgb`]).
pub(crate) fn rgb565_lut() -> &'static [u32] {
    static LUT: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        atlantis2_engine::png::rgb565_to_xrgb()
            .iter()
            .map(|&p| from_xrgb(p))
            .collect()
    })
}

impl Frame {
    /// The window's `ANativeWindow`, or None (not an Android window).
    pub(crate) fn new(window: &Window) -> Option<Frame> {
        let RawWindowHandle::AndroidNdk(a) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        let ptr = a.a_native_window.cast();
        // SAFETY: the handle is the window's live ANativeWindow; the
        // clone holds its own reference (released on drop).
        let window = unsafe { NativeWindow::clone_from_ptr(ptr) };
        Some(Frame {
            window,
            geometry: (0, 0),
        })
    }

    /// Shows a `gw` x `gh` picture in the middle of a window of
    /// `ww` x `wh` pixels, at the largest size keeping its proportions:
    /// `row(y, out)` writes the picture's row `y` into `out` (`gw`
    /// pixels, made with [`from_xrgb`]). Returns the picture's rectangle
    /// in the window's pixels (x, y, width, height), or None when the
    /// window took no frame.
    pub(crate) fn present(
        &mut self,
        (ww, wh): (usize, usize),
        (gw, gh): (usize, usize),
        mut row: impl FnMut(usize, &mut [u32]),
    ) -> Option<(usize, usize, usize, usize)> {
        let (ww, wh, gw, gh) = (ww.max(1), wh.max(1), gw.max(1), gh.max(1));
        // The buffers: the picture's own pixels on the side where it
        // fills the window, the bars' share added on the other.
        let wider = ww * gh > wh * gw;
        let (bw, bh) = if wider {
            ((ww * gh + wh / 2) / wh, gh)
        } else {
            (gw, (wh * gw + ww / 2) / ww)
        };
        let (bw, bh) = (bw.max(gw), bh.max(gh));
        let geometry = (bw as i32, bh as i32);
        if geometry != self.geometry {
            self.window
                .set_buffers_geometry(
                    geometry.0,
                    geometry.1,
                    Some(HardwareBufferFormat::R8G8B8X8_UNORM),
                )
                .ok()?;
            self.geometry = geometry;
        }
        let mut lock = self.window.lock(None).ok()?;
        let (lw, lh, stride) = (lock.width(), lock.height(), lock.stride());
        if lw < gw || lh < gh || stride < lw {
            return None;
        }
        // SAFETY: the locked buffer holds `lh` rows of `stride` 32-bit
        // pixels (R8G8B8X8), ours until the guard is dropped (posted).
        let pixels =
            unsafe { std::slice::from_raw_parts_mut(lock.bits().cast::<u32>(), stride * lh) };
        let (ox, oy) = ((lw - gw) / 2, (lh - gh) / 2);
        for (y, line) in pixels.chunks_exact_mut(stride).enumerate() {
            let line = &mut line[..lw];
            if y < oy || y >= oy + gh {
                line.fill(0);
                continue;
            }
            line[..ox].fill(0);
            row(y - oy, &mut line[ox..ox + gw]);
            line[ox + gw..].fill(0);
        }
        drop(lock);
        // The same rectangle on the screen, for the touches.
        let (dw, dh) = if wider {
            (wh * gw / gh, wh)
        } else {
            (ww, ww * gh / gw)
        };
        let (dw, dh) = (dw.max(1), dh.max(1));
        Some(((ww - dw.min(ww)) / 2, (wh - dh.min(wh)) / 2, dw, dh))
    }
}

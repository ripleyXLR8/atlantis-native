//! The warp's markers (`TeWarpMarker3`, atlantis4-gog-en-2363910).
//!
//! - Each XML marker (objects, animations, exits) becomes a marker at its
//!   3-D point (`0x004f4f00`); the point's `y` is negated in format 1.0
//!   files ([`atlantis4_data::scene`]).
//! - `SetMarker(name, image, n)` (`0x004f4fc0`): `image = −1` hides it,
//!   else the sprite is `2D/Menus/InGame/Marker_<image>.png` (and its
//!   `_over` twin); an exit's marker starts hidden (`0x004f5340`). A
//!   sprite shows when its marker is set and the global marker flag
//!   (`App+0x86b0`, `0x004f4ba0`) is on.
//! - **The strip** (read for this port, `STATIC`): `SetMarker` calls
//!   `0x00503890(8.0)` (the float @ `0x005bee10`), which sets the sprite to
//!   8 frames of `w × h/8` (`+0xb0 = 8`, first 0, last 7, the height times
//!   0.125 @ `0x005038d3`), stores **8.0 as its frame rate** (`+0xc0`),
//!   no loop limit (`+0xc4 = 0`) and the reverse flag (`+0xcc = 1`); then
//!   it starts the sprite's timer at once (@ `0x004f51c4..0x004f51e8`). Each
//!   draw, `0x00503a70`: `k = trunc(elapsed_µs · 8.0 / 10⁶)`, frame `k mod
//!   8` (with no loop limit the `k ≥ last` branch also takes the modulo);
//!   the quad's texture band is `count − frame − 1` (`0x005040d5`) from
//!   texture row 0, `v0 = band · h/8 / H`. Textures hold the pictures
//!   bottom row first (as the background atlases' rows, which the data
//!   shows), so
//!   frame `k` is the `k`-th 48-pixel band from the top of the file
//!   [INFERRED: PNGs uploaded as the JPEGs].
//! - Placement each frame (`0x00569b60`): `p = project(point)` (window
//!   pixels, y up); if `p.z > 1` (behind the camera) the sprite goes to
//!   `(−w, −h)`. Otherwise the sprite's centre in the 480 × 320 layer is
//!   `floor(p.x·480/W + (w − w·480/W)/2)`, `floor(p.y·320/H + (h −
//!   h·320/H)/2)` and its corner that centre less `(w·480/W, h·320/H)/2`,
//!   the layer stretched to the window, so that the sprite keeps its own
//!   pixel size `[CONFIRMED-DYN]` (atlantis4-re experiment 2026-11-12-01,
//!   item 4: GL quads of 3 markers, 6 coordinates exact at 1365 × 768).
//!   The sprite sits about `(w − w·480/W)/2` units (≈ 44 px at 1365)
//!   right of and `(h − h·320/H)/2` (≈ 34 px at 768) above the point,
//!   on whole layer units.
//!
//! Not settled: when the `_over` picture replaces the sprite (not shown
//! here), and the draw order of two overlapping markers (the original
//! draws each as two one-triangle draws interleaved with the next
//! marker's; here each is one quad).

use crate::camera::Camera;

/// The frames of a marker sprite (`+0xb0 = 8` in `0x00503890`).
pub const MARKER_FRAMES: u32 = 8;

/// The marker strip's frame rate: `0x00503890`'s argument, 8.0
/// (`0x005bee10`), stored at `+0xc0` and read by `0x00503a70`.
pub const MARKER_FPS: f32 = 8.0;

/// A marker's sprite as `SetMarker` sets it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerSprite {
    /// `Marker_<image>.png`.
    pub image: u32,
    /// When `SetMarker` started its timer (engine clock, ms).
    pub start_ms: f64,
}

impl MarkerSprite {
    /// The frame shown at `now_ms` (`0x00503a70`): `trunc(µs · 8 / 10⁶)
    /// mod 8`, the timer in whole microseconds.
    pub fn frame(&self, now_ms: f64) -> u32 {
        let us = ((now_ms - self.start_ms) * 1000.0).floor().max(0.0);
        let k = (us * f64::from(MARKER_FPS) / 1_000_000.0).trunc() as i64 as u32;
        k % MARKER_FRAMES
    }

    /// The top-down 48-pixel band of the strip shown for `frame`: the
    /// texture band `MARKER_FRAMES − frame − 1` counted from the picture's
    /// bottom row (see the module notes).
    pub fn band(frame: u32) -> u32 {
        let texture_band = MARKER_FRAMES - (frame % MARKER_FRAMES) - 1;
        MARKER_FRAMES - 1 - texture_band
    }
}

/// The marker sprite of `image`.
pub fn sprite_path(image: u32, over: bool) -> String {
    if over {
        format!("2D/Menus/InGame/Marker_{image}_over.png")
    } else {
        format!("2D/Menus/InGame/Marker_{image}.png")
    }
}

/// A marker of the warp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    /// The point, `y` already negated for format-1.0 files.
    pub point: [f32; 3],
    /// The sprite's picture number; `None` hidden (`SetMarker(…, −1)`).
    pub image: Option<u32>,
}

impl Marker {
    /// A marker as the XML creates it: an exit's gets `Marker_0`, hidden.
    pub fn new(point: [f32; 3]) -> Marker {
        Marker { point, image: None }
    }
}

/// The 480 × 320 layer of the markers (`0x00569b60`).
pub const LAYER: (f32, f32) = (480.0, 320.0);

/// `0x00569b60`: the centre, in 480 × 320 layer units (y up), of a `w` ×
/// `h` sprite for a point projected at `p` (window pixels, y up) in a
/// `width` × `height` window (see the module notes).
pub fn layer_centre(p: [f32; 2], w: f32, h: f32, width: u32, height: u32) -> (f32, f32) {
    let sx = LAYER.0 / width as f32;
    let sy = LAYER.1 / height as f32;
    (
        (p[0] * sx + (w - w * sx) / 2.0).floor(),
        (p[1] * sy + (h - h * sy) / 2.0).floor(),
    )
}

/// `0x00569b60`: the bottom-left corner of a `w` × `h` sprite for the
/// marker at `point` in window pixels (y from the bottom): the layer
/// centre of [`layer_centre`] stretched to the window, less half the
/// sprite. `None` when the point is behind the camera (the original
/// moves the sprite to `(−w, −h)`).
pub fn place(cam: &Camera, point: [f32; 3], w: f32, h: f32) -> Option<(f64, f64)> {
    let p = cam.project(point);
    if p[2] > 1.0 {
        return None;
    }
    let (cx, cy) = layer_centre([p[0], p[1]], w, h, cam.width, cam.height);
    let (kx, ky) = (
        f64::from(cam.width) / f64::from(LAYER.0),
        f64::from(cam.height) / f64::from(LAYER.1),
    );
    Some((
        f64::from(cx) * kx - f64::from(w) / 2.0,
        f64::from(cy) * ky - f64::from(h) / 2.0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::FOV_HD;

    /// The original's GL quads (experiment 2026-11-12-01, item 4): the
    /// projected point and the quad's centre in 480 × 320 units.
    #[test]
    fn markers_are_placed_as_the_originals_quads() {
        for (p, centre) in [
            ([744.32, 159.14], (277.0, 80.0)),
            ([1931.66, 458.23], (694.0, 204.0)),
            ([162.87, 428.22], (72.0, 192.0)),
        ] {
            assert_eq!(layer_centre(p, 48.0, 48.0, 1365, 768), centre, "{p:?}");
        }
    }

    #[test]
    fn a_marker_ahead_is_placed_and_one_behind_is_hidden() {
        let cam = Camera::new(FOV_HD, 1365, 768, 0.0, 0.0);
        // Projected at (682, 384): centre floor(239.82 + 15.56) = 255,
        // floor(160 + 14) = 174 units; corner 255·1365/480 − 24,
        // 174·768/320 − 24 pixels.
        let (x, y) = place(&cam, [0.0, 0.0, -500.0], 48.0, 48.0).unwrap();
        assert!((x - (255.0 * 1365.0 / 480.0 - 24.0)).abs() < 1e-9, "{x}");
        assert!((y - (174.0 * 2.4 - 24.0)).abs() < 1e-9, "{y}");
        assert_eq!(place(&cam, [0.0, 0.0, 500.0], 48.0, 48.0), None);
        assert_eq!(sprite_path(0, true), "2D/Menus/InGame/Marker_0_over.png");
    }

    #[test]
    fn the_strip_turns_at_8_frames_a_second() {
        let m = MarkerSprite {
            image: 0,
            start_ms: 1000.0,
        };
        assert_eq!(m.frame(1000.0), 0);
        assert_eq!(m.frame(1124.999), 0);
        assert_eq!(m.frame(1125.0), 1);
        assert_eq!(m.frame(1875.0), 7);
        assert_eq!(m.frame(2000.0), 0);
        assert_eq!(m.frame(2125.0), 1);
        // Before the start (a clock that went back): the first frame.
        assert_eq!(m.frame(0.0), 0);
        assert_eq!(
            (0..8).map(MarkerSprite::band).collect::<Vec<_>>(),
            (0..8).collect::<Vec<_>>()
        );
    }
}

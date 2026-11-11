//! The warp's markers (`TeWarpMarker3`, atlantis4-gog-en-2363910).
//!
//! - Each XML marker (objects, animations, exits) becomes a marker at its
//!   3-D point (`0x004f4f00`); the point's `y` is negated in format 1.0
//!   files ([`atlantis4_data::scene`]).
//! - `SetMarker(name, image, n)` (`0x004f4fc0`): `image = −1` hides it,
//!   else the sprite is `2D/Menus/InGame/Marker_<image>.png` (and its
//!   `_over` twin), played as **8 frames** (`0x00503890(8.0)`; the files are
//!   48 × 384 strips); an exit's marker starts with `Marker_0` and hidden
//!   (`0x004f5340`). A sprite shows when its marker is set and the global
//!   marker flag (`App+0x86b0`, `0x004f4ba0`) is on.
//! - Placement each frame (`0x00569b60`): `p = project(point)`; if `p.z >
//!   1` (behind the camera) the sprite goes to `(−w, −h)`; else its corner
//!   is `(p.x − w/2, p.y − h/2)` (y from the bottom), so that the sprite,
//!   at its own pixel size (the 480 × 320 interface scale cancels out), is
//!   centred on the point.
//!
//! Not settled yet: the sprites' frame rate (`0x00503890`'s timer), the
//! interface's y origin (taken as bottom-up, like `p.y`) and the drawing
//! order after the warp (the markers are drawn with the interface).

use crate::camera::Camera;

/// The frames of a marker sprite (`0x00503890(8.0)`).
pub const MARKER_FRAMES: u32 = 8;

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

/// `0x00569b60`: the bottom-left corner of a `w` × `h` sprite centred on
/// the projection of `point` (pixels, y from the bottom), or `None` when
/// the point is behind the camera (the original moves the sprite to `(−w,
/// −h)`).
pub fn place(cam: &Camera, point: [f32; 3], w: f32, h: f32) -> Option<(f32, f32)> {
    let p = cam.project(point);
    if p[2] > 1.0 {
        return None;
    }
    Some((p[0] - w / 2.0, p[1] - h / 2.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::FOV_HD;

    #[test]
    fn a_marker_ahead_is_centred_and_one_behind_is_hidden() {
        let cam = Camera::new(FOV_HD, 1365, 768, 0.0, 0.0);
        assert_eq!(
            place(&cam, [0.0, 0.0, -500.0], 48.0, 48.0),
            Some((658.0, 360.0))
        );
        assert_eq!(place(&cam, [0.0, 0.0, 500.0], 48.0, 48.0), None);
        assert_eq!(sprite_path(0, true), "2D/Menus/InGame/Marker_0_over.png");
    }
}

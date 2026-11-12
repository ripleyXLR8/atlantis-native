//! The GUI layouts' geometry, as far as the full-screen movie needs it:
//! `TeLayout::updateSize` (atlantis4-gog-en-2363910 @ 0x004fc5b0) and
//! `updatePosition` (@ 0x004fc950) for `Percent` layouts, and where a
//! layout lands in the window.
//!
//! The GUI's root is the window itself, one unit a window pixel, origin
//! at the window's centre, y down (the root camera @ 0x00510760). A
//! layout's position is the offset of its centre from its parent's
//! centre.
//!
//! Every intermediate value the two functions store is a `float`; the
//! arithmetic between the stores is x87 at 53 bits, done here in `f64`
//! and rounded to `f32` at the same places.

/// `ratioMode`: how a `Percent` size keeps the layout's `ratio`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RatioMode {
    None,
    LetterBox,
    PanScan,
}

/// `safeAreaRatio`'s default (the `TeLayout` constructor @ 0x004fd3c0).
pub const SAFE_AREA_RATIO: f32 = 4.0 / 3.0;

#[inline]
fn f(x: f64) -> f32 {
    x as f32
}

#[inline]
fn d(x: f32) -> f64 {
    f64::from(x)
}

/// A `Percent` size `user` of a parent of size `parent`
/// (@ 0x004fc654..0x004fc771): `n = user × parent`, then, when both are
/// positive and `ratio` and `safe` are too, with `r = n.x / n.y`:
/// `LetterBox` makes `n.x = ratio × n.y` if `ratio < r`, else
/// `n.y = n.x / ratio`; `PanScan` makes `n.x = ratio × n.y` if
/// `safe <= r`, else `n.x = ratio × n.y × (1 − (safe − r) / safe)`.
pub fn percent_size(
    user: (f32, f32),
    parent: (f32, f32),
    ratio: f32,
    mode: RatioMode,
    safe: f32,
) -> (f32, f32) {
    let (mut nx, mut ny) = (f(d(user.0) * d(parent.0)), f(d(user.1) * d(parent.1)));
    if !(nx > 0.0 && ny > 0.0 && ratio > 0.0 && safe > 0.0) {
        return (nx, ny);
    }
    let r = f(d(nx) / d(ny));
    match mode {
        RatioMode::None => {}
        RatioMode::LetterBox => {
            if ratio < r {
                nx = f(d(ratio) * d(ny));
            } else {
                ny = f(d(nx) / d(ratio));
            }
        }
        RatioMode::PanScan => {
            if safe <= r {
                nx = f(d(ratio) * d(ny));
            } else {
                let t = f(1.0 - (d(safe) - d(r)) / d(safe));
                nx = f(d(ratio) * d(ny) * d(t));
            }
        }
    }
    (nx, ny)
}

/// A `Percent` position (@ 0x004fca1d..0x004fcaea): the offset of the
/// centre from the parent's centre, `(user − 0.5) × parent + (0.5 −
/// anchor) × size`, each product and each difference stored as a float.
pub fn percent_position(
    user: (f32, f32),
    anchor: (f32, f32),
    size: (f32, f32),
    parent: (f32, f32),
) -> (f32, f32) {
    let axis = |u: f32, a: f32, s: f32, p: f32| {
        let sa = f(d(f(0.5 - d(a))) * d(s));
        let pu = f(d(f(d(u) - 0.5)) * d(p));
        f(d(pu) + d(sa))
    };
    (
        axis(user.0, anchor.0, size.0, parent.0),
        axis(user.1, anchor.1, size.1, parent.1),
    )
}

/// The draw of a `Pixel`-sized sprite truncates its world translation
/// toward zero (`_ftol` @ 0x0054cde5); `Percent` sprites keep theirs.
pub fn pixel_snap(centre: (f32, f32)) -> (f32, f32) {
    (centre.0.trunc(), centre.1.trunc())
}

/// A layout's rectangle in window pixels (left, top, width, height; y
/// down) from its centre offset and size, in a `width × height` window.
pub fn window_rect(centre: (f32, f32), size: (f32, f32), width: u32, height: u32) -> Rect {
    let (cx, cy) = (
        f64::from(width) / 2.0 + d(centre.0),
        f64::from(height) / 2.0 + d(centre.1),
    );
    Rect {
        left: cx - d(size.0) / 2.0,
        top: cy - d(size.1) / 2.0,
        width: d(size.0),
        height: d(size.1),
    }
}

/// An axis-aligned rectangle in window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// Whether the window point (`x`, `y`) is inside (left and top edges
    /// included, right and bottom excluded).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.left && x < self.left + self.width && y >= self.top && y < self.top + self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_box_keeps_the_picture_inside() {
        // Cases run in the original's code (Unicorn): 4/3 in 1920 × 1080 → 1440 × 1080,
        // 16/9 in 1024 × 768 → 1024 × 576.
        let s = percent_size(
            (1.0, 1.0),
            (1920.0, 1080.0),
            f(4.0 / 3.0),
            RatioMode::LetterBox,
            SAFE_AREA_RATIO,
        );
        assert_eq!(s, (1440.0, 1080.0));
        let s = percent_size(
            (1.0, 1.0),
            (1024.0, 768.0),
            f(16.0 / 9.0),
            RatioMode::LetterBox,
            SAFE_AREA_RATIO,
        );
        assert_eq!(s, (1024.0, 576.0));
    }

    #[test]
    fn pan_scan_fills_the_height() {
        // The original's code (Unicorn): 1365/768 in 1024 × 768 → 1365 × 768; in 1280 × 1024 →
        // 1706.25 × 1024.
        let r = f(1365.0 / 768.0);
        let s = percent_size(
            (1.0, 1.0),
            (1024.0, 768.0),
            r,
            RatioMode::PanScan,
            SAFE_AREA_RATIO,
        );
        assert_eq!(s, (1365.0, 768.0));
        let s = percent_size(
            (1.0, 1.0),
            (1280.0, 1024.0),
            r,
            RatioMode::PanScan,
            SAFE_AREA_RATIO,
        );
        assert_eq!(s, (1706.25, 1024.0));
    }

    #[test]
    fn positions_put_the_anchor_on_the_point() {
        // The original's code (Unicorn): anchor (0.5, 1), position (0.5, 1.001), 1024 × 4 in
        // 1024 × 768 → (0, 382.768).
        let p = percent_position((0.5, 1.001), (0.5, 1.0), (1024.0, 4.0), (1024.0, 768.0));
        assert_eq!(p.0, 0.0);
        assert!((p.1 - 382.768).abs() < 1e-3);
        assert_eq!(pixel_snap((583.499_94, -338.82)), (583.0, -338.0));
    }
}

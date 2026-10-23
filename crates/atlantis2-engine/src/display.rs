//! The original's two screen sizes: 640 x 480 and 800 x 600, the options
//! page's resolution button (action 14, `FUN_00402de0`
//! `AtlDisplay_SetNewScreenSize`), kept in `atlant2.cfg` (its screen width
//! and height, @ 0x005e2f24 and 0x005e2f28). The original's own mode, not
//! an enhancement.
//!
//! At 800 x 600 the original has two drawing surfaces: the screen, 800 x
//! 600, and a 640 x 480 surface (@ 0x006166f8, made by `FUN_00402720`).
//! `FUN_00402960`, called by every loop type change (`AtlMain_SetLoopType`
//! `FUN_0049da90`), chooses which one the frame is drawn on, and the
//! "logical" size the code lays things out for (@ 0x005d9b9c,
//! 0x005d9ba0; the flag @ 0x005d9ba4):
//!
//! - **native**, for the panorama (loop type 0) and the `MESSAGE_CLIC`
//!   box (10): the screen itself, 800 x 600. The cube is drawn at 800 x
//!   600 with the same 76° across (fx = 800 / (2 tan 38°)), centre
//!   (400, 300); the cursor, the object in hand, the bag bar (10 objects,
//!   its art twice, `FUN_00475740`, `FUN_00474c30`), the dowsing rod and
//!   the overlays are laid out for 800 x 600 by their own formulas. A
//!   dialog is native only when its number is one of
//!   [`NATIVE_DIALOGS`] (`FUN_00480110`, from `AtlDial_start`
//!   `FUN_0047ff60`: Ireland's and Yucatan's off-screen voices over the
//!   panorama);
//! - **not native**, for every other loop type (videos, dialogs, menus,
//!   the credits, `ViewBigSpr`'s pictures): the 640 x 480 surface, drawn
//!   exactly as at 640 x 480 (camera centre (320, 240), 76° across 640),
//!   then copied to the screen by `AtlDisplay_AutoBlit` `FUN_00402c30`
//!   with the MMX routine `FUN_00401202` ([`blit_640_to_800`]): 4 pixels
//!   to 5 across and 4 rows to 5 down, so the videos, the menu pages and
//!   the heads are the 640 x 480 pictures enlarged 1.25 times.
//!
//! Each mode change sets the warp camera again (`FUN_004a48f0` centre,
//! `FUN_004a4990` viewport, `FUN_004a4950` 76° for the logical size,
//! `FUN_0047ba50` the sprites' clip). The dialogs' field of view
//! (`FUN_004801d0`, `FUN_004808a0`) is computed with the screen's size,
//! not the logical one: behind a head at 800 x 600 the panorama, drawn
//! on the 640 x 480 surface, is 1.25 times narrower than at 640 x 480.
//! At 800 wide the menus' dimmed backdrop is black (`FUN_00470430`).

/// The dialogs drawn natively at 800 x 600 (`FUN_00480110`: the table
/// built on its stack, 0x2c, 0x52..0x58, 0xbb..0xbd).
pub const NATIVE_DIALOGS: [u32; 11] = [44, 82, 83, 84, 85, 86, 87, 88, 187, 188, 189];

/// Whether dialog `n` is drawn on the screen itself at 800 x 600.
pub fn native_dialog(n: u32) -> bool {
    NATIVE_DIALOGS.contains(&n)
}

/// The 640 x 480 surface's size.
pub const SMALL_W: usize = 640;
pub const SMALL_H: usize = 480;
/// The 800 x 600 screen's.
pub const LARGE_W: usize = 800;
pub const LARGE_H: usize = 600;

/// `FUN_00401202` (RGB565; `FUN_00401000` is its RGB555 twin): the 640 x
/// 480 surface `src` onto the 800 x 600 screen `dst` (`pitch` pixels per
/// row). Per channel (blue `& 0x1f`, green `>> 5 & 0x3f`, red `>> 11`),
/// with the weights of the qwords @ 0x005d98d7 (1, 2, 3, 4) and
/// @ 0x005d98df (3, 2, 1, 0) and truncating shifts, 4 pixels a, b, c, d
/// become 5: a (the pixel itself), (a + 3b) >> 2, (2b + 2c) >> 2,
/// (3c + d) >> 2, d; then, on the widened rows, 4 rows to 5 the same way
/// (the first and the last copied, the 3 between mixed). Exact to the
/// original (`tests/golden/blit800.txt`, the routine run under
/// Unicorn).
pub fn blit_640_to_800(src: &[u16], dst: &mut [u16], pitch: usize) {
    assert!(src.len() >= SMALL_W * SMALL_H);
    assert!(pitch >= LARGE_W && dst.len() >= pitch * (LARGE_H - 1) + LARGE_W);
    let ch = |p: u16| (p & 0x1f, (p >> 5) & 0x3f, p >> 11);
    let pack = |(b, g, r): (u16, u16, u16)| b | (g << 5) | (r << 11);
    let mix = |p: u16, wp: u16, q: u16, wq: u16| {
        let (p, q) = (ch(p), ch(q));
        pack((
            (p.0 * wp + q.0 * wq) >> 2,
            (p.1 * wp + q.1 * wq) >> 2,
            (p.2 * wp + q.2 * wq) >> 2,
        ))
    };
    // The row buffer @ 0x005d70c7: 4 widened rows.
    let mut rows = [[0u16; LARGE_W]; 4];
    for group in 0..SMALL_H / 4 {
        for (r, row) in rows.iter_mut().enumerate() {
            let s = &src[(group * 4 + r) * SMALL_W..][..SMALL_W];
            for (i, q) in s.as_chunks::<4>().0.iter().enumerate() {
                let o = &mut row[i * 5..i * 5 + 5];
                o[0] = q[0];
                o[1] = mix(q[0], 1, q[1], 3);
                o[2] = mix(q[1], 2, q[2], 2);
                o[3] = mix(q[2], 3, q[3], 1);
                o[4] = mix(q[3], 4, 0, 0);
            }
        }
        let base = group * 5 * pitch;
        for x in 0..LARGE_W {
            let [r0, r1, r2, r3] = [rows[0][x], rows[1][x], rows[2][x], rows[3][x]];
            dst[base + x] = r0;
            dst[base + pitch + x] = mix(r0, 1, r1, 3);
            dst[base + 2 * pitch + x] = mix(r1, 2, r2, 2);
            dst[base + 3 * pitch + x] = mix(r2, 3, r3, 1);
            dst[base + 4 * pitch + x] = r3;
        }
    }
}

/// The screen's size from `atlant2.cfg`'s width: 800 x 600 for an
/// 800-wide screen (the original's tests are on the width alone,
/// `DAT_005e2f24 == 800`), else 640 x 480: the two sizes the options page
/// offers (the original would try any other size the file holds).
pub fn screen_for_cfg(width: i32) -> (usize, usize) {
    if width == LARGE_W as i32 {
        (LARGE_W, LARGE_H)
    } else {
        (SMALL_W, SMALL_H)
    }
}

/// The engine's screen: its size and, at 800 x 600, the mode the current
/// loop type draws in and the 640 x 480 surface.
#[derive(Debug)]
pub(crate) struct Display {
    /// The screen (@ 0x005e2f24, 0x005e2f28).
    pub width: usize,
    pub height: usize,
    /// The flag @ 0x005d9ba4: the frame drawn on the screen itself.
    pub native: bool,
    /// The logical size the camera was last set for (@ 0x005d9b9c,
    /// 0x005d9ba0).
    pub logical: (usize, usize),
    /// The 640 x 480 surface (@ 0x006166f8), kept between frames as the
    /// original's.
    pub small: Vec<u16>,
}

impl Display {
    pub fn new(width: usize, height: usize) -> Display {
        Display {
            width,
            height,
            native: true,
            logical: (width, height),
            small: Vec::new(),
        }
    }

    /// Whether the screen is the 800 x 600 one.
    pub fn large(&self) -> bool {
        (self.width, self.height) == (LARGE_W, LARGE_H)
    }

    /// The logical size for the current mode.
    pub fn logical_size(&self) -> (usize, usize) {
        if self.large() && !self.native {
            (SMALL_W, SMALL_H)
        } else {
            (self.width, self.height)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_native_dialogs() {
        assert!(native_dialog(44) && native_dialog(85) && native_dialog(189));
        assert!(!native_dialog(43) && !native_dialog(89) && !native_dialog(190));
        assert!(!native_dialog(0));
    }

    #[test]
    fn four_pixels_become_five() {
        // One group of 4 x 4: (a, b, c, d) per row, rows equal.
        let mut src = vec![0u16; SMALL_W * SMALL_H];
        let row = [0x0000, 0xffff, 0xf800, 0x001f];
        for y in 0..4 {
            src[y * SMALL_W..y * SMALL_W + 4].copy_from_slice(&row);
        }
        let mut dst = vec![0u16; LARGE_W * LARGE_H];
        blit_640_to_800(&src, &mut dst, LARGE_W);
        // b = white mixed 3:1 with black: 31·3 >> 2 = 23, 63·3 >> 2 = 47.
        let w3 = 23 | (47 << 5) | (23 << 11);
        let w2r2 = 31 >> 1 | (63 >> 1) << 5 | 31 << 11;
        assert_eq!(&dst[..5], &[0, w3, w2r2, ((31 * 3) >> 2) << 11 | 7, 0x001f]);
        // Rows 1..4 are mixes of equal rows: the same.
        for y in 1..5 {
            assert_eq!(dst[y * LARGE_W..y * LARGE_W + 5], dst[..5]);
        }
        assert!(dst[5 * LARGE_W..].iter().all(|&p| p == 0));
    }

    #[test]
    fn cfg_sizes() {
        assert_eq!(screen_for_cfg(800), (800, 600));
        assert_eq!(screen_for_cfg(640), (640, 480));
        assert_eq!(screen_for_cfg(1024), (640, 480));
        assert!(Display::new(800, 600).large() && !Display::new(640, 480).large());
        let mut d = Display::new(800, 600);
        d.native = false;
        assert_eq!(d.logical_size(), (640, 480));
    }
}

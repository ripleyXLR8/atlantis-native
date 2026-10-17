//! The panoramic ("warp") view: a cylindrical 2048-pixel panorama projected
//! onto the 640x480 screen.
//!
//! Translated from ScummVM's CryOmni3D engine (`omni3d.cpp`,
//! `atlantis/omni3d.cpp`; GPLv3, the ScummVM team), which reproduces
//! `atlantis.exe`:
//!
//! - `FUN_0045179a(hfov, 50)`: the projection tables ([`Warp::set_hfov`]);
//! - `FUN_00451915(alpha, beta)`: the 31x41 grid of panorama coordinates,
//!   one point per 16x16 screen tile corner ([`Warp::set_alpha`],
//!   [`Warp::set_beta`]);
//! - `FUN_00451d8b(src, dst)`: the tile renderer ([`Warp::render`]);
//! - `FUN_0045166f`: screen point to panorama point
//!   ([`Warp::screen_to_panorama`]).
//!
//! # Floating point
//!
//! The game runs the x87 at 53-bit precision (control word 0x127f, set at
//! 0x004719a5) and stores most intermediates as floats. The code below
//! computes in `f64` and rounds to `f32` exactly where the original stores a
//! float, so the grid matches the original's to the bit. Rust never fuses
//! multiply-adds nor reorders floating-point operations, so the order of the
//! expressions below is significant and must be kept. The transcendental
//! functions come from the pure-Rust `libm` crate so that every target
//! (Windows, macOS, wasm) computes the same values.
//!
//! # Panorama layout
//!
//! The renderer addresses the source as `(row << 11) | column` with a
//! 10-bit row (mask 0x1ff800): 2048 pixels per row, up to 1024 rows. The
//! vertical origin of the grid is row 384, the centre of the game's
//! 2048x768 panoramas; a taller source (such as the 2048x1024 test
//! panorama) works too. Pixels outside the source slice read as 0.

/// Screen width of the warp view.
pub const SCREEN_W: usize = 640;
/// Screen height of the warp view.
pub const SCREEN_H: usize = 480;
/// Width of a panorama, fixed by the renderer's addressing.
pub const PANO_W: usize = 2048;
/// Rows the renderer can address (10-bit row index).
pub const PANO_MAX_H: usize = 1024;

/// The warp field of view in degrees: atlantis.exe pushes the float
/// 0x42964625 to `FUN_0045179a` at 0x0044a701, 0x0043044c, 0x00428abb,
/// 0x00428b48 and 0x00428cbd.
pub const WARP_FOV_DEG: f32 = 75.137;

/// Dialog camera FOVs in degrees for cams 2..5: the float table at
/// 0x00496080, read as `[cam * 4 + 0x496078]` at 0x00428a0e / 0x00428ad4.
pub const DIALOG_CAM_FOV_DEG: [f32; 4] = [20.68, 30.0, 48.0, 48.26];

/// The vertical field of view every caller passes to `FUN_0045179a`.
const VFOV_DEG: f32 = 50.0;

/// Grid points per row (40 tiles + 1) and rows (30 tiles + 1).
const GRID_COLS: usize = 41;
const GRID_ROWS: usize = 31;
/// Points of a row computed directly; the rest are mirrored.
const HALF_COLS: usize = 21;
/// Ints per grid row (x, y per point).
const ROW_STRIDE: usize = GRID_COLS * 2;
/// The grid starts at index 2, as in the original table.
const COORDS_LEN: usize = 2 + GRID_ROWS * ROW_STRIDE;

/// 2048 * 65536: the panorama width in 16.16 fixed point.
const PAN_FULL: f64 = 134_217_728.0;
/// 384 * 65536: the vertical origin (row 384) in 16.16 fixed point.
const ROW_ORIGIN: f64 = 25_165_824.0;

/// Shifts of the per-row change of the tile step in `FUN_00451d8b`.
const ROW_STEP_SHIFT_X: u32 = 4;
const ROW_STEP_SHIFT_Y: u32 = 9;

/// pi as `FUN_0045179a` computes it: `acos(sqrt(2) / 2) * 4` written as
/// `(pi/2 - atan2(q, sqrt(1 - q*q))) * 4` (0x004517ab..0x004517c1). It is
/// kept in a register for the 155 deg term and stored as a float
/// (0x00bcb594) for everything else.
fn exe_pi_f64() -> f64 {
    let q = 2.0f64.sqrt() * 0.5;
    (core::f64::consts::FRAC_PI_2 - libm::atan2(q, (1.0 - q * q).sqrt())) * 4.0
}

/// `FUN_00451915`'s angle rule: beta clamped to `+-beta_max` (0.9 x the
/// vertical limit), alpha brought into `[0, 2 pi)` by one float 2 pi. The
/// comparisons of beta are made in double, as the original's.
pub fn wrap_angles(alpha: &mut f32, beta: &mut f32, two_pi: f32, beta_max: f64) {
    if f64::from(*beta) > beta_max {
        *beta = beta_max as f32;
    } else if f64::from(*beta) < -beta_max {
        *beta = (-beta_max) as f32;
    }
    // "Not below 2 pi": a NaN takes this branch too.
    if (*alpha).partial_cmp(&two_pi) != Some(core::cmp::Ordering::Less) {
        *alpha = (f64::from(*alpha) - f64::from(two_pi)) as f32;
    } else if *alpha < 0.0 {
        *alpha = (f64::from(two_pi) + f64::from(*alpha)) as f32;
    }
}

/// The panoramic projection of the warp view.
#[derive(Clone)]
pub struct Warp {
    hfov_deg: f32,
    alpha: f32,
    beta: f32,

    /// pi and 2 pi as floats (0x00bcb594).
    pi: f32,
    two_pi: f32,
    /// The vertical limit (0x00bcab58).
    vfov: f32,
    /// Fixed-point scale of the panorama angle: 2048 * 65536 / 2 pi.
    pan_scale: f32,
    /// Per-row angle (0x00bca9fc) and hypotenuse (0x00bcaacc).
    row_angle: [f32; GRID_ROWS],
    row_hyp: [f32; GRID_ROWS],
    /// Per-column opposite side (0x00bcaa78).
    col_opp: [f32; HALF_COLS],
    /// Vertical scale per row and column (0x00bcab68).
    row_col_scale: [[f32; HALF_COLS]; GRID_ROWS],

    /// The grid in 16.16 panorama coordinates, `x, y` per point, rows of
    /// 41 points starting at index 2.
    coords: [i32; COORDS_LEN],
    coords_dirty: bool,
}

impl Default for Warp {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for Warp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Warp")
            .field("hfov_deg", &self.hfov_deg)
            .field("alpha", &self.alpha)
            .field("beta", &self.beta)
            .finish_non_exhaustive()
    }
}

impl Warp {
    /// A view at the warp field of view ([`WARP_FOV_DEG`]), looking at
    /// alpha = beta = 0.
    pub fn new() -> Self {
        let mut w = Warp {
            hfov_deg: 0.0,
            alpha: 0.0,
            beta: 0.0,
            pi: 0.0,
            two_pi: 0.0,
            vfov: 0.0,
            pan_scale: 0.0,
            row_angle: [0.0; GRID_ROWS],
            row_hyp: [0.0; GRID_ROWS],
            col_opp: [0.0; HALF_COLS],
            row_col_scale: [[0.0; HALF_COLS]; GRID_ROWS],
            coords: [0; COORDS_LEN],
            coords_dirty: true,
        };
        w.set_hfov(WARP_FOV_DEG);
        w
    }

    /// pi as the original stores it (0x00bcb594, a float).
    pub fn exe_pi() -> f32 {
        exe_pi_f64() as f32
    }

    /// Sets the horizontal field of view, in degrees: `FUN_0045179a(hfov,
    /// 50)` (every caller passes 50 deg as the vertical one). The view
    /// angles are kept.
    pub fn set_hfov(&mut self, hfov_deg: f32) {
        self.hfov_deg = hfov_deg;

        let pi = exe_pi_f64();
        self.pi = pi as f32;
        self.two_pi = (pi * 2.0) as f32;
        let pi_f = f64::from(self.pi);
        let h = (pi_f * f64::from(hfov_deg) / 180.0) as f32;
        self.vfov = (pi_f * f64::from(VFOV_DEG) / 180.0) as f32;
        // The 155 deg vertical warp angle uses the register pi.
        let b5c = (384.0 / libm::sin(pi * 155.0 / 180.0 * 0.5)) as f32;
        let step = (libm::tan(f64::from(h) * 0.5) * 16.0 / 320.0) as f32;
        self.pan_scale = (PAN_FULL / f64::from(self.two_pi)) as f32;

        for i in 0..GRID_ROWS {
            let t = (i as f64 - 15.0) * f64::from(step);
            self.row_angle[i] = libm::atan2(t, 1.0) as f32;
            let tf = f64::from(t as f32);
            let c = (tf * tf + 1.0).sqrt() as f32;
            self.row_hyp[i] = c;
            for j in 0..HALF_COLS {
                let u = ((j as f64 - 20.0) * f64::from(step)) as f32;
                self.col_opp[j] = u;
                let den = (f64::from(u) * f64::from(u) + f64::from(c) * f64::from(c)).sqrt();
                self.row_col_scale[i][j] = (f64::from(c) * f64::from(b5c) * 65536.0 / den) as f32;
            }
        }

        self.coords_dirty = true;
    }

    /// The horizontal field of view last set, in degrees.
    pub fn hfov(&self) -> f32 {
        self.hfov_deg
    }

    /// Sets the yaw (radians). It is wrapped into `[0, 2 pi)` when the grid
    /// is next computed.
    pub fn set_alpha(&mut self, alpha: f32) {
        self.alpha = alpha;
        self.coords_dirty = true;
    }

    /// Sets the pitch (radians, positive up). It is clamped to
    /// `+-`[`beta_limit`](Self::beta_limit) when the grid is next computed.
    pub fn set_beta(&mut self, beta: f32) {
        self.beta = beta;
        self.coords_dirty = true;
    }

    /// The yaw: after a render or mapping, the wrapped value the original
    /// keeps in its view-angle global (0x00bcb59c).
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// The pitch: after a render or mapping, the clamped value the original
    /// keeps in its view-angle global (0x00bcb598).
    pub fn beta(&self) -> f32 {
        self.beta
    }

    /// pi as stored by `FUN_0045179a` (0x00bcb594).
    pub fn pi(&self) -> f32 {
        self.pi
    }

    /// 2 pi as stored by `FUN_0045179a`.
    pub fn two_pi(&self) -> f32 {
        self.two_pi
    }

    /// `FUN_00451915`'s pitch limit: 0.9 (0x004befe0) x the vertical limit
    /// 0x00bcab58.
    pub fn beta_limit(&self) -> f64 {
        f64::from(self.vfov) * 0.9
    }

    /// `FUN_00451915(alpha, beta)`: the grid of panorama coordinates. The
    /// wrapped alpha and clamped beta become the view angles.
    fn update_coords(&mut self) {
        if !self.coords_dirty {
            return;
        }

        let (mut a, mut b) = (self.alpha, self.beta);
        wrap_angles(&mut a, &mut b, self.two_pi, self.beta_limit());
        self.alpha = a;
        self.beta = b;

        let pan_scale = f64::from(self.pan_scale);
        let base = f64::from((PAN_FULL - f64::from(a) * pan_scale) as f32);

        for i in 0..GRID_ROWS {
            let v = f64::from(b) + f64::from(self.row_angle[i]);
            let s = f64::from(libm::sin(v) as f32);
            let r = f64::from((f64::from(self.row_hyp[i]) * libm::cos(v)) as f32);
            let row = &mut self.coords[2 + ROW_STRIDE * i..2 + ROW_STRIDE * (i + 1)];
            let scale = &self.row_col_scale[i];

            for j in 0..HALF_COLS - 1 {
                let d = f64::from((libm::atan2(f64::from(self.col_opp[j]), r) * pan_scale) as f32);
                let y = (ROW_ORIGIN - s * f64::from(scale[j])) as i32;
                row[2 * j] = (base + d) as i32;
                row[80 - 2 * j] = (base - d) as i32;
                row[2 * j + 1] = y;
                row[81 - 2 * j] = y;
            }
            let centre = libm::atan2(f64::from(self.col_opp[20]), r);
            row[40] = (PAN_FULL - (f64::from(a) - centre) * pan_scale) as i32;
            row[41] = (ROW_ORIGIN - s * f64::from(scale[20])) as i32;
        }

        self.coords_dirty = false;
    }

    /// `FUN_00451d8b(src, dst)`: renders the view into `out` (640x480,
    /// row-major) from the panorama `pano` (2048 pixels per row, see the
    /// module documentation).
    ///
    /// Each 16x16 screen tile is filled by stepping linearly through the
    /// panorama between the grid points at its corners, in 16.16 fixed
    /// point with wrapping 32-bit arithmetic (the horizontal coordinate
    /// wraps around the panorama through the overflow).
    ///
    /// # Panics
    ///
    /// If `out` is smaller than 640x480.
    pub fn render(&mut self, pano: &[u16], out: &mut [u16]) {
        assert!(
            out.len() >= SCREEN_W * SCREEN_H,
            "output smaller than 640x480"
        );
        self.update_coords();
        let c = &self.coords;

        let mut off = 2usize;
        for tile_y in 0..SCREEN_H / 16 {
            for tile_x in 0..SCREEN_W / 16 {
                let mut x1 = c[off + 2].wrapping_sub(c[off]) >> 4;
                let mut y1 = c[off + 3].wrapping_sub(c[off + 1]) >> 4;
                let x1n = c[off + 84].wrapping_sub(c[off + 82]) >> 4;
                let y1n = c[off + 85].wrapping_sub(c[off + 83]) >> 4;

                let dx1 = x1n.wrapping_sub(x1) >> ROW_STEP_SHIFT_X;
                let dy1 = y1n.wrapping_sub(y1) >> ROW_STEP_SHIFT_Y;

                y1 >>= 5;

                let dx2 = c[off + 82].wrapping_sub(c[off]) >> 4;
                let dy2 = c[off + 83].wrapping_sub(c[off + 1]) >> 9;
                let mut x2 = c[off].wrapping_mul(2).wrapping_add(dx2) >> 1;
                let mut y2 = (c[off + 1] >> 5).wrapping_mul(2).wrapping_add(dy2) >> 1;

                for y in 0..16 {
                    let mut px = (x2.wrapping_mul(2).wrapping_add(x1) as u32).wrapping_mul(16);
                    // Signed division (rounds toward zero), as the original.
                    let mut py = (y2.wrapping_mul(2).wrapping_add(y1) / 2) as u32;
                    let delta_x = (x1 as u32).wrapping_mul(32);
                    let delta_y = y1 as u32;

                    let start = (tile_y * 16 + y) * SCREEN_W + tile_x * 16;
                    for dst in &mut out[start..start + 16] {
                        let src = ((py & 0x1f_f800) | (px >> 21)) as usize;
                        *dst = pano.get(src).copied().unwrap_or(0);
                        px = px.wrapping_add(delta_x);
                        py = py.wrapping_add(delta_y);
                    }

                    x1 = x1.wrapping_add(dx1);
                    y1 = y1.wrapping_add(dy1);
                    x2 = x2.wrapping_add(dx2);
                    y2 = y2.wrapping_add(dy2);
                }
                off += 2;
            }
            off += 2;
        }
    }

    /// The generalised `FUN_00451d8b` (a port addition, for the HD mode):
    /// each 16x16 tile of the grid becomes a `tile` x `tile` block of
    /// output pixels, so the output is `40 * tile` x `30 * tile`
    /// ([`hd_size`]). The steps through the panorama are the original's
    /// with the tile size as divisor (floor division, which is the
    /// original's arithmetic shift at `tile` = 16): at `tile` = 16 the
    /// sample points are those of [`Warp::render`], bit for bit.
    ///
    /// `sample` gets the 32-bit panorama coordinates of each output
    /// pixel's centre: the column in the top 11 bits (21 fraction bits,
    /// wrapping around the panorama), the row times 2048 in `py` (11
    /// fraction bits, the row masked by 0x1ff800 as the original reads
    /// it). Large outputs are rendered by several threads where the
    /// platform has them.
    ///
    /// # Panics
    ///
    /// If `tile` is 0 or `out` is smaller than [`hd_size`]`(tile)`.
    pub fn render_tiles<T, F>(&mut self, tile: usize, out: &mut [T], sample: F)
    where
        T: Send,
        F: Fn(u32, u32) -> T + Sync,
    {
        assert!(tile > 0, "tile size 0");
        let (w, h) = hd_size(tile);
        assert!(out.len() >= w * h, "output smaller than {w}x{h}");
        self.update_coords();
        let c = &self.coords;
        let band = w * tile; // one row of tiles
        let out = &mut out[..w * h];
        let threads = render_threads(w * h);
        if threads <= 1 {
            for (tile_y, rows) in out.chunks_mut(band).enumerate() {
                tile_row(c, tile, tile_y, rows, &sample);
            }
            return;
        }
        let per = (SCREEN_H / 16).div_ceil(threads);
        std::thread::scope(|s| {
            for (k, chunk) in out.chunks_mut(band * per).enumerate() {
                let sample = &sample;
                s.spawn(move || {
                    for (i, rows) in chunk.chunks_mut(band).enumerate() {
                        tile_row(c, tile, k * per + i, rows, sample);
                    }
                });
            }
        });
    }

    /// [`Warp::render_tiles`] with the original's nearest sampling into
    /// RGB565 (pixels outside the source read as 0): at `tile` = 16 the
    /// same pixels as [`Warp::render`].
    pub fn render_scaled(&mut self, pano: &[u16], tile: usize, out: &mut [u16]) {
        self.render_tiles(tile, out, |px, py| {
            let src = ((py & 0x1f_f800) | (px >> 21)) as usize;
            pano.get(src).copied().unwrap_or(0)
        });
    }

    /// [`Warp::render_tiles`] with bilinear sampling of a panorama in
    /// XRGB8888 (`pano`, 2048 pixels per row): the HD view. The rows
    /// wrap as the original's addressing does; past the source's last row
    /// a pixel reads as 0, as in [`Warp::render`].
    pub fn render_bilinear(&mut self, pano: &[u32], tile: usize, out: &mut [u32]) {
        let rows = (pano.len() / PANO_W) as u32;
        self.render_tiles(tile, out, |px, py| {
            // The sample point is the pixel's centre; a texel's centre is
            // half a texel into it.
            let x = px.wrapping_sub(1 << 20);
            let y = py.wrapping_sub(1 << 10);
            let c0 = x >> 21;
            let c1 = (c0 + 1) & (PANO_W as u32 - 1);
            let fx = (x >> 13) & 0xff;
            let r0 = (y >> 11) & 0x3ff;
            let fy = (y >> 3) & 0xff;
            if r0 >= rows {
                return 0;
            }
            let r1 = if r0 + 1 < rows { r0 + 1 } else { r0 };
            let (o0, o1) = ((r0 as usize) * PANO_W, (r1 as usize) * PANO_W);
            let top = lerp888(pano[o0 + c0 as usize], pano[o0 + c1 as usize], fx);
            let bottom = lerp888(pano[o1 + c0 as usize], pano[o1 + c1 as usize], fx);
            lerp888(top, bottom, fy)
        });
    }

    /// The grid's panorama point under a screen point, bilinearly
    /// interpolated in the 16x16 tile: `(column, v)` with `v` counted from
    /// the grid's own vertical origin (ScummVM's `mapMouseCoords`).
    ///
    /// # Panics
    ///
    /// If the point is outside the 640x480 screen.
    pub fn map_screen_point(&mut self, x: i32, y: i32) -> (i32, i32) {
        assert!(
            (0..SCREEN_W as i32).contains(&x) && (0..SCREEN_H as i32).contains(&y),
            "screen point ({x}, {y}) outside 640x480"
        );
        self.update_coords();
        let c = &self.coords;

        let (small_x, square_x) = (x & 0xf, (x >> 4) as usize);
        let (small_y, square_y) = (y & 0xf, (y >> 4) as usize);
        let off = ROW_STRIDE * square_y + 2 * square_x;

        let interp = |k: usize| {
            c[off + k]
                .wrapping_add(small_y.wrapping_mul(c[off + 82 + k].wrapping_sub(c[off + k]) >> 4))
                .wrapping_add(
                    (small_x * small_y)
                        .wrapping_mul(c[off + 84 + k].wrapping_sub(c[off + 82 + k]) >> 8),
                )
                .wrapping_add(
                    (small_x * (16 - small_y))
                        .wrapping_mul(c[off + 2 + k].wrapping_sub(c[off + k]) >> 8),
                )
        };

        let px = (interp(2) & 0x07ff_0000) >> 16;
        let py = interp(3) >> 16;
        (px, py)
    }

    /// `FUN_0045166f`: the panorama point under a screen point, as the
    /// original hit-tests it against zones and NPCs. It returns
    /// `y = 0x2ff - v` (row 0 at the top of a 768-row panorama).
    pub fn screen_to_panorama(&mut self, x: i32, y: i32) -> (i32, i32) {
        let (px, v) = self.map_screen_point(x, y);
        (px, 0x2ff - v)
    }
}

/// The output size of [`Warp::render_tiles`] for a tile size: 40 x 30
/// tiles (640x480 at 16).
pub fn hd_size(tile: usize) -> (usize, usize) {
    (SCREEN_W / 16 * tile, SCREEN_H / 16 * tile)
}

/// The threads to render `pixels` output pixels with: one below 250000,
/// one per core up to 8 above, one on the web.
fn render_threads(pixels: usize) -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = pixels;
        1
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if pixels < 250_000 {
            return 1;
        }
        std::thread::available_parallelism().map_or(1, |n| n.get().min(8))
    }
}

/// `a` to `b` at `w` / 256, per channel of XRGB8888.
#[inline]
fn lerp888(a: u32, b: u32, w: u32) -> u32 {
    let iw = 256 - w;
    let rb = (((a & 0xff_00ff) * iw + (b & 0xff_00ff) * w) >> 8) & 0xff_00ff;
    let g = (((a & 0xff00) * iw + (b & 0xff00) * w) >> 8) & 0xff00;
    rb | g
}

/// One row of tiles of [`Warp::render_tiles`]: `FUN_00451d8b`'s tile loop
/// with `tile` for the original's 16 (the shifts by 4 and 9 are the
/// divisions by 16 and 16 * 32).
fn tile_row<T>(
    c: &[i32; COORDS_LEN],
    tile: usize,
    tile_y: usize,
    rows: &mut [T],
    sample: &impl Fn(u32, u32) -> T,
) {
    let n = tile as i32;
    let d = |v: i32, k: i32| v.div_euclid(k);
    let w = SCREEN_W / 16 * tile;
    let mut off = 2 + tile_y * ROW_STRIDE;
    for tile_x in 0..SCREEN_W / 16 {
        let mut x1 = d(c[off + 2].wrapping_sub(c[off]), n);
        let mut y1 = d(c[off + 3].wrapping_sub(c[off + 1]), n);
        let x1n = d(c[off + 84].wrapping_sub(c[off + 82]), n);
        let y1n = d(c[off + 85].wrapping_sub(c[off + 83]), n);

        let dx1 = d(x1n.wrapping_sub(x1), n);
        let dy1 = d(y1n.wrapping_sub(y1), n * 32);

        y1 >>= 5;

        let dx2 = d(c[off + 82].wrapping_sub(c[off]), n);
        let dy2 = d(c[off + 83].wrapping_sub(c[off + 1]), n * 32);
        let mut x2 = c[off].wrapping_mul(2).wrapping_add(dx2) >> 1;
        let mut y2 = (c[off + 1] >> 5).wrapping_mul(2).wrapping_add(dy2) >> 1;

        for y in 0..tile {
            let mut px = (x2.wrapping_mul(2).wrapping_add(x1) as u32).wrapping_mul(16);
            let mut py = (y2.wrapping_mul(2).wrapping_add(y1) / 2) as u32;
            let delta_x = (x1 as u32).wrapping_mul(32);
            let delta_y = y1 as u32;

            let start = y * w + tile_x * tile;
            for dst in &mut rows[start..start + tile] {
                *dst = sample(px, py);
                px = px.wrapping_add(delta_x);
                py = py.wrapping_add(delta_y);
            }

            x1 = x1.wrapping_add(dx1);
            y1 = y1.wrapping_add(dy1);
            x2 = x2.wrapping_add(dx2);
            y2 = y2.wrapping_add(dy2);
        }
        off += 2;
    }
}

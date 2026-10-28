//! The HD mode (a port addition, the first game's: `atlantis_engine::hd`):
//! the panorama drawn at the window's resolution with the original's
//! projection, everything else as the original's 640x480 art.
//!
//! The original draws the cube with its polygon rasteriser at 640 x 480
//! (`FUN_0045c030`, `warpPII`, see [`crate::warp`]): a
//! camera point (x, y, z) goes to the screen point sx = fx·x/z + cx,
//! sy = fy·y/z + cy, with fx = fy = W / (2 tan 38°) (76° across 640
//! pixels) and (cx, cy) = (320, 240), and the texture coordinates of the
//! face it lies on are interpolated perspective-correctly between the
//! corners' (1/512 and 511/512: texel 1 + 510·t of the 512 across the
//! face, `FUN_0045c790`). Its pixel i covers [i, i + 1) and is sampled at
//! its centre i + 0.5 (the set-up adds 0.5 to the vertices, @ 0x0057b3c7).
//!
//! The HD view evaluates the same mapping at more pixels: the pixel
//! (X, Y) of a picture `s` times larger (s = tile / 16, the first game's
//! grid of 40 x 30 tiles) is the screen point ((X + 0.5) / s,
//! (Y + 0.5) / s); its ray, (sx − cx) / fx, (sy − cy) / fy, 1 in the
//! camera's space, is turned to the world by the camera's matrix (the
//! game's floats, [`crate::camera::Camera::matrix`]) as `FUN_0045adf0`
//! turns the screen centre's ([`crate::camera::Camera::ray`]), and the
//! cube face it hits gives the texel coordinates the original's
//! interpolation reaches at that point ([`cube_point`]). The original
//! takes the texel there (nearest, its 16-bit steps truncating); the HD
//! view samples the face bilinearly, or bicubically and sharpened as the
//! first game's ([`HdOptions`]), so that a texel spread over several
//! pixels is not a block. Widescreen widens the field of view: the pixels
//! of the side columns are screen points past 0 and 640, on the same
//! formulas, the vertical field of view, the angles and their limits
//! unchanged.
//!
//! The place animations are drawn into the faces before the cube
//! (`FUN_0045b120`), so the view shows them at the HD resolution too. What
//! the original draws over the cube (the cursor and the object in hand at
//! the centre, the bag bar, the subject menu, the dialogs' videos and
//! subtitles) and the frames without a cube (the videos, the menus' TGA
//! pages) are its 640x480 pixels, upscaled by the front end
//! (`atlantis_engine::hd::HdCompositor`): the layer is shown where the
//! 640x480 frame still shows the cube it was drawn with
//! (`HdLayer::base`; on the spider's web, the cube and the creatures
//! drawn again over the HD view).

pub use atlantis_engine::hd::{
    rgb565_to_888, tile_for_output, HdLayer, HdOptions, HdText, PanoFilter, MAX_SIDE_TILES,
    MAX_TILE,
};
pub use atlantis_engine::upscale::PanoChanges;
use atlantis_engine::upscale::{same_pixels, PanoramaPrep};

use crate::subtitle::{SubtitleFont, TextOut};

use crate::warp::FACE_SIZE;

/// The original's screen.
pub const SCREEN_W: usize = 640;
pub const SCREEN_H: usize = 480;

/// The face whose plane a direction meets first, and the texel
/// coordinates of that point: the face 0 `ft`, 1 `bk`, 2 `up`, 3 `dn`,
/// 4 `lf`, 5 `rt`, and u, v in texels of the face (1 + 510·t, t = (s + 1)
/// / 2 of the point's coordinates s ∈ [−1, 1] across it, as the corners'
/// texture coordinates 1/512 and 511/512 give, `FUN_0045c790`). The
/// original samples the texel (⌊u⌋, ⌊v⌋). `None` for a direction that is
/// not one (zero, not finite).
pub fn cube_point(dir: [f64; 3]) -> Option<(usize, f64, f64)> {
    let [x, y, z] = dir;
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    let (face, u, v) = if az >= ax && az >= ay {
        if z > 0.0 {
            (0, x / az, y / az)
        } else {
            (1, -x / az, y / az)
        }
    } else if ay >= ax {
        if y < 0.0 {
            (2, x / ay, z / ay)
        } else {
            (3, x / ay, -z / ay)
        }
    } else if x < 0.0 {
        (4, z / ax, y / ax)
    } else {
        (5, -z / ax, y / ax)
    };
    if !(u.is_finite() && v.is_finite()) {
        return None;
    }
    let texel = |s: f64| 1.0 + 510.0 * (s + 1.0) / 2.0;
    Some((face, texel(u), texel(v)))
}

/// The original's projection for a picture `tile` / 16 times the 640x480
/// screen, with `side` pixel columns of widescreen on each side: the
/// camera's matrix and its fx, fy, cx, cy (the game's floats).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HdProjection {
    m: [f64; 9],
    fx: f64,
    fy: f64,
    cx: f64,
    cy: f64,
    scale: f64,
    side: f64,
}

impl HdProjection {
    /// `matrix` and `proj` (fx, fy, cx, cy) as the cube renderer takes
    /// them ([`crate::warp::Renderer::render`]).
    pub fn new(matrix: &[f32; 9], proj: [f32; 4], tile: usize, side: usize) -> HdProjection {
        let [fx, fy, cx, cy] = proj.map(f64::from);
        HdProjection {
            m: matrix.map(f64::from),
            fx,
            fy,
            cx,
            cy,
            scale: tile as f64 / 16.0,
            side: side as f64,
        }
    }

    /// The 640x480 screen point at the centre of the picture's pixel
    /// (`x`, `y`) (sides included: x from 0 at the left side's edge).
    pub fn screen_point(&self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.side + 0.5) / self.scale, (y + 0.5) / self.scale)
    }

    /// The camera-space ray of a screen point, at depth 1: the inverse of
    /// sx = fx·x/z + cx, sy = fy·y/z + cy.
    pub fn camera_ray(&self, sx: f64, sy: f64) -> (f64, f64) {
        ((sx - self.cx) / self.fx, (sy - self.cy) / self.fy)
    }

    /// The world direction of the camera-space ray (vx, vy, 1), turned by
    /// the matrix as `FUN_0045adf0` turns the centre's.
    pub fn world(&self, vx: f64, vy: f64) -> [f64; 3] {
        world(&self.m, vx, vy)
    }

    /// The world direction through a screen point.
    pub fn direction(&self, sx: f64, sy: f64) -> [f64; 3] {
        let (vx, vy) = self.camera_ray(sx, sy);
        self.world(vx, vy)
    }
}

/// The world direction of (vx, vy, 1) in the camera's space: each
/// component (vz·m[a + 2] + vy·m[a + 1]) + vx·m[a], the order of
/// `FUN_0045adf0`'s turn.
#[inline]
fn world(m: &[f64; 9], vx: f64, vy: f64) -> [f64; 3] {
    let row = |a: usize| (m[a + 2] + vy * m[a + 1]) + vx * m[a];
    [row(0), row(3), row(6)]
}

/// The threads to draw `pixels` pixels with: one below 250000, one per
/// core up to 8 above, one on the web (the first game's rule).
fn threads(pixels: usize) -> usize {
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

/// Draws a `width` x `height` view through `proj` into `out`: each
/// pixel is `sample(face, u, v)` of the cube point of its ray
/// ([`cube_point`]), 0 where there is none. The rows are shared out among
/// the processor's cores.
pub fn render_view<F>(proj: &HdProjection, width: usize, height: usize, out: &mut [u32], sample: F)
where
    F: Fn(usize, f64, f64) -> u32 + Sync,
{
    assert!(
        out.len() >= width * height,
        "output smaller than {width}x{height}"
    );
    if width == 0 || height == 0 {
        return;
    }
    // The camera-space x of each column, y of each row.
    let vx: Vec<f64> = (0..width)
        .map(|x| proj.camera_ray(proj.screen_point(x as f64, 0.0).0, 0.0).0)
        .collect();
    // The pixels x0..x1 of row y.
    let span = |y: usize, x0: usize, line: &mut [u32]| {
        let vy = proj.camera_ray(0.0, proj.screen_point(0.0, y as f64).1).1;
        // world()'s sums, the row's part first (the same operations).
        let m = &proj.m;
        let a = [m[2] + vy * m[1], m[5] + vy * m[4], m[8] + vy * m[7]];
        for (d, &vx) in line.iter_mut().zip(&vx[x0..]) {
            let dir = [a[0] + vx * m[0], a[1] + vx * m[3], a[2] + vx * m[6]];
            *d = match cube_point(dir) {
                Some((face, u, v)) => sample(face, u, v),
                None => 0,
            };
        }
    };
    // Rows `first..` of `band`, in columns of BLOCK pixels: a block's
    // pixels read a compact part of the faces (fewer cache and page
    // misses than whole rows across a face seen at a slant).
    const BLOCK: usize = 64;
    let rows = |first: usize, band: &mut [u32]| {
        for x0 in (0..width).step_by(BLOCK) {
            let x1 = (x0 + BLOCK).min(width);
            for (i, line) in band.chunks_mut(width).enumerate() {
                span(first + i, x0, &mut line[x0..x1]);
            }
        }
    };
    let out = &mut out[..width * height];
    let n = threads(width * height);
    // Bands of 32 rows, shared out among the threads.
    let per = 32.min(height.div_ceil(n));
    if n <= 1 {
        for (k, band) in out.chunks_mut(per * width).enumerate() {
            rows(k * per, band);
        }
        return;
    }
    let bands: Vec<(usize, &mut [u32])> = out.chunks_mut(per * width).enumerate().collect();
    let next = std::sync::Mutex::new(bands.into_iter());
    std::thread::scope(|s| {
        for _ in 0..n {
            let (rows, next) = (&rows, &next);
            s.spawn(move || loop {
                let job = next.lock().map(|mut it| it.next()).unwrap_or(None);
                let Some((k, band)) = job else { break };
                rows(k * per, band);
            });
        }
    });
}

/// The faces' border in the atlas: each face's edge texels repeated, so
/// that the filters' taps past a face's edge (the sharpening's 1, the
/// doubling's 2) read its own edge, never the next face.
pub const PAD: usize = 4;

/// A face with its border, in the atlas: the six one under the other.
pub const CELL: usize = FACE_SIZE + 2 * PAD;

/// What a renderer of the HD view outside the engine needs (the
/// browser's graphics card: one core draws the view too slowly there):
/// the view of [`HdPanorama::render`] through the projection of
/// `matrix`, `proj`, `tile` and `side` ([`HdProjection::new`]), `width`
/// x `height` pixels, from the prepared atlas `pano` (`pano_size`, the
/// faces one under the other, [`CELL`] pixels each with their [`PAD`],
/// twice that when `doubled`), of which the tiles `changes` (squares of
/// `pano_tile` pixels) changed since the last job.
pub struct CubeJob<'a> {
    pub pano: &'a [u32],
    pub pano_size: (usize, usize),
    pub pano_tile: usize,
    pub changes: PanoChanges,
    pub doubled: bool,
    pub matrix: [f32; 9],
    pub proj: [f32; 4],
    pub tile: usize,
    pub side: usize,
    pub width: usize,
    pub height: usize,
    /// The drawing's number, which [`HdLayer::card_gen`] names.
    pub gen: u64,
}

/// A renderer of the HD view outside the engine ([`crate::Engine::cube_renderer`]).
pub trait CubeRenderer {
    /// Draws the view of `job`, kept for the front end's picture
    /// ([`HdLayer::on_card`]), and into `out` as well when given (read
    /// back); false when it cannot (the engine then draws it, and the next
    /// job brings the whole atlas: the renderer forgets its copy).
    fn render(&mut self, job: CubeJob<'_>, out: Option<&mut [u32]>) -> bool;
}

/// The panorama the HD view samples: the six faces as drawn (their
/// sprites in), in XRGB8888, sharpened and doubled as the options say
/// (`atlantis_engine::upscale::PanoramaPrep`, the first game's, which
/// computes again only the tiles a sprite's frame changed).
#[derive(Default)]
pub struct HdPanorama {
    atlas: Vec<u16>,
    prep: PanoramaPrep,
    double: bool,
}

impl HdPanorama {
    pub fn new() -> HdPanorama {
        HdPanorama::default()
    }

    /// Brings the prepared faces up to date with `faces` (cube order,
    /// 512 x 512 RGB565 each) and the options; the prepared tiles
    /// computed again.
    pub fn update(&mut self, faces: &[Vec<u16>; 6], opts: &HdOptions) -> usize {
        let fresh = self.atlas.len() != CELL * 6 * CELL;
        self.atlas.resize(CELL * 6 * CELL, 0);
        for (f, face) in faces.iter().enumerate() {
            assert_eq!(face.len(), FACE_SIZE * FACE_SIZE);
            for r in 0..CELL {
                let sy = r.clamp(PAD, PAD + FACE_SIZE - 1) - PAD;
                let src = &face[sy * FACE_SIZE..][..FACE_SIZE];
                let dst = &mut self.atlas[(f * CELL + r) * CELL..][..CELL];
                // (only the rows that changed: the faces change where a
                // sprite moves)
                if !fresh && same_pixels(&dst[PAD..PAD + FACE_SIZE], src) {
                    continue;
                }
                dst[..PAD].fill(src[0]);
                dst[PAD..PAD + FACE_SIZE].copy_from_slice(src);
                dst[PAD + FACE_SIZE..].fill(src[FACE_SIZE - 1]);
            }
        }
        self.double = opts.panorama == PanoFilter::Bicubic;
        self.prep
            .update(&self.atlas, CELL, opts.sharpen, self.double)
    }

    /// Keeps the tiles computed again by [`HdPanorama::update`] for an
    /// outside renderer ([`HdPanorama::take_changes`]).
    pub fn track_changes(&mut self, on: bool) {
        self.prep.track = on;
    }

    /// The prepared atlas (XRGB8888), its size, its tiles' side, whether
    /// it is doubled.
    pub fn atlas(&self) -> (&[u32], (usize, usize), usize, bool) {
        (
            self.prep.pixels(),
            self.prep.size(),
            self.prep.tile_size(),
            self.double,
        )
    }

    /// The atlas's tiles computed again since the last call.
    pub fn take_changes(&mut self) -> PanoChanges {
        self.prep.take_changes()
    }

    /// The view through `proj`, `width` x `height` pixels, into `out`
    /// (XRGB8888): each pixel the prepared faces sampled bilinearly at its
    /// cube point (texel centres at half a texel, [`cube_point`]).
    pub fn render(&self, proj: &HdProjection, width: usize, height: usize, out: &mut [u32]) {
        let px = self.prep.pixels();
        let k = if self.double { 2.0 } else { 1.0 };
        let pw = if self.double { 2 * CELL } else { CELL };
        if px.len() < pw * pw * 6 {
            out[..width * height].fill(0);
            return;
        }
        // In 1/64 of a prepared pixel, from the first one's centre: the
        // texel coordinate c is k·(PAD + c) − 1/2 prepared pixels.
        let scale = 64.0 * k;
        let x0 = 64.0 * (k * PAD as f64 - 0.5);
        let y0: [f64; 6] = std::array::from_fn(|f| 64.0 * (k * (f * CELL + PAD) as f64 - 0.5));
        render_view(proj, width, height, out, |face, u, v| {
            let ax = (u * scale + x0) as usize;
            let ay = (v * scale + y0[face]) as usize;
            let o0 = (ay >> 6) * pw + (ax >> 6);
            let o1 = o0 + pw;
            let (wx, wy) = ((ax & 63) as u64, (ay & 63) as u64);
            let acc = spread(px[o0]) * ((64 - wx) * (64 - wy))
                + spread(px[o0 + 1]) * (wx * (64 - wy))
                + spread(px[o1]) * ((64 - wx) * wy)
                + spread(px[o1 + 1]) * (wx * wy);
            ((acc >> 12) & 0xff | (acc >> 25) & 0xff00 | (acc >> 38) & 0xff_0000) as u32
        });
    }
}

/// The channels of an XRGB8888 pixel 21 bits apart (blue, green, red),
/// so that one multiplication weighs the three: a weight up to 4096
/// keeps each within its 21 bits.
#[inline]
fn spread(p: u32) -> u64 {
    let p = u64::from(p);
    (p & 0xff) | (p & 0xff00) << 13 | (p & 0xff_0000) << 26
}

/// Where the 640x480 picture is in a `width` x `height` window: its left
/// and top offsets, width and height (the largest 4:3 picture that fits,
/// centred; the first game's front end's rule).
pub fn picture_rect(width: usize, height: usize) -> (usize, usize, usize, usize) {
    let scale = (width as f64 / SCREEN_W as f64).min(height as f64 / SCREEN_H as f64);
    let (dw, dh) = (
        (SCREEN_W as f64 * scale) as usize,
        (SCREEN_H as f64 * scale) as usize,
    );
    ((width - dw) / 2, (height - dh) / 2, dw, dh)
}

/// The 640x480 screen pixel a window pixel shows, given the picture's
/// rectangle ([`picture_rect`]): the pixel the front end's composition
/// draws there (`x · 640 / dw`, the nearest map of
/// `atlantis_engine::upscale::Axis`), outside the picture past its edges
/// (negative, or 640 and more). A click there is a click on that pixel.
pub fn window_to_screen(x: i64, y: i64, rect: (usize, usize, usize, usize)) -> (i32, i32) {
    let (ox, oy, dw, dh) = rect;
    let map = |p: i64, o: usize, d: usize, n: usize| -> i32 {
        if d == 0 {
            return 0;
        }
        (p - o as i64).wrapping_mul(n as i64).div_euclid(d as i64) as i32
    };
    (map(x, ox, dw, SCREEN_W), map(y, oy, dh, SCREEN_H))
}

/// The widescreen tile columns on each side of the picture in a window
/// with `ox` columns of bars beside a picture `dw` wide (a tile is dw / 40
/// pixels): enough to fill them, at most [`MAX_SIDE_TILES`] (the first
/// game's rule).
pub fn side_tiles(ox: usize, dw: usize) -> usize {
    if dw == 0 {
        return 0;
    }
    (ox * (SCREEN_W / 16)).div_ceil(dw).min(MAX_SIDE_TILES)
}

/// The font a text was drawn with: `AtlFT_Dialog` (the subtitles) or
/// `AtlFT_POPUP` (the menus' tooltips and lists, the bag's names, the
/// credits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFont {
    Dialog,
    Popup,
    /// `AtlFT_Big_Default` (the `MESSAGE_CLIC` viewer).
    Big,
}

/// A `TextOutA` call of a frame, for the HD mode to draw again at the
/// output's resolution: its font, the call (position, colour, bytes, on
/// the 640x480 screen) and its weight (255, or the credits' fade).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HdTextRun {
    pub font: TextFont,
    pub call: TextOut,
    pub alpha: u8,
}

/// The texts of a frame ("Sharp subtitles", the first game's
/// `hd_sharp_subtitles`): the calls drawn, and the first game's
/// [`HdText`] holding the 640x480 pixels under the glyphs and the glyphs'
/// own (`under`, `after`; its `items` stay empty: the Atlantis II font is
/// drawn by [`crate::subtitle::SubtitleFont::draw_hd`]), so that the
/// compositor shows the picture under the 640x480 glyphs where the frame
/// still shows them and the sharp glyphs are drawn over it.
///
/// Recording is on only in the HD mode ([`HdTexts::begin`]); off, a draw
/// is the font's own [`SubtitleFont::draw`] and nothing is kept. On, the
/// 640x480 pixels written are the same.
#[derive(Clone, Debug, Default)]
pub struct HdTexts {
    pub runs: Vec<HdTextRun>,
    pub text: HdText,
    /// The pixels a glyph set (their `under` is kept from the first one).
    set: Vec<u8>,
    on: bool,
}

impl HdTexts {
    /// A frame starts: nothing recorded; recording when `on`.
    pub fn begin(&mut self, on: bool) {
        self.runs.clear();
        self.on = on;
        let n = SCREEN_W * SCREEN_H;
        if on {
            for b in [&mut self.text.under, &mut self.text.after] {
                b.clear();
                b.resize(n, 0);
            }
            self.set.clear();
            self.set.resize(n, 0);
        } else {
            self.text.under.clear();
            self.text.after.clear();
        }
    }

    /// Whether texts are recorded.
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// `calls` drawn with `font` (`id`) into `out` (`w` x `h` RGB565) as
    /// [`SubtitleFont::draw`] draws them, and recorded when on (a 640 x
    /// 480 screen only).
    pub fn draw(
        &mut self,
        id: TextFont,
        font: &SubtitleFont,
        out: &mut [u16],
        w: usize,
        h: usize,
        calls: &[TextOut],
    ) {
        if !self.on || w != SCREEN_W || h != SCREEN_H {
            font.draw(out, w, h, calls);
            return;
        }
        let (under, after, set) = (&mut self.text.under, &mut self.text.after, &mut self.set);
        font.draw_with(calls, |px, py, colour| {
            if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                let i = py as usize * w + px as usize;
                if set[i] == 0 {
                    set[i] = 1;
                    under[i] = out[i];
                }
                let c = crate::subtitle::rgb565(colour);
                out[i] = c;
                after[i] = c;
            }
        });
        self.push(id, calls, 255);
    }

    /// `calls` recorded with weight `alpha` without drawing them (their
    /// pixels are set with [`HdTexts::set_pixels`]).
    pub fn push(&mut self, id: TextFont, calls: &[TextOut], alpha: u8) {
        if self.on {
            self.runs.extend(calls.iter().map(|c| HdTextRun {
                font: id,
                call: c.clone(),
                alpha,
            }));
        }
    }

    /// The 640x480 picture without the texts (`under`) and with them
    /// (`after`), for texts drawn elsewhere (the credits' pages).
    pub fn set_pixels(&mut self, under: &[u16], after: &[u16]) {
        if self.on {
            self.text.under.copy_from_slice(under);
            self.text.after.copy_from_slice(after);
        }
    }
}

/// A dialog frame's pictures for the HD mode (a port addition, the first
/// game's HD heads): captured while the dialog frame draws its 640x480
/// picture (@ 0x00461430), then composed into the HD layer by the first
/// game's `atlantis_engine::hd::compose_dialog`.
#[derive(Clone, Debug, Default)]
pub struct DialogHd {
    /// The HD tile size (the head is drawn `tile` / 16 times larger, into
    /// 40·tile x 30·tile pixels).
    pub tile: usize,
    /// The picture under the body video (the panorama), after the body,
    /// after the head (640x480).
    pub pano: Vec<u16>,
    pub pre_head: Vec<u16>,
    pub composite: Vec<u16>,
    /// The pixels the body video drew (its `AL` mask not 0).
    pub body: Vec<u8>,
    /// The head at the HD size, its Z buffer, and where it was drawn.
    pub head: Vec<u16>,
    pub zbuf: Vec<u32>,
    pub mask: Vec<u8>,
    /// A head was drawn; the frame was captured.
    pub drawn: bool,
    pub ready: bool,
}

impl DialogHd {
    /// The HD head's size: 40·tile x 30·tile.
    pub fn head_size(&self) -> (usize, usize) {
        (40 * self.tile, 30 * self.tile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_and_texels() {
        // The centre of each face: texel 256.
        let c = |d| cube_point(d).unwrap();
        assert_eq!(c([0.0, 0.0, 1.0]), (0, 256.0, 256.0));
        assert_eq!(c([0.0, 0.0, -1.0]), (1, 256.0, 256.0));
        assert_eq!(c([0.0, -1.0, 0.0]), (2, 256.0, 256.0));
        assert_eq!(c([0.0, 1.0, 0.0]), (3, 256.0, 256.0));
        assert_eq!(c([-1.0, 0.0, 0.0]), (4, 256.0, 256.0));
        assert_eq!(c([1.0, 0.0, 0.0]), (5, 256.0, 256.0));
        // A face's corners: texels 1 and 511.
        assert_eq!(c([-1.0, -1.0, 1.0]), (0, 1.0, 1.0));
        assert_eq!(c([1.0, 1.0, 1.0]), (0, 511.0, 511.0));
        assert_eq!(cube_point([0.0; 3]), None);
    }

    #[test]
    fn one_multiplication_weighs_the_three_channels() {
        let pack = |acc: u64| (acc >> 12) & 0xff | (acc >> 25) & 0xff00 | (acc >> 38) & 0xff_0000;
        for p in [0u32, 0x00ff_ffff, 0x0012_3456, 0x00ff_00ff] {
            assert_eq!(pack(spread(p) * 4096) as u32, p);
        }
        // Halfway between black and white, and a quarter of the way.
        let (a, b) = (spread(0), spread(0x00ff_ffff));
        assert_eq!(pack(a * 2048 + b * 2048), 0x007f_7f7f);
        assert_eq!(pack(a * 3072 + b * 1024), 0x003f_3f3f);
    }

    #[test]
    fn the_picture_and_the_clicks() {
        // 1920x1080: a 1440x1080 picture with 240 columns each side.
        let r = picture_rect(1920, 1080);
        assert_eq!(r, (240, 0, 1440, 1080));
        assert_eq!(side_tiles(240, 1440), 7);
        // Every window pixel is the screen pixel the nearest map shows.
        let axis = atlantis_engine::upscale::Axis::new(SCREEN_W, r.2);
        for x in 0..r.2 {
            let (sx, _) = window_to_screen((r.0 + x) as i64, 0, r);
            assert_eq!(sx as usize, axis.nearest[x]);
        }
        assert_eq!(window_to_screen(239, 0, r).0, -1);
        assert_eq!(window_to_screen(240 + 1440, 1079, r), (640, 479));
    }
}

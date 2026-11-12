//! What a frame is drawn on: the drawing primitives of the original's GL
//! (`TeWarp3::draw`, `TeMesh::draw`, the GUI's sprites and texts), each
//! described by what it reads, so that two renderers can draw the same
//! frame:
//!
//! - [`CpuCanvas`], the processor's renderer: the per-pixel model of GL's
//!   `GL_LINEAR` and blending in [`crate::render`], [`crate::sprite`],
//!   [`crate::gui`] and [`crate::movie`], the exact reference (the frame
//!   MD5s of `tests/frames.rs`);
//! - a graphics card's renderer (`atlantis4-gl`), which gives the same
//!   primitives to OpenGL / WebGL 2 with the original's GL state
//!   (`TeOpenGLTexture`'s parameters, the one `glBlendFunc`, the
//!   material modes).
//!
//! The engine's `draw_on` functions call these in the original's draw
//! order; nothing in them depends on which canvas draws.

use atlantis4_data::anim::TileRange;

use crate::camera::Camera;
use crate::gui::text::{Block, Clip, Font};
use crate::layout::Rect;
use crate::movie::Mode;
use crate::package::Picture;
use crate::render::{draw_mesh, render, Coverage, Weights};

/// What a texture holds beyond a picture's edges (what `GL_LINEAR`'s
/// taps reach there).
#[derive(Clone, Copy, Debug)]
pub enum Edge<'a> {
    /// `GL_CLAMP_TO_EDGE` on the picture itself.
    Clamp,
    /// The GUI's and the sprites' rule ([`crate::sprite`]): the picture
    /// bottom row first in a power-of-two texture; clamped left and
    /// below, transparent right and above.
    Padded,
    /// A video texture's margin ([`crate::movie::VideoSprite`]): clamped
    /// left and above; right of the picture `col[y]`, below it `row[x]`,
    /// the corner `col`'s last.
    Margin {
        col: &'a [[u8; 4]],
        row: &'a [[u8; 4]],
    },
}

impl Edge<'_> {
    /// The texel at (`x`, `y`) of the texture holding `pic` (texel
    /// centres at integers, rows from the top).
    #[inline]
    pub fn tap(&self, pic: &Picture, x: i64, y: i64) -> [u8; 4] {
        match self {
            Edge::Clamp => pic.texel_clamped(x, y),
            Edge::Padded => {
                let (w, h) = (i64::from(pic.width), i64::from(pic.height));
                if x >= w || y < 0 {
                    return [0; 4];
                }
                pic.texel(x.max(0), y.min(h - 1))
            }
            Edge::Margin { col, row } => {
                let (w, h) = (i64::from(pic.width), i64::from(pic.height));
                let (x, y) = (x.max(0), y.max(0));
                if x >= w && y >= h {
                    col[col.len() - 1]
                } else if x >= w {
                    col[y as usize]
                } else if y >= h {
                    row[x as usize]
                } else {
                    pic.texel_clamped(x, y)
                }
            }
        }
    }
}

/// A frame being drawn, `size()` pixels, rows top-down.
pub trait Canvas {
    /// The frame's width and height in pixels.
    fn size(&self) -> (u32, u32);
    /// Every pixel black.
    fn clear(&mut self);
    /// The pixels whose centre is in `rect` set to `colour` (XRGB).
    fn fill(&mut self, rect: Rect, colour: u32);
    /// The node's background cube through `cam` (`TeWarp3::draw`'s cells,
    /// material mode 0): the six face pictures of [`crate::cube`], which
    /// stay the same while `key` does.
    fn background(&mut self, cam: &Camera, faces: &[Vec<u32>; 6], key: u64);
    /// A tile's triangles textured with `pic` through `cam` (material
    /// mode 1, blended), `GL_CLAMP_TO_EDGE`.
    fn mesh(&mut self, cam: &Camera, range: &TileRange, pic: &Picture);
    /// The `src` part (x, y, w, h, texels) of `pic` at its own size with
    /// its top-left at window (`left`, `top`), [`Edge::Padded`], blended
    /// (markers, the cursor: [`crate::sprite::draw`]).
    fn sprite(&mut self, pic: &Picture, src: (u32, u32, u32, u32), left: f64, top: f64);
    /// `pic` stretched over `rect`, its texels modulated by `tint`,
    /// [`Edge::Padded`], blended, inside `clip` (a GUI sprite:
    /// [`crate::gui::draw_picture`]).
    fn picture(&mut self, pic: &Picture, rect: Rect, clip: Option<Clip>, tint: [u8; 4]);
    /// `pic` over `rect` turned by `degrees` about its centre, modulated
    /// by `tint` ([`crate::gui::draw_rotated`]).
    fn rotated(&mut self, pic: &Picture, rect: Rect, degrees: f32, tint: [u8; 4]);
    /// `pic` stretched over `rect` in material `mode` with `edge` beyond
    /// its edges (the movies: [`crate::movie::draw_stretched`]); the
    /// pixels of `hole` (columns, rows) may be left out, an opaque
    /// picture drawn next covering them.
    fn stretched(
        &mut self,
        pic: &Picture,
        rect: Rect,
        mode: Mode,
        edge: Edge<'_>,
        hole: Option<(i64, i64, i64, i64)>,
    );
    /// A text block with its top-left at window pixel (`ox`, `oy`)
    /// ([`crate::gui::text::draw`]).
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        font: &Font,
        block: &Block,
        ox: i64,
        oy: i64,
        clip: Option<Clip>,
        tint: [u8; 4],
    );
}

/// What the processor's renderer keeps between frames: the background of
/// the last view (redrawn only when the view or the node changes) and the
/// meshes' coverage stamps.
#[derive(Default)]
pub struct CpuCache {
    background: Option<(BackgroundKey, Vec<u32>)>,
    cover: Coverage,
}

type BackgroundKey = (u64, u32, [u32; 4], u32, u32);

impl CpuCache {
    /// The background drawn again at the next frame (a node left).
    pub fn forget_background(&mut self) {
        self.background = None;
    }
}

/// The processor's renderer: a `width × height` XRGB8888 frame, rows
/// top-down.
pub struct CpuCanvas<'a> {
    pub out: &'a mut [u32],
    pub width: u32,
    pub height: u32,
    pub cache: &'a mut CpuCache,
    /// The bilinear weights' precision of the 3-D part.
    pub weights: Weights,
}

impl<'a> CpuCanvas<'a> {
    pub fn new(
        out: &'a mut [u32],
        width: u32,
        height: u32,
        cache: &'a mut CpuCache,
        weights: Weights,
    ) -> CpuCanvas<'a> {
        let n = width as usize * height as usize;
        CpuCanvas {
            out: &mut out[..n],
            width,
            height,
            cache,
            weights,
        }
    }
}

impl Canvas for CpuCanvas<'_> {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn clear(&mut self) {
        self.out.fill(0);
    }

    fn fill(&mut self, rect: Rect, colour: u32) {
        crate::movie::fill(self.out, self.width, self.height, rect, colour);
    }

    fn background(&mut self, cam: &Camera, faces: &[Vec<u32>; 6], key: u64) {
        let k = (
            key,
            cam.fov.to_bits(),
            cam.q.map(f32::to_bits),
            cam.width,
            cam.height,
        );
        let stale = self.cache.background.as_ref().is_none_or(|b| b.0 != k);
        if stale {
            let mut pixels = vec![0u32; self.out.len()];
            render(cam, faces, &mut pixels, self.weights);
            self.cache.background = Some((k, pixels));
        }
        if let Some((_, b)) = &self.cache.background {
            self.out.copy_from_slice(b);
        }
    }

    fn mesh(&mut self, cam: &Camera, range: &TileRange, pic: &Picture) {
        draw_mesh(
            cam,
            range,
            pic,
            self.out,
            &mut self.cache.cover,
            self.weights,
        );
    }

    fn sprite(&mut self, pic: &Picture, src: (u32, u32, u32, u32), left: f64, top: f64) {
        crate::sprite::draw(self.out, self.width, self.height, pic, src, left, top);
    }

    fn picture(&mut self, pic: &Picture, rect: Rect, clip: Option<Clip>, tint: [u8; 4]) {
        crate::gui::draw_picture(self.out, self.width, self.height, pic, rect, clip, tint);
    }

    fn rotated(&mut self, pic: &Picture, rect: Rect, degrees: f32, tint: [u8; 4]) {
        crate::gui::draw_rotated(self.out, self.width, self.height, pic, rect, degrees, tint);
    }

    fn stretched(
        &mut self,
        pic: &Picture,
        rect: Rect,
        mode: Mode,
        edge: Edge<'_>,
        hole: Option<(i64, i64, i64, i64)>,
    ) {
        crate::movie::draw_stretched_around(
            self.out,
            self.width,
            self.height,
            pic,
            rect,
            mode,
            &|x, y| edge.tap(pic, x, y),
            hole,
        );
    }

    fn text(
        &mut self,
        font: &Font,
        block: &Block,
        ox: i64,
        oy: i64,
        clip: Option<Clip>,
        tint: [u8; 4],
    ) {
        crate::gui::text::draw(
            self.out,
            self.width,
            self.height,
            font,
            block,
            ox,
            oy,
            clip,
            tint,
        );
    }
}

//! *Atlantis Evolution*'s frames drawn by the graphics card: the engine's
//! drawing primitives ([`atlantis4_engine::canvas::Canvas`]) given to
//! OpenGL 3.3 core, OpenGL ES 3.0 or WebGL 2 through `glow`, with the
//! original's GL state (`OPENGL32.dll` 1.x, fixed function):
//!
//! - every texture `GL_LINEAR` for magnification and minification, no
//!   mipmaps, `GL_CLAMP_TO_EDGE`; 8 bits per channel;
//! - blending on for the whole frame, `GL_SRC_ALPHA,
//!   GL_ONE_MINUS_SRC_ALPHA`; no depth test (the draw order is the
//!   original's: background, draw list, markers, GUI, cursor);
//! - material mode 0 (`GL_COMBINE`, alpha 1) for the background cells,
//!   the full-screen pictures and the opaque videos; mode 1
//!   (`GL_MODULATE` with the vertex colour) for the tiles, the sprites,
//!   the GUI pictures and the alpha videos; the texts' `GL_BLEND`
//!   environment (the vertex colour, the atlas's coverage as alpha);
//! - the warp's projection as the original loads it (`glLoadMatrixf` of
//!   `TeCamera`'s type-1 matrix, column-major) times the view's rotation;
//!   the GUI in window pixels, y down.
//!
//! The texture matrix of the original maps a picture's UVs onto the
//! picture's part of its power-of-two texture (`scale(w/W, h/H)`, the
//! flip for Theora frames). Here a texture holds the picture at its own
//! size, plus one texel row or column where the engine's model says what
//! lies beyond the picture ([`Edge`]): the same texels reach every tap.
//!
//! The frame is drawn into an RGBA8 frame buffer of the window's size (the
//! original's pixel format: RGBA 8-8-8-8) and copied to the window; it
//! can be read back ([`Renderer::read_pixels`]) to compare it with the
//! processor's ([`check`]).
//!
//! What stays the driver's (decision 0002 § 4): the bilinear weights'
//! precision, the rasterisation of pixels on an edge, the rounding of
//! blending. [`check`] measures them against the processor's exact model.

use std::collections::HashMap;

use atlantis4_data::anim::TileRange;
use atlantis4_engine::camera::Camera;
use atlantis4_engine::canvas::{Canvas, Edge};
use atlantis4_engine::cube::{CELLS, CELL_GUTTER, CELL_TEXELS, FACES, FACE_PICTURE, SLOT};
use atlantis4_engine::gui::modulate;
use atlantis4_engine::gui::text::{line_quads, Block, Clip, Font};
use atlantis4_engine::layout::Rect;
use atlantis4_engine::movie::Mode;
use atlantis4_engine::package::Picture;
use glow::HasContext;

pub mod check;

/// The shaders' body; the first line (`#version`) depends on the context.
const VERTEX: &str = r#"
in vec3 a_pos;
in vec2 a_uv;
in vec4 a_col;
uniform mat4 u_mvp;
out vec2 v_uv;
out vec4 v_col;
void main() {
    v_uv = a_uv;
    v_col = a_col;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
"#;

/// Modes: 0 material mode 0 (`GL_COMBINE`: colour × texture, alpha 1);
/// 1 mode 1 (`GL_MODULATE`); 2 the texts' `GL_BLEND` environment with a
/// coverage atlas (the vertex colour, its alpha × the coverage); 3 a
/// plain colour.
const FRAGMENT: &str = r#"
precision highp float;
uniform sampler2D u_tex;
uniform int u_mode;
in vec2 v_uv;
in vec4 v_col;
out vec4 o_col;
void main() {
    if (u_mode == 3) {
        o_col = v_col;
        return;
    }
    vec4 t = texture(u_tex, v_uv);
    if (u_mode == 0) {
        o_col = vec4(t.rgb * v_col.rgb, 1.0);
    } else if (u_mode == 1) {
        o_col = t * v_col;
    } else {
        o_col = vec4(v_col.rgb, v_col.a * t.r);
    }
}
"#;

const MODE_OPAQUE: i32 = 0;
const MODE_MODULATE: i32 = 1;
const MODE_TEXT: i32 = 2;
const MODE_SOLID: i32 = 3;

/// A vertex: position, texture coordinates, colour (RGBA bytes).
#[derive(Clone, Copy, Debug)]
struct Vertex {
    p: [f32; 3],
    uv: [f32; 2],
    c: [u8; 4],
}

const STRIDE: i32 = 24;

fn put(out: &mut Vec<u8>, v: &Vertex) {
    for x in v.p.iter().chain(&v.uv) {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.extend_from_slice(&v.c);
}

/// A texture of the cache: its size (texels, the edge's row and column
/// included), where the picture's texel (0, 0) is in it, and the last
/// frame that drew with it.
struct Tex {
    tex: glow::Texture,
    size: (u32, u32),
    origin: (u32, u32),
    used: u64,
}

/// A picture's texture, its size and where the picture's texel (0, 0) is
/// in it.
type Placed = (glow::Texture, (u32, u32), (u32, u32));

/// What a cached texture holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Clamp,
    Padded,
    Margin,
    Coverage,
}

/// The renderer: the context, its program and buffers, the frame buffer
/// and the textures of the pictures drawn lately.
pub struct Renderer {
    gl: glow::Context,
    program: glow::Program,
    u_mvp: Option<glow::UniformLocation>,
    u_mode: Option<glow::UniformLocation>,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    /// The background cube's cells (static).
    cube: glow::Buffer,
    fbo: glow::Framebuffer,
    colour: glow::Renderbuffer,
    fb_size: (u32, u32),
    /// The pictures' textures, by content.
    textures: HashMap<u64, Tex>,
    /// This frame's content keys of the pictures already seen (address
    /// and length → key): each picture is hashed once per frame.
    seen: HashMap<(usize, usize, Kind), u64>,
    /// The six face textures and the key they were made for.
    faces: Option<(u64, [glow::Texture; 6])>,
    frame: u64,
    /// `GL_RENDERER`, `GL_VERSION`.
    pub info: String,
    /// The frame's size and its 2-D matrix.
    size: (u32, u32),
    ortho: [f32; 16],
    /// Vertices waiting for one draw call.
    batch: Vec<u8>,
}

/// Why no renderer could be made.
#[derive(Debug)]
pub struct GlError(pub String);

impl std::fmt::Display for GlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OpenGL: {}", self.0)
    }
}

impl std::error::Error for GlError {}

/// How many frames a texture stays after its last use (a GUI picture that
/// comes back soon keeps its texture; a movie's frames go).
const KEEP_FRAMES: u64 = 3;

impl Renderer {
    /// A renderer on the current context `gl` (OpenGL 3.3 or later, OpenGL
    /// ES 3.0 or later, or WebGL 2).
    pub fn new(gl: glow::Context) -> Result<Renderer, GlError> {
        // SAFETY: `gl` is a current context; every object made here is
        // owned by the renderer and used with this context only.
        unsafe {
            let v = gl.version();
            let es = v.is_embedded;
            if v.major < 3 {
                return Err(GlError(format!("version {}.{} under 3", v.major, v.minor)));
            }
            let info = format!(
                "{} / {} / {}",
                gl.get_parameter_string(glow::VENDOR),
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION)
            );
            let header = if es {
                "#version 300 es\n"
            } else {
                "#version 330 core\n"
            };
            let program = link(
                &gl,
                &format!("{header}{VERTEX}"),
                &format!("{header}{FRAGMENT}"),
            )?;
            let u_mvp = gl.get_uniform_location(program, "u_mvp");
            let u_mode = gl.get_uniform_location(program, "u_mode");
            gl.use_program(Some(program));
            let u_tex = gl.get_uniform_location(program, "u_tex");
            gl.uniform_1_i32(u_tex.as_ref(), 0);
            let vao = gl.create_vertex_array().map_err(GlError)?;
            let vbo = gl.create_buffer().map_err(GlError)?;
            let cube = gl.create_buffer().map_err(GlError)?;
            gl.bind_vertex_array(Some(vao));
            let mut bytes = Vec::new();
            for v in cube_vertices() {
                put(&mut bytes, &v);
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(cube));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
            let fbo = gl.create_framebuffer().map_err(GlError)?;
            let colour = gl.create_renderbuffer().map_err(GlError)?;
            // The original's global state (@ 0x0050b2e0): blending on,
            // `GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA`; no face culling; the
            // warp's drawing without depth test.
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::DEPTH_TEST);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
            Ok(Renderer {
                gl,
                program,
                u_mvp,
                u_mode,
                vao,
                vbo,
                cube,
                fbo,
                colour,
                fb_size: (0, 0),
                textures: HashMap::new(),
                seen: HashMap::new(),
                faces: None,
                frame: 0,
                info,
                size: (0, 0),
                ortho: [0.0; 16],
                batch: Vec::new(),
            })
        }
    }

    /// A renderer on a canvas's WebGL 2 context (made with `antialias:
    /// false`, so that the frame can be copied to it, and `alpha: false`).
    #[cfg(target_arch = "wasm32")]
    pub fn from_webgl2(ctx: web_sys::WebGl2RenderingContext) -> Result<Renderer, GlError> {
        Renderer::new(glow::Context::from_webgl2_context(ctx))
    }

    /// The context.
    pub fn gl(&self) -> &glow::Context {
        &self.gl
    }

    /// Starts a `width × height` frame (the frame buffer resized when
    /// needed): a [`Canvas`] until [`Frame::finish`].
    pub fn begin(&mut self, width: u32, height: u32) -> Frame<'_> {
        let (width, height) = (width.max(1), height.max(1));
        self.frame += 1;
        self.seen.clear();
        // SAFETY: the renderer's own objects on its context.
        unsafe {
            let gl = &self.gl;
            if self.fb_size != (width, height) {
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(self.colour));
                gl.renderbuffer_storage(
                    glow::RENDERBUFFER,
                    glow::RGBA8,
                    width as i32,
                    height as i32,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::RENDERBUFFER,
                    Some(self.colour),
                );
                let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
                if status != glow::FRAMEBUFFER_COMPLETE {
                    log::error!("frame buffer incomplete: {status:#x}");
                }
                self.fb_size = (width, height);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, width as i32, height as i32);
            gl.disable(glow::SCISSOR_TEST);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.active_texture(glow::TEXTURE0);
        }
        self.size = (width, height);
        // Window pixels, y down, to normalised device coordinates.
        let (w, h) = (f64::from(width), f64::from(height));
        self.ortho = [
            (2.0 / w) as f32,
            0.0,
            0.0,
            0.0,
            0.0,
            (-2.0 / h) as f32,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            -1.0,
            1.0,
            0.0,
            1.0,
        ];
        Frame { r: self }
    }

    /// The last frame copied to the window's frame buffer (`target`, the
    /// default one when `None`), `window` pixels, the frame stretched over
    /// it.
    pub fn present(&self, target: Option<glow::Framebuffer>, window: (u32, u32)) {
        let (w, h) = self.fb_size;
        // SAFETY: the renderer's own objects on its context.
        unsafe {
            let gl = &self.gl;
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, target);
            gl.disable(glow::SCISSOR_TEST);
            gl.blit_framebuffer(
                0,
                0,
                w as i32,
                h as i32,
                0,
                0,
                window.0 as i32,
                window.1 as i32,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
    }

    /// The last frame read back: XRGB8888, rows top-down (as the
    /// processor's).
    pub fn read_pixels(&self) -> Vec<u32> {
        let (w, h) = self.fb_size;
        let mut rgba = vec![0u8; 4 * w as usize * h as usize];
        // SAFETY: the buffer holds `w × h` RGBA pixels.
        unsafe {
            let gl = &self.gl;
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.fbo));
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut rgba)),
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        }
        let (w, h) = (w as usize, h as usize);
        let mut out = vec![0u32; w * h];
        for (j, row) in out.chunks_mut(w).enumerate() {
            // GL's rows go up from the bottom.
            let src = &rgba[(h - 1 - j) * w * 4..][..w * 4];
            for (o, p) in row.iter_mut().zip(src.as_chunks::<4>().0) {
                *o = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
            }
        }
        out
    }

    /// Waits until the card has drawn everything (for timings).
    pub fn finish_gpu(&self) {
        // SAFETY: no object involved.
        unsafe { self.gl.finish() }
    }

    /// The texture of `pic` with `edge` beyond it, made or found by
    /// content.
    fn texture(&mut self, pic: &Picture, edge: Edge<'_>) -> Option<Placed> {
        if pic.width == 0
            || pic.height == 0
            || pic.rgba.len() < 4 * (pic.width * pic.height) as usize
        {
            return None;
        }
        let kind = match edge {
            Edge::Clamp => Kind::Clamp,
            Edge::Padded => Kind::Padded,
            Edge::Margin { .. } => Kind::Margin,
        };
        let id = (pic.rgba.as_ptr() as usize, pic.rgba.len(), kind);
        let key = match self.seen.get(&id) {
            Some(&k) => k,
            None => {
                let mut h = Hasher::new(kind as u64);
                h.u64(u64::from(pic.width) << 32 | u64::from(pic.height));
                h.bytes(&pic.rgba);
                if let Edge::Margin { col, row } = edge {
                    h.bytes(col.as_flattened());
                    h.bytes(row.as_flattened());
                }
                let k = h.finish();
                self.seen.insert(id, k);
                k
            }
        };
        if !self.textures.contains_key(&key) {
            let (w, h) = (pic.width as usize, pic.height as usize);
            let (data, size, origin) = match edge {
                Edge::Clamp => (None, (pic.width, pic.height), (0, 0)),
                Edge::Padded => {
                    // A transparent row above, a transparent column on the
                    // right.
                    let tw = w + 1;
                    let mut d = vec![0u8; 4 * tw * (h + 1)];
                    for y in 0..h {
                        d[4 * (y + 1) * tw..][..4 * w]
                            .copy_from_slice(&pic.rgba[4 * y * w..][..4 * w]);
                    }
                    (Some(d), (pic.width + 1, pic.height + 1), (0, 1))
                }
                Edge::Margin { col, row } => {
                    let tw = w + 1;
                    let mut d = vec![0u8; 4 * tw * (h + 1)];
                    for y in 0..h {
                        d[4 * y * tw..][..4 * w].copy_from_slice(&pic.rgba[4 * y * w..][..4 * w]);
                        d[4 * (y * tw + w)..][..4].copy_from_slice(&col[y.min(col.len() - 1)]);
                    }
                    for x in 0..w {
                        d[4 * (h * tw + x)..][..4].copy_from_slice(&row[x.min(row.len() - 1)]);
                    }
                    d[4 * (h * tw + w)..][..4].copy_from_slice(&col[col.len() - 1]);
                    (Some(d), (pic.width + 1, pic.height + 1), (0, 0))
                }
            };
            let bytes = data.as_deref().unwrap_or(&pic.rgba[..4 * w * h]);
            let tex = self.upload(size, glow::RGBA8, glow::RGBA, bytes)?;
            self.textures.insert(
                key,
                Tex {
                    tex,
                    size,
                    origin,
                    used: self.frame,
                },
            );
        }
        let t = self.textures.get_mut(&key)?;
        t.used = self.frame;
        Some((t.tex, t.size, t.origin))
    }

    /// A font atlas's coverage as a one-channel texture.
    fn coverage(&mut self, font: &Font) -> Option<(glow::Texture, (u32, u32))> {
        let (w, h) = (font.atlas_width, font.atlas_height);
        if w == 0 || h == 0 || font.alpha.len() < (w * h) as usize {
            return None;
        }
        let id = (
            font.alpha.as_ptr() as usize,
            font.alpha.len(),
            Kind::Coverage,
        );
        let key = match self.seen.get(&id) {
            Some(&k) => k,
            None => {
                let mut hs = Hasher::new(Kind::Coverage as u64);
                hs.u64(u64::from(w) << 32 | u64::from(h));
                hs.bytes(&font.alpha);
                let k = hs.finish();
                self.seen.insert(id, k);
                k
            }
        };
        if !self.textures.contains_key(&key) {
            let tex = self.upload((w, h), glow::R8, glow::RED, &font.alpha[..(w * h) as usize])?;
            self.textures.insert(
                key,
                Tex {
                    tex,
                    size: (w, h),
                    origin: (0, 0),
                    used: self.frame,
                },
            );
        }
        let t = self.textures.get_mut(&key)?;
        t.used = self.frame;
        Some((t.tex, t.size))
    }

    /// A new texture of `size` from `bytes` (rows top-down), `GL_LINEAR`
    /// both ways, `GL_CLAMP_TO_EDGE`, level 0 only (the four
    /// `glTexParameteri` of @ 0x004ea337..0x004ea376).
    fn upload(
        &self,
        size: (u32, u32),
        internal: u32,
        format: u32,
        bytes: &[u8],
    ) -> Option<glow::Texture> {
        // SAFETY: `bytes` holds `size` texels of `format`; the texture is
        // the renderer's.
        unsafe {
            let gl = &self.gl;
            let tex = match gl.create_texture() {
                Ok(t) => t,
                Err(e) => {
                    log::error!("no texture: {e}");
                    return None;
                }
            };
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            for (p, v) in [
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, p, v as i32);
            }
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal as i32,
                size.0 as i32,
                size.1 as i32,
                0,
                format,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(bytes)),
            );
            Some(tex)
        }
    }

    /// The six face pictures as textures (made again when `key` changes,
    /// or the pictures: their addresses and a sample of their texels are
    /// part of the key, the pictures being too large to hash each frame).
    fn face_textures(&mut self, faces: &[Vec<u32>; 6], key: u64) -> Option<[glow::Texture; 6]> {
        let mut h = Hasher::new(key);
        for f in faces {
            h.u64(f.as_ptr() as u64 ^ (f.len() as u64) << 40);
            let step = (f.len() / 61).max(1);
            for p in f.iter().step_by(step) {
                h.u64(u64::from(*p));
            }
        }
        let key = h.finish();
        if let Some((k, t)) = &self.faces {
            if *k == key {
                return Some(*t);
            }
        }
        if let Some((_, old)) = self.faces.take() {
            // SAFETY: the renderer's textures.
            unsafe {
                for t in old {
                    self.gl.delete_texture(t);
                }
            }
        }
        let n = FACE_PICTURE;
        let mut made = Vec::with_capacity(6);
        for f in faces {
            if f.len() < n * n {
                return None;
            }
            let rgb: Vec<u8> = f[..n * n]
                .iter()
                .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
                .collect();
            made.push(self.upload((n as u32, n as u32), glow::RGB8, glow::RGB, &rgb)?);
        }
        let t: [glow::Texture; 6] = made.try_into().ok()?;
        self.faces = Some((key, t));
        Some(t)
    }

    /// Drops the textures not drawn with for [`KEEP_FRAMES`] frames.
    fn evict(&mut self) {
        let frame = self.frame;
        let gl = &self.gl;
        self.textures.retain(|_, t| {
            let keep = frame - t.used < KEEP_FRAMES;
            if !keep {
                // SAFETY: the renderer's texture, dropped from the cache.
                unsafe { gl.delete_texture(t.tex) };
            }
            keep
        });
    }

    /// Draws the batch's vertices as triangles with `tex`, `mode`, `mvp`,
    /// inside `clip` (columns, rows, top-down) when given.
    fn flush(
        &mut self,
        tex: Option<glow::Texture>,
        mode: i32,
        mvp: &[f32; 16],
        clip: Option<Clip>,
    ) {
        if self.batch.is_empty() {
            return;
        }
        let n = self.batch.len() as i32 / STRIDE;
        // SAFETY: the batch holds `n` vertices of the program's layout.
        unsafe {
            let gl = &self.gl;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &self.batch, glow::STREAM_DRAW);
            attributes(gl);
            self.draw_bound(tex, mode, mvp, clip, 0, n);
        }
        self.batch.clear();
    }

    /// Draws `count` vertices from `first` of the bound buffer.
    unsafe fn draw_bound(
        &self,
        tex: Option<glow::Texture>,
        mode: i32,
        mvp: &[f32; 16],
        clip: Option<Clip>,
        first: i32,
        count: i32,
    ) {
        let gl = &self.gl;
        gl.uniform_matrix_4_f32_slice(self.u_mvp.as_ref(), false, mvp);
        gl.uniform_1_i32(self.u_mode.as_ref(), mode);
        gl.bind_texture(glow::TEXTURE_2D, tex);
        match clip {
            Some((x0, x1, y0, y1)) => {
                let (w, h) = (i64::from(self.size.0), i64::from(self.size.1));
                let (x0, x1) = (x0.clamp(0, w), x1.clamp(0, w));
                let (y0, y1) = (y0.clamp(0, h), y1.clamp(0, h));
                if x0 >= x1 || y0 >= y1 {
                    return;
                }
                gl.enable(glow::SCISSOR_TEST);
                gl.scissor(
                    x0 as i32,
                    (h - y1) as i32,
                    (x1 - x0) as i32,
                    (y1 - y0) as i32,
                );
            }
            None => gl.disable(glow::SCISSOR_TEST),
        }
        gl.draw_arrays(glow::TRIANGLES, first, count);
    }

    /// A window-pixel quad (`x0, y0` – `x1, y1`, y down) with texture
    /// coordinates `u0, v0` – `u1, v1` into the batch.
    #[allow(clippy::too_many_arguments)]
    fn quad(
        &mut self,
        (x0, y0, x1, y1): (f64, f64, f64, f64),
        (u0, v0, u1, v1): (f64, f64, f64, f64),
        c: [u8; 4],
    ) {
        let v = |x: f64, y: f64, u: f64, t: f64| Vertex {
            p: [x as f32, y as f32, 0.0],
            uv: [u as f32, t as f32],
            c,
        };
        let (a, b, cc, d) = (
            v(x0, y0, u0, v0),
            v(x1, y0, u1, v0),
            v(x1, y1, u1, v1),
            v(x0, y1, u0, v1),
        );
        for p in [a, b, cc, a, cc, d] {
            put(&mut self.batch, &p);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // SAFETY: the renderer's own objects, deleted once.
        unsafe {
            let gl = &self.gl;
            for t in self.textures.values() {
                gl.delete_texture(t.tex);
            }
            if let Some((_, f)) = &self.faces {
                for t in f {
                    gl.delete_texture(*t);
                }
            }
            gl.delete_buffer(self.vbo);
            gl.delete_buffer(self.cube);
            gl.delete_vertex_array(self.vao);
            gl.delete_framebuffer(self.fbo);
            gl.delete_renderbuffer(self.colour);
            gl.delete_program(self.program);
        }
    }
}

/// The vertex layout of the bound buffer.
unsafe fn attributes(gl: &glow::Context) {
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, STRIDE, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, STRIDE, 12);
    gl.enable_vertex_attrib_array(2);
    gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, STRIDE, 20);
}

unsafe fn link(gl: &glow::Context, vs: &str, fs: &str) -> Result<glow::Program, GlError> {
    let program = gl.create_program().map_err(GlError)?;
    let mut shaders = Vec::new();
    for (ty, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
        let s = gl.create_shader(ty).map_err(GlError)?;
        gl.shader_source(s, src);
        gl.compile_shader(s);
        if !gl.get_shader_compile_status(s) {
            return Err(GlError(format!("shader: {}", gl.get_shader_info_log(s))));
        }
        gl.attach_shader(program, s);
        shaders.push(s);
    }
    for (i, name) in ["a_pos", "a_uv", "a_col"].into_iter().enumerate() {
        gl.bind_attrib_location(program, i as u32, name);
    }
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        return Err(GlError(format!(
            "program: {}",
            gl.get_program_info_log(program)
        )));
    }
    for s in shaders {
        gl.detach_shader(program, s);
        gl.delete_shader(s);
    }
    Ok(program)
}

/// The background's 6 × 13 × 13 cells as triangles, face after face: each
/// cell `(ci, cj)` of face `f` the quad `C + s·U + t·V`, `s` from `ci/13`
/// to `(ci + 1)/13`, `t` likewise, textured from its slot of the face
/// picture ([`atlantis4_engine::cube::cell_texel`]: 60 × 60 texels at
/// (2, 2) of the 64 × 64 slot at `(64·ci, 64·(12 − cj))`, rows top-down).
fn cube_vertices() -> Vec<Vertex> {
    let n = CELLS as f64;
    let pic = FACE_PICTURE as f64;
    let (g, k, slot) = (CELL_GUTTER as f64, CELL_TEXELS as f64, SLOT as f64);
    let mut out = Vec::with_capacity(6 * CELLS * CELLS * 6);
    for face in &FACES {
        let at = |s: f64, t: f64| -> [f32; 3] {
            std::array::from_fn(|i| {
                (f64::from(face.c[i]) + s * f64::from(face.u[i]) + t * f64::from(face.v[i])) as f32
            })
        };
        for cj in 0..CELLS {
            for ci in 0..CELLS {
                let (fi, fj) = (ci as f64, cj as f64);
                let v = |a: f64, b: f64| Vertex {
                    p: at((fi + a) / n, (fj + b) / n),
                    uv: [
                        ((slot * fi + g + k * a) / pic) as f32,
                        ((slot * (n - 1.0 - fj) + g + k * (1.0 - b)) / pic) as f32,
                    ],
                    c: [255; 4],
                };
                let (p00, p10, p11, p01) = (v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0));
                out.extend([p00, p10, p11, p00, p11, p01]);
            }
        }
    }
    out
}

/// The warp's matrix: the projection as `glLoadMatrixf` takes it
/// (column-major) times the view's turn (world to camera, the transpose
/// of [`Camera::to_world`]), in `f64`, then stored as floats.
pub fn warp_matrix(cam: &Camera) -> [f32; 16] {
    let m = cam.m.map(f64::from);
    // `to_world` of the camera's axes: the columns of R (world = R·cam).
    let cols: [[f64; 3]; 3] = std::array::from_fn(|j| {
        let mut e = [0.0; 3];
        e[j] = 1.0;
        cam.to_world(e)
    });
    // View: V[r][c] = R[c][r] = cols[r][c].
    let view = |r: usize, c: usize| -> f64 {
        match (r, c) {
            (3, 3) => 1.0,
            (3, _) | (_, 3) => 0.0,
            _ => cols[r][c],
        }
    };
    let p = |r: usize, c: usize| m[c * 4 + r];
    let mut out = [0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| p(r, k) * view(k, c)).sum::<f64>() as f32;
        }
    }
    out
}

/// A frame being drawn on the card.
pub struct Frame<'r> {
    r: &'r mut Renderer,
}

impl Frame<'_> {
    /// The frame done: the textures not used lately dropped. Draw it with
    /// [`Renderer::present`], read it with [`Renderer::read_pixels`].
    pub fn finish(self) {
        // Opaque alpha everywhere (the blending leaves `As² + Ad·(1 − As)`
        // there): the frame shows as is on a window with an alpha channel.
        // SAFETY: the frame buffer is bound.
        unsafe {
            let gl = &self.r.gl;
            gl.disable(glow::SCISSOR_TEST);
            gl.color_mask(false, false, false, true);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.color_mask(true, true, true, true);
        }
        self.r.evict();
    }
}

fn to_rgba(c: u32) -> [u8; 4] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]
}

impl Canvas for Frame<'_> {
    fn size(&self) -> (u32, u32) {
        self.r.size
    }

    fn clear(&mut self) {
        // SAFETY: the frame buffer is bound.
        unsafe {
            let gl = &self.r.gl;
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    fn fill(&mut self, rect: Rect, colour: u32) {
        let (x1, y1) = (rect.left + rect.width, rect.top + rect.height);
        self.r.quad(
            (rect.left, rect.top, x1, y1),
            (0.0, 0.0, 0.0, 0.0),
            to_rgba(colour),
        );
        let m = self.r.ortho;
        self.r.flush(None, MODE_SOLID, &m, None);
    }

    fn background(&mut self, cam: &Camera, faces: &[Vec<u32>; 6], key: u64) {
        self.clear();
        let Some(tex) = self.r.face_textures(faces, key) else {
            return;
        };
        let mvp = warp_matrix(cam);
        let per = (CELLS * CELLS * 6) as i32;
        // SAFETY: the static cube buffer holds 6 × `per` vertices.
        unsafe {
            let gl = &self.r.gl;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.r.cube));
            attributes(gl);
            for (f, t) in tex.iter().enumerate() {
                self.r
                    .draw_bound(Some(*t), MODE_OPAQUE, &mvp, None, f as i32 * per, per);
            }
        }
    }

    fn mesh(&mut self, cam: &Camera, range: &TileRange, pic: &Picture) {
        let Some((tex, size, _)) = self.r.texture(pic, Edge::Clamp) else {
            return;
        };
        debug_assert_eq!(size, (pic.width, pic.height));
        let vert = |i: u32| range.vertices.get(i as usize).copied().unwrap_or([0.0; 3]);
        let uv = |i: u32| range.uvs.get(i as usize).copied().unwrap_or([0.0; 2]);
        for t in range.indices.as_chunks::<3>().0 {
            for &i in t {
                let q = uv(i);
                // The picture's texel `(u·w, (1 − v)·h)` from its top row.
                let v = Vertex {
                    p: vert(i),
                    uv: [q[0], (1.0 - f64::from(q[1])) as f32],
                    c: [255; 4],
                };
                put(&mut self.r.batch, &v);
            }
        }
        let mvp = warp_matrix(cam);
        self.r.flush(Some(tex), MODE_MODULATE, &mvp, None);
    }

    fn sprite(&mut self, pic: &Picture, src: (u32, u32, u32, u32), left: f64, top: f64) {
        let Some((tex, size, o)) = self.r.texture(pic, Edge::Padded) else {
            return;
        };
        let (sx, sy, sw, sh) = src;
        let (tw, th) = (f64::from(size.0), f64::from(size.1));
        let u0 = f64::from(sx + o.0) / tw;
        let v0 = f64::from(sy + o.1) / th;
        let u1 = f64::from(sx + o.0 + sw) / tw;
        let v1 = f64::from(sy + o.1 + sh) / th;
        let (w, h) = (f64::from(sw), f64::from(sh));
        self.r
            .quad((left, top, left + w, top + h), (u0, v0, u1, v1), [255; 4]);
        let m = self.r.ortho;
        self.r.flush(Some(tex), MODE_MODULATE, &m, None);
    }

    fn picture(&mut self, pic: &Picture, rect: Rect, clip: Option<Clip>, tint: [u8; 4]) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let Some((tex, size, o)) = self.r.texture(pic, Edge::Padded) else {
            return;
        };
        let (tw, th) = (f64::from(size.0), f64::from(size.1));
        let uv = (
            f64::from(o.0) / tw,
            f64::from(o.1) / th,
            f64::from(o.0 + pic.width) / tw,
            f64::from(o.1 + pic.height) / th,
        );
        self.r.quad(
            (
                rect.left,
                rect.top,
                rect.left + rect.width,
                rect.top + rect.height,
            ),
            uv,
            tint,
        );
        let m = self.r.ortho;
        self.r.flush(Some(tex), MODE_MODULATE, &m, clip);
    }

    fn rotated(&mut self, pic: &Picture, rect: Rect, degrees: f32, tint: [u8; 4]) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let Some((tex, _, _)) = self.r.texture(pic, Edge::Clamp) else {
            return;
        };
        let (cx, cy) = (rect.left + rect.width / 2.0, rect.top + rect.height / 2.0);
        let a = f64::from(degrees).to_radians();
        let (s, c) = (a.sin(), a.cos());
        let (hw, hh) = (rect.width / 2.0, rect.height / 2.0);
        // The unturned (u, v) to the window: dx = c·u − s·v, dy = s·u + c·v
        // (the inverse of the processor's turn).
        let at = |u: f64, v: f64| Vertex {
            p: [
                (cx + c * u - s * v) as f32,
                (cy + s * u + c * v) as f32,
                0.0,
            ],
            uv: [
                ((u + hw) / rect.width) as f32,
                ((v + hh) / rect.height) as f32,
            ],
            c: tint,
        };
        let (a0, b0, c0, d0) = (at(-hw, -hh), at(hw, -hh), at(hw, hh), at(-hw, hh));
        for p in [a0, b0, c0, a0, c0, d0] {
            put(&mut self.r.batch, &p);
        }
        let m = self.r.ortho;
        self.r.flush(Some(tex), MODE_MODULATE, &m, None);
    }

    fn stretched(
        &mut self,
        pic: &Picture,
        rect: Rect,
        mode: Mode,
        edge: Edge<'_>,
        _hole: Option<(i64, i64, i64, i64)>,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let Some((tex, size, o)) = self.r.texture(pic, edge) else {
            return;
        };
        let (tw, th) = (f64::from(size.0), f64::from(size.1));
        let uv = (
            f64::from(o.0) / tw,
            f64::from(o.1) / th,
            f64::from(o.0 + pic.width) / tw,
            f64::from(o.1 + pic.height) / th,
        );
        self.r.quad(
            (
                rect.left,
                rect.top,
                rect.left + rect.width,
                rect.top + rect.height,
            ),
            uv,
            [255; 4],
        );
        let m = self.r.ortho;
        let mode = match mode {
            Mode::Opaque => MODE_OPAQUE,
            Mode::Blend => MODE_MODULATE,
        };
        self.r.flush(Some(tex), mode, &m, None);
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
        let Some((tex, (aw, ah))) = self.r.coverage(font) else {
            return;
        };
        let (aw, ah) = (f64::from(aw), f64::from(ah));
        for line in &block.lines {
            for q in line_quads(font, block, line) {
                let (qw, qh) = (q.x1 - q.x0, q.y1 - q.y0);
                if qw <= 0 || qh <= 0 {
                    continue;
                }
                let [f8, f9, f0, f1] = q.atlas.map(f64::from);
                // The atlas row (from its top) of the quad's top edge.
                let top = ah - (f9 + f1);
                let x = |v: i32| (ox + i64::from(v)) as f64;
                let y = |v: i32| (oy + i64::from(v)) as f64;
                self.r.quad(
                    (x(q.x0), y(q.y0), x(q.x1), y(q.y1)),
                    (f8 / aw, top / ah, (f8 + f0) / aw, (top + f1) / ah),
                    modulate(q.colour, tint),
                );
            }
        }
        let m = self.r.ortho;
        self.r.flush(Some(tex), MODE_TEXT, &m, clip);
    }
}

/// A 64-bit hash of the pictures' bytes (eight bytes at a time, a
/// multiply-xorshift mix): fast enough to key every picture of a frame.
struct Hasher(u64);

impl Hasher {
    fn new(seed: u64) -> Hasher {
        Hasher(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    #[inline]
    fn u64(&mut self, x: u64) {
        self.0 = (self.0 ^ x)
            .wrapping_mul(0xff51_afd7_ed55_8ccd)
            .rotate_left(29);
    }

    fn bytes(&mut self, b: &[u8]) {
        let (words, rest) = b.as_chunks::<8>();
        for w in words {
            self.u64(u64::from_le_bytes(*w));
        }
        let mut last = [0u8; 8];
        last[..rest.len()].copy_from_slice(rest);
        self.u64(u64::from_le_bytes(last) ^ (b.len() as u64) << 56);
    }

    fn finish(&self) -> u64 {
        let mut x = self.0;
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^ (x >> 33)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis4_engine::camera::FOV_HD;

    /// The matrix takes a world point where the processor's `project`
    /// puts it.
    #[test]
    fn the_warp_matrix_projects_as_the_camera() {
        let cam = Camera::new(FOV_HD, 1365, 768, 37.0, -12.0);
        let m = warp_matrix(&cam);
        for p in [
            [-300.0f32, 120.0, -500.0],
            [500.0, -40.0, -260.0],
            [10.0, 500.0, -20.0],
        ] {
            let clip: [f64; 4] = std::array::from_fn(|r| {
                (0..3)
                    .map(|k| f64::from(m[k * 4 + r]) * f64::from(p[k]))
                    .sum::<f64>()
                    + f64::from(m[12 + r])
            });
            let want = cam.project(p);
            if clip[3] <= 0.0 {
                continue;
            }
            let x = (clip[0] / clip[3] + 1.0) * f64::from(cam.width / 2);
            let y = (clip[1] / clip[3] + 1.0) * f64::from(cam.height / 2);
            assert!((x - f64::from(want[0])).abs() < 1e-2, "{x} {want:?}");
            assert!((y - f64::from(want[1])).abs() < 1e-2, "{y} {want:?}");
        }
    }

    /// The cube's texture coordinates are those of `cell_texel` (texel
    /// centres at half integers in GL).
    #[test]
    fn the_cells_sample_their_slots() {
        let v = cube_vertices();
        assert_eq!(v.len(), 6 * 169 * 6);
        let pic = FACE_PICTURE as f64;
        let (x, y) = atlantis4_engine::cube::cell_texel(0.0, 0.0);
        assert!((f64::from(v[0].uv[0]) * pic - 0.5 - x).abs() < 1e-3);
        assert!((f64::from(v[0].uv[1]) * pic - 0.5 - y).abs() < 1e-3);
    }

    #[test]
    fn the_hash_sees_every_byte() {
        let a = vec![7u8; 1000];
        let mut b = a.clone();
        b[999] = 8;
        let h = |d: &[u8]| {
            let mut h = Hasher::new(0);
            h.bytes(d);
            h.finish()
        };
        assert_ne!(h(&a), h(&b));
        assert_ne!(h(&a[..999]), h(&a));
    }
}

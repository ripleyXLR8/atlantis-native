//! The browser's presentation through WebGL 2: a frame (XRGB8888, as the
//! engine draws it) goes to the graphics card as a texture and is drawn
//! scaled into a rectangle of the canvas. It replaces the 2D canvas, where
//! every frame was converted to RGBA pixel by pixel, copied into an
//! `ImageData` and drawn, all on the one core the game runs on.

use atlantis_engine::hd::ViewJob;
use wasm_bindgen::JsCast;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlProgram, WebGlShader, WebGlTexture,
    WebGlUniformLocation,
};

/// A full-screen triangle from the vertex index (no vertex buffer), the
/// texture's rows from the top.
const VERTEX: &str = r#"#version 300 es
out vec2 uv;
void main() {
    vec2 p = vec2(float((gl_VertexID & 1) << 2), float((gl_VertexID & 2) << 1)) - 1.0;
    uv = vec2(p.x + 1.0, 1.0 - p.y) * 0.5;
    gl_Position = vec4(p, 0.0, 1.0);
}
"#;

/// The XRGB8888 words arrive as the bytes B, G, R, X; with `overlay`, X
/// is the alpha of a premultiplied overlay (see `HdLayer::on_card`).
const FRAGMENT: &str = r#"#version 300 es
precision mediump float;
uniform sampler2D frame;
uniform int overlay;
in vec2 uv;
out vec4 color;
void main() {
    vec4 t = texture(frame, uv);
    color = overlay != 0 ? t.bgra : vec4(t.bgr, 1.0);
}
"#;

/// A rectangle of the canvas: x, y from the top left, width, height.
pub type Rect = (i32, i32, i32, i32);

pub struct GlPresenter {
    canvas: HtmlCanvasElement,
    gl: Gl,
    program: WebGlProgram,
    vao: web_sys::WebGlVertexArrayObject,
    texture: WebGlTexture,
    overlay: Option<WebGlUniformLocation>,
    /// The texture's size, to update it in place while it is unchanged.
    size: (u32, u32),
}

impl GlPresenter {
    /// WebGL 2 on `canvas`, or None (no WebGL 2, a shader refused): the
    /// caller keeps the 2D canvas then.
    pub fn new(canvas: HtmlCanvasElement) -> Option<GlPresenter> {
        let opts = js_sys::Object::new();
        for (k, v) in [
            ("alpha", false),
            ("antialias", false),
            ("depth", false),
            ("stencil", false),
            ("premultipliedAlpha", false),
            ("preserveDrawingBuffer", false),
        ] {
            js_sys::Reflect::set(&opts, &k.into(), &v.into()).ok()?;
        }
        let gl: Gl = canvas
            .get_context_with_context_options("webgl2", &opts)
            .ok()??
            .dyn_into()
            .ok()?;
        let program = link(&gl)?;
        gl.use_program(Some(&program));
        let sampler: Option<WebGlUniformLocation> = gl.get_uniform_location(&program, "frame");
        gl.uniform1i(sampler.as_ref(), 0);
        let overlay = gl.get_uniform_location(&program, "overlay");
        // The triangle needs a vertex array bound, even without attributes.
        let vao = gl.create_vertex_array()?;
        gl.bind_vertex_array(Some(&vao));
        let texture = gl.create_texture()?;
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&texture));
        for (p, v) in [
            (Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE),
            (Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameteri(Gl::TEXTURE_2D, p, v as i32);
        }
        gl.pixel_storei(Gl::UNPACK_ALIGNMENT, 4);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        Some(GlPresenter {
            canvas,
            gl,
            program,
            vao,
            texture,
            overlay,
            size: (0, 0),
        })
    }

    /// Clears the canvas (`width` x `height` pixels, resized when needed)
    /// and draws `pixels` (`w` x `h`, XRGB8888) into the rectangle (`x`,
    /// `y`, `dw`, `dh`) of it, counted from the top left; `smooth`
    /// interpolates the pixels (an HD frame being enlarged), otherwise
    /// each is a block (the original's pixels). With `under`, the view's
    /// renderer's picture is drawn first into its rectangle, and `pixels`
    /// over it as a premultiplied overlay.
    #[allow(clippy::too_many_arguments)]
    pub fn present(
        &mut self,
        (width, height): (u32, u32),
        pixels: &[u32],
        (w, h): (u32, u32),
        (x, y, dw, dh): (i32, i32, i32, i32),
        smooth: bool,
        under: Option<(&WebGlTexture, Rect)>,
    ) {
        if self.canvas.width() != width || self.canvas.height() != height {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
        }
        let gl = &self.gl;
        if gl.is_context_lost() {
            return;
        }
        // SAFETY: the words' memory read as bytes, for the upload only.
        let bytes =
            unsafe { std::slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), pixels.len() * 4) };
        // (the view's renderer shares the context)
        gl.use_program(Some(&self.program));
        gl.bind_vertex_array(Some(&self.vao));
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.texture));
        gl.pixel_storei(Gl::UNPACK_ALIGNMENT, 4);
        let filter = if smooth { Gl::LINEAR } else { Gl::NEAREST } as i32;
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, filter);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, filter);
        if self.size == (w, h) {
            let _ = gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                0,
                0,
                w as i32,
                h as i32,
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                Some(bytes),
            );
        } else {
            let _ = gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                Gl::RGBA8 as i32,
                w as i32,
                h as i32,
                0,
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                Some(bytes),
            );
            self.size = (w, h);
        }
        gl.viewport(0, 0, width as i32, height as i32);
        gl.clear(Gl::COLOR_BUFFER_BIT);
        // WebGL counts the viewport from the bottom.
        if let Some((view, (ux, uy, uw, uh))) = under {
            gl.bind_texture(Gl::TEXTURE_2D, Some(view));
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::LINEAR as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::LINEAR as i32);
            gl.uniform1i(self.overlay.as_ref(), 0);
            // (the frame buffer's row 0, the view's first, is the texture's
            // first row as for the uploaded frames)
            gl.viewport(ux, height as i32 - uy - uh, uw, uh);
            gl.draw_arrays(Gl::TRIANGLES, 0, 3);
            gl.bind_texture(Gl::TEXTURE_2D, Some(&self.texture));
            gl.enable(Gl::BLEND);
            gl.blend_func(Gl::ONE, Gl::ONE_MINUS_SRC_ALPHA);
            gl.uniform1i(self.overlay.as_ref(), 1);
        } else {
            gl.uniform1i(self.overlay.as_ref(), 0);
        }
        gl.viewport(x, height as i32 - y - dh, dw, dh);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
        gl.disable(Gl::BLEND);
    }
}

impl GlPresenter {
    /// The WebGL context, for the view's renderer.
    pub fn context(&self) -> &Gl {
        &self.gl
    }
}

fn shader(gl: &Gl, kind: u32, src: &str) -> Option<WebGlShader> {
    let s = gl.create_shader(kind)?;
    gl.shader_source(&s, src);
    gl.compile_shader(&s);
    if gl
        .get_shader_parameter(&s, Gl::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Some(s)
    } else {
        log::warn!(
            "WebGL shader: {}",
            gl.get_shader_info_log(&s).unwrap_or_default()
        );
        None
    }
}

fn link(gl: &Gl) -> Option<WebGlProgram> {
    link_with(gl, FRAGMENT)
}

fn link_with(gl: &Gl, fragment: &str) -> Option<WebGlProgram> {
    let p = gl.create_program()?;
    gl.attach_shader(&p, &shader(gl, Gl::VERTEX_SHADER, VERTEX)?);
    gl.attach_shader(&p, &shader(gl, Gl::FRAGMENT_SHADER, fragment)?);
    gl.link_program(&p);
    if gl
        .get_program_parameter(&p, Gl::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Some(p)
    } else {
        log::warn!(
            "WebGL program: {}",
            gl.get_program_info_log(&p).unwrap_or_default()
        );
        None
    }
}

/// The HD view drawn by the graphics card (see
/// [`atlantis_engine::hd::ViewRenderer`]): the engine's integer formulas
/// (`Warp::render_tiles`'s tile interpolation and the bilinear samplers)
/// run per pixel on an integer copy of the prepared panorama, into a
/// frame buffer read back into the engine's view, so that the pixels are
/// the engine's own, bit for bit.
const VIEW_FRAGMENT: &str = r#"#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;
precision highp isampler2D;
uniform usampler2D pano;  // RGBA8UI: the XRGB words' bytes B, G, R, X
uniform isampler2D grid;  // R32I: the grid past its 2 leading values
uniform int rs;           // values per grid row
uniform int tile;
uniform int doubled;
uniform int pano_w;
uniform int rows;
out vec4 color;

// Division by k > 0 rounding down (div_euclid), by 2 toward zero (Rust's
// i32 division): GLSL leaves negative division to the implementation.
int fdiv(int a, int k) { return a >= 0 ? a / k : -((-a - 1) / k) - 1; }
int tdiv2(int a) { return a >= 0 ? a / 2 : -((-a) / 2); }
// Wrapping arithmetic (int(uint) and uint(int) keep the bits).
int wsub(int a, int b) { return int(uint(a) - uint(b)); }
int wadd(int a, int b) { return int(uint(a) + uint(b)); }
int wmul(int a, int b) { return int(uint(a) * uint(b)); }
int g(int k) { k -= 2; return texelFetch(grid, ivec2(k % rs, k / rs), 0).r; }
uvec3 texel(uint c, uint r) { return texelFetch(pano, ivec2(int(c), int(r)), 0).rgb; }
uvec3 lerp(uvec3 a, uvec3 b, uint w) { return (a * (256u - w) + b * w) >> 8; }

void main() {
    int X = int(gl_FragCoord.x), Y = int(gl_FragCoord.y);
    int n = tile;
    int tx = X / n, i = X - tx * n, ty = Y / n, yy = Y - ty * n;
    int off = 2 + ty * rs + tx * 2;
    int x1 = fdiv(wsub(g(off + 2), g(off)), n);
    int y1 = fdiv(wsub(g(off + 3), g(off + 1)), n);
    int x1n = fdiv(wsub(g(off + rs + 2), g(off + rs)), n);
    int y1n = fdiv(wsub(g(off + rs + 3), g(off + rs + 1)), n);
    int dx1 = fdiv(wsub(x1n, x1), n);
    int dy1 = fdiv(wsub(y1n, y1), n * 32);
    y1 = fdiv(y1, 32);
    int dx2 = fdiv(wsub(g(off + rs), g(off)), n);
    int dy2 = fdiv(wsub(g(off + rs + 1), g(off + 1)), n * 32);
    int x2 = fdiv(wadd(wmul(g(off), 2), dx2), 2);
    int y2 = fdiv(wadd(wmul(fdiv(g(off + 1), 32), 2), dy2), 2);
    x1 = wadd(x1, wmul(dx1, yy));
    y1 = wadd(y1, wmul(dy1, yy));
    x2 = wadd(x2, wmul(dx2, yy));
    y2 = wadd(y2, wmul(dy2, yy));
    uint px = uint(wadd(wmul(x2, 2), x1)) * 16u + uint(i) * (uint(x1) * 32u);
    uint py = uint(tdiv2(wadd(wmul(y2, 2), y1))) + uint(i) * uint(y1);

    uint w = uint(pano_w), h = uint(rows);
    uint c0, fx, r0, fy;
    if (doubled != 0) {
        uint x = px - (1u << 19);
        c0 = x >> 20;
        fx = (x >> 12) & 0xffu;
        uint y = ((py & 0x1fffffu) << 1) - (1u << 10);
        r0 = y >> 11;
        fy = (y >> 3) & 0xffu;
    } else {
        uint x = px - (1u << 20);
        c0 = x >> 21;
        fx = (x >> 13) & 0xffu;
        uint y = py - (1u << 10);
        r0 = (y >> 11) & 0x3ffu;
        fy = (y >> 3) & 0xffu;
    }
    if (r0 >= h) {
        color = vec4(0.0);
        return;
    }
    uint c1 = (c0 + 1u) & (w - 1u);
    uint r1 = r0 + 1u < h ? r0 + 1u : r0;
    uvec3 top = lerp(texel(c0, r0), texel(c1, r0), fx);
    uvec3 bottom = lerp(texel(c0, r1), texel(c1, r1), fx);
    color = vec4(vec3(lerp(top, bottom, fy)), 0.0) / 255.0;
}
"#;

pub struct GlView {
    gl: Gl,
    program: WebGlProgram,
    vao: web_sys::WebGlVertexArrayObject,
    pano: WebGlTexture,
    grid: WebGlTexture,
    target: WebGlTexture,
    fbo: web_sys::WebGlFramebuffer,
    loc: [Option<WebGlUniformLocation>; 5],
    max_texture: usize,
    /// The panorama texture's size (None: to be sent whole).
    pano_size: Option<(usize, usize)>,
    target_size: (usize, usize),
    /// The drawing the frame buffer holds (`ViewJob::gen`).
    drawn_gen: Option<u64>,
}

impl GlView {
    pub fn new(gl: &Gl) -> Option<GlView> {
        let gl = gl.clone();
        let program = link_with(&gl, VIEW_FRAGMENT)?;
        let loc = ["rs", "tile", "doubled", "pano_w", "rows"]
            .map(|n| gl.get_uniform_location(&program, n));
        gl.use_program(Some(&program));
        gl.uniform1i(gl.get_uniform_location(&program, "pano").as_ref(), 0);
        gl.uniform1i(gl.get_uniform_location(&program, "grid").as_ref(), 1);
        let max_texture = gl.get_parameter(Gl::MAX_TEXTURE_SIZE).ok()?.as_f64()? as usize;
        let integer_texture = || -> Option<WebGlTexture> {
            let t = gl.create_texture()?;
            gl.bind_texture(Gl::TEXTURE_2D, Some(&t));
            for (p, v) in [
                (Gl::TEXTURE_MIN_FILTER, Gl::NEAREST),
                (Gl::TEXTURE_MAG_FILTER, Gl::NEAREST),
                (Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE),
                (Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameteri(Gl::TEXTURE_2D, p, v as i32);
            }
            Some(t)
        };
        let pano = integer_texture()?;
        let grid = integer_texture()?;
        let target = integer_texture()?;
        let fbo = gl.create_framebuffer()?;
        let vao = gl.create_vertex_array()?;
        Some(GlView {
            gl,
            program,
            vao,
            pano,
            grid,
            target,
            fbo,
            loc,
            max_texture,
            pano_size: None,
            target_size: (0, 0),
            drawn_gen: None,
        })
    }

    /// Sends the prepared panorama, or its tiles that changed.
    fn upload_pano(&mut self, job: &ViewJob<'_>) {
        use atlantis_engine::upscale::PanoChanges;
        let gl = &self.gl;
        let (w, h) = job.pano_size;
        // SAFETY: the words' memory read as bytes, for the upload only.
        let bytes = unsafe {
            std::slice::from_raw_parts(job.pano.as_ptr().cast::<u8>(), job.pano.len() * 4)
        };
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.pano));
        gl.pixel_storei(Gl::UNPACK_ALIGNMENT, 4);
        if self.pano_size != Some((w, h)) || job.changes == PanoChanges::All {
            let _ = gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                Gl::RGBA8UI as i32,
                w as i32,
                h as i32,
                0,
                Gl::RGBA_INTEGER,
                Gl::UNSIGNED_BYTE,
                Some(bytes),
            );
            self.pano_size = Some((w, h));
            return;
        }
        let PanoChanges::Tiles(tiles) = &job.changes else {
            return;
        };
        let t = job.pano_tile;
        gl.pixel_storei(Gl::UNPACK_ROW_LENGTH, w as i32);
        for &(tx, ty) in tiles {
            let (x0, y0) = (tx * t, ty * t);
            if x0 >= w || y0 >= h {
                continue;
            }
            gl.pixel_storei(Gl::UNPACK_SKIP_PIXELS, x0 as i32);
            gl.pixel_storei(Gl::UNPACK_SKIP_ROWS, y0 as i32);
            let _ = gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                x0 as i32,
                y0 as i32,
                t.min(w - x0) as i32,
                t.min(h - y0) as i32,
                Gl::RGBA_INTEGER,
                Gl::UNSIGNED_BYTE,
                Some(bytes),
            );
        }
        gl.pixel_storei(Gl::UNPACK_ROW_LENGTH, 0);
        gl.pixel_storei(Gl::UNPACK_SKIP_PIXELS, 0);
        gl.pixel_storei(Gl::UNPACK_SKIP_ROWS, 0);
    }
}

impl GlView {
    pub fn render(&mut self, job: ViewJob<'_>, out: Option<&mut [u32]>) -> bool {
        let (pw, ph) = job.pano_size;
        let rs = (job.cols + 1) * 2;
        let ok_sizes = pw.is_power_of_two()
            && pw.max(ph).max(job.width).max(job.height) <= self.max_texture
            && job.grid.len() >= 2 + 31 * rs
            && out
                .as_ref()
                .is_none_or(|o| o.len() >= job.width * job.height)
            && job.pano.len() >= pw * ph;
        if !ok_sizes || self.gl.is_context_lost() {
            self.pano_size = None;
            return false;
        }
        self.upload_pano(&job);
        let gl = &self.gl;
        // The grid, past its 2 leading values, as rows of `rs` values.
        gl.active_texture(Gl::TEXTURE1);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.grid));
        gl.pixel_storei(Gl::UNPACK_ALIGNMENT, 4);
        // SAFETY: a view of the grid's memory used at once, before any
        // allocation.
        let grid = unsafe { js_sys::Int32Array::view(&job.grid[2..2 + 31 * rs]) };
        let _ = gl
            .tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_array_buffer_view(
                Gl::TEXTURE_2D,
                0,
                Gl::R32I as i32,
                rs as i32,
                31,
                0,
                Gl::RED_INTEGER,
                Gl::INT,
                Some(&grid),
            );
        // The frame buffer the view is drawn into.
        let (w, h) = (job.width, job.height);
        gl.bind_framebuffer(Gl::FRAMEBUFFER, Some(&self.fbo));
        if self.target_size != (w, h) {
            gl.active_texture(Gl::TEXTURE2);
            gl.bind_texture(Gl::TEXTURE_2D, Some(&self.target));
            let _ = gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                Gl::RGBA8 as i32,
                w as i32,
                h as i32,
                0,
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                None,
            );
            gl.framebuffer_texture_2d(
                Gl::FRAMEBUFFER,
                Gl::COLOR_ATTACHMENT0,
                Gl::TEXTURE_2D,
                Some(&self.target),
                0,
            );
            // (checked once per size: the check waits for the card)
            if gl.check_framebuffer_status(Gl::FRAMEBUFFER) != Gl::FRAMEBUFFER_COMPLETE {
                gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
                self.target_size = (0, 0);
                self.pano_size = None;
                return false;
            }
            self.target_size = (w, h);
        }
        gl.use_program(Some(&self.program));
        gl.bind_vertex_array(Some(&self.vao));
        let [l_rs, l_tile, l_doubled, l_w, l_rows] = &self.loc;
        gl.uniform1i(l_rs.as_ref(), rs as i32);
        gl.uniform1i(l_tile.as_ref(), job.tile as i32);
        gl.uniform1i(l_doubled.as_ref(), job.doubled as i32);
        gl.uniform1i(l_w.as_ref(), pw as i32);
        gl.uniform1i(l_rows.as_ref(), ph as i32);
        gl.viewport(0, 0, w as i32, h as i32);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
        self.drawn_gen = Some(job.gen);
        let mut read = Ok(());
        if let Some(out) = out {
            // Row 0 of the frame buffer is the view's first row.
            // SAFETY: the words' memory written as bytes.
            let bytes =
                unsafe { std::slice::from_raw_parts_mut(out.as_mut_ptr().cast::<u8>(), w * h * 4) };
            read = gl.read_pixels_with_opt_u8_array(
                0,
                0,
                w as i32,
                h as i32,
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                Some(bytes),
            );
        }
        gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
        gl.active_texture(Gl::TEXTURE0);
        if read.is_err() {
            self.pano_size = None;
            self.drawn_gen = None;
            return false;
        }
        true
    }

    /// The view of drawing `gen`, as a texture, when the frame buffer
    /// holds it.
    pub fn picture(&self, gen: u64) -> Option<&WebGlTexture> {
        (self.drawn_gen == Some(gen)).then_some(&self.target)
    }
}

/// The view's renderer shared by the engine (which draws with it) and the
/// front end (which shows its picture).
pub struct SharedView(pub std::rc::Rc<std::cell::RefCell<GlView>>);

impl atlantis_engine::hd::ViewRenderer for SharedView {
    fn render(&mut self, job: ViewJob<'_>, out: Option<&mut [u32]>) -> bool {
        self.0.borrow_mut().render(job, out)
    }
}

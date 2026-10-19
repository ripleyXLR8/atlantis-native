//! The browser's presentation through WebGL 2: a frame (XRGB8888, as the
//! engine draws it) goes to the graphics card as a texture and is drawn
//! scaled into a rectangle of the canvas. It replaces the 2D canvas, where
//! every frame was converted to RGBA pixel by pixel, copied into an
//! `ImageData` and drawn, all on the one core the game runs on.

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

/// The XRGB8888 words arrive as the bytes B, G, R, X.
const FRAGMENT: &str = r#"#version 300 es
precision mediump float;
uniform sampler2D frame;
in vec2 uv;
out vec4 color;
void main() {
    color = vec4(texture(frame, uv).bgr, 1.0);
}
"#;

pub struct GlPresenter {
    canvas: HtmlCanvasElement,
    gl: Gl,
    texture: WebGlTexture,
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
            texture,
            size: (0, 0),
        })
    }

    /// Clears the canvas (`width` x `height` pixels, resized when needed)
    /// and draws `pixels` (`w` x `h`, XRGB8888) into the rectangle (`x`,
    /// `y`, `dw`, `dh`) of it, counted from the top left; `smooth`
    /// interpolates the pixels (an HD frame being enlarged), otherwise
    /// each is a block (the original's pixels).
    #[allow(clippy::too_many_arguments)]
    pub fn present(
        &mut self,
        (width, height): (u32, u32),
        pixels: &[u32],
        (w, h): (u32, u32),
        (x, y, dw, dh): (i32, i32, i32, i32),
        smooth: bool,
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
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.texture));
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
        gl.viewport(x, height as i32 - y - dh, dw, dh);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
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
    let p = gl.create_program()?;
    gl.attach_shader(&p, &shader(gl, Gl::VERTEX_SHADER, VERTEX)?);
    gl.attach_shader(&p, &shader(gl, Gl::FRAGMENT_SHADER, FRAGMENT)?);
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

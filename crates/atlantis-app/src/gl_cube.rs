//! Atlantis II's HD view drawn by the browser's graphics card (see
//! [`atlantis2_engine::hd::CubeRenderer`]): one core draws a view of
//! 960 x 720 in about 15 ms in WebAssembly, the card in a fraction of a
//! millisecond. The fragment shader is [`HdPanorama::render`]'s loop for
//! one pixel: the ray of its screen point turned by the camera's matrix,
//! the cube face it meets and the texel coordinates there
//! ([`cube_point`]), then the engine's integer bilinear sample of the
//! prepared atlas (in 64ths of a pixel, the weights summing to 4096). The
//! card computes the ray in single precision where the engine uses double:
//! a coordinate may then fall on the other side of a 64th, and that
//! pixel differ by a level or two (`atlantis-native/gpu-view` = `check`
//! in the page's storage logs the differences of each view).
//!
//! [`HdPanorama::render`]: atlantis2_engine::hd::HdPanorama::render
//! [`cube_point`]: atlantis2_engine::hd::cube_point

use atlantis2_engine::hd::{CubeJob, CubeRenderer, PanoChanges, CELL, PAD};
use web_sys::{WebGl2RenderingContext as Gl, WebGlProgram, WebGlTexture, WebGlUniformLocation};

use crate::gl_present::link_with;

const CUBE_FRAGMENT: &str = r#"#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;
uniform usampler2D pano;  // RGBA8UI: the XRGB words' bytes B, G, R, X
uniform float m[9];       // the camera's matrix
uniform vec4 proj;        // fx, fy, cx, cy
uniform float scale;      // tile / 16
uniform float side;       // the widescreen columns on each side
uniform int doubled;
out vec4 color;

uvec3 texel(int c, int r) { return texelFetch(pano, ivec2(c, r), 0).rgb; }

void main() {
    float X = floor(gl_FragCoord.x), Y = floor(gl_FragCoord.y);
    // The screen point of the pixel's centre, its camera ray at depth 1.
    float sx = (X - side + 0.5) / scale, sy = (Y + 0.5) / scale;
    float vx = (sx - proj.z) / proj.x, vy = (sy - proj.w) / proj.y;
    vec3 a = vec3(m[2] + vy * m[1], m[5] + vy * m[4], m[8] + vy * m[7]);
    vec3 d = vec3(a.x + vx * m[0], a.y + vx * m[3], a.z + vx * m[6]);
    vec3 ad = abs(d);
    int face;
    float u, v;
    if (ad.z >= ad.x && ad.z >= ad.y) {
        if (d.z > 0.0) { face = 0; u = d.x / ad.z; v = d.y / ad.z; }
        else { face = 1; u = -d.x / ad.z; v = d.y / ad.z; }
    } else if (ad.y >= ad.x) {
        if (d.y < 0.0) { face = 2; u = d.x / ad.y; v = d.z / ad.y; }
        else { face = 3; u = d.x / ad.y; v = -d.z / ad.y; }
    } else if (d.x < 0.0) { face = 4; u = d.z / ad.x; v = d.y / ad.x; }
    else { face = 5; u = -d.z / ad.x; v = d.y / ad.x; }
    if (isnan(u) || isnan(v) || isinf(u) || isinf(v)) {
        color = vec4(0.0);
        return;
    }
    float tu = 1.0 + 510.0 * (u + 1.0) / 2.0;
    float tv = 1.0 + 510.0 * (v + 1.0) / 2.0;
    // In 64ths of a prepared pixel, from the first one's centre.
    float k = doubled != 0 ? 2.0 : 1.0;
    float x0 = 64.0 * (k * float(PAD) - 0.5);
    float y0 = 64.0 * (k * float(face * CELL + PAD) - 0.5);
    uint ax = uint(tu * 64.0 * k + x0), ay = uint(tv * 64.0 * k + y0);
    int c0 = int(ax >> 6u), r0 = int(ay >> 6u);
    uint wx = ax & 63u, wy = ay & 63u;
    uvec3 acc = texel(c0, r0) * ((64u - wx) * (64u - wy))
        + texel(c0 + 1, r0) * (wx * (64u - wy))
        + texel(c0, r0 + 1) * ((64u - wx) * wy)
        + texel(c0 + 1, r0 + 1) * (wx * wy);
    color = vec4(vec3((acc >> 12u) & 0xffu), 0.0) / 255.0;
}
"#;

pub struct GlCube {
    gl: Gl,
    program: WebGlProgram,
    vao: web_sys::WebGlVertexArrayObject,
    pano: WebGlTexture,
    /// Two pictures, drawn in turn: the frame shown may be of the view
    /// before the last drawn.
    targets: [WebGlTexture; 2],
    fbo: web_sys::WebGlFramebuffer,
    loc: [Option<WebGlUniformLocation>; 5],
    max_texture: usize,
    /// The atlas's size on the card (None: to be sent whole).
    pano_size: Option<(usize, usize)>,
    target_sizes: [(usize, usize); 2],
    drawn_gens: [Option<u64>; 2],
    next: usize,
}

impl GlCube {
    pub fn new(gl: &Gl) -> Option<GlCube> {
        let gl = gl.clone();
        let source = CUBE_FRAGMENT.replacen(
            "out vec4 color;",
            &format!("out vec4 color;\nconst int PAD = {PAD};\nconst int CELL = {CELL};"),
            1,
        );
        let program = link_with(&gl, &source)?;
        let loc =
            ["m", "proj", "scale", "side", "doubled"].map(|n| gl.get_uniform_location(&program, n));
        gl.use_program(Some(&program));
        gl.uniform1i(gl.get_uniform_location(&program, "pano").as_ref(), 0);
        let max_texture = gl.get_parameter(Gl::MAX_TEXTURE_SIZE).ok()?.as_f64()? as usize;
        let texture = || -> Option<WebGlTexture> {
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
        let pano = texture()?;
        let targets = [texture()?, texture()?];
        let fbo = gl.create_framebuffer()?;
        let vao = gl.create_vertex_array()?;
        Some(GlCube {
            gl,
            program,
            vao,
            pano,
            targets,
            fbo,
            loc,
            max_texture,
            pano_size: None,
            target_sizes: [(0, 0); 2],
            drawn_gens: [None; 2],
            next: 0,
        })
    }

    /// Sends the atlas, or its tiles that changed.
    fn upload(&mut self, job: &CubeJob<'_>) {
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
                Some(&bytes[..w * h * 4]),
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

    /// The view of drawing `gen`, as a texture, when a picture holds it.
    pub fn picture(&self, gen: u64) -> Option<&WebGlTexture> {
        let i = self.drawn_gens.iter().position(|&g| g == Some(gen))?;
        Some(&self.targets[i])
    }
}

impl CubeRenderer for GlCube {
    fn render(&mut self, job: CubeJob<'_>, out: Option<&mut [u32]>) -> bool {
        let (pw, ph) = job.pano_size;
        let (w, h) = (job.width, job.height);
        let ok = pw.max(ph).max(w).max(h) <= self.max_texture
            && pw > 0
            && job.pano.len() >= pw * ph
            && out.as_ref().is_none_or(|o| o.len() >= w * h);
        if !ok || self.gl.is_context_lost() {
            self.pano_size = None;
            return false;
        }
        self.upload(&job);
        let gl = &self.gl;
        let i = self.next;
        gl.bind_framebuffer(Gl::FRAMEBUFFER, Some(&self.fbo));
        gl.framebuffer_texture_2d(
            Gl::FRAMEBUFFER,
            Gl::COLOR_ATTACHMENT0,
            Gl::TEXTURE_2D,
            Some(&self.targets[i]),
            0,
        );
        if self.target_sizes[i] != (w, h) {
            gl.active_texture(Gl::TEXTURE2);
            gl.bind_texture(Gl::TEXTURE_2D, Some(&self.targets[i]));
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
            // (checked once per size: the check waits for the card)
            if gl.check_framebuffer_status(Gl::FRAMEBUFFER) != Gl::FRAMEBUFFER_COMPLETE {
                gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
                gl.active_texture(Gl::TEXTURE0);
                self.target_sizes[i] = (0, 0);
                self.pano_size = None;
                return false;
            }
            self.target_sizes[i] = (w, h);
        }
        gl.use_program(Some(&self.program));
        gl.bind_vertex_array(Some(&self.vao));
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.pano));
        let [l_m, l_proj, l_scale, l_side, l_doubled] = &self.loc;
        gl.uniform1fv_with_f32_array(l_m.as_ref(), &job.matrix);
        let [fx, fy, cx, cy] = job.proj;
        gl.uniform4f(l_proj.as_ref(), fx, fy, cx, cy);
        gl.uniform1f(l_scale.as_ref(), job.tile as f32 / 16.0);
        gl.uniform1f(l_side.as_ref(), job.side as f32);
        gl.uniform1i(l_doubled.as_ref(), i32::from(job.doubled));
        gl.viewport(0, 0, w as i32, h as i32);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
        self.drawn_gens[i] = Some(job.gen);
        self.next = 1 - i;
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
        if read.is_err() {
            self.pano_size = None;
            self.drawn_gens[i] = None;
            return false;
        }
        true
    }
}

/// The renderer shared by the engine (which draws with it) and the front
/// end (which shows its picture).
pub struct SharedCube(pub std::rc::Rc<std::cell::RefCell<GlCube>>);

impl CubeRenderer for SharedCube {
    fn render(&mut self, job: CubeJob<'_>, out: Option<&mut [u32]>) -> bool {
        self.0.borrow_mut().render(job, out)
    }
}

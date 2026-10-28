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
//!
//! A dialog line's HD picture is composed there too
//! ([`CubeRenderer::compose_dialog`]): the first game's
//! `compose_dialog_plan` per output pixel, over the card's view, with the
//! engine's integer arithmetic (the same pixels; `check` compares them).
//!
//! [`HdPanorama::render`]: atlantis2_engine::hd::HdPanorama::render
//! [`cube_point`]: atlantis2_engine::hd::cube_point

use atlantis2_engine::hd::{CubeJob, CubeRenderer, DialogJob, PanoChanges, CELL, PAD};
use atlantis_engine::upscale::Axis;
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

/// The dialog's picture (`compose_dialog_plan` for one output pixel).
const DIALOG_FRAGMENT: &str = r#"#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;
precision highp isampler2D;
precision highp sampler2D;
uniform sampler2D view;     // RGBA8: the card's view (bytes B, G, R)
uniform usampler2D comp;    // R16UI 640x480: the composite
uniform usampler2D over;    // R16UI 640x480: the composite with the overlay
uniform usampler2D chars;   // R8UI 640x480: the character's pixels
uniform usampler2D foot;    // R8UI 640x480: the head's footprint
uniform usampler2D head;    // R16UI: the HD head in its box
uniform usampler2D hmask;   // R8UI: its mask
uniform isampler2D xs;      // per column: 640x480 x, bilinear x0, x1, weight
uniform isampler2D ys;      // per row: 640x480 y, bilinear y0, y1, weight
uniform int side;
uniform int shift;
uniform int w;
uniform int has_over;
uniform int soft;           // the body upscaled bilinearly (`smooth` is a GLSL word)
uniform int has_head;
uniform ivec4 hbox;         // the head's box: x0, y0, x1, y1
out vec4 color;

uvec3 lut(uint p) {
    uint r = (p >> 11) & 31u, g = (p >> 5) & 63u, b = p & 31u;
    return uvec3((b << 3) | (b >> 2), (g << 2) | (g >> 4), (r << 3) | (r >> 2));
}
uvec3 lerp(uvec3 a, uvec3 b, uint t) { return (a * (256u - t) + b * t) >> 8; }
uvec3 ov(int x, int y) { return lut(texelFetch(over, ivec2(x, y), 0).r); }
uvec3 bil(ivec4 xv, ivec4 yv) {
    uvec3 top = lerp(ov(xv.y, yv.y), ov(xv.z, yv.y), uint(xv.w));
    uvec3 bottom = lerp(ov(xv.y, yv.z), ov(xv.z, yv.z), uint(xv.w));
    return lerp(top, bottom, uint(yv.w));
}

void main() {
    ivec2 o = ivec2(gl_FragCoord.xy);
    uvec3 v = uvec3(round(texelFetch(view, o, 0).rgb * 255.0));
    int x = o.x - side - shift;
    if (x >= 0 && x < w) {
        ivec4 xv = texelFetch(xs, ivec2(x, 0), 0);
        ivec4 yv = texelFetch(ys, ivec2(o.y, 0), 0);
        ivec2 s = ivec2(xv.x, yv.x);
        uint c = texelFetch(comp, s, 0).r;
        if (texelFetch(chars, s, 0).r != 0u) {
            v = soft != 0 ? bil(xv, yv) : lut(c);
        }
        if (has_head != 0 && x >= hbox.x && x < hbox.z && o.y >= hbox.y && o.y < hbox.w) {
            ivec2 h = ivec2(x - hbox.x, o.y - hbox.y);
            if (texelFetch(hmask, h, 0).r != 0u && texelFetch(foot, s, 0).r != 0u) {
                v = lut(texelFetch(head, h, 0).r);
            }
        }
        if (has_over != 0) {
            uint p = texelFetch(over, s, 0).r;
            if (p != c) {
                v = soft != 0 ? bil(xv, yv) : lut(p);
            }
        }
    }
    color = vec4(vec3(v), 0.0) / 255.0;
}
"#;

/// The dialog's pictures and what they are composed from.
struct GlDialog {
    program: WebGlProgram,
    loc: [Option<WebGlUniformLocation>; 7],
    /// comp, over, chars, foot, head, hmask, xs, ys.
    inputs: [WebGlTexture; 8],
    targets: [WebGlTexture; 2],
    target_sizes: [(usize, usize); 2],
    drawn_gens: [Option<u64>; 2],
    next: usize,
    /// The tables' key: the picture's width, height and tile.
    tables: Option<(usize, usize, usize)>,
}

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
    dialog: Option<GlDialog>,
}

/// An integer texture of `w` x `h` texels from `data` (`internal`,
/// `format` and `kind` as WebGL names them), on texture unit `unit`.
#[allow(clippy::too_many_arguments)]
fn send(
    gl: &Gl,
    unit: u32,
    t: &WebGlTexture,
    internal: u32,
    format: u32,
    kind: u32,
    (w, h): (usize, usize),
    data: &js_sys::Object,
) {
    gl.active_texture(Gl::TEXTURE0 + unit);
    gl.bind_texture(Gl::TEXTURE_2D, Some(t));
    gl.pixel_storei(Gl::UNPACK_ALIGNMENT, 1);
    let _ = gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_array_buffer_view(
        Gl::TEXTURE_2D,
        0,
        internal as i32,
        w as i32,
        h as i32,
        0,
        format,
        kind,
        Some(data),
    );
    gl.pixel_storei(Gl::UNPACK_ALIGNMENT, 4);
}

/// The texture units 1 to 9 let go after a dialog's composition: the
/// first game's compositor samples textures of its own kinds there, and a
/// float sampler over an integer texture is an error.
fn release_units(gl: &Gl) {
    for unit in 1..=9 {
        gl.active_texture(Gl::TEXTURE0 + unit);
        gl.bind_texture(Gl::TEXTURE_2D, None);
    }
    gl.active_texture(Gl::TEXTURE0);
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
        // The dialogs' composition (without it, the engine composes them).
        let dialog = (|| {
            let program = link_with(&gl, DIALOG_FRAGMENT)?;
            gl.use_program(Some(&program));
            for (i, name) in [
                "view", "comp", "over", "chars", "foot", "head", "hmask", "xs", "ys",
            ]
            .iter()
            .enumerate()
            {
                gl.uniform1i(gl.get_uniform_location(&program, name).as_ref(), i as i32);
            }
            let loc = ["side", "shift", "w", "has_over", "soft", "has_head", "hbox"]
                .map(|n| gl.get_uniform_location(&program, n));
            Some(GlDialog {
                program,
                loc,
                inputs: [
                    texture()?,
                    texture()?,
                    texture()?,
                    texture()?,
                    texture()?,
                    texture()?,
                    texture()?,
                    texture()?,
                ],
                targets: [texture()?, texture()?],
                target_sizes: [(0, 0); 2],
                drawn_gens: [None; 2],
                next: 0,
                tables: None,
            })
        })();
        gl.use_program(Some(&program));
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
            dialog,
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

    /// The view or dialog picture of drawing `gen`, as a texture, when a
    /// picture holds it.
    pub fn picture(&self, gen: u64) -> Option<&WebGlTexture> {
        if let Some(i) = self.drawn_gens.iter().position(|&g| g == Some(gen)) {
            return Some(&self.targets[i]);
        }
        let d = self.dialog.as_ref()?;
        let i = d.drawn_gens.iter().position(|&g| g == Some(gen))?;
        Some(&d.targets[i])
    }

    /// A frame buffer on `target`, sized `w` x `h` (made when its size
    /// changed); false when the card refuses it.
    fn bind_target(
        gl: &Gl,
        fbo: &web_sys::WebGlFramebuffer,
        target: &WebGlTexture,
        size: &mut (usize, usize),
        (w, h): (usize, usize),
    ) -> bool {
        gl.bind_framebuffer(Gl::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            Gl::FRAMEBUFFER,
            Gl::COLOR_ATTACHMENT0,
            Gl::TEXTURE_2D,
            Some(target),
            0,
        );
        if *size != (w, h) {
            gl.active_texture(Gl::TEXTURE0 + 9);
            gl.bind_texture(Gl::TEXTURE_2D, Some(target));
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
                *size = (0, 0);
                return false;
            }
            *size = (w, h);
        }
        true
    }

    /// [`CubeRenderer::compose_dialog`].
    fn dialog(&mut self, job: DialogJob<'_>, out: Option<&mut [u32]>) -> bool {
        let p = job.plan;
        let (lw, h, w) = (p.layer_width, p.height, p.width);
        let Some(view) = self
            .drawn_gens
            .iter()
            .position(|&g| g == Some(job.view_gen))
            .map(|i| self.targets[i].clone())
        else {
            return false;
        };
        let gl = self.gl.clone();
        let Some(d) = self.dialog.as_mut() else {
            return false;
        };
        let n = 640 * 480;
        if gl.is_context_lost()
            || lw.max(h) > self.max_texture
            || p.composite.len() < n
            || p.over.len() < n
            || p.chars.len() < n
            || out.as_ref().is_some_and(|o| o.len() < lw * h)
        {
            return false;
        }
        let [comp, over, chars, foot, head, hmask, xs, ys] = &d.inputs;
        // SAFETY (each view below): a view of memory used at once, before
        // any allocation.
        let u16s = |px: &[u16]| unsafe { js_sys::Uint16Array::view(px) };
        let u8s = |px: &[u8]| unsafe { js_sys::Uint8Array::view(px) };
        let r16 = |unit, t, px: &[u16], size| {
            send(
                &gl,
                unit,
                t,
                Gl::R16UI,
                Gl::RED_INTEGER,
                Gl::UNSIGNED_SHORT,
                size,
                &u16s(px),
            )
        };
        let r8 = |unit, t, px: &[u8], size| {
            send(
                &gl,
                unit,
                t,
                Gl::R8UI,
                Gl::RED_INTEGER,
                Gl::UNSIGNED_BYTE,
                size,
                &u8s(px),
            )
        };
        r16(1, comp, &p.composite[..n], (640, 480));
        r16(2, over, &p.over[..n], (640, 480));
        r8(3, chars, &p.chars[..n], (640, 480));
        let foot_px = p.footprint.as_deref().filter(|f| f.len() >= n);
        r8(
            4,
            foot,
            foot_px.map_or(&[0u8][..], |f| &f[..n]),
            match foot_px {
                Some(_) => (640, 480),
                None => (1, 1),
            },
        );
        // The HD head: the box of its mask.
        let mut hbox = [0i32; 4];
        let mut has_head = false;
        if let (Some((hp, hm)), Some(_)) = (p.head, foot_px) {
            if hp.len() >= w * h && hm.len() >= w * h {
                let rows: Vec<usize> = (0..h)
                    .filter(|&y| hm[y * w..][..w].iter().any(|&m| m != 0))
                    .collect();
                if let (Some(&y0), Some(&y1)) = (rows.first(), rows.last()) {
                    let mut x0 = w;
                    let mut x1 = 0;
                    for &y in &rows {
                        let r = &hm[y * w..][..w];
                        if let Some(a) = r.iter().position(|&m| m != 0) {
                            x0 = x0.min(a);
                        }
                        if let Some(b) = r.iter().rposition(|&m| m != 0) {
                            x1 = x1.max(b + 1);
                        }
                    }
                    let (bw, bh) = (x1 - x0, y1 + 1 - y0);
                    let mut px = Vec::with_capacity(bw * bh);
                    let mut mk = Vec::with_capacity(bw * bh);
                    for y in y0..=y1 {
                        px.extend_from_slice(&hp[y * w + x0..y * w + x1]);
                        mk.extend_from_slice(&hm[y * w + x0..y * w + x1]);
                    }
                    r16(5, head, &px, (bw, bh));
                    r8(6, hmask, &mk, (bw, bh));
                    hbox = [x0 as i32, y0 as i32, x1 as i32, (y1 + 1) as i32];
                    has_head = true;
                }
            }
        }
        // The column and row tables (640x480 pixel, bilinear pair, weight).
        let key = (w, h, p.tile);
        if d.tables != Some(key) {
            let (ax, ay) = (Axis::new(640, w), Axis::new(480, h));
            let cols = atlantis_engine::hd::dialog_map(w, p.tile);
            let rows = atlantis_engine::hd::dialog_map(h, p.tile);
            let table = |map: &[usize], axis: &Axis| -> Vec<i32> {
                map.iter()
                    .zip(&axis.lin)
                    .flat_map(|(&n, &(a, b, t))| [n as i32, a as i32, b as i32, t as i32])
                    .collect()
            };
            for (unit, t, v, len) in [(7, xs, table(&cols, &ax), w), (8, ys, table(&rows, &ay), h)]
            {
                // SAFETY: a view of the table used at once.
                let view = unsafe { js_sys::Int32Array::view(&v) };
                send(
                    &gl,
                    unit,
                    t,
                    Gl::RGBA32I,
                    Gl::RGBA_INTEGER,
                    Gl::INT,
                    (len, 1),
                    &view,
                );
            }
            d.tables = Some(key);
        } else {
            for (unit, t) in [(7, xs), (8, ys)] {
                gl.active_texture(Gl::TEXTURE0 + unit);
                gl.bind_texture(Gl::TEXTURE_2D, Some(t));
            }
        }
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&view));
        for (unit, t) in [
            (1, comp),
            (2, over),
            (3, chars),
            (4, foot),
            (5, head),
            (6, hmask),
        ] {
            gl.active_texture(Gl::TEXTURE0 + unit);
            gl.bind_texture(Gl::TEXTURE_2D, Some(t));
        }
        let i = d.next;
        if !Self::bind_target(
            &gl,
            &self.fbo,
            &d.targets[i],
            &mut d.target_sizes[i],
            (lw, h),
        ) {
            release_units(&gl);
            return false;
        }
        gl.use_program(Some(&d.program));
        gl.bind_vertex_array(Some(&self.vao));
        let [l_side, l_shift, l_w, l_over, l_smooth, l_head, l_box] = &d.loc;
        gl.uniform1i(l_side.as_ref(), p.side as i32);
        gl.uniform1i(l_shift.as_ref(), p.shift as i32);
        gl.uniform1i(l_w.as_ref(), w as i32);
        gl.uniform1i(l_over.as_ref(), i32::from(p.has_over));
        gl.uniform1i(l_smooth.as_ref(), i32::from(p.smooth));
        gl.uniform1i(l_head.as_ref(), i32::from(has_head));
        gl.uniform4i(l_box.as_ref(), hbox[0], hbox[1], hbox[2], hbox[3]);
        gl.viewport(0, 0, lw as i32, h as i32);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
        d.drawn_gens[i] = Some(job.gen);
        d.next = 1 - i;
        let mut read = Ok(());
        if let Some(out) = out {
            // SAFETY: the words' memory written as bytes.
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(out.as_mut_ptr().cast::<u8>(), lw * h * 4)
            };
            read = gl.read_pixels_with_opt_u8_array(
                0,
                0,
                lw as i32,
                h as i32,
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                Some(bytes),
            );
        }
        gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
        release_units(&gl);
        if read.is_err() {
            d.drawn_gens[i] = None;
            return false;
        }
        true
    }
}

impl CubeRenderer for GlCube {
    fn composes_dialogs(&self) -> bool {
        self.dialog.is_some()
    }

    fn compose_dialog(&mut self, job: DialogJob<'_>, out: Option<&mut [u32]>) -> bool {
        self.dialog(job, out)
    }

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

    fn composes_dialogs(&self) -> bool {
        self.0.borrow().composes_dialogs()
    }

    fn compose_dialog(&mut self, job: DialogJob<'_>, out: Option<&mut [u32]>) -> bool {
        self.0.borrow_mut().compose_dialog(job, out)
    }
}

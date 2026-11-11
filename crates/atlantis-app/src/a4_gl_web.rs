//! The Atlantis Evolution viewer's canvas drawn by the graphics card in the
//! browser: a WebGL 2 context on the window's canvas (no antialiasing, no
//! alpha: the original's pixel format), `atlantis4_gl`'s renderer on it
//! (its shaders switch to GLSL ES 3.00). The page falls back to the
//! processor's renderer on a 2D canvas when the browser has no WebGL 2.
//! The desktop's counterpart is `a4_gl.rs` (glutin).

use atlantis4_gl::Renderer;
use wasm_bindgen::JsCast;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

/// The renderer on the canvas's context.
pub struct GlView {
    pub renderer: Renderer,
    canvas: web_sys::HtmlCanvasElement,
}

/// The window, and the card's view of it unless it could not be made
/// (then why).
pub type Made = (Option<Window>, Result<GlView, String>);

/// A window of `attrs` (its canvas added to the page) and the renderer on
/// its WebGL 2 context.
pub fn create(el: &ActiveEventLoop, attrs: WindowAttributes) -> Made {
    let window = match el.create_window(attrs) {
        Ok(w) => w,
        Err(e) => return (None, Err(e.to_string())),
    };
    let view = make(&window);
    (Some(window), view)
}

fn make(window: &Window) -> Result<GlView, String> {
    use winit::platform::web::WindowExtWebSys;
    let canvas = window.canvas().ok_or("no canvas")?;
    let opts = js_sys::Object::new();
    for (k, v) in [
        ("antialias", false),
        ("alpha", false),
        ("depth", false),
        ("stencil", false),
        ("premultipliedAlpha", false),
        // The check mode reads the renderer's own framebuffer, never the
        // canvas: the drawing buffer need not be kept.
        ("preserveDrawingBuffer", false),
    ] {
        let _ = js_sys::Reflect::set(&opts, &k.into(), &v.into());
    }
    let ctx = canvas
        .get_context_with_context_options("webgl2", &opts)
        .map_err(|e| format!("{e:?}"))?
        .ok_or("no WebGL 2")?
        .dyn_into::<web_sys::WebGl2RenderingContext>()
        .map_err(|_| "not a WebGL 2 context".to_string())?;
    let renderer = Renderer::from_webgl2(ctx).map_err(|e| e.to_string())?;
    Ok(GlView { renderer, canvas })
}

impl GlView {
    /// The drawing buffer is sized at the presentation.
    pub fn resize(&self, _w: u32, _h: u32) {}

    /// The last frame to the canvas, its drawing buffer made the window's
    /// size first (winit sizes the canvas on the page, not its buffer);
    /// the browser shows it at its next composition (no buffer swap).
    pub fn present(&self, w: u32, h: u32) -> Result<(), String> {
        if self.canvas.width() != w || self.canvas.height() != h {
            self.canvas.set_width(w);
            self.canvas.set_height(h);
        }
        self.renderer.present(None, (w, h));
        Ok(())
    }
}

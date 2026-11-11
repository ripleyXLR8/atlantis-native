//! The Atlantis Evolution viewer's window drawn by the graphics card: an
//! OpenGL 3.3 core context (OpenGL ES 3.0 where only that exists) on the
//! window through glutin, `atlantis4_gl`'s renderer on it. The viewer
//! falls back to the processor's renderer when none can be made.

use std::num::NonZeroU32;

use atlantis4_gl::Renderer;
use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, Version,
};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowAttributes};

/// The window's context and surface and the renderer on them.
pub struct GlView {
    pub renderer: Renderer,
    surface: Surface<WindowSurface>,
    ctx: PossiblyCurrentContext,
}

/// The window, and the card's view of it unless it could not be made
/// (then why).
pub type Made = (Option<Window>, Result<GlView, String>);

/// The configuration with the fewest samples and no alpha (the original's
/// pixel format has no multisampling; an alpha channel could make the
/// window see-through on some desktops).
fn pick(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    configs
        .min_by_key(|c| (c.num_samples(), c.alpha_size()))
        .expect("at least one configuration")
}

/// A window of `attrs` with a context and the renderer.
pub fn create(el: &ActiveEventLoop, attrs: WindowAttributes) -> Made {
    let template = ConfigTemplateBuilder::new();
    let (window, config) = match DisplayBuilder::new()
        .with_window_attributes(Some(attrs))
        .build(el, template, pick)
    {
        Ok(x) => x,
        Err(e) => return (None, Err(e.to_string())),
    };
    let Some(window) = window else {
        return (None, Err("no window".into()));
    };
    let view = make(&window, &config);
    (Some(window), view)
}

fn make(window: &Window, config: &Config) -> Result<GlView, String> {
    let display = config.display();
    let raw = window.window_handle().map_err(|e| e.to_string())?.as_raw();
    let mut ctx = None;
    let mut why = String::new();
    for api in [
        ContextApi::OpenGl(Some(Version::new(3, 3))),
        ContextApi::Gles(Some(Version::new(3, 0))),
    ] {
        let attrs = ContextAttributesBuilder::new()
            .with_context_api(api)
            .build(Some(raw));
        // SAFETY: the configuration is the display's, the handle the
        // window's, which outlives the context (both kept by the viewer).
        match unsafe { display.create_context(config, &attrs) } {
            Ok(c) => {
                ctx = Some(c);
                break;
            }
            Err(e) => why = e.to_string(),
        }
    }
    let ctx = ctx.ok_or(why)?;
    let attrs = window
        .build_surface_attributes(Default::default())
        .map_err(|e| e.to_string())?;
    // SAFETY: as above.
    let surface =
        unsafe { display.create_window_surface(config, &attrs) }.map_err(|e| e.to_string())?;
    let ctx = ctx.make_current(&surface).map_err(|e| e.to_string())?;
    // The viewer paces its own frames (60 updates a second, a draw when
    // the picture changed), as with the processor's renderer.
    let _ = surface.set_swap_interval(&ctx, SwapInterval::DontWait);
    // SAFETY: the context is current on this thread.
    let gl =
        unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s).cast()) };
    let renderer = Renderer::new(gl).map_err(|e| e.to_string())?;
    Ok(GlView {
        renderer,
        surface,
        ctx,
    })
}

impl GlView {
    /// The surface follows the window's size.
    pub fn resize(&self, w: u32, h: u32) {
        if let (Some(w), Some(h)) = (NonZeroU32::new(w), NonZeroU32::new(h)) {
            self.surface.resize(&self.ctx, w, h);
        }
    }

    /// The last frame to the window.
    pub fn present(&self, w: u32, h: u32) -> Result<(), String> {
        self.renderer.present(None, (w, h));
        self.surface
            .swap_buffers(&self.ctx)
            .map_err(|e| e.to_string())
    }
}

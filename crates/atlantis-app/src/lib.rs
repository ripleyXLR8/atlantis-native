//! Atlantis Native: the desktop and web front end. It owns the window, turns
//! input into engine events and shows the engine's 640x480 frames scaled to
//! the window.

use std::num::NonZeroU32;
use std::rc::Rc;

use atlantis_data::Game;
use atlantis_engine::warp::{Warp, SCREEN_H, SCREEN_W};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

mod audio_out;
#[cfg(target_arch = "wasm32")]
mod web;

const W: usize = SCREEN_W;
const H: usize = SCREEN_H;

/// Phase 0: one panorama, turned with the mouse (left button held) or the
/// arrow keys.
pub struct Viewer {
    pano: Vec<u16>,
    warp: Warp,
    dragging: bool,
    last: Option<(f64, f64)>,
}

pub struct App {
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    /// The engine's frame, RGB565.
    frame: Vec<u16>,
    viewer: Option<Viewer>,
}

impl App {
    pub fn new(viewer: Option<Viewer>) -> App {
        // Without a panorama: a test pattern.
        let frame = (0..W * H)
            .map(|i| (((i % W) * 31 / W) << 11 | ((i / W) * 63 / H) << 5) as u16)
            .collect();
        let mut app = App {
            window: None,
            surface: None,
            frame,
            viewer,
        };
        app.render();
        app
    }

    fn render(&mut self) {
        if let Some(v) = &mut self.viewer {
            v.warp.render(&v.pano, &mut self.frame);
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn turn(&mut self, dalpha: f32, dbeta: f32) {
        if let Some(v) = &mut self.viewer {
            let (a, b) = (v.warp.alpha(), v.warp.beta());
            v.warp.set_alpha(a + dalpha);
            v.warp.set_beta(b + dbeta);
            self.render();
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.inner_size();
        let (Some(sw), Some(sh)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        surface.resize(sw, sh).unwrap();
        let mut buf = surface.buffer_mut().unwrap();
        let (sw, sh) = (size.width as usize, size.height as usize);
        // Integer-free nearest scaling keeping the 4:3 aspect, black bars.
        let scale = (sw as f32 / W as f32).min(sh as f32 / H as f32);
        let (dw, dh) = ((W as f32 * scale) as usize, (H as f32 * scale) as usize);
        let (ox, oy) = ((sw - dw) / 2, (sh - dh) / 2);
        buf.fill(0);
        for y in 0..dh {
            let src_row = &self.frame[(y * H / dh) * W..][..W];
            let dst_row = &mut buf[(oy + y) * sw + ox..][..dw];
            for (x, d) in dst_row.iter_mut().enumerate() {
                let p = u32::from(src_row[x * W / dw]);
                let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
                *d = (r << 3 | r >> 2) << 16 | (g << 2 | g >> 4) << 8 | (b << 3 | b >> 2);
            }
        }
        buf.present().unwrap();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Atlantis Native")
            .with_inner_size(LogicalSize::new(W as f64 * 1.5, H as f64 * 1.5));
        #[cfg(target_arch = "wasm32")]
        let attrs = {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs.with_append(true)
        };
        let window = Rc::new(el.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Resized(_) => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(v) = &mut self.viewer {
                    v.dragging = state == ElementState::Pressed;
                    v.last = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let mut delta = None;
                if let Some(v) = &mut self.viewer {
                    if v.dragging {
                        if let Some((x, y)) = v.last {
                            delta = Some((position.x - x, position.y - y));
                        }
                        v.last = Some((position.x, position.y));
                    }
                }
                if let Some((dx, dy)) = delta {
                    self.turn(-dx as f32 * 0.004, -dy as f32 * 0.004);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::ArrowLeft) => self.turn(0.05, 0.0),
                    Key::Named(NamedKey::ArrowRight) => self.turn(-0.05, 0.0),
                    Key::Named(NamedKey::ArrowUp) => self.turn(0.0, 0.05),
                    Key::Named(NamedKey::ArrowDown) => self.turn(0.0, -0.05),
                    Key::Named(NamedKey::Escape) => el.exit(),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// The panorama `CYCLO\\<name>.HNM` of a game, for the phase-0 viewer.
pub fn panorama_name(name: &str) -> String {
    format!("CYCLO\\{}.HNM", name.to_ascii_uppercase())
}

pub fn open_viewer(game: &Game, name: &str) -> Result<Viewer, String> {
    log::info!(
        "{}: {} archive members",
        game.exe.build.name,
        game.big.len()
    );
    let name = panorama_name(name);
    let hnm = game.big.read(&name).ok_or(format!("{name} not found"))?;
    let pano = atlantis_codecs::decode_panorama(&hnm).map_err(|e| format!("{name}: {e}"))?;
    log::info!("{name}: {}x{}", pano.width, pano.height);
    Ok(Viewer {
        pano: pano.pixels,
        warp: Warp::new(),
        dragging: false,
        last: None,
    })
}

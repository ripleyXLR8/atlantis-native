//! A development viewer of *Atlantis Evolution*'s warp view (cargo
//! feature `atlantis4`, off by default and in every release: the game is
//! not offered until its port is complete).
//!
//! `atlantis-native --atlantis4-view <game folder> [node]` opens the node
//! (by default the first of a new game) in a window of 1365 × 768 drawn
//! on the processor by `atlantis4_engine::render`: dragging with the left
//! button turns the view (0.5° per pixel, the original's mouse look), a
//! left click of less than 20 pixels logs what it hits; an exit's click
//! opens the node it leads to. No scripts yet: the view shows the
//! background only.

use std::num::NonZeroU32;
use std::path::Path;
use std::rc::Rc;

use atlantis4_data::{A4Files, WarpPath};
use atlantis4_engine::camera::{fov_for_height, Camera};
use atlantis4_engine::node::{first_warp_path, Node, FIRST_WARP_SCRIPT};
use atlantis4_engine::pick::{click, is_click, Target};
use atlantis4_engine::render::{render, Weights};
use atlantis4_engine::View;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const WIDTH: u32 = 1365;
const HEIGHT: u32 = 768;

struct Viewer {
    files: A4Files,
    node: Node,
    view: View,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    pointer: (f64, f64),
    down: Option<(f64, f64)>,
    last: (f64, f64),
    size: (u32, u32),
}

impl Viewer {
    fn draw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let (w, h) = self.size;
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return;
        };
        if surface.resize(nw, nh).is_err() {
            return;
        }
        let cam = Camera::new(fov_for_height(h), w, h, self.view.yaw, self.view.pitch);
        if let Ok(mut buf) = surface.buffer_mut() {
            render(&cam, &self.node.faces, &mut buf, Weights::Exact);
            window.pre_present_notify();
            let _ = buf.present();
        }
    }

    fn release(&mut self) {
        let Some(down) = self.down.take() else { return };
        let up = self.pointer;
        if !is_click((down.0 as f32, down.1 as f32), (up.0 as f32, up.1 as f32)) {
            return;
        }
        let (w, h) = self.size;
        let cam = Camera::new(fov_for_height(h), w, h, self.view.yaw, self.view.pitch);
        let hit = click(&cam, up.0 as i32, up.1 as i32, &[], &self.node.exits);
        log::info!("click at {up:?}: {hit:?}");
        if let Some(Target::Exit(to)) = hit {
            match WarpPath::parse(&to).map(|p| Node::load(&self.files, &p)) {
                Some(Ok(n)) => {
                    // `ChangeWarp` keeps the angles and resets the limits.
                    self.node = n;
                    self.view.reset_limits();
                }
                Some(Err(e)) => log::warn!("{to}: {e}"),
                None => log::warn!("{to}: not a node"),
            }
        }
    }
}

impl ApplicationHandler for Viewer {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Atlantis Evolution (warp view) - Atlantis Native")
            .with_inner_size(PhysicalSize::new(WIDTH, HEIGHT));
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                log::error!("no window: {e}");
                el.exit();
                return;
            }
        };
        match softbuffer::Context::new(window.clone())
            .and_then(|c| softbuffer::Surface::new(&c, window.clone()))
        {
            Ok(s) => self.surface = Some(s),
            Err(e) => {
                log::error!("no picture in the window: {e}");
                el.exit();
                return;
            }
        }
        let s = window.inner_size();
        self.size = (s.width, s.height);
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                self.size = (s.width, s.height);
                self.request();
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = (position.x, position.y);
                if self.down.is_some() {
                    // Input pixels, y up.
                    let dx = position.x - self.last.0;
                    let dy_up = self.last.1 - position.y;
                    self.view.drag(dx as f32, dy_up as f32, false);
                    self.request();
                }
                self.last = self.pointer;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => {
                    self.down = Some(self.pointer);
                    self.last = self.pointer;
                }
                ElementState::Released => {
                    self.release();
                    self.request();
                }
            },
            _ => {}
        }
    }
}

impl Viewer {
    fn request(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

/// Opens the viewer on the game at `dir`, at `node` or the first node.
pub fn run_view(dir: &Path, node: Option<&str>) -> Result<(), String> {
    let (files, build) = A4Files::open_dir(dir).map_err(|e| e.to_string())?;
    log::info!("Atlantis Evolution {}", build.id);
    let name = match node {
        Some(n) => n.to_string(),
        None => files
            .read(FIRST_WARP_SCRIPT)
            .and_then(|s| first_warp_path(&s))
            .ok_or("no firstWarpPath")?,
    };
    let path = WarpPath::parse(&name).ok_or_else(|| format!("{name}: not a node"))?;
    let node = Node::load(&files, &path).map_err(|e| e.to_string())?;
    let mut app = Viewer {
        files,
        node,
        view: View::default(),
        window: None,
        surface: None,
        pointer: (0.0, 0.0),
        down: None,
        last: (0.0, 0.0),
        size: (WIDTH, HEIGHT),
    };
    let el = EventLoop::new().map_err(|e| format!("No window can be opened: {e}"))?;
    el.run_app(&mut app)
        .map_err(|e| format!("The window failed: {e}"))
}

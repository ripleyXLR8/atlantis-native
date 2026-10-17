//! Atlantis Native: the desktop and web front end. It owns the window, turns
//! input into engine events, runs the engine task one step per displayed
//! frame and shows its 640x480 frames scaled to the window.

use std::num::NonZeroU32;
use std::rc::Rc;

use atlantis_data::Game;
use atlantis_engine::audio::Mixer;
use atlantis_engine::sys::{Input, Key as EngineKey, MouseButton as EngineButton, Runner, Sys};
use atlantis_engine::warp::{SCREEN_H, SCREEN_W};
use web_time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Window, WindowId};

mod audio_out;
#[cfg(target_arch = "wasm32")]
mod web;

const W: usize = SCREEN_W;
const H: usize = SCREEN_H;

/// The engine's pace: one step per frame of 1/60 s. The original's loops
/// are not paced; ScummVM's port waits for the display.
const FRAME: Duration = Duration::from_micros(16_667);

/// Where the 640x480 picture sits in the window.
#[derive(Clone, Copy, Default)]
struct Viewport {
    ox: f64,
    oy: f64,
    scale: f64,
}

impl Viewport {
    fn to_screen(self, x: f64, y: f64) -> (i32, i32) {
        if self.scale <= 0.0 {
            return (0, 0);
        }
        let sx = ((x - self.ox) / self.scale).floor() as i32;
        let sy = ((y - self.oy) / self.scale).floor() as i32;
        (sx.clamp(0, W as i32 - 1), sy.clamp(0, H as i32 - 1))
    }

    fn to_window(self, x: i32, y: i32) -> (f64, f64) {
        (
            self.ox + (f64::from(x) + 0.5) * self.scale,
            self.oy + (f64::from(y) + 0.5) * self.scale,
        )
    }
}

static RGB_LUT: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();

pub struct App {
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    sys: Rc<Sys>,
    runner: Option<Runner>,
    /// Shown without a game: a test pattern.
    pattern: Vec<u16>,
    start: Instant,
    next_frame: Instant,
    viewport: Viewport,
    /// Source column of each window column, for the current width.
    col_map: Vec<usize>,
    /// The pointer lock as applied to the window.
    locked: bool,
    /// Raw device motion arrives (desktop): window cursor motion is then
    /// not used for the relative look.
    raw_motion: bool,
    last_cursor: Option<(f64, f64)>,
    /// Fractions of the relative motion not yet sent.
    rel_acc: (f64, f64),
    audio: Option<cpal::Stream>,
    /// The browser starts sound only after a user gesture.
    audio_pending: bool,
}

impl App {
    /// A front end for `runner` (the engine task on `sys`), or a test
    /// pattern without one.
    pub fn new(sys: Rc<Sys>, runner: Option<Runner>) -> App {
        let pattern = (0..W * H)
            .map(|i| (((i % W) * 31 / W) << 11 | ((i / W) * 63 / H) << 5) as u16)
            .collect();
        let now = Instant::now();
        App {
            window: None,
            surface: None,
            sys,
            runner,
            pattern,
            start: now,
            next_frame: now,
            viewport: Viewport::default(),
            col_map: Vec::new(),
            locked: false,
            raw_motion: false,
            last_cursor: None,
            rel_acc: (0.0, 0.0),
            audio: None,
            audio_pending: true,
        }
    }

    fn start_audio(&mut self) {
        if self.audio_pending {
            self.audio_pending = false;
            self.audio = audio_out::start(self.sys.mixer.clone());
        }
    }

    /// Runs the engine to its next frame when it is due; false once the
    /// engine has ended.
    fn step(&mut self) -> bool {
        let Some(runner) = &mut self.runner else {
            return true;
        };
        let now = Instant::now();
        // The browser paces the frames itself (requestAnimationFrame, one
        // RedrawRequested per screen refresh): a timer before the redraw
        // made the two delays add up and dropped frames.
        #[cfg(not(target_arch = "wasm32"))]
        if now < self.next_frame {
            return true;
        }
        self.next_frame += FRAME;
        if self.next_frame < now {
            self.next_frame = now + FRAME; // fell behind: do not catch up
        }
        let ms = now.duration_since(self.start).as_millis() as u64;
        let alive = runner.step(ms);
        self.apply_pointer();
        alive
    }

    /// The engine's pointer requests: lock (relative mode) and moves.
    fn apply_pointer(&mut self) {
        let Some(window) = &self.window else { return };
        let want = self.sys.mouse_locked();
        if want != self.locked {
            self.locked = want;
            self.last_cursor = None;
            if want {
                let _ = window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                    .map_err(|e| log::debug!("pointer lock: {e}"));
            } else {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
            }
        }
        if let Some((x, y)) = self.sys.take_warp_request() {
            if !self.locked {
                let (wx, wy) = self.viewport.to_window(x, y);
                let _ = window.set_cursor_position(PhysicalPosition::new(wx, wy));
                self.last_cursor = Some((wx, wy));
            }
        }
    }

    fn relative(&mut self, dx: f64, dy: f64) {
        self.rel_acc.0 += dx;
        self.rel_acc.1 += dy;
        let (ix, iy) = (self.rel_acc.0.trunc(), self.rel_acc.1.trunc());
        self.rel_acc.0 -= ix;
        self.rel_acc.1 -= iy;
        if ix != 0.0 || iy != 0.0 {
            self.sys.push_input(Input::MouseDelta {
                dx: ix as i32,
                dy: iy as i32,
            });
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
        // Nearest scaling keeping the 4:3 aspect, black bars.
        let scale = (sw as f64 / W as f64).min(sh as f64 / H as f64);
        let (dw, dh) = ((W as f64 * scale) as usize, (H as f64 * scale) as usize);
        let (ox, oy) = ((sw - dw) / 2, (sh - dh) / 2);
        self.viewport = Viewport {
            ox: ox as f64,
            oy: oy as f64,
            scale,
        };
        let screen = self.sys.screen.borrow();
        let frame: &[u16] = if self.runner.is_some() {
            &screen
        } else {
            &self.pattern
        };
        // RGB565 -> XRGB8888 table and source column per window column,
        // built once (the per-pixel arithmetic was the cost in the browser).
        let lut = RGB_LUT.get_or_init(|| {
            (0..=u16::MAX as u32)
                .map(|p| {
                    let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
                    (r << 3 | r >> 2) << 16 | (g << 2 | g >> 4) << 8 | (b << 3 | b >> 2)
                })
                .collect()
        });
        if self.col_map.len() != dw {
            self.col_map = (0..dw).map(|x| x * W / dw).collect();
        }
        buf.fill(0);
        let mut prev_src = usize::MAX;
        for y in 0..dh {
            let src_y = y * H / dh;
            let start = (oy + y) * sw + ox;
            if src_y == prev_src {
                // same source row as the line above: copy it
                buf.copy_within(start - sw..start - sw + dw, start);
                continue;
            }
            prev_src = src_y;
            let src_row = &frame[src_y * W..][..W];
            for (d, &x) in buf[start..start + dw].iter_mut().zip(&self.col_map) {
                *d = lut[usize::from(src_row[x])];
            }
        }
        buf.present().unwrap();
    }
}

fn engine_key(key: &Key) -> Option<EngineKey> {
    Some(match key {
        Key::Named(NamedKey::Escape) => EngineKey::Escape,
        Key::Named(NamedKey::Enter) => EngineKey::Enter,
        Key::Named(NamedKey::Space) => EngineKey::Space,
        Key::Named(NamedKey::Backspace) => EngineKey::Backspace,
        Key::Named(NamedKey::ArrowLeft) => EngineKey::Left,
        Key::Named(NamedKey::ArrowRight) => EngineKey::Right,
        Key::Named(NamedKey::ArrowUp) => EngineKey::Up,
        Key::Named(NamedKey::ArrowDown) => EngineKey::Down,
        Key::Named(NamedKey::F1) => EngineKey::F(1),
        Key::Named(NamedKey::F2) => EngineKey::F(2),
        Key::Named(NamedKey::F3) => EngineKey::F(3),
        Key::Named(NamedKey::F4) => EngineKey::F(4),
        Key::Named(NamedKey::F5) => EngineKey::F(5),
        Key::Named(NamedKey::F6) => EngineKey::F(6),
        Key::Named(NamedKey::F7) => EngineKey::F(7),
        Key::Named(NamedKey::F8) => EngineKey::F(8),
        Key::Named(NamedKey::F9) => EngineKey::F(9),
        Key::Named(NamedKey::F10) => EngineKey::F(10),
        Key::Named(NamedKey::F11) => EngineKey::F(11),
        Key::Named(NamedKey::F12) => EngineKey::F(12),
        Key::Character(s) => EngineKey::Char(s.chars().next()?),
        _ => return None,
    })
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
        // The game draws its own cursors.
        if self.runner.is_some() {
            window.set_cursor_visible(false);
        }
        let context = softbuffer::Context::new(window.clone()).unwrap();
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
        self.window = Some(window);
        // A desktop can open the sound device at once.
        #[cfg(not(target_arch = "wasm32"))]
        if self.runner.is_some() {
            self.start_audio();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // In the browser the redraw loop keeps itself going (see
        // RedrawRequested).
        #[cfg(target_arch = "wasm32")]
        el.set_control_flow(ControlFlow::Wait);
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.runner.is_none() {
                el.set_control_flow(ControlFlow::Wait);
                return;
            }
            if Instant::now() >= self.next_frame {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            // Raw motion: what the relative look reads, in pixels of the
            // 640x480 screen the original ran fullscreen in.
            if self.locked {
                self.raw_motion = true;
                self.relative(dx, dy);
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.sys.push_input(Input::Quit);
                el.exit();
            }
            WindowEvent::RedrawRequested => {
                if !self.step() {
                    el.exit();
                    return;
                }
                self.redraw();
                #[cfg(target_arch = "wasm32")]
                if self.runner.is_some() {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            WindowEvent::Resized(_) => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x, position.y);
                if self.locked {
                    // Without raw device motion (a browser without pointer
                    // lock), the cursor's own motion turns the view.
                    if !self.raw_motion {
                        if let Some((lx, ly)) = self.last_cursor {
                            let s = self.viewport.scale.max(1e-6);
                            self.relative((x - lx) / s, (y - ly) / s);
                        }
                    }
                } else {
                    let (sx, sy) = self.viewport.to_screen(x, y);
                    self.sys.push_input(Input::MouseMove { x: sx, y: sy });
                }
                self.last_cursor = Some((x, y));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.start_audio();
                let button = match button {
                    MouseButton::Left => EngineButton::Left,
                    MouseButton::Right => EngineButton::Right,
                    _ => return,
                };
                let (x, y) = self.sys.mouse();
                self.sys.push_input(match state {
                    ElementState::Pressed => Input::MouseDown { button, x, y },
                    ElementState::Released => Input::MouseUp { button, x, y },
                });
                // A browser grants the pointer lock only on a user gesture.
                if self.locked {
                    if let Some(w) = &self.window {
                        let _ = w.set_cursor_grab(CursorGrabMode::Locked);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // TODO(phase 3): Escape in the panoramic view (the pointer
                // locked) opens the in-game menu; until then it leaves the
                // desktop game. Elsewhere (a dialog line) the game reads it.
                #[cfg(not(target_arch = "wasm32"))]
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape)
                    && self.sys.mouse_locked()
                {
                    self.sys.push_input(Input::Quit);
                    el.exit();
                    return;
                }
                if event.repeat {
                    return;
                }
                if let Some(k) = engine_key(&event.logical_key) {
                    self.sys.push_input(match event.state {
                        ElementState::Pressed => Input::KeyDown(k),
                        ElementState::Released => Input::KeyUp(k),
                    });
                }
            }
            _ => {}
        }
    }
}

/// The engine task for `game`, from the new-game start.
pub fn start_game(game: Game, options: atlantis_engine::Options) -> (Rc<Sys>, Runner) {
    log::info!(
        "{}: {} archive members",
        game.exe.build.name,
        game.big.len()
    );
    let sys = Sys::new(Mixer::new());
    let runner = Runner::new(
        sys.clone(),
        atlantis_engine::run(sys.clone(), game, options),
    );
    (sys, runner)
}

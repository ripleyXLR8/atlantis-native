//! Atlantis III, unfinished: a developers' window, not part of the
//! program. The intro, then a new game (the Hoggar after chapter 1's
//! cinematics); the mouse turns the view, a left click moves, Space or
//! Escape skips a cinematic, Escape in the panorama quits, F12 writes a
//! screenshot (`a3-<n>.png` in the current folder).
//!
//! `cargo run --release -p atlantis-app --example atlantis3 -- <game folder> [--no-intro] [--shot-after <seconds>]`
//! (`--shot-after`: a screenshot of the window's picture at that time of
//! the game, then the end).
//! (the game folder defaults to `ATLANTIS3_DATA`).

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use atlantis3_data::A3Files;
use atlantis3_engine::audio::Audio;
use atlantis3_engine::{Game, Mode};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Window, WindowId};

struct App {
    game: Game,
    start: Instant,
    intro_done: bool,
    no_intro: bool,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    shots: u32,
    shot_after: Option<f64>,
}

/// The game's sound to the default output device.
fn open_audio(audio: Audio) -> Option<cpal::Stream> {
    let device = cpal::default_host().default_output_device()?;
    let config = device.default_output_config().ok()?;
    let rate = config.sample_rate().0;
    let channels = usize::from(config.channels());
    if config.sample_format() != cpal::SampleFormat::F32 {
        eprintln!("audio: only f32 output devices are used by this example");
        return None;
    }
    let stream = device
        .build_output_stream(
            &config.into(),
            move |out: &mut [f32], _| audio.fill(out, rate, channels),
            |e| eprintln!("audio: {e}"),
            None,
        )
        .ok()?;
    stream.play().ok()?;
    Some(stream)
}

impl App {
    fn present(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, self.surface.as_mut()) else {
            return;
        };
        let size = window.inner_size();
        let (ww, wh) = (size.width.max(1) as usize, size.height.max(1) as usize);
        let (Some(w), Some(h)) = (NonZeroU32::new(ww as u32), NonZeroU32::new(wh as u32)) else {
            return;
        };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };
        let (gw, gh) = (self.game.width(), self.game.height());
        let screen = self.game.screen();
        for y in 0..wh {
            let sy = y * gh / wh;
            for x in 0..ww {
                let p = screen[sy * gw + x * gw / ww];
                let [r, g, b] = atlantis2_engine::png::rgb565_to_rgb(p);
                buf[y * ww + x] = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
            }
        }
        let _ = buf.present();
    }

    fn screenshot(&mut self) {
        let png = atlantis2_engine::png::encode_rgb565(
            self.game.screen(),
            self.game.width(),
            self.game.height(),
        );
        let name = format!("a3-{}.png", self.shots);
        self.shots += 1;
        match std::fs::write(&name, png) {
            Ok(()) => println!("{name}"),
            Err(e) => eprintln!("{name}: {e}"),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Atlantis III (unfinished) - Atlantis Native")
            .with_inner_size(LogicalSize::new(800, 600));
        let Ok(window) = el.create_window(attrs) else {
            eprintln!("no window");
            el.exit();
            return;
        };
        let window = Rc::new(window);
        let context = softbuffer::Context::new(window.clone()).unwrap();
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
        let _ = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        window.set_cursor_visible(false);
        self.window = Some(window);
        self.start = Instant::now();
        if self.no_intro {
            self.intro_done = true;
            self.game.new_game();
        } else {
            self.game.start_intro();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        el.set_control_flow(ControlFlow::Poll);
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.game.mouse_motion(dx as i32, dy as i32);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.game.click(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) if self.game.mode() == Mode::Warp => el.exit(),
                    Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Space) => self.game.skip(),
                    Key::Named(NamedKey::F12) => self.screenshot(),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = self.start.elapsed().as_secs_f64();
                self.game.step(now);
                if !self.intro_done && self.game.mode() == Mode::Idle {
                    self.intro_done = true;
                    self.game.new_game();
                }
                if self.game.quit {
                    el.exit();
                }
                self.present();
                if self.shot_after.is_some_and(|t| now >= t) {
                    self.screenshot();
                    el.exit();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut dir = std::env::var_os("ATLANTIS3_DATA").map(PathBuf::from);
    let mut no_intro = false;
    let mut shot_after = None;
    while let Some(a) = args.next() {
        if a == "--no-intro" {
            no_intro = true;
        } else if a == "--shot-after" {
            shot_after = args.next().and_then(|v| v.parse().ok());
        } else {
            dir = Some(PathBuf::from(a));
        }
    }
    let Some(dir) = dir else {
        eprintln!("usage: atlantis3 <game folder> [--no-intro]");
        return;
    };
    let files = match A3Files::open_dir(&dir) {
        Ok((f, _)) => f,
        Err(e) => {
            eprintln!("{}: {e}", dir.display());
            return;
        }
    };
    let game = match Game::new(files) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    let _stream = open_audio(game.audio.clone());
    let el = EventLoop::new().expect("an event loop");
    let mut app = App {
        game,
        start: Instant::now(),
        intro_done: false,
        no_intro,
        window: None,
        surface: None,
        shots: 0,
        shot_after,
    };
    let _ = el.run_app(&mut app);
}

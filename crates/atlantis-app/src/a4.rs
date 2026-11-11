//! A development viewer of *Atlantis Evolution*'s warp view (cargo
//! feature `atlantis4`, off by default and in every release: the game is
//! not offered until its port is complete).
//!
//! `atlantis-native --atlantis4-view <game folder> [node]` boots the game's
//! scripts as a new game does (the `Texts` boot files, `3D/OnGameEnter.lua`,
//! `Diary/Diary.lua`, `Inventory/InventoryScripts.lua`, then the node
//! change to `firstWarpPath`, or to `node` when given) and shows the node
//! in a window of 1365 × 768 drawn on the processor by
//! `atlantis4_engine`: the background, the objects and animations the
//! node's `OnWarpEnter` puts there, the markers and the game's cursor.
//! Dragging with the left button turns the view (0.5° per pixel, the
//! original's mouse look); a left click of less than 20 pixels calls the
//! node's `OnWarpObjectHit` with what it hits, which leads through an exit
//! with `ChangeWarp` as the original. `PlayMovie` plays its video full
//! screen (the intro at a new game) over a black background: while it
//! runs the view does not turn and clicks go to the movie's buttons (a
//! click shows or hides the skip button at the top right, a click on it
//! ends the movie), and its end calls `OnMovieFinished`. The sounds play
//! as the original mixes them (`atlantis4_engine::sound`, `oalmix`): the
//! music, the node's `PlaySound` sounds and ambiences, `OnSoundStopped`,
//! the `AddToBag` notification; the device is asked for 44,100 Hz, else the mix is resampled linearly
//! to its rate (outside the exact part, like Windows' own audio engine
//! after OpenAL). Dialogs and the inventory are not there yet (logged).

use std::cell::RefCell;
use std::num::NonZeroU32;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use atlantis4_data::A4Files;
use atlantis4_engine::pick::is_click;
use atlantis4_engine::Game;
use atlantis4_script::{
    new_game, read_boot_vars, BootValue, EngineRef, ErrorMode, Options, Script, Value,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const WIDTH: u32 = 1365;
const HEIGHT: u32 = 768;

/// The time between two frames of the viewer.
const FRAME: Duration = Duration::from_millis(33);

struct Viewer {
    game: Rc<RefCell<Game>>,
    /// The sound output, kept alive.
    _audio: Option<cpal::Stream>,
    script: Script,
    start: Instant,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    pointer: Option<(f64, f64)>,
    down: Option<(f64, f64)>,
    last: (f64, f64),
    size: (u32, u32),
}

impl Viewer {
    /// The engine clock: whole microseconds, divided by 1000.0 as the
    /// original's animation clock is.
    fn now_ms(&self) -> f64 {
        clock_us(self.start) as f64 / 1000.0
    }

    /// The frame animations, then the `OnAnimationFinished` callbacks of
    /// those that finished; the movie, then `OnMovieFinished` if it ended.
    fn tick(&mut self) {
        let now = self.now_ms();
        let finished = self.game.borrow_mut().update(now);
        // `TeSoundManager::update` ran first in the frame: its signals'
        // callbacks (`OnSoundStopped`) come before the animations'.
        let sounds = self.game.borrow_mut().sound_callbacks();
        for (name, args) in sounds {
            if let Err(e) = self.script.callback(&name, &args) {
                log::error!("{e}");
            }
        }
        for name in finished {
            let arg = [Value::Str(name.into_bytes())];
            if let Err(e) = self.script.callback("OnAnimationFinished", &arg) {
                log::error!("{e}");
            }
        }
        let ended = self.game.borrow_mut().update_movie(now);
        if let Some(name) = ended {
            self.movie_finished(name);
        }
    }

    fn movie_finished(&mut self, name: String) {
        let arg = [Value::Str(name.into_bytes())];
        if let Err(e) = self.script.callback("OnMovieFinished", &arg) {
            log::error!("{e}");
        }
    }

    fn in_movie(&self) -> bool {
        self.game.borrow().movie.is_some()
    }

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
        if let Ok(mut buf) = surface.buffer_mut() {
            {
                let mut g = self.game.borrow_mut();
                g.height = h;
                g.draw(&mut buf, w, h, self.pointer);
            }
            window.pre_present_notify();
            let _ = buf.present();
        }
    }

    fn release(&mut self) {
        let Some(down) = self.down.take() else { return };
        let Some(up) = self.pointer else { return };
        if self.in_movie() {
            let (w, h) = self.size;
            let ended = self.game.borrow_mut().movie_release(w, h, up.0, up.1);
            if let Some(name) = ended {
                self.movie_finished(name);
            }
            return;
        }
        if !is_click((down.0 as f32, down.1 as f32), (up.0 as f32, up.1 as f32)) {
            return;
        }
        let (w, h) = self.size;
        let hit = self.game.borrow().click(w, h, up.0 as i32, up.1 as i32);
        log::info!("click at {up:?}: {hit:?}");
        if let Some(name) = hit {
            // The warp's click callback (`+0xbc`): `OnWarpObjectHit(name)`.
            let arg = [Value::Str(name.into_bytes())];
            if let Err(e) = self.script.callback("OnWarpObjectHit", &arg) {
                log::error!("{e}");
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
        // The game draws its own cursor (`2D/arrow6.png`).
        window.set_cursor_visible(false);
        let s = window.inner_size();
        self.size = (s.width, s.height);
        self.window = Some(window);
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + FRAME));
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.tick();
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + FRAME));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => self.size = (s.width, s.height),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            WindowEvent::CursorMoved { position, .. } => {
                let p = (position.x, position.y);
                self.pointer = Some(p);
                if self.in_movie() {
                    // The view update (0x0052ab40) does not run during a
                    // movie; a held press only changes the skip button's
                    // picture.
                    if self.down.is_some() {
                        let (w, h) = self.size;
                        self.game.borrow_mut().movie_drag(w, h, p.0, p.1);
                    }
                } else if self.down.is_some() {
                    // Input pixels, y up.
                    let dx = p.0 - self.last.0;
                    let dy_up = self.last.1 - p.1;
                    self.game
                        .borrow_mut()
                        .view
                        .drag(dx as f32, dy_up as f32, false);
                }
                self.last = p;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => {
                    if let Some(p) = self.pointer {
                        self.down = Some(p);
                        self.last = p;
                        if self.in_movie() {
                            let (w, h) = self.size;
                            self.game.borrow_mut().movie_press(w, h, p.0, p.1);
                        }
                    }
                }
                ElementState::Released => self.release(),
            },
            _ => {}
        }
    }
}

/// Whole microseconds since `start` (the original's clock counts whole
/// microseconds, @ 0x004faf40).
fn clock_us(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// The sound output: the 44,100 Hz stereo mix to the device, as is when
/// the device runs at 44,100 Hz, else linearly resampled.
fn start_sound(core: atlantis4_engine::sound::Core) -> Option<cpal::Stream> {
    const RATE: u32 = atlantis4_engine::oalmix::RATE;
    // Resampler state: source frames not yet used, position in them.
    let mut src: Vec<i16> = Vec::new();
    let mut pos = 0f64;
    crate::audio_out::start_with_rate(
        move |out, rate, channels| {
            let channels = channels.max(1);
            let frames = out.len() / channels;
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            let put = |o: &mut [f32], l: f32, r: f32| {
                if channels == 1 {
                    o[0] = (l + r) / 2.0;
                } else {
                    o[0] = l;
                    o[1] = r;
                    o[2..].fill(0.0);
                }
            };
            if rate == RATE {
                let mut buf = vec![0i16; 2 * frames];
                core.pull(&mut buf);
                for (o, s) in out.chunks_exact_mut(channels).zip(buf.as_chunks::<2>().0) {
                    put(o, f32::from(s[0]) / 32768.0, f32::from(s[1]) / 32768.0);
                }
                return;
            }
            let step = f64::from(RATE) / f64::from(rate);
            for o in out.chunks_exact_mut(channels) {
                let i = pos as usize;
                while src.len() < 2 * (i + 2) {
                    let mut more = vec![0i16; 2 * 256];
                    core.pull(&mut more);
                    src.extend(more);
                }
                let f = (pos - i as f64) as f32;
                let at = |k: usize| f32::from(src[k]) / 32768.0;
                let l = at(2 * i) * (1.0 - f) + at(2 * i + 2) * f;
                let r = at(2 * i + 1) * (1.0 - f) + at(2 * i + 3) * f;
                put(o, l, r);
                pos += step;
            }
            let used = pos as usize;
            src.drain(..2 * used.min(src.len() / 2));
            pos -= used as f64;
        },
        RATE,
    )
}

/// Opens the viewer on the game at `dir`, at `node` or the first node.
pub fn run_view(dir: &Path, node: Option<&str>) -> Result<(), String> {
    let (files, build) = A4Files::open_dir(dir).map_err(|e| e.to_string())?;
    log::info!("Atlantis Evolution {}", build.id);
    let start = Instant::now();
    let mut g = Game::new(files, HEIGHT);
    // `play()` reads the clock at the call (a movie started by a script).
    g.clock_us = Some(Box::new(move || clock_us(start)));
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let options = Options {
        errors: ErrorMode::Log,
        ..Options::default()
    };
    let boot = read_boot_vars(&engine, options).map_err(|e| e.to_string())?;
    let first = match (node, &boot.first_warp_path) {
        (Some(n), _) => n.as_bytes().to_vec(),
        (None, Some(BootValue::Str(s))) => s.clone(),
        (None, other) => return Err(format!("no firstWarpPath ({other:?})")),
    };
    let audio = start_sound(game.borrow().sounds.core());
    game.borrow_mut().sounds.headless = audio.is_none();
    let script = new_game(&engine, options, &first).map_err(|e| e.to_string())?;
    let mut app = Viewer {
        _audio: audio,
        game,
        script,
        start,
        window: None,
        surface: None,
        pointer: None,
        down: None,
        last: (0.0, 0.0),
        size: (WIDTH, HEIGHT),
    };
    let el = EventLoop::new().map_err(|e| format!("No window can be opened: {e}"))?;
    el.run_app(&mut app)
        .map_err(|e| format!("The window failed: {e}"))
}

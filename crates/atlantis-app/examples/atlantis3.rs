//! Atlantis III, unfinished: a developers' window, not part of the
//! program, good enough to play the game from the intro to the credits.
//!
//! As the original: the intro (Space or Escape skips it), the start page
//! and its five players, the in-game menu on Escape (save, load, volumes,
//! options, quit), the mouse turns the view, a left click acts, the right
//! button opens and closes the inventory, Space or Escape skips a
//! cinematic, Escape ends a travel video, Alt+Enter switches between the
//! window and the full screen (the original's `FullScreen` setting chooses
//! the first). The port's own: F11 switches the full screen too, F12
//! writes a screenshot of the game's picture (`captures/a3-<n>.png` in
//! the current folder).
//!
//! The saves (`save/game<P>_<N>.sav` with their thumbnails, the scripts'
//! quick saves) and `Atlantis3.cfg` live in the player's data folder
//! (`%APPDATA%\atlantis-native\atlantis3` on Windows, `ATLANTIS_NATIVE_DATA`
//! when set), never in the game's folder; so does `atlantis3.log` when no
//! terminal shows the log.
//!
//! `cargo run --release -p atlantis-app --example atlantis3 -- <game folder> [--no-intro] [--window | --fullscreen] [--saves <folder>] [--shot-after <seconds>]`
//! (the game folder defaults to `ATLANTIS3_DATA`; `--no-intro`: the start
//! page at once; `--shot-after`: a screenshot of the window's picture at
//! that time of the game, then the end).

// No console window behind the game on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

use atlantis3_engine::px::Surface;
use std::io::IsTerminal;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use atlantis3_data::A3Files;
use atlantis3_engine::audio::Audio;
use atlantis3_engine::{Game, Mode};
use atlantis_data::{FileSource, StdDir};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

/// The window's refresh: the game's clocks are its own (the 25 Hz view
/// tick, the videos' frame times), so this only spares the processor.
const FRAME: Duration = Duration::from_micros(16_667);

/// The install with `Datas\Atlantis3.cfg` taken from the player's folder
/// once the menus wrote it there (the game's folder stays untouched).
struct WithCfg {
    install: StdDir,
    cfg: PathBuf,
}

impl WithCfg {
    fn is_cfg(name: &str) -> bool {
        name.replace('\\', "/")
            .eq_ignore_ascii_case(atlantis3_data::cfg::CFG_PATH)
    }
}

impl FileSource for WithCfg {
    fn size(&self, name: &str) -> Option<u64> {
        if Self::is_cfg(name) {
            if let Ok(m) = std::fs::metadata(&self.cfg) {
                return Some(m.len());
            }
        }
        self.install.size(name)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        if Self::is_cfg(name) {
            if let Ok(d) = std::fs::read(&self.cfg) {
                let start = usize::try_from(offset).ok()?.min(d.len());
                return Some(d[start..(start + len).min(d.len())].to_vec());
            }
        }
        self.install.read_at(name, offset, len)
    }

    fn has_dir(&self, name: &str) -> bool {
        self.install.has_dir(name)
    }

    fn list(&self, dir: &str) -> Vec<String> {
        self.install.list(dir)
    }
}

struct App {
    game: Game,
    start: Instant,
    next_frame: Instant,
    intro_done: bool,
    no_intro: bool,
    fullscreen: bool,
    focused: bool,
    alt: bool,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    shot_after: Option<f64>,
    cfg_path: PathBuf,
    /// The game's screen size the window was last given.
    shown: (usize, usize),
}

/// The game's sound to the default output device.
fn open_audio(audio: Audio) -> Option<cpal::Stream> {
    let device = cpal::default_host().default_output_device()?;
    let config = device.default_output_config().ok()?;
    let rate = config.sample_rate().0;
    let channels = usize::from(config.channels());
    if config.sample_format() != cpal::SampleFormat::F32 {
        log::warn!("audio: only f32 output devices are used by this example");
        return None;
    }
    let stream = device
        .build_output_stream(
            &config.into(),
            move |out: &mut [f32], _| audio.fill(out, rate, channels),
            |e| log::warn!("audio: {e}"),
            None,
        )
        .ok()?;
    stream.play().ok()?;
    Some(stream)
}

/// The first free `captures/a3-<n>.png`.
fn shot_path() -> PathBuf {
    let dir = PathBuf::from("captures");
    let _ = std::fs::create_dir_all(&dir);
    (0..)
        .map(|n| dir.join(format!("a3-{n}.png")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join("a3.png"))
}

/// The log: to the terminal when there is one (or `RUST_LOG` is set),
/// else to `atlantis3.log` in the player's folder (the one before kept as
/// `atlantis3.previous.log`); panics logged too.
fn init_log(dir: &Path) {
    if std::io::stderr().is_terminal() || std::env::var_os("RUST_LOG").is_some() {
        env_logger::init();
    } else {
        let path = dir.join("atlantis3.log");
        let _ = std::fs::rename(&path, dir.join("atlantis3.previous.log"));
        match std::fs::File::create(&path) {
            Ok(file) => env_logger::Builder::new()
                .filter_level(log::LevelFilter::Info)
                .target(env_logger::Target::Pipe(Box::new(file)))
                .init(),
            Err(_) => env_logger::init(),
        }
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        default(info);
    }));
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
        // The game's picture at its own proportions, black bars around.
        let (gw, gh) = (self.game.width(), self.game.height());
        // The options' size switch re-creates the display at once
        // (@ 0x0040fa60): a window takes the new size (the surface follows
        // the window's at the next frame); full screen keeps the monitor.
        if (gw, gh) != self.shown {
            self.shown = (gw, gh);
            if !self.fullscreen {
                let _ = window.request_inner_size(LogicalSize::new(gw as u32, gh as u32));
            }
        }
        let (dw, dh) = if ww * gh > wh * gw {
            (wh * gw / gh, wh)
        } else {
            (ww, ww * gh / gw)
        };
        let (dw, dh) = (dw.max(1), dh.max(1));
        let (ox, oy) = ((ww - dw) / 2, (wh - dh) / 2);
        let xs: Vec<usize> = (0..dw).map(|x| x * gw / dw).collect();
        buf.fill(0);
        match self.game.surface() {
            Surface::Rgb565(screen) => {
                for y in 0..dh {
                    let row = &screen[(y * gh / dh) * gw..][..gw];
                    let out = &mut buf[(oy + y) * ww + ox..][..dw];
                    for (o, &sx) in out.iter_mut().zip(&xs) {
                        let [r, g, b] = atlantis2_engine::png::rgb565_to_rgb(row[sx]);
                        *o = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
                    }
                }
            }
            // A 32-bit display: the pixels as they are (x byte dropped).
            Surface::Xrgb8888(screen) => {
                for y in 0..dh {
                    let row = &screen[(y * gh / dh) * gw..][..gw];
                    let out = &mut buf[(oy + y) * ww + ox..][..dw];
                    for (o, &sx) in out.iter_mut().zip(&xs) {
                        *o = row[sx] & 0x00ff_ffff;
                    }
                }
            }
        }
        let _ = buf.present();
    }

    fn screenshot(&mut self) {
        let (w, h) = (self.game.width(), self.game.height());
        let png = match self.game.surface() {
            Surface::Rgb565(s) => atlantis2_engine::png::encode_rgb565(s, w, h),
            Surface::Xrgb8888(s) => atlantis2_engine::png::encode_xrgb8888(s, w, h),
        };
        let path = shot_path();
        match std::fs::write(&path, png) {
            Ok(()) => {
                log::info!("screenshot {}", path.display());
                println!("{}", path.display());
            }
            Err(e) => log::warn!("{}: {e}", path.display()),
        }
    }

    /// The pointer kept in the window while it has the focus (the original
    /// reads the mouse through DirectInput and draws its own pointer).
    fn grab(&self) {
        let Some(window) = &self.window else {
            return;
        };
        if self.focused {
            let _ = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            window.set_cursor_visible(false);
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
        }
    }

    /// Alt+Enter (@ 0x0042ab9d: `AtlDisplay_SetFullScreen(!FullScreen)`,
    /// the setting written with the next `Atlantis3.cfg`).
    fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        self.game.startup.config.full_screen = self.fullscreen;
        if let Some(w) = &self.window {
            w.set_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)));
        }
        self.grab();
    }

    /// Escape on the credits page (@ 0x00457360): `Atlantis3.cfg`
    /// written, then the end.
    fn write_cfg(&self) {
        let data = self
            .game
            .startup
            .config
            .write(&self.game.startup.install_language);
        if let Err(e) = std::fs::write(&self.cfg_path, data) {
            log::warn!("{}: {e}", self.cfg_path.display());
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (gw, gh) = (self.game.width() as u32, self.game.height() as u32);
        let attrs = Window::default_attributes()
            .with_title("Atlantis III (test) - Atlantis Native")
            .with_inner_size(LogicalSize::new(gw, gh))
            .with_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)));
        let Ok(window) = el.create_window(attrs) else {
            log::error!("no window");
            el.exit();
            return;
        };
        let window = Rc::new(window);
        let context = softbuffer::Context::new(window.clone()).unwrap();
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
        self.window = Some(window);
        self.focused = true;
        self.grab();
        self.start = Instant::now();
        self.next_frame = self.start;
        if self.no_intro {
            self.intro_done = true;
            if !self.game.show_start_page() {
                self.game.new_game();
            }
        } else {
            self.game.start_intro();
        }
    }

    fn new_events(&mut self, _el: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if !self.focused {
            return;
        }
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.game.mouse_motion(dx as i32, dy as i32);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Focused(f) => {
                self.focused = f;
                self.grab();
            }
            WindowEvent::ModifiersChanged(m) => self.alt = m.state().alt_key(),
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => self.game.click(),
                (ElementState::Released, MouseButton::Left) => self.game.release(),
                (ElementState::Pressed, MouseButton::Right) => self.game.right_click(),
                _ => {}
            },
            // The original reads key presses only (DirectInput's buffer,
            // @ 0x0042ab20; the cinematics' `WM_KEYDOWN`): no repeats.
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                match event.logical_key {
                    Key::Named(NamedKey::Enter) if self.alt => self.toggle_fullscreen(),
                    Key::Named(NamedKey::F11) => self.toggle_fullscreen(),
                    Key::Named(NamedKey::Escape) if self.game.credits().is_some() => {
                        if self.game.credits_escape() {
                            self.write_cfg();
                        }
                    }
                    // By loop mode (@ 0x0042ac50): the in-game menu, a
                    // travel video's end, a cinematic's skip, a dialog's
                    // click.
                    Key::Named(NamedKey::Escape) => self.game.escape(),
                    // A cinematic's wait loop only (@ 0x00421c5b).
                    Key::Named(NamedKey::Space) => self.game.skip(),
                    Key::Named(NamedKey::F12) => self.screenshot(),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = self.start.elapsed().as_secs_f64();
                self.game.step(now);
                if !self.intro_done && self.game.mode() == Mode::Idle {
                    self.intro_done = true;
                    // The start page after the intro (mode 6, page 0).
                    if !self.game.show_start_page() {
                        self.game.new_game();
                    }
                }
                if self.game.quit {
                    el.exit();
                }
                self.present();
                if self.shot_after.is_some_and(|t| now >= t) {
                    self.screenshot();
                    el.exit();
                }
                let t = Instant::now();
                self.next_frame = (self.next_frame + FRAME).max(t);
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
    let mut fullscreen = None;
    let mut saves = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--no-intro" => no_intro = true,
            "--shot-after" => shot_after = args.next().and_then(|v| v.parse().ok()),
            "--window" => fullscreen = Some(false),
            "--fullscreen" => fullscreen = Some(true),
            "--saves" => saves = args.next().map(PathBuf::from),
            _ => dir = Some(PathBuf::from(a)),
        }
    }
    let base = saves.unwrap_or_else(|| atlantis_app::desktop::data_dir().join("atlantis3"));
    let _ = std::fs::create_dir_all(&base);
    init_log(&base);
    let Some(dir) = dir else {
        log::error!("usage: atlantis3 <game folder> [--no-intro] [--window | --fullscreen] [--saves <folder>] [--shot-after <seconds>]");
        return;
    };
    let cfg_path = base.join("Atlantis3.cfg");
    let files = A3Files::new(Arc::new(WithCfg {
        install: StdDir::new(&dir),
        cfg: cfg_path.clone(),
    }));
    if let Err(e) = files.build() {
        log::error!("{}: {e}", dir.display());
        return;
    }
    let mut game = match Game::new(files) {
        Ok(g) => g,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };
    game.save_dir = Some(base.join("save"));
    game.menus_mut().cfg_path = Some(cfg_path.clone());
    log::info!("saves and settings in {}", base.display());
    let fullscreen = fullscreen.unwrap_or(game.startup.config.full_screen);
    let _stream = open_audio(game.audio.clone());
    let el = EventLoop::new().expect("an event loop");
    let shown = (game.width(), game.height());
    let mut app = App {
        game,
        start: Instant::now(),
        next_frame: Instant::now(),
        intro_done: false,
        no_intro,
        fullscreen,
        focused: true,
        alt: false,
        window: None,
        surface: None,
        shot_after,
        cfg_path,
        shown,
    };
    let _ = el.run_app(&mut app);
}

//! Atlantis III on the desktop: the game in a window of its own, as
//! Atlantis II's mode ([`crate::a2`]) runs beside the first game's front
//! end.
//!
//! As the original: the intro (Space or Escape skips it), the start page
//! and its five players, the in-game menu on Escape (save, load, volumes,
//! options, quit), the mouse turns the view, a left click acts, the right
//! button opens and closes the inventory, Space or Escape skips a
//! cinematic, Escape ends a travel video, Alt+Enter switches between the
//! window and the full screen (the original's `FullScreen` setting
//! chooses the first, and the switch is kept there). The port's own: F11
//! switches the full screen too, F12 writes a screenshot of the game's
//! picture (`atlantis3/screenshots/a3-<n>.png` in the per-user data
//! folder). The window keeps the pointer while it has the focus (the
//! original reads the mouse through DirectInput and draws its own).
//!
//! The saves (`save/game<P>_<N>.sav` with their thumbnails, the scripts'
//! quick saves) and `Atlantis3.cfg` (the original's settings, which its
//! options page writes) live in the `atlantis3` folder of the per-user
//! data folder, never in the game's folder: the original's `Datas\Atlantis3.cfg`
//! is read until the menus write the player's own.
//!
//! The game folder is GOG.com's installation (`Game.exe` beside `Datas\`),
//! the only edition known ([`atlantis3_data::exe::KNOWN_BUILDS`]).
//!
//! Started from the game library, Shift held in the first moments goes
//! back to the library's screen ([`crate::a2::Ended::Library`]), as for
//! the other games.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use atlantis3_data::files::OpenError;
use atlantis3_data::A3Files;
use atlantis3_engine::px::Surface;
use atlantis3_engine::{Game, Mode};
use atlantis_data::{FileSource, StdDir};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

pub use atlantis3_data::files::holds_atlantis3;

use crate::a2::Ended;

/// The window's refresh: the game's clocks are its own (the 25 Hz view
/// tick, the videos' frame times), so this only spares the processor.
const FRAME: Duration = Duration::from_micros(16_667);

/// How long after an automatic start Shift still goes back to the game
/// library (the other games' delay).
const SHIFT_GRACE: Duration = Duration::from_millis(2000);

/// Atlantis III's folder in the per-user data folder: its saves,
/// `Atlantis3.cfg` and the screenshots.
pub fn data_dir() -> PathBuf {
    crate::desktop::data_dir().join("atlantis3")
}

/// The edition of the Atlantis III installation in `dir`, for the game
/// library ("Atlantis III, GOG.com, French").
pub fn label(dir: &Path) -> Result<String, OpenError> {
    A3Files::open_dir(dir).map(|(_, build)| build.label.to_string())
}

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

/// The game in `dir`, its saves and `Atlantis3.cfg` in `base`; with the
/// edition's name.
pub fn open_in(dir: &Path, base: &Path) -> Result<(Game, String), String> {
    let _ = std::fs::create_dir_all(base);
    let cfg_path = base.join("Atlantis3.cfg");
    let files = A3Files::new(Arc::new(WithCfg {
        install: StdDir::new(dir),
        cfg: cfg_path.clone(),
    }));
    let build = files
        .build()
        .map_err(|e| crate::setup::explain_atlantis3(dir, &e))?;
    let mut game = Game::new(files).map_err(|e| format!("{}: {e}", dir.display()))?;
    game.save_dir = Some(base.join("save"));
    game.menus_mut().cfg_path = Some(cfg_path);
    log::info!(
        "{}, {}; saves and settings in {}",
        build.label,
        crate::version_string(),
        base.display()
    );
    Ok((game, build.label.to_string()))
}

/// The game in `dir`, its saves and settings in [`data_dir`].
pub fn open(dir: &Path) -> Result<(Game, String), String> {
    open_in(dir, &data_dir())
}

/// The game's picture as PNG.
fn png(game: &Game) -> Vec<u8> {
    let (w, h) = (game.width(), game.height());
    match game.surface() {
        Surface::Rgb565(s) => atlantis2_engine::png::encode_rgb565(s, w, h),
        Surface::Xrgb8888(s) => atlantis2_engine::png::encode_xrgb8888(s, w, h),
    }
}

/// Plays the game in `dir` without a window for `frames` frames of 1/60
/// s (the intro, then the start page; with `no_intro` the start page at
/// once), the sound mixed and dropped, the saves in `base`; prints where
/// it is and writes the screen to `screenshot` (PNG) when given. A check
/// that the game starts.
pub fn headless(
    dir: &Path,
    base: &Path,
    frames: u64,
    screenshot: Option<&Path>,
    no_intro: bool,
) -> Result<(), String> {
    let (mut game, label) = open_in(dir, base)?;
    println!("{label}, {}", crate::version_string());
    let mut intro_done = no_intro;
    if no_intro {
        if !game.show_start_page() {
            game.new_game();
        }
    } else {
        game.start_intro();
    }
    let mut sound = vec![0.0f32; 2 * (22050 / 60)];
    for i in 0..frames {
        game.step(i as f64 / 60.0);
        game.audio.fill(&mut sound, 22050, 2);
        if !intro_done && game.mode() == Mode::Idle {
            intro_done = true;
            if !game.show_start_page() {
                game.new_game();
            }
        }
        if game.quit {
            break;
        }
    }
    println!(
        "{frames} frames, mode {:?}, {}x{}",
        game.mode(),
        game.width(),
        game.height()
    );
    if let Some(path) = screenshot {
        std::fs::write(path, png(&game)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("screenshot: {}", path.display());
    }
    Ok(())
}

/// The window's application.
pub struct A3App {
    game: Game,
    start: Instant,
    next_frame: Instant,
    intro_done: bool,
    /// The start page at once, without the intro.
    no_intro: bool,
    fullscreen: bool,
    focused: bool,
    alt: bool,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    _audio: Option<cpal::Stream>,
    /// The game's screen size the window was last given.
    shown: (usize, usize),
    /// Until when Shift goes back to the game library (started from it).
    shift_until: Option<Instant>,
    /// The window ended for the game library's screen.
    to_library: bool,
}

/// The first free `a3-<n>.png` in `dir`.
fn shot_path(dir: &Path) -> PathBuf {
    let _ = std::fs::create_dir_all(dir);
    (0..)
        .map(|n| dir.join(format!("a3-{n}.png")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join("a3.png"))
}

impl A3App {
    /// The window's game before its window opens; full screen as
    /// `Atlantis3.cfg` says, or as `fullscreen` asks; the intro first
    /// unless `no_intro`.
    pub fn new(game: Game, fullscreen: bool, no_intro: bool) -> A3App {
        let fullscreen = fullscreen || game.startup.config.full_screen;
        let audio = game.audio.clone();
        let stream = crate::audio_out::start_with(move |out, rate, channels| {
            audio.fill(out, rate, channels);
        });
        let shown = (game.width(), game.height());
        A3App {
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
            _audio: stream,
            shown,
            shift_until: None,
            to_library: false,
        }
    }

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
        let path = shot_path(&data_dir().join("screenshots"));
        match std::fs::write(&path, png(&self.game)) {
            Ok(()) => log::info!("screenshot {}", path.display()),
            Err(e) => log::warn!("{}: {e}", path.display()),
        }
    }

    /// The pointer kept in the window while it has the focus.
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
        let Some(path) = self.game.menus().cfg_path.clone() else {
            return;
        };
        let data = self
            .game
            .startup
            .config
            .write(&self.game.startup.install_language);
        if let Err(e) = std::fs::write(&path, data) {
            log::warn!("{}: {e}", path.display());
        }
    }

    /// Started from the game library: Shift until `shift_until` goes back
    /// to it.
    fn started_from_library(&mut self, shift_until: Option<Instant>) {
        self.shift_until = shift_until;
    }
}

impl ApplicationHandler for A3App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (gw, gh) = (self.game.width() as u32, self.game.height() as u32);
        let attrs = Window::default_attributes()
            .with_title("Atlantis III - Atlantis Native")
            .with_inner_size(LogicalSize::new(gw, gh))
            .with_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)));
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                log::error!("no window: {e}");
                el.exit();
                return;
            }
        };
        let surface = softbuffer::Context::new(window.clone())
            .and_then(|context| softbuffer::Surface::new(&context, window.clone()));
        match surface {
            Ok(s) => self.surface = Some(s),
            Err(e) => {
                log::error!("no picture in the window: {e}");
                el.exit();
                return;
            }
        }
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
            WindowEvent::ModifiersChanged(m) => {
                self.alt = m.state().alt_key();
                // Shift in the first moments of an automatic start: the
                // game library.
                let early = self.shift_until.is_some_and(|t| Instant::now() < t);
                if early && m.state().shift_key() {
                    log::info!("Atlantis III stopped: the game library");
                    self.to_library = true;
                    el.exit();
                }
            }
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
                if self.shift_until.is_some_and(|t| Instant::now() >= t) {
                    self.shift_until = None;
                }
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
                let t = Instant::now();
                self.next_frame = (self.next_frame + FRAME).max(t);
            }
            _ => {}
        }
    }
}

/// Opens a window on the game; returns when it is closed.
pub fn run_window(game: Game, fullscreen: bool, no_intro: bool) -> Result<(), String> {
    let el = EventLoop::new().map_err(|e| format!("No window can be opened: {e}"))?;
    let mut app = A3App::new(game, fullscreen, no_intro);
    el.run_app(&mut app)
        .map_err(|e| format!("The window failed: {e}"))
}

/// [`run_window`] for the game started from the game library, on its event
/// loop (a program makes one event loop only): with `shift_grace` (the
/// library's one game, started without its screen) Shift held in the
/// first moments goes back to the library.
pub fn run_window_on(
    el: &mut EventLoop<()>,
    game: Game,
    fullscreen: bool,
    shift_grace: bool,
) -> Result<Ended, String> {
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    let mut app = A3App::new(game, fullscreen, false);
    app.started_from_library(shift_grace.then(|| Instant::now() + SHIFT_GRACE));
    el.run_app_on_demand(&mut app)
        .map_err(|e| format!("The window failed: {e}"))?;
    Ok(if app.to_library {
        Ended::Library
    } else {
        Ended::Quit
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game opens from GOG.com's folder with the player's own folder
    /// for its saves and settings, and starts: the intro, then the start
    /// page.
    #[test]
    fn opens_and_reaches_the_start_page() {
        let Some(dir) = atlantis3_data::testing::install_dir() else {
            return;
        };
        assert_eq!(label(&dir).unwrap(), "Atlantis III, GOG.com, French");
        let base = std::env::temp_dir().join(format!("atlantis-a3-desktop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (mut game, label) = open_in(&dir, &base).unwrap();
        assert_eq!(label, "Atlantis III, GOG.com, French");
        assert_eq!(game.save_dir.as_deref(), Some(base.join("save").as_path()));
        assert_eq!(
            game.menus_mut().cfg_path.as_deref(),
            Some(base.join("Atlantis3.cfg").as_path())
        );
        assert!(game.show_start_page());
        assert_eq!(game.mode(), Mode::Menu);
        let shot = base.join("a3.png");
        headless(&dir, &base, 30, Some(&shot), true).unwrap();
        assert!(std::fs::read(&shot).unwrap().starts_with(b"\x89PNG"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn screenshots_are_numbered() {
        let d = std::env::temp_dir().join(format!("atlantis-a3-shots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(shot_path(&d), d.join("a3-0.png"));
        std::fs::write(d.join("a3-0.png"), b"x").unwrap();
        assert_eq!(shot_path(&d), d.join("a3-1.png"));
        let _ = std::fs::remove_dir_all(&d);
    }
}

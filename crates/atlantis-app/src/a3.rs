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
//! quick saves), `Atlantis3.cfg` (the original's settings, which its
//! options page writes) and `settings.ini` (the port's bug fixes,
//! [`atlantis3_engine::fixes`]; `--original` turns them off) live in the `atlantis3` folder of the per-user
//! data folder, never in the game's folder: the original's `Datas\Atlantis3.cfg`
//! is read until the menus write the player's own.
//!
//! The game folder is GOG.com's installation (`Game.exe` beside `Datas\`),
//! in English, French, German or Spanish: one executable for the four
//! ([`atlantis3_data::exe::KNOWN_BUILDS`]), the language is the data's;
//! the Italian *Atlantis Collection*'s installation (GOG.com's earlier
//! `Game.exe`, Italian data);
//! or the French retail CD's installation (`Atlantis3.exe` beside
//! `Datas\`) or the North American CD's (`Beyond Atlantis 2.exe`) with
//! its three discs (CD images or copies of the CDs) in it,
//! in a folder inside it or in a `--discs <folder>`
//! ([`atlantis3_data::discs`]).
//!
//! Started from the game library, Shift held in the first moments goes
//! back to the library's screen ([`crate::a2::Ended::Library`]), as for
//! the other games.

#[cfg(not(target_os = "android"))]
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use atlantis3_data::files::OpenError;
use atlantis3_data::A3Files;
use atlantis3_engine::px::Surface;
use atlantis3_engine::{Fixes, Game, Mode};
use atlantis_data::{FileSource, StdDir};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey, PhysicalKey};
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

/// The edition of the Atlantis III installation in `dir` with its
/// language, for the game library ("Atlantis III, GOG.com, French").
pub fn label(dir: &Path) -> Result<String, OpenError> {
    A3Files::open_dir(dir).and_then(|(files, _)| files.edition_label())
}

/// The install with `Datas\Atlantis3.cfg` taken from the player's folder
/// once the menus wrote it there (the game's folder stays untouched).
struct WithCfg {
    install: Arc<dyn FileSource>,
    cfg: PathBuf,
    /// The settings file's name in the install (`Datas/Atlantis3.cfg`;
    /// the 2004 program's `datas/Atlantis4.cfg`, as
    /// [`atlantis3_data::evolution2004::Evolution2004Source`] maps it).
    name: &'static str,
}

impl WithCfg {
    fn is_cfg(&self, name: &str) -> bool {
        name.replace('\\', "/").eq_ignore_ascii_case(self.name)
    }
}

impl FileSource for WithCfg {
    fn size(&self, name: &str) -> Option<u64> {
        if self.is_cfg(name) {
            if let Ok(m) = std::fs::metadata(&self.cfg) {
                return Some(m.len());
            }
        }
        self.install.size(name)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        if self.is_cfg(name) {
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

/// The bug fixes of `base`'s `settings.ini` (written with the defaults
/// when there is none); `original` (`--original`) turns them all off
/// without writing the file.
pub fn fixes_in(base: &Path, original: bool) -> Fixes {
    if original {
        return Fixes::original();
    }
    let path = base.join(atlantis3_engine::fixes::SETTINGS_FILE);
    match std::fs::read(&path) {
        Ok(d) => Fixes::parse(&String::from_utf8_lossy(&d)),
        Err(_) => {
            let f = Fixes::default();
            if let Err(e) = std::fs::write(&path, f.to_text()) {
                log::warn!("{}: {e}", path.display());
            }
            f
        }
    }
}

/// The game in `dir`, its saves, `Atlantis3.cfg` and `settings.ini` in
/// `base`; with the edition's name. `original` (`--original`): every bug
/// fix off ([`fixes_in`]).
pub fn open_in(dir: &Path, base: &Path, original: bool) -> Result<(Game, String), String> {
    open_in_with_discs(dir, &[], base, original)
}

/// [`open_in`] with more folders to look for the retail CD's discs in
/// (`--discs <folder>`), beside the game folder and the folders inside
/// it ([`atlantis3_data::discs::CdSource::open`]).
pub fn open_in_with_discs(
    dir: &Path,
    discs: &[PathBuf],
    base: &Path,
    original: bool,
) -> Result<(Game, String), String> {
    let _ = std::fs::create_dir_all(base);
    let fixes = fixes_in(base, original);
    let cfg_path = base.join("Atlantis3.cfg");
    let files = A3Files::with_discs(
        Arc::new(WithCfg {
            install: Arc::new(StdDir::new(dir)),
            cfg: cfg_path.clone(),
            name: atlantis3_data::cfg::CFG_PATH,
        }),
        dir,
        discs,
    );
    open_files_in(files, dir, base, fixes, cfg_path)
}

/// The game from `install` (a folder chosen on Android: its files read
/// through `FileSource`) with the retail CD's discs among `discs`, its
/// saves, `Atlantis3.cfg` and `settings.ini` in `base`; `shown` names the
/// folder in the errors.
#[cfg(target_os = "android")]
pub fn open_source_in(
    install: Arc<dyn FileSource>,
    discs: Vec<(String, Arc<dyn FileSource>)>,
    shown: &Path,
    base: &Path,
) -> Result<(Game, String), String> {
    let _ = std::fs::create_dir_all(base);
    let fixes = fixes_in(base, false);
    let cfg_path = base.join("Atlantis3.cfg");
    let install: Arc<dyn FileSource> = Arc::new(WithCfg {
        install,
        cfg: cfg_path.clone(),
        name: atlantis3_data::cfg::CFG_PATH,
    });
    let files = A3Files::new(install.clone());
    let files = match files.build() {
        Ok(b) if b.release.disc_test() => A3Files::cd(Arc::new(
            atlantis3_data::discs::CdSource::new(install, discs),
        )),
        _ => files,
    };
    open_files_in(files, shown, base, fixes, cfg_path)
}

/// The game from `files` (in `dir`, named in the errors), its saves in
/// `base`, with `fixes`, the menus writing `cfg_path`.
fn open_files_in(
    files: A3Files,
    dir: &Path,
    base: &Path,
    fixes: Fixes,
    cfg_path: PathBuf,
) -> Result<(Game, String), String> {
    let label = files
        .edition_label()
        .map_err(|e| crate::setup::explain_atlantis3(dir, &e))?;
    if let Some(d) = files.discs() {
        let found = d.disc_numbers();
        if found.len() < 3 {
            log::warn!(
                "{label}: discs found {found:?}; the CD release reads its videos and panoramas from its three discs"
            );
        }
        if found.is_empty() {
            return Err(crate::setup::explain_atlantis3_no_disc(dir));
        }
    }
    let mut game = Game::with_fixes(files, fixes).map_err(|e| format!("{}: {e}", dir.display()))?;
    game.save_dir = Some(base.join("save"));
    game.menus_mut().cfg_path = Some(cfg_path);
    log::info!(
        "{label}, {}; saves and settings in {}; {fixes:?}",
        crate::version_string(),
        base.display()
    );
    Ok((game, label))
}

/// The game in `dir`, its saves and settings in [`data_dir`]; the retail
/// CD's discs also looked for in `discs`.
pub fn open(dir: &Path, discs: &[PathBuf], original: bool) -> Result<(Game, String), String> {
    open_in_with_discs(dir, discs, &data_dir(), original)
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
#[allow(clippy::too_many_arguments)]
pub fn headless(
    dir: &Path,
    discs: &[PathBuf],
    base: &Path,
    frames: u64,
    screenshot: Option<&Path>,
    no_intro: bool,
    original: bool,
) -> Result<(), String> {
    let (mut game, label) = open_in_with_discs(dir, discs, base, original)?;
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
    /// Android: the window's buffers at the game's size (android_frame.rs).
    #[cfg(target_os = "android")]
    frame: Option<crate::android_frame::Frame>,
    _audio: Option<cpal::Stream>,
    /// The game's screen size the window was last given.
    shown: (usize, usize),
    /// Until when Shift goes back to the game library (started from it).
    shift_until: Option<Instant>,
    /// The window ended for the game library's screen.
    to_library: bool,
    /// The game was started (its intro, logos or start page), at the
    /// first window.
    started: bool,
    /// Where the game's picture was last drawn in the window: x, y,
    /// width, height (window pixels).
    view: (usize, usize, usize, usize),
    /// Android: the fingers on the screen (see [`A3App::touch`]).
    #[cfg(target_os = "android")]
    touch: TouchA3,
    /// The 2004 program ([`run_evolution_2004`]): its start-up logos and
    /// start page in place of the intro.
    evolution_2004: bool,
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
    pub fn new(mut game: Game, fullscreen: bool, no_intro: bool) -> A3App {
        // The saves' `DateAndTime` in local time.
        game.utc_offset = crate::a2::utc_offset_now;
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
            #[cfg(target_os = "android")]
            frame: None,
            _audio: stream,
            shown,
            shift_until: None,
            to_library: false,
            started: false,
            view: (0, 0, 1, 1),
            #[cfg(target_os = "android")]
            touch: TouchA3::default(),
            evolution_2004: false,
        }
    }

    /// Android: the picture through [`crate::android_frame`], scaled by
    /// the system's composer.
    #[cfg(target_os = "android")]
    fn present(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        if self.frame.is_none() {
            self.frame = crate::android_frame::Frame::new(window);
        }
        let Some(frame) = self.frame.as_mut() else {
            return;
        };
        let size = window.inner_size();
        let (gw, gh) = (self.game.width(), self.game.height());
        self.shown = (gw, gh);
        let window_size = (size.width as usize, size.height as usize);
        // The bag and menu buttons in the bars, as in Atlantis I (not
        // during a cinematic or the credits); where a tap presses them,
        // in the buffer's pixels.
        let shown = !self.game.skippable() && self.game.credits().is_none();
        let mut pressed: [Option<crate::android::Rect>; 2] = [None; 2];
        let bars = |b: crate::android_frame::Bars<'_>| {
            let (lw, lh) = b.size;
            let (ox, _, pw, _) = b.picture;
            let right = ox + pw;
            for (i, (bar, button)) in [
                ((right, lw - right), crate::android::Button::Bag),
                ((0, ox), crate::android::Button::Menu),
            ]
            .into_iter()
            .enumerate()
            {
                let r = crate::android::draw_button(b.px, b.stride, bar, lh, shown, button);
                // (drawn as 0x00RRGGBB: put in the buffer's byte order)
                if let Some((x, y, w, h)) = r {
                    for row in b.px.chunks_exact_mut(b.stride).skip(y).take(h) {
                        for p in &mut row[x..x + w] {
                            *p = crate::android_frame::from_xrgb(*p);
                        }
                    }
                }
                pressed[i] = r;
            }
        };
        let view = match self.game.surface() {
            Surface::Rgb565(screen) => {
                let lut = crate::android_frame::rgb565_lut();
                frame.present(
                    window_size,
                    (gw, gh),
                    |y, out| {
                        for (o, &p) in out.iter_mut().zip(&screen[y * gw..][..gw]) {
                            *o = lut[usize::from(p)];
                        }
                    },
                    bars,
                )
            }
            Surface::Xrgb8888(screen) => frame.present(
                window_size,
                (gw, gh),
                |y, out| {
                    for (o, &p) in out.iter_mut().zip(&screen[y * gw..][..gw]) {
                        *o = crate::android_frame::from_xrgb(p);
                    }
                },
                bars,
            ),
        };
        if let Some(view) = view {
            self.view = view;
            // The buttons' rectangles in window pixels: the buffer's
            // pixel is the picture's (gw across `view.2` window pixels),
            // its picture at the view's place.
            let (vx, vy, vw, vh) = view;
            let (bw, bh) = frame.buffer_size();
            let (box_, boy) = ((bw - gw) / 2, (bh - gh) / 2);
            let to_window = |r: Option<crate::android::Rect>| {
                r.map(|(x, y, w, h)| {
                    let sx = |v: usize| {
                        (vx as f64 + (v as f64 - box_ as f64) * vw as f64 / gw as f64).max(0.0)
                    };
                    let sy = |v: usize| {
                        (vy as f64 + (v as f64 - boy as f64) * vh as f64 / gh as f64).max(0.0)
                    };
                    let (x0, y0, x1, y1) = (sx(x), sy(y), sx(x + w), sy(y + h));
                    (
                        x0 as usize,
                        y0 as usize,
                        (x1 - x0) as usize,
                        (y1 - y0) as usize,
                    )
                })
            };
            self.touch.buttons = [to_window(pressed[0]), to_window(pressed[1])];
        }
    }

    #[cfg(not(target_os = "android"))]
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
        self.view = (ox, oy, dw, dh);
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
    fn write_cfg(&mut self) {
        let Some(path) = self.game.menus().cfg_path.clone() else {
            return;
        };
        let data = self.game.cfg_file_text();
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
    /// Android: the activity's window is gone (the app in the
    /// background); `resumed` makes another.
    #[cfg(target_os = "android")]
    fn suspended(&mut self, _el: &ActiveEventLoop) {
        self.surface = None;
        self.frame = None;
        self.window = None;
        crate::android::pause_audio(&self._audio, true);
    }

    fn resumed(&mut self, el: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        crate::android::pause_audio(&self._audio, false);
        if self.window.is_some() {
            return;
        }
        let (gw, gh) = (self.game.width() as u32, self.game.height() as u32);
        let title = if self.evolution_2004 {
            "Atlantis Evolution (2004) - Atlantis Native"
        } else {
            "Atlantis III - Atlantis Native"
        };
        let attrs = Window::default_attributes()
            .with_title(title)
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
        // Android: a window made again (the app back from the background)
        // goes on where the game was; only the first starts it.
        if self.started {
            self.next_frame = Instant::now();
            return;
        }
        self.started = true;
        self.start = Instant::now();
        self.next_frame = self.start;
        if self.evolution_2004 {
            // Loop mode 1 (FUN_0045e970): the logos, then the start page.
            self.intro_done = true;
            self.game.start_logos_2004();
        } else if self.no_intro {
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
        // Android: the activity is being destroyed.
        #[cfg(target_os = "android")]
        if crate::android::destroyed() {
            el.exit();
            return;
        }
        // Android: Back and the on-screen keyboard's text.
        #[cfg(target_os = "android")]
        if let Some(id) = self.window.as_ref().map(|w| w.id()) {
            let mut typed = crate::android::mode_typed();
            if std::mem::take(&mut self.touch.escape) {
                typed.extend(crate::android::escape_press());
            }
            for e in typed {
                self.window_event(el, id, e);
            }
        }
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
        // Android: the touch screen (the game reads the mouse's moves
        // only, as DirectInput gave them).
        #[cfg(target_os = "android")]
        if let WindowEvent::Touch(t) = &event {
            self.touch(t);
            return;
        }
        // Android: Back as Escape.
        #[cfg(target_os = "android")]
        if let Some(events) = crate::android::mode_events(&event) {
            for e in events {
                self.window_event(el, _id, e);
            }
            return;
        }
        // The 2004 program: every key press (repeats included, as
        // `WM_KEYDOWN`) and release reaches its key handlers and the
        // mini-games' `GetKeyboardState`.
        if let WindowEvent::KeyboardInput { event: k, .. } = &event {
            if self.evolution_2004 {
                if let PhysicalKey::Code(c) = k.physical_key {
                    if let Some(vk) = crate::vk::virtual_key(c) {
                        self.game.key_2004(vk, k.state == ElementState::Pressed);
                    }
                }
            }
        }
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
                    // Alt+Enter: not while a puzzle freezes the keys (@ 0x0042ab20).
                    Key::Named(NamedKey::Enter) if self.alt && !self.game.keys_frozen() => {
                        self.toggle_fullscreen()
                    }
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
            // (none before the game's window: the game starts with it)
            WindowEvent::RedrawRequested if self.window.is_none() => {}
            WindowEvent::RedrawRequested => {
                if self.shift_until.is_some_and(|t| Instant::now() >= t) {
                    self.shift_until = None;
                }
                let now = self.start.elapsed().as_secs_f64();
                // Android: the touch screen's next button action.
                #[cfg(target_os = "android")]
                match self.touch.queue.pop_front() {
                    Some(Press::Down) => self.game.click(),
                    Some(Press::Up) => self.game.release(),
                    Some(Press::Right) => self.game.right_click(),
                    Some(Press::Skip) => self.game.skip(),
                    None => {}
                }
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

/// Android: a finger moving less than this (window pixels) from where it
/// went down is still a tap.
#[cfg(target_os = "android")]
const TAP_SLOP: f64 = 24.0;

/// Android: the fingers on the screen in Atlantis III.
#[cfg(target_os = "android")]
#[derive(Default)]
struct TouchA3 {
    fingers: Vec<u64>,
    /// The first finger, where it went down and where it was last.
    primary: Option<u64>,
    from: (f64, f64),
    last: (f64, f64),
    /// It moved past [`TAP_SLOP`].
    moved: bool,
    /// A second finger came: a right click when they lift.
    two: bool,
    /// Where a tap presses the bag and the menu buttons (window pixels),
    /// as last drawn.
    buttons: [Option<crate::android::Rect>; 2],
    /// The first finger went down on the bag (0) or menu (1) button.
    on_button: Option<usize>,
    /// The menu button was tapped: Escape for the game, as Back.
    escape: bool,
    /// The left button is held in the game (a free pointer's press).
    pressed: bool,
    /// The game's button actions, one per frame (a press is seen after the
    /// pointer's move, as the menus hit-test the pointer of the frame
    /// before; a release after its press).
    queue: std::collections::VecDeque<Press>,
}

/// Android: a button action queued for the game.
#[cfg(target_os = "android")]
#[derive(Clone, Copy)]
enum Press {
    Down,
    Up,
    Right,
    /// Space: the cinematic stops ([`Game::skip`]).
    Skip,
}

#[cfg(target_os = "android")]
impl A3App {
    /// Whether the game's pointer is free on the screen (the menus, the
    /// inventory, the subject menu, a dialog, a full-screen puzzle): a
    /// finger puts it where it is. Elsewhere (the panorama) a drag turns
    /// the view and a tap clicks where the view points.
    fn free_pointer(&self) -> bool {
        matches!(
            self.game.mode(),
            Mode::Menu | Mode::Inventory | Mode::Subjects | Mode::Dialog | Mode::Screen
        )
    }

    /// Window point (x, y) as the game's screen point.
    fn to_game(&self, (x, y): (f64, f64)) -> (i32, i32) {
        let (ox, oy, dw, dh) = self.view;
        let (gw, gh) = (self.game.width() as f64, self.game.height() as f64);
        let gx = ((x - ox as f64) * gw / dw.max(1) as f64).floor();
        let gy = ((y - oy as f64) * gh / dh.max(1) as f64).floor();
        (
            (gx as i32).clamp(0, gw as i32 - 1),
            (gy as i32).clamp(0, gh as i32 - 1),
        )
    }

    /// The pointer moved to window point `at` (the free pointer).
    fn point_at(&mut self, at: (f64, f64)) {
        let (gx, gy) = self.to_game(at);
        let (px, py) = self.game.pointer();
        self.game.mouse_motion(gx - px, gy - py);
    }

    /// A finger on the screen: on a free pointer a press there (moving it
    /// while down); on the panorama a drag turns the view, a tap clicks;
    /// two fingers a right click.
    fn touch(&mut self, t: &winit::event::Touch) {
        use winit::event::TouchPhase;
        let at = (t.location.x, t.location.y);
        match t.phase {
            TouchPhase::Started => {
                self.touch.fingers.retain(|f| *f != t.id);
                self.touch.fingers.push(t.id);
                if self.touch.fingers.len() == 1 {
                    self.touch.primary = Some(t.id);
                    self.touch.from = at;
                    self.touch.last = at;
                    self.touch.moved = false;
                    self.touch.two = false;
                    self.touch.on_button = self.touch.buttons.iter().position(|r| {
                        r.is_some_and(|(x, y, w, h)| {
                            at.0 >= x as f64
                                && at.1 >= y as f64
                                && at.0 < (x + w) as f64
                                && at.1 < (y + h) as f64
                        })
                    });
                    if self.touch.on_button.is_some() {
                        // (a button: nothing for the game until it lifts)
                    } else if self.free_pointer() {
                        self.point_at(at);
                        self.touch.queue.push_back(Press::Down);
                        self.touch.pressed = true;
                    }
                } else if !self.touch.two {
                    self.touch.two = true;
                    if std::mem::take(&mut self.touch.pressed) {
                        self.touch.queue.push_back(Press::Up);
                    }
                }
            }
            TouchPhase::Moved => {
                if self.touch.primary != Some(t.id)
                    || self.touch.two
                    || self.touch.on_button.is_some()
                {
                    return;
                }
                let (lx, ly) = self.touch.last;
                self.touch.last = at;
                let (fx, fy) = self.touch.from;
                if (at.0 - fx).hypot(at.1 - fy) > TAP_SLOP {
                    self.touch.moved = true;
                }
                if self.touch.pressed {
                    self.point_at(at);
                } else if self.touch.moved {
                    // The view turns with the finger, as with the mouse
                    // (the game's pixels as mickeys).
                    let k = self.game.width() as f64 / self.view.2.max(1) as f64;
                    let (dx, dy) = ((at.0 - lx) * k, (at.1 - ly) * k);
                    self.game.mouse_motion(dx.round() as i32, dy.round() as i32);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.touch.fingers.retain(|f| *f != t.id);
                let button = self.touch.on_button.take();
                if self.touch.primary == Some(t.id) && !self.touch.two && button.is_some() {
                    // A button lifted where it went down: the bag (the
                    // right button, as two fingers) or the menu (Escape).
                    if t.phase == TouchPhase::Ended {
                        match button {
                            Some(0) => self.touch.queue.push_back(Press::Right),
                            _ => self.touch.escape = true,
                        }
                    }
                } else if self.touch.primary == Some(t.id) && !self.touch.two {
                    if std::mem::take(&mut self.touch.pressed) {
                        self.touch.queue.push_back(Press::Up);
                    } else if !self.touch.moved
                        && t.phase == TouchPhase::Ended
                        && self.game.skippable()
                    {
                        // A tap during a cinematic: Space, the key that
                        // stops it (the phone has no keyboard; the
                        // original's click does not stop it).
                        self.touch.queue.push_back(Press::Skip);
                    } else if !self.touch.moved && t.phase == TouchPhase::Ended {
                        // A tap on the panorama: a click where it points.
                        self.touch.queue.push_back(Press::Down);
                        self.touch.queue.push_back(Press::Up);
                    }
                }
                if self.touch.fingers.is_empty() {
                    if self.touch.two && t.phase == TouchPhase::Ended {
                        self.touch.queue.push_back(Press::Right);
                    }
                    self.touch.primary = None;
                    self.touch.two = false;
                }
            }
        }
    }
}

/// *Atlantis Evolution*'s 2004 program (`--evolution2004 <install>`, the
/// development entry; the game library and a bare install folder on the
/// command line run it as well, [`run_evolution_2004_on`]; atlantis4-re
/// decision 0008): the CD install read through
/// [`atlantis3_data::evolution2004::Evolution2004Source`], its saves and
/// settings in the player's `atlantis-evolution-2004` folder (its saves
/// in `save/`, its `Atlantis4.cfg` read from there once the menus wrote
/// it); the start-up's logos, then the start page. Opens a window on the
/// game; returns when it is closed.
pub fn run_evolution_2004(dir: &Path, fullscreen: bool) -> Result<(), String> {
    let game = open_evolution_2004(dir, &evolution_2004_data_dir())?;
    let el = EventLoop::new().map_err(|e| format!("No window can be opened: {e}"))?;
    let mut app = A3App::new(game, fullscreen, true);
    app.evolution_2004 = true;
    el.run_app(&mut app)
        .map_err(|e| format!("The window failed: {e}"))
}

/// [`run_evolution_2004`] for the game started from the game library (or
/// its folder given alone on the command line), on its event loop: with
/// `shift_grace` Shift held in the first moments goes back to the library,
/// as for the other games.
pub fn run_evolution_2004_on(
    el: &mut EventLoop<()>,
    dir: &Path,
    fullscreen: bool,
    shift_grace: bool,
) -> Result<Ended, String> {
    let game = open_evolution_2004(dir, &evolution_2004_data_dir())?;
    run_evolution_2004_game_on(el, game, fullscreen, shift_grace)
}

/// [`run_evolution_2004_on`] for a game already opened (Android: from a
/// chosen folder, [`open_evolution_2004_in`]).
pub fn run_evolution_2004_game_on(
    el: &mut EventLoop<()>,
    game: Game,
    fullscreen: bool,
    shift_grace: bool,
) -> Result<Ended, String> {
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    let mut app = A3App::new(game, fullscreen, true);
    app.evolution_2004 = true;
    app.started_from_library(shift_grace.then(|| Instant::now() + SHIFT_GRACE));
    el.run_app_on_demand(&mut app)
        .map_err(|e| format!("The window failed: {e}"))?;
    Ok(if app.to_library {
        Ended::Library
    } else {
        Ended::Quit
    })
}

pub use atlantis3_data::evolution2004::holds_evolution_2004;

/// The edition of the 2004 program's install in `dir`, for the game
/// library and the window ("Atlantis Evolution (2004), CD, English",
/// atlantis4-re decision 0008): its `Atlantis Evolution.exe` identified by
/// size, PE timestamp and MD5
/// ([`atlantis3_data::evolution2004::KNOWN_BUILDS`]).
pub fn evolution_2004_label(dir: &Path) -> Result<String, atlantis3_data::evolution2004::ExeError> {
    use atlantis3_data::evolution2004::{edition_label, identify_dir, ExeError};
    match identify_dir(dir) {
        Some(build) => build.map(edition_label),
        None => Err(ExeError::NotPe),
    }
}

/// [`headless`] for the 2004 program: the start-up's logos (skipped with
/// `skip_logos`), then the start page, `frames` frames of 1/60 s, the
/// saves in `base`; the screen to `screenshot` when given.
pub fn headless_evolution_2004(
    dir: &Path,
    base: &Path,
    frames: u64,
    screenshot: Option<&Path>,
    skip_logos: bool,
) -> Result<(), String> {
    let label = evolution_2004_label(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut game = open_evolution_2004(dir, base)?;
    println!("{label}, {}", crate::version_string());
    game.start_logos_2004();
    let mut sound = vec![0.0f32; 2 * (22050 / 60)];
    for i in 0..frames {
        if skip_logos && game.logos_running_2004() {
            game.skip();
        }
        game.step(i as f64 / 60.0);
        game.audio.fill(&mut sound, 22050, 2);
        if game.quit {
            break;
        }
    }
    println!(
        "{frames} frames, mode {:?}, {}x{}, logos {}",
        game.mode(),
        game.width(),
        game.height(),
        if game.logos_running_2004() {
            "running"
        } else {
            "done"
        }
    );
    if let Some(path) = screenshot {
        std::fs::write(path, png(&game)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("screenshot: {}", path.display());
    }
    Ok(())
}

/// The 2004 program's folder in the per-user data folder.
pub fn evolution_2004_data_dir() -> PathBuf {
    crate::desktop::data_dir().join("atlantis-evolution-2004")
}

/// Why the 2004 program's install in `dir` cannot be played to its end,
/// for the player: disc 2's world data missing (the Italian *Atlantis
/// Collection*'s installer leaves them out,
/// [`atlantis3_data::evolution2004::check_disc2`]); `None` when they are
/// there or the program is not a known build.
pub fn evolution_2004_incomplete(dir: &Path) -> Option<String> {
    evolution_2004_incomplete_in(&StdDir::new(dir), dir)
}

/// [`evolution_2004_incomplete`] for the installation read through
/// `install` (Android: a chosen folder), named `shown` in the message.
fn evolution_2004_incomplete_in(install: &dyn FileSource, shown: &Path) -> Option<String> {
    use atlantis3_data::evolution2004::{check_disc2, identify_install};
    let build = identify_install(install)?.ok()?;
    check_disc2(install, build).map(|e| format!("{}: {e}", shown.display()))
}

/// The 2004 program's game in `dir`, its saves and `Atlantis4.cfg` in
/// `base` (the install's `datas\Atlantis4.cfg` until the menus write the
/// player's own); refused when disc 2's world data are missing
/// ([`evolution_2004_incomplete`]).
pub fn open_evolution_2004(dir: &Path, base: &Path) -> Result<Game, String> {
    open_evolution_2004_in(Arc::new(StdDir::new(dir)), dir, base)
}

/// [`open_evolution_2004`] for the installation read through `install`
/// (Android: a chosen folder), named `shown` in the messages.
pub fn open_evolution_2004_in(
    install: Arc<dyn FileSource>,
    shown: &Path,
    base: &Path,
) -> Result<Game, String> {
    use atlantis3_data::evolution2004::Evolution2004Source;
    if let Some(why) = evolution_2004_incomplete_in(&*install, shown) {
        return Err(why);
    }
    let _ = std::fs::create_dir_all(base);
    let cfg_path = base.join("Atlantis4.cfg");
    let src = Evolution2004Source::new(Arc::new(WithCfg {
        install,
        cfg: cfg_path.clone(),
        name: "datas/Atlantis4.cfg",
    }));
    let files = A3Files::evolution_2004(Arc::new(src));
    let mut game = Game::new(files).map_err(|e| format!("{}: {e}", shown.display()))?;
    game.save_dir = Some(base.join("save"));
    game.menus_mut().cfg_path = Some(cfg_path);
    log::info!(
        "Atlantis Evolution (2004), {}; saves and settings in {}",
        crate::version_string(),
        base.display()
    );
    Ok(game)
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
        let (mut game, label) = open_in(&dir, &base, false).unwrap();
        assert_eq!(label, "Atlantis III, GOG.com, French");
        assert_eq!(game.save_dir.as_deref(), Some(base.join("save").as_path()));
        assert_eq!(
            game.menus_mut().cfg_path.as_deref(),
            Some(base.join("Atlantis3.cfg").as_path())
        );
        assert!(game.show_start_page());
        assert_eq!(game.mode(), Mode::Menu);
        let shot = base.join("a3.png");
        headless(&dir, &[], &base, 30, Some(&shot), true, false).unwrap();
        assert!(std::fs::read(&shot).unwrap().starts_with(b"\x89PNG"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// *Atlantis Evolution*'s Italian *Atlantis Collection* install
    /// (`ATLANTIS4_IT_COLLECTION`): named in the library, refused as its
    /// installer leaves it (disc 2's world data missing), in Italian, and,
    /// with the English CD's install's `cd2` world folders beside its own
    /// (`ATLANTIS4_CD`, linked into a scratch folder as a player's merged
    /// copy has them), opened and started to the start page.
    #[test]
    fn the_italian_collection_of_evolution_2004() {
        let Some(dir) = atlantis3_data::testing::a4_it_collection() else {
            return;
        };
        let label = "Atlantis Evolution (2004), Atlantis Collection, Italian";
        assert_eq!(crate::a4::label(&dir).unwrap(), label);
        assert_eq!(evolution_2004_label(&dir).unwrap(), label);
        let why = evolution_2004_incomplete(&dir).expect("disc 2 missing");
        assert!(
            why.contains(label) && why.contains("English 2004 CD"),
            "{why}"
        );
        let base = std::env::temp_dir().join(format!("atlantis-a4it-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(
            open_evolution_2004(&dir, &base.join("data")).err(),
            Some(why)
        );
        // The registry's `Language` = `ita` (from `loc\\text\\`): texts and
        // voices in Italian.
        let src =
            atlantis3_data::evolution2004::Evolution2004Source::new(Arc::new(StdDir::new(&dir)));
        let startup =
            atlantis3_engine::startup::Startup::read(&A3Files::evolution_2004(Arc::new(src)));
        assert_eq!(
            (startup.text_language(), startup.voice_language()),
            ("ita".to_string(), "ita".to_string())
        );
        #[cfg(unix)]
        {
            let Some(cd) = std::env::var_os("ATLANTIS4_CD").map(PathBuf::from) else {
                eprintln!("SKIPPED the_italian_collection_of_evolution_2004 (merged): ATLANTIS4_CD not set");
                let _ = std::fs::remove_dir_all(&base);
                return;
            };
            let link = |t: &Path, l: &Path| {
                std::fs::create_dir_all(l.parent().unwrap()).unwrap();
                std::os::unix::fs::symlink(t, l).unwrap();
            };
            let merged = base.join("install");
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                if e.file_name() != "cd2" {
                    link(&e.path(), &merged.join(e.file_name()));
                }
            }
            link(&dir.join("cd2/datas/loc"), &merged.join("cd2/datas/loc"));
            for f in atlantis3_data::evolution2004::DISC2_FOLDERS {
                let theirs = std::fs::read_dir(cd.join("cd2/datas"))
                    .unwrap()
                    .flatten()
                    .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(f))
                    .unwrap_or_else(|| panic!("the CD's cd2/datas/{f}"));
                link(
                    &theirs.path(),
                    &merged.join("cd2/datas").join(theirs.file_name()),
                );
            }
            assert_eq!(evolution_2004_incomplete(&merged), None);
            assert_eq!(crate::a4::label(&merged).unwrap(), label);
            let mut game = open_evolution_2004(&merged, &base.join("data")).unwrap();
            game.start_logos_2004();
            let mut t = 0.0;
            while game.logos_running_2004() && t < 60.0 {
                game.skip();
                t += 0.04;
                game.step(t);
            }
            for _ in 0..25 {
                t += 0.04;
                game.step(t);
            }
            assert!(!game.logos_running_2004());
            assert_eq!(game.mode(), Mode::Menu);
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// *Atlantis Evolution CZ*, the Czech *Level* 153 DVD's install
    /// (`ATLANTIS4_DVD_CS`): named in the library by its install (the
    /// Italian collection's program), complete, its texts Czech and its
    /// voices English, opened and started to the start page.
    #[test]
    fn the_czech_level_dvd_of_evolution_2004() {
        let Some(dir) = atlantis3_data::testing::a4_dvd_cs() else {
            return;
        };
        let label = "Atlantis Evolution (2004), Level DVD (CINEMAX), Czech";
        assert_eq!(crate::a4::label(&dir).unwrap(), label);
        assert_eq!(evolution_2004_label(&dir).unwrap(), label);
        assert_eq!(evolution_2004_incomplete(&dir), None);
        // The registry's `Language` = `gbr`, `LanguageST` = `tch`, no
        // `Atlantis4.cfg`: the defaults.
        let src =
            atlantis3_data::evolution2004::Evolution2004Source::new(Arc::new(StdDir::new(&dir)));
        let startup =
            atlantis3_engine::startup::Startup::read(&A3Files::evolution_2004(Arc::new(src)));
        assert_eq!(
            (startup.text_language(), startup.voice_language()),
            ("tch".to_string(), "gbr".to_string())
        );
        assert_eq!(startup.install_language_st, b"tch");
        assert_eq!(startup.config.resolution, (1024, 768));
        assert!(startup.config.subtitle_on && startup.config.full_screen);
        let base = std::env::temp_dir().join(format!("atlantis-a4cs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut game = open_evolution_2004(&dir, &base.join("data")).unwrap();
        game.start_logos_2004();
        let mut t = 0.0;
        while game.logos_running_2004() && t < 60.0 {
            game.skip();
            t += 0.04;
            game.step(t);
        }
        for _ in 0..25 {
            t += 0.04;
            game.step(t);
        }
        assert!(!game.logos_running_2004());
        assert_eq!(game.mode(), Mode::Menu);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The French CD's installed folder: its edition, refused without its
    /// discs, opened with them (`--discs`) and started to the start page.
    #[test]
    fn the_french_cd_with_its_discs() {
        let Some((dir, discs)) = atlantis3_data::testing::cd_fr() else {
            return;
        };
        assert_eq!(label(&dir).unwrap(), "Atlantis III, CD, French");
        assert!(holds_atlantis3(&dir));
        let base = std::env::temp_dir().join(format!("atlantis-a3-cd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        if atlantis3_data::discs::disc_candidates(&dir).is_empty() {
            let e = open_in(&dir, &base, false).err().unwrap();
            assert!(e.contains("none of its discs"), "{e}");
        }
        let (mut game, label) =
            open_in_with_discs(&dir, std::slice::from_ref(&discs), &base, false).unwrap();
        assert_eq!(label, "Atlantis III, CD, French");
        assert_eq!(
            game.files().discs().map(|d| d.disc_numbers()),
            Some(vec![1, 2, 3])
        );
        assert!(game.show_start_page());
        assert_eq!(game.mode(), Mode::Menu);
        let shot = base.join("a3cd.png");
        headless(&dir, &[discs], &base, 30, Some(&shot), true, false).unwrap();
        assert!(std::fs::read(&shot).unwrap().starts_with(b"\x89PNG"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The North American CD (*Beyond Atlantis II*): its installed folder
    /// (`Beyond Atlantis 2.exe`) refused without its discs, opened with
    /// the folder of its three `.bin` images and started to the start
    /// page.
    #[test]
    fn the_north_american_cd_with_its_discs() {
        let Some((dir, discs)) = atlantis3_data::testing::cd_us() else {
            return;
        };
        let name = "Atlantis III, CD, English (DreamCatcher, 2001)";
        assert_eq!(label(&dir).unwrap(), name);
        assert!(holds_atlantis3(&dir));
        let base = std::env::temp_dir().join(format!("atlantis-a3-cdus-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let e = open_in(&dir, &base, false).err().unwrap();
        assert!(e.contains("none of its discs"), "{e}");
        let (mut game, label) =
            open_in_with_discs(&dir, std::slice::from_ref(&discs), &base, false).unwrap();
        assert_eq!(label, name);
        assert_eq!(
            game.files().discs().map(|d| d.disc_numbers()),
            Some(vec![1, 2, 3])
        );
        assert!(game.show_start_page());
        assert_eq!(game.mode(), Mode::Menu);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The Portuguese CD (*O Novo Mundo*): its installed folder refused
    /// without its discs, opened with the folder of its three CloneCD
    /// images and started to the start page.
    #[test]
    fn the_portuguese_cd_with_its_discs() {
        let Some((dir, discs)) = atlantis3_data::testing::cd_pt() else {
            return;
        };
        let name = "Atlantis III, Portuguese CD, Portuguese (Playgames, 2002)";
        assert_eq!(label(&dir).unwrap(), name);
        assert!(holds_atlantis3(&dir));
        let base = std::env::temp_dir().join(format!("atlantis-a3-cdpt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let e = open_in(&dir, &base, false).err().unwrap();
        assert!(e.contains("none of its discs"), "{e}");
        let (mut game, label) =
            open_in_with_discs(&dir, std::slice::from_ref(&discs), &base, false).unwrap();
        assert_eq!(label, name);
        assert_eq!(
            game.files().discs().map(|d| d.disc_numbers()),
            Some(vec![1, 2, 3])
        );
        assert!(game.show_start_page());
        assert_eq!(game.mode(), Mode::Menu);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The Swedish CD (*Den nya världen*): its installed folder refused
    /// without its discs, opened with the folder of its three CloneCD
    /// images and started to the start page.
    #[test]
    fn the_swedish_cd_with_its_discs() {
        let Some((dir, discs)) = atlantis3_data::testing::cd_sv() else {
            return;
        };
        let name = "Atlantis III, Swedish CD, Swedish (Toptronics, 2002)";
        assert_eq!(label(&dir).unwrap(), name);
        assert!(holds_atlantis3(&dir));
        let base = std::env::temp_dir().join(format!("atlantis-a3-cdsv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let e = open_in(&dir, &base, false).err().unwrap();
        assert!(e.contains("none of its discs"), "{e}");
        let (mut game, label) =
            open_in_with_discs(&dir, std::slice::from_ref(&discs), &base, false).unwrap();
        assert_eq!(label, name);
        assert_eq!(
            game.files().discs().map(|d| d.disc_numbers()),
            Some(vec![1, 2, 3])
        );
        assert!(game.show_start_page());
        assert_eq!(game.mode(), Mode::Menu);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The Traditional Chinese CD (*亞特蘭提斯Ⅲ 失落的帝國*): its installed folder refused
    /// without its discs, opened with the folder of its three CloneCD
    /// images and started to the start page; its texts measured in Big5.
    #[test]
    fn the_traditional_chinese_cd_with_its_discs() {
        let Some((dir, discs)) = atlantis3_data::testing::cd_zh_tw() else {
            return;
        };
        let name = "Atlantis III, Taiwanese CD, Traditional Chinese (Interwise, 2002)";
        assert_eq!(label(&dir).unwrap(), name);
        assert!(holds_atlantis3(&dir));
        let base = std::env::temp_dir().join(format!("atlantis-a3-cdzhtw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let e = open_in(&dir, &base, false).err().unwrap();
        assert!(e.contains("none of its discs"), "{e}");
        let (mut game, label) =
            open_in_with_discs(&dir, std::slice::from_ref(&discs), &base, false).unwrap();
        assert_eq!(label, name);
        assert_eq!(
            game.files().discs().map(|d| d.disc_numbers()),
            Some(vec![1, 2, 3])
        );
        assert!(game.show_start_page());
        assert_eq!(game.mode(), Mode::Menu);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The Czech CD (GAME4U Speciál 10): its installed folder refused
    /// without its discs, opened with the folder of its three CloneCD
    /// images and started to the start page.
    #[test]
    fn the_czech_cd_with_its_discs() {
        let Some((dir, discs)) = atlantis3_data::testing::cd_cs() else {
            return;
        };
        let name = "Atlantis III, Czech CD, Czech (GAME4U Speciál 10, 2003)";
        assert_eq!(label(&dir).unwrap(), name);
        assert!(holds_atlantis3(&dir));
        let base = std::env::temp_dir().join(format!("atlantis-a3-cdcs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let e = open_in(&dir, &base, false).err().unwrap();
        assert!(e.contains("none of its discs"), "{e}");
        let (mut game, label) =
            open_in_with_discs(&dir, std::slice::from_ref(&discs), &base, false).unwrap();
        assert_eq!(label, name);
        assert_eq!(
            game.files().discs().map(|d| d.disc_numbers()),
            Some(vec![1, 2, 3])
        );
        assert!(game.show_start_page());
        assert_eq!(game.mode(), Mode::Menu);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The Italian *Atlantis Collection*'s installed folder: its edition,
    /// started to the start page; the player's `Atlantis3.cfg` read with
    /// `fix_cfg_name`, the defaults with `--original` (the program's
    /// reader opens `Atlantis4.cfg`).
    #[test]
    fn the_italian_collection() {
        let Some(dir) = atlantis3_data::testing::it_collection() else {
            return;
        };
        assert_eq!(
            label(&dir).unwrap(),
            "Atlantis III, Atlantis Collection, Italian"
        );
        assert!(holds_atlantis3(&dir));
        let base = std::env::temp_dir().join(format!("atlantis-a3-itcoll-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("Atlantis3.cfg"), "Resolution\t= 1024 768\r\n").unwrap();
        let (mut game, label) = open_in(&dir, &base, false).unwrap();
        assert_eq!(label, "Atlantis III, Atlantis Collection, Italian");
        assert_eq!(game.startup.config.resolution, (1024, 768));
        assert_eq!(&game.startup.config.langue[..], b"ita");
        assert!(game.show_start_page());
        assert_eq!(game.mode(), Mode::Menu);
        let (game, _) = open_in(&dir, &base, true).unwrap();
        assert_eq!(game.startup.config.resolution, (800, 600));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `settings.ini`: written with every fix on when missing, read back,
    /// and `--original` turns the fixes off without writing it.
    #[test]
    fn fixes_from_settings_ini() {
        let d = std::env::temp_dir().join(format!("atlantis-a3-fixes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(fixes_in(&d, true), Fixes::original());
        assert!(!d.join("settings.ini").exists());
        assert_eq!(fixes_in(&d, false), Fixes::default());
        assert!(std::fs::read_to_string(d.join("settings.ini"))
            .unwrap()
            .contains("fix_warpres_cd = 1"));
        std::fs::write(d.join("settings.ini"), "fix_warpres_cd = off\n").unwrap();
        assert!(!fixes_in(&d, false).warpres_cd);
        let _ = std::fs::remove_dir_all(&d);
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

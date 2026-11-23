//! *Atlantis V* (*The Secrets of Atlantis: The Sacred Legacy*, 2006), the
//! development entry (`--atlantis5 <disc, image or folder>`): the data
//! opened in any form the player has them
//! ([`atlantis3_data::atlantis5::Atlantis5Source`]), then the new game's
//! game played from a new game: the player's own scripts run
//! ([`atlantis5_script`] through [`atlantis3_engine::atlantis5_play`]).
//! As in the original, the mouse turns the view (its relative moves, the
//! system cursor hidden and kept in the window) and a left click acts on
//! what is under the screen centre; Escape or Space ends a film, Space a
//! dialog, a left click a tirade; the right button opens the inventory or
//! puts the hand's object back; Escape in play opens the in-game menu; F11
//! switches between the window and the full screen (the port's own). The
//! game starts in the main menu; in the menus the mouse moves the game's
//! cursor. Its `quit_game`, or closing the window, ends the program. The
//! saves go to the data folder's `atlantis5/saves`. In play the game draws
//! its own cursor at the screen centre (the system cursor stays hidden).
//!
//! On Android (docs/ANDROID.md) the data come from a folder chosen in the
//! system's picker ([`run_source_on`]), the picture goes through
//! `android_frame` with the bag and menu buttons in its black bars, and
//! the touch screen stands for the mouse (`A5App::touch`).

#[cfg(not(target_os = "android"))]
use std::num::NonZeroU32;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{View, FIRST_VIEW, VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_play::Play;
use atlantis3_engine::audio::Audio;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

/// The data at `path` and what they are, for people: the build of the
/// program beside them (when there is one), the form, the languages.
///
/// The disc's cabinets keep their checkpoint indexes in the player's data
/// folder ([`cab_index_dir`]): a run after the first starts its reads
/// deep in the disc's large folders without decoding them from their
/// start.
pub fn open(path: &Path) -> Result<(Atlantis5Source, String), String> {
    atlantis_data::cab::set_index_dir(Some(cab_index_dir()));
    let src = Atlantis5Source::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let build = match src.build() {
        Some(Ok(b)) => b.label.to_string(),
        Some(Err(e)) => e.to_string(),
        None => "The Secrets of Atlantis (2006), no program beside the data".to_string(),
    };
    let label = format!(
        "{build}; {:?}; languages {}",
        src.form(),
        src.languages().join(", ")
    );
    Ok((src, label))
}

/// How the game library shows the data at `path`: the game's name, the
/// year, the medium and the language ("The Secrets of Atlantis : L'Héritage
/// sacré (2006), DVD, French"); with the DVD's image, "DVD image"; with
/// an install or the extracted cabinets, "DVD, installed"; with the
/// copy of the disc's files made by [`crate::a5_copy`], "DVD, copied to
/// this computer".
pub fn label(path: &Path) -> Result<String, atlantis3_data::atlantis5::OpenError> {
    edition(path).map(|e| e.0)
}

/// [`label`], and whether the data are read from the disc (a drive or a
/// copy of its files elsewhere than [`crate::a5_copy::copy_dir`], or its
/// image): what the game library offers to copy to the computer.
pub fn edition(path: &Path) -> Result<(String, bool), atlantis3_data::atlantis5::OpenError> {
    let src = Atlantis5Source::open(path)?;
    let copy = crate::a5_copy::is_copy(path);
    Ok(edition_of(&src, copy))
}

/// [`edition`] of the data opened in `src` (`copy`: they are the copy
/// [`crate::a5_copy`] made): the label, and whether they are read from
/// the disc.
pub fn edition_of(src: &Atlantis5Source, copy: bool) -> (String, bool) {
    use atlantis3_data::atlantis5::Form;
    let medium = match src.form() {
        Form::DiscFolder if copy => "DVD, copied to this computer",
        Form::DiscFolder => "DVD",
        Form::DiscImage => "DVD image",
        Form::Tree => "DVD, installed",
    };
    let lang = language(src);
    let mut lang_name = String::new();
    let mut chars = lang.chars();
    if let Some(c) = chars.next() {
        lang_name.extend(c.to_uppercase());
        lang_name.extend(chars);
    }
    let label = format!(
        "{} (2006), {medium}, {lang_name}",
        crate::setup::GameKind::Atlantis5.name()
    );
    (label, src.form() != Form::Tree && !copy)
}

/// The first view's picture (1024 × 576 x8r8g8b8).
pub fn first_view(src: &Atlantis5Source) -> Result<Vec<u32>, String> {
    let view = View::load(src, &FIRST_VIEW).map_err(|e| e.to_string())?;
    log::info!(
        "{} view {}: {} with {} sprites",
        view.scene,
        view.view,
        view.panorama,
        view.sprites.len()
    );
    Ok(view.render(true))
}

/// Without a window: prints what the data are and plays a new game until
/// the first view (its films ended at once); with `screenshot`, writes it
/// there.
pub fn headless(path: &Path, screenshot: Option<&Path>) -> Result<(), String> {
    let (src, label) = open(path)?;
    println!("{label}, {}", crate::version_string());
    let lang = language(&src);
    let src: Arc<dyn atlantis_data::FileSource> = Arc::new(src);
    let mut play = Play::new(src, &lang, None)?;
    play.new_game();
    for _ in 0..600 {
        play.update(1.0 / 30.0);
        if play.stage().film_playing() {
            play.skip();
        }
        if play.game.state() == 4 && !play.game.busy() {
            break;
        }
    }
    let (scene, view) = play
        .stage()
        .address()
        .ok_or_else(|| "no view after the new game".to_string())?;
    println!(
        "first view: {} view {view}, {VIEW_WIDTH}x{VIEW_HEIGHT}",
        String::from_utf8_lossy(&scene)
    );
    if let Some(p) = screenshot {
        let mut pic = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
        play.render(&mut pic);
        let png = atlantis2_engine::png::encode_xrgb8888(&pic, VIEW_WIDTH, VIEW_HEIGHT);
        std::fs::write(p, png).map_err(|e| format!("{}: {e}", p.display()))?;
        println!("screenshot: {}", p.display());
    }
    Ok(())
}

/// The language of the films' voices: French when the data have it, else
/// the first.
fn language(src: &Atlantis5Source) -> String {
    let langs = src.languages();
    langs
        .iter()
        .find(|l| l.eq_ignore_ascii_case("french"))
        .or_else(|| langs.first())
        .cloned()
        .unwrap_or_else(|| "french".to_string())
}

/// What the game needs before it starts, while the start's first logo is
/// read in the background ([`start`]).
struct Start {
    src: Arc<Atlantis5Source>,
    lang: String,
    audio: Audio,
    warm: std::thread::JoinHandle<()>,
    /// How far the films' folder is decoded (the bar on the black
    /// screen): (decoded up to, the logo's offset in it).
    progress: Arc<(AtomicU64, AtomicU64)>,
}

/// The game made and booted once the logos are read; meanwhile the window
/// is black, with a bar ([`crate::a5_bar`]). From a disc the first run
/// reads (and decodes) the cabinet's `Videos` folder up to the logos,
/// about 460 MB with the French DVD, a step at a time: the port's own,
/// the original plays from its install. Later runs start from the kept
/// checkpoints, at once.
fn start(src: Arc<Atlantis5Source>, lang: String, audio: Audio) -> Start {
    let s = src.clone();
    let progress = Arc::new((AtomicU64::new(0), AtomicU64::new(0)));
    let p = progress.clone();
    let warm = std::thread::spawn(move || {
        let t = Instant::now();
        for logo in atlantis3_engine::atlantis5_play::LOGOS {
            let name = format!("datas/videos/{logo}.bik");
            while let Some((at, to)) = s.warm_progress(&name, WARM_STEP) {
                p.0.store(at, Ordering::Relaxed);
                p.1.store(to, Ordering::Relaxed);
                if at >= to {
                    break;
                }
            }
            let _ = atlantis_data::FileSource::read(&*s, &name);
        }
        log::info!(
            "the films' folder read up to the logos in {:.1} s ({} MB)",
            t.elapsed().as_secs_f64(),
            p.1.load(Ordering::Relaxed) >> 20
        );
    });
    Start {
        src,
        lang,
        audio,
        warm,
        progress,
    }
}

/// The decoding's step before the game starts (uncompressed bytes).
const WARM_STEP: u64 = 4 << 20;

struct A5App {
    play: Option<Play>,
    start: Option<Start>,
    picture: Vec<u32>,
    title: String,
    fullscreen: bool,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    last: Instant,
    /// Whether the window has the focus (the mouse's moves turn the view
    /// only then).
    focused: bool,
    /// Whether the mouse is free for a menu.
    menu_cursor: bool,
    _audio: Option<cpal::Stream>,
    /// Why the game could not be made, for the caller.
    error: Option<String>,
    /// Android: the window's buffers at the picture's size
    /// (android_frame.rs).
    #[cfg(target_os = "android")]
    frame: Option<crate::android_frame::Frame>,
    /// Android: where the picture was last drawn in the window (x, y,
    /// width, height, window pixels).
    #[cfg(target_os = "android")]
    view: (usize, usize, usize, usize),
    /// Android: the fingers on the screen (see [`A5App::touch`]).
    #[cfg(target_os = "android")]
    touch: TouchA5,
}

impl A5App {
    /// The picture's place in the window: (x, y, width, height).
    #[cfg(not(target_os = "android"))]
    fn place(&self) -> (usize, usize, usize, usize) {
        let Some(window) = &self.window else {
            return (0, 0, VIEW_WIDTH, VIEW_HEIGHT);
        };
        let size = window.inner_size();
        let (ww, wh) = (size.width.max(1) as usize, size.height.max(1) as usize);
        let (gw, gh) = (VIEW_WIDTH, VIEW_HEIGHT);
        let (dw, dh) = if ww * gh > wh * gw {
            (wh * gw / gh, wh)
        } else {
            (ww, ww * gh / gw)
        };
        let (dw, dh) = (dw.max(1), dh.max(1));
        ((ww - dw) / 2, (wh - dh) / 2, dw, dh)
    }

    /// Android: the picture through [`crate::android_frame`] at its own
    /// size, scaled by the system's composer, with the bag and menu
    /// buttons in the black bars (not during a film).
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
        let shown = self
            .play
            .as_ref()
            .is_some_and(|p| !p.logos_playing() && !p.stage().film_playing());
        let picture = &self.picture;
        let drawn = crate::android::present_with_buttons(
            frame,
            (size.width as usize, size.height as usize),
            (VIEW_WIDTH, VIEW_HEIGHT),
            |y, out| {
                for (o, &p) in out.iter_mut().zip(&picture[y * VIEW_WIDTH..][..VIEW_WIDTH]) {
                    *o = crate::android_frame::from_xrgb(p);
                }
            },
            shown,
        );
        if let Some((view, buttons)) = drawn {
            self.view = view;
            self.touch.buttons = buttons;
        }
    }

    #[cfg(not(target_os = "android"))]
    fn present(&mut self) {
        let (ox, oy, dw, dh) = self.place();
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
        // The picture at its proportions, black bars around, nearest
        // pixel.
        let (gw, gh) = (VIEW_WIDTH, VIEW_HEIGHT);
        buf.fill(0);
        for y in 0..dh {
            let row = &self.picture[(y * gh / dh) * gw..][..gw];
            let out = &mut buf[(oy + y) * ww + ox..][..dw];
            for (x, o) in out.iter_mut().enumerate() {
                *o = row[x * gw / dw] & 0x00ff_ffff;
            }
        }
        let _ = buf.present();
    }

    /// The game, made once the start's reads end (black until then).
    fn ready(&mut self, el: &ActiveEventLoop) -> bool {
        if self.play.is_some() {
            return true;
        }
        if let Some(s) = self.start.as_ref().filter(|s| !s.warm.is_finished()) {
            let (at, to) = (
                s.progress.0.load(Ordering::Relaxed),
                s.progress.1.load(Ordering::Relaxed),
            );
            if to > 0 {
                crate::a5_bar::draw(&mut self.picture, VIEW_WIDTH, VIEW_HEIGHT, at, to);
            }
            return false;
        }
        let Some(start) = self.start.take() else {
            return false;
        };
        let _ = start.warm.join();
        match Play::new(start.src, &start.lang, Some(start.audio)) {
            Ok(mut play) => {
                // The saves (`saves/autoslot`, `slot_<n>`) in the player's
                // data folder; their dates in local time.
                play.set_save_dir(Some(saves_dir()));
                play.menus.utc_offset = crate::a2::utc_offset_now;
                play.boot();
                self.play = Some(play);
                self.last = Instant::now();
                true
            }
            Err(e) => {
                log::error!("{e}");
                self.error = Some(e);
                el.exit();
                false
            }
        }
    }

    fn frame(&mut self, el: &ActiveEventLoop) {
        if !self.ready(el) {
            self.present();
            return;
        }
        let Some(play) = self.play.as_mut() else {
            return;
        };
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64().min(0.25);
        self.last = now;
        // Android: the touch screen's next action, one a frame (a menu's
        // press after the cursor moved there).
        #[cfg(target_os = "android")]
        match self.touch.queue.pop_front().flatten() {
            Some(Press::Click) => play.click(),
            Some(Press::Right) => play.right_press(),
            Some(Press::Escape) => play.escape(),
            Some(Press::Space) => play.space(),
            None => {}
        }
        play.update(dt);
        if play.quit_requested() {
            el.exit();
            return;
        }
        // A menu frees the mouse (the game draws its cursor); play holds
        // it. The system cursor stays hidden (ShowCursor(0), @ 0x0043c200).
        let menu = play.in_menu();
        if menu != self.menu_cursor {
            self.menu_cursor = menu;
            if let Some(w) = &self.window {
                if menu {
                    let _ = w.set_cursor_grab(CursorGrabMode::None);
                } else if w.set_cursor_grab(CursorGrabMode::Locked).is_err() {
                    let _ = w.set_cursor_grab(CursorGrabMode::Confined);
                }
            }
        }
        play.render(&mut self.picture);
        self.present();
    }
}

impl ApplicationHandler for A5App {
    /// Android: the activity's window is gone (the app in the
    /// background): the game and its sound stop until `resumed` makes
    /// another.
    #[cfg(target_os = "android")]
    fn suspended(&mut self, _el: &ActiveEventLoop) {
        self.surface = None;
        self.frame = None;
        self.window = None;
        self.touch = TouchA5::default();
        crate::android::pause_audio(&self._audio, true);
    }

    fn resumed(&mut self, el: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        crate::android::pause_audio(&self._audio, false);
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(VIEW_WIDTH as u32, VIEW_HEIGHT as u32))
            .with_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)));
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
        // The original hides the system cursor (ShowCursor(0) at start,
        // @ 0x0043c200) and reads the mouse's relative moves (DirectInput).
        window.set_cursor_visible(false);
        if window.set_cursor_grab(CursorGrabMode::Locked).is_err() {
            let _ = window.set_cursor_grab(CursorGrabMode::Confined);
        }
        self.window = Some(window);
        self.focused = true;
        // (Android: a window made again, the app back from the
        // background: the game goes on where it was)
        self.last = Instant::now();
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.focused {
                #[allow(clippy::cast_possible_truncation)]
                if let Some(p) = self.play.as_mut() {
                    p.look(delta.0 as f32, delta.1 as f32);
                }
            }
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        // Android: the activity is being destroyed.
        #[cfg(target_os = "android")]
        if crate::android::destroyed() {
            _el.exit();
            return;
        }
        // Android: Back (Escape), which reaches the app outside winit.
        #[cfg(target_os = "android")]
        if let Some(id) = self.window.as_ref().map(|w| w.id()) {
            for e in crate::android::mode_typed() {
                self.window_event(_el, id, e);
            }
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Android: the touch screen, and Back as Escape.
        #[cfg(target_os = "android")]
        if let WindowEvent::Touch(t) = &event {
            self.touch(t);
            return;
        }
        #[cfg(target_os = "android")]
        if let WindowEvent::KeyboardInput { event: k, .. } = &event {
            if k.logical_key == Key::Named(NamedKey::BrowserBack) {
                if k.state == ElementState::Pressed {
                    self.touch.queue.push_back(Some(Press::Escape));
                }
                return;
            }
        }
        if self.play.is_none() {
            // Black while the start's reads go on: only closing and the
            // redraws that make the game once they end.
            match event {
                WindowEvent::CloseRequested => el.exit(),
                WindowEvent::RedrawRequested => self.frame(el),
                _ => {}
            }
            return;
        }
        let Some(play) = self.play.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    // Escape (DIK 1): a film ends, or the in-game menu.
                    Key::Named(NamedKey::Escape) => play.escape(),
                    // Space (0x39): a film or a dialog ends.
                    Key::Named(NamedKey::Space) => play.space(),
                    // The port's own, as in the other games: F11 switches
                    // between the window and the full screen.
                    Key::Named(NamedKey::F11) => {
                        self.fullscreen = !self.fullscreen;
                        if let Some(w) = &self.window {
                            w.set_fullscreen(
                                self.fullscreen.then_some(Fullscreen::Borderless(None)),
                            );
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::Focused(f) => self.focused = f,
            // A press of the left button: the original's mouse event of
            // kind 0 with the buttons 1 (@ 0x0043bbc0).
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => play.click(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => play.right_press(),
            // In a menu the mouse moves a cursor on the picture.
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(w) = &self.window {
                    let size = w.inner_size();
                    let (ww, wh) = (f64::from(size.width.max(1)), f64::from(size.height.max(1)));
                    #[allow(clippy::cast_precision_loss)]
                    let (gw, gh) = (VIEW_WIDTH as f64, VIEW_HEIGHT as f64);
                    let k = (ww / gw).min(wh / gh);
                    let (ox, oy) = ((ww - gw * k) / 2.0, (wh - gh * k) / 2.0);
                    #[allow(clippy::cast_possible_truncation)]
                    play.mouse_at(
                        ((position.x - ox) / k) as i32,
                        ((position.y - oy) / k) as i32,
                    );
                }
            }
            WindowEvent::RedrawRequested => self.frame(el),
            _ => {}
        }
    }
}

/// Opens a window and plays the game from the data at `path` (its main
/// menu first); returns when the window is closed.
pub fn run(path: &Path, fullscreen: bool) -> Result<(), String> {
    let mut el = EventLoop::new().map_err(|e| format!("No window can be opened: {e}"))?;
    run_window_on(&mut el, path, fullscreen).map(|_| ())
}

/// [`run`] on the program's event loop (the game chosen in the game
/// library, or a folder given to the program): the window opens, the game
/// plays until its `quit_game` or the window's closing.
pub fn run_window_on(
    el: &mut EventLoop<()>,
    path: &Path,
    fullscreen: bool,
) -> Result<crate::a2::Ended, String> {
    let (src, label) = open(path)?;
    run_source_on(el, src, &label, fullscreen)
}

/// [`run_window_on`] with the data opened (`label` says what they are):
/// on Android, a folder chosen in the picker, read through the Storage
/// Access Framework.
pub fn run_source_on(
    el: &mut EventLoop<()>,
    src: Atlantis5Source,
    label: &str,
    fullscreen: bool,
) -> Result<crate::a2::Ended, String> {
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    log::info!("{label}");
    let lang = language(&src);
    let audio = Audio::new();
    let out = {
        let a = audio.clone();
        crate::audio_out::start_with(move |buf, rate, channels| a.fill(buf, rate, channels))
    };
    let src = Arc::new(src);
    let mut app = A5App {
        play: None,
        start: Some(start(src, lang, audio)),
        error: None,
        picture: vec![0; VIEW_WIDTH * VIEW_HEIGHT],
        title: format!(
            "{} (2006) - Atlantis Native",
            crate::setup::GameKind::Atlantis5.name()
        ),
        fullscreen,
        window: None,
        surface: None,
        last: Instant::now(),
        focused: false,
        menu_cursor: false,
        _audio: out,
        #[cfg(target_os = "android")]
        frame: None,
        #[cfg(target_os = "android")]
        view: (0, 0, 1, 1),
        #[cfg(target_os = "android")]
        touch: TouchA5::default(),
    };
    el.run_app_on_demand(&mut app)
        .map_err(|e| format!("The window failed: {e}"))?;
    if let Some(e) = app.error {
        return Err(e);
    }
    Ok(crate::a2::Ended::Quit)
}

/// The cabinets' checkpoint indexes: `atlantis5/cabinets` in the per-user
/// data folder (about 26 MB for the French DVD once played through its
/// largest folders; deleting it only makes the next run slower).
pub fn cab_index_dir() -> std::path::PathBuf {
    crate::desktop::data_dir()
        .join("atlantis5")
        .join("cabinets")
}

/// The player's saves: `atlantis5/saves` in the per-user data folder.
pub fn saves_dir() -> std::path::PathBuf {
    crate::desktop::data_dir().join("atlantis5").join("saves")
}

/// Android: a finger moving less than this (window pixels) from where it
/// went down is still a tap.
#[cfg(target_os = "android")]
const TAP_SLOP: f64 = 24.0;

/// Android: an action for the game at a frame.
#[cfg(target_os = "android")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Press {
    /// The left button's press.
    Click,
    /// The right button's press: the inventory, or the hand's object put
    /// back.
    Right,
    /// Escape: a film ends, or the in-game menu.
    Escape,
    /// Space: a film or a dialog ends.
    Space,
}

/// Android: the fingers on the screen.
#[cfg(target_os = "android")]
#[derive(Default)]
struct TouchA5 {
    fingers: Vec<u64>,
    /// The first finger, where it went down and where it was last.
    primary: Option<u64>,
    from: (f64, f64),
    last: (f64, f64),
    /// It moved past [`TAP_SLOP`].
    moved: bool,
    /// A second finger came: a right press when they lift.
    two: bool,
    /// It went down in a menu (the game's cursor follows it).
    menu: bool,
    /// Where a tap presses the bag and the menu buttons (window pixels),
    /// as last drawn.
    buttons: [Option<crate::android::Rect>; 2],
    /// The first finger went down on the bag (0) or menu (1) button.
    on_button: Option<usize>,
    /// The game's actions, one a frame (`None`: a frame without one).
    queue: std::collections::VecDeque<Option<Press>>,
}

#[cfg(target_os = "android")]
impl A5App {
    /// Window point (x, y) as the picture's point.
    fn to_game(&self, (x, y): (f64, f64)) -> (i32, i32) {
        let (ox, oy, dw, dh) = self.view;
        let (gw, gh) = (VIEW_WIDTH as f64, VIEW_HEIGHT as f64);
        let gx = ((x - ox as f64) * gw / dw.max(1) as f64).floor();
        let gy = ((y - oy as f64) * gh / dh.max(1) as f64).floor();
        (
            (gx as i32).clamp(0, VIEW_WIDTH as i32 - 1),
            (gy as i32).clamp(0, VIEW_HEIGHT as i32 - 1),
        )
    }

    /// A finger on the screen. In a menu (the inventory and the subject
    /// menu too) the game's cursor goes where the finger is and its
    /// press is the left button's, a frame later. In play a drag turns
    /// the view, as the mouse's moves, and a tap is the left button's
    /// press on what is under the screen centre, as the click; during a
    /// film a tap is Space, which ends it. Two fingers are the right
    /// button. The bag button is the right button too, the menu button
    /// Escape.
    fn touch(&mut self, t: &winit::event::Touch) {
        use winit::event::TouchPhase;
        if self.play.is_none() {
            return;
        }
        let at = (t.location.x, t.location.y);
        let in_menu = self.play.as_ref().is_some_and(Play::in_menu);
        match t.phase {
            TouchPhase::Started => {
                self.touch.fingers.retain(|f| *f != t.id);
                self.touch.fingers.push(t.id);
                if self.touch.fingers.len() == 1 {
                    let st = &mut self.touch;
                    st.primary = Some(t.id);
                    st.from = at;
                    st.last = at;
                    st.moved = false;
                    st.two = false;
                    st.menu = false;
                    st.on_button = st.buttons.iter().position(|r| {
                        r.is_some_and(|(x, y, w, h)| {
                            at.0 >= x as f64
                                && at.1 >= y as f64
                                && at.0 < (x + w) as f64
                                && at.1 < (y + h) as f64
                        })
                    });
                    if st.on_button.is_none() && in_menu {
                        st.menu = true;
                        let (gx, gy) = self.to_game(at);
                        if let Some(p) = self.play.as_mut() {
                            p.mouse_at(gx, gy);
                        }
                        self.touch.queue.push_back(None);
                        self.touch.queue.push_back(Some(Press::Click));
                    }
                } else if !self.touch.two {
                    // A second finger: no left press, the right one when
                    // they lift.
                    self.touch.two = true;
                    if self.touch.menu {
                        self.touch.queue.retain(|p| *p != Some(Press::Click));
                    }
                }
            }
            TouchPhase::Moved => {
                let st = &mut self.touch;
                if st.primary != Some(t.id) || st.two || st.on_button.is_some() {
                    return;
                }
                let (lx, ly) = st.last;
                st.last = at;
                if (at.0 - st.from.0).hypot(at.1 - st.from.1) > TAP_SLOP {
                    st.moved = true;
                }
                if st.menu {
                    let (gx, gy) = self.to_game(at);
                    if let Some(p) = self.play.as_mut() {
                        p.mouse_at(gx, gy);
                    }
                } else if st.moved && !in_menu {
                    // The view turns with the finger, as with the mouse
                    // (the picture's pixels as the mouse's counts).
                    let k = VIEW_WIDTH as f64 / self.view.2.max(1) as f64;
                    if let Some(p) = self.play.as_mut() {
                        p.look(((at.0 - lx) * k) as f32, ((at.1 - ly) * k) as f32);
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.touch.fingers.retain(|f| *f != t.id);
                let ended = t.phase == TouchPhase::Ended;
                let st = &mut self.touch;
                let button = st.on_button.take();
                if st.primary == Some(t.id) && !st.two {
                    if let Some(b) = button {
                        if ended {
                            st.queue.push_back(Some(if b == 0 {
                                Press::Right
                            } else {
                                Press::Escape
                            }));
                        }
                    } else if ended && !st.menu && !st.moved {
                        let film = self
                            .play
                            .as_ref()
                            .is_some_and(|p| p.logos_playing() || p.stage().film_playing());
                        st.queue
                            .push_back(Some(if film { Press::Space } else { Press::Click }));
                    }
                }
                if st.fingers.is_empty() {
                    if st.two && ended {
                        st.queue.push_back(Some(Press::Right));
                    }
                    st.primary = None;
                    st.two = false;
                }
            }
        }
    }
}

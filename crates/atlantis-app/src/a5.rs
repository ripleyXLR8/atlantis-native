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
use winit::event::StartCause;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
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
    use atlantis3_data::atlantis5::Form;
    let src = Atlantis5Source::open(path)?;
    let copy = crate::a5_copy::is_copy(path);
    let medium = match src.form() {
        Form::DiscFolder if copy => "DVD, copied to this computer",
        Form::DiscFolder => "DVD",
        Form::DiscImage => "DVD image",
        Form::Tree => "DVD, installed",
    };
    let lang = language(&src);
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
    Ok((label, src.form() != Form::Tree && !copy))
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
    /// The mixer, none when no sound output could be opened.
    audio: Option<Audio>,
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
fn start(src: Arc<Atlantis5Source>, lang: String, audio: Option<Audio>) -> Start {
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

/// A frame's time: one screen refresh, as the original's `Flip(DDFLIP_WAIT)`
/// (play.md § 6), at 60 Hz, the common refresh [INFERRED there].
const FRAME: std::time::Duration = std::time::Duration::from_micros(16_667);

/// Until when the event loop sleeps: the next frame's time, or, while a
/// redraw asked for has not come (a hidden, minimised or covered window,
/// whose redraws the system holds back), a frame from now: a wake per
/// frame instead of a loop that never sleeps.
fn wait_until(next_frame: Instant, redraw_pending: bool, now: Instant) -> Instant {
    if redraw_pending && next_frame <= now {
        now + FRAME
    } else {
        next_frame
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
    /// The picture's column of each window column ([`Self::present`]).
    columns: Vec<usize>,
    last: Instant,
    /// When the next frame is due ([`FRAME`] after the last one's).
    next_frame: Instant,
    /// A redraw was asked for and has not come yet.
    redraw_pending: bool,
    /// Whether the window has the focus (the mouse's moves turn the view
    /// or move the menus' mouse only then; the pointer is held only then).
    focused: bool,
    /// The fractions of the mouse's relative moves not yet given to a
    /// menu (some systems report them below a pixel).
    menu_move: (f64, f64),
    _audio: Option<cpal::Stream>,
    /// Why the game could not be made, for the caller.
    error: Option<String>,
}

impl A5App {
    /// The picture's place in the window: (x, y, width, height).
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
        // pixel: each window column's picture column computed once per
        // size (a division a pixel is dear on a small ARM board).
        let (gw, gh) = (VIEW_WIDTH, VIEW_HEIGHT);
        if self.columns.len() != dw {
            self.columns = (0..dw).map(|x| x * gw / dw).collect();
        }
        buf[..oy * ww].fill(0);
        buf[(oy + dh) * ww..].fill(0);
        for y in 0..dh {
            let row = &self.picture[(y * gh / dh) * gw..][..gw];
            let line = &mut buf[(oy + y) * ww..][..ww];
            line[..ox].fill(0);
            line[ox + dw..].fill(0);
            for (o, &c) in line[ox..ox + dw].iter_mut().zip(&self.columns) {
                *o = row[c] & 0x00ff_ffff;
            }
        }
        let _ = buf.present();
    }

    /// The pointer held in the window while it has the focus, in play and
    /// in the menus alike (the original reads the mouse's relative moves
    /// through DirectInput throughout), and let go when another program
    /// takes the focus.
    fn grab(&self) {
        let Some(window) = &self.window else {
            return;
        };
        if self.focused {
            let _ = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
        }
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
        match Play::new(start.src, &start.lang, start.audio) {
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
        self.redraw_pending = false;
        self.next_frame = (self.next_frame + FRAME).max(Instant::now());
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
        play.update(dt);
        if play.quit_requested() {
            el.exit();
            return;
        }
        play.render(&mut self.picture);
        self.present();
    }
}

impl ApplicationHandler for A5App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
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
                self.error = Some(format!("The window could not be opened: {e}"));
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
                self.error = Some(format!("The window cannot show the game: {e}"));
                el.exit();
                return;
            }
        }
        // The original hides the system cursor (ShowCursor(0) at start,
        // @ 0x0043c200) and reads the mouse's relative moves (DirectInput).
        window.set_cursor_visible(false);
        self.window = Some(window);
        self.focused = true;
        self.grab();
        self.last = Instant::now();
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if !self.focused {
                return;
            }
            let Some(p) = self.play.as_mut() else {
                return;
            };
            if p.in_menu() {
                // The menus move their mouse by the relative moves
                // (menus.md § 2: @ 0x0040d920, kind 2, through
                // @ 0x0040d890), from the screen's centre where each mode
                // starts it.
                let (fx, fy) = (self.menu_move.0 + delta.0, self.menu_move.1 + delta.1);
                let (ix, iy) = (fx.trunc(), fy.trunc());
                self.menu_move = (fx - ix, fy - iy);
                #[allow(clippy::cast_possible_truncation)]
                p.mouse_by(ix as i32, iy as i32);
            } else {
                #[allow(clippy::cast_possible_truncation)]
                p.look(delta.0 as f32, delta.1 as f32);
            }
        }
    }

    /// A frame at each [`FRAME`] (the original's frame ends with
    /// `Flip(DDFLIP_WAIT)`: one per screen refresh); in between the
    /// program sleeps instead of drawing again at once.
    fn new_events(&mut self, _el: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            if let Some(w) = &self.window {
                w.request_redraw();
                self.redraw_pending = true;
            }
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let until = wait_until(self.next_frame, self.redraw_pending, Instant::now());
        el.set_control_flow(ControlFlow::WaitUntil(until));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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
            // A key's press, not the system's repeats of a held key: the
            // original reads the keyboard's buffered DirectInput data
            // (@ 0x0047c8a0, GetDeviceData), one event per transition.
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
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
            WindowEvent::Focused(f) => {
                self.focused = f;
                self.grab();
            }
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
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    let (src, label) = open(path)?;
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
        start: Some(start(src, lang, mixer_for_play(audio, out.is_some()))),
        error: None,
        picture: vec![0; VIEW_WIDTH * VIEW_HEIGHT],
        title: format!(
            "{} (2006) - Atlantis Native",
            crate::setup::GameKind::Atlantis5.name()
        ),
        fullscreen,
        window: None,
        surface: None,
        columns: Vec::new(),
        last: Instant::now(),
        next_frame: Instant::now(),
        redraw_pending: false,
        focused: false,
        menu_move: (0.0, 0.0),
        _audio: out,
    };
    el.run_app_on_demand(&mut app)
        .map_err(|e| format!("The window failed: {e}"))?;
    if let Some(e) = app.error {
        return Err(e);
    }
    Ok(crate::a2::Ended::Quit)
}

/// The mixer the game is given: none without a sound output, so that the
/// engine times a voice, a sound or a music track by its length (the
/// mixer's queues drain only in the output's callback: given the mixer,
/// a voiced tirade would never end by itself and no playlist would move
/// on).
fn mixer_for_play(audio: Audio, output: bool) -> Option<Audio> {
    output.then_some(audio)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_sound_output_the_game_times_its_sounds() {
        assert!(mixer_for_play(Audio::new(), false).is_none());
        assert!(mixer_for_play(Audio::new(), true).is_some());
    }

    #[test]
    fn a_redraw_that_does_not_come_does_not_spin_the_loop() {
        let now = Instant::now();
        let past = now - FRAME * 3;
        let soon = now + FRAME / 2;
        // A redraw asked for and not delivered: a frame from now.
        assert_eq!(wait_until(past, true, now), now + FRAME);
        // Otherwise the frame's own time.
        assert_eq!(wait_until(soon, true, now), soon);
        assert_eq!(wait_until(past, false, now), past);
        assert_eq!(wait_until(soon, false, now), soon);
    }
}

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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{View, FIRST_VIEW, VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_play::{PictureCache, Play};
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
        "{build}; {:?}; languages {}; playing {}",
        src.form(),
        src.languages().join(", "),
        language(&src)
    );
    Ok((src, label))
}

/// How the game library shows the data at `path`: the edition's title
/// ([`title`]), the year ([`year`]), the medium and the language played
/// ("The Secrets of Atlantis : L'Héritage sacré (2006), DVD, French",
/// "The Secrets of Atlantis: The Sacred Legacy (2006), DVD, German", "The
/// Secrets of Atlantis (2007), 3 CDs, English"); with the DVD's image,
/// "DVD image"; with an install or the extracted cabinets, "DVD,
/// installed"; with the copy of the disc's files made by
/// [`crate::a5_copy`], "DVD, copied to this computer" (the CDs: "CD
/// images", "CDs, installed", "CDs, copied to this computer"); the
/// Italian *Atlantis Collection*'s install, "The Secrets of Atlantis
/// (2014), Atlantis Collection, installed, Italian".
pub fn label(path: &Path) -> Result<String, atlantis3_data::atlantis5::OpenError> {
    edition(path).map(|e| e.0)
}

/// [`label`], and whether the data are read from the disc (a drive or a
/// copy of its files elsewhere than [`crate::a5_copy::copy_dir`], or its
/// image): what the game library offers to copy to the computer.
pub fn edition(path: &Path) -> Result<(String, bool), atlantis3_data::atlantis5::OpenError> {
    let src = Atlantis5Source::open(path)?;
    let copy = crate::a5_copy::is_copy(&atlantis3_data::atlantis5::place_root(path));
    Ok(edition_of(&src, copy))
}

/// [`edition`] of the data opened in `src` (`copy`: they are the copy
/// [`crate::a5_copy`] made): the label, and whether they are read from
/// the disc.
pub fn edition_of(src: &Atlantis5Source, copy: bool) -> (String, bool) {
    use atlantis3_data::atlantis5::{Edition, Form};
    let cds = src.edition() == Edition::NorthAmericanCds;
    let medium = match (src.form(), cds) {
        _ if src.edition() == Edition::ItalianCollection => "Atlantis Collection, installed",
        (Form::DiscFolder, false) if copy => "DVD, copied to this computer",
        (Form::DiscFolder, false) => "DVD",
        (Form::DiscImage, false) => "DVD image",
        (Form::Tree, false) => "DVD, installed",
        (Form::DiscFolder, true) if copy => "CDs, copied to this computer",
        (Form::DiscFolder, true) => "3 CDs",
        (Form::DiscImage, true) => "CD images",
        (Form::Tree, true) => "CDs, installed",
    };
    let lang_name = atlantis3_data::atlantis5::language_name(&language(src));
    let label = format!("{} ({}), {medium}, {lang_name}", title(src), year(src));
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

/// The language played: the one of the place opened (a language's place
/// of the game library, [`atlantis3_data::atlantis5::language_place`]),
/// else the only one, else the system's when the disc has it, else
/// English ([`atlantis3_data::atlantis5::default_language`]).
pub fn language(src: &Atlantis5Source) -> String {
    src.language(crate::locale::system_language().as_deref())
}

/// The edition's title: the French DVD's (French only) is *L'Héritage
/// sacré*; the international DVD's (English, German, Italian, Spanish) is
/// sold as *The Sacred Legacy*; the North-American CDs' and the Italian
/// *Atlantis Collection*'s (its installer's title) as *The Secrets of
/// Atlantis*, with no subtitle.
pub fn title(src: &Atlantis5Source) -> &'static str {
    use atlantis3_data::atlantis5::Edition;
    if matches!(
        src.edition(),
        Edition::NorthAmericanCds | Edition::ItalianCollection
    ) {
        "The Secrets of Atlantis"
    } else if src.languages().iter().all(|l| l == "french") {
        crate::setup::GameKind::Atlantis5.name()
    } else {
        "The Secrets of Atlantis: The Sacred Legacy"
    }
}

/// The edition's year: 2006 for the DVDs, 2007 for the North-American
/// CDs, 2014 for the Italian *Atlantis Collection*.
pub fn year(src: &Atlantis5Source) -> u16 {
    match src.edition() {
        atlantis3_data::atlantis5::Edition::NorthAmericanCds => 2007,
        atlantis3_data::atlantis5::Edition::Dvd => 2006,
        atlantis3_data::atlantis5::Edition::ItalianCollection => 2014,
    }
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

/// The next frame's time after a frame run at `now`: [`FRAME`] after the
/// last one's, not before `now` (fell behind: no catching up) and never
/// past `now` + [`FRAME`]. Every redraw runs a frame, the system's own too
/// (a live resize, an expose); without the bound, each one that comes
/// before its time moved the schedule a further frame ahead, and after a
/// burst of them the game stood still until that time came.
fn next_frame_after(next_frame: Instant, now: Instant) -> Instant {
    (next_frame + FRAME).clamp(now, now + FRAME)
}

/// The longest a picture goes without a present while it does not
/// change ([`must_present`]).
const KEEP_SHOWN: std::time::Duration = std::time::Duration::from_secs(1);

/// Whether a frame presents: when its picture was composed, when the
/// system asked for the window's content (a resize, an expose, a redraw
/// the port did not ask: on Wayland a skipped present then leaves
/// garbage), and at least every [`KEEP_SHOWN`] (a system redraw that
/// came with the port's own cannot be told from it). A picture that did
/// not change is otherwise not presented again (the menus: the port's
/// own saving, the original presents every frame the same picture).
fn must_present(composed: bool, asked: bool, since: std::time::Duration) -> bool {
    composed || asked || since >= KEEP_SHOWN
}

/// The present's state after one (`asked`, when the last was): one that
/// reached the window clears the ask and is the last; a failed one (the
/// surface's resize, its buffer or its present refused) leaves the ask
/// set, so the next frame presents the picture again even when it was
/// not composed anew.
fn after_present(ok: bool, presented: Instant, now: Instant) -> (bool, Instant) {
    if ok {
        (false, now)
    } else {
        (true, presented)
    }
}

/// The decoding's step before the game starts (uncompressed bytes).
const WARM_STEP: u64 = 4 << 20;

/// When the pointer is held in the window. Held only while the window has
/// the focus and the system pointer is known to be in it: winit's
/// `Locked` holds the pointer where it is (Windows clips it to a 1 × 1
/// rectangle at its place, macOS freezes it), so a lock taken with the
/// pointer over another window or on another screen would send the
/// next press there. The pointer is seen in the window by `CursorEntered`,
/// `CursorMoved` or a press (which only comes to the window under it);
/// it is known gone by `CursorLeft` while it is not held (a held pointer
/// cannot leave, and a crossing that a grab itself makes is not a leave).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PointerHold {
    inside: bool,
    held: bool,
}

impl PointerHold {
    /// The pointer seen in the window.
    fn seen_inside(&mut self) {
        self.inside = true;
    }

    /// The pointer left the window.
    fn left(&mut self) {
        if !self.held {
            self.inside = false;
        }
    }

    /// The window changed under the pointer (the full screen switched):
    /// where the pointer is now is not known until it is seen again.
    fn forget(&mut self) {
        self.inside = false;
    }

    /// Whether the pointer is to be held with the window's `focused`:
    /// the new state when it changes.
    fn update(&mut self, focused: bool) -> Option<bool> {
        let want = focused && self.inside;
        (want != self.held).then(|| {
            self.held = want;
            want
        })
    }
}

/// Whether a wheel event turned the wheel (DirectInput's lZ not 0).
fn wheel_turned(delta: winit::event::MouseScrollDelta) -> bool {
    match delta {
        winit::event::MouseScrollDelta::LineDelta(_, y) => y != 0.0,
        winit::event::MouseScrollDelta::PixelDelta(p) => p.y != 0.0,
    }
}

struct A5App {
    play: Option<Play>,
    start: Option<Start>,
    picture: Vec<u32>,
    title: String,
    fullscreen: bool,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    /// The picture's column of each window column ([`Self::present`]).
    #[cfg(not(target_os = "android"))]
    columns: Vec<usize>,
    last: Instant,
    /// When the next frame is due ([`FRAME`] after the last one's).
    next_frame: Instant,
    /// A redraw was asked for and has not come yet.
    redraw_pending: bool,
    /// Whether the window has the focus (the mouse's moves turn the view
    /// or move the menus' mouse only then; the pointer is held only then).
    focused: bool,
    /// Whether the pointer is in the window and held ([`PointerHold`]).
    pointer: PointerHold,
    /// The fractions of the mouse's relative moves not yet given to a
    /// menu (some systems report them below a pixel).
    menu_move: (f64, f64),
    /// The left and right buttons held (the original's press events carry
    /// both, [`Play::set_buttons`]).
    buttons: (bool, bool),
    /// The picture last composed ([`Play::render_cached`]).
    cache: PictureCache,
    /// The system asked for the window's content since the last present.
    present_asked: bool,
    /// When the last present was.
    presented: Instant,
    /// The sound output (kept alive while it plays), the mixer it pulls
    /// from, and the flag its stream sets when the device goes away.
    audio_out: Option<cpal::Stream>,
    mixer: Audio,
    audio_lost: Arc<AtomicBool>,
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
    /// Started as the library's one game without its screen: until then
    /// Shift goes back to the library (as in the other games' modes).
    shift_until: Option<Instant>,
    /// Shift was pressed in time: the library's screen next.
    to_library: bool,
}

/// How long after an automatic start Shift goes back to the game library
/// (the other games' modes' grace).
const SHIFT_GRACE: std::time::Duration = std::time::Duration::from_millis(2000);

/// Whether Shift (`shift`, the modifiers now) at `now` goes back to the
/// game library: only within the grace `until` of an automatic start.
fn shift_to_library(until: Option<Instant>, now: Instant, shift: bool) -> bool {
    shift && until.is_some_and(|t| now < t)
}

impl A5App {
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
        let ok = drawn.is_some();
        if let Some((view, buttons)) = drawn {
            self.view = view;
            self.touch.buttons = buttons;
        }
        (self.present_asked, self.presented) = after_present(ok, self.presented, Instant::now());
    }

    #[cfg(not(target_os = "android"))]
    fn present(&mut self) {
        let ok = self.try_present();
        (self.present_asked, self.presented) = after_present(ok, self.presented, Instant::now());
    }

    /// The picture into the window; false when it did not get there.
    #[cfg(not(target_os = "android"))]
    fn try_present(&mut self) -> bool {
        let (Some(window), Some(surface)) = (&self.window, self.surface.as_mut()) else {
            return false;
        };
        // The window's size read once: the placement and the buffer are
        // both made from it (a second read may differ during a resize).
        let size = window.inner_size();
        let (ww, wh) = (size.width.max(1) as usize, size.height.max(1) as usize);
        let (Some(w), Some(h)) = (NonZeroU32::new(ww as u32), NonZeroU32::new(wh as u32)) else {
            return false;
        };
        if surface.resize(w, h).is_err() {
            return false;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return false;
        };
        scale_into(&mut buf, (ww, wh), &self.picture, &mut self.columns);
        buf.present().is_ok()
    }

    /// The pointer held in the window while it has the focus, in play and
    /// in the menus alike (the original reads the mouse's relative moves
    /// through DirectInput throughout), and let go when another program
    /// takes the focus. Held only once the pointer is in the window
    /// ([`PointerHold`]).
    fn grab(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        match self.pointer.update(self.focused) {
            Some(true) => {
                let _ = window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            }
            Some(false) => {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
            }
            None => {}
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
            self.cache.invalidate();
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

    /// The output's device gone while the game plays (a headset unplugged):
    /// the default device opened again on the same mixer; when there is
    /// none, the game goes on as without an output from the start
    /// ([`mixer_for_play`], [`Play::drop_audio`]).
    fn output_lost(&mut self) {
        if !self.audio_lost.swap(false, Ordering::Relaxed) {
            return;
        }
        log::warn!("audio output lost");
        self.audio_out = None;
        self.audio_out = open_output(&self.mixer, &self.audio_lost);
        if self.audio_out.is_none() {
            if let Some(play) = self.play.as_mut() {
                play.drop_audio();
            }
            if let Some(s) = self.start.as_mut() {
                s.audio = None;
            }
        }
    }

    fn frame(&mut self, el: &ActiveEventLoop) {
        self.redraw_pending = false;
        self.next_frame = next_frame_after(self.next_frame, Instant::now());
        if !self.ready(el) {
            self.present();
            return;
        }
        self.output_lost();
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
        let composed = play.render_cached(&mut self.picture, &mut self.cache);
        if must_present(composed, self.present_asked, self.presented.elapsed()) {
            self.present();
        }
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
        crate::android::pause_audio(&self.audio_out, true);
    }

    fn resumed(&mut self, el: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        crate::android::pause_audio(&self.audio_out, false);
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
        // (Android: a window made again, the app back from the
        // background: the game goes on where it was)
        self.grab();
        self.last = Instant::now();
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            // Not while the pointer is free (outside the window): those
            // moves are the player's on the other programs.
            if !self.focused || !self.pointer.held {
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
        // Android: the activity is being destroyed.
        #[cfg(target_os = "android")]
        if crate::android::destroyed() {
            el.exit();
            return;
        }
        // Android: Back (Escape), which reaches the app outside winit.
        #[cfg(target_os = "android")]
        if let Some(id) = self.window.as_ref().map(|w| w.id()) {
            for e in crate::android::mode_typed() {
                self.window_event(el, id, e);
            }
        }
        // Android in the background (no window): the game waits.
        #[cfg(target_os = "android")]
        if self.window.is_none() {
            el.set_control_flow(ControlFlow::Wait);
            return;
        }
        let until = wait_until(self.next_frame, self.redraw_pending, Instant::now());
        el.set_control_flow(ControlFlow::WaitUntil(until));
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
        // The focus in every phase, the start's bar included: the pointer
        // let go when another program takes it, held again on its return.
        // Where the system pointer is: held once it is in the window.
        match &event {
            WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => {
                self.pointer.seen_inside();
                self.grab();
            }
            WindowEvent::CursorLeft { .. } => self.pointer.left(),
            _ => {}
        }
        if let WindowEvent::Focused(f) = event {
            self.focused = f;
            self.grab();
            // Away from the window no release comes: both buttons up.
            if !f {
                self.buttons = (false, false);
                if let Some(play) = self.play.as_mut() {
                    play.set_buttons(false, false);
                }
            }
            return;
        }
        // Shift in the first moments of an automatic start (the start's
        // reads included): the game library.
        if let WindowEvent::ModifiersChanged(m) = &event {
            if shift_to_library(self.shift_until, Instant::now(), m.state().shift_key()) {
                log::info!("The Secrets of Atlantis stopped: the game library");
                self.to_library = true;
                el.exit();
                return;
            }
        }
        // The window's content asked by the system: presented at the next
        // frame even when the picture did not change (`must_present`).
        match event {
            WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::Occluded(false) => self.present_asked = true,
            WindowEvent::RedrawRequested if !self.redraw_pending => self.present_asked = true,
            _ => {}
        }
        if self.play.is_none() {
            // Black while the start's reads go on: only closing, the focus
            // (above) and the redraws that make the game once they end.
            match event {
                WindowEvent::CloseRequested => el.exit(),
                WindowEvent::RedrawRequested => self.frame(el),
                _ => {}
            }
            return;
        }
        // Both buttons' state, before the press is handled (@ 0x0047c8a0
        // sets 0x006b0b04 before calling the press handlers).
        if let WindowEvent::MouseInput { state, button, .. } = event {
            let down = state == ElementState::Pressed;
            match button {
                MouseButton::Left => self.buttons.0 = down,
                MouseButton::Right => self.buttons.1 = down,
                _ => {}
            }
        }
        let buttons = self.buttons;
        let Some(play) = self.play.as_mut() else {
            return;
        };
        if matches!(event, WindowEvent::MouseInput { .. }) {
            play.set_buttons(buttons.0, buttons.1);
        }
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
                        // The held place may be outside the new window:
                        // let go until the pointer is seen in it again.
                        self.pointer.forget();
                        self.grab();
                    }
                    _ => {}
                }
            }
            // A press of the left button: the original's mouse event of
            // kind 0 with the buttons 1, 5 with the right one held
            // (@ 0x0043bbc0).
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
            // The third and fourth buttons (DirectInput's `DIMOUSESTATE`
            // has four, @ 0x0047c8a0 walks them; X1 is the fourth, X2 is
            // not read): a press with the left and right buttons' state.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Middle | MouseButton::Back,
                ..
            } => play.other_press(),
            // A wheel turn (lZ): a mouse move event with no move.
            WindowEvent::MouseWheel { delta, .. } if self.focused && wheel_turned(delta) => {
                play.wheel();
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
    run_window_on(&mut el, path, fullscreen, false).map(|_| ())
}

/// [`run`] on the program's event loop (the game chosen in the game
/// library, or a folder given to the program): the window opens, the game
/// plays until its `quit_game` or the window's closing. With `shift_grace`
/// (the library's one game, started without its screen) Shift pressed in
/// the first moments goes back to the library ([`crate::a2::Ended::Library`]).
pub fn run_window_on(
    el: &mut EventLoop<()>,
    path: &Path,
    fullscreen: bool,
    shift_grace: bool,
) -> Result<crate::a2::Ended, String> {
    let (src, label) = open(path)?;
    run_source_on(el, src, &label, fullscreen, shift_grace)
}

/// [`run_window_on`] with the data opened (`label` says what they are):
/// on Android, a folder chosen in the picker, read through the Storage
/// Access Framework.
pub fn run_source_on(
    el: &mut EventLoop<()>,
    src: Atlantis5Source,
    label: &str,
    fullscreen: bool,
    shift_grace: bool,
) -> Result<crate::a2::Ended, String> {
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    log::info!("{label}");
    let lang = language(&src);
    let game_title = format!("{} ({})", title(&src), year(&src));
    let audio = Audio::new();
    let audio_lost = Arc::new(AtomicBool::new(false));
    let out = open_output(&audio, &audio_lost);
    let src = Arc::new(src);
    let mut app = A5App {
        play: None,
        start: Some(start(
            src,
            lang,
            mixer_for_play(audio.clone(), out.is_some()),
        )),
        error: None,
        picture: vec![0; VIEW_WIDTH * VIEW_HEIGHT],
        title: format!("{game_title} - Atlantis Native"),
        fullscreen,
        window: None,
        surface: None,
        #[cfg(not(target_os = "android"))]
        columns: Vec::new(),
        last: Instant::now(),
        next_frame: Instant::now(),
        redraw_pending: false,
        focused: false,
        pointer: PointerHold::default(),
        buttons: (false, false),
        menu_move: (0.0, 0.0),
        cache: PictureCache::default(),
        present_asked: true,
        presented: Instant::now(),
        audio_out: out,
        mixer: audio,
        audio_lost,
        shift_until: shift_grace.then(|| Instant::now() + SHIFT_GRACE),
        to_library: false,
        #[cfg(target_os = "android")]
        frame: None,
        #[cfg(target_os = "android")]
        view: (0, 0, 1, 1),
        #[cfg(target_os = "android")]
        touch: TouchA5::default(),
    };
    el.run_app_on_demand(&mut app)
        .map_err(|e| format!("The window failed: {e}"))?;
    if app.to_library {
        return Ok(crate::a2::Ended::Library);
    }
    if let Some(e) = app.error {
        return Err(e);
    }
    Ok(crate::a2::Ended::Quit)
}

/// The default output device's stream pulling from `audio`; `lost` set
/// when the device goes away.
fn open_output(audio: &Audio, lost: &Arc<AtomicBool>) -> Option<cpal::Stream> {
    let a = audio.clone();
    crate::audio_out::start_watched(
        move |buf, rate, channels| a.fill(buf, rate, channels),
        lost.clone(),
    )
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

/// The picture's place in a window of `ww` × `wh`: (x, y, width,
/// height), at its proportions, centred.
#[cfg(not(target_os = "android"))]
fn place(ww: usize, wh: usize) -> (usize, usize, usize, usize) {
    let (ww, wh) = (ww.max(1), wh.max(1));
    let (gw, gh) = (VIEW_WIDTH, VIEW_HEIGHT);
    let (dw, dh) = if ww * gh > wh * gw {
        (wh * gw / gh, wh)
    } else {
        (ww, ww * gh / gw)
    };
    let (dw, dh) = (dw.max(1), dh.max(1));
    ((ww - dw) / 2, (wh - dh) / 2, dw, dh)
}

/// The game's picture into a window buffer of `ww` × `wh` (row by row):
/// at its proportions, black bars around, nearest pixel. Each window
/// column's picture column is computed once per size in `columns` (a
/// division a pixel is dear on a small ARM board). A buffer smaller than
/// the size given is left alone rather than overrun.
#[cfg(not(target_os = "android"))]
fn scale_into(
    buf: &mut [u32],
    (ww, wh): (usize, usize),
    picture: &[u32],
    columns: &mut Vec<usize>,
) {
    let (gw, gh) = (VIEW_WIDTH, VIEW_HEIGHT);
    if ww == 0 || wh == 0 || buf.len() < ww * wh || picture.len() < gw * gh {
        return;
    }
    let (ox, oy, dw, dh) = place(ww, wh);
    if columns.len() != dw {
        *columns = (0..dw).map(|x| x * gw / dw).collect();
    }
    let buf = &mut buf[..ww * wh];
    buf[..oy * ww].fill(0);
    buf[(oy + dh) * ww..].fill(0);
    for y in 0..dh {
        let row = &picture[(y * gh / dh) * gw..][..gw];
        let line = &mut buf[(oy + y) * ww..][..ww];
        line[..ox].fill(0);
        line[ox + dw..].fill(0);
        for (o, &c) in line[ox..ox + dw].iter_mut().zip(columns.iter()) {
            *o = row[c] & 0x00ff_ffff;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The pointer is held only once it is seen in the window: the focus
    /// coming back with the pointer over another window or screen does
    /// not lock it there (Windows' `Locked` clips it where it is, macOS
    /// freezes it), and the full screen's switch lets it go until it is
    /// seen in the new window.
    #[test]
    fn the_pointer_is_held_only_inside_the_window() {
        let mut p = PointerHold::default();
        // The window opens with the focus, the pointer not seen yet.
        assert_eq!(p.update(true), None);
        assert!(!p.held);
        p.seen_inside();
        assert_eq!(p.update(true), Some(true));
        // A crossing while held is not a leave.
        p.left();
        assert_eq!(p.update(true), None);
        assert!(p.held);
        // Another program takes the focus; the pointer goes over it.
        assert_eq!(p.update(false), Some(false));
        p.left();
        // The focus back with the pointer still outside: not held.
        assert_eq!(p.update(true), None);
        assert!(!p.held);
        // The pointer comes into the window: held.
        p.seen_inside();
        assert_eq!(p.update(true), Some(true));
        // F11: let go until seen again.
        p.forget();
        assert_eq!(p.update(true), Some(false));
        p.seen_inside();
        assert_eq!(p.update(true), Some(true));
    }

    /// The library's one game started without its screen: Shift within
    /// the grace goes back to the library, as in the other games' modes;
    /// not after it, nor without the grace (a folder given to the program).
    #[test]
    fn shift_in_the_first_moments_goes_back_to_the_library() {
        let t0 = Instant::now();
        let until = Some(t0 + SHIFT_GRACE);
        assert!(shift_to_library(until, t0, true));
        assert!(!shift_to_library(until, t0, false));
        assert!(!shift_to_library(until, t0 + SHIFT_GRACE, true));
        assert!(!shift_to_library(None, t0, true));
    }

    #[test]
    fn without_a_sound_output_the_game_times_its_sounds() {
        assert!(mixer_for_play(Audio::new(), false).is_none());
        assert!(mixer_for_play(Audio::new(), true).is_some());
    }

    #[test]
    fn the_picture_fits_any_window_and_never_overruns_its_buffer() {
        let picture: Vec<u32> = (0..VIEW_WIDTH * VIEW_HEIGHT)
            .map(|i| 0xff00_0000 | i as u32)
            .collect();
        let mut columns = Vec::new();
        // A window wider than 16:9: bars left and right.
        let (ww, wh) = (1200, 576);
        let mut buf = vec![0xdead_beef; ww * wh];
        scale_into(&mut buf, (ww, wh), &picture, &mut columns);
        assert_eq!(place(ww, wh), (88, 0, 1024, 576));
        assert_eq!(buf[0], 0);
        assert_eq!(buf[88], picture[0] & 0x00ff_ffff);
        assert_eq!(buf[88 + 1024], 0);
        // The size changed between the reads (a buffer of the new, smaller
        // size given with the old size, and the other way): no panic, the
        // columns follow the size actually drawn.
        let mut small = vec![0xdead_beef; 900 * 506];
        scale_into(&mut small, (1024, 576), &picture, &mut columns);
        assert!(small.iter().all(|&p| p == 0xdead_beef));
        let mut big = vec![0xdead_beef; 1024 * 576];
        scale_into(&mut big, (900, 506), &picture, &mut columns);
        assert_eq!(columns.len(), place(900, 506).2);
        // Every size from 1 × 1 up, odd ones included.
        for (ww, wh) in [(1, 1), (3, 1000), (1000, 3), (1023, 577), (2560, 1440)] {
            let mut buf = vec![0xdead_beef; ww * wh];
            scale_into(&mut buf, (ww, wh), &picture, &mut columns);
            let (ox, oy, dw, dh) = place(ww, wh);
            assert!(ox + dw <= ww && oy + dh <= wh, "{ww}x{wh}");
            assert!(buf.iter().all(|&p| p != 0xdead_beef), "{ww}x{wh}");
        }
    }

    #[test]
    fn an_unchanged_picture_is_presented_when_the_system_asks() {
        let short = FRAME;
        // Composed: presented.
        assert!(must_present(true, false, short));
        // The same picture: not presented again...
        assert!(!must_present(false, false, short));
        // ...unless the system asked (a resize, an expose) or it was
        // long ago.
        assert!(must_present(false, true, short));
        assert!(must_present(false, false, KEEP_SHOWN));
    }

    #[test]
    fn a_failed_present_is_tried_again_at_the_next_frame() {
        let before = Instant::now();
        let now = before + FRAME;
        // Reached the window: the ask cleared, the next frame's unchanged
        // picture not presented again.
        let (asked, at) = after_present(true, before, now);
        assert_eq!((asked, at), (false, now));
        assert!(!must_present(false, asked, FRAME));
        // Refused: asked again at the next frame, the last present's time
        // kept.
        let (asked, at) = after_present(false, before, now);
        assert_eq!((asked, at), (true, before));
        assert!(must_present(false, asked, FRAME));
    }

    #[test]
    fn system_redraws_do_not_push_the_next_frame_away() {
        let now = Instant::now();
        // A live resize: 200 redraws in the same instant.
        let mut next = now;
        for _ in 0..200 {
            next = next_frame_after(next, now);
        }
        assert_eq!(next, now + FRAME);
        // On time: a frame after the last; late: from now.
        assert_eq!(next_frame_after(now, now + FRAME / 2), now + FRAME);
        let late = now + FRAME * 5;
        assert_eq!(next_frame_after(now, late), late);
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

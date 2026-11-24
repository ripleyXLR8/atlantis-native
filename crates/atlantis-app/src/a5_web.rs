//! *The Secrets of Atlantis* (*Atlantis V*) in the browser: the desktop's
//! window (`crate::a5`) in a page, the frame presented through WebGL 2.
//!
//! The data are the player's DVD as they picked it ([`crate::web`]): the
//! DVD's image (read a part at a time, never whole), a copy of the disc,
//! or an install. From the disc the game's files are in Microsoft
//! cabinets whose folders are each one compressed stream: a file deep in
//! one is decoded from the folder's start the first time
//! ([`atlantis_data::cab`]). The desktop keeps the cabinets' checkpoints
//! between runs; the page cannot, so before the game starts it decodes
//! the films folder up to the logos a step per frame (about 450 MB with
//! the French DVD, a bar on the screen), and while the game runs it
//! decodes the other folders a little every frame
//! ([`Atlantis5Source::warm_next`]): a later read anywhere then starts
//! from a checkpoint near it.
//!
//! The saves (`autoslot`, `slot_<n>`) are kept in the browser's
//! `localStorage` under `atlantis-native/atlantis5/`, one key per file,
//! with their dates (the load page shows them).
//!
//! The mouse, as on the desktop: in play a click takes the pointer lock
//! (the browser grants it on a click only) and the mouse's motion turns
//! the view; without the lock the page's pointer's motion does. A menu
//! lets the lock go: the page's pointer is the game's.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_play::{Play, LOGOS};
use atlantis3_engine::atlantis5_saves::SaveFiles;
use atlantis3_engine::audio::Audio;
use atlantis_data::FileSource;
use atlantis_engine::SaveStore;
use wasm_bindgen::prelude::*;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::platform::web::EventLoopExtWebSys;
use winit::window::{Window, WindowId};

use crate::a2_web;
use crate::gl_present::GlPresenter;

/// The game's name in the page.
const NAME: &str = "The Secrets of Atlantis";

/// A game step's time: the desktop's (`a5.rs` FRAME), one frame at 60 Hz.
const FRAME: std::time::Duration = std::time::Duration::from_micros(16_667);

/// A frame's time for decoding the logos' folder before the game starts
/// (the page answers between two).
const WARM_START_MS: f64 = 30.0;

/// A frame's time for decoding the other folders while the game runs.
const WARM_PLAY_MS: f64 = 3.0;

/// The sound output's rate.
const AUDIO_RATE: u32 = 44_100;

/// The decoding's step (uncompressed bytes).
const WARM_STEP: u64 = 256 << 10;

/// The saves in the browser's storage: each file under its name, its
/// last write (seconds since 1970) under `<name>.time`.
struct WebSaves(Rc<dyn SaveStore>);

impl WebSaves {
    fn time_key(name: &str) -> String {
        format!("{name}.time")
    }
}

impl SaveFiles for WebSaves {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.0.read(name)
    }

    fn write(&self, name: &str, data: &[u8]) -> bool {
        if !self.0.write(name, data) {
            return false;
        }
        let now = (js_sys::Date::now() / 1000.0) as i64;
        self.0
            .write(&Self::time_key(name), now.to_string().as_bytes());
        true
    }

    fn remove(&self, name: &str) {
        self.0.remove(name);
        self.0.remove(&Self::time_key(name));
    }

    fn exists(&self, name: &str) -> bool {
        self.0.read(name).is_some()
    }

    fn modified(&self, name: &str) -> Option<i64> {
        self.0.read(name)?;
        Some(
            self.0
                .read(&Self::time_key(name))
                .and_then(|t| String::from_utf8_lossy(&t).trim().parse().ok())
                .unwrap_or(0),
        )
    }
}

/// The language of the films' voices: French when the data have it, else
/// the first (as the desktop's).
fn language(src: &Atlantis5Source) -> String {
    let langs = src.languages();
    langs
        .iter()
        .find(|l| l.eq_ignore_ascii_case("french"))
        .or_else(|| langs.first())
        .cloned()
        .unwrap_or_else(|| "french".to_string())
}

/// Starts the game from `src`, the saves in `store`: the logos' folder
/// decoded, then the game's logos and its main menu.
pub fn start(src: Atlantis5Source, store: Rc<dyn SaveStore>) -> Result<(), JsValue> {
    let lang = language(&src);
    log::info!(
        "{NAME}: {:?}, languages {}, {}",
        src.form(),
        src.languages().join(", "),
        crate::version_string()
    );
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.spawn_app(A5App {
        src: Arc::new(src),
        lang,
        store,
        play: None,
        warm: (0, 0),
        warm_started: None,
        warm_done: false,
        audio: Audio::new(),
        out: None,
        audio_pending: true,
        silent_frames: 0.0,
        window: None,
        gl: None,
        picture: vec![0; VIEW_WIDTH * VIEW_HEIGHT],
        last: None,
        next_frame: Instant::now(),
        shown_size: None,
        clock: 0.0,
        menu: false,
        lock_asked: None,
        lock_refused: None,
        motion_acc: (0.0, 0.0),
        cursor: None,
        state_in: 0,
        done: false,
    });
    Ok(())
}

struct A5App {
    src: Arc<Atlantis5Source>,
    lang: String,
    store: Rc<dyn SaveStore>,
    play: Option<Play>,
    /// The logos' folder: decoded up to, the logo's offset in it.
    warm: (u64, u64),
    warm_started: Option<Instant>,
    /// Every folder decoded to its end.
    warm_done: bool,
    audio: Audio,
    /// The sound, started on the first click or key (a browser allows it
    /// only then).
    out: Option<crate::audio_out::web_out::WebOutput>,
    audio_pending: bool,
    /// The part of a frame not yet played out silently while there is no
    /// output ([`A5App::play_silently`]).
    silent_frames: f64,
    window: Option<Rc<Window>>,
    gl: Option<GlPresenter>,
    picture: Vec<u32>,
    /// The last step's time.
    last: Option<Instant>,
    /// When the next step is due ([`FRAME`] after the last one's).
    next_frame: Instant,
    /// The canvas's size at the last picture drawn.
    shown_size: Option<winit::dpi::PhysicalSize<u32>>,
    /// The game's time (seconds).
    clock: f64,
    /// Whether a menu is up (the pointer is free).
    menu: bool,
    lock_asked: Option<Instant>,
    /// When a lock request was last taken as refused: a click asks again
    /// once [`a2_web::LOCK_RETRY`] has passed (a browser refuses for a moment
    /// after the player left the lock with Escape).
    lock_refused: Option<Instant>,
    /// The fractions of the pointer-lock motion not yet sent.
    motion_acc: (f64, f64),
    /// The page's pointer at the last move (canvas pixels).
    cursor: Option<PhysicalPosition<f64>>,
    state_in: u32,
    done: bool,
}

impl A5App {
    /// The first click or key: the sound starts (silent while the page is
    /// hidden).
    fn gesture(&mut self) {
        if !std::mem::take(&mut self.audio_pending) {
            return;
        }
        let audio = self.audio.clone();
        // Mixed ahead on the page's thread (as Atlantis Evolution's):
        // cpal's single buffer ahead runs dry while a frame reads.
        self.out = crate::audio_out::web_out::start(
            move |out, rate, channels| {
                if a2_web::page_hidden() {
                    out.fill(0.0);
                } else {
                    audio.fill(out, rate, channels);
                }
            },
            AUDIO_RATE,
        );
    }

    /// Until the output exists (the first click or key; never on a page
    /// that cannot make one) the mix is played out silently on the game's
    /// clock, as Atlantis Evolution's page does (`Sounds::headless`): a
    /// film's sound does not pile up to play late at the first click, and
    /// a voice or a playlist still ends when its sound would.
    fn play_silently(&mut self, dt: f64) {
        if self.out.is_some() {
            self.silent_frames = 0.0;
            return;
        }
        self.silent_frames += dt * f64::from(AUDIO_RATE);
        let n = self.silent_frames.floor();
        self.silent_frames -= n;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        self.audio.play_silently(n as usize, AUDIO_RATE);
    }

    /// Whether the page holds the pointer lock; a request not granted in
    /// time is taken as refused.
    fn locked(&mut self) -> bool {
        if a2_web::pointer_locked() {
            self.lock_asked = None;
            self.lock_refused = None;
            return true;
        }
        if self
            .lock_asked
            .is_some_and(|t| t.elapsed() >= a2_web::LOCK_GRACE)
        {
            self.lock_asked = None;
            if self.lock_refused.is_none() {
                log::info!("pointer lock refused: the page's pointer turns the view");
            }
            self.lock_refused = Some(Instant::now());
        }
        false
    }

    /// Whether a click may ask for the pointer lock now.
    fn may_ask_lock(&mut self) -> bool {
        !self.menu
            && !self.locked()
            && self.lock_asked.is_none()
            && self
                .lock_refused
                .is_none_or(|t| t.elapsed() >= a2_web::LOCK_RETRY)
    }

    /// The picture's place in the canvas (x, y, width, height): 16:9,
    /// black bars around.
    fn place(&self) -> (usize, usize, usize, usize) {
        let Some(window) = &self.window else {
            return (0, 0, VIEW_WIDTH, VIEW_HEIGHT);
        };
        let size = window.inner_size();
        place_in(size.width as usize, size.height as usize)
    }
    /// A canvas position on the game's 1024 × 576 picture.
    fn to_picture(&self, p: PhysicalPosition<f64>) -> (i32, i32) {
        let (ox, oy, dw, dh) = self.place();
        let x = (p.x - ox as f64) * VIEW_WIDTH as f64 / dw as f64;
        let y = (p.y - oy as f64) * VIEW_HEIGHT as f64 / dh as f64;
        (x.floor() as i32, y.floor() as i32)
    }

    fn present(&mut self) {
        let (Some(window), Some(gl)) = (&self.window, self.gl.as_mut()) else {
            return;
        };
        // The canvas's size read once, for the placement and the draw.
        let size = window.inner_size();
        self.shown_size = Some(size);
        let (ww, wh) = (size.width.max(1), size.height.max(1));
        let (ox, oy, dw, dh) = place_in(ww as usize, wh as usize);
        for p in &mut self.picture {
            *p &= 0x00ff_ffff;
        }
        gl.present(
            (ww, wh),
            &self.picture,
            (VIEW_WIDTH as u32, VIEW_HEIGHT as u32),
            (ox as i32, oy as i32, dw as i32, dh as i32),
            false,
            None,
        );
    }

    /// Before the game: the logos' folder decoded a step at a time, a bar
    /// on a black screen. True once done.
    fn warm_start(&mut self) -> bool {
        let t = Instant::now();
        let started = *self.warm_started.get_or_insert(t);
        let mut done = true;
        for logo in LOGOS {
            let name = format!("datas/videos/{logo}.bik");
            loop {
                // A part of the disc asked ahead on its way: the next
                // frame (the browser reads it meanwhile).
                if crate::web::reads_pending() {
                    done = false;
                    break;
                }
                match self.src.warm_progress(&name, WARM_STEP) {
                    Some((at, to)) if at < to => {
                        self.warm = (at, to);
                        if t.elapsed().as_secs_f64() * 1e3 >= WARM_START_MS {
                            done = false;
                            break;
                        }
                    }
                    _ => break,
                }
            }
            if !done {
                break;
            }
        }
        if done {
            log::info!(
                "{NAME}: the films' folder read up to the logos in {:.1} s",
                started.elapsed().as_secs_f64()
            );
            return true;
        }
        let (at, to) = self.warm;
        crate::a5_bar::draw(&mut self.picture, VIEW_WIDTH, VIEW_HEIGHT, at, to);
        false
    }

    /// The game made once the logos' folder is decoded.
    fn ready(&mut self, el: &ActiveEventLoop) -> bool {
        if self.play.is_some() {
            return true;
        }
        if !self.warm_start() {
            return false;
        }
        let src: Arc<dyn FileSource> = self.src.clone();
        match Play::new(src, &self.lang, Some(self.audio.clone())) {
            Ok(mut play) => {
                play.set_saves(Some(Rc::new(WebSaves(self.store.clone()))));
                // The saves' dates in the browser's time zone.
                play.menus.utc_offset = crate::a2::utc_offset_now;
                play.boot();
                self.play = Some(play);
                self.last = None;
                true
            }
            Err(e) => {
                log::error!("{NAME}: {e}");
                crate::web::notice(&format!("{NAME}: {e}"));
                self.done = true;
                el.exit();
                false
            }
        }
    }

    fn redraw(&mut self, el: &ActiveEventLoop) {
        let Some(window) = self.window.clone() else {
            return;
        };
        crate::web::fit_canvas(&window, true);
        self.locked();
        if !self.ready(el) {
            if !self.done {
                self.present();
                window.request_redraw();
            }
            return;
        }
        // The desktop's pace (`a5.rs` FRAME, the original's frame at a
        // 60 Hz refresh): a redraw per screen refresh runs the steps due
        // since the last one, none on most redraws of a 120 or 144 Hz
        // screen, two at 30 Hz. The look's rule is per frame and not
        // linear in it (play.md § 2), so a step per redraw turned the view
        // slower on a fast screen. As on the desktop, a pause longer than
        // a quarter second (a hidden page, a long read) counts as a
        // quarter second.
        let now = Instant::now();
        if self.last.is_none() {
            // The game's first step: due now, not since the page opened.
            self.next_frame = now;
        }
        let dts = crate::web_pace::step_dts(&mut self.next_frame, &mut self.last, now, FRAME, 0.25);
        let Some(play) = self.play.as_mut() else {
            return;
        };
        run_test_commands(play);
        if dts.is_empty() {
            // Nothing due: the picture as it is (drawn again only when the
            // canvas's size changed, which clears it).
            if self.shown_size != Some(window.inner_size()) {
                self.present();
            }
            window.request_redraw();
            return;
        }
        for dt in dts {
            self.clock += dt;
            let Some(play) = self.play.as_mut() else {
                return;
            };
            crate::audio_out::web_out::keep_ahead();
            play.update(dt);
            crate::audio_out::web_out::keep_ahead();
            self.play_silently(dt);
            let Some(play) = self.play.as_mut() else {
                return;
            };
            if play.quit_requested() {
                self.done = true;
                a2_web::exit_lock();
                a2_web::quit();
                el.exit();
                return;
            }
        }
        let Some(play) = self.play.as_mut() else {
            return;
        };
        // A menu frees the pointer (the game draws its cursor); play
        // takes it again at the next click.
        let menu = play.in_menu();
        if menu != self.menu {
            self.menu = menu;
            if menu {
                a2_web::exit_lock();
            }
        }
        play.render(&mut self.picture);
        self.present();
        // The other folders decoded a little, so that the game's later
        // reads seldom decode a folder from its start.
        if !self.warm_done {
            let w = Instant::now();
            // Not while a part of the disc asked ahead is on its way: the
            // step would wait for it (tens of ms).
            while w.elapsed().as_secs_f64() * 1e3 < WARM_PLAY_MS && !crate::web::reads_pending() {
                if !self.src.warm_next(WARM_STEP) {
                    self.warm_done = true;
                    log::info!("{NAME}: every folder of the cabinets read");
                    break;
                }
            }
        }
        crate::audio_out::web_out::keep_ahead();
        if self.state_in == 0 {
            self.state_in = 15;
            let locked = self.locked();
            if let Some(play) = &self.play {
                record_state(play, self.clock, self.out.is_some(), locked, self.warm_done);
            }
        } else {
            self.state_in -= 1;
        }
        window.request_redraw();
    }
}

impl ApplicationHandler for A5App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        use winit::platform::web::{WindowAttributesExtWebSys, WindowExtWebSys};
        let attrs = Window::default_attributes()
            .with_title(format!("{NAME} - Atlantis Native"))
            .with_append(true);
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                log::error!("No window can be opened: {e}");
                el.exit();
                return;
            }
        };
        if let Some(canvas) = window.canvas() {
            let _ = canvas.focus();
            self.gl = GlPresenter::new(canvas);
        }
        if self.gl.is_none() {
            log::error!("WebGL 2 is needed for {NAME}");
        }
        // The original hides the system cursor (ShowCursor(0)) and draws
        // its own.
        window.set_cursor_visible(false);
        window.request_redraw();
        self.window = Some(window);
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        el.set_control_flow(ControlFlow::Wait);
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if !a2_web::pointer_locked() || self.menu {
                return;
            }
            let k = a2_web::motion_scale();
            let (x, y) = crate::web_pace::whole_motion(&mut self.motion_acc, dx * k, dy * k);
            if let Some(p) = self.play.as_mut() {
                if x != 0 || y != 0 {
                    p.look(x as f32, y as f32);
                }
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.done {
            return;
        }
        if let WindowEvent::RedrawRequested = event {
            self.redraw(el);
            return;
        }
        match &event {
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => self.gesture(),
            WindowEvent::KeyboardInput { event: k, .. } if k.state == ElementState::Pressed => {
                self.gesture();
            }
            _ => {}
        }
        if self.play.is_none() {
            return;
        }
        match event {
            WindowEvent::Occluded(_) => {
                // Back from a hidden page: the time away is not game time.
                if !a2_web::page_hidden() {
                    self.last = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let before = self.cursor.replace(position);
                let (x, y) = self.to_picture(position);
                let before = before.map(|b| self.to_picture(b));
                let menu = self.menu;
                let Some(play) = self.play.as_mut() else {
                    return;
                };
                if menu {
                    play.mouse_at(x, y);
                } else if !a2_web::pointer_locked() {
                    // Without the lock the page's pointer turns the view.
                    if let Some((bx, by)) = before {
                        play.look((x - bx) as f32, (y - by) as f32);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if self.may_ask_lock() {
                    if let Some(w) = &self.window {
                        self.lock_asked = Some(Instant::now());
                        a2_web::request_lock(w);
                    }
                }
                if let Some(play) = self.play.as_mut() {
                    play.click();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                if let Some(play) = self.play.as_mut() {
                    play.right_press();
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                let Some(play) = self.play.as_mut() else {
                    return;
                };
                match event.logical_key {
                    // Escape: a film ends, or the in-game menu (the
                    // browser keeps it to let the pointer lock go).
                    Key::Named(NamedKey::Escape) => play.escape(),
                    // Space: a film or a dialog ends.
                    Key::Named(NamedKey::Space) => play.space(),
                    // The port's own: the page's full screen.
                    Key::Named(NamedKey::F11) => a2_web::toggle_fullscreen(),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

thread_local! {
    /// The testing aids' commands, run at the next pass.
    static COMMANDS: RefCell<VecDeque<String>> = const { RefCell::new(VecDeque::new()) };
    /// The game's state after the last recorded pass (JSON).
    static STATE: RefCell<String> = const { RefCell::new(String::new()) };
}

/// A testing aid for the end-to-end runs, run before the next pass:
/// `click <name>` clicks the menu's element `name` (its middle).
#[wasm_bindgen]
pub fn atlantis5_test(command: String) {
    COMMANDS.with(|c| c.borrow_mut().push_back(command));
}

/// The game's state, for the end-to-end runs (JSON: the game manager's
/// state, the menu up and its elements, the logos or a film playing, the
/// place, whether the sound plays and the pointer is locked, whether
/// every cabinet folder has been decoded, the clock).
#[wasm_bindgen]
pub fn atlantis5_state() -> String {
    STATE.with(|s| s.borrow().clone())
}

fn run_test_commands(play: &mut Play) {
    let commands: Vec<String> = COMMANDS.with(|c| c.borrow_mut().drain(..).collect());
    for c in commands {
        match c.split_whitespace().collect::<Vec<_>>().as_slice() {
            ["click", name] => {
                if !play.click_menu(name) {
                    log::warn!("test: no element {name} in the menu");
                }
            }
            _ => log::warn!("test: {c}?"),
        }
    }
}

fn record_state(play: &Play, clock: f64, audio: bool, locked: bool, warmed: bool) {
    let clean = |s: &str| s.replace(['"', '\\'], "");
    let items: Vec<String> = play
        .menus
        .shown()
        .into_iter()
        .filter(|e| e.2)
        .map(|e| format!("\"{}\"", clean(&e.0)))
        .collect();
    let stage = play.stage();
    let place = stage.address().map_or("null".to_string(), |(scene, view)| {
        format!("[\"{}\",{view}]", clean(&String::from_utf8_lossy(&scene)))
    });
    let s = format!(
        "{{\"state\":{},\"menu\":{},\"items\":[{}],\"logos\":{},\"film\":{},\"place\":{place},\
         \"audio\":{audio},\"locked\":{locked},\"warmed\":{warmed},\"underruns\":{},\"clock\":{clock:.3}}}",
        play.game.state(),
        play.menus
            .current()
            .map_or("null".to_string(), |m| m.to_string()),
        items.join(","),
        play.logos_playing(),
        stage.film_playing(),
        crate::audio_out::underruns(),
    );
    STATE.with(|st| *st.borrow_mut() = s);
}

/// The picture's place in a canvas of `ww` × `wh` (x, y, width, height).
fn place_in(ww: usize, wh: usize) -> (usize, usize, usize, usize) {
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

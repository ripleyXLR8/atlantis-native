//! Atlantis III in the browser: the desktop's window (`crate::a3`) in a
//! page: the intro, the start page, the menus, the saves, the sound, the
//! original's keys and its display sizes and depths (the options'
//! switch), the frame presented through WebGL 2.
//!
//! The game's files are the picked folder's, read when the engine reads
//! them ([`crate::web`]'s `PickedDir`). The saves (with their thumbnails)
//! and `Atlantis3.cfg` are kept in the browser's `localStorage` under
//! `atlantis-native/atlantis3/`, one key per file as the other games'.
//!
//! The mouse: a click takes the pointer lock (the browser grants it on a
//! click only); held, the mouse's motion is the original's DirectInput
//! motion (the view turns, the free pointer moves). Without the lock
//! (refused, or let go with Escape) the page's pointer is the game's in
//! the menus and the inventory, and its motion turns the view.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use atlantis3_data::A3Files;
use atlantis3_engine::px::Surface;
use atlantis3_engine::{Game, Mode};
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

use crate::a2_web::{self, FrameTimes};
use crate::gl_present::GlPresenter;

/// `Atlantis3.cfg`'s key in the storage.
const CFG_KEY: &str = "atlantis3.cfg";

/// A pause of the redraws longer than this (a hidden tab, the browser
/// busy) is not game time.
const GAP: f64 = 0.25;

/// The install with `Datas\Atlantis3.cfg` taken from the browser's
/// storage once the menus wrote it there.
struct WithCfg {
    install: Arc<dyn FileSource>,
    cfg: Option<Vec<u8>>,
}

impl WithCfg {
    fn is_cfg(name: &str) -> bool {
        name.replace('\\', "/")
            .eq_ignore_ascii_case(atlantis3_data::cfg::CFG_PATH)
    }
}

impl FileSource for WithCfg {
    fn size(&self, name: &str) -> Option<u64> {
        match &self.cfg {
            Some(d) if Self::is_cfg(name) => Some(d.len() as u64),
            _ => self.install.size(name),
        }
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        match &self.cfg {
            Some(d) if Self::is_cfg(name) => {
                let start = usize::try_from(offset).ok()?.min(d.len());
                Some(d[start..(start + len).min(d.len())].to_vec())
            }
            _ => self.install.read_at(name, offset, len),
        }
    }

    fn has_dir(&self, name: &str) -> bool {
        self.install.has_dir(name)
    }

    fn list(&self, dir: &str) -> Vec<String> {
        self.install.list(dir)
    }
}

/// Starts Atlantis III from its installed folder `install`, the saves in
/// `store`.
pub fn start(install: Arc<dyn FileSource>, store: Rc<dyn SaveStore>) -> Result<(), JsValue> {
    let cfg = store.read(CFG_KEY);
    let files = A3Files::new(Arc::new(WithCfg {
        install,
        cfg: cfg.clone(),
    }));
    let label = files
        .edition_label()
        .map_err(|e| JsValue::from(format!("Atlantis III: {e}")))?;
    let mut game = Game::new(files).map_err(|e| JsValue::from(format!("Atlantis III: {e}")))?;
    game.unix_time = || (js_sys::Date::now() / 1000.0) as i64;
    let mut kept = 0;
    for name in store.list() {
        if name == CFG_KEY {
            continue;
        }
        if let Some(d) = store.read(&name) {
            game.insert_save(&name, d);
            kept += 1;
        }
    }
    log::info!(
        "{label}, {} files kept in the browser, {}",
        kept,
        crate::version_string()
    );
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.spawn_app(A3App::new(game, store, cfg));
    Ok(())
}

/// The window's application.
struct A3App {
    game: Game,
    store: Rc<dyn SaveStore>,
    /// `Atlantis3.cfg` as last kept.
    cfg_kept: Option<Vec<u8>>,
    window: Option<Rc<Window>>,
    gl: Option<GlPresenter>,
    frame: Vec<u32>,
    /// The game's clock (seconds) and the last redraw.
    clock: f64,
    last: Option<Instant>,
    intro_started: bool,
    intro_done: bool,
    /// The sound, started on the first click or key (a browser allows it
    /// only then).
    audio: Option<cpal::Stream>,
    audio_pending: bool,
    /// The pointer lock: when it was asked for, whether it was refused.
    lock_asked: Option<Instant>,
    lock_refused: bool,
    /// The fractions of the pointer-lock motion not yet sent.
    motion_acc: (f64, f64),
    /// The page's pointer at the last move (canvas pixels).
    cursor: Option<PhysicalPosition<f64>>,
    alt: bool,
    times: FrameTimes,
    state_in: u32,
    done: bool,
}

impl A3App {
    fn new(mut game: Game, store: Rc<dyn SaveStore>, cfg: Option<Vec<u8>>) -> A3App {
        // The saves' `DateAndTime` in the browser's time zone.
        game.utc_offset = crate::a2::utc_offset_now;
        A3App {
            game,
            store,
            cfg_kept: cfg,
            window: None,
            gl: None,
            frame: Vec::new(),
            clock: 0.0,
            last: None,
            intro_started: false,
            intro_done: false,
            audio: None,
            audio_pending: true,
            lock_asked: None,
            lock_refused: false,
            motion_acc: (0.0, 0.0),
            cursor: None,
            alt: false,
            times: FrameTimes::default(),
            state_in: 0,
            done: false,
        }
    }

    /// The first click or key: the sound starts (silent while the page is
    /// hidden: the browser stops the redraws, not the output).
    fn gesture(&mut self) {
        if !std::mem::take(&mut self.audio_pending) {
            return;
        }
        let audio = self.game.audio.clone();
        self.audio = crate::audio_out::start_with(move |out, rate, channels| {
            if a2_web::page_hidden() {
                out.fill(0.0);
            } else {
                audio.fill(out, rate, channels);
            }
        });
    }

    /// Whether the page holds the pointer lock; a request not granted in
    /// time is taken as refused (clicks then act at the page's pointer).
    fn locked(&mut self) -> bool {
        if a2_web::pointer_locked() {
            self.lock_asked = None;
            return true;
        }
        if self
            .lock_asked
            .is_some_and(|t| t.elapsed() >= a2_web::LOCK_GRACE)
        {
            self.lock_asked = None;
            if !self.lock_refused {
                log::info!("pointer lock refused: the page's pointer is the game's");
            }
            self.lock_refused = true;
        }
        false
    }

    /// The modes whose pointer is free (@ 0x004313f6).
    fn free_pointer(&self) -> bool {
        matches!(
            self.game.mode(),
            Mode::Inventory | Mode::Subjects | Mode::Menu | Mode::Screen
        )
    }

    /// A canvas position on the game's screen (the picture fills the
    /// 4:3 rectangle of the canvas).
    fn to_screen(&self, p: PhysicalPosition<f64>) -> Option<(i32, i32)> {
        let size = self.window.as_ref()?.inner_size();
        let (ox, oy, dw, dh) = atlantis2_engine::hd::picture_rect(
            size.width.max(1) as usize,
            size.height.max(1) as usize,
        );
        let (gw, gh) = (self.game.width() as f64, self.game.height() as f64);
        let x = (p.x - ox as f64) * gw / dw.max(1) as f64;
        let y = (p.y - oy as f64) * gh / dh.max(1) as f64;
        Some((x.floor() as i32, y.floor() as i32))
    }

    /// The saves and `Atlantis3.cfg` written since the last pass, kept.
    fn keep_files(&mut self) {
        for (name, data) in self.game.take_written_saves() {
            self.store.write(&name, &data);
        }
        let cfg = self.game.menus().cfg_written.clone();
        if let Some(c) = cfg.filter(|c| self.cfg_kept.as_ref() != Some(c)) {
            self.store.write(CFG_KEY, &c);
            self.cfg_kept = Some(c);
        }
    }

    /// The game's screen to the canvas.
    fn present(&mut self) {
        let (Some(window), Some(gl)) = (&self.window, self.gl.as_mut()) else {
            return;
        };
        let size = window.inner_size();
        let (ww, wh) = (size.width.max(1), size.height.max(1));
        let (w, h) = (self.game.width(), self.game.height());
        self.frame.clear();
        match self.game.surface() {
            Surface::Rgb565(s) => {
                let lut = atlantis2_engine::png::rgb565_to_xrgb();
                self.frame
                    .extend(s[..w * h].iter().map(|&p| lut[usize::from(p)]));
            }
            // A 32-bit display: the pixels as they are (x byte dropped).
            Surface::Xrgb8888(s) => self.frame.extend(s[..w * h].iter().map(|&p| p & 0xff_ffff)),
        }
        let (ox, oy, dw, dh) = atlantis2_engine::hd::picture_rect(ww as usize, wh as usize);
        gl.present(
            (ww, wh),
            &self.frame,
            (w as u32, h as u32),
            (ox as i32, oy as i32, dw as i32, dh as i32),
            false,
            None,
        );
    }

    fn redraw(&mut self, el: &ActiveEventLoop) {
        let Some(window) = self.window.clone() else {
            return;
        };
        crate::web::fit_canvas(&window, false);
        // The lock looked at every redraw: one granted and lost between
        // two looks is not taken for a refusal.
        self.locked();
        let t = Instant::now();
        if let Some(last) = self.last.replace(t) {
            let dt = t.saturating_duration_since(last).as_secs_f64();
            if dt <= GAP {
                self.clock += dt;
            }
        }
        run_test_commands(&mut self.game, &mut self.times);
        if !self.intro_started {
            self.intro_started = true;
            self.game.start_intro();
        }
        self.game.step(self.clock);
        if !self.intro_done && self.game.mode() == Mode::Idle {
            self.intro_done = true;
            // The start page after the intro (mode 6, page 0).
            if !self.game.show_start_page() {
                self.game.new_game();
            }
        }
        self.keep_files();
        if self.game.quit {
            self.done = true;
            a2_web::exit_lock();
            a2_web::quit();
            el.exit();
            return;
        }
        let pass = t.elapsed().as_secs_f64() * 1e3;
        let p = Instant::now();
        self.present();
        let present = p.elapsed().as_secs_f64() * 1e3;
        self.times.add(frame_kind(self.game.mode()), pass, present);
        if self.state_in == 0 {
            self.state_in = 15;
            let locked = self.locked();
            record_state(
                &self.game,
                self.clock,
                &self.times,
                self.audio.is_some(),
                locked,
            );
        } else {
            self.state_in -= 1;
        }
        window.request_redraw();
    }
}

impl ApplicationHandler for A3App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        use winit::platform::web::{WindowAttributesExtWebSys, WindowExtWebSys};
        let attrs = Window::default_attributes()
            .with_title("Atlantis III - Atlantis Native")
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
            log::error!("WebGL 2 is needed for Atlantis III");
        }
        window.set_cursor_visible(false);
        window.request_redraw();
        self.window = Some(window);
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // The redraws keep themselves going (one per screen refresh).
        el.set_control_flow(ControlFlow::Wait);
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if a2_web::pointer_locked() {
                // winit's motion back to the page's own counts, the same
                // in every browser; the fractions kept for the next motion
                // (dropped, slow moves were lost at a page zoom or a
                // devicePixelRatio of 1.25 or 1.5).
                let k = a2_web::motion_scale();
                let (x, y) = crate::web_pace::whole_motion(&mut self.motion_acc, dx * k, dy * k);
                if x != 0 || y != 0 {
                    self.game.mouse_motion(x, y);
                }
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.done {
            return;
        }
        match event {
            WindowEvent::RedrawRequested => self.redraw(el),
            WindowEvent::ModifiersChanged(m) => self.alt = m.state().alt_key(),
            WindowEvent::Occluded(_) => {
                // Back from a hidden page: the time away is not game time.
                if !a2_web::page_hidden() {
                    self.last = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let before = self.cursor.replace(position);
                if a2_web::pointer_locked() {
                    return;
                }
                if self.free_pointer() {
                    if let Some((x, y)) = self.to_screen(position) {
                        self.game.set_pointer(x, y);
                    }
                } else if let (Some(a), Some(b)) = (
                    before.and_then(|b| self.to_screen(b)),
                    self.to_screen(position),
                ) {
                    self.game.mouse_motion(b.0 - a.0, b.1 - a.1);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                if pressed {
                    self.gesture();
                }
                match (state, button) {
                    (ElementState::Pressed, MouseButton::Left) => {
                        if !self.locked() && !self.lock_refused && self.lock_asked.is_none() {
                            if let Some(w) = &self.window {
                                self.lock_asked = Some(Instant::now());
                                a2_web::request_lock(w);
                            }
                        }
                        self.game.click();
                    }
                    (ElementState::Released, MouseButton::Left) => self.game.release(),
                    (ElementState::Pressed, MouseButton::Right) => self.game.right_click(),
                    _ => {}
                }
            }
            // The original reads key presses only (DirectInput's buffer,
            // @ 0x0042ab20; the cinematics' `WM_KEYDOWN`): no repeats.
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                self.gesture();
                match event.logical_key {
                    // Alt+Enter (@ 0x0042ab9d, not while a puzzle freezes
                    // the keys, @ 0x0042ab20) and the port's F11: the
                    // page's full screen.
                    Key::Named(NamedKey::Enter) if self.alt && !self.game.keys_frozen() => {
                        a2_web::toggle_fullscreen()
                    }
                    Key::Named(NamedKey::F11) => a2_web::toggle_fullscreen(),
                    Key::Named(NamedKey::Escape) if self.game.credits().is_some() => {
                        if self.game.credits_escape() {
                            let data = self
                                .game
                                .startup
                                .config
                                .write(&self.game.startup.install_language);
                            self.store.write(CFG_KEY, &data);
                            self.cfg_kept = Some(data);
                        }
                    }
                    // By loop mode (@ 0x0042ac50).
                    Key::Named(NamedKey::Escape) => self.game.escape(),
                    // A cinematic's wait loop only (@ 0x00421c5b).
                    Key::Named(NamedKey::Space) => self.game.skip(),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// What a frame shows, for [`FrameTimes`].
fn frame_kind(mode: Mode) -> &'static str {
    match mode {
        Mode::Warp => "view",
        Mode::Video => "video",
        Mode::Dialog => "dialog",
        Mode::Menu => "menu",
        Mode::Inventory | Mode::Subjects => "inventory",
        Mode::Screen => "screen",
        Mode::Intro => "intro",
        _ => "other",
    }
}

thread_local! {
    /// The testing aids' commands, run at the next pass.
    static COMMANDS: RefCell<VecDeque<String>> = const { RefCell::new(VecDeque::new()) };
    /// The game's state after the last recorded pass (JSON).
    static STATE: RefCell<String> = const { RefCell::new(String::new()) };
    /// The point `find` found (game pixels).
    static FOUND: RefCell<Option<(i32, i32)>> = const { RefCell::new(None) };
}

/// A testing aid for the end-to-end runs, run before the next pass:
/// `pointer <x> <y>` puts the free pointer there (game pixels); `find
/// <name>` finds a point of the menu page's element `name` (its `nomref`
/// or bank) that a click hits, for the state's `found` (the runner then
/// clicks there for real); `times` clears the frame times.
#[wasm_bindgen]
pub fn atlantis3_test(command: String) {
    COMMANDS.with(|c| c.borrow_mut().push_back(command));
}

/// The game's state, for the end-to-end runs (JSON: the loop mode, the
/// place and view, the screen's size and depth, the free pointer, the
/// menu page, the clock, the frame times).
#[wasm_bindgen]
pub fn atlantis3_state() -> String {
    STATE.with(|s| s.borrow().clone())
}

fn run_test_commands(game: &mut Game, times: &mut FrameTimes) {
    let commands: Vec<String> = COMMANDS.with(|c| c.borrow_mut().drain(..).collect());
    for c in commands {
        let parts: Vec<&str> = c.split_whitespace().collect();
        match parts.as_slice() {
            ["pointer", x, y] => match (x.parse(), y.parse()) {
                (Ok(x), Ok(y)) => game.set_pointer(x, y),
                _ => log::warn!("test: {c}?"),
            },
            ["times"] => times.clear(),
            ["find", name] => {
                let found = find_element(game, name);
                if found.is_none() {
                    log::warn!("test: no element {name} on the page");
                }
                FOUND.with(|f| *f.borrow_mut() = found);
            }
            _ => log::warn!("test: {c}?"),
        }
    }
}

/// The menu page's elements (`nomref`).
fn page_items(game: &Game) -> Vec<String> {
    let m = game.menus();
    m.get(m.page)
        .filter(|_| game.mode() == Mode::Menu)
        .map(|p| {
            p.items
                .iter()
                .map(|it| String::from_utf8_lossy(&it.el.nomref).into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// A point of the screen where a click hits the page's element `name`
/// (its `nomref`, or its bank's name).
fn find_element(game: &Game, name: &str) -> Option<(i32, i32)> {
    let m = game.menus();
    let page = m.get(m.page)?;
    let same = |b: &[u8]| String::from_utf8_lossy(b).eq_ignore_ascii_case(name);
    let k = page
        .items
        .iter()
        .position(|it| same(&it.el.nomref) || it.el.nomspr.as_deref().is_some_and(same))?;
    let (w, h) = (game.width() as i32, game.height() as i32);
    let mut hits = Vec::new();
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            if page.click_hit((x, y)) == Some(k) {
                hits.push((x, y));
            }
        }
    }
    // The middle one of the hits (away from the edges).
    hits.get(hits.len() / 2).copied()
}

fn record_state(game: &Game, clock: f64, times: &FrameTimes, audio: bool, locked: bool) {
    let (wam, view) = game.place();
    let page = game
        .menu_page()
        .map_or("null".to_string(), |(p, s)| format!("[{p},{s}]"));
    let (px, py) = game.pointer();
    let items: Vec<String> = page_items(game)
        .iter()
        .map(|n| format!("\"{}\"", n.replace(['"', '\\'], "")))
        .collect();
    let found = FOUND
        .with(|f| *f.borrow())
        .map_or("null".to_string(), |(x, y)| format!("[{x},{y}]"));
    let s = format!(
        "{{\"mode\":\"{:?}\",\"wam\":\"{}\",\"view\":{},\"width\":{},\"height\":{},\
         \"depth\":\"{:?}\",\"pointer\":[{px},{py}],\"page\":{page},\"audio\":{audio},\
         \"locked\":{locked},\"items\":[{}],\"found\":{found},\"clock\":{:.3},\"times\":{}}}",
        game.mode(),
        wam.replace(['"', '\\'], ""),
        view.map_or("null".to_string(), |v| v.to_string()),
        game.width(),
        game.height(),
        game.depth(),
        items.join(","),
        clock,
        times.report()
    );
    STATE.with(|st| *st.borrow_mut() = s);
}

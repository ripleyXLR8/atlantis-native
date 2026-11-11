//! Atlantis Evolution in the browser: the desktop's viewer ([`super`]) in
//! a page.
//!
//! - **The files**: the install folder the player picked
//!   (`<input webkitdirectory>`), each file read when the engine reads it,
//!   the part it reads ([`crate::web`]'s `PickedDir`): nothing of the
//!   install is copied into the page. The movie colour table is read from
//!   the picked `Atlantis Evolution.exe`, as on the desktop.
//! - **The picture**: the graphics card's renderer on the canvas's WebGL 2
//!   context (`a4_gl_web.rs`), or the processor's on a 2D canvas when the
//!   browser has no WebGL 2 or the page's storage says
//!   `atlantis-native/gpu-view` = `off`; `check` draws every second frame
//!   by both and compares them, as the desktop's `--gpu-check`.
//! - **The pace**: a redraw per screen refresh runs the game's updates due
//!   at the desktop's 60 a second ([`crate::web_pace::steps_due`]); a
//!   redraw gap over a quarter of a second (a hidden page, the browser
//!   busy) is not game time.
//! - **The sound**: the exact mix at 44,100 Hz through the page's audio
//!   output, started at the first click or key (a browser allows it only
//!   then); until then the engine plays the sounds out silently on the
//!   game's clock (`Sounds::headless`), so that a voice's end still comes.
//!   Silent while the page is hidden.
//! - **The saves and `options.xml`**: the backup folder of the desktop,
//!   `Documents/Atlantis Evolution/`, kept in the page's `localStorage`
//!   under `atlantis-native/atlantis4/`, one key per file: written as the
//!   game writes them (`options.xml` as the settings change: a page has no
//!   exit).

use std::collections::BTreeMap;
use std::sync::Arc;

use atlantis4_data::A4Files;
use atlantis4_engine::saves::{self as backup, BackupFolder};
use atlantis4_script::{read_boot_vars, BootValue, EngineRef, ErrorMode};
use atlantis_data::FileSource;
use wasm_bindgen::prelude::*;
use winit::event_loop::EventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::platform::web::EventLoopExtWebSys;

use super::*;
use crate::a2_web::{self, FrameTimes};

/// A redraw longer than this is logged: the sound's output is filled
/// about this far ahead, so a longer one may leave a gap in it.
const SLOW_REDRAW_MS: f64 = 80.0;

/// The sound's output buffers: 4,096 frames (93 ms at 44,100 Hz), twice
/// cpal's default, so that a node change (50 to 90 ms of the page's
/// thread) leaves no gap in the ambience; the sound comes 46 ms later.
pub const OUTPUT_FRAMES: u32 = 4096;

/// The page's part of the viewer.
pub struct WebPart {
    /// The page's storage, and the backup folder's path in it.
    store: Rc<dyn atlantis_engine::SaveStore>,
    dir: String,
    /// The backup folder's files as last kept.
    kept: BTreeMap<String, Vec<u8>>,
    /// `options.xml` as last kept, and whether the settings changed.
    options_kept: Option<Vec<u8>>,
    pub options_due: bool,
    /// The sound, started on the first click or key.
    audio: Option<cpal::Stream>,
    audio_pending: bool,
    /// The end of the last redraw and the read time then.
    last_redraw: Option<(Instant, Duration)>,
    alt: bool,
    times: FrameTimes,
    state_in: u32,
    /// The late sound buffers when the frame times were last cleared.
    underruns0: u32,
    /// How the frame is drawn ("card", "processor").
    by: &'static str,
}

impl WebPart {
    fn key(&self, name: &str) -> String {
        format!("{}/{name}", self.dir)
    }
}

/// The backup folder's path for the boot variables
/// (`Documents/Atlantis Evolution`): the desktop's below its data folder.
fn backup_dir(boot: &atlantis4_script::BootVars) -> String {
    let s = |v: &Option<BootValue>| match v {
        Some(BootValue::Str(s)) => Some(s.iter().map(|&c| char::from(c)).collect::<String>()),
        _ => None,
    };
    let template = s(&boot.backup_path_template).unwrap_or_default();
    let app = s(&boot.application_title).unwrap_or_default();
    backup::native_backup_dir(std::path::Path::new(""), &template, &app)
        .to_string_lossy()
        .trim_matches('/')
        .to_string()
}

/// The page's storage setting of the graphics card's drawing.
fn render_setting() -> Render {
    let setting = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("atlantis-native/gpu-view").ok().flatten())
        .unwrap_or_default();
    Render {
        cpu: setting == "off",
        check: setting == "check",
    }
}

/// Starts Atlantis Evolution from its install `install` (the picked
/// folder), the saves in `store`.
pub fn start(
    install: Arc<dyn FileSource>,
    store: Rc<dyn atlantis_engine::SaveStore>,
    language: Option<&str>,
) -> Result<(), JsValue> {
    let files = A4Files::new(install);
    let build = files
        .build()
        .map_err(|e| JsValue::from(format!("Atlantis Evolution: {e}")))?;
    let clock = GameClock::new();
    let mut g = Game::new(files, HEIGHT);
    if g.table.is_none() {
        log::warn!("no movie colour table: the movies stay black");
    }
    let c = clock.clone();
    g.clock_us = Some(Box::new(move || c.us()));
    g.app_planks = true;
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let options = Options {
        errors: ErrorMode::Log,
        ..Options::default()
    };
    let boot = read_boot_vars(&engine, options).map_err(|e| e.to_string())?;
    // The saves in memory, filled from the page's storage and kept back
    // there as the game writes them.
    game.borrow_mut().configure_saves(&boot, None);
    let dir = backup_dir(&boot).to_ascii_lowercase();
    let prefix = format!("{dir}/");
    let opts_name = format!("{prefix}{}", options::FILE);
    let mut kept = BTreeMap::new();
    for name in store.list() {
        let Some(file) = name.strip_prefix(&prefix) else {
            continue;
        };
        if name == opts_name || file.contains('/') {
            continue;
        }
        if let Some(d) = store.read(&name) {
            kept.insert(file.to_string(), d);
        }
    }
    log::info!(
        "Atlantis Evolution ({}), {} saves kept in the browser, {}",
        build.label,
        kept.len(),
        crate::version_string()
    );
    if let BackupFolder::Memory(m) = &mut game.borrow_mut().backup {
        *m = kept.clone();
    }
    let options_kept = store.read(&opts_name);
    let settings = options_kept
        .as_deref()
        .map(Settings::read)
        .unwrap_or_default();
    game.borrow_mut().apply_settings(&settings);
    if let Some(l) = language {
        game.borrow_mut().set_language(Some(l));
    }
    log::info!("language {}", game.borrow().lang);
    let version = match &boot.version_string {
        Some(BootValue::Str(s)) => s.iter().map(|&c| char::from(c)).collect(),
        _ => String::new(),
    };
    let files = game.borrow().files.clone();
    let mut front = Front::new(&files, settings, version);
    let saves = GameSaves {
        game: game.clone(),
        options,
    };
    front.start(0.0, &saves);
    let render = render_setting();
    let viewer = Viewer {
        _audio: None,
        game,
        script: None,
        front,
        saves,
        web: WebPart {
            store,
            dir,
            kept,
            options_kept,
            options_due: false,
            audio: None,
            audio_pending: true,
            last_redraw: None,
            alt: false,
            times: FrameTimes::default(),
            state_in: 0,
            underruns0: 0,
            by: "processor",
        },
        clock,
        window: None,
        surface: None,
        gl: None,
        render,
        cpu_cache: CpuCache::default(),
        check_buf: Vec::new(),
        last_check: None,
        last_diff: None,
        pointer: None,
        down: None,
        gui_down: false,
        last: (0.0, 0.0),
        size: (WIDTH, HEIGHT),
        next_frame: Instant::now(),
        shown: None,
        stats: Stats::default(),
        quit: false,
    };
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.spawn_app(viewer);
    Ok(())
}

impl Viewer {
    /// The canvas added to the page; the card's renderer on it unless the
    /// page's setting says `off` or the browser has no WebGL 2.
    pub(super) fn resumed_web(&mut self, el: &ActiveEventLoop) {
        use winit::platform::web::{WindowAttributesExtWebSys, WindowExtWebSys};
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Atlantis Evolution - Atlantis Native")
            .with_append(true);
        let mut window = None;
        if !self.render.cpu {
            let (win, view) = crate::a4_gl::create(el, attrs.clone());
            window = win;
            match view {
                Ok(v) => {
                    log::info!("drawn by the graphics card: {}", v.renderer.info);
                    self.gl = Some(v);
                    self.web.by = "card";
                }
                Err(e) => log::warn!("no WebGL 2 ({e}): drawn by the processor"),
            }
        }
        let window = match window {
            Some(w) => Rc::new(w),
            None => match el.create_window(attrs) {
                Ok(w) => Rc::new(w),
                Err(e) => {
                    log::error!("no window: {e}");
                    el.exit();
                    return;
                }
            },
        };
        if self.gl.is_none() {
            // A canvas with a WebGL context refuses a 2D one: a new canvas.
            if self.web.by != "processor" || !self.render.cpu {
                log::info!("drawn by the processor on a 2D canvas");
            }
            match softbuffer::Context::new(window.clone())
                .and_then(|c| softbuffer::Surface::new(&c, window.clone()))
            {
                Ok(s) => self.surface = Some(s),
                Err(e) => {
                    log::error!("no picture in the page: {e}");
                    el.exit();
                    return;
                }
            }
        }
        self.stats.by = self.web.by;
        if let Some(canvas) = window.canvas() {
            let _ = canvas.focus();
        }
        // The game draws its own cursor (`2D/arrow6.png`).
        window.set_cursor_visible(false);
        fit_canvas(&window);
        let s = window.inner_size();
        self.size = (s.width, s.height);
        self.next_frame = Instant::now();
        window.request_redraw();
        self.window = Some(window);
    }

    /// The page's events; true when taken.
    pub(super) fn web_event(&mut self, el: &ActiveEventLoop, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::RedrawRequested => {
                self.redraw_web(el);
                true
            }
            WindowEvent::ModifiersChanged(m) => {
                self.web.alt = m.state().alt_key();
                true
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => {
                self.gesture();
                false
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    self.gesture();
                    if !event.repeat {
                        match &event.logical_key {
                            // The port's full screen keys (the page's).
                            Key::Named(NamedKey::F11) => a2_web::toggle_fullscreen(),
                            Key::Named(NamedKey::Enter) if self.web.alt => {
                                a2_web::toggle_fullscreen()
                            }
                            Key::Character(c) if c.eq_ignore_ascii_case("f") => {
                                a2_web::toggle_fullscreen()
                            }
                            _ => {}
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// The first click or key: the sound's output starts, and the engine
    /// stops mixing on its own clock.
    fn gesture(&mut self) {
        if !std::mem::take(&mut self.web.audio_pending) {
            return;
        }
        let core = self.game.borrow().sounds.core();
        self.web.audio = start_sound(core);
        self.game.borrow_mut().sounds.headless = self.web.audio.is_none();
    }

    /// One redraw: the updates due, the saves kept, the frame drawn when
    /// it changed.
    fn redraw_web(&mut self, el: &ActiveEventLoop) {
        let Some(window) = self.window.clone() else {
            return;
        };
        fit_canvas(&window);
        let t = Instant::now();
        // A hidden page and a long gap between two redraws (from the end of
        // the last one: the game's own work, a file read, is game time as
        // on the desktop) are not game time; the game never saw that time,
        // so its clock does not go back.
        if let Some((last, reads)) = self.web.last_redraw {
            // (less the file reads made meanwhile, already held)
            let read = crate::web::read_time().saturating_sub(reads);
            let dt = t.saturating_duration_since(last).saturating_sub(read);
            if dt > a2_web::GAP || a2_web::page_hidden() {
                self.clock.hold(dt);
                self.next_frame = t;
            }
        }
        if a2_web::page_hidden() {
            self.web.last_redraw = Some((Instant::now(), crate::web::read_time()));
            window.request_redraw();
            return;
        }
        self.run_test_commands();
        let reads0 = crate::web::read_time();
        let (n, _) =
            crate::web_pace::steps_due(&mut self.next_frame, t + crate::web_pace::SLACK, FRAME);
        for _ in 0..n {
            self.tick(el);
            self.stats.frames += 1;
        }
        self.keep_files();
        let pass = t.elapsed().as_secs_f64() * 1e3;
        let key = self.frame_key();
        let mut present = 0.0;
        if self.shown != Some(key) {
            let p = Instant::now();
            self.draw();
            present = p.elapsed().as_secs_f64() * 1e3;
        }
        self.stats.tick += t.elapsed();
        self.stats.log();
        if n > 0 {
            let kind = self.frame_kind();
            self.web.times.add(kind, pass, present);
            if pass + present > SLOW_REDRAW_MS {
                log::info!(
                    "slow redraw ({kind}): {pass:.0} ms update, {present:.0} ms draw, {:.0} ms reading",
                    (crate::web::read_time() - reads0).as_secs_f64() * 1e3
                );
            }
        }
        if self.web.state_in == 0 {
            self.web.state_in = 15;
            self.record_state();
        } else {
            self.web.state_in -= 1;
        }
        self.web.last_redraw = Some((Instant::now(), crate::web::read_time()));
        if !self.quit {
            window.request_redraw();
        }
    }

    /// The backup folder's changed files and the settings kept in the
    /// page's storage.
    fn keep_files(&mut self) {
        {
            let g = self.game.borrow();
            if let BackupFolder::Memory(m) = &g.backup {
                if *m != self.web.kept {
                    for (name, data) in m {
                        if self.web.kept.get(name) != Some(data) {
                            self.web.store.write(&self.web.key(name), data);
                        }
                    }
                    for name in self.web.kept.keys() {
                        if !m.contains_key(name) {
                            self.web.store.remove(&self.web.key(name));
                        }
                    }
                    self.web.kept = m.clone();
                }
            }
        }
        if std::mem::take(&mut self.web.options_due) {
            self.write_options();
        }
    }

    /// `options.xml` kept in the page's storage when it changed.
    pub(super) fn write_options(&mut self) {
        let data = self.front.settings.write();
        if self.web.options_kept.as_deref() != Some(&data[..]) {
            let key = self.web.key(options::FILE);
            if self.web.store.write(&key, &data) {
                self.web.options_kept = Some(data);
            }
        }
    }

    /// What the frame shows, for the frame times.
    fn frame_kind(&mut self) -> &'static str {
        if self.front.active() || self.front.in_transition() {
            return "menu";
        }
        let mut g = self.game.borrow_mut();
        if g.movie.is_some() {
            "movie"
        } else if g.gui_shown() {
            "dialog"
        } else if g.documents.shown.is_some() {
            "document"
        } else if self.script.is_some() && g.gui().menu_shown {
            "inventory"
        } else {
            "view"
        }
    }
}

/// Outside full screen: the canvas as large as the page allows (less the
/// frame's margin), at the game's 16:9 (1365 × 768); the browser then
/// reports the new size as a resize.
fn fit_canvas(w: &Window) {
    use winit::platform::web::WindowExtWebSys;
    let Some(page) = web_sys::window() else {
        return;
    };
    if let Some(body) = page.document().and_then(|d| d.body()) {
        let _ = body.class_list().toggle_with_force("show-keys", false);
    }
    if page
        .document()
        .and_then(|d| d.fullscreen_element())
        .is_some()
    {
        return;
    }
    let Some(canvas) = w.canvas() else { return };
    let size = |v: Result<JsValue, _>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    let avail_w = (size(page.inner_width()) - 42.0).max(320.0);
    let avail_h = (size(page.inner_height()) - 42.0).max(180.0);
    let ratio = f64::from(WIDTH) / f64::from(HEIGHT);
    let h = avail_h.min(avail_w / ratio);
    let w = h * ratio;
    let (w, h) = (format!("{}px", w.floor()), format!("{}px", h.floor()));
    let style = canvas.style();
    if style.get_property_value("width").ok().as_deref() != Some(w.as_str())
        || style.get_property_value("height").ok().as_deref() != Some(h.as_str())
    {
        let _ = style.set_property("width", &w);
        let _ = style.set_property("height", &h);
    }
}

thread_local! {
    /// The testing aids' commands, run at the next redraw.
    static COMMANDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The state after the last recorded redraw (JSON).
    static STATE: RefCell<String> = const { RefCell::new(String::new()) };
    /// The rectangle `find` found (canvas pixels: left, top, width, height).
    static FOUND: RefCell<Option<[f64; 4]>> = const { RefCell::new(None) };
}

/// A testing aid for the end-to-end runs, run at the next redraw:
/// `find <name>` finds the rectangle of a layout of the front end's screen
/// (`newGameButton`, …), of the inventory menu (`mainMenuButton`, …), or
/// `hud` (the HUD's inventory button), or `slot <n>` (the inventory's
/// slot `n` on its first page), or `skip` (the movie's skip button), for
/// the state's `found`; `warp <path>` changes the node as `ChangeWarp`
/// does, `hit <name>` calls `OnWarpObjectHit(name)` as a click on that
/// object does; `times` clears the frame times.
#[wasm_bindgen]
pub fn atlantis4_test(command: String) {
    COMMANDS.with(|c| c.borrow_mut().push(command));
}

/// The state for the end-to-end runs (JSON): the front end's screen, the
/// game in play and its node, the movie, the dialog and document shown,
/// the inventory, the sound, the late sound buffers, the clock, the
/// canvas, the renderer and the check mode's last comparison, the backup
/// folder's files and their sizes, the frame times.
#[wasm_bindgen]
pub fn atlantis4_state() -> String {
    STATE.with(|s| s.borrow().clone())
}

impl Viewer {
    fn run_test_commands(&mut self) {
        let commands: Vec<String> = COMMANDS.with(|c| c.borrow_mut().drain(..).collect());
        if !commands.is_empty() {
            // Their result in the state at once.
            self.web.state_in = 0;
        }
        let (w, h) = self.size;
        for c in commands {
            let parts: Vec<&str> = c.split_whitespace().collect();
            let found = match parts.as_slice() {
                ["times"] => {
                    self.web.times.clear();
                    self.web.underruns0 = crate::audio_out::underruns();
                    continue;
                }
                ["find", "hud"] => self.game.borrow_mut().gui().hud_button(w, h),
                ["find", "skip"] => {
                    let mut g = self.game.borrow_mut();
                    let skip = g.skip_size();
                    g.movie.as_ref().map(|m| m.layout(w, h, skip).skip)
                }
                ["warp", path] => {
                    if let Some(s) = self.script.as_mut() {
                        if let Err(e) = s.change_warp(path.as_bytes()) {
                            log::warn!("test: {e}");
                        }
                    }
                    continue;
                }
                ["hit", name] => {
                    let arg = [Value::Str(name.as_bytes().to_vec())];
                    self.callback("OnWarpObjectHit", &arg);
                    continue;
                }
                ["slot", n] => n.parse::<usize>().ok().and_then(|n| {
                    self.game
                        .borrow_mut()
                        .gui()
                        .slot_rects(0, w, h)
                        .get(n)
                        .copied()
                }),
                ["find", name] => self
                    .front
                    .rect_of(name, w, h)
                    .filter(|_| self.front.active() || self.front.in_transition())
                    .or_else(|| self.game.borrow_mut().gui().menu_button(name, w, h)),
                _ => {
                    log::warn!("test: {c}?");
                    continue;
                }
            };
            if found.is_none() {
                log::warn!("test: nothing for {c}");
            }
            let found = found.map(|r| [r.left, r.top, r.width, r.height]);
            FOUND.with(|f| *f.borrow_mut() = found);
        }
    }

    fn record_state(&mut self) {
        let q = |s: &str| format!("\"{}\"", s.replace(['"', '\\'], ""));
        let kind = self.frame_kind();
        let in_game = self.script.is_some();
        let mut g = self.game.borrow_mut();
        let movie = g.movie.as_ref().map_or("null".into(), |m| q(&m.path));
        let document = g
            .documents
            .shown
            .as_ref()
            .map_or("null".into(), |d| q(&d.name));
        let warp = q(&String::from_utf8_lossy(g.warp_name()));
        let menu = in_game && g.gui().menu_shown;
        let objects: Vec<String> = (0..16)
            .filter_map(|i| g.inventory.object((0, i)).map(|o| q(&o.id)))
            .collect();
        let selected = g.inventory.selected.is_some();
        let dialog = g.gui_shown();
        let backup: Vec<String> = match &g.backup {
            BackupFolder::Memory(m) => m
                .iter()
                .map(|(k, v)| format!("{}:{}", q(k), v.len()))
                .collect(),
            BackupFolder::Dir(_) => Vec::new(),
        };
        let headless = g.sounds.headless;
        drop(g);
        let found = FOUND.with(|f| *f.borrow()).map_or("null".to_string(), |r| {
            format!("[{},{},{},{}]", r[0], r[1], r[2], r[3])
        });
        let check = self.last_diff.map_or("null".to_string(), |d| {
            format!(
                "{{\"diff\":{},\"within\":{},\"differ\":{},\"over\":{},\"max\":{},\"mean\":{:.4}}}",
                q(&d.to_string()),
                d.within(),
                d.differ,
                d.over,
                d.max,
                d.mean
            )
        });
        let s = format!(
            "{{\"screen\":\"{:?}\",\"front\":{},\"inGame\":{in_game},\"kind\":\"{kind}\",\
             \"warp\":{warp},\"movie\":{movie},\"document\":{document},\"dialog\":{dialog},\
             \"inventoryMenu\":{menu},\"objects\":[{}],\"selected\":{selected},\
             \"audio\":{},\"headless\":{headless},\"underruns\":{},\"underrunsSince\":{},\"clock\":{:.1},\
             \"musicVolume\":{},\"size\":[{},{}],\"by\":\"{}\",\"check\":{check},\"backup\":{{{}}},\
             \"found\":{found},\"times\":{}}}",
            self.front.screen,
            self.front.active() || self.front.in_transition(),
            objects.join(","),
            self.web.audio.is_some(),
            crate::audio_out::underruns(),
            crate::audio_out::underruns() - self.web.underruns0,
            self.now_ms(),
            self.front.settings.volume("music"),
            self.size.0,
            self.size.1,
            self.web.by,
            backup.join(","),
            self.web.times.report()
        );
        STATE.with(|st| *st.borrow_mut() = s);
    }
}

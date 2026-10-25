//! Atlantis II, as far as it goes: the player menu opens first (a symbol
//! continues that player's newest save or starts a new game, on Tibet's
//! first view, `tibet.wam` view 2); the player looks around with the mouse
//! or the arrow keys and walks from view to view with a left click on a move
//! zone (the turn, the transition video, the arrival), the place animations
//! playing. The chapter scripts run the game as the original's: the opening
//! film, the music, the dialogs they call (a left click goes to the next
//! line, Escape ends a dialog). The right button opens the bag bar at the
//! bottom of the screen (the mouse then moves a pointer over it; a click
//! puts an object in the hand, drawn at the screen centre). Escape opens
//! the main menu (options, volumes, load and save, quit) and closes it
//! again; in the menus the pointer is the game's own. A mode of its own,
//! beside the first game's front end: a window, the screen scaled to it,
//! the sound. A left click on a character opens the menu of the subjects
//! to ask him about (the mouse then moves a pointer over it). The saves (`save\ATLpp_ss.SAV`, the
//! original's format) and `atlant2.cfg` are kept in the `atlantis2` folder
//! of the port's data folder.
//!
//! The game folder is the installed one (`atlantis2.exe`, `wam\`,
//! `scenar\`...); the discs (CD images or copies of the CDs) are looked for
//! in it, in the folders directly inside it, and in the folders given with
//! `--discs`.
//!
//! `--original` turns the enhancements off, as for the first game: the
//! minor bug fixes (`atlantis2_engine::Enhancements::minor_bug_fixes`:
//! every music track heard to its end, a track that ends with nothing
//! after it played again, Ireland's puzzle zones answering for no named
//! zone) and the HD mode.
//!
//! `--hd` draws the panorama at the window's resolution (the first game's
//! HD mode, `atlantis2_engine::hd`), `--widescreen` too with the panorama
//! filling the sides of a window wider than 4:3; the rest (the cursor,
//! the bag bar, the menus, the videos) is the original's 640x480 art,
//! upscaled. `--size <w>x<h>` gives the window's size (in a headless run,
//! the size of the screenshot, composed as the window would show it).
//!
//! F10 (or O) opens the port's options menu
//! (`atlantis2_engine::options_menu`, the first game's): the graphics
//! (original or HD) and the HD options, the screen (the original's 640 x
//! 480 or 800 x 600, `atlantis2_engine::display`), the minor bug fixes;
//! the game is paused while it is open. Its choices are kept in
//! `settings.ini` beside the saves (the first game's keys); `--hd`,
//! `--widescreen` and `--original` hold for the run without changing it.
//! The screen size is the original's own option, kept in `atlant2.cfg` as
//! its options page's button keeps it; `--resolution 800x600` chooses it
//! from the command line. A frame at 800 x 600 is shown scaled to the
//! window as the 640 x 480 one is.
//!
//! Testing aids: `--view <id>[,<yaw>,<pitch>]` starts in another view of
//! the place (angles in degrees), `--dialog <n>` plays dialog `n` at once
//! over the view (a click goes to the next line, as in the original), with
//! its voices, `--menu <page>` shows a menu page; `--take
//! <object>[,<object>...]` puts objects in the bag and `--hold <object>`
//! one in the hand (by their `defnames.con` names, as the scripts'
//! `sacados` and `objetenmain` do), `--bag` opens the bag bar,
//! `--pointer <x>,<y>` puts the mouse pointer there and `--talk` clicks
//! the character under the screen centre (its subject menu opens);
//! `--command <commands>` runs script commands at the start, before the
//! view entry (`(monde=1)(changewam=bigile1.wam)(vue=5)`: Ireland).
//!
//! At start-up the intro videos play before the player menu, as in the
//! original (a click skips one, a double click all of them); `--no-intro`
//! skips them, `--intro` plays them in a headless run (which otherwise
//! starts a new game at once) or with `--menu player`.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use atlantis2_data::files::OpenError;
use atlantis2_data::save::SystemTime;
use atlantis2_data::A2Files;
use atlantis2_engine::hd::{picture_rect, side_tiles, tile_for_output, window_to_screen};
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::options_menu::{MenuAction, MenuValues, OptionsMenu};
use atlantis2_engine::settings::{Settings, SETTINGS_FILE};
use atlantis2_engine::{Engine, Enhancements, Fpu, LoopType, Variant};
use atlantis_engine::hd::{HdCompositor, HdFrame, HdText};
use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::sys::Key as MenuKey;
use web_time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

pub use atlantis2_data::files::holds_atlantis2;

/// The original's screen.
pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 480;

/// A turn key held: the mouse move it stands for each frame, in mickeys
/// (a port convenience; the original turns with the mouse only). With the
/// original's decay this turns about 55° a second at 60 frames a second.
const KEY_TURN: i32 = 8;

/// The frame period: the original's warp loop is unpaced, the port shows
/// one frame per 1/60 s.
const FRAME: Duration = Duration::from_micros(16_667);

/// Where a run starts besides the new game's first view (testing aids).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct A2Start {
    /// A view of the place and its angles in degrees.
    pub view: Option<(u32, f32, f32)>,
    /// A dialog played at once.
    pub dialog: Option<u32>,
    /// A menu page shown at once.
    pub menu: Option<PageKind>,
    /// Objects put in the bag, one put in the hand (`defnames.con`
    /// names).
    pub take: Vec<String>,
    pub hold: Option<String>,
    /// The bag bar opened, the pointer moved.
    pub bag: bool,
    pub pointer: Option<(i32, i32)>,
    /// A click at the start: on a character under the centre, its subject
    /// menu.
    pub talk: bool,
    /// The start-up intro videos: `Some(false)` skips them (`--no-intro`),
    /// `Some(true)` plays them where they would not be (`--intro`: a
    /// headless run or `--menu player`); `None` plays them when the game
    /// starts on the player menu as the original does.
    pub intro: Option<bool>,
    /// The window's size (`--size`): a headless run's screenshot is
    /// composed at that size.
    pub size: Option<(u32, u32)>,
    /// Script commands run at the start (`--command`), before the view
    /// entry: `(monde=1)(changewam=bigile1.wam)(vue=5)` goes to Ireland.
    pub commands: Vec<String>,
    /// The original's screen size (`--resolution`): its options page's
    /// choice, kept in `atlant2.cfg`.
    pub resolution: Option<(u32, u32)>,
    /// A headless run's screenshot shows the options menu (F10) over the
    /// last frame (`--options-menu`).
    pub options_menu: bool,
}

impl A2Start {
    /// Parses `--view`'s value: `<id>[,<yaw>,<pitch>]`.
    pub fn parse_view(v: &str) -> Option<(u32, f32, f32)> {
        let mut it = v.split(',').map(str::trim);
        let id = it.next()?.parse().ok()?;
        let yaw = it.next().map_or(Some(0.0), |s| s.parse().ok())?;
        let pitch = it.next().map_or(Some(0.0), |s| s.parse().ok())?;
        Some((id, yaw, pitch))
    }

    /// Parses `--pointer`'s value: `<x>,<y>`.
    pub fn parse_pointer(v: &str) -> Option<(i32, i32)> {
        let (x, y) = v.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    }

    /// Parses `--size`'s value: `<w>x<h>`.
    pub fn parse_size(v: &str) -> Option<(u32, u32)> {
        let (w, h) = v.split_once(['x', 'X'])?;
        let (w, h) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
        (w > 0 && h > 0).then_some((w, h))
    }

    /// Parses `--resolution`'s value: `640x480` or `800x600`.
    pub fn parse_resolution(v: &str) -> Option<(u32, u32)> {
        match Self::parse_size(v)? {
            s @ ((640, 480) | (800, 600)) => Some(s),
            _ => None,
        }
    }

    /// Parses `--menu`'s value.
    pub fn parse_menu(v: &str) -> Option<PageKind> {
        Some(match v.to_ascii_lowercase().as_str() {
            "player" => PageKind::Player,
            "main" => PageKind::Main,
            "options" => PageKind::Options,
            "sound" => PageKind::Sound,
            "load" => PageKind::Load,
            "credits" => PageKind::Credits,
            _ => return None,
        })
    }

    /// How the game starts: the player menu with the intro videos over it
    /// (the original's start-up), or a new game at once (headless runs and
    /// the testing aids), then the view, the dialog and the menu page asked
    /// for.
    pub fn begin(&self, engine: &mut Engine, headless: bool) -> Result<(), String> {
        if let Some((w, _)) = self.resolution {
            engine.set_screen_800(w == 800);
        }
        let at_once = self.view.is_some()
            || !self.commands.is_empty()
            || self.dialog.is_some()
            || !self.take.is_empty()
            || self.hold.is_some()
            || self.bag
            || self.pointer.is_some()
            || self.talk;
        let start_up = !(headless || at_once || self.menu.is_some());
        if self.menu == Some(PageKind::Player)
            || start_up
            || (self.intro == Some(true) && !at_once && self.menu.is_none())
        {
            // The player page is built, then the intro takes the screen
            // until its last video ends (FUN_0049d680's last two calls).
            engine.open_player_menu();
            if self.intro.unwrap_or(start_up) {
                engine.start_intro();
            }
            return Ok(());
        }
        engine.start_new_game().map_err(|e| e.to_string())?;
        if at_once || self.menu.is_some() {
            // The new game's music copy (`LOADMUSIK` in chapter 0's /INIT,
            // when the data folder holds no copy yet) holds the rest of
            // /INIT under the please-wait page: gone through first, so the
            // opening film and its music have started before the aids end
            // the film and apply theirs (`engine/media.rs`).
            while engine.copying() {
                engine.pass(None);
            }
        }
        self.apply(engine)?;
        match self.menu {
            Some(PageKind::Credits) => engine.open_credits(),
            Some(page) => engine.open_menu_page(page),
            None => {}
        }
        Ok(())
    }

    /// Enters the view, starts the dialog, fills the bag and the hand,
    /// opens the bar (the opening film ended first).
    pub fn apply(&self, engine: &mut Engine) -> Result<(), String> {
        let bag_aids = !self.take.is_empty() || self.hold.is_some() || self.bag || self.talk;
        if self.view.is_some() || self.dialog.is_some() || bag_aids || !self.commands.is_empty() {
            engine.end_video();
        }
        for c in &self.commands {
            // One command at a time, each from its `(`.
            for part in c
                .split_inclusive(')')
                .map(str::trim)
                .filter(|p| !p.is_empty())
            {
                engine.run_command(part.as_bytes());
            }
        }
        if let Some((id, yaw, pitch)) = self.view {
            engine
                .enter_view(id, yaw.to_radians(), pitch.to_radians())
                .map_err(|e| e.to_string())?;
        }
        if let Some(n) = self.dialog {
            engine.start_dialog(n).map_err(|e| e.to_string())?;
        }
        for name in &self.take {
            engine.run_command(format!("(sacados={name})").as_bytes());
        }
        if let Some(name) = &self.hold {
            engine.run_command(format!("(objetenmain={name})").as_bytes());
        }
        if self.bag {
            engine.right_click();
        }
        if let Some(p) = self.pointer {
            engine.pointer = p;
        }
        if self.talk {
            engine.click(engine.clock());
        }
        Ok(())
    }
}

/// The time of a save: the local time, as the original stamps it
/// (`GetLocalTime`).
#[cfg(windows)]
fn local_now() -> SystemTime {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: GetLocalTime only fills the structure it is given.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    SystemTime {
        year: t.wYear,
        month: t.wMonth,
        day_of_week: t.wDayOfWeek,
        day: t.wDay,
        hour: t.wHour,
        minute: t.wMinute,
        second: t.wSecond,
        milliseconds: t.wMilliseconds,
    }
}

/// The time of a save: the local time, as the original stamps it
/// (`GetLocalTime`): the browser's time zone.
#[cfg(target_arch = "wasm32")]
fn local_now() -> SystemTime {
    let d = js_sys::Date::new_0();
    SystemTime {
        year: d.get_full_year() as u16,
        month: d.get_month() as u16 + 1,
        day_of_week: d.get_day() as u16,
        day: d.get_date() as u16,
        hour: d.get_hours() as u16,
        minute: d.get_minutes() as u16,
        second: d.get_seconds() as u16,
        milliseconds: d.get_milliseconds() as u16,
    }
}

/// The time of a save: the local time, as the original stamps it
/// (`GetLocalTime`): the clock shifted by the time zone's offset.
#[cfg(not(any(windows, target_arch = "wasm32")))]
fn local_now() -> SystemTime {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let offset = {
        let t = secs as libc::time_t;
        // SAFETY: localtime_r only reads `t` and fills `tm`.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            0
        } else {
            // `c_long`: 32 bits on 32-bit targets.
            #[allow(clippy::useless_conversion)]
            i64::from(tm.tm_gmtoff)
        }
    };
    SystemTime::from_unix(secs + offset, d.subsec_millis() as u16)
}

/// The folder of the saves, `atlant2.cfg` and `settings.ini`.
pub fn data_dir() -> PathBuf {
    crate::desktop::data_dir().join("atlantis2")
}

/// Atlantis II's `settings.ini` (the port's options: the enhancements, the
/// graphics), written with the defaults when there is none, as the first
/// game's.
pub fn stored_settings() -> Settings {
    let path = data_dir().join(SETTINGS_FILE);
    match std::fs::read(&path) {
        Ok(b) => Settings::parse(&String::from_utf8_lossy(&b)),
        Err(_) => {
            let s = Settings::default();
            save_settings(&s);
            s
        }
    }
}

/// Writes `settings.ini`.
fn save_settings(s: &Settings) {
    let dir = data_dir();
    let path = dir.join(SETTINGS_FILE);
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, s.to_text())) {
        log::warn!("{}: {e}", path.display());
    }
}

/// Opens the game folder and starts the game as `start` says, the cube
/// drawn by `warpPII` (the reference renderer) for a process whose x87
/// control word starts as Win32's default.
/// Returns the engine and the game's release (for the options menu).
pub fn open(
    dir: &Path,
    discs: &[PathBuf],
    start: &A2Start,
    enhancements: Enhancements,
) -> Result<(Engine, String), String> {
    let (mut engine, label) = boot(dir, discs, enhancements)?;
    start.begin(&mut engine, false)?;
    Ok((engine, label))
}

/// The game folder opened and the game booted, with its saves' folder
/// and the enhancements asked for; the game's release.
fn boot(
    dir: &Path,
    discs: &[PathBuf],
    enhancements: Enhancements,
) -> Result<(Engine, String), String> {
    let (files, build) = A2Files::open_dir(dir, discs).map_err(|e| match e {
        OpenError::NoDisc => format!(
            "{}: {e}, or give their folder with --discs <folder>",
            dir.display()
        ),
        e => format!("{}: {e}", dir.display()),
    })?;
    log::info!(
        "{} ({}), discs {:?}",
        build.label,
        build.id,
        files.disc_numbers()
    );
    // Start-up: the script tables and chapter 0; the player menu or a new
    // game follows.
    let mut engine = Engine::boot(files, Variant::default(), Fpu::default(), WIDTH, HEIGHT)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    engine.enhancements = enhancements;
    engine.set_data_dir(Some(data_dir()));
    engine.wall_clock = local_now;
    // The original seeds `rand` with GetTickCount() (@ 0x0047d740).
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u32);
    engine.anim.rand = atlantis2_engine::anim::MsvcRand(seed);
    Ok((engine, format!("Game: {}", build.label)))
}

/// `--headless`: runs `frames` frames of 1/60 s without input (the 15 Hz
/// ticks run on the scheduler's clock), prints where the game is, and writes the
/// screen to `screenshot` (PNG) when given.
pub fn headless(
    dir: &Path,
    discs: &[PathBuf],
    frames: u64,
    screenshot: Option<&Path>,
    start: &A2Start,
    enhancements: Enhancements,
) -> Result<(), String> {
    let (mut engine, label) = boot(dir, discs, enhancements)?;
    start.begin(&mut engine, true)?;
    if start.menu.is_some() {
        // The pointer at the screen centre, or where `--pointer` puts it.
        let (x, y) = start
            .pointer
            .unwrap_or((WIDTH as i32 / 2, HEIGHT as i32 / 2));
        engine.pointer(x, y);
    }
    let mut screen = Vec::new();
    let mut sound = vec![0.0f32; 2 * 22050 / 60];
    // The window's picture, composed every frame when it is given a size
    // (the HD mode's timings).
    let (sw, sh) = start
        .size
        .map_or((WIDTH as usize, HEIGHT as usize), |(w, h)| {
            (w as usize, h as usize)
        });
    let mut picture = Picture::new();
    let mut window = vec![0u32; sw * sh];
    let compose = start.size.is_some();
    let mut times = Timings::default();
    for i in 0..frames {
        engine.set_clock(i as f64 / 60.0);
        picture.prepare(&mut engine, sw, sh);
        let (w, h) = engine.screen_size();
        screen.resize(w * h, 0);
        let t0 = Instant::now();
        engine.pass(Some(&mut screen));
        let pass = t0.elapsed();
        // The sound is mixed and dropped.
        engine.voices().fill(&mut sound, 22050, 2);
        if compose {
            let t1 = Instant::now();
            picture.compose(&engine, &screen, &mut window, sw, sh);
            times.add(&engine, pass, t1.elapsed());
        }
    }
    if frames == 0 {
        picture.prepare(&mut engine, sw, sh);
        let (w, h) = engine.screen_size();
        screen.resize(w * h, 0);
        engine.render(&mut screen);
    }
    let (scr_w, scr_h) = screen_dims(&screen);
    println!("screen {scr_w} x {scr_h}");
    let mut menu_text = HdText::default();
    if start.options_menu {
        // The options menu (F10) over the last frame, as the window shows
        // it.
        let mut menu = OptionsMenu::new();
        menu.set_info(menu_info(&label));
        menu.open();
        let values = menu_values(&engine);
        let font = SubtitleFont::new();
        menu.draw_recording(
            &mut screen,
            scr_w,
            scr_h,
            &font,
            values,
            Some(&mut menu_text),
        );
    }
    let (wam, view) = engine.place();
    let (yaw, pitch) = engine.angles_deg();
    println!("Atlantis II, {}", crate::version_string());
    println!("{wam} view {view}, yaw {yaw:.3}, pitch {pitch:.3}");
    match engine.centre_zone() {
        Some((i, z)) => println!(
            "zone {i} under the centre: type {}, value {}",
            z.kind, z.value
        ),
        None => println!("no zone under the centre"),
    }
    let distinct = screen
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    println!("{frames} frames, {distinct} colours on screen");
    if let Some(d) = engine.dialog() {
        if let Some(l) = d.current_line() {
            println!(
                "dialog {}, line {} ({}, stand {:?}, camera {:?}), video frame {}",
                d.number,
                l.id,
                l.speaker,
                l.stand.map(|s| s + 1),
                l.camera.map(|c| c + 1),
                d.stream().map_or(0, |s| s.number())
            );
        }
    }
    if let Some(s) = engine.script_state() {
        println!("chapter {}, {:?} loop", s.chapter, engine.loop_type);
    }
    if let Some((name, frames)) = engine.video_playing() {
        let intro = if engine.intro_playing() {
            " (intro)"
        } else {
            ""
        };
        println!("video {name}{intro}, {frames} frames shown");
    }
    if let Some(page) = engine.menu().and_then(|m| m.kind()) {
        println!("menu page {page:?}");
    }
    match engine.music_playing() {
        Some(m) => println!("music {m}, volume {}", engine.music.volume),
        None => println!("no music, volume {}", engine.music.volume),
    }
    if let Some(t) = times.report() {
        println!("{t}");
    }
    if let Some(path) = screenshot.filter(|_| compose) {
        picture.compose_with_text(&engine, &screen, &menu_text, &mut window, sw, sh);
        let png = atlantis2_engine::png::encode_xrgb8888(&window, sw, sh);
        std::fs::write(path, png).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("screenshot: {} ({sw}x{sh})", path.display());
    } else if let Some(path) = screenshot {
        let png = atlantis2_engine::png::encode_rgb565(&screen, scr_w, scr_h);
        std::fs::write(path, png).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("screenshot: {} ({scr_w}x{scr_h})", path.display());
    }
    Ok(())
}

/// The size of a screen buffer the engine drew (640 x 480 or 800 x 600,
/// `Engine::screen_size`).
fn screen_dims(screen: &[u16]) -> (usize, usize) {
    use atlantis2_engine::display::{LARGE_H, LARGE_W};
    if screen.len() == LARGE_W * LARGE_H {
        (LARGE_W, LARGE_H)
    } else {
        (WIDTH as usize, HEIGHT as usize)
    }
}

/// The options menu's foot: the game's release, the port.
fn menu_info(label: &str) -> Vec<String> {
    vec![label.to_string(), crate::version_string()]
}

/// What the options menu shows.
fn menu_values(engine: &Engine) -> MenuValues {
    MenuValues {
        enhancements: engine.enhancements,
        screen_800: engine.cfg().width == 800,
    }
}

/// Opens a window on the game; returns when it is closed. `label` names
/// the game's release in the options menu.
pub fn run_window(
    engine: Engine,
    label: String,
    fullscreen: bool,
    size: Option<(u32, u32)>,
) -> Result<(), String> {
    let el = EventLoop::new().map_err(|e| format!("No window can be opened: {e}"))?;
    let voices = engine.voices().clone();
    // The sound stops while the options menu is open (the game paused):
    // silence, the voices kept where they are.
    let paused = Arc::new(AtomicBool::new(false));
    let silent = paused.clone();
    let audio = crate::audio_out::start_with(move |out, rate, channels| {
        if silent.load(Ordering::Relaxed) {
            out.fill(0.0);
        } else {
            voices.fill(out, rate, channels);
        }
    });
    let (w, h) = engine.screen_size();
    let mut menu = OptionsMenu::new();
    menu.set_info(menu_info(&label));
    let mut app = A2App {
        _audio: audio,
        started: Instant::now(),
        engine,
        fullscreen,
        size,
        window: None,
        surface: None,
        picture: Picture::new(),
        screen: vec![0u16; w * h],
        shown: Vec::new(),
        menu_text: HdText::default(),
        menu,
        font: SubtitleFont::new(),
        paused,
        paused_at: None,
        menu_pointer: None,
        game_pointer: None,
        motion: (0.0, 0.0),
        keys: [false; 4],
        locked: false,
        focused: true,
        next_frame: Instant::now(),
    };
    el.run_app(&mut app)
        .map_err(|e| format!("The window failed: {e}"))
}

struct A2App {
    /// The sound output (kept alive while the window is open).
    _audio: Option<cpal::Stream>,
    /// The game's clock starts with the window (moved on by the time the
    /// options menu was open).
    started: Instant,
    engine: Engine,
    fullscreen: bool,
    /// The window's first size (`--size`), else twice 640 x 480.
    size: Option<(u32, u32)>,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    /// The engine's screen (640 x 480 or 800 x 600).
    screen: Vec<u16>,
    /// The screen with the options menu drawn over it, the menu's text.
    shown: Vec<u16>,
    menu_text: HdText,
    /// The window's picture of the frame (the HD mode's composition).
    picture: Picture,
    /// The port's options menu (F10, O), its font (the first game's).
    menu: OptionsMenu,
    font: SubtitleFont,
    /// The sound silenced, since when the game is paused (the menu open).
    paused: Arc<AtomicBool>,
    paused_at: Option<Instant>,
    /// The window's pointer on the screen (the options menu's clicks).
    menu_pointer: Option<(i32, i32)>,
    /// The window's pointer as the game's menus last had it (its moves
    /// turn the view: [`system_pointer`]); none outside the menus.
    game_pointer: Option<(i32, i32)>,
    /// Raw mouse motion not yet given to the engine (fractions kept).
    motion: (f64, f64),
    /// Left, right, up, down held.
    keys: [bool; 4],
    locked: bool,
    focused: bool,
    next_frame: Instant,
}

impl A2App {
    /// Hides and holds the pointer: the mouse turns the view, as in the
    /// original (which reads DirectInput's relative motion).
    fn grab(&mut self, on: bool) {
        let Some(w) = &self.window else { return };
        if on {
            let ok = w.set_cursor_grab(CursorGrabMode::Locked).is_ok()
                || w.set_cursor_grab(CursorGrabMode::Confined).is_ok();
            // Hidden only when grabbed: without a grab the mouse does not
            // turn the view, and the player needs to see the pointer.
            w.set_cursor_visible(!ok);
            self.locked = ok;
        } else {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
            w.set_cursor_visible(true);
            self.locked = false;
        }
    }

    fn step(&mut self) {
        // In the menus the pointer is the game's own, moved by the mouse
        // over the window; in the game the mouse turns the view.
        let in_menu = self.engine.loop_type == LoopType::Menu;
        if !in_menu {
            self.game_pointer = None;
        }
        if in_menu && self.locked {
            self.grab(false);
            if let Some(w) = &self.window {
                w.set_cursor_visible(false);
            }
        } else if !in_menu && !self.locked && self.focused {
            self.grab(true);
        }
        let (mx, my) = (self.motion.0.trunc(), self.motion.1.trunc());
        self.motion.0 -= mx;
        self.motion.1 -= my;
        let [left, right, up, down] = self.keys.map(i32::from);
        let dx = mx as i32 + (right - left) * KEY_TURN;
        let dy = my as i32 + (down - up) * KEY_TURN;
        if (dx != 0 || dy != 0) && !in_menu {
            self.engine.mouse_motion(dx, dy);
        }
        self.engine.set_clock(self.started.elapsed().as_secs_f64());
        if let Some(size) = self.window.as_ref().map(|w| w.inner_size()) {
            self.picture
                .prepare(&mut self.engine, size.width as usize, size.height as usize);
        }
        // The screen's size: 800 x 600 after the original's option.
        let (w, h) = self.engine.screen_size();
        self.screen.resize(w * h, 0);
        self.engine.pass(Some(&mut self.screen));
    }

    /// A window position on the screen the frame is laid out for (the
    /// logical size: 640 x 480, or 800 x 600 in the panorama of the
    /// 800 x 600 mode; the options menu's, the screen's): the pixel the
    /// window shows there (outside the picture, past its edges;
    /// [`window_point`] on the 640 x 480 screen).
    fn to_screen(&self, p: PhysicalPosition<f64>, (lw, lh): (usize, usize)) -> Option<(i32, i32)> {
        let size = self.window.as_ref()?.inner_size();
        let (ww, wh) = (size.width as usize, size.height as usize);
        if (lw, lh) == (WIDTH as usize, HEIGHT as usize) {
            return window_point(ww, wh, p);
        }
        let rect = picture_rect(ww, wh);
        if rect.2 == 0 || rect.3 == 0 {
            return None;
        }
        // The same place on a larger screen.
        let x = p.x.floor() as i64;
        let y = p.y.floor() as i64;
        let (ox, oy, dw, dh) = rect;
        let map = |v: i64, o: usize, d: usize, n: usize| -> i32 {
            ((v - o as i64) * n as i64).div_euclid(d as i64) as i32
        };
        Some((map(x, ox, dw, lw), map(y, oy, dh, lh)))
    }

    /// The logical size: what the engine's pointer is in.
    fn logical(&self) -> (usize, usize) {
        (self.engine.width(), self.engine.height())
    }

    /// Opens the options menu: the game pauses (its clock and its sound),
    /// the system pointer is free.
    fn open_menu(&mut self) {
        if self.menu.is_open() {
            return;
        }
        self.menu.open();
        self.paused_at = Some(Instant::now());
        self.paused.store(true, Ordering::Relaxed);
        self.grab(false);
        self.keys = [false; 4];
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// The menu closed: the game goes on where it was.
    fn close_menu(&mut self) {
        self.menu.close();
        if let Some(t) = self.paused_at.take() {
            let now = Instant::now();
            self.started += now.duration_since(t);
            self.next_frame = now;
        }
        self.paused.store(false, Ordering::Relaxed);
        self.motion = (0.0, 0.0);
        if self.engine.loop_type == LoopType::Menu {
            // The game's pointer is drawn by its menus.
            if let Some(w) = &self.window {
                w.set_cursor_visible(false);
            }
        } else if self.focused {
            self.grab(true);
        }
    }

    fn menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::None => {}
            MenuAction::Close => self.close_menu(),
            MenuAction::SetEnhancements(x) => {
                // In force at once: the HD mode at the next frame, the
                // music's from the next track.
                self.engine.enhancements = x;
                log::info!("enhancements: {x:?}");
                let mut s = stored_settings();
                s.enhancements = x;
                save_settings(&s);
            }
            MenuAction::SetScreen800(on) => {
                // The original's option: the screen changes at the next
                // pass; atlant2.cfg written now (the original writes it at
                // exit, FUN_0047f530).
                self.engine.set_screen_800(on);
                log::info!("screen: {}", if on { "800 x 600" } else { "640 x 480" });
                self.engine.write_cfg();
            }
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// The frame in the window: the screen scaled to it (nearest pixel,
    /// kept in proportion, black bars), or in the HD mode the composition
    /// of the HD view and the upscaled screen; the options menu over it.
    fn present(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.inner_size();
        let (Some(nw), Some(nh)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        if surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };
        let (ww, wh) = (size.width as usize, size.height as usize);
        let screen = if self.menu.is_open() {
            let (sw, sh) = screen_dims(&self.screen);
            self.shown.clone_from(&self.screen);
            let values = menu_values(&self.engine);
            // Its text drawn sharp in the HD mode, as the first game's.
            self.menu.draw_recording(
                &mut self.shown,
                sw,
                sh,
                &self.font,
                values,
                Some(&mut self.menu_text),
            );
            &self.shown
        } else {
            self.menu_text.items.clear();
            &self.screen
        };
        self.picture
            .compose_with_text(&self.engine, screen, &self.menu_text, &mut buf, ww, wh);
        let _ = buf.present();
    }
}

/// The options menu's key for a window key.
fn menu_key(key: &Key) -> Option<MenuKey> {
    Some(match key {
        Key::Named(NamedKey::Escape) => MenuKey::Escape,
        Key::Named(NamedKey::Enter) => MenuKey::Enter,
        Key::Named(NamedKey::Space) => MenuKey::Space,
        Key::Named(NamedKey::ArrowLeft) => MenuKey::Left,
        Key::Named(NamedKey::ArrowRight) => MenuKey::Right,
        Key::Named(NamedKey::ArrowUp) => MenuKey::Up,
        Key::Named(NamedKey::ArrowDown) => MenuKey::Down,
        Key::Named(NamedKey::F10) => MenuKey::F(10),
        Key::Character(c) => MenuKey::Char(c.chars().next()?),
        _ => return None,
    })
}

impl ApplicationHandler for A2App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes().with_title("Atlantis II - Atlantis Native");
        attrs = match self.size {
            Some((w, h)) => attrs.with_inner_size(PhysicalSize::new(w, h)),
            None => attrs.with_inner_size(LogicalSize::new(WIDTH * 2, HEIGHT * 2)),
        };
        if self.fullscreen {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                log::error!("No window can be opened: {e}");
                el.exit();
                return;
            }
        };
        let context = softbuffer::Context::new(window.clone()).unwrap();
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
        self.window = Some(window);
        self.grab(true);
        let (w, h) = self.engine.screen_size();
        self.screen.resize(w * h, 0);
        self.engine.render(&mut self.screen);
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if Instant::now() >= self.next_frame {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.locked && !self.menu.is_open() {
                self.motion.0 += dx;
                self.motion.1 += dy;
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let in_menu = self.engine.loop_type == LoopType::Menu;
        // The options menu open: it takes the keys and the mouse.
        if self.menu.is_open() {
            let dims = screen_dims(&self.screen);
            match &event {
                WindowEvent::CursorMoved { position, .. } => {
                    if let Some((x, y)) = self.to_screen(*position, dims) {
                        self.menu_pointer = Some((x, y));
                        self.menu.pointer(x, y, dims.0, dims.1);
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    return;
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some((x, y)) = self.menu_pointer {
                        let values = menu_values(&self.engine);
                        let action = self.menu.click(x, y, dims.0, dims.1, values);
                        self.menu_action(action);
                    }
                    return;
                }
                WindowEvent::MouseInput { .. } => return,
                WindowEvent::KeyboardInput { event: k, .. } => {
                    let full = matches!(k.logical_key, Key::Named(NamedKey::F11));
                    if !full {
                        if k.state == ElementState::Pressed {
                            if let Some(key) = menu_key(&k.logical_key) {
                                let values = menu_values(&self.engine);
                                let action = self.menu.key(key, values);
                                self.menu_action(action);
                            }
                        }
                        return;
                    }
                }
                _ => {}
            }
        }
        match event {
            WindowEvent::CloseRequested => {
                // `atlant2.cfg` is written at exit (FUN_0047f530).
                self.engine.write_cfg();
                el.exit();
            }
            WindowEvent::Focused(on) => {
                self.focused = on;
                if !on {
                    self.grab(false);
                    self.keys = [false; 4];
                }
            }
            // The window's pointer is the game's in the menus (and while
            // the mouse is not held); held, the raw motion moves it (the
            // bag bar's pointer).
            WindowEvent::CursorMoved { position, .. } if in_menu || !self.locked => {
                self.menu_pointer = self.to_screen(position, screen_dims(&self.screen));
                if let Some(p) = self.to_screen(position, self.logical()) {
                    let last = if in_menu {
                        Some(&mut self.game_pointer)
                    } else {
                        None
                    };
                    system_pointer(&mut self.engine, last, p);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.menu_pointer = self.to_screen(position, screen_dims(&self.screen));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let now = self.started.elapsed().as_secs_f64();
                let pressed = state == ElementState::Pressed;
                if mouse_button(&mut self.engine, self.locked, button, pressed, now) {
                    self.grab(true);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                match event.logical_key {
                    // Escape (@ 0x004762f0): the main menu from the game,
                    // the end of a dialog, the main menu closed.
                    Key::Named(NamedKey::Escape) => {
                        self.engine.escape_key(down);
                        if down && !event.repeat {
                            self.engine.escape();
                        }
                    }
                    Key::Named(NamedKey::ArrowLeft) => self.keys[0] = down,
                    Key::Named(NamedKey::ArrowRight) => self.keys[1] = down,
                    Key::Named(NamedKey::ArrowUp) => self.keys[2] = down,
                    Key::Named(NamedKey::ArrowDown) => self.keys[3] = down,
                    Key::Named(NamedKey::F11) if down => {
                        if let Some(w) = &self.window {
                            let full = w.fullscreen().is_some();
                            w.set_fullscreen((!full).then_some(Fullscreen::Borderless(None)));
                        }
                    }
                    // The port's options menu (F10, or O for keyboards
                    // without function keys): not the game's keys.
                    Key::Named(NamedKey::F10) if down && !event.repeat => self.open_menu(),
                    Key::Character(ref c)
                        if down && !event.repeat && c.eq_ignore_ascii_case("o") =>
                    {
                        self.open_menu()
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now >= self.next_frame && !self.menu.is_open() {
                    self.step();
                    // `EXIT` in a script, or the menus, quit the game.
                    if self.engine.quit() {
                        self.engine.write_cfg();
                        el.exit();
                    }
                    self.next_frame += FRAME;
                    if self.next_frame < now {
                        self.next_frame = now + FRAME;
                    }
                }
                self.present();
            }
            _ => {}
        }
    }
}

/// A mouse button for the engine (the input callbacks FUN_0040c510 and
/// FUN_0040c660): every release of the left button clears the held flag
/// (@ 0x006569ca), whatever the loop type; a press clicks (the left
/// button: the click flag @ 0x006569c9, a video ended) or opens the bag
/// (the right one). True when the press only grabs the pointer (outside
/// the menus, with the pointer free; a left click still ends a video).
fn mouse_button(
    engine: &mut Engine,
    locked: bool,
    button: MouseButton,
    pressed: bool,
    now: f64,
) -> bool {
    if !pressed {
        if button == MouseButton::Left {
            engine.release();
        }
        return false;
    }
    let in_menu = engine.loop_type == LoopType::Menu;
    let in_video = engine.loop_type == LoopType::Video;
    if !locked && !in_menu && !(in_video && button == MouseButton::Left) {
        return true;
    }
    match button {
        MouseButton::Left => engine.click(now),
        MouseButton::Right => engine.right_click(),
        _ => {}
    }
    false
}

/// The window's pointer at `p` on the screen where the pointer is the
/// system's: the game's pointer there; in the menus (`last` given, the
/// position it had) its moves also add to the turn speeds, as the
/// original's motion callback (@ 0x0040c420) adds the mouse's moves to
/// them in every loop type (the port adds the pointer's moves in screen
/// pixels, the original the mouse's counts, which moved its pointer one
/// pixel each).
fn system_pointer(engine: &mut Engine, last: Option<&mut Option<(i32, i32)>>, p: (i32, i32)) {
    if let Some(last) = last {
        if let Some((x, y)) = *last {
            engine.pointer_moved(p.0 - x, p.1 - y);
        }
        *last = Some(p);
    }
    engine.pointer(p.0, p.1);
}

/// A window position on the 640x480 screen, in a `width` x `height`
/// window: the screen pixel the window shows under it (outside the
/// picture, past its edges), HD mode or not (`window_to_screen`, the
/// nearest map both pictures are drawn with: pixel x of the picture shows
/// screen column ⌊x · 640 / dw⌋). Before the HD mode a position was
/// divided by the float scale from an offset that could fall on half a
/// pixel, which put some clicks one pixel off the pixel shown; see the
/// test `clicks_hit_the_pixel_drawn`.
fn window_point(width: usize, height: usize, p: PhysicalPosition<f64>) -> Option<(i32, i32)> {
    let rect = picture_rect(width, height);
    if rect.2 == 0 || rect.3 == 0 {
        return None;
    }
    Some(window_to_screen(
        p.x.floor() as i64,
        p.y.floor() as i64,
        rect,
    ))
}

/// The window's picture of the engine's frames: the 640x480 screen
/// scaled, or in the HD mode (`Enhancements::hd`) the first game's
/// composition (`atlantis_engine::hd::HdCompositor`): the engine's HD view
/// where the frame shows the cube, the rest of the screen upscaled (xBR
/// for the art, bilinearly for the videos' frames), widescreen sides.
pub struct Picture {
    compositor: HdCompositor,
}

impl Default for Picture {
    fn default() -> Self {
        Picture::new()
    }
}

impl Picture {
    pub fn new() -> Picture {
        Picture {
            compositor: HdCompositor::new(),
        }
    }

    /// Whether the window shows the HD composition.
    pub fn hd(&self, engine: &Engine) -> bool {
        engine.enhancements.hd
    }

    /// Before a pass: the HD size the engine draws its view at, for a
    /// `width` x `height` window (none when the picture is not larger than
    /// 640 x 480), with the widescreen sides that fill the bars.
    pub fn prepare(&mut self, engine: &mut Engine, width: usize, height: usize) {
        if !engine.enhancements.hd {
            engine.set_hd_output(None, 0);
            return;
        }
        let tile = tile_for_output(width, height, atlantis2_engine::hd::MAX_TILE);
        let (ox, _, dw, _) = picture_rect(width, height);
        let side = if engine.enhancements.hd_options.widescreen {
            side_tiles(ox, dw)
        } else {
            0
        };
        engine.set_hd_output(tile, side);
    }

    /// The frame `screen` (640 x 480, or 800 x 600 outside the HD mode)
    /// in `out` (`width` x `height`, XRGB8888).
    pub fn compose(
        &mut self,
        engine: &Engine,
        screen: &[u16],
        out: &mut [u32],
        width: usize,
        height: usize,
    ) {
        self.compose_with_text(engine, screen, &HdText::default(), out, width, height);
    }

    /// [`Picture::compose`] with `text` drawn sharp in the HD mode (the
    /// options menu's).
    pub fn compose_with_text(
        &mut self,
        engine: &Engine,
        screen: &[u16],
        text: &HdText,
        out: &mut [u32],
        width: usize,
        height: usize,
    ) {
        let (ox, oy, dw, dh) = picture_rect(width, height);
        out[..width * height].fill(0);
        // (A frame of 800 x 600 left from before the HD mode was turned
        // on is scaled as without it.)
        if engine.enhancements.hd && screen.len() == (WIDTH * HEIGHT) as usize {
            // The frame's texts: their 640x480 glyphs uncovered by the
            // compositor, then drawn again at this size (sharp
            // subtitles; Atlantis II's font is drawn by the engine); the
            // options menu's last, over them.
            let menu = !text.items.is_empty();
            let mut held: Vec<&HdText> = engine.hd_texts().iter().map(|t| &t.text).collect();
            if menu {
                held.push(text);
            }
            let f = HdFrame {
                screen,
                layer: engine.hd_layer(),
                texts: &held,
                photo: engine.hd_photo(),
                video_pano: None,
            };
            let opts = engine.enhancements.hd_options;
            // The texts the compositor uncovers are drawn again: with
            // sharp subtitles, and over a video zoomed to the sides
            // whatever that option (the first game's compositor draws its
            // own texts again then).
            let redraw = HdCompositor::uncovers_texts(&f, opts, ox);
            let rect = (ox, oy, dw, dh);
            if redraw && menu {
                // The game's texts under the options menu's.
                self.compositor
                    .compose_with(f, opts, out, width, rect, false);
                engine.draw_hd_texts(out, width, rect);
                let menu_only = [text];
                let f = HdFrame {
                    texts: &menu_only,
                    ..f
                };
                self.compositor.draw_texts(&f, out, width, rect);
                return;
            }
            self.compositor.compose(f, opts, out, width, ox, oy, dw, dh);
            if redraw {
                engine.draw_hd_texts(out, width, rect);
            }
            return;
        }
        Self::scale(screen, out, width, height);
    }

    /// The original graphics: the screen (640 x 480 or 800 x 600) scaled
    /// to the 4:3 picture of a `width` x `height` window (nearest pixel,
    /// black bars), its colours [`atlantis2_engine::png::rgb565_to_rgb`]'s
    /// (as before the HD mode).
    fn scale(screen: &[u16], out: &mut [u32], width: usize, height: usize) {
        let (ox, oy, dw, dh) = picture_rect(width, height);
        let lut = atlantis2_engine::png::rgb565_to_xrgb();
        let (w, h) = screen_dims(screen);
        let cols: Vec<usize> = (0..dw).map(|x| x * w / dw).collect();
        for y in 0..dh {
            let src = &screen[(y * h / dh) * w..][..w];
            let row = &mut out[(oy + y) * width + ox..][..dw];
            for (d, &sx) in row.iter_mut().zip(&cols) {
                *d = lut[usize::from(src[sx])];
            }
        }
    }
}

/// The frame times of a headless run in the HD mode.
#[derive(Default)]
struct Timings {
    /// Frames with an HD view: the pass (the view included), the view,
    /// the composition, in seconds; the slowest frame.
    n: u32,
    pass: f64,
    view: f64,
    compose: f64,
    worst: f64,
    /// Frames without one (videos, menus, the original graphics): their
    /// pass and their composition.
    other: u32,
    other_pass: f64,
    other_compose: f64,
}

impl Timings {
    fn add(&mut self, engine: &Engine, pass: Duration, compose: Duration) {
        let (p, c) = (pass.as_secs_f64(), compose.as_secs_f64());
        if engine.hd_layer().is_some() {
            self.n += 1;
            self.pass += p;
            self.view += engine.hd_view_ms() / 1e3;
            self.compose += c;
            self.worst = self.worst.max(p + c);
        } else {
            self.other += 1;
            self.other_pass += p;
            self.other_compose += c;
        }
    }

    fn report(&self) -> Option<String> {
        if self.n == 0 && self.other == 0 {
            return None;
        }
        let ms = |t: f64, n: u32| t * 1e3 / f64::from(n.max(1));
        Some(format!(
            "HD: {} frames with the view: pass {:.2} ms (view {:.2} ms), composition {:.2} ms, \
             slowest {:.2} ms; {} other frames: pass {:.2} ms, composition {:.2} ms",
            self.n,
            ms(self.pass, self.n),
            ms(self.view, self.n),
            ms(self.compose, self.n),
            self.worst * 1e3,
            self.other,
            ms(self.other_pass, self.other),
            ms(self.other_compose, self.other)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game's files, or `None` (the test skipped) without
    /// `ATLANTIS2_DATA` / `ATLANTIS2_DISCS`.
    fn a2_files() -> Option<atlantis2_data::A2Files> {
        let (Some(dir), Some(discs)) = (
            std::env::var_os("ATLANTIS2_DATA"),
            std::env::var_os("ATLANTIS2_DISCS"),
        ) else {
            eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
            return None;
        };
        let (files, _) = atlantis2_data::A2Files::open_dir(
            std::path::Path::new(&dir),
            &[std::path::PathBuf::from(discs)],
        )
        .ok()?;
        Some(files)
    }

    /// The testing aids' start on a data folder with no music copy yet:
    /// the please-wait page of chapter 0's `LOADMUSIK` goes first, then
    /// the rest of /INIT (the opening film and music 30), then the aids.
    /// The view asked for is shown with the music, no film over it.
    #[test]
    fn the_aids_start_after_the_music_copy() {
        let Some(files) = a2_files() else { return };
        let dir = std::env::temp_dir().join(format!("a2-aids-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        e.set_data_dir(Some(dir.clone()));
        let start = A2Start {
            view: Some((10, 0.0, 0.0)),
            ..A2Start::default()
        };
        start.begin(&mut e, true).unwrap();
        assert!(!e.copying());
        for i in 0..120 {
            e.set_clock(f64::from(i) / 60.0);
            e.pass(None);
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(e.loop_type, LoopType::Warp);
        assert!(e.video_playing().is_none());
        assert_eq!(e.place().1, 10);
        assert_eq!(e.music_playing(), Some(30));
    }

    /// Every release of the left button reaches the engine, also outside
    /// the menus (FUN_0040c660 clears the held flag in every loop type):
    /// a press on a player symbol closes the player menu, and its release
    /// comes in the game.
    #[test]
    fn every_left_release_reaches_the_engine() {
        let Some(files) = a2_files() else { return };
        let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        e.open_player_menu();
        let mut clock = 0.0;
        let mut pass = |e: &mut Engine| {
            clock += 1.0 / 60.0;
            e.set_clock(clock);
            e.pass(None);
        };
        pass(&mut e);
        let w = e
            .menu()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .widget("b_gala.spr")
            .unwrap();
        e.pointer(
            (w.hit.left + w.hit.right) / 2,
            (w.hit.top + w.hit.bottom) / 2,
        );
        // In the menus a press clicks, whether the pointer is held or not.
        assert!(!mouse_button(&mut e, false, MouseButton::Left, true, 1.0));
        assert!(e.button_held());
        pass(&mut e);
        pass(&mut e);
        while e.copying() {
            pass(&mut e);
        }
        assert_ne!(e.loop_type, LoopType::Menu);
        assert!(!mouse_button(&mut e, true, MouseButton::Left, false, 1.1));
        assert!(!e.button_held());
        // Outside the menus, with the pointer free, a press grabs it.
        assert_eq!(e.loop_type, LoopType::Video);
        e.end_video();
        pass(&mut e);
        assert!(mouse_button(&mut e, false, MouseButton::Left, true, 2.0));
        assert!(!e.button_held());
    }

    /// In the menus the system pointer's moves add to the turn speeds
    /// (the motion callback @ 0x0040c420 runs in every loop type); outside
    /// them, with the pointer free, it only places the game's pointer.
    #[test]
    fn the_menus_pointer_turns_the_view() {
        let Some(files) = a2_files() else { return };
        let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
        let mut last = None;
        system_pointer(&mut e, None, (100, 100));
        assert_eq!((e.look.yaw_speed, e.look.pitch_speed), (0.0, 0.0));
        e.escape();
        assert_eq!(e.loop_type, LoopType::Menu);
        system_pointer(&mut e, Some(&mut last), (100, 100));
        assert_eq!((e.look.yaw_speed, e.look.pitch_speed), (0.0, 0.0));
        system_pointer(&mut e, Some(&mut last), (140, 90));
        let mut want = atlantis2_engine::look::Look::default();
        want.mouse(e.fpu(), 40, -10);
        assert_eq!(
            (e.look.yaw_speed, e.look.pitch_speed),
            (want.yaw_speed, want.pitch_speed)
        );
        assert_eq!(last, Some((140, 90)));
    }

    #[test]
    fn sizes() {
        assert_eq!(A2Start::parse_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(A2Start::parse_size("1440X1080"), Some((1440, 1080)));
        assert_eq!(A2Start::parse_size("0x1080"), None);
        assert_eq!(A2Start::parse_size("1920"), None);
        assert_eq!(A2Start::parse_resolution("800x600"), Some((800, 600)));
        assert_eq!(A2Start::parse_resolution("640x480"), Some((640, 480)));
        assert_eq!(A2Start::parse_resolution("1024x768"), None);
    }

    #[test]
    fn screens_and_window_points() {
        assert_eq!(screen_dims(&vec![0; 800 * 600]), (800, 600));
        assert_eq!(screen_dims(&vec![0; 640 * 480]), (640, 480));
        // The non-HD picture of an 800 x 600 screen fills the 4:3 window.
        let screen: Vec<u16> = (0..800 * 600).map(|i| (i % 800) as u16).collect();
        let mut out = vec![0u32; 1600 * 1200];
        let lut = atlantis2_engine::png::rgb565_to_xrgb();
        Picture::scale(&screen, &mut out, 1600, 1200);
        assert_eq!(out[0], lut[0]);
        assert_eq!(out[1599], lut[799]);
        assert_eq!(out[1200 * 1600 - 2], lut[799]);
    }

    /// Without the HD mode the window is what it was before it: every
    /// RGB565 value through `png::rgb565_to_rgb`, the picture scaled with
    /// the same nearest map (the window's code before the HD mode).
    #[test]
    fn original_graphics_window_unchanged() {
        for p in 0..=u16::MAX {
            let [r, g, b] = atlantis2_engine::png::rgb565_to_rgb(p);
            let want = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
            assert_eq!(
                atlantis2_engine::png::rgb565_to_xrgb()[usize::from(p)],
                want
            );
        }
        let screen: Vec<u16> = (0..640 * 480u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 16) as u16)
            .collect();
        for (ww, wh) in [(1280, 960), (1920, 1080), (1000, 700), (641, 2000)] {
            let mut out = vec![0u32; ww * wh];
            Picture::scale(&screen, &mut out, ww, wh);
            // The former present().
            let scale = (ww as f64 / 640.0).min(wh as f64 / 480.0);
            let (dw, dh) = ((640.0 * scale) as usize, (480.0 * scale) as usize);
            let (ox, oy) = ((ww - dw) / 2, (wh - dh) / 2);
            let mut want = vec![0u32; ww * wh];
            for y in 0..dh {
                let sy = (y * 480 / dh.max(1)).min(479);
                for x in 0..dw {
                    let sx = (x * 640 / dw.max(1)).min(639);
                    let [r, g, b] = atlantis2_engine::png::rgb565_to_rgb(screen[sy * 640 + sx]);
                    want[(oy + y) * ww + ox + x] =
                        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
                }
            }
            assert!(out == want, "{ww} x {wh}");
        }
    }

    /// The compositor puts back what was under the 640x480 glyphs exactly
    /// when `HdCompositor::uncovers_texts` says so, the case where
    /// `Picture::compose` draws Atlantis II's texts again (they have no
    /// `items` for the compositor to draw): with every option and frame
    /// kind, a glyph either stays in the picture or is redrawn.
    #[test]
    fn texts_uncovered_are_drawn_again() {
        use atlantis_engine::hd::{HdOptions, HdText, VideoSides};
        let (ww, wh) = (1920, 1080);
        let (ox, oy, dw, dh) = picture_rect(ww, wh);
        let (w, h) = (WIDTH as usize, HEIGHT as usize);
        let under = vec![0x001fu16; w * h];
        let mut after = under.clone();
        for y in 200..260 {
            for x in 300..340 {
                after[y * w + x] = 0xf800;
            }
        }
        let mut text = HdText::default();
        text.set(Vec::new(), &under, &after);
        let held = [&text];
        let mut out = vec![0u32; ww * wh];
        let mut c = HdCompositor::default();
        let mut redrawn = 0;
        for sides in [
            VideoSides::Black,
            VideoSides::Blur,
            VideoSides::Zoom,
            VideoSides::Panorama,
        ] {
            for sharp in [false, true] {
                for photo in [false, true] {
                    let opts = HdOptions {
                        sharp_subtitles: sharp,
                        widescreen: true,
                        video_sides: sides,
                        ..HdOptions::default()
                    };
                    let f = HdFrame {
                        screen: &after,
                        layer: None,
                        texts: &held,
                        photo,
                        video_pano: None,
                    };
                    let uncovers = HdCompositor::uncovers_texts(&f, opts, ox);
                    c.compose(f, opts, &mut out, ww, ox, oy, dw, dh);
                    let red = out.iter().any(|&p| (p >> 16) & 0xff > 0x40);
                    assert_eq!(red, !uncovers, "{sides:?} sharp {sharp} photo {photo}");
                    redrawn += usize::from(uncovers && !sharp);
                }
            }
        }
        // A video zoomed to the sides, without sharp subtitles.
        assert_eq!(redrawn, 1);
    }

    /// With the HD mode off too, a click names the 640x480 pixel the
    /// window draws under it (`present`: picture pixel x shows screen
    /// column x · 640 / dw). The mapping before the HD mode (a division by
    /// the float scale, from an offset that could be half a pixel) is at
    /// most one pixel away from it, and differs only where it missed the
    /// pixel drawn.
    #[test]
    fn clicks_hit_the_pixel_drawn() {
        let old = |ww: usize, wh: usize, x: f64, y: f64| {
            let (ww, wh) = (ww as f64, wh as f64);
            let scale = (ww / 640.0).min(wh / 480.0);
            let (dw, dh) = (640.0 * scale, 480.0 * scale);
            let (ox, oy) = ((ww - dw).trunc() / 2.0, (wh - dh).trunc() / 2.0);
            (((x - ox) / scale) as i32, ((y - oy) / scale) as i32)
        };
        let mut changed = 0;
        for (ww, wh) in [
            (1280, 960),
            (1920, 1080),
            (1366, 768),
            (1023, 767),
            (800, 1000),
            (1281, 961),
            (2561, 1441),
        ] {
            let (ox, oy, dw, dh) = picture_rect(ww, wh);
            for y in oy..oy + dh {
                for x in ox..ox + dw {
                    let drawn = (
                        ((x - ox) * WIDTH as usize / dw) as i32,
                        ((y - oy) * HEIGHT as usize / dh) as i32,
                    );
                    let p = PhysicalPosition::new(x as f64 + 0.25, y as f64 + 0.75);
                    assert_eq!(window_point(ww, wh, p), Some(drawn), "{ww}x{wh} ({x}, {y})");
                    let o = old(ww, wh, x as f64, y as f64);
                    assert!(
                        (o.0 - drawn.0).abs() <= 1 && (o.1 - drawn.1).abs() <= 1,
                        "{ww}x{wh} ({x}, {y}): {o:?} {drawn:?}"
                    );
                    changed += usize::from(o != drawn);
                }
            }
        }
        // Some window pixels did name another pixel than the one drawn.
        assert!(changed > 0);
        eprintln!("{changed} window pixels clicked one pixel off before");
    }

    /// With the game (`ATLANTIS2_DATA`, `ATLANTIS2_DISCS`; skipped
    /// without): a short stretch played as the window plays it, at once in
    /// the HD mode with the widescreen sides in a 1920 x 1080 window and
    /// with the original graphics: Tibet's first view and the shaman's
    /// dialog (360) clicked through, with the options menu (F10) over one
    /// of its lines; Ireland's and China's first views turned around; the
    /// game's main menu with a button's tooltip. Every pass gives the same
    /// 640 x 480 frame and the game the same script events either way;
    /// the HD picture is composed (its view drawn, the sides filled by the
    /// panorama) where the original graphics leave black bars.
    #[test]
    fn hd_widescreen_plays_as_the_original() {
        use atlantis2_engine::Event;
        use atlantis_engine::hd::HdOptions;
        let (Some(dir), Some(discs)) = (
            std::env::var_os("ATLANTIS2_DATA"),
            std::env::var_os("ATLANTIS2_DISCS"),
        ) else {
            eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
            return;
        };
        let (dir, discs) = (PathBuf::from(dir), [PathBuf::from(discs)]);
        let (ww, wh) = (1920, 1080);
        let (ox, _, _, _) = picture_rect(ww, wh);
        let open = |hd: bool| {
            let x = Enhancements {
                hd,
                hd_options: HdOptions {
                    widescreen: true,
                    ..HdOptions::default()
                },
                ..Enhancements::default()
            };
            let (mut e, _) = boot(&dir, &discs, x).expect("game");
            e.anim.rand = atlantis2_engine::anim::MsvcRand(1);
            e.trace = true;
            e
        };
        struct Side {
            engine: Engine,
            picture: Picture,
            screen: Vec<u16>,
            window: Vec<u32>,
        }
        let mut sides = [false, true].map(|hd| Side {
            engine: open(hd),
            picture: Picture::new(),
            screen: vec![0u16; (WIDTH * HEIGHT) as usize],
            window: vec![0u32; ww * wh],
        });
        let mut frame = 0usize;
        let (mut composed, mut layers, mut filled) = (0, 0, 0);
        // `n` frames of 1/60 s as the window runs them, `input` given to
        // both engines before each; every 8th frame (and the last)
        // composed in the window, with the options menu over it when
        // `options` is given.
        let mut play = |sides: &mut [Side; 2],
                        n: usize,
                        input: &mut dyn FnMut(&mut Engine, usize, f64),
                        options: Option<&OptionsMenu>| {
            for k in 0..n {
                for s in sides.iter_mut() {
                    let t = frame as f64 / 60.0;
                    input(&mut s.engine, k, t);
                    s.engine.set_clock(t);
                    s.picture.prepare(&mut s.engine, ww, wh);
                    s.engine.pass(Some(&mut s.screen));
                }
                let [off, on] = &mut *sides;
                assert!(
                    off.screen == on.screen,
                    "frame {frame}: the HD mode's frame differs"
                );
                assert_eq!(
                    off.engine.events.len(),
                    on.engine.events.len(),
                    "frame {frame}"
                );
                assert!(off.engine.hd_layer().is_none());
                if on.engine.loop_type == LoopType::Warp && !on.engine.turning() {
                    layers += usize::from(on.engine.hd_layer().is_some());
                }
                if k % 8 == 7 || k + 1 == n {
                    for (hd, s) in sides.iter_mut().enumerate() {
                        let mut text = HdText::default();
                        let mut shown = s.screen.clone();
                        if let Some(menu) = options {
                            let values = menu_values(&s.engine);
                            let font = SubtitleFont::new();
                            menu.draw_recording(
                                &mut shown,
                                WIDTH as usize,
                                HEIGHT as usize,
                                &font,
                                values,
                                Some(&mut text),
                            );
                        }
                        s.picture.compose_with_text(
                            &s.engine,
                            &shown,
                            &text,
                            &mut s.window,
                            ww,
                            wh,
                        );
                        // The left bar: the panorama in widescreen HD,
                        // black with the original graphics.
                        let bar = (0..wh).any(|y| s.window[y * ww..][..ox].iter().any(|&p| p != 0));
                        if hd == 0 {
                            assert!(!bar, "frame {frame}: the original graphics' bar");
                        } else {
                            composed += 1;
                            filled += usize::from(bar);
                        }
                    }
                }
                frame += 1;
            }
            let [off, on] = &*sides;
            assert!(
                off.engine.events == on.engine.events,
                "frame {frame}: the events differ"
            );
        };
        let none = &mut |_: &mut Engine, _: usize, _: f64| {};
        // A new game: the music copy's please-wait page (the data folder
        // may be new), its opening film ended, Tibet's first view.
        for s in sides.iter_mut() {
            s.engine.start_new_game().unwrap();
        }
        if sides[0].engine.copying() {
            play(&mut sides, 60, none, None);
        }
        for s in sides.iter_mut() {
            assert!(!s.engine.copying());
            s.engine.end_video();
        }
        play(&mut sides, 60, none, None);
        assert_eq!(sides[1].engine.loop_type, LoopType::Warp);
        let tibet = sides[1].engine.place().0.to_string();
        // The shaman's dialog, a line clicked every two seconds; the
        // options menu over it for a while.
        for s in sides.iter_mut() {
            s.engine.start_dialog(360).unwrap();
        }
        let click = &mut |e: &mut Engine, k: usize, t: f64| {
            if k % 120 == 119 {
                e.click(t);
            }
        };
        play(&mut sides, 240, click, None);
        let mut options = OptionsMenu::new();
        options.set_info(menu_info("Game"));
        options.open();
        play(&mut sides, 16, none, Some(&options));
        // (A subtitle under the menu: the game's texts drawn again before
        // the menu's.)
        let on = &sides[1].engine;
        assert_eq!(on.loop_type, LoopType::Dialog);
        assert!(on.hd_texts().is_some_and(|t| !t.runs.is_empty()));
        play(&mut sides, 300, click, None);
        for s in sides.iter_mut() {
            if s.engine.loop_type == LoopType::Dialog {
                s.engine.end_dialog();
            }
        }
        // Ireland's and China's first views, turned around.
        let turn = &mut |e: &mut Engine, _: usize, _: f64| e.mouse_motion(12, 1);
        for (commands, place) in [
            (
                ["(monde=1)", "(changewam=bigile1.wam)", "(vue=5)"],
                ("bigile1.wam", 5),
            ),
            (
                ["(monde=3)", "(changewam=chine.wam)", "(vue=10)"],
                ("chine.wam", 10),
            ),
        ] {
            for s in sides.iter_mut() {
                for c in commands {
                    s.engine.run_command(c.as_bytes());
                }
            }
            play(&mut sides, 30, none, None);
            for s in sides.iter_mut() {
                assert_eq!(s.engine.place(), place);
            }
            play(&mut sides, 90, turn, None);
        }
        // The game's main menu, the pointer on a button (its tooltip).
        for s in sides.iter_mut() {
            s.engine.open_menu_page(PageKind::Main);
            s.engine.pointer(375, 238);
        }
        play(&mut sides, 60, none, None);
        assert_eq!(sides[1].engine.loop_type, LoopType::Menu);
        let events = &sides[1].engine.events;
        let entered: Vec<u32> = events
            .iter()
            .filter_map(|e| match e {
                Event::Enter { view } => Some(*view),
                _ => None,
            })
            .collect();
        eprintln!(
            "{frame} frames from {tibet}, {} events, views entered {entered:?}; \
             {composed} HD pictures, {filled} with the sides, {layers} frames with the HD view",
            events.len()
        );
        assert!(entered.contains(&5) && entered.contains(&10), "{entered:?}");
        assert!(layers > 150, "{layers} frames with the HD view");
        assert!(
            filled * 2 > composed,
            "{filled} of {composed} with the sides filled"
        );
    }
}

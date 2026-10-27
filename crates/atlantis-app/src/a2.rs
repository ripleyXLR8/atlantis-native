//! Atlantis II, as far as it goes: the player menu opens first (a symbol
//! continues that player's newest save or starts a new game, on Tibet's
//! first view, `tibet.wam` view 2); the player looks around with the mouse
//! (or the arrow keys, a port convenience) and walks from view to view
//! with a left click on a move zone (the turn, the transition video, the
//! arrival), the place animations playing. The chapter scripts run the
//! game as the original's: the opening film, the music, the dialogs they
//! call (a left click goes to the next line, Escape ends a dialog). The
//! right button opens the bag bar at the bottom of the screen (the mouse
//! then moves a pointer over it; a click puts an object in the hand, drawn
//! at the screen centre). Escape opens
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
//! `--discs`. GOG.com's installation is a game folder too: its program is
//! `Game.exe` (the 1.01 patch's, with GOG's fixes; its `atlantis2.exe` is a
//! launcher) and its `DATAS_A2` folder holds every disc, so no disc is
//! asked for. With a 1.01 executable (GOG's, or a CD installation patched
//! to 1.01) the game behaves as 1.01 does: the subtitles' line breaks and
//! font sizes, and no Alt+Enter (1.00's switch between full screen and
//! window; F11 is the port's own and works with both).
//!
//! The window opens in full screen when `atlant2.cfg`'s full-screen byte
//! says so, as the original's start-up does (1, its default; both
//! releases read it), or with `--fullscreen`; Alt+Enter (1.00) and F11
//! switch it and keep the choice there, written at exit as the original
//! writes it.
//!
//! `--original` turns the enhancements off, as for the first game: the
//! bug fixes, every one of them (`atlantis2_engine::BugFixes`: every
//! music track heard to its end, a track that ends with nothing after it
//! played again, the random music mark past the last, Ireland's puzzle
//! zones answering for no named zone), the interface conveniences
//! (`atlantis2_engine::Enhancements::ui_ux`: the arrow keys' turn) and the
//! HD mode.
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
//! 480 or 800 x 600, `atlantis2_engine::display`), the bug fixes (all
//! at once, or one by one in "Choose the bug fixes..."); the game is
//! paused while it is open. Its choices are kept in `settings.ini` beside
//! the saves (the first game's keys, and one per bug fix); `--hd`,
//! `--widescreen` and `--original` hold for the run without changing it.
//! Started from the desktop's game library, the menu ends with "Game
//! library..." (back to the library's screen, [`Ended::Library`]), and
//! Shift in the first moments of the library's one game does the same.
//!
//! The game is held while the window does not have the focus, as the
//! original holds it: no frame runs, its clock stands still and its sound
//! is silent until the focus comes back.
//!
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
//! view entry (`(monde=1)(changewam=bigile1.wam)(vue=5)`: Ireland);
//! `--seed <n>` gives `rand` the seed `n` instead of the clock (the
//! original seeds it with `GetTickCount()` at start-up), so that a run
//! draws the same animations, cameras and choices again.
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

use atlantis2_data::exe::Release;
use atlantis2_data::files::OpenError;
use atlantis2_data::save::SystemTime;
use atlantis2_data::A2Files;
use atlantis2_engine::engine::PASS;
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
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

pub use atlantis2_data::files::holds_atlantis2;

/// The original's screen.
pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 480;

/// A turn key held: the mouse move it stands for each frame, in mickeys
/// (a port convenience, [`Enhancements::ui_ux`]; the original turns with
/// the mouse only). With the original's decay this turns about 55° a
/// second at 60 frames a second.
const KEY_TURN: i32 = 8;

/// The frame period: the original shows a frame per refresh of the screen
/// (its flip waits for the vertical blank), the port one per refresh of a
/// 60 Hz VGA screen ([`PASS`], 16.683 ms).
const FRAME: Duration = Duration::from_nanos(16_683_217);

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
    /// Keys pressed in that menu before the screenshot (`--options-keys`:
    /// `down,down,enter`); its changes hold for the run only.
    pub options_keys: Vec<MenuKey>,
    /// The `rand` seed of the start-up (`--seed`), instead of the clock
    /// ([`Engine::seed_rand`]): the same draws on every run.
    pub seed: Option<u32>,
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

    /// Parses `--options-keys`' value: key names (`up`, `down`, `left`,
    /// `right`, `enter`, `escape`) separated by commas.
    pub fn parse_options_keys(v: &str) -> Option<Vec<MenuKey>> {
        v.split(',')
            .map(|k| {
                Some(match k.trim().to_ascii_lowercase().as_str() {
                    "up" => MenuKey::Up,
                    "down" => MenuKey::Down,
                    "left" => MenuKey::Left,
                    "right" => MenuKey::Right,
                    "enter" => MenuKey::Enter,
                    "escape" => MenuKey::Escape,
                    _ => return None,
                })
            })
            .collect()
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
        if let Some(seed) = self.seed {
            engine.seed_rand(seed);
        }
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

/// The load page's text of a save's time, as the original's
/// (FUN_0046f830 @ 0x0046f928, 0x0046f94c): the user's locale's short date
/// and time, a space between.
#[cfg(windows)]
fn local_date_text(t: &SystemTime) -> Vec<u8> {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::Globalization::{
        GetDateFormatA, GetTimeFormatA, DATE_SHORTDATE, LOCALE_USER_DEFAULT,
    };
    let st = SYSTEMTIME {
        wYear: t.year,
        wMonth: t.month,
        wDayOfWeek: t.day_of_week,
        wDay: t.day,
        wHour: t.hour,
        wMinute: t.minute,
        wSecond: t.second,
        wMilliseconds: t.milliseconds,
    };
    let mut date = [0u8; 256];
    let mut time = [0u8; 256];
    // SAFETY: both calls read `st` and write at most 256 bytes (their
    // count, the terminating zero included) into their buffers.
    let (nd, nt) = unsafe {
        (
            GetDateFormatA(
                LOCALE_USER_DEFAULT,
                DATE_SHORTDATE,
                &st,
                std::ptr::null(),
                date.as_mut_ptr(),
                256,
            ),
            GetTimeFormatA(
                LOCALE_USER_DEFAULT,
                0,
                &st,
                std::ptr::null(),
                time.as_mut_ptr(),
                256,
            ),
        )
    };
    if nd <= 0 || nt <= 0 {
        return atlantis2_engine::menu::date_time(t);
    }
    let mut out = date[..nd as usize - 1].to_vec();
    out.push(b' ');
    out.extend_from_slice(&time[..nt as usize - 1]);
    out
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
    #[cfg(windows)]
    {
        engine.date_text = local_date_text;
    }
    // The original seeds `rand` with GetTickCount() at start-up
    // (@ 0x0047d740): a clock in milliseconds (`--seed` fixes it).
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u32);
    engine.seed_rand(seed);
    Ok((engine, format!("Game: {}", build.label)))
}

/// `--headless`: runs `frames` frames of [`PASS`] without input (the 15 Hz
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
        engine.set_clock(i as f64 * PASS);
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
        // Their changes for this run only: the screen's row as the
        // window's at the key (its new size from the next pass), no file
        // written.
        for &k in &start.options_keys {
            match menu.key(k, menu_values(&engine)) {
                MenuAction::SetEnhancements(x) => engine.enhancements = x,
                MenuAction::SetScreen800(on) => engine.set_screen_800(on),
                MenuAction::None | MenuAction::Close | MenuAction::OpenLibrary => {}
            }
        }
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
    let mut app = window_app(engine, &label, fullscreen, size);
    el.run_app(&mut app)
        .map_err(|e| format!("The window failed: {e}"))
}

/// How a window of the game started from the game library ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// The game was quit (its menus, the window closed).
    Quit,
    /// The options menu's "Game library..." entry, or Shift held as the
    /// game started: the game library's screen is to be shown.
    Library,
}

/// [`run_window`] for the game started from the game library, on its event
/// loop (a program makes one event loop only), run again for this game:
/// the options menu has the "Game library..." entry, and with
/// `shift_grace` (the library's one game, started without its screen)
/// Shift held in the first moments goes back to the library, as the
/// first game's start does ([`crate::session::Session::shift_grace`]).
pub fn run_window_on(
    el: &mut EventLoop<()>,
    engine: Engine,
    label: String,
    fullscreen: bool,
    size: Option<(u32, u32)>,
    shift_grace: bool,
) -> Result<Ended, String> {
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    let mut app = window_app(engine, &label, fullscreen, size);
    app.started_from_library(shift_grace.then(|| Instant::now() + SHIFT_GRACE));
    el.run_app_on_demand(&mut app)
        .map_err(|e| format!("The window failed: {e}"))?;
    Ok(if app.to_library {
        Ended::Library
    } else {
        Ended::Quit
    })
}

/// How long after an automatic start Shift still goes back to the game
/// library (the first game's delay).
const SHIFT_GRACE: Duration = Duration::from_millis(2000);

/// The window's application: the game, its sound started.
fn window_app(engine: Engine, label: &str, fullscreen: bool, size: Option<(u32, u32)>) -> A2App {
    let voices = engine.voices().clone();
    // The sound stops while the game is held (the options menu open, the
    // window without the focus): silence, the voices kept where they are.
    let paused = Arc::new(AtomicBool::new(false));
    let silent = paused.clone();
    let audio = crate::audio_out::start_with(move |out, rate, channels| {
        if silent.load(Ordering::Relaxed) {
            out.fill(0.0);
        } else {
            voices.fill(out, rate, channels);
        }
    });
    A2App::new(
        engine,
        label,
        fullscreen,
        size,
        audio,
        paused,
        Instant::now(),
    )
}

/// Whether the game is held: while the port's options menu is open, and
/// while the window does not have the focus, as the original, whose 15 Hz
/// task (@ 0x0040caa5) and frame dispatcher (@ 0x0049ce25) return at once
/// unless its window is active (FUN_0058ff10: the window's active byte and
/// DirectDraw's `TestCooperativeLevel`): timers, scripts, videos, dialogs
/// and animations stop, and its DirectSound buffers, made without the
/// global focus flag, are not heard. The time away does not count: the
/// scheduler moves the task's last run on while it returns (the seconds
/// accumulator gains none of it).
fn must_hold(menu_open: bool, focused: bool) -> bool {
    menu_open || !focused
}

/// The game's clock: the time since it started, less the time it was
/// held.
#[derive(Clone, Copy, Debug)]
struct GameClock {
    started: Instant,
    held_since: Option<Instant>,
}

impl GameClock {
    fn new(now: Instant) -> GameClock {
        GameClock {
            started: now,
            held_since: None,
        }
    }

    fn held(&self) -> bool {
        self.held_since.is_some()
    }

    /// Stops the clock (it stays stopped from the first hold on).
    fn hold(&mut self, now: Instant) {
        self.held_since.get_or_insert(now);
    }

    /// Starts it again where it stopped.
    fn release(&mut self, now: Instant) {
        if let Some(t) = self.held_since.take() {
            self.started += now.saturating_duration_since(t);
        }
    }

    /// Its time at `now`, seconds.
    fn secs(&self, now: Instant) -> f64 {
        self.held_since
            .unwrap_or(now)
            .saturating_duration_since(self.started)
            .as_secs_f64()
    }
}

impl A2App {
    /// The window's game before its window opens, its clock started at
    /// `now`.
    fn new(
        engine: Engine,
        label: &str,
        fullscreen: bool,
        size: Option<(u32, u32)>,
        audio: Option<cpal::Stream>,
        paused: Arc<AtomicBool>,
        now: Instant,
    ) -> A2App {
        let (w, h) = engine.screen_size();
        let mut menu = OptionsMenu::new();
        menu.set_info(menu_info(label));
        // Full screen as `atlant2.cfg` says (the start-up's mode 3 or 2,
        // @ 0x0049d0af), or as `--fullscreen` asks.
        let fullscreen = fullscreen || engine.fullscreen();
        // The voices' status is the output's from now on: a voice ends
        // when it has been played out, as DirectSound's buffers do (a
        // dialog line, voice 2's `NEWSOUND`, FUN_0047a9c0). Without a
        // sound device the mix is played into nothing at the game's pace
        // ([`A2App::play_silent`]), so the voices still end.
        engine.voices().attach_output();
        let silent = audio.is_none().then_some(0);
        A2App {
            _audio: audio,
            silent,
            clock: GameClock::new(now),
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
            menu_pointer: None,
            game_pointer: None,
            motion: (0.0, 0.0),
            keys: [false; 4],
            locked: false,
            focused: true,
            alt_left: false,
            refocused: false,
            next_frame: now,
            shift_until: None,
            to_library: false,
        }
    }

    /// Started from the game library: the options menu's "Game
    /// library..." entry, and Shift until `shift_until` goes back to it.
    fn started_from_library(&mut self, shift_until: Option<Instant>) {
        self.menu.set_library(true);
        self.shift_until = shift_until;
    }

    /// Ends the game for the game library's screen (the exit hook run, as
    /// when the game is quit).
    fn back_to_library(&mut self, el: &ActiveEventLoop) {
        log::info!("Atlantis II stopped: the game library");
        self.to_library = true;
        self.engine.at_exit();
        el.exit();
    }
}

struct A2App {
    /// The sound output (kept alive while the window is open).
    _audio: Option<cpal::Stream>,
    /// Without a sound output: the primary frames the mix has been played
    /// to so far, on the game's clock.
    silent: Option<u64>,
    /// The game's clock: it starts with the window and stands still while
    /// the game is held (the options menu open, the window without the
    /// focus).
    clock: GameClock,
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
    /// The sound silenced while the game is held.
    paused: Arc<AtomicBool>,
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
    /// The left Alt key held (Alt+Enter, 1.00 only).
    alt_left: bool,
    /// The focus came back with the pointer free: the next press may be
    /// the one that gave it ([`mouse_button`]'s `focus_click`).
    refocused: bool,
    next_frame: Instant,
    /// Until when Shift goes back to the game library (the library's one
    /// game started without its screen).
    shift_until: Option<Instant>,
    /// The game library's screen asked for: the program shows it once the
    /// window has closed ([`Ended::Library`]).
    to_library: bool,
}

impl A2App {
    /// Full screen or window (F11; Alt+Enter with 1.00), the choice kept
    /// in `atlant2.cfg`'s byte 8 as the original's Alt+Enter keeps it.
    fn toggle_fullscreen(&mut self) {
        if let Some(w) = &self.window {
            let full = w.fullscreen().is_none();
            w.set_fullscreen(full.then_some(Fullscreen::Borderless(None)));
            self.engine.set_fullscreen(full);
        }
    }

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
            // A press after this pass is not the one that gave the focus:
            // grabbed, it reaches the game anyway; not grabbed (both grab
            // modes refused), the pointer stays free and every press is a
            // click, so the flag must not swallow a later one.
            self.refocused = false;
        }
        let (mx, my) = (self.motion.0.trunc(), self.motion.1.trunc());
        self.motion.0 -= mx;
        self.motion.1 -= my;
        // The arrow keys turn only with the interface conveniences.
        let keys = if self.engine.enhancements.ui_ux {
            self.keys
        } else {
            [false; 4]
        };
        let [left, right, up, down] = keys.map(i32::from);
        let dx = mx as i32 + (right - left) * KEY_TURN;
        let dy = my as i32 + (down - up) * KEY_TURN;
        if (dx != 0 || dy != 0) && !in_menu {
            self.engine.mouse_motion(dx, dy);
        }
        // The pass's time is the one it was due at, as the original's
        // passes keep the pace of the screen's refresh.
        self.engine.set_clock(self.clock.secs(self.next_frame));
        self.play_silent();
        if let Some(size) = self.window.as_ref().map(|w| w.inner_size()) {
            self.picture
                .prepare(&mut self.engine, size.width as usize, size.height as usize);
        }
        // The screen's size: 800 x 600 after the original's option.
        let (w, h) = self.engine.screen_size();
        self.screen.resize(w * h, 0);
        self.engine.pass(Some(&mut self.screen));
    }

    /// Without a sound device, the mix played up to the game's clock and
    /// dropped: the voices advance and end as with a device (DirectSound
    /// plays a buffer out whatever the card), the clock standing still
    /// while the game is held.
    fn play_silent(&mut self) {
        let Some(done) = self.silent else { return };
        let rate = atlantis2_engine::sound::PRIMARY_RATE;
        let due = (self.engine.clock() * f64::from(rate)) as u64;
        let mut buf = vec![0.0f32; 2 * 1024];
        let mut at = done;
        while at < due {
            let n = (due - at).min(1024) as usize;
            self.engine.voices().fill(&mut buf[..2 * n], rate, 2);
            at += n as u64;
        }
        self.silent = Some(at);
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
        self.hold();
        self.grab(false);
        self.keys = [false; 4];
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// The menu closed: the game goes on where it was.
    fn close_menu(&mut self) {
        self.menu.close();
        self.hold();
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

    /// Holds the game, or lets it go on, as [`must_hold`] says: held, its
    /// clock stands still and its sound is silent (the voices kept where
    /// they are), and no frame runs.
    fn hold(&mut self) {
        self.hold_at(Instant::now());
    }

    fn hold_at(&mut self, now: Instant) {
        if must_hold(self.menu.is_open(), self.focused) {
            self.clock.hold(now);
            self.paused.store(true, Ordering::Relaxed);
        } else if self.clock.held() {
            self.clock.release(now);
            self.paused.store(false, Ordering::Relaxed);
            self.next_frame = now;
        }
    }

    /// The window gains or loses the focus at `now`: without it the game
    /// is held ([`must_hold`]).
    fn set_focus(&mut self, on: bool, now: Instant) {
        self.focused = on;
        if !on {
            self.grab(false);
            self.keys = [false; 4];
            // Its release is not seen once the window is left (Alt+Tab).
            self.alt_left = false;
        } else {
            self.motion = (0.0, 0.0);
            self.refocused = !self.locked;
        }
        self.hold_at(now);
    }

    /// Whether a press (`pressed`) is the one that gave the window the
    /// focus, with the interface conveniences on ([`mouse_button`]'s
    /// `focus_click`): only the first press after a focus gain, before the
    /// next pass ([`A2App::step`]).
    fn focus_click(&mut self, pressed: bool) -> bool {
        pressed && std::mem::take(&mut self.refocused) && self.engine.enhancements.ui_ux
    }

    /// A redraw at `now`: the game's pass when one is due and the game is
    /// not held; whether it ran.
    fn redraw(&mut self, now: Instant) -> bool {
        if now < self.next_frame || self.clock.held() {
            return false;
        }
        self.step();
        self.next_frame += FRAME;
        if self.next_frame < now {
            self.next_frame = now + FRAME;
        }
        true
    }

    fn menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::None => {}
            MenuAction::Close => self.close_menu(),
            // The window's event ends the event loop ([`A2App::back_to_library`]).
            MenuAction::OpenLibrary => self.to_library = true,
            MenuAction::SetEnhancements(x) => {
                // In force at once: the HD mode at the next frame, the
                // music's from the next track. Only what changed is
                // written: `--original`, `--hd` and `--widescreen` hold for
                // the run without changing `settings.ini`.
                let before = std::mem::replace(&mut self.engine.enhancements, x);
                log::info!("enhancements: {x:?}");
                let mut s = stored_settings();
                s.enhancements = s.enhancements.with_changes(&before, &x);
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
        if let Some(t) = self.shift_until {
            if Instant::now() >= t {
                self.shift_until = None;
            } else if matches!(&event, WindowEvent::ModifiersChanged(m) if m.state().shift_key()) {
                log::info!("Shift held at start-up: the game library");
                self.back_to_library(el);
                return;
            }
        }
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
                        if self.to_library {
                            self.back_to_library(el);
                        }
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
                                if self.to_library {
                                    self.back_to_library(el);
                                }
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
                // The exit hook: the mandala solution log, `atlant2.cfg`
                // (FUN_00495650, FUN_0047f530).
                self.engine.at_exit();
                el.exit();
            }
            WindowEvent::Focused(on) => self.set_focus(on, Instant::now()),
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
                let now = self.clock.secs(Instant::now());
                let pressed = state == ElementState::Pressed;
                let focus_click = self.focus_click(pressed);
                if mouse_button(
                    &mut self.engine,
                    self.locked,
                    focus_click,
                    button,
                    pressed,
                    now,
                ) {
                    self.grab(true);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                let code = match event.physical_key {
                    PhysicalKey::Code(c) => Some(c),
                    PhysicalKey::Unidentified(_) => None,
                };
                if code == Some(KeyCode::AltLeft) {
                    self.alt_left = down;
                }
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
                    Key::Named(NamedKey::F11) if down => self.toggle_fullscreen(),
                    // Alt+Enter: full screen or window, as 1.00's key-down
                    // callback switches (@ 0x004762f0: the main Enter,
                    // `DIK_RETURN`, while the key state @ 0x006e2b6c of the
                    // modifier slot 0x7a of the key table is down; with
                    // DirectInput that slot is `DIK_LMENU`, the left Alt,
                    // @ 0x00586133); the 1.01 patch removed it
                    // (`atlantis2-gog-fr-2183168` @ 0x004763c0).
                    Key::Named(NamedKey::Enter)
                        if down
                            && !event.repeat
                            && code == Some(KeyCode::Enter)
                            && self.alt_left
                            && self.engine.files().build().release == Release::V100 =>
                    {
                        self.toggle_fullscreen()
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
                // `EXIT` in a script, or the menus, quit the game.
                if self.redraw(Instant::now()) && self.engine.quit() {
                    self.engine.at_exit();
                    el.exit();
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
/// (the right one), also while the pointer is free, as the original's: it
/// reads DirectInput's mouse state once a pass (FUN_0058e9f0,
/// `GetDeviceState` @ 0x0058eaf5) and calls the press callback for a
/// button down that was up at the pass before (@ 0x0058ec28..0x0058ec55),
/// so the press that gives its window the focus is a click once the device
/// is acquired again. `focus_click`: the press is the one that gave the
/// window the focus and the interface conveniences
/// ([`Enhancements::ui_ux`]) are on: it only grabs the pointer (outside
/// the menus; a left click still ends a video). True when the pointer is
/// to be grabbed (a press outside the menus with the pointer free).
fn mouse_button(
    engine: &mut Engine,
    locked: bool,
    focus_click: bool,
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
    let grab = !locked && !in_menu;
    if grab && focus_click && !(in_video && button == MouseButton::Left) {
        return true;
    }
    match button {
        MouseButton::Left => engine.click(now),
        MouseButton::Right => engine.right_click(),
        _ => {}
    }
    grab
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

    /// The window losing the focus holds the game as the options menu
    /// does: no frame runs and the clock stands still until it comes back.
    #[test]
    fn the_game_clock_stands_while_held() {
        assert!(must_hold(false, false));
        assert!(must_hold(true, true));
        assert!(!must_hold(false, true));
        let t0 = Instant::now();
        let at = |s: f64| t0 + Duration::from_secs_f64(s);
        let mut c = GameClock::new(t0);
        assert_eq!(c.secs(at(2.0)), 2.0);
        // Away from 2 s to 7 s, then the menu opened at 8 s while the
        // clock already stands (a second hold changes nothing), closed at
        // 9 s with the window still away, back at 10 s.
        c.hold(at(2.0));
        assert_eq!(c.secs(at(7.0)), 2.0);
        c.hold(at(8.0));
        assert_eq!(c.secs(at(9.0)), 2.0);
        c.release(at(10.0));
        assert!(!c.held());
        assert_eq!(c.secs(at(10.0)), 2.0);
        assert_eq!(c.secs(at(11.5)), 3.5);
    }

    /// With the game (`ATLANTIS2_DATA`, `ATLANTIS2_DISCS`; skipped
    /// without): the window's game as its events drive it. A pass a
    /// [`FRAME`] at the time it was due ([`PASS`] apart); the focus lost,
    /// no pass runs, the 15 Hz task and the clock stand still and the
    /// sound is silent, as in the original, whose frame dispatcher
    /// (@ 0x0049ce25) and 15 Hz task (@ 0x0040caa5) return at once while
    /// its window is not active (FUN_0058ff10); the focus back, the game
    /// goes on a pass later, the time away not counted.
    #[test]
    fn the_game_is_held_without_the_focus() {
        let (Some(dir), Some(discs)) = (
            std::env::var_os("ATLANTIS2_DATA"),
            std::env::var_os("ATLANTIS2_DISCS"),
        ) else {
            atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
            return;
        };
        let (engine, label) = boot(
            &PathBuf::from(dir),
            &[PathBuf::from(discs)],
            Enhancements::default(),
        )
        .expect("game");
        assert!((FRAME.as_secs_f64() - PASS).abs() < 1e-9);
        let t0 = Instant::now();
        let paused = Arc::new(AtomicBool::new(false));
        let mut app = A2App::new(engine, &label, false, None, None, paused.clone(), t0);
        let at = |k: u32| t0 + FRAME * k;
        let mut ticks = Vec::new();
        for k in 0..60 {
            // Due once a frame, not before.
            assert!(!app.redraw(at(k) - Duration::from_micros(1)));
            assert!(app.redraw(at(k)), "frame {k}");
            assert!((app.engine.clock() - f64::from(k) * PASS).abs() < 1e-6);
            ticks.push(app.engine.ticks);
        }
        // The 15 Hz task every 4th pass (from the first after the start).
        assert!(ticks.windows(5).all(|w| w[4] == w[0] + 1));
        let (clock, tick) = (app.engine.clock(), app.engine.ticks);
        app.set_focus(false, at(60));
        assert!(paused.load(Ordering::Relaxed));
        for k in 60..600 {
            assert!(!app.redraw(at(k)), "frame {k} away");
        }
        assert_eq!((app.engine.clock(), app.engine.ticks), (clock, tick));
        app.set_focus(true, at(600));
        assert!(!paused.load(Ordering::Relaxed));
        assert!(app.redraw(at(600)));
        assert!((app.engine.clock() - (clock + PASS)).abs() < 1e-6);
        // The options menu over the focused window holds it the same way.
        app.open_menu();
        assert!(app.clock.held() && paused.load(Ordering::Relaxed));
        assert!(!app.redraw(at(700)));
    }

    /// Review L19/L20: a window without a sound device still plays the
    /// voices out on the game's clock, as DirectSound plays a buffer out
    /// whatever the card, so the dispatcher's FUN_0047a2f0 frees a
    /// `NEWSOUND`'s entry once its sound has been played (it stayed in the
    /// shared table for good, voice 2 never ending without a device).
    #[test]
    fn a_window_without_sound_device_plays_the_voices_out() {
        let Some(files) = a2_files() else { return };
        let mut engine = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        engine.start_new_game().unwrap();
        engine.end_video();
        for c in ["(changewam=tibet.wam)", "(vue=2)", "(chapter=75)"] {
            engine.run_command(c.as_bytes());
        }
        for _ in 0..400 {
            match engine.loop_type {
                LoopType::Video => engine.end_video(),
                LoopType::Dialog => engine.end_dialog(),
                LoopType::Warp if !engine.turning() => break,
                _ => {}
            }
            engine.tick();
            engine.pass(None);
        }
        assert_eq!(engine.loop_type, LoopType::Warp);
        engine.sound_table_clear();
        engine.run_command(b"(NewSound=1)");
        let (rate, _, frames, _) = engine
            .voices()
            .format(atlantis2_engine::sound::VOICE_EFFECTS)
            .expect("sound001 on voice 2");
        let named = |e: &Engine| e.sound_table().iter().any(|(_, n)| n.contains("001"));
        assert!(named(&engine));
        let t0 = Instant::now();
        let paused = Arc::new(AtomicBool::new(false));
        let mut app = A2App::new(engine, "test", false, None, None, paused, t0);
        let length = frames as f64 / f64::from(rate);
        let passes = ((length + 0.5) / PASS) as u32;
        for k in 0..passes {
            app.redraw(t0 + FRAME * k);
        }
        let voice2 = app
            .engine
            .voices()
            .is_playing(atlantis2_engine::sound::VOICE_EFFECTS);
        assert!(!voice2, "voice 2 still plays {length:.2} s on");
        assert!(!named(&app.engine), "{:?}", app.engine.sound_table());
    }

    /// The game's files, or `None` (the test skipped) without
    /// `ATLANTIS2_DATA` / `ATLANTIS2_DISCS`.
    fn a2_files() -> Option<atlantis2_data::A2Files> {
        let (Some(dir), Some(discs)) = (
            std::env::var_os("ATLANTIS2_DATA"),
            std::env::var_os("ATLANTIS2_DISCS"),
        ) else {
            atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
            return None;
        };
        let (files, _) = atlantis2_data::A2Files::open_dir(
            std::path::Path::new(&dir),
            &[std::path::PathBuf::from(discs)],
        )
        .expect("game folder");
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
            e.set_clock(f64::from(i) * PASS);
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
            clock += PASS;
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
        assert!(!mouse_button(
            &mut e,
            false,
            false,
            MouseButton::Left,
            true,
            1.0
        ));
        assert!(e.button_held());
        pass(&mut e);
        pass(&mut e);
        while e.copying() {
            pass(&mut e);
        }
        assert_ne!(e.loop_type, LoopType::Menu);
        assert!(!mouse_button(
            &mut e,
            true,
            false,
            MouseButton::Left,
            false,
            1.1
        ));
        assert!(!e.button_held());
        // Outside the menus, with the pointer free, the press that gave
        // the window the focus only grabs it (the interface conveniences).
        assert_eq!(e.loop_type, LoopType::Video);
        e.end_video();
        pass(&mut e);
        assert!(mouse_button(
            &mut e,
            false,
            true,
            MouseButton::Left,
            true,
            2.0
        ));
        assert!(!e.button_held());
    }

    /// The original reads the mouse's buttons once a pass and clicks for a
    /// button newly down (FUN_0058e9f0 @ 0x0058ec28..0x0058ec55), the press
    /// that activates its window included: a press with the pointer free
    /// clicks and grabs it, unless it is the focusing press and the
    /// interface conveniences are on.
    #[test]
    fn a_press_with_the_pointer_free_clicks() {
        let Some(files) = a2_files() else { return };
        let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
        e.end_video();
        assert_eq!(e.loop_type, LoopType::Warp);
        // Not the focusing press (or `--original`): a click.
        assert!(mouse_button(
            &mut e,
            false,
            false,
            MouseButton::Left,
            true,
            3.0
        ));
        assert!(e.button_held());
        assert!(!mouse_button(
            &mut e,
            false,
            false,
            MouseButton::Left,
            false,
            3.1
        ));
        // The right button opens the bag bar, and grabs.
        assert!(mouse_button(
            &mut e,
            false,
            false,
            MouseButton::Right,
            true,
            4.0
        ));
        assert!(e.bar.open);
    }

    /// The arrow keys turn the view only with the interface conveniences
    /// (`--original` turns them off: the original's key-down callback
    /// @ 0x004762f0 has no turn key); the window opens in full screen when
    /// `atlant2.cfg`'s byte 8 says so (1 by default, the start-up's mode 3
    /// @ 0x0049d0af).
    #[test]
    fn arrow_keys_are_a_convenience_and_the_cfg_chooses_the_screen() {
        let Some(files) = a2_files() else { return };
        let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
        e.end_video();
        assert!(e.fullscreen(), "atlant2.cfg's default");
        let paused = Arc::new(AtomicBool::new(false));
        let t0 = Instant::now();
        let mut app = A2App::new(e, "", false, None, None, paused, t0);
        assert!(app.fullscreen);
        for (ui_ux, turns) in [(false, false), (true, true)] {
            app.engine.enhancements.ui_ux = ui_ux;
            app.engine.look.yaw_speed = 0.0;
            app.keys = [false, true, false, false];
            app.step();
            assert_eq!(app.engine.look.yaw_speed != 0.0, turns, "ui_ux {ui_ux}");
        }
        app.engine.set_fullscreen(false);
        assert!(!app.engine.fullscreen());
        assert_eq!(app.engine.cfg().fullscreen, 0);
        let e = app.engine;
        let app = A2App::new(
            e,
            "",
            false,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            t0,
        );
        assert!(!app.fullscreen);
    }

    /// Started from the game library: the options menu's last entry,
    /// "Game library...", asks for the library's screen (the program shows
    /// it once the window has closed); started from the command line, the
    /// menu has no such entry.
    #[test]
    fn the_library_entry_goes_back_to_the_library() {
        let Some(files) = a2_files() else { return };
        let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
        e.end_video();
        let paused = Arc::new(AtomicBool::new(false));
        let t0 = Instant::now();
        let mut app = A2App::new(e, "", false, None, None, paused, t0);
        let up_to_last = |app: &mut A2App| {
            app.open_menu();
            let v = menu_values(&app.engine);
            app.menu.key(MenuKey::Up, v);
            app.menu.selected()
        };
        use atlantis2_engine::options_menu::Entry;
        assert_eq!(up_to_last(&mut app), Entry::Close);
        app.menu.close();
        app.started_from_library(Some(t0 + SHIFT_GRACE));
        assert_eq!(up_to_last(&mut app), Entry::Library);
        let action = app.menu.key(MenuKey::Enter, menu_values(&app.engine));
        assert_eq!(action, MenuAction::OpenLibrary);
        assert!(!app.to_library);
        app.menu_action(action);
        assert!(app.to_library && !app.menu.is_open());
        assert_eq!(app.shift_until, Some(t0 + SHIFT_GRACE));
    }

    /// The press that gave the window the focus only grabs the pointer
    /// (the interface conveniences); a press after the next pass is a
    /// click even when the pointer could not be grabbed (here: no window,
    /// as where both grab modes are refused), so a focus gained from the
    /// keyboard does not swallow a press long after.
    #[test]
    fn only_the_focusing_press_is_swallowed() {
        let Some(files) = a2_files() else { return };
        let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
        e.end_video();
        let t0 = Instant::now();
        let mut app = A2App::new(
            e,
            "",
            false,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            t0,
        );
        app.set_focus(false, t0);
        app.set_focus(true, t0);
        assert!(!app.locked);
        assert!(app.focus_click(true), "the press that gave the focus");
        assert!(!app.focus_click(true));
        // The focus gained from the keyboard: a pass runs, the grab fails.
        app.set_focus(false, t0);
        app.set_focus(true, t0);
        app.step();
        assert!(!app.locked);
        assert!(!app.focus_click(true), "a press after a pass is a click");
        // `--original`: never swallowed.
        app.set_focus(false, t0);
        app.set_focus(true, t0);
        app.engine.enhancements.ui_ux = false;
        assert!(!app.focus_click(true));
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
            atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
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
            e.seed_rand(1);
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
        // `n` frames of `PASS` as the window runs them, `input` given to
        // both engines before each; every 8th frame (and the last)
        // composed in the window, with the options menu over it when
        // `options` is given.
        let mut play = |sides: &mut [Side; 2],
                        n: usize,
                        input: &mut dyn FnMut(&mut Engine, usize, f64),
                        options: Option<&OptionsMenu>| {
            for k in 0..n {
                for s in sides.iter_mut() {
                    let t = frame as f64 * PASS;
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

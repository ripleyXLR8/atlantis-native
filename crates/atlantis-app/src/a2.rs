//! Atlantis II, as far as it goes: the player menu opens first (a symbol
//! continues that player's newest save or starts a new game, on Tibet's
//! first view, `tibet.wam` view 2); the player looks around with the mouse
//! or the arrow keys and walks from view to view with a left click on a move
//! zone (the turn, the transition video, the arrival), the place animations
//! playing. The chapter scripts run the game as the original's: the opening
//! film, the music, the dialogs they call (a left click goes to the next
//! line, Escape ends a dialog). Escape opens the main menu (options,
//! volumes, load and save, quit) and closes it again; in the menus the
//! pointer is the game's own. A mode of its own, beside the first game's
//! front end: a window, the screen scaled to it, the sound; no bag or
//! subject menu yet. The saves (`save\ATLpp_ss.SAV`, the original's
//! format) and `atlant2.cfg` are kept in the `atlantis2` folder of the
//! port's data folder.
//!
//! The game folder is the installed one (`atlantis2.exe`, `wam\`,
//! `scenar\`...); the discs (CD images or copies of the CDs) are looked for
//! in it, in the folders directly inside it, and in the folders given with
//! `--discs`.
//!
//! `--original` turns the enhancements off (the minor bug fixes: the
//! music tracks heard to their end), as for the first game.
//!
//! Testing aids: `--view <id>[,<yaw>,<pitch>]` starts in another view of
//! the place (angles in degrees), `--dialog <n>` plays dialog `n` at once
//! over the view (a click goes to the next line, as in the original), with
//! its voices, `--menu <page>` shows a menu page.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use atlantis2_data::files::OpenError;
use atlantis2_data::save::SystemTime;
use atlantis2_data::A2Files;
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::{Engine, Enhancements, Fpu, LoopType, Variant};
use web_time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::dpi::PhysicalPosition;
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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct A2Start {
    /// A view of the place and its angles in degrees.
    pub view: Option<(u32, f32, f32)>,
    /// A dialog played at once.
    pub dialog: Option<u32>,
    /// A menu page shown at once.
    pub menu: Option<PageKind>,
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

    /// How the game starts: the player menu (the original's start-up), or
    /// a new game at once (headless runs and the testing aids), then the
    /// view, the dialog and the menu page asked for.
    pub fn begin(&self, engine: &mut Engine, headless: bool) -> Result<(), String> {
        let at_once = self.view.is_some() || self.dialog.is_some();
        if self.menu == Some(PageKind::Player) || !(headless || at_once || self.menu.is_some()) {
            engine.open_player_menu();
            return Ok(());
        }
        engine.start_new_game().map_err(|e| e.to_string())?;
        self.apply(engine)?;
        match self.menu {
            Some(PageKind::Credits) => engine.open_credits(),
            Some(page) => engine.open_menu_page(page),
            None => {}
        }
        Ok(())
    }

    /// Enters the view and starts the dialog (the opening film ended
    /// first).
    pub fn apply(&self, engine: &mut Engine) -> Result<(), String> {
        if self.view.is_some() || self.dialog.is_some() {
            engine.end_video();
        }
        if let Some((id, yaw, pitch)) = self.view {
            engine
                .enter_view(id, yaw.to_radians(), pitch.to_radians())
                .map_err(|e| e.to_string())?;
        }
        if let Some(n) = self.dialog {
            engine.start_dialog(n).map_err(|e| e.to_string())?;
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

/// The folder of the saves and `atlant2.cfg`.
pub fn data_dir() -> PathBuf {
    crate::desktop::data_dir().join("atlantis2")
}

/// Opens the game folder and starts the game as `start` says, the cube
/// drawn by `warpPII` (the reference renderer) for a process whose x87
/// control word starts as Win32's default.
pub fn open(
    dir: &Path,
    discs: &[PathBuf],
    start: &A2Start,
    enhancements: Enhancements,
) -> Result<Engine, String> {
    let mut engine = boot(dir, discs, enhancements)?;
    start.begin(&mut engine, false)?;
    Ok(engine)
}

/// The game folder opened and the game booted, with its saves' folder
/// and the enhancements asked for.
fn boot(dir: &Path, discs: &[PathBuf], enhancements: Enhancements) -> Result<Engine, String> {
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
    Ok(engine)
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
    let mut engine = boot(dir, discs, enhancements)?;
    start.begin(&mut engine, true)?;
    if start.menu.is_some() {
        // The pointer at the screen centre.
        engine.pointer(WIDTH as i32 / 2, HEIGHT as i32 / 2);
    }
    let mut screen = vec![0u16; (WIDTH * HEIGHT) as usize];
    let mut sound = vec![0.0f32; 2 * 22050 / 60];
    for i in 0..frames {
        engine.set_clock(i as f64 / 60.0);
        engine.pass(Some(&mut screen));
        // The sound is mixed and dropped.
        engine.voices().fill(&mut sound, 22050, 2);
    }
    if frames == 0 {
        engine.render(&mut screen);
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
    if let Some(page) = engine.menu().and_then(|m| m.kind()) {
        println!("menu page {page:?}");
    }
    match engine.music_playing() {
        Some(m) => println!("music {m}, volume {}", engine.music.volume),
        None => println!("no music, volume {}", engine.music.volume),
    }
    if let Some(path) = screenshot {
        let png = atlantis2_engine::png::encode_rgb565(&screen, WIDTH as usize, HEIGHT as usize);
        std::fs::write(path, png).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("screenshot: {}", path.display());
    }
    Ok(())
}

/// Opens a window on the game; returns when it is closed.
pub fn run_window(engine: Engine, fullscreen: bool) -> Result<(), String> {
    let el = EventLoop::new().map_err(|e| format!("No window can be opened: {e}"))?;
    let voices = engine.voices().clone();
    let audio =
        crate::audio_out::start_with(move |out, rate, channels| voices.fill(out, rate, channels));
    let mut app = A2App {
        _audio: audio,
        started: Instant::now(),
        engine,
        fullscreen,
        window: None,
        surface: None,
        screen: vec![0u16; (WIDTH * HEIGHT) as usize],
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
    /// The game's clock starts with the window.
    started: Instant,
    engine: Engine,
    fullscreen: bool,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    screen: Vec<u16>,
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
        self.engine.pass(Some(&mut self.screen));
    }

    /// Where the 640x480 screen is in the window: its offset and scale.
    fn layout(&self) -> Option<(f64, f64, f64)> {
        let size = self.window.as_ref()?.inner_size();
        let (ww, wh) = (f64::from(size.width), f64::from(size.height));
        let scale = (ww / f64::from(WIDTH)).min(wh / f64::from(HEIGHT));
        if scale <= 0.0 {
            return None;
        }
        let (dw, dh) = (f64::from(WIDTH) * scale, f64::from(HEIGHT) * scale);
        Some(((ww - dw).trunc() / 2.0, (wh - dh).trunc() / 2.0, scale))
    }

    /// A window position on the 640x480 screen.
    fn to_screen(&self, p: PhysicalPosition<f64>) -> Option<(i32, i32)> {
        let (ox, oy, scale) = self.layout()?;
        Some((((p.x - ox) / scale) as i32, ((p.y - oy) / scale) as i32))
    }

    /// The 640x480 screen scaled to the window (nearest pixel, kept in
    /// proportion, black bars).
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
        let scale = (ww as f64 / f64::from(WIDTH)).min(wh as f64 / f64::from(HEIGHT));
        let (dw, dh) = (
            (f64::from(WIDTH) * scale) as usize,
            (f64::from(HEIGHT) * scale) as usize,
        );
        let (ox, oy) = ((ww - dw) / 2, (wh - dh) / 2);
        buf.fill(0);
        let xs: Vec<usize> = (0..dw)
            .map(|x| (x * WIDTH as usize / dw.max(1)).min(WIDTH as usize - 1))
            .collect();
        for y in 0..dh {
            let sy = (y * HEIGHT as usize / dh.max(1)).min(HEIGHT as usize - 1);
            let src = &self.screen[sy * WIDTH as usize..(sy + 1) * WIDTH as usize];
            let row = &mut buf[(oy + y) * ww + ox..(oy + y) * ww + ox + dw];
            for (d, &sx) in row.iter_mut().zip(&xs) {
                let [r, g, b] = atlantis2_engine::png::rgb565_to_rgb(src[sx]);
                *d = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
            }
        }
        let _ = buf.present();
    }
}

impl ApplicationHandler for A2App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title("Atlantis II - Atlantis Native")
            .with_inner_size(LogicalSize::new(WIDTH * 2, HEIGHT * 2));
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
            if self.locked {
                self.motion.0 += dx;
                self.motion.1 += dy;
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let in_menu = self.engine.loop_type == LoopType::Menu;
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
            WindowEvent::CursorMoved { position, .. } => {
                if let Some((x, y)) = self.to_screen(position) {
                    self.engine.pointer(x, y);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } if in_menu => {
                // The click flag (@ 0x006569c9) and the held flag
                // (@ 0x006569ca).
                if state == ElementState::Pressed {
                    let now = self.started.elapsed().as_secs_f64();
                    self.engine.click(now);
                } else {
                    self.engine.release();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                if !self.locked {
                    self.grab(true);
                } else if button == MouseButton::Left {
                    // The click flag (@ 0x006569c9) is the left button's
                    // (input callback FUN_0040c510); the right one opens
                    // the bag.
                    let now = self.started.elapsed().as_secs_f64();
                    self.engine.click(now);
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
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now >= self.next_frame {
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

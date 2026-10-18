//! Atlantis Native: the desktop and web front end. It owns the window, turns
//! input into engine events, runs the engine task one step per displayed
//! frame and shows its 640x480 frames scaled to the window, with the HD
//! layer of the frames that have one in the HD mode
//! ([`atlantis_engine::hd`]). It also runs the port's options menu (F10),
//! and on the desktop the game library's screen ([`launcher`]), where the
//! player chooses the edition to play, and now and then at start-up the
//! support screen ([`support`]); the running game can be stopped and
//! another started without leaving the program.

use std::num::NonZeroU32;
use std::rc::Rc;

use atlantis_data::Game;
use atlantis_engine::audio::Mixer;
use atlantis_engine::hd::{self, tile_for_output, HdCompositor, HdFrame, HdText};
use atlantis_engine::options_menu::{MenuAction, MenuValues, OptionsMenu};
use atlantis_engine::persist::{Graphics, Settings};
use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::sys::{Input, Key as EngineKey, MouseButton as EngineButton, Runner, Sys};
use atlantis_engine::warp::{SCREEN_H, SCREEN_W};
use atlantis_engine::{Enhancements, SaveStore, Start};
use web_time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Window, WindowId};

mod audio_out;
#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
pub mod dock_icon;
pub mod guide;
#[cfg(not(target_arch = "wasm32"))]
pub mod launcher;
#[cfg(not(target_arch = "wasm32"))]
pub mod library;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
pub mod setup;
#[cfg(not(target_arch = "wasm32"))]
pub mod support;
#[cfg(not(target_arch = "wasm32"))]
pub mod watchdog;
#[cfg(target_arch = "wasm32")]
mod web;

const W: usize = SCREEN_W;
const H: usize = SCREEN_H;

/// The engine's pace: one step per frame of 1/60 s. The original's loops
/// are not paced; ScummVM's port waits for the display.
const FRAME: Duration = Duration::from_micros(16_667);
/// An engine step longer than this is logged.
const SLOW_STEP: Duration = Duration::from_millis(100);

/// Where the 640x480 picture sits in the window.
#[derive(Clone, Copy, Default)]
struct Viewport {
    ox: f64,
    oy: f64,
    scale: f64,
}

impl Viewport {
    fn to_screen(self, x: f64, y: f64) -> (i32, i32) {
        if self.scale <= 0.0 {
            return (0, 0);
        }
        let sx = ((x - self.ox) / self.scale).floor() as i32;
        let sy = ((y - self.oy) / self.scale).floor() as i32;
        (sx.clamp(0, W as i32 - 1), sy.clamp(0, H as i32 - 1))
    }

    fn to_window(self, x: i32, y: i32) -> (f64, f64) {
        (
            self.ox + (f64::from(x) + 0.5) * self.scale,
            self.oy + (f64::from(y) + 0.5) * self.scale,
        )
    }
}

/// Marks the event-loop thread idle again when a handler returns.
#[cfg(not(target_arch = "wasm32"))]
struct WatchdogLeave;

#[cfg(not(target_arch = "wasm32"))]
impl Drop for WatchdogLeave {
    fn drop(&mut self) {
        watchdog::leave();
    }
}

/// The largest HD tile size: 4 times 640x480 on desktop, twice on the web
/// (one thread renders it there).
#[cfg(not(target_arch = "wasm32"))]
const HD_MAX_TILE: usize = hd::MAX_TILE;
/// Widescreen: at most this many tile columns on each side (a 2.4:1
/// window).
const HD_MAX_SIDE_TILES: usize = 16;
#[cfg(target_arch = "wasm32")]
const HD_MAX_TILE: usize = 32;

/// Opens the project's Buy Me a Coffee page in the browser.
pub fn open_support_page() {
    #[cfg(not(target_arch = "wasm32"))]
    setup::open_url(guide::SUPPORT_URL);
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(guide::SUPPORT_URL, "_blank");
    }
}

/// The port's version and the commit it was built from.
pub fn version_string() -> String {
    let commit = env!("ATLANTIS_NATIVE_COMMIT");
    if commit.is_empty() {
        format!("Atlantis Native {}", env!("CARGO_PKG_VERSION"))
    } else {
        format!("Atlantis Native {} ({commit})", env!("CARGO_PKG_VERSION"))
    }
}

pub struct App {
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    sys: Rc<Sys>,
    runner: Option<Runner>,
    /// Shown without a game: a test pattern.
    pattern: Vec<u16>,
    start: Instant,
    next_frame: Instant,
    viewport: Viewport,
    /// Source column of each window column, for the current width.
    col_map: Vec<usize>,
    /// The pointer lock as applied to the window.
    locked: bool,
    /// The window's title: the port and the game's release.
    title: String,
    /// The window's icon: the player's atlantis.exe icon.
    icon: Option<atlantis_data::exe_icon::Icon>,
    /// The lock fell back to confining the pointer (Windows, X11): the
    /// system pointer is then kept at the centre of the window.
    confined: bool,
    /// The window has the keyboard focus.
    focused: bool,
    /// Raw device motion arrives (desktop): window cursor motion is then
    /// not used for the relative look.
    raw_motion: bool,
    last_cursor: Option<(f64, f64)>,
    /// Fractions of the relative motion not yet sent.
    rel_acc: (f64, f64),
    audio: Option<cpal::Stream>,
    /// The browser starts sound only after a user gesture.
    audio_pending: bool,
    /// The port's options menu (F10); the game is paused while it is open,
    /// since `paused_at`.
    menu: OptionsMenu,
    menu_font: Option<SubtitleFont>,
    menu_frame: Vec<u16>,
    /// The game library's screen (desktop).
    launcher_frame: Vec<u16>,
    paused_at: Option<Instant>,
    graphics: Graphics,
    /// Where the graphics mode is saved (`settings.ini`).
    saves: Option<Rc<dyn SaveStore>>,
    /// The HD mode's composition, and the options menu's text drawn sharp.
    compositor: HdCompositor,
    menu_text: HdText,
    /// The game library and its screen (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    session: Option<session::Session>,
    /// The support screen, while it is shown (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    support: Option<support::Shown>,
}

impl App {
    /// A front end for `runner` (the engine task on `sys`), or a test
    /// pattern without one.
    pub fn new(sys: Rc<Sys>, runner: Option<Runner>) -> App {
        let pattern = (0..W * H)
            .map(|i| (((i % W) * 31 / W) << 11 | ((i / W) * 63 / H) << 5) as u16)
            .collect();
        let now = Instant::now();
        App {
            window: None,
            surface: None,
            sys,
            runner,
            pattern,
            start: now,
            next_frame: now,
            viewport: Viewport::default(),
            col_map: Vec::new(),
            locked: false,
            title: "Atlantis Native".to_string(),
            icon: None,
            confined: false,
            focused: true,
            raw_motion: false,
            last_cursor: None,
            rel_acc: (0.0, 0.0),
            audio: None,
            audio_pending: true,
            menu: OptionsMenu::new(),
            menu_font: None,
            menu_frame: Vec::new(),
            launcher_frame: Vec::new(),
            paused_at: None,
            graphics: Graphics::Original,
            saves: None,
            compositor: HdCompositor::new(),
            menu_text: HdText::default(),
            #[cfg(not(target_arch = "wasm32"))]
            session: None,
            #[cfg(not(target_arch = "wasm32"))]
            support: None,
        }
    }

    /// The game's release (see [`atlantis_data::Edition`]): in the
    /// window's title and at the foot of the options menu, with the
    /// port's version.
    pub fn with_edition(mut self, edition: &atlantis_data::Edition) -> App {
        self.set_edition(Some(edition));
        self
    }

    /// [`App::with_edition`] on a running front end; `None` without a game.
    fn set_edition(&mut self, edition: Option<&atlantis_data::Edition>) {
        match edition {
            Some(edition) => {
                self.title = format!("Atlantis Native - {edition}");
                self.menu
                    .set_info(vec![format!("Game: {edition}"), version_string()]);
            }
            None => {
                self.title = "Atlantis Native".to_string();
                self.menu.set_info(vec![version_string()]);
            }
        }
        if let Some(w) = &self.window {
            w.set_title(&self.title);
        }
    }

    /// The window's icon, read from the player's `atlantis.exe` (the port
    /// distributes none of the game's data).
    fn window_icon(&self) -> Option<winit::window::Icon> {
        let i = self.icon.as_ref()?;
        winit::window::Icon::from_rgba(i.rgba.clone(), i.width, i.height).ok()
    }

    pub fn with_icon(mut self, icon: Option<atlantis_data::exe_icon::Icon>) -> App {
        self.icon = icon;
        self
    }

    /// [`App::with_icon`] on a running front end (the window's, the
    /// taskbar's and the Dock's icon change at once; without an icon they
    /// keep theirs).
    #[cfg(not(target_arch = "wasm32"))]
    fn set_icon(&mut self, icon: Option<atlantis_data::exe_icon::Icon>) {
        if icon.is_none() {
            return;
        }
        self.icon = icon;
        if let Some(w) = &self.window {
            w.set_window_icon(self.window_icon());
            #[cfg(target_os = "windows")]
            {
                use winit::platform::windows::WindowExtWindows;
                w.set_taskbar_icon(self.window_icon());
            }
        }
        #[cfg(target_os = "macos")]
        if self.window.is_some() {
            if let Some(icon) = &self.icon {
                dock_icon::set(icon);
            }
        }
    }

    /// `--widescreen`: the widescreen option on for this run (not saved).
    pub fn force_widescreen(&mut self) {
        let mut o = self.sys.hd_options();
        o.widescreen = true;
        self.sys.set_hd_options(o);
    }

    /// The graphics mode to start with, and the store its changes in the
    /// options menu are saved to (the HD options are read from it).
    pub fn with_graphics(mut self, graphics: Graphics, saves: Option<Rc<dyn SaveStore>>) -> App {
        self.set_graphics(graphics, saves);
        self
    }

    fn set_graphics(&mut self, graphics: Graphics, saves: Option<Rc<dyn SaveStore>>) {
        self.graphics = graphics;
        if let Some(s) = saves.as_ref().and_then(|s| Settings::load(s.as_ref())) {
            self.sys.set_hd_options(s.hd);
        }
        self.saves = saves;
    }

    /// Starts the engine task `runner` on `sys` in place of the game
    /// running, if any (see [`App::stop_game`]): its graphics mode and
    /// store, its release and icon in the window, its sound.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn install_game(
        &mut self,
        (sys, runner): (Rc<Sys>, Runner),
        graphics: Graphics,
        saves: Rc<dyn SaveStore>,
        edition: &atlantis_data::Edition,
        icon: Option<atlantis_data::exe_icon::Icon>,
    ) {
        self.stop_game();
        self.sys = sys;
        self.runner = Some(runner);
        self.set_graphics(graphics, Some(saves));
        self.set_edition(Some(edition));
        self.set_icon(icon);
        let now = Instant::now();
        self.start = now;
        self.next_frame = now;
        if let Some(w) = self.window.clone() {
            // The game draws its own cursors.
            w.set_cursor_visible(false);
            self.start_audio();
            w.request_redraw();
        }
    }

    /// Stops the game running, if any: its engine task is dropped (the
    /// engine runs on this thread; its worker threads are scoped to each
    /// frame, so none is left), its sound stream closed, the pointer
    /// freed and the options menu closed. The front end is then without a
    /// game, on a blank system.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stop_game(&mut self) {
        if self.runner.take().is_some() {
            log::info!("game stopped");
        }
        self.save_play_time();
        if let Some(s) = &mut self.session {
            s.play_clock.pause();
        }
        self.audio = None;
        self.audio_pending = true;
        if self.menu.is_open() {
            self.menu.close();
        }
        self.paused_at = None;
        self.sys = Sys::new(Mixer::new());
        self.saves = None;
        self.locked = false;
        self.confined = false;
        self.raw_motion = false;
        self.last_cursor = None;
        self.rel_acc = (0.0, 0.0);
        self.compositor = HdCompositor::new();
        self.menu_text = HdText::default();
        self.menu_frame.clear();
        self.set_edition(None);
        if let Some(w) = &self.window {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
            w.set_cursor_visible(true);
            w.request_redraw();
        }
    }

    /// Whether a game is running (not the game library's screen).
    pub fn has_game(&self) -> bool {
        self.runner.is_some()
    }

    /// Opens the options menu: the game pauses (its clock and its sound),
    /// the system pointer is free.
    fn open_menu(&mut self) {
        if self.menu.is_open() {
            return;
        }
        self.menu.open();
        self.paused_at = Some(Instant::now());
        // The time in the menu is not play.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(s) = &mut self.session {
            s.play_clock.pause();
        }
        self.sys.mixer.set_paused(true);
        if let Some(w) = &self.window {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
            w.set_cursor_visible(true);
            w.request_redraw();
        }
        self.locked = false;
        self.last_cursor = None;
    }

    /// The menu closed: the game goes on where it was.
    fn close_menu(&mut self) {
        self.menu.close();
        if let Some(t) = self.paused_at.take() {
            let now = Instant::now();
            self.start += now.duration_since(t);
            self.next_frame = now;
        }
        self.sys.mixer.set_paused(false);
        if let Some(w) = &self.window {
            if self.runner.is_some() {
                w.set_cursor_visible(false);
            }
        }
        // The engine's pointer lock is applied again.
        self.locked = false;
        self.apply_pointer();
    }

    fn menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::None => {}
            MenuAction::Close => self.close_menu(),
            MenuAction::SetGraphics(g) => {
                self.graphics = g;
                log::info!("graphics: {}", g.name());
                if let Some(saves) = &self.saves {
                    let mut s = Settings::load(saves.as_ref()).unwrap_or_default();
                    s.graphics = g;
                    s.save(saves.as_ref());
                }
            }
            MenuAction::SetHd(o) => {
                // The engine renders the view again at the next frame, the
                // composition takes them at the next redraw.
                self.sys.set_hd_options(o);
                log::info!("HD options: {o:?}");
                if let Some(saves) = &self.saves {
                    let mut s = Settings::load(saves.as_ref()).unwrap_or_default();
                    s.hd = o;
                    s.save(saves.as_ref());
                }
            }
            MenuAction::SetEnhancements(x) => {
                // The engine reads them where it uses them: in force at once
                // (the music mode from the next track).
                self.sys.set_enhancements(x);
                log::info!("enhancements: {x:?}");
                if let Some(saves) = &self.saves {
                    let mut s = Settings::load(saves.as_ref()).unwrap_or_default();
                    s.enhancements = x;
                    s.save(saves.as_ref());
                }
            }
            MenuAction::OpenLibrary => {
                // (the web build has no game library: no such entry)
                #[cfg(not(target_arch = "wasm32"))]
                self.show_launcher(None);
            }
            MenuAction::OpenSupport => open_support_page(),
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn menu_values(&self) -> MenuValues {
        MenuValues {
            graphics: self.graphics,
            hd: self.sys.hd_options(),
            enhancements: self.sys.enhancements(),
        }
    }

    fn start_audio(&mut self) {
        if self.audio_pending {
            self.audio_pending = false;
            self.audio = audio_out::start(self.sys.mixer.clone());
        }
    }

    /// Runs the engine to its next frame when it is due; false once the
    /// engine has ended.
    fn step(&mut self) -> bool {
        if self.menu.is_open() {
            return true; // paused
        }
        let Some(runner) = &mut self.runner else {
            return true;
        };
        let now = Instant::now();
        // The browser paces the frames itself (requestAnimationFrame, one
        // RedrawRequested per screen refresh): a timer before the redraw
        // made the two delays add up and dropped frames.
        #[cfg(not(target_arch = "wasm32"))]
        if now < self.next_frame {
            return true;
        }
        self.next_frame += FRAME;
        if self.next_frame < now {
            self.next_frame = now + FRAME; // fell behind: do not catch up
        }
        let ms = now.duration_since(self.start).as_millis() as u64;
        let alive = runner.step(ms);
        let took = now.elapsed();
        #[cfg(not(target_arch = "wasm32"))]
        self.tick_play_time(now);
        if took > SLOW_STEP {
            // A step this long keeps the window from answering the system
            // (Windows closes a window that does not for ~5 s).
            log::warn!("slow engine step: {} ms at {ms} ms", took.as_millis());
        }
        self.apply_pointer();
        alive
    }

    /// The engine's pointer requests: lock (relative mode) and moves.
    fn apply_pointer(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let want = self.sys.mouse_locked();
        if want != self.locked {
            self.locked = want;
            self.last_cursor = None;
            if want {
                self.grab();
            } else {
                self.confined = false;
                let _ = window.set_cursor_grab(CursorGrabMode::None);
            }
        }
        // Confined instead of locked (Windows has no lock): the system
        // pointer still moves while the view turns on raw motion, reaches
        // the window's edge and a click lands on the title bar or outside.
        // Keep it at the centre while the view looks (the raw motion the
        // look reads is not affected by moving the pointer).
        #[cfg(not(target_arch = "wasm32"))]
        if self.locked && self.confined && self.focused {
            let size = window.inner_size();
            let centre = (f64::from(size.width / 2), f64::from(size.height / 2));
            if window
                .set_cursor_position(PhysicalPosition::new(centre.0, centre.1))
                .is_ok()
            {
                // Without raw motion the look reads the pointer's motion:
                // the move back to the centre is not a motion.
                self.last_cursor = Some(centre);
            }
        }
        if let Some((x, y)) = self.sys.take_warp_request() {
            if !self.locked {
                let (wx, wy) = self.viewport.to_window(x, y);
                let _ = window.set_cursor_position(PhysicalPosition::new(wx, wy));
                self.last_cursor = Some((wx, wy));
            }
        }
    }

    /// Takes the pointer for the relative look: locked where the system
    /// can, confined to the window otherwise.
    fn grab(&mut self) {
        let Some(window) = &self.window else { return };
        self.confined = false;
        if window.set_cursor_grab(CursorGrabMode::Locked).is_err() {
            match window.set_cursor_grab(CursorGrabMode::Confined) {
                Ok(()) => self.confined = true,
                Err(e) => log::debug!("pointer lock: {e}"),
            }
        }
    }

    fn relative(&mut self, dx: f64, dy: f64) {
        self.rel_acc.0 += dx;
        self.rel_acc.1 += dy;
        let (ix, iy) = (self.rel_acc.0.trunc(), self.rel_acc.1.trunc());
        self.rel_acc.0 -= ix;
        self.rel_acc.1 -= iy;
        if ix != 0.0 || iy != 0.0 {
            self.sys.push_input(Input::MouseDelta {
                dx: ix as i32,
                dy: iy as i32,
            });
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.inner_size();
        let (Some(sw), Some(sh)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        surface.resize(sw, sh).unwrap();
        let mut buf = surface.buffer_mut().unwrap();
        // The support screen or the game library's screen, drawn in
        // `launcher_frame`.
        #[cfg(not(target_arch = "wasm32"))]
        let launcher = support::draw_support(self.support.as_ref(), &mut self.launcher_frame)
            || session::draw_launcher(
                self.session.as_ref(),
                &mut self.launcher_frame,
                &mut self.menu_font,
            );
        #[cfg(target_arch = "wasm32")]
        let launcher = false;
        let (sw, sh) = (size.width as usize, size.height as usize);
        // Nearest scaling keeping the 4:3 aspect, black bars.
        let scale = (sw as f64 / W as f64).min(sh as f64 / H as f64);
        let (dw, dh) = ((W as f64 * scale) as usize, (H as f64 * scale) as usize);
        let (ox, oy) = ((sw - dw) / 2, (sh - dh) / 2);
        self.viewport = Viewport {
            ox: ox as f64,
            oy: oy as f64,
            scale,
        };
        // The HD tile the engine draws its next frames at.
        let tile = match self.graphics {
            Graphics::Hd if !launcher => tile_for_output(sw, sh, HD_MAX_TILE),
            _ => None,
        };
        self.sys.set_hd_tile(tile);
        // Widescreen: the tile columns of panorama that fill the black
        // bars on each side (a tile is dw / 40 output pixels wide).
        let side = match tile {
            Some(_) if self.sys.hd_options().widescreen && dw > 0 => {
                (ox * 40).div_ceil(dw).min(HD_MAX_SIDE_TILES)
            }
            _ => 0,
        };
        self.sys.set_hd_side_tiles(side);
        let screen = self.sys.screen.borrow();
        let mut frame: &[u16] = if launcher {
            &self.launcher_frame
        } else if self.runner.is_some() {
            &screen
        } else {
            &self.pattern
        };
        let hd_mode = tile.is_some();
        if self.menu.is_open() {
            self.menu_frame.clear();
            self.menu_frame.extend_from_slice(frame);
            let values = MenuValues {
                graphics: self.graphics,
                hd: self.sys.hd_options(),
                enhancements: self.sys.enhancements(),
            };
            let font = self.menu_font.get_or_insert_with(SubtitleFont::new);
            let text = hd_mode.then_some(&mut self.menu_text);
            self.menu
                .draw_recording(&mut self.menu_frame, font, values, text);
            frame = &self.menu_frame;
        }
        buf.fill(0);
        if hd_mode {
            // HD: the layer, the interface art upscaled, the text sharp.
            let running = self.runner.is_some();
            let layer = self.sys.hd.borrow();
            let text = self.sys.hd_text.borrow();
            let video = self.sys.hd_video.borrow();
            let mut texts: Vec<&HdText> = Vec::new();
            if running && self.sys.hd_text_shown() {
                texts.push(&text);
            }
            if self.menu.is_open() {
                texts.push(&self.menu_text);
            }
            let f = HdFrame {
                screen: frame,
                layer: (running && self.sys.hd_shown()).then_some(&*layer),
                texts: &texts,
                photo: running && self.sys.photo_shown(),
                video_pano: Some(&*video).filter(|v| running && v.active),
            };
            self.compositor
                .compose(f, self.sys.hd_options(), &mut buf[..], sw, ox, oy, dw, dh);
            drop((layer, text, video));
            buf.present().unwrap();
            return;
        }
        // RGB565 -> XRGB8888 table and source column per window column,
        // built once (the per-pixel arithmetic was the cost in the browser).
        let lut = hd::rgb565_to_888();
        if self.col_map.len() != dw {
            self.col_map = (0..dw).map(|x| x * W / dw).collect();
        }
        let mut prev_src = usize::MAX;
        for y in 0..dh {
            let src_y = y * H / dh;
            let start = (oy + y) * sw + ox;
            if src_y == prev_src {
                // same source row as the line above: copy it
                buf.copy_within(start - sw..start - sw + dw, start);
                continue;
            }
            prev_src = src_y;
            let src_row = &frame[src_y * W..][..W];
            for (d, &x) in buf[start..start + dw].iter_mut().zip(&self.col_map) {
                *d = lut[usize::from(src_row[x])];
            }
        }
        buf.present().unwrap();
    }
}

fn engine_key(key: &Key) -> Option<EngineKey> {
    Some(match key {
        Key::Named(NamedKey::Escape) => EngineKey::Escape,
        Key::Named(NamedKey::Enter) => EngineKey::Enter,
        Key::Named(NamedKey::Space) => EngineKey::Space,
        Key::Named(NamedKey::Backspace) => EngineKey::Backspace,
        Key::Named(NamedKey::ArrowLeft) => EngineKey::Left,
        Key::Named(NamedKey::ArrowRight) => EngineKey::Right,
        Key::Named(NamedKey::ArrowUp) => EngineKey::Up,
        Key::Named(NamedKey::ArrowDown) => EngineKey::Down,
        Key::Named(NamedKey::F1) => EngineKey::F(1),
        Key::Named(NamedKey::F2) => EngineKey::F(2),
        Key::Named(NamedKey::F3) => EngineKey::F(3),
        Key::Named(NamedKey::F4) => EngineKey::F(4),
        Key::Named(NamedKey::F5) => EngineKey::F(5),
        Key::Named(NamedKey::F6) => EngineKey::F(6),
        Key::Named(NamedKey::F7) => EngineKey::F(7),
        Key::Named(NamedKey::F8) => EngineKey::F(8),
        Key::Named(NamedKey::F9) => EngineKey::F(9),
        Key::Named(NamedKey::F10) => EngineKey::F(10),
        Key::Named(NamedKey::F11) => EngineKey::F(11),
        Key::Named(NamedKey::F12) => EngineKey::F(12),
        Key::Character(s) => EngineKey::Char(s.chars().next()?),
        _ => return None,
    })
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        #[cfg(not(target_arch = "wasm32"))]
        watchdog::enter(watchdog::Phase::Resumed);
        #[cfg(not(target_arch = "wasm32"))]
        let _guard = WatchdogLeave;
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_window_icon(self.window_icon())
            .with_inner_size(LogicalSize::new(W as f64 * 1.5, H as f64 * 1.5));
        // Windows takes the taskbar and Alt+Tab icon separately.
        #[cfg(target_os = "windows")]
        let attrs = {
            use winit::platform::windows::WindowAttributesExtWindows;
            attrs.with_taskbar_icon(self.window_icon())
        };
        #[cfg(target_arch = "wasm32")]
        let attrs = {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs.with_append(true)
        };
        let window = Rc::new(el.create_window(attrs).unwrap());
        // The game draws its own cursors.
        if self.runner.is_some() {
            window.set_cursor_visible(false);
        }
        let context = softbuffer::Context::new(window.clone()).unwrap();
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
        #[cfg(target_os = "macos")]
        if let Some(icon) = &self.icon {
            dock_icon::set(icon);
        }
        self.window = Some(window);
        // A desktop can open the sound device at once.
        #[cfg(not(target_arch = "wasm32"))]
        if self.runner.is_some() {
            self.start_audio();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        #[cfg(not(target_arch = "wasm32"))]
        watchdog::enter(watchdog::Phase::AboutToWait);
        #[cfg(not(target_arch = "wasm32"))]
        let _guard = WatchdogLeave;
        // In the browser the redraw loop keeps itself going (see
        // RedrawRequested).
        #[cfg(target_arch = "wasm32")]
        el.set_control_flow(ControlFlow::Wait);
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.runner.is_none() || self.menu.is_open() {
                // (the menu redraws on its events)
                el.set_control_flow(ControlFlow::Wait);
                return;
            }
            if Instant::now() >= self.next_frame {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        #[cfg(not(target_arch = "wasm32"))]
        watchdog::enter(watchdog::Phase::DeviceEvent);
        #[cfg(not(target_arch = "wasm32"))]
        let _guard = WatchdogLeave;
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            // Raw motion: what the relative look reads, in pixels of the
            // 640x480 screen the original ran fullscreen in.
            if self.locked {
                self.raw_motion = true;
                self.relative(dx, dy);
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        #[cfg(not(target_arch = "wasm32"))]
        watchdog::enter(watchdog::Phase::WindowEvent);
        #[cfg(not(target_arch = "wasm32"))]
        let _guard = WatchdogLeave;
        #[cfg(not(target_arch = "wasm32"))]
        if self.support_event(el, &event) || self.launcher_event(el, &event) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.sys.push_input(Input::Quit);
                el.exit();
            }
            WindowEvent::RedrawRequested => {
                #[cfg(not(target_arch = "wasm32"))]
                watchdog::enter(watchdog::Phase::EngineStep);
                if !self.step() {
                    el.exit();
                    return;
                }
                #[cfg(not(target_arch = "wasm32"))]
                watchdog::enter(watchdog::Phase::Redraw);
                self.redraw();
                #[cfg(target_arch = "wasm32")]
                if self.runner.is_some() {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            // The system drops a confined pointer on a focus change, and a
            // confinement keeps the window's old rectangle after a move or
            // a resize: take it again.
            WindowEvent::Focused(f) => {
                self.focused = f;
                if f && self.locked {
                    self.grab();
                }
            }
            WindowEvent::Moved(_) => {
                if self.locked && self.focused {
                    self.grab();
                }
            }
            WindowEvent::Resized(_) => {
                if self.locked && self.focused {
                    self.grab();
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x, position.y);
                if self.menu.is_open() {
                    let (sx, sy) = self.viewport.to_screen(x, y);
                    self.menu.pointer(sx, sy);
                    self.last_cursor = Some((x, y));
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }
                if self.locked {
                    // Without raw device motion (a browser without pointer
                    // lock), the cursor's own motion turns the view.
                    if !self.raw_motion {
                        if let Some((lx, ly)) = self.last_cursor {
                            let s = self.viewport.scale.max(1e-6);
                            self.relative((x - lx) / s, (y - ly) / s);
                        }
                    }
                } else {
                    let (sx, sy) = self.viewport.to_screen(x, y);
                    self.sys.push_input(Input::MouseMove { x: sx, y: sy });
                }
                self.last_cursor = Some((x, y));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.menu.is_open() {
                    if let (MouseButton::Left, ElementState::Pressed, Some((x, y))) =
                        (button, state, self.last_cursor)
                    {
                        let (sx, sy) = self.viewport.to_screen(x, y);
                        let action = self.menu.click(sx, sy, self.menu_values());
                        self.menu_action(action);
                    }
                    return;
                }
                self.start_audio();
                let button = match button {
                    MouseButton::Left => EngineButton::Left,
                    MouseButton::Right => EngineButton::Right,
                    _ => return,
                };
                let (x, y) = self.sys.mouse();
                self.sys.push_input(match state {
                    ElementState::Pressed => Input::MouseDown { button, x, y },
                    ElementState::Released => Input::MouseUp { button, x, y },
                });
                // A browser grants the pointer lock only on a user gesture.
                if self.locked {
                    if let Some(w) = &self.window {
                        let _ = w.set_cursor_grab(CursorGrabMode::Locked);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = engine_key(&event.logical_key);
                let pressed = event.state == ElementState::Pressed;
                if self.menu.is_open() {
                    // The menu takes every key (with the repeats, to move).
                    if let (Some(k), true) = (key, pressed) {
                        let action = self.menu.key(k, self.menu_values());
                        self.menu_action(action);
                    }
                    return;
                }
                if event.repeat {
                    return;
                }
                if key == Some(EngineKey::F(10)) {
                    // The port's options menu: not the engine's key.
                    if pressed {
                        self.open_menu();
                    }
                    return;
                }
                if let Some(k) = key {
                    self.sys.push_input(match event.state {
                        ElementState::Pressed => Input::KeyDown(k),
                        ElementState::Released => Input::KeyUp(k),
                    });
                }
            }
            _ => {}
        }
    }
}

/// The options of a run with its saves in `saves`: the subtitles and the
/// enhancements read from the store's `settings.ini` (written with the
/// defaults when there is none); `original` turns the enhancements off.
pub fn options_with_settings(
    saves: Rc<dyn SaveStore>,
    original: bool,
    start: Start,
) -> atlantis_engine::Options {
    let stored = Settings::load(saves.as_ref());
    let settings = stored.unwrap_or_default();
    if stored.is_none() {
        settings.save(saves.as_ref());
    }
    atlantis_engine::Options {
        enhancements: if original {
            Enhancements::original()
        } else {
            settings.enhancements
        },
        subtitles: settings.subtitles,
        saves,
        start,
        ..atlantis_engine::Options::default()
    }
}

/// The engine task for `game`, from the new-game start.
pub fn start_game(game: Game, options: atlantis_engine::Options) -> (Rc<Sys>, Runner) {
    log::info!(
        "{}: {} archive members",
        game.exe.build.name,
        game.big.len()
    );
    let sys = Sys::new(Mixer::new());
    let runner = Runner::new(
        sys.clone(),
        atlantis_engine::run(sys.clone(), game, options),
    );
    (sys, runner)
}

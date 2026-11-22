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
/// an install or the extracted cabinets, "DVD, installed".
pub fn label(path: &Path) -> Result<String, atlantis3_data::atlantis5::OpenError> {
    use atlantis3_data::atlantis5::Form;
    let src = Atlantis5Source::open(path)?;
    let medium = match src.form() {
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
    Ok(format!(
        "{} (2006), {medium}, {lang_name}",
        crate::setup::GameKind::Atlantis5.name()
    ))
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

struct A5App {
    play: Play,
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

    fn frame(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64().min(0.25);
        self.last = now;
        self.play.update(dt);
        if self.play.quit_requested() {
            el.exit();
            return;
        }
        // A menu frees the mouse (the game draws its cursor); play holds
        // it. The system cursor stays hidden (ShowCursor(0), @ 0x0043c200).
        let menu = self.play.in_menu();
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
        self.play.render(&mut self.picture);
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
        self.last = Instant::now();
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.focused {
                #[allow(clippy::cast_possible_truncation)]
                self.play.look(delta.0 as f32, delta.1 as f32);
            }
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    // Escape (DIK 1): a film ends, or the in-game menu.
                    Key::Named(NamedKey::Escape) => self.play.escape(),
                    // Space (0x39): a film or a dialog ends.
                    Key::Named(NamedKey::Space) => self.play.space(),
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
            } => self.play.click(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => self.play.right_press(),
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
                    self.play.mouse_at(
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
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    let (src, label) = open(path)?;
    log::info!("{label}");
    let lang = language(&src);
    let audio = Audio::new();
    let out = {
        let a = audio.clone();
        crate::audio_out::start_with(move |buf, rate, channels| a.fill(buf, rate, channels))
    };
    let src: Arc<dyn atlantis_data::FileSource> = Arc::new(src);
    let mut play = Play::new(src, &lang, Some(audio))?;
    // The saves (`saves/autoslot`, `slot_<n>`) in the player's data
    // folder; their dates in local time.
    play.set_save_dir(Some(saves_dir()));
    play.menus.utc_offset = crate::a2::utc_offset_now;
    play.boot();
    let mut app = A5App {
        play,
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
    };
    el.run_app_on_demand(&mut app)
        .map_err(|e| format!("The window failed: {e}"))?;
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

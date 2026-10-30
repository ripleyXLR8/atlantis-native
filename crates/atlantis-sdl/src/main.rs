//! Atlantis Native for handhelds and PortMaster:
//! `atlantis-native-sdl [--original] [--new-game] [--input-test]
//! [--windowed] [<game dir>]`.
//!
//! The SDL2 front end of the first game, for devices without a desktop
//! (SDL draws through KMS/DRM) as well as under Wayland or X11. The
//! engine's 640x480 RGB565 frame goes to an SDL texture, scaled to the
//! screen keeping 4:3; the sound comes from an SDL audio callback pulling
//! the engine's mixer; the gamepad stands in for the mouse (see
//! `atlantis_sdl::buttons` for its buttons): the sticks or the D-pad move
//! the pointer, or turn the panoramic view while the engine locks the
//! mouse; an on-screen keyboard types the players' names. A keyboard and a
//! mouse work too. Select (or F10 / O on a keyboard) opens the port's
//! options menu (the enhancements and the bug fixes, kept in
//! `settings.ini`).
//!
//! Based on Klaas Van den Broeck's (@Klaasvdb) R36S front end,
//! github.com/Klaasvdb/atlantis-native-handheld (GPL-3.0-or-later), brought
//! into Atlantis Native with his agreement.
//!
//! Without a game folder on the command line, `gamedata` next to the
//! program is used. The players, the checkpoints and `settings.ini` are
//! kept in `saves/<build>` next to the program (or in `$ATLANTIS_SAVES`).
//! `controls.ini` and `gamecontrollerdb.txt` next to the program are read
//! (see `atlantis_sdl::config`).
//!
//! `--input-test` shows nothing but prints the gamepad's buttons, axes and
//! hats, to write a `controls.ini` for a pad SDL has no mapping for.

mod pad;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::options_menu::{MenuAction, MenuValues, OptionsMenu};
use atlantis_engine::persist::DirStore;
use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::warp::{SCREEN_H, SCREEN_W};
use atlantis_engine::{BugFix, Enhancements, Graphics, Options, SaveStore, Settings, Start};
use atlantis_sdl::buttons::Action;
use atlantis_sdl::motion::{Motion, Step};
use sdl2::audio::{AudioCallback, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

use pad::Pad;

/// The engine's pace: one step per frame of 1/60 s (as the desktop port).
const FRAME: Duration = Duration::from_micros(16_667);

/// The audio thread's view of the engine's mixer.
struct MixerOut {
    mixer: Mixer,
    rate: u32,
    channels: usize,
}

impl AudioCallback for MixerOut {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        self.mixer.fill(out, self.rate, self.channels);
    }
}

/// The folder the program is in (the port's folder).
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn fail(text: &str) -> ! {
    log::error!("{text}");
    eprintln!("{text}");
    std::process::exit(1);
}

/// The options of a run with its saves in `saves` (as the desktop's
/// `options_with_settings`).
fn options(saves: Rc<dyn SaveStore>, original: bool, start: Start) -> Options {
    let stored = Settings::load(saves.as_ref());
    let settings = stored.unwrap_or_default();
    if stored.is_none() {
        settings.save(saves.as_ref());
    }
    Options {
        enhancements: if original {
            Enhancements::original()
        } else {
            settings.enhancements
        },
        subtitles: settings.subtitles,
        saves,
        start,
        ..Options::default()
    }
}

/// A keyboard key for the engine (a keyboard on the device's USB port, or
/// a desktop while testing).
fn engine_key(k: Keycode) -> Option<Key> {
    Some(match k {
        Keycode::Escape => Key::Escape,
        Keycode::Return | Keycode::KpEnter => Key::Enter,
        Keycode::Space => Key::Space,
        Keycode::Backspace => Key::Backspace,
        Keycode::Left => Key::Left,
        Keycode::Right => Key::Right,
        Keycode::Up => Key::Up,
        Keycode::Down => Key::Down,
        Keycode::F1 => Key::F(1),
        Keycode::F2 => Key::F(2),
        Keycode::F3 => Key::F(3),
        Keycode::F4 => Key::F(4),
        Keycode::F5 => Key::F(5),
        Keycode::F6 => Key::F(6),
        Keycode::F7 => Key::F(7),
        Keycode::F8 => Key::F(8),
        Keycode::F9 => Key::F(9),
        Keycode::F10 => Key::F(10),
        Keycode::F11 => Key::F(11),
        Keycode::F12 => Key::F(12),
        k => {
            let name = k.name();
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c.to_ascii_lowercase()),
                _ => return None,
            }
        }
    })
}

/// The port's options menu (the enhancements and the bug fixes); the game
/// is paused while it is open.
struct PortMenu {
    menu: OptionsMenu,
    font: Option<SubtitleFont>,
    saves: Rc<dyn SaveStore>,
    /// When it opened (the game's clock skips the time in the menu).
    opened: Option<Instant>,
}

impl PortMenu {
    fn values(sys: &Sys) -> MenuValues {
        MenuValues {
            graphics: Graphics::Original,
            hd: sys.hd_options(),
            enhancements: sys.enhancements(),
            fullscreen: false,
        }
    }

    fn open(&mut self, sys: &Sys) {
        if self.menu.is_open() {
            return;
        }
        self.menu.open();
        self.opened = Some(Instant::now());
        sys.mixer.set_paused(true);
    }

    /// Closes the menu; how long it was open.
    fn close(&mut self, sys: &Sys) -> Duration {
        self.menu.close();
        sys.mixer.set_paused(false);
        self.opened.take().map_or(Duration::ZERO, |t| t.elapsed())
    }

    /// Applies `action`; how long the menu was open when it closed.
    fn act(&mut self, action: MenuAction, sys: &Sys) -> Option<Duration> {
        match action {
            MenuAction::Close => return Some(self.close(sys)),
            MenuAction::SetEnhancements(x) => {
                // In force at once; only what changed is written
                // (`--original` holds for the run without changing
                // `settings.ini`).
                let before = sys.enhancements();
                sys.set_enhancements(x);
                log::info!("enhancements: {x:?}");
                let mut s = Settings::load(self.saves.as_ref()).unwrap_or_default();
                s.enhancements = s.enhancements.with_changes(&before, &x);
                s.save(self.saves.as_ref());
            }
            // (no HD mode, full screen setting, library or browser here:
            // their entries are not shown)
            MenuAction::None
            | MenuAction::SetGraphics(_)
            | MenuAction::SetHd(_)
            | MenuAction::SetFullscreen(_)
            | MenuAction::OpenLibrary
            | MenuAction::OpenSupport => {}
        }
        None
    }

    fn draw(&mut self, frame: &mut [u16], sys: &Sys) {
        if self.menu.is_open() {
            let font = self.font.get_or_insert_with(SubtitleFont::new);
            self.menu.draw(frame, font, Self::values(sys));
        }
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut flag = |name: &str| match args.iter().position(|a| a == name) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    };
    let original = flag("--original");
    let new_game = flag("--new-game");
    let input_test = flag("--input-test");
    // A window instead of the full screen (trying the port on a desktop).
    let windowed = flag("--windowed");
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if args.len() > 1 || args.first().is_some_and(|a| a.starts_with('-')) {
        fail("usage: atlantis-native-sdl [--original] [--new-game] [--input-test] [--windowed] [<game dir>]");
    }
    log::info!("Atlantis Native {} (SDL)", env!("CARGO_PKG_VERSION"));

    let here = exe_dir();
    let sdl = sdl2::init().unwrap_or_else(|e| fail(&format!("SDL: {e}")));
    let pad_ctx = sdl
        .game_controller()
        .unwrap_or_else(|e| fail(&format!("SDL game controllers: {e}")));
    let joy_ctx = sdl
        .joystick()
        .unwrap_or_else(|e| fail(&format!("SDL joysticks: {e}")));
    let mut events = sdl
        .event_pump()
        .unwrap_or_else(|e| fail(&format!("SDL events: {e}")));
    if input_test {
        pad::input_test(&mut events, &joy_ctx);
        return;
    }
    let mut pad = Pad::open(&pad_ctx, &joy_ctx, &here);
    let mut motion = Motion::new(pad.controls.tuning());

    let dir = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| here.join("gamedata"));
    let game = Game::open(Arc::new(StdDir::new(&dir))).unwrap_or_else(|e| {
        fail(&format!(
            "{}: {e}\nCopy your own Atlantis: The Lost Tales (CD or GOG.com) there.",
            dir.display()
        ))
    });
    let edition = game.edition();
    log::info!("{}: {edition}", dir.display());
    let saves_root = std::env::var_os("ATLANTIS_SAVES")
        .map(PathBuf::from)
        .unwrap_or_else(|| here.join("saves"));
    let saves: Rc<dyn SaveStore> = Rc::new(DirStore::new(saves_root.join(game.exe.build.name)));
    let start = if new_game {
        Start::NewGame
    } else {
        Start::Boot
    };
    let opts = options(saves.clone(), original, start);

    let mut menu = PortMenu {
        menu: OptionsMenu::new(),
        font: None,
        saves,
        opened: None,
    };
    menu.menu.set_hd_entries(false);
    menu.menu.set_support_entry(false);
    menu.menu.set_info(vec![
        format!("Game: {edition}"),
        format!("Atlantis Native {}", env!("CARGO_PKG_VERSION")),
    ]);
    menu.menu
        .set_game(Some(&edition.to_string()), Some(BugFix::of_game(&game)));

    let sys = Sys::new(Mixer::new());
    let mut runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), game, opts));

    // The picture: full screen at the display's size, the 640x480 frame
    // scaled into it keeping 4:3.
    let video = sdl
        .video()
        .unwrap_or_else(|e| fail(&format!("SDL video: {e}")));
    log::info!("video driver: {}", video.current_video_driver());
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "nearest");
    let mut window = video.window("Atlantis", SCREEN_W as u32, SCREEN_H as u32);
    if !windowed {
        window.fullscreen_desktop();
    }
    let window = window
        .build()
        .unwrap_or_else(|e| fail(&format!("SDL window: {e}")));
    // The engine draws its own pointer into the frame.
    sdl.mouse().show_cursor(false);
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .unwrap_or_else(|e| fail(&format!("SDL renderer: {e}")));
    let _ = canvas.set_logical_size(SCREEN_W as u32, SCREEN_H as u32);
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::RGB565, SCREEN_W as u32, SCREEN_H as u32)
        .unwrap_or_else(|e| fail(&format!("SDL texture: {e}")));

    // The sound: whatever rate and channels the device takes, the mixer
    // resamples.
    let audio = sdl.audio().ok();
    let _device = audio.as_ref().and_then(|a| {
        let want = AudioSpecDesired {
            freq: Some(44100),
            channels: Some(2),
            samples: Some(1024),
        };
        let mixer = sys.mixer.clone();
        match a.open_playback(None, &want, |spec| {
            log::info!("audio output: {} Hz, {} channels", spec.freq, spec.channels);
            MixerOut {
                mixer,
                rate: spec.freq as u32,
                channels: usize::from(spec.channels),
            }
        }) {
            Ok(d) => {
                d.resume();
                Some(d)
            }
            Err(e) => {
                log::warn!("audio output: {e}");
                None
            }
        }
    });

    // The game's clock: the time since `t0`, without the time in the menu.
    let mut t0 = Instant::now();
    let mut next_frame = t0;
    let mut last_motion = t0;
    let mut locked = false;
    let mut menu_was_open = false;
    let mut quit_by: Option<Instant> = None;
    let mut pixels: Vec<u8> = vec![0; SCREEN_W * SCREEN_H * 2];
    let mut frame: Vec<u16> = vec![0; SCREEN_W * SCREEN_H];
    'run: loop {
        pad.buttons.in_menu = sys.in_menu();
        pad.buttons.options_open = menu.menu.is_open();
        let mut actions = Vec::new();
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => actions.push(Action::Quit),
                Event::KeyDown {
                    keycode: Some(k), ..
                } if menu.menu.is_open() => {
                    if let Some(k) = engine_key(k) {
                        actions.push(Action::MenuKey(k));
                    }
                }
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } => match engine_key(k) {
                    // The port's options menu: not the engine's key (but
                    // a game menu may be taking letters).
                    Some(Key::F(10) | Key::Char('o')) if !sys.in_menu() => {
                        actions.push(Action::OptionsMenu)
                    }
                    Some(k) => sys.push_input(Input::KeyDown(k)),
                    None => {}
                },
                Event::KeyUp {
                    keycode: Some(k), ..
                } => {
                    if let Some(k) = engine_key(k) {
                        sys.push_input(Input::KeyUp(k));
                    }
                }
                Event::MouseMotion {
                    x, y, xrel, yrel, ..
                } => {
                    if menu.menu.is_open() {
                        menu.menu.pointer(x, y);
                    } else {
                        sys.push_input(if locked {
                            Input::MouseDelta { dx: xrel, dy: yrel }
                        } else {
                            Input::MouseMove { x, y }
                        });
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } if menu.menu.is_open() => {
                    if mouse_btn == sdl2::mouse::MouseButton::Left {
                        let action = menu.menu.click(x, y, PortMenu::values(&sys));
                        if let Some(paused) = menu.act(action, &sys) {
                            t0 += paused;
                        }
                    }
                }
                Event::MouseButtonDown { mouse_btn, .. }
                | Event::MouseButtonUp { mouse_btn, .. } => {
                    let button = match mouse_btn {
                        sdl2::mouse::MouseButton::Left => MouseButton::Left,
                        sdl2::mouse::MouseButton::Right => MouseButton::Right,
                        _ => continue,
                    };
                    let (x, y) = sys.mouse();
                    sys.push_input(if matches!(event, Event::MouseButtonDown { .. }) {
                        Input::MouseDown { button, x, y }
                    } else {
                        Input::MouseUp { button, x, y }
                    });
                }
                e => actions.extend(pad.event(&e, &pad_ctx, &joy_ctx)),
            }
        }
        for action in actions {
            match action {
                Action::Mouse(button, down) => {
                    let (x, y) = sys.mouse();
                    sys.push_input(if down {
                        Input::MouseDown { button, x, y }
                    } else {
                        Input::MouseUp { button, x, y }
                    });
                }
                Action::Key(k, true) => sys.push_input(Input::KeyDown(k)),
                Action::Key(k, false) => sys.push_input(Input::KeyUp(k)),
                Action::OptionsMenu => {
                    if menu.menu.is_open() {
                        t0 += menu.close(&sys);
                    } else {
                        menu.open(&sys);
                        // The real mouse's pointer, to click in it.
                        sdl.mouse().set_relative_mouse_mode(false);
                        sdl.mouse().show_cursor(true);
                    }
                }
                Action::MenuKey(k) => {
                    if menu.menu.is_open() {
                        let action = menu.menu.key(k, PortMenu::values(&sys));
                        if let Some(paused) = menu.act(action, &sys) {
                            t0 += paused;
                        }
                    }
                }
                Action::Quit => {
                    if quit_by.is_none() {
                        sys.push_input(Input::Quit);
                        // The engine ends at its next frame; a second at most.
                        quit_by = Some(Instant::now() + Duration::from_secs(1));
                        if menu.menu.is_open() {
                            menu.close(&sys);
                        }
                    }
                }
            }
        }
        if quit_by.is_some_and(|t| Instant::now() > t) {
            break 'run;
        }

        let now = Instant::now();
        if menu.menu.is_open() {
            // Paused: the menu drawn over the last frame.
            if now >= next_frame {
                next_frame = now + FRAME;
                frame.copy_from_slice(&sys.screen.borrow());
                menu.draw(&mut frame, &sys);
                present(&mut frame, &mut pixels, &mut texture, &mut canvas);
            } else {
                std::thread::sleep((next_frame - now).min(Duration::from_millis(4)));
            }
            last_motion = now;
            menu_was_open = true;
            continue;
        }
        if menu_was_open {
            // Back from the menu: the engine's pointer again.
            menu_was_open = false;
            sdl.mouse().show_cursor(false);
            sdl.mouse().set_relative_mouse_mode(locked);
        }
        if now >= next_frame {
            next_frame += FRAME;
            if next_frame < now {
                next_frame = now + FRAME; // fell behind: do not catch up
            }
            // The pointer, once per engine frame: the engine's own moves
            // first, then the pad's for the time since the last frame.
            let want = sys.mouse_locked();
            if want != locked {
                locked = want;
                sdl.mouse().set_relative_mouse_mode(locked);
                motion.set_pointer(sys.mouse());
            }
            if let Some(p) = sys.take_warp_request() {
                motion.set_pointer(p);
            }
            pad.buttons.tick(pad.stick_x());
            let dt = now.duration_since(last_motion).as_secs_f32();
            last_motion = now;
            match motion.step(&pad.inputs(), dt, locked) {
                Step::Pointer(x, y) => sys.push_input(Input::MouseMove { x, y }),
                Step::Look(dx, dy) => sys.push_input(Input::MouseDelta { dx, dy }),
                Step::None => {}
            }

            let ms = now.duration_since(t0).as_millis() as u64;
            if !runner.step(ms) {
                break 'run;
            }
            let took = now.elapsed();
            if took > Duration::from_millis(100) {
                log::warn!("slow engine step: {} ms at {ms} ms", took.as_millis());
            }
            frame.copy_from_slice(&sys.screen.borrow());
            pad.buttons.osk.draw(&mut frame);
            present(&mut frame, &mut pixels, &mut texture, &mut canvas);
        } else {
            std::thread::sleep((next_frame - now).min(Duration::from_millis(4)));
        }
    }
    log::info!("bye");
}

/// Shows the 640x480 RGB565 frame.
fn present(
    frame: &mut [u16],
    pixels: &mut [u8],
    texture: &mut sdl2::render::Texture,
    canvas: &mut sdl2::render::WindowCanvas,
) {
    for (d, s) in pixels.as_chunks_mut::<2>().0.iter_mut().zip(frame.iter()) {
        *d = s.to_ne_bytes();
    }
    let _ = texture.update(None, pixels, SCREEN_W * 2);
    canvas.clear();
    let _ = canvas.copy(texture, None, None);
    canvas.present();
}

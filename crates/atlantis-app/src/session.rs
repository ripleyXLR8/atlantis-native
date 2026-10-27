//! The desktop's game sessions: the game library ([`crate::library`]),
//! its screen ([`crate::launcher`]) in the window, and the switch from one
//! game to another without leaving the program (the options menu's "Game
//! library..." entry stops the game and shows the screen). Atlantis II,
//! chosen in the library, runs in its own mode ([`crate::a2`]): the
//! session keeps its folder ([`Session::take_atlantis2`]), the event loop
//! ends, and the program runs it in its place.

use std::path::{Path, PathBuf};

use atlantis_data::Game;
use atlantis_engine::persist::{Graphics, Settings};
use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::warp::{SCREEN_H, SCREEN_W};
use atlantis_engine::Start;
use web_time::{Duration, Instant};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};

use crate::desktop::{self, Opened};
use crate::launcher::{Launcher, LauncherAction, LauncherEntry, LauncherKey};
use crate::library::Library;
use crate::support::PlayClock;
use crate::{options_with_settings, start_game, App};

/// How long after an automatic start Shift still opens the game library
/// (where the system reports a held key only once the window has the
/// focus).
const SHIFT_GRACE: Duration = Duration::from_millis(2000);

/// What the program shows at start-up when no game folder is given on the
/// command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Startup {
    /// The library is empty: the first-start flow (the installation found,
    /// the welcome, the folder picker).
    FirstStart,
    /// The library's one game, at once.
    PlayFirst,
    /// The game library's screen.
    Launcher,
}

/// The start-up choice: the game library's screen when it was asked for
/// (Shift held, `--launcher`), when the library holds two games or more,
/// or when its one game no longer opens (`first_opens`, only called then).
pub fn startup(library_len: usize, requested: bool, first_opens: impl FnOnce() -> bool) -> Startup {
    match library_len {
        _ if requested => Startup::Launcher,
        0 => Startup::FirstStart,
        1 if first_opens() => Startup::PlayFirst,
        _ => Startup::Launcher,
    }
}

/// Whether Shift is held now, where the system can tell before a window
/// opens (Windows, macOS); elsewhere a Shift reported in the first moments
/// of the window does it (see [`Session::shift_grace`]).
pub fn shift_held() -> bool {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        extern "system" {
            fn GetAsyncKeyState(key: i32) -> i16;
        }
        const VK_SHIFT: i32 = 0x10;
        // SAFETY: a plain query of the keyboard state, no pointers.
        unsafe { GetAsyncKeyState(VK_SHIFT) < 0 }
    }
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventSourceFlagsState(state: i32) -> u64;
        }
        const COMBINED_SESSION_STATE: i32 = 0;
        const FLAG_MASK_SHIFT: u64 = 0x0002_0000;
        // SAFETY: a plain query of the modifier keys, no pointers.
        unsafe { CGEventSourceFlagsState(COMBINED_SESSION_STATE) & FLAG_MASK_SHIFT != 0 }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    false
}

/// The desktop session: the library and the settings of the games
/// started from it.
pub struct Session {
    /// The per-user data folder, where the library is kept.
    data_dir: PathBuf,
    library: Library,
    /// `--original`, `--hd`, `--widescreen`: for every game started.
    original: bool,
    force_hd: bool,
    force_wide: bool,
    /// How the next game starts: the command line's for the first, the
    /// usual start after.
    start: Start,
    /// The game library's screen, while it is shown.
    launcher: Option<Launcher>,
    /// The entries found last time the screen was shown: shown again at
    /// once (opening a game folder takes a moment, more on a network or
    /// optical drive).
    known: Vec<LauncherEntry>,
    /// Until when Shift opens the game library (after an automatic start).
    shift_until: Option<Instant>,
    /// The play time not yet written to `support.ini` (see
    /// [`crate::support`]).
    /// Whether this run's play time counts towards the support screen
    /// (not for a run with a game folder or a testing option).
    pub counts_play: bool,
    pub(crate) play_clock: PlayClock,
    /// Atlantis II's folder, chosen to play: the program runs it once the
    /// event loop has ended.
    atlantis2: Option<PathBuf>,
}

impl Session {
    /// A session with the library kept in `data_dir`.
    pub fn new(
        data_dir: PathBuf,
        original: bool,
        force_hd: bool,
        force_wide: bool,
        start: Start,
    ) -> Session {
        let library = Library::load(&data_dir);
        Session {
            data_dir,
            library,
            original,
            force_hd,
            force_wide,
            start,
            launcher: None,
            known: Vec::new(),
            shift_until: None,
            play_clock: PlayClock::default(),
            counts_play: true,
            atlantis2: None,
        }
    }

    /// The folder of Atlantis II chosen to play, if any (taken).
    pub fn take_atlantis2(&mut self) -> Option<PathBuf> {
        self.atlantis2.take()
    }

    /// Whether Atlantis II was chosen to play: the event loop is to end.
    pub fn wants_atlantis2(&self) -> bool {
        self.atlantis2.is_some()
    }

    /// Adds `dirs` to the library (those not in it yet), saved.
    pub fn add_games(&mut self, dirs: &[PathBuf]) {
        for d in dirs {
            self.library.add(d);
        }
        self.library.save_logged(&self.data_dir);
    }

    /// The per-user data folder.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    /// Whether the game library's screen is shown.
    pub fn launcher(&self) -> Option<&Launcher> {
        self.launcher.as_ref()
    }

    /// The game was started without the screen at start-up: Shift held
    /// in the next moments still opens it.
    pub fn shift_grace(&mut self) {
        self.shift_until = Some(Instant::now() + SHIFT_GRACE);
    }

    /// `dir` was played: first in the library, saved.
    fn played(&mut self, dir: &Path) {
        self.library.played(dir);
        self.library.save_logged(&self.data_dir);
    }

    /// The screen's entries after a change of the library, `selected`
    /// selected: the games found before are kept (each folder is opened
    /// once), the others are looked at again (a drive plugged in since).
    fn refresh(&mut self, selected: usize, describe: &dyn Fn(&Path) -> LauncherEntry) {
        let mut old = self
            .launcher
            .as_ref()
            .map(|l| l.entries().to_vec())
            .unwrap_or_default();
        old.extend(self.known.iter().cloned());
        let entries: Vec<LauncherEntry> = self
            .library
            .folders()
            .iter()
            .map(|f| match old.iter().find(|e| e.path == *f && e.found) {
                Some(e) => e.clone(),
                None => describe(f),
            })
            .collect();
        self.known = entries.clone();
        self.launcher
            .get_or_insert_with(Launcher::default)
            .set_entries(entries, selected);
    }

    fn message(&mut self, text: impl Into<String>) {
        if let Some(l) = &mut self.launcher {
            l.set_message(text);
        }
    }
}

/// Draws the game library's screen in `frame` when it is shown; false
/// when it is not.
pub fn draw_launcher(
    session: Option<&Session>,
    frame: &mut Vec<u16>,
    font: &mut Option<SubtitleFont>,
) -> bool {
    let Some(launcher) = session.and_then(|s| s.launcher.as_ref()) else {
        return false;
    };
    frame.resize(SCREEN_W * SCREEN_H, 0);
    launcher.draw(frame, font.get_or_insert_with(SubtitleFont::new));
    true
}

fn launcher_key(key: &Key) -> Option<LauncherKey> {
    Some(match key {
        Key::Named(NamedKey::ArrowUp) => LauncherKey::Up,
        Key::Named(NamedKey::ArrowDown) => LauncherKey::Down,
        Key::Named(NamedKey::ArrowLeft) => LauncherKey::Left,
        Key::Named(NamedKey::ArrowRight) => LauncherKey::Right,
        Key::Named(NamedKey::Home) => LauncherKey::Home,
        Key::Named(NamedKey::End) => LauncherKey::End,
        Key::Named(NamedKey::PageUp) => LauncherKey::PageUp,
        Key::Named(NamedKey::PageDown) => LauncherKey::PageDown,
        Key::Named(NamedKey::Tab) => LauncherKey::Tab,
        Key::Named(NamedKey::Enter | NamedKey::Space) => LauncherKey::Enter,
        Key::Named(NamedKey::Delete | NamedKey::Backspace) => LauncherKey::Delete,
        Key::Named(NamedKey::Escape) => LauncherKey::Escape,
        Key::Character(s) => LauncherKey::Char(s.chars().next()?),
        _ => return None,
    })
}

/// "1 installation", "3 installations".
fn installations(n: usize) -> String {
    if n == 1 {
        "1 installation".to_string()
    } else {
        format!("{n} installations")
    }
}

impl App {
    /// The desktop session: the options menu gets its "Game library..."
    /// entry.
    pub fn with_session(mut self, session: Session) -> App {
        self.menu.set_library(true);
        self.session = Some(session);
        self
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    /// Starts `game` (in place of the one running, if any) with the
    /// session's options, its settings and saves those of its build;
    /// `folder`, when given, moves to the top of the library.
    pub fn play_game(&mut self, game: Game, folder: Option<&Path>) {
        let Some(session) = &mut self.session else {
            log::error!("play_game without a session");
            return;
        };
        session.launcher = None;
        if let Some(dir) = folder {
            session.played(dir);
        }
        let (original, force_hd, force_wide) =
            (session.original, session.force_hd, session.force_wide);
        let start = std::mem::replace(&mut session.start, Start::Boot);
        let saves = desktop::store_for(&game);
        let options = options_with_settings(saves.clone(), original, start);
        let graphics = if force_hd {
            Graphics::Hd
        } else {
            Settings::load(saves.as_ref()).unwrap_or_default().graphics
        };
        let edition = game.edition();
        log::info!("{edition}, {}", crate::version_string());
        let icon = game.icon.clone();
        let fixes = atlantis_engine::BugFix::of_game(&game);
        self.install_game(
            start_game(game, options),
            graphics,
            saves,
            (&edition, fixes),
            icon,
        );
        if force_wide {
            self.force_widescreen();
        }
    }

    /// Plays what `opened` is: the first game at once, Atlantis II once the
    /// event loop has ended ([`Session::take_atlantis2`]); `folder` moves to
    /// the top of the library.
    pub fn play_opened(&mut self, opened: Opened, folder: &Path) {
        match opened {
            Opened::LostTales(game, _) => self.play_game(*game, Some(folder)),
            Opened::Atlantis2(build) => {
                log::info!("game folder {} ({})", folder.display(), build.label);
                self.stop_game();
                let Some(session) = &mut self.session else {
                    return;
                };
                session.launcher = None;
                session.played(folder);
                session.atlantis2 = Some(folder.to_path_buf());
                // The window of this event loop closes; Atlantis II opens
                // its own.
                if let Some(w) = &self.window {
                    w.set_visible(false);
                }
            }
        }
    }

    /// Stops the game running, if any, and shows the game library's
    /// screen (the last played selected), with `message` under it.
    pub fn show_launcher(&mut self, message: Option<String>) {
        self.stop_game();
        let Some(session) = &mut self.session else {
            return;
        };
        session.shift_until = None;
        session.launcher = None;
        session.refresh(0, &desktop::describe);
        let message = message.unwrap_or_else(|| {
            if session.library.is_empty() {
                String::new()
            } else {
                "Choose the game to play".to_string()
            }
        });
        session.message(message);
        log::info!("game library: {} games", session.library.len());
    }

    /// The window events while the game library's screen is shown (and
    /// Shift in the first moments of an automatic start); true when the
    /// event was taken.
    pub(crate) fn launcher_event(&mut self, el: &ActiveEventLoop, event: &WindowEvent) -> bool {
        let Some(session) = &mut self.session else {
            return false;
        };
        if let WindowEvent::ModifiersChanged(m) = event {
            let early = session.shift_until.is_some_and(|t| Instant::now() < t);
            if early && m.state().shift_key() {
                log::info!("Shift held at start-up: the game library");
                self.show_launcher(None);
                return true;
            }
        }
        if session.shift_until.is_some_and(|t| Instant::now() >= t) {
            session.shift_until = None;
        }
        let Some(launcher) = &mut session.launcher else {
            return false;
        };
        let action = match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor = Some((position.x, position.y));
                let (x, y) = self.viewport.to_screen(position.x, position.y);
                launcher.pointer(x, y);
                LauncherAction::None
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => match self.last_cursor {
                Some((wx, wy)) => {
                    let (x, y) = self.viewport.to_screen(wx, wy);
                    let ms = self.start.elapsed().as_millis() as u64;
                    launcher.click(x, y, ms)
                }
                None => LauncherAction::None,
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let rows = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y.round() as i32,
                    MouseScrollDelta::PixelDelta(p) => -(p.y / 40.0).round() as i32,
                };
                launcher.wheel(rows);
                LauncherAction::None
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match launcher_key(&event.logical_key) {
                    Some(k) => launcher.key(k),
                    None => LauncherAction::None,
                }
            }
            // The rest (close, redraw, resize, focus) as with a game.
            _ => return false,
        };
        self.launcher_action(el, action);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        true
    }

    /// Carries out an action of the game library's screen.
    fn launcher_action(&mut self, el: &ActiveEventLoop, action: LauncherAction) {
        if action == LauncherAction::Quit {
            el.exit();
            return;
        }
        // The dialogs below wait for the player: not a hang.
        crate::watchdog::leave();
        self.run_launcher_action(action);
        // Atlantis II chosen: the program runs it after this event loop.
        if self.session.as_ref().is_some_and(Session::wants_atlantis2) {
            el.exit();
        }
    }

    /// [`App::launcher_action`] but Quit (no event loop needed).
    fn run_launcher_action(&mut self, action: LauncherAction) {
        let Some(session) = &mut self.session else {
            return;
        };
        match action {
            LauncherAction::None | LauncherAction::Quit => {}
            LauncherAction::Support => crate::open_support_page(),
            LauncherAction::Play(i) => {
                let Some(dir) = session.library.folders().get(i).cloned() else {
                    return;
                };
                match desktop::open_any(&dir) {
                    Ok(opened) => {
                        log::info!("game folder {}", dir.display());
                        self.play_opened(opened, &dir);
                    }
                    Err(e) => {
                        log::warn!("{}: {e}", dir.display());
                        // It no longer opens: shown as not found.
                        if let Some(l) = &mut session.launcher {
                            let mut entries = l.entries().to_vec();
                            if let Some(entry) = entries.get_mut(i) {
                                *entry = desktop::describe(&dir);
                            }
                            l.set_entries(entries, i);
                        }
                        let side = matches!(
                            desktop::try_open(&dir),
                            Err(atlantis_data::GameError::DvdOtherSide(_))
                        );
                        session.message(if side {
                            "This version is on the other side of the DVD: turn the disc over"
                        } else {
                            "This folder no longer opens: locate it or remove it"
                        });
                    }
                }
            }
            LauncherAction::Locate(i) => {
                if let Some(mut found) = desktop::pick_game_folder("Cancel") {
                    // The DVD: the same language as the entry's, if found.
                    let was = session
                        .library
                        .folders()
                        .get(i)
                        .and_then(|d| desktop::dvd_language_of(d))
                        .map(|(l, _)| l);
                    let k = found
                        .iter()
                        .position(|(_, d)| {
                            was.is_some() && desktop::dvd_language_of(d).map(|(l, _)| l) == was
                        })
                        .unwrap_or(0);
                    let (opened, dir) = found.swap_remove(k);
                    let at = session.library.relocate(i, &dir);
                    session.library.save_logged(&session.data_dir);
                    session.refresh(at, &desktop::describe);
                    session.message(format!("Found again: {}", opened.label()));
                }
            }
            LauncherAction::Add => {
                if let Some(found) = desktop::pick_game_folder("Cancel") {
                    let added: Vec<&(desktop::Opened, PathBuf)> = found
                        .iter()
                        .filter(|(_, dir)| session.library.add(dir))
                        .collect();
                    session.library.save_logged(&session.data_dir);
                    let first = added.first().map_or(&found[0], |a| *a);
                    let at = session.library.position(&first.1).unwrap_or(0);
                    session.refresh(at, &desktop::describe);
                    session.message(match added.len() {
                        0 if found.len() > 1 => "These games are in the list already".to_string(),
                        0 => "This game is in the list already".to_string(),
                        1 => format!("Added: {}", added[0].0.label()),
                        n => format!("Added {n} editions of the DVD: choose the language to play"),
                    });
                }
            }
            LauncherAction::Find => {
                let found = desktop::find_installations();
                let new: Vec<PathBuf> = found
                    .iter()
                    .filter(|d| session.library.add(d))
                    .cloned()
                    .collect();
                session.library.save_logged(&session.data_dir);
                let selected = new
                    .first()
                    .and_then(|d| session.library.position(d))
                    .unwrap_or_else(|| session.launcher.as_ref().map_or(0, |l| l.selected()));
                session.refresh(selected, &desktop::describe);
                let text = match (found.len(), new.len()) {
                    (0, _) => {
                        "No installation found: use Add a game folder to choose yours".to_string()
                    }
                    (n, 0) => format!("{} found, all in the list already", installations(n)),
                    (n, k) => format!("{} found, {k} added to the list", installations(n)),
                };
                log::info!("{text}");
                session.message(text);
            }
            LauncherAction::Remove(i) => {
                if let Some(dir) = session.library.remove(i) {
                    log::info!("{}: taken off the library", dir.display());
                    session.library.save_logged(&session.data_dir);
                    session.refresh(i, &desktop::describe);
                    session.message("Removed from the list (no file was deleted)");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_engine::audio::Mixer;
    use atlantis_engine::options_menu::MenuAction;
    use atlantis_engine::sys::Sys;

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("atlantis-session-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn startup_choice() {
        let never = || -> bool { panic!("not asked") };
        assert_eq!(startup(0, false, never), Startup::FirstStart);
        assert_eq!(startup(0, true, never), Startup::Launcher);
        assert_eq!(startup(1, false, || true), Startup::PlayFirst);
        assert_eq!(startup(1, false, || false), Startup::Launcher);
        assert_eq!(startup(1, true, never), Startup::Launcher);
        assert_eq!(startup(2, false, never), Startup::Launcher);
    }

    /// `ATLANTIS_NATIVE_DATA` pointed at a test's own folder, for one test
    /// at a time (the variable is the process's: two such tests at once
    /// would read and remove each other's folder); on drop the variable
    /// is set back, then the folder removed.
    struct OwnDataDir {
        dir: PathBuf,
        before: Option<std::ffi::OsString>,
        _one_at_a_time: std::sync::MutexGuard<'static, ()>,
    }

    fn own_data_dir(name: &str) -> OwnDataDir {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tmp(name);
        let before = std::env::var_os("ATLANTIS_NATIVE_DATA");
        std::env::set_var("ATLANTIS_NATIVE_DATA", &dir);
        OwnDataDir {
            dir,
            before,
            _one_at_a_time: guard,
        }
    }

    impl Drop for OwnDataDir {
        fn drop(&mut self) {
            match &self.before {
                Some(v) => std::env::set_var("ATLANTIS_NATIVE_DATA", v),
                None => std::env::remove_var("ATLANTIS_NATIVE_DATA"),
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn app_with_library(d: &Path) -> App {
        let session = Session::new(d.to_path_buf(), false, false, false, Start::Boot);
        App::new(Sys::new(Mixer::new()), None).with_session(session)
    }

    #[test]
    fn the_menu_entry_opens_the_library() {
        let d = tmp("menu");
        let gone = d.join("gone");
        std::fs::write(d.join("games.txt"), format!("{}\n", gone.display())).unwrap();
        let mut app = app_with_library(&d);
        assert!(app.session().unwrap().launcher().is_none());
        app.open_menu();
        app.menu_action(MenuAction::OpenLibrary);
        assert!(!app.menu.is_open());
        let l = app.session().unwrap().launcher().expect("the game library");
        assert_eq!(l.entries().len(), 1);
        assert!(!l.entries()[0].found);
        assert!(l.entries()[0].label.starts_with("Not found"));
        // Removed: the file is rewritten, the folder untouched.
        app.run_launcher_action(LauncherAction::Remove(0));
        assert!(app
            .session()
            .unwrap()
            .launcher()
            .unwrap()
            .entries()
            .is_empty());
        assert_eq!(std::fs::read_to_string(d.join("games.txt")).unwrap(), "");
        let mut frame = Vec::new();
        let mut font = None;
        assert!(draw_launcher(app.session(), &mut frame, &mut font));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The game library with both games: the first game and GOG.com's
    /// Atlantis II, each shown with its name and edition; Atlantis II
    /// chosen on the screen is kept for the program to run after the
    /// event loop, first in the library, and boots. With
    /// `ATLANTIS_LIBRARY_SHOTS=<folder>` the screen is saved there
    /// (`library-both.png`, `library-atlantis2-chosen.png`).
    #[test]
    fn the_library_plays_atlantis2() {
        let Some(a2) = std::env::var_os("ATLANTIS2_GOG").map(PathBuf::from) else {
            atlantis2_data::testing::missing("ATLANTIS2_GOG not set");
            return;
        };
        let first = std::env::var_os("ATLANTIS_DATA").map(PathBuf::from);
        let own = own_data_dir("both-games");
        let d = own.dir.clone();
        let mut folders: Vec<PathBuf> = first.iter().cloned().collect();
        folders.push(a2.clone());
        let text: String = folders
            .iter()
            .map(|f| format!("{}\n", f.display()))
            .collect();
        std::fs::write(d.join("games.txt"), &text).unwrap();
        let mut app = app_with_library(&d);
        app.show_launcher(None);
        let shot = |app: &App, name: &str| {
            let Some(dir) = std::env::var_os("ATLANTIS_LIBRARY_SHOTS") else {
                return;
            };
            let mut frame = Vec::new();
            let mut font = None;
            assert!(draw_launcher(app.session(), &mut frame, &mut font));
            let png = atlantis2_engine::png::encode_rgb565(&frame, SCREEN_W, SCREEN_H);
            std::fs::write(PathBuf::from(dir).join(name), png).unwrap();
        };
        shot(&app, "library-both.png");
        let l = app.session().unwrap().launcher().unwrap();
        let labels: Vec<&str> = l.entries().iter().map(|e| e.label.as_str()).collect();
        assert!(l.entries().iter().all(|e| e.found), "{labels:?}");
        assert_eq!(labels.last(), Some(&"Atlantis II, GOG.com (patch 1.01)"));
        if first.is_some() {
            assert!(labels[0].starts_with("Atlantis: The Lost Tales, "));
        }
        // Played from the screen: kept for after the event loop, first
        // in the library (saved).
        let i = folders.len() - 1;
        app.run_launcher_action(LauncherAction::Play(i));
        assert!(app.session().unwrap().launcher().is_none());
        assert!(app.session().unwrap().wants_atlantis2());
        assert_eq!(app.session().unwrap().library().folders()[0], a2);
        assert_eq!(
            Library::load(&d).folders().first(),
            Some(&a2),
            "games.txt rewritten"
        );
        assert_eq!(
            app.session_mut().unwrap().take_atlantis2(),
            Some(a2.clone())
        );
        assert!(!app.session().unwrap().wants_atlantis2());
        // The screen again: Atlantis II selected, last played.
        app.show_launcher(None);
        shot(&app, "library-atlantis2-chosen.png");
        // What the program then runs: Atlantis II's mode, from this folder.
        let (engine, label) = crate::a2::open(
            &a2,
            &[],
            &crate::a2::A2Start {
                intro: Some(false),
                ..Default::default()
            },
            atlantis2_engine::Enhancements::default(),
        )
        .unwrap();
        assert!(label.contains("GOG.com"), "{label}");
        drop(engine);
    }

    /// Switching games drops the engine: nothing keeps its system alive.
    #[test]
    fn switching_drops_the_engine() {
        let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
            eprintln!("ATLANTIS_DATA not set: switch test skipped");
            return;
        };
        let dir = PathBuf::from(dir);
        // The saves of this test in its own folder.
        let own = own_data_dir("switch");
        let d = own.dir.clone();
        let mut app = app_with_library(&d);
        let game = desktop::try_open(&dir).unwrap();
        app.play_game(game, Some(&dir));
        assert!(app.has_game());
        assert_eq!(
            app.session().unwrap().library().folders(),
            std::slice::from_ref(&dir)
        );
        let sys = std::rc::Rc::downgrade(&app.sys);
        for i in 0..30 {
            assert!(app.runner.as_mut().unwrap().step(i * 16));
        }
        app.menu_action(MenuAction::OpenLibrary);
        assert!(!app.has_game());
        assert!(sys.upgrade().is_none(), "the engine's system is dropped");
        let l = app.session().unwrap().launcher().unwrap();
        assert!(l.entries()[0].found);
        assert!(l.entries()[0]
            .label
            .starts_with("Atlantis: The Lost Tales, CD, English"));
        // Played again from the screen.
        app.run_launcher_action(LauncherAction::Play(0));
        assert!(app.has_game());
        assert!(app.session().unwrap().launcher().is_none());
        // The engine before the folder goes.
        drop(app);
        drop(own);
    }

    /// The game library with Cryo's DVD: each of its eight languages found
    /// in the images of both sides its own edition ("from disc images"), and
    /// the Perfect Edition; one played from the screen, first in the
    /// library. With `ATLANTIS_DVD_SHOTS=<folder>` the screen is saved
    /// there (`library-dvd.png`).
    #[test]
    fn the_library_lists_the_dvd_languages() {
        let (Some(ia), Some(ib)) = (
            std::env::var_os("ATLANTIS_DVD_A").map(PathBuf::from),
            std::env::var_os("ATLANTIS_DVD_B").map(PathBuf::from),
        ) else {
            eprintln!("ATLANTIS_DVD_A / ATLANTIS_DVD_B not set: skipped");
            return;
        };
        let own = own_data_dir("dvd-library");
        let d = own.dir.clone();
        let isos = d.join("Atlantis DVD");
        std::fs::create_dir_all(&isos).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&ia, isos.join("side_a.iso")).unwrap();
            std::os::unix::fs::symlink(&ib, isos.join("side_b.iso")).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(&ia, isos.join("side_a.iso")).unwrap();
            std::os::windows::fs::symlink_file(&ib, isos.join("side_b.iso")).unwrap();
        }
        // What Add a game folder adds: the languages that open.
        let mut folders: Vec<PathBuf> = desktop::dvd_locations(&isos)
            .into_iter()
            .filter(|p| desktop::open_any(p).is_ok())
            .collect();
        assert_eq!(folders.len(), 8);
        folders.extend(std::env::var_os("ATLANTIS_PERFECT_EDITION").map(PathBuf::from));
        let text: String = folders
            .iter()
            .map(|f| format!("{}\n", f.display()))
            .collect();
        std::fs::write(d.join("games.txt"), &text).unwrap();
        let mut app = app_with_library(&d);
        app.show_launcher(None);
        if let Some(dir) = std::env::var_os("ATLANTIS_DVD_SHOTS") {
            let mut frame = Vec::new();
            let mut font = None;
            assert!(draw_launcher(app.session(), &mut frame, &mut font));
            let png = atlantis2_engine::png::encode_rgb565(&frame, SCREEN_W, SCREEN_H);
            std::fs::write(PathBuf::from(dir).join("library-dvd.png"), png).unwrap();
        }
        let l = app.session().unwrap().launcher().unwrap();
        let labels: Vec<&str> = l.entries().iter().map(|e| e.label.as_str()).collect();
        assert!(l.entries().iter().all(|e| e.found), "{labels:?}");
        for (i, language) in [
            "German", "English", "French", "Dutch", "Spanish", "Italian", "Swedish", "Japanese",
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(
                labels[i],
                format!("Atlantis: The Lost Tales, DVD, {language} (1998 build), from disc images")
            );
        }
        // The Italian played: first in the library.
        app.run_launcher_action(LauncherAction::Play(5));
        assert!(app.has_game());
        assert_eq!(app.session().unwrap().library().folders()[0], folders[5]);
        drop(app);
        drop(own);
    }
}

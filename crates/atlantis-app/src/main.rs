//! Desktop entry point: `atlantis-native [<game dir>]`.
//!
//! Without a game folder (a double click), the game library decides
//! (`games.txt` in the per-user data folder, one game folder per line, the
//! last played first): its one game starts at once; with two games or
//! more, or when its game no longer opens, or with Shift held at start-up
//! or `--launcher`, the game library's screen shows them to choose from.
//! The first time (an empty library) an installation found on the
//! computer is offered, else the system's folder picker asks for the
//! folder, which joins the library. A game folder on the command line is
//! played as it is (not added to the library). Errors are shown in message
//! boxes. Without a terminal the log goes to `atlantis-native.log` in the
//! same folder.
//!
//! `atlantis-native --headless <game dir> [frames]` runs the engine without
//! a window for a number of frames (default 600) with no input, and prints
//! what happened: a check that the game starts. The sound is mixed and
//! dropped, so that the voices end as they would.
//!
//! The options menu's "Game library..." entry (F10) stops the game and
//! shows the game library's screen, to play another edition.
//!
//! `--hd` starts in the HD mode (the panoramic view and the talking heads
//! at the window's resolution), whatever `settings.ini` says; F10 opens the
//! port's options menu, where it can be switched.
//!
//! `--fullscreen` opens the window in full screen (F, F11 or Alt+Enter
//! switches it back).
//!
//! A folder holding Atlantis II (`atlantis2.exe`) opens Atlantis II's own
//! mode (`atlantis_app::a2`): the player menu, then Tibet, to look around
//! and walk from view to view with a left click; Escape opens the menus
//! (options, volumes, saves). Its discs (CD images or copies) are looked
//! for in the folder, in the folders directly inside it and in each
//! `--discs <folder>`; `--headless <folder> [frames] [--screenshot
//! <file.png>]` plays a new game, prints where it is and saves the screen;
//! `--view <id>[,<yaw>,<pitch>]` starts in another view, `--dialog <n>`
//! plays a dialog at once, `--menu <page>` (`player`, `main`, `options`,
//! `sound`, `load`, `credits`) shows a menu page (testing aids). Only the
//! command line opens Atlantis II for now:
//! the launcher, the game library and the web build know the first game
//! alone. The first game's options are refused in this mode, and `--discs`
//! and `--screenshot` with the first game.
//!
//! `--original` (before the game directory) turns every enhancement off:
//! the original game, bug for bug. `--new-game` skips the start-up videos
//! and the player screen.
//!
//! `--puzzle <n> [--object <id>]` (a testing aid) plays StartPuzzle `n` at
//! once, with object `id` in hand, reports its FinPuzzle, then starts a new
//! game.
//!
//! Now and then at start-up (after a few launches and two hours of play, at
//! most once a week, never with a game folder or a testing option on the
//! command line) a support screen thanks the player, with links to the
//! project's pages; "Don't show this again" turns it off for good, and
//! `--support-screen` shows it at once (see `atlantis_app::support`).
//!
//! The players, the checkpoints (the original's `players.atl` and
//! `p<letter><chapter>.gam`) and `settings.ini` are kept in the per-user
//! data folder (`~/.local/share/atlantis-native/<build>` on Linux,
//! `%APPDATA%\atlantis-native\<build>` on Windows, `~/Library/Application
//! Support/atlantis-native/<build>` on macOS); `settings.ini` also holds
//! the enhancements.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use std::cell::RefCell;
    use std::io::IsTerminal;
    use std::path::Path;
    use std::rc::Rc;

    use atlantis_app::desktop::{
        data_dir, error_box, first_start, init_log, install_panic_hook, open_game, try_open,
    };
    use atlantis_app::session::{shift_held, startup, Session, Startup};
    use atlantis_app::support::{should_show, unix_now, SupportState};
    use atlantis_app::{start_game, App};
    use atlantis_engine::audio::Mixer;
    use atlantis_engine::sys::Sys;
    use atlantis_engine::{Enhancements, Options, Start};
    use winit::event_loop::EventLoop;

    let mut args: Vec<String> = std::env::args()
        .skip(1)
        // Old macOS versions pass the process serial number to an app
        // opened from the Finder.
        .filter(|a| !a.starts_with("-psn_"))
        .collect();
    let headless = args.iter().any(|a| a == "--headless");
    let log_file = init_log(headless);
    // No terminal: the panics and the errors in message boxes.
    install_panic_hook(log_file.is_some() || !std::io::stderr().is_terminal());
    atlantis_app::watchdog::start();
    if let Some(path) = &log_file {
        log::info!("log in {}", path.display());
    }
    let mut flag = |name: &str| match args.iter().position(|a| a == name) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    };
    let original = flag("--original");
    let force_wide = flag("--widescreen");
    let force_hd = flag("--hd") || force_wide;
    let new_game = flag("--new-game");
    let launcher = flag("--launcher");
    let force_support = flag("--support-screen");
    let fullscreen = flag("--fullscreen");
    // Atlantis II: the folders of its discs, and a screenshot of a
    // headless run.
    let mut discs = Vec::new();
    let mut screenshot = None;
    while let Some(i) = args
        .iter()
        .position(|a| a == "--discs" || a == "--screenshot")
    {
        let name = args.remove(i);
        if i >= args.len() {
            eprintln!("{name} needs a folder or a file");
            std::process::exit(2);
        }
        let path = std::path::PathBuf::from(args.remove(i));
        if name == "--discs" {
            discs.push(path);
        } else {
            screenshot = Some(path);
        }
    }
    let mut value = |name: &str| -> Option<i32> {
        let i = args.iter().position(|a| a == name)?;
        args.remove(i);
        if i >= args.len() {
            eprintln!("{name} needs a number");
            std::process::exit(2);
        }
        let v = args.remove(i);
        Some(v.parse().unwrap_or_else(|_| {
            eprintln!("{name} {v}: not a number");
            std::process::exit(2);
        }))
    };
    let puzzle = value("--puzzle");
    let object = value("--object");
    let dialog = value("--dialog");
    // Atlantis II: `--view <id>[,<yaw>,<pitch>]`.
    let view = match args.iter().position(|a| a == "--view") {
        Some(i) if i + 1 < args.len() => {
            args.remove(i);
            let v = args.remove(i);
            Some(
                atlantis_app::a2::A2Start::parse_view(&v).unwrap_or_else(|| {
                    eprintln!("--view {v}: not <id>[,<yaw>,<pitch>]");
                    std::process::exit(2);
                }),
            )
        }
        Some(_) => {
            eprintln!("--view needs a view");
            std::process::exit(2);
        }
        None => None,
    };
    // Atlantis II: `--menu <page>`.
    let menu = match args.iter().position(|a| a == "--menu") {
        Some(i) if i + 1 < args.len() => {
            args.remove(i);
            let v = args.remove(i);
            Some(
                atlantis_app::a2::A2Start::parse_menu(&v).unwrap_or_else(|| {
                    eprintln!("--menu {v}: not player, main, options, sound, load or credits");
                    std::process::exit(2);
                }),
            )
        }
        Some(_) => {
            eprintln!("--menu needs a page");
            std::process::exit(2);
        }
        None => None,
    };
    let a2_start = atlantis_app::a2::A2Start {
        view,
        dialog: dialog.map(|d| d.max(0) as u32),
        menu,
    };
    // The options of one game only, refused for the other rather than
    // ignored.
    let first_game_flags: Vec<&str> = [
        ("--original", original),
        ("--hd", force_hd && !force_wide),
        ("--widescreen", force_wide),
        ("--new-game", new_game),
        ("--launcher", launcher),
        ("--support-screen", force_support),
        ("--puzzle", puzzle.is_some()),
        ("--object", object.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, on)| on.then_some(name))
    .collect();
    let a2_flags: Vec<&str> = [
        ("--discs", !discs.is_empty()),
        ("--screenshot", screenshot.is_some()),
        ("--dialog", dialog.is_some()),
        ("--view", view.is_some()),
        ("--menu", menu.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, on)| on.then_some(name))
    .collect();
    let object = object.unwrap_or(0);
    let refuse = |flags: &[&str], game: &str| {
        if !flags.is_empty() {
            let text = format!("{}: not an option of {game}", flags.join(", "));
            if headless {
                eprintln!("{text}");
            } else {
                error_box(&text);
            }
            std::process::exit(2);
        }
    };
    let start = match puzzle {
        Some(id) => Start::Puzzle { id, object },
        None if new_game => Start::NewGame,
        None => Start::Boot,
    };

    if args.first().map(String::as_str) == Some("--headless")
        && args
            .get(1)
            .is_some_and(|d| atlantis_app::a2::holds_atlantis2(Path::new(d)))
    {
        refuse(&first_game_flags, "Atlantis II's mode");
        let frames: u64 = args.get(2).and_then(|f| f.parse().ok()).unwrap_or(600);
        if let Err(e) = atlantis_app::a2::headless(
            Path::new(&args[1]),
            &discs,
            frames,
            screenshot.as_deref(),
            &a2_start,
        ) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    let is_a2 = args.len() == 1 && atlantis_app::a2::holds_atlantis2(Path::new(&args[0]));
    if !is_a2 {
        refuse(&a2_flags, "the first game (Atlantis II only)");
    }
    if args.first().map(String::as_str) == Some("--headless") {
        let Some(dir) = args.get(1) else {
            eprintln!(
                "usage: atlantis-native [--original] [--puzzle <n> [--object <id>]] --headless <game dir> [frames]"
            );
            std::process::exit(2);
        };
        let game = open_game(Path::new(dir)).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });
        println!("{}, {}", game.edition(), atlantis_app::version_string());
        let frames: u64 = args.get(2).and_then(|f| f.parse().ok()).unwrap_or(600);
        let trace = Rc::new(RefCell::new(Vec::new()));
        // Headless: a new game at once, the saves in memory.
        let options = Options {
            trace: Some(trace.clone()),
            enhancements: if original {
                Enhancements::original()
            } else {
                Enhancements::default()
            },
            start: if puzzle.is_some() {
                start
            } else {
                Start::NewGame
            },
            ..Options::default()
        };
        let (sys, mut runner) = start_game(game, options);
        let mut alive = true;
        // 16 ms of sound per frame at 22050 Hz, stereo.
        let mut sound = vec![0.0f32; 2 * (22050 * 16 / 1000)];
        for i in 0..frames {
            let t0 = std::time::Instant::now();
            alive = runner.step(i * 16);
            let took = t0.elapsed();
            if took.as_millis() > 100 {
                eprintln!("slow engine step: {} ms at frame {i}", took.as_millis());
            }
            sys.mixer.fill(&mut sound, 22050, 2);
            if !alive {
                break;
            }
        }
        for e in trace.borrow().iter() {
            println!("{e:?}");
        }
        let screen = sys.screen.borrow();
        let distinct = screen
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        println!(
            "{frames} frames, engine {}, {distinct} colours on screen",
            if alive { "running" } else { "ended" }
        );
        return;
    }

    // A folder of Atlantis II: its own mode.
    if is_a2 {
        refuse(&first_game_flags, "Atlantis II's mode");
        let run = atlantis_app::a2::open(Path::new(&args[0]), &discs, &a2_start)
            .and_then(|engine| atlantis_app::a2::run_window(engine, fullscreen));
        if let Err(e) = run {
            error_box(&e);
            std::process::exit(1);
        }
        return;
    }

    const USAGE: &str = "usage: atlantis-native [--original] [--hd] [--widescreen] [--fullscreen] [--launcher] [--support-screen] [--new-game | --puzzle <n> [--object <id>]] [--discs <dir>] [<game dir>]";
    if args.len() > 1 || args.first().is_some_and(|a| a.starts_with('-')) {
        error_box(&format!("{USAGE}\n\n'{}' not understood", args.join(" ")));
        std::process::exit(2);
    }
    // Shift held as the program starts (read before the window opens, as
    // long as the key is still down).
    let requested = launcher || shift_held();
    // The event loop first: on macOS winit must own the application before
    // the folder picker or a message box creates it.
    let el = EventLoop::new().unwrap_or_else(|e| {
        error_box(&format!("No window can be opened: {e}"));
        std::process::exit(1);
    });
    // A run with a game folder or a testing option: it neither counts
    // towards the support screen (launches, play time) nor shows it.
    let scripted = !args.is_empty() || new_game || puzzle.is_some() || force_support;
    let mut session = Session::new(data_dir(), original, force_hd, force_wide, start);
    session.counts_play = !scripted;
    let mut app = App::new(Sys::new(Mixer::new()), None)
        .with_session(session)
        .with_fullscreen(fullscreen);
    // The launch counted; the support screen when it is due, or asked for.
    let mut counts = SupportState::load(&data_dir());
    if !scripted {
        counts.launches = counts.launches.saturating_add(1);
    }
    let now = unix_now();
    let due = !scripted && should_show(&counts, now);
    if due {
        counts.last_shown = now;
    }
    counts.save(&data_dir());
    // What the program starts with (after the support screen, if shown);
    // false: nothing to play, the program ends.
    let dir_arg = args.first().cloned();
    let begin = move |app: &mut App| -> bool {
        match dir_arg {
            // A game folder on the command line: played as it is.
            Some(dir) => {
                let game = open_game(Path::new(&dir)).unwrap_or_else(|e| {
                    error_box(&e);
                    std::process::exit(1);
                });
                app.play_game(game, None);
            }
            None => {
                let library = app
                    .session()
                    .map(|s| s.library().clone())
                    .unwrap_or_default();
                let first = library.folders().first().cloned();
                // The library's one game, opened once.
                let opened = (library.len() == 1 && !requested)
                    .then(|| first.as_deref().and_then(|d| try_open(d).ok()))
                    .flatten();
                let choice = startup(library.len(), requested, || opened.is_some());
                match choice {
                    Startup::FirstStart => match first_start() {
                        Some((game, dir)) => app.play_game(game, Some(&dir)),
                        None => {
                            eprintln!("{USAGE}");
                            return false;
                        }
                    },
                    Startup::PlayFirst => {
                        let (Some(game), Some(dir)) = (opened, first) else {
                            unreachable!("the first game opened");
                        };
                        log::info!("game folder {}", dir.display());
                        app.play_game(game, Some(&dir));
                        if let Some(s) = app.session_mut() {
                            s.shift_grace();
                        }
                    }
                    Startup::Launcher => app.show_launcher(None),
                }
            }
        }
        true
    };
    if due || force_support {
        app.show_support(data_dir(), Box::new(begin));
    } else if !begin(&mut app) {
        return;
    }
    let run = el.run_app(&mut app);
    app.save_play_time();
    if let Err(e) = run {
        error_box(&format!("The window failed: {e}"));
        std::process::exit(1);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

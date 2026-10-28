//! Desktop entry point: `atlantis-native [<game dir>]`.
//!
//! Without a game folder (a double click), the game library decides
//! (`games.txt` in the per-user data folder, one game folder per line, the
//! last played first; folders of the first game and of Atlantis II): its
//! one game starts at once; with two games or more, or when its game no
//! longer opens, or with Shift held at start-up or `--launcher`, the game
//! library's screen shows them to choose from. The first time (an empty
//! library) the installation found on the computer is offered (two games
//! or more found: they join the library, whose screen shows them), else
//! the system's folder picker asks for the folder, which joins the
//! library. Atlantis II chosen there runs in its own mode (below) once the
//! library's event loop has ended, on the same event loop. A game folder on
//! the command line is played as it is (not added to the library). Errors
//! are shown in message boxes. Without a terminal the log goes to
//! `atlantis-native.log` in the same folder.
//!
//! `atlantis-native --headless <game dir> [frames]` runs the engine without
//! a window for a number of frames (default 600) with no input, and prints
//! what happened: a check that the game starts. The sound is mixed and
//! dropped, so that the voices end as they would.
//!
//! The options menu's "Game library..." entry (F10) stops the game and
//! shows the game library's screen, to play another edition; Atlantis
//! II's has it too when it was started from the library (its window
//! ends, and the library's screen opens in a new one), and Shift in its
//! first moments does the same as for the first game.
//!
//! `--hd` starts in the HD mode (the panoramic view and the talking heads
//! at the window's resolution), whatever `settings.ini` says; F10 opens the
//! port's options menu, where it can be switched.
//!
//! `--fullscreen` opens the window in full screen (F, F11 or Alt+Enter
//! switches it back), whatever the first game's "Start in full screen"
//! (options menu, F10; kept in `settings.ini`, which F, F11 and
//! Alt+Enter set as they switch) says, without changing it.
//!
//! A folder holding Atlantis II (`atlantis2.exe`, or GOG.com's `Game.exe`)
//! opens Atlantis II's own mode (`atlantis_app::a2`): the player menu, then
//! Tibet, to look around and walk from view to view with a left click;
//! Escape opens the menus (options, volumes, saves). Its discs (CD images
//! or copies) are looked for in the folder, in the folders directly inside
//! it and in each `--discs <folder>` (GOG's folder holds them all); `--headless <folder> [frames] [--screenshot
//! <file.png>]` plays a new game, prints where it is and saves the screen;
//! `--view <id>[,<yaw>,<pitch>]` starts in another view, `--dialog <n>`
//! plays a dialog at once, `--menu <page>` (`player`, `main`, `options`,
//! `sound`, `load`, `credits`) shows a menu page (testing aids). The intro
//! videos play before the player menu, as in the original: `--no-intro`
//! skips them, `--intro` plays them in a headless run. `--hd` and
//! `--widescreen` draw its panorama at the window's resolution, as the
//! first game's HD mode does, `--size <w>x<h>` sizes its window (and a
//! headless run's screenshot); F10 (or O) opens its options menu, whose
//! choices are kept in its own `settings.ini`; `--resolution 800x600`
//! chooses the original's 800 x 600 screen (its options page's button,
//! kept in `atlant2.cfg`), `--options-menu` shows the options menu in a
//! headless run's screenshot (`--options-keys down,enter` presses keys in
//! it first, for that run only). The game library finds and opens Atlantis
//! II too (its folder holding its discs; the web build knows the first
//! game alone). The first game's options but `--original` are refused in
//! this mode, and `--discs` and `--screenshot` with the first game.
//!
//! `atlantis-native --list-games` prints the games found on the computer
//! (both games: what the library's "Find installations" adds) and ends.
//!
//! `--original` (before the game directory) turns every enhancement off,
//! in both games: the original game, bug for bug. `--new-game` skips the start-up videos
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
        data_dir, error_box, first_start, init_log, install_panic_hook, open_any, open_game,
        FirstStart,
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
    // The games found on this computer, printed (what Find installations
    // adds to the library).
    if flag("--list-games") {
        if !args.is_empty() {
            eprintln!("--list-games takes no game folder");
            std::process::exit(2);
        }
        let found = atlantis_app::desktop::find_installations();
        for dir in &found {
            let e = atlantis_app::desktop::describe(dir);
            println!("{}: {}", e.label, dir.display());
        }
        if found.is_empty() {
            println!("no game found");
        }
        return;
    }
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
    // Atlantis II's bag aids: `--take <names>`, `--hold <name>`, `--bag`,
    // `--pointer <x>,<y>`.
    let mut text_value = |name: &str| -> Option<String> {
        let i = args.iter().position(|a| a == name)?;
        args.remove(i);
        if i >= args.len() {
            eprintln!("{name} needs a value");
            std::process::exit(2);
        }
        Some(args.remove(i))
    };
    let take = text_value("--take");
    let command = text_value("--command");
    let seed = text_value("--seed").map(|v| {
        v.parse::<u32>().unwrap_or_else(|_| {
            eprintln!("--seed {v}: not a number from 0 to 4294967295");
            std::process::exit(2);
        })
    });
    let hold = text_value("--hold");
    let size = text_value("--size").map(|v| {
        atlantis_app::a2::A2Start::parse_size(&v).unwrap_or_else(|| {
            eprintln!("--size {v}: not <width>x<height>");
            std::process::exit(2);
        })
    });
    // Atlantis II: the original's screen option (its options page,
    // atlant2.cfg).
    let resolution = text_value("--resolution").map(|v| {
        atlantis_app::a2::A2Start::parse_resolution(&v).unwrap_or_else(|| {
            eprintln!("--resolution {v}: not 640x480 or 800x600");
            std::process::exit(2);
        })
    });
    let options_keys = text_value("--options-keys").map(|v| {
        atlantis_app::a2::A2Start::parse_options_keys(&v).unwrap_or_else(|| {
            eprintln!("--options-keys {v}: not up, down, left, right, enter or escape, separated by commas");
            std::process::exit(2);
        })
    });
    let pointer = text_value("--pointer").map(|v| {
        atlantis_app::a2::A2Start::parse_pointer(&v).unwrap_or_else(|| {
            eprintln!("--pointer {v}: not <x>,<y>");
            std::process::exit(2);
        })
    });
    let bag = match args.iter().position(|a| a == "--bag") {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    };
    // `--talk` (a click on the character under the centre: its subject
    // menu) and Atlantis II's start-up intro: `--intro`, `--no-intro`.
    let mut flag = |name: &str| match args.iter().position(|a| a == name) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    };
    let talk = flag("--talk");
    let options_menu = flag("--options-menu") || options_keys.is_some();
    let intro = match (flag("--intro"), flag("--no-intro")) {
        (true, true) => {
            eprintln!("--intro and --no-intro together");
            std::process::exit(2);
        }
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
    };
    let a2_start = atlantis_app::a2::A2Start {
        view,
        dialog: dialog.map(|d| d.max(0) as u32),
        menu,
        take: take
            .iter()
            .flat_map(|t| t.split(','))
            .map(str::to_string)
            .collect(),
        hold,
        bag,
        pointer,
        talk,
        intro,
        size,
        commands: command.into_iter().collect(),
        resolution,
        options_menu,
        options_keys: options_keys.clone().unwrap_or_default(),
        seed,
    };
    // `--original`, `--hd` and `--widescreen` hold for both games (for
    // this run: `settings.ini` is not changed). A headless run starts
    // from the defaults, a window from Atlantis II's `settings.ini`.
    let a2_flags_applied = |mut x: atlantis2_engine::Enhancements| {
        if original {
            x.fixes = atlantis2_engine::BugFixes::NONE;
            x.ui_ux = false;
        }
        x.hd |= force_hd;
        x.hd_options.widescreen |= force_wide;
        x
    };
    let a2_enhancements = a2_flags_applied(if original {
        atlantis2_engine::Enhancements::original()
    } else {
        atlantis2_engine::Enhancements::default()
    });
    // The options of one game only, refused for the other rather than
    // ignored.
    let first_game_flags: Vec<&str> = [
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
        ("--take", !a2_start.take.is_empty()),
        ("--hold", a2_start.hold.is_some()),
        ("--bag", a2_start.bag),
        ("--pointer", a2_start.pointer.is_some()),
        ("--talk", a2_start.talk),
        ("--intro", intro == Some(true)),
        ("--no-intro", intro == Some(false)),
        ("--size", size.is_some()),
        ("--resolution", resolution.is_some()),
        ("--options-menu", options_menu),
        ("--options-keys", options_keys.is_some()),
        ("--command", !a2_start.commands.is_empty()),
        ("--seed", seed.is_some()),
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
            a2_enhancements,
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
        let enhancements = a2_flags_applied(atlantis_app::a2::stored_settings().enhancements);
        let run = atlantis_app::a2::open(Path::new(&args[0]), &discs, &a2_start, enhancements)
            .and_then(|(engine, label)| {
                atlantis_app::a2::run_window(engine, label, fullscreen, size)
            });
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
    let mut el = EventLoop::new().unwrap_or_else(|e| {
        error_box(&format!("No window can be opened: {e}"));
        std::process::exit(1);
    });
    // A run with a game folder or a testing option: it neither counts
    // towards the support screen (launches, play time) nor shows it.
    let scripted = !args.is_empty() || new_game || puzzle.is_some() || force_support;
    // The first game's front end and the game library's screen (made
    // again when Atlantis II goes back to the library).
    let new_app = move |start: Start| {
        let mut session = Session::new(data_dir(), original, force_hd, force_wide, start);
        session.counts_play = !scripted;
        App::new(Sys::new(Mixer::new()), None)
            .with_session(session)
            .with_fullscreen(fullscreen)
    };
    let mut app = new_app(start);
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
    // Atlantis II started as the library's one game, without its screen:
    // Shift in its first moments goes back to the library (as for the
    // first game, [`Session::shift_grace`]).
    let a2_grace = Rc::new(std::cell::Cell::new(false));
    let grace = a2_grace.clone();
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
                // The library's one game, opened once (either game).
                let opened = (library.len() == 1 && !requested)
                    .then(|| first.as_deref().and_then(|d| open_any(d).ok()))
                    .flatten();
                let choice = startup(library.len(), requested, || opened.is_some());
                match choice {
                    Startup::FirstStart => match first_start() {
                        Some(FirstStart::Play(opened, dir)) => app.play_opened(opened, &dir),
                        Some(FirstStart::Library(dirs)) => {
                            if let Some(s) = app.session_mut() {
                                s.add_games(&dirs);
                            }
                            app.show_launcher(None);
                        }
                        None => {
                            eprintln!("{USAGE}");
                            return false;
                        }
                    },
                    Startup::PlayFirst => {
                        let (Some(opened), Some(dir)) = (opened, first) else {
                            unreachable!("the first game opened");
                        };
                        log::info!("game folder {}", dir.display());
                        app.play_opened(opened, &dir);
                        if let Some(s) = app.session_mut() {
                            s.shift_grace();
                            grace.set(s.wants_atlantis2());
                        }
                    }
                    Startup::Launcher => app.show_launcher(None),
                }
            }
        }
        // Atlantis II chosen: its own mode, after this event loop.
        !app.session().is_some_and(|s| s.wants_atlantis2())
    };
    let windowed = if due || force_support {
        app.show_support(data_dir(), Box::new(begin));
        true
    } else {
        begin(&mut app)
    };
    // On demand: the event loop runs again for Atlantis II when it is
    // chosen in the game library (a program has one event loop), and for
    // the library's screen when Atlantis II goes back to it.
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    let mut windowed = windowed;
    loop {
        if windowed {
            let run = el.run_app_on_demand(&mut app);
            app.save_play_time();
            if let Err(e) = run {
                error_box(&format!("The window failed: {e}"));
                std::process::exit(1);
            }
        }
        let atlantis2 = app.session_mut().and_then(|s| s.take_atlantis2());
        // The first game's window and sound end here.
        drop(app);
        let Some(dir) = atlantis2 else {
            return;
        };
        atlantis_app::watchdog::leave();
        let enhancements = a2_flags_applied(atlantis_app::a2::stored_settings().enhancements);
        let run = atlantis_app::a2::open(
            &dir,
            &[],
            &atlantis_app::a2::A2Start::default(),
            enhancements,
        )
        .and_then(|(engine, label)| {
            atlantis_app::a2::run_window_on(
                &mut el,
                engine,
                label,
                fullscreen,
                None,
                a2_grace.replace(false),
            )
        });
        match run {
            Ok(atlantis_app::a2::Ended::Quit) => return,
            // The options menu's "Game library...", or Shift held as it
            // started: the library's screen in the first game's window.
            Ok(atlantis_app::a2::Ended::Library) => {
                app = new_app(Start::Boot);
                app.show_launcher(None);
                windowed = true;
            }
            Err(e) => {
                error_box(&e);
                std::process::exit(1);
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

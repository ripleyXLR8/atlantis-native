//! Desktop entry point: `atlantis-native [<game dir>]`.
//!
//! Without a game folder (a double click), the folder chosen last time is
//! used if it still holds the game; else the system's folder picker asks
//! for it, and it is remembered (`game-folder.txt` in the per-user data
//! folder). Errors are shown in message boxes. Without a terminal the log
//! goes to `atlantis-native.log` in the same folder.
//!
//! `atlantis-native --headless <game dir> [frames]` runs the engine without
//! a window for a number of frames (default 600) with no input, and prints
//! what happened: a check that the game starts. The sound is mixed and
//! dropped, so that the voices end as they would.
//!
//! `--original` (before the game directory) turns every enhancement off:
//! the original game, bug for bug. `--new-game` skips the start-up videos
//! and the player screen.
//!
//! `--puzzle <n> [--object <id>]` (a testing aid) plays StartPuzzle `n` at
//! once, with object `id` in hand, reports its FinPuzzle, then starts a new
//! game.
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

    use atlantis_app::desktop::{choose_game, error_box, init_log, install_panic_hook, open_game};
    use atlantis_app::{options_with_settings, start_game, App};
    use atlantis_data::Game;
    use atlantis_engine::persist::{user_data_dir, DirStore};
    use atlantis_engine::{Enhancements, Options, SaveStore, Start};
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
    let new_game = flag("--new-game");
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
    let object = value("--object").unwrap_or(0);
    let start = match puzzle {
        Some(id) => Start::Puzzle { id, object },
        None if new_game => Start::NewGame,
        None => Start::Boot,
    };
    // The saves of each build in their own folder (the player lists of the
    // languages differ in their marker row).
    let store_for = |game: &Game| -> Rc<dyn SaveStore> {
        let dir = user_data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("atlantis-native-data"))
            .join(game.exe.build.name);
        log::info!("saves in {}", dir.display());
        Rc::new(DirStore::new(dir))
    };

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
            alive = runner.step(i * 16);
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

    if args.len() > 1 || args.first().is_some_and(|a| a.starts_with('-')) {
        error_box(&format!(
            "usage: atlantis-native [--original] [--new-game | --puzzle <n> [--object <id>]] [<game dir>]\n\n\
             '{}' not understood",
            args.join(" ")
        ));
        std::process::exit(2);
    }
    // The event loop first: on macOS winit must own the application before
    // the folder picker or a message box creates it.
    let el = EventLoop::new().unwrap_or_else(|e| {
        error_box(&format!("No window can be opened: {e}"));
        std::process::exit(1);
    });
    let game = match args.first() {
        Some(dir) => open_game(Path::new(dir)).unwrap_or_else(|e| {
            error_box(&e);
            std::process::exit(1);
        }),
        None => match choose_game() {
            Some(game) => game,
            None => {
                eprintln!(
                    "usage: atlantis-native [--original] [--new-game | --puzzle <n> [--object <id>]] [<game dir>]"
                );
                return;
            }
        },
    };
    let saves = store_for(&game);
    let options = options_with_settings(saves, original, start);
    let (sys, runner) = start_game(game, options);
    if let Err(e) = el.run_app(&mut App::new(sys, Some(runner))) {
        error_box(&format!("The window failed: {e}"));
        std::process::exit(1);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

//! Desktop entry point: `atlantis-native <game dir>`.
//!
//! `atlantis-native --headless <game dir> [frames]` runs the engine without
//! a window for a number of frames (default 600) with no input, and prints
//! what happened: a check that the game starts. The sound is mixed and
//! dropped, so that the voices end as they would.
//!
//! `--original` (before the game directory) turns every enhancement off:
//! the original game, bug for bug.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use atlantis_app::{start_game, App};
    use atlantis_data::{Game, StdDir};
    use atlantis_engine::audio::Mixer;
    use atlantis_engine::{Enhancements, Options};
    use winit::event_loop::EventLoop;

    env_logger::init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let enhancements = match args.iter().position(|a| a == "--original") {
        Some(i) => {
            args.remove(i);
            Enhancements::original()
        }
        None => Enhancements::default(),
    };
    let open = |dir: &str| {
        Game::open(Arc::new(StdDir::new(dir))).unwrap_or_else(|e| {
            eprintln!("{dir}: {e}");
            std::process::exit(1);
        })
    };

    if args.first().map(String::as_str) == Some("--headless") {
        let Some(dir) = args.get(1) else {
            eprintln!("usage: atlantis-native [--original] --headless <game dir> [frames]");
            std::process::exit(2);
        };
        let frames: u64 = args.get(2).and_then(|f| f.parse().ok()).unwrap_or(600);
        let trace = Rc::new(RefCell::new(Vec::new()));
        let options = Options {
            trace: Some(trace.clone()),
            enhancements,
            ..Options::default()
        };
        let (sys, mut runner) = start_game(open(dir), options);
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

    let Some(dir) = args.first() else {
        eprintln!("usage: atlantis-native [--original] <game dir>");
        // Without a game: the window with a test pattern.
        let el = EventLoop::new().unwrap();
        el.run_app(&mut App::new(
            atlantis_engine::sys::Sys::new(Mixer::new()),
            None,
        ))
        .unwrap();
        return;
    };
    let options = Options {
        enhancements,
        ..Options::default()
    };
    let (sys, runner) = start_game(open(dir), options);
    let el = EventLoop::new().unwrap();
    el.run_app(&mut App::new(sys, Some(runner))).unwrap();
}

#[cfg(target_arch = "wasm32")]
fn main() {}

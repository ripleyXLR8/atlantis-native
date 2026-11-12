//! Whole frames of [`Game::draw`] at 1365 × 768: the intro and the first
//! node with its animations and markers, as MD5s of the XRGB8888 words.
//! The hashes were taken from the plain per-pixel code before the drawing
//! was made faster (rows in parallel, the movie's rows and columns
//! computed once): any change to a pixel shows here.
//! Re-pinned for the live check of 2026-11-12 (atlantis4-re experiment
//! 2026-11-12-01): the cursor's corner truncated about the window's centre
//! (every frame) and the markers placed as the original's quads (the
//! node's frames). The timing test
//! (`--ignored`) prints how long each part of a frame takes.
//!
//! These tests need `ATLANTIS4_DATA` (see `atlantis4_data::testing`) and
//! skip cleanly with `ATLANTIS4_SKIP_DATA_TESTS=1`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use atlantis4_data::A4Files;
use atlantis4_engine::Game;
use atlantis4_script::{new_game, read_boot_vars, BootValue, EngineRef, Options, Script};
use md5::{Digest, Md5};

const W: u32 = 1365;
const H: u32 = 768;
/// The clock of these tests at the new game, in µs.
const T0: u64 = 5_000_000;

fn install() -> Option<PathBuf> {
    atlantis4_data::testing::install_dir()
}

fn ms(us: u64) -> f64 {
    us as f64 / 1000.0
}

fn md5_hex(px: &[u32]) -> String {
    let mut h = Md5::new();
    for p in px {
        h.update(p.to_le_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn boot(dir: &std::path::Path) -> (Rc<RefCell<Game>>, Script) {
    let (files, _) = A4Files::open_dir(dir).unwrap();
    let mut g = Game::new(files, H);
    g.clock_us = Some(Box::new(|| T0));
    g.now_ms = ms(T0);
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let boot = read_boot_vars(&engine, Options::default()).unwrap();
    let Some(BootValue::Str(first)) = boot.first_warp_path else {
        panic!("no firstWarpPath")
    };
    let script = new_game(&engine, Options::default(), &first).unwrap();
    (game, script)
}

fn frame(game: &Rc<RefCell<Game>>, pointer: Option<(f64, f64)>) -> Vec<u32> {
    let mut out = vec![0u32; (W * H) as usize];
    game.borrow_mut().draw(&mut out, W, H, pointer);
    out
}

/// The intro at a few times (the skip button shown for the last two),
/// then the first node after it at a few views and animation times; with
/// or without the GUI layers (the HUD's inventory button).
fn frames(dir: &std::path::Path, gui: bool) -> Vec<String> {
    let (game, _script) = boot(dir);
    game.borrow_mut().draw_gui = gui;
    let mut got = Vec::new();
    for (t, skip) in [
        (T0, false),
        (T0 + 1_000_000, false),
        (T0 + 17_560_000, false),
        (T0 + 40_000_000, true),
        (T0 + 63_000_000, true),
    ] {
        let mut g = game.borrow_mut();
        g.update_movie(ms(t));
        g.movie.as_mut().unwrap().skip_shown = skip;
        drop(g);
        got.push(md5_hex(&frame(&game, Some((700.5, 300.25)))));
    }
    let end = T0 + 1578 * 40_000;
    assert!(game.borrow_mut().update_movie(ms(end)).is_some());
    for (k, (yaw, pitch)) in [(0.0, 0.0), (90.0, -10.0), (200.0, 20.0), (-45.0, -40.0)]
        .into_iter()
        .enumerate()
    {
        let now = end + 1_000_000 + k as u64 * 733_000;
        {
            let mut g = game.borrow_mut();
            g.update(ms(now));
            g.view.yaw = yaw;
            g.view.pitch = pitch;
        }
        got.push(md5_hex(&frame(&game, Some((100.0, 650.0)))));
    }
    got
}

const FRAMES: [&str; 9] = [
    "06262b1ff64427cb303d908f7cda937c",
    "cbae9e96f23e2d433428350701f1181f",
    "b6d7e4e8461204b6e7fbb5610632fd27",
    "352eecf011f12a38843145b1c588a64d",
    "9a7699ae51bc814062df91c9de58e935",
    "955b52856fb166cb77975483f5ecd131",
    "363369fca3c4a36e5d42ff061251ce44",
    "bf3bca811cbe560279a604aa0bd6b436",
    "255f8037214627a6c4fae4c21bd77158",
];

/// [`FRAMES`] with the GUI layers: the intro's background covers the HUD;
/// on the node the inventory button is in the bottom-right corner (its
/// pixels alone differ, `tests/inventory.rs`). No GUI text shows in these
/// frames (no dialog, notification or open inventory), so drawing the
/// text layouts left them as they were.
const FRAMES_GUI: [&str; 9] = [
    "06262b1ff64427cb303d908f7cda937c",
    "cbae9e96f23e2d433428350701f1181f",
    "b6d7e4e8461204b6e7fbb5610632fd27",
    "352eecf011f12a38843145b1c588a64d",
    "9a7699ae51bc814062df91c9de58e935",
    "f43e1535fd3ae9a8fb4c1e785d2a51d7",
    "7f14c8e07b57a1b4e40859df6bd54fa9",
    "b361f7309a15a462792346bc6be7dcbc",
    "a099347555dd97c0c80a58875df9dc18",
];

#[test]
fn the_frames_with_the_gui_are_the_plain_codes() {
    let Some(dir) = install() else { return };
    let got = frames(&dir, true);
    if std::env::var_os("A4_PRINT_FRAMES").is_some() {
        for h in &got {
            println!("    \"{h}\",");
        }
    }
    assert_eq!(got, FRAMES_GUI);
}

#[test]
fn the_frames_are_the_plain_codes() {
    let Some(dir) = install() else { return };
    let got = frames(&dir, false);
    if std::env::var_os("A4_PRINT_FRAMES").is_some() {
        for h in &got {
            println!("    \"{h}\",");
        }
    }
    assert_eq!(got, FRAMES);
}

/// The viewer redraws only when [`Game::frame_key`] changes: it must
/// change with the movie's frame, the skip button, the cursor and the
/// view, and stay when nothing moved.
#[test]
fn the_frame_key_follows_the_picture() {
    let Some(dir) = install() else { return };
    let (game, _script) = boot(&dir);
    let p = Some((10.0, 10.0));
    let key = |g: &Rc<RefCell<Game>>, p| g.borrow().frame_key(W, H, p);
    frame(&game, p);
    let k0 = key(&game, p);
    assert_eq!(key(&game, p), k0);
    // Within frame 0 of the intro (40 ms each): the same picture.
    game.borrow_mut().update_movie(ms(T0 + 39_999));
    assert_eq!(key(&game, p), k0);
    game.borrow_mut().update_movie(ms(T0 + 40_000));
    let k1 = key(&game, p);
    assert_ne!(k1, k0);
    assert_ne!(key(&game, Some((11.0, 10.0))), k1);
    game.borrow_mut().movie.as_mut().unwrap().skip_shown = true;
    assert_ne!(key(&game, p), k1);
    let end = T0 + 1578 * 40_000;
    game.borrow_mut().update_movie(ms(end));
    let k2 = key(&game, p);
    game.borrow_mut().view.yaw += 0.5;
    assert_ne!(key(&game, p), k2);
}

/// Prints the time of each part of a frame (release build).
#[test]
#[ignore]
fn frame_times() {
    let Some(dir) = install() else { return };
    let (game, _script) = boot(&dir);
    let n = 50u64;
    // The movie: decode (update) and draw of each new frame.
    let (mut dec, mut draw) = (0f64, 0f64);
    let mut out = vec![0u32; (W * H) as usize];
    for k in 1..=n {
        let t = T0 + k * 40_000;
        let a = Instant::now();
        game.borrow_mut().update_movie(ms(t));
        let b = Instant::now();
        game.borrow_mut().draw(&mut out, W, H, Some((10.0, 10.0)));
        let c = Instant::now();
        dec += (b - a).as_secs_f64();
        draw += (c - b).as_secs_f64();
    }
    println!(
        "movie: update (decode + RGBA) {:.2} ms, draw {:.2} ms",
        dec * 1e3 / n as f64,
        draw * 1e3 / n as f64
    );
    let end = T0 + 1578 * 40_000;
    game.borrow_mut().update_movie(ms(end));
    // The node: a still view (cached background), then a turning one.
    for turning in [false, true] {
        let (mut upd, mut draw) = (0f64, 0f64);
        for k in 1..=n {
            let t = end + k * 33_333;
            let a = Instant::now();
            {
                let mut g = game.borrow_mut();
                g.update(ms(t));
                if turning {
                    g.view.yaw = k as f32 * 0.5;
                }
            }
            let b = Instant::now();
            game.borrow_mut().draw(&mut out, W, H, Some((10.0, 10.0)));
            let c = Instant::now();
            upd += (b - a).as_secs_f64();
            draw += (c - b).as_secs_f64();
        }
        println!(
            "node ({}): update {:.2} ms, draw {:.2} ms",
            if turning { "turning" } else { "still" },
            upd * 1e3 / n as f64,
            draw * 1e3 / n as f64
        );
    }
    // The mixer: one second of the node's music and ambience.
    let core = game.borrow().sounds.core();
    let mut buf = vec![0i16; 2 * 44_100];
    let a = Instant::now();
    core.lock().unwrap().pull(&mut buf);
    println!(
        "sound: one second of mix {:.2} ms",
        a.elapsed().as_secs_f64() * 1e3
    );
}

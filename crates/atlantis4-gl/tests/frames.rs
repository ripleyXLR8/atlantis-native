//! Whole frames of the game drawn by the processor and by the card,
//! compared within [`atlantis4_gl::check`]'s tolerance: the intro (with
//! and without the skip button), the first node at four views with its
//! animations, markers, HUD and cursor (the scenes of the engine's
//! `tests/frames.rs`), and a narration and a conversation at the next
//! node (text, the talking head's video, the topics). The ignored
//! `frame_times` prints the milliseconds per frame of both.
//!
//! Needs `ATLANTIS4_DATA` (see `atlantis4_data::testing`) and a headless
//! OpenGL context (see `common`); skips without either. On WSL the card
//! is reached with `GALLIUM_DRIVER=d3d12` (else llvmpipe draws).

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use atlantis4_data::A4Files;
use atlantis4_engine::Game;
use atlantis4_gl::check::{compare, Diff};
use atlantis4_gl::Renderer;
use atlantis4_script::{new_game, read_boot_vars, BootValue, EngineRef, Options, Script};

const W: u32 = 1365;
const H: u32 = 768;
const T0: u64 = 5_000_000;

fn ms(us: u64) -> f64 {
    us as f64 / 1000.0
}

type Booted = (Rc<RefCell<Game>>, Script, Rc<RefCell<u64>>);

/// A new game at `node` (the first node, through the intro, when `None`).
fn boot(files: A4Files, node: Option<&str>) -> Booted {
    let clock = Rc::new(RefCell::new(T0));
    let mut g = Game::new(files, H);
    let c = clock.clone();
    g.clock_us = Some(Box::new(move || *c.borrow()));
    g.update(ms(T0));
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let first = match node {
        Some(n) => n.as_bytes().to_vec(),
        None => {
            let boot = read_boot_vars(&engine, Options::default()).unwrap();
            let Some(BootValue::Str(first)) = boot.first_warp_path else {
                panic!("no firstWarpPath")
            };
            first
        }
    };
    let script = new_game(&engine, Options::default(), &first).unwrap();
    (game, script, clock)
}

/// Steps the clock by 20 ms to `until` µs, running the callbacks the
/// dialogs need.
fn run(game: &Rc<RefCell<Game>>, script: &mut Script, clock: &Rc<RefCell<u64>>, until: u64) {
    while *clock.borrow() < until {
        *clock.borrow_mut() += 20_000;
        let now = ms(*clock.borrow());
        game.borrow_mut().update(now);
        game.borrow_mut().update_movie(now);
        let mut cbs = game.borrow_mut().sound_callbacks();
        cbs.extend(game.borrow_mut().dialog_callbacks());
        for (name, args) in cbs {
            script.callback(&name, &args).unwrap();
        }
    }
}

/// The frame on both: the processor's, the card's, their difference.
fn both(
    game: &Rc<RefCell<Game>>,
    r: &mut Renderer,
    pointer: Option<(f64, f64)>,
) -> (Vec<u32>, Vec<u32>, Diff) {
    let mut cpu = vec![0u32; (W * H) as usize];
    game.borrow_mut().draw(&mut cpu, W, H, pointer);
    let mut f = r.begin(W, H);
    game.borrow_mut().draw_on(&mut f, pointer);
    f.finish();
    let gpu = r.read_pixels();
    let d = compare(&cpu, &gpu, W as usize);
    (cpu, gpu, d)
}

/// Writes a frame as a binary PPM into `A4_GL_SHOTS` when set.
fn shot(name: &str, px: &[u32]) {
    let Some(dir) = std::env::var_os("A4_GL_SHOTS") else {
        return;
    };
    let mut out = format!("P6\n{W} {H}\n255\n").into_bytes();
    for p in px {
        out.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
    }
    std::fs::write(std::path::Path::new(&dir).join(name), out).unwrap();
}

/// Every scene's name and difference.
fn scenes(files: &A4Files, r: &mut Renderer) -> Vec<(String, Diff)> {
    let mut got = Vec::new();
    let mut look = |name: String, game: &Rc<RefCell<Game>>, r: &mut Renderer, p| {
        let (cpu, gpu, d) = both(game, r, p);
        shot(&format!("{name}-cpu.ppm"), &cpu);
        shot(&format!("{name}-gpu.ppm"), &gpu);
        eprintln!("{name}: {d}");
        got.push((name, d));
    };
    let (game, _script, _clock) = boot(files.clone(), None);
    for (t, skip) in [
        (T0, false),
        (T0 + 17_560_000, false),
        (T0 + 40_000_000, true),
    ] {
        {
            let mut g = game.borrow_mut();
            g.update_movie(ms(t));
            g.movie.as_mut().unwrap().skip_shown = skip;
        }
        look(
            format!("intro-{}", t / 1000),
            &game,
            r,
            Some((700.5, 300.25)),
        );
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
        look(
            format!("node-{yaw}-{pitch}"),
            &game,
            r,
            Some((100.0, 650.0)),
        );
    }
    drop(game);
    // A narration (its text in the bar), then a conversation (the head's
    // video, the topics).
    let (game, mut script, clock) = boot(files.clone(), Some("3D/Centre17_C1/0001.wp"));
    run(&game, &mut script, &clock, T0 + 600_000);
    assert!(game.borrow().dialogs.shown());
    look("narration".into(), &game, r, None);
    run(&game, &mut script, &clock, T0 + 120_000_000);
    script.run_chunk(b"ShowDialog()", b"test");
    run(&game, &mut script, &clock, T0 + 121_500_000);
    look("conversation".into(), &game, r, Some((400.0, 300.0)));
    got
}

fn setup() -> Option<(common::Headless, A4Files)> {
    let dir = atlantis4_data::testing::install_dir()?;
    let h = common::context()?;
    eprintln!("device: {}", h.device);
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    Some((h, files))
}

#[test]
fn the_cards_frames_are_the_processors_within_the_tolerance() {
    let Some((mut h, files)) = setup() else {
        return;
    };
    let mut r = Renderer::new(h.gl.take().unwrap()).unwrap();
    eprintln!("GL: {}", r.info);
    for (name, d) in scenes(&files, &mut r) {
        assert!(d.within(), "{name}: {d}");
    }
}

/// Milliseconds per frame on the processor and on the card (the card's
/// time includes waiting for it, `glFinish`, not the read back).
#[test]
#[ignore]
fn frame_times() {
    let Some((mut h, files)) = setup() else {
        return;
    };
    let mut r = Renderer::new(h.gl.take().unwrap()).unwrap();
    println!("GL: {}", r.info);
    let (game, _script, _clock) = boot(files, None);
    let n = 50u64;
    let mut cpu = vec![0u32; (W * H) as usize];
    let mut time = |game: &Rc<RefCell<Game>>, r: &mut Renderer, step: &dyn Fn(&mut Game, u64)| {
        let (mut tc, mut tg) = (0f64, 0f64);
        for k in 1..=n {
            step(&mut game.borrow_mut(), k);
            let a = Instant::now();
            game.borrow_mut().draw(&mut cpu, W, H, Some((10.0, 10.0)));
            let b = Instant::now();
            let mut f = r.begin(W, H);
            game.borrow_mut().draw_on(&mut f, Some((10.0, 10.0)));
            f.finish();
            r.finish_gpu();
            let c = Instant::now();
            tc += (b - a).as_secs_f64();
            tg += (c - b).as_secs_f64();
        }
        (tc * 1e3 / n as f64, tg * 1e3 / n as f64)
    };
    let (c, g) = time(&game, &mut r, &|g, k| {
        g.update_movie(ms(T0 + k * 40_000));
    });
    println!("movie frame: processor {c:.2} ms, card {g:.2} ms");
    let end = T0 + 1578 * 40_000;
    game.borrow_mut().update_movie(ms(end));
    for turning in [false, true] {
        let (c, g) = time(&game, &mut r, &|g, k| {
            g.update(ms(end + k * 33_333));
            if turning {
                g.view.yaw = k as f32 * 0.5;
            }
        });
        println!(
            "node ({}): processor {c:.2} ms, card {g:.2} ms",
            if turning { "turning" } else { "still" }
        );
    }
}

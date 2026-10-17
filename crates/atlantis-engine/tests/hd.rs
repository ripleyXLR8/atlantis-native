//! The port's HD mode: a new game played headless twice in lockstep, once
//! as the original and once with the HD layer (tile 48, 1920x1440), up to
//! the first dialog line and into the navigation of place 1. The 640x480
//! frames must be the same in both runs; the HD run must show its layer on
//! the panorama frames, the talking head's line included.
//!
//! The data tests need the game: set `ATLANTIS_DATA` to the game directory;
//! without it they are skipped. Run them with `--release`. With
//! `ATLANTIS_DUMP_DIR` set, the test writes a navigation frame and a dialog
//! frame there as PPM, the original scaled (nearest) and the HD picture.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::hd::{compose, rgb565_to_888};
use atlantis_engine::sys::{Input, Runner, Sys};
use atlantis_engine::warp::{hd_size, SCREEN_H, SCREEN_W};
use atlantis_engine::{Event, Options, Start};

const TILE: usize = 48;

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

struct Run {
    sys: Rc<Sys>,
    runner: Runner,
    trace: Rc<RefCell<Vec<Event>>>,
    sound: Vec<f32>,
    /// Time spent in the engine's steps, and in the line's and the
    /// navigation's steps.
    busy: Duration,
    last: Duration,
    line: Duration,
    nav: Duration,
}

impl Run {
    fn new(g: Game, tile: Option<usize>) -> Run {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let sys = Sys::new(Mixer::new());
        sys.set_hd_tile(tile);
        let opts = Options {
            trace: Some(trace.clone()),
            start: Start::NewGame,
            ..Options::default()
        };
        let runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
        Run {
            sys,
            runner,
            trace,
            sound: vec![0.0; 2 * (22050 * 16 / 1000)],
            busy: Duration::ZERO,
            last: Duration::ZERO,
            line: Duration::ZERO,
            nav: Duration::ZERO,
        }
    }

    fn step(&mut self, now: u64) -> bool {
        let t = Instant::now();
        let alive = self.runner.step(now);
        self.last = t.elapsed();
        self.busy += self.last;
        self.sys.mixer.fill(&mut self.sound, 22050, 2);
        alive
    }
}

/// The frame scaled to the HD size: nearest (the original's), or composed
/// with the HD layer.
fn picture(sys: &Sys, hd: bool) -> (usize, usize, Vec<u32>) {
    let (w, h) = hd_size(TILE);
    let mut out = vec![0u32; w * h];
    let screen = sys.screen.borrow();
    if hd {
        compose(&screen, &sys.hd.borrow(), &mut out, w, 0, 0, w, h);
    } else {
        let lut = rgb565_to_888();
        for y in 0..h {
            for x in 0..w {
                out[y * w + x] =
                    lut[usize::from(screen[y * SCREEN_H / h * SCREEN_W + x * SCREEN_W / w])];
            }
        }
    }
    (w, h, out)
}

fn dump(name: &str, sys: &Sys, hd: bool) {
    let Some(dir) = std::env::var_os("ATLANTIS_DUMP_DIR") else {
        return;
    };
    let (w, h, px) = picture(sys, hd);
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for p in px {
        out.extend([(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    let path = std::path::Path::new(&dir).join(format!("{name}.ppm"));
    std::fs::write(&path, out).expect("cannot write the dump");
    eprintln!("wrote {}", path.display());
}

/// The share of the frame's pixels taken from the HD layer.
fn hd_share(sys: &Sys) -> f64 {
    let screen = sys.screen.borrow();
    let layer = sys.hd.borrow();
    let same = screen
        .iter()
        .zip(&layer.base)
        .filter(|(a, b)| a == b)
        .count();
    same as f64 / screen.len() as f64
}

#[test]
fn hd_mode_keeps_the_frames_and_draws_the_view() {
    let (Some(g1), Some(g2)) = (game("hd_mode"), game("hd_mode")) else {
        return;
    };
    let mut orig = Run::new(g1, None);
    let mut hd = Run::new(g2, Some(TILE));
    let (mut in_line, mut line_hd, mut place_frames) = (0u32, 0u32, None::<u32>);
    let mut nav_hd = 0u32;
    let mut frame = 0u64;
    loop {
        let now = frame * 16;
        assert!(orig.step(now), "the engine ended");
        assert!(hd.step(now), "the HD engine ended");
        frame += 1;
        assert!(frame < 60_000, "no navigation reached");
        assert!(
            *orig.sys.screen.borrow() == *hd.sys.screen.borrow(),
            "frame {frame}: the 640x480 frames differ"
        );
        assert!(!orig.sys.hd_shown());
        let shown = hd.sys.hd_shown();
        let t = hd.trace.borrow();
        if shown {
            let layer = hd.sys.hd.borrow();
            assert_eq!((layer.width, layer.height), hd_size(TILE));
            assert_eq!(layer.tile, TILE);
        }
        // The pilot's talking head line.
        if matches!(t.last(), Some(Event::Voice(v)) if v == "WAV\\ATA0001A.APC") {
            in_line += 1;
            orig.line += orig.last;
            hd.line += hd.last;
            if shown {
                line_hd += 1;
                // The UBB body and the head are in the layer: most of
                // the frame (all but the subtitle) comes from it.
                let share = hd_share(&hd.sys);
                assert!(share > 0.9, "frame {frame}: {share} of the line from HD");
            }
            if in_line == 90 {
                drop(t);
                dump("dialog_original", &hd.sys, false);
                dump("dialog_hd", &hd.sys, true);
                continue;
            }
        }
        if place_frames.is_none()
            && t.iter()
                .any(|e| matches!(e, Event::PlaceEntered { place: 1, .. }))
        {
            place_frames = Some(0);
        }
        drop(t);
        if let Some(n) = place_frames.as_mut() {
            *n += 1;
            orig.nav += orig.last;
            hd.nav += hd.last;
            if hd.sys.mouse_locked() {
                // Turn the view a little, in both runs.
                if *n % 3 == 0 && *n < 200 {
                    for r in [&orig, &hd] {
                        r.sys.push_input(Input::MouseDelta { dx: 4, dy: 0 });
                    }
                }
                if shown {
                    nav_hd += 1;
                    let share = hd_share(&hd.sys);
                    assert!(share > 0.95, "frame {frame}: {share} of the view from HD");
                }
            }
            if *n == 320 {
                dump("navigation_original", &hd.sys, false);
                dump("navigation_hd", &hd.sys, true);
                break;
            }
        }
    }
    let ms = |d: Duration, n: u32| d.as_secs_f64() * 1e3 / f64::from(n.max(1));
    eprintln!(
        "{frame} frames; line {in_line} frames ({line_hd} HD); navigation {nav_hd} HD frames; \
         engine time: original {:.1} s, HD {:.1} s; per line frame {:.2} / {:.2} ms; \
         per navigation frame {:.2} / {:.2} ms",
        orig.busy.as_secs_f64(),
        hd.busy.as_secs_f64(),
        ms(orig.line, in_line),
        ms(hd.line, in_line),
        ms(orig.nav, 320),
        ms(hd.nav, 320),
    );
    assert!(in_line > 60, "{in_line} line frames");
    assert!(
        line_hd * 10 >= in_line * 9,
        "{line_hd} of {in_line} line frames HD"
    );
    assert!(nav_hd > 100, "{nav_hd} navigation frames HD");
}

/// The cost of the HD view and of the front end's composition per tile
/// size (`cargo test --release --test hd -- --ignored --nocapture`).
#[test]
#[ignore]
fn hd_timings() {
    use atlantis_engine::hd::HdLayer;
    use atlantis_engine::warp::{Warp, PANO_W};
    let pano: Vec<u32> = (0..PANO_W as u32 * 768)
        .map(|i| i.wrapping_mul(2_654_435_761) >> 8)
        .collect();
    let screen = vec![0u16; SCREEN_W * SCREEN_H];
    for tile in [24, 32, 36, 48, 64] {
        let (w, h) = hd_size(tile);
        let mut out = vec![0u32; w * h];
        let mut warp = Warp::new();
        let n = 20;
        let t = Instant::now();
        for i in 0..n {
            warp.set_alpha(i as f32 * 0.05);
            warp.render_bilinear(&pano, tile, &mut out);
        }
        let view = t.elapsed().as_secs_f64() * 1e3 / f64::from(n);
        let mut layer = HdLayer::default();
        layer.resize(tile);
        let mut win = vec![0u32; w * h];
        let t = Instant::now();
        for _ in 0..n {
            compose(&screen, &layer, &mut win, w, 0, 0, w, h);
        }
        let comp = t.elapsed().as_secs_f64() * 1e3 / f64::from(n);
        eprintln!("tile {tile} ({w}x{h}): HD view {view:.2} ms, composition {comp:.2} ms");
    }
}

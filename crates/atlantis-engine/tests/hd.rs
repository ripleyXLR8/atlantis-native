//! The port's HD mode: a new game played headless twice in lockstep, once
//! as the original and once with the HD layer (tile 48, 1920x1440), up to
//! the first dialog line and into the navigation of place 1. The 640x480
//! frames must be the same in both runs; the HD run must show its layer on
//! the panorama frames, the talking head's line included.
//!
//! The data tests need the game: set `ATLANTIS_DATA` to the game directory;
//! without it they are skipped. Run them with `--release`. With
//! `ATLANTIS_DUMP_DIR` set, the test writes there as PPM a navigation
//! frame, a dialog frame with its subtitle, the options menu and the
//! inventory bar over the view: the original scaled (nearest), the HD
//! picture as first released (bilinear view, the rest nearest; a third
//! run with those options) and with the default HD options (bicubic and
//! sharpened view, smooth interface, sharp subtitles).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::hd::{
    compose, rgb565_to_888, HdCompositor, HdFrame, HdOptions, HdText, PanoFilter,
};
use atlantis_engine::options_menu::{MenuValues, OptionsMenu};
use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::sys::{Input, Runner, Sys};
use atlantis_engine::warp::{hd_size, SCREEN_H, SCREEN_W};
use atlantis_engine::{Enhancements, Graphics};
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

/// How a dump shows a frame.
#[derive(Clone, Copy, PartialEq)]
enum Look {
    /// The original, scaled (nearest).
    Original,
    /// The HD layer, the rest nearest (the first HD mode).
    Plain,
    /// The HD compositor with the default options.
    Default,
}

/// The frame of `sys` (or `screen` over it, with `menu_text`) scaled to
/// the HD size as `look` shows it.
fn picture(
    sys: &Sys,
    look: Look,
    screen: Option<&[u16]>,
    menu_text: Option<&HdText>,
) -> (usize, usize, Vec<u32>) {
    let (w, h) = hd_size(TILE);
    let mut out = vec![0u32; w * h];
    let own = sys.screen.borrow();
    let screen = screen.unwrap_or(&own);
    match look {
        Look::Original => {
            let lut = rgb565_to_888();
            for y in 0..h {
                for x in 0..w {
                    out[y * w + x] =
                        lut[usize::from(screen[y * SCREEN_H / h * SCREEN_W + x * SCREEN_W / w])];
                }
            }
        }
        Look::Plain => compose(screen, &sys.hd.borrow(), &mut out, w, 0, 0, w, h),
        Look::Default => {
            let layer = sys.hd.borrow();
            let text = sys.hd_text.borrow();
            let mut texts: Vec<&HdText> = Vec::new();
            if sys.hd_text_shown() {
                texts.push(&text);
            }
            if let Some(t) = menu_text {
                texts.push(t);
            }
            let f = HdFrame {
                screen,
                layer: sys.hd_shown().then_some(&*layer),
                texts: &texts,
                photo: sys.photo_shown(),
            };
            HdCompositor::new().compose(f, HdOptions::default(), &mut out, w, 0, 0, w, h);
        }
    }
    (w, h, out)
}

fn write_ppm(name: &str, w: usize, h: usize, px: &[u32]) {
    let Some(dir) = std::env::var_os("ATLANTIS_DUMP_DIR") else {
        return;
    };
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for p in px {
        out.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
    }
    let path = std::path::Path::new(&dir).join(format!("{name}.ppm"));
    std::fs::write(&path, out).expect("cannot write the dump");
    eprintln!("wrote {}", path.display());
}

fn dump(name: &str, sys: &Sys, look: Look) {
    if std::env::var_os("ATLANTIS_DUMP_DIR").is_none() {
        return;
    }
    let (w, h, px) = picture(sys, look, None, None);
    write_ppm(name, w, h, &px);
}

/// The frame as the original, the first HD mode (`plain`, the run with
/// those options) and the default HD options (`hd`).
fn dump_all(name: &str, hd: &Sys, plain: Option<&Sys>) {
    dump(&format!("{name}_original"), hd, Look::Original);
    if let Some(p) = plain {
        dump(&format!("{name}_hd_plain"), p, Look::Plain);
    }
    dump(&format!("{name}_hd"), hd, Look::Default);
}

/// The options menu over the frame, nearest and with the defaults.
fn dump_menu(name: &str, sys: &Sys) {
    if std::env::var_os("ATLANTIS_DUMP_DIR").is_none() {
        return;
    }
    let font = SubtitleFont::new();
    let mut menu = OptionsMenu::new();
    menu.open();
    let values = MenuValues {
        graphics: Graphics::Hd,
        hd: HdOptions::default(),
        enhancements: Enhancements::default(),
    };
    let mut frame = sys.screen.borrow().clone();
    let mut text = HdText::default();
    menu.draw_recording(&mut frame, &font, values, Some(&mut text));
    let (w, h, px) = picture(sys, Look::Plain, Some(&frame), None);
    write_ppm(&format!("{name}_hd_plain"), w, h, &px);
    let (w, h, px) = picture(sys, Look::Default, Some(&frame), Some(&text));
    write_ppm(&format!("{name}_hd"), w, h, &px);
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
    // Dumping: a third run with the first HD mode's options, for the
    // pictures before and after.
    let mut plain = std::env::var_os("ATLANTIS_DUMP_DIR")
        .and_then(|_| game("hd_mode"))
        .map(|g| {
            let r = Run::new(g, Some(TILE));
            r.sys.set_hd_options(HdOptions {
                panorama: PanoFilter::Bilinear,
                sharpen: false,
                smooth_interface: false,
                sharp_subtitles: false,
                widescreen: false,
            });
            r
        });
    let (mut in_line, mut line_hd, mut place_frames) = (0u32, 0u32, None::<u32>);
    let mut nav_hd = 0u32;
    let mut frame = 0u64;
    loop {
        let now = frame * 16;
        assert!(orig.step(now), "the engine ended");
        assert!(hd.step(now), "the HD engine ended");
        if let Some(p) = plain.as_mut() {
            assert!(p.step(now));
            assert!(*p.sys.screen.borrow() == *hd.sys.screen.borrow());
        }
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
                // The subtitle is in the text record, drawn sharp.
                assert!(hd.sys.hd_text_shown());
                assert!(!hd.sys.hd_text.borrow().items.is_empty());
                dump_all("dialog", &hd.sys, plain.as_ref().map(|p| &*p.sys));
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
                    for r in [Some(&orig), Some(&hd), plain.as_ref()]
                        .into_iter()
                        .flatten()
                    {
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
                dump_all("navigation", &hd.sys, plain.as_ref().map(|p| &*p.sys));
                dump_menu("menu", &hd.sys);
                if plain.is_none() {
                    break;
                }
                // The inventory bar (a right click), when dumping.
                for r in [Some(&orig), Some(&hd), plain.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    let (x, y) = r.sys.mouse();
                    let button = atlantis_engine::sys::MouseButton::Right;
                    r.sys.push_input(Input::MouseDown { button, x, y });
                    r.sys.push_input(Input::MouseUp { button, x, y });
                }
            }
            if *n == 420 {
                dump_all("bar", &hd.sys, plain.as_ref().map(|p| &*p.sys));
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
    use atlantis_engine::hd::{HdCompositor, HdFrame, HdLayer, HdOptions};
    use atlantis_engine::upscale::{
        prepare_panorama, sharpen_panorama, PanoramaPrep, SHARPEN_AMOUNT,
    };
    use atlantis_engine::warp::{Warp, PANO_W};
    let pano: Vec<u32> = (0..PANO_W as u32 * 768)
        .map(|i| i.wrapping_mul(2_654_435_761) >> 8)
        .collect();
    let n = 20;
    let time = |f: &mut dyn FnMut(usize)| {
        let t = Instant::now();
        for i in 0..n {
            f(i);
        }
        t.elapsed().as_secs_f64() * 1e3 / n as f64
    };
    let mut sharp = Vec::new();
    let sharpen = time(&mut |_| sharpen_panorama(&pano, PANO_W, SHARPEN_AMOUNT, &mut sharp));
    let surface: Vec<u16> = pano.iter().map(|&p| (p >> 3) as u16).collect();
    let mut prepared = Vec::new();
    let prep_sharp = time(&mut |_| prepare_panorama(&surface, PANO_W, true, false, &mut prepared));
    let prep_double = time(&mut |_| prepare_panorama(&surface, PANO_W, false, true, &mut prepared));
    let prep_both = time(&mut |_| prepare_panorama(&surface, PANO_W, true, true, &mut prepared));
    let tiled_full = time(&mut |_| {
        PanoramaPrep::new().update(&surface, PANO_W, true, true);
    });
    let mut prep = PanoramaPrep::new();
    prep.update(&surface, PANO_W, true, true);
    let mut moving = surface.clone();
    let sprite = time(&mut |i| {
        // A 100x150 sprite's frame.
        for y in 300..450 {
            for x in 900..1000 {
                moving[y * PANO_W + x] = (i * 977 + x) as u16;
            }
        }
        prep.update(&moving, PANO_W, true, true);
    });
    eprintln!(
        "panorama (2048x768), whole-plane reference: sharpening (packed) {sharpen:.2} ms; \
         sharpened {prep_sharp:.2} ms, doubled {prep_double:.2} ms, both {prep_both:.2} ms; \
         tiles: new panorama {tiled_full:.2} ms, a sprite's frame {sprite:.2} ms"
    );
    let pano2 = prepared.clone();
    // A menu-like screen: flat boxes, text-like noise.
    let screen: Vec<u16> = (0..SCREEN_W * SCREEN_H)
        .map(|i| {
            let (x, y) = (i % SCREEN_W, i / SCREEN_W);
            if (x / 40 + y / 30) % 3 == 0 {
                ((x * 7 + y * 13) % 5) as u16 * 0x2104
            } else {
                0x18e3
            }
        })
        .collect();
    for tile in [24, 32, 36, 48, 64] {
        let (w, h) = hd_size(tile);
        let mut out = vec![0u32; w * h];
        let mut warp = Warp::new();
        let bilinear = time(&mut |i| {
            warp.set_alpha(i as f32 * 0.05);
            warp.render_bilinear(&pano, tile, &mut out);
        });
        let bicubic = time(&mut |i| {
            warp.set_alpha(i as f32 * 0.05);
            warp.render_bicubic(&pano, tile, &mut out);
        });
        let doubled = time(&mut |i| {
            warp.set_alpha(i as f32 * 0.05);
            warp.render_bilinear_2x(&pano2, tile, &mut out);
        });
        let mut layer = HdLayer::default();
        layer.resize(tile, 0);
        let mut win = vec![0u32; w * h];
        let comp = time(&mut |_| compose(&screen, &layer, &mut win, w, 0, 0, w, h));
        // The new composition: the layer everywhere but a bar (cursor-like
        // changes each frame); a full screen of interface art, first frame
        // (xBR of everything) and still frames; a video frame.
        let mut c = HdCompositor::new();
        let opts = HdOptions::default();
        layer.base.copy_from_slice(&screen);
        let mut s2 = screen.clone();
        let bar = time(&mut |i| {
            for y in 400..440 {
                s2[y * SCREEN_W + 20 + i] = 0xffff;
            }
            let f = HdFrame {
                screen: &s2,
                layer: Some(&layer),
                texts: &[],
                photo: false,
            };
            c.compose(f, opts, &mut win, w, 0, 0, w, h);
        });
        let ui_frame = |photo| HdFrame {
            screen: &screen,
            layer: None,
            texts: &[],
            photo,
        };
        let full = time(&mut |_| {
            let mut c = HdCompositor::new();
            c.compose(ui_frame(false), opts, &mut win, w, 0, 0, w, h);
        });
        let still = time(&mut |_| c.compose(ui_frame(false), opts, &mut win, w, 0, 0, w, h));
        let video = time(&mut |_| c.compose(ui_frame(true), opts, &mut win, w, 0, 0, w, h));
        eprintln!(
            "tile {tile} ({w}x{h}): HD view bilinear {bilinear:.2} ms, bicubic direct {bicubic:.2} ms, \
             2x + bilinear {doubled:.2} ms; \
             composition: nearest {comp:.2} ms, layer + changing bar {bar:.2} ms, \
             interface xBR first frame {full:.2} ms, still {still:.2} ms, video {video:.2} ms"
        );
    }
}

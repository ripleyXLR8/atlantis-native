//! A headless tour of Tibet: pictures of views with their place
//! animations, and a recorded walk from the first view (mouse turn, click,
//! the move's turn, the transition video, the arrival, a second move).
//!
//! Usage: `tibet_tour <install dir> <discs dir> <out dir>`; writes
//! `tibet-*.png` and `tibet-sequence.txt` into the out dir. 59.94 passes a
//! second, a 15 Hz tick every fourth pass, `rand` seeded with 1000.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use atlantis2_data::A2Files;
use atlantis2_engine::anim::MsvcRand;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const W: usize = 640;
const H: usize = 480;

/// The direction of a face texel (the inverse of the cube's texture
/// mapping, see `texel_of` in the engine), as (yaw, pitch) in radians.
fn texel_angles(face: usize, col: f64, row: f64) -> (f32, f32) {
    let u = (col - 1.0) / 510.0 * 2.0 - 1.0;
    let v = (row - 1.0) / 510.0 * 2.0 - 1.0;
    let [x, y, z] = match face {
        0 => [u, v, 1.0],
        1 => [-u, v, -1.0],
        2 => [u, -1.0, v],
        3 => [u, 1.0, -v],
        4 => [-1.0, v, u],
        _ => [1.0, v, -u],
    };
    (z.atan2(x) as f32, y.atan2((x * x + z * z).sqrt()) as f32)
}

fn open(install: &Path, discs: &Path) -> Engine {
    let (files, _) = A2Files::open_dir(install, &[discs.to_path_buf()]).expect("game folder");
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Double, W as u32, H as u32).unwrap();
    e.anim.rand = MsvcRand(1000);
    e.trace = true;
    e
}

fn save(out: &Path, name: &str, screen: &[u16]) {
    let path = out.join(name);
    std::fs::write(&path, encode_rgb565(screen, W, H)).unwrap();
    println!("{}", path.display());
}

/// A view looking at the box of one of its animations (the slot whose
/// SPW name starts with `prefix`), after `ticks` ticks of animation.
fn picture(install: &Path, discs: &Path, out: &Path, view: u32, prefix: &str, ticks: u32) {
    let mut e = open(install, discs);
    e.enter_at(view, 0.0, 0.0).unwrap();
    let slot = e
        .anim
        .slots
        .iter()
        .find(|s| s.name.to_ascii_lowercase().starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix} in view {view}"));
    let (face, b) = (0..6)
        .find_map(|f| slot.frame_box(f).map(|b| (f, b)))
        .expect("a face");
    let (yaw, pitch) = texel_angles(
        face,
        f64::from(b.x) + f64::from(b.w) / 2.0,
        f64::from(b.y) + f64::from(b.h) / 2.0,
    );
    // Keep the original's ±70° pitch limit.
    let pitch = pitch.clamp(-1.2, 1.2);
    e.enter_at(view, yaw, pitch).unwrap();
    let mut screen = vec![0u16; W * H];
    for i in 0..ticks * 4 {
        if i % 4 == 0 {
            e.tick();
        }
        e.pass(Some(&mut screen));
    }
    let (y, p) = e.angles_deg();
    let active = e.anim.slots.iter().filter(|s| s.active).count();
    println!(
        "view {view}: {} slots, {active} shown, looking at {prefix} on face {face} ({y:.1}, {p:.1})",
        e.anim.slots.len()
    );
    save(out, &format!("tibet-view{view}-{prefix}.png"), &screen);
}

/// One pass (a tick every fourth), logged when something happened.
fn step(e: &mut Engine, screen: &mut [u16], log: &mut String, pass: &mut u64, what: &str) {
    if (*pass).is_multiple_of(4) {
        e.tick();
    }
    e.pass(Some(screen));
    let (y, p) = e.angles_deg();
    let evs: Vec<String> = e
        .events
        .drain(..)
        .filter(|ev| !matches!(ev, Event::SpwLoad { .. } | Event::VideoFrame(_)))
        .map(|ev| format!("{ev:?}"))
        .collect();
    if !evs.is_empty() || !what.is_empty() {
        let _ = writeln!(
            log,
            "{:7.3} s pass {:4} tick {:3} view {:2} {} yaw {y:8.3} pitch {p:7.3} zone {:2}{}{}",
            *pass as f64 * PASS,
            *pass,
            e.ticks,
            e.place().1,
            if e.loop_type == LoopType::Video {
                "video"
            } else {
                "warp "
            },
            e.centre_index(),
            if what.is_empty() {
                String::new()
            } else {
                format!(" [{what}]")
            },
            if evs.is_empty() {
                String::new()
            } else {
                format!(" {}", evs.join(", "))
            }
        );
    }
    *pass += 1;
}

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [install, discs, out] = &args[..] else {
        eprintln!("usage: tibet_tour <install dir> <discs dir> <out dir>");
        std::process::exit(2);
    };
    std::fs::create_dir_all(out).unwrap();
    picture(install, discs, out, 7, "sha", 30);
    picture(install, discs, out, 10, "sha", 30);
    picture(install, discs, out, 6, "2wa", 4);

    // The walk.
    let mut e = open(install, discs);
    let mut screen = vec![0u16; W * H];
    let mut log = String::new();
    let mut pass = 0u64;
    let mut shots = 0;
    let _ = writeln!(
        log,
        "new game: tibet.wam view 2 at (-90, 0); 59.94 passes a second"
    );
    for _ in 0..30 {
        step(&mut e, &mut screen, &mut log, &mut pass, "");
    }
    save(out, "tibet-seq-1-start.png", &screen);
    // The mouse a little to the right and down for 4 frames.
    for i in 0..30 {
        if i < 4 {
            e.mouse_motion(4, 8);
        }
        step(
            &mut e,
            &mut screen,
            &mut log,
            &mut pass,
            if i < 4 { "mouse 4,8" } else { "" },
        );
    }
    assert_eq!(e.centre_index(), 0, "zone 0 (to view 3) under the centre");
    e.click(pass as f64 * PASS);
    let mut turned = false;
    for _ in 0..600 {
        if e.place().1 != 2 {
            break;
        }
        step(&mut e, &mut screen, &mut log, &mut pass, "");
        if e.turning() && !turned && pass.is_multiple_of(8) {
            save(out, "tibet-seq-2-turn.png", &screen);
            turned = true;
        }
    }
    // The video: one frame per tick.
    for _ in 0..2000 {
        if e.loop_type != LoopType::Video {
            break;
        }
        step(&mut e, &mut screen, &mut log, &mut pass, "");
        if shots == 0 && pass.is_multiple_of(160) {
            save(out, "tibet-seq-3-video.png", &screen);
            shots += 1;
        }
    }
    for _ in 0..30 {
        step(&mut e, &mut screen, &mut log, &mut pass, "");
    }
    save(out, "tibet-seq-4-view3.png", &screen);
    // Zone 1 of view 3 (to view 4) is under the centre: a click, a short
    // turn to its aim, no video on the discs, a cut.
    assert_eq!(e.centre_index(), 1, "zone 1 (to view 4) under the centre");
    e.click(pass as f64 * PASS);
    for _ in 0..600 {
        if e.place().1 != 3 {
            break;
        }
        step(&mut e, &mut screen, &mut log, &mut pass, "");
    }
    for _ in 0..30 {
        step(&mut e, &mut screen, &mut log, &mut pass, "");
    }
    save(out, "tibet-seq-5-view4.png", &screen);
    std::fs::write(out.join("tibet-sequence.txt"), &log).unwrap();
    print!("{log}");
}

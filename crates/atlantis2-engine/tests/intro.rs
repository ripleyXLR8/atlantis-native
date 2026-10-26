//! The start-up intro videos (FUN_0049d9c0): `intro%02d.hnm` from 00
//! over the just-built player menu, each with its `intro%02d.apc` on voice
//! 2, chained until a file the current disc does not have, then the player
//! menu. A click ends one video, a double click all of them (its flag
//! stays set), Escape none.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::sound::VOICE_EFFECTS;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

fn game_with(discs: PathBuf) -> Option<A2Files> {
    let dir = std::env::var_os("ATLANTIS2_DATA")?;
    let (files, _) = A2Files::open_dir(&PathBuf::from(dir), &[discs]).expect("game folder");
    Some(files)
}

fn discs() -> Option<PathBuf> {
    let (Some(_), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    Some(PathBuf::from(discs))
}

/// The engine at 59.94 passes a second ([`PASS`]), every pass drawn.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
}

impl Run {
    /// The start-up as the original's: the scripts, the player menu, the
    /// intro.
    fn start(files: A2Files) -> Run {
        Run::boot(files, None)
    }

    /// The same with the saves and `atlant2.cfg` kept in `data`, read
    /// before the menu as the application does.
    fn start_in(files: A2Files, data: &std::path::Path) -> Run {
        Run::boot(files, Some(data.to_path_buf()))
    }

    fn boot(files: A2Files, data: Option<PathBuf>) -> Run {
        let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        e.trace = true;
        e.set_data_dir(data);
        e.open_player_menu();
        e.start_intro();
        Run {
            e,
            pass: 0,
            screen: vec![0; 640 * 480],
        }
    }

    fn now(&self) -> f64 {
        self.pass as f64 * PASS
    }

    fn step(&mut self) {
        self.e.set_clock(self.now());
        self.e.pass(Some(&mut self.screen));
        self.pass += 1;
    }

    /// Steps until the intro is over (at most `limit` passes).
    fn until_menu(&mut self, limit: u64) {
        for _ in 0..limit {
            if self.e.loop_type == LoopType::Menu {
                return;
            }
            self.step();
        }
        panic!("the intro still plays after {limit} passes");
    }

    /// The videos started (name, frame count when it opened), the frames
    /// shown by each, and the sounds started.
    fn videos(&self) -> Vec<(String, Option<u32>, u32)> {
        let mut out: Vec<(String, Option<u32>, u32)> = Vec::new();
        for ev in &self.e.events {
            match ev {
                Event::VideoStart { name, frames } => out.push((name.clone(), *frames, 0)),
                Event::VideoFrame(_) => out.last_mut().unwrap().2 += 1,
                _ => {}
            }
        }
        out
    }

    fn sounds(&self) -> Vec<String> {
        self.e
            .events
            .iter()
            .filter_map(|ev| match ev {
                Event::Sound(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }
}

/// A folder holding only disc `n`'s image, as a drive with that disc.
#[cfg(unix)]
fn one_disc(discs: &std::path::Path, n: u32) -> Option<PathBuf> {
    let image = std::fs::read_dir(discs)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|f| f.to_str())
                .is_some_and(|f| f.eq_ignore_ascii_case(&format!("CD{n}.iso")))
        })?;
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("a2-intro-disc{n}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::os::unix::fs::symlink(&image, d.join(image.file_name().unwrap())).unwrap();
    Some(d)
}

#[test]
fn disc_1_plays_both_videos_then_the_player_menu() {
    let Some(discs) = discs() else { return };
    let mut r = Run::start(game_with(discs).unwrap());
    assert_eq!(r.e.loop_type, LoopType::Video);
    assert!(r.e.intro_playing());
    // The menu page is built under the intro, not drawn.
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Player));
    // Escape does nothing in the video loop.
    r.e.escape();
    r.step();
    assert_eq!(r.e.loop_type, LoopType::Video);
    r.until_menu(60 * 60 * 10);
    assert!(!r.e.intro_playing());
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Player));
    let v = r.videos();
    assert_eq!(v.len(), 3, "{v:?}");
    assert_eq!(v[0].0, "intro00.hnm");
    assert_eq!(v[1].0, "intro01.hnm");
    // Every frame shown, to the end of each file.
    for (name, frames, shown) in &v[..2] {
        assert_eq!(Some(*shown), *frames, "{name}");
    }
    // intro02 is on no disc: the chain ends.
    assert_eq!((v[2].0.as_str(), v[2].1), ("intro02.hnm", None));
    assert_eq!(r.sounds(), ["intro00.apc", "intro01.apc"]);
    assert!(!r.e.voices().is_playing(VOICE_EFFECTS));
    // The menu's frame draws the player page once the intro is over.
    r.step();
    assert!(r.e.events.iter().filter(|e| **e == Event::VideoEnd).count() == 2);
    let distinct = r
        .screen
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert!(distinct > 16, "the player page drawn");
}

#[test]
fn a_click_skips_one_video_a_double_click_all() {
    let Some(discs) = discs() else { return };
    // One click: intro00 ends at the next tick (one more frame), intro01
    // plays.
    let mut r = Run::start(game_with(discs.clone()).unwrap());
    for _ in 0..30 {
        r.step();
    }
    let shown = r.videos()[0].2;
    r.e.click(r.now());
    for _ in 0..4 {
        r.step();
    }
    let v = r.videos();
    assert_eq!(v.len(), 2);
    assert_eq!((v[0].0.as_str(), v[0].2), ("intro00.hnm", shown + 1));
    assert_eq!(v[1].0, "intro01.hnm");
    assert_eq!(r.e.loop_type, LoopType::Video);
    assert_eq!(r.sounds(), ["intro00.apc", "intro01.apc"]);

    // A double click: both videos end, intro01 after its first frame and
    // one more.
    let mut r = Run::start(game_with(discs).unwrap());
    for _ in 0..30 {
        r.step();
    }
    r.e.click(r.now());
    r.e.click(r.now() + 0.1);
    r.until_menu(16);
    let v = r.videos();
    assert_eq!(v.len(), 3);
    assert_eq!(v[1].0, "intro01.hnm");
    assert_eq!(v[1].2, 2);
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Player));
    assert!(!r.e.voices().is_playing(VOICE_EFFECTS));
}

/// With one disc in the drive: disc 1 plays two videos, discs 2-4 one,
/// the same file as disc 1's second (its `.hnm` and `.apc` byte for byte,
/// read from that disc), shown to its last frame.
#[cfg(unix)]
#[test]
fn the_other_discs_play_one_video() {
    let Some(discs) = discs() else { return };
    let mut played = Vec::new();
    for n in 1..=4u8 {
        let d = one_disc(&discs, u32::from(n))
            .unwrap_or_else(|| panic!("CD{n}.iso not in ATLANTIS2_DISCS"));
        let files = game_with(d).unwrap();
        // What the drive holds: disc n's own files.
        let mut bytes = Vec::new();
        for i in 0.. {
            let name = format!("intro{i:02}");
            let Some((hnm, from)) = files.read_with_disc(Folder::Hnm, &format!("{name}.hnm"))
            else {
                break;
            };
            assert_eq!(from, n, "{name}.hnm");
            let (apc, from) = files
                .read_with_disc(Folder::Apc, &format!("{name}.apc"))
                .unwrap();
            assert_eq!(from, n, "{name}.apc");
            bytes.push((hnm, apc));
        }
        let mut r = Run::start(files);
        r.until_menu(60 * 60 * 10);
        let v = r.videos();
        // Every file the disc has, each to its last frame, then the
        // missing one ends the chain.
        assert_eq!(v.len(), bytes.len() + 1, "disc {n}: {v:?}");
        for (i, (name, frames, shown)) in v[..bytes.len()].iter().enumerate() {
            assert_eq!(*name, format!("intro{i:02}.hnm"));
            assert_eq!(Some(*shown), *frames, "disc {n} {name}");
        }
        assert_eq!(v[bytes.len()].1, None);
        played.push(bytes);
    }
    assert_eq!(played[0].len(), 2);
    for (n, other) in played.iter().enumerate().skip(1) {
        assert_eq!(other.len(), 1, "disc {}", n + 1);
        assert!(
            other[0] == played[0][1],
            "disc {}: not disc 1's second",
            n + 1
        );
    }
}

/// Nothing a start leaves behind turns the intro off (FUN_0049d680 plays
/// it without a condition): a first start into a data folder, the intro
/// played to its end, a new game, a save, `atlant2.cfg` written at exit;
/// the next start on that folder (its save seen) plays the whole intro
/// again from `intro00`.
#[test]
fn every_start_plays_it_again() {
    let Some(discs) = discs() else { return };
    let data = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("a2-intro-again");
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).unwrap();
    let names = |r: &Run| -> Vec<String> { r.videos().into_iter().map(|v| v.0).collect() };
    let whole = ["intro00.hnm", "intro01.hnm", "intro02.hnm"];

    let mut r = Run::start_in(game_with(discs.clone()).unwrap(), &data);
    r.until_menu(60 * 60 * 10);
    assert_eq!(names(&r), whole);
    r.e.start_new_game().unwrap();
    for _ in 0..30 {
        r.step();
    }
    assert!(r.e.save_slot(0));
    r.e.write_cfg();
    drop(r);
    assert!(data.join("atlant2.cfg").is_file());

    let mut r = Run::start_in(game_with(discs).unwrap(), &data);
    assert_eq!(r.e.saves().len(), 1);
    assert!(r.e.intro_playing());
    assert_eq!(r.e.video_playing().map(|v| v.0), Some("intro00.hnm"));
    r.until_menu(60 * 60 * 10);
    assert_eq!(names(&r), whole);
    let _ = std::fs::remove_dir_all(&data);
}

/// One case of `tests/golden/intro.txt` (the original's FUN_0049d9c0 and
/// video loop under emulation, a tick every fourth pass): the videos
/// started (pass, name, the last frame the one before showed) and the pass
/// the menu is back.
type Starts = (Vec<(i64, String, Option<u32>)>, i64);

fn golden_case(golden: &str, head: &str) -> Starts {
    let mut rows = golden
        .lines()
        .skip_while(|l| *l != head)
        .skip(1)
        .take_while(|l| !l.starts_with("=="));
    let mut starts = Vec::new();
    let mut end = -1;
    for l in rows.by_ref() {
        let pass = if l.starts_with("start ") {
            -1
        } else if let Some(p) = l.strip_prefix('p') {
            p.split(' ').next().unwrap().parse().unwrap()
        } else {
            continue;
        };
        if l.contains("loop 5 ") {
            end = pass;
        }
        let Some(i) = l.find("AtlHnm_start(") else {
            continue;
        };
        let name = l[i + 13..].split(')').next().unwrap().to_string();
        let last = l[..i]
            .rsplit("hnm_frame ")
            .next()
            .filter(|_| l[..i].contains("hnm_frame "))
            .and_then(|s| s.split(';').next())
            .map(|s| s.trim().parse().unwrap());
        starts.push((pass, name, last));
    }
    (starts, end)
}

/// The same run by the engine: `clicks` maps a pass to a double click.
fn native_case(files: A2Files, clicks: &[(i64, bool)]) -> Starts {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.trace = true;
    e.open_player_menu();
    e.events.clear();
    e.start_intro();
    let mut starts = Vec::new();
    let mut last = None;
    let mut record = |e: &mut Engine, pass: i64, last: &mut Option<u32>| {
        for ev in std::mem::take(&mut e.events) {
            match ev {
                Event::VideoStart { name, .. } => {
                    starts.push((pass, name, last.take()));
                }
                Event::VideoFrame(k) => *last = Some(k),
                _ => {}
            }
        }
    };
    record(&mut e, -1, &mut last);
    let mut t = 100.0;
    for n in 0..4000i64 {
        if n % 4 == 0 {
            e.tick();
        }
        if let Some(&(_, double)) = clicks.iter().find(|c| c.0 == n) {
            t += 10.0;
            e.click(t);
            if double {
                e.click(t + 0.1);
            }
        }
        e.pass(None);
        record(&mut e, n, &mut last);
        if e.loop_type == LoopType::Menu {
            return (starts, n);
        }
    }
    panic!("the intro still plays");
}

/// L49: the intro's chain and its skip rules against the original's
/// (`intro.txt`): discs 1 and 2, no input, a click, a double click, two
/// clicks; the passes each video starts and the menu comes back, and the
/// last frame each video showed.
#[cfg(unix)]
#[test]
fn the_intro_and_its_skips_match_the_original() {
    let Some(discs) = discs() else { return };
    let golden = include_str!("golden/intro.txt");
    let mut n = 0;
    for disc in [1, 2] {
        for (label, clicks) in [
            ("no input", vec![]),
            ("click at pass 20", vec![(20, false)]),
            ("double click at pass 20", vec![(20, true)]),
            (
                "click at pass 20 and at pass 200",
                vec![(20, false), (200, false)],
            ),
        ] {
            let head = format!("== disc {disc} {label}");
            if !golden.lines().any(|l| l == head) {
                continue;
            }
            let Some(one) = one_disc(&discs, disc) else {
                eprintln!("CD{disc}.iso not found: case skipped");
                continue;
            };
            let want = golden_case(golden, &head);
            let got = native_case(game_with(one).unwrap(), &clicks);
            assert_eq!(got, want, "{head}");
            n += 1;
        }
    }
    assert_eq!(n, 7);
}

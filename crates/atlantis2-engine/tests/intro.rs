//! The start-up intro videos (FUN_0049d9c0): `intro%02d.hnm` from 00
//! over the just-built player menu, each with its `intro%02d.apc` on voice
//! 2, chained until a file the current disc does not have, then the player
//! menu. A click ends one video, a double click all of them (its flag
//! stays set), Escape none.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::A2Files;
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

/// The engine at 60 passes a second, every pass drawn.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
}

impl Run {
    /// The start-up as the original's: the scripts, the player menu, the
    /// intro.
    fn start(files: A2Files) -> Run {
        let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        e.trace = true;
        e.open_player_menu();
        e.start_intro();
        Run {
            e,
            pass: 0,
            screen: vec![0; 640 * 480],
        }
    }

    fn now(&self) -> f64 {
        self.pass as f64 / 60.0
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

#[cfg(unix)]
#[test]
fn the_other_discs_play_one_video() {
    let Some(discs) = discs() else { return };
    let mut sizes = Vec::new();
    for n in 1..=4 {
        let Some(d) = one_disc(&discs, n) else {
            eprintln!("CD{n}.iso not found: skipped");
            continue;
        };
        let mut r = Run::start(game_with(d).unwrap());
        // The frame counts only: a click ends each video.
        let mut names = Vec::new();
        while r.e.loop_type == LoopType::Video {
            names.push(r.e.video_playing().unwrap().0.to_string());
            r.e.click(r.now());
            r.step();
            r.step();
            r.step();
            r.step();
            r.pass += 60;
        }
        let v = r.videos();
        let started: Vec<_> = v.iter().filter(|x| x.1.is_some()).collect();
        sizes.push((n, started.iter().map(|x| x.1.unwrap()).collect::<Vec<_>>()));
        assert_eq!(names.len(), started.len());
        assert_eq!(r.e.loop_type, LoopType::Menu);
    }
    eprintln!("{sizes:?}");
    // Disc 1: two videos; discs 2-4: one, the same as disc 1's second.
    if let Some((_, one)) = sizes.iter().find(|(n, _)| *n == 1) {
        assert_eq!(one.len(), 2);
        for (n, other) in sizes.iter().filter(|(n, _)| *n != 1) {
            assert_eq!(other.as_slice(), &one[1..], "disc {n}");
        }
    }
}

#[test]
fn every_start_plays_it_again() {
    let Some(discs) = discs() else { return };
    for _ in 0..2 {
        let r = Run::start(game_with(discs.clone()).unwrap());
        assert_eq!(r.e.video_playing().map(|v| v.0), Some("intro00.hnm"));
    }
}

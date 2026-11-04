//! The view a dialog leaves behind: why there is no bug fix for it.
//!
//! The original turns the panorama by π for the reverse shots: the hero's
//! first line turns it, and so does every switch between the hero and
//! someone else (the dialog tick FUN_004801d0; `voixoff0` lines have a
//! shorter rule of their own, `voixoff` lines none). Its end
//! (FUN_004808a0) turns back only when the line still current is the
//! hero's: when the player presses Escape, or a new `dial` stops the
//! dialog. At the normal end no line is current, and the angles the start
//! saves (@ 0x006cf2c0) are never read again, so a dialog whose hero
//! speaks last leaves the view facing away from the one he spoke to,
//! until the next move. Reported by Klaas Van den Broeck (@Klaasvdb),
//! github.com/Klaasvdb/atlantis2-notes (original-bugs.md #12).
//!
//! In the game's data no dialog the game can play ends that way, so the
//! port offers no switch for it (it would change nothing). Every dialog
//! of every world played to its end with the port's turn rule (the
//! original's): six end turned, Ireland's 68 (one line of the hero) and
//! China's 348 to 352 (the same). No chapter starts dialog 68, and 348 to
//! 352 are the labyrinth souls' answers, started only by a click on a
//! soul, which cannot happen (`tests/labyrinth.rs`,
//! `a_click_on_a_soul_as_the_original`). An Escape (or another `dial`)
//! at any line of any dialog leaves the view as it found it. Every
//! edition's dialog files and chapters are the same.
//!
//! The game-data tests need `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them they are skipped.

use std::collections::BTreeSet;
use std::f32::consts::TAU;
use std::path::PathBuf;

use atlantis2_data::dialog::World;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::dialog::DialogScript;
use atlantis2_engine::{BugFixes, Engine, Enhancements, Fpu, LoopType, Variant};

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA or ATLANTIS2_DISCS not set");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// Whether the view's yaw is half a turn from `y0`.
fn turned(e: &Engine, y0: f32) -> bool {
    let d = (e.camera.angles().0 - y0).rem_euclid(TAU);
    d.min(TAU - d) > 1.0
}

/// Every dialog of every world played to its end from Tibet's view 7 (a
/// click every 20 passes), with the original's rules: those that leave
/// the view turned, and those where an Escape at some line would.
#[test]
fn only_dialogs_the_game_never_plays_end_turned() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.enhancements = Enhancements {
        fixes: BugFixes::NONE,
        ..Enhancements::original()
    };
    e.start_new_game().unwrap();
    e.end_video();
    for _ in 0..30 {
        e.tick();
        e.pass(None);
    }
    assert_eq!(e.loop_type, LoopType::Warp);
    let mut ends_turned = Vec::new();
    let mut escape_turned = BTreeSet::new();
    let mut played = 0;
    for monde in [1, 2, 3, 4, 5] {
        e.run_command(format!("(monde={monde})").as_bytes());
        let world = World::of_monde(monde).unwrap();
        let script = DialogScript::load(e.files(), world, Fpu::Double).unwrap();
        let numbers: BTreeSet<u32> = script
            .com
            .iter()
            .filter_map(|l| l.id.split_once('_')?.0.trim().parse().ok())
            .collect();
        for n in numbers {
            e.enter_view(7, 0.5, 0.0).unwrap();
            let y0 = e.camera.angles().0;
            e.start_dialog(n)
                .unwrap_or_else(|err| panic!("{} {n}: {err}", world.code));
            let mut ended = false;
            for pass in 0..20_000 {
                // An Escape now: the end turns back for a hero's line.
                if let Some(l) = e.dialog().and_then(|d| d.current_line()) {
                    if turned(&e, y0) != (l.speaker == world.hero()) {
                        escape_turned.insert((world.code, n));
                    }
                }
                if pass % 20 == 19 {
                    e.click(f64::from(pass) * 10.0 + f64::from(n) * 1e6);
                }
                e.tick();
                e.pass(None);
                if e.dialog().is_none() {
                    ended = true;
                    break;
                }
            }
            assert!(ended, "{} {n} does not end", world.code);
            played += 1;
            if turned(&e, y0) {
                ends_turned.push((world.code, n));
            }
        }
    }
    assert_eq!(played, 26 + 94 + 104 + 116 + 18);
    assert_eq!(
        ends_turned,
        vec![
            ("IR", 68),
            ("CH", 348),
            ("CH", 349),
            ("CH", 350),
            ("CH", 351),
            ("CH", 352)
        ]
    );
    assert!(escape_turned.is_empty(), "{escape_turned:?}");
}

/// The chapters: none starts dialog 68 (a number, or `MemoNumDial`, which
/// only takes the values 327 to 342), and 348 to 352 are started only in
/// chapter 60's sections for a click on one of the souls `AME1` to `AME4`.
#[test]
fn the_chapters_never_start_the_dialogs_that_end_turned() {
    let Some(files) = game() else { return };
    let mut started: BTreeSet<u32> = BTreeSet::new();
    let mut memo: BTreeSet<u32> = BTreeSet::new();
    let mut late: Vec<(String, String)> = Vec::new();
    for c in 0..100 {
        let name = format!("chapi{c:03}.con");
        let Some(text) = files.read(Folder::Scenar, &name) else {
            continue;
        };
        let lines: Vec<String> = text
            .split(|&b| b == b'\n')
            .map(|l| {
                String::from_utf8_lossy(l.strip_suffix(b"\r").unwrap_or(l)).to_ascii_lowercase()
            })
            .collect();
        let mut section = String::new();
        for l in &lines {
            // The character a section answers a click on.
            if let Some(who) = l.strip_prefix("/con(clicperso=") {
                section = who.chars().take_while(|c| *c != ')').collect();
            }
            let value = |key: &str| {
                let i = l.find(key)? + key.len();
                let v: String = l[i..].chars().take_while(|c| *c != ')').collect();
                Some(v)
            };
            if let Some(v) = value("/set(dial=") {
                match v.parse::<u32>() {
                    Ok(n) => {
                        started.insert(n);
                        if (348..=352).contains(&n) {
                            late.push((name.clone(), section.clone()));
                        }
                    }
                    Err(_) => assert_eq!(v, "memonumdial", "{name}"),
                }
            }
            if let Some(v) = value("/set(memonumdial=") {
                memo.insert(v.parse().unwrap());
            }
        }
    }
    assert!(!started.contains(&68));
    assert_eq!(memo, BTreeSet::from([327, 330, 333, 336, 339, 342]));
    let sections: BTreeSet<&str> = late.iter().map(|(_, s)| s.as_str()).collect();
    assert!(late.iter().all(|(f, _)| f == "chapi060.con"));
    assert_eq!(sections, BTreeSet::from(["ame1", "ame2", "ame3", "ame4"]));
}

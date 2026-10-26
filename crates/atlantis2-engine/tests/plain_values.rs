//! Ireland's two puzzle views, Brigid's cross (`bigile1.wam` view 2)
//! and the ogham stones (view 6): their zones are of type 4, whose plain
//! values (1..25, 99) the original's zone click (FUN_0049ca80) puts in
//! `CLICZONE` as it is, while the chapters compare `CLICZONE` with the
//! indices of the named click zones and name no view. So, with the
//! original's rules (`--original`), the zone of value 4 answers for the
//! weeping head (index 4) in chapter 12, the one of value 9 for the
//! water (index 9) in chapter 14, and so on; with the minor bug fixes on
//! (the default), a click there runs its puzzle hook and nothing meant
//! for a named zone.
//!
//! The game-data tests need `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them they are skipped.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use atlantis2_data::A2Files;
use atlantis2_engine::engine::irish_plain_value_zone;
use atlantis2_engine::{Engine, Enhancements, Event, Fpu, LoopType, Variant};

fn game() -> Option<(A2Files, PathBuf)> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
        return None;
    };
    let dir = PathBuf::from(dir);
    let (files, _) = A2Files::open_dir(&dir, &[PathBuf::from(discs)]).expect("game folder");
    Some((files, dir))
}

/// Ireland's chapters (those its scripts and the flow load).
const CHAPTERS: [i32; 18] = [
    2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17, 19, 20, 24,
];

/// Every object an Irish condition names next to a `cliczone` term, and
/// the ogham stones' knife.
const HANDS: [&str; 11] = [
    "CRAYON",
    "CRANE_CROIX",
    "CRANE_RUCHE",
    "CRANE_PUIT",
    "CRANE_CHAPELLE",
    "CRANE_W18",
    "BATON_OGHAM",
    "FOURCHE",
    "CRUCHE",
    "CHEVAL",
    "COUTEAU_AILILL",
];

fn settle(e: &mut Engine) {
    for _ in 0..400 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Warp && !e.turning() {
            break;
        }
    }
}

/// A new game in Ireland at view `view` of `bigile1.wam`, chapter
/// `chapter` loaded, with the fixes on or off; the `setup` commands run.
fn at(dir: &Path, fixes: bool, chapter: i32, view: u32, setup: &[&str]) -> Engine {
    let discs = PathBuf::from(std::env::var_os("ATLANTIS2_DISCS").unwrap());
    let (files, _) = A2Files::open_dir(dir, &[discs]).unwrap();
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.enhancements = if fixes {
        Enhancements::default()
    } else {
        Enhancements::original()
    };
    e.start_new_game().unwrap();
    e.end_video();
    for c in ["(monde=1)", "(changewam=bigile1.wam)", "(vue=5)"] {
        e.run_command(c.as_bytes());
    }
    e.run_command(format!("(chapter={chapter})").as_bytes());
    settle(&mut e);
    e.run_command(format!("(vue={view})").as_bytes());
    settle(&mut e);
    assert_eq!(e.place(), ("bigile1.wam", view));
    assert_eq!(e.script_state().unwrap().chapter, chapter);
    for c in setup {
        e.run_command(c.as_bytes());
    }
    e.trace = true;
    e
}

/// The index of the zone of value `value` (type 4) in the current view.
fn zone_of_value(e: &Engine, value: i32) -> usize {
    let view = e.place().1;
    e.wam()
        .places
        .iter()
        .flat_map(|p| p.views.iter())
        .find(|v| v.id == view)
        .unwrap()
        .zones
        .iter()
        .position(|z| z.kind == 4 && z.value == value)
        .unwrap_or_else(|| panic!("no zone of value {value} in view {view}"))
}

/// The type-4 zones of the current view: index and value.
fn plain_zones(e: &Engine) -> Vec<(usize, i32)> {
    let view = e.place().1;
    e.wam()
        .places
        .iter()
        .flat_map(|p| p.views.iter())
        .find(|v| v.id == view)
        .unwrap()
        .zones
        .iter()
        .enumerate()
        .filter(|(_, z)| z.kind == 4)
        .map(|(i, z)| (i, z.value))
        .collect()
}

/// A click at the centre aimed at zone `zi`, `hand` in hand: the events.
fn click(e: &mut Engine, zi: usize, hand: Option<&str>, t: &mut f64) -> Vec<Event> {
    let aim = e.zone_aim(e.place().1, zi).unwrap();
    e.camera.enter(aim.0, aim.1);
    e.pass(None);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zi));
    if let Some(h) = hand {
        e.run_command(format!("(objetenmain={h})").as_bytes());
    }
    e.events.clear();
    *t += 10.0;
    e.click(*t);
    e.pass(None);
    settle(e);
    std::mem::take(&mut e.events)
}

/// The chapters' lines by number (1-based), CR LF cut.
struct Chapters(Vec<(i32, Vec<String>)>);

impl Chapters {
    fn read(dir: &Path) -> Chapters {
        let mut v = Vec::new();
        for c in CHAPTERS.iter().copied().chain([75]) {
            let t = std::fs::read(dir.join(format!("scenar/chapi{c:03}.con"))).unwrap();
            let t = String::from_utf8_lossy(&t).into_owned();
            v.push((c, t.lines().map(|l| l.trim_end().to_owned()).collect()));
        }
        Chapters(v)
    }

    /// Whether line `line` of chapter `chapter` is in the body of an
    /// entry whose conditions (the section's head and the sub-conditions
    /// above the line) test `cliczone`.
    fn under_cliczone(&self, chapter: i32, line: usize) -> bool {
        let Some((_, lines)) = self.0.iter().find(|(c, _)| *c == chapter) else {
            return false;
        };
        for l in lines[..line.saturating_sub(1)].iter().rev() {
            let low = l.to_ascii_lowercase();
            if low.starts_with("/con") && low.contains("cliczone") {
                return true;
            }
            let header = l.len() > 1 && l.as_bytes()[0] == b'/' && l.as_bytes()[1].is_ascii_digit();
            if header || low.starts_with("/fininit") {
                return false;
            }
        }
        false
    }

    /// The lines run by the events that belong to a `cliczone` entry.
    fn cliczone_sets(&self, evs: &[Event]) -> Vec<(i32, usize)> {
        evs.iter()
            .filter_map(|e| match e {
                Event::Set { chapter, line } if self.under_cliczone(*chapter, *line) => {
                    Some((*chapter, *line))
                }
                _ => None,
            })
            .collect()
    }
}

fn hooks(evs: &[Event]) -> Vec<u32> {
    evs.iter()
        .filter_map(|e| match e {
            Event::PuzzleRan { function } => Some(*function),
            _ => None,
        })
        .collect()
}

fn films(evs: &[Event]) -> Vec<String> {
    evs.iter()
        .filter_map(|e| match e {
            Event::VideoStart { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn hand_id(e: &Engine) -> i32 {
    e.script_state().unwrap().var("objetenmain").unwrap()
}

fn obj(e: &Engine, name: &str) -> i32 {
    let s = e.script_state().unwrap();
    s.names
        .id(atlantis2_data::defnames::OBJETS, name.as_bytes())
        .unwrap() as i32
}

#[test]
fn only_the_two_puzzle_views_of_ireland_and_their_plain_values() {
    assert!(irish_plain_value_zone(b"ir0002", 2, 4));
    assert!(irish_plain_value_zone(b"IR0006", 6, 4));
    assert!(!irish_plain_value_zone(b"ir0006", 6, 3));
    assert!(!irish_plain_value_zone(b"ir0006", 6, 1));
    assert!(!irish_plain_value_zone(b"ir0006", 6, 4 + 88888));
    assert!(!irish_plain_value_zone(b"ir0042", 42, 4));
    assert!(!irish_plain_value_zone(b"ch0002", 2, 4));
    assert!(!irish_plain_value_zone(b"ti0010", 6, 4));
}

/// One colliding click: chapter, value, hand, set-up commands, and what
/// the original does (checked with the fixes off).
type Case = (
    &'static str,
    i32,
    i32,
    Option<&'static str>,
    &'static [&'static str],
    fn(&Engine, &[Event]),
);

const CASES: [Case; 12] = [
    (
        "ch12 value 4 (TETE_PLEURANTE) empty hand: back to Tibet",
        12,
        4,
        None,
        &[],
        |e, _| {
            assert_eq!(e.place(), ("tibet.wam", 7));
            assert_eq!(e.script_state().unwrap().chapter, 75);
        },
    ),
    (
        "ch12 value 4 (TETE_PLEURANTE) jug: filled",
        12,
        4,
        Some("CRUCHE"),
        &[],
        |e, _| assert_eq!(hand_id(e), obj(e, "CRUCHE_EAU")),
    ),
    (
        "ch14 value 9 (EAU) horse: the water's film",
        14,
        9,
        Some("CHEVAL"),
        &[],
        |_, evs| assert_eq!(films(evs), ["cine006.hnm"]),
    ),
    (
        "ch8 value 5 (MENHIR) ogham staff: the menhir's film",
        8,
        5,
        Some("BATON_OGHAM"),
        &[],
        |_, evs| assert_eq!(films(evs), ["cine005.hnm"]),
    ),
    (
        "ch9 value 8 (HAUT_CROIX) fork: the high cross's skull knocked off",
        9,
        8,
        Some("FOURCHE"),
        &[],
        |_, evs| {
            assert!(evs.iter().any(|x| matches!(
                x,
                Event::Set {
                    chapter: 9,
                    line: 8
                }
            )))
        },
    ),
    (
        "ch7 value 1 (CRANE_ZONE) cross's skull laid",
        7,
        1,
        Some("CRANE_CROIX"),
        &[],
        |e, _| assert_eq!(e.script_state().unwrap().var("fincrane"), Some(1)),
    ),
    (
        "ch7 value 1 (CRANE_ZONE) beehive's skull laid",
        7,
        1,
        Some("CRANE_RUCHE"),
        &[],
        |e, _| assert_eq!(e.script_state().unwrap().var("fincrane"), Some(1)),
    ),
    (
        "ch7 value 1 (CRANE_ZONE) well's skull laid",
        7,
        1,
        Some("CRANE_PUIT"),
        &[],
        |e, _| assert_eq!(e.script_state().unwrap().var("fincrane"), Some(1)),
    ),
    (
        "ch7 value 1 (CRANE_ZONE) chapel's skull laid",
        7,
        1,
        Some("CRANE_CHAPELLE"),
        &[],
        |e, _| assert_eq!(e.script_state().unwrap().var("fincrane"), Some(1)),
    ),
    (
        "ch7 value 1 (CRANE_ZONE) view 18's skull laid",
        7,
        1,
        Some("CRANE_W18"),
        &[],
        |e, _| assert_eq!(e.script_state().unwrap().var("fincrane"), Some(1)),
    ),
    (
        "ch5 value 6 (NUADA) pencil: the sword drawn",
        5,
        6,
        Some("CRAYON"),
        &["(flag30=1)", "(flag7=1)", "(flag9=0)"],
        |e, _| assert_eq!(e.script_state().unwrap().flags[9], 1),
    ),
    (
        "ch20 value 10 (PUIT_ZONE) dowsing: flag 70",
        20,
        10,
        None,
        &["(sourcier=1)"],
        |e, _| assert_eq!(e.script_state().unwrap().flags[70], 1),
    ),
];

/// `FILLE_LIVRE` (index 7), chapter 5, kept apart: its set-up names
/// flags 16 and 17.
const FILLE_LIVRE: Case = (
    "ch5 value 7 (FILLE_LIVRE) pencil: the salmon drawn",
    5,
    7,
    Some("CRAYON"),
    &["(flag30=1)", "(flag16=1)", "(flag17=0)"],
    |e, _| assert_eq!(e.script_state().unwrap().flags[17], 1),
);

#[test]
fn every_collision_answers_as_the_original_off_and_only_the_puzzle_on() {
    let Some((_, dir)) = game() else { return };
    let chapters = Chapters::read(&dir);
    let mut t = 100.0;
    for (label, chapter, value, hand, setup, original) in CASES.iter().chain([&FILLE_LIVRE]) {
        for view in [2, 6] {
            let what = format!("{label}, view {view}");
            // The original: a section meant for the named zone runs.
            let mut e = at(&dir, false, *chapter, view, setup);
            let zi = zone_of_value(&e, *value);
            let off = click(&mut e, zi, *hand, &mut t);
            assert!(!chapters.cliczone_sets(&off).is_empty(), "{what}: off");
            original(&e, &off);
            // The fix: none runs, the place and chapter stay, the hand is
            // kept; the puzzle hooks run as with the fix off.
            let mut e = at(&dir, true, *chapter, view, setup);
            let held = hand_id(&e);
            let on = click(&mut e, zi, *hand, &mut t);
            assert_eq!(chapters.cliczone_sets(&on), [], "{what}: on");
            assert_eq!(e.place(), ("bigile1.wam", view), "{what}: on");
            assert_eq!(e.script_state().unwrap().chapter, *chapter, "{what}");
            assert_eq!(films(&on), Vec::<String>::new(), "{what}: on");
            let want = hand.map_or(held, |h| obj(&e, h));
            assert_eq!(hand_id(&e), want, "{what}: hand kept");
            assert_eq!(hooks(&on), hooks(&off), "{what}: the same hooks");
            if view == 2 {
                assert_eq!(hooks(&on), [0x0049_4190], "{what}: the cross's hook");
            }
        }
    }
}

/// Every type-4 zone of both views clicked in every Irish chapter, with
/// an empty hand and each object an Irish `cliczone` condition names:
/// with the fixes off the entries meant for named zones that answer are
/// exactly the census's (the start state of each chapter); on, none.
#[test]
fn every_zone_every_chapter_every_hand() {
    let Some((_, dir)) = game() else { return };
    let chapters = Chapters::read(&dir);
    let mut t = 100.0;
    for fixes in [false, true] {
        let mut answered = BTreeSet::new();
        for chapter in CHAPTERS {
            for view in [2, 6] {
                let mut e = at(&dir, fixes, chapter, view, &[]);
                let zones = plain_zones(&e);
                for hand in std::iter::once(None).chain(HANDS.iter().copied().map(Some)) {
                    for &(zi, value) in &zones {
                        let evs = click(&mut e, zi, hand, &mut t);
                        if !chapters.cliczone_sets(&evs).is_empty() {
                            answered.insert((chapter, view, value, hand.unwrap_or("-")));
                        }
                        // Anything that ran: a fresh state for the next.
                        let quiet = e.place() == ("bigile1.wam", view)
                            && !evs.iter().any(|x| matches!(x, Event::Set { .. }));
                        if !quiet {
                            e = at(&dir, fixes, chapter, view, &[]);
                        }
                    }
                }
            }
        }
        let want: BTreeSet<_> = if fixes {
            BTreeSet::new()
        } else {
            let mut w = BTreeSet::new();
            for view in [2, 6] {
                for skull in [
                    "CRANE_CROIX",
                    "CRANE_RUCHE",
                    "CRANE_PUIT",
                    "CRANE_CHAPELLE",
                    "CRANE_W18",
                ] {
                    w.insert((7, view, 1, skull));
                }
                w.insert((8, view, 5, "BATON_OGHAM"));
                w.insert((9, view, 8, "FOURCHE"));
                w.insert((12, view, 4, "-"));
                w.insert((12, view, 4, "CRUCHE"));
                w.insert((14, view, 9, "CHEVAL"));
            }
            w
        };
        assert_eq!(answered, want, "fixes {fixes}");
    }
}

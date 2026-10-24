//! Yucatán (world 2) without its counting grids: every Yucatán chapter's
//! `/INIT` run; every view of the three places entered with the puzzles'
//! entry hooks; the click hooks (the grids' noted, the statue's run, the
//! canoe's view taking the click); the moves between the places (WAM
//! links); `AngleWarpXNow` on the arrival in the king's hall; the
//! priestess's yes, the way into Xibalba, refused; the brazier's and the
//! king's ways back to Tibet; the grids' testing aid; every dialog the
//! chapters start; the subjects, objects and musics on disc 2; the place
//! sounds; a save in Yucatán loaded.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::wam::Zone;
use atlantis2_data::A2Files;
use atlantis2_engine::engine::{yucatan_click_hooks, yucatan_entry_hooks, STATUE_BRANCH, WEB_MODE};
use atlantis2_engine::{CursorBank, Engine, EngineError, Event, Fpu, LoopType, Variant};

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// Yucatán's chapters (the files of `scenar\`).
const CHAPTERS: [i32; 10] = [25, 26, 27, 28, 29, 30, 31, 32, 33, 34];

/// Yucatán's places.
const PLACES: [&str; 3] = ["yucatan.wam", "salleroi.wam", "pyratez.wam"];

/// A game in Yucatán as the boat's trip leaves it, without the trip: a new
/// game's film ended, then the section's commands (the world, the place,
/// view 10, chapter 25).
fn in_yucatan(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=2)"[..],
        b"(changewam=yucatan.wam)",
        b"(vue=10)",
        b"(chapter=25)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("yucatan.wam", 10));
    assert_eq!(e.files().current_disc(), 2);
    e.trace = true;
    e
}

/// The game moved to `view` of `place` in chapter `chapter`, as a
/// section's `changewam`, `vue` and `chapter` lines do.
fn goto(e: &mut Engine, place: &str, view: u32, chapter: i32) {
    for c in [
        format!("(changewam={place})"),
        format!("(vue={view})"),
        format!("(chapter={chapter})"),
    ] {
        e.run_command(c.as_bytes());
    }
    settle(e);
    assert_eq!(e.place(), (place, view));
    assert_eq!(e.script_state().unwrap().chapter, chapter);
}

/// Films and dialogs ended until the panorama is back, the passes run.
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
    assert_eq!(e.loop_type, LoopType::Warp);
}

/// Passes run, films and dialogs ended, until `done`.
fn run_until(e: &mut Engine, done: impl Fn(&Engine) -> bool) {
    for _ in 0..3000 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        if done(e) && e.loop_type == LoopType::Warp && !e.turning() {
            return;
        }
    }
    panic!("not reached: {:?}", e.place());
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

fn id(e: &Engine, section: usize, name: &str) -> i32 {
    e.script_state()
        .unwrap()
        .names
        .id(section, name.as_bytes())
        .unwrap() as i32
}

fn in_bag(e: &Engine, name: &str) -> bool {
    let id = id(e, defnames::OBJETS, name);
    e.bag_entries().iter().any(|b| b.id == id)
}

/// The centre on zone `zone` of the current view.
fn aim(e: &mut Engine, zone: usize) {
    let view = e.place().1;
    let (yaw, pitch) = e.zone_aim(view, zone).unwrap();
    e.camera.enter(yaw, pitch);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zone), "view {view}");
}

/// The index of the first zone of the current view that `pick` accepts.
fn zone(e: &Engine, pick: impl Fn(&Zone) -> bool) -> usize {
    let view = e.place().1;
    e.wam()
        .view(view)
        .unwrap()
        .zones
        .iter()
        .position(pick)
        .unwrap_or_else(|| panic!("no such zone in view {view}"))
}

fn named(name: &str) -> impl Fn(&Zone) -> bool + '_ {
    move |z| {
        z.name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case(name))
    }
}

/// Without the scripts (a new game's place and view, no chapter): `name`
/// loaded and `view` entered.
fn bare(files: A2Files, name: &str, view: u32) -> Engine {
    let mut e = Engine::new_game(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.trace = true;
    e.load_place(name);
    e.enter_at(view, 0.0, 0.0).unwrap();
    e
}

/// The puzzle hooks reached in `events`, in order: run or noted.
fn noted(events: &[Event]) -> Vec<u32> {
    events
        .iter()
        .filter_map(|x| match x {
            Event::PuzzleNotPorted { function } | Event::PuzzleRan { function } => Some(*function),
            _ => None,
        })
        .collect()
}

#[test]
fn every_yucatan_chapter_runs_its_init() {
    let Some(files) = game() else { return };
    let mut e = in_yucatan(files);
    // The world's banks and texts (`Interp_SetMonde(2)`): disc 2.
    assert_eq!(var(&e, "MONDE"), 2);
    assert_eq!(var(&e, "YucatanVisited"), 1);
    for chapter in CHAPTERS {
        e.run_command(format!("(chapter={chapter})").as_bytes());
        settle(&mut e);
        let s = e.script_state().unwrap();
        assert!(s.fatal.is_none(), "chapter {chapter}: {:?}", s.fatal);
        assert_eq!(s.chapter, chapter);
        assert!(!s.travel_refused);
        assert!(!e
            .events
            .iter()
            .any(|x| matches!(x, Event::WorldNotPorted(_))));
    }
}

#[test]
fn every_yucatan_view_is_entered_with_its_hooks() {
    let Some(files) = game() else { return };
    let mut e = bare(files, "yucatan.wam", 10);
    let mut views = 0;
    let mut missing_spw = Vec::new();
    let mut missing_warp = Vec::new();
    let mut hooks = Vec::new();
    for place in PLACES {
        e.load_place(place);
        let ids: Vec<(u32, String)> = e
            .wam()
            .places
            .iter()
            .flat_map(|p| p.views.iter())
            .map(|v| (v.id, v.name.clone()))
            .collect();
        for (id, name) in ids {
            e.events.clear();
            match e.enter_at(id, 0.0, 0.0) {
                Ok(()) => {}
                Err(EngineError::NoWarp(w)) => {
                    missing_warp.push(format!("{place} {id} {w}"));
                    continue;
                }
                Err(err) => panic!("{place} view {id}: {err}"),
            }
            views += 1;
            missing_spw.extend(e.events.iter().filter_map(|x| match x {
                Event::SpwLoad { name, found: false } => Some(name.clone()),
                _ => None,
            }));
            let n = noted(&e.events);
            assert_eq!(
                n,
                yucatan_entry_hooks(name.as_bytes(), id, 0),
                "{place} {id}"
            );
            hooks.extend(n.into_iter().map(|f| (place, id, f)));
        }
    }
    // 84 views, every panorama on a disc (the records that stand for a
    // link's other end, `yucatan.wam`'s 50 and 80-82, `salleroi.wam`'s 39,
    // `pyratez.wam`'s 28, 34 and 41, are views of the other places).
    assert_eq!(views, 84);
    assert!(missing_warp.is_empty(), "{missing_warp:?}");
    // One place animation is on no disc: the closed jaw of view 78
    // (`GUEULE_F`), which the original logs too.
    assert_eq!(missing_spw, ["w3A1078078.SPW"]);
    // The hooks: the counting grids' entries, noted.
    assert_eq!(
        hooks,
        [
            ("pyratez.wam", 55, 0x0049_2f60),
            ("pyratez.wam", 64, 0x0049_3070),
            ("pyratez.wam", 75, 0x0049_3100),
        ]
    );
    assert!(e
        .events
        .iter()
        .all(|x| !matches!(x, Event::PuzzleRan { .. })));
}

#[test]
fn the_yucatan_click_hooks() {
    // The dispatcher's choice: by prefix (any case), view, zone type, the
    // puzzle mode and the canoe's switch.
    let none: (Vec<u32>, bool) = (Vec::new(), false);
    assert_eq!(
        yucatan_click_hooks(b"yu0055.hnm", 55, 1, 0, false),
        (vec![0x0049_2d60], false)
    );
    assert_eq!(
        yucatan_click_hooks(b"YU0064.hnm", 64, 2, 0, false),
        (vec![0x0049_2e80], false)
    );
    assert_eq!(
        yucatan_click_hooks(b"yu0075.hnm", 75, 0x32, 0, false),
        (vec![0x0049_3510], false)
    );
    // Types 3 and 4 do not reach the grids.
    assert_eq!(yucatan_click_hooks(b"yu0075.hnm", 75, 3, 0, false), none);
    assert_eq!(
        yucatan_click_hooks(b"yu0121.hnm", 121, 0x32, 0, false),
        (vec![0x0049_4e80], false)
    );
    // The web's click in its mode, after a grid's leave.
    assert_eq!(
        yucatan_click_hooks(b"yu0055.hnm", 55, 1, WEB_MODE, false),
        (vec![0x0049_2d60, 0x0048_fdd0], false)
    );
    // The canoe: the click consumed until it is enabled.
    assert_eq!(
        yucatan_click_hooks(b"yu0402.hnm", 402, 3, 0, false),
        (Vec::new(), true)
    );
    assert_eq!(
        yucatan_click_hooks(b"yu0402.hnm", 402, 0x32, 0, true),
        (vec![0x0049_c010], false)
    );
    assert_eq!(yucatan_click_hooks(b"yu0402.hnm", 402, 1, 0, false), none);
    assert_eq!(
        yucatan_click_hooks(b"yu0122.hnm", 122, 4, 0, false),
        (vec![STATUE_BRANCH], false)
    );
    assert_eq!(yucatan_click_hooks(b"ir0055.hnm", 55, 1, 0, false), none);
    assert_eq!(yucatan_entry_hooks(b"ti0055.hnm", 55, 0), Vec::<u32>::new());
    assert_eq!(
        yucatan_entry_hooks(b"yu0198.hnm", 198, 0),
        vec![0x0048_f530]
    );
    assert_eq!(
        yucatan_entry_hooks(b"yu0198.hnm", 198, WEB_MODE),
        vec![0x0048_f9a0]
    );
    assert_eq!(
        yucatan_entry_hooks(b"ch0010.hnm", 10, WEB_MODE),
        vec![0x0048_f9a0]
    );
    assert_eq!(
        yucatan_entry_hooks(b"yu0121.hnm", 121, 0),
        vec![0x0049_4e00]
    );

    let Some(files) = game() else { return };
    // Each case: a zone of a grid's view clicked, its hook noted, then
    // `CLICZONE` and the click scan (type 50) or the move (types 1).
    let mut e = bare(files, "pyratez.wam", 55);
    let mut t = 10.0;
    for (view, click, leave) in [
        (55, 0x0049_31a0, 0x0049_2d60),
        (64, 0x0049_3490, 0x0049_2e80),
        (75, 0x0049_3510, 0x0049_2ef0),
    ] {
        e.enter_at(view, 0.0, 0.0).unwrap();
        let z = zone(&e, |z| z.kind == 50 && z.value == 3);
        aim(&mut e, z);
        e.events.clear();
        t += 10.0;
        e.click(t);
        e.pass(None);
        assert_eq!(noted(&e.events), [click], "view {view}");
        assert!(e
            .events
            .iter()
            .any(|x| matches!(x, Event::ClickScan(v) if v.cliczone == 3)));
        let z = zone(&e, |z| z.kind == 1);
        aim(&mut e, z);
        e.events.clear();
        t += 10.0;
        e.click(t);
        e.pass(None);
        assert_eq!(noted(&e.events), [leave], "view {view}");
        assert!(e
            .events
            .iter()
            .any(|x| matches!(x, Event::TurnSetup { .. })));
        settle(&mut e);
    }
    // The canoe's view (Xibalba's): its click consumed, nothing else.
    let canoe = Zone {
        kind: 3,
        value: 1,
        name: None,
        points: Vec::new(),
    };
    e.events.clear();
    assert!(e.yucatan_click(b"yu0402.hnm", 402, &canoe));
    assert!(noted(&e.events).is_empty());
}

/// The Tezcatlipoca statue (`xibalba.wam` view 122, chapter 39, loaded
/// here though the port refuses Xibalba to the scripts): a click on its
/// zone makes it the clicked character and opens its subject menu, with
/// no click scan; a subject asked starts its answer. Hidden, or another
/// zone: nothing.
#[test]
fn the_tezcatlipoca_statue_talks() {
    let Some(files) = game() else { return };
    let mut e = in_yucatan(files);
    e.load_place("xibalba.wam");
    e.enter_at(122, 0.0, 0.0).unwrap();
    // Chapter 39's `/INIT` enables the zone once the star map is done.
    e.run_command(b"(etoiles_fini=1)");
    e.run_command(b"(chapter=39)");
    settle(&mut e);
    let statue = id(&e, defnames::PERSONNAGES, "TEZ_STATUE");
    let z = zone(&e, named("ClicTezStatue"));
    let value = e.wam().view(122).unwrap().zones[z].value;
    assert_eq!(value, id(&e, defnames::ZONES_CLIC, "ClicTezStatue"));
    assert!(e.wam().view(122).unwrap().zones[z].kind < 88888);
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    e.pass(None);
    assert_eq!(noted(&e.events), [STATUE_BRANCH]);
    assert!(e.subject_menu.open);
    assert_eq!(var(&e, "CLICPERSO"), statue);
    // `CLICZONE` set by the zone click (and cleared at the frame's end).
    assert!(e
        .events
        .iter()
        .any(|x| matches!(x, Event::ZoneClick { cliczone, .. } if *cliczone == value)));
    assert_eq!(
        (e.cursor.bank, e.cursor.shown),
        (CursorBank::Interface, Some(40))
    );
    assert!(e.events.contains(&Event::SubjectMenu {
        character: statue,
        count: 4
    }));
    assert!(!e.events.iter().any(|x| matches!(x, Event::ClickScan(_))));
    // The first subject (TEZ): its answer, dialog 193.
    let (x, y) = atlantis2_engine::subjects::cell(4, 0);
    e.pointer(x + 31, y + 31);
    e.pass(None);
    e.events.clear();
    e.click(20.0);
    e.pass(None);
    assert!(!e.subject_menu.open);
    assert!(e.events.contains(&Event::DialogStart {
        number: 193,
        started: true
    }));
    settle(&mut e);
    // Hidden: no menu; a click of type 3 on another zone: no menu.
    e.run_command(b"(hideperso=TEZ_STATUE)");
    aim(&mut e, z);
    e.events.clear();
    e.click(30.0);
    e.pass(None);
    assert!(!e.subject_menu.open);
    assert!(noted(&e.events).is_empty());
    assert!(e.events.iter().any(|x| matches!(x, Event::ClickScan(_))));
}

/// Moves along each link between Yucatán's places, without the scripts (a
/// type-2 zone; the link's view is a destination of the view left, so the
/// change waits for the move's end). The transition video is the
/// destination old → new of the new place's file (@ 0x0045a518): none to
/// and from the king's hall (`salleroi.wam`'s view 39 and `yucatan.wam`'s
/// 80-82 have no destination), one to and from the pyramid, none out of
/// its secret door (`yucatan.wam` has no view 71).
#[test]
fn the_places_of_yucatan_are_linked() {
    let Some(files) = game() else { return };
    let mut e = bare(files, "yucatan.wam", 39);
    let trips = [
        (("yucatan.wam", 39), ("salleroi.wam", 80)),
        (("salleroi.wam", 80), ("yucatan.wam", 39)),
        (("yucatan.wam", 39), ("salleroi.wam", 81)),
        (("salleroi.wam", 81), ("yucatan.wam", 39)),
        (("yucatan.wam", 39), ("salleroi.wam", 82)),
        (("salleroi.wam", 82), ("yucatan.wam", 39)),
        (("yucatan.wam", 34), ("pyratez.wam", 50)),
        (("pyratez.wam", 50), ("yucatan.wam", 41)),
        (("pyratez.wam", 71), ("yucatan.wam", 28)),
    ];
    let mut t = 0.0;
    for ((from, v), (to, w)) in trips {
        e.load_place(from);
        e.enter_at(v, 0.0, 0.0).unwrap();
        let z = zone(&e, |z| z.kind == 2 && z.value == w as i32);
        aim(&mut e, z);
        e.events.clear();
        t += 10.0;
        e.click(t);
        let mut video = false;
        for _ in 0..600 {
            e.tick();
            e.pass(None);
            if e.loop_type == LoopType::Video {
                video = true;
                e.end_video();
            }
            if e.place() == (to, w) && e.loop_type == LoopType::Warp {
                break;
            }
        }
        assert_eq!(e.place(), (to, w), "from {from} {v}");
        let want: Vec<String> = e
            .wam()
            .view(v)
            .iter()
            .flat_map(|r| r.destinations.iter())
            .filter(|d| d.target == w)
            .map(|d| d.video.clone())
            .collect();
        let started: Vec<String> = e
            .events
            .iter()
            .filter_map(|x| match x {
                Event::VideoStart { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(started, want, "{from} {v} -> {to} {w}");
        assert_eq!(video, !want.is_empty(), "{from} {v} -> {to} {w}");
        assert_eq!(
            want.is_empty(),
            from == "salleroi.wam" || to == "salleroi.wam" || v == 71,
            "{from} {v} -> {to} {w}"
        );
        assert!(e.events.contains(&Event::LinkMove {
            wam: to.to_string(),
            view: w,
            deferred: true
        }));
        assert!(e.events.contains(&Event::Enter { view: w }));
    }
}

/// The first arrival in the king's hall (chapter 25's departure to view
/// 81, chapter 26's arrival): `AngleWarpXNow=270` turns the view at once
/// (the read-back yaw 90° − 270°, the pitch 0), then dialog 101.
#[test]
fn the_kings_hall_turns_the_view_at_the_arrival() {
    let Some(files) = game() else { return };
    let mut e = in_yucatan(files);
    goto(&mut e, "yucatan.wam", 39, 25);
    let z = zone(&e, |z| z.kind == 2 && z.value == 81);
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    for _ in 0..600 {
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Dialog {
            break;
        }
    }
    assert_eq!(e.loop_type, LoopType::Dialog);
    assert_eq!(e.place(), ("salleroi.wam", 81));
    assert_eq!(e.script_state().unwrap().chapter, 26);
    assert!(e.events.contains(&Event::DialogStart {
        number: 101,
        started: true
    }));
    let mut c = atlantis2_engine::Camera::new(e.camera.fpu, 640, 480);
    c.set_yaw(atlantis2_engine::camera::deg_to_rad(e.camera.fpu, 270));
    assert_eq!(e.camera.q, c.q);
    let (yaw, pitch) = e.camera.angles();
    assert!(
        (f64::from(yaw).to_degrees().abs() - 180.0).abs() < 1e-3,
        "{yaw}"
    );
    assert!(pitch.abs() < 1e-6);
    // The section waits for the dialog, then goes on.
    settle(&mut e);
    assert_eq!(e.script_state().unwrap().flags[76], 1);
}

/// Chapter 30's way back: the brazier of view 35 clicked with an empty
/// hand sends the game to Tibet's view 7 and chapter 75, disc 1.
#[test]
fn the_brazier_sends_the_game_back_to_tibet() {
    let Some(files) = game() else { return };
    let mut e = in_yucatan(files);
    goto(&mut e, "yucatan.wam", 35, 30);
    let z = zone(&e, named("BRAZERO_ZONE"));
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    run_until(&mut e, |e| e.place().0 == "tibet.wam");
    assert_eq!(e.place(), ("tibet.wam", 7));
    assert_eq!(e.script_state().unwrap().chapter, 75);
    assert_eq!(var(&e, "MONDE"), 5);
    assert_eq!(e.files().current_disc(), 1);
    assert!(e.events.contains(&Event::Monde(5)));
    assert!(!e
        .events
        .iter()
        .any(|x| matches!(x, Event::WorldNotPorted(_))));
    let lines: Vec<usize> = e
        .events
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter: 30, line } => Some(*line),
            _ => None,
        })
        .collect();
    assert_eq!(lines, (16..=21).collect::<Vec<_>>());
}

/// The end of Yucatán, chapter 28's arrival section: with the soul
/// (`CHULEL`, which only Xibalba gives, chapter 39) in the bag, the move
/// from view 85 to the king in view 86 (chapter 27's departure: chapter
/// 28) plays film 55 with dialog 168 over it, takes the soul, gives
/// Yucatán's mandala and sends the game to Tibet's view 7, chapter 75.
/// Every `/set` line run is one of the original's.
#[test]
fn the_soul_brought_to_the_king_ends_yucatan() {
    let Some(files) = game() else { return };
    let mut e = in_yucatan(files);
    goto(&mut e, "salleroi.wam", 85, 27);
    e.run_command(b"(sacados=CHULEL)");
    e.run_command(b"(objetenmain=0)");
    let z = zone(&e, |z| z.kind == 1 && z.value == 86);
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    run_until(&mut e, |e| e.place().0 == "tibet.wam");
    assert_eq!(e.place(), ("tibet.wam", 7));
    assert_eq!(e.script_state().unwrap().chapter, 75);
    assert_eq!((var(&e, "MONDE"), var(&e, "dial168_done")), (5, 1));
    assert_eq!(e.files().current_disc(), 1);
    assert!(in_bag(&e, "MANDALA_YUCATAN"));
    assert!(!in_bag(&e, "CHULEL"));
    let film = e
        .events
        .iter()
        .position(|x| matches!(x, Event::VideoStart { name, frames: Some(_) } if name.eq_ignore_ascii_case("cine055.hnm")))
        .expect("film 55");
    let tibet = e
        .events
        .iter()
        .position(|x| *x == Event::Wam("tibet.wam".into()))
        .unwrap();
    assert!(film < tibet);
    let lines: Vec<(i32, usize)> = e
        .events
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter, line } if (25..=34).contains(chapter) => Some((*chapter, *line)),
            _ => None,
        })
        .collect();
    assert!(lines.contains(&(27, 30)) && lines.contains(&(27, 31)));
    assert!((16..=32).all(|l| lines.contains(&(28, l))));
    let golden = include_str!("golden/scripts_worlds_china.txt");
    for (c, l) in lines {
        let head = format!("chapi{c:03}.con:{l} ");
        assert!(golden.lines().any(|g| g.starts_with(&head)), "{head}");
    }
}

/// The counting grids' testing aid: `finmayaN` = 1 and the win sound.
#[test]
fn the_grids_aid_writes_their_wins() {
    let Some(files) = game() else { return };
    let mut e = in_yucatan(files);
    goto(&mut e, "pyratez.wam", 55, 31);
    assert_eq!(var(&e, "finmaya1"), 0);
    for n in 1..=3 {
        e.events.clear();
        e.solve_maya_grid(n);
        assert_eq!(var(&e, &format!("finmaya{n}")), 1);
        assert!(e.events.contains(&Event::Sound("sound101.apc".into())));
    }
}

/// The values of `/set(key=...)` lines of Yucatán's chapters (comments
/// left out), each once, in order.
fn set_values(files: &A2Files, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chapter in CHAPTERS {
        let data = files
            .read(
                atlantis2_data::Folder::Scenar,
                &format!("chapi{chapter:03}.con"),
            )
            .unwrap();
        for line in data.split(|&c| c == b'\n') {
            let line: String = line.iter().map(|&c| char::from(c)).collect();
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let lower = line.to_ascii_lowercase();
            let head = format!("/set({}=", key.to_ascii_lowercase());
            if let Some(i) = lower.find(&head) {
                let v = &line[i + head.len()..];
                let v = v.split(')').next().unwrap().to_string();
                if !out.contains(&v) {
                    out.push(v);
                }
            }
        }
    }
    out
}

/// Every dialog Yucatán's chapters start (`dial=N`, and dialog 168 that
/// film 55 carries) plays to its end with its heads and bodies drawn.
#[test]
fn every_yucatan_dialog_plays() {
    let Some(files) = game() else { return };
    let mut numbers: Vec<u32> = set_values(&files, "dial")
        .iter()
        .filter_map(|v| v.parse().ok())
        .collect();
    numbers.push(168);
    numbers.sort_unstable();
    numbers.dedup();
    // 101..170 but 144, and 210.
    assert_eq!(numbers.len(), 70, "{numbers:?}");
    assert!(numbers
        .iter()
        .all(|&n| (101..=170).contains(&n) || n == 210));
    let mut e = in_yucatan(files);
    let mut screen = vec![0u16; 640 * 480];
    let mut lines = 0;
    for n in numbers {
        e.events.clear();
        e.start_dialog(n)
            .unwrap_or_else(|err| panic!("dialog {n}: {err}"));
        assert_eq!(e.loop_type, LoopType::Dialog, "dialog {n}");
        let mut ended = false;
        for pass in 0..20_000 {
            if pass % 20 == 19 {
                e.click(f64::from(pass) * 10.0 + f64::from(n) * 1e6);
            }
            e.tick();
            e.pass(Some(&mut screen));
            if e.events.contains(&Event::DialogEnd) {
                ended = true;
                break;
            }
        }
        assert!(ended, "dialog {n} does not end");
        lines += e
            .events
            .iter()
            .filter(|x| matches!(x, Event::DialogLine(_)))
            .count();
        settle(&mut e);
    }
    assert!(lines > 100, "{lines}");
}

#[test]
fn yucatan_subjects_objects_and_music_are_on_disc_2() {
    let Some(files) = game() else { return };
    let e = in_yucatan(files);
    let s = e.script_state().unwrap();
    let names = s.names.clone();
    let bank = |name: &str| {
        let (data, disc) = e
            .files()
            .read_with_disc(atlantis2_data::Folder::Spr, name)
            .unwrap_or_else(|| panic!("spr\\{name}"));
        assert!(disc <= 2, "{name} on disc {disc}");
        atlantis2_data::spr::SprBank::parse(&data).unwrap()
    };
    // Every subject the chapters enable has its picture in the world's
    // subjects bank (its parameter is the frame).
    let subjects = bank("sujyuc.spr");
    let mut n = 0;
    for list in set_values(e.files(), "enablesuj") {
        for subject in list.split(',').skip(1) {
            let frame = names.param(defnames::SUJETS, subject.as_bytes());
            assert!(
                subjects.info(frame as usize).is_some(),
                "{subject}: frame {frame} of sujyuc.spr"
            );
            n += 1;
        }
    }
    assert_eq!(n, 58);
    // Every object the chapters put in the bag, and every object a click
    // takes in a Yucatán place, has its picture in the objects bank.
    let objects = bank("objyuc.spr");
    let mut taken: Vec<String> = set_values(e.files(), "sacados")
        .into_iter()
        .filter(|v| !v.to_ascii_uppercase().starts_with("NOT_"))
        .collect();
    let mut e2 = bare(game().unwrap(), "yucatan.wam", 10);
    for place in PLACES {
        e2.load_place(place);
        for ent in &e2.wam().entities {
            if ent.kind == 2 && ent.state_records().any(|st| st.click == 1) {
                taken.push(ent.con_name.clone());
            }
        }
    }
    taken.sort();
    taken.dedup();
    assert_eq!(
        taken,
        [
            "BANDES_PROPRES",
            "BANDES_SANG",
            "MANDALA_YUCATAN",
            "OBSIDIENNE1",
            "PIED2_CROCO"
        ]
    );
    for object in &taken {
        let frame = names.param(defnames::OBJETS, object.as_bytes());
        assert!(
            objects.info(frame as usize).is_some(),
            "{object}: frame {frame} of objyuc.spr"
        );
    }
    // Every music the chapters' lists name is on disc 2 with its marks.
    let mut musics = Vec::new();
    for list in set_values(e.files(), "startmusik") {
        for entry in atlantis2_engine::music::parse_list(list.as_bytes()) {
            if !musics.contains(&entry.music) {
                musics.push(entry.music);
            }
        }
    }
    musics.sort_unstable();
    assert_eq!(musics, [11, 12, 15, 16]);
    for m in &musics {
        let name = atlantis2_engine::music::music_name(*m);
        assert!(
            e.files()
                .read_from_disc(2, atlantis2_data::Folder::ApcMusic, &name)
                .is_some(),
            "{name}"
        );
    }
    // Chapter 25's music plays.
    assert!(e.music_playing().is_some_and(|m| [11, 12, 15].contains(&m)));
}

/// `yucatan.wam`'s place sounds (the only Yucatán place of the
/// executable's listener table that has sources) start in a view that
/// hears them.
#[test]
fn yucatan_place_sounds_play() {
    let Some(files) = game() else { return };
    let mut e = bare(files, "yucatan.wam", 2);
    assert!(!e.place.sources.is_empty());
    let mut started = false;
    for _ in 0..3000 {
        e.tick();
        e.pass(None);
        if e.events
            .iter()
            .any(|x| matches!(x, Event::PlaceSound { .. }))
        {
            started = true;
            break;
        }
    }
    assert!(started);
}

/// A game saved in the pyramid (chapter 34, the jaw open, the foot taken)
/// and loaded in a new game: the place, the view, the world on disc 2,
/// the chapter and the story.
#[test]
fn a_save_in_yucatan_loads() {
    let Some(files) = game() else { return };
    let mut e = in_yucatan(files);
    goto(&mut e, "pyratez.wam", 78, 34);
    for c in [
        &b"(croco_ouvert=1)"[..],
        b"(pied_pris=1)",
        b"(sacados=PIED2_CROCO)",
        b"(finmaya3=1)",
    ] {
        e.run_command(c);
    }
    let saved = e.save_data();
    let mut f = Engine::boot(
        game().unwrap(),
        Variant::default(),
        Fpu::default(),
        640,
        480,
    )
    .unwrap();
    f.start_new_game().unwrap();
    f.end_video();
    assert_eq!(f.files().current_disc(), 1);
    f.load_data(&saved).unwrap();
    settle(&mut f);
    assert_eq!(f.place(), ("pyratez.wam", 78));
    assert_eq!(f.script_state().unwrap().chapter, 34);
    assert_eq!(f.files().current_disc(), 2);
    assert_eq!(var(&f, "MONDE"), 2);
    assert_eq!((var(&f, "croco_ouvert"), var(&f, "finmaya3")), (1, 1));
    assert!(in_bag(&f, "PIED2_CROCO"));
}

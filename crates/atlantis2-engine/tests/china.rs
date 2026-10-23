//! China (world 3) without its puzzles: every Chinese chapter's `/INIT`
//! run; every view of the five places entered, the puzzles' entry hooks
//! noted; the click hooks, which take no click; the moves between the
//! places (type-2 zones and WAM links); `NextWarpInvalide` as chapter 59's
//! door uses it; the picture viewer (`ViewBigSpr`); a demon's special
//! event; chapter 46's lantern back to Tibet; every dialog the chapters
//! start; the subjects, objects and musics on disc 3; the place sounds; a
//! save in China loaded; the testing aid that writes the puzzles' wins.
//!
//! The five wheels (`SS` 12-13) are ported (`tests/wheels.rs`): their
//! hooks run here and are counted with the others.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::A2Files;
use atlantis2_engine::engine::{chinese_click_hooks, chinese_entry_hooks, ChinesePuzzle};
use atlantis2_engine::{Engine, EngineError, Event, Fpu, LoopType, Variant};

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

/// China's chapters (the files of `scenar\`).
const CHAPTERS: [i32; 13] = [42, 43, 44, 45, 46, 47, 48, 55, 56, 57, 58, 59, 60];

/// China's places.
const PLACES: [&str; 5] = [
    "chine.wam",
    "miniature.wam",
    "salletortue.wam",
    "labysol.wam",
    "labyplaf.wam",
];

/// A game in China as the boat's trip leaves it, without the trip: a new
/// game's film ended, then the section's commands (the world, the place,
/// view 10, chapter 42).
fn in_china(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=3)"[..],
        b"(changewam=chine.wam)",
        b"(vue=10)",
        b"(chapter=42)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("chine.wam", 10));
    assert_eq!(e.files().current_disc(), 3);
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

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
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
fn zone(e: &Engine, pick: impl Fn(&atlantis2_data::wam::Zone) -> bool) -> usize {
    let view = e.place().1;
    e.wam()
        .view(view)
        .unwrap()
        .zones
        .iter()
        .position(pick)
        .unwrap_or_else(|| panic!("no such zone in view {view}"))
}

fn named(name: &str) -> impl Fn(&atlantis2_data::wam::Zone) -> bool + '_ {
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

/// The puzzle hooks noted (not ported) or run (the wheels', ported) in
/// `events`, in order; the wheels' check (FUN_0048d310, run when a disc
/// is laid) left out.
fn noted(events: &[Event]) -> Vec<u32> {
    events
        .iter()
        .filter_map(|x| match x {
            Event::PuzzleNotPorted { function } => Some(*function),
            Event::PuzzleRan { function } if *function != 0x0048_d310 => Some(*function),
            _ => None,
        })
        .collect()
}

/// The wheels' hooks (FUN_0048d080, FUN_0048d170) run, never noted as
/// missing.
fn wheels_ported(events: &[Event]) -> bool {
    !events.iter().any(|x| {
        matches!(
            x,
            Event::PuzzleNotPorted {
                function: 0x0048_d080 | 0x0048_d170
            }
        )
    })
}

#[test]
fn every_chinese_chapter_runs_its_init() {
    let Some(files) = game() else { return };
    let mut e = in_china(files);
    // The world's banks and texts (`Interp_SetMonde(3)`): disc 3.
    assert_eq!(var(&e, "MONDE"), 3);
    assert!(e.script().is_some());
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
fn every_chinese_view_is_entered_with_its_hooks() {
    let Some(files) = game() else { return };
    let mut e = bare(files, "chine.wam", 10);
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
            assert_eq!(n, chinese_entry_hooks(name.as_bytes(), id), "{place} {id}");
            assert!(wheels_ported(&e.events), "{place} {id}");
            hooks.extend(n.into_iter().map(|f| (place, id, f)));
        }
    }
    // 151 views; four records only stand for a link's other end (their
    // panoramas are on no disc, and no move enters them: the link loads
    // the other place first).
    assert_eq!(views, 151);
    assert_eq!(
        missing_warp,
        [
            "miniature.wam 47 ss0047.hnm",
            "miniature.wam 122 ss0122.hnm",
            "salletortue.wam 32 ch0065.hnm",
            "labysol.wam 123 la0123.hnm",
        ]
    );
    // One place animation is on no disc (disc 3 holds it renamed
    // `1ma1024025.err.SPW`; the original logs it too).
    assert_eq!(missing_spw, ["1mA1024025.SPW"]);
    // The hooks: the turtle ring in every `CH` view 40-64 (and
    // `chine.wam`'s view 40), the wheels in `SS` 12-13, the chest and the
    // figurines in `SS` 1-7, the clouds in `SS` 31, the souls in every
    // `LA` view.
    let count = |f: u32| hooks.iter().filter(|h| h.2 == f).count();
    assert_eq!(count(0x0049_c340), 25);
    assert_eq!(count(0x0048_d080), 2);
    assert_eq!(count(0x0049_38f0), 7);
    assert_eq!(count(0x0048_d680), 7);
    assert_eq!(count(0x0049_44a0), 1);
    assert_eq!(count(0x0048_df10), 62);
    assert_eq!(hooks.len(), 104);
}

#[test]
fn the_chinese_click_hooks_take_no_click() {
    // The dispatcher's choice: by prefix (any case), view and zone type.
    assert_eq!(
        chinese_click_hooks(b"ss0006.hnm", 6, 3),
        [0x0049_3960, 0x0048_da20]
    );
    assert_eq!(chinese_click_hooks(b"SS0005.hnm", 5, 1), [0x0049_3960]);
    assert_eq!(chinese_click_hooks(b"ss0013.hnm", 13, 3), [0x0048_d170]);
    assert_eq!(chinese_click_hooks(b"ss0013.hnm", 13, 1), Vec::<u32>::new());
    assert_eq!(chinese_click_hooks(b"ss0031.hnm", 31, 1), [0x0049_45f0]);
    assert_eq!(chinese_click_hooks(b"ss0031.hnm", 31, 0x32), [0x0049_4600]);
    assert_eq!(chinese_click_hooks(b"ch0056.hnm", 56, 3), [0x0049_c290]);
    assert_eq!(chinese_click_hooks(b"ch0064.hnm", 64, 4), [0x0049_c290]);
    assert_eq!(chinese_click_hooks(b"ch0065.hnm", 65, 3), Vec::<u32>::new());
    assert_eq!(chinese_click_hooks(b"ch0056.hnm", 56, 1), Vec::<u32>::new());
    assert_eq!(chinese_click_hooks(b"la0025.hnm", 25, 3), Vec::<u32>::new());
    assert_eq!(chinese_click_hooks(b"ir0006.hnm", 6, 3), Vec::<u32>::new());
    assert_eq!(chinese_entry_hooks(b"ti0040.hnm", 40), Vec::<u32>::new());

    let Some(files) = game() else { return };
    // Each case: a zone clicked, its hooks noted in order, then `CLICZONE`
    // and the click scan (types 3, 4, 50) or the move (types 1, 2), as
    // without them.
    let cases: [(&str, u32, &str, Vec<u32>); 5] = [
        (
            "miniature.wam",
            6,
            "ENCENS1",
            vec![0x0049_3960, 0x0048_da20],
        ),
        ("miniature.wam", 5, "PRENDRE_BAGUETTE", vec![0x0049_3960]),
        ("miniature.wam", 13, "AXE_5ROUES1", vec![0x0048_d170]),
        ("miniature.wam", 31, "CLIC_DRAGON", vec![0x0049_4600]),
        ("salletortue.wam", 56, "TORTUE_GAUCHE", vec![0x0049_c290]),
    ];
    let mut e = bare(files, "chine.wam", 10);
    let mut t = 10.0;
    for (place, view, name, want) in cases {
        e.load_place(place);
        e.enter_at(view, 0.0, 0.0).unwrap();
        let z = zone(&e, named(name));
        let value = e.wam().view(view).unwrap().zones[z].value;
        aim(&mut e, z);
        e.events.clear();
        t += 10.0;
        e.click(t);
        e.pass(None);
        assert_eq!(noted(&e.events), want, "{name}");
        assert!(wheels_ported(&e.events), "{name}");
        assert!(
            e.events
                .iter()
                .any(|x| matches!(x, Event::ClickScan(v) if v.cliczone == value)),
            "{name}"
        );
    }
    // A move zone of `SS` view 31 (FUN_004945f0, the mode back to 0):
    // noted, and the move goes on.
    e.load_place("miniature.wam");
    e.enter_at(31, 0.0, 0.0).unwrap();
    let z = zone(&e, |z| z.kind == 1);
    aim(&mut e, z);
    e.events.clear();
    e.click(100.0);
    e.pass(None);
    assert_eq!(noted(&e.events), [0x0049_45f0]);
    assert!(e
        .events
        .iter()
        .any(|x| matches!(x, Event::TurnSetup { .. })));
}

/// Moves along each link between China's places, without the scripts (a
/// type-2 zone; the link's view is a destination of the view left, so the
/// change waits for the move's end). The transition video is the
/// destination old → new of the new place's file (@ 0x0045a518): none
/// between `chine.wam`, `salletortue.wam` and `miniature.wam` (the
/// records of the views left, in the new file, have no destination), one
/// to and from the labyrinth, where `ss003123.HNM` (back to `miniature.wam`)
/// is on no disc: a cut.
#[test]
fn the_places_of_china_are_linked() {
    let Some(files) = game() else { return };
    let mut e = bare(files, "chine.wam", 32);
    let trips = [
        (("chine.wam", 32), ("salletortue.wam", 40)),
        (("salletortue.wam", 40), ("chine.wam", 32)),
        (("salletortue.wam", 47), ("miniature.wam", 1)),
        (("miniature.wam", 1), ("salletortue.wam", 47)),
        (("miniature.wam", 123), ("labysol.wam", 3)),
        (("labysol.wam", 3), ("miniature.wam", 123)),
        (("labysol.wam", 28), ("labyplaf.wam", 33)),
        (("labyplaf.wam", 33), ("labysol.wam", 28)),
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
        let started: Vec<(String, bool)> = e
            .events
            .iter()
            .filter_map(|x| match x {
                Event::VideoStart { name, frames } => Some((name.clone(), frames.is_some())),
                _ => None,
            })
            .collect();
        assert_eq!(
            started.iter().map(|s| s.0.clone()).collect::<Vec<_>>(),
            want,
            "{from} {v} -> {to} {w}"
        );
        assert_eq!(
            want.is_empty(),
            !from.starts_with("laby") && !to.starts_with("laby")
        );
        assert_eq!(video, started.iter().any(|s| s.1));
        assert_eq!(video, !want.is_empty() && want[0] != "ss003123.HNM");
        assert!(e.events.contains(&Event::LinkMove {
            wam: to.to_string(),
            view: w,
            deferred: true
        }));
        assert!(e.events.contains(&Event::Enter { view: w }));
    }
}

/// Chapter 59's door (`miniature.wam` view 22 to 122, whose panorama is on
/// no disc): the departure sets `NextWarpInvalide` = 1, the move's view
/// entry is skipped (the variable 2), the transition video plays, the
/// frame after it draws nothing, and the arrival scan sends the game back
/// to view 22 (`vue=22` clears the variable).
#[test]
fn next_warp_invalide_skips_the_view_entry() {
    let Some(files) = game() else { return };
    let mut e = in_china(files);
    goto(&mut e, "miniature.wam", 22, 59);
    let z = zone(&e, |z| z.kind == 1 && z.value == 122);
    aim(&mut e, z);
    e.events.clear();
    e.click(50.0);
    let mut screen = vec![0u16; 640 * 480];
    let mut frozen = 0;
    for _ in 0..600 {
        e.tick();
        let invalid = var(&e, "NextWarpInvalide") == 2 && e.loop_type == LoopType::Warp;
        let before = screen.clone();
        e.pass(Some(&mut screen));
        if invalid && e.loop_type == LoopType::Warp {
            assert_eq!(screen, before, "a frozen frame was drawn");
            frozen += 1;
        }
        if e.loop_type == LoopType::Video {
            for _ in 0..10 {
                e.tick();
                e.pass(Some(&mut screen));
            }
            e.end_video();
        }
        if e.events.contains(&Event::Enter { view: 22 }) {
            break;
        }
    }
    let at = |p: &dyn Fn(&Event) -> bool| {
        e.events
            .iter()
            .position(p)
            .unwrap_or_else(|| panic!("{:?}", e.events))
    };
    let depart = at(&|x| matches!(x, Event::DepartScan(v) if v.departvue == 122));
    let sound = at(&|x| *x == Event::Sound("sound217.apc".into()));
    let skipped = at(&|x| *x == Event::EnterSkipped { view: 122 });
    let video = at(&|x| {
        matches!(
            x,
            Event::VideoStart {
                frames: Some(_),
                ..
            }
        )
    });
    let arrive = at(&|x| matches!(x, Event::ArrivalScan(v) if v.arriveevue == 122));
    let back = at(&|x| *x == Event::Enter { view: 22 });
    assert!(
        depart < sound && sound < skipped && skipped < video && video < arrive && arrive < back
    );
    assert!(!e.events.contains(&Event::Enter { view: 122 }));
    assert_eq!(frozen, 1);
    assert_eq!(e.place(), ("miniature.wam", 22));
    assert_eq!(var(&e, "NextWarpInvalide"), 0);
    assert_eq!(var(&e, "nbpassageporte"), 1);
}

/// `ViewBigSpr`: in the turtle room (chapter 46), a click on the turtle's
/// picture zone shows `zoomTor.spr` over the panorama until a click; a
/// command shows the map `CarteChi.spr`.
#[test]
fn a_picture_until_a_click() {
    let Some(files) = game() else { return };
    let mut e = in_china(files);
    goto(&mut e, "salletortue.wam", 56, 46);
    let z = zone(&e, named("TGA_TORTUE"));
    aim(&mut e, z);
    e.events.clear();
    e.click(50.0);
    e.pass(None);
    assert!(e.events.contains(&Event::Picture {
        name: "zoomTor.spr".into(),
        found: true
    }));
    assert_eq!(e.loop_type, LoopType::Picture);
    // The ring's hook (not ported) did not take the click.
    assert_eq!(noted(&e.events), [0x0049_c290]);
    let mut screen = vec![0u16; 640 * 480];
    let mut plain = vec![0u16; 640 * 480];
    e.tick();
    e.pass(Some(&mut screen));
    e.render(&mut plain);
    let info = e.picture().unwrap().info(0).unwrap();
    assert!(info.w > 100 && info.h > 100);
    assert_ne!(screen, plain, "the picture over the panorama");
    // Passes without a click keep it.
    for _ in 0..20 {
        e.tick();
        e.pass(None);
    }
    assert_eq!(e.loop_type, LoopType::Picture);
    e.click(60.0);
    e.pass(None);
    assert!(e.events.contains(&Event::PictureEnd));
    assert_eq!(e.loop_type, LoopType::Warp);
    assert!(e.picture().is_none());
    // The map.
    e.run_command(b"(ViewBigSpr=CarteChi.spr)");
    assert_eq!(e.loop_type, LoopType::Picture);
    assert!(e.events.contains(&Event::Picture {
        name: "CarteChi.spr".into(),
        found: true
    }));
    e.click(70.0);
    e.pass(None);
    assert_eq!(e.loop_type, LoopType::Warp);
}

/// A demon's special event as chapter 60 starts it (the rat, view 25, an
/// empty form given): its animation played on, the sound at frame 51, the
/// event's end at frame 60 (the record rewound), then the section of
/// `SpecialEventDone=1` gives back the stamped form and plays the
/// remembered dialog.
#[test]
fn a_demons_special_event() {
    let Some(files) = game() else { return };
    let mut e = in_china(files);
    goto(&mut e, "labysol.wam", 25, 60);
    let s = e.script_state().unwrap();
    let rat = s.names.id(defnames::PERSONNAGES, b"DEMON_RAT").unwrap() as i32;
    let form1 = s.names.id(defnames::OBJETS, b"FORMULAIRE_1").unwrap() as i32;
    let slot = e.anim.slots.iter().position(|s| s.id == rat).unwrap();
    for c in [
        &b"(MemoNumDial=333)"[..],
        b"(tampondemon=1)",
        b"(ContinueAnim=DEMON_RAT)",
        b"(TestSpecialEvent=1)",
    ] {
        e.run_command(c);
    }
    e.events.clear();
    let mut frames = Vec::new();
    for _ in 0..4000 {
        e.tick();
        e.pass(None);
        frames.push(e.anim.slots[slot].frame);
        if var(&e, "TestSpecialEvent") == 0 {
            break;
        }
    }
    assert_eq!(var(&e, "TestSpecialEvent"), 0, "{frames:?}");
    assert!(frames.contains(&51));
    let sound = e
        .events
        .iter()
        .position(|x| *x == Event::Sound("sound203.apc".into()))
        .expect("sound 203");
    assert!(e.events[sound..].contains(&Event::DialogStart {
        number: 333,
        started: true
    }));
    assert_eq!(e.anim.slots[slot].frame, 0);
    assert_eq!(var(&e, "SpecialEventDone"), 0);
    assert_eq!(var(&e, "OBJETENMAIN"), form1);
    assert!(e.bag_entries().iter().any(|b| b.id == form1));
}

/// Chapter 46's lantern: with the lantern brought back (`flag177` = 1)
/// and China not finished, the move from the turtle room's view 40 to
/// `chine.wam` view 32 runs the departure section (lines 53-78): film 32,
/// `chine.wam` view 200, the lantern out of the bag, dialogs 346 and 347,
/// film 33, then Tibet (world 5, disc 1) in view 7 with China's mandala
/// in the bag, `isChineFinished` = 1 and chapter 75. The hand is empty
/// there, as in the original: the arrival's per-view reset empties it, and
/// `ObjetEnMain_NextWarp=MANDALA_CHINE` stores `atoi` of a name, 0. Every
/// `/set` line run is one the original's handler runs.
#[test]
fn the_lantern_sends_the_game_back_to_tibet() {
    let Some(files) = game() else { return };
    let mut e = in_china(files);
    goto(&mut e, "salletortue.wam", 40, 46);
    for c in [
        &b"(sacados=LANTERNE)"[..],
        b"(flag177=1)",
        b"(isChineFinished=0)",
    ] {
        e.run_command(c);
    }
    let z = zone(&e, |z| z.kind == 2 && z.value == 32);
    aim(&mut e, z);
    e.events.clear();
    e.click(50.0);
    for _ in 0..2000 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.place() == ("tibet.wam", 7) && e.loop_type == LoopType::Warp && !e.turning() {
            break;
        }
    }
    assert_eq!(e.place(), ("tibet.wam", 7));
    assert_eq!(e.script_state().unwrap().chapter, 75);
    assert_eq!((var(&e, "MONDE"), var(&e, "isChineFinished")), (5, 1));
    assert_eq!(e.files().current_disc(), 1);
    let at = |p: &dyn Fn(&Event) -> bool| {
        e.events
            .iter()
            .position(p)
            .unwrap_or_else(|| panic!("{:?}", e.events))
    };
    let film = |n: &'static str| move |x: &Event| matches!(x, Event::VideoStart { name, frames: Some(_) } if name.eq_ignore_ascii_case(n));
    let f32_ = at(&film("cine032.hnm"));
    let china = at(&|x| *x == Event::Enter { view: 200 });
    let d346 = at(&|x| {
        *x == Event::DialogStart {
            number: 346,
            started: true,
        }
    });
    let d347 = at(&|x| {
        *x == Event::DialogStart {
            number: 347,
            started: true,
        }
    });
    let f33 = at(&film("cine033.hnm"));
    let world = at(&|x| *x == Event::Monde(5));
    let tibet = at(&|x| *x == Event::Wam("tibet.wam".into()));
    assert!(
        f32_ < china && china < d346 && d346 < d347 && d347 < f33 && f33 < world && world < tibet
    );
    let s = e.script_state().unwrap();
    let lantern = s.names.id(defnames::OBJETS, b"LANTERNE").unwrap() as i32;
    let mandala = s.names.id(defnames::OBJETS, b"MANDALA_CHINE").unwrap() as i32;
    assert!(!e.bag_entries().iter().any(|b| b.id == lantern));
    assert!(e.bag_entries().iter().any(|b| b.id == mandala));
    assert_eq!(
        (var(&e, "OBJETENMAIN"), var(&e, "Objetenmain_NextWarp")),
        (0, 0)
    );
    let lines: Vec<usize> = e
        .events
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter: 46, line } => Some(*line),
            _ => None,
        })
        .collect();
    assert_eq!(lines, (53..=78).collect::<Vec<_>>());
    let golden = include_str!("golden/scripts_worlds_china.txt");
    for line in lines {
        let head = format!("chapi046.con:{line} ");
        assert!(golden.lines().any(|l| l.starts_with(&head)), "line {line}");
    }
}

/// The values of `/set(key=...)` lines of China's chapters (comments
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

/// Every dialog China's chapters start (`dial=N`, and the numbers
/// `MemoNumDial` keeps for `dial=MemoNumDial`) plays to its end with its
/// heads and bodies drawn.
#[test]
fn every_chinese_dialog_plays() {
    let Some(files) = game() else { return };
    let mut numbers: Vec<u32> = set_values(&files, "dial")
        .iter()
        .chain(set_values(&files, "MemoNumDial").iter())
        .filter_map(|v| v.parse().ok())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    assert_eq!(numbers.len(), 115, "{numbers:?}");
    assert!(
        numbers.iter().all(|&n| (220..=360).contains(&n)),
        "{numbers:?}"
    );
    let mut e = in_china(files);
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
    assert!(lines > 300, "{lines}");
}

#[test]
fn chinese_subjects_objects_and_music_are_on_disc_3() {
    let Some(files) = game() else { return };
    let e = in_china(files);
    let s = e.script_state().unwrap();
    let names = s.names.clone();
    let bank = |name: &str| {
        let (data, disc) = e
            .files()
            .read_with_disc(atlantis2_data::Folder::Spr, name)
            .unwrap_or_else(|| panic!("spr\\{name}"));
        assert!(disc <= 3, "{name} on disc {disc}");
        atlantis2_data::spr::SprBank::parse(&data).unwrap()
    };
    // Every subject the chapters enable has its picture in the world's
    // subjects bank (its parameter is the frame).
    let subjects = bank("sujchine.spr");
    let mut n = 0;
    for list in set_values(e.files(), "enablesuj") {
        for subject in list.split(',').skip(1) {
            let frame = names.param(defnames::SUJETS, subject.as_bytes());
            assert!(
                subjects.info(frame as usize).is_some(),
                "{subject}: frame {frame} of sujchine.spr"
            );
            n += 1;
        }
    }
    assert_eq!(n, 39);
    // Every object the chapters put in the bag, and every object a click
    // takes in a Chinese place, has its picture in the objects bank.
    let objects = bank("objchine.spr");
    let mut taken: Vec<String> = set_values(e.files(), "sacados")
        .into_iter()
        .filter(|v| !v.to_ascii_uppercase().starts_with("NOT_"))
        .collect();
    let mut e2 = bare(game().unwrap(), "chine.wam", 10);
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
    assert!(taken.len() > 15, "{taken:?}");
    for object in &taken {
        let frame = names.param(defnames::OBJETS, object.as_bytes());
        assert!(
            objects.info(frame as usize).is_some(),
            "{object}: frame {frame} of objchine.spr"
        );
    }
    // Every music the chapters' lists name is on disc 3 with its marks.
    // Chapter 75's trip also copies music 21 (`LOADMUSIK=20,21,22,23`),
    // which no list names and no disc holds.
    let mut musics = Vec::new();
    for list in set_values(e.files(), "startmusik") {
        for entry in atlantis2_engine::music::parse_list(list.as_bytes()) {
            if !musics.contains(&entry.music) {
                musics.push(entry.music);
            }
        }
    }
    musics.sort_unstable();
    assert_eq!(musics, [20, 22, 23]);
    for m in musics {
        let name = atlantis2_engine::music::music_name(m);
        assert!(
            e.files()
                .read_from_disc(3, atlantis2_data::Folder::ApcMusic, &name)
                .is_some(),
            "{name}"
        );
        assert!(e
            .files()
            .read_from_disc(
                3,
                atlantis2_data::Folder::ApcMusic,
                &atlantis2_engine::music::spp_name(m)
            )
            .is_some());
    }
    let name = atlantis2_engine::music::music_name(21);
    assert!(e
        .files()
        .read(atlantis2_data::Folder::ApcMusic, &name)
        .is_none());
    // Chapter 42's music plays.
    assert!(e.music_playing().is_some_and(|m| (20..=23).contains(&m)));
}

/// `chine.wam`'s and `miniature.wam`'s place sounds (the places of the
/// executable's listener table that have sources) start in a view that
/// hears them.
#[test]
fn chinese_place_sounds_play() {
    let Some(files) = game() else { return };
    for (place, view) in [("chine.wam", 1), ("miniature.wam", 21)] {
        let mut e = bare(game().unwrap(), place, view);
        assert!(!e.place.sources.is_empty(), "{place}");
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
        assert!(started, "{place} view {view}");
    }
    drop(files);
}

/// The testing aid writes what the puzzles' wins write.
#[test]
fn the_aid_writes_the_wins() {
    let Some(files) = game() else { return };
    let mut e = in_china(files);
    goto(&mut e, "miniature.wam", 6, 48);
    e.events.clear();
    e.solve_chinese_puzzle(ChinesePuzzle::Figurines);
    assert_eq!(var(&e, "etat5statues"), 1);
    // `AtlHnm_start("???")`: no such film, the section goes on.
    assert!(e.events.contains(&Event::VideoStart {
        name: "???".into(),
        frames: None
    }));
    assert_eq!(e.loop_type, LoopType::Warp);
    let pos: Vec<i32> = (1..=6)
        .map(|k| var(&e, &format!("pos_statue{k}")))
        .collect();
    assert_eq!(pos, [4, 3, 5, 1, 2, 6]);
    e.solve_chinese_puzzle(ChinesePuzzle::Chest);
    let s = e.script_state().unwrap();
    let chopstick = s.names.id(defnames::OBJETS, b"BAGUETTE").unwrap() as i32;
    assert!(e.bag_entries().iter().any(|b| b.id == chopstick));
    assert_eq!(
        (var(&e, "frame_coffre_chine"), var(&e, "BAGUETTE_PRISE")),
        (4, 1)
    );
    e.events.clear();
    e.solve_chinese_puzzle(ChinesePuzzle::Pearl);
    assert_eq!(var(&e, "FINPERLE"), 1);
    assert_eq!(
        (var(&e, "State_nuages_bas"), var(&e, "Action_nuage_bas")),
        (5, 0)
    );
    assert!(e.events.contains(&Event::Sound("sound210.apc".into())));
    let pearl = e
        .script_state()
        .unwrap()
        .names
        .id(defnames::OBJETS, b"PERLE_TROU")
        .unwrap() as i32;
    assert!(e
        .script_state()
        .unwrap()
        .shown
        .iter()
        .any(|r| r.id == pearl && r.shown));
}

/// A game saved in China (the turtle room, chapter 46, whose `/INIT` a
/// load runs again) and loaded in a new game: the place, the view, the
/// world on disc 3, the chapter and the story.
#[test]
fn a_save_in_china_loads() {
    let Some(files) = game() else { return };
    let mut e = in_china(files);
    goto(&mut e, "salletortue.wam", 56, 46);
    e.run_command(b"(indication_tortue=3)");
    e.run_command(b"(sacados=TORTUE_NOIRE)");
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
    assert_eq!(f.place(), ("salletortue.wam", 56));
    assert_eq!(f.script_state().unwrap().chapter, 46);
    assert_eq!(f.files().current_disc(), 3);
    assert_eq!(var(&f, "MONDE"), 3);
    assert_eq!(var(&f, "indication_tortue"), 3);
    let turtle = f
        .script_state()
        .unwrap()
        .names
        .id(defnames::OBJETS, b"TORTUE_NOIRE")
        .unwrap() as i32;
    assert!(f.bag_entries().iter().any(|b| b.id == turtle));
}

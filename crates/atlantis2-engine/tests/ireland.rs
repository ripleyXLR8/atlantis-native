//! Ireland (world 1) without its four puzzles: every Irish chapter's
//! `/INIT` run, every view of the six places entered, the puzzles' hooks
//! noted and the variables their wins write set by the testing aid, the
//! moves to another place (a type-2 zone, at once or deferred), the
//! transition sprites over a transition video, and the horse's frame kept
//! across views (`FRAMECHEVAL`).
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::wam::Destination;
use atlantis2_data::A2Files;
use atlantis2_engine::engine::{irish_click_hook, irish_entry_hook, IrishPuzzle};
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

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

/// Ireland's chapters (the files of `scenar\`), the demo place's first.
const CHAPTERS: [i32; 19] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17, 19, 20, 24,
];

/// Ireland's places.
const PLACES: [&str; 5] = [
    "bigile1.wam",
    "livre.wam",
    "ilepoete.wam",
    "ilepoete2.wam",
    "puit.wam",
];

/// A game in Ireland as the boat's trip leaves it, without the trip: a
/// new game's film ended, then the section's commands (the world, the
/// place, view 5, chapter 2), the dialog of the first arrival ended.
fn in_ireland(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=1)"[..],
        b"(changewam=bigile1.wam)",
        b"(vue=5)",
        b"(chapter=2)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("bigile1.wam", 5));
    e.trace = true;
    e
}

/// Films and dialogs ended until the panorama is back, the passes run.
fn settle(e: &mut Engine) {
    for _ in 0..200 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Warp {
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
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zone));
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

#[test]
fn every_irish_chapter_runs_its_init() {
    let Some(files) = game() else { return };
    let mut e = in_ireland(files);
    for chapter in CHAPTERS {
        e.run_command(format!("(chapter={chapter})").as_bytes());
        settle(&mut e);
        let s = e.script_state().unwrap();
        assert!(s.fatal.is_none(), "chapter {chapter}: {:?}", s.fatal);
        assert_eq!(s.chapter, chapter);
        assert!(!s.travel_refused);
    }
    // Chapter 1's `/INIT` asks for the demo place, which the port does not
    // have (no script reaches chapter 1, none of its panoramas is on a
    // disc): refused, the game stays where it is.
    let mut e = in_ireland(game().unwrap());
    e.run_command(b"(chapter=1)");
    settle(&mut e);
    assert!(e.events.contains(&Event::WorldNotPorted("demo.wam".into())));
    assert_eq!(e.place(), ("bigile1.wam", 5));
}

#[test]
fn every_irish_view_is_entered_with_its_hooks() {
    let Some(files) = game() else { return };
    let mut e = bare(files, "bigile1.wam", 5);
    let mut hooks = Vec::new();
    let mut views = 0;
    let mut missing = Vec::new();
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
            e.enter_at(id, 0.0, 0.0)
                .unwrap_or_else(|err| panic!("{place} view {id}: {err}"));
            views += 1;
            missing.extend(e.events.iter().filter_map(|x| match x {
                Event::SpwLoad { name, found: false } => Some(name.clone()),
                _ => None,
            }));
            let noted: Vec<u32> = e
                .events
                .iter()
                .filter_map(|x| match x {
                    Event::PuzzleNotPorted { function } => Some(*function),
                    _ => None,
                })
                .collect();
            assert_eq!(
                noted,
                irish_entry_hook(name.as_bytes(), id)
                    .into_iter()
                    .collect::<Vec<_>>()
            );
            if let Some(f) = noted.first() {
                hooks.push((place.to_string(), id, *f));
            }
        }
    }
    assert_eq!(views, 83);
    // Every place animation found on a disc but one, which none has (a
    // shadow in view 1, hidden at load; the original logs it too).
    assert_eq!(missing, ["woA1001001.SPW"]);
    // The cross (view 2), the ogham stones (6), the skull (40-42), all in
    // `bigile1.wam`; the blackbird (73) in the poet's island
    // (`ilepoete.wam`).
    let want: Vec<(String, u32, u32)> = [
        ("bigile1.wam", 2, 0x0049_4120),
        ("bigile1.wam", 6, 0x0049_6020),
        ("bigile1.wam", 40, 0x0049_3cb0),
        ("bigile1.wam", 41, 0x0049_3cb0),
        ("bigile1.wam", 42, 0x0049_3cb0),
        ("ilepoete.wam", 73, 0x0049_58f0),
    ]
    .iter()
    .map(|&(p, v, f)| (p.to_string(), v, f))
    .collect();
    assert_eq!(hooks, want);
}

#[test]
fn the_irish_click_hooks_take_no_click() {
    // The dispatcher's choice: by prefix (any case), view and zone type;
    // the ogham's only with the knife.
    assert_eq!(
        irish_click_hook(b"ir0002.hnm", 2, 4, false),
        Some(0x0049_4190)
    );
    assert_eq!(
        irish_click_hook(b"IR0002.hnm", 2, 1, false),
        Some(0x0049_4170)
    );
    assert_eq!(irish_click_hook(b"ir0006.hnm", 6, 4, false), None);
    assert_eq!(
        irish_click_hook(b"ir0006.hnm", 6, 4, true),
        Some(0x0049_60a0)
    );
    assert_eq!(
        irish_click_hook(b"ir0006.hnm", 6, 2, false),
        Some(0x0049_6080)
    );
    assert_eq!(
        irish_click_hook(b"ir0042.hnm", 42, 3, false),
        Some(0x0049_3d40)
    );
    assert_eq!(
        irish_click_hook(b"ir0042.hnm", 42, 1, false),
        Some(0x0049_3d20)
    );
    assert_eq!(
        irish_click_hook(b"ir0073.hnm", 73, 0x32, false),
        Some(0x0049_5ae0)
    );
    assert_eq!(
        irish_click_hook(b"ir0073.hnm", 73, 2, false),
        Some(0x0049_59f0)
    );
    assert_eq!(irish_click_hook(b"ir0041.hnm", 41, 3, false), None);
    assert_eq!(irish_click_hook(b"ir0002.hnm", 2, 1000, false), None);
    assert_eq!(irish_click_hook(b"ti0002.hnm", 2, 4, false), None);

    let Some(files) = game() else { return };
    let mut e = bare(files, "bigile1.wam", 2);
    // A zone of the cross: the hook noted, then `CLICZONE` and the click
    // scan as for any zone.
    let zones = e.wam().view(2).unwrap().zones.clone();
    let z = zones.iter().position(|z| z.kind == 4).unwrap();
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    e.pass(None);
    let hook = e
        .events
        .iter()
        .position(|x| {
            *x == Event::PuzzleNotPorted {
                function: 0x0049_4190,
            }
        })
        .expect("the cross's hook");
    let scan = e
        .events
        .iter()
        .position(|x| matches!(x, Event::ClickScan(v) if v.cliczone == zones[z].value))
        .expect("the click scan");
    assert!(hook < scan);
    // Its move zone: the hook (FUN_00494170) and the move.
    let m = zones.iter().position(|z| z.kind == 1).unwrap();
    aim(&mut e, m);
    e.events.clear();
    e.click(20.0);
    e.pass(None);
    assert!(e.events.contains(&Event::PuzzleNotPorted {
        function: 0x0049_4170
    }));
    assert!(e
        .events
        .iter()
        .any(|x| matches!(x, Event::TurnSetup { .. })));
}

#[test]
fn the_puzzles_wins_set_their_variables() {
    let Some(files) = game() else { return };
    let mut e = in_ireland(files);
    let before = e.script_state().unwrap().vars.clone();
    e.solve_irish_puzzle(IrishPuzzle::Skull);
    assert_eq!(e.script_state().unwrap().vars, before);
    for (puzzle, name) in [
        (IrishPuzzle::Ogham, "FINOGHAM"),
        (IrishPuzzle::Blackbird, "FINMERLE"),
        (IrishPuzzle::Cross, "FINCROIX"),
    ] {
        assert_eq!(var(&e, name), 0);
        e.solve_irish_puzzle(puzzle);
        assert_eq!(var(&e, name), 1, "{name}");
    }
    // The ogham's win only sets a variable still 0.
    e.run_command(b"(finogham=5)");
    e.solve_irish_puzzle(IrishPuzzle::Ogham);
    assert_eq!(var(&e, "FINOGHAM"), 5);
}

#[test]
fn a_type_2_zone_changes_place() {
    let Some(files) = game() else { return };
    // `livre.wam` view 55: its type-2 zone (value 5) and the link to
    // `Bigile1.wam`; no destination 55 → 5: the place loaded and view 5
    // entered at once, no arrival scan, no video (@ 0x0040fc9f).
    let mut e = bare(files, "livre.wam", 55);
    let zones = e.wam().view(55).unwrap().zones.clone();
    let z = zones.iter().position(|z| z.kind == 2).unwrap();
    assert_eq!(zones[z].value, 5);
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    e.pass(None);
    assert!(e.events.contains(&Event::LinkMove {
        wam: "Bigile1.wam".into(),
        view: 5,
        deferred: false
    }));
    assert_eq!(e.place(), ("Bigile1.wam", 5));
    for _ in 0..10 {
        e.pass(None);
    }
    assert!(!e.events.iter().any(|x| matches!(
        x,
        Event::ArrivalScan(_) | Event::VideoStart { .. } | Event::TurnSetup { .. }
    )));
    assert_eq!(e.loop_type, LoopType::Warp);

    // With a destination 55 → 5 (made for the test, flag 1, a start angle
    // a quarter turn away): deferred (@ 0x0040fce0): the turn first, then
    // the place loaded at the move's end, view 5 entered at the
    // destination's end angles, its arrival scan; the video searched in
    // the new place (none there).
    let mut e = bare(game().unwrap(), "livre.wam", 55);
    {
        let wam = e.wam_mut();
        let v = wam
            .places
            .iter_mut()
            .flat_map(|p| p.views.iter_mut())
            .find(|v| v.id == 55)
            .unwrap();
        v.destinations.push(Destination {
            video: "055005.HNM".into(),
            target: 5,
            angles_as_given: 1,
            start_yaw: 1.5,
            start_pitch: 0.0,
            end_yaw: 0.5,
            end_pitch: 0.0,
            ..Default::default()
        });
    }
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    e.pass(None);
    assert!(e.events.contains(&Event::LinkMove {
        wam: "Bigile1.wam".into(),
        view: 5,
        deferred: true
    }));
    assert!(e.turning());
    assert_eq!(e.place(), ("livre.wam", 55));
    for _ in 0..200 {
        e.pass(None);
        if e.place().0 == "Bigile1.wam" {
            break;
        }
    }
    assert_eq!(e.place(), ("Bigile1.wam", 5));
    assert!(e.events.contains(&Event::Wam("Bigile1.wam".into())));
    assert_eq!((e.vars.last_vue, e.vars.arrivee_vue), (0, 5));
    let (yaw, _) = e.angles_deg();
    assert!((yaw - 0.5f32.to_degrees()).abs() < 0.01, "{yaw}");
    e.pass(None);
    assert!(e.events.iter().any(|x| matches!(x, Event::ArrivalScan(_))));
    assert!(!e
        .events
        .iter()
        .any(|x| matches!(x, Event::VideoStart { .. })));
}

#[test]
fn transition_sprites_over_the_video() {
    let Some(files) = game() else { return };
    // `bigile1.wam` 7 → 8: the video and two sprite banks (Liam's and a
    // hen's), loaded after the arrival, drawn over the video's frames,
    // freed at its end.
    let mut e = bare(files, "bigile1.wam", 7);
    let zones = e.wam().view(7).unwrap().zones.clone();
    let z = zones
        .iter()
        .position(|z| z.kind == 1 && z.value == 8)
        .unwrap();
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    let mut screen = vec![0u16; 640 * 480];
    for _ in 0..200 {
        e.pass(Some(&mut screen));
        if e.loop_type == LoopType::Video {
            break;
        }
    }
    assert_eq!(e.video_playing().map(|v| v.0), Some("ir007008.HNM"));
    let loaded: Vec<(String, bool)> = e
        .events
        .iter()
        .filter_map(|x| match x {
            Event::SpfLoad { name, found } => Some((name.clone(), *found)),
            _ => None,
        })
        .collect();
    assert_eq!(loaded.len(), 2);
    assert!(loaded.iter().all(|(n, f)| *f && n.ends_with("007008.spr")));
    assert_eq!(e.transition_sprites(), 2);
    // The sprites change the picture: the same frame drawn without them
    // differs.
    let mut with = vec![0u16; 640 * 480];
    e.render(&mut with);
    let kept = e.take_transition_sprites();
    let mut without = vec![0u16; 640 * 480];
    e.render(&mut without);
    assert_ne!(with, without);
    e.put_transition_sprites(kept);
    for _ in 0..2000 {
        e.tick();
        e.pass(Some(&mut screen));
        if e.loop_type == LoopType::Warp {
            break;
        }
    }
    assert_eq!(e.loop_type, LoopType::Warp);
    assert_eq!(e.transition_sprites(), 0);

    // A sprite whose character is hidden by the show / hide table is not
    // loaded (FUN_00484d90).
    let mut e = bare(game().unwrap(), "bigile1.wam", 7);
    let dn = atlantis2_data::defnames::DefNames::parse(
        &e.files()
            .read(atlantis2_data::Folder::Scenar, "defnames.con")
            .unwrap(),
    )
    .unwrap();
    let liam = dn.id(defnames::PERSONNAGES, b"LIAM_CELLULE").unwrap() as i32;
    e.anim.table.set(liam, 0, 1);
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    for _ in 0..200 {
        e.pass(None);
        if e.loop_type == LoopType::Video {
            break;
        }
    }
    assert_eq!(e.transition_sprites(), 1);
}

#[test]
fn the_horse_keeps_its_frame_across_views() {
    let Some(files) = game() else { return };
    let mut e = in_ireland(files);
    // The horse shown (it is hidden at load), view 13: its slot steps and
    // `FRAMECHEVAL` follows it.
    e.run_command(b"(showobjet=CHEVAL)");
    e.enter_at(13, 0.0, 0.0).unwrap();
    let horse = e
        .script_state()
        .unwrap()
        .names
        .id(defnames::OBJETS, b"CHEVAL")
        .unwrap() as i32;
    let slot = e.anim.slots.iter().position(|s| s.id == horse).unwrap();
    assert!(e.anim.slots[slot].active);
    for _ in 0..40 {
        e.tick();
        e.pass(None);
    }
    let frame = e.anim.slots[slot].frame;
    assert!(frame > 0);
    assert_eq!(var(&e, "FRAMECHEVAL"), frame);
    // A new view's horse starts at that frame.
    e.enter_at(14, 0.0, 0.0).unwrap();
    let slot = e.anim.slots.iter().position(|s| s.id == horse).unwrap();
    assert_eq!(e.anim.slots[slot].frame, frame);
    // `framecheval=0` resets it for the next view.
    e.run_command(b"(framecheval=0)");
    e.enter_at(15, 0.0, 0.0).unwrap();
    let slot = e.anim.slots.iter().position(|s| s.id == horse).unwrap();
    assert_eq!(e.anim.slots[slot].frame, 0);
}

/// The values a key takes in the Irish chapters' `/set` lines (not the
/// `#` ones), by the text after `KEY=` up to `)`.
fn set_values(files: &A2Files, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chapter in CHAPTERS.iter().skip(1) {
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
            let head = format!("/set({key}=");
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

#[test]
fn every_irish_dialog_plays() {
    let Some(files) = game() else { return };
    let mut numbers: Vec<u32> = set_values(&files, "dial")
        .iter()
        .filter_map(|v| v.parse().ok())
        .collect();
    // The dialog over film 19 (`cinema=19,71`).
    numbers.push(71);
    numbers.sort_unstable();
    assert!(numbers.len() >= 70, "{numbers:?}");
    let mut e = in_ireland(files);
    let mut screen = vec![0u16; 640 * 480];
    let mut lines = 0;
    for n in numbers {
        e.events.clear();
        e.start_dialog(n)
            .unwrap_or_else(|err| panic!("dialog {n}: {err}"));
        assert_eq!(e.loop_type, LoopType::Dialog, "dialog {n}");
        // Played with the heads and bodies drawn, a click every 20 passes.
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
    assert!(lines > 200, "{lines}");
}

#[test]
fn irish_subjects_objects_and_music_are_on_the_discs() {
    let Some(files) = game() else { return };
    let e = in_ireland(files);
    let s = e.script_state().unwrap();
    let names = s.names.clone();
    let bank = |name: &str| {
        atlantis2_data::spr::SprBank::parse(
            &e.files()
                .read(atlantis2_data::Folder::Spr, name)
                .unwrap_or_else(|| panic!("spr\\{name}")),
        )
        .unwrap()
    };
    // Every subject the chapters enable has its picture in the world's
    // subjects bank (its parameter is the frame).
    let subjects = bank("sujeire.spr");
    let mut n = 0;
    for list in set_values(e.files(), "enablesuj") {
        for subject in list.split(',').skip(1) {
            let frame = names.param(defnames::SUJETS, subject.as_bytes());
            assert!(
                subjects.info(frame as usize).is_some(),
                "{subject}: frame {frame} of sujeire.spr"
            );
            n += 1;
        }
    }
    assert!(n > 30);
    // Every object the chapters put in the bag, and every object a click
    // takes in an Irish place, has its picture in the objects bank.
    let objects = bank("objeire.spr");
    let mut taken: Vec<String> = set_values(e.files(), "sacados")
        .into_iter()
        .filter(|v| !v.to_ascii_uppercase().starts_with("NOT_"))
        .collect();
    let mut e2 = bare(game().unwrap(), "bigile1.wam", 5);
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
            "{object}: frame {frame} of objeire.spr"
        );
    }
    // Every music the chapters' lists name is on disc 1 with its marks.
    let mut musics = Vec::new();
    for list in set_values(e.files(), "startmusik") {
        for entry in atlantis2_engine::music::parse_list(list.as_bytes()) {
            if !musics.contains(&entry.music) {
                musics.push(entry.music);
            }
        }
    }
    musics.sort_unstable();
    assert_eq!(musics, [1, 2, 3, 4, 5]);
    for m in musics {
        let name = atlantis2_engine::music::music_name(m);
        assert!(e
            .files()
            .read_from_disc(1, atlantis2_data::Folder::ApcMusic, &name)
            .is_some());
    }
}

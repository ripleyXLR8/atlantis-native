//! The end of the game: the space-travel hub (`planetes.wam`, chapter 83,
//! the world kept), Shamballa and Atlantis (`MONDE` 4, disc 4: chapters
//! 78, 80-82 and 201 in `voiles.wam`, `rhea.wam`, `annulair.wam`,
//! `atlantis.wam`), the octopus's death (chapter 200, back to Tibet) and the
//! end (chapter 83's last section: films, the credits, the last film, the
//! quit). Every chapter's `/INIT`; every view entered with its hooks (the
//! fireflies' click run, the octopus's entry noted, not ported, with its
//! testing aid); the fireflies lit, their move end and their win (the
//! puzzle itself against the original in `tests/fireflies.rs`); the
//! vortex both ways; Rhea and
//! the way to the ring; the ring's rolling zoom, its characters and the
//! star; the octopus's two ends; the hub's travels from each world and
//! back, the balls made crystals; the end; every dialog; the subjects,
//! objects, musics and films; saves in Shamballa, Atlantis and the hub.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::wam::Zone;
use atlantis2_data::A2Files;
use atlantis2_engine::engine::{
    atlantis_entry_hooks, shamballa_click_hooks, FIREFLIES_CLICK, FIREFLIES_MOVE_END,
    OCTOPUS_ENTRY, OCTOPUS_TICK,
};
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

/// The chapters of the end (the files of `scenar\`).
const CHAPTERS: [i32; 7] = [78, 80, 81, 82, 83, 200, 201];

/// Shamballa's and Atlantis's places, then the hub.
const PLACES: [&str; 5] = [
    "voiles.wam",
    "rhea.wam",
    "annulair.wam",
    "atlantis.wam",
    "planetes.wam",
];

/// A new game's film ended, then `commands`, settled.
fn started(files: A2Files, commands: &[&str]) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in commands {
        e.run_command(c.as_bytes());
    }
    settle(&mut e);
    e.trace = true;
    e
}

/// A game in Shamballa as chapter 75's crystal leaves it, without the
/// trip: the world, the place, view 1, chapter 78.
fn in_shamballa(files: A2Files) -> Engine {
    let e = started(
        files,
        &[
            "(monde=4)",
            "(changewam=voiles.wam)",
            "(vue=1)",
            "(chapter=78)",
        ],
    );
    assert_eq!(e.place(), ("voiles.wam", 1));
    assert_eq!(e.files().current_disc(), 4);
    e
}

/// A game in Tibet's view `view` of chapter 75.
fn in_tibet(files: A2Files, view: u32) -> Engine {
    let e = started(
        files,
        &[
            "(changewam=tibet.wam)",
            &format!("(vue={view})"),
            "(chapter=75)",
        ],
    );
    assert_eq!(e.place(), ("tibet.wam", view));
    e
}

/// The game moved to `view` of `place` in chapter `chapter`.
fn goto(e: &mut Engine, place: &str, view: u32, chapter: i32) {
    for c in [
        format!("(changewam={place})"),
        format!("(vue={view})"),
        format!("(chapter={chapter})"),
    ] {
        e.run_command(c.as_bytes());
    }
    settle(e);
    assert!(e.place().0.eq_ignore_ascii_case(place));
    assert_eq!(e.place().1, view);
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

fn flag(e: &Engine, n: usize) -> i32 {
    e.script_state().unwrap().flags[n]
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
    e.camera.enter(yaw + 0.01, pitch);
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

/// A click on the zone `pick` accepts, then the passes until `done`.
fn click_zone(
    e: &mut Engine,
    t: f64,
    pick: impl Fn(&Zone) -> bool,
    done: impl Fn(&Engine) -> bool,
) {
    let z = zone(e, pick);
    aim(e, z);
    e.events.clear();
    e.click(t);
    run_until(e, done);
}

/// A move to view `to` of the current place, by its zone (types 1 and 2).
fn walk(e: &mut Engine, t: f64, to: u32) {
    let place = e.place().0.to_string();
    click_zone(
        e,
        t,
        |z| z.kind == 1 && z.value == to as i32,
        |e| e.place().0 == place && e.place().1 == to,
    );
}

/// The camera's projection at the warp's field of view (76°) under the
/// current control word, as a view entry and FUN_004952d0 set it.
fn warp_fov(e: &Engine) -> [f32; 4] {
    atlantis2_engine::Camera::new(e.fpu(), 640, 480).projection()
}

/// Passes run (a 15 Hz tick every fourth) until the music streamed is
/// `music`: the switch fades the music before.
fn music_reaches(e: &mut Engine, music: i32) -> bool {
    for n in 0..1200 {
        if e.music_playing() == Some(music) {
            return true;
        }
        if n % 4 == 0 {
            e.tick();
        }
        e.pass(None);
    }
    false
}

/// The angles of the texel (col, row) of face `face` of a 512 x 512 cube
/// face (as `tests/flow.rs`).
fn texel_angles(face: usize, col: f64, row: f64) -> (f32, f32) {
    let s = |t: f64| (t - 0.5) * 2.0 / 510.0 - 1.0;
    let (u, v) = (s(col), s(row));
    let [x, y, z] = match face {
        0 => [u, v, 1.0],
        1 => [-u, v, -1.0],
        2 => [u, -1.0, v],
        3 => [u, 1.0, -v],
        4 => [-1.0, v, u],
        _ => [1.0, v, -u],
    };
    (z.atan2(x) as f32, y.atan2((z * z + x * x).sqrt()) as f32)
}

/// The centre on the shown sprite of the first slot of `id` (a character
/// when `character`), the cursor showing it clickable.
fn aim_sprite(e: &mut Engine, character: bool, id: i32) -> usize {
    let n = e
        .anim
        .slots
        .iter()
        .position(|s| s.active && s.id == id && matches!(s.kind, 1 | 3) == character)
        .unwrap_or_else(|| panic!("no slot of {id}"));
    for face in 0..6 {
        let Some(b) = e.anim.slots[n].frame_box(face) else {
            continue;
        };
        if b.w == 0 || b.h == 0 {
            continue;
        }
        let (yaw, pitch) = texel_angles(
            face,
            f64::from(b.x) + f64::from(b.w) / 2.0 + 1.0,
            f64::from(b.y) + f64::from(b.h) / 2.0 + 1.0,
        );
        e.camera.enter(yaw, pitch);
        e.pass(None);
        if e.sprite_under_centre().1 != 0 {
            return n;
        }
    }
    panic!("slot {n} of {id} not under the centre");
}

/// The puzzle hooks reached in `events`, in order: run, noted or aided.
fn noted(events: &[Event]) -> Vec<u32> {
    events
        .iter()
        .filter_map(|x| match x {
            Event::PuzzleNotPorted { function }
            | Event::PuzzleRan { function }
            | Event::PuzzleAid { function } => Some(*function),
            _ => None,
        })
        .collect()
}

/// The `/set` lines run (chapter, line) in `events`.
fn set_lines(events: &[Event]) -> Vec<(i32, usize)> {
    events
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter, line } => Some((*chapter, *line)),
            _ => None,
        })
        .collect()
}

/// Each line one the original ran (`scripts_worlds_*.txt` rows).
fn lines_as_the_original(lines: &[(i32, usize)]) {
    let goldens = [
        include_str!("golden/scripts_worlds_ireland.txt"),
        include_str!("golden/scripts_worlds_china.txt"),
        include_str!("golden/scripts_worlds_endgame.txt"),
    ];
    for (c, l) in lines {
        if matches!(c, 0 | 75) {
            continue;
        }
        let head = format!("chapi{c:03}.con:{l} ");
        assert!(
            goldens
                .iter()
                .any(|g| g.lines().any(|r| r.starts_with(&head))),
            "{head}"
        );
    }
}

/// The films started in `events`, by number (`cineNNN.hnm`).
fn films(events: &[Event]) -> Vec<u32> {
    events
        .iter()
        .filter_map(|x| match x {
            Event::VideoStart { name, .. } => name
                .to_ascii_lowercase()
                .strip_prefix("cine")
                .and_then(|r| r.strip_suffix(".hnm"))
                .and_then(|n| n.parse().ok()),
            _ => None,
        })
        .collect()
}

fn dialogs(events: &[Event]) -> Vec<u32> {
    events
        .iter()
        .filter_map(|x| match x {
            Event::DialogStart {
                number,
                started: true,
            } => Some(*number),
            _ => None,
        })
        .collect()
}

#[test]
fn every_endgame_chapter_runs_its_init() {
    let Some(files) = game() else { return };
    let mut e = in_shamballa(files);
    assert_eq!(var(&e, "MONDE"), 4);
    // Chapter 83's `/INIT` with `SpaceDeep` 0 is the trap (a fatal error,
    // "never the case"): one trip made.
    e.run_command(b"(SpaceDeep=1)");
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
    // The trap, as the original: a fatal error.
    e.run_command(b"(SpaceDeep=0)");
    e.run_command(b"(chapter=83)");
    assert!(e.script_state().unwrap().fatal.is_some());
}

#[test]
fn every_endgame_view_is_entered_with_its_hooks() {
    let Some(files) = game() else { return };
    let mut e = in_shamballa(files);
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
        for (view, name) in ids {
            e.events.clear();
            match e.enter_at(view, 0.0, 0.0) {
                Ok(()) => {}
                Err(EngineError::NoWarp(w)) => {
                    missing_warp.push(format!("{place} {view} {w}"));
                    continue;
                }
                Err(err) => panic!("{place} view {view}: {err}"),
            }
            views += 1;
            missing_spw.extend(e.events.iter().filter_map(|x| match x {
                Event::SpwLoad { name, found: false } => Some(name.clone()),
                _ => None,
            }));
            let n = noted(&e.events);
            assert_eq!(
                n,
                atlantis_entry_hooks(name.as_bytes(), view),
                "{place} {view}"
            );
            hooks.extend(n.into_iter().map(|f| (place, view, f)));
        }
    }
    // 46 views entered. Five records have no panorama on any disc: the
    // hub's 10 (the way back's destination, left by the departure
    // section's place change before the move) and 11 (a record no zone or
    // destination leads to), Atlantis's 802 and
    // 810 (the holders of the octopuses' and the plankton's animations,
    // which the octopus's entry loads in the views it fights in) and 19
    // (a destination of view 18 that no zone leads to).
    assert_eq!(views, 46);
    assert_eq!(
        missing_warp,
        [
            "atlantis.wam 802 at0802.hnm",
            "atlantis.wam 810 at0810.hnm",
            "atlantis.wam 19 at0019.hnm",
            "planetes.wam 10 pl0010.hnm",
            "planetes.wam 11 pl0011.hnm"
        ]
    );
    assert!(missing_spw.is_empty(), "{missing_spw:?}");
    // The hooks: the octopus's entry in Atlantis's views 1-18, noted.
    let want: Vec<(&str, u32, u32)> = (1..=18)
        .map(|v| ("atlantis.wam", v, OCTOPUS_ENTRY))
        .collect();
    let mut got = hooks.clone();
    got.sort_by_key(|h| h.1);
    assert_eq!(got, want);
    assert!(e
        .events
        .iter()
        .all(|x| !matches!(x, Event::PuzzleRan { .. })));
}

/// `character` asked about `subject` through the menu its sprite's click
/// opens: the dialogs started, the game settled after them.
fn ask(e: &mut Engine, t: f64, who: &str, subject: &str) -> Vec<u32> {
    let character = id(e, defnames::PERSONNAGES, who);
    aim_sprite(e, true, character);
    e.events.clear();
    e.click(t);
    e.pass(None);
    assert!(e.subject_menu.open, "{who}: no menu");
    let s = e.script_state().unwrap();
    let value = s.names.param(defnames::SUJETS, subject.as_bytes());
    let list = s.subjects.list(character);
    let i = list
        .iter()
        .position(|x| x.value == value)
        .unwrap_or_else(|| panic!("{who}: no subject {subject}"));
    let (x, y) = atlantis2_engine::subjects::cell(list.len() as i32, i as i32);
    e.pointer(x + 31, y + 31);
    e.pass(None);
    e.click(t + 1.0);
    e.pass(None);
    assert!(!e.subject_menu.open);
    let mut evs = std::mem::take(&mut e.events);
    settle(e);
    evs.append(&mut e.events);
    dialogs(&evs)
}

/// Object `name` taken from the bag bar into the hand (the bar opened with
/// the right button, its cell clicked), the bag holding it alone.
fn pick(e: &mut Engine, t: f64, name: &str) {
    e.script_state_mut().unwrap().bag.clear();
    e.run_command(format!("(sacados={name})").as_bytes());
    e.right_click();
    assert!(e.bar.open);
    let (px, py) = e.pointer;
    e.mouse_motion(40 - px, 440 - py);
    e.click(t);
    e.pass(None);
    assert!(!e.bar.open);
}

#[test]
fn the_shamballa_and_atlantis_hooks() {
    // The dispatchers' choices: the prefix in any case, the view, the
    // zone type.
    assert_eq!(
        shamballa_click_hooks(b"sh0001.hnm", 1, 1),
        [FIREFLIES_CLICK]
    );
    assert_eq!(
        shamballa_click_hooks(b"SH0001.hnm", 1, 2),
        [FIREFLIES_CLICK]
    );
    assert!(shamballa_click_hooks(b"sh0001.hnm", 1, 3).is_empty());
    assert!(shamballa_click_hooks(b"sh0001.hnm", 2, 1).is_empty());
    assert!(shamballa_click_hooks(b"at0001.hnm", 1, 1).is_empty());
    assert_eq!(atlantis_entry_hooks(b"at0001.hnm", 1), [OCTOPUS_ENTRY]);
    assert_eq!(atlantis_entry_hooks(b"AT0019.hnm", 19), [OCTOPUS_ENTRY]);
    assert!(atlantis_entry_hooks(b"at0020.hnm", 20).is_empty());
    assert!(atlantis_entry_hooks(b"at0802.hnm", 802).is_empty());
    assert!(atlantis_entry_hooks(b"at0000.hnm", 0).is_empty());
    assert!(atlantis_entry_hooks(b"an0001.hnm", 1).is_empty());

    let Some(files) = game() else { return };
    let mut e = in_shamballa(files);
    // A firefly's zone (to view 3, not in the place): the hook run (the
    // count, sound 300), the departure section shows the firefly and starts
    // the zoom; the move goes on; at its end, without a view, FUN_004952d0
    // sets the field of view back to 76°.
    let firefly = id(&e, defnames::OBJETS, "LUCIOLLE3");
    assert!(e.anim.table.get(firefly, 2) < 1);
    let z = zone(&e, |z| z.kind == 1 && z.value == 3);
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    e.pass(None);
    assert_eq!(noted(&e.events), [FIREFLIES_CLICK]);
    assert!(e.events.contains(&Event::FireflySound));
    assert!(e.zoom.pending());
    assert!(e.turning());
    assert!(e.anim.table.get(firefly, 2) >= 1);
    run_until(&mut e, |e| !e.turning());
    assert_eq!(noted(&e.events), [FIREFLIES_CLICK, FIREFLIES_MOVE_END]);
    assert_eq!(e.place(), ("voiles.wam", 1));
    assert_eq!(e.zoom.current, 35.0, "under 0x0e7f the zoom stays at 35");
    assert_eq!(e.camera.projection(), warp_fov(&e));
    assert!(!e.events.iter().any(|x| matches!(x, Event::Enter { .. })));
    assert_eq!(
        (var(&e, "NbLucioleAllumed"), var(&e, "Puzzle_Lucioles_Fini")),
        (1, 0)
    );
    assert_eq!(var(&e, "Puzzle_Lucioles_AutoWin"), 0);
    assert_eq!(e.script_state().unwrap().chapter, 78);
    // The firefly of view 1 (the view itself): entered again, no move end.
    let z = zone(&e, |z| z.kind == 1 && z.value == 1);
    aim(&mut e, z);
    e.events.clear();
    e.click(20.0);
    run_until(&mut e, |e| !e.turning());
    assert_eq!(noted(&e.events), [FIREFLIES_CLICK]);
    assert!(e.events.contains(&Event::Enter { view: 1 }));
    assert_eq!(var(&e, "NbLucioleAllumed"), 2);
    // The six mandalas laid in Tibet in their hook's order: the sixth
    // firefly must be the one of the sixth mandala (`MANDALA_YUCATAN`,
    // `OrdreMandala6`: value 6); three more, then it. Its move end then
    // runs chapter 78's automatic way on: film 62, Rhea's place.
    for k in 1..=6 {
        e.run_command(format!("(OrdreMandala{k}={k})").as_bytes());
    }
    let mut t = 20.0;
    for v in [2, 4, 5] {
        t += 10.0;
        light(&mut e, v, t);
        assert_eq!(var(&e, "Puzzle_Lucioles_Fini"), 0);
    }
    let z = zone(&e, |z| z.kind == 1 && z.value == 6);
    aim(&mut e, z);
    e.events.clear();
    e.click(t + 10.0);
    e.pass(None);
    assert_eq!(
        (var(&e, "NbLucioleAllumed"), var(&e, "Puzzle_Lucioles_Fini")),
        (6, 1)
    );
    run_until(&mut e, |e| e.place().0 == "rhea.wam");
    assert_eq!(noted(&e.events), [FIREFLIES_CLICK, FIREFLIES_MOVE_END]);
    assert_eq!(e.place(), ("rhea.wam", 1));
    assert_eq!(e.script_state().unwrap().chapter, 80);
    assert_eq!(var(&e, "Puzzle_Lucioles_AutoWin"), 1);
    assert_eq!(films(&e.events), [62]);
    let lines = set_lines(&e.events);
    assert!((41..=45).all(|l| lines.contains(&(78, l))), "{lines:?}");
    lines_as_the_original(&lines);
    assert!(music_reaches(&mut e, 35));
}

/// Firefly `v` of `voiles.wam`'s view 1 clicked, its move played out.
fn light(e: &mut Engine, v: i32, t: f64) {
    let z = zone(e, |z| z.kind == 1 && z.value == v);
    aim(e, z);
    e.events.clear();
    e.click(t);
    run_until(e, |e| !e.turning());
    assert_eq!(noted(&e.events)[0], FIREFLIES_CLICK);
}

/// The vortex (`TOURBILLON`, click kind 3) of `voiles.wam`: before the
/// fireflies are won it sends the game back to Tibet's view 10 (film 62,
/// the fireflies hidden again); after, to Rhea (chapter 80).
#[test]
fn the_vortex_leads_back_or_on() {
    let Some(files) = game() else { return };
    let mut e = in_shamballa(files);
    let vortex = id(&e, defnames::OBJETS, "TOURBILLON");
    aim_sprite(&mut e, false, vortex);
    e.events.clear();
    e.click(10.0);
    run_until(&mut e, |e| e.place().0 == "tibet.wam");
    assert_eq!(e.place(), ("tibet.wam", 10));
    assert_eq!(e.script_state().unwrap().chapter, 75);
    assert_eq!((var(&e, "MONDE"), e.files().current_disc()), (5, 1));
    assert_eq!(films(&e.events), [62]);
    let lines = set_lines(&e.events);
    assert!((19..=32).all(|l| lines.contains(&(78, l))), "{lines:?}");
    lines_as_the_original(&lines);

    // The fireflies won with the last one in view 1 (the mandala of
    // Ireland, `OrdreMandala4`, laid last: value 1): the view entered
    // again, no automatic way on; the vortex then leads to Rhea.
    let mut e = in_shamballa(game().unwrap());
    for (k, rank) in [1, 2, 3, 6, 4, 5].into_iter().enumerate() {
        e.run_command(format!("(OrdreMandala{}={rank})", k + 1).as_bytes());
    }
    for (n, v) in [2, 3, 5, 4, 6, 1].into_iter().enumerate() {
        light(&mut e, v, 20.0 + 10.0 * n as f64);
    }
    assert_eq!(e.place(), ("voiles.wam", 1));
    assert_eq!(
        (
            var(&e, "Puzzle_Lucioles_Fini"),
            var(&e, "Puzzle_Lucioles_AutoWin")
        ),
        (1, 0)
    );
    aim_sprite(&mut e, false, vortex);
    e.events.clear();
    e.click(10.0);
    run_until(&mut e, |e| e.place().0 == "rhea.wam");
    assert_eq!(e.place(), ("rhea.wam", 1));
    assert_eq!(e.script_state().unwrap().chapter, 80);
    assert_eq!((var(&e, "MONDE"), e.files().current_disc()), (4, 4));
    assert_eq!(films(&e.events), [62]);
    let lines = set_lines(&e.events);
    assert!((34..=38).all(|l| lines.contains(&(78, l))), "{lines:?}");
    lines_as_the_original(&lines);
}

/// Rhea (chapter 80), met in view 6: her subjects, each answer once; the
/// shadow's (`OMBRE_SHAMB`, dialog 395) opens the way: the link of view
/// 3 is refused before (`abortdepart`), after it plays film 67 and enters
/// the ring (`annulair.wam` view 1, chapter 81, music 37).
#[test]
fn rhea_opens_the_way_to_the_ring() {
    let Some(files) = game() else { return };
    let mut e = in_shamballa(files);
    goto(&mut e, "rhea.wam", 1, 80);
    let mut t = 10.0;
    for v in 2..=6 {
        t += 10.0;
        walk(&mut e, t, v);
    }
    let mut heard = Vec::new();
    for subject in [
        "SHAMBALA",
        "HEROS_SH",
        "RHEA",
        "CHAMAN",
        "ESPACE_SHAMB",
        "SUPERNOVA_SHAMB",
    ] {
        t += 10.0;
        heard.extend(ask(&mut e, t, "RHEA", subject));
    }
    assert_eq!(heard, [391, 392, 397, 393, 394, 396]);
    assert_eq!(flag(&e, 232), 0);
    for v in [5, 4, 3] {
        t += 10.0;
        walk(&mut e, t, v);
    }
    // The link to the ring: refused.
    let z = zone(&e, |z| z.kind == 2 && z.value == 1);
    aim(&mut e, z);
    e.events.clear();
    e.click(t + 5.0);
    for _ in 0..200 {
        e.tick();
        e.pass(None);
    }
    assert_eq!(e.place(), ("rhea.wam", 3));
    assert!(set_lines(&e.events).contains(&(80, 11)));
    // Back to Rhea for the shadow.
    for v in [4, 5, 6] {
        t += 10.0;
        walk(&mut e, t, v);
    }
    t += 10.0;
    assert_eq!(ask(&mut e, t, "RHEA", "OMBRE_SHAMB"), [395]);
    assert_eq!(flag(&e, 232), 1);
    for v in [5, 4, 3] {
        t += 10.0;
        walk(&mut e, t, v);
    }
    click_zone(
        &mut e,
        t + 5.0,
        |z| z.kind == 2 && z.value == 1,
        |e| e.place().0 == "annulair.wam",
    );
    assert_eq!(e.place(), ("annulair.wam", 1));
    assert_eq!(e.script_state().unwrap().chapter, 81);
    assert_eq!(films(&e.events), [67]);
    assert!(music_reaches(&mut e, 37));
    lines_as_the_original(&set_lines(&e.events));
}

/// The ring (`annulair.wam`, chapter 81): every move zooms with a roll
/// (`ZoomPsychoStart=180,3`): the view rolls instead of turning, the roll
/// is undone and the field of view back to 76° at the end; its characters
/// answer a click (their dialogs); the star (`ETOILE_LA`, taken in view
/// 12) laid on view 5's stone is the way to Atlantis: refused without it
/// (`abortdepart`), with it in hand film 63, `atlantis.wam` view 8,
/// chapter 82, the ring's sixteen objects gone from the bag.
#[test]
fn the_ring_rolls_and_its_star_opens_atlantis() {
    let Some(files) = game() else { return };
    let mut e = in_shamballa(files);
    goto(&mut e, "annulair.wam", 1, 81);
    let z = zone(&e, |z| z.kind == 1 && z.value == 16);
    aim(&mut e, z);
    let aimed = e.camera.angles();
    e.events.clear();
    e.click(10.0);
    e.pass(None);
    assert_eq!(e.zoom.roll_step, atlantis2_engine::turn::ROLL_STEP);
    let mut rolled = 0;
    for _ in 0..200 {
        e.tick();
        e.pass(None);
        if e.zoom.roll != 0.0 {
            rolled += 1;
            // The view rolls about its own axis, the direction kept.
            let (y, p) = e.camera.angles();
            assert!((y - aimed.0).abs() < 1e-3 && (p - aimed.1).abs() < 1e-3);
        }
        if e.place().1 == 16 && !e.turning() && e.loop_type == LoopType::Warp {
            break;
        }
        if e.loop_type == LoopType::Video {
            e.end_video();
        }
    }
    assert!(rolled >= 30, "{rolled}");
    assert_eq!(e.place(), ("annulair.wam", 16));
    assert_eq!((e.zoom.roll, e.zoom.roll_step), (0.0, 0.0));
    assert_eq!(e.camera.projection(), warp_fov(&e));
    // A character of the ring: the dog of view 16 (dialog 406).
    let dog = id(&e, defnames::PERSONNAGES, "DEMON_DOGUE");
    aim_sprite(&mut e, true, dog);
    e.events.clear();
    e.click(20.0);
    let mut evs = Vec::new();
    for _ in 0..30 {
        e.tick();
        e.pass(None);
        evs.append(&mut e.events);
        if e.subject_menu.open {
            break;
        }
    }
    settle(&mut e);
    evs.append(&mut e.events);
    assert_eq!(dialogs(&evs), [406], "{:?}", e.subject_menu.open);
    let mut t = 30.0;
    for v in [15, 14, 13, 12] {
        t += 10.0;
        walk(&mut e, t, v);
    }
    // The star taken.
    let star = id(&e, defnames::OBJETS, "ETOILE_LA");
    aim_sprite(&mut e, false, star);
    e.events.clear();
    e.click(t + 5.0);
    e.pass(None);
    assert!(in_bag(&e, "ETOILE_LA"));
    for v in [11, 10, 9, 8, 7, 6, 5] {
        t += 10.0;
        walk(&mut e, t, v);
    }
    // Without the star in hand: refused.
    let stone = zone(&e, named("ZONEPOSEETOILEPIERRE"));
    aim(&mut e, stone);
    e.events.clear();
    e.click(t + 5.0);
    for _ in 0..200 {
        e.tick();
        e.pass(None);
    }
    assert_eq!(e.place(), ("annulair.wam", 5));
    assert!(set_lines(&e.events).contains(&(81, 43)));
    // With it.
    e.run_command(b"(objetenmain=ETOILE_LA)");
    aim(&mut e, stone);
    e.events.clear();
    e.click(t + 10.0);
    run_until(&mut e, |e| e.place().0 == "atlantis.wam");
    assert_eq!(e.place(), ("atlantis.wam", 8));
    assert_eq!(e.script_state().unwrap().chapter, 82);
    assert_eq!(films(&e.events), [63]);
    assert_eq!((var(&e, "EnergieJoueur"), flag(&e, 249)), (10, 1));
    assert!(!e.bag_entries().iter().any(|b| e
        .script_state()
        .unwrap()
        .names
        .name(defnames::OBJETS, b.id as usize)
        .is_some_and(|n| n.ends_with(b"_LA"))));
    assert_eq!(noted(&e.events), [OCTOPUS_ENTRY]);
    assert!(music_reaches(&mut e, 36));
    lines_as_the_original(&set_lines(&e.events));
}

/// Atlantis (chapter 82): every view's entry is the octopus's (noted, its
/// mode never set, so no tick); the aid's two ends: the hero's death
/// (chapter 201: film 64, back to view 8 and chapter 82), the octopus's
/// (chapter 200: film 65, Tibet's view 7, chapter 75, `flag260`; chapter
/// 75's section then plays dialog 372, counts a trip and sets `flag261`).
#[test]
fn the_octopus_s_two_ends() {
    let Some(files) = game() else { return };
    let mut e = in_shamballa(files);
    goto(&mut e, "atlantis.wam", 8, 82);
    let mut t = 0.0;
    for v in [7, 6, 5, 4] {
        t += 10.0;
        walk(&mut e, t, v);
        assert_eq!(noted(&e.events), [OCTOPUS_ENTRY], "view {v}");
        assert_eq!(e.puzzle_mode(), 0);
        assert_eq!(e.puzzle_tick(), None);
    }
    e.run_command(b"(EnergieJoueur=3)");
    e.events.clear();
    e.end_octopus_fight(false);
    run_until(&mut e, |e| e.place().1 == 8);
    assert_eq!(e.place(), ("atlantis.wam", 8));
    assert_eq!(e.script_state().unwrap().chapter, 82);
    assert_eq!(films(&e.events), [64]);
    assert_eq!(var(&e, "EnergieJoueur"), 10);
    assert_eq!(noted(&e.events), [OCTOPUS_TICK, OCTOPUS_ENTRY]);
    lines_as_the_original(&set_lines(&e.events));
    let deep = var(&e, "SpaceDeep");
    e.events.clear();
    e.end_octopus_fight(true);
    run_until(&mut e, |e| e.place().0 == "tibet.wam");
    assert_eq!(e.place(), ("tibet.wam", 7));
    assert_eq!(e.script_state().unwrap().chapter, 75);
    assert_eq!((var(&e, "MONDE"), e.files().current_disc()), (5, 1));
    assert_eq!(films(&e.events), [65]);
    assert_eq!(dialogs(&e.events), [372]);
    assert_eq!((flag(&e, 260), flag(&e, 261)), (1, 1));
    assert_eq!(var(&e, "SpaceDeep"), deep + 1);
    lines_as_the_original(&set_lines(&e.events));
}

/// The worlds the hub is entered from, as chapters 20, 25, 56 and 75 make
/// the trip and chapter 83 the way back: the world, the place, view and
/// chapter of the ball's zone, the ball, `SourceWorld_SpaceTravel` and the
/// variable of the first trip.
const TRAVELLERS: [(i32, &str, u32, i32, &str, i32, &str); 4] = [
    (
        1,
        "ilepoete.wam",
        70,
        20,
        "BOULE_SHAMAN_IR",
        1,
        "TravelIRDone",
    ),
    (
        2,
        "yucatan.wam",
        22,
        25,
        "BOULE_SHAMAN_YU",
        2,
        "TravelYUDone",
    ),
    (
        3,
        "miniature.wam",
        16,
        56,
        "BOULE_SHAMAN_CH",
        3,
        "TravelCHDone",
    ),
    (5, "tibet.wam", 4, 75, "BOULE_SHAMAN", 5, "TravelTIDone"),
];

/// The ball used on its zone in view `view` (the overlay opened from the
/// bag, the centre on the zone, a click): the trip's section runs.
fn use_ball(e: &mut Engine, t: f64, ball: &str) {
    pick(e, t, ball);
    assert!(e.overlays.ball);
    let view = e.place().1;
    let z = zone(e, |z| z.kind == 1000);
    let (yaw, pitch) = e.zone_aim(view, z).unwrap();
    e.camera.enter(yaw + 0.01, pitch);
    e.pass(None);
    e.pass(None);
    e.events.clear();
    e.click(t + 1.0);
    e.pass(None);
    assert!(!e.overlays.any());
}

/// Each world's ball takes the traveller to the hub (film 200, view 1,
/// chapter 83) with the world and the disc kept; the first trip from a
/// world counts one more `SpaceDeep`, whose value chapter 83's `/INIT`
/// reads to open the hub's moves one by one; view 1's way back (zone 10)
/// plays film 201 and returns to the world's ball view and chapter. The
/// second trip from a world counts nothing.
#[test]
fn the_hub_takes_each_world_s_traveller_and_back() {
    let Some(files) = game() else { return };
    let mut e = in_tibet(files, 2);
    let mut t = 0.0;
    for (k, &(monde, place, view, chapter, ball, source, done)) in TRAVELLERS.iter().enumerate() {
        for trip in 0..2 {
            e.run_command(format!("(monde={monde})").as_bytes());
            goto(&mut e, place, view, chapter);
            let disc = e.files().current_disc();
            let deep = var(&e, "SpaceDeep");
            t += 10.0;
            use_ball(&mut e, t, ball);
            run_until(&mut e, |e| e.place().0.eq_ignore_ascii_case("planetes.wam"));
            assert_eq!(e.place().1, 1, "{place}");
            assert_eq!(e.script_state().unwrap().chapter, 83);
            assert_eq!((var(&e, "MONDE"), e.files().current_disc()), (monde, disc));
            assert_eq!(films(&e.events), [200], "{place}");
            assert_eq!(var(&e, "SourceWorld_SpaceTravel"), source);
            assert_eq!(var(&e, done), 1);
            assert_eq!(var(&e, "SpaceTravel"), 0);
            let want = if trip == 0 { deep + 1 } else { deep };
            assert_eq!(var(&e, "SpaceDeep"), want, "{place} trip {trip}");
            assert_eq!(want, k as i32 + 1);
            assert!(music_reaches(&mut e, 39), "{place}");
            // Chapter 83's `/INIT`: with `SpaceDeep` d (1..4) the move
            // from view d to d + 1 is disabled, those before it enabled
            // (the ones after keep the file's state, enabled, out of
            // reach); with 5 all are enabled.
            for v in 1..=4u32 {
                let next = e
                    .wam()
                    .view(v)
                    .unwrap()
                    .zones
                    .iter()
                    .find(|z| z.value == v as i32 + 1 || z.value == v as i32 + 1 + 88888)
                    .map(|z| z.kind)
                    .unwrap();
                assert_eq!(
                    next < 88888,
                    v != want as u32,
                    "view {v} with SpaceDeep {want}"
                );
            }
            lines_as_the_original(&set_lines(&e.events));
            // Back.
            t += 10.0;
            click_zone(
                &mut e,
                t,
                |z| z.kind == 1 && z.value == 10,
                |e| !e.place().0.eq_ignore_ascii_case("planetes.wam"),
            );
            assert_eq!(e.place(), (place, view));
            assert_eq!(e.script_state().unwrap().chapter, chapter);
            assert_eq!(films(&e.events), [201]);
            assert_eq!(var(&e, "MONDE"), monde);
            lines_as_the_original(&set_lines(&e.events));
        }
    }
    // Ireland's way back to the second island once `flag70` is set.
    e.run_command(b"(monde=1)");
    e.run_command(b"(flag70=1)");
    goto(&mut e, "ilepoete2.wam", 170, 20);
    t += 10.0;
    use_ball(&mut e, t, "BOULE_SHAMAN_IR");
    run_until(&mut e, |e| e.place().0.eq_ignore_ascii_case("planetes.wam"));
    t += 10.0;
    click_zone(
        &mut e,
        t,
        |z| z.kind == 1 && z.value == 10,
        |e| !e.place().0.eq_ignore_ascii_case("planetes.wam"),
    );
    assert_eq!(e.place(), ("ilepoete2.wam", 170));
}

/// The hub's moves, each with its sound (`TrajSound` of the departure
/// sections) and its video; in view 4 the balls become crystals once: the
/// four balls leave the bag, the four crystals enter it, each in its
/// world.
#[test]
fn the_hub_s_moves_and_the_crystals() {
    let Some(files) = game() else { return };
    let mut e = in_tibet(files, 2);
    for c in [
        "(sacados=BOULE_SHAMAN)",
        "(sacados=BOULE_SHAMAN_IR)",
        "(sacados=BOULE_SHAMAN_YU)",
        "(sacados=BOULE_SHAMAN_CH)",
        "(SpaceDeep=4)",
        "(SourceWorld_SpaceTravel=5)",
    ] {
        e.run_command(c.as_bytes());
    }
    goto(&mut e, "Planetes.wam", 1, 83);
    let mut t = 0.0;
    for (to, sound) in [(2, 512), (3, 523), (4, 534), (3, 543), (4, 534)] {
        t += 10.0;
        walk(&mut e, t, to);
        assert!(
            e.events
                .contains(&Event::Sound(format!("sound{sound}.apc"))),
            "to {to}: {:?}",
            e.events
        );
    }
    for ball in [
        "BOULE_SHAMAN",
        "BOULE_SHAMAN_IR",
        "BOULE_SHAMAN_YU",
        "BOULE_SHAMAN_CH",
    ] {
        assert!(!in_bag(&e, ball), "{ball}");
    }
    let world = |e: &Engine, name: &str| {
        let id = id(e, defnames::OBJETS, name);
        e.bag_entries().iter().find(|b| b.id == id).map(|b| b.world)
    };
    assert_eq!(world(&e, "CRISTAL_SHAMAN"), Some(5));
    assert_eq!(world(&e, "CRISTAL_SHAMAN_IR"), Some(1));
    assert_eq!(world(&e, "CRISTAL_SHAMAN_YU"), Some(2));
    assert_eq!(world(&e, "CRISTAL_SHAMAN_CH"), Some(3));
    assert_eq!(flag(&e, 214), 1);
    lines_as_the_original(&set_lines(&e.events));
}

/// In the hub after the octopus's death (`SpaceDeep` 5: every move
/// open), view 5 and its last zone: chapter 83's last section, the world
/// 4 (disc 4), the music stopped, films 68 and 66, the credits; once they
/// end, film 99 and the quit (`EXIT`). A click in the credits quits at once
/// (the menu's key action), without film 99.
#[test]
fn the_end_of_the_game() {
    if game().is_none() {
        return;
    }
    for click_in_the_credits in [false, true] {
        let mut e = in_tibet(game().unwrap(), 2);
        e.run_command(b"(SpaceDeep=5)");
        e.run_command(b"(SourceWorld_SpaceTravel=5)");
        goto(&mut e, "Planetes.wam", 1, 83);
        let mut t = 0.0;
        for v in 2..=5 {
            t += 10.0;
            walk(&mut e, t, v);
        }
        let z = zone(&e, |z| z.kind == 1 && z.value == 77450);
        aim(&mut e, z);
        e.events.clear();
        e.click(t + 5.0);
        let mut evs = Vec::new();
        for _ in 0..400 {
            if e.loop_type == LoopType::Video {
                e.end_video();
            }
            e.tick();
            e.pass(None);
            evs.append(&mut e.events);
            if e.loop_type == LoopType::Menu {
                break;
            }
        }
        assert_eq!(e.loop_type, LoopType::Menu);
        assert_eq!(films(&evs), [68, 66]);
        assert!(evs.contains(&Event::Monde(4)));
        assert_eq!(e.files().current_disc(), 4);
        assert!(!e.quit());
        // The credits page, then its end.
        let c0 = e.clock();
        for n in 0..20_000 {
            e.set_clock(c0 + f64::from(n) / 60.0);
            if click_in_the_credits && n == 600 {
                e.click(c0 + 1000.0);
            }
            e.tick();
            e.pass(None);
            evs.append(&mut e.events);
            if e.loop_type != LoopType::Menu || e.quit() {
                break;
            }
        }
        for _ in 0..50 {
            if e.quit() {
                break;
            }
            if e.loop_type == LoopType::Video {
                e.end_video();
            }
            e.tick();
            e.pass(None);
            evs.append(&mut e.events);
        }
        assert!(e.quit());
        if click_in_the_credits {
            assert_eq!(films(&evs), [68, 66]);
        } else {
            assert_eq!(films(&evs), [68, 66, 99]);
            let lines = set_lines(&evs);
            assert!((142..=148).all(|l| lines.contains(&(83, l))), "{lines:?}");
            lines_as_the_original(&lines);
        }
    }
}

/// The values of `/set(key=...)` lines of the end's chapters (comments
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

/// Every dialog the end's chapters start, and Tibet's after the octopus
/// (372), plays to its end with its heads and bodies drawn.
#[test]
fn every_endgame_dialog_plays() {
    let Some(files) = game() else { return };
    let mut numbers: Vec<u32> = set_values(&files, "dial")
        .iter()
        .filter_map(|v| v.parse().ok())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    numbers.push(372);
    assert_eq!(
        numbers,
        [391, 392, 393, 394, 395, 396, 397, 399, 400, 401, 402, 403, 404, 405, 406, 407, 372]
    );
    let mut e = in_shamballa(files);
    let mut screen = vec![0u16; 640 * 480];
    let mut lines = 0;
    for n in numbers {
        if n == 372 {
            goto(&mut e, "tibet.wam", 7, 75);
            e.run_command(b"(monde=5)");
        }
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
    assert!(lines > 17, "{lines}");
}

/// Shamballa's subjects and objects in its banks (`sujshamb.spr`,
/// `objshamb.spr`), the musics on their discs (35-38 on disc 4, the hub's
/// 39 on every disc), the films on disc 4 (and the hub's 200 and 201 on
/// every disc).
#[test]
fn endgame_subjects_objects_musics_and_films() {
    let Some(files) = game() else { return };
    let e = in_shamballa(files);
    let names = e.script_state().unwrap().names.clone();
    let bank = |name: &str| {
        let data = e
            .files()
            .read(atlantis2_data::Folder::Spr, name)
            .unwrap_or_else(|| panic!("spr\\{name}"));
        atlantis2_data::spr::SprBank::parse(&data).unwrap()
    };
    let subjects = bank("sujshamb.spr");
    let mut n = 0;
    for list in set_values(e.files(), "enablesuj") {
        for subject in list.split(',').skip(1) {
            let frame = names.param(defnames::SUJETS, subject.as_bytes());
            assert!(
                subjects.info(frame as usize).is_some(),
                "{subject}: {frame}"
            );
            n += 1;
        }
    }
    assert_eq!(n, 7);
    // The objects a click takes in Shamballa's and Atlantis's places.
    let objects = bank("objshamb.spr");
    let mut taken = Vec::new();
    let mut e2 = in_shamballa(game().unwrap());
    for place in &PLACES[..4] {
        e2.load_place(place);
        for ent in &e2.wam().entities {
            if ent.kind == 2 && ent.state_records().any(|st| st.click == 1) {
                taken.push(ent.con_name.clone());
            }
        }
    }
    taken.sort();
    assert_eq!(taken.len(), 17, "{taken:?}");
    for object in &taken {
        let frame = names.param(defnames::OBJETS, object.as_bytes());
        assert!(objects.info(frame as usize).is_some(), "{object}: {frame}");
    }
    let mut musics = Vec::new();
    for list in set_values(e.files(), "startmusik") {
        for entry in atlantis2_engine::music::parse_list(list.as_bytes()) {
            if !musics.contains(&entry.music) {
                musics.push(entry.music);
            }
        }
    }
    musics.sort_unstable();
    assert_eq!(musics, [20, 35, 36, 37, 38, 39]);
    for m in [35, 36, 37, 38] {
        let name = atlantis2_engine::music::music_name(m);
        assert!(
            e.files()
                .read_from_disc(4, atlantis2_data::Folder::ApcMusic, &name)
                .is_some(),
            "{name}"
        );
    }
    for d in 1..=4 {
        let name = atlantis2_engine::music::music_name(39);
        assert!(
            e.files()
                .read_from_disc(d, atlantis2_data::Folder::ApcMusic, &name)
                .is_some(),
            "{name} disc {d}"
        );
        for f in [200, 201] {
            assert!(e
                .files()
                .read_from_disc(d, atlantis2_data::Folder::Hnm, &format!("cine{f:03}.hnm"))
                .is_some());
        }
    }
    for f in [61, 62, 63, 64, 65, 66, 67, 68, 99] {
        assert!(
            e.files()
                .read_from_disc(4, atlantis2_data::Folder::Hnm, &format!("cine{f:03}.hnm"))
                .is_some(),
            "film {f}"
        );
    }
}

/// A game saved in `place` (view, chapter, world) and loaded in a new
/// game: the place, the view, the world and its disc, the chapter, the
/// story.
fn save_and_load(place: &str, view: u32, chapter: i32, monde: i32, disc: u8) {
    let mut e = in_shamballa(game().unwrap());
    e.run_command(format!("(monde={monde})").as_bytes());
    e.run_command(b"(SpaceDeep=3)");
    goto(&mut e, place, view, chapter);
    e.run_command(b"(flag232=1)");
    e.run_command(b"(sacados=ETOILE_LA)");
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
    f.load_data(&saved).unwrap();
    settle(&mut f);
    assert!(f.place().0.eq_ignore_ascii_case(place));
    assert_eq!(f.place().1, view);
    assert_eq!(f.script_state().unwrap().chapter, chapter);
    assert_eq!(
        (var(&f, "MONDE"), f.files().current_disc()),
        (monde, disc),
        "{place}"
    );
    assert_eq!((flag(&f, 232), var(&f, "SpaceDeep")), (1, 3));
    assert!(in_bag(&f, "ETOILE_LA"));
}

#[test]
fn saves_in_shamballa_atlantis_and_the_hub_load() {
    if game().is_none() {
        return;
    }
    save_and_load("voiles.wam", 1, 78, 4, 4);
    save_and_load("rhea.wam", 6, 80, 4, 4);
    save_and_load("annulair.wam", 12, 81, 4, 4);
    save_and_load("atlantis.wam", 18, 82, 4, 4);
    // The hub keeps the world of the traveller: China's, disc 3.
    save_and_load("Planetes.wam", 4, 83, 3, 3);
}

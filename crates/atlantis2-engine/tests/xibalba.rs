//! Xibalba (world 6, disc 4): every Xibalba chapter's `/INIT` run; every
//! view of the four places entered with the puzzles' entry hooks; the
//! click hooks (the canoe's view taking the click until it is enabled,
//! then starting it; the star map's run); the moves between the places
//! (WAM links); the canoe played with the mouse to its win, the testing
//! aid that writes the web's wins, and what the scripts do after them;
//! every dialog the chapters and the web
//! start; the subjects, objects, musics and films on disc 4; the place
//! sounds; a save in Xibalba loaded, and one in the web loaded in view
//! 198. The way in from the priestess and the way back to the king's
//! hall, with the mouse, are `tests/flow.rs`'s.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::wam::Zone;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::display::blit_640_to_800;
use atlantis2_engine::engine::{yucatan_click_hooks, yucatan_entry_hooks};
use atlantis2_engine::png::encode_rgb565;
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

/// Xibalba's chapters (the files of `scenar\`).
const CHAPTERS: [i32; 7] = [35, 36, 37, 38, 39, 40, 41];

/// Xibalba's places.
const PLACES: [&str; 4] = ["bat.wam", "xibalba.wam", "riviere.wam", "toile.wam"];

/// A game in Xibalba as the priestess's yes leaves it, without the film:
/// a new game's film ended, Yucatán's musics loaded as chapter 75's trip
/// loads them, then the section's commands (the world, the place, view
/// 403, chapter 35).
fn in_xibalba(files: A2Files) -> Engine {
    in_xibalba_at(files, false)
}

/// [`in_xibalba`] with the screen option at 800 x 600 when `large`.
fn in_xibalba_at(files: A2Files, large: bool) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.set_screen_800(large);
    e.trace = true;
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(delmusik=1,2,3,4,5)"[..],
        b"(LOADMUSIK=10,11,12,13,14,15,16)",
        b"(monde=6)",
        b"(changewam=bat.wam)",
        b"(vue=403)",
        b"(chapter=35)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("bat.wam", 403));
    assert_eq!(e.files().current_disc(), 4);
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

/// The `/set` lines the events report, as (chapter, line).
fn set_lines(events: &[Event]) -> Vec<(i32, usize)> {
    events
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter, line } => Some((*chapter, *line)),
            _ => None,
        })
        .collect()
}

/// Each `/set` line of `lines` is one the original's handler ran
/// (`scripts_worlds_china.txt`, chapters 25-60).
fn lines_as_the_original(lines: &[(i32, usize)]) {
    let golden = include_str!("golden/scripts_worlds_china.txt");
    for (c, l) in lines {
        if (25..=60).contains(c) {
            let head = format!("chapi{c:03}.con:{l} ");
            assert!(golden.lines().any(|g| g.starts_with(&head)), "{head}");
        }
    }
}

#[test]
fn every_xibalba_chapter_runs_its_init() {
    let Some(files) = game() else { return };
    let mut e = in_xibalba(files);
    // The world's banks and texts (`Interp_SetMonde(6)`): disc 4; chapter
    // 35's `/INIT` (`flag81`: "has been to Xibalba") and its first
    // arrival's dialog 171.
    assert_eq!(var(&e, "MONDE"), 6);
    assert_eq!((flag(&e, 81), flag(&e, 102)), (1, 1));
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
    lines_as_the_original(&set_lines(&e.events));
}

#[test]
fn every_xibalba_view_is_entered_with_its_hooks() {
    let Some(files) = game() else { return };
    let mut e = bare(files, "bat.wam", 403);
    let mut views = Vec::new();
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
        let mut n = 0;
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
            n += 1;
            missing_spw.extend(e.events.iter().filter_map(|x| match x {
                Event::SpwLoad { name, found: false } => Some(name.clone()),
                _ => None,
            }));
            let h = noted(&e.events);
            assert_eq!(
                h,
                yucatan_entry_hooks(name.as_bytes(), id, 0),
                "{place} {id}"
            );
            // Only the star map's entry runs.
            assert!(e.events.iter().all(|x| match x {
                Event::PuzzleRan { function } => *function == 0x0049_4e00,
                Event::PuzzleNotPorted { function } => *function != 0x0049_4e00,
                _ => true,
            }));
            hooks.extend(h.into_iter().map(|f| (place, id, f)));
            assert_eq!(e.puzzle_mode(), 0, "{place} {id}");
        }
        views.push((place, n));
    }
    // Every view record has its panorama on a disc (the records that
    // stand for a link's other end, such as `riviere.wam`'s 404, 117, 124
    // and 198, are entered here too) and every place animation.
    assert_eq!(
        views,
        [
            ("bat.wam", 7),
            ("xibalba.wam", 26),
            ("riviere.wam", 17),
            ("toile.wam", 69)
        ]
    );
    assert!(missing_warp.is_empty(), "{missing_warp:?}");
    assert!(missing_spw.is_empty(), "{missing_spw:?}");
    // The hooks: the star map's entry (view 121), run, and the web's
    // (view 198, also `riviere.wam`'s record of it, a `YU` view 198),
    // noted.
    assert_eq!(
        hooks,
        [
            ("xibalba.wam", 121, 0x0049_4e00),
            ("riviere.wam", 198, 0x0048_f530),
            ("toile.wam", 198, 0x0048_f530),
        ]
    );
}

/// The values of `/set(key=...)` lines of Xibalba's chapters (comments
/// left out), each once, in order.
fn set_values(files: &A2Files, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chapter in CHAPTERS {
        let data = files
            .read(Folder::Scenar, &format!("chapi{chapter:03}.con"))
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

/// The click hooks on Xibalba's zones, with the scripts: the canoe's
/// zone of view 402 takes the click (`CLICZONE` left at 0) until
/// `enable_pz_pirog` is 1; then the canoe starts (loop type 9) and the
/// click goes on (FUN_0049c010's branch returns 0); a star map's hole
/// (view 121, type 50) runs the map's click (the star laid), and the
/// click scan follows.
#[test]
fn the_xibalba_click_hooks() {
    // The dispatcher's lists, for Xibalba's views.
    assert_eq!(
        yucatan_click_hooks(b"yu0402.hnm", 402, 4, 0, false),
        (Vec::new(), true)
    );
    assert_eq!(
        yucatan_click_hooks(b"yu0402.hnm", 402, 4, 0, true),
        (vec![0x0049_c010], false)
    );
    assert_eq!(
        yucatan_click_hooks(b"yu0121.hnm", 121, 0x32, 0, false),
        (vec![0x0049_4e80], false)
    );
    assert_eq!(
        yucatan_click_hooks(b"yu0230.hnm", 230, 1, 0, false),
        (Vec::new(), false)
    );

    let Some(files) = game() else { return };
    let mut e = in_xibalba(files);
    goto(&mut e, "bat.wam", 402, 35);
    let z = zone(&e, |z| z.kind == 4 && z.value == 469);
    aim(&mut e, z);
    e.events.clear();
    e.click(10.0);
    e.pass(None);
    assert!(noted(&e.events).is_empty());
    // Consumed: `CLICZONE` left at 0 for the frame's click scan.
    assert!(e
        .events
        .iter()
        .any(|x| matches!(x, Event::ZoneClick { cliczone: 0, .. })));
    assert!(!e
        .events
        .iter()
        .any(|x| matches!(x, Event::ClickScan(v) if v.cliczone != 0)));
    settle(&mut e);
    e.run_command(b"(enable_pz_pirog=1)");
    aim(&mut e, z);
    e.events.clear();
    e.click(20.0);
    e.pass(None);
    // The start (FUN_0049c010(1) → `pz_pirogue_init`).
    assert_eq!(noted(&e.events), [0x0049_6a60]);
    assert!(e
        .events
        .iter()
        .any(|x| matches!(x, Event::ZoneClick { cliczone: 469, .. })));
    assert!(e.events.iter().any(|x| matches!(x, Event::ClickScan(_))));
    assert_eq!((e.loop_type, flag(&e, 103)), (LoopType::Canoe, 1));
    e.escape();
    e.pass(None);
    settle(&mut e);

    goto(&mut e, "xibalba.wam", 121, 39);
    let hole = zone(&e, |z| z.kind == 50 && z.value == 0);
    e.run_command(b"(sacados=ETOILE102)");
    e.run_command(b"(objetenmain=ETOILE102)");
    aim(&mut e, hole);
    e.events.clear();
    e.click(30.0);
    e.pass(None);
    // The click, then its win test (the map not won).
    assert_eq!(noted(&e.events), [0x0049_4e80, 0x0049_4f60]);
    assert!(e.events.iter().any(|x| matches!(x, Event::ClickScan(_))));
    // Laid: the star out of the hand and the bag, on hole 0.
    assert!(!in_bag(&e, "ETOILE102"));
    assert_eq!(
        var(&e, "etat_etoile0"),
        id(&e, defnames::OBJETS, "ETOILE102")
    );
    assert_eq!(var(&e, "OBJETENMAIN"), 0);
}

/// Moves along each link between Xibalba's places, without the scripts
/// (a type-2 zone; the link's view is a destination of the view left, so
/// the change waits for the move's end). The transition video is the
/// destination old → new of the new place's file (@ 0x0045a518): only
/// the web's records have one (`riviere.wam`'s 198 → 314, `toile.wam`'s
/// 314 → 198).
#[test]
fn the_places_of_xibalba_are_linked() {
    let Some(files) = game() else { return };
    let mut e = bare(files, "bat.wam", 404);
    let trips = [
        (("bat.wam", 404), ("riviere.wam", 300)),
        (("riviere.wam", 300), ("bat.wam", 404)),
        (("riviere.wam", 311), ("xibalba.wam", 117)),
        (("xibalba.wam", 117), ("riviere.wam", 311)),
        (("riviere.wam", 301), ("xibalba.wam", 124)),
        (("xibalba.wam", 124), ("riviere.wam", 301)),
        (("riviere.wam", 314), ("toile.wam", 198)),
        (("toile.wam", 198), ("riviere.wam", 314)),
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
            to != "toile.wam" && from != "toile.wam",
            "{from} {v} -> {to} {w}"
        );
        assert!(e.events.contains(&Event::LinkMove {
            wam: to.to_string(),
            view: w,
            deferred: true
        }));
    }
}

/// The canoe played with the mouse in loop type 9, one pass a frame
/// (`pass`, with the screen written as `pz-canoe-<name>.png` in
/// `ATLANTIS2_SHOTS` when set): each tile taken to its own square (a left
/// click on it, one on its square: the two swap) and turned upright (right
/// clicks), the stage clicked, then the canoe left to paddle along the
/// river to its end. The passes it took.
fn play_canoe(
    e: &mut Engine,
    t: &mut f64,
    pass: &mut dyn FnMut(&mut Engine, Option<&str>),
) -> usize {
    let centre = |e: &Engine, k: usize| {
        let s = e.canoe().unwrap().board[k];
        (s.x + 0x10, s.y + 0x36)
    };
    let mut passes = 0;
    let mut frame = |e: &mut Engine, k: usize, left: bool, right: bool, shot: Option<&str>| {
        let (x, y) = centre(e, k);
        e.pointer(x, y);
        if left {
            *t += 1.0;
            e.click(*t);
        }
        if right {
            e.right_click();
        }
        pass(e, shot);
        passes += 1;
    };
    assert_eq!(e.loop_type, LoopType::Canoe);
    frame(e, 4, false, false, Some("1-shuffled"));
    for k in 0..8 {
        let board = e.canoe().unwrap().board;
        let s = (0..8).find(|&s| board[s].tile / 4 == k as i32).unwrap();
        if s != k {
            frame(e, s, true, false, None);
            assert_ne!(e.canoe().unwrap().held, -1);
            frame(e, k, true, false, None);
            assert_eq!(e.canoe().unwrap().held, -1);
        }
        while e.canoe().unwrap().board[k].tile % 4 != 0 {
            frame(e, k, false, true, None);
        }
    }
    frame(e, 8, false, false, Some("2-solved"));
    frame(e, 8, true, false, None);
    assert!(e.canoe().unwrap().moving);
    let mut n = 0;
    while e.loop_type == LoopType::Canoe {
        n += 1;
        assert!(n < 5000, "the canoe never arrives");
        let shot = (n == 600).then_some("3-paddling");
        frame(e, 8, false, false, shot);
    }
    passes
}

/// The canoe in view 402 (chapter 35), started by its zone and played
/// with the mouse: the start (`(flag103=1)`, the loop type 9, the pointer
/// pair), the play, the win's writes (`finpirogue`,
/// `Pirogue_JustFinished` and the scan) and the quit back to the
/// panorama: the first win's section sends the game to `xibalba.wam`
/// view 124, chapter 39 (`flag117`); a second win, from the bat's island
/// again, to view 404. Escape quits a canoe started again, nothing
/// written.
#[test]
fn the_canoe_is_played_to_its_win() {
    let Some(files) = game() else { return };
    let shots = std::env::var_os("ATLANTIS2_SHOTS").map(PathBuf::from);
    if let Some(d) = &shots {
        std::fs::create_dir_all(d).unwrap();
    }
    let mut screen = vec![0u16; 640 * 480];
    let mut pass = |e: &mut Engine, shot: Option<&str>| {
        e.pass(Some(&mut screen));
        if let (Some(d), Some(name)) = (&shots, shot) {
            let png = encode_rgb565(&screen, 640, 480);
            std::fs::write(d.join(format!("pz-canoe-{name}.png")), png).unwrap();
        }
    };
    let mut e = in_xibalba(files);
    goto(&mut e, "bat.wam", 402, 35);
    e.run_command(b"(enable_pz_pirog=1)");
    let z = zone(&e, |z| z.kind == 4 && z.value == 469);
    let mut t = 10.0;
    let start = |e: &mut Engine, t: &mut f64| {
        aim(e, z);
        e.events.clear();
        *t += 1.0;
        e.click(*t);
        e.pass(None);
        assert!(e.events.contains(&Event::PuzzleRan {
            function: 0x0049_6a60
        }));
        assert_eq!(e.loop_type, LoopType::Canoe);
        assert_eq!(flag(e, 103), 1);
        assert_eq!(e.pointer, (320, 240));
    };
    start(&mut e, &mut t);
    let passes = play_canoe(&mut e, &mut t, &mut pass);
    assert!(passes > 1000, "{passes}");
    assert!(e.events.contains(&Event::PuzzleRan {
        function: 0x0049_a570
    }));
    assert!(e.canoe().is_none());
    pass(&mut e, Some("4-won"));
    settle(&mut e);
    assert_eq!(e.place(), ("xibalba.wam", 124));
    assert_eq!(e.script_state().unwrap().chapter, 39);
    assert_eq!(
        (
            flag(&e, 103),
            var(&e, "finpirogue"),
            var(&e, "Pirogue_JustFinished"),
            flag(&e, 117),
            var(&e, "showbarque")
        ),
        (1, 1, 0, 1, 3)
    );
    let lines = set_lines(&e.events);
    assert!((103..=111).all(|l| lines.contains(&(35, l))), "{lines:?}");
    lines_as_the_original(&lines);
    // Again: chapter 35's second-win section, view 404.
    goto(&mut e, "bat.wam", 402, 35);
    start(&mut e, &mut t);
    e.events.clear();
    play_canoe(&mut e, &mut t, &mut |e, _| e.pass(None));
    settle(&mut e);
    assert_eq!(e.place(), ("bat.wam", 404));
    assert_eq!(var(&e, "Pirogue_JustFinished"), 0);
    let lines = set_lines(&e.events);
    assert!(lines.contains(&(35, 116)) && lines.contains(&(35, 117)));
    // Escape: the quit at once, nothing written, back to the panorama.
    goto(&mut e, "bat.wam", 402, 35);
    start(&mut e, &mut t);
    e.events.clear();
    e.escape();
    e.pass(None);
    assert_eq!(e.loop_type, LoopType::Warp);
    assert!(e.canoe().is_none());
    assert!(e.events.contains(&Event::PuzzleRan {
        function: 0x0049_a570
    }));
    assert!(set_lines(&e.events).is_empty());
    assert_eq!(e.place(), ("bat.wam", 402));
}

/// At 800 x 600 the canoe is drawn on the 640 x 480 surface, enlarged
/// (its init's FUN_00402960(0)), the pointer at that surface's centre:
/// the frames are those of a 640 x 480 game enlarged.
#[test]
fn the_canoe_at_800_by_600() {
    let Some(files) = game() else { return };
    let mut small = in_xibalba(files);
    let mut big = in_xibalba_at(game().unwrap(), true);
    let (mut a, mut b, mut want) = (
        vec![0u16; 640 * 480],
        vec![0u16; 800 * 600],
        vec![0u16; 800 * 600],
    );
    assert_eq!(big.screen_size(), (800, 600));
    for e in [&mut small, &mut big] {
        goto(e, "bat.wam", 402, 35);
        e.run_command(b"(enable_pz_pirog=1)");
        e.start_canoe();
        assert_eq!((e.loop_type, e.pointer), (LoopType::Canoe, (320, 240)));
    }
    for i in 0..60 {
        for e in [&mut small, &mut big] {
            e.pointer(150 + 5 * i, 100 + 3 * i);
            if i % 15 == 7 {
                e.right_click();
            }
        }
        small.pass(Some(&mut a));
        big.pass(Some(&mut b));
        blit_640_to_800(&a, &mut want, 800);
        let d: Vec<usize> = (0..800 * 600).filter(|&k| b[k] != want[k]).collect();
        assert!(
            d.is_empty(),
            "frame {i}: {} pixels, first {:?}, last {:?}",
            d.len(),
            d.first().map(|k| (k % 800, k / 800)),
            d.last().map(|k| (k % 800, k / 800))
        );
    }
}

/// The web's testing aid (`toile.wam` view 198, chapter 38): each round
/// by its stage, what the original writes along a won round: the
/// caterpillar (sound 104, `finaraign1`), the grasshopper (sound 105,
/// `finaraign2`), the big spider (`flag92`, the spider goddess a subject
/// of the bat and the priestess, film 48, the skull, the sword given and
/// taken back, `finaraign3`), then the grasshopper again (stage 4, no
/// variable) while it is not in the bag, and nothing when it is.
#[test]
fn the_web_aid_writes_each_round() {
    let Some(files) = game() else { return };
    let mut e = in_xibalba(files);
    goto(&mut e, "toile.wam", 198, 38);
    assert_eq!(e.puzzle_mode(), 0);
    assert_eq!(e.web_stage(), 1);
    e.events.clear();
    assert_eq!(e.win_web_round(), 1);
    assert!(in_bag(&e, "CHENILLE"));
    assert_eq!(var(&e, "finaraign1"), 1);
    assert!(e.events.contains(&Event::Sound("sound104.apc".into())));
    assert_eq!(e.web_stage(), 2);
    e.events.clear();
    assert_eq!(e.win_web_round(), 2);
    assert!(in_bag(&e, "SAUTERELLE"));
    assert_eq!(var(&e, "finaraign2"), 1);
    assert!(e.events.contains(&Event::Sound("sound105.apc".into())));
    assert_eq!(e.web_stage(), 3);
    e.run_command(b"(sacados=NOT_SAUTERELLE)");
    e.events.clear();
    assert_eq!(e.win_web_round(), 3);
    assert!(e.events.iter().any(
        |x| matches!(x, Event::VideoStart { name, frames: Some(_) } if name.eq_ignore_ascii_case("cine048.hnm"))
    ));
    settle(&mut e);
    assert!(in_bag(&e, "CRANE_ARAIG2"));
    assert!(!in_bag(&e, "EPEE"));
    assert_eq!((var(&e, "finaraign3"), flag(&e, 92)), (1, 1));
    let s = e.script_state().unwrap();
    let goddess = s.names.param(defnames::SUJETS, b"DEESSE_ARAIGNEE");
    for who in ["CHAUVE_SOURIS", "PRETRESSE"] {
        let c = id(&e, defnames::PERSONNAGES, who);
        assert!(
            s.subjects.list(c).iter().any(|x| x.value == goddess),
            "{who}"
        );
    }
    assert_eq!(e.web_stage(), 4);
    e.events.clear();
    assert_eq!(e.win_web_round(), 4);
    assert!(in_bag(&e, "SAUTERELLE"));
    assert!(e.events.contains(&Event::Sound("sound105.apc".into())));
    let bag = e.bag_entries().len();
    e.events.clear();
    assert_eq!(e.win_web_round(), 4);
    assert_eq!(e.bag_entries().len(), bag);
    assert!(!e.events.contains(&Event::Sound("sound105.apc".into())));
    assert_eq!(e.web_stage(), 4);
    assert_eq!(e.puzzle_mode(), 0);
}

/// Every dialog Xibalba's chapters start (`dial=N`) and the web's code
/// (187-189, by RunOneCommand) plays to its end with its heads and bodies
/// drawn.
#[test]
fn every_xibalba_dialog_plays() {
    let Some(files) = game() else { return };
    let mut numbers: Vec<u32> = set_values(&files, "dial")
        .iter()
        .filter_map(|v| v.parse().ok())
        .collect();
    numbers.extend([187, 188, 189]);
    numbers.sort_unstable();
    numbers.dedup();
    let want: Vec<u32> = (171..=202).chain([204, 208]).collect();
    assert_eq!(numbers, want);
    let mut e = in_xibalba(files);
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
        let n_lines = e
            .events
            .iter()
            .filter(|x| matches!(x, Event::DialogLine(_)))
            .count();
        assert!(n_lines > 0, "dialog {n}: no line");
        lines += n_lines;
        settle(&mut e);
    }
    assert!(lines > 50, "{lines}");
}

/// The subjects, objects, musics and films Xibalba uses are on disc 4 (or
/// installed): every subject the chapters enable has its picture in the
/// world's subjects bank; every object the chapters, the web's code and a
/// click give has its picture in the objects bank; the musics of the
/// chapters' lists and the films they and the web play open.
#[test]
fn xibalba_subjects_objects_musics_and_films() {
    let Some(files) = game() else { return };
    let e = in_xibalba(files);
    let names = e.script_state().unwrap().names.clone();
    let bank = |name: &str| {
        let (data, _) = e
            .files()
            .read_with_disc(Folder::Spr, name)
            .unwrap_or_else(|| panic!("spr\\{name}"));
        atlantis2_data::spr::SprBank::parse(&data).unwrap()
    };
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
    assert_eq!(n, 26);
    // The web's subjects (by RunOneCommand) too.
    let frame = names.param(defnames::SUJETS, b"DEESSE_ARAIGNEE");
    assert!(subjects.info(frame as usize).is_some());
    let objects = bank("objyuc.spr");
    let mut taken: Vec<String> = set_values(e.files(), "sacados")
        .into_iter()
        .filter(|v| !v.to_ascii_uppercase().starts_with("NOT_"))
        .map(|v| v.to_ascii_uppercase())
        .collect();
    taken.extend(["CHENILLE", "SAUTERELLE", "CRANE_ARAIG2", "EPEE", "PLUME"].map(String::from));
    let mut e2 = bare(game().unwrap(), "bat.wam", 403);
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
            "CHENILLE",
            "CHULEL",
            "CRANE_ARAIG2",
            "EPEE",
            "ETOILE102",
            "ETOILE103",
            "ETOILE105",
            "ETOILE108",
            "ETOILE111",
            "ETOILE113",
            "ETOILE115",
            "OBSIDIENNE1",
            "PIED2_CROCO",
            "PLUME",
            "SAUTERELLE"
        ]
    );
    for object in &taken {
        let frame = names.param(defnames::OBJETS, object.as_bytes());
        assert!(
            objects.info(frame as usize).is_some(),
            "{object}: frame {frame} of objyuc.spr"
        );
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
    assert_eq!(musics, [13, 14, 16]);
    for m in &musics {
        let name = atlantis2_engine::music::music_name(*m);
        assert!(
            e.files()
                .read_from_disc(4, Folder::ApcMusic, &name)
                .is_some(),
            "{name}"
        );
    }
    // (Chapter 35's music, started with its dialog 171, is checked by
    // `tests/flow.rs`, where the dialog plays to its end.)
    // The films: chapter 27's ways in (43, 44), the chapters' (45, 47,
    // 49-52), the web's (46, 48); all on disc 4.
    let mut films: Vec<u32> = set_values(e.files(), "cinema")
        .iter()
        .filter_map(|v| v.parse().ok())
        .collect();
    films.extend([43, 44, 46, 48]);
    films.sort_unstable();
    assert_eq!(films, (43..=52).collect::<Vec<_>>());
    for f in films {
        let name = format!("cine{f:03}.hnm");
        assert!(
            e.files().read_from_disc(4, Folder::Hnm, &name).is_some(),
            "{name}"
        );
    }
}

/// The place sounds of `xibalba.wam` and `riviere.wam` (the Xibalba
/// places of the executable's listener table that have sources) start in
/// a view that hears them.
#[test]
fn xibalba_place_sounds_play() {
    let Some(files) = game() else { return };
    for (place, view) in [("xibalba.wam", 101), ("riviere.wam", 300)] {
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
        assert!(started, "{place}");
    }
    let e = bare(files, "bat.wam", 403);
    assert!(e.place.sources.is_empty());
}

/// A new game whose film ended, then `saved` loaded.
fn loaded(saved: &atlantis2_data::save::SaveData) -> Engine {
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
    f.trace = true;
    f.load_data(saved).unwrap();
    settle(&mut f);
    f
}

/// A game saved in the statues' clearing (chapter 39, the star map done,
/// the soul in the bag) and loaded in a new game: the place, the view,
/// the world on disc 4, the chapter (whose `/INIT` runs again at a load:
/// the statue's zones enabled) and the story.
#[test]
fn a_save_in_xibalba_loads() {
    let Some(files) = game() else { return };
    let mut e = in_xibalba(files);
    goto(&mut e, "xibalba.wam", 122, 39);
    for c in [
        &b"(etoiles_fini=1)"[..],
        b"(finpirogue=1)",
        b"(sacados=CHULEL)",
        b"(flag83=1)",
    ] {
        e.run_command(c);
    }
    let saved = e.save_data();
    let f = loaded(&saved);
    assert_eq!(f.place(), ("xibalba.wam", 122));
    assert_eq!(f.script_state().unwrap().chapter, 39);
    assert_eq!(f.files().current_disc(), 4);
    assert_eq!(var(&f, "MONDE"), 6);
    assert_eq!((var(&f, "etoiles_fini"), flag(&f, 83)), (1, 1));
    assert!(in_bag(&f, "CHULEL"));
    let z = f
        .wam()
        .view(122)
        .unwrap()
        .zones
        .iter()
        .position(|z| z.name.as_deref() == Some("CLICTEZSTATUE"))
        .unwrap();
    assert!(f.wam().view(122).unwrap().zones[z].kind < 88888);
    assert!(f.music_playing().is_some_and(|m| [14, 16].contains(&m)));
}

/// A game saved in the web (view 230) loads in view 198, as the
/// original's load does (FUN_0048f450, the rest of it the web's, noted),
/// whose entry is noted too.
#[test]
fn a_save_in_the_web_loads_in_view_198() {
    let Some(files) = game() else { return };
    let mut e = in_xibalba(files);
    goto(&mut e, "toile.wam", 230, 38);
    let saved = e.save_data();
    let f = loaded(&saved);
    assert_eq!(f.place(), ("toile.wam", 198));
    assert_eq!(f.script_state().unwrap().chapter, 38);
    assert_eq!(f.files().current_disc(), 4);
    assert_eq!(noted(&f.events), [0x0048_f450, 0x0048_f530]);
    assert_eq!(f.puzzle_mode(), 0);
}

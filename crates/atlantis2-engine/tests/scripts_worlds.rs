//! The place animations' script keys on synthetic slots, as atlantis2-re
//! `tools/emu/scripts_worlds.py anim` lays them: a fatal error of the
//! scripts (the original's reporter FUN_00480cd0).
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::wam::EntityState;
use atlantis2_data::A2Files;
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

/// A new game's first view, the scripts running, a view with a slot.
fn started(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for _ in 0..30 {
        e.tick();
        e.pass(None);
    }
    assert_eq!(e.loop_type, LoopType::Warp);
    e.enter_view(7, 0.0, 0.0).unwrap();
    assert!(!e.anim.slots.is_empty());
    e
}

/// A slot of the harness (`ANIM_SLOTS`).
struct Row {
    kind: i32,
    name: &'static str,
    anim: u32,
    frame: i32,
    end: i32,
    count: i32,
    active: bool,
    rec20: i32,
    rec24: i32,
    playing: bool,
}

const fn row(kind: i32, name: &'static str, anim: u32, frame: i32, end: i32, count: i32) -> Row {
    Row {
        kind,
        name,
        anim,
        frame,
        end,
        count,
        active: true,
        rec20: 0,
        rec24: 0,
        playing: true,
    }
}

fn anim_slots() -> Vec<Row> {
    vec![
        row(1, "CHAC", 1, 5, 20, 20),
        Row {
            active: false,
            rec24: 31,
            ..row(1, "CHAC", 2, 0, 0, 40)
        },
        row(1, "GARDIEN1", 7, 3, 39, 40),
        Row {
            rec20: 10,
            rec24: 61,
            ..row(2, "CHEVAL", 1, 40, 60, 80)
        },
        row(2, "CROCO", 1, 0, 10, 10),
        Row {
            active: false,
            ..row(2, "CROCO", 2, 0, 0, 25)
        },
        Row {
            playing: false,
            ..row(1, "DEMON_CHEVAL", 1, 50, 70, 71)
        },
        row(2, "FUMEE_ENCENS2", 1, 7, 12, 12),
        row(1, "OGRE", 3, 2, 9, 9),
    ]
}

fn id(e: &Engine, kind: i32, name: &str) -> i32 {
    let section = if kind == 1 {
        defnames::PERSONNAGES
    } else {
        defnames::OBJETS
    };
    e.script_state()
        .unwrap()
        .names
        .id(section, name.as_bytes())
        .unwrap() as i32
}

/// The harness's `set_slots`: the slots replaced by `rows` (ids given or
/// looked up), nothing else of a slot set (start 0, no hand-over, no
/// chain, the record's other words 0).
fn set_slots(e: &mut Engine, rows: &[(Row, Option<i32>)]) {
    let template = e.anim.slots[0].clone();
    let slots = rows
        .iter()
        .map(|(r, given)| {
            let mut s = template.clone();
            s.kind = r.kind;
            s.id = given.unwrap_or_else(|| id(e, r.kind, r.name));
            s.frame = r.frame;
            s.end = r.end;
            s.count = r.count;
            s.start = 0;
            s.click = 0;
            s.handing = false;
            s.chain = -1;
            s.active = r.active;
            s.playing = r.playing;
            s.delay = 0;
            s.blink = 0;
            s.record = None;
            s.next = None;
            s.state = EntityState {
                anim: r.anim,
                first: r.rec20,
                end: r.rec24,
                ..EntityState::default()
            };
            s
        })
        .collect();
    e.anim.slots = slots;
}

/// L9: a fatal error of the scripts runs the original's reporter
/// (FUN_00480cd0): with a view current, the emergency save of player 9,
/// slot 8 (`save\ATL09_08.SAV`, @ 0x00480dd3..0x00480ddc), then the quit.
/// `GOTOFRAMEOBJET` past the file's count is one (the key decides, not the
/// slot's kind: @ 0x004831fd..0x0048320e), before the frame is set;
/// `GOTOFRAMEPERSO` has no such test.
#[test]
fn a_fatal_error_saves_and_quits() {
    let Some(files) = game() else { return };
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("a2-fatal-save");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("save")).unwrap();
    let mut e = started(files);
    e.set_data_dir(Some(dir.clone()));
    let rows: Vec<(Row, Option<i32>)> = anim_slots().into_iter().map(|r| (r, None)).collect();
    set_slots(&mut e, &rows);
    // A character's slot: no test.
    e.run_command(b"(GOTOFRAMEPERSO=GARDIEN1,99)");
    assert_eq!(e.anim.slots[2].frame, 99);
    assert!(!e.quit());
    assert!(e.script_state().unwrap().fatal.is_none());
    // An object's: fatal.
    e.trace = true;
    e.events.clear();
    e.run_command(b"(GOTOFRAMEOBJET=FUMEE_ENCENS2,99)");
    assert_eq!(e.anim.slots[7].frame, 7);
    assert!(e.script_state().unwrap().fatal.is_some());
    assert!(e.quit());
    assert!(e.events.iter().any(|x| matches!(x, Event::Fatal(_))));
    assert_eq!(e.player_number, 9);
    assert!(dir.join("save").join("ATL09_08.SAV").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

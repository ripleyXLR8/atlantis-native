//! The `/set` keys of the place animations and the script keys the other
//! worlds use, against the original's `SetVariables` under emulation
//! (`tests/golden/scripts_worlds_anim.txt`, `scripts_worlds_misc.txt`,
//! atlantis2-re `tools/emu/scripts_worlds.py anim` / `misc`).
//!
//! - anim: nine synthetic slots as the harness lays them, then each line
//!   run alone: the slots' frame, end, count, active, hand-over, chain,
//!   playing byte and state record's first frame and end after it, and
//!   the fatal errors; two slots whose ids clash (an object's id equal to
//!   a character's: the kind is not checked); the `NUMFRAMEOBJET` /
//!   `NUMFRAMEPERSO` conditions and the variables they leave.
//! - misc: the variables and calls of the handler-level keys.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::wam::EntityState;
use atlantis2_data::A2Files;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const ANIM: &str = include_str!("golden/scripts_worlds_anim.txt");
const MISC: &str = include_str!("golden/scripts_worlds_misc.txt");

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

/// The harness's `slot_state`.
fn slot_state(e: &Engine) -> String {
    e.anim
        .slots
        .iter()
        .enumerate()
        .map(|(n, s)| {
            format!(
                "s{n}:f{}/e{}/c{} a{} h{} ch{} p{} r20={} r24={}",
                s.frame,
                s.end,
                s.count,
                u8::from(s.active),
                u8::from(s.handing),
                s.chain,
                u8::from(s.playing),
                s.state.first,
                s.state.end
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs one golden `/set` line alone (the harness clears the dialog and
/// the waiting section first) and returns its fatal error, if any; the
/// scripts go on afterwards as the harness's do.
fn run(e: &mut Engine, line: &str) -> Option<String> {
    let command = line.strip_prefix("/set").unwrap();
    e.run_command(command.as_bytes());
    let s = e.script_state_mut().unwrap();
    s.resume_pending = false;
    s.fatal.take()
}

/// The golden row of a `/set` line: its `errors=` part (`-` none).
fn golden_error(row: &str) -> String {
    row.split(" | ")
        .find_map(|f| f.strip_prefix("errors="))
        .map_or("-".to_string(), |e| e.to_string())
}

#[test]
fn the_animation_keys_match_the_original() {
    let Some(files) = game() else { return };
    let mut e = started(files);
    let rows: Vec<(Row, Option<i32>)> = anim_slots().into_iter().map(|r| (r, None)).collect();
    set_slots(&mut e, &rows);
    let lines: Vec<&str> = ANIM.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(lines[0], format!("start {}", slot_state(&e)));
    let mut i = 1;
    let mut n = 0;
    while n < 26 {
        let set = lines[i].split(" | ").next().unwrap();
        let got = run(&mut e, set);
        let want = golden_error(lines[i]);
        let got = got.map_or("-".to_string(), |m| format!("fatal:{m}"));
        // The golden gives the format string, Native the formatted text.
        let fmt = |s: &str| s.split('!').next().unwrap().to_string();
        assert_eq!(fmt(&got), fmt(&want), "{set}");
        if set == "/set(GOTOFRAMEOBJET=FUMEE_ENCENS2,99)" {
            // The harness's fatal reporter returns and the line goes on
            // to set the frame (@ 0x00483216); the game's quits there, as
            // Native's stops: the harness's state is taken over.
            e.anim.slots[7].frame = 99;
        }
        assert_eq!(format!("  {}", slot_state(&e)), lines[i + 1], "{set}");
        i += 2;
        n += 1;
    }
    assert_eq!(n, 26);
    // The id clash.
    assert!(
        lines[i].starts_with("/set(GOTOFRAMEPERSO=CHAC,4)"),
        "{}",
        lines[i]
    );
    let chac = id(&e, 1, "CHAC");
    set_slots(
        &mut e,
        &[
            (row(2, "", 1, 1, 5, 5), Some(chac)),
            (row(1, "CHAC", 1, 2, 5, 5), None),
        ],
    );
    while i < lines.len() && lines[i].starts_with("/set(") {
        let set = lines[i].split(" | ").next().unwrap();
        assert_eq!(run(&mut e, set), None, "{set}");
        assert_eq!(format!("  {}", slot_state(&e)), lines[i + 1], "{set}");
        i += 2;
        n += 1;
    }
    assert_eq!(n, 28);
    // The conditions, on the first table again.
    set_slots(&mut e, &rows);
    for l in &lines[i..] {
        let Some(r) = l.strip_prefix("cond '") else {
            continue;
        };
        let (cond, rest) = r.split_once("' -> ").unwrap();
        let t = e.eval_condition(cond.as_bytes()).unwrap();
        let s = e.script_state().unwrap();
        let got = format!(
            "{} numframeobjet_var={} numframeperso_var={}",
            u8::from(t),
            s.var("NUMFRAMEOBJET").unwrap(),
            s.var("NUMFRAMEPERSO").unwrap()
        );
        assert_eq!(got, rest, "{cond}");
        n += 1;
    }
    assert_eq!(n, 36);
}

/// The handler-level keys of `scripts_worlds_misc.txt` whose effect is a
/// variable: `LIMITEANGLEWARPX` (twice: 999 is capped to −1), `exit`,
/// `showbarque`, `MemoNumDial`, `EnergieJoueur` and the two `++` keys.
#[test]
fn the_handler_keys_match_the_original() {
    let Some(files) = game() else { return };
    let mut e = started(files);
    let names = e.script_state().unwrap().names.clone();
    let mut n = 0;
    for l in MISC.lines() {
        let Some(set) = l.strip_prefix("/set(") else {
            continue;
        };
        let f: Vec<&str> = l.split(" | ").collect();
        let changes = f.iter().find_map(|x| x.strip_prefix("changes=")).unwrap();
        // The variables' changes only (`var[i]=v`), and the quit.
        let vars: Vec<(usize, i32)> = changes
            .split(' ')
            .filter_map(|c| c.strip_prefix("var["))
            .map(|c| {
                let (i, v) = c.split_once("]=").unwrap();
                (i.parse().unwrap(), v.parse().unwrap())
            })
            .collect();
        let key = set.split(['=', '+', ',', ')']).next().unwrap();
        let skip = [
            "DisableZoneClicNWNZ",
            "EnableZoneClicNWNZ",
            "DisableZoneClicNWNameZ",
            "EnableZoneClicNWNameZ",
            "ZoomStart",
            "ZoomPsychoStart",
            "NextVue",
            "credits",
            "dial",
            "viewbigspr",
        ];
        if skip.iter().any(|k| k.eq_ignore_ascii_case(key)) || vars.is_empty() {
            continue;
        }
        let before = e.script_state().unwrap().vars.clone();
        let command = &l[4..l.find(" | ").unwrap()];
        e.run_command(command.as_bytes());
        let s = e.script_state().unwrap();
        for (i, v) in &vars {
            assert_eq!(
                s.vars[*i],
                *v,
                "{command}: var {i} ({:?})",
                names.name(defnames::VARIABLES, *i)
            );
        }
        // Nothing else changed (a value already there is no change).
        let changed: Vec<usize> = (0..before.len())
            .filter(|&i| s.vars[i] != before[i])
            .collect();
        assert!(
            changed.iter().all(|i| vars.iter().any(|v| v.0 == *i)),
            "{command}: {changed:?}"
        );
        if changes.contains("quit=1") {
            assert!(s.quit, "{command}");
        }
        n += 1;
    }
    assert_eq!(n, 8);
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

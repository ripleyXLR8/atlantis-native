//! The five wheels and the bridge (China, `miniature.wam` views 12-13,
//! chapter 55) against the original (`tests/golden/wheels.txt`, its header
//! there): the view entry hook (rows B), the click hook alone (C, through
//! [`Engine::wheels_click`]) and the check of the two orders, the
//! dispatcher's choice of the hooks (D), the zone click and the scans of
//! chapter 55 as the game runs them (E), and the wheels' sprites drawn
//! (F: the faces' md5s and sampled pixels).
//!
//! Then the puzzle played as a player plays it, with the mouse: from
//! chapter 48 with the five discs in the bag (the figurines' gifts) to the
//! bridge lowered and chapter 56 across it; and the wheels drawn again
//! after a load. With `ATLANTIS2_SHOTS` set to a folder, the played test
//! writes `pz-wheels-*.png` there.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::spw::Spw;
use atlantis2_data::wam::{EntityState, Zone};
use atlantis2_data::{A2Files, Folder};

use atlantis2_engine::anim::Anim;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::engine::{DISC_WHEEL, WHEEL_DISC};
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{CursorBank, Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/wheels.txt");

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// The golden's rows starting with `prefix`, in order.
fn rows(prefix: &str) -> Vec<&'static str> {
    GOLDEN.lines().filter(|l| l.starts_with(prefix)).collect()
}

fn lst<T: ToString>(xs: impl IntoIterator<Item = T>) -> String {
    let v: Vec<String> = xs.into_iter().map(|x| x.to_string()).collect();
    format!("[{}]", v.join(","))
}

/// Films and dialogs ended until the panorama is back; the passes run.
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

/// A game in China's `miniature.wam`, view `view`, chapter `chapter`, as
/// the scripts put it there (the world, the place, the view, the chapter).
fn in_miniature(files: A2Files, view: u32, chapter: i32) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        "(monde=3)".to_string(),
        "(changewam=miniature.wam)".into(),
        format!("(vue={view})"),
        format!("(chapter={chapter})"),
    ] {
        e.run_command(c.as_bytes());
    }
    settle(&mut e);
    assert_eq!(e.place(), ("miniature.wam", view));
    assert_eq!(e.script_state().unwrap().chapter, chapter);
    assert_eq!(e.files().current_disc(), 3);
    e.trace = true;
    e
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

fn id(e: &Engine, name: &str) -> i32 {
    e.script_state()
        .unwrap()
        .names
        .id(defnames::OBJETS, name.as_bytes())
        .unwrap() as i32
}

/// The golden's fresh state: no hand, an empty bag and show / hide table,
/// `findisque1` / `findisque2` 0, `pos_roue1..5` = `pos`, `MONDE` =
/// `monde`; view 13 entered again (its slots loaded, the entry hook run).
fn fresh(e: &mut Engine, pos: [i32; 5], monde: i32) {
    settle(e);
    e.run_command(b"(objetenmain=0)");
    let s = e.script_state_mut().unwrap();
    s.bag.clear();
    s.shown.clear();
    s.warnings.clear();
    for (k, v) in [("findisque1", 0), ("findisque2", 0), ("MONDE", monde)] {
        assert!(s.set_var(k, v));
    }
    for (k, v) in pos.iter().enumerate() {
        assert!(s.set_var(&format!("pos_roue{}", k + 1), *v));
    }
    e.anim.table.0.clear();
    e.enter_at(13, 0.0, 0.0).unwrap();
    e.cursor.shown = None;
    e.events.clear();
}

/// The first animation record of object `name`: active/frame/playing.
fn slot(e: &Engine, name: &str) -> String {
    let i = id(e, name);
    let s = e.anim.slots.iter().find(|s| s.id == i).expect(name);
    format!(
        "{}/{}/{}",
        i32::from(s.active),
        s.frame,
        i32::from(s.playing)
    )
}

/// The golden's state (its `## state` header).
fn st(e: &Engine) -> String {
    let s = e.script_state().unwrap();
    let cursor = match (e.cursor.shown, e.cursor.bank) {
        (None, _) => "(0,-1)".to_string(),
        (Some(f), CursorBank::Objects) => format!("(OBJ,{f})"),
        (Some(f), _) => format!("(?,{f})"),
    };
    let pair = format!(
        "({},{})",
        s.cursor_sprite,
        if s.cursor_objects { "OBJ" } else { "0" }
    );
    format!(
        "pos {} fin {}/{} wheels {} bridge {} hand {} pair {pair} cursor {cursor} bag {} table {}",
        lst((1..=5).map(|k| var(e, &format!("pos_roue{k}")))),
        var(e, "findisque1"),
        var(e, "findisque2"),
        lst((1..=5).map(|k| slot(e, &format!("RoueK{k}")))),
        lst(["Pont_Leve", "Pont_Moitie", "Pont_Baisse"].map(|n| slot(e, n))),
        var(e, "OBJETENMAIN"),
        lst(s
            .bag
            .iter()
            .map(|b| format!("{}/{}/{}", b.id, b.sprite, b.world))),
        lst(e
            .anim
            .table
            .0
            .iter()
            .map(|(i, v, k)| format!("{i}/{v}/{k}"))),
    )
}

/// What the original's stubs record for the effects the port has, read
/// from what the engine did: a film's start (`AtlMuzik_Volume(p)` with p
/// the music's fade target after it, in % of the music volume: a music
/// plays here, so the cinema's 20 % is there to read; `AtlSound_CinemaApc`
/// for the sound track the engine started
/// with the film, `AtlHnm_start` for the film; `Input_Refresh()`, which
/// has no counterpart in the port, where the stubs record it), the bag's
/// layout and scroll after an object is added (FUN_004754f0's
/// `Inventaire_Layout`, `Inventaire_Step`).
fn calls(e: &Engine, evs: &[Event], added: bool) -> String {
    let mut out = Vec::new();
    for (i, x) in evs.iter().enumerate() {
        if let Event::VideoStart {
            name,
            frames: Some(_),
        } = x
        {
            assert!(e.music.max != 0 && e.music.state >= 3, "no music to duck");
            out.push(format!(
                "AtlMuzik_Volume({})",
                e.music.target * 100 / e.music.max
            ));
            out.push("Input_Refresh()".into());
            // The track starts with the film (FUN_0047a650 in its start).
            let track = evs[i + 1..]
                .iter()
                .take_while(|x| !matches!(x, Event::VideoStart { .. }))
                .find_map(|x| match x {
                    Event::Sound(n) => Some(n.clone()),
                    _ => None,
                });
            if let Some(apc) = track {
                out.push(format!("AtlSound_CinemaApc('{apc}')"));
            }
            out.push("Input_Refresh()".into());
            out.push(format!("AtlHnm_start('{}')", name.to_ascii_lowercase()));
        }
    }
    // FUN_00475590's bag entry comes after the check's film.
    if added {
        out.push("Inventaire_Layout()".to_string());
        out.push("Inventaire_Step()".to_string());
    }
    if out.is_empty() {
        "-".into()
    } else {
        lst(out)
    }
}

/// The zone of view 13 as the golden's harness builds it: type `t`, value
/// 1, name `name` (`None`: the 8-byte form, no name).
fn zone(t: i32, name: Option<&str>) -> Zone {
    Zone {
        kind: t,
        value: 1,
        name: name.map(str::to_string),
        points: Vec::new(),
    }
}

fn hold(e: &mut Engine, name: Option<&str>) {
    let n = name.unwrap_or("0");
    e.run_command(format!("(objetenmain={n})").as_bytes());
}

fn give(e: &mut Engine, name: &str) {
    e.run_command(format!("(sacados={name})").as_bytes());
}

fn warns(e: &mut Engine) -> String {
    let s = e.script_state_mut().unwrap();
    lst(s.warnings.drain(..).map(|w| w.replace(' ', "_")))
}

/// One C row: the hook on a zone of type `t` named `name`; `check` is
/// FUN_0048d310 run (noted [`Event::PuzzleRan`]).
fn click(e: &mut Engine, name: Option<&str>, t: i32) -> String {
    e.events.clear();
    let before: Vec<i32> = e.bag_entries().iter().map(|b| b.id).collect();
    e.wheels_click(&zone(t, name));
    let evs = std::mem::take(&mut e.events);
    let check = evs
        .iter()
        .filter(|x| {
            matches!(
                x,
                Event::PuzzleRan {
                    function: 0x0048_d310
                }
            )
        })
        .count();
    let added = e.bag_entries().iter().any(|b| !before.contains(&b.id));
    format!(
        "{} | check {check} | calls {} | warn {}",
        st(e),
        calls(e, &evs, added),
        warns(e)
    )
}

#[test]
fn the_entry_and_click_hooks_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_miniature(files, 13, 55);
    let names = e.script_state().unwrap().names.clone();
    let oid = |n: &str| names.id(defnames::OBJETS, n.as_bytes()).unwrap();
    let objs = [
        "RoueK1",
        "RoueK2",
        "RoueK3",
        "RoueK4",
        "RoueK5",
        "DISQUE1",
        "DISQUE2",
        "DISQUE3",
        "DISQUE4",
        "DISQUE5",
        "Pont_Leve",
        "Pont_Moitie",
        "Pont_Baisse",
        "PIECE1",
    ];
    assert_eq!(
        rows("ids objects")[0],
        format!("ids objects {}", objs.map(|n| oid(n).to_string()).join(" "))
    );
    let param = |n: &str| names.param(defnames::OBJETS, n.as_bytes()).to_string();
    assert_eq!(
        rows("params")[0],
        format!(
            "params {}",
            ["DISQUE1", "DISQUE2", "DISQUE3", "DISQUE4", "DISQUE5", "PIECE1"]
                .map(param)
                .join(" ")
        )
    );
    assert_eq!(
        rows("tables")[0],
        format!(
            "tables 0x005e0f7c {} 0x005e0f68 {}",
            lst(WHEEL_DISC),
            lst(DISC_WHEEL)
        )
    );

    // B: the entry hook (run by the entry of view 13; "times 2" runs it
    // once more).
    let mut got = Vec::new();
    for row in rows("B pos ") {
        let w: Vec<&str> = row.split(' ').collect();
        let pos: Vec<i32> = w[2]
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|x| x.parse().unwrap())
            .collect();
        let pos: [i32; 5] = pos.try_into().unwrap();
        fresh(&mut e, pos, 3);
        if w[3] == "frames" {
            // Playing set and frames moved before the hook runs again.
            for k in 1..=5 {
                let i = oid(&format!("RoueK{k}")) as i32;
                let s = e.anim.slots.iter_mut().find(|s| s.id == i).unwrap();
                s.playing = true;
                s.frame = 3;
            }
            e.wheels_entry();
            got.push(format!(
                "B pos {} frames 3 before -> {} | ids {} | warn {}",
                lst(pos),
                st(&e),
                lst(e.wheel_slots),
                warns(&mut e)
            ));
            continue;
        }
        let times: usize = w[4].parse().unwrap();
        for _ in 1..times {
            e.wheels_entry();
        }
        got.push(format!(
            "B pos {} times {times} -> {} | ids {} | warn {}",
            lst(pos),
            st(&e),
            lst(e.wheel_slots),
            warns(&mut e)
        ));
    }
    // The golden's slots are view 13's in the WAM's order, as the port
    // loads them: the wheels' records are 12..16 in both.
    assert_eq!(got, rows("B pos "));
    // No `RoueK3` record: FUN_0047de60's fatal (the original then quits;
    // the port stops the hook and the scripts).
    fresh(&mut e, [1, 2, 3, 4, 5], 3);
    let k3 = oid("RoueK3") as i32;
    e.anim.slots.retain(|s| s.id != k3);
    e.wheels_entry();
    let fatal = e.script_state().unwrap().fatal.clone().unwrap();
    assert_eq!(
        fatal,
        format!("AtlSpw_find_index(): vue 13 ! AtlSpw_find_index({k3}) introuvable")
    );
    assert!(rows("B no ROUEK3")[0]
        .ends_with("warn [fatal:AtlSpw_find_index():_vue_%d_!_AtlSpw_find_index(%d)_introuvable]"));
    e.script_state_mut().unwrap().fatal = None;

    // C: the click hook alone, row by row as the golden's harness runs it.
    let golden = rows("C ");
    let mut got = Vec::new();
    let c = |e: &mut Engine, label: &str, name: &str, got: &mut Vec<String>| {
        let r = click(e, Some(name), 3);
        got.push(format!("C {label} {name} -> {r}"));
    };
    fresh(&mut e, [0; 5], 3);
    c(&mut e, "empty", "AXE_5ROUES3", &mut got);
    for k in 1..=5 {
        give(&mut e, &format!("DISQUE{k}"));
    }
    hold(&mut e, Some("DISQUE1"));
    got.push(format!("C given -> {}", st(&e)));
    c(&mut e, "DISQUE1", "AXE_5ROUES1", &mut got);
    hold(&mut e, Some("DISQUE2"));
    c(&mut e, "DISQUE2", "AXE_5ROUES1", &mut got);
    c(&mut e, "DISQUE1 back in hand", "AXE_5ROUES2", &mut got);
    hold(&mut e, None);
    c(&mut e, "empty", "AXE_5ROUES2", &mut got);
    give(&mut e, "PIECE1");
    hold(&mut e, Some("PIECE1"));
    c(&mut e, "PIECE1", "AXE_5ROUES1", &mut got);
    hold(&mut e, Some("PIECE1"));
    c(&mut e, "PIECE1", "AXE_5ROUES4", &mut got);
    for (k, name) in [
        (2, "AXE_5ROUES2"),
        (1, "AXE_5ROUES1"),
        (3, "AXE_5ROUES3"),
        (4, "AXE_5ROUES4"),
        (5, "AXE_5ROUES5"),
    ] {
        hold(&mut e, Some(&format!("DISQUE{k}")));
        c(&mut e, &format!("DISQUE{k}"), name, &mut got);
    }
    // The first order started film 24: ended, the rows go on.
    assert_eq!(e.loop_type, LoopType::Video);
    e.end_video();
    hold(&mut e, None);
    c(&mut e, "empty", "AXE_5ROUES1", &mut got);
    c(&mut e, "DISQUE1 in hand", "AXE_5ROUES5", &mut got);
    c(&mut e, "DISQUE5 in hand", "AXE_5ROUES2", &mut got);
    c(&mut e, "DISQUE2 in hand", "AXE_5ROUES3", &mut got);
    c(&mut e, "DISQUE3 in hand", "AXE_5ROUES1", &mut got);
    assert_eq!(e.loop_type, LoopType::Video);
    e.end_video();
    hold(&mut e, None);
    c(&mut e, "after the win, empty", "AXE_5ROUES1", &mut got);
    hold(&mut e, Some("PIECE1"));
    c(&mut e, "after the win, PIECE1", "AXE_5ROUES1", &mut got);
    for name in [
        Some("axe_5roues3"),
        Some("Axe_5Roues3"),
        Some("AXE_5ROUES"),
        Some("AXE_5ROUES9"),
        Some("AXE_5ROUESX"),
        Some("AXE_5ROUES12"),
        Some("AXE_5ROUE3"),
        Some(""),
        None,
        Some("XAXE_5ROUES3"),
    ] {
        fresh(&mut e, [0; 5], 3);
        give(&mut e, "DISQUE4");
        hold(&mut e, Some("DISQUE4"));
        let label = name.map_or("none".to_string(), |n| format!("'{n}'"));
        let r = click(&mut e, name, 3);
        got.push(format!("C names DISQUE4 {label} -> {r}"));
    }
    for t in [4, 0x32, 1, 0] {
        fresh(&mut e, [0; 5], 3);
        give(&mut e, "DISQUE4");
        hold(&mut e, Some("DISQUE4"));
        let r = click(&mut e, Some("AXE_5ROUES2"), t);
        got.push(format!("C type {t} DISQUE4 AXE_5ROUES2 -> {r}"));
    }
    let set = |e: &mut Engine, k: &str, v: i32| {
        assert!(e.script_state_mut().unwrap().set_var(k, v));
    };
    fresh(&mut e, [0, 3, 1, 4, 2], 3);
    give(&mut e, "DISQUE1");
    hold(&mut e, Some("DISQUE1"));
    c(&mut e, "no first: DISQUE1", "AXE_5ROUES5", &mut got);
    fresh(&mut e, [1, 2, 3, 4, 0], 3);
    set(&mut e, "findisque1", 1);
    give(&mut e, "DISQUE5");
    hold(&mut e, Some("DISQUE5"));
    c(&mut e, "findisque1=1: DISQUE5", "AXE_5ROUES5", &mut got);
    fresh(&mut e, [5, 3, 1, 4, 0], 3);
    set(&mut e, "findisque1", 1);
    set(&mut e, "findisque2", 1);
    give(&mut e, "DISQUE5");
    hold(&mut e, Some("DISQUE5"));
    c(&mut e, "findisque2=1: DISQUE5", "AXE_5ROUES2", &mut got);
    fresh(&mut e, [1, 2, 3, 4, 0], 3);
    set(&mut e, "findisque2", 2);
    give(&mut e, "DISQUE5");
    hold(&mut e, Some("DISQUE5"));
    c(&mut e, "findisque2=2: DISQUE5", "AXE_5ROUES5", &mut got);
    settle(&mut e);
    fresh(&mut e, [3, 3, 0, 0, 0], 3);
    c(&mut e, "two on 3, empty", "AXE_5ROUES3", &mut got);
    fresh(&mut e, [2, 0, 0, 0, 0], 3);
    hold(&mut e, Some("DISQUE1"));
    c(
        &mut e,
        "DISQUE1 laid on 2 and held",
        "AXE_5ROUES4",
        &mut got,
    );
    c(&mut e, "again empty", "AXE_5ROUES4", &mut got);
    fresh(&mut e, [0; 5], 1);
    hold(&mut e, Some("DISQUE3"));
    c(
        &mut e,
        "MONDE 1 DISQUE3 not in the bag",
        "AXE_5ROUES3",
        &mut got,
    );
    c(&mut e, "MONDE 1 DISQUE3 in hand", "AXE_5ROUES3", &mut got);
    hold(&mut e, None);
    c(&mut e, "MONDE 1 empty", "AXE_5ROUES3", &mut got);

    // The check alone.
    for row in rows("C check pos") {
        let w: Vec<&str> = row.split(' ').collect();
        let pos: Vec<i32> = w[3]
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|x| x.parse().unwrap())
            .collect();
        let (f1, f2) = w[5].split_once('/').unwrap();
        settle(&mut e);
        fresh(&mut e, pos.clone().try_into().unwrap(), 3);
        set(&mut e, "findisque1", f1.parse().unwrap());
        set(&mut e, "findisque2", f2.parse().unwrap());
        // The golden's slots are those of the load, before the entry.
        reload_without_entry(&mut e);
        e.events.clear();
        e.wheels_check();
        let evs = std::mem::take(&mut e.events);
        got.push(format!(
            "C check pos {} fin {f1}/{f2} -> {} | calls {} | warn {}",
            lst(&pos),
            st(&e),
            calls(&e, &evs, false),
            warns(&mut e)
        ));
    }
    let want = golden;
    assert_eq!(got, want);
}

/// View 13's slots loaded again without the wheels' entry: the golden's
/// check rows run on the slots as AtlSpw_load leaves them.
fn reload_without_entry(e: &mut Engine) {
    for k in 1..=5 {
        let i = id(e, &format!("RoueK{k}"));
        let s = e.anim.slots.iter_mut().find(|s| s.id == i).unwrap();
        s.active = k == 5;
        s.frame = 0;
        s.playing = true;
    }
}

#[test]
fn the_dispatcher_chooses_the_wheels_as_the_original() {
    // FUN_0049b910 for `SS` 13: the wheels' click (which runs the check
    // when a disc is laid) for zone types 3, 4 and 0x32 only; the chest's
    // click alone in views 1-7; nothing for other prefixes or views. The
    // engine's dispatcher (its Chinese part) runs on each row's warp
    // name, number and zone type, `DISQUE4` held, on a zone named
    // `AXE_5ROUES2` (value 2), the puzzle mode set as the row gives it:
    // the hooks it reaches (their events), AL, the mode after and
    // `pos_roue4` are the engine's (mode 5: the chest's early-out, which
    // clears the mode and returns before any hook). The chest's click,
    // reached in view 3, finds no chest among view 13's slots: the
    // original's fatal (`find_idx_pzcoffre`), taken here so that the next
    // rows run as the golden's.
    let Some(files) = game() else { return };
    let mut e = in_miniature(files, 13, 55);
    let mut got = Vec::new();
    let mut want = Vec::new();
    for row in rows("D ") {
        let w: Vec<&str> = row.split(' ').collect();
        let (name, number, kind, mode): (&str, u32, i32, i32) = (
            w[1],
            w[2].parse().unwrap(),
            w[3].parse().unwrap(),
            w[5].parse().unwrap(),
        );
        fresh(&mut e, [0; 5], 3);
        give(&mut e, "DISQUE4");
        hold(&mut e, Some("DISQUE4"));
        e.set_puzzle_mode(mode);
        e.events.clear();
        let z = Zone {
            kind,
            value: 2,
            name: Some("AXE_5ROUES2".into()),
            points: Vec::new(),
        };
        let al = e.chinese_click(0, name.as_bytes(), number, &z);
        let fatal = e.script_state_mut().unwrap().fatal.take();
        if number == 3 {
            assert!(fatal.is_some_and(|f| f.contains("find_idx_pzcoffre")));
        } else {
            assert_eq!(fatal, None, "{row}");
        }
        let ent: Vec<String> = e
            .events
            .iter()
            .filter_map(|x| match x {
                Event::PuzzleRan { function } | Event::PuzzleNotPorted { function } => {
                    Some(format!("{function:08x}"))
                }
                _ => None,
            })
            .collect();
        let ent = if ent.is_empty() {
            "-".to_string()
        } else {
            ent.join(",")
        };
        got.push(format!(
            "D {name} {number} {kind} mode {mode} -> {ent} AL {} mode {} pos4 {}",
            i32::from(al),
            e.puzzle_mode(),
            var(&e, "pos_roue4")
        ));
        want.push(row.to_string());
    }
    assert_eq!(got, want);
}

/// One E row: the centre on the zone of view `view` of type `kind` and
/// value `value` (an axle's place when `name` is given), a click, the
/// passes until the click is done (a film the hook starts left running; a
/// move made to its arrival, its video ended).
fn game_click(
    e: &mut Engine,
    t: &mut f64,
    kind: i32,
    value: i32,
    name: Option<&str>,
) -> Vec<Event> {
    let view = e.place().1;
    let zi = e
        .wam()
        .view(view)
        .unwrap()
        .zones
        .iter()
        .position(|z| match name {
            Some(n) => z.name.as_deref() == Some(n),
            None => z.kind == kind && z.value == value,
        })
        .unwrap_or_else(|| panic!("no zone {kind} {value} {name:?} in view {view}"));
    let z = &e.wam().view(view).unwrap().zones[zi];
    assert_eq!((z.kind, z.value), (kind, value));
    let (yaw, pitch) = e.zone_aim(view, zi).unwrap();
    e.camera.enter(yaw, pitch);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zi));
    e.events.clear();
    *t += 10.0;
    e.click(*t);
    e.pass(None);
    let mut evs = std::mem::take(&mut e.events);
    if kind == 1 {
        for _ in 0..600 {
            if e.loop_type == LoopType::Video {
                e.end_video();
            }
            e.tick();
            e.pass(None);
            evs.extend(std::mem::take(&mut e.events));
            if evs.iter().any(|x| matches!(x, Event::ArrivalScan(_))) {
                break;
            }
        }
    }
    evs
}

#[test]
fn the_zone_clicks_and_chapter_55_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_miniature(files, 12, 55);
    // The golden's state: variables at 0, the five discs in the bag, an
    // empty hand and show / hide table.
    e.run_command(b"(objetenmain=0)");
    {
        let s = e.script_state_mut().unwrap();
        s.bag.clear();
        s.shown.clear();
        for k in 1..=5 {
            assert!(s.set_var(&format!("pos_roue{k}"), 0));
        }
        for k in ["findisque1", "findisque2", "ClicZone"] {
            assert!(s.set_var(k, 0));
        }
    }
    e.anim.table.0.clear();
    for k in 1..=5 {
        give(&mut e, &format!("DISQUE{k}"));
    }
    e.enter_at(12, 0.0, 0.0).unwrap();
    let mut t = 100.0;
    let mut got = Vec::new();
    let golden: Vec<&str> = rows("E ");
    // The axle's zones are valued by their names' `#ZONES_CLIC#` index
    // (the golden's `ids zones` row).
    let names = e.script_state().unwrap().names.clone();
    let axe: Vec<i32> = (1..=5)
        .map(|k| {
            names
                .id(defnames::ZONES_CLIC, format!("AXE_5ROUES{k}").as_bytes())
                .unwrap() as i32
        })
        .collect();
    assert_eq!(
        rows("ids zones")[0],
        format!(
            "ids zones {}",
            axe.iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        )
    );
    let a = |k: usize| axe[k - 1];
    let script: [ERow; 13] = [
        ("move to 14", 1, 14, None, None),
        ("move to 13", 1, 13, None, None),
        ("DISQUE1", 3, a(1), Some("AXE_5ROUES1"), Some("DISQUE1")),
        ("DISQUE2", 3, a(2), Some("AXE_5ROUES2"), Some("DISQUE2")),
        ("DISQUE3", 3, a(3), Some("AXE_5ROUES3"), Some("DISQUE3")),
        ("DISQUE4", 3, a(4), Some("AXE_5ROUES4"), Some("DISQUE4")),
        ("DISQUE5", 3, a(5), Some("AXE_5ROUES5"), Some("DISQUE5")),
        ("empty", 3, a(1), Some("AXE_5ROUES1"), Some("0")),
        ("DISQUE1 in hand", 3, a(5), Some("AXE_5ROUES5"), None),
        ("DISQUE5 in hand", 3, a(2), Some("AXE_5ROUES2"), None),
        ("DISQUE2 in hand", 3, a(3), Some("AXE_5ROUES3"), None),
        ("DISQUE3 in hand", 3, a(1), Some("AXE_5ROUES1"), None),
        ("move to 12", 1, 12, None, None),
    ];
    let mut rows_done = 0;
    for (label, kind, value, name, hand) in
        script
            .into_iter()
            .chain([("move to 14", 1, 14, None, None)])
    {
        let view = e.place().1;
        if let Some(h) = hand {
            e.run_command(format!("(objetenmain={h})").as_bytes());
        }
        let chapter = e.script_state().unwrap().chapter;
        let before: Vec<i32> = e.bag_entries().iter().map(|b| b.id).collect();
        let evs = game_click(&mut e, &mut t, kind, value, name);
        let hook = evs
            .iter()
            .position(|x| matches!(x, Event::PuzzleHook { .. }))
            .unwrap_or(evs.len());
        let scan = evs.iter().position(|x| matches!(x, Event::ClickScan(_)));
        let out = lines(&evs[..hook], 55);
        let in_lines = scan.map_or(Vec::new(), |n| lines(&evs[n..], 55));
        let cz = evs
            .iter()
            .find_map(|x| match x {
                Event::ZoneClick { cliczone, .. } => Some(*cliczone),
                _ => None,
            })
            .unwrap_or(0);
        let moved = evs.iter().find_map(|x| match x {
            Event::Enter { view } => Some(*view),
            _ => None,
        });
        let abort = i32::from(kind == 1 && moved.is_none());
        let added = e.bag_entries().iter().any(|b| !before.contains(&b.id));
        let mut c = calls(&e, &evs, added);
        if let Some(v) = moved {
            assert_eq!(v, value as u32);
            let mut list = Vec::new();
            if e.script_state().unwrap().chapter != chapter {
                list.push(format!(
                    "LoadCurrentChap({})",
                    e.script_state().unwrap().chapter
                ));
                list.push("RunInit()".into());
            }
            list.push(format!("Move_Start({v})"));
            c = lst(list);
        }
        // The state after the move: the golden's slots stay view 13's;
        // the port's are the arrival view's, which (views 12 and 13) has
        // the wheels and the bridge as well, placed by the same entry.
        // Across the bridge (view 14) only the rest is compared.
        let state = if e.place().1 == 14 {
            // Chapter 56's `/INIT` ran (the golden stubs `RunInit`): its
            // three arrows (lines 7-9) follow the bridge's rows in the
            // show / hide table.
            let arrows: Vec<(i32, i32, i32)> = [
                ("FLECHE_RENARDE", 0),
                ("FLECHE_CIEL", 1),
                ("FLECHE_CHASSEUR", 0),
            ]
            .iter()
            .map(|&(n, v)| (id(&e, n), v, 2))
            .collect();
            let t = &mut e.anim.table.0;
            assert!(t.len() > 3 && t[3..] == arrows[..], "{t:?}");
            t.truncate(3);
            st_without_slots(&e)
        } else {
            st(&e)
        };
        // The cursor pair as the golden leaves it is the hook's (none here);
        // the passes after it choose the cursor again.
        let state = blank_cursor(&state);
        got.push(format!(
            "E {label} view {view} zone {kind} {value} {} -> out {} | hook [] | abort {abort} | cliczone {cz} | gere [] | click scan {} | in {} - | calls {c} | {state}",
            name.unwrap_or("-"),
            lst(out),
            i32::from(scan.is_some()),
            lst(in_lines),
        ));
        rows_done += 1;
        // A film the hook started, ended before the next click.
        if e.loop_type == LoopType::Video {
            settle(&mut e);
        }
        e.script_state_mut().unwrap().set_var("ClicZone", 0);
    }
    let mut want: Vec<String> = golden[..rows_done]
        .iter()
        .map(|r| blank_cursor(r))
        .collect();
    let last = want.last_mut().unwrap();
    let (head, tail) = last.split_once(" | pos ").unwrap();
    *last = format!("{head} | {}", without_slots(&format!("pos {tail}")));
    // The golden's last row (the move to 11 from view 12) is left out: the
    // port is across the bridge by then, in chapter 56.
    assert_eq!(golden.len(), rows_done + 1);
    assert_eq!(got, want);
}

/// An E row: its label, the zone's type and value, the axle's zone name,
/// the object put in hand before the click.
type ERow<'a> = (&'a str, i32, i32, Option<&'a str>, Option<&'a str>);

/// A state with its cursor pair blanked.
fn blank_cursor(s: &str) -> String {
    let (a, b) = s.split_once(" cursor (").unwrap();
    format!("{a} cursor * bag{}", b.split_once(") bag").unwrap().1)
}

/// The state without the wheels' and the bridge's records.
fn without_slots(s: &str) -> String {
    let (a, b) = s.split_once(" wheels ").unwrap();
    format!(
        "{a} {}",
        b.split_once(" hand ")
            .map(|(_, r)| format!("hand {r}"))
            .unwrap()
    )
}

fn st_without_slots(e: &Engine) -> String {
    let s = e.script_state().unwrap();
    let cursor = match (e.cursor.shown, e.cursor.bank) {
        (None, _) => "(0,-1)".to_string(),
        (Some(f), CursorBank::Objects) => format!("(OBJ,{f})"),
        (Some(f), _) => format!("(?,{f})"),
    };
    format!(
        "pos {} fin {}/{} hand {} pair ({},{}) cursor {cursor} bag {} table {}",
        lst((1..=5).map(|k| var(e, &format!("pos_roue{k}")))),
        var(e, "findisque1"),
        var(e, "findisque2"),
        var(e, "OBJETENMAIN"),
        s.cursor_sprite,
        if s.cursor_objects { "OBJ" } else { "0" },
        lst(s
            .bag
            .iter()
            .map(|b| format!("{}/{}/{}", b.id, b.sprite, b.world))),
        lst(e
            .anim
            .table
            .0
            .iter()
            .map(|(i, v, k)| format!("{i}/{v}/{k}"))),
    )
}

/// The engine at 59.94 passes a second ([`PASS`]), a 15 Hz tick every fourth, every
/// pass drawn; the mouse as a player moves it.
struct Run {
    e: Engine,
    pass: u64,
    gap: f64,
    screen: Vec<u16>,
    shots: Option<PathBuf>,
}

impl Run {
    fn new(e: Engine) -> Run {
        let shots = std::env::var_os("ATLANTIS2_SHOTS").map(PathBuf::from);
        if let Some(d) = &shots {
            std::fs::create_dir_all(d).unwrap();
        }
        Run {
            e,
            pass: 0,
            gap: 0.0,
            screen: vec![0; 640 * 480],
            shots,
        }
    }

    fn now(&self) -> f64 {
        self.pass as f64 * PASS
    }

    fn step(&mut self) {
        if self.pass.is_multiple_of(4) {
            self.e.tick();
        }
        self.e.set_clock(self.now());
        self.e.pass(Some(&mut self.screen));
        self.pass += 1;
        let fatal = self.e.script_state().and_then(|s| s.fatal.clone());
        assert!(fatal.is_none(), "{fatal:?}");
    }

    fn wait(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    fn events(&mut self) -> Vec<Event> {
        self.step();
        std::mem::take(&mut self.e.events)
    }

    /// A left click far from the last one (never a double click).
    fn tap(&mut self) {
        self.gap += 10.0;
        self.e.click(self.now() + self.gap);
    }

    fn point(&mut self, x: i32, y: i32) {
        let (px, py) = self.e.pointer;
        self.e.mouse_motion(x - px, y - py);
    }

    fn shot(&self, name: &str) {
        if let Some(dir) = &self.shots {
            std::fs::write(
                dir.join(format!("pz-wheels-{name}.png")),
                encode_rgb565(&self.screen, 640, 480),
            )
            .unwrap();
        }
    }

    fn var(&self, name: &str) -> i32 {
        var(&self.e, name)
    }

    /// Turns the view onto zone `zone` (the centre on it).
    fn aim(&mut self, zone: usize) {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        assert_eq!(self.e.centre_zone().map(|(i, _)| i), Some(zone));
    }

    fn move_zone(&self, to: u32) -> usize {
        let view = self.e.place().1;
        self.e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 1 && z.value == to as i32)
            .unwrap_or_else(|| panic!("no move from view {view} to {to}"))
    }

    /// A click on the move zone to view `to`, the transition video
    /// clicked away; the events until the arrival.
    fn walk(&mut self, to: u32) -> Vec<Event> {
        self.wait(15);
        let zone = self.move_zone(to);
        self.aim(zone);
        self.tap();
        let mut all = Vec::new();
        for _ in 0..900 {
            let evs = self.events();
            let video = evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            });
            let arrived = evs.iter().any(|e| matches!(e, Event::ArrivalScan(_)));
            all.extend(evs);
            if video {
                self.wait(15);
                self.tap();
            }
            if arrived {
                assert_eq!(self.e.place().1, to);
                self.wait(4);
                all.extend(std::mem::take(&mut self.e.events));
                assert_eq!(self.e.loop_type, LoopType::Warp);
                return all;
            }
        }
        panic!("no arrival in view {to}");
    }

    /// The bag bar opened with the right button and object `id`'s cell
    /// clicked: the object in hand, the bar closed.
    fn pick(&mut self, id: i32) {
        self.e.right_click();
        assert!(self.e.bar.open);
        self.step();
        let bag = self.e.bag_entries();
        let k = self
            .e
            .bar
            .shown
            .iter()
            .position(|&n| bag[n].id == id)
            .expect("the object in a cell of the bar");
        self.point(40 + 80 * k as i32, 440);
        self.step();
        self.tap();
        self.step();
        assert!(!self.e.bar.open);
        assert_eq!(self.var("OBJETENMAIN"), id);
    }

    /// A click on the axle's place `p` (1..5) of view 13, the centre on
    /// it.
    fn click_place(&mut self, p: i32) -> Vec<Event> {
        let name = format!("AXE_5ROUES{p}");
        let zone = self
            .e
            .wam()
            .view(13)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some(name.as_str()))
            .unwrap();
        self.aim(zone);
        self.tap();
        let evs = self.events();
        assert!(evs.iter().any(|e| matches!(
            e,
            Event::PuzzleRan {
                function: 0x0048_d170
            }
        )));
        evs
    }

    /// The film the last click started, watched a while then clicked
    /// away; the events until the panorama is back.
    fn watch(&mut self, shot: &str) -> Vec<Event> {
        assert_eq!(self.e.loop_type, LoopType::Video);
        self.wait(120);
        self.shot(shot);
        self.tap();
        let mut evs = Vec::new();
        for _ in 0..600 {
            evs.extend(self.events());
            if self.e.loop_type == LoopType::Warp {
                break;
            }
        }
        assert_eq!(self.e.loop_type, LoopType::Warp);
        evs
    }

    fn pos(&self) -> Vec<i32> {
        (1..=5).map(|k| self.var(&format!("pos_roue{k}"))).collect()
    }
}

fn film(evs: &[Event]) -> Option<String> {
    evs.iter().find_map(|e| match e {
        Event::VideoStart {
            name,
            frames: Some(_),
        } if name.to_ascii_lowercase().starts_with("cine") => Some(name.to_ascii_lowercase()),
        _ => None,
    })
}

fn lines(evs: &[Event], chapter: i32) -> Vec<usize> {
    evs.iter()
        .filter_map(|e| match e {
            Event::Set { chapter: c, line } if *c == chapter => Some(*line),
            _ => None,
        })
        .collect()
}

#[test]
fn the_wheels_played_with_the_mouse_to_the_bridge() {
    let Some(files) = game() else { return };
    // China as chapter 48 leaves the player who has given the five
    // figurines their coins: the five discs in the bag (chapter 48's
    // `sacados=DISQUE<k>`), the game in view 7 of the miniature garden.
    let mut e = in_miniature(files, 7, 48);
    for k in 1..=5 {
        e.run_command(format!("(sacados=DISQUE{k})").as_bytes());
        e.run_command(format!("(flag15{}=1)", k - 1).as_bytes());
    }
    e.run_command(b"(objetenmain=0)");
    let mut r = Run::new(e);
    r.wait(20);
    let discs: Vec<i32> = (1..=5).map(|k| id(&r.e, &format!("DISQUE{k}"))).collect();

    // To the bridge: 7 → 10 (chapter 48 line 14: chapter 55), 11, 12.
    let evs = r.walk(10);
    assert_eq!(lines(&evs, 48), [8, 9, 10, 13, 14]);
    assert_eq!(r.e.script_state().unwrap().chapter, 55);
    r.walk(11);
    r.walk(12);
    // Across the raised bridge: chapter 55 aborts the move (line 14).
    let zone = r.move_zone(14);
    r.wait(15);
    r.aim(zone);
    r.wait(2);
    r.shot("1-bridge-raised");
    r.tap();
    let mut evs = Vec::new();
    for _ in 0..120 {
        evs.extend(r.events());
    }
    assert_eq!(lines(&evs, 55), [14]);
    assert_eq!(r.e.place(), ("miniature.wam", 12));
    assert!(!evs.iter().any(|e| matches!(e, Event::ArrivalScan(_))));

    // To the axle (view 13): the five places empty (`RoueK5`, shown at
    // load, hidden by the entry).
    let evs = r.walk(13);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0048_d080
    }));
    for k in 1..=5 {
        assert!(slot(&r.e, &format!("RoueK{k}")).starts_with("0/"));
    }
    r.shot("2-axle-empty");

    // An empty hand on a place: nothing.
    r.click_place(3);
    assert_eq!(r.pos(), [0; 5]);

    // Disc 3 on place 1, then disc 1 on the same place: a swap (disc 3 in
    // hand again, back in the bag).
    r.pick(discs[2]);
    r.click_place(1);
    assert_eq!(r.pos(), [0, 0, 1, 0, 0]);
    assert_eq!(r.var("OBJETENMAIN"), 0);
    assert!(!r.e.bag_entries().iter().any(|b| b.id == discs[2]));
    r.pick(discs[0]);
    r.click_place(1);
    assert_eq!(r.pos(), [1, 0, 0, 0, 0]);
    assert_eq!(r.var("OBJETENMAIN"), discs[2]);
    assert!(r.e.bag_entries().iter().any(|b| b.id == discs[2]));
    r.wait(2);
    r.shot("3-disc-1-on-1");
    // Disc 3 (in hand) on place 3, then the others in order: the last
    // one half lowers the bridge (film 24; nothing in chapter 55 reacts).
    r.click_place(3);
    for k in [2, 4] {
        r.pick(discs[k - 1]);
        r.click_place(k as i32);
    }
    r.wait(2);
    r.shot("4-four-discs");
    r.pick(discs[4]);
    let evs = r.click_place(5);
    assert_eq!(film(&evs).as_deref(), Some("cine024.hnm"));
    assert_eq!(r.pos(), [1, 2, 3, 4, 5]);
    assert_eq!((r.var("findisque1"), r.var("findisque2")), (1, 0));
    // The click scan of that frame does not run (the film's `cinema`
    // clears the click).
    assert!(!evs.iter().any(|e| matches!(e, Event::ClickScan(_))));
    r.watch("5-film-24");
    r.wait(2);
    r.shot("6-first-order");

    // The second order, 5 3 1 4 2, by swaps: disc 1 taken, laid on 5
    // (disc 5 in hand), on 2 (disc 2), on 3 (disc 3), on 1.
    r.click_place(1);
    assert_eq!(r.var("OBJETENMAIN"), discs[0]);
    r.click_place(5);
    assert_eq!(r.var("OBJETENMAIN"), discs[4]);
    r.click_place(2);
    assert_eq!(r.var("OBJETENMAIN"), discs[1]);
    r.click_place(3);
    assert_eq!(r.var("OBJETENMAIN"), discs[2]);
    let evs = r.click_place(1);
    assert_eq!(film(&evs).as_deref(), Some("cine025.hnm"));
    assert_eq!(r.pos(), [5, 3, 1, 4, 2]);
    assert_eq!((r.var("findisque1"), r.var("findisque2")), (1, 1));
    r.watch("7-film-25");
    r.wait(2);
    r.shot("8-second-order");
    // The bridge lowered: `Pont_Baisse` shown in every view that has it.
    let lowered = id(&r.e, "Pont_Baisse");
    assert!(r.e.anim.table.0.contains(&(lowered, 1, 2)));
    // Done: the places take no more click.
    r.click_place(4);
    assert_eq!(r.pos(), [5, 3, 1, 4, 2]);
    assert_eq!(r.var("OBJETENMAIN"), 0);

    // Back to view 12 and across: chapter 55 line 18, chapter 56.
    r.walk(12);
    assert!(r.e.anim.slots.iter().any(|s| s.id == lowered && s.active));
    let zone = r.move_zone(14);
    r.aim(zone);
    r.wait(2);
    r.shot("9-bridge-lowered");
    let evs = r.walk(14);
    assert_eq!(lines(&evs, 55), [18]);
    assert_eq!(r.e.script_state().unwrap().chapter, 56);
    r.shot("10-across");
}

#[test]
fn the_wheels_are_drawn_again_after_a_load() {
    let Some(files) = game() else { return };
    let mut e = in_miniature(files, 13, 55);
    for k in 1..=5 {
        e.run_command(format!("(sacados=DISQUE{k})").as_bytes());
    }
    for (k, p) in [(1, 2), (3, 5)] {
        e.run_command(format!("(objetenmain=DISQUE{k})").as_bytes());
        e.wheels_click(&zone(3, Some(&format!("AXE_5ROUES{p}"))));
    }
    let d = e.save_data();
    let before = st(&e);
    // Both taken back, then the save loaded: view 13 entered again, the
    // two wheels drawn from `pos_roue1` and `pos_roue3`.
    e.run_command(b"(objetenmain=0)");
    e.wheels_click(&zone(3, Some("AXE_5ROUES2")));
    e.run_command(b"(objetenmain=0)");
    e.wheels_click(&zone(3, Some("AXE_5ROUES5")));
    assert_eq!(slot(&e, "RoueK3"), "0/1/0");
    e.load_data(&d).unwrap();
    settle(&mut e);
    assert_eq!(e.place(), ("miniature.wam", 13));
    assert_eq!(
        (1..=5)
            .map(|k| var(&e, &format!("pos_roue{k}")))
            .collect::<Vec<_>>(),
        [2, 0, 5, 0, 0]
    );
    // `RoueK3` shows disc 1, `RoueK5` disc 3.
    assert_eq!(slot(&e, "RoueK3"), "1/1/0");
    assert_eq!(slot(&e, "RoueK5"), "1/4/0");
    for k in [1, 2, 4] {
        assert_eq!(slot(&e, &format!("RoueK{k}")), "0/0/1");
    }
    assert!(before.contains("wheels [0/0/1,0/0/1,1/1/0,0/0/1,1/4/0]"));
}

/// The background of the golden's faces.
fn background() -> Vec<u16> {
    (0..512u64 * 512)
        .map(|i| (((i * 2_654_435_761) & 0xffff_ffff) >> 16) as u16)
        .collect()
}

fn md5_of(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    format!("{:x}", Md5::digest(&bytes))
}

#[test]
fn the_wheels_are_drawn_as_the_original() {
    let Some(files) = game() else { return };
    let bg = background();
    let bg_md5 = md5_of(&bg);
    assert!(GOLDEN.contains(&format!("md5 {bg_md5}")));
    let mut got = Vec::new();
    let mut want = Vec::new();
    for row in rows("F file ") {
        let w: Vec<&str> = row.split(' ').collect();
        let name = w[2];
        let data = files
            .read_from_disc(3, Folder::Spw, name)
            .expect("the wheel's SPW");
        assert_eq!(format!("{:x}", Md5::digest(&data)), w[4], "{name}");
        let spw = Spw::parse(data.clone()).unwrap();
        let count: i32 = w[6].parse().unwrap();
        for f in 0..count {
            let mut a = Anim::new();
            a.load(
                name,
                Spw::parse(data.clone()).unwrap(),
                2,
                132,
                0,
                EntityState {
                    letter: 65,
                    anim: 1,
                    shown: 1,
                    single: 1,
                    ..Default::default()
                },
            );
            a.slots[0].frame = f;
            a.slots[0].playing = false;
            let mut faces: [Vec<u16>; 6] = std::array::from_fn(|_| bg.clone());
            let (_, hit) = a.draw(&mut faces, None, true);
            let al = i32::from(hit.slot.is_some());
            let mut parts = Vec::new();
            for (k, px) in faces.iter().enumerate() {
                let h = md5_of(px);
                if h == bg_md5 {
                    parts.push(format!("f{k} bg"));
                    continue;
                }
                let n = px.iter().zip(&bg).filter(|(a, b)| a != b).count();
                let mut samples = Vec::new();
                if let Some(b) = spw.face_box(f, k) {
                    let (x, y, w, hh) = (
                        i32::from(b.x),
                        i32::from(b.y),
                        i32::from(b.w),
                        i32::from(b.h),
                    );
                    if w > 0 && hh > 0 {
                        for (sx, sy) in [(x + w / 2, y + hh / 2), (x, y), (x + w - 1, y + hh - 1)] {
                            samples.push(format!("{sx},{sy}={:04x}", px[(sy * 512 + sx) as usize]));
                        }
                    }
                }
                parts.push(format!("f{k} {h} n{n} {}", samples.join(" ")));
            }
            got.push(format!(
                "F {name} frame {f} AL {al} | {}",
                parts.join(" | ")
            ));
        }
        want.extend(
            rows(&format!("F {name} frame "))
                .iter()
                .map(|r| r.to_string()),
        );
    }
    assert_eq!(got.len(), 50);
    assert_eq!(got, want);
}

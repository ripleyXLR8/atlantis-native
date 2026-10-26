//! The turtle ring (`salletortue.wam` views 40-64, chapter 46) against the
//! original (`tests/golden/turtle.txt`, its header there): the view entry
//! hook (rows B), the click hook alone (C), the tick step by step on the
//! dispatcher's 15 Hz flag (D), the click and entry dispatchers' choice
//! and the puzzle tick by mode (E), the zone clicks with the click scan
//! of the same frame on chapter 46 and the arrivals of the master's
//! indications to the win (F), the save entries (G) and the ring drawn
//! (H: the faces' md5s and sampled pixels).
//!
//! Then the turtle room played as a player plays it, with the mouse: from
//! chapter 43 in `chine.wam` view 32 with the gardener master's word given
//! (`flag133`, the turtle master shown, as chapter 42's dialog 223 leaves
//! them) into the room and chapter 46, the turtle and the ring received,
//! the ring turned both ways, the turtle laid on the water, the master
//! asked about the tiger's home, his indications walked to view 47, the
//! film and chapter 47 in `miniature.wam`; and the ring kept by a save.
//! With `ATLANTIS2_SHOTS` set to a folder, the played test writes
//! `pz-turtle-*.png` there.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::collections::VecDeque;
use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::spw::Spw;
use atlantis2_data::wam::EntityState;
use atlantis2_data::{A2Files, Folder};

use atlantis2_engine::anim::Anim;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::engine::{chinese_click_hooks, chinese_entry_hooks, RING_MODE};
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::subjects::cell;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/turtle.txt");

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

/// The golden's rows starting with `prefix`, in order.
fn rows(prefix: &str) -> Vec<&'static str> {
    GOLDEN.lines().filter(|l| l.starts_with(prefix)).collect()
}

fn lst<T: ToString>(xs: impl IntoIterator<Item = T>) -> String {
    let v: Vec<String> = xs.into_iter().map(|x| x.to_string()).collect();
    format!("[{}]", v.join(","))
}

/// The rows against the golden's, the first different pair shown.
fn same<A: AsRef<str>, B: AsRef<str>>(got: &[A], want: &[B]) {
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g.as_ref(), w.as_ref());
    }
    assert_eq!(got.len(), want.len());
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

/// A game in the turtle room, chapter 46, view `view`, as the scripts put
/// it there (the world, the place, the view, the chapter).
fn in_chapter_46(files: A2Files, view: u32) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=3)"[..],
        b"(changewam=salletortue.wam)",
        format!("(vue={view})").as_bytes(),
        b"(chapter=46)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("salletortue.wam", view));
    assert_eq!(e.script_state().unwrap().chapter, 46);
    assert_eq!(e.files().current_disc(), 3);
    e.trace = true;
    e
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

/// The `ANNEAU_GRAVE` slots as the golden writes them.
fn rings(e: &Engine) -> String {
    let ring = id(e, defnames::OBJETS, "ANNEAU_GRAVE");
    let v: Vec<String> = e
        .anim
        .slots
        .iter()
        .filter(|s| s.id == ring)
        .map(|s| {
            format!(
                "f{}/p{}/a{}",
                s.frame,
                i32::from(s.playing),
                i32::from(s.active)
            )
        })
        .collect();
    if v.is_empty() {
        "-".into()
    } else {
        v.join(",")
    }
}

/// The golden's state.
fn st(e: &Engine) -> String {
    format!(
        "mode {} count {} store {} ring {} flag {}",
        e.puzzle_mode(),
        e.ring_count(),
        e.ring_store(),
        rings(e),
        i32::from(e.frame_flag)
    )
}

/// The `NEWSOUND`s voice 2 starts (`sound%03d.apc`), in order.
fn sounds(evs: &[Event]) -> String {
    lst(evs.iter().filter_map(|e| match e {
        Event::Sound(n) if n.starts_with("sound") => Some(n.clone()),
        _ => None,
    }))
}

/// The lines of `chapter` the scans ran after the arrival scan's start
/// (the chapter's `/INIT`, run at the chapter change, left out).
fn arrival_lines(evs: &[Event], chapter: i32) -> Vec<usize> {
    let start = evs
        .iter()
        .position(|x| matches!(x, Event::ArrivalScan(_)))
        .expect("an arrival");
    lines(&evs[start..], chapter)
}

fn lines(evs: &[Event], chapter: i32) -> Vec<usize> {
    evs.iter()
        .filter_map(|e| match e {
            Event::Set { chapter: c, line } if *c == chapter => Some(*line),
            _ => None,
        })
        .collect()
}

/// The warnings and the fatal error since the last call, as the golden
/// writes them; both cleared.
fn warns(e: &mut Engine) -> String {
    let s = e.script_state_mut().unwrap();
    let mut w: Vec<String> = s
        .warnings
        .drain(..)
        .map(|f| format!("warning:{f}"))
        .collect();
    if let Some(f) = s.fatal.take() {
        w.push(format!("fatal:{f}"));
    }
    lst(w)
}

/// The golden's names of the hooks and ticks the dispatchers reach.
fn hook_name(f: u32) -> String {
    match f {
        0x0049_c290 => "ring-click".into(),
        0x0049_c340 => "ring-entry".into(),
        0x0049_c400 => "ring-tick".into(),
        0x0048_da20 => "figurines-click".into(),
        0x0048_d680 => "figurines-entry".into(),
        0x0048_d170 => "wheels-click".into(),
        0x0048_d080 => "wheels-entry".into(),
        0x0049_4600 => "clouds-click".into(),
        0x0049_44a0 => "clouds-entry".into(),
        0x0049_45f0 => "clouds-move".into(),
        0x0049_3960 => "chest-click".into(),
        0x0049_38f0 => "chest-entry".into(),
        0x0048_df10 => "souls-entry".into(),
        0x0049_1610 => "spider-tick".into(),
        0x0048_eb30 => "souls-tick".into(),
        0x0049_aaa0 => "octopus-tick".into(),
        0x0049_47b0 => "dragon-tick".into(),
        0x0049_3190 => "maya-tick".into(),
        f => format!("{f:08x}"),
    }
}

/// The golden's harness state: the mode, count and store given, the flag
/// clear, the view's slots as AtlSpw_load leaves them (`rings` ring
/// slots at frame `frame`, playing and shown, then the view's others);
/// the warnings, the fatal error and the events dropped.
fn fresh(e: &mut Engine, mode: i32, count: i32, store: i32, rings: usize, frame: i32) {
    e.set_puzzle_mode(mode);
    e.set_ring_count(count);
    e.set_ring_store(store);
    e.frame_flag = false;
    let ring = id(e, defnames::OBJETS, "ANNEAU_GRAVE");
    let k = e.anim.slots.iter().position(|s| s.id == ring).unwrap();
    let mut slot = e.anim.slots[k].clone();
    slot.frame = frame;
    slot.playing = true;
    slot.active = true;
    e.anim.slots.retain(|s| s.id != ring);
    for _ in 0..rings {
        e.anim.slots.insert(0, slot.clone());
    }
    let _ = warns(e);
    e.events.clear();
}

/// The left and right zones' values (their `#ZONES_CLIC#` indices).
fn zones(e: &Engine) -> (i32, i32) {
    (
        id(e, defnames::ZONES_CLIC, "TORTUE_GAUCHE"),
        id(e, defnames::ZONES_CLIC, "TORTUE_DROITE"),
    )
}

#[test]
fn the_ids_and_the_save_entries() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_46(files, 56);
    let ids = format!(
        "ids objects {} {} character {} zones {} {} {} {}",
        id(&e, defnames::OBJETS, "ANNEAU_GRAVE"),
        id(&e, defnames::OBJETS, "TORTUE_NOIRE"),
        id(&e, defnames::PERSONNAGES, "MAITRE_TORTUE"),
        id(&e, defnames::ZONES_CLIC, "TORTUE_GAUCHE"),
        id(&e, defnames::ZONES_CLIC, "TORTUE_DROITE"),
        id(&e, defnames::ZONES_CLIC, "EAU_TORTUE"),
        id(&e, defnames::ZONES_CLIC, "TGA_TORTUE"),
    );
    same(&[ids], &rows("ids objects"));
    // G: the mode, the count and the store are save entries 26, 28, 29.
    same(
        &rows("G "),
        &[
            "G save entry 26 = 0x006d84d0 4",
            "G save entry 28 = 0x006d84e0 4",
            "G save entry 29 = 0x006d84e4 4",
        ],
    );
    e.set_ring_count(-7);
    e.set_ring_store(31);
    let d = e.save_data();
    assert_eq!(d.entry(26), RING_MODE.to_le_bytes());
    assert_eq!(d.entry(28), (-7i32).to_le_bytes());
    assert_eq!(d.entry(29), 31i32.to_le_bytes());
}

/// B: the entry hook alone on the view's slots.
#[test]
fn the_entry_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_46(files, 56);
    let mut got = Vec::new();
    let mut cases: Vec<(u32, i32, i32, i32)> = [38, 39, 40, 47, 56, 63, 64, 65, 200]
        .iter()
        .map(|&v| (v, 0, 7, 0))
        .collect();
    cases.extend(
        [0, 9, 19, 29, 37, 38, 39, 40, -1, -5]
            .iter()
            .map(|&s| (56, 2, s, 0)),
    );
    cases.extend([
        (56, 5, 19, 3),
        (56, 1, 19, 3),
        (56, 3, 19, 3),
        (56, 2, 19, 3),
        (39, 2, 19, 3),
        (65, 5, 19, 3),
    ]);
    for (view, mode, store, frame) in cases {
        fresh(&mut e, mode, 4, store, 1, frame);
        e.turtle_entry(view);
        got.push(format!(
            "B view {view} mode {mode} store {store} frame {frame} -> {} | warn {}",
            st(&e),
            warns(&mut e)
        ));
    }
    fresh(&mut e, 2, 0, 12, 2, 5);
    e.turtle_entry(56);
    got.push(format!(
        "B two ring slots, mode 2 store 12 -> {} | warn {}",
        st(&e),
        warns(&mut e)
    ));
    fresh(&mut e, 0, 0, 12, 0, 0);
    e.turtle_entry(56);
    got.push(format!(
        "B no ring slot, mode 0 store 12 -> {} | warn {}",
        st(&e),
        warns(&mut e)
    ));
    // The table hides the ring: loaded hidden, the entry makes it active.
    let ring = id(&e, defnames::OBJETS, "ANNEAU_GRAVE");
    e.set_puzzle_mode(2);
    e.set_ring_count(0);
    e.set_ring_store(9);
    e.anim.table.set(ring, 0, 2);
    e.enter_at(56, 0.0, 0.0).unwrap();
    got.push(format!(
        "B ring hidden by the table, mode 2 store 9 -> {} | warn {}",
        st(&e),
        warns(&mut e)
    ));
    same(&got, &rows("B "));
}

/// C: the click hook alone; D: the tick, step by step.
#[test]
fn the_click_and_the_tick_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_46(files, 56);
    let (gauche, droite) = zones(&e);
    let others = [
        id(&e, defnames::ZONES_CLIC, "EAU_TORTUE"),
        id(&e, defnames::ZONES_CLIC, "TGA_TORTUE"),
        0,
        -1,
        999,
    ];
    let mut got = Vec::new();
    let click = |e: &mut Engine, label: &str, view: u32, v: i32, t: i32, got: &mut Vec<String>| {
        e.events.clear();
        e.turtle_click(view, v);
        let evs = std::mem::take(&mut e.events);
        got.push(format!(
            "C {label} zone {t} {v} -> {} | sounds {} | warn {}",
            st(e),
            sounds(&evs),
            warns(e)
        ));
    };
    fresh(&mut e, 0, 0, 0, 1, 0);
    e.turtle_entry(56);
    got.push(format!("C entered -> {} | warn {}", st(&e), warns(&mut e)));
    click(&mut e, "left", 56, gauche, 3, &mut got);
    click(&mut e, "left again", 56, gauche, 3, &mut got);
    click(&mut e, "right", 56, droite, 3, &mut got);
    for store in [0, 9, 19, 29, 1, 8, 10, 38, 39, -1] {
        for (name, v) in [("left", gauche), ("right", droite)] {
            fresh(&mut e, 2, 0, store, 1, 0);
            e.turtle_entry(56);
            e.set_ring_store(store);
            click(&mut e, &format!("{name} store {store}"), 56, v, 3, &mut got);
        }
    }
    fresh(&mut e, 0, 0, 0, 1, 0);
    e.turtle_entry(56);
    // The zone's type is not read: the hook gets the value only.
    for t in [4, 0x32, 1, 2, 0, 10] {
        e.set_ring_count(0);
        click(&mut e, "type", 56, gauche, t, &mut got);
    }
    for v in others {
        e.set_ring_count(0);
        click(&mut e, "other value", 56, v, 3, &mut got);
    }
    for count in [1, -1, 5] {
        e.set_ring_count(count);
        click(&mut e, &format!("count {count}"), 56, droite, 3, &mut got);
    }
    for view in [39, 40, 64, 65, 3] {
        fresh(&mut e, 2, 0, 0, 1, 0);
        e.turtle_entry(56);
        click(&mut e, &format!("view {view}"), view, gauche, 3, &mut got);
    }
    fresh(&mut e, 0, 0, 0, 1, 0);
    click(&mut e, "no entry, mode 0", 56, gauche, 3, &mut got);
    same(&got, &rows("C "));

    // D: one tick per row element, the flag set before each unless given.
    let mut got = Vec::new();
    let seq =
        |e: &mut Engine, label: &str, view: u32, n: usize, flag: bool, got: &mut Vec<String>| {
            let mut out = Vec::new();
            for _ in 0..n {
                e.frame_flag = flag;
                e.turtle_tick(view);
                out.push(format!("{}:{}", rings(e), e.ring_count()));
            }
            got.push(format!(
                "D {label} -> {} | {} | warn {}",
                out.join(" "),
                st(e),
                warns(e)
            ));
        };
    for (store, name, v) in [
        (0, "left", gauche),
        (9, "left", gauche),
        (19, "left", gauche),
        (29, "left", gauche),
        (0, "right", droite),
        (9, "right", droite),
        (19, "right", droite),
        (29, "right", droite),
        (38, "left", gauche),
        (38, "right", droite),
    ] {
        fresh(&mut e, 2, 0, store, 1, 0);
        e.turtle_entry(56);
        e.turtle_click(56, v);
        seq(
            &mut e,
            &format!("{name} from {store}"),
            56,
            12,
            true,
            &mut got,
        );
    }
    fresh(&mut e, 0, 0, 0, 1, 0);
    e.turtle_entry(56);
    e.turtle_click(56, gauche);
    seq(&mut e, "flag clear", 56, 3, false, &mut got);
    seq(&mut e, "then set", 56, 3, true, &mut got);
    // The count carried into the next view: its slots loaded again (the
    // ring at frame 0, playing), its entry restores the store.
    let (count, store) = (e.ring_count(), e.ring_store());
    fresh(&mut e, 2, count, store, 1, 0);
    e.turtle_entry(57);
    got.push(format!("D entered 57 -> {}", st(&e)));
    seq(&mut e, "in 57", 57, 8, true, &mut got);
    let ring_slot = |e: &mut Engine| {
        let ring = id(e, defnames::OBJETS, "ANNEAU_GRAVE");
        e.anim.slots.iter().position(|s| s.id == ring).unwrap()
    };
    fresh(&mut e, 2, 0, 0, 1, 0);
    e.turtle_entry(56);
    let k = ring_slot(&mut e);
    e.anim.slots[k].frame = 45;
    e.set_ring_count(2);
    seq(&mut e, "frame 45 count 2", 56, 2, true, &mut got);
    fresh(&mut e, 2, 0, 0, 1, 0);
    e.turtle_entry(56);
    e.anim.slots[k].frame = -3;
    e.set_ring_count(-2);
    seq(&mut e, "frame -3 count -2", 56, 2, true, &mut got);
    fresh(&mut e, 2, 0, 0, 1, 0);
    e.turtle_entry(56);
    e.set_ring_count(0);
    e.anim.slots[k].frame = 7;
    seq(
        &mut e,
        "count 0, frame 7 set in the slot",
        56,
        1,
        true,
        &mut got,
    );
    for view in [39, 65, 7] {
        fresh(&mut e, 2, 0, 0, 1, 0);
        e.turtle_entry(56);
        e.turtle_click(56, gauche);
        seq(&mut e, &format!("view {view}"), view, 1, true, &mut got);
    }
    // The 15 Hz SPW step leaves the held ring alone; set playing again,
    // it runs on.
    fresh(&mut e, 2, 0, 19, 1, 0);
    e.turtle_entry(56);
    let mut steps = Vec::new();
    for _ in 0..4 {
        e.anim.step(&mut e.rand);
        steps.push(rings(&e));
    }
    e.anim.slots[k].playing = true;
    for _ in 0..3 {
        e.anim.step(&mut e.rand);
        steps.push(rings(&e));
    }
    got.push(format!(
        "D SPW step FUN_0047d7f0 x4 held, x3 playing set back -> {}",
        steps.join(" ")
    ));
    same(&got, &rows("D "));
}

/// E: the dispatchers' choice (the port's lists) with the ring's hooks
/// run, and the puzzle tick by mode (the pause byte, which the port does
/// not have, left out).
#[test]
fn the_dispatchers_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_46(files, 56);
    let (gauche, _) = zones(&e);
    let mut got = Vec::new();
    let dispatch = |e: &mut Engine, name: &str, number: u32, t: i32, mode: i32| {
        fresh(e, mode, 0, 0, 1, 0);
        let hooks = chinese_click_hooks(name.as_bytes(), number, t);
        for &f in &hooks {
            if f == 0x0049_c290 {
                e.turtle_click(number, gauche);
            }
        }
        let evs = std::mem::take(&mut e.events);
        let names: Vec<String> = hooks.iter().map(|&f| hook_name(f)).collect();
        format!(
            "E click {name} {number} {t} mode {mode} -> {} AL 0 | {} | sounds {}",
            if names.is_empty() {
                "-".to_string()
            } else {
                names.join(",")
            },
            st(e),
            sounds(&evs)
        )
    };
    for number in [39, 40, 56, 63, 64, 65] {
        for t in [1, 3] {
            got.push(dispatch(&mut e, "ch0056.hnm", number, t, 2));
        }
    }
    for t in [2, 4, 0x32, 0, 10] {
        got.push(dispatch(&mut e, "ch0056.hnm", 56, t, 2));
    }
    for name in [
        "CH0056.HNM",
        "Ch56",
        "ss0056.hnm",
        "la0056.hnm",
        "ti0056.hnm",
        "C",
    ] {
        got.push(dispatch(&mut e, name, 56, 3, 2));
    }
    got.push(dispatch(&mut e, "ch0056.hnm", 56, 3, 0));
    got.push(dispatch(&mut e, "ch0056.hnm", 56, 3, 5));
    let cases: [(&str, u32, i32); 13] = [
        ("ch0039.hnm", 39, 0),
        ("ch0040.hnm", 40, 0),
        ("ch0047.hnm", 47, 0),
        ("ch0056.hnm", 56, 0),
        ("ch0064.hnm", 64, 0),
        ("ch0065.hnm", 65, 0),
        ("ch0056.hnm", 56, 2),
        ("ch0056.hnm", 56, 5),
        ("CH0050.HNM", 50, 0),
        ("ch0032.hnm", 32, 2),
        ("la0040.hnm", 40, 0),
        ("ss0040.hnm", 40, 0),
        ("ss0005.hnm", 5, 2),
    ];
    for (name, number, mode) in cases {
        fresh(&mut e, mode, 0, 12, 1, 0);
        let hooks = chinese_entry_hooks(name.as_bytes(), number);
        for &f in &hooks {
            if f == 0x0049_c340 {
                e.turtle_entry(number);
            }
        }
        let names: Vec<String> = hooks.iter().map(|&f| hook_name(f)).collect();
        got.push(format!(
            "E entry {name} {number} mode {mode} -> {} | {} | warn {}",
            if names.is_empty() {
                "-".to_string()
            } else {
                names.join(",")
            },
            st(&e),
            warns(&mut e)
        ));
    }
    for mode in [2, 0, 1, 3, 4, 5, 6, 7] {
        fresh(&mut e, 2, 0, 0, 1, 0);
        e.turtle_entry(56);
        e.turtle_click(56, gauche);
        e.set_puzzle_mode(mode);
        e.frame_flag = true;
        e.events.clear();
        let first = e.puzzle_tick();
        let mut names: Vec<String> = e
            .events
            .iter()
            .filter_map(|x| match x {
                Event::PuzzleRan { function } | Event::PuzzleNotPorted { function } => {
                    Some(hook_name(*function))
                }
                _ => None,
            })
            .collect();
        // The pearl and dragon's step (`a2/pz-dragon`) runs without an
        // event: the step the tick returns names it.
        if let Some(f) = first.map(hook_name).filter(|f| !names.contains(f)) {
            names.insert(0, f);
        }
        // The golden's other puzzles' ticks are stubs that leave the mode;
        // the octopus fight's runs here (`a2/pz-octopus`) and, in a view
        // numbered above 19, quits: the mode 0.
        if mode == 6 {
            assert_eq!(e.puzzle_mode(), 0);
            e.set_puzzle_mode(6);
        }
        got.push(format!(
            "E tick mode {mode} pause 0 -> {} | {}",
            if names.is_empty() {
                "-".to_string()
            } else {
                names.join(",")
            },
            st(&e)
        ));
    }
    let want: Vec<&str> = rows("E ")
        .into_iter()
        .filter(|r| !r.contains(" pause 1 "))
        .collect();
    same(&got, &want);
}

/// The view's zone named `name` turned to (the centre on it, as the
/// golden's click under the centre).
fn aim_at(e: &mut Engine, name: &str) -> (usize, i32, i32) {
    let view = e.place().1;
    let zones = &e.wam().view(view).unwrap().zones;
    let z = zones
        .iter()
        .position(|z| z.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no zone {name} in view {view}"));
    let (kind, value) = (zones[z].kind, zones[z].value);
    let (yaw, pitch) = e.zone_aim(view, z).unwrap();
    e.camera.enter(yaw, pitch);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(z));
    (z, kind, value)
}

/// F: the zone clicks as the game runs them (the handler, the hook,
/// `CLICZONE`, the click scan of the same frame on chapter 46), then the
/// arrivals of the master's indications through real moves to the win.
#[test]
fn the_clicks_and_chapter_46_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_46(files, 56);
    for c in [
        &b"(sacados=TORTUE_NOIRE)"[..],
        b"(sacados=ANNEAU_GRAVE)",
        b"(objetenmain=TORTUE_NOIRE)",
    ] {
        e.run_command(c);
    }
    let mut got = Vec::new();
    let mut t = 0.0;
    let mut click = |e: &mut Engine, label: &str, name: &str, got: &mut Vec<String>| {
        let view = e.place().1;
        let (_, kind, value) = aim_at(e, name);
        let _ = warns(e);
        e.events.clear();
        t += 100.0;
        e.click(t);
        e.pass(None);
        let evs = std::mem::take(&mut e.events);
        let zc = evs
            .iter()
            .position(|x| matches!(x, Event::ZoneClick { .. }))
            .expect("the zone click");
        let Event::ZoneClick { cliczone, .. } = evs[zc] else {
            unreachable!()
        };
        let hooks: Vec<String> = evs[..zc]
            .iter()
            .filter_map(|x| match x {
                Event::PuzzleRan { function } | Event::PuzzleNotPorted { function } => {
                    Some(hook_name(*function))
                }
                _ => None,
            })
            .collect();
        let scan = evs.iter().any(|x| matches!(x, Event::ClickScan(_)));
        let pictures: Vec<String> = evs
            .iter()
            .filter_map(|x| match x {
                Event::Picture { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        if label == "picture" {
            assert_eq!(pictures, ["zoomTor.spr"]);
            // The picture until a click.
            e.click(t + 50.0);
            e.pass(None);
            assert_eq!(e.loop_type, LoopType::Warp);
        } else {
            assert!(pictures.is_empty());
        }
        let state = st(e);
        let warn = warns(e);
        got.push(format!(
            "F {label} view {view} zone {kind} {value} -> out {} | hooks {} sounds {} | cliczone {cliczone} | click scan {} | in {} sounds {} calls * | resumes 0 - | {state} | hand {} flag184 {} | warn {}",
            lst(lines(&evs[..zc], 46)),
            if hooks.is_empty() { "-".to_string() } else { hooks.join(",") },
            sounds(&evs[..zc]),
            i32::from(scan),
            lst(lines(&evs[zc..], 46)),
            sounds(&evs[zc..]),
            var(e, "OBJETENMAIN"),
            e.script_state().unwrap().flags[184],
            warn,
        ));
    };
    click(&mut e, "left", "TORTUE_GAUCHE", &mut got);
    click(&mut e, "left busy", "TORTUE_GAUCHE", &mut got);
    // The harness's twelve ticks, each on the dispatcher's flag.
    for _ in 0..12 {
        e.frame_flag = true;
        e.turtle_tick(56);
    }
    click(&mut e, "right", "TORTUE_DROITE", &mut got);
    click(&mut e, "water", "EAU_TORTUE", &mut got);
    click(&mut e, "picture", "TGA_TORTUE", &mut got);
    // View 58, whose water zone is of type 10: no hook, the scan all the
    // same; the turtle given back and held, the mode 2.
    e.run_command(b"(sacados=TORTUE_NOIRE)");
    e.set_ring_count(0);
    e.set_ring_store(0);
    e.enter_at(58, 0.0, 0.0).unwrap();
    e.run_command(b"(objetenmain=TORTUE_NOIRE)");
    click(&mut e, "water type 10", "EAU_TORTUE", &mut got);
    let want: Vec<String> = rows("F ")
        .iter()
        .filter(|r| !r.starts_with("F arrive"))
        .map(|r| {
            let (a, b) = r.split_once(" calls ").unwrap();
            format!(
                "{a} calls * | resumes{}",
                b.split_once(" | resumes").unwrap().1
            )
        })
        .collect();
    same(&got, &want);

    // The arrivals, each a move from the view before.
    let mut got = Vec::new();
    let walk_state = |e: &mut Engine, ind: i32, flags: [i32; 3], at: u32| {
        e.set_puzzle_mode(2);
        e.set_ring_count(0);
        e.set_ring_store(0);
        for (k, v) in [133, 134, 184].iter().zip(flags) {
            e.run_command(format!("(flag{k}={v})").as_bytes());
        }
        e.run_command(format!("(indication_tortue={ind})").as_bytes());
        e.enter_at(at, 0.0, 0.0).unwrap();
        e.reset_view();
        e.run_command(b"(objetenmain=0)");
        let _ = warns(e);
        e.events.clear();
    };
    let arrive = |e: &mut Engine, label: &str, to: u32, got: &mut Vec<String>| {
        let evs = move_to(e, to);
        let warn = warns(e);
        let fl: Vec<String> = [133, 134, 135, 136, 141, 184]
            .iter()
            .map(|&k| e.script_state().unwrap().flags[k].to_string())
            .collect();
        got.push(format!(
            "F arrive {label} view {to} -> {} | indication {} flags {} chapter {} hand {} | warn {}",
            arrival_story(&evs),
            var(e, "indication_tortue"),
            fl.join("/"),
            e.script_state().unwrap().chapter,
            var(e, "OBJETENMAIN"),
            warn
        ));
    };
    // Back to chapter 46 in the room, the bag as the first meeting finds it.
    e.run_command(b"(sacados=NOT_TORTUE_NOIRE)");
    e.run_command(b"(sacados=NOT_ANNEAU_GRAVE)");
    e.run_command(b"(hideobjet=TORTUE_NOIRE)");
    walk_state(&mut e, 0, [1, 0, 0], 41);
    arrive(&mut e, "first", 40, &mut got);
    arrive(&mut e, "again", 41, &mut got);
    walk_state(&mut e, 1, [1, 1, 1], 40);
    for (k, v) in [41, 42, 43, 44, 45, 46, 47, 48, 47].iter().enumerate() {
        arrive(&mut e, &format!("walk {}", k + 1), *v, &mut got);
    }
    walk_state(&mut e, 1, [1, 1, 1], 40);
    for (k, v) in [55, 54, 53, 52, 51, 50, 49, 60, 47].iter().enumerate() {
        arrive(&mut e, &format!("walk2 {}", k + 1), *v, &mut got);
    }
    walk_state(&mut e, 9, [1, 1, 1], 46);
    arrive(&mut e, "at 9 not 47", 45, &mut got);
    walk_state(&mut e, 9, [1, 1, 1], 46);
    arrive(&mut e, "at 9, 47", 47, &mut got);
    // The golden's harness stubs `RunInit`; the port runs chapter 47's
    // `/INIT` at the win's `chapter=47`, whose line 8 sets `flag135` (as
    // the game does): that flag is left out of the win's row.
    let want: Vec<String> = rows("F arrive")
        .iter()
        .map(|r| {
            if r.starts_with("F arrive at 9, 47 ") {
                r.replace(" flags 1/1/0/1/1/1 ", " flags 1/1/1/1/1/1 ")
            } else {
                r.to_string()
            }
        })
        .collect();
    same(&got, &want);
    assert_eq!(e.place(), ("miniature.wam", 1));
    // Out of the room the ring's mode is gone. The original's is then 5:
    // the chest's entry of `SS` views 1-7 sets it on arrival
    // (@ 0x00493926), which is on `a2/pz-chest`, not here; so the value
    // is not pinned (the golden does not record it).
    assert_ne!(e.puzzle_mode(), RING_MODE);
}

/// A move zone of the current view to view `to` clicked (the centre on
/// it), the transition video, the films and the dialogs ended as they
/// come, until the panorama is back: the events.
fn move_to(e: &mut Engine, to: u32) -> Vec<Event> {
    let view = e.place().1;
    let z = e
        .wam()
        .view(view)
        .unwrap()
        .zones
        .iter()
        .position(|z| z.kind == 1 && z.value == to as i32)
        .unwrap_or_else(|| panic!("no move from view {view} to {to}"));
    let (yaw, pitch) = e.zone_aim(view, z).unwrap();
    e.camera.enter(yaw, pitch);
    e.pass(None);
    e.events.clear();
    let t = e.clock() + 1000.0;
    e.click(t);
    let mut evs = Vec::new();
    for _ in 0..2000 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        evs.append(&mut e.events);
        let arrived = evs.iter().any(|x| matches!(x, Event::ArrivalScan(_)));
        if arrived && e.loop_type == LoopType::Warp && !e.turning() {
            return evs;
        }
    }
    panic!("no arrival in view {to}");
}

/// The golden's account of an arrival: the arrival scan's chapter-46
/// lines up to the first dialog or film, each dialog's or film's end
/// with the lines it resumes, the dialogs and films started.
fn arrival_story(evs: &[Event]) -> String {
    let start = evs
        .iter()
        .position(|x| matches!(x, Event::ArrivalScan(_)))
        .unwrap();
    let mut gere = Vec::new();
    let mut resumes: Vec<Vec<usize>> = Vec::new();
    let mut dialogs = Vec::new();
    let mut films = Vec::new();
    for x in &evs[start..] {
        match x {
            Event::Set { chapter: 46, line } => match resumes.last_mut() {
                Some(r) => r.push(*line),
                None => gere.push(*line),
            },
            Event::DialogStart {
                number,
                started: true,
            } => dialogs.push(*number),
            Event::VideoStart {
                name,
                frames: Some(_),
            } => films.push(name.to_ascii_lowercase()),
            Event::DialogEnd | Event::VideoEnd => resumes.push(Vec::new()),
            _ => {}
        }
    }
    // A film's or dialog's end that resumes nothing of chapter 46 (the
    // section had ended) is no resume of it.
    while resumes.last().is_some_and(|r| r.is_empty()) {
        resumes.pop();
    }
    format!(
        "gere {} | resumes {} {} | dialogs {} films {}",
        lst(&gere),
        resumes.len(),
        if resumes.is_empty() {
            "-".to_string()
        } else {
            resumes.iter().map(lst).collect::<Vec<_>>().join(" ")
        },
        lst(&dialogs),
        lst(&films)
    )
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

/// H: the ring's files drawn at the golden's frames.
#[test]
fn the_ring_is_drawn_as_the_original() {
    let Some(files) = game() else { return };
    let bg = background();
    let bg_md5 = md5_of(&bg);
    assert!(GOLDEN.contains(&format!("md5 {bg_md5}")));
    let golden: Vec<&str> = rows("H ")
        .into_iter()
        .filter(|r| !r.starts_with("H errors"))
        .collect();
    let mut got = Vec::new();
    let mut current: Option<(String, Vec<u8>)> = None;
    for row in &golden {
        let w: Vec<&str> = row.split(' ').collect();
        if row.starts_with("H file ") {
            let name = w[2].to_string();
            let data = files.read(Folder::Spw, &name).expect("the SPW");
            let n = Spw::parse(data.clone()).unwrap().frame_count();
            got.push(format!(
                "H file {name} md5 {:x} frames {n}",
                Md5::digest(&data)
            ));
            current = Some((name, data));
            continue;
        }
        let (name, data) = current.as_ref().unwrap();
        assert_eq!(w[1], name);
        let f: i32 = w[3].parse().unwrap();
        let spw = Spw::parse(data.clone()).unwrap();
        let mut a = Anim::new();
        let state = EntityState {
            letter: 65,
            anim: 1,
            shown: 1,
            single: 1,
            ..Default::default()
        };
        a.load("s", Spw::parse(data.clone()).unwrap(), 2, 84, 0, state);
        a.slots[0].frame = f;
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
            let changed = px.iter().zip(&bg).filter(|(a, b)| a != b).count();
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
            parts.push(format!("f{k} {h} n{changed} {}", samples.join(" ")));
        }
        got.push(format!(
            "H {name} frame {f} AL {al} | {}",
            parts.join(" | ")
        ));
    }
    same(&got, &golden);
}

/// The engine at 59.94 passes a second ([`PASS`]) (the 15 Hz ticks from the clock),
/// every pass drawn; the mouse as a player moves it.
struct Run {
    e: Engine,
    pass: u64,
    gap: f64,
    screen: Vec<u16>,
    shots: Option<PathBuf>,
    mouse_signs: (f32, f32),
}

impl Run {
    fn now(&self) -> f64 {
        self.pass as f64 * PASS
    }

    fn step(&mut self) {
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
                dir.join(format!("pz-turtle-{name}.png")),
                encode_rgb565(&self.screen, 640, 480),
            )
            .unwrap();
        }
    }

    fn var(&self, name: &str) -> i32 {
        var(&self.e, name)
    }

    fn flag(&self, n: usize) -> i32 {
        self.e.script_state().unwrap().flags[n]
    }

    fn ring(&self) -> (i32, bool) {
        let ring = id(&self.e, defnames::OBJETS, "ANNEAU_GRAVE");
        let s = self
            .e
            .anim
            .slots
            .iter()
            .rfind(|s| s.id == ring)
            .expect("the ring's slot");
        (s.frame, s.playing)
    }

    /// Turns the view with the mouse, as a player does, toward (`yaw`,
    /// `pitch`) until `on` holds with the turn spent (a move of d mickeys
    /// turns the view by about d / 500 radians in all).
    fn steer(&mut self, yaw: f32, pitch: f32, on: &dyn Fn(&Engine) -> bool) -> bool {
        let wrap = |a: f32| {
            let t = std::f32::consts::TAU;
            (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
        };
        let (mut sx, mut sy) = self.mouse_signs;
        for k in 0..24 {
            if k > 0 && on(&self.e) {
                self.wait(24);
                if on(&self.e) {
                    self.mouse_signs = (sx, sy);
                    return true;
                }
            }
            let (y0, p0) = self.e.camera.angles();
            let (dy, dp) = (wrap(yaw - y0), pitch - p0);
            let mx = (sx * dy * 500.0).round() as i32;
            let my = (sy * dp * 500.0).round() as i32;
            if mx == 0 && my == 0 {
                break;
            }
            self.e.mouse_motion(mx, my);
            self.wait(30);
            let (y1, p1) = self.e.camera.angles();
            if mx != 0 && wrap(yaw - y1).abs() > dy.abs() {
                sx = -sx;
            }
            if my != 0 && (pitch - p1).abs() > dp.abs() {
                sy = -sy;
            }
        }
        self.mouse_signs = (sx, sy);
        self.wait(24);
        on(&self.e)
    }

    /// The view turned with the mouse onto zone `zone` (the centre on it,
    /// no clickable sprite under the centre), a little off its aim as a
    /// player's hand is; false when it cannot be reached.
    fn try_aim(&mut self, zone: usize) -> bool {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        let on = move |e: &Engine| {
            e.centre_zone().map(|(i, _)| i) == Some(zone) && e.sprite_under_centre().1 == 0
        };
        let off = 0.6f32.to_radians();
        let mut targets = vec![
            (yaw + off, pitch),
            (yaw - off, pitch),
            (yaw, pitch + off),
            (yaw, pitch - off),
        ];
        let fpu = self.e.fpu();
        for p in &self.e.wam().view(view).unwrap().zones[zone].points {
            let (y, q) = atlantis2_engine::camera::direction_angles(fpu, *p);
            let dy = (y - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            targets.push((yaw + dy / 2.0, (pitch + q) / 2.0));
        }
        targets.into_iter().any(|(y, p)| self.steer(y, p, &on))
    }

    /// The zone of the current view named `name`.
    fn zone(&self, name: &str) -> usize {
        let view = self.e.place().1;
        self.e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("no zone {name} in view {view}"))
    }

    /// The running dialog played to its end, a click every 20 passes.
    fn listen(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        for n in 0..20_000 {
            if n % 20 == 19 {
                self.tap();
            }
            let evs = self.events();
            let end = evs.contains(&Event::DialogEnd);
            all.extend(evs);
            if end {
                self.wait(4);
                all.extend(std::mem::take(&mut self.e.events));
                return all;
            }
        }
        panic!("the dialog does not end");
    }

    /// Passes until the panorama is back and still, a film clicked away
    /// after a while, a dialog listened to.
    fn back_to_panorama(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        let mut still = 0;
        for _ in 0..6000 {
            let evs = self.events();
            let film = evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            });
            all.extend(evs);
            if film {
                self.wait(30);
                self.tap();
            }
            if self.e.loop_type == LoopType::Dialog {
                all.extend(self.listen());
            }
            if self.e.loop_type == LoopType::Warp && !self.e.turning() {
                still += 1;
                if still == 8 {
                    all.extend(std::mem::take(&mut self.e.events));
                    return all;
                }
            } else {
                still = 0;
            }
        }
        panic!("the panorama does not come back");
    }

    /// A click on a move zone (types 1, 2) of the current view to view
    /// `to`, turned to with the mouse; the films and dialogs as they come.
    fn walk(&mut self, to: u32) -> Vec<Event> {
        self.wait(15);
        let view = self.e.place().1;
        let zones: Vec<usize> = self
            .e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| matches!(z.kind, 1 | 2) && z.value == to as i32)
            .map(|(i, _)| i)
            .collect();
        assert!(!zones.is_empty(), "no move from view {view} to {to}");
        assert!(
            zones.iter().any(|&z| self.try_aim(z)),
            "no move zone to view {to} within reach in view {view}"
        );
        self.tap();
        let mut all = Vec::new();
        for _ in 0..3000 {
            let evs = self.events();
            let arrived = evs.iter().any(|e| matches!(e, Event::ArrivalScan(_)));
            let film = evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            });
            all.extend(evs);
            if film {
                self.wait(15);
                self.tap();
            }
            if arrived {
                all.extend(self.back_to_panorama());
                return all;
            }
        }
        panic!("no arrival in view {to}");
    }

    /// A click on the zone named `name` of the current view, turned to
    /// with the mouse: the events of that pass.
    /// [`Run::click_zone`] a pass after the aim, and whether the 15 Hz
    /// task ran in that pass: its flag is then up in the click's frame
    /// (the task runs after the frame, its flags read by the next).
    fn click_zone_ticks(&mut self, name: &str) -> (Vec<Event>, i32) {
        let zone = self.zone(name);
        assert!(self.try_aim(zone), "zone {name} out of reach");
        let t = self.e.ticks;
        self.step();
        let n = self.e.ticks.wrapping_sub(t) as i32;
        self.tap();
        (self.events(), n)
    }

    fn click_zone(&mut self, name: &str) -> Vec<Event> {
        let zone = self.zone(name);
        assert!(self.try_aim(zone), "zone {name} out of reach");
        self.tap();
        self.events()
    }

    /// Where a player looks to have the sprite of slot `n` under the
    /// centre: texels of its box on each face it is drawn on.
    fn slot_aims(&self, n: usize) -> Vec<(f32, f32)> {
        let s = &self.e.anim.slots[n];
        let mut out = Vec::new();
        for face in 0..6 {
            let Some(b) = s.frame_box(face) else {
                continue;
            };
            if b.w == 0 || b.h == 0 {
                continue;
            }
            let mut texels = Vec::new();
            for i in 1..8 {
                for j in 1..8 {
                    let col = i32::from(b.x) + 1 + (i32::from(b.w) - 1) * i / 8;
                    let row = i32::from(b.y) + 1 + (i32::from(b.h) - 1) * j / 8;
                    let d = (i - 4) * (i - 4) + (j - 4) * (j - 4);
                    texels.push((d, col, row));
                }
            }
            texels.sort();
            for (_, col, row) in texels.into_iter().take(6) {
                out.push(texel_angles(face, f64::from(col), f64::from(row)));
            }
        }
        out
    }

    /// The character `who` turned to with the mouse and clicked, until
    /// his subject menu opens.
    fn click_character(&mut self, who: i32) {
        let n = self
            .e
            .anim
            .slots
            .iter()
            .position(|s| s.id == who && s.kind == 1 && s.active && s.click != 0)
            .expect("the character shown and clickable");
        for (yaw, pitch) in self.slot_aims(n) {
            let on = |e: &Engine| e.sprite_under_centre().1 != 0 && centre_slot(e) == Some(n);
            if self.steer(yaw, pitch, &on) {
                self.tap();
                let evs = self.events();
                if evs
                    .iter()
                    .any(|e| matches!(e, Event::SubjectMenu { character, .. } if *character == who))
                {
                    return;
                }
            }
        }
        panic!("character {who} not clicked");
    }

    /// Asks `who` about subject `name` through his menu (the pointer on
    /// its cell, a click), the answer listened to: the dialogs started.
    fn ask(&mut self, who: i32, name: &str) -> Vec<u32> {
        self.click_character(who);
        let value = self
            .e
            .script_state()
            .unwrap()
            .names
            .param(defnames::SUJETS, name.as_bytes());
        let list = self.e.script_state().unwrap().subjects.list(who);
        let i = list
            .iter()
            .position(|s| s.value == value)
            .unwrap_or_else(|| panic!("subject {name} not offered"));
        let (x, y) = cell(list.len() as i32, i as i32);
        self.point(x + 31, y + 31);
        self.step();
        self.tap();
        let mut evs = self.events();
        evs.extend(self.back_to_panorama());
        evs.iter()
            .filter_map(|e| match e {
                Event::DialogStart {
                    number,
                    started: true,
                } => Some(*number),
                _ => None,
            })
            .collect()
    }

    /// The bag bar opened with the right button and object `id`'s cell
    /// clicked: the object in hand, the bar closed.
    fn pick(&mut self, id: i32) {
        self.e.right_click();
        assert!(self.e.bar.open);
        self.step();
        let mut right = true;
        for _ in 0..400 {
            let bag = self.e.bag_entries();
            if let Some(k) = self.e.bar.shown.iter().position(|&n| bag[n].id == id) {
                self.point(40 + 80 * k as i32, 440);
                self.step();
                self.tap();
                self.step();
                assert!(!self.e.bar.open);
                assert_eq!(self.var("OBJETENMAIN"), id);
                return;
            }
            let first = self.e.bar.first;
            if right {
                self.point(636, 440);
            } else {
                self.point(1, 440);
            }
            self.wait(8);
            if self.e.bar.first == first {
                assert!(right, "object {id} not in the bag");
                right = false;
            }
        }
        panic!("object {id} never shown in the bar");
    }
}

/// The angles of the direction through texel (`col`, `row`) of cube face
/// `face` (the warp's sampling inverted).
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

/// The clickable slot under the screen centre (the warp's sampling of the
/// centre ray).
fn centre_slot(e: &Engine) -> Option<usize> {
    let [x, y, z] = e.camera.ray(e.camera.cx, e.camera.cy).map(f64::from);
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    let (face, u, v) = if az >= ax && az >= ay {
        if z > 0.0 {
            (0, x / az, y / az)
        } else {
            (1, -x / az, y / az)
        }
    } else if ay >= ax {
        if y < 0.0 {
            (2, x / ay, z / ay)
        } else {
            (3, x / ay, -z / ay)
        }
    } else if x < 0.0 {
        (4, z / ax, y / ax)
    } else {
        (5, -z / ax, y / ax)
    };
    let texel = |s: f64| (1.0 + 510.0 * (s + 1.0) / 2.0).floor() as i32;
    let (col, row) = (texel(u), texel(v));
    e.anim.slots.iter().position(|s| {
        s.active
            && s.click > 0
            && s.frame_box(face).is_some_and(|b| {
                let (bx, by, bw, bh) = (
                    i32::from(b.x),
                    i32::from(b.y),
                    i32::from(b.w),
                    i32::from(b.h),
                );
                row < by + bh + 1 && by < row && bx < col && col < bx + bw + 1
            })
    })
}

/// The views of the room a move leads to from `view` (its move zones).
fn neighbours(e: &Engine, view: u32) -> Vec<u32> {
    e.wam()
        .view(view)
        .map(|v| {
            v.zones
                .iter()
                .filter(|z| z.kind == 1)
                .map(|z| z.value as u32)
                .collect()
        })
        .unwrap_or_default()
}

/// A walk of exactly `n` moves from `from` ending in `to` that does not
/// enter `to` before (breadth first over the room's moves).
fn walk_of(e: &Engine, from: u32, to: u32, n: usize) -> Option<Vec<u32>> {
    let mut queue = VecDeque::from([vec![from]]);
    while let Some(path) = queue.pop_front() {
        let last = *path.last().unwrap();
        if path.len() == n + 1 {
            if last == to {
                return Some(path[1..].to_vec());
            }
            continue;
        }
        if last == to && path.len() > 1 {
            continue;
        }
        for v in neighbours(e, last) {
            let mut p = path.clone();
            p.push(v);
            queue.push_back(p);
        }
    }
    None
}

#[test]
fn the_turtle_room_played_with_the_mouse_to_chapter_47() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    // `chine.wam` view 32 in chapter 43, as the gardener master's dialog
    // 223 leaves the story (chapter 42 lines 57-58: `flag133`, the turtle
    // master shown).
    for c in [
        &b"(monde=3)"[..],
        b"(changewam=chine.wam)",
        b"(vue=32)",
        b"(chapter=43)",
        b"(flag133=1)",
        b"(showperso=MAITRE_TORTUE)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    e.trace = true;
    let shots = std::env::var_os("ATLANTIS2_SHOTS").map(PathBuf::from);
    if let Some(d) = &shots {
        std::fs::create_dir_all(d).unwrap();
    }
    let mut r = Run {
        e,
        pass: 0,
        gap: 0.0,
        screen: vec![0; 640 * 480],
        shots,
        mouse_signs: (1.0, -1.0),
    };
    r.wait(20);
    assert_eq!(r.e.place(), ("chine.wam", 32));
    assert_eq!(r.e.puzzle_mode(), 0);
    let turtle = id(&r.e, defnames::OBJETS, "TORTUE_NOIRE");
    let ring = id(&r.e, defnames::OBJETS, "ANNEAU_GRAVE");
    let master = id(&r.e, defnames::PERSONNAGES, "MAITRE_TORTUE");

    // Into the room: chapter 43's departure line sends the game to
    // chapter 46; the first arrival in view 40 is the meeting (dialog 260,
    // lines 84-89): the turtle and the ring in the bag, the turtle in hand.
    let evs = r.walk(40);
    assert_eq!(r.e.place(), ("salletortue.wam", 40));
    assert_eq!(r.e.script_state().unwrap().chapter, 46);
    assert_eq!(arrival_lines(&evs, 46), [84, 85, 86, 87, 88, 89]);
    assert!(evs.contains(&Event::DialogStart {
        number: 260,
        started: true
    }));
    let bag: Vec<i32> = r.e.bag_entries().iter().map(|b| b.id).collect();
    assert!(bag.contains(&turtle) && bag.contains(&ring));
    assert_eq!(r.flag(134), 1);
    // The ring's entry: the mode 2, the ring held at frame 0.
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_c340
    }));
    assert_eq!(r.e.puzzle_mode(), RING_MODE);
    assert_eq!(r.ring(), (0, false));
    r.wait(30);
    assert_eq!(r.ring(), (0, false));
    r.shot("1-view-40");

    // To view 56, where the ring turns.
    for to in [55, 56] {
        r.walk(to);
    }
    assert_eq!(r.e.place(), ("salletortue.wam", 56));
    r.wait(10);
    r.shot("2-view-56");
    // A quarter to the left: sound 218, nine frames one a tick (from the
    // tick of the click's pass, when one falls in it).
    let (evs, n) = r.click_zone_ticks("TORTUE_GAUCHE");
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_c290
    }));
    assert_eq!(sounds(&evs), "[sound218.apc]");
    assert_eq!(r.e.ring_count(), 9 - n);
    // A click during the turn (the centre still on the zone) is lost.
    r.tap();
    let evs = r.events();
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_c290
    }));
    assert_eq!(sounds(&evs), "[]");
    assert!((1..=9).contains(&r.e.ring_count()));
    r.wait(60);
    assert_eq!(
        (r.ring(), r.e.ring_count(), r.e.ring_store()),
        ((9, false), 0, 9)
    );
    r.shot("3-ring-left-1");
    let (evs, n) = r.click_zone_ticks("TORTUE_GAUCHE");
    assert_eq!(sounds(&evs), "[sound218.apc]");
    assert_eq!(r.e.ring_count(), 10 - n);
    // Half way: one frame per tick.
    r.wait(20);
    let (mid, _) = r.ring();
    assert!(mid > 9 && mid < 19, "{mid}");
    r.shot("4-ring-turning");
    r.wait(60);
    assert_eq!(r.ring(), (19, false));
    r.shot("5-ring-left-2");
    // A quarter to the right: back to 9 (−10 from 19).
    let (evs, n) = r.click_zone_ticks("TORTUE_DROITE");
    assert_eq!(sounds(&evs), "[sound218.apc]");
    assert_eq!(r.e.ring_count(), -10 + n);
    r.wait(60);
    assert_eq!(r.ring(), (9, false));
    // The frame kept in the next view.
    r.walk(57);
    assert_eq!(r.ring(), (9, false));
    r.shot("6-view-57-ring-kept");

    // The turtle laid on the water (lines 187-190): shown, out of the bag,
    // the master's subject `TORTUE`.
    r.pick(turtle);
    let evs = r.click_zone("EAU_TORTUE");
    assert_eq!(lines(&evs, 46), [187, 188, 189, 190]);
    assert!(!r.e.bag_entries().iter().any(|b| b.id == turtle));
    r.wait(10);
    r.shot("7-turtle-laid");

    // The turtle master asked about the tiger's home until he gives his
    // indications (dialog 266, `indication_tortue` = 1).
    let mut heard = Vec::new();
    for _ in 0..6 {
        if r.var("indication_tortue") == 1 {
            break;
        }
        heard.extend(r.ask(master, "DEMEURE_TIGRE"));
    }
    assert_eq!(r.var("indication_tortue"), 1, "{heard:?}");
    assert_eq!(heard.last(), Some(&266));
    r.shot("8-master-asked");

    // His indications, one an arrival (dialogs 268, 269, 270, 271, 272,
    // 274), the seventh arrival silent (`indication_tortue` 9), the eighth
    // in view 47: dialog 276, film 22, chapter 47 in `miniature.wam`. (The
    // room's moves make a two-coloured graph: from view 56 every walk to
    // 47 is odd, so the master is asked here, in view 57.)
    assert!(walk_of(&r.e, 56, 47, 8).is_none());
    let path = walk_of(&r.e, 57, 47, 8).expect("a walk of eight moves");
    let mut dialogs = Vec::new();
    for (k, &to) in path.iter().enumerate() {
        let evs = r.walk(to);
        dialogs.extend(evs.iter().filter_map(|e| match e {
            Event::DialogStart {
                number,
                started: true,
            } => Some(*number),
            _ => None,
        }));
        if k == 6 {
            assert_eq!(r.var("indication_tortue"), 9);
            // The ring still where it was left.
            assert_eq!(r.ring(), (9, false));
        }
        if k == 7 {
            assert_eq!(
                arrival_lines(&evs, 46),
                [169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181]
            );
            assert!(evs.iter().any(|e| matches!(
                e,
                Event::VideoStart { name, frames: Some(_) } if name.eq_ignore_ascii_case("cine022.hnm")
            )));
        }
    }
    assert_eq!(dialogs, [268, 269, 270, 271, 272, 274, 276]);
    r.wait(10);
    assert_eq!(r.e.place(), ("miniature.wam", 1));
    assert_eq!(r.e.script_state().unwrap().chapter, 47);
    assert_eq!((r.flag(136), r.flag(141)), (1, 1));
    assert_eq!(r.var("indication_tortue"), 11);
    // Out of the room the ring's mode is gone (the original's is 5, the
    // chest's entry's in view 1: on `a2/pz-chest`, not here).
    assert_ne!(r.e.puzzle_mode(), RING_MODE);
    r.shot("9-miniature-chapter-47");
}

#[test]
fn the_ring_is_kept_by_a_save() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_46(files, 56);
    let (gauche, _) = zones(&e);
    e.turtle_click(56, gauche);
    for _ in 0..4 {
        e.frame_flag = true;
        e.turtle_tick(56);
    }
    assert_eq!((e.ring_count(), e.ring_store()), (5, 4));
    let d = e.save_data();
    for _ in 0..5 {
        e.frame_flag = true;
        e.turtle_tick(56);
    }
    assert_eq!(e.ring_store(), 9);
    e.load_data(&d).unwrap();
    settle(&mut e);
    assert_eq!(e.place(), ("salletortue.wam", 56));
    assert_eq!(e.puzzle_mode(), RING_MODE);
    let ring = id(&e, defnames::OBJETS, "ANNEAU_GRAVE");
    let s = e.anim.slots.iter().rfind(|s| s.id == ring).unwrap();
    // The view entered again at the load: held at the saved frame, and
    // the rest of the turn goes on.
    assert!(!s.playing);
    assert!(s.frame >= 4 && s.frame <= 9);
    for _ in 0..10 {
        e.tick();
        e.pass(None);
    }
    assert_eq!((e.ring_count(), e.ring_store()), (0, 9));
}

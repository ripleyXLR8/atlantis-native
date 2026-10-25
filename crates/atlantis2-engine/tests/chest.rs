//! The Chinese chest (`miniature.wam` views 1-7, chapter 47) against the
//! original (`tests/golden/chest.txt`, its header there): the view entry
//! hook (rows B), the click hook alone (C, through
//! [`Engine::chest_click`]), the click dispatcher's choice and the puzzle
//! mode it clears (D), the entry dispatcher's (E), the zone click and the
//! click scan of the same frame on chapter 47 (F: the chest opened, the
//! chopstick taken, then the incense sticks lit with it to chapter 48),
//! the save entry of the mode (G), the chest's and the chopstick's slots
//! drawn (H: the faces' md5s and sampled pixels) and the 15 Hz step that
//! leaves the held chest still (I).
//!
//! Then the puzzle played as a player plays it, with the mouse: from
//! chapter 48 (view 7) into chapter 47, to view 5, the chest opened and
//! the chopstick taken, back to view 6 and the four incense sticks lit
//! with it (chapter 47's lines it unlocks) to the film and chapter 48;
//! and the state kept by a save. With `ATLANTIS2_SHOTS` set to a folder,
//! the played test writes `pz-chest-*.png` there.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::spw::Spw;
use atlantis2_data::wam::EntityState;
use atlantis2_data::{A2Files, Folder};

use atlantis2_engine::anim::Anim;
use atlantis2_engine::engine::chinese_entry_hooks;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{CursorBank, Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/chest.txt");

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
/// The number of films and dialogs ended.
fn settle(e: &mut Engine) -> usize {
    let mut resumes = 0;
    for _ in 0..400 {
        match e.loop_type {
            LoopType::Video => {
                e.end_video();
                resumes += 1;
            }
            LoopType::Dialog => {
                e.end_dialog();
                resumes += 1;
            }
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Warp && !e.turning() {
            break;
        }
    }
    assert_eq!(e.loop_type, LoopType::Warp);
    resumes
}

/// A game in `miniature.wam`, chapter 47, view `view`, as the scripts put
/// it there (the world, the place, the view, the chapter).
fn in_chapter_47(files: A2Files, view: u32) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=3)"[..],
        b"(changewam=miniature.wam)",
        format!("(vue={view})").as_bytes(),
        b"(chapter=47)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("miniature.wam", view));
    assert_eq!(e.script_state().unwrap().chapter, 47);
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

/// The golden's fresh state: variables at 0 but `MONDE` = `monde`,
/// `frame_coffre_chine` = `v`, `BAGUETTE_PRISE` = `prise`, no hand, an
/// empty bag, no show / hide row of the chopstick, the mode `mode`; then
/// view `view` entered (its slots loaded, the entry hooks run).
fn fresh(e: &mut Engine, view: u32, monde: i32, v: i32, prise: i32, mode: i32) {
    settle(e);
    e.run_command(b"(objetenmain=0)");
    let chopstick = id(e, defnames::OBJETS, "BAGUETTE");
    let s = e.script_state_mut().unwrap();
    s.bag.clear();
    s.shown.retain(|r| r.id != chopstick);
    for (k, x) in [
        ("MONDE", monde),
        ("frame_coffre_chine", v),
        ("BAGUETTE_PRISE", prise),
        ("ClicZone", 0),
    ] {
        assert!(s.set_var(k, x));
    }
    s.warnings.clear();
    e.anim.table.0.retain(|r| r.0 != chopstick);
    e.set_puzzle_mode(mode);
    e.events.clear();
    e.enter_at(view, 0.0, 0.0).unwrap();
    e.cursor.shown = None;
}

/// The first slot of object `name` (any kind), as the golden writes it.
fn slot(e: &Engine, name: &str) -> String {
    let i = id(e, defnames::OBJETS, name);
    e.anim
        .slots
        .iter()
        .find(|s| s.id == i)
        .map_or("-".into(), |s| {
            format!(
                "f{}/p{}/a{}",
                s.frame,
                i32::from(s.playing),
                i32::from(s.active)
            )
        })
}

/// The golden's state.
fn st(e: &Engine) -> String {
    let s = e.script_state().unwrap();
    let chopstick = id(e, defnames::OBJETS, "BAGUETTE");
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
    let bag = lst(s
        .bag
        .iter()
        .map(|b| format!("{}/{}/{}", b.id, b.sprite, b.world)));
    format!(
        "var {} prise {} mode {} chest {} chopstick {} table {} hand {} pair {pair} cursor {cursor} bag {bag}",
        var(e, "frame_coffre_chine"),
        var(e, "BAGUETTE_PRISE"),
        e.puzzle_mode(),
        slot(e, "COFFRE_CHINE"),
        slot(e, "BAGUETTE"),
        e.anim.table.get(chopstick, 2),
        var(e, "OBJETENMAIN"),
    )
}

/// The `NEWSOUND`s voice 2 starts (`sound%03d.apc`), in order; not a
/// film's track (the golden records `AtlSound_NewSound` only).
fn sounds(evs: &[Event]) -> String {
    lst(evs.iter().filter_map(|e| match e {
        Event::Sound(n) if n.starts_with("sound") => Some(n.clone()),
        _ => None,
    }))
}

/// The films started.
fn films(evs: &[Event]) -> String {
    lst(evs.iter().filter_map(|e| match e {
        Event::VideoStart {
            name,
            frames: Some(_),
        } => Some(name.clone()),
        _ => None,
    }))
}

fn lines(evs: &[Event]) -> Vec<usize> {
    evs.iter()
        .filter_map(|e| match e {
            Event::Set { chapter: 47, line } => Some(*line),
            _ => None,
        })
        .collect()
}

/// The warnings (`FUN_00480c20`, their format strings) and the fatal
/// error since the last call, as the golden writes them; both cleared.
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

/// The golden's names of the hooks the dispatchers reach.
fn hook_name(f: u32) -> String {
    match f {
        0x0048_da20 => "figurines-click".into(),
        0x0048_d680 => "figurines-entry".into(),
        0x0048_d170 => "wheels-click".into(),
        0x0048_d080 => "wheels-entry".into(),
        0x0049_4600 => "clouds-click".into(),
        0x0049_44a0 => "clouds-entry".into(),
        0x0049_c290 => "ring-click".into(),
        0x0049_c340 => "ring-entry".into(),
        0x0048_df10 => "souls-entry".into(),
        f => format!("{f:08x}"),
    }
}

#[test]
fn the_entry_and_click_hooks_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_47(files, 5);
    let names = e.script_state().unwrap().names.clone();
    let obj = |n: &str| names.id(defnames::OBJETS, n.as_bytes()).unwrap();
    let zone = |n: &str| names.id(defnames::ZONES_CLIC, n.as_bytes()).unwrap();
    let vars = |n: &str| names.id(defnames::VARIABLES, n.as_bytes()).unwrap();
    assert_eq!(
        rows("ids objects")[0],
        format!(
            "ids objects {} {} {} param {} {} zone {} {} {} {} {}",
            obj("COFFRE_CHINE"),
            obj("BAGUETTE"),
            obj("PIECE5"),
            names.param(defnames::OBJETS, b"BAGUETTE"),
            names.param(defnames::OBJETS, b"PIECE5"),
            zone("PRENDRE_BAGUETTE"),
            zone("ENCENS1"),
            zone("ENCENS2"),
            zone("ENCENS3"),
            zone("ENCENS4")
        )
    );
    assert_eq!(
        rows("ids variables")[0],
        format!(
            "ids variables {} {} {} {} {}",
            vars("frame_coffre_chine"),
            vars("BAGUETTE_PRISE"),
            vars("OBJETENMAIN"),
            vars("MONDE"),
            vars("ClicZone")
        )
    );
    let take = zone("PRENDRE_BAGUETTE") as i32;

    // B: the entry hook, run by the view's entry on its slots as loaded.
    // Views 0 and 8 have no chest slots in `miniature.wam`: the golden
    // runs the hook alone on view 5's slots under those numbers (its own
    // view test), and so does the port (the dispatchers never call it
    // there, rows E).
    let mut got = Vec::new();
    let mut want = Vec::new();
    for row in rows("B view") {
        let w: Vec<&str> = row.split(' ').collect();
        let (view, v, mode): (u32, i32, i32) = (
            w[2].parse().unwrap(),
            w[4].parse().unwrap(),
            w[6].parse().unwrap(),
        );
        if (1..=7).contains(&view) {
            fresh(&mut e, view, 3, v, 0, mode);
            assert!(e.events.contains(&Event::PuzzleRan {
                function: 0x0049_38f0
            }));
        } else {
            // View 5's slots as loaded (the entry undone), the hook alone.
            fresh(&mut e, 5, 3, v, 0, mode);
            e.reload_animations();
            e.set_puzzle_mode(mode);
            e.chest_entry_as(view);
        }
        got.push(format!(
            "B view {view} var {v} mode {mode} -> {} | warn {}",
            st(&e),
            warns(&mut e)
        ));
        want.push(row.to_string());
    }
    same(&got, &want);
    // No slot of the chest: the fatal search.
    fresh(&mut e, 5, 3, 0, 0, 0);
    let chest = obj("COFFRE_CHINE") as i32;
    e.anim.slots.retain(|s| s.id != chest);
    e.chest_entry();
    assert_eq!(
        format!("B no slot of the chest, view 5 -> warn {}", warns(&mut e)),
        rows("B no slot")[0]
    );

    // C: the click hook alone.
    let mut got = Vec::new();
    fn c(e: &mut Engine, label: &str, v: i32, t: i32, got: &mut Vec<String>) {
        e.events.clear();
        e.chest_click(v);
        let evs = std::mem::take(&mut e.events);
        got.push(format!(
            "C {label} zone {t} {v} -> {} | sounds {} | warn {}",
            st(e),
            sounds(&evs),
            warns(e)
        ));
    }
    fresh(&mut e, 5, 3, 0, 0, 0);
    got.push(format!("C entered 5 -> {}", st(&e)));
    c(&mut e, "move", 4, 1, &mut got);
    for k in 1..7 {
        c(&mut e, &format!("take {k}"), take, 3, &mut got);
    }
    c(&mut e, "other value", 23, 3, &mut got);
    fresh(&mut e, 5, 3, 0, 0, 0);
    for t in [4, 0x32, 1, 0] {
        c(&mut e, "type", take, t, &mut got);
    }
    for v in [1, 3, 6, 7] {
        fresh(&mut e, v, 3, 2, 0, 0);
        c(&mut e, &format!("view {v}"), take, 3, &mut got);
    }
    fresh(&mut e, 5, 3, 4, 0, 0);
    e.run_command(b"(sacados=BAGUETTE)");
    got.push(format!("C given -> {}", st(&e)));
    c(&mut e, "in bag", take, 3, &mut got);
    fresh(&mut e, 5, 3, 4, 0, 0);
    e.run_command(b"(sacados=PIECE5)");
    e.run_command(b"(objetenmain=PIECE5)");
    got.push(format!("C held -> {}", st(&e)));
    c(&mut e, "piece in hand", take, 3, &mut got);
    fresh(&mut e, 5, 3, 4, 1, 0);
    c(&mut e, "prise 1", take, 3, &mut got);
    for v in [3, 5, 7, -1, -3] {
        fresh(&mut e, 5, 3, v, 0, 0);
        c(&mut e, &format!("var {v}"), take, 3, &mut got);
    }
    fresh(&mut e, 5, 5, 4, 0, 0);
    c(&mut e, "MONDE 5", take, 3, &mut got);
    // No chopstick slot in view 5: the fatal search at the click.
    fresh(&mut e, 5, 3, 0, 0, 0);
    let chopstick = obj("BAGUETTE") as i32;
    e.anim.slots.retain(|s| s.id != chopstick);
    got.push(format!("C no chopstick slot -> warn {}", warns(&mut e)));
    e.chest_click(take);
    got.push(format!(
        "C no chopstick slot zone 3 {take} -> warn {}",
        warns(&mut e)
    ));
    // The chopstick shown, other views entered (the table), taken.
    fresh(&mut e, 5, 3, 3, 0, 0);
    c(&mut e, "show", take, 3, &mut got);
    for v in [1, 6, 5] {
        e.enter_at(v, 0.0, 0.0).unwrap();
        got.push(format!("C entered {v} -> {}", st(&e)));
    }
    c(&mut e, "take", take, 3, &mut got);
    for v in [1, 5] {
        e.enter_at(v, 0.0, 0.0).unwrap();
        got.push(format!("C entered {v} -> {}", st(&e)));
    }
    same(&got, &rows("C "));

    // D: the engine's click dispatcher (its Chinese part) on the golden's
    // warp name, view number, zone type and mode: the hooks it reaches
    // (the chest's run, the others noted), what it returns and the mode
    // after. The zone's value is no chest zone, so the chest's click only
    // holds the chest. FUN_004945f0 (the clouds' move zones) sets the
    // mode to 0 (`a2/pz-dragon`): its one row has the mode 0 before.
    fresh(&mut e, 5, 3, 0, 0, 0);
    let mut got = Vec::new();
    for row in rows("D ") {
        let w: Vec<&str> = row.split(' ').collect();
        let (name, number, kind, mode): (&str, u32, i32, i32) = (
            w[1],
            w[2].parse().unwrap(),
            w[3].parse().unwrap(),
            w[5].parse().unwrap(),
        );
        let z = atlantis2_data::wam::Zone {
            kind,
            value: 0,
            name: None,
            points: Vec::new(),
        };
        e.set_puzzle_mode(mode);
        e.events.clear();
        let al = e.chinese_click(0, name.as_bytes(), number, &z);
        let hooks: Vec<String> = e
            .events
            .iter()
            .filter_map(|x| match x {
                Event::PuzzleRan { function } | Event::PuzzleNotPorted { function } => {
                    Some(hook_name(*function))
                }
                _ => None,
            })
            .collect();
        let hooks = if hooks.is_empty() {
            "-".to_string()
        } else {
            hooks.join(",")
        };
        got.push(format!(
            "D {name} {number} {kind} mode {mode} -> {hooks} AL {} mode {}",
            i32::from(al),
            e.puzzle_mode()
        ));
    }
    same(&got, &rows("D "));
    // The engine's dispatcher on a real click: the mode 5 cleared by the
    // first click in view 13, which runs no hook.
    fresh(&mut e, 5, 3, 0, 0, 0);
    assert_eq!(e.puzzle_mode(), 5);

    // E: the entry dispatcher's choice; the mode after (the others'
    // entries stubbed in the golden); the chest's slot for the views
    // where the chest's entry runs.
    let mut got = Vec::new();
    let mut want = Vec::new();
    for row in rows("E ") {
        let w: Vec<&str> = row.split(' ').collect();
        let (name, number, mode): (&str, u32, i32) =
            (w[1], w[2].parse().unwrap(), w[4].parse().unwrap());
        let hooks = chinese_entry_hooks(name.as_bytes(), number);
        let chest = hooks.contains(&0x0049_38f0);
        let after = if chest { 5 } else { mode };
        let names = if hooks.is_empty() {
            "-".to_string()
        } else {
            hooks
                .iter()
                .map(|&f| hook_name(f))
                .collect::<Vec<_>>()
                .join(",")
        };
        let mut line = format!("E {name} {number} mode {mode} -> {names} mode {after}");
        if chest {
            fresh(&mut e, number, 3, 1, 0, mode);
            assert_eq!(e.puzzle_mode(), after);
            line += &format!(
                " chest {} | warn {}",
                slot(&e, "COFFRE_CHINE"),
                warns(&mut e)
            );
            want.push(row.to_string());
        } else {
            want.push(row.split(" chest ").next().unwrap().to_string());
        }
        got.push(line);
    }
    same(&got, &want);

    // G: the save entry of the mode.
    assert_eq!(
        format!(
            "G save entry 26 = 0x{:08x} {}",
            atlantis2_data::save::ENTRIES[26].0,
            atlantis2_data::save::ENTRIES[26].1
        ),
        rows("G ")[0]
    );

    // I: the 15 Hz step on view 5's slots after the entry.
    let mut got = Vec::new();
    for (v, playing) in [(0, None), (4, None), (2, Some(true))] {
        fresh(&mut e, 5, 3, v, 0, 0);
        let n = e.anim.slots.iter().position(|s| s.id == chest).unwrap();
        if let Some(p) = playing {
            e.anim.slots[n].playing = p;
        }
        let seq: Vec<String> = (0..7)
            .map(|_| {
                e.anim.step();
                slot(&e, "COFFRE_CHINE")
            })
            .collect();
        got.push(format!(
            "I var {v} playing {} -> {}",
            playing.map_or("hook".to_string(), |p| i32::from(p).to_string()),
            seq.join(" ")
        ));
    }
    same(&got, &rows("I "));
}

/// The engine's dispatcher on real clicks: in view 13 after views 1-7 the
/// first click clears the mode and reaches no hook; the next one does.
#[test]
fn the_mode_of_the_chest_eats_one_click_outside_views_1_to_7() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_47(files, 5);
    assert_eq!(e.puzzle_mode(), 5);
    e.enter_at(13, 0.0, 0.0).unwrap();
    assert_eq!(e.puzzle_mode(), 5);
    let z = e
        .wam()
        .view(13)
        .unwrap()
        .zones
        .iter()
        .position(|z| z.name.as_deref() == Some("AXE_5ROUES1"))
        .unwrap();
    let (yaw, pitch) = e.zone_aim(13, z).unwrap();
    e.camera.enter(yaw, pitch);
    e.pass(None);
    let reached = |evs: &[Event]| {
        evs.iter()
            .filter(|x| matches!(x, Event::PuzzleRan { .. } | Event::PuzzleNotPorted { .. }))
            .count()
    };
    e.events.clear();
    e.click(10.0);
    e.pass(None);
    assert_eq!(reached(&e.events), 0);
    assert_eq!(e.puzzle_mode(), 0);
    // `CLICZONE` and the click scan go on.
    assert!(e.events.iter().any(|x| matches!(x, Event::ClickScan(_))));
    e.events.clear();
    e.click(20.0);
    e.pass(None);
    // The wheels' click runs (`a2/pz-wheels`).
    assert!(e.events.contains(&Event::PuzzleRan {
        function: 0x0048_d170
    }));
    // The mode is kept by a save.
    e.enter_at(5, 0.0, 0.0).unwrap();
    let d = e.save_data();
    assert_eq!(d.entry(26), 5i32.to_le_bytes());
    e.set_puzzle_mode(0);
    e.load_data(&d).unwrap();
    assert_eq!(e.puzzle_mode(), 5);
}

/// F: the click as the game runs it (the handler, the hook, `CLICZONE`,
/// the click scan of the same frame on chapter 47, then the section a
/// film suspended resumed at its end).
#[test]
fn the_clicks_and_chapter_47_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_47(files, 5);
    fresh(&mut e, 5, 3, 0, 0, 0);
    let mut got = Vec::new();
    let mut t = 0.0;
    let mut click = |e: &mut Engine, label: &str, view: u32, name: &str, got: &mut Vec<String>| {
        let z = e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some(name))
            .unwrap();
        let value = e.wam().view(view).unwrap().zones[z].value;
        let (yaw, pitch) = e.zone_aim(view, z).unwrap();
        e.camera.enter(yaw, pitch);
        e.pass(None);
        assert_eq!(e.centre_zone().map(|(i, _)| i), Some(z));
        e.events.clear();
        t += 100.0;
        e.click(t);
        e.pass(None);
        let evs = std::mem::take(&mut e.events);
        let hooks: Vec<String> = evs
            .iter()
            .filter_map(|x| match x {
                Event::PuzzleRan { function } | Event::PuzzleNotPorted { function } => {
                    Some(hook_name(*function))
                }
                _ => None,
            })
            .collect();
        let zc = evs
            .iter()
            .position(|x| matches!(x, Event::ZoneClick { .. }))
            .expect("the zone click");
        let Event::ZoneClick { cliczone, .. } = evs[zc] else {
            unreachable!()
        };
        let scan = evs.iter().position(|x| matches!(x, Event::ClickScan(_)));
        let state = st(e);
        let state = format!(
            "{} cursor * bag{}",
            state.split(" cursor ").next().unwrap(),
            state.split(" bag").nth(1).unwrap()
        );
        let tail = format!(
            "EtatPzEncens {} flag142 {}",
            var(e, "EtatPzEncens"),
            e.script_state().unwrap().flags[142]
        );
        let warn = warns(e);
        let resumes = settle(e);
        let after = std::mem::take(&mut e.events);
        got.push(format!(
            "F {label} view {view} zone 3 {value} -> out {} | hooks {} sounds {} | cliczone {cliczone} | click scan {} | in {} sounds {} films {} | resumes {resumes} {} chapter {} | {state} | {tail} | warn {warn}",
            lst(lines(&evs[..zc])),
            if hooks.is_empty() { "-".to_string() } else { hooks.join(",") },
            sounds(&evs[..zc]),
            i32::from(scan.is_some()),
            lst(lines(&evs[zc..])),
            sounds(&evs),
            films(&evs),
            lst(lines(&after)),
            e.script_state().unwrap().chapter,
        ));
    };
    for k in 1..7 {
        click(
            &mut e,
            &format!("take {k}"),
            5,
            "PRENDRE_BAGUETTE",
            &mut got,
        );
    }
    e.enter_at(6, 0.0, 0.0).unwrap();
    e.run_command(b"(objetenmain=BAGUETTE)");
    for z in ["ENCENS1", "ENCENS2", "ENCENS3", "ENCENS4"] {
        click(&mut e, z, 6, z, &mut got);
    }
    let blank = |r: &str| {
        let (a, b) = r.split_once(" cursor (").unwrap();
        format!("{a} cursor * bag{}", b.split_once(") bag").unwrap().1)
    };
    let want: Vec<String> = rows("F ").iter().map(|r| blank(r)).collect();
    same(&got, &want);
    assert_eq!(e.place(), ("miniature.wam", 7));
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
fn the_chest_and_the_chopstick_are_drawn_as_the_original() {
    let Some(files) = game() else { return };
    let bg = background();
    let bg_md5 = md5_of(&bg);
    assert!(GOLDEN.contains(&format!("md5 {bg_md5}")));
    let mut got = Vec::new();
    let golden: Vec<&str> = rows("H ")
        .into_iter()
        .filter(|r| !r.starts_with("H errors"))
        .collect();
    for row in golden.iter().filter(|r| r.starts_with("H file ")) {
        let w: Vec<&str> = row.split(' ').collect();
        let name = w[2];
        let data = files.read(Folder::Spw, name).expect("the SPW");
        let spw = Spw::parse(data.clone()).unwrap();
        let n = spw.frame_count();
        got.push(format!(
            "H file {name} md5 {:x} frames {n}",
            Md5::digest(&data)
        ));
        for f in 0..n {
            let mut a = Anim::new(0);
            let state = EntityState {
                letter: 65,
                anim: 1,
                shown: 1,
                single: 1,
                ..Default::default()
            };
            a.load("s", Spw::parse(data.clone()).unwrap(), 2, 85, 0, state);
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
    }
    // The files' names in the golden are in lower case (CD 3's); the
    // engine reads them in any case.
    same(&got, &golden);
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
                dir.join(format!("pz-chest-{name}.png")),
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

    /// A click on the move zone to view `to`; the films clicked away, a
    /// dialog listened to, until the panorama of view `to` is back.
    fn walk(&mut self, to: u32) -> Vec<Event> {
        self.wait(15);
        let view = self.e.place().1;
        let zone = self
            .e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 1 && z.value == to as i32)
            .unwrap_or_else(|| panic!("no move from view {view} to {to}"));
        self.aim(zone);
        self.tap();
        let mut all = Vec::new();
        let mut still = 0;
        for _ in 0..3000 {
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
            all.extend(evs);
            if video {
                self.wait(30);
                self.tap();
            }
            if self.e.loop_type == LoopType::Dialog {
                self.tap();
            }
            if self.e.place().1 == to && self.e.loop_type == LoopType::Warp && !self.e.turning() {
                still += 1;
                if still == 8 {
                    all.extend(std::mem::take(&mut self.e.events));
                    return all;
                }
            } else {
                still = 0;
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

    /// A click on the zone named `name` of the current view, the centre
    /// on it: the events of that pass.
    fn click_zone(&mut self, name: &str) -> Vec<Event> {
        let zone = self.zone(name);
        self.aim(zone);
        self.tap();
        self.events()
    }

    /// The first slot of object `name`, any kind.
    fn slot(&self, name: &str) -> Option<&atlantis2_engine::anim::Slot> {
        let i = id(&self.e, defnames::OBJETS, name);
        self.e.anim.slots.iter().find(|s| s.id == i)
    }
}

#[test]
fn the_chest_played_with_the_mouse_to_the_incense_and_chapter_48() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    // `miniature.wam` as chapter 55 leaves it: view 7, chapter 48 (the
    // bridge crossed from view 10).
    for c in [
        &b"(monde=3)"[..],
        b"(changewam=miniature.wam)",
        b"(vue=7)",
        b"(chapter=48)",
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
    };
    r.wait(20);
    assert_eq!(r.e.place(), ("miniature.wam", 7));
    assert_eq!(r.e.files().current_disc(), 3);
    let chopstick = id(&r.e, defnames::OBJETS, "BAGUETTE");
    // The chest's entry in view 7: the mode 5, the chest held shut.
    assert_eq!(r.e.puzzle_mode(), 5);
    let chest = r.slot("COFFRE_CHINE").unwrap();
    assert_eq!((chest.frame, chest.playing), (4, false));

    // Into view 6: chapter 48's film 34, chapter 47.
    let evs = r.walk(6);
    assert!(evs.iter().any(|e| matches!(
        e,
        Event::VideoStart { name, frames: Some(_) } if name.eq_ignore_ascii_case("cine034.hnm")
    )));
    assert_eq!(r.e.script_state().unwrap().chapter, 47);
    // To view 5: 6 → 3 → 1 → 4 → 5.
    for to in [3, 1, 4, 5] {
        r.walk(to);
    }
    assert_eq!(r.e.place(), ("miniature.wam", 5));
    let take = r.zone("PRENDRE_BAGUETTE");
    r.aim(take);
    r.wait(8);
    r.shot("1-chest-shut");
    // Still shut after many ticks: the entry stopped its play.
    assert_eq!(r.slot("COFFRE_CHINE").unwrap().frame, 4);

    // Four clicks open it one step each, sounds 211 to 214; at the
    // fourth the chopstick shows.
    for k in 1..=4 {
        let evs = r.click_zone("PRENDRE_BAGUETTE");
        assert!(evs.contains(&Event::PuzzleRan {
            function: 0x0049_3960
        }));
        assert_eq!(sounds(&evs), format!("[sound{}.apc]", 210 + k));
        assert_eq!(r.var("frame_coffre_chine"), k);
        assert_eq!(r.slot("COFFRE_CHINE").unwrap().frame, 4 - k);
        assert_eq!(r.slot("BAGUETTE").unwrap().active, k == 4);
        r.wait(8);
        r.shot(&format!("2-open-{k}"));
    }
    // The fifth takes the chopstick: in the bag and in hand,
    // `BAGUETTE_PRISE`, its sprite hidden for good.
    let evs = r.click_zone("PRENDRE_BAGUETTE");
    assert_eq!(sounds(&evs), "[]");
    assert_eq!(r.var("BAGUETTE_PRISE"), 1);
    assert_eq!(r.var("OBJETENMAIN"), chopstick);
    assert!(r.e.bag_entries().iter().any(|b| b.id == chopstick));
    assert!(!r.slot("BAGUETTE").unwrap().active);
    r.wait(2);
    r.shot("3-chopstick-taken");
    // A sixth click: nothing.
    let evs = r.click_zone("PRENDRE_BAGUETTE");
    assert_eq!(sounds(&evs), "[]");

    // Back to view 6: the chest open in every view, the chopstick gone.
    for to in [4, 1, 3, 6] {
        r.walk(to);
        assert_eq!(r.slot("COFFRE_CHINE").unwrap().frame, 0);
        assert!(r.slot("BAGUETTE").is_none_or(|s| !s.active));
    }
    assert_eq!(r.e.script_state().unwrap().chapter, 47);
    r.shot("4-view-6");

    // The four incense sticks lit with the chopstick, in the order the
    // scripts want: chapter 47's lines 44-47, 56-59, 68-71, then 80-87
    // (the film) and 88-89 (view 7, chapter 48).
    r.pick(chopstick);
    for (k, (stick, want)) in [
        ("ENCENS1", vec![44, 45, 46, 47]),
        ("ENCENS2", vec![56, 57, 58, 59]),
        ("ENCENS3", vec![68, 69, 70, 71]),
    ]
    .into_iter()
    .enumerate()
    {
        let evs = r.click_zone(stick);
        assert_eq!(lines(&evs), want, "{stick}");
        assert_eq!(r.var("EtatPzEncens"), k as i32 + 1);
        r.wait(30);
        r.shot(&format!("5-incense-{}", k + 1));
    }
    let evs = r.click_zone("ENCENS4");
    assert_eq!(lines(&evs), [80, 81, 82, 83, 84, 85, 86, 87]);
    assert_eq!(films(&evs), "[cine023.hnm]");
    assert_eq!(r.e.script_state().unwrap().flags[142], 1);
    assert!(!r.e.bag_entries().iter().any(|b| b.id == chopstick));
    r.wait(60);
    r.shot("6-film-23");
    r.tap();
    let mut evs = Vec::new();
    for _ in 0..600 {
        evs.extend(r.events());
        if r.e.loop_type == LoopType::Warp && !r.e.turning() {
            break;
        }
    }
    assert_eq!(lines(&evs), [88, 89]);
    r.wait(10);
    assert_eq!(r.e.place(), ("miniature.wam", 7));
    assert_eq!(r.e.script_state().unwrap().chapter, 48);
    r.shot("7-view-7-chapter-48");
}

#[test]
fn the_chest_is_saved_and_held_again_after_a_load() {
    let Some(files) = game() else { return };
    let mut e = in_chapter_47(files, 5);
    let take = id(&e, defnames::ZONES_CLIC, "PRENDRE_BAGUETTE");
    e.chest_click(take);
    e.chest_click(take);
    let d = e.save_data();
    assert_eq!(d.entry(26), 5i32.to_le_bytes());
    e.chest_click(take);
    e.load_data(&d).unwrap();
    assert_eq!(var(&e, "frame_coffre_chine"), 2);
    assert_eq!(e.puzzle_mode(), 5);
    let s = e
        .anim
        .slots
        .iter()
        .find(|s| s.id == id(&e, defnames::OBJETS, "COFFRE_CHINE"))
        .unwrap();
    assert_eq!((s.frame, s.playing), (2, false));
}

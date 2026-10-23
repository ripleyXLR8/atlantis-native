//! Brigid's cross (Ireland, view 2) against the original
//! (`tests/golden/cross.txt`, its header there): the frame-table functions
//! on synthetic slots (rows A), the view entry hook (B), the click hook
//! alone (C, through [`Engine::cross_click`]), the dispatcher's choice of
//! the hooks (D), the zone click and the click scan of the same frame on
//! chapter 16, with the section the win starts resumed by the film's and
//! the dialog's ends (E), and the cross's slot drawn from its frame table
//! (G: the faces' md5s, sampled pixels, the centre test).
//!
//! Then the puzzle played as a player plays it, with the mouse: from
//! chapter 19 (Liam in his cell gives the cross) to the win in view 2
//! (chapter 16: the goddess's film and dialog), and the goddess's
//! subjects that take the cross away; and the state kept by a save. With
//! `ATLANTIS2_SHOTS` set to a folder, the played test writes
//! `pz-cross-*.png` there.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped (rows A run
//! always).

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::spw::Spw;
use atlantis2_data::wam::EntityState;
use atlantis2_data::{A2Files, Folder};
use std::cell::RefCell;

use atlantis2_engine::anim::Anim;
use atlantis2_engine::engine::irish_click_hook;
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::subjects::cell;
use atlantis2_engine::{CursorBank, Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/cross.txt");

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

/// A frame table up to its end mark, as signed frames.
fn tab(t: &[u8]) -> String {
    lst(t.iter().take_while(|&&b| b != 0xfe).map(|&b| b as i8))
}

thread_local! {
    /// The warnings this test's thread logged.
    static LOGGED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Keeps the warnings in the thread that logs them (each test runs in
/// its own thread).
struct Capture;

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }

    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            LOGGED.with(|l| l.borrow_mut().push(r.args().to_string()));
        }
    }

    fn flush(&self) {}
}

fn capture() {
    static CAPTURE: Capture = Capture;
    if log::set_logger(&CAPTURE).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    LOGGED.with(|l| l.borrow_mut().clear());
}

/// The frame-table warnings logged since the last call, as the golden's
/// tags (the original's format strings, @ 0x005df40c, 0x005df3d4,
/// 0x005df440).
fn warns() -> String {
    let w: Vec<&str> = LOGGED.with(|l| {
        l.borrow_mut()
            .drain(..)
            .filter_map(|m| {
                if m.starts_with("AddSpwToFrameTab impossible for - non trouvé - :") {
                    Some("add-none")
                } else if m.starts_with("AddSpwToFrameTab impossible - Trop d'entrée - for :") {
                    Some("add-full")
                } else if m.starts_with("SubSpwToFrameTab impossible !! for :") {
                    Some("sub")
                } else {
                    None
                }
            })
            .collect()
    });
    lst(w)
}

fn state(single: i32, kind: i32) -> EntityState {
    EntityState {
        letter: 65,
        anim: 1,
        shown: 1,
        single,
        click: kind,
        ..Default::default()
    }
}

/// Rows A: the three slots of the golden (object 27, object 5,
/// character 27), with a file of no frame.
#[test]
fn the_frame_table_functions_as_the_original() {
    let empty = || Spw::parse(vec![0; 4]).unwrap();
    let setup = || {
        let mut a = Anim::new(0);
        a.load("a", empty(), 2, 27, 0, state(0, 0));
        a.load("b", empty(), 2, 5, 0, state(0, 0));
        a.load("c", empty(), 1, 27, 0, state(0, 0));
        a
    };
    let tabs = |a: &Anim| {
        format!(
            "{} {} {}",
            tab(&a.slots[0].frames),
            tab(&a.slots[1].frames),
            tab(&a.slots[2].frames)
        )
    };
    capture();
    let golden = rows("A ");
    let mut got = Vec::new();
    let mut a = setup();
    for row in golden
        .iter()
        .filter(|r| r.starts_with("A add") || r.starts_with("A sub") || r.starts_with("A has"))
    {
        let w: Vec<&str> = row.split(' ').collect();
        let (op, id, f): (&str, i32, i32) = (w[1], w[2].parse().unwrap(), w[3].parse().unwrap());
        let r = match op {
            "add" => a.add_frame(id, f),
            "sub" => a.sub_frame(id, f),
            _ => a.has_frame(id, f),
        };
        let t = tabs(&a);
        got.push(format!(
            "A {op} {id} {f} -> {} | {t} | warn {}",
            i32::from(r),
            warns()
        ));
    }
    // A table filled to its 128 entries, then searched.
    let mut a = setup();
    let rs: Vec<i32> = (0..129)
        .map(|k| i32::from(a.add_frame(5, k % 100)))
        .collect();
    let raw = a.slots[1].frames;
    got.push(format!(
        "A full: 129 adds of k mod 100 on id 5 -> {}..{} | bytes 0-3 {} 126-127 {} | warn {}",
        lst(&rs[..3]),
        lst(&rs[126..]),
        lst(&raw[..4]),
        lst(&raw[126..]),
        warns()
    ));
    for f in [27, 99, 26] {
        let r = i32::from(a.has_frame(5, f));
        let r2 = i32::from(a.sub_frame(5, f));
        let raw = a.slots[1].frames;
        got.push(format!(
            "A full: has 5 {f} -> {r}, sub 5 {f} -> {r2} | bytes 25-28 {} 98-100 {} 126-127 {} | warn {}",
            lst(&raw[25..29]),
            lst(&raw[98..101]),
            lst(&raw[126..]),
            warns()
        ));
    }
    // FUN_0047ca10 (the clear) is not the cross's (the star map uses it):
    // its row is the original's only.
    let golden: Vec<&str> = golden
        .into_iter()
        .filter(|r| !r.starts_with("A clear"))
        .collect();
    assert_eq!(got, golden);
}

/// Films and dialogs ended until the panorama is back; the passes run.
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
        if e.loop_type == LoopType::Warp {
            break;
        }
    }
    assert_eq!(e.loop_type, LoopType::Warp);
    resumes
}

/// A game in Ireland's view 2, chapter 16, as the scripts put it there.
fn in_view_2(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=1)"[..],
        b"(changewam=bigile1.wam)",
        b"(vue=2)",
        b"(chapter=16)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("bigile1.wam", 2));
    assert_eq!(e.script_state().unwrap().chapter, 16);
    e.trace = true;
    e
}

/// The golden's fresh state: no hand, an empty bag, `FINCROIX` and
/// `flagdeesse` 0, `MONDE` = `monde`, the cross's state `cross`, view 2
/// entered again (its slots loaded, the entry hook run).
fn fresh(e: &mut Engine, monde: i32, cross: i32) {
    settle(e);
    e.run_command(b"(objetenmain=0)");
    let s = e.script_state_mut().unwrap();
    s.bag.clear();
    for (k, v) in [("FINCROIX", 0), ("flagdeesse", 0), ("MONDE", monde)] {
        assert!(s.set_var(k, v));
    }
    e.set_cross_state(cross);
    e.enter_at(2, 0.0, 0.0).unwrap();
    e.cursor.shown = None;
    LOGGED.with(|l| l.borrow_mut().clear());
    e.events.clear();
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

/// The cross's slot (`CROIXBRIGIT_SPRITE`, an object).
fn cross_slot(e: &Engine) -> Option<usize> {
    let id = e
        .script_state()
        .unwrap()
        .names
        .id(defnames::OBJETS, b"CROIXBRIGIT_SPRITE")
        .unwrap() as i32;
    e.anim.slots.iter().position(|s| s.id == id && s.kind == 2)
}

/// The golden's state: the cross's state and table, `OBJETENMAIN`, the
/// cursor pair, the hand pair, the bag, `FINCROIX`, `flagdeesse`.
fn st(e: &Engine) -> String {
    let s = e.script_state().unwrap();
    let t = cross_slot(e).map_or("-".to_string(), |n| tab(&e.anim.slots[n].frames));
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
        "state {} tab {t} hand {} cursor {cursor} pair {pair} bag {bag} FINCROIX {} flagdeesse {}",
        e.cross_state(),
        var(e, "OBJETENMAIN"),
        var(e, "FINCROIX"),
        var(e, "flagdeesse"),
    )
}

/// What voice 2 starts, in order: a film's video, its track, a
/// `NEWSOUND`.
fn sounds(evs: &[Event]) -> String {
    lst(evs.iter().filter_map(|e| match e {
        Event::VideoStart {
            name,
            frames: Some(_),
        } => Some(format!("film:{name}")),
        Event::Sound(n) => Some(n.clone()),
        _ => None,
    }))
}

fn lines(evs: &[Event]) -> Vec<usize> {
    evs.iter()
        .filter_map(|e| match e {
            Event::Set { chapter: 16, line } => Some(*line),
            _ => None,
        })
        .collect()
}

#[test]
fn the_entry_and_click_hooks_as_the_original() {
    let Some(files) = game() else { return };
    capture();
    let mut e = in_view_2(files);
    let names = e.script_state().unwrap().names.clone();
    let id = |n: &str| names.id(defnames::OBJETS, n.as_bytes()).unwrap() as i32;
    assert_eq!(
        rows("ids objects")[0],
        format!(
            "ids objects {} {} {} {} {} character {} param {}",
            id("CROIXBRIGIT_SPRITE"),
            id("CROIX_BRIGID"),
            id("CRANE_CROIX"),
            id("BOUGIE"),
            id("CRANE_CHAPELLE"),
            names.id(defnames::PERSONNAGES, b"DEESSE").unwrap(),
            names.param(defnames::OBJETS, b"CROIX_BRIGID")
        )
    );
    let sprite = id("CROIXBRIGIT_SPRITE");

    // B: the entry hook; the table filled before it (pre) on a view
    // entered with the state 0, whose entry hook changes nothing.
    let mut got = Vec::new();
    for row in rows("B state") {
        let w: Vec<&str> = row.split(' ').collect();
        let st0: i32 = w[2].parse().unwrap();
        let pre: Vec<i32> = w[4]
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .filter(|x| !x.is_empty())
            .map(|x| x.parse().unwrap())
            .collect();
        let times: usize = w[6].parse().unwrap();
        fresh(&mut e, 1, 0);
        for &f in &pre {
            assert!(e.anim.add_frame(sprite, f));
        }
        e.set_cross_state(st0);
        for _ in 0..times {
            e.cross_entry();
        }
        got.push(format!(
            "B state {st0} pre {} times {times} -> {} | warn {}",
            lst(&pre),
            st(&e),
            warns()
        ));
    }
    assert_eq!(got, rows("B state"));
    // No slot of the cross (view 1): the add's warning.
    e.set_cross_state(0);
    e.enter_at(1, 0.0, 0.0).unwrap();
    LOGGED.with(|l| l.borrow_mut().clear());
    e.set_cross_state(4);
    e.cross_entry();
    assert_eq!(
        format!("B no slot of the cross, state 4 -> warn {}", warns()),
        rows("B no slot")[0]
    );

    // C: the click hook alone.
    let mut got = Vec::new();
    fn c(e: &mut Engine, label: &str, v: i32, t: i32, got: &mut Vec<String>) {
        e.events.clear();
        e.cross_click(v);
        let evs = std::mem::take(&mut e.events);
        got.push(format!(
            "C {label} zone {t} {v} -> {} | lines {} | sounds {} | warn {}",
            st(e),
            lst(lines(&evs)),
            sounds(&evs),
            warns()
        ));
    }
    let give = |e: &mut Engine, n: &str| {
        e.run_command(format!("(sacados={n})").as_bytes());
        e.run_command(format!("(objetenmain={n})").as_bytes());
    };
    fresh(&mut e, 1, 0);
    c(&mut e, "empty", 5, 4, &mut got);
    give(&mut e, "CROIX_BRIGID");
    got.push(format!("C held -> {}", st(&e)));
    c(&mut e, "cross", 5, 4, &mut got);
    c(&mut e, "empty", 7, 4, &mut got);
    c(&mut e, "empty", 5, 4, &mut got);
    c(&mut e, "cross", 25, 4, &mut got);
    give(&mut e, "CRANE_CROIX");
    c(&mut e, "CRANE_CROIX", 3, 4, &mut got);
    c(&mut e, "CRANE_CROIX", 25, 4, &mut got);
    c(&mut e, "cross", 12, 3, &mut got);
    e.run_command(b"(objetenmain=0)");
    c(&mut e, "empty", 12, 50, &mut got);
    e.run_command(b"(objetenmain=CROIX_BRIGID)");
    c(&mut e, "cross", 1, 4, &mut got);
    // The win started the goddess's film: the rows go on from a fresh
    // state after the win (the section ended by `fresh`).
    assert_eq!(e.loop_type, LoopType::Video);
    fresh(&mut e, 1, 1);
    let s = e.script_state_mut().unwrap();
    s.set_var("FINCROIX", 1);
    s.set_var("flagdeesse", 1);
    got.push(format!("C after the win -> {}", st(&e)));
    c(&mut e, "empty", 1, 4, &mut got);
    c(&mut e, "cross", 1, 4, &mut got);
    c(&mut e, "empty", 1, 4, &mut got);
    c(&mut e, "cross", 6, 4, &mut got);
    give(&mut e, "CROIX_BRIGID");
    c(&mut e, "cross", 9, 4, &mut got);
    c(&mut e, "empty", 6, 4, &mut got);
    c(&mut e, "empty", 9, 4, &mut got);
    fresh(&mut e, 1, 0);
    c(&mut e, "empty", 0, 4, &mut got);
    fresh(&mut e, 5, 0);
    give(&mut e, "CROIX_BRIGID");
    c(&mut e, "MONDE 5 cross", 2, 4, &mut got);
    c(&mut e, "MONDE 5 empty", 2, 4, &mut got);
    assert_eq!(got, rows("C "));

    // D: the dispatcher's choice of the Irish hooks of view 2.
    let mut got = Vec::new();
    for row in rows("D ") {
        let w: Vec<&str> = row.split(' ').collect();
        let (name, number, kind): (&str, u32, i32) =
            (w[1], w[2].parse().unwrap(), w[3].parse().unwrap());
        let hook = irish_click_hook(name.as_bytes(), number, kind, false)
            .filter(|&f| f == 0x0049_4170 || f == 0x0049_4190)
            .map_or("-".to_string(), |f| format!("{f:08x}"));
        // FUN_0049b910 never consumes an `IR` click.
        got.push(format!("D {name} {number} {kind} -> {hook} AL 0"));
    }
    assert_eq!(got, rows("D "));

    // E: the click as the game runs it, then the section resumed by the
    // film's and the dialog's ends.
    let mut got = Vec::new();
    fresh(&mut e, 1, 0);
    give(&mut e, "CROIX_BRIGID");
    // The golden starts from variables at 0.
    e.script_state_mut().unwrap().set_var("ClicZone", 0);
    for (label, v) in [("cross", 8), ("empty", 8), ("cross", 1), ("empty", 14)] {
        let zone = e
            .wam()
            .view(2)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 4 && z.value == v)
            .unwrap();
        let (yaw, pitch) = e.zone_aim(2, zone).unwrap();
        e.camera.enter(yaw, pitch);
        e.pass(None);
        assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zone));
        e.events.clear();
        e.click(100.0 * v as f64 + got.len() as f64 * 1000.0);
        e.pass(None);
        let mut evs = std::mem::take(&mut e.events);
        let resumes = settle(&mut e);
        evs.extend(std::mem::take(&mut e.events));
        let hook = evs
            .iter()
            .position(|e| matches!(e, Event::PuzzleHook { .. }))
            .expect("the hook");
        let Event::PuzzleHook {
            cliczone: at_hook, ..
        } = evs[hook]
        else {
            unreachable!()
        };
        // `CLICZONE` as the click handler leaves it (the golden reads it
        // after FUN_0049ca80); then the click scan, unless the film the
        // hook's scan started cleared the click.
        let after = evs
            .iter()
            .find_map(|x| match x {
                Event::ZoneClick { zone: z, cliczone } if *z == zone => Some(*cliczone),
                _ => None,
            })
            .expect("the zone click");
        let scan = evs.iter().position(|e| matches!(e, Event::ClickScan(_)));
        let (end, scanned) = match scan {
            Some(n) => {
                let Event::ClickScan(vars) = evs[n] else {
                    unreachable!()
                };
                assert_eq!(vars.cliczone, after);
                (n, 1)
            }
            None => {
                let n = evs
                    .iter()
                    .position(|e| matches!(e, Event::CrossLaid { .. } | Event::CrossTaken { .. }))
                    .map_or(evs.len(), |n| n + 1);
                (n, 0)
            }
        };
        let dialogs = lst(evs.iter().filter_map(|e| match e {
            Event::DialogStart {
                number,
                started: true,
            } => Some(*number),
            _ => None,
        }));
        // The cursor pair as the golden leaves it is the hook's; the
        // passes after it choose the cursor again (C rows check it).
        let state = st(&e);
        let state = format!(
            "{} cursor * pair{}",
            state.split(" cursor ").next().unwrap(),
            state.split(" pair").nth(1).unwrap()
        );
        got.push(format!(
            "E {label} zone 4 {v} -> out {} | hook cliczone {at_hook} lines {} sounds {} | cliczone {after} | click scan {scanned} | in {} resumes {resumes} {dialogs} | {state}",
            lst(lines(&evs[..hook])),
            lst(lines(&evs[hook..end])),
            sounds(&evs[hook..end]),
            lst(lines(&evs[end..])),
        ));
    }
    let blank = |r: &str| {
        let (a, b) = r.split_once(" cursor (").unwrap();
        format!("{a} cursor * pair{}", b.split_once(") pair").unwrap().1)
    };
    let want: Vec<String> = rows("E ").iter().map(|r| blank(r)).collect();
    assert_eq!(got, want);
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
fn the_cross_is_drawn_from_its_frame_table_as_the_original() {
    let Some(files) = game() else { return };
    let data = files
        .read(Folder::Spw, "cbA1002002.SPW")
        .expect("the cross's SPW");
    let bg = background();
    let bg_md5 = md5_of(&bg);
    assert!(GOLDEN.contains(&format!("md5 {bg_md5}")));
    let golden_md5 = format!("{:x}", Md5::digest(&data));
    assert!(GOLDEN.contains(&format!("cba1002002.SPW (md5 {golden_md5}, 25 frames)")));
    let slot = |table: &[i32], click: i32| {
        let mut a = Anim::new(0);
        a.load(
            "cb",
            Spw::parse(data.clone()).unwrap(),
            2,
            27,
            0,
            state(0, click),
        );
        for (k, &f) in table.iter().enumerate() {
            a.slots[0].frames[k] = f as u8;
        }
        a
    };
    let parse = |s: &str| -> Vec<i32> {
        s.trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .filter(|x| !x.is_empty())
            .map(|x| x.parse().unwrap())
            .collect()
    };
    let spw = Spw::parse(data.clone()).unwrap();
    let mut got = Vec::new();
    let golden: Vec<&str> = rows("G tab");
    for row in &golden {
        let table = parse(row.split(' ').nth(2).unwrap());
        let a = slot(&table, 0);
        let mut faces: [Vec<u16>; 6] = std::array::from_fn(|_| bg.clone());
        let (_, hit) = a.draw(&mut faces, None, true);
        let al = i32::from(hit.slot.is_some());
        let mut parts = Vec::new();
        for (k, f) in faces.iter().enumerate() {
            let h = md5_of(f);
            if h == bg_md5 {
                parts.push(format!("f{k} bg"));
                continue;
            }
            let n = f.iter().zip(&bg).filter(|(a, b)| a != b).count();
            let mut samples = Vec::new();
            for &fr in table.iter().filter(|&&x| x >= 0) {
                let Some(b) = spw.face_box(fr, k) else {
                    continue;
                };
                let (x, y, w, hh) = (
                    i32::from(b.x),
                    i32::from(b.y),
                    i32::from(b.w),
                    i32::from(b.h),
                );
                if w > 0 && hh > 0 {
                    for (px, py) in [(x + w / 2, y + hh / 2), (x, y), (x + w - 1, y + hh - 1)] {
                        samples.push(format!("{px},{py}={:04x}", f[(py * 512 + px) as usize]));
                    }
                }
            }
            parts.push(format!("f{k} {h} n{n} {}", samples.join(" ")));
        }
        got.push(format!(
            "G tab {} AL {al} | {}",
            lst(&table),
            parts.join(" | ")
        ));
    }
    assert_eq!(got, golden);

    // The centre test on a frame-table slot with a click kind.
    let mut got = Vec::new();
    let golden = rows("G centre");
    for row in &golden {
        let w: Vec<&str> = row.split(' ').collect();
        let (face, col, rw): (usize, i32, i32) = (
            w[2].parse().unwrap(),
            w[3].parse().unwrap(),
            w[4].parse().unwrap(),
        );
        let table = parse(w[6]);
        let click: i32 = w[8].parse().unwrap();
        let a = slot(&table, click);
        let mut faces: [Vec<u16>; 6] = std::array::from_fn(|_| bg.clone());
        let (_, hit) = a.draw(&mut faces, Some((face, col, rw)), true);
        // The byte returned: the cursor state set and a frame drawn;
        // the slot recorded when it also has a click kind.
        let drawn = table.iter().any(|&f| f >= 0);
        let al = i32::from(hit.cursor != 0 && drawn);
        if click > 0 {
            assert_eq!(hit.slot.is_some(), al == 1);
        }
        got.push(format!(
            "G centre {face} {col} {rw} tab {} click {click} -> cursor {} AL {al}",
            lst(&table),
            hit.cursor
        ));
    }
    assert_eq!(got, golden);
}

/// The engine at 60 passes a second, a 15 Hz tick every fourth, every
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
        self.pass as f64 / 60.0
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
                dir.join(format!("pz-cross-{name}.png")),
                encode_rgb565(&self.screen, 640, 480),
            )
            .unwrap();
        }
    }

    fn var(&self, name: &str) -> i32 {
        var(&self.e, name)
    }

    fn id(&self, section: usize, name: &str) -> i32 {
        let s = self.e.script_state().unwrap();
        s.names.id(section, name.as_bytes()).unwrap() as i32
    }

    /// Turns the view onto zone `zone` (the centre on it).
    fn aim(&mut self, zone: usize) {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        assert_eq!(self.e.centre_zone().map(|(i, _)| i), Some(zone));
    }

    /// The zone of view 2 the cross is laid on, by its value.
    fn place(&self, v: i32) -> usize {
        self.e
            .wam()
            .view(2)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 4 && z.value == v)
            .unwrap()
    }

    /// A click on the move zone to view `to`, the transition video
    /// clicked away, a dialog the arrival plays listened to.
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
                if self.e.loop_type == LoopType::Dialog {
                    all.extend(self.listen());
                }
                assert_eq!(self.e.loop_type, LoopType::Warp);
                return all;
            }
        }
        panic!("no arrival in view {to}");
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
                return all;
            }
        }
        panic!("the dialog does not end");
    }

    /// Turns the view until the sprite of character `who` is under the
    /// centre and clicks it: its subject menu.
    fn open_menu(&mut self, who: i32) {
        for pitch in (-8..=8).map(|p| p * 5) {
            for yaw in (0..72).map(|y| y * 5 - 180) {
                let (y, p) = ((yaw as f32).to_radians(), (pitch as f32).to_radians());
                self.e.camera.enter(y, p);
                self.step();
                if self.e.sprite_under_centre().1 != 2 {
                    continue;
                }
                self.tap();
                let evs = self.events();
                if evs
                    .iter()
                    .any(|e| matches!(e, Event::SubjectMenu { character, .. } if *character == who))
                {
                    return;
                }
                // Another character's menu: closed by a click outside.
                if self.e.subject_menu.open {
                    self.point(0, 0);
                    self.tap();
                    self.step();
                }
            }
        }
        let slots: Vec<(i32, i32, bool, i32, String)> = self
            .e
            .anim
            .slots
            .iter()
            .map(|s| (s.id, s.kind, s.active, s.click, s.name.clone()))
            .collect();
        panic!("no sprite of {who} in view {}: {slots:?}", self.e.place().1);
    }

    /// Subject `name` asked through `who`'s menu: its dialog listened to.
    /// The dialog's number.
    fn ask(&mut self, who: i32, name: &str) -> u32 {
        self.open_menu(who);
        let value = self
            .e
            .script_state()
            .unwrap()
            .names
            .param(defnames::SUJETS, name.as_bytes());
        let list = self.e.script_state().unwrap().subjects.list(who);
        let i = list
            .iter()
            .position(|e| e.value == value)
            .unwrap_or_else(|| panic!("{name} not offered"));
        let (x, y) = cell(list.len() as i32, i as i32);
        self.point(x + 31, y + 31);
        self.step();
        self.shot(&format!("menu-{}", name.to_ascii_lowercase()));
        self.tap();
        let evs = self.events();
        let number = evs
            .iter()
            .find_map(|e| match e {
                Event::DialogStart {
                    number,
                    started: true,
                } => Some(*number),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}: no dialog"));
        self.wait(120);
        self.shot(&format!("dialog-{number}"));
        self.listen();
        number
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

    /// A click on the place of value `v` of view 2, the centre on it.
    fn click_place(&mut self, v: i32) -> Vec<Event> {
        let zone = self.place(v);
        self.aim(zone);
        self.tap();
        let evs = self.events();
        assert!(evs.iter().any(|e| matches!(e, Event::PuzzleHook { .. })));
        evs
    }
}

#[test]
fn the_cross_played_with_the_mouse_from_chapter_19_to_the_goddess() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    // Ireland as the chapters before 19 leave it: Liam in his cell
    // (chapter 4), his first talk done (`FLAG63`, chapter 5), the
    // statue's story told by Ailill (chapter 7: the subject
    // `ANCIENT_SOLEIL` offered on Liam), the game in view 9, chapter 3.
    for c in [
        &b"(monde=1)"[..],
        b"(changewam=bigile1.wam)",
        b"(vue=9)",
        b"(chapter=3)",
        b"(showperso=LIAM_CELLULE)",
        b"(flag63=1)",
        b"(enablesuj=LIAM_CELLULE,ANCIENT_SOLEIL)",
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
    assert_eq!(r.e.place(), ("bigile1.wam", 9));
    assert_eq!(r.e.script_state().unwrap().chapter, 3);
    let cross = r.id(defnames::OBJETS, "CROIX_BRIGID");
    let liam = r.id(defnames::PERSONNAGES, "LIAM_CELLULE");

    // Into Liam's cell (view 10): chapter 19. He is asked about the old
    // sun: dialog 76, the cross in hand and in the bag (chapter 19's
    // lines 51-55), the subject gone and `CROIX_BRIGID` offered.
    r.walk(10);
    assert_eq!(r.e.script_state().unwrap().chapter, 19);
    r.shot("1-liam-cell");
    assert_eq!(r.ask(liam, "ANCIENT_SOLEIL"), 76);
    assert!(r.e.bag_entries().iter().any(|b| b.id == cross));
    let s = r.e.script_state().unwrap();
    let offered: Vec<i32> = s.subjects.list(liam).iter().map(|e| e.value).collect();
    assert!(offered.contains(&s.names.param(defnames::SUJETS, b"CROIX_BRIGID")));
    assert!(!offered.contains(&s.names.param(defnames::SUJETS, b"ANCIENT_SOLEIL")));
    // The cross goes back to the bag (the bar opened and closed).
    if r.var("OBJETENMAIN") != 0 {
        r.e.right_click();
        r.step();
        r.e.right_click();
        r.step();
    }

    // To view 2: 10 → 9 (chapter 3), 8, 7, 1 (chapter 12), 2 (chapter 16).
    for to in [9, 8, 7, 1, 2] {
        r.walk(to);
    }
    assert_eq!(r.e.place(), ("bigile1.wam", 2));
    assert_eq!(r.e.script_state().unwrap().chapter, 16);
    assert_eq!((r.e.cross_state(), r.var("FINCROIX")), (0, 0));
    let slot = cross_slot(&r.e).expect("the cross's slot");
    assert_eq!(tab(&r.e.anim.slots[slot].frames), "[]");
    r.shot("2-the-place");

    // An empty hand on a place: the sound of a move, nothing else.
    let evs = r.click_place(5);
    assert_eq!(sounds(&evs), "[sound009.apc]");
    assert_eq!(r.e.cross_state(), 0);

    // The cross picked from the bag and laid on a wrong place (value 5):
    // drawn there, out of the hand and the bag, sounds 8 then 9.
    r.pick(cross);
    let evs = r.click_place(5);
    assert!(evs.contains(&Event::CrossLaid { value: 5 }));
    assert_eq!(sounds(&evs), "[sound008.apc,sound009.apc]");
    assert_eq!(r.e.cross_state(), 5);
    assert_eq!(tab(&r.e.anim.slots[slot].frames), "[4]");
    assert_eq!(r.var("OBJETENMAIN"), 0);
    assert!(!r.e.bag_entries().iter().any(|b| b.id == cross));
    assert_eq!(r.var("FINCROIX"), 0);
    r.wait(2);
    r.shot("3-laid-on-5");
    // Another place: only the sound.
    let evs = r.click_place(12);
    assert_eq!(sounds(&evs), "[sound009.apc]");
    assert_eq!(r.e.cross_state(), 5);
    // Its own place again: taken back, in hand and in the bag.
    let evs = r.click_place(5);
    assert!(evs.contains(&Event::CrossTaken { value: 5 }));
    assert_eq!(r.var("OBJETENMAIN"), cross);
    assert!(r.e.bag_entries().iter().any(|b| b.id == cross));
    assert_eq!(tab(&r.e.anim.slots[slot].frames), "[]");

    // Laid on the place of value 1: the win. `FINCROIX`, chapter 16's
    // section (lines 5-9): the goddess's film (its track cut by the two
    // sounds, all three on voice 2), then her dialog 56 and `flagdeesse`.
    let evs = r.click_place(1);
    assert!(evs.contains(&Event::CrossLaid { value: 1 }));
    assert_eq!(
        sounds(&evs),
        "[film:cine015.hnm,cine015.apc,sound008.apc,sound009.apc]"
    );
    assert_eq!(lines(&evs), vec![5]);
    assert_eq!((r.e.cross_state(), r.var("FINCROIX")), (1, 1));
    assert_eq!(r.e.loop_type, LoopType::Video);
    r.wait(150);
    r.shot("4-goddess-film");
    r.tap();
    let mut evs = Vec::new();
    for _ in 0..600 {
        evs.extend(r.events());
        if r.e.loop_type != LoopType::Video {
            break;
        }
    }
    assert!(evs.contains(&Event::DialogStart {
        number: 56,
        started: true
    }));
    r.wait(150);
    r.shot("5-goddess-dialog");
    evs.extend(r.listen());
    let ran = lines(&evs);
    assert!(
        [6, 7, 8, 9].iter().all(|l| ran.contains(l)),
        "chapter 16's lines 6-9: {ran:?}"
    );
    assert_eq!(r.var("flagdeesse"), 1);
    r.aim(r.place(1));
    r.wait(2);
    r.shot("6-cross-and-goddess");

    // The goddess asked about the statue (dialog 60): the cross leaves
    // the bag for good and its sprite is hidden (lines 51-52).
    let goddess = r.id(defnames::PERSONNAGES, "DEESSE");
    assert_eq!(r.ask(goddess, "STATUE"), 60);
    let id = r.id(defnames::OBJETS, "CROIXBRIGIT_SPRITE");
    assert!(!r.e.anim.slots.iter().any(|s| s.id == id && s.active));
    r.aim(r.place(1));
    r.wait(2);
    r.shot("7-after-the-statue");
}

#[test]
fn the_cross_state_is_saved_and_drawn_again_after_a_load() {
    let Some(files) = game() else { return };
    let mut e = in_view_2(files);
    e.run_command(b"(sacados=CROIX_BRIGID)");
    e.run_command(b"(objetenmain=CROIX_BRIGID)");
    e.cross_click(7);
    let d = e.save_data();
    assert_eq!(d.entry(32), 7i32.to_le_bytes());
    // Taken back, then the save loaded: view 2 entered again, the cross
    // drawn on place 7.
    e.cross_click(7);
    assert_eq!(e.cross_state(), 0);
    e.load_data(&d).unwrap();
    assert_eq!(e.cross_state(), 7);
    let n = cross_slot(&e).unwrap();
    assert_eq!(tab(&e.anim.slots[n].frames), "[6]");
}

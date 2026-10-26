//! The skull puzzle of Ireland's views 40-42 against the original
//! (`tests/golden/skull.txt`, its header there): the SPW frame lists
//! (rows A), the view entry (B) and the click (C) alone, the dispatchers'
//! choice (D), the zone click and the click scan of chapter 7 for whole
//! placement orders (E, through [`Engine::click`] and a pass, the film
//! ended), and the drawing of the frame lists into the cube faces (F, the
//! three sprite files). Then the state saved and loaded, and a game that
//! plays the puzzle with the mouse from chapter 3's hub to the win and
//! the dialog it unlocks.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped. With
//! `ATLANTIS2_PNG_DIR` set, the played game writes `pz-skull-*.png` there.

use std::path::PathBuf;

use atlantis2_data::defnames::{self, DefNames};
use atlantis2_data::files::Folder;
use atlantis2_data::spw::{Spw, FACE};
use atlantis2_data::wam::{EntityState, Zone};
use atlantis2_data::A2Files;
use atlantis2_engine::anim::{Anim, LIST_END};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::engine::{irish_click_hook, irish_entry_hook, SKULL_OBJECTS, SKULL_START};
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/skull.txt");

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

fn md5(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// The golden's rows of section `s` (their text after `"s "`).
fn rows(s: char) -> Vec<&'static str> {
    GOLDEN
        .lines()
        .filter(|l| l.len() > 2 && l.starts_with(s) && l.as_bytes()[1] == b' ')
        .map(|l| &l[2..])
        .collect()
}

/// The row whose label (the text before `": "`) is `label`: its result.
fn row<'a>(rows: &[&'a str], label: &str) -> &'a str {
    let head = format!("{label}: ");
    rows.iter()
        .find_map(|r| r.strip_prefix(head.as_str()))
        .unwrap_or_else(|| panic!("no golden row {label:?}"))
}

/// `{S:I}` → the name.
fn untok(n: &DefNames, t: &str) -> String {
    let t = t.trim_start_matches('{').trim_end_matches('}');
    let (s, i) = t.split_once(':').unwrap();
    let name = n.name(s.parse().unwrap(), i.parse().unwrap()).unwrap();
    String::from_utf8_lossy(name).into_owned()
}

fn names(e: &Engine) -> DefNames {
    e.script_state().unwrap().names.clone()
}

fn id(e: &Engine, section: usize, name: &str) -> i32 {
    names(e).id(section, name.as_bytes()).unwrap() as i32
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

/// A list as the golden prints it.
fn fmt_list(l: &[i32]) -> String {
    format!(
        "[{}]",
        l.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// A python list (`[0, -1, 1]`) of the golden's labels.
/// The frame lists' warnings given since the last call, as the golden
/// prints them (`-` for none).
fn warnings(a: &mut Anim) -> String {
    let w: Vec<String> = std::mem::take(&mut a.warnings)
        .iter()
        .map(|w| format!("warning:{w}"))
        .collect();
    if w.is_empty() {
        "-".into()
    } else {
        w.join(",")
    }
}

fn parse_list(t: &str) -> Vec<i32> {
    let a = t.find('[').unwrap();
    let b = t[a..].find(']').unwrap() + a;
    t[a + 1..b]
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(|x| x.parse().unwrap())
        .collect()
}

/// The skull sprite's state record (`bigile1.wam`): a list of frames,
/// shown, not clickable unless `click`.
fn state(click: i32) -> EntityState {
    EntityState {
        letter: u32::from(b'A'),
        anim: 1,
        view: 42,
        made_for: 42,
        shown: 1,
        click,
        ..EntityState::default()
    }
}

fn skull_spw(files: &A2Files, view: u32) -> (String, Vec<u8>) {
    let name = format!("cra1{view:03}042.SPW");
    let data = files.read(Folder::Spw, &name).expect("the skull's sprite");
    (name, data)
}

// ---- A: the frame lists ---------------------------------------------------

#[test]
fn frame_lists_as_the_original() {
    let Some(files) = game() else { return };
    let e = booted(files);
    let n = names(&e);
    let sk = n.id(defnames::OBJETS, b"CRANE_SPRITE").unwrap() as i32;
    let other = n.id(defnames::OBJETS, b"CRANE_LIVRE").unwrap() as i32;
    let (_, data) = skull_spw(e.files(), 42);
    let golden = rows('A');
    let mut a = Anim::new();
    let set = |a: &mut Anim, ids: &[i32]| {
        a.clear();
        for &i in ids {
            a.load("x", Spw::parse(data.clone()).unwrap(), 2, i, 0, state(0));
        }
    };
    let lists = |a: &Anim| {
        (0..a.slots.len())
            .map(|k| format!("s{k}{}", fmt_list(&a.frame_list(k))))
            .collect::<Vec<_>>()
            .join(" ")
    };
    // ret, lists, the warnings the call gave.
    let check = |a: &mut Anim, label: &str, ret: i32| {
        let want = row(&golden, label);
        let got = format!("ret={ret} | {} | {}", lists(a), warnings(a));
        assert_eq!(got, want, "{label}");
    };
    let id_of = |w: &str| {
        if w == "SPRITE" {
            sk
        } else if w == "OTHER" {
            other
        } else {
            99999
        }
    };
    set(&mut a, &[sk, other, sk]);
    let ops: [(&str, &str, i32); 21] = [
        ("add", "SPRITE", 3),
        ("add", "SPRITE", 5),
        ("add", "SPRITE", 3),
        ("add", "SPRITE", 7),
        ("add", "OTHER", 0),
        ("has", "SPRITE", 5),
        ("has", "SPRITE", 4),
        ("has", "OTHER", 3),
        ("sub", "SPRITE", 3),
        ("sub", "SPRITE", 3),
        ("sub", "SPRITE", 3),
        ("sub", "SPRITE", 9),
        ("sub", "99999", 1),
        ("add", "99999", 1),
        ("add", "SPRITE", -1),
        ("has", "SPRITE", -1),
        ("add", "SPRITE", 0x105),
        ("sub", "SPRITE", 5),
        ("clear", "SPRITE", 0),
        ("has", "SPRITE", 7),
        ("end", "", 0),
    ];
    let labels = [
        "add(SPRITE,3)",
        "add(SPRITE,5)",
        "add(SPRITE,3) again",
        "add(SPRITE,7)",
        "add(OTHER,0)",
        "has(SPRITE,5)",
        "has(SPRITE,4)",
        "has(OTHER,3)",
        "sub(SPRITE,3)",
        "sub(SPRITE,3) again",
        "sub(SPRITE,3) third",
        "sub(SPRITE,9) absent",
        "sub(99999,1) no slot",
        "add(99999,1) no slot",
        "add(SPRITE,-1)",
        "has(SPRITE,-1)",
        "add(SPRITE,0x105)",
        "sub(SPRITE,5) first of s0",
        "clear(SPRITE)",
        "has(SPRITE,7) after clear",
    ];
    for ((op, who, f), label) in ops.iter().zip(labels) {
        let i = id_of(who);
        match *op {
            "add" => {
                let r = a.add_frame(i, *f);
                check(&mut a, label, i32::from(r));
            }
            "sub" => {
                let r = a.sub_frame(i, *f);
                check(&mut a, label, i32::from(r));
            }
            "has" => {
                let r = a.has_frame(i, *f);
                check(&mut a, label, i32::from(r));
            }
            // FUN_0047ca10 (not used by the port): every list of the id
            // emptied, 0 returned.
            _ => {
                for s in a.slots.iter_mut().filter(|s| s.id == i) {
                    s.frames = [LIST_END; 128];
                }
                check(&mut a, label, 0);
            }
        }
    }
    set(&mut a, &[sk]);
    for k in 0..128 {
        a.add_frame(sk, k % 7);
    }
    let r = a.add_frame(sk, 1);
    check(
        &mut a,
        "add(SPRITE,1) on a full list (128 entries)",
        i32::from(r),
    );
    let r = a.sub_frame(sk, 6);
    check(&mut a, "sub(SPRITE,6) on a full list", i32::from(r));
    set(&mut a, &[other]);
    let r = a.has_frame(sk, 0);
    check(
        &mut a,
        &format!("has(SPRITE,0) without a {{1:{sk}}} slot"),
        i32::from(r),
    );
}

// ---- B, C, D: the hooks alone ----------------------------------------------

/// A new game, its film ended (the scripts loaded).
fn booted(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    e
}

/// `bigile1.wam` view 42 entered from a new game, without the story.
fn view_42(files: A2Files) -> Engine {
    let mut e = booted(files);
    for c in [&b"(monde=1)"[..], b"(changewam=bigile1.wam)", b"(vue=42)"] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("bigile1.wam", 42));
    e.trace = true;
    e
}

fn skull_slot(e: &Engine) -> usize {
    let sk = id(e, defnames::OBJETS, "CRANE_SPRITE");
    e.anim.slots.iter().position(|s| s.id == sk).unwrap()
}

fn put_list(e: &mut Engine, slot: usize, l: &[i32]) {
    let f = &mut e.anim.slots[slot].frames;
    *f = [LIST_END; 128];
    for (k, v) in l.iter().enumerate() {
        f[k] = *v as u8;
    }
}

fn set_state(e: &mut Engine, s: &str) {
    let mut b = [0u8; 8];
    for (k, c) in s.bytes().take(7).enumerate() {
        b[k] = c;
    }
    e.skull = b;
}

fn state_str(e: &Engine) -> String {
    let end = e.skull.iter().position(|&c| c == 0).unwrap_or(8);
    String::from_utf8_lossy(&e.skull[..end]).into_owned()
}

#[test]
fn the_view_entry_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = view_42(files);
    let sk = id(&e, defnames::OBJETS, "CRANE_SPRITE");
    let slot = skull_slot(&e);
    // The entry of view 42 itself left the start's frames.
    assert_eq!(e.skull, SKULL_START);
    assert_eq!(e.anim.frame_list(slot), [0, 1]);
    assert_eq!(e.skull_sprite, sk);
    let golden = rows('B');
    let mut seen = 0;
    for r in &golden {
        let Some(rest) = r.strip_prefix("view ") else {
            continue;
        };
        let Some((head, want)) = rest.split_once(": ") else {
            continue;
        };
        if !head.contains(" state '") {
            continue;
        }
        // view V state 'S' list [..]
        let st = head.split('\'').nth(1).unwrap();
        let pre = parse_list(head.split(" list ").nth(1).unwrap());
        put_list(&mut e, slot, &pre);
        set_state(&mut e, st);
        e.skull_sprite = 0;
        e.skull_entry();
        let got = format!(
            "s0{} id={} state='{}' | -",
            fmt_list(&e.anim.frame_list(slot)),
            e.skull_sprite,
            state_str(&e)
        );
        assert_eq!(got, *want, "{head}");
        seen += 1;
    }
    assert_eq!(seen, 33);
    // Without the sprite's slot: nothing to add to (two warnings).
    let want = row(&golden, &format!("view 42 without a {{1:{sk}}} slot"));
    let mut no = e.anim.slots.remove(slot);
    set_state(&mut e, "0199999");
    e.skull_sprite = 0;
    e.anim.warnings.clear();
    e.skull_entry();
    assert!(e.anim.slots.iter().all(|s| s.id != sk));
    let got = format!("id={} | {}", e.skull_sprite, warnings(&mut e.anim));
    assert_eq!(Some(got.as_str()), want.split_once(" ").map(|(_, w)| w));
    // Two slots of the sprite: both lists.
    no.frames = [LIST_END; 128];
    e.anim.slots.insert(slot, no.clone());
    e.anim.slots.insert(slot, no);
    put_list(&mut e, slot + 1, &[]);
    e.skull_entry();
    let got = format!(
        "s0{} s1{} | -",
        fmt_list(&e.anim.frame_list(slot)),
        fmt_list(&e.anim.frame_list(slot + 1))
    );
    assert_eq!(got, row(&golden, &format!("view 42 two {{1:{sk}}} slots")));
}

/// A zone of the 40-byte form.
fn zone(kind: i32, value: i32, name: &str) -> Zone {
    Zone {
        kind,
        value,
        name: Some(name.to_string()),
        points: Vec::new(),
    }
}

#[test]
fn the_click_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = view_42(files);
    let n = names(&e);
    let slot = skull_slot(&e);
    let crane_zone = id(&e, defnames::ZONES_CLIC, "CRANE_ZONE");
    let golden = rows('C');
    // One click: the cursor pair set before (any frame), the hand, the
    // hook; the row's result.
    let click = |e: &mut Engine, z: &Zone, hand: i32, fresh: bool| -> String {
        if fresh {
            put_list(e, slot, &[]);
            set_state(e, "0199999");
            e.skull_entry();
        }
        e.cursor.shown = Some(5);
        let k = e.script_state().unwrap().keys.objetenmain;
        e.script_state_mut().unwrap().vars[k] = hand;
        e.events.clear();
        e.skull_click(z);
        let cleared = e.cursor.shown.is_none();
        let sounds: Vec<&Event> = e
            .events
            .iter()
            .filter(|x| matches!(x, Event::Sound(_)))
            .collect();
        let calls = if sounds.is_empty() {
            "-".to_string()
        } else {
            assert_eq!(sounds, [&Event::Sound("sound007.apc".into())]);
            "RunOneCommand('(NewSound=7)')".to_string()
        };
        format!(
            "s0{} state='{}' cursor={} hand={} | {calls}",
            fmt_list(&e.anim.frame_list(slot)),
            state_str(e),
            if cleared { "(0,0)" } else { "(77,5)" },
            var(e, "OBJETENMAIN"),
        )
    };
    let hand_of = |t: &str| -> i32 {
        if t == "0" {
            0
        } else {
            n.id(defnames::OBJETS, untok(&n, t).as_bytes()).unwrap() as i32
        }
    };
    let mut seen = 0;
    for r in &golden {
        let (head, want) = r.split_once(": ").unwrap();
        let tok = |s: &str| format!("{{4:{crane_zone}}}{s}");
        let z = if let Some(h) = head.strip_prefix(&tok(" hand=")) {
            Some((zone(3, crane_zone, "CRANE_ZONE"), h))
        } else if let Some(h) = head.strip_prefix(&tok(" type 1 hand=")) {
            Some((zone(1, crane_zone, "CRANE_ZONE"), h))
        } else if head.contains("(its own value)") {
            let zt = head.split_whitespace().nth(1).unwrap();
            let v = n
                .id(defnames::ZONES_CLIC, untok(&n, zt).as_bytes())
                .unwrap() as i32;
            Some((
                zone(3, v, &untok(&n, zt)),
                head.split("hand=").nth(1).unwrap(),
            ))
        } else if head.contains(", value 0,") {
            Some((
                zone(3, 0, "CRANE_ZONE"),
                head.split("hand=").nth(1).unwrap(),
            ))
        } else if head.starts_with("zone named XX") {
            Some((
                zone(3, crane_zone, "XX"),
                head.split("hand=").nth(1).unwrap(),
            ))
        } else {
            None
        };
        if let Some((z, h)) = z {
            let hand = hand_of(h);
            assert_eq!(click(&mut e, &z, hand, true), *want, "{head}");
            seen += 1;
        }
    }
    assert_eq!(seen, 14);
    let cz = zone(3, crane_zone, "CRANE_ZONE");
    // The same object twice: listed twice.
    let ruche = id(&e, defnames::OBJETS, "CRANE_RUCHE");
    click(&mut e, &cz, ruche, true);
    let got = click(&mut e, &cz, ruche, false);
    let want = golden
        .iter()
        .find_map(|r| r.strip_prefix(&format!("{{1:{ruche}}} twice: ")))
        .unwrap();
    assert_eq!(got, want);
    // All five, the last first: the list in the clicks' order.
    put_list(&mut e, slot, &[]);
    set_state(&mut e, "0199999");
    e.skull_entry();
    let mut parts = Vec::new();
    for (o, _) in SKULL_OBJECTS.iter().rev() {
        let i = id(&e, defnames::OBJETS, o);
        parts.push(format!("{{1:{i}}}: {}", click(&mut e, &cz, i, false)));
    }
    let want = row(&golden, "all five, W18 first");
    assert_eq!(parts.join(" || "), want);
    // Then the entry of view 41 (its argument unread): the list rebuilt in
    // the state's order.
    e.skull_entry();
    let want = row(&golden, "then the entry hook of view 41 (same slot)");
    assert_eq!(
        format!(
            "s0{} state='{}'",
            fmt_list(&e.anim.frame_list(slot)),
            state_str(&e)
        ),
        want
    );
    // Before any entry (the kept id 0): no slot of id 0, the frame lost,
    // the digit and the sound as always.
    put_list(&mut e, slot, &[]);
    set_state(&mut e, "0199999");
    e.skull_sprite = 0;
    let croix = id(&e, defnames::OBJETS, "CRANE_CROIX");
    e.anim.warnings.clear();
    let got = click(&mut e, &cz, croix, false);
    let want = row(&golden, "without an entry hook (0x006d26a8 = 0)");
    assert_eq!(format!("{got},{}", warnings(&mut e.anim)), want);
}

#[test]
fn the_dispatchers_choose_the_skull_as_the_original() {
    let mut n = 0;
    for r in rows('D') {
        let (head, want) = r.split_once(": ").unwrap();
        let w: Vec<&str> = head.split_whitespace().collect();
        let (name, view) = (w[1], w[3].parse::<u32>().unwrap());
        let res = want.split_whitespace().next().unwrap();
        match w[0] {
            "click" => {
                let kind: i32 = w[5].parse().unwrap();
                let got = match irish_click_hook(name.as_bytes(), view, kind, false) {
                    Some(0x0049_3d20) => "00493d20()",
                    Some(0x0049_3d40) => "00493d40(zone)",
                    Some(f) => panic!("{head}: FUN_{f:08x}"),
                    None => "-",
                };
                assert_eq!(got, res, "{head}");
                assert!(want.ends_with("AL=0"));
            }
            _ => {
                let got = match irish_entry_hook(name.as_bytes(), view) {
                    Some(0x0049_3cb0) => format!("00493cb0({view})"),
                    Some(f) => panic!("{head}: FUN_{f:08x}"),
                    None => "-".to_string(),
                };
                assert_eq!(got, res, "{head}");
            }
        }
        n += 1;
    }
    assert_eq!(n, 210);
}

// ---- E: the clicks of a game, on chapter 7 --------------------------------

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

/// Ireland in chapter 7, view 42, the five objects in the bag.
fn chapter_7_view_42(files: A2Files, n: &mut Option<DefNames>) -> Engine {
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
    e.run_command(b"(chapter=7)");
    e.run_command(b"(vue=42)");
    settle(&mut e);
    for (o, _) in SKULL_OBJECTS {
        e.run_command(format!("(sacados={o})").as_bytes());
    }
    e.pass(None);
    assert_eq!(e.place(), ("bigile1.wam", 42));
    assert_eq!(e.script_state().unwrap().chapter, 7);
    *n = Some(names(&e));
    e.trace = true;
    e
}

fn aim(e: &mut Engine, zone: usize) {
    let view = e.place().1;
    let (yaw, pitch) = e.zone_aim(view, zone).unwrap();
    e.camera.enter(yaw, pitch);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zone));
}

fn crane_zone_index(e: &Engine) -> usize {
    e.wam()
        .view(42)
        .unwrap()
        .zones
        .iter()
        .position(|z| z.name.as_deref() == Some("CRANE_ZONE"))
        .unwrap()
}

/// One E row: the hand set, a click on the zone, the film a section
/// starts ended; the row's result (the hook's sound for its command, the
/// scans' `/set` lines of chapter 7, the variables, the bag, the list,
/// the state, the two slots' active flags, the films).
fn e_click(e: &mut Engine, hand: Option<&str>, t: &mut f64) -> String {
    let slot = skull_slot(e);
    let ailill = id(e, defnames::PERSONNAGES, "AILILL");
    let a_slot = e
        .anim
        .slots
        .iter()
        .position(|s| s.id == ailill && s.kind == 1)
        .unwrap();
    match hand {
        Some(o) => e.run_command(format!("(objetenmain={o})").as_bytes()),
        None => e.run_command(b"(objetenmain=0)"),
    }
    let z = crane_zone_index(e);
    aim(e, z);
    e.events.clear();
    *t += 10.0;
    e.click(*t);
    e.pass(None);
    let mut evs = std::mem::take(&mut e.events);
    let cliczone = evs
        .iter()
        .find_map(|x| match x {
            Event::ClickScan(v) => Some(v.cliczone),
            _ => None,
        })
        .unwrap();
    let mut resumes = 0;
    if e.loop_type == LoopType::Video {
        resumes = 1;
        e.end_video();
        for _ in 0..10 {
            e.pass(None);
            if e.loop_type == LoopType::Warp {
                break;
            }
        }
        evs.extend(std::mem::take(&mut e.events));
    }
    let hook = evs
        .iter()
        .position(|x| {
            *x == Event::PuzzleRan {
                function: 0x0049_3d40,
            }
        })
        .expect("the hook");
    let sounds: Vec<&Event> = evs
        .iter()
        .filter(|x| matches!(x, Event::Sound(_)))
        .collect();
    let hook_sound = sounds.first() == Some(&&Event::Sound("sound007.apc".into()));
    if hook_sound {
        let at = evs
            .iter()
            .position(|x| *x == Event::Sound("sound007.apc".into()))
            .unwrap();
        assert!(hook < at);
    }
    let lines: Vec<String> = evs
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter: 7, line } => Some(line.to_string()),
            _ => None,
        })
        .collect();
    let mut calls: Vec<String> = Vec::new();
    for x in sounds.iter().skip(usize::from(hook_sound)) {
        if let Event::Sound(s) = x {
            // A film's sound track (`AtlSound_CinemaApc`) or a sound.
            let f = if s.starts_with("cine") {
                "AtlSound_CinemaApc"
            } else {
                "AtlSound_NewSound"
            };
            calls.push(format!("{f}('{s}')"));
        }
    }
    for x in &evs {
        if let Event::VideoStart { name, .. } = x {
            calls.push(format!("AtlHnm_start('{name}')"));
        }
    }
    let bag: Vec<String> = e.bag_entries().iter().map(|b| b.id.to_string()).collect();
    let (cz, fc, hand) = (
        id(e, defnames::VARIABLES, "ClicZone"),
        id(e, defnames::VARIABLES, "fincrane"),
        id(e, defnames::VARIABLES, "ObjetEnMain"),
    );
    format!(
        "OUT lines - | hook {} | {{3:{cz}}}={cliczone} | IN lines {} resumes={resumes} | {{3:{fc}}}={} {{3:{hand}}}={} bag=[{}] | {} state='{}' active={},{}",
        if hook_sound {
            "RunOneCommand('(NewSound=7)')"
        } else {
            "-"
        },
        if lines.is_empty() {
            "-".to_string()
        } else {
            lines.join(",")
        },
        var(e, "fincrane"),
        var(e, "OBJETENMAIN"),
        bag.join(","),
        fmt_list(&e.anim.frame_list(slot)),
        state_str(e),
        i32::from(e.anim.slots[slot].active),
        i32::from(e.anim.slots[a_slot].active),
    ) + &format!(" | calls={}", calls.join(";"))
}

/// The golden's E result without its stub-only call (the music volume,
/// which the port's events do not show), so that both sides list sounds
/// and films.
fn e_want(w: &str) -> String {
    let (head, tail) = w.split_once(" | calls=").unwrap();
    let calls: Vec<&str> = tail
        .split(" | ")
        .next()
        .unwrap()
        .split(';')
        .filter(|c| !c.starts_with("AtlMuzik_Volume") && *c != "-")
        .collect();
    format!("{head} | calls={}", calls.join(";"))
}

#[test]
fn the_clicks_of_a_game_as_the_original() {
    if game().is_none() {
        return;
    }
    let golden = rows('E');
    let mut t = 0.0;
    let mut k = 0;
    while k < golden.len() {
        let r = golden[k];
        let mut n = None;
        if let Some(rest) = r.strip_prefix("order ") {
            let mut e = chapter_7_view_42(game().unwrap(), &mut n);
            let n = n.unwrap();
            let start = rest.split("bag at start ").nth(1).unwrap();
            let bag: Vec<String> = e.bag_entries().iter().map(|b| b.id.to_string()).collect();
            assert_eq!(format!("[{}]", bag.join(",")), start);
            for step in 1..=5 {
                let row = golden[k + step];
                let (head, want) = row.split_once(": ").unwrap();
                let hand = untok(&n, head.split("hand=").nth(1).unwrap());
                let got = e_click(&mut e, Some(&hand), &mut t);
                assert_eq!(got, e_want(want), "{head}");
            }
            // The win: film 2 played, Ailill shown, the skull hidden.
            k += 6;
            continue;
        }
        k += 1;
    }
    // A click with nothing, or another object, in hand: no frame, no line.
    let mut n = None;
    let mut e = chapter_7_view_42(game().unwrap(), &mut n);
    let n = n.unwrap();
    for r in &golden {
        let (head, want) = r.split_once(": ").unwrap();
        if head.starts_with("empty hand on ") {
            assert_eq!(e_click(&mut e, None, &mut t), e_want(want));
        } else if head.contains(" in hand on ") {
            let o = untok(&n, head.split_whitespace().next().unwrap());
            assert_eq!(e_click(&mut e, Some(&o), &mut t), e_want(want));
        }
    }
    // `fincrane` = 4 before: the fifth object's section at once.
    let mut n = None;
    let mut e = chapter_7_view_42(game().unwrap(), &mut n);
    let n = n.unwrap();
    e.run_command(b"(fincrane=4)");
    let fc = id(&e, defnames::VARIABLES, "fincrane");
    let r = golden
        .iter()
        .find(|r| r.starts_with(&format!("{{3:{fc}}}=4 before")))
        .unwrap();
    let (head, want) = r.split_once(": ").unwrap();
    let hand = untok(&n, head.split("hand=").nth(1).unwrap());
    assert_eq!(e_click(&mut e, Some(&hand), &mut t), e_want(want));
}

// ---- F: the drawing --------------------------------------------------------

/// Pixel i of face k: ((i + k) × 2654435761 mod 2^32) >> 16.
fn background(k: usize) -> Vec<u16> {
    (0..FACE * FACE)
        .map(|i| (((i + k) as u32).wrapping_mul(2_654_435_761) >> 16) as u16)
        .collect()
}

#[test]
fn the_frame_lists_drawn_as_the_original() {
    let Some(files) = game() else { return };
    let e = booted(files);
    let sk = id(&e, defnames::OBJETS, "CRANE_SPRITE");
    let bgs: Vec<Vec<u16>> = (0..6).map(background).collect();
    let golden = rows('F');
    let mut n = 0;
    let mut files_seen = 0;
    for view in [40, 41, 42] {
        let (name, data) = skull_spw(e.files(), view);
        let spw = Spw::parse(data.clone()).unwrap();
        let head = format!("file {name} md5 {} size {}", md5(&data), data.len());
        assert!(golden.contains(&head.as_str()), "{head}");
        files_seen += 1;
        for r in &golden {
            let Some(rest) = r.strip_prefix(&format!("{name} ")) else {
                continue;
            };
            let (desc, want) = rest.split_once(": faces ").unwrap();
            let list = parse_list(desc);
            let active = !desc.contains("inactive");
            let single = desc.starts_with("single");
            let frame = if single {
                desc.split("frame ")
                    .nth(1)
                    .unwrap()
                    .split(',')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap()
            } else {
                0
            };
            let click = desc
                .split("click kind ")
                .nth(1)
                .map_or(0, |c| c.split(',').next().unwrap().parse().unwrap());
            let texel = desc.split("texel (").nth(1).map(|t| {
                let v: Vec<i32> = t
                    .split(')')
                    .next()
                    .unwrap()
                    .split(',')
                    .map(|x| x.trim().parse().unwrap())
                    .collect();
                (v[0] as usize, v[1], v[2])
            });
            let mut a = Anim::new();
            let mut st = state(click);
            if single {
                st.single = 1;
                st.first = frame;
            }
            a.load(&name, Spw::parse(data.clone()).unwrap(), 2, sk, 0, st);
            a.slots[0].active = active;
            for (k, v) in list.iter().enumerate() {
                a.slots[0].frames[k] = *v as u8;
            }
            let mut faces: [Vec<u16>; 6] = std::array::from_fn(|k| bgs[k].clone());
            let (_, hit) = a.draw(&mut faces, Some(texel.unwrap_or((0, -100, -100))), true);
            let bytes = |f: &[u16]| -> Vec<u8> { f.iter().flat_map(|p| p.to_le_bytes()).collect() };
            let md5s: Vec<String> = faces.iter().map(|f| md5(&bytes(f))).collect();
            let boxes: Vec<(i32, i32, i32, i32)> = list
                .iter()
                .filter(|&&f| f != -1)
                .filter_map(|&f| spw.face_box(f, 5))
                .map(|b| {
                    (
                        i32::from(b.x),
                        i32::from(b.y),
                        i32::from(b.w),
                        i32::from(b.h),
                    )
                })
                .filter(|b| b.2 > 0 && b.3 > 0)
                .collect();
            let mut samples = Vec::new();
            if !boxes.is_empty() {
                let x0 = boxes.iter().map(|b| b.0).min().unwrap();
                let y0 = boxes.iter().map(|b| b.1).min().unwrap();
                let x1 = boxes.iter().map(|b| b.0 + b.2).max().unwrap();
                let y1 = boxes.iter().map(|b| b.1 + b.3).max().unwrap();
                for j in 0..4 {
                    for i in 0..6 {
                        let x = x0 + (x1 - x0 - 1) * i / 5;
                        let y = y0 + (y1 - y0 - 1) * j / 3;
                        let p = faces[5][y as usize * FACE + x as usize];
                        samples.push(format!("{x},{y}={p:04x}"));
                    }
                }
            }
            let fmt_or = |v: Vec<String>, sep: &str| {
                if v.is_empty() {
                    "-".to_string()
                } else {
                    v.join(sep)
                }
            };
            let got = format!(
                "{} | cursor={} hit={} | boxes {} | samples {}",
                md5s.join(" "),
                hit.cursor,
                hit.slot.map_or(-1, |s| s as i32),
                fmt_or(
                    boxes
                        .iter()
                        .map(|b| format!("{},{},{},{}", b.0, b.1, b.2, b.3))
                        .collect(),
                    ";"
                ),
                fmt_or(samples, " "),
            );
            assert_eq!(got, want, "{name} {desc}");
            n += 1;
        }
    }
    assert_eq!((files_seen, n), (3, 3 * 24));
}

// ---- Saves -----------------------------------------------------------------

#[test]
fn the_state_is_saved_and_loaded() {
    if game().is_none() {
        return;
    }
    let mut n = None;
    let mut e = chapter_7_view_42(game().unwrap(), &mut n);
    let mut t = 0.0;
    e_click(&mut e, Some("CRANE_PUIT"), &mut t);
    e_click(&mut e, Some("CRANE_CROIX"), &mut t);
    assert_eq!(&e.skull, b"0129499\0");
    let d = e.save_data();
    // Save entry 30: the eight bytes.
    let off: usize = atlantis2_data::save::entry_offset(30);
    assert_eq!(&d.bytes()[off..off + 8], b"0129499\0");
    // A second new game in the same engine does not reset it
    // (FUN_0049bf70 leaves it): its view 42 draws the first game's pieces.
    e.start_new_game().unwrap();
    e.end_video();
    for c in [&b"(monde=1)"[..], b"(changewam=bigile1.wam)", b"(vue=42)"] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("bigile1.wam", 42));
    assert_eq!(&e.skull, b"0129499\0");
    assert_eq!(e.anim.frame_list(skull_slot(&e)), [0, 1, 2, 4]);
    // A load does: a fresh engine (the start's state) given the save.
    let mut f = chapter_7_view_42(game().unwrap(), &mut None);
    assert_eq!(f.skull, SKULL_START);
    f.load_data(&d).unwrap();
    assert_eq!(&f.skull, b"0129499\0");
    // The view entry of the load drew the frames from the state.
    let slot = skull_slot(&f);
    assert_eq!(f.anim.frame_list(slot), [0, 1, 2, 4]);
}

// ---- The puzzle played ------------------------------------------------------

/// The engine at 59.94 passes a second ([`PASS`]) (a 15 Hz tick every fourth), every
/// pass drawn.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
    gap: f64,
    png: Option<PathBuf>,
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

    fn tap(&mut self) {
        self.gap += 10.0;
        self.e.click(self.now() + self.gap);
    }

    fn events(&mut self) -> Vec<Event> {
        self.step();
        std::mem::take(&mut self.e.events)
    }

    fn point(&mut self, x: i32, y: i32) {
        let (px, py) = self.e.pointer;
        self.e.mouse_motion(x - px, y - py);
    }

    fn var(&self, name: &str) -> i32 {
        var(&self.e, name)
    }

    fn save(&self, name: &str) {
        if let Some(dir) = &self.png {
            std::fs::create_dir_all(dir).unwrap();
            let p = dir.join(format!("pz-skull-{name}.png"));
            std::fs::write(p, encode_rgb565(&self.screen, 640, 480)).unwrap();
        }
    }

    /// Turns the view onto zone `zone` (the centre on it).
    fn aim(&mut self, zone: usize) {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        assert_eq!(self.e.centre_zone().map(|(i, _)| i), Some(zone));
    }

    /// A click on the move zone to view `to`, the transition clicked
    /// away; the events, in order.
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
        for _ in 0..600 {
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

    /// Turns the view until a sprite is under the centre and clicks it;
    /// the events of the click when `done` holds on them.
    fn click_sprite(&mut self, done: impl Fn(&Event) -> bool) -> Vec<Event> {
        for pitch in (-8..=8).map(|p| p * 5) {
            for yaw in (0..72).map(|y| y * 5 - 180) {
                let (y, p) = ((yaw as f32).to_radians(), (pitch as f32).to_radians());
                self.e.camera.enter(y, p);
                self.step();
                if self.e.sprite_under_centre().1 == 0 {
                    continue;
                }
                self.tap();
                let evs = self.events();
                if evs.iter().any(&done) {
                    return evs;
                }
            }
        }
        panic!("no sprite answers in view {}", self.e.place().1);
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
}

/// Every `/set` line of an Irish chapter that ran is one of the lines the
/// original's handler ran (`scripts_worlds_ireland.txt`).
fn ireland_lines_as_the_original(evs: &[Event]) {
    let golden = include_str!("golden/scripts_worlds_ireland.txt");
    for e in evs {
        if let Event::Set { chapter, line } = e {
            if (1..=24).contains(chapter) {
                let head = format!("chapi{chapter:03}.con:{line} ");
                assert!(
                    golden.lines().any(|l| l.starts_with(&head)),
                    "chapter {chapter} line {line}: not a line the original runs"
                );
            }
        }
    }
}

#[test]
fn the_skull_played_with_the_mouse() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    // Ireland's hub (chapter 3) in view 28, the five objects of the skull
    // collected (chapters 9 and 13 give two, three are taken in views):
    // the state the story leaves before the skull's place.
    for c in [
        &b"(monde=1)"[..],
        b"(changewam=bigile1.wam)",
        b"(vue=5)",
        b"(chapter=2)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    e.run_command(b"(chapter=3)");
    e.run_command(b"(vue=28)");
    settle(&mut e);
    for (o, _) in SKULL_OBJECTS {
        e.run_command(format!("(sacados={o})").as_bytes());
    }
    e.trace = true;
    let mut r = Run {
        e,
        pass: 0,
        screen: vec![0; 640 * 480],
        gap: 0.0,
        png: std::env::var_os("ATLANTIS2_PNG_DIR").map(PathBuf::from),
    };
    r.wait(10);
    assert_eq!(r.e.place(), ("bigile1.wam", 28));
    assert_eq!(r.e.script_state().unwrap().chapter, 3);

    // The walk: 28 → 40 (chapter 3 sends the game to chapter 7 on the
    // way), 41, 42; each view's entry shows the skull's two first frames.
    let mut evs = Vec::new();
    for to in [40, 41, 42] {
        evs.extend(r.walk(to));
        assert!(evs.contains(&Event::PuzzleRan {
            function: 0x0049_3cb0
        }));
        let slot = skull_slot(&r.e);
        assert_eq!(r.e.anim.frame_list(slot), [0, 1]);
    }
    assert_eq!(r.e.script_state().unwrap().chapter, 7);
    assert!(!evs
        .iter()
        .any(|x| matches!(x, Event::PuzzleNotPorted { .. })));
    ireland_lines_as_the_original(&evs);
    let zone = crane_zone_index(&r.e);
    r.aim(zone);
    r.wait(4);
    r.save("1-view42");

    // Each object picked from the bar and laid on the skull: its frame
    // shown, the hook's sound then chapter 7's, `fincrane` counted, the
    // object gone from the hand and the bag.
    let ailill = id(&r.e, defnames::PERSONNAGES, "AILILL");
    let mut all = Vec::new();
    for (k, (o, frame)) in SKULL_OBJECTS.iter().enumerate() {
        let object = id(&r.e, defnames::OBJETS, o);
        r.pick(object);
        r.aim(zone);
        r.tap();
        let mut evs = r.events();
        assert!(evs.contains(&Event::PuzzleRan {
            function: 0x0049_3d40
        }));
        let s7 = evs
            .iter()
            .position(|x| *x == Event::Sound("sound007.apc".into()))
            .expect("the hook's sound");
        let slot = skull_slot(&r.e);
        let mut want = vec![0, 1];
        want.extend(SKULL_OBJECTS[..=k].iter().map(|o| o.1));
        assert_eq!(r.e.anim.frame_list(slot), want);
        assert_eq!(r.e.skull[*frame as usize], b'0' + *frame as u8);
        assert!(!r.e.bag_entries().iter().any(|b| b.id == object));
        if k < 4 {
            let s3 = evs
                .iter()
                .position(|x| *x == Event::Sound("sound003.apc".into()))
                .expect("chapter 7's sound");
            assert!(s7 < s3);
            assert_eq!(r.var("fincrane"), k as i32 + 1);
            assert_eq!(r.var("OBJETENMAIN"), 0);
            r.wait(8);
            r.save(&format!("{}-laid-{}", k + 2, frame));
        } else {
            // The fifth: film 2, then Ailill shown and the skull hidden.
            assert!(evs
                .iter()
                .any(|x| matches!(x, Event::VideoStart { name, .. } if name == "cine002.hnm")));
            assert_eq!(r.var("fincrane"), 6);
            r.wait(30);
            r.save("7-film");
            r.tap();
            for _ in 0..600 {
                evs.extend(r.events());
                if r.e.loop_type == LoopType::Warp {
                    break;
                }
            }
            assert_eq!(r.e.loop_type, LoopType::Warp);
        }
        all.extend(evs);
    }
    assert_eq!(&r.e.skull, b"0123456\0");
    let lines: Vec<usize> = all
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter: 7, line } => Some(*line),
            _ => None,
        })
        .collect();
    // The original's lines (rows E of the golden, the objects in this
    // order): four counts, then the fifth's section with its film.
    assert_eq!(
        lines,
        [
            65, 66, 67, 68, 79, 80, 81, 82, 93, 94, 95, 96, 107, 108, 109, 110, 114, 115, 116, 117,
            118, 119
        ]
    );
    ireland_lines_as_the_original(&all);
    let slot = skull_slot(&r.e);
    assert!(!r.e.anim.slots[slot].active);
    let a_slot =
        r.e.anim
            .slots
            .iter()
            .position(|s| s.id == ailill && s.kind == 1)
            .unwrap();
    assert!(r.e.anim.slots[a_slot].active);
    assert!(r
        .e
        .bag_entries()
        .iter()
        .all(|b| SKULL_OBJECTS
            .iter()
            .all(|(o, _)| id(&r.e, defnames::OBJETS, o) != b.id)));
    r.wait(8);
    r.save("8-ailill");

    // What the win unlocks: Ailill, shown, answers a click (chapter 7's
    // first section for him: dialog 31).
    let evs = r.click_sprite(|x| matches!(x, Event::DialogStart { .. }));
    assert!(evs.contains(&Event::DialogStart {
        number: 31,
        started: true
    }));
    r.wait(30);
    r.save("9-ailill-dialog");
    let evs = r.listen();
    ireland_lines_as_the_original(&evs);
    assert_eq!(r.e.loop_type, LoopType::Warp);
}

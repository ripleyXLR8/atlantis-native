//! The star map (Xibalba, `xibalba.wam` view 121) against the original
//! (`tests/golden/stars.txt`, its header there): the view entry hook
//! (rows B), the click hook alone and the win test alone (C, through
//! [`Engine::stars_click`] and [`Engine::stars_win_test`]), the
//! dispatchers' choice of the hooks (D), the zone click and the click
//! scan of the same frame on chapter 39, with the section the win starts
//! resumed by the film's end (E), and the map's slot drawn from its frame
//! table (G: the faces' md5s and sampled pixels).
//!
//! Then the puzzle played as a player plays it, with the mouse: from
//! chapter 39 (the clearing reached by the river, the canoe won) the
//! seven stars taken in their views, laid on the map, a wrong one taken
//! back; laid all on wrong holes, the bat of chapter 35 offers its subject
//! about the stars (its `/INIT` reads `pz_etoile_started`); back in the
//! clearing, the stars moved to the right holes: the win (film 49, the
//! statue of Tezcatlipoca, its zones enabled), the statue asked through
//! its zone. The map kept by a save. With `ATLANTIS2_SHOTS` set to a
//! folder, the played test writes `pz-stars-*.png` there (one of them
//! the HD view).
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
use atlantis2_engine::engine::{yucatan_entry_hooks, STAR_HOLES};
use atlantis2_engine::png::{encode_rgb565, encode_xrgb8888};
use atlantis2_engine::subjects::cell;
use atlantis2_engine::{CursorBank, Engine, Enhancements, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/stars.txt");

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

/// A frame table up to its end mark, as signed frames.
fn tab(t: &[u8]) -> String {
    lst(t.iter().take_while(|&&b| b != 0xfe).map(|&b| b as i8))
}

/// The seven stars, in the golden's order.
const STARS: [&str; 7] = [
    "ETOILE102",
    "ETOILE103",
    "ETOILE105",
    "ETOILE108",
    "ETOILE111",
    "ETOILE113",
    "ETOILE115",
];

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
        if e.loop_type == LoopType::Warp && !e.turning() {
            break;
        }
    }
    assert_eq!(e.loop_type, LoopType::Warp);
    resumes
}

/// Commands run, then the films and dialogs they start ended.
fn run(e: &mut Engine, commands: &[&[u8]]) {
    for c in commands {
        e.run_command(c);
    }
    settle(e);
}

/// Yucatán's musics loaded as chapter 75's trip loads them, the world 6.
const XIBALBA: [&[u8]; 3] = [
    b"(delmusik=1,2,3,4,5)",
    b"(LOADMUSIK=10,11,12,13,14,15,16)",
    b"(monde=6)",
];

/// A game in the star map's view, chapter 39, as the scripts put it
/// there.
fn in_view_121(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    run(&mut e, &XIBALBA);
    run(
        &mut e,
        &[b"(changewam=xibalba.wam)", b"(vue=121)", b"(chapter=39)"],
    );
    assert_eq!(e.place(), ("xibalba.wam", 121));
    assert_eq!(e.script_state().unwrap().chapter, 39);
    e.trace = true;
    e
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

fn set(e: &mut Engine, name: &str, v: i32) {
    assert!(e.script_state_mut().unwrap().set_var(name, v), "{name}");
}

fn id(e: &Engine, section: usize, name: &str) -> i32 {
    e.script_state()
        .unwrap()
        .names
        .id(section, name.as_bytes())
        .unwrap() as i32
}

/// The golden's fresh state: no hand, an empty bag, the map's variables
/// at 0 but the holes given, `MONDE` = `monde`, view 121 entered again
/// (its slots loaded, the entry hook run).
fn fresh(e: &mut Engine, etat: &[(i32, i32)], monde: i32) {
    settle(e);
    e.run_command(b"(objetenmain=0)");
    let s = e.script_state_mut().unwrap();
    s.bag.clear();
    s.fatal = None;
    for h in 0..17 {
        assert!(s.set_var(&format!("etat_etoile{h}"), 0));
    }
    for (h, v) in etat {
        assert!(s.set_var(&format!("etat_etoile{h}"), *v));
    }
    for (k, v) in [
        ("etoiles_fini", 0),
        ("pz_etoile_started", 0),
        ("Etoiles_JustFinished", 0),
        ("MONDE", monde),
    ] {
        assert!(s.set_var(k, v));
    }
    e.enter_at(121, 0.0, 0.0).unwrap();
    e.cursor.shown = None;
    e.anim.warnings.clear();
    e.events.clear();
}

/// The golden's state: the non-zero holes, the map's table and record,
/// `OBJETENMAIN`, the hand pair, the cursor pair, the bag, the three
/// variables.
fn st(e: &Engine) -> String {
    let s = e.script_state().unwrap();
    let etat = lst((0..17).filter_map(|h| {
        let v = s.var(&format!("etat_etoile{h}")).unwrap();
        (v != 0).then(|| format!("{h}:{v}"))
    }));
    let slot = &e.anim.slots[0];
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
        "etat {etat} tab {} slot0 {}/{}/{} hand {} pair {pair} cursor {cursor} bag {bag} started {} just {} fini {}",
        tab(&slot.frames),
        i32::from(slot.active),
        slot.frame,
        i32::from(slot.playing),
        var(e, "OBJETENMAIN"),
        var(e, "pz_etoile_started"),
        var(e, "Etoiles_JustFinished"),
        var(e, "etoiles_fini"),
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
            Event::Set { chapter: 39, line } => Some(*line),
            _ => None,
        })
        .collect()
}

/// The warnings and the fatal error since the last call, as the golden's
/// (the original's format strings, spaces as `_`). A fatal error ends the
/// original game (an emergency save, then the game quits); the port's
/// scripts stop: it is cleared for the rows after it.
fn warns(e: &mut Engine) -> String {
    let mut w: Vec<String> = Vec::new();
    if let Some(f) = e.script_state_mut().unwrap().fatal.take() {
        let fmt = if f.starts_with("AtlSpw_find_index()") {
            "AtlSpw_find_index(): vue %d ! AtlSpw_find_index(%d) introuvable"
        } else if f.starts_with("DefName_GetIdName: Nom de ") {
            "DefName_GetIdName: Nom de %s inconnu:%s"
        } else {
            panic!("{f}")
        };
        w.push(format!("fatal:{}", fmt.replace(' ', "_")));
    }
    for x in e.anim.warnings.drain(..) {
        w.push(format!("warning:{}", x.replace(' ', "_")));
    }
    lst(w)
}

fn give(e: &mut Engine, name: &str) {
    e.run_command(format!("(sacados={name})").as_bytes());
}

fn hold(e: &mut Engine, name: Option<&str>) {
    e.run_command(format!("(objetenmain={})", name.unwrap_or("0")).as_bytes());
}

fn parse(s: &str) -> Vec<i32> {
    s.trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .filter(|x| !x.is_empty())
        .map(|x| x.parse().unwrap())
        .collect()
}

fn parse_etat(s: &str) -> Vec<(i32, i32)> {
    s.trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .filter(|x| !x.is_empty())
        .map(|x| {
            let (h, v) = x.split_once(':').unwrap();
            (h.parse().unwrap(), v.parse().unwrap())
        })
        .collect()
}

#[test]
fn the_entry_click_and_win_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_view_121(files);
    let names = e.script_state().unwrap().names.clone();
    let obj = |n: &str| names.id(defnames::OBJETS, n.as_bytes()).unwrap() as i32;
    let ids: Vec<String> = ["Etoiles_Puz"]
        .iter()
        .chain(STARS.iter())
        .chain(["OBSIDIENNE1"].iter())
        .map(|n| obj(n).to_string())
        .collect();
    assert_eq!(
        rows("ids objects")[0],
        format!("ids objects {}", ids.join(" "))
    );
    let params: Vec<String> = STARS
        .iter()
        .map(|n| names.param(defnames::OBJETS, n.as_bytes()).to_string())
        .collect();
    assert_eq!(rows("params ")[0], format!("params {}", params.join(" ")));
    let map = obj("Etoiles_Puz");
    assert_eq!(e.anim.slots[0].id, map);

    // B: the entry hook; the table filled before it (pre) on a view
    // entered with the holes given (whose own entry the hook redoes).
    let mut got = Vec::new();
    for row in rows("B etat") {
        let w: Vec<&str> = row.split(' ').collect();
        let etat = parse_etat(w[2]);
        let pre = parse(w[4]);
        let pre_slot: Vec<i32> = if w[7] == "-" {
            Vec::new()
        } else {
            w[7].split('/').map(|x| x.parse().unwrap()).collect()
        };
        let times: usize = w[9].parse().unwrap();
        fresh(&mut e, &etat, 6);
        e.stars_sprite = 0;
        for &f in &pre {
            assert!(e.anim.add_frame(map, f));
        }
        if let [a, f, p] = pre_slot[..] {
            let s = &mut e.anim.slots[0];
            s.active = a != 0;
            s.frame = f;
            s.playing = p != 0;
        }
        for _ in 0..times {
            e.stars_entry();
        }
        let w = warns(&mut e);
        got.push(format!(
            "{} -> {} | kept {} 0 | warn {w}",
            row.split(" -> ").next().unwrap(),
            st(&e),
            e.stars_sprite,
        ));
    }
    assert_eq!(got, rows("B etat"));
    // A second slot of the id after the others (the WAM has one).
    fresh(&mut e, &[(2, obj("ETOILE102"))], 6);
    let mut second = e.anim.slots[0].clone();
    second.active = false;
    second.frame = 0;
    second.playing = true;
    second.frames = [0xfe; 128];
    e.anim.slots.push(second);
    let n = e.anim.slots.len() - 1;
    assert!(e.anim.add_frame(map, 5));
    e.stars_entry();
    let s = &e.anim.slots[n];
    let extra = format!(
        "slot{n} {}/{}/{} tab {}",
        i32::from(s.active),
        s.frame,
        i32::from(s.playing),
        tab(&s.frames)
    );
    let w = warns(&mut e);
    assert_eq!(
        format!(
            "B two slots of the id, etat [2:{}] pre [5] -> {} | kept {} 0 | warn {w} | {extra}",
            obj("ETOILE102"),
            st(&e),
            e.stars_sprite,
        ),
        rows("B two slots")[0]
    );
    // No slot of the id: FUN_0047de60's fatal error (the original's rows
    // after it are what its code does once the logging returns, which in
    // the game it does not: the port stops there).
    fresh(&mut e, &[(2, obj("ETOILE102"))], 6);
    e.anim.slots.remove(0);
    e.stars_sprite = 0;
    e.stars_entry();
    let fatal = e.script_state().unwrap().fatal.clone().unwrap();
    assert_eq!(
        fatal,
        format!("AtlSpw_find_index(): vue 121 ! AtlSpw_find_index({map}) introuvable")
    );
    let row = rows("B no slot")[0];
    assert!(row.contains(&format!("kept {} 0", e.stars_sprite)));
    let w = warns(&mut e);
    assert!(row.contains(&format!("warn {}", w.trim_end_matches(']'))));

    // C: the click hook alone.
    let mut got = Vec::new();
    fn c(e: &mut Engine, label: &str, v: i32, t: i32, got: &mut Vec<String>) {
        e.events.clear();
        e.stars_click(t, v);
        let evs = std::mem::take(&mut e.events);
        let win = evs
            .iter()
            .filter(|x| {
                matches!(
                    x,
                    Event::PuzzleRan {
                        function: 0x0049_4f60
                    }
                )
            })
            .count();
        let fatal = e.script_state().unwrap().fatal.is_some();
        // The original goes on to the win test after a fatal lookup of
        // the hole (the logging returns in the harness): the port stops.
        let win = if fatal { 1 } else { win };
        got.push(format!(
            "C {label} zone {t} {v} -> {} | win test {win} | lines {} | sounds {} | warn {}",
            st(e),
            lst(lines(&evs)),
            sounds(&evs),
            warns(e)
        ));
    }
    let st0 = obj("ETOILE102");
    let seven: Vec<(i32, i32)> = STAR_HOLES.iter().map(|&h| (h, st0)).collect();
    fresh(&mut e, &[], 6);
    c(&mut e, "empty", 3, 50, &mut got);
    for s in STARS {
        give(&mut e, s);
    }
    give(&mut e, "OBSIDIENNE1");
    got.push(format!("C given -> {}", st(&e)));
    hold(&mut e, Some("ETOILE102"));
    c(&mut e, "ETOILE102 type 4", 3, 4, &mut got);
    c(&mut e, "ETOILE102 type 3", 3, 3, &mut got);
    c(&mut e, "ETOILE102 type 1", 3, 1, &mut got);
    c(&mut e, "ETOILE102", 3, 50, &mut got);
    hold(&mut e, Some("ETOILE103"));
    c(&mut e, "ETOILE103 on the full hole", 3, 50, &mut got);
    hold(&mut e, None);
    c(&mut e, "empty on the full hole", 3, 50, &mut got);
    c(&mut e, "ETOILE102 back in hand", 4, 50, &mut got);
    hold(&mut e, Some("OBSIDIENNE1"));
    c(&mut e, "OBSIDIENNE1", 5, 50, &mut got);
    hold(&mut e, None);
    c(&mut e, "empty", 5, 50, &mut got);
    hold(&mut e, Some("ETOILE115"));
    c(&mut e, "ETOILE115", 16, 50, &mut got);
    hold(&mut e, Some("ETOILE103"));
    c(&mut e, "ETOILE103", 0, 50, &mut got);
    for (s, h) in [
        ("ETOILE105", 2),
        ("ETOILE108", 6),
        ("ETOILE111", 8),
        ("ETOILE113", 11),
    ] {
        hold(&mut e, Some(s));
        c(&mut e, s, h, 50, &mut got);
    }
    hold(&mut e, Some("ETOILE102"));
    c(&mut e, "ETOILE102 the seventh", 13, 50, &mut got);
    // The win started film 49: the panorama back (the section's end), as
    // the golden's rows after it have no scan.
    assert_eq!(e.loop_type, LoopType::Video);
    settle(&mut e);
    // The passes chose the cursor again: the golden's has none.
    e.cursor.shown = None;
    e.events.clear();
    c(&mut e, "after the win, empty", 13, 50, &mut got);
    hold(&mut e, Some("OBSIDIENNE1"));
    c(&mut e, "after the win, OBSIDIENNE1", 1, 50, &mut got);
    let wrong: Vec<(i32, i32)> = [1, 3, 4, 5, 7, 9, 10, 12, 14, 15]
        .iter()
        .map(|&h| (h, st0))
        .collect();
    fresh(&mut e, &wrong, 6);
    for s in STARS {
        give(&mut e, s);
    }
    for (s, h) in STARS.iter().zip([0, 2, 6, 8, 11, 13]) {
        hold(&mut e, Some(s));
        c(&mut e, s, h, 50, &mut got);
    }
    hold(&mut e, None);
    c(&mut e, "empty, a right hole", 13, 50, &mut got);
    c(&mut e, "ETOILE113 back in hand", 16, 50, &mut got);
    c(&mut e, "ETOILE113 back in hand", 13, 50, &mut got);
    fresh(&mut e, &seven, 6);
    hold(&mut e, None);
    c(&mut e, "seven held, empty", 5, 50, &mut got);
    fresh(&mut e, &seven, 6);
    set(&mut e, "etoiles_fini", 2);
    c(&mut e, "seven held, etoiles_fini 2, empty", 5, 50, &mut got);
    fresh(&mut e, &seven[..6], 6);
    set(&mut e, "etoiles_fini", 1);
    give(&mut e, "ETOILE115");
    hold(&mut e, Some("ETOILE115"));
    c(&mut e, "etoiles_fini 1, ETOILE115", 16, 50, &mut got);
    fresh(&mut e, &[(4, 7)], 6);
    c(&mut e, "empty on a hole holding 7", 4, 50, &mut got);
    give(&mut e, "OBSIDIENNE1");
    hold(&mut e, Some("OBSIDIENNE1"));
    c(&mut e, "OBSIDIENNE1 on the emptied hole", 4, 50, &mut got);
    for h in [
        obj("ETOILE102") - 1,
        obj("ETOILE115") + 1,
        obj("ETOILE102") + 1,
    ] {
        fresh(&mut e, &[], 6);
        set(&mut e, "ObjetEnMain", h);
        c(
            &mut e,
            &format!("ObjetEnMain {h} (no bag)"),
            7,
            50,
            &mut got,
        );
    }
    fresh(&mut e, &[], 2);
    give(&mut e, "ETOILE108");
    hold(&mut e, Some("ETOILE108"));
    c(&mut e, "MONDE 2 ETOILE108", 9, 50, &mut got);
    c(&mut e, "MONDE 2 empty", 9, 50, &mut got);
    fresh(&mut e, &[], 6);
    for v in [17, -1] {
        c(&mut e, "empty", v, 50, &mut got);
    }
    // The golden's rows of the ids of its first lookups after a fatal
    // one are the original's alone.
    assert_eq!(
        got,
        rows("C ")
            .into_iter()
            .filter(|r| !r.starts_with("C win"))
            .collect::<Vec<_>>()
    );

    // C win: the win test alone.
    let mut got = Vec::new();
    for row in rows("C win") {
        let holes = parse(row.split(' ').nth(2).unwrap());
        let etat: Vec<(i32, i32)> = holes.iter().map(|&h| (h, st0)).collect();
        fresh(&mut e, &etat, 6);
        e.stars_win_test();
        let evs = std::mem::take(&mut e.events);
        let l = lines(&evs);
        got.push(format!(
            "C win {} -> just {} scan {} lines {}",
            lst(&holes),
            var(&e, "Etoiles_JustFinished"),
            i32::from(!l.is_empty()),
            lst(l)
        ));
    }
    assert_eq!(got, rows("C win"));

    // D: the dispatchers on the warp names and numbers of the golden.
    let mut got = Vec::new();
    for row in rows("D ").into_iter().filter(|r| !r.starts_with("D entry")) {
        let w: Vec<&str> = row.split(' ').collect();
        let (name, number, kind): (&str, u32, i32) =
            (w[1], w[2].parse().unwrap(), w[3].parse().unwrap());
        fresh(&mut e, &[], 6);
        give(&mut e, "ETOILE102");
        hold(&mut e, Some("ETOILE102"));
        let zone = Zone {
            kind,
            value: 3,
            ..Default::default()
        };
        let consumed = e.yucatan_click(name.as_bytes(), number, &zone);
        let entered: Vec<String> = e
            .events
            .iter()
            .filter_map(|x| match x {
                Event::PuzzleRan { function } => Some(format!("{function:08x}")),
                _ => None,
            })
            .collect();
        let entered = if entered.is_empty() {
            "-".to_string()
        } else {
            entered.join(",")
        };
        got.push(format!(
            "D {name} {number} {kind} -> {entered} AL {} etat3 {}",
            i32::from(consumed),
            var(&e, "etat_etoile3")
        ));
    }
    for row in rows("D entry") {
        let w: Vec<&str> = row.split(' ').collect();
        let (name, number): (&str, u32) = (w[2], w[3].parse().unwrap());
        let hooks = yucatan_entry_hooks(name.as_bytes(), number, 0);
        e.stars_sprite = 0;
        if hooks.contains(&0x0049_4e00) {
            e.stars_entry();
        }
        let entered = if hooks.is_empty() {
            "-".to_string()
        } else {
            hooks
                .iter()
                .map(|f| format!("{f:08x}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        got.push(format!(
            "D entry {name} {number} -> {entered} kept {}",
            e.stars_sprite
        ));
    }
    assert_eq!(got, rows("D "));

    // E: the click as the game runs it, then the section resumed by the
    // film's end.
    let mut got = Vec::new();
    fresh(&mut e, &[], 6);
    for s in STARS {
        give(&mut e, s);
    }
    set(&mut e, "ClicZone", 0);
    let mut plays: Vec<(&str, Option<&str>, i32)> =
        vec![("ETOILE102", Some("ETOILE102"), 5), ("empty", None, 5)];
    for (s, h) in STARS.iter().zip(STAR_HOLES) {
        plays.push((s, Some(s), h));
    }
    plays.push(("after the win, empty", None, 16));
    for (label, held, v) in plays {
        hold(&mut e, held);
        let zone = e
            .wam()
            .view(121)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 50 && z.value == v)
            .unwrap();
        let (yaw, pitch) = e.zone_aim(121, zone).unwrap();
        e.camera.enter(yaw, pitch);
        e.pass(None);
        assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zone));
        e.events.clear();
        e.click(100.0 * f64::from(v) + got.len() as f64 * 1000.0);
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
        let after = evs
            .iter()
            .find_map(|x| match x {
                Event::ZoneClick { zone: z, cliczone } if *z == zone => Some(*cliczone),
                _ => None,
            })
            .expect("the zone click");
        let scan = evs.iter().position(|e| matches!(e, Event::ClickScan(_)));
        let (end, scanned) = match scan {
            Some(n) => (n, 1),
            None => (
                evs.iter()
                    .position(|e| matches!(e, Event::VideoEnd))
                    .unwrap_or(evs.len()),
                0,
            ),
        };
        // The cursor pair as the golden leaves it is the hook's; the
        // passes after it choose the cursor again (C rows check it).
        let state = st(&e);
        let state = format!(
            "{} cursor *{}",
            state.split(" cursor ").next().unwrap(),
            state
                .split_once(" bag ")
                .map(|x| format!(" bag {}", x.1))
                .unwrap()
        );
        got.push(format!(
            "E {label} zone 50 {v} -> out {} | hook cliczone {at_hook} lines {} sounds {} | cliczone {after} | click scan {scanned} | in {} resumes {resumes} | {state}",
            lst(lines(&evs[..hook])),
            lst(lines(&evs[hook..end])),
            sounds(&evs[hook..end]),
            lst(lines(&evs[end..])),
        ));
    }
    let blank = |r: &str| {
        let (a, b) = r.split_once(" cursor (").unwrap();
        format!("{a} cursor * bag {}", b.split_once(" bag ").unwrap().1)
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
fn the_map_is_drawn_from_its_frame_table_as_the_original() {
    let Some(files) = game() else { return };
    let data = files
        .read(Folder::Spw, "epa1121121.SPW")
        .expect("the map's SPW");
    let bg = background();
    let bg_md5 = md5_of(&bg);
    assert!(GOLDEN.contains(&format!("md5 {bg_md5}")));
    let golden_md5 = format!("{:x}", Md5::digest(&data));
    assert!(GOLDEN.contains(&format!("epa1121121.SPW (md5 {golden_md5}, 17 frames)")));
    let spw = Spw::parse(data.clone()).unwrap();
    let mut got = Vec::new();
    let golden: Vec<&str> = rows("G tab");
    for row in &golden {
        let table = parse(row.split(' ').nth(2).unwrap());
        let mut a = Anim::new(0);
        a.load(
            "ep",
            Spw::parse(data.clone()).unwrap(),
            2,
            150,
            0,
            EntityState {
                letter: 65,
                anim: 1,
                shown: 1,
                single: 0,
                ..Default::default()
            },
        );
        for (k, &f) in table.iter().enumerate() {
            a.slots[0].frames[k] = f as u8;
        }
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
}

// ---------------------------------------------------------------------------
// The puzzle played with the mouse.

/// The engine at 59.94 passes a second ([`PASS`]), a 15 Hz tick every fourth, every
/// pass drawn; the mouse as a player moves it.
struct Run {
    e: Engine,
    pass: u64,
    gap: f64,
    screen: Vec<u16>,
    shots: Option<PathBuf>,
    /// The signs of the mouse axes for the view's turn, found by the
    /// first moves.
    mouse_signs: (f32, f32),
}

/// The angles of the ray through the centre of texel (`col`, `row`) of
/// face `face` (the cube's faces as the warp samples them).
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

    /// The HD view at 1920x1440 (no sharpening, the panorama bilinear)
    /// of the frame a pass draws: the layer's 640x480 base, and its pixels
    /// with the width and height; the HD mode off again after.
    #[allow(clippy::type_complexity)]
    fn hd_view(&mut self) -> (Vec<u16>, (Vec<u32>, usize, usize)) {
        self.e.enhancements = Enhancements {
            hd: true,
            ..Enhancements::default()
        };
        self.e.enhancements.hd_options.sharpen = false;
        self.e.enhancements.hd_options.panorama = atlantis2_engine::hd::PanoFilter::Bilinear;
        self.e.set_hd_output(Some(48), 0);
        self.step();
        let l = self.e.hd_layer().expect("the HD layer");
        let v = (l.base.clone(), (l.pixels.clone(), l.width, l.height));
        self.e.enhancements = Enhancements::default();
        self.e.set_hd_output(None, 0);
        v
    }

    fn shot(&self, name: &str) {
        if let Some(dir) = &self.shots {
            std::fs::write(
                dir.join(format!("pz-stars-{name}.png")),
                encode_rgb565(&self.screen, 640, 480),
            )
            .unwrap();
        }
    }

    fn var(&self, name: &str) -> i32 {
        var(&self.e, name)
    }

    fn obj(&self, name: &str) -> i32 {
        id(&self.e, defnames::OBJETS, name)
    }

    fn in_bag(&self, name: &str) -> bool {
        let id = self.obj(name);
        self.e.bag_entries().iter().any(|b| b.id == id)
    }

    fn chapter(&self) -> i32 {
        self.e.script_state().unwrap().chapter
    }

    /// Turns the view with the mouse, as a player does, toward (`yaw`,
    /// `pitch`) until `on` holds with the turn spent (a move of d mickeys
    /// turns the view by about d / 500 radians in all). False when `on`
    /// never holds.
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

    /// Turns the view with the mouse onto zone `zone` (the centre on it,
    /// no sprite under it), a little off the zone's own aim as a player's
    /// hand is, then half way to each of its corners.
    fn aim(&mut self, zone: usize) {
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
        assert!(
            targets.into_iter().any(|(y, p)| self.steer(y, p, &on)),
            "the centre never reached zone {zone} of view {view}"
        );
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

    /// The shortest walk to view `to` by the place's move zones.
    fn go(&mut self, to: u32) -> Vec<Event> {
        let from = self.e.place().1;
        let wam = self.e.wam();
        let mut prev = std::collections::HashMap::new();
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some(v) = queue.pop_front() {
            if v == to {
                break;
            }
            for z in wam.view(v).map(|w| w.zones.as_slice()).unwrap_or_default() {
                let n = z.value as u32;
                if z.kind != 1 || n == from || prev.contains_key(&n) || wam.view(n).is_none() {
                    continue;
                }
                prev.insert(n, v);
                queue.push_back(n);
            }
        }
        let mut path = vec![to];
        while *path.last().unwrap() != from {
            let at = *path.last().unwrap();
            path.push(
                *prev
                    .get(&at)
                    .unwrap_or_else(|| panic!("no way from view {from} to {to}")),
            );
        }
        path.pop();
        path.reverse();
        let mut all = Vec::new();
        for v in path {
            all.extend(self.walk(v));
        }
        all
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

    /// A film clicked away after a second, a dialog listened to, until
    /// the panorama is back and still.
    fn settle(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        let mut still = 0;
        for _ in 0..20_000 {
            match self.e.loop_type {
                LoopType::Video => {
                    all.extend(self.events());
                    for _ in 0..60 {
                        all.extend(self.events());
                    }
                    if self.e.loop_type == LoopType::Video {
                        self.tap();
                    }
                    still = 0;
                }
                LoopType::Dialog => {
                    all.extend(self.listen());
                    still = 0;
                }
                _ => {
                    all.extend(self.events());
                    still += 1;
                    if still > 20 && !self.e.turning() {
                        return all;
                    }
                }
            }
        }
        panic!("the game never settles");
    }

    /// The bag bar opened with the right button and object `name`'s cell
    /// clicked: the object in hand, the bar closed.
    fn pick(&mut self, name: &str) {
        let id = self.obj(name);
        if self.e.bar.open {
            self.e.right_click();
            self.step();
        }
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

    /// The object in hand put back in the bag (the bar opened and closed).
    fn stow(&mut self) {
        if self.var("OBJETENMAIN") != 0 {
            self.e.right_click();
            self.step();
            self.e.right_click();
            self.step();
            assert_eq!(self.var("OBJETENMAIN"), 0);
        }
    }

    /// The sprite a click would hit now: the first shown clickable slot
    /// whose box on the face under the centre holds the centre's texel.
    fn centre_slot(&self) -> Option<usize> {
        let e = &self.e;
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

    /// A click on the shown sprite of character (`character`) or object
    /// `id`, turned to with the mouse (the middle of its box first): the
    /// click's events.
    fn click_on(&mut self, character: bool, id: i32) -> Vec<Event> {
        let n = self
            .e
            .anim
            .slots
            .iter()
            .position(|s| {
                s.id == id && s.active && s.click != 0 && matches!(s.kind, 1 | 3) == character
            })
            .unwrap_or_else(|| panic!("{id} not shown in view {}", self.e.place().1));
        let mut aims = Vec::new();
        for face in 0..6 {
            let Some(b) = self.e.anim.slots[n].frame_box(face) else {
                continue;
            };
            if b.w == 0 || b.h == 0 {
                continue;
            }
            for (i, j) in [(4, 4), (3, 4), (5, 4), (4, 3), (4, 5)] {
                let col = i32::from(b.x) + 1 + (i32::from(b.w) - 1) * i / 8;
                let row = i32::from(b.y) + 1 + (i32::from(b.h) - 1) * j / 8;
                aims.push(texel_angles(face, f64::from(col), f64::from(row)));
            }
        }
        for (yaw, pitch) in aims {
            let on = |e: &Engine| e.sprite_under_centre().1 != 0;
            if self.steer(yaw, pitch, &on) && self.centre_slot() == Some(n) {
                self.tap();
                return self.events();
            }
        }
        panic!("{id} not clickable in view {}", self.e.place().1);
    }

    /// Object `name` taken with a click on its sprite, put in the bag.
    fn take(&mut self, name: &str) {
        let id = self.obj(name);
        let evs = self.click_on(false, id);
        assert!(evs.contains(&Event::Take { id }), "{name} not taken");
        self.settle();
        assert!(self.in_bag(name), "{name} not in the bag");
        self.stow();
    }

    /// The hole `h` of the map (its zone of type 0x32).
    fn hole(&self, h: i32) -> usize {
        self.e
            .wam()
            .view(121)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 50 && z.value == h)
            .unwrap()
    }

    /// A click on hole `h` with what is in hand: the click's events,
    /// played out.
    fn click_hole(&mut self, h: i32) -> Vec<Event> {
        let z = self.hole(h);
        self.aim(z);
        self.tap();
        let mut evs = self.events();
        assert!(evs.contains(&Event::PuzzleRan {
            function: 0x0049_4e80
        }));
        evs.extend(self.settle());
        evs
    }

    /// Star `name` picked from the bag and laid on hole `h`.
    fn lay(&mut self, name: &str, h: i32) -> Vec<Event> {
        self.pick(name);
        let star = self.obj(name);
        let evs = self.click_hole(h);
        assert!(evs.contains(&Event::StarLaid { hole: h, star }));
        assert!(evs.contains(&Event::Sound("sound115.apc".into())));
        evs
    }

    /// The star on hole `h` taken back with the empty hand: the star.
    fn take_back(&mut self, h: i32) -> i32 {
        self.stow();
        let star = self.var(&format!("etat_etoile{h}"));
        let evs = self.click_hole(h);
        assert!(evs.contains(&Event::StarTaken { hole: h, star }));
        assert!(evs.contains(&Event::Sound("sound114.apc".into())));
        assert_eq!(self.var("OBJETENMAIN"), star);
        star
    }

    /// The map's frame list (the `Etoiles_Puz` slot's).
    fn map(&self) -> Vec<i32> {
        let id = self.obj("Etoiles_Puz");
        let n = self.e.anim.slots.iter().position(|s| s.id == id).unwrap();
        self.e.anim.frame_list(n)
    }

    /// Character `who`'s subject `subject` chosen in the menu its click
    /// (on `zone`, a zone of the view, or on its sprite) opens: the
    /// dialog its answer starts, listened to.
    fn ask(&mut self, who: &str, subject: &str, zone: Option<usize>) -> u32 {
        let character = id(&self.e, defnames::PERSONNAGES, who);
        let value = self
            .e
            .script_state()
            .unwrap()
            .names
            .param(defnames::SUJETS, subject.as_bytes());
        self.stow();
        let evs = match zone {
            Some(z) => {
                self.aim(z);
                self.tap();
                self.events()
            }
            None => self.click_on(true, character),
        };
        assert!(
            evs.iter()
                .any(|e| matches!(e, Event::SubjectMenu { character: c, .. } if *c == character)),
            "{who}: no subject menu"
        );
        let list = self.e.script_state().unwrap().subjects.list(character);
        let i = list
            .iter()
            .position(|e| e.value == value)
            .unwrap_or_else(|| panic!("{who} offers no {subject}"));
        let (x, y) = cell(list.len() as i32, i as i32);
        self.point(x + 31, y + 31);
        self.step();
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
            .unwrap_or_else(|| panic!("{who} {subject}: no dialog"));
        self.settle();
        number
    }
}

#[test]
fn the_star_map_played_with_the_mouse_from_chapter_39() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    // Xibalba as the chapters before 39 leave it: the canoe won
    // (chapter 35: `finpirogue`), the bat's first talk heard (`flag102`),
    // the clearing reached by the river's link (view 124, chapter 39).
    run(&mut e, &XIBALBA);
    run(
        &mut e,
        &[
            b"(finpirogue=1)",
            b"(flag102=1)",
            b"(changewam=xibalba.wam)",
            b"(vue=124)",
            b"(chapter=39)",
        ],
    );
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
    assert_eq!((r.e.place(), r.chapter()), (("xibalba.wam", 124), 39));

    // The seven stars, each taken in its view of the clearing.
    for (star, view) in [
        ("ETOILE105", 105),
        ("ETOILE102", 102),
        ("ETOILE103", 103),
        ("ETOILE108", 108),
        ("ETOILE111", 111),
        ("ETOILE113", 113),
        ("ETOILE115", 115),
    ] {
        r.go(view);
        r.take(star);
    }
    assert!(STARS.iter().all(|s| r.in_bag(s)));

    // The map: its entry runs, no star drawn.
    let evs = r.go(121);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_4e00
    }));
    assert!(r.map().is_empty());
    r.shot("1-the-map");

    // A star on a wrong hole, then taken back with the empty hand.
    r.lay("ETOILE102", 5);
    assert_eq!(r.var("etat_etoile5"), r.obj("ETOILE102"));
    assert_eq!(r.var("pz_etoile_started"), 1);
    assert!(!r.in_bag("ETOILE102"));
    assert_eq!(r.map(), [5]);
    r.wait(10);
    r.shot("2-a-star-laid");
    assert_eq!(r.take_back(5), r.obj("ETOILE102"));
    assert!(r.in_bag("ETOILE102"));
    assert!(r.map().is_empty());
    r.stow();

    // All seven on wrong holes: no win, no star left in the bag.
    let wrong = [1, 3, 4, 5, 7, 9, 10];
    for (s, h) in STARS.iter().zip(wrong) {
        r.lay(s, h);
    }
    assert_eq!(r.var("Etoiles_JustFinished"), 0);
    assert_eq!(r.var("etoiles_fini"), 0);
    assert_eq!(r.map(), wrong);
    assert!(STARS.iter().all(|s| !r.in_bag(s)));
    r.wait(10);
    r.shot("3-all-wrong");

    // The map kept by a save: loaded, the entry draws it again.
    let saved = r.e.save_data();
    r.e.load_data(&saved).unwrap();
    r.settle();
    assert_eq!(r.e.place(), ("xibalba.wam", 121));
    assert_eq!(r.map(), wrong);

    // The bat's island (chapter 35, its `/INIT`): with no star in the bag,
    // `pz_etoile_started` alone offers the bat its subject about the
    // stars (line 22); the bat asked: dialog 177 (lines 180-181).
    run(
        &mut r.e,
        &[b"(changewam=bat.wam)", b"(vue=403)", b"(chapter=35)"],
    );
    r.e.events.clear();
    assert_eq!(r.e.script_state().unwrap().flags[107], 1);
    r.wait(20);
    assert_eq!(r.ask("CHAUVE_SOURIS", "ETOILE", None), 177);

    // Back in the clearing: the map drawn from the holes again; each star
    // taken from its wrong hole and laid on a right one. The seventh wins:
    // chapter 39's section (lines 27-33: `etoiles_fini`, `flag85`, film
    // 49, the statue shown, its zones enabled).
    run(
        &mut r.e,
        &[b"(changewam=xibalba.wam)", b"(vue=121)", b"(chapter=39)"],
    );
    r.e.events.clear();
    r.wait(20);
    assert_eq!(r.map(), wrong);
    let mut evs = Vec::new();
    let mut before = None;
    for (k, (&w, h)) in wrong.iter().zip(STAR_HOLES).enumerate() {
        let star = r.take_back(w);
        if k == 6 {
            r.shot("4-the-last-star");
            // The last hole aimed at, still empty: its HD view kept.
            let z = r.hole(h);
            r.aim(z);
            before = Some((r.e.camera.clone(), r.hd_view()));
        }
        evs = r.click_hole(h);
        assert!(evs.contains(&Event::StarLaid { hole: h, star }));
    }
    assert!(evs.iter().any(
        |x| matches!(x, Event::VideoStart { name, frames: Some(_) } if name.eq_ignore_ascii_case("cine049.hnm"))
    ));
    let set: Vec<usize> = evs
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter: 39, line } => Some(*line),
            _ => None,
        })
        .collect();
    assert_eq!(set, [27, 28, 29, 30, 31, 32, 33]);
    assert_eq!(
        (
            r.var("etoiles_fini"),
            r.var("Etoiles_JustFinished"),
            r.e.script_state().unwrap().flags[85]
        ),
        (1, 0, 1)
    );
    let mut sorted = r.map();
    sorted.sort_unstable();
    assert_eq!(sorted, STAR_HOLES);
    r.wait(10);
    r.shot("5-won");
    // Done: a hole clicked with the empty hand takes nothing.
    let evs = r.click_hole(0);
    assert!(!evs.iter().any(|x| matches!(x, Event::StarTaken { .. })));
    assert_eq!(r.var("OBJETENMAIN"), 0);

    // The HD view of the won map, the last hole aimed at as before its
    // star: the 640x480 frame changed around the centre (the star laid),
    // and there the HD view shows the frame's pixels, not those it showed
    // before.
    let (cam, (base0, hd0)) = before.unwrap();
    r.e.camera = cam;
    let (base1, hd1) = r.hd_view();
    if let Some(dir) = &r.shots {
        std::fs::write(
            dir.join("pz-stars-6-won-hd.png"),
            encode_xrgb8888(&hd1.0, hd1.1, hd1.2),
        )
        .unwrap();
    }
    let lut = atlantis2_engine::hd::rgb565_to_888();
    let close = |a: u32, b: u32| {
        (0..3)
            .map(|k| ((a >> (8 * k)) & 0xff).abs_diff((b >> (8 * k)) & 0xff))
            .max()
            .unwrap()
            <= 48
    };
    let at = |l: &(Vec<u32>, usize, usize), x: usize, y: usize| l.0[(3 * y + 1) * l.1 + 3 * x + 1];
    let star: Vec<(usize, usize)> = (140..340)
        .flat_map(|y| (220..420).map(move |x| (x, y)))
        .filter(|&(x, y)| base0[y * 640 + x] != base1[y * 640 + x])
        .collect();
    let shown = star
        .iter()
        .filter(|&&(x, y)| close(at(&hd1, x, y), lut[usize::from(base1[y * 640 + x])]))
        .count();
    let changed = star
        .iter()
        .filter(|&&(x, y)| at(&hd1, x, y) != at(&hd0, x, y))
        .count();
    eprintln!(
        "the last star: {} pixels, {shown} shown, {changed} changed in HD",
        star.len()
    );
    assert!(star.len() >= 20, "{} pixels of the last star", star.len());
    assert!(
        shown * 10 >= star.len() * 9,
        "{shown} of {} shown",
        star.len()
    );
    assert!(
        changed * 10 >= star.len() * 9,
        "{changed} of {} changed",
        star.len()
    );

    // View 122: the statue of Tezcatlipoca, shown by the win, asked
    // through its zone.
    r.walk(122);
    let z = {
        let v = id(&r.e, defnames::ZONES_CLIC, "ClicTezStatue");
        r.e.wam()
            .view(122)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.value == v && z.kind < 88888)
            .expect("the statue's zone enabled")
    };
    r.shot("7-the-statue");
    assert_eq!(r.ask("TEZ_STATUE", "TEZ", Some(z)), 193);
}

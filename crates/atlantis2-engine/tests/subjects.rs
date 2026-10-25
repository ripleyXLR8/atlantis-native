//! The subject menu against the original's code run under emulation
//! (`tests/golden/subjects.txt`, header there): the cells it draws and the
//! cell a click picks for every list length, the shaman's subject lists
//! along chapter 75 as the questions and the story change them, and the
//! menu in the warp frame (opening, drawing, closing, a choice that starts
//! a dialog). Then a game asks the two shamans what can be asked before a
//! boat is placed (the golden's first ten questions), as a player does:
//! the view turned to put him under the centre, a click, the pointer moved
//! to a cell, a click, the dialog listened to; and the astral journey of
//! view 10, offered by the scripts' key. The later questions (the astral
//! journey in view 7, the crystal's other dialogs, chapter 78) need a boat
//! placed and the travel to a world: the chapter test asks them.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::collections::BTreeMap;
use std::path::PathBuf;

use atlantis2_data::defnames::{self, DefNames};
use atlantis2_data::spr::SprBank;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::anim::{MsvcRand, Texel};
use atlantis2_engine::bag::{UiBank, UiDraw};
use atlantis2_engine::script::{DialStart, Host, Script, SubjectTable};
use atlantis2_engine::subjects::{cell, first_row, hit};
use atlantis2_engine::{CentreDraw, CursorBank, Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/subjects.txt");

/// The entry values and flags of the golden's layout rows.
const LAYOUT_VALUES: [i32; 16] = [10, 12, 1, 4, 2, 6, 13, 3, 5, 7, 8, 9, 11, 14, 0, 1];
const LAYOUT_FLAGS: [u8; 16] = [1, 0, 1, 2, 1, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 1];
/// The golden's hit region.
const HIT_X: std::ops::Range<i32> = 380..640;
const HIT_Y: std::ops::Range<i32> = 60..480;

fn install() -> Option<PathBuf> {
    std::env::var_os("ATLANTIS2_DATA").map(PathBuf::from)
}

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (install(), std::env::var_os("ATLANTIS2_DISCS")) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) = A2Files::open_dir(&dir, &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn scenar(name: &str) -> Vec<u8> {
    std::fs::read(install().unwrap().join("scenar").join(name)).expect(name)
}

fn hex(d: &[u8]) -> String {
    Md5::digest(d).iter().map(|b| format!("{b:02x}")).collect()
}

/// The golden's rows starting with `prefix`.
fn rows(prefix: &str) -> Vec<&'static str> {
    GOLDEN.lines().filter(|l| l.starts_with(prefix)).collect()
}

/// `{S:I}` → the name of entry I of section S.
fn fill(names: &DefNames, s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let j = rest[i..].find('}').unwrap() + i;
        let f: Vec<usize> = rest[i + 1..j]
            .split(':')
            .map(|x| x.parse().unwrap())
            .collect();
        let name = names.name(f[0], f[1]).unwrap();
        out.extend(name.iter().map(|&c| char::from(c)));
        rest = &rest[j + 1..];
    }
    out.push_str(rest);
    out
}

/// A table holding one character's record: `count`, then the entries
/// (value, index i, flag).
fn table_with(character: i32, count: i32, values: &[i32], flags: &[u8]) -> SubjectTable {
    let mut bytes = vec![0u8; SubjectTable::CHARACTERS * SubjectTable::RECORD];
    let at = character as usize * SubjectTable::RECORD;
    bytes[at..at + 4].copy_from_slice(&count.to_le_bytes());
    for (i, (v, f)) in values.iter().zip(flags).enumerate() {
        let e = at + 4 + 12 * i;
        bytes[e..e + 4].copy_from_slice(&v.to_le_bytes());
        bytes[e + 4..e + 8].copy_from_slice(&(i as i32).to_le_bytes());
        bytes[e + 8] = *f;
    }
    SubjectTable::from_bytes(&bytes)
}

fn list(t: &SubjectTable, character: i32) -> String {
    let l: Vec<String> = t
        .list(character)
        .iter()
        .map(|e| format!("{}/{}/{}", e.value, e.index, e.flag))
        .collect();
    format!(
        "{}:{}",
        t.count(character),
        if l.is_empty() {
            "-".into()
        } else {
            l.join(";")
        }
    )
}

/// The x the draw leaves in its column register: the last column it
/// moved to.
fn last_column(count: i32) -> i32 {
    if count < 1 {
        450
    } else {
        cell(count, count - 1).0
    }
}

/// The engine at 60 passes a second; ticks with the clock.
struct Run {
    e: Engine,
    pass: u64,
    clicks: f64,
}

impl Run {
    fn step(&mut self) -> Vec<Event> {
        self.e.set_clock(self.pass as f64 / 60.0);
        self.e.pass(None);
        self.pass += 1;
        std::mem::take(&mut self.e.events)
    }

    fn click(&mut self) {
        // Clicks far apart: never a double click.
        self.clicks += 10.0;
        self.e.click(self.pass as f64 / 60.0 + self.clicks);
    }

    fn until(&mut self, max: u32, f: impl Fn(&Event) -> bool) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..max {
            let evs = self.step();
            let hit = evs.iter().any(&f);
            all.extend(evs);
            if hit {
                return all;
            }
        }
        panic!("not reached in {max} passes: {all:?}");
    }
}

/// A new game played into chapter 75 on view 2: the film clicked away,
/// then the first click (the departure scan of chapter 0 loads it).
fn chapter_75(files: A2Files) -> Run {
    let mut e = Engine::boot(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
    e.anim.rand = MsvcRand(1000);
    e.trace = true;
    e.start_new_game().unwrap();
    let mut r = Run {
        e,
        pass: 0,
        clicks: 0.0,
    };
    r.until(30, |e| matches!(e, Event::VideoFrame(2)));
    r.click();
    r.until(10, |e| *e == Event::VideoEnd);
    for _ in 0..15 {
        r.step();
    }
    r.click();
    r.until(3, |e| matches!(e, Event::ClickScan(_)));
    assert_eq!(r.e.script_state().unwrap().chapter, 75);
    r
}

/// The draws of a pass in the golden's words: the centre's, then the
/// bar's, the overlays', the subject menu's and the pointer.
fn draws(e: &Engine) -> Vec<String> {
    let bank = |b: &UiBank| match b {
        UiBank::Interface => "interface",
        UiBank::Objects => "objects",
        UiBank::Subjects => "subjects",
        _ => "other",
    };
    let mut out: Vec<String> = e
        .centre_draws
        .iter()
        .map(|d| match d {
            CentreDraw::Sprite { bank, frame, x, y } => {
                let b = if *bank == CursorBank::Interface {
                    "interface"
                } else {
                    "objects"
                };
                format!("spr {b}#{frame}@{x},{y}")
            }
            CentreDraw::Line { from, to, argb } => {
                format!("line {},{}-{},{} {argb:08x}", from.0, from.1, to.0, to.1)
            }
            CentreDraw::Crosshair { frame, x, y } => format!("spr viseur#{frame}@{x},{y}"),
        })
        .collect();
    for d in &e.ui_draws {
        match d {
            UiDraw::Sprite {
                bank: b,
                frame,
                x,
                y,
            } => out.push(format!("spr {}#{frame}@{x},{y}", bank(b))),
            UiDraw::Translucent {
                bank: b,
                frame,
                x,
                y,
            } => out.push(format!("spr {}#{frame}@{x},{y} mode 1", bank(b))),
            UiDraw::Text { .. } => {}
        }
    }
    out
}

#[test]
fn the_cells_and_the_click_grid_match_the_original() {
    let Some(files) = game() else { return };
    // The bank's sizes, which the centring uses.
    let bank = SprBank::parse(&files.read(Folder::Spr, "sujtibet.spr").unwrap()).unwrap();
    let sizes = rows("layout sprite ");
    assert_eq!(sizes.len(), bank.len());
    for (i, row) in sizes.iter().enumerate() {
        let info = bank.info(i).unwrap();
        assert_eq!(*row, format!("layout sprite {i} {}x{}", info.w, info.h));
    }

    // The draws, by the engine's warp frame with the menu open.
    let mut r = chapter_75(files);
    r.e.trace = false;
    let shaman =
        r.e.script_state()
            .unwrap()
            .names
            .id(defnames::PERSONNAGES, b"SHAMAN")
            .unwrap() as i32;
    let saved = r.e.script_state().unwrap().subjects.clone();
    let layouts = rows("layout ");
    let layouts: Vec<&str> = layouts
        .into_iter()
        .filter(|l| !l.starts_with("layout sprite"))
        .collect();
    assert_eq!(layouts.len(), 17);
    for (count, row) in layouts.iter().enumerate() {
        let count = count as i32;
        {
            let s = r.e.script_state_mut().unwrap();
            s.subjects = table_with(shaman, count, &LAYOUT_VALUES, &LAYOUT_FLAGS);
            let k = s.keys.clicperso;
            s.vars[k] = shaman;
        }
        r.e.subject_menu.open = true;
        r.step();
        let got: Vec<String> = draws(&r.e)
            .into_iter()
            .filter(|d| d.starts_with("spr subjects"))
            .map(|d| d[4..].to_string())
            .collect();
        let want = row.split(" | ").nth(1).unwrap();
        let want: Vec<&str> = if want == "-" {
            Vec::new()
        } else {
            want.split("; ").collect()
        };
        assert_eq!(got, want, "{count} subjects");
        assert_eq!(
            row.split(" | ").next().unwrap(),
            format!(
                "layout {count} origin {},{}",
                last_column(count),
                r.e.subject_menu.first_row
            )
        );
        assert!(r.e.subject_menu.open);
    }
    r.e.subject_menu.open = false;
    r.e.script_state_mut().unwrap().subjects = saved.clone();

    // The click grid, entry i holding 100 + i (past the count too).
    let hits = rows("hit ");
    assert_eq!(hits.len(), 18);
    for (n, row) in hits.iter().take(17).enumerate() {
        let count = n as i32;
        let values: Vec<i32> = (0..16).map(|i| 100 + i).collect();
        let t = table_with(shaman, count, &values, &[1; 16]);
        let y0 = first_row(count);
        let mut map = Vec::new();
        let mut boxes: BTreeMap<i32, [i32; 5]> = BTreeMap::new();
        for y in HIT_Y {
            for x in HIT_X {
                let v = hit(count, y0, (x, y)).map_or(-2, |i| t.entry(shaman, i).value);
                map.extend_from_slice(&v.to_le_bytes());
                let b = boxes.entry(v).or_insert([x, x, y, y, 0]);
                *b = [b[0].min(x), b[1].max(x), b[2].min(y), b[3].max(y), b[4] + 1];
            }
        }
        let cells: Vec<String> = boxes
            .iter()
            .map(|(v, b)| format!("{v}:{}..{},{}..{},n{}", b[0], b[1], b[2], b[3], b[4]))
            .collect();
        let got = format!(
            "hit {count} origin {},{y0} md5 {} | {}",
            last_column(count),
            hex(&map),
            cells.join(" ")
        );
        assert_eq!(*row, got);
    }
    // Without the click flag the menu answers nothing (-1): the engine,
    // the menu of 16 open, the pointer on a cell as in the harness.
    assert_eq!(hits[17], "hit noclick -1");
    let values: Vec<i32> = (0..16).map(|i| 100 + i).collect();
    {
        let s = r.e.script_state_mut().unwrap();
        s.subjects = table_with(shaman, 16, &values, &[1; 16]);
        let k = s.keys.clicperso;
        s.vars[k] = shaman;
    }
    let sujet = r.e.script_state().unwrap().var("SUJET").unwrap();
    r.e.subject_menu.open = true;
    r.step();
    r.e.pointer = (450, 300);
    assert!(hit(16, r.e.subject_menu.first_row, r.e.pointer).is_some());
    r.e.trace = true;
    let events = r.step();
    assert!(!r.e.click_flag());
    assert!(
        !events.iter().any(|e| matches!(e, Event::Subject { .. })),
        "{events:?}"
    );
    assert!(r.e.subject_menu.open);
    assert_eq!(r.e.script_state().unwrap().var("SUJET").unwrap(), sujet);
    r.e.subject_menu.open = false;
    r.e.script_state_mut().unwrap().subjects = saved;
}

/// The host of the chapter rows: the original's harness stubs the dialogs,
/// the films and the chapter load (the buffer keeps chapter 75).
struct Rec {
    chapter: Vec<u8>,
    calls: Vec<String>,
}

impl Host for Rec {
    fn read_chapter(&mut self, chapter: i32) -> Option<Vec<u8>> {
        self.calls.push(format!("LoadCurrentChap({chapter})"));
        Some(self.chapter.clone())
    }
    fn play_video(&mut self, hnm: &str, _dialog: Option<i32>) -> bool {
        self.calls.push(format!("AtlHnm_start('{hnm}')"));
        true
    }
    fn dial_start(&mut self, n: i32) -> DialStart {
        self.calls.push(format!("AtlDial_start({n})"));
        DialStart::Started
    }
}

#[test]
fn the_shaman_subjects_along_chapter_75_match_the_original() {
    if install().is_none() {
        eprintln!("ATLANTIS2_DATA not set: test skipped");
        return;
    }
    let chapter = scenar("chapi075.con");
    let mut s = Script::new(&scenar("defnames.con")).unwrap();
    s.stub_run_init = true;
    let names = s.names.clone();
    let want: Vec<String> = GOLDEN
        .lines()
        .filter(|l| l.starts_with("chapter ") || l.starts_with("ask "))
        .map(|l| fill(&names, l))
        .collect();
    let id = |section, name: &str| names.id(section, name.as_bytes()).unwrap() as i32;
    let (sham, sham10) = (
        id(defnames::PERSONNAGES, "SHAMAN"),
        id(defnames::PERSONNAGES, "SHAMANW10"),
    );
    let mut h = Rec {
        chapter: chapter.clone(),
        calls: Vec::new(),
    };
    let mut got: Vec<String> = Vec::new();
    s.set_chapter_text(&chapter);
    s.lines = Some(Vec::new());

    let lines = |s: &mut Script| -> String {
        let l: Vec<String> = s
            .lines
            .replace(Vec::new())
            .unwrap()
            .iter()
            .map(|(_, n)| n.to_string())
            .collect();
        if l.is_empty() {
            "-".into()
        } else {
            l.join(",")
        }
    };
    let row = |s: &Script, label: &str, lines: &str, resumes: u32, calls: &[String]| {
        let calls: Vec<&String> = calls
            .iter()
            .filter(|c| {
                c.starts_with("AtlDial_start")
                    || c.starts_with("AtlHnm")
                    || c.starts_with("LoadCurrentChap")
            })
            .collect();
        let calls: Vec<&str> = calls.iter().map(|c| c.as_str()).collect();
        format!(
            "chapter {label} | lines {lines} | resumes {resumes} | {} | SHAMAN {} | SHAMANW10 {} | SUJET {} CLICPERSO {} | {}",
            if calls.is_empty() { "-".into() } else { calls.join(" ") },
            list(&s.subjects, sham),
            list(&s.subjects, sham10),
            s.var("SUJET").unwrap(),
            s.var("CLICPERSO").unwrap(),
            if s.warnings.is_empty() { "-".into() } else { s.warnings.join(";") }
        )
    };
    // A /set line out of the chapter, then the chapter back.
    let set_line = |s: &mut Script, h: &mut Rec, line: &str| {
        s.set_chapter_text(format!("{line}\r\n/000\r\n").as_bytes());
        s.set_variables(h, atlantis2_engine::script::Src::Chapter, 0);
        s.set_chapter_text(&chapter);
        s.lines = Some(Vec::new());
    };
    let resume = |s: &mut Script, h: &mut Rec| {
        let mut n = 0;
        while s.resume_pending && n < 20 {
            n += 1;
            s.resume_pending = false;
            let at = s.resume_at;
            s.run_section(h, at);
        }
        n
    };
    let scan =
        |s: &mut Script, h: &mut Rec, got: &mut Vec<String>, label: &str, set: &[(&str, i32)]| {
            h.calls.clear();
            s.warnings.clear();
            for (k, v) in set {
                s.set_var(k, *v);
            }
            s.resume_pending = false;
            s.ctrl_scenar_in(h);
            let n = resume(s, h);
            for (k, _) in set {
                s.set_var(k, 0);
            }
            let l = lines(s);
            got.push(row(s, label, &l, n, &h.calls));
        };
    let ask = |s: &mut Script, h: &mut Rec, got: &mut Vec<String>, perso: &str, subject: &str| {
        h.calls.clear();
        s.warnings.clear();
        let value = names.param(defnames::SUJETS, subject.as_bytes());
        s.set_var("CLICPERSO", id(defnames::PERSONNAGES, perso));
        s.resume_pending = false;
        s.subject_chosen(h, Some(value));
        let scan_lines = lines(s);
        let dials: Vec<String> = h
            .calls
            .iter()
            .filter(|c| c.starts_with("AtlDial"))
            .cloned()
            .collect();
        let state = format!(
            "SUJET {} CLICPERSO {} wait {}",
            s.var("SUJET").unwrap(),
            s.var("CLICPERSO").unwrap(),
            u8::from(s.resume_pending)
        );
        // The end of the dialog (@ 0x004808a0): CLICPERSO = 0, SUJET = −2,
        // the section resumed.
        let n = u32::from(s.resume_pending);
        s.dialog_ended(h);
        let after = lines(s);
        got.push(format!(
            "ask {perso} {subject} value {value} | scan lines {scan_lines} | {} | {state} | after the dialog lines {after} resumes {n}",
            if dials.is_empty() { "-".into() } else { dials.join(" ") }
        ));
        got.push(row(s, &format!("after {subject}"), &after, n, &[]));
    };

    let d00 = scenar("chapi000.con");
    let lines00: Vec<&[u8]> = d00.split(|&b| b == b'\n').collect();
    for no in [22, 23] {
        let l = String::from_utf8_lossy(lines00[no - 1])
            .trim_end()
            .to_string();
        set_line(&mut s, &mut h, &l);
    }
    s.set_var("CHAPTER", 75);
    s.set_var("MONDE", 5);
    got.push(row(&s, "newgame", "-", 0, &[]));
    scan(
        &mut s,
        &mut h,
        &mut got,
        "arrival view 7",
        &[("ARRIVEEVUE", 7)],
    );
    for subject in ["TATOUAGE", "SHAMAN", "TATOUAGE", "HEROS_TIBET", "CRISTAL"] {
        ask(&mut s, &mut h, &mut got, "SHAMAN", subject);
    }
    set_line(&mut s, &mut h, "/set(sacados=MANDALA_BATEAU_YU)");
    scan(&mut s, &mut h, &mut got, "click after the boat", &[]);
    ask(&mut s, &mut h, &mut got, "SHAMAN", "MORCEAU_MANDALA");
    scan(
        &mut s,
        &mut h,
        &mut got,
        "arrival view 10",
        &[("ARRIVEEVUE", 10)],
    );
    for subject in ["HEROS_TIBET", "MANDALA", "MORCEAU_MANDALA", "CRISTAL"] {
        ask(&mut s, &mut h, &mut got, "SHAMANW10", subject);
    }
    set_line(&mut s, &mut h, "/set(objetenmain=MANDALA_BATEAU_YU)");
    let zone = id(defnames::ZONES_CLIC, "ZONE_YUB");
    scan(
        &mut s,
        &mut h,
        &mut got,
        "boat placed",
        &[("CLICZONE", zone)],
    );
    scan(
        &mut s,
        &mut h,
        &mut got,
        "then the click on the zone again",
        &[("CLICZONE", zone)],
    );
    s.set_chapter_text(&chapter);
    s.set_var("CHAPTER", 75);
    ask(&mut s, &mut h, &mut got, "SHAMANW10", "VOYAGE_ASTRAL");
    ask(&mut s, &mut h, &mut got, "SHAMAN", "VOYAGE_ASTRAL");
    for flags in [&[205][..], &[215, 218], &[214]] {
        for &f in flags {
            s.flags[f] = 1;
        }
        ask(&mut s, &mut h, &mut got, "SHAMAN", "CRISTAL");
    }
    s.set_var("NbMandalaPosed", 6);
    ask(&mut s, &mut h, &mut got, "SHAMAN", "CRISTAL");
    let d78 = scenar("chapi078.con");
    let l78: Vec<&[u8]> = d78.split(|&b| b == b'\n').collect();
    set_line(&mut s, &mut h, String::from_utf8_lossy(l78[10]).trim_end());
    got.push(row(&s, "chapi078:11", "-", 0, &[]));
    ask(&mut s, &mut h, &mut got, "SHAMAN", "OMBRE_TIBET");

    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "row {i}");
    }
    assert_eq!(got.len(), want.len());
    assert!(s.fatal.is_none());
}

/// A golden frame row split: the action, the draws, the script lines, the
/// dialogs asked, the scans; and the state after the pass, by key.
#[derive(Debug, Default, PartialEq)]
struct FrameRow {
    action: Vec<String>,
    draws: Vec<String>,
    lines: Vec<String>,
    dials: Vec<String>,
    scans: Vec<String>,
    after: BTreeMap<String, String>,
}

/// The state keys the port compares: all but the turn speeds (cleared
/// every warp frame; no mouse motion here).
const KEYS: [&str; 10] = [
    "menu",
    "CLICPERSO",
    "SUJET",
    "click",
    "aux",
    "hit",
    "cursor",
    "OBJETENMAIN",
    "wait",
    "pointer",
];

fn parse_row(events: &str, after: &str) -> FrameRow {
    let mut r = FrameRow::default();
    for e in events.split("; ").filter(|e| !e.is_empty()) {
        if e == "click" || e == "right" {
            r.action.push(e.into());
        } else if e.starts_with("spr ") || e.starts_with("line ") {
            r.draws.push(e.into());
        } else if e.starts_with("chapi075:") {
            r.lines.push(e.into());
        } else if e.starts_with("AtlDial_start") {
            r.dials.push(e.into());
        } else if e.starts_with("scan(") {
            r.scans.push(e.into());
        } else {
            // SetCursorPos (@ 0x0040c3d0): the system cursor, not the
            // game's pointer.
            assert_eq!(e, "SetCursorPos");
        }
    }
    let t: Vec<&str> = after.split(' ').collect();
    for k in KEYS {
        let i = t.iter().position(|x| *x == k).unwrap();
        r.after.insert(k.into(), t[i + 1].into());
    }
    r
}

#[test]
fn the_menu_in_the_warp_frame_matches_the_original() {
    let Some(files) = game() else { return };
    let mut r = chapter_75(files);
    let names = r.e.script_state().unwrap().names.clone();
    // The golden's cases.
    let texel: Texel = {
        let row = GOLDEN
            .lines()
            .find(|l| l.starts_with("# frame: view 7"))
            .unwrap();
        let t = row
            .split("texel ")
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let v: Vec<i32> = t.split(',').map(|x| x.parse().unwrap()).collect();
        (v[0] as usize, v[1], v[2])
    };
    let mut cases: Vec<(String, Vec<&str>)> = Vec::new();
    let mut in_frame = false;
    for line in GOLDEN.lines() {
        if line.starts_with("== ") {
            in_frame = true;
        }
        if !in_frame || line.starts_with('#') || line.starts_with("frame errors") {
            continue;
        }
        if let Some(h) = line.strip_prefix("== ") {
            cases.push((h.to_string(), Vec::new()));
        } else if line.contains(" dialog end | ") {
            cases.push((line.to_string(), Vec::new()));
        } else {
            cases.last_mut().unwrap().1.push(line);
        }
    }
    assert_eq!(cases.len(), 10, "{cases:?}");
    let start = GOLDEN
        .lines()
        .find(|l| l.starts_with("# frame: subjects at the start "))
        .unwrap();

    r.e.enter_at(7, 0.0, 0.0).unwrap();
    r.e.forced_centre = Some(texel);
    r.e.pointer = (0, 0);
    // The harness's state has had no click yet: no cursor, `SUJET` 0
    // (the port's first click left −2).
    r.e.cursor.shown = None;
    r.e.script_state_mut().unwrap().set_var("SUJET", 0);
    let shaman = names.id(defnames::PERSONNAGES, b"SHAMAN").unwrap() as i32;
    assert_eq!(
        start,
        format!(
            "# frame: subjects at the start {}",
            list(&r.e.script_state().unwrap().subjects, shaman)
        )
    );
    let mut pass_no = 0u64;
    let mut clock = 1000.0;
    for (header, want) in &cases {
        if header.contains(" dialog end | ") {
            let label = header.as_str();
            // The dialog's end (@ 0x004808a0).
            assert_eq!(r.e.loop_type, LoopType::Dialog, "{label}");
            r.e.events.clear();
            r.e.end_dialog();
            let lines: Vec<String> =
                r.e.events
                    .iter()
                    .filter_map(|e| match e {
                        Event::Set { line, .. } => Some(format!("chapi075:{line}")),
                        _ => None,
                    })
                    .collect();
            let parts: Vec<&str> = label.split(" | ").collect();
            assert_eq!(lines.join("; "), parts[1], "{label}");
            let want = parse_row("", parts[2]);
            assert_eq!(after(&r.e), want.after, "{label}");
            let s = r.e.script_state().unwrap();
            assert_eq!(parts[3], format!("subjects {}", list(&s.subjects, shaman)));
            continue;
        }
        if header.starts_with("F ") {
            r.e.run_command(b"(objetenmain=0)");
        }
        for (k, row) in want.iter().enumerate() {
            let parts: Vec<&str> = row.split(" | ").collect();
            assert_eq!(parts[0], format!("p{k}"));
            let want = parse_row(parts[1], parts[2]);
            if pass_no.is_multiple_of(4) {
                r.e.tick();
            }
            pass_no += 1;
            let pointer = &want.after["pointer"];
            let (x, y) = pointer.split_once(',').unwrap();
            r.e.pointer = (x.parse().unwrap(), y.parse().unwrap());
            for a in &want.action {
                if a == "click" {
                    clock += 10.0;
                    r.e.click(clock);
                } else {
                    r.e.right_click();
                }
            }
            r.e.events.clear();
            r.e.pass(None);
            let mut got = FrameRow {
                action: want.action.clone(),
                draws: draws(&r.e),
                ..Default::default()
            };
            for e in &r.e.events {
                match e {
                    Event::Set { line, .. } => got.lines.push(format!("chapi075:{line}")),
                    Event::DialogStart { number, .. } => {
                        got.dials.push(format!("AtlDial_start({number})"))
                    }
                    Event::ClickScan(v) => got.scans.push(format!(
                        "scan(DEPARTVUE={},ARRIVEEVUE={},CLICZONE={},CLICPERSO={},CLICOBJET={})",
                        v.departvue, v.arriveevue, v.cliczone, v.clicperso, v.clicobjet
                    )),
                    _ => {}
                }
            }
            got.after = after(&r.e);
            assert_eq!(got, want, "{header}, p{k}");
        }
    }
}

/// The state after a pass, in the golden's words.
fn after(e: &Engine) -> BTreeMap<String, String> {
    let s = e.script_state().unwrap();
    let cursor = match e.cursor.shown {
        Some(f) => format!(
            "{}#{f}",
            if e.cursor.bank == CursorBank::Interface {
                "interface"
            } else {
                "objects"
            }
        ),
        None => "none".into(),
    };
    let hit = e.sprite_under_centre().0.map_or(-1, |n| n as i32);
    [
        ("menu", u8::from(e.subject_menu.open).to_string()),
        ("CLICPERSO", s.var("CLICPERSO").unwrap().to_string()),
        ("SUJET", s.var("SUJET").unwrap().to_string()),
        ("click", u8::from(e.click_flag()).to_string()),
        ("aux", e.click_aux.to_string()),
        ("hit", hit.to_string()),
        ("cursor", cursor),
        ("OBJETENMAIN", s.var("OBJETENMAIN").unwrap().to_string()),
        ("wait", u8::from(s.resume_pending).to_string()),
        ("pointer", format!("{},{}", e.pointer.0, e.pointer.1)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

impl Run {
    /// Looks at zone `zone` of the current view and clicks; then runs to
    /// the arrival in `to` (a transition video clicked away).
    fn walk(&mut self, zone: usize, to: u32) {
        for _ in 0..15 {
            self.step();
        }
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        self.click();
        for _ in 0..600 {
            let evs = self.step();
            if evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            }) {
                for _ in 0..15 {
                    self.step();
                }
                self.click();
            }
            if evs.iter().any(|e| matches!(e, Event::ArrivalScan(_))) {
                assert_eq!(self.e.place().1, to);
                return;
            }
        }
        panic!("no arrival in view {to}");
    }

    /// Turns the view as a player does until a sprite is under the centre
    /// (the cursor's sprite state) and clicks it; the events of the click
    /// when `done` holds on them.
    fn click_sprite(&mut self, done: impl Fn(&Event) -> bool) -> Vec<Event> {
        for pitch in (-8..=8).map(|p| p * 5) {
            for yaw in (0..72).map(|y| y * 5 - 180) {
                let (y, p) = ((yaw as f32).to_radians(), (pitch as f32).to_radians());
                self.e.camera.enter(y, p);
                self.step();
                if self.e.sprite_under_centre().1 == 0 {
                    continue;
                }
                self.click();
                let evs = self.step();
                if evs.iter().any(&done) {
                    return evs;
                }
                // Another character's menu: closed by a click outside.
                if self.e.subject_menu.open {
                    let (px, py) = self.e.pointer;
                    self.e.mouse_motion(-px, -py);
                    self.click();
                    self.step();
                }
            }
        }
        panic!("no sprite answers in view {}", self.e.place().1);
    }

    /// Plays the running dialog to its end, a click every 20 passes
    /// (a click ends a line).
    fn listen(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        for n in 0..20_000 {
            if n % 20 == 19 {
                self.click();
            }
            let evs = self.step();
            let end = evs.contains(&Event::DialogEnd);
            all.extend(evs);
            if end {
                for _ in 0..4 {
                    all.extend(self.step());
                }
                return all;
            }
        }
        panic!("the dialog does not end");
    }

    /// With the menu of `character` open, the pointer on the cell of
    /// `subject` and a click: the dialog the answer starts.
    fn ask(&mut self, character: i32, subject: &str) -> u32 {
        assert!(self.e.subject_menu.open);
        let s = self.e.script_state().unwrap();
        let value = s.names.param(defnames::SUJETS, subject.as_bytes());
        let list = s.subjects.list(character);
        let i = list.iter().position(|e| e.value == value).unwrap();
        let (x, y) = cell(list.len() as i32, i as i32);
        // The pointer moved there by the mouse.
        let (px, py) = self.e.pointer;
        self.e.mouse_motion(x + 31 - px, y + 31 - py);
        self.step();
        self.click();
        let evs = self.step();
        assert!(evs.contains(&Event::Subject {
            character,
            value: Some(value)
        }));
        assert!(!self.e.subject_menu.open);
        let number = evs
            .iter()
            .find_map(|e| match e {
                Event::DialogStart {
                    number,
                    started: true,
                } => Some(*number),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{subject}: no dialog in {evs:?}"));
        let evs = self.listen();
        assert_eq!(self.e.loop_type, LoopType::Warp);
        let s = self.e.script_state().unwrap();
        assert_eq!((s.var("CLICPERSO"), s.var("SUJET")), (Some(0), Some(-2)));
        assert!(evs.iter().all(|e| !matches!(e, Event::DialogStart { .. })));
        number
    }

    /// A click on the character under the centre opens its menu.
    fn talk_to(&mut self, character: i32) {
        let evs = self.click_sprite(|e| matches!(e, Event::SubjectMenu { .. }));
        let count = self.e.script_state().unwrap().subjects.count(character);
        assert!(evs.contains(&Event::SubjectMenu { character, count }));
    }

    /// The move zone of the current view that leads to view `to`.
    fn zone_to(&self, to: u32) -> usize {
        let view = self.e.place().1;
        let zones = &self.e.wam().view(view).unwrap().zones;
        zones
            .iter()
            .position(|z| z.kind == 1 && z.value == to as i32)
            .unwrap_or_else(|| panic!("no move from view {view} to {to}"))
    }
}

/// The dialogs the golden's questions start, in its order.
fn golden_dialogs() -> Vec<u32> {
    rows("ask ")
        .iter()
        .map(|r| {
            let d = r.split("AtlDial_start(").nth(1).unwrap();
            d[..d.find(')').unwrap()].parse().unwrap()
        })
        .collect()
}

#[test]
fn a_tibet_game_asks_the_shamans_until_a_boat_is_placed() {
    let Some(files) = game() else { return };
    let mut r = chapter_75(files);
    let names = r.e.script_state().unwrap().names.clone();
    let id = |name: &[u8]| names.id(defnames::PERSONNAGES, name).unwrap() as i32;
    let (shaman, shaman10) = (id(b"SHAMAN"), id(b"SHAMANW10"));
    let want = golden_dialogs();
    // To view 7; the arrival plays dialog 360.
    for (zone, to) in [(0, 3), (1, 4), (0, 5), (1, 6), (0, 7)] {
        r.walk(zone, to);
    }
    assert_eq!(r.e.loop_type, LoopType::Dialog);
    r.listen();

    // The shaman of view 7, as the golden asks him (its first five
    // questions): the tattoo, himself (his gift: the balls, the ball in
    // the hand), the tattoo again, the hero, the crystal.
    let mut got = Vec::new();
    for subject in ["TATOUAGE", "SHAMAN"] {
        r.talk_to(shaman);
        got.push(r.ask(shaman, subject));
    }
    // The ball in the hand: no menu; the bag opened and closed empties it.
    assert_ne!(r.e.script_state().unwrap().var("OBJETENMAIN"), Some(0));
    r.e.right_click();
    r.step();
    r.e.right_click();
    r.step();
    for subject in ["TATOUAGE", "HEROS_TIBET", "CRISTAL"] {
        r.talk_to(shaman);
        got.push(r.ask(shaman, subject));
    }
    assert_eq!(got, want[..5]);
    // Asked subjects are drawn at 50 %, still offered.
    r.talk_to(shaman);
    r.step();
    let half =
        r.e.ui_draws
            .iter()
            .filter(|d| matches!(d, UiDraw::Translucent { .. }))
            .count();
    let s = r.e.script_state().unwrap();
    let asked = s
        .subjects
        .list(shaman)
        .iter()
        .filter(|e| e.flag != 1)
        .count();
    assert!(half >= 1 && half == asked, "{half} {asked}");
    // A click outside closes it.
    r.e.mouse_motion(100 - r.e.pointer.0, 100 - r.e.pointer.1);
    r.step();
    r.click();
    let evs = r.step();
    assert!(evs.contains(&Event::Subject {
        character: shaman,
        value: None
    }));
    assert!(!r.e.subject_menu.open);

    // View 8: the boat taken (a new subject for both shamans), then on to
    // view 10, where the arrival raises flag 202.
    r.walk(1, 8);
    r.click_sprite(|e| matches!(e, Event::Take { .. }));
    let s = r.e.script_state().unwrap();
    assert_eq!(s.flags[200], 1);
    let morceau = names.param(defnames::SUJETS, b"MORCEAU_MANDALA");
    assert!(s.subjects.list(shaman10).iter().any(|e| e.value == morceau));
    // Emptied hand, as before.
    r.e.right_click();
    r.step();
    r.e.right_click();
    r.step();
    // Back to the shaman of view 7: the piece of mandala.
    let back = r.zone_to(7);
    r.walk(back, 7);
    if r.e.loop_type == LoopType::Dialog {
        r.listen();
    }
    r.talk_to(shaman);
    assert_eq!(r.ask(shaman, "MORCEAU_MANDALA"), want[5]);
    r.walk(1, 8);
    r.walk(1, 9);
    r.walk(1, 10);
    assert_eq!(r.e.script_state().unwrap().flags[202], 1);
    // The shaman of view 10: the hero (now the mandala is offered), the
    // mandala, the piece of mandala, the crystal; then the astral
    // journey, which the scripts offer when the first boat placed sends
    // the game to a world (here offered by their key, not played).
    let mut got = Vec::new();
    for subject in ["HEROS_TIBET", "MANDALA", "MORCEAU_MANDALA", "CRISTAL"] {
        r.talk_to(shaman10);
        got.push(r.ask(shaman10, subject));
    }
    assert_eq!(got, want[6..10]);
    r.e.run_command(b"(enablesuj=SHAMANW10,VOYAGE_ASTRAL)");
    r.talk_to(shaman10);
    let n = r.ask(shaman10, "VOYAGE_ASTRAL");
    assert_eq!(n, want[10]);
    assert!(r.e.script_state().unwrap().fatal.is_none());
}

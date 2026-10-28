//! Cryo's DVD-ROM of 1998 (its eight languages) and the Perfect Edition:
//! the subtitle rows of every dialog line against each build's breaker run
//! under emulation, and for each language a new game from the start-up to
//! its first dialog (its subtitle rows on the screen), the LOAD list, a save
//! and its load; the bug fixes each build lists, and the Dutch and Swedish
//! DOS letters with and without their fix; the Japanese build's own: its
//! Japanese and English rows, the three-state SUBTITLES button and its CON
//! reload, the missing logo films with and without their fix.
//!
//! Needs copies of the sides: `ATLANTIS_DVD_A_FILES` / `ATLANTIS_DVD_B_FILES`
//! (the folders holding `FACE_A` / `FACE_B`); `ATLANTIS_PERFECT_EDITION`
//! for the Perfect Edition. Skipped, each saying so, without them. With
//! `ATLANTIS_DVD_SHOTS=<folder>` the first dialog of each language is
//! saved there (`dvd-<folder>-first-dialog.png`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::dvd::{self, DvdFiles};
use atlantis_data::exe::DVD_JA;
use atlantis_data::saves::Players;
use atlantis_data::sprite::{limits, read_spr_file, SprSprite};
use atlantis_data::sprite_blend::SPR_NO_BLEND;
use atlantis_data::{Game, StdDir, TextCodePage};
use atlantis_engine::audio::Mixer;
use atlantis_engine::checkpoint::PLAYERS_FILE;
use atlantis_engine::con_script::script::ConItem;
use atlantis_engine::con_script::{con_track_letter, ConScript};
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::scripts::Scripts;
use atlantis_engine::subtitles::{split_subtitle, split_subtitle_cjk, CjkSplit};
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::ui::layout::{menu_number, walk_menu_text, MenuRules, MenuWalk};
use atlantis_engine::{
    BugFix, BugFixes, Engine, Enhancements, Event, MemStore, Options, SaveStore, Start,
};
use md5::{Digest, Md5};

#[allow(dead_code)]
#[path = "../../atlantis2-engine/src/png.rs"]
mod png;

/// The golden of the breakers: each build's FUN_0042f108 (its own address
/// in the Dutch, Italian and Swedish builds) run unmodified under
/// emulation on its language's dialog lines (md5s of the rows only).
const GOLDEN: &str = include_str!("golden/video_subtitles_dvd1998.txt");

/// The golden of the Japanese build's breaker, FUN_0042f618 run unmodified
/// under emulation on its CHAPJ lines (language counter 0) and its CHAPI
/// lines (counter 1): md5s of the rows, their counts, samples.
const GOLDEN_JA: &str = include_str!("golden/video_subtitles_dvd_ja.txt");

/// The Japanese build.
const JA: &str = "atlantis-dvd-ja-696832";

/// (language folder, build)
const LANGUAGES: [(&str, &str); 7] = [
    ("ALL", "atlantis-dvd-de-695296"),
    ("ENG", "atlantis-dvd-en-695296"),
    ("FRA", "atlantis-dvd-fr-695296"),
    ("HOL", "atlantis-dvd-nl-695808"),
    ("ESP", "atlantis-dvd-es-695296"),
    ("ITA", "atlantis-dvd-it-695296"),
    ("SUE", "atlantis-dvd-sv-695808"),
];

/// The language `folder` of the DVD, from the copy of its side.
fn dvd(folder: &str) -> Option<Game> {
    let l = dvd::LANGUAGES.iter().find(|l| l.folder == folder).unwrap();
    let var = if l.side == 'A' {
        "ATLANTIS_DVD_A_FILES"
    } else {
        "ATLANTIS_DVD_B_FILES"
    };
    let Some(dir) = std::env::var_os(var) else {
        eprintln!("{var} not set: {folder} skipped");
        return None;
    };
    let files = DvdFiles::new(Arc::new(StdDir::new(dir)), l).expect("the DVD's side");
    Some(Game::open(Arc::new(files)).expect("the language"))
}

fn perfect_edition() -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_PERFECT_EDITION") else {
        eprintln!("ATLANTIS_PERFECT_EDITION not set: skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("the Perfect Edition"))
}

/// The golden's lines of `build`: "key value" pairs.
fn golden(build: &str) -> BTreeMap<String, String> {
    golden_in(GOLDEN, build)
}

/// The lines of `text`'s section whose head starts with `build`: "key
/// value" pairs.
fn golden_in(text: &str, build: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut on = false;
    for line in text.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            on = head.starts_with(build);
            continue;
        }
        if on {
            if let Some((k, v)) = line.rsplit_once(' ') {
                out.insert(k.to_string(), v.to_string());
            }
        }
    }
    assert!(!out.is_empty(), "{build}: not in the golden");
    out
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The dialog lines' texts of a CON file, as the breaker gets them: the
/// text after "] " up to the CR.
fn dialog_texts(con: &[u8]) -> Vec<&[u8]> {
    con.split(|&c| c == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .filter(|l| l.first() == Some(&b'['))
        .filter_map(|l| {
            let close = l.iter().position(|&c| c == b']')?;
            let rest = &l[close + 1..];
            (!rest.iter().all(u8::is_ascii_whitespace))
                .then(|| rest.strip_prefix(b" ").unwrap_or(rest))
        })
        .collect()
}

/// Every dialog line of the release's CON files through the port's
/// breaker: each file's rows (each line's rows joined by NUL, the lines by
/// LF) against the original's, emulated.
fn rows_match(g: &Game, build: &str) {
    rows_match_with(g, build, None, &golden(build), split_subtitle);
}

/// [`rows_match`] on the CON files whose name starts with `prefix` (every
/// one for `None`), cut by `split`, against `want`.
fn rows_match_with(
    g: &Game,
    build: &str,
    prefix: Option<&str>,
    want: &BTreeMap<String, String>,
    split: impl Fn(&[u8]) -> Vec<Vec<u8>>,
) {
    let src = g.big.source();
    let mut names: Vec<String> = src
        .list("SCENAR")
        .into_iter()
        .map(|n| n.to_ascii_uppercase())
        .filter(|n| n.ends_with(".CON") && prefix.is_none_or(|p| n.starts_with(p)))
        .collect();
    names.sort();
    let mut total = Md5::new();
    let mut lines = 0;
    for n in &names {
        let con = src.read(&format!("SCENAR\\{n}")).unwrap();
        let mut h = Md5::new();
        for t in dialog_texts(&con) {
            let rows = split(t);
            lines += 1;
            h.update(rows.join(&0u8));
            h.update(b"\n");
        }
        let d: [u8; 16] = h.finalize().into();
        assert_eq!(
            Some(&hex(&d)),
            want.get(&format!("SCENAR\\{n}")),
            "{build}: {n}"
        );
        total.update(d);
    }
    assert!(lines > 2500, "{build}: {lines} lines");
    assert_eq!(
        Some(&hex(&total.finalize())),
        want.get("rows md5"),
        "{build}"
    );
}

#[test]
fn subtitle_rows_match_the_original() {
    for (folder, build) in LANGUAGES {
        if let Some(g) = dvd(folder) {
            assert_eq!(g.exe.build.name, build);
            rows_match(&g, build);
        }
    }
    if let Some(g) = perfect_edition() {
        rows_match(&g, "atlantis-pe-2018-695296");
    }
}

/// The fixes each build lists: the hexagon's for every 1998 build, the
/// Italian menu width, the Dutch and Swedish DOS letters; the German
/// DVD's `ß` (its EPI.TXT, as the German CD's).
#[test]
fn the_fixes_of_each_dvd_language() {
    use BugFix::*;
    let every: Vec<BugFix> = BugFix::ALL
        .into_iter()
        .filter(|f| f.of_every_build())
        .collect();
    for (folder, _) in LANGUAGES {
        let Some(g) = dvd(folder) else { continue };
        let own: &[BugFix] = match folder {
            "ALL" => &[HexagonPieces, SharpS],
            "ITA" => &[HexagonPieces, MenuLineWidth],
            "HOL" | "SUE" => &[HexagonPieces, DosLetters],
            _ => &[HexagonPieces],
        };
        let mut want = every.clone();
        want.extend_from_slice(own);
        assert_eq!(BugFix::of_game(&g), want, "{folder}");
    }
    if let Some(g) = perfect_edition() {
        let mut want = every.clone();
        want.push(HexagonPieces);
        assert_eq!(BugFix::of_game(&g), want);
    }
}

struct Headless {
    sys: Rc<Sys>,
    runner: Runner,
    now: u64,
    sound: Vec<f32>,
    alive: bool,
}

impl Headless {
    fn new(g: Game, opts: Options) -> Headless {
        let sys = Sys::new(Mixer::new());
        Headless {
            runner: Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts)),
            sys,
            now: 0,
            sound: vec![0.0; 2 * (22050 * 16 / 1000)],
            alive: true,
        }
    }

    fn step(&mut self) -> bool {
        self.alive = self.runner.step(self.now);
        self.sys.mixer.fill(&mut self.sound, 22050, 2);
        self.now += 16;
        self.alive
    }

    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            if !self.step() {
                break;
            }
        }
    }

    fn click(&mut self, x: i32, y: i32) {
        let button = MouseButton::Left;
        self.sys.push_input(Input::MouseMove { x, y });
        self.sys.push_input(Input::MouseDown { button, x, y });
        self.step();
        self.sys.push_input(Input::MouseUp { button, x, y });
        self.steps(3);
    }

    fn key(&mut self, k: Key) {
        self.sys.push_input(Input::KeyDown(k));
        self.sys.push_input(Input::KeyUp(k));
        self.steps(2);
    }

    fn until(&mut self, n: usize, mut f: impl FnMut(&Headless) -> bool) -> bool {
        for _ in 0..n {
            if f(self) || !self.step() {
                return f(self);
            }
        }
        false
    }
}

/// From the start-up to the player screen: clicks through the films, Enter
/// on each error box (the Japanese build's missing logos). Escape first,
/// which leaves the box open as in the original (its procedure ends the
/// dialog on IDOK only): neither the next box nor a film follows it.
fn to_player_screen(h: &mut Headless, trace: &Rc<RefCell<Vec<Event>>>) -> bool {
    let mut closed = 0;
    for _ in 0..100 {
        h.steps(30);
        if menus(trace, "SELEMENU.TXT") > 0 {
            return true;
        }
        if error_boxes(trace).len() > closed {
            closed += 1;
            let seen = trace.borrow().len();
            h.key(Key::Escape);
            h.steps(60);
            assert!(
                !trace.borrow()[seen..]
                    .iter()
                    .any(|e| matches!(e, Event::ErrorBox(_) | Event::Video { .. })),
                "Escape closed error box {closed}"
            );
            h.key(Key::Enter);
            continue;
        }
        h.click(320, 240);
    }
    false
}

/// The texts of the error boxes shown.
fn error_boxes(t: &Rc<RefCell<Vec<Event>>>) -> Vec<String> {
    t.borrow()
        .iter()
        .filter_map(|e| match e {
            Event::ErrorBox(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn menus(t: &Rc<RefCell<Vec<Event>>>, m: &str) -> usize {
    t.borrow()
        .iter()
        .filter(|e| matches!(e, Event::Menu(n) if n == m))
        .count()
}

/// The text of the dialog line `letter` of section `section` in the CON
/// of `chapter`.
fn line_text(g: &Game, chapter: u32, section: i32, letter: char) -> Option<Vec<u8>> {
    let con = g.big.read(&format!("SCENAR\\CHAPI{chapter:03}.CON"))?;
    let script = ConScript::parse(&con);
    script
        .sections()
        .iter()
        .filter(|s| s.id == section)
        .find_map(|s| {
            s.items
                .iter()
                .filter_map(|i| match i {
                    ConItem::Line(l) => Some(l),
                    ConItem::Set { .. } => None,
                })
                .enumerate()
                .find(|(k, _)| con_track_letter(*k) == letter)
                .map(|(_, l)| l.text.chars().map(|c| c as u32 as u8).collect())
        })
}

/// The number of 14-pixel row bands above y 0x1ce, from x 5, holding the
/// subtitles' white (0xffff) over at least `min` pixels, counted up from
/// the bottom row until a band without.
fn white_rows(screen: &[u16], min: usize) -> usize {
    let mut n = 0;
    loop {
        let y0 = 0x1ce - (n as i32 + 2) * 14;
        if y0 < 0 {
            return n;
        }
        let white = (y0..y0 + 14)
            .flat_map(|y| (5..635).map(move |x| (x, y)))
            .filter(|&(x, y)| screen[y as usize * 640 + x as usize] == 0xffff)
            .count();
        if white < min {
            return n;
        }
        n += 1;
    }
}

/// From the start-up: the logos (the Dutch build's R&P logo among them),
/// the player screen, a new player, the first chapter, the clicks through
/// the opening to the first dialog line with a subtitle: its rows on the
/// screen are the original's for that line; then the LOAD list with the
/// checkpoint the chapter wrote, and its load.
fn first_dialog(folder: &str, build: &str) {
    first_dialog_with(folder, build, Enhancements::original(), "");
}

/// [`first_dialog`] with `enhancements`, its screenshot named with `suffix`.
fn first_dialog_with(folder: &str, build: &str, enhancements: Enhancements, suffix: &str) {
    let Some(g) = dvd(folder) else { return };
    let Some(probe) = dvd(folder) else { return };
    assert_eq!(g.exe.build.name, build);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let store = Rc::new(MemStore::new());
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        subtitles: true,
        enhancements,
        ..Options::default()
    };
    let mut h = Headless::new(g, opts);
    assert!(
        to_player_screen(&mut h, &trace),
        "{build}: no player screen"
    );
    let videos: Vec<String> = trace
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::Video { name, .. } => Some(name.to_ascii_uppercase()),
            _ => None,
        })
        .collect();
    let rp = videos.iter().any(|v| v.ends_with("LOGORP.HNM"));
    assert_eq!(rp, folder == "HOL", "{build}: {videos:?}");
    h.steps(10);
    h.click(200, 259);
    for c in "Tester".chars() {
        h.key(Key::Char(c));
    }
    h.steps(2);
    h.key(Key::Enter);
    assert!(
        h.until(2000, |_| trace.borrow().contains(&Event::ChapterLoaded(1))),
        "{build}: chapter 1 not loaded"
    );
    // Clicks until a dialog line with a subtitle starts.
    let mut found = None;
    for _ in 0..400 {
        let seen = trace.borrow().len();
        h.steps(20);
        let hit = trace.borrow()[seen..].iter().find_map(|e| match e {
            Event::Line {
                section, letter, ..
            } => Some((*section, *letter)),
            _ => None,
        });
        if let Some((section, letter)) = hit {
            let chapter = trace
                .borrow()
                .iter()
                .rev()
                .find_map(|e| match e {
                    Event::ChapterLoaded(c) => Some(*c),
                    _ => None,
                })
                .unwrap();
            if let Some(text) = line_text(&probe, chapter, section, letter) {
                let trimmed = text.trim_ascii_end().to_vec();
                if !trimmed.is_empty() {
                    found = Some((chapter, section, letter, trimmed));
                    break;
                }
            }
        }
        if !h.sys.mouse_locked() {
            h.click(320, 240);
        }
    }
    let (chapter, section, letter, text) = found.expect("no dialog line");
    let rows = split_subtitle(&text);
    // The line's rows as the original's breaker cuts them (the golden's
    // file md5 covers every line; this one's count is its own).
    let want = golden(build);
    // The rows on the screen while the line plays.
    let mut drawn = 0;
    let mut shot = Vec::new();
    for _ in 0..200 {
        h.step();
        let screen = h.sys.screen.borrow().clone();
        let n = white_rows(&screen, 15);
        if n >= rows.len() {
            drawn = n;
            shot = screen;
            break;
        }
    }
    assert_eq!(drawn, rows.len(), "{build}: rows on the screen");
    if let Some(dir) = std::env::var_os("ATLANTIS_DVD_SHOTS") {
        let png = png::encode_rgb565(&shot, 640, 480);
        std::fs::write(
            PathBuf::from(dir).join(format!("dvd-{folder}-first-dialog{suffix}.png")),
            png,
        )
        .unwrap();
    }
    // The original's rows of this line: CHAPI001's first dialog line.
    assert_eq!(chapter, 1, "{build}");
    let con = probe.big.read("SCENAR\\CHAPI001.CON").unwrap();
    let raw = dialog_texts(&con)[0];
    assert_eq!(
        raw.trim_ascii_end(),
        &text[..],
        "{build}: not CHAPI001's first line"
    );
    let lens: Vec<String> = split_subtitle(raw)
        .iter()
        .map(|r| r.len().to_string())
        .collect();
    assert_eq!(
        Some(&lens.join(",")),
        want.get("first line SCENAR\\CHAPI001.CON"),
        "{build}: the first dialog's rows"
    );
    // Played: its subtitle rows counted.
    assert!(
        h.until(3000, |_| trace.borrow().iter().any(|e| matches!(
            e,
            Event::LinePlayed { section: s, letter: l, subtitles, .. }
                if *s == section && *l == letter && *subtitles == rows.len()
        ))),
        "{build}: the line's subtitles"
    );
    h.sys.push_input(Input::Quit);
    assert!(
        h.until(600, |hh| !hh.alive),
        "{build}: the game did not end"
    );
}

/// A checkpoint of chapter 2 for the player "Tester" in `store`, as the
/// chapter change writes it (`Chapter=2`: FUN_0042d6b8's checkpoint, then
/// players.atl).
fn checkpoint_of_chapter_2(g: Game, store: Rc<MemStore>) {
    let sys = Sys::new(Mixer::new());
    let s2 = sys.clone();
    let mut r = Runner::new(sys.clone(), async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.saves = store;
        let marker = e.game.exe.new_player_marker.clone();
        e.players = Players::new(&marker);
        e.player = e.players.create(b"Tester").expect("a row");
        let mut scripts = Scripts::default();
        e.load_checkpoint(&mut scripts, false).await;
        scripts.exec_set(&mut e, None, "chapter=2").await;
    });
    let mut t = 0;
    while r.step(t) {
        t += 16;
        assert!(t < 16 * 200_000, "the chapter change did not end");
    }
}

/// A save and its load: the checkpoint of a chapter change, the player
/// chosen on the player screen (its checkpoint loaded), then the in-game
/// menu's LOAD list with that checkpoint and its load.
fn save_and_load(folder: &str, build: &str) {
    let Some(g) = dvd(folder) else { return };
    let Some(g2) = dvd(folder) else { return };
    let Some(probe) = dvd(folder) else { return };
    assert_eq!(g.exe.build.name, build);
    let store = Rc::new(MemStore::new());
    checkpoint_of_chapter_2(g, store.clone());
    assert!(
        store.read("pa2.gam").is_some(),
        "{build}: {:?}",
        store.list()
    );
    assert!(store.read(PLAYERS_FILE).is_some(), "{build}");
    let trace = Rc::new(RefCell::new(Vec::new()));
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        enhancements: Enhancements::original(),
        ..Options::default()
    };
    let mut h = Headless::new(g2, opts);
    assert!(
        to_player_screen(&mut h, &trace),
        "{build}: no player screen"
    );
    h.steps(10);
    let loaded = |t: &Rc<RefCell<Vec<Event>>>| {
        t.borrow()
            .iter()
            .filter(|e| matches!(e, Event::CheckpointLoaded(n) if n == "pa2.gam"))
            .count()
    };
    // The player's row, a double click: its checkpoint.
    h.click(200, 259);
    h.click(200, 259);
    assert!(
        h.until(2000, |_| loaded(&trace) > 0),
        "{build}: pa2.gam not loaded from the player screen"
    );
    // Escape in the place: the in-game menu; LOAD: the list with the
    // checkpoint; a double click on its row loads it again.
    let mut menu = false;
    for _ in 0..400 {
        h.steps(30);
        if h.sys.mouse_locked() {
            h.key(Key::Escape);
            if h.until(10, |_| menus(&trace, "MAINMENU.TXT") > 0) {
                menu = true;
                break;
            }
        } else {
            h.click(320, 240);
        }
    }
    assert!(menu, "{build}: the in-game menu did not open");
    let load = record_centre(&probe, "MAINMENU.TXT", 0);
    h.click(load.0, load.1);
    assert!(
        h.until(30, |_| menus(&trace, "LOADMENU.TXT") > 0),
        "{build}: no LOAD list"
    );
    h.sys.push_input(Input::MouseMove { x: 639, y: 0 });
    h.steps(10);
    if let Some(dir) = std::env::var_os("ATLANTIS_DVD_SHOTS") {
        let png = png::encode_rgb565(&h.sys.screen.borrow(), 640, 480);
        std::fs::write(
            PathBuf::from(dir).join(format!("dvd-{folder}-load-list.png")),
            png,
        )
        .unwrap();
    }
    let row = (200, 0x6e + 11);
    h.click(row.0, row.1);
    h.click(row.0, row.1);
    assert!(
        h.until(2000, |_| loaded(&trace) > 1),
        "{build}: pa2.gam not loaded from the LOAD list"
    );
    h.sys.push_input(Input::Quit);
    assert!(
        h.until(600, |hh| !hh.alive),
        "{build}: the game did not end"
    );
}

/// The centre of record `n` of a menu as the build walks it.
fn record_centre(g: &Game, file: &str, n: usize) -> (i32, i32) {
    let text = g.big.read(&format!("SPRLIST\\{file}")).unwrap();
    let mut walk = MenuWalk::default();
    let rules = MenuRules::of_build(g.exe.build, &BugFixes::NONE);
    let number = menu_number(file).unwrap() as i32;
    walk_menu_text(&text, number, -1, 0, 0, rules, &mut walk);
    let r = walk.records[n];
    (
        i32::from(r.x) + i32::from(r.w) / 2,
        i32::from(r.y) + i32::from(r.h) / 2,
    )
}

#[test]
fn first_dialog_german() {
    first_dialog("ALL", "atlantis-dvd-de-695296");
}

#[test]
fn save_and_load_german() {
    save_and_load("ALL", "atlantis-dvd-de-695296");
}

#[test]
fn first_dialog_english() {
    first_dialog("ENG", "atlantis-dvd-en-695296");
}

#[test]
fn save_and_load_english() {
    save_and_load("ENG", "atlantis-dvd-en-695296");
}

#[test]
fn first_dialog_french() {
    first_dialog("FRA", "atlantis-dvd-fr-695296");
}

#[test]
fn save_and_load_french() {
    save_and_load("FRA", "atlantis-dvd-fr-695296");
}

#[test]
fn first_dialog_dutch() {
    first_dialog("HOL", "atlantis-dvd-nl-695808");
}

#[test]
fn save_and_load_dutch() {
    save_and_load("HOL", "atlantis-dvd-nl-695808");
}

#[test]
fn first_dialog_spanish() {
    first_dialog("ESP", "atlantis-dvd-es-695296");
}

#[test]
fn save_and_load_spanish() {
    save_and_load("ESP", "atlantis-dvd-es-695296");
}

#[test]
fn first_dialog_italian() {
    first_dialog("ITA", "atlantis-dvd-it-695296");
}

#[test]
fn save_and_load_italian() {
    save_and_load("ITA", "atlantis-dvd-it-695296");
}

#[test]
fn first_dialog_swedish() {
    first_dialog("SUE", "atlantis-dvd-sv-695808");
}

/// The Swedish first dialog with the DOS letters' fix: the same rows (the
/// code page changes no byte count).
#[test]
fn first_dialog_swedish_with_dos_letters() {
    first_dialog_with(
        "SUE",
        "atlantis-dvd-sv-695808",
        Enhancements::only_fix(BugFix::DosLetters),
        "-dos-letters",
    );
}

#[test]
fn save_and_load_swedish() {
    save_and_load("SUE", "atlantis-dvd-sv-695808");
}

/// The Dutch and Swedish DOS letters: as the original reads them (the
/// Windows code page) without the fix, in the DOS code page with it.
#[test]
fn dos_letters() {
    for folder in ["HOL", "SUE"] {
        let Some(g) = dvd(folder) else { continue };
        assert!(g.exe.build.dos_subtitle_texts);
        assert_eq!(g.exe.build.text_code_page, TextCodePage::Windows1252);
        let mut e = Engine::new(Sys::new(Mixer::new()), g, Rc::new(DirectLoader));
        e.set_enhancements(Enhancements::original());
        assert_eq!(e.subtitle_code_page(), TextCodePage::Windows1252);
        e.set_enhancements(Enhancements::only_fix(BugFix::DosLetters));
        assert_eq!(e.subtitle_code_page(), TextCodePage::Dos850);
        assert_eq!(TextCodePage::Dos850.decode(&[0x84, 0x86, 0x94]), "äåö");
        assert_eq!(TextCodePage::Windows1252.decode(&[0x84, 0x86, 0x94]), "„†”");
    }
    for folder in ["ENG", "ALL"] {
        let Some(g) = dvd(folder) else { continue };
        assert!(!g.exe.build.dos_subtitle_texts);
    }
}

/// The Japanese build's breaker on the golden's samples: the rows of
/// FUN_0042f618 emulated with the language counter at 0 (the port's
/// Japanese rows) and at 1 (the reference's rows).
#[test]
fn japanese_breaker_samples() {
    let want = golden_in(GOLDEN_JA, "samples");
    assert!(want.len() >= 10);
    let split = CjkSplit::of_build(&DVD_JA, false).unwrap();
    let unhex = |h: &str| -> Vec<u8> {
        (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
            .collect()
    };
    for (k, v) in &want {
        let mut f = k.split(' ');
        let (Some("counter"), Some(counter), Some(text)) = (f.next(), f.next(), f.next()) else {
            panic!("{k}");
        };
        let t = if text == "-" { Vec::new() } else { unhex(text) };
        let rows = if counter == "0" {
            split_subtitle_cjk(&t, split)
        } else {
            split_subtitle(&t)
        };
        let got: Vec<String> = rows
            .iter()
            .map(|r| {
                if r.is_empty() {
                    "-".to_string()
                } else {
                    hex(r)
                }
            })
            .collect();
        assert_eq!(&got.join("|"), v, "{k}");
    }
}

/// Every Japanese dialog line (CHAPJ, language counter 0) cut at the
/// spaces and every English one (CHAPI, counter 1) as the reference, each
/// file's rows against FUN_0042f618 emulated.
#[test]
fn japanese_subtitle_rows_match_the_original() {
    let Some(g) = dvd("JAP") else { return };
    assert_eq!(g.exe.build.name, JA);
    let split = CjkSplit::of_build(g.exe.build, false).unwrap();
    let want = golden_in(GOLDEN_JA, "CHAPJ");
    rows_match_with(&g, JA, Some("CHAPJ"), &want, |t| {
        split_subtitle_cjk(t, split)
    });
    let want = golden_in(GOLDEN_JA, "CHAPI");
    rows_match_with(&g, JA, Some("CHAPI"), &want, split_subtitle);
}

/// The Japanese build's fixes: those of every build, the hexagon's and
/// the missing logos'; not the `ß` of its leftover texts no build reads
/// (ALLSAVE.TXT, CREDTEST.TXT).
#[test]
fn the_fixes_of_the_japanese_dvd() {
    let Some(g) = dvd("JAP") else { return };
    let mut want: Vec<BugFix> = BugFix::ALL
        .into_iter()
        .filter(|f| f.of_every_build())
        .collect();
    want.extend([BugFix::HexagonPieces, BugFix::StartupLogos]);
    assert_eq!(BugFix::of_game(&g), want);
}

/// A dialog line of a CON file: its text and its final blanks.
fn con_line(g: &Game, file: &str, section: i32, letter: char) -> Option<(Vec<u8>, Vec<u8>)> {
    let con = g.big.read(&format!("SCENAR\\{file}"))?;
    let script = ConScript::parse(&con);
    let bytes = |s: &str| s.chars().map(|c| c as u32 as u8).collect::<Vec<u8>>();
    script
        .sections()
        .iter()
        .filter(|s| s.id == section)
        .find_map(|s| {
            s.items
                .iter()
                .filter_map(|i| match i {
                    ConItem::Line(l) => Some(l),
                    ConItem::Set { .. } => None,
                })
                .enumerate()
                .find(|(k, _)| con_track_letter(*k) == letter)
                .map(|(_, l)| (bytes(&l.text), bytes(&l.trail)))
        })
}

/// The white (0xffff) pixels of the rows band from y (18 high) from x 5.
fn white_in(screen: &[u16], y: i32) -> usize {
    (y.max(0)..(y + 18).min(480))
        .flat_map(|y| (5..635).map(move |x| (x, y)))
        .filter(|&(x, y)| screen[y as usize * 640 + x as usize] == 0xffff)
        .count()
}

/// The Japanese build from the start-up: with the original, the error box
/// of each missing logo film (its text the original's), then the Cryo
/// logo; the player screen, a new player, the clicks to the first dialog
/// line with a subtitle: CHAPJ001's first, its rows the original's (the
/// golden's), drawn 18 px apart from the bottom row's base 0x1f2, the
/// empty last row of its final space counted, and the line played with
/// that many subtitles. (The save, the LOAD list's sprite rows and the
/// load: `save_and_load_japanese`, `japanese_load_list_and_captions`.)
fn first_dialog_japanese_with(enhancements: Enhancements, suffix: &str) {
    let Some(g) = dvd("JAP") else { return };
    let Some(probe) = dvd("JAP") else { return };
    assert_eq!(g.exe.build.name, JA);
    let original = enhancements == Enhancements::original();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let store = Rc::new(MemStore::new());
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        subtitles: false,
        enhancements,
        ..Options::default()
    };
    let mut h = Headless::new(g, opts);
    assert!(to_player_screen(&mut h, &trace), "{JA}: no player screen");
    let boxes = error_boxes(&trace);
    let want: Vec<String> = if original {
        ["logogaga.hnm", "logospik.hnm"]
            .iter()
            .map(|n| format!("Fichier introuvable : \\face_b\\ubb_vue\\{n}"))
            .collect()
    } else {
        Vec::new()
    };
    assert_eq!(boxes, want, "{JA}: the logos' error boxes");
    let videos: Vec<String> = trace
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::Video { name, .. } => Some(name.to_ascii_uppercase()),
            _ => None,
        })
        .collect();
    assert_eq!(videos.first().map(String::as_str), Some("LOGO15N.HNM"));
    assert!(!videos
        .iter()
        .any(|v| v.starts_with("LOGOGAGA") || v.starts_with("LOGOSPIK")));
    h.steps(10);
    h.click(200, 259);
    for c in "Tester".chars() {
        h.key(Key::Char(c));
    }
    h.steps(2);
    h.key(Key::Enter);
    assert!(
        h.until(2000, |_| trace.borrow().contains(&Event::ChapterLoaded(1))),
        "{JA}: chapter 1 not loaded"
    );
    // SUBTITLES is 0 (the file's, and the options given): the Japanese
    // subtitles show all the same (the language counter is 0).
    let mut found = None;
    for _ in 0..400 {
        let seen = trace.borrow().len();
        h.steps(20);
        let hit = trace.borrow()[seen..].iter().find_map(|e| match e {
            Event::Line {
                section, letter, ..
            } => Some((*section, *letter)),
            _ => None,
        });
        if let Some((section, letter)) = hit {
            if let Some((text, trail)) = con_line(&probe, "CHAPJ001.CON", section, letter) {
                if !text.trim_ascii_end().is_empty() {
                    found = Some((section, letter, text, trail));
                    break;
                }
            }
        }
        if !h.sys.mouse_locked() {
            h.click(320, 240);
        }
    }
    let (section, letter, text, trail) = found.expect("no dialog line");
    let full = [text.as_slice(), &trail].concat();
    let split = CjkSplit::of_build(&DVD_JA, false).unwrap();
    let rows = split_subtitle_cjk(&full, split);
    // CHAPJ001's first dialog line, its rows the original's.
    let con = probe.big.read("SCENAR\\CHAPJ001.CON").unwrap();
    assert_eq!(
        dialog_texts(&con)[0],
        &full[..],
        "{JA}: not CHAPJ001's first line"
    );
    let lens: Vec<String> = rows.iter().map(|r| r.len().to_string()).collect();
    let want = golden_in(GOLDEN_JA, "CHAPJ");
    assert_eq!(
        Some(&lens.join(",")),
        want.get("first line SCENAR\\CHAPJ001.CON"),
        "{JA}: the first dialog's rows"
    );
    assert!(rows.last().unwrap().is_empty(), "{JA}: {lens:?}");
    // On the screen: row i of n at 0x1f2 - (n - i + 1) * 18, white over
    // its shadow; nothing in the band above.
    let n = rows.len() as i32;
    let y = |i: i32| 0x1f2 - (n - i + 1) * 18;
    let mut shot = Vec::new();
    for _ in 0..200 {
        h.step();
        let screen = h.sys.screen.borrow().clone();
        let drawn = rows
            .iter()
            .enumerate()
            .all(|(i, r)| r.is_empty() || white_in(&screen, y(i as i32)) >= 15);
        if drawn {
            shot = screen;
            break;
        }
    }
    assert!(!shot.is_empty(), "{JA}: the rows not on the screen");
    assert!(white_in(&shot, y(0) - 18) < 15, "{JA}: a row above");
    if let Some(dir) = std::env::var_os("ATLANTIS_DVD_SHOTS") {
        let png = png::encode_rgb565(&shot, 640, 480);
        std::fs::write(
            PathBuf::from(dir).join(format!("dvd-JAP-first-dialog{suffix}.png")),
            png,
        )
        .unwrap();
    }
    assert!(
        h.until(3000, |_| trace.borrow().iter().any(|e| matches!(
            e,
            Event::LinePlayed { section: s, letter: l, subtitles, .. }
                if *s == section && *l == letter && *subtitles == rows.len()
        ))),
        "{JA}: the line's subtitles"
    );
    h.sys.push_input(Input::Quit);
    assert!(h.until(600, |hh| !hh.alive), "{JA}: the game did not end");
}

#[test]
fn first_dialog_japanese() {
    first_dialog_japanese_with(Enhancements::original(), "");
}

/// With the missing logos' fix: no error box, the same first dialog.
#[test]
fn first_dialog_japanese_without_the_logos_boxes() {
    first_dialog_japanese_with(
        Enhancements::only_fix(BugFix::StartupLogos),
        "-startup-logos-fix",
    );
}

#[test]
fn save_and_load_japanese() {
    save_and_load("JAP", JA);
}

/// The share of a sprite's opaque pixels on screen that show it, drawn
/// with its hotspot at (x, y).
fn shown(screen: &[u16], spr: &SprSprite, x: i32, y: i32) -> f64 {
    let (bx, by) = (x - i32::from(spr.xoff), y - i32::from(spr.yoff));
    let (mut opaque, mut same) = (0usize, 0usize);
    for j in 0..i32::from(spr.h) {
        for i in 0..i32::from(spr.w) {
            let k = (j * i32::from(spr.w) + i) as usize;
            let (sx, sy) = (bx + i, by + j);
            if spr.blend[k] != SPR_NO_BLEND || !(0..640).contains(&sx) || !(0..480).contains(&sy) {
                continue;
            }
            opaque += 1;
            if screen[sy as usize * 640 + sx as usize] == spr.pixels[k] {
                same += 1;
            }
        }
    }
    assert!(opaque > 0, "a sprite with no opaque pixel on screen");
    same as f64 / opaque as f64
}

fn bank(g: &Game, name: &str) -> Vec<SprSprite> {
    let d = g.big.read(&format!("SPRITE\\2D\\{name}")).unwrap();
    read_spr_file(&d, limits::DEFAULT.0, limits::DEFAULT.1).unwrap()
}

/// The Japanese build's LOAD list: its rows the EPI1 frames by row (the
/// Chinese builds' code, `LoadList::Sprites`), whatever the checkpoints'
/// chapters; its 74 chapters and widths from the executable; and the
/// opening captions' bank, SUBINTRO, of 13 frames.
#[test]
fn japanese_load_list_and_captions() {
    let Some(g) = dvd("JAP") else { return };
    let Some(probe) = dvd("JAP") else { return };
    let build = g.exe.build;
    let atlantis_data::LoadList::Sprites { banks, rows, .. } = build.load_list else {
        panic!("not a sprite LOAD list");
    };
    assert_eq!(banks, ["EPI1.SPR", "EPI2.SPR"]);
    let frames = bank(&probe, banks[0]);
    assert_eq!(frames.len(), usize::from(rows));
    assert_eq!(bank(&probe, banks[1]).len(), usize::from(rows));
    assert_eq!(g.exe.load_chapters.len(), 74);
    assert_eq!(g.exe.load_widths.len(), 74);
    // The chapter table is the Chinese builds' (the same 74 chapters).
    assert_eq!(g.exe.load_chapters[..6], [2, 3, 4, 5, 6, 9]);
    assert_eq!(*g.exe.load_chapters.last().unwrap(), 85);
    for (i, (f, &w)) in frames.iter().zip(&g.exe.load_widths).enumerate() {
        if f.w < 0x100 {
            assert_eq!(f.w, u16::from(w), "frame {i}");
        }
    }
    let captions = bank(&probe, build.cinema_captions.unwrap().bank);
    assert_eq!(captions.len(), 13);

    // In play: three checkpoints, the list's rows 0..2 as EPI1 frames 0..2.
    let store = Rc::new(MemStore::new());
    checkpoint_of_chapter_2(g, store.clone());
    for n in ["pa5.gam", "pa9.gam"] {
        store.write(n, &store.read("pa2.gam").unwrap());
    }
    let Some(g2) = dvd("JAP") else { return };
    let trace = Rc::new(RefCell::new(Vec::new()));
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        enhancements: Enhancements::only_fix(BugFix::StartupLogos),
        ..Options::default()
    };
    let mut h = Headless::new(g2, opts);
    assert!(to_player_screen(&mut h, &trace), "no player screen");
    h.steps(10);
    h.click(200, 259);
    h.click(200, 259);
    assert!(h.until(2000, |_| trace
        .borrow()
        .iter()
        .any(|e| matches!(e, Event::CheckpointLoaded(_)))));
    let mut menu = false;
    for _ in 0..400 {
        h.steps(30);
        if h.sys.mouse_locked() {
            h.key(Key::Escape);
            if h.until(10, |_| menus(&trace, "MAINMENU.TXT") > 0) {
                menu = true;
                break;
            }
        } else {
            h.click(320, 240);
        }
    }
    assert!(menu, "the in-game menu did not open");
    let load = record_centre(&probe, "MAINMENU.TXT", 0);
    h.click(load.0, load.1);
    assert!(h.until(30, |_| menus(&trace, "LOADMENU.TXT") > 0));
    h.sys.push_input(Input::MouseMove { x: 639, y: 0 });
    h.steps(10);
    let screen = h.sys.screen.borrow().clone();
    for (k, row) in frames.iter().enumerate().take(3) {
        let at = shown(&screen, row, 0x37, 0x6e + 22 * k as i32);
        assert!(at > 0.9, "row {k}: {at}");
    }
    if let Some(dir) = std::env::var_os("ATLANTIS_DVD_SHOTS") {
        let png = png::encode_rgb565(&screen, 640, 480);
        std::fs::write(PathBuf::from(dir).join("dvd-JAP-load-list-rows.png"), png).unwrap();
    }
    h.sys.push_input(Input::Quit);
    assert!(h.until(600, |hh| !hh.alive), "the game did not end");
}

/// The three-state SUBTITLES button of the Japanese build, from a start
/// with SUBTITLES off: Japanese (CHAPJ, shown, rows at the spaces), then
/// at each click English (CHAPI read again, shown, the reference's rows),
/// none (SUBTITLES forced off), Japanese again; the other builds keep the
/// plain rule (shown with SUBTITLES only, CHAPI).
#[test]
fn japanese_subtitle_languages() {
    use atlantis_engine::scripts::con_file_name;
    use atlantis_engine::ui::layout::subtitle_toggle_click;
    let Some(g) = dvd("JAP") else { return };
    let Some(probe) = dvd("JAP") else { return };
    let chapj = probe.big.read("SCENAR\\CHAPJ001.CON").unwrap();
    let chapi = probe.big.read("SCENAR\\CHAPI001.CON").unwrap();
    assert_ne!(chapj, chapi);
    let sys = Sys::new(Mixer::new());
    let s2 = sys.clone();
    let done = Rc::new(RefCell::new(false));
    let d2 = done.clone();
    let mut r = Runner::new(sys.clone(), async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.saves = Rc::new(MemStore::new());
        let marker = e.game.exe.new_player_marker.clone();
        e.players = Players::new(&marker);
        e.player = e.players.create(b"Tester").expect("a row");
        e.subtitles = false;
        let mut scripts = Scripts::default();
        // The chapter's load reads the language of the counter.
        e.load_checkpoint(&mut scripts, false).await;
        assert_eq!(con_file_name(&e, 1), "SCENAR\\CHAPJ001.CON");
        assert_eq!(scripts.script.text(), &chapj[..]);
        assert!(e.japanese_texts() && e.subtitles_shown());
        assert!(e.subtitle_split().is_some());
        let mut on = e.subtitles;
        let want = [
            // (counter, SUBTITLES, shown, CON)
            (1, true, true, &chapi),
            (2, false, false, &chapi),
            (0, true, true, &chapj),
            (1, false, true, &chapi),
        ];
        for (counter, subtitles, shown, con) in want {
            subtitle_toggle_click(&mut on, &mut e.subtitle_clicks, true);
            e.subtitles = on;
            scripts.reload_con_text(&mut e).await;
            assert_eq!(
                (e.subtitle_clicks, e.subtitles, e.subtitles_shown()),
                (counter, subtitles, shown)
            );
            assert_eq!(scripts.script.text(), &con[..], "counter {counter}");
            assert_eq!(e.subtitle_split().is_some(), counter == 0);
        }
        *d2.borrow_mut() = true;
    });
    let mut t = 0;
    while r.step(t) {
        t += 16;
        assert!(t < 16 * 200_000, "did not end");
    }
    assert!(*done.borrow());
    // Another build: SUBTITLES alone, CHAPI.
    if let Some(g) = dvd("ENG") {
        let mut e = Engine::new(Sys::new(Mixer::new()), g, Rc::new(DirectLoader));
        e.subtitles = false;
        assert!(!e.subtitles_shown() && !e.japanese_texts());
        assert_eq!(con_file_name(&e, 1), "SCENAR\\CHAPI001.CON");
    }
}

/// The SUBTITLES button in play: in navigation, the in-game menu, OPTIONS,
/// one click on the button (ENGLISH), OK: the chapter's CHAPI text read
/// again when the menu closes; again (NONE): CHAPI again. (The rows of
/// each language on the screen: the dialog presenter's test
/// `japanese_and_english_rows_on_the_screen`.)
#[test]
fn japanese_subtitle_button_in_play() {
    let Some(g) = dvd("JAP") else { return };
    let Some(probe) = dvd("JAP") else { return };
    let trace = Rc::new(RefCell::new(Vec::new()));
    let opts = Options {
        trace: Some(trace.clone()),
        saves: Rc::new(MemStore::new()),
        start: Start::Boot,
        enhancements: Enhancements::only_fix(BugFix::StartupLogos),
        ..Options::default()
    };
    let mut h = Headless::new(g, opts);
    assert!(to_player_screen(&mut h, &trace), "no player screen");
    h.steps(10);
    h.click(200, 259);
    for c in "Tester".chars() {
        h.key(Key::Char(c));
    }
    h.steps(2);
    h.key(Key::Enter);
    assert!(h.until(2000, |_| trace.borrow().contains(&Event::ChapterLoaded(1))));
    // Navigation: the in-game menu, OPTIONS, the SUBTITLES word, OK.
    let mut menu = false;
    for _ in 0..400 {
        h.steps(30);
        if h.sys.mouse_locked() {
            h.key(Key::Escape);
            if h.until(10, |_| menus(&trace, "MAINMENU.TXT") > 0) {
                menu = true;
                break;
            }
        } else {
            h.click(320, 240);
        }
    }
    assert!(menu, "the in-game menu did not open");
    let options = record_centre(&probe, "MAINMENU.TXT", 1);
    h.click(options.0, options.1);
    assert!(h.until(30, |_| menus(&trace, "OPTMENU.TXT") > 0));
    let word = record_centre(&probe, "OPTMENU.TXT", 3);
    h.click(word.0, word.1);
    let ok = record_centre(&probe, "OPTMENU.TXT", 6);
    h.click(ok.0, ok.1);
    assert!(
        h.until(30, |_| trace.borrow().contains(&Event::ConTextReloaded(
            "SCENAR\\CHAPI001.CON".into()
        ))),
        "CHAPI001 not read again"
    );
    // A second click: NONE, CHAPI001 again (the counter 2 reads CHAPI).
    h.steps(30);
    let mut reopened = false;
    for _ in 0..100 {
        h.steps(10);
        if h.sys.mouse_locked() {
            h.key(Key::Escape);
            if h.until(10, |_| menus(&trace, "MAINMENU.TXT") > 1) {
                reopened = true;
                break;
            }
        }
    }
    assert!(reopened, "the in-game menu did not open again");
    h.click(options.0, options.1);
    assert!(h.until(30, |_| menus(&trace, "OPTMENU.TXT") > 1));
    h.click(word.0, word.1);
    h.click(ok.0, ok.1);
    let reloads = |t: &Rc<RefCell<Vec<Event>>>| {
        t.borrow()
            .iter()
            .filter(|e| matches!(e, Event::ConTextReloaded(_)))
            .count()
    };
    assert!(h.until(30, |_| reloads(&trace) == 2), "no second reload");
    assert_eq!(
        trace
            .borrow()
            .iter()
            .rev()
            .find(|e| matches!(e, Event::ConTextReloaded(_))),
        Some(&Event::ConTextReloaded("SCENAR\\CHAPI001.CON".into()))
    );
    h.sys.push_input(Input::Quit);
    assert!(h.until(600, |hh| !hh.alive), "the game did not end");
}

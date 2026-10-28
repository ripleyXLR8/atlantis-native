//! The German retail CD's first pressing (atlantis-cd-de-693248, May 1997:
//! the English May 1997 build's code): its subtitles (breaker
//! `FUN_0042f6fc`, drawer `FUN_0042f8c8`, `DIALOG\FONTS.BIN`, glyph = byte)
//! against the rows and the picture of their emulation, its DOS code page
//! letters, its menu font advances, the `ß` of its LOAD list, and from the
//! start-up a new game to its first dialog line (its rows on the screen), a
//! save and its load from the player screen and the LOAD list. The expected
//! values are numbers and digests only.
//!
//! Needs the folder of the CD's images (`disc1.iso` .. `disc4.iso`):
//! set `ATLANTIS_DATA_DE_MAY97`; without it the tests are skipped. Only the
//! folder's `.iso` images are read (the folder may hold other copies of
//! the discs).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::disc_image::{DiscImage, DiscSet};
use atlantis_data::saves::{epi_episode_name, Players};
use atlantis_data::sprite::{limits, read_spr_file};
use atlantis_data::{Game, TextCodePage};
use atlantis_engine::audio::Mixer;
use atlantis_engine::checkpoint::PLAYERS_FILE;
use atlantis_engine::con_script::script::{latin1_bytes, ConItem};
use atlantis_engine::con_script::{con_track_letter, ConScript};
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::scripts::Scripts;
use atlantis_engine::subtitles::{bitmap_line_y, split_subtitle_1997, BitmapFont};
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::ui::layout::{
    font_min_blits, menu_number, walk_menu_text, FontMin, FontMinAdvances, MenuRules, MenuWalk,
    FONTMAX_WIDTHS, FONTMIN_ADVANCE, SHARP_S,
};
use atlantis_engine::{
    BugFix, BugFixes, Engine, Enhancements, Event, MemStore, Options, SaveStore, Start,
};
use md5::{Digest, Md5};

const VAR: &str = "ATLANTIS_DATA_DE_MAY97";
const BUILD: &str = "atlantis-cd-de-693248";

/// The golden of the breaker: FUN_0042f6fc run unmodified under emulation
/// on every dialog line (md5s of the rows, their counts, one picture's).
const GOLDEN: &str = include_str!("golden/video_subtitles_de693248.txt");

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

/// The `.iso` images of `dir`, in name order.
fn iso_set(dir: &Path) -> DiscSet {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .expect("the images' folder")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
        .collect();
    paths.sort();
    let images: Vec<DiscImage> = paths.iter().filter_map(|p| DiscImage::open(p)).collect();
    assert_eq!(images.len(), 4, "four disc images in {}", dir.display());
    DiscSet { images }
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(VAR) else {
        eprintln!("{VAR} not set: {test} skipped");
        return None;
    };
    let g = Game::open(Arc::new(iso_set(dir.as_ref()))).expect("the game from its images");
    assert_eq!(g.exe.build.name, BUILD);
    Some(g)
}

/// The golden's "key value" lines (the key everything before the last
/// space).
fn golden() -> BTreeMap<String, String> {
    GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.rsplit_once(' '))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
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

/// The CON files (first copies over the discs), in name order.
fn con_files(g: &Game) -> Vec<(String, Vec<u8>)> {
    let mut names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|n| {
            let data = g.big.read(&n).unwrap();
            (n, data)
        })
        .collect()
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Every dialog line through the port's breaker, as the first line after
/// start-up: each CON file's rows (each line's rows joined by NUL, the
/// lines by LF) against the original's, emulated, and the statistics.
#[test]
fn con_lines_match_original() {
    let Some(g) = game("con_lines_match_original") else {
        return;
    };
    assert!(g.exe.build.bitmap_subtitles);
    let want = golden();
    let mut total = Md5::new();
    let (mut lines, mut hist, mut longest, mut over79, mut over84) = (0, [0usize; 8], 0, 0, 0);
    for (name, con) in con_files(&g) {
        let mut h = Md5::new();
        for t in dialog_texts(&con) {
            let rows = split_subtitle_1997(t);
            lines += 1;
            hist[rows.len().min(7)] += 1;
            for r in &rows {
                longest = longest.max(r.len());
                over79 += usize::from(r.len() > 79);
                over84 += usize::from(r.len() > 84);
            }
            h.update(rows.join(&0u8));
            h.update(b"\n");
        }
        let d: [u8; 16] = h.finalize().into();
        assert_eq!(Some(&hex(&d)), want.get(&name), "{name}");
        total.update(d);
    }
    assert_eq!(Some(&hex(&total.finalize())), want.get("rows md5"));
    assert_eq!(lines, 2540);
    assert_eq!(hist, [0, 1778, 638, 96, 22, 5, 1, 0]);
    assert_eq!((longest, over79, over84), (82, 9, 0));
}

/// The dialog texts' bytes above 0x7f, and the letters code page 850 reads
/// in them (FONTS.BIN's code page 437 glyphs show the same letters).
#[test]
fn dos_code_page_letters() {
    let Some(g) = game("dos_code_page_letters") else {
        return;
    };
    assert_eq!(g.exe.build.text_code_page, TextCodePage::Dos850);
    let mut count = [0usize; 256];
    for (_, con) in con_files(&g) {
        for t in dialog_texts(&con) {
            for &c in t {
                count[usize::from(c)] += 1;
            }
        }
    }
    let want = [
        (0x81, 534, 'ü'),
        (0x84, 798, 'ä'),
        (0x8e, 13, 'Ä'),
        (0x94, 612, 'ö'),
        (0x9a, 18, 'Ü'),
        (0xe1, 544, 'ß'),
    ];
    let high: Vec<usize> = (0x80..0x100).filter(|&c| count[c] > 0).collect();
    assert_eq!(high, want.map(|w| w.0).to_vec());
    for (b, n, letter) in want {
        assert_eq!(count[b], n, "{b:#x}");
        assert_eq!(TextCodePage::Dos850.char_of(b as u8), letter);
    }
}

/// The subtitle the emulation rendered: the first dialog line with a `ß`,
/// four bytes above 0x7f and two rows or more, drawn over a mid grey
/// screen; pixel rows 400..479, RGB565.
#[test]
fn umlaut_subtitle_pixels_match_original() {
    let Some(g) = game("umlaut_subtitle_pixels_match_original") else {
        return;
    };
    let data = g.big.read("DIALOG\\FONTS.BIN").expect("FONTS.BIN");
    assert_eq!(md5_hex(&data), "4aec21ccbf3bd593dc01751adb834964");
    let font = BitmapFont::new(&data).unwrap();
    let text = con_files(&g)
        .iter()
        .flat_map(|(_, con)| {
            dialog_texts(con)
                .into_iter()
                .map(<[u8]>::to_vec)
                .collect::<Vec<_>>()
        })
        .find(|t| {
            t.contains(&0xe1)
                && t.iter().filter(|&&c| c >= 0x80).count() >= 4
                && split_subtitle_1997(t).len() >= 2
        })
        .unwrap();
    let rows = split_subtitle_1997(&text);
    assert_eq!((rows.len(), text.len()), (3, 181));
    let mut screen = vec![0x8410u16; 640 * 480];
    font.draw_subtitles(&mut screen, 640, &rows, 0);
    let crop: Vec<u8> = screen[400 * 640..]
        .iter()
        .flat_map(|p| p.to_le_bytes())
        .collect();
    assert_eq!(md5_hex(&crop), "a34a3a2e5bc8cdfd4995c4060980e3b7");
    let count = |c: u16| screen.iter().filter(|&&p| p == c).count();
    assert_eq!((count(0xffff), count(0)), (3017, 1724));
}

/// The menu font advances of this build are the ones the port carries:
/// FONTMAX at 0x004865ea, FONTMIN at 0x00486673 (file offsets 0x737ea and
/// 0x73873: DGROUP at 0x00480000, raw 0x6d200).
#[test]
fn menu_font_advances() {
    let Some(g) = game("menu_font_advances") else {
        return;
    };
    let exe = g.big.source().read("ATLANTIS.EXE").unwrap();
    assert_eq!(exe[0x737ea..0x737ea + FONTMAX_WIDTHS.len()], FONTMAX_WIDTHS);
    assert_eq!(
        exe[0x73873..0x73873 + FONTMIN_ADVANCE.len()],
        FONTMIN_ADVANCE
    );
}

/// EPI.TXT's `ß` (disc 1's copy, which the LOAD list reads): no FONTMIN
/// glyph, the pen moved 7, as on the August CD; the fix applies.
#[test]
fn load_list_sharp_s() {
    let Some(g) = game("load_list_sharp_s") else {
        return;
    };
    let epi = g.big.read("SPRLIST\\EPI.TXT").unwrap();
    let d = limits::DEFAULT;
    let fontmin = read_spr_file(&g.big.read("SPRITE\\2D\\FONTMIN.SPR").unwrap(), d.0, d.1).unwrap();
    assert_eq!(fontmin.len(), 137);
    let adv = FontMinAdvances::new(&g.exe.credit_widths);
    let named: Vec<Vec<u8>> = (2..200)
        .map(|c| epi_episode_name(&epi, c))
        .filter(|n| n.contains(&SHARP_S))
        .collect();
    assert_eq!(named.len(), 3);
    for name in &named {
        let raw = font_min_blits(FontMin::LoadList, name, 55, 130, &adv);
        let at = raw.iter().position(|b| b.idx == SHARP_S - 0x20).unwrap();
        assert!(fontmin.get(usize::from(raw[at].idx)).is_none());
        assert_eq!(raw[at + 1].x - raw[at].x, 7);
    }
    assert!(BugFix::of_game(&g).contains(&BugFix::SharpS));
}

/// The bug fixes the options list for this CD: the hexagon's (the 1997
/// code), the `ß` of its LOAD list and the disc revisions' (disc 1's
/// CREDBLAN.SPR).
#[test]
fn the_fixes_of_this_cd() {
    let Some(g) = game("the_fixes_of_this_cd") else {
        return;
    };
    let own: Vec<BugFix> = BugFix::of_game(&g)
        .into_iter()
        .filter(|f| !f.of_every_build())
        .collect();
    assert_eq!(
        own,
        [BugFix::HexagonPieces, BugFix::SharpS, BugFix::DiscRevision]
    );
}

/// Runs `f` on an engine of the game with its interface loaded.
fn with_engine(g: Game, f: impl AsyncFnOnce(&mut Engine) + 'static) {
    let sys = Sys::new(Mixer::new());
    let done = Rc::new(RefCell::new(false));
    let (s2, d2) = (sys.clone(), done.clone());
    let mut runner = Runner::new(sys, async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.load_interface().await;
        f(&mut e).await;
        *d2.borrow_mut() = true;
    });
    let mut n = 0u64;
    while !*done.borrow() {
        runner.step(n * 16);
        n += 1;
        assert!(n < 10_000, "the task does not end");
    }
}

/// The credits fonts, read at each credits run from the disc in the drive:
/// disc 1's CREDBLAN.SPR draws `b` (frame 65) 2 pixels to the left of the
/// place discs 2..4's draw it (hotspot x 2 against 0); CREDBLEU.SPR is
/// the same on every disc. With the disc revision's fix, disc 1's
/// whatever the disc.
#[test]
fn credits_fonts_of_the_disc_in_the_drive() {
    let Some(g) = game("credits_fonts_of_the_disc_in_the_drive") else {
        return;
    };
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = out.clone();
    with_engine(g, async move |e: &mut Engine| {
        for (enh, disc) in [
            (Enhancements::original(), 1),
            (Enhancements::original(), 2),
            (Enhancements::original(), 4),
            (Enhancements::only_fix(BugFix::DiscRevision), 4),
            (Enhancements::all_but_fix(BugFix::DiscRevision), 3),
        ] {
            e.sys.set_enhancements(enh);
            e.disc = disc;
            let [blan, bleu] = e.load_credits_fonts().await;
            assert_eq!((blan.len(), bleu.len()), (136, 136));
            let b = usize::from(b'b' - 0x21);
            o.borrow_mut()
                .push((blan[b].xoff, bleu[b].xoff, blan[b].pixels.len()));
        }
    });
    let out = out.borrow();
    let xoffs: Vec<i16> = out.iter().map(|o| o.0).collect();
    assert_eq!(xoffs, [2, 0, 0, 2, 0]);
    assert!(out.iter().all(|o| o.1 == out[0].1 && o.2 == out[0].2));
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

fn menus(t: &Rc<RefCell<Vec<Event>>>, m: &str) -> usize {
    t.borrow()
        .iter()
        .filter(|e| matches!(e, Event::Menu(n) if n == m))
        .count()
}

/// From the start-up to the player screen: clicks through the films.
fn to_player_screen(h: &mut Headless, trace: &Rc<RefCell<Vec<Event>>>) -> bool {
    for _ in 0..100 {
        h.steps(30);
        if menus(trace, "SELEMENU.TXT") > 0 {
            return true;
        }
        h.click(320, 240);
    }
    false
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
                .map(|(_, l)| latin1_bytes(&l.text))
        })
}

/// From the start-up: the logos, the player screen, a new player, the
/// first chapter, the clicks through the opening to the first dialog line
/// with a subtitle: CHAPI001's first line, its rows as the original's
/// breaker cuts them, drawn on the screen in the bitmap font, white over
/// the black shadow; then the line played with that many rows.
#[test]
fn new_game_to_the_first_dialog() {
    let Some(g) = game("new_game_to_the_first_dialog") else {
        return;
    };
    let Some(probe) = game("new_game_to_the_first_dialog") else {
        return;
    };
    let font = BitmapFont::new(&probe.big.read("DIALOG\\FONTS.BIN").unwrap()).unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let store = Rc::new(MemStore::new());
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        subtitles: true,
        ..Options::default()
    };
    let mut h = Headless::new(g, opts);
    assert!(to_player_screen(&mut h, &trace), "no player screen");
    h.steps(10);
    h.click(200, 259);
    for c in "Anna".chars() {
        h.key(Key::Char(c));
    }
    h.steps(2);
    h.key(Key::Enter);
    assert!(
        h.until(2000, |_| trace.borrow().contains(&Event::ChapterLoaded(1))),
        "chapter 1 not loaded"
    );
    // The player row: the typed name upper-cased (FUN_0041e45c), in a
    // players.atl of 95 bytes.
    let file = store.read(PLAYERS_FILE).expect("players.atl");
    assert_eq!(file.len(), 95);
    let marker = probe.exe.new_player_marker.clone();
    let players = Players::read(&file, &marker).expect("the players");
    assert!(
        (0..players.player_count()).any(|i| players.name(i) == b"ANNA"),
        "{:?}",
        (0..players.player_count())
            .map(|i| players.name(i).to_vec())
            .collect::<Vec<_>>()
    );
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
            if let Some(text) = line_text(&probe, 1, section, letter) {
                let trimmed = text.trim_ascii_end().to_vec();
                if !trimmed.is_empty() {
                    found = Some((section, letter, trimmed));
                    break;
                }
            }
        }
        if !h.sys.mouse_locked() {
            h.click(320, 240);
        }
    }
    let (section, letter, text) = found.expect("no dialog line");
    let con = probe.big.read("SCENAR\\CHAPI001.CON").unwrap();
    let raw = dialog_texts(&con)[0];
    assert_eq!(raw.trim_ascii_end(), &text[..], "not CHAPI001's first line");
    let rows = split_subtitle_1997(raw);
    let lens: Vec<String> = rows.iter().map(|r| r.len().to_string()).collect();
    assert_eq!(
        Some(&lens.join(",")),
        golden().get("first line SCENAR\\CHAPI001.CON")
    );
    // The pixels the font sets for the rows and their shadow.
    let mut white = vec![0u16; 640 * 480];
    let mut shadow = vec![0u16; 640 * 480];
    for (i, r) in rows.iter().enumerate() {
        let y = bitmap_line_y(i, rows.len());
        font.draw(&mut white, 640, r, 5, y, 1);
        font.draw(&mut shadow, 640, r, 6, y + 1, 1);
    }
    let mut drawn = false;
    for _ in 0..200 {
        h.step();
        let screen = h.sys.screen.borrow();
        let text_ok = (0..640 * 480).all(|i| white[i] == 0 || screen[i] == 0xffff);
        let shadow_ok = (0..640 * 480).all(|i| shadow[i] == 0 || white[i] == 1 || screen[i] == 0);
        if text_ok && shadow_ok {
            drawn = true;
            break;
        }
    }
    assert!(drawn, "the first line's rows on the screen");
    assert!(
        h.until(3000, |_| trace.borrow().iter().any(|e| matches!(
            e,
            Event::LinePlayed { section: s, letter: l, subtitles, .. }
                if *s == section && *l == letter && *subtitles == rows.len()
        ))),
        "the line's subtitles"
    );
    h.sys.push_input(Input::Quit);
    assert!(h.until(600, |hh| !hh.alive), "the game did not end");
}

/// A checkpoint of chapter 2 for the player "ANNA" in `store`, as the
/// chapter change writes it (`Chapter=2`: the checkpoint, then
/// players.atl).
fn checkpoint_of_chapter_2(g: Game, store: Rc<MemStore>) {
    let sys = Sys::new(Mixer::new());
    let s2 = sys.clone();
    let mut r = Runner::new(sys.clone(), async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.saves = store;
        let marker = e.game.exe.new_player_marker.clone();
        e.players = Players::new(&marker);
        // The 1997 builds' 95-byte file, as the engine lays it out.
        e.players.layout_1997 = e.game.exe.build.players_1997;
        e.player = e.players.create(b"ANNA").expect("a row");
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

/// A save and its load: the checkpoint of a chapter change (players.atl of
/// 95 bytes), the player chosen on the player screen (its checkpoint
/// loaded), then the in-game menu's LOAD list with that checkpoint and its
/// load.
#[test]
fn save_and_load() {
    let Some(g) = game("save_and_load") else {
        return;
    };
    let Some(g2) = game("save_and_load") else {
        return;
    };
    let Some(probe) = game("save_and_load") else {
        return;
    };
    let store = Rc::new(MemStore::new());
    checkpoint_of_chapter_2(g, store.clone());
    assert!(store.read("pa2.gam").is_some(), "{:?}", store.list());
    assert_eq!(store.read(PLAYERS_FILE).map(|p| p.len()), Some(95));
    let trace = Rc::new(RefCell::new(Vec::new()));
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        ..Options::default()
    };
    let mut h = Headless::new(g2, opts);
    assert!(to_player_screen(&mut h, &trace), "no player screen");
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
        "pa2.gam not loaded from the player screen"
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
    assert!(menu, "the in-game menu did not open");
    let load = record_centre(&probe, "MAINMENU.TXT", 0);
    h.click(load.0, load.1);
    assert!(
        h.until(30, |_| menus(&trace, "LOADMENU.TXT") > 0),
        "no LOAD list"
    );
    h.sys.push_input(Input::MouseMove { x: 639, y: 0 });
    h.steps(10);
    let row = (200, 0x6e + 11);
    h.click(row.0, row.1);
    h.click(row.0, row.1);
    assert!(
        h.until(2000, |_| loaded(&trace) > 1),
        "pa2.gam not loaded from the LOAD list"
    );
    h.sys.push_input(Input::Quit);
    assert!(h.until(600, |hh| !hh.alive), "the game did not end");
}

//! The Spanish and Italian retail CDs (atlantis-cd-es-687104 and
//! atlantis-cd-it-687104, the French August 1997 build's code with one
//! changed function each): their subtitle breakers (`FUN_0042e84c` /
//! `FUN_0042e858`) against the statistics of their emulation on every
//! dialog line, the Windows-1252 letters of their CON texts, the Spanish
//! build's rat search restart, and the Italian menu walk (`FUN_0041ecb4`:
//! positioned zones of 16 px per byte, bytes above 0xa8 as spaces) on
//! synthetic texts, on the shipped menus and in a headless confirmation,
//! as the original and with the minor bug fixes. The expected values are
//! numbers only.
//!
//! Needs the game folders of those builds: set `ATLANTIS_DATA_ES1997` and
//! `ATLANTIS_DATA_IT1997`; without them the tests are skipped (the
//! synthetic ones always run).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::exe::{ExeBuild, BUILDS};
use atlantis_data::{Game, StdDir, TextCodePage};
use atlantis_engine::audio::Mixer;
use atlantis_engine::enhancements::Enhancements;
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::subtitles::split_subtitle;
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::ui::layout::{
    menu_walk_hit, walk_menu_text, MenuRules, MenuWalk, FONTMAX_WIDTHS,
};
use atlantis_engine::ui::menus::MenuOutcome;
use atlantis_engine::{Dialogs, Engine, Scripts};

fn game(var: &str, test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(var) else {
        eprintln!("{var} not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

fn build(name: &str) -> &'static ExeBuild {
    BUILDS.iter().find(|b| b.name == name).unwrap()
}

/// The dialog texts of the CON files (first copies), in file-name order.
fn dialog_texts(g: &Game) -> Vec<Vec<u8>> {
    let mut names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    let mut out = Vec::new();
    for n in &names {
        let data = g.big.read(n).unwrap();
        for line in data.split(|&c| c == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.first() != Some(&b'[') {
                continue;
            }
            let Some(close) = line.iter().position(|&c| c == b']') else {
                continue;
            };
            let rest = &line[close + 1..];
            if rest.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            out.push(rest.strip_prefix(b" ").unwrap_or(rest).to_vec());
        }
    }
    out
}

/// Lines, rows per line, the longest row and the bytes above 0x7f, as the
/// emulation of the build's breaker counts them.
fn stats(texts: &[Vec<u8>]) -> (usize, [usize; 9], usize, usize) {
    let (mut hist, mut longest, mut high) = ([0usize; 9], 0, 0);
    for text in texts {
        high += text.iter().filter(|&&c| c >= 0x80).count();
        let rows = split_subtitle(text);
        hist[rows.len().min(8)] += 1;
        for r in &rows {
            longest = longest.max(r.len());
        }
    }
    (texts.len(), hist, longest, high)
}

/// The letters the CON texts' bytes above 0x7f are in Windows-1252.
fn high_letters(texts: &[Vec<u8>]) -> String {
    let mut seen = [false; 256];
    for t in texts {
        for &c in t {
            seen[usize::from(c)] |= c >= 0x80;
        }
    }
    (0x80..=0xffu8)
        .filter(|&c| seen[usize::from(c)])
        .map(|c| TextCodePage::Windows1252.char_of(c))
        .collect()
}

#[test]
fn spanish_con_lines_match_original() {
    let Some(g) = game("ATLANTIS_DATA_ES1997", "spanish_con_lines_match_original") else {
        return;
    };
    assert!(!g.exe.build.bitmap_subtitles);
    assert_eq!(g.exe.build.text_code_page, TextCodePage::Windows1252);
    let texts = dialog_texts(&g);
    assert_eq!(
        stats(&texts),
        (2541, [0, 2183, 331, 23, 2, 2, 0, 0, 0], 100, 5484)
    );
    assert_eq!(high_letters(&texts), "¡¿ÁÉÓÚáéíñóúü");
}

#[test]
fn italian_con_lines_match_original() {
    let Some(g) = game("ATLANTIS_DATA_IT1997", "italian_con_lines_match_original") else {
        return;
    };
    assert!(!g.exe.build.bitmap_subtitles);
    assert_eq!(g.exe.build.text_code_page, TextCodePage::Windows1252);
    let texts = dialog_texts(&g);
    assert_eq!(
        stats(&texts),
        (2541, [0, 2171, 337, 27, 6, 0, 0, 0, 0], 100, 1472)
    );
    assert_eq!(high_letters(&texts), "àèéìòù");
}

/// The Spanish build restarts the rat's T0023 search at point 0
/// (FUN_004348fc @ 0x00434933), the reference's rule; the French August,
/// German and Italian builds do not.
#[test]
fn rat_search_restart_by_build() {
    assert!(build("atlantis-cd-es-687104").rat_search_restart);
    assert!(build("atlantis-cd-en-714240").rat_search_restart);
    for b in [
        "atlantis-cd-fr-687104",
        "atlantis-cd-de-687104",
        "atlantis-cd-it-687104",
    ] {
        assert!(!build(b).rat_search_restart, "{b}");
    }
    if let Some(g) = game("ATLANTIS_DATA_ES1997", "rat_search_restart_by_build") {
        assert!(g.exe.build.rat_search_restart);
    }
}

fn walk(text: &[u8], number: i32, rules: MenuRules) -> MenuWalk {
    let mut w = MenuWalk::default();
    walk_menu_text(text, number, -1, 0, 0, rules, &mut w);
    w
}

/// The positioned records of a synthetic menu: 22 px per byte (every other
/// build, and the Italian one with the minor bug fixes), 16 px (the
/// Italian build as the original); the centred records and the glyphs do
/// not change.
#[test]
fn italian_positioned_zones_synthetic() {
    let text = b"<>,250>TITLE\r\n<170,400>OK \r\n<400,400>Annulla\r\n/fin\r\n";
    let it = build("atlantis-cd-it-687104");
    let widths = |rules: MenuRules| -> Vec<i16> {
        walk(text, 6, rules).records.iter().map(|r| r.w).collect()
    };
    let title = walk(text, 6, MenuRules::default()).records[0].w;
    assert_eq!(widths(MenuRules::default()), [title, 66, 154]);
    assert_eq!(widths(MenuRules::of_build(it, false)), [title, 48, 112]);
    assert_eq!(widths(MenuRules::of_build(it, true)), [title, 66, 154]);
    for b in &BUILDS {
        if b.name != it.name {
            assert_eq!(widths(MenuRules::of_build(b, false)), [title, 66, 154]);
        }
    }
    assert_eq!(
        walk(text, 6, MenuRules::default()).ops,
        walk(text, 6, MenuRules::of_build(it, false)).ops
    );
    // A point right of the 16-px zone of OK (x 159..207), inside the 22-px
    // one (159..225).
    let r16 = walk(text, 6, MenuRules::of_build(it, false)).records;
    let r22 = walk(text, 6, MenuRules::of_build(it, true)).records;
    assert_eq!(menu_walk_hit(&r16, 214, 386), None);
    assert_eq!(menu_walk_hit(&r22, 214, 386), Some(1));
    assert_eq!(menu_walk_hit(&r16, 180, 386), Some(1));
}

/// FUN_0041ecb4 @ 0x0041f235: a byte above 0xa8 is a space (pen + 22,
/// nothing drawn); 0xa8 and below are glyphs. The other builds draw both.
#[test]
fn italian_high_bytes_as_spaces() {
    let text = b"<100,100>A\xa8B\xa9C\r\n/fin\r\n";
    let it = MenuRules::of_build(build("atlantis-cd-it-687104"), false);
    let glyphs = |rules: MenuRules| -> Vec<(i32, i16)> {
        walk(text, 1, rules)
            .ops
            .iter()
            .map(|o| (o.index, o.x))
            .collect()
    };
    let other = glyphs(MenuRules::default());
    let italian = glyphs(it);
    assert_eq!(other.len(), 5);
    assert_eq!(italian.len(), 4);
    assert_eq!(italian[..3], other[..3]);
    assert_eq!(italian[3].0, i32::from(b'C' - 0x20));
    // 0xa9 moved the pen 22 past the end of `B`.
    let b_end = i32::from(other[2].1) + i32::from(FONTMAX_WIDTHS[usize::from(b'B' - 0x20)]);
    assert_eq!(i32::from(italian[3].1), b_end + 22);
    // With the minor bug fixes too (only the zones change).
    assert_eq!(
        glyphs(MenuRules::of_build(build("atlantis-cd-it-687104"), true)),
        italian
    );
}

/// The shipped Italian menus: every positioned record 16 px per byte as
/// the original, 22 with the minor bug fixes (the arithmetic of
/// binaries' zone table: `OK ` 48 / 66, `Annulla` 112 / 154, `Sottotitoli:`
/// 192 / 264, ...); the centred and sprite records, and the glyphs, the
/// same either way; no menu byte above 0xa8.
#[test]
fn italian_shipped_menu_zones_both_ways() {
    let Some(g) = game(
        "ATLANTIS_DATA_IT1997",
        "italian_shipped_menu_zones_both_ways",
    ) else {
        return;
    };
    let b = g.exe.build;
    let menus = [
        ("MAINMENU", 0, vec![]),
        ("LOADMENU", 1, vec![(6, 48, 66), (7, 112, 154)]),
        (
            "OPTMENU",
            2,
            vec![(2, 192, 264), (3, 128, 176), (4, 128, 176), (5, 256, 352)],
        ),
        (
            "SONMENU",
            5,
            vec![
                (10, 96, 132),
                (11, 96, 132),
                (12, 112, 154),
                (13, 64, 88),
                (14, 128, 176),
                (15, 96, 132),
                (16, 48, 66),
                (17, 112, 154),
            ],
        ),
        ("SUREMENU", 6, vec![(3, 48, 66), (4, 112, 154)]),
        ("SELEMENU", 4, vec![]),
    ];
    for (name, number, changed) in menus {
        let text = g.big.read(&format!("SPRLIST\\{name}.TXT")).unwrap();
        assert!(text.iter().all(|&c| c <= 0xa8), "{name}");
        let orig = walk(&text, number, MenuRules::of_build(b, false));
        let fixed = walk(&text, number, MenuRules::of_build(b, true));
        assert_eq!(orig.ops, fixed.ops, "{name}");
        assert_eq!(orig.records.len(), fixed.records.len(), "{name}");
        for (i, (o, f)) in orig.records.iter().zip(&fixed.records).enumerate() {
            assert_eq!((o.x, o.y, o.h), (f.x, f.y, f.h), "{name} {i}");
            match changed.iter().find(|c| c.0 == i) {
                Some(&(_, w16, w22)) => assert_eq!((o.w, f.w), (w16, w22), "{name} {i}"),
                None => assert_eq!(o.w, f.w, "{name} {i}"),
            }
        }
    }
}

// ---- headless ---------------------------------------------------------------

/// The in-game menu's QUIT GAME, then a click on the confirmation right of
/// the `OK` zone the original draws (x 214 of record 159..207, inside the
/// 22-px record 159..225), then Cancel and Escape: the original ignores the
/// click (Cancel leaves the confirmation, the game goes on), the port with
/// the minor bug fixes takes it (the game ends; Escape cuts the credits
/// short).
fn quit_click(g: Game, fixes: bool) -> MenuOutcome {
    let quit = {
        let text = g.big.read("SPRLIST\\MAINMENU.TXT").unwrap();
        let r = walk(&text, 0, MenuRules::of_build(g.exe.build, fixes)).records[2];
        (
            i32::from(r.x) + i32::from(r.w) / 2,
            i32::from(r.y) + i32::from(r.h) / 2,
        )
    };
    let cancel = {
        let text = g.big.read("SPRLIST\\SUREMENU.TXT").unwrap();
        let r = walk(&text, 6, MenuRules::of_build(g.exe.build, fixes)).records[4];
        (
            i32::from(r.x) + i32::from(r.w) / 2,
            i32::from(r.y) + i32::from(r.h) / 2,
        )
    };
    let sys = Sys::new(Mixer::new());
    let out = Rc::new(RefCell::new(None));
    let (s2, o2) = (sys.clone(), out.clone());
    let mut runner = Runner::new(sys.clone(), async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        let mut enh = Enhancements::original();
        enh.minor_bug_fixes = fixes;
        e.sys.set_enhancements(enh);
        e.load_interface().await;
        let mut scripts = Scripts::with_dialog(Box::new(Dialogs::new()));
        e.new_game(&mut scripts, false).await;
        let outcome = e.ingame_menu().await;
        *o2.borrow_mut() = Some(outcome);
    });
    let click = |x: i32, y: i32| {
        sys.push_input(Input::MouseMove { x, y });
        sys.push_input(Input::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        });
    };
    let release = |x: i32, y: i32| {
        sys.push_input(Input::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        });
    };
    let mut n = 0u64;
    let mut stage = 0;
    while out.borrow().is_none() {
        runner.step(n * 16);
        n += 1;
        assert!(n < 50_000, "the menu does not end");
        // Once the game is set up: QUIT GAME, the click right of OK, then
        // Cancel (it leaves the confirmation; the credits ignore it) and
        // Escape (it cuts the credits short; the confirmation reads no key).
        match (stage, n % 60) {
            (0, 0) if n > 600 => {
                click(quit.0, quit.1);
                stage = 1;
            }
            (1, 5) => {
                release(quit.0, quit.1);
                stage = 2;
            }
            (2, 30) => {
                click(214, 386);
                stage = 3;
            }
            (3, 35) => {
                release(214, 386);
                stage = 4;
            }
            (4.., 0) => {
                click(cancel.0, cancel.1);
                sys.push_input(Input::KeyDown(Key::Escape));
                sys.push_input(Input::KeyUp(Key::Escape));
                stage += 1;
            }
            (5.., 5) => release(cancel.0, cancel.1),
            _ => {}
        }
    }
    let outcome = out.borrow().unwrap();
    eprintln!("fixes {fixes}: {outcome:?} after {n} frames, {stage} stages");
    // The menu stayed up until the click on the confirmation, Cancel and
    // Escape.
    assert!(stage >= 5, "the menu ended at stage {stage}");
    outcome
}

#[test]
fn italian_confirmation_zone_headless_both_ways() {
    let Some(g) = game(
        "ATLANTIS_DATA_IT1997",
        "italian_confirmation_zone_headless_both_ways",
    ) else {
        return;
    };
    let g2 = game("ATLANTIS_DATA_IT1997", "").unwrap();
    assert_eq!(quit_click(g, false), MenuOutcome::Resume);
    assert_eq!(quit_click(g2, true), MenuOutcome::Quit);
}

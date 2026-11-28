//! The Simplified Chinese CD (*亚特兰蒂斯3*, 2002; a program of its own,
//! GOG.com English's data, a text hook) played from its first disc's
//! `GAMEDATA\` and its three CD images (`ATLANTIS3_CD_ZH_CN`, optional:
//! `SKIPPED` without it):
//!
//! - the language is `gbr` (English voices), the intro `intro.hnm`, and a
//!   new game reads the Hoggar's first view from disc 2;
//! - the fonts are `font.txt`'s Arial (the English data's), the hook's
//!   Chinese font a 14-pixel cell in the face a Simplified Chinese Windows
//!   gives "宋体" (here the port's Noto Serif SC, no `simsun.ttc` beside the
//!   game);
//! - every `Dial.txt` line, as the dialog draws it at the three widths:
//!   the 529 of 16 bytes or more laid out as their tag and drawn in
//!   Chinese (wrapped upwards when wider than the screen allows), the 88
//!   shorter ones matched line by line, one (a stage direction) left in English;
//!   in the bundled face 35, 15 and 10 of them wider than the screen less
//!   32 pixels, drawn upwards in pieces;
//! - every Chinese character of the table has a glyph in the bundled
//!   face, and a dialog line draws other pixels than the English.

use std::path::Path;
use std::sync::Arc;

use atlantis3_data::testing::cd_zh_cn;
use atlantis3_data::{A3Files, Release};
use atlantis3_engine::text::{
    chars, layout, FontId, GsooDraw, Texts, DIALOG_SHADOW, DIALOG_TEXT, GSOO_FONT_HEIGHT,
};
use atlantis3_engine::{Game, Mode};
use atlantis_data::StdDir;
use atlantis_engine::subtitles::FaceSource;

fn files(install: &Path, discs: &Path) -> A3Files {
    A3Files::with_discs(
        Arc::new(StdDir::new(install)),
        install,
        &[discs.to_path_buf()],
    )
}

#[test]
fn intro_new_game_and_the_first_disc() {
    let Some((install, discs)) = cd_zh_cn() else {
        return;
    };
    let mut g = Game::new(files(&install, &discs)).expect("the game starts");
    assert_eq!(g.program_release(), Release::CdPt2001);
    assert_eq!(g.startup.config.langue, b"gbr");
    g.start_intro();
    let first = g.video().map(|v| v.0.to_string());
    let mut t = 0.0;
    while t < 120.0 && g.mode() == Mode::Intro {
        t += 1.0 / 60.0;
        g.step(t);
    }
    assert_eq!(g.mode(), Mode::Idle, "the intro ends ({first:?})");
    assert_eq!(first.as_deref(), Some("intro.hnm"));
    assert!(g.new_game());
    assert_eq!(g.files().discs().unwrap().current(), 2);
    for _ in 0..30 {
        t += 1.0 / 60.0;
        g.step(t);
    }
    assert!(
        !g.place_faces().any_empty(),
        "the Hoggar's cube from disc 2"
    );
}

/// Every `Dial.txt` line as the dialog prints it, at the three widths.
#[test]
fn dial_lines_through_the_hook() {
    let Some((install, discs)) = cd_zh_cn() else {
        return;
    };
    let f = files(&install, &discs);
    let texts = Texts::load(&f, "gbr");
    let k = texts.gsoo().expect("the text hook");
    assert_eq!(k.face().source(), FaceSource::Bundled);
    let cjk = k.font();
    assert_eq!(cjk.tm_height, GSOO_FONT_HEIGHT);
    assert!(cjk.ascent > 0 && cjk.ascent < cjk.tm_height);
    // An ideograph is one em, ASCII narrower.
    let d = cjk.dx(b"\xb0\xa1A");
    assert_eq!(d.len(), 2);
    assert!(d[0] == GSOO_FONT_HEIGHT && d[1] > 0 && d[1] < d[0], "{d:?}");
    let dial = atlantis3_data::dialog::parse_dial(&f.read("loc/text/gbr/Dial.txt").unwrap());
    assert_eq!(dial.lines.len(), 618);
    for w in [640usize, 800, 1024] {
        let font = texts.gdi(FontId::Default, w).unwrap();
        assert!(font.dbcs.is_none());
        let (mut tagged, mut matched, mut english, mut wrapped) = (0, 0, 0, 0);
        for l in &dial.lines {
            let lib = texts.library_text(&l.text);
            let is_tag = lib.as_ref() != l.text.as_slice();
            for zone in [DIALOG_SHADOW, DIALOG_TEXT] {
                let lines = layout(font, &lib, w as i32, 600, zone);
                if is_tag {
                    assert_eq!(lines.len(), 1, "{} {}", l.dialog, l.line);
                }
                let mut chinese = false;
                for line in &lines {
                    match k.draw(line, w as i32) {
                        GsooDraw::Chinese(pieces) => {
                            chinese = true;
                            assert!(!pieces.is_empty());
                            if pieces[0].x == line.x + 3 {
                                assert_eq!(pieces.len(), 1);
                                assert_eq!(pieces[0].y, line.y + 3);
                            } else {
                                wrapped += 1;
                                // Upwards from the line's place, at its x,
                                // one cell apart (a last piece left empty
                                // when the one before took the text's end).
                                for (a, p) in pieces.iter().enumerate() {
                                    assert_eq!(p.x, line.x);
                                    let up = line.y - 3 - p.y;
                                    assert!(up >= 0 && up % GSOO_FONT_HEIGHT == 0, "{up}");
                                    if let Some(q) = pieces.get(a + 1) {
                                        assert_eq!(q.y, p.y + GSOO_FONT_HEIGHT);
                                    }
                                }
                            }
                            let all: Vec<u8> =
                                pieces.iter().flat_map(|p| p.bytes.clone()).collect();
                            assert!(all.len() > 1 && chars(&all, cjk.code_page()).len() > 1);
                        }
                        GsooDraw::Original { selected } => assert!(!selected),
                    }
                }
                if zone == DIALOG_TEXT {
                    match (is_tag, chinese) {
                        (true, true) => tagged += 1,
                        (false, true) => matched += 1,
                        (_, false) => english += 1,
                    }
                }
            }
        }
        assert_eq!((tagged, matched, english), (529, 88, 1), "at {w}");
        // The texts wider than W − 32 in the bundled face (shadow and
        // text each): 35, 15 and 10 lines.
        let want = match w {
            640 => 70,
            800 => 30,
            _ => 20,
        };
        assert_eq!(wrapped, want, "wrapped draws at {w}");
    }
}

/// Every Chinese character of the table has a glyph in the bundled face;
/// a dialog line, shadow and text, draws in Chinese.
#[test]
fn glyphs_and_a_drawn_line() {
    let Some((install, discs)) = cd_zh_cn() else {
        return;
    };
    let f = files(&install, &discs);
    let texts = Texts::load(&f, "gbr");
    let k = texts.gsoo().unwrap();
    let cjk = k.font();
    let d = cjk.dbcs.as_ref().unwrap();
    let mut missing = Vec::new();
    let mut n = 0;
    for e in k.table().entries() {
        let st = chars(e.chinese, d.code_page);
        for w in st.windows(2) {
            let c = d.char_of(&e.chinese[w[0]..w[1]]);
            if c > ' ' {
                n += 1;
                if !d.face.has_char(c) {
                    missing.push(c);
                }
            }
        }
    }
    missing.sort_unstable();
    missing.dedup();
    assert!(n > 10_000, "{n}");
    assert!(missing.is_empty(), "{} without a glyph", missing.len());
    let dial = atlantis3_data::dialog::parse_dial(&f.read("loc/text/gbr/Dial.txt").unwrap());
    let text = &dial.lines[0].text;
    assert_eq!(texts.library_text(text).len(), 16);
    let (w, h) = (800, 600);
    let mut hooked = vec![0u16; w * h];
    texts.print_shadowed(
        &mut hooked,
        w,
        h,
        text,
        DIALOG_SHADOW,
        DIALOG_TEXT,
        0xff_ffff,
    );
    let lit: Vec<usize> = (0..w * h).filter(|&i| hooked[i] != 0).collect();
    assert!(lit.len() > 100, "{}", lit.len());
    // The Chinese from x + 3 of the tag's line, near the bottom.
    let first_x = lit.iter().map(|i| i % w).min().unwrap();
    assert!((10..=20).contains(&first_x), "{first_x}");
    assert!(lit.iter().all(|i| i / w > 500), "a line at the bottom");
}

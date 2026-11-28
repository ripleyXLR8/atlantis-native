//! The Traditional Chinese CD (*亞特蘭提斯Ⅲ 失落的帝國*, Interwise, 2002;
//! the Portuguese CD's program) played from its installed folder and its
//! three CD images (`ATLANTIS3_CD_ZH_TW`, optional: `SKIPPED` without it):
//!
//! - the language is `gbr` (English voices), the intro the 2003 program's
//!   table for it, and a new game reads the Hoggar's first view from
//!   disc 2;
//! - the five fonts are `CHINESEBIG5_CHARSET`'s: measured per character
//!   in the face a Traditional Chinese Windows gives (here the port's Noto
//!   Serif TC, no `mingliu.ttc` beside the game), `tmHeight` the cell;
//! - every `Dial.txt` line breaks as atlantis3-re's model of the double-
//!   byte layout (`tools/emu/a3_text_model_dbcs.py --dial`, golden
//!   `a3_big5_dial_lines.txt`: ranges and hashes, no text), without a line
//!   the original could not break;
//! - every character of the texts has a glyph in the bundled face, and a
//!   dialog line draws pixels.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use atlantis3_data::testing::cd_zh_tw;
use atlantis3_data::{A3Files, Release};
use atlantis3_engine::text::{
    big5_lines_cut, chars, layout, FontId, Glyphs, Texts, CHARSET_BIG5, DIALOG_TEXT, FONT_IDS,
};
use atlantis3_engine::{Game, Mode};
use atlantis_data::StdDir;
use atlantis_engine::subtitles::FaceSource;
use md5::Digest;

fn files(install: &Path, discs: &Path) -> A3Files {
    A3Files::with_discs(
        Arc::new(StdDir::new(install)),
        install,
        &[discs.to_path_buf()],
    )
}

#[test]
fn intro_new_game_and_the_first_disc() {
    let Some((install, discs)) = cd_zh_tw() else {
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

/// Every font: Big5, the bundled face, `tmHeight` the cell height of
/// `font.txt` at each screen width.
#[test]
fn big5_fonts() {
    let Some((install, discs)) = cd_zh_tw() else {
        return;
    };
    let texts = Texts::load(&files(&install, &discs), "gbr");
    assert_eq!(texts.big5().map(|f| f.source()), Some(FaceSource::Bundled));
    let heights = |id: FontId| match id {
        FontId::Default => [18, 21, 23],
        FontId::Bulle => [17, 19, 20],
        FontId::Special => [21, 24, 26],
        FontId::Titre => [53, 60, 66],
        FontId::Credit => [16, 18, 19],
    };
    for id in FONT_IDS {
        for (k, w) in [640, 800, 1024].into_iter().enumerate() {
            let f = texts.gdi(id, w).unwrap_or_else(|| panic!("{id:?} {w}"));
            assert_eq!(f.charset, CHARSET_BIG5, "{id:?} {w}");
            assert!(f.dbcs.is_some());
            assert_eq!(f.tm_height, heights(id)[k], "{id:?} {w}");
            assert!(f.ascent > 0 && f.ascent < f.tm_height, "{id:?} {w}");
            // An ideograph is one em, ASCII narrower.
            let d = f.dx(b"\xa4\x40A");
            assert_eq!(d.len(), 2);
            assert_eq!(d[0], f.tm_height, "{id:?} {w}");
            assert!(d[1] > 0 && d[1] < d[0], "{id:?} {w}");
        }
    }
}

/// Every `Dial.txt` line, at the three widths: the lines of the RE
/// model's golden; no line the port had to cut.
#[test]
fn dial_lines_as_the_model() {
    let Some((install, discs)) = cd_zh_tw() else {
        return;
    };
    let f = files(&install, &discs);
    let dial = atlantis3_data::dialog::parse_dial(&f.read("loc/text/gbr/Dial.txt").unwrap());
    assert_eq!(dial.lines.len(), 619);
    let golden: HashMap<(String, i32), String> = include_str!("golden/a3_big5_dial_lines.txt")
        .lines()
        .filter(|l| l.starts_with("DIAL "))
        .map(|l| {
            let w: Vec<&str> = l.splitn(6, ' ').collect();
            (
                (w[3].to_string(), w[4].parse().unwrap()),
                w.get(5).copied().unwrap_or("").to_string(),
            )
        })
        .collect();
    let texts = Texts::load(&f, "gbr");
    let (mut multi, mut checked) = (0, 0);
    for w in [640, 800, 1024] {
        let font = texts.gdi(FontId::Default, w).unwrap();
        let d = font.dbcs.as_ref().unwrap();
        let wi = w as i32;
        let z = DIALOG_TEXT;
        let (avail, limit) = ((wi - 1) - z.mr - z.ml, wi - 1 - z.mr);
        for l in &dial.lines {
            let md5 = format!("{:x}", md5::Md5::digest(&l.text));
            let mut got = Vec::new();
            for p in l.text.split(|&c| c == b'\n') {
                let adv = d.advances(p);
                assert_eq!(adv.len(), chars(p, d.code_page).len() - 1);
                let (lines, cuts) =
                    big5_lines_cut(p, &adv, font.ave_char_width, avail, limit, z.ml, b' ');
                assert_eq!(cuts, 0, "{} {} at {w}", l.dialog, l.line);
                got.extend(lines.iter().map(|(s, e)| format!("{s}-{e}")));
            }
            assert_eq!(
                Some(&got.join(" ")),
                golden.get(&(md5, wi)),
                "{} {} at {w}",
                l.dialog,
                l.line
            );
            // The layout draws as many lines.
            let drawn = layout(font, &l.text, wi, 600, z);
            assert_eq!(drawn.len(), got.len());
            multi += usize::from(got.len() > 1);
            checked += 1;
        }
    }
    assert_eq!(checked, 3 * 619);
    assert!(multi > 50, "{multi}");
}

/// Every character of the texts (but `i_creditold.txt`) has a glyph in
/// the bundled face; a dialog line draws pixels.
#[test]
fn glyphs_of_every_character() {
    let Some((install, discs)) = cd_zh_tw() else {
        return;
    };
    let f = files(&install, &discs);
    let texts = Texts::load(&f, "gbr");
    let font = texts.gdi(FontId::Default, 800).unwrap();
    let d = font.dbcs.as_ref().unwrap();
    let mut missing = Vec::new();
    let mut n = 0;
    for name in f.list("loc/text/gbr") {
        if name.eq_ignore_ascii_case("i_creditold.txt") {
            continue;
        }
        let t = f.read(&format!("loc/text/gbr/{name}")).unwrap();
        let st = chars(&t, d.code_page);
        for w in st.windows(2) {
            let c = d.char_of(&t[w[0]..w[1]]);
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
    let text = &dial.lines.iter().max_by_key(|l| l.text.len()).unwrap().text;
    let lines = layout(font, text, 800, 600, DIALOG_TEXT);
    assert!(lines.len() >= 2);
    let ttf = f.read("font/atlantis.TTF").unwrap();
    let g = Glyphs::new(ttf).unwrap();
    let mut px = vec![0u16; 800 * 600];
    g.draw(font, &lines, 0xff_ffff, &mut px, 800, 600);
    let lit = px.iter().filter(|&&p| p != 0).count();
    assert!(lit > 1000, "{lit}");
    let (x0, y0, x1, y1) = g.bounds(font, &lines).unwrap();
    assert!(
        x0 >= 0 && x1 <= 800 && y0 > 400 && y1 <= 600,
        "{x0} {y0} {x1} {y1}"
    );
}

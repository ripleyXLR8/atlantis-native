//! The German retail CD (atlantis-cd-de-687104, the French August 1997
//! build's code): its subtitle breaker `FUN_0042e84c`, checked against the
//! reference's `FUN_0042ffe8` by running both unmodified under emulation on
//! every dialog line of the German CON files; its menu font advances; and
//! the LOAD list's `ß` (0xe1), which no sprite font has a glyph for. The
//! expected values are numbers only.
//!
//! Needs the game folder of that build: set `ATLANTIS_DATA_DE1997`;
//! without it the tests are skipped.

use std::sync::Arc;

use atlantis_data::saves::epi_episode_name;
use atlantis_data::sprite::{limits, read_spr_file};
use atlantis_data::{FileSource, Game, StdDir};
use atlantis_engine::subtitles::split_subtitle;
use atlantis_engine::ui::layout::{
    font_min_blits, font_min_load_list_width, spell_sharp_s, FontMin, FontMinAdvances,
    FONTMAX_WIDTHS, FONTMIN_ADVANCE, SHARP_S,
};

const VAR: &str = "ATLANTIS_DATA_DE1997";

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(VAR) else {
        eprintln!("{VAR} not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// Every dialog line of the CON files through the breaker: rows per line
/// and the longest row, as the emulation of `FUN_0042e84c` counts them.
#[test]
fn german_con_lines_match_original() {
    let Some(g) = game("german_con_lines_match_original") else {
        return;
    };
    assert!(!g.exe.build.bitmap_subtitles);
    let mut names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    let (mut lines, mut hist, mut longest, mut high) = (0, [0usize; 9], 0, 0);
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
            let text = rest.strip_prefix(b" ").unwrap_or(rest);
            high += text.iter().filter(|&&c| c >= 0x80).count();
            let rows = split_subtitle(text);
            lines += 1;
            hist[rows.len().min(8)] += 1;
            for r in &rows {
                longest = longest.max(r.len());
            }
        }
    }
    assert_eq!(lines, 2542);
    assert_eq!(hist, [0, 2113, 381, 42, 5, 1, 0, 0, 0]);
    assert_eq!(longest, 100);
    assert_eq!(high, 2519);
}

/// The menu font advances of this build are the ones the port carries:
/// FONTMAX at 0x004865e6, FONTMIN at 0x0048666f (file offsets 0x71fe6 and
/// 0x7206f: DGROUP at 0x00480000, raw 0x6ba00).
#[test]
fn menu_font_advances() {
    let Some(dir) = std::env::var_os(VAR) else {
        eprintln!("{VAR} not set: menu_font_advances skipped");
        return;
    };
    let exe = StdDir::new(dir).read("atlantis.exe").unwrap();
    assert_eq!(exe[0x71fe6..0x71fe6 + FONTMAX_WIDTHS.len()], FONTMAX_WIDTHS);
    assert_eq!(
        exe[0x7206f..0x7206f + FONTMIN_ADVANCE.len()],
        FONTMIN_ADVANCE
    );
}

/// The three EPI.TXT names with `ß`: in the original glyph 193, past
/// FONTMIN's 137 sprites (not drawn), the pen moved 7 (`FUN_00410430`);
/// with the port's fix, two 's' glyphs, both drawn, and the title centred
/// with their width.
#[test]
fn load_list_sharp_s_both_ways() {
    let Some(g) = game("load_list_sharp_s_both_ways") else {
        return;
    };
    let epi = g.big.read("SPRLIST\\EPI.TXT").unwrap();
    let d = limits::DEFAULT;
    let fontmin = read_spr_file(&g.big.read("SPRITE\\2D\\FONTMIN.SPR").unwrap(), d.0, d.1).unwrap();
    assert_eq!(fontmin.len(), 137);
    let s_glyph = usize::from(b's' - 0x20);
    assert!(!fontmin[s_glyph].pixels.is_empty());
    let adv = FontMinAdvances::new(&g.exe.credit_widths);
    let named: Vec<Vec<u8>> = (2..200)
        .map(|c| epi_episode_name(&epi, c))
        .filter(|n| n.contains(&SHARP_S))
        .collect();
    assert_eq!(named.len(), 3);
    for name in &named {
        // The original's bytes.
        let raw = font_min_blits(FontMin::LoadList, name, 55, 130, &adv);
        let at = raw.iter().position(|b| b.idx == SHARP_S - 0x20).unwrap();
        assert!(fontmin.get(usize::from(raw[at].idx)).is_none());
        assert_eq!(raw[at + 1].x - raw[at].x, 7);
        let w_raw = font_min_load_list_width(name, &adv);
        // The fix.
        let fixed = spell_sharp_s(name, true);
        assert!(!fixed.contains(&SHARP_S));
        assert_eq!(fixed.len(), name.len() + 1);
        let blits = font_min_blits(FontMin::LoadList, &fixed, 55, 130, &adv);
        assert_eq!(blits.len(), raw.len() + 1);
        assert_eq!(blits[at].idx, b's' - 0x20);
        assert_eq!(blits[at + 1].idx, b's' - 0x20);
        assert!(blits
            .iter()
            .all(|b| fontmin.get(usize::from(b.idx)).is_some()));
        let s = adv.get(b's' - 0x20);
        assert_eq!(font_min_load_list_width(&fixed, &adv), w_raw - 7 + 2 * s);
        // Off, the bytes as they are.
        assert_eq!(&*spell_sharp_s(name, false), name.as_slice());
    }
    let fixed: Vec<String> = named
        .iter()
        .map(|n| {
            spell_sharp_s(n, true)
                .iter()
                .map(|&c| match c {
                    0x81 => 'ü',
                    0x84 => 'ä',
                    0x94 => 'ö',
                    c => char::from(c),
                })
                .collect()
        })
        .collect();
    assert!(fixed.iter().filter(|n| n.contains("verlässt")).count() == 2);
    assert!(fixed.iter().any(|n| n.contains("Ausserhalb")));
}

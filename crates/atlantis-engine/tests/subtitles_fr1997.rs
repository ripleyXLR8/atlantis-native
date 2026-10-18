//! The subtitles of the French retail CD's August 1997 build
//! (atlantis-cd-fr-687104): its breaker `FUN_0042e84c` is the reference's
//! `FUN_0042ffe8` (a 100-byte window), checked by running both unmodified
//! under emulation on every dialog line of the French CON files; the port's
//! breaker must give the same statistics. The expected values are numbers
//! only.
//!
//! Needs the game folder of that build: set `ATLANTIS_DATA_FR1997`;
//! without it the test is skipped.

use std::sync::Arc;

use atlantis_data::{Game, StdDir};
use atlantis_engine::subtitles::split_subtitle;

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_FR1997") else {
        eprintln!("ATLANTIS_DATA_FR1997 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// Every dialog line of the CON files through the breaker: rows per line
/// and the longest row, as the emulation of `FUN_0042e84c` counts them.
#[test]
fn french_con_lines_match_original() {
    let Some(g) = game("french_con_lines_match_original") else {
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
    assert_eq!(hist, [0, 2191, 314, 30, 6, 1, 0, 0, 0]);
    assert_eq!(longest, 100);
    assert_eq!(high, 3303);
}

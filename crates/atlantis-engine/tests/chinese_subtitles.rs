//! The subtitle breakers of the Chinese CDs (atlantis-cd-zh-tw-696832,
//! atlantis-cd-zh-cn-692736, `FUN_0042f424`) on every dialog line of their
//! CON files, against the rows of the original functions run unmodified
//! under emulation on the same lines. The expected values are numbers
//! only: the rows per line, the rows that do not decode, the MD5 of every
//! row's place in its line's buffer (`start+length`, the rows of a line
//! joined by `,`, the lines by a newline, in the order of the sorted CON
//! names), and the emulation's own MD5 of the rows (the same with each
//! row's bytes in lower-case hex, `FAULT` for a line whose scan does not
//! return). The lines the CON parser hands to the dialog presenter, final
//! blanks included, are checked to be lines of that scan.
//!
//! Needs the folders of the discs' images: set `ATLANTIS_ISO_ZH_TW` (the
//! Traditional CD's ISO images) and `ATLANTIS_BIN_ZH_CN` (the Simplified
//! CD's raw images); without them the tests are skipped.

use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::Game;
use atlantis_engine::con_script::script::{latin1_bytes, ConText};
use atlantis_engine::con_script::{ConItem, ConScript};
use atlantis_engine::subtitles::{cjk_subtitle_rows, CjkSplit};
use md5::{Digest, Md5};

fn game(var: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(var) else {
        eprintln!("{var} not set: skipped");
        return None;
    };
    let set = DiscSet::open_folder(dir.as_ref()).expect("CD images");
    Some(Game::open(Arc::new(set)).expect("the game from its images"))
}

/// The dialog lines of the CON files (the text after `]` and its space),
/// with their file's name.
fn dialog_lines(g: &Game) -> Vec<(String, Vec<u8>)> {
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
            out.push((n.clone(), rest.strip_prefix(b" ").unwrap_or(rest).to_vec()));
        }
    }
    out
}

/// What the rows of every line add up to.
#[derive(Debug, PartialEq, Eq)]
struct Rows {
    lines: usize,
    /// Lines of 1..=5 rows (none has more).
    per_line: [usize; 6],
    longest: usize,
    /// Rows that do not decode in the code page (they start or end inside
    /// a two-byte character), the lines that have one, and the rows of a
    /// single byte from 0x80.
    broken: (usize, usize, usize),
    md5: String,
    /// The MD5 of the rows' bytes, as the emulation writes it.
    hex_md5: String,
}

/// The rows of every line; `runaway` names the line (file, length) whose
/// first window has no mark, which the original's emulation cannot run
/// (written `FAULT` in the MD5's text).
fn rows(g: &Game, split: CjkSplit, runaway: Option<(&str, usize)>) -> Rows {
    let lines = dialog_lines(g);
    let mut r = Rows {
        lines: lines.len(),
        per_line: [0; 6],
        longest: 0,
        broken: (0, 0, 0),
        md5: String::new(),
        hex_md5: String::new(),
    };
    let mut places = Vec::new();
    let mut hex = Vec::new();
    for (name, text) in &lines {
        let rows = cjk_subtitle_rows(text, split);
        r.per_line[rows.len().min(5)] += 1;
        let buf = [b" ".as_slice(), text].concat();
        let mut broken = false;
        for x in &rows {
            r.longest = r.longest.max(x.len());
            let row = &buf[x.clone()];
            if split.code_page.decode(row).contains('\0') {
                r.broken.0 += 1;
                broken = true;
                r.broken.2 += usize::from(row.len() == 1 && row[0] >= 0x80);
            }
        }
        r.broken.1 += usize::from(broken);
        if runaway == Some((name.as_str(), text.len())) {
            places.push("FAULT".to_string());
            hex.push("FAULT".to_string());
        } else {
            let h: Vec<String> = rows
                .iter()
                .map(|x| buf[x.clone()].iter().map(|c| format!("{c:02x}")).collect())
                .collect();
            hex.push(h.join(","));
            let p: Vec<String> = rows
                .iter()
                .map(|x| format!("{}+{}", x.start, x.len()))
                .collect();
            places.push(p.join(","));
        }
    }
    r.md5 = format!("{:x}", Md5::digest(places.join("\n").as_bytes()));
    r.hex_md5 = format!("{:x}", Md5::digest(hex.join("\n").as_bytes()));
    r
}

/// Whether every row of `rows` starts and ends at a character boundary
/// of its line's buffer (a space, then `text`), counted from the text's
/// start.
fn in_step(text: &[u8], rows: &[std::ops::Range<usize>], split: CjkSplit) -> bool {
    let mut starts = vec![false; text.len() + 2];
    starts[0] = true;
    let mut i = 1;
    while i <= text.len() {
        starts[i] = true;
        i += 1 + usize::from(split.code_page.is_lead_byte(text[i - 1]));
    }
    starts[text.len() + 1] = i == text.len() + 1;
    rows.iter().all(|x| starts[x.start] && starts[x.end])
}

/// The game's path: every dialog line the CON parser hands to the
/// presenter, its text and final blanks (`ConLine::trail`) put together as
/// the dialog code copies them, is one of the lines of [`dialog_lines`],
/// so its rows are those checked against the original. Returns the lines
/// the sections hold and those of them with final blanks (each line
/// counted once).
fn parsed_lines(g: &Game) -> (usize, usize) {
    let mut by_file: std::collections::HashMap<String, Vec<Vec<u8>>> = Default::default();
    for (n, t) in dialog_lines(g) {
        by_file.entry(n).or_default().push(t);
    }
    let (mut lines, mut blanks) = (0, 0);
    for (name, texts) in &by_file {
        let script = ConScript::parse(&g.big.read(name).unwrap());
        let t = ConText(script.text());
        let mut seen = std::collections::HashSet::new();
        let sections = (0..t.len())
            .filter(|&p| t.is_header(p))
            .filter_map(|p| script.goto_section(t.atoi(p + 1)))
            .chain(script.sections().iter().cloned());
        for sec in sections {
            for item in &sec.items {
                let ConItem::Line(l) = item else { continue };
                if l.text.is_empty() {
                    assert!(l.trail.is_empty());
                    continue;
                }
                let copied = latin1_bytes(&(l.text.clone() + &l.trail));
                assert!(texts.contains(&copied), "{name}: {l:?}");
                if seen.insert(copied) {
                    lines += 1;
                    blanks += usize::from(!l.trail.is_empty());
                }
            }
        }
    }
    (lines, blanks)
}

/// The Traditional CD: every row as the original's; the one line with no
/// mark in its first 75 bytes (CHAPI084, 80 bytes) cut by the port at 75.
#[test]
fn traditional_rows_match_original() {
    let Some(g) = game("ATLANTIS_ISO_ZH_TW") else {
        return;
    };
    assert_eq!(g.exe.build.name, "atlantis-cd-zh-tw-696832");
    let runaway = ("SCENAR\\CHAPI084.CON", 80);
    let lines = dialog_lines(&g);
    let at = lines
        .iter()
        .position(|(n, t)| (n.as_str(), t.len()) == runaway)
        .unwrap();
    for fixes in [false, true] {
        let split = CjkSplit::of_build(g.exe.build, fixes).unwrap();
        let r = rows(&g, split, Some(runaway));
        assert_eq!(
            r,
            Rows {
                lines: 2542,
                // The emulation's {1: 2156, 2: 339, 3: 40, 4: 4, 5: 2} and
                // the runaway line in 2.
                per_line: [0, 2156, 340, 40, 4, 2],
                longest: 75,
                broken: (0, 0, 0),
                md5: "354c6b87cf663ffb05f2f07e201b7a49".into(),
                hex_md5: "6b0450476c4cfad5af20ef9a745d01ca".into(),
            }
        );
        assert_eq!(cjk_subtitle_rows(&lines[at].1, split), [1..76, 76..81]);
    }
    assert_eq!(parsed_lines(&g), (2241, 1944));
}

/// The Simplified CD: every row as the original's, the rows started on an
/// ideographic space's second byte included (72 rows of 50 lines out of
/// step). With the rows' fix (BugFix::ChineseRows), every row as the original's with the
/// skip after an ideographic space patched to 2 (@ 0x0042f5ef) and its
/// marks read at character boundaries only (the pairs of the scan @
/// 0x0042f510 and of the checks @ 0x0042f599 cleared there under
/// emulation): every row decodes, and only the 50 lines the original
/// breaks change.
#[test]
fn simplified_rows_match_original() {
    let Some(g) = game("ATLANTIS_BIN_ZH_CN") else {
        return;
    };
    assert_eq!(g.exe.build.name, "atlantis-cd-zh-cn-692736");
    let per_line = [0, 2153, 343, 40, 4, 2];
    let original = CjkSplit::of_build(g.exe.build, false).unwrap();
    assert_eq!(
        rows(&g, original, None),
        Rows {
            lines: 2542,
            per_line,
            longest: 75,
            broken: (72, 50, 19),
            md5: "e40532b39cffb491b359a1d4fa826d79".into(),
            hex_md5: "9067901aad1f1e113fbe26ca512023b4".into(),
        }
    );
    let fixed = CjkSplit::of_build(g.exe.build, true).unwrap();
    let r = rows(&g, fixed, None);
    assert_eq!(
        (r.lines, r.longest, r.broken, r.hex_md5.as_str()),
        (2542, 75, (0, 0, 0), "77504624d3bfa7563aa7acc7a5d0ba32")
    );
    // Line by line: the rows of every line in step in the original are
    // kept; the 50 others change, all in step.
    let (mut kept, mut changed) = (0, 0);
    for (_, text) in dialog_lines(&g) {
        let (o, f) = (
            cjk_subtitle_rows(&text, original),
            cjk_subtitle_rows(&text, fixed),
        );
        assert!(in_step(&text, &f, fixed));
        if in_step(&text, &o, original) {
            assert_eq!(o, f);
            kept += 1;
        } else {
            assert_ne!(o, f);
            changed += 1;
        }
    }
    assert_eq!((kept, changed), (2492, 50));
    assert_eq!(parsed_lines(&g), (2258, 100));
}

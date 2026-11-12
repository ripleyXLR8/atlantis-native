//! `TeFont2`'s kerning map against the original's: the golden
//! `tests/golden/a4_text_kerning.txt` (atlantis4-re's
//! `tools/gui/kern_golden.py`: the executable's own `std::map`
//! `operator[]` and `find` run under emulation on each `.tef`) gives per
//! font the SHA-256 of the dx table (`i8`, `[c1 × 256 + c2]`, all 65,536
//! keys), its counts and 30 lookups. Then a French topic (`27-1`) wraps in
//! its 389-px box as the original's three lines.
//!
//! The game-data tests need `ATLANTIS4_DATA` (see `atlantis4_data::testing`)
//! and skip cleanly with `ATLANTIS4_SKIP_DATA_TESTS=1`. No game text is
//! written here: lines are named by their ids.

use atlantis4_data::texts::{texts_path, DialogTable};
use atlantis4_data::xml::to_cp1252;
use atlantis4_data::A4Files;
use atlantis4_engine::gui::text::{build, parse_markup, Font, Frame};
use sha2::{Digest, Sha256};

fn files() -> Option<A4Files> {
    let dir = atlantis4_data::testing::install_dir()?;
    Some(A4Files::open_dir(&dir).unwrap().0)
}

fn golden() -> String {
    let path = format!(
        "{}/tests/golden/a4_text_kerning.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap()
}

/// The value after `name` in a line of `name value` words.
fn field<'a>(words: &[&'a str], name: &str) -> &'a str {
    let i = words.iter().position(|w| *w == name).unwrap();
    words[i + 1]
}

fn font(files: &A4Files, name: &str) -> Font {
    let data = files.read(&format!("Common/Fonts/{name}")).unwrap();
    Font::parse(&data).unwrap()
}

#[test]
fn the_kerning_tables_are_the_originals() {
    let Some(files) = files() else { return };
    let g = golden();
    let mut lines = g.lines().filter(|l| !l.starts_with('#'));
    let mut fonts = 0;
    while let Some(l) = lines.next() {
        let words: Vec<&str> = l.split_whitespace().collect();
        let name = words[0];
        let f = font(&files, name);
        let mut table = Vec::with_capacity(65536);
        let (mut nonzero, mut dy_nonzero) = (0, 0);
        let mut printable = [0usize; 4];
        for c1 in 0..256u32 {
            for c2 in 0..256u32 {
                let (dx, dy) = f.kern(c1, c2);
                let dx = dx as i8;
                table.push(dx as u8);
                nonzero += usize::from(dx != 0);
                dy_nonzero += usize::from(dy != 0.0);
                if c1 >= 32 && c2 >= 32 {
                    if let Some(n) = printable.get_mut((i32::from(dx) + 2) as usize) {
                        *n += 1;
                    }
                }
            }
        }
        let first = (0..256).filter(|&c| table[c] != 0).count();
        let sha: String = Sha256::digest(&table)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(sha, field(&words, "sha256"), "{name}");
        assert_eq!(nonzero.to_string(), field(&words, "nonzero"), "{name}");
        assert_eq!(
            dy_nonzero.to_string(),
            field(&words, "dy_nonzero"),
            "{name}"
        );
        assert_eq!(first.to_string(), field(&words, "first_nonzero"), "{name}");
        let p = format!(
            "-2:{}/-1:{}/0:{}/1:{}",
            printable[0], printable[1], printable[2], printable[3]
        );
        assert_eq!(p, field(&words, "printable"), "{name}");
        // The 30 lookups.
        let samples = lines.next().unwrap();
        let mut n = 0;
        for s in samples.split_whitespace().skip(2) {
            let (keys, dx) = s.split_once(':').unwrap();
            let (c1, c2) = keys.split_once(',').unwrap();
            let got = f.kern(c1.parse().unwrap(), c2.parse().unwrap()).0 as i32;
            assert_eq!(got, dx.parse::<i32>().unwrap(), "{name} {keys}");
            n += 1;
        }
        assert_eq!(n, 30, "{name}");
        fonts += 1;
    }
    assert_eq!(fonts, 6);
}

/// Topic `27-1` in French, Colaborate 24 as the topics draw it.
fn topic(files: &A4Files) -> (Font, Vec<u8>) {
    let table = DialogTable::parse(&files.read(&texts_path("fr")).unwrap()).unwrap();
    let text = to_cp1252(&table.get("27-1").unwrap().text);
    (font(files, "Colaborate-Regular_r_24.tef"), text)
}

fn frame(width: i32) -> Frame {
    Frame {
        width,
        single_line: false,
        inter_line: 0.0,
    }
}

#[test]
fn topic_27_1_wraps_in_three_lines_in_its_389_px_box() {
    let Some(files) = files() else { return };
    let (font, text) = topic(&files);
    let b = build(&font, parse_markup(&text), frame(389));
    // Each line's width over its own pairs: the builder's `L` less the
    // kerning of its first byte with the byte before it (0 at offset 0).
    let own: Vec<f32> = b
        .lines
        .iter()
        .map(|l| {
            let prev = if l.start > 0 { text[l.start - 1] } else { 0 };
            l.width - font.kern(u32::from(prev), u32::from(text[l.start])).0
        })
        .collect();
    assert_eq!(own, [368.0, 388.0, 379.0], "{:?}", b.lines);
}

#[test]
fn the_builder_kerns_the_space_before_the_second_line() {
    let Some(files) = files() else { return };
    let (font, text) = topic(&files);
    let b = build(&font, parse_markup(&text), frame(389));
    // The builder kerns the second line's first byte with the space
    // before it, so its width counts 41 kerned pairs: the golden's 387.
    assert_eq!(b.lines.len(), 3);
    assert_eq!(b.lines[1].width, 387.0);
}

//! The subtitles' and menus' glyphs against GDI's (review M17;
//! `tests/golden/gdi_glyphs.txt`, measured on Windows 98 SE for 1.00's
//! sizes and on Windows 11 for 1.01's: the md5 of each byte's lit pixels).
//! The port runs the font's hinting program and lights the pixels whose
//! centre is inside the hinted outline, with a dropout rule of its own
//! (`subtitle.rs`); GDI's dropout and stub rules are not all known, and
//! Windows 98 and 11 themselves differ on about one glyph in twenty, so
//! the test holds the share of glyphs drawn exactly as GDI draws them
//! (each at least the counts below; unhinted outlines filled at half
//! coverage, the port's earlier drawing, gave 13 to 34 of the 223).
//!
//! Needs the French CD installation (`ATLANTIS2_DATA`) and, for GOG's
//! font file, `ATLANTIS2_GOG`; without them the cases are skipped.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::PathBuf;

use atlantis2_data::dialog::{FontEntry, FontTxt};
use atlantis2_data::exe::Release;
use atlantis2_engine::subtitle::{SubtitleFont, TextOut};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/gdi_glyphs.txt");

/// The fewest glyphs of 223 drawn exactly as GDI draws them, per font file
/// and `CreateFontA` height (heights below 14 get 7 pixels per em).
const AT_LEAST: [(&str, i32, usize); 14] = [
    ("CD", -16, 164),
    ("CD", -13, 178),
    ("CD", 22, 156),
    ("CD", 20, 172),
    ("CD", 16, 166),
    ("CD", 12, 168),
    ("CD", 10, 168),
    ("GOG", -16, 162),
    ("GOG", -13, 176),
    ("GOG", 22, 156),
    ("GOG", 20, 172),
    ("GOG", 16, 166),
    ("GOG", 12, 168),
    ("GOG", 10, 168),
];

fn md5(s: &str) -> String {
    Md5::digest(s.as_bytes())
        .iter()
        .fold(String::new(), |mut o, b| {
            let _ = write!(o, "{b:02x}");
            o
        })
}

/// Byte `b` drawn alone at (32, 32) in the golden file's text form.
fn glyph_text(f: &SubtitleFont, b: u8) -> String {
    const SZ: i32 = 96;
    let mut px = vec![false; (SZ * SZ) as usize];
    let call = TextOut {
        x: 32,
        y: 32,
        colour: 0xff_ffff,
        bytes: vec![b],
    };
    f.draw_with(&[call], |x, y, _| {
        if (0..SZ).contains(&x) && (0..SZ).contains(&y) {
            px[(y * SZ + x) as usize] = true;
        }
    });
    let (mut x0, mut y0, mut x1, mut y1) = (SZ, SZ, -1, -1);
    for y in 0..SZ {
        for x in 0..SZ {
            if px[(y * SZ + x) as usize] {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if x1 < 0 {
        return "empty".into();
    }
    let mut s = format!("{} {} {} {} ", x0 - 32, y0 - 32, x1 - x0 + 1, y1 - y0 + 1);
    for y in y0..=y1 {
        if y > y0 {
            s.push('.');
        }
        let mut x = x0;
        while x <= x1 {
            let mut d = 0;
            for k in 0..4 {
                if x + k <= x1 && px[(y * SZ + x + k) as usize] {
                    d |= 8 >> k;
                }
            }
            let _ = write!(s, "{d:x}");
            x += 4;
        }
    }
    s
}

#[test]
fn glyphs_as_gdi_draws_them() {
    let mut fonts: HashMap<&str, Vec<u8>> = HashMap::new();
    let read = |var: &str| -> Option<Vec<u8>> {
        std::fs::read(PathBuf::from(std::env::var_os(var)?).join("loc/Atl_Font.ttf")).ok()
    };
    if let Some(t) = read("ATLANTIS2_DATA") {
        fonts.insert("CD", t);
    }
    if let Some(t) = read("ATLANTIS2_GOG") {
        fonts.insert("GOG", t);
    }
    if fonts.is_empty() {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_GOG not set: test skipped");
        return;
    }
    // The golden's blocks: (font, height) -> code -> md5.
    let mut want: HashMap<(String, i32), (String, HashMap<u8, String>)> = HashMap::new();
    let mut cur = None;
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let p: Vec<&str> = l.split(' ').collect();
        if p[0] == "f" {
            let k = (p[1].to_string(), p[4].parse().unwrap());
            want.insert(k.clone(), (p[2].to_string(), HashMap::new()));
            cur = Some(k);
        } else {
            let code = u8::from_str_radix(p[1], 16).unwrap();
            let k = cur.as_ref().unwrap();
            want.get_mut(k).unwrap().1.insert(code, p[2].to_string());
        }
    }
    let mut checked = 0;
    let mut bad = Vec::new();
    for (name, height, least) in AT_LEAST {
        let Some(ttf) = fonts.get(name) else { continue };
        let (font_md5, glyphs) = &want[&(name.to_string(), height)];
        assert_eq!(&md5_bytes(ttf), font_md5, "{name}'s Atl_Font.ttf");
        // 1.00 asks points (−MulDiv(points, 96, 72)), 1.01 a cell height.
        let (release, value) = match height {
            -16 => (Release::V100, 12),
            -13 => (Release::V100, 10),
            h => (Release::V101, h),
        };
        let mut ft = FontTxt::default();
        ft.fonts.push(FontEntry {
            id: "AtlFT_Dialog".into(),
            height: value,
            ..Default::default()
        });
        let f = SubtitleFont::named_for(ttf.clone(), &ft, "AtlFT_Dialog", release).unwrap();
        let same = (0x21..=0xffu8)
            .filter(|&b| md5(&glyph_text(&f, b)) == glyphs[&b])
            .count();
        if same < least {
            bad.push(format!("{name} height {height}: {same} (at least {least})"));
        }
        checked += 1;
    }
    assert!(bad.is_empty(), "{bad:?}");
    assert!(checked >= 7);
}

fn md5_bytes(b: &[u8]) -> String {
    Md5::digest(b).iter().fold(String::new(), |mut o, x| {
        let _ = write!(o, "{x:02x}");
        o
    })
}

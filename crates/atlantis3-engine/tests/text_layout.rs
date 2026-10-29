//! The text layout against the original's own code under emulation
//! (`tests/golden/text_layout.txt`): every `Dial.txt` line as a dialog
//! line and as a cue line, the two captions and the edge cases, at 640,
//! 800 and 1024 pixels wide: each line's position, bytes (by md5) and
//! advances.
//!
//! Needs the game for the texts: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::text::{layout, to_cp1252, GdiFont, Zone};
use md5::{Digest, Md5};
use std::collections::HashMap;

const GOLDEN: &str = include_str!("golden/text_layout.txt");

fn md5(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// The font heights of `font.txt` at a screen width (`ftol(h × f)`).
fn height(font: &str, w: i32) -> i32 {
    let base = match font {
        "AtlFont_Default" => 21.0f32,
        "AtlFont_Bulle" => 19.0,
        "AtlFont_Special" => 24.0,
        "AtlFont_Titre" => 60.0,
        _ => 18.0,
    };
    let f = match w {
        640 => 0.9f32,
        1024 => 1.1f32,
        _ => 1.0f32,
    };
    (f64::from(base) * f64::from(f)) as i32
}

#[test]
fn every_text_laid_out_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut texts: HashMap<String, Vec<u8>> = HashMap::new();
    let dial = files
        .read(atlantis3_data::dialog::dial_path("fra").trim_start_matches("Datas/"))
        .expect("Dial.txt");
    for l in atlantis3_data::dialog::parse_dial(&dial).lines {
        texts.insert(md5(&l.text), l.text);
    }
    for c in ["Cine001.txt", "Cine093.txt"] {
        let d = files
            .read(atlantis3_data::cine_txt::caption_path("fra", c).trim_start_matches("Datas/"))
            .unwrap();
        let cap = atlantis3_data::cine_txt::Caption::parse(&d).unwrap();
        texts.insert(md5(&cap.text), cap.text);
    }
    let (mut w, mut h) = (800, 600);
    let mut text: Option<Vec<u8>> = None;
    let mut zone = Zone::new(0, 0, 0, 0, 0, 0);
    let mut want: Vec<String> = Vec::new();
    let mut checked = 0;
    let mut flush = |text: &Option<Vec<u8>>, zone: Zone, want: &mut Vec<String>, w: i32, h: i32| {
        if want.is_empty() {
            return;
        }
        let Some(t) = text else {
            want.clear();
            return;
        };
        let font = want[0].split(' ').nth(4).unwrap().to_string();
        let gdi = GdiFont::arial(height(&font, w)).expect("measured");
        let got: Vec<String> = layout(gdi, t, w, h, zone)
            .iter()
            .map(|l| {
                let dx = if l.dx.is_empty() {
                    "-".to_string()
                } else {
                    l.dx.iter()
                        .map(i32::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                };
                format!("{} {} {} {}", l.x, l.y, l.bytes.len(), dx) + " " + &md5(&l.bytes)
            })
            .collect();
        let want_cmp: Vec<String> = want
            .iter()
            .map(|d| {
                let f: Vec<&str> = d.split(' ').collect();
                format!("{} {} {} {} {}", f[1], f[2], f[5], f[6], f[7])
            })
            .collect();
        assert_eq!(got, want_cmp, "{w}x{h} {zone:?}");
        checked += want.len();
        want.clear();
    };
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "SCREEN" => {
                flush(&text, zone, &mut want, w, h);
                (w, h) = (f[1].parse().unwrap(), f[2].parse().unwrap());
            }
            "CASE" => {
                flush(&text, zone, &mut want, w, h);
                text = match l.split_once(" | ") {
                    Some((_, t)) => Some(t.replace("\\n", "\n").chars().map(to_cp1252).collect()),
                    None if f[1] == "VOICE" => None,
                    None => Some(texts.get(f[2]).cloned().unwrap_or_else(|| panic!("{l}"))),
                };
            }
            "LAYOUT" => {
                flush(&text, zone, &mut want, w, h);
                let n: Vec<i32> = f[1..7].iter().map(|v| v.parse().unwrap()).collect();
                zone = Zone::new(n[0], n[1], n[2], n[3], n[4], n[5]);
            }
            "DRAW" => want.push(l.to_string()),
            _ => panic!("{l}"),
        }
    }
    flush(&text, zone, &mut want, w, h);
    assert!(checked > 4800, "{checked}");
}

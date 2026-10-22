//! The place sprites (SPW) and the sprite banks (SPRCRYO) against what the
//! original code draws (`tests/golden/spw_faces.txt`, `spr_aff.txt`).
//!
//! Needs the game: `ATLANTIS2_DATA` = the installed folder,
//! `ATLANTIS2_DISCS` = a folder holding the four CD images. Without them
//! the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::spr::{SprBank, Target};
use atlantis2_data::spw::{Blend, Clip, Spw, FACE};
use atlantis2_data::{A2Files, Folder};
use md5::{Digest, Md5};

const SPW_GOLDEN: &str = include_str!("golden/spw_faces.txt");
const SPR_GOLDEN: &str = include_str!("golden/spr_aff.txt");

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn hex(d: &[u8]) -> String {
    Md5::digest(d).iter().map(|b| format!("{b:02x}")).collect()
}

fn px_md5(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    hex(&bytes)
}

/// The harnesses' background: pixel i = (i × 2654435761 mod 2³²) >> 16.
fn background(n: usize) -> Vec<u16> {
    (0..n)
        .map(|i| ((i as u32).wrapping_mul(2_654_435_761) >> 16) as u16)
        .collect()
}

/// The value of one frame: the md5 of the six face md5s.
fn frame_value(spw: &Spw, frame: i32, blend: &Blend, bg: &[u16], bg_md5: &str) -> String {
    let faces: Vec<String> = (0..6)
        .map(|face| {
            let mut buf = bg.to_vec();
            if spw.draw(frame, face, &mut buf, Clip::FACE, blend) {
                px_md5(&buf)
            } else {
                bg_md5.to_string()
            }
        })
        .collect();
    hex(faces.join(" ").as_bytes())
}

#[test]
fn spw_faces_as_the_original_draws_them() {
    let Some(files) = game() else { return };
    let blend = Blend::rgb565();
    let bg = background(FACE * FACE);
    let bg_md5 = px_md5(&bg);
    assert!(SPW_GOLDEN.contains(&format!("background md5 {bg_md5}")));
    let mut checked = 0;
    let mut all = 0;
    let mut current: Option<(String, Spw)> = None;
    for line in SPW_GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let (key, want) = line.split_once(' ').unwrap();
        let (file, frame) = key.split_once('#').unwrap();
        let (disc, path) = file.split_once(':').unwrap();
        let name = path.strip_prefix("spw/").unwrap();
        if current.as_ref().map(|(n, _)| n.as_str()) != Some(file) {
            let n: u8 = disc[2..].parse().unwrap();
            let data = files
                .read_from_disc(n, Folder::Spw, name)
                .unwrap_or_else(|| panic!("{file} not found"));
            current = Some((file.to_string(), Spw::parse(data).expect(file)));
        }
        let spw = &current.as_ref().unwrap().1;
        let got = if frame == "all" {
            all += 1;
            let values: String = (0..spw.frame_count())
                .map(|f| frame_value(spw, f, &blend, &bg, &bg_md5))
                .collect();
            hex(values.as_bytes())
        } else {
            frame_value(spw, frame.parse().unwrap(), &blend, &bg, &bg_md5)
        };
        assert_eq!(got, want, "{key}");
        checked += 1;
    }
    assert!(
        checked > 2000 && all > 40,
        "{checked} lines, {all} whole files"
    );
}

#[test]
fn spr_banks_as_the_original_draws_them() {
    let Some(files) = game() else { return };
    let (w, h) = (640usize, 480usize);
    let bg = background(w * h);
    let mut current: Option<(String, SprBank)> = None;
    let mut checked = 0;
    for line in SPR_GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let (key, want) = line.split_once(' ').unwrap();
        let (bank_name, rest) = key.split_once('#').unwrap();
        let (index, pos) = rest.split_once('@').unwrap();
        let index: usize = index.parse().unwrap();
        if current.as_ref().map(|(n, _)| n.as_str()) != Some(bank_name) {
            let path = bank_name.strip_prefix("spr/").unwrap();
            let (folder, name) = match path.strip_prefix("Menu/") {
                Some(n) => (Folder::SprMenu, n),
                None => (Folder::Spr, path),
            };
            let data = files
                .read(folder, name)
                .unwrap_or_else(|| panic!("{bank_name} not found"));
            let bank = SprBank::parse(&data).unwrap_or_else(|e| panic!("{bank_name}: {e}"));
            current = Some((bank_name.to_string(), bank));
        }
        let bank = &current.as_ref().unwrap().1;
        let mut px = bg.clone();
        if let Some(info) = bank.info(index) {
            let (xo, yo) = (i32::from(info.xoff), i32::from(info.yoff));
            let (sw, sh) = (i32::from(info.w), i32::from(info.h));
            let (x, y) = match pos {
                "tl" => (Some(xo), yo),
                "clipA" => (Some(xo - sw / 3), yo - sh / 3),
                "clipB" => (Some(xo + 640 - 2 * sw / 3), yo + 480 - 2 * sh / 3),
                "def" => (None, 0),
                p => panic!("position {p}"),
            };
            let mut t = Target::screen(&mut px, w, h);
            assert!(bank.draw(&mut t, x, y, index), "{key}");
        }
        assert_eq!(px_md5(&px), want, "{key}");
        checked += 1;
    }
    // Every row of the golden, the 0x878 bank's included.
    assert_eq!(checked, 1949);
}

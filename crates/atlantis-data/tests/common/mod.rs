//! Shared helpers of the golden tests: the golden files, md5, the game.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use atlantis_data::{Game, StdDir};
use md5::{Digest, Md5};

/// A golden file as "key value" pairs, the key being everything before the
/// last space; comment lines (#) skipped. Keys keep the file order.
pub fn golden(text: &str) -> Vec<(&str, &str)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.rsplit_once(' '))
        .map(|(k, v)| (k.trim(), v))
        .collect()
}

pub fn golden_map(text: &str) -> BTreeMap<&str, &str> {
    golden(text).into_iter().collect()
}

/// The non-comment, non-empty lines of a golden file, trimmed.
pub fn golden_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

pub fn md5_hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn md5_le16(pixels: &[u16]) -> String {
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
    md5_hex(&bytes)
}

/// The background of the original's blits: pixel i = (i * 2654435761) >> 16.
pub fn fill_background(p: &mut [u16]) {
    for (i, v) in p.iter_mut().enumerate() {
        *v = ((i as u32).wrapping_mul(2_654_435_761) >> 16) as u16;
    }
}

/// The game of `ATLANTIS_DATA`, `None` (with a message) when it is not set.
pub fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The English retail CD's game folder of the 1997 build
/// (`ATLANTIS_DATA_1997`), `None` (the test skipped) without it.
pub fn game_1997(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_1997") else {
        eprintln!("ATLANTIS_DATA_1997 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The French retail CD's game folder of the August 1997 build
/// (`ATLANTIS_DATA_FR1997`), `None` (the test skipped) without it.
pub fn game_fr1997(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_FR1997") else {
        eprintln!("ATLANTIS_DATA_FR1997 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The French retail CD's game folder of the May 1997 build (the first
/// pressing, `ATLANTIS_DATA_FR1997_MAY`), `None` (the test skipped) without
/// it.
pub fn game_fr1997_may(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_FR1997_MAY") else {
        eprintln!("ATLANTIS_DATA_FR1997_MAY not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The German retail CD's game folder (the August 1997 build,
/// `ATLANTIS_DATA_DE1997`), `None` (the test skipped) without it.
pub fn game_de1997(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_DE1997") else {
        eprintln!("ATLANTIS_DATA_DE1997 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The Dutch retail CD's game folder (the June 1997 build,
/// `ATLANTIS_DATA_NL1997`), `None` (the test skipped) without it.
pub fn game_nl1997(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_NL1997") else {
        eprintln!("ATLANTIS_DATA_NL1997 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The Swedish retail CD's game folder (the June 1997 build,
/// `ATLANTIS_DATA_SV1997`), `None` (the test skipped) without it.
pub fn game_sv1997(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_SV1997") else {
        eprintln!("ATLANTIS_DATA_SV1997 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The Spanish retail CD's game folder (the August 1997 build,
/// `ATLANTIS_DATA_ES1997`), `None` (the test skipped) without it.
pub fn game_es1997(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_ES1997") else {
        eprintln!("ATLANTIS_DATA_ES1997 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The Italian retail CD's game folder (the July 1997 build,
/// `ATLANTIS_DATA_IT1997`), `None` (the test skipped) without it.
pub fn game_it1997(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_IT1997") else {
        eprintln!("ATLANTIS_DATA_IT1997 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// A CD install with Cryo's patch 1.01 executable
/// (`ATLANTIS_DATA_PATCH101`), `None` (the test skipped) without it.
pub fn game_patch101(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_PATCH101") else {
        eprintln!("ATLANTIS_DATA_PATCH101 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// C printf's `%.9g` for a float widened to double.
pub fn g9(v: f32) -> String {
    let v = f64::from(v);
    if v == 0.0 {
        return if v.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    let sci = format!("{v:.8e}");
    let (mant, exp) = sci.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if !(-4..9).contains(&exp) {
        let mant = trim_zeros(mant);
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mant}e{sign}{:02}", exp.abs())
    } else {
        let prec = (8 - exp) as usize;
        trim_zeros(&format!("{v:.prec$}")).to_string()
    }
}

fn trim_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

#[test]
fn g9_matches_printf() {
    assert_eq!(g9(3628.24), "3628.23999");
    assert_eq!(g9(0.2), "0.200000003");
    assert_eq!(g9(1000.0), "1000");
    assert_eq!(g9(-4499.9995), "-4499.99951");
    assert_eq!(g9(1e10), "1e+10");
    assert_eq!(g9(1.5e-5), "1.49999996e-05");
}

/// The German retail CD's first pressing (the May 1997 build) from the
/// `.iso` images in the folder `ATLANTIS_DATA_DE_MAY97` names (the folder
/// may hold other copies of the discs), `None` (the test skipped) without
/// it.
pub fn game_de_may97(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_DE_MAY97") else {
        eprintln!("ATLANTIS_DATA_DE_MAY97 not set: {test} skipped");
        return None;
    };
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("the images' folder")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
        .collect();
    paths.sort();
    let images: Vec<atlantis_data::disc_image::DiscImage> = paths
        .iter()
        .filter_map(|p| atlantis_data::disc_image::DiscImage::open(p))
        .collect();
    assert_eq!(images.len(), 4, "four disc images");
    let set = atlantis_data::disc_image::DiscSet { images };
    Some(Game::open(Arc::new(set)).expect("the game from its images"))
}

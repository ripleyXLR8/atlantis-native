//! Every panorama (CYCLO\*.HNM) of the game decoded and compared with the
//! md5 of the picture the original atlantis.exe decodes (warp decoder
//! FUN_0045084c), hashed as RGB565 little-endian.
//!
//! Needs the game: set `ATLANTIS_DATA` to the game directory (atlantis.exe
//! and BIGCD1..4.BIG). Without it the test is skipped. Run it with
//! `--release`: debug builds decode slowly.

use std::collections::HashMap;
use std::sync::Arc;

use atlantis_codecs::decode_panorama;
use atlantis_data::{Game, StdDir};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/video_warp_frames.txt");

fn golden() -> HashMap<&'static str, &'static str> {
    GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split_once(' '))
        .map(|(name, md5)| (name, md5.trim()))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn panoramas_match_original() {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: panorama test skipped");
        return;
    };
    let game = Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory");
    let golden = golden();

    let mut names: Vec<&str> = game
        .big
        .names()
        .filter(|n| n.starts_with("CYCLO\\") && n.ends_with(".HNM"))
        .collect();
    names.sort_unstable();
    assert!(!names.is_empty(), "no CYCLO\\*.HNM in the archives");

    let mut bad = Vec::new();
    for name in &names {
        let key = &name["CYCLO\\".len()..];
        let data = game.big.read(name).expect("cannot read archive member");
        let got = match decode_panorama(&data) {
            Ok(p) => hex(&Md5::digest(p.to_le_bytes())),
            Err(e) => format!("error: {e}"),
        };
        match golden.get(key) {
            Some(&want) if want == got => {}
            want => bad.push(format!("{key}: got {got}, want {want:?}")),
        }
    }
    eprintln!(
        "{} panoramas, {} differ from the original",
        names.len(),
        bad.len()
    );
    assert!(bad.is_empty(), "{} differ:\n{}", bad.len(), bad.join("\n"));
}

//! Every APC sound (WAV\*.APC) decoded and compared with the frame count
//! and the md5 of the PCM the original atlantis.exe produces (header reader
//! FUN_00417dd0, decode loop FUN_004101ce), hashed as signed 16-bit
//! little-endian.
//!
//! Needs the game: set `ATLANTIS_DATA` to the game directory (atlantis.exe
//! and BIGCD1..4.BIG). Without it the test is skipped. Run it with
//! `--release`.

use std::sync::Arc;

use atlantis_codecs::decode_apc;
use atlantis_data::{Game, StdDir};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/apc_whole_game.txt");

#[test]
fn apc_match_original() {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: APC test skipped");
        return;
    };
    let game = Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory");

    let (mut files, mut bad) = (0, Vec::new());
    for line in GOLDEN.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        // NAME KIND CHANNELS frames N md5 M
        let f: Vec<&str> = line.split_whitespace().collect();
        let [name, _kind, chans, "frames", frames, "md5", want] = f[..] else {
            panic!("bad golden line {line:?}");
        };
        let frames: usize = frames.parse().expect("bad frame count");
        let channels: u16 = if chans == "stereo" { 2 } else { 1 };
        files += 1;

        let data = game
            .big
            .read(&format!("WAV\\{name}"))
            .unwrap_or_else(|| panic!("{name} missing from the archives"));
        let got = match decode_apc(&data) {
            Ok(apc) if apc.channels != channels => format!("{} channels", apc.channels),
            Ok(apc) if apc.samples.len() != frames * usize::from(channels) => {
                format!("{} samples", apc.samples.len())
            }
            Ok(apc) => {
                let bytes: Vec<u8> = apc.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
                Md5::digest(bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect()
            }
            Err(e) => format!("error: {e}"),
        };
        if got != want {
            bad.push(format!("{name}: got {got}, want {want}"));
        }
    }
    eprintln!("{files} APC files, {} differ from the original", bad.len());
    assert_eq!(files, 2468);
    assert!(bad.is_empty(), "{} differ:\n{}", bad.len(), bad.join("\n"));
}

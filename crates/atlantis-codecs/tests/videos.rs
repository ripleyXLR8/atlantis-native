//! The IX videos (UBB_VUE\*.HNM) decoded and compared with the md5 of the
//! frames the original atlantis.exe decodes (IX video decoder
//! FUN_004506ec), hashed as RGB565 little-endian.
//!
//! Needs the game: set `ATLANTIS_DATA` to the game directory (atlantis.exe
//! and BIGCD1..4.BIG). Without it the tests are skipped. Run them with
//! `--release`: debug builds decode slowly.

use std::collections::BTreeMap;
use std::sync::Arc;

use atlantis_codecs::HnmVideo;
use atlantis_data::{Game, StdDir};
use md5::{Digest, Md5};

const FRAMES: &str = include_str!("golden/video_ix_frames.txt");
const KEYFRAMES: &str = include_str!("golden/video_ix_keyframes.txt");

const DIR: &str = "UBB_VUE\\";

/// Golden lines `NAME FRAME md5`, by file then frame.
fn golden(text: &'static str) -> BTreeMap<&'static str, BTreeMap<u32, &'static str>> {
    let mut map: BTreeMap<_, BTreeMap<_, _>> = BTreeMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut f = line.split_whitespace();
        let (Some(name), Some(frame), Some(md5)) = (f.next(), f.next(), f.next()) else {
            panic!("bad golden line {line:?}");
        };
        map.entry(name)
            .or_default()
            .insert(frame.parse().expect("bad frame number"), md5);
    }
    map
}

fn md5_hex(frame: &[u16]) -> String {
    let bytes: Vec<u8> = frame.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn game() -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: video test skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The first frames of every video (and more of some) against the original.
#[test]
fn ix_frames_match_original() {
    let Some(game) = game() else { return };
    let golden = golden(FRAMES);
    let (mut frames, mut bad) = (0, Vec::new());
    for (name, want) in &golden {
        let data = game
            .big
            .read(&format!("{DIR}{name}"))
            .unwrap_or_else(|| panic!("{name} missing from the archives"));
        let mut video = HnmVideo::new(data).expect("cannot open video");
        let last = *want.keys().last().unwrap();
        for k in 0..=last {
            let got = match video.next_frame() {
                Some(Ok(frame)) => md5_hex(frame),
                Some(Err(e)) => format!("error: {e}"),
                None => "end of video".to_string(),
            };
            if let Some(&w) = want.get(&k) {
                frames += 1;
                if w != got {
                    bad.push(format!("{name} {k}: got {got}, want {w}"));
                }
            }
        }
    }
    eprintln!(
        "{} files, {frames} IX frames, {} differ from the original",
        golden.len(),
        bad.len()
    );
    assert!(frames > 5000);
    assert!(bad.is_empty(), "{} differ:\n{}", bad.len(), bad.join("\n"));
}

/// Every frame of every video decodes; every keyframe matches the
/// original's.
#[test]
fn ix_whole_videos_and_keyframes_match_original() {
    let Some(game) = game() else { return };
    let golden = golden(KEYFRAMES);
    let mut names: Vec<&str> = game
        .big
        .names()
        .filter(|n| n.starts_with(DIR) && n.ends_with(".HNM"))
        .collect();
    names.sort_unstable();
    names.dedup();
    let (mut frames, mut keyframes, mut bad) = (0u64, 0, Vec::new());
    for name in &names {
        let key = &name[DIR.len()..];
        let data = game.big.read(name).expect("cannot read archive member");
        let mut video = HnmVideo::new(data).expect("cannot open video");
        assert!(!video.is_warp(), "{key} is a warp file");
        let want = golden.get(key);
        let mut k = 0;
        while let Some(frame) = video.next_frame() {
            match frame {
                Ok(frame) => {
                    if let Some(&w) = want.and_then(|m| m.get(&k)) {
                        keyframes += 1;
                        let got = md5_hex(frame);
                        if got != w {
                            bad.push(format!("{key} {k}: got {got}, want {w}"));
                        }
                    }
                }
                Err(e) => bad.push(format!("{key} {k}: {e}")),
            }
            k += 1;
        }
        frames += u64::from(k);
        if k != video.frame_count() {
            bad.push(format!(
                "{key}: {k} frames, header says {}",
                video.frame_count()
            ));
        }
    }
    let want_keyframes: usize = golden.values().map(|m| m.len()).sum();
    eprintln!(
        "{} files, {frames} frames, {keyframes} of {want_keyframes} keyframes checked, {} problems",
        names.len(),
        bad.len()
    );
    assert_eq!(keyframes, want_keyframes, "keyframes not reached");
    assert!(
        bad.is_empty(),
        "{} problems:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The frame buffers kept from one video to the next, as the original
/// does (it never clears them): CINEM005 frames 0..3, then CINEM006 frames
/// 0..3 in the same 640x480x2 block. The md5s come from the original
/// decoder run on one pair of buffers without clearing them.
#[test]
fn ix_persistent_buffers() {
    let Some(game) = game() else { return };
    const WANT: [&str; 8] = [
        "a950f43cca589dd51a63086590f648d2",
        "1aa0981d0d9c57f1104f073b7c58964f",
        "5fdea6775495fe0cd80b5aa7ab442075",
        "d44c3013dd5acaf9d0137d1f1c762835",
        "03a30e3b414e5ca08aa40046e0598373",
        "02c124b4fce69daaa16016939d9d4cbd",
        "245ab618e95664c0edbbd3a791e849cc",
        "389e8ddb627b2f81c7b0d62278ebb3eb",
    ];
    let mut block = vec![0u16; 2 * 640 * 480];
    let mut want = WANT.iter();
    for name in ["CINEM005.HNM", "CINEM006.HNM"] {
        let data = game
            .big
            .read(&format!("{DIR}{name}"))
            .expect("missing video");
        let mut video = HnmVideo::with_buffers(data, block).expect("cannot open video");
        for k in 0..4 {
            let frame = video
                .next_frame()
                .expect("video ended")
                .expect("decode error");
            assert_eq!(md5_hex(frame), *want.next().unwrap(), "{name} frame {k}");
        }
        // No sound track: one frame every 66 ms
        assert_eq!(video.next_frame_start_ms(), 4 * 66);
        block = video.into_buffers();
    }
}

/// The overlays drawn into the decode buffers: the original's presenter
/// draws a frame's overlays into its buffer, which the decode two frames
/// later writes into and whose motion copies read it. Here a marker 0xf81f
/// over x 540..639, y 350..449 is written into the previous frame's buffer
/// after each decode from frame 1 on; the md5s come from the original
/// decoder with the same marker written into its out buffer before each
/// decode from frame 2 on.
#[test]
fn ix_overlay_write_back() {
    let Some(game) = game() else { return };
    let golden = golden(include_str!("golden/video_ix_overlay.txt"));
    assert_eq!(golden.len(), 5);
    let (mut compared, mut bad) = (0, Vec::new());
    for (name, frames) in &golden {
        let data = game
            .big
            .read(&format!("{DIR}{name}"))
            .expect("missing video");
        let mut video =
            HnmVideo::with_buffers(data, vec![0u16; 2 * 640 * 480]).expect("cannot open video");
        let (w, h) = (video.width(), video.height());
        for (&k, want) in frames {
            let frame = video
                .next_frame()
                .expect("video ended")
                .expect("decode error");
            compared += 1;
            if md5_hex(frame) != *want {
                bad.push(format!("{name} {k}"));
            }
            if k >= 1 {
                let prev = video.previous_frame_mut().expect("an IX video");
                for y in 350..450.min(h) {
                    prev[y * w + 540..y * w + 640.min(w)].fill(0xf81f);
                }
            }
        }
    }
    assert_eq!(compared, 125);
    assert!(bad.is_empty(), "{bad:?}");
}

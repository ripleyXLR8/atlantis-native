//! The HNM6 decoder on a 32-bit display against the original's own
//! decoders (`tests/golden/hnm_frames_x888.txt`): two cinematics, the first
//! place's cube and a travel video, decoded into x8r8g8b8 frames (the
//! colour writer @ 0x006a349d: each channel saturated to 0..255, the x
//! byte 0), frame by frame.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use std::collections::BTreeMap;

use atlantis3_data::A3Files;
use atlantis_codecs::{HnmVideo, SoundRule};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/hnm_frames_x888.txt");

fn md5_u32(px: &[u32]) -> String {
    let b: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn frames_as_the_original_decodes_them_for_a_32_bit_display() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut by_file: BTreeMap<&str, BTreeMap<u32, &str>> = BTreeMap::new();
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f[1] != "pcm" {
            by_file
                .entry(f[0])
                .or_default()
                .insert(f[1].parse().unwrap(), f[2]);
        }
    }
    assert_eq!(by_file.len(), 4);
    let mut frames = 0;
    for (name, want) in &by_file {
        let data = files.read(name).unwrap_or_else(|| panic!("{name}"));
        let mut v: HnmVideo<_, u32> =
            HnmVideo::open(data, Vec::new(), SoundRule::ApcHeader).unwrap();
        let last = *want.keys().max().unwrap();
        for k in 0..=last {
            v.next_frame().unwrap().unwrap();
            if let Some(w) = want.get(&k) {
                assert_eq!(md5_u32(v.frame()), *w, "{name} frame {k}");
                frames += 1;
            }
        }
    }
    assert_eq!(frames, 176);
}

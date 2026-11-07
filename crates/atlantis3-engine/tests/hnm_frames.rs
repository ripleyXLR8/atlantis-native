//! The HNM6 decoder and the cinematics' sound against the original's own
//! decoders (`tests/golden/hnm_frames.txt`): the intro, cinematics with
//! sound (one cut short), a localised one, the first place's cube and a
//! travel video, frame by frame and sound buffer by sound buffer.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use std::collections::BTreeMap;

use atlantis3_data::A3Files;
use atlantis_codecs::{HnmVideo, SoundRule};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/hnm_frames.txt");

fn md5_bytes(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn md5_u16(px: &[u16]) -> String {
    md5_bytes(&px.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<_>>())
}

fn md5_i16(px: &[i16]) -> String {
    md5_bytes(&px.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<_>>())
}

#[derive(Default)]
struct Expect {
    frames: BTreeMap<u32, &'static str>,
    pcm: BTreeMap<u32, (&'static str, usize)>,
}

#[test]
fn frames_and_sound_as_the_original_decodes_them() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut by_file: BTreeMap<&str, Expect> = BTreeMap::new();
    for line in GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split_whitespace().collect();
        let e = by_file.entry(f[0]).or_default();
        if f[1] == "pcm" {
            e.pcm
                .insert(f[2].parse().unwrap(), (f[3], f[4].parse().unwrap()));
        } else {
            e.frames.insert(f[1].parse().unwrap(), f[2]);
        }
    }
    assert_eq!(by_file.len(), 6);
    let (mut frames, mut sounds) = (0, 0);
    for (name, e) in &by_file {
        let data = files.read(name).unwrap_or_else(|| panic!("{name}"));
        let mut v = HnmVideo::with_rule(data, Vec::new(), SoundRule::ApcHeader).unwrap();
        let last = *e.frames.keys().max().unwrap();
        for k in 0..=last {
            v.next_frame().unwrap().unwrap();
            let pcm = v.take_audio();
            if let Some(want) = e.frames.get(&k) {
                assert_eq!(md5_u16(v.frame()), *want, "{name} frame {k}");
                frames += 1;
            }
            if let Some((want, n)) = e.pcm.get(&k) {
                assert_eq!(pcm.len(), *n, "{name} sound of frame {k}");
                assert_eq!(md5_i16(&pcm), *want, "{name} sound of frame {k}");
                sounds += 1;
            }
        }
    }
    assert_eq!((frames, sounds), (236, 206));
}

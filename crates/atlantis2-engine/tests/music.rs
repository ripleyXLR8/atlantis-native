//! The music's choices and files against the original
//! (`tests/golden/music.txt`): the C runtime's `rand`, the random mark,
//! the `startmusik` list choice on every list of the scripts, every music
//! file decoded from its start and from each of its `.spp` marks.
//!
//! The file rows need the game's discs: `ATLANTIS2_DATA` and
//! `ATLANTIS2_DISCS` (see `tests/warp.rs`); without them they are skipped.

use std::path::PathBuf;

use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::music;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/music.txt");

fn rows(prefix: &str) -> impl Iterator<Item = (&str, &str)> {
    GOLDEN
        .lines()
        .filter(move |l| l.starts_with(prefix))
        .map(|l| l.split_once(' ').unwrap())
}

/// The number after `key` in a dotted name (`seed12345`).
fn field(name: &str, key: &str) -> i64 {
    name.split('.')
        .find_map(|p| p.strip_prefix(key)?.parse().ok())
        .unwrap_or_else(|| panic!("{key} in {name}"))
}

#[test]
fn rand_and_random_marks() {
    let mut n = 0;
    for (name, want) in rows("rand.") {
        let mut r = Rand::new(field(name, "seed") as u32);
        let got: Vec<String> = (0..12).map(|_| r.rand().to_string()).collect();
        assert_eq!(got.join(","), want, "{name}");
        n += 1;
    }
    for (name, want) in rows("randmark.") {
        let count = field(name, "n") as i32;
        let mut r = Rand::new(field(name, "seed") as u32);
        let mut cur = 0;
        let got: Vec<String> = (0..10)
            .map(|_| {
                cur = music::random_mark(&mut r, count, cur);
                cur.to_string()
            })
            .collect();
        assert_eq!(got.join(","), want, "{name}");
        n += 1;
    }
    assert_eq!(n, 5 + 12);
}

#[test]
fn startmusik_choices() {
    let mut n = 0;
    for (name, want) in rows("select.") {
        let q1 = name.find('\'').unwrap();
        let q2 = q1 + 1 + name[q1 + 1..].find('\'').unwrap();
        let list = &name[q1 + 1..q2];
        let rest = &name[q2 + 1..];
        let mut r = Rand::new(field(rest, "seed") as u32);
        let fixed = rest.contains(".cur0.");
        let mut first = None;
        let got: Vec<String> = (0..6)
            .map(|_| {
                let cur = if fixed { 0 } else { first.unwrap_or(0) };
                let (e, count) = music::select(&mut r, list.as_bytes(), cur, 0).unwrap();
                first.get_or_insert(e.music);
                format!("{}/{}/{}/{count}", e.music, e.mark, e.loops)
            })
            .collect();
        assert_eq!(got.join(";"), want, "{name}");
        n += 1;
    }
    assert!(n >= 100, "{n}");
}

fn md5_pcm(samples: &[i16]) -> String {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn music_files_from_their_start_and_marks() {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
        return;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    // `cdN/apc/music/NAME`: the file of disc N.
    let read = |path: &str| {
        let disc: u8 = path[2..3].parse().unwrap();
        let name = path.rsplit('/').next().unwrap();
        files
            .read_from_disc(disc, Folder::ApcMusic, name)
            .unwrap_or_else(|| panic!("{path}"))
    };
    let mut checked = 0;
    for (name, want) in rows("stream.") {
        let path = name.strip_prefix("stream.").unwrap();
        let data = read(path);
        let pcm = music::decode_music(&data, None).unwrap();
        let frames = pcm.samples.len() / usize::from(pcm.channels);
        let got = format!(
            "frames={frames} stereo={} md5={}",
            u8::from(pcm.channels == 2),
            md5_pcm(&pcm.samples)
        );
        assert_eq!(got, want, "{name}");
        checked += 1;
    }
    for (name, want) in rows("spp.") {
        let (path, mark) = name
            .strip_prefix("spp.")
            .unwrap()
            .rsplit_once(".mark")
            .unwrap();
        let k: usize = mark.parse().unwrap();
        let data = read(path);
        let spp_path = format!("{}.spp", &path[..path.len() - 4]);
        let marks = music::parse_spp(&read(&spp_path)).unwrap();
        let m = &marks[k - 1];
        // 0x4000 file bytes from the mark, as the golden decodes them.
        let end = (m.offset as usize + 0x20 + 0x4000).min(data.len());
        let mut cut = data[..end].to_vec();
        cut.resize(m.offset as usize + 0x20 + 0x4000, 0);
        let mut m2 = *m;
        m2.frames_left = m.frames_left.min(0x4000);
        let pcm = music::decode_music(&cut, Some(&m2)).unwrap();
        let got = format!("offset={} md5={}", m.offset, md5_pcm(&pcm.samples));
        assert_eq!(got, want, "{name}");
        checked += 1;
    }
    assert_eq!(checked, 21 + 83);
}

//! Atlantis II's dialog videos (`hnm/dial/*.hnm`, with `AL` alpha chunks)
//! and Tibet's panoramas (`warp/ti*.hnm`) against the frames the original
//! atlantis2.exe decodes, and the alpha blend against the frames its dialog
//! video stream draws (`tests/golden/a2_dialog_videos.txt`).
//!
//! Needs the game's discs: `ATLANTIS2_DISCS` = a folder holding the four
//! CD images (or copies of the CDs). Without it the tests are skipped. Run
//! them with `--release`.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis2_data::files::disc_candidates;
use atlantis2_data::{A2Files, Folder};
use atlantis_codecs::{AlphaTables, HnmVideo};
use atlantis_data::MemFiles;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/a2_dialog_videos.txt");

fn md5_hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn frame_md5(frame: &[u16]) -> String {
    let bytes: Vec<u8> = frame.iter().flat_map(|p| p.to_le_bytes()).collect();
    md5_hex(&bytes)
}

fn discs() -> Option<A2Files> {
    let Some(dir) = std::env::var_os("ATLANTIS2_DISCS") else {
        eprintln!("ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let files = A2Files::new(
        Arc::new(MemFiles::new()),
        disc_candidates(&PathBuf::from(dir)),
    );
    assert_eq!(files.disc_numbers(), vec![1, 2, 3, 4]);
    Some(files)
}

/// A file of disc `n` by its path on the disc.
fn read(files: &A2Files, n: u8, path: &str) -> Vec<u8> {
    let (folder, name) = if let Some(name) = path.strip_prefix("datas_a2/hnm/") {
        (Folder::Hnm, name)
    } else {
        (Folder::Warp, path.strip_prefix("datas_a2/warp/").unwrap())
    };
    files
        .read_from_disc(n, folder, name)
        .unwrap_or_else(|| panic!("disc {n}: {path}"))
}

/// Every frame of every dialog video (126 of them carry `AL` chunks,
/// which the decoder now passes over) and of Tibet's panoramas, on the
/// four discs.
#[test]
fn dialog_videos_decode_as_the_original() {
    let Some(files) = discs() else { return };
    let (mut videos, mut frames, mut with_alpha, mut alpha_frames) = (0, 0, 0, 0);
    for line in GOLDEN.lines().filter(|l| l.starts_with("F ")) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let (disc, path, count, digest) = (
            f[1].parse().unwrap(),
            f[2],
            f[3].parse::<usize>().unwrap(),
            f[4],
        );
        let data = read(&files, disc, path);
        let mut v = HnmVideo::new(&data[..]).unwrap();
        let mut md5s = Vec::new();
        let mut alpha = 0;
        while let Some(frame) = v.next_frame() {
            md5s.push(frame_md5(frame.unwrap_or_else(|e| panic!("{path}: {e}"))));
            alpha += usize::from(v.alpha().is_some());
        }
        assert_eq!(md5s.len(), count, "disc {disc} {path}");
        assert_eq!(
            md5_hex(md5s.join("\n").as_bytes()),
            digest,
            "disc {disc} {path}"
        );
        videos += 1;
        frames += count;
        with_alpha += usize::from(alpha > 0);
        alpha_frames += alpha;
    }
    assert_eq!((videos, frames), (164, 5459));
    // 126 videos carry masks, 5123 frames in all.
    assert_eq!((with_alpha, alpha_frames), (126, 5123));
}

/// The harness's background under the dialog videos: r = x·31/639,
/// g = y·63/479, b = (x + y) & 31.
fn background() -> Vec<u16> {
    let (w, h) = (640u32, 480u32);
    (0..h)
        .flat_map(|y| {
            (0..w).map(move |x| {
                let (r, g, b) = (x * 31 / (w - 1), y * 63 / (h - 1), (x + y) & 31);
                ((r << 11) | (g << 5) | b) as u16
            })
        })
        .collect()
}

/// The `AL` blend (atlantis2.exe @ 0x0046b9b0 with the RGB565 tables of
/// @ 0x0046b850) on every frame of two dialog videos.
#[test]
fn alpha_blend_matches_the_original() {
    let Some(files) = discs() else { return };
    let tables = AlphaTables::new(false);
    let bg = background();
    let mut checked = 0;
    let golden: Vec<Vec<&str>> = GOLDEN
        .lines()
        .filter(|l| l.starts_with("A "))
        .map(|l| l.split_whitespace().collect())
        .collect();
    for path in [
        "datas_a2/hnm/dial/Des1c1.hnm",
        "datas_a2/hnm/dial/H1s1c1.hnm",
    ] {
        let data = read(&files, 1, path);
        let mut v = HnmVideo::new(&data[..]).unwrap();
        let mut got = Vec::new();
        while let Some(frame) = v.next_frame() {
            let frame = frame.unwrap().to_vec();
            let mut dst = bg.clone();
            let runs = v.alpha().expect("an alpha mask in every frame");
            tables.blend(&frame, &mut dst, runs, 640, 480, 640).unwrap();
            got.push(frame_md5(&dst));
        }
        for g in golden.iter().filter(|g| g[2] == path) {
            let k: usize = g[3].parse().unwrap();
            assert_eq!(got[k], g[4], "{path} frame {k}");
            checked += 1;
        }
    }
    assert_eq!(checked, 82);
}

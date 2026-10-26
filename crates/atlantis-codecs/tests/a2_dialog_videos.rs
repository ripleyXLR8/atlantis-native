//! Atlantis II's videos (every `.hnm` of the four discs: the cinematics,
//! the moves, the dialog videos with their `AL` alpha chunks, the
//! panoramas) against the frames the original atlantis2.exe decodes, and
//! the alpha blend (RGB565 and RGB555 tables, 640 and 800 pixels a row)
//! against the frames its dialog video stream draws
//! (`tests/golden/a2_dialog_videos.txt`).
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
        atlantis2_data::testing::missing("ATLANTIS2_DISCS not set");
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

/// The superchunks of an HNM6 file before the first size dword of 0, or
/// `None` when the file ends (or a size runs past it) before such a dword:
/// the player's other end test (@ 0x0040bdda) reads that dword after the
/// last superchunk.
fn superchunks(data: &[u8]) -> Option<u32> {
    let (mut pos, mut n) = (0x40usize, 0);
    while pos + 4 <= data.len() {
        let size = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) & 0xff_ffff;
        if size == 0 {
            return Some(n);
        }
        n += 1;
        pos += size as usize;
    }
    None
}

/// Review L24: a file whose last superchunk ends at the file's end, with
/// no size-0 dword after it, does not count as ending on its count.
#[test]
fn superchunks_need_the_size_0_dword() {
    let mut data = vec![0u8; 0x40];
    data.extend_from_slice(&8u32.to_le_bytes());
    data.extend_from_slice(&[1, 2, 3, 4]);
    assert_eq!(superchunks(&data), None);
    data.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(superchunks(&data), Some(1));
    // A size running past the file's end.
    data.truncate(0x40);
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&[0; 8]);
    assert_eq!(superchunks(&data), None);
}

/// One file of the golden's `F` rows decoded: its frames' md5s, the
/// frames carrying an `AL` mask.
fn decode_file(files: &A2Files, disc: u8, path: &str) -> (Vec<String>, usize) {
    let data = read(files, disc, path);
    let video = path.starts_with("datas_a2/hnm/");
    let mut v = if video {
        // The video path's two surfaces (FUN_0040b830) are allocated once
        // and never cleared: whatever a previous video left there, its
        // frames are the same (the golden's surfaces were zeroed).
        let (w, h) = (
            usize::from(u16::from_le_bytes([data[8], data[9]])),
            usize::from(u16::from_le_bytes([data[10], data[11]])),
        );
        HnmVideo::with_buffers(&data[..], vec![0xa5a5; 2 * w * h]).unwrap()
    } else {
        HnmVideo::new(&data[..]).unwrap()
    };
    if video {
        // The player's other end test (@ 0x0040bdda, the next superchunk's
        // size 0) ends every video on the frame its count does: a size-0
        // dword right after the counted superchunks.
        assert_eq!(
            superchunks(&data),
            Some(v.frame_count()),
            "disc {disc} {path}"
        );
    }
    let mut md5s = Vec::new();
    let mut alpha = 0;
    while let Some(frame) = v.next_frame() {
        md5s.push(frame_md5(frame.unwrap_or_else(|e| panic!("{path}: {e}"))));
        alpha += usize::from(v.alpha().is_some());
    }
    (md5s, alpha)
}

/// Every frame of every `.hnm` of the four discs (1086 files: the
/// cinematics, moves, dialog videos and panoramas) against the frames the
/// original decodes; 126 dialog videos carry `AL` chunks, which the
/// decoder passes over. The files are shared between a few threads.
#[test]
fn every_hnm_file_decodes_as_the_original() {
    let Some(files) = discs() else { return };
    let rows: Vec<(u8, &str, usize, &str)> = GOLDEN
        .lines()
        .filter(|l| l.starts_with("F "))
        .map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            (f[1].parse().unwrap(), f[2], f[3].parse().unwrap(), f[4])
        })
        .collect();
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let totals = std::sync::Mutex::new((0usize, 0usize, 0usize, 0usize));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(disc, path, count, digest)) = rows.get(i) else {
                    break;
                };
                let (md5s, alpha) = decode_file(&files, disc, path);
                assert_eq!(md5s.len(), count, "disc {disc} {path}");
                assert_eq!(
                    md5_hex(md5s.join("\n").as_bytes()),
                    digest,
                    "disc {disc} {path}"
                );
                let mut t = totals.lock().unwrap();
                t.0 += 1;
                t.1 += count;
                t.2 += usize::from(alpha > 0);
                t.3 += alpha;
            });
        }
    });
    let (videos, frames, with_alpha, alpha_frames) = *totals.lock().unwrap();
    assert_eq!((videos, frames), (1086, 69127));
    // 126 videos carry masks, 5123 frames in all.
    assert_eq!((with_alpha, alpha_frames), (126, 5123));
}

/// The harness's background under the dialog videos, `pitch` pixels a
/// row: r = x·31/(pitch − 1), g = y·63/479, b = (x + y) & 31.
fn background(pitch: u32) -> Vec<u16> {
    let h = 480u32;
    (0..h)
        .flat_map(|y| {
            (0..pitch).map(move |x| {
                let (r, g, b) = (x * 31 / (pitch - 1), y * 63 / (h - 1), (x + y) & 31);
                ((r << 11) | (g << 5) | b) as u16
            })
        })
        .collect()
}

/// The `AL` blend (atlantis2.exe @ 0x0046b9b0 with the tables of
/// @ 0x0046b850) on every frame of two dialog videos: the RGB565 tables
/// at the screen's pitch of 640 (`A` rows), the RGB555 ones (`A555`), and
/// both into an 800-pixel row (`A565 800`, `A555 800`).
#[test]
fn alpha_blend_matches_the_original() {
    let Some(files) = discs() else { return };
    let mut checked = 0;
    for (kind, rgb555, pitch) in [
        ("A", false, 640u32),
        ("A555 640", true, 640),
        ("A565 800", false, 800),
        ("A555 800", true, 800),
    ] {
        let tables = AlphaTables::new(rgb555);
        let bg = background(pitch);
        let golden: Vec<Vec<&str>> = GOLDEN
            .lines()
            .filter_map(|l| l.strip_prefix(&format!("{kind} ")))
            .map(|l| l.split_whitespace().collect())
            .collect();
        assert!(!golden.is_empty(), "{kind}");
        for path in [
            "datas_a2/hnm/dial/Des1c1.hnm",
            "datas_a2/hnm/dial/H1s1c1.hnm",
        ] {
            let rows: Vec<&Vec<&str>> = golden.iter().filter(|g| g[1] == path).collect();
            if rows.is_empty() {
                continue;
            }
            let data = read(&files, 1, path);
            let mut v = HnmVideo::new(&data[..]).unwrap();
            let mut got = Vec::new();
            while let Some(frame) = v.next_frame() {
                let frame = frame.unwrap().to_vec();
                let mut dst = bg.clone();
                let runs = v.alpha().expect("an alpha mask in every frame");
                tables
                    .blend(&frame, &mut dst, runs, 640, 480, pitch as usize)
                    .unwrap();
                got.push(frame_md5(&dst));
            }
            for g in rows {
                let k: usize = g[2].parse().unwrap();
                assert_eq!(got[k], g[3], "{kind} {path} frame {k}");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 82 + 82 + 41 + 41);
}

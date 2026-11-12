//! The Vorbis decoder against the game's own: per-file MD5s of the 16-bit
//! output for the 564 files decoded by the game's libvorbisfile 1.2.0
//! code in an emulator, and the whole install (5,303 files) against the
//! per-folder and aggregate hashes of a reference build equal to the game.
//!
//! Needs the game: `ATLANTIS4_DATA` = the installed folder. Run with
//! `--release`; the whole install is about 7 hours of sound.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use atlantis4_data::vorbis;
use md5::{Digest, Md5};

fn install() -> Option<PathBuf> {
    atlantis4_data::testing::install_dir()
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

const GOLDEN: &str = include_str!("golden/vorbis.txt");

/// `md5 frames channels path` of a decoded file.
fn line(root: &Path, rel: &str) -> String {
    let data = std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
    let (ch, _, pcm) = vorbis::decode(&data).unwrap_or_else(|e| panic!("{rel}: {e}"));
    let mut h = Md5::new();
    for s in &pcm {
        h.update(s.to_le_bytes());
    }
    format!("{} {} {} {}", hex(&h.finalize()), pcm.len() / ch, ch, rel)
}

/// Runs `f` over `items` on two threads, results in order.
fn par<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let half = items.len().div_ceil(2);
    std::thread::scope(|s| {
        let (a, b) = items.split_at(half.min(items.len()));
        let h = s.spawn(|| a.iter().map(&f).collect::<Vec<_>>());
        let mut rb: Vec<R> = b.iter().map(&f).collect();
        let mut ra = h.join().unwrap();
        ra.append(&mut rb);
        ra
    })
}

#[test]
fn the_samples_of_564_files_equal_the_games_decoder() {
    let Some(root) = install() else { return };
    let want: Vec<&str> = GOLDEN.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(want.len(), 564);
    let only = std::env::var("A4_VORBIS_ONLY").ok();
    let want: Vec<&str> = want
        .into_iter()
        .filter(|l| only.as_deref().is_none_or(|o| l.contains(o)))
        .collect();
    let got = par(&want, |w| line(&root, w.split(' ').nth(3).unwrap()));
    let bad: Vec<_> = want.iter().zip(&got).filter(|(w, g)| w != g).collect();
    assert!(
        bad.is_empty(),
        "{} of {} differ, first: {:?}",
        bad.len(),
        want.len(),
        bad.first()
    );
}

/// Every `.ogg` of the install, `/`-separated, sorted as byte strings.
fn all_oggs(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if p.to_string_lossy().to_ascii_lowercase().ends_with(".ogg") {
                out.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn the_whole_install_equals_the_reference_aggregate() {
    let Some(root) = install() else { return };
    let rels = all_oggs(&root);
    assert_eq!(rels.len(), 5_303);
    let lines = par(&rels, |r| line(&root, r));
    let mut all = Md5::new();
    let mut folders: BTreeMap<String, (usize, u64, Md5)> = BTreeMap::new();
    let (mut frames, mut samples) = (0u64, 0u64);
    for (l, rel) in lines.iter().zip(&rels) {
        all.update(format!("{l}\n"));
        let f: Vec<&str> = l.split(' ').collect();
        let (fr, ch): (u64, u64) = (f[1].parse().unwrap(), f[2].parse().unwrap());
        frames += fr;
        samples += fr * ch;
        let parts: Vec<&str> = rel.split('/').collect();
        let key = if parts[0] == "Videos" && parts.len() == 2 {
            "Videos/.".to_string()
        } else {
            parts[..2].join("/")
        };
        let e = folders.entry(key).or_insert_with(|| (0, 0, Md5::new()));
        e.0 += 1;
        e.1 += fr;
        e.2.update(format!("{l}\n"));
    }
    let mut want_folders = Vec::new();
    for l in GOLDEN.lines().filter(|l| l.starts_with("# folder")) {
        let f: Vec<&str> = l.split_whitespace().collect();
        want_folders.push(format!("{} {} {} {}", f[2], f[4], f[6], f[8]));
    }
    let got_folders: Vec<String> = folders
        .into_iter()
        .map(|(k, (n, fr, h))| format!("{k} {n} {fr} {}", hex(&h.finalize())))
        .collect();
    assert_eq!(got_folders, want_folders);
    let agg = GOLDEN
        .lines()
        .find(|l| l.starts_with("# aggregate"))
        .unwrap();
    assert_eq!(
        format!(
            "# aggregate files {} frames {frames} samples {samples} md5 {}",
            rels.len(),
            hex(&all.finalize())
        ),
        agg
    );
}

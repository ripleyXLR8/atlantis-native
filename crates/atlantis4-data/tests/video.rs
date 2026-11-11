//! Every Theora stream of the install decoded and compared with libtheora,
//! and the RGBA conversion compared with a reference model of the
//! original's (equal to the executable's own code run in an emulator, for
//! every triple).
//!
//! Needs the game: `ATLANTIS4_DATA` = the installed folder. Run with
//! `--release` (about 245,000 frames).

use std::path::{Path, PathBuf};

use atlantis4_data::video::{self, RgbTable, Theora, Video};
use atlantis4_data::wpa::Package;
use md5::{Digest, Md5};

fn install() -> Option<PathBuf> {
    atlantis4_data::testing::install_dir()
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The golden `key` of `tests/golden/video.txt`: its fields after the key.
fn golden(key: &str) -> Vec<String> {
    include_str!("golden/video.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|f| f[0] == key)
        .unwrap_or_else(|| panic!("no golden {key}"))[1..]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Every file of the install whose name ends with `ext`, as `/`-separated
/// names relative to the install, sorted.
fn files_with(root: &Path, ext: &str) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, ext: &str, out: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, ext, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy();
                if rel.to_ascii_lowercase().ends_with(ext) {
                    out.push(rel.replace('\\', "/"));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, ext, &mut out);
    out
}

fn table(dir: &Path) -> RgbTable {
    let exe = std::fs::read(dir.join(atlantis4_data::exe::EXE_NAME)).unwrap();
    RgbTable::from_exe(&exe).unwrap()
}

/// A stream's bytes: a file, or `file:entry` of a warp package.
fn stream(dir: &Path, name: &str) -> Vec<u8> {
    match name.split_once(".wpa:") {
        Some((file, entry)) => {
            let data = std::fs::read(dir.join(format!("{file}.wpa"))).unwrap();
            let p = Package::parse(&data).unwrap();
            let e = p.entry(entry).unwrap();
            p.slice(&data, e).unwrap().to_vec()
        }
        None => std::fs::read(dir.join(name)).unwrap(),
    }
}

/// One stream decoded: "name\tframe\tMD5 of the cropped planes\n" per
/// frame (the reference decoder's output: Y, Cb, Cr, rows top first).
fn frame_lines(name: &str, data: &[u8]) -> Vec<String> {
    let mut t = Theora::open(data).unwrap_or_else(|e| panic!("{name}: {e}"));
    let n = t.frame_count();
    let mut lines = Vec::with_capacity(n);
    let mut k = 0;
    while let Some(pic) = t.next_frame().unwrap_or_else(|e| panic!("{name}: {e}")) {
        lines.push(format!(
            "{name}\t{k}\t{}\n",
            hex(&Md5::digest(pic.packed()))
        ));
        k += 1;
    }
    assert_eq!(k, n, "{name}");
    lines
}

/// Every Theora stream (446 `.ogv` files, 1,484 package entries) equals
/// libtheora 1.2.0 frame for frame: the MD5 of the sorted
/// "stream\tframe\tmd5" lines is the reference decoder's.
#[test]
fn every_video_frame_equals_libtheora() {
    let Some(dir) = install() else { return };
    // Work by file, each package read once (only those with videos are
    // decoded, but every package is listed).
    let mut files = files_with(&dir, ".ogv");
    files.extend(files_with(&dir, ".wpa"));
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let streams = std::sync::atomic::AtomicUsize::new(0);
    let entries = std::sync::atomic::AtomicUsize::new(0);
    let mut lines: Vec<String> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    use std::sync::atomic::Ordering::Relaxed;
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Relaxed);
                        let Some(name) = files.get(i) else { break };
                        let data = std::fs::read(dir.join(name)).unwrap();
                        if !name.ends_with(".wpa") {
                            out.extend(frame_lines(name, &data));
                            streams.fetch_add(1, Relaxed);
                            continue;
                        }
                        let pkg = Package::parse(&data).unwrap();
                        for e in pkg.entries.iter().filter(|e| e.name.ends_with(".ogv")) {
                            // The reference's names are "<file>/<entry>".
                            let n = format!("{name}/{}", e.name);
                            out.extend(frame_lines(&n, pkg.slice(&data, e).unwrap()));
                            entries.fetch_add(1, Relaxed);
                        }
                    }
                    out
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect()
    });
    assert_eq!((streams.into_inner(), entries.into_inner()), (446, 1_484));
    lines.sort();
    let want = golden("ycbcr");
    assert_eq!(lines.len().to_string(), want[0]);
    assert_eq!(hex(&Md5::digest(lines.concat())), want[1]);
}

/// The table read from the executable gives the model's map of all 2^24
/// (Y, Cb, Cr) triples, and a few of its values.
#[test]
fn the_colour_table_gives_the_models_map() {
    let Some(dir) = install() else { return };
    let t = table(&dir);
    // (Y, Cb, Cr) -> (R, G, B), from the reference model; the first
    // three are the carries no formula gives.
    let v = [
        ((0, 0, 0), [0, 153, 242]),
        ((255, 255, 255), [255, 108, 1]),
        ((20, 135, 0), [0, 106, 20]),
        ((16, 128, 128), [0, 0, 0]),
        ((235, 128, 128), [255, 255, 255]),
        ((128, 128, 128), [130, 130, 130]),
        ((81, 90, 240), [255, 0, 2]),
        ((145, 54, 34), [0, 255, 5]),
        ((41, 240, 110), [0, 0, 250]),
        ((200, 16, 200), [255, 198, 0]),
        ((100, 200, 50), [0, 134, 241]),
        ((239, 0, 255), [255, 205, 8]),
    ];
    for ((y, cb, cr), rgb) in v {
        assert_eq!(t.rgb(y, cb, cr), rgb, "{y} {cb} {cr}");
    }
    let mut md5 = Md5::new();
    let mut row = Vec::with_capacity(3 * 256);
    for y in 0..=255 {
        for cb in 0..=255 {
            row.clear();
            for cr in 0..=255 {
                row.extend(t.rgb(y, cb, cr));
            }
            md5.update(&row);
        }
    }
    assert_eq!(hex(&md5.finalize()), golden("rgb-map")[0]);
}

/// A few videos to RGBA, colour and alpha paired by packet index, equal
/// to the model run on libtheora's planes.
#[test]
fn some_videos_convert_to_the_models_rgba() {
    let Some(dir) = install() else { return };
    let t = table(&dir);
    let text = include_str!("golden/video.txt");
    let mut checked = 0;
    for l in text.lines().filter(|l| l.starts_with("rgba\t")) {
        let f: Vec<&str> = l.split('\t').collect();
        let (name, size, frames, want) = (f[1], f[2], f[3], f[4]);
        let colour = stream(&dir, name);
        let alpha_name = match name.split_once(':') {
            Some((p, e)) => format!("{p}:{}", video::alpha_name(e).unwrap()),
            None => video::alpha_name(name).unwrap(),
        };
        let has_alpha = alpha_name.contains(".wpa:") || dir.join(&alpha_name).exists();
        let alpha = has_alpha.then(|| stream(&dir, &alpha_name));
        let mut v = Video::open(&colour, alpha.as_deref()).unwrap();
        let (w, h) = v.size();
        assert_eq!(format!("{w}x{h}"), size, "{name}");
        let mut out = vec![0u8; (w * h * 4) as usize];
        let mut md5 = Md5::new();
        let mut n = 0;
        while v.next_rgba(&t, &mut out).unwrap() {
            md5.update(&out);
            n += 1;
        }
        assert_eq!(n.to_string(), frames, "{name}");
        assert_eq!(hex(&md5.finalize()), want, "{name}");
        // Seeking back gives the same frame as decoding in order.
        if n > 3 {
            let mut a = vec![0u8; out.len()];
            v.seek_rgba(&t, 2, &mut a).unwrap();
            let mut b = vec![0u8; out.len()];
            v.rewind().unwrap();
            for _ in 0..3 {
                v.next_rgba(&t, &mut b).unwrap();
            }
            assert_eq!(a, b, "{name}");
        }
        checked += 1;
    }
    assert_eq!(checked, 4);
}

/// The `.rgb.jpg` + `.alpha.jpg` pairs of the packages merge: every alpha
/// picture is grey and of its colour picture's size.
#[test]
fn every_jpeg_pair_merges() {
    use atlantis4_data::image;

    let Some(dir) = install() else { return };
    let mut pairs = 0;
    for p in files_with(&dir, ".wpa") {
        let data = std::fs::read(dir.join(&p)).unwrap();
        let pkg = Package::parse(&data).unwrap();
        for e in pkg
            .entries
            .iter()
            .filter(|e| e.name.ends_with(".alpha.jpg"))
        {
            let stem = e.name.strip_suffix(".alpha.jpg").unwrap();
            let c = pkg.entry(&format!("{stem}.rgb.jpg")).unwrap();
            let rgb = image::decode_jpeg(pkg.slice(&data, c).unwrap()).unwrap();
            let a = image::decode_jpeg(pkg.slice(&data, e).unwrap()).unwrap();
            assert_eq!(a.channels, 1, "{p}:{}", e.name);
            let m = video::rgb::merge_jpeg_alpha(&rgb, &a).unwrap();
            assert_eq!(m.len(), a.pixels.len() * 4);
            pairs += 1;
        }
    }
    assert_eq!(pairs, 345);
}

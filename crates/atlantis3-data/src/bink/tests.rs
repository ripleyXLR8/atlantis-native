use std::path::{Path, PathBuf};

use md5::{Digest, Md5};

use super::*;

#[test]
fn refuses_what_is_not_bink() {
    assert_eq!(
        BinkVideo::open(&b"SMK2...."[..]).err(),
        Some(BinkError::NotBink)
    );
    assert_eq!(BinkVideo::open(&b"BI"[..]).err(), Some(BinkError::NotBink));
    let mut header = b"BIKi".to_vec();
    header.extend_from_slice(&[0; 8]);
    assert_eq!(
        BinkVideo::open(&header[..]).err(),
        Some(BinkError::BadHeader("cut short"))
    );
}

/// The CD's films (`ATLANTIS4_CD`), sorted, relative to the install.
fn cd_films() -> Option<(PathBuf, Vec<PathBuf>)> {
    let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(PathBuf::from) else {
        crate::testing::missing("ATLANTIS4_CD not set");
        return None;
    };
    let mut films = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap_or_else(|e| panic!("{}: {e}", d.display())) {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bik")) {
                films.push(p.strip_prefix(&dir).unwrap().to_path_buf());
            }
        }
    }
    films.sort();
    Some((dir, films))
}

/// The MD5 of a frame's Y, U, V and A planes as ScummVM holds them (A
/// only with an alpha plane), and of its x8r8g8b8 conversion.
fn frame_md5s(f: &Frame<'_>) -> (String, String) {
    let mut h = Md5::new();
    h.update(f.y);
    h.update(f.u);
    h.update(f.v);
    if let Some(a) = f.a {
        h.update(a);
    }
    let planes = format!("{:x}", h.finalize());
    let mut rgb = vec![0u32; f.width * f.height];
    f.to_argb8888(&mut rgb);
    let mut h = Md5::new();
    for p in &rgb {
        h.update(p.to_le_bytes());
    }
    (planes, format!("{:x}", h.finalize()))
}

/// Every film of the CD: `BIKi`, no audio, and every frame of the header's
/// count decodes.
#[test]
fn cd_films_decode() {
    let Some((dir, films)) = cd_films() else {
        return;
    };
    assert_eq!(films.len(), 300);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4));
    let decoded: usize = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut frames = 0;
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(rel) = films.get(i) else {
                            return frames;
                        };
                        let data = std::fs::read(dir.join(rel)).unwrap();
                        let mut v = BinkVideo::open(&data[..])
                            .unwrap_or_else(|e| panic!("{}: {e}", rel.display()));
                        assert_eq!(&v.id(), b"BIKi", "{}", rel.display());
                        assert_eq!(v.audio_tracks(), 0, "{}", rel.display());
                        let count = v.frame_count();
                        let mut n = 0;
                        while let Some(f) = v.next_frame() {
                            f.unwrap_or_else(|e| panic!("{} frame {n}: {e}", rel.display()));
                            n += 1;
                        }
                        assert_eq!(n, count, "{}", rel.display());
                        frames += n as usize;
                    }
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).sum()
    });
    assert_eq!(decoded, 73_863);
}

/// The reference: per-frame MD5s of the planes and of the a8r8g8b8 frame
/// that ScummVM's own `BinkDecoder` gives for one film of each size (the
/// fixture's header says how they were made).
#[test]
fn cd_films_match_scummvm() {
    let Some((dir, _)) = cd_films() else {
        return;
    };
    let fixture = include_str!("../../tests/golden/bink_scummvm.txt");
    let mut current: Option<(String, Vec<(String, String)>)> = None;
    let mut films = 0;
    let check = |name: &str, expected: &[(String, String)]| {
        let data = std::fs::read(dir.join(Path::new(name))).unwrap();
        let mut v = BinkVideo::open(&data[..]).unwrap();
        for (i, (planes, rgb)) in expected.iter().enumerate() {
            let f = v.next_frame().unwrap().unwrap();
            let (p, r) = frame_md5s(&f);
            assert_eq!((&p, &r), (planes, rgb), "{name} frame {i}");
        }
    };
    for line in fixture.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("film ") {
            if let Some((n, e)) = current.take() {
                check(&n, &e);
                films += 1;
            }
            current = Some((name.to_string(), Vec::new()));
        } else {
            let mut it = line.split_whitespace();
            let (_, p, r) = (it.next(), it.next().unwrap(), it.next().unwrap());
            current
                .as_mut()
                .unwrap()
                .1
                .push((p.to_string(), r.to_string()));
        }
    }
    if let Some((n, e)) = current.take() {
        check(&n, &e);
        films += 1;
    }
    assert!(films >= 5);
}

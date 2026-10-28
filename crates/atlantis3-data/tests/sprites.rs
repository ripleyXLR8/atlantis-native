//! Every sprite of every bank and every place-sprite face, drawn as the
//! original sprite library draws them.
//!
//! The goldens (`tests/golden/spr_aff_*.txt`, `spr_spw_*.txt`) hold, per
//! file, the md5 of the lines `KEY VALUE` the original `spr_p5.dll` /
//! `spr_p6.dll` gave under emulation: for `AFF_SPR`, the md5 of a 1024 x
//! 768 framebuffer after each sprite at each position; for `AFF_SPW`, the
//! md5 of the six 512 x 512 face buffers' md5s of frame 0 and the last
//! frame (every frame of a file whose flags are not 0x79). The target's
//! clip maxima are the size − 1; the background pixel i is
//! `(i × 2654435761 mod 2³²) >> 16` in 16 bits, `i × 2654435761 mod 2³²`
//! in 32 bits, reset before each call.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use atlantis3_data::spr::{Bank, Pixel, SprDll, Target};
use md5::{Digest, Md5};

const AFF_P6_565: &str = include_str!("golden/spr_aff_p6_565.txt");
const AFF_P6_X888: &str = include_str!("golden/spr_aff_p6_x888.txt");
const AFF_P5_565: &str = include_str!("golden/spr_aff_p5_565.txt");
const AFF_P5_X888: &str = include_str!("golden/spr_aff_p5_x888.txt");
const SPW_P6_565: &str = include_str!("golden/spr_spw_p6_565.txt");
const SPW_P6_X888: &str = include_str!("golden/spr_spw_p6_x888.txt");
const SPW_P5_565: &str = include_str!("golden/spr_spw_p5_565.txt");
const SPW_P5_X888: &str = include_str!("golden/spr_spw_p5_x888.txt");
const SPWALL_P6_565: &str = include_str!("golden/spr_spwall_p6_565.txt");
const SPWALL_P6_X888: &str = include_str!("golden/spr_spwall_p6_x888.txt");

fn datas() -> Option<PathBuf> {
    Some(atlantis3_data::testing::install_dir()?.join("Datas"))
}

fn hex(d: &[u8]) -> String {
    Md5::digest(d).iter().map(|b| format!("{b:02x}")).collect()
}

/// The golden's files: name → (line count, digest), in order.
fn digests(golden: &str) -> Vec<(String, usize, String)> {
    golden
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let mut it = l.rsplitn(3, ' ');
            let md5 = it.next().unwrap().to_string();
            let n = it.next().unwrap().parse().unwrap();
            (it.next().unwrap().to_string(), n, md5)
        })
        .collect()
}

/// A pixel type's harness background and the md5 of a buffer.
trait Harness: Pixel + Default {
    fn background(n: usize) -> Vec<Self>;
    fn md5(px: &[Self]) -> String;
}

impl Harness for u16 {
    fn background(n: usize) -> Vec<u16> {
        (0..n)
            .map(|i| ((i as u32).wrapping_mul(2_654_435_761) >> 16) as u16)
            .collect()
    }
    fn md5(px: &[u16]) -> String {
        hex(&px.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<u8>>())
    }
}

impl Harness for u32 {
    fn background(n: usize) -> Vec<u32> {
        (0..n)
            .map(|i| (i as u32).wrapping_mul(2_654_435_761))
            .collect()
    }
    fn md5(px: &[u32]) -> String {
        hex(&px.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<u8>>())
    }
}

fn read(datas: &Path, name: &str) -> Vec<u8> {
    std::fs::read(datas.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The golden lines of one bank, drawn by the port.
fn aff_lines<P: Harness>(data: &[u8], name: &str, bg: &[P], dll: SprDll) -> String {
    const W: usize = 1024;
    const H: usize = 768;
    let bank = Bank::parse(data, P::DEPTH).unwrap();
    let mut out = String::new();
    let mut buf = bg.to_vec();
    for i in 0..bank.len() {
        let Some(info) = bank.info(i) else {
            out += &format!("{name}#{i}@tl {}\n", P::md5(bg));
            continue;
        };
        let (w, h) = (i32::from(info.w), i32::from(info.h));
        let (xo, yo) = (i32::from(info.xoff), i32::from(info.yoff));
        let mut pos = vec![
            ("tl", Some(xo), yo),
            ("clipA", Some(xo - w / 3), yo - h / 3),
            (
                "clipB",
                Some(xo + W as i32 - 2 * w / 3),
                yo + H as i32 - 2 * h / 3,
            ),
        ];
        if info.default_pos.is_some() {
            pos.push(("def", None, 0));
        }
        for (tag, x, y) in pos {
            buf.copy_from_slice(bg);
            let mut t = Target::screen(&mut buf, W, H);
            assert!(
                bank.draw_with(&mut t, x, y, i, dll),
                "{name}#{i}: no blitter"
            );
            out += &format!("{name}#{i}@{tag} {}\n", P::md5(&buf));
        }
    }
    out
}

/// The golden lines of one place-sprite file, drawn by the port.
fn spw_lines<P: Harness>(data: &[u8], name: &str, bg: &[P], dll: SprDll) -> String {
    const F: usize = 512;
    let bank = Bank::parse(data, P::DEPTH).unwrap();
    assert!(bank.is_spw(), "{name}");
    let n = bank.len();
    let frames: Vec<usize> = if bank.flags != 0x79 {
        (0..n).collect()
    } else if n > 1 {
        vec![0, n - 1]
    } else {
        vec![0]
    };
    let bg_md5 = P::md5(bg);
    let mut out = String::new();
    let mut buf = bg.to_vec();
    for frame in frames {
        let faces: Vec<String> = (0..6)
            .map(|face| {
                buf.copy_from_slice(bg);
                let mut t = Target::screen(&mut buf, F, F);
                if bank.draw_face_with(&mut t, frame, face, dll) {
                    P::md5(&buf)
                } else {
                    bg_md5.clone()
                }
            })
            .collect();
        out += &format!("{name}#{frame} {}\n", hex(faces.join(" ").as_bytes()));
    }
    out
}

/// The golden lines of every frame of one place-sprite file: per face the
/// md5 of the rows the face covers, `-` for an empty face.
fn spw_all_lines<P: Harness>(data: &[u8], name: &str, bg: &[P], dll: SprDll) -> String {
    const F: usize = 512;
    let bank = Bank::parse(data, P::DEPTH).unwrap();
    let mut out = String::new();
    let mut buf = bg.to_vec();
    for frame in 0..bank.len() {
        let faces: Vec<String> = (0..6)
            .map(|face| {
                let Some(info) = bank.face_info(frame, face) else {
                    return "-".to_string();
                };
                let r0 = (-i32::from(info.yoff)).max(0) as usize;
                let r1 = (i32::from(info.h) - i32::from(info.yoff)).clamp(0, 511) as usize;
                buf.copy_from_slice(bg);
                let mut t = Target::screen(&mut buf, F, F);
                assert!(bank.draw_face_with(&mut t, frame, face, dll), "{name}");
                if r1 <= r0 {
                    P::md5(&[])
                } else {
                    P::md5(&buf[r0 * F..r1 * F])
                }
            })
            .collect();
        out += &format!("{name}#{frame} {}\n", hex(faces.join(" ").as_bytes()));
    }
    out
}

/// Checks every file of a golden; `only` limits the check to the files it
/// names. Returns the number of lines checked.
fn check<P: Harness>(
    golden: &str,
    size: usize,
    only: Option<&[&str]>,
    dll: SprDll,
    lines: fn(&[u8], &str, &[P], SprDll) -> String,
) -> usize {
    let Some(datas) = datas() else { return 0 };
    let bg = P::background(size);
    let bg_md5 = P::md5(&bg);
    assert!(
        golden.contains(&format!("background md5 {bg_md5}")),
        "background"
    );
    let mut wrong = BTreeMap::new();
    let mut checked = 0;
    for (name, n, want) in digests(golden) {
        if only.is_some_and(|o| !o.contains(&name.as_str())) {
            continue;
        }
        let got = lines(&read(&datas, &name), &name, &bg, dll);
        assert_eq!(got.lines().count(), n, "{name}: line count");
        if hex(got.as_bytes()) != want {
            wrong.insert(name, got);
        }
        checked += n;
    }
    assert!(
        wrong.is_empty(),
        "{} files differ from the original: {:?}",
        wrong.len(),
        wrong.keys().collect::<Vec<_>>()
    );
    checked
}

#[test]
fn every_sprite_as_spr_p6_draws_it_in_16_bits() {
    let n = check::<u16>(AFF_P6_565, 1024 * 768, None, SprDll::P6, aff_lines::<u16>);
    assert!(n == 0 || n == 15228, "{n}");
}

#[test]
fn every_sprite_as_spr_p6_draws_it_in_32_bits() {
    let n = check::<u32>(AFF_P6_X888, 1024 * 768, None, SprDll::P6, aff_lines::<u32>);
    assert!(n == 0 || n == 15228, "{n}");
}

#[test]
fn every_place_sprite_face_as_spr_p6_draws_it() {
    let n = check::<u16>(SPW_P6_565, 512 * 512, None, SprDll::P6, spw_lines::<u16>);
    let m = check::<u32>(SPW_P6_X888, 512 * 512, None, SprDll::P6, spw_lines::<u32>);
    assert!((n, m) == (0, 0) || (n, m) == (2843, 2843), "{n} {m}");
}

/// Every face of every frame (83,145 faces), the rows each covers.
#[test]
fn every_frame_of_every_place_sprite_as_spr_p6_draws_it() {
    let n = check::<u16>(
        SPWALL_P6_565,
        512 * 512,
        None,
        SprDll::P6,
        spw_all_lines::<u16>,
    );
    let m = check::<u32>(
        SPWALL_P6_X888,
        512 * 512,
        None,
        SprDll::P6,
        spw_all_lines::<u32>,
    );
    assert!((n, m) == (0, 0) || (n, m) == (67_723, 67_723), "{n} {m}");
}

/// `spr_p5.dll` draws as `spr_p6.dll` but for one call; the goldens of
/// both libraries are the same file for file except where listed here.
#[test]
fn spr_p5_draws_as_spr_p6() {
    let same = |a: &str, b: &str, except: &[&str]| {
        let (a, b) = (digests(a), digests(b));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.0, y.0);
            assert_eq!(x.2 == y.2, !except.contains(&x.0.as_str()), "{}", x.0);
        }
    };
    same(AFF_P5_565, AFF_P6_565, &[]);
    same(AFF_P5_X888, AFF_P6_X888, &["spr/Ecrin32.SPR"]);
    same(SPW_P5_565, SPW_P6_565, &[]);
    same(SPW_P5_X888, SPW_P6_X888, &[]);
    // The one bank where they differ, drawn with spr_p5.dll.
    let only: &[&str] = &["spr/Ecrin32.SPR"];
    let n = check::<u32>(
        AFF_P5_X888,
        1024 * 768,
        Some(only),
        SprDll::P5,
        aff_lines::<u32>,
    );
    assert!(n == 0 || n == 12, "{n}");
}

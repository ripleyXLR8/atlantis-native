//! The dialog videos (DIALOG\PERSO\*.UBB) against the original atlantis.exe:
//! the 8-bit frames of its IV decoder FUN_0041101f, and the composite of
//! its dialog compositor FUN_00450b08 (colours from FUN_00450e61, blend
//! FUN_004508e0), all run under emulation for the golden values.
//!
//! The data tests need the game: set `ATLANTIS_DATA` to the game directory
//! (atlantis.exe and BIGCD1..4.BIG); without it they are skipped. Run them
//! with `--release`: debug builds decode slowly.

use std::collections::BTreeMap;
use std::sync::Arc;

use atlantis_codecs::{palette_to_rgb565, UbbVideo};
use atlantis_data::sprite_blend::{blend_ubb_pixel565, compose_ubb_ia};
use atlantis_data::{Game, StdDir};
use md5::{Digest, Md5};

const FRAMES: &str = include_str!("golden/video_ubb_frames.txt");
const COMPOSITE: &str = include_str!("golden/video_ia_composite.txt");

const DIR: &str = "DIALOG\\PERSO\\";
const N: usize = 640 * 480;

fn golden(text: &str) -> BTreeMap<&str, &str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.rsplit_once(' ').expect("bad golden line"))
        .collect()
}

fn md5_hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn md5_le16(pixels: &[u16]) -> String {
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
    md5_hex(&bytes)
}

fn game() -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: dialog video test skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The dialog videos, sorted, as (file name, archive member name).
fn ubb_names(game: &Game) -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = game
        .big
        .names()
        .filter(|n| {
            n.to_ascii_uppercase().starts_with(DIR) && n.to_ascii_uppercase().ends_with(".UBB")
        })
        .map(|n| (n[DIR.len()..].to_ascii_uppercase(), n.to_string()))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// The background of the compositor golden values.
fn background() -> Vec<u16> {
    (0..N as u64)
        .map(|i| ((i * 2_654_435_761) >> 13) as u16)
        .collect()
}

/// The blend that replaces FUN_004508e0 to check the opcode walk alone.
fn standin_blend(fg: u16, dst: u16, shade: u32) -> u16 {
    if shade == 31 {
        return fg;
    }
    ((shade * 0x841) ^ u32::from(dst)).wrapping_add(u32::from(fg)) as u16
}

/// Every frame of every dialog video, decoded twice (the video loops as in
/// the dialog player), against FUN_0041101f.
#[test]
fn ubb_frames_match_original() {
    let Some(game) = game() else { return };
    let golden = golden(FRAMES);
    let names = ubb_names(&game);
    assert_eq!(names.len(), 124);
    let (mut frames, mut bad) = (0usize, Vec::new());
    for (name, member) in &names {
        let data = game.big.read(member).expect("cannot read archive member");
        let mut video = UbbVideo::new(data).expect("cannot open video");
        assert_eq!((video.width(), video.height()), (640, 480), "{name}");
        let n = video.frame_count();
        for k in 0..2 * n {
            let key = format!("{name}#{}", k % n);
            let got = match video.next_frame() {
                Ok(f) => {
                    assert_eq!(f.index, k % n, "{name}");
                    md5_hex(f.pixels)
                }
                Err(e) => format!("error: {e}"),
            };
            frames += 1;
            if golden.get(key.as_str()) != Some(&got.as_str()) {
                bad.push(format!("{key} (pass {}): got {got}", k / n + 1));
            }
        }
    }
    eprintln!(
        "{} files, {frames} frames, {} differ from FUN_0041101f",
        names.len(),
        bad.len()
    );
    assert_eq!(frames, 2 * golden.len());
    assert!(
        bad.is_empty(),
        "{} differ:\n{}",
        bad.len(),
        bad[..bad.len().min(20)].join("\n")
    );
}

/// Every frame of every dialog video composited over a test background
/// (frame, palette and IA stream from the decoder) against FUN_00450b08,
/// with the original blend and with a stand-in blend.
#[test]
fn ubb_composite_matches_original() {
    let Some(game) = game() else { return };
    let golden = golden(COMPOSITE);
    let names = ubb_names(&game);
    assert_eq!(names.len(), 124);
    let bg = background();
    let mut dst = vec![0u16; N];
    let (mut frames, mut bad) = (0usize, Vec::new());
    for (name, member) in &names {
        let data = game.big.read(member).expect("cannot read archive member");
        let mut video = UbbVideo::new(data).expect("cannot open video");
        for k in 0..video.frame_count() {
            let f = video.next_frame().expect("decode error");
            frames += 1;
            for (blend, real) in [("standin", false), ("real", true)] {
                dst.copy_from_slice(&bg);
                if real {
                    compose_ubb_ia(&mut dst, f.pixels, f.palette, f.ia, blend_ubb_pixel565);
                } else {
                    compose_ubb_ia(&mut dst, f.pixels, f.palette, f.ia, standin_blend);
                }
                let key = format!("{name}#{k}#{blend}");
                let got = md5_le16(&dst);
                if golden.get(key.as_str()) != Some(&got.as_str()) {
                    bad.push(format!("{key}: got {got}"));
                }
            }
        }
    }
    eprintln!(
        "{} files, {frames} frames composited, {} of {} composites differ from FUN_00450b08",
        names.len(),
        bad.len(),
        2 * frames
    );
    assert_eq!(2 * frames, golden.len());
    assert!(
        bad.is_empty(),
        "{} differ:\n{}",
        bad.len(),
        bad[..bad.len().min(20)].join("\n")
    );
}

/// Synthetic compositor inputs: a palette, a frame and an opcode stream
/// drawn from an LCG (the same generator produced the golden values).
struct Synth {
    pal: [u8; 768],
    src: Vec<u8>,
    ops: Vec<u8>,
}

fn synth() -> Synth {
    let mut x: u32 = 0x0045_0b08;
    let mut next = move || {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
        x >> 16
    };
    let mut pal = [0u8; 768];
    for p in &mut pal {
        *p = ((next() % 64) * 4) as u8;
    }
    let src: Vec<u8> = (0..N).map(|_| next() as u8).collect();
    let mut ops = Vec::new();
    let mut pos = 0u32;
    while pos < N as u32 - 4000 {
        let k = next() % 10;
        let n;
        if k < 3 {
            n = 1 + next() % 3000;
            ops.extend_from_slice(&[0, n as u8, (n >> 8) as u8, (n >> 16) as u8]);
        } else {
            n = next() % 256;
            ops.push(if k < 6 { 1 } else { 2 });
            ops.push(n as u8);
            let count = if k < 6 { 1 } else { n };
            for _ in 0..count {
                let a = next() % 3;
                ops.push(match a {
                    0 => 0xff,
                    1 => 0xf8,
                    _ => next() as u8,
                });
            }
        }
        pos += n;
    }
    ops.extend_from_slice(&[0; 4]);
    Synth { pal, src, ops }
}

/// The synthetic inputs, then FUN_00450e61's colours and FUN_00450b08's
/// composites (original and stand-in blend) from them.
#[test]
fn synthetic_composite_matches_original() {
    let s = synth();
    assert_eq!(s.ops.len(), 32656);
    assert_eq!(md5_hex(&s.ops), "dc874cfd9dd6e5478cd2ad7ba66e218a");
    assert_eq!(md5_hex(&s.pal), "4a9596f1a589fdb70b557f542bce1ada");
    assert_eq!(md5_hex(&s.src), "0fba06faf7992b2ea1ffd87552dc4999");

    let lut = palette_to_rgb565(&s.pal);
    assert_eq!(md5_le16(&lut), "c36ec22f6b667e9e6c08ed941f8e3057");

    let mut dst = background();
    compose_ubb_ia(&mut dst, &s.src, &lut, &s.ops, blend_ubb_pixel565);
    assert_eq!(md5_le16(&dst), "19bfdac58978c08eb475df1927aea7d0");

    let mut dst = background();
    compose_ubb_ia(&mut dst, &s.src, &lut, &s.ops, standin_blend);
    assert_eq!(md5_le16(&dst), "4347c4d120840c24542bb439fe787f82");
}

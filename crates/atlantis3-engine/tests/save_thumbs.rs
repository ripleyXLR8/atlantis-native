//! The save thumbnails against the original's chain under emulation
//! (`tests/golden/save_thumbs.txt`): the captured pictures (synthetic ones,
//! and the start page and the in-game menu at rest as the port draws them),
//! the stretch model, the conversion to x8r8g8b8, the pre-pass and the
//! encoder (md5 after each step), the encoder alone on the harness's
//! inputs, bank bytes, and the banks drawn by `AFF_SPR` in 16 and 32 bits.
//!
//! The synthetic part runs without the game; the game's pictures need
//! `ATLANTIS3_DATA`.

use std::collections::HashMap;

use atlantis3_data::cfg::Config;
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis3_data::spr_encode::{darken_black, ThumbEncoder};
use atlantis3_data::A3Files;
use atlantis3_engine::menu::{stretch, to_x888, Env, Host, Menus};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/save_thumbs.txt");

fn hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn md5_16(px: &[u16]) -> String {
    hex(&px.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<u8>>())
}

fn md5_32(px: &[u32]) -> String {
    hex(&px.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<u8>>())
}

/// `a3_save_thumbs.py synth565`.
fn synth565(w: usize, h: usize, kind: &str) -> Vec<u16> {
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let (r, g, b) = match kind {
                "ramp" => (
                    x * 31 / (w - 1),
                    y * 63 / (h - 1),
                    (x + y) * 31 / (w + h - 2),
                ),
                "bars" => {
                    if x < 3 || y < 3 || x >= w - 3 || y >= h - 3 {
                        (0, 0, 0)
                    } else if w / 10 <= x && x < w / 5 && h / 10 <= y && y < h / 5 {
                        (0, 1, 0)
                    } else {
                        [
                            (31, 63, 31),
                            (31, 63, 0),
                            (0, 63, 31),
                            (0, 63, 0),
                            (31, 0, 31),
                            (31, 0, 0),
                            (0, 0, 31),
                            (2, 4, 2),
                        ][x * 8 / w]
                    }
                }
                "checker" => {
                    let v = (x + y) & 1;
                    (31 * v, 63 * v, 31 * v)
                }
                _ => {
                    let v = ((x as u64)
                        .wrapping_mul(1_103_515_245)
                        .wrapping_add((y as u64).wrapping_mul(12_345))
                        .wrapping_add(0x9E37_79B9))
                    .wrapping_mul(2_654_435_761)
                        & 0xffff;
                    let v = v as usize;
                    (v >> 11, (v >> 5) & 63, v & 31)
                }
            };
            px.push(((r << 11) | (g << 5) | b) as u16);
        }
    }
    px
}

/// `a3_save.py thumb_image` (x8r8g8b8).
pub fn thumb_image(w: usize, h: usize, kind: &str) -> Vec<u32> {
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let c = match kind {
                "grad" => {
                    ((x * 255 / (w - 1).max(1)) << 16) as u32
                        | ((y * 255 / (h - 1).max(1)) << 8) as u32
                        | ((x + y) * 3 & 0xff) as u32
                }
                "bars" => {
                    if x < 2 || y < 2 || x >= w - 2 || y >= h - 2 {
                        0
                    } else if (10..20).contains(&x) && (10..20).contains(&y) {
                        0x050307
                    } else {
                        [
                            0xFFFFFF, 0xFFFF00, 0x00FFFF, 0x00FF00, 0xFF00FF, 0xFF0000, 0x0000FF,
                            0x101010,
                        ][x * 8 / w]
                    }
                }
                _ => {
                    ((x as u64)
                        .wrapping_mul(1_103_515_245)
                        .wrapping_add((y as u64).wrapping_mul(12_345))
                        .wrapping_add(0x9E37_79B9))
                    .wrapping_mul(2_654_435_761) as u32
                        & 0xFF_FFFF
                }
            };
            px.push(c);
        }
    }
    px
}

/// The `enc` inputs of the harness.
fn enc_input(name: &str, w: usize, h: usize) -> Vec<u32> {
    match name {
        "flat" => vec![0x0033_6699; w * h],
        "allblack" => vec![0; w * h],
        "frame" => (0..h)
            .flat_map(|y| {
                (0..w).map(move |x| {
                    if x == 0 || x == 51 || y == 0 || y == 39 {
                        0
                    } else {
                        ((x * 5) << 16 | (y * 6) << 8 | 0x80) as u32
                    }
                })
            })
            .collect(),
        "tiny" => [0xFF0000u32, 0x00FF00, 0x0000FF, 0xFFFFFF].repeat(3),
        "manycolours" => (0..(w * h) as u64)
            .map(|i| ((i * 2_654_435_761) >> 8) as u32 & 0xFF_FFFF)
            .collect(),
        kind => thumb_image(w, h, kind),
    }
}

struct NoSaves;

impl Host for NoSaves {
    fn exists(&mut self, _: &str) -> bool {
        false
    }
    fn read(&mut self, _: &str) -> Option<Vec<u8>> {
        None
    }
    fn thumbnail(&mut self, _: usize, _: &str) -> Option<Vec<u8>> {
        None
    }
}

/// The harness's game pictures: the start page at rest (100 s, pointer at
/// (5, 5)), then the in-game menu at rest drawn over it 1/32 s later.
fn game_art(files: &A3Files, w: usize, h: usize) -> [Vec<u16>; 2] {
    let mut config = Config::default();
    config.resolution = (w as i32, h as i32);
    config.langue = b"fra".to_vec();
    config.sous_titre = b"fra".to_vec();
    let mut menus = Menus::default();
    menus.load(files, w, h, &config);
    let mut screen = vec![0u16; w * h];
    let mut host = NoSaves;
    let mut shot = |menus: &mut Menus, page: u8, now: f64| {
        menus.page = page;
        let mut env = Env {
            files,
            config: &mut config,
            texts: None,
            host: &mut host,
            player: 1,
            now,
            pointer: (5, 5),
        };
        menus.frame(&mut env, &mut screen);
        screen.clone()
    };
    let start = shot(&mut menus, 0, 100.0);
    let princ = shot(&mut menus, 1, 100.03125);
    [start, princ]
}

/// The harness framebuffer's background (as `spr_aff_*` goldens).
fn background16() -> Vec<u16> {
    (0..1024 * 768u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 16) as u16)
        .collect()
}

fn background32() -> Vec<u32> {
    (0..1024 * 768u32)
        .map(|i| i.wrapping_mul(2_654_435_761))
        .collect()
}

#[test]
fn save_thumbnails_as_the_original() {
    let files = atlantis3_data::testing::install_dir()
        .and_then(|d| A3Files::open_dir(&d).ok())
        .map(|f| f.0);
    let mut pics: HashMap<String, (usize, usize, Vec<u16>)> = HashMap::new();
    let mut banks: HashMap<String, Vec<u8>> = HashMap::new();
    let mut art: HashMap<usize, [Vec<u16>; 2]> = HashMap::new();
    let (bg16, bg32) = (background16(), background32());
    let mut n = HashMap::<&str, usize>::new();
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        let kv = |k: &str| {
            f.iter()
                .find_map(|t| t.strip_prefix(k).and_then(|t| t.strip_prefix('=')))
                .unwrap_or("")
        };
        match f[0] {
            "pic" => {
                let (w, h): (usize, usize) = (f[2].parse().unwrap(), f[3].parse().unwrap());
                let kind = f[1].trim_end_matches(|c: char| c.is_ascii_digit());
                let px = match kind {
                    "ramp" | "bars" | "checker" | "noise" => synth565(w, h, kind),
                    _ => {
                        let Some(files) = &files else { continue };
                        let a = art.entry(w).or_insert_with(|| game_art(files, w, h));
                        a[usize::from(kind == "ingame_menu")].clone()
                    }
                };
                assert_eq!(md5_16(&px), kv("md5"), "{l}");
                pics.insert(f[1].to_string(), (w, h, px));
            }
            "chain" => {
                let Some((w, h, px)) = pics.get(f[1]) else {
                    continue;
                };
                let (tw, th): (usize, usize) = (f[2].parse().unwrap(), f[3].parse().unwrap());
                let st = stretch(px, *w, *h, tw, th);
                assert_eq!(md5_16(&st), kv("stretch"), "{l}");
                let mut c = to_x888(&st);
                assert_eq!(md5_32(&c), kv("conv"), "{l}");
                darken_black(&mut c);
                assert_eq!(md5_32(&c), kv("black"), "{l}");
                let enc = ThumbEncoder::new().encode(&c, tw, th, tw, &[]);
                assert_eq!(enc.len().to_string(), kv("len"), "{l}");
                let mut spr = b"SPRCRYO1".to_vec();
                spr.extend(enc);
                assert_eq!(hex(&spr), kv("spr"), "{l}");
                banks.insert(format!("{}_{tw}x{th}", f[1]), spr);
            }
            "enc" => {
                let (tw, th): (usize, usize) = (f[2].parse().unwrap(), f[3].parse().unwrap());
                let mut px = enc_input(f[1], tw, th);
                let guard = kv("guard") == "ff";
                if !guard {
                    assert_eq!(md5_32(&px), kv("in"), "{l}");
                }
                darken_black(&mut px);
                let tail = if guard { vec![0xff; 1024] } else { Vec::new() };
                let enc = ThumbEncoder::new().encode(&px, tw, th, tw, &tail);
                assert_eq!(enc.len().to_string(), kv("len"), "{l}");
                let mut spr = b"SPRCRYO1".to_vec();
                spr.extend(enc);
                assert_eq!(hex(&spr), kv("spr"), "{l}");
                if !guard {
                    let head: String = spr[..16].iter().map(|b| format!("{b:02x}")).collect();
                    assert_eq!(head, kv("head"), "{l}");
                    banks.insert(format!("enc_{}_{tw}x{th}", f[1]), spr);
                }
            }
            "hex" => {
                let key = format!("{}_{}x{}", f[1], f[2], f[3]);
                let Some(bank) = banks.get(&key) else {
                    continue;
                };
                let off: usize = f[4].parse().unwrap();
                let got: String = bank[off..(off + 64).min(bank.len())]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                assert_eq!(got, f[5], "{l}");
            }
            "draw" => {
                let Some(spr) = banks.get(f[1]) else { continue };
                let (x, y) = f[3].split_once(',').unwrap();
                let (x, y): (i32, i32) = (x.parse().unwrap(), y.parse().unwrap());
                let got = if f[2] == "565" {
                    let bank = Bank::parse(spr, Depth::Rgb565).unwrap();
                    let mut buf = bg16.clone();
                    let mut t = Target::screen(&mut buf, 1024, 768);
                    assert!(bank.draw(&mut t, Some(x), y, 0), "{l}: no blitter");
                    md5_16(&buf)
                } else {
                    let bank = Bank::parse(spr, Depth::Xrgb8888).unwrap();
                    let mut buf = bg32.clone();
                    let mut t = Target::screen(&mut buf, 1024, 768);
                    assert!(bank.draw(&mut t, Some(x), y, 0), "{l}: no blitter");
                    md5_32(&buf)
                };
                assert_eq!(got, kv("md5"), "{l}");
            }
            _ => continue,
        }
        *n.entry(f[0]).or_default() += 1;
    }
    // 12 synthetic pictures (+ 6 of the game), 3 sizes each; 14 encoder
    // inputs with two guards; the banks' bytes; 4 positions × 2 depths of
    // every bank (synthetic: 36 chains + 14 inputs).
    let game = usize::from(files.is_some());
    assert_eq!(n["pic"], 12 + 6 * game);
    assert_eq!(n["chain"], 36 + 18 * game);
    assert_eq!(n["enc"], 28);
    assert_eq!(n["draw"], 8 * (36 + 14) + 8 * 18 * game);
    assert!(n["hex"] > 0);
}

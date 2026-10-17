//! The sprite decoders and blitters against the original atlantis.exe:
//! every sprite of every SPR bank (FUN_00410782 + FUN_00410430), every
//! frame of every SPW and SPF (FUN_00452bde), every TGA (FUN_00449600 +
//! FUN_004174a0), the place sprite animation (FUN_00425304) and the dialog
//! compositor's blend (FUN_004508e0), all run under emulation for the golden
//! values.
//!
//! The data tests need the game: set `ATLANTIS_DATA` to the game directory
//! (atlantis.exe and BIGCD1..4.BIG); without it they are skipped. Run them
//! with `--release`.

mod common;
#[path = "data/ubb_blend.rs"]
mod ubb;

use atlantis_data::sprite::{composite_spf_frame, limits, read_spr_file, read_tga15};
use atlantis_data::sprite_anim::{animate_place_sprites, SPRITE_ANIM_SLOTS};
use atlantis_data::sprite_blend::{blend_ubb_pixel565, compose_ubb_ia};
use atlantis_data::SpritePlaceEntry;
use common::{fill_background, game, golden, golden_lines, md5_le16};

const FB_W: usize = 640;
const FB_H: usize = 480;
const CYC_W: usize = 2048;
const CYC_H: usize = 768;

/// Groups "FILE#rest" golden keys by FILE, in file order.
fn group_keys<'a>(g: &[(&'a str, &'a str)]) -> Vec<(&'a str, Vec<(&'a str, &'a str)>)> {
    let mut out: Vec<(&str, Vec<(&str, &str)>)> = Vec::new();
    for &(key, md5) in g {
        let (file, rest) = key.rsplit_once('#').unwrap();
        match out.last_mut() {
            Some((f, v)) if *f == file => v.push((rest, md5)),
            _ => out.push((file, vec![(rest, md5)])),
        }
    }
    out
}

/// Every sprite of every SPR bank, three blit positions each.
#[test]
fn spr_all_files_match_original() {
    let Some(game) = game("SPR test") else { return };
    let golden = golden(include_str!("golden/graphics_spr.txt"));
    let (mut total, mut bad) = (0, Vec::new());
    let mut fb = vec![0u16; FB_W * FB_H];
    for (file, keys) in group_keys(&golden) {
        let data = game
            .big
            .read(file)
            .unwrap_or_else(|| panic!("{file} missing"));
        let (max_w, max_h) = limits::DEFAULT;
        let sprites = read_spr_file(&data, max_w, max_h).unwrap_or_default();
        for (rest, want) in keys {
            let (idx, tag) = rest.split_once('@').unwrap();
            let spr = &sprites[idx.parse::<usize>().unwrap()];
            let (w, h) = (i32::from(spr.w), i32::from(spr.h));
            let (mut x, mut y) = (i32::from(spr.xoff), i32::from(spr.yoff));
            match tag {
                "clipA" => {
                    x -= w / 3;
                    y -= h / 3;
                }
                "clipB" => {
                    x += FB_W as i32 - (2 * w) / 3;
                    y += FB_H as i32 - (2 * h) / 3;
                }
                _ => {}
            }
            fill_background(&mut fb);
            spr.draw(&mut fb, FB_W, FB_H, x, y);
            total += 1;
            if md5_le16(&fb) != want {
                bad.push(format!("{file}#{rest}"));
            }
        }
    }
    eprintln!("SPR: {total} blits compared, {} differ", bad.len());
    assert_eq!(total, golden.len());
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(10)]);
}

fn spw_files(golden_text: &str, w: usize, h: usize, what: &str) {
    let Some(game) = game(what) else { return };
    let golden = golden(golden_text);
    let (mut total, mut bad) = (0, Vec::new());
    let mut p = vec![0u16; w * h];
    for (file, keys) in group_keys(&golden) {
        let data = game
            .big
            .read(file)
            .unwrap_or_else(|| panic!("{file} missing"));
        for (rest, want) in keys {
            fill_background(&mut p);
            composite_spf_frame(&data, rest.parse().unwrap(), &mut p, w, h);
            total += 1;
            if md5_le16(&p) != want {
                bad.push(format!("{file}#{rest}"));
            }
        }
    }
    eprintln!("{what}: {total} frames compared, {} differ", bad.len());
    assert_eq!(total, golden.len());
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(10)]);
}

/// Every frame of every SPW, into a 2048x768 panorama buffer.
#[test]
fn spw_all_files_match_original() {
    spw_files(
        include_str!("golden/graphics_spw.txt"),
        CYC_W,
        CYC_H,
        "SPW test",
    );
}

/// Every frame of every SPF, into a 640x480 buffer.
#[test]
fn spf_all_files_match_original() {
    spw_files(
        include_str!("golden/graphics_spf.txt"),
        FB_W,
        FB_H,
        "SPF test",
    );
}

/// Every TGA picture, top-down RGB565.
#[test]
fn tga_all_files_match_original() {
    let Some(game) = game("TGA test") else { return };
    let golden = golden(include_str!("golden/graphics_tga.txt"));
    let mut bad = Vec::new();
    for &(file, want) in &golden {
        let data = game
            .big
            .read(file)
            .unwrap_or_else(|| panic!("{file} missing"));
        let img = read_tga15(&data).expect("not a 15-bit TGA");
        assert_eq!((img.width, img.height), (FB_W, FB_H), "{file}");
        if md5_le16(&img.pixels) != want {
            bad.push(file);
        }
    }
    eprintln!(
        "TGA: {} images compared, {} differ",
        golden.len(),
        bad.len()
    );
    assert!(bad.is_empty(), "{bad:#?}");
}

/// FUN_00425304 on 24 synthetic place tables of up to 5 rows (active,
/// type 0..3, frame count 1..8), a current animation row and 60 calls at
/// increasing tick values each; after every call the rows' active flags and
/// frames and WSPRFRAME. Needs no game data.
#[test]
fn sprite_anim_matches_original() {
    let mut entries: Vec<SpritePlaceEntry> = Vec::new();
    let mut current = 0usize;
    let mut last = [0u32; SPRITE_ANIM_SLOTS];
    let mut wspr = 0i32;
    let (mut scenarios, mut steps) = (0, 0);
    let mut bad = Vec::new();
    for l in golden_lines(include_str!("golden/graphics_sprite_anim.txt")) {
        let mut t = l.split_whitespace();
        match t.next() {
            Some("S") => {
                scenarios += 1;
                entries.clear();
                last = [0; SPRITE_ANIM_SLOTS];
                wspr = 0;
                current = t.next().unwrap().parse().unwrap();
                for row in t {
                    let v: Vec<i32> = row.split(',').map(|x| x.parse().unwrap()).collect();
                    entries.push(SpritePlaceEntry {
                        place_id: 5,
                        active: v[0] != 0,
                        kind: v[1],
                        frame_count: v[2],
                        ..SpritePlaceEntry::default()
                    });
                }
            }
            Some("T") => {
                steps += 1;
                let ticks: u32 = t.next().unwrap().parse().unwrap();
                animate_place_sprites(&mut entries, 5, current, ticks, &mut last, &mut wspr, false);
                let mut got = format!("T {ticks}");
                for e in &entries {
                    got += &format!(" {}:{}", u8::from(e.active), e.frame);
                }
                got += &format!(" W {wspr}");
                if got != l {
                    bad.push(format!("got '{got}', original '{l}'"));
                }
            }
            _ => {}
        }
    }
    assert_eq!(scenarios, 24);
    assert_eq!(steps, 24 * 60);
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(5)]);
}

/// blend_ubb_pixel565(fg, dst, shade) against FUN_004508e0(shade, fg, dst)
/// for the shades the compositor blends (0..30).
#[test]
fn ubb_blend_matches_original() {
    for (lv, row) in ubb::BLEND_ORIG.iter().enumerate().take(31) {
        for (&(fg, dst), &want) in ubb::BLEND_PAIRS.iter().zip(row) {
            assert_eq!(
                blend_ubb_pixel565(fg, dst, lv as u32),
                want,
                "shade {lv} fg {fg:04x} dst {dst:04x}"
            );
        }
    }
}

/// Opcode 2 with shade 31: the original still calls the blend
/// (FUN_00450b08), whose result differs from fg in some cases.
#[test]
fn ubb_op2_full_shade_blends() {
    for (&(fg, dst), &want) in ubb::BLEND_PAIRS.iter().zip(&ubb::BLEND_ORIG[31]) {
        let mut pal = [0u16; 256];
        pal[0] = fg;
        let mut out = [dst];
        compose_ubb_ia(&mut out, &[0], &pal, &[2, 1, 0xf8], blend_ubb_pixel565);
        assert_eq!(out[0], want);
    }
}

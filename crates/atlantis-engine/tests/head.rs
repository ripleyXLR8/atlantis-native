//! The 3D talking heads of the dialogs (the animated mouth) against the
//! original `atlantis.exe` (2001 English build): the head of the line
//! player `FUN_0042ee88` (step 4) and its per-frame calls `FUN_0044dd10`,
//! `FUN_0044e860` and `FUN_0044c9c0`, run under emulation for the golden
//! values (digests of the projected vertices and of the drawn pixels).
//! Ported from ScummVM's `test/engines/cryomni3d/atlantis/video_mouth.h`.
//!
//! The data tests need the game: set `ATLANTIS_DATA` to the game directory;
//! without it they are skipped. Run them with `--release`.

use std::collections::HashMap;
use std::sync::Arc;

use atlantis_data::{Game, StdDir};
use atlantis_engine::dialog::rules::DialInit;
use atlantis_engine::head::{DialogHead, ProjectedVertex};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/video_mouth.txt");

/// The traced heads (INITDIAL row, camera), with ATA0001A.SYC.
const HEADS: [(i32, i32); 9] = [
    (539, 2),
    (543, 2),
    (551, 3),
    (560, 4), // group type 2
    (512, 2),
    (567, 3),
    (650, 5), // group type 9
    (552, 3), // types 2 + 1
    (525, 2), // bottom clipping
];
/// The traced frames: (UBB frame index, elapsed ms).
const FRAMES: [(i32, i32); 6] = [
    (0, 0),
    (3, 120),
    (12, 500),
    (25, 1000),
    (40, 1600),
    (60, 2400),
];

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn golden() -> HashMap<&'static str, &'static str> {
    GOLDEN
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.rsplit_once(' '))
        .collect()
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

fn load(g: &Game, init: &DialInit, row: i32, cam: i32, syc: &str) -> Option<DialogHead> {
    let r = init.rows.get(&row)?;
    DialogHead::load(
        |name| g.big.read(name),
        &r.s3d,
        &r.anim_3dc,
        &r.anim_3da,
        &r.anim_m,
        cam,
        syc,
    )
}

fn verts_md5(pv: &[ProjectedVertex]) -> String {
    let mut txt = String::new();
    for (i, v) in pv.iter().enumerate() {
        if v.flags & 0x40 != 0 {
            txt += &format!("{i} {} {}\n", v.x, v.y);
        }
    }
    md5_hex(txt.as_bytes())
}

/// Lip-sync, head animation, camera, tree walk and projection; then the
/// rasteriser: every traced frame against the original.
#[test]
fn heads_match_the_original() {
    let Some(g) = game("heads_match_the_original") else {
        return;
    };
    // The traced voice (ATA0001A.SYC) is the English CD's; the other
    // languages have their own lip-sync tracks.
    if g.exe.build.name != "atlantis-cd-en-714240" {
        eprintln!(
            "{}: not the traced build, heads_match_the_original skipped",
            g.exe.build.name
        );
        return;
    }
    let gold = golden();
    let init = DialInit::parse(&g.big.read("DIALOG\\INITDIAL.TXT").expect("INITDIAL.TXT"));
    let mut buf = vec![0u16; 640 * 480];
    let (mut checked, mut failed) = (0, Vec::new());
    for (row, cam) in HEADS {
        let mut head = load(&g, &init, row, cam, "ATA0001A.SYC")
            .unwrap_or_else(|| panic!("head {row} cam {cam} does not load"));
        for (frame, ms) in FRAMES {
            head.set_frame(frame);
            let r = head.lipsync(ms);
            head.transform();
            buf.fill(0);
            head.render(&mut buf, 640);
            let le: Vec<u8> = buf.iter().flat_map(|p| p.to_le_bytes()).collect();
            let key = format!("{row} {cam} {frame} {ms}");
            for (what, got) in [
                ("lipsync", r.to_string()),
                ("verts", verts_md5(head.projected())),
                ("pixels", md5_hex(&le)),
            ] {
                let k = format!("{key} {what}");
                let want = gold
                    .get(k.as_str())
                    .unwrap_or_else(|| panic!("{k} not in golden"));
                checked += 1;
                if got != *want {
                    failed.push(format!("{k}: {got} != {want}"));
                }
            }
        }
    }
    eprintln!("{checked} values checked, {} differ", failed.len());
    assert!(failed.is_empty(), "{failed:#?}");
    assert_eq!(checked, 162);
}

/// Every talking head of the INITDIAL table, at each of its cameras, loads
/// and draws something over its lines' frames.
#[test]
fn every_head_loads_and_draws() {
    let Some(g) = game("every_head_loads_and_draws") else {
        return;
    };
    let init = DialInit::parse(&g.big.read("DIALOG\\INITDIAL.TXT").expect("INITDIAL.TXT"));
    let mut rows: Vec<i32> = init.rows.keys().copied().collect();
    rows.sort_unstable();
    let mut buf = vec![0u16; 640 * 480];
    let (mut loaded, mut drawn) = (0, 0);
    for row in rows {
        let r = &init.rows[&row];
        if r.s3d.is_empty() || r.anim_3dc.is_empty() || r.anim_3da.is_empty() || r.anim_m.is_empty()
        {
            continue;
        }
        for cam in 2..=5 {
            let Some(mut head) = load(&g, &init, row, cam, "ATA0001A.SYC") else {
                continue;
            };
            loaded += 1;
            let mut any = false;
            for f in 0..40 {
                head.set_frame(f);
                if head.lipsync(f * 80) < 0 {
                    head.neutral();
                }
                head.transform();
                buf.fill(0);
                head.render(&mut buf, 640);
                any |= buf.iter().any(|&p| p != 0);
            }
            drawn += usize::from(any);
        }
    }
    eprintln!("{loaded} heads loaded, {drawn} drew pixels");
    assert!(loaded >= 100, "{loaded} heads loaded");
    assert!(drawn * 10 >= loaded * 9, "{drawn} of {loaded} drew pixels");
}

/// The HD mode's copies of a head (`DialogHead::scaled`): at tile 16 the
/// original's pixels; at 32 and 64 the same head over 4 and 16 times the
/// pixels, the same at every run, the original-size head unchanged.
#[test]
fn scaled_heads() {
    let Some(g) = game("scaled_heads") else {
        return;
    };
    let init = DialInit::parse(&g.big.read("DIALOG\\INITDIAL.TXT").expect("INITDIAL.TXT"));
    let mut buf = vec![0u16; 640 * 480];
    let mut buf16 = vec![0u16; 640 * 480];
    let mut checked = 0;
    for (row, cam) in HEADS {
        let Some(mut head) = load(&g, &init, row, cam, "ATA0001A.SYC") else {
            continue;
        };
        let mut h16 = head.scaled(16);
        let mut h32 = head.scaled(32);
        let mut h32b = head.scaled(32);
        let mut h64 = head.scaled(64);
        assert_eq!(h32.output_size(), (1280, 960));
        assert_eq!(h64.output_size(), (2560, 1920));
        let mut b32 = vec![0u16; 1280 * 960];
        let mut m32 = vec![0u8; 1280 * 960];
        let mut b32b = vec![0u16; 1280 * 960];
        let mut m32b = vec![0u8; 1280 * 960];
        let mut b64 = vec![0u16; 2560 * 1920];
        let mut m64 = vec![0u8; 2560 * 1920];
        for (frame, ms) in FRAMES {
            for h in [&mut head, &mut h16, &mut h32, &mut h32b, &mut h64] {
                h.set_frame(frame);
                h.lipsync(ms);
                h.transform();
            }
            buf.fill(0);
            head.render(&mut buf, 640);
            buf16.fill(0);
            h16.render(&mut buf16, 640);
            assert_eq!(buf, buf16, "head {row} cam {cam} frame {frame}");
            m32.fill(0);
            m32b.fill(0);
            m64.fill(0);
            h32.render_masked(&mut b32, 1280, &mut m32);
            h32b.render_masked(&mut b32b, 1280, &mut m32b);
            h64.render_masked(&mut b64, 2560, &mut m64);
            assert!(
                m32 == m32b && b32 == b32b,
                "head {row} cam {cam}: not deterministic"
            );
            let n1 = buf.iter().filter(|&&p| p != 0).count();
            let n2 = m32.iter().filter(|&&m| m != 0).count();
            let n4 = m64.iter().filter(|&&m| m != 0).count();
            if n1 > 500 {
                let (r2, r4) = (n2 as f64 / n1 as f64, n4 as f64 / n1 as f64);
                assert!(
                    (3.0..5.0).contains(&r2),
                    "head {row} cam {cam}: x2 ratio {r2}"
                );
                assert!(
                    (12.0..20.0).contains(&r4),
                    "head {row} cam {cam}: x4 ratio {r4}"
                );
                checked += 1;
            }
        }
    }
    eprintln!("{checked} scaled frames checked");
    assert!(checked >= 20, "{checked}");
}

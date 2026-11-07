//! The inventory overlay against the original's code under emulation
//! (`tests/golden/inventory_fade.txt`): for 1 to 12 objects at three
//! screen sizes, the fade in and out frame by frame (alpha bits), each
//! sprite call (bank, sprite, position, alpha, AFF_SPR or AFF_SPRTRS and
//! its level), the screen's md5 over a 0xf81f background, and the picks.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis2_engine::script::BagEntry;
use atlantis3_data::spr::{Bank, Depth};
use atlantis3_data::A3Files;
use atlantis3_engine::inventory::{aff, draw_items, Aff, Banks, Inventory};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/inventory_fade.txt");

fn md5(px: &[u16]) -> String {
    let b: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(&b).iter().map(|x| format!("{x:02x}")).collect()
}

fn line(a: &Aff) -> String {
    let what = match a.drawn {
        None => "skip".to_string(),
        Some(None) => format!("SPR {} {} -1", a.x, a.y),
        Some(Some(l)) => format!("TRS {} {} {l}", a.x, a.y),
    };
    format!(
        "{} {} {} {} {:08x} {what}",
        a.bank,
        a.index,
        a.x,
        a.y,
        a.alpha.to_bits()
    )
}

fn hexfloat(s: &str) -> f64 {
    // `0x1.0000000000000p-6`: the cases' steps are powers of two.
    let e: i32 = s.split('p').nth(1).unwrap().parse().unwrap();
    2f64.powi(e)
}

#[test]
fn the_inventory_fades_and_picks_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let bank = |p: &str| Bank::parse(&files.read(p).unwrap(), Depth::Rgb565).unwrap();
    let interface = bank("spr/Interface.spr");
    let objects = bank("spr/HoggarObjets.spr");
    let arrows = bank("spr/fleche.SPR");
    let banks = Banks {
        interface: Some(&interface),
        objects: Some(&objects),
        arrows: Some(&arrows),
    };
    let lines: Vec<&str> = GOLDEN.lines().filter(|l| !l.starts_with('#')).collect();
    let (mut w, mut h, mut dt) = (800usize, 600usize, 0.0f64);
    let mut bag: Vec<BagEntry> = Vec::new();
    let mut inv = Inventory::default();
    let mut now = 100.0f64;
    let mut screen = Vec::new();
    let mut calls: Vec<String> = Vec::new();
    let mut checked = (0, 0, 0);
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "case" => {
                (w, h) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                dt = hexfloat(f[4]);
                bag.clear();
                screen = vec![0xf81fu16; w * h];
            }
            "bag" => bag.push(BagEntry {
                id: f[2].parse().unwrap(),
                sprite: f[3].parse().unwrap(),
                world: 1,
            }),
            "open" => {
                now = 100.0;
                inv = Inventory {
                    open: true,
                    alpha: 0.0,
                    fade_time: now,
                    ..Inventory::default()
                };
            }
            "close" => inv.open = false,
            "frame" => {
                let phase_in = f[3] == "in";
                now += dt;
                screen.fill(0xf81f);
                calls.clear();
                if phase_in {
                    calls.extend(
                        draw_items(
                            &mut screen,
                            w,
                            h,
                            &banks,
                            &bag,
                            inv.scroll,
                            inv.alpha,
                            "HoggarObjets.spr",
                        )
                        .iter()
                        .map(line),
                    );
                    let (px, py) = ((w / 2) as i32, (h / 2) as i32);
                    calls.push(line(&aff(
                        &mut screen,
                        w,
                        h,
                        "Interface.spr",
                        &interface,
                        40,
                        px,
                        py,
                        1.0,
                    )));
                    inv.fade_step(now);
                } else if inv.alpha > 0.0 {
                    inv.fade_step(now);
                    calls.extend(
                        draw_items(
                            &mut screen,
                            w,
                            h,
                            &banks,
                            &bag,
                            inv.scroll,
                            inv.alpha,
                            "HoggarObjets.spr",
                        )
                        .iter()
                        .map(line),
                    );
                }
                assert_eq!(format!("alpha={:08x}", inv.alpha.to_bits()), f[5], "{l}");
                assert_eq!(format!("md5={}", md5(&screen)), f[8], "{l}");
                // The frame's draw lines.
                let mut want = Vec::new();
                while i + 1 < lines.len() && lines[i + 1].starts_with("draw ") {
                    i += 1;
                    want.push(lines[i].splitn(7, ' ').nth(6).unwrap().to_string());
                }
                assert_eq!(calls, want, "{l}");
                checked.0 += 1;
                checked.1 += want.len();
            }
            "pick" => {
                let (px, py): (i32, i32) = (f[3].parse().unwrap(), f[4].parse().unwrap());
                let click = f[5] == "click=1";
                let got = if click {
                    inv.pick(w, h, (px, py), &bag).map_or(-1, |e| e.id)
                } else {
                    -1
                };
                assert_eq!(got.to_string(), f[7], "{l}");
                checked.2 += 1;
            }
            _ => {}
        }
        i += 1;
    }
    assert_eq!(checked, (427, 3119, 800));
}

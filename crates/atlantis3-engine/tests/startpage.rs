//! The start page against the original's code under emulation
//! (`tests/golden/startpage.txt`): the element table, the hovered element
//! on a 10-pixel grid, and each frame's sprites and screen (md5) at rest,
//! hovered and clicked.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::menu::Page;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/startpage.txt");

fn md5(px: &[u16]) -> String {
    let b: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(&b).iter().map(|x| format!("{x:02x}")).collect()
}

fn hexfloat(s: &str) -> f64 {
    // `0x1.9020000000000p+6`
    let (m, e) = s.trim_start_matches("0x").split_once('p').unwrap();
    let (int, frac) = m.split_once('.').unwrap();
    let mut v = f64::from(u32::from_str_radix(int, 16).unwrap());
    for (i, c) in frac.chars().enumerate() {
        v += f64::from(c.to_digit(16).unwrap()) / 16f64.powi(i as i32 + 1);
    }
    v * 2f64.powi(e.parse().unwrap())
}

#[test]
fn the_start_page_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let (w, h) = (800, 600);
    let mut page = Page::load(&files, "i_debut", w, h).unwrap();
    let mut screen = vec![0u16; w * h];
    let (mut hovers, mut frames) = (0, 0);
    let mut case = String::new();
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "element" => {
                let k: usize = f[1].parse().unwrap();
                let it = &page.items[k];
                assert_eq!(String::from_utf8_lossy(&it.el.nomref), f[3], "{l}");
                assert_eq!(
                    it.bank.as_ref().map_or(0, |b| b.len()).to_string(),
                    f[7].trim_start_matches("frames="),
                    "{l}"
                );
            }
            "hover" => {
                let y: i32 = f[1].parse().unwrap();
                for (i, want) in f[2..].iter().enumerate() {
                    let x = 10 * i as i32;
                    let got = page.hit((x, y)).map_or("-".to_string(), |k| {
                        String::from_utf8_lossy(&page.items[k].el.nomref).into_owned()
                    });
                    assert_eq!(&got, want, "hover at {x}, {y}");
                    hovers += 1;
                }
            }
            "frame" => {
                if f[1] != case {
                    // Each case starts from a fresh page.
                    page = Page::load(&files, "i_debut", w, h).unwrap();
                    case = f[1].to_string();
                }
                let t = hexfloat(f[3]);
                let p = (f[4].parse().unwrap(), f[5].parse().unwrap());
                let draws = page.frame(&mut screen, w, h, p, t);
                let got: Vec<String> = draws
                    .iter()
                    .map(|d| {
                        format!(
                            "{}#{}@{}",
                            d.bank,
                            d.frame,
                            if d.pos.is_none() {
                                "def".into()
                            } else {
                                format!("{:?}", d.pos)
                            }
                        )
                    })
                    .collect();
                assert_eq!(format!("draws={}", got.join(",")), f[8], "{l}");
                assert_eq!(format!("md5={}", md5(&screen)), f[7], "{l}");
                frames += 1;
            }
            _ => {}
        }
    }
    assert_eq!((hovers, frames), (60 * 80, 90));
}

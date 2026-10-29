//! The dialog heads against the original under emulation
//! (`tests/golden/head_frames.txt`): `ToS1.DB` at 800 x 600 over a
//! checker, cameras 1 and 2, the `-base` mouth, a pure `-a`, a mix with
//! mood 1, at frames 0, 20 and 39, then 80 ticks of eyes and pose; the
//! screen's md5 after each.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis2_data::tga::Tga;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::head::Head;
use atlantis2_engine::Fpu;
use atlantis3_data::A3Files;
use atlantis3_engine::lipsync::lerp_factors;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/head_frames.txt");

fn md5(px: &[u16]) -> String {
    let b: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(&b).iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn heads_drawn_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let scene = atlantis3_data::db::parse(&files.read("mesh/ToS1.DB").unwrap()).unwrap();
    let tga = |n: &str| {
        files
            .read(&format!("map/Stand1/{n}.tga"))
            .and_then(|d| Tga::parse(&d).ok())
    };
    let mut head = Head::new_a3(
        Fpu::Chop,
        &scene,
        "to",
        800,
        600,
        tga("To").unwrap(),
        [tga("Toyf"), tga("Toymf"), tga("Toyd"), tga("Toyg")],
    )
    .unwrap();
    let (w, h) = (800usize, 600usize);
    let body: Vec<u16> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            if ((x >> 3) ^ (y >> 3)) & 1 != 0 {
                0x4208
            } else {
                0x8410
            }
        })
        .collect();
    let mut rand = Rand::new(1);
    let fpu = Fpu::Chop;
    let mut n = 0;
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        let get = |k: &str| f[f.iter().position(|x| *x == k).unwrap() + 1];
        let (cam, mouth, mood, frame) = if f[0] == "FRAME" {
            (
                get("cam").parse::<usize>().unwrap(),
                get("mouth"),
                get("mood").parse::<i32>().unwrap(),
                get("frame").parse::<i32>().unwrap(),
            )
        } else {
            (1, "base", 0, get("frame").parse().unwrap())
        };
        head.eyes_a3(&mut rand);
        head.pose(frame);
        let weights: Option<[f32; 9]> = match mouth {
            "a" => Some([0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            "mix" => Some([0.0, 0.2, 0.3, 0.0, 0.1, 0.0, 0.0, 0.1, 0.3]),
            _ => None,
        };
        let factors = weights.map_or([0.0; 9], |w| lerp_factors(fpu, &w));
        head.mouth_a3(mood, weights.as_ref(), &factors);
        let mut px = body.clone();
        let mut z = vec![0u32; w * h];
        head.display_a3(cam, &mut px, &mut z, w, h);
        if n == 0 {
            if let Ok(path) = std::env::var("HEAD_PNG") {
                std::fs::write(path, atlantis2_engine::png::encode_rgb565(&px, w, h)).unwrap();
                let raw: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
                std::fs::write(std::env::var("HEAD_RAW").unwrap(), raw).unwrap();
            }
        }
        let (eyes, k) = head.eyes_state_a3().unwrap();
        assert_eq!(eyes.to_string(), get("eyes"), "{l}");
        if f[0] == "TICK" {
            assert_eq!(format!("{:08x}", k.to_bits()), get("k"), "{l}");
        }
        assert_eq!(md5(&px), get("md5"), "{l}");
        n += 1;
    }
    assert_eq!(n, 18 + 80);
}

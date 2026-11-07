//! The TGA maps (heads, the unicorn board) copied into each display
//! format, against the original's loader (@ 0x00418e70 → @ 0x004ecc40)
//! under emulation (`tests/golden/tga_depth.txt`): RGB565 the top bits of
//! each channel, x8r8g8b8 the channels as they are, x 0.
//!
//! Needs `ATLANTIS3_DATA`.

use atlantis2_data::tga::{Tga, Tga32};
use atlantis3_data::A3Files;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/tga_depth.txt");

fn md5(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn maps_in_both_formats_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut n = 0;
    for line in GOLDEN.lines().filter(|l| l.starts_with("tga ")) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let data = files.read(f[1]).unwrap();
        let want = f[6].trim_start_matches("md5=");
        let got = if f[2] == "x8r8g8b8" {
            let t = Tga32::parse(&data).unwrap();
            assert_eq!(
                (t.width.to_string(), t.height.to_string()),
                (f[3].into(), f[4].into())
            );
            md5(&t
                .pixels
                .iter()
                .flat_map(|p| p.to_le_bytes())
                .collect::<Vec<_>>())
        } else {
            let t = Tga::parse(&data).unwrap();
            md5(&t
                .pixels
                .iter()
                .flat_map(|p| p.to_le_bytes())
                .collect::<Vec<_>>())
        };
        assert_eq!(got, want, "{} {}", f[1], f[2]);
        n += 1;
    }
    assert_eq!(n, 16);
}

//! Place sprites after a switch to a 32-bit display (`tests/golden/
//! spw_stale.txt`): the view's SPW banks, loaded for RGB565, drawn by the
//! 32-bit blitters of `spr_p6.dll` (the options' switch sets the library
//! up again but keeps the slots until the next view entry), against the
//! original's `AFF_SPW` under emulation.
//!
//! Needs `ATLANTIS3_DATA`.

use atlantis3_data::spr::{Bank, Depth, SprDll, Target};
use atlantis3_data::A3Files;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/spw_stale.txt");

fn md5(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn stale_banks_drawn_at_32_bits() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let bg: Vec<u32> = (0..512u32 * 512)
        .map(|i| i.wrapping_mul(2_654_435_761))
        .collect();
    let bytes = |v: &[u32]| -> Vec<u8> { v.iter().flat_map(|p| p.to_le_bytes()).collect() };
    let mut n = 0;
    for line in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let (key, want) = line.split_once(' ').unwrap();
        let name = key.trim_end_matches("#0");
        let bank = Bank::parse(&files.read(name).unwrap(), Depth::Rgb565)
            .unwrap()
            .as_drawn_at(Depth::Xrgb8888);
        let faces: Vec<String> = (0..6)
            .map(|face| {
                let mut buf = bg.clone();
                let mut t = Target::screen(&mut buf, 512, 512);
                bank.draw_face_with(&mut t, 0, face, SprDll::P6);
                md5(&bytes(&buf))
            })
            .collect();
        assert_eq!(md5(faces.join(" ").as_bytes()), want, "{name}");
        n += 1;
    }
    assert_eq!(n, 12);
}

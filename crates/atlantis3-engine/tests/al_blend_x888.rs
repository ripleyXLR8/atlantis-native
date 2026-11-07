//! The dialog body videos' `AL` blend on a 32-bit display against the
//! original's routine (@ 0x006a2031) run under emulation on synthetic
//! frames (`tests/golden/al_blend_x888.txt`, its inputs from the LCG
//! below, as the producer's docstring gives them): a mix of the video and
//! the background by a / 32 on each byte, rows of a wider target included.
//! No game data needed.

use atlantis_codecs::blend_x888;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/al_blend_x888.txt");

struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = (self.0.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x7fff_ffff;
        self.0
    }
}

#[test]
fn the_32_bit_blend_as_the_original() {
    let mut n = 0;
    for line in GOLDEN.lines().filter(|l| l.starts_with("case ")) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let k: u32 = f[1].parse().unwrap();
        let (w, h, pitch): (usize, usize, usize) = (
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
            f[4].parse().unwrap(),
        );
        let mut g = Lcg(k + 1);
        let video: Vec<u32> = (0..w * h).map(|_| g.next()).collect();
        let mut dst: Vec<u32> = (0..pitch * h).map(|_| g.next()).collect();
        let mut runs = Vec::new();
        let mut left = w * h;
        while left > 0 {
            let x = g.next();
            let n = (((x >> 8) as usize) % w.min(24) + 1).min(left);
            let y = g.next();
            let a = [0, 31, (y >> 16) % 32, (y >> 16) % 32][((y >> 4) & 3) as usize];
            runs.extend([(n - 1) as u8, a as u8]);
            left -= n;
        }
        blend_x888(&video, &mut dst, &runs, w, h, pitch).unwrap();
        let bytes: Vec<u8> = dst.iter().flat_map(|p| p.to_le_bytes()).collect();
        let md5: String = Md5::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(format!("md5={md5}"), f[5], "case {k}");
        n += 1;
    }
    assert_eq!(n, 8);
}

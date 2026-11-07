//! The heads' polygon routine (`Zi32Texture16Linear1516II`) against the
//! original run alone under emulation (`tests/golden/raster_linear.txt`):
//! 480 polygons at 640 x 480 and 800 x 600 with 512² and 1024² textures,
//! the colour and Z buffers' md5 after each.

use atlantis2_engine::raster3d::{linear_ii, Target, Texture, Vertex};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/raster_linear.txt");

fn md5(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn polygons_as_the_original_draws_them() {
    let mut cfgs = std::collections::HashMap::new();
    let mut n = 0;
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        if f[0] == "CFG" {
            let v: Vec<usize> = f[2..6].iter().map(|x| x.parse().unwrap()).collect();
            cfgs.insert(f[1].to_string(), (v[0], v[1], v[2], v[3]));
            continue;
        }
        let (w, h, tw, th) = cfgs[f[1]];
        let tex: Vec<u16> = (0..tw * th)
            .map(|i| ((i as u32).wrapping_mul(0x9E37_79B1) >> 16) as u16)
            .collect();
        let mut px: Vec<u16> = (0..w * h)
            .map(|i| {
                ((i as u32)
                    .wrapping_mul(0x2545_F491)
                    .wrapping_add(0x0123_4567)
                    >> 16) as u16
            })
            .collect();
        let mut z: Vec<u32> = (0..w * h)
            .map(|i| {
                (i as u32)
                    .wrapping_mul(0x9E37_79B1)
                    .wrapping_add(0x7F4A_7C15)
                    >> 1
            })
            .collect();
        let calls: usize = f[4].parse().unwrap();
        let parts: Vec<&str> = l.split(" | ").collect();
        for call in parts.iter().skip(1).take(calls) {
            let v: Vec<f64> = call
                .split(' ')
                .skip(1)
                .map(|x| f64::from(f32::from_bits(u32::from_str_radix(x, 16).unwrap())))
                .collect();
            let mut poly: Vec<Vertex> = v
                .chunks(5)
                .map(|c| Vertex {
                    x: c[0],
                    y: c[1],
                    w: c[2],
                    u: c[3],
                    v: c[4],
                })
                .collect();
            let t = Texture {
                pixels: &tex,
                width: tw,
                height: th,
            };
            let mut out = Target {
                pixels: &mut px,
                zbuf: &mut z,
                width: w,
                height: h,
            };
            linear_ii::draw_polygon(&mut poly, &t, &mut out);
        }
        let tail = parts.last().unwrap();
        let tf: Vec<&str> = tail.split(' ').collect();
        let cb: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
        let zb: Vec<u8> = z.iter().flat_map(|p| p.to_le_bytes()).collect();
        assert_eq!(md5(&cb), tf[1], "{l}");
        assert_eq!(md5(&zb), tf[3], "{l}");
        n += 1;
    }
    assert_eq!(n, 480);
}

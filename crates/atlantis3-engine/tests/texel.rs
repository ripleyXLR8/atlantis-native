//! The centre ray's face pixel (`Camera::texel_a3`) against the original
//! under emulation (`tests/golden/texel.txt`): 16,560 rays of seven
//! Hoggar views, from the original's camera quaternion, under both
//! control words.

use atlantis2_engine::{Camera, Fpu};

const GOLDEN: &str = include_str!("golden/texel.txt");

#[test]
fn the_centre_ray_lands_where_the_original_lands() {
    let mut n = 0;
    for l in GOLDEN.lines().filter(|l| l.starts_with("ray ")) {
        let f: Vec<&str> = l.split(' ').collect();
        let fpu = if f[2] == "0e7f" {
            Fpu::Chop
        } else {
            Fpu::Double
        };
        let mut c = Camera::new(fpu, 800, 600);
        let q: Vec<f32> = f[5]
            .trim_start_matches("q=")
            .split(',')
            .map(|h| f32::from_bits(u32::from_str_radix(h, 16).unwrap()))
            .collect();
        c.q = [q[0], q[1], q[2], q[3]];
        let got = match c.texel_a3(c.cx, c.cy) {
            Some((k, u, v)) => format!("{k} {u} {v}"),
            None => "-1 400 300".to_string(),
        };
        assert_eq!(got, f[6..9].join(" "), "{l}");
        n += 1;
    }
    assert_eq!(n, 16_560);
}

#[test]
fn entries_give_the_original_quaternions() {
    use atlantis2_engine::script::a3::deg_to_rad_a3;
    let (mut n, mut bad) = (0, 0);
    for l in GOLDEN.lines().filter(|l| l.starts_with("ray ")) {
        let f: Vec<&str> = l.split(' ').collect();
        let fpu = if f[2] == "0e7f" {
            Fpu::Chop
        } else {
            Fpu::Double
        };
        let (yaw, pitch): (i32, i32) = (f[3].parse().unwrap(), f[4].parse().unwrap());
        let mut c = Camera::new(fpu, 800, 600);
        c.enter(deg_to_rad_a3(fpu, yaw), deg_to_rad_a3(fpu, pitch));
        let got: Vec<String> = c.q.iter().map(|x| format!("{:08x}", x.to_bits())).collect();
        if got.join(",") != f[5].trim_start_matches("q=") {
            bad += 1;
            if bad < 5 {
                eprintln!("{l}: {}", got.join(","));
            }
        }
        n += 1;
    }
    eprintln!("entries {n} differing {bad}");
    assert_eq!(bad, 0);
}

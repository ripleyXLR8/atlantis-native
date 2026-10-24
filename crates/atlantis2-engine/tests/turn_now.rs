//! `AngleWarpXNow` and `AngleWarpYNow` against the original
//! (`tests/golden/turn_now.txt`): a view entered at (yaw, pitch) from the
//! start-up camera, then each step, under both control words; the camera
//! node's quaternion and the angles read back, bit for bit. No game data
//! needed.

use atlantis2_engine::camera::deg_to_rad;
use atlantis2_engine::{Camera, Fpu};

const GOLDEN: &str = include_str!("golden/turn_now.txt");

fn bits(q: [f32; 4]) -> String {
    q.iter()
        .map(|v| format!("{:08x}", v.to_bits()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn readback(c: &Camera) -> String {
    let (y, p) = c.angles();
    format!(
        "readback {:08x} {:08x} ({:.4} {:.4})",
        y.to_bits(),
        p.to_bits(),
        f64::from(y).to_degrees(),
        f64::from(p).to_degrees()
    )
}

#[test]
fn turn_now_as_the_original() {
    let mut fpu = Fpu::Double;
    let mut cam = Camera::new(fpu, 640, 480);
    let mut lines = 0;
    for line in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f[0] {
            "cw" => {
                fpu = match f[1] {
                    "027f" => Fpu::Double,
                    "0e7f" => Fpu::Chop,
                    other => panic!("control word {other}"),
                };
            }
            "case" => {
                let (yaw, pitch): (i32, i32) = (f[3].parse().unwrap(), f[4].parse().unwrap());
                cam = Camera::new(fpu, 640, 480);
                cam.enter(deg_to_rad(fpu, yaw), deg_to_rad(fpu, pitch));
                let got = format!(
                    "case {} entry {yaw} {pitch} q {} {}",
                    f[1],
                    bits(cam.q),
                    readback(&cam)
                );
                assert_eq!(got, line);
            }
            "x" | "y" => {
                let deg: i32 = f[1].parse().unwrap();
                let rad = deg_to_rad(fpu, deg);
                if f[0] == "x" {
                    cam.set_yaw(rad);
                } else {
                    cam.pitch_from_here(rad);
                }
                let got = format!(
                    "  {} {deg} rad {:08x} q {} {}",
                    f[0],
                    rad.to_bits(),
                    bits(cam.q),
                    readback(&cam)
                );
                assert_eq!(got, line);
            }
            other => panic!("golden line {other}"),
        }
        lines += 1;
    }
    assert_eq!(lines, 50);
}

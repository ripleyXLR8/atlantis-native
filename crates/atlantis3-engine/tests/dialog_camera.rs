//! The dialogs' panorama camera against the original under emulation
//! (`tests/golden/dialog_camera.txt`): `Orient` to every Hoggar head
//! camera and `Horizontalize`, from several view entries under both x87
//! control words (the quaternion, the matrix, the read-back angles), and
//! the field of view's conversion and projections.

use atlantis2_engine::{Camera, Fpu};
use std::collections::HashMap;

const GOLDEN: &str = include_str!("golden/dialog_camera.txt");

fn floats(s: &str) -> Vec<u32> {
    s.split(',')
        .map(|h| {
            let b = u32::from_str_radix(h, 16).unwrap();
            b.swap_bytes()
        })
        .collect()
}

fn field<'a>(l: &'a str, k: &str) -> &'a str {
    l.split(' ').find_map(|w| w.strip_prefix(k)).unwrap()
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn orient_and_horizontalize_as_the_original() {
    let mut heads: HashMap<(String, String, String), [f32; 4]> = HashMap::new();
    let mut starts: HashMap<(String, String), [f32; 4]> = HashMap::new();
    let mut n = 0;
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let w: Vec<&str> = l.split(' ').collect();
        let fpu = |cw: &str| if cw == "0e7f" { Fpu::Chop } else { Fpu::Double };
        let q4 = |s: &str| {
            let f = floats(s);
            [f[0], f[1], f[2], f[3]].map(f32::from_bits)
        };
        match w[0] {
            "HEADCAM" => {
                heads.insert(
                    (w[1].into(), w[2].into(), w[3].into()),
                    q4(field(l, "world=")),
                );
            }
            "START" => {
                starts.insert((w[1].into(), w[5].into()), q4(field(l, "q=")));
            }
            "ORIENT" | "HORIZ" => {
                let mut c = Camera::new(fpu(w[2]), 800, 600);
                c.q = starts[&(w[1].to_string(), w[2].to_string())];
                if w[0] == "ORIENT" {
                    c.orient_to(heads[&(w[3].into(), w[4].into(), w[5].into())]);
                } else {
                    c.horizontalize();
                }
                assert_eq!(bits(&c.q), floats(field(l, "q=")), "{l}");
                assert_eq!(bits(&c.matrix()), floats(field(l, "matrix=")), "{l}");
                let (y, p) = c.angles();
                assert_eq!(bits(&[y, p]), floats(field(l, "angles=")), "{l}");
                n += 1;
            }
            "FOV" => {
                let f = fpu(w[2]);
                let deg: f64 = w[1].parse().unwrap();
                let pi = f64::from(f32::from_bits(0x4049_0fdb));
                let rad = f.f32(f.mul(f.mul(pi, deg), 1.0 / 180.0));
                assert_eq!(vec![rad.to_bits()], floats(field(l, "rad=")), "{l}");
                let mut c = Camera::new(f, 800, 600);
                c.set_fov(rad, 800, 600);
                assert_eq!(bits(&c.projection()), floats(field(l, "proj800=")), "{l}");
                // Viewport(640, 480) (@ 0x0043f760): the centre too.
                (c.cx, c.cy) = (320.0, 240.0);
                c.set_fov(rad, 640, 480);
                assert_eq!(bits(&c.projection()), floats(field(l, "proj640=")), "{l}");
                n += 1;
            }
            _ => {}
        }
    }
    assert_eq!(n, 246);
}

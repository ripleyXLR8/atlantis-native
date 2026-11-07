//! The camera re-alignment (`INITRECALAGE`) against the original under
//! emulation (`tests/golden/recalage.txt`): the chapters' script angles,
//! view entries, `AngleWarpXNow` / `AngleWarpYNow` and re-alignments,
//! tick by tick, under both control words, and extra starts.

use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::turn::{Turn, Zoom};
use atlantis2_engine::{Camera, Fpu};

const GOLDEN: &str = include_str!("golden/recalage.txt");

fn hex(x: f32) -> String {
    format!("{:08x}", x.to_bits())
}

fn cam_line(c: &Camera) -> String {
    let (y, p) = c.angles();
    format!("{} {}", hex(y), hex(p))
}

#[test]
fn re_alignments_as_the_original() {
    let (w, h) = (800u32, 600u32);
    let mut fpu = Fpu::Double;
    let mut cam = Camera::new(fpu, w, h);
    let mut zoom = Zoom::default();
    let (mut sy, mut sp) = (0f32, 0f32);
    let (mut ay, mut ap) = (0f32, 0f32);
    let mut turn: Option<Turn> = None;
    let mut ticks = 0;
    let mut checked = 0;
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "cw" => {
                fpu = if f[1] == "0x0e7f" {
                    Fpu::Chop
                } else {
                    Fpu::Double
                };
            }
            "chain" => {
                cam = Camera::new(fpu, w, h);
                cam.enter(0.0, 0.0);
                (sy, sp, ay, ap) = (0.0, 0.0, 0.0, 0.0);
            }
            "case" => {
                cam = Camera::new(fpu, w, h);
                let (y, p): (i32, i32) = (f[3].parse().unwrap(), f[4].parse().unwrap());
                let (ry, rp) = (deg_to_rad_a3(fpu, y), deg_to_rad_a3(fpu, p));
                cam.enter(ry, rp);
                (sy, sp) = (
                    deg_to_rad_a3(fpu, f[1].split(':').nth(1).unwrap().parse().unwrap()),
                    deg_to_rad_a3(fpu, f[1].split(':').nth(2).unwrap().parse().unwrap()),
                );
                assert_eq!(cam_line(&cam), f[6..8].join(" "), "{l}");
                checked += 1;
            }
            "op" => {
                let v: i32 = f[3].parse().unwrap_or(0);
                match f[2] {
                    "x" | "y" | "xnow" | "ynow" => {
                        let r = deg_to_rad_a3(fpu, v);
                        if f[2].starts_with('x') {
                            sy = r;
                        } else {
                            sp = r;
                        }
                        if f[2] == "x" || f[2] == "y" {
                            (ay, ap) = (sy, sp);
                        } else if f[2] == "xnow" {
                            cam.set_yaw(sy);
                        } else {
                            cam.pitch_from_here(sp);
                        }
                        assert_eq!(
                            format!("yaw {} pitch {}", hex(sy), hex(sp)),
                            f[5..].join(" "),
                            "{l}"
                        );
                    }
                    "vue" => {
                        cam.enter(ay, ap);
                        assert_eq!(cam_line(&cam), f[6..8].join(" "), "{l}");
                    }
                    _ => panic!("{l}"),
                }
                checked += 1;
            }
            "now" => {
                assert_eq!(cam_line(&cam), f[4..6].join(" "), "{l}");
                checked += 1;
            }
            "rec" => {
                let (t, going) = Turn::new_a3(&cam, sy, sp, &zoom);
                assert_eq!(
                    format!("turning {}", u8::from(going)),
                    f[f.len() - 2..].join(" "),
                    "{l}"
                );
                turn = going.then_some(t);
                ticks = 0;
                checked += 1;
            }
            "tick" => {
                let t = turn.as_mut().expect("a turn");
                cam.set_fov(atlantis3_engine::game::dialog_fov(fpu, 76.0), w, h);
                let going = t.step(&mut cam, &mut zoom, (w, h)).going;
                ticks += 1;
                assert_eq!(
                    format!("{} {} {}", f[1], cam_line(&cam), u8::from(going)),
                    format!("{} {} {} {}", f[1], f[2], f[3], f[6]),
                    "{l}"
                );
                if !going {
                    turn = None;
                }
                checked += 1;
            }
            "end" => {
                assert_eq!(ticks.to_string(), f[3], "{l}");
                ticks = 0;
            }
            _ => panic!("{l}"),
        }
    }
    assert!(checked > 11_000, "{checked}");
}

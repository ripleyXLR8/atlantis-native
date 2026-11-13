//! *Atlantis Evolution*'s 2004 warp camera and `TANGAGEWARP` pitching
//! against the original's own functions under emulation
//! (`tests/golden/tangage_2004.txt`, atlantis4-cd-en-3022848 run by
//! `a4cd_tangage.py`): the view entry FUN_00442c80 with the pitching off
//! and on, the step FUN_00445760, the set FUN_00445450 and stop
//! FUN_004455b0, the turns FUN_00444920 / FUN_004449e0, `AngleWarpXNow`
//! FUN_00444a40, `AngleWarpYNow` FUN_00444b00, the menu re-entry
//! FUN_00443a10: after each step the ship node's q, the camera node's own
//! and world q, its parent, the pitching state and the read-back angles,
//! bit for bit, under the control words 0x027f and 0x0e7f.

use atlantis2_engine::camera::Camera;
use atlantis2_engine::Fpu;
use atlantis3_engine::tangage::{normalize_axis, Ship};

const GOLDEN: &str = include_str!("golden/tangage_2004.txt");

fn bits(s: &str) -> u32 {
    u32::from_str_radix(s, 16).expect("hex")
}

fn f(s: &str) -> f32 {
    f32::from_bits(bits(s))
}

/// `deg · π · 1/180` as the `/set` handler converts (the harness passes
/// the same float under both control words).
fn deg_rad(deg: i32) -> f32 {
    let fpu = Fpu::Double;
    let pi = f64::from(f32::from_bits(0x4049_0fdb));
    let k = f64::from(f32::from_bits(0x3bb6_0b61));
    fpu.f32(fpu.mul(fpu.mul(f64::from(deg), pi), k))
}

fn q_hex(q: [f32; 4]) -> String {
    q.iter()
        .map(|x| format!("{:08x}", x.to_bits()))
        .collect::<Vec<_>>()
        .join(",")
}

/// The golden line's fields of the model's state.
fn state(ship: &Ship, cam: &Camera) -> Vec<(String, String)> {
    let local = ship.cam.unwrap_or(cam.q);
    let (yaw, pitch) = cam.angles();
    let parent = if ship.cam.is_some() { "ship" } else { "root" };
    vec![
        ("ship".into(), q_hex(ship.q)),
        ("cam".into(), q_hex(local)),
        ("world".into(), q_hex(cam.q)),
        ("parent".into(), parent.into()),
        ("on".into(), u8::from(ship.on).to_string()),
        ("phase".into(), format!("{:08x}", ship.phase.to_bits())),
        ("amp".into(), format!("{:08x}", ship.amp.to_bits())),
        ("dir".into(), u8::from(ship.up).to_string()),
        ("damp".into(), ship.damp.to_string()),
        ("speed".into(), format!("{:08x}", ship.speed.to_bits())),
        ("prev".into(), format!("{:08x}", ship.saved_amp.to_bits())),
        (
            "angles".into(),
            format!("{:08x},{:08x}", yaw.to_bits(), pitch.to_bits()),
        ),
    ]
}

fn apply(ship: &mut Ship, cam: &mut Camera, step: &str) {
    let p: Vec<&str> = step.split(':').collect();
    let int = |i: usize| p[i].parse::<i32>().expect("int");
    match p[0] {
        "set" => {
            let axis = normalize_axis(cam.fpu, [int(1) as f32, int(2) as f32, int(3) as f32]);
            ship.set(cam.fpu, axis, int(4), int(5), int(6));
        }
        "stop" => ship.stop(),
        "enter" => ship.enter(cam, deg_rad(int(1)), deg_rad(int(2)), int(3) != 0),
        "reenter" => ship.reenter(cam),
        "step" => ship.step(cam, f(p[1])),
        "yaw" => ship.yaw(cam, f(p[1])),
        "pitch" => ship.pitch(cam, f(p[1])),
        "setyaw" => ship.set_yaw(cam, f(p[1])),
        "setpitch" => ship.set_pitch(cam, f(p[1])),
        s => panic!("unknown step {s}"),
    }
}

#[test]
fn the_original_s_camera_and_pitching() {
    let mut run: Option<(Ship, Camera, String)> = None;
    let (mut checked, mut wrong) = (0, Vec::new());
    for line in GOLDEN.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(h) = line.strip_prefix("== ") {
            let fpu = if h.ends_with("cw=0e7f") {
                Fpu::Chop
            } else {
                Fpu::Double
            };
            run = Some((Ship::default(), Camera::new(fpu, 800, 600), h.to_string()));
            continue;
        }
        let (ship, cam, name) = run.as_mut().expect("a run header first");
        let mut words = line.split(' ');
        let tag = words.next().unwrap();
        if tag != "init" {
            apply(ship, cam, words.next().unwrap());
        } else {
            words = line.split(' ');
            words.next();
        }
        let got = state(ship, cam);
        for w in line.split(' ').skip(if tag == "init" { 1 } else { 2 }) {
            let (k, v) = w.split_once('=').expect("key=value");
            let mine = &got.iter().find(|(g, _)| g == k).expect("field").1;
            checked += 1;
            if mine != v {
                wrong.push(format!("{name} {line:.24}: {k} {mine} != {v}"));
            }
        }
    }
    assert!(checked > 4000, "{checked} fields");
    assert!(
        wrong.is_empty(),
        "{} of {checked} fields differ:\n{}",
        wrong.len(),
        wrong[..wrong.len().min(40)].join("\n")
    );
}

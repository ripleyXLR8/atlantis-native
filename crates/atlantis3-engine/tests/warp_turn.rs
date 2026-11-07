//! Mouse turns of the warp against the original under emulation
//! (`tests/golden/warp_turn_*_v2.txt`): from the first Hoggar view and
//! the funeral chamber's boot view, with the start-up's limits (70, −1),
//! mouse moves (the callback @ 0x004313a0: speed += dx / 2000, −= dy /
//! 2000, += with `InvertMouseY`), frames of the 25 Hz warp step
//! (@ 0x004317a0: the clock's accumulator, then per tick speeds × 0.8, yaw
//! and pitch turns, the limits, the 76° field of view), limit changes and
//! view entries (the camera part of @ 0x0043f840 from the turned camera,
//! the speeds stopped). After each frame or entry: the clock, the ticks,
//! the speeds, the read-back angles, the projection and the matrix bit for
//! bit, then the bare cube's frame by MD5 and, where no place sprite is
//! drawn, the whole frame. The first frame runs under the control word
//! 0x027f, the rest under 0x0e7f (the renderer leaves it).
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use atlantis2_engine::look::Look;
use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::warp::A3Raster;
use atlantis2_engine::{Camera, Fpu, Renderer, Variant};
use atlantis3_data::A3Files;
use atlantis3_engine::game::{dialog_fov, faces_from_hnm};
use md5::{Digest, Md5};

const HOGGAR: &str = include_str!("golden/warp_turn_hoggar_v2.txt");
const CFUNER: &str = include_str!("golden/warp_turn_cfuner_v2.txt");

const W: u32 = 800;
const H: u32 = 600;

fn md5_hex(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The bytes of a golden value written as LE hex.
fn le_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn le32(hex: &str) -> u32 {
    u32::from_le_bytes(le_bytes(hex).try_into().unwrap())
}

fn le32s(list: &str) -> Vec<u32> {
    list.split(',').map(le32).collect()
}

/// The frame of the cube at the camera, as the warp draws it (warpBFPII,
/// smooth), and whether a bilinear sample read past a face (the original
/// reads the heap there, see `tests/warp.rs`).
fn render(faces: &[Vec<u16>; 6], cam: &Camera) -> (String, bool, Fpu) {
    let mut r = Renderer::new(Variant::PII, cam.fpu, W as usize, H as usize);
    r.a3 = Some(A3Raster { smooth: true });
    let mut out = vec![0u16; (W * H) as usize];
    r.render(faces, &cam.matrix(), cam.projection(), &mut out);
    (md5_hex(&out), r.out_of_face > 0, r.fpu)
}

/// Replays one golden; returns (fields checked, frames compared, frames
/// that differ where they read past a face).
fn replay(text: &str, faces: &[Vec<u16>; 6], sprites: bool) -> (usize, usize, usize) {
    let mut cam = Camera::new(Fpu::Double, W, H);
    let mut look = Look::default();
    // The view entry resets the turn timer at clock 100.0.
    let (mut clock, mut last, mut acc) = (100.0f64, 100.0f64, 0f32);
    let mut ticks = 0;
    let (mut fields, mut frames, mut loose) = (0, 0, 0);
    let mut first = true;
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let (key, value) = line.split_once(' ').unwrap();
        match key {
            "turn.limits" => {
                assert_eq!(value, format!("{},{}", look.limit_y, look.limit_x));
                continue;
            }
            "turn.sensitivity" => {
                assert_eq!(value, look.sensitivity.to_string());
                continue;
            }
            _ => {}
        }
        let (step, field) = match key.rsplit_once('.') {
            Some((s, f)) if s.contains('.') => (s, Some(f)),
            _ => (key, None),
        };
        let Some(field) = field else {
            // A step.
            let f: Vec<&str> = value.split(':').collect();
            let int = |i: usize| -> i32 { f[i].parse().unwrap() };
            match f[0] {
                "enter" => {
                    let fpu = cam.fpu;
                    cam.enter(deg_to_rad_a3(fpu, int(1)), deg_to_rad_a3(fpu, int(2)));
                    look.stop();
                    last = clock;
                }
                "mouse" => {
                    look.invert_y = int(3) != 0;
                    look.mouse(cam.fpu, int(1), int(2));
                }
                "limits" => {
                    look.limit_y = int(1);
                    look.limit_x = int(2);
                }
                "frame" => {
                    // @ 0x004317a0: dt (at most 0.5 s) × 25 into the
                    // accumulator, a tick per whole unit.
                    clock += 0.04;
                    let fpu = cam.fpu;
                    let dt = (clock - last).min(0.5);
                    last = clock;
                    acc = fpu.f32(fpu.add(f64::from(acc), fpu.mul(dt, 25.0)));
                    ticks = 0;
                    while acc >= 1.0 {
                        acc -= 1.0;
                        look.frame(&mut cam);
                        cam.set_fov(dialog_fov(cam.fpu, 76.0), W, H);
                        ticks += 1;
                    }
                }
                _ => panic!("{line}"),
            }
            continue;
        };
        let what = format!("{step} {field}");
        match field {
            "clock" => assert_eq!(
                clock.to_bits(),
                u64::from_le_bytes(le_bytes(value).try_into().unwrap()),
                "{what}"
            ),
            "ticks" => assert_eq!(ticks.to_string(), value, "{what}"),
            "speed" => assert_eq!(
                vec![look.yaw_speed.to_bits(), look.pitch_speed.to_bits()],
                le32s(value),
                "{what}"
            ),
            "angles" => {
                let (y, p) = cam.angles();
                assert_eq!(vec![y.to_bits(), p.to_bits()], le32s(value), "{what}");
            }
            "proj" => assert_eq!(
                cam.projection().map(f32::to_bits).to_vec(),
                le32s(value),
                "{what}"
            ),
            "matrix" => assert_eq!(
                cam.matrix().map(f32::to_bits).to_vec(),
                le32s(value),
                "{what}"
            ),
            "md5" => {
                // The whole frame: the cube alone where no place sprite
                // is drawn. The first frame leaves 0x0e7f.
                let (md5, past, fpu) = render(faces, &cam);
                if first {
                    cam.fpu = fpu;
                    first = false;
                }
                if !sprites {
                    frames += 1;
                    if md5 != value {
                        assert!(past, "{what}");
                        loose += 1;
                    }
                }
            }
            "cube" => {
                let (md5, past, _) = render(faces, &cam);
                frames += 1;
                if md5 != value {
                    assert!(past, "{what}");
                    loose += 1;
                }
            }
            _ => panic!("{line}"),
        }
        fields += 1;
    }
    (fields, frames, loose)
}

fn faces(files: &A3Files, path: &str) -> [Vec<u16>; 6] {
    faces_from_hnm(&files.read(path).unwrap()).unwrap()
}

#[test]
fn mouse_turns_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    // Hoggar view 4 has no place sprite: every whole frame is the cube's.
    let ho = faces(&files, "warp/Warp512/hoggar/ho0004.hnm");
    let (n, f, loose) = replay(HOGGAR, &ho, false);
    assert_eq!((n, f), (52 * 7 + 49, 104), "hoggar");
    if loose > 0 {
        eprintln!("hoggar: {loose} of {f} frames differ where they read past a face");
    }
    // The funeral chamber's view 4 draws its place sprites: the bare cube
    // only.
    let cf = faces(&files, "warp/Warp512/ldm/cf0004.hnm");
    let (n, f, loose) = replay(CFUNER, &cf, true);
    assert_eq!((n, f), (52 * 7 + 49, 52), "cfuner");
    if loose > 0 {
        eprintln!("cfuner: {loose} of {f} frames differ where they read past a face");
    }
}

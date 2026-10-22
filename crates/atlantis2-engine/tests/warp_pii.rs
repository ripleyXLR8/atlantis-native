//! `warpPII`, the reference renderer, against frames drawn by the original
//! (`tests/golden/warp_cube_pii.txt`, `warp_cube_pii_uv.txt`, and the
//! `warpPII` set of `warp_turn.txt`): every golden frame is drawn with the
//! port's own camera in the golden's x87 state and compared by MD5, the
//! camera matrix bit for bit.
//!
//! The synthetic `uv` frames need no game data; the others need
//! `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see `tests/warp.rs`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::camera::{deg_to_rad, WARP_FOV};
use atlantis2_engine::look::Look;
use atlantis2_engine::warp::{faces_from_hnm, PII_MAX_WIDTH};
use atlantis2_engine::{Camera, Engine, Fpu, Renderer, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/warp_cube_pii.txt");
const GOLDEN_UV: &str = include_str!("golden/warp_cube_pii_uv.txt");
const TURN: &str = include_str!("golden/warp_turn.txt");

fn md5_hex(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The x87 state of a golden frame.
#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    /// Entry and frame under 0x0e7f (an assembly frame was drawn before).
    Steady,
    /// Entry under the process's control word, the frame switching to
    /// 0x0e7f at its first polygon.
    First(Fpu),
}

struct Row {
    state: State,
    w: u32,
    h: u32,
    fov: u32,
    file: &'static str,
    yaw: &'static str,
    pitch: &'static str,
    md5: &'static str,
    matrix: [u32; 9],
}

fn rows(text: &'static str) -> (Vec<Row>, BTreeMap<(&'static str, usize), &'static str>) {
    let mut rows = Vec::new();
    let mut faces = BTreeMap::new();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f[0] == "face" {
            faces.insert((f[1], f[2].parse().unwrap()), f[3]);
            continue;
        }
        let (w, h) = f[1].split_once('x').unwrap();
        let matrix: Vec<u32> = f[7]
            .split(',')
            .map(|b| u32::from_le_bytes(u32::from_str_radix(b, 16).unwrap().to_be_bytes()))
            .collect();
        rows.push(Row {
            state: match f[0] {
                "steady0e7f" => State::Steady,
                "first027f" => State::First(Fpu::Double),
                "first007f" => State::First(Fpu::Single),
                s => panic!("state {s}"),
            },
            w: w.parse().unwrap(),
            h: h.parse().unwrap(),
            fov: f[2].parse().unwrap(),
            file: f[3],
            yaw: f[4],
            pitch: f[5],
            md5: f[6],
            matrix: matrix.try_into().unwrap(),
        });
    }
    (rows, faces)
}

/// The camera of a golden row: the warp's initialisation at start-up
/// (under 0x027f), then the view entry at the row's angles (integers
/// through the `/set` conversion, decimals as float radians, as the
/// harness gave them; the rotations and the projection) under the state's
/// control word, then the zoom's projection for a FOV other than 76°.
fn camera(row: &Row) -> Camera {
    let fpu = match row.state {
        State::Steady => Fpu::Chop,
        State::First(cw) => cw,
    };
    let mut cam = Camera::new(Fpu::Double, row.w, row.h);
    cam.fpu = fpu;
    let angle = |s: &str| -> f32 {
        if s.contains('.') {
            s.parse::<f64>().unwrap().to_radians() as f32
        } else {
            deg_to_rad(fpu, s.parse().unwrap())
        }
    };
    cam.enter(angle(row.yaw), angle(row.pitch));
    cam.set_fov(WARP_FOV, row.w, row.h);
    if row.fov != 76 {
        // The harness's radians: deg * 3.1415927 * 0.0055555557 in doubles
        // (the game's float pi and 1/180).
        #[allow(clippy::approx_constant)]
        let fov = (f64::from(row.fov) * 3.1415927 * 0.0055555557) as f32;
        cam.set_fov(fov, row.w, row.h);
    }
    cam
}

/// Draws every row of `file` with `faces`; returns (frames, matrices that
/// differ, frames that differ).
fn check(
    text: &'static str,
    file: &str,
    faces: &[Vec<u16>; 6],
) -> (usize, Vec<String>, Vec<String>) {
    let (rows, _) = rows(text);
    let (mut n, mut bad_m, mut bad_f) = (0, Vec::new(), Vec::new());
    for row in rows.iter().filter(|r| r.file == file) {
        let cam = camera(row);
        let key = format!(
            "{:?} {}x{} fov{} {} y{} p{}",
            row.state, row.w, row.h, row.fov, row.file, row.yaw, row.pitch
        );
        let m = cam.matrix();
        if m.map(f32::to_bits) != row.matrix {
            bad_m.push(key.clone());
        }
        let mut r = Renderer::new(Variant::PII, cam.fpu, row.w as usize, row.h as usize);
        let mut out = vec![0xf81fu16; (row.w * row.h) as usize];
        r.render(faces, &m, cam.projection(), &mut out);
        assert_eq!(r.fpu, Fpu::Chop, "{key}: the renderer leaves 0x0e7f");
        if md5_hex(&out) != row.md5 {
            bad_f.push(key);
        }
        n += 1;
    }
    (n, bad_m, bad_f)
}

fn assert_clean(what: &str, bad_m: &[String], bad_f: &[String]) {
    assert!(
        bad_m.is_empty(),
        "{what}: {} matrices differ: {:?}",
        bad_m.len(),
        &bad_m[..bad_m.len().min(4)]
    );
    assert!(
        bad_f.is_empty(),
        "{what}: {} frames differ: {:?}",
        bad_f.len(),
        &bad_f[..bad_f.len().min(4)]
    );
}

/// Synthetic faces: texel (u, v) = ((v & 0x7f) << 9) | u on every face, so
/// each pixel tells the texel it sampled; both x87 states.
#[test]
fn synthetic_faces_match_the_original() {
    let face: Vec<u16> = (0..512 * 512)
        .map(|i| ((((i / 512) & 0x7f) << 9) | (i % 512)) as u16)
        .collect();
    let faces: [Vec<u16>; 6] = std::array::from_fn(|_| face.clone());
    let (n, bad_m, bad_f) = check(GOLDEN_UV, "uv", &faces);
    assert_eq!(n, 2 * 168);
    assert_clean("uv", &bad_m, &bad_f);
}

/// A screen wider than the original's span buffer allows is refused.
#[test]
#[should_panic(expected = "warpPII draws at most")]
fn a_wider_screen_is_refused() {
    let faces: [Vec<u16>; 6] = std::array::from_fn(|_| vec![0; 512 * 512]);
    let w = PII_MAX_WIDTH + 1;
    let cam = Camera::new(Fpu::Chop, w as u32, 8);
    let mut r = Renderer::new(Variant::PII, Fpu::Chop, w, 8);
    let mut out = vec![0u16; w * 8];
    r.render(&faces, &cam.matrix(), cam.projection(), &mut out);
}

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA or ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// The panoramas of the first view (`ti0002.hnm`) and of Ireland's first
/// (`ir0001.hnm`) at every golden angle, size, field of view and x87 state.
#[test]
fn panoramas_match_the_original() {
    let Some(files) = game() else { return };
    let (_, face_md5) = rows(GOLDEN);
    let mut total = 0;
    for file in ["ti0002.hnm", "ir0001.hnm"] {
        let hnm = files.read(Folder::Warp, file).expect(file);
        let faces = faces_from_hnm(&hnm).unwrap();
        for (i, f) in faces.iter().enumerate() {
            assert_eq!(md5_hex(f), face_md5[&(file, i)], "{file} face {i}");
        }
        let (n, bad_m, bad_f) = check(GOLDEN, file, &faces);
        assert_clean(file, &bad_m, &bad_f);
        total += n;
    }
    // In the steady state: the full grid (168 angles) and 60 random
    // fractional angles on both files, the full grid at 800 x 600 and 15
    // angles at 35° on ti0002; the first view's grid (12 entries) in the
    // three states on ti0002, its y0 p0 steady row being in the full grid
    // already; 7 more steady angles on ti0002 (yaw 37 and pitch 33).
    assert_eq!(total, 2 * (168 + 60) + 168 + 15 + 3 * 12 - 1 + 7);
}

/// A new game opens on the first view as the original draws it when no
/// assembly frame was drawn before (state first027f), and leaves the
/// control word 0x0e7f for what follows.
#[test]
fn new_game_shows_the_first_view() {
    let Some(files) = game() else { return };
    let (rows, _) = rows(GOLDEN);
    let find = |state| {
        rows.iter()
            .find(|r| {
                r.state == state && r.file == "ti0002.hnm" && r.yaw == "-90" && r.pitch == "0"
            })
            .unwrap()
            .md5
    };
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Double, 640, 480).unwrap();
    assert_eq!(e.variant(), Variant::PII);
    assert_eq!(e.fpu(), Fpu::Double);
    let mut out = vec![0u16; 640 * 480];
    e.render(&mut out);
    assert_eq!(md5_hex(&out), find(State::First(Fpu::Double)));
    assert_eq!(e.fpu(), Fpu::Chop);
}

/// Mouse turns frame by frame from the first view, then view entries from
/// the turned camera, all under 0x0e7f (`warp_turn.txt`, set
/// `turn.warpPII.steady0e7f`): speeds, angles and matrix bit for bit after
/// every frame, and the frame's MD5 when the game is there.
#[test]
fn turns_match_the_original() {
    let faces = game().map(|files| {
        let hnm = files.read(Folder::Warp, "ti0002.hnm").expect("ti0002.hnm");
        faces_from_hnm(&hnm).unwrap()
    });
    let prefix = "turn.warpPII.steady0e7f.";
    let fpu = Fpu::Chop;
    let mut cam = Camera::new(Fpu::Double, 640, 480);
    cam.fpu = fpu;
    let mut look = Look::default();
    let mut r = Renderer::new(Variant::PII, fpu, 640, 480);
    let mut out = vec![0xf81fu16; 640 * 480];
    let (mut steps, mut checked, mut frames) = (0, 0, 0);
    for line in TURN.lines().filter(|l| l.starts_with(prefix)) {
        let (key, value) = line.split_once(' ').unwrap();
        let rest = &key[prefix.len()..];
        let bits = |v: &str| -> Vec<u32> {
            v.split(',')
                .map(|b| u32::from_str_radix(b, 16).unwrap())
                .collect()
        };
        match rest.split_once('.') {
            None => {
                steps += 1;
                let f: Vec<&str> = value.split(':').collect();
                let n = |i: usize| f[i].parse::<i32>().unwrap();
                match f[0] {
                    "enter0" | "enter" => {
                        cam.enter(deg_to_rad(fpu, n(1)), deg_to_rad(fpu, n(2)));
                        cam.set_fov(WARP_FOV, 640, 480);
                        look.stop();
                    }
                    "mouse" => {
                        look.invert_y = n(3) != 0;
                        look.mouse(fpu, n(1), n(2));
                    }
                    "limits" => (look.limit_y, look.limit_x) = (n(1), n(2)),
                    "frame" => look.frame(&mut cam),
                    s => panic!("step {s}"),
                }
            }
            Some((n, "speed")) => assert_eq!(
                vec![look.yaw_speed.to_bits(), look.pitch_speed.to_bits()],
                bits(value),
                "{n} speed"
            ),
            Some((n, "angles")) => {
                let (y, p) = cam.angles();
                assert_eq!(vec![y.to_bits(), p.to_bits()], bits(value), "{n} angles");
            }
            Some((n, "matrix")) => {
                assert_eq!(
                    cam.matrix().map(f32::to_bits).to_vec(),
                    bits(value),
                    "{n} matrix"
                );
                checked += 1;
            }
            Some((n, "md5")) => {
                if let Some(faces) = &faces {
                    r.render(faces, &cam.matrix(), cam.projection(), &mut out);
                    assert_eq!(md5_hex(&out), value, "{n} frame");
                    frames += 1;
                }
            }
            Some((_, k)) => panic!("key {k}"),
        }
    }
    assert_eq!(steps, 121);
    // 65 frames and 4 view entries.
    assert_eq!(checked, 69);
    assert!(frames == 0 || frames == 69);
}

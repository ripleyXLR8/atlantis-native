//! The cube renderer and its camera against frames drawn by the original
//! (`tests/golden/warp_cube.txt`): every golden frame is drawn with the
//! port's own camera matrix and compared by MD5, and the matrix itself is
//! compared bit for bit.
//!
//! The synthetic `uv` frames need no game data. The others need the game:
//! `ATLANTIS2_DATA` = the installed folder, `ATLANTIS2_DISCS` = a folder
//! holding the four CD images (or copies of the CDs); without them those
//! tests are skipped.

use std::collections::BTreeMap;
use std::path::PathBuf;

use atlantis2_data::con::NewGame;
use atlantis2_data::wam::{pick, Wam};
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::camera::deg_to_rad;
use atlantis2_engine::warp::faces_from_hnm;
use atlantis2_engine::{Camera, Engine, Fpu, Renderer};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/warp_cube.txt");

fn md5_hex(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Row {
    fpu: Fpu,
    w: u32,
    h: u32,
    fov: u32,
    file: &'static str,
    yaw: &'static str,
    pitch: &'static str,
    md5: &'static str,
    matrix: [u32; 9],
}

fn rows() -> (Vec<Row>, BTreeMap<(&'static str, usize), &'static str>) {
    let mut rows = Vec::new();
    let mut faces = BTreeMap::new();
    for line in GOLDEN
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
            fpu: match f[0] {
                "027f" => Fpu::Double,
                "007f" => Fpu::Single,
                cw => panic!("control word {cw}"),
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

/// The camera of a golden row: the projection, then the view entry at the
/// row's angles (integers through the `/set` conversion, decimals as
/// float radians, as the harness gave them).
fn camera(row: &Row) -> Camera {
    let mut cam = Camera::new(row.fpu, row.w, row.h);
    if row.fov != 76 {
        // The harness's radians: deg * 3.1415927 * 0.0055555557 in doubles
        // (the game's float pi and 1/180).
        #[allow(clippy::approx_constant)]
        let fov = (f64::from(row.fov) * 3.1415927 * 0.0055555557) as f32;
        cam.set_fov(fov, row.w, row.h);
    }
    let angle = |s: &str| -> f32 {
        if s.contains('.') {
            s.parse::<f64>().unwrap().to_radians() as f32
        } else {
            deg_to_rad(row.fpu, s.parse().unwrap())
        }
    };
    cam.enter(angle(row.yaw), angle(row.pitch));
    cam
}

/// Draws every row of `file` with `faces`; returns (frames, matrices that
/// differ, frames that differ).
fn check(file: &str, faces: &[Vec<u16>; 6]) -> (usize, Vec<String>, Vec<String>) {
    let (rows, _) = rows();
    let (mut n, mut bad_m, mut bad_f) = (0, Vec::new(), Vec::new());
    for row in rows.iter().filter(|r| r.file == file) {
        let cam = camera(row);
        let key = format!(
            "{:04x} {}x{} fov{} {} y{} p{}",
            row.fpu.control_word(),
            row.w,
            row.h,
            row.fov,
            row.file,
            row.yaw,
            row.pitch
        );
        let m = cam.matrix();
        if m.map(f32::to_bits) != row.matrix {
            bad_m.push(key.clone());
        }
        let mut r = Renderer::new(row.fpu, row.w as usize, row.h as usize);
        let mut out = vec![0xf81fu16; (row.w * row.h) as usize];
        r.render(faces, &m, cam.projection(), &mut out);
        assert_eq!(r.out_of_face, 0, "{key}");
        if md5_hex(&out) != row.md5 {
            bad_f.push(key);
        }
        n += 1;
    }
    (n, bad_m, bad_f)
}

/// Synthetic faces: texel (u, v) = ((v & 0x7f) << 9) | u on every face.
#[test]
fn synthetic_faces_match_the_original() {
    let face: Vec<u16> = (0..512 * 512)
        .map(|i| ((((i / 512) & 0x7f) << 9) | (i % 512)) as u16)
        .collect();
    let faces: [Vec<u16>; 6] = std::array::from_fn(|_| face.clone());
    let (n, bad_m, bad_f) = check("uv", &faces);
    assert_eq!(n, 168);
    assert!(
        bad_m.is_empty(),
        "{} matrices differ: {:?}",
        bad_m.len(),
        &bad_m[..bad_m.len().min(4)]
    );
    assert!(
        bad_f.is_empty(),
        "{} frames differ: {:?}",
        bad_f.len(),
        &bad_f[..bad_f.len().min(4)]
    );
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
/// (`ir0001.hnm`) at every golden angle, size, field of view and precision.
#[test]
fn panoramas_match_the_original() {
    let Some(files) = game() else { return };
    let (_, face_md5) = rows();
    let mut total = 0;
    for file in ["ti0002.hnm", "ir0001.hnm"] {
        let hnm = files.read(Folder::Warp, file).expect(file);
        let faces = faces_from_hnm(&hnm).unwrap();
        for (i, f) in faces.iter().enumerate() {
            assert_eq!(md5_hex(f), face_md5[&(file, i)], "{file} face {i}");
        }
        let (n, bad_m, bad_f) = check(file, &faces);
        assert!(
            bad_m.is_empty(),
            "{file}: {} matrices differ: {:?}",
            bad_m.len(),
            &bad_m[..bad_m.len().min(4)]
        );
        assert!(
            bad_f.is_empty(),
            "{file}: {} frames differ: {:?}",
            bad_f.len(),
            &bad_f[..bad_f.len().min(4)]
        );
        total += n;
    }
    // 168 x 2 (both precisions) + 60 random + 168 at 800x600 + 15 at 35°
    // + 12 x 2 of the first view, for ti0002; 168 x 2 + 60 for ir0001.
    assert_eq!(total, 997);
}

/// A new game opens on the first view as the original draws it, with the
/// move to view 3 under the centre.
#[test]
fn new_game_shows_the_first_view() {
    let (rows, _) = rows();
    for fpu in [Fpu::Double, Fpu::Single] {
        let Some(files) = game() else { return };
        let mut e = Engine::new_game(files, fpu, 640, 480).unwrap();
        assert_eq!(e.place(), ("tibet.wam", 2));
        assert_eq!(e.files().current_disc(), 1);
        let mut out = vec![0u16; 640 * 480];
        e.render(&mut out);
        let golden = rows
            .iter()
            .find(|r| {
                r.fpu == fpu
                    && r.file == "ti0002.hnm"
                    && r.yaw == "-90"
                    && r.pitch == "0"
                    && r.w == 640
            })
            .unwrap();
        assert_eq!(md5_hex(&out), golden.md5, "{fpu:?}");
        let (yaw, pitch) = e.angles_deg();
        assert!((yaw + 90.0).abs() < 1e-3 && pitch.abs() < 1e-3);
        let (i, zone) = e.centre_zone().expect("a zone under the centre");
        assert_eq!((i, zone.kind, zone.value), (0, 1, 3));
    }
}

/// The original's picks again, through the port's camera (the view entry
/// from the start-up camera, the unprojection and the matrix).
#[test]
fn picks_through_the_camera() {
    let Some(dir) = std::env::var_os("ATLANTIS2_DATA") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let w = Wam::parse(&std::fs::read(dir.join("wam/tibet.wam")).unwrap()).unwrap();
    let con = std::fs::read(dir.join("scenar/chapi000.con")).unwrap();
    let start =
        NewGame::from_chapter0(&con.iter().map(|&b| char::from(b)).collect::<String>()).unwrap();
    assert_eq!((start.view, start.yaw_deg, start.pitch_deg), (2, -90, 0));
    let golden = include_str!("../../atlantis2-data/tests/golden/wam_pick_tibet.txt");
    let (mut n, mut bad) = (0, Vec::new());
    for fpu in [Fpu::Single, Fpu::Double] {
        for line in golden
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with("aim"))
        {
            let f: Vec<i32> = line
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            let mut cam = Camera::new(fpu, 640, 480);
            cam.enter(deg_to_rad(fpu, f[1]), deg_to_rad(fpu, f[2]));
            let d = cam.ray(f[3] as f32, f[4] as f32).map(f64::from);
            let got = pick(&w.view(f[0] as u32).unwrap().zones, d).map_or(-1, |i| i as i32);
            if got != f[5] {
                bad.push(format!("{fpu:?} {line}"));
            }
            n += 1;
        }
    }
    assert_eq!(n, 2 * 10908);
    assert!(
        bad.is_empty(),
        "{} picks differ: {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

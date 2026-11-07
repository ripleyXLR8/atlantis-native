//! The panorama as *Atlantis III* draws it, against frames of its own
//! renderer under emulation (`tests/golden/warp_*.txt`): `warpBFPII`
//! (*Atlantis II*'s `warpPII` with the texture scaled by 2^30, the flat-top
//! rule and the bilinear span) and, without `Smooth`, `warpPII`; the camera
//! of *Atlantis II* with *Atlantis III*'s `/set` angle conversion; the
//! cube faces as the game's loader places them. Every row's camera matrix
//! is compared bit for bit, then the 800 x 600 frame by MD5.
//!
//! The synthetic faces need no game data; the others need
//! `ATLANTIS3_DATA`.

use std::collections::BTreeMap;

use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::warp::A3Raster;
use atlantis2_engine::{Camera, Fpu, Renderer, Variant};
use atlantis3_data::A3Files;
use atlantis3_engine::game::faces_from_hnm;
use md5::{Digest, Md5};

const UV: &str = include_str!("golden/warp_grid_uv.txt");
const UV_NOSMOOTH: &str = include_str!("golden/warp_grid_uv_nosmooth.txt");
const CF0004: &str = include_str!("golden/warp_grid_cf0004.txt");
const OTHERS: &str = include_str!("golden/warp_grid_others.txt");
const FIRST_HOGGAR: &str = include_str!("golden/warp_first_hoggar.txt");

fn md5_hex(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Row {
    steady: bool,
    file: String,
    yaw: String,
    pitch: String,
    md5: String,
    matrix: [u32; 9],
}

/// The frame rows (grid rows, or the `full` rows of the first-view files)
/// and the face MD5s by (file, face).
fn rows(text: &str) -> (Vec<Row>, BTreeMap<(String, usize), String>) {
    let mut rows = Vec::new();
    let mut faces = BTreeMap::new();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let mut f: Vec<&str> = line.split_whitespace().collect();
        if f[0] == "face" {
            faces.insert((f[1].to_string(), f[2].parse().unwrap()), f[3].to_string());
            continue;
        }
        if f[0] == "full" {
            f.remove(0);
        }
        let steady = match f[0] {
            "steady0e7f" => true,
            "first027f" => false,
            _ => continue,
        };
        let matrix: Vec<u32> = f[7]
            .split(',')
            .map(|b| u32::from_le_bytes(u32::from_str_radix(b, 16).unwrap().to_be_bytes()))
            .collect();
        rows.push(Row {
            steady,
            file: f[3].to_string(),
            yaw: f[4].to_string(),
            pitch: f[5].to_string(),
            md5: f[6].to_string(),
            matrix: matrix.try_into().unwrap(),
        });
    }
    (rows, faces)
}

/// The camera of a row: the warp's initialisation (800 x 600, 76°), then
/// the view entry at the row's angles through the `/set` conversion, under
/// the row's control word.
fn camera(row: &Row) -> Camera {
    let fpu = if row.steady { Fpu::Chop } else { Fpu::Double };
    let mut cam = Camera::new(Fpu::Double, 800, 600);
    cam.fpu = fpu;
    let angle = |s: &str| -> f32 {
        if s.contains('.') {
            s.parse::<f64>().unwrap().to_radians() as f32
        } else {
            deg_to_rad_a3(fpu, s.parse().unwrap())
        }
    };
    cam.enter(angle(&row.yaw), angle(&row.pitch));
    cam
}

/// (rows drawn, matrices that differ, frames that differ). A frame whose
/// bilinear samples read past a face's last row (`Renderer::out_of_face`)
/// may differ: the original reads the heap beyond the face's buffer there,
/// which the emulation's heap filled with its own values. Those frames are
/// counted in `LOOSE` and printed.
fn check(
    text: &str,
    file: &str,
    faces: &[Vec<u16>; 6],
    smooth: bool,
) -> (usize, Vec<String>, Vec<String>) {
    let (rows, _) = rows(text);
    let (mut n, mut bad_m, mut bad_f) = (0, Vec::new(), Vec::new());
    let mut loose = 0;
    for row in rows.iter().filter(|r| r.file == file) {
        let cam = camera(row);
        let key = format!("{} {} y{} p{}", row.steady, row.file, row.yaw, row.pitch);
        let m = cam.matrix();
        if m.map(f32::to_bits) != row.matrix {
            bad_m.push(key.clone());
        }
        let mut r = Renderer::new(Variant::PII, cam.fpu, 800, 600);
        r.a3 = Some(A3Raster { smooth });
        let mut out = vec![0xf81fu16; 800 * 600];
        r.render(faces, &m, cam.projection(), &mut out);
        if md5_hex(&out) != row.md5 {
            if r.out_of_face > 0 {
                loose += 1;
            } else {
                bad_f.push(key);
            }
        }
        n += 1;
    }
    if loose > 0 {
        eprintln!("{file}: {loose} of {n} frames differ where they read past a face");
    }
    (n, bad_m, bad_f)
}

fn assert_clean(what: &str, n: usize, bad_m: &[String], bad_f: &[String]) {
    assert!(
        bad_m.is_empty() && bad_f.is_empty(),
        "{what}: of {n}, {} matrices differ {:?}, {} frames differ {:?}",
        bad_m.len(),
        &bad_m[..bad_m.len().min(4)],
        bad_f.len(),
        &bad_f[..bad_f.len().min(4)]
    );
}

fn uv_faces() -> [Vec<u16>; 6] {
    let face: Vec<u16> = (0..512 * 512)
        .map(|i| ((((i / 512) & 0x7f) << 9) | (i % 512)) as u16)
        .collect();
    std::array::from_fn(|_| face.clone())
}

#[test]
fn synthetic_faces_without_smooth() {
    let (n, m, f) = check(UV_NOSMOOTH, "uv", &uv_faces(), false);
    assert_eq!(n, 336);
    assert_clean("uv warpPII", n, &m, &f);
}

#[test]
fn synthetic_faces_bilinear() {
    let (n, m, f) = check(UV, "uv", &uv_faces(), true);
    assert_eq!(n, 336);
    assert_clean("uv warpBFPII", n, &m, &f);
}

fn files() -> Option<A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(A3Files::open_dir(&dir).unwrap().0)
}

/// The faces of a cube by its file name, checked against the golden's.
fn faces(files: &A3Files, text: &str, name: &str) -> [Vec<u16>; 6] {
    let folder = match &name[..2] {
        "cf" => "ldm",
        "ho" => "hoggar",
        "pa" => "paleo",
        "ba" => "bagdad",
        _ => "inter",
    };
    let data = files
        .read(&format!("warp/Warp512/{folder}/{name}"))
        .unwrap();
    let faces = faces_from_hnm(&data).unwrap();
    let (_, want) = rows(text);
    for i in 0..6 {
        assert_eq!(
            &md5_hex(&faces[i]),
            &want[&(name.to_string(), i)],
            "{name} face {i}"
        );
    }
    faces
}

#[test]
fn the_cfuner_cube_on_a_grid() {
    let Some(files) = files() else { return };
    let f = faces(&files, CF0004, "cf0004.hnm");
    let (n, m, bad) = check(CF0004, "cf0004.hnm", &f, true);
    assert_eq!(n, 336);
    assert_clean("cf0004", n, &m, &bad);
}

#[test]
fn other_cubes() {
    let Some(files) = files() else { return };
    let mut total = 0;
    for name in ["cf0003.hnm", "ho0001.hnm", "pa0001.hnm"] {
        let f = faces(&files, OTHERS, name);
        let (n, m, bad) = check(OTHERS, name, &f, true);
        assert_clean(name, n, &m, &bad);
        total += n;
    }
    assert_eq!(total, 90);
}

/// The first interactive view of a new game: Hoggar view 4 (no place
/// sprite), at yaw 108° and the angles around it.
#[test]
fn the_first_hoggar_view() {
    let Some(files) = files() else { return };
    let f = faces(&files, FIRST_HOGGAR, "ho0004.hnm");
    let (n, m, bad) = check(FIRST_HOGGAR, "ho0004.hnm", &f, true);
    assert_eq!(n, 24);
    assert_clean("ho0004", n, &m, &bad);
}

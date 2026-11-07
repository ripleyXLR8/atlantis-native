//! The panorama on a 32-bit display against frames of the original's
//! renderer under emulation (`tests/golden/warp_x888_*.txt`): the faces
//! decoded in x8r8g8b8 (the warp init makes them in the display's format),
//! drawn by `warp32BFPII` (span @ 0x00539000: the 16-bit span's weights on
//! each 8-bit channel, the lower row clamped to the face's last row), the
//! camera rows and frames compared by MD5 of the 800 x 600 x8r8g8b8
//! screen. The CFuner boot cube on the full grid and the first Hoggar
//! view (no place sprite) with its 24 angles.
//!
//! Needs `ATLANTIS3_DATA`.

use std::collections::BTreeMap;

use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::warp::A3Raster;
use atlantis2_engine::{Camera, Fpu, Renderer, Variant};
use atlantis3_data::A3Files;
use atlantis3_engine::game::faces_from_hnm_px;
use md5::{Digest, Md5};

const CF0004: &str = include_str!("golden/warp_x888_grid_cf0004.txt");
const FIRST_HOGGAR: &str = include_str!("golden/warp_x888_first_hoggar.txt");

fn md5_hex(px: &[u32]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Row {
    steady: bool,
    yaw: String,
    pitch: String,
    md5: String,
    matrix: [u32; 9],
}

fn rows(text: &str) -> (Vec<Row>, BTreeMap<usize, String>) {
    let mut rows = Vec::new();
    let mut faces = BTreeMap::new();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let mut f: Vec<&str> = line.split_whitespace().collect();
        if f[0] == "face" {
            faces.insert(f[2].parse().unwrap(), f[3].to_string());
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
            yaw: f[4].to_string(),
            pitch: f[5].to_string(),
            md5: f[6].to_string(),
            matrix: matrix.try_into().unwrap(),
        });
    }
    (rows, faces)
}

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

/// (rows, frames that differ); every matrix must match.
fn check(text: &str, folder: &str, name: &str, files: &A3Files) -> (usize, Vec<String>) {
    let data = files
        .read(&format!("warp/Warp512/{folder}/{name}"))
        .unwrap();
    let faces = faces_from_hnm_px::<u32>(&data).unwrap();
    let (rows, want) = rows(text);
    for (i, face) in faces.iter().enumerate() {
        assert_eq!(&md5_hex(face), &want[&i], "{name} face {i}");
    }
    let mut bad = Vec::new();
    for row in &rows {
        let cam = camera(row);
        let m = cam.matrix();
        assert_eq!(
            m.map(f32::to_bits),
            row.matrix,
            "{name} {} {}",
            row.yaw,
            row.pitch
        );
        let mut r = Renderer::new(Variant::PII, cam.fpu, 800, 600);
        r.a3 = Some(A3Raster { smooth: true });
        let mut out = vec![0x5af8_1f5au32; 800 * 600];
        r.render(&faces, &m, cam.projection(), &mut out);
        if md5_hex(&out) != row.md5 {
            bad.push(format!(
                "{} y{} p{} (reads past a face: {})",
                row.steady, row.yaw, row.pitch, r.out_of_face
            ));
        }
    }
    (rows.len(), bad)
}

fn files() -> Option<A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(A3Files::open_dir(&dir).unwrap().0)
}

#[test]
fn the_cfuner_cube_at_32_bits() {
    let Some(files) = files() else { return };
    let (n, bad) = check(CF0004, "ldm", "cf0004.hnm", &files);
    assert_eq!(n, 336);
    assert!(
        bad.is_empty(),
        "{} of {n} differ: {:?}",
        bad.len(),
        &bad[..bad.len().min(6)]
    );
}

#[test]
fn the_first_hoggar_view_at_32_bits() {
    let Some(files) = files() else { return };
    let (n, bad) = check(FIRST_HOGGAR, "hoggar", "ho0004.hnm", &files);
    assert_eq!(n, 24);
    assert!(
        bad.is_empty(),
        "{} of {n} differ: {:?}",
        bad.len(),
        &bad[..bad.len().min(6)]
    );
}

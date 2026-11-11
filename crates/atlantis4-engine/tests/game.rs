//! The warp view against the installed game's data.
//!
//! Needs the game: `ATLANTIS4_DATA` = the installed folder (see
//! `atlantis4_data::testing`).

use std::path::{Path, PathBuf};

use atlantis4_data::anim::Anim;
use atlantis4_data::scene::Scene;
use atlantis4_data::wpa::Package;
use atlantis4_data::{A4Files, WarpPath};
use atlantis4_engine::camera::{Camera, FOV_HD};
use atlantis4_engine::cube::{block_of, Background, BLOCKS};
use atlantis4_engine::node::{first_warp_path, Node, FIRST_WARP_SCRIPT};
use atlantis4_engine::pick::{click, Target};
use atlantis4_engine::render::{render, Weights};
use md5::{Digest, Md5};

fn install() -> Option<PathBuf> {
    atlantis4_data::testing::install_dir()
}

/// The scene descriptions `3D/<zone>/<node>.xml`, sorted.
fn scenes(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut zones: Vec<_> = std::fs::read_dir(dir.join("3D"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    zones.sort();
    for z in zones {
        let mut xs: Vec<_> = std::fs::read_dir(&z)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")))
            .collect();
        xs.sort();
        out.extend(xs);
    }
    out
}

#[test]
fn every_exit_marker_is_in_one_of_its_blocks() {
    let Some(dir) = install() else { return };
    let (mut checked, mut inside, mut files) = (0, 0, 0);
    for p in scenes(&dir) {
        let s = Scene::parse(&std::fs::read(&p).unwrap()).unwrap();
        files += 1;
        for e in &s.exits {
            let (Some(m), false) = (e.markers.first(), e.blocks.is_empty()) else {
                continue;
            };
            checked += 1;
            let b = block_of(m.map(f64::from), BLOCKS).unwrap();
            if e.blocks
                .iter()
                .any(|k| (k.face as usize, k.x, k.y) == (b.0, b.1, b.2))
            {
                inside += 1;
            }
        }
    }
    assert_eq!((files, checked, inside), (398, 842, 842));
}

/// A JPEG's width and height from its frame header.
fn jpeg_size(j: &[u8]) -> (u32, u32) {
    let mut o = 2;
    while o + 9 < j.len() {
        let m = j[o + 1];
        let len = usize::from(u16::from_be_bytes([j[o + 2], j[o + 3]]));
        if matches!(m, 0xc0..=0xc2) {
            let h = u16::from_be_bytes([j[o + 5], j[o + 6]]);
            let w = u16::from_be_bytes([j[o + 7], j[o + 8]]);
            return (u32::from(w), u32::from(h));
        }
        o += 2 + len;
    }
    panic!("no frame header")
}

#[test]
fn every_background_is_1014_cells_of_60_texels_in_64_texel_slots() {
    let Some(dir) = install() else { return };
    let files = A4Files::new(std::sync::Arc::new(atlantis_data::StdDir::new(&dir)));
    let mut n = 0;
    for p in scenes(&dir) {
        let rel = p
            .strip_prefix(&dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let w = WarpPath::parse(rel.strip_suffix(".xml").unwrap()).unwrap();
        let name = w.package("background");
        let src = files.source();
        let Some(pkg) = Package::open(src, &name) else {
            continue;
        };
        let pkg = pkg.unwrap();
        let read = |e: &str| pkg.read(src, &name, pkg.entry(e).unwrap()).unwrap();
        let anim = Anim::parse(&read("anim.anim")).unwrap();
        let sizes: Vec<(u32, u32)> = anim
            .tiles
            .iter()
            .map(|t| jpeg_size(&read(&t.name)))
            .collect();
        Background::from_anim(&anim, &sizes).unwrap_or_else(|e| panic!("{name}: {e}"));
        n += 1;
    }
    assert_eq!(n, 398);
}

fn md5_hex(px: &[u32]) -> String {
    let mut h = Md5::new();
    for p in px {
        h.update(p.to_le_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The first node at a few views of a 1365 × 768 window (60°): the hashes
/// of the XRGB8888 pixels (rows top-down, little-endian words), with the
/// specification's bilinear weights.
#[test]
fn the_first_node_renders_its_views() {
    let Some(dir) = install() else { return };
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    let first = first_warp_path(&files.read(FIRST_WARP_SCRIPT).unwrap()).unwrap();
    assert_eq!(first, "3D/Bateau_CA/0001.wp");
    let node = Node::load(&files, &WarpPath::parse(&first).unwrap()).unwrap();
    assert_eq!(node.exits.len(), 2);
    let face_hashes: Vec<String> = node.faces.iter().map(|f| md5_hex(f)).collect();
    let views = [
        (0.0, 0.0),
        (90.0, 0.0),
        (180.0, 0.0),
        (-90.0, 0.0),
        (30.0, 45.0),
        (200.0, -55.0),
    ];
    let mut got = Vec::new();
    for (yaw, pitch) in views {
        let cam = Camera::new(FOV_HD, 1365, 768, yaw, pitch);
        let mut out = vec![0u32; 1365 * 768];
        render(&cam, &node.faces, &mut out, Weights::Exact);
        got.push(md5_hex(&out));
    }
    let want_faces: [&str; 6] = FACE_HASHES;
    let want_views: [&str; 6] = VIEW_HASHES;
    assert_eq!(face_hashes, want_faces, "faces");
    assert_eq!(got, want_views, "views");
    // A click ahead and low at yaw 0 takes the exit to the next node.
    let cam = Camera::new(FOV_HD, 1365, 768, 0.0, -30.0);
    assert_eq!(
        click(&cam, 682, 384, &[], &node.exits),
        Some(Target::Exit("Bateau_CA\\0002.wp".into()))
    );
}

const FACE_HASHES: [&str; 6] = [
    "34a28de75ac9d6ed031c780916d797f7",
    "0afb26d5a36ad8a69d46049381fa0aa5",
    "97699f6b4afc389c5f172c235679cb09",
    "c4318e2334b8e36da7591b8835856dfc",
    "ccab00790cc4c48070c67bc3c8f5bf7d",
    "a7984221dc929be8a934b366cf9c148f",
];
const VIEW_HASHES: [&str; 6] = [
    "1a4f2d84f4c4e5a2954cdbb505a7e5ce",
    "a6dd8b68da9798c99e7b97a7fc304d9f",
    "6dea9d4ead7f6faffa7b97ea59f62b5e",
    "1e885b48d8ba6f7606fd09613f4d15f9",
    "830baa4cc5143d4278763ef0856e4ff5",
    "dac6360f046569e4694f9f5f9f1593ff",
];

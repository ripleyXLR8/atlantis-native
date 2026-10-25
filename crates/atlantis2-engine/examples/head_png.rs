//! Draws a dialog head as the original does and writes it to a PNG.
//!
//! ```text
//! ATLANTIS2_DATA=<install> ATLANTIS2_DISCS=<disc images> \
//!     cargo run --release --example head_png -- H5s1.db 20 0 o out.png
//! ```
//!
//! Arguments: the head's `.db`, the body-video frame, the camera, the
//! mouth (a viseme `d i a o b s v`, or `idle`), the output file, and
//! optionally `c` for the C routines under 0x027f instead of the Pentium
//! II's under 0x0e7f. The head is drawn over a grey background (the
//! original draws it over the body video and the panorama).

use std::path::PathBuf;

use atlantis2_data::db::Scene;
use atlantis2_data::tga::Tga;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::head::{Head, VISEMES};
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::raster3d::{HEIGHT, WIDTH};
use atlantis2_engine::{Fpu, Variant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize, d: &str| args.get(i).cloned().unwrap_or_else(|| d.to_string());
    let (db, frame, cam, mouth, out) = (
        arg(0, "H5s1.db"),
        arg(1, "1").parse::<i32>()?,
        arg(2, "0").parse::<usize>()?,
        arg(3, "a"),
        arg(4, "head.png"),
    );
    let (variant, fpu) = if arg(5, "pii") == "c" {
        (Variant::C, Fpu::Double)
    } else {
        (Variant::PII, Fpu::Chop)
    };
    let dir = std::env::var_os("ATLANTIS2_DATA").ok_or("ATLANTIS2_DATA not set")?;
    let discs = std::env::var_os("ATLANTIS2_DISCS").ok_or("ATLANTIS2_DISCS not set")?;
    let (files, _) = A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)])?;
    let read = |name: &str| files.read(Folder::ThreeDDial, name);
    let scene = Scene::parse(&read(&db).ok_or("no such head")?)?;
    let map = Head::texture_name(&scene).ok_or("no texture")?.to_string();
    let (yf, ymf) = Head::eye_map_names(&map);
    let tga = |n: &str| read(n).and_then(|d| Tga::parse(&d).ok());
    let mut head = Head::new(
        fpu,
        variant,
        &scene,
        tga(&map).ok_or("texture missing")?,
        tga(&yf),
        tga(&ymf),
    )?;
    let mut rand = Rand::new(0);
    head.pose(frame);
    if mouth == "idle" {
        head.idle_mouth(1, &mut rand);
    } else {
        let mut w = [0.0f32; 7];
        let i = VISEMES
            .iter()
            .position(|&c| mouth.as_bytes().first() == Some(&c))
            .ok_or("mouth: d i a o b s v or idle")?;
        w[i] = 1.0;
        head.mouth(1, &w);
    }
    let mut pixels = vec![0x4208u16; WIDTH * HEIGHT];
    let mut z = vec![0u32; WIDTH * HEIGHT];
    head.display(cam, &mut rand, &mut pixels, &mut z);
    std::fs::write(&out, encode_rgb565(&pixels, WIDTH, HEIGHT))?;
    println!("{out}: {db} frame {frame} camera {cam} mouth {mouth}");
    Ok(())
}

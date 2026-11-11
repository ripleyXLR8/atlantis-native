//! Draws a node of the installed game to a PNG, on the processor.
//!
//! `cargo run --release -p atlantis4-engine --example a4_render_node --
//! <out.png> [yaw pitch [width height [node]]]`, with `ATLANTIS4_DATA` =
//! the installed game; the node defaults to the first of a new game. Prints
//! the MD5 of the pixels (XRGB8888 words, little-endian, rows top-down).

use std::io::BufWriter;

use atlantis4_data::{A4Files, WarpPath};
use atlantis4_engine::camera::{fov_for_height, Camera};
use atlantis4_engine::node::{first_warp_path, Node, FIRST_WARP_SCRIPT};
use atlantis4_engine::render::{render, Weights};
use md5::{Digest, Md5};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args
        .first()
        .ok_or("usage: a4_render_node <out.png> [yaw pitch [w h [node]]]")?;
    let num = |i: usize, d: f32| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (yaw, pitch) = (num(1, 0.0), num(2, 0.0));
    let (w, h) = (num(3, 1365.0) as u32, num(4, 768.0) as u32);
    let dir = std::env::var_os("ATLANTIS4_DATA").ok_or("ATLANTIS4_DATA not set")?;
    let (files, build) = A4Files::open_dir(dir.as_ref())?;
    let node_name = match args.get(5) {
        Some(n) => n.clone(),
        None => first_warp_path(&files.read(FIRST_WARP_SCRIPT).ok_or("no first warp")?)
            .ok_or("no firstWarpPath")?,
    };
    let path = WarpPath::parse(&node_name).ok_or("not a node name")?;
    let node = Node::load(&files, &path)?;
    let cam = Camera::new(fov_for_height(h), w, h, yaw, pitch);
    let mut px = vec![0u32; (w * h) as usize];
    render(&cam, &node.faces, &mut px, Weights::Exact);
    let mut md5 = Md5::new();
    for p in &px {
        md5.update(p.to_le_bytes());
    }
    let hash: String = md5.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let rgb: Vec<u8> = px
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();
    let mut enc = png::Encoder::new(BufWriter::new(std::fs::File::create(out)?), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgb)?;
    println!(
        "{} {} yaw {yaw} pitch {pitch} {w}x{h}: md5 {hash} -> {out}",
        build.id,
        path.stem()
    );
    Ok(())
}

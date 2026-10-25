//! Plays the credits of Atlantis II headless at 59.94 frames a second and
//! writes some of their frames to PNG files: `<prefix>page00.png` (2 s),
//! `<prefix>fade.png` (6 s, page 0 half faded into page 1),
//! `<prefix>page01.png` (9 s), `<prefix>page16.png` (page 16, the first
//! picture again, 114 s), `<prefix>black.png` (the last page half faded to
//! black, 146 s), and prints when they end.
//!
//! ```text
//! ATLANTIS2_DATA=<install> ATLANTIS2_DISCS=<disc images> \
//!     cargo run --release --example credits_png -- <prefix>
//! ```

use std::path::PathBuf;

use atlantis2_data::A2Files;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{Engine, Fpu, Variant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let prefix = std::env::args().nth(1).unwrap_or_else(|| "credits-".into());
    let dir = std::env::var_os("ATLANTIS2_DATA").ok_or("ATLANTIS2_DATA not set")?;
    let discs = std::env::var_os("ATLANTIS2_DISCS").ok_or("ATLANTIS2_DISCS not set")?;
    let (files, _) = A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)])?;
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480)?;
    let mut screen = vec![0u16; 640 * 480];
    let shots = [
        (120, "page00"),
        (360, "fade"),
        (540, "page01"),
        (114 * 60, "page16"),
        (146 * 60, "black"),
    ];
    e.set_clock(0.0);
    e.open_credits();
    for frame in 0..200 * 60 {
        e.set_clock(frame as f64 * PASS);
        e.pass(Some(&mut screen));
        if let Some((_, name)) = shots.iter().find(|s| s.0 == frame) {
            let path = format!("{prefix}{name}.png");
            std::fs::write(&path, encode_rgb565(&screen, 640, 480))?;
            println!("{path}");
        }
        if e.quit() {
            println!("the credits ended at {:.3} s", frame as f64 * PASS);
            break;
        }
    }
    Ok(())
}

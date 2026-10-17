//! Decoding speed of one video: `cargo run --release --example bench_video
//! -- <game dir> UBB_VUE\\CINEM020.HNM`.
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let mut a = std::env::args().skip(1);
    let dir = a.next().expect("game dir");
    let name = a.next().unwrap_or_else(|| "UBB_VUE\\CINEM020.HNM".into());
    let game = atlantis_data::Game::open(Arc::new(atlantis_data::StdDir::new(dir))).unwrap();
    let data = game.big.read(&name).expect("video");
    let t = Instant::now();
    let mut v = atlantis_codecs::HnmVideo::new(data).unwrap();
    let mut n = 0;
    while let Some(f) = v.next_frame() {
        f.unwrap();
        n += 1;
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "{name}: {n} frames {}x{}, {:.2} ms per frame",
        v.width(),
        v.height(),
        ms / n as f64
    );
}

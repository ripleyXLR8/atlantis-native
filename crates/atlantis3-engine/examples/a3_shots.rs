//! Developers' aid: plays a new game headless and writes screenshots
//! (PNG) of the intro, the cinematics and the first places to a folder.
//!
//! `cargo run --release -p atlantis3-engine --example a3_shots -- <game folder> <out folder>`

use std::path::PathBuf;

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("game folder"));
    let out = PathBuf::from(args.next().expect("output folder"));
    std::fs::create_dir_all(&out).unwrap();
    let files = A3Files::open_dir(&dir).expect("an Atlantis III install").0;
    let mut g = Game::new(files).expect("the game starts");
    let save = |g: &Game, name: &str| {
        let png = atlantis2_engine::png::encode_rgb565(g.screen(), g.width(), g.height());
        std::fs::write(out.join(format!("{name}.png")), png).unwrap();
        println!("{name}");
    };
    let mut t = 0.0;
    let step = |g: &mut Game, t: &mut f64| {
        *t += 1.0 / 60.0;
        g.step(*t);
    };
    g.start_intro();
    let mut shot = [80u32, 200, 300].into_iter().peekable();
    while g.mode() == Mode::Intro {
        step(&mut g, &mut t);
        if let (Some(&k), Some((_, f))) = (shot.peek(), g.video()) {
            if f >= k {
                save(&g, &format!("intro-{k:03}"));
                shot.next();
            }
        }
    }
    g.new_game();
    let mut films = Vec::<String>::new();
    while g.mode() != Mode::Warp
        || g.video().is_some()
        || !g.script().is_some_and(|s| s.flags[1000] == 1)
    {
        step(&mut g, &mut t);
        if let Some((name, f)) = g.video() {
            let name = name.to_string();
            if f == 100 && !films.contains(&name) {
                save(&g, &format!("film-{}", name.trim_end_matches(".hnm")));
                films.push(name);
            }
        }
    }
    step(&mut g, &mut t);
    save(&g, "hoggar-v4-first");
    // The zone to view 5 is under the centre: the move, the travel video.
    g.click();
    while g.mode() != Mode::Video {
        step(&mut g, &mut t);
    }
    for _ in 0..40 {
        step(&mut g, &mut t);
    }
    save(&g, "travel-ho004005");
    while g.mode() != Mode::Warp {
        step(&mut g, &mut t);
    }
    for _ in 0..30 {
        step(&mut g, &mut t);
    }
    save(&g, "hoggar-v5");
    for _ in 0..10 {
        g.mouse_motion(-60, 10);
        step(&mut g, &mut t);
    }
    for _ in 0..60 {
        step(&mut g, &mut t);
    }
    save(&g, "hoggar-v5-turned");
}

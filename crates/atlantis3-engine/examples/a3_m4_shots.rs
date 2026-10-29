//! Developers' aid: screenshots (PNG) of the M4 boards, headless, played
//! through the board API: the merchant's house guard with the stone in
//! flight, the garden's jewel case with a gem glowing under the pointer.
//!
//! `cargo run --release -p atlantis3-engine --example a3_m4_shots -- <game folder> <out folder>`

use std::path::{Path, PathBuf};

use atlantis3_data::A3Files;
use atlantis3_engine::{BoardInput, Game};

/// Frame n of the place harnesses' clock.
fn at(n: u32) -> f64 {
    100.0 + f64::from(n) * 1.0025 / 15.0
}

fn shot(g: &Game, out: &Path, name: &str) {
    let png = atlantis2_engine::png::encode_rgb565(g.screen(), g.width(), g.height());
    std::fs::write(out.join(format!("{name}.png")), png).unwrap();
    println!("{name}");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("game folder"));
    let out = PathBuf::from(args.next().expect("output folder"));
    std::fs::create_dir_all(&out).unwrap();
    let files = || A3Files::open_dir(&dir).expect("an Atlantis III install").0;

    // The guard: the cat freed, view 1, the stone thrown on its zone, 20
    // frames of its flight (the hook draws the panorama and the stone).
    let mut g = Game::new(files()).expect("the game starts");
    for f in [118, 119] {
        g.script_mut().unwrap().flags[f] = 1;
    }
    assert!(g.board_begin("MMarchd.wam", 3));
    g.board_enter(1, at(0));
    g.run_command("(sacados=MMA_PIERRE)");
    g.run_command("(objetenmain=MMA_PIERRE)");
    let mut n = 1;
    assert!(g.board_frame(at(n), BoardInput::Zone("MMA_ZONE_PIERRE")));
    for _ in 0..20 {
        n += 1;
        g.board_frame(at(n), BoardInput::None);
    }
    shot(&g, &out, "guard");

    // The jewel case: view 114, the case opened by its zone, the pointer
    // on the first gem for a few ticks.
    let mut g = Game::new(files()).expect("the game starts");
    for f in [829, 144] {
        g.script_mut().unwrap().flags[f] = 1;
    }
    assert!(g.board_begin("jardin.wam", 3));
    g.board_enter(114, 100.0);
    assert!(g.board_zone_hook("BAG_ZONE_ECRIN", 200.0));
    let gem = (0..600)
        .step_by(8)
        .flat_map(|y| (0..800).step_by(8).map(move |x| (x, y)))
        .find(|&p| g.board_ecrin_hit(0, p))
        .unwrap_or((400, 300));
    for k in 0..20 {
        let t = 200.0 + f64::from(k + 1) / 32.0;
        g.board_screen_frame(t, gem, false, false);
    }
    shot(&g, &out, "ecrin");
}

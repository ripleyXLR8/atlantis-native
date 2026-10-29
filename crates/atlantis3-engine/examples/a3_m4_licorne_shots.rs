//! Developers' aid: screenshots (PNG) of the unicorn board (`LICORNE`),
//! headless, played through the game: the board started by its flag in
//! the garden, the four beacons dropped on a winning placement
//! (`licorne_beacons`), then the unicorn clicked and the run under way
//! with the beast behind it (`licorne_run`).
//!
//! `cargo run --release -p atlantis3-engine --example a3_m4_licorne_shots -- <game folder> <out folder>`

use std::path::{Path, PathBuf};

use atlantis3_data::A3Files;
use atlantis3_engine::licorne::{self, Board};
use atlantis3_engine::{BoardInput, Game};

/// Frame n of the board harness's clock.
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

    let mut g = Game::new(files()).expect("the game starts");
    g.script_mut().unwrap().flags[147] = 1;
    assert!(g.board_begin("jardin.wam", 3));
    g.board_enter(31, at(0));
    let mut n = 1;
    g.board_frame(at(n), BoardInput::None);
    assert_eq!(g.board_mode_number(), 11, "the board runs");
    let (w, h) = (g.width() as i32, g.height() as i32);
    // A scratch board finds a pointer that drops on each cell.
    let mut probe = Board::new(&files(), g.width(), g.height(), 100.0).unwrap();
    let mut drop_point = |cell: (i32, i32)| {
        (0..h)
            .step_by(2)
            .flat_map(|y| (0..w).step_by(2).map(move |x| (x, y)))
            .find(|&(x, y)| probe.drop_cell(0, x, y) == Some(cell))
            .expect("a pointer drops there")
    };
    let tray = (430 * w / 800, 570 * h / 600);
    let mut frame = |g: &mut Game, p: (i32, i32), click: bool| {
        n += 1;
        g.board_screen_frame(at(n), p, click, false)
    };
    for cell in [(9, 1), (3, 6), (0, 3), (1, 0)] {
        frame(&mut g, tray, false);
        frame(&mut g, tray, true);
        let p = drop_point(cell);
        frame(&mut g, p, false);
        frame(&mut g, p, true);
    }
    let unicorn = licorne::cell_xy(10, 6, w, h);
    frame(&mut g, unicorn, false);
    shot(&g, &out, "licorne_beacons");
    frame(&mut g, unicorn, true);
    for _ in 0..110 {
        frame(&mut g, unicorn, false);
    }
    shot(&g, &out, "licorne_run");
}

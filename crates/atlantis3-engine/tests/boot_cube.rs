//! The boot state's cube against the original under emulation
//! (`tests/golden/boot_cube.txt`): chapter 0's `CFuner.wam` view 4 entered
//! by the start-up with world 1 asks for `warp\Warp512\hoggar\cf0004.hnm`,
//! which the game has only in `ldm\`; the loader (@ 0x0042fd30) then
//! leaves its six debug colours, tries six TGA faces that do not exist,
//! and logs nothing. The same view entered with world 2 reads the real
//! cube. Compared: the cube's path and each face's md5.
//!
//! Needs the game: `ATLANTIS3_DATA`.

mod common;

use atlantis3_data::A3Files;
use atlantis3_engine::Game;
use common::board::md5_screen;

const GOLDEN: &str = include_str!("golden/boot_cube.txt");

fn files() -> Option<impl Fn() -> A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(move || A3Files::open_dir(&dir).unwrap().0)
}

/// The golden's runs: world, the cube asked for, the faces' md5s.
fn golden() -> Vec<(i32, String, Vec<String>)> {
    let mut out: Vec<(i32, String, Vec<String>)> = Vec::new();
    for l in GOLDEN.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first().copied() {
            Some("run") => out.push((w[2].parse().unwrap(), String::new(), Vec::new())),
            Some("exists") => {
                let r = out.last_mut().unwrap();
                r.1 = w[1].replace('\\', "/");
            }
            Some("face") => out.last_mut().unwrap().2.push(w[7].to_string()),
            _ => {}
        }
    }
    out
}

fn faces_md5(g: &Game) -> Vec<String> {
    g.board_cube().1.iter().map(|f| md5_screen(f)).collect()
}

#[test]
fn the_boot_cube_as_the_original() {
    let Some(files) = files() else {
        return;
    };
    let runs = golden();
    assert_eq!(runs.len(), 2);
    // World 1: the start-up's own entry (Game::new runs chapter 0).
    let (world, path, md5s) = &runs[0];
    assert_eq!(*world, 1);
    let g = Game::new(files()).unwrap();
    assert_eq!(g.board_cube().0, path);
    assert!(files().size(path).is_none());
    assert_eq!(&faces_md5(&g), md5s);
    // World 2: the view in its own folder.
    let (world, path, md5s) = &runs[1];
    assert_eq!(*world, 2);
    let mut g = Game::new(files()).unwrap();
    assert!(g.board_begin("CFuner.wam", 2));
    g.board_enter(4, 100.0);
    assert_eq!(g.board_cube().0, path);
    assert_eq!(&faces_md5(&g), md5s);
}

//! The folders WarpRes chooses (`Atlantis3.cfg`'s `WarpRes`, the
//! options' texture button; 0x00789d34). Each of the original's path
//! builders tests it for 1024 exactly: the cube `warp\Warp1024\`
//! (@ 0x00415a30), the place animations `spw\SPW1024\` (@ 0x004164c0),
//! and `HNM800\` for the cinematics (@ 0x00416880, @ 0x00416c60), the
//! dialogs' body videos (@ 0x00417280) and the travel videos
//! (@ 0x00417670), else `Warp512\`, `SPW512\`, `HNM640\`. GOG.com ships
//! none of the 1024 / 800 folders, so with WarpRes 1024 the original shows
//! the cube's debug colours, no place animation, and plays no cinematic
//! and no travel video (the travel's exists test @ 0x004402da).
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

fn game(warp_res: i32) -> Option<Game> {
    let dir = atlantis3_data::testing::install_dir()?;
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    g.startup.config.warp_res = warp_res;
    g.set_world(1);
    assert!(g.load_place("hoggar.wam"));
    Some(g)
}

/// Hoggar view 4, the move to view 5 (`ho004005.HNM`,
/// `tests/travel_escape.rs`) clicked and run: whether a travel video
/// started.
fn walk(g: &mut Game) -> bool {
    g.enter_at(4, 0.0, 0.0);
    g.set_harness_turns(true);
    let mut t = 10.0;
    t += 1.0 / 30.0;
    g.step(t);
    assert!(g.aim_at_zone(|z| z.kind == 1 && z.value == 5));
    g.click();
    for _ in 0..300 {
        if g.mode() == Mode::Video {
            return true;
        }
        t += 1.0 / 30.0;
        g.step(t);
    }
    assert_eq!(g.place().1, Some(5), "arrived");
    false
}

#[test]
fn warp_res_512_reads_the_shipped_folders() {
    let Some(mut g) = game(512) else {
        return;
    };
    assert!(walk(&mut g), "the travel video plays");
    assert!(g.board_cube().0.starts_with("warp/Warp512/hoggar/"));
    assert!(!g.slots().slots.is_empty());
    g.finish_video();
    g.run_command("(Cinema=93)");
    assert_eq!(g.mode(), Mode::Video, "the cinematic plays");
}

#[test]
fn warp_res_1024_reads_the_1024_folders_as_the_original() {
    let Some(mut g) = game(1024) else {
        return;
    };
    assert!(
        !walk(&mut g),
        "no travel video (hnmtraj\\HNM800\\ is not shipped)"
    );
    let (cube, faces) = g.board_cube();
    assert_eq!(cube, "warp/Warp1024/hoggar/ho0005.hnm");
    // The debug colour of face 0 (x8r8g8b8 0xff000080 in RGB565).
    assert_eq!(faces[0][0], 0x0010);
    assert!(g.slots().slots.is_empty(), "no place animation");
    // No cinematic either (hnm\HNM800\).
    g.run_command("(Cinema=93)");
    assert_ne!(g.mode(), Mode::Video);
}

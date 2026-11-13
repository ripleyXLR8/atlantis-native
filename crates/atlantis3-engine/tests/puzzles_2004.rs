//! *Atlantis Evolution*'s 2004 CD program (`ATLANTIS4_CD`): the
//! mini-games' frame ([`atlantis3_engine::puzzles2004`]): `INITPUZZLE`,
//! `STARTPUZZLE`, `FINPUZZLE`, the loop mode, `Atlantis4.cfg`'s `Puzzles`
//! flags, the `/puz` scan, and each game played from its chapter.
//!
//! `PUZZLES_2004_PPM=dir` writes the games' screens as PPM files.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};
use atlantis3_engine::puzzles2004::PuzzleId;
use atlantis3_engine::px::Surface;

fn game() -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
        atlantis3_data::testing::missing("ATLANTIS4_CD not set");
        return None;
    };
    let src = Arc::new(Evolution2004Source::new(Arc::new(
        atlantis_data::StdDir::new(&dir),
    )));
    Some(Game::new(A3Files::evolution_2004(src)).expect("the 2004 program's game"))
}

/// A new game in `chapter` (the intro skipped, then `CHAPTER=n`).
fn in_chapter(chapter: i32) -> Option<(Game, f64)> {
    let mut g = game()?;
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    g.run_command(&format!("(chapter={chapter})"));
    g.step(0.02);
    Some((g, 0.02))
}

/// 25 frames a second for `secs`.
#[allow(dead_code)]
fn run(g: &mut Game, t: &mut f64, secs: f64) {
    let end = *t + secs;
    while *t < end {
        *t += 0.04;
        g.step(*t);
    }
}

#[allow(dead_code)]
fn write_ppm(name: &str, s: &Surface, w: usize, h: usize) {
    let Ok(dir) = std::env::var("PUZZLES_2004_PPM") else {
        return;
    };
    let mut out = format!("P6 {w} {h} 255\n").into_bytes();
    match s {
        Surface::Rgb565(p) => {
            for &c in p {
                let c = u32::from(c);
                out.extend([
                    ((c >> 11) << 3) as u8,
                    (((c >> 5) & 63) << 2) as u8,
                    ((c & 31) << 3) as u8,
                ]);
            }
        }
        Surface::Xrgb8888(p) => {
            for &c in p {
                out.extend([(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            }
        }
    }
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out).unwrap();
}

/// The keys: `INITPUZZLE` starts the game (its init ends with its start:
/// the loop mode, the `Puzzles` flag), `FINPUZZLE` goes back to the
/// panorama; an unknown name does nothing.
#[test]
fn init_start_fin() {
    let Some((mut g, _)) = in_chapter(110) else {
        return;
    };
    assert_eq!(g.mode(), Mode::Warp);
    assert_eq!(g.puzzles_2004_cfg(), [0; 8]);
    g.run_command("(initpuzzle=PUZ_NOTHING)");
    assert_eq!(g.mode(), Mode::Warp);
    g.run_command("(initpuzzle=PUZ_CATAPULTE)");
    assert_eq!(g.mode(), Mode::Puzzle2004);
    assert_eq!(g.puzzle_2004(), Some(PuzzleId::Catapulte));
    assert_eq!(g.puzzles_2004_cfg(), [0, 1, 0, 0, 0, 0, 0, 0]);
    assert!(g.puzzle_2004_surface().is_some());
    g.run_command("(startpuzzle=PUZ_CATAPULTE)");
    assert_eq!(g.mode(), Mode::Puzzle2004);
    g.run_command("(finpuzzle=PUZ_CATAPULTE)");
    assert_eq!(g.mode(), Mode::Warp);
    assert_eq!(g.puzzle_2004(), None);
    assert!(g.puzzle_2004_surface().is_none());
    assert_eq!(g.puzzles_2004_cfg(), [0, 1, 0, 0, 0, 0, 0, 0]);
}

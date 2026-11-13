//! *Atlantis Evolution*'s 2004 CD program (`ATLANTIS4_CD`): Hanoi ([`atlantis3_engine::puzzles2004`]) started in their chapters and
//! won by input, the chapters' `/puz` sections reacting.
//!
//! `PUZZLES_2004_PPM=dir` writes the games' screens as PPM files.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};
use atlantis3_engine::puzzles2004::hanoi::{Hanoi, RECTS};
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

fn var(g: &Game, name: &str) -> i32 {
    g.script().and_then(|s| s.var(name)).unwrap_or(0)
}

/// One frame 1/25 s later.
fn frame(g: &mut Game, t: &mut f64) {
    *t += 0.04;
    g.step(*t);
}

/// The pointer moved to (x, y) of the stretch surface (relative motion,
/// clamped to it).
fn point(g: &mut Game, x: i32, y: i32) {
    let (px, py) = g.pointer();
    g.mouse_motion(x - px, y - py);
    assert_eq!(g.pointer(), (x, y));
}

fn write_ppm(name: &str, s: &Surface, w: usize, h: usize) {
    let Ok(dir) = std::env::var("PUZZLES_2004_PPM") else {
        return;
    };
    let mut out = format!("P6 {w} {h} 255\n").into_bytes();
    for c in s.to_rgb() {
        out.extend(c);
    }
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out).unwrap();
}

fn hanoi(g: &Game) -> &Hanoi {
    g.puzzle_2004_state::<Hanoi>().expect("Hanoi runs")
}

/// The moves of `n` disks from `a` to `c`.
fn solve(n: i32, a: usize, b: usize, c: usize, out: &mut Vec<(usize, usize)>) {
    if n > 0 {
        solve(n - 1, a, c, b, out);
        out.push((a, c));
        solve(n - 1, b, a, c, out);
    }
}

/// Hanoi in chapter 31 (the Cosmo console, /009): the disks in the bag
/// count; solved by clicks, `HANOI_GAGNE` = 1, then the /puz section
/// (/010) ends the game and opens the drawer (`CONSOLE_COSMO_OK`).
#[test]
fn hanoi_won() {
    let Some((mut g, mut t)) = in_chapter(31) else {
        return;
    };
    for o in ["DISQUE_1", "DISQUE_2", "DISQUE_KAMA", "CASQUE_SUR_TETE"] {
        g.run_command(&format!("(sacados={o})"));
    }
    g.run_command("(initpuzzle=PUZ_HANOI)");
    g.run_command("(startpuzzle=PUZ_HANOI)");
    assert_eq!(g.puzzle_2004(), Some(PuzzleId::Hanoi));
    assert_eq!(hanoi(&g).disks, 3);
    assert_eq!(hanoi(&g).pegs[0], [2, 1, 0]);
    // The pointer centred on the display by the init.
    frame(&mut g, &mut t);
    write_ppm("hanoi_start", g.puzzle_2004_surface().unwrap(), 800, 600);
    // A refused move: the big disk onto the small one.
    let click = |g: &mut Game, t: &mut f64, peg: usize| {
        let r = RECTS[peg];
        point(g, (r[0] + r[2]) / 2, 300);
        g.click();
        frame(g, t);
    };
    click(&mut g, &mut t, 0);
    click(&mut g, &mut t, 2);
    click(&mut g, &mut t, 0);
    assert_eq!(hanoi(&g).held, Some((1, 0)));
    write_ppm("hanoi_held", g.puzzle_2004_surface().unwrap(), 800, 600);
    click(&mut g, &mut t, 2);
    assert_eq!(hanoi(&g).pegs[2], [2]);
    assert_eq!(hanoi(&g).pegs[0], [1, 0]);
    // Back to the start, then the 7 moves.
    click(&mut g, &mut t, 2);
    click(&mut g, &mut t, 0);
    assert_eq!(hanoi(&g).pegs[0], [2, 1, 0]);
    let mut moves = Vec::new();
    solve(3, 0, 1, 2, &mut moves);
    for (i, (a, c)) in moves.into_iter().enumerate() {
        click(&mut g, &mut t, a);
        click(&mut g, &mut t, c);
        if i < 6 {
            assert_eq!(var(&g, "HANOI_GAGNE"), 0);
        }
    }
    assert_eq!(var(&g, "HANOI_GAGNE"), 1);
    assert_eq!(var(&g, "CONSOLE_COSMO_OK"), 0);
    write_ppm("hanoi_won", g.puzzle_2004_surface().unwrap(), 800, 600);
    // The next frame's /puz scan: /010.
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "HANOI_GAGNE"), 2);
    assert_eq!(var(&g, "CONSOLE_COSMO_OK"), 1);
    assert_eq!(g.puzzle_2004(), None);
    assert_ne!(g.mode(), Mode::Puzzle2004);
}

/// Hanoi's exit sprite: `HANOI_QUIT`, /014 ends the game.
#[test]
fn hanoi_quit() {
    let Some((mut g, mut t)) = in_chapter(31) else {
        return;
    };
    for o in ["DISQUE_1", "DISQUE_2", "CASQUE_SUR_TETE"] {
        g.run_command(&format!("(sacados={o})"));
    }
    g.run_command("(initpuzzle=PUZ_HANOI)");
    g.run_command("(startpuzzle=PUZ_HANOI)");
    assert_eq!(hanoi(&g).disks, 2);
    frame(&mut g, &mut t);
    // hanoi_bout.spr drawn at (50, 50).
    point(&mut g, 60, 60);
    g.click();
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "HANOI_QUIT"), 1);
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "HANOI_QUIT"), 2);
    assert_eq!(g.puzzle_2004(), None);
}

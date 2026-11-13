//! *Atlantis Evolution*'s 2004 CD program (`ATLANTIS4_CD`): Caisses
//! ([`atlantis3_engine::puzzles2004::caisses`]), which no script of the
//! CD starts: started by the test's own keys and played by input.
//!
//! `PUZZLES_2004_PPM=dir` writes the game's screens as PPM files.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::Game;
use atlantis3_engine::puzzles2004::caisses::{self, Caisses, Click, Dir};
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

/// A new game in chapter 28 (no `/puz` section of it names a Caisses
/// variable), Caisses started.
fn started() -> Option<(Game, f64)> {
    let mut g = game()?;
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    g.run_command("(chapter=28)");
    g.step(0.02);
    g.run_command("(initpuzzle=PUZ_CAISSES)");
    g.run_command("(startpuzzle=PUZ_CAISSES)");
    assert_eq!(g.puzzle_2004(), Some(PuzzleId::Caisses));
    Some((g, 0.02))
}

fn var(g: &Game, name: &str) -> i32 {
    g.script().and_then(|s| s.var(name)).unwrap_or(0)
}

fn frame(g: &mut Game, t: &mut f64) {
    *t += 0.04;
    g.step(*t);
}

fn point(g: &mut Game, x: i32, y: i32) {
    let (px, py) = g.pointer();
    g.mouse_motion(x - px, y - py);
    assert_eq!(g.pointer(), (x, y));
}

fn shot(g: &Game, name: &str) {
    let Ok(dir) = std::env::var("PUZZLES_2004_PPM") else {
        return;
    };
    let s: &Surface = g.puzzle_2004_surface().unwrap();
    let mut out = b"P6 800 600 255\n".to_vec();
    for c in s.to_rgb() {
        out.extend(c);
    }
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out).unwrap();
}

fn caisses(g: &Game) -> &Caisses {
    g.puzzle_2004_state::<Caisses>().expect("Caisses runs")
}

/// A pixel where a click hits `want` (the game's own tests on its
/// sprites), searched around (x, y).
fn pixel(c: &Caisses, want: Click, x: i32, y: i32) -> (i32, i32) {
    for dy in -10..80 {
        for dx in -10..80 {
            if c.hit(x + dx, y + dy) == want {
                return (x + dx, y + dy);
            }
        }
    }
    panic!("no pixel for {want:?}");
}

fn vk(d: Dir) -> u8 {
    match d {
        Dir::Up => 0x26,
        Dir::Down => 0x28,
        Dir::Left => 0x25,
        Dir::Right => 0x27,
    }
}

/// The four levels solved (each by its shortest solution, the moves by
/// the arrows' sprites and the arrow keys in turn), `CAISSES_GAGNE` = 1.
#[test]
fn caisses_won() {
    let Some((mut g, mut t)) = started() else {
        return;
    };
    assert_eq!(var(&g, "CAISSES_GAGNE"), 0);
    frame(&mut g, &mut t);
    let arrows: Vec<(Dir, (i32, i32))> = Dir::ALL
        .into_iter()
        .map(|d| {
            let (_, x, y) = d.arrow();
            (d, pixel(caisses(&g), Click::Arrow(d), x, y))
        })
        .collect();
    eprintln!("caisses: arrows' pixels {arrows:?}");
    for level in 0..4 {
        assert_eq!(caisses(&g).level, level);
        shot(&g, &format!("caisses_level{level}"));
        let moves = caisses::solve(caisses(&g)).expect("a solution");
        eprintln!("caisses: level {level} in {} moves", moves.len());
        for (i, d) in moves.iter().enumerate() {
            if i % 2 == 0 {
                let p = arrows.iter().find(|a| a.0 == *d).unwrap().1;
                point(&mut g, p.0, p.1);
                g.click();
                frame(&mut g, &mut t);
            } else {
                g.key_2004(vk(*d), true);
                frame(&mut g, &mut t);
                g.key_2004(vk(*d), false);
            }
            if level == 2 && i == moves.len() / 2 {
                shot(&g, "caisses_level2_half");
            }
        }
    }
    assert_eq!(var(&g, "CAISSES_GAGNE"), 1);
    assert_eq!(caisses(&g).level, 4);
}

/// The reset button rebuilds the level, the quit button sets
/// `CAISSES_QUIT`.
#[test]
fn caisses_buttons() {
    let Some((mut g, mut t)) = started() else {
        return;
    };
    frame(&mut g, &mut t);
    g.key_2004(0x25, true);
    frame(&mut g, &mut t);
    g.key_2004(0x25, false);
    assert_eq!(caisses(&g).grids[0].get(4, 2), caisses::CRATE);
    let p = pixel(caisses(&g), Click::Reset, 50, 50);
    point(&mut g, p.0, p.1);
    g.click();
    frame(&mut g, &mut t);
    let c = caisses(&g);
    assert_eq!((c.pos, c.grids[0].get(4, 3)), ((4.0, 4.0), caisses::CRATE));
    let p = pixel(caisses(&g), Click::Quit, 650, 50);
    point(&mut g, p.0, p.1);
    g.click();
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "CAISSES_QUIT"), 1);
}

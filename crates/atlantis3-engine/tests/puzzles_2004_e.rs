//! *Atlantis Evolution*'s 2004 CD program (`ATLANTIS4_CD`): Pong, Bulle
//! and Caisses ([`atlantis3_engine::puzzles2004`]) started in their
//! chapters (Caisses, which no script starts, by the test) and played by
//! input, the chapters' `/puz` sections reacting.
//!
//! `PUZZLES_2004_PPM=dir` writes the games' screens as PPM files.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};
use atlantis3_engine::puzzles2004::bulle::{self, Bulle};
use atlantis3_engine::puzzles2004::pong::Pong;
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
    let mut g = Game::new(A3Files::evolution_2004(src)).expect("the 2004 program's game");
    // The games seed `rand()` with `srand(time(NULL))` at their start
    // (Pong @ its init): a fixed clock keeps the scripted plays repeatable.
    g.unix_time = || 1_794_580_000;
    Some(g)
}

/// A new game in `chapter` (the intro skipped, then `CHAPTER=n`).
fn in_chapter(chapter: i32) -> Option<(Game, f64)> {
    let mut g = game()?;
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    // The cabin's `vue=1` leaves `ARRIVEEVUE` = 1 (the new view,
    // @ 0x0044e185): the next `Interp_GereFrame` runs the arrival scan,
    // which here would be the new chapter's (chapter 31's /001 Cosmo's
    // greeting, chapter 42's /001 dialog 722) and its dialog would keep
    // the game from starting. In the game that scan is the cabin's.
    g.set_var("ARRIVEEVUE", 0);
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

/// The pointer moved to (x, y) of the stretch surface.
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

fn shot(g: &Game, name: &str) {
    write_ppm(name, g.puzzle_2004_surface().unwrap(), 800, 600);
}

fn pong(g: &Game) -> &Pong {
    g.puzzle_2004_state::<Pong>().expect("Pong runs")
}

/// Pong in chapter 42 (the console, /002's keys): the player's paddle
/// put by the pointer where the ball comes, at an offset changed at each
/// return (`a4cd_pong.py`'s player); the god knocked out, the
/// sarcophagus broken three times, `PONG_GAGNE` = 1, then /003 ends the
/// game.
#[test]
fn pong_won() {
    let Some((mut g, mut t)) = in_chapter(42) else {
        return;
    };
    g.run_command("(initpuzzle=PUZ_PONG)");
    g.run_command("(startpuzzle=PUZ_PONG)");
    assert_eq!(g.puzzle_2004(), Some(PuzzleId::Pong));
    assert_eq!(var(&g, "PONG_GAGNE"), 0);
    let p = pong(&g);
    assert_eq!((p.gods, p.player.0, p.cpu.0, p.sarco.0), (4, 100, 100, 100));
    frame(&mut g, &mut t);
    shot(&g, "pong_start");
    const OFFSETS: [i32; 11] = [0, -30, 30, -44, 44, -58, 58, -10, 15, -65, 62];
    let (mut returns, mut coming) = (0usize, false);
    let mut shots = [false; 3];
    let mut gods = 4;
    for _ in 0..25 * 900 {
        let p = pong(&g);
        let ((bx, by), vx) = (p.ball, p.vel.0);
        if vx < 0.0 && !coming {
            coming = true;
        }
        if vx > 0.0 && coming {
            coming = false;
            returns += 1;
        }
        if vx < 0.0 && bx < 520.0 {
            let target = f64::from(by) + f64::from(OFFSETS[returns % OFFSETS.len()]) - 50.0;
            let want = ((target * 600.0 / 500.0).round() as i32).clamp(0, 599);
            if want != g.pointer().1 {
                point(&mut g, 400, want);
            }
        }
        frame(&mut g, &mut t);
        if var(&g, "PONG_GAGNE") != 0 {
            break;
        }
        assert_eq!(var(&g, "PONG_PERDU"), 0, "lost");
        let p = pong(&g);
        if p.gods != gods {
            gods = p.gods;
            eprintln!("pong: round {gods} at t = {t:.1} s");
        }
        if !shots[0] && p.ball.0 > 300.0 && p.ball.0 < 320.0 {
            shot(&g, "pong_play");
            shots[0] = true;
        }
        if !shots[1] && p.knocked && p.sarco.0 < 100 {
            shot(&g, "pong_knocked_out");
            shots[1] = true;
        }
        if !shots[2] && p.gods == 2 && p.ball.0 > 300.0 && p.ball.0 < 320.0 {
            shot(&g, "pong_round2");
            shots[2] = true;
        }
    }
    eprintln!(
        "pong: won at t = {t:.1} s, sarcophagus hit {} times",
        var(&g, "PONG_FRAPPE_SARCOPHAGE")
    );
    assert_eq!(var(&g, "PONG_GAGNE"), 1);
    assert_eq!(var(&g, "PONG_SAMA_DESACTIVE"), 1);
    assert_eq!(var(&g, "PONG_KAMA_DESACTIVE"), 1);
    assert!(var(&g, "PONG_FRAPPE_SARCOPHAGE") >= 15);
    // The next frame's /puz scan: /003.
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "PONG_GAGNE"), 2);
    let s = g.script().unwrap();
    assert_eq!((s.flags[176], s.flags[226]), (1, 1));
    assert_eq!(g.puzzle_2004(), None);
    assert_ne!(g.mode(), Mode::Puzzle2004);
}

/// The paddle left alone: five balls lost, `PONG_PERDU` = 1, /004 ends
/// the game.
#[test]
fn pong_lost() {
    let Some((mut g, mut t)) = in_chapter(42) else {
        return;
    };
    g.run_command("(initpuzzle=PUZ_PONG)");
    g.run_command("(startpuzzle=PUZ_PONG)");
    for _ in 0..25 * 120 {
        frame(&mut g, &mut t);
        if var(&g, "PONG_PERDU") != 0 {
            break;
        }
    }
    assert_eq!(var(&g, "PONG_PERDU"), 1);
    assert_eq!(pong(&g).player.0, 0);
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "PONG_PERDU"), 2);
    assert_eq!(g.puzzle_2004(), None);
}

fn bulle_state(g: &Game) -> &Bulle {
    g.puzzle_2004_state::<Bulle>().expect("Bulle runs")
}

/// The first step of a shortest drive (open edges, no U-turn) from the
/// crossroads ahead to the level's end (`a4cd_bulle.py`'s player).
fn bulle_plan(b: &Bulle) -> Option<i32> {
    let n = bulle::size(b.level);
    let end = bulle::start_end(b.level).1;
    let ahead = bulle::next_cell(b.level, b.cell, b.dir);
    if ahead == end || !(0..n * n).contains(&ahead) {
        return None;
    }
    let start = (ahead, b.dir);
    let mut prev = std::collections::HashMap::new();
    prev.insert(start, start);
    let mut q = std::collections::VecDeque::from([start]);
    while let Some((c, din)) = q.pop_front() {
        if c == end {
            let mut s = (c, din);
            while prev[&s] != start {
                s = prev[&s];
            }
            return Some(s.1);
        }
        for o in 0..4 {
            let (r, cc) = (c / n, c % n);
            let (r, cc) = [(r - 1, cc), (r, cc + 1), (r + 1, cc), (r, cc - 1)][o as usize];
            let t = n * r + cc;
            if o == (din + 2) % 4
                || !(0..n).contains(&r)
                || !(0..n).contains(&cc)
                || bulle::edge(b.level, c, t).is_none_or(|e| e <= 0)
                || prev.contains_key(&(t, o))
            {
                continue;
            }
            prev.insert((t, o), (c, din));
            q.push_back((t, o));
        }
    }
    None
}

/// Bulle in chapter 7 (/021's keys), the helmet in the bag: the start's 5
/// s (`BULLE_ETOILE`), then the arrow keys along a shortest drive through
/// both levels' films: `BULLE_GAGNE` = 1, /038 ends the game (chapter 8).
#[test]
fn bulle_won() {
    let Some((mut g, mut t)) = in_chapter(7) else {
        return;
    };
    g.run_command("(sacados=CASQUE_C17)");
    g.run_command("(initpuzzle=PUZ_BULLE)");
    g.run_command("(startpuzzle=PUZ_BULLE)");
    assert_eq!(g.puzzle_2004(), Some(PuzzleId::Bulle));
    assert_eq!(var(&g, "BULLE_DEBUT"), 1);
    assert!(bulle_state(&g).helmet);
    let mut shots = 0;
    let mut level = 1;
    for k in 0..25 * 200 {
        let b = bulle_state(&g);
        let vk = if b.star {
            bulle_plan(b).map(|o| [0x26u8, 0x27, 0x28, 0x25][o as usize])
        } else {
            None
        };
        if let Some(vk) = vk {
            g.key_2004(vk, true);
        }
        frame(&mut g, &mut t);
        if let Some(vk) = vk {
            g.key_2004(vk, false);
        }
        if var(&g, "BULLE_GAGNE") != 0 {
            break;
        }
        assert_eq!(var(&g, "BULLE_PERDU"), 0, "crashed");
        let b = bulle_state(&g);
        if k == 10 {
            shot(&g, "bulle_start");
            assert!(!b.star);
        }
        if b.level != level {
            level = b.level;
            eprintln!("bulle: level {level} at t = {t:.1} s");
        }
        if shots < 3 && k > 200 && k % 97 == 0 {
            shot(&g, &format!("bulle_drive{shots}"));
            shots += 1;
        }
    }
    eprintln!("bulle: won at t = {t:.1} s");
    assert_eq!(var(&g, "BULLE_GAGNE"), 1);
    // /038: the game freed, the film `BULLE_GAGNE`, then chapter 8.
    frame(&mut g, &mut t);
    assert_eq!(g.puzzle_2004(), None);
    for _ in 0..25 * 120 {
        if var(&g, "chapter") == 8 {
            break;
        }
        frame(&mut g, &mut t);
    }
    assert_eq!(var(&g, "chapter"), 8);
}

/// No key: the level's route drives into a wall (crossroads 8 to 4 of
/// level 1): `BULLE_PERDU`, `BULLE_NB_MORTS`, /036 ends the game; the
/// next session starts the level again.
#[test]
fn bulle_crash() {
    let Some((mut g, mut t)) = in_chapter(7) else {
        return;
    };
    g.run_command("(initpuzzle=PUZ_BULLE)");
    g.run_command("(startpuzzle=PUZ_BULLE)");
    assert!(!bulle_state(&g).helmet);
    for _ in 0..25 * 60 {
        frame(&mut g, &mut t);
        if var(&g, "BULLE_PERDU") != 0 {
            break;
        }
    }
    assert_eq!(var(&g, "BULLE_PERDU"), 1);
    assert_eq!(var(&g, "BULLE_NB_MORTS"), 1);
    let b = bulle_state(&g);
    assert_eq!((b.cell, b.dir, b.restart), (8, 0, true));
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "BULLE_PERDU"), 0);
    assert_eq!(g.puzzle_2004(), None);
    // A second session: the level's start again, the dead count kept.
    // /036's film `BULLE_PERDU` first.
    for _ in 0..25 * 120 {
        if g.mode() != Mode::Video {
            break;
        }
        frame(&mut g, &mut t);
    }
    assert_ne!(g.mode(), Mode::Video);
    g.run_command("(initpuzzle=PUZ_BULLE)");
    g.run_command("(startpuzzle=PUZ_BULLE)");
    frame(&mut g, &mut t);
    let b = bulle_state(&g);
    assert_eq!((b.level, b.cell, b.driving), (1, 14, true));
}

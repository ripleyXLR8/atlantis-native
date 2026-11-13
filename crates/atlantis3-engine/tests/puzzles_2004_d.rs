//! *Atlantis Evolution*'s 2004 CD program (`ATLANTIS4_CD`): Frogger and
//! Pong ([`atlantis3_engine::puzzles2004`]) started in their chapters and
//! played by input, the chapters' `/puz` sections reacting.
//!
//! `PUZZLES_2004_PPM=dir` writes the games' screens as PPM files.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};
use atlantis3_engine::puzzles2004::frogger::{self, Frogger};
use atlantis3_engine::puzzles2004::sim::SimHost;
use atlantis3_engine::puzzles2004::{Host, PuzzleId};
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

fn frog(g: &Game) -> &Frogger {
    g.puzzle_2004_state::<Frogger>().expect("Frogger runs")
}

const VK_UP: u8 = 0x26;
const VK_LEFT: u8 = 0x25;
const VK_RIGHT: u8 = 0x27;
const VK_DOWN: u8 = 0x28;

fn code(vk: u8) -> i32 {
    match vk {
        VK_UP => frogger::KEY_UP,
        VK_LEFT => frogger::KEY_LEFT,
        VK_RIGHT => frogger::KEY_RIGHT,
        _ => frogger::KEY_DOWN,
    }
}

/// Whether pressing `key` (or nothing) now leaves the frog alive for the
/// next `frames` frames of 1/25 s (the game's own step on a copy).
fn safe(f: &Frogger, key: Option<u8>, frames: usize, q4: i32) -> bool {
    let mut f = f.clone();
    let mut h = SimHost::new(0.04);
    h.set_var("FROG_QUATRIEME_SYMB", q4);
    h.key_code = key.map_or(0, code);
    let lives = f.lives;
    for _ in 0..frames {
        f.step(&mut h);
        h.key_code = 0;
        if f.lives != lives || h.var("FROG_QUIT") != 0 {
            return false;
        }
    }
    true
}

/// The player: on the top lane, towards the symbol then up; elsewhere up
/// when it is safe for half a second, else waiting, else sideways.
fn choose(f: &Frogger, q4: i32) -> Option<u8> {
    if f.jumping || f.scrolling {
        return None;
    }
    const H: usize = 14;
    if f.lane == 9 {
        let held = if f.level == 4 {
            q4 == 3
        } else {
            f.symbols[(f.level - 1) as usize].found == 1
        };
        if held {
            return Some(VK_UP);
        }
        if f.level == 4 && f.symbols[3].found == 1 {
            // The script's FROG_QUATRIEME_SYMB = 3 comes with the next
            // /puz scan.
            return None;
        }
        return Some(if f.x < 400 { VK_RIGHT } else { VK_LEFT });
    }
    [
        Some(VK_UP),
        None,
        Some(VK_LEFT),
        Some(VK_RIGHT),
        Some(VK_DOWN),
    ]
    .into_iter()
    .find(|&k| safe(f, k, H, q4))
    .flatten()
}

/// Frogger in chapter 28 (the hangar's console, /005): the four levels
/// crossed by the arrow keys, each symbol picked up (the script's /015..
/// /018 answer), `FROG_GAGNE` = 1, then /019 ends the game.
#[test]
fn frogger_won() {
    let Some((mut g, mut t)) = in_chapter(28) else {
        return;
    };
    // /005's keys.
    g.run_command("(FROG_GAGNE=0)");
    g.run_command("(FROG_QUIT=0)");
    g.run_command("(initpuzzle=PUZ_FROGGER)");
    g.run_command("(startpuzzle=PUZ_FROGGER)");
    assert_eq!(g.puzzle_2004(), Some(PuzzleId::Frogger));
    assert_eq!(var(&g, "FROG_DEBUT"), 1);
    let f = frog(&g);
    assert_eq!((f.level, f.lives, f.x, f.y), (1, 3, 400, 530));
    // The banks' half widths.
    assert!(f.frog_hw > 0 && f.sym_hw > 0);
    assert!(f.lanes[1].objs[0].hw > 0 && f.lanes[3].objs[0].hw > 0);
    eprintln!(
        "frog hw {}, symbol hw {}, car hw {} / {}, boat hw {}",
        f.frog_hw, f.sym_hw, f.lanes[1].objs[0].hw, f.lanes[2].objs[0].hw, f.lanes[3].objs[0].hw
    );
    frame(&mut g, &mut t);
    // /013: the game reports its start.
    assert_eq!(g.script().unwrap().flags[127], 1);
    shot(&g, "frogger_start");
    let mut level = 1;
    let mut deaths = 0;
    let mut lives = 3;
    let mut shots = [false; 4];
    for _ in 0..25 * 280 {
        let q4 = var(&g, "FROG_QUATRIEME_SYMB");
        let key = choose(frog(&g), q4);
        if let Some(k) = key {
            g.key_2004(k, true);
        }
        frame(&mut g, &mut t);
        if let Some(k) = key {
            g.key_2004(k, false);
        }
        if var(&g, "FROG_GAGNE") != 0 {
            break;
        }
        let f = frog(&g);
        if f.lives < lives {
            deaths += 1;
            lives = f.lives;
        }
        if f.level != level {
            level = f.level;
        }
        let i = (f.level - 1) as usize;
        if !shots[i] && f.lane == 5 && !f.jumping && !f.scrolling {
            shot(&g, &format!("frogger_level{}", f.level));
            shots[i] = true;
        }
        if f.scrolling && f.scroll > 0.5 && f.scroll < 0.53 {
            shot(&g, "frogger_scroll");
        }
        assert_eq!(var(&g, "FROG_QUIT"), 0, "lost at level {}", f.level);
    }
    eprintln!("frogger: won at t = {t:.1} s, {deaths} deaths");
    assert_eq!(var(&g, "FROG_GAGNE"), 1);
    for v in frogger::SYMBOL_VARS {
        assert_eq!(var(&g, v), 3, "{v}");
    }
    assert_eq!(var(&g, "FROG_TRAVERSE_UNE_RUE"), 1);
    // The next frame's /puz scan: /019.
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "FROG_GAGNE"), 2);
    assert_eq!(g.script().unwrap().flags[124], 0);
    assert_eq!(g.puzzle_2004(), None);
    assert_ne!(g.mode(), Mode::Puzzle2004);
}

/// Frogger's exit sprite (`F_qu` at (720, 575)): `FROG_QUIT`, the frame
/// stops; the third /019 ends the game.
#[test]
fn frogger_quit() {
    let Some((mut g, mut t)) = in_chapter(28) else {
        return;
    };
    g.run_command("(initpuzzle=PUZ_FROGGER)");
    g.run_command("(startpuzzle=PUZ_FROGGER)");
    frame(&mut g, &mut t);
    point(&mut g, 0x2d0 + 5, 0x23f + 5);
    g.click();
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "FROG_QUIT"), 1);
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "FROG_QUIT"), 2);
    assert_eq!(g.puzzle_2004(), None);
}

/// The arrows' sprites move the frog like the keys.
#[test]
fn frogger_arrows() {
    let Some((mut g, mut t)) = in_chapter(28) else {
        return;
    };
    g.run_command("(initpuzzle=PUZ_FROGGER)");
    g.run_command("(startpuzzle=PUZ_FROGGER)");
    frame(&mut g, &mut t);
    // The left arrow (frame 2 at (0x2fd, 0x23f)): find an opaque pixel
    // of it that no earlier arrow covers (a copy of the game tries
    // each pixel).
    let x0 = frog(&g).x;
    let mut done = false;
    'search: for dy in -120..60 {
        for dx in -120..80 {
            let mut f = frog(&g).clone();
            let mut sim = SimHost::new(0.04);
            sim.pointer = (0x2fd + dx, 0x23f + dy);
            sim.click = true;
            f.step(&mut sim);
            if f.jumping && f.dir == frogger::LEFT {
                point(&mut g, 0x2fd + dx, 0x23f + dy);
                g.click();
                frame(&mut g, &mut t);
                assert!(frog(&g).jumping);
                assert_eq!(frog(&g).tx, x0 - 40);
                done = true;
                break 'search;
            }
        }
    }
    assert!(done, "no pixel of the left arrow found");
}

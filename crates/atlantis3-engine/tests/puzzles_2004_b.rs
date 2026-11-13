//! *Atlantis Evolution*'s 2004 CD program (`ATLANTIS4_CD`): Hanoi and
//! Tron ([`atlantis3_engine::puzzles2004`]) started in their chapters and
//! won by input, the chapters' `/puz` sections reacting.
//!
//! `PUZZLES_2004_PPM=dir` writes the games' screens as PPM files.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};
use atlantis3_engine::puzzles2004::hanoi::{Hanoi, RECTS};
use atlantis3_engine::puzzles2004::tron::{hits, Seg, Tron, WALLS};
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

fn tron(g: &Game) -> &Tron {
    g.puzzle_2004_state::<Tron>().expect("Tron runs")
}

const UP: u8 = 0x26;
const LEFT: u8 = 0x25;
const RIGHT: u8 = 0x27;
const DOWN: u8 = 0x28;

/// The free area reachable from (x, y) on a 1-pixel grid of the arena
/// (walls, trails, current segments blocking), up to `cap` cells.
fn room(g: &Tron, x: i32, y: i32, cap: usize) -> usize {
    let (x0, y0, w, h) = (0xf1, 0x8d, 0x22e - 0xf1 + 1, 0x1cd - 0x8d + 1);
    let mut grid = vec![false; (w * h) as usize];
    let mut mark = |s: &Seg| {
        let (ax, bx) = (s.x1.min(s.x2), s.x1.max(s.x2));
        let (ay, by) = (s.y1.min(s.y2), s.y1.max(s.y2));
        for yy in ay..=by {
            for xx in ax..=bx {
                if (x0..x0 + w).contains(&xx) && (y0..y0 + h).contains(&yy) {
                    grid[((yy - y0) * w + xx - x0) as usize] = true;
                }
            }
        }
    };
    WALLS.iter().for_each(&mut mark);
    for p in &g.players {
        p.trail.iter().for_each(&mut mark);
        mark(&p.seg);
    }
    let inside = |x: i32, y: i32| (x0..x0 + w).contains(&x) && (y0..y0 + h).contains(&y);
    if !inside(x, y) || grid[((y - y0) * w + x - x0) as usize] {
        return 0;
    }
    let mut stack = vec![(x, y)];
    grid[((y - y0) * w + x - x0) as usize] = true;
    let mut n = 0;
    while let Some((x, y)) = stack.pop() {
        n += 1;
        if n >= cap {
            break;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if inside(nx, ny) && !grid[((ny - y0) * w + nx - x0) as usize] {
                grid[((ny - y0) * w + nx - x0) as usize] = true;
                stack.push((nx, ny));
            }
        }
    }
    n
}

/// The human's arrow for this frame: straight on while 12 px ahead are
/// free and the AI's way (its segment carried on 40 px) is not crossed,
/// else the side with the larger reachable area.
fn steer(g: &Tron) -> Option<u8> {
    let p = &g.players[0];
    let s = p.seg;
    let (dx, dy) = match (s.dir, p.speed > 0.0) {
        (1, true) => (0, 1),
        (1, false) => (0, -1),
        (_, true) => (1, 0),
        (_, false) => (-1, 0),
    };
    let l = 12;
    let pr = 40;
    let ahead = Seg {
        x1: s.x1,
        y1: s.y1,
        x2: s.x2 + dx * l,
        y2: s.y2 + dy * l,
        dir: s.dir,
    };
    // The AI's segment carried on `pr` px.
    let a = &g.players[1];
    let (ax, ay) = match (a.seg.dir, a.speed > 0.0) {
        (1, true) => (0, 1),
        (1, false) => (0, -1),
        (_, true) => (1, 0),
        (_, false) => (-1, 0),
    };
    let projected = Seg {
        x2: a.seg.x2 + ax * pr,
        y2: a.seg.y2 + ay * pr,
        ..a.seg
    };
    if !g.blocked(0, &ahead) && !hits(&ahead, &projected) {
        return None;
    }
    // The two sides, 3 px off the end.
    let (ax, ay, bx, by) = if s.dir == 1 {
        (s.x2 - 3, s.y2, s.x2 + 3, s.y2)
    } else {
        (s.x2, s.y2 - 3, s.x2, s.y2 + 3)
    };
    let (ra, rb) = (room(g, ax, ay, 40_000), room(g, bx, by, 40_000));
    Some(match (s.dir, ra >= rb) {
        (1, true) => LEFT,
        (1, false) => RIGHT,
        (_, true) => UP,
        (_, false) => DOWN,
    })
}

/// Tron in chapter 40 (the cannon's console): the human steered by the
/// arrows until the AI crashes, `TRON_GAGNE` = 1, then /010 ends the game
/// (`flag170`).
#[test]
fn tron_won() {
    let Some((mut g, mut t)) = in_chapter(40) else {
        return;
    };
    g.run_command("(initpuzzle=PUZ_TRON)");
    g.run_command("(startpuzzle=PUZ_TRON)");
    assert_eq!(g.puzzle_2004(), Some(PuzzleId::Tron));
    assert_eq!(tron(&g).players[1].ai, 1);
    let mut lost = 0;
    let mut shot = false;
    let mut held: Option<u8> = None;
    for i in 0..25 * 600 {
        if let Some(k) = held.take() {
            g.key_2004(k, false);
        }
        if let Some(k) = steer(tron(&g)) {
            g.key_2004(k, true);
            held = Some(k);
        }
        let before = tron(&g).players[0].trail.len();
        frame(&mut g, &mut t);
        if var(&g, "TRON_GAGNE") != 0 {
            break;
        }
        let after = tron(&g).players[0].trail.len();
        if after < before && tron(&g).players[0].seg.y1 == 0x1c3 {
            lost += 1;
        }
        if !shot && i == 25 * 4 {
            write_ppm("tron_play", g.puzzle_2004_surface().unwrap(), 800, 600);
            shot = true;
        }
    }
    eprintln!("tron: {lost} lost rounds, t = {t:.1} s");
    assert_eq!(var(&g, "TRON_GAGNE"), 1);
    write_ppm("tron_won", g.puzzle_2004_surface().unwrap(), 800, 600);
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "TRON_GAGNE"), 2);
    assert_eq!(g.script().unwrap().flags[170], 1);
    assert_eq!(g.puzzle_2004(), None);
}

/// Tron's exit sprite (frame 1 at (50, 50)): `TRON_QUIT`, the frame stops
/// before moving; /010 (the second) ends the game.
#[test]
fn tron_quit() {
    let Some((mut g, mut t)) = in_chapter(40) else {
        return;
    };
    g.run_command("(initpuzzle=PUZ_TRON)");
    g.run_command("(startpuzzle=PUZ_TRON)");
    frame(&mut g, &mut t);
    let y = tron(&g).players[0].seg.y2;
    point(&mut g, 60, 60);
    g.click();
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "TRON_QUIT"), 1);
    assert_eq!(tron(&g).players[0].seg.y2, y);
    frame(&mut g, &mut t);
    assert_eq!(var(&g, "TRON_QUIT"), 2);
    assert_eq!(g.puzzle_2004(), None);
}

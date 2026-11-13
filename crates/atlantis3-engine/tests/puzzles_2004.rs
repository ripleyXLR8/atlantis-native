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

/// A bank of `cd0\datas\spr` (the CD's names are of mixed case).
fn bank(g: &Game, name: &str) -> Arc<atlantis3_data::spr::Bank> {
    let dir =
        std::path::PathBuf::from(std::env::var_os("ATLANTIS4_CD").unwrap()).join("cd0/datas/spr");
    let f = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("{name}"));
    let d = std::fs::read(f.path()).unwrap();
    Arc::new(atlantis3_data::spr::Bank::parse(&d, g.depth()).unwrap())
}

/// A pixel of `bank`'s sprite `index` (drawn at its own place) that none of
/// `above` (tested first by the game) covers.
fn spot(b: &atlantis3_data::spr::Bank, index: usize, above: &[usize]) -> (i32, i32) {
    for y in (0..600).step_by(3) {
        for x in (0..800).step_by(3) {
            if b.opaque_at(index, None, 0, x, y)
                && !above.iter().any(|&a| b.opaque_at(a, None, 0, x, y))
            {
                return (x, y);
            }
        }
    }
    panic!("no pixel of #{index}");
}

fn click_at(g: &mut Game, t: &mut f64, p: (i32, i32)) {
    g.set_pointer(p.0, p.1);
    g.click();
    *t += 0.04;
    g.step(*t);
}

/// `PUZ_CATAPULTE` from chapter 110's /030 (the console clicked): the game
/// runs; the play button; shots aimed (by the game's own flight) at the
/// right block until it breaks: `FLAG21` = 1, back on the console; the
/// exit: `CATA_CLIC_EXIT` = 1, and the chapter's /042 runs `FINPUZZLE`,
/// the film `CURTIS_DIEU` and chapter 11.
#[test]
fn catapulte_won_from_chapter_110() {
    use atlantis3_engine::puzzles2004::catapulte::{self as cata, Ball, Catapulte, Field, State};
    let Some((mut g, mut t)) = in_chapter(110) else {
        return;
    };
    g.unix_time = || 1_100_000_000;
    // The click scan of the console's object (`CtrlScenarIN` with
    // `CLICOBJET` set).
    let console = g
        .names()
        .id(atlantis2_data::defnames::OBJETS, b"CONSOLE_VILLAGE_FX")
        .unwrap();
    g.set_var("CLICOBJET", console as i32);
    g.ctrl_scenar_in();
    for _ in 0..3 {
        t += 0.04;
        g.step(t);
    }
    assert_eq!(g.mode(), Mode::Puzzle2004);
    assert_eq!(g.puzzle_2004(), Some(PuzzleId::Catapulte));
    assert_eq!(g.puzzles_2004_cfg()[1], 1);
    let ecran = bank(&g, "Cata_Ecran1.spr");
    let bloc = bank(&g, "Cata_bloc.spr");
    let mur = bank(&g, "Cata_mur.spr");
    if let Some(s) = g.puzzle_2004_surface() {
        write_ppm("catapulte_console", s, 800, 600);
    }
    let play = spot(&ecran, 4, &[3]);
    click_at(&mut g, &mut t, play);
    let c = g.puzzle_2004_state::<Catapulte>().unwrap();
    assert_eq!((c.state, c.screen), (State::Aim, 2));
    let mut shots = 0;
    while !g.puzzle_2004_state::<Catapulte>().unwrap().won {
        shots += 1;
        assert!(shots < 60, "not won in 60 shots");
        // Wait for the player's turn.
        while g.puzzle_2004_state::<Catapulte>().unwrap().state != State::Aim {
            t += 0.04;
            g.step(t);
        }
        t += 0.04;
        g.step(t);
        let c = g.puzzle_2004_state::<Catapulte>().unwrap();
        let field = Field {
            bloc: Some(&bloc),
            mur: Some(&mur),
            hits: c.hits,
        };
        let aim = (300..800)
            .step_by(4)
            .flat_map(|x| (0..600).step_by(4).map(move |y| (x, y)))
            .find(|&p| {
                let ball = Ball {
                    pos: cata::PLAYER_START,
                    vel: cata::aim_velocity(cata::PLAYER_START, (p.0 as f32, p.1 as f32)),
                    acc: (0.0, 0.0),
                    damp: 1.0,
                };
                // The frames last 0.04 s (the clock's steps, as f32).
                matches!(
                    cata::landing(ball, c.wind, &field, 0.04, 500),
                    Some((_, Some(2)))
                )
            })
            .expect("an aim at the right block");
        if shots == 1 {
            g.set_pointer(aim.0, aim.1);
            t += 0.04;
            g.step(t);
            if let Some(s) = g.puzzle_2004_surface() {
                write_ppm("catapulte_aim", s, 800, 600);
            }
        }
        click_at(&mut g, &mut t, aim);
        assert_eq!(
            g.puzzle_2004_state::<Catapulte>().unwrap().state,
            State::PlayerShot
        );
        while g.puzzle_2004_state::<Catapulte>().unwrap().state == State::PlayerShot {
            t += 0.04;
            g.step(t);
        }
    }
    assert_eq!(g.script().unwrap().flags[21], 1);
    // A second later, the console with the mark moved.
    while g.puzzle_2004_state::<Catapulte>().unwrap().state != State::Console {
        t += 0.04;
        g.step(t);
    }
    assert_eq!(
        g.puzzle_2004_state::<Catapulte>().unwrap().mark,
        cata::MARK_AFTER
    );
    let exit = spot(&ecran, 3, &[]);
    click_at(&mut g, &mut t, exit);
    for _ in 0..3 {
        t += 0.04;
        g.step(t);
    }
    // /042: FINPUZZLE, the film, chapter 11.
    assert_eq!(g.script().unwrap().var("CATA_CLIC_EXIT"), Some(2));
    assert_ne!(g.mode(), Mode::Puzzle2004);
    assert_eq!(g.puzzle_2004(), None);
}

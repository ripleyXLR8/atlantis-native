//! `fix_smooth_turn` on the 2004 data: between two 1/25 s look steps the
//! warp is drawn with the camera turned by the elapsed part of the next
//! step; with the fix off (`--original`) it is drawn as the camera is.
//! Needs `ATLANTIS4_CD`.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::{Fixes, Game};

fn game(fixes: Fixes) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
        atlantis3_data::testing::missing("ATLANTIS4_CD not set");
        return None;
    };
    let src = Arc::new(Evolution2004Source::new(Arc::new(
        atlantis_data::StdDir::new(&dir),
    )));
    let mut g = Game::with_fixes(A3Files::evolution_2004(src), fixes).unwrap();
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    Some(g)
}

/// Turning: a mouse move, then frames 1/100 s apart.
fn turning(g: &mut Game) -> Vec<([f32; 9], [f32; 9])> {
    let mut t = 1.0;
    for _ in 0..10 {
        t += 0.04;
        g.step(t);
    }
    g.mouse_motion(400, 0);
    let mut out = Vec::new();
    for _ in 0..12 {
        t += 0.01;
        g.step(t);
        out.push(g.test_matrices());
    }
    out
}

#[test]
fn the_turn_is_drawn_between_the_steps() {
    let Some(mut g) = game(Fixes::default()) else {
        return;
    };
    let frames = turning(&mut g);
    // Some frames fall between two look steps: drawn ahead of the camera.
    assert!(frames.iter().any(|(drawn, cam)| drawn != cam));
    // The drawn yaw moves at every frame while the camera's moves at the
    // 25 Hz steps only.
    let drawn: Vec<_> = frames.iter().map(|f| f.0).collect();
    assert!(drawn.windows(2).all(|w| w[0] != w[1]));
}

#[test]
fn the_original_draws_the_camera() {
    let Some(mut g) = game(Fixes::original()) else {
        return;
    };
    for (drawn, cam) in turning(&mut g) {
        assert_eq!(drawn, cam);
    }
}

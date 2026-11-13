//! *Atlantis Evolution*'s 2004 place animations (`ATLANTIS4_CD`): the
//! loader FUN_00439040, the view entry's pass (FUN_00442c80
//! @ 0x004431be..0x00443418), the step FUN_0043b0a0 and the hand-over
//! FUN_0043b420, in the ship's cabin (`cabine.wam` view 1, chapter 1).
//!
//! The cabin's states have first frame 0 and end 0; the porthole's
//! opening `HUBLOT_OUVERT` 2 (19 frames) names `A3` as its next state, and
//! `A3` (one frame) names itself. Without the chain the opening played
//! again and again; the program plays it once and shows the open porthole.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};

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

/// The cabin's view 1 after the intro, at clock `t`.
fn cabin() -> Option<(Game, f64)> {
    let mut g = game()?;
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
    assert_eq!(g.mode(), Mode::Warp);
    Some((g, 0.01))
}

fn slot(g: &Game, name: &str) -> usize {
    g.slots()
        .slots
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("{name} not loaded"))
}

#[test]
fn the_cabin_loads_its_chains() {
    let Some((g, _)) = cabin() else { return };
    let s = &g.slots().slots;
    assert!(g.slots().evolution);
    let names: Vec<String> = s.iter().map(|s| s.name.to_ascii_lowercase()).collect();
    assert_eq!(
        names,
        [
            "fna1001002",
            "fla1001001",
            "hua1001002",
            "hua3001002",
            "hua2001002",
            "mla3001002",
            "mla1001002"
        ]
    );
    // +0x1e: `A3` names the state itself; the opening names `A3`.
    let (a3, a2) = (slot(&g, "hua3001002"), slot(&g, "hua2001002"));
    assert_eq!(s[a3].next, Some(a3));
    assert!(s[a3].queued && s[a3].chain);
    assert_eq!(s[a2].next, Some(a3));
    // No next state: the sea and the candle.
    let sea = slot(&g, "fna1001002");
    assert_eq!(s[sea].next, None);
    assert!(!s[sea].queued && !s[sea].chain);
    // First frame 0 → −1 at the load, then the entry pass's loop: frame
    // +0x14 = 0; the rate the state's (15).
    for x in s.iter().filter(|x| x.shown) {
        assert_eq!(x.first, 0, "{}", x.name);
        assert_eq!(x.fps, 15, "{}", x.name);
        assert_eq!(x.mode, 1, "{}", x.name);
    }
    // `/INIT`: `hideobjet=HUBLOT_FERME`, `showobjet=HUBLOT_OUVERT`,
    // `playcutanimo=HUBLOT_OUVERT,3`; the trunk closed.
    assert!(s[a3].shown && !s[a2].shown);
    assert!(!s[slot(&g, "hua1001002")].shown);
    assert!(s[slot(&g, "mla1001002")].shown && !s[slot(&g, "mla3001002")].shown);
}

#[test]
fn the_porthole_opens_once() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    let (a3, a2) = (slot(&g, "hua3001002"), slot(&g, "hua2001002"));
    let (id, kind) = (g.slots().slots[a2].id, g.slots().slots[a2].kind);
    // chapi001 /009: `playcutanimo=HUBLOT_OUVERT,2` (hide, then show the
    // slot of animation 2).
    g.test_slots_mut().hide(id, kind, t);
    g.test_slots_mut().show(id, kind, 2, t);
    let start = t;
    let mut frames = Vec::new();
    // 25 frames a second for 4 s.
    for _ in 0..100 {
        t += 0.04;
        g.step(t);
        let s = &g.slots().slots;
        if s[a2].shown {
            frames.push(s[a2].frame);
            // The clock's frame: ftol(elapsed · 15).
            assert_eq!(s[a2].frame, ((t - start) * 15.0) as i32);
            assert!(!s[a3].shown);
        } else {
            // Handed over to `A3` at the end of the play, for good.
            assert!(s[a3].shown, "at {t}");
            assert_eq!(s[a3].frame, 0);
        }
    }
    // Frames 0..18 once (19 frames at 15 a second, 1.27 s).
    assert_eq!(frames.first(), Some(&0));
    assert_eq!(frames.iter().max(), Some(&18));
    assert!(frames.windows(2).all(|w| w[0] <= w[1]), "{frames:?}");
    assert!(frames.len() < 34, "{}", frames.len());
}

#[test]
fn the_sea_and_the_candle_loop() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    let candle = slot(&g, "fla1001001");
    let mut restarts = 0;
    let mut last = g.slots().slots[candle].frame;
    for _ in 0..250 {
        t += 0.04;
        g.step(t);
        let s = &g.slots().slots[candle];
        assert!(s.shown);
        assert!((0..60).contains(&s.frame) || s.frame == -1, "{}", s.frame);
        if s.frame < last {
            restarts += 1;
        }
        last = s.frame;
    }
    // 10 s of a 4 s loop.
    assert_eq!(restarts, 2);
}

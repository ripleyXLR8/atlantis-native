//! *Atlantis Evolution*'s 2004 music on the CD (`ATLANTIS4_CD`): a new
//! game's chapter 1 `/INIT` (after chapter 0's intro film) starts its three couches
//! (`startmusik=1,muzik20`, `=2,muzik21`, `=3,muzik24`, chapi001.con), each
//! a playlist of one Ogg file on disc 1, faded in from 0.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::Game;
use atlantis3_engine::music2004::Fade;

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

#[test]
fn chapter_1_starts_three_couches() {
    let Some(mut g) = game() else { return };
    assert!(g.new_game());
    // Chapter 0's `/INIT` waits for its intro film (`cinema=INTRO`,
    // `tests/film_2004.rs`): skipped, chapter 1's `/INIT` runs.
    assert!(g.music2004_opened.is_empty());
    g.skip();
    g.step(0.0);
    assert_eq!(
        g.music2004_opened,
        [
            "music/muzik20.ogg",
            "music/muzik21.ogg",
            "music/muzik24.ogg"
        ]
    );
    let names: Vec<&str> = g.music2004.slots.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "music/muzik20.mls",
            "music/muzik21.mls",
            "music/muzik24.mls",
            "",
            ""
        ]
    );
    for s in &g.music2004.slots[..3] {
        assert!(s.stream && s.loaded);
        assert_eq!(s.tracks.len(), 1);
    }
    // muzik20.mls: `muzik20.ogg vol=100 tpsfi=3000 tpsfo=1000`, faded in.
    let s = &g.music2004.slots[0];
    assert_eq!(
        (s.fade, s.cur, s.target, s.dur),
        (Fade::Up, 0.0, 100.0, 3000)
    );
    // muzik21.mls, muzik24.mls: `vol=40 tpsfi=0 tpsfo=0`, at once.
    for s in &g.music2004.slots[1..3] {
        assert_eq!((s.fade, s.cur), (Fade::None, 40.0));
    }
    // A second of frames: 15 ticks of the fade, the streams decoded.
    let mut t = 0.0;
    for _ in 0..60 {
        t += 1.0 / 60.0;
        g.step(t);
    }
    let s = &g.music2004.slots[0];
    assert_eq!(s.fade, Fade::Up);
    assert!((30.0..=36.0).contains(&s.cur), "{}", s.cur);
    assert!(g.music2004.slots[..3].iter().all(|s| s.volume > 0.0));
}

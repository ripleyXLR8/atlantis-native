//! *Atlantis Evolution*'s 2004 films on the CD (`ATLANTIS4_CD`): a new
//! game's chapter 0 `/INIT` plays `/set(cinema=INTRO,nocinein,nocineout)`
//! (chapi000.con): `#CINEMATIQUES#` gives `intro.bik`, the cue file
//! `bink\intro.txt` exists so the film is `loc\bink\gbr\intro.bik` on
//! disc 1, with its sound track `intro.ogg`; the section waits for the
//! film, then `anglewarpx`, `anglewarpy`, `vue=1` and `chapter=1` run: the
//! cabin (`cabine.wam`) at view 1.

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

/// The time of step `k`: frame k + 1's, 25 a second, a millisecond after.
fn at(k: u32) -> f64 {
    f64::from(k) * 0.04 + 0.001
}

fn numframevideo(g: &Game) -> Option<i32> {
    g.script().and_then(|s| s.var("NUMFRAMEVIDEO"))
}

#[test]
fn the_intro_plays_then_the_cabin() {
    let Some(mut g) = game() else { return };
    assert!(g.new_game());
    // The film opened, the section waits: no view entered yet, chapter 1's
    // music not started.
    assert_eq!(g.film_2004(), Some(("intro.bik", 1, 1584)));
    assert!(g.film_sound_playing(), "intro.ogg plays");
    assert_eq!(g.mode(), Mode::Video);
    assert_eq!(g.place(), ("cabine.wam", None));
    assert!(g.music2004_opened.is_empty());
    assert_eq!(numframevideo(&g), Some(0));

    // 25 frames a second: one frame a step of 1/25 s.
    for k in 1..=10 {
        g.step(at(k));
    }
    let (_, n, _) = g.film_2004().expect("still playing");
    assert_eq!(n, 11, "ten frames drawn, FrameNum on the 11th");
    // The 800 × 448 film in the 800 × 600 surface at y = 76, stretched to
    // 1024 × 768: rows 0..97 black.
    let w = g.width();
    let px = g.surface();
    let row_black = |y: usize| match px {
        atlantis3_engine::px::Surface::Rgb565(p) => p[y * w..(y + 1) * w].iter().all(|&c| c == 0),
        atlantis3_engine::px::Surface::Xrgb8888(p) => {
            p[y * w..(y + 1) * w].iter().all(|&c| c & 0xff_ffff == 0)
        }
    };
    assert!(row_black(0) && row_black(96));

    // Space (0x006e9e70): the film ends, `NUMFRAMEVIDEO` = `FrameNum`, the
    // sound track stops, the section goes on to the cabin's view 1.
    // (At this step frame 11 is due and drawn first.)
    g.skip();
    g.step(at(10) + 0.01);
    assert_eq!(g.film_2004(), None);
    assert!(!g.film_sound_playing());
    assert_eq!(numframevideo(&g), Some(11));
    assert_eq!(g.mode(), Mode::Warp);
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
    // `chapter=1`'s `/INIT` started its music.
    assert!(!g.music2004_opened.is_empty());
}

#[test]
fn a_click_stops_the_intro() {
    let Some(mut g) = game() else { return };
    assert!(g.new_game());
    g.step(0.0);
    g.click();
    g.step(0.01);
    assert_eq!(g.film_2004(), None);
    assert_eq!(numframevideo(&g), Some(2));
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
}

#[test]
fn the_intro_subtitles() {
    let Some(mut g) = game() else { return };
    assert!(g.new_game());
    // `#NEW 1 1 375 810 255 200 100`: dialog 1 line 1 from frame 375 to
    // frame 810, in (255, 200, 100).
    for k in 0..374 {
        g.step(at(k));
    }
    assert_eq!(g.film_2004().map(|f| f.1), Some(375));
    assert_eq!(g.test_subtitle(), None);
    g.step(at(374));
    // The 2004 `dial.txt`: `1 1 Narrateur It was a beautiful night ...`.
    let (text, rgb, caption) = g.test_subtitle().expect("the first line");
    assert!(text.starts_with(b"It was a beautiful night in June 1904."));
    assert_eq!((rgb, caption), ([255, 200, 100], false));
    // Hidden once frame 810 is drawn.
    for k in 375..809 {
        g.step(at(k));
    }
    assert!(g.test_subtitle().is_some());
    g.step(at(809));
    assert_eq!(g.film_2004().map(|f| f.1), Some(811));
    assert_eq!(g.test_subtitle(), None);
}

/// The whole film (1,584 frames): the natural end, the sound track (67 s,
/// longer than the film's 63.4 s) playing on.
#[test]
#[cfg_attr(debug_assertions, ignore = "1,584 Bink frames: run in release")]
fn the_intro_to_its_end() {
    let Some(mut g) = game() else { return };
    assert!(g.new_game());
    let mut steps = 0;
    while g.film_2004().is_some() {
        g.step(at(steps));
        steps += 1;
        assert!(steps < 2000);
    }
    assert_eq!(steps, 1584);
    assert_eq!(numframevideo(&g), Some(1584));
    assert!(g.film_sound_playing(), "intro.ogg plays on");
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
}

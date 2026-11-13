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

/// The cabin's view 1 after the intro (skipped).
fn cabin() -> Option<(Game, f64)> {
    let mut g = game()?;
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
    Some((g, 0.01))
}

fn row_black(g: &Game, y: usize) -> bool {
    let w = g.width();
    match g.surface() {
        atlantis3_engine::px::Surface::Rgb565(p) => p[y * w..(y + 1) * w].iter().all(|&c| c == 0),
        atlantis3_engine::px::Surface::Xrgb8888(p) => {
            p[y * w..(y + 1) * w].iter().all(|&c| c & 0xff_ffff == 0)
        }
    }
}

/// chapi018 /012: `CINEMA=TRAJECTOIRE_CINTRE1,nocinein,nocineout,traj006`
/// at `monstre.wam` view 5: the travel film `binktraj\trajcintre1.bik`
/// (disc 2, 800 × 600, 131 frames) with its Ogg track and the move 5 → 6's
/// transition sprites (`spf\ic005006.spr`); a click stops it with
/// `NUMFRAMEVIDEO` = `FrameNum`, then the warp's timer scan runs /030
/// (`numframevideo` < 100 with `flag94` = 1): `CINEMA=CINTRE_MORT_EAU1`
/// (`cintreeau1.bik`).
#[test]
fn the_cintre_travel_film() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    for c in [
        "(changecd=2)",
        "(changewam=monstre.wam)",
        "(chapter=18)",
        "(vue=5)",
    ] {
        g.run_command(c);
    }
    // The arrival's /001 (`dial=359,off`): clicks end it.
    for _ in 0..40 {
        if g.mode() == Mode::Dialog {
            g.click();
        }
        t += 0.04;
        g.step(t);
    }
    assert_eq!(g.place(), ("monstre.wam", Some(5)));
    assert_eq!(g.mode(), Mode::Warp);
    for c in [
        "(flag94=1)",
        "(testtimervideo=1)",
        "(testtime1=0)",
        "(testtime2=1)",
        "(time2=0)",
        "(cinema=TRAJECTOIRE_CINTRE1,nocinein,nocineout,traj006)",
    ] {
        g.run_command(c);
    }
    assert_eq!(g.film_2004(), Some(("trajcintre1.bik", 1, 131)));
    assert_eq!(g.film_cine_2004(), None, "a travel film has no cine-in");
    assert!(g.film_sound_playing(), "trajcintre1.ogg plays");
    assert_eq!(g.film_spf_2004(), 1, "ic005006.spr");
    for _ in 0..20 {
        t += 0.04;
        g.step(t);
    }
    assert_eq!(g.film_2004().map(|f| f.1), Some(21));
    g.click();
    t += 0.04;
    g.step(t);
    // `FrameNum` when the click is seen (frame 21, or 22 when due first).
    assert!(matches!(numframevideo(&g), Some(21 | 22)));
    // The film's end resumed nothing (a command, no section waiting); the
    // warp's frame runs the timer scan: /030.
    let mut next = None;
    for _ in 0..5 {
        t += 0.04;
        g.step(t);
        if let Some((name, _, _)) = g.film_2004() {
            next = Some(name.to_string());
            break;
        }
    }
    assert_eq!(next.as_deref(), Some("cintreeau1.bik"));
}

/// A `CINEMA=` without `nocinein` (120 of the scripts' lines): the 0.5 s
/// cine-in over the panorama first (FUN_00402fb0, FUN_00444870): black
/// bands of ftol(p · 76) rows growing by 2 · dt a pass, then the film.
#[test]
fn the_cine_in() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    g.run_command("(cinema=INTRO)");
    assert_eq!(g.mode(), Mode::Video);
    assert_eq!(g.film_2004(), None);
    assert_eq!(g.film_cine_2004(), Some((false, 0.0)));
    // The first pass: band 0, then p = 2 · 0.04.
    t += 0.04;
    g.step(t);
    let (out, p) = g.film_cine_2004().expect("cine-in");
    assert!(!out && (p - 0.08).abs() < 1e-4, "{p}");
    t += 0.04;
    g.step(t);
    // Band ftol(0.08 · 76) = 6: rows 0..6 and h − 6.. black, the panorama
    // between.
    let h = g.height();
    assert!(row_black(&g, 0) && row_black(&g, 5) && row_black(&g, h - 1));
    assert!(!row_black(&g, 6) && !row_black(&g, h / 2) && !row_black(&g, h - 7));
    let mut passes = 2;
    while g.film_2004().is_none() {
        t += 0.04;
        g.step(t);
        passes += 1;
        assert!(passes < 30);
    }
    // 0.5 s at 25 passes a second: p reaches 1.0 at the 13th pass, the
    // film opens at the next.
    assert_eq!(passes, 14);
    assert_eq!(g.film_cine_2004(), None);
    assert_eq!(g.film_2004().map(|f| f.0), Some("intro.bik"));
}

/// `cineout` (no CD script uses it): the film's end (here Space) runs the
/// cine-out (FUN_00403340: p from 1.0 down by 2 · dt a pass) with the film
/// still open and its track stopped only at the close (FUN_00403230),
/// then the mode back.
#[test]
fn the_cine_out() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    g.run_command("(cinema=INTRO,nocinein,cineout)");
    assert_eq!(g.film_2004().map(|f| f.0), Some("intro.bik"));
    t += 0.04;
    g.step(t);
    g.skip();
    t += 0.04;
    g.step(t);
    assert_eq!(g.film_cine_2004(), Some((true, 1.0)));
    assert!(g.film_2004().is_some(), "Bink closes after the cine-out");
    assert!(g.film_sound_playing());
    let mut passes = 0;
    while g.film_cine_2004().is_some() {
        t += 0.04;
        g.step(t);
        passes += 1;
        assert!(passes < 30);
    }
    assert_eq!(passes, 14);
    assert_eq!(g.film_2004(), None);
    assert!(!g.film_sound_playing(), "stopped by the player");
    assert_eq!(g.mode(), Mode::Warp);
}

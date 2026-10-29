//! A new game from the intro to the first interactive view, headless: the
//! intro plays to its end, the new game's arrival scan plays cinematics
//! 93, 1 and 2, then the Hoggar's view 4 is shown at yaw 108°, its zone
//! to view 5 under the centre; a click there turns the view, enters view
//! 5 and plays the travel video.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

fn game() -> Option<Game> {
    let dir = atlantis3_data::testing::install_dir()?;
    let files = A3Files::open_dir(&dir).unwrap().0;
    Some(Game::new(files).expect("the game starts"))
}

/// Steps at 60 frames a second from `*t` until `until` holds (at most
/// `max` seconds).
fn run(g: &mut Game, t: &mut f64, max: f64, until: impl Fn(&Game) -> bool) -> bool {
    let end = *t + max;
    while *t < end {
        *t += 1.0 / 60.0;
        g.step(*t);
        if until(g) {
            return true;
        }
    }
    false
}

#[test]
fn intro_then_new_game_to_the_hoggar() {
    let Some(mut g) = game() else { return };
    // The boot state: the funeral chamber's view 4, nothing shown.
    assert_eq!(g.place(), ("CFuner.wam", Some(4)));
    assert_eq!(g.mode(), Mode::Idle);
    let mut t = 0.0;
    g.start_intro();
    assert_eq!(g.mode(), Mode::Intro);
    assert_eq!(g.video().map(|v| v.0), Some("intro.hnm"));
    // 327 frames at 15 a second: about 21.8 s.
    assert!(run(&mut g, &mut t, 30.0, |g| g.mode() == Mode::Idle));
    assert!((21.0..23.0).contains(&t), "intro took {t} s");

    assert!(g.new_game());
    assert_eq!(g.place(), ("hoggar.wam", Some(4)));
    let done = run(&mut g, &mut t, 300.0, |g| {
        g.mode() == Mode::Warp && g.script().is_some_and(|s| s.flags[1000] == 1)
    });
    // `Flag1000` = 1 ends the section, after the third film.
    assert!(done, "the cinematics did not end");
    let (yaw, pitch) = g.angles_deg();
    assert!(
        (yaw - 108.0).abs() < 1e-3 && pitch.abs() < 1e-4,
        "{yaw} {pitch}"
    );
    assert_eq!(g.centre_zone(), Some(0));

    // The click on the zone to view 5: 14 turn ticks, then view 5 and the
    // travel video ho004005.hnm.
    g.click();
    assert!(run(&mut g, &mut t, 3.0, |g| g.mode() == Mode::Video));
    assert_eq!(g.place(), ("hoggar.wam", Some(5)));
    assert_eq!(
        g.video().map(|v| v.0.to_ascii_lowercase()).as_deref(),
        Some("ho004005.hnm")
    );
    assert!(run(&mut g, &mut t, 10.0, |g| g.mode() == Mode::Warp));
    let (yaw, pitch) = g.angles_deg();
    assert!(
        (yaw - 110.5056).abs() < 1e-3 && (pitch - 10.0).abs() < 1e-3,
        "{yaw} {pitch}"
    );
}

//! The French retail CD (three CDs, 2001) played from its installed
//! folder and its discs (`ATLANTIS3_CD_FR`, `ATLANTIS3_DISCS_FR`):
//!
//! - the intro (`intro.hnm`, the CD program's table), then a new game:
//!   `chapi600.con`'s `ChangeCD=2` runs the disc test (disc 2 current,
//!   `CurrentCD` 2 in the saves, no `CHANGECD` variable stored) and the
//!   Hoggar's first view is read from disc 2 (its panorama is on the
//!   discs only);
//! - the scarab case draws `Scarabée.spr` (the CD program's name, which
//!   the French data has: GOG.com's program draws nothing there).

use std::path::Path;
use std::sync::Arc;

use atlantis3_data::testing::cd_fr;
use atlantis3_data::{A3Files, Release};
use atlantis3_engine::{Game, Mode};
use atlantis_data::{FileSource, StdDir};

fn open(install: &Path, discs: &Path) -> Game {
    let files = A3Files::with_discs(
        Arc::new(StdDir::new(install)),
        install,
        &[discs.to_path_buf()],
    );
    Game::new(files).expect("the game starts")
}

#[test]
fn intro_new_game_and_the_first_disc() {
    let Some((install, discs)) = cd_fr() else {
        return;
    };
    let mut g = open(&install, &discs);
    assert_eq!(g.program_release(), Release::Cd2001);
    assert_eq!(g.startup.config.langue, b"fra");
    g.start_intro();
    assert_eq!(g.video().map(|v| v.0), Some("intro.hnm"));
    let mut t = 0.0;
    while t < 40.0 && g.mode() == Mode::Intro {
        t += 1.0 / 60.0;
        g.step(t);
    }
    assert_eq!(g.mode(), Mode::Idle);
    assert_eq!(g.files().discs().unwrap().current(), 0);
    // The panorama of the Hoggar's first view is not in the install.
    assert!(StdDir::new(&install)
        .list("Datas/warp/warp512")
        .iter()
        .all(|d| !d.eq_ignore_ascii_case("hoggar")));
    assert!(g.new_game());
    assert_eq!(g.files().discs().unwrap().current(), 2);
    let s = g.script().unwrap();
    // The disc test does not store the variable.
    assert_eq!(s.vars[s.keys.changecd], 0);
    assert!(s.wam.eq_ignore_ascii_case(b"hoggar.wam"), "{:?}", s.wam);
    // A few frames of the first view: its cube read from disc 2.
    for _ in 0..30 {
        t += 1.0 / 60.0;
        g.step(t);
    }
    assert!(
        !g.place_faces().any_empty(),
        "the Hoggar's cube from disc 2"
    );
    let name = g.save_game().unwrap();
    let text = String::from_utf8_lossy(g.saved(&name).unwrap()).into_owned();
    assert!(text.starts_with("CurrentCD = 2\n"), "{text}");
    // The loader's `(changecd=2)`: still disc 2, the variable not stored.
    assert!(g.load_save_text("again.sav", text.into_bytes()));
    assert_eq!(g.files().discs().unwrap().current(), 2);
}

/// The scarab case's first frame with the CD program's bank name.
#[test]
fn scarab_case_draws_on_the_cd() {
    let Some((install, discs)) = cd_fr() else {
        return;
    };
    let mut g = open(&install, &discs);
    assert!(g.new_game());
    g.flat_warp = true;
    g.trace = Some(Vec::new());
    let t = 1000.0;
    assert!(g.board_screen_start(13, t));
    assert!(g.board_screen_frame(t + 0.1, (400, 300), true, false));
    let calls: Vec<String> = g
        .trace
        .as_mut()
        .unwrap()
        .drain(..)
        .filter(|l| l.starts_with("affc spr\\Scarab"))
        .collect();
    assert_eq!(calls, ["affc spr\\Scarabée.spr#0@400,300"]);
}

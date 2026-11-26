//! The North American CD (*Beyond Atlantis II*, DreamCatcher, 2001)
//! played from its installed folder and its three CD images
//! (`ATLANTIS3_CD_US`, optional: `SKIPPED` without it):
//!
//! - the intro is the 2003 program's table: `intro_usa.hnm` alone for
//!   `usa` (atlantis3-cd-us-3366912 @ 0x00458d80), to its last frame;
//! - a new game runs `chapi600.con`'s `ChangeCD=2` as the French CD's
//!   program does (disc 2 current, the variable not stored, `CurrentCD`
//!   2 in the saves) and reads the Hoggar's first view from disc 2;
//! - the scarab case draws `Scarabee.spr` (the 2003 program's name);
//! - the fonts are `EASTEUROPE_CHARSET`'s (its `font.txt`), unlike GOG.com's
//!   English install (`SHIFTJIS_CHARSET`, drawn as ANSI).

use std::path::Path;
use std::sync::Arc;

use atlantis3_data::testing::cd_us;
use atlantis3_data::{A3Files, Release};
use atlantis3_engine::text::{FontId, Texts, CHARSET_EE};
use atlantis3_engine::{Game, Mode};
use atlantis_data::StdDir;

fn files(install: &Path, discs: &Path) -> A3Files {
    A3Files::with_discs(
        Arc::new(StdDir::new(install)),
        install,
        &[discs.to_path_buf()],
    )
}

fn open(install: &Path, discs: &Path) -> Game {
    Game::new(files(install, discs)).expect("the game starts")
}

#[test]
fn intro_new_game_and_the_first_disc() {
    let Some((install, discs)) = cd_us() else {
        return;
    };
    let mut g = open(&install, &discs);
    assert_eq!(g.program_release(), Release::CdUs2001);
    assert_eq!(g.startup.config.langue, b"usa");
    let frames = {
        let d = g.files().read("hnm/intro_usa.hnm").unwrap();
        atlantis3_data::hnm_header::HnmHeader::parse(&d)
            .unwrap()
            .frame_count()
    };
    g.start_intro();
    assert_eq!(g.video().map(|v| v.0), Some("intro_usa.hnm"));
    let (mut t, mut shown) = (0.0, 0);
    while t < 60.0 && g.mode() == Mode::Intro {
        t += 1.0 / 60.0;
        g.step(t);
        if let Some((n, f)) = g.video() {
            assert_eq!(n, "intro_usa.hnm");
            shown = f;
        }
    }
    assert_eq!(g.mode(), Mode::Idle);
    assert!(shown + 1 >= frames, "{shown} of {frames} frames");
    assert_eq!(g.files().discs().unwrap().current(), 0);
    assert!(g.new_game());
    assert_eq!(g.files().discs().unwrap().current(), 2);
    let s = g.script().unwrap();
    assert_eq!(s.vars[s.keys.changecd], 0);
    assert!(s.wam.eq_ignore_ascii_case(b"hoggar.wam"), "{:?}", s.wam);
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
    assert!(g.load_save_text("again.sav", text.into_bytes()));
    assert_eq!(g.files().discs().unwrap().current(), 2);
}

/// The scarab case's first frame with the 2003 program's bank name.
#[test]
fn scarab_case_draws_scarabee() {
    let Some((install, discs)) = cd_us() else {
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
    assert_eq!(calls, ["affc spr\\Scarabee.spr#0@400,300"]);
}

/// Every font in the Central European character set.
#[test]
fn central_european_fonts() {
    let Some((install, discs)) = cd_us() else {
        return;
    };
    let texts = Texts::load(&files(&install, &discs), "usa");
    for id in [
        FontId::Default,
        FontId::Bulle,
        FontId::Special,
        FontId::Titre,
        FontId::Credit,
    ] {
        for w in [640, 800, 1024] {
            let f = texts.gdi(id, w).unwrap_or_else(|| panic!("{id:?} {w}"));
            assert_eq!(f.charset, CHARSET_EE, "{id:?} {w}");
        }
    }
}

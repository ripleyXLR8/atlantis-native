//! The Portuguese CD (*Atlantis III: O Novo Mundo*, Playgames, 2002)
//! played from its installed folder and its three CD images
//! (`ATLANTIS3_CD_PT`, optional: `SKIPPED` without it):
//!
//! - the intro is the 2003 program's table, which for `por` is
//!   `intro.hnm` alone (atlantis3-cd-pt-3424256 @ 0x00459a00);
//! - a new game runs `chapi600.con`'s `ChangeCD=2` as the other CD programs
//!   do and reads the Hoggar's first view from disc 2;
//! - the scarab case draws `Scarabee.spr`, the only name the data has;
//! - the fonts are `ANSI_CHARSET`'s (`font.txt` is GOG.com's French and
//!   Spanish one).

use std::path::Path;
use std::sync::Arc;

use atlantis3_data::testing::cd_pt;
use atlantis3_data::{A3Files, Release};
use atlantis3_engine::text::{FontId, Texts};
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
    let Some((install, discs)) = cd_pt() else {
        return;
    };
    let mut g = open(&install, &discs);
    assert_eq!(g.program_release(), Release::CdPt2001);
    assert_eq!(g.startup.config.langue, b"por");
    let frames = {
        let d = g.files().read("hnm/intro.hnm").unwrap();
        atlantis3_data::hnm_header::HnmHeader::parse(&d)
            .unwrap()
            .frame_count()
    };
    g.start_intro();
    assert_eq!(g.video().map(|v| v.0), Some("intro.hnm"));
    let (mut t, mut shown) = (0.0, 0);
    while t < 60.0 && g.mode() == Mode::Intro {
        t += 1.0 / 60.0;
        g.step(t);
        if let Some((n, f)) = g.video() {
            assert_eq!(n, "intro.hnm");
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
    let Some((install, discs)) = cd_pt() else {
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

/// Every font in the ANSI character set.
#[test]
fn ansi_fonts() {
    let Some((install, discs)) = cd_pt() else {
        return;
    };
    let texts = Texts::load(&files(&install, &discs), "por");
    for id in [
        FontId::Default,
        FontId::Bulle,
        FontId::Special,
        FontId::Titre,
        FontId::Credit,
    ] {
        for w in [640, 800, 1024] {
            let f = texts.gdi(id, w).unwrap_or_else(|| panic!("{id:?} {w}"));
            assert_eq!(f.charset, 0, "{id:?} {w}");
        }
    }
}

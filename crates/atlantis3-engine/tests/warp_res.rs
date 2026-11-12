//! The folders WarpRes chooses (`Atlantis3.cfg`'s `WarpRes`, the
//! options' texture button; 0x00789d34). Each of the original's path
//! builders tests it for 1024 exactly: the cube `warp\Warp1024\`
//! (@ 0x00415a30), the place animations `spw\SPW1024\` (@ 0x004164c0),
//! and `HNM800\` for the cinematics (@ 0x00416880, @ 0x00416c60), the
//! dialogs' body videos (@ 0x00417280) and the travel videos
//! (@ 0x00417670), else `Warp512\`, `SPW512\`, `HNM640\`. GOG.com ships
//! none of the 1024 / 800 folders, so with WarpRes 1024 the original shows
//! the cube's debug colours, no place animation, and plays no cinematic
//! and no travel video (the travel's exists test @ 0x004402da).
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

fn game(warp_res: i32) -> Option<Game> {
    let dir = atlantis3_data::testing::install_dir()?;
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    g.startup.config.warp_res = warp_res;
    g.set_world(1);
    assert!(g.load_place("hoggar.wam"));
    Some(g)
}

/// Hoggar view 4, the move to view 5 (`ho004005.HNM`,
/// `tests/travel_escape.rs`) clicked and run: whether a travel video
/// started.
fn walk(g: &mut Game) -> bool {
    g.enter_at(4, 0.0, 0.0);
    g.set_harness_turns(true);
    let mut t = 10.0;
    t += 1.0 / 30.0;
    g.step(t);
    assert!(g.aim_at_zone(|z| z.kind == 1 && z.value == 5));
    g.click();
    for _ in 0..300 {
        if g.mode() == Mode::Video {
            return true;
        }
        t += 1.0 / 30.0;
        g.step(t);
    }
    assert_eq!(g.place().1, Some(5), "arrived");
    false
}

#[test]
fn warp_res_512_reads_the_shipped_folders() {
    let Some(mut g) = game(512) else {
        return;
    };
    assert!(walk(&mut g), "the travel video plays");
    assert!(g.board_cube().0.starts_with("warp/Warp512/hoggar/"));
    assert!(!g.slots().slots.is_empty());
    g.finish_video();
    g.run_command("(Cinema=93)");
    assert_eq!(g.mode(), Mode::Video, "the cinematic plays");
}

#[test]
fn warp_res_1024_reads_the_1024_folders_as_the_original() {
    let Some(mut g) = game(1024) else {
        return;
    };
    assert!(
        !walk(&mut g),
        "no travel video (hnmtraj\\HNM800\\ is not shipped)"
    );
    let (cube, faces) = g.board_cube();
    assert_eq!(cube, "warp/Warp1024/hoggar/ho0005.hnm");
    // The debug colour of face 0 (x8r8g8b8 0xff000080 in RGB565).
    assert_eq!(faces[0][0], 0x0010);
    assert!(g.slots().slots.is_empty(), "no place animation");
    // No cinematic either (hnm\HNM800\).
    g.run_command("(Cinema=93)");
    assert_ne!(g.mode(), Mode::Video);
}

/// GOG.com's Spanish install (`ATLANTIS3_GOG_ES`): its `Atlantis3.cfg`
/// says `WarpRes = 1024`, its folders are the 512 / 640 ones.
fn spanish(fixes: atlantis3_engine::Fixes) -> Option<Game> {
    let dir = atlantis3_data::testing::dir_of("ATLANTIS3_GOG_ES")?;
    let mut g = Game::with_fixes(A3Files::open_dir(&dir).unwrap().0, fixes).unwrap();
    g.set_world(1);
    assert!(g.load_place("hoggar.wam"));
    Some(g)
}

/// `fix_warpres_cd` on (the default): the options page's `Version = CD`
/// rule at start-up, so WarpRes 512 and the 640 x 480 view before the
/// first cube; the shipped folders play.
#[test]
fn spanish_gog_fix_on_starts_at_512() {
    let Some(mut g) = spanish(atlantis3_engine::Fixes::default()) else {
        return;
    };
    assert_eq!(g.startup.config.version, b"CD");
    assert_eq!(g.startup.config.warp_res, 512);
    assert_eq!(g.startup.warp_view(), (640, 480));
    assert!(walk(&mut g), "the travel video plays");
    assert!(g.board_cube().0.starts_with("warp/Warp512/hoggar/"));
    assert!(!g.slots().slots.is_empty());
}

/// `fix_warpres_cd` off (`--original`): the reader's 1024 and 800 x 600
/// stay, and every builder takes the 1024 / 800 folders GOG.com does not
/// ship, as the original until its options page is opened. Then that
/// page's `Version = CD` branch (@ 0x00456041) sets WarpRes 512 alone:
/// the view stays 800 x 600, and the next view entry reads `Warp512\`.
/// (How the original draws those 512 faces in the 1024 x 1024 surfaces
/// its start-up made is not known; the port's faces take the file's size.)
#[test]
fn spanish_gog_fix_off_takes_the_1024_paths() {
    let Some(mut g) = spanish(atlantis3_engine::Fixes::original()) else {
        return;
    };
    assert_eq!(g.startup.config.warp_res, 1024);
    assert_eq!(g.startup.warp_view(), (800, 600));
    assert!(!walk(&mut g), "no travel video (hnmtraj\\HNM800\\)");
    assert_eq!(g.board_cube().0, "warp/Warp1024/hoggar/ho0005.hnm");
    // The debug colour of face 0 on its 32-bit display (`PixelFormat =
    // 32bits`).
    let faces = g.place_faces().as_x888().expect("32-bit faces");
    assert_eq!(faces[0][0], 0xff00_0080, "debug colours");
    assert!(g.slots().slots.is_empty(), "no place animation");
    g.run_command("(Cinema=93)");
    assert_ne!(g.mode(), Mode::Video, "no film (hnm\\HNM800\\)");
    // The options page, sub-state 0.
    assert!(g.open_menu());
    g.menus_mut().open(atlantis3_engine::menu::PAGE_OPTIONS);
    let mut t = 100.0;
    for _ in 0..4 {
        t += 1.0 / 30.0;
        g.step(t);
    }
    assert_eq!(g.startup.config.warp_res, 512, "the CD branch");
    assert_eq!(g.startup.warp_view(), (800, 600), "the view stays");
    g.menu_back();
    assert_eq!(g.board_cube().0, "warp/Warp512/hoggar/ho0005.hnm");
}

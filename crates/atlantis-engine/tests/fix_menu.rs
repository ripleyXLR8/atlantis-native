//! The bug fixes the options menu lists for each game, read from the
//! game's data (`BugFix::of_game`: its build and language), and, with
//! `ATLANTIS_DUMP_DIR` set, pictures of the options menu and of its bug
//! fixes' box over a navigation view, at 640x480 and in HD
//! (`fixmenu-<name>-*.ppm`).

use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::hd::{rgb565_to_888, HdCompositor, HdFrame, HdOptions, HdText};
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::options_menu::{MenuValues, OptionsMenu};
use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::sys::{Key, Runner, Sys};
use atlantis_engine::warp::{hd_size, SCREEN_H, SCREEN_W};
use atlantis_engine::{
    BugFix, BugFixes, Engine, Enhancements, Event, FixData, Graphics, Options, Start,
};

use BugFix::*;

/// The game in the folder `var` names: an installed game or the CD's
/// files, else its CD images.
fn game(var: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(var) else {
        eprintln!("{var} not set: skipped");
        return None;
    };
    Game::open(Arc::new(StdDir::new(&dir)))
        .ok()
        .or_else(|| {
            let set = DiscSet::open_folder(dir.as_ref())?;
            Game::open(Arc::new(set)).ok()
        })
        .or_else(|| panic!("{var}: no game"))
}

/// Each data set's build-specific fixes (the fixes of every build come
/// first in every list). The patch 1.01's data set is installed over the
/// English CD: neither the German `ß` nor the Dutch discs' revisions.
const SETS: [(&str, &str, &[BugFix]); 17] = [
    ("ATLANTIS_DATA", "atlantis-cd-en-714240", &[]),
    ("ATLANTIS_GOG_FR", "atlantis-gog-714752", &[]),
    ("ATLANTIS_DATA_PATCH101", "atlantis-patch-fr-714240", &[]),
    (
        "ATLANTIS_DATA_1997",
        "atlantis-cd-en-688128",
        &[HexagonPieces],
    ),
    (
        "ATLANTIS_DATA_FR1997",
        "atlantis-cd-fr-687104",
        &[HexagonPieces],
    ),
    (
        "ATLANTIS_DATA_FR1997_MAY",
        "atlantis-cd-fr-688640",
        &[HexagonPieces, LoadMusik],
    ),
    (
        "ATLANTIS_DATA_DE1997",
        "atlantis-cd-de-687104",
        &[HexagonPieces, SharpS],
    ),
    (
        "ATLANTIS_ISO_DE",
        "atlantis-cd-de-687104",
        &[HexagonPieces, SharpS],
    ),
    (
        "ATLANTIS_DATA_DE_MAY97",
        "atlantis-cd-de-693248",
        &[HexagonPieces, SharpS, DiscRevision],
    ),
    (
        "ATLANTIS_DATA_EN_US97",
        "atlantis-cd-en-687104",
        &[HexagonPieces],
    ),
    (
        "ATLANTIS_DATA_NL1997",
        "atlantis-cd-nl-688128",
        &[HexagonPieces, DiscRevision],
    ),
    (
        "ATLANTIS_BIN_NL",
        "atlantis-cd-nl-688128",
        &[HexagonPieces, DiscRevision],
    ),
    (
        "ATLANTIS_DATA_SV1997",
        "atlantis-cd-sv-688128",
        &[HexagonPieces],
    ),
    (
        "ATLANTIS_DATA_ES1997",
        "atlantis-cd-es-687104",
        &[HexagonPieces],
    ),
    (
        "ATLANTIS_DATA_IT1997",
        "atlantis-cd-it-687104",
        &[HexagonPieces, MenuLineWidth],
    ),
    (
        "ATLANTIS_ISO_ZH_TW",
        "atlantis-cd-zh-tw-696832",
        &[HexagonPieces, SubtitlesToggle],
    ),
    (
        "ATLANTIS_BIN_ZH_CN",
        "atlantis-cd-zh-cn-692736",
        &[HexagonPieces, ChineseRows, SubtitlesToggle],
    ),
];

#[test]
fn the_fixes_of_each_game() {
    let every: Vec<BugFix> = BugFix::ALL
        .into_iter()
        .filter(|f| f.of_every_build())
        .collect();
    assert_eq!(every.len(), 8);
    for (var, build, own) in SETS {
        let Some(g) = game(var) else { continue };
        assert_eq!(g.exe.build.name, build, "{var}");
        let mut want = every.clone();
        want.extend_from_slice(own);
        assert_eq!(BugFix::of_game(&g), want, "{var}");
        // The data's own: the `ß` of the German texts, the revisions of
        // the Dutch discs.
        let data = FixData::of(&g);
        assert_eq!(data.sharp_s, own.contains(&SharpS), "{var}");
        assert_eq!(data.disc_revisions, own.contains(&DiscRevision), "{var}");
        let members = FixData::members(&g);
        assert!(
            members
                .iter()
                .any(|(n, d)| n == "SPRLIST\\EPI.TXT" && d.is_none()),
            "{var}: {members:?}"
        );
    }
}

/// Each place a fix changes reads that fix and no other: with every fix
/// alone, then every fix but one, each of the engine's readings follows
/// its own fix only (the 1997 English build, whose hexagon puzzle has the
/// fix to read, and the Italian build, whose menu line width has).
#[test]
fn each_place_reads_its_own_fix() {
    for var in ["ATLANTIS_DATA_1997", "ATLANTIS_DATA_IT1997"] {
        let Some(g) = game(var) else { continue };
        let build = g.exe.build;
        let sys = Sys::new(Mixer::new());
        let mut e = Engine::new(sys, g, Rc::new(DirectLoader));
        e.disc = 3;
        let mut cases = vec![BugFixes::NONE, BugFixes::ALL];
        for f in BugFix::ALL {
            cases.extend([BugFixes::only(f), BugFixes::all_but(f)]);
        }
        for fixes in cases {
            e.set_enhancements(Enhancements {
                fixes,
                ..Enhancements::default()
            });
            // Whether fix `g` is on in this case.
            let is = |g: BugFix| fixes.get(g);
            let ctx = format!("{var} {fixes:?}");
            assert_eq!(e.music_through_ring(), !is(MusicLoop), "{ctx}");
            assert_eq!(e.head_overlay_drawn(), is(HeadOverlay), "{ctx}");
            assert_eq!(e.head_rest_key_fix(), is(HeadFirstFrame), "{ctx}");
            let bar = e.bar_state();
            assert_eq!(bar.slide_out_fix, is(BarSlideOut), "{ctx}");
            assert_eq!(bar.pickup_fix, is(PickupIcon), "{ctx}");
            assert_eq!(e.menus_own_records(), is(MenuRecords), "{ctx}");
            assert_eq!(e.keep_fifth_name(), is(FifthPlayer), "{ctx}");
            assert_eq!(e.snakes_first_run_fix(), is(SnakesFirstWin), "{ctx}");
            assert!(!build.hexa_forces_402);
            assert_eq!(e.hexa_forces_402(), is(HexagonPieces), "{ctx}");
            assert_eq!(e.loadmusik_track_by_track(), is(LoadMusik), "{ctx}");
            assert_eq!(e.sharp_s_as_ss(), is(SharpS), "{ctx}");
            let disc = (!is(DiscRevision)).then_some(3);
            assert_eq!(e.current_disc_copy(), disc, "{ctx}");
            let rules = e.menu_rules();
            assert_eq!(rules.sharp_s_as_ss, is(SharpS), "{ctx}");
            let width = if is(MenuLineWidth) {
                22
            } else {
                i32::from(build.menu_positioned_byte_width)
            };
            assert_eq!(rules.positioned_byte_width, width, "{ctx}");
            assert_eq!(rules.toggle_in_step, is(SubtitlesToggle), "{ctx}");
            assert_eq!(e.chinese_row_fix(), is(ChineseRows), "{ctx}");
        }
    }
}

const TILE: usize = 48;

fn write_ppm(dir: &std::path::Path, name: &str, w: usize, h: usize, px: &[u32]) {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for p in px {
        out.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
    }
    let path = dir.join(format!("{name}.ppm"));
    std::fs::write(&path, out).expect("cannot write the dump");
    eprintln!("wrote {}", path.display());
}

/// A new game run to its first navigation view in HD, then the options
/// menu (its "Choose the bug fixes..." row selected, one fix off) and the
/// bug fixes' box (the last build-specific fix selected), each at 640x480
/// and composed in HD.
fn dump(var: &str, name: &str) {
    let Some(dir) = std::env::var_os("ATLANTIS_DUMP_DIR") else {
        eprintln!("ATLANTIS_DUMP_DIR not set: {name} skipped");
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let Some(g) = game(var) else { return };
    let edition = g.edition().to_string();
    let fixes = BugFix::of_game(&g);
    let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
    let sys = Sys::new(Mixer::new());
    sys.set_hd_tile(Some(TILE));
    let opts = Options {
        trace: Some(trace.clone()),
        start: Start::NewGame,
        ..Options::default()
    };
    let mut runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
    let mut sound = vec![0.0f32; 2 * (22050 * 16 / 1000)];
    let mut after = None::<u32>;
    for frame in 0u64.. {
        assert!(runner.step(frame * 16), "the engine ended");
        sys.mixer.fill(&mut sound, 22050, 2);
        assert!(frame < 60_000, "no navigation reached");
        if after.is_none()
            && trace
                .borrow()
                .iter()
                .any(|e| matches!(e, Event::PlaceEntered { .. }))
        {
            after = Some(0);
        }
        if let Some(n) = after.as_mut() {
            *n += 1;
            if *n == 120 {
                break;
            }
        }
    }
    let mut x = Enhancements::default();
    // One fix off: the "Fix original bugs" row shows Custom.
    x.fixes.set(*fixes.last().unwrap(), false);
    let values = MenuValues {
        graphics: Graphics::Hd,
        hd: HdOptions::default(),
        enhancements: x,
        fullscreen: false,
    };
    let mut menu = OptionsMenu::new();
    menu.set_info(vec![
        format!("Game: {edition}"),
        "Atlantis Native".to_string(),
    ]);
    menu.set_game(Some(&edition), Some(fixes.clone()));
    menu.open();
    for _ in 0..8 {
        menu.key(Key::Down, values);
    }
    assert_eq!(
        menu.selected(),
        atlantis_engine::options_menu::Entry::ChooseFixes
    );
    let font = SubtitleFont::new();
    let mut pictures = Vec::new();
    let mut draw = |menu: &OptionsMenu, shot: &'static str| {
        let mut frame = sys.screen.borrow().clone();
        let mut text = HdText::default();
        menu.draw_recording(&mut frame, &font, values, Some(&mut text));
        pictures.push((shot, frame, text));
    };
    draw(&menu, "menu");
    menu.key(Key::Enter, values);
    assert!(menu.in_fixes());
    for _ in 0..fixes.len() + 1 {
        menu.key(Key::Down, values);
    }
    draw(&menu, "fixes");
    let lut = rgb565_to_888();
    for (shot, frame, text) in &pictures {
        let px: Vec<u32> = frame.iter().map(|&p| lut[usize::from(p)]).collect();
        write_ppm(
            &dir,
            &format!("fixmenu-{name}-{shot}-640x480"),
            SCREEN_W,
            SCREEN_H,
            &px,
        );
        let (w, h) = hd_size(TILE);
        let mut out = vec![0u32; w * h];
        let layer = sys.hd.borrow();
        let texts: Vec<&HdText> = vec![text];
        let f = HdFrame {
            screen: frame,
            layer: sys.hd_shown().then_some(&*layer),
            texts: &texts,
            photo: sys.photo_shown(),
            video_pano: None,
        };
        HdCompositor::new().compose(f, HdOptions::default(), &mut out, w, 0, 0, w, h);
        write_ppm(&dir, &format!("fixmenu-{name}-{shot}-hd"), w, h, &out);
    }
}

/// The French CD (1997 build): the hexagon pieces' fix besides those of
/// every build.
#[test]
fn dump_french_cd() {
    dump("ATLANTIS_DATA_FR1997", "fr-cd");
}

/// The French May 1997 build: LOADMUSIK's fix too.
#[test]
fn dump_french_may_1997() {
    dump("ATLANTIS_DATA_FR1997_MAY", "fr-may-1997");
}

/// The Simplified Chinese CD: eleven fixes, rows of 20 pixels.
#[test]
fn dump_simplified_chinese() {
    dump("ATLANTIS_BIN_ZH_CN", "zh-cn");
}

//! *Atlantis Evolution*'s 2004 program's start-up, menus and saves
//! (`ATLANTIS4_CD`; atlantis4-re `subsystems/cd2004-menus.md`): the logos
//! (FUN_0045e970) then the start page (FUN_00457850); player 1's new game
//! from it (chapter 0, the cabin); Escape's in-game menu (FUN_00459ea0), a
//! save in the cabin (FUN_0045c4b0 → FUN_0045f080) and its load from the
//! load page (FUN_00458b30 → FUN_00460b50) bringing the view, the flags,
//! the variables, the bag and the music back; the options' subtitle toggle
//! and `Atlantis4.cfg`'s writer (FUN_0040c940).
//!
//! With `ATLANTIS4_SHOTS=<folder>` the start page and the save list are
//! written there as PNG.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};
use atlantis3_engine::menu::{PAGE_LOAD, PAGE_MAIN, PAGE_OPTIONS, PAGE_SAVE, PAGE_START};
use atlantis3_engine::px::Surface;

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

fn shot(g: &Game, name: &str) {
    let Some(dir) = std::env::var_os("ATLANTIS4_SHOTS") else {
        return;
    };
    let (w, h) = (g.width(), g.height());
    let png = match g.surface() {
        Surface::Rgb565(s) => atlantis2_engine::png::encode_rgb565(s, w, h),
        Surface::Xrgb8888(s) => atlantis2_engine::png::encode_xrgb8888(s, w, h),
    };
    let path = std::path::Path::new(&dir).join(name);
    std::fs::write(&path, png).expect("the screenshot");
    eprintln!("screenshot {}", path.display());
}

/// A point of the screen where a click on page `page` hits `name`.
fn point_of(g: &Game, page: u8, name: &[u8]) -> (i32, i32) {
    let p = g.menus().get(page).expect("the page");
    for y in (0..g.height() as i32).step_by(3) {
        for x in (0..g.width() as i32).step_by(3) {
            if p.clicked((x, y)) == Some(name) && p.hit((x, y)).is_some() {
                return (x, y);
            }
        }
    }
    panic!("{} not on the page", String::from_utf8_lossy(name));
}

/// A click (press, then release) on `name` of the page shown, with the
/// frames around it.
fn click_on(g: &mut Game, t: &mut f64, name: &[u8]) {
    let page = g.menu_page().expect("a menu page").0;
    let (x, y) = point_of(g, page, name);
    g.set_pointer(x, y);
    *t += 0.02;
    g.step(*t);
    g.click();
    *t += 0.02;
    g.step(*t);
    g.release();
    *t += 0.02;
    g.step(*t);
}

fn frames(g: &mut Game, t: &mut f64, n: usize) {
    for _ in 0..n {
        *t += 0.02;
        g.step(*t);
    }
}

/// The logos (`logos.txt`: `logoTAC.bik`, `logoAIE.bik`, `logo.bik` from
/// `loc\bink\gbr\`), each ended by a skip, then the start page in loop
/// mode 7 with the player 0.
fn to_start_page(g: &mut Game, t: &mut f64) {
    g.start_logos_2004();
    g.step(*t);
    for want in ["logoTAC.bik", "logoAIE.bik", "logo.bik"] {
        assert_eq!(g.film_2004().map(|f| f.0), Some(want));
        assert_eq!(g.mode(), Mode::Video);
        frames(g, t, 3);
        g.skip();
        *t += 0.02;
        g.step(*t);
    }
    assert!(!g.logos_running_2004());
    assert_eq!(g.menu_page(), Some((PAGE_START, 0)));
}

#[test]
fn the_start_up_reaches_the_start_page() {
    let Some(mut g) = game() else { return };
    let mut t = 0.0;
    to_start_page(&mut g, &mut t);
    let p = g.menus().get(PAGE_START).unwrap();
    let names: Vec<&[u8]> = p.items.iter().map(|it| &it.el.nomref[..]).collect();
    assert_eq!(
        names,
        [
            &b"FOND"[..],
            b"BT_LOAD1",
            b"BT_LOAD2",
            b"BT_LOAD3",
            b"BT_LOAD4",
            b"BT_LOAD5",
            b"BT_QUIT"
        ]
    );
    assert!(p.items.iter().all(|it| it.bank.is_some()));
    // The pointer away from the buttons: everything at rest.
    g.set_pointer(5, 5);
    frames(&mut g, &mut t, 2);
    let d = &g.menus().draws;
    assert_eq!(d.len(), 7);
    assert_eq!(
        (d[1].bank.as_str(), d[1].frame, d[1].pos),
        ("ng_btna_", 0, Some((310, 257)))
    );
    shot(&g, "a4cd_start_page.png");
    // Hovered: a `BT_ANIME3` plays its frames from 2.
    let (x, y) = point_of(&g, PAGE_START, b"BT_LOAD1");
    g.set_pointer(x, y);
    frames(&mut g, &mut t, 2);
    assert!(g.menus().draws[1].frame >= 2, "{:?}", g.menus().draws[1]);
}

#[test]
fn new_game_save_and_load_from_the_menus() {
    let Some(mut g) = game() else { return };
    let mut t = 0.0;
    to_start_page(&mut g, &mut t);
    // Escape on the start page with no player: the in-game menu with
    // `BT_LOAD`, `BT_SAVE` and `BT_BACK` off (FUN_0042c560).
    g.escape();
    assert_eq!(g.menu_page().map(|p| p.0), Some(PAGE_MAIN));
    let p = g.menus().get(PAGE_MAIN).unwrap();
    let k = p.find(b"BT_SAVE").unwrap();
    assert!(!p.items[k].el.clickable);
    click_on(&mut g, &mut t, b"BT_QUIT");
    assert_eq!(g.menu_page().map(|p| p.0), Some(PAGE_START));

    // Player 1, no save: chapter 0 (`cinema=INTRO`, skipped), the cabin.
    click_on(&mut g, &mut t, b"BT_LOAD1");
    assert_eq!(g.player(), 1);
    assert_eq!(g.film_2004().map(|f| f.0), Some("intro.bik"));
    g.skip();
    frames(&mut g, &mut t, 2);
    assert_eq!(g.mode(), Mode::Warp);
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
    let couches: Vec<String> = g.music2004.slots.iter().map(|s| s.name.clone()).collect();
    assert!(couches[0].ends_with("muzik20.mls"), "{couches:?}");

    // A state to save: a flag, a variable, an object in the bag.
    let obj = 3;
    {
        let s = g.script_mut().unwrap();
        s.flags[1234] = 77;
        s.vars[250] = -5;
        let world = s.a3.world;
        s.bag.push(atlantis2_engine::script::BagEntry {
            sprite: 0,
            id: obj,
            world,
        });
    }
    let bag: Vec<i32> = g.script().unwrap().bag.iter().map(|e| e.id).collect();
    assert!(bag.contains(&obj));
    // Escape: the in-game menu (the screen captured), `BT_SAVE`, `BT_YES`.
    g.escape();
    assert_eq!(g.menu_page(), Some((PAGE_MAIN, 0)));
    click_on(&mut g, &mut t, b"BT_SAVE");
    assert_eq!(g.menu_page().map(|p| p.0), Some(PAGE_SAVE));
    frames(&mut g, &mut t, 2);
    shot(&g, "a4cd_save_page.png");
    click_on(&mut g, &mut t, b"BT_YES");
    assert_eq!(g.menu_page().map(|p| p.0), Some(PAGE_MAIN));
    let saved = g.take_written_saves();
    let names: Vec<&str> = saved.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["game1_1.sav", "spr_1024/game1_1.sav.spr"]);
    let text = String::from_utf8_lossy(&saved[0].1).into_owned();
    assert!(text.starts_with("CurrentCD = 1\nDateAndTime = "), "{text}");
    assert!(text.contains("\nWam = cabine.wam\n"), "{text}");
    assert!(text.contains("\n1234 = 77\n"));
    assert!(
        text.contains(&format!("\nNBObjet = {}\n", bag.len())),
        "{text}"
    );
    assert!(text.contains("\nNBSpwHideTable = 0\nNBZoneTable = 0\nSprPleinEcran = \n"));
    assert!(saved[1].1.starts_with(b"SPRCRYO1"));

    // Back to the game; the state changed.
    click_on(&mut g, &mut t, b"BT_BACK");
    assert_eq!(g.mode(), Mode::Warp);
    {
        let s = g.script_mut().unwrap();
        s.flags[1234] = 0;
        s.vars[250] = 0;
        s.bag.clear();
    }
    // Escape, `BT_LOAD`, the list (one save), `SAVE1`.
    g.escape();
    click_on(&mut g, &mut t, b"BT_LOAD");
    assert_eq!(g.menu_page().map(|p| p.0), Some(PAGE_LOAD));
    frames(&mut g, &mut t, 3);
    let list: Vec<&str> = g.menus().list().map(|(n, _)| n).collect();
    assert_eq!(list, ["game1_1.sav"]);
    shot(&g, "a4cd_load_page.png");
    click_on(&mut g, &mut t, b"SAVE1");
    frames(&mut g, &mut t, 2);
    assert_eq!(g.mode(), Mode::Warp);
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
    let s = g.script().unwrap();
    assert_eq!((s.flags[1234], s.vars[250]), (77, -5));
    assert_eq!(s.bag.iter().map(|e| e.id).collect::<Vec<_>>(), bag);
    let couches: Vec<String> = g.music2004.slots.iter().map(|s| s.name.clone()).collect();
    assert!(couches[0].ends_with("muzik20.mls"), "{couches:?}");
}

#[test]
fn the_options_toggle_the_subtitles_and_write_the_cfg() {
    let Some(mut g) = game() else { return };
    let mut t = 0.0;
    assert!(g.new_game());
    g.skip();
    frames(&mut g, &mut t, 2);
    let on = g.startup.config.subtitle_on;
    g.escape();
    click_on(&mut g, &mut t, b"BT_OPTION");
    assert_eq!(g.menu_page(), Some((PAGE_OPTIONS, 0)));
    // The options are set up in sub-state 0: `BT_TITRE` at 2 when on.
    frames(&mut g, &mut t, 1);
    let p = g.menus().get(PAGE_OPTIONS).unwrap();
    let k = p.find(b"BT_TITRE").unwrap();
    assert_eq!(p.items[k].frame, if on { 2 } else { 0 });
    click_on(&mut g, &mut t, b"BT_TITRE");
    assert_eq!(g.startup.config.subtitle_on, !on);
    click_on(&mut g, &mut t, b"BT_BACK");
    assert_eq!(g.menu_page().map(|p| p.0), Some(PAGE_MAIN));
    // Every click of the in-game menu writes `Atlantis4.cfg`.
    click_on(&mut g, &mut t, b"BT_OPTION");
    let cfg = g
        .menus()
        .cfg_written
        .clone()
        .expect("Atlantis4.cfg written");
    let text = String::from_utf8_lossy(&cfg);
    assert_eq!(text.contains("SubTitleOn\t\t= false\r\n"), on, "{text}");
    let back = atlantis3_data::evolution2004_cfg::Config::parse(&cfg);
    assert_eq!(back.subtitle_on, !on);
}

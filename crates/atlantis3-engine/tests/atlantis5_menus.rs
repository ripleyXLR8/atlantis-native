//! *Atlantis V*'s menus played headless (`ATLANTIS5_DVD_FR`: the folder
//! holding `datas/`, the French DVD's cabinets extracted; skipped when
//! unset): the main menu at the start, a new game, the in-game menu, the
//! save and load pages, the option pages, the end of a game, the death
//! panel, the examine view and the credits. Saves go to a fresh folder
//! under `ATLANTIS_NATIVE_DATA` (else the system's temporary folder).
//! Pictures go to `ATLANTIS5_SHOT_DIR` when set. The texts are compared
//! with the data's own strings, read at run time.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_menu as menus;
use atlantis3_engine::atlantis5_play::Play;

fn dvd() -> Option<PathBuf> {
    match std::env::var_os("ATLANTIS5_DVD_FR") {
        Some(d) => Some(PathBuf::from(d)),
        None => {
            eprintln!("SKIPPED: ATLANTIS5_DVD_FR is not set");
            None
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let base =
        std::env::var_os("ATLANTIS_NATIVE_DATA").map_or_else(std::env::temp_dir, PathBuf::from);
    let d = base.join(format!("a5-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch folder");
    d
}

fn shot(play: &Play, name: &str) -> Vec<u32> {
    let mut px = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
    play.render(&mut px);
    if let Some(dir) = std::env::var_os("ATLANTIS5_SHOT_DIR") {
        let png = atlantis2_engine::png::encode_xrgb8888(&px, VIEW_WIDTH, VIEW_HEIGHT);
        let _ = std::fs::write(PathBuf::from(dir).join(name), png);
    }
    px
}

fn frames(play: &mut Play, n: usize) {
    for _ in 0..n {
        play.update(1.0 / 30.0);
    }
}

fn booted(d: &std::path::Path, saves: &std::path::Path) -> Play {
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let mut play = Play::new(src, "french", None).expect("play");
    play.set_save_dir(Some(saves.to_path_buf()));
    play.boot();
    // The logos (skipped as films), then the main menu.
    for _ in 0..10 {
        if play.stage().film_playing() {
            play.skip();
        }
        frames(&mut play, 1);
        if play.menus.current().is_some() {
            break;
        }
    }
    play
}

/// The element `name` as shown: (text, visible, x, y).
fn element(play: &Play, name: &str) -> (String, bool, i32, i32) {
    play.menus
        .shown()
        .into_iter()
        .find(|e| e.0 == name)
        .map(|e| (e.1, e.2, e.3, e.4))
        .unwrap_or_else(|| panic!("no element {name}"))
}

fn click(play: &mut Play, name: &str) {
    assert!(play.click_menu(name), "{name} shown");
    frames(play, 1);
}

/// From the main menu, a new game to its first view (its film ended).
fn new_game(play: &mut Play) {
    click(play, "new_game");
    for _ in 0..30 {
        frames(play, 1);
        if play.stage().film_playing() {
            play.skip();
        }
        if play.game.state() == 4 && !play.game.busy() && play.stage().address().is_some() {
            break;
        }
    }
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 3)));
    assert_eq!(play.game.state(), 4);
}

#[test]
fn the_game_starts_in_the_main_menu() {
    let Some(d) = dvd() else { return };
    let dir = scratch("main-menu");
    let mut play = booted(&d, &dir);
    assert_eq!(play.game.state(), 3);
    assert_eq!(play.menus.current(), Some(menus::MAIN));
    let px = shot(&play, "a5-main-menu.png");
    assert!(px.iter().any(|&p| p & 0x00ff_ffff != 0), "the background");
    // The buttons' texts are the data's strings; `quit_game` shows
    // `quit_atlantis`.
    let s = |k: &str| play.menus.strings.get(k).unwrap_or("").to_string();
    let (t, v, x, y) = element(&play, "new_game");
    assert_eq!((t, v, x, y), (s("new_game"), true, 100, 250));
    assert_eq!(element(&play, "quit_game").0, s("quit_atlantis"));
    // No autosave: `rescue_game` hidden, `quit_game` right below
    // `load_game`, `credits` below it.
    let (_, rescue, ..) = element(&play, "rescue_game");
    assert!(!rescue);
    let (_, _, _, ly) = element(&play, "load_game");
    let (_, _, _, qy) = element(&play, "quit_game");
    let (_, _, _, cy) = element(&play, "credits");
    assert!(ly > 250 && qy > ly && cy > qy, "{ly} {qy} {cy}");
    assert_eq!(qy - ly, cy - qy - 20, "20 then 40 below a text's height");
    new_game(&mut play);
    assert_eq!(play.menus.current(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_in_game_menu_saves_and_the_load_page_reads_it() {
    let Some(d) = dvd() else { return };
    let dir = scratch("save-load");
    let mut play = booted(&d, &dir);
    new_game(&mut play);
    // Escape: the in-game menu.
    play.escape();
    frames(&mut play, 1);
    assert_eq!(play.game.state(), 3);
    assert_eq!(play.menus.current(), Some(menus::IN_GAME));
    let _ = shot(&play, "a5-in-game-menu.png");
    assert!(!element(&play, "exit_yes").1, "the confirmation hidden");
    // The save page, slot 3.
    click(&mut play, "save");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::SAVE));
    assert_eq!(play.game.state(), 3, "the menus go on in state 3");
    let empty = play.menus.strings.get("empty").unwrap_or("").to_string();
    assert!(element(&play, "save_3").0.ends_with(&empty));
    click(&mut play, "save_3");
    frames(&mut play, 1);
    let text = std::fs::read_to_string(dir.join("slot_3")).expect("slot_3 written");
    for section in ["<LUA_DB>", "<CONFIGURATION ", "<CAMERA ", "<SOUND>"] {
        assert!(text.contains(section), "{section}");
    }
    assert!(text.contains("res_path=\"./datas/\""));
    assert!(
        !element(&play, "save_3").0.ends_with(&empty),
        "the label made again with its date"
    );
    let _ = shot(&play, "a5-save-page.png");
    // Back to the in-game menu, then `resume`: play again.
    click(&mut play, "back");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::IN_GAME));
    click(&mut play, "resume");
    frames(&mut play, 2);
    assert_eq!(play.menus.current(), None);
    assert_eq!(play.game.state(), 4);
    // Elsewhere, then the end of the game from the in-game menu: the main
    // menu.
    assert!(play
        .game
        .run_string("return function() Atl_Address_Go(\"zeppelin\", 8, \"no_fx\") end"));
    frames(&mut play, 30);
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 8)));
    play.escape();
    frames(&mut play, 1);
    click(&mut play, "exit");
    assert!(element(&play, "exit_yes").1 && !element(&play, "save").1);
    click(&mut play, "exit_no");
    assert!(!element(&play, "exit_yes").1 && element(&play, "save").1);
    click(&mut play, "exit");
    click(&mut play, "exit_yes");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::MAIN));
    assert_eq!(play.game.state(), 3);
    assert_eq!(play.stage().address(), None, "the scene unloaded");
    // The load page: slot 3 read, view 3 again.
    click(&mut play, "load_game");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::LOAD));
    let _ = shot(&play, "a5-load-page.png");
    click(&mut play, "save_3");
    for _ in 0..60 {
        frames(&mut play, 1);
        if play.game.state() == 4 && !play.game.busy() {
            break;
        }
    }
    assert_eq!(play.menus.current(), None);
    assert_eq!(play.game.state(), 4);
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 3)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two presses in one long frame (a double click at a low frame rate):
/// the one that closes the menu is its last, as when each press has a
/// frame of its own at the screen's refresh.
#[test]
fn a_double_click_in_one_frame_clicks_once() {
    let Some(d) = dvd() else { return };
    let dir = scratch("double-click");
    let mut play = booted(&d, &dir);
    new_game(&mut play);
    play.escape();
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::IN_GAME));
    assert!(play.click_menu("save"));
    play.click();
    frames(&mut play, 2);
    assert_eq!(play.menus.current(), Some(menus::SAVE));
    click(&mut play, "back");
    frames(&mut play, 1);
    assert_eq!(
        play.menus.current(),
        Some(menus::IN_GAME),
        "one save page, not two"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A slot that could not be written keeps its earlier autosave copy (the
/// page's `autoslot_<n>` replacement is skipped), so the two stay a pair.
#[test]
fn a_failed_slot_save_leaves_its_autosave_copy() {
    let Some(d) = dvd() else { return };
    let dir = scratch("save-fails");
    let mut play = booted(&d, &dir);
    new_game(&mut play);
    std::fs::write(dir.join("autoslot_3"), b"old").expect("autoslot_3");
    // `slot_3.tmp` taken by a folder: the slot cannot be written.
    std::fs::create_dir_all(dir.join("slot_3.tmp")).expect("blocker");
    play.escape();
    frames(&mut play, 1);
    click(&mut play, "save");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::SAVE));
    click(&mut play, "save_3");
    frames(&mut play, 1);
    assert!(!dir.join("slot_3").exists());
    assert_eq!(std::fs::read(dir.join("autoslot_3")).unwrap(), b"old");
    // Written: the autosave copied with it.
    std::fs::remove_dir(dir.join("slot_3.tmp")).expect("blocker gone");
    click(&mut play, "save_3");
    frames(&mut play, 1);
    assert!(dir.join("slot_3").is_file());
    assert_eq!(
        std::fs::read(dir.join("autoslot_3")).ok(),
        std::fs::read(dir.join("autoslot")).ok()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unreadable_save_keeps_its_page() {
    let Some(d) = dvd() else { return };
    let dir = scratch("unreadable");
    // Present but not an `ATL_FILE`: an empty slot, a cut autosave.
    std::fs::write(dir.join("slot_3"), b"").expect("slot_3");
    std::fs::write(dir.join("autoslot"), b"<ATL_FILE type=\"").expect("autoslot");
    let mut play = booted(&d, &dir);
    assert_eq!(play.menus.current(), Some(menus::MAIN));
    click(&mut play, "rescue_game");
    frames(&mut play, 3);
    assert_eq!(play.game.state(), 3);
    assert_eq!(
        play.menus.current(),
        Some(menus::MAIN),
        "the main menu again"
    );
    click(&mut play, "load_game");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::LOAD));
    click(&mut play, "save_3");
    frames(&mut play, 3);
    assert_eq!(play.game.state(), 3);
    assert_eq!(
        play.menus.current(),
        Some(menus::LOAD),
        "the load page again"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_save_whose_lua_db_fails_keeps_its_page() {
    // @ 0x0043b210 is one try: a throw in `LUA_DB`'s read (@ 0x00439270,
    // ReadTable @ 0x004393e0) or in `return Atl_Address_GetCurrent()`
    // (@ 0x00454220) skips the queueing of 5 and 4 (0x0043b6f4..0x0043b7c1).
    let Some(d) = dvd() else { return };
    let dir = scratch("lua-db-fails");
    let head = "<ATL_FILE type=\"atlantis_5_save\">";
    // No `LUA_DB` at all.
    std::fs::write(dir.join("autoslot"), format!("{head}</ATL_FILE>")).expect("autoslot");
    // An element without its `v`.
    std::fs::write(
        dir.join("slot_3"),
        format!("{head}<LUA_DB><N sk=\"x\"/></LUA_DB></ATL_FILE>"),
    )
    .expect("slot_3");
    // A readable but empty `atl`: `Atl_Address_GetCurrent` raises.
    std::fs::write(dir.join("slot_4"), format!("{head}<LUA_DB/></ATL_FILE>")).expect("slot_4");
    let mut play = booted(&d, &dir);
    assert_eq!(play.menus.current(), Some(menus::MAIN));
    click(&mut play, "rescue_game");
    frames(&mut play, 3);
    assert_eq!(play.game.state(), 3);
    assert_eq!(play.menus.current(), Some(menus::MAIN), "no LUA_DB");
    for slot in ["save_3", "save_4"] {
        click(&mut play, "load_game");
        frames(&mut play, 1);
        assert_eq!(play.menus.current(), Some(menus::LOAD));
        click(&mut play, slot);
        frames(&mut play, 3);
        assert_eq!(play.game.state(), 3, "{slot}");
        assert_eq!(play.menus.current(), Some(menus::LOAD), "{slot}");
        // Back to the main menu for the next slot.
        click(&mut play, "back");
        frames(&mut play, 1);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_option_pages_change_the_configuration() {
    let Some(d) = dvd() else { return };
    let dir = scratch("options");
    let mut play = booted(&d, &dir);
    new_game(&mut play);
    play.escape();
    frames(&mut play, 1);
    click(&mut play, "option_sound");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::SOUND));
    assert_eq!(element(&play, "music_level").0, "100%");
    click(&mut play, "music_sub");
    click(&mut play, "music_sub");
    assert_eq!(element(&play, "music_level").0, "80%");
    let m = play.stage().config.music;
    assert!((m - 0.800_000_2).abs() < 1e-6, "{m}");
    click(&mut play, "music_add");
    assert_eq!(element(&play, "music_level").0, "90%");
    click(&mut play, "master_add");
    assert_eq!(element(&play, "master_level").0, "100%", "kept at 1");
    let _ = shot(&play, "a5-sound-page.png");
    click(&mut play, "back");
    frames(&mut play, 1);
    click(&mut play, "option_control");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::CONTROL));
    let s = |k: &str| play.menus.strings.get(k).unwrap_or("").to_string();
    let no = s("no");
    let yes = s("yes");
    assert!(element(&play, "subtitle").0.ends_with(&no));
    click(&mut play, "subtitle");
    assert!(element(&play, "subtitle").0.ends_with(&yes));
    assert!(play.stage().config.subtitles);
    click(&mut play, "invert_pitch");
    assert!(play.stage().config.invert_mouse);
    let _ = shot(&play, "a5-control-page.png");
    click(&mut play, "back");
    frames(&mut play, 1);
    click(&mut play, "option_graphic");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::GRAPHIC));
    let _ = shot(&play, "a5-graphic-page.png");
    // `depth` (0x0041ee4d..0x0041ee6d): graphic_quality 1 ↔ 0, the label
    // `high` ↔ `low` (@ 0x0041e8b0).
    let string = |play: &Play, k: &str| play.menus.strings.get(k).unwrap_or("").to_string();
    let (high, low) = (string(&play, "high"), string(&play, "low"));
    assert!(!high.is_empty() && !low.is_empty() && high != low);
    assert!(element(&play, "depth").0.ends_with(&high));
    click(&mut play, "depth");
    assert_eq!(play.stage().config.graphic_quality, 0);
    assert!(element(&play, "depth").0.ends_with(&low));
    click(&mut play, "depth");
    assert_eq!(play.stage().config.graphic_quality, 1);
    assert!(element(&play, "depth").0.ends_with(&high));
    click(&mut play, "back");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::IN_GAME));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_death_panel_shows_the_game_over_text() {
    let Some(d) = dvd() else { return };
    let dir = scratch("death");
    let mut play = booted(&d, &dir);
    new_game(&mut play);
    assert!(play
        .game
        .run_string("return function() Atl_GameOver(\"_gameover_groom\") end"));
    frames(&mut play, 3);
    assert_eq!(play.game.state(), 3);
    assert_eq!(play.menus.current(), Some(menus::DEATH));
    let text = play
        .menus
        .strings
        .get("_gameover_groom")
        .unwrap_or("")
        .to_string();
    assert!(!text.is_empty());
    assert_eq!(element(&play, "text").0, text);
    assert_eq!(element(&play, "text").2, 150);
    let (_, _, bx, by) = element(&play, "button");
    assert_eq!((bx, by), (1024 - 100, 576 - 100));
    let _ = shot(&play, "a5-death-panel.png");
    // The button: its picture plays, then the panel ends.
    click(&mut play, "button");
    for _ in 0..60 {
        frames(&mut play, 1);
        if play.menus.current().is_none() {
            break;
        }
    }
    assert_eq!(play.menus.current(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_examine_view_shows_an_item_and_scrolls() {
    let Some(d) = dvd() else { return };
    let dir = scratch("examine");
    let mut play = booted(&d, &dir);
    new_game(&mut play);
    assert!(play
        .game
        .run_string("return function() Atl_ExamineItem(\"elmer_note\") end"));
    frames(&mut play, 2);
    assert_eq!(play.game.state(), 3);
    assert_eq!(play.menus.current(), Some(menus::EXAMINE));
    let desc = play
        .menus
        .strings
        .get("_desc_elmer_note")
        .unwrap_or("")
        .to_string();
    assert_eq!(element(&play, "description").0, desc);
    let (_, _, dx, dy) = element(&play, "description");
    assert_eq!((dx, dy), (80, 120));
    let (_, _, ux, uy) = element(&play, "up");
    assert_eq!((ux, uy), (80 + 140, 120 - 64));
    let _ = shot(&play, "a5-examine.png");
    // A left press outside the elements ends it (@ 0x004138f0).
    play.mouse_at(5, 5);
    play.click();
    frames(&mut play, 2);
    assert_eq!(play.menus.current(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_credits_turn_their_pages() {
    let Some(d) = dvd() else { return };
    let dir = scratch("credits");
    let mut play = booted(&d, &dir);
    assert!(play.stage().music().is_empty(), "the main menu is silent");
    click(&mut play, "credits");
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::CREDITS));
    // @ 0x0040e510 state 0: the music stopped, `credits` on channel 4.
    let music: Vec<String> = play.stage().music().into_iter().map(|m| m.0).collect();
    assert_eq!(music, ["credits"]);
    let first = element(&play, "title").0;
    assert_eq!(
        first,
        play.menus
            .strings
            .get("_credits_start")
            .unwrap_or("")
            .to_string()
    );
    let _ = shot(&play, "a5-credits.png");
    // 4 seconds a page.
    frames(&mut play, 125);
    assert_eq!(
        element(&play, "title").0,
        play.menus
            .strings
            .get("_credits_start_game_title")
            .unwrap_or("")
            .to_string()
    );
    click(&mut play, "back");
    for _ in 0..60 {
        frames(&mut play, 1);
        if play.menus.current() == Some(menus::MAIN) {
            break;
        }
    }
    assert_eq!(play.menus.current(), Some(menus::MAIN));
    // The credits' end (state 2): the music stopped with a 1 s fade.
    frames(&mut play, 40);
    assert!(play.stage().music().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_right_press_on_an_inventory_item_examines_it() {
    let Some(d) = dvd() else { return };
    let dir = scratch("inventory-examine");
    let mut play = booted(&d, &dir);
    new_game(&mut play);
    // An item with a detailed picture in slot 1 (as `SetInventoryItem`
    // writes it: `items[slot + 1]`).
    assert!(play.game.run_string(
        "return function() atl.hero.inventory.items[2] = \"elmer_note\"; \
         atl.hero.inventory.nb_items = atl.hero.inventory.nb_items + 1 end"
    ));
    frames(&mut play, 1);
    // The empty hand opens the inventory.
    play.right_press();
    frames(&mut play, 1);
    assert!(play.stage().inventory_open_now());
    let mut found = None;
    'outer: for y in (300..576).step_by(4) {
        for x in (0..1024).step_by(4) {
            play.mouse_at(x, y);
            if play.stage().inventory_hit()
                == Some(atlantis3_engine::atlantis5_play::InventoryHit::Slot(1))
            {
                found = Some((x, y));
                break 'outer;
            }
        }
    }
    let (x, y) = found.expect("slot 1's cell");
    play.mouse_at(x, y);
    play.right_press();
    frames(&mut play, 2);
    assert!(!play.stage().inventory_open_now());
    assert_eq!(play.menus.current(), Some(menus::EXAMINE));
    assert_eq!(play.game.state(), 3);
    // Its end: the inventory again (mode 8 queued after it).
    play.mouse_at(5, 5);
    play.click();
    frames(&mut play, 2);
    assert_eq!(play.menus.current(), None);
    assert!(play.stage().inventory_open_now());
    assert_eq!(play.game.state(), 3);
    // A right press off the cells ends it: play again.
    play.mouse_at(500, 100);
    play.right_press();
    frames(&mut play, 2);
    assert!(!play.stage().inventory_open_now());
    assert_eq!(play.game.state(), 4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_logos_play_before_the_main_menu() {
    let Some(d) = dvd() else { return };
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let mut play = Play::new(src, "french", None).expect("play");
    play.boot();
    assert!(play.logos_playing());
    // Each logo to its end (nothing pressed), the Nobilis one first, then
    // the main menu.
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..30 * 60 {
        if let Some(n) = play.stage().film_name() {
            if seen.last() != Some(&n) {
                seen.push(n);
            }
            // @ 0x0043d6a0: stretched into (0, 0, width − 1, height − 1).
            assert_eq!(play.stage().film_dest(), [0, 0, 1023, 575]);
        }
        if play.menus.current() == Some(menus::MAIN) {
            break;
        }
        // The logos, or the main menu asked once they end.
        assert!(play.logos_playing() || play.menus.active());
        frames(&mut play, 1);
    }
    assert_eq!(seen, ["Logo_Nobilis", "Logo_AIE"]);
    assert_eq!(play.menus.current(), Some(menus::MAIN));
    assert!(!play.logos_playing());
}

/// Each mode starts the menus' mouse at the screen's centre
/// (@ 0x0041b780), and the mouse's relative moves move it from there,
/// kept within the screen (@ 0x0040d890).
#[test]
fn a_menu_mouse_moves_from_the_centre() {
    let Some(d) = dvd() else { return };
    let dir = scratch("menu-mouse");
    let mut play = booted(&d, &dir);
    new_game(&mut play);
    play.escape();
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::IN_GAME));
    assert_eq!(play.menus.mouse, (512, 288));
    play.mouse_by(5, -3);
    assert_eq!(play.menus.mouse, (517, 285));
    play.mouse_by(-5000, 5000);
    assert_eq!(play.menus.mouse, (0, 576));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The menus' cursor (`menu/cursor.spr`, @ 0x0040d9d0 drawn last) is on
/// the picture where the mouse is, and follows it; nothing else changes.
#[test]
fn the_menu_cursor_is_drawn_at_the_mouse() {
    let Some(d) = dvd() else { return };
    let dir = scratch("menu-cursor");
    let mut play = booted(&d, &dir);
    assert_eq!(play.menus.current(), Some(menus::MAIN));
    play.mouse_at(700, 450);
    frames(&mut play, 1);
    let a = shot(&play, "a5-menu-cursor-a.png");
    play.mouse_at(900, 100);
    frames(&mut play, 1);
    let b = shot(&play, "a5-menu-cursor-b.png");
    let changed: Vec<(usize, usize)> = (0..VIEW_HEIGHT * VIEW_WIDTH)
        .filter(|&i| a[i] != b[i])
        .map(|i| (i % VIEW_WIDTH, i / VIEW_WIDTH))
        .collect();
    let near = |x: usize, y: usize| {
        changed
            .iter()
            .filter(|&&(px, py)| px.abs_diff(x) < 64 && py.abs_diff(y) < 64)
            .count()
    };
    let (at_a, at_b) = (near(700, 450), near(900, 100));
    assert!(at_a > 20 && at_b > 20, "{at_a} {at_b}");
    assert_eq!(at_a + at_b, changed.len(), "only the cursor moved");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The logos' sound: their Bink audio track (DCT, stereo, 44100 Hz) plays
/// while each logo plays and stops when it is skipped (`BinkClose`).
#[test]
fn the_logos_sound_plays() {
    use atlantis3_engine::atlantis5_play::CHANNEL_BINK;
    use atlantis3_engine::audio::Audio;
    let Some(d) = dvd() else { return };
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let audio = Audio::new();
    let mut play = Play::new(src, "french", Some(audio.clone())).expect("play");
    play.boot();
    for logo in ["Logo_Nobilis", "Logo_AIE"] {
        assert_eq!(play.stage().film_name().as_deref(), Some(logo));
        frames(&mut play, 30);
        assert!(audio.is_playing(CHANNEL_BINK), "{logo}");
        // A second of the mix at the track's rate: not silent.
        let mut out = vec![0f32; 44100 * 2];
        audio.fill(&mut out, 44100, 2);
        let peak = out.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.01, "{logo}: {peak}");
        // Skipped: its sound stops at once. (The next logo opens in the
        // next frame and its first step starts its own sound.)
        play.skip();
        assert!(!audio.is_playing(CHANNEL_BINK), "{logo} skipped");
        frames(&mut play, 1);
    }
    frames(&mut play, 2);
    assert_eq!(play.menus.current(), Some(menus::MAIN));
}

/// A menu opens with the mouse at the screen's centre and no button hot:
/// the buttons' hit test runs on a mouse move only (@ 0x004523e0), so a
/// press before any move clicks nothing, even on the button under the
/// centre (the in-game menu's `option_control`).
#[test]
fn a_press_before_any_move_clicks_nothing() {
    let Some(d) = dvd() else { return };
    let dir = scratch("still-mouse");
    let mut play = booted(&d, &dir);
    new_game(&mut play);
    play.escape();
    frames(&mut play, 1);
    assert_eq!(play.menus.current(), Some(menus::IN_GAME));
    play.click();
    frames(&mut play, 2);
    assert_eq!(play.menus.current(), Some(menus::IN_GAME));
    // A move, then the press: the button under the mouse is clicked.
    assert!(play.click_menu("option_control"));
    frames(&mut play, 2);
    assert_eq!(play.menus.current(), Some(menus::CONTROL));
    let _ = std::fs::remove_dir_all(&dir);
}

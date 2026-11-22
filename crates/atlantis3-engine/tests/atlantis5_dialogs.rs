//! *Atlantis V*'s dialogs, subject menu, film subtitles and saves, played
//! headless from a new game (`ATLANTIS5_DVD_FR`: the folder holding
//! `datas/`, the French DVD's cabinets extracted; skipped when unset).
//! Saves go to a fresh folder under `ATLANTIS_NATIVE_DATA` (else the
//! system's temporary folder). Pictures go to `ATLANTIS5_SHOT_DIR` when
//! set.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_play::Play;
use atlantis5_script::Value;

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

fn new_play(d: &std::path::Path) -> Play {
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    Play::new(src, "french", None).expect("play")
}

fn to_first_view(d: &std::path::Path, saves: Option<PathBuf>) -> Play {
    let mut play = new_play(d);
    play.set_save_dir(saves);
    play.new_game();
    frames(&mut play, 3);
    play.skip();
    frames(&mut play, 3);
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 3)));
    play
}

fn go(play: &mut Play, view: i32) {
    assert!(play.game.run_string(&format!(
        "return function() Atl_Address_Go(\"zeppelin\", {view}, \"no_fx\") end"
    )));
    for _ in 0..60 {
        frames(play, 1);
        if play.stage().address() == Some((b"zeppelin".to_vec(), view)) {
            break;
        }
    }
    frames(play, 2);
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), view)));
}

fn anim(play: &Play, object: &str) -> Option<String> {
    play.stage()
        .sprites()
        .into_iter()
        .find(|s| s.0 == object)
        .map(|s| s.1)
}

#[test]
fn the_first_film_shows_its_subtitles() {
    let Some(d) = dvd() else { return };
    let mut play = new_play(&d);
    play.new_game();
    frames(&mut play, 3);
    assert!(play.stage().film_playing(), "cine01 plays");
    // cine01's subtitles start at frame 2 (25 frames a second).
    let mut seen = Vec::new();
    for _ in 0..150 {
        frames(&mut play, 1);
        if let Some(t) = play.stage().film_subtitle() {
            if seen.last() != Some(&t) {
                if seen.is_empty() {
                    let _ = shot(&play, "a5-film-subtitle.png");
                }
                seen.push(t);
            }
        }
    }
    assert_eq!(seen.len(), 9, "the 9 subtitles of cine01 in turn");
}

#[test]
fn the_gorillas_subject_menu_and_dialog() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d, None);
    go(&mut play, 17);
    assert!(
        play.aim_at_sprite(b"gorilla"),
        "the gorilla under the centre"
    );
    play.click();
    frames(&mut play, 1);
    // A character with subjects and an empty hand: the subject menu, the
    // manager in state 3, the gorilla turning to face.
    assert_eq!(play.game.state(), 3);
    assert!(play.stage().menu_open_now());
    assert_eq!(anim(&play, "gorilla").as_deref(), Some("facing"));
    for _ in 0..300 {
        frames(&mut play, 1);
        if !play.stage().menu_subjects().is_empty() {
            break;
        }
    }
    assert_eq!(anim(&play, "gorilla").as_deref(), Some("listen"));
    assert_eq!(play.stage().menu_subjects(), [b"gorille".to_vec()]);
    let menu = shot(&play, "a5-subject-menu.png");
    assert!(menu.iter().any(|&p| p & 0xff_ffff != 0));
    // The subject: `click_subject`, the scene's `on_click` plays dialog 1
    // (no facing: the menu is open).
    let (x, y) = play.stage().menu_subject_point(0).expect("subject 0");
    play.mouse_at(x, y);
    play.click();
    frames(&mut play, 2);
    let (id, tirade, state) = play.stage().dialog_state().expect("dialog 1 plays");
    assert_eq!((id, tirade), (1, 0));
    assert!(matches!(state, 1..=3), "{state}");
    let talk = shot(&play, "a5-dialog-1.png");
    assert!(talk.iter().any(|&p| p & 0xff_ffff != 0));
    // The voices end in turn (no sound: their lengths), each tirade
    // following; a click ends a tirade at once.
    let mut tirades = vec![0];
    for _ in 0..30 * 120 {
        frames(&mut play, 1);
        match play.stage().dialog_state() {
            Some((_, t, _)) if tirades.last() != Some(&t) => tirades.push(t),
            None => break,
            _ => {}
        }
    }
    assert!(play.stage().dialog_state().is_none(), "dialog 1 is over");
    assert!(tirades.len() > 1, "{tirades:?}");
    // The menu is made again with the new subjects; `gorille` disabled.
    frames(&mut play, 2);
    let mut subjects = play.stage().menu_subjects();
    subjects.sort();
    assert_eq!(
        subjects,
        [
            b"agresseurs".to_vec(),
            b"gorille".to_vec(),
            b"hindenburg".to_vec()
        ]
    );
    // A click outside the subjects closes it: `unfacing`, back to play.
    play.mouse_at(5, 5);
    play.click();
    frames(&mut play, 2);
    assert!(!play.stage().menu_open_now());
    assert_eq!(play.game.state(), 4);
    assert_eq!(anim(&play, "gorilla").as_deref(), Some("unfacing"));
}

#[test]
fn space_ends_a_dialog_and_its_talkers_turn_back() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d, None);
    go(&mut play, 17);
    // Dialog 1 from a script: the 3D talker faces first.
    assert!(play
        .game
        .run_string("return function() Atl_Dialog_Play(1) end"));
    frames(&mut play, 1);
    assert_eq!(anim(&play, "gorilla").as_deref(), Some("facing"));
    for _ in 0..300 {
        frames(&mut play, 1);
        if play.stage().dialog_state().is_some_and(|s| s.2 != 0)
            && anim(&play, "gorilla").as_deref() == Some("listen")
        {
            break;
        }
    }
    assert_eq!(anim(&play, "gorilla").as_deref(), Some("listen"));
    play.space();
    frames(&mut play, 3);
    assert!(play.stage().dialog_state().is_none());
    assert!(!play.game.busy());
    assert_eq!(anim(&play, "gorilla").as_deref(), Some("unfacing"));
    // A dialog that does not exist: logged, nothing waits.
    assert!(play
        .game
        .run_string("return function() Atl_Dialog_Play(9999) end"));
    frames(&mut play, 1);
    assert!(!play.game.busy());
}

#[test]
fn the_autosave_is_written_and_loaded() {
    let Some(d) = dvd() else { return };
    let dir = scratch("autosave");
    let mut play = to_first_view(&d, Some(dir.clone()));
    go(&mut play, 4);
    assert!(play.game.run_string(
        "return function() atl.flags.native_test = \"yes\"; atl.flags.n = 12.5; \
         Atl_Timer_Create(\"native_t\", 30, 2); Atl_Save() end"
    ));
    frames(&mut play, 1);
    let file = dir.join("autoslot");
    let text = std::fs::read_to_string(&file).expect("saves/autoslot written");
    assert!(
        text.starts_with("<ATL_FILE type=\"atlantis_5_save\">"),
        "{}",
        &text[..80.min(text.len())]
    );
    for section in [
        "<LUA_DB>",
        "<CAMERA ",
        "<SOUND>",
        "<PERSISTANT_SPRITES",
        "<CALLBACK_SHOOTER_MANAGER>",
    ] {
        assert!(text.contains(section), "{section}");
    }
    assert!(text.contains("<S v=\"yes\" sk=\"native_test\"/>"));
    assert!(text.contains("<N v=\"12.5\" sk=\"n\"/>"));
    assert!(text.contains("<CALLBACK_SHOOTER name=\"native_t\" duration=\"30\""));
    // Elsewhere with other values, then `Atl_Load`: the saved table, the
    // timer and the view back.
    go(&mut play, 8);
    assert!(play.game.run_string(
        "return function() atl.flags.native_test = \"no\"; Atl_Timer_Destroy(\"native_t\"); Atl_Load() end"
    ));
    for _ in 0..60 {
        frames(&mut play, 1);
        if play.stage().address() == Some((b"zeppelin".to_vec(), 4)) && !play.game.busy() {
            break;
        }
    }
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 4)));
    assert_eq!(play.game.state(), 4);
    assert!(play
        .game
        .run_string("return function() native_check = atl.flags.native_test end"));
    frames(&mut play, 1);
    assert_eq!(
        play.game.script().global(b"native_check"),
        Value::Str(b"yes".to_vec())
    );
    assert!(play.game.timer_names().contains(&b"native_t".to_vec()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_right_button_puts_the_hands_object_back() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d, None);
    // The coin is reachable from view 4 (`chapter_01.lua`'s Init_Coin).
    go(&mut play, 4);
    assert!(play.aim_at_sprite(b"coin"), "the coin under the centre");
    play.click();
    frames(&mut play, 2);
    assert_eq!(
        play.game.script().handled_item(),
        Ok(b"coin".to_vec()),
        "the coin in the hand"
    );
    // `return Atl_Hero_ArrangeHandledObject`: into the inventory.
    play.right_click();
    frames(&mut play, 1);
    assert_eq!(play.game.script().handled_item(), Ok(Vec::new()));
    assert!(play.game.run_string(
        "return function() native_has = Atl_Inventory_HasItem(atl.hero.inventory, \"coin\") end"
    ));
    frames(&mut play, 1);
    assert_eq!(play.game.script().global(b"native_has"), Value::Bool(true));
}

#[test]
fn the_inventory_takes_and_puts_back() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d, None);
    go(&mut play, 4);
    assert!(play.aim_at_sprite(b"coin"));
    play.click();
    frames(&mut play, 2);
    // The hand's coin into the inventory, then the empty hand opens it.
    play.right_press();
    frames(&mut play, 1);
    play.right_press();
    frames(&mut play, 1);
    assert_eq!(play.game.state(), 3);
    assert!(play.stage().inventory_open_now());
    let items = play.stage().inventory_items();
    let slot = items
        .iter()
        .position(|i| i.as_deref() == Some(&b"coin"[..]))
        .expect("the coin in a slot");
    let _ = shot(&play, "a5-inventory.png");
    // A press on its cell: the coin into the hand, the slot empty.
    let x = (1024 - 800) / 2;
    let _ = x;
    let mut found = None;
    'outer: for y in (300..576).step_by(4) {
        for x in (0..1024).step_by(4) {
            play.mouse_at(x, y);
            if play.stage().inventory_hit()
                == Some(atlantis3_engine::atlantis5_play::InventoryHit::Slot(slot))
            {
                found = Some((x, y));
                break 'outer;
            }
        }
    }
    let (x, y) = found.expect("the slot's cell");
    play.mouse_at(x, y);
    play.click();
    frames(&mut play, 1);
    assert_eq!(play.game.script().handled_item(), Ok(b"coin".to_vec()));
    assert_eq!(play.stage().inventory_items()[slot], None);
    // Again on the now empty slot: back in it.
    play.click();
    frames(&mut play, 1);
    assert_eq!(play.game.script().handled_item(), Ok(Vec::new()));
    assert_eq!(
        play.stage().inventory_items()[slot].as_deref(),
        Some(&b"coin"[..])
    );
    // The right button closes it.
    play.right_press();
    frames(&mut play, 1);
    assert!(!play.stage().inventory_open_now());
    assert_eq!(play.game.state(), 4);
}

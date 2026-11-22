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
    // The subtitles shown (the control page's `subtitle`).
    play.stage_mut().config.subtitles = true;
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
fn without_the_subtitle_option_a_film_shows_none() {
    let Some(d) = dvd() else { return };
    let mut play = new_play(&d);
    assert!(!play.stage().config.subtitles);
    play.new_game();
    frames(&mut play, 3);
    assert!(play.stage().film_playing(), "cine01 plays");
    for _ in 0..150 {
        frames(&mut play, 1);
        assert_eq!(play.stage().film_subtitle(), None);
    }
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
    // The subject menu and the dialog held the music at about half its
    // level (@ 0x0045caf0: the first holder fades to the level then × 0.5,
    // so a holder taken while the level comes back aims a little lower);
    // the menu still holds it.
    let music = play.stage().config.music;
    let level = play.stage().music_level();
    assert!(
        (level - music * 0.5).abs() < 0.02 * music,
        "{level} {music}"
    );
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
    // Released: back to the option's level over 1 s (@ 0x0045cb20).
    frames(&mut play, 40);
    assert!((play.stage().music_level() - music).abs() < 1e-4);
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
    // `CAMERA_MOVER` after `CONFIGURATION` (@ 0x00437f40): the free mode,
    // its speeds, no boat.
    let at = |s: &str| text.find(s).unwrap_or_else(|| panic!("{s}"));
    assert!(at("<CONFIGURATION ") < at("<CAMERA_MOVER moving_type=\"0\""));
    assert!(at("<CAMERA_MOVER ") < at("<CAMERA "));
    assert!(text.contains("<MOVING_DATAS camera_speed_x="));
    assert!(!text.contains("<FX_DATAS"));
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
    // On a cell the grid takes the right press (event 3): the coin has
    // no detailed picture, nothing happens.
    play.right_press();
    frames(&mut play, 1);
    assert!(play.stage().inventory_open_now());
    // Off the cells the right button closes it.
    play.mouse_at(500, 100);
    play.right_press();
    frames(&mut play, 1);
    assert!(!play.stage().inventory_open_now());
    assert_eq!(play.game.state(), 4);
}

/// The pixels that differ between `a` and `b` in the rectangle (x0, y0,
/// w, h), and how many of `a`'s there are near `rgb` (each channel
/// within 40: the glyphs' edges are smoothed).
fn changed(a: &[u32], b: &[u32], (x0, y0, w, h): (i32, i32, i32, i32), rgb: u32) -> (usize, usize) {
    let (mut n, mut of) = (0, 0);
    for y in y0.max(0)..(y0 + h).min(576) {
        for x in x0.max(0)..(x0 + w).min(1024) {
            let i = (y * 1024 + x) as usize;
            if a[i] & 0x00ff_ffff != b[i] & 0x00ff_ffff {
                n += 1;
                let near = [0, 8, 16]
                    .iter()
                    .all(|&k| ((a[i] >> k) & 0xff).abs_diff((rgb >> k) & 0xff) <= 40);
                if near {
                    of += 1;
                }
            }
        }
    }
    (n, of)
}

#[test]
fn the_inventory_bubble_and_the_hand_name() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d, None);
    go(&mut play, 4);
    assert!(play.aim_at_sprite(b"coin"));
    play.click();
    frames(&mut play, 2);
    play.right_press();
    frames(&mut play, 1);
    play.right_press();
    frames(&mut play, 1);
    assert!(play.stage().inventory_open_now());
    let coin = play
        .menus
        .string_opt("_item_coin")
        .expect("the coin's description");
    assert!(!coin.is_empty());
    // The mouse at the centre (off the cells): no texts.
    assert_eq!(
        play.stage().inventory_texts(),
        Some((String::new(), String::new()))
    );
    play.mouse_at(500, 100);
    frames(&mut play, 1);
    let empty_hand = shot(&play, "a5-inventory-empty-hand.png");
    let slot = play
        .stage()
        .inventory_items()
        .iter()
        .position(|i| i.as_deref() == Some(&b"coin"[..]))
        .unwrap();
    let mut cell = None;
    'outer: for y in (300..576).step_by(2) {
        for x in (0..1024).step_by(2) {
            play.mouse_at(x, y);
            if play.stage().inventory_hit()
                == Some(atlantis3_engine::atlantis5_play::InventoryHit::Slot(slot))
            {
                cell = Some((x, y));
                break 'outer;
            }
        }
    }
    // The cell's top-left corner (its 64 x 64 at the scan's first hit).
    let (x, y) = cell.expect("the coin's cell");
    play.mouse_at(x, y);
    frames(&mut play, 1);
    assert_eq!(
        play.stage().inventory_texts(),
        Some((coin.clone(), String::new())),
        "the bubble over the coin"
    );
    let bubble = shot(&play, "a5-inventory-bubble.png");
    // The coin into the hand: the elements made again (@ 0x00415fe0)
    // empty the bubble, which an empty slot does not set again; the hand's
    // name is under the cursor.
    play.click();
    frames(&mut play, 1);
    assert_eq!(play.game.script().handled_item(), Ok(b"coin".to_vec()));
    assert_eq!(
        play.stage().inventory_texts(),
        Some((String::new(), coin.clone()))
    );
    let no_bubble = shot(&play, "a5-inventory-no-bubble.png");
    // Right of the cell, on the bubble's first line (from (x + 31, y −
    // 1)): the black text over its white shadow, gone once emptied.
    let (n, black) = changed(&bubble, &no_bubble, (x + 66, y - 1, 266, 16), 0);
    assert!(n > 40 && black > 10, "{n} changed, {black} black");
    // Nothing changed left of the cursor on those rows.
    let (left, _) = changed(&bubble, &no_bubble, (x - 300, y - 1, 290, 16), 0);
    assert_eq!(left, 0);
    // The hand's name: white over its black shadow from (x − 1, y + 94)
    // with the mouse at (500, 100); right of the hand's icon (5, 30)
    // from the mouse, 64 wide.
    play.mouse_at(500, 100);
    frames(&mut play, 1);
    let hand = shot(&play, "a5-inventory-hand-name.png");
    let (n, white) = changed(&hand, &empty_hand, (575, 194, 225, 16), 0x00ff_ffff);
    assert!(n > 40 && white > 10, "{n} changed, {white} white");
    // The hand's icon follows the cursor: at (505, 130), not at (5, 30).
    let (icon, _) = changed(&hand, &empty_hand, (505, 131, 60, 60), 0);
    let (corner, _) = changed(&hand, &empty_hand, (5, 31, 60, 60), 0);
    assert!(
        icon > 500 && corner == 0,
        "{icon} at the cursor, {corner} at (5, 30)"
    );
    // Off the cells, then back on the empty slot: the bubble stays empty.
    play.mouse_at(x, y);
    frames(&mut play, 1);
    assert_eq!(play.stage().inventory_texts(), Some((String::new(), coin)));
}

/// The gorilla's mouth during dialog 1 (@ 0x004346f0): each frame of a
/// 3D talker's tirade it plays `talk` when the voice's 0.3 s window at the
/// tirade's time is loud, else `listen`, as the analyser of its voice file
/// says (@ 0x0045cb40); `listen` again at the tirade's end.
#[test]
fn the_3d_talker_moves_its_mouth_with_its_voice() {
    use atlantis3_engine::atlantis5_dialog::{voice_path, LipSync};
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d, None);
    go(&mut play, 17);
    assert!(play
        .game
        .run_string("return function() Atl_Dialog_Play(1) end"));
    let src = Atlantis5Source::open(&d.join("datas")).unwrap();
    let lips = |k: usize| {
        let data =
            atlantis_data::FileSource::read(&src, &voice_path("french", 1, k)).expect("the voice");
        let v = atlantis4_data::audio::decode_vorbis(&data).unwrap();
        LipSync::analyse(&v.samples, v.channels, v.sample_rate)
    };
    let dt = 1.0f64 / 30.0;
    // (tirade, the player's time before the frame's step) while state 3.
    let mut at: Option<(usize, f32, LipSync)> = None;
    let (mut checked, mut talks, mut listens) = (0, 0, 0);
    for _ in 0..30 * 120 {
        frames(&mut play, 1);
        let Some((_, tirade, state)) = play.stage().dialog_state() else {
            break;
        };
        let anim = anim(&play, "gorilla").unwrap_or_default();
        match &mut at {
            // A frame of state 3 that went on: the lip-sync read the time
            // before it grew.
            Some((k, t, l)) if *k == tirade && state == 3 => {
                let want = if l.talking(*t) { "talk" } else { "listen" };
                assert_eq!(anim, want, "tirade {k} at {t}");
                checked += 1;
                if want == "talk" {
                    talks += 1;
                } else {
                    listens += 1;
                }
                *t = (dt + f64::from(*t)) as f32;
            }
            Some((k, _, _)) if *k != tirade => {
                // The tirade ended: `listen`.
                assert_eq!(anim, "listen", "after tirade {k}");
                at = None;
            }
            _ => {}
        }
        if at.is_none() && state == 3 {
            // State 0 ran this frame: time 0, the lip-sync at 0, no time
            // added.
            let l = lips(tirade);
            let want = if l.talking(0.0) { "talk" } else { "listen" };
            assert_eq!(anim, want, "tirade {tirade} starts");
            at = Some((tirade, 0.0, l));
        }
    }
    assert!(play.stage().dialog_state().is_none(), "dialog 1 is over");
    assert!(
        checked > 30 && talks > 5 && listens > 5,
        "{checked} frames, {talks} talk, {listens} listen"
    );
}

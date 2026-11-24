//! *Atlantis V*'s poker board played headless (`ATLANTIS5_DVD_FR`: the
//! folder holding `datas/`, the French DVD's cabinets extracted; skipped
//! when unset): chapter 7, the punter's `commencer_poker` subject opens
//! the board (mode 12), whose C steps ask the player's own scripts
//! (`Atl_PuzzlePoker_*`); the player's choices on the panel win the five
//! games (the `OnWin` script runs) or lose them (the death panel of
//! `OnLose`). Dialogs are ended with Space, films skipped.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_menu as menus;
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

fn shot(play: &Play, name: &str) -> Vec<u32> {
    let mut px = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
    play.render(&mut px);
    if let Some(dir) = std::env::var_os("ATLANTIS5_SHOT_DIR") {
        let png = atlantis2_engine::png::encode_xrgb8888(&px, VIEW_WIDTH, VIEW_HEIGHT);
        let _ = std::fs::write(PathBuf::from(dir).join(name), png);
    }
    px
}

/// Frames with the films skipped and the dialogs ended.
fn frames(play: &mut Play, n: usize) {
    for _ in 0..n {
        if play.stage().film_playing() {
            play.skip();
        }
        if play.stage().dialog_running() {
            play.space();
        }
        play.update(1.0 / 30.0);
    }
}

fn lua(play: &mut Play, expr: &str) -> Value {
    play.game
        .script()
        .one_shot(&format!("return {expr}"), None, 1)
        .expect("lua")
        .remove(0)
}

fn num(v: &Value) -> i32 {
    match v {
        #[allow(clippy::cast_possible_truncation)]
        Value::Num(x) => *x as i32,
        _ => panic!("{v:?}"),
    }
}

/// A new game, chapter 7 (Macao), view 20 with the punter at the table
/// and its `commencer_poker` subject; the subject picked: the board.
fn to_poker(d: &std::path::Path) -> Play {
    to_poker_saving(d, None)
}

/// [`to_poker`] with a save folder (the chapter's `Atl_Save` writes its
/// `autoslot` there).
fn to_poker_saving(d: &std::path::Path, saves: Option<PathBuf>) -> Play {
    to_poker_with(d, saves, |_| {})
}

/// [`to_poker_saving`] with `before` run on the new [`Play`] first.
fn to_poker_with(
    d: &std::path::Path,
    saves: Option<PathBuf>,
    before: impl FnOnce(&mut Play),
) -> Play {
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let mut play = Play::new(src, "french", None).expect("play");
    before(&mut play);
    play.set_save_dir(saves);
    play.new_game();
    frames(&mut play, 6);
    assert!(play
        .game
        .run_string("return function() Atl_InitChapter(7) end"));
    for _ in 0..200 {
        frames(&mut play, 1);
        if play.game.state() == 4
            && !play.game.busy()
            && play.stage().address() == Some((b"macao".to_vec(), 1))
        {
            break;
        }
    }
    assert_eq!(play.stage().address(), Some((b"macao".to_vec(), 1)));
    // The state the subject asks for (on_click_macao.lua: d65, d113,
    // d119, the punter at the poker table).
    assert!(play.game.run_string(
        "return function() \
           Atl_Object_SetState('punter', 'play_poker') \
           Atl_Character_AddSubject('punter-poker', 'commencer_poker', 1) \
           Atl_Address_Go('macao', 20, 'no_fx') end"
    ));
    for _ in 0..60 {
        frames(&mut play, 1);
        if !play.game.busy() && play.stage().address() == Some((b"macao".to_vec(), 20)) {
            break;
        }
    }
    frames(&mut play, 2);
    assert!(
        play.aim_at_sprite(b"punter-poker"),
        "the punter at the table"
    );
    play.click();
    for _ in 0..300 {
        frames(&mut play, 1);
        if !play.stage().menu_subjects().is_empty() {
            break;
        }
    }
    let subjects = play.stage().menu_subjects();
    let k = subjects
        .iter()
        .position(|s| s == b"commencer_poker")
        .expect("the subject");
    let (x, y) = play.stage().menu_subject_point(k).expect("its place");
    play.mouse_at(x, y);
    play.click();
    for _ in 0..300 {
        frames(&mut play, 1);
        if play.poker().is_some() {
            break;
        }
    }
    assert!(play.poker().is_some(), "the board is up");
    assert_eq!(play.game.state(), 3);
    play
}

/// Plays the board: on the panel (state 3) `choose(game, round)` names the
/// button; Elliot's raise (state 0xb) is followed; after each game
/// `next_game`. Returns the games seen ended.
fn play_board(play: &mut Play, choose: &dyn Fn(i32, i32) -> &'static str) -> Vec<i32> {
    let mut ended = Vec::new();
    let mut shots = 0;
    for _ in 0..30_000 {
        frames(play, 1);
        let Some(state) = play.poker().map(|p| p.state) else {
            break;
        };
        if play.game.busy() {
            continue;
        }
        match state {
            3 => {
                let game = num(&lua(play, "atl.puzzle.poker.game"));
                let round = num(&lua(play, "atl.puzzle.poker.round"));
                if shots < 2 {
                    frames(play, 2);
                    shot(play, &format!("a5-poker-choice-{shots}.png"));
                    shots += 1;
                }
                let b = choose(game, round);
                assert!(
                    play.click_poker(b),
                    "{b} shown in game {game} round {round}"
                );
            }
            0xb => assert!(play.click_poker("follow_bet")),
            0xd => {
                let game = num(&lua(play, "atl.puzzle.poker.game"));
                if ended.last() != Some(&game) {
                    ended.push(game);
                    // The table's cards turn at the scene's next steps.
                    frames(play, 2);
                    shot(play, &format!("a5-poker-winner-{game}.png"));
                }
                assert!(play.click_poker("next_game"));
            }
            _ => {}
        }
    }
    ended
}

/// The board's state 0 places `next_game` and the choice panel's buttons
/// by heights measured with @ 0x00461800(400 / 0x17c, font, text, 0): not
/// partitioned, so a text wider than its width counts as one line.
#[test]
fn the_board_places_its_buttons_by_their_own_lines() {
    let Some(d) = dvd() else { return };
    let wide = "abcdefgh ".repeat(60);
    let mut play = to_poker_with(&d, None, |play| {
        for (id, text) in &mut play.menus.strings.strings {
            if id == "increase_bet" || id == "back_to_game" {
                *text = wide.clone();
            }
        }
    });
    frames(&mut play, 2);
    let (choice, cont, wrapped) = {
        let st = play.stage();
        (
            st.fonts.text_height("AtlFont_PokerChoice", "x", 0x17c),
            st.fonts.text_height("AtlFont_PokerContinue", "x", 400),
            st.fonts.text_height("AtlFont_PokerChoice", &wide, 0x17c),
        )
    };
    assert!(wrapped > choice, "the test's text wraps");
    let poker = play.poker().expect("the board");
    let at = |n: &str| poker.place_of(n).expect(n);
    assert_eq!(at("follow_bet").1 - at("increase_bet").1, choice + 10);
    assert_eq!(at("stop_bet").1 - at("follow_bet").1, choice + 10);
    assert_eq!(at("next_game").1 - at("winner_background").1, -(cont / 2));
}

#[test]
fn the_five_games_won() {
    let Some(d) = dvd() else { return };
    let mut play = to_poker(&d);
    // The scenario's strengths: Howard has the best hand in games 1, 4
    // and 5; he stops at once in games 2 and 3.
    let ended = play_board(&mut play, &|game, _| match game {
        2 | 3 => "stop_bet",
        _ => "follow_bet",
    });
    assert_eq!(ended, [1, 2, 3, 4, 5]);
    for _ in 0..600 {
        frames(&mut play, 1);
        if play.game.state() == 4 && !play.game.busy() {
            break;
        }
    }
    assert!(play.poker().is_none(), "the board ended");
    assert_eq!(play.game.state(), 4, "back to play");
    // OnWin: Ishtar kidnapped, the bas-relief stolen, the doors open.
    assert_eq!(
        lua(&mut play, "atl.objects.ishtar.state"),
        Value::Str(b"kidnapped".to_vec())
    );
    assert_eq!(
        lua(&mut play, "atl.objects.macao_bas_relief.state"),
        Value::Str(b"stolen".to_vec())
    );
    assert_eq!(
        play.game.zone(b"poker_room_out").map(|z| z.1),
        Some(true),
        "OnWin opened the way out"
    );
}

#[test]
fn stopping_every_game_loses_the_last() {
    let Some(d) = dvd() else { return };
    // With an autosave, as in a real game (chapter 7 calls `Atl_Save`):
    // `OnLose(3)` (`Atl_GameOver`, then `Atl_Load`) runs in the board's
    // last frame, and the death panel it asks comes before the load.
    let saves = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("a5-poker-lost-saves");
    let _ = std::fs::remove_dir_all(&saves);
    std::fs::create_dir_all(&saves).expect("the save folder");
    let mut play = to_poker_saving(&d, Some(saves.clone()));
    assert!(saves.join("autoslot").is_file(), "the chapter saved");
    let ended = play_board(&mut play, &|_, _| "stop_bet");
    // Games 1 to 3 end with another winner; in game 4 the fisherman's
    // hand wins (the board's dialog 285, then OnLose(3)): the death
    // panel.
    assert_eq!(ended, [1, 2, 3]);
    assert_eq!(num(&lua(&mut play, "atl.puzzle.poker.game")), 4);
    for _ in 0..600 {
        frames(&mut play, 1);
        if play.menus.current() == Some(menus::DEATH) {
            break;
        }
    }
    assert!(play.poker().is_none(), "the board ended");
    assert_eq!(play.menus.current(), Some(menus::DEATH));
    assert_eq!(play.game.state(), 3);
    shot(&play, "a5-poker-lost.png");
}

/// The buttons' hover (@ 0x004523e0, @ 0x0044eb60) is tested on mouse
/// moves only and reset by a show (@ 0x00452530, 0x0044ec50): when the
/// panel comes back under a still mouse, a press on `follow_bet` does
/// nothing until the mouse moves.
#[test]
fn a_shown_panel_takes_a_click_only_after_a_move() {
    let Some(d) = dvd() else { return };
    let mut play = to_poker(&d);
    let wait_panel = |play: &mut Play| {
        for _ in 0..30_000 {
            frames(play, 1);
            if !play.game.busy() && play.poker().is_some_and(|p| p.state == 3) {
                return;
            }
        }
        panic!("no panel");
    };
    wait_panel(&mut play);
    assert!(play.click_poker("follow_bet"));
    frames(&mut play, 1);
    assert_ne!(play.poker().map(|p| p.state), Some(3), "the click taken");
    wait_panel(&mut play);
    assert!(play.poker().unwrap().shown("follow_bet"));
    play.click();
    frames(&mut play, 30);
    assert_eq!(
        play.poker().map(|p| p.state),
        Some(3),
        "a press without a move after the show"
    );
    assert!(play.click_poker("follow_bet"));
    frames(&mut play, 1);
    assert_ne!(play.poker().map(|p| p.state), Some(3), "after a move");
}

/// The panel's return (states 2, 0xa, 0xc) puts the mouse back at the
/// cursor's place (@ 0x004520a0 on the cursor, @ 0x0040d890): the moves
/// made while the opponents' dialogs played are dropped, and the button
/// under it is not hot until a move.
#[test]
fn the_panel_puts_the_mouse_back_at_the_cursor() {
    let Some(d) = dvd() else { return };
    let mut play = to_poker(&d);
    let wait_panel = |play: &mut Play| {
        for _ in 0..30_000 {
            frames(play, 1);
            if !play.game.busy() && play.poker().is_some_and(|p| p.state == 3) {
                return;
            }
        }
        panic!("no panel");
    };
    wait_panel(&mut play);
    assert!(play.click_poker("follow_bet"));
    let at_click = play.menus.mouse;
    frames(&mut play, 1);
    assert!(play.game.busy(), "the opponents' dialogs");
    play.mouse_by(200, 50);
    assert_ne!(play.menus.mouse, at_click);
    wait_panel(&mut play);
    assert_eq!(play.menus.mouse, at_click, "back at the cursor's place");
    // On the button, but not hot: a press does nothing.
    play.click();
    frames(&mut play, 3);
    assert_eq!(play.poker().map(|p| p.state), Some(3));
}

/// The showdown's dialogs (281 in game 2, the punter's win) run after
/// state 0xc's put-back and block the board's process: the moves made
/// meanwhile reach the mouse, not the cursor element (@ 0x0040d920 sees
/// no move event after them), so the cursor stays at its place and
/// `next_game` is not hot (+0x8c = 0 from its show) until a move.
#[test]
fn a_move_during_the_showdown_s_dialog_waits_for_the_next_move() {
    let Some(d) = dvd() else { return };
    let mut play = to_poker(&d);
    let mut moved_to = None;
    for _ in 0..30_000 {
        frames(&mut play, 1);
        let Some(state) = play.poker().map(|p| p.state) else {
            panic!("the board ended");
        };
        let game = num(&lua(&mut play, "atl.puzzle.poker.game"));
        if play.game.busy() {
            if game == 2 && state == 0xe && moved_to.is_none() {
                let (x, y) = play.poker().unwrap().place_of("next_game").unwrap();
                play.mouse_at(x + 5, y + 5);
                moved_to = Some((x + 5, y + 5));
            }
            continue;
        }
        match state {
            3 => assert!(play.click_poker(if game == 2 { "stop_bet" } else { "follow_bet" })),
            0xb => assert!(play.click_poker("follow_bet")),
            0xd if moved_to.is_some() => break,
            0xd => assert!(play.click_poker("next_game")),
            _ => {}
        }
    }
    let at = moved_to.expect("game 2's showdown dialog");
    assert_eq!(play.poker().map(|p| p.state), Some(0xd));
    assert_eq!(play.menus.mouse, at, "the mouse moved");
    assert_ne!(
        play.poker().unwrap().cursor_place(),
        Some(at),
        "the cursor at its place"
    );
    // A press without a move: `next_game` is not hot.
    play.click();
    frames(&mut play, 3);
    assert_eq!(play.poker().map(|p| p.state), Some(0xd));
    // After a move: the cursor follows, the press starts the next game.
    assert!(play.click_poker("next_game"));
    frames(&mut play, 1);
    assert_ne!(play.poker().map(|p| p.state), Some(0xd));
}

/// A move goes through the panel's buttons from the last made
/// (@ 0x00452140 puts each new child at the head) and stops at the one it
/// turns hot (@ 0x004523e0, @ 0x0044eb60): from `follow_bet` onto
/// `stop_bet`, made after it, both stay hot until the next move, and a
/// press would go to `stop_bet`, the first in that order.
#[test]
fn a_move_onto_a_later_button_leaves_the_earlier_one_hot() {
    let Some(d) = dvd() else { return };
    let mut play = to_poker(&d);
    for _ in 0..30_000 {
        frames(&mut play, 1);
        if !play.game.busy() && play.poker().is_some_and(|p| p.state == 3) {
            break;
        }
    }
    assert_eq!(play.poker().map(|p| p.state), Some(3));
    let at = |play: &Play, n: &str| {
        let st = play.stage();
        play.poker().unwrap().point_of(n, &st.fonts).expect(n)
    };
    let (ix, iy) = at(&play, "follow_bet");
    let (fx, fy) = at(&play, "stop_bet");
    play.mouse_at(ix + 1, iy);
    play.update(0.0);
    play.mouse_at(ix, iy);
    play.update(0.0);
    assert_eq!(play.poker().unwrap().hot(), ["follow_bet"]);
    play.mouse_at(fx, fy);
    play.update(0.0);
    assert_eq!(play.poker().unwrap().hot(), ["follow_bet", "stop_bet"]);
    play.mouse_at(fx + 1, fy);
    play.update(0.0);
    assert_eq!(play.poker().unwrap().hot(), ["stop_bet"]);
}

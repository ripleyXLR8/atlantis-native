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
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let mut play = Play::new(src, "french", None).expect("play");
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
    let mut play = to_poker(&d);
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

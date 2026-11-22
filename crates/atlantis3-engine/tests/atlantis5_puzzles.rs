//! *Atlantis V*'s puzzles played headless through the player's own
//! scripts (`ATLANTIS5_DVD_FR`: the folder holding `datas/`, the French
//! DVD's cabinets extracted; skipped when unset). Each test starts the
//! puzzle's chapter (`Atl_InitChapter`), goes to its view, sets what the
//! story would have set before (the hand's item, a flag), then clicks the
//! zones and sprites as a player does (the screen centre turned onto them)
//! until the puzzle is solved; a wrong play leaves it unsolved, or loses
//! where the game can be lost. Films are skipped and dialogs ended with
//! Space. Pictures go to the folder `ATLANTIS5_SHOT_DIR` names, when set.
//! (Poker has its own file, `atlantis5_poker.rs`.)

use std::path::PathBuf;
use std::sync::Arc;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_play::{Hover, Play};
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

fn shot(play: &Play, name: &str) {
    if let Some(dir) = std::env::var_os("ATLANTIS5_SHOT_DIR") {
        let mut px = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
        play.render(&mut px);
        let png = atlantis2_engine::png::encode_xrgb8888(&px, VIEW_WIDTH, VIEW_HEIGHT);
        let _ = std::fs::write(PathBuf::from(dir).join(name), png);
    }
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

/// Frames until the game is in play with no script running.
fn settle(play: &mut Play) {
    for _ in 0..2000 {
        frames(play, 1);
        if play.game.state() == 4 && !play.game.busy() {
            frames(play, 1);
            return;
        }
    }
    panic!("not back to play (state {})", play.game.state());
}

fn lua(play: &mut Play, expr: &str) -> Value {
    play.game
        .script()
        .one_shot(&format!("return {expr}"), None, 1)
        .unwrap_or_else(|e| panic!("{expr}: {e}"))
        .remove(0)
}

fn s(v: &str) -> Value {
    Value::Str(v.as_bytes().to_vec())
}

/// Runs `body` as a script run (its calls may wait) and settles.
fn run(play: &mut Play, body: &str) {
    assert!(
        play.game
            .run_string(&format!("return function() {body} end")),
        "{body}"
    );
    settle(play);
}

/// A new game, then chapter `n` started (its film skipped).
fn chapter(d: &std::path::Path, n: u32) -> Play {
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let mut play = Play::new(src, "french", None).expect("play");
    play.new_game();
    settle(&mut play);
    run(&mut play, &format!("Atl_InitChapter({n})"));
    settle(&mut play);
    play
}

/// `Atl_Address_Go(scene, view, "no_fx")`.
fn go(play: &mut Play, scene: &str, view: i32) {
    run(
        play,
        &format!("Atl_Address_Go(\"{scene}\", {view}, \"no_fx\")"),
    );
    assert_eq!(
        play.stage().address(),
        Some((scene.as_bytes().to_vec(), view))
    );
}

/// A click with the centre on the sprite of `object`.
fn sprite(play: &mut Play, object: &str) {
    assert!(
        play.aim_at_sprite(object.as_bytes()),
        "{object} under the centre"
    );
    play.click();
    settle(play);
}

/// A click with the centre on the zone named `name` (or, for a move
/// zone without a name, numbered `id`).
fn zone(play: &mut Play, name: &str) {
    let id = play
        .zones()
        .into_iter()
        .find(|z| z.1 == name.as_bytes())
        .map(|z| z.0)
        .unwrap_or_else(|| panic!("zone {name} active in {:?}", play.stage().address()));
    assert!(play.aim_at_zone_named(id, name.as_bytes()));
    assert_eq!(
        play.hover(),
        Some(&Hover::Zone(id, name.as_bytes().to_vec())),
        "the pick takes {name}"
    );
    play.click();
    settle(play);
}

/// A click with the centre on the zone named `name` numbered `id`.
fn zone_id(play: &mut Play, name: &str, id: i32) {
    assert!(
        play.zones()
            .iter()
            .any(|z| z.0 == id && z.1 == name.as_bytes()),
        "zone {name} {id} active in {:?}",
        play.stage().address()
    );
    assert!(play.aim_at_zone_named(id, name.as_bytes()));
    assert_eq!(
        play.hover(),
        Some(&Hover::Zone(id, name.as_bytes().to_vec())),
        "the pick takes {name} {id}"
    );
    play.click();
    settle(play);
}

/// The first view of `views` in `scene` that has an active zone `name`.
fn view_with_zone(play: &mut Play, scene: &str, views: &[i32], name: &str) -> i32 {
    for &v in views {
        go(play, scene, v);
        if play.zones().iter().any(|z| z.1 == name.as_bytes()) {
            return v;
        }
    }
    panic!("no view of {views:?} has the zone {name}");
}

fn int(v: &Value) -> i32 {
    match v {
        #[allow(clippy::cast_possible_truncation)]
        Value::Num(x) => *x as i32,
        _ => panic!("not a number: {v:?}"),
    }
}

/// The animation the sprite of `object` plays, if it has one.
fn anim(play: &Play, object: &str) -> Option<String> {
    play.stage()
        .sprites()
        .into_iter()
        .find(|s| s.0 == object)
        .map(|s| s.1)
}

#[test]
fn den_mother_box() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 4);
    go(&mut play, "etage18", 18);
    assert_eq!(anim(&play, "den_mother_box_lid").as_deref(), Some("z1"));
    // A wrong code: the lid stays shut.
    sprite(&mut play, "den_mother_box_number_1");
    assert_eq!(
        lua(&mut play, "Atl_PuzzleDenMotherBox_Check()"),
        Value::Bool(false)
    );
    // The code 7 3 8 0 4 (Atl_PuzzleDenMotherBox_Init): each wheel turns
    // one step a click, modulo 9.
    for (wheel, value) in [(1, 7), (2, 3), (3, 8), (4, 0), (5, 4)] {
        let now = int(&lua(&mut play, &format!("atl.puzzle.dmbox.state[{wheel}]")));
        for _ in 0..(value - now).rem_euclid(9) {
            sprite(&mut play, &format!("den_mother_box_number_{wheel}"));
        }
    }
    assert_eq!(
        lua(&mut play, "Atl_PuzzleDenMotherBox_Check()"),
        Value::Bool(true)
    );
    // The wheels gone, the lid opening (z2).
    assert_eq!(anim(&play, "den_mother_box_lid").as_deref(), Some("z2"));
    assert!(anim(&play, "den_mother_box_number_1").is_none());
    shot(&play, "a5-puzzle-den-mother-box.png");
}

/// The shortest presses from `from` to `to` with the steps `moves`
/// (kept within 0..=max, as `OnButtonActivated` keeps them).
fn presses(from: i32, to: i32, max: i32, moves: &[i32]) -> Vec<i32> {
    let mut prev = vec![None; usize::try_from(max + 1).expect("max")];
    let mut queue = std::collections::VecDeque::from([from]);
    let idx = |x: i32| usize::try_from(x).expect("in range");
    prev[idx(from)] = Some((from, 0));
    while let Some(x) = queue.pop_front() {
        if x == to {
            break;
        }
        for &m in moves {
            let y = x + m;
            if (0..=max).contains(&y) && prev[idx(y)].is_none() {
                prev[idx(y)] = Some((x, m));
                queue.push_back(y);
            }
        }
    }
    let mut out = Vec::new();
    let mut x = to;
    while x != from {
        let (p, m) = prev[idx(x)].expect("reachable");
        out.push(m);
        x = p;
    }
    out.reverse();
    out
}

#[test]
fn lansky_chest() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 4);
    let view = view_with_zone(&mut play, "etage12", &[9, 10, 11, 12], "gold");
    assert_eq!(anim(&play, "lansky_chest_door").as_deref(), Some("z1"));
    // The zones' numbers are the moves (on_click_etage12.lua): 7 → +7,
    // 8 → −7, 5 → +5, 6 → −5.
    let zone_of = |m: i32| match m {
        7 => 7,
        -7 => 8,
        5 => 5,
        _ => 6,
    };
    zone_id(&mut play, "gold", 7);
    assert_eq!(
        int(&lua(&mut play, "atl.puzzle.lansky_chest.state.gold")),
        7
    );
    assert_eq!(
        lua(&mut play, "Atl_PuzzleLanskyChest_Check()"),
        Value::Bool(false)
    );
    // Out of range: nothing moves (14 is gold's top).
    zone_id(&mut play, "gold", 8);
    zone_id(&mut play, "gold", 8);
    assert_eq!(
        int(&lua(&mut play, "atl.puzzle.lansky_chest.state.gold")),
        0
    );
    for (metal, target, max) in [("gold", 1, 14), ("silver", 11, 19), ("copper", 6, 24)] {
        let now = int(&lua(
            &mut play,
            &format!("atl.puzzle.lansky_chest.state.{metal}"),
        ));
        for m in presses(now, target, max, &[7, -7, 5, -5]) {
            zone_id(&mut play, metal, zone_of(m));
        }
    }
    assert_eq!(
        lua(&mut play, "Atl_PuzzleLanskyChest_Check()"),
        Value::Bool(true)
    );
    assert_eq!(
        anim(&play, "lansky_chest_door").as_deref(),
        Some("z2"),
        "it opens"
    );
    assert!(
        play.zones().iter().all(|z| z.1 != b"gold"),
        "the dials are off"
    );
    shot(&play, &format!("a5-puzzle-lansky-chest-{view}.png"));
}

#[test]
fn lift_board() {
    let Some(d) = dvd() else { return };
    // Chapter 4 makes the jumpers (Init_Jumpers), chapter 5 the board's
    // zones.
    let mut play = chapter(&d, 4);
    run(&mut play, "Atl_InitChapter(5)");
    let view = view_with_zone(&mut play, "etage-2", &[7, 8, 9], "drop_jumpers");
    // What the story gave before: the jumpers in the hand, the board's
    // key turned (lift_board_locked false, the key's sprite branch).
    run(
        &mut play,
        "Atl_Hero_SetHandledItem(\"jumpers\") Atl_Flag_Set(\"lift_board_locked\", false)",
    );
    zone(&mut play, "drop_jumpers");
    assert_eq!(
        lua(&mut play, "Atl_Object_GetState(\"jumpers\")"),
        s("in_liftboard")
    );
    // A wrong plug: jumper 1 at floor 3 of elevator 1, then taken back.
    sprite(&mut play, "unplugged_jumper_01");
    assert_eq!(lua(&mut play, "atl.hero.handled_item"), s("jumper_01"));
    zone_id(&mut play, "elevator1", 3);
    assert_eq!(int(&lua(&mut play, "Atl_GetJumperPluggedAt(1, 3)")), 1);
    zone_id(&mut play, "elevator1", 3);
    assert_eq!(lua(&mut play, "atl.hero.handled_item"), s("jumper_01"));
    zone_id(&mut play, "elevator1", 0);
    // The code {0, 10, 15, 9, 16}: elevator e spans code[e]..code[e + 1].
    let plugs = [(1, 10), (2, 10), (2, 15), (3, 9), (3, 15), (4, 9), (4, 16)];
    for (k, (e, floor)) in plugs.into_iter().enumerate() {
        assert_eq!(
            lua(&mut play, "Atl_Flag_Get(\"lifts_cheated\")"),
            Value::Bool(false)
        );
        sprite(&mut play, &format!("unplugged_jumper_0{}", k + 2));
        zone_id(&mut play, &format!("elevator{e}"), floor);
    }
    // Finish: the flag, the elevator zones off.
    assert_eq!(
        lua(&mut play, "Atl_Flag_Get(\"lifts_cheated\")"),
        Value::Bool(true)
    );
    assert!(play.zones().iter().all(|z| !z.1.starts_with(b"elevator")));
    shot(&play, &format!("a5-puzzle-lift-board-{view}.png"));
}

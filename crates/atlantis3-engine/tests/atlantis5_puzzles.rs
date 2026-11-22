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
        // Some views send on to another as they are entered.
        run(
            play,
            &format!("Atl_Address_Go(\"{scene}\", {v}, \"no_fx\")"),
        );
        if play.stage().address() == Some((scene.as_bytes().to_vec(), v))
            && play.zones().iter().any(|z| z.1 == name.as_bytes())
        {
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

/// One move on the chessboard: the pawn taken (it lights the squares it
/// may go to), then the square clicked.
fn chess_move(play: &mut Play, square: i32) {
    sprite(play, "player_pawn");
    assert_eq!(lua(play, "atl.hero.handled_item"), s("player_pawn"));
    let name = format!("chess_white_box_{square}");
    let id = play
        .zones()
        .into_iter()
        .find(|z| z.1 == name.as_bytes())
        .map(|z| z.0)
        .unwrap_or_else(|| panic!("{name} lit"));
    zone_id(play, &name, id);
}

fn chessboard_start(d: &std::path::Path) -> Play {
    let mut play = chapter(d, 6);
    go(&mut play, "etage28", 8);
    // Bugsy's subject starts the game (on_click_etage28.lua).
    run(
        &mut play,
        "Atl_PuzzleChessboard_Start() Atl_PuzzleChessboard_Display(8)",
    );
    play
}

#[test]
fn chessboard() {
    let Some(d) = dvd() else { return };
    let mut play = chessboard_start(&d);
    // A line found by a breadth-first walk of the scripts' own rules
    // under the reference Lua 5.0.3 (the scarab's shortest path is the
    // script's): the three crystals eaten, the scarab never on the pawn.
    for sq in [11, 7, 3, 9, 15, 19, 25, 19, 13, 17, 21, 17, 11, 7, 3, 9, 5] {
        assert_eq!(
            lua(&mut play, "Atl_Flag_Get(\"chessboard_game\")"),
            s("playing")
        );
        chess_move(&mut play, sq);
    }
    assert_eq!(
        lua(&mut play, "Atl_Flag_Get(\"chessboard_game\")"),
        s("won")
    );
    shot(&play, "a5-puzzle-chessboard.png");
}

#[test]
fn chessboard_lost() {
    let Some(d) = dvd() else { return };
    let mut play = chessboard_start(&d);
    // 3, then 9: the scarab reaches the pawn.
    chess_move(&mut play, 3);
    chess_move(&mut play, 9);
    assert_eq!(
        lua(&mut play, "Atl_Flag_Get(\"chessboard_game\")"),
        s("lost")
    );
    assert_eq!(
        lua(&mut play, "Atl_PuzzleChessboard_GetScarabeePosition()"),
        Value::Num(9.0)
    );
}

/// A move list (the slots, 0..16, whose piece goes into the hole) that
/// orders a 4 × 4 sliding puzzle: weighted A* on the Manhattan distance
/// (not the shortest, found fast). `slots[i]` is the piece of slot i, 16
/// the hole; ordered is `slots[i] == i + 1`.
fn sliding_solution(start: [u8; 16]) -> Vec<usize> {
    use std::collections::{BinaryHeap, HashMap};
    let h = |s: &[u8; 16]| -> u32 {
        let mut d = 0;
        for (i, &p) in s.iter().enumerate() {
            if p != 16 {
                let g = usize::from(p - 1);
                d += (i / 4).abs_diff(g / 4) + (i % 4).abs_diff(g % 4);
            }
        }
        u32::try_from(d).expect("small")
    };
    /// Each board's cost and the board and slot it came from.
    type Seen = HashMap<[u8; 16], (u32, Option<([u8; 16], usize)>)>;
    let mut best: Seen = HashMap::new();
    let mut open = BinaryHeap::new();
    best.insert(start, (0, None));
    open.push(std::cmp::Reverse((3 * h(&start), 0u32, start)));
    while let Some(std::cmp::Reverse((_, g, s))) = open.pop() {
        if h(&s) == 0 {
            let mut out = Vec::new();
            let mut cur = s;
            while let Some((_, Some((prev, slot)))) = best.get(&cur).copied() {
                out.push(slot);
                cur = prev;
            }
            out.reverse();
            return out;
        }
        if best.get(&s).is_some_and(|b| b.0 < g) {
            continue;
        }
        let e = s.iter().position(|&p| p == 16).expect("a hole");
        let mut next = Vec::new();
        if e >= 4 {
            next.push(e - 4);
        }
        if e % 4 < 3 {
            next.push(e + 1);
        }
        if e < 12 {
            next.push(e + 4);
        }
        if e % 4 > 0 {
            next.push(e - 1);
        }
        for slot in next {
            let mut t = s;
            t.swap(e, slot);
            if best.get(&t).is_none_or(|b| b.0 > g + 1) {
                best.insert(t, (g + 1, Some((s, slot))));
                open.push(std::cmp::Reverse((g + 1 + 3 * h(&t), g + 1, t)));
            }
        }
    }
    panic!("no solution");
}

#[test]
fn taquin() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 7);
    go(&mut play, "macao", 7);
    // The curtain opened: the slots around the hole are lit.
    sprite(&mut play, "taquin_curtain");
    let mut slots = [0u8; 16];
    for (i, v) in slots.iter_mut().enumerate() {
        *v = u8::try_from(int(&lua(
            &mut play,
            &format!("atl.puzzle.taquin.slots[{}]", i + 1),
        )))
        .expect("a piece");
    }
    assert_eq!(int(&lua(&mut play, "atl.puzzle.taquin.empty_slot")), 15);
    let moves = sliding_solution(slots);
    assert!(!moves.is_empty());
    for (k, slot) in moves.iter().enumerate() {
        assert_eq!(
            lua(&mut play, "Atl_PuzzleTaquin_Check()"),
            Value::Bool(false)
        );
        zone_id(
            &mut play,
            &format!("taquin_slot_{slot}"),
            i32::try_from(*slot).expect("slot"),
        );
        if k == 0 {
            shot(&play, "a5-puzzle-taquin-start.png");
        }
    }
    assert_eq!(
        lua(&mut play, "Atl_PuzzleTaquin_Check()"),
        Value::Bool(true)
    );
    assert_eq!(
        lua(&mut play, "Atl_Object_GetState(\"cash_room_door\")"),
        s("open")
    );
    assert_eq!(play.game.zone(b"cash_room_in").map(|z| z.1), Some(true));
    shot(&play, "a5-puzzle-taquin.png");
}

#[test]
fn winch() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 7);
    // The punter's `partie_poker` subject with the winch code in hand
    // (on_click_macao.lua): the winches' zones on, view 18.
    run(&mut play, "Atl_PuzzleWinch_Start()");
    go(&mut play, "macao", 18);
    // The zones' numbers are the winches; each turns −5..=5.
    for _ in 0..7 {
        zone_id(&mut play, "winch_left", 1);
    }
    assert_eq!(
        int(&lua(&mut play, "atl.puzzle.winch.states[1]")),
        -5,
        "kept at −5"
    );
    assert_eq!(
        lua(&mut play, "Atl_PuzzleWinch_Check(18)"),
        Value::Bool(false)
    );
    for (winch, target) in [(1, 3), (2, -4), (3, 2)] {
        loop {
            let now = int(&lua(
                &mut play,
                &format!("atl.puzzle.winch.states[{winch}]"),
            ));
            if now == target {
                break;
            }
            let way = if now < target {
                "winch_right"
            } else {
                "winch_left"
            };
            zone_id(&mut play, way, winch);
        }
    }
    // The sail goes up (film ma0010), the winches off, the poker room's
    // door on.
    assert_eq!(lua(&mut play, "Atl_Object_GetState(\"veil\")"), s("up"));
    assert_eq!(play.game.zone(b"winch_left").map(|z| z.1), Some(false));
    assert_eq!(play.game.zone(b"poker_room_in").map(|z| z.1), Some(true));
    shot(&play, "a5-puzzle-winch.png");
}

#[test]
fn vishnu_chest() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 8);
    go(&mut play, "ishtar", 20);
    // Each click turns an arm to its next of six places; 4 4 2 4 opens.
    for _ in 0..6 {
        sprite(&mut play, "vishnu_arm_1");
    }
    assert_eq!(
        int(&lua(&mut play, "atl.puzzle.vishnu_chest.arm_state[1]")),
        1,
        "round"
    );
    for (arm, target) in [(1, 4), (2, 4), (3, 2), (4, 4)] {
        assert_eq!(
            lua(
                &mut play,
                "Atl_Object_GetState(\"ishtar_palace_chest_lid\")"
            ),
            s("closed")
        );
        while int(&lua(
            &mut play,
            &format!("atl.puzzle.vishnu_chest.arm_state[{arm}]"),
        )) != target
        {
            sprite(&mut play, &format!("vishnu_arm_{arm}"));
        }
    }
    assert_eq!(
        lua(
            &mut play,
            "Atl_Object_GetState(\"ishtar_palace_chest_lid\")"
        ),
        s("open")
    );
    assert!(anim(&play, "vishnu_arm_1").is_none());
    shot(&play, "a5-puzzle-vishnu-chest.png");
}

/// The buttons of a 3 × 5 lights-out grid (`OnClicked`'s neighbours: ±5,
/// +1 unless at a row's end, −1 unless at its start) that turn every
/// light off from all on: the first subset found.
fn light_out_solution() -> Vec<i32> {
    let toggles = |b: i32| -> u32 {
        let mut m = 1 << (b - 1);
        if b - 5 > 0 {
            m |= 1 << (b - 6);
        }
        if b + 5 < 16 {
            m |= 1 << (b + 4);
        }
        if b % 5 != 0 {
            m |= 1 << b;
        }
        if b % 5 != 1 {
            m |= 1 << (b - 2);
        }
        m
    };
    (1u32..1 << 15)
        .map(|set| {
            let presses: Vec<i32> = (1..=15).filter(|b| set & (1 << (b - 1)) != 0).collect();
            let lit = presses.iter().fold(0x7fff, |g, &b| g ^ toggles(b));
            (presses, lit)
        })
        .find(|(_, lit)| *lit == 0)
        .map(|(p, _)| p)
        .expect("a solution")
}

#[test]
fn light_out() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 8);
    go(&mut play, "ishtar", 18);
    let presses = light_out_solution();
    // One press and back: unsolved, as before.
    zone_id(&mut play, "light_out", presses[0]);
    zone_id(&mut play, "light_out", presses[0]);
    assert_eq!(
        lua(&mut play, "Atl_PuzzleLightOut_Check()"),
        Value::Bool(false)
    );
    for &b in &presses {
        assert_eq!(
            lua(&mut play, "Atl_Object_GetState(\"vishnu_statuette\")"),
            s("closed")
        );
        zone_id(&mut play, "light_out", b);
    }
    // The statuette opens on the ruby; the buttons go off.
    assert_eq!(
        lua(&mut play, "Atl_Object_GetState(\"vishnu_statuette\")"),
        s("open")
    );
    assert_eq!(play.game.zone(b"light_out").map(|z| z.1), Some(false));
    shot(&play, "a5-puzzle-light-out.png");
}

#[test]
fn sudoku() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 8);
    go(&mut play, "ishtar", 30);
    // The grid `DoSudoku` (the scripts' own debug shortcut) sets: the
    // answer.
    const ANSWER: [i32; 81] = [
        7, 3, 2, 4, 9, 8, 5, 1, 6, 4, 5, 1, 7, 6, 2, 9, 3, 8, 9, 6, 8, 5, 1, 3, 4, 7, 2, 6, 7, 4,
        3, 8, 1, 2, 9, 5, 1, 9, 5, 2, 4, 7, 8, 6, 3, 8, 2, 3, 6, 5, 9, 1, 4, 7, 3, 1, 7, 8, 2, 4,
        6, 5, 9, 5, 8, 9, 1, 3, 6, 7, 2, 4, 2, 4, 6, 9, 7, 5, 3, 8, 1,
    ];
    // The given figures have no zone.
    assert!(!play.zones().iter().any(|z| z.0 == 3 && z.1 == b"sudoku"));
    for (i, &v) in ANSWER.iter().enumerate() {
        let case = i32::try_from(i + 1).expect("case");
        while int(&lua(&mut play, &format!("atl.puzzle.sudoku.grid[{case}]"))) != v {
            assert_eq!(play.stage().address(), Some((b"ishtar".to_vec(), 30)));
            zone_id(&mut play, "sudoku", case);
        }
    }
    // Solved: film is0005, then view 27.
    assert_eq!(
        lua(&mut play, "Atl_PuzzleSudoku_Check()"),
        Value::Bool(true)
    );
    assert_eq!(play.stage().address(), Some((b"ishtar".to_vec(), 27)));
}

#[test]
fn lazer() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 8);
    // The four slots lit as the story lights them (on_click_ishtar.lua).
    run(
        &mut play,
        "for i = 1, 4 do Atl_Zone_SetActivity(\"gem_slot_\"..i, true) end",
    );
    let views: Vec<i32> = (14..=26).collect();
    let view = view_with_zone(&mut play, "ishtar", &views, "gem_slot_1");
    let put = |play: &mut Play, stone: &str, slot: i32| {
        let name = format!("gem_slot_{slot}");
        if !play.zones().iter().any(|z| z.1 == name.as_bytes()) {
            view_with_zone(play, "ishtar", &views, &name);
        }
        run(play, &format!("Atl_Hero_SetHandledItem(\"{stone}\")"));
        let id = play
            .zones()
            .into_iter()
            .find(|z| z.1 == name.as_bytes())
            .map(|z| z.0)
            .expect("the slot");
        zone_id(play, &name, id);
    };
    // A wrong stone in slot 1, taken back with an empty hand.
    put(&mut play, "ruby", 1);
    assert_eq!(lua(&mut play, "atl.puzzle.lazer.slots[1]"), s("ruby"));
    let id = play
        .zones()
        .into_iter()
        .find(|z| z.1 == b"gem_slot_1")
        .map(|z| z.0)
        .expect("slot 1");
    zone_id(&mut play, "gem_slot_1", id);
    assert_eq!(lua(&mut play, "atl.hero.handled_item"), s("ruby"));
    run(&mut play, "Atl_Hero_SetHandledItem(nil)");
    for (slot, stone) in [
        (1, "emerald"),
        (2, "sapphire"),
        (3, "ruby"),
        (4, "moon_stone"),
    ] {
        assert_eq!(
            lua(&mut play, "Atl_Flag_Get(\"mission_light_ray\")") == s("done"),
            false
        );
        put(&mut play, stone, slot);
    }
    assert_eq!(lua(&mut play, "Atl_PuzzleLazer_Check()"), Value::Bool(true));
    assert_eq!(
        lua(&mut play, "Atl_Flag_Get(\"mission_light_ray\")"),
        s("done")
    );
    shot(&play, &format!("a5-puzzle-lazer-{view}.png"));
}

#[test]
fn bas_relief() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 8);
    // Ishtar on the flagstone lights the bas-relief (info_bas_relief).
    run(&mut play, "Atl_Zone_SetActivity(\"bas_relief\", true)");
    let view = view_with_zone(
        &mut play,
        "ishtar",
        &(1..=40).collect::<Vec<_>>(),
        "bas_relief",
    );
    // Six wrong presses: the code starts again.
    for _ in 0..6 {
        zone_id(&mut play, "bas_relief", 1);
    }
    assert_eq!(
        int(&lua(&mut play, "atl.puzzle.bas_relief.code_counter")),
        0
    );
    for part in [1, 2, 2, 2, 1, 1] {
        assert_eq!(
            lua(&mut play, "Atl_Object_GetState(\"ishtar_palace_door_2\")"),
            s("closed")
        );
        zone_id(&mut play, "bas_relief", part);
    }
    assert_eq!(
        lua(&mut play, "Atl_Object_GetState(\"ishtar_palace_door_2\")"),
        s("open")
    );
    assert_eq!(play.game.zone(b"bas_relief").map(|z| z.1), Some(false));
    shot(&play, &format!("a5-puzzle-bas-relief-{view}.png"));
}

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
use atlantis5_script::{Value, LUA_MEMORY_LIMIT};

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
    if std::env::var_os("A5_GC_STRESS").is_some() {
        play.game.script().gc_pace(100, 1000);
    }
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
    // A sprite still at the start of its animation may cover nothing yet:
    // the player waits for it.
    let mut aimed = play.aim_at_sprite(object.as_bytes());
    for _ in 0..10 {
        if aimed {
            break;
        }
        frames(play, 10);
        aimed = play.aim_at_sprite(object.as_bytes());
    }
    assert!(aimed, "{object} under the centre");
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

/// `GetShortestPath(p, p)`, asked when the pawn moves onto the scarab's
/// square (the lit squares do not exclude it): the breadth-first loop
/// never meets `End` and adds an empty level per turn. Lua 5.0.3 would
/// raise `table overflow` at the 2^25th level (about 3.5 GB on a 64-bit
/// build); the original, a 32-bit program, runs out of memory first, and
/// so does Native's state at its bound ([`LUA_MEMORY_LIMIT`]): a Lua
/// error the click's script logs, after which the game goes on. Here
/// with a small bound, so that it is quick.
#[test]
fn chessboard_path_to_the_same_square_ends_on_a_memory_error() {
    let Some(d) = dvd() else { return };
    let mut play = chessboard_start(&d);
    assert_eq!(play.game.script().lua_memory().1, LUA_MEMORY_LIMIT);
    play.game.script().set_lua_memory_limit(64 << 20);
    run(
        &mut play,
        "chess_ok, chess_err = pcall(Atl_PuzzleChessboard_GetShortestPath, 13, 13)",
    );
    assert_eq!(lua(&mut play, "chess_ok"), Value::Bool(false));
    assert_eq!(lua(&mut play, "chess_err"), s("not enough memory"));
    // The tree is garbage: the state works on and the game plays.
    assert_eq!(
        lua(&mut play, "string.len(string.rep(\"ab\", 3))"),
        Value::Num(6.0)
    );
    chess_move(&mut play, 3);
    chess_move(&mut play, 9);
    assert_eq!(
        lua(&mut play, "Atl_Flag_Get(\"chessboard_game\")"),
        s("lost")
    );
}

/// The same at the real bound: how long and how much (run on demand,
/// `--ignored --nocapture`).
#[test]
#[ignore = "about 1.5 GB of memory and 5 s"]
fn chessboard_path_to_the_same_square_at_the_real_bound() {
    let Some(d) = dvd() else { return };
    let mut play = chessboard_start(&d);
    let t = std::time::Instant::now();
    run(
        &mut play,
        "chess_ok, chess_err = pcall(Atl_PuzzleChessboard_GetShortestPath, 13, 13)",
    );
    eprintln!(
        "ended in {:.1} s, Lua memory {:?}",
        t.elapsed().as_secs_f64(),
        play.game.script().lua_memory()
    );
    assert_eq!(lua(&mut play, "chess_err"), s("not enough memory"));
    assert_eq!(
        lua(&mut play, "string.len(string.rep(\"ab\", 3))"),
        Value::Num(6.0)
    );
    eprintln!("after: Lua memory {:?}", play.game.script().lua_memory());
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

/// A click's `Atl_Sprite_PlayRebound(.., "loop", 0, v − 1, v − 1)` (a
/// new player at frame 0, @ 0x00448d00) is stepped to its figure before
/// it is first drawn: state 4 runs the scene's update (@ 0x0044c380)
/// after the frame's events (@ 0x0042f000 case 4).
#[test]
fn a_clicked_sudoku_figure_shows_its_frame_at_once() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 8);
    go(&mut play, "ishtar", 30);
    let id = play
        .zones()
        .into_iter()
        .find(|z| z.1 == b"sudoku")
        .map(|z| z.0)
        .expect("a sudoku case");
    let (x, y) = ((id - 1) % 9, (id - 1) / 9);
    let name = format!(
        "sudoku_{}{}",
        3 * (y / 3) + x / 3 + 1,
        3 * (y % 3) + x % 3 + 1
    );
    for _ in 0..3 {
        assert!(play.aim_at_zone_named(id, b"sudoku"));
        play.click();
        // One frame: the click's script, then the scene's update, then the
        // picture.
        play.update(1.0 / 30.0);
        assert!(!play.game.busy());
        let v = int(&lua(&mut play, &format!("atl.puzzle.sudoku.grid[{id}]")));
        let frame = play
            .stage()
            .sprites()
            .into_iter()
            .find(|s| s.0 == name)
            .map(|s| s.2);
        if v == 0 {
            continue;
        }
        assert_eq!(frame, Some(usize::try_from(v - 1).unwrap()), "{name} = {v}");
    }
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
        assert_ne!(
            lua(&mut play, "Atl_Flag_Get(\"mission_light_ray\")"),
            s("done")
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

/// The trap's buttons to press, from step 4 up, until steps 2..=10 are
/// up: a breadth-first walk of `Atl_PuzzleTrap_OnClick`'s rule (a button
/// can be pressed only while its step is up; it goes down and turns its
/// `actions` over; with 4..=10 all up, step 1 comes up, which the script
/// asserts was down).
fn trap_solution() -> Vec<usize> {
    const ACTIONS: [&[usize]; 11] = [
        &[],
        &[2, 3],
        &[3, 6, 9],
        &[5, 8],
        &[6, 8, 10],
        &[3, 9],
        &[2, 4, 7],
        &[3, 10],
        &[4, 5, 7],
        &[4, 10],
        &[2, 5, 9],
    ];
    let up = |s: u16, i: usize| s & (1 << i) != 0;
    let start: u16 = 1 << 4;
    let goal = |s: u16| (2..=10).all(|i| up(s, i));
    let mut prev = std::collections::HashMap::from([(start, (start, 0))]);
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(s) = queue.pop_front() {
        if goal(s) {
            let mut out = Vec::new();
            let mut x = s;
            while x != start {
                let (p, b) = prev[&x];
                out.push(b);
                x = p;
            }
            out.reverse();
            return out;
        }
        for b in (1..=10).filter(|&b| up(s, b)) {
            let mut t = s & !(1 << b);
            if b == 1 {
                t |= (1 << 2) | (1 << 3);
            } else {
                for &a in ACTIONS[b] {
                    t ^= 1 << a;
                }
                if (4..=10).all(|i| up(t, i)) {
                    if up(t, 1) {
                        continue;
                    }
                    t |= 1 << 1;
                }
            }
            if let std::collections::hash_map::Entry::Vacant(e) = prev.entry(t) {
                e.insert((s, b));
                queue.push_back(t);
            }
        }
    }
    panic!("no solution");
}

#[test]
fn trap() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 9);
    go(&mut play, "mesopotamie", 23);
    // The ring pulled thrice: step 4 comes up (on_click_mesopotamie.lua).
    run(&mut play, "Atl_PuzzleTrap_Activate()");
    let presses = trap_solution();
    for &b in &presses {
        assert_eq!(
            lua(&mut play, "Atl_PuzzleTrap_IsSolved()"),
            Value::Bool(false)
        );
        sprite(&mut play, &format!("trap_button_{b}"));
    }
    assert_eq!(
        lua(&mut play, "Atl_PuzzleTrap_IsSolved()"),
        Value::Bool(true)
    );
    assert_eq!(play.game.zone(b"trap_upstairs").map(|z| z.1), Some(true));
    shot(&play, "a5-puzzle-trap.png");
}

fn hand(play: &mut Play) -> Value {
    lua(play, "atl.hero.handled_item")
}

#[test]
fn stone_cutting() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 9);
    go(&mut play, "mesopotamie", 8);
    // A rough stone (`gross_stone_heap` gives `stone1_1`) on the bench.
    run(&mut play, "Atl_Hero_SetHandledItem(\"stone1_1\")");
    zone(&mut play, "cut_zone");
    assert_eq!(hand(&mut play), Value::Nil);
    // Cut twice with the sculptor's tools: size 3; taken: `stone1_3`.
    for _ in 0..2 {
        run(&mut play, "Atl_Hero_SetHandledItem(\"sculptor_tools\")");
        sprite(&mut play, "gross_stone");
    }
    assert_eq!(int(&lua(&mut play, "atl.puzzle.stone_cut.size")), 3);
    run(&mut play, "Atl_Hero_SetHandledItem(nil)");
    sprite(&mut play, "gross_stone");
    assert_eq!(hand(&mut play), s("stone1_3"));
    shot(&play, "a5-puzzle-stone-cutting.png");
    // Cut too far: at size 9 the stone breaks (dialog 74), the bench free.
    zone(&mut play, "cut_zone");
    for _ in 0..6 {
        run(&mut play, "Atl_Hero_SetHandledItem(\"sculptor_tools\")");
        sprite(&mut play, "gross_stone");
    }
    assert_eq!(
        int(&lua(&mut play, "Atl_PuzzleStoneCutting_GetNumStone()")),
        0
    );
    assert!(anim(&play, "gross_stone").is_none());
    assert_eq!(play.game.zone(b"cut_zone").map(|z| z.1), Some(true));
}

#[test]
fn stone_painting() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 9);
    go(&mut play, "mesopotamie", 17);
    // A stone of the wrong size does not go in (dialog 31).
    run(&mut play, "Atl_Hero_SetHandledItem(\"stone1_4\")");
    zone(&mut play, "marduk_stone_1");
    assert_eq!(hand(&mut play), s("stone1_4"));
    // The cut stones (sizes 3, 5, 8, 8) in their holes, then the paints
    // the blender makes (`paint<n>_<colour>`): db, lo, lb, do.
    for (n, size) in [(1, 3), (2, 5), (3, 8), (4, 8)] {
        run(
            &mut play,
            &format!("Atl_Hero_SetHandledItem(\"stone{n}_{size}\")"),
        );
        zone(&mut play, &format!("marduk_stone_{n}"));
        assert_eq!(hand(&mut play), Value::Nil);
    }
    for (n, colour) in [(1, "db"), (2, "lo"), (3, "lb"), (4, "do")] {
        assert_eq!(
            lua(&mut play, "Atl_PuzzleStonePainting_Check()"),
            Value::Bool(false)
        );
        run(
            &mut play,
            &format!("Atl_Hero_SetHandledItem(\"paint{n}_{colour}\")"),
        );
        sprite(&mut play, &format!("marduk_stone_{n}"));
    }
    assert_eq!(
        lua(&mut play, "Atl_PuzzleStonePainting_Check()"),
        Value::Bool(true)
    );
    assert_eq!(
        lua(&mut play, "atl.objects.marduk_stone_1.warp_reachable[1]"),
        Value::Nil,
        "the stones are done"
    );
    shot(&play, "a5-puzzle-stone-painting.png");
}

#[test]
fn blender() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 9);
    go(&mut play, "mesopotamie", 9);
    // Neither fed nor lit: the main valve gives nothing.
    sprite(&mut play, "nisir_main_valve");
    assert_eq!(lua(&mut play, "atl.puzzle.blender.state"), s("ready"));
    // Fed with the refined madder, lit with the matches.
    run(&mut play, "Atl_Hero_SetHandledItem(\"refined_garance\")");
    zone(&mut play, "blender_input");
    run(&mut play, "Atl_Hero_SetHandledItem(\"match_box\")");
    zone(&mut play, "blender_light_zone");
    run(&mut play, "Atl_Hero_SetHandledItem(nil)");
    // Each valve pair gives one paint: both open light orange, the left
    // dark orange, the right light blue, none dark blue.
    let mut left = false;
    let mut right = false;
    for (l, r, sprite_name, colour) in [
        (true, true, "paint_light_orange", "lo"),
        (true, false, "paint_dark_orange", "do"),
        (false, true, "paint_light_blue", "lb"),
        (false, false, "paint_dark_blue", "db"),
    ] {
        if l != left {
            sprite(&mut play, "nisir_left_valve");
            left = l;
        }
        if r != right {
            sprite(&mut play, "nisir_right_valve");
            right = r;
        }
        sprite(&mut play, "nisir_main_valve");
        sprite(&mut play, sprite_name);
        let held = hand(&mut play);
        let Value::Str(h) = &held else {
            panic!("{held:?}")
        };
        assert!(
            h.ends_with(colour.as_bytes()),
            "{}",
            String::from_utf8_lossy(h)
        );
        // Into the inventory (the right button, ArrangeHandledObject).
        play.right_click();
        settle(&mut play);
        assert_eq!(hand(&mut play), Value::Nil);
    }
    for p in 1..=4 {
        assert_eq!(
            lua(
                &mut play,
                &format!("Atl_Hero_HasItemPrefixed(\"paint{p}_\")")
            ),
            Value::Bool(true)
        );
    }
    // A fifth: too much paint (dialog 67), nothing made.
    sprite(&mut play, "nisir_main_valve");
    assert_eq!(lua(&mut play, "atl.puzzle.blender.state"), s("ready"));
    shot(&play, "a5-puzzle-blender.png");
}

#[test]
fn gridlock() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 9);
    go(&mut play, "mesopotamie", 19);
    // The Atlantean crystal on the door (gridlock_reset, lit by the story)
    // sets the cars and lights the zones they can move by.
    let reset = |play: &mut Play| {
        run(
            play,
            "Atl_Zone_SetActivity(\"gridlock_reset\", true) \
             Atl_Hero_SetHandledItem(\"atlantean_cristal\")",
        );
        zone(play, "gridlock_reset");
        // A second on, the crystal comes back to the hand (the timer
        // `gridlock_crystal`); the right button puts it away.
        frames(play, 40);
        settle(play);
        assert_eq!(hand(play), s("atlantean_cristal"));
        play.right_click();
        settle(play);
        assert_eq!(hand(play), Value::Nil);
    };
    reset(&mut play);
    let start = lua(&mut play, "atl.puzzle.gridlock.cars.eb[1]");
    zone_id(&mut play, "gridlock_3", 3);
    assert_ne!(
        lua(&mut play, "atl.puzzle.gridlock.cars.eb[1]"),
        start,
        "eb moved"
    );
    reset(&mut play);
    assert_eq!(
        lua(&mut play, "atl.puzzle.gridlock.cars.eb[1]"),
        start,
        "set again"
    );
    // A line found by a breadth-first walk of the script's own rules under
    // the reference Lua 5.0.3 (the zones it lights are the moves); the
    // last click takes `ea` out by zone 18.
    let line = [
        3, 17, 11, 22, 15, 8, 9, 21, 14, 15, 19, 13, 7, 20, 14, 32, 27, 22, 21, 20, 27, 28, 32, 33,
        34, 27, 20, 13, 2, 35, 28, 21, 14, 9, 20, 15, 26, 21, 28, 22, 16, 10, 15, 16, 29, 24, 17,
        18,
    ];
    for z in line {
        assert_eq!(
            lua(&mut play, "Atl_PuzzleGridlock_IsSolved()"),
            Value::Bool(false)
        );
        zone_id(&mut play, &format!("gridlock_{z}"), z);
    }
    assert_eq!(
        lua(&mut play, "Atl_PuzzleGridlock_IsSolved()"),
        Value::Bool(true)
    );
    // `opening`, which the display turns to `open`.
    assert_eq!(
        lua(&mut play, "Atl_Object_GetState(\"gridlock_door\")"),
        s("open")
    );
    assert_eq!(play.game.zone(b"trap_room_in").map(|z| z.1), Some(true));
    shot(&play, "a5-puzzle-gridlock.png");
}

/// The bees to move, from `slots` (0 the hole), until each side holds
/// the other's (`RefreshActivity`'s rule: a bee of the left, numbered
/// below `center`, steps or jumps right into the hole; one of the right
/// left), by a depth-first walk; `dead` instead finds a line that leaves
/// no bee to move before the end.
fn bees_line(slots: &[i32], center: i32, dead: bool) -> Option<Vec<i32>> {
    let n = slots.len();
    let hole = slots.iter().position(|&b| b == 0)?;
    let mut moves = Vec::new();
    for (c, right) in [
        (hole.wrapping_sub(2), true),
        (hole.wrapping_sub(1), true),
        (hole + 1, false),
        (hole + 2, false),
    ] {
        if c < n && slots[c] != 0 && ((slots[c] < center) == right) {
            moves.push(c);
        }
    }
    let solved = (0..n).all(|s| {
        let s1 = i32::try_from(s + 1).expect("slot");
        if s1 < center {
            slots[s] >= center
        } else if s1 > center {
            slots[s] != 0 && slots[s] < center
        } else {
            true
        }
    });
    if moves.is_empty() {
        return if solved != dead {
            Some(Vec::new())
        } else {
            None
        };
    }
    for c in moves {
        let mut t = slots.to_vec();
        t.swap(c, hole);
        if let Some(mut rest) = bees_line(&t, center, dead) {
            rest.insert(0, slots[c]);
            return Some(rest);
        }
    }
    None
}

fn bees_slots(play: &mut Play) -> (Vec<i32>, i32) {
    let level = int(&lua(play, "atl.puzzle.bees.level"));
    let nb = int(&lua(play, &format!("atl.puzzle.bees.nb_bees[{level}]")));
    let slots = (1..=2 * nb + 1)
        .map(|s| int(&lua(play, &format!("atl.puzzle.bees.slots[{s}]"))))
        .collect();
    (slots, nb + 1)
}

fn bee(play: &mut Play, b: i32) {
    let level = int(&lua(play, "atl.puzzle.bees.level"));
    run(play, "Atl_Hero_SetHandledItem(\"bee_smoker_on\")");
    sprite(play, &format!("bee_{level}{b}"));
}

#[test]
fn bees() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 9);
    // The swarm smoked away from the hive (the story's step before).
    run(&mut play, "Atl_Object_SetState(\"bees_cloud\", \"away\")");
    go(&mut play, "mesopotamie", 13);
    sprite(&mut play, "hive_lid");
    assert_eq!(int(&lua(&mut play, "atl.puzzle.bees.level")), 1);
    // A line that blocks the bees: the level starts again.
    let (slots, center) = bees_slots(&mut play);
    let dead = bees_line(&slots, center, true).expect("a dead end");
    for b in dead {
        bee(&mut play, b);
    }
    assert_eq!(bees_slots(&mut play).0, slots, "level 1 again");
    for level in 1..=3 {
        assert_eq!(int(&lua(&mut play, "atl.puzzle.bees.level")), level);
        let (slots, center) = bees_slots(&mut play);
        for b in bees_line(&slots, center, false).expect("a solution") {
            bee(&mut play, b);
        }
        // Solved: the level's tray can be taken off.
        assert_eq!(
            int(&lua(
                &mut play,
                &format!("atl.objects.hive_level_{level}.warp_reachable[1]")
            )),
            13
        );
        run(&mut play, "Atl_Hero_SetHandledItem(nil)");
        if level < 3 {
            sprite(&mut play, &format!("hive_level_{level}"));
        }
    }
    shot(&play, "a5-puzzle-bees.png");
}

/// A search for the hex player's moves, run in the game's own Lua state
/// on the scripts' own `Atl_PuzzleHex_OnClick` (the computer's answer
/// included, its `math.random` replayed from the state `seed`): at each
/// ply the moves are tried, ranked by the player's resistance after them
/// (the script's `resistance_board_2`), the `beam` best kept; a line is
/// dropped when the computer's resistance falls to 0 (`Check`'s 1). The
/// board, the generator and the zones are put back at the end, the
/// pawns the search showed taken off. Returns "x,y;..."
/// or nil.
const HEX_SEARCH: &str = r#"
local seed, beam, depth = ...
local hx = atl.puzzle.hex
local function cp(b) local r = {} for y = 1, 7 do r[y] = {} for x = 1, 7 do r[y][x] = b[y][x] end end return r end
local function min2() local m for i = 1, 7 do local v = hx.resistance_board_2[1][i] if type(v) == "number" and (m == nil or m > v) then m = v end end return m end
local function min1() local m for i = 1, 7 do local v = hx.resistance_board_1[i][7] if type(v) == "number" and (m == nil or m > v) then m = v end end return m end
local function next_seed(s) return math.mod(s * 214013 + 2531011, 4294967296) end
local start, start_seed = cp(hx.board_state), seed
local function rec(board, s, d, path)
  local cands, n = {}, 0
  for y = 1, 7 do for x = 1, 7 do if board[y][x] == 0 then
    hx.board_state = cp(board)
    math.randomseed(s)
    Atl_PuzzleHex_OnClick(29, x, y)
    local h, c = min2(), min1()
    local s2 = s
    if h ~= nil and c ~= nil then s2 = next_seed(s) end
    local p = path .. x .. "," .. y .. ";"
    if h == 0 then return p end
    Atl_PuzzleHex_CalculateResistanceBoard(1, hx.resistance_board_1)
    local c2 = min1()
    if c2 ~= 0 and h ~= nil then
      n = n + 1
      cands[n] = { h = h, c = c2 or 99, board = cp(hx.board_state), s = s2, p = p }
    end
  end end end
  if d == 0 then return nil end
  for i = 2, n do
    local v, j = cands[i], i - 1
    while j >= 1 and (cands[j].h > v.h or (cands[j].h == v.h and cands[j].c < v.c)) do cands[j + 1] = cands[j] j = j - 1 end
    cands[j + 1] = v
  end
  for i = 1, math.min(beam, n) do
    local r = rec(cands[i].board, cands[i].s, d - 1, cands[i].p)
    if r then return r end
  end
  return nil
end
local r = rec(start, seed, depth, "")
hx.board_state = start
math.randomseed(start_seed)
for y = 1, 7 do for x = 1, 7 do Atl_Sprite_Stop("hex_pawn_" .. string.sub("abcdefg", x, x) .. y) end end
Atl_PuzzleHex_SetActivityZone()
return r
"#;

#[test]
fn hex() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 9);
    go(&mut play, "mesopotamie", 29);
    // The Atlantean crystal on the ark's console lights the board.
    run(&mut play, "Atl_Hero_SetHandledItem(\"atlantean_cristal\")");
    zone(&mut play, "ark_crystal_place");
    settle(&mut play);
    let seed = play.game.script().lua_holdrand();
    let found = play
        .game
        .script()
        .one_shot(
            &format!("return (function(...) {HEX_SEARCH} end)({seed}, 3, 12)"),
            None,
            1,
        )
        .expect("the search")
        .remove(0);
    assert_eq!(
        play.game.script().lua_holdrand(),
        seed,
        "the generator put back"
    );
    let Value::Str(line) = found else {
        panic!("no winning line: {found:?}")
    };
    let line = String::from_utf8(line).expect("text");
    for mv in line.split(';').filter(|m| !m.is_empty()) {
        assert_eq!(
            lua(&mut play, "Atl_Object_GetState(\"atlantean_crystal\")"),
            s("on_ark_console_off")
        );
        let (x, y) = mv.split_once(',').expect("x,y");
        let name = format!("hex_pawn_{x}{y}");
        let id = play
            .zones()
            .into_iter()
            .find(|z| z.1 == name.as_bytes())
            .map(|z| z.0)
            .unwrap_or_else(|| panic!("{name} free"));
        zone_id(&mut play, &name, id);
    }
    // The player's path joins the two sides (Check's 2): the crystal lit.
    assert_eq!(
        lua(&mut play, "Atl_Object_GetState(\"atlantean_crystal\")"),
        s("on_ark_console_on")
    );
    shot(&play, "a5-puzzle-hex.png");
}

#[test]
fn hex_lost() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 9);
    go(&mut play, "mesopotamie", 29);
    run(&mut play, "Atl_Hero_SetHandledItem(\"atlantean_cristal\")");
    zone(&mut play, "ark_crystal_place");
    settle(&mut play);
    // The player fills the top rows from the left, letting the computer
    // join its sides: film me0016, the death panel (`_gameover_ark`).
    'play: for y in 1..=7 {
        for x in 1..=7 {
            let name = format!("hex_pawn_{x}{y}");
            let Some(id) = play
                .zones()
                .into_iter()
                .find(|z| z.1 == name.as_bytes())
                .map(|z| z.0)
            else {
                continue;
            };
            assert!(play.aim_at_zone_named(id, name.as_bytes()));
            play.click();
            for _ in 0..300 {
                frames(&mut play, 1);
                if play.game.state() != 4 || !play.game.busy() {
                    break;
                }
            }
            if play.game.state() == 3 {
                break 'play;
            }
            settle(&mut play);
        }
    }
    frames(&mut play, 5);
    assert_eq!(
        play.menus.current(),
        Some(atlantis3_engine::atlantis5_menu::DEATH),
        "the death panel"
    );
}

#[test]
fn pipe() {
    let Some(d) = dvd() else { return };
    let mut play = chapter(&d, 1);
    go(&mut play, "zeppelin", 25);
    let done = |play: &mut Play| lua(play, "Atl_Flag_Get(\"mission_fix_pipe\")") == s("done");
    // The broken pipe not patched yet: the valves alone fix nothing.
    sprite(&mut play, "valve_01");
    sprite(&mut play, "valve_03");
    assert_eq!(lua(&mut play, "IsPipeFixed()"), Value::Bool(false));
    assert!(!done(&mut play));
    sprite(&mut play, "valve_01");
    sprite(&mut play, "valve_03");
    // The healthy pipe set as the patch (the story's step before,
    // on_click_zeppelin.lua), then valve 1 open, valve 3 closed with the
    // gas lever open: IsPipeFixed.
    run(
        &mut play,
        "Atl_Object_SetState(\"healthy_pipe\", \"patch\")",
    );
    assert_eq!(
        lua(&mut play, "Atl_Object_GetState(\"gaz_lever\")"),
        s("open")
    );
    sprite(&mut play, "valve_01");
    assert!(!done(&mut play));
    sprite(&mut play, "valve_03");
    assert_eq!(lua(&mut play, "IsPipeFixed()"), Value::Bool(true));
    assert!(done(&mut play));
    assert_eq!(
        lua(&mut play, "atl.objects.valve_01.warp_reachable[1]"),
        Value::Nil
    );
    shot(&play, "a5-puzzle-pipe.png");
}

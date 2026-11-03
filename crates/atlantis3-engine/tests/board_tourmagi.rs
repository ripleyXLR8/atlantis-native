//! The magician's tower (world 3) against the original's C code under
//! emulation:
//!
//! - the game boy's dice (`GBOY`, loop mode 12; `tests/golden/puzzle_gboy.txt`):
//!   the start, every frame's state, sprite calls and screen md5, the stop,
//!   for the six results, early / late / off-button clicks, the range's
//!   edges and the right button;
//! - the race (`puzzle_tower_race.txt`, replayed through [`common::pz`]):
//!   the race step, the demon's pawns, the code's own view entries, the
//!   stop-alls and the music volume around the dice, the dice thrown by
//!   clicks in the real `GBOY` play; a player's win and a demon's win.
//!
//! Needs the game: `ATLANTIS3_DATA`.

mod common;

use atlantis3_data::A3Files;
use atlantis3_engine::Game;
use common::board::{assert_same, draws, md5_screen, runs, Clock};
use common::pz::{replay_all, Spec};

fn files() -> Option<impl Fn() -> A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(move || A3Files::open_dir(&dir).unwrap().0)
}

/// A field `key=` of a state line.
fn field<'a>(st: &'a str, key: &str) -> &'a str {
    st.split_whitespace()
        .find_map(|w| w.strip_prefix(key))
        .unwrap_or_else(|| panic!("{st}: no {key}"))
}

/// The game boy's runs: the start at 100 s in view 39 of `TourMagi.wam`,
/// then one frame of the mode per frame of the golden, its inputs before
/// it, until the stop.
#[test]
fn the_game_boy_as_the_original() {
    let Some(files) = files() else {
        return;
    };
    let golden = include_str!("golden/puzzle_gboy.txt");
    let clock = Clock::PLACE;
    let mut names = Vec::new();
    for run in runs(golden).into_iter().skip(1) {
        let mut g = Game::new(files()).unwrap();
        g.trace = Some(Vec::new());
        g.trace_digest = Some(md5_screen);
        g.flat_warp = true;
        assert!(g.board_begin("TourMagi.wam", 3));
        assert_eq!((g.width(), g.height()), (800, 600));
        g.board_enter(39, 50.0);
        g.trace.as_mut().unwrap().clear();
        g.board_gboy_start(clock.at(0));
        let mut out = Vec::new();
        let tower = g.board_st("tower").unwrap();
        let st = g.board_st("gboy_full").unwrap();
        out.push(format!(
            "start 0 mode={} old={} cursor={} vol={} saved={}",
            g.board_mode_number(),
            g.board_screen_back(),
            field(&st, "cur="),
            field(&tower, "vol="),
            field(&tower, "saved=")
        ));
        out.push(format!("st 0 {st}"));
        let mut last = st;
        g.trace.as_mut().unwrap().clear();
        // The inputs by frame.
        let mut inputs: Vec<(u32, &str, (i32, i32))> = Vec::new();
        let mut end = 0;
        for l in &run.lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w[0] {
                "click" | "rclick" => inputs.push((
                    w[1].parse().unwrap(),
                    w[0],
                    (w[2].parse().unwrap(), w[3].parse().unwrap()),
                )),
                "stop" => end = w[1].parse().unwrap(),
                _ => {}
            }
        }
        let mut ptr = (400, 300);
        for f in 1..=end {
            let (mut left, mut right) = (false, false);
            for &(at, kind, p) in &inputs {
                if at == f {
                    ptr = p;
                    left |= kind == "click";
                    right |= kind == "rclick";
                    out.push(format!("{kind} {f} {} {}", p.0, p.1));
                }
            }
            let stay = g.board_screen_frame(clock.at(f), ptr, left, right);
            let trace = std::mem::take(g.trace.as_mut().unwrap());
            let (d, flips) = draws(&trace);
            if let Some(m) = flips.last() {
                let d: Vec<String> = d
                    .iter()
                    .filter(|x| x.as_str() != "warp=flat")
                    .map(|x| x.trim_end_matches("*0000803f").to_string())
                    .collect();
                out.push(format!("draw {f} {}", d.join(" ")));
                out.push(format!("md5 {f} {m}"));
            }
            let st = g.board_st("gboy_full").unwrap();
            if st != last {
                out.push(format!("st {f} {st}"));
                last = st;
            }
            if !stay {
                out.push(format!("ret {f} 0"));
            }
            for t in trace.iter().filter(|t| {
                !(t.as_str() == "warp=flat"
                    || t.as_str() == "stop"
                    || t.starts_with("aff ")
                    || t.starts_with("affc ")
                    || t.starts_with("flip ")
                    || t.starts_with("snd "))
            }) {
                out.push(match t.split_once(' ') {
                    Some((k, r)) => format!("{k} {f} {r}"),
                    None => format!("{t} {f}"),
                });
            }
            if !stay {
                let tower = g.board_st("tower").unwrap();
                out.push(format!(
                    "stop {f} mode={} vol={} result={}",
                    g.board_mode_number(),
                    field(&tower, "vol="),
                    field(&tower, "res=")
                ));
                out.push(format!(
                    "end {} frames={f} done=1",
                    run.name.split_whitespace().next().unwrap()
                ));
                break;
            }
        }
        let want: Vec<String> = run.lines.iter().map(|l| l.to_string()).collect();
        assert_same(&format!("run {}", run.name), &want, &out);
        names.push(run.name.split_whitespace().next().unwrap().to_string());
    }
    assert_eq!(
        names,
        [
            "result1",
            "result2",
            "result3",
            "result4",
            "result5",
            "result6",
            "early_then_late_then_5",
            "edge_113_141",
            "edge_141",
            "rclick"
        ]
    );
}

#[test]
fn the_tower_race_as_the_original() {
    let Some(files) = files() else {
        return;
    };
    let spec = Spec {
        world: 3,
        st: "tower",
        palace: true,
        tower: true,
    };
    let names = replay_all(files, include_str!("golden/puzzle_tower_race.txt"), spec);
    let names: Vec<&str> = names
        .iter()
        .map(|n| n.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(names, ["player_win", "demon_win"]);
}

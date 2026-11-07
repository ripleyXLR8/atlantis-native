//! Bagdad's boards (world 3) against the original's C code under
//! emulation, replayed through the port's board API
//! ([`common::board`]):
//!
//! - the merchant's house guard (`tests/golden/puzzle_guard.txt`): the
//!   winning sneak, the captures 880 / 881 / 882 / 888, the grumbling;
//! - the garden's banana bunch, monkey and soldiers
//!   (`tests/golden/puzzle_banana.txt`);
//! - the jewel case (`tests/golden/puzzle_ecrin.txt`): the gems' hit
//!   test, then per frame the sprite calls, the screen's md5, the state.
//!
//! Needs the game: `ATLANTIS3_DATA`.

mod common;

use atlantis3_data::A3Files;
use atlantis3_engine::Game;
use common::board::{assert_same, draws, md5_screen, replay_all, runs, Clock, Spec};

const GUARD: &str = include_str!("golden/puzzle_guard.txt");
const BANANA: &str = include_str!("golden/puzzle_banana.txt");
const ECRIN: &str = include_str!("golden/puzzle_ecrin.txt");

fn files() -> Option<impl Fn() -> A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(move || A3Files::open_dir(&dir).unwrap().0)
}

#[test]
fn the_guard_as_the_original() {
    let Some(files) = files() else {
        return;
    };
    let spec = Spec {
        world: 3,
        st: Some("guard"),
        clock: Clock::PLACE,
    };
    let names = replay_all(files, GUARD, spec);
    assert_eq!(
        names,
        [
            "sneak",
            "stay1",
            "stay2",
            "view2_881",
            "view3",
            "view4",
            "view5_g2",
            "view1_cat",
            "grumble",
            "after8"
        ]
    );
}

#[test]
fn the_banana_bunch_monkey_and_soldiers_as_the_original() {
    let Some(files) = files() else {
        return;
    };
    let spec = Spec {
        world: 3,
        st: Some("jardin"),
        clock: Clock::PLACE,
    };
    let names = replay_all(files, BANANA, spec);
    assert_eq!(names, ["banana", "monkey", "soldiers"]);
}

/// The jewel case's runs: started by the zone hook in view 114 at 200 s,
/// then one loop call per `frame` line at 200 + (n + 1) / 32 s.
#[test]
fn the_jewel_case_as_the_original() {
    let Some(files) = files() else {
        return;
    };
    let all = runs(ECRIN);
    // The hit lines: each gem's hits on the 8-pixel grid.
    {
        let mut g = Game::new(files()).unwrap();
        assert!(g.board_begin("jardin.wam", 3));
        assert_eq!((g.width(), g.height()), (800, 600));
        let mut hits: Vec<Vec<(i32, i32)>> = vec![Vec::new(); 3];
        for (k, h) in hits.iter_mut().enumerate() {
            for y in (0..600).step_by(8) {
                for x in (0..800).step_by(8) {
                    if g.board_ecrin_hit(k, (x, y)) {
                        h.push((x, y));
                    }
                }
            }
        }
        let mut checked = 0;
        for l in &all[0].lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            if w[1] == "overlap" {
                let n = (0..3)
                    .flat_map(|i| hits[i].iter().map(move |p| (i, p)))
                    .filter(|(i, p)| (0..3).any(|j| j > *i && hits[j].contains(p)))
                    .count();
                assert_eq!(n.to_string(), w[3], "{l}");
                checked += 1;
                continue;
            }
            let k: usize = w[1].parse().unwrap();
            let h = &hits[k];
            let (x0, x1) = (h.iter().map(|p| p.0).min(), h.iter().map(|p| p.0).max());
            let (y0, y1) = (h.iter().map(|p| p.1).min(), h.iter().map(|p| p.1).max());
            let got = format!(
                "hit {k} {} box {},{}-{},{} n {}",
                k + 1,
                x0.unwrap(),
                y0.unwrap(),
                x1.unwrap(),
                y1.unwrap(),
                h.len()
            );
            assert!(l.starts_with(&got), "{l}\n{got}");
            checked += 1;
        }
        assert_eq!(checked, 4);
    }
    let gems = ["BAG_SAPHIR", "BAG_RUBIS", "BAG_EMERAUDE"];
    for run in &all[1..] {
        let mut b = common::board::Board::new(
            files(),
            Spec {
                world: 3,
                st: Some("ecrin"),
                clock: Clock {
                    base: 200.0,
                    period: 1.0 / 32.0,
                },
            },
        );
        b.g.script_mut().unwrap().flags[829] = 1;
        assert!(b.g.board_begin("jardin.wam", 3));
        b.g.board_enter(114, 100.0);
        b.raw_trace();
        let ids: Vec<i32> = gems
            .iter()
            .map(|n| {
                b.g.names()
                    .id(atlantis2_data::defnames::OBJETS, n.as_bytes())
                    .unwrap() as i32
            })
            .collect();
        let mut out = Vec::new();
        let mut last_st = String::new();
        let mut last_bag = String::new();
        let mut state = |b: &mut common::board::Board, n: u32, out: &mut Vec<String>| {
            let st = b.g.board_st("ecrin").unwrap();
            if st != last_st {
                out.push(format!("st {n} {st}"));
                last_st = st;
            }
            let s = b.g.script().unwrap();
            let bag: Vec<String> = gems
                .iter()
                .zip(&ids)
                .map(|(nm, id)| format!("{nm}={}", u8::from(s.bag.iter().any(|e| e.id == *id))))
                .collect();
            let bag = bag.join(" ");
            if bag != last_bag {
                out.push(format!("bag {n} {bag}"));
                last_bag = bag;
            }
        };
        // The start: flag 144, the zone hook at 200 s.
        b.g.script_mut().unwrap().flags[144] = 1;
        assert!(b.g.board_zone_hook("BAG_ZONE_ECRIN", 200.0));
        let s = b.g.script().unwrap();
        let st = b.g.board_st("ecrin").unwrap();
        let cursor = st
            .split("cursor=")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap();
        out.push(format!(
            "start mode={} old={} active={} cursor={cursor} flag131={}",
            b.g.board_mode_number(),
            b.g.board_screen_back(),
            u8::from(b.g.puzzle_screen().is_some()),
            s.flags[131]
        ));
        for t in b.raw_trace() {
            out.push(format!("{} 0", t));
        }
        state(&mut b, 0, &mut out);
        for l in run.lines.iter().filter(|l| l.starts_with("frame ")) {
            let w: Vec<&str> = l.split(' ').collect();
            let n: u32 = w[1].parse().unwrap();
            let t = 200.0 + f64::from(n + 1) / 32.0;
            let hex: String = t.to_le_bytes().iter().map(|x| format!("{x:02x}")).collect();
            assert_eq!(hex, w[2], "{l}: the clock");
            let (x, y): (i32, i32) = (w[3].parse().unwrap(), w[4].parse().unwrap());
            let (left, right) = (w[5] == "1", w[6] == "1");
            let stay = b.g.board_screen_frame(t, (x, y), left, right);
            let trace = b.raw_trace();
            let (d, flips) = draws(&trace);
            out.push(format!(
                "frame {n} {hex} {x} {y} {} {} ret={} md5={} draws={}",
                w[5],
                w[6],
                u8::from(stay),
                flips.last().map_or("-", |s| s.as_str()),
                d.join(",")
            ));
            for t in trace.iter().filter(|t| {
                !(t.as_str() == "warp=flat"
                    || t.starts_with("aff ")
                    || t.starts_with("affc ")
                    || t.starts_with("flip "))
            }) {
                let (k, r) = t.split_once(' ').unwrap_or((t.as_str(), ""));
                out.push(if r.is_empty() {
                    format!("{k} {n}")
                } else {
                    format!("{k} {n} {r}")
                });
            }
            state(&mut b, n, &mut out);
        }
        let want: Vec<String> = run.lines.iter().map(|l| l.to_string()).collect();
        assert_same(&format!("run {}", run.name), &want, &out);
    }
    let _ = md5_screen;
}

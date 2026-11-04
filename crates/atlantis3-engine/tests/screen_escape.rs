//! Escape in the full-screen loop modes against the original under
//! emulation (`tests/golden/screen_escape.txt`): each run starts a screen
//! (the tower map, the unicorn, the game boy, the jewel case, the skull,
//! and the map, scarab case and remote where Escape does nothing), plays
//! its frames from the golden's inputs, then presses Escape
//! ([`Game::escape`]); compared: each frame's return and state, the stop
//! entered, the stop's sound test and stop, `CtrlScenarIN`, the commands
//! in their order, and the state after (loop mode, the screen's open byte,
//! flags, bag, music volume, the dice).
//!
//! Needs the game: `ATLANTIS3_DATA`.

mod common;

use atlantis2_data::defnames;
use atlantis3_data::A3Files;
use atlantis3_engine::Game;
use common::board::{assert_same, runs, Clock};

const GOLDEN: &str = include_str!("golden/screen_escape.txt");

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

/// The stop the original enters for a loop mode.
fn stop_name(mode: i32) -> &'static str {
    match mode {
        9 => "AtlCarte_Close",
        10 => "AtlCarteTour_Close",
        11 => "Licorne_Stop",
        12 => "AtlGBoy_Close",
        14 => "AtlEcrin_Stop",
        15 => "CraneBagdad_Stop",
        _ => "?",
    }
}

fn in_bag(g: &Game, name: &str) -> u8 {
    let id = g.names().id(defnames::OBJETS, name.as_bytes()).unwrap() as i32;
    u8::from(g.script().unwrap().bag.iter().any(|e| e.id == id))
}

fn flag(g: &Game, n: usize) -> i32 {
    g.script().unwrap().flags[n]
}

/// The run's set-up (as the harness's base) and its frames' clock.
fn setup(g: &mut Game, kind: &str, mode: i32) -> Clock {
    let tick30 = Clock {
        base: 300.0,
        period: 1.0 / 30.0,
    };
    match kind {
        "tour" | "crane" | "noop" => {
            assert!(g.board_begin("TourMagi.wam", 3));
            g.board_enter(39, 299.0);
            assert!(g.board_screen_start(mode, 300.0));
            tick30
        }
        "licorne" => {
            assert!(g.board_begin("jardin.wam", 3));
            assert!(g.board_screen_start(11, 100.0));
            g.script_mut().unwrap().flags[147] = 1;
            g.script_mut().unwrap().flags[148] = 1;
            Clock::PLACE
        }
        "gboy" => {
            assert!(g.board_begin("TourMagi.wam", 3));
            g.board_enter(39, 50.0);
            g.board_gboy_start(100.0);
            Clock::PLACE
        }
        "ecrin" => {
            g.script_mut().unwrap().flags[829] = 1;
            assert!(g.board_begin("jardin.wam", 3));
            g.board_enter(114, 100.0);
            g.script_mut().unwrap().flags[144] = 1;
            assert!(g.board_zone_hook("BAG_ZONE_ECRIN", 200.0));
            // Frame n at 200 + (n + 1) / 32 s.
            Clock {
                base: 200.0 + 1.0 / 32.0,
                period: 1.0 / 32.0,
            }
        }
        _ => panic!("{kind}"),
    }
}

/// A frame line's state, by screen.
fn frame_state(g: &Game, kind: &str, mode: i32) -> String {
    match kind {
        "tour" | "crane" => format!("open={}", u8::from(g.board_mode_number() == mode)),
        "licorne" => g.board_st("licorne").unwrap(),
        "gboy" => {
            let st = g.board_st("gboy").unwrap();
            format!(
                "cur={} anim={} res={}",
                field(&st, "cur="),
                field(&st, "anim="),
                field(&st, "res=")
            )
        }
        "ecrin" => {
            let st = g.board_st("ecrin").unwrap();
            format!(
                "count={} taken={}",
                field(&st, "count="),
                field(&st, "taken=")
            )
        }
        _ => String::new(),
    }
}

/// The after line's fields, by screen.
fn after_state(g: &Game, kind: &str, mode: i32) -> String {
    let now = g.board_mode_number();
    let open = u8::from(now == mode);
    match kind {
        "tour" | "crane" => format!("mode={now} open={open}"),
        "noop" => format!("mode={now}"),
        "licorne" => format!(
            "mode={now} act={open} flag146={} flag147={} flag148={} flag149={} \
             BAG_BOULE_LAITEUSE={}",
            flag(g, 146),
            flag(g, 147),
            flag(g, 148),
            flag(g, 149),
            in_bag(g, "BAG_BOULE_LAITEUSE")
        ),
        "gboy" => {
            let tower = g.board_st("tower").unwrap();
            let st = g.board_st("gboy").unwrap();
            format!(
                "mode={now} open={open} vol={} res={}",
                field(&tower, "vol="),
                field(&st, "res=")
            )
        }
        "ecrin" => {
            let st = g.board_st("ecrin").unwrap();
            let gems: Vec<String> = ["BAG_SAPHIR", "BAG_RUBIS", "BAG_EMERAUDE"]
                .iter()
                .map(|n| format!("{n}={}", in_bag(g, n)))
                .collect();
            format!(
                "mode={now} open={open} count={} taken={} flag131={} {}",
                field(&st, "count="),
                field(&st, "taken="),
                flag(g, 131),
                gems.join(" ")
            )
        }
        _ => panic!("{kind}"),
    }
}

#[test]
fn escape_in_the_screens_as_the_original() {
    let Some(files) = files() else {
        return;
    };
    let mut names = Vec::new();
    for run in runs(GOLDEN).into_iter().skip(1) {
        let mut words = run.name.split_whitespace();
        let name = words.next().unwrap();
        let mode: i32 = words
            .next()
            .and_then(|m| m.strip_prefix("mode="))
            .unwrap()
            .parse()
            .unwrap();
        let kind = name.split('_').next().unwrap();
        let mut g = Game::new(files()).unwrap();
        g.flat_warp = true;
        g.stub_cinemas = true;
        g.story_sound_model = true;
        assert_eq!((g.width(), g.height()), (800, 600));
        let clock = setup(&mut g, kind, mode);
        g.trace = Some(Vec::new());
        let mut out = Vec::new();
        let mut input = ((400, 300), false, false);
        for l in &run.lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w[0] {
                "in" => {
                    out.push(l.to_string());
                    let p: Vec<i32> = w[2..].iter().map(|v| v.parse().unwrap()).collect();
                    input = ((p[0], p[1]), p[2] == 1, p[3] == 1);
                }
                "frame" => {
                    let f: u32 = w[1].parse().unwrap();
                    let stay = g.board_screen_frame(clock.at(f), input.0, input.1, input.2);
                    g.trace.as_mut().unwrap().clear();
                    out.push(format!(
                        "frame {f} ret={} {}",
                        u8::from(stay),
                        frame_state(&g, kind, mode)
                    ));
                }
                "key" => {
                    let f = w[1];
                    out.push(l.to_string());
                    out.push(format!("esc {f} mode={}", g.board_mode_number()));
                    g.escape();
                    for t in std::mem::take(g.trace.as_mut().unwrap()) {
                        let (k, r) = t.split_once(' ').unwrap_or((t.as_str(), ""));
                        match k {
                            "stop" => out.push(format!("stop {f} {}", stop_name(mode))),
                            "scan" => out.push(format!("scan {f}")),
                            "cmd" => out.push(format!("cmd {f} {r}")),
                            "snd" if r.starts_with("test ") || r.starts_with("stop ") => {
                                out.push(format!("snd {f} {r}"))
                            }
                            _ => {}
                        }
                    }
                    out.push(format!("after {f} {}", after_state(&g, kind, mode)));
                }
                _ => {}
            }
        }
        let want: Vec<String> = run.lines.iter().map(|l| l.to_string()).collect();
        assert_same(&format!("run {}", run.name), &want, &out);
        names.push(name.to_string());
    }
    assert_eq!(
        names,
        [
            "tour",
            "licorne_held",
            "licorne_running",
            "gboy_before",
            "gboy_counting",
            "ecrin_one",
            "ecrin_fade",
            "crane",
            "noop_9",
            "noop_13",
            "noop_18"
        ]
    );
}

//! The late Hoggar's remote control (`TELECOMMANDE`, loop mode 18)
//! against the original's C code under emulation
//! (`tests/golden/puzzle_telecommande.txt`, which also holds the skull's
//! runs, `CRANEBAGDAD`, loop mode 15). First each button's hit test on an
//! 8-pixel grid, then per frame the sprite calls, the screen's md5, the
//! commands and the state (the hover bytes and frames, flags 400..404,
//! 425, 427, the loop mode), all with the pointer at real positions: the
//! buttons are found and pressed as a player's mouse does.
//!
//! The harness logs no sound, show or hide call (the commands' own
//! lines stand for them). Its `RunOneCommand` returns at once for the chapter and
//! world commands (`(Chapter=607)`, the skull's travel): their effects
//! are not part of the screens, so the port's are not compared (the
//! lines after such a command).
//!
//! Needs the game: `ATLANTIS3_DATA`.

mod common;

use atlantis3_data::A3Files;
use atlantis3_engine::Game;
use common::board::{assert_same, draws, runs, Clock, Spec};

const GOLDEN: &str = include_str!("golden/puzzle_telecommande.txt");

fn files() -> Option<impl Fn() -> A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(move || A3Files::open_dir(&dir).unwrap().0)
}

/// The buttons' sprites in the loop's order.
const BUTTONS: [usize; 4] = [0x00, 0x0e, 0x1d, 0x2c];

#[test]
fn the_remote_buttons_hit_as_the_original() {
    let Some(files) = files() else {
        return;
    };
    let all = runs(GOLDEN);
    let mut g = Game::new(files()).unwrap();
    assert!(g.board_begin("hoggar.wam", 1));
    assert_eq!((g.width(), g.height()), (800, 600));
    let mut checked = 0;
    for l in &all[0].lines {
        let w: Vec<&str> = l.split_whitespace().collect();
        let sprite: usize = w[1].parse().unwrap();
        let k = BUTTONS.iter().position(|&s| s == sprite).unwrap();
        let mut hits = Vec::new();
        for y in (0..600).step_by(8) {
            for x in (0..800).step_by(8) {
                if g.board_tele_hit(k, (x, y)) {
                    hits.push((x, y));
                }
            }
        }
        let (x0, x1) = (
            hits.iter().map(|p| p.0).min(),
            hits.iter().map(|p| p.0).max(),
        );
        let (y0, y1) = (
            hits.iter().map(|p| p.1).min(),
            hits.iter().map(|p| p.1).max(),
        );
        let got = format!(
            "hit {sprite} box {},{}-{},{} n {}",
            x0.unwrap(),
            y0.unwrap(),
            x1.unwrap(),
            y1.unwrap(),
            hits.len()
        );
        assert!(l.starts_with(&got), "{l}\n{got}");
        checked += 1;
    }
    assert_eq!(checked, 4);
}

/// The remote's runs: Bagdad hovered and pressed (the first click also
/// brings the Atlantean figure back), the power button, Paleo, home in
/// the Hoggar (nothing); Bagdad and Paleo after their ends; home from
/// another world (`(Chapter=607)`).
#[test]
fn the_remote_as_the_original() {
    replay(&["hoggar", "ends", "home"]);
}

/// Each run: the place loaded and its view entered, the flags the run
/// names, the screen started at 200 s, then one loop call per `frame`
/// line at 200 + (n + 1) / 32 s with the pointer and the click bytes.
fn replay(which: &[&str]) {
    let Some(files) = files() else {
        return;
    };
    let all = runs(GOLDEN);
    let mut names = Vec::new();
    for run in all[1..].iter().filter(|r| which.contains(&r.name)) {
        names.push(run.name);
        let (wam, world, view, flags, mode): (&str, i32, u32, &[(usize, i32)], i32) = match run.name
        {
            "hoggar" => ("hoggar.wam", 1, 24, &[], 18),
            "ends" => ("hoggar.wam", 1, 24, &[(908, 1), (605, 1), (427, 1)], 18),
            "home" => ("jardin.wam", 3, 114, &[(427, 1), (829, 1)], 18),
            "crane" => ("hoggar.wam", 1, 24, &[], 15),
            "crane_travel" => ("hoggar.wam", 1, 24, &[(891, 1)], 15),
            n => panic!("run {n}"),
        };
        let mut b = common::board::Board::new(
            files(),
            Spec {
                world,
                st: Some("tele"),
                clock: Clock {
                    base: 200.0,
                    period: 1.0 / 32.0,
                },
            },
        );
        for &(f, v) in flags {
            b.g.script_mut().unwrap().flags[f] = v;
        }
        assert!(b.g.board_begin(wam, world));
        b.g.board_enter(view, 100.0);
        b.raw_trace();
        let mut out = Vec::new();
        let mut last_st = String::new();
        let mut state = |b: &mut common::board::Board, n: u32, out: &mut Vec<String>| {
            let st = b.g.board_st("tele").unwrap();
            if st != last_st {
                out.push(format!("st {n} {st}"));
                last_st = st;
            }
        };
        assert!(b.g.board_screen_start(mode, 200.0));
        out.push(format!(
            "start mode={} active={}",
            b.g.board_mode_number(),
            u8::from(b.g.puzzle_screen().is_some())
        ));
        b.raw_trace();
        state(&mut b, 0, &mut out);
        // Where the golden's harness stopped running the commands.
        let mut cut = None;
        for l in run.lines.iter().filter(|l| l.starts_with("frame ")) {
            let w: Vec<&str> = l.split(' ').collect();
            let n: u32 = w[1].parse().unwrap();
            let t = 200.0 + f64::from(n + 1) / 32.0;
            let hex: String = t.to_le_bytes().iter().map(|x| format!("{x:02x}")).collect();
            assert_eq!(hex, w[2], "{l}: the clock");
            let (x, y): (i32, i32) = (w[3].parse().unwrap(), w[4].parse().unwrap());
            let (left, right) = (w[5] == "1", w[6] == "1");
            b.g.board_screen_frame(t, (x, y), left, right);
            let trace = b.raw_trace();
            let (d, flips) = draws(&trace);
            out.push(format!(
                "frame {n} {hex} {x} {y} {} {} ret={} md5={} draws={}",
                w[5],
                w[6],
                u8::from(b.g.puzzle_screen().is_some()),
                flips.first().map_or("-", |s| s.as_str()),
                d.join(",")
            ));
            for t in trace.iter().filter(|t| {
                !(t.as_str() == "warp=flat"
                    || t.starts_with("aff ")
                    || t.starts_with("affc ")
                    || t.starts_with("flip ")
                    || t.starts_with("snd ")
                    || t.starts_with("show ")
                    || t.starts_with("hide "))
            }) {
                let (k, r) = t.split_once(' ').unwrap_or((t.as_str(), ""));
                out.push(if r.is_empty() {
                    format!("{k} {n}")
                } else {
                    format!("{k} {n} {r}")
                });
                let lower = r.to_ascii_lowercase();
                if cut.is_none()
                    && k == "cmd"
                    && (lower.starts_with("(chapter=") || lower.starts_with("(changecd="))
                {
                    cut = Some(out.len());
                }
            }
            state(&mut b, n, &mut out);
        }
        let want: Vec<String> = run.lines.iter().map(|l| l.to_string()).collect();
        let (want, got) = match cut {
            // The golden's lines up to the same command.
            Some(c) => {
                let last = out[c - 1].clone();
                let i = want.iter().position(|l| *l == last).expect("the command");
                (want[..=i].to_vec(), out[..c].to_vec())
            }
            None => (want, out),
        };
        assert_same(&format!("run {}", run.name), &want, &got);
    }
    assert_eq!(names, which);
}

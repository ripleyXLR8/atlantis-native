//! The open bag's frames against the original's 0x00427c40 under emulation
//! (`tests/golden/inventory_pending.txt`, atlantis3-re
//! `tools/emu/a3_inventory.py pending`): each case fills a fresh bag in
//! world 1, opens it, then replays the golden's presses and frames
//! (pointer, clock) and compares the state after each frame.
//!
//! - A click above the bar stays pending (0x00429b80 tests and clears the
//!   click byte only with the pointer in the bar's row, @ 0x00429cfd): the
//!   pointer later entering the row over an object takes it.
//! - The place hook 0x004c27d0 runs in every open-bag frame (@ 0x00427ce6,
//!   after the `jle` that skips only the close and the hold).
//! - The edge scrolling's timer and the hover label after the step.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

const GOLDEN: &str = include_str!("golden/inventory_pending.txt");

/// The fields of a golden `frame` line the test compares besides open,
/// mode and hand.
#[derive(Clone, Copy)]
struct Fields {
    hooks: bool,
    scroll: bool,
}

fn frame_line(g: &Game, f: &str, x: i32, y: i32, which: Fields) -> String {
    let s = g.script().unwrap();
    let mode = match g.mode() {
        Mode::Inventory => 7,
        Mode::Warp => 3,
        m => panic!("{m:?}"),
    };
    let mut l = format!(
        "frame {f} {x} {y} open={} mode={mode} hand={}",
        u8::from(g.inventory_open()),
        s.vars[s.keys.objetenmain]
    );
    if which.hooks {
        l += &format!(" hooks={}", g.test_hook_calls());
    }
    if which.scroll {
        let label = g.test_inventory_label().map_or("-".to_string(), |t| {
            t.iter().map(|b| format!("{b:02x}")).collect()
        });
        l += &format!(" scroll={} label={label}", g.test_inventory_scroll());
    }
    l
}

/// A golden `frame` line with the fields the test compares.
fn golden_line(l: &str, which: Fields) -> String {
    let w: Vec<&str> = l.split_whitespace().collect();
    let mut out: Vec<&str> = w[..4].to_vec();
    for x in &w[4..] {
        let key = x.split('=').next().unwrap();
        let keep = match key {
            "open" | "mode" | "hand" => true,
            "hooks" => which.hooks,
            "scroll" | "label" => which.scroll,
            _ => false,
        };
        if keep {
            out.push(x);
        }
    }
    out.join(" ")
}

/// Replays the golden's cases named in `cases`.
fn replay(cases: &[&str], which: Fields) {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    assert_eq!((g.width(), g.height()), (800, 600));
    g.run_command("(monde=1)");
    g.set_world(1);
    assert!(g.load_place("hoggar.wam"));
    g.reset_camera();
    g.enter_at(5, 0.0, 0.0);
    let mut t = 100.0;
    let mut dt = 1.0 / 64.0;
    let (mut active, mut to_open) = (false, false);
    let (mut want, mut got) = (Vec::new(), Vec::new());
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let w: Vec<&str> = l.split_whitespace().collect();
        if w[0] == "case" {
            active = cases.contains(&w[1]);
            if active {
                // The harness: the close, alpha 0, the bag emptied and
                // scroll 0, then (after the bag lines) the open.
                if g.inventory_open() {
                    g.right_click();
                }
                g.test_inventory_reset();
                g.script_mut().unwrap().bag.clear();
                if w[1] == "scroll" {
                    t += 10.0;
                    dt = 1.0 / 16.0;
                }
                to_open = true;
            }
            continue;
        }
        if !active {
            continue;
        }
        if w[0] == "bag" {
            g.run_command(&format!("(SacADos={})", w[4]));
            continue;
        }
        if std::mem::take(&mut to_open) {
            assert_eq!(g.mode(), Mode::Warp);
            g.right_click();
            assert!(g.inventory_open());
        }
        match w[0] {
            "press" => {
                g.set_pointer(w[1].parse().unwrap(), w[2].parse().unwrap());
                g.click();
            }
            "frame" => {
                let (x, y) = (w[2].parse().unwrap(), w[3].parse().unwrap());
                g.set_pointer(x, y);
                t += dt;
                g.test_hook_calls_reset();
                g.step(t);
                got.push(frame_line(&g, w[1], x, y, which));
                want.push(golden_line(l, which));
            }
            _ => panic!("{l}"),
        }
    }
    assert!(!want.is_empty());
    assert_eq!(got, want);
}

#[test]
fn a_click_above_the_bar_stays_pending() {
    replay(
        &["take", "empty", "band"],
        Fields {
            hooks: false,
            scroll: false,
        },
    );
}

/// The place hook 0x004c27d0 runs once in every open-bag frame
/// (@ 0x00427ce6), with or without a pick.
#[test]
fn the_place_hook_runs_in_every_bag_frame() {
    replay(
        &["take", "empty", "band"],
        Fields {
            hooks: true,
            scroll: false,
        },
    );
}

/// Edge scrolling (@ 0x00429bd2..0x00429ce5): at x = 0 or W − 1 the 0.25-s
/// timer restarts whenever it expires, even with nothing to scroll, and
/// the hover label of the same frame names the object under the pointer
/// after the step; and the labels of the take cases.
#[test]
fn the_edge_scrolling_and_its_label() {
    replay(
        &["take", "empty", "band", "scroll"],
        Fields {
            hooks: true,
            scroll: true,
        },
    );
}

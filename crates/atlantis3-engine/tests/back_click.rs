//! A click on the in-game menu's `BT_BACK` made over a move zone, against
//! the original under emulation (`tests/golden/back_click.txt`): the
//! Hoggar's view 4 at yaw 108 (the centre on the type-1 zone to view 5),
//! Escape, the pointer on `BT_BACK`, the button down, a menu frame, the
//! button up, a menu frame (`BT_BACK` acts on the release), then four warp
//! frames. The original has one click byte (0x007592b8) for the menus and
//! the warp: the down callback sets it in every mode (@ 0x004315a5), page
//! 1 clears it at the end of every frame (0x00451560 @ 0x004544af), and its
//! `BT_BACK` clears it twice more (@ 0x0045474e and in the back to the
//! game @ 0x004512ba). So the click does not go on into the panorama: no
//! zone click, no move, on any frame. The golden's control case: the same
//! press made in the warp starts the move to view 5.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

const GOLDEN: &str = include_str!("golden/back_click.txt");

/// The golden's line `CASE STEP ...` with `key=` (the second line of a
/// step when `key` is one of its fields).
fn field<'a>(case: &str, step: &str, key: &str) -> &'a str {
    let head = format!("{case} {step} ");
    GOLDEN
        .lines()
        .filter_map(|l| l.strip_prefix(head.as_str()))
        .find_map(|l| l.split(' ').find_map(|w| w.strip_prefix(key)))
        .unwrap_or_else(|| panic!("{case} {step}: no {key}"))
}

/// Hoggar view 4 at yaw 108, pitch 0, after one warp frame.
fn hoggar_view_4() -> Option<(Game, f64)> {
    let dir = atlantis3_data::testing::install_dir()?;
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    assert!(g.board_begin("hoggar.wam", 1));
    g.board_wam_setup();
    let t = 100.0;
    g.board_enter_at(4, 108f32.to_radians(), 0.0, t);
    let t = t + 0.04;
    g.step(t);
    assert_eq!(g.mode(), Mode::Warp);
    Some((g, t))
}

/// The zone under the centre as the golden writes it (`TYPE/VALUE`).
fn zone(g: &Game) -> String {
    let view = g.place().1.unwrap();
    g.centre_zone()
        .map(|z| {
            let z = &g.wam().view(view).unwrap().zones[z];
            format!("{}/{}", z.kind, z.value)
        })
        .unwrap_or_else(|| "-".into())
}

/// The golden's next view (0: none) and whether a turn runs.
fn moving(case: &str, step: &str) -> (bool, bool) {
    (
        field(case, step, "next=") != "0",
        field(case, step, "recal=") != "0",
    )
}

#[test]
fn a_press_in_the_warp_starts_the_move() {
    let Some((mut g, t)) = hoggar_view_4() else {
        return;
    };
    assert_eq!(zone(&g), field("control", "warp0", "zone="));
    assert_eq!(field("control", "warp0", "zone="), "1/5");
    g.click();
    g.step(t + 0.04);
    let (next, recal) = moving("control", "warp1");
    assert!(next && recal, "the golden's control moves");
    assert!(g.turning(), "the press in the warp starts the move");
}

fn back(one_poll: bool) {
    let case = if one_poll { "back_one" } else { "back" };
    let Some((mut g, mut t)) = hoggar_view_4() else {
        return;
    };
    let mut tick = || {
        t += 0.04;
        t
    };
    g.escape();
    assert_eq!(g.menu_page().map(|p| p.0), Some(1));
    let p = GOLDEN
        .lines()
        .find_map(|l| l.strip_prefix("pointer BT_BACK "))
        .unwrap();
    let (x, y) = p.split_once(' ').unwrap();
    g.set_pointer(x.parse().unwrap(), y.parse().unwrap());
    for _ in 0..2 {
        g.step(tick());
    }
    if one_poll {
        g.click();
        g.release();
    } else {
        g.click();
        g.step(tick());
        assert_eq!(g.mode(), Mode::Menu, "BT_BACK waits for the release");
        assert_eq!(field(case, "menu_held", "mode="), "6");
        g.release();
    }
    g.step(tick());
    assert_eq!(field(case, "menu_release", "mode="), "3");
    assert_eq!(field(case, "menu_release", "b8="), "0");
    assert_eq!(g.mode(), Mode::Warp, "{case}: BT_BACK on the release");
    for k in 1..=4 {
        let step = format!("warp{k}");
        g.step(tick());
        assert_eq!(zone(&g), field(case, &step, "zone="), "{case} {step}");
        assert_eq!(moving(case, &step), (false, false), "{case} {step}");
        assert!(!g.turning(), "{case} {step}: no move");
        assert_eq!(g.place().1, Some(4), "{case} {step}");
        assert_eq!(g.mode(), Mode::Warp, "{case} {step}");
    }
}

#[test]
fn back_clicked_over_a_move_zone_does_not_move() {
    back(false);
}

#[test]
fn back_clicked_in_one_poll_does_not_move() {
    back(true);
}

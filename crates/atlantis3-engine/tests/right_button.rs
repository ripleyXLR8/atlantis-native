//! The right button against the original's mouse callback
//! (@ 0x00431580, button 2, run under emulation):
//!
//! ```text
//! right press in mode 3  mode=7 inv_open=1 rbyte=0 turning=1
//! right press in mode 7  mode=3 inv_open=0 rbyte=0 turning=1
//! ```
//!
//! It sets the right button's byte (0x007592b9), then by loop mode: in the
//! warp (3) the inventory opens unless its open flag is set, with no test
//! of a move's turn or pending move (@ 0x00431648..0x00431671); in the
//! inventory (7) it closes (@ 0x00427b20, which clears the byte itself,
//! @ 0x00427b5d); in the subject menu (8) the menu closes (@ 0x00427be0);
//! after each of the three the byte is cleared (@ 0x00431676). So the
//! next warp frame never sees it: in `paleo.wam` view 4 the hunt's frame
//! (@ 0x004c9350) does not end the hunt after the bag was closed with the
//! right button.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

fn game() -> Option<Game> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap())
}

fn has_spear(g: &Game) -> bool {
    let id = g
        .names()
        .id(atlantis2_data::defnames::OBJETS, b"PAL_LANCE")
        .unwrap() as i32;
    g.script().unwrap().bag.iter().any(|e| e.id == id)
}

#[test]
fn closing_the_bag_with_the_right_button_keeps_the_hunt() {
    let Some(mut g) = game() else {
        return;
    };
    assert!(g.board_begin("paleo.wam", 4));
    g.board_wam_setup();
    g.board_enter(4, 100.0);
    g.run_command("(SacADos=PAL_LANCE)");
    assert!(has_spear(&g));
    let mut t = 100.0;
    g.step(t);
    assert_eq!(g.script().unwrap().flags[91], 0);
    g.right_click();
    assert!(g.inventory_open());
    t += 0.04;
    g.step(t);
    g.right_click();
    assert!(!g.inventory_open());
    assert_eq!(g.mode(), Mode::Warp);
    // The next warp frames: the hunt's tail does not run.
    for _ in 0..3 {
        t += 0.04;
        g.step(t);
    }
    assert_eq!(g.script().unwrap().flags[91], 0, "the hunt ended");
    assert!(has_spear(&g), "the spear taken out of the bag");
}

#[test]
fn the_right_button_opens_the_bag_during_a_turn() {
    let Some(mut g) = game() else {
        return;
    };
    g.set_world(1);
    assert!(g.load_place("hoggar.wam"));
    // `tests/golden/moves_hoggar.txt` case 1 z0: a move to view 2 with a
    // turn first.
    let bits = |s: &str| f32::from_bits(u32::from_str_radix(s, 16).unwrap());
    g.reset_camera();
    g.enter_at(1, bits("c0339fa2"), bits("3c3f89d6"));
    let (target, turning, pending) = g.click_centre_now();
    assert_eq!((target, turning, pending), (Some(2), true, false));
    assert!(g.turning());
    g.right_click();
    assert!(g.inventory_open(), "the bag opens during the turn");
    assert!(g.turning(), "the turn waits");
    g.right_click();
    assert!(!g.inventory_open());
    assert!(g.turning());
    // The turn goes on to its end.
    let mut n = 0;
    while g.turn_tick() {
        n += 1;
        assert!(n < 100);
    }
}

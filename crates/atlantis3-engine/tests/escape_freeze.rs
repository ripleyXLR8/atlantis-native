//! Escape while a puzzle freezes the input. The original's keyboard
//! callback (@ 0x0042ab20) starts with `mov al, [0x00789d56]; test al,
//! al; jne ret`: while the freeze byte is set no key is handled, so the
//! Escape handler (@ 0x0042ac50) does not run and the aim is not stored
//! (@ 0x0042ac35). Under emulation (the callback run with Escape in the
//! scan-code buffer): freeze = 1 returns at once, freeze = 0 reaches the
//! aim read before the handler. So no menu opens during the spinning
//! top, a corridor door, the merchant's stone, the tiger's stone, the
//! fisherman's scale or the tower's demon turn, and no load or new game
//! can follow from it (neither clears the byte in the original).
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::Fpu;
use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

#[test]
fn escape_does_nothing_while_the_input_is_frozen() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    assert!(g.board_begin("hoggar.wam", 1));
    g.board_wam_setup();
    g.board_enter(1, 100.0);
    let fpu = Fpu::Chop;
    g.board_camera(deg_to_rad_a3(fpu, 150), deg_to_rad_a3(fpu, -10));
    // The freeze set, as a throw of the spinning top sets it (@ 0x004d44b9).
    assert!(g.test_poke(0x00789d56, 1));
    assert!(g.keys_frozen());
    let before = g.escape_angles();
    g.escape();
    assert_eq!(g.mode(), Mode::Warp, "no menu while frozen");
    assert_eq!(g.escape_angles(), before, "no aim stored while frozen");
    // The freeze cleared (the throw's end, @ 0x004d3b2f): Escape opens
    // the in-game menu and stores the aim.
    assert!(g.test_poke(0x00789d56, 0));
    assert!(!g.keys_frozen());
    g.escape();
    assert_eq!(g.mode(), Mode::Menu);
    assert_ne!(g.escape_angles(), before);
}

//! The warp's navigation against the static reading of the original
//! (atlantis3-gog-fr-3473408; atlantis3-cd-fr-3379200 has the same
//! functions, `evidence/compare/cdfr_gog_pairs.tsv` of atlantis3-re): the
//! fixes of the 2026-11-17 fidelity review, area a3-navigation
//! (`audit/native-a3-review-2026-11-17/a3-navigation.md`).
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::{cross_colour, Game, Mode};

/// Hoggar view 4 at `yaw` degrees, pitch 0, after one warp frame.
fn hoggar_view_4(yaw: f32) -> Option<(Game, f64)> {
    let dir = atlantis3_data::testing::install_dir()?;
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    assert!(g.board_begin("hoggar.wam", 1));
    g.board_wam_setup();
    let t = 100.0;
    g.board_enter_at(4, yaw.to_radians(), 0.0, t);
    let t = t + 0.04;
    g.step(t);
    assert_eq!(g.mode(), Mode::Warp);
    Some((g, t))
}

/// An ARGB colour as an RGB565 surface stores it (truncated).
fn rgb565(argb: u32) -> u16 {
    let (r, g, b) = ((argb >> 16) & 0xff, (argb >> 8) & 0xff, argb & 0xff);
    (((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)) as u16
}

/// The nine pixels of the cross (@ 0x00440a6a..0x00440b6c).
fn cross_pixels(g: &Game) -> Vec<u16> {
    let (w, h) = (g.width(), g.height());
    let (cx, cy) = (w / 2, h / 2);
    let s = g.screen();
    let mut out = Vec::new();
    for k in 0..5 {
        out.push(s[(cy + k - 2) * w + cx]);
        out.push(s[cy * w + cx + k - 2]);
    }
    out
}

/// F6: 0x00440a30 draws the object in the hand (0x00428500 @ 0x00440a44)
/// first, then the cross (0x004e9a00 @ 0x00440b1c / 0x00440b7b), then the
/// zone's cursor: the cross shows over the object.
#[test]
fn the_cross_is_drawn_over_the_object_in_the_hand() {
    // A yaw with no zone and no sprite under the centre: no zone cursor
    // over the cross.
    let mut found = None;
    for yaw in (0..360).step_by(15) {
        let Some((g, t)) = hoggar_view_4(yaw as f32) else {
            return;
        };
        if g.centre_zone().is_none() {
            found = Some((g, t));
            break;
        }
    }
    let (mut g, t) = found.expect("a yaw with no zone under the centre");
    let empty = g.screen().to_vec();
    let s = g.script_mut().unwrap();
    let k = s.keys.objetenmain;
    s.vars[k] = 1;
    s.cursor_objects = true;
    s.cursor_sprite = 0;
    g.step(t + 0.04);
    let held = g.screen().to_vec();
    // The object is drawn around the centre.
    let (w, h) = (g.width(), g.height());
    let changed = (h / 2 - 8..h / 2 + 8)
        .flat_map(|y| (w / 2 - 8..w / 2 + 8).map(move |x| y * w + x))
        .filter(|&i| empty[i] != held[i])
        .count();
    assert!(changed > 20, "the object in the hand is drawn ({changed})");
    let want = rgb565(cross_colour(None, false));
    assert!(
        cross_pixels(&g).iter().all(|&p| p == want),
        "the cross over the object: {:04x?}",
        cross_pixels(&g)
    );
}

/// Runs warp frames of 0.04 s from `t` until view 5 is entered and the
/// warp is back (at most 1000 frames); whether a travel video played.
fn walk_to_view_5(g: &mut Game, mut t: f64) -> bool {
    let mut travel = false;
    for _ in 0..1000 {
        t += 0.04;
        g.step(t);
        travel |= g.video_is_travel();
        if g.place().1 == Some(5) && g.mode() == Mode::Warp {
            return travel;
        }
    }
    panic!("view 5 not reached");
}

/// F4: the double-click byte 0x007592bb is written only by a press
/// (@ 0x004315b1 / 0x004315ec), the click that ends a travel video
/// (@ 0x00422073) and `CINEMA` (@ 0x00448322). The second press of a
/// double click on the move zone to view 5 comes during the turn; the byte
/// is still set when the turn ends, and the completion starts no travel
/// video (@ 0x00440242).
#[test]
fn a_double_click_on_a_move_skips_the_travel_video() {
    let Some((mut g, t)) = hoggar_view_4(108.0) else {
        return;
    };
    // The control: one press, the travel video plays.
    g.click();
    assert!(walk_to_view_5(&mut g, t), "a single click plays the video");

    let (mut g, t) = hoggar_view_4(108.0).unwrap();
    g.click();
    let t = t + 0.04;
    g.step(t);
    assert!(g.turning());
    g.click();
    assert!(!walk_to_view_5(&mut g, t), "a double click skips it");

    // Two presses 0.2 s or more apart are no double click.
    let (mut g, t) = hoggar_view_4(108.0).unwrap();
    g.click();
    let mut t = t;
    for _ in 0..6 {
        t += 0.04;
        g.step(t);
    }
    assert!(g.turning());
    g.click();
    assert!(walk_to_view_5(&mut g, t), "presses 0.24 s apart");
}

/// F7: the mouse callback moves and clamps the free pointer
/// (0x007592b0 / 0x007592b4) in every mode except the warp while a move
/// turns (@ 0x004313b7..0x00431444); the warp's Escape does not centre it
/// and the menu draws its pointer from it.
#[test]
fn the_free_pointer_follows_the_mouse_in_the_warp() {
    let Some((mut g, t)) = hoggar_view_4(108.0) else {
        return;
    };
    g.set_pointer(400, 300);
    g.mouse_motion(100, -50);
    assert_eq!(g.pointer(), (500, 250));
    g.mouse_motion(1000, 1000);
    assert_eq!(g.pointer(), (799, 599));
    g.mouse_motion(-30, -40);
    assert_eq!(g.pointer(), (769, 559));
    g.step(t + 0.04);
    g.escape();
    assert_eq!(g.mode(), Mode::Menu);
    assert_eq!(
        g.pointer(),
        (769, 559),
        "the menu's pointer where the warp left it"
    );

    // While a move turns the pointer stays.
    let (mut g, t) = hoggar_view_4(108.0).unwrap();
    g.set_pointer(400, 300);
    g.click();
    g.step(t + 0.04);
    assert!(g.turning());
    g.mouse_motion(100, 100);
    assert_eq!(g.pointer(), (400, 300));
}

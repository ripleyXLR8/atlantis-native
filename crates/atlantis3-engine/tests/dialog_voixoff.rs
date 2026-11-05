//! A voice-over dialog (`DIAL=n, voixoff`) on the screen against the
//! original under emulation (`tests/golden/dialog_voixoff.txt`): the
//! panorama is drawn into the dialog's body surface (the screen's size) at
//! each 15 Hz tick, and `CDialogue::Render` copies that surface to the
//! screen at every pass before the flip prints the texts. So what a pass
//! left on the screen (the golden's marker, standing for the texts) is gone
//! after the next pass, and the passes between two ticks show the same
//! picture. Dialog 1 of the Hoggar, the harness's voice model (3
//! `IsPlaying` calls), passes as the golden's.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/dialog_voixoff.txt");
const MARK: (usize, usize, usize, usize) = (10, 10, 40, 8);

fn mark(g: &mut Game) {
    let w = g.width();
    let (x0, y0, mw, mh) = MARK;
    let s = g.test_screen_mut();
    for y in y0..y0 + mh {
        s[y * w + x0..y * w + x0 + mw].fill(0xffff);
    }
}

fn marker_left(g: &Game) -> bool {
    let w = g.width();
    let (x0, y0, mw, mh) = MARK;
    (y0..y0 + mh).all(|y| {
        g.screen()[y * w + x0..y * w + x0 + mw]
            .iter()
            .all(|&p| p == 0xffff)
    })
}

fn md5_screen(g: &Game) -> String {
    let mut h = Md5::new();
    for p in g.screen() {
        h.update(p.to_le_bytes());
    }
    format!("{:x}", h.finalize())
}

#[test]
fn a_voice_over_dialog_redraws_the_screen_at_every_pass_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut g = Game::new(files).unwrap();
    assert!(g.new_game());
    let mut t = 0.0;
    while g.mode() != Mode::Warp || g.video().is_some() {
        t += 1.0 / 30.0;
        g.step(t);
        if g.video().is_some() {
            g.finish_video();
        }
    }
    g.reload_dialogs(1);
    g.voice_model = Some(3);
    g.dialog_heads = false;
    // Passes: (header, the golden's flip: screen md5 and marker_left).
    let mut passes: Vec<(String, Option<(String, bool)>)> = Vec::new();
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        if let Some(e) = l.strip_prefix("  EV ") {
            if let Some(f) = e.strip_prefix("Flip(2) ") {
                let get = |k: &str| {
                    f.split(' ')
                        .find_map(|x| x.strip_prefix(k))
                        .unwrap()
                        .to_string()
                };
                passes.last_mut().unwrap().1 = Some((get("screen="), get("marker_left=") == "1"));
                assert_eq!(get("body="), "1", "{l}: the screen is the body surface");
            }
        } else {
            passes.push((l.to_string(), None));
        }
    }
    let (start, _) = &passes[0];
    assert!(start.starts_with("START 1 voixoff=1"), "{start}");
    g.start_dialog_now(1, true);
    assert_eq!(g.mode(), Mode::Dialog);
    // The golden's screen md5 → the port's for the same pass.
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut checked = 0;
    for (head, flip) in &passes[1..] {
        mark(&mut g);
        let running = if head.starts_with("TICK") {
            g.dialog_pass(true)
        } else {
            g.dialog_pass(false)
        };
        let want_running = !head.contains("ret=0");
        assert_eq!(running, want_running, "{head}");
        let Some((golden, left)) = flip else {
            assert!(!running, "{head}: a pass with no flip");
            continue;
        };
        assert_eq!(marker_left(&g), *left, "{head}: the last pass's marker");
        // The same picture in the original (the passes between two ticks)
        // ⇒ the same picture here (the warp's own picture may also repeat
        // across ticks here: the golden's stub changed it at each call).
        let ours = md5_screen(&g);
        for (gm, om) in &seen {
            if gm == golden {
                assert_eq!(*om, ours, "{head}: same screen as an earlier pass");
            }
        }
        seen.push((golden.clone(), ours));
        checked += 1;
    }
    assert!(checked >= 6, "{checked}");
}

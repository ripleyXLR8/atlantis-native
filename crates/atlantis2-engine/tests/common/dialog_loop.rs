//! The dialog loop against the original (`golden/dialog_loop.txt`): what
//! a pass does by the current line's kind, and the warp camera a dialog's
//! end leaves at 640 x 480. Shared by the dialog tests of each world.

#![allow(dead_code)]

use std::collections::BTreeSet;

use atlantis2_data::spr::SprBank;
use atlantis2_data::Folder;
use atlantis2_engine::bag::{Overlays, UiBank, UiDraw};
use atlantis2_engine::{Engine, Fpu};

const GOLDEN: &str = include_str!("../golden/dialog_loop.txt");

/// The calls of one dialog-loop pass for a line of kind `kind`.
fn calls(kind: i32) -> Vec<&'static str> {
    let head = format!("kind {kind}: ");
    let row = GOLDEN
        .lines()
        .find_map(|l| l.strip_prefix(head.as_str()))
        .unwrap_or_else(|| panic!("no golden row for kind {kind}"));
    row.split(' ').collect()
}

/// The camera floats (fx, fy, cx, cy, 1/fx, 1/fy, fov) the original's
/// camera holds after a dialog's end at 640 x 480 under `fpu`.
pub fn camera_after_dialog(fpu: Fpu) -> [u32; 7] {
    let cw = match fpu {
        Fpu::Chop => "0x0e7f",
        Fpu::Double => "0x027f",
        Fpu::Single => panic!("no golden for 0x007f"),
    };
    let head = format!("camera cw {cw} loop type 0 (0x0049da90): ");
    let row = GOLDEN
        .lines()
        .find_map(|l| l.strip_prefix(head.as_str()))
        .unwrap();
    let v: Vec<u32> = row
        .split(' ')
        .map(|f| u32::from_str_radix(f.split_once('/').unwrap().1, 16).unwrap())
        .collect();
    v.try_into().unwrap()
}

/// The engine's warp camera as [`camera_after_dialog`] lists it.
pub fn camera_bits(e: &Engine) -> [u32; 7] {
    let c = &e.camera;
    [c.fx, c.fy, c.cx, c.cy, c.inv_fx, c.inv_fy, c.fov].map(f32::to_bits)
}

/// After a dialog's end at 640 x 480: the panorama back at 76° with the
/// screen's centre, as FUN_00402960 leaves it.
pub fn assert_camera_after_dialog(e: &Engine, what: &str) {
    assert_eq!(
        camera_bits(e),
        camera_after_dialog(e.camera.fpu),
        "{what}: the camera after the dialog"
    );
}

/// Dialog `number` started in the current view and played to its end,
/// a tick every four passes; at the second pass of each line, the
/// golden's calls for its kind checked on the engine:
///
/// - the panorama (0x0045a930): the frame does not depend on what the
///   buffer held (drawn twice over two fillings, on passes without a
///   tick), and a tick's pass steps the place animations (a playing slot
///   moves) while a pass without one does not;
/// - the dowsing rod and the overlays (0x00475320, FUN_0049c6e0 and
///   FUN_0047f0b0): with the rod shown and the ring open, both drawn and
///   the rod stepped in the pass, and the frame not the one without them
///   (they are painted under the dialog frame, not left out); for the
///   other kinds neither is drawn nor stepped and the frame is the same;
/// - the turn (0x0040c6a0): a mouse motion turns the view in that pass.
///
/// Returns the kinds seen (and checked) and whether a playing slot was
/// there to check the step; the camera after the end is checked too.
pub fn check_dialog_loop(e: &mut Engine, number: u32) -> (BTreeSet<i32>, bool) {
    let (w, h) = (e.width(), e.height());
    let mut buf = vec![0u16; w * h];
    let mut seen = BTreeSet::new();
    let mut stepped_any = false;
    // The harness runs the 15 Hz task itself (a tick every four passes,
    // as the golden's emulated loop): the clock moves without the
    // scheduler.
    let mut clock = 0.0;
    e.set_time(clock);
    e.start_dialog(number)
        .unwrap_or_else(|err| panic!("dialog {number}: {err}"));
    let mut checked: Option<usize> = None;
    let mut ended = false;
    for pass in 0..40_000 {
        if pass % 4 == 0 {
            clock += 1.0 / 15.0;
            e.set_time(clock);
            e.tick();
        }
        e.pass(Some(&mut buf));
        if e.dialog().is_none() {
            ended = true;
            break;
        }
        let Some(d) = e.dialog() else { continue };
        let (Some(line), kind) = (d.current(), d.kind()) else {
            continue;
        };
        if checked == Some(line) || pass % 4 != 1 {
            continue;
        }
        checked = Some(line);
        seen.insert(kind);
        let want = calls(kind);
        let panorama = want.contains(&"panorama(0x0045a930)");
        let turn = want.contains(&"turn(0x0040c6a0)");
        // Drawn over two fillings, without a tick in between, from the
        // same `rand` state (a head's blinks draw from it each frame).
        let rand = e.rand;
        buf.fill(0);
        e.pass(Some(&mut buf));
        let a = buf.clone();
        buf.fill(0xffff);
        e.rand = rand;
        e.pass(Some(&mut buf));
        assert_eq!(
            a == buf,
            panorama,
            "dialog {number} line {line} (kind {kind}): the panorama under the frame"
        );
        // The rod and the ring, on passes without a tick.
        let rod_overlays = want.contains(&"rod_overlays(0x00475320)");
        check_rod_overlays(e, &mut buf, rod_overlays, number, line, kind);
        // The step: only on a tick's pass, when a slot plays.
        let frames = |e: &Engine| -> Vec<(bool, i32)> {
            e.anim.slots.iter().map(|s| (s.active, s.frame)).collect()
        };
        let playing = e
            .anim
            .slots
            .iter()
            .any(|s| s.active && s.playing && s.count > 1);
        let before = frames(e);
        e.pass(None);
        assert_eq!(frames(e), before, "dialog {number}: a step without a tick");
        if playing {
            // A tick at the same clock: the line goes on (its voice has
            // not ended), the 15 Hz flag is set.
            e.tick();
            e.pass(None);
            if e.dialog().is_some_and(|d| d.current() == Some(line)) {
                assert_eq!(
                    frames(e) != before,
                    panorama,
                    "dialog {number} line {line} (kind {kind}): the place animations' step"
                );
                stepped_any = true;
            }
        }
        // The turn.
        let yaw = e.camera.q;
        e.mouse_motion(40, 0);
        e.pass(None);
        e.mouse_motion(-40, 0);
        assert_eq!(
            e.camera.q != yaw,
            turn,
            "dialog {number} line {line} (kind {kind}): the turn"
        );
        e.look.stop();
    }
    assert!(ended, "dialog {number} does not end");
    if e.width() == 640 {
        assert_camera_after_dialog(e, &format!("dialog {number}"));
    }
    (seen, stepped_any)
}

/// The sprites of `bank` the last pass drew.
fn sprites_of(e: &Engine, bank: UiBank) -> Vec<(i32, i32, i32)> {
    e.ui_draws
        .iter()
        .filter_map(|d| match d {
            UiDraw::Sprite {
                bank: b,
                frame,
                x,
                y,
            } if *b == bank => Some((*frame, *x, *y)),
            _ => None,
        })
        .collect()
}

/// One pass of a line of kind `kind` with the dowsing rod shown
/// (`NEARW77` = 3, frame 0, step 1; only when a script with the variable
/// runs, as the rod reads 0 and goes away otherwise) and the ring open
/// at the screen centre, then one without them (no rod, no overlay), both
/// into a zeroed `buf`: whether the pass draws the rod (at (W / 2,
/// H − 240), frame 0, stepped to 1) and the ring, and whether the frame
/// differs, is `rod_overlays`. The state is put back after.
fn check_rod_overlays(
    e: &mut Engine,
    buf: &mut [u16],
    rod_overlays: bool,
    number: u32,
    line: usize,
    kind: i32,
) {
    let what = format!("dialog {number} line {line} (kind {kind})");
    let (w, h) = (e.width() as i32, e.height() as i32);
    let bank = |e: &Engine, name: &str| {
        let data = e.files().read(Folder::Spr, name).unwrap();
        Some(SprBank::parse(&data).unwrap())
    };
    let near = e.script_state().and_then(|s| s.var("NEARW77"));
    let shown = near.is_some();
    if shown {
        assert!(e.script_state_mut().unwrap().set_var("NEARW77", 3));
    }
    let rod = std::mem::take(&mut e.rod);
    let overlays = std::mem::replace(&mut e.overlays, Overlays::new());
    if shown {
        e.rod.bank = bank(e, "baton.spr");
        (e.rod.active, e.rod.frame, e.rod.step) = (true, 0, 1);
    }
    e.overlays.ring_bank = bank(e, "anneau.spr");
    (e.overlays.ring, e.overlays.centre) = (true, (w / 2, h / 2));
    buf.fill(0);
    e.pass(Some(buf));
    let with = buf.to_vec();
    let (rods, rings) = (sprites_of(e, UiBank::Rod), sprites_of(e, UiBank::Ring));
    let stepped = e.rod.frame;
    e.rod = Default::default();
    e.overlays = Overlays::new();
    buf.fill(0);
    e.pass(Some(buf));
    (e.rod, e.overlays) = (rod, overlays);
    if let Some(near) = near {
        assert!(e.script_state_mut().unwrap().set_var("NEARW77", near));
    }
    if rod_overlays {
        if shown {
            assert_eq!(rods, [(0, w / 2, h - 240)], "{what}: the rod");
            assert_eq!(stepped, 1, "{what}: the rod's step");
        }
        assert_eq!(rings, [(0, w / 2, h / 2)], "{what}: the ring");
    } else {
        assert!(
            rods.is_empty() && rings.is_empty(),
            "{what}: the rod and the ring"
        );
        assert_eq!(stepped, 0, "{what}: the rod's step");
    }
    assert_eq!(
        with != buf,
        rod_overlays,
        "{what}: the rod and the ring on the frame"
    );
}

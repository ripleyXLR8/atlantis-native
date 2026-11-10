//! The dialog camera of Rhea's old couple. `GereOrientation`
//! (@ 0x0040b700) gives the speakers `dame` and `mons` their own set-up
//! (@ 0x0040b7c2..0x0040b80a, 0x0040b8e3..0x0040b907): `SetFov` with the
//! field of view Rhea's entry (@ 0x004cd300) stored for the view,
//! `Horizontalize`, then `RunOneCommand("(AngleWarpXNow=%d)", now = 1)`
//! with its yaw. The original's values, its entry then its set-up run
//! under emulation for views 10..13 (the audit's Unicorn runs):
//!
//! ```text
//! vue 10 dame: SetFov(50.0) Horizontalize() RunOneCommand('(AngleWarpXNow=485)', now=1)
//! vue 10 mons: SetFov(50.0) Horizontalize() RunOneCommand('(AngleWarpXNow=100)', now=1)
//! vue 11 dame: SetFov(60.0) ... (AngleWarpXNow=165); mons: SetFov(80.0) ... (AngleWarpXNow=100)
//! vue 12 dame: 80 / 115; mons: 60 / 15
//! vue 13 dame: 40 / 90; mons: 40 / 20
//! ```
//!
//! The section that started the dialog waits, so the command must run at
//! once (`now` = 1), not be queued behind the wait.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::Fpu;
use atlantis3_data::A3Files;
use atlantis3_engine::Game;

/// (view, (dame FOV, dame yaw), (mons FOV, mons yaw)).
const GOLDEN: [(u32, (f32, i32), (f32, i32)); 4] = [
    (10, (50.0, 485), (50.0, 100)),
    (11, (60.0, 165), (80.0, 100)),
    (12, (80.0, 115), (60.0, 15)),
    (13, (40.0, 90), (40.0, 20)),
];

fn events_for(fov: f32, yaw: i32) -> Vec<String> {
    vec![
        format!("SetFov({:08x})", fov.to_bits()),
        "Horizontalize".to_string(),
        format!("RunOneCommand((AngleWarpXNow={yaw}), now=1)"),
    ]
}

/// The camera's yaw is the command's (`AngleWarpXNow` reads back as
/// 90° − the angle, `Camera::set_yaw`), the pitch levelled.
fn check_camera(g: &Game, yaw: i32, what: &str) {
    let want = std::f32::consts::FRAC_PI_2 - deg_to_rad_a3(g.camera().fpu, yaw);
    let (y, p) = g.camera().angles();
    let tau = std::f32::consts::TAU;
    let d = (y - want).rem_euclid(tau);
    assert!(d.min(tau - d) < 1e-4, "{what}: yaw {y} want {want}");
    assert!(p.abs() < 1e-4, "{what}: pitch {p}");
}

#[test]
fn the_old_couples_dialog_camera_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    assert!(g.board_begin("rhea.wam", 5));
    g.board_wam_setup();
    for (vue, dame, mons) in GOLDEN {
        g.board_enter(vue, 100.0);
        g.reload_dialogs(1);
        g.dialog_heads = false;
        g.dialog_events = Some(Vec::new());
        let fpu = Fpu::Chop;
        g.board_camera(deg_to_rad_a3(fpu, 300), deg_to_rad_a3(fpu, -10));
        // The section that ran `(Dial=242)` waits for the dialog.
        g.script_mut().unwrap().resume_pending = true;
        // Dialog 242: the old woman, then the old man.
        g.start_dialog_now(242, false);
        let got = std::mem::take(g.dialog_events.as_mut().unwrap());
        let want = events_for(dame.0, dame.1);
        assert!(
            got.windows(3).any(|w| w == want.as_slice()),
            "vue {vue} dame: {got:?}"
        );
        assert!(
            !got.iter().any(|e| e == "Orient"),
            "vue {vue} dame: {got:?}"
        );
        check_camera(&g, dame.1, &format!("vue {vue} dame"));
        let want = events_for(mons.0, mons.1);
        let mut seen = false;
        for _ in 0..2000 {
            g.click();
            if !g.dialog_pass(true) {
                break;
            }
            let got = std::mem::take(g.dialog_events.as_mut().unwrap());
            if got.windows(3).any(|w| w == want.as_slice()) {
                seen = true;
                break;
            }
        }
        assert!(seen, "vue {vue}: the old man's set-up");
        check_camera(&g, mons.1, &format!("vue {vue} mons"));
        while g.dialog_pass(true) {
            g.click();
        }
        g.script_mut().unwrap().resume_pending = false;
        assert!(g.script().unwrap().a3.queue.is_empty(), "nothing queued");
    }
}

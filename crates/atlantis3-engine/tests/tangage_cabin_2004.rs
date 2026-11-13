//! *Atlantis Evolution*'s 2004 cabin pitches (`ATLANTIS4_CD`): chapter 0's
//! `/INIT` (chapi000.con) sets `tangagewarp=1,0,0,1,5,5,0` (the axis
//! (0, 0, 1), 5°, speed 5, undamped: FUN_00445450), plays the intro, then
//! `anglewarpx=90`, `anglewarpy=20`, `vue=1`: the view entry FUN_00442c80
//! with the pitching on hangs the camera node under the ship node, and
//! every warp frame (FUN_00445760) rocks the ship about the world's Z axis
//! (the view's forward axis at yaw 90°) between −5° and +5°.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};

fn game() -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
        atlantis3_data::testing::missing("ATLANTIS4_CD not set");
        return None;
    };
    let src = Arc::new(Evolution2004Source::new(Arc::new(
        atlantis_data::StdDir::new(&dir),
    )));
    Some(Game::new(A3Files::evolution_2004(src)).expect("the 2004 program's game"))
}

/// The relative rotation b · a* (x, y, z, w) in doubles.
fn relative(a: [f32; 4], b: [f32; 4]) -> [f64; 4] {
    let [ax, ay, az, aw] = a.map(|x| -f64::from(x));
    let aw = -aw;
    let [bx, by, bz, bw] = b.map(f64::from);
    [
        bw * ax + bx * aw + by * az - bz * ay,
        bw * ay - bx * az + by * aw + bz * ax,
        bw * az + bx * ay - by * ax + bz * aw,
        bw * aw - bx * ax - by * ay - bz * az,
    ]
}

#[test]
fn the_cabin_pitches() {
    let Some(mut g) = game() else { return };
    assert!(g.new_game());
    // The intro skipped: the section goes on to the cabin's view 1.
    g.step(0.0);
    g.skip();
    g.step(0.01);
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
    assert_eq!(g.mode(), Mode::Warp);
    let ship = g.ship().clone();
    assert!(ship.on, "tangagewarp=1 set by /INIT");
    assert!(ship.cam.is_some(), "the camera node under the ship");
    assert_eq!(ship.axis, [0.0, 0.0, 1.0]);
    // 5 · π · 1/180 as a float; speed 5; undamped.
    assert_eq!(ship.amp.to_bits(), 0x3db2_b8c3);
    assert_eq!((ship.speed, ship.damp), (5.0, 0));
    // The entry at phase 0: the arrival angles.
    let (yaw, pitch) = g.camera().angles();
    assert!(
        (yaw.to_degrees() - 90.0).abs() < 1e-3,
        "{}",
        yaw.to_degrees()
    );
    assert!(
        (pitch.to_degrees() - 20.0).abs() < 1e-3,
        "{}",
        pitch.to_degrees()
    );

    // 25 frames a second for 40 s: the world orientation turns about the
    // world's Z by the phase, which swings within ±5°, reaching both ends.
    // The phase moves by speed · (cos(π φ / A) · 0.5 + 0.6) degrees a
    // second: a swing from one end to the other takes
    // (A / π) · 2π / √(3² − 2.5²) / 1° = 6.03 s.
    let q0 = g.camera().q;
    let amp = f64::from(ship.amp);
    let (mut lo, mut hi, mut flips) = (0.0f64, 0.0f64, Vec::new());
    let mut up = ship.up;
    let mut t = 0.01;
    for _ in 0..1000 {
        t += 0.04;
        g.step(t);
        if g.mode() != Mode::Warp {
            continue;
        }
        let s = g.ship();
        if s.up != up {
            flips.push(t);
            up = s.up;
        }
        let r = relative(q0, g.camera().q);
        let half_sin = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        let angle = 2.0 * half_sin.atan2(r[3].abs()) * r[2].signum() * r[3].signum();
        if half_sin > 1e-4 {
            // About the world's Z only.
            assert!(
                (r[2].abs() / half_sin) > 0.9999,
                "axis {:?}",
                [r[0], r[1], r[2]].map(|x| x / half_sin)
            );
        }
        // The phase is the swing's angle.
        assert!(
            (angle - f64::from(s.phase)).abs() < 1e-4,
            "{angle} vs {}",
            s.phase
        );
        lo = lo.min(angle);
        hi = hi.max(angle);
    }
    assert!(flips.len() >= 6, "{flips:?}");
    for w in flips.windows(2) {
        assert!((w[1] - w[0] - 6.03).abs() < 0.1, "{flips:?}");
    }
    assert!(
        (hi - amp).abs() < 1e-4 && (lo + amp).abs() < 1e-4,
        "{lo} .. {hi}"
    );
    assert!((hi.to_degrees() - 5.0).abs() < 0.01);
}

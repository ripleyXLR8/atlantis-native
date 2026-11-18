//! The interdimension's swaying camera against the original under
//! emulation (`tests/golden/a3_sway.txt`, atlantis3-re
//! `tools/emu/a3_sway.py`): `Interdim.wam` view 1 entered at 0, 0, one
//! frame drawn (the control word 0x0e7f from there), then frames of the
//! 25 Hz step 0x004317a0 (per tick: the look's speeds × 0.8, turns and
//! limits, then the sway branch @ 0x004319b7..0x00431a8e) and mouse moves
//! (0x004313a0: the look's speeds, then the amplitude += min(dx², 1000),
//! at most 10000). After each step: the ticks, the amplitude and phase,
//! the speeds, the read-back angles and the projection, bit for bit.
//! Then the game's own wiring: the place hook's byte in `interdim.wam`.
//!
//! The game part needs the game: `ATLANTIS3_DATA`.

use atlantis2_engine::look::Look;
use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::{Camera, Fpu};
use atlantis3_data::A3Files;
use atlantis3_engine::game::dialog_fov;
use atlantis3_engine::sway::Sway;
use atlantis3_engine::{Game, Mode};

const GOLDEN: &str = include_str!("golden/a3_sway.txt");

fn le32(hex: &str) -> u32 {
    u32::from_str_radix(hex, 16).unwrap().swap_bytes()
}

fn le32s(list: &str) -> Vec<u32> {
    list.split(',').map(le32).collect()
}

#[test]
fn the_interdimension_sways_as_the_original() {
    let mut cam = Camera::new(Fpu::Double, 800, 600);
    let fpu = cam.fpu;
    cam.enter(deg_to_rad_a3(fpu, 0), deg_to_rad_a3(fpu, 0));
    let mut look = Look {
        limit_y: 70,
        limit_x: -1,
        ..Default::default()
    };
    let mut sway = Sway::default();
    let (mut clock, mut last, mut acc) = (100.0f64, 100.0f64, 0f32);
    let mut ticks = 0;
    let mut fields = 0;
    for line in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let (key, value) = line.split_once(' ').unwrap();
        match key {
            "sway.byte" => {
                assert_eq!(value, "1");
                continue;
            }
            "sway.cw" => {
                // The frame drawn after the entry leaves 0x0e7f.
                assert_eq!(value, "0e7f");
                cam.fpu = Fpu::Chop;
                continue;
            }
            _ => {}
        }
        let (step, field) = match key.rsplit_once('.') {
            Some((s, f)) if s.contains('.') => (s, Some(f)),
            _ => (key, None),
        };
        let Some(field) = field else {
            let f: Vec<&str> = value.split(':').collect();
            match f[0] {
                "enter" => {}
                "mouse" => {
                    let (dx, dy): (i32, i32) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                    look.mouse(cam.fpu, dx, dy);
                    sway.mouse(cam.fpu, dx);
                }
                "frame" => {
                    clock += f[1].parse::<f64>().unwrap();
                    let fpu = cam.fpu;
                    let dt = (clock - last).min(0.5);
                    last = clock;
                    acc = fpu.f32(fpu.add(f64::from(acc), fpu.mul(dt, 25.0)));
                    ticks = 0;
                    while acc >= 1.0 {
                        acc -= 1.0;
                        look.frame(&mut cam);
                        let deg = sway.tick(&mut cam, true);
                        cam.set_fov(dialog_fov(cam.fpu, deg), 800, 600);
                        ticks += 1;
                    }
                }
                _ => panic!("{line}"),
            }
            continue;
        };
        let what = format!("{step} {field}");
        match field {
            "ticks" => assert_eq!(ticks.to_string(), value, "{what}"),
            "amp" => assert_eq!(sway.amp.to_bits(), le32(value), "{what}"),
            "phase" => assert_eq!(sway.phase.to_bits(), le32(value), "{what}"),
            "speed" => assert_eq!(
                vec![look.yaw_speed.to_bits(), look.pitch_speed.to_bits()],
                le32s(value),
                "{what}"
            ),
            "angles" => {
                let (y, p) = cam.angles();
                assert_eq!(vec![y.to_bits(), p.to_bits()], le32s(value), "{what}");
            }
            "proj" => assert_eq!(
                cam.projection().map(f32::to_bits).to_vec(),
                le32s(value),
                "{what}"
            ),
            _ => panic!("{line}"),
        }
        fields += 1;
    }
    assert_eq!(fields, 871, "fields checked");
}

/// The game: `interdim.wam` sets the byte, its warp steps sway (the 130°
/// field of view: fx 186.523 against 511.977), the mouse feeds the
/// amplitude in the warp, and another place keeps the 76° view.
#[test]
fn the_game_sways_in_the_interdimension_only() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    assert!(g.board_begin("Interdim.wam", 5));
    let t = 100.0;
    g.board_enter_at(1, 0.0, 0.0, t);
    for k in 1..=5 {
        g.step(t + 0.04 * f64::from(k));
    }
    assert_eq!(g.mode(), Mode::Warp);
    let s = g.test_sway();
    assert!(s.phase > 0.0, "{s:?}");
    let fx = g.camera().projection()[0];
    assert!((fx - 186.52).abs() < 0.1, "fx {fx}");
    g.mouse_motion(30, 0);
    assert_eq!(g.test_sway().amp, 900.0);

    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    assert!(g.board_begin("hoggar.wam", 1));
    g.board_enter_at(4, 0.0, 0.0, t);
    for k in 1..=5 {
        g.step(t + 0.04 * f64::from(k));
    }
    assert_eq!(g.test_sway().phase, 0.0);
    let fx = g.camera().projection()[0];
    assert!((fx - 511.98).abs() < 0.1, "fx {fx}");
}

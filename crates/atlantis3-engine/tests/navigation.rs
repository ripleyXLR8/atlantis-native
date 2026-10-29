//! Zone picks and moves against the original's own code under emulation
//! (`tests/golden/wam_pick_*.txt`, `moves_*.txt`): the Hoggar (the first
//! interactive place) and the funeral chamber (the boot place). Picks:
//! every view at a grid of angles, the zone under the screen centre and
//! under 25 screen points, and every zone's aim. Moves: a click on each
//! move zone from several start angles, the target, whether it turns or
//! goes at once, then the turn tick by tick (the angles read back, to the
//! golden's 0.001°) and the arrival; the starts are the golden's floats.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use atlantis3_data::A3Files;
use atlantis3_engine::game::zone_aim_a3;
use atlantis3_engine::Game;

fn game(wam: &str, world: i32) -> Option<Game> {
    let dir = atlantis3_data::testing::install_dir()?;
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut g = Game::new(files).unwrap();
    g.set_world(world);
    assert!(g.load_place(wam));
    Some(g)
}

fn rad(deg: f64) -> f32 {
    deg.to_radians() as f32
}

fn picks(wam: &str, world: i32, golden: &str) {
    let Some(mut g) = game(wam, world) else {
        return;
    };
    let (mut n, mut bad) = (0, Vec::new());
    let mut entered = (0, 0, 0);
    for line in golden.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let zone = |s: &str| -> Option<usize> {
            s.parse::<i32>().ok().and_then(|z| usize::try_from(z).ok())
        };
        match f[0] {
            "view" => {
                let (v, y, p): (u32, i32, i32) = (
                    f[1].parse().unwrap(),
                    f[3].parse().unwrap(),
                    f[5].parse().unwrap(),
                );
                g.enter_at(v, rad(y.into()), rad(p.into()));
                entered = (v, y, p);
                if g.centre_zone() != zone(f[7]) {
                    bad.push(line.to_string());
                }
                n += 1;
            }
            "grid" => {
                let (v, y, p): (u32, i32, i32) = (
                    f[1].parse().unwrap(),
                    f[2].parse().unwrap(),
                    f[3].parse().unwrap(),
                );
                if entered != (v, y, p) {
                    g.enter_at(v, rad(y.into()), rad(p.into()));
                    entered = (v, y, p);
                }
                let (sx, sy) = f[4].split_once(',').unwrap();
                if g.pick_at(sx.parse().unwrap(), sy.parse().unwrap()) != zone(f[6]) {
                    bad.push(line.to_string());
                }
                n += 1;
            }
            "aim" => {
                let (v, z): (u32, usize) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                let zone = &g.wam().view(v).unwrap().zones[z];
                let (y, p) = zone_aim_a3(atlantis2_engine::Fpu::Double, &zone.points);
                let mut yd = f64::from(y).to_degrees();
                if yd < 0.0 {
                    yd += 360.0;
                }
                let pd = f64::from(p).to_degrees();
                let want: (f64, f64) = (f[4].parse().unwrap(), f[6].parse().unwrap());
                if (yd - want.0).abs() > 1e-4 || (pd - want.1).abs() > 1e-4 {
                    bad.push(format!("{line}: {yd:.4} {pd:.4}"));
                }
                n += 1;
            }
            _ => {}
        }
    }
    assert!(n > 1000, "{n} rows");
    assert!(
        bad.is_empty(),
        "{wam}: {} of {n} picks differ: {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

#[test]
fn hoggar_picks() {
    picks("hoggar.wam", 1, include_str!("golden/wam_pick_hoggar.txt"));
}

#[test]
fn cfuner_picks() {
    picks("CFuner.wam", 2, include_str!("golden/wam_pick_cfuner.txt"));
}

/// The angles read back, in degrees 0..360 and −90..90.
fn angles(g: &Game) -> (f64, f64) {
    let (y, p) = g.camera().angles();
    let mut y = f64::from(y).to_degrees();
    if y < 0.0 {
        y += 360.0;
    }
    (y, f64::from(p).to_degrees())
}

fn near(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d) < 0.0025
}

fn moves(wam: &str, world: i32, golden: &str, fpu: atlantis2_engine::Fpu) {
    let Some(mut g) = game(wam, world) else {
        return;
    };
    g.set_fpu(fpu);
    let lines: Vec<&str> = golden.lines().filter(|l| !l.starts_with('#')).collect();
    let (mut cases, mut ticks, mut bad) = (0, 0, Vec::<String>::new());
    let mut i = 0;
    while i < lines.len() {
        let f: Vec<&str> = lines[i].split_whitespace().collect();
        i += 1;
        if f[0] != "case" {
            continue;
        }
        cases += 1;
        let v: u32 = f[1].parse().unwrap();
        let bits = |s: &str| f32::from_bits(u32::from_str_radix(s, 16).unwrap());
        // The harness enters every case from the camera of the warp's
        // initialisation.
        g.reset_camera();
        g.enter_at(v, bits(f[19]), bits(f[20]));
        let want_pick: i32 = f[7].parse().unwrap();
        let pick = g.centre_zone().map_or(-1, |z| z as i32);
        if pick != want_pick {
            bad.push(format!("{}: pick {pick}", lines[i - 1]));
            continue;
        }
        let (target, turning, pending) = g.click_centre_now();
        let want_target: i64 = f[10].parse().unwrap();
        let ok = target.map_or(-1, i64::from) == want_target
            && turning == (f[12] == "1")
            && pending == (f[14] == "1");
        if !ok {
            bad.push(format!("{}: {target:?} {turning} {pending}", lines[i - 1]));
        }
        let (ay, ap) = g.move_arrival();
        let (mut ayd, apd) = (f64::from(ay).to_degrees(), f64::from(ap).to_degrees());
        if ayd < 0.0 {
            ayd += 360.0;
        }
        if want_target >= 0
            && (!near(ayd, f[16].parse().unwrap()) || !near(apd, f[17].parse().unwrap()))
        {
            bad.push(format!("{}: arrival {ayd:.3} {apd:.3}", lines[i - 1]));
        }
        while i < lines.len() && lines[i].starts_with("tick") {
            let t: Vec<&str> = lines[i].split_whitespace().collect();
            i += 1;
            let going = g.turn_tick();
            let (y, p) = angles(&g);
            ticks += 1;
            let (ty, tp): (f64, f64) = (t[2].parse().unwrap(), t[3].parse().unwrap());
            if !near(y, ty) || !near(p, tp) || going != (t[4] == "1") {
                bad.push(format!(
                    "{} / {}: {y:.3} {p:.3} {going}",
                    lines[i - 1],
                    f[1..3].join(" ")
                ));
            }
        }
        // The turn ended with the golden's.
        if g.turning() {
            bad.push(format!("{}: still turning", lines[i - 1]));
            while g.turn_tick() {}
        }
    }
    assert!(cases > 40, "{cases}");
    assert!(
        bad.is_empty(),
        "{wam}: {} problems in {cases} cases, {ticks} ticks: {:?}",
        bad.len(),
        &bad[..bad.len().min(6)]
    );
}

#[test]
fn hoggar_moves() {
    moves(
        "hoggar.wam",
        1,
        include_str!("golden/moves_hoggar.txt"),
        atlantis2_engine::Fpu::Double,
    );
}

/// The same under 0x0e7f, the control word of the game once a warp frame
/// was drawn.
#[test]
fn hoggar_moves_0e7f() {
    moves(
        "hoggar.wam",
        1,
        include_str!("golden/moves_hoggar_0e7f.txt"),
        atlantis2_engine::Fpu::Chop,
    );
}

#[test]
fn cfuner_moves() {
    moves(
        "CFuner.wam",
        2,
        include_str!("golden/moves_cfuner.txt"),
        atlantis2_engine::Fpu::Double,
    );
}

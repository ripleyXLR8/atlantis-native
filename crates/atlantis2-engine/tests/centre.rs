//! The screen centre and the clicks on sprites against the original
//! (`tests/golden/centre_tibet.txt`): with the same texel under the centre,
//! the same passes draw the same object in hand, cursor and centre marker,
//! choose the same cursor, take the same object, and run the same scans
//! with the same `CLICZONE` / `CLICPERSO` / `CLICOBJET`; a click in the
//! arrival frame runs no click scan.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the test is skipped.

use std::collections::BTreeMap;
use std::path::PathBuf;

use atlantis2_data::A2Files;
use atlantis2_engine::anim::Texel;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::{CentreDraw, CursorBank, Engine, Event, Fpu, LoopType, ScanVars, Variant};

const GOLDEN: &str = include_str!("golden/centre_tibet.txt");

/// The texel the original's harness gives when a case sets none: on no
/// sprite.
const NO_TEXEL: Texel = (0, -100, -100);

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn degrees(r: f32) -> f64 {
    f64::from(r) / (std::f64::consts::PI / 180.0)
}

fn radians(d: f64) -> f32 {
    (d * (std::f64::consts::PI / 180.0)) as f32
}

fn bank(b: CursorBank) -> &'static str {
    match b {
        CursorBank::Interface => "interface",
        CursorBank::Objects => "objects",
        CursorBank::Puzzle => "puzzle",
    }
}

fn scan(v: &ScanVars) -> String {
    format!(
        "scan(DEPARTVUE={},ARRIVEEVUE={},CLICZONE={},CLICPERSO={},CLICOBJET={})",
        v.departvue, v.arriveevue, v.cliczone, v.clicperso, v.clicobjet
    )
}

struct Driver {
    e: Engine,
    pass_no: u64,
    clock: f64,
}

impl Driver {
    fn enter_at(&mut self, view: u32, yaw: f64, pitch: f64) {
        self.e.enter_at(view, radians(yaw), radians(pitch)).unwrap();
        self.e.events.clear();
    }

    fn aim(&self, view: u32, zone: usize) -> (f64, f64) {
        let (y, p) = self.e.zone_aim(view, zone).unwrap();
        (degrees(y), degrees(p))
    }

    fn after(&self) -> String {
        let e = &self.e;
        let cursor = match e.cursor.shown {
            Some(f) => format!("{}#{f}", bank(e.cursor.bank)),
            None => "none".into(),
        };
        let hand = match e.hand {
            Some((_, s)) => format!("objects#{s}"),
            None => "none".into(),
        };
        let (hit, state) = e.sprite_under_centre();
        format!(
            "cursor {cursor} range {} hand {hand} OBJETENMAIN {} CLICOBJET {} CLICPERSO {} hit {} state {state} bag {}",
            e.cursor.start,
            e.vars.objet_en_main,
            e.vars.clicobjet,
            e.vars.clicperso,
            hit.map_or(-1, |n| n as i32),
            e.bag.len()
        )
    }

    /// `passes` passes with clicks at the given passes and the texel under
    /// the centre changed at the given passes.
    fn run(
        &mut self,
        header: &str,
        clicks: &[u32],
        passes: u32,
        texels: &BTreeMap<u32, Texel>,
    ) -> Vec<String> {
        let mut out = vec![format!("== {header}")];
        let (y, p) = self.e.camera.angles();
        out.push(format!(
            "start view {} yaw {:.3} pitch {:.3} centre zone {}",
            self.e.place().1,
            degrees(y),
            degrees(p),
            self.e.centre_index()
        ));
        for n in 0..passes {
            if let Some(t) = texels.get(&n) {
                self.e.forced_centre = Some(*t);
            }
            if self.pass_no.is_multiple_of(4) {
                self.e.tick();
            }
            self.pass_no += 1;
            let mut ev = Vec::new();
            if clicks.contains(&n) {
                self.clock += 10.0;
                self.e.click(self.clock);
                ev.push("click".to_string());
            }
            self.e.events.clear();
            self.e.pass(None);
            for d in &self.e.centre_draws {
                ev.push(match d {
                    CentreDraw::Sprite {
                        bank: b,
                        frame,
                        x,
                        y,
                    } => {
                        format!("spr {}#{frame}@{x},{y}", bank(*b))
                    }
                    CentreDraw::Line { from, to, argb } => {
                        format!("line {},{}-{},{} {argb:08x}", from.0, from.1, to.0, to.1)
                    }
                    CentreDraw::Crosshair { frame, x, y } => {
                        format!("spr viseur#{frame}@{x},{y}")
                    }
                });
            }
            for e in &self.e.events {
                match e {
                    Event::DepartScan(v) | Event::ClickScan(v) | Event::ArrivalScan(v) => {
                        ev.push(scan(v))
                    }
                    _ => {}
                }
            }
            out.push(format!(
                "p{n} t{} view {} zone {} loop {} turn {} | {} | {}",
                self.e.ticks,
                self.e.place().1,
                self.e.centre_index(),
                i32::from(self.e.loop_type == LoopType::Video),
                i32::from(self.e.turning()),
                ev.join("; "),
                self.after()
            ));
        }
        out.push(format!("end view {}", self.e.place().1));
        out
    }
}

#[test]
fn centre_and_sprite_clicks_match_the_original() {
    let Some(files) = game() else { return };
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
    e.trace = true;
    e.rand = Rand::new(1000);
    let mut d = Driver {
        e,
        pass_no: 0,
        clock: 0.0,
    };
    // The golden's cases, and the texels its harness chose (from the
    // slots' frame boxes).
    let mut cases: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    let mut texels = Vec::new();
    let mut current: Option<String> = None;
    for line in GOLDEN.lines() {
        if let Some(t) = line.strip_prefix("# texel ") {
            let v: Vec<i32> = t.split(',').map(|x| x.parse().unwrap()).collect();
            texels.push((v[0] as usize, v[1], v[2]));
        } else if let Some(h) = line.strip_prefix("== ") {
            let name = h.split(' ').next().unwrap().to_string();
            cases.insert(name.clone(), (h.to_string(), vec![line.to_string()]));
            current = Some(name);
        } else if line.starts_with('#') || line.starts_with("errors") {
            continue;
        } else if let Some(c) = &current {
            cases.get_mut(c).unwrap().1.push(line.to_string());
        }
    }
    assert_eq!(texels.len(), 3, "the golden's texels");
    let none = |n: u32| BTreeMap::from([(n, NO_TEXEL)]);
    let mut got: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let header = |c: &str| cases[c].0.clone();

    // T: the object of view 5 under the centre, a click takes it; off it,
    // a click on the move zone with the object in hand.
    let (y, p) = d.aim(5, 1);
    d.enter_at(5, y, p);
    let t = BTreeMap::from([(0, texels[0]), (4, NO_TEXEL)]);
    got.insert("T", d.run(&header("T"), &[2, 8], 40, &t));
    // P: the shaman of view 7, a character without subjects.
    d.enter_at(7, 0.0, 0.0);
    let t = BTreeMap::from([(0, texels[1])]);
    got.insert("P", d.run(&header("P"), &[2, 5], 8, &t));
    // R: a click in the arrival frame.
    let (y, p) = d.aim(6, 0);
    d.enter_at(6, y, p);
    got.insert("R", d.run(&header("R"), &[2, 3, 6], 8, &none(0)));
    // Z1..Z4: the cursor over zones.
    for (name, view, zone, off) in [
        ("Z1", 2, Some(0), (0.0, 0.0)),
        ("Z2", 4, Some(2), (0.0, 0.0)),
        ("Z3", 10, Some(1), (0.0, 0.0)),
        ("Z4", 10, None, (0.0, -60.0)),
    ] {
        let (y, p) = zone.map_or(off, |z| d.aim(view, z));
        d.enter_at(view, y, p);
        got.insert(name, d.run(&header(name), &[], 10, &none(0)));
    }
    // M opens the subject menu from the harness's subject table, which
    // `Engine::new_game` (no scripts) does not have: the menu's frames are
    // compared in `tests/subjects.rs`. Every other case is compared here.
    assert_eq!(cases.len(), got.len() + 1, "the golden's cases");
    for name in ["T", "P", "R", "Z1", "Z2", "Z3", "Z4"] {
        let rows = &got[name];
        let want = &cases
            .get(name)
            .unwrap_or_else(|| panic!("case {name} not in the golden"))
            .1;
        for (i, (g, w)) in rows.iter().zip(want).enumerate() {
            assert_eq!(g, w, "case {name}, row {i}");
        }
        assert_eq!(rows.len(), want.len(), "case {name}: row count");
    }
}

/// The texel under the screen centre (FUN_0047db60, run by the original
/// under emulation: `tests/golden/centre_texel.txt`, atlantis2-re
/// `tools/emu/centre_texel.py`) against the port's cube mapping, on view
/// 2 entered at every 5° of yaw and pitch, under 0x0e7f.
#[test]
fn the_texel_under_the_centre_matches_the_original() {
    use atlantis2_engine::camera::Camera;
    let golden = include_str!("golden/centre_texel.txt");
    let (mut n, mut bad) = (0, Vec::new());
    for row in golden.lines().filter(|l| l.starts_with("yaw ")) {
        let f: Vec<&str> = row.split(' ').collect();
        let (yaw, pitch): (f64, f64) = (f[1].parse().unwrap(), f[3].parse().unwrap());
        let want: Vec<i32> = f[5].split(',').map(|v| v.parse().unwrap()).collect();
        let mut cam = Camera::new(Fpu::Chop, 640, 480);
        cam.enter(yaw.to_radians() as f32, pitch.to_radians() as f32);
        let got = cam
            .texel(cam.cx, cam.cy)
            .map_or((-1, 320, 240), |(face, x, y)| (face as i32, x, y));
        if got != (want[0], want[1], want[2]) {
            bad.push(format!("{row}: got {got:?}"));
        }
        n += 1;
    }
    assert_eq!(n, 72 * 35);
    assert!(
        bad.is_empty(),
        "{} of {n} differ: {:?}",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
}

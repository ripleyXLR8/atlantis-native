//! Tibet's moves and place animations against the original
//! (`tests/golden/moves_tibet.txt`): the same clicks at the same passes
//! give the same turns, frame by frame, the same view entries, videos and
//! scans; the SPW slots of views 7 and 10 play the same frames tick by
//! tick.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::collections::BTreeMap;
use std::path::PathBuf;

use atlantis2_data::A2Files;
use atlantis2_engine::anim::MsvcRand;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const GOLDEN: &str = include_str!("golden/moves_tibet.txt");

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

/// Python's `math.degrees` / `math.radians` (the harness's conversions).
fn degrees(r: f32) -> f64 {
    f64::from(r) / (std::f64::consts::PI / 180.0)
}

fn radians(d: f64) -> f32 {
    (d * (std::f64::consts::PI / 180.0)) as f32
}

fn fmt3(x: f64) -> String {
    format!("{x:.3}")
}

/// The harness's `Moves` driver over the engine.
struct Driver {
    e: Engine,
    pass_no: u64,
    clock: f64,
}

#[derive(Clone, PartialEq)]
struct State {
    loop_type: i32,
    view: u32,
    turn: bool,
    zone: i32,
    vars: [u32; 3],
    yaw: f32,
    pitch: f32,
}

impl Driver {
    fn new(files: A2Files) -> Driver {
        let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
        e.trace = true;
        e.sprite_picks = false;
        e.anim.rand = MsvcRand(1000);
        Driver {
            e,
            pass_no: 0,
            clock: 0.0,
        }
    }

    fn state(&self) -> State {
        let (yaw, pitch) = self.e.camera.angles();
        State {
            loop_type: i32::from(self.e.loop_type == LoopType::Video),
            view: self.e.place().1,
            turn: self.e.turning(),
            zone: self.e.centre_index(),
            vars: [
                self.e.vars.arrivee_vue,
                self.e.vars.last_vue,
                self.e.vars.vue,
            ],
            yaw,
            pitch,
        }
    }

    fn enter_at(&mut self, view: u32, yaw: f64, pitch: f64) {
        self.e.enter_at(view, radians(yaw), radians(pitch)).unwrap();
        self.e.events.clear();
    }

    fn events(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        let mut spw = None;
        for ev in self.e.events.drain(..) {
            if let Some(n) = spw {
                if !matches!(ev, Event::SpwLoad { .. }) {
                    out.push(format!("enter spw={n}"));
                    spw = None;
                }
            }
            match ev {
                Event::Click { .. } => out.push("click".into()),
                Event::DepartScan { target } => out.push(format!("depart {target}")),
                Event::ClickScan { zone } => out.push(format!("clickscan {zone}")),
                Event::ArrivalScan { view } => out.push(format!("arrive {view}")),
                Event::TurnSetup { yaw, pitch, .. } => out.push(format!(
                    "turn {},{}",
                    fmt3(degrees(yaw)),
                    fmt3(degrees(pitch))
                )),
                Event::Enter { .. } => spw = Some(0),
                Event::SpwLoad { .. } => *spw.as_mut().unwrap() += 1,
                Event::VideoStart { frames, .. } => out.push(match frames {
                    Some(n) => format!("video {n}"),
                    None => "video missing".into(),
                }),
                Event::VideoFrame(k) => out.push(format!("frame {k}")),
                Event::VideoEnd => out.push("close".into()),
                Event::PuzzleHook { zone } => out.push(format!("hook {zone}")),
                Event::Take { id } => out.push(format!("take {id}")),
            }
        }
        if let Some(n) = spw {
            out.push(format!("enter spw={n}"));
        }
        out
    }

    /// `Moves.run`: `passes` passes, the clicks at their pass (seconds
    /// since the previous click, `None` for a lone click).
    fn run(&mut self, clicks: &BTreeMap<u32, Option<f64>>, passes: u32, out: &mut Vec<String>) {
        let s0 = self.state();
        out.push(format!(
            "start view {} yaw {} pitch {} centre zone {}",
            s0.view,
            fmt3(degrees(s0.yaw)),
            fmt3(degrees(s0.pitch)),
            s0.zone
        ));
        self.clock += 100.0;
        let mut prev = s0;
        for n in 0..passes {
            if self.pass_no.is_multiple_of(4) {
                self.e.tick();
            }
            self.pass_no += 1;
            if let Some(dt) = clicks.get(&n) {
                self.clock += dt.unwrap_or(10.0);
                self.e.click(self.clock);
            }
            self.e.pass(None);
            let s = self.state();
            let mut changes = Vec::new();
            if s.loop_type != prev.loop_type {
                changes.push(format!("loop={}", s.loop_type));
            }
            if s.view != prev.view {
                changes.push(format!("view={}", s.view));
            }
            if s.turn != prev.turn {
                changes.push(format!("turn={}", i32::from(s.turn)));
            }
            if s.zone != prev.zone {
                changes.push(format!("zone={}", s.zone));
            }
            for (k, name) in ["ARRIVEEVUE", "LASTVUE", "VUE"].iter().enumerate() {
                if s.vars[k] != prev.vars[k] {
                    changes.push(format!("{name}={}", s.vars[k]));
                }
            }
            let evs = self.events();
            let moved = (degrees(s.yaw) - degrees(prev.yaw)).abs() > 1e-4
                || (degrees(s.pitch) - degrees(prev.pitch)).abs() > 1e-4;
            if !changes.is_empty() || !evs.is_empty() || moved {
                let ang = if moved || changes.iter().any(|c| c.starts_with("view")) {
                    format!(
                        " yaw {} pitch {}",
                        fmt3(degrees(s.yaw)),
                        fmt3(degrees(s.pitch))
                    )
                } else {
                    String::new()
                };
                let mut row = format!("p{n} t{}{ang} {}", self.e.ticks, changes.join(" "));
                if !evs.is_empty() {
                    row += &format!(" | {}", evs.join("; "));
                }
                out.push(row.trim_end().to_string());
            }
            prev = s;
        }
        let se = self.state();
        out.push(format!(
            "end view {} yaw {} pitch {} loop {} centre zone {}",
            se.view,
            fmt3(degrees(se.yaw)),
            fmt3(degrees(se.pitch)),
            se.loop_type,
            se.zone
        ));
    }

    fn aim(&self, view: u32, zone: usize) -> (f64, f64) {
        let (y, p) = self.e.zone_aim(view, zone).unwrap();
        (degrees(y), degrees(p))
    }
}

fn clicks(list: &[(u32, Option<f64>)]) -> BTreeMap<u32, Option<f64>> {
    list.iter().copied().collect()
}

/// The golden's sections: (header, rows).
fn sections() -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![(String::new(), Vec::new())];
    for line in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        if let Some(h) = line.strip_prefix("== ") {
            out.push((h.to_string(), Vec::new()));
        } else {
            out.last_mut().unwrap().1.push(line.to_string());
        }
    }
    out
}

fn compare(name: &str, got: &[String], want: &[String]) {
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(g, w, "case {name}, row {i}");
    }
    assert_eq!(got.len(), want.len(), "case {name}: row count");
}

#[test]
fn moves_match_the_original() {
    let Some(files) = game() else { return };
    let mut d = Driver::new(files);
    let secs = sections();
    // The zones, their aims and the destinations.
    let mut head = Vec::new();
    for vid in 2..=10 {
        let view = d.e.wam().view(vid).unwrap().clone();
        for (i, z) in view.zones.iter().enumerate() {
            let (y, p) = d.aim(vid, i);
            head.push(format!(
                "zone v{vid} z{i} type {} value {} aim {},{}",
                z.kind,
                z.value,
                fmt3(y),
                fmt3(p)
            ));
        }
    }
    for vid in 2..=10 {
        for dst in &d.e.wam().view(vid).unwrap().destinations {
            head.push(format!(
                "dest v{vid}->{} flag {} start {:.4},{:.4} end {:.4},{:.4}",
                dst.target,
                dst.angles_as_given,
                dst.start_yaw,
                dst.start_pitch,
                dst.end_yaw,
                dst.end_pitch
            ));
        }
    }
    compare("header", &head, &secs[0].1);
    let mut got = BTreeMap::new();
    let mut run = |d: &mut Driver, name: &str, c: &[(u32, Option<f64>)], passes: u32| {
        let mut rows = Vec::new();
        d.run(&clicks(c), passes, &mut rows);
        got.insert(name.to_string(), rows);
    };
    run(&mut d, "A", &[(2, None)], 12);
    d.enter_at(2, -75.0, 15.0);
    run(&mut d, "B", &[(2, None)], 400);
    d.enter_at(3, -95.0, 3.0);
    run(&mut d, "C", &[(2, None)], 60);
    d.enter_at(3, 80.0, 0.0);
    run(&mut d, "D", &[(2, None), (6, Some(0.15))], 120);
    let (y, p) = d.aim(6, 0);
    d.enter_at(6, y, p);
    run(&mut d, "E", &[(2, None)], 40);
    let (y, p) = d.aim(9, 1);
    d.enter_at(9, y - 30.0, p);
    run(&mut d, "F", &[(2, None)], 120);
    let (y, p) = d.aim(4, 2);
    d.enter_at(4, y, p);
    run(&mut d, "G", &[(2, None)], 6);
    let (y, p) = d.aim(10, 1);
    d.enter_at(10, y, p);
    run(&mut d, "H", &[(2, None)], 6);
    d.enter_at(10, 0.0, -60.0);
    run(&mut d, "I", &[(2, None)], 6);
    for (name, rows) in &secs[1..] {
        if let Some(g) = got.get(name) {
            compare(name, g, rows);
        }
    }
    assert_eq!(got.len(), 9);
    // S: every move zone clicked at its aim from a fresh entry.
    let want = &secs.iter().find(|(n, _)| n == "S").unwrap().1;
    let mut sweep = Vec::new();
    for vid in 2..=10 {
        let zones = d.e.wam().view(vid).unwrap().zones.clone();
        for (zi, z) in zones.iter().enumerate() {
            if z.kind != 1 {
                continue;
            }
            let (y, p) = d.aim(vid, zi);
            d.enter_at(vid, y, p);
            let mut rows = Vec::new();
            let before = d.e.events.len();
            assert_eq!(before, 0);
            d.run(&clicks(&[(2, None)]), 400, &mut rows);
            let turn = rows
                .iter()
                .filter(|r| r.starts_with('p') && r.contains(" yaw ") && !r.contains("view="))
                .count();
            let all = rows.join("\n");
            let video = if all.contains("video missing") {
                "missing".to_string()
            } else {
                all.split("video ")
                    .nth(1)
                    .map(|s| {
                        s.split(|c: char| !c.is_ascii_digit())
                            .next()
                            .unwrap()
                            .to_string()
                    })
                    .unwrap_or_else(|| "none".into())
            };
            let frames = all.matches("frame ").count();
            sweep.push(format!(
                "sweep v{vid} z{zi} turn_passes {turn} video {video} frames {frames} | {}",
                rows.last().unwrap()
            ));
        }
    }
    compare("S", &sweep, want);
}

#[test]
fn animations_match_the_original() {
    let secs = sections();
    for vid in [7u32, 10] {
        let Some(files) = game() else { return };
        let mut d = Driver::new(files);
        d.enter_at(vid, 0.0, 0.0);
        let (_, want) = secs
            .iter()
            .find(|(n, _)| n.starts_with(&format!("animation view {vid}:")))
            .unwrap();
        let mut rows = Vec::new();
        for (n, s) in d.e.anim.slots.iter().enumerate() {
            rows.push(format!(
                "slot {n} id {} kind {} anim {} weight {} click {} active {} frames {} start {} end {} chain {} handing {}",
                s.id,
                s.kind,
                s.state.anim,
                s.state.weight,
                s.click,
                i32::from(s.active),
                s.count,
                s.start,
                s.end,
                s.chain,
                i32::from(s.handing)
            ));
        }
        let ticks = want.iter().filter(|r| r.starts_with("tick")).count();
        for t in 0..ticks {
            d.e.anim.step();
            let active: Vec<String> =
                d.e.anim
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.active)
                    .map(|(n, s)| {
                        let h = if s.handing {
                            format!("h{}", s.chain)
                        } else {
                            String::new()
                        };
                        format!("{n}:{}/{}{h}", s.frame, s.end)
                    })
                    .collect();
            rows.push(format!("tick {t} {}", active.join(" ")));
        }
        compare(&format!("animation {vid}"), &rows, want);
    }
}

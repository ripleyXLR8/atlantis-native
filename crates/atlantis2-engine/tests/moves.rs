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
use atlantis2_engine::crt::Rand;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const GOLDEN: &str = include_str!("golden/moves_tibet.txt");

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// Radians to degrees and back in double precision, as the golden's
/// angles were converted.
fn degrees(r: f32) -> f64 {
    f64::from(r) / (std::f64::consts::PI / 180.0)
}

fn radians(d: f64) -> f32 {
    (d * (std::f64::consts::PI / 180.0)) as f32
}

fn fmt3(x: f64) -> String {
    format!("{x:.3}")
}

/// Drives the engine as the golden's cases were run: a 15 Hz tick every
/// fourth pass, clicks at given passes, one row per pass that changed
/// something.
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
        e.rand = Rand::new(1000);
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
                Event::DepartScan(v) => out.push(format!("depart {}", v.departvue)),
                Event::ClickScan(v) => out.push(format!("clickscan {}", v.cliczone)),
                Event::ArrivalScan(v) => out.push(format!("arrive {}", v.arriveevue)),
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
                Event::PuzzleHook { zone, .. } => out.push(format!("hook {zone}")),
                Event::Take { id } => out.push(format!("take {id}")),
                // The scripts do not run here (the golden's lines are left
                // out).
                _ => {}
            }
        }
        if let Some(n) = spw {
            out.push(format!("enter spw={n}"));
        }
        out
    }

    /// Runs `passes` passes, the clicks at their pass (seconds
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
    // Every case run is compared, and every golden case of that kind run.
    let cases: Vec<&(String, Vec<String>)> = secs[1..]
        .iter()
        .filter(|(n, _)| n.len() == 1 && n != "S")
        .collect();
    assert_eq!(cases.len(), got.len(), "the golden's cases");
    for (name, rows) in cases {
        let g = got
            .get(name)
            .unwrap_or_else(|| panic!("case {name} not run"));
        compare(name, g, rows);
    }
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
            d.e.anim.step(&mut d.e.rand);
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

const ZOOM: &str = include_str!("golden/scripts_worlds_zoom.txt");

/// `%.9g`, as the golden prints the fields of view.
fn g9(v: f32) -> String {
    let v = f64::from(v);
    if v == 0.0 {
        return "0".into();
    }
    let e = v.abs().log10().floor() as i32;
    let decimals = (8 - e).max(0) as usize;
    let mut t = format!("{v:.decimals$}");
    if t.contains('.') {
        t = t.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    t
}

/// `ZoomStart=100` before the move of case B (view 2 at (−75°, 15°), a
/// click on the move to view 3 at pass 2), under 0x0e7f and 0x027f,
/// against the original (`scripts_worlds_zoom.txt`, its `ZoomStart`
/// sections): the field of view one degree less per frame during the
/// turn, the move held until the zoom ends; under 0x0e7f it lands on 35
/// and stays there, under 0x027f it is reset to 76. One row per pass that
/// changed the field of view, the target, the turn, the loop or the view.
/// `ZoomPsychoStart=180,3` the same way: the field of view 1.8° less per
/// frame towards 3°, the camera rolled by −0.16 rad a frame instead of
/// turning, the roll undone and the field of view back to 76 at the end,
/// under both control words.
#[test]
fn the_zoom_of_a_move_matches_the_original() {
    if game().is_none() {
        return;
    }
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    for line in ZOOM.lines().filter(|l| !l.starts_with('#')) {
        if let Some(h) = line.strip_prefix("== ") {
            sections.push((h.to_string(), Vec::new()));
        } else {
            sections.last_mut().unwrap().1.push(line.to_string());
        }
    }
    let mut checked = 0;
    for (head, want) in &sections {
        let psycho = head.contains("ZoomPsychoStart=180,3");
        assert!(psycho || head.contains("ZoomStart=100"), "{head}");
        let (variant, fpu) = if head.starts_with("cw 0x0e7f") {
            (Variant::PII, Fpu::Chop)
        } else {
            assert!(head.starts_with("cw 0x027f"), "{head}");
            (Variant::C, Fpu::Double)
        };
        let mut e = Engine::new_game(game().unwrap(), variant, fpu, 640, 480).unwrap();
        e.sprite_picks = false;
        e.rand = Rand::new(1000);
        e.enter_at(2, radians(-75.0), radians(15.0)).unwrap();
        let f = e.fpu();
        if psycho {
            e.zoom.psycho_start(f, 180.0, 3.0);
        } else {
            e.zoom.start(f, 100.0, 35.0);
        }
        let mut got = vec![format!(
            "set: fov {:.4} target {:.4} step {:.4} roll {:.4}/{:.4}",
            e.zoom.current, e.zoom.target, e.zoom.step, e.zoom.roll, e.zoom.roll_step
        )];
        let mut prev = None;
        let mut clock = 100.0;
        for n in 0..160u64 {
            if n % 4 == 0 {
                e.tick();
            }
            if n == 2 {
                clock += 10.0;
                e.click(clock);
            }
            e.pass(None);
            let cur = (
                e.zoom.current,
                e.zoom.target,
                e.zoom.roll,
                e.turning(),
                i32::from(e.loop_type == LoopType::Video),
                e.place().1,
            );
            if Some(cur) != prev {
                let (yaw, pitch) = e.camera.angles();
                got.push(format!(
                    "p{n} fov {} target {} roll {:.4} turn {} loop {} view {} yaw {} pitch {}",
                    g9(cur.0),
                    g9(cur.1),
                    cur.2,
                    i32::from(cur.3),
                    cur.4,
                    cur.5,
                    fmt3(degrees(yaw)),
                    fmt3(degrees(pitch))
                ));
            }
            prev = Some(cur);
        }
        compare(head, &got, want);
        checked += 1;
    }
    assert_eq!(checked, 4);
}

const RING: &str = include_str!("golden/moves_ring.txt");

/// The ring's moves against the original (`moves_ring.txt`): in
/// `annulair.wam` view 1, chapter 81, a click on the move to view 16 at its
/// aim and 8° off it runs chapter 81's departure section
/// (`ZoomPsychoStart=180,3`): the field of view 1.8° less a frame towards
/// 3°, the camera rolled by −0.16 rad a frame with its direction kept (no
/// turn to the zone), then view 16 at 76° with no roll, under both control
/// words; one row per pass that changed the field of view, its target, the
/// roll, the turn, the loop or the view.
#[test]
fn the_rings_rolling_moves_match_the_original() {
    if game().is_none() {
        return;
    }
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    for line in RING.lines().filter(|l| !l.starts_with('#')) {
        if let Some(h) = line.strip_prefix("== ") {
            sections.push((h.to_string(), Vec::new()));
        } else if !line.starts_with("chapi081 ") {
            sections.last_mut().unwrap().1.push(line.to_string());
        }
    }
    assert_eq!(sections.len(), 4);
    for (head, want) in &sections {
        let (variant, fpu) = if head.starts_with("cw 0x0e7f") {
            (Variant::PII, Fpu::Chop)
        } else {
            assert!(head.starts_with("cw 0x027f"), "{head}");
            (Variant::C, Fpu::Double)
        };
        let words: Vec<&str> = head.split_whitespace().collect();
        let zi: usize = words[6].parse().unwrap();
        let mut e = Engine::boot(game().unwrap(), variant, fpu, 640, 480).unwrap();
        e.start_new_game().unwrap();
        e.end_video();
        for c in [
            "(monde=4)",
            "(changewam=annulair.wam)",
            "(vue=1)",
            "(chapter=81)",
        ] {
            e.run_command(c.as_bytes());
        }
        for _ in 0..400 {
            if e.loop_type == LoopType::Video {
                e.end_video();
            }
            e.tick();
            e.pass(None);
            if e.loop_type == LoopType::Warp && !e.turning() {
                break;
            }
        }
        assert_eq!(e.place(), ("annulair.wam", 1));
        let z = &e.wam().view(1).unwrap().zones[zi];
        assert_eq!((z.kind, z.value), (1, 16), "{head}");
        let (ay, ap) = e.zone_aim(1, zi).unwrap();
        let aim = format!("aim {},{}", fmt3(degrees(ay)), fmt3(degrees(ap)));
        assert!(head.contains(&aim), "{head}: {aim}");
        // The view entered at the original's own arrival angles (its
        // f32 bits), so that the rows follow the roll alone.
        let bits: Vec<u32> = want[0]
            .split(|c: char| !c.is_ascii_hexdigit())
            .filter(|t| t.len() == 8)
            .map(|t| u32::from_str_radix(t, 16).unwrap())
            .collect();
        e.sprite_picks = false;
        e.seed_rand(1000);
        // As the harness: the start-up camera, the view entered by
        // `(vue=1)` at (0, 0) (`ANGLEWARPX` / `ANGLEWARPY` = 0), then at
        // the arrival angles (the camera's residues follow its history).
        e.camera = atlantis2_engine::Camera::new(fpu, 640, 480);
        e.enter_at(1, 0.0, 0.0).unwrap();
        e.enter_at(1, f32::from_bits(bits[2]), f32::from_bits(bits[3]))
            .unwrap();
        let mut got = vec![format!(
            "aim bits {:08x} {:08x}; entered at bits {:08x} {:08x}",
            ay.to_bits(),
            ap.to_bits(),
            bits[2],
            bits[3]
        )];
        got.push(format!(
            "set: fov {:.4} target {:.4} step {:.4} roll {:.4}/{:.4}",
            e.zoom.current, e.zoom.target, e.zoom.step, e.zoom.roll, e.zoom.roll_step
        ));
        let mut prev = None;
        let mut clock = 100.0;
        for n in 0..240u64 {
            if n % 4 == 0 {
                e.tick();
            }
            if n == 2 {
                clock += 10.0;
                e.click(clock);
            }
            e.pass(None);
            let cur = (
                format!("{:.4}", e.zoom.current),
                format!("{:.4}", e.zoom.target),
                format!("{:.4}", e.zoom.roll),
                e.turning(),
                i32::from(e.loop_type == LoopType::Video),
                e.place().1,
            );
            if Some(&cur) != prev.as_ref() {
                let (yaw, pitch) = e.camera.angles();
                got.push(format!(
                    "p{n} fov {} target {} roll {} step {:.4} turn {} loop {} view {} yaw {} pitch {}",
                    g9(e.zoom.current),
                    g9(e.zoom.target),
                    cur.2,
                    e.zoom.roll_step,
                    i32::from(cur.3),
                    cur.4,
                    cur.5,
                    fmt3(degrees(yaw)),
                    fmt3(degrees(pitch))
                ));
            }
            prev = Some(cur);
        }
        compare(head, &got, want);
    }
}

//! Moves to another place and the transition sprites against the original
//! (`tests/golden/links.txt`, atlantis2-re `tools/emu/links.py`: the
//! original's frame code under emulation, as `moves_tibet.txt`'s): a move
//! with two transition sprite banks drawn over its video (`bigile1.wam`
//! 7 → 8), the same with one character hidden by the show / hide table,
//! a type-2 zone that changes place at once (`livre.wam` 55 →
//! `Bigile1.wam` 5) and two deferred ones, one with a video (`chine.wam`
//! 32 → `salletortue.wam` 40, `miniature.wam` 123 → `labysol.wam` 3). The
//! same clicks at the same passes give the same turns, view entries,
//! place loads, sprite loads and draws, videos and scans.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::A2Files;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const GOLDEN: &str = include_str!("golden/links.txt");

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

fn fmt3(x: f64) -> String {
    format!("{x:.3}")
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

fn state(e: &Engine) -> State {
    let (yaw, pitch) = e.camera.angles();
    State {
        loop_type: i32::from(e.loop_type == LoopType::Video),
        view: e.place().1,
        turn: e.turning(),
        zone: e.centre_index(),
        vars: [e.vars.arrivee_vue, e.vars.last_vue, e.vars.vue],
        yaw,
        pitch,
    }
}

/// The events of a pass in the golden's form (`links.py --native`).
fn events(e: &mut Engine) -> Vec<String> {
    let mut out = Vec::new();
    let mut spw = None;
    let evs: Vec<Event> = std::mem::take(&mut e.events);
    for ev in evs {
        if let Some(n) = spw {
            // The entry's puzzle hooks (the labyrinth's) load SPW files
            // of their own: counted with the view's.
            if !matches!(ev, Event::SpwLoad { .. } | Event::PuzzleRan { .. }) {
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
            Event::LinkMove { wam, view, .. } => out.push(format!("link {wam} {view}")),
            Event::Wam(name) => out.push(format!("wam {name}")),
            Event::SpfLoad { name, found } => {
                out.push(format!("spf {name}{}", if found { "" } else { "=missing" }))
            }
            Event::VideoStart { frames, .. } => out.push(match frames {
                Some(n) => format!("video {n}"),
                None => "video missing".into(),
            }),
            Event::VideoFrame(k) => out.push(format!("frame {k}")),
            Event::SpfDraw { bank, frame } => out.push(format!("spfdraw {bank} {frame}")),
            Event::VideoEnd => out.push("close".into()),
            _ => {}
        }
    }
    if let Some(n) = spw {
        out.push(format!("enter spw={n}"));
    }
    out
}

/// One case of the harness (`links.py`'s `case`).
struct Case {
    name: &'static str,
    wam: &'static str,
    view: u32,
    kind: i32,
    value: i32,
    passes: u32,
    hide: Option<&'static str>,
    off: f64,
}

/// One case as the harness runs it: the place loaded, the view entered at
/// the zone's aim (plus `off` degrees of yaw), a click at pass 2, a tick
/// every fourth pass.
fn run_case(files: A2Files, c: &Case) -> Vec<String> {
    let (wam, view, kind, value, passes, hide, off) =
        (c.wam, c.view, c.kind, c.value, c.passes, c.hide, c.off);
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
    e.trace = true;
    e.sprite_picks = false;
    e.rand = Rand::new(1000);
    e.load_place(wam);
    e.enter_at(view, 0.0, 0.0).unwrap();
    let zones = e.wam().view(view).unwrap().zones.clone();
    let z = zones
        .iter()
        .position(|z| z.kind == kind && z.value == value)
        .unwrap();
    let (y, p) = e.zone_aim(view, z).unwrap();
    e.enter_at(view, radians(degrees(y) + off), radians(degrees(p)))
        .unwrap();
    if let Some(name) = hide {
        let dn = defnames::DefNames::parse(
            &e.files()
                .read(atlantis2_data::Folder::Scenar, "defnames.con")
                .unwrap(),
        )
        .unwrap();
        let id = dn.id(defnames::PERSONNAGES, name.as_bytes()).unwrap() as i32;
        e.anim.table.set(id, 0, 1);
    }
    e.events.clear();
    let mut out = Vec::new();
    let s0 = state(&e);
    out.push(format!(
        "start view {} yaw {} pitch {} centre zone {}",
        s0.view,
        fmt3(degrees(s0.yaw)),
        fmt3(degrees(s0.pitch)),
        s0.zone
    ));
    let mut prev = s0;
    for n in 0..passes {
        if n % 4 == 0 {
            e.tick();
        }
        if n == 2 {
            e.click(100.0);
        }
        e.pass(None);
        let s = state(&e);
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
        let evs = events(&mut e);
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
            let mut row = format!("p{n} t{}{ang} {}", e.ticks, changes.join(" "));
            if !evs.is_empty() {
                row += &format!(" | {}", evs.join("; "));
            }
            out.push(row.trim_end().to_string());
        }
        prev = s;
    }
    let se = state(&e);
    out.push(format!(
        "end view {} yaw {} pitch {} loop {} centre zone {}",
        se.view,
        fmt3(degrees(se.yaw)),
        fmt3(degrees(se.pitch)),
        se.loop_type,
        se.zone
    ));
    out
}

fn section(head: &str) -> Vec<String> {
    GOLDEN
        .lines()
        .skip_while(|l| !l.starts_with(&format!("== {head}")))
        .skip(1)
        .take_while(|l| !l.starts_with("=="))
        .map(str::to_string)
        .collect()
}

fn compare(name: &str, got: &[String], want: &[String]) {
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(g, w, "case {name}, row {i}");
    }
    assert_eq!(got.len(), want.len(), "case {name}: row count");
}

/// L44: the transition sprites (FUN_0047c100, FUN_0047c200, FUN_0047c240),
/// the type-2 zones and the place changes, at once and deferred
/// (FUN_0049cbf0, FUN_0040fc60, the completion in FUN_0045a3b0).
#[test]
fn links_and_transition_sprites_match_the_original() {
    let Some(files) = game() else { return };
    let case = |name, wam, view, kind, value, passes, hide, off| Case {
        name,
        wam,
        view,
        kind,
        value,
        passes,
        hide,
        off,
    };
    let cases = [
        case("T", "bigile1.wam", 7, 1, 8, 300, None, 0.0),
        case("U", "bigile1.wam", 7, 1, 8, 300, Some("LIAM_CELLULE"), 0.0),
        case("L", "livre.wam", 55, 2, 5, 40, None, 0.0),
        case("M", "chine.wam", 32, 2, 40, 200, None, 0.2),
        case("N", "miniature.wam", 123, 2, 3, 400, None, 0.2),
    ];
    let mut files = Some(files);
    for c in &cases {
        let f = files.take().unwrap_or_else(|| game().unwrap());
        let got = run_case(f, c);
        compare(c.name, &got, &section(c.name));
    }
}

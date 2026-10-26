//! The place sounds against the original's emulated runs
//! (`tests/golden/place_sounds.txt`, header there): the ears' gains and
//! the voice's volume and pan (FUN_004594c0, FUN_0047b390) under the three
//! x87 control words, eight slots, a sound's start (FUN_0047a650), the
//! listener and the fade along a video (FUN_0047b030, FUN_0047b090), the
//! scheduler (FUN_0047b4d0); with the game's files, every bruitage of the
//! places from the executable's listener table; and Ireland's sounds
//! played by the engine.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::path::PathBuf;

use atlantis2_data::{A2Files, Folder, Wam};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::place_sound::{
    draw, gains, pan_volume, sources, transition_factor, Listeners, FIRST_VOICE,
};
use atlantis2_engine::sound::attenuation_table;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const GOLDEN: &str = include_str!("golden/place_sounds.txt");

fn rows(cw: &str) -> HashMap<String, String> {
    GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let (c, rest) = l.split_once(' ')?;
            let (k, v) = rest.split_once(' ')?;
            (c == cw).then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

fn fpu(cw: &str) -> Fpu {
    match cw {
        "0e7f" => Fpu::Chop,
        "027f" => Fpu::Double,
        _ => Fpu::Single,
    }
}

fn hx(v: f32) -> String {
    format!("{:08x}", v.to_bits())
}

#[test]
fn gains_and_levels_match_the_original() {
    let att = attenuation_table();
    let srcs: [[f32; 3]; 7] = [
        [1000.0, 0.0, 0.0],
        [0.0, 1000.0, 0.0],
        [-300.0, 400.0, 100.0],
        [50.0, -20.0, 400.0],
        [2500.0, -3600.0, -322.0],
        [0.0, 0.0, 0.0],
        [5.0, 3.0, 1.0],
    ];
    let qs: [[f32; 3]; 4] = [
        [0.0, 100.0, 0.0],
        [0.5, 100.0, 0.0],
        [1.0, 50.0, 3.7],
        [0.25, 7.5, -2.0],
    ];
    let mut n = 0;
    for cw in ["0e7f", "027f", "007f"] {
        let f = fpu(cw);
        let rows = rows(cw);
        for (k, want) in &rows {
            let Some(rest) = k.strip_prefix("grid.") else {
                continue;
            };
            // fx127.m256.s0.q0.v400.p0.0.y0
            let p: Vec<&str> = rest.split('.').collect();
            let fx: i32 = p[0][2..].parse().unwrap();
            let master: i32 = p[1][1..].parse().unwrap();
            let si: usize = p[2][1..].parse().unwrap();
            let qi: usize = p[3][1..].parse().unwrap();
            let level: f32 = p[4][1..].parse().unwrap();
            let pitch: f64 = format!("{}.{}", &p[5][1..], p[6]).parse().unwrap();
            let yi: i32 = p[7][1..].parse().unwrap();
            let yaw = (f64::from(yi - 8) * PI / 8.0) as f32;
            let g = gains(f, srcs[si], level, [0.0; 3], qs[qi], yaw, pitch as f32);
            let (a, pan) = pan_volume(f, &att, master, fx, g.0, g.1, None, true);
            assert_eq!(
                &format!("g={},{},{} set={a},{pan}", g.0, g.1, g.2),
                want,
                "{cw} {k}"
            );
            n += 1;
        }
        // Eight slots at once.
        for i in 0..8 {
            let fi = i as f32;
            let g = gains(
                f,
                [100.0 * fi - 300.0, 50.0 * fi, 10.0],
                300.0 + 20.0 * fi,
                [0.0; 3],
                [0.0, 100.0, 0.0],
                0.3,
                0.1,
            );
            let (a, pan) = pan_volume(f, &att, 256, 127, g.0, g.1, None, true);
            assert_eq!(
                format!("g={},{},{} set={a},{pan}", g.0, g.1, g.2),
                rows[&format!("eight.slot{i}")],
                "{cw} eight {i}"
            );
        }
        // A sound's start: from the gains of the source (now, cleared, or
        // others), the level halved before its truncation; a sound with no
        // position at the effects volume, centred.
        let g = gains(
            f,
            [1000.0, 0.0, 0.0],
            500.0,
            [0.0; 3],
            [0.0, 100.0, 0.0],
            0.0,
            0.0,
        );
        for (tag, (g0, g1)) in [("now", (g.0, g.1)), ("0,0", (0, 0)), ("200,40", (200, 40))] {
            let (a, pan) = pan_volume(f, &att, 256, 127, g0, g1, None, false);
            assert_eq!(
                format!("voice=4 loop=0 att={a} pan={pan}"),
                rows[&format!("firstplay.positioned.g{tag}")]
            );
            assert_eq!(
                format!("voice=4 loop=0 att={} pan=0", att[127]),
                rows[&format!("firstplay.plain.g{tag}")]
            );
        }
        // Along a video: the listener's steps, the fade of a source the
        // new view does not hear.
        for inview in [true, false] {
            for frames in [7u32, 20] {
                let tag = format!(
                    "trans.{}.n{frames}",
                    if inview { "inview" } else { "leaving" }
                );
                let mut cur = [-500.0f32, 300.0, 20.0];
                let tgt = [800.0f32, -100.0, 60.0];
                let nf = frames as f32;
                let step: Vec<f32> = (0..3)
                    .map(|i| {
                        f.f32(f.div(f.sub(f64::from(tgt[i]), f64::from(cur[i])), f64::from(nf)))
                    })
                    .collect();
                assert_eq!(
                    format!(
                        "step={} n={}",
                        step.iter().map(|&v| hx(v)).collect::<Vec<_>>().join(","),
                        hx(nf)
                    ),
                    rows[&format!("{tag}.init")]
                );
                let mut count = 0.0f32;
                for k in 0..frames + 2 {
                    let g = gains(
                        f,
                        [1000.0, 200.0, 0.0],
                        500.0,
                        cur,
                        [0.0, 100.0, 0.0],
                        0.2,
                        0.0,
                    );
                    let fade = if inview {
                        2.0
                    } else {
                        transition_factor(f, f64::from(nf), f64::from(count))
                    };
                    let (a, pan) = pan_volume(f, &att, 256, 127, g.0, g.1, Some(fade), true);
                    let lis: Vec<String> = cur.iter().map(|&v| hx(v)).collect();
                    let next = f.f32(f.add(f64::from(count), 1.0));
                    assert_eq!(
                        format!(
                            "listener={} g={},{},{} set={a},{pan} count={}",
                            lis.join(","),
                            g.0,
                            g.1,
                            g.2,
                            hx(next)
                        ),
                        rows[&format!("{tag}.frame{k}")],
                        "{cw} {tag} {k}"
                    );
                    for i in 0..3 {
                        cur[i] = f.f32(f.add(f64::from(cur[i]), f64::from(step[i])));
                    }
                    count = next;
                }
            }
        }
    }
    assert!(n > 2000, "{n}");
}

#[test]
fn the_scheduler_matches_the_original() {
    let rows = rows("any");
    let mut n = 0;
    for (k, want) in &rows {
        let Some(rest) = k.strip_prefix("sched.") else {
            continue;
        };
        let (hex, seed) = rest.split_once(".seed").unwrap();
        let probs: Vec<i8> = (0..4)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap() as i8)
            .collect();
        let probs = [probs[0], probs[1], probs[2], probs[3]];
        let loaded = [0, 1, 2, 3].map(|i| probs[i] != 0 || i == 0);
        let mut r = Rand::new(seed.parse().unwrap());
        let mut timer = 0;
        let mut evts = Vec::new();
        for tick in 1..400 {
            // FUN_0047b4d0 on a slot that never plays long: due, or waiting.
            if timer == 0 {
                let (pick, t) = draw(&mut r, probs, loaded, tick);
                timer = t;
                if let Some(p) = pick {
                    evts.push(format!("{tick}:{p}:{}", u8::from(probs[0] == 100)));
                }
            } else if timer < tick {
                timer = 0;
            }
        }
        let got = format!(
            "timer={timer} {}",
            if evts.is_empty() {
                "none".to_string()
            } else {
                evts.join(" ")
            }
        );
        assert_eq!(&got, want, "{k}");
        n += 1;
    }
    assert_eq!(n, 14);
}

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

#[test]
fn the_listener_table_of_each_build() {
    let Some(files) = game() else { return };
    let cd = Listeners::from_exe(&files.read_exe().unwrap(), &files.build().place_sounds);
    // Ten places have a listener; Tibet is not one of them.
    assert!(cd.place("xibalba.wam").is_some());
    assert!(cd.place("tibet.wam").is_none());
    // GOG's Game.exe (1.01): the same table at its own addresses.
    let Some(gog) = std::env::var_os("ATLANTIS2_GOG") else {
        eprintln!("ATLANTIS2_GOG not set: GOG part skipped");
        return;
    };
    let (g, build) = A2Files::open_dir(&PathBuf::from(gog), &[]).expect("GOG folder");
    assert_eq!(build.id, "atlantis2-gog-fr-2183168");
    let gl = Listeners::from_exe(&g.read_exe().unwrap(), &build.place_sounds);
    assert_eq!(gl, cd);
    // Read at the reference's addresses, 1.01's bytes are something else.
    let wrong = Listeners::from_exe(
        &g.read_exe().unwrap(),
        &atlantis2_data::exe::KNOWN_BUILDS[0].place_sounds,
    );
    assert_ne!(wrong, cd);
}

#[test]
fn every_bruitage_of_the_places_matches_the_original() {
    let Some(files) = game() else { return };
    let exe = files.read_exe().unwrap();
    let listeners = Listeners::from_exe(&exe, &files.build().place_sounds);
    let att = attenuation_table();
    for cw in ["0e7f", "027f", "007f"] {
        let f = fpu(cw);
        let mut n = 0;
        for (k, want) in rows(cw) {
            let Some(rest) = k.strip_prefix("data.") else {
                continue;
            };
            // <wam>.v<view>.id<object>
            let (wam_name, tail) = rest.split_once(".wam.v").unwrap();
            let wam_name = format!("{wam_name}.wam");
            let (view, id) = tail.split_once(".id").unwrap();
            let (view, id): (u32, u32) = (view.parse().unwrap(), id.parse().unwrap());
            let wam = Wam::parse(&files.read(Folder::Wam, &wam_name).unwrap()).unwrap();
            let place = listeners.place(&wam_name).expect("a place of the table");
            let Some(listener) = listeners.position(place, view) else {
                assert_eq!(want, "skipped=nolistener");
                continue;
            };
            let srcs = sources(&wam);
            let src = srcs.iter().find(|s| s.id == id).unwrap();
            let q = wam
                .view(view)
                .unwrap()
                .sounds
                .iter()
                .find(|b| b.ambient_id == id)
                .unwrap()
                .position;
            let vals: Vec<String> = (0..8)
                .map(|kk| {
                    let yaw = (f64::from(kk) * PI / 4.0 - PI) as f32;
                    let g = gains(f, src.pos, src.level, listener, q, yaw, 0.0);
                    let (a, p) = pan_volume(f, &att, 256, 127, g.0, g.1, None, true);
                    format!("{a}/{p}")
                })
                .collect();
            assert_eq!(vals.join(";"), want, "{cw} {k}");
            n += 1;
        }
        assert_eq!(n, 193, "{cw}");
    }
}

#[test]
fn irelands_place_sounds_play() {
    let Some(files) = game() else { return };
    let mut e = Engine::new_game(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.trace = true;
    // Tibet: no source, no listener row.
    assert!(e.place.sources.is_empty());
    e.load_place("bigile1.wam");
    assert_eq!(e.place.sources.len(), 8);
    // View 9 hears two sources: two slots, their files loaded once each.
    e.enter_at(9, 0.0, 0.0).unwrap();
    let held: Vec<usize> = e.place.slots.iter().filter_map(|s| s.source).collect();
    assert_eq!(held.len(), 2);
    assert_ne!(e.place.target, [0.0; 3]);
    // The panorama frames: the schedulers draw; a sound starts silent on
    // its voice, then gets the level of its gains.
    let mut started = Vec::new();
    for _ in 0..2000 {
        e.tick();
        e.pass(None);
        for ev in std::mem::take(&mut e.events) {
            if let Event::PlaceSound { slot, name } = ev {
                started.push((slot, name));
            }
        }
        if !started.is_empty() {
            break;
        }
    }
    let (slot, _) = started.first().cloned().expect("a place sound");
    let voice = FIRST_VOICE + slot;
    assert_eq!(e.voices().levels(voice), Some((-10000, 10000)));
    e.pass(None);
    let (att, _) = e.voices().levels(voice).unwrap();
    assert!(att > -10000, "{att}");
    // A move without a video keeps the slots.
    let before = e.place.slots.clone();
    e.enter_at(8, 0.0, 0.0).unwrap();
    for (a, b) in e.place.slots.iter().zip(&before) {
        if b.source.is_some() {
            assert_eq!(a.source, b.source);
        }
    }
    // A place file loaded frees them all and stops their voices.
    e.load_place("livre.wam");
    assert!(e.place.slots.iter().all(|s| s.source.is_none()));
    assert!(!e.voices().is_playing(voice));
    assert_eq!(e.loop_type, LoopType::Warp);
}

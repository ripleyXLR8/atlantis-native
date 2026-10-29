//! The two map screens against the original's code under emulation
//! (`tests/golden/map_carte.txt`, `tests/golden/map_tour.txt`):
//!
//! - `AtlCarte` (mode 9): the hit-test positions, the hover label and the
//!   click destination over an 80 × 60 grid of pointer positions, then per
//!   frame the refusals, the sprite calls in order, the screen's md5 (the
//!   panorama replaced by black), the return and the commands run;
//! - `AtlCarteTour` (mode 10): the board's squares, then per frame the
//!   pawns' animation frames, the sprite calls, the md5 and the return.
//!
//! Each at 640 × 480, 800 × 600 and 1024 × 768 (`map_carte_640.txt`,
//! `map_carte_1024.txt`, `map_tour_640.txt`, `map_tour_1024.txt`; the
//! pointer at the screen's centre).
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis2_engine::Fpu;
use atlantis3_data::spr::{Bank, Depth};
use atlantis3_data::A3Files;
use atlantis3_engine::inventory::Aff;
use atlantis3_engine::maps::{
    carte_click, carte_fishermen, carte_hover, carte_label, carte_place, carte_refusal, draw_carte,
    draw_tour, tour_scale, CarteRefusal, TourAnim, CARTE_CHAPTERS, TOUR_X, TOUR_Y,
};
use md5::{Digest, Md5};

const CARTE_640: &str = include_str!("golden/map_carte_640.txt");
const CARTE_800: &str = include_str!("golden/map_carte.txt");
const CARTE_1024: &str = include_str!("golden/map_carte_1024.txt");
const TOUR_640: &str = include_str!("golden/map_tour_640.txt");
const TOUR_800: &str = include_str!("golden/map_tour.txt");
const TOUR_1024: &str = include_str!("golden/map_tour_1024.txt");

fn md5(px: &[u16]) -> String {
    let b: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(&b).iter().map(|x| format!("{x:02x}")).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A double from its little-endian bytes in hex.
fn clock(s: &str) -> f64 {
    let b: Vec<u8> = (0..8)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    f64::from_le_bytes(b.try_into().unwrap())
}

/// A golden draw `spr\Name123.spr#F@X,Y*S` as `Name#F@X,Y*S` (the bank's
/// file name without its folder, its width digits and its extension);
/// none for the warp and the centred calls (the plain call that follows
/// them is the one compared).
fn golden_draw(d: &str) -> Option<String> {
    if d.starts_with("warp") || d.starts_with("C:") || d.is_empty() {
        return None;
    }
    let (bank, rest) = d.split_once('#').unwrap();
    let name = bank.rsplit('\\').next().unwrap();
    let name = &name[..name.len() - 4];
    let name = name.trim_end_matches(|c: char| c.is_ascii_digit());
    Some(format!("{name}#{rest}"))
}

/// The draws of a frame line, split at the commas that separate calls
/// (a piece starting with a digit or a minus sign is the y of the call
/// before it).
fn split_draws(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in s.split(',') {
        let cont = p.starts_with(|c: char| c.is_ascii_digit() || c == '-');
        match out.last_mut() {
            Some(last) if cont => {
                last.push(',');
                last.push_str(p);
            }
            _ => out.push(p.to_string()),
        }
    }
    out
}

fn port_draw(a: &Aff) -> String {
    assert_eq!(a.drawn, Some(None), "{a:?}: drawn with AFF_SPR");
    format!(
        "{}#{}@{},{}*{}",
        a.bank,
        a.index,
        a.x,
        a.y,
        hex(&a.alpha.to_bits().to_le_bytes())
    )
}

/// The value of the field `p…` of a split line.
fn field<'a>(f: &[&'a str], p: &str) -> Option<&'a str> {
    f.iter().find_map(|x| x.strip_prefix(p))
}

/// A frame line's fields.
struct Frame<'a> {
    case: &'a str,
    n: &'a str,
    t: f64,
    ptr: (i32, i32),
    left: bool,
    right: bool,
    ret: u8,
    md5: &'a str,
    warp: Option<bool>,
    draws: Vec<String>,
    anim: Option<(i32, i32)>,
}

fn frame(l: &str) -> Frame<'_> {
    let f: Vec<&str> = l.split(' ').collect();
    let val = |p: &str| field(&f, p).unwrap_or_else(|| panic!("{l}: no {p}"));
    let draws = split_draws(val("draws="));
    let anim = field(&f, "anim=").map(|a| {
        let (x, y) = a.split_once(',').unwrap();
        (x.parse().unwrap(), y.parse().unwrap())
    });
    Frame {
        case: f[1],
        n: f[2],
        t: clock(f[3]),
        ptr: (f[4].parse().unwrap(), f[5].parse().unwrap()),
        left: f[6] == "1",
        right: f[7] == "1",
        ret: val("ret=").parse().unwrap(),
        md5: val("md5="),
        warp: draws
            .iter()
            .find(|d| d.starts_with("warp"))
            .map(|d| d == "warp=flat"),
        draws: draws.iter().filter_map(|d| golden_draw(d)).collect(),
        anim,
    }
}

fn banks() -> Option<A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(A3Files::open_dir(&dir).unwrap().0)
}

fn bank(files: &A3Files, p: &str) -> Bank {
    Bank::parse(&files.read(p).unwrap(), Depth::Rgb565).unwrap()
}

/// The Land of the Dead map's golden of the `w` × `h` screen replayed.
fn carte(golden: &str, w: usize, h: usize) {
    let Some(files) = banks() else {
        return;
    };
    let carte = bank(&files, "spr/Carte.spr");
    let interface = bank(&files, "spr/Interface.spr");
    let lines: Vec<&str> = golden.lines().filter(|l| !l.starts_with('#')).collect();
    // The case's flags (the harness's set-up before each frame).
    let flags_of = |case: &str| -> Vec<(i32, i32)> {
        let mut v = vec![(80, 1)];
        if case.starts_with("fish_") {
            v.extend([(69, 1), (560, 0)]);
        }
        match case {
            "two_feathers" => v.extend([(535, 1), (536, 1)]),
            "two_feathers_one_on_balance" => v.extend([(535, 1), (536, 1), (562, 1)]),
            "gp_not_seen" => v[0] = (80, 0),
            _ => {}
        }
        v
    };
    let mut counts = [0usize; 6];
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        i += 1;
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "coll" => {
                let k: usize = f[2].parse().unwrap();
                let at = (f[3].parse().unwrap(), f[4].parse().unwrap());
                let port = if f[1] == "hover" {
                    carte_label(w, h, k)
                } else {
                    carte_place(w, h, k)
                };
                assert_eq!(port, at, "{l}");
                counts[0] += 1;
            }
            "hover" | "click" => {
                let y: i32 = f[1].parse().unwrap();
                for (j, want) in f[2..].iter().enumerate() {
                    let ptr = (10 * j as i32, y);
                    let port = if f[0] == "hover" {
                        carte_hover(&carte, w, h, ptr).map(|k| (k + 5).to_string())
                    } else {
                        carte_click(&carte, w, h, ptr).map(|k| CARTE_CHAPTERS[k - 1].to_string())
                    };
                    assert_eq!(port.as_deref().unwrap_or("-"), *want, "{} at {ptr:?}", f[0]);
                    counts[1] += 1;
                }
            }
            "frame" => {
                let fr = frame(l);
                let mut events: Vec<String> = Vec::new();
                while i < lines.len() && lines[i].starts_with("event ") {
                    let e = lines[i].splitn(4, ' ').nth(3).unwrap();
                    events.push(e.to_string());
                    i += 1;
                }
                let flags = flags_of(fr.case);
                let flag = |n: i32| flags.iter().rev().find(|f| f.0 == n).map_or(0, |f| f.1);
                let close = [
                    "AtlCarte_Close_0x004054d0".to_string(),
                    "CtrlScenarIN_0x0044a2f0".to_string(),
                ];
                let mut want_events: Vec<String> = Vec::new();
                let ret;
                if let Some(r) = carte_refusal(flag) {
                    // No drawing, no flip.
                    assert!(fr.draws.is_empty() && fr.md5 == "-", "{l}");
                    let state = lines[i];
                    assert!(state.starts_with("state "), "{state}");
                    let var = match r {
                        CarteRefusal::Feathers => "TROP_DE_PLUMES",
                        CarteRefusal::PriestNotSeen => "GP_PAS_VU",
                    };
                    assert_eq!(r.var(), var);
                    assert!(state.contains(&format!(" {var}=1 ")), "{state}");
                    ret = 0;
                    want_events.extend(close.clone());
                    counts[5] += 1;
                } else {
                    let mut screen = vec![0u16; w * h];
                    let calls = draw_carte(&mut screen, w, h, &carte, Some(&interface), fr.ptr);
                    let port: Vec<String> = calls.iter().map(port_draw).collect();
                    assert_eq!(port, fr.draws, "{} {}", fr.case, fr.n);
                    counts[2] += port.len();
                    if fr.warp == Some(true) {
                        assert_eq!(md5(&screen), fr.md5, "{} {}", fr.case, fr.n);
                        counts[3] += 1;
                    }
                    if fr.left {
                        match carte_click(&carte, w, h, fr.ptr) {
                            Some(k) => {
                                let chapter = CARTE_CHAPTERS[k - 1];
                                if carte_fishermen(chapter, flag(69), flag(560)) {
                                    for c in [
                                        "(HidePerso=LDM_PECHEUR01)",
                                        "(HidePerso=LDM_PECHEUR02)",
                                        "(HideObjet=LDM_POISSON2)",
                                    ] {
                                        want_events.push(format!("RunOneCommand '{c}'"));
                                    }
                                }
                                // The harness's current chapter is 12.
                                if chapter == 12 {
                                    ret = 1;
                                } else {
                                    want_events.push("RunOneCommand '(chapter=500)'".into());
                                    want_events.extend(close.clone());
                                    ret = 0;
                                    let state = lines[i];
                                    assert!(
                                        state.contains(&format!(
                                            "LDM_CHAPITRE_DEPART=12 LDM_CHAPITRE_ARRIVEE={chapter} "
                                        )),
                                        "{state}"
                                    );
                                }
                            }
                            None => ret = 1,
                        }
                    } else if fr.right {
                        want_events.extend(close.clone());
                        ret = 0;
                    } else {
                        ret = 1;
                    }
                }
                assert_eq!(ret, fr.ret, "{} {}: return", fr.case, fr.n);
                assert_eq!(want_events, events, "{} {}: events", fr.case, fr.n);
                counts[4] += 1;
            }
            _ => {}
        }
    }
    // 10 hit-test positions, 2 × ⌈h / 10⌉ × ⌈w / 10⌉ grid points, 22
    // frames (20 drawn, 19 of them without the panorama, 2 refusals).
    assert_eq!(counts[0], 10);
    assert_eq!(counts[1], 2 * h.div_ceil(10) * w.div_ceil(10));
    assert_eq!(counts[4], 22);
    assert_eq!(counts[3], 19);
    assert_eq!(counts[5], 2);
    eprintln!(
        "map: {} positions, {} grid points, {} frames ({} sprite calls, {} md5s, {} refusals)",
        counts[0], counts[1], counts[4], counts[2], counts[3], counts[5]
    );
}

/// The tower board's golden of the `w` × `h` screen replayed.
fn tour(golden: &str, w: usize, h: usize) {
    let Some(files) = banks() else {
        return;
    };
    let board = bank(&files, &format!("spr/CarteTour{w}.spr"));
    let pawns = bank(&files, "spr/PionsTour.spr");
    // The renderer's control word, which the warp leaves (0x0e7f): at
    // 800 the factor rounds below 1.
    let fpu = Fpu::Chop;
    assert_eq!(tour_scale(fpu, w) < 1.0, w <= 800);
    let lines: Vec<&str> = golden.lines().filter(|l| !l.starts_with('#')).collect();
    let mut anim = TourAnim::default();
    let (mut a, mut b, mut on) = (0, 0, false);
    let mut last_t = 300.0f64;
    let mut counts = [0usize; 4];
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        i += 1;
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "table" => {
                let list =
                    |s: &str| -> Vec<i32> { s.split(',').map(|v| v.parse().unwrap()).collect() };
                if let Some(x) = l.split(" x=").nth(1) {
                    assert_eq!(list(x), TOUR_X.to_vec());
                } else {
                    assert_eq!(list(f[1].strip_prefix("y=").unwrap()), TOUR_Y.to_vec());
                }
                counts[0] += 1;
            }
            "pawns" => {
                a = f[1].parse().unwrap();
                b = f[2].parse().unwrap();
                on = f[3] == "on=1";
                anim = TourAnim::start(a, b, last_t);
                let want = f.iter().find_map(|x| x.strip_prefix("anim=")).unwrap();
                assert_eq!(format!("{},{}", anim.frame_a, anim.frame_b), want, "{l}");
            }
            "frame" => {
                let fr = frame(l);
                let mut events: Vec<String> = Vec::new();
                while i < lines.len() && lines[i].starts_with("event ") {
                    events.push(lines[i].splitn(4, ' ').nth(3).unwrap().to_string());
                    i += 1;
                }
                last_t = fr.t;
                let mut screen = vec![0u16; w * h];
                let calls = draw_tour(
                    &mut screen,
                    w,
                    h,
                    &board,
                    Some(&pawns),
                    &mut anim,
                    fpu,
                    fr.t,
                    (a, b, on),
                );
                let port: Vec<String> = calls.iter().map(port_draw).collect();
                assert_eq!(port, fr.draws, "{} {}", fr.case, fr.n);
                counts[1] += port.len();
                assert_eq!(
                    Some((anim.frame_a, anim.frame_b)),
                    fr.anim,
                    "{} {}: frames",
                    fr.case,
                    fr.n
                );
                if fr.warp == Some(true) {
                    assert_eq!(md5(&screen), fr.md5, "{} {}", fr.case, fr.n);
                    counts[2] += 1;
                }
                // Any click closes.
                let ret = u8::from(!(fr.left || fr.right));
                assert_eq!(ret, fr.ret, "{} {}: return", fr.case, fr.n);
                let want: Vec<String> = if ret == 0 {
                    vec![
                        "AtlCarteTour_Close_0x00406730".into(),
                        "CtrlScenarIN_0x0044a2f0".into(),
                    ]
                } else {
                    Vec::new()
                };
                assert_eq!(want, events, "{} {}: events", fr.case, fr.n);
                counts[3] += 1;
            }
            _ => {}
        }
    }
    // 2 table lines, 155 frames (2 over the panorama).
    assert_eq!(counts[0], 2);
    assert_eq!(counts[3], 155);
    assert_eq!(counts[2], 153);
    eprintln!(
        "tower board: {} frames, {} sprite calls, {} md5s",
        counts[3], counts[1], counts[2]
    );
}

#[test]
fn the_land_of_the_dead_map_draws_and_picks_as_the_original_at_640() {
    carte(CARTE_640, 640, 480);
}

#[test]
fn the_land_of_the_dead_map_draws_and_picks_as_the_original_at_800() {
    carte(CARTE_800, 800, 600);
}

#[test]
fn the_land_of_the_dead_map_draws_and_picks_as_the_original_at_1024() {
    carte(CARTE_1024, 1024, 768);
}

#[test]
fn the_tower_board_draws_its_pawns_as_the_original_at_640() {
    tour(TOUR_640, 640, 480);
}

#[test]
fn the_tower_board_draws_its_pawns_as_the_original_at_800() {
    tour(TOUR_800, 800, 600);
}

#[test]
fn the_tower_board_draws_its_pawns_as_the_original_at_1024() {
    tour(TOUR_1024, 1024, 768);
}

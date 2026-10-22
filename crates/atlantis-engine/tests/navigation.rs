//! Navigation against the original `atlantis.exe` (2001 English build):
//! the view step of the warp loop `FUN_0041bfa0`, the turn before a
//! transition `FUN_004212cc`, the zone under the pointer `FUN_00422bb0`
//! and the new-game start `FUN_0042de5c`.
//!
//! The expected values come from the original code run under emulation
//! (see the headers of the golden files); they hold numbers and digests
//! only. Ported from ScummVM's `test/engines/cryomni3d/atlantis/`
//! (`graphics_navigation.h`, `graphics_transition_turn.h`,
//! `archive_chapter_start.h`).

use std::collections::BTreeMap;
use std::sync::Arc;

use atlantis_data::wam::{Place, Wam, Zone};
use atlantis_data::{Game, StdDir};
use atlantis_engine::look::{angle_warp_degrees, warp_look_frame, WarpMomentum};
use atlantis_engine::turn::{transition_turn, TurnDoubleClick, TURN_END_FOV_DEG};
use atlantis_engine::warp::{Warp, WARP_FOV_DEG};
use md5::{Digest, Md5};

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// ANGLEWARPX / ANGLEWARPY and the look step against the original run
/// unmodified: the smallest float of every degree on both axes (B rows),
/// and 6720 frames of scripted mouse motion in OMNI 3D MODE 1 and 2 (F
/// rows: ANGLEWARPX / ANGLEWARPY stored at the frame start, then the
/// angles and the momentum).
#[test]
fn anglewarp_matches_original() {
    let mut warp = Warp::new();
    warp.set_hfov(WARP_FOV_DEG);
    let pi = warp.pi();
    let (mut bounds, mut frames, mut bad) = (0, 0, 0);
    let mut trace = (String::new(), -1);
    let mut m = WarpMomentum::default();
    let (mut alpha, mut beta, mut prev_a, mut prev_b) = (0f32, 0f32, -1f32, -1f32);
    for l in lines(include_str!("golden/graphics_anglewarp.txt")) {
        let t: Vec<&str> = l.split_whitespace().collect();
        match t[0] {
            "B" => {
                bounds += 1;
                let k: i32 = t[2].parse().unwrap();
                let bits = u32::from_str_radix(t[3], 16).unwrap();
                let at = angle_warp_degrees(f32::from_bits(bits), pi);
                // The next float toward 0.
                let below = angle_warp_degrees(f32::from_bits(bits - 1), pi);
                if at != k || below != if k > 0 { k - 1 } else { k + 1 } {
                    bad += 1;
                    eprintln!("{l}: port {at} / {below}");
                }
            }
            "F" => {
                // F <name> <mode> <i>: <mx> <my> d <dx> <dy> -> <awx> <awy> <a> <b> <vx> <vy>
                let name = t[1].to_string();
                let mode: i32 = t[2].parse().unwrap();
                let num = |i: usize| t[i].parse::<i32>().unwrap();
                let hex = |i: usize| u32::from_str_radix(t[i], 16).unwrap();
                let (dx, dy) = (num(7), num(8));
                let (awx, awy) = (num(10), num(11));
                let (a, b, vx, vy) = (hex(12), hex(13), hex(14), hex(15));
                if trace != (name.clone(), mode) {
                    trace = (name, mode);
                    m = WarpMomentum::default();
                    alpha = 0.0;
                    beta = 0.0;
                    prev_a = -1.0;
                    prev_b = -1.0;
                }
                frames += 1;
                let f = warp_look_frame(
                    &mut m,
                    &mut alpha,
                    &mut beta,
                    dx,
                    dy,
                    mode != 0,
                    prev_a,
                    prev_b,
                    pi,
                    warp.two_pi(),
                    warp.beta_limit(),
                );
                prev_a = alpha;
                prev_b = beta;
                if (f.angle_warp_x, f.angle_warp_y) != (awx, awy)
                    || (
                        alpha.to_bits(),
                        beta.to_bits(),
                        m.vx.to_bits(),
                        m.vy.to_bits(),
                    ) != (a, b, vx, vy)
                {
                    bad += 1;
                    if bad < 10 {
                        eprintln!(
                            "{l}: port {} {} {:08x} {:08x} {:08x} {:08x}",
                            f.angle_warp_x,
                            f.angle_warp_y,
                            alpha.to_bits(),
                            beta.to_bits(),
                            m.vx.to_bits(),
                            m.vy.to_bits()
                        );
                    }
                }
            }
            _ => panic!("bad golden line {l:?}"),
        }
    }
    assert_eq!(bounds, 450);
    assert_eq!(frames, 6720);
    assert_eq!(bad, 0);
}

fn hex_of(v: f32) -> String {
    v.to_le_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// The turn before a transition video against the original `FUN_004212cc`
/// (drawing stubbed, the angle and FOV calls recorded) for 120 cases: the
/// frames (md5 of the call list) and the angles reached. The final
/// `FUN_0045179a(75.187, 50)`, the jump to the destination angles and
/// ZOOM = 0 are the original's end actions (the engine's
/// `turn_to_transition` and `execute_transition`).
#[test]
#[allow(clippy::approx_constant)] // the golden's source angle 3.14
fn transition_turn_matches_original() {
    let pi = Warp::exe_pi();
    let alphas = [
        (0.0, "0.0"),
        (1.0, "1.0"),
        (3.0, "3.0"),
        (5.5, "5.5"),
        (6.25, "6.25"),
    ];
    let betas = [(-0.3, "-0.3"), (0.0, "0.0"), (0.25, "0.25")];
    let srcs = [
        ((0.5, "0.5"), (0.0, "0.0")),
        ((3.14, "3.14"), (-0.2, "-0.2")),
        ((6.0, "6.0"), (0.3, "0.3")),
        ((1.0, "1.0"), (0.0, "0.0")),
    ];
    let mut out = Vec::new();
    let mut full = Vec::new();
    let mut n = 0;
    for z in 0..2 {
        for (av, an) in alphas {
            for (bv, bn) in betas {
                for ((sav, san), (sbv, sbn)) in srcs {
                    let (sa, sb) = (sav as f32, sbv as f32);
                    let (da, db) = ((sav + 1.0) as f32, (-sbv) as f32);
                    let (frames, end_a, end_b) =
                        transition_turn(pi, av as f32, bv as f32, sa, sb, z != 0);
                    let mut lst = String::new();
                    for f in &frames {
                        let sep = |s: &String| if s.is_empty() { "" } else { " " };
                        if f.fov_set {
                            lst += &format!("{}f{}{}", sep(&lst), hex_of(f.fov), hex_of(50.0));
                        }
                        lst += &format!("{}a{}{}", sep(&lst), hex_of(f.alpha), hex_of(f.beta));
                    }
                    lst += &format!(" f{}{}", hex_of(TURN_END_FOV_DEG), hex_of(50.0));
                    let head = format!("zoom {z} start {an} {bn} src {san} {sbn}");
                    out.push(format!(
                        "{head} -> frames {} end {} {} {} {} 0 md5 {}",
                        frames.len(),
                        hex_of(da),
                        hex_of(db),
                        hex_of(end_a),
                        hex_of(end_b),
                        md5_hex(lst.as_bytes())
                    ));
                    if n < 3 || (z == 1 && n % 60 == 0) {
                        full.push(format!("frames of [{head}]: {lst}"));
                    }
                    n += 1;
                }
            }
        }
    }
    out.push(format!("cases {n}"));
    out.extend(full);
    let want: Vec<&str> = lines(include_str!("golden/graphics_transition_turn.txt")).collect();
    assert_eq!(out.len(), want.len());
    for (got, want) in out.iter().zip(&want) {
        assert_eq!(got, want);
    }
}

/// The double-click flag 0x00496118 that skips a move's video: the
/// cursor-state / menu-request condition of the original window procedure
/// for WM_LBUTTONDBLCLK, then the port's model of the Windows pairing
/// (500 ms, 4 x 4 pixels).
#[test]
fn double_click_matches_original() {
    let mut n = 0;
    for l in lines(include_str!("golden/graphics_double_click.txt")) {
        // WM_LBUTTONDBLCLK cursor C 0x496138 M -> [0x496118] F
        let t: Vec<&str> = l.split(' ').collect();
        if t[0] != "WM_LBUTTONDBLCLK" {
            continue;
        }
        let (cursor, menu, flag): (u8, u8, u8) = (
            t[2].parse().unwrap(),
            t[4].parse().unwrap(),
            t[7].parse().unwrap(),
        );
        let mut d = TurnDoubleClick::default();
        d.first_press(1000, 320, 240);
        assert!(d.press(1100, 321, 239, cursor, menu != 0), "{l}");
        assert_eq!(d.flag, flag != 0, "{l}");
        n += 1;
    }
    assert_eq!(n, 16);

    // A second press later than the double-click time, or outside the
    // rectangle, is a new first press; the press after a double click
    // starts a new pair.
    let mut late = TurnDoubleClick::default();
    late.first_press(1000, 320, 240);
    assert!(!late.press(1000 + TurnDoubleClick::TIME_MS, 320, 240, 5, false));
    assert!(!late.flag);
    assert!(late.press(1100 + TurnDoubleClick::TIME_MS, 320, 240, 5, false));
    assert!(late.flag);
    let mut distant = TurnDoubleClick::default();
    distant.first_press(1000, 320, 240);
    assert!(!distant.press(1050, 323, 240, 5, false));
    assert!(!distant.flag);
    let mut third = TurnDoubleClick::default();
    third.first_press(1000, 320, 240);
    assert!(third.press(1050, 320, 240, 4, false));
    assert!(!third.flag);
    assert!(!third.press(1100, 320, 240, 5, false));
    assert!(!third.flag);
    // No first press kept: never a double click.
    let mut none = TurnDoubleClick::default();
    assert!(!none.press(10, 320, 240, 5, false));
    assert!(!none.flag);
}

/// Off-node points of `FUN_0045166f`: at (320, 240) x & 15 = y & 15 = 0,
/// so its three bilinear terms (0x00451712..0x00451782) weigh 0; these
/// screen points have non-zero x & 15 and y & 15. The expected values come
/// from the original function run under emulation after
/// `FUN_0045179a(75.137, 50)` and `FUN_00451915(alpha, beta)`. The warp
/// loop hit-tests the pointer wherever it is.
#[test]
fn off_node_points_match_original() {
    let pts = [(333, 247), (7, 471), (639, 479), (100, 13)];
    type Case = (f32, f32, [(i32, i32); 4]);
    let cases: [Case; 4] = [
        (0.0, 0.0, [(10, 390), (1837, 543), (213, 547), (1889, 213)]),
        (
            1.0,
            0.25,
            [(1732, 487), (1482, 609), (1965, 612), (1576, 295)],
        ),
        (3.0, -0.5, [(1081, 201), (880, 386), (1261, 391), (838, 83)]),
        (
            6.0,
            0.75,
            [(106, 656), (1771, 696), (468, 696), (1994, 472)],
        ),
    ];
    for (a, b, want) in cases {
        let mut w = Warp::new();
        w.set_hfov(WARP_FOV_DEG);
        w.set_alpha(a);
        w.set_beta(b);
        for ((x, y), o) in pts.iter().zip(want) {
            assert_eq!(w.screen_to_panorama(*x, *y), o, "{a} {b} ({x}, {y})");
        }
    }
}

/// The zone under the pointer: `FUN_00422bb0` (inclusive right / bottom
/// edges, the largest id wins) on zones placed around the original's
/// centre point, with the engine's mapping of the screen centre.
#[test]
fn zone_resolution_matches_original() {
    let angles = [(1.0f32, 0.25f32, 1722i16, 481i16), (3.0, -0.5, 1070, 195)];
    for (a, b, x, y) in angles {
        let mut w = Warp::new();
        w.set_alpha(a);
        w.set_beta(b);
        let (px, py) = w.screen_to_panorama(320, 240);
        assert_eq!((px, py), (i32::from(x), i32::from(y)));
        let z = |x, y, w, h, action| Zone { x, y, w, h, action };
        let cases: [(&str, Vec<Zone>, u16); 7] = [
            ("inside", vec![z(x - 5, y - 5, 10, 10, 7)], 7),
            ("right_bottom_edge", vec![z(x - 10, y - 10, 10, 10, 7)], 7),
            ("left_top_edge", vec![z(x, y, 10, 10, 7)], 7),
            (
                "overlap_low_first",
                vec![z(x - 5, y - 5, 10, 10, 3), z(x - 6, y - 6, 12, 12, 9)],
                9,
            ),
            (
                "overlap_high_first",
                vec![z(x - 6, y - 6, 12, 12, 9), z(x - 5, y - 5, 10, 10, 3)],
                9,
            ),
            ("one_below", vec![z(x - 5, y + 1, 10, 10, 7)], 0),
            ("one_above", vec![z(x - 5, y - 11, 10, 10, 7)], 0),
        ];
        for (name, zones, want) in cases {
            let place = Place {
                zones,
                ..Place::default()
            };
            assert_eq!(place.hit_test(px as i16, py as i16), want, "{name}");
        }
    }
}

/// The new game: `wam\atlan1.wam`, vue 1, whose record is the first of
/// ATLAN1.WAM with the panorama the original loads; the first record of
/// each ATLANn.WAM.
#[test]
fn new_game_start_matches_original() {
    let Some(game) = game("new-game start test") else {
        return;
    };
    let golden: BTreeMap<&str, &str> = lines(include_str!("golden/archive_chapter.txt"))
        .filter_map(|l| l.split_once(' '))
        .collect();
    assert_eq!(
        golden["exe|newgame_wam"],
        atlantis_engine::engine::NEW_GAME_WAM.to_ascii_lowercase()
    );
    assert_eq!(
        golden["exe|newgame_vue"],
        atlantis_engine::engine::NEW_GAME_VUE.to_string()
    );
    for n in 1..=5 {
        let name = format!("ATLAN{n}.WAM");
        let w = Wam::parse(&game.big.read(&format!("WAM\\{name}")).unwrap());
        assert_eq!(
            w.places[0].id.to_string(),
            golden[format!("{name}|record0_id").as_str()]
        );
        if n == 1 {
            let i = w
                .places
                .iter()
                .position(|p| p.id == atlantis_engine::engine::NEW_GAME_VUE)
                .unwrap();
            assert_eq!(i.to_string(), golden["ATLAN1.WAM|vue1_index"]);
            assert_eq!(w.places[i].cyclo, golden["ATLAN1.WAM|vue1_cyclo"]);
        }
    }
}

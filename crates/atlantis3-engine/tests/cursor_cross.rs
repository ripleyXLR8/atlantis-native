//! The warp cursor against the original under emulation
//! (`tests/golden/cursor_cross.txt`, @ 0x00440a30): the cross's colour
//! (green over a place sprite the player can click, @ 0x0076e928) and the
//! cursor's frame range (@ 0x00428060), the place sprite first, then the
//! zone. `set` lines give the zone and the sprite hit directly; `ray`
//! lines turn the camera of a Hoggar view (`tests/golden/spwz_pick.txt`'s
//! slots) and take the hit from the frame's sprite pass.
//!
//! The `ray` part needs the game: `ATLANTIS3_DATA`.

use std::collections::HashMap;

use atlantis2_data::wam::Wam;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::{Camera, Fpu};
use atlantis3_data::A3Files;
use atlantis3_engine::place_anim::Slots;
use atlantis3_engine::{cross_colour, cursor_range, Game};

const GOLDEN: &str = include_str!("golden/cursor_cross.txt");
const SPWZ: &str = include_str!("golden/spwz_pick.txt");

fn field<'a>(l: &'a str, k: &str) -> &'a str {
    l.split(' ').find_map(|w| w.strip_prefix(k)).unwrap()
}

/// The expected tail of a line from the zone type and the hit (click kind,
/// cursor kind), the crosshair on or off, as the port computes it (the
/// harness resets the range to 0..0 before each case).
fn port(zone: Option<i32>, hit: Option<(i32, i32)>, cross: bool) -> String {
    let line = if cross {
        let c = format!("0x{:08x}", cross_colour(zone, hit.is_some()));
        format!("{c},{c}")
    } else {
        "-".to_string()
    };
    let (cursor, range, aff) = match hit.or(zone.map(|z| (z, 0))) {
        Some((kind, mode)) => {
            let r = cursor_range(kind, mode, (0, 0));
            (
                format!("{kind},{mode}"),
                format!("{}..{} frame={}", r.0, r.1, r.0),
                if kind < 1000 { "1" } else { "-" },
            )
        }
        None => ("-".to_string(), "-".to_string(), "-"),
    };
    format!("line={line} cursor={cursor} range={range} aff={aff}")
}

fn zone_of(l: &str) -> Option<i32> {
    match field(l, "zone=") {
        "-" => None,
        z => Some(z.parse().unwrap()),
    }
}

fn tail(l: &str) -> &str {
    let at = l.find("line=").unwrap();
    &l[at..]
}

#[test]
fn cursor_without_the_sprite_pass_as_the_original() {
    let mut n = 0;
    for l in GOLDEN.lines().filter(|l| l.starts_with("set ")) {
        let hit: i32 = field(l, "hit=").parse().unwrap();
        let hit = (hit != 0).then(|| (field(l, "slot_click=").parse().unwrap(), hit));
        let cross = field(l, "cross=") == "1";
        assert_eq!(port(zone_of(l), hit, cross), tail(l), "{l}");
        n += 1;
    }
    assert_eq!(n, 56);
}

#[test]
fn cursor_over_place_sprites_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    let mut wam = Wam::parse(&files.read("wam/hoggar.wam").unwrap()).unwrap();
    wam.apply_defnames(g.names());
    let mut cache = HashMap::new();
    let mut slots: HashMap<u32, Slots> = HashMap::new();
    for l in SPWZ.lines().filter(|l| l.starts_with("slot ")) {
        let f: Vec<&str> = l.split(' ').collect();
        let v: u32 = f[1].parse().unwrap();
        let s = slots.entry(v).or_insert_with(|| {
            Slots::load(
                &files,
                &mut cache,
                &wam,
                v,
                "hoggar",
                &mut Rand::new(1),
                0.0,
                |_, _| None,
            )
        });
        let sl = &mut s.slots[f[2].parse::<usize>().unwrap()];
        sl.shown = field(l, "shown=") == "1";
        sl.frame = field(l, "frame=").parse().unwrap();
    }
    let (mut n, mut greens) = (0, 0);
    for l in GOLDEN.lines().filter(|l| l.starts_with("ray ")) {
        let f: Vec<&str> = l.split(' ').collect();
        let v: u32 = f[1].parse().unwrap();
        let (yaw, pitch): (i32, i32) = (f[2].parse().unwrap(), f[3].parse().unwrap());
        let s = &slots[&v];
        let mut cam = Camera::new(Fpu::Chop, 800, 600);
        cam.enter(
            deg_to_rad_a3(Fpu::Chop, yaw),
            deg_to_rad_a3(Fpu::Chop, pitch),
        );
        let found = cam
            .texel_a3(cam.cx, cam.cy)
            .and_then(|(face, u, w)| s.hit(face, u, w));
        let (slot, kind) = found.map_or((-1, 0), |(k, c)| (k as i32, c));
        assert_eq!(
            format!("hit={kind} slot={slot}"),
            format!("hit={} slot={}", field(l, "hit="), field(l, "slot=")),
            "{l}"
        );
        let hit = found.map(|(k, c)| (s.slots[k].click, c));
        assert_eq!(port(zone_of(l), hit, true), tail(l), "{l}");
        n += 1;
        greens += usize::from(hit.is_some());
    }
    assert_eq!((n, greens), (12, 8));
}

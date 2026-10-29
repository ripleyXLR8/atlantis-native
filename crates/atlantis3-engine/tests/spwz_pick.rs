//! The place-sprite hit test against the original under emulation
//! (`tests/golden/spwz_pick.txt`): for seven Hoggar views and every
//! orientation of a 1° grid, the centre ray's face pixel and the slot
//! under it (`GET_SPWZ` and `SPW_COLLISION`) with the cursor kind.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use std::collections::HashMap;

use atlantis2_data::wam::Wam;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::{Camera, Fpu};
use atlantis3_data::A3Files;
use atlantis3_engine::place_anim::Slots;
use atlantis3_engine::Game;

const GOLDEN: &str = include_str!("golden/spwz_pick.txt");

fn field<'a>(l: &'a str, k: &str) -> &'a str {
    l.split(' ').find_map(|w| w.strip_prefix(k)).unwrap()
}

#[test]
fn place_sprites_hit_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    let mut wam = Wam::parse(&files.read("wam/hoggar.wam").unwrap()).unwrap();
    wam.apply_defnames(g.names());
    let mut cache = HashMap::new();
    let mut views: Vec<u32> = Vec::new();
    let mut slots: HashMap<u32, Slots> = HashMap::new();
    let mut rays: HashMap<(u32, i32, i32), String> = HashMap::new();
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "slot" => {
                let v: u32 = f[1].parse().unwrap();
                let s = slots.entry(v).or_insert_with(|| {
                    views.push(v);
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
                let k: usize = f[2].parse().unwrap();
                let sl = &mut s.slots[k];
                assert_eq!(sl.name, f[3].trim_end_matches(".SPW"), "{l}");
                sl.shown = field(l, "shown=") == "1";
                sl.frame = field(l, "frame=").parse().unwrap();
            }
            "ray" => {
                let k = (
                    f[1].parse().unwrap(),
                    f[2].parse().unwrap(),
                    f[3].parse().unwrap(),
                );
                rays.insert(k, f[4..].join(" "));
            }
            _ => {}
        }
    }
    let mut hits = 0;
    let mut off = 0;
    for &v in &views {
        let s = &slots[&v];
        for yaw in 0..360 {
            for pitch in -40..=40 {
                let mut cam = Camera::new(Fpu::Double, 800, 600);
                cam.enter(
                    deg_to_rad_a3(Fpu::Double, yaw),
                    deg_to_rad_a3(Fpu::Double, pitch),
                );
                let Some((face, u, w)) = cam.texel(cam.cx, cam.cy) else {
                    continue;
                };
                match rays.get(&(v, yaw, pitch)) {
                    Some(want) => {
                        // The hit test at the original's face pixel.
                        let g: Vec<i32> = want
                            .split(' ')
                            .filter(|x| *x != "->")
                            .map(|x| x.parse().unwrap())
                            .collect();
                        let hit = s.hit(g[0] as usize, g[1], g[2]);
                        let got = match hit {
                            Some((k, c)) => format!("{} {} {} -> {k} {c}", g[0], g[1], g[2]),
                            None => format!("{} {} {} -> -1 0", g[0], g[1], g[2]),
                        };
                        assert_eq!(&got, want, "view {v} at {yaw}, {pitch}");
                        hits += usize::from(hit.is_some());
                        // The centre ray's pixel: Atlantis II's texel; one
                        // pixel off on 99 rays (0x0043a1a0 not transcribed).
                        if (face as i32, u, w) != (g[0], g[1], g[2]) {
                            off += 1;
                        }
                    }
                    None => {
                        let hit = s.hit(face, u, w);
                        assert!(hit.is_none(), "view {v} at {yaw}, {pitch}: {hit:?}");
                    }
                }
            }
        }
    }
    assert_eq!(hits, 2396);
    assert!(off <= 99, "{off}");
}

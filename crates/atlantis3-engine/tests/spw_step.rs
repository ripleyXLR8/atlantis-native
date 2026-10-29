//! The place animations' step against the original under emulation
//! (`tests/golden/spw_step.txt`): the slots hoggar.wam's views 5 and 3
//! load, then 600 ticks each: frames, ends, the weighted hand-overs
//! between an entity's states and the `rand` draws.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use std::collections::HashMap;

use atlantis2_data::wam::Wam;
use atlantis2_engine::crt::Rand;
use atlantis3_data::A3Files;
use atlantis3_engine::place_anim::Slots;
use atlantis3_engine::Game;

const GOLDEN: &str = include_str!("golden/spw_step.txt");

/// A `Rand` that records its draws.
struct Rec {
    r: Rand,
}

#[test]
fn place_animations_step_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    // The place's own show / hide set-up (every flag 0).
    g.run_command("(changewam=hoggar.wam)");
    let shown = g.script().unwrap().shown.clone();
    let mut wam = Wam::parse(&files.read("wam/hoggar.wam").unwrap()).unwrap();
    wam.apply_defnames(g.names());
    let mut cache = HashMap::new();
    let mut slots = Slots::default();
    let mut rand = Rec { r: Rand::new(1) };
    let mut checked = 0;
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "view" => {
                rand.r = Rand::new(1);
                let view: u32 = f[1].parse().unwrap();
                slots = Slots::load(
                    &files,
                    &mut cache,
                    &wam,
                    view,
                    "hoggar",
                    &mut rand.r,
                    100.0,
                    |id, kind| {
                        shown
                            .iter()
                            .find(|r| r.id == id && r.kind == kind)
                            .map(|r| r.shown)
                    },
                );
                assert_eq!(slots.slots.len().to_string(), f[3], "{l}");
            }
            "slot" => {
                let k: usize = f[1].parse().unwrap();
                let s = &slots.slots[k];
                assert_eq!(format!("{}", s.name), f[2].trim_end_matches(".SPW"), "{l}");
                assert_eq!(
                    (s.kind, s.id),
                    (f[4].parse().unwrap(), f[6].parse().unwrap()),
                    "{l}"
                );
                checked += 1;
            }
            "t" => {
                let t: u32 = f[1].parse().unwrap();
                let before = rand.r.state;
                if t > 0 {
                    let now = 100.0 + f64::from(t) * 1.0025 / 15.0;
                    slots.step(now, &mut rand.r);
                }
                let got: Vec<String> = slots
                    .slots
                    .iter()
                    .enumerate()
                    .map(|(k, s)| {
                        format!(
                            "{k}:{}/{}/{}/{}/{}/{}/{}/{}/{}/{}",
                            s.frame,
                            s.last,
                            u8::from(s.shown),
                            u8::from(s.fresh),
                            u8::from(s.ended),
                            u8::from(s.waiting),
                            s.pause,
                            s.weight,
                            u8::from(s.queued),
                            s.next.map_or(-1, |n| n as i32)
                        )
                    })
                    .collect();
                let n = slots.slots.len();
                assert_eq!(got.join(" "), f[2..2 + n].join(" "), "{l}");
                // The rand values drawn in the tick.
                let mut r = Rand::new(before);
                let want: Vec<i32> = f[2 + n..]
                    .iter()
                    .skip(1)
                    .map(|v| v.parse().unwrap())
                    .collect();
                let drawn: Vec<i32> = want.iter().map(|_| r.rand()).collect();
                assert_eq!(drawn, want, "{l}");
                assert_eq!(r.state, rand.r.state, "{l}: draws");
                checked += 1;
            }
            _ => {}
        }
    }
    assert_eq!(checked, 32 + 1202);
}

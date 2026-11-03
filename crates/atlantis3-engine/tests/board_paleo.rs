//! The boards of Paleo (world 4) and the corridor (world 5) against the
//! original's C code under emulation, replayed through the port's board
//! API ([`common::place`]):
//!
//! - the wolves' dominance fight (`tests/golden/puzzle_wolves.txt`);
//! - the tiger stone throw (`puzzle_tiger.txt`): the stone's sprites, the
//!   camera lock, the hit window;
//! - the hunt with the spear (`puzzle_hunt.txt`): the spear's sprites, the
//!   camera lock, the hits and the right-click end;
//! - the altar of the four hearts (`puzzle_altar.txt`): the hearts' frame
//!   lists;
//! - the corridor's doors (`puzzle_corridor.txt`): the door plays, their
//!   frame lists and the code's own `(Vue=15)`.
//!
//! Every line of every run is compared (the place animations' slots
//! included). Needs the game: `ATLANTIS3_DATA`.

mod common;

use atlantis3_data::A3Files;
use common::place::{replay_all, Spec};

fn files() -> Option<impl Fn() -> A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(move || A3Files::open_dir(&dir).unwrap().0)
}

fn check(golden: &str, spec: Spec, runs: &[&str]) {
    let Some(files) = files() else {
        return;
    };
    let names = replay_all(files, golden, spec);
    assert_eq!(names, runs);
}

const HEARTS: [&str; 4] = [
    "PAL_COEUR_CERF",
    "PAL_COEUR_AVROCH",
    "PAL_COEUR_CHEVAL",
    "PAL_COEUR_BOUQUETIN",
];

#[test]
fn the_wolves_as_the_original() {
    check(
        include_str!("golden/puzzle_wolves.txt"),
        Spec {
            world: 4,
            st: "wolves",
            wam_setup: true,
            flags: &[105, 603],
            bag: &[],
        },
        &[
            "solution noir gris jaune marron blanc",
            "wrong blanc noir gris jaune marron, then PAL_ZLCOMBAT",
            "view1 flag105=1 (after the success)",
        ],
    );
}

#[test]
fn the_tiger_stone_as_the_original() {
    check(
        include_str!("golden/puzzle_tiger.txt"),
        Spec {
            world: 4,
            st: "tiger",
            wam_setup: true,
            flags: &[111, 612],
            bag: &["PAL_CAILLOU"],
        },
        &[
            "hit yaw 55 pitch -3 tf=70",
            "miss_timing yaw 55 pitch -3 tf=10",
            "miss_yaw yaw 45 pitch -3 tf=70",
            "miss_pitch yaw 55 pitch 0 tf=70",
            "edge_early_miss yaw 55 pitch -3 tf=64 (frame 85 at the test)",
            "edge_early_hit yaw 55 pitch -3 tf=65 (frame 86 at the test)",
            "edge_late_hit yaw 55 pitch -3 tf=113 (frame 134 at the test)",
            "edge_late_miss yaw 55 pitch -3 tf=114 (frame 135 at the test)",
        ],
    );
}

#[test]
fn the_hunt_as_the_original() {
    const BAG: [&str; 5] = ["PAL_LANCE", HEARTS[0], HEARTS[1], HEARTS[2], HEARTS[3]];
    check(
        include_str!("golden/puzzle_hunt.txt"),
        Spec {
            world: 4,
            st: "hunt",
            wam_setup: true,
            flags: &[608, 609, 610, 611, 620, 91],
            bag: &BAG,
        },
        &[
            "win propulsor, four hits",
            "miss propulsor, aim off the stag, then a right click",
            "no_propulsor flag614=0, aim on the stag (no hit test)",
            "edges stag window -3 % (miss), +3 % (hit); aurochs at pitch -10 (no band: miss)",
        ],
    );
}

#[test]
fn the_altar_as_the_original() {
    check(
        include_str!("golden/puzzle_altar.txt"),
        Spec {
            world: 4,
            st: "altar",
            wam_setup: true,
            flags: &[616, 617, 618, 619, 70, 604],
            bag: &HEARTS,
        },
        &[
            "win horse fire, ibex earth, aurochs water, stag air",
            "wrong stag fire, horse fire (same place), empty hand on fire (both back), stag air",
            "reentry flags 616..619 = 2, 3, 0, 0 (stag air, aurochs water placed before), then horse fire, ibex earth",
        ],
    );
}

#[test]
fn the_corridor_doors_as_the_original() {
    check(
        include_str!("golden/puzzle_corridor.txt"),
        Spec {
            world: 5,
            st: "couloir",
            wam_setup: false,
            flags: &[17, 871, 872, 873, 906, 918, 920],
            bag: &[],
        },
        &[
            "solution 15G 17D 18G 17G 15D 20G",
            "wrong 15G 15G 17D 18D (15G again keeps step 1; a wrong door resets)",
            "no_world_flags the six right doors without flags 159 866 867 908 605, then 15G",
        ],
    );
}

//! The boards of worlds 1 and 2 against the original's C code under
//! emulation, replayed through the port's board API ([`common::pz`]):
//!
//! - the Hoggar's spinning top and stars (`tests/golden/puzzle_toupie_v2.txt`);
//! - the funeral chamber's star zones (`puzzle_cfuner_stars.txt`);
//! - the Land of the Dead's Isis tiles (`puzzle_isis_tiles.txt`), the
//!   fisherman's scale (`puzzle_fisherman.txt`), the feather chest, scales
//!   and pedestal (`puzzle_ldm_feathers.txt`) and the Ankh coffer's lid
//!   (`puzzle_ankh_lid.txt`).
//!
//! Every line of every run is compared (the place animations' slots
//! included). Needs the game: `ATLANTIS3_DATA`.

mod common;

use atlantis3_data::A3Files;
use common::pz::{replay_all, Spec};

fn files() -> Option<impl Fn() -> A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(move || A3Files::open_dir(&dir).unwrap().0)
}

fn check(golden: &str, world: i32, st: &'static str, runs: &[&str]) {
    let Some(files) = files() else {
        return;
    };
    let names = replay_all(files, golden, Spec { world, st });
    assert_eq!(names, runs);
}

#[test]
fn the_spinning_top_as_the_original() {
    check(
        include_str!("golden/puzzle_toupie_v2.txt"),
        1,
        "toupie",
        &["solution 4 1 3 5 2", "wrong 1 2 3 2"],
    );
}

#[test]
fn the_funeral_chamber_stars_as_the_original() {
    check(
        include_str!("golden/puzzle_cfuner_stars.txt"),
        2,
        "cfuner",
        &["win GDDGD ", "wrong DDGDGG GDDGD", "after GDDGD G"],
    );
}

#[test]
fn the_isis_tiles_as_the_original() {
    check(
        include_str!("golden/puzzle_isis_tiles.txt"),
        2,
        "isis",
        &["win", "wrong", "solved"],
    );
}

#[test]
fn the_fisherman_as_the_original() {
    check(
        include_str!("golden/puzzle_fisherman.txt"),
        2,
        "fisherman",
        &["anim", "taken", "loaded"],
    );
}

#[test]
fn the_feathers_as_the_original() {
    check(
        include_str!("golden/puzzle_ldm_feathers.txt"),
        2,
        "feathers",
        &["chest", "scales", "pedestal_right", "pedestal_wrong"],
    );
}

#[test]
fn the_ankh_lid_as_the_original() {
    check(
        include_str!("golden/puzzle_ankh_lid.txt"),
        2,
        "ankh",
        &["open", "entry_open"],
    );
}

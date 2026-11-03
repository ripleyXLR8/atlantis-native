//! The boards of the magician's palace (world 3) against the original's C
//! code under emulation, replayed through the port's board API
//! ([`common::pz`], the palace's format):
//!
//! - the drawer wall `PMA_TT` (`tests/golden/puzzle_drawers_tt.txt`): the
//!   rotated drawers, the locks, the fifth row and its clues;
//! - the drawer wall `PMA_TR` (`puzzle_drawers_tr.txt`): the steps, the
//!   cupboard, lamp and pull-tab animations;
//! - the star room (`puzzle_star_room.txt`): the constellations, the path,
//!   the blinks, the spoken tips; its `table` lines against the port's
//!   tables.
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

fn check(golden: &str, st: &'static str, runs: &[&str]) {
    let Some(files) = files() else {
        return;
    };
    let spec = Spec {
        world: 3,
        st,
        palace: true,
        tower: false,
    };
    let names = replay_all(files, golden, spec);
    let names: Vec<&str> = names
        .iter()
        .map(|n| n.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(names, runs);
}

#[test]
fn the_drawer_wall_tt_as_the_original() {
    check(
        include_str!("golden/puzzle_drawers_tt.txt"),
        "tt",
        &["solution", "wrong", "relock"],
    );
}

#[test]
fn the_drawer_wall_tr_as_the_original() {
    check(
        include_str!("golden/puzzle_drawers_tr.txt"),
        "tr",
        &["solution", "wrong"],
    );
}

#[test]
fn the_star_room_as_the_original() {
    check(
        include_str!("golden/puzzle_star_room.txt"),
        "star",
        &["win", "tips_and_wrong"],
    );
}

#[test]
fn the_star_room_tables_as_the_original() {
    let golden = include_str!("golden/puzzle_star_room.txt");
    let want: Vec<&str> = golden.lines().filter(|l| l.starts_with("table ")).collect();
    let got = atlantis3_engine::star_room_tables();
    assert_eq!(want, got);
}

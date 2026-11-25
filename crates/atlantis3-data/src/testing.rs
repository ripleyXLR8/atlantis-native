//! The rule of the game-data tests when their data is missing.
//!
//! The tests that need the game read its installed folder (the one holding
//! `Game.exe` and `Datas\`) from `ATLANTIS3_DATA` (GOG.com's French
//! install); the tests of the other languages read GOG.com's English,
//! German and Spanish installs from `ATLANTIS3_GOG_EN`, `ATLANTIS3_GOG_DE`
//! and `ATLANTIS3_GOG_ES` ([`GOG_LANGUAGES`]); the French retail CD's
//! tests read its installed folder (`Atlantis3.exe` and `Datas\`) from
//! `ATLANTIS3_CD_FR` and the folder of its three discs (the CD images or
//! copies of the CDs) from `ATLANTIS3_DISCS_FR` ([`cd_fr`]); the Italian
//! *Atlantis Collection*'s installed folder from `ATLANTIS3_IT_COLLECTION`
//! ([`it_collection`]). Without it a
//! test must
//! not pass as if it had run: [`missing`] fails it, unless
//! `ATLANTIS3_SKIP_DATA_TESTS=1` (or `ATLANTIS2_SKIP_DATA_TESTS=1`, which a
//! build without any game sets, such as the CI's) asks for the game-data
//! tests to be skipped, and then writes a `SKIPPED` line that `cargo test`
//! shows (not captured).

use std::io::Write;
use std::path::PathBuf;

/// The variable that lets the game-data tests skip without their data.
pub const SKIP_VAR: &str = "ATLANTIS3_SKIP_DATA_TESTS";

/// The variable naming the installed game.
pub const DATA_VAR: &str = "ATLANTIS3_DATA";

/// Whether `ATLANTIS3_SKIP_DATA_TESTS` (or Atlantis II's variable) is `1`.
pub fn skip_allowed() -> bool {
    [SKIP_VAR, atlantis2_data::testing::SKIP_VAR]
        .iter()
        .any(|v| std::env::var_os(v).is_some_and(|v| v == "1"))
}

/// Called by a test whose data `what` is not there, before it returns
/// without running: panics, or with `ATLANTIS3_SKIP_DATA_TESTS=1` writes
/// `SKIPPED <test>: <what>` to the standard error past the test harness's
/// capture.
///
/// # Panics
///
/// Unless the game-data tests may be skipped ([`skip_allowed`]).
pub fn missing(what: &str) {
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("?");
    if !skip_allowed() {
        panic!(
            "{what}: this test needs the game's data; set {DATA_VAR} to the \
             installed game or {SKIP_VAR}=1 to skip the game-data tests"
        );
    }
    let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {what}");
}

/// The variables naming GOG.com's other languages' installs, with their
/// registry language codes.
pub const GOG_LANGUAGES: [(&str, &str); 3] = [
    ("ATLANTIS3_GOG_EN", "gbr"),
    ("ATLANTIS3_GOG_DE", "ger"),
    ("ATLANTIS3_GOG_ES", "esp"),
];

/// The install named by the variable `var`, or `None` after [`missing`].
pub fn dir_of(var: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
        _ => {
            missing(&format!("{var} not set"));
            None
        }
    }
}

/// The variable naming the French retail CD's installed folder.
pub const CD_FR_VAR: &str = "ATLANTIS3_CD_FR";

/// The variable naming the folder of the French retail CD's discs.
pub const DISCS_FR_VAR: &str = "ATLANTIS3_DISCS_FR";

/// The French retail CD's installed folder and its discs' folder
/// (`ATLANTIS3_CD_FR`, `ATLANTIS3_DISCS_FR`), or `None` after [`missing`].
pub fn cd_fr() -> Option<(PathBuf, PathBuf)> {
    Some((dir_of(CD_FR_VAR)?, dir_of(DISCS_FR_VAR)?))
}

/// The variable naming the Italian *Atlantis Collection*'s installed
/// folder (`Game.exe` and `Datas\`, as its installer lays them out).
pub const IT_COLLECTION_VAR: &str = "ATLANTIS3_IT_COLLECTION";

/// The Italian *Atlantis Collection*'s installed folder
/// (`ATLANTIS3_IT_COLLECTION`), or `None` after [`missing`].
pub fn it_collection() -> Option<PathBuf> {
    dir_of(IT_COLLECTION_VAR)
}

/// The installed game's folder from `ATLANTIS3_DATA`, or `None` after
/// [`missing`].
pub fn install_dir() -> Option<PathBuf> {
    match std::env::var_os(DATA_VAR) {
        Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
        _ => {
            missing("ATLANTIS3_DATA not set");
            None
        }
    }
}

/// The variable naming the French DVD of *Atlantis V* (the folder holding
/// `soa-fr.iso`, the disc's files in `files/` and the extracted cabinets
/// in `datas/`).
pub const A5_DVD_FR_VAR: &str = "ATLANTIS5_DVD_FR";

/// The French DVD's folder from `ATLANTIS5_DVD_FR`. Unlike the other
/// games' data, its absence never fails a test: the test writes
/// `SKIPPED <test>` and returns.
pub fn a5_dvd_fr() -> Option<PathBuf> {
    match std::env::var_os(A5_DVD_FR_VAR) {
        Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
        _ => {
            let thread = std::thread::current();
            let test = thread.name().unwrap_or("?");
            let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {A5_DVD_FR_VAR} not set");
            None
        }
    }
}

/// The variable naming the international DVD's folder: a copy of the
/// pressed disc's files (`datas.cab`, `datas_english.cab`, ...).
pub const A5_DVD_INT_VAR: &str = "ATLANTIS5_DVD_INT";

/// The international DVD's folder from `ATLANTIS5_DVD_INT`; as
/// [`a5_dvd_fr`], its absence never fails a test.
pub fn a5_dvd_int() -> Option<PathBuf> {
    match std::env::var_os(A5_DVD_INT_VAR) {
        Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
        _ => {
            let thread = std::thread::current();
            let test = thread.name().unwrap_or("?");
            let _ = writeln!(
                std::io::stderr(),
                "SKIPPED {test}: {A5_DVD_INT_VAR} not set"
            );
            None
        }
    }
}

/// The variable naming the North-American CDs' folder: a copy of each
/// disc's files in `CD1`, `CD2` and `CD3`.
pub const A5_CD_US_VAR: &str = "ATLANTIS5_CD_US";

/// The North-American CDs' folder from `ATLANTIS5_CD_US`; as
/// [`a5_dvd_fr`], its absence never fails a test.
pub fn a5_cd_us() -> Option<PathBuf> {
    match std::env::var_os(A5_CD_US_VAR) {
        Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
        _ => {
            let thread = std::thread::current();
            let test = thread.name().unwrap_or("?");
            let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {A5_CD_US_VAR} not set");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without the variable a missing data set fails the test; with the
    /// skip variable it returns.
    #[test]
    fn missing_data_fails_unless_skips_are_asked_for() {
        let allowed = skip_allowed();
        let r = std::panic::catch_unwind(|| missing("ATLANTIS3_TEST_NOTHING not set"));
        assert_eq!(r.is_ok(), allowed);
    }
}

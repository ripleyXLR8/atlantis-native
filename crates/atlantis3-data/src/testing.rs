//! The rule of the game-data tests when their data is missing.
//!
//! The tests that need the game read its installed folder (the one holding
//! `Game.exe` and `Datas\`) from `ATLANTIS3_DATA`. Without it a test must
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

//! The rule of the game-data tests when their data is missing.
//!
//! The tests that need the game read its folders from `ATLANTIS2_DATA`,
//! `ATLANTIS2_DISCS` (the French CD), `ATLANTIS2_GOG` (GOG.com's
//! edition) and `ATLANTIS2_PATCH101` (the 1.01 executable). Without them a
//! test must not pass as if it had run: [`missing`] fails it, unless
//! `ATLANTIS2_SKIP_DATA_TESTS=1` asks for the game-data tests to be
//! skipped (a build without the game, such as the CI's), and then writes a
//! `SKIPPED` line that `cargo test` shows (not captured).

use std::io::Write;

/// The variable that lets the game-data tests skip without their data.
pub const SKIP_VAR: &str = "ATLANTIS2_SKIP_DATA_TESTS";

/// Whether `ATLANTIS2_SKIP_DATA_TESTS` is `1`.
pub fn skip_allowed() -> bool {
    std::env::var_os(SKIP_VAR).is_some_and(|v| v == "1")
}

/// Called by a test whose data `what` (the variables, or the file it
/// needs) is not there, before it returns without running: panics, or
/// with `ATLANTIS2_SKIP_DATA_TESTS=1` writes `SKIPPED <test>: <what>` to
/// the standard error past the test harness's capture.
///
/// # Panics
///
/// Unless `ATLANTIS2_SKIP_DATA_TESTS=1`.
pub fn missing(what: &str) {
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("?");
    if !skip_allowed() {
        panic!(
            "{what}: this test needs the game's data; set the variables \
             (ATLANTIS2_DATA, ATLANTIS2_DISCS, ATLANTIS2_GOG, \
             ATLANTIS2_PATCH101) or {SKIP_VAR}=1 to skip the game-data tests"
        );
    }
    // Written straight to the standard error, which the harness does not
    // capture (it captures `eprintln!`): the skip is seen.
    let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {what}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without the variable a missing data set fails the test; with
    /// `ATLANTIS2_SKIP_DATA_TESTS=1` it returns.
    #[test]
    fn missing_data_fails_unless_skips_are_asked_for() {
        let allowed = skip_allowed();
        let r = std::panic::catch_unwind(|| missing("ATLANTIS2_TEST_NOTHING not set"));
        assert_eq!(r.is_ok(), allowed);
    }
}

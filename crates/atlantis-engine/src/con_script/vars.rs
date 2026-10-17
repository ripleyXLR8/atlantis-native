//! The CON variables of the game state, by lower-case VARIAS.CON name, and
//! the ones the place loop writes around its scans.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`place_loop_vars.h`;
//! GPLv3, the ScummVM team).
//!
//! DEPARTVUE and ABORTDEPART are ordinary stored variables: a script Vue=
//! leaves DEPARTVUE = N for every later scan until a move ends, and an
//! AbortDepart=1 set outside a departure scan cancels the next move.

use std::collections::HashMap;

/// The variable array of the original (one int per VARIAS.CON line), by
/// name; a variable never written reads 0.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vars(HashMap<String, i32>);

impl Vars {
    pub fn get(&self, key: &str) -> i32 {
        self.0.get(key).copied().unwrap_or(0)
    }

    pub fn set(&mut self, key: &str, value: i32) {
        self.0.insert(key.to_string(), value);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, i32)> {
        self.0.iter().map(|(k, &v)| (k.as_str(), v))
    }

    /// `FUN_00427634` on a chapter change zeroes the variables from slot 7
    /// on (0x0042767f..): the first 7 of VARIAS.CON survive, and ZOOM,
    /// which it reads before the clear and writes back
    /// (0x00427664..0x004276ca).
    pub fn chapter_reset(&mut self) {
        self.0.retain(|k, _| is_persistent(k));
    }

    /// `FUN_00421cfc` @ 0x00421db3: DEPARTVUE = the id of the place looked
    /// up, on a move (the destination, @ 0x00423dc5) and on a script Vue=
    /// (@ 0x0042a66d).
    pub fn place_looked_up(&mut self, place: i32) {
        self.set("departvue", place);
    }

    /// `FUN_00427548` @ 0x00427607, the arrival preparation run before the
    /// place loop's own arrival scan (its only call, @ 0x00423b63).
    pub fn arrival_prepared(&mut self) {
        self.set("abortdepart", 0);
    }

    /// `FUN_00427814` @ 0x00427843: FLAGVUE1 = 0 after the departure scan.
    pub fn departure_scanned(&mut self) {
        self.set("flagvue1", 0);
    }

    /// `FUN_00423ad8` @ 0x00423e2b..0x00423e7e, after the departure scan:
    /// an AbortDepart (from any scan since the last arrival preparation)
    /// ends the move and clears ARRIVEEVUE, ABORTDEPART and DEPARTVUE.
    pub fn take_abort(&mut self) -> bool {
        if self.get("abortdepart") == 0 {
            return false;
        }
        self.set("arriveevue", 0);
        self.set("abortdepart", 0);
        self.set("departvue", 0);
        true
    }

    /// `FUN_00423ad8` @ 0x00424089: DEPARTVUE = 0 once the move has entered
    /// the new place, before the next pass's arrival scan.
    pub fn move_ended(&mut self) {
        self.set("departvue", 0);
    }

    /// `FUN_00423ad8` @ 0x00423c70: the zone-999 scan runs with DEPARTVUE =
    /// 999...
    pub fn zone999_begin(&mut self) {
        self.set("departvue", 999);
    }

    /// ... then 0 (@ 0x00423c91).
    pub fn zone999_end(&mut self) {
        self.set("departvue", 0);
    }
}

/// The variables a chapter change keeps (see [`Vars::chapter_reset`]).
pub fn is_persistent(key: &str) -> bool {
    const PERSISTENT: [&str; 8] = [
        "chapter",
        "gardecel",
        "newsound",
        "flagchp8",
        "bouclecreon",
        "codetronevu",
        "nbstatues",
        "zoom",
    ];
    PERSISTENT.iter().any(|p| key.eq_ignore_ascii_case(p))
}

/// A script Vue= sets ARRIVEEVUE (`FUN_0042a100`); the next pass then runs
/// the arrival scan (@ 0x00423b48), except when the Vue= ran inside the
/// place loop's own arrival scan, after which ARRIVEEVUE is cleared
/// (@ 0x00423be7).
pub fn vue_queues_arrival_scan(in_place_arrival_scan: bool) -> bool {
    !in_place_arrival_scan
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The place loop's writes around its scans (`FUN_00423ad8`,
    /// `FUN_00427548`, `FUN_00427814`, `FUN_00421cfc`).
    #[test]
    fn place_loop_vars() {
        let mut v = Vars::default();
        v.place_looked_up(30); // script Vue=30
        assert_eq!(v.get("departvue"), 30);
        v.set("abortdepart", 1); // set by the next arrival scan
        v.arrival_prepared(); // FUN_00427548 runs before it
        assert_eq!(v.get("abortdepart"), 0);
        assert_eq!(v.get("departvue"), 30); // not cleared there
        v.set("abortdepart", 1);
        v.set("arriveevue", 12);
        v.place_looked_up(14); // next move, destination 14
        v.set("flagvue1", 3);
        v.departure_scanned();
        assert_eq!(v.get("flagvue1"), 0);
        assert!(v.take_abort()); // the stale AbortDepart cancels it
        assert_eq!(v.get("arriveevue"), 0);
        assert_eq!(v.get("abortdepart"), 0);
        assert_eq!(v.get("departvue"), 0);
        v.place_looked_up(14);
        assert!(!v.take_abort());
        assert_eq!(v.get("departvue"), 14);
        v.move_ended();
        assert_eq!(v.get("departvue"), 0);
        v.zone999_begin();
        assert_eq!(v.get("departvue"), 999);
        v.zone999_end();
        assert_eq!(v.get("departvue"), 0);
        assert!(vue_queues_arrival_scan(false));
        assert!(!vue_queues_arrival_scan(true));
    }

    #[test]
    fn chapter_reset_keeps_the_persistent_variables() {
        let mut v = Vars::default();
        v.set("chapter", 3);
        v.set("zoom", 1);
        v.set("flagtemp1", 5);
        v.chapter_reset();
        assert_eq!(
            (v.get("chapter"), v.get("zoom"), v.get("flagtemp1")),
            (3, 1, 0)
        );
    }
}

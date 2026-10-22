//! The puzzles' engine code hung on the panorama, and the places the port
//! can enter.
//!
//! - The click hook dispatcher FUN_0049b910 (from the zone click
//!   FUN_0049ca80): by the view's warp prefix (`_strnicmp`, two
//!   characters), its number and the zone type. Tibet's case is the
//!   mandala placing of view 10 (zone types 3, 4 and 50,
//!   @ 0x0049bcca..0x0049bcf5), which never consumes the click.
//! - The mandala hook FUN_004954f0 with FUN_00495590 and FUN_004955e0:
//!   the hand (`ObjetEnMain`) and the zone are kept, then for each of the
//!   six (object, zone, order variable) rows, in the hook's order: when the
//!   zone's name equals the row's zone (an exact, case-sensitive `strcmp`)
//!   and the hand is the row's object (`DefName_GetIdName(1, name)`),
//!   `RunOneCommand("(NewSound=400)")`, `NbMandalaPosed` + 1, and the
//!   row's `OrdreMandalaN` = the new `NbMandalaPosed`. No guard: the same
//!   placement twice counts twice. What follows is chapter 75's (the
//!   click scan of the same frame): the `poseXX` flags, the laid
//!   mandala shown, the hand and the bag emptied, and for a boat the trip
//!   to its world.
//! - The places the port has: Tibet only. A `changewam` to any other place
//!   (a boat's world, Shamballa, the space travel) is refused: the
//!   section stops there, the world goes back to the current place's, the
//!   view's per-view reset runs (as the section's `vue=` would) and the
//!   game stays in the view it was in ([`Event::WorldNotPorted`]).

use atlantis2_data::defnames;
use atlantis2_data::wam::Zone;

use super::{Engine, Event};

/// The warp prefix of Tibet's views (@ 0x005daff8).
const TIBET_PREFIX: &[u8] = b"TI";

/// The view of the mandala hook (@ 0x0049bce9).
const MANDALA_VIEW: u32 = 10;

/// FUN_004954f0's six calls of FUN_00495590: the object, the zone and the
/// order variable (the strings @ 0x005e1dd8..0x005e1edc).
pub const MANDALAS: [(&str, &str, &str); 6] = [
    ("MANDALA_BATEAU_IR", "ZONE_IRB", "OrdreMandala1"),
    ("MANDALA_BATEAU_CH", "ZONE_CHB", "OrdreMandala2"),
    ("MANDALA_BATEAU_YU", "ZONE_YUB", "OrdreMandala3"),
    ("MANDALA_IRLANDE", "ZONE_IRA", "OrdreMandala4"),
    ("MANDALA_CHINE", "ZONE_CHA", "OrdreMandala5"),
    ("MANDALA_YUCATAN", "ZONE_YUA", "OrdreMandala6"),
];

/// The command FUN_00495590 runs (@ 0x005e1efc).
const LAID_SOUND: &[u8] = b"(NewSound=400)";

/// The places the port has, with their world (`MONDE`).
const PORTED_PLACES: [(&str, i32); 1] = [("tibet.wam", 5)];

/// The world of a place the port has, `None` for any other.
pub fn ported_place(wam: &str) -> Option<i32> {
    PORTED_PLACES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(wam))
        .map(|&(_, m)| m)
}

impl Engine {
    /// FUN_0049b910: the puzzles' own handling of a zone click, by warp
    /// prefix and view; true when it consumes the click. Tibet's mandala
    /// hook returns 0 (@ 0x0049bcf9).
    pub(super) fn puzzle_hooks(&mut self, zi: usize, zone: &Zone) -> bool {
        let tibet = self.current_view().is_some_and(|v| {
            v.name.len() >= 2 && v.name.as_bytes()[..2].eq_ignore_ascii_case(TIBET_PREFIX)
        });
        if tibet && self.view == MANDALA_VIEW && matches!(zone.kind, 3 | 4 | 50) {
            let cliczone = self
                .con
                .as_deref()
                .map_or(self.vars.cliczone, |s| s.vars[s.keys.cliczone]);
            self.event(Event::PuzzleHook { zone: zi, cliczone });
            self.mandala_hook(zone.name.as_deref());
        }
        false
    }

    /// FUN_004954f0 on a zone of that name (`None`: a zone of the 8-byte
    /// form, whose record's name is empty, the place's block being
    /// allocated zeroed by `GlobalAlloc(GMEM_ZEROINIT)` @ 0x0040eee0; no
    /// row's name equals it), with the hand as the scripts hold it;
    /// for tests and tools as well (the hook alone, without the dispatcher
    /// and the scans).
    pub fn mandala_hook(&mut self, zone: Option<&str>) {
        let names = &self.names;
        // The rows that match, decided on the hand read once at the start
        // (@ 0x006d2708) and the zone kept (@ 0x006d2714).
        let rows: Vec<&'static str> = {
            let Some(s) = self.con.as_deref() else {
                return;
            };
            let hand = s.vars[s.keys.objetenmain];
            MANDALAS
                .iter()
                .filter(|(object, name, _)| {
                    // The zone's name (record +0x08) against the row's,
                    // byte for byte (the inline `strcmp` @ 0x004955f0).
                    let same = zone.is_some_and(|z| {
                        z.chars().map(|c| c as u32).eq(name.bytes().map(u32::from))
                    });
                    same && names
                        .id(defnames::OBJETS, object.as_bytes())
                        .is_ok_and(|id| id as i32 == hand)
                })
                .map(|&(_, _, var)| var)
                .collect()
        };
        for var in rows {
            self.with_script(|s, h| {
                s.run_command(h, LAID_SOUND);
                if let Some(n) = s.add_var("NbMandalaPosed", 1) {
                    s.set_var(var, n);
                }
            });
        }
    }

    /// The scripts asked for a place the port does not have: noted, and
    /// the walk that asked stops ([`crate::script::Script::travel_refused`]).
    pub(super) fn refuse_place(&mut self, name: &str) {
        log::warn!("{name}: this world is not ported yet, the game stays here");
        self.refused_place = Some(name.to_string());
    }

    /// After a refused place, once the scripts are back in the engine (so
    /// after the lines that ran are noted): the event, the interpreter's
    /// flag cleared, and the world of the current place set
    /// again (`Interp_SetMonde`: the cursor, the objects bank, the texts),
    /// as the section had switched it before its film.
    pub(super) fn after_refused_place(&mut self) {
        let Some(name) = self.refused_place.take() else {
            return;
        };
        self.event(Event::WorldNotPorted(name));
        let monde = ported_place(&self.wam_name);
        self.with_script(|s, h| {
            s.travel_refused = false;
            if let Some(m) = monde {
                if s.vars[s.keys.monde] != m {
                    let k = s.keys.monde;
                    s.vars[k] = m;
                    s.set_monde(h, m, false);
                }
            }
        });
        // The per-view reset the section's `vue=` would have run, on the
        // view the game stays in (the hand emptied, the click variables
        // and the limits back), without entering it again.
        self.reset_view();
    }
}

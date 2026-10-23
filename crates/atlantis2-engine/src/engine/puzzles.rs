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
//! - Ireland's hooks (prefix `IR`): the click hooks of views 2, 6, 42
//!   and 73 and the entry hooks of views 2, 6, 40-42 and 73 are the cross,
//!   the ogham stones, the skull and the blackbird songs, all ported:
//!   the cross (`cross.rs`), the skull (below), the ogham stones
//!   (`ogham.rs`) and the blackbird songs (`blackbird.rs`), each hook run
//!   noted as [`Event::PuzzleRan`]. The dispatcher never consumes an `IR`
//!   click (@ 0x0049baba, @ 0x0049bcf8).
//! - The skull (views 40-42 of `bigile1.wam`): FUN_00493cb0 at each
//!   view entry shows `CRANE_SPRITE`'s frames from the seven-digit state
//!   (@ 0x005e1a4c), FUN_00493d40 on a click in view 42 lays one of five
//!   objects on `CRANE_ZONE`: its frame added, the cursor pair cleared,
//!   its digit written, `(NewSound=7)`. The engine writes no variable and
//!   leaves the object in hand: chapter 7's sections (the click scan of
//!   the same frame) count `fincrane`, take the object and, at the fifth,
//!   play film 2, show Ailill and hide the skull.
//! - The places the port has: Tibet and Ireland. A `changewam` to any
//!   other place (the boats' other worlds, Shamballa, the space travel)
//!   is refused: the section stops there, the world goes back to the
//!   current place's, the view's per-view reset runs (as the section's
//!   `vue=` would) and the game stays in the view it was in
//!   ([`Event::WorldNotPorted`]).

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

/// The prefix of Ireland's views (@ 0x005db008).
const IRELAND_PREFIX: &[u8] = b"IR";

/// The places the port has, with the world (`MONDE`) the scripts set
/// before entering them: Tibet's and Ireland's five places (chapters
/// 2-24). Not `demo.wam`, which only chapter 1 loads: no script jumps to
/// chapter 1, and none of its nine panoramas (`de0001.hnm` ..
/// `de0009.hnm`) is on a disc.
const PORTED_PLACES: [(&str, i32); 6] = [
    ("tibet.wam", 5),
    ("bigile1.wam", 1),
    ("livre.wam", 1),
    ("ilepoete.wam", 1),
    ("ilepoete2.wam", 1),
    ("puit.wam", 1),
];

/// Whether the port has a place (`changewam`, a save's place), by its
/// file name in any case; with the world its scripts set.
pub fn ported_place(wam: &str) -> Option<i32> {
    PORTED_PLACES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(wam))
        .map(|&(_, m)| m)
}

/// The Irish hook FUN_0049b910 calls on a click of a zone of type `kind`
/// in view `view` of a place whose warp names start with `IR` (any case),
/// with `knife` = `OBJETENMAIN` is `COUTEAU_AILILL`: the original's
/// function. Move zones (types 1, 2): views 42, 6, 2, 73 (@ 0x0049ba17..
/// 0x0049ba55); types 3, 4 and 0x32: views 42, 6 (knife in hand), 2, 73
/// (@ 0x0049bbdc..0x0049bc4d). None consumes the click.
pub fn irish_click_hook(view_name: &[u8], view: u32, kind: i32, knife: bool) -> Option<u32> {
    if view_name.len() < 2 || !view_name[..2].eq_ignore_ascii_case(IRELAND_PREFIX) {
        return None;
    }
    match (kind, view) {
        (1 | 2, 42) => Some(0x0049_3d20),
        (1 | 2, 6) => Some(0x0049_6080),
        (1 | 2, 2) => Some(0x0049_4170),
        (1 | 2, 73) => Some(0x0049_59f0),
        (3 | 4 | 0x32, 42) => Some(0x0049_3d40),
        (3 | 4 | 0x32, 6) if knife => Some(0x0049_60a0),
        (3 | 4 | 0x32, 2) => Some(0x0049_4190),
        (3 | 4 | 0x32, 73) => Some(0x0049_5ae0),
        _ => None,
    }
}

/// The Irish entry hook FUN_0049bd50 calls when view `view` of an `IR`
/// place is entered (@ 0x0049bdf0..0x0049be5e): the original's function.
pub fn irish_entry_hook(view_name: &[u8], view: u32) -> Option<u32> {
    if view_name.len() < 2 || !view_name[..2].eq_ignore_ascii_case(IRELAND_PREFIX) {
        return None;
    }
    match view {
        40..=42 => Some(0x0049_3cb0),
        6 => Some(0x0049_6020),
        2 => Some(0x0049_4120),
        73 => Some(0x0049_58f0),
        _ => None,
    }
}

/// The skull's state at start (@ 0x005e1a4c, `.data`): frames 0 and 1
/// shown, 2..6 hidden ('9').
pub const SKULL_START: [u8; 8] = *b"0199999\0";

/// FUN_00493d40's five blocks, in order: the object in hand on
/// `CRANE_ZONE`, and the frame of `CRANE_SPRITE` it shows, which is also
/// the digit written at that position of the state (@ 0x00493d9b
/// `[0x5e1a4e]` = '2' … @ 0x00493f22 `[0x5e1a52]` = '6'; the strings
/// @ 0x005e1a64..0x005e1a98).
pub const SKULL_OBJECTS: [(&str, i32); 5] = [
    ("CRANE_CROIX", 2),
    ("CRANE_RUCHE", 3),
    ("CRANE_PUIT", 4),
    ("CRANE_CHAPELLE", 5),
    ("CRANE_W18", 6),
];

/// The skull's sprite (@ 0x005e1a54), zone (@ 0x005e1aa4) and sound
/// command (@ 0x005e1ab0, FUN_00493f40).
const SKULL_SPRITE: &[u8] = b"CRANE_SPRITE";
const SKULL_ZONE: &[u8] = b"CRANE_ZONE";
const SKULL_SOUND: &[u8] = b"(NewSound=7)";

/// Whether FUN_0049b910 sends a zone click to the mandala hook: a view
/// whose warp name starts with `TI` (`_strnicmp` over 2 characters,
/// @ 0x0049bcda), numbered 10 (@ 0x0049bce9), and a zone of type 3, 4
/// or 50 (the switch @ 0x0049b99d); any other type, view or name: no hook.
pub fn mandala_hook_applies(view_name: &[u8], view: u32, zone_kind: i32) -> bool {
    view_name.len() >= 2
        && view_name[..2].eq_ignore_ascii_case(TIBET_PREFIX)
        && view == MANDALA_VIEW
        && matches!(zone_kind, 3 | 4 | 50)
}

impl Engine {
    /// FUN_0049b910: the puzzles' own handling of a zone click, by warp
    /// prefix and view; true when it consumes the click. Tibet's mandala
    /// hook returns 0 (@ 0x0049bcf9).
    pub(super) fn puzzle_hooks(&mut self, zi: usize, zone: &Zone) -> bool {
        let applies = self
            .current_view()
            .is_some_and(|v| mandala_hook_applies(v.name.as_bytes(), self.view, zone.kind));
        if applies {
            let cliczone = self.hand_scan_cliczone();
            self.event(Event::PuzzleHook { zone: zi, cliczone });
            self.mandala_hook(zone.name.as_deref());
        }
        // Ireland's hooks (@ 0x0049ba03, @ 0x0049bbc2): the cross's, the
        // skull's, the ogham stones' and the blackbird songs' click and
        // move-zone hooks run.
        let knife = self
            .names
            .id(defnames::OBJETS, b"COUTEAU_AILILL")
            .is_ok_and(|id| id as i32 == self.hand_object());
        let hook = self
            .current_view()
            .and_then(|v| irish_click_hook(v.name.as_bytes(), self.view, zone.kind, knife));
        if let Some(function) = hook {
            self.irish_move_hook(function);
            match function {
                // The cross's click (FUN_00494190).
                0x0049_4190 => {
                    let cliczone = self.hand_scan_cliczone();
                    self.event(Event::PuzzleHook { zone: zi, cliczone });
                    self.event(Event::PuzzleRan { function });
                    self.cross_click(zone.value);
                }
                // The skull's click (FUN_00493d40).
                0x0049_3d40 => {
                    self.event(Event::PuzzleRan { function });
                    self.skull_click(zone);
                }
                // The ogham stones (FUN_004960a0), by the zone's value.
                0x0049_60a0 => {
                    self.event(Event::PuzzleRan { function });
                    self.ogham_click(zone.value);
                }
                // The cross's, the skull's and the ogham stones' move-zone
                // hooks (FUN_00494170, FUN_00493d20, FUN_00496080): their
                // whole work is the cursor reset above.
                0x0049_4170 | 0x0049_3d20 | 0x0049_6080 => {
                    self.event(Event::PuzzleRan { function })
                }
                // The blackbird songs' move-zone hook (FUN_004959f0).
                0x0049_59f0 => {
                    self.event(Event::PuzzleRan { function });
                    self.blackbird_leave();
                }
                // Their click (FUN_00495ae0), here again after the zone
                // click's own call.
                0x0049_5ae0 => self.blackbird_click(),
                _ => self.puzzle_not_ported(function),
            }
        }
        false
    }

    /// `CLICZONE` as a hook finds it.
    fn hand_scan_cliczone(&self) -> i32 {
        self.con
            .as_deref()
            .map_or(self.vars.cliczone, |s| s.vars[s.keys.cliczone])
    }

    /// What the Irish move-zone hooks of the cross, the skull and the
    /// ogham stones do: FUN_00493d20 in view 42, FUN_00496080 in view 6
    /// and FUN_00494170 in view 2 set the cursor pair to no bank, frame 0
    /// (FUN_0047bef0(0, 0), @ 0x00493d2f, 0x0049608f, 0x00494179),
    /// whatever is in hand. The blackbird's (FUN_004959f0) is
    /// [`Engine::blackbird_leave`].
    fn irish_move_hook(&mut self, function: u32) {
        if matches!(function, 0x0049_3d20 | 0x0049_6080 | 0x0049_4170) {
            self.clear_cursor_pair();
        }
    }

    /// `OBJETENMAIN` as the scripts hold it.
    fn hand_object(&self) -> i32 {
        self.con
            .as_deref()
            .map_or(self.vars.objet_en_main, |s| s.vars[s.keys.objetenmain])
    }

    /// A hook of a puzzle the port does not have: noted only.
    pub(super) fn puzzle_not_ported(&mut self, function: u32) {
        log::debug!("puzzle hook FUN_{function:08x}: not ported");
        self.event(Event::PuzzleNotPorted { function });
    }

    /// The view entry's puzzle set-up (FUN_0049bd50) for the views the
    /// port has: Ireland's cross (view 2), skull (views 40-42), ogham
    /// stones (view 6) and blackbird songs (view 73) run; Tibet has none.
    pub(super) fn entry_hooks(&mut self) {
        let hook = self
            .current_view()
            .and_then(|v| irish_entry_hook(v.name.as_bytes(), self.view));
        match hook {
            Some(function @ 0x0049_4120) => {
                self.event(Event::PuzzleRan { function });
                self.cross_entry();
            }
            Some(function @ 0x0049_6020) => {
                self.event(Event::PuzzleRan { function });
                self.ogham_entry();
            }
            Some(function @ 0x0049_3cb0) => {
                self.event(Event::PuzzleRan { function });
                self.skull_entry();
            }
            Some(function @ 0x0049_58f0) => {
                self.event(Event::PuzzleRan { function });
                self.blackbird_entry();
            }
            Some(function) => self.puzzle_not_ported(function),
            None => {}
        }
    }

    /// FUN_00493cb0 (the entry of views 40, 41 and 42; its argument, the
    /// view, is not read): `CRANE_SPRITE`'s id kept (@ 0x00493cc0), frames
    /// 0..6 each taken out of its list once when there
    /// (@ 0x00493cc7..0x00493cee), then for each of the seven characters
    /// of the state, in order, the frame `c − '0'` added when it is 0..8
    /// (signed byte, @ 0x00493cf2..0x00493d17): '9' hides, and the list
    /// order is the state's. Public for tests and tools as well.
    pub fn skull_entry(&mut self) {
        // `DefName_GetIdName(1, name)` (FUN_004105a0): an unknown name is
        // the original's fatal error (FUN_00480cd0, @ 0x004106a3: an
        // emergency save, then the game quits); here the scripts stop and
        // so does the hook. The game's data never reach it.
        let id = match self.names.id(defnames::OBJETS, SKULL_SPRITE) {
            Ok(i) => i as i32,
            Err(_) => {
                let msg = format!(
                    "DefName_GetIdName: Nom de #OBJETS# inconnu:{}",
                    String::from_utf8_lossy(SKULL_SPRITE)
                );
                log::error!("{msg}");
                self.with_script(|s, _| s.fail(msg));
                return;
            }
        };
        self.skull_sprite = id;
        for frame in 0..7 {
            if self.anim.has_frame(id, frame) {
                self.anim.sub_frame(id, frame);
            }
        }
        for k in 0..7 {
            let c = i32::from(self.skull[k] as i8) - i32::from(b'0');
            if (0..9).contains(&c) {
                self.anim.add_frame(self.skull_sprite, c);
            }
        }
    }

    /// FUN_00493d40(zone) (a click on a zone of type 3, 4 or 0x32 in view
    /// 42): for each of the five objects in order, when the zone's value
    /// (+0x04) is `CRANE_ZONE`'s index and `OBJETENMAIN` (read again each
    /// time) is the object: its frame added to the list of the id the
    /// last entry kept (@ 0x006d26a8), the cursor pair cleared
    /// (FUN_0047bef0(0, 0)), its digit written in the state, then
    /// `(NewSound=7)` (FUN_00493f40). No guard: the same object twice is
    /// listed twice. The hand and the bag are the scripts'. Public for
    /// tests and tools as well (the hook alone, without the dispatcher
    /// and the scans).
    pub fn skull_click(&mut self, zone: &Zone) {
        let zone_id = self.names.id(defnames::ZONES_CLIC, SKULL_ZONE);
        for (object, frame) in SKULL_OBJECTS {
            let hand = self.hand_object();
            let object = self.names.id(defnames::OBJETS, object.as_bytes());
            let same_zone = zone_id.as_ref().is_ok_and(|&z| z as i32 == zone.value);
            if !same_zone || !object.is_ok_and(|o| o as i32 == hand) {
                continue;
            }
            self.anim.add_frame(self.skull_sprite, frame);
            self.clear_cursor_pair();
            self.skull[frame as usize] = b'0' + frame as u8;
            self.with_script(|s, h| s.run_command(h, SKULL_SOUND));
        }
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

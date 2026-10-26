//! The turtle ring, China's turtle room (`salletortue.wam`, `CH` views
//! 40-64, chapter 46).
//!
//! An engraved ring (`ANNEAU_GRAVE`, a 39-frame place animation in every
//! view of the room) turns a quarter at a click on `TORTUE_GAUCHE` (to
//! the left, frames forward) or `TORTUE_DROITE` (to the right, frames
//! back), one frame per 15 Hz tick, and keeps its frame from view to
//! view. It is the player's aid for the turtle master's indications: no
//! script reads it, and the hooks write no variable. The room's win is
//! chapter 46's count of arrivals.
//!
//! - The state: the puzzle mode 2 (@ 0x006d84d0, save entry 26), the
//!   pending turn count (@ 0x006d84e0, entry 28) and the ring's frame
//!   store (@ 0x006d84e4, entry 29), kept in the save's bytes. Nothing
//!   resets them but the hooks below and a load (the puzzle reset
//!   FUN_0049bf70 runs at start-up and at a load only, and a load copies
//!   the save's entries after it); a new game, which the original starts
//!   only from the start-up's player menu, finds the start-up's zeros.
//! - find_num_spw_anneau FUN_0049c3b0: the **last** loaded slot whose id
//!   is `ANNEAU_GRAVE`'s, whatever its kind; none is the original's fatal
//!   error `find_num_spw_anneau() !!` (an emergency save, then the game
//!   quits): here the scripts stop and so does the hook.
//! - The quit FUN_0049c270: the mode 2 back to 0.
//! - The entry FUN_0049c340 (from FUN_0049bd50 after the view's slots are
//!   loaded): outside views 40..64 the quit; when the mode is not 2, the
//!   mode = 2 and the store = 0; then the ring's slot held (playing byte
//!   0), active whatever the show / hide table says, and at the store's
//!   frame when the store is below its end (a signed test).
//! - The click FUN_0049c290(zone) (from FUN_0049b910, zone types 3, 4
//!   and 0x32, which it never consumes): outside views 40..64 the quit;
//!   while a turn is pending nothing; the zone's value `TORTUE_GAUCHE`:
//!   `(NewSound=218)`, the count 9, or 10 when the store is not 0;
//!   `TORTUE_DROITE`: the same sound, the count −10, or −9 when the store
//!   is 9. The mode is not read.
//! - The tick FUN_0049c400 (FUN_0049bfb0's mode 2, which the warp frame
//!   runs after its moves and scans whenever the mode is not 0): outside
//!   views 40..64 the quit (the place is not tested); the ring's slot
//!   looked up; on the frame dispatcher's 15 Hz flag (@ 0x00656a50, which
//!   it clears): one frame forward (0 after end − 1) or back (end − 1
//!   from 0 or below) towards a count of 0, and the store = the frame.

use atlantis2_data::defnames;

use super::Engine;

/// The puzzle mode of the ring (@ 0x0049c358).
pub const RING_MODE: i32 = 2;

/// The views of the hooks: 40..64 (`0x27 < view < 0x41`, @ 0x0049c348).
const VIEWS: std::ops::RangeInclusive<u32> = 40..=64;

/// The save entries of the count (@ 0x006d84e0) and the store
/// (@ 0x006d84e4).
const COUNT_ENTRY: usize = 28;
const STORE_ENTRY: usize = 29;

/// The ring's object (@ 0x005df834), the zones (@ 0x005e2ed0,
/// 0x005e2eb0), the sound (@ 0x005e2ec0) and the fatal message
/// (@ 0x005e2ee0).
const RING: &[u8] = b"ANNEAU_GRAVE";
const LEFT: &[u8] = b"TORTUE_GAUCHE";
const RIGHT: &[u8] = b"TORTUE_DROITE";
const SOUND: &[u8] = b"(NewSound=218)";
const NO_RING: &str = "find_num_spw_anneau() !!";

/// The view numbers as the hooks compare them (signed).
fn in_room(view: u32) -> bool {
    VIEWS.contains(&view)
}

impl Engine {
    /// The pending turn count (@ 0x006d84e0, save entry 28): positive
    /// forward, negative back.
    pub fn ring_count(&self) -> i32 {
        i32::from_le_bytes(self.save_extra.entry(COUNT_ENTRY).try_into().unwrap())
    }

    /// Sets the pending turn count, as the click and a save's entry 28 do.
    pub fn set_ring_count(&mut self, v: i32) {
        self.save_extra
            .entry_mut(COUNT_ENTRY)
            .copy_from_slice(&v.to_le_bytes());
    }

    /// The ring's frame store (@ 0x006d84e4, save entry 29).
    pub fn ring_store(&self) -> i32 {
        i32::from_le_bytes(self.save_extra.entry(STORE_ENTRY).try_into().unwrap())
    }

    /// Sets the ring's frame store, as the tick and a save's entry 29 do.
    pub fn set_ring_store(&mut self, v: i32) {
        self.save_extra
            .entry_mut(STORE_ENTRY)
            .copy_from_slice(&v.to_le_bytes());
    }

    /// `DefName_GetIdName(section, name)` (FUN_004105a0). An unknown name
    /// is the original's fatal error (FUN_00480cd0 with "DefName_GetIdName:
    /// Nom de %s inconnu:%s", @ 0x004106a3: an emergency save, then the
    /// game quits): here the scripts stop and so does the hook (`None`).
    /// The game's data never reach it.
    fn ring_name_index(&mut self, section: usize, label: &str, name: &[u8]) -> Option<i32> {
        match self.names.id(section, name) {
            Ok(id) => Some(id as i32),
            Err(_) => {
                let msg = format!(
                    "DefName_GetIdName: Nom de {label} inconnu:{}",
                    String::from_utf8_lossy(name)
                );
                self.ring_fatal(msg);
                None
            }
        }
    }

    /// The original's fatal error (FUN_00480cd0): the scripts stop.
    fn ring_fatal(&mut self, msg: String) {
        log::error!("{msg}");
        self.with_script(|s, _| s.fail(msg));
    }

    /// FUN_0049c3b0, find_num_spw_anneau: the last loaded slot whose id is
    /// `ANNEAU_GRAVE`'s index, whatever its kind (@ 0x0049c3d0..0x0049c3e4);
    /// none: the fatal error (@ 0x0049c3eb) and `None`.
    fn ring_slot(&mut self) -> Option<usize> {
        let id = self.ring_name_index(defnames::OBJETS, "#OBJETS#", RING)?;
        let n = self.anim.slots.iter().rposition(|s| s.id == id);
        if n.is_none() {
            self.ring_fatal(NO_RING.to_string());
        }
        n
    }

    /// FUN_0049c270: the mode 2 back to 0 (@ 0x0049c270..0x0049c279).
    fn ring_quit(&mut self) {
        if self.puzzle_mode() == RING_MODE {
            self.set_puzzle_mode(0);
        }
    }

    /// FUN_0049c340 in a view numbered `view` (the current view's +0x10,
    /// which the hook reads): run by the entry of `CH` views 40-64; public
    /// for tests and tools (the hook alone, on the slots loaded).
    pub fn turtle_entry(&mut self, view: u32) {
        if !in_room(view) {
            self.ring_quit();
            return;
        }
        if self.puzzle_mode() != RING_MODE {
            self.set_puzzle_mode(RING_MODE);
            self.set_ring_store(0);
        }
        let Some(n) = self.ring_slot() else {
            return;
        };
        let store = self.ring_store();
        let slot = &mut self.anim.slots[n];
        slot.playing = false;
        slot.active = true;
        if store < slot.end {
            slot.frame = store;
        }
    }

    /// FUN_0049c290 on a zone of value `value` in a view numbered `view`:
    /// run by a click on an action zone of `CH` views 40-64; public for
    /// tests and tools (the hook alone, without the dispatcher and the
    /// scans around it).
    pub fn turtle_click(&mut self, view: u32, value: i32) {
        if !in_room(view) {
            self.ring_quit();
            return;
        }
        if self.ring_count() != 0 {
            return;
        }
        let Some(left) = self.ring_name_index(defnames::ZONES_CLIC, "#ZONES_CLIC#", LEFT) else {
            return;
        };
        if value == left {
            self.with_script(|s, h| s.run_command(h, SOUND));
            let count = if self.ring_store() != 0 { 10 } else { 9 };
            self.set_ring_count(count);
            return;
        }
        let Some(right) = self.ring_name_index(defnames::ZONES_CLIC, "#ZONES_CLIC#", RIGHT) else {
            return;
        };
        if value == right {
            self.with_script(|s, h| s.run_command(h, SOUND));
            let count = if self.ring_store() == 9 { -9 } else { -10 };
            self.set_ring_count(count);
        }
    }

    /// FUN_0049c400 in a view numbered `view`: the puzzle tick of mode 2;
    /// public for tests and tools (the tick alone; [`Engine::frame_flag`]
    /// is the 15 Hz flag it reads and clears).
    pub fn turtle_tick(&mut self, view: u32) {
        if !in_room(view) {
            self.ring_quit();
            return;
        }
        let Some(n) = self.ring_slot() else {
            return;
        };
        if !self.frame_flag {
            return;
        }
        self.frame_flag = false;
        let count = self.ring_count();
        let slot = &mut self.anim.slots[n];
        if count > 0 {
            slot.frame = if slot.frame < slot.end - 1 {
                slot.frame + 1
            } else {
                0
            };
            self.set_ring_count(count - 1);
        } else if count < 0 {
            let from = if slot.frame > 0 { slot.frame } else { slot.end };
            slot.frame = from - 1;
            self.set_ring_count(count + 1);
        }
        let frame = self.anim.slots[n].frame;
        self.set_ring_store(frame);
    }
}

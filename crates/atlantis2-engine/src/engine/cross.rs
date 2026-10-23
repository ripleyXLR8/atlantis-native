//! Brigid's cross, Ireland's view 2 (`bigile1.wam`, chapters 16 and 19).
//!
//! The cross `CROIX_BRIGID`, given by Liam in chapter 19, is laid on one
//! of the view's 25 action zones (type 4, values 1..25) and drawn there:
//! the slot `CROIXBRIGIT_SPRITE` is a frame-table state, one frame per
//! place (frame = value − 1). Laid on the zone of value 1, it sets
//! `FINCROIX` and runs a scan: chapter 16 plays the goddess's film and
//! dialog. A click on the zone that holds it takes it back.
//!
//! - The state (@ 0x006d26b4, save entry 32): the value of the zone that
//!   holds the cross, 0 when it is not laid. No instruction writes it
//!   but the hook's, and a load copies the save's bytes: the port resets
//!   it nowhere, not at a new game either. Whether the original's new
//!   game clears it through some block clear is not settled (no direct
//!   reference does; see `docs/TECHNICAL.md`).
//! - The view entry FUN_00494120 (from FUN_0049bd50, after the slots are
//!   loaded): the slot's id kept (@ 0x006d26b8, `DefName_GetIdName(1,
//!   "CROIXBRIGIT_SPRITE")`); frame 0 taken out of its table when there
//!   (FUN_0047c9c0, FUN_0047c910); then, when the state is > 0, frame
//!   state − 1 added (FUN_0047c880).
//! - The click FUN_00494190(zone) (from FUN_0049b910, zone types 3, 4
//!   and 0x32, which it never consumes), on the zone's value v
//!   (record +4):
//!   - `OBJETENMAIN` = the cross: `OBJETENMAIN` = 0, the hand pair (0, 0)
//!     (FUN_0047c0c0), the hand emptied (FUN_0047bf40), the cross out of
//!     the bag (FUN_00475680), the state = v, frame v − 1 added; at v = 1
//!     `FINCROIX` = 1 and `CtrlScenarIN`; then `RunOneCommand`
//!     `(NewSound=8)` and `(NewSound=9)` (the second stops the first, and
//!     a film the scan started: both play on voice 2);
//!   - else v = the state: frame v − 1 taken out, the cursor pair (the
//!     objects bank, the cross's sprite) (FUN_0047bef0), the cross in hand
//!     (FUN_00475340, whatever was held) and in the bag (FUN_004754f0),
//!     the state = 0, `(NewSound=9)`;
//!   - else `(NewSound=9)` alone.
//!
//!   No guard: the cross laid again while the state is not 0 adds a second
//!   frame and the older one stays drawn for good.

use atlantis2_data::defnames;

use super::{CursorBank, Engine, Event};

/// The cross's slot name and object (@ 0x005e1ac0, 0x005e1b00).
const SPRITE: &[u8] = b"CROIXBRIGIT_SPRITE";
const CROSS: &[u8] = b"CROIX_BRIGID";

/// The save entry of the state (@ 0x006d26b4).
const STATE_ENTRY: usize = 32;

/// The commands of FUN_00494190 (@ 0x005e1ae4, 0x005e1ad4).
const LAID_SOUND: &[u8] = b"(NewSound=8)";
const MOVED_SOUND: &[u8] = b"(NewSound=9)";

impl Engine {
    /// The cross's state (@ 0x006d26b4): the value of the zone holding it,
    /// 0 when it is not laid.
    pub fn cross_state(&self) -> i32 {
        i32::from_le_bytes(self.save_extra.entry(STATE_ENTRY).try_into().unwrap())
    }

    /// Sets the cross's state, as a save's entry 32 does: for tests and
    /// tools (the slot is drawn from it at the next view entry).
    pub fn set_cross_state(&mut self, v: i32) {
        self.save_extra
            .entry_mut(STATE_ENTRY)
            .copy_from_slice(&v.to_le_bytes());
    }

    /// `DefName_GetIdName(1, name)` (FUN_004105a0). An unknown name is
    /// the original's fatal error (FUN_00480cd0 with "DefName_GetIdName:
    /// Nom de %s inconnu:%s", @ 0x004106a3: an emergency save, then the
    /// game quits): here the scripts stop and so does the hook (`None`).
    /// The game's data never reach it.
    fn object_index(&mut self, name: &[u8]) -> Option<i32> {
        match self.names.id(defnames::OBJETS, name) {
            Ok(id) => Some(id as i32),
            Err(_) => {
                let msg = format!(
                    "DefName_GetIdName: Nom de #OBJETS# inconnu:{}",
                    String::from_utf8_lossy(name)
                );
                log::error!("{msg}");
                self.with_script(|s, _| s.fail(msg));
                None
            }
        }
    }

    /// FUN_00494120: the cross drawn where it was laid (run by the entry
    /// of view 2; public for tests and tools: the hook alone).
    pub fn cross_entry(&mut self) {
        let Some(id) = self.object_index(SPRITE) else {
            return;
        };
        self.cross_sprite = id;
        if self.anim.has_frame(id, 0) {
            self.anim.sub_frame(id, 0);
        }
        let state = self.cross_state();
        if state > 0 {
            self.anim.add_frame(id, state - 1);
        }
    }

    /// FUN_00494190 on a zone of value `value` (run by a click on an
    /// action zone of view 2; public for tests and tools: the hook alone,
    /// without the dispatcher and the scans around it).
    pub fn cross_click(&mut self, value: i32) {
        let Some(cross) = self.object_index(CROSS) else {
            return;
        };
        if self.objetenmain() == cross {
            self.set_objetenmain(0);
            self.set_hand_pair(0, false);
            self.drop_hand();
            self.with_script(|s, _| s.bag_remove(cross));
            self.set_cross_state(value);
            self.anim.add_frame(self.cross_sprite, value - 1);
            if value == 1 {
                self.with_script(|s, h| {
                    s.set_var("FINCROIX", value);
                    s.ctrl_scenar_in(h);
                });
            }
            self.with_script(|s, h| {
                s.run_command(h, LAID_SOUND);
                s.run_command(h, MOVED_SOUND);
            });
            self.event(Event::CrossLaid { value });
        } else if value == self.cross_state() {
            self.anim.sub_frame(self.cross_sprite, value - 1);
            let sprite = self.names.param(defnames::OBJETS, CROSS);
            self.set_cursor_pair(CursorBank::Objects, sprite);
            // FUN_00475340: `OBJETENMAIN` and the hand pair (the
            // object's sprite in the objects bank).
            self.set_objetenmain(cross);
            self.set_hand_pair(sprite, true);
            self.with_script(|s, h| {
                s.hold(cross);
                s.bag_add(h, cross, sprite as u8);
            });
            self.set_cross_state(0);
            self.with_script(|s, h| s.run_command(h, MOVED_SOUND));
            self.event(Event::CrossTaken { value });
        } else {
            self.with_script(|s, h| s.run_command(h, MOVED_SOUND));
        }
    }
}

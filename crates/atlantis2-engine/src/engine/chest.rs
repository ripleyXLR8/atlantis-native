//! The Chinese chest (`pz_coffre`), `miniature.wam` views 1-7 (chapter 47).
//!
//! The chest `COFFRE_CHINE` has a slot in each of the seven views: one
//! animated SPW state of five frames, which the puzzle holds still on
//! frame 4 − `frame_coffre_chine` (frame 4 shut, frame 0 open). In view
//! 5, four clicks on the zone `PRENDRE_BAGUETTE` open it one step each
//! (sounds 211-214); at the fourth the chopstick `BAGUETTE` is shown in
//! it; the fifth puts the chopstick in the bag and in hand
//! (`BAGUETTE_PRISE` = 1). Chapter 47's incense sticks then light with it.
//!
//! - The view entry FUN_004938f0 (from FUN_0049bd50 for `SS` views 1-7,
//!   after the slots are loaded): for a view numbered 1..7
//!   (@ 0x004938f8..0x00493900), the chest's slot found
//!   (`DefName_GetIdName(1, "COFFRE_CHINE")`, then FUN_00493aa0: the first
//!   slot of that id, any kind; none is the fatal
//!   `pz_coffre vue %d ! find_idx_pzcoffre(%d) introuvable`), the puzzle
//!   mode set to 5 (@ 0x00493926), the slot's playing byte cleared
//!   (@ 0x0049393b) and its frame = 4 − `frame_coffre_chine`
//!   (@ 0x00493951).
//! - The click FUN_00493960(zone) (from FUN_0049b910 for `SS` views 1-7,
//!   on every zone click before the zone's type is read; it never
//!   consumes the click): the chest's slot found; in view 5
//!   (@ 0x0049397e) the chopstick's slot found as well (fatal when none),
//!   and when the zone's value (+4) is `PRENDRE_BAGUETTE`'s index:
//!   - `frame_coffre_chine` < 4: + 1 (`Interp_AddVar`), then
//!     `RunOneCommand("(NewSound=%d)", 210 + frame_coffre_chine)`
//!     (FUN_004818c0); at 4 the chopstick shown (FUN_0048c610(id, 2, 0):
//!     its first slot of kind 2 active, the show / hide table);
//!   - else, while `BAGUETTE_PRISE` is 0: FUN_004755f0("BAGUETTE") (the
//!     chopstick in the bag, `OBJETENMAIN`, the hand pair: FUN_00475590),
//!     its table row hidden (FUN_0048c6b0(id, 2, 1): the table only), its
//!     slot's active flag cleared (@ 0x00493a5b), `BAGUETTE_PRISE` = 1.
//!
//!   Then in every view the chest's slot as the entry leaves it
//!   (@ 0x00493a6e..0x00493a92), the mode untouched.
//! - The puzzle mode 5 (@ 0x006d84d0, save entry 26) has no tick; the
//!   click dispatcher clears it on the first zone click in an `SS` view
//!   outside 1-7 and runs no hook on that click (@ 0x0049b96d..0x0049b984):
//!   see [`super::puzzles`].
//!
//! No guard: `frame_coffre_chine` above 4 takes the chopstick at the
//! next click (and draws frame 4 − value, before the file's first frame),
//! below 0 counts up with sounds 210 and less; the take puts the
//! chopstick in hand whatever was held (the object held before stays in
//! the bag).

use atlantis2_data::defnames;

use super::Engine;

/// The strings of FUN_004938f0 and FUN_00493960 (@ 0x005e19b0..0x005e1a08).
const CHEST: &[u8] = b"COFFRE_CHINE";
const CHOPSTICK: &[u8] = b"BAGUETTE";
const ZONE: &[u8] = b"PRENDRE_BAGUETTE";
const FRAME_VAR: &str = "frame_coffre_chine";
const TAKEN_VAR: &str = "BAGUETTE_PRISE";

/// The view of the zone (@ 0x0049397e).
const CHEST_VIEW: u32 = 5;

/// The sound of step k is 210 + k (@ 0x004939e0).
const SOUND_BASE: i32 = 0xd2;

/// The puzzle mode the entry sets (@ 0x0049391a).
pub(super) const CHEST_MODE: i32 = 5;

impl Engine {
    /// FUN_00493aa0 (`find_idx_pzcoffre`): the first slot whose id is
    /// `id`, whatever its kind. None: the original's fatal error
    /// (FUN_00480cd0: an emergency save, then the game quits); here the
    /// scripts stop and so does the hook. The game's data never reach it.
    fn chest_find(&mut self, id: i32) -> Option<usize> {
        if let Some(n) = self.anim.slots.iter().position(|s| s.id == id) {
            return Some(n);
        }
        let msg = format!(
            "pz_coffre vue {} ! find_idx_pzcoffre({id}) introuvable",
            self.view
        );
        log::error!("{msg}");
        self.with_script(|s, _| s.fail(msg));
        None
    }

    /// `frame_coffre_chine` (`Interp_ReadVar`, @ 0x00484ff0).
    fn chest_var(&self) -> i32 {
        self.con
            .as_deref()
            .and_then(|s| s.var(FRAME_VAR))
            .unwrap_or(0)
    }

    /// The chest's slot held on its frame (@ 0x0049393b..0x00493951,
    /// @ 0x00493a7c..0x00493a92): not playing, frame 4 −
    /// `frame_coffre_chine`.
    fn chest_hold(&mut self, slot: usize) {
        let frame = 4i32.wrapping_sub(self.chest_var());
        let s = &mut self.anim.slots[slot];
        s.playing = false;
        s.frame = frame;
    }

    /// FUN_004938f0 (run by the entry of `SS` views 1-7; public for tests
    /// and tools: the hook alone).
    pub fn chest_entry(&mut self) {
        if !(1..=7).contains(&self.view) {
            return;
        }
        let Some(id) = self.object_index(CHEST) else {
            return;
        };
        let Some(slot) = self.chest_find(id) else {
            return;
        };
        self.set_puzzle_mode(CHEST_MODE);
        self.chest_hold(slot);
    }

    /// FUN_00493960 on a zone of value `value` (run by every zone click of
    /// `SS` views 1-7; public for tests and tools: the hook alone,
    /// without the dispatcher and the scans around it).
    pub fn chest_click(&mut self, value: i32) {
        let Some(id) = self.object_index(CHEST) else {
            return;
        };
        let Some(chest) = self.chest_find(id) else {
            return;
        };
        if self.view == CHEST_VIEW && !self.chest_view_click(value) {
            return;
        }
        self.chest_hold(chest);
    }

    /// The view-5 part of FUN_00493960 (@ 0x00493988..0x00493a6b); false
    /// when a fatal error stopped it.
    fn chest_view_click(&mut self, value: i32) -> bool {
        let Some(chopstick) = self.object_index(CHOPSTICK) else {
            return false;
        };
        let Some(slot) = self.chest_find(chopstick) else {
            return false;
        };
        let zone = match self.names.id(defnames::ZONES_CLIC, ZONE) {
            Ok(z) => z as i32,
            Err(_) => {
                let msg = "DefName_GetIdName: Nom de #ZONES_CLIC# inconnu:PRENDRE_BAGUETTE";
                log::error!("{msg}");
                self.with_script(|s, _| s.fail(msg.to_string()));
                return false;
            }
        };
        if value != zone {
            return true;
        }
        if self.chest_var() < 4 {
            self.with_script(|s, h| {
                s.add_var(FRAME_VAR, 1);
                let k = s.var(FRAME_VAR).unwrap_or(0);
                let c = format!("(NewSound={})", k.wrapping_add(SOUND_BASE));
                s.run_command(h, c.as_bytes());
                if k == 4 {
                    s.show(h, chopstick, 2, 0);
                }
            });
        } else if self.con.as_deref().and_then(|s| s.var(TAKEN_VAR)) == Some(0) {
            // FUN_004755f0 → FUN_00475590: the bag (FUN_004754f0 with the
            // object's parameter), `OBJETENMAIN`, the hand pair (its
            // sprite in the objects bank, FUN_0047c0c0); the cursor pair
            // is left as it is.
            let sprite = self.names.param(defnames::OBJETS, CHOPSTICK);
            self.with_script(|s, h| s.bag_add(h, chopstick, sprite as u8));
            self.set_objetenmain(chopstick);
            self.set_hand_pair(sprite, true);
            self.with_script(|s, h| s.hide(h, chopstick, 2, true));
            self.anim.slots[slot].active = false;
            self.with_script(|s, _| s.set_var(TAKEN_VAR, 1));
        }
        true
    }
}

//! The subject menu in the warp frame: opened by a click on a character
//! (@ 0x0045a7c0), drawn by the warp draw (@ 0x0047fb20, the pointer
//! @ 0x0047c090) and answered after it (@ 0x0047fd00 in @ 0x0045a9f0).
//!
//! - Opening, in the frame's clicks after the take (@ 0x0045a650): the
//!   sprite hit is a character record (click kind 2) whose subject count is
//!   above 0, the menu is not open, the hand is empty (`OBJETENMAIN` = 0),
//!   neither a full-screen overlay (@ 0x00671efc) nor the bag bar is open:
//!   `CLICPERSO` = the character, the cursor pair `interfac.spr` frame 40,
//!   the menu open. The zone click and the click scan of that frame do not
//!   run (they test the menu), so the click opens the menu and nothing
//!   else.
//! - While open: no mouse look; the centre shows only its marker (no hand,
//!   no cursor); the cursor is not chosen; after the rod and the overlays,
//!   the cells (`subjects` geometry) then the cursor pair at the pointer.
//! - The answer, after the draw, on a click: outside the grid the menu
//!   closes (the turn stopped, the sprite hit and the click dropped, the
//!   hand emptied and the cursor hidden as @ 0x0047bf40, `CLICPERSO` = 0,
//!   `SUJET` = −2); on a cell, `SUJET` = the entry's value and the subject
//!   scan runs (a `dial=` there starts the dialog), then the turn stopped,
//!   the menu closed, the sprite hit and the click dropped. The cursor pair
//!   is kept: frame 40 shows at the centre in the next frame, before the
//!   cursor choice replaces it. `CLICPERSO` and `SUJET` keep their values
//!   until the dialog ends (@ 0x004808a0) or the next click scan.
//! - A right click with the menu open only stops the turn (and moves the
//!   system's cursor to the window's centre, @ 0x0040c3d0, which is not the
//!   game's pointer).
//!
//! The blackbird game of Ireland (mode @ 0x006d2754 = 2) shows a fixed
//! menu of its songs (@ 0x0047fc70, @ 0x00495a70) instead: not here, as
//! that puzzle.

use atlantis2_data::spr::SprBank;

use super::{CursorBank, Engine, Event};
use crate::bag::{UiBank, UiDraw};
use crate::subjects::{cell, first_row, hit};

/// The pointer frame of `interfac.spr`.
const POINTER_FRAME: i32 = 40;

/// The world's subjects bank `/set(monde=N)` loads (@ 0x00484df0).
pub fn subjects_bank_name(monde: i32) -> Option<&'static str> {
    Some(match monde {
        1 => "sujeire.spr",
        2 | 6 => "sujyuc.spr",
        3 => "sujchine.spr",
        4 => "sujshamb.spr",
        5 => "sujtibet.spr",
        _ => return None,
    })
}

impl Engine {
    /// The world's subjects bank changes with `MONDE` (other values keep
    /// it).
    pub(super) fn load_subjects_bank(&mut self, monde: i32) {
        if let Some(name) = subjects_bank_name(monde) {
            self.subjects_bank = self.load_bank(name.as_bytes());
        }
    }

    /// The character whose menu is open (`CLICPERSO`).
    fn subject_character(&self) -> i32 {
        self.con
            .as_ref()
            .map_or(self.vars.clicperso, |s| s.vars[s.keys.clicperso])
    }

    /// The subject count of a character (0 without the scripts).
    pub fn subject_count(&self, character: i32) -> i32 {
        self.con.as_ref().map_or(0, |s| s.subjects.count(character))
    }

    /// FUN_0045a7c0: a click on a character with subjects opens its menu.
    pub(super) fn open_subjects(&mut self) {
        let Some(slot) = self.sprite_hit.and_then(|n| self.anim.slots.get(n)) else {
            return;
        };
        let (id, click) = (slot.id, slot.click);
        if click != 2 || self.subject_menu.open || self.subject_count(id) <= 0 {
            return;
        }
        if self.objetenmain() != 0 || self.overlays.cover || self.bar.open {
            return;
        }
        self.vars.clicperso = id;
        self.set_script_var(|k| k.clicperso, id);
        self.set_cursor_pair(CursorBank::Interface, POINTER_FRAME);
        self.subject_menu.open = true;
        self.event(Event::SubjectMenu {
            character: id,
            count: self.subject_count(id),
        });
    }

    /// FUN_0047fb20: the cells of the open menu, then the cursor pair at
    /// the pointer (FUN_0047c090).
    pub(super) fn subjects_draw(&mut self) {
        let character = self.subject_character();
        let count = self.subject_count(character);
        self.subject_menu.first_row = first_row(count);
        let entries: Vec<_> = match &self.con {
            // A count past the table reads zeros there.
            Some(s) => (0..count.clamp(0, 4096))
                .map(|i| s.subjects.entry(character, i as usize))
                .collect(),
            None => Vec::new(),
        };
        for (i, e) in entries.iter().enumerate() {
            let (x, y) = cell(count, i as i32);
            self.centred(e.value, x + 25, y + 25, e.flag != 1);
            self.centred(0, x + 31, y + 31, false);
        }
        self.pointer_draw();
    }

    /// FUN_0047bdb0 on the subjects bank: frame `frame` centred on (x, y)
    /// by its size (w / 2, h / 2), at 50 % when `half`.
    fn centred(&mut self, frame: i32, x: i32, y: i32, half: bool) {
        let Some(bank) = &self.subjects_bank else {
            log::warn!("AtlSpr_Aff_centre, bank=NULL");
            return;
        };
        let Some(info) = usize::try_from(frame)
            .ok()
            .filter(|&f| f < bank.len())
            .map(|f| size(bank, f))
        else {
            log::warn!("AtlSpr_Aff_centre:num spr incorrect : {frame} (subjects)");
            return;
        };
        let (bank, x, y) = (UiBank::Subjects, x - info.0 / 2, y - info.1 / 2);
        self.ui_draws.push(if half {
            UiDraw::Translucent { bank, frame, x, y }
        } else {
            UiDraw::Sprite { bank, frame, x, y }
        });
    }

    /// The answer of the open menu to this frame's click (@ 0x0045a9f0
    /// after FUN_0047fd00): a subject, or the menu closed.
    pub(super) fn subjects_click(&mut self) {
        if !self.subject_menu.open || !self.click {
            return;
        }
        let character = self.subject_character();
        let count = self.subject_count(character);
        let entry = hit(count, self.subject_menu.first_row, self.pointer);
        match entry {
            None => {
                self.look.stop();
                self.subject_menu.open = false;
                self.sprite_hit = None;
                self.click = false;
                self.drop_hand();
                self.vars.clicperso = 0;
                self.with_script(|s, h| s.subject_chosen(h, None));
                self.event(Event::Subject {
                    character,
                    value: None,
                });
            }
            Some(i) => {
                let value = self
                    .con
                    .as_ref()
                    .map_or(0, |s| s.subjects.entry(character, i).value);
                self.event(Event::Subject {
                    character,
                    value: Some(value),
                });
                self.with_script(|s, h| s.subject_chosen(h, Some(value)));
                self.look.stop();
                self.subject_menu.open = false;
                self.sprite_hit = None;
                self.click = false;
            }
        }
    }
}

/// A sprite's width and height (`GET_SPRT`: 0, 0 for an empty slot).
fn size(bank: &SprBank, frame: usize) -> (i32, i32) {
    bank.info(frame)
        .map_or((0, 0), |i| (i32::from(i.w), i32::from(i.h)))
}

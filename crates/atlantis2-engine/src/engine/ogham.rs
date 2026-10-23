//! The Ogham stones of Ireland (`pz_ogham`, the place's view 6, with
//! `COUTEAU_AILILL` in hand).
//!
//! Fifteen stones in three rows of five, drawn by the frame list of the
//! `OGHAM_SPRITE` slot: stone k (0..14) shows frame 4 k + e, e being one of
//! four engravings, or nothing (0xff). The state is 15 bytes
//! (@ 0x005e20dc, the save's entry 31), the frame of each stone; the slot's
//! list holds the same bytes from the view entry on.
//!
//! - The view entry (FUN_00496020): the slot's id kept (@ 0x006d2768),
//!   frames 0..14 taken out of its list (FUN_0047c9c0, FUN_0047c910), then
//!   the 15 state bytes appended (FUN_0047c880). The list of a slot just
//!   loaded is empty, so it is the state.
//! - A click (FUN_004960a0, from the hook dispatcher on a zone of type 3,
//!   4 or 0x32 when the hand is `COUTEAU_AILILL`), by the zone's value:
//!   99 compares the state with the target (@ 0x005e20ec) and, when equal
//!   and `FINOGHAM` is 0, sets `FINOGHAM` = 1 (no scan: the click scan of
//!   the same frame sees it); 16, 17, 18 turn the engraving of row 0, 1,
//!   2 (the row's first stone's frame + 1 through FUN_0047ca50, 4 / 0x18
//!   / 0x2c wrapping to 0 / 0x14 / 0x28, then the row's stones that are
//!   there take that frame + 4 j); 1, 6, 11 (a row's first stone) put back
//!   a stone with engraving 2 when it and the next are gone, or take it
//!   when the next is gone; 5, 10, 15 (a row's last stone) take it, or
//!   put it back as the previous stone's frame + 4, whatever that is (a
//!   missing previous stone, 0xff, gives frame 3); any other value, the
//!   generic branch: a missing stone comes back as the previous one + 4
//!   when the previous one is there, a stone is taken when the next one is
//!   gone. Every value from 1 plays `(newsound=4)`; 0 and below do
//!   nothing. The list and the state change together.
//! - The state is not reset by a new game (FUN_0049bf70 has no Ogham
//!   step): only a load writes it.
//!
//! Every reachable row state and its six moves, the entry, the win and
//! the drawing are checked against the original's in
//! `tests/golden/ogham.txt`.

use atlantis2_data::defnames;

use super::Engine;
use crate::anim::{FRAME_TAB, LIST_SKIP};

/// The state at start-up (@ 0x005e20dc): every stone, engraving 2.
pub const OGHAM_START: [u8; 15] = [
    0x02, 0x06, 0x0a, 0x0e, 0x12, 0x16, 0x1a, 0x1e, 0x22, 0x26, 0x2a, 0x2e, 0x32, 0x36, 0x3a,
];

/// The state zone 99 compares with (@ 0x005e20ec).
pub const OGHAM_TARGET: [u8; 15] = [
    0x00, 0x04, 0xff, 0xff, 0xff, 0x17, 0x1b, 0x1f, 0x23, 0x27, 0xff, 0xff, 0xff, 0xff, 0xff,
];

/// The slot's object (@ 0x005e20fc).
const SPRITE: &[u8] = b"OGHAM_SPRITE";

/// The command every click runs (@ 0x005e210c).
const SOUND: &[u8] = b"(newsound=4)";

/// The check's zone value (@ 0x004960e7).
pub const CHECK: i32 = 99;

/// A row's first frame when its engraving turns past the last one
/// (@ 0x00496164, 0x004961bf, 0x00496220), the wrap being that + 4.
const RING_START: [i32; 3] = [0x00, 0x14, 0x28];

/// The frame a row's first stone comes back with (@ 0x004964ff,
/// 0x004963c9, 0x0049629e).
const FIRST_BACK: [u8; 3] = [0x02, 0x16, 0x2a];

/// What a click did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OghamClick {
    /// `(newsound=4)` ran (every value from 1).
    pub sound: bool,
    /// `FINOGHAM` set to 1.
    pub won: bool,
}

/// FUN_004960a0 on zone value `value`: `list` is the first slot of the
/// sprite's id (a zeroed record past the last slot when there is none,
/// as the original's search leaves it), `found` FUN_0047ca50 over every
/// slot of the id (the frame when a list has it, −1 otherwise),
/// `finogham` the variable before the click.
pub fn ogham_click(
    state: &mut [u8; 15],
    list: &mut [u8; FRAME_TAB],
    found: impl FnOnce(i32) -> i32,
    finogham: i32,
    value: i32,
) -> OghamClick {
    let sound = OghamClick {
        sound: true,
        won: false,
    };
    if value == CHECK {
        // @ 0x004960ec..0x00496131: the 15 bytes, then FINOGHAM.
        return OghamClick {
            sound: true,
            won: *state == OGHAM_TARGET && finogham == 0,
        };
    }
    if (16..=18).contains(&value) {
        // @ 0x0049614d, 0x004961a8, 0x00496209.
        let r = (value - 16) as usize;
        let first = 5 * r;
        let mut f = found(i32::from(state[first] as i8)) + 1;
        if f == RING_START[r] + 4 {
            f = RING_START[r];
        }
        for k in first..first + 5 {
            if list[k] != LIST_SKIP {
                list[k] = f as u8;
                state[k] = f as u8;
            }
            f += 4;
        }
        return sound;
    }
    if value < 1 {
        // @ 0x004964ce: no sound.
        return OghamClick::default();
    }
    let k = (value - 1) as usize;
    let gone = |b: u8| b == LIST_SKIP;
    // Past the list the original reads the slot's next fields, none 0xff.
    let at = |l: &[u8; FRAME_TAB], i: usize| l.get(i).copied().unwrap_or(0);
    match value {
        1 | 6 | 11 => {
            // @ 0x004964d6, 0x004963a0, 0x00496275.
            if gone(list[k]) {
                if gone(list[k + 1]) {
                    set(state, list, k, FIRST_BACK[k / 5]);
                }
            } else if gone(list[k + 1]) {
                set(state, list, k, LIST_SKIP);
            }
        }
        5 | 10 | 15 => {
            // @ 0x0049654b, 0x00496418, 0x004962ea: the frame from the
            // list, the state from the state.
            if gone(list[k]) {
                list[k] = list[k - 1].wrapping_add(4);
                state[k] = state[k - 1].wrapping_add(4);
            } else {
                set(state, list, k, LIST_SKIP);
            }
        }
        _ => {
            // The generic branch (@ 0x004965a8, 0x0049646f, 0x00496347).
            if k < FRAME_TAB && gone(list[k]) {
                if !gone(list[k - 1]) {
                    list[k] = list[k - 1].wrapping_add(4);
                    match state.get(k - 1).copied() {
                        Some(p) if k < 15 => state[k] = p.wrapping_add(4),
                        _ => log::warn!("Ogham: byte {k} past the state"),
                    }
                }
            } else if !gone(at(list, k)) && gone(at(list, k + 1)) {
                set(state, list, k, LIST_SKIP);
            }
        }
    }
    sound
}

/// A stone's byte written into the list and the state alike.
fn set(state: &mut [u8; 15], list: &mut [u8; FRAME_TAB], i: usize, b: u8) {
    list[i] = b;
    match state.get_mut(i) {
        Some(s) => *s = b,
        // Not reachable: the list's bytes past the 15th are 0xfe, so no
        // branch writes there.
        None => log::warn!("Ogham: byte {i} past the state"),
    }
}

impl Engine {
    /// FUN_00496020, the view entry: the sprite's id kept for the clicks
    /// (@ 0x006d2768); frames 0..14 taken out of its slots' lists where
    /// one has them (FUN_0047c9c0, FUN_0047c910); the 15 state bytes
    /// appended to each (FUN_0047c880, signed bytes).
    pub fn ogham_entry(&mut self) {
        let id = self
            .names
            .id(defnames::OBJETS, SPRITE)
            .map_or(-1, |i| i as i32);
        self.ogham_sprite = id;
        for frame in 0..15 {
            if self.anim.has_frame(id, frame) {
                self.anim.sub_frame(id, frame);
            }
        }
        for b in self.ogham {
            self.anim.add_frame(id, i32::from(b as i8));
        }
    }

    /// FUN_004960a0, a click on a zone of value `value` with the knife in
    /// hand: the stones, `FINOGHAM` on a win, the sound.
    pub fn ogham_click(&mut self, value: i32) -> OghamClick {
        let id = self.ogham_sprite;
        let slot = self.anim.slots.iter().position(|s| s.id == id);
        if slot.is_none() {
            log::warn!("pz_oghamgereclic ID introuvable :{id}");
        }
        let mut list = slot.map_or([0; FRAME_TAB], |n| self.anim.slots[n].frames);
        let finogham = self
            .con
            .as_deref()
            .and_then(|s| s.var("FINOGHAM"))
            .unwrap_or(0);
        // FUN_0047ca50: the frame when a list of the id has it (any of
        // its 128 bytes, signed), else −1.
        let anim = &self.anim;
        let found = |f: i32| if anim.has_frame(id, f) { f } else { -1 };
        let r = ogham_click(&mut self.ogham, &mut list, found, finogham, value);
        if let Some(n) = slot {
            self.anim.slots[n].frames = list;
        }
        self.with_script(|s, h| {
            if r.won {
                s.set_var("FINOGHAM", 1);
            }
            if r.sound {
                s.run_command(h, SOUND);
            }
        });
        r
    }
}

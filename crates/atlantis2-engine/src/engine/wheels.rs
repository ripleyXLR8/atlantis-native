//! The five wheels and the bridge, China's `miniature.wam` views 12-13
//! (chapter 55).
//!
//! Five discs `DISQUE1..5` (given by the figurines of chapter 48) are laid
//! on an axle of five places, the action zones `AXE_5ROUES1..5` of view 13.
//! Each disc has its own wheel sprite `RoueK1..5`, a one-frame state of 5
//! frames, one per place; the variable `pos_roue<k>` holds the place of
//! disc k (1..5, 0 when not laid). The order 1, 2, 3, 4, 5 half lowers the
//! bridge (film 24, `findisque1`), then 5, 3, 1, 4, 2 lowers it (film 25,
//! `findisque2`): chapter 55 then lets the move from view 12 to 14 go on
//! to chapter 56 instead of aborting it.
//!
//! - The pairing of discs and wheels (@ 0x005e0f7c and @ 0x005e0f68): wheel
//!   `RoueK<i+1>` shows disc [`WHEEL_DISC`]`[i]` + 1, and disc j + 1 is
//!   drawn by wheel [`DISC_WHEEL`]`[j]` + 1.
//! - The view entry FUN_0048d080 (from FUN_0049bd50 for `SS` views 12 and
//!   13, after the slots are loaded): the first animation record of each
//!   `RoueK<k>` kept (FUN_0047de60, @ 0x006d189c..0x006d18ac), then each
//!   wheel hidden when its disc's `pos_roue` is 0, otherwise held still
//!   (+0xbc playing = 0) at frame `pos_roue` − 1 and shown.
//! - The click FUN_0048d170(zone) (from FUN_0049b910, `SS` view 13, zone
//!   types 3, 4 and 0x32, which it never consumes): nothing once
//!   `findisque2` = 1, nor on a zone whose name does not start with
//!   `AXE_5ROUES` (`strncmp`, case-sensitive); the place d is the name's
//!   11th byte − '0' (the code subtracts '1' for the frame d − 1, @
//!   0x0048d1b4..0x0048d1b9). Every shown wheel at frame d − 1 is hidden and its
//!   disc's `pos_roue` = 0, the last one kept to be taken; then the disc in
//!   hand, if any, is laid: its wheel at frame d − 1, held and shown, the
//!   disc out of the hand and the bag (FUN_004756f0), its `pos_roue` = d,
//!   the check FUN_0048d310; then the disc taken goes to the bag and the
//!   hand (FUN_00475590). So a disc laid on a taken place swaps.
//! - The check FUN_0048d310: with `findisque1` = 0 and the order 1..5,
//!   `Pont_Leve` hidden, `Pont_Moitie` shown, `findisque1` = 1,
//!   `(cinema=24)`; else with `findisque2` = 0 and the order 5, 3, 1, 4, 2,
//!   `Pont_Moitie` hidden, `Pont_Baisse` shown, `findisque2` = 1,
//!   `(cinema=25)`.
//!
//! No engine state is saved: the wheels are drawn again from the
//! `pos_roue` variables at each entry. Kept from the original: two discs
//! on one place (only scripts could do it) are both taken and only the
//! last reaches the hand and the bag; the place comes from the name's
//! 11th byte whatever it is (a name of 10 characters, its 11th byte the
//! NUL, gives the frame −49 and `pos_roue` −48).

use atlantis2_data::defnames;
use atlantis2_data::wam::Zone;

use super::{Engine, Event};

/// The disc each wheel shows, less one (@ 0x005e0f7c): `RoueK1` disc 4,
/// `RoueK2` disc 5, `RoueK3` disc 1, `RoueK4` disc 2, `RoueK5` disc 3.
pub const WHEEL_DISC: [usize; 5] = [3, 4, 0, 1, 2];

/// The wheel of each disc, less one (@ 0x005e0f68).
pub const DISC_WHEEL: [usize; 5] = [2, 3, 4, 0, 1];

/// The two orders of FUN_0048d310: `pos_roue1..5` for the half-lowered
/// bridge, then for the lowered one.
pub const WHEELS_FIRST: [i32; 5] = [1, 2, 3, 4, 5];
pub const WHEELS_SECOND: [i32; 5] = [5, 3, 1, 4, 2];

/// The zone names' prefix (@ 0x005e0fe4) and its length (`push 0xa`).
const AXLE: &[u8] = b"AXE_5ROUES";

/// The fatal of FUN_0047de60 (@ 0x005df4f0).
fn not_found(view: u32, id: i32) -> String {
    format!("AtlSpw_find_index(): vue {view} ! AtlSpw_find_index({id}) introuvable")
}

impl Engine {
    /// `DefName_GetIdName(1, name)`, fatal when unknown (see
    /// `object_index`); `None` then.
    fn wheels_object(&mut self, name: &str) -> Option<i32> {
        match self.names.id(defnames::OBJETS, name.as_bytes()) {
            Ok(id) => Some(id as i32),
            Err(_) => {
                let msg = format!("DefName_GetIdName: Nom de #OBJETS# inconnu:{name}");
                log::error!("{msg}");
                self.with_script(|s, _| s.fail(msg));
                None
            }
        }
    }

    /// `Interp_ReadVar` (FUN_00484ff0) of a variable the game defines.
    fn wheels_var(&self, name: &str) -> i32 {
        self.con.as_deref().and_then(|s| s.var(name)).unwrap_or(0)
    }

    fn set_wheels_var(&mut self, name: &str, v: i32) {
        self.with_script(|s, _| s.set_var(name, v));
    }

    /// FUN_0048d080: the wheels drawn from `pos_roue1..5` (run by the
    /// entry of `SS` views 12 and 13; public for tests and tools: the hook
    /// alone).
    pub fn wheels_entry(&mut self) {
        for k in 0..5 {
            let Some(id) = self.wheels_object(&format!("RoueK{}", k + 1)) else {
                return;
            };
            // FUN_0047de60: the first record of the id, any kind; none is
            // fatal (@ 0x0047de94: an emergency save, then the game quits).
            match self.anim.slots.iter().position(|s| s.id == id) {
                Some(n) => self.wheel_slots[k] = n,
                None => {
                    let msg = not_found(self.view, id);
                    log::error!("{msg}");
                    self.with_script(|s, _| s.fail(msg));
                    return;
                }
            }
        }
        for (i, &disc) in WHEEL_DISC.iter().enumerate() {
            let v = self.wheels_var(&format!("pos_roue{}", disc + 1));
            let Some(s) = self.anim.slots.get_mut(self.wheel_slots[i]) else {
                return;
            };
            if v == 0 {
                s.active = false;
            } else {
                s.playing = false;
                s.frame = v - 1;
                s.active = true;
            }
        }
    }

    /// FUN_0048d170 on `zone` (run by a click on an action zone of view
    /// 13; public for tests and tools: the hook alone, without the
    /// dispatcher and the scans).
    pub fn wheels_click(&mut self, zone: &Zone) {
        if self.wheels_var("findisque2") == 1 {
            return;
        }
        // The name as the record holds it (Latin-1, NUL-ended): compared
        // over 10 bytes, then its 11th byte (record +0x12) read.
        let name: Vec<u8> = zone
            .name
            .as_deref()
            .map(|n| n.chars().map(|c| c as u32 as u8).collect())
            .unwrap_or_default();
        let byte = |k: usize| name.get(k).copied().unwrap_or(0);
        if (0..AXLE.len()).any(|k| byte(k) != AXLE[k]) {
            return;
        }
        let place = i32::from(byte(AXLE.len()) as i8) - i32::from(b'1');
        let hand = self.wheels_var("ObjetEnMain");
        let mut taken = None;
        for (i, &disc) in WHEEL_DISC.iter().enumerate() {
            // The records the last entry kept (the entry of view 13 always
            // runs before its clicks).
            let Some(s) = self.anim.slots.get_mut(self.wheel_slots[i]) else {
                return;
            };
            if s.frame != place || !s.active {
                continue;
            }
            s.active = false;
            let Some(first) = self.wheels_object("DISQUE1") else {
                return;
            };
            taken = Some(first + disc as i32);
            self.set_wheels_var(&format!("pos_roue{}", disc + 1), 0);
        }
        let mut laid = None;
        for j in 0..5 {
            let Some(id) = self.wheels_object(&format!("DISQUE{}", j + 1)) else {
                return;
            };
            if hand == id {
                laid = Some(j);
                break;
            }
        }
        if let Some(j) = laid {
            let Some(s) = self.anim.slots.get_mut(self.wheel_slots[DISC_WHEEL[j]]) else {
                return;
            };
            s.frame = place;
            s.active = true;
            s.playing = false;
            if hand != 0 {
                // FUN_004756f0: out of the hand (the hand pair only) and
                // the bag.
                self.set_objetenmain(0);
                self.set_hand_pair(0, false);
                self.with_script(|s, _| s.bag_remove(hand));
            }
            self.set_wheels_var(&format!("pos_roue{}", j + 1), place + 1);
            self.event(Event::PuzzleRan {
                function: 0x0048_d310,
            });
            self.wheels_check();
        }
        if let Some(id) = taken {
            // FUN_00475590: into the bag, then in hand (`OBJETENMAIN` and
            // the hand pair: its sprite in the objects bank).
            let sprite = self.names.param_of(defnames::OBJETS, id as usize);
            self.with_script(|s, h| s.bag_add(h, id, sprite as u8));
            self.set_objetenmain(id);
            self.set_hand_pair(sprite, true);
        }
    }

    /// FUN_0048d310: the two orders and the bridge (public for tests).
    pub fn wheels_check(&mut self) {
        let (order, hide, show, var, film): (_, &[u8], &[u8], _, &[u8]) =
            if self.wheels_var("findisque1") == 0 {
                (
                    WHEELS_FIRST,
                    b"Pont_Leve",
                    b"Pont_Moitie",
                    "findisque1",
                    b"(cinema=24)",
                )
            } else if self.wheels_var("findisque2") == 0 {
                (
                    WHEELS_SECOND,
                    b"Pont_Moitie",
                    b"Pont_Baisse",
                    "findisque2",
                    b"(cinema=25)",
                )
            } else {
                return;
            };
        // Each variable read in turn, the first difference ending it.
        for (k, &v) in order.iter().enumerate() {
            if self.wheels_var(&format!("pos_roue{}", k + 1)) != v {
                return;
            }
        }
        let hide = String::from_utf8_lossy(hide).into_owned();
        let show = String::from_utf8_lossy(show).into_owned();
        let Some(hidden) = self.wheels_object(&hide) else {
            return;
        };
        let Some(shown) = self.wheels_object(&show) else {
            return;
        };
        self.with_script(|s, h| {
            // FUN_0048c6b0(id, 2, 0) and FUN_0048c610(id, 2, 0).
            s.hide(h, hidden, 2, false);
            s.show(h, shown, 2, 0);
            s.set_var(var, 1);
            s.run_command(h, film);
        });
    }
}

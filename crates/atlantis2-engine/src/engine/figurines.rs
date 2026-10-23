//! The figurines on the floor of China's miniature garden (`miniature.wam`,
//! `SS` views 1-7; chapters 47, 48 and 55).
//!
//! Six figurines are taken in view 13 (the objects `OB_FIG1HERBE` ..
//! `OB_FIG6HERBE`, click kind 1) and laid on the six places of view 6's
//! floor (the zones `SOL_FIGUR1` .. `SOL_FIGUR6`). On the floor, figurines
//! 1-5 are the characters `FIGURINE1..5` and the sixth is the object
//! `OB_FIG6HERBE`, each a slot with one frame per place. A figurine on its
//! own place stays there; in view 7 it comes alive (its animation 2 plays,
//! a character the player can talk to: chapter 48). All six on their
//! places set `etat5statues`, which no chapter reads.
//!
//! - The state is the scripts': `pos_statue1..6` (the place of each
//!   figurine, 0 = not laid) and `etat5statues`. The slot indexes the
//!   entry keeps (@ 0x006d18b8..0x006d18cc) are never saved.
//! - The entry FUN_0048d680 (every `SS` view 1-7, from FUN_0049bd50 after
//!   the slots are loaded): figurine k's slot is the first of its id
//!   (FUN_0047de60); in view 7, for a figurine on its target, the first of
//!   its id playing animation 2 (FUN_0047dea0), whose +0x34 and playing
//!   byte are set (@ 0x0048d6d2..0x0048d6d8). Then for each figurine, in
//!   order: `pos_statue` 0 hides its slot; otherwise the slot is shown and,
//!   in view 7 on its target, set to frame 0 and playing
//!   (@ 0x0048d9d8..0x0048d9e2), else to frame `pos_statue` − 1 and
//!   stopped (@ 0x0048d9ed..0x0048d9f5).
//! - The click FUN_0048da20(zone) (view 6, any zone, before the zone type
//!   is read; it never takes the click): nothing while `etat5statues` is
//!   1 or when the zone's name does not start with `SOL_FIGUR` (a
//!   case-sensitive `strncmp` of 9 bytes); its tenth byte is the place
//!   digit d. For each figurine shown at frame d − 1 (@ 0x0048da9f): on
//!   its target, the click ends there; otherwise it is hidden, put in hand
//!   (`OBJETENMAIN` = `OB_FIG1HERBE`'s index − 1 + k, the hand and cursor
//!   pairs on its sprite) and in the bag, and `(pos_statue<k>=0)`. Then
//!   when the object held before the click is a figurine, it is laid:
//!   frame d − 1, shown, stopped, out of the bag, `(NewSound=202)`; the
//!   hand emptied unless a figurine was taken with something held
//!   (@ 0x0048db56: a swap keeps the taken one in hand);
//!   `(pos_statue<k>=<d>)` and the check.
//! - The check FUN_0048dcb0: while `etat5statues` is 0, all six
//!   `pos_statue` on their targets (@ 0x005e1084: 4, 3, 5, 1, 2, 6) run
//!   `(etat5statues=1)` and `AtlHnm_start("???")`, which opens nothing
//!   (no such file: the music back to 100 %, no film).

use atlantis2_data::defnames;
use atlantis2_data::wam::Zone;

use super::{CursorBank, Engine};

/// The figurines' entities on the floor (@ 0x005e10ec .. 0x005e10ac): the
/// characters `FIGURINE1..5`, then the object `OB_FIG6HERBE`.
const FIGURES: [(usize, &str); 6] = [
    (defnames::PERSONNAGES, "FIGURINE1"),
    (defnames::PERSONNAGES, "FIGURINE2"),
    (defnames::PERSONNAGES, "FIGURINE3"),
    (defnames::PERSONNAGES, "FIGURINE4"),
    (defnames::PERSONNAGES, "FIGURINE5"),
    (defnames::OBJETS, "OB_FIG6HERBE"),
];

/// Each figurine's place, `pos_statue1..6` (@ 0x005e1084).
pub const FIGURINE_TARGETS: [i32; 6] = [4, 3, 5, 1, 2, 6];

/// The places' zone names (@ 0x005e1150) and the strings of the click and
/// the check (@ 0x005e1140, 0x005e1130, 0x005e110c).
const PLACE: &[u8] = b"SOL_FIGUR";
const FIRST_OBJECT: &[u8] = b"OB_FIG1HERBE";
const LAID_SOUND: &[u8] = b"(NewSound=202)";
const WON: &[u8] = b"(etat5statues=1)";
/// The video the check starts (@ 0x005e116c): no file has that name.
const WIN_VIDEO: &str = "???";

/// The view where the figurines on their places come alive.
const ALIVE_VIEW: u32 = 7;

/// The name of figurine k's object (`OB_FIG%dHERBE`, @ 0x005e1130).
fn object_name(k: usize) -> String {
    format!("OB_FIG{k}HERBE")
}

impl Engine {
    /// `DefName_GetIdName(section, name)` (FUN_004105a0); an unknown name
    /// is the original's fatal error (@ 0x004106a3): the scripts stop, and
    /// so does the hook (`None`). The game's data never reach it.
    fn figurine_name_id(&mut self, section: usize, name: &[u8]) -> Option<i32> {
        match self.names.id(section, name) {
            Ok(id) => Some(id as i32),
            Err(_) => {
                let sec = ["#PERSONNAGES#", "#OBJETS#"][section.min(1)];
                let msg = format!(
                    "DefName_GetIdName: Nom de {sec} inconnu:{}",
                    String::from_utf8_lossy(name)
                );
                log::error!("{msg}");
                self.with_script(|s, _| s.fail(msg));
                None
            }
        }
    }

    /// A script variable by name (FUN_004812c0's index into the
    /// variables), also during a `vue=` line; 0 without the scripts.
    fn figurine_var(&self, name: &str) -> i32 {
        self.script_var_or_lent(name).unwrap_or(0)
    }

    /// FUN_0048dc60(k): whether figurine k (1..6) is on its target.
    fn figurine_on_target(&self, k: usize) -> bool {
        self.figurine_var(&format!("pos_statue{k}")) == FIGURINE_TARGETS[k - 1]
    }

    /// FUN_0047de60 (`anim` = `None`) or FUN_0047dea0: the first slot of
    /// `id` (any kind), with its state's animation `anim`. None is the
    /// original's fatal error (FUN_00480cd0 @ 0x0047de94 / 0x0047dee3,
    /// with the current view).
    fn figurine_slot(&mut self, id: i32, anim: Option<i32>) -> Option<usize> {
        let found = self
            .anim
            .slots
            .iter()
            .position(|s| s.id == id && anim.is_none_or(|a| s.state.anim as i32 == a));
        if found.is_none() {
            let view = self.view;
            let msg = match anim {
                None => format!("AtlSpw_find_index(): vue {view} ! AtlSpw_find_index({id}) introuvable"),
                Some(a) => format!(
                    "AtlSpw_find_index_anum(): vue {view} ! AtlSpw_find_index_anum({id}, {a}) introuvable"
                ),
            };
            log::error!("{msg}");
            self.with_script(|s, _| s.fail(msg));
        }
        found
    }

    /// FUN_0048d680: the figurines placed from `pos_statue1..6` (run by
    /// the entry of `SS` views 1-7; public for tests and tools: the hook
    /// alone).
    pub fn figurines_entry(&mut self) {
        let alive = self.view == ALIVE_VIEW;
        for (k, (section, name)) in FIGURES.iter().enumerate() {
            let Some(id) = self.figurine_name_id(*section, name.as_bytes()) else {
                return;
            };
            let slot = if alive && self.figurine_on_target(k + 1) {
                let Some(n) = self.figurine_slot(id, Some(2)) else {
                    return;
                };
                self.anim.slots[n].single = 1;
                self.anim.slots[n].playing = true;
                n
            } else {
                let Some(n) = self.figurine_slot(id, None) else {
                    return;
                };
                n
            };
            self.figurine_slots[k] = slot;
        }
        for k in 0..6 {
            let pos = self.figurine_var(&format!("pos_statue{}", k + 1));
            let on_target = alive && pos != 0 && self.figurine_on_target(k + 1);
            let Some(s) = self.anim.slots.get_mut(self.figurine_slots[k]) else {
                continue;
            };
            if pos == 0 {
                s.active = false;
            } else if on_target {
                s.active = true;
                s.frame = 0;
                s.playing = true;
            } else {
                s.active = true;
                s.playing = false;
                s.frame = pos.wrapping_sub(1);
            }
        }
    }

    /// FUN_0048da20(zone): a click in view 6 (public for tests and tools:
    /// the hook alone, without the dispatcher and the scans).
    pub fn figurines_click(&mut self, zone: &Zone) {
        if self.figurine_var("etat5statues") == 1 {
            return;
        }
        // The zone's name (record +0x08, zeroed past its end) against
        // `SOL_FIGUR` over 9 bytes, then its tenth byte (signed).
        let name: Vec<u8> = zone
            .name
            .as_deref()
            .map_or(Vec::new(), |n| n.chars().map(|c| c as u32 as u8).collect());
        let byte = |i: usize| name.get(i).copied().unwrap_or(0);
        if (0..PLACE.len()).any(|i| byte(i) != PLACE[i]) {
            return;
        }
        let digit = i32::from(byte(9) as i8);
        let place = digit - i32::from(b'1');
        let held = self.objetenmain();
        let mut swapped = false;
        for k in 1..=6usize {
            let n = self.figurine_slots[k - 1];
            let Some(s) = self.anim.slots.get(n) else {
                continue;
            };
            if s.frame != place || !s.active {
                continue;
            }
            if self.figurine_on_target(k) {
                return;
            }
            self.anim.slots[n].active = false;
            let Some(first) = self.figurine_name_id(defnames::OBJETS, FIRST_OBJECT) else {
                return;
            };
            let id = first - 1 + k as i32;
            self.set_objetenmain(id);
            // FUN_00410e50(1, "OB_FIG%dHERBE"): its sprite in the objects
            // bank, the hand pair (FUN_0047c0c0), the bag (FUN_004754f0,
            // whose byte is that sprite), the cursor pair (FUN_0047bef0).
            let sprite = self
                .names
                .param(defnames::OBJETS, object_name(k).as_bytes());
            self.set_hand_pair(sprite, true);
            self.with_script(|s, h| s.bag_add(h, id, sprite as u8));
            self.set_cursor_pair(CursorBank::Objects, sprite);
            if held != 0 {
                swapped = true;
            }
            let c = format!("(pos_statue{k}=0)");
            self.with_script(|s, h| s.run_command(h, c.as_bytes()));
        }
        let mut laid = None;
        for k in 1..=6usize {
            let Some(id) = self.figurine_name_id(defnames::OBJETS, object_name(k).as_bytes())
            else {
                return;
            };
            if held == id {
                laid = Some(k);
                break;
            }
        }
        let Some(k) = laid else {
            return;
        };
        if let Some(s) = self.anim.slots.get_mut(self.figurine_slots[k - 1]) {
            s.frame = place;
            s.active = true;
            s.playing = false;
        }
        if held != 0 {
            self.with_script(|s, h| {
                s.bag_remove(held);
                s.run_command(h, LAID_SOUND);
            });
        }
        if !swapped {
            self.set_objetenmain(0);
            self.clear_cursor_pair();
            self.set_hand_pair(0, false);
        }
        let c = format!("(pos_statue{k}={})", digit - i32::from(b'0'));
        self.with_script(|s, h| s.run_command(h, c.as_bytes()));
        self.figurines_check();
    }

    /// FUN_0048dcb0: the six on their places while `etat5statues` is 0
    /// (public for tests and tools: the check alone).
    pub fn figurines_check(&mut self) {
        if self.figurine_var("etat5statues") != 0 {
            return;
        }
        if (1..=6).all(|k| self.figurine_on_target(k)) {
            self.with_script(|s, h| s.run_command(h, WON));
            self.start_video(WIN_VIDEO);
        }
    }
}

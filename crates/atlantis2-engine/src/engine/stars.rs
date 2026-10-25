//! The star map, Xibalba's view 121 (`xibalba.wam`, the statues'
//! clearing, chapter 39).
//!
//! The seven stars `ETOILE102..115` taken in the clearing's views are laid
//! on a map of 17 holes, the view's action zones of type 0x32 whose
//! values are the holes 0..16. Each hole has its frame in the object
//! `Etoiles_Puz`, a frame-list state of 17 frames (frame = hole), and
//! its variable `etat_etoile<hole>`: the star laid there, 0 when empty.
//! When the seven holes of [`STAR_HOLES`] hold a star (any star: only
//! "not 0" is tested), `Etoiles_JustFinished` = 1 and a scan: chapter 39
//! plays film 49 and shows the statue of Tezcatlipoca.
//!
//! - The view entry FUN_00494e00 (from FUN_0049bd50, after the slots are
//!   loaded): `Etoiles_Puz`'s id kept (@ 0x006d26ec,
//!   `DefName_GetIdName(1, "Etoiles_Puz")`) and its first record
//!   (FUN_0047de60, any kind, @ 0x006d26f0; none is fatal): held
//!   (+0xbc = 0), frame 0, shown (+0x30 = 1); the id's frame lists
//!   emptied (FUN_0047ca10, every record of the id); then frame h added
//!   (FUN_0047c880) for each hole h of 0..16, in order, whose
//!   `etat_etoile<h>` is not 0 (FUN_00494fe0).
//! - The click FUN_00494e80(zone) (from FUN_0049b910, zone type 0x32,
//!   which it tests again and never consumes): nothing while
//!   `etoiles_fini` = 1. With H = `OBJETENMAIN` and h = the zone's value
//!   (record +4):
//!   - hole h empty and `ETOILE102` ≤ H ≤ `ETOILE115` (by index: the seven
//!     stars are consecutive objects): frame h added, `etat_etoile<h>` = H
//!     (FUN_00495010), H out of the hand and the bag (FUN_004756f0:
//!     `OBJETENMAIN` = 0, the hand pair (0, 0)), `RunOneCommand`
//!     `(pz_etoile_started=1)` and `(NewSound=115)`;
//!   - hole h full and the hand empty: frame h taken out (FUN_0047c910),
//!     `etat_etoile<h>` = 0, the star into the bag and the hand
//!     (FUN_00475590: `OBJETENMAIN` and the hand pair, its sprite in the
//!     objects bank; not the cursor pair), `(NewSound=114)`;
//!   - otherwise nothing;
//!
//!   then, in every case past `etoiles_fini`, the win test.
//! - The win test FUN_00494f60: the holes of [`STAR_HOLES`] read in
//!   order up to the first empty one; all seven full: `Interp_WriteVar`
//!   `Etoiles_JustFinished` = 1 and `CtrlScenarIN`.
//!
//! Nothing is saved but the variables: the entry draws the map again from
//! them. A hole holding a value that is not a star (only scripts could
//! write one) is taken back as that object.

use atlantis2_data::defnames;

use super::{Engine, Event};

/// The seven right holes, in the order the win test reads them
/// (FUN_00494f60, the table built on the stack
/// @ 0x00494f68..0x00494f94).
pub const STAR_HOLES: [i32; 7] = [0, 2, 6, 8, 11, 13, 16];

/// The holes the entry reads (@ 0x00494e6f: 0..0x10).
pub const STAR_HOLE_COUNT: i32 = 17;

/// The map's object (@ 0x005e1ca4), the first and last stars
/// (@ 0x005e1cf4, 0x005e1ce8).
const MAP: &[u8] = b"Etoiles_Puz";
const FIRST_STAR: &[u8] = b"ETOILE102";
const LAST_STAR: &[u8] = b"ETOILE115";

/// The click's commands (@ 0x005e1cd0, 0x005e1cc0, 0x005e1cb0).
const STARTED: &[u8] = b"(pz_etoile_started=1)";
const LAID_SOUND: &[u8] = b"(NewSound=115)";
const TAKEN_SOUND: &[u8] = b"(NewSound=114)";

/// The variables (@ 0x005e1d00, 0x005e1d10) and the holes' format
/// (@ 0x005e1d28).
const DONE: &str = "etoiles_fini";
const JUST_FINISHED: &str = "Etoiles_JustFinished";

/// The map's functions, as the events name them.
pub const STARS_ENTRY: u32 = 0x0049_4e00;
pub const STARS_CLICK: u32 = 0x0049_4e80;
pub const STARS_WIN: u32 = 0x0049_4f60;

/// The zone type the click takes (@ 0x00494e85).
const HOLE_ZONE: i32 = 0x32;

/// The fatal of FUN_0047de60 (@ 0x005df4f0).
fn not_found(view: u32, id: i32) -> String {
    format!("AtlSpw_find_index(): vue {view} ! AtlSpw_find_index({id}) introuvable")
}

fn hole_var(h: i32) -> String {
    format!("etat_etoile{h}")
}

impl Engine {
    /// `Interp_ReadVar` (FUN_00484ff0) of `name`, from the variables a
    /// `vue=` line lends while it runs the entry. An unknown name is
    /// `DefName_GetIdName`'s fatal error (@ 0x004106a3): the scripts stop
    /// and so does the hook (`None`); only a zone of value outside 0..16
    /// would reach it, and the view has none.
    fn stars_var(&mut self, name: &str) -> Option<i32> {
        let v = self.script_var_or_lent(name);
        if v.is_none() {
            let msg = format!("DefName_GetIdName: Nom de #VARIABLES# inconnu:{name}");
            log::error!("{msg}");
            self.with_script(|s, _| s.fail(msg));
        }
        v
    }

    /// FUN_00494e00: the map drawn from `etat_etoile0..16` (run by the
    /// entry of `YU` view 121; public for tests and tools: the hook
    /// alone).
    pub fn stars_entry(&mut self) {
        let Some(id) = self.object_index(MAP) else {
            return;
        };
        self.stars_sprite = id;
        // FUN_0047de60: the first record of the id, any kind; none is
        // fatal (@ 0x0047de94: an emergency save, then the game quits).
        let Some(n) = self.anim.slots.iter().position(|s| s.id == id) else {
            let msg = not_found(self.view, id);
            log::error!("{msg}");
            self.with_script(|s, _| s.fail(msg));
            return;
        };
        let s = &mut self.anim.slots[n];
        s.playing = false;
        s.frame = 0;
        s.active = true;
        self.anim.clear_frames(id);
        for h in 0..STAR_HOLE_COUNT {
            let Some(v) = self.stars_var(&hole_var(h)) else {
                return;
            };
            if v != 0 {
                self.anim.add_frame(id, h);
            }
        }
    }

    /// FUN_00494e80 on a zone of type `kind` and value `hole` (run by a
    /// click on an action zone of view 121; public for tests and tools:
    /// the hook alone, without the dispatcher and the scans around it).
    pub fn stars_click(&mut self, kind: i32, hole: i32) {
        if kind != HOLE_ZONE {
            return;
        }
        let Some(done) = self.stars_var(DONE) else {
            return;
        };
        if done == 1 {
            return;
        }
        let Some(hand) = self.stars_var("ObjetEnMain") else {
            return;
        };
        let Some(held) = self.stars_var(&hole_var(hole)) else {
            return;
        };
        if held == 0 {
            let Some(first) = self.object_index(FIRST_STAR) else {
                return;
            };
            if hand >= first {
                let Some(last) = self.object_index(LAST_STAR) else {
                    return;
                };
                if hand <= last {
                    self.anim.add_frame(self.stars_sprite, hole);
                    self.with_script(|s, _| s.set_var(&hole_var(hole), hand));
                    // FUN_004756f0: out of the hand (the hand pair only)
                    // and the bag.
                    self.set_objetenmain(0);
                    self.set_hand_pair(0, false);
                    self.with_script(|s, h| {
                        s.bag_remove(hand);
                        s.run_command(h, STARTED);
                        s.run_command(h, LAID_SOUND);
                    });
                    self.event(Event::StarLaid { hole, star: hand });
                }
            }
        } else if hand == 0 {
            self.anim.sub_frame(self.stars_sprite, hole);
            self.with_script(|s, _| s.set_var(&hole_var(hole), 0));
            // FUN_00475590: into the bag, then in hand (`OBJETENMAIN` and
            // the hand pair: its sprite in the objects bank).
            let sprite =
                usize::try_from(held).map_or(0, |i| self.names.param_of(defnames::OBJETS, i));
            self.with_script(|s, h| s.bag_add(h, held, sprite as u8));
            self.set_objetenmain(held);
            self.set_hand_pair(sprite, true);
            self.with_script(|s, h| s.run_command(h, TAKEN_SOUND));
            self.event(Event::StarTaken { hole, star: held });
        }
        self.stars_win_test();
    }

    /// FUN_00494f60 (noted [`Event::PuzzleRan`] each time it runs): the
    /// seven right holes all full → `Etoiles_JustFinished` = 1 and a scan
    /// (public for tests and tools).
    pub fn stars_win_test(&mut self) {
        self.event(Event::PuzzleRan {
            function: STARS_WIN,
        });
        for h in STAR_HOLES {
            match self.stars_var(&hole_var(h)) {
                Some(0) | None => return,
                Some(_) => {}
            }
        }
        self.with_script(|s, h| {
            s.set_var(JUST_FINISHED, 1);
            s.ctrl_scenar_in(h);
        });
    }
}

//! Xibalba (`MONDE` 6, disc 4, chapters 35-41): `bat.wam` (the bat's
//! island, chapter 35), `riviere.wam` (the river, 36), `xibalba.wam`
//! (Chac's pond, 37; the statues' clearing, 39) and `toile.wam` (the
//! spider's web, 38), reached from the priestess of chapter 27 and left
//! for the king's hall by chapter 35. Their views share Yucatán's `YU`
//! prefix, so their puzzle hooks are the `YU` branches of `yucatan.rs`.
//!
//! The spider's web (`toile.wam`, views 198-265) runs (`web.rs`): the
//! entry of view 198 FUN_0048f530 (the puzzle mode 1), in that mode the
//! entry FUN_0048f9a0, the move click FUN_0048fdd0, the zone click
//! FUN_00491290, the tick FUN_00491610 and the draw FUN_00491b30, and a
//! save made in the web loads in view 198 (FUN_0048f450 from
//! FUN_004785a0, [`Engine::load_data`]).
//!
//! Its two other puzzles are not ported; their hooks stay listed and
//! noted ([`Event::PuzzleNotPorted`]):
//!
//! - the star map (view 121 of `xibalba.wam`): the entry FUN_00494e00
//!   and the click FUN_00494e80 (zones of type 0x32);
//! - the canoe (view 402 of `bat.wam`): FUN_0049c010(1), the 3D puzzle's
//!   own loop (type 9), started by a click there once `enable_pz_pirog`
//!   is 1 (before, the click is consumed).
//!
//! What each of them writes when won is available as a testing aid, for
//! tests and tools only, so that the story after it can be played:
//! [`Engine::win_star_map`] and [`Engine::win_canoe`]. They write the
//! variables, the bag, the hand and the sounds the original's code writes
//! along a won game of the puzzle, in its order, and nothing it draws.

use atlantis2_data::defnames;

use super::{Engine, Event};

/// The star map's view (@ 0x0049bb1f) and its seven right holes, in the
/// order the win test reads them (FUN_00494f60, the table built on the
/// stack @ 0x00494f6b..0x00494f9a).
pub const STAR_HOLES: [i32; 7] = [0, 2, 6, 8, 11, 13, 16];

/// The first and last star objects (@ 0x005e1cf4, 0x005e1ce8): a star is
/// any object between them, by index (@ 0x00494ec4..0x00494eea).
const FIRST_STAR: &[u8] = b"ETOILE102";
const LAST_STAR: &[u8] = b"ETOILE115";

/// The star map's commands (@ 0x005e1cd0, 0x005e1cc0) and its variables
/// (@ 0x005e1d28 `etat_etoile%d`, 0x005e1d00, 0x005e1d10).
const STAR_STARTED: &[u8] = b"(pz_etoile_started=1)";
const STAR_LAID_SOUND: &[u8] = b"(NewSound=115)";
const STARS_DONE: &str = "etoiles_fini";
const STARS_JUST_FINISHED: &str = "Etoiles_JustFinished";

/// The canoe's start command (FUN_00496a60 @ 0x00496a7f, the string
/// @ 0x005e24fc) and its end's variables (FUN_0049a040, @ 0x005e2cd4,
/// 0x005e2cbc).
const CANOE_STARTED: &[u8] = b"(flag103=1)";
const CANOE_DONE: &str = "finpirogue";
const CANOE_JUST_FINISHED: &str = "Pirogue_JustFinished";

impl Engine {
    /// A testing aid, for tests and tools only (the star map is not
    /// ported): what the seven lays that win the star map write
    /// (FUN_00494e80 on a type-0x32 zone of view 121 with a star in hand,
    /// @ 0x00494e80..0x00494f1f, its value an empty right hole), each
    /// with a star taken from the bag in its order onto the holes of
    /// [`STAR_HOLES`] in order: `etat_etoile<hole>` = the star
    /// (FUN_00495010, Interp_WriteVar), the star out of the hand and the
    /// bag (FUN_004756f0: `OBJETENMAIN` = 0, the hand pair cleared),
    /// `(pz_etoile_started=1)` and `(NewSound=115)` (RunOneCommand); after
    /// the seventh the win (FUN_00494f60 @ 0x00494fc5..0x00494fd2):
    /// `Etoiles_JustFinished` = 1 (Interp_WriteVar) and `CtrlScenarIN`.
    /// The frames the lays add to `Etoiles_Puz`'s list (FUN_0047c880) are
    /// not drawn. Nothing while `etoiles_fini` is 1 (@ 0x00494e8e) or with
    /// fewer than seven stars in the bag (false).
    pub fn win_star_map(&mut self) -> bool {
        let done = self.script_var_or_lent(STARS_DONE).unwrap_or(0);
        let (Ok(first), Ok(last)) = (
            self.names.id(defnames::OBJETS, FIRST_STAR),
            self.names.id(defnames::OBJETS, LAST_STAR),
        ) else {
            return false;
        };
        let stars: Vec<i32> = self
            .bag_entries()
            .iter()
            .map(|b| b.id)
            .filter(|&id| (first as i32..=last as i32).contains(&id))
            .collect();
        if done == 1 || stars.len() < STAR_HOLES.len() {
            return false;
        }
        for (&hole, &star) in STAR_HOLES.iter().zip(&stars) {
            self.event(Event::PuzzleAid {
                function: 0x0049_4e80,
            });
            self.with_script(|s, _| s.set_var(&format!("etat_etoile{hole}"), star));
            self.set_objetenmain(0);
            self.set_hand_pair(0, false);
            self.with_script(|s, h| {
                s.bag_remove(star);
                s.run_command(h, STAR_STARTED);
                s.run_command(h, STAR_LAID_SOUND);
            });
        }
        self.event(Event::PuzzleAid {
            function: 0x0049_4f60,
        });
        self.with_script(|s, h| {
            s.set_var(STARS_JUST_FINISHED, 1);
            s.ctrl_scenar_in(h);
        });
        true
    }

    /// A testing aid, for tests and tools only (the canoe is not ported):
    /// what a canoe started and won writes. The start (FUN_00496a60, from
    /// FUN_0049c010(1)): `(flag103=1)` (RunOneCommand @ 0x00496a7f). The
    /// end (FUN_0049a040 @ 0x0049a040..0x0049a06c): `finpirogue` = 1
    /// (Interp_WriteVar), then when `Pirogue_JustFinished` is 0, it = 1
    /// and `CtrlScenarIN` (chapter 35 then sends the game on). The quit
    /// that follows the won frame (FUN_0049a570 from FUN_00498ca0) clears
    /// the cursor pair (FUN_0047bef0(0, 0)); the loop type 9 and its
    /// return to the loop type it left are not entered.
    pub fn win_canoe(&mut self) {
        self.event(Event::PuzzleAid {
            function: 0x0049_6a60,
        });
        self.with_script(|s, h| s.run_command(h, CANOE_STARTED));
        self.event(Event::PuzzleAid {
            function: 0x0049_a040,
        });
        self.with_script(|s, h| {
            s.set_var(CANOE_DONE, 1);
            if s.var(CANOE_JUST_FINISHED) == Some(0) {
                s.set_var(CANOE_JUST_FINISHED, 1);
                s.ctrl_scenar_in(h);
            }
        });
        self.clear_cursor_pair();
    }
}

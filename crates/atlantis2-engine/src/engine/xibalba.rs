//! Xibalba (`MONDE` 6, disc 4, chapters 35-41): `bat.wam` (the bat's
//! island, chapter 35), `riviere.wam` (the river, 36), `xibalba.wam`
//! (Chac's pond, 37; the statues' clearing, 39) and `toile.wam` (the
//! spider's web, 38), reached from the priestess of chapter 27 and left
//! for the king's hall by chapter 35. Their views share Yucatán's `YU`
//! prefix, so their puzzle hooks are the `YU` branches of `yucatan.rs`.
//!
//! Its star map (view 121 of `xibalba.wam`) is ported (`stars.rs`). Its
//! two other puzzles are not; their hooks stay listed and noted
//! ([`Event::PuzzleNotPorted`]):
//!
//! - the canoe (view 402 of `bat.wam`): FUN_0049c010(1), the 3D puzzle's
//!   own loop (type 9), started by a click there once `enable_pz_pirog`
//!   is 1 (before, the click is consumed);
//! - the spider's web (`toile.wam`, views 198-265): the entry of view 198
//!   FUN_0048f530 (the puzzle mode 1, which the port never sets), and in
//!   that mode the entry FUN_0048f9a0 and the move click FUN_0048fdd0. The
//!   zone click's own call FUN_00491290 returns 0 at once while the web
//!   is not loaded (@ 0x006d2668, set only in the mode 1): the port does
//!   not call it. A save made in the web loads in view 198
//!   (FUN_0048f450 from FUN_004785a0, [`Engine::load_data`]); the rest of
//!   FUN_0048f450 (the stage's variables from the bag, then FUN_0048f530)
//!   is the web's, noted.
//!
//! What each of the two puzzles' wins writes is available as a testing
//! aid, for tests and tools only, so that the story after it can be
//! played: [`Engine::win_canoe`] and [`Engine::win_web_round`]. They
//! write the variables, the bag, the hand and the sounds the original's
//! code writes along a won game of the puzzle, in its order, and nothing
//! it draws.

use super::{Engine, Event};

/// The canoe's start command (FUN_00496a60 @ 0x00496a7f, the string
/// @ 0x005e24fc) and its end's variables (FUN_0049a040, @ 0x005e2cd4,
/// 0x005e2cbc).
const CANOE_STARTED: &[u8] = b"(flag103=1)";
const CANOE_DONE: &str = "finpirogue";
const CANOE_JUST_FINISHED: &str = "Pirogue_JustFinished";

/// The web's stage variables (FUN_0048f930, @ 0x005e14ec, 0x005e14e0,
/// 0x005e14d4).
const WEB_STAGES: [&str; 3] = ["finaraign1", "finaraign2", "finaraign3"];

impl Engine {
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

    /// The web's stage (FUN_0048f930, @ 0x0048f930..0x0048f99a): 1 + the
    /// number of leading `finaraignN` that are not 0, 4 when all three
    /// are.
    pub fn web_stage(&self) -> i32 {
        let v = |n: &str| self.script_var_or_lent(n).unwrap_or(0);
        if v(WEB_STAGES[0]) == 0 {
            1
        } else if v(WEB_STAGES[1]) == 0 {
            2
        } else {
            3 + i32::from(v(WEB_STAGES[2]) != 0)
        }
    }

    /// A testing aid, for tests and tools only (the spider's web is not
    /// ported): what one won round of the web writes, by its stage
    /// ([`Engine::web_stage`]), as the original's code writes it along a
    /// round played to its end: the web entered at view 198
    /// (FUN_0048f530: `(sacados=NOT_EPEE)` @ 0x0048f530; the
    /// `(sacados=PLUME)` after it tests a word no code writes,
    /// @ 0x006d0744, so never), the first view of the web proper
    /// (FUN_0048fd60, stage 3 only: `(flag92=1)`,
    /// `(enablesuj=CHAUVE_SOURIS,DEESSE_ARAIGNEE)`,
    /// `(enablesuj=PRETRESSE,DEESSE_ARAIGNEE)`, @ 0x0048fda8..0x0048fdc1),
    /// the prey reached (FUN_0048f9a0: stage 1 in view 265
    /// `(sacados=CHENILLE)`, `(NewSound=104)` @ 0x0048fb07, 0x0048fb1b;
    /// stages 2 and 4 in view 251 `(sacados=SAUTERELLE)`,
    /// `(NewSound=105)` @ 0x0048fbc6, 0x0048fbda while the word
    /// @ 0x006d2614 is 0, which the web's load (FUN_00490260 from
    /// FUN_0048fd60) sets to 0 at stage 2 and at stage 4 only while
    /// `SAUTERELLE` is not in the bag (FUN_004753d0 @ 0x004904c9; in the
    /// bag it is set to 1 @ 0x00490511, and stage 4 gives nothing); stage
    /// 3 the sword in view 247 `(sacados=EPEE)` @ 0x0048fc3b, the spider
    /// struck with it from view 247 towards 248 (FUN_0048fdd0,
    /// `(cinema=48)` @ 0x0048fed8), the skull in view 265
    /// `(sacados=CRANE_ARAIG2)` @ 0x0048fb60), then the web left by view
    /// 199 or 200 (FUN_0048f9a0 @ 0x0048fc71..0x0048fd4f, a chain of
    /// tests on the stage, each exit recomputing it by FUN_0048f930 for
    /// the tests after: stage 1 `(finaraign1=1)` @ 0x0048fc8b, then the
    /// stage-2 test stops on the word @ 0x006d2614, 0; stage 2
    /// `(finaraign2=1)` @ 0x0048fcbc, then the stage-3 test runs:
    /// `(sacados=NOT_EPEE)` @ 0x0048fcfc, the word @ 0x006d2618 (the
    /// skull taken) 0 and @ 0x006d2624 (the feather laid) 0, so nothing
    /// more; stage 3 `(sacados=NOT_EPEE)` and `(finaraign3=1)`
    /// @ 0x0048fcfc, 0x0048fd12; stage 4 nothing). The three words are
    /// zero data written only by the web's code, by a later stage than
    /// the one they are 0 for here; a game loaded back to an earlier
    /// stage in the same run is not modelled. All by RunOneCommand, no
    /// scan. The stage it played is returned.
    pub fn win_web_round(&mut self) -> i32 {
        let stage = self.web_stage();
        self.event(Event::PuzzleAid {
            function: 0x0048_f9a0,
        });
        let mut commands: Vec<&[u8]> = vec![b"(sacados=NOT_EPEE)"];
        match stage {
            1 => commands.extend([
                &b"(sacados=CHENILLE)"[..],
                b"(NewSound=104)",
                b"(finaraign1=1)",
            ]),
            2 => commands.extend([
                &b"(sacados=SAUTERELLE)"[..],
                b"(NewSound=105)",
                b"(finaraign2=1)",
                b"(sacados=NOT_EPEE)",
            ]),
            3 => commands.extend([
                &b"(flag92=1)"[..],
                b"(enablesuj=CHAUVE_SOURIS,DEESSE_ARAIGNEE)",
                b"(enablesuj=PRETRESSE,DEESSE_ARAIGNEE)",
                b"(sacados=EPEE)",
                b"(cinema=48)",
                b"(sacados=CRANE_ARAIG2)",
                b"(sacados=NOT_EPEE)",
                b"(finaraign3=1)",
            ]),
            _ => {
                let Some(hopper) = self.object_index(b"SAUTERELLE") else {
                    return stage;
                };
                if !self.bag_entries().iter().any(|b| b.id == hopper) {
                    commands.extend([&b"(sacados=SAUTERELLE)"[..], b"(NewSound=105)"]);
                }
            }
        }
        for c in commands {
            self.with_script(|s, h| s.run_command(h, c));
        }
        stage
    }
}

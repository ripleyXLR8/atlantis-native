//! Shamballa's and Atlantis's puzzle hooks (`MONDE` 4): the dispatchers'
//! branches for the views whose warp names start with `SH` (@ 0x005daffc,
//! `voiles.wam`) and `AT` (@ 0x005e2ea8, `atlantis.wam`), `_strnicmp` over
//! two characters, any case; and the end of a move without a next view in
//! `voiles.wam`. `rhea.wam` (`RH`) and `annulair.wam` (`AN`) have none.
//!
//! - **Click** (FUN_0049b910, move zones, types 1 and 2,
//!   @ 0x0049ba5a..0x0049ba84, after Ireland's and before `SS` 31's):
//!   `SH` view 1, the fireflies' click FUN_00495210 (catalogue row 17),
//!   which acts on type 1 only (@ 0x0049521e). The dispatcher returns 0:
//!   the move goes on (the zones of view 1 lead to views 1..6, of which
//!   only view 1 is in the file).
//! - **Entry** (FUN_0049bd50, @ 0x0049bec6..0x0049bef2, after `LA`'s and
//!   before `SS`'s): `AT` views 1..19, the octopus's FUN_0049a8c0
//!   (catalogue row 18), which sets the puzzle mode 6; its tick
//!   FUN_0049aaa0 runs in that mode only (FUN_0049bfb0), its cursor
//!   FUN_0049a9a0 returns 0 outside it.
//! - **A move's end without a next view** (FUN_0045a3b0 @ 0x0045a5d0 →
//!   FUN_0045a630): when the current place is `voiles.wam` (`_stricmp`
//!   with the name @ 0x00659168), FUN_004952d0, ported: the camera's field
//!   of view back to 76° (FUN_004a4950(0x3fa9c91f, W, H, W, H): the zoom
//!   of chapter 78's firefly moves may have left it at 35°, there being
//!   no view entry to set it back), then, when `Puzzle_Lucioles_Fini` is
//!   1, `Puzzle_Lucioles_AutoWin` = 1 and a scan (FUN_00483c90): chapter
//!   78's way to `rhea.wam`.
//!
//! The fireflies' click and the octopus's entry are not ported: noted
//! ([`Event::PuzzleNotPorted`]), none takes the click, the mode 6 is never
//! set. What they would write is the testing aids'
//! ([`Engine::win_fireflies`], [`Engine::end_octopus_fight`]).

use atlantis2_data::wam::Zone;

use super::{Engine, Event};

/// The prefix of `voiles.wam`'s views (@ 0x005daffc) and of
/// `atlantis.wam`'s (@ 0x005e2ea8).
const SHAMBALLA_PREFIX: &[u8] = b"SH";
const ATLANTIS_PREFIX: &[u8] = b"AT";

/// The fireflies' view (@ 0x0049ba79, EBX = 1).
const FIREFLIES_VIEW: u32 = 1;

/// The fireflies' click (FUN_00495210) and their move end (FUN_004952d0).
pub const FIREFLIES_CLICK: u32 = 0x0049_5210;
pub const FIREFLIES_MOVE_END: u32 = 0x0049_52d0;

/// The octopus's entry (FUN_0049a8c0), tick (FUN_0049aaa0) and puzzle
/// mode (@ 0x0049a8cf).
pub const OCTOPUS_ENTRY: u32 = 0x0049_a8c0;
pub const OCTOPUS_TICK: u32 = 0x0049_aaa0;
pub const OCTOPUS_MODE: i32 = 6;

/// The place FUN_0045a630 compares with (@ 0x005db944).
const FIREFLIES_PLACE: &str = "voiles.wam";

/// The field of view FUN_004952d0 sets, 76° in radians (0x3fa9c91f).
const FOV_76: f32 = f32::from_bits(0x3fa9_c91f);

/// The fireflies' variables (@ 0x005e1dac, 0x005e1d50, 0x005e1dc0).
const LIT: &str = "NbLucioleAllumed";
const DONE: &str = "Puzzle_Lucioles_Fini";
const AUTO_WIN: &str = "Puzzle_Lucioles_AutoWin";

/// The octopus's ends (@ 0x005e2d5c, 0x005e2d4c): its death, the hero's.
const OCTOPUS_DEAD: &[u8] = b"(chapter=200)";
const HERO_DEAD: &[u8] = b"(chapter=201)";

fn prefixed(view_name: &[u8], prefix: &[u8]) -> bool {
    view_name.len() >= 2 && view_name[..2].eq_ignore_ascii_case(prefix)
}

/// The `SH` part of FUN_0049b910 for a click on a zone of type `kind` in
/// view `view` of a place whose warp names start as `view_name`: the
/// fireflies' click on a move zone of view 1.
pub fn shamballa_click_hooks(view_name: &[u8], view: u32, kind: i32) -> Vec<u32> {
    match kind {
        1 | 2 if prefixed(view_name, SHAMBALLA_PREFIX) && view == FIREFLIES_VIEW => {
            vec![FIREFLIES_CLICK]
        }
        _ => Vec::new(),
    }
}

/// The `AT` part of FUN_0049bd50 when view `view` is entered
/// (@ 0x0049bee8..0x0049bef0: 1 <= view <= 0x13): the octopus's entry.
pub fn atlantis_entry_hooks(view_name: &[u8], view: u32) -> Vec<u32> {
    if prefixed(view_name, ATLANTIS_PREFIX) && (1..20).contains(&view) {
        vec![OCTOPUS_ENTRY]
    } else {
        Vec::new()
    }
}

impl Engine {
    /// The `SH` hooks of a zone click (noted, the click not consumed).
    pub(super) fn shamballa_click(&mut self, view_name: &[u8], view: u32, zone: &Zone) {
        for function in shamballa_click_hooks(view_name, view, zone.kind) {
            self.puzzle_not_ported(function);
        }
    }

    /// The `AT` hooks of a view entry (noted).
    pub(super) fn atlantis_entry(&mut self) {
        let hooks = self
            .current_view()
            .map(|v| atlantis_entry_hooks(v.name.as_bytes(), self.view))
            .unwrap_or_default();
        for function in hooks {
            self.puzzle_not_ported(function);
        }
    }

    /// FUN_0045a630, at the end of a move whose view is not in the
    /// place: in `voiles.wam`, FUN_004952d0 (noted [`Event::PuzzleRan`]).
    pub(super) fn move_end_without_view(&mut self) {
        if !self.wam_name.eq_ignore_ascii_case(FIREFLIES_PLACE) {
            return;
        }
        self.event(Event::PuzzleRan {
            function: FIREFLIES_MOVE_END,
        });
        let (w, h) = (self.renderer.width() as u32, self.renderer.height() as u32);
        self.camera.set_fov(FOV_76, w, h);
        self.with_script(|s, h| {
            if s.var(DONE) == Some(1) {
                s.set_var(AUTO_WIN, 1);
                s.ctrl_scenar_in(h);
            }
        });
    }

    /// A testing aid, not the original's: what the fireflies' winning
    /// click writes (FUN_00495210 @ 0x00495234..0x004952a8: the sixth
    /// firefly lit, `NbLucioleAllumed` = 6 and `Puzzle_Lucioles_Fini` = 1),
    /// without its sound (`(NewSound=300)`). The scripts then go on as
    /// after the win: the vortex clicked, or the next firefly's move end.
    pub fn win_fireflies(&mut self) {
        self.event(Event::PuzzleAid {
            function: FIREFLIES_CLICK,
        });
        self.with_script(|s, _| {
            s.set_var(LIT, 6);
            s.set_var(DONE, 1);
        });
    }

    /// A testing aid, not the original's: one of the octopus fight's two
    /// ends (FUN_0049aaa0): `octopus_dead`, the last big tentacle's death
    /// (@ 0x0049ae84..0x0049ae9b: the tentacle's record FUN_0049af90, the
    /// quit FUN_0049aef0, `(chapter=200)` and a scan), else the hero's
    /// (@ 0x0049aea8..0x0049aeba: the quit,
    /// `(chapter=201)` and a scan). The quit clears the mode 6, which the
    /// port never sets.
    pub fn end_octopus_fight(&mut self, octopus_dead: bool) {
        self.event(Event::PuzzleAid {
            function: OCTOPUS_TICK,
        });
        if self.puzzle_mode() == OCTOPUS_MODE {
            self.set_puzzle_mode(0);
        }
        let command = if octopus_dead {
            OCTOPUS_DEAD
        } else {
            HERO_DEAD
        };
        self.with_script(|s, h| {
            s.run_command(h, command);
            s.ctrl_scenar_in(h);
        });
    }
}

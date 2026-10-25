//! Shamballa's and Atlantis's puzzle hooks (`MONDE` 4): the dispatchers'
//! branches for the views whose warp names start with `SH` (@ 0x005daffc,
//! `voiles.wam`) and `AT` (@ 0x005e2ea8, `atlantis.wam`), `_strnicmp` over
//! two characters, any case; and the end of a move without a next view in
//! `voiles.wam`. `rhea.wam` (`RH`) and `annulair.wam` (`AN`) have none.
//!
//! - **Click** (FUN_0049b910, move zones, types 1 and 2,
//!   @ 0x0049ba5a..0x0049ba84, after Ireland's and before `SS` 31's):
//!   `SH` view 1, the fireflies' click FUN_00495210 (catalogue row 17,
//!   `pz_lucioles`), ported: see [`Engine::fireflies_click`]. The
//!   dispatcher returns 0: the move goes on (the zones of view 1 lead to
//!   views 1..6, of which only view 1 is in the file).
//! - **Entry** (FUN_0049bd50, @ 0x0049bec6..0x0049bef2, after `LA`'s and
//!   before `SS`'s): `AT` views 1..19, the octopus fight's FUN_0049a8c0
//!   (catalogue row 18, `engine/octopus.rs`), which sets the puzzle mode
//!   6; its tick FUN_0049aaa0 runs in that mode only (FUN_0049bfb0), its
//!   cursor FUN_0049a9a0 returns 0 outside it.
//! - **A move's end without a next view** (FUN_0045a3b0 @ 0x0045a5d0 →
//!   FUN_0045a630): when the current place is `voiles.wam` (`_stricmp`
//!   with the name @ 0x00659168), FUN_004952d0, ported: the camera's field
//!   of view back to 76° (FUN_004a4950(0x3fa9c91f, W, H, W, H): the zoom
//!   of chapter 78's firefly moves may have left it at 35°, there being
//!   no view entry to set it back), then, when `Puzzle_Lucioles_Fini` is
//!   1, `Puzzle_Lucioles_AutoWin` = 1 and a scan (FUN_00483c90): chapter
//!   78's way to `rhea.wam`.
//!
//! Both puzzles run: the fireflies' click here, the octopus fight in
//! `octopus.rs`.

use atlantis2_data::wam::Zone;

use super::octopus::OCTOPUS_ENTRY;
use super::{Engine, Event};

/// The prefix of `voiles.wam`'s views (@ 0x005daffc) and of
/// `atlantis.wam`'s (@ 0x005e2ea8).
const SHAMBALLA_PREFIX: &[u8] = b"SH";
pub(super) const ATLANTIS_PREFIX: &[u8] = b"AT";

/// The fireflies' view (@ 0x0049ba79, EBX = 1).
const FIREFLIES_VIEW: u32 = 1;

/// The fireflies' click (FUN_00495210) and their move end (FUN_004952d0).
pub const FIREFLIES_CLICK: u32 = 0x0049_5210;
pub const FIREFLIES_MOVE_END: u32 = 0x0049_52d0;

/// The place FUN_0045a630 compares with (@ 0x005db944).
const FIREFLIES_PLACE: &str = "voiles.wam";

/// The field of view FUN_004952d0 sets, 76° in radians (0x3fa9c91f).
const FOV_76: f32 = f32::from_bits(0x3fa9_c91f);

/// The fireflies' variables (@ 0x005e1dac, 0x005e1d50, 0x005e1dc0).
const LIT: &str = "NbLucioleAllumed";
const DONE: &str = "Puzzle_Lucioles_Fini";
const AUTO_WIN: &str = "Puzzle_Lucioles_AutoWin";

/// The byte table @ 0x005e1d38 the click indexes by the zone's value
/// (`movsx`, no bound check, @ 0x0049525f): bytes 0..7, of which 1..6
/// are the `OrdreMandala<k>` each firefly reads. Byte 0 and byte 7 (the
/// table's padding) are 0; beyond, the bytes of the next string
/// (`(NewSound=300)` @ 0x005e1d40) and, before, the tail of the previous
/// one (`etat_etoile%d`).
pub const FIREFLY_MANDALA: [u8; 8] = [0, 4, 1, 2, 5, 3, 6, 0];

/// The executable's bytes 0x005e1d28..0x005e1dd8 around the table (its
/// `.data`, read from atlantis2-cd-fr-2199552): `etat_etoile%d`, the
/// table, `(NewSound=300)`, `Puzzle_Lucioles_Fini`, `OrdreMandala%d`, the
/// warning, `NbLucioleAllumed` and `Puzzle_Lucioles_AutoWin`, each padded
/// to 4 bytes. A value v in -16..160 reads its byte here; outside, the
/// original reads another byte of its image (other strings, or the
/// zero-filled `.data` the game writes at run time, or an unmapped
/// address), which the port does not have.
const AROUND_TABLE: &[u8; 0xb0] = b"etat_etoile%d\0\0\0\
    \0\x04\x01\x02\x05\x03\x06\0(NewSound=300)\0\0\
    Puzzle_Lucioles_Fini\0\0\0\0OrdreMandala%d\0\0\
    pz_lucioles_gere_clic(): zone_clicked->numero=%d\0\0\0\0\
    NbLucioleAllumed\0\0\0\0Puzzle_Lucioles_AutoWin\0";

/// The address of [`AROUND_TABLE`]'s first byte.
const AROUND_TABLE_AT: i64 = 0x005e_1d28;

/// The address of [`FIREFLY_MANDALA`].
const TABLE_AT: i64 = 0x005e_1d38;

/// The byte @ 0x005e1d38 + `v` (`movsx` @ 0x0049525f), when the port has
/// it ([`AROUND_TABLE`]).
pub fn firefly_order_byte(v: i32) -> Option<i8> {
    let i = usize::try_from(TABLE_AT + i64::from(v) - AROUND_TABLE_AT).ok()?;
    AROUND_TABLE.get(i).map(|&b| b as i8)
}

/// The variable whose rank the sixth firefly is compared with
/// (`OrdreMandala%d` @ 0x005e1d68).
const ORDER: &str = "OrdreMandala";

/// The click's sound (@ 0x005e1d40), run for every zone of type 1:
/// `sound300.apc`, on none of the four discs, so that it plays nothing
/// (FUN_0047a290 does nothing when the file does not open).
const LIT_SOUND: &[u8] = b"(NewSound=300)";

/// The warning of a value outside 1..6 (@ 0x005e1d78).
const BAD_VALUE: &str = "pz_lucioles_gere_clic(): zone_clicked->numero=%d";

/// The firefly count that wins (@ 0x0049529c).
const FIREFLIES: i32 = 6;

fn prefixed(view_name: &[u8], prefix: &[u8]) -> bool {
    view_name.len() >= 2 && view_name[..2].eq_ignore_ascii_case(prefix)
}

/// FUN_0045a630's test (@ 0x0045a630..0x0045a645): the current place's
/// name (@ 0x00659168) against `voiles.wam` (@ 0x005db944) by `_stricmp`
/// (FUN_005b91d0), the whole name in any case.
pub fn fireflies_place(name: &str) -> bool {
    name.eq_ignore_ascii_case(FIREFLIES_PLACE)
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
    /// The `SH` hooks of a zone click (the click not consumed): the
    /// fireflies' click.
    pub(super) fn shamballa_click(&mut self, view_name: &[u8], view: u32, zone: &Zone) {
        for function in shamballa_click_hooks(view_name, view, zone.kind) {
            self.event(Event::PuzzleRan { function });
            self.fireflies_click(zone);
        }
    }

    /// FUN_00495210 (`pz_lucioles_gere_clic`, @ 0x00495210..0x004952c4)
    /// on a zone of `voiles.wam`'s view 1; public for tests and tools (the
    /// hook alone, without the dispatcher and the scans).
    ///
    /// Nothing unless the zone's type is 1 (@ 0x0049521e: a type 2 reaches
    /// the hook and returns). For a value v in 1..6, `NbLucioleAllumed` + 1
    /// (`Interp_AddVar` @ 0x0049523b) and the turn's speeds stopped
    /// (FUN_0040c680 @ 0x00495243); any other value, the warning
    /// (FUN_00480c20 @ 0x00495250), the count kept. Then, whatever v,
    /// `OrdreMandala<t[v]>` (t = [`FIREFLY_MANDALA`] @ 0x005e1d38) compared
    /// with the count (@ 0x0049528b, the count read first): equal and the
    /// count 6 (@ 0x0049529c), `Puzzle_Lucioles_Fini` = 1 (@ 0x004952a8);
    /// then `(NewSound=300)` (@ 0x004952b5). Only the sixth firefly is
    /// tested: the one whose mandala was laid last in Tibet's view 10 wins
    /// at the count 6, whatever the five before; the count is never reset
    /// here (the scripts set it to 0), so a seventh click does not test
    /// again. No zone of the data has a value outside 1..6: there the name
    /// looked up (`OrdreMandala0`, or the bytes around the table:
    /// `OrdreMandala40` for 8, the `(`) is not a variable,
    /// `DefName_GetIdName`'s fatal error (@ 0x004106a3); for a v whose
    /// byte the port does not have ([`firefly_order_byte`]), the message
    /// names the byte instead.
    pub fn fireflies_click(&mut self, zone: &Zone) {
        if zone.kind != 1 {
            return;
        }
        let v = zone.value;
        if (1..=6).contains(&v) {
            self.with_script(|s, _| s.add_var(LIT, 1));
            self.look.stop();
        } else {
            self.with_script(|s, _| s.warn(BAD_VALUE, &v.to_string()));
        }
        // The byte @ 0x005e1d38 + v, sign-extended, and
        // `sprintf("OrdreMandala%d")` (@ 0x0049526d): the table's for
        // 0..7; outside, the bytes around it, none of which names a
        // variable.
        let order = firefly_order_byte(v).map(|k| format!("{ORDER}{k}"));
        let sounded = self.with_script(|s, h| {
            let lit = s.var(LIT);
            let Some(rank) = order.as_deref().and_then(|name| s.var(name)) else {
                // Beyond the bytes the port has, the name is unknown: the
                // message says which byte the original reads instead.
                let name = order.unwrap_or_else(|| format!("{ORDER}<byte 0x005e1d38{v:+}>"));
                let msg = format!("DefName_GetIdName: Nom de #VARIABLES# inconnu:{name}");
                log::error!("{msg}");
                s.fail(msg);
                return false;
            };
            if lit == Some(rank) && s.var(LIT) == Some(FIREFLIES) {
                s.set_var(DONE, 1);
            }
            s.run_command(h, LIT_SOUND);
            true
        });
        if sounded == Some(true) {
            self.event(Event::FireflySound);
        }
    }

    /// The `AT` hooks of a view entry: the octopus fight's entry.
    pub(super) fn atlantis_entry(&mut self) {
        let hooks = self
            .current_view()
            .map(|v| atlantis_entry_hooks(v.name.as_bytes(), self.view))
            .unwrap_or_default();
        for function in hooks {
            match function {
                OCTOPUS_ENTRY => self.octopus_entry(),
                _ => self.puzzle_not_ported(function),
            }
        }
    }

    /// FUN_0045a630, at the end of a move whose view is not in the
    /// place: in `voiles.wam` ([`fireflies_place`]), FUN_004952d0 (noted
    /// [`Event::PuzzleRan`]); public for tests and tools.
    pub fn move_end_without_view(&mut self) {
        if !fireflies_place(&self.wam_name) {
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
}

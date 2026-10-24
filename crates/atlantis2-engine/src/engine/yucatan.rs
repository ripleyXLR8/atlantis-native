//! Yucatán's puzzle hooks: the dispatchers' branches for the views whose
//! warp names start with `YU` (@ 0x005db004, `_strnicmp` over two
//! characters, any case). Yucatán's three places (`yucatan.wam`,
//! `salleroi.wam`, `pyratez.wam`, `MONDE` 2) and Xibalba's four (`MONDE`
//! 6, `xibalba.rs`) share the prefix.
//!
//! - **Click** (FUN_0049b910): move zones (types 1, 2,
//!   @ 0x0049b9a4..0x0049b9fd): views 55, 64, 75, the counting grids'
//!   leave FUN_00492d60 / FUN_00492e80 / FUN_00492ef0; then, in the puzzle
//!   mode 1 (the spider web), the web's click FUN_0048fdd0, whose value
//!   the dispatcher returns. Types 3, 4 and 0x32 (@ 0x0049babf..0x0049bbbc):
//!   type 0x32 only, views 55, 64, 75 the grids' click FUN_004931a0 /
//!   FUN_00493490 / FUN_00493510 and view 121 the star map FUN_00494e80;
//!   view 402 the canoe: while `enable_pz_pirog` is not 1 the click is
//!   **consumed** (the dispatcher's own return of 1, @ 0x0049bb53 →
//!   0x0049bcfd; the web's branch, @ 0x0049b9f3..0x0049b9fd, returns
//!   FUN_0048fdd0's value, which can be 1 too), else FUN_0049c010(1) starts it; view 122 the
//!   Tezcatlipoca statue (below). Every other `YU` path returns 0: the
//!   click goes on to `CLICZONE` and the move.
//! - **Entry** (FUN_0049bd50, @ 0x0049bd50..0x0049bdd2): in the puzzle mode
//!   1, the web's FUN_0048f9a0 first, whatever the prefix; then for `YU`,
//!   views 55, 64, 75 the grids' FUN_00492f60 / FUN_00493070 /
//!   FUN_00493100, view 198 outside the mode 1 the web's FUN_0048f530,
//!   view 121 the star map's FUN_00494e00.
//! - **The Tezcatlipoca statue** (view 122, @ 0x0049bb62..0x0049bbbc),
//!   ported: on a zone whose value is `ClicTezStatue`'s index
//!   (`DefName_GetIdName(4, ..)`) while `TEZ_STATUE` is not hidden
//!   (FUN_00484d90), `CLICPERSO` = `TEZ_STATUE` (Interp_WriteVar), the
//!   cursor pair `interfac.spr` frame 40 (FUN_0047bef0) and the subject
//!   menu open (@ 0x006617b0 = 1): the statue answers as a character.
//!   The hand and the subject count are not tested. The dispatcher returns
//!   0, `CLICZONE` is set as for any action zone, and the click scan of
//!   that frame does not run (the menu is open, FUN_0045a870).
//!
//! The Maya counting grids (views 55, 64, 75 of `pyratez.wam`, chapters
//! 31, 32, 34) and Xibalba's star map (view 121 of `xibalba.wam`,
//! chapter 39) run (`grids.rs`, `stars.rs`), each hook noted
//! [`Event::PuzzleRan`]. The canoe (view 402) and the spider web (views
//! 198-265, the mode 1), Xibalba's (`MONDE` 6) other puzzles, the port
//! does not have: they are listed and noted
//! ([`Event::PuzzleNotPorted`]); testing aids write their wins
//! (`xibalba.rs`).

use atlantis2_data::defnames;
use atlantis2_data::wam::Zone;

use super::grids::{GRID_CLICKS, GRID_ENTRIES, GRID_LEAVES, GRID_VIEWS};
use super::stars::{STARS_CLICK, STARS_ENTRY};
use super::{CursorBank, Engine, Event};

/// The prefix of Yucatán's views (@ 0x005db004).
const YUCATAN_PREFIX: &[u8] = b"YU";

/// The counting grids' views (@ 0x0049b9bd, 0x0049b9ce, 0x0049b9df).
pub const MAYA_GRID_VIEWS: [u32; 3] = GRID_VIEWS;

/// The star map's view (@ 0x0049bb1f).
const STARS_VIEW: u32 = 121;

/// The canoe's view (@ 0x0049bb34), its switch (@ 0x005e2e94) and start
/// (FUN_0049c010(1), loop type 9).
const CANOE_VIEW: u32 = 402;
const CANOE_SWITCH: &str = "enable_pz_pirog";
const CANOE_START: u32 = 0x0049_c010;

/// The statue's view (@ 0x0049bb68), the branch's address (the port's
/// name for it in the events), its zone (@ 0x005e2e84) and character
/// (@ 0x005e2e78).
pub const STATUE_VIEW: u32 = 122;
pub const STATUE_BRANCH: u32 = 0x0049_bb62;
const STATUE_ZONE: &[u8] = b"ClicTezStatue";
const STATUE: &[u8] = b"TEZ_STATUE";
const STATUE_NAME: &str = "TEZ_STATUE";

/// The spider web's puzzle mode, its entry views and functions: the
/// click in the mode (FUN_0048fdd0), the entry in the mode (FUN_0048f9a0)
/// and the entry of view 198 outside it (FUN_0048f530).
pub const WEB_MODE: i32 = 1;
const WEB_VIEW: u32 = 198;
const WEB_CLICK: u32 = 0x0048_fdd0;
const WEB_MODE_ENTRY: u32 = 0x0048_f9a0;
const WEB_ENTRY: u32 = 0x0048_f530;

/// The pointer frame of `interfac.spr` (@ 0x0049bbb1).
const POINTER_FRAME: i32 = 0x28;

fn prefixed(view_name: &[u8]) -> bool {
    view_name.len() >= 2 && view_name[..2].eq_ignore_ascii_case(YUCATAN_PREFIX)
}

/// The `YU` part of FUN_0049b910 for a click on a zone of type `kind` in
/// view `view` of a place whose warp names start as `view_name`, in the
/// puzzle mode `mode`, with `canoe` = `enable_pz_pirog` is 1: the
/// functions it reaches, in its order, and whether it consumes the click
/// (the canoe's view while the canoe is not enabled; the web's click,
/// whose value it returns, is counted as not consuming: the port does not
/// have it). [`STATUE_BRANCH`] stands for the statue's branch, which then
/// tests the zone and the statue.
pub fn yucatan_click_hooks(
    view_name: &[u8],
    view: u32,
    kind: i32,
    mode: i32,
    canoe: bool,
) -> (Vec<u32>, bool) {
    let mut hooks = Vec::new();
    if !prefixed(view_name) {
        return (hooks, false);
    }
    let grid = MAYA_GRID_VIEWS.iter().position(|&v| v == view);
    match kind {
        1 | 2 => {
            hooks.extend(grid.map(|g| GRID_LEAVES[g]));
            if mode == WEB_MODE {
                hooks.push(WEB_CLICK);
            }
        }
        3 | 4 | 0x32 => {
            if kind == 0x32 {
                hooks.extend(grid.map(|g| GRID_CLICKS[g]));
                if view == STARS_VIEW {
                    hooks.push(STARS_CLICK);
                }
            }
            if view == CANOE_VIEW {
                if !canoe {
                    return (hooks, true);
                }
                hooks.push(CANOE_START);
            }
            if view == STATUE_VIEW {
                hooks.push(STATUE_BRANCH);
            }
        }
        _ => {}
    }
    (hooks, false)
}

/// The Yucatán part of FUN_0049bd50 when view `view` is entered in the
/// puzzle mode `mode`: the web's entry in the mode 1 (any prefix), then
/// for a `YU` view the grids', the web's (view 198 outside the mode 1)
/// and the star map's, in that order.
pub fn yucatan_entry_hooks(view_name: &[u8], view: u32, mode: i32) -> Vec<u32> {
    let mut hooks = Vec::new();
    if mode == WEB_MODE {
        hooks.push(WEB_MODE_ENTRY);
    }
    if prefixed(view_name) {
        if let Some(g) = MAYA_GRID_VIEWS.iter().position(|&v| v == view) {
            hooks.push(GRID_ENTRIES[g]);
        }
        if mode != WEB_MODE && view == WEB_VIEW {
            hooks.push(WEB_ENTRY);
        }
        if view == STARS_VIEW {
            hooks.push(STARS_ENTRY);
        }
    }
    hooks
}

impl Engine {
    /// The `YU` part of FUN_0049b910 for a click on `zone` of view
    /// `view` (warp name `view_name`): the grids' leaves and clicks, the
    /// star map's click and the statue's branch run, the canoe's and the
    /// web's hooks are noted; true when the click
    /// is consumed (the canoe's view while the canoe is not enabled).
    /// Public for tests and tools: the dispatcher on any warp name and
    /// number.
    pub fn yucatan_click(&mut self, view_name: &[u8], view: u32, zone: &Zone) -> bool {
        self.yucatan_click_on(None, view_name, view, zone)
    }

    /// [`Engine::yucatan_click`] on the zone of index `zi` of the view,
    /// when the click handler gives it (noted [`Event::PuzzleHook`] at the
    /// star map's click).
    pub(super) fn yucatan_click_on(
        &mut self,
        zi: Option<usize>,
        view_name: &[u8],
        view: u32,
        zone: &Zone,
    ) -> bool {
        let canoe = self
            .con
            .as_deref()
            .and_then(|s| s.var(CANOE_SWITCH))
            .is_some_and(|v| v == 1);
        let (hooks, consumed) =
            yucatan_click_hooks(view_name, view, zone.kind, self.puzzle_mode(), canoe);
        for function in hooks {
            if let Some(g) = GRID_LEAVES.iter().position(|&f| f == function) {
                self.event(Event::PuzzleRan { function });
                self.grid_leave(g);
            } else if let Some(g) = GRID_CLICKS.iter().position(|&f| f == function) {
                self.event(Event::PuzzleRan { function });
                self.grid_click(g, zone.value);
            } else if function == STARS_CLICK {
                if let Some(zi) = zi {
                    let cliczone = self.hand_scan_cliczone();
                    self.event(Event::PuzzleHook { zone: zi, cliczone });
                }
                self.event(Event::PuzzleRan { function });
                self.stars_click(zone.kind, zone.value);
            } else if function == STATUE_BRANCH {
                self.statue_click(zone);
            } else {
                self.puzzle_not_ported(function);
            }
        }
        consumed
    }

    /// The Yucatán part of FUN_0049bd50: the grids' and the star map's
    /// entries run, the web's are noted.
    pub(super) fn yucatan_entry(&mut self) {
        let hooks = self
            .current_view()
            .map(|v| yucatan_entry_hooks(v.name.as_bytes(), self.view, self.puzzle_mode()))
            .unwrap_or_default();
        for function in hooks {
            if let Some(g) = GRID_ENTRIES.iter().position(|&f| f == function) {
                self.event(Event::PuzzleRan { function });
                self.grid_entry(g);
            } else if function == STARS_ENTRY {
                self.event(Event::PuzzleRan { function });
                self.stars_entry();
            } else {
                self.puzzle_not_ported(function);
            }
        }
    }

    /// The statue's branch (@ 0x0049bb62..0x0049bbbc) on `zone`: when
    /// the zone's value (+0x04) is `ClicTezStatue`'s index and
    /// `TEZ_STATUE` is not hidden, the statue made the clicked character
    /// and its subject menu opened. Public for tests and tools.
    pub fn statue_click(&mut self, zone: &Zone) {
        // `DefName_GetIdName(4, ..)` (@ 0x0049bb75), the zone's value
        // compared (@ 0x0049bb80), FUN_00484d90 (@ 0x0049bb89), then
        // `DefName_GetIdName(0, ..)` (@ 0x0049bb9c).
        let Some(z) = self.statue_name_id(defnames::ZONES_CLIC, "#ZONES_CLIC#", STATUE_ZONE) else {
            return;
        };
        if zone.value != z || self.perso_hidden(STATUE_NAME) {
            return;
        }
        let Some(id) = self.statue_name_id(defnames::PERSONNAGES, "#PERSONNAGES#", STATUE) else {
            return;
        };
        self.event(Event::PuzzleRan {
            function: STATUE_BRANCH,
        });
        // Interp_WriteVar("CLICPERSO", ..) (@ 0x0049bba7).
        self.vars.clicperso = id;
        self.set_script_var(|k| k.clicperso, id);
        // FUN_0047bef0(interfac.spr, 40) (@ 0x0049bbb4), the menu flag
        // (@ 0x0049bbbc).
        self.set_cursor_pair(CursorBank::Interface, POINTER_FRAME);
        self.subject_menu.open = true;
        self.event(Event::SubjectMenu {
            character: id,
            count: self.subject_count(id),
        });
    }

    /// `DefName_GetIdName(section, name)` (FUN_004105a0). An unknown name
    /// is the original's fatal error (FUN_00480cd0 with "DefName_GetIdName:
    /// Nom de %s inconnu:%s", @ 0x004106a3: an emergency save, then the
    /// game quits): here the scripts stop and so does the hook (`None`).
    /// The game's data never reach it.
    fn statue_name_id(&mut self, section: usize, label: &str, name: &[u8]) -> Option<i32> {
        match self.names.id(section, name) {
            Ok(id) => Some(id as i32),
            Err(_) => {
                let msg = format!(
                    "DefName_GetIdName: Nom de {label} inconnu:{}",
                    String::from_utf8_lossy(name)
                );
                log::error!("{msg}");
                self.with_script(|s, _| s.fail(msg));
                None
            }
        }
    }
}

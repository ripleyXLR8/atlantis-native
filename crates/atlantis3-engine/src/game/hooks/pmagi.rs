//! The magician's palace (`pmagi.wam`, world 3, chapters 49..51): the star
//! room (view 10), the drawer walls `PMA_TT` (views 11, 12) and `PMA_TR`
//! (views 111, 112).
//!
//! What the scripts see: the story flags 811..865, 915..972 (the puzzles'
//! saved state), `Flag154` / `Flag812` and the commands the hooks run, the
//! scans. The sprite objects' frame tables (`PMA_ETOILESLIGHT`,
//! `PMA_LIENSLIGHT`, the drawers' open / closed frames, @ 0x00438a40 /
//! 0x00438ae0) are drawing only (M4).

use atlantis2_engine::script::Host;

use super::{Hx, ZoneRec};

/// The constellations' stars (@ 0x007894b4 + 40k, filled once by
/// @ 0x004cda70 under the byte 0x0071eeb1; −1 = none).
const CONSTELLATIONS: [[i32; 10]; 5] = [
    [15, 16, 17, 18, 19, 20, 21, 22, -1, -1],
    [9, 10, 11, 12, 13, 14, -1, -1, -1, -1],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, -1],
    [23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
    [34, 35, 36, 37, 38, 39, 40, 41, 42, -1],
];

/// Their link sprites of `PMA_LIENSLIGHT` (@ 0x007895ac + 40k; drawing).
#[allow(dead_code)]
const CONSTELLATION_LINKS: [[i32; 10]; 5] = [
    [12, 2, 44, 13, 5, 20, 6, 16, -1, -1],
    [41, 38, 37, 40, 43, 39, -1, -1, -1, -1],
    [103, 60, 58, 57, 56, 53, 61, 48, 32, -1],
    [50, 76, 85, 86, 77, 75, 62, 63, 83, 74],
    [34, 84, 71, 31, 30, 28, 24, 29, -1, -1],
];

/// The link matrix (@ 0x007877d0[43][43], symmetric, −1 elsewhere): star
/// a, star b, the `PMA_LIENSLIGHT` sprite joining them.
const LINKS: [(i32, i32, i32); 103] = [
    (0, 1, 103),
    (0, 2, 53),
    (0, 30, 100),
    (0, 31, 101),
    (0, 32, 54),
    (1, 2, 52),
    (1, 3, 60),
    (1, 7, 104),
    (1, 8, 46),
    (1, 23, 98),
    (1, 30, 99),
    (2, 3, 59),
    (2, 4, 55),
    (2, 5, 56),
    (3, 4, 58),
    (3, 6, 61),
    (4, 5, 57),
    (4, 6, 88),
    (4, 11, 93),
    (6, 7, 48),
    (6, 11, 94),
    (7, 8, 32),
    (7, 11, 95),
    (7, 12, 91),
    (7, 14, 92),
    (7, 33, 26),
    (8, 20, 33),
    (8, 23, 47),
    (8, 24, 49),
    (8, 33, 27),
    (9, 10, 41),
    (9, 12, 38),
    (9, 13, 36),
    (10, 11, 37),
    (10, 12, 42),
    (11, 12, 40),
    (12, 13, 43),
    (12, 14, 35),
    (13, 14, 39),
    (13, 15, 97),
    (14, 15, 96),
    (14, 33, 90),
    (15, 16, 1),
    (15, 17, 2),
    (15, 18, 3),
    (15, 19, 44),
    (15, 33, 0),
    (16, 17, 12),
    (17, 18, 13),
    (18, 19, 5),
    (18, 21, 6),
    (18, 22, 15),
    (18, 34, 8),
    (19, 20, 20),
    (19, 33, 4),
    (19, 34, 19),
    (20, 24, 51),
    (20, 25, 65),
    (20, 33, 21),
    (20, 34, 22),
    (21, 22, 16),
    (21, 34, 9),
    (21, 35, 10),
    (21, 38, 11),
    (21, 40, 7),
    (22, 40, 17),
    (22, 42, 23),
    (23, 24, 50),
    (23, 25, 78),
    (23, 26, 76),
    (23, 30, 64),
    (24, 25, 77),
    (25, 27, 86),
    (25, 28, 67),
    (25, 34, 66),
    (25, 36, 69),
    (26, 27, 85),
    (26, 30, 75),
    (27, 28, 83),
    (28, 29, 74),
    (28, 30, 82),
    (28, 36, 70),
    (28, 37, 72),
    (29, 30, 81),
    (29, 31, 80),
    (29, 32, 79),
    (29, 37, 73),
    (30, 31, 62),
    (31, 32, 63),
    (34, 35, 34),
    (34, 36, 68),
    (35, 36, 84),
    (35, 38, 31),
    (36, 37, 71),
    (36, 38, 89),
    (38, 39, 30),
    (38, 40, 18),
    (38, 41, 25),
    (39, 40, 28),
    (39, 41, 29),
    (39, 42, 24),
    (40, 42, 45),
    (41, 42, 87),
];

/// The link sprite joining stars a and b, −1 for none.
fn link(a: i32, b: i32) -> i32 {
    LINKS
        .iter()
        .find(|&&(x, y, _)| (x, y) == (a, b) || (y, x) == (a, b))
        .map_or(-1, |l| l.2)
}

/// The blink's half-period (@ 0x0071eeb8, seconds).
const BLINK: f64 = 0.3;

/// `PMA_TT`'s value table (@ 0x00789828, filled under 0x0071f4a9).
const TT_VALUES: [[i32; 4]; 4] = [[0, 3, 2, 1], [1, 0, 3, 2], [2, 1, 0, 3], [3, 2, 1, 0]];

/// `PMA_TT`'s lock table (@ 0x00789894): grid drawer → lock, −1 none.
const TT_LOCKS: [i32; 16] = [-1, -1, 2, -1, -1, 0, -1, -1, -1, -1, -1, 3, -1, -1, 1, -1];

/// `PMA_TR`'s step table (@ 0x007897b8, filled under 0x0071f30d).
const TR_STEPS: [i32; 16] = [4, 0, 2, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 1, 0];

/// A "play once" animation record (@ 0x004c1c70: object, frames, current
/// frame, playing).
#[derive(Debug, Default, Clone)]
pub struct Anim {
    pub obj: i32,
    pub frames: i32,
    pub current: i32,
    pub playing: bool,
}

/// The palace's C globals.
#[derive(Debug, Clone)]
pub struct State {
    // ---- the star room -------------------------------------------------
    /// The reload byte (@ 0x0071eeb0).
    pub star_reload: bool,
    /// Star k selected, phase 1 (@ 0x00789580[43]).
    pub selected: [bool; 43],
    /// The path, phase 2 (@ 0x0078967c[6]) and its length (@ 0x00789698).
    pub path: [i32; 6],
    pub path_len: i32,
    /// The blink (@ 0x0078969c), its half-period count (@ 0x007896a0) and
    /// its clock's start (@ 0x007877a0).
    pub blink: bool,
    pub blink_count: i32,
    pub blink_start: f64,
    /// Solved (@ 0x00789694) and inert (@ 0x00789695).
    pub star_solved: bool,
    pub star_inert: bool,
    /// The spoken tips' state (@ 0x007877cc): 0..3 playing, 4 done.
    pub tip: i32,
    // ---- `PMA_TT` -------------------------------------------------------
    /// The reload byte (@ 0x0071f4a8).
    pub tt_reload: bool,
    /// Drawer r·4 + c open (rows 0..3 the grid, 4 the fifth row;
    /// @ 0x00789880[20]).
    pub tt_open: [bool; 20],
    /// Lock k closed (@ 0x00789824[4]).
    pub tt_locks: [bool; 4],
    /// Grid drawers open (@ 0x007898d8).
    pub tt_count: i32,
    /// All four locks released (@ 0x007898d6).
    pub tt_unlocked: bool,
    /// The wall done (@ 0x007898d4) and its end asked (@ 0x007898d5).
    pub tt_done: bool,
    pub tt_end: bool,
    /// The click counter (@ 0x0071f4ac, 1 in the image).
    pub tt_counter: i32,
    // ---- `PMA_TR` -------------------------------------------------------
    /// The reload byte (@ 0x0071f30c).
    pub tr_reload: bool,
    /// Drawer r·4 + c open (@ 0x007897f8[16]), their count
    /// (@ 0x0078981c), the step (@ 0x00789820).
    pub tr_open: [bool; 16],
    pub tr_count: i32,
    pub tr_step: i32,
    /// Solved (@ 0x00789819), all done (@ 0x00789818), the cupboard
    /// played (@ 0x0078981a), the lamp and pull-tab played (@ 0x0078981b).
    pub tr_solved: bool,
    pub tr_done: bool,
    pub tr_cupboard: bool,
    pub tr_anims: bool,
    /// The objects' show to come (@ 0x0071f30e) and its twin, cleared
    /// only (@ 0x0071f30f); 1 in the image.
    pub tr_show: bool,
    pub tr_show2: bool,
    /// `PMA_PLACARD01` (@ 0x007897a0), `PMA_TIRETTE` (@ 0x00789788),
    /// `PMA_LAMPE` (@ 0x00789808).
    pub placard: Anim,
    pub tirette: Anim,
    pub lampe: Anim,
}

impl Default for State {
    fn default() -> State {
        State::new()
    }
}

impl State {
    pub fn new() -> State {
        State {
            star_reload: true,
            selected: [false; 43],
            path: [0; 6],
            path_len: 0,
            blink: false,
            blink_count: 0,
            blink_start: 0.0,
            star_solved: false,
            star_inert: false,
            tip: 0,
            tt_reload: true,
            tt_open: [false; 20],
            tt_locks: [false; 4],
            tt_count: 0,
            tt_unlocked: false,
            tt_done: false,
            tt_end: false,
            tt_counter: 1,
            tr_reload: true,
            tr_open: [false; 16],
            tr_count: 0,
            tr_step: 0,
            tr_solved: false,
            tr_done: false,
            tr_cupboard: false,
            tr_anims: false,
            tr_show: true,
            tr_show2: true,
            placard: Anim::default(),
            tirette: Anim::default(),
            lampe: Anim::default(),
        }
    }

    /// The load's one-shot bytes (@ 0x00459e70): 0x0071eeb0, 0x0071f30c,
    /// 0x0071f4a8.
    pub fn after_load(&mut self) {
        self.star_reload = true;
        self.tr_reload = true;
        self.tt_reload = true;
    }
}

/// A sound of the C code (@ 0x00443f00 → 0x00415df0 → 0x004329e0(s, 0, 1)
/// → 0x00433140): `sfx\<apc>` on the sound channel.
fn sound(h: &mut Hx, apc: &str) {
    h.g.new_sound(apc);
}

fn flag_range_all(h: &Hx, from: usize, n: usize) -> bool {
    (from..from + n).all(|f| h.flag(f) != 0)
}

// ---- the star room ------------------------------------------------------

/// The star room's view entry (@ 0x004cda70): `PMA_LIENSLIGHT` and
/// `PMA_ETOILESLIGHT` shown; the first time (`Flag915` = 0) the run-time
/// state cleared, `Flag915` := 1 and the tips to play (state 0), else
/// tips done (4); with the reload byte, the state rebuilt from the flags
/// (phase 1: selections from 930..972, the completion check, and phase 2
/// entered when 922..926 are all set; phase 2: the path from 819..825).
pub(super) fn view_stars(h: &mut Hx) {
    // `"?(ShowObjet=PMA_ETOILESLIGHT" + 1`: the string without its '?';
    // both commands lack their ')' in the exe.
    h.run("(ShowObjet=PMA_LIENSLIGHT");
    h.run("(ShowObjet=PMA_ETOILESLIGHT");
    if h.flag(915) == 0 {
        let p = &mut h.g.puzzles.pmagi;
        p.path = [-1; 6];
        p.selected = [false; 43];
        p.path_len = 0;
        p.blink = false;
        p.blink_count = 0;
        p.star_inert = false;
        p.star_solved = false;
        p.tip = 0;
        h.set_flag(915, 1);
    } else {
        h.g.puzzles.pmagi.tip = 4;
    }
    if h.g.puzzles.pmagi.star_reload {
        let solved = h.flag(154) != 0;
        {
            let p = &mut h.g.puzzles.pmagi;
            p.blink = false;
            p.star_solved = solved;
            p.star_inert = solved;
        }
        if h.flag(921) == 0 {
            h.g.puzzles.pmagi.path_len = 0;
            for s in 0..43 {
                let v = h.flag(930 + s) != 0;
                h.g.puzzles.pmagi.selected[s] = v;
            }
            check_constellations(h);
            if flag_range_all(h, 922, 5) {
                h.set_flag(921, 1);
                reset_path(h);
            }
        } else {
            let n = h.flag(819);
            let path: Vec<i32> = (0..n.clamp(0, 6) as usize)
                .map(|i| h.flag(820 + i))
                .collect();
            let p = &mut h.g.puzzles.pmagi;
            p.blink = n == 6;
            p.path_len = n;
            for (i, s) in path.into_iter().enumerate() {
                p.path[i] = s;
            }
        }
    }
    // visual: both sprite objects' frame tables rebuilt from the
    // selections and completed constellations (phase 1) or from the path
    // and its links (phase 2) (M4).
    h.g.puzzles.pmagi.star_reload = false;
}

/// Star s in constellation k (@ 0x004ce870).
fn in_constellation(k: usize, s: i32) -> bool {
    CONSTELLATIONS[k].contains(&s)
}

/// Star s on the path (@ 0x004ce840).
fn on_path(h: &Hx, s: i32) -> bool {
    let p = &h.g.puzzles.pmagi;
    p.path[..p.path_len.clamp(0, 6) as usize].contains(&s)
}

/// The completion check (@ 0x004ce8a0), skipped while star 33 is
/// selected (@ 0x007895a1): among the incomplete constellations, one with
/// all its stars selected while no other has some selected becomes
/// complete (flag 922 + k := 1, its selections cleared, its links shown,
/// the blink started). Quirk kept: flag 930 + **k** := the selection of
/// star **k** (@ 0x004ce983), not the cleared stars' flags.
fn check_constellations(h: &mut Hx) {
    if h.g.puzzles.pmagi.selected[33] {
        return;
    }
    let mut full: Option<usize> = None;
    for k in 0..5 {
        if h.flag(922 + k) != 0 {
            continue;
        }
        let sel = &h.g.puzzles.pmagi.selected;
        let stars = CONSTELLATIONS[k].iter().filter(|&&s| s != -1);
        let all = stars.clone().all(|&s| sel[s as usize]);
        let none = !stars.clone().any(|&s| sel[s as usize]);
        if all {
            if full.is_some() {
                return;
            }
            full = Some(k);
        } else if !none {
            return;
        }
    }
    let Some(k) = full else {
        return;
    };
    log::debug!("Constellation {k} Complétée :-)");
    h.set_flag(922 + k, 1);
    for &s in CONSTELLATIONS[k].iter().filter(|&&s| s != -1) {
        h.g.puzzles.pmagi.selected[s as usize] = false;
        let v = h.g.puzzles.pmagi.selected[k] as i32;
        h.set_flag(930 + k, v);
    }
    // visual: the constellation's links added to `PMA_LIENSLIGHT` (M4).
    let now = h.now();
    let p = &mut h.g.puzzles.pmagi;
    p.blink = true;
    p.blink_count = 0;
    p.blink_start = now;
}

/// The path's last star has a neighbour not on the path (@ 0x004cea20).
fn has_way(h: &Hx) -> bool {
    let p = &h.g.puzzles.pmagi;
    if p.path_len < 1 {
        return true;
    }
    let last = p.path[(p.path_len - 1) as usize];
    (0..43).any(|s| link(last, s) != -1 && !on_path(h, s))
}

/// The path reset (@ 0x004cea70, log `Reset Etoiles`): path = [33],
/// `Flag819` = 1, `Flag820` = 33, the other entries and flags 821.. −1.
fn reset_path(h: &mut Hx) {
    log::debug!("Reset Etoiles");
    // visual: the path's links and stars (but the first) removed (M4).
    let len = h.g.puzzles.pmagi.path_len;
    for i in 1..len.clamp(0, 6) as usize {
        h.g.puzzles.pmagi.path[i] = -1;
        h.set_flag(820 + i, -1);
    }
    let p = &mut h.g.puzzles.pmagi;
    p.path[0] = 33;
    p.path_len = 1;
    h.set_flag(820, 33);
    h.set_flag(819, 1);
}

/// The success (@ 0x004cf850): solved, `(Flag154=1)`, the tip voice
/// stopped if it still plays, `CtrlScenarIN`.
pub(super) fn stars_solved(h: &mut Hx) {
    h.g.puzzles.pmagi.star_solved = true;
    h.run("(Flag154=1)");
    if h.g.puzzles.pmagi.tip < 4 {
        // @ 0x004335f0(0x007877b8), 0x0040e330: the tip's voice and
        // subtitle stopped (M4: the tips are not played).
    }
    h.ctrl();
}

/// The star room's step (@ 0x004ceb50), in the warp mode only: the tips,
/// then (not inert) the blink of a six-star path, the clicks on the zones
/// `PMA_ZONE_ETOILEnn` (phase 1: select / deselect toward the
/// constellations; phase 2: the path from star 33), the blink's end.
pub(super) fn frame_stars(h: &mut Hx, z: &ZoneRec) {
    if h.g.mode != super::super::Mode::Warp {
        return;
    }
    // The tips (states 0..3): each plays voice 216 part n with its
    // subtitle on the private channel 0x007877b8 (0x004658e0, 0x004329e0,
    // 0x00433140, 0x0040e290) and moves on when it no longer plays
    // (0x00433780). M4: the tips are not played, so each "ends" at once
    // and the states run through to 4 in this frame (log `Fin Tip Salle
    // Etoile`, 0x0040e330).
    if h.g.puzzles.pmagi.tip < 4 {
        h.g.not_ported("the star room's spoken tips (voice 216)");
        h.g.puzzles.pmagi.tip = 4;
    }
    if h.g.puzzles.pmagi.star_inert {
        return;
    }
    let six = h.flag(921) != 0 && h.g.puzzles.pmagi.path_len == 6;
    if six && !h.g.puzzles.pmagi.blink {
        // The six-star path's blink started.
        let now = h.now();
        let p = &mut h.g.puzzles.pmagi;
        p.blink = true;
        p.blink_count = 0;
        p.blink_start = now;
    }
    if !h.g.puzzles.pmagi.blink {
        star_clicks(h, z);
        return;
    }
    // The blink: a half-period every 0.3 s; after the sixth, its end.
    let now = h.now();
    let ended = {
        let p = &mut h.g.puzzles.pmagi;
        if BLINK < now - p.blink_start {
            p.blink_start = now;
            p.blink_count += 1;
            // visual: the sprite object's +0x44 toggled (0x0045f770) (M4).
            if p.blink_count > 5 {
                p.blink = false;
            }
        }
        !p.blink
    };
    if !ended {
        return;
    }
    if h.flag(921) == 0 {
        sound(h, "sound804.apc");
        if flag_range_all(h, 922, 5) {
            h.set_flag(921, 1);
            // visual: both sprite objects cleared (0x00438c00), star 33
            // shown (M4).
            reset_path(h);
        }
    } else {
        // visual: the blinking object shown (+0x44 := 1) (M4).
        let path = h.g.puzzles.pmagi.path;
        if path[0] == 33 && path[5] == 42 {
            h.g.puzzles.pmagi.star_inert = true;
            stars_solved(h);
        } else {
            reset_path(h);
        }
    }
}

/// The star clicks of @ 0x004ceb50 (no blink running): the zone record's
/// name `PMA_ZONE_ETOILE%.2d`.
fn star_clicks(h: &mut Hx, z: &ZoneRec) {
    if z.value == 0 {
        return;
    }
    let Some(s) = (0..43).find(|s| z.name == format!("PMA_ZONE_ETOILE{s:02}")) else {
        return;
    };
    let su = s as usize;
    if h.flag(921) == 0 {
        log::debug!("-------- Action Etoile {s} Phase 1");
        let refused = (0..5).any(|k| h.flag(922 + k) != 0 && in_constellation(k, s));
        if refused {
            log::debug!("l'étoile {s} est déjà dans une constellation");
            return;
        }
        let sel = !h.g.puzzles.pmagi.selected[su];
        h.g.puzzles.pmagi.selected[su] = sel;
        h.set_flag(930 + su, sel as i32);
        if sel {
            log::debug!("Sélection de l'étoile {s}");
            // visual: the star's frame added to `PMA_ETOILESLIGHT` (M4).
            sound(h, "sound803.apc");
        } else {
            log::debug!("Désélection de l'étoile {s}");
            // visual: the star's frame removed (M4).
        }
        check_constellations(h);
        return;
    }
    log::debug!("-------- Action Etoile {s} Phase 2");
    let len = h.g.puzzles.pmagi.path_len;
    if on_path(h, s) || len >= 6 {
        return;
    }
    if len == 0 {
        log::debug!("Première étoile");
        let p = &mut h.g.puzzles.pmagi;
        p.path[0] = s;
        p.path_len = 1;
        // visual: the star shown (M4).
        sound(h, "sound803.apc");
    } else if link(h.g.puzzles.pmagi.path[(len - 1) as usize], s) != -1 {
        log::debug!("Nouvelle étoile en position {len} du chemin");
        let p = &mut h.g.puzzles.pmagi;
        p.path[len as usize] = s;
        p.path_len = len + 1;
        // visual: the star and its link shown (M4).
        sound(h, "sound804.apc");
    }
    // @ 0x004cf7d1, whether a star was added or not.
    let len = h.g.puzzles.pmagi.path_len;
    let last = h.g.puzzles.pmagi.path[(len - 1).clamp(0, 5) as usize];
    h.set_flag(819, len);
    h.set_flag(819 + len as usize, last);
    if h.flag(819) < 6 && !has_way(h) {
        log::debug!("Cul de sac");
        reset_path(h);
    }
}

// ---- the drawer wall `PMA_TT` ------------------------------------------

/// `PMA_TT`'s view entry (@ 0x004d2800): the first time (`Flag916` = 0)
/// every drawer closed, the locks closed, the counter 1, `Flag811` := 0;
/// with the reload byte, the state from the flags (830..849 drawers,
/// 813..816 locks, 817 counter, 811 done).
pub(super) fn view_tt(h: &mut Hx) {
    if h.flag(916) == 0 {
        {
            let p = &mut h.g.puzzles.pmagi;
            p.tt_open = [false; 20];
            p.tt_end = false;
            p.tt_done = false;
            p.tt_locks = [true; 4];
            p.tt_unlocked = false;
            p.tt_count = 0;
            p.tt_counter = 1;
        }
        h.set_flag(916, 1);
        h.set_flag(811, 0);
        for f in 830..850 {
            h.set_flag(f, 0);
        }
        for f in 813..=816 {
            h.set_flag(f, 1);
        }
        h.set_flag(817, 1);
    }
    if h.g.puzzles.pmagi.tt_reload {
        let done = h.flag(811) != 0;
        let open: Vec<bool> = (0..20).map(|i| h.flag(830 + i) != 0).collect();
        let locks: Vec<bool> = (0..4).map(|i| h.flag(813 + i) != 0).collect();
        let counter = h.flag(817);
        let p = &mut h.g.puzzles.pmagi;
        p.tt_done = done;
        p.tt_end = done;
        p.tt_count = open[..16].iter().filter(|&&o| o).count() as i32;
        for (i, o) in open.into_iter().enumerate() {
            p.tt_open[i] = o;
        }
        for (i, l) in locks.into_iter().enumerate() {
            p.tt_locks[i] = l;
        }
        p.tt_unlocked = !p.tt_locks.iter().any(|&l| l);
        p.tt_counter = counter;
    }
    // visual: `PMA_TT%d%d` (rows 1..5, columns 1..4) drawn open or
    // closed (M4).
    h.g.puzzles.pmagi.tt_reload = false;
}

/// `PMA_TT`'s step (@ 0x004d2ac0): a click on a grid drawer `PMA_TTrc`
/// (`CLICOBJET`) moves the drawer at row (r + counter) mod 4 holding the
/// same value (@ 0x004d2a70, @ 0x004d2a90); the locks follow their
/// drawers (flags 813..816); all four released → `sound807.apc`; a click
/// on `PMA_ZONE_TT5k` with all released opens the fifth-row drawer k
/// (flag 845 + k, `(ShowObjet=PMA_INDICEk)`). `Flag811` set → the wall
/// stops (@ 0x007898d5, @ 0x004d3500).
pub(super) fn frame_tt(h: &mut Hx, z: &ZoneRec) {
    if !h.g.puzzles.pmagi.tt_done {
        tt_clicks(h, z);
    }
    if h.flag(811) != 0 {
        h.g.puzzles.pmagi.tt_end = true;
        frame_tt_end(h);
    }
}

fn tt_clicks(h: &mut Hx, z: &ZoneRec) {
    let clicked = h.var("CLICOBJET");
    if clicked != 0 {
        let hit = (0..4)
            .flat_map(|r| (0..4).map(move |c| (r, c)))
            .find(|&(r, c)| clicked == h.obj(&format!("PMA_TT{}{}", r + 1, c + 1)));
        if let Some((r, c)) = hit {
            tt_drawer(h, r, c);
        }
    }
    let all_released = !h.g.puzzles.pmagi.tt_locks.iter().any(|&l| l);
    let unlocked = h.g.puzzles.pmagi.tt_unlocked;
    if all_released {
        if !unlocked {
            sound(h, "sound807.apc");
            h.g.puzzles.pmagi.tt_unlocked = true;
        }
    } else if unlocked {
        sound(h, "sound807.apc");
        h.g.puzzles.pmagi.tt_unlocked = false;
    }
    if z.value == 0 {
        return;
    }
    for k in 1..=4usize {
        if z.name != format!("PMA_ZONE_TT5{k}") || h.g.puzzles.pmagi.tt_open[15 + k] {
            continue;
        }
        if !h.g.puzzles.pmagi.tt_unlocked {
            log::debug!("Les tiroirs ne sont pas tous déverrouillés");
            continue;
        }
        sound(h, "sound805.apc");
        h.g.puzzles.pmagi.tt_open[15 + k] = true;
        h.set_flag(845 + k, 1);
        // visual: `PMA_TT5k` open (M4).
        h.run(&format!("(ShowObjet=PMA_INDICE{k})"));
    }
}

/// A grid drawer clicked (row r, column c, 0-based).
fn tt_drawer(h: &mut Hx, r: usize, c: usize) {
    let v = TT_VALUES[r][c];
    let counter = h.g.puzzles.pmagi.tt_counter;
    let r2 = (r as i32 + counter).rem_euclid(4) as usize;
    let Some(c2) = TT_VALUES[r2].iter().position(|&x| x == v) else {
        return;
    };
    let i = r2 * 4 + c2;
    let mut changed = false;
    if !h.g.puzzles.pmagi.tt_open[i] {
        if h.g.puzzles.pmagi.tt_count < 4 {
            sound(h, "sound805.apc");
            h.g.puzzles.pmagi.tt_open[i] = true;
            h.set_flag(830 + i, 1);
            // visual: `PMA_TT{r2+1}{c2+1}` open (M4).
            h.g.puzzles.pmagi.tt_count += 1;
            changed = true;
        }
    } else {
        sound(h, "sound806.apc");
        h.g.puzzles.pmagi.tt_open[i] = false;
        h.set_flag(830 + i, 0);
        // visual: closed (M4).
        h.g.puzzles.pmagi.tt_count -= 1;
        changed = true;
    }
    if changed {
        h.g.puzzles.pmagi.tt_counter += 1;
        let n = h.g.puzzles.pmagi.tt_counter;
        h.set_flag(817, n);
    }
    let lock = TT_LOCKS[i];
    if lock != -1 {
        let closed = !h.g.puzzles.pmagi.tt_open[i];
        if closed {
            log::debug!("-- Verrouillage Tiroir {v} ({}) !!!", v + 1);
        } else {
            log::debug!("-- Deverrouillage Tiroir {v} ({}) !!!", v + 1);
        }
        h.set_flag(813 + v as usize, closed as i32);
        h.g.puzzles.pmagi.tt_locks[lock as usize] = closed;
    }
}

/// @ 0x004d3500: the wall done (0x007898d4 := 1).
pub(super) fn frame_tt_end(h: &mut Hx) {
    h.g.puzzles.pmagi.tt_done = true;
}

// ---- the drawer wall `PMA_TR` ------------------------------------------

/// A record's set-up (@ 0x004c1c70): the object, its frame count, frame
/// 0; its frames taken out of the object's frame table (visual).
fn anim_init(h: &Hx, name: &str, frames: i32) -> Anim {
    Anim {
        obj: h.obj(name),
        frames,
        current: 0,
        playing: false,
    }
}

/// Plays a record once (@ 0x004c1cd0): every slot of its object (any
/// kind) in single-frame mode (+0x2c = 1) that has ended goes back to the
/// list mode and the record to its last frame; a slot in the other mode
/// starts its play (frame 0, +0x2c := 1, its clock restarted). True when
/// the record is at its last frame.
fn play_once(h: &mut Hx, which: fn(&mut super::Puzzles) -> &mut Anim) -> bool {
    let now = h.now();
    let obj = which(&mut h.g.puzzles).obj;
    // visual: the record's frames taken out of the object's table (M4).
    for k in 0..h.g.slots.slots.len() {
        if h.g.slots.slots[k].id != obj {
            continue;
        }
        if h.g.slots.slots[k].mode == 1 {
            if h.g.slots.slots[k].ended {
                h.g.slots.slots[k].mode = 0;
                let a = which(&mut h.g.puzzles);
                a.current = a.frames;
                a.playing = false;
                // visual: the record's frame shown (@ 0x004c1df0) (M4).
            }
        } else {
            let s = &mut h.g.slots.slots[k];
            s.frame = 0;
            s.mode = 1;
            s.start = now;
            which(&mut h.g.puzzles).playing = true;
        }
    }
    let a = which(&mut h.g.puzzles);
    a.current == a.frames
}

fn placard(p: &mut super::Puzzles) -> &mut Anim {
    &mut p.pmagi.placard
}

fn tirette(p: &mut super::Puzzles) -> &mut Anim {
    &mut p.pmagi.tirette
}

fn lampe(p: &mut super::Puzzles) -> &mut Anim {
    &mut p.pmagi.lampe
}

/// `PMA_TR`'s view entry (@ 0x004d1b00): the first time (`Flag917` = 0)
/// the three animations set up, every drawer closed, step 0, `Flag812`
/// := 0; with the reload byte, the state from the flags (850..865
/// drawers, 818 step; `Flag812` ≠ 0 → solved and done, the animations at
/// their last frame).
pub(super) fn view_tr(h: &mut Hx) {
    if h.flag(917) == 0 {
        let pl = anim_init(h, "PMA_PLACARD01", 12);
        let ti = anim_init(h, "PMA_TIRETTE", 9);
        let la = anim_init(h, "PMA_LAMPE", 9);
        {
            let p = &mut h.g.puzzles.pmagi;
            // The records' playing bytes are not touched.
            p.placard = Anim {
                playing: p.placard.playing,
                ..pl
            };
            p.tirette = Anim {
                playing: p.tirette.playing,
                ..ti
            };
            p.lampe = Anim {
                playing: p.lampe.playing,
                ..la
            };
            p.tr_open = [false; 16];
            p.tr_solved = false;
            p.tr_done = false;
            p.tr_count = 0;
            p.tr_step = 0;
            p.tr_cupboard = false;
            p.tr_show = true;
            p.tr_anims = false;
            p.tr_show2 = true;
        }
        h.set_flag(917, 1);
        h.set_flag(812, 0);
        for f in 850..866 {
            h.set_flag(f, 0);
        }
        h.set_flag(818, 0);
    }
    if h.g.puzzles.pmagi.tr_reload {
        let pl = anim_init(h, "PMA_PLACARD01", 12);
        let ti = anim_init(h, "PMA_TIRETTE", 9);
        let la = anim_init(h, "PMA_LAMPE", 9);
        let done = h.flag(812) != 0;
        let open: Vec<bool> = (0..16).map(|i| h.flag(850 + i) != 0).collect();
        let step = h.flag(818);
        let p = &mut h.g.puzzles.pmagi;
        // The records rebuilt; their playing bytes are left as they were.
        p.placard = Anim {
            playing: p.placard.playing,
            ..pl
        };
        p.tirette = Anim {
            playing: p.tirette.playing,
            ..ti
        };
        p.lampe = Anim {
            playing: p.lampe.playing,
            ..la
        };
        p.tr_done = done;
        if done {
            p.placard.current = p.placard.frames;
            p.tirette.current = p.tirette.frames;
            p.lampe.current = p.lampe.frames;
        }
        p.tr_show2 = !done;
        p.tr_show = !done;
        p.tr_solved = done;
        p.tr_cupboard = done;
        p.tr_anims = done;
        p.tr_count = open.iter().filter(|&&o| o).count() as i32;
        for (i, o) in open.into_iter().enumerate() {
            p.tr_open[i] = o;
        }
        p.tr_step = step;
    }
    // visual: the three records' frames shown (@ 0x004c1df0) and
    // `PMA_TR%d%d` drawn open or closed (M4).
    h.g.puzzles.pmagi.tr_reload = false;
}

/// `PMA_TR`'s step (@ 0x004d1e00; the zone record is not read): a click
/// on a drawer `PMA_TRrc` (`CLICOBJET`), not solved, opens or closes it
/// (flags 850..865); an opening whose step value is both the step + 1 and
/// the open count + 1 moves the step on (`Flag818`), any other move resets
/// it; at step 4 the wall is solved: `Flag812` := 1, the hand emptied, the
/// four clues out of the bag. Then the cupboard animation (with
/// `sound808.apc` and the three objects shown), the lamp and pull-tab
/// animations, and @ 0x004d2620: done, `CtrlScenarIN`.
pub(super) fn frame_tr(h: &mut Hx, z: &ZoneRec) {
    let _ = z;
    let clicked = h.var("CLICOBJET");
    if clicked != 0 && !h.g.puzzles.pmagi.tr_solved {
        let hit = (0..4)
            .flat_map(|r| (0..4).map(move |c| (r, c)))
            .find(|&(r, c)| clicked == h.obj(&format!("PMA_TR{}{}", r + 1, c + 1)));
        if let Some((r, c)) = hit {
            tr_drawer(h, r, c);
        }
    }
    if !h.g.puzzles.pmagi.tr_solved || h.g.puzzles.pmagi.tr_done {
        return;
    }
    if !h.g.puzzles.pmagi.tr_cupboard {
        if h.g.puzzles.pmagi.tr_show {
            sound(h, "sound808.apc");
            h.run("(ShowObjet=PMA_TIRETTE)");
            h.run("(ShowObjet=PMA_LAMPE)");
            h.run("(ShowObjet=PMA_CHIFFON)");
            h.g.puzzles.pmagi.tr_show = false;
        }
        if play_once(h, placard) {
            h.g.puzzles.pmagi.tr_cupboard = true;
        }
    } else if !h.g.puzzles.pmagi.tr_anims {
        h.g.puzzles.pmagi.tr_show2 = false;
        let a = play_once(h, lampe);
        let b = play_once(h, tirette);
        if a || b {
            let p = &mut h.g.puzzles.pmagi;
            p.lampe.current = p.lampe.frames;
            p.tirette.current = p.tirette.frames;
            // visual: both shown at their last frame (M4).
            p.tr_anims = true;
            // @ 0x004d2620.
            p.tr_done = true;
            h.ctrl();
        }
    }
}

/// A drawer of `PMA_TR` clicked (row r, column c, 0-based).
fn tr_drawer(h: &mut Hx, r: usize, c: usize) {
    log::debug!("-------- Action Tiroir {r};{c} ({};{})", r + 1, c + 1);
    let i = r * 4 + c;
    if !h.g.puzzles.pmagi.tr_open[i] {
        sound(h, "sound805.apc");
        h.g.puzzles.pmagi.tr_open[i] = true;
        h.set_flag(850 + i, 1);
        // visual: open (M4).
        let v = TR_STEPS[i];
        let (step, count) = (h.g.puzzles.pmagi.tr_step, h.g.puzzles.pmagi.tr_count);
        if v == step + 1 && v == count + 1 {
            let step = step + 1;
            h.set_flag(818, step);
            h.g.puzzles.pmagi.tr_step = step;
            if step == 4 {
                h.g.puzzles.pmagi.tr_solved = true;
                h.set_flag(812, 1);
                h.run("(ObjetEnMain=0)");
                h.run("(SacADos=NOT_PMA_INDICE1)");
                h.run("(SacADos=NOT_PMA_INDICE2)");
                h.run("(SacADos=NOT_PMA_INDICE3)");
                h.run("(SacADos=NOT_PMA_INDICE4)");
            }
        } else {
            h.g.puzzles.pmagi.tr_step = 0;
            h.set_flag(818, 0);
        }
        h.g.puzzles.pmagi.tr_count += 1;
    } else {
        sound(h, "sound806.apc");
        h.g.puzzles.pmagi.tr_open[i] = false;
        h.set_flag(850 + i, 0);
        // visual: closed (M4).
        h.g.puzzles.pmagi.tr_step = 0;
        h.set_flag(818, 0);
        h.g.puzzles.pmagi.tr_count -= 1;
    }
}

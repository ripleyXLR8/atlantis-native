//! The Maya counting grids of Yucatán's pyramid (`YU` views 55, 64 and 75
//! of `pyratez.wam`, chapters 31, 32 and 34).
//!
//! Each grid is 16 cells in four groups of four (cells 4k..4k + 3: the
//! tests read only i mod 4; where a group lies on the screen is not read),
//! a 16-digit string (cell i = digit 15 − i; the save's entries 21, 23 and
//! 25, `0000000000000000` at start-up, @ 0x005e1840, 0x005e1854,
//! 0x005e1868) whose digit is the number of marbles in the cell, 0..5. In
//! a group the first cell takes 1..5 marbles, each next cell only when
//! the one before holds five (a bar), the last one at most four
//! (FUN_004935c0, FUN_00493590) [inferred from that rule: a group is a
//! Maya digit, bars and dots]. The grid is the slot of
//! `SPW_BASE5A` / `B` / `C` (80 frames: cell i with n marbles is frame
//! 5 i + n − 1, one frame per non-empty cell in its frame list); the
//! marbles in hand (@ 0x006d2690, 0..5, not saved) are the hand pair's
//! frame n − 1 of `spr\sprboul.spr` (@ 0x006d2694).
//!
//! - **Entry** (FUN_00492f60 / FUN_00493070 / FUN_00493100, from the entry
//!   dispatcher FUN_0049bd50 in views 55, 64, 75): the slot's id kept
//!   (`DefName_GetIdName(1, "SPW_BASE5A")` …, @ 0x006d2678 / 0x006d2674 /
//!   0x006d2670); when `resetmayaN` is 1, it is set back to 0 and the
//!   digits to 16 zeros (@ 0x005e18e8): grid 1's, grid 2's, and for
//!   `resetmaya3` grid 2's again (@ 0x00493150, a slip of the original;
//!   no chapter sets `resetmaya3`); the grid drawn (FUN_00492ff0); the
//!   module's state (@ 0x006d2684 / 0x006d2688 / 0x006d268c, save entries
//!   20, 22, 24) 1 unless it is 2.
//! - **Draw** (FUN_00492ff0): `sprboul.spr` loaded (FUN_0047ba80: a
//!   warning when already loaded or missing), frames 0..79 taken out of
//!   the slot's list (FUN_0047c9c0, FUN_0047c910), then for each digit
//!   from the last (cell 0) to the first, a positive digit d − '0' adds
//!   frame 5 (15 − j) + d − 1 (FUN_0047c880); the puzzle mode 7
//!   (@ 0x006d84d0).
//! - **Click** (FUN_004931a0 / FUN_00493490 / FUN_00493510, from the click
//!   dispatcher on a zone of type 0x32): the step FUN_00493220 ([`grid_step`]),
//!   then, when it changed the grid and the digits equal the solution
//!   (`0000000000000045`, `0000000000040001`, `0000155500350001`
//!   @ 0x005e187c, 0x005e1890, 0x005e18a4, a `strcmp`): the state 2,
//!   `finmayaN` = 1 (Interp_WriteVar) and `sound101.apc` loaded and played
//!   on voice 2 (FUN_00493470). The dispatcher returns 0: `CLICZONE` is set
//!   as for any action zone and the click scan of the same frame follows
//!   (chapter 34's crocodile opens there, its sound 112 replacing 101).
//! - **Leave** (FUN_00492d60 / FUN_00492e80 / FUN_00492ef0, from the click
//!   dispatcher on a move zone of the view, and from the reset FUN_0049bf70
//!   at a load): the state 0 unless 2; `finmayaN` = 0 when the digits
//!   differ from the solution; then FUN_00492dd0(view): when the current
//!   view is that number, the hand pair emptied (FUN_0047c0c0(0, 0)), the
//!   bank freed (FUN_0047bbb0), no marble in hand, the three sound
//!   handles freed (FUN_0047aa30 on voice 2, then −1), the puzzle mode 0.
//! - **Tick** (FUN_00493660, the mode 7's step of FUN_0049bfb0 through the
//!   thunk @ 0x00493190, once per warp frame; also called by the step):
//!   while `OBJETENMAIN` is 0, the hand pair = (marbles − 1, the bank), or
//!   (0, none) for no marble; another count is the original's fatal
//!   `set_curseur_billes : nb_billes_en_main= %d !`.
//!
//! The mode 7 outlives the view: only a leave (a move zone of the grid's
//! view) or the reset of a load in that view clears it, so a view left by
//! a script's `vue=` keeps the marbles in hand. Every cell click for each
//! hand and neighbourhood, the other zones, the entries, the wins, the
//! leaves, the tick, the dispatchers, the clicks with chapters 31, 32 and
//! 34 and the drawing are checked against the original's in
//! `tests/golden/grids.txt`.

use atlantis2_data::defnames;
use atlantis2_data::save::SaveData;

use super::{Engine, HandBank};

/// The grids' views (@ 0x0049b9bd, 0x0049b9ce, 0x0049b9df).
pub const GRID_VIEWS: [u32; 3] = [55, 64, 75];

/// The digits at start-up and the reset's (@ 0x005e1840, 0x005e18e8),
/// with their terminator.
pub const GRID_START: [u8; 17] = *b"0000000000000000\0";

/// The solutions (@ 0x005e187c, 0x005e1890, 0x005e18a4).
pub const GRID_SOLUTIONS: [[u8; 16]; 3] = [
    *b"0000000000000045",
    *b"0000000000040001",
    *b"0000155500350001",
];

/// The entries, clicks and leaves, by grid.
pub const GRID_ENTRIES: [u32; 3] = [0x0049_2f60, 0x0049_3070, 0x0049_3100];
pub const GRID_CLICKS: [u32; 3] = [0x0049_31a0, 0x0049_3490, 0x0049_3510];
pub const GRID_LEAVES: [u32; 3] = [0x0049_2d60, 0x0049_2e80, 0x0049_2ef0];

/// The puzzle mode the draw sets (@ 0x0049305d).
pub const MARBLES_MODE: i32 = 7;

/// The grids' sprites (@ 0x005e1908, 0x005e192c, 0x005e1944), variables
/// (@ 0x005e18c4.., 0x005e18fc, 0x005e1920, 0x005e1938).
const SPRITES: [&[u8]; 3] = [b"SPW_BASE5A", b"SPW_BASE5B", b"SPW_BASE5C"];
const FIN: [&str; 3] = ["finmaya1", "finmaya2", "finmaya3"];
const RESET: [&str; 3] = ["resetmaya1", "resetmaya2", "resetmaya3"];

/// The grid whose digits each entry's reset clears (@ 0x00492fa8,
/// 0x004930c0, 0x00493150: grid 3's clears grid 2's).
const RESET_GRID: [usize; 3] = [0, 1, 1];

/// The save entries of the states and the digits.
const STATE_ENTRIES: [usize; 3] = [20, 22, 24];
const DIGIT_ENTRIES: [usize; 3] = [21, 23, 25];

/// The marbles' bank (@ 0x005e1914) and the sounds (@ 0x005e1960,
/// 0x005e1970, 0x005e1950).
const BANK: &[u8] = b"sprboul.spr";
const TAKE_SOUND: &str = "sound100.apc";
const WIN_SOUND: &str = "sound101.apc";
const REFUSE_SOUND: &str = "sound102.apc";

/// The voice of the grids' sounds.
const VOICE: usize = 2;

/// The zone values of the hand: one more marble (0x11, wrapping after
/// four), five or none (0x12), one less or none from five (0x13, 0x14).
pub const ONE_MORE: i32 = 0x11;
pub const FIVE: i32 = 0x12;
pub const ONE_LESS: [i32; 2] = [0x13, 0x14];

/// The module's state beside the save's entries.
#[derive(Clone, Debug)]
pub struct Grids {
    /// The marbles in hand (@ 0x006d2690): set at start-up only.
    pub hand: i32,
    /// `spr\sprboul.spr` (@ 0x006d2694), loaded by the draw, freed by the
    /// leave in the grid's view.
    bank: Option<atlantis2_data::spr::SprBank>,
    /// Whether the bank is loaded (its handle is not 0); the file may be
    /// missing (the handle stays 0 then).
    pub bank_loaded: bool,
    /// The sound handles of `sound100.apc`, `sound101.apc`, `sound102.apc`
    /// (@ 0x005e18b8, 0x005e18bc, 0x005e18c0; −1 at start-up).
    pub handles: [i32; 3],
    /// The slot ids the entries kept (@ 0x006d2678, 0x006d2674,
    /// 0x006d2670; 0 before any).
    pub sprites: [i32; 3],
}

impl Default for Grids {
    fn default() -> Grids {
        Grids {
            hand: 0,
            bank: None,
            bank_loaded: false,
            handles: [-1; 3],
            sprites: [0; 3],
        }
    }
}

impl Grids {
    pub(super) fn bank(&self) -> Option<&atlantis2_data::spr::SprBank> {
        self.bank.as_ref()
    }
}

/// FUN_004935c0(i, hand, s): whether marbles may be laid on cell i: its
/// group's first cell (i mod 4 = 0), or the cell before it holds '5'
/// (digit 16 − i); and the group's last cell (i mod 4 = 3), or the cell
/// after it holds '0' (digit 14 − i); five marbles are refused on a
/// group's last cell.
pub fn may_lay(i: usize, hand: i32, s: &[u8]) -> bool {
    let c = i & 3;
    let before = |s: &[u8]| s[16 - i] == b'5';
    if c == 3 {
        if hand == 5 && before(s) {
            return false;
        }
        if !before(s) {
            return false;
        }
    } else if c != 0 && !before(s) {
        return false;
    }
    // @ 0x0049360d: the cell after, read as a signed digit.
    c == 3 || s[14 - i] == b'0'
}

/// FUN_00493590(i, s): whether the marbles of cell i may be taken: the
/// group's last cell, or the cell after it holds '0'.
pub fn may_take(i: usize, s: &[u8]) -> bool {
    i & 3 == 3 || s[14 - i] == b'0'
}

/// What the click step asks of the engine, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridOp {
    /// `sound100.apc` loaded (FUN_0047a530, the handle @ 0x005e18b8) and
    /// played on voice 2 (FUN_0047a650): zones 0x11 and 0x12.
    Take,
    /// `sound102.apc` the same (the handle @ 0x005e18c0): a refusal.
    Refuse,
    /// The handle @ 0x005e18b8 played again on voice 2, whatever it is
    /// (−1 before any 0x11 / 0x12 zone of the visit: the warning): a lay.
    Replay,
    /// FUN_00493660, the hand's cursor.
    Cursor,
}

/// The frame list of the grid's slot, as the step uses it: FUN_0047c9c0,
/// FUN_0047c910 and FUN_0047c880 on the slot's id.
pub trait GridList {
    fn has(&self, frame: i32) -> bool;
    fn sub(&mut self, frame: i32);
    fn add(&mut self, frame: i32);
}

/// FUN_00493220(zone, id, digits): `value` the zone's (+0x04), `objet`
/// `OBJETENMAIN`, `hand` the marbles in hand, `digits` the grid's 17
/// bytes, `list` the slot's frame list. Returns whether the grid changed
/// (the byte the clicks test) and the engine's part in order.
///
/// With an object in hand nothing happens. Zone 0x11: sound 100, one
/// marble more, from 4 (or 5) to none. 0x12: sound 100, five marbles or
/// none from five. A cell (1..16) with marbles in hand: refused (sound 102) unless
/// [`may_lay`]; else the cell's marbles taken (its digit '0', its first
/// frame of the five found in the list taken out), the hand's laid (its
/// frame added, the digit the count, sound 100 replayed) and the hand
/// gets what the cell held: a swap. With no marble in hand: refused
/// (sound 102) unless [`may_take`], else the cell's marbles taken into
/// the hand (no sound). A negative hand does nothing. 0x13 and 0x14: none
/// from five, else one less.
pub fn grid_step(
    value: i32,
    objet: i32,
    hand: &mut i32,
    digits: &mut [u8; 17],
    list: &mut impl GridList,
) -> (bool, Vec<GridOp>) {
    let mut ops = Vec::new();
    if objet != 0 {
        // @ 0x00493235.
        return (false, ops);
    }
    let mut changed = false;
    if value == ONE_MORE {
        // @ 0x00493249..0x00493286.
        ops.push(GridOp::Take);
        if *hand >= 5 {
            *hand = 0;
        }
        *hand += 1;
        if *hand >= 5 {
            *hand = 0;
        }
        ops.push(GridOp::Cursor);
    }
    if value == FIVE {
        // @ 0x00493291..0x004932c1.
        ops.push(GridOp::Take);
        *hand = if *hand != 5 { 5 } else { 0 };
        ops.push(GridOp::Cursor);
    }
    if (1..=16).contains(&value) {
        let i = (value - 1) as usize;
        let base = value * 5 - 5;
        let lay = may_lay(i, *hand, &digits[..]);
        let take_ok = may_take(i, &digits[..]);
        match *hand {
            h if h < 0 => {}
            0 if take_ok => {
                // @ 0x00493409..0x00493422: the hand gets the cell's
                // marbles, when it had any.
                let found = take_cell(i, base, digits, list);
                if found + 1 != 0 {
                    *hand = found + 1;
                    ops.push(GridOp::Cursor);
                    changed = true;
                }
            }
            h if h > 0 && lay => {
                // @ 0x004933b7..0x00493400: the cell's marbles taken, the
                // hand's laid, the hand gets the cell's.
                let found = take_cell(i, base, digits, list);
                ops.push(GridOp::Replay);
                list.add(h - 1 + base);
                digits[15 - i] = (h as u8).wrapping_add(b'0');
                *hand = found + 1;
                ops.push(GridOp::Cursor);
                changed = true;
            }
            // @ 0x00493326.
            _ => ops.push(GridOp::Refuse),
        }
    }
    if ONE_LESS.contains(&value) {
        // @ 0x0049343d..0x00493461.
        if *hand == 5 {
            *hand = 0;
        } else if *hand > 0 {
            *hand -= 1;
        }
        ops.push(GridOp::Cursor);
    }
    (changed, ops)
}

/// The list part of FUN_00492ff0 (@ 0x00493010..0x00493057): frames
/// 0..79 taken out where the list has them, then for each digit from the
/// last (cell 0) to the first a positive `d − '0'` (a signed byte) adds
/// frame 5 (15 − j) + d − 1.
pub fn grid_list_draw(list: &mut impl GridList, digits: &[u8; 17]) {
    for f in 0..0x50 {
        if list.has(f) {
            list.sub(f);
        }
    }
    for j in (0..16).rev() {
        let n = i32::from(digits[j] as i8) - 0x30;
        if n > 0 {
            list.add(5 * (15 - j as i32) + n - 1);
        }
    }
}

/// FUN_00493660's choice for `hand` marbles (with no object in hand): no
/// marble the empty pair, 1..5 the frame `hand − 1` of the marbles' bank;
/// any other count is the original's fatal error (`Err`).
pub fn grid_cursor_frame(hand: i32) -> Result<Option<i32>, String> {
    match hand {
        0 => Ok(None),
        1..=5 => Ok(Some(hand - 1)),
        h => Err(format!("set_curseur_billes : nb_billes_en_main= {h} !")),
    }
}

/// FUN_00493630 (the cell's digit '0'), then the cell's first frame of
/// its five that the list has taken out (@ 0x00493363..0x004933a1): its
/// index (the count − 1), −1 for none.
fn take_cell(i: usize, base: i32, digits: &mut [u8; 17], list: &mut impl GridList) -> i32 {
    digits[15 - i] = b'0';
    for k in 0..5 {
        if list.has(base + k) {
            list.sub(base + k);
            return k;
        }
    }
    -1
}

/// The save's entries as the start-up leaves them: zeros but the grids'
/// digits (the `.data` strings @ 0x005e1840, 0x005e1854, 0x005e1868).
pub(super) fn save_extra_at_start() -> SaveData {
    let mut d = SaveData::default();
    for k in DIGIT_ENTRIES {
        d.entry_mut(k).copy_from_slice(&GRID_START);
    }
    d
}

/// A C string's bytes (up to its first 0).
fn c_str(b: &[u8]) -> &[u8] {
    &b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]
}

/// The grid's frame list: every slot of the id, as the frame-list
/// functions read them.
struct SlotList<'a> {
    anim: &'a mut crate::anim::Anim,
    id: i32,
}

impl GridList for SlotList<'_> {
    fn has(&self, frame: i32) -> bool {
        self.anim.has_frame(self.id, frame)
    }
    fn sub(&mut self, frame: i32) {
        self.anim.sub_frame(self.id, frame);
    }
    fn add(&mut self, frame: i32) {
        self.anim.add_frame(self.id, frame);
    }
}

impl Engine {
    /// Grid g's module state (save entry 20, 22 or 24): 0, 1 while its
    /// view is the current one, 2 once solved.
    pub fn grid_state(&self, g: usize) -> i32 {
        i32::from_le_bytes(self.save_extra.entry(STATE_ENTRIES[g]).try_into().unwrap())
    }

    pub fn set_grid_state(&mut self, g: usize, v: i32) {
        self.save_extra
            .entry_mut(STATE_ENTRIES[g])
            .copy_from_slice(&v.to_le_bytes());
    }

    /// Grid g's 17 bytes (save entry 21, 23 or 25).
    pub fn grid_digits(&self, g: usize) -> [u8; 17] {
        self.save_extra.entry(DIGIT_ENTRIES[g]).try_into().unwrap()
    }

    pub fn set_grid_digits(&mut self, g: usize, d: &[u8; 17]) {
        self.save_extra
            .entry_mut(DIGIT_ENTRIES[g])
            .copy_from_slice(d);
    }

    /// `Interp_ReadVar` from the interpreter or the variables a `vue=`
    /// line lent; 0 without the scripts.
    fn grid_var(&self, name: &str) -> i32 {
        self.script_var_or_lent(name).unwrap_or(0)
    }

    /// `Interp_WriteVar`, to the interpreter or the variables lent.
    fn grid_write_var(&mut self, name: &str, value: i32) {
        if let Some(s) = self.con.as_deref_mut() {
            s.set_var(name, value);
            return;
        }
        let Ok(i) = self.names.id(defnames::VARIABLES, name.as_bytes()) else {
            return;
        };
        if let Some(v) = self.lent_vars.as_mut().and_then(|l| l.get_mut(i)) {
            *v = value;
        }
    }

    /// The entries (FUN_00492f60 / FUN_00493070 / FUN_00493100) of grid
    /// g (0..2). Public for tests and tools: the hook alone.
    pub fn grid_entry(&mut self, g: usize) {
        // `DefName_GetIdName(1, ..)` (FUN_004105a0): an unknown name is
        // the original's fatal error; the game's data have the three.
        let id = match self.names.id(defnames::OBJETS, SPRITES[g]) {
            Ok(i) => i as i32,
            Err(_) => {
                let msg = format!(
                    "DefName_GetIdName: Nom de #OBJETS# inconnu:{}",
                    String::from_utf8_lossy(SPRITES[g])
                );
                self.grid_fatal(&msg);
                return;
            }
        };
        self.grids.sprites[g] = id;
        if self.grid_var(RESET[g]) == 1 {
            self.grid_write_var(RESET[g], 0);
            self.set_grid_digits(RESET_GRID[g], &GRID_START);
        }
        self.grid_draw(id, g);
        if self.grid_state(g) != 2 {
            self.set_grid_state(g, 1);
        }
    }

    /// FUN_00492ff0(id, digits of grid g): the bank, the list, the mode 7.
    fn grid_draw(&mut self, id: i32, g: usize) {
        // FUN_0047ba80(name, &bank, 0).
        if self.grids.bank_loaded {
            log::warn!("AtlSpr_LoadBank(): bank spr déja init ! sprboul.spr");
        } else {
            match self.load_bank(BANK) {
                Some(b) => {
                    self.grids.bank = Some(b);
                    self.grids.bank_loaded = true;
                }
                None => log::warn!("AtlSpr_LoadBank(): fichier non trouvé: sprboul.spr"),
            }
        }
        let d = self.grid_digits(g);
        grid_list_draw(
            &mut SlotList {
                anim: &mut self.anim,
                id,
            },
            &d,
        );
        self.set_puzzle_mode(MARBLES_MODE);
    }

    /// The clicks (FUN_004931a0 / FUN_00493490 / FUN_00493510) of grid g
    /// on a zone of value `value`. Public for tests and tools: the hook
    /// alone, without the dispatcher and the scans.
    pub fn grid_click(&mut self, g: usize, value: i32) {
        let changed = self.grid_step(g, value);
        let digits = self.grid_digits(g);
        if changed && c_str(&digits) == c_str(&GRID_SOLUTIONS[g]) {
            // @ 0x00493202..0x00493214, FUN_00493470.
            self.set_grid_state(g, 2);
            self.grid_write_var(FIN[g], 1);
            self.grids.handles[1] = self.sound_load(WIN_SOUND);
            self.sound_play(self.grids.handles[1], VOICE);
        }
    }

    /// FUN_00493220 on grid g's slot (the id its entry kept) and digits:
    /// [`grid_step`], then its sounds and cursor; whether the grid
    /// changed. Public for tests and tools.
    pub fn grid_step(&mut self, g: usize, value: i32) -> bool {
        let objet = self.objetenmain();
        let mut digits = self.grid_digits(g);
        let mut hand = self.grids.hand;
        let id = self.grids.sprites[g];
        let (changed, ops) = {
            let mut list = SlotList {
                anim: &mut self.anim,
                id,
            };
            grid_step(value, objet, &mut hand, &mut digits, &mut list)
        };
        self.set_grid_digits(g, &digits);
        self.grids.hand = hand;
        // The sounds and the cursor in the step's order: neither reads
        // what the other parts change.
        for op in ops {
            match op {
                GridOp::Take => {
                    self.grids.handles[0] = self.sound_load(TAKE_SOUND);
                    self.sound_play(self.grids.handles[0], VOICE);
                }
                GridOp::Refuse => {
                    self.grids.handles[2] = self.sound_load(REFUSE_SOUND);
                    self.sound_play(self.grids.handles[2], VOICE);
                }
                GridOp::Replay => self.sound_play(self.grids.handles[0], VOICE),
                GridOp::Cursor => self.grids_cursor(),
            }
        }
        changed
    }

    /// The leaves (FUN_00492d60 / FUN_00492e80 / FUN_00492ef0) of grid g.
    /// Public for tests and tools.
    pub fn grid_leave(&mut self, g: usize) {
        if self.grid_state(g) != 2 {
            self.set_grid_state(g, 0);
        }
        if c_str(&self.grid_digits(g)) != c_str(&GRID_SOLUTIONS[g]) {
            self.grid_write_var(FIN[g], 0);
        }
        self.grid_drop(GRID_VIEWS[g]);
    }

    /// FUN_00492dd0(view): in that view only, the hand pair, the bank, the
    /// marbles, the sounds and the mode.
    fn grid_drop(&mut self, view: u32) {
        if self.current_view().is_none() || self.view != view {
            return;
        }
        self.set_hand_pair_bank(0, HandBank::None);
        self.grid_bank_free();
        self.grids.hand = 0;
        for k in 0..3 {
            if self.grids.handles[k] != -1 {
                self.sound_free(self.grids.handles[k], VOICE);
                self.grids.handles[k] = -1;
            }
        }
        self.set_puzzle_mode(0);
    }

    /// FUN_0047bbb0(&bank) on the marbles' bank: a warning when it is not
    /// loaded; a hand pair of that bank emptied; the bank freed. Public
    /// for tests and tools.
    pub fn grid_bank_free(&mut self) {
        if !self.grids.bank_loaded {
            log::warn!("AtlSpr_LibereBank(): adr_bank déja NULL, 6D2694");
            return;
        }
        if self.hand_bank() == HandBank::Puzzle {
            self.set_hand_pair_bank(0, HandBank::None);
        }
        self.grids.bank = None;
        self.grids.bank_loaded = false;
    }

    /// FUN_00493660, the hand's cursor (the mode 7's tick, and the step's).
    pub fn grids_cursor(&mut self) {
        if self.objetenmain() != 0 {
            return;
        }
        match grid_cursor_frame(self.grids.hand) {
            Ok(None) => self.set_hand_pair_bank(0, HandBank::None),
            Ok(Some(f)) => {
                // FUN_0047c0c0(frame, the bank's handle): 0 when the file
                // was missing, so nothing drawn.
                let bank = if self.grids.bank_loaded {
                    HandBank::Puzzle
                } else {
                    HandBank::None
                };
                self.set_hand_pair_bank(f, bank);
            }
            Err(msg) => self.grid_fatal(&msg),
        }
    }

    /// The grids' part of the reset FUN_0049bf70 (a load, before the
    /// save's entries are copied): the three leaves. Public for tests and
    /// tools.
    pub fn grids_reset(&mut self) {
        for g in 0..3 {
            self.grid_leave(g);
        }
    }

    /// A fatal error of the original (FUN_00480cd0: an emergency save, then
    /// the game quits): here the scripts stop. The game's data never
    /// reach it.
    fn grid_fatal(&mut self, msg: &str) {
        log::error!("{msg}");
        let msg = msg.to_string();
        self.with_script(|s, _| s.fail(msg));
    }
}

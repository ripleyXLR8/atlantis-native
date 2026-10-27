//! The player screen's state and its per-frame blocks (`FUN_0041dea4`, the
//! SELECT PLAYER NAME loop), apart from the drawing, the input and the ARE
//! YOU SURE prompt, which `menus.rs` keeps.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`menu_layout.cpp`
//! `PlayerScreenState`, `playerScreenExit`; GPLv3, the ScummVM team). The
//! blocks run in the original's order each frame: the click
//! (0x0041e0a5..0x0041e1e4), the field draw (shown when not empty,
//! 0x0041e1e9), the name entry (0x0041e212..0x0041e4cc), the double-click
//! exit (0x0041e4d3), the frame drawn, the OK exit (0x0041e5bf) and, on any
//! exit, the commit test (0x0041e6af..0x0041e6d4). There is no typing mode:
//! a click is handled the same way while a name is typed. The rows are the
//! player list as the original keeps it ([`Players`]: the players, then the
//! marker row while there are fewer than five); the records are SELEMENU's
//! (5 OK, 6 DELETE).

use atlantis_data::saves::{Players, NAME_MAX_CHARS};

use super::layout::name_entry_code;

/// SELEMENU's OK and DELETE records (the title, four sprites, then the two
/// labels; tested by number, so that every language's menu works).
pub const OK: usize = 5;
pub const DELETE: usize = 6;

/// How a frame leaves the loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerScreenExit {
    /// The loop goes on.
    None,
    /// A typed name committed with Enter (0x0041e44c).
    Enter,
    /// The double-click flag with a valid name (0x0041e4fd).
    Double,
    /// A click on OK with a valid name (0x0041e5f2).
    Ok,
}

/// The loop's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerScreenState {
    /// [EBP-8]: the selected row, -1 for none.
    pub row: i32,
    /// 0x00612080: the name field, "" at entry.
    pub field: Vec<u8>,
    /// 0x00772910: the typed length, at most 15.
    pub len: usize,
    /// [EBP-0x18]: a 16th character was typed.
    pub full: bool,
    /// [EBP-4]: the 16th character.
    pub last_key: u8,
    /// [EBP-0x10]: the insertion point's phase, 1 at entry, never reset.
    pub blink_on: bool,
    /// 0x00496607: the current player's row.
    pub current: i32,
    /// 0x00496118: set by the window procedure on WM_LBUTTONDBLCLK.
    pub dbl_flag: bool,
    /// 0x006ce262: the last WM_CHAR code (Windows-1252, 8 Backspace, 0x0d
    /// Enter), kept until a block clears it; 0 for none.
    pub key: u8,
}

impl Default for PlayerScreenState {
    fn default() -> Self {
        PlayerScreenState {
            row: -1,
            field: Vec::new(),
            len: 0,
            full: false,
            last_key: b'.',
            blink_on: true,
            current: -1,
            dbl_flag: false,
            key: 0,
        }
    }
}

/// Row `r` is the last and is the marker's: the "new player" row
/// (0x0041e212..0x0041e23c, 0x0041e153).
pub fn is_new_row(players: &Players, r: i32) -> bool {
    r >= 0 && r as usize + 1 == players.count && players.is_new_row(r as usize)
}

/// Byte `i` of the field buffer (it holds 16 bytes whenever the name entry
/// writes it).
fn put_field_byte(field: &mut Vec<u8>, i: usize, c: u8) {
    if i < field.len() {
        field[i] = c;
    } else if i == field.len() {
        field.push(c);
    }
}

impl PlayerScreenState {
    /// The first byte neither NUL, ' ' nor '.' (0x0041e37c, 0x0041e4dc,
    /// 0x0041e5d3).
    pub fn field_valid(&self) -> bool {
        !matches!(self.field.first(), None | Some(b' ' | b'.'))
    }

    /// The click block, for a frame with a click (0x0049611c) on a record
    /// other than OK: unless on DELETE the row becomes the grid's
    /// (`FUN_004211f0`, -1 off the rows); the length, key and 16th-character
    /// flag reset; a row gives its name (16 dots on the new player row) and
    /// becomes the current player, no row empties the field and clears the
    /// double-click flag (0x0041e184). True when `FUN_0041e8a8` (ARE YOU
    /// SURE, then the delete) runs for the row: DELETE on a row whose name
    /// is not the marker (0x0041e18e..0x0041e1c7); the caller asks,
    /// deletes and calls [`PlayerScreenState::delete_done`].
    pub fn click(
        &mut self,
        clicked: bool,
        record: Option<usize>,
        grid: i32,
        players: &Players,
    ) -> bool {
        if !clicked || record == Some(OK) {
            return false; // 0x0041e0a5
        }
        if record != Some(DELETE) {
            self.row = grid; // 0x0041e0bc..0x0041e0ea
        }
        self.len = 0; // 0x0041e0ed..0x0041e0fd
        self.key = 0;
        self.full = false;
        let on_list = self.row >= 0 && (self.row as usize) < players.count;
        if self.row != -1 {
            // 0x0041e10d..0x0041e165 (the port gives no row past the list).
            self.field = if on_list {
                players.name(self.row as usize).to_vec()
            } else {
                Vec::new()
            };
            if is_new_row(players, self.row) {
                self.field = vec![b'.'; NAME_MAX_CHARS]; // 0x0041e153
            }
            self.current = self.row;
        } else {
            self.field.clear(); // 0x0041e175
            self.dbl_flag = false; // 0x0041e184
        }
        if record == Some(DELETE)
            && self.field.first() != Some(&b' ')
            && on_list
            && players.name(self.row as usize) != players.marker.as_slice()
        {
            self.dbl_flag = false; // 0x0041e1cb
            return true;
        }
        false
    }

    /// After `FUN_0041e8a8`: a confirmed delete empties the field
    /// (0x0041ec58); the flag is cleared (0x0041e8db) and the row is -1
    /// either way (0x0041e1e3).
    pub fn delete_done(&mut self, confirmed: bool) {
        if confirmed {
            self.field.clear();
        }
        self.dbl_flag = false;
        self.row = -1;
    }

    /// The name entry on the new player row: a key (0x20..0xa7 through
    /// [`name_entry_code`]; from the 16th on the length stays 15 and the
    /// key replaces the 16th character), Backspace, Enter, or the blink
    /// when no key; then key and length reset off the last row
    /// (0x0041e4b3). True on an Enter commit: the field then holds the name
    /// and the row is -1 (0x0041e3a2..0x0041e43c); the loop ends as on OK.
    pub fn name_entry(&mut self, players: &Players, upper_case: bool, dos_letters: bool) -> bool {
        if is_new_row(players, self.row) {
            if self.key != 0 && self.len < NAME_MAX_CHARS {
                self.dbl_flag = false; // 0x0041e25b
                                       // 0x0041e265..0x0041e27a: a leading space is no key.
                let k = if self.key == 0x20 && self.len == 0 {
                    0
                } else {
                    self.key
                };
                let c = if k < 0x20 {
                    None
                } else {
                    name_entry_code(k, upper_case, dos_letters)
                };
                if let Some(c) = c {
                    // 0x0041e29d..0x0041e302
                    if self.len == 0 {
                        self.field = vec![b'.'; NAME_MAX_CHARS];
                    }
                    put_field_byte(&mut self.field, self.len, c);
                    self.last_key = b'.';
                    if self.len == NAME_MAX_CHARS - 1 {
                        self.full = true;
                        self.last_key = c;
                    } else {
                        self.len += 1;
                    }
                } else if k == 8 {
                    // 0x0041e310..0x0041e361
                    if self.len != 0 {
                        put_field_byte(&mut self.field, self.len, b'.');
                        if self.len == NAME_MAX_CHARS - 1 && self.last_key != b'.' {
                            self.last_key = b'.';
                        } else {
                            put_field_byte(&mut self.field, self.len - 1, b'.');
                            self.len -= 1;
                        }
                    }
                } else if k == 0x0d && self.field_valid() {
                    // 0x0041e39e..0x0041e44c: the key stays 0x0d.
                    self.field = self.typed_name();
                    self.row = -1;
                    return true;
                }
                self.key = 0;
            } else {
                // 0x0041e45c..0x0041e4b0
                let c = match (self.blink_on, self.full) {
                    (true, true) => self.last_key,
                    (true, false) => b'.',
                    (false, _) => b' ',
                };
                put_field_byte(&mut self.field, self.len, c);
                self.blink_on = !self.blink_on;
            }
        }
        if self.row < players.count as i32 - 1 {
            // 0x0041e4b3..0x0041e4cc
            self.key = 0;
            self.len = 0;
        }
        false
    }

    /// The double-click exit (0x0041e4d3), whatever the record.
    pub fn dbl_exit(&self) -> bool {
        self.dbl_flag && self.field_valid()
    }

    /// The OK exit (0x0041e5bf..0x0041e5f2).
    pub fn ok_exit(&self, clicked: bool, record: Option<usize>) -> bool {
        clicked && record == Some(OK) && self.field_valid()
    }

    /// The frame up to the draw: the name entry, then the double-click exit
    /// (0x0041e212..0x0041e4fd).
    pub fn exit_before_draw(
        &mut self,
        players: &Players,
        upper_case: bool,
        dos_letters: bool,
    ) -> PlayerScreenExit {
        if self.name_entry(players, upper_case, dos_letters) {
            PlayerScreenExit::Enter
        } else if self.dbl_exit() {
            PlayerScreenExit::Double
        } else {
            PlayerScreenExit::None
        }
    }

    /// The loop test after the draw (0x0041e5bf..0x0041e5f2).
    pub fn exit_after_draw(&self, clicked: bool, record: Option<usize>) -> PlayerScreenExit {
        if self.ok_exit(clicked, record) {
            PlayerScreenExit::Ok
        } else {
            PlayerScreenExit::None
        }
    }

    /// The record 0x006cd850 the exit reads: OK after Enter (0x0041e442)
    /// and OK, the frame's own after a double click.
    pub fn exit_record(e: PlayerScreenExit, record: Option<usize>) -> Option<usize> {
        if e == PlayerScreenExit::Double {
            record
        } else {
            Some(OK)
        }
    }

    /// On any exit: record 5, no pending Enter and the last row selected run
    /// the commit of a typed name (0x0041e6af..0x0041e6d4), whatever that
    /// row holds.
    pub fn exit_commits(&self, record: Option<usize>, count: usize) -> bool {
        record == Some(OK) && self.key != 0x0d && self.row == count as i32 - 1
    }

    /// The name the commits store: the typed bytes, then the 16th character
    /// when one was typed (0x0041e39e..0x0041e3cc, 0x0041e6db..0x0041e709).
    pub fn typed_name(&self) -> Vec<u8> {
        let mut name = self.field[..self.len.min(self.field.len())].to_vec();
        if self.full {
            name.push(self.last_key);
        }
        name
    }

    /// The exit (0x0041e5f7..0x0041e6d4): the double-click flag cleared,
    /// then the commit test. True when the exit stores the typed name into
    /// the last row: after an Enter commit (the field holds the name), or
    /// when [`PlayerScreenState::exit_commits`] holds (the field then holds
    /// the name, the row is the last).
    pub fn leave(&mut self, e: PlayerScreenExit, record: Option<usize>, count: usize) -> bool {
        self.dbl_flag = false; // 0x0041e5f7
        if e == PlayerScreenExit::Enter {
            return true;
        }
        if !self.exit_commits(PlayerScreenState::exit_record(e, record), count) {
            return false;
        }
        self.field = self.typed_name();
        self.row = count as i32 - 1;
        true
    }
}

/// What an exit did to the players.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerScreenExitResult {
    /// The exit ran a commit of the typed name.
    pub commit: bool,
    /// The name was stored into the last row.
    pub stored: bool,
    /// `players.atl` is written: the exit's record is OK (0x0041e6af,
    /// 0x0041e77c); a double click elsewhere skips the write.
    pub write: bool,
    /// The player who starts: the current row, if it is on the list.
    pub chosen: Option<usize>,
}

/// The end of an exit on the player list: [`PlayerScreenState::leave`],
/// then the store of a commit into the last row
/// ([`Players::store_in_last_row`]). With five players and the fifth
/// selected, the original's OK runs that commit with an empty typed name
/// and empties the fifth name (the player then stays listed with no name,
/// unselectable); with `keep_fifth_name` (`BugFix::FifthPlayer`) the name is
/// kept.
pub fn player_screen_exit(
    st: &mut PlayerScreenState,
    e: PlayerScreenExit,
    record: Option<usize>,
    players: &mut Players,
    keep_fifth_name: bool,
) -> PlayerScreenExitResult {
    let count = players.count;
    let mut r = PlayerScreenExitResult {
        write: PlayerScreenState::exit_record(e, record) == Some(OK),
        ..PlayerScreenExitResult::default()
    };
    r.commit = st.leave(e, record, count);
    if r.commit && count > 0 && (is_new_row(players, count as i32 - 1) || !keep_fifth_name) {
        players.store_in_last_row(&st.field);
        r.stored = true;
    }
    r.chosen = usize::try_from(st.current)
        .ok()
        .filter(|&c| c < players.count);
    r
}

/// The double click the original reads as WM_LBUTTONDBLCLK (window class
/// style CS_DBLCLKS @ 0x0042e4eb; the window procedure stores 0x00496118 =
/// 1 @ 0x00419c8b): a press within the double-click time of the previous
/// press and inside the double-click rectangle centred on it; the press
/// after a double click starts a new pair. The time and the rectangle are
/// settings of the player's Windows: its defaults, 500 ms and 4 x 4 pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuDoubleClick {
    pending: bool,
    ms: u64,
    at: (i32, i32),
}

impl MenuDoubleClick {
    /// Windows' default double-click time.
    pub const TIME_MS: u64 = 500;

    /// A left press at `at`: true when it completes a double click.
    pub fn press(&mut self, now: u64, at: (i32, i32)) -> bool {
        if self.pending
            && now.saturating_sub(self.ms) < MenuDoubleClick::TIME_MS
            && (at.0 - self.at.0).abs() <= 2
            && (at.1 - self.at.1).abs() <= 2
        {
            self.pending = false;
            return true;
        }
        *self = MenuDoubleClick {
            pending: true,
            ms: now,
            at,
        };
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_click_pairs() {
        let mut d = MenuDoubleClick::default();
        assert!(!d.press(1000, (10, 10)));
        assert!(d.press(1400, (12, 8)));
        // The press after a double click starts a new pair.
        assert!(!d.press(1500, (12, 8)));
        // Too late, too far.
        assert!(!d.press(2000, (12, 8)));
        assert!(!d.press(2100, (15, 8)));
        assert!(d.press(2200, (15, 8)));
    }
}

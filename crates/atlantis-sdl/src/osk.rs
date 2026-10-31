//! The on-screen keyboard: a grid over the top of the frame, for the
//! game's name entry (a handheld has no keyboard).
//!
//! From Klaas Van den Broeck (@Klaasvdb),
//! github.com/Klaasvdb/atlantis-native-handheld (GPL-3.0-or-later), brought
//! into Atlantis Native with his agreement; his one-row strip made a grid
//! here.
//!
//! Four rows of 13 cells: A..M, N..Z, the digits and `- . '`, then four
//! wide keys: the case switch, Space, Erase and Done. Every build's name
//! entry takes 0x20..0x7a (the English 1997 one storing a..z as A..Z,
//! the others keeping the case up to 0xa7), so every key of the grid
//! types into every build.
//!
//! The D-pad or the left stick moves in two dimensions (held: repeats),
//! wrapping at the edges; A presses the key, B erases, X types a space, Y
//! switches upper / lower case, Start (or Done) sends Enter and closes the
//! keyboard, Select closes it.

use atlantis_engine::sys::Key;
use atlantis_engine::warp::SCREEN_W;

/// A cell of the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// A character (the letters as drawn: upper or lower case).
    Char(char),
    /// Switches upper / lower case.
    Shift,
    Space,
    Erase,
    /// Enter, and the keyboard closes.
    Done,
}

pub const COLS: usize = 13;
pub const ROWS: usize = 4;

/// The character rows (the letters in upper case).
const CHARS: [&[u8; COLS]; 3] = [b"ABCDEFGHIJKLM", b"NOPQRSTUVWXYZ", b"1234567890-.'"];

/// The last row's wide keys: the key, its first column and its width.
const WIDE: [(Cell, usize, usize); 4] = [
    (Cell::Shift, 0, 3),
    (Cell::Space, 3, 5),
    (Cell::Erase, 8, 3),
    (Cell::Done, 11, 2),
];

const CELL_W: usize = 40;
const CELL_H: usize = 32;
/// The panel's top row and margin (above the player screen's title,
/// SELEMENU.TXT's line at y 140).
const TOP: usize = 4;
const MARGIN: usize = 3;
const LEFT: usize = (SCREEN_W - COLS * CELL_W) / 2;
pub const BOTTOM: usize = TOP + ROWS * CELL_H + MARGIN;
const _: () = assert!(BOTTOM <= 140);
const BG: u16 = 0x0000;
const FG: u16 = 0xffff;
const FRAME: u16 = 0x4208; // dark grey
const HIGHLIGHT: u16 = 0xfd20; // orange

/// A move held down repeats after this many frames, then every few.
const REPEAT_DELAY: u32 = 18;
const REPEAT_EVERY: u32 = 5;

#[derive(Default)]
pub struct Osk {
    pub active: bool,
    row: usize,
    /// The column; on the last row the wide key covering it is selected
    /// (going down then up comes back to the same column).
    col: usize,
    lower: bool,
    /// The direction held (x, y each -1, 0 or 1) and for how many frames.
    held: ((i32, i32), u32),
}

impl Osk {
    pub fn open(&mut self) {
        self.active = true;
        self.held = ((0, 0), 0);
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    /// The direction held this frame (x right, y down; each -1, 0 or 1):
    /// moves the selection on the press and on the repeats.
    pub fn tick(&mut self, dir: (i32, i32)) {
        if dir != self.held.0 {
            self.held = (dir, 0);
        } else {
            self.held.1 = self.held.1.saturating_add(1);
        }
        let (d, n) = self.held;
        if d != (0, 0)
            && (n == 0 || (n >= REPEAT_DELAY && (n - REPEAT_DELAY).is_multiple_of(REPEAT_EVERY)))
        {
            self.step(d.0, d.1);
        }
    }

    /// One move, wrapping at the edges.
    pub fn step(&mut self, dx: i32, dy: i32) {
        if dy != 0 {
            self.row = (self.row as i32 + dy).rem_euclid(ROWS as i32) as usize;
        }
        if dx != 0 {
            if self.row == ROWS - 1 {
                let i = wide_at(self.col) as i32 + dx;
                self.col = WIDE[i.rem_euclid(WIDE.len() as i32) as usize].1;
            } else {
                self.col = (self.col as i32 + dx).rem_euclid(COLS as i32) as usize;
            }
        }
    }

    /// The selected cell.
    pub fn cell(&self) -> Cell {
        self.cell_at(self.row, self.col)
    }

    fn cell_at(&self, row: usize, col: usize) -> Cell {
        if row == ROWS - 1 {
            return WIDE[wide_at(col)].0;
        }
        let c = char::from(CHARS[row][col]);
        Cell::Char(if self.lower {
            c.to_ascii_lowercase()
        } else {
            c
        })
    }

    /// A pressed on the selected cell: the key it types (Enter for Done,
    /// which also closes the keyboard; nothing for the case switch).
    pub fn press(&mut self) -> Option<Key> {
        match self.cell() {
            Cell::Char(c) => Some(Key::Char(c)),
            Cell::Shift => {
                self.toggle_case();
                None
            }
            Cell::Space => Some(Key::Space),
            Cell::Erase => Some(Key::Backspace),
            Cell::Done => {
                self.close();
                Some(Key::Enter)
            }
        }
    }

    pub fn toggle_case(&mut self) {
        self.lower = !self.lower;
    }

    /// Draws the grid over the RGB565 640x480 frame (the screen's size,
    /// 640x480 or HD, only scales it).
    pub fn draw(&self, frame: &mut [u16]) {
        if !self.active {
            return;
        }
        let width = COLS * CELL_W;
        for y in TOP - MARGIN..BOTTOM {
            frame[y * SCREEN_W + LEFT - MARGIN..y * SCREEN_W + LEFT + width + MARGIN].fill(BG);
        }
        let selected = (self.row, wide_or_col(self.row, self.col));
        for row in 0..ROWS {
            let y0 = TOP + row * CELL_H;
            let cells: Vec<(usize, usize)> = if row == ROWS - 1 {
                WIDE.iter().map(|&(_, c, w)| (c, w)).collect()
            } else {
                (0..COLS).map(|c| (c, 1)).collect()
            };
            for (col, span) in cells {
                let x0 = LEFT + col * CELL_W;
                let w = span * CELL_W;
                let on = selected == (row, col);
                let (fill, ink) = if on { (HIGHLIGHT, BG) } else { (BG, FG) };
                // A cell: its 1-pixel gap, a frame, the inside.
                for y in y0 + 1..y0 + CELL_H - 1 {
                    let line = &mut frame[y * SCREEN_W + x0 + 1..y * SCREEN_W + x0 + w - 1];
                    if on || y == y0 + 1 || y == y0 + CELL_H - 2 {
                        line.fill(if on { fill } else { FRAME });
                    } else {
                        line[0] = FRAME;
                        line[line.len() - 1] = FRAME;
                    }
                }
                let (text, scale) = match self.cell_at(row, col) {
                    Cell::Char(c) => (c.to_string(), 3),
                    Cell::Shift => ((if self.lower { "ABC" } else { "abc" }).to_string(), 2),
                    Cell::Space => ("SPACE".to_string(), 2),
                    Cell::Erase => ("ERASE".to_string(), 2),
                    Cell::Done => ("DONE".to_string(), 2),
                };
                let tw = text.len() * 6 * scale - scale;
                let (tx, ty) = (x0 + (w - tw) / 2, y0 + (CELL_H - 7 * scale) / 2);
                draw_text(frame, &text, tx, ty, scale, ink);
            }
        }
    }
}

/// The index in [`WIDE`] of the key covering column `col`.
fn wide_at(col: usize) -> usize {
    WIDE.iter()
        .position(|&(_, c, w)| (c..c + w).contains(&col))
        .unwrap_or(0)
}

/// The column a cell is drawn from: on the last row, its wide key's.
fn wide_or_col(row: usize, col: usize) -> usize {
    if row == ROWS - 1 {
        WIDE[wide_at(col)].1
    } else {
        col
    }
}

fn draw_text(frame: &mut [u16], text: &str, x: usize, y: usize, scale: usize, colour: u16) {
    for (i, c) in text.chars().enumerate() {
        let gx = x + i * 6 * scale;
        for (row, bits) in glyph(c).iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    let p = (y + row * scale + dy) * SCREEN_W + gx + col * scale;
                    frame[p..p + scale].fill(colour);
                }
            }
        }
    }
}

/// 5x7 glyphs, one row per byte (bit 4 = leftmost column).
fn glyph(c: char) -> [u8; 7] {
    const UPPER: [[u8; 7]; 26] = [
        [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11], // A
        [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e], // B
        [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e], // C
        [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e], // D
        [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f], // E
        [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10], // F
        [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f], // G
        [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11], // H
        [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e], // I
        [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0c], // J
        [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11], // K
        [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f], // L
        [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11], // M
        [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11], // N
        [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e], // O
        [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10], // P
        [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d], // Q
        [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11], // R
        [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e], // S
        [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // T
        [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e], // U
        [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04], // V
        [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a], // W
        [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11], // X
        [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04], // Y
        [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f], // Z
    ];
    const LOWER: [[u8; 7]; 26] = [
        [0x00, 0x00, 0x0e, 0x01, 0x0f, 0x11, 0x0f], // a
        [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x1e], // b
        [0x00, 0x00, 0x0e, 0x10, 0x10, 0x11, 0x0e], // c
        [0x01, 0x01, 0x0d, 0x13, 0x11, 0x11, 0x0f], // d
        [0x00, 0x00, 0x0e, 0x11, 0x1f, 0x10, 0x0e], // e
        [0x06, 0x09, 0x08, 0x1c, 0x08, 0x08, 0x08], // f
        [0x00, 0x0f, 0x11, 0x11, 0x0f, 0x01, 0x0e], // g
        [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11], // h
        [0x04, 0x00, 0x0c, 0x04, 0x04, 0x04, 0x0e], // i
        [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0c], // j
        [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12], // k
        [0x0c, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e], // l
        [0x00, 0x00, 0x1a, 0x15, 0x15, 0x11, 0x11], // m
        [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11], // n
        [0x00, 0x00, 0x0e, 0x11, 0x11, 0x11, 0x0e], // o
        [0x00, 0x00, 0x1e, 0x11, 0x1e, 0x10, 0x10], // p
        [0x00, 0x00, 0x0d, 0x13, 0x0f, 0x01, 0x01], // q
        [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10], // r
        [0x00, 0x00, 0x0e, 0x10, 0x0e, 0x01, 0x1e], // s
        [0x08, 0x08, 0x1c, 0x08, 0x08, 0x09, 0x06], // t
        [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0d], // u
        [0x00, 0x00, 0x11, 0x11, 0x11, 0x0a, 0x04], // v
        [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0a], // w
        [0x00, 0x00, 0x11, 0x0a, 0x04, 0x0a, 0x11], // x
        [0x00, 0x00, 0x11, 0x11, 0x0f, 0x01, 0x0e], // y
        [0x00, 0x00, 0x1f, 0x02, 0x04, 0x08, 0x1f], // z
    ];
    const DIGITS: [[u8; 7]; 10] = [
        [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e], // 0
        [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e], // 1
        [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f], // 2
        [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e], // 3
        [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02], // 4
        [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e], // 5
        [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e], // 6
        [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08], // 7
        [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e], // 8
        [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c], // 9
    ];
    match c {
        'A'..='Z' => UPPER[c as usize - 'A' as usize],
        'a'..='z' => LOWER[c as usize - 'a' as usize],
        '0'..='9' => DIGITS[c as usize - '0' as usize],
        '-' => [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
        '\'' => [0x04, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_engine::warp::SCREEN_H;

    fn at(row: usize, col: usize) -> Osk {
        let mut k = Osk {
            row,
            col,
            ..Osk::default()
        };
        k.open();
        k
    }

    #[test]
    fn moves_in_two_dimensions_and_wraps() {
        let mut k = at(0, 0);
        assert_eq!(k.cell(), Cell::Char('A'));
        k.step(1, 0);
        assert_eq!(k.cell(), Cell::Char('B'));
        k.step(0, 1);
        assert_eq!(k.cell(), Cell::Char('O'));
        k.step(0, 1);
        assert_eq!(k.cell(), Cell::Char('2'));
        // Left of the first column: the last one, same row.
        let mut k = at(1, 0);
        k.step(-1, 0);
        assert_eq!(k.cell(), Cell::Char('Z'));
        k.step(1, 0);
        assert_eq!(k.cell(), Cell::Char('N'));
        // Above the first row: the last row's key under the column.
        let mut k = at(0, 0);
        k.step(0, -1);
        assert_eq!(k.cell(), Cell::Shift);
        k.step(0, 1);
        assert_eq!(k.cell(), Cell::Char('A'));
        // The wide keys: each covers its columns; down and up again come
        // back to the column left.
        let mut k = at(2, 5);
        k.step(0, 1);
        assert_eq!(k.cell(), Cell::Space);
        k.step(0, -1);
        assert_eq!(k.cell(), Cell::Char('6'));
        let mut k = at(2, 12);
        k.step(0, 1);
        assert_eq!(k.cell(), Cell::Done);
        // Right of Done: the case switch; left of it: Done again.
        k.step(1, 0);
        assert_eq!(k.cell(), Cell::Shift);
        k.step(-1, 0);
        assert_eq!(k.cell(), Cell::Done);
        k.step(-1, 0);
        assert_eq!(k.cell(), Cell::Erase);
        // Moving off a wide key, from its first column.
        k.step(0, -1);
        assert_eq!(k.cell(), Cell::Char('9'));
        // Below the last row: the first row.
        let mut k = at(3, 4);
        k.step(0, 1);
        assert_eq!(k.cell(), Cell::Char('E'));
        // Diagonal (the D-pad's two directions held).
        k.step(1, 1);
        assert_eq!(k.cell(), Cell::Char('S'));
    }

    #[test]
    fn a_held_move_repeats() {
        let mut k = at(0, 0);
        k.tick((1, 0));
        assert_eq!(k.cell(), Cell::Char('B'));
        for _ in 1..REPEAT_DELAY {
            k.tick((1, 0));
        }
        assert_eq!(k.cell(), Cell::Char('B'));
        k.tick((1, 0));
        assert_eq!(k.cell(), Cell::Char('C'));
        for _ in 0..REPEAT_EVERY {
            k.tick((1, 0));
        }
        assert_eq!(k.cell(), Cell::Char('D'));
        // A new direction moves at once.
        k.tick((0, 1));
        assert_eq!(k.cell(), Cell::Char('Q'));
        k.tick((0, 0));
        k.tick((0, 1));
        assert_eq!(k.cell(), Cell::Char('4'));
    }

    #[test]
    fn the_keys_it_types() {
        let mut k = at(0, 2);
        assert_eq!(k.press(), Some(Key::Char('C')));
        let mut k = at(2, 9);
        assert_eq!(k.press(), Some(Key::Char('0')));
        let mut k = at(2, 11);
        assert_eq!(k.press(), Some(Key::Char('.')));
        // The case switch types nothing; the letters then are small, the
        // digits as they are.
        let mut k = at(3, 1);
        assert_eq!(k.press(), None);
        k.step(0, -1);
        k.step(0, -1);
        assert_eq!(k.press(), Some(Key::Char('o')));
        k.step(0, 1);
        assert_eq!(k.press(), Some(Key::Char('2')));
        k.toggle_case();
        k.step(0, -1);
        assert_eq!(k.press(), Some(Key::Char('O')));
        let mut k = at(3, 6);
        assert_eq!(k.press(), Some(Key::Space));
        let mut k = at(3, 8);
        assert_eq!(k.press(), Some(Key::Backspace));
        let mut k = at(3, 12);
        assert_eq!(k.press(), Some(Key::Enter));
        assert!(!k.active, "Done closes the keyboard");
    }

    #[test]
    fn every_key_has_a_glyph_and_an_accepted_code() {
        let k = at(0, 0);
        for row in 0..ROWS - 1 {
            for col in 0..COLS {
                let Cell::Char(c) = k.cell_at(row, col) else {
                    panic!("{row} {col}");
                };
                assert_ne!(glyph(c), [0; 7], "{c}");
                assert_ne!(glyph(c.to_ascii_lowercase()), [0; 7], "{c}");
                // Within every build's name entry (0x20..0x7a).
                assert!((0x20..=0x7a).contains(&(c as u32)), "{c}");
            }
        }
    }

    #[test]
    fn draws_only_when_open() {
        let mut f = vec![0x1234u16; SCREEN_W * SCREEN_H];
        let mut k = Osk::default();
        k.draw(&mut f);
        assert!(f.iter().all(|&p| p == 0x1234));
        k.open();
        k.draw(&mut f);
        assert!(f.contains(&HIGHLIGHT));
        // Nothing below the panel.
        assert!(f[BOTTOM * SCREEN_W..].iter().all(|&p| p == 0x1234));
        // Every key position drawn, the last row too.
        for (row, col) in [(0, 0), (2, 12), (3, 0), (3, 12)] {
            let mut k = at(row, col);
            k.toggle_case();
            let mut f = vec![0u16; SCREEN_W * SCREEN_H];
            k.draw(&mut f);
            let y = TOP + row * CELL_H + CELL_H / 2;
            let x = LEFT + wide_or_col(row, col) * CELL_W + 2;
            assert_eq!(f[y * SCREEN_W + x], HIGHLIGHT);
        }
    }
}

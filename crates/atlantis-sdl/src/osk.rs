//! The on-screen keyboard: a strip of letters over the top of the frame,
//! for the game's name entry (a handheld has no keyboard).
//!
//! From Klaas Van den Broeck (@Klaasvdb),
//! github.com/Klaasvdb/atlantis-native-handheld (GPL-3.0-or-later), brought
//! into Atlantis Native with his agreement.
//!
//! Left / right pick a cell (held: repeats), A types it, B erases, X types a
//! space, Y switches upper / lower case, Start sends Enter and closes the
//! strip, Select closes it.

use atlantis_engine::sys::Key;
use atlantis_engine::warp::SCREEN_W;

/// 5x7 glyphs, one row per byte (bit 4 = leftmost column): A..Z, then the
/// space cell and the erase cell.
const GLYPHS: [[u8; 7]; 28] = [
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
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1f], // space
    [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02], // erase
];
const SPACE: usize = 26;
const ERASE: usize = 27;

const CELL: usize = 22;
const TOP: usize = 6;
const BG: u16 = 0x0000;
const FG: u16 = 0xffff;
const HIGHLIGHT: u16 = 0xfd20; // orange

/// A move held down repeats after this many frames, then every few.
const REPEAT_DELAY: u32 = 18;
const REPEAT_EVERY: u32 = 5;

#[derive(Default)]
pub struct Osk {
    pub active: bool,
    cell: usize,
    lower: bool,
    /// The direction held (-1, 0, 1) and for how many frames.
    held: (i32, u32),
}

impl Osk {
    pub fn open(&mut self) {
        self.active = true;
        self.held = (0, 0);
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    /// The cell left / right held this frame (-1, 0 or 1): moves the
    /// selection on the press and on the repeats.
    pub fn tick(&mut self, dir: i32) {
        if dir != self.held.0 {
            self.held = (dir, 0);
        } else {
            self.held.1 += 1;
        }
        let (d, n) = self.held;
        if d != 0
            && (n == 0 || (n >= REPEAT_DELAY && (n - REPEAT_DELAY).is_multiple_of(REPEAT_EVERY)))
        {
            self.cell = (self.cell as i32 + d).rem_euclid(GLYPHS.len() as i32) as usize;
        }
    }

    /// The key A types.
    pub fn typed(&self) -> Key {
        match self.cell {
            SPACE => Key::Space,
            ERASE => Key::Backspace,
            c => {
                let ch = (b'A' + c as u8) as char;
                Key::Char(if self.lower {
                    ch.to_ascii_lowercase()
                } else {
                    ch
                })
            }
        }
    }

    pub fn toggle_case(&mut self) {
        self.lower = !self.lower;
    }

    /// Draws the strip over the RGB565 frame.
    pub fn draw(&self, frame: &mut [u16]) {
        if !self.active {
            return;
        }
        let width = GLYPHS.len() * CELL;
        let left = (SCREEN_W - width) / 2;
        for y in TOP - 2..TOP + CELL + 2 {
            frame[y * SCREEN_W + left - 2..y * SCREEN_W + left + width + 2].fill(BG);
        }
        for (i, glyph) in GLYPHS.iter().enumerate() {
            let x0 = left + i * CELL;
            let colour = if i == self.cell { HIGHLIGHT } else { FG };
            if i == self.cell {
                for y in TOP..TOP + CELL {
                    for x in [x0, x0 + CELL - 1] {
                        frame[y * SCREEN_W + x] = colour;
                    }
                }
                frame[TOP * SCREEN_W + x0..TOP * SCREEN_W + x0 + CELL].fill(colour);
                let bottom = (TOP + CELL - 1) * SCREEN_W;
                frame[bottom + x0..bottom + x0 + CELL].fill(colour);
            }
            // Capitals drawn twice the size; small letters at their size.
            let letter = i < SPACE;
            let scale = if letter && self.lower { 1 } else { 2 };
            let (gw, gh) = (5 * scale, 7 * scale);
            let (gx, gy) = (x0 + (CELL - gw) / 2, TOP + (CELL - gh) / 2);
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..5 {
                    if bits & (0x10 >> col) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let (x, y) = (gx + col * scale + dx, gy + row * scale + dy);
                            frame[y * SCREEN_W + x] = colour;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_engine::warp::SCREEN_H;

    #[test]
    fn moves_types_and_repeats() {
        let mut k = Osk::default();
        k.open();
        assert_eq!(k.typed(), Key::Char('A'));
        k.tick(1);
        assert_eq!(k.typed(), Key::Char('B'));
        // Held: no move until the repeat delay, then every few frames.
        for _ in 1..REPEAT_DELAY {
            k.tick(1);
        }
        assert_eq!(k.typed(), Key::Char('B'));
        k.tick(1);
        assert_eq!(k.typed(), Key::Char('C'));
        k.tick(0);
        k.tick(-1);
        k.tick(0);
        k.tick(-1);
        k.tick(0);
        k.tick(-1);
        assert_eq!(k.typed(), Key::Backspace, "wraps to the erase cell");
        k.tick(0);
        k.tick(-1);
        assert_eq!(k.typed(), Key::Space);
        k.tick(0);
        k.tick(1);
        k.tick(0);
        k.tick(1);
        k.toggle_case();
        assert_eq!(k.typed(), Key::Char('a'));
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
        assert!(f[(SCREEN_H - 1) * SCREEN_W..].iter().all(|&p| p == 0x1234));
    }
}

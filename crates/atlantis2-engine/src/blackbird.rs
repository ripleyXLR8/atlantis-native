//! The blackbird songs of Ireland (the poet `POETE_ASSIS` in view 73 of
//! `ilepoete.wam`): the pieces of the original's code that hold no engine
//! state, the random draws, the grid test and the fixed menu's geometry.
//! The game itself is in the engine (`engine/blackbird.rs`).
//!
//! - `random(n)` (@ 0x004958c0) is `rand() · n / 32767` toward zero with
//!   the C runtime's `rand` (MSVC's generator, one state for the whole
//!   program): 0..n − 1 nearly evenly, and n itself for `rand()` = 32767
//!   only.
//! - The five songs (@ 0x004959a0, at the view's entry): 1 + `random(10)`
//!   each, drawn again while equal to one drawn before; 11 for
//!   `rand()` = 32767, a song with no file.
//! - The grid test (@ 0x00485620): a pointer inside a grid of cells, both
//!   far edges included, numbered column by column or row by row; −1
//!   outside.

use crate::crt::Rand;

/// The view of the game (@ 0x004959f9, the `IR` prefix tested by the
/// dispatchers).
pub const VIEW: u32 = 73;

/// The answer sounds (@ 0x005e1fb4 bad, @ 0x005e1f74 good).
pub const BAD_SOUND: &str = "merlem.apc";
pub const GOOD_SOUND: &str = "merleb.apc";

/// The character whose click starts a round (@ 0x005e1fc0), the variable
/// of the win (@ 0x005e20d0).
pub const POET: &[u8] = b"POETE_ASSIS";
pub const WIN_VAR: &str = "FINMERLE";

/// Right answers in a row that win (@ 0x00495b81).
pub const WIN_COUNT: i32 = 3;

/// The fixed menu (@ 0x0047fc70): the subjects bank's frames 26..30 in one
/// column, the menu registers at (450, 180), each bird centred on
/// (x + 25, y + 25 + 62 k) and its ring (frame 0) on (x + 31, y + 31 + 62 k).
pub const MENU_X: i32 = 450;
pub const MENU_Y: i32 = 180;
pub const FIRST_BIRD_FRAME: i32 = 26;
pub const BIRDS: i32 = 5;
pub const CELL: i32 = 62;

/// `random(n)` (@ 0x004958c0): `rand() · n` divided by 32767 by the
/// compiler's multiply (0x80010003, shift 14, sign bit added).
pub fn random(rand: &mut Rand, n: i32) -> i32 {
    let p = rand.rand().wrapping_mul(n);
    let high = ((i64::from(p) * i64::from(0x8001_0003_u32 as i32)) >> 32) as i32;
    let q = high.wrapping_add(p) >> 14;
    q.wrapping_add(((q as u32) >> 31) as i32)
}

/// The five songs (@ 0x004959a0): song i = 1 + `random(10)`, drawn again
/// while it equals one of songs 0..i − 1.
pub fn draw_songs(rand: &mut Rand) -> [i32; 5] {
    let mut songs = [0; 5];
    for i in 0..5 {
        loop {
            let v = random(rand, 10) + 1;
            songs[i] = v;
            if !songs[..i].contains(&v) {
                break;
            }
        }
    }
    songs
}

/// The song file of song number `n` (`"merle"`, `_itoa(n, 10)`, `".apc"`,
/// @ 0x00495c0b..0x00495c99).
pub fn song_file(n: i32) -> String {
    format!("merle{n}.apc")
}

/// The command the poet's question runs for the right bird's song
/// (@ 0x00495ce2..0x00495dc8): `(dial=90)` .. `(dial=99)` for songs
/// 1..10, none (a fatal error) otherwise.
pub fn question(song: i32) -> Option<String> {
    (1..=10)
        .contains(&song)
        .then(|| format!("(dial={})", 89 + song))
}

/// The grid test (@ 0x00485620): the cell under `pointer` of a grid at
/// (x0, y0) of `columns` × `rows` cells of w × h, numbered column by
/// column when `column_major`; −1 outside. The far edges are inside
/// (x0 + w · columns, y0 + h · rows), so the last pixel column and row
/// answer the cells past the grid.
#[allow(clippy::too_many_arguments)]
pub fn grid(
    x0: i32,
    y0: i32,
    w: i32,
    h: i32,
    columns: i32,
    rows: i32,
    column_major: bool,
    pointer: (i32, i32),
) -> i32 {
    let (px, py) = pointer;
    if px < x0
        || px > w.wrapping_mul(columns).wrapping_add(x0)
        || py < y0
        || py > h.wrapping_mul(rows).wrapping_add(y0)
    {
        return -1;
    }
    let (cx, cy) = (px.wrapping_sub(x0) / w, py.wrapping_sub(y0) / h);
    if column_major {
        cx.wrapping_mul(rows).wrapping_add(cy)
    } else {
        cy.wrapping_mul(columns).wrapping_add(cx)
    }
}

/// The blackbird's grid (@ 0x00495a86, 0x00495b1f, 0x00495bbc): one
/// column of `rows` cells of 62 from the menu registers less 31.
pub fn bird_cell(registers: (i32, i32), rows: i32, pointer: (i32, i32)) -> i32 {
    grid(
        registers.0 - 31,
        registers.1 - 31,
        CELL,
        CELL,
        1,
        rows,
        true,
        pointer,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_edges() {
        // rand() = 32767 gives n itself.
        let inv = 0xb9b3_3155_u32; // 214013⁻¹ mod 2³²
        assert_eq!(214_013_u32.wrapping_mul(inv), 1);
        let forced = |r: u32| Rand::new(((r << 16).wrapping_sub(2_531_011)).wrapping_mul(inv));
        assert_eq!(random(&mut forced(32767), 5), 5);
        assert_eq!(random(&mut forced(32766), 5), 4);
        assert_eq!(random(&mut forced(6553), 5), 0);
        assert_eq!(random(&mut forced(6554), 5), 1);
    }

    #[test]
    fn grids() {
        assert_eq!(bird_cell((450, 180), 5, (419, 149)), 0);
        assert_eq!(bird_cell((450, 180), 5, (481, 149)), 5);
        assert_eq!(bird_cell((450, 180), 5, (481, 459)), 10);
        assert_eq!(bird_cell((450, 180), 5, (450, 460)), -1);
        assert_eq!(bird_cell((450, 180), 3, (450, 336)), -1);
        assert_eq!(question(1).as_deref(), Some("(dial=90)"));
        assert_eq!(question(11), None);
    }
}

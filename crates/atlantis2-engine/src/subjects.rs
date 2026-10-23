//! The subject menu's geometry: where the draw (@ 0x0047fb20) puts the
//! cells and which cell a click (@ 0x0047fd00, the grid test
//! @ 0x00485620) picks.
//!
//! The cells are 62 pixels square. The first column is at x = 450, the
//! next ones 62 pixels to the right; a column holds 4 subjects; the first
//! row is at y = 366 − 62 × count when there are fewer than 5 subjects (the
//! column ends at y = 366), else at 180. Each cell draws its subject's
//! frame of the world's `suj*.spr` centred on (x + 25, y + 25), at 50 %
//! when the subject's flag is not 1 (asked, or disabled), then frame 0 (the
//! ring) centred on (x + 31, y + 31).
//!
//! The click grid starts at (434, first row − 16) with `count / 5 + 1`
//! columns of `min(count, 4)` rows, both edges included, and numbers the
//! cells column by column: with 9, 13 or 14 subjects the last column
//! drawn cannot be clicked, and a click on an empty cell of the last
//! column (or on the one-pixel edge after the last row or column) answers
//! the entry the memory holds there, past the list.

/// The first column (@ 0x006cf298 = 0x1c2).
pub const COLUMN_X: i32 = 450;
/// A cell's side.
pub const CELL: i32 = 62;
/// Subjects per column of the drawing.
pub const PER_COLUMN: i32 = 4;

/// The menu's state: open (@ 0x006617b0), the column (@ 0x006cf298) and
/// the first row (@ 0x006cf29c) the last draw left, which the clicks read
/// (the subject menu's sets the column back to 450 first, the blackbird
/// game's does not).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SubjectMenu {
    pub open: bool,
    pub column_x: i32,
    pub first_row: i32,
}

/// The first row of a list of `count` subjects.
pub fn first_row(count: i32) -> i32 {
    if count < 5 {
        366 - CELL * count
    } else {
        180
    }
}

/// The column the draw leaves in its register (@ 0x006cf298): 450, moved
/// right by 62 at entries 4, 8 and 12.
pub fn last_column(count: i32) -> i32 {
    COLUMN_X + CELL * (count.saturating_sub(1) / PER_COLUMN).clamp(0, 3)
}

/// The top-left corner of entry `i`'s cell in a list of `count`
/// subjects. The draw moves to the next column only at entries 4, 8 and
/// 12, so a 17th entry and later go on down the fourth column.
pub fn cell(count: i32, i: i32) -> (i32, i32) {
    let column = (i / PER_COLUMN).min(3);
    let top = if column == 0 {
        first_row(count)
    } else {
        180 - CELL * PER_COLUMN * column
    };
    (COLUMN_X + CELL * column, top + CELL * i)
}

/// The entry a click at `pointer` picks in a list of `count` subjects
/// whose draw left `first_row`; none outside the grid (the menu closes).
/// The index is not checked against the count; with no subject the grid
/// has no row, and its top line (x 434..496) answers entry 0.
pub fn hit(count: i32, first_row: i32, pointer: (i32, i32)) -> Option<usize> {
    let (x0, y0) = (COLUMN_X - 16, first_row - 16);
    let columns = count / 5 + 1;
    let rows = count.min(PER_COLUMN);
    let (px, py) = pointer;
    if px < x0 || px > x0 + CELL * columns || py < y0 || py > y0 + CELL * rows {
        return None;
    }
    usize::try_from((px - x0) / CELL * rows + (py - y0) / CELL).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_and_clicks() {
        assert_eq!(cell(2, 0), (450, 242));
        assert_eq!(cell(2, 1), (450, 304));
        assert_eq!(cell(9, 4), (512, 180));
        assert_eq!(cell(9, 8), (574, 180));
        assert_eq!(cell(16, 15), (636, 366));
        assert_eq!(cell(17, 16), (636, 428));
        // One subject: its 62-pixel cell, the edges after it answer the
        // next entries.
        assert_eq!(hit(1, 304, (434, 288)), Some(0));
        assert_eq!(hit(1, 304, (495, 349)), Some(0));
        assert_eq!(hit(1, 304, (496, 349)), Some(1));
        assert_eq!(hit(1, 304, (496, 350)), Some(2));
        assert_eq!(hit(1, 304, (433, 300)), None);
        // No subject: the line y0 - 16 answers entry 0.
        assert_eq!(hit(0, 366, (434, 350)), Some(0));
        assert_eq!(hit(0, 366, (496, 350)), Some(0));
        assert_eq!(hit(0, 366, (497, 350)), None);
        assert_eq!(hit(0, 366, (450, 351)), None);
        // Nine subjects: two columns of the grid for three drawn.
        assert_eq!(hit(9, 180, (560, 200)), None);
    }
}

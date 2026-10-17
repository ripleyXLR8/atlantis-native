//! The layout and hit test of the dialog subject menu.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`subject_menu.h`,
//! `menu_layout.cpp` `menuGridHit`; GPLv3, the ScummVM team).
//! `FUN_004261a0` draws n subjects in columns of four from x = 450 (the
//! column moves right at subjects 4, 8 and 12 only); `FUN_00425828`
//! hit-tests with `FUN_004211f0` from x = 450 (it sets 0x006cd83c = 0x1c2
//! at 0x004259d8) and the first column's y the draw left in 0x006cd840.
//! The grid has n / 5 + 1 columns (fewer than the drawn ones from nine
//! subjects on), min(n, 4) rows, column-major, edges inclusive. An index at
//! or past n reads an empty slot (subject 0).

/// One sprite of the menu: its position and SUJETS.SPR frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuDraw {
    pub x: i32,
    pub y: i32,
    pub frame: i32,
}

/// The draws (per subject: the border, frame 0, at the cell's top-left,
/// then the icon, frame = the subject id, at its centre) and the grid
/// origin of the hit test.
pub fn menu_draws(ids: &[i32]) -> (Vec<MenuDraw>, i32, i32) {
    let n = ids.len() as i32;
    let mut x0 = 0x1c2;
    let mut y0 = if n <= 4 { 180 - (n - 3) * 50 } else { 180 };
    let first_y = y0;
    let mut out = Vec::with_capacity(2 * ids.len());
    for (i, &id) in ids.iter().enumerate() {
        let i = i as i32;
        if i == 4 || i == 8 || i == 12 {
            x0 += 50;
            y0 = 180 - 50 * i;
        }
        let y = i32::from((y0 + 50 * i) as i16);
        out.push(MenuDraw { x: x0, y, frame: 0 });
        out.push(MenuDraw {
            x: x0 + 25,
            y: y + 25,
            frame: id,
        });
    }
    // The draw leaves the last column's x in 0x006cd83c, which
    // FUN_00425828 resets to 0x1c2 (0x004259d8).
    (out, 0x1c2, first_y)
}

/// `FUN_004211f0`: the cell under (mx, my), -1 outside; edges inclusive.
#[allow(clippy::too_many_arguments)]
pub fn menu_grid_hit(
    x0: i32,
    y0: i32,
    cell_w: i32,
    cell_h: i32,
    cols: i32,
    rows: i32,
    col_major: bool,
    mx: i32,
    my: i32,
) -> i32 {
    if mx < x0 || mx > x0 + cols * cell_w || my < y0 || my > y0 + rows * cell_h {
        return -1;
    }
    let (col, row) = ((mx - x0) / cell_w, (my - y0) / cell_h);
    if col_major {
        col * rows + row
    } else {
        row * cols + col
    }
}

/// `FUN_004211f0` as called at 0x00425a53..0x00425a76.
pub fn menu_hit(n: i32, grid_x: i32, grid_y: i32, mx: i32, my: i32) -> i32 {
    menu_grid_hit(grid_x, grid_y, 50, 50, n / 5 + 1, n.min(4), true, mx, my)
}

/// The subject byte `FUN_00425828` reads for a cell (0x00425a87..): 0 past
/// the n subjects; -1 outside the grid or past the 0xe1-byte record.
pub fn menu_subject(ids: &[i32], idx: i32) -> i32 {
    if idx < 0 || 0xe + idx * 0xe >= 0xe1 {
        return -1;
    }
    ids.get(idx as usize).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cell_reads_subject_zero() {
        let ids = [1, 2, 3, 4, 5];
        let (_, gx, gy) = menu_draws(&ids);
        // Column 1, row 1 (cell 5): no subject drawn there.
        let idx = menu_hit(5, gx, gy, gx + 75, gy + 75);
        assert_eq!(idx, 5);
        assert_eq!(menu_subject(&ids, idx), 0);
        assert_eq!(menu_subject(&ids, menu_hit(5, gx, gy, 10, 10)), -1);
    }
}

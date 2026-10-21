//! StartPuzzle 2 at CHAPI018: the ogre and the hero (`FUN_0044203c`). On a
//! 7 x 7 board the player drags the hero one or two cells a move (straight,
//! or with an elbow); then each of the two ogres steps toward him, and the
//! hidden queen takes a random diagonal step that avoids the hero's path.
//! The queen shows herself every fourth move. A path over the queen's cell
//! catches her (255); an ogre on the hero's cell loses (2); a click on the
//! left or right strip leaves (1).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`ogre_rules.cpp`,
//! `ogre_draw.h`, `puzzle_ogregame.cpp`; GPLv3, the ScummVM team).

use super::{load_spr, load_tga, puzzle_lcg_next, PuzzleHost, PuzzleSounds, SamplePlayer};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;

/// A piece record (0x00495e4c, 3 s16): flag, x, y.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Piece {
    pub flag: i32,
    pub x: i32,
    pub y: i32,
}

pub const HERO: usize = 0;
pub const QUEEN: usize = 1;
pub const OGRE_A: usize = 2;
pub const NUM_PIECES: usize = 4;

/// The cell edges at 0x0049743c: 7 column starts and the board's right
/// edge, then 7 row starts and a value whose + 60 bounds the last row.
pub const COL_EDGE: [i32; 8] = [112, 173, 233, 294, 354, 415, 476, 536];
pub const ROW_EDGE: [i32; 8] = [27, 88, 148, 209, 269, 330, 390, 451];
const CELL: i32 = 60;
const BOARD_X: i32 = 0x71;
const BOARD_Y: i32 = 0x1d;

/// [0x00496d14], [0x00496d10], [0x00496d0c].
pub const WIN: i32 = 255;
pub const LOSE: i32 = 2;
pub const ABORT: i32 = 1;

/// The 60 one-pixel frames of an ogre's slide (0x0044266e: cmp 0x3c).
pub const SLIDE_FRAMES: i32 = 60;

/// Where a piece in cell (x, y) is drawn.
pub fn piece_x(x: i32) -> i32 {
    x * CELL + BOARD_X
}
pub fn piece_y(y: i32) -> i32 {
    y * CELL + BOARD_Y
}

/// `FUN_00443ea0`: the column under a screen x through the edge table, 99
/// off the board.
pub fn cell_column(mx: i32) -> i32 {
    let mut cell = 99;
    for (i, &e) in COL_EDGE[..7].iter().enumerate() {
        if e > mx || COL_EDGE[7] <= mx {
            break;
        }
        cell = i as i32;
    }
    cell
}

/// `FUN_00443f18`: the row under a screen y, 99 off the board.
pub fn cell_row(my: i32) -> i32 {
    let mut cell = 99;
    for (i, &e) in ROW_EDGE[..7].iter().enumerate() {
        if e > my || ROW_EDGE[7] + CELL <= my {
            break;
        }
        cell = i as i32;
    }
    cell
}

/// `FUN_00443c3c`'s leave strip.
pub fn abort_click(mx: i32) -> bool {
    !(0x32..=0x24e).contains(&mx)
}

/// `FUN_004422b8`: the queen's cell, two draws of the puzzle LCG
/// (`FUN_00447524(6)`) a try until x + y >= 10 and (6 - x) + (6 - y) >= 2:
/// (4, 6), (5, 5) or (6, 4).
pub fn queen_cell(seed: &mut u16) -> (i32, i32) {
    loop {
        let x = puzzle_lcg_next(seed, 6);
        let y = puzzle_lcg_next(seed, 6);
        if x + y >= 10 && (6 - x) + (6 - y) >= 2 {
            return (x, y);
        }
    }
}

/// The rules on the original's globals: `FUN_00443f94` (path and mask),
/// `FUN_00443c3c` (pick-up, drop, catch), `FUN_00442360` (the ogres' turn)
/// and `FUN_00442a0c` (the queen's walk).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OgreRules {
    /// Hero, queen (flag 0: hidden), ogre A, ogre B.
    pub p: [Piece; NUM_PIECES],
    /// The occupancy (0x007738b8). The scans index past its 49 cells into
    /// bytes that stay 0; the padding stands for them.
    pub occ: [u8; 64],
    /// The legal path cells while the hero is held (0x00773938).
    pub mask: [u8; 49],
    /// Origin, elbow, destination (0x00773870).
    pub path_x: [i32; 3],
    pub path_y: [i32; 3],
}

impl Default for OgreRules {
    fn default() -> Self {
        OgreRules {
            p: [Piece::default(); NUM_PIECES],
            occ: [0; 64],
            mask: [0; 49],
            path_x: [-1; 3],
            path_y: [-1; 3],
        }
    }
}

impl OgreRules {
    fn occ_at(&self, i: i32) -> u8 {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.occ.get(i).copied())
            .unwrap_or(0)
    }

    /// `FUN_0044203c` steps 2-3 (0x00442054..0x004422b7): the occupancy
    /// cleared (the ogres' cells are not marked), hero {1, 0, 0}, queen
    /// flag 0, ogre A {1, 6, 6}, ogre B {1, 0, 6} (its position written by
    /// `FUN_004422b8`). The queen's cell is [`queen_cell`]'s.
    pub fn reset_board(&mut self) {
        self.occ = [0; 64];
        self.p[HERO] = Piece {
            flag: 1,
            x: 0,
            y: 0,
        };
        self.p[QUEEN].flag = 0;
        self.p[OGRE_A] = Piece {
            flag: 1,
            x: 6,
            y: 6,
        };
        self.p[3] = Piece {
            flag: 1,
            x: 0,
            y: 6,
        };
    }

    /// `FUN_00443f94`, every pass while the hero is held. Off the board the
    /// mask and path stay as they are; on a cell out of reach only the
    /// origin is written (the elbow and destination keep the last legal
    /// hover).
    pub fn build_move_mask(&mut self, mx: i32, my: i32) {
        let cx = cell_column(mx);
        let cy = cell_row(my);
        if cx >= 7 || cy >= 7 {
            return;
        }
        self.mask = [0; 49];
        let (hx, hy) = (self.p[HERO].x, self.p[HERO].y);
        let (dx, dy) = (cx - hx, cy - hy);
        let dist = dx.abs() + dy.abs();
        if (1..=2).contains(&dist) {
            let (mid_x, mid_y) = if dx == 0 || dy == 0 {
                (hx + dx / 2, hy + dy / 2)
            } else {
                // The elbow follows the cursor's offset from the hero's
                // cell centre (0x0044409f..0x0044410e).
                let ex = mx - (COL_EDGE[hx as usize] + 30);
                let ey = my - (ROW_EDGE[hy as usize] + 30);
                if ex.abs() > ey.abs() {
                    (cx, hy)
                } else {
                    (hx, cy)
                }
            };
            self.mask[(mid_y * 7 + mid_x) as usize] = 1;
            self.mask[(cy * 7 + cx) as usize] = 1;
            self.path_x[1] = mid_x;
            self.path_y[1] = mid_y;
            self.path_x[2] = cx;
            self.path_y[2] = cy;
        }
        self.mask[(hy * 7 + hx) as usize] = 1;
        self.path_x[0] = hx;
        self.path_y[0] = hy;
    }

    /// `FUN_00443c3c`'s pick-up rectangle.
    pub fn pick_up(&self, mx: i32, my: i32) -> bool {
        let px = self.p[HERO].x * CELL;
        let py = self.p[HERO].y * CELL;
        mx > px + 0x78 && mx < px + 0x9f && my > py + 0x1d && my < py + 0x54
    }

    /// The cell `FUN_00443c3c` drops on: C division, toward zero
    /// (0x00443c9b..0x00443cc2).
    pub fn drop_cell(mx: i32, my: i32) -> (i32, i32) {
        ((mx - BOARD_X) / CELL, (my - BOARD_Y) / CELL)
    }

    /// The drop on a free cell: false when refused.
    pub fn drop(&mut self, mx: i32, my: i32) -> bool {
        let (cx, cy) = Self::drop_cell(mx, my);
        if self.occ_at(cy * 7 + cx) != 0 {
            return false;
        }
        if !(0..7).contains(&cx) || !(0..7).contains(&cy) {
            return false;
        }
        if self.mask[(cy * 7 + cx) as usize] != 1 {
            return false;
        }
        self.p[HERO] = Piece {
            flag: 1,
            x: cx,
            y: cy,
        };
        true
    }

    /// The catch: the queen on one of the three path cells.
    pub fn queen_caught(&self) -> bool {
        (0..3).any(|i| self.p[QUEEN].x == self.path_x[i] && self.p[QUEEN].y == self.path_y[i])
    }

    /// `FUN_00442360`, ogre `o`'s step. When the target cell is taken the
    /// original scans the clamped 3 x 3 box testing occ[yy * 7 + x0 + xx]
    /// and takes (x0, yy) for each free hit (sic). With no free hit it uses
    /// uninitialised locals, which no state reachable from the start
    /// needs; the ogre then stays.
    pub fn ogre_step(&self, o: usize) -> (i32, i32) {
        let ogre = self.p[o];
        let hero = self.p[HERO];
        let mut sx = (hero.x - ogre.x).signum();
        let mut sy = (hero.y - ogre.y).signum();
        if !(0..=6).contains(&(ogre.x + sx)) {
            sx = 0;
        }
        if !(0..=6).contains(&(ogre.y + sy)) {
            sy = 0;
        }
        if self.occ_at((ogre.y + sy) * 7 + ogre.x + sx) != 0 {
            let (mut nx, mut ny) = (ogre.x, ogre.y);
            let (x0, x1) = ((ogre.x - 1).max(0), (ogre.x + 1).min(6));
            let (y0, y1) = ((ogre.y - 1).max(0), (ogre.y + 1).min(6));
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    if self.occ_at(yy * 7 + x0 + xx) == 0 {
                        nx = x0;
                        ny = yy;
                    }
                }
            }
            sx = nx - ogre.x;
            sy = ny - ogre.y;
        }
        if !(0..=6).contains(&(ogre.x + sx)) {
            sx = 0;
        }
        if !(0..=6).contains(&(ogre.y + sy)) {
            sy = 0;
        }
        (sx, sy)
    }

    /// The step made: the old cell freed, the new one taken.
    pub fn move_ogre(&mut self, o: usize, sx: i32, sy: i32) {
        let ogre = &mut self.p[o];
        self.occ[(ogre.y * 7 + ogre.x) as usize] = 0;
        ogre.x += sx;
        ogre.y += sy;
        self.occ[(ogre.y * 7 + ogre.x) as usize] = 1;
    }

    /// The ogres' turn without its slides: ogre A then B, [`LOSE`] at a
    /// catch, else 0.
    pub fn move_ogres(&mut self) -> i32 {
        for o in OGRE_A..NUM_PIECES {
            if self.p[o].flag == 1 {
                let (sx, sy) = self.ogre_step(o);
                self.move_ogre(o, sx, sy);
            }
            if self.p[o].x == self.p[HERO].x && self.p[o].y == self.p[HERO].y {
                return LOSE;
            }
        }
        0
    }

    /// `FUN_00442a0c`: the queen's step, `rand32767` the C rand. Up to 200
    /// tries for a cell off the hero's path; she stays when they fail.
    pub fn walk_queen(&mut self, mut rand32767: impl FnMut() -> i32) {
        let (mut sx, mut sy) = (1, 1);
        let mut tries = 0;
        loop {
            if rand32767() > 16000 {
                sx = -sx;
            }
            if rand32767() > 16000 {
                sy = -sy;
            }
            let q = self.p[QUEEN];
            if q.x + sx > 6 || q.x + sx < 0 {
                sx = -sx;
            }
            if q.y + sy > 6 || q.y + sy < 0 {
                sy = -sy;
            }
            if tries >= 200 {
                sx = 0;
                sy = 0;
                break;
            }
            let (qx, qy) = (q.x + sx, q.y + sy);
            if !(0..3).any(|i| qx == self.path_x[i] && qy == self.path_y[i]) {
                break;
            }
            tries += 1;
        }
        self.p[QUEEN].x += sx;
        self.p[QUEEN].y += sy;
    }
}

/// The game's I/O.
#[allow(async_fn_in_trait)]
pub trait OgreIo {
    /// `FUN_0041a298` then `FUN_0043318c`: the mouse and a new left
    /// click.
    fn read_input(&mut self, g: &OgreGame) -> (i32, i32, bool);
    /// `FUN_00410f36`: the background, or the copy of the frame.
    fn draw_background(&mut self);
    /// 'P' PUZCYCLO, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    /// `FUN_0042d558`: the standard cursor at the mouse.
    fn draw_standard_cursor(&mut self, x: i32, y: i32);
    /// SetCursorPos.
    fn set_cursor_pos(&mut self, x: i32, y: i32);
    /// `FUN_0041afd0`.
    async fn present(&mut self, g: &OgreGame);
    /// The busy wait on the 200 Hz timer.
    async fn wait_ticks(&mut self, n: u32);
    /// cyclo1..cyclo6 (`FUN_0043ca28`).
    fn play_sound(&mut self, n: i32);
    /// `FUN_0043d0e0`.
    fn stop_sounds(&mut self) {}
    /// `FUN_00453cd1`.
    fn rand32767(&mut self) -> i32;
    fn aborted(&self) -> bool {
        false
    }
}

/// The ogre game: `FUN_0044203c` and its helpers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OgreGame {
    pub rules: OgreRules,
    /// 0x007beaa8: starts at 2, the queen shows at 4.
    pub moves: i32,
    /// 0x007bebf8: the hero on the cursor.
    pub held: bool,
    /// 0x007bebe8 / 0x007bebec: the mouse minus the hero's piece at the
    /// pick-up.
    pub grab_x: i32,
    pub grab_y: i32,
    /// 0x004974c0.
    pub result: i32,
    /// 0x004960ec / 0x004960f0, 0x0049611c.
    pub mx: i32,
    pub my: i32,
    pub click: bool,
    /// The queen reveal's frames and ticks per frame: 3 x 200
    /// (FUN_004436f4), 5 x 75 in the 1997 build (atlantis-cd-en-688128
    /// FUN_0043eebc); (0, 0) stands for the 2001 values.
    pub reveal: (u8, u16),
}

impl OgreGame {
    /// The set-up of `FUN_0044203c` (steps 2-3) and `FUN_004422b8`, with
    /// the puzzle LCG `seed`.
    pub fn new(seed: &mut u16) -> OgreGame {
        let mut g = OgreGame {
            moves: 2,
            ..OgreGame::default()
        };
        g.rules.reset_board();
        let (x, y) = queen_cell(seed);
        g.rules.p[QUEEN].x = x;
        g.rules.p[QUEEN].y = y;
        g
    }

    /// `FUN_004432f8` / `FUN_00443944`: cell (x, y), plain or lit.
    pub fn draw_cell(io: &mut impl OgreIo, x: i32, y: i32, lit: bool) {
        let (Some(&cx), Some(&cy)) = (
            usize::try_from(x).ok().and_then(|x| COL_EDGE.get(x)),
            usize::try_from(y).ok().and_then(|y| ROW_EDGE.get(y)),
        ) else {
            return;
        };
        io.draw_sprite('P', cx, cy, y * 7 + x + 0x34 - if lit { 0x31 } else { 0 });
    }

    /// The hero, 10 pixels right when he stands on the queen's cell.
    fn draw_hero(&self, io: &mut impl OgreIo) {
        let p = &self.rules.p;
        let on_queen = p[HERO].x == p[QUEEN].x && p[HERO].y == p[QUEEN].y;
        let shift = if on_queen { 10 } else { 0 };
        io.draw_sprite('P', piece_x(p[HERO].x) + shift, piece_y(p[HERO].y), 0);
    }

    fn draw_piece(io: &mut impl OgreIo, p: &Piece, frame: i32) {
        io.draw_sprite('P', piece_x(p.x), piece_y(p.y), frame);
    }

    /// `FUN_00442b88`: the background; while held, `FUN_00443f94`'s cells
    /// (the elbow and destination lit on a legal hover, else the
    /// destination plain, then the hero's cell lit; nothing off the
    /// board); the hero; records 1..3 with a flag, frame min(i, 2).
    pub fn draw_board(&mut self, io: &mut impl OgreIo, held: bool, mx: i32, my: i32) {
        io.draw_background();
        if held {
            let (cx, cy) = (cell_column(mx), cell_row(my));
            if cx < 7 && cy < 7 {
                self.rules.build_move_mask(mx, my);
                let r = &self.rules;
                let d = (cx - r.p[HERO].x).abs() + (cy - r.p[HERO].y).abs();
                if (1..=2).contains(&d) {
                    Self::draw_cell(io, r.path_x[1], r.path_y[1], true);
                    Self::draw_cell(io, cx, cy, true);
                } else {
                    Self::draw_cell(io, cx, cy, false);
                }
                Self::draw_cell(io, r.p[HERO].x, r.p[HERO].y, true);
            }
        }
        if self.rules.p[HERO].flag != 0 {
            self.draw_hero(io);
        }
        for i in 1..=3 {
            let p = self.rules.p[i];
            if p.flag != 0 {
                Self::draw_piece(io, &p, (i as i32).min(2));
            }
        }
    }

    /// `FUN_004439c4`. Held: the hero at the mouse minus the grab offset,
    /// kept in [0x5a, 0x1f4] x [0x16, 0x190] by moving the mouse. Free:
    /// CURSEURS frame 11 outside x 0x32..0x24e, else the standard cursor.
    pub fn draw_cursor(&mut self, io: &mut impl OgreIo) {
        if self.held {
            let mut dx = self.mx - self.grab_x;
            let mut dy = self.my - self.grab_y;
            if !(0x5a..=0x1f4).contains(&dx) || !(0x16..=0x190).contains(&dy) {
                dx = dx.clamp(0x5a, 0x1f4);
                dy = dy.clamp(0x16, 0x190);
                self.mx = dx + self.grab_x;
                self.my = dy + self.grab_y;
                io.set_cursor_pos(self.mx, self.my);
            }
            io.draw_sprite('P', dx, dy, 0);
        } else if abort_click(self.mx) {
            io.draw_sprite('C', self.mx, self.my, 11);
        } else {
            io.draw_standard_cursor(self.mx, self.my);
        }
    }

    /// The copy of the frame to the screen, the present and the busy
    /// wait of the sequences.
    async fn show(&self, io: &mut impl OgreIo, ticks: u32) {
        io.draw_background();
        io.present(self).await;
        io.wait_ticks(ticks).await;
    }

    /// `FUN_004436f4`: nothing when the queen shares an ogre's cell; else
    /// cyclo4, then 3 frames: the board, her cell plain / lit / plain, the
    /// queen; 200 ticks each. The 1997 build shows 5 frames of 75 ticks,
    /// plain / lit / plain / lit / plain (atlantis-cd-en-688128
    /// FUN_0043eebc).
    pub async fn reveal(&mut self, io: &mut impl OgreIo) {
        let p = self.rules.p;
        if (2..=3).any(|o| p[QUEEN].x == p[o].x && p[QUEEN].y == p[o].y) {
            return;
        }
        let (frames, ticks) = match self.reveal {
            (0, _) => (3, 0xc8),
            r => r,
        };
        io.play_sound(4);
        let mut lit = false;
        for _ in 0..frames {
            self.draw_board(io, false, 0, 0);
            Self::draw_cell(io, p[QUEEN].x, p[QUEEN].y, lit);
            Self::draw_piece(io, &p[QUEEN], 1);
            lit = !lit;
            self.show(io, u32::from(ticks)).await;
        }
    }

    /// `FUN_00443378(0)`: 5 frames: the three path cells (lit on odd
    /// frames), the hero, the queen, records 2..3 with a flag; 150 ticks
    /// each.
    pub async fn win(&mut self, io: &mut impl OgreIo) {
        let mut lit = false;
        for _ in 0..5 {
            io.draw_background();
            let r = &self.rules;
            for k in 0..3 {
                Self::draw_cell(io, r.path_x[k], r.path_y[k], lit);
            }
            self.draw_hero(io);
            Self::draw_piece(io, &r.p[QUEEN], 1);
            for o in 2..=3 {
                if r.p[o].flag != 0 {
                    Self::draw_piece(io, &r.p[o], 2);
                }
            }
            lit = !lit;
            self.show(io, 0x96).await;
        }
    }

    /// `FUN_00442d78(0)` (its argument, 0x007beabc, is 0): 5 cycles of
    /// two frames: the 49 cells plain (column by column), the hero's cell
    /// (lit on odd frames), the hero, records 2..3 with a flag; the second
    /// frame draws the hero again on top; 100 ticks each.
    pub async fn lose(&mut self, io: &mut impl OgreIo) {
        let mut lit = false;
        for _ in 0..5 {
            for second in [false, true] {
                io.draw_background();
                for x in 0..7 {
                    for y in 0..7 {
                        Self::draw_cell(io, x, y, false);
                    }
                }
                let p = self.rules.p;
                Self::draw_cell(io, p[HERO].x, p[HERO].y, lit);
                Self::draw_piece(io, &p[HERO], 0);
                for q in &p[2..=3] {
                    if q.flag != 0 {
                        Self::draw_piece(io, q, 2);
                    }
                }
                if second {
                    Self::draw_piece(io, &p[HERO], 0);
                }
                lit = !lit;
                self.show(io, 0x64).await;
            }
        }
    }

    /// One frame of ogre `o`'s slide (`FUN_00442360`,
    /// 0x00442681..0x0044295e): the background, the hero frame 0 at his
    /// cell (no shift, whatever his flag), the queen frame 1 when her flag
    /// is 1, the ogres 2..o-1 with flag 1, ogre o at (px, py), the ogres
    /// o+1..3 with flag 1, all frame 2; the copy, the present, no wait.
    pub async fn slide_frame(&mut self, io: &mut impl OgreIo, o: usize, px: i32, py: i32) {
        io.draw_background();
        let p = self.rules.p;
        Self::draw_piece(io, &p[HERO], 0);
        if p[QUEEN].flag == 1 {
            Self::draw_piece(io, &p[QUEEN], 1);
        }
        for q in &p[2..o] {
            if q.flag == 1 {
                Self::draw_piece(io, q, 2);
            }
        }
        io.draw_sprite('P', px, py, 2);
        for q in &p[o + 1..] {
            if q.flag == 1 {
                Self::draw_piece(io, q, 2);
            }
        }
        io.draw_background();
        io.present(self).await;
    }

    /// `FUN_00442360`: each ogre's step, cyclo3 and its 60-frame slide
    /// (also for a zero step), then the loss test.
    async fn ogres_turn(&mut self, io: &mut impl OgreIo) {
        for o in OGRE_A..NUM_PIECES {
            if self.rules.p[o].flag == 1 {
                let (sx, sy) = self.rules.ogre_step(o);
                let ogre = self.rules.p[o];
                self.rules.occ[(ogre.y * 7 + ogre.x) as usize] = 0;
                io.play_sound(3);
                let (mut px, mut py) = (piece_x(ogre.x), piece_y(ogre.y));
                for _ in 0..SLIDE_FRAMES {
                    if io.aborted() {
                        break;
                    }
                    self.slide_frame(io, o, px, py).await;
                    px += sx;
                    py += sy;
                }
                self.rules.move_ogre(o, sx, sy);
            }
            let p = &self.rules.p;
            if p[o].x == p[HERO].x && p[o].y == p[HERO].y {
                self.result = LOSE;
                return;
            }
        }
    }

    /// `FUN_00443c3c`: the click at the mouse of the previous pass.
    async fn handle_click(&mut self, io: &mut impl OgreIo) {
        let (mx, my) = (self.mx, self.my);
        if !self.held {
            if self.rules.pick_up(mx, my) {
                io.play_sound(1);
                self.grab_x = mx - piece_x(self.rules.p[HERO].x);
                self.grab_y = my - piece_y(self.rules.p[HERO].y);
                self.held = true;
                self.rules.p[HERO].flag = 0;
            }
        } else {
            let (cx, cy) = OgreRules::drop_cell(mx, my);
            if self.rules.occ_at(cy * 7 + cx) != 0 {
                return;
            }
            if self.rules.drop(mx, my) {
                self.moves += 1;
                io.play_sound(2);
                self.held = false;
                if self.moves == 4 {
                    self.reveal(io).await;
                    self.moves = 0;
                }
                if self.rules.queen_caught() {
                    self.result = WIN;
                    return;
                }
                self.ogres_turn(io).await;
                self.rules.walk_queen(|| io.rand32767());
            }
        }
        if abort_click(mx) {
            self.result = ABORT;
        }
    }

    /// The input at a frame boundary. On the pass that raises the click
    /// flag `FUN_0041a298` returns without pumping the messages: the mouse
    /// stays where the previous pass left it.
    fn read_input(&mut self, io: &mut impl OgreIo) {
        let (x, y, click) = io.read_input(self);
        if !click {
            (self.mx, self.my) = (x, y);
        }
        self.click = click;
    }

    /// `FUN_0044203c` after the set-up: 255 won, 2 lost, 1 left.
    pub async fn run(&mut self, io: &mut impl OgreIo) -> i32 {
        self.read_input(io);
        self.click = false;
        self.result = 0;
        loop {
            if self.click {
                self.handle_click(io).await;
            }
            self.click = false;
            let (held, mx, my) = (self.held, self.mx, self.my);
            self.draw_board(io, held, mx, my);
            self.draw_cursor(io);
            io.draw_background();
            io.present(self).await;
            self.read_input(io);
            if self.result != 0 || io.aborted() {
                break;
            }
        }
        match self.result {
            WIN => {
                io.play_sound(4);
                self.win(io).await;
                io.stop_sounds();
            }
            LOSE => {
                io.play_sound(6);
                self.lose(io).await;
                io.stop_sounds();
            }
            _ => {}
        }
        if self.result == 0 {
            self.result = ABORT;
        }
        self.result
    }
}

/// The samples at 0x007739b8.. (cyclo5 is loaded and never played).
pub const SOUNDS: [&str; 6] = ["CYCLO1", "CYCLO2", "CYCLO3", "CYCLO4", "CYCLO5", "CYCLO6"];

struct Host<'a>(PuzzleHost<'a>);

impl OgreIo for Host<'_> {
    fn read_input(&mut self, _g: &OgreGame) -> (i32, i32, bool) {
        let m = self.0.read_input();
        (m.x, m.y, m.click)
    }
    fn draw_background(&mut self) {
        self.0.draw_background();
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.0.draw_sprite(bank, x, y, frame);
    }
    fn draw_standard_cursor(&mut self, x: i32, y: i32) {
        self.0.draw_standard_cursor(x, y);
    }
    fn set_cursor_pos(&mut self, x: i32, y: i32) {
        self.0.e.sys.set_mouse_pos(x, y);
    }
    async fn present(&mut self, _g: &OgreGame) {
        self.0.present().await;
    }
    async fn wait_ticks(&mut self, n: u32) {
        self.0.wait_ticks(n).await;
    }
    fn play_sound(&mut self, n: i32) {
        if let Some(name) = usize::try_from(n - 1).ok().and_then(|i| SOUNDS.get(i)) {
            self.0.play_sound(name, SamplePlayer::Ca28);
        }
    }
    fn stop_sounds(&mut self) {
        self.0.stop_samples();
    }
    /// `FUN_00442a0c`'s rand.
    fn rand32767(&mut self) -> i32 {
        self.0.e.puzzles.crt_rand()
    }
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

/// StartPuzzle 2: 255 the queen caught, 2 the hero caught, 1 left, a file
/// missing or the game quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let bg = load_tga(e, "PUZCYCLO.TGA").await;
    let frames = load_spr(e, "PUZCYCLO.SPR").await;
    let (Some(bg), Some(frames)) = (bg, frames) else {
        log::warn!("ogre: missing files");
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &SOUNDS).await;
    // FUN_0043a1e0 seeded the puzzle LCG from the clock.
    let mut game = OgreGame::new(&mut e.puzzles.lcg_seed);
    game.reveal = e.game.exe.build.queen_reveal;
    // The mouse unlocked. No SetCursorPos before the loop (the import slot
    // 0x00c302f8 has no reader in FUN_004240fc, FUN_0043a1e0, FUN_00448ad0
    // or FUN_0044203c; FUN_004439c4's is the held-piece clamp): the puzzle
    // starts with the pointer where the warp loop left it at the zone
    // click. The mouse of the previous pass (0x004960ec / 0x004960f0, as
    // the FUN_004439c4 clamp at 0x00442178 leaves it) starts there too.
    let mut host = Host(PuzzleHost::new(e, hooks));
    (game.mx, game.my) = host.0.e.sys.mouse();
    host.0.screen.pictures = vec![bg];
    host.0.screen.frames = frames;
    host.0.sounds = sounds;
    let r = game.run(&mut host).await;
    if host.0.e.should_abort() {
        1
    } else {
        r
    }
}

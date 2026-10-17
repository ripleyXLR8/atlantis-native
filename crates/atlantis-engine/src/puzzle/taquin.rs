//! StartPuzzle 6, 7 and 14 at CHAPI045..051: the three 3x3 sliding
//! puzzles (`FUN_00446300`, `FUN_0044659c`, `FUN_00446830`). They share
//! their code and differ in the cell positions, the pictures and where the
//! frame boundary `FUN_0043318c` sits in the loop. Tile 7 is missing: with
//! object 322 in hand its cell empties, and the board is solved when cells
//! 0..7 read 1 2 3 4 5 6 _ 8.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`taquin_game.cpp`,
//! `puzzle_taquin.cpp`; GPLv3, the ScummVM team).

use super::{load_spr, load_tga, PuzzleCursor, PuzzleHost, PuzzleSounds, SamplePlayer};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;

/// The start board of all three (0x00496f14, 0x00496f26, 0x00497188).
pub const START: [i16; 9] = [8, 1, 4, 6, 2, 3, 7, 9, 5];
/// The target 0x00496f4a (cell 8 is not compared).
pub const TARGET: [i16; 9] = [1, 2, 3, 4, 5, 6, 0, 8, 9];
/// The cell positions (0x00496f80/92, 0x00496fa4/b6, 0x0049719a/ac).
const X6: [i16; 9] = [241, 301, 360, 241, 301, 360, 241, 301, 360];
const Y6: [i16; 9] = [152, 152, 152, 211, 211, 211, 271, 271, 271];
const X7: [i16; 9] = [220, 296, 372, 220, 296, 372, 220, 296, 372];
const Y7: [i16; 9] = [106, 106, 106, 182, 182, 182, 259, 259, 259];
const X14: [i16; 9] = [247, 302, 357, 247, 302, 357, 247, 302, 357];
const Y14: [i16; 9] = [118, 119, 119, 173, 173, 173, 228, 229, 229];

/// The missing tile.
pub const PIECE_OBJECT: i32 = 0x142;

/// The game's I/O: the C++ `TaquinGame` virtual functions.
#[allow(async_fn_in_trait)]
pub trait TaquinIo {
    /// The input when the puzzle starts: the mouse and a new click.
    fn start_input(&mut self) -> (i32, i32, bool);
    /// `FUN_0043318c`: a frame boundary, where the input is read.
    fn boundary(&mut self, g: &TaquinGame) -> (i32, i32, bool);
    fn draw_background(&mut self);
    /// 'P' PUZTAQn, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    /// `FUN_0042d558`.
    fn draw_standard_cursor(&mut self, x: i32, y: i32);
    async fn present(&mut self, g: &TaquinGame);
    /// tak1..tak4 = 0..3.
    fn play_sound(&mut self, slot: i32);
    fn aborted(&self) -> bool {
        false
    }
}

/// A sliding puzzle's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaquinGame {
    /// 0x00496f38.
    pub board: [i16; 9],
    pub cell_x: [i16; 9],
    pub cell_y: [i16; 9],
    /// 0x00496fc8, 99 when none.
    pub moving_cell: i16,
    /// 0x00496fca, 0x00496fcc.
    pub move_dx: i16,
    pub move_dy: i16,
    pub cursor: PuzzleCursor,
    /// OBJETENMAIN.
    pub object_in_hand: i32,
    /// 0x004974c0: 1 solved, 2 left.
    pub result: i32,
    /// 0x007bead0: object 322 was in hand.
    pub with_piece: bool,
    variant: i32,
    mx: i32,
    my: i32,
    click: bool,
}

fn outside(mx: i32, my: i32) -> bool {
    !(0x50..=0x230).contains(&mx) || !(0x32..=0x1ae).contains(&my)
}

impl TaquinGame {
    /// Variant 6, 7 or 14 (the StartPuzzle number).
    pub fn new(variant: i32) -> TaquinGame {
        let (xs, ys) = match variant {
            7 => (X7, Y7),
            14 => (X14, Y14),
            _ => (X6, Y6),
        };
        TaquinGame {
            board: START,
            cell_x: xs,
            cell_y: ys,
            moving_cell: 99,
            move_dx: 0,
            move_dy: 0,
            cursor: PuzzleCursor::default(),
            object_in_hand: 0,
            result: 0,
            with_piece: false,
            variant,
            mx: 0,
            my: 0,
            click: false,
        }
    }

    fn read_boundary(&mut self, io: &mut impl TaquinIo) {
        let (mx, my, c) = io.boundary(self);
        self.mx = mx;
        self.my = my;
        if c {
            self.click = true; // FUN_0041a298 only ever sets the flag
        }
    }

    /// `FUN_00446cf8`.
    fn draw(&mut self, io: &mut impl TaquinIo, extra: i16) {
        io.draw_background();
        for i in 0..9 {
            let v = self.board[i];
            if v > 0 && self.moving_cell != i as i16 {
                io.draw_sprite(
                    'P',
                    self.cell_x[i].into(),
                    self.cell_y[i].into(),
                    i32::from(v - 1),
                );
            }
            if v > 0 && self.moving_cell == i as i16 {
                // The moved tile is drawn once, 30 pixels back toward its
                // cell.
                io.draw_sprite(
                    'P',
                    i32::from(self.cell_x[i] + self.move_dx),
                    i32::from(self.cell_y[i] + self.move_dy),
                    i32::from(v - 1),
                );
                self.moving_cell = 99;
            }
        }
        if extra > 0 {
            io.draw_sprite(
                'P',
                self.cell_x[6].into(),
                self.cell_y[6].into(),
                extra.into(),
            );
        }
        // The win test is made here, on every drawing.
        if self.result == 0 && self.board[..8] == TARGET[..8] {
            self.result = 1;
        }
    }

    /// `FUN_00446ac8`.
    async fn frame(&mut self, io: &mut impl TaquinIo, extra: i16) {
        self.draw(io, extra);
        io.draw_background();
        io.present(self).await;
        self.click = false;
        self.read_boundary(io);
    }

    /// `FUN_0044727c`.
    fn error(&mut self, io: &mut impl TaquinIo) {
        io.play_sound(3);
        self.cursor.refuse();
    }

    /// `FUN_00447048`.
    fn handle_click(&mut self, io: &mut impl TaquinIo) {
        let col = ((self.mx - i32::from(self.cell_x[0])) as u32)
            / ((i32::from(self.cell_x[1]) - i32::from(self.cell_x[0])) as u32);
        let row = ((self.my - i32::from(self.cell_y[0])) as u32)
            / ((i32::from(self.cell_y[3]) - i32::from(self.cell_y[0])) as u32);
        if col >= 3 || row >= 3 {
            if outside(self.mx, self.my) {
                self.result = 2;
            } else {
                self.error(io);
            }
            return;
        }
        let e = self.board.iter().rposition(|&v| v == 0).unwrap_or(99);
        let c = (row * 3 + col) as usize;
        if e >= 99 {
            self.error(io);
            return;
        }
        let d = (c as i32 - e as i32).abs();
        if !(d == 3 || (d == 1 && e / 3 == c / 3)) {
            self.error(io);
            return;
        }
        io.play_sound(2);
        self.board[e] = self.board[c];
        self.board[c] = 0;
        self.moving_cell = e as i16;
        self.move_dy = 0;
        self.move_dx = 0;
        match c as i32 - e as i32 + 3 {
            0 => self.move_dy = -30,
            2 => self.move_dx = -30,
            4 => self.move_dx = 30,
            6 => self.move_dy = 30,
            _ => {}
        }
    }

    /// `FUN_00446f28`, `FUN_00445d70`.
    fn draw_cursor(&mut self, io: &mut impl TaquinIo) {
        if self.cursor.mode == 2 {
            // FUN_00445d70: frames 2..6 and back to -1, then the mode ends.
            io.draw_sprite('C', self.mx, self.my, self.cursor.frame.into());
            self.cursor.frame += self.cursor.step;
            if self.cursor.frame == 6 {
                self.cursor.step = -1;
            }
            if self.cursor.frame <= -1 {
                self.cursor.mode = 0;
            }
            return;
        }
        if outside(self.mx, self.my) {
            io.draw_sprite('C', self.mx, self.my, 11);
        } else {
            io.draw_standard_cursor(self.mx, self.my);
        }
    }

    /// `FUN_00446c54`.
    async fn win_animation(&mut self, io: &mut impl TaquinIo) {
        self.moving_cell = 99;
        io.play_sound(2);
        if !self.with_piece {
            return;
        }
        for _ in 0..15 {
            self.frame(io, 0).await;
        }
        for i in (10..=14).rev().step_by(2) {
            self.frame(io, i).await;
        }
        for _ in 0..20 {
            self.frame(io, 6).await;
        }
    }

    /// The entry: 255 when solved, 2 when left. Its picture is the "b"
    /// one when [`TaquinGame::with_piece`] is set.
    pub async fn run(&mut self, io: &mut impl TaquinIo) -> i32 {
        (self.mx, self.my, self.click) = io.start_input();
        self.with_piece = self.object_in_hand == PIECE_OBJECT;
        io.play_sound(0);
        // FUN_00446bbc.
        self.object_in_hand = 0;
        self.moving_cell = 99;
        if self.with_piece {
            for _ in 0..15 {
                self.frame(io, 0).await;
            }
            self.board[6] = 0;
            for i in (9..15).step_by(3) {
                self.frame(io, i).await;
            }
        }
        self.result = 0;
        loop {
            if self.variant == 7 {
                self.read_boundary(io);
            }
            if self.click {
                self.handle_click(io);
            }
            self.draw(io, 0);
            self.draw_cursor(io);
            io.draw_background();
            io.present(self).await;
            self.click = false;
            if self.variant != 7 {
                self.read_boundary(io);
            }
            if self.result != 0 || io.aborted() {
                break;
            }
        }
        if self.result == 1 {
            self.win_animation(io).await;
            return 255;
        }
        2
    }
}

struct Host<'a>(PuzzleHost<'a>);

impl TaquinIo for Host<'_> {
    fn start_input(&mut self) -> (i32, i32, bool) {
        let m = self.0.read_input();
        (m.x, m.y, m.click)
    }
    fn boundary(&mut self, _g: &TaquinGame) -> (i32, i32, bool) {
        self.start_input()
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
    async fn present(&mut self, _g: &TaquinGame) {
        self.0.present().await;
    }
    fn play_sound(&mut self, slot: i32) {
        const NAMES: [&str; 4] = ["TAK1", "TAK2", "TAK3", "TAK4"];
        if let Some(n) = usize::try_from(slot).ok().and_then(|s| NAMES.get(s)) {
            self.0.play_sound(n, SamplePlayer::Ca28);
        }
    }
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

/// StartPuzzle 6, 7 or 14: 255 when solved, 2 when the player leaves, 1
/// when a file is missing or the game is quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks, variant: i32) -> i32 {
    let (spr, pics) = match variant {
        7 => ("PUZTAQ2.SPR", ["PUZTAQ2.TGA", "PUZTAQ2B.TGA"]),
        14 => ("PUZTAQ3.SPR", ["PUZTAQ3.TGA", "PUZTAQ3B.TGA"]),
        _ => ("PUZTAQ1.SPR", ["PUZTAQ1.TGA", "PUZTAQ1B.TGA"]),
    };
    let Some(frames) = load_spr(e, spr).await else {
        return 1;
    };
    // The entry reads OBJETENMAIN and picks the picture: the "b" one with
    // the missing tile in hand.
    let held = e.vars.get("objetenmain");
    let Some(bg) = load_tga(e, pics[usize::from(held == PIECE_OBJECT)]).await else {
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &["TAK1", "TAK2", "TAK3", "TAK4"]).await;
    let mut game = TaquinGame::new(variant);
    game.object_in_hand = held;
    game.cursor = e.puzzles.cursor;
    let mut host = Host(PuzzleHost::new(e, hooks));
    host.0.screen.pictures = vec![bg];
    host.0.screen.frames = frames;
    host.0.sounds = sounds;
    let r = game.run(&mut host).await;
    let e = host.0.e;
    e.puzzles.cursor = game.cursor;
    // FUN_00446bbc emptied the hand.
    e.vars.set("objetenmain", 0);
    if e.should_abort() {
        1
    } else {
        r
    }
}

//! StartPuzzle 3 at CHAPI029: the six rotating pieces (`FUN_004459c0`).
//! Six pieces in a ring, each swapped once with a neighbour, back to
//! their places. The click map is PUZZLES\PUZSRP2.BMP read raw
//! (`FUN_004472cc`).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`pieces_rules.h`,
//! `puzzle_pieces.cpp`; GPLv3, the ScummVM team).

use atlantis_data::sprite::SprSprite;

use super::SamplePlayer;
use super::{load_spr, load_tga, read_puzzle_file, PuzzleCursor, PuzzleHost, PuzzleSounds};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;
use crate::warp::{SCREEN_H, SCREEN_W};

pub const NUM_PIECES: usize = 6;

/// The start order (0x00496eae).
pub const START: [i16; NUM_PIECES] = [3, 1, 0, 4, 5, 2];

/// The base of the click map's reads at [x - y * 640 + base].
pub const CLICK_BASE: i32 = 0x4b438;
/// The base of the cursor's reads.
pub const CURSOR_BASE: i32 = 0x4b1b8;

/// What a click did (`FUN_004460c8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    None,
    /// Off the board: the puzzle is left (2).
    Leave,
    /// The reset button (serp_2_2).
    Reset,
    /// A piece swapped (serp_2_1).
    Move,
    /// The refusal: the board off the pieces, or a piece that has moved
    /// (serp_2_3).
    Board,
}

/// The cursor drawn (`FUN_00445e7c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    /// The animation, a CURSEURS frame.
    Anim(i32),
    /// CURSEURS frame 11.
    Exit,
    /// CURSEURS frame 6 (`FUN_0042d558`).
    Normal,
    /// A PUZSRP2C frame (13 over a piece that has moved).
    Piece(i32),
}

/// The game's I/O: the C++ `RotatingPiecesPuzzle` drawing and sounds.
#[allow(async_fn_in_trait)]
pub trait PiecesIo {
    /// The mouse and a new left click.
    fn read_input(&mut self) -> (i32, i32, bool);
    fn draw_background(&mut self);
    /// 'P' PUZSRP2, 'S' PUZSRP2C, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    async fn present(&mut self, g: &PiecesGame);
    /// serp_2_1..serp_2_3 = 0..2.
    fn play_sound(&mut self, n: i32);
    fn aborted(&self) -> bool {
        false
    }
}

/// The rotating pieces' state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiecesGame {
    /// 0x00496eba.
    pub pieces: [i16; NUM_PIECES],
    /// 0x00496ec6: the moves of each piece.
    pub counters: [i16; NUM_PIECES],
    /// The shared cursor animation.
    pub cursor: PuzzleCursor,
    /// PUZSRP2.BMP, the whole file.
    pub map: Vec<u8>,
    /// 0x004974c0: 2 left, 255 solved.
    pub result: i32,
}

/// A byte of the raw BMP at [x - y * 640 + base], 0 off the file.
pub fn map_value(map: &[u8], x: i32, y: i32, base: i32) -> u8 {
    let i = x - y * 640 + base;
    usize::try_from(i)
        .ok()
        .and_then(|i| map.get(i).copied())
        .unwrap_or(0)
}

fn on_board(x: i32, y: i32) -> bool {
    (0x32..=0x24e).contains(&x) && (0x1e..=0x1c2).contains(&y)
}

impl PiecesGame {
    pub fn new(map: Vec<u8>) -> PiecesGame {
        PiecesGame {
            pieces: START,
            counters: [0; NUM_PIECES],
            cursor: PuzzleCursor::default(),
            map,
            result: 0,
        }
    }

    /// `FUN_00445ba0`: the start order, the counters cleared.
    pub fn reset(&mut self) {
        self.pieces = START;
        self.counters = [0; NUM_PIECES];
    }

    /// The win test of `FUN_00445c00`.
    pub fn solved(&self) -> bool {
        self.pieces.iter().enumerate().all(|(i, &p)| p == i as i16)
    }

    /// `FUN_004462b0`: the refusal (the cursor animation and serp_2_3)
    /// unless the animation is at its first frame.
    fn refuse(&mut self) -> Click {
        if self.cursor.refuse() {
            Click::Board
        } else {
            Click::None
        }
    }

    /// `FUN_004460c8`. Values above 13 index past the original's arrays;
    /// the shipped map has none.
    pub fn click(&mut self, x: i32, y: i32) -> Click {
        let v = map_value(&self.map, x, y, CLICK_BASE);
        if v == 0 {
            if !on_board(x, y) {
                return Click::Leave;
            }
            return self.refuse();
        }
        if v == 13 {
            self.reset();
            return Click::Reset;
        }
        let code = usize::from(v - 1);
        if code >= 2 * NUM_PIECES {
            return Click::None;
        }
        let i = code >> 1;
        // A piece that has already moved (0x0044619c -> 0x00446239).
        if self.counters[self.pieces[i] as usize] != 0 {
            return self.refuse();
        }
        self.counters[self.pieces[i] as usize] += 1;
        let j = if code % 2 == 0 {
            (i + 5) % NUM_PIECES
        } else {
            (i + 1) % NUM_PIECES
        };
        self.pieces.swap(i, j);
        Click::Move
    }

    /// `FUN_00445e7c`: the cursor at (x, y). The animation
    /// (`FUN_00445d70`) draws CURSEURS frame `cursor.frame`, then steps
    /// 2 -> 6 -> 0 and ends.
    pub fn cursor_at(&mut self, x: i32, y: i32) -> Cursor {
        let c = &mut self.cursor;
        if c.mode == 2 {
            let f = c.frame;
            c.frame += c.step;
            if c.frame == 6 {
                c.step = -1;
            }
            if c.frame < 0 {
                c.mode = 0;
            }
            return Cursor::Anim(f.into());
        }
        let v = map_value(&self.map, x, y, CURSOR_BASE);
        if v == 0 {
            return if on_board(x, y) {
                Cursor::Normal
            } else {
                Cursor::Exit
            };
        }
        let mut frame = i32::from(v) - 1;
        if frame < 2 * NUM_PIECES as i32
            && self.counters[self.pieces[(frame >> 1) as usize] as usize] != 0
        {
            frame = 13;
        }
        Cursor::Piece(frame)
    }

    /// `FUN_00445c00` and `FUN_00445e7c`: the picture, every piece with
    /// its hotspot at (320, 200) (frame i + place * 6), the cursor.
    pub fn render(&mut self, io: &mut impl PiecesIo, x: i32, y: i32) {
        io.draw_background();
        for i in 0..NUM_PIECES {
            io.draw_sprite('P', 320, 200, i as i32 + i32::from(self.pieces[i]) * 6);
        }
        match self.cursor_at(x, y) {
            Cursor::Anim(f) => io.draw_sprite('C', x, y, f),
            Cursor::Exit => io.draw_sprite('C', x, y, 11),
            Cursor::Normal => io.draw_sprite('C', x, y, 6),
            Cursor::Piece(f) => io.draw_sprite('S', x, y, f),
        }
    }

    /// `FUN_004459c0`'s loop: a click (`FUN_004460c8`), the frame, the
    /// win test, until a result: 255 solved, 2 left.
    pub async fn run(&mut self, io: &mut impl PiecesIo) -> i32 {
        self.result = 0;
        loop {
            let (x, y, clicked) = io.read_input();
            if clicked {
                match self.click(x, y) {
                    Click::Leave => self.result = 2,
                    Click::Move => io.play_sound(0),
                    Click::Reset => io.play_sound(1),
                    Click::Board => io.play_sound(2),
                    Click::None => {}
                }
            }
            self.render(io, x, y);
            if self.result == 0 && self.solved() {
                self.result = 255;
            }
            io.present(self).await;
            if self.result != 0 {
                return self.result;
            }
            if io.aborted() {
                return 2;
            }
        }
    }
}

struct Host<'a> {
    h: PuzzleHost<'a>,
    /// PUZSRP2C.SPR.
    cursors: Vec<SprSprite>,
}

impl PiecesIo for Host<'_> {
    fn read_input(&mut self) -> (i32, i32, bool) {
        let m = self.h.read_input();
        (m.x, m.y, m.click)
    }
    fn draw_background(&mut self) {
        self.h.draw_background();
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        if bank != 'S' {
            return self.h.draw_sprite(bank, x, y, frame);
        }
        if let Some(spr) = usize::try_from(frame)
            .ok()
            .and_then(|f| self.cursors.get(f))
        {
            spr.draw(&mut self.h.screen.working, SCREEN_W, SCREEN_H, x, y);
        }
    }
    async fn present(&mut self, _g: &PiecesGame) {
        self.h.present().await;
    }
    fn play_sound(&mut self, n: i32) {
        const NAMES: [&str; 3] = ["SERP_2_1", "SERP_2_2", "SERP_2_3"];
        if let Some(name) = usize::try_from(n).ok().and_then(|n| NAMES.get(n)) {
            self.h.play_sound(name, SamplePlayer::Ca28);
        }
    }
    fn aborted(&self) -> bool {
        self.h.aborted()
    }
}

/// StartPuzzle 3: 255 when solved, 2 when the player leaves, 1 when a
/// file is missing or the game is quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let (Some(bg), Some(frames), Some(cursors), Some(map)) = (
        load_tga(e, "PUZSRP2.TGA").await,
        load_spr(e, "PUZSRP2.SPR").await,
        load_spr(e, "PUZSRP2C.SPR").await,
        read_puzzle_file(e, "PUZSRP2.BMP").await,
    ) else {
        log::warn!("rotating pieces: missing files");
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &["SERP_2_1", "SERP_2_2", "SERP_2_3"]).await;
    let mut game = PiecesGame::new(map);
    game.cursor = e.puzzles.cursor;
    let mut host = Host {
        h: PuzzleHost::new(e, hooks),
        cursors,
    };
    host.h.screen.pictures = vec![bg];
    host.h.screen.frames = frames;
    host.h.sounds = sounds;
    let r = game.run(&mut host).await;
    let e = host.h.e;
    e.puzzles.cursor = game.cursor;
    if e.should_abort() {
        1
    } else {
        r
    }
}

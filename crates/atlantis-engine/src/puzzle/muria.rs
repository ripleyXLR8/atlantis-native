//! StartPuzzle 16 at CHAPI069 and 070: Muria's tablets (`FUN_0043ee54`).
//! Twelve stone pieces of three kinds lie below two broken tablets with
//! twelve slots of the same kinds; a piece goes only on a slot of its
//! kind, and the puzzle is won when piece i sits on slot i for all twelve.
//! A kind-1 piece placed on its own slot sets FLAGTEMP3 to 1021 + slot and
//! runs the chapter's /sel scan.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`muria_game.cpp`,
//! `puzzle_muria.cpp`; GPLv3, the ScummVM team).

use super::{load_spr, load_tga, PuzzleCursor, PuzzleHost, PuzzleSounds, SamplePlayer};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;

/// A piece record (0x00497290, 8 bytes): kind, home x, y, slot (-1 free,
/// -2 hidden during the win animation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: i16,
    pub x: i16,
    pub y: i16,
    pub slot: i16,
}

/// A slot record (0x004972f0, 12 bytes): kind, draw x, y, click x, y,
/// the piece on it (-1 empty).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub kind: i16,
    pub x: i16,
    pub y: i16,
    pub cx: i16,
    pub cy: i16,
    pub piece: i16,
}

pub const NUM_PIECES: usize = 12;
pub const NUM_SLOTS: usize = 12;

const fn p(kind: i16, x: i16, y: i16) -> Piece {
    Piece {
        kind,
        x,
        y,
        slot: -1,
    }
}

const fn s(kind: i16, x: i16, y: i16, cx: i16, cy: i16) -> Slot {
    Slot {
        kind,
        x,
        y,
        cx,
        cy,
        piece: -1,
    }
}

pub const PIECES: [Piece; NUM_PIECES] = [
    p(1, 570, 403),
    p(1, 212, 407),
    p(1, 444, 436),
    p(1, 48, 369),
    p(2, 112, 437),
    p(2, 327, 390),
    p(2, 363, 439),
    p(2, 181, 368),
    p(3, 280, 439),
    p(3, 479, 382),
    p(3, 124, 368),
    p(3, 523, 454),
];

pub const SLOTS: [Slot; NUM_SLOTS] = [
    s(1, 130, 69, 186, 184),
    s(1, 233, 68, 267, 184),
    s(1, 361, 68, 379, 184),
    s(1, 438, 69, 457, 184),
    s(2, 148, 204, 175, 227),
    s(2, 237, 204, 262, 227),
    s(2, 356, 205, 380, 227),
    s(2, 444, 203, 468, 227),
    s(3, 140, 250, 164, 270),
    s(3, 236, 250, 261, 270),
    s(3, 360, 249, 385, 270),
    s(3, 455, 247, 478, 269),
];

/// The game's I/O: the C++ `MuriaGame` virtual functions.
#[allow(async_fn_in_trait)]
pub trait MuriaIo {
    /// The input when the puzzle starts: the mouse and a new click.
    fn start_input(&mut self) -> (i32, i32, bool);
    /// `FUN_0043318c`: a frame boundary.
    fn boundary(&mut self, g: &MuriaGame) -> (i32, i32, bool);
    /// `FUN_00447478`.
    fn load_picture(&mut self, name: &str);
    /// `FUN_00410f36`.
    fn draw_background(&mut self);
    /// 'P' puzmuria.spr, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    /// `FUN_0041afd0`.
    async fn present(&mut self, g: &MuriaGame);
    /// muria1..5 = 0..4.
    fn play_sound(&mut self, slot: i32);
    /// `FUN_004277c8` after FLAGTEMP3 is set to `flag_temp3`.
    async fn script_hook(&mut self, flag_temp3: i32);
    /// `FUN_0043d0e0`.
    fn stop_sounds(&mut self) {}
    fn aborted(&self) -> bool {
        false
    }
}

/// Muria's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MuriaGame {
    pub pieces: [Piece; NUM_PIECES],
    pub slots: [Slot; NUM_SLOTS],
    /// 0x007bebf8: the piece carried + 1.
    pub hand: i32,
    pub cursor: PuzzleCursor,
    /// 0x004974c0.
    pub result: i32,
    /// 0x0049611c.
    pub click: bool,
    /// The CON variable FLAGTEMP3.
    pub flag_temp3: i32,
    mx: i32,
    my: i32,
}

impl Default for MuriaGame {
    fn default() -> Self {
        MuriaGame {
            pieces: PIECES,
            slots: SLOTS,
            hand: 0,
            cursor: PuzzleCursor::default(),
            result: 0,
            click: false,
            flag_temp3: 0,
            mx: 0,
            my: 0,
        }
    }
}

impl MuriaGame {
    fn read_boundary(&mut self, io: &mut impl MuriaIo) {
        let (mx, my, c) = io.boundary(self);
        self.mx = mx;
        self.my = my;
        self.click = c;
    }

    /// `FUN_0043f08c`.
    fn set_up(&mut self, io: &mut impl MuriaIo) {
        for p in self.pieces.iter_mut() {
            p.slot = -1;
        }
        for s in self.slots.iter_mut() {
            s.piece = -1;
        }
        io.load_picture("puzmuria.tga");
        self.hand = 0;
    }

    /// `FUN_0043f750`: the slot under the mouse, -1 for none. The nearest
    /// click point within 40; the top row is aimed 50 pixels higher, with
    /// its vertical distance halved.
    pub fn slot_at(&self, mx: i32, my: i32) -> i32 {
        let mut best = 999;
        let mut found = -1;
        for (i, s) in self.slots.iter().enumerate() {
            let d = if i < 4 {
                (my + 0x32 - i32::from(s.cy)).abs() / 2 + (mx - i32::from(s.cx)).abs()
            } else {
                (mx - i32::from(s.cx)).abs() + (my - i32::from(s.cy)).abs()
            };
            if d < best {
                best = d;
                found = i as i32;
            }
        }
        if best < 0x28 {
            found
        } else {
            -1
        }
    }

    /// `FUN_0043f6cc`.
    pub fn won(&self) -> bool {
        self.pieces
            .iter()
            .enumerate()
            .all(|(i, p)| i32::from(p.slot) == i as i32)
    }

    /// `FUN_0043faa0`.
    fn refuse(&mut self, io: &mut impl MuriaIo) {
        if self.cursor.refuse() {
            io.play_sound(2);
        }
    }

    /// `FUN_0043faf0`: two passes drawn, then FLAGTEMP3 = n and the
    /// chapter's /sel scan.
    async fn placed(&mut self, io: &mut impl MuriaIo, n: i32) {
        for _ in 0..2 {
            self.draw(io);
            self.draw_cursor(io);
            io.draw_background();
            io.present(self).await;
        }
        self.flag_temp3 = n;
        io.script_hook(n).await;
    }

    /// `FUN_0043f850`.
    async fn handle_click(&mut self, io: &mut impl MuriaIo) {
        if self.hand != 0 {
            let p = (self.hand - 1) as usize;
            if self.my > 0x15e {
                // Below the tablets: the piece goes back home.
                self.pieces[p].slot = -1;
                self.hand = 0;
                io.play_sound(4);
                return;
            }
            let s = self.slot_at(self.mx, self.my);
            if s >= 0
                && self.slots[s as usize].piece == -1
                && self.slots[s as usize].kind == self.pieces[p].kind
            {
                self.pieces[p].slot = s as i16;
                self.slots[s as usize].piece = p as i16;
                self.hand = 0;
                io.play_sound(0);
                if s < 4 && s == p as i32 {
                    self.placed(io, s + 0x3fd).await;
                }
            } else {
                self.refuse(io);
            }
            return;
        }
        let s = self.slot_at(self.mx, self.my);
        if s >= 0 {
            let slot = &mut self.slots[s as usize];
            if slot.piece != -1 {
                // The piece on the slot is taken back.
                self.hand = i32::from(slot.piece);
                slot.piece = -1;
                self.pieces[self.hand as usize].slot = -1;
                self.hand += 1;
                io.play_sound(1);
            } else {
                self.refuse(io);
            }
        } else {
            // The nearest free piece within 35 of its home.
            let mut best = 999;
            let mut found = -1;
            for (i, p) in self.pieces.iter().enumerate() {
                if p.slot != -1 {
                    continue;
                }
                let d = (self.mx - i32::from(p.x)).abs() + (self.my - i32::from(p.y)).abs();
                if d < best {
                    best = d;
                    found = i as i32;
                }
            }
            if best < 0x23 {
                self.hand = found + 1;
                io.play_sound(1);
            }
        }
        if self.my < 40 {
            self.result = 2;
        } else if self.hand == 0 {
            self.refuse(io);
        }
    }

    /// `FUN_0043f328`.
    fn draw(&self, io: &mut impl MuriaIo) {
        io.draw_background();
        for (i, p) in self.pieces.iter().enumerate() {
            let s = p.slot;
            if self.hand - 1 == i as i32 || s == -2 {
                continue;
            }
            if s == -1 {
                io.draw_sprite('P', p.x.into(), p.y.into(), i as i32 + 12);
            } else if let Some(slot) = usize::try_from(s).ok().and_then(|s| self.slots.get(s)) {
                io.draw_sprite(
                    'P',
                    slot.x.into(),
                    slot.y.into(),
                    i32::from(s) % 4 + 4 * i as i32 + 0x18,
                );
            }
        }
    }

    /// `FUN_0043f4d4`.
    fn draw_cursor(&mut self, io: &mut impl MuriaIo) {
        if self.hand != 0 {
            io.draw_sprite('P', self.mx, self.my, i32::from((self.hand as i16) - 1));
        }
        if self.cursor.bouncing() {
            // FUN_00448958, the shared cursor animation.
            let f = self.cursor.bounce();
            io.draw_sprite('C', self.mx, self.my, f.into());
            return;
        }
        io.draw_sprite('C', self.mx, self.my, if self.my < 40 { 10 } else { 5 });
    }

    async fn pass(&mut self, io: &mut impl MuriaIo) {
        self.draw(io);
        self.draw_cursor(io);
        io.draw_background();
        io.present(self).await;
    }

    /// `FUN_0043f110`: sound 3, then pieces 4..11 blink in turn, eight
    /// times, a pass per half-step (no cursor).
    async fn win_animation(&mut self, io: &mut impl MuriaIo) {
        io.play_sound(3);
        for _ in 0..8 {
            for i in 4..NUM_PIECES {
                for slot in [-2, i as i16] {
                    self.pieces[i].slot = slot;
                    self.draw(io);
                    io.draw_background();
                    io.present(self).await;
                    self.click = false;
                    self.read_boundary(io);
                }
            }
        }
    }

    /// `FUN_0043ee54`: 255 won, 2 left.
    pub async fn run(&mut self, io: &mut impl MuriaIo) -> i32 {
        (self.mx, self.my, self.click) = io.start_input();
        self.set_up(io);
        self.result = 0;
        loop {
            if self.click {
                self.handle_click(io).await;
            }
            self.pass(io).await;
            self.click = false;
            if self.won() {
                self.result = 1;
            }
            self.read_boundary(io);
            if self.result != 0 || io.aborted() {
                break;
            }
        }
        self.flag_temp3 = 0;
        if self.result == 1 {
            self.win_animation(io).await;
            io.stop_sounds();
            return 255;
        }
        io.stop_sounds();
        2
    }
}

struct Host<'a>(PuzzleHost<'a>);

impl MuriaIo for Host<'_> {
    fn start_input(&mut self) -> (i32, i32, bool) {
        let m = self.0.read_input();
        (m.x, m.y, m.click)
    }
    fn boundary(&mut self, _g: &MuriaGame) -> (i32, i32, bool) {
        self.start_input()
    }
    fn load_picture(&mut self, _name: &str) {
        // The one picture, loaded with the puzzle.
    }
    fn draw_background(&mut self) {
        self.0.draw_background();
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.0.draw_sprite(bank, x, y, frame);
    }
    async fn present(&mut self, _g: &MuriaGame) {
        self.0.present().await;
    }
    fn play_sound(&mut self, slot: i32) {
        const NAMES: [&str; 5] = ["MURIA1", "MURIA2", "MURIA3", "MURIA4", "MURIA5"];
        if let Some(n) = usize::try_from(slot).ok().and_then(|s| NAMES.get(s)) {
            self.0.play_sound(n, SamplePlayer::Ca28);
        }
    }
    /// `FUN_0043faf0`: FLAGTEMP3 = 1021 + slot, then `FUN_004277c8` runs
    /// the first /sel entry that holds (CHAPI069 and 070 react to
    /// 1021..1024).
    async fn script_hook(&mut self, flag_temp3: i32) {
        let h = &mut self.0;
        h.e.vars.set("flagtemp3", flag_temp3);
        h.hooks.sel_scan(h.e).await;
    }
    fn stop_sounds(&mut self) {
        self.0.stop_samples();
    }
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

/// StartPuzzle 16: 255 when solved, 2 when the player leaves, 1 when a
/// file is missing or the game is quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let (Some(frames), Some(bg)) = (
        load_spr(e, "PUZMURIA.SPR").await,
        load_tga(e, "PUZMURIA.TGA").await,
    ) else {
        log::warn!("muria: missing files");
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &["MURIA1", "MURIA2", "MURIA3", "MURIA4", "MURIA5"]).await;
    let mut game = MuriaGame {
        cursor: e.puzzles.cursor,
        ..MuriaGame::default()
    };
    let mut host = Host(PuzzleHost::new(e, hooks));
    host.0.screen.pictures = vec![bg];
    host.0.screen.frames = frames;
    host.0.sounds = sounds;
    let r = game.run(&mut host).await;
    let e = host.0.e;
    e.puzzles.cursor = game.cursor;
    // FUN_0043ee54 zeroes FLAGTEMP3 after the loop.
    e.vars.set("flagtemp3", 0);
    if e.should_abort() {
        1
    } else {
        r
    }
}

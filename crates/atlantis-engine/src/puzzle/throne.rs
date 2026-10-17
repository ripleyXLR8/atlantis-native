//! StartPuzzle 5 at CHAPI044: the throne lock (`FUN_0043fcb4`). Four
//! rings of 8 positions turned by four buttons, and a lever that opens the
//! lock when all rings are at 0.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`throne_game.cpp`,
//! `puzzle_throne.cpp`; GPLv3, the ScummVM team).

use super::{load_spr, load_tga, PuzzleCursor, PuzzleHost, PuzzleSounds, SamplePlayer};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;

/// One of the 9 parts at 0x00497216 (12 bytes): rings 0..3, buttons
/// 4..7, the lever 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    /// The first PUZTRONE frame.
    pub base: i16,
    /// Frames (ring positions).
    pub count: i16,
    /// The position at the start.
    pub init: i16,
    /// The current position.
    pub cur: i16,
    pub x: i16,
    pub y: i16,
}

const fn part(base: i16, count: i16, init: i16, x: i16, y: i16) -> Part {
    Part {
        base,
        count,
        init,
        cur: 0,
        x,
        y,
    }
}

/// The parts as in the original's data (0x00497216).
pub const PARTS: [Part; 9] = [
    part(0, 8, 4, 390, 148),
    part(8, 8, 1, 389, 283),
    part(16, 8, 5, 254, 149),
    part(24, 8, 2, 254, 283),
    part(32, 2, 0, 447, 4),
    part(34, 2, 0, 447, 405),
    part(36, 2, 0, 112, 4),
    part(38, 2, 0, 112, 405),
    part(40, 4, 0, 321, 435),
];

/// The game's I/O: the C++ `ThroneGame` virtual functions.
#[allow(async_fn_in_trait)]
pub trait ThroneIo {
    /// The mouse and a new left click.
    fn read_input(&mut self) -> (i32, i32, bool);
    fn draw_background(&mut self);
    /// 'P' PUZTRONE, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    async fn present(&mut self, g: &ThroneGame);
    /// trone1..trone4 = 0..3.
    fn play_sound(&mut self, slot: i32);
    /// Ticks of the 200 Hz timer.
    async fn wait_ticks(&mut self, n: u32);
    fn aborted(&self) -> bool {
        false
    }
}

/// The throne lock's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThroneGame {
    pub parts: [Part; 9],
    pub cursor: PuzzleCursor,
    /// 0x007bebf0.
    pub counter: i32,
    /// 0x007bebf8: the part being animated, 0 when none.
    pub active: i32,
    /// 0x004974c0: 1 opened, 2 left.
    pub result: i32,
}

impl Default for ThroneGame {
    fn default() -> Self {
        ThroneGame {
            parts: PARTS,
            cursor: PuzzleCursor::default(),
            counter: 0,
            active: 0,
            result: 0,
        }
    }
}

fn outside(mx: i32) -> bool {
    !(0x50..0x230).contains(&mx)
}

impl ThroneGame {
    /// `FUN_004401f4`: every ring at 0.
    pub fn solved(&self) -> bool {
        self.parts[..4].iter().all(|p| p.cur == 0)
    }

    /// `FUN_004403b4`: the animation of the part pressed.
    pub fn update(&mut self) {
        let a = self.active;
        if a == 0 {
            return;
        }
        self.counter += 1;
        if a == 8 {
            // The lever falls back after 14 passes, then the rings are
            // checked.
            if self.counter == 0xe {
                self.parts[8].cur = 0;
                self.active = 0;
                if self.solved() {
                    self.result = 1;
                }
            }
            return;
        }
        // A button: its ring turns one step on the 8th pass, the button
        // rises three passes later.
        if self.counter > 7 && self.counter < 0x64 {
            let ring = &mut self.parts[(a - 4) as usize];
            ring.cur += 1;
            if ring.cur >= ring.count {
                ring.cur = 0;
            }
            self.counter = 0x64;
        }
        if self.counter > 0x66 {
            self.parts[a as usize].cur = 0;
            self.active = 0;
        }
    }

    /// `FUN_004404ac`: the refusal; the sound to play.
    fn error_cursor(&mut self, sound: &mut i32) {
        if self.cursor.refuse() {
            *sound = 2;
        }
    }

    /// `FUN_00440270`: a click; returns the sound to play, -1 for none.
    pub fn click(&mut self, mx: i32, my: i32) -> i32 {
        let mut sound = -1;
        if self.active != 0 {
            self.error_cursor(&mut sound);
            return sound;
        }
        for k in 4..8 {
            let dx = (mx - i32::from(self.parts[k].x)) as u32;
            let dy = (my - i32::from(self.parts[k].y)) as u32;
            if dx < 0x52 && dy < 0x22 {
                self.parts[k].cur += 1;
                self.active = k as i32;
                self.counter = 0;
                sound = 0;
            }
        }
        let lever = &self.parts[8];
        let d = (mx - i32::from(lever.x)).abs() + (my - i32::from(lever.y)).abs();
        if d < 0x32 {
            self.parts[8].cur += 1;
            self.active = 8;
            self.counter = 0;
            sound = 1;
        }
        if self.active == 0 {
            if outside(mx) {
                self.result = 2;
            } else {
                self.error_cursor(&mut sound);
            }
        }
        sound
    }

    /// `FUN_004400ec`, `FUN_00448958`.
    fn draw_cursor(&mut self, io: &mut impl ThroneIo, mx: i32, my: i32) {
        if self.cursor.bouncing() {
            let f = self.cursor.bounce();
            io.draw_sprite('C', mx, my, i32::from(f));
            return;
        }
        io.draw_sprite('C', mx, my, if outside(mx) { 10 } else { 5 });
    }

    /// `FUN_0043fcb4` after the loads: 255 when opened, 2 when left.
    pub async fn run(&mut self, io: &mut impl ThroneIo) -> i32 {
        for p in self.parts.iter_mut() {
            p.cur = p.init;
        }
        self.active = 0;
        self.result = 0;
        loop {
            let (mx, my, clicked) = io.read_input();
            self.update();
            if clicked {
                let sound = self.click(mx, my);
                if sound >= 0 {
                    io.play_sound(sound);
                }
            }
            io.draw_background();
            for p in self.parts {
                io.draw_sprite('P', p.x.into(), p.y.into(), i32::from(p.base + p.cur));
            }
            self.draw_cursor(io, mx, my);
            io.draw_background();
            io.present(self).await;
            if self.result != 0 || io.aborted() {
                break;
            }
        }
        if self.result == 1 {
            io.play_sound(3);
            for _ in 0..5 {
                io.wait_ticks(100).await;
            }
            return 255;
        }
        2
    }
}

struct Host<'a>(PuzzleHost<'a>);

impl ThroneIo for Host<'_> {
    fn read_input(&mut self) -> (i32, i32, bool) {
        let m = self.0.read_input();
        (m.x, m.y, m.click)
    }
    fn draw_background(&mut self) {
        self.0.draw_background();
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.0.draw_sprite(bank, x, y, frame);
    }
    async fn present(&mut self, _g: &ThroneGame) {
        self.0.present().await;
    }
    fn play_sound(&mut self, slot: i32) {
        const NAMES: [&str; 4] = ["TRONE1", "TRONE2", "TRONE3", "TRONE4"];
        if let Some(n) = usize::try_from(slot).ok().and_then(|s| NAMES.get(s)) {
            self.0.play_sound(n, SamplePlayer::Ca28);
        }
    }
    async fn wait_ticks(&mut self, n: u32) {
        self.0.wait_ticks(n).await;
    }
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

/// StartPuzzle 5: 255 when opened, 2 when the player leaves, 1 when a
/// file is missing or the game is quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let (Some(bg), Some(frames)) = (
        load_tga(e, "PUZTRONE.TGA").await,
        load_spr(e, "PUZTRONE.SPR").await,
    ) else {
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &["TRONE1", "TRONE2", "TRONE3", "TRONE4"]).await;
    let mut game = ThroneGame {
        cursor: e.puzzles.cursor,
        ..ThroneGame::default()
    };
    let mut host = Host(PuzzleHost::new(e, hooks));
    host.0.screen.pictures = vec![bg];
    host.0.screen.frames = frames;
    host.0.sounds = sounds;
    let r = game.run(&mut host).await;
    let e = host.0.e;
    e.puzzles.cursor = game.cursor;
    if e.should_abort() {
        1
    } else {
        r
    }
}

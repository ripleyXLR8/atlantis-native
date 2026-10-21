//! StartPuzzle 0 at CHAPI012: the eclipse (`FUN_0044154c`). Two levers
//! turn the sun, the moon and the earth; the puzzle is won when the sun
//! and the moon line up.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`eclipse_rules.h`,
//! `puzzle_eclipse.cpp`; GPLv3, the ScummVM team).

use atlantis_data::sprite::SprSprite;

use super::{load_spr, load_tga, PuzzleHost, PuzzleSounds, SamplePlayer};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;
use crate::warp::{SCREEN_H, SCREEN_W};

/// One of the 7 records at 0x0049745e (14 bytes: dir, x, y, min, max,
/// curr, pad).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    /// 0 still, 1 a frame up each pass, 2 a frame down (set for one step
    /// by a lever click).
    pub dir: i16,
    pub x: i16,
    pub y: i16,
    /// The first and last frames of the record's bank.
    pub min: i16,
    pub max: i16,
    /// The frame shown.
    pub curr: i16,
}

const fn record(dir: i16, x: i16, y: i16, min: i16, max: i16) -> Record {
    Record {
        dir,
        x,
        y,
        min,
        max,
        curr: min,
    }
}

/// The records as in the original's data (0x0049745e): 0 and 1 (PZ2L)
/// turn on their own, 2 and 3 (PZ2L) are the levers' mechanisms, 4..6
/// (PZ1L) the sun (lever 2), the moon and the earth (lever 1).
pub const RECORDS: [Record; 7] = [
    record(1, 215, 308, 0, 7),
    record(1, 298, 308, 8, 15),
    record(0, 227, 384, 16, 86),
    record(0, 443, 373, 87, 157),
    record(0, 319, 204, 0, 70),
    record(0, 236, 222, 71, 141),
    record(0, 271, 198, 142, 212),
];

/// The records each lever turns (`FUN_00441cd8`).
const LEVER1: [usize; 3] = [2, 5, 6];
const LEVER2: [usize; 2] = [3, 4];

/// The PZ1L frames of the win highlight (0x00441a8c, 0x00441ab8).
const HIGHLIGHT_SUN: i32 = 0xd5;
const HIGHLIGHT_MOON: i32 = 0xd6;

/// The passes of the win animation `FUN_00441724`.
const WIN_FRAMES: u32 = 0x50;

/// What a click did (`FUN_00441cd8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    None,
    /// A top corner: the puzzle is left.
    Exit,
    /// A lever moved its records one step.
    Lever,
}

/// The game's I/O: the C++ `EclipsePuzzle` drawing and sounds.
#[allow(async_fn_in_trait)]
pub trait EclipseIo {
    /// The mouse and a new left click.
    fn read_input(&mut self) -> (i32, i32, bool);
    fn draw_background(&mut self);
    /// '1' PZ1L, '2' PZ2L, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    async fn present(&mut self, g: &EclipseGame);
    /// LION1..LION3 = 0..2.
    fn play_sound(&mut self, n: i32);
    /// `FUN_0043d0e0`.
    fn stop_sounds(&mut self);
    fn aborted(&self) -> bool {
        false
    }
}

/// The eclipse's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EclipseGame {
    pub records: [Record; 7],
    /// 0x007bebf0: 0 while playing, counted by the win animation; its bit
    /// 1 makes the highlight blink.
    pub frame_counter: u32,
}

impl Default for EclipseGame {
    /// `FUN_0044154c` @ 0x004415df: curr = min + 0x25 for records 2..6;
    /// records 0 and 1 keep their frame (static data, see
    /// [`crate::puzzle::PuzzleState`]).
    fn default() -> Self {
        let mut records = RECORDS;
        for r in &mut records[2..] {
            r.curr = r.min + 0x25;
        }
        EclipseGame {
            records,
            frame_counter: 0,
        }
    }
}

impl EclipseGame {
    /// `FUN_00441f50`: a record one step in its direction, wrapping.
    pub fn tick(&mut self, i: usize) {
        let r = &mut self.records[i];
        match r.dir {
            1 => {
                r.curr += 1;
                if r.curr > r.max {
                    r.curr = r.min;
                }
            }
            2 => {
                r.curr -= 1;
                if r.curr < r.min {
                    r.curr = r.max;
                }
            }
            _ => {}
        }
    }

    /// The win test of `FUN_00441cd8` (0x00441f0b..0x00441f32): the sun
    /// at its second frame, the moon at its first.
    pub fn is_win(&self) -> bool {
        let r = &self.records;
        r[4].curr == r[4].min + 1 && r[5].curr == r[5].min
    }

    /// The two exit zones of `FUN_00441cd8` and of the cursor
    /// `FUN_00441b2c` (0x00441b44..0x00441ba1).
    pub fn over_exit(mx: i32, my: i32) -> bool {
        let row = (0..0xdc).contains(&my);
        row && ((0..0xa0).contains(&mx) || (0x21d..0x280).contains(&mx))
    }

    /// `FUN_00441cd8`: the exits, or a lever whose records step once, up
    /// in its upper half, down in its lower half.
    pub fn click(&mut self, mx: i32, my: i32) -> Click {
        if Self::over_exit(mx, my) {
            return Click::Exit;
        }
        if !(0x173..0x1e0).contains(&my) {
            return Click::None;
        }
        let dir = if my < 0x1a9 + 1 { 1 } else { 2 };
        let recs: &[usize] = if mx > 0x78 && mx < 0xf0 {
            &LEVER1
        } else if mx > 0x1ae && mx < 0x226 {
            &LEVER2
        } else {
            return Click::None;
        };
        for &i in recs {
            self.records[i].dir = dir;
            self.tick(i);
            self.records[i].dir = 0;
        }
        Click::Lever
    }

    fn draw_record(&self, io: &mut impl EclipseIo, bank: char, i: usize, frame: i32) {
        let r = &self.records[i];
        io.draw_sprite(bank, r.x.into(), r.y.into(), frame);
    }

    /// `FUN_00441828`: the picture, records 0..3 stepped and drawn (PZ2L),
    /// records 4..6 stepped, then drawn (PZ1L) earth, sun, moon (moon,
    /// sun, earth while the moon's frame is in 97..130), and the
    /// highlight when the sun and moon line up on the counter's bit 1.
    pub fn render(&mut self, io: &mut impl EclipseIo) {
        io.draw_background();
        for i in 0..4 {
            self.tick(i);
            self.draw_record(io, '2', i, self.records[i].curr.into());
        }
        for i in 4..7 {
            self.tick(i);
        }
        let (back, front) = if self.records[5].curr > 0x60 && self.records[5].curr < 0x83 {
            (5, 6)
        } else {
            (6, 5)
        };
        for i in [back, 4, front] {
            self.draw_record(io, '1', i, self.records[i].curr.into());
        }
        if self.is_win() && self.frame_counter & 2 != 0 {
            self.draw_record(io, '1', 6, self.records[6].curr.into());
            self.draw_record(io, '1', 4, HIGHLIGHT_SUN);
            self.draw_record(io, '1', 5, HIGHLIGHT_MOON);
        }
    }

    /// `FUN_0044154c`'s loop: the click, the exit, a frame with the cursor
    /// (`FUN_00441b2c`: CURSEURS frame 11 over the exits, else frame 6),
    /// then the win test and `FUN_00441724`: LION3 and 80 passes without
    /// a cursor, the highlight blinking. 255 won, 1 left (the original
    /// returns `DAT_00496d0c` there).
    pub async fn run(&mut self, io: &mut impl EclipseIo) -> i32 {
        self.frame_counter = 0;
        loop {
            let (mx, my, clicked) = io.read_input();
            if clicked {
                match self.click(mx, my) {
                    Click::Exit => {
                        io.stop_sounds();
                        return 1;
                    }
                    // Both halves of both levers play LION1.
                    Click::Lever => io.play_sound(0),
                    Click::None => {}
                }
            }
            self.render(io);
            let cursor = if Self::over_exit(mx, my) { 11 } else { 6 };
            io.draw_sprite('C', mx, my, cursor);
            io.present(self).await;
            if io.aborted() {
                return 1;
            }
            if self.is_win() {
                io.play_sound(2);
                self.frame_counter = 0;
                for _ in 0..WIN_FRAMES {
                    self.render(io);
                    io.present(self).await;
                    self.frame_counter += 1;
                    if io.aborted() {
                        break;
                    }
                }
                io.stop_sounds();
                return 255;
            }
        }
    }
}

struct Host<'a> {
    h: PuzzleHost<'a>,
    /// PZ1L.SPR (0x007beae0), PZ2L.SPR (0x007beadc).
    pz1l: Vec<SprSprite>,
    pz2l: Vec<SprSprite>,
}

impl EclipseIo for Host<'_> {
    fn read_input(&mut self) -> (i32, i32, bool) {
        let m = self.h.read_input();
        (m.x, m.y, m.click)
    }
    fn draw_background(&mut self) {
        self.h.draw_background();
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        let bank = match bank {
            '1' => &self.pz1l,
            '2' => &self.pz2l,
            _ => return self.h.draw_sprite(bank, x, y, frame),
        };
        if let Some(spr) = usize::try_from(frame).ok().and_then(|f| bank.get(f)) {
            spr.draw(&mut self.h.screen.working, SCREEN_W, SCREEN_H, x, y);
        }
    }
    async fn present(&mut self, _g: &EclipseGame) {
        self.h.present().await;
    }
    fn play_sound(&mut self, n: i32) {
        const NAMES: [&str; 3] = ["LION1", "LION2", "LION3"];
        if let Some(name) = usize::try_from(n).ok().and_then(|n| NAMES.get(n)) {
            self.h.play_sound(name, SamplePlayer::Ca28);
        }
    }
    fn stop_sounds(&mut self) {
        self.h.stop_samples();
    }
    fn aborted(&self) -> bool {
        self.h.aborted()
    }
}

/// StartPuzzle 0: 255 when solved, 1 when the player leaves, a file is
/// missing or the game is quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let (Some(bg), Some(pz1l), Some(pz2l)) = (
        load_tga(e, "PZLION.TGA").await,
        load_spr(e, "PZ1L.SPR").await,
        load_spr(e, "PZ2L.SPR").await,
    ) else {
        log::warn!("eclipse: missing files");
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &["LION1", "LION2", "LION3"]).await;
    let mut game = EclipseGame::default();
    if let Some([c0, c1]) = e.puzzles.eclipse {
        game.records[0].curr = c0;
        game.records[1].curr = c1;
    }
    // The mouse unlocked. No SetCursorPos before the loop (the import slot
    // 0x00c302f8 has no reader in FUN_004240fc, FUN_0043a1e0, FUN_00448ad0
    // or FUN_0044154c): the puzzle starts with the pointer where the warp
    // loop left it at the zone click, which the warp exit has put in the
    // mouse.
    let mut host = Host {
        h: PuzzleHost::new(e, hooks),
        pz1l,
        pz2l,
    };
    host.h.screen.pictures = vec![bg];
    host.h.sounds = sounds;
    let r = game.run(&mut host).await;
    let e = host.h.e;
    e.puzzles.eclipse = Some([game.records[0].curr, game.records[1].curr]);
    if e.should_abort() {
        1
    } else {
        r
    }
}

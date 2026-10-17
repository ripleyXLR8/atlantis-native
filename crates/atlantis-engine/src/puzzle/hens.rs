//! StartPuzzle 1 at CHAPI034: the hens on the levers (`FUN_004441e8`).
//! Hens put on a network of levers: two hens on the ends a and b of a
//! lever balance when their weights add up to the lever's sum; a weighing
//! passes the tilt of hen 9 along the balanced levers, and is won when it
//! reaches hen 1 (the balance) against the button's direction. Four won
//! weighings solve the puzzle.
//!
//! The original reads every short of its tables as the high half of the
//! dword two bytes before it, so target t is at (0x00496d92 + 4t,
//! 0x00496d94 + 4t) and the buttons at (0x00496dd2, 0x00496dd4) and
//! (0x00496dd6, 0x00496dd8).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`hens_rules.h`,
//! `puzzle_hens.cpp`; GPLv3, the ScummVM team).

use atlantis_data::sprite::SprSprite;

use super::{load_spr, load_tga, PuzzleHost, PuzzleSounds, SamplePlayer};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;
use crate::warp::{SCREEN_H, SCREEN_W};

pub const NUM_HENS: usize = 10;
pub const NUM_TARGETS: usize = 16;
pub const NUM_MARKS: usize = 4;
/// A target from this value up: the hen falls home, a step a frame.
pub const MOVING: i16 = 0x80;

/// The sounds of the puzzle (POULE1..POULE5, and the hens falling home,
/// sample 0x007739e8, which only puzzle 4 loads).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// POULE1: a hen picked up.
    Pick,
    /// POULE2: a hen put back home.
    Home,
    /// POULE3: a hen put on a target.
    Drop,
    /// POULE4: the balance swings, nothing happens.
    Even,
    /// POULE5: a weighing won.
    Win,
    /// The weighing lost: the hens fall home.
    Lose,
}

/// A hen record (0x00496e2c, 10 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hen {
    /// 0 at home, 1..15 on a target, from [`MOVING`] falling home.
    pub target: i16,
    pub weight: i16,
    pub home_x: i16,
    pub home_y: i16,
    /// -1, 0, 1.
    pub tilt: i16,
}

const fn hen(target: i16, weight: i16, home_x: i16, home_y: i16, tilt: i16) -> Hen {
    Hen {
        target,
        weight,
        home_x,
        home_y,
        tilt,
    }
}

/// The hens at the start: 1 is the balance, 9 the weight that tilts.
pub const HENS: [Hen; NUM_HENS] = [
    hen(0, 0, 0, 0, 0),
    hen(1, 41, 0, 0, 0),
    hen(0, 20, 163, 447, 0),
    hen(0, 24, 216, 456, 0),
    hen(0, 38, 119, 428, 0),
    hen(0, 12, 369, 460, 0),
    hen(0, 36, 478, 454, 0),
    hen(0, 22, 414, 453, 0),
    hen(0, 11, 340, 463, 0),
    hen(9, 33, 0, 0, 1),
];

/// The targets (0x00496d92 + 4t).
pub const TARGETS: [(i16, i16); NUM_TARGETS] = [
    (0, 0),
    (298, 150),
    (260, 198),
    (245, 239),
    (307, 254),
    (359, 258),
    (392, 222),
    (434, 263),
    (430, 295),
    (393, 320),
    (283, 199),
    (336, 203),
    (346, 175),
    (364, 164),
    (400, 276),
    (341, 300),
];

/// The levers (0x00496dda): (a, b, sum), ended in the original by a >= 99.
pub const LEVERS: [(i16, i16, i16); 26] = [
    (8, 7, 33),
    (8, 9, 44),
    (14, 5, 45),
    (14, 7, 35),
    (14, 9, 44),
    (15, 4, 57),
    (15, 5, 46),
    (15, 9, 55),
    (7, 6, 58),
    (6, 5, 48),
    (6, 11, 58),
    (5, 4, 50),
    (5, 9, 69),
    (5, 11, 58),
    (4, 3, 62),
    (4, 10, 60),
    (4, 11, 58),
    (11, 1, 65),
    (11, 12, 30),
    (2, 1, 61),
    (2, 3, 44),
    (2, 4, 74),
    (10, 1, 52),
    (10, 2, 24),
    (12, 1, 53),
    (12, 13, 20),
];

/// A won weighing's mark (0x00496e90), PUZPOUL0 frame 4 + i.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    pub shown: i16,
    pub x: i16,
    pub y: i16,
}

/// The game's I/O: the C++ `HensPuzzle` drawing and sounds.
#[allow(async_fn_in_trait)]
pub trait HensIo {
    /// The mouse and a new left click.
    fn read_input(&mut self) -> (i32, i32, bool);
    fn draw_background(&mut self);
    /// '0', '1', '2' PUZPOUL0..2, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    async fn present(&mut self, g: &HensGame);
    fn play_sound(&mut self, s: Sound);
    fn aborted(&self) -> bool {
        false
    }
}

/// The hens' state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HensGame {
    pub hens: [Hen; NUM_HENS],
    pub marks: [Mark; NUM_MARKS],
    /// 0x007bebf8: the hen held, 0 for none.
    pub carried: i16,
    /// 0x00496d18: the weighings won.
    pub won: i16,
    /// 0x00496d1a: the marks placed; the loop ends at 4.
    pub marked: i16,
    /// 0x004974c0 = 1: a top corner was clicked.
    pub leave: bool,
}

impl Default for HensGame {
    /// `FUN_004441e8`'s set-up.
    fn default() -> Self {
        HensGame {
            hens: HENS,
            marks: [Mark {
                shown: 0,
                x: 307,
                y: 399,
            }; NUM_MARKS],
            carried: 0,
            won: 0,
            marked: 0,
            leave: false,
        }
    }
}

/// Whether (x, y) is in a top corner, where a click leaves.
pub fn in_corner(x: i32, y: i32) -> bool {
    (0..0xdc).contains(&y) && ((0..0xa0).contains(&x) || (x > 0x21c && x < 0x280))
}

fn target_pos(t: i16) -> (i32, i32) {
    let (x, y) = TARGETS[t as usize];
    (x.into(), y.into())
}

impl HensGame {
    /// The position of target `t` (1..15).
    pub fn target_pos(t: usize) -> (i32, i32) {
        let (x, y) = TARGETS[t];
        (x.into(), y.into())
    }

    /// Four weighings won.
    pub fn solved(&self) -> bool {
        self.marked >= 4
    }

    /// `FUN_004457bc`: can hen n go on target t? Not if another hen is
    /// there, nor if a lever from t would be overloaded: the lever's sum
    /// less the hens already at its other end must leave room for n's
    /// weight.
    pub fn can_drop(&self, n: usize, t: i16) -> bool {
        if (1..NUM_HENS).any(|m| m != n && self.hens[m].target == t) {
            return false;
        }
        for &(a, b, sum) in &LEVERS {
            if t != a && t != b {
                continue;
            }
            let other = if t == a { b } else { a };
            let mut room = sum;
            for m in 1..NUM_HENS {
                if m != n && self.hens[m].target == other {
                    room -= self.hens[m].weight;
                }
            }
            if room < self.hens[n].weight {
                return false;
            }
        }
        true
    }

    /// `FUN_00445614`: hen m, on a balanced lever with hen n and not yet
    /// tilted, tilts the other way; recursively, in the original's order.
    pub fn propagate(&mut self, n: usize) {
        for m in 1..9 {
            if m == n || self.hens[m].target == 0 {
                continue;
            }
            let sum = self.hens[n].weight + self.hens[m].weight;
            for &(a, b, s) in &LEVERS {
                if a == self.hens[n].target
                    && b == self.hens[m].target
                    && s == sum
                    && self.hens[m].tilt == 0
                {
                    self.hens[m].tilt = -self.hens[n].tilt;
                    self.propagate(m);
                }
                if a == self.hens[m].target
                    && b == self.hens[n].target
                    && s == sum
                    && self.hens[m].tilt == 0
                {
                    self.hens[m].tilt = -self.hens[n].tilt;
                    self.propagate(m);
                }
            }
        }
    }

    /// `FUN_00444460(a, b, c)`: the picture, the marks, the balance and
    /// the counter (PUZPOUL2), the hens; a is the tilt of the balance, b
    /// and c frames of the weighing animations.
    pub fn draw_frame(&self, io: &mut impl HensIo, a: i32, b: i32, c: i32) {
        io.draw_background();
        for (i, m) in self.marks.iter().enumerate() {
            if m.shown > 0 {
                io.draw_sprite('0', m.x.into(), m.y.into(), i as i32 + 4);
            }
        }
        io.draw_sprite('2', 0x11c, 0x19f, c);
        let (tx, ty) = target_pos(1);
        io.draw_sprite('2', tx, ty, b + 0x2f);
        let counter = i32::from(self.won) * 0x15 + 0x46 + if b < 0x28 { b } else { 0 };
        io.draw_sprite('2', 0xf7, 0x3f, counter);
        for n in 2..NUM_HENS {
            if n as i16 == self.carried {
                continue;
            }
            let h = &self.hens[n];
            let base = (n as i32 - 2) * 31;
            if h.target == 0 {
                io.draw_sprite('0', h.home_x.into(), h.home_y.into(), n as i32 + 6);
            } else if h.target < MOVING {
                let mut f = match h.tilt {
                    1 => a,
                    -1 => -a,
                    _ => 0,
                };
                if f < 0 {
                    f = f.abs() + 15;
                }
                let (tx, ty) = target_pos(h.target);
                io.draw_sprite('1', tx - 1, ty - 1, f + base);
            } else {
                // Falling home: from target n's place (not the hen's), 10
                // pixels a step.
                let (tx, ty) = target_pos(n as i16);
                let y = ty + i32::from(h.target - MOVING) * 10;
                if y >= i32::from(h.home_y) - 8 {
                    io.draw_sprite('0', h.home_x.into(), h.home_y.into(), n as i32 + 6);
                } else {
                    io.draw_sprite('1', tx, y, base);
                }
            }
        }
    }

    /// A frame of a weighing animation, without the cursor.
    async fn anim(&self, io: &mut impl HensIo, a: i32, b: i32, c: i32) {
        self.draw_frame(io, a, b, c);
        io.present(self).await;
    }

    /// `FUN_00444d80`: a weighing; dir is 1 for the first button, -1 for
    /// the second; a is the tilt of the balance, reversed at +-15. The
    /// sound plays before the animation.
    pub async fn weigh(&mut self, io: &mut impl HensIo, mut dir: i32) -> Sound {
        for h in &mut self.hens[1..9] {
            h.tilt = 0;
        }
        self.propagate(9);
        let mut a = 0;
        let tilt = i32::from(self.hens[1].tilt);
        if tilt == 0 {
            // The balance swings once, nothing happens.
            io.play_sound(Sound::Even);
            loop {
                a += dir;
                self.anim(io, a, 0, 0).await;
                if a.abs() == 15 {
                    dir = -dir;
                }
                if a == 0 {
                    break;
                }
            }
            return Sound::Even;
        }
        if tilt * dir > 0 {
            // Lost: the hens fall home.
            io.play_sound(Sound::Lose);
            let (mut b, mut v) = (0x6d, MOVING + 8);
            for c in 0x12..0x2f {
                a += dir;
                if c > 0x23 {
                    for h in &mut self.hens[2..9] {
                        h.target = v;
                    }
                    v += 4;
                }
                self.anim(io, a, b, c).await;
                if a.abs() == 15 {
                    dir = -dir;
                }
                b += 1;
            }
            for h in &mut self.hens[2..9] {
                h.target = 0;
            }
            return Sound::Lose;
        }
        // Won.
        io.play_sound(Sound::Win);
        a = -8;
        for i in 1..0x16 {
            a += dir;
            self.anim(io, a, i, 0).await;
            if a.abs() == 15 {
                dir = -dir;
            }
        }
        self.won += 1;
        for i in 1..0x12 {
            self.anim(io, 0, 0, i).await;
        }
        let k = self.marked as usize;
        if k < NUM_MARKS {
            self.marks[k].shown += 1;
            let mut y = 0x18d;
            while y < 0x1a0 {
                self.marks[k].y = y;
                self.anim(io, 0, 0, 0).await;
                y += 5;
            }
            for i in 1..0x40 {
                if i > 0x19 {
                    self.marks[k].shown = (i / 4) % 2;
                    self.marks[k].x = self.marked * 20 + 0x106;
                    self.marks[k].y = 0x1c9;
                }
                self.anim(io, 0, 0, 0).await;
            }
            self.marks[k].shown = 1;
        }
        self.marked += 1;
        Sound::Win
    }

    /// `FUN_00444a4c`: a click. Returns the original's value: the hen
    /// picked up, the target + 16 of a drop, else 0.
    pub async fn click(&mut self, io: &mut impl HensIo, x: i32, y: i32) -> i32 {
        if in_corner(x, y) {
            self.leave = true;
        }
        if self.carried == 0 {
            for n in 2..9 {
                let h = self.hens[n];
                let r = i32::from(h.weight);
                let (dx, dy) = if h.target == 0 {
                    (
                        (x - i32::from(h.home_x)).abs(),
                        (y - i32::from(h.home_y)).abs() * 2,
                    )
                } else {
                    let (tx, ty) = target_pos(h.target);
                    ((x - tx).abs(), (y - ty).abs())
                };
                if dx < r && dy < r {
                    io.play_sound(Sound::Pick);
                    self.carried = n as i16;
                    return n as i32;
                }
            }
            if (x - 356).abs() < 12 && (y - 360).abs() < 12 {
                self.weigh(io, 1).await;
            }
            if (x - 420).abs() < 12 && (y - 360).abs() < 12 {
                self.weigh(io, -1).await;
            }
            return 0;
        }
        for t in 1..NUM_TARGETS as i16 {
            let (tx, ty) = target_pos(t);
            if (x - tx).abs() < 12 && (y - ty).abs() < 12 && self.can_drop(self.carried as usize, t)
            {
                io.play_sound(Sound::Drop);
                self.hens[self.carried as usize].target = t;
                self.carried = 0;
                return i32::from(t) + 16;
            }
        }
        io.play_sound(Sound::Home);
        self.hens[self.carried as usize].target = 0;
        self.carried = 0;
        0
    }

    /// `FUN_0044484c`: the carried hen at the mouse, then CURSEURS frame
    /// 11 over the corners, else frame 6 (`FUN_0042d558`).
    pub fn draw_cursor(&self, io: &mut impl HensIo, x: i32, y: i32) {
        if self.carried != 0 {
            io.draw_sprite('1', x, y, (i32::from(self.carried) - 2) * 31);
        }
        io.draw_sprite('C', x, y, if in_corner(x, y) { 11 } else { 6 });
    }

    /// `FUN_004441e8`'s loop: a click (`FUN_00444a4c`), the frame
    /// (`FUN_00444460`, `FUN_0044484c`), the corner test, until four
    /// weighings are won. 255 solved, 2 left.
    pub async fn run(&mut self, io: &mut impl HensIo) -> i32 {
        loop {
            let (x, y, clicked) = io.read_input();
            if clicked {
                self.click(io, x, y).await;
            }
            self.draw_frame(io, 0, 0, 0);
            self.draw_cursor(io, x, y);
            io.present(self).await;
            if self.leave || io.aborted() {
                return 2;
            }
            if self.solved() {
                return 255;
            }
        }
    }
}

struct Host<'a> {
    h: PuzzleHost<'a>,
    /// PUZPOUL0..2.SPR (0x007beae0, 0x007beadc, 0x007bead8).
    banks: [Vec<SprSprite>; 3],
}

impl HensIo for Host<'_> {
    fn read_input(&mut self) -> (i32, i32, bool) {
        let m = self.h.read_input();
        (m.x, m.y, m.click)
    }
    fn draw_background(&mut self) {
        self.h.draw_background();
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        let bank = match bank {
            '0' => &self.banks[0],
            '1' => &self.banks[1],
            '2' => &self.banks[2],
            _ => return self.h.draw_sprite(bank, x, y, frame),
        };
        if let Some(spr) = usize::try_from(frame).ok().and_then(|f| bank.get(f)) {
            spr.draw(&mut self.h.screen.working, SCREEN_W, SCREEN_H, x, y);
        }
    }
    async fn present(&mut self, _g: &HensGame) {
        self.h.present().await;
    }
    fn play_sound(&mut self, s: Sound) {
        let name = match s {
            Sound::Pick => "POULE1",
            Sound::Home => "POULE2",
            Sound::Drop => "POULE3",
            Sound::Even => "POULE4",
            Sound::Win => "POULE5",
            // Sample 0x007739e8 is not loaded by this puzzle.
            Sound::Lose => return,
        };
        self.h.play_sound(name, SamplePlayer::Ca28);
    }
    fn aborted(&self) -> bool {
        self.h.aborted()
    }
}

/// StartPuzzle 1: 255 when solved, 2 when the player leaves, 1 when a
/// file is missing or the game is quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let (Some(bg), Some(b0), Some(b1), Some(b2)) = (
        load_tga(e, "PUZPOUL1.TGA").await,
        load_spr(e, "PUZPOUL0.SPR").await,
        load_spr(e, "PUZPOUL1.SPR").await,
        load_spr(e, "PUZPOUL2.SPR").await,
    ) else {
        log::warn!("hens: missing files");
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &["POULE1", "POULE2", "POULE3", "POULE4", "POULE5"]).await;
    let mut game = HensGame::default();
    let mut host = Host {
        h: PuzzleHost::new(e, hooks),
        banks: [b0, b1, b2],
    };
    host.h.screen.pictures = vec![bg];
    host.h.sounds = sounds;
    let r = game.run(&mut host).await;
    if host.h.e.should_abort() {
        1
    } else {
        r
    }
}

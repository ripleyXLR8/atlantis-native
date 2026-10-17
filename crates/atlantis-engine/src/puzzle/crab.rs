//! StartPuzzle 15 at CHAPI066: the crab race (`FUN_0043d28c`). The player
//! picks one of four crabs and puts it down on the start line; the two
//! crabs then run a 1280-pixel track on their own (one pixel a pass,
//! random turns from the puzzle LCG) over three scrolling pictures, and the
//! first past the finish line wins the round. Rounds go on to three wins
//! (255) or three losses (2); leaving the choice screen counts as three
//! losses (1).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`crab_game.cpp`,
//! `puzzle_crab.cpp`; GPLv3, the ScummVM team).

use super::{
    load_spr, load_tga, puzzle_lcg_next, PuzzleCursor, PuzzleHost, PuzzleSounds, SamplePlayer,
};
use crate::ambient::{SMP_DONE, SMP_STOPPED};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;

/// A crab record (0x0049739a, 16 bytes). Only kind, x, y, dir, interval
/// and reload are reset by a round: phase and timer carry over, from the
/// previous round and from the previous run (static data).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Crab {
    pub kind: i16,
    pub x: i16,
    pub y: i16,
    pub dir: i16,
    pub phase: i16,
    pub timer: i16,
    pub interval: i16,
    pub reload: i16,
}

pub const NUM_CRABS: usize = 2;

/// A choice box (0x004973ba): x, y, and the crab's turn interval A and
/// timer reload B. A box is 200 x 140 from (x, y).
struct CrabBox {
    x: i16,
    y: i16,
    a: i16,
    b: i16,
}

const BOXES: [CrabBox; 4] = [
    CrabBox {
        x: 59,
        y: 63,
        a: 140,
        b: 100,
    },
    CrabBox {
        x: 300,
        y: 50,
        a: 190,
        b: 100,
    },
    CrabBox {
        x: 309,
        y: 237,
        a: 50,
        b: 50,
    },
    CrabBox {
        x: 161,
        y: 205,
        a: 80,
        b: 50,
    },
];

/// The jump points (0x004973fa): direction, x, y.
const POINTS: [(i16, i16, i16); 6] = [
    (0x10, 254, 346),
    (0x10, 299, 150),
    (0x10, 640, 215),
    (0x11, 640, 274),
    (0x11, 932, 182),
    (0x11, 958, 327),
];

/// The score comments (0x004973da) by wins, then losses.
const SCORE_COMMENTS: [[i16; 4]; 4] = [
    [0, 13, 17, 0],
    [21, 14, 18, 0],
    [22, 15, 19, 0],
    [23, 16, 20, 0],
];

/// The pictures: the choice, then the three parts of the track.
pub const PICTURES: [&str; 4] = [
    "crbpani.tga",
    "crbfond1.tga",
    "crbfond2.tga",
    "crbfond3.tga",
];

/// The spoken comments (the name table 0x004978bc), comment n = entry
/// n - 1.
const COMMENTS: [&str; 26] = [
    "ATA0939A", "ATA0939B", "ATA0939C", "ATA0939D", "ATA0939E", "ATA0939F", "ATA0939G", "ATA0939H",
    "ATA0939I", "ATA0939J", "ATA0939K", "ATA0939L", "ATA0940A", "ATA0940B", "ATA0940C", "ATA0940D",
    "ATA0940E", "ATA0940F", "ATA0940G", "ATA0940H", "ATA0940I", "ATA0940J", "ATA0940K", "ATA0943A",
    "ATA0938F", "ATA0938G",
];

/// The game's I/O: the C++ `CrabGame` virtual functions.
#[allow(async_fn_in_trait)]
pub trait CrabIo {
    /// The input when the puzzle starts: the mouse and a new click.
    fn start_input(&mut self) -> (i32, i32, bool);
    /// `FUN_0043318c`: a frame boundary.
    fn boundary(&mut self, g: &CrabGame) -> (i32, i32, bool);
    /// `FUN_00447478`: one of [`PICTURES`].
    fn load_picture(&mut self, name: &str);
    /// `FUN_00410f36`.
    fn draw_background(&mut self);
    /// 'A' crabe0.spr (the portraits, never seen), 'B' crabe.spr, 'C'
    /// CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    /// `FUN_0041afd0`.
    async fn present(&mut self, g: &CrabGame);
    /// 0 crabe1, 1 crabe2.
    fn play_sound(&mut self, slot: i32);
    /// `FUN_0043cf70`, n >= 1.
    fn play_comment(&mut self, n: i32);
    /// The comment sample's status is not 2 or 8.
    fn comment_busy(&mut self) -> bool;
    /// `FUN_0043d0e0`.
    fn stop_sounds(&mut self) {}
    /// `FUN_00453cd1`.
    fn rand32767(&mut self) -> i32;
    fn aborted(&self) -> bool {
        false
    }
}

/// The crab race's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrabGame {
    pub crabs: [Crab; NUM_CRABS],
    /// 0x007bead0: 0 the choice, 1 the introductions, 2..4 the panel shown.
    pub state: i32,
    /// 0x007bebf8: the box chosen + 1 until the crab is put down.
    pub hand: i32,
    /// 0x007beaac.
    pub last_comment: i32,
    pub cursor: PuzzleCursor,
    /// 0x004974c0.
    pub result: i32,
    /// 0x007bec20, seeded by the dispatcher.
    pub lcg_seed: u16,
    /// 0x0049611c.
    pub click: bool,
    mx: i32,
    my: i32,
}

/// The static records: kinds 1 and 2, all else 0.
pub const INITIAL_CRABS: [Crab; NUM_CRABS] = [
    Crab {
        kind: 1,
        x: 0,
        y: 0,
        dir: 0,
        phase: 0,
        timer: 0,
        interval: 0,
        reload: 0,
    },
    Crab {
        kind: 2,
        x: 0,
        y: 0,
        dir: 0,
        phase: 0,
        timer: 0,
        interval: 0,
        reload: 0,
    },
];

impl Default for CrabGame {
    fn default() -> Self {
        CrabGame {
            crabs: INITIAL_CRABS,
            state: 0,
            hand: 0,
            last_comment: -1,
            cursor: PuzzleCursor::default(),
            result: 0,
            lcg_seed: 0,
            click: false,
            mx: 0,
            my: 0,
        }
    }
}

/// Past the finish line.
fn finished(c: &Crab) -> bool {
    let (x, y) = (i32::from(c.x), i32::from(c.y));
    x > 0x49b && (x - 0x49b) * 3 + 0xc5 > y
}

impl CrabGame {
    /// `FUN_00447524`: 0..n.
    pub fn lcg(&mut self, n: i32) -> i32 {
        puzzle_lcg_next(&mut self.lcg_seed, n)
    }

    fn read_boundary(&mut self, io: &mut impl CrabIo) {
        let (mx, my, c) = io.boundary(self);
        self.mx = mx;
        self.my = my;
        self.click = c;
    }

    /// `FUN_0043d494`.
    fn set_up(&mut self, io: &mut impl CrabIo) {
        io.load_picture("crbpani.tga");
        self.hand = 0;
    }

    /// `FUN_0043ed98`.
    fn refuse(&mut self, io: &mut impl CrabIo) {
        if self.cursor.frame == 2 {
            return;
        }
        io.play_sound(1);
        self.cursor.refuse();
    }

    /// `FUN_0043ebf8`.
    fn handle_click(&mut self, io: &mut impl CrabIo) {
        let (mx, my) = (self.mx, self.my);
        if self.state != 0 {
            if self.hand > 0 {
                // The start: a triangle with its apex at (88, 229), widening
                // leftwards down to y 303.
                let dx = 88 - mx;
                let dy = my - 229;
                if dx >= 0 && (0..75).contains(&dy) && 3 * dx > dy {
                    self.crabs[0].x = mx as i16;
                    self.crabs[0].y = my as i16;
                    self.hand = 0;
                    io.play_sound(0);
                } else {
                    self.refuse(io);
                }
            } else if my < 40 {
                self.result = 2;
            }
            return;
        }
        if self.hand != 0 {
            return;
        }
        // Of two boxes under the mouse the later one wins.
        for (i, b) in BOXES.iter().enumerate() {
            let (x, y) = (i32::from(b.x), i32::from(b.y));
            if mx > x && x + 200 > mx && my > y && y + 140 > my {
                self.hand = i as i32 + 1;
                io.play_sound(0);
            }
        }
        if my < 40 {
            self.result = 1;
        } else if self.hand == 0 {
            self.refuse(io);
        }
    }

    /// `FUN_0043d614`.
    fn panel_picture(&mut self, io: &mut impl CrabIo) {
        if (0..=4).contains(&self.state) {
            let i = if self.state < 3 { 1 } else { self.state - 1 };
            io.load_picture(PICTURES[i as usize]);
        }
    }

    /// `FUN_0043d500`.
    fn race_set_up(&mut self, io: &mut impl CrabIo) {
        self.panel_picture(io);
        let k = loop {
            let k = self.lcg(3);
            if k != self.hand - 1 {
                break k;
            }
        };
        let mine = &BOXES[(self.hand - 1) as usize];
        let other = &BOXES[k as usize];
        self.crabs[0].kind = (self.hand - 1) as i16;
        self.crabs[1].kind = k as i16;
        for c in self.crabs.iter_mut() {
            c.x = 93;
            c.y = 206;
            c.dir = 3;
        }
        self.crabs[0].interval = mine.a;
        self.crabs[1].interval = other.a;
        self.crabs[0].reload = mine.b;
        self.crabs[1].reload = other.b;
        // crabe.spr replaces crabe0.spr from here on (bank 'B').
    }

    /// `FUN_0043dd38`.
    fn move_crab(&mut self, i: usize) {
        let t = self.crabs[i].timer;
        self.crabs[i].timer = t.wrapping_add(1);
        let r = self.lcg(255);
        let c = self.crabs[i];
        let mut dir = i32::from(c.dir);
        let mut x = i32::from(c.x);
        let mut y = i32::from(c.y);
        let wheel = if c.kind == 1 { 6 } else { 5 };
        let mut turned = false;
        if c.interval < t && r < 0x14 {
            let mut done = false;
            if (dir == 2 || dir == 4) && (r & 3) != 0 {
                dir = 3;
                done = true;
            }
            if !done {
                dir += if r & 1 != 0 { 1 } else { -1 };
            }
            if dir == 0 {
                dir = wheel;
            }
            if dir > wheel {
                dir = 1;
            }
            self.crabs[i].dir = dir as i16;
            self.crabs[i].timer = 0;
            turned = true;
        }
        let mut point = 0;
        match dir {
            1 => y -= 1,
            2 => {
                y -= 1;
                x += 1;
            }
            3 => x += 1,
            4 => {
                y += 1;
                x += 1;
            }
            5 => y += 1,
            6 => x -= 1,
            0x10 | 0x11 => {
                if self.crabs[i].phase < 7 {
                    x += if dir == 0x10 { 0x12 } else { -0x12 };
                } else {
                    // The draws only advance the LCG: the direction is
                    // then set.
                    loop {
                        self.crabs[i].dir = self.lcg(6) as i16;
                        if self.crabs[i].dir != 0 {
                            break;
                        }
                    }
                    self.crabs[i].dir = if dir == 0x10 { 3 } else { 6 };
                }
            }
            _ => {}
        }
        let mut ok = y > 0x73 && y < 0x184;
        if x < 0xb1 - (y - 0x6e) / 4 && !(2..=4).contains(&dir) {
            ok = false;
        }
        if x > 0x43c && y > 0x183 - (x - 0x43c) / 3 && dir > 2 && dir < 6 {
            ok = false;
        }
        if y < 0xc5 && x > 0x3ff && y < x - 0x38a && dir < 4 {
            ok = false;
        }
        if dir < 0x10 {
            for &(pd, px, py) in &POINTS {
                let (px, py) = (i32::from(px), i32::from(py));
                if (py - y).abs() * 2 + (px - x).abs() < 0x28 {
                    x = px;
                    y = py;
                    point = i32::from(pd);
                }
            }
        }
        let o = self.crabs[if i != 0 { 0 } else { 1 }];
        if (i32::from(o.y) - y).abs() * 2 + (i32::from(o.x) - x).abs() < 0x32 && self.hand == 0 {
            ok = false;
        }
        let c = &mut self.crabs[i];
        if ok {
            c.x = x as i16;
            c.y = y as i16;
            if point != 0 {
                c.dir = point as i16;
                c.phase = 7;
                c.timer = 0;
            }
        } else {
            if turned {
                c.timer = c.reload;
            }
            if c.phase == 0 {
                c.phase = 7;
            } else {
                c.phase -= 1;
            }
        }
    }

    /// `FUN_0043dbec`.
    fn race_tick(&mut self, io: &mut impl CrabIo) {
        if self.hand == 0 {
            self.move_crab(0);
        }
        self.move_crab(1);
        let mut n = [0i32; 5];
        for c in &self.crabs {
            let x = i32::from(c.x);
            if x < 0x262 {
                n[2] += 1;
            }
            if x > 0x15e && x < 0x3a2 {
                n[3] += 1;
            }
            if x > 0x29e {
                n[4] += 1;
            }
        }
        let state = self.state;
        if n[state as usize] < 2 {
            let mut k = state;
            for (j, &c) in n.iter().enumerate().skip(2) {
                if c == 2 {
                    k = j as i32;
                }
            }
            if k == state {
                if n[3] != 0 {
                    k = 3;
                }
                if n[4] != 0 {
                    k = 4;
                }
            }
            if k != state {
                self.state = k;
                self.panel_picture(io);
            }
        }
    }

    /// `FUN_0043dad0`: the frame of crab `i`, its phase advanced first.
    fn frame_of(&mut self, i: usize, advance: bool) -> i32 {
        let c = &mut self.crabs[i];
        let mut phase = i32::from(c.phase);
        if advance {
            phase += 1;
            if phase > 7 {
                phase = 0;
            }
            c.phase = phase as i16;
        }
        let mut f = i32::from(c.kind) * 0x38 + 1 + phase;
        f += match c.dir {
            1 | 5 => 0x10,
            2 => 0x28,
            3 => 0x18,
            4 => 0x20,
            6 => 0x30,
            0x10 => 8,
            _ => 0,
        };
        i32::from(f as i16)
    }

    /// `FUN_0043e2ec`: the track's x on the screen.
    fn screen_x(&self, x: i16) -> i32 {
        let x = i32::from(x);
        match self.state {
            3 => x - 0x140,
            4 => x - 0x280,
            _ => x,
        }
    }

    /// `FUN_0043e350`.
    fn draw_frame(&mut self, io: &mut impl CrabIo) {
        io.draw_background();
        if self.state != 0 {
            let first = usize::from(self.state > 1 && self.crabs[0].y > self.crabs[1].y);
            let second = if first != 0 && self.hand == 0 { 0 } else { 1 };
            if self.state > 1 && self.hand == 0 {
                let f = self.frame_of(first, true);
                let c = self.crabs[first];
                io.draw_sprite('B', self.screen_x(c.x), c.y.into(), f);
            }
            let f = self.frame_of(second, true);
            let c = self.crabs[second];
            io.draw_sprite('B', self.screen_x(c.x), c.y.into(), f);
            if self.state == 4 {
                io.draw_sprite('B', 0x1d5, 0x9a, 0);
            }
            return;
        }
        // The portraits are drawn on the locked surface itself, which the
        // pass then overwrites with the work buffer: they are never seen.
        for (i, b) in BOXES.iter().enumerate() {
            if self.hand - 1 != i as i32 {
                io.draw_sprite('A', b.x.into(), b.y.into(), i as i32);
            }
        }
    }

    /// `FUN_0043e5a4`.
    fn draw_cursor(&mut self, io: &mut impl CrabIo) {
        if self.hand != 0 && self.state != 0 {
            let f = self.frame_of(0, true);
            io.draw_sprite('B', self.mx, self.my, f);
        }
        if self.cursor.bouncing() {
            // FUN_00448958, the shared cursor animation.
            let f = self.cursor.bounce();
            io.draw_sprite('C', self.mx, self.my, f.into());
            return;
        }
        let f = if self.my < 40 { 10 } else { 5 };
        if !(self.state >= 2 && f == 5) {
            io.draw_sprite('C', self.mx, self.my, f);
        }
    }

    async fn pass(&mut self, io: &mut impl CrabIo) {
        self.draw_frame(io);
        self.draw_cursor(io);
        io.draw_background();
        io.present(self).await;
    }

    /// `FUN_0043e7d0`.
    async fn finish_and_start(&mut self, io: &mut impl CrabIo) {
        if self.state != 0 {
            if finished(&self.crabs[0]) {
                self.result = 255;
            }
            if finished(&self.crabs[1]) {
                self.result = 2;
            }
            return;
        }
        if self.hand == 0 {
            return;
        }
        self.race_set_up(io);
        self.state += 1;
        // The two introductions, each played to its end with passes drawn
        // meanwhile; the clicks of those passes are dropped.
        let mut n = 0;
        while n < 3 && !io.aborted() {
            let mut busy = io.comment_busy();
            while busy && !io.aborted() {
                self.pass(io).await;
                if n == 0 {
                    self.click = false;
                }
                self.read_boundary(io);
                busy = io.comment_busy();
            }
            match n {
                0 => self.comment(io, 0x19, true),
                1 => {
                    self.click = false;
                    self.comment(io, 0x1a, true);
                }
                _ => self.click = false,
            }
            n += 1;
        }
        self.state += 1;
    }

    /// `FUN_0043d994`.
    fn choose_comment(&self) -> i32 {
        let x0 = i32::from(self.crabs[0].x);
        let x1 = i32::from(self.crabs[1].x);
        let d = x0 - x1;
        let dir1 = self.crabs[1].dir;
        if self.crabs[0].dir == 0x11 || dir1 == 0x10 {
            return 8;
        }
        match dir1 {
            6 => return 5,
            3 => return 7,
            2 | 4 => return 2,
            _ => {}
        }
        if x1 > 0x44c && x1 > x0 {
            return 12;
        }
        if x1 < 0x3e8 && x1 > x0 {
            return 11;
        }
        if d > 0x1f4 {
            return 4;
        }
        if d > 0x12c {
            return 9;
        }
        if d > 0xc8 {
            return 3;
        }
        if d < 0xc8 {
            return 6;
        }
        1
    }

    /// `FUN_0043ede8`.
    fn comment(&mut self, io: &mut impl CrabIo, n: i32, force: bool) {
        if force {
            if n >= 1 {
                io.play_comment(n);
            }
            return;
        }
        let busy = io.comment_busy();
        let r = io.rand32767();
        if r < 0x3e80 && !busy && n >= 1 {
            io.play_comment(n);
        }
    }

    /// `FUN_0043d91c`.
    fn race_comment(&mut self, io: &mut impl CrabIo) {
        if self.state <= 2 || self.result != 0 {
            return;
        }
        if io.comment_busy() {
            return;
        }
        let c = self.choose_comment();
        if c == self.last_comment {
            return;
        }
        self.comment(io, c, false);
        self.last_comment = c;
    }

    /// `FUN_0043d6b0`.
    async fn score_comment(&mut self, io: &mut impl CrabIo, won: i32, lost: i32) {
        // Two identical waits for the comment sample, with the status read
        // before the boundary of each pass.
        for _ in 0..2 {
            let mut busy = io.comment_busy();
            while busy && !io.aborted() {
                self.pass(io).await;
                self.click = false;
                busy = io.comment_busy();
                self.read_boundary(io);
            }
        }
        if (0..4).contains(&won) && (0..4).contains(&lost) {
            let c = SCORE_COMMENTS[won as usize][lost as usize];
            self.comment(io, c.into(), true);
        }
    }

    /// `FUN_0043d28c`: 255 three wins, 2 three losses, 1 left.
    pub async fn run(&mut self, io: &mut impl CrabIo) -> i32 {
        (self.mx, self.my, self.click) = io.start_input();
        let mut won = 0;
        let mut lost = 0;
        self.last_comment = -1;
        loop {
            self.set_up(io);
            self.result = 0;
            self.state = 0;
            loop {
                if self.click {
                    self.handle_click(io);
                }
                if self.state > 1 {
                    self.race_tick(io);
                }
                self.pass(io).await;
                self.click = false;
                self.finish_and_start(io).await;
                self.race_comment(io);
                self.read_boundary(io);
                if self.result != 0 || io.aborted() {
                    break;
                }
            }
            if io.aborted() {
                return 1;
            }
            if self.result == 255 {
                won += 1;
            } else {
                lost += 1;
            }
            if self.result == 1 {
                lost = 3;
            } else {
                self.score_comment(io, won, lost).await;
            }
            if won >= 3 || lost >= 3 {
                break;
            }
        }
        io.stop_sounds();
        if won == 3 {
            self.result = 255;
        } else if lost == 3 && self.result != 1 {
            self.result = 2;
        }
        io.stop_sounds();
        self.result
    }
}

struct Host<'a> {
    host: PuzzleHost<'a>,
}

impl CrabIo for Host<'_> {
    fn start_input(&mut self) -> (i32, i32, bool) {
        let m = self.host.read_input();
        (m.x, m.y, m.click)
    }
    fn boundary(&mut self, _g: &CrabGame) -> (i32, i32, bool) {
        self.start_input()
    }
    fn load_picture(&mut self, name: &str) {
        if let Some(i) = PICTURES.iter().position(|p| p.eq_ignore_ascii_case(name)) {
            self.host.screen.picture = i;
        }
    }
    fn draw_background(&mut self) {
        self.host.draw_background();
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        if bank == 'A' {
            return; // the portraits, overwritten before they are shown
        }
        self.host.draw_sprite(bank, x, y, frame);
    }
    async fn present(&mut self, _g: &CrabGame) {
        self.host.present().await;
    }
    fn play_sound(&mut self, slot: i32) {
        if slot == 0 {
            // FUN_0043ce80: the click sound plays on sample 0 only when its
            // status is 2 or 8 (done or stopped); while sample 0 plays (the
            // previous click, or a sound FUN_0043ca28 put there) it is
            // dropped.
            self.host.play_sound("CRABE1", SamplePlayer::Ce80);
        } else {
            self.host.play_sound("CRABE2", SamplePlayer::Ca28); // FUN_0043ed98
        }
    }
    /// `FUN_0043cf70`: AIL_stop_sample and AIL_start_sample on sample 1.
    fn play_comment(&mut self, n: i32) {
        if let Some(name) = usize::try_from(n - 1).ok().and_then(|i| COMMENTS.get(i)) {
            self.host.play_sound(name, SamplePlayer::Cf70);
        }
    }
    /// The waits test sample 1 for status 2 or 8 (`FUN_0043d6b0`, ...).
    fn comment_busy(&mut self) -> bool {
        let st = self.host.sample_status(1);
        st != SMP_DONE && st != SMP_STOPPED
    }
    fn stop_sounds(&mut self) {
        self.host.stop_samples();
    }
    /// `FUN_0043ede8` @ 0x0043ee25.
    fn rand32767(&mut self) -> i32 {
        self.host.e.puzzles.crt_rand()
    }
    fn aborted(&self) -> bool {
        self.host.aborted()
    }
}

/// StartPuzzle 15: 255 after three wins, 2 after three losses, 1 when the
/// player leaves the choice, a file is missing or the game is quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let frames = load_spr(e, "CRABE.SPR").await;
    let mut pictures: [Vec<u16>; 4] = Default::default();
    let mut ok = frames.is_some();
    for (p, name) in pictures.iter_mut().zip(PICTURES) {
        match load_tga(e, &name.to_ascii_uppercase()).await {
            Some(px) => *p = px,
            None => ok = false,
        }
    }
    let Some(frames) = frames.filter(|_| ok) else {
        log::warn!("crab: missing files");
        return 1;
    };
    let mut names = vec!["CRABE1", "CRABE2"];
    names.extend(COMMENTS);
    let sounds = PuzzleSounds::load(e, &names).await;
    let mut game = CrabGame {
        crabs: e.puzzles.crabs.unwrap_or(INITIAL_CRABS),
        // FUN_0043a1e0 seeded the puzzle LCG from the clock.
        lcg_seed: e.puzzles.lcg_seed,
        cursor: e.puzzles.cursor,
        ..CrabGame::default()
    };
    let mut host = Host {
        host: PuzzleHost::new(e, hooks),
    };
    host.host.screen.pictures = pictures.into();
    host.host.screen.frames = frames;
    host.host.sounds = sounds;
    let r = game.run(&mut host).await;
    let e = host.host.e;
    e.puzzles.cursor = game.cursor;
    e.puzzles.crabs = Some(game.crabs);
    e.puzzles.lcg_seed = game.lcg_seed;
    if e.should_abort() {
        1
    } else {
        r
    }
}

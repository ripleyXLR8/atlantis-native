//! StartPuzzle 8 at CHAPI034: the four snakes (`FUN_0043b0f4`). Four
//! pieces put in eight holes linked in pairs; snake k starts from the
//! partner of piece k's hole and is steered through the maze of
//! PUZZLES\CHEMIN00.PCX (0 wall, 1 corridor, 6 junction) back to its
//! piece. Paths never share a junction.
//!
//! Direction codes: 0 up, 1 left, 2 right, 3 down; a click direction d
//! (`FUN_0043aad0`'s argument): 0 right, 1 left, 2 up, 3 down.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`snakes_rules.h`,
//! `puzzle_snakes.cpp`; GPLv3, the ScummVM team).

use super::{load_spr, load_tga, read_puzzle_file, PuzzleHost, PuzzleSounds, SamplePlayer};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;

pub const MAP_W: i32 = 640;
pub const MAP_H: i32 = 480;
pub const NUM_HOLES: usize = 8;
pub const NUM_SNAKES: usize = 4;
pub const MAX_SEGMENTS: usize = 0xc8;

/// The sounds (PzS1_1, 2, 3 and 5; PzS1_4 is loaded but not played).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    None,
    Pick,
    Place,
    Move,
    Win,
}

/// A hole (0x004974c4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hole {
    pub used: u8,
    pub x: i32,
    pub y: i32,
    /// The hole the snake of a piece put here starts from.
    pub partner: u8,
    /// The snake's first direction code from this hole.
    pub dir: u8,
}

const fn hole(x: i32, y: i32, partner: u8, dir: u8) -> Hole {
    Hole {
        used: 0,
        x,
        y,
        partner,
        dir,
    }
}

/// The holes as in the original's data.
pub const HOLES: [Hole; NUM_HOLES] = [
    hole(175, 205, 1, 2),
    hole(455, 355, 0, 1),
    hole(225, 65, 3, 2),
    hole(455, 215, 2, 1),
    hole(295, 165, 5, 3),
    hole(365, 295, 4, 1),
    hole(175, 325, 7, 0),
    hole(425, 75, 6, 3),
];

/// A piece (0x00497894).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Piece {
    pub placed: u8,
    pub x: i32,
    pub y: i32,
}

/// A snake (0x0049751c).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snake {
    pub active: i32,
    pub start_x: i32,
    pub start_y: i32,
    /// The head.
    pub x: i32,
    pub y: i32,
    pub len: u8,
    /// The direction code of each segment.
    pub dirs: [u8; MAX_SEGMENTS],
    /// The piece it goes to.
    pub piece: u8,
}

impl Default for Snake {
    fn default() -> Self {
        Snake {
            active: 0,
            start_x: 0,
            start_y: 0,
            x: 0,
            y: 0,
            len: 0,
            dirs: [0; MAX_SEGMENTS],
            piece: 0,
        }
    }
}

impl Snake {
    /// The segment byte at index i (dirs[i - 1]), the original's index 0
    /// being the length byte itself (and index 201 the piece byte).
    pub fn segment(&self, i: usize) -> u8 {
        match i {
            0 => self.len,
            _ => self
                .dirs
                .get(i - 1)
                .copied()
                .unwrap_or(if i - 1 == MAX_SEGMENTS { self.piece } else { 0 }),
        }
    }
}

/// A sprite of SERPNEW.SPR at (x, y).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Draw {
    pub x: i32,
    pub y: i32,
    pub frame: i32,
}

/// The game's I/O: the C++ `SnakesPuzzle` drawing and sounds.
#[allow(async_fn_in_trait)]
pub trait SnakesIo {
    /// The mouse and a new left click.
    fn read_input(&mut self) -> (i32, i32, bool);
    fn draw_background(&mut self);
    /// 'S' SERPNEW, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    async fn present(&mut self, g: &SnakesGame);
    fn play_sound(&mut self, s: Sound);
    /// Ticks of the 200 Hz timer.
    async fn wait_ticks(&mut self, n: u32);
    fn aborted(&self) -> bool {
        false
    }
}

/// The snakes' state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnakesGame {
    /// 0x007beab8: the maze, 640 x 480.
    pub map: Vec<u8>,
    pub holes: [Hole; NUM_HOLES],
    pub pieces: [Piece; NUM_SNAKES],
    pub snakes: [Snake; NUM_SNAKES],
    /// 0x007bebf8: the piece carried + 1, 0 for none.
    pub carried: i32,
    /// 0x004974c0: 1 left, 255 solved.
    pub result: i32,
    /// Fix of the original's first-run win (minor bug fixes): see
    /// [`SnakesGame::frame`].
    pub fix_first_run_win: bool,
}

/// `FUN_0043a53c`: a 640 x 480 PCX, the 128-byte header skipped, 0xC0
/// runs. `None` when the file ends early.
pub fn decode_map(pcx: &[u8]) -> Option<Vec<u8>> {
    let n = (MAP_W * MAP_H) as usize;
    let mut map = Vec::with_capacity(n);
    let mut pos = 0x80;
    while map.len() < n {
        let b = *pcx.get(pos)?;
        if b & 0xc0 == 0xc0 {
            pos += 1;
            let v = *pcx.get(pos)?;
            let run = usize::from(b & 0x3f).min(n - map.len());
            map.extend(std::iter::repeat_n(v, run));
        } else {
            map.push(b);
        }
        pos += 1;
    }
    Some(map)
}

/// The step of a direction code.
fn step(code: u8) -> (i32, i32) {
    (
        i32::from(code == 2) - i32::from(code == 1),
        i32::from(code == 3) - i32::from(code == 0),
    )
}

/// The direction code of a click direction.
fn code_of(d: usize) -> u8 {
    [2, 1, 0, 3][d]
}

/// `FUN_0043a748`: a click near the left or right edge leaves
/// (`DAT_00496d0c`); the click is still processed.
pub fn at_edge(x: i32) -> bool {
    !(0x1e..=0x26c).contains(&x)
}

impl SnakesGame {
    /// The game on `map` with the snakes `snakes` of the last run.
    pub fn new(map: Vec<u8>, snakes: [Snake; NUM_SNAKES]) -> SnakesGame {
        let mut g = SnakesGame {
            map,
            holes: HOLES,
            pieces: [Piece::default(); NUM_SNAKES],
            snakes,
            carried: 0,
            result: 0,
            fix_first_run_win: false,
        };
        g.setup();
        g
    }

    /// The entry's set-up. It leaves the snakes' places and pieces as the
    /// last run left them (0 on a first run).
    pub fn setup(&mut self) {
        self.holes = HOLES;
        for k in 0..NUM_SNAKES {
            self.snakes[k].active = 0;
            self.snakes[k].len = 0;
            self.pieces[k] = Piece {
                placed: 0,
                x: 0x28 + 0x14 * k as i32,
                y: 0x1c0,
            };
        }
        self.carried = 0;
        self.result = 0;
    }

    pub fn all_placed(&self) -> bool {
        self.pieces.iter().all(|p| p.placed != 0)
    }

    pub fn at(&self, x: i32, y: i32) -> u8 {
        usize::try_from(y * MAP_W + x)
            .ok()
            .and_then(|i| self.map.get(i).copied())
            .unwrap_or(0)
    }

    /// Walks from (x, y) along `code` to the first cell that is not a
    /// corridor.
    pub fn walk(&self, code: u8, x: &mut i32, y: &mut i32) {
        let (dx, dy) = step(code);
        loop {
            *x += dx;
            *y += dy;
            if self.at(*x, *y) != 1 {
                break;
            }
        }
    }

    /// `FUN_0043a8e8`: is (x, y) a node of some snake's path?
    pub fn on_a_path(&self, x: i32, y: i32) -> bool {
        for s in &self.snakes {
            let (mut cx, mut cy) = (s.start_x, s.start_y);
            for i in 0..usize::from(s.len) {
                self.walk(s.segment(i + 1), &mut cx, &mut cy);
                if cx == x && cy == y {
                    return true;
                }
            }
        }
        false
    }

    /// `FUN_0043aad0`: snake k one segment in click direction d, or one
    /// back when d undoes its last segment.
    pub fn move_snake(&mut self, k: usize, d: usize) -> Sound {
        let code = code_of(d);
        // The code d undoes.
        const REVERSE: [u8; 4] = [1, 2, 3, 0];
        let len = usize::from(self.snakes[k].len);
        if len > 1 && self.snakes[k].segment(len) == REVERSE[d] {
            let (mut x, mut y) = (self.snakes[k].x, self.snakes[k].y);
            self.walk(code, &mut x, &mut y);
            let s = &mut self.snakes[k];
            s.len -= 1;
            (s.x, s.y) = (x, y);
            return Sound::Move;
        }
        if len == 1 && self.snakes[k].segment(1) == REVERSE[d] {
            return Sound::None;
        }
        let (dx, dy) = step(code);
        let (sx, sy) = (self.snakes[k].x, self.snakes[k].y);
        // FUN_0043a788.
        if self.at(sx + dx, sy + dy) != 1 {
            return Sound::None;
        }
        // FUN_0043a820.
        let (mut ex, mut ey) = (sx, sy);
        self.walk(code, &mut ex, &mut ey);
        if self.on_a_path(ex, ey) {
            return Sound::None;
        }
        let s = &mut self.snakes[k];
        let snd = if s.len > 0 { Sound::Move } else { Sound::None };
        (s.x, s.y) = (ex, ey);
        if let Some(d) = s.dirs.get_mut(len) {
            *d = code;
        }
        s.len = s.len.wrapping_add(1);
        snd
    }

    /// `FUN_0043c35c`: a click of phase 1, a piece picked up or put in a
    /// hole.
    pub fn place(&mut self, x: i32, y: i32) -> Sound {
        if at_edge(x) {
            self.result = 1;
        }
        let near = |px: i32, py: i32| px - 10 < x && x < px + 10 && py - 10 < y && y < py + 10;
        if self.carried == 0 {
            for k in 0..NUM_SNAKES {
                let p = self.pieces[k];
                if near(p.x, p.y) && p.placed == 0 {
                    self.carried = k as i32 + 1;
                    return Sound::Pick;
                }
            }
            return Sound::None;
        }
        for h in 0..NUM_HOLES {
            let hole = self.holes[h];
            if !(near(hole.x, hole.y) && hole.used == 0) {
                continue;
            }
            let k = (self.carried - 1) as usize;
            let other = self.holes[usize::from(hole.partner)];
            self.pieces[k] = Piece {
                placed: 1,
                x: hole.x,
                y: hole.y,
            };
            self.holes[h].used = 1;
            self.holes[usize::from(hole.partner)].used = 1;
            let s = &mut self.snakes[k];
            s.active = 1;
            (s.x, s.start_x) = (other.x, other.x);
            (s.y, s.start_y) = (other.y, other.y);
            s.dirs[0] = other.dir;
            s.piece = k as u8;
            const FIRST_MOVE: [usize; 4] = [2, 1, 0, 3];
            let d = FIRST_MOVE[usize::from(s.dirs[0] & 3)];
            self.move_snake(k, d);
            self.carried = 0;
            return Sound::Place;
        }
        Sound::None
    }

    /// `FUN_0043b040`: the nearest head, by the original's float
    /// distance.
    pub fn nearest(&self, x: i32, y: i32) -> usize {
        let mut best = 2000.0f32;
        let mut k = 0;
        for (i, s) in self.snakes.iter().enumerate() {
            let dy = (y - s.y) as f32;
            let dx = (x - s.x) as f32;
            let dist = f64::from(dy * dy + dx * dx).sqrt() as f32;
            if dist < best {
                k = i;
                best = dist;
            }
        }
        k
    }

    /// `FUN_0043af08`: the click direction from the head (0 offsets count
    /// as 1; the original compares the float dx / dy with +-1.0).
    pub fn direction(mut dx: i32, mut dy: i32) -> usize {
        if dy == 0 {
            dy = 1;
        }
        if dx == 0 {
            dx = 1;
        }
        let closer_x = dx.abs() < dy.abs();
        if dx < 0 && dy < 0 {
            return if closer_x { 2 } else { 1 };
        }
        if dx < 0 {
            return if closer_x { 3 } else { 1 };
        }
        if dy < 0 {
            return if closer_x { 2 } else { 0 };
        }
        if dx.abs() > dy.abs() {
            0
        } else {
            3
        }
    }

    /// `FUN_0043c6bc`: a click of phase 2, the nearest snake steered.
    pub fn steer(&mut self, x: i32, y: i32) -> Sound {
        if at_edge(x) {
            self.result = 1;
        }
        let k = self.nearest(x, y);
        let s = &self.snakes[k];
        let d = Self::direction(x - s.x, y - s.y);
        if self.at(s.x, s.y) == 6 {
            return self.move_snake(k, d);
        }
        // The last code -> the d that undoes it.
        const BACK: [usize; 4] = [3, 0, 1, 2];
        let last = s.segment(usize::from(s.len));
        if last <= 3 && d == BACK[usize::from(last)] {
            return self.move_snake(k, d);
        }
        Sound::None
    }

    /// `FUN_0043bebc`'s state part: the carried piece follows the mouse,
    /// then the win test (every head on its piece; result 255,
    /// `DAT_00496d14`).
    ///
    /// On a first run the snakes not yet released still hold the zeroed
    /// memory (place (0, 0), piece 0), so carrying piece 0 to (0, 0) wins
    /// (the original's behaviour). With `fix_first_run_win` (the minor bug
    /// fixes) the win also needs every snake released, as a real win has.
    pub fn frame(&mut self, mx: i32, my: i32) -> bool {
        if self.carried != 0 {
            let p = &mut self.pieces[(self.carried - 1) as usize];
            (p.x, p.y) = (mx, my);
        }
        if self.fix_first_run_win && self.snakes.iter().any(|s| s.active == 0) {
            return false;
        }
        for s in &self.snakes {
            let p = &self.pieces[usize::from(s.piece & 3)];
            if p.x != s.x || p.y != s.y {
                return false;
            }
        }
        self.result = 255;
        true
    }

    /// `FUN_0043bebc`'s sprite draws on SERPNEW.SPR: the active snakes
    /// (`FUN_0043b49c`), the pieces, the carried piece again.
    pub fn draws(&self) -> Vec<Draw> {
        let mut out = Vec::new();
        for k in 0..NUM_SNAKES {
            if self.snakes[k].active != 0 {
                self.draw_snake(k, &mut out);
            }
        }
        for (k, p) in self.pieces.iter().enumerate() {
            add(&mut out, p.x, p.y, k as i32 * 15);
        }
        if self.carried != 0 {
            let k = (self.carried - 1) as usize;
            add(&mut out, self.pieces[k].x, self.pieces[k].y, k as i32 * 15);
        }
        out
    }

    /// `FUN_0043c114`'s draws for one step: the pieces when shown, then
    /// the active snakes.
    pub fn draws_win(&self, pieces_shown: bool) -> Vec<Draw> {
        let mut out = Vec::new();
        if pieces_shown {
            for (k, p) in self.pieces.iter().enumerate() {
                add(&mut out, p.x, p.y, k as i32 * 15);
            }
        }
        for k in 0..NUM_SNAKES {
            if self.snakes[k].active != 0 {
                self.draw_snake(k, &mut out);
            }
        }
        out
    }

    /// `FUN_0043b49c`: the bodies every 10 pixels, then the nodes (corners
    /// from the previous code), the tail at the start and the head.
    fn draw_snake(&self, k: usize, out: &mut Vec<Draw>) {
        let s = &self.snakes[k];
        let len = usize::from(s.len);
        if len == 0 {
            return;
        }
        let f = k as i32 * 15;
        let (mut x, mut y) = (s.start_x, s.start_y);
        for i in 0..len {
            let code = s.segment(i + 1);
            if code > 3 {
                continue;
            }
            let (mut ex, mut ey) = (x, y);
            self.walk(code, &mut ex, &mut ey);
            let (dx, dy) = step(code);
            let vertical = code == 0 || code == 3;
            let mut n = ((if vertical { ey - y } else { ex - x }).abs() - 10) / 10;
            x += 10 * dx;
            y += 10 * dy;
            if i == 0 {
                x += 10 * dx;
                y += 10 * dy;
                n -= 1;
            }
            if len - 1 == i {
                n -= 1;
            }
            while n > 0 {
                add(out, x, y, f + if vertical { 1 } else { 2 });
                x += 10 * dx;
                y += 10 * dy;
                n -= 1;
            }
            (x, y) = (ex, ey);
        }
        (x, y) = (s.start_x, s.start_y);
        let mut prev = -1;
        for i in 0..len {
            let code = s.segment(i + 1);
            if code > 3 {
                continue;
            }
            let frame = match (code, prev) {
                (0, 1) | (2, 3) => 9,
                (0, 2) | (1, 3) => 10,
                (3, 1) | (2, 0) => 7,
                (3, 2) | (1, 0) => 8,
                (0 | 3, p) if p != -1 => 1,
                (1 | 2, p) if p != -1 => 2,
                _ => -1,
            };
            if frame >= 0 {
                add(out, x, y, f + frame);
            }
            self.walk(code, &mut x, &mut y);
            prev = i32::from(code);
        }
        const TAIL: [i32; 4] = [13, 12, 11, 14];
        if s.dirs[0] <= 3 {
            add(out, s.start_x, s.start_y, f + TAIL[usize::from(s.dirs[0])]);
        }
        match s.segment(len) {
            0 => add(out, s.x, s.y + 5, f + 3),
            1 => add(out, s.x + 5, s.y, f + 5),
            2 => add(out, s.x - 5, s.y, f + 6),
            3 => add(out, s.x, s.y - 5, f + 4),
            _ => {}
        }
    }

    fn draw_list(io: &mut impl SnakesIo, draws: &[Draw]) {
        for d in draws {
            io.draw_sprite('S', d.x, d.y, d.frame);
        }
    }

    /// `FUN_0043c114`: 20 steps of 100 ticks of the 200 Hz timer, the
    /// pieces shown every other step (from the second), drawn before the
    /// snakes; no cursor in the 20 steps (the entry adds it once after
    /// them, see [`SnakesGame::run`]).
    async fn win_animation(&self, io: &mut impl SnakesIo) {
        let mut hidden = true;
        for _ in 0..0x14 {
            io.draw_background();
            Self::draw_list(io, &self.draws_win(!hidden));
            hidden = !hidden;
            io.present(self).await;
            io.wait_ticks(100).await;
            if io.aborted() {
                return;
            }
        }
    }

    /// `FUN_0043b0f4`: phase 1 (`FUN_0043c35c`) until the four pieces are
    /// placed, then phase 2 (`FUN_0043c6bc`); each pass `FUN_0043bebc`,
    /// then the cursor `FUN_0043c7e8` (CURSEURS frame 11 near the edges,
    /// else frame 6). A leave click ends phase 2 before the frame (the
    /// result test at 0x0043b392 follows `FUN_0043c6bc`), phase 1 after
    /// it: its loop draws, presents and polls the input (0x0043b344)
    /// before testing the result at 0x0043b34e, and leaves through the
    /// phase-2 loop head (0x0043b384), which runs `FUN_0043c6bc` on a press
    /// taken by that poll before the result is returned (0x0043b392). A
    /// win (`FUN_0043c114` inside `FUN_0043bebc`) is followed by the cursor
    /// and one more present in both loops. 255 solved, 1 left.
    pub async fn run(&mut self, io: &mut impl SnakesIo) -> i32 {
        while self.result == 0 {
            let (mx, my, clicked) = io.read_input();
            let phase1 = !self.all_placed();
            if clicked {
                let snd = if phase1 {
                    self.place(mx, my)
                } else {
                    self.steer(mx, my)
                };
                if snd != Sound::None {
                    io.play_sound(snd);
                }
            }
            if self.result != 0 && !phase1 {
                break;
            }
            if self.frame(mx, my) {
                io.play_sound(Sound::Win);
                self.win_animation(io).await;
                // FUN_0043bebc returns after FUN_0043c114 and the entry
                // goes on with the cursor (FUN_0043c7e8, 0x0043b28c /
                // 0x0043b3b2) drawn onto the 20th step still in the back
                // buffer, then presents it (0x0043b33f / 0x0043b465) before
                // its result test. Not the pass's drawing: the 20th step
                // draws the pieces before the snakes.
            } else {
                io.draw_background();
                Self::draw_list(io, &self.draws());
            }
            io.draw_sprite('C', mx, my, if at_edge(mx) { 11 } else { 6 });
            io.present(self).await;
            if self.result != 0 {
                if phase1 {
                    // The poll at 0x0043b344 precedes the test at
                    // 0x0043b34e, whose exit goes through the phase-2 loop
                    // head: a new press is run by FUN_0043c6bc
                    // (0x0043b384..0x0043b38d) before the result is
                    // returned (0x0043b392).
                    let (mx, my, clicked) = io.read_input();
                    if clicked {
                        let snd = self.steer(mx, my);
                        if snd != Sound::None {
                            io.play_sound(snd);
                        }
                    }
                }
                break;
            }
            if io.aborted() {
                break;
            }
        }
        if self.result != 0 {
            self.result
        } else {
            1
        }
    }
}

fn add(out: &mut Vec<Draw>, x: i32, y: i32, frame: i32) {
    // The original's draw list keeps 16-bit positions.
    out.push(Draw {
        x: i32::from(x as i16),
        y: i32::from(y as i16),
        frame,
    });
}

struct Host<'a>(PuzzleHost<'a>);

impl SnakesIo for Host<'_> {
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
    async fn present(&mut self, _g: &SnakesGame) {
        self.0.present().await;
    }
    fn play_sound(&mut self, s: Sound) {
        let name = match s {
            Sound::None => return,
            Sound::Pick => "PZS1_1",
            Sound::Place => "PZS1_2",
            Sound::Move => "PZS1_3",
            Sound::Win => "PZS1_5",
        };
        self.0.play_sound(name, SamplePlayer::Ca28);
    }
    async fn wait_ticks(&mut self, n: u32) {
        self.0.wait_ticks(n).await;
    }
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

/// StartPuzzle 8: 255 when solved, 1 when the player leaves or the game is
/// quit. Here only: 1 when a file is missing; the entry's one load test
/// (`FUN_0043a53c`, 0x0043b200..0x0043b210) never fails, `FUN_0043a53c`
/// always returning 1.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let (Some(bg), Some(frames), Some(pcx)) = (
        load_tga(e, "LABBASE4.TGA").await,
        load_spr(e, "SERPNEW.SPR").await,
        read_puzzle_file(e, "CHEMIN00.PCX").await,
    ) else {
        log::warn!("snakes: missing files");
        return 1;
    };
    let Some(map) = decode_map(&pcx) else {
        log::warn!("snakes: bad CHEMIN00.PCX");
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &["PZS1_1", "PZS1_2", "PZS1_3", "PZS1_4", "PZS1_5"]).await;
    // FUN_0043b0f4 does not clear the snakes' places and pieces: they
    // carry over from the last run, as in the original's memory.
    let last = e.puzzles.snakes.unwrap_or_default();
    let mut game = SnakesGame::new(map, last);
    game.fix_first_run_win = e.enhancements().minor_bug_fixes;
    let mut host = Host(PuzzleHost::new(e, hooks));
    host.0.screen.pictures = vec![bg];
    host.0.screen.frames = frames;
    host.0.sounds = sounds;
    let r = game.run(&mut host).await;
    let e = host.0.e;
    e.puzzles.snakes = Some(game.snakes);
    if e.should_abort() {
        1
    } else {
        r
    }
}

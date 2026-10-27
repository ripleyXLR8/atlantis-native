//! StartPuzzle 13 at CHAPI060: the hexagon (`FUN_004404fc`). Three pieces
//! dropped on a class map (puzhexam.bmp: 0 off the board, 1 the board,
//! 0x1f the frame), turned in sixths; the puzzle is won with the pieces
//! turned 5, 3, 5 and laid out inside a box, piece 1 leftmost, piece 0 at
//! the top, piece 2 at the bottom. The engine of the metal pieces
//! ([`super::meta`]), with its own drop test, no board turn and its own
//! sounds.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`hexa_game.cpp`,
//! `puzzle_hexa.cpp`; GPLv3, the ScummVM team).

use super::meta::{MetaGame, DROP_DX, DROP_DY};
use super::{
    load_class_map, load_spr, load_tga, read_coverage, read_puzzle_file, Coverage, PuzzleCursor,
    PuzzleHost, PuzzleSounds, SamplePlayer, MAP_SIZE,
};
use crate::engine::Engine;
use crate::enhancements::BugFix;
use crate::hooks::ScriptHooks;

/// A piece record (0x004971ce, 12 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub base: i16,
    pub cls: i16,
    pub orient: i16,
    pub x: i16,
    pub y: i16,
    pub unk_a: i16,
}

pub const NUM_PIECES: usize = 3;

const fn piece(base: i16, x: i16, y: i16, unk_a: i16) -> Piece {
    Piece {
        base,
        cls: 0,
        orient: 0,
        x,
        y,
        unk_a,
    }
}

pub const PIECES: [Piece; NUM_PIECES] = [
    piece(0, 60, 50, 2),
    piece(6, 110, 50, 3),
    piece(12, 190, 50, 1),
];
/// The start table (0x004971f0): class, orientation, x, y.
const START: [[i16; 4]; NUM_PIECES] = [[1, 4, 220, 300], [1, 4, 309, 120], [1, 2, 400, 300]];

/// The game's I/O: the C++ `HexaGame` virtual functions.
#[allow(async_fn_in_trait)]
pub trait HexaIo {
    /// The input when the puzzle starts: mouse, a new click, the right
    /// button down.
    fn start_input(&mut self) -> (i32, i32, bool, bool);
    /// `FUN_0043318c`: a frame boundary.
    fn boundary(&mut self, g: &HexaGame) -> (i32, i32, bool, bool);
    fn draw_background(&mut self);
    /// 'P' pieces, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    /// `FUN_0042d558`.
    fn draw_standard_cursor(&mut self, x: i32, y: i32);
    async fn present(&mut self, g: &HexaGame);
    /// hexa1..hexa5 = 0..4.
    fn play_sound(&mut self, slot: i32);
    fn aborted(&self) -> bool {
        false
    }
}

/// The hexagon's state.
#[derive(Clone, Debug)]
pub struct HexaGame {
    pub pieces: [Piece; NUM_PIECES],
    /// 0x007bebe8..0x007bebf8.
    pub grab_x: i32,
    pub grab_y: i32,
    pub repeat: i32,
    pub unk_f4: i32,
    pub held: i32,
    pub cursor: PuzzleCursor,
    /// OBJETENMAIN.
    pub object_in_hand: i32,
    /// FLAGTEMP6.
    pub flag_temp6: i32,
    /// The 2001 entry's OBJETENMAIN = 402 before the test (0x0044076e).
    /// Without it (the 1997 build, atlantis-cd-en-688128 FUN_0043c570) the
    /// pieces are placed only when the player enters holding object 402
    /// or with FLAGTEMP6 = 2.
    pub force_held_402: bool,
    pub result: i32,
    pub map: Vec<u8>,
    pub map_file: Vec<u8>,
    pub coverage: Vec<Coverage>,
    mx: i32,
    my: i32,
    click: bool,
    right: bool,
}

impl HexaGame {
    pub fn new(map_file: Vec<u8>, coverage: Vec<Coverage>) -> HexaGame {
        HexaGame {
            pieces: PIECES,
            grab_x: 0,
            grab_y: 0,
            repeat: 0,
            unk_f4: 0,
            held: 0,
            cursor: PuzzleCursor::default(),
            object_in_hand: 0,
            flag_temp6: 0,
            force_held_402: true,
            result: 0,
            map: Vec::new(),
            map_file,
            coverage,
            mx: 0,
            my: 0,
            click: false,
            right: false,
        }
    }

    fn read_boundary(&mut self, io: &mut impl HexaIo) {
        let (mx, my, c, r) = io.boundary(self);
        self.mx = mx;
        self.my = my;
        if c {
            self.click = true;
        }
        if r {
            self.right = true;
        }
    }

    /// `FUN_00440efc`: piece `i` dropped at (x, y), true when it lies
    /// there.
    pub fn place_piece(&mut self, i: usize, x: i32, y: i32) -> bool {
        let pc = self.pieces[i];
        let cells = MetaGame::cells(&self.coverage, i32::from(pc.base + pc.orient), x, y);
        let mut hist = [0u16; 32];
        for &k in &cells {
            hist[usize::from(self.map[k] & 0x1f)] += 1;
        }
        let mut best: i16 = 0;
        let mut m = 0usize;
        for (c, &h) in hist.iter().enumerate() {
            if best < h as i16 {
                best = h as i16;
                m = c;
            }
        }
        // s1 and h0 are read as dwords (hist[1] | hist[2] << 16, hist[0] |
        // hist[1] << 16); only their low words count below.
        let s1 = (u32::from(hist[1]) | (u32::from(hist[2]) << 16)) as i32;
        let s2: i32 = hist[10..0x13].iter().map(|&h| i32::from(h)).sum();
        let h0 = (u32::from(hist[0]) | (u32::from(hist[1]) << 16)) as i32;
        let t = s1.wrapping_add(h0).wrapping_add(s2) as i16;
        let (p0, p1, p2) = if t != 0 {
            let t = i32::from(t);
            let mut p0 = i32::from(h0 as i16) * 100 / t;
            let p1 = i32::from(s1 as i16) * 100 / t;
            let mut p2 = i32::from(s2 as i16) * 100 / t;
            // A single pixel off the board or on another piece counts.
            if h0 as i16 > 0 && p0 == 0 {
                p0 = 1;
            }
            if s2 as i16 > 0 && p2 == 0 {
                p2 = 1;
            }
            (p0, p1, p2)
        } else {
            (0, 0, 100)
        };
        let on_board = p0 == 0 && p2 == 0;
        if !(on_board || p1 + p2 == 0) {
            return false;
        }
        let mark = 10 + (i as u8 & 7);
        for &k in &cells {
            let b = &mut self.map[k];
            if if on_board {
                b.wrapping_sub(1) < 4
            } else {
                *b == 0
            } {
                *b = mark;
            }
        }
        let pc = &mut self.pieces[i];
        pc.cls = m as i16;
        pc.x = x as i16;
        pc.y = y as i16;
        true
    }

    /// `FUN_00441210`.
    pub fn won(&self) -> bool {
        let [a, b, c] = &self.pieces;
        if !(a.orient == 5 && b.orient == 3 && c.orient == 5) {
            return false;
        }
        let inside = |p: &Piece| (0xdc..=0x190).contains(&p.x) && (0xa0..=0x14a).contains(&p.y);
        if !(inside(a) && inside(b) && inside(c)) {
            return false;
        }
        if b.x > a.x || b.x > c.x {
            return false;
        }
        if a.y > b.y || a.y > c.y {
            return false;
        }
        !(c.y < a.y || c.y < b.y)
    }

    /// `FUN_004406fc`.
    fn set_up(&mut self) {
        // The entry sets OBJETENMAIN to 0x192 and then tests it, so the
        // pieces are always there and FlagTemp6 is always 1. The 1997
        // build tests the object in hand as it is: no pieces (class -1)
        // and FlagTemp6 left as it was, unless the player holds 402 or
        // FlagTemp6 is 2 (atlantis-cd-en-688128 FUN_0043c570).
        let mut count = 0;
        if self.force_held_402 {
            self.object_in_hand = 0x192;
        }
        if self.object_in_hand == 0x192 {
            count += 1;
            self.flag_temp6 = 1;
        }
        if self.flag_temp6 == 2 {
            count += 1;
        }
        for (i, s) in START.iter().enumerate() {
            self.pieces[i].orient = s[1];
            self.pieces[i].cls = if count != 0 { s[0] } else { -1 };
            if self.pieces[i].cls >= 0 {
                self.place_piece(i, s[2].into(), s[3].into());
            }
        }
        self.object_in_hand = 0;
    }

    /// `FUN_004414fc`.
    fn refuse(&mut self, io: &mut impl HexaIo) {
        if self.cursor.refuse() {
            io.play_sound(3);
        }
    }

    /// `FUN_00441184`.
    fn drop_piece(&mut self, i: usize, x: i32, y: i32) -> bool {
        (0..17).any(|k| self.place_piece(i, x + i32::from(DROP_DX[k]), y + i32::from(DROP_DY[k])))
    }

    /// `FUN_00440bfc`, on the right flag of `FUN_0041a298` (one pass per
    /// press, so the repeat counter never passes 1).
    fn right_button(&mut self, io: &mut impl HexaIo) {
        if self.right {
            if self.repeat == 0 && self.held != 0 {
                let pc = &mut self.pieces[(self.held - 1) as usize];
                self.grab_y = 0;
                self.grab_x = 0;
                pc.orient += 1;
                if pc.orient >= 6 {
                    pc.orient = 0;
                }
                io.play_sound(0);
            }
            self.repeat += 1;
            if self.repeat > 0x32 {
                self.repeat = 0;
            }
            self.right = false;
        } else {
            self.repeat = 0;
        }
    }

    fn map_at(&self, x: i32, y: i32) -> u8 {
        self.map[((y * 640 + x) as u32 % MAP_SIZE as u32) as usize]
    }

    /// `FUN_00441390`.
    fn left_click(&mut self, io: &mut impl HexaIo) {
        if self.held != 0 {
            let p = (self.held - 1) as usize;
            let (x, y) = (self.mx + self.grab_x, self.my + self.grab_y);
            let ok = self.drop_piece(p, x, y);
            log::debug!(
                "hexa: drop piece {p} orientation {} at {x},{y} (mouse {},{}) {}",
                self.pieces[p].orient,
                self.mx,
                self.my,
                if ok { "accepted" } else { "refused" }
            );
            if ok {
                self.held = 0;
                io.play_sound(2);
                if self.won() {
                    self.result = 1;
                }
            } else {
                self.refuse(io);
            }
            return;
        }
        let b = self.map_at(self.mx, self.my);
        if (0xa..=0x10).contains(&b) {
            let p = usize::from(b - 0xa);
            self.held = p as i32 + 1;
            self.grab_x = i32::from(self.pieces[p].x) - self.mx;
            self.grab_y = i32::from(self.pieces[p].y) - self.my;
            let cls = self.pieces[p].cls as u8;
            for v in self.map.iter_mut() {
                if *v == b {
                    *v = cls;
                }
            }
            io.play_sound(1);
        } else if b == 0x1f {
            self.result = 2;
        } else {
            self.refuse(io);
        }
    }

    /// `FUN_00440aa8`.
    fn draw(&self, io: &mut impl HexaIo) {
        io.draw_background();
        for (i, p) in self.pieces.iter().enumerate() {
            if self.held - 1 != i as i32 && p.cls >= 0 {
                io.draw_sprite('P', p.x.into(), p.y.into(), i32::from(p.base + p.orient));
            }
        }
    }

    /// `FUN_00440cb8`.
    fn draw_cursor(&mut self, io: &mut impl HexaIo) {
        if self.held != 0 {
            let pc = self.pieces[(self.held - 1) as usize];
            io.draw_sprite(
                'P',
                i32::from((self.mx + self.grab_x) as i16),
                i32::from((self.my + self.grab_y) as i16),
                i32::from(pc.base + pc.orient),
            );
        }
        if self.cursor.bouncing() {
            let f = self.cursor.bounce();
            io.draw_sprite('C', self.mx, self.my, f.into());
            return;
        }
        if self.held != 0 {
            io.draw_standard_cursor(self.mx, self.my);
            return;
        }
        let b = self.map_at(self.mx, self.my);
        io.draw_sprite('C', self.mx, self.my, if b == 0x1f { 10 } else { 5 });
    }

    async fn frame(&mut self, io: &mut impl HexaIo) {
        self.draw(io);
        io.draw_background();
        io.present(self).await;
        self.click = false;
        self.read_boundary(io);
    }

    /// `FUN_00440898`.
    async fn win_animation(&mut self, io: &mut impl HexaIo) {
        io.play_sound(4);
        for _ in 0..0x10 {
            for i in 0..NUM_PIECES {
                self.pieces[i].cls = -1;
                self.frame(io).await;
                self.pieces[i].cls = 1;
                self.frame(io).await;
            }
        }
    }

    /// `FUN_00448ad0` then `FUN_004404fc`: 255 won, 2 left.
    pub async fn run(&mut self, io: &mut impl HexaIo) -> i32 {
        (self.mx, self.my, self.click, self.right) = io.start_input();
        self.map = self.map_file.clone();
        self.set_up();
        self.result = 0;
        loop {
            self.read_boundary(io);
            self.right_button(io);
            if self.click {
                self.left_click(io);
            }
            self.draw(io);
            self.draw_cursor(io);
            io.draw_background();
            io.present(self).await;
            self.click = false;
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

impl HexaIo for Host<'_> {
    fn start_input(&mut self) -> (i32, i32, bool, bool) {
        let m = self.0.read_input();
        (m.x, m.y, m.click, m.right)
    }
    fn boundary(&mut self, _g: &HexaGame) -> (i32, i32, bool, bool) {
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
    async fn present(&mut self, _g: &HexaGame) {
        self.0.present().await;
    }
    fn play_sound(&mut self, slot: i32) {
        const NAMES: [&str; 5] = ["HEXA1", "HEXA2", "HEXA3", "HEXA4", "HEXA5"];
        if let Some(n) = usize::try_from(slot).ok().and_then(|s| NAMES.get(s)) {
            self.0.play_sound(n, SamplePlayer::Ca28);
        }
    }
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

/// StartPuzzle 13: 255 when solved, 2 when the player leaves, 1 when a
/// file is missing or the game is quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let frames = load_spr(e, "PUZHEXA.SPR").await;
    let bg = load_tga(e, "PUZHEXA.TGA").await;
    let coverage = match read_puzzle_file(e, "PUZHEXA.SPR").await {
        Some(d) => read_coverage(&d),
        None => None,
    };
    let map = load_class_map(e, "PUZHEXAM.BMP").await;
    let (Some(frames), Some(bg), Some(coverage), Some(map)) = (frames, bg, coverage, map) else {
        log::warn!("hexa: missing files");
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &["HEXA1", "HEXA2", "HEXA3", "HEXA4", "HEXA5"]).await;
    let mut game = HexaGame::new(map, coverage);
    game.cursor = e.puzzles.cursor;
    // The 1997 build's entry rule; the hexagon pieces' fix keeps the 2001
    // forcing, so that the pieces are always there.
    game.force_held_402 =
        e.game.exe.build.hexa_forces_402 || e.enhancements().fix(BugFix::HexagonPieces);
    game.object_in_hand = e.vars.get("objetenmain");
    game.flag_temp6 = e.vars.get("flagtemp6");
    let mut host = Host(PuzzleHost::new(e, hooks));
    host.0.screen.pictures = vec![bg];
    host.0.screen.frames = frames;
    host.0.sounds = sounds;
    let r = game.run(&mut host).await;
    let e = host.0.e;
    e.puzzles.cursor = game.cursor;
    // FUN_004406fc set OBJETENMAIN to 0 and FlagTemp6 to 1 (the 1997
    // build: to 1 when 402 was in hand, else left).
    e.vars.set("objetenmain", 0);
    e.vars.set("flagtemp6", game.flag_temp6);
    if e.should_abort() {
        1
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_up(force: bool, held: i32, flag: i32) -> (Vec<i16>, i32, i32) {
        let mut g = HexaGame::new(vec![0; 640 * 480], Vec::new());
        g.map = g.map_file.clone();
        g.force_held_402 = force;
        g.object_in_hand = held;
        g.flag_temp6 = flag;
        g.set_up();
        (
            g.pieces.iter().map(|p| p.cls).collect(),
            g.flag_temp6,
            g.object_in_hand,
        )
    }

    /// FUN_004406fc places the three pieces whatever the player holds;
    /// atlantis-cd-en-688128 FUN_0043c570 only with 402 in hand or
    /// FLAGTEMP6 = 2, and leaves FLAGTEMP6 otherwise.
    #[test]
    fn pieces_rule_of_each_build() {
        let all = vec![1, 1, 1];
        let none = vec![-1, -1, -1];
        assert_eq!(set_up(true, 0, 0), (all.clone(), 1, 0));
        assert_eq!(set_up(false, 0, 0), (none, 0, 0));
        assert_eq!(set_up(false, 0x192, 0), (all.clone(), 1, 0));
        assert_eq!(set_up(false, 0, 2), (all.clone(), 2, 0));
        assert_eq!(set_up(false, 0x101, 2), (all, 2, 0));
    }
}

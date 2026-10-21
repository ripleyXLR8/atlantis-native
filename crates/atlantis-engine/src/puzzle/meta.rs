//! StartPuzzle 9..12 at CHAPI046: the metal pieces (`FUN_0044763c` with
//! variant 1..4), the four stages of one board. Pieces are dropped on an
//! 8-bit class map (pzmetam.bmp: 0 the centre, 1..4 the zones, 0x1f the
//! frame, 10..16 the pieces' marks); stage v is won with its first v + 3
//! pieces in zone v. The pieces live on between the stages, as in the
//! original's memory ([`MetaMemory`]).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`meta_game.cpp`,
//! `puzzle_meta.cpp`; GPLv3, the ScummVM team).

use super::{
    load_class_map, load_spr, load_tga, read_coverage, read_puzzle_file, Coverage, PuzzleCursor,
    PuzzleHost, PuzzleSounds, SamplePlayer, MAP_SIZE,
};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;

/// A piece record (0x00496fe0, 14 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    /// The first PZMETAP1 frame of its 8 orientations.
    pub base: i16,
    pub unk2: i16,
    /// The map class it lies on, -1 when absent.
    pub cls: i16,
    /// 0..7.
    pub orient: i16,
    pub x: i16,
    pub y: i16,
    pub unk_c: i16,
}

const fn piece(base: i16, unk2: i16, x: i16, y: i16, unk_c: i16) -> Piece {
    Piece {
        base,
        unk2,
        cls: 0,
        orient: 0,
        x,
        y,
        unk_c,
    }
}

pub const NUM_PIECES: usize = 7;

/// The piece records as in the original's data (0x00496fe0).
pub const PIECES: [Piece; NUM_PIECES] = [
    piece(0, 15, 60, 50, 2),
    piece(8, 15, 110, 50, 3),
    piece(16, 19, 190, 50, 4),
    piece(24, 23, 300, 50, 6),
    piece(40, 15, 400, 50, 5),
    piece(32, 15, 450, 50, 4),
    piece(24, 23, 530, 50, 1),
];

/// The stage tables (0x00497040 + 0x38 * (v - 1)): class, orientation, x,
/// y.
const STAGES: [[[i16; 4]; NUM_PIECES]; 4] = [
    [
        [1, 4, 317, 391],
        [0, 4, 321, 207],
        [0, 2, 291, 242],
        [0, 6, 351, 256],
        [-1, 0, 226, 343],
        [-1, 0, 421, 364],
        [-1, 0, 257, 143],
    ],
    [
        [1, 4, 317, 391],
        [1, 6, 334, 407],
        [1, 5, 366, 421],
        [1, 0, 286, 409],
        [0, 0, 320, 240],
        [-1, 0, 421, 364],
        [-1, 0, 257, 143],
    ],
    [
        [2, 7, 460, 293],
        [2, 3, 440, 225],
        [2, 4, 492, 259],
        [2, 5, 480, 212],
        [2, 1, 447, 259],
        [0, 0, 320, 240],
        [-1, 0, 257, 143],
    ],
    [
        [3, 4, 318, 138],
        [3, 6, 367, 58],
        [3, 5, 304, 106],
        [3, 4, 317, 57],
        [3, 2, 271, 76],
        [3, 1, 350, 90],
        [0, 0, 320, 240],
    ],
];

/// The turn centres by picture (x: shorts from 0x00497120, y: from
/// 0x0049712a).
const CENTRE_X: [i16; 5] = [0, 319, 320, 320, 319];
const CENTRE_Y: [i16; 5] = [0, 239, 239, 240, 240];
/// pzmetam.bmp holds two bytes after its 640x480 map (0x436 + 0x4b000 + 2
/// = 0x4b438 bytes, the size `FUN_004472cc` allocates).
const MAP_FILE_TAIL: u32 = 2;
/// The drop offsets (shorts from 0x00497134 and 0x00497158).
pub(crate) const DROP_DX: [i16; 17] = [0, -1, 0, 0, 1, -1, -1, 1, 1, -2, 2, 0, 0, -2, 2, 2, -2];
pub(crate) const DROP_DY: [i16; 17] = [0, 0, 1, -1, 0, 1, -1, 1, -1, 0, 0, 2, -2, 2, -2, 2, -2];

/// The objects brought for stages 1..4: 0x13c + v.
pub const OBJECT_BASE: i32 = 0x13c;

/// The game's I/O: the C++ `MetaGame` virtual functions.
#[allow(async_fn_in_trait)]
pub trait MetaIo {
    /// The input when the puzzle starts: mouse, a new click, the right
    /// button down.
    fn start_input(&mut self) -> (i32, i32, bool, bool);
    /// `FUN_0043318c`: a frame boundary.
    fn boundary(&mut self, g: &MetaGame) -> (i32, i32, bool, bool);
    /// `FUN_00447834`'s picture 1..4.
    fn load_picture(&mut self, n: i32);
    fn draw_background(&mut self);
    /// 'P' pieces, 'C' CURSEURS.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    /// `FUN_0042d558`.
    fn draw_standard_cursor(&mut self, x: i32, y: i32);
    async fn present(&mut self, g: &MetaGame);
    /// meta1..meta5 = 0..4.
    fn play_sound(&mut self, slot: i32);
    /// 0x006cd7c0.
    fn current_place(&mut self) -> i32;
    fn aborted(&self) -> bool {
        false
    }
}

/// What the original keeps between the stages: the pieces, their count,
/// the picture and the grab offset. The hand, 0x007bebf4 and the repeat
/// counter are not kept: `FUN_0043a1e0` clears 0x007bebf8 / 0x007bebf4 /
/// 0x007bebf0 before every StartPuzzle (0x0043a206..0x0043a21a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaMemory {
    pub pieces: [Piece; NUM_PIECES],
    pub count: i16,
    pub picture: i16,
    pub grab_x: i32,
    pub grab_y: i32,
}

/// The metal pieces' state.
#[derive(Clone, Debug)]
pub struct MetaGame {
    pub pieces: [Piece; NUM_PIECES],
    /// 0x00496fdc: pieces present.
    pub count: i16,
    /// 0x007bec1e: 1..4.
    pub picture: i16,
    /// 0x007bec1c.
    pub variant: i16,
    /// 0x007bebe8, 0x007bebec.
    pub grab_x: i32,
    pub grab_y: i32,
    /// 0x007bebf0: the right button's repeat counter (0 or 1: one right
    /// press per press, [`crate::puzzle::right_press`]).
    pub repeat: i32,
    /// 0x007bebf4.
    pub unk_f4: i32,
    /// 0x007bebf8: piece + 1, 0 when none.
    pub held: i32,
    pub cursor: PuzzleCursor,
    /// OBJETENMAIN.
    pub object_in_hand: i32,
    /// The variable named at 0x004be584 (FLAGTEMP6).
    pub brought_flag: i32,
    pub result: i32,
    /// The class map.
    pub map: Vec<u8>,
    /// The class map as the file holds it (`FUN_004472cc` reloads it).
    pub map_file: Vec<u8>,
    /// Per frame of pzmetap1.spr, the pixels it covers.
    pub coverage: Vec<Coverage>,
    mx: i32,
    my: i32,
    click: bool,
    right: bool,
}

impl MetaGame {
    pub fn new(map_file: Vec<u8>, coverage: Vec<Coverage>) -> MetaGame {
        MetaGame {
            pieces: PIECES,
            count: 0,
            picture: 0,
            variant: 0,
            grab_x: 0,
            grab_y: 0,
            repeat: 0,
            unk_f4: 0,
            held: 0,
            cursor: PuzzleCursor::default(),
            object_in_hand: 0,
            brought_flag: 0,
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

    pub fn memory(&self) -> MetaMemory {
        MetaMemory {
            pieces: self.pieces,
            count: self.count,
            picture: self.picture,
            grab_x: self.grab_x,
            grab_y: self.grab_y,
        }
    }

    pub fn restore(&mut self, m: &MetaMemory) {
        self.pieces = m.pieces;
        self.count = m.count;
        self.picture = m.picture;
        self.grab_x = m.grab_x;
        self.grab_y = m.grab_y;
    }

    fn read_boundary(&mut self, io: &mut impl MetaIo) {
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

    /// `FUN_00447834`.
    fn set_picture(&mut self, io: &mut impl MetaIo, mut n: i32) {
        if n > 4 {
            n = 1;
        }
        self.picture = n as i16;
        io.load_picture(n);
    }

    /// The covered map cells of frame `f` drawn at (x, y), clipped to the
    /// screen.
    pub(crate) fn cells(coverage: &[Coverage], f: i32, x: i32, y: i32) -> Vec<usize> {
        let Some(c) = usize::try_from(f).ok().and_then(|f| coverage.get(f)) else {
            return Vec::new();
        };
        c.dx.iter()
            .zip(&c.dy)
            .filter_map(|(&dx, &dy)| {
                let (px, py) = (x + i32::from(dx), y + i32::from(dy));
                ((0..640).contains(&px) && (0..480).contains(&py))
                    .then_some((py * 640 + px) as usize)
            })
            .collect()
    }

    /// `FUN_00448404`: piece `i` dropped at (x, y), true when it lies
    /// there.
    pub fn place_piece(&mut self, i: usize, x: i32, y: i32) -> bool {
        let pc = self.pieces[i];
        let cells = Self::cells(&self.coverage, i32::from(pc.base + pc.orient), x, y);
        let mut hist = [0u16; 32];
        for &k in &cells {
            hist[usize::from(self.map[k] & 0x1f)] += 1;
        }
        // The class with the largest count (the first, signed 16-bit
        // compare).
        let mut best: i16 = 0;
        let mut m = 0usize;
        for (c, &h) in hist.iter().enumerate() {
            if best < h as i16 {
                best = h as i16;
                m = c;
            }
        }
        let s1: i32 = hist[1..5].iter().map(|&h| i32::from(h)).sum();
        let mut s2: i32 = hist[10..0x13].iter().map(|&h| i32::from(h)).sum();
        // The dword reads of hist[31] and hist[0] also take the next word
        // (m, hist[1]); only the low 16 bits are used below.
        s2 = s2.wrapping_add((u32::from(hist[31]) | ((m as u32 & 0xffff) << 16)) as i32);
        let h0 = (u32::from(hist[0]) | (u32::from(hist[1]) << 16)) as i32;
        let t = s1.wrapping_add(h0).wrapping_add(s2) as i16;
        let (p0, p1, p2) = if t != 0 {
            let t = i32::from(t);
            (
                i32::from(h0 as i16) * 100 / t,
                i32::from(s1 as i16) * 100 / t,
                i32::from(s2 as i16) * 100 / t,
            )
        } else {
            (0, 0, 100)
        };
        let in_zone = p0 == 0 && p2 < 3;
        if !(in_zone || p1 + p2 == 0) {
            return false;
        }
        // FUN_00410e35: the piece's mark, on the zone (1..4) or on the
        // centre (0).
        let mark = 10 + (i as u8 & 7);
        for &k in &cells {
            let b = &mut self.map[k];
            if if in_zone {
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

    /// `FUN_00448158`: the board turned a quarter, to the next picture when
    /// `advance`.
    pub fn turn_board(&mut self, io: &mut impl MetaIo, advance: bool) {
        let old = self.picture as usize;
        if advance {
            self.set_picture(io, i32::from(self.picture) + 1);
        }
        let nw = self.picture as usize;
        let offset = (i32::from(CENTRE_X[nw]) + i32::from(CENTRE_Y[old]) - 0x50) * 0x280
            + (i32::from(CENTRE_X[old]) + 1 - i32::from(CENTRE_Y[nw]));
        let mut work = vec![0x1fu8; MAP_SIZE];
        let size = MAP_SIZE as u32;
        let mut o: u32 = 640;
        for src in offset..offset + 0x1de {
            o += 80;
            let mut s = src;
            for _ in 0..480 {
                let mut v = 0u8;
                if s >= 0 && (s as u32) < size {
                    v = self.map[s as usize];
                } else if s as u32 >= size + MAP_FILE_TAIL {
                    // Past the map's end: the original's pool
                    // (FUN_004475b4, a bump allocator) puts the work buffer
                    // (0x96000 bytes, FUN_0044763c @ 0x00447688) right after
                    // pzmetam.bmp (0x4b438 bytes, FUN_004472cc), so these
                    // reads take the work bytes written so far. Only a turn
                    // in place of picture 3 (the set-up of a place % 10 ==
                    // 5) reads there: s = MAP_SIZE + 81..557, in the work
                    // buffer's first row of 0x1f. Every turn reads s >= 80
                    // (no read before the map).
                    let w = (s as u32).wrapping_sub(size + MAP_FILE_TAIL);
                    if w < o {
                        v = work[w as usize];
                    }
                }
                work[o as usize] = v;
                o += 1;
                s -= 640;
            }
            o += 80;
        }
        self.map = work;
        for pc in self.pieces.iter_mut() {
            if pc.cls < 0 {
                continue;
            }
            pc.orient += 2;
            if pc.orient >= 8 {
                pc.orient -= 8;
            }
            let dx = pc.x - CENTRE_X[old];
            let dy = pc.y - CENTRE_Y[old];
            pc.x = CENTRE_X[nw] - dy;
            pc.y = CENTRE_Y[nw] + dx;
        }
    }

    /// `FUN_004478d4`.
    fn set_up(&mut self, io: &mut impl MetaIo) {
        if self.object_in_hand == OBJECT_BASE + i32::from(self.variant) {
            self.count += 1;
            self.brought_flag = 1;
        }
        self.object_in_hand = 0;
        let pic = match io.current_place() % 10 {
            5 => 3,
            6 => 4,
            8 => 2,
            _ => 1,
        };
        if self.variant == 1 {
            self.count = 4;
        }
        for (i, e) in STAGES[(self.variant - 1) as usize].into_iter().enumerate() {
            self.pieces[i].orient = e[1];
            self.pieces[i].cls = if i32::from(self.count) > i as i32 {
                e[0]
            } else {
                -1
            };
            if self.pieces[i].cls >= 0 {
                self.place_piece(i, e[2].into(), e[3].into());
            }
        }
        self.set_picture(io, pic);
        for _ in 1..pic {
            self.turn_board(io, false);
        }
    }

    /// `FUN_00448a80`.
    fn refuse(&mut self, io: &mut impl MetaIo) {
        if self.cursor.refuse() {
            io.play_sound(4);
        }
    }

    /// `FUN_00448688`: the drop at (x, y) or one of the 16 points around.
    pub(crate) fn drop_piece(&mut self, i: usize, x: i32, y: i32) -> bool {
        (0..17).any(|k| self.place_piece(i, x + i32::from(DROP_DX[k]), y + i32::from(DROP_DY[k])))
    }

    /// `FUN_00448714`.
    pub fn won(&self) -> bool {
        let n = (self.variant + 3) as usize;
        self.pieces[..n.min(NUM_PIECES)]
            .iter()
            .all(|p| p.cls == self.variant)
    }

    /// `FUN_00448084`, on the right flag of `FUN_0041a298` (one pass per
    /// press, so the repeat counter never passes 1).
    fn right_button(&mut self, io: &mut impl MetaIo) {
        if self.right {
            if self.repeat == 0 {
                if self.held != 0 {
                    let pc = &mut self.pieces[(self.held - 1) as usize];
                    self.grab_y = 0;
                    self.grab_x = 0;
                    pc.orient += 1;
                    if pc.orient >= 8 {
                        pc.orient = 0;
                    }
                } else {
                    self.turn_board(io, true);
                }
                io.play_sound(0);
            }
            self.repeat += 1;
            if self.repeat > 0x32 {
                self.repeat = 0;
            }
        } else {
            self.repeat = 0;
        }
        self.right = false;
    }

    fn map_at(&self, x: i32, y: i32) -> u8 {
        self.map[((y * 640 + x) as u32 % MAP_SIZE as u32) as usize]
    }

    /// `FUN_004487ec`.
    fn left_click(&mut self, io: &mut impl MetaIo) {
        if self.held != 0 {
            let p = (self.held - 1) as usize;
            if self.drop_piece(p, self.mx + self.grab_x, self.my + self.grab_y) {
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
            // FUN_00410e10: the mark goes back to the piece's class.
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

    /// `FUN_00447cec`.
    fn draw(&self, io: &mut impl MetaIo) {
        io.draw_background();
        for (i, p) in self.pieces.iter().enumerate() {
            if self.held - 1 != i as i32 && p.cls >= 0 {
                io.draw_sprite('P', p.x.into(), p.y.into(), i32::from(p.base + p.orient));
            }
        }
    }

    /// `FUN_00447e40`, `FUN_00448958`.
    fn draw_cursor(&mut self, io: &mut impl MetaIo) {
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

    /// A pass of the win animation.
    async fn frame(&mut self, io: &mut impl MetaIo) {
        self.draw(io);
        io.draw_background();
        io.present(self).await;
        self.click = false;
        self.read_boundary(io);
    }

    /// `FUN_00447ad0`.
    async fn win_animation(&mut self, io: &mut impl MetaIo) {
        io.play_sound(3);
        let n = ((self.variant + 3) as usize).min(NUM_PIECES);
        for _ in 0..0x10 {
            for i in 0..n {
                self.pieces[i].cls = -1;
                self.frame(io).await;
                self.pieces[i].cls = self.variant;
                self.frame(io).await;
            }
        }
    }

    /// `FUN_00448ad0` then `FUN_0044763c(v)`: 255 won, 2 left.
    pub async fn run(&mut self, io: &mut impl MetaIo, v: i32) -> i32 {
        self.variant = v as i16;
        (self.mx, self.my, self.click, self.right) = io.start_input();
        self.map = self.map_file.clone();
        self.set_up(io);
        self.result = 0;
        loop {
            self.right_button(io);
            if self.click {
                self.left_click(io);
            }
            self.draw(io);
            self.draw_cursor(io);
            io.draw_background();
            io.present(self).await;
            self.click = false;
            self.read_boundary(io);
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

struct Host<'a> {
    host: PuzzleHost<'a>,
    pictures: [Option<Vec<u16>>; 4],
}

impl MetaIo for Host<'_> {
    fn start_input(&mut self) -> (i32, i32, bool, bool) {
        let m = self.host.read_input();
        (m.x, m.y, m.click, m.right)
    }
    fn boundary(&mut self, _g: &MetaGame) -> (i32, i32, bool, bool) {
        self.start_input()
    }
    fn load_picture(&mut self, n: i32) {
        let pic = usize::try_from(n - 1)
            .ok()
            .and_then(|i| self.pictures.get(i))
            .and_then(Clone::clone);
        if let Some(p) = pic {
            self.host.screen.pictures = vec![p];
        }
    }
    fn draw_background(&mut self) {
        self.host.draw_background();
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.host.draw_sprite(bank, x, y, frame);
    }
    fn draw_standard_cursor(&mut self, x: i32, y: i32) {
        self.host.draw_standard_cursor(x, y);
    }
    async fn present(&mut self, _g: &MetaGame) {
        self.host.present().await;
    }
    fn play_sound(&mut self, slot: i32) {
        const NAMES: [&str; 5] = ["META1", "META2", "META3", "META4", "META5"];
        if let Some(n) = usize::try_from(slot).ok().and_then(|s| NAMES.get(s)) {
            self.host.play_sound(n, SamplePlayer::Ca28);
        }
    }
    fn current_place(&mut self) -> i32 {
        self.host.e.current_place_id.map_or(0, i32::from)
    }
    fn aborted(&self) -> bool {
        self.host.aborted()
    }
}

/// StartPuzzle 9..12 (stage 1..4): 255 when solved, 2 when the player
/// leaves, 1 when a file is missing or the game is quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks, stage: i32) -> i32 {
    let frames = load_spr(e, "PZMETAP1.SPR").await;
    let coverage = match read_puzzle_file(e, "PZMETAP1.SPR").await {
        Some(d) => read_coverage(&d),
        None => None,
    };
    let map = load_class_map(e, "PZMETAM.BMP").await;
    let mut pictures: [Option<Vec<u16>>; 4] = Default::default();
    for (i, p) in pictures.iter_mut().enumerate() {
        *p = load_tga(e, &format!("PZMETA{}.TGA", i + 1)).await;
    }
    let (Some(frames), Some(coverage), Some(map)) = (frames, coverage, map) else {
        return 1;
    };
    let sounds = PuzzleSounds::load(e, &["META1", "META2", "META3", "META4", "META5"]).await;
    let mut game = MetaGame::new(map, coverage);
    if let Some(m) = &e.puzzles.meta {
        game.restore(m);
    }
    game.object_in_hand = e.vars.get("objetenmain");
    game.cursor = e.puzzles.cursor;
    let mut host = Host {
        host: PuzzleHost::new(e, hooks),
        pictures,
    };
    host.host.screen.frames = frames;
    host.host.sounds = sounds;
    let r = game.run(&mut host, stage).await;
    let e = host.host.e;
    e.puzzles.cursor = game.cursor;
    e.puzzles.meta = Some(game.memory());
    // FUN_004478d4 emptied the hand and may have set FlagTemp6.
    e.vars.set("objetenmain", 0);
    if game.brought_flag != 0 {
        e.vars.set("flagtemp6", 1);
    }
    if e.should_abort() {
        1
    } else {
        r
    }
}

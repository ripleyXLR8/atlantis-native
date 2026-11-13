//! `PUZ_CAISSES` (loop mode 0x15, `Puzzles[0]`): crates pushed onto marks
//! (atlantis4-cd-en-3022848: init FUN_00476be0, start FUN_004770d0, free
//! FUN_00477120, frame FUN_00477320, draw FUN_0047ccb0, level builder
//! FUN_0047d180, solved test FUN_00480b60). No script of the CD starts it.
//!
//! Four levels (records of 0x30 bytes from 0x00716de0, built by their
//! constructor 0x00476a00 and FUN_0047d180, which assigns every cell): the
//! player's start (floats +0 row, +4 column), the drawing origin (+8 x,
//! +0xc y), the cell size (+0x10 w, +0x14 h), the columns (+0x18) and rows
//! (+0x1c) and the cells (+0x20: `std::map<int row, std::map<int column,
//! int>>`). A cell is 0 floor, 1 wall, 2 outside, 3 a crate, 4 a mark, 5 a
//! crate on a mark ([`LEVELS`], dumped under emulation). The player (row
//! 0x00716dc0, column 0x00716dc4, floats holding whole numbers) moves by
//! the arrows' sprites (`caisses_fleches`, a click) or the arrow keys,
//! pushing one crate at a time; a level without a 3 left is solved
//! (FUN_00480b60): the next one starts, after the fourth `CAISSES_GAGNE`
//! := 1. The buttons (`caisses_bout`): #0 at (50, 50) rebuilds the levels
//! (the current one starts again), #1 at (650, 50) sets `CAISSES_QUIT`.
//!
//! Banks (`spr\`, handles ← the init's `MOV ECX` before FUN_00449020):
//! `caisses_back` (0x00716ed8, frame = the level), `caisses_bloc`
//! (0x00716ec0: #0 a crate, #1 a crate on a mark, #2 a mark),
//! `caisses_bout` (0x00716ef0), `caisses_perso` (0x00716dc8, the player's
//! animation), `caisses_fleches` (0x00716da8, the four arrows). Sounds:
//! `caisse_sfx.wav` (a crate pushed), `caisse_ok.wav` (a crate onto a
//! mark).

use std::any::Any;
use std::sync::Arc;

use atlantis3_data::spr::Bank;

use super::{aff, collision, ftol, Host, Puzzle, PuzzleId};

/// A cell's values.
pub const FLOOR: i32 = 0;
pub const WALL: i32 = 1;
pub const OUTSIDE: i32 = 2;
pub const CRATE: i32 = 3;
pub const MARK: i32 = 4;
pub const PLACED: i32 = 5;

/// The arrow keys' codes (0x006e9e6c; the frame's table 0x0047cc0c).
pub const KEY_UP: i32 = 0x6d;
pub const KEY_LEFT: i32 = 0x6f;
pub const KEY_RIGHT: i32 = 0x70;
pub const KEY_DOWN: i32 = 0x72;

/// A level as FUN_0047d180 builds it.
#[derive(Clone, Copy, Debug)]
pub struct LevelDef {
    /// +0, +4: the player's start (row, column).
    pub start: (i32, i32),
    /// +8, +0xc: the drawing origin.
    pub org: (i32, i32),
    /// +0x10, +0x14: a cell's width and height.
    pub cell: (i32, i32),
    /// +0x18, +0x1c.
    pub cols: i32,
    pub rows: i32,
    /// The cells, a row per string (the outer map's key), a column per
    /// character (the inner map's).
    pub cells: [&'static str; 10],
}

/// FUN_0047d180's four levels (`a4cd_caisses_levels.py`: the original
/// builder run under Unicorn, its maps walked).
pub const LEVELS: [LevelDef; 4] = [
    LevelDef {
        start: (4, 4),
        org: (200, 100),
        cell: (50, 50),
        cols: 8,
        rows: 8,
        cells: [
            "22111222", "22141222", "22101111", "11130341", "14030111", "11113122", "22214122",
            "22211122", "", "",
        ],
    },
    LevelDef {
        start: (1, 1),
        org: (175, 75),
        cell: (50, 50),
        cols: 9,
        rows: 9,
        cells: [
            "111112222",
            "100012222",
            "103312111",
            "103012141",
            "111011141",
            "211000041",
            "210001001",
            "210001111",
            "211111222",
            "",
        ],
    },
    LevelDef {
        start: (3, 2),
        org: (150, 125),
        cell: (50, 50),
        cols: 10,
        rows: 7,
        cells: [
            "2111111122",
            "2100000111",
            "1131110001",
            "1000300301",
            "1044103011",
            "1144100012",
            "2111111112",
            "",
            "",
            "",
        ],
    },
    LevelDef {
        start: (2, 1),
        org: (250, 100),
        cell: (50, 50),
        cols: 6,
        rows: 8,
        cells: [
            "211112", "110012", "103012", "113011", "110301", "143001", "144541", "111111", "", "",
        ],
    },
];

/// A level's cells (row-major).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Grid {
    pub rows: i32,
    pub cols: i32,
    pub cells: Vec<i32>,
}

impl Grid {
    pub fn of(def: &LevelDef) -> Grid {
        let cells = def.cells[..def.rows as usize]
            .iter()
            .flat_map(|r| r.bytes().map(|b| i32::from(b - b'0')))
            .collect();
        Grid {
            rows: def.rows,
            cols: def.cols,
            cells,
        }
    }

    pub fn get(&self, r: i32, c: i32) -> i32 {
        self.cells[(r * self.cols + c) as usize]
    }

    pub fn set(&mut self, r: i32, c: i32, v: i32) {
        self.cells[(r * self.cols + c) as usize] = v;
    }

    /// FUN_00480b60: no crate left off a mark.
    pub fn solved(&self) -> bool {
        !self.cells.contains(&CRATE)
    }
}

/// A move: along the rows (0x00716dc0) or the columns (0x00716dc4), by
/// ±1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    /// (row step, column step).
    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::Up => (-1, 0),
            Dir::Down => (1, 0),
            Dir::Left => (0, -1),
            Dir::Right => (0, 1),
        }
    }

    pub fn key(self) -> i32 {
        match self {
            Dir::Up => KEY_UP,
            Dir::Down => KEY_DOWN,
            Dir::Left => KEY_LEFT,
            Dir::Right => KEY_RIGHT,
        }
    }

    /// The arrow's frame of `caisses_fleches` and its place (the frame's
    /// tests in this order, @ 0x004774ce, 0x0047831f, 0x0047919a,
    /// 0x00479f73).
    pub fn arrow(self) -> (usize, i32, i32) {
        match self {
            Dir::Down => (0, 700, 500),
            Dir::Right => (1, 710, 490),
            Dir::Left => (2, 690, 490),
            Dir::Up => (3, 700, 480),
        }
    }
}

/// What a click hit (the frame's tests in order; the first hit wins).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// `caisses_bout` #0 at (50, 50).
    Reset,
    /// `caisses_bout` #1 at (650, 50).
    Quit,
    Arrow(Dir),
    /// Nothing hit.
    Miss,
}

/// The game's state.
#[derive(Clone)]
pub struct Caisses {
    /// The four levels' cells (the records' maps).
    pub grids: Vec<Grid>,
    /// The level (0x00716f28, 0..3).
    pub level: usize,
    /// The player (0x00716dc0 row, 0x00716dc4 column).
    pub pos: (f32, f32),
    /// The player's animation (0x00716f24).
    pub anim: f32,
    back: Option<Arc<Bank>>,
    bloc: Option<Arc<Bank>>,
    bout: Option<Arc<Bank>>,
    perso: Option<Arc<Bank>>,
    fleches: Option<Arc<Bank>>,
}

impl Default for Caisses {
    fn default() -> Caisses {
        Caisses::new()
    }
}

impl Caisses {
    /// The init's state without the engine (tests): the levels built,
    /// level 0, the player at its start.
    pub fn new() -> Caisses {
        let mut g = Caisses {
            grids: Vec::new(),
            level: 0,
            pos: (0.0, 0.0),
            anim: 0.0,
            back: None,
            bloc: None,
            bout: None,
            perso: None,
            fleches: None,
        };
        g.build();
        g.restart();
        g
    }

    /// FUN_00476be0 (its start FUN_004770d0 is the engine's).
    pub fn init(host: &mut dyn Host) -> Caisses {
        // 0x00716f20 := 1 (the loaded byte); srand(time(0)).
        host.srand_time();
        let mut g = Caisses::new();
        g.back = host.bank("caisses_back.spr");
        g.bloc = host.bank("caisses_bloc.spr");
        g.bout = host.bank("caisses_bout.spr");
        g.perso = host.bank("caisses_perso.spr");
        g.fleches = host.bank("caisses_fleches.spr");
        // FUN_0042e390 (the pointer centred), cursor 29.
        let (w, h) = host.display_size();
        host.set_pointer(w / 2, h / 2);
        // 0x006b1d20 / 0x006b1d24 := 800, 600: their initial values.
        host.set_var("CAISSES_GAGNE", 0);
        host.set_var("CAISSES_QUIT", 0);
        host.sfx_load("caisse_ok.wav");
        host.sfx_load("caisse_sfx.wav");
        g
    }

    /// FUN_0047d180: every level's cells assigned again.
    pub fn build(&mut self) {
        self.grids = LEVELS.iter().map(Grid::of).collect();
    }

    /// The player at the level's start (the record's +0, +4).
    fn restart(&mut self) {
        let (r, c) = LEVELS[self.level].start;
        self.pos = (r as f32, c as f32);
    }

    /// One move (a direction's block of the frame, the arrows' and the
    /// keys' alike): onto a floor or a mark; onto a crate (3, or 5 on a
    /// mark) when the cell behind it is a floor (`caisse_sfx.wav`, the
    /// crate becomes 3) or a mark (`caisse_ok.wav`, 5); the cell left is a
    /// floor (from 3) or a mark (from 5). The row / column bounds: one
    /// step stays below the count − 1 (or above 0), a push below the count
    /// − 2 (or above 1). Whether the player moved.
    pub fn step(&mut self, dir: Dir, host: &mut dyn Host) -> bool {
        let g = &mut self.grids[self.level];
        let (dr, dc) = dir.delta();
        let (r, c) = (self.pos.0 as i32, self.pos.1 as i32);
        // The coordinate that moves, its count.
        let (p, n) = if dr != 0 { (r, g.rows) } else { (c, g.cols) };
        let d = dr + dc;
        let inside = |k: i32| if d > 0 { n - k > p } else { p > k - 1 };
        let at = |k: i32| (r + dr * k, c + dc * k);
        if !inside(1) {
            return false;
        }
        let (r1, c1) = at(1);
        let one = g.get(r1, c1);
        let moved = if one == FLOOR || one == MARK {
            true
        } else if (one == CRATE || one == PLACED) && inside(2) {
            let (r2, c2) = at(2);
            let two = g.get(r2, c2);
            let left = if one == CRATE { FLOOR } else { MARK };
            if two == FLOOR {
                host.sfx_play("caisse_sfx.wav");
                g.set(r2, c2, CRATE);
                g.set(r1, c1, left);
                true
            } else if two == MARK {
                host.sfx_play("caisse_ok.wav");
                g.set(r2, c2, PLACED);
                g.set(r1, c1, left);
                true
            } else {
                false
            }
        } else {
            false
        };
        if moved {
            self.pos = (
                (f64::from(self.pos.0) + f64::from(dr)) as f32,
                (f64::from(self.pos.1) + f64::from(dc)) as f32,
            );
        }
        moved
    }

    /// The frame's game part after its start, the click already decoded
    /// (`None`: no click). False when the frame stops (quit, or won).
    pub fn play(&mut self, click: Option<Click>, host: &mut dyn Host) -> bool {
        if let Some(click) = click {
            match click {
                Click::Reset => {
                    self.build();
                    self.restart();
                }
                Click::Quit => {
                    // @ 0x004774b3: the click byte is left set.
                    host.set_var("CAISSES_QUIT", 1);
                    return false;
                }
                Click::Arrow(d) => {
                    self.step(d, host);
                }
                Click::Miss => {}
            }
            host.set_click(false);
        }
        // The keys (0x00716f2c, never written, would lock them): any of
        // the four arrows, moved or not, clears the code (@ 0x0047cb93).
        let key = host.key_code();
        if let Some(d) = Dir::ALL.into_iter().find(|d| d.key() == key) {
            self.step(d, host);
            host.set_key_code(0);
        }
        if self.grids[self.level].solved() {
            self.level += 1;
            if self.level > 3 {
                host.set_var("CAISSES_GAGNE", 1);
                return false;
            }
            self.restart();
        }
        true
    }

    /// The frame's click tests (@ 0x0047743a..0x00479f92): the buttons,
    /// then the arrows.
    pub fn hit(&self, x: i32, y: i32) -> Click {
        if collision(self.bout.as_ref(), 0, Some(50), 50, x, y) {
            return Click::Reset;
        }
        if collision(self.bout.as_ref(), 1, Some(650), 50, x, y) {
            return Click::Quit;
        }
        for d in [Dir::Down, Dir::Right, Dir::Left, Dir::Up] {
            let (f, ax, ay) = d.arrow();
            if collision(self.fleches.as_ref(), f, Some(ax), ay, x, y) {
                return Click::Arrow(d);
            }
        }
        Click::Miss
    }

    /// FUN_0047ccb0.
    fn draw(&mut self, host: &mut dyn Host) {
        let def = &LEVELS[self.level];
        aff(
            host,
            "caisses_back.spr",
            self.back.as_ref(),
            self.level,
            Some(0),
            0,
            1.0,
        );
        let bout = self.bout.as_ref();
        aff(host, "caisses_bout.spr", bout, 0, Some(50), 50, 1.0);
        aff(host, "caisses_bout.spr", bout, 1, Some(650), 50, 1.0);
        for d in [Dir::Down, Dir::Right, Dir::Left, Dir::Up] {
            let (f, x, y) = d.arrow();
            aff(
                host,
                "caisses_fleches.spr",
                self.fleches.as_ref(),
                f,
                Some(x),
                y,
                1.0,
            );
        }
        let g = &self.grids[self.level];
        let (w, h) = def.cell;
        // Level 0's crates one pixel lower (@ 0x0047cfb7, 0x0047cee9).
        let low = i32::from(self.level == 0);
        for r in 0..def.rows {
            for c in 0..def.cols {
                let (x, y) = (w * c + def.org.0, h * r + def.org.1);
                let (frame, x, y) = match g.get(r, c) {
                    CRATE => (0, x, y + low),
                    PLACED => (1, x, y + low),
                    MARK => (2, x + w / 2, y + h / 2),
                    _ => continue,
                };
                aff(
                    host,
                    "caisses_bloc.spr",
                    self.bloc.as_ref(),
                    frame,
                    Some(x),
                    y,
                    1.0,
                );
            }
        }
        // The animation: 4 frames a second, from 0 at the bank's count.
        self.anim = (f64::from(host.frame_time()) * 4.0 + f64::from(self.anim)) as f32;
        let count = self.perso.as_ref().map_or(0, |b| b.len());
        if count as f32 <= self.anim {
            self.anim = 0.0;
        }
        let y = ftol_cell(h, self.pos.0, def.org.1);
        let x = ftol_cell(w, self.pos.1, def.org.0);
        aff(
            host,
            "caisses_perso.spr",
            self.perso.as_ref(),
            ftol(self.anim).max(0) as usize,
            Some(x),
            y,
            1.0,
        );
        host.draw_pointer();
        host.flip();
    }
}

/// `__ftol`(size × p + origin + size / 2) (@ 0x0047d0d3..0x0047d130).
fn ftol_cell(size: i32, p: f32, org: i32) -> i32 {
    (f64::from(size) * f64::from(p) + f64::from(org) + f64::from(size / 2)) as i32
}

impl Puzzle for Caisses {
    fn id(&self) -> PuzzleId {
        PuzzleId::Caisses
    }

    /// FUN_00477320.
    fn frame(&mut self, host: &mut dyn Host) {
        if !host.script_prologue() {
            return;
        }
        if host.var("CAISSES_GAGNE") != 0 || host.var("CAISSES_QUIT") != 0 {
            return;
        }
        // Past the fourth level with `CAISSES_GAGNE` cleared again (no
        // script does): the original would read past the four records
        // [UNKNOWN]; the port stops.
        if self.level > 3 {
            return;
        }
        let click = host.click().then(|| {
            let (x, y) = host.pointer();
            self.hit(x, y)
        });
        if self.play(click, host) {
            self.draw(host);
        }
    }

    /// FUN_00477120's game part (its play time, 0x00716eb0, has no
    /// reader).
    fn free(&mut self, host: &mut dyn Host) {
        self.back = None;
        self.bloc = None;
        self.bout = None;
        self.perso = None;
        self.fleches = None;
        host.sfx_remove("caisse_ok.wav");
        host.sfx_remove("caisse_sfx.wav");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A shortest solution of the current level (breadth-first over the
/// player's cell and the crates'): the moves, `None` when there is none.
/// For the tests.
pub fn solve(g: &Caisses) -> Option<Vec<Dir>> {
    use std::collections::{HashMap, VecDeque};
    let start = (g.clone_state(), g.pos);
    type Key = (Grid, (i32, i32));
    let mut seen: HashMap<Key, Option<(Key, Dir)>> = HashMap::new();
    let key = |s: &Caisses| (s.grids[s.level].clone(), (s.pos.0 as i32, s.pos.1 as i32));
    let k0 = key(&start.0);
    seen.insert(k0.clone(), None);
    let mut q = VecDeque::from([start.0]);
    let mut sink = super::sim::SimHost::new(0.0);
    while let Some(s) = q.pop_front() {
        let ks = key(&s);
        if s.grids[s.level].solved() {
            let mut path = Vec::new();
            let mut k = ks;
            while let Some(Some((prev, d))) = seen.get(&k) {
                path.push(*d);
                k = prev.clone();
            }
            path.reverse();
            return Some(path);
        }
        for d in Dir::ALL {
            let mut n = s.clone();
            if n.step(d, &mut sink) {
                let kn = key(&n);
                if let std::collections::hash_map::Entry::Vacant(v) = seen.entry(kn) {
                    v.insert(Some((ks.clone(), d)));
                    q.push_back(n);
                }
            }
        }
        sink.sounds.clear();
    }
    None
}

impl Caisses {
    /// A copy without the banks (the solver's states).
    fn clone_state(&self) -> Caisses {
        Caisses {
            grids: vec![self.grids[self.level].clone()],
            level: 0,
            pos: self.pos,
            anim: 0.0,
            back: None,
            bloc: None,
            bout: None,
            perso: None,
            fleches: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzles2004::sim::SimHost;

    fn at(g: &Caisses) -> (i32, i32) {
        (g.pos.0 as i32, g.pos.1 as i32)
    }

    #[test]
    fn levels() {
        for def in &LEVELS {
            let g = Grid::of(def);
            assert_eq!(g.cells.len() as i32, def.rows * def.cols);
            let crates = g.cells.iter().filter(|&&v| v == CRATE).count();
            let marks = g.cells.iter().filter(|&&v| v == MARK).count();
            assert_eq!(crates, marks, "{def:?}");
            assert_eq!(g.get(def.start.0, def.start.1), FLOOR);
        }
    }

    /// Level 0: walking, a wall, a push onto a floor and onto a mark.
    #[test]
    fn moves_and_pushes() {
        let mut g = Caisses::new();
        let mut h = SimHost::new(0.04);
        assert_eq!(at(&g), (4, 4));
        // Left: the crate at (4, 3) onto the floor (4, 2).
        assert!(g.step(Dir::Left, &mut h));
        assert_eq!(at(&g), (4, 3));
        assert_eq!(g.grids[0].get(4, 2), CRATE);
        assert_eq!(g.grids[0].get(4, 3), FLOOR);
        assert_eq!(h.sounds, ["caisse_sfx.wav"]);
        // Again: the crate onto the mark (4, 1).
        assert!(g.step(Dir::Left, &mut h));
        assert_eq!(g.grids[0].get(4, 1), PLACED);
        assert_eq!(h.sounds[1], "caisse_ok.wav");
        // Again: crate on a mark against a wall: blocked.
        assert!(!g.step(Dir::Left, &mut h));
        assert_eq!(at(&g), (4, 2));
        // Down into a wall.
        assert!(!g.step(Dir::Down, &mut h));
        assert_eq!(h.sounds.len(), 2);
    }

    /// Two crates in a row cannot be pushed; a crate leaves its mark.
    #[test]
    fn blocked_and_unplaced() {
        let mut g = Caisses::new();
        let mut h = SimHost::new(0.04);
        // Level 0, row 3: "11130341": from (3, 6)? Use a grid of our own.
        g.grids[0] = Grid {
            rows: 3,
            cols: 6,
            cells: vec![1, 1, 1, 1, 1, 1, 1, 0, 3, 3, 0, 1, 1, 1, 1, 1, 1, 1],
        };
        g.pos = (1.0, 1.0);
        assert!(!g.step(Dir::Right, &mut h));
        g.grids[0].cells = vec![1, 1, 1, 1, 1, 1, 1, 0, 5, 0, 4, 1, 1, 1, 1, 1, 1, 1];
        assert!(g.step(Dir::Right, &mut h));
        assert_eq!(g.grids[0].cells[7..11], [0, 4, 3, 4]);
        assert!(g.step(Dir::Right, &mut h));
        assert_eq!(g.grids[0].cells[7..11], [0, 4, 0, 5]);
        assert_eq!(h.sounds, ["caisse_sfx.wav", "caisse_ok.wav"]);
    }

    /// The buttons and the keys.
    #[test]
    fn buttons_and_keys() {
        let mut g = Caisses::new();
        let mut h = SimHost::new(0.04);
        h.key_code = KEY_LEFT;
        assert!(g.play(None, &mut h));
        assert_eq!((at(&g), h.key_code), ((4, 3), 0));
        // A key that does not move is cleared too.
        h.key_code = KEY_DOWN;
        g.play(None, &mut h);
        assert_eq!((at(&g), h.key_code), ((4, 3), 0));
        // Another key is kept.
        h.key_code = 0x10;
        g.play(None, &mut h);
        assert_eq!(h.key_code, 0x10);
        h.key_code = 0;
        h.click = true;
        g.play(Some(Click::Reset), &mut h);
        assert_eq!(
            (at(&g), g.grids[0].get(4, 3), h.click),
            ((4, 4), CRATE, false)
        );
        h.click = true;
        assert!(!g.play(Some(Click::Quit), &mut h));
        assert_eq!((h.var("CAISSES_QUIT"), h.click), (1, true));
    }

    /// Each level solved in turn, the next one started; after the fourth,
    /// `CAISSES_GAGNE`.
    #[test]
    fn levels_won() {
        let mut g = Caisses::new();
        let mut h = SimHost::new(0.04);
        for level in 0..4 {
            assert_eq!(g.level, level);
            let moves = solve(&g).expect("a solution");
            let n = moves.len();
            for (i, d) in moves.into_iter().enumerate() {
                h.key_code = d.key();
                let go_on = g.play(None, &mut h);
                assert_eq!(go_on, !(level == 3 && i == n - 1));
            }
            if level < 3 {
                assert_eq!(at(&g), LEVELS[level + 1].start);
            }
        }
        assert_eq!(h.var("CAISSES_GAGNE"), 1);
    }
}

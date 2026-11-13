//! `PUZ_BULLE`: the drive through the labyrinth of crossroads ("PuzzleDriver":
//! init FUN_0048d960, start FUN_0048dca0, frame FUN_0048dcf0, free
//! FUN_0048ea90, the crossroads FUN_0048eda0).
//!
//! The car drives on a grid of crossroads (4 × 4 for level 1, 6 × 6 for
//! level 2; level 3's 8 × 8 is built and never reached: the end of level
//! 2 wins). Each stretch between two crossroads and each turn is a travel
//! film of `binktraj\` (`LAxxxyyy.bik`, `LAxxxG/D.bik`) with transition
//! sprites of `spf\` drawn over its frames; the car's place is shown on a
//! radar. A wall in a stretch (a negative edge) crashes the car 30 % into
//! its film: `BULLE_PERDU`. The start waits 5 s (or a click on the star)
//! before giving the player the wheel (`BULLE_ETOILE`); until a direction
//! is chosen (an arrow sprite or an arrow key, with the helmet
//! `CASQUE_C17` in the bag) the car follows the level's route. The end
//! crossroads of level 2: `BULLE_GAGNE`.
//!
//! The films' names and sprites (FUN_0048fae0, FUN_00490890, FUN_00491580,
//! FUN_00492270, FUN_00492d60: 15 KB of name choices) and the edges
//! (FUN_0048f1a0) are the original code's output under Unicorn
//! (`a4cd_bulle_tables.py`), kept in `bulle_tables.txt`.
//!
//! The driver's globals are program-wide: the init resets only the star,
//! its timer and the helmet; the level, the car and the crossroads stay
//! from one session to the next (the port keeps them with
//! [`Host::keep`]).

use std::any::Any;
use std::sync::{Arc, OnceLock};

use atlantis3_data::spr::Bank;

use super::{aff, collision, ftol, Host, Puzzle, PuzzleId};

/// The arrows' key codes (0x006e9e6c): up, left, right, down.
pub const KEY_UP: i32 = 0x6d;
pub const KEY_LEFT: i32 = 0x6f;
pub const KEY_RIGHT: i32 = 0x70;
pub const KEY_DOWN: i32 = 0x72;

/// The route memory FUN_0048f6e0 reads: level 3's route at 0x00718d40,
/// level 2's at 0x00718d68, level 1's at 0x00718d90, the zeros between
/// (never written) up to 0x00718db8 (the car's x: not a route).
const ROUTES: [i32; 30] = [
    3, 0, 1, 1, 1, 0, 3, 0, 0, 0, // 0x00718d40
    3, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x00718d68
    3, 0, 3, 0, 0, 0, 0, 0, 0, 0, // 0x00718d90
];

/// A level's grid size (FUN_0048f790; another level logs
/// `PuzzleDriver::GetTailleGrilleCarrefour` and gives 1).
pub fn size(level: i32) -> i32 {
    match level {
        1 => 4,
        2 => 6,
        3 => 8,
        _ => 1,
    }
}

/// FUN_0048f6e0: route entry `i` of `level`, `None` past the static
/// memory ([UNKNOWN]: the original then reads its own globals).
pub fn route(level: i32, i: i32) -> Option<i32> {
    let base = match level {
        1 => 20,
        2 => 10,
        _ => 0,
    };
    usize::try_from(base + i)
        .ok()
        .and_then(|k| ROUTES.get(k))
        .copied()
}

/// The start and end crossroads (0x00718d20 / 0x00718cfc + 4 level).
pub fn start_end(level: i32) -> (i32, i32) {
    match level {
        1 => (14, 2),
        2 => (33, 1),
        _ => (61, 4),
    }
}

/// FUN_0048f800: the crossroads after `cell` towards `dir` (0 up, 1
/// right, 2 down, 3 left; another logs `GetProchainCarrefour`), with
/// the current level's width and C's division.
pub fn next_cell(level: i32, cell: i32, dir: i32) -> i32 {
    let w = size(level);
    let (mut r, mut c) = (cell / w, cell % w);
    match dir {
        0 => r -= 1,
        1 => c += 1,
        2 => r += 1,
        3 => c -= 1,
        _ => log::warn!("PuzzleDriver::GetProchainCarrefour : direction {dir}"),
    }
    w * r + c
}

/// FUN_0048f8a0: whether `dir` leaves the grid from `cell`.
pub fn at_border(level: i32, cell: i32, dir: i32) -> bool {
    let w = size(level);
    let (r, c) = (cell / w, cell % w);
    match dir {
        0 => r == 0,
        1 => c == w - 1,
        2 => r == w - 1,
        3 => c == 0,
        _ => false,
    }
}

/// FUN_0048f970: the first direction that is neither back nor out of the
/// grid (0 when none).
pub fn first_way(level: i32, cell: i32, dir: i32) -> i32 {
    let back = (dir + 2).rem_euclid(4);
    let mut u = 0;
    while u == back || at_border(level, cell, u) {
        u += 1;
        if u > 3 {
            return 0;
        }
    }
    u
}

/// FUN_0048fa60: the turn from `from` to `to` (0 left, 1 straight, 2
/// right; −1 for none) — with the jump table's fall-throughs: 0 → 2 and
/// 1 → 3 give 1, 2 → 0 gives 0.
pub fn turn_of(from: i32, to: i32) -> i32 {
    const T: [[i32; 4]; 4] = [[1, 2, 1, 0], [0, 1, 2, 1], [0, 0, 1, 2], [2, -1, 0, 1]];
    match (usize::try_from(from), usize::try_from(to)) {
        (Ok(f), Ok(t)) if f < 4 && t < 4 => T[f][t],
        (Ok(f), _) if f < 4 => -1,
        _ => -1,
    }
}

/// FUN_0048f550: a crossroads' place on the radar (FUN_0048f4f0's
/// corner by `level`, FUN_0048f4d0's 3 and FUN_0048f4e0's 24 for every
/// level).
pub fn radar_pos(level: i32, cell: i32) -> (f32, f32) {
    let w = size(level);
    let (r, c) = (cell / w, cell % w);
    let (ox, oy) = match level {
        1 => (0x12e, 0x198),
        2 => (0x10e, 0x17a),
        _ => (0xf6, 0x164),
    };
    let x = f64::from(24 * c) + (f64::from(c) + 0.5) * 3.0 + f64::from(ox);
    let y = f64::from(24 * r) + (f64::from(oy) + (f64::from(r) + 0.5) * 3.0);
    (x as f32, y as f32)
}

/// A film and its transition sprites.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    pub film: String,
    pub spf: Vec<String>,
}

/// A level's tables (from the original's output).
#[derive(Clone, Debug, Default)]
pub struct Level {
    /// FUN_0048f650: the edge byte between two crossroads (0 none, 1 / 2
    /// open, 0xfa..0xff a wall).
    pub edges: Vec<u8>,
    /// Per crossroads and direction: the stretch's film (FUN_0048fae0 +
    /// FUN_00492270).
    pub straight: Vec<[Clip; 4]>,
    /// Per crossroads, direction and turn (0, 2): the turn's film
    /// (FUN_00491580 / FUN_00490890 + FUN_00492d60).
    pub turns: Vec<[[Clip; 2]; 4]>,
}

/// The three levels' tables (`bulle_tables.txt`).
pub fn levels() -> &'static [Level; 3] {
    static L: OnceLock<[Level; 3]> = OnceLock::new();
    L.get_or_init(|| {
        let mut out: [Level; 3] = Default::default();
        let mut cur = 0usize;
        let clip = |s: &str| {
            let film = s
                .split(" film=")
                .nth(1)
                .and_then(|t| t.split(' ').next())
                .unwrap_or("")
                .to_string();
            let spf = s.split(" spf=").nth(1).unwrap_or("");
            Clip {
                film,
                spf: spf
                    .split(',')
                    .filter(|n| !n.is_empty())
                    .map(str::to_string)
                    .collect(),
            }
        };
        for line in include_str!("bulle_tables.txt").lines() {
            let w: Vec<&str> = line.split(' ').collect();
            match w[0] {
                "==" => {
                    cur = w[2].parse::<usize>().unwrap() - 1;
                    let n = size(cur as i32 + 1) as usize;
                    let l = &mut out[cur];
                    l.edges = vec![0; n * n * n * n];
                    l.straight = vec![Default::default(); n * n];
                    l.turns = vec![Default::default(); n * n];
                }
                "edge" => {
                    let n = (size(cur as i32 + 1) * size(cur as i32 + 1)) as usize;
                    let a: usize = w[1].parse().unwrap();
                    let b: usize = w[2].parse().unwrap();
                    let l = &mut out[cur];
                    l.edges[a * n + b] = u8::from_str_radix(w[3], 16).unwrap();
                    l.edges[b * n + a] = u8::from_str_radix(w[4], 16).unwrap();
                }
                "straight" => {
                    let cell: usize = w[1].parse().unwrap();
                    let d: usize = w[2].parse().unwrap();
                    out[cur].straight[cell][d] = clip(line);
                }
                "turn" => {
                    let cell: usize = w[1].parse().unwrap();
                    let d: usize = w[2].parse().unwrap();
                    let t: usize = w[3].parse().unwrap();
                    out[cur].turns[cell][d][t / 2] = clip(line);
                }
                _ => {}
            }
        }
        out
    })
}

/// FUN_0048f650's byte for (a, b), as the signed char the frame tests;
/// `None` outside the level's tables ([UNKNOWN]: the original reads
/// other memory).
pub fn edge(level: i32, a: i32, b: i32) -> Option<i8> {
    let l = levels().get(usize::try_from(level - 1).ok()?)?;
    let n = size(level) * size(level);
    if !(0..n).contains(&a) || !(0..n).contains(&b) {
        return None;
    }
    Some(l.edges[(a * n + b) as usize] as i8)
}

/// The game's state.
#[derive(Clone)]
pub struct Bulle {
    /// The level (0x006b2148, 1 in .data) and "start the level" (0x006b2138,
    /// 1 in .data).
    pub level: i32,
    pub restart: bool,
    /// On a stretch (0x00718dcd; else in a turn).
    pub driving: bool,
    /// The last crossroads reached (0x00718dc9).
    pub won: bool,
    /// The crossroads (0x00718dd0), the direction (0x00718dd4), the next
    /// one (0x00718dd8), the route index (0x00718ddc, −1 off the route),
    /// a direction chosen (0x00718dcc), the last turn chosen (0x00718de0).
    pub cell: i32,
    pub dir: i32,
    pub next: i32,
    pub idx: i32,
    pub chosen: bool,
    pub turn: i32,
    /// The car's place on the radar: the stretch's start (0x00718d30),
    /// end (0x00718cd8) and now (0x00718db8).
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub car: (f32, f32),
    /// The player has the wheel (0x00718dcb).
    pub star: bool,
    /// `CASQUE_C17` in the bag at the init (0x00718de4).
    pub helmet: bool,
    /// The start's timer (0x00718dca), its length (0x006b2144, 5.0 at the
    /// init) and start (0x00718ce8).
    pub timer_on: bool,
    pub timer_len: f32,
    pub timer_start: f64,
    voiture: Option<Arc<Bank>>,
    radar: Option<Arc<Bank>>,
    bout: Option<Arc<Bank>>,
}

/// The program-wide globals kept between sessions.
const KEPT: [&str; 16] = [
    "level", "restart", "driving", "won", "cell", "dir", "next", "idx", "chosen", "turn", "from.x",
    "from.y", "to.x", "to.y", "car.x", "car.y",
];

impl Bulle {
    /// The state of a first session without the engine (tests).
    pub fn new() -> Bulle {
        Bulle {
            level: 1,
            restart: true,
            driving: false,
            won: false,
            cell: 0,
            dir: 0,
            next: 0,
            idx: 0,
            chosen: false,
            turn: 0,
            from: (0.0, 0.0),
            to: (0.0, 0.0),
            car: (0.0, 0.0),
            star: false,
            helmet: true,
            timer_on: false,
            timer_len: 5.0,
            timer_start: 0.0,
            voiture: None,
            radar: None,
            bout: None,
        }
    }

    fn kept_values(&self) -> [i32; 16] {
        let f = |v: f32| v.to_bits() as i32;
        [
            self.level,
            i32::from(self.restart),
            i32::from(self.driving),
            i32::from(self.won),
            self.cell,
            self.dir,
            self.next,
            self.idx,
            i32::from(self.chosen),
            self.turn,
            f(self.from.0),
            f(self.from.1),
            f(self.to.0),
            f(self.to.1),
            f(self.car.0),
            f(self.car.1),
        ]
    }

    fn load_kept(&mut self, host: &dyn Host) {
        let mut v = self.kept_values();
        for (k, slot) in KEPT.iter().zip(v.iter_mut()) {
            if let Some(x) = host.kept(&format!("bulle.{k}")) {
                *slot = x;
            }
        }
        let f = |x: i32| f32::from_bits(x as u32);
        self.level = v[0];
        self.restart = v[1] != 0;
        self.driving = v[2] != 0;
        self.won = v[3] != 0;
        (self.cell, self.dir, self.next, self.idx) = (v[4], v[5], v[6], v[7]);
        self.chosen = v[8] != 0;
        self.turn = v[9];
        self.from = (f(v[10]), f(v[11]));
        self.to = (f(v[12]), f(v[13]));
        self.car = (f(v[14]), f(v[15]));
    }

    fn save_kept(&self, host: &mut dyn Host) {
        for (k, v) in KEPT.iter().zip(self.kept_values()) {
            host.keep(&format!("bulle.{k}"), v);
        }
    }

    /// FUN_0048d960 (its start FUN_0048dca0 is called at its end: the
    /// variables and the timer here, the mode the engine's).
    pub fn init(host: &mut dyn Host) -> Bulle {
        let mut g = Bulle::new();
        g.load_kept(host);
        // The tables (constants) and FUN_0048f1a0 for levels 1..3:
        // [`levels`]. FUN_0042e390, cursor 29.
        let (w, h) = host.display_size();
        host.set_pointer(w / 2, h / 2);
        g.star = false;
        g.timer_len = 5.0;
        g.helmet = host.in_bag("CASQUE_C17");
        // FUN_0048eb10.
        g.voiture = host.bank("voiture.spr");
        g.radar = host.bank("radar.spr");
        g.bout = host.bank("bout.spr");
        // 0x006b1d20 / 0x006b1d24 := 800, 600. The start:
        host.set_var("BULLE_DEBUT", 1);
        host.set_var("BULLE_ETOILE", 0);
        g.timer_begin(host);
        g
    }

    /// FUN_0048f050: the start's timer (FUN_00402d80 pauses a film open:
    /// none at a session's start in the port, the last session's being
    /// closed by its free).
    fn timer_begin(&mut self, host: &dyn Host) {
        if self.timer_len > 0.0 && !self.timer_on {
            self.timer_on = true;
            self.timer_start = host.clock();
        }
    }

    /// FUN_0048f0d0.
    fn timer_done(&self, host: &dyn Host) -> bool {
        f64::from(self.timer_len) <= host.clock() - self.timer_start
    }

    /// FUN_0048f100 (FUN_00402db0 resumes a film never paused).
    fn timer_end(&mut self, host: &dyn Host) {
        if self.timer_on {
            let left = f64::from(self.timer_len) - (host.clock() - self.timer_start);
            self.timer_len = left as f32;
            if self.timer_len <= 0.0 {
                self.timer_len = 0.0;
            }
            self.timer_on = false;
        }
    }

    /// The star: `BULLE_ETOILE`, the wheel to the player.
    fn take_wheel(&mut self, host: &mut dyn Host) {
        self.star = true;
        host.set_var("BULLE_ETOILE", 1);
        self.timer_end(host);
    }

    /// FUN_0048f9c0(turn): the next direction from the current one.
    pub fn choose(&mut self, turn: i32) {
        self.turn = turn;
        let to = match (self.dir, turn) {
            (0, 0) => Some(3),
            (0, 2) => Some(1),
            (1, 0) => Some(0),
            (1, 2) => Some(2),
            (2, 0) => Some(1),
            (2, 2) => Some(3),
            (3, 0) => Some(2),
            (3, 2) => Some(0),
            _ => None,
        };
        match to {
            Some(d) => self.next = d,
            None if turn == 1 => self.next = self.dir,
            None => {}
        }
    }

    /// An arrow towards `abs` (the four click and key branches): the turn
    /// from the current direction, nothing for a U-turn.
    fn steer(&mut self, abs: i32) {
        let t = match (abs, self.dir) {
            (a, d) if a == d => 1,
            (a, d) if (d + 3) % 4 == a => 0,
            (a, d) if (d + 1) % 4 == a => 2,
            _ => return,
        };
        self.choose(t);
    }

    fn route(&self, i: i32) -> i32 {
        route(self.level, i).unwrap_or(0)
    }

    /// FUN_0048eda0: a level's start, or the crossroads reached.
    pub fn crossroads(&mut self, host: &mut dyn Host) {
        if self.restart {
            self.idx = 0;
            self.cell = start_end(self.level).0;
            self.dir = self.route(0);
            self.next = self.route(1);
            self.from = radar_pos(self.level, self.cell);
            self.car = self.from;
            self.to = radar_pos(self.level, next_cell(self.level, self.cell, self.dir));
            self.driving = true;
            return;
        }
        self.cell = next_cell(self.level, self.cell, self.dir);
        if self.cell == start_end(self.level).1 {
            if self.level != 2 {
                self.restart = true;
                self.level += 1;
                return;
            }
            self.won = true;
        }
        if self.won {
            host.set_var("BULLE_GAGNE", 1);
            // FUN_00412be0: the screen cleared.
            return;
        }
        let on_route = if !self.star {
            true
        } else if self.chosen {
            self.idx = -1;
            self.chosen = false;
            self.check_border(host);
            false
        } else if self.idx >= 0 {
            true
        } else {
            self.choose(1);
            self.check_border(host);
            false
        };
        if on_route {
            self.dir = self.next;
            self.idx += 1;
            self.next = self.route(self.idx + 1);
        } else {
            self.dir = self.next;
        }
        self.from = self.to;
        self.car = self.to;
        self.to = radar_pos(self.level, next_cell(self.level, self.cell, self.dir));
    }

    /// The chosen direction leaves the grid: the first other way.
    fn check_border(&mut self, _host: &mut dyn Host) {
        if at_border(self.level, self.cell, self.next) {
            log::warn!("GestionCarrefour : Direction interdite");
            let u = first_way(self.level, self.cell, self.dir);
            self.choose(turn_of(self.dir, u));
        }
    }

    fn clip(&self, turn: Option<i32>) -> Option<&'static Clip> {
        let l = levels().get(usize::try_from(self.level - 1).ok()?)?;
        let cell = usize::try_from(self.cell).ok()?;
        let d = usize::try_from(self.dir).ok().filter(|&d| d < 4)?;
        match turn {
            None => l.straight.get(cell).map(|s| &s[d]),
            Some(t) => l.turns.get(cell).map(|s| &s[d][(t / 2) as usize]),
        }
    }

    /// A stretch's or a turn's film (the names' functions, then
    /// `AtlBink_Start(.., 15, 1, 0)`; 0x006ac080 := 0).
    fn start_film(&self, host: &mut dyn Host, turn: Option<i32>) {
        let Some(c) = self.clip(turn) else {
            log::warn!(
                "PUZ_BULLE: crossroads {} direction {} outside the tables [UNKNOWN]",
                self.cell,
                self.dir
            );
            return;
        };
        let spf: Vec<&str> = c.spf.iter().map(String::as_str).collect();
        host.film_start(&c.film, 15, &spf);
    }

    /// The overlays (@ 0x0048e2ca..0x0048e5ce).
    fn draw(&self, host: &mut dyn Host) {
        let sel = if self.idx >= 0 {
            self.route(self.idx)
        } else {
            self.next
        };
        let b = self.bout.clone();
        let bb = b.as_ref();
        let put = |host: &mut dyn Host, f: usize| aff(host, "bout.spr", bb, f, None, 0, 1.0);
        let h = self.helmet;
        // North 0 / 1, west 6 / 7, east 2 / 3, south 8 / 9.
        enum Next {
            West,
            East,
            South,
        }
        let mut next = if sel == 0 && h {
            put(host, 1);
            Next::West
        } else {
            put(host, 0);
            if sel == 3 && h {
                put(host, 7);
                Next::East
            } else {
                Next::West
            }
        };
        if let Next::West = next {
            put(host, 6);
            next = if sel == 1 && h {
                put(host, 3);
                Next::South
            } else {
                Next::East
            };
        }
        if let Next::East = next {
            put(host, 2);
            if sel == 2 && h {
                put(host, 9);
            } else {
                put(host, 8);
            }
        } else {
            put(host, 8);
        }
        put(host, if self.star && h { 5 } else { 4 });
        if h {
            let r = self.radar.clone();
            let (x, y) = (0xe2, 0x154);
            aff(
                host,
                "radar.spr",
                r.as_ref(),
                (self.level - 1).max(0) as usize,
                Some(x),
                y,
                1.0,
            );
            let v = self.voiture.clone();
            aff(
                host,
                "voiture.spr",
                v.as_ref(),
                0,
                Some(ftol(self.car.0)),
                ftol(self.car.1),
                1.0,
            );
            // 0x00718de5 is never written: the pointer (FUN_00429460).
            host.draw_pointer();
        }
    }

    /// The clicks on the arrows and the star (@ 0x0048dd30..0x0048df60).
    fn clicks(&mut self, host: &mut dyn Host) {
        if !host.click() || !self.helmet {
            return;
        }
        let (px, py) = host.pointer();
        let hit = |f: usize| collision(self.bout.as_ref(), f, None, 0, px, py);
        for (f, abs) in [(6, 3), (0, 0), (2, 1), (8, 2)] {
            if hit(f) && self.star {
                self.chosen = true;
                self.idx = -1;
                self.steer(abs);
                host.set_click(false);
                return;
            }
        }
        if hit(4) && !self.star {
            host.set_click(false);
            self.take_wheel(host);
            return;
        }
        host.set_click(false);
    }

    /// The frame without the prologue (@ 0x0048dd30..0x0048ea5c).
    pub fn step(&mut self, host: &mut dyn Host) {
        self.clicks(host);
        if self.timer_on && self.timer_done(host) {
            self.take_wheel(host);
        }
        let key = host.key_code();
        if key != 0 && self.helmet && self.star {
            self.chosen = true;
            self.idx = -1;
            match key {
                KEY_UP => self.steer(0),
                KEY_LEFT => self.steer(3),
                KEY_RIGHT => self.steer(1),
                KEY_DOWN => self.steer(2),
                _ => {}
            }
        }
        if self.driving {
            if self.restart {
                self.restart = false;
                self.start_film(host, None);
            }
            let p = progress(host.film_frame_num(), host.film_frames());
            self.car = lerp(self.from, self.to, p);
            let ahead = next_cell(self.level, self.cell, self.dir);
            let wall = edge(self.level, self.cell, ahead).is_some_and(|e| e < 0);
            if wall && f64::from(p) >= f64::from(0.3f32) {
                host.film_close();
                self.driving = false;
                self.restart = true;
                host.set_var("BULLE_PERDU", 1);
                let n = host.var("BULLE_NB_MORTS");
                host.set_var("BULLE_NB_MORTS", n + 1);
                return;
            }
        } else if self.restart {
            self.crossroads(host);
        }
        if host.film_frame_num() == -1 {
            return;
        }
        let drawn = host.film_draw();
        self.draw(host);
        if !drawn {
            return;
        }
        host.flip();
        if self.timer_on || host.film_next() {
            return;
        }
        host.film_close();
        if !self.driving {
            if self.restart {
                return;
            }
            self.driving = true;
            self.start_film(host, None);
            return;
        }
        if self.restart {
            return;
        }
        self.driving = false;
        self.crossroads(host);
        if self.restart {
            return;
        }
        let t = if !self.star || self.idx > 0 {
            turn_of(self.route(self.idx - 1), self.route(self.idx))
        } else if self.idx == 0 {
            1
        } else {
            self.turn
        };
        match t {
            0 | 2 => self.start_film(host, Some(t)),
            _ => {
                self.start_film(host, None);
                self.driving = true;
            }
        }
    }
}

/// The film's progress: `FrameNum` / `Frames` (`fild`, `fstp` float,
/// `fdivr`, `fst` float), at most 1.0.
pub fn progress(frame: i32, frames: i32) -> f32 {
    let p = (f64::from(frame as f32) / f64::from(frames)) as f32;
    if p < 1.0 {
        p
    } else {
        1.0
    }
}

/// The car between `from` and `to` (@ 0x0048e1a4..0x0048e1f3).
pub fn lerp(from: (f32, f32), to: (f32, f32), p: f32) -> (f32, f32) {
    let p = f64::from(p);
    let dx = (f64::from(to.0) - f64::from(from.0)) as f32;
    let x = (f64::from((f64::from(dx) * p) as f32) + f64::from(from.0)) as f32;
    let y = ((f64::from(to.1) - f64::from(from.1)) * p + f64::from(from.1)) as f32;
    (x, y)
}

impl Default for Bulle {
    fn default() -> Bulle {
        Bulle::new()
    }
}

impl Puzzle for Bulle {
    fn id(&self) -> PuzzleId {
        PuzzleId::Bulle
    }

    /// FUN_0048dcf0.
    fn frame(&mut self, host: &mut dyn Host) {
        if !host.script_prologue() {
            return;
        }
        self.step(host);
        self.save_kept(host);
    }

    /// FUN_0048ea90's game part: the transition sprites cleared
    /// (FUN_00436150), the banks. The film stays open in the original
    /// (no reader until the next `AtlBink_Start`); the port closes it.
    fn free(&mut self, host: &mut dyn Host) {
        self.save_kept(host);
        host.film_close();
        self.voiture = None;
        self.radar = None;
        self.bout = None;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzles2004::sim::SimHost;

    #[test]
    fn grid() {
        assert_eq!(next_cell(1, 14, 3), 13);
        assert_eq!(next_cell(1, 0, 0), -4);
        assert!(at_border(1, 3, 1) && !at_border(1, 2, 1));
        assert_eq!(first_way(1, 0, 2), 1);
        assert_eq!(turn_of(0, 3), 0);
        assert_eq!(turn_of(0, 1), 2);
        assert_eq!(turn_of(0, 2), 1, "the table's fall-through");
        assert_eq!(turn_of(3, 1), -1);
        assert_eq!(route(1, 0), Some(3));
        assert_eq!(route(2, 10), Some(3), "level 1's route after level 2's");
        assert_eq!(route(1, 10), None);
        assert_eq!(radar_pos(1, 14), (357.5, 490.5));
    }

    #[test]
    fn tables() {
        let l = &levels()[0];
        assert_eq!(edge(1, 8, 4), Some(-5), "the route's wall (0xfb)");
        assert_eq!(edge(1, 14, 13), Some(1));
        assert_eq!(l.straight[14][3].film, "LA009005.bik");
        assert_eq!(l.turns[1][1][0].spf, ["PND011G.spr", "PS06_07G.spr"]);
    }

    /// The arrows steer relatively to the direction; a U-turn is refused.
    #[test]
    fn steering() {
        let mut g = Bulle::new();
        g.dir = 0;
        g.steer(3);
        assert_eq!((g.next, g.turn), (3, 0));
        g.steer(1);
        assert_eq!((g.next, g.turn), (1, 2));
        g.steer(0);
        assert_eq!((g.next, g.turn), (0, 1));
        g.next = 9;
        g.steer(2);
        assert_eq!(g.next, 9);
    }

    /// The start: the level's start, the 5 s timer, then the wheel.
    #[test]
    fn start_and_star() {
        let mut h = SimHost::new(0.04);
        let mut g = Bulle::init(&mut h);
        assert!(g.timer_on && !g.star);
        g.step(&mut h);
        assert_eq!((g.cell, g.dir, g.next, g.driving), (14, 3, 0, true));
        h.now = 5.0;
        g.step(&mut h);
        assert!(g.star && !g.timer_on);
        assert_eq!(h.var("BULLE_ETOILE"), 1);
        assert_eq!(g.timer_len, 0.0);
    }

    #[test]
    fn car() {
        assert_eq!(progress(30, 60), 0.5);
        assert_eq!(progress(-1, -1), 1.0);
        assert_eq!(lerp((10.0, 20.0), (30.0, 0.0), 0.5), (20.0, 10.0));
    }
}

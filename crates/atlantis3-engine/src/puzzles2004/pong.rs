//! `PUZ_PONG`: the paddle game against a god (init FUN_0049b1e0, start
//! FUN_0049b760, frame FUN_0049b9a0, free FUN_0049b7b0).
//!
//! Five segments in a `std::list` (0x0071993c, set up by FUN_0049cd10):
//! the player's paddle (x 110), the god's (x 590), and the sarcophagus's
//! three sides (x 625..725, y 160..440). A segment is `{p, d, n, |d|²,
//! radius, id, behaviour, destructible, hp, armed, timer}` (+0x00..+0x34
//! of the node's data). The ball (radius 10) moves in sub-steps of 0.005 s
//! (0.03 s between x 200 and 500); at each, FUN_0049d4d0 tests it against
//! every segment (FUN_0049d5c0: the nearest point of the segment, a hit
//! within radius + 10) and bounces it once per contact (the map 0x007198e0
//! keeps the ids in contact). Past the player's line (x < 95) the player
//! loses 20 points, past the god's (x > 605) the god does; the god at 0 is
//! knocked out (its paddle gone, the ball bouncing at 715, the sarcophagus
//! open to the ball: 20 points a hit, 2 s apart), and its points regrow
//! (3.33 a second, back at 100 with the ball left of 600). The
//! sarcophagus at 0 ends a round: the god's variable (`PONG_*_DESACTIVE`)
//! and the next round with one god less (FUN_0049e110); after the god of
//! round 2, `PONG_GAGNE` = 1. The player at 0: `PONG_PERDU` = 1.
//!
//! Floats: the program's x87 runs with precision 53 bits (0x027f): a value
//! kept on the x87 stack is an `f64`, rounded to `f32` where the original
//! stores it.

use std::any::Any;
use std::sync::Arc;

use atlantis3_data::spr::Bank;

use super::{aff, ftol, ftol_d, Host, Puzzle, PuzzleId};

/// The arrows' key codes (0x006e9e6c): up, down.
pub const KEY_UP: i32 = 0x6d;
pub const KEY_DOWN: i32 = 0x72;

/// The ball's radius (FUN_0049d4d0's third argument).
pub const BALL_RADIUS: i32 = 10;

/// The gods' variables by the round's god (0x007199f4, @ 0x0049bb69,
/// table 0x0049ccf4): 0..3.
pub const GOD_VARS: [&str; 4] = [
    "PONG_ENNA_DESACTIVE",
    "PONG_COSMO_DESACTIVE",
    "PONG_KAMA_DESACTIVE",
    "PONG_SAMA_DESACTIVE",
];

/// A segment (the node's data, 0x38 bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
    /// +0x00: the start.
    pub p: (f32, f32),
    /// +0x08: start to end.
    pub d: (f32, f32),
    /// +0x10: the normal (copied as is, not read by the bounce).
    pub n: (f32, f32),
    /// +0x18: |d|².
    pub len2: f32,
    /// +0x1c: its half thickness (an int).
    pub radius: i32,
    /// +0x20: its id (1 the player's paddle, 2 the god's, 3..5 the
    /// sarcophagus).
    pub id: i32,
    /// +0x24: what a hit does after the bounce (3: a paddle's; 1, 2 are
    /// handled by FUN_0049d5c0, no segment of the game has them).
    pub behaviour: i32,
    /// +0x28: 1 for the sarcophagus.
    pub destructible: i32,
    /// +0x2c, +0x30, +0x34 (the paddles' are the stack's leftovers in
    /// FUN_0049cd10, never read: 0 here).
    pub hp: i32,
    pub armed: bool,
    pub timer: f32,
}

impl Seg {
    /// |d| as the x87 has it (FUN_004855d0 then `fsqrt`).
    fn len(&self) -> f64 {
        len2(self.d).sqrt()
    }
}

/// FUN_004855d0: x² + y² (on the x87 stack).
fn len2(v: (f32, f32)) -> f64 {
    let (x, y) = (f64::from(v.0), f64::from(v.1));
    x * x + y * y
}

/// FUN_0049e3d0: `k · v`, each part stored as a float.
fn scale(v: (f32, f32), k: f32) -> (f32, f32) {
    let k = f64::from(k);
    ((k * f64::from(v.0)) as f32, (k * f64::from(v.1)) as f32)
}

/// `v` normalised by FUN_0049e3d0: `1 / sqrt(len2)` stored as its float
/// argument.
fn unit_f(v: (f32, f32)) -> (f32, f32) {
    scale(v, (1.0 / len2(v).sqrt()) as f32)
}

/// `v` normalised on the stack (`fld st0`, `fmul`): `1 / sqrt(len2)` kept
/// as a double, each part stored.
fn unit(v: (f32, f32)) -> (f32, f32) {
    let k = 1.0 / len2(v).sqrt();
    ((k * f64::from(v.0)) as f32, (k * f64::from(v.1)) as f32)
}

/// FUN_0049cd10's five segments, in the list's order.
pub fn segments() -> [Seg; 5] {
    let wall = |x: f32, id: i32| Seg {
        p: (x, 275.0),
        d: (0.0, 100.0),
        n: (1.0, 0.0),
        len2: 10000.0,
        radius: 2,
        id,
        behaviour: 3,
        destructible: 0,
        hp: 0,
        armed: false,
        timer: 0.0,
    };
    let side = |p: (f32, f32), d: (f32, f32), n: (f32, f32), len2: f32, id: i32| Seg {
        p,
        d,
        n,
        len2,
        radius: 5,
        id,
        behaviour: 0,
        destructible: 1,
        hp: 100,
        armed: true,
        timer: 2.0,
    };
    [
        wall(110.0, 1),
        wall(590.0, 2),
        side((630.0, 160.0), (95.0, 0.0), (0.0, -1.0), 9025.0, 3),
        side((625.0, 165.0), (0.0, 270.0), (1.0, 0.0), 72900.0, 4),
        side((630.0, 440.0), (95.0, 0.0), (0.0, 1.0), 9025.0, 5),
    ]
}

/// The game's state.
#[derive(Clone)]
pub struct Pong {
    /// The list 0x0071993c: player's paddle (0x00719a00), god's
    /// (0x00719a04), the sarcophagus (0x007198a0 / a4 / a8).
    pub segs: [Seg; 5],
    /// The ball's centre (0x007198c4, 0x007198c8) and velocity
    /// (0x007198bc, 0x007198c0).
    pub ball: (f32, f32),
    pub vel: (f32, f32),
    /// The paddles' moves of this frame (0x00719890 player, 0x00719894
    /// god: 0x0071988c + 4 id).
    pub speeds: [f32; 2],
    /// Player (0x007199e4, 0x007199e8, 0x007199ec), god (0x00719994,
    /// 0x00719998, 0x0071999c), sarcophagus (0x007198ac, 0x007198b0,
    /// 0x007198b4): points, armed, re-arm timer.
    pub player: (i32, bool, f32),
    pub cpu: (i32, bool, f32),
    pub sarco: (i32, bool, f32),
    /// The round's god (0x007199f4): 4, 3, 2.
    pub gods: i32,
    /// The god knocked out (0x007199f9) and its points' regrowth
    /// (0x007199fc, seconds).
    pub knocked: bool,
    pub regrow: f32,
    /// A paddle's end hit while it moved (0x007199f8): the next paddle
    /// bounce leaves at 600 instead of 450.
    pub fast: bool,
    /// The god's paddle moves (0x006b27ad); else its reaction delay runs
    /// (0x006b27b0).
    pub cpu_ready: bool,
    pub cpu_delay: f32,
    /// The map 0x007198e0: the ids in contact at the last test (`None`:
    /// not in the map).
    pub contact: [Option<bool>; 6],
    fond: Option<Arc<Bank>>,
    sarco_bank: Option<Arc<Bank>>,
    bille: Option<Arc<Bank>>,
    ja: Option<Arc<Bank>>,
    dieux: Option<Arc<Bank>>,
}

impl Pong {
    /// The init's state without the engine (tests): the set-up and round
    /// 4 on an 800 × 600 display.
    pub fn new() -> Pong {
        let mut g = Pong {
            segs: segments(),
            ball: (400.0, 300.0),
            vel: (400.0, 200.0),
            speeds: [0.0; 2],
            player: (100, true, 2.0),
            cpu: (100, true, 2.0),
            sarco: (100, true, 2.0),
            gods: 4,
            knocked: false,
            regrow: 0.0,
            fast: false,
            cpu_ready: true,
            cpu_delay: 0.0,
            contact: [None; 6],
            fond: None,
            sarco_bank: None,
            bille: None,
            ja: None,
            dieux: None,
        };
        g.round(4, (800, 600));
        g
    }

    /// FUN_0049b1e0 (its start FUN_0049b760 is the engine's).
    pub fn init(host: &mut dyn Host) -> Pong {
        // 0x007199f0 := 1 (the loaded byte, no reader besides the free).
        host.srand_time();
        let mut g = Pong::new();
        let (w, h) = host.display_size();
        g.ball = ((w / 2) as f32, (h / 2) as f32);
        g.fond = host.bank("pong_fond.spr");
        g.bille = host.bank("bille.spr");
        g.ja = host.bank("F_JA.spr");
        g.dieux = host.bank("dieuxpong.spr");
        g.sarco_bank = host.bank("pong_sarco.spr");
        // FUN_0042df10 (the system cursor centred) and FUN_0042e390.
        host.set_pointer(w / 2, h / 2);
        // FUN_0049cd10; 0x006b27ac := 0 (no reader); round 4.
        g.segs = segments();
        g.gods = 4;
        g.round(4, (w, h));
        host.set_pointer(w / 2, h / 2);
        // 0x006b1d20 / 0x006b1d24 := 800, 600: their initial values.
        host.set_var("PONG_GAGNE", 0);
        host.set_var("PONG_PERDU", 0);
        host.sfx_load("placard.wav");
        host.sfx_load("sound500.wav");
        g
    }

    /// FUN_0049e110(gods): a new round (the pointer centred by the
    /// caller).
    pub fn round(&mut self, gods: i32, display: (i32, i32)) {
        self.gods = gods;
        // 0x007198b8..0x007198dc: written only.
        self.contact = [None; 6];
        self.fast = false;
        self.ball = ((display.0 / 2) as f32, (display.1 / 2) as f32);
        self.vel = (400.0, 200.0);
        self.speeds = [0.0; 2];
        self.segs[0].p = (110.0, 275.0);
        self.segs[1].p = (590.0, 275.0);
        self.player = (100, true, 2.0);
        self.cpu = (100, true, 2.0);
        self.knocked = false;
        self.regrow = 0.0;
        self.sarco = (100, true, 2.0);
        for s in &mut self.segs[2..] {
            s.hp = 100;
            s.armed = true;
            s.timer = 2.0;
        }
        self.cpu_ready = true;
        self.cpu_delay = Self::delay(gods);
    }

    /// The god's reaction delay: `gods × 0.05 + 0.8` (@ 0x0049e325).
    fn delay(gods: i32) -> f32 {
        (f64::from(gods) * f64::from(0.05f32) + f64::from(0.8f32)) as f32
    }

    /// The paddles' speeds by a segment's id (0x0071988c + 4 id).
    fn speed(&self, id: i32) -> f32 {
        match id {
            1 => self.speeds[0],
            2 => self.speeds[1],
            // 0x0071988c and past 0x00719894: not reached (ids 1, 2 only).
            _ => 0.0,
        }
    }

    /// FUN_0049d5c0(ball, velocity, 10, segment, bounce): whether the
    /// ball touches segment `i`; with `bounce`, the velocity changed.
    pub fn collide(&mut self, i: usize, bounce: bool, host: &mut dyn Host) -> bool {
        let s = self.segs[i];
        if s.destructible == 1 && s.hp == 0 {
            return false;
        }
        if s.id == self.segs[1].id && self.knocked {
            return false;
        }
        if s.id >= 3 && !self.knocked {
            return false;
        }
        let (px, py) = self.ball;
        let (px64, py64) = (f64::from(px), f64::from(py));
        let (sx, sy) = (f64::from(s.p.0), f64::from(s.p.1));
        let t_raw = ((py64 - sy) * f64::from(s.d.1) + (px64 - sx) * f64::from(s.d.0)) as f32;
        // n: the normal of the bounce (local 0x2c), dir: its tangent
        // (0x18), t: the nearest point's place on d (0x34).
        let (n, dir, t);
        // An end: n from the end to the ball, dir its perpendicular.
        let end = |v: (f32, f32)| {
            let n = unit_f(v);
            (n, unit_f((n.1, -n.0)))
        };
        if f64::from(t_raw) <= f64::from(0.001f32) {
            let v = ((px64 - sx) as f32, (py64 - sy) as f32);
            (n, dir) = end(v);
            t = 0.0f32;
        } else if f64::from(s.len2) * f64::from(0.99f32) <= f64::from(t_raw) {
            // x from the stored px - sx, y from the stack's py - sy.
            let ex = (f64::from((px64 - sx) as f32) - f64::from(s.d.0)) as f32;
            let ey = ((py64 - sy) - f64::from(s.d.1)) as f32;
            (n, dir) = end((ex, ey));
            t = 1.0;
        } else {
            t = (f64::from(t_raw) / f64::from(s.len2)) as f32;
            dir = s.d;
            n = s.n;
        }
        let td = scale(s.d, t);
        let q = ((f64::from(td.0) + sx) as f32, (f64::from(td.1) + sy) as f32);
        let off = (
            (f64::from(q.0) - px64) as f32,
            (f64::from(q.1) - py64) as f32,
        );
        if f64::from(s.radius + BALL_RADIUS) < len2(off).sqrt() {
            return false;
        }
        if !bounce {
            return true;
        }
        let (vx, vy) = (f64::from(self.vel.0), f64::from(self.vel.1));
        let (nx, ny) = (f64::from(n.0), f64::from(n.1));
        // -(n · v), stored (0x3c).
        let nv = (-(ny * vy + nx * vx)) as f32;
        let proj = (f64::from(dir.1) * vy + f64::from(dir.0) * vx) / len2(dir).sqrt();
        let mid = f64::from(t) > f64::from(0.01f32) && f64::from(t) < f64::from(0.99f32);
        if mid || s.id > 2 {
            // The mirror: the tangent part kept, the normal part turned.
            let dl =
                (f64::from(dir.0) * f64::from(dir.0) + f64::from(dir.1) * f64::from(dir.1)).sqrt();
            let a = scale(dir, (proj / dl) as f32);
            let b = ((nx * f64::from(nv)) as f32, (ny * f64::from(nv)) as f32);
            self.vel = (
                (f64::from(a.0) + f64::from(b.0)) as f32,
                (f64::from(a.1) + f64::from(b.1)) as f32,
            );
            if s.id == 1 && self.vel.0 < 0.0 || s.id == 2 && self.vel.0 > 0.0 {
                self.vel.0 = -self.vel.0;
            }
        } else {
            // A paddle's end: a new direction, the speed kept.
            let speed = len2(self.vel).sqrt() as f32;
            let sp = self.speed(s.id);
            let side = (f64::from(sp) * 15.0) as f32;
            let (x0, k) = if f64::from(sp.abs()) > f64::from(0.0001f32) {
                let r = host.rand();
                let x0 = if s.id == 1 { r % 25 + 50 } else { -50 - r % 25 };
                (x0 as f32, 10.0f32)
            } else {
                ((host.rand() % 10 + 20) as f32, nv)
            };
            let m = scale(n, k);
            self.vel = (
                (f64::from(x0) + f64::from(m.0)) as f32,
                (f64::from(side) + f64::from(m.1)) as f32,
            );
            if s.id == 1 && self.vel.0 < 0.0 || s.id != 1 && self.vel.0 > 0.0 {
                self.vel.0 = -self.vel.0;
            }
            if f64::from(sp.abs()) > f64::from(0.0001f32) {
                self.fast = true;
            }
            let u = unit_f(self.vel);
            self.vel = (
                (f64::from(u.0) * f64::from(speed)) as f32,
                (f64::from(u.1) * f64::from(speed)) as f32,
            );
        }
        if s.destructible == 1 {
            let mut sarco = false;
            for j in 2..5 {
                if s.id == self.segs[j].id {
                    sarco = true;
                    if self.sarco.1 {
                        self.sarco.0 -= 20;
                        self.sarco.1 = false;
                        host.sfx_play("placard.wav");
                        break;
                    }
                }
            }
            if sarco {
                for j in 2..5 {
                    self.segs[j].hp = self.sarco.0;
                }
                let r = f64::from(self.regrow) - 5.0;
                self.regrow = r as f32;
                if r <= 0.0 {
                    self.regrow = 0.0;
                }
                let v = host.var("PONG_FRAPPE_SARCOPHAGE");
                host.set_var("PONG_FRAPPE_SARCOPHAGE", v + 1);
            } else if self.segs[i].armed {
                self.segs[i].armed = false;
                self.segs[i].hp -= 20;
            }
        }
        let cap = |v: &mut (f32, f32), at: f32| {
            let u = unit(*v);
            *v = (
                (f64::from(u.0) * f64::from(at)) as f32,
                (f64::from(u.1) * f64::from(at)) as f32,
            );
        };
        match s.behaviour {
            1 => {
                let u = unit(self.vel);
                let a = (f64::from(u.0) * 150.0, (f64::from(u.1) * 150.0) as f32);
                let x = a.0 + f64::from(self.vel.0);
                let y = f64::from(a.1) + f64::from(self.vel.1);
                self.vel = (x as f32, y as f32);
                if (y * y + x * x).sqrt() >= 600.0 {
                    cap(&mut self.vel, 600.0);
                }
            }
            2 => {
                let (sx, sy) = if nv < 0.0 { (-nx, -ny) } else { (nx, ny) };
                let ay = (sy * 150.0) as f32;
                self.vel.0 = (sx * 150.0 + f64::from(self.vel.0)) as f32;
                self.vel.1 = (f64::from(ay) + f64::from(self.vel.1)) as f32;
                if len2(self.vel).sqrt() >= 600.0 {
                    cap(&mut self.vel, 600.0);
                }
            }
            3 => {
                if mid {
                    let m = (f64::from(t) - 0.5) * 5.0;
                    let a = ((m * f64::from(dir.0)) as f32, (f64::from(dir.1) * m) as f32);
                    let x = f64::from(a.0) + f64::from(self.vel.0);
                    let y = (f64::from(a.1) + f64::from(self.vel.1)) as f32;
                    let push = (f64::from(self.speed(s.id)) * 25.0) as f32;
                    let side = if s.id != 1 { -85.0 } else { 85.0 };
                    self.vel = ((side + x) as f32, (f64::from(push) + f64::from(y)) as f32);
                }
                host.sfx_play("sound500.wav");
                let at = if self.fast { 600.0 } else { 450.0 };
                cap(&mut self.vel, at);
                self.fast = false;
                self.cpu_ready = false;
            }
            _ => {}
        }
        true
    }

    /// FUN_0049d4d0(1): every segment tested in the list's order, a
    /// segment still in contact since the last test only tested.
    pub fn contacts(&mut self, host: &mut dyn Host) -> bool {
        let mut any = false;
        for i in 0..5 {
            let id = self.segs[i].id as usize;
            let bounce = self.contact[id] != Some(true);
            let hit = self.collide(i, bounce, host);
            any |= hit;
            self.contact[id] = Some(hit);
        }
        any
    }

    /// One sub-step of `dt` seconds (@ 0x0049c49c..0x0049c818).
    fn sub_step(&mut self, dt: f32, host: &mut dyn Host) {
        let dt64 = f64::from(dt);
        let dx = (f64::from(self.vel.0) * dt64) as f32;
        self.ball.1 = (f64::from(self.vel.1) * dt64 + f64::from(self.ball.1)) as f32;
        self.ball.0 = (f64::from(dx) + f64::from(self.ball.0)) as f32;
        self.contacts(host);
        let (x, y) = self.ball;
        if x < 95.0 && self.vel.0 < 0.0 {
            self.vel.0 = -self.vel.0;
            self.player.0 -= 20;
            self.player.1 = false;
            host.sfx_play("placard.wav");
        }
        if self.knocked {
            if x > 715.0 && self.vel.0 > 0.0 {
                self.vel.0 = -self.vel.0;
            }
        } else if x > 605.0 && self.vel.0 > 0.0 {
            self.vel.0 = -self.vel.0;
            self.cpu.0 -= 20;
            self.cpu.1 = false;
            host.sfx_play("placard.wav");
        }
        if y < 60.0 && self.vel.1 < 0.0 {
            self.vel.1 = -self.vel.1;
        }
        if y > 537.0 && self.vel.1 > 0.0 {
            self.vel.1 = -self.vel.1;
        }
    }

    /// The god's paddle (@ 0x0049bdd9..0x0049c241).
    fn cpu_move(&mut self, dt: f32, height: i32) {
        let dt64 = f64::from(dt);
        let len = self.segs[1].len();
        let y = &mut self.segs[1].p.1;
        if self.vel.0 >= 0.0 {
            // Towards the ball.
            let c = f64::from(self.ball.1) - len * 0.5;
            let k =
                (dt64 * (f64::from(0.9f32) - f64::from(self.gods) * f64::from(0.05f32))) * 400.0;
            if c > f64::from(*y) {
                *y = (k + f64::from(*y)) as f32;
                self.speeds[1] = k as f32;
            } else if c < f64::from(*y) {
                *y = (f64::from(*y) - k) as f32;
                self.speeds[1] = (-k) as f32;
            }
            if self.speeds[1] < 0.0 && c > f64::from(*y)
                || self.speeds[1] > 0.0 && c < f64::from(*y)
            {
                *y = c as f32;
            }
            if *y < 50.0 {
                *y = 50.0;
            }
            if 550.0 - len < f64::from(*y) {
                *y = (550.0 - len) as f32;
            }
        } else {
            // Towards the middle.
            let c = f64::from(height / 2) - f64::from(len as f32) * 0.5;
            if c > f64::from(*y) {
                *y = (dt64 * 200.0 + f64::from(*y)) as f32;
                self.speeds[1] = (dt64 * 200.0) as f32;
            } else if c < f64::from(*y) {
                *y = (f64::from(*y) - dt64 * 200.0) as f32;
                self.speeds[1] = (dt64 * -200.0) as f32;
            }
            if self.speeds[1] < 0.0 && c > f64::from(*y)
                || self.speeds[1] > 0.0 && c < f64::from(*y)
            {
                *y = c as f32;
            }
        }
    }

    /// The player's paddle from the key code and the pointer's motion
    /// (@ 0x0049bbf1..0x0049bdb5).
    fn player_move(&mut self, dt: f32, key: i32, motion: f32, pointer_y: i32, height: i32) {
        let dt64 = f64::from(dt);
        let len = self.segs[0].len();
        let y = &mut self.segs[0].p.1;
        if key == KEY_DOWN {
            *y = (dt64 * 400.0 + f64::from(*y)) as f32;
            self.speeds[0] = (dt64 * 400.0) as f32;
            if f64::from(*y) >= 544.0 - len {
                *y = (544.0 - len) as f32;
            }
        } else if key == KEY_UP {
            *y = (dt64 * -400.0 + f64::from(*y)) as f32;
            self.speeds[0] = (dt64 * -400.0) as f32;
            if *y <= 53.0 {
                *y = 53.0;
            }
        }
        if f64::from(motion.abs()) > f64::from(0.01f32) {
            self.speeds[0] = motion;
            let lf = len as f32;
            let tmp = ((f64::from(height) - f64::from(lf)) * f64::from(pointer_y)) as f32;
            *y = (f64::from(tmp) / f64::from(height)) as f32;
            if *y <= 53.0 {
                *y = 53.0;
            }
            if f64::from(*y) >= 544.0 - len {
                *y = (544.0 - len) as f32;
            }
        }
    }

    /// The frame's game part after the end tests: rounds, paddles, ball.
    /// False when the frame stops before drawing (a variable set).
    pub fn step(&mut self, host: &mut dyn Host) -> bool {
        let dt = host.frame_time();
        let dt64 = f64::from(dt);
        if self.player.0 == 0 {
            host.set_var("PONG_PERDU", 1);
            return false;
        }
        if self.knocked {
            let s = dt64 + f64::from(self.regrow);
            self.regrow = s as f32;
            self.cpu.0 = ftol_d(s * f64::from(3.333_333_3f32));
            if self.cpu.0 >= 100 {
                self.cpu.0 = 100;
                if self.ball.0 < 600.0 {
                    self.regrow = 0.0;
                    self.knocked = false;
                }
            }
        } else if self.cpu.0 == 0 {
            self.knocked = true;
        }
        if self.sarco.0 == 0 {
            if let Some(v) = GOD_VARS.get(self.gods as usize) {
                host.set_var(v, host.var(v) + 1);
            }
            self.gods -= 1;
            if self.gods < 2 {
                self.gods = 2;
                host.set_var("PONG_GAGNE", 1);
                return false;
            }
            let d = host.display_size();
            self.round(self.gods, d);
            // FUN_0042df10, FUN_0042e390.
            host.set_pointer(d.0 / 2, d.1 / 2);
        }
        self.speeds = [0.0; 2];
        let height = host.display_size().1;
        let motion = host.mouse_dy();
        self.player_move(dt, host.key_code(), motion, host.pointer().1, height);
        if f64::from(motion.abs()) > f64::from(0.01f32) {
            host.set_mouse_dy(0.0);
        }
        if !self.knocked {
            if !self.cpu_ready {
                let r = f64::from(self.cpu_delay) - dt64;
                self.cpu_delay = r as f32;
                if r < 0.0 {
                    self.cpu_ready = true;
                    self.cpu_delay = Self::delay(self.gods);
                }
            } else {
                self.cpu_move(dt, height);
            }
        }
        // The ball gathers speed up to 450 across.
        if f64::from(self.vel.0.abs()) < 450.0 {
            let speed = len2(self.vel).sqrt() as f32;
            self.vel.0 = if self.vel.0 > 0.0 {
                (dt64 * 70.0 + f64::from(self.vel.0)) as f32
            } else {
                (f64::from(self.vel.0) - dt64 * 70.0) as f32
            };
            let u = unit(self.vel);
            self.vel = (
                (f64::from(u.0) * f64::from(speed)) as f32,
                (f64::from(u.1) * f64::from(speed)) as f32,
            );
        }
        // The re-arm timers: the destructible segments, the sarcophagus,
        // the player, the god.
        let rearm = |armed: &mut bool, timer: &mut f32| {
            if !*armed {
                let r = f64::from(*timer) - dt64;
                *timer = r as f32;
                if r < 0.0 {
                    *timer = 2.0;
                    *armed = true;
                }
            }
        };
        for s in &mut self.segs {
            if s.destructible == 1 {
                rearm(&mut s.armed, &mut s.timer);
            }
        }
        rearm(&mut self.sarco.1, &mut self.sarco.2);
        rearm(&mut self.player.1, &mut self.player.2);
        rearm(&mut self.cpu.1, &mut self.cpu.2);
        let step = if self.ball.0 > 200.0 && self.ball.0 < 500.0 {
            0.03f32
        } else {
            0.005f32
        };
        let mut rem = dt;
        if dt >= step {
            loop {
                self.sub_step(step, host);
                let r = f64::from(rem) - f64::from(step);
                rem = r as f32;
                if r < f64::from(step) {
                    break;
                }
            }
        }
        if rem >= 0.0 {
            self.sub_step(rem, host);
        }
        if self.ball.0 < -100.0 || self.ball.0 > 900.0 {
            self.ball.0 = 400.0;
        }
        if self.ball.1 < -100.0 || self.ball.1 > 700.0 {
            self.ball.1 = 300.0;
        }
        true
    }

    /// FUN_0049d0e0.
    fn draw(&self, host: &mut dyn Host) {
        let f = self.fond.clone();
        let fb = f.as_ref();
        aff(host, "pong_fond.spr", fb, 0, Some(0), 0, 1.0);
        // The sarcophagus's frame: 4 - (points - 1) / 20 (C division).
        let k = (self.sarco.0 - 1) / 20;
        let frame = ftol_d(f64::from(4.0f32) - f64::from(k));
        aff(
            host,
            "pong_sarco.spr",
            self.sarco_bank.as_ref(),
            frame.max(0) as usize,
            Some(0x262),
            0xa0,
            1.0,
        );
        aff(host, "pong_fond.spr", fb, 1, Some(0x5c), 0x41, 1.0);
        if !self.knocked {
            aff(host, "pong_fond.spr", fb, 1, Some(0x262), 0x41, 1.0);
        }
        let p = self.segs[0].p;
        aff(
            host,
            "pong_fond.spr",
            fb,
            3,
            Some(ftol(p.0)),
            ftol(p.1),
            1.0,
        );
        if !self.knocked {
            let p = self.segs[1].p;
            aff(
                host,
                "pong_fond.spr",
                fb,
                2,
                Some(ftol(p.0)),
                ftol(p.1),
                1.0,
            );
        }
        aff(
            host,
            "pong_fond.spr",
            fb,
            4,
            Some(ftol(self.ball.0)),
            ftol(self.ball.1),
            1.0,
        );
        // The segments' outlines and the points' texts (0x006e0518) are
        // never drawn: the flag is only written with 0. No pointer.
        host.flip();
    }
}

impl Default for Pong {
    fn default() -> Pong {
        Pong::new()
    }
}

impl Puzzle for Pong {
    fn id(&self) -> PuzzleId {
        PuzzleId::Pong
    }

    /// FUN_0049b9a0.
    fn frame(&mut self, host: &mut dyn Host) {
        if !host.script_prologue() {
            return;
        }
        if host.var("PONG_GAGNE") != 0 || host.var("PONG_PERDU") != 0 {
            return;
        }
        if self.step(host) {
            self.draw(host);
        }
    }

    /// FUN_0049b7b0's game part.
    fn free(&mut self, host: &mut dyn Host) {
        self.fond = None;
        self.sarco_bank = None;
        self.bille = None;
        self.ja = None;
        self.dieux = None;
        host.sfx_remove("placard.wav");
        host.sfx_remove("sound500.wav");
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
    fn setup() {
        let g = Pong::new();
        assert_eq!(g.segs[0].p, (110.0, 275.0));
        assert_eq!(g.segs[1].p, (590.0, 275.0));
        assert_eq!(g.segs[0].len(), 100.0);
        assert_eq!(g.segs.map(|s| s.id), [1, 2, 3, 4, 5]);
        assert_eq!(g.segs.map(|s| s.radius), [2, 2, 5, 5, 5]);
        assert_eq!((g.ball, g.vel), ((400.0, 300.0), (400.0, 200.0)));
        assert_eq!((g.gods, g.cpu_delay, g.cpu_ready), (4, 1.0, true));
        assert_eq!(Pong::delay(4), 1.0);
    }

    /// The sarcophagus is out of reach while the god plays, its paddle
    /// while the god is knocked out.
    #[test]
    fn reach() {
        let mut g = Pong::new();
        let mut h = SimHost::new(0.04);
        g.ball = (622.0, 300.0);
        assert!(!g.collide(3, false, &mut h));
        g.knocked = true;
        assert!(g.collide(3, false, &mut h));
        g.ball = (590.0, 300.0);
        assert!(!g.collide(1, false, &mut h));
        g.knocked = false;
        assert!(g.collide(1, false, &mut h));
    }

    /// A ball in the middle of the player's paddle comes back at 450, a
    /// sound; a second test in contact does not bounce again.
    #[test]
    fn paddle_bounce() {
        let mut g = Pong::new();
        let mut h = SimHost::new(0.04);
        g.ball = (120.0, 325.0);
        g.vel = (-450.0, 0.0);
        g.contacts(&mut h);
        assert!(g.vel.0 > 0.0);
        assert!((len2(g.vel).sqrt() - 450.0).abs() < 0.01);
        assert_eq!(h.sounds, ["sound500.wav"]);
        assert!(!g.cpu_ready);
        assert_eq!(g.contact[1], Some(true));
        let v = g.vel;
        g.contacts(&mut h);
        assert_eq!(g.vel, v);
        assert_eq!(h.sounds.len(), 1);
    }

    /// The god knocked out at 0, back at 100 (3.33 a second) with the
    /// ball left of 600.
    #[test]
    fn knocked_out() {
        let mut g = Pong::new();
        let mut h = SimHost::new(0.04);
        g.cpu.0 = 0;
        g.step(&mut h);
        assert!(g.knocked);
        g.regrow = 29.98;
        g.ball = (650.0, 500.0);
        g.vel = (-450.0, 0.0);
        g.step(&mut h);
        assert_eq!(g.cpu.0, 100);
        assert!(g.knocked);
        g.ball = (500.0, 500.0);
        g.step(&mut h);
        assert!(!g.knocked);
        assert_eq!(g.regrow, 0.0);
    }

    /// The rounds: god 4 (no variable), 3 (Sama), 2 (Kama, then won).
    #[test]
    fn rounds() {
        let mut g = Pong::new();
        let mut h = SimHost::new(0.04);
        g.sarco.0 = 0;
        assert!(g.step(&mut h));
        assert_eq!((g.gods, g.sarco.0, g.cpu_delay), (3, 100, Pong::delay(3)));
        g.sarco.0 = 0;
        g.step(&mut h);
        assert_eq!((g.gods, h.var("PONG_SAMA_DESACTIVE")), (2, 1));
        g.sarco.0 = 0;
        assert!(!g.step(&mut h));
        assert_eq!(h.var("PONG_KAMA_DESACTIVE"), 1);
        assert_eq!((g.gods, h.var("PONG_GAGNE")), (2, 1));
        let mut g = Pong::new();
        g.player.0 = 0;
        assert!(!g.step(&mut h));
        assert_eq!(h.var("PONG_PERDU"), 1);
    }

    /// The pointer places the paddle: (600 - 100) × y / 600, within
    /// 53..444; the motion is cleared.
    #[test]
    fn pointer() {
        let mut g = Pong::new();
        let mut h = SimHost::new(0.0);
        for (py, want) in [(300, 250.0), (0, 53.0), (599, 444.0)] {
            h.pointer = (400, py);
            h.mouse_dy = 3.0;
            g.step(&mut h);
            assert_eq!(g.segs[0].p.1, want);
            assert_eq!(h.mouse_dy, 0.0);
            assert_eq!(g.speeds[0], 3.0);
        }
        // The keys: 400 px a second.
        h.dt = 0.1;
        h.key_code = KEY_UP;
        g.step(&mut h);
        assert_eq!(g.segs[0].p.1, 404.0);
    }
}

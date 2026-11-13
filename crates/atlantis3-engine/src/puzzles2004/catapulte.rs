//! `PUZ_CATAPULTE` (loop mode 0x14): two catapults shoot over a wall at
//! each other's tower block (atlantis4-cd-en-3022848: init FUN_00483690,
//! start FUN_00483e00, free FUN_00483e50, frame FUN_00484180, draw
//! FUN_00485000). The CD starts it in chapter 110 (/030, the village's
//! console); the player wins by breaking the right block, which sets
//! `FLAG21` (0x0070dc30), and leaves through the screen's exit button,
//! which sets `CATA_CLIC_EXIT` := 1 for the chapter's `/puz` entries
//! /042 (`FLAG21` = 1: `FINPUZZLE`, the film `CURTIS_DIEU`, chapter 11)
//! and /043 (`FINPUZZLE` only). The chapter's other `/puz` entries
//! (`CATA_CLIC_VISEUR`, `CATA_SEQ_*`, `CATA_CHANGEMENT*`: a "mastermind")
//! test variables this program never sets [CONFIRMED-STATIC: no string].
//!
//! Banks (`spr\`): `Cata_Ecran1.spr` (0x00717050: the console screen,
//! #0 #1 the picture, #2 the target mark, #3 the exit, #4 the play
//! button), `Cata_Back.spr` (0x00717068), `Cata_mur.spr` (0x00717038,
//! the wall), `Cata_bloc.spr` (0x00716fa8, the blocks: frame = hits),
//! `Cata_cata.spr` (0x00716f88: #0 the left catapult, #1 the right one,
//! #2 the ball), `Boutpuz2.spr` (0x00717080, the back button),
//! `Cata_feu.spr` (0x00717020, the impact's fire), `Cata_ciel.spr`
//! (0x00716ff0, the clouds). Sounds (`sfx\`, loaded by the init):
//! `cata_tir.wav` (a shot), `cata_boulet.wav` (the ball at the top of its
//! arc), `cata_exp.wav` (the ground or the wall), `cata_exp2.wav` (a
//! block).
//!
//! Floats: the program's x87 runs with precision 53 bits (0x027f, as the
//! other 2004 ports assume): a value kept on the x87 stack is an `f64`
//! here, one stored to memory an `f32`.

use std::sync::Arc;

use atlantis3_data::spr::Bank;

use super::{aff, collision, ftol, ftol_d, Host, Puzzle, PuzzleId};
use crate::px::Surface;

/// The game's state (`0x006b1e54`; FUN_00484fb0 keeps the previous one in
/// 0x007170b4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// 1: the console screen.
    Console = 1,
    /// 2: the player aims (the ball on the left catapult).
    Aim = 2,
    /// 3: the player's ball flies.
    PlayerShot = 3,
    /// 4: an impact burns for a second.
    Impact = 4,
    /// 5: the right catapult's ball flies.
    EnemyShot = 5,
    /// 6: nothing (no path sets it).
    Idle = 6,
}

/// A cloud (0x00716fc0 + 12 i, four): position and frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cloud {
    pub x: f32,
    pub y: f32,
    pub frame: usize,
}

/// FUN_00485410: the clouds' starting positions.
pub const CLOUDS: [Cloud; 4] = [
    Cloud {
        x: 200.0,
        y: 40.0,
        frame: 0,
    },
    Cloud {
        x: 550.0,
        y: 70.0,
        frame: 1,
    },
    Cloud {
        x: -50.0,
        y: 90.0,
        frame: 2,
    },
    Cloud {
        x: 350.0,
        y: 150.0,
        frame: 3,
    },
];

/// The left catapult's ball start (0x42fa0000, 0x44058000) and the
/// right one's (0x4428c000, 0x44058000).
pub const PLAYER_START: (f32, f32) = (125.0, 534.0);
pub const ENEMY_START: (f32, f32) = (675.0, 534.0);

/// The wall's pieces (`Cata_mur` #0 at x 390, y 550, 510, .. 270: the
/// draw @ 0x004852f7 and the test @ 0x0048498c).
pub const WALL_X: i32 = 390;
pub fn wall_ys() -> impl Iterator<Item = i32> {
    (270..=550).rev().step_by(40)
}

/// The blocks' places (`Cata_bloc`, frame = hits): left (10, 550), right
/// (670, 550).
pub const BLOCK_LEFT: (i32, i32) = (10, 550);
pub const BLOCK_RIGHT: (i32, i32) = (670, 550);

/// The target mark's places (0x00716f80): before the win (248, 295),
/// after (548, 328).
pub const MARK_BEFORE: (f32, f32) = (248.0, 295.0);
pub const MARK_AFTER: (f32, f32) = (548.0, 328.0);

/// What ended a flight step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    /// The ground (y ≥ 550) or the wall: `cata_exp`.
    Ground,
    /// A block hit, not broken: `cata_exp2`.
    Block,
    /// The left block broken (its fourth hit): the round starts again.
    LeftBroken,
    /// The right block broken: won.
    RightBroken,
}

/// The ball in flight (0x00716f58 velocity, 0x00716f60 position,
/// 0x00716f68 acceleration, 0x00716f78 damping).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ball {
    pub vel: (f32, f32),
    pub pos: (f32, f32),
    pub acc: (f32, f32),
    pub damp: f32,
}

/// The sprites a flight tests.
pub struct Field<'a> {
    pub bloc: Option<&'a Arc<Bank>>,
    pub mur: Option<&'a Arc<Bank>>,
    /// The blocks' frames (0x007170b8, 0x007170bc).
    pub hits: (i32, i32),
}

/// The events of one flight step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepEvents {
    /// The ball passed the top of its arc: `cata_boulet`.
    pub apex: bool,
    /// Which block it touched (1 left, 2 right), if any.
    pub block: Option<u8>,
    /// It landed (the ground, the wall, or a block).
    pub landed: bool,
}

impl Ball {
    /// One sub-step of `step` seconds (FUN_00484180 @ 0x00484674..
    /// 0x004848a5): the position, the damping, the drag and the wind, the
    /// apex test, the velocity.
    pub fn advance(&mut self, step: f32, wind: f32) -> bool {
        let step64 = f64::from(step);
        // @ 0x00484674: x through a stored f32 product, y on the stack.
        let dx = (f64::from(self.vel.0) * step64) as f32;
        let y = (f64::from(self.vel.1) * step64 + f64::from(self.pos.1)) as f32;
        let x = (f64::from(dx) + f64::from(self.pos.0)) as f32;
        self.pos = (x, y);
        let vy = (f64::from(self.vel.1) * f64::from(self.damp)) as f32;
        let vx = (f64::from(self.vel.0) * f64::from(self.damp)) as f32;
        self.vel = (vx, vy);
        // @ 0x004846df: the drag opposes the horizontal motion, the wind
        // adds; FUN_004855d0 = y² + x².
        let len2 = f64::from(vy) * f64::from(vy) + f64::from(vx) * f64::from(vx);
        let inv = 1.0 / len2.sqrt();
        let nx = (inv * f64::from(vx)) as f32;
        let drag = if vx > 0.0 { -40.0 } else { 40.0 };
        self.acc = ((f64::from(nx) * drag + f64::from(wind)) as f32, 210.0);
        let ay = (f64::from(self.acc.1) * step64) as f32;
        let ax = (f64::from(self.acc.0) * step64) as f32;
        // @ 0x004847eb: going up, and up no more after this step.
        let apex = vy < 0.0 && f64::from(ay) + f64::from(vy) >= 0.0;
        self.vel = (
            (f64::from(ax) + f64::from(vx)) as f32,
            (f64::from(ay) + f64::from(vy)) as f32,
        );
        apex
    }

    /// The flight's collision tests after a sub-step (@ 0x004848ab..
    /// 0x004849cd): the left block, else the right block; then, nothing
    /// touched and y < 550, the wall's eight pieces. `Some(block)` when it
    /// landed (`block` the block touched).
    pub fn hits(&self, f: &Field) -> Option<Option<u8>> {
        let (ix, iy) = (ftol(self.pos.0), ftol(self.pos.1));
        if collision(
            f.bloc,
            f.hits.0 as usize,
            Some(BLOCK_LEFT.0),
            BLOCK_LEFT.1,
            ix,
            iy,
        ) {
            return Some(Some(1));
        }
        if collision(
            f.bloc,
            f.hits.1 as usize,
            Some(BLOCK_RIGHT.0),
            BLOCK_RIGHT.1,
            ix,
            iy,
        ) {
            return Some(Some(2));
        }
        if self.pos.1 < 550.0 {
            let wall = (270..=550)
                .step_by(40)
                .any(|y| collision(f.mur, 0, Some(WALL_X), y, ix, iy));
            if !wall {
                return None;
            }
        }
        Some(None)
    }
}

/// The player's ball's velocity toward `target` from `pos` (@ 0x0048452c..
/// 0x0048460b): twice the difference, at most 500 long.
pub fn aim_velocity(pos: (f32, f32), target: (f32, f32)) -> (f32, f32) {
    let dx = (f64::from(target.0) - f64::from(pos.0)) as f32;
    let dy = (f64::from(target.1) - f64::from(pos.1)) as f32;
    let mut vel = (
        (f64::from(dx) + f64::from(dx)) as f32,
        (f64::from(dy) + f64::from(dy)) as f32,
    );
    let (vx, vy) = (f64::from(vel.0), f64::from(vel.1));
    if (vx * vx + vy * vy).sqrt() > 500.0 {
        let inv = 1.0 / (vy * vy + vx * vx).sqrt();
        let ny = (inv * vy) as f32;
        let nx = (inv * vx) as f32;
        vel = (
            (f64::from(nx) * 500.0) as f32,
            (f64::from(ny) * 500.0) as f32,
        );
    }
    vel
}

/// Where a ball lands when every frame lasts `frame_t` (the flight's
/// sub-steps of FUN_00484180 without its effects): the landing position
/// and the block touched, `None` when it flies longer than `frames`.
pub fn landing(
    mut ball: Ball,
    wind: f32,
    field: &Field,
    frame_t: f32,
    frames: usize,
) -> Option<((f32, f32), Option<u8>)> {
    for _ in 0..frames {
        let mut t = frame_t;
        while f64::from(t) > 0.0001f32 as f64 {
            let step = if t < 0.02 { t } else { 0.02f32 };
            ball.advance(step, wind);
            if let Some(b) = ball.hits(field) {
                return Some((ball.pos, b));
            }
            t = (f64::from(t) - f64::from(step)) as f32;
        }
    }
    None
}

/// The game.
pub struct Catapulte {
    pub state: State,
    /// The previous state (0x007170b4; `.bss` 0 before any change).
    pub prev: i32,
    /// 0x006b1e50: 1 the console, 2 the field. Its .data value is 1 and
    /// every way out of the field sets 1 (the exit is only clickable
    /// there), so a start always finds 1.
    pub screen: i32,
    pub ball: Ball,
    /// The player's target (0x00716f70).
    pub target: (f32, f32),
    /// The wind (0x006b1e4c: 10.0 in .data, rand() % 20 − 10 at each
    /// round).
    pub wind: f32,
    /// The blocks' hits (0x007170b8 left, 0x007170bc right; FUN_00485400
    /// clears them).
    pub hits: (i32, i32),
    /// The right block broken (0x007170b2).
    pub won: bool,
    /// The target mark (0x00716f80).
    pub mark: (f32, f32),
    pub clouds: [Cloud; 4],
    /// The impacts (the vector 0x00716f48: points where balls landed,
    /// drawn burning during state 4).
    pub impacts: Vec<(f32, f32)>,
    /// The 1 s timer of state 4 (0x00717098 start, 0x007170a8 kept).
    pub timer: (f64, f64),
    /// The play clock started (0x007170b1) and its start and sum
    /// (0x00717008, 0x00717010: no reader [UNKNOWN]).
    pub clock_on: bool,
    pub play_time: (f64, f64),
    banks: Banks,
}

#[derive(Default, Clone)]
struct Banks {
    ecran: Option<Arc<Bank>>,
    back: Option<Arc<Bank>>,
    mur: Option<Arc<Bank>>,
    bloc: Option<Arc<Bank>>,
    cata: Option<Arc<Bank>>,
    bout: Option<Arc<Bank>>,
    feu: Option<Arc<Bank>>,
    ciel: Option<Arc<Bank>>,
}

const SFX: [&str; 4] = [
    "cata_tir.wav",
    "cata_boulet.wav",
    "cata_exp.wav",
    "cata_exp2.wav",
];

impl Catapulte {
    /// FUN_00483690: the loaded byte (0x007170b0), `srand(time(0))`, the
    /// banks, the pointer centred (FUN_0042e390), cursor #29, state 1,
    /// blocks cleared, clouds placed, the target mark by `FLAG21`, the won
    /// byte cleared, `CATA_CLIC_EXIT` := 0, the sounds loaded.
    pub fn init(host: &mut dyn Host) -> Catapulte {
        host.srand_time();
        let banks = Banks {
            ecran: host.bank("Cata_Ecran1.spr"),
            back: host.bank("Cata_Back.spr"),
            mur: host.bank("Cata_mur.spr"),
            bloc: host.bank("Cata_bloc.spr"),
            cata: host.bank("Cata_cata.spr"),
            bout: host.bank("Boutpuz2.spr"),
            feu: host.bank("Cata_feu.spr"),
            ciel: host.bank("Cata_ciel.spr"),
        };
        // FUN_0042e390: the pointer at the display's centre.
        let (w, h) = host.display_size();
        host.set_pointer(w / 2, h / 2);
        let mark = if host.flag(21) == 0 {
            MARK_BEFORE
        } else {
            MARK_AFTER
        };
        host.set_var("CATA_CLIC_EXIT", 0);
        for s in SFX {
            host.sfx_load(s);
        }
        Catapulte {
            state: State::Console,
            prev: 0,
            screen: 1,
            ball: Ball {
                vel: (0.0, 0.0),
                pos: (0.0, 0.0),
                acc: (0.0, 0.0),
                damp: 0.0,
            },
            target: (0.0, 0.0),
            wind: 10.0,
            hits: (0, 0),
            won: false,
            mark,
            clouds: CLOUDS,
            impacts: Vec::new(),
            timer: (0.0, 0.0),
            clock_on: false,
            play_time: (0.0, 0.0),
            banks,
        }
    }

    /// FUN_00484fb0.
    fn set_state(&mut self, s: State) {
        self.prev = self.state as i32;
        self.state = s;
    }

    /// The timer restarted with the clock (@ 0x004849de..0x00484a26).
    fn restart_timer(&mut self, host: &dyn Host) {
        self.timer = (host.clock(), 0.0);
    }

    /// `rand() % m - k` as a float (the `idiv` remainder: rand ≥ 0).
    fn roll(host: &mut dyn Host, m: i32, k: i32) -> f32 {
        (host.rand() % m - k) as f32
    }

    /// FUN_004854d0(block): true and one more hit while the block has
    /// fewer than 3, false (broken) at 3.
    fn hit_block(&mut self, block: u8) -> bool {
        let h = if block == 1 {
            &mut self.hits.0
        } else {
            &mut self.hits.1
        };
        if *h != 3 {
            *h += 1;
            true
        } else {
            false
        }
    }

    /// The flight (states 3 and 5, @ 0x00484629..0x00484b67). False when
    /// the frame ends there without drawing (a block broken).
    fn fly(&mut self, host: &mut dyn Host) -> bool {
        if host.click() {
            host.set_click(false);
        }
        let mut t = host.frame_time();
        if f64::from(t) <= 0.0001f32 as f64 {
            return true;
        }
        loop {
            let step = if t < 0.02 { t } else { 0.02f32 };
            if self.ball.advance(step, self.wind) {
                host.sfx_play("cata_boulet.wav");
            }
            let field = Field {
                bloc: self.banks.bloc.as_ref(),
                mur: self.banks.mur.as_ref(),
                hits: self.hits,
            };
            if let Some(block) = self.ball.hits(&field) {
                match block {
                    Some(1) if !self.hit_block(1) => {
                        // @ 0x00484b6c: the left block broken: both
                        // cleared, the player aims again, no draw.
                        self.hits = (0, 0);
                        self.set_state(State::Aim);
                        return false;
                    }
                    Some(2) if !self.hit_block(2) => {
                        // @ 0x00484b91: won.
                        self.won = true;
                        host.set_flag(21, 1);
                        self.restart_timer(host);
                        self.set_state(State::Impact);
                        self.impacts.push(self.ball.pos);
                        return false;
                    }
                    _ => {}
                }
                // @ 0x004849d6: landed.
                t = 0.0;
                self.restart_timer(host);
                self.set_state(State::Impact);
                self.impacts.push(self.ball.pos);
                host.sfx_play(if block.is_some() {
                    "cata_exp2.wav"
                } else {
                    "cata_exp.wav"
                });
            }
            t = (f64::from(t) - f64::from(step)) as f32;
            if f64::from(t) <= 0.0001f32 as f64 {
                return true;
            }
        }
    }

    /// The enemy's shot (@ 0x00484d55): from the right catapult,
    /// vy = rand() % 60 − 470, vx = 130000 / (3 vy) + rand() % 45 − 20 −
    /// 2 · wind.
    fn enemy_shot(&mut self, host: &mut dyn Host) {
        self.set_state(State::EnemyShot);
        self.impacts.clear();
        self.ball.pos = ENEMY_START;
        let vy = Self::roll(host, 60, 0x1d6);
        self.ball.vel.1 = vy;
        let r = host.rand() % 45;
        let w = f64::from(self.wind);
        let v = 130000.0 / (f64::from(vy) * 3.0) + f64::from(r) - 20.0;
        self.ball.vel.0 = (v - (w + w)) as f32;
        self.ball.damp = 1.0;
        self.ball.acc = (0.0, 0.0);
        host.sfx_play("cata_tir.wav");
    }

    /// The player's shot (@ 0x0048449f..0x00484624) at the pointer.
    fn player_shot(&mut self, host: &mut dyn Host) {
        host.sfx_play("cata_tir.wav");
        host.set_click(false);
        let (px, py) = host.pointer();
        self.target = (px as f32, py as f32);
        self.set_state(State::PlayerShot);
        self.ball.vel = aim_velocity(self.ball.pos, self.target);
        self.ball.damp = 1.0;
        self.ball.acc = (0.0, 0.0);
    }

    /// The draw FUN_00485000 into the stretch surface, then the pointer
    /// and the flip.
    fn draw(&self, host: &mut dyn Host) {
        let b = &self.banks;
        if self.screen == 1 {
            aff(host, "Cata_Ecran1", b.ecran.as_ref(), 0, None, 0, 1.0);
            aff(host, "Cata_Ecran1", b.ecran.as_ref(), 1, None, 0, 1.0);
            let (mx, my) = (ftol(self.mark.0), ftol(self.mark.1));
            aff(host, "Cata_Ecran1", b.ecran.as_ref(), 2, Some(mx), my, 1.0);
            aff(host, "Cata_Ecran1", b.ecran.as_ref(), 3, None, 0, 1.0);
            aff(host, "Cata_Ecran1", b.ecran.as_ref(), 4, None, 0, 1.0);
        } else {
            aff(host, "Cata_Back", b.back.as_ref(), 0, Some(0), 0, 1.0);
            let (l, r) = (self.hits.0 as usize, self.hits.1 as usize);
            aff(
                host,
                "Cata_bloc",
                b.bloc.as_ref(),
                l,
                Some(BLOCK_LEFT.0),
                BLOCK_LEFT.1,
                1.0,
            );
            if !self.won {
                aff(
                    host,
                    "Cata_bloc",
                    b.bloc.as_ref(),
                    r,
                    Some(BLOCK_RIGHT.0),
                    BLOCK_RIGHT.1,
                    1.0,
                );
            }
            aff(host, "Cata_cata", b.cata.as_ref(), 0, Some(125), 534, 1.0);
            aff(host, "Cata_cata", b.cata.as_ref(), 1, Some(675), 534, 1.0);
            if matches!(self.state, State::PlayerShot | State::EnemyShot) {
                let (x, y) = (ftol(self.ball.pos.0), ftol(self.ball.pos.1));
                aff(host, "Cata_cata", b.cata.as_ref(), 2, Some(x), y, 1.0);
            }
            for c in &self.clouds {
                aff(
                    host,
                    "Cata_ciel",
                    b.ciel.as_ref(),
                    c.frame,
                    Some(ftol(c.x)),
                    ftol(c.y),
                    1.0,
                );
            }
            if self.state == State::Aim {
                // FUN_004c3720: a white line from the ball to the pointer.
                let from = (ftol(self.ball.pos.0), ftol(self.ball.pos.1));
                let to = host.pointer();
                if host.tracing() {
                    host.trace(format!("line {} {} {} {}", from.0, from.1, to.0, to.1));
                }
                line(host.surface(), from, to);
            }
            for y in wall_ys() {
                aff(host, "Cata_mur", b.mur.as_ref(), 0, Some(WALL_X), y, 1.0);
            }
            if self.state == State::Impact && !self.impacts.is_empty() {
                // The fire's frame: 11 a second since the impact.
                let f = ftol_d((host.clock() - self.timer.0 + self.timer.1) * 11.0);
                for &(x, y) in &self.impacts {
                    if let Ok(f) = usize::try_from(f) {
                        aff(
                            host,
                            "Cata_feu",
                            b.feu.as_ref(),
                            f,
                            Some(ftol(x)),
                            ftol(y),
                            1.0,
                        );
                    }
                }
            }
            aff(host, "Boutpuz2", b.bout.as_ref(), 0, Some(50), 50, 1.0);
            host.draw_pointer();
            host.flip();
            return;
        }
        host.draw_pointer();
        host.flip();
    }
}

/// FUN_004c3720 → FUN_004c3760 (32 bits FUN_004c4fb0, 16 bits
/// FUN_004c4c20): a white Bresenham line, clipped to the surface
/// (FUN_004c3200; the game's ends are always inside it).
pub fn line(s: &mut Surface, from: (i32, i32), to: (i32, i32)) {
    let (w, h) = (super::WIDTH as i32, super::HEIGHT as i32);
    let inside = |p: (i32, i32)| p.0 >= 0 && p.1 >= 0 && p.0 < w && p.1 < h;
    if !inside(from) || !inside(to) {
        return;
    }
    let mut plot = |x: i32, y: i32| {
        let i = (y * w + x) as usize;
        match s {
            Surface::Rgb565(p) => p[i] = 0xffff,
            Surface::Xrgb8888(p) => p[i] = 0xffff_ffff,
        }
    };
    let (mut x0, mut y0, mut x1, mut y1) = (from.0, from.1, to.0, to.1);
    if (x1 - x0).abs() < (y1 - y0).abs() {
        if y1 < y0 {
            std::mem::swap(&mut y0, &mut y1);
            std::mem::swap(&mut x0, &mut x1);
        }
        let step = if x1 <= x0 { -1 } else { 1 };
        let dx = (x1 - x0).abs();
        let dy = y1 - y0;
        let mut e = 2 * dx - dy;
        let mut x = x0;
        plot(x, y0);
        for y in y0 + 1..=y1 {
            let inc = if e >= 0 {
                x += step;
                (dx - dy) * 2
            } else {
                2 * dx
            };
            e += inc;
            plot(x, y);
        }
    } else {
        if x1 < x0 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
        }
        let step = if y1 <= y0 { -1 } else { 1 };
        let dy = (y1 - y0).abs();
        let dx = x1 - x0;
        let mut e = 2 * dy - dx;
        let mut y = y0;
        plot(x0, y);
        for x in x0 + 1..=x1 {
            let inc = if e >= 0 {
                y += step;
                (dy - dx) * 2
            } else {
                2 * dy
            };
            e += inc;
            plot(x, y);
        }
    }
}

impl Puzzle for Catapulte {
    fn id(&self) -> PuzzleId {
        PuzzleId::Catapulte
    }

    fn frame(&mut self, host: &mut dyn Host) {
        if !host.script_prologue() {
            return;
        }
        if host.var("CATA_CLIC_EXIT") != 0 {
            return;
        }
        // @ 0x0048425d: the back button (field only).
        if self.screen == 2
            && host.click()
            && collision(
                self.banks.bout.as_ref(),
                0,
                Some(50),
                50,
                host.pointer().0,
                host.pointer().1,
            )
        {
            host.set_click(false);
            self.screen = 1;
            self.set_state(State::Console);
            if self.won {
                self.mark = MARK_AFTER;
            }
        }
        let click = host.click();
        let (px, py) = host.pointer();
        match self.state {
            State::Console => {
                if click {
                    let e = self.banks.ecran.as_ref();
                    if collision(e, 3, None, 0, px, py) {
                        host.set_var("CATA_CLIC_EXIT", 1);
                        host.set_click(false);
                        return;
                    }
                    if collision(e, 4, None, 0, px, py) && host.flag(21) == 0 {
                        host.set_click(false);
                        self.screen = 2;
                        self.set_state(State::Aim);
                        self.wind = Self::roll(host, 20, 10);
                        if !self.clock_on {
                            self.clock_on = true;
                            self.play_time = (host.clock(), 0.0);
                        }
                    } else {
                        host.set_click(false);
                    }
                }
            }
            State::Aim => {
                self.ball.pos = PLAYER_START;
                self.ball.vel = (0.0, 0.0);
                if click {
                    self.player_shot(host);
                }
            }
            State::PlayerShot | State::EnemyShot => {
                if !self.fly(host) {
                    return;
                }
            }
            State::Impact => {
                if click {
                    host.set_click(false);
                }
                if host.clock() - self.timer.0 + self.timer.1 > 1.0 {
                    // @ 0x00484c72: the timer zeroed (its start too).
                    self.timer = (0.0, 0.0);
                    if self.prev == State::EnemyShot as i32 {
                        self.set_state(State::Aim);
                        self.impacts.clear();
                        self.wind = Self::roll(host, 20, 10);
                    } else if self.won {
                        self.screen = 1;
                        self.set_state(State::Console);
                        self.mark = MARK_AFTER;
                    } else {
                        self.enemy_shot(host);
                    }
                }
            }
            State::Idle => {
                if click {
                    host.set_click(false);
                }
            }
        }
        // @ 0x00484eb3: the clouds drift with the wind, in 0.02 s steps,
        // and wrap between −200 and 810.
        if self.screen == 2 {
            let t0 = host.frame_time();
            for c in &mut self.clouds {
                let mut t = f64::from(t0);
                if t > 0.0001f32 as f64 {
                    let k = f64::from(self.wind) * 4.0;
                    loop {
                        let step = if t < 0.02f32 as f64 {
                            t
                        } else {
                            0.02f32 as f64
                        };
                        c.x = (k * step + f64::from(c.x)) as f32;
                        c.y = (0.0 + f64::from(c.y)) as f32;
                        if c.x < -200.0 {
                            c.x = 810.0;
                        } else if c.x > 810.0 {
                            c.x = -200.0;
                        }
                        t -= step;
                        if t <= 0.0001f32 as f64 {
                            break;
                        }
                    }
                }
            }
        }
        self.draw(host);
    }

    fn free(&mut self, host: &mut dyn Host) {
        if self.clock_on {
            self.play_time.1 += host.clock() - self.play_time.0;
        }
        self.banks = Banks::default();
        for s in SFX {
            host.sfx_remove(s);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

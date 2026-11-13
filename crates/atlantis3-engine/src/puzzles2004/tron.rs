//! `PUZ_TRON`: the light cycles (init FUN_0049f260, start FUN_0049f510,
//! frame FUN_0049f700, free FUN_0049f560).
//!
//! Two players (15 dwords each from 0x00719a18): a trail (`std::list`
//! 0x00719a1c of segments), the current segment `{x1, y1, x2, y2, dir}`
//! (0x00719a24, dir 0 horizontal, 1 vertical) whose end (x2, y2) moves,
//! the AI flag (0x00719a38), the speed (0x00719a3c, signed px/s), the end
//! as floats (0x00719a40, 0x00719a44) and the AI's counters (0x00719a48
//! frames, 0x00719a4c look-ahead hit, 0x00719a50 timer). Player 0 is the
//! human (the arrows, Ctrl for speed 150 instead of 50), player 1 the AI
//! (the players' flags 0x00719b00 / 0x00719b04 are never written: both
//! 0 → player 1 forced to AI, @ 0x0049f8f0). A player whose current
//! segment crosses a wall (the arena 0x00719aa8), a trail or the other's
//! current segment loses: the human → `tron_perdu.wav` and both reset;
//! the AI → `TRON_GAGNE` = 1.

use std::any::Any;
use std::sync::Arc;

use atlantis3_data::spr::Bank;

use super::{aff, collision, Host, Puzzle, PuzzleId, HEIGHT, WIDTH};
use crate::px::ScreenPixel;

/// A segment: from (x1, y1) to (x2, y2), dir 0 horizontal, 1 vertical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seg {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub dir: i32,
}

const fn seg(x1: i32, y1: i32, x2: i32, y2: i32, dir: i32) -> Seg {
    Seg {
        x1,
        y1,
        x2,
        y2,
        dir,
    }
}

/// The arena's walls (FUN_0049f850).
pub const WALLS: [Seg; 4] = [
    seg(0xf1, 0x8d, 0x22e, 0x8d, 0),
    seg(0x22e, 0x8d, 0x22e, 0x1cd, 1),
    seg(0x22e, 0x1cd, 0xf1, 0x1cd, 0),
    seg(0xf1, 0x1cd, 0xf1, 0x8d, 1),
];

/// The AI's look-ahead distances by level (0x006b2894 / 0x006b2898,
/// pairs `{lo, hi}`; the game only calls level 0).
const AHEAD: [(i32, i32); 2] = [(8, 10), (2, 5)];
/// The random turn's factor by level (0x006b28a4).
const RANDOM: [f32; 2] = [0.4, 0.5];
/// The AI's period by level (0x006b28ac).
const PERIOD: [f32; 2] = [0.1, 0.05];

/// FUN_0049fe30: whether the moving segment `a` hits `b` (the original's
/// integer tests, `imul` wrapping).
pub fn hits(a: &Seg, b: &Seg) -> bool {
    let m = i32::wrapping_mul;
    let s = i32::wrapping_sub;
    match a.dir {
        0 => {
            if a.x1 == a.x2 {
                return false;
            }
            if b.dir == 1 {
                let e = m(s(b.y2, a.y1), s(b.y1, a.y1));
                let d = s(a.x2, b.x1);
                if d != 0 && m(s(a.x1, b.x1), d) >= 0 {
                    return false;
                }
                e <= 0
            } else {
                if a.y1 != b.y1 {
                    return false;
                }
                if m(s(a.x2, b.x1), s(a.x1, b.x1)) <= 0 {
                    return true;
                }
                m(s(a.x2, b.x2), s(a.x1, b.x2)) <= 0
            }
        }
        1 => {
            if a.y1 == a.y2 {
                return false;
            }
            if b.dir == 0 {
                let e = m(s(b.x2, a.x1), s(b.x1, a.x1));
                let d = s(a.y2, b.y1);
                if d != 0 && m(s(a.y1, b.y1), d) >= 0 {
                    return false;
                }
                e <= 0
            } else {
                if a.x1 != b.x1 {
                    return false;
                }
                if m(s(a.y2, b.y1), s(a.y1, b.y1)) <= 0 {
                    return true;
                }
                m(s(a.y2, b.y2), s(a.y1, b.y2)) <= 0
            }
        }
        _ => false,
    }
}

/// A player.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub trail: Vec<Seg>,
    pub seg: Seg,
    /// 0: the keys (FUN_0049ff30), 1: the AI (FUN_004a0960).
    pub ai: i32,
    pub speed: f32,
    pub fx: f32,
    pub fy: f32,
    pub frames: i32,
    pub ahead: i32,
    pub timer: f32,
}

impl Player {
    /// The turn: the segment kept in the trail, a new one from its end,
    /// the other direction.
    fn turn(&mut self) {
        self.trail.push(self.seg);
        self.seg.x1 = self.seg.x2;
        self.seg.y1 = self.seg.y2;
        self.seg.dir = (self.seg.dir + 1) % 2;
    }
}

/// The game's state.
pub struct Tron {
    pub players: [Player; 2],
    bank: Option<Arc<Bank>>,
}

/// What a step did besides moving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Running,
    /// The human crashed: both players reset.
    Lost,
    /// The AI crashed: `TRON_GAGNE`.
    Won,
}

impl Tron {
    /// FUN_0049f8f0(0x00719b00, 0x00719b04): both players' start.
    pub fn players(ai0: i32, ai1: i32) -> [Player; 2] {
        let p0 = Player {
            trail: Vec::new(),
            seg: seg(400, 0x1c3, 400, 0x1c2, 1),
            ai: ai0,
            speed: -50.0,
            fx: 400.0,
            fy: 450.0,
            frames: 0,
            ahead: 0,
            timer: 0.0,
        };
        let mut p1 = Player {
            trail: Vec::new(),
            seg: seg(400, 0x95, 400, 0x96, 1),
            ai: ai1,
            speed: 50.0,
            fx: 400.0,
            fy: 150.0,
            frames: 0,
            ahead: 0,
            timer: 0.0,
        };
        // The counters are not reset (only the init's zeroed memory).
        if ai0 == 0 && ai1 == 0 {
            p1.ai = 1;
        }
        [p0, p1]
    }

    /// The init's set-up without the engine (tests).
    pub fn new() -> Tron {
        Tron {
            players: Tron::players(0, 0),
            bank: None,
        }
    }

    /// FUN_0049f260 (its start FUN_0049f510 is the engine's).
    pub fn init(host: &mut dyn Host) -> Tron {
        let mut g = Tron::new();
        g.bank = host.bank("tron.spr");
        host.set_var("TRON_GAGNE", 0);
        host.set_var("TRON_QUIT", 0);
        let (w, h) = host.display_size();
        host.set_pointer(w / 2, h / 2);
        // 0x006b1d20 / 0x006b1d24 := 800, 600: their initial values.
        host.sfx_load("tron_sfx.wav");
        host.sfx_load("tron_perdu.wav");
        g
    }

    /// The reset after the human's crash (FUN_0049f8f0, the AI counters
    /// kept).
    fn reset(&mut self) {
        let keep = [
            (
                self.players[0].frames,
                self.players[0].ahead,
                self.players[0].timer,
            ),
            (
                self.players[1].frames,
                self.players[1].ahead,
                self.players[1].timer,
            ),
        ];
        self.players = Tron::players(0, 0);
        for (p, k) in self.players.iter_mut().zip(keep) {
            (p.frames, p.ahead, p.timer) = k;
        }
    }

    /// FUN_004a08e0: whether `probe` (of player `p`) hits a wall, a trail
    /// or the other player's current segment.
    pub fn blocked(&self, p: usize, probe: &Seg) -> bool {
        WALLS.iter().any(|w| hits(probe, w))
            || self.players.iter().enumerate().any(|(j, q)| {
                q.trail.iter().any(|t| hits(probe, t)) || (j != p && hits(probe, &q.seg))
            })
    }

    /// FUN_0049ff30 for player 0: the speed from Ctrl, a turn from an
    /// arrow (the key code cleared, other keys kept).
    fn keys(&mut self, host: &mut dyn Host) {
        let pl = &mut self.players[0];
        let fast = host.key_ctrl();
        pl.speed = match (fast, pl.speed <= 0.0) {
            (true, true) => -150.0,
            (true, false) => 150.0,
            (false, true) => -50.0,
            (false, false) => 50.0,
        };
        let code = host.key_code();
        if code == 0 {
            return;
        }
        // The arrows' codes: up 0x6d, left 0x6f, right 0x70, down 0x72.
        let (dir, negative) = match code {
            0x6d => (0, true),
            0x6f => (1, true),
            0x70 => (1, false),
            0x72 => (0, false),
            _ => return,
        };
        if pl.seg.dir == dir {
            if (negative && pl.speed > 0.0) || (!negative && pl.speed < 0.0) {
                pl.speed = -pl.speed;
            }
            pl.turn();
            host.sfx_play("tron_sfx.wav");
        }
        host.set_key_code(0);
    }

    /// The probe of the AI's turn tests: from the end, `k` pixels
    /// sideways; `left` the first test (@ 0x004a0aca), else the second
    /// (@ 0x004a0b73).
    fn side_probe(pl: &Player, k: i32, left: bool) -> Seg {
        let s = pl.seg;
        let pos = pl.speed > 0.0;
        let (mut x, mut y) = (s.x2, s.y2);
        if s.dir == 1 {
            x = if pos == left { x + k } else { x - k };
        } else {
            y = if pos == left { y - k } else { y + k };
        }
        seg(s.x2, s.y2, x, y, (s.dir + 1) % 2)
    }

    /// FUN_004a0960(p, other, level 0): the AI.
    fn ai(&mut self, host: &mut dyn Host, p: usize) {
        let level = 0usize;
        let dt = host.frame_time();
        self.players[p].frames += 1;
        let t = f64::from(dt) + f64::from(self.players[p].timer);
        self.players[p].timer = t as f32;
        if t < f64::from(PERIOD[level]) {
            return;
        }
        // Look ahead: the first distance hitting something, levels down.
        for l in (0..=level).rev() {
            let (lo, hi) = AHEAD[l];
            for d in lo..=hi {
                let pl = &self.players[p];
                let mut probe = pl.seg;
                let sign = if pl.speed <= 0.0 { -d } else { d };
                if pl.seg.dir == 1 {
                    probe.y2 += sign;
                } else {
                    probe.x2 += sign;
                }
                if self.blocked(p, &probe) {
                    self.players[p].ahead = d;
                    break;
                }
            }
            if self.players[p].ahead != 0 {
                break;
            }
        }
        if self.players[p].ahead == 0 {
            let frames = self.players[p].frames as f32;
            let r = host.rand();
            let v = f64::from(r % 300 + 600) * f64::from(RANDOM[level]);
            if v >= f64::from(frames) {
                return;
            }
        }
        self.players[p].frames = 0;
        // The free distances on both sides (49 when nothing is hit).
        let room = |g: &Tron, left: bool| {
            let mut k = 1;
            loop {
                if g.blocked(p, &Tron::side_probe(&g.players[p], k, left)) || k + 1 >= 0x32 {
                    return k;
                }
                k += 1;
            }
        };
        let a = room(self, true);
        let b = room(self, false);
        let pl = &mut self.players[p];
        if pl.ahead > b && pl.ahead > a {
            pl.ahead -= 1;
            return;
        }
        let (fa, fb) = (f64::from(a), f64::from(b));
        let k13 = f64::from(1.3f32);
        // Toward b: negate when vertical; toward a: when horizontal.
        let toward_b = if fb > fa * k13 {
            true
        } else if fa > fb * k13 {
            false
        } else {
            host.rand() % 100 > 0x32
        };
        if (toward_b && pl.seg.dir == 1) || (!toward_b && pl.seg.dir == 0) {
            pl.speed *= -1.0;
        }
        pl.turn();
        pl.timer = 0.0;
        pl.ahead = 0;
    }

    /// FUN_0049fa30: one step of both players.
    pub fn step(&mut self, host: &mut dyn Host) -> Outcome {
        for i in 0..2 {
            match self.players[i].ai {
                0 if i == 0 => self.keys(host),
                // Player 1's keys (0x4b / 0x4d, @ 0x0049ff30's other
                // branch) are unreachable: player 1 is always the AI.
                0 => {}
                1 => self.ai(host, i),
                _ => {}
            }
            let dt = f64::from(host.frame_time());
            let pl = &mut self.players[i];
            match pl.seg.dir {
                0 => {
                    let v = dt * f64::from(pl.speed) + f64::from(pl.fx);
                    pl.fx = v as f32;
                    pl.seg.x2 = v as i32;
                }
                1 => {
                    let v = dt * f64::from(pl.speed) + f64::from(pl.fy);
                    pl.fy = v as f32;
                    pl.seg.y2 = v as i32;
                }
                _ => {}
            }
            if self.blocked(i, &self.players[i].seg) {
                if self.players[i].ai == 0 {
                    host.sfx_play("tron_perdu.wav");
                    self.reset();
                    return Outcome::Lost;
                }
                host.set_var("TRON_GAGNE", 1);
                return Outcome::Won;
            }
        }
        Outcome::Running
    }

    /// FUN_004a06c0.
    fn draw(&self, host: &mut dyn Host) {
        // FUN_00412c00: the surface filled with 0.
        host.surface().clear();
        let b = self.bank.clone();
        // `AtlSpr_AffCentre(tron, 0, 400, 300)` (FUN_00436f50).
        if let Some(info) = b.as_ref().and_then(|b| b.info(0)) {
            let (x, y) = (400 - i32::from(info.w) / 2, 300 - i32::from(info.h) / 2);
            aff(host, "tron.spr", b.as_ref(), 0, Some(x), y, 1.0);
        }
        aff(host, "tron.spr", b.as_ref(), 1, Some(0x32), 0x32, 1.0);
        // The walls (0x006e0518) are never drawn.
        let colours = [(0u8, 0xffu8, 0u8), (0, 0, 0xff)];
        for (pl, &(r, g, bl)) in self.players.iter().zip(&colours) {
            for s in pl.trail.iter().chain(std::iter::once(&pl.seg)) {
                crate::with_px!(host.surface(), px => line(px, s, r, g, bl));
            }
        }
        host.draw_pointer();
        host.flip();
    }
}

impl Default for Tron {
    fn default() -> Tron {
        Tron::new()
    }
}

/// FUN_004c3720 → FUN_004c4c20 / FUN_004c4fb0: a Bresenham line from
/// (x1, y1) to (x2, y2) both included, clipped to the surface (FUN_004c3200),
/// in the colour FUN_004b3540 makes of `0xff_r_g_b`.
fn line<P: ScreenPixel>(px: &mut [P], s: &Seg, r: u8, g: u8, b: u8) {
    let c = P::from_rgb8(r, g, b);
    let (w, h) = (WIDTH as i32, HEIGHT as i32);
    let mut put = |x: i32, y: i32| {
        if (0..w).contains(&x) && (0..h).contains(&y) {
            px[(y * w + x) as usize] = c;
        }
    };
    let (mut x0, mut y0, mut x1, mut y1) = (s.x1, s.y1, s.x2, s.y2);
    if (x1 - x0).abs() < (y1 - y0).abs() {
        if y1 < y0 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
        }
        let step = if x1 <= x0 { -1 } else { 1 };
        let dx = (x1 - x0).abs();
        let dy = y1 - y0;
        let mut e = 2 * dx - dy;
        let mut x = x0;
        put(x, y0);
        for y in y0 + 1..=y1 {
            if e >= 0 {
                x += step;
                e += 2 * (dx - dy);
            } else {
                e += 2 * dx;
            }
            put(x, y);
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
        put(x0, y);
        for x in x0 + 1..=x1 {
            if e >= 0 {
                y += step;
                e += 2 * (dy - dx);
            } else {
                e += 2 * dy;
            }
            put(x, y);
        }
    }
}

impl Puzzle for Tron {
    fn id(&self) -> PuzzleId {
        PuzzleId::Tron
    }

    /// FUN_0049f700.
    fn frame(&mut self, host: &mut dyn Host) {
        if !host.script_prologue() {
            return;
        }
        if host.var("TRON_GAGNE") != 0 || host.var("TRON_QUIT") != 0 {
            return;
        }
        if host.click() {
            let (x, y) = host.pointer();
            if collision(self.bank.as_ref(), 1, Some(0x32), 0x32, x, y) {
                host.set_var("TRON_QUIT", 1);
                host.set_click(false);
                return;
            }
            host.set_click(false);
        }
        self.step(host);
        self.draw(host);
    }

    /// FUN_0049f560's game part.
    fn free(&mut self, host: &mut dyn Host) {
        self.bank = None;
        host.sfx_remove("tron_sfx.wav");
        host.sfx_remove("tron_perdu.wav");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::px::Surface;
    use atlantis3_data::spr::Depth;

    /// A host with a fixed frame time, a key code and the CRT's `rand`.
    struct Mock {
        dt: f32,
        code: i32,
        ctrl: bool,
        rand: atlantis2_engine::crt::Rand,
        vars: Vec<(String, i32)>,
        sounds: Vec<String>,
        surface: Surface,
    }

    impl Mock {
        fn new() -> Mock {
            Mock {
                dt: 0.04,
                code: 0,
                ctrl: false,
                rand: atlantis2_engine::crt::Rand::new(1),
                vars: Vec::new(),
                sounds: Vec::new(),
                surface: Surface::new(Depth::Rgb565, WIDTH * HEIGHT),
            }
        }
    }

    impl Host for Mock {
        fn clock(&self) -> f64 {
            0.0
        }
        fn frame_time(&self) -> f32 {
            self.dt
        }
        fn pointer(&self) -> (i32, i32) {
            (0, 0)
        }
        fn set_pointer(&mut self, _x: i32, _y: i32) {}
        fn click(&self) -> bool {
            false
        }
        fn set_click(&mut self, _on: bool) {}
        fn key(&self, _vk: u8) -> bool {
            false
        }
        fn srand_time(&mut self) {}
        fn rand(&mut self) -> i32 {
            self.rand.rand()
        }
        fn var(&self, name: &str) -> i32 {
            self.vars
                .iter()
                .rev()
                .find(|v| v.0 == name)
                .map_or(0, |v| v.1)
        }
        fn set_var(&mut self, name: &str, v: i32) {
            self.vars.push((name.to_string(), v));
        }
        fn flag(&self, _n: usize) -> i32 {
            0
        }
        fn set_flag(&mut self, _n: usize, _v: i32) {}
        fn bank(&mut self, _file: &str) -> Option<Arc<Bank>> {
            None
        }
        fn read(&mut self, _path: &str) -> Option<Vec<u8>> {
            None
        }
        fn sfx_load(&mut self, _file: &str) {}
        fn sfx_play(&mut self, file: &str) {
            self.sounds.push(file.to_string());
        }
        fn sfx_remove(&mut self, _file: &str) {}
        fn surface(&mut self) -> &mut Surface {
            &mut self.surface
        }
        fn draw_pointer(&mut self) {}
        fn flip(&mut self) {}
        fn script_prologue(&mut self) -> bool {
            true
        }
        fn key_code(&self) -> i32 {
            self.code
        }
        fn set_key_code(&mut self, code: i32) {
            self.code = code;
        }
        fn key_ctrl(&self) -> bool {
            self.ctrl
        }
    }

    #[test]
    fn crossings() {
        // A vertical move across a horizontal wall, and stopping short.
        let wall = WALLS[0];
        assert!(hits(&seg(400, 150, 400, 141, 1), &wall));
        assert!(hits(&seg(400, 150, 400, 130, 1), &wall));
        assert!(!hits(&seg(400, 150, 400, 142, 1), &wall));
        // Touching at the corner of its own last segment: no hit.
        assert!(!hits(
            &seg(300, 300, 300, 290, 1),
            &seg(250, 300, 300, 300, 0)
        ));
        // A zero-length segment hits nothing.
        assert!(!hits(&seg(300, 300, 300, 300, 1), &wall));
        // Collinear overlap.
        assert!(hits(
            &seg(300, 200, 300, 220, 1),
            &seg(300, 210, 300, 260, 1)
        ));
        assert!(!hits(
            &seg(300, 200, 300, 205, 1),
            &seg(300, 210, 300, 260, 1)
        ));
    }

    #[test]
    fn keys_turn_and_speed() {
        let mut g = Tron::new();
        let mut h = Mock::new();
        // Up while going up: no turn, the code cleared.
        h.code = 0x6d;
        g.step(&mut h);
        assert_eq!(h.code, 0);
        assert!(g.players[0].trail.is_empty());
        // Left: a turn, a sound, speed negative.
        h.code = 0x6f;
        g.step(&mut h);
        assert_eq!(g.players[0].trail.len(), 1);
        assert_eq!(g.players[0].seg.dir, 0);
        assert!(g.players[0].speed < 0.0);
        assert_eq!(h.sounds, ["tron_sfx.wav"]);
        // Another key stays.
        h.code = 0x10;
        g.step(&mut h);
        assert_eq!(h.code, 0x10);
        // Ctrl: 150.
        h.ctrl = true;
        h.code = 0;
        g.step(&mut h);
        assert_eq!(g.players[0].speed, -150.0);
    }

    #[test]
    fn human_crash_resets() {
        let mut g = Tron::new();
        let mut h = Mock::new();
        let mut out = Outcome::Running;
        for _ in 0..1000 {
            out = g.step(&mut h);
            if out != Outcome::Running {
                break;
            }
        }
        assert_eq!(out, Outcome::Lost);
        assert_eq!(h.sounds.last().map(String::as_str), Some("tron_perdu.wav"));
        assert_eq!(g.players[0].seg, seg(400, 0x1c3, 400, 0x1c2, 1));
        assert!(g.players[0].trail.is_empty() && g.players[1].trail.is_empty());
    }

    #[test]
    fn ai_avoids_walls() {
        // Player 0 parked at the right (no keys, no speed): the AI turns
        // before the walls for 20 s.
        let mut g = Tron::new();
        let mut h = Mock::new();
        g.players[0].seg = seg(550, 300, 550, 301, 1);
        g.players[0].ai = 2;
        g.players[0].speed = 0.0;
        for _ in 0..500 {
            assert_eq!(g.step(&mut h), Outcome::Running);
        }
        assert!(!g.players[1].trail.is_empty());
    }

    #[test]
    fn key_codes() {
        assert_eq!(super::super::key_code_of_vk(0x26), 0x6d);
        assert_eq!(super::super::key_code_of_vk(0x25), 0x6f);
        assert_eq!(super::super::key_code_of_vk(0x27), 0x70);
        assert_eq!(super::super::key_code_of_vk(0x28), 0x72);
        assert_eq!(super::super::key_code_of_vk(0x11), 0x79);
        // VK_DECIMAL: codes 0x53 and 0x68, the later wins.
        assert_eq!(super::super::key_code_of_vk(0x6e), 0x68);
        assert_eq!(super::super::key_code_of_vk(0xff), 0);
    }
}

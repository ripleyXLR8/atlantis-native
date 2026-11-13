//! `PUZ_FROGGER` (loop mode 0x11, `Puzzles[4]`): a frog crosses four
//! screens of roads and rivers (atlantis4-cd-en-3022848: init
//! FUN_00493fc0, start FUN_00494fb0, free FUN_00495040, frame
//! FUN_00495420, draw FUN_00496b20, lanes FUN_00497650, symbols
//! FUN_004992a0, death FUN_004993c0, ride FUN_00499530 / FUN_00499550).
//!
//! Each level (0x006b2564, 1..4) has 20 lanes (0x00719058 + 20 i: a
//! `std::list` of objects, the lane's y float +0xc and kind +0x10: 0
//! ground, 1 road, 2 water); FUN_00497650 fills lanes 0..10 of each
//! ([`lanes`], dumped under emulation). An object (a list node's data,
//! +8..+0x1f) is `{x, y, speed, kind, anim, half width}`: kinds < 10
//! (0, 1: cars) kill, kinds ≥ 10 (0x14, 0x15: boats) carry the frog. The
//! frog jumps lane by lane (the arrows' sprites `F_FL` or the arrow
//! keys), a jump lasting a quarter of a second; on the top lane (y ≤ 20)
//! it picks the level's symbol (`FROG_PREMIER..QUATRIEME_SYMB` := 1) and
//! may leave the level once the symbol is held (level 4: once the script
//! set `FROG_QUATRIEME_SYMB` to 3). Leaving level 4 wins
//! (`FROG_GAGNE` := 1). Three lives (0x006b255c), 300 s (0x006b2560):
//! running out of either, or the exit sprite `F_qu`, sets `FROG_QUIT`.
//!
//! Banks (`spr\`): `F_Vr1` (0x00719758, the frog), `F_BAR_D` (0x007196d8)
//! / `F_BAR_G` (0x00719720, boats), `F_MUR_G` (0x00719788) / `F_LIM_D`
//! (0x00718e70, cars), `F_vi` (0x00719698, a life), `F_S1..4`
//! (0x00718e88, 0x00719708, 0x00719740, 0x00719770: the symbols use
//! `F_S4` for levels 1 and 2 and `F_S1` for 3 and 4, `F_S2` and `F_S3`
//! are loaded and never drawn), `F_JA` (0x007196b8, the time gauge),
//! `F_qu` (0x007196f0, the exit), `F_FL` (0x00718ea0, the four arrows),
//! `F_E1..4` (0x00718ee8, 0x00718f00, 0x00718eb8, 0x00718ed0: each
//! level's animation); `map\IF<level>.tga` and `IF<level+1>.tga` into two
//! surfaces (0x00718f18, 0x00718de8) the levels scroll between. Sounds:
//! `frog_saute.wav` (a jump), `frog_mort.wav` (a death).
//!
//! Floats: the x87 runs at 53 bits (cw 0x027f): values kept on its stack
//! are `f64` here, stored ones `f32`.

use std::any::Any;
use std::sync::Arc;

use atlantis3_data::spr::Bank;

use super::{aff, collision, ftol, ftol_d, Host, Puzzle, PuzzleId, HEIGHT, WIDTH};
use crate::px::Surface;

/// The lanes of each level (FUN_00497650 @ 0x00497650..0x0049929b).
pub const LANES: usize = 20;
pub const LEVELS: usize = 4;

/// The frog's start x per level (0x006b2568 + 4 (level − 1)).
pub const START_X: [i32; 4] = [400; 4];
/// The sideways jump per level (0x006b2578 + 4 (level − 1)).
pub const STEP_X: [i32; 4] = [40; 4];
/// The time (0x006b2560's start value, set by the start @ 0x00495029).
pub const TIME: f32 = 300.0;

/// An object of a lane (a node's data).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obj {
    pub x: f32,
    pub y: f32,
    pub speed: f32,
    pub kind: i32,
    pub anim: f32,
    /// Half its sprite's width (set by the init @ 0x00494d2e; 0 for a kind
    /// without a bank).
    pub hw: i32,
}

/// A lane: its y, its kind (0 ground, 1 road, 2 water), its objects in
/// list order.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub y: f32,
    pub kind: i32,
    pub objs: Vec<Obj>,
}

/// A symbol (0x00718f98 + 0x30 i).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Symbol {
    /// +0x14, +0x18.
    pub x: f32,
    pub y: f32,
    /// +0x1c: the level it lies in.
    pub level: i32,
    /// +0x20: the draw's alpha.
    pub alpha: f32,
    /// +0x24: picked up.
    pub found: i32,
    /// +0x28: shown (blinks while not found).
    pub blink: bool,
    /// +0x2c: the blink's countdown.
    pub timer: f32,
}

/// FUN_00497650's lanes, levels 1..4, 20 lanes each (lanes 11..19 stay
/// at y 0, kind 0, empty: .bss). Values read under emulation of the
/// function (the harness `a4cd_frogger_lanes.py`, atlantis4-re tools).
pub fn lanes() -> Vec<Lane> {
    // Per level: lanes 1 and 2's speed, lanes 3 and 4's, four cars on
    // lanes 5 and 6.
    const LEVEL: [(f32, f32, bool); 4] = [
        (30.0, 60.0, false),
        (30.0, 60.0, false),
        (50.0, 60.0, true),
        (60.0, 70.0, true),
    ];
    let mut out = Vec::with_capacity(LANES * LEVELS);
    for (r, w, four) in LEVEL {
        // (y, kind, [(x, speed, kind, anim)]) for lanes 0..10.
        let mut l5 = vec![(400.0, -90.0, 1, 0.0), (700.0, -90.0, 1, 1.0)];
        let mut l6 = vec![(300.0, 90.0, 0, 0.0), (600.0, 90.0, 0, 1.0)];
        if four {
            l5.extend([(50.0, -90.0, 1, 0.0), (520.0, -90.0, 1, 1.0)]);
            l6.extend([(450.0, 90.0, 0, 0.0), (100.0, 90.0, 0, 1.0)]);
        }
        type Objs = Vec<(f32, f32, i32, f32)>;
        let table: [(f32, i32, Objs); 11] = [
            (530.0, 0, vec![]),
            (470.0, 1, vec![(600.0, -r, 1, 0.0), (110.0, -r, 1, 1.0)]),
            (430.0, 1, vec![(750.0, r, 0, 0.0), (100.0, r, 0, 1.0)]),
            (350.0, 2, vec![(150.0, -w, 20, 0.0), (450.0, -w, 20, 1.0)]),
            (310.0, 2, vec![(0.0, w, 21, 0.0), (500.0, w, 21, 2.0)]),
            (230.0, 1, l5),
            (190.0, 1, l6),
            (
                110.0,
                2,
                vec![(550.0, -120.0, 20, 0.0), (300.0, -120.0, 20, 1.0)],
            ),
            (
                70.0,
                2,
                vec![(200.0, 120.0, 21, 0.0), (400.0, 120.0, 21, 2.0)],
            ),
            (15.0, 0, vec![]),
            (-20.0, 0, vec![]),
        ];
        let n = table.len();
        for (y, kind, objs) in table {
            out.push(Lane {
                y,
                kind,
                objs: objs
                    .into_iter()
                    .map(|(x, speed, kind, anim)| Obj {
                        x,
                        y,
                        speed,
                        kind,
                        anim,
                        hw: 0,
                    })
                    .collect(),
            });
        }
        for _ in n..LANES {
            out.push(Lane {
                y: 0.0,
                kind: 0,
                objs: Vec::new(),
            });
        }
    }
    out
}

/// FUN_004992a0's symbols (written while the game is not loaded).
pub fn symbols() -> [Symbol; 4] {
    std::array::from_fn(|i| Symbol {
        x: 400.0,
        y: 15.0,
        level: i as i32 + 1,
        alpha: 1.0,
        found: 0,
        blink: false,
        timer: 0.5,
    })
}

/// The variables the symbols set (@ 0x00496a30..0x00496a9c).
pub const SYMBOL_VARS: [&str; 4] = [
    "FROG_PREMIER_SYMB",
    "FROG_DEUXIEME_SYMB",
    "FROG_TROISIEME_SYMB",
    "FROG_QUATRIEME_SYMB",
];

/// A jump's way (0x007197c4).
pub const UP: i32 = 0;
pub const DOWN: i32 = 1;
pub const LEFT: i32 = 2;
pub const RIGHT: i32 = 3;

/// The key codes (0x006e9e6c) the frame reads.
pub const KEY_UP: i32 = 0x6d;
pub const KEY_LEFT: i32 = 0x6f;
pub const KEY_RIGHT: i32 = 0x70;
pub const KEY_DOWN: i32 = 0x72;

/// The arrows' sprites (`F_FL` frame, x; y 0x23f): the click tests'
/// order (@ 0x00495a1c, 0x00495b70, 0x00495e09, 0x00495f55).
pub const ARROW_DOWN: (usize, i32) = (0, 0x302);
pub const ARROW_UP: (usize, i32) = (3, 0x301);
pub const ARROW_LEFT: (usize, i32) = (2, 0x2fd);
pub const ARROW_RIGHT: (usize, i32) = (1, 0x307);
pub const BAR_Y: i32 = 0x23f;
/// The exit (`F_qu` 0 at (0x2d0, 0x23f)).
pub const EXIT_X: i32 = 0x2d0;

#[derive(Default, Clone)]
struct Banks {
    frog: Option<Arc<Bank>>,
    bar_d: Option<Arc<Bank>>,
    bar_g: Option<Arc<Bank>>,
    mur_g: Option<Arc<Bank>>,
    lim_d: Option<Arc<Bank>>,
    vi: Option<Arc<Bank>>,
    s: [Option<Arc<Bank>>; 4],
    ja: Option<Arc<Bank>>,
    qu: Option<Arc<Bank>>,
    fl: Option<Arc<Bank>>,
    e: [Option<Arc<Bank>>; 4],
}

/// The game's state.
#[derive(Clone)]
pub struct Frogger {
    /// 0x006b2564 (1..4).
    pub level: i32,
    /// 0x006b255c.
    pub lives: i32,
    /// 0x006b2560, seconds left.
    pub timer: f32,
    /// The 80 lanes, level-major.
    pub lanes: Vec<Lane>,
    pub symbols: [Symbol; 4],
    /// The frog (0x007196d0, 0x007196d4), a jump's start (0x007196ac,
    /// 0x007196b0) and end (0x0071979c, 0x007197a0).
    pub x: i32,
    pub y: i32,
    pub x0: i32,
    pub y0: i32,
    pub tx: i32,
    pub ty: i32,
    /// The lane (0x007197c8).
    pub lane: i32,
    /// A jump runs (0x007197bc), its way (0x007197c4) and progress
    /// (0x007197c0, 0..1).
    pub jumping: bool,
    pub dir: i32,
    pub progress: f32,
    /// The jump leaves the level (0x007197cc).
    pub level_up: bool,
    /// The screens scroll to the next level (0x007197cd), progress
    /// 0x007197d0.
    pub scrolling: bool,
    pub scroll: f32,
    /// The boat carrying the frog (0x007197b4: a pointer to its x; here
    /// its lane and index) and its x at the last test (0x007197b8).
    pub ride: Option<(usize, usize)>,
    pub ride_x0: i32,
    /// Leaving level 4 (0x007197a5, no reader).
    pub won: bool,
    /// The time decrement skips its first frame (0x007197a6) and is
    /// corrected by the next level's load time (0x007197a8).
    pub started: bool,
    pub corr: f32,
    /// The frog's (0x007197ac) and the level's (0x007197b0) animation.
    pub frog_anim: f32,
    pub level_anim: f32,
    /// Half widths: the frog (0x006b2588), a symbol (0x006b258c).
    pub frog_hw: i32,
    pub sym_hw: i32,
    banks: Banks,
    /// The current and next levels' screens (0x007197d4, 0x007197d8).
    bg: [Option<Arc<Surface>>; 2],
}

/// What [`Frogger::step`] asks of the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Draw, then clear the click.
    Draw,
    /// Return (a variable stopped the game).
    Return,
    /// Clear the click and return (the exit sprite).
    ClearAndReturn,
}

impl Default for Frogger {
    fn default() -> Frogger {
        Frogger::new()
    }
}

/// A 24-bit TGA as a surface of the stretch surface's format
/// (FUN_0041a790 with the display's format).
fn load_bg(host: &mut dyn Host, n: i32) -> Option<Arc<Surface>> {
    let d = host.read(&format!("map/IF{n}.tga"))?;
    let s = match host.surface() {
        Surface::Rgb565(_) => {
            let t = atlantis2_data::tga::Tga::parse(&d).ok()?;
            (t.width == WIDTH && t.height == HEIGHT).then_some(Surface::Rgb565(t.pixels))
        }
        Surface::Xrgb8888(_) => {
            let t = atlantis2_data::tga::Tga32::parse(&d).ok()?;
            (t.width == WIDTH && t.height == HEIGHT).then_some(Surface::Xrgb8888(t.pixels))
        }
    };
    if s.is_none() {
        log::warn!("map\\IF{n}.tga: not loaded");
    }
    s.map(Arc::new)
}

/// FUN_004b5140 → FUN_004c19b0 / FUN_004c2160: the rows `src_top..=src_bottom`
/// of `src` (full width) copied to `dst` from row `dst_top`, the key colour
/// (0xff000000 converted: black, 0) skipped.
fn blit_rows(dst: &mut Surface, src: &Surface, src_top: i32, src_bottom: i32, dst_top: i32) {
    fn rows<P: Copy + PartialEq + Default>(d: &mut [P], s: &[P], st: i32, sb: i32, dt: i32) {
        let key = P::default();
        for (i, sy) in (st..=sb).enumerate() {
            let dy = dt + i as i32;
            if !(0..HEIGHT as i32).contains(&sy) || !(0..HEIGHT as i32).contains(&dy) {
                continue;
            }
            let (so, dof) = (sy as usize * WIDTH, dy as usize * WIDTH);
            for x in 0..WIDTH {
                let p = s[so + x];
                if p != key {
                    d[dof + x] = p;
                }
            }
        }
    }
    match (dst, src) {
        (Surface::Rgb565(d), Surface::Rgb565(s)) => rows(d, s, src_top, src_bottom, dst_top),
        (Surface::Xrgb8888(d), Surface::Xrgb8888(s)) => rows(d, s, src_top, src_bottom, dst_top),
        _ => {}
    }
}

fn width_half(bank: Option<&Arc<Bank>>) -> Option<i32> {
    // FUN_00437260 (AtlSpr_GetSize) of frame 0, then `/ 2` (cdq, sub, sar).
    bank.and_then(|b| b.info(0)).map(|i| i32::from(i.w) / 2)
}

fn frames(bank: Option<&Arc<Bank>>) -> i32 {
    // FUN_004374f0 (AtlSpr_GetNbSpr).
    bank.map_or(0, |b| b.len() as i32)
}

impl Frogger {
    /// The state FUN_00493fc0 sets, without the engine (tests): level 1,
    /// half widths 0 (no banks).
    pub fn new() -> Frogger {
        let lanes = lanes();
        let level = 1;
        let x = START_X[0];
        let y = ftol(lanes[0].y);
        Frogger {
            level,
            lives: 3,
            timer: TIME,
            lanes,
            symbols: symbols(),
            x,
            y,
            x0: x,
            y0: y,
            tx: 0,
            ty: 0,
            lane: 0,
            jumping: false,
            dir: 0,
            progress: 0.0,
            level_up: false,
            scrolling: false,
            scroll: 0.0,
            ride: None,
            ride_x0: 0,
            won: false,
            started: false,
            corr: 0.0,
            frog_anim: 0.0,
            level_anim: 0.0,
            frog_hw: 15,
            sym_hw: 25,
            banks: Banks::default(),
            bg: [None, None],
        }
    }

    /// FUN_00493fc0, then its start FUN_00494fb0's game part.
    pub fn init(host: &mut dyn Host) -> Frogger {
        let mut g = Frogger::new();
        let b = &mut g.banks;
        b.frog = host.bank("F_Vr1.spr");
        b.bar_d = host.bank("F_BAR_D.spr");
        b.bar_g = host.bank("F_BAR_G.spr");
        b.mur_g = host.bank("F_MUR_G.spr");
        b.lim_d = host.bank("F_LIM_D.spr");
        b.vi = host.bank("F_vi.spr");
        b.s[0] = host.bank("F_S1.spr");
        b.s[1] = host.bank("F_S2.spr");
        b.s[2] = host.bank("F_S3.spr");
        b.s[3] = host.bank("F_S4.spr");
        b.ja = host.bank("F_JA.spr");
        b.qu = host.bank("F_qu.spr");
        b.fl = host.bank("F_FL.spr");
        b.e[0] = host.bank("F_E1.spr");
        b.e[1] = host.bank("F_E2.spr");
        b.e[2] = host.bank("F_E3.spr");
        b.e[3] = host.bank("F_E4.spr");
        // `map\IF%1d.tga` of the level and the next (@ 0x00494a6a,
        // @ 0x00494b39).
        g.bg = [load_bg(host, g.level), load_bg(host, g.level + 1)];
        // The half widths (@ 0x00494c5d..0x00494dca): kinds 0 and 2
        // `F_LIM_D`, 1 and 3 `F_MUR_G`, 0x14 and 0x15 `F_BAR_D`.
        let hws = [
            width_half(g.banks.lim_d.as_ref()),
            width_half(g.banks.mur_g.as_ref()),
            width_half(g.banks.bar_d.as_ref()),
        ];
        for lane in &mut g.lanes {
            for o in &mut lane.objs {
                let hw = match o.kind {
                    0 | 2 => hws[0],
                    1 | 3 => hws[1],
                    0x14 | 0x15 => hws[2],
                    _ => None,
                };
                if let Some(hw) = hw {
                    o.hw = hw;
                }
            }
        }
        if let Some(hw) = width_half(g.banks.frog.as_ref()) {
            g.frog_hw = hw;
        }
        if let Some(hw) = width_half(g.banks.s[0].as_ref()) {
            g.sym_hw = hw;
        }
        // FUN_0042e390, cursor 29.
        let (w, h) = host.display_size();
        host.set_pointer(w / 2, h / 2);
        host.sfx_load("frog_mort.wav");
        host.sfx_load("frog_saute.wav");
        // The start (FUN_00494fb0; the CD's scripts always follow the init
        // with STARTPUZZLE, which runs it again with the same effect).
        host.set_var("FROG_DEBUT", 1);
        host.set_var("FROG_GAGNE", 0);
        host.set_var("FROG_QUIT", 0);
        g.timer = TIME;
        g
    }

    fn lane_idx(&self, lane: i32) -> Option<usize> {
        let i = (self.level - 1) * LANES as i32 + lane;
        (lane >= 0 && lane < LANES as i32 && i >= 0 && (i as usize) < self.lanes.len())
            .then_some(i as usize)
    }

    /// A lane's y (0 outside the table).
    pub fn lane_y(&self, lane: i32) -> f32 {
        self.lane_idx(lane).map_or(0.0, |i| self.lanes[i].y)
    }

    /// The level's lanes.
    pub fn level_lanes(&self) -> &[Lane] {
        let a = ((self.level - 1).clamp(0, LEVELS as i32 - 1) as usize) * LANES;
        &self.lanes[a..a + LANES]
    }

    fn reset_level(&mut self) {
        self.level = 1;
        self.lane = 0;
        self.jumping = false;
        self.scrolling = false;
        self.level_up = false;
    }

    /// A jump's start (common to the clicks and the keys).
    fn jump(&mut self, host: &mut dyn Host, dir: i32, tx: i32, ty: i32) {
        self.dir = dir;
        self.jumping = true;
        self.tx = tx;
        self.ty = ty;
        self.x0 = self.x;
        self.y0 = self.y;
        host.sfx_play("frog_saute.wav");
    }

    /// Up (the arrow @ 0x00495ba0, the key @ 0x004960bd).
    fn up(&mut self, host: &mut dyn Host) {
        let l = self.lane;
        // FLD y[l]; FSUB y[l + 1]; then y − that.
        let d = f64::from(self.lane_y(l)) - f64::from(self.lane_y(l + 1));
        let ty = ftol_d(f64::from(self.y) - d);
        if f64::from(self.y) > 20.0 {
            self.lane = l + 1;
            self.jump(host, UP, self.x, ty);
            return;
        }
        // The top: the level's symbol held (0x00718f8c + 0x30 level), on
        // level 4 `FROG_QUATRIEME_SYMB` = 3 (set by the script).
        let ok = if self.level == 4 {
            host.var("FROG_QUATRIEME_SYMB") == 3
        } else {
            (1..=4).contains(&self.level) && self.symbols[(self.level - 1) as usize].found == 1
        };
        if ok {
            self.level_up = true;
            self.lane = 0;
            self.jump(host, UP, self.x, ty);
        }
    }

    /// Down (@ 0x00495a58, 0x0049636a).
    fn down(&mut self, host: &mut dyn Host) {
        if self.y >= 0x1fe {
            return;
        }
        self.lane -= 1;
        let l = self.lane;
        let ty =
            ftol_d(f64::from(self.y) - f64::from(self.lane_y(l + 1)) + f64::from(self.lane_y(l)));
        self.jump(host, DOWN, self.x, ty);
    }

    fn step_x(&self) -> i32 {
        STEP_X[((self.level - 1).clamp(0, 3)) as usize]
    }

    /// Left (@ 0x00495e3a, 0x004964ba).
    fn left(&mut self, host: &mut dyn Host) {
        let s = self.step_x();
        if s < self.x {
            self.jump(host, LEFT, self.x - s, self.y);
        }
    }

    /// Right (@ 0x00495f8c, 0x004965dd): FUN_00411a30 is the stretch
    /// surface's width.
    fn right(&mut self, host: &mut dyn Host) {
        let s = self.step_x();
        if self.x < host.display_size().0 - s {
            self.jump(host, RIGHT, self.x + s, self.y);
        }
    }

    /// FUN_004993c0: a death.
    fn die(&mut self, host: &mut dyn Host) {
        host.sfx_play("frog_mort.wav");
        self.lives -= 1;
        self.jumping = false;
        self.x = START_X[((self.level - 1).clamp(0, 3)) as usize];
        self.y = ftol(self.lane_y(0));
        self.y0 = self.y;
        self.x0 = self.x;
        self.progress = 0.0;
        self.lane = 0;
        if self.lives <= 0 {
            self.reset_level();
            host.set_var("FROG_QUIT", 1);
        }
    }

    /// FUN_00495420 from after the prologue to the draw.
    pub fn step(&mut self, host: &mut dyn Host) -> Step {
        if host.var("FROG_GAGNE") != 0 || host.var("FROG_QUIT") != 0 {
            return Step::Return;
        }
        let dt = host.frame_time();
        if self.started {
            self.timer = (f64::from(self.timer) - (f64::from(dt) - f64::from(self.corr))) as f32;
        } else {
            self.started = true;
        }
        if self.timer <= 1e-5_f32 {
            self.timer = 0.0;
            self.reset_level();
            host.set_var("FROG_QUIT", 1);
            return Step::Return;
        }
        let (px, py) = host.pointer();
        if host.click() && collision(self.banks.qu.as_ref(), 0, Some(EXIT_X), BAR_Y, px, py) {
            self.reset_level();
            host.set_var("FROG_QUIT", 1);
            return Step::ClearAndReturn;
        }
        if self.scrolling {
            let s = f64::from(dt) * 0.5 + f64::from(self.scroll);
            self.scroll = s as f32;
            if s >= 1.0 {
                self.scroll = 0.0;
                self.scrolling = false;
                self.dir = UP;
                self.jumping = true;
                self.y = host.display_size().1;
                self.y0 = self.y;
                self.ty = ftol(self.lane_y(0));
                let t0 = host.clock();
                self.bg.swap(0, 1);
                if self.level < 4 {
                    self.bg[1] = load_bg(host, self.level + 1);
                }
                self.corr = (host.clock() - t0) as f32;
            }
            return Step::Draw;
        }
        if self.jumping {
            let d = f64::from(dt) - f64::from(self.corr);
            self.corr = 0.0;
            self.progress = (d * 4.0 + f64::from(self.progress)) as f32;
            let p = f64::from(self.progress);
            let done = self.progress >= 1.0;
            match self.dir {
                UP | DOWN if done => {
                    self.jumping = false;
                    self.y = self.ty;
                    self.y0 = self.ty;
                    self.progress = 0.0;
                    self.x0 = self.tx;
                }
                UP => self.y = ftol_d(f64::from(self.y0) - f64::from(self.y0 - self.ty) * p),
                DOWN => self.y = ftol_d(f64::from(self.ty - self.y0) * p + f64::from(self.y0)),
                LEFT | RIGHT if done => {
                    self.x = self.tx;
                    self.jumping = false;
                    self.progress = 0.0;
                    self.x0 = self.tx;
                    self.y0 = self.ty;
                }
                LEFT => self.x = ftol_d(f64::from(self.x0) - f64::from(self.x0 - self.tx) * p),
                RIGHT => self.x = ftol_d(f64::from(self.tx - self.x0) * p + f64::from(self.x0)),
                _ => {}
            }
            if !self.jumping {
                if self.level_up {
                    self.level_up = false;
                    self.level += 1;
                    self.scrolling = true;
                    if self.level == 5 {
                        self.level = 1;
                        self.lane = 0;
                        self.jumping = false;
                        self.scrolling = false;
                        self.won = true;
                        host.set_var("FROG_GAGNE", 1);
                        return Step::Return;
                    }
                }
                if self.level == 1 && self.lane == 2 {
                    host.set_var("FROG_TRAVERSE_UNE_RUE", 1);
                }
            }
        } else {
            if host.click() {
                let fl = self.banks.fl.clone();
                let hit = |(f, x): (usize, i32)| collision(fl.as_ref(), f, Some(x), BAR_Y, px, py);
                if hit(ARROW_DOWN) {
                    self.down(host);
                } else if hit(ARROW_UP) {
                    self.up(host);
                } else if hit(ARROW_LEFT) {
                    self.left(host);
                } else if hit(ARROW_RIGHT) {
                    self.right(host);
                }
                host.set_click(false);
            }
            let code = host.key_code();
            if code != 0 && !self.jumping {
                match code {
                    KEY_UP => self.up(host),
                    KEY_LEFT => self.left(host),
                    KEY_RIGHT => self.right(host),
                    KEY_DOWN => self.down(host),
                    _ => {}
                }
            }
            // @ 0x004966d8: FUN_00499550.
            if self.jumping {
                self.ride = None;
            }
        }
        self.move_and_test(host, dt)
    }

    /// @ 0x004966e5..0x00496ab4: the objects move, the boat carries the
    /// frog, the collisions, the symbols.
    fn move_and_test(&mut self, host: &mut dyn Host, dt: f32) -> Step {
        let w = host.display_size().0;
        let a = ((self.level - 1).clamp(0, 3) as usize) * LANES;
        for lane in &mut self.lanes[a..a + LANES] {
            for o in &mut lane.objs {
                if o.speed > 0.0 && f64::from(w + o.hw) < f64::from(o.x) {
                    loop {
                        let v = f64::from(o.x) - f64::from(w + o.hw * 2);
                        o.x = v as f32;
                        if f64::from(w + o.hw) >= f64::from(o.x) {
                            break;
                        }
                    }
                }
                if o.speed < 0.0 && f64::from(-o.hw) > f64::from(o.x) {
                    loop {
                        let v = f64::from(w + o.hw * 2) + f64::from(o.x);
                        o.x = v as f32;
                        if v >= f64::from(-o.hw) {
                            break;
                        }
                    }
                }
                o.x = (f64::from(dt) * f64::from(o.speed) + f64::from(o.x)) as f32;
            }
        }
        if let Some((l, i)) = self.ride {
            let ox = self.lanes[l].objs[i].x;
            self.x += ftol(ox) - self.ride_x0;
            if self.x < 0 || self.x > w {
                self.ride = None;
                self.die(host);
                if host.var("FROG_QUIT") != 0 {
                    return Step::Return;
                }
            }
        }
        if let Some(li) = self.lane_idx(self.lane) {
            for i in 0..self.lanes[li].objs.len() {
                let o = self.lanes[li].objs[i];
                let dy = f64::from(self.y) - f64::from(o.y);
                let dx = f64::from(self.x) - f64::from(o.x);
                let d2 = ftol_d(dx * dx + dy * dy);
                let d = ftol_d(f64::from(d2).sqrt());
                if o.kind < 10 {
                    if d <= o.hw + self.frog_hw
                        && f64::from(self.frog_hw) * 1.5 > f64::from(ftol_d(dy).wrapping_abs())
                    {
                        self.die(host);
                        if host.var("FROG_QUIT") != 0 {
                            return Step::Return;
                        }
                    }
                } else if d <= o.hw && !self.jumping {
                    // FUN_00499530.
                    self.ride = Some((li, i));
                    self.ride_x0 = ftol(o.x);
                }
            }
        }
        if !self.jumping
            && self
                .lane_idx(self.lane)
                .is_some_and(|i| self.lanes[i].kind == 2)
            && self.ride.is_none()
        {
            self.die(host);
            if host.var("FROG_QUIT") != 0 {
                return Step::Return;
            }
        }
        for (i, name) in SYMBOL_VARS.iter().enumerate() {
            let s = self.symbols[i];
            if s.found != 0 || s.level != self.level || self.scrolling || self.level_up {
                continue;
            }
            let dy = f64::from(self.y) - f64::from(s.y);
            let dx = f64::from(self.x) - f64::from(s.x);
            let d = ftol_d(f64::from(ftol_d(dx * dx + dy * dy)).sqrt());
            if d <= self.sym_hw + self.frog_hw {
                self.symbols[i].found = 1;
                host.set_var(name, 1);
            }
        }
        Step::Draw
    }

    /// FUN_00496b20.
    fn draw(&mut self, host: &mut dyn Host) {
        let dt = host.frame_time();
        // The screens (FUN_004b5140 with the key 0xff000000).
        let h = HEIGHT as i32;
        if self.scrolling {
            let s = ftol_d(f64::from(h) * f64::from(self.scroll));
            if let Some(cur) = self.bg[0].as_ref() {
                blit_rows(host.surface(), cur, 0, 0x257 - s, s);
            }
            let (src_top, src_bottom) = if s == 0 {
                (0x257, 0x257)
            } else {
                (0x258 - s, 0x257)
            };
            if let Some(next) = self.bg[1].as_ref() {
                blit_rows(host.surface(), next, src_top, src_bottom, 0);
            }
        } else if let Some(cur) = self.bg[0].as_ref() {
            blit_rows(host.surface(), cur, 0, 0x257, 0);
        }
        if !self.scrolling {
            if (1..=4).contains(&self.level) {
                const E: [&str; 4] = ["F_E1.spr", "F_E2.spr", "F_E3.spr", "F_E4.spr"];
                let i = (self.level - 1) as usize;
                let e = self.banks.e[i].clone();
                self.level_anim = (f64::from(dt) * 0.5 + f64::from(self.level_anim)) as f32;
                let n = frames(e.as_ref());
                if n as f32 <= self.level_anim {
                    self.level_anim = 0.0;
                }
                aff(
                    host,
                    E[i],
                    e.as_ref(),
                    ftol(self.level_anim) as usize,
                    Some(0),
                    0x1e,
                    1.0,
                );
                if n != 0 {
                    let f = ftol_d(f64::from(self.level_anim) + 1.0) % n;
                    aff(host, E[i], e.as_ref(), f as usize, Some(0), 0x10e, 1.0);
                }
            }
            let a = ((self.level - 1).clamp(0, 3) as usize) * LANES;
            for li in a..a + LANES {
                for oi in 0..self.lanes[li].objs.len() {
                    let o = &mut self.lanes[li].objs[oi];
                    o.anim = (f64::from(dt) + f64::from(o.anim)) as f32;
                    let (name, bank) = match o.kind {
                        0 => ("F_LIM_D.spr", self.banks.lim_d.clone()),
                        1 => ("F_MUR_G.spr", self.banks.mur_g.clone()),
                        0x14 => ("F_BAR_G.spr", self.banks.bar_g.clone()),
                        _ => ("F_BAR_D.spr", self.banks.bar_d.clone()),
                    };
                    if frames(bank.as_ref()) as f32 <= o.anim {
                        o.anim = 0.0;
                    }
                    let o = *o;
                    aff(
                        host,
                        name,
                        bank.as_ref(),
                        ftol(o.anim) as usize,
                        Some(ftol(o.x)),
                        ftol(o.y),
                        1.0,
                    );
                }
            }
            for i in 0..4 {
                let s = self.symbols[i];
                if s.level != self.level {
                    continue;
                }
                // Symbols 1, 2 draw `F_S4`, 3, 4 `F_S1` (FUN_004992a0).
                let (name, bank) = if i < 2 {
                    ("F_S4.spr", self.banks.s[3].clone())
                } else {
                    ("F_S1.spr", self.banks.s[0].clone())
                };
                let frame = if s.found == 1 {
                    Some(1)
                } else {
                    let t = f64::from(s.timer) - f64::from(dt);
                    self.symbols[i].timer = t as f32;
                    if t < 0.0 {
                        self.symbols[i].timer = 0.5;
                        self.symbols[i].blink = !s.blink;
                    }
                    self.symbols[i].blink.then_some(0)
                };
                if let Some(f) = frame {
                    aff(
                        host,
                        name,
                        bank.as_ref(),
                        f,
                        Some(ftol(s.x)),
                        ftol(s.y),
                        s.alpha,
                    );
                }
            }
        }
        let frog = self.banks.frog.clone();
        self.frog_anim = (f64::from(dt) * 3.0 + f64::from(self.frog_anim)) as f32;
        if frames(frog.as_ref()) as f32 <= self.frog_anim {
            self.frog_anim = 0.0;
        }
        aff(
            host,
            "F_Vr1.spr",
            frog.as_ref(),
            ftol(self.frog_anim) as usize,
            Some(self.x),
            self.y,
            1.0,
        );
        let vi = self.banks.vi.clone();
        for i in 0..self.lives.max(0) {
            aff(
                host,
                "F_vi.spr",
                vi.as_ref(),
                0,
                Some(0x113 + 0x1a * i),
                BAR_Y,
                1.0,
            );
        }
        let gauge = ftol_d((300.0 - f64::from(self.timer)) * f64::from(0.05f32));
        aff(
            host,
            "F_JA.spr",
            self.banks.ja.clone().as_ref(),
            gauge.max(0) as usize,
            Some(0x82),
            BAR_Y,
            1.0,
        );
        aff(
            host,
            "F_qu.spr",
            self.banks.qu.clone().as_ref(),
            0,
            Some(EXIT_X),
            BAR_Y,
            1.0,
        );
        let fl = self.banks.fl.clone();
        for (f, x) in [ARROW_RIGHT, ARROW_UP, ARROW_DOWN, ARROW_LEFT] {
            aff(host, "F_FL.spr", fl.as_ref(), f, Some(x), BAR_Y, 1.0);
        }
        host.draw_pointer();
        host.flip();
    }
}

impl Puzzle for Frogger {
    fn id(&self) -> PuzzleId {
        PuzzleId::Frogger
    }

    /// FUN_00495420.
    fn frame(&mut self, host: &mut dyn Host) {
        if !host.script_prologue() {
            return;
        }
        match self.step(host) {
            Step::Return => {}
            Step::ClearAndReturn => host.set_click(false),
            Step::Draw => {
                self.draw(host);
                host.set_click(false);
            }
        }
    }

    /// FUN_00495040's game part. Its light-dialog branch (0x006e6f59:
    /// the dialog's sound removed, FUN_004111e0) is not ported [UNKNOWN:
    /// no 2004 script opens a light dialog during Frogger].
    fn free(&mut self, host: &mut dyn Host) {
        self.banks = Banks::default();
        self.bg = [None, None];
        self.reset_level();
        host.sfx_remove("frog_mort.wav");
        host.sfx_remove("frog_saute.wav");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::super::sim::SimHost;
    use super::*;

    /// A game with half widths given (no banks here).
    fn game() -> (Frogger, SimHost) {
        let mut g = Frogger::new();
        for l in &mut g.lanes {
            for o in &mut l.objs {
                o.hw = if o.kind < 10 { 30 } else { 60 };
            }
        }
        let mut h = SimHost::new(0.04);
        h.set_var("FROG_DEBUT", 1);
        (g, h)
    }

    fn run(g: &mut Frogger, h: &mut SimHost, frames: usize) {
        for _ in 0..frames {
            if g.step(h) == Step::Draw {
                h.click = false;
            }
        }
    }

    /// The objects out of the way (tests that are not about them).
    fn clear(g: &mut Frogger) {
        for l in &mut g.lanes {
            l.objs.clear();
        }
    }

    fn key(g: &mut Frogger, h: &mut SimHost, code: i32) {
        h.key_code = code;
        g.step(h);
        h.key_code = 0;
    }

    #[test]
    fn lane_table() {
        let l = lanes();
        assert_eq!(l.len(), 80);
        assert_eq!(l[0].y, 530.0);
        assert_eq!(l[9].y, 15.0);
        assert_eq!(l[10].y, -20.0);
        assert_eq!(l[11].y, 0.0);
        assert_eq!(l[1].objs[1].x, 110.0);
        assert_eq!(l[41].objs[0].speed, -50.0);
        assert_eq!(l[65].objs.len(), 4);
        assert_eq!(l[63].objs[0].speed, -70.0);
        assert_eq!(l[4].objs[1].anim, 2.0);
        assert!(l.iter().all(|l| l.objs.iter().all(|o| o.y == l.y)));
        let kinds: Vec<i32> = l[..11].iter().map(|l| l.kind).collect();
        assert_eq!(kinds, [0, 1, 1, 2, 2, 1, 1, 2, 2, 0, 0]);
    }

    #[test]
    fn jumps() {
        let (mut g, mut h) = game();
        clear(&mut g);
        key(&mut g, &mut h, KEY_UP);
        assert!(g.jumping);
        assert_eq!((g.lane, g.ty, g.dir), (1, 470, UP));
        assert_eq!(h.sounds, ["frog_saute.wav"]);
        // 4 per second: 0.16 a frame.
        run(&mut g, &mut h, 3);
        assert_eq!(g.y, ftol_d(530.0 - 60.0 * f64::from(g.progress)));
        run(&mut g, &mut h, 4);
        assert!(!g.jumping);
        assert_eq!(g.y, 470);
        // A key held: a new jump each time one ends (the code is not
        // cleared), none while one runs.
        h.key_code = KEY_RIGHT;
        g.step(&mut h);
        assert_eq!((g.dir, g.tx), (RIGHT, 440));
        let tx = g.tx;
        run(&mut g, &mut h, 1);
        assert_eq!(g.tx, tx);
        h.key_code = 0;
        run(&mut g, &mut h, 7);
        assert_eq!(g.x, 440);
        // Down from lane 0 is refused (y 530 ≥ 0x1fe).
        let (mut g, mut h) = game();
        key(&mut g, &mut h, KEY_DOWN);
        assert!(!g.jumping);
        // An unknown code does nothing.
        key(&mut g, &mut h, 0x6e);
        assert!(!g.jumping);
        // Left needs x > 40, right x < 800 − 40.
        g.x = 40;
        key(&mut g, &mut h, KEY_LEFT);
        assert!(!g.jumping);
        g.x = 760;
        key(&mut g, &mut h, KEY_RIGHT);
        assert!(!g.jumping);
        key(&mut g, &mut h, KEY_LEFT);
        assert!(g.jumping);
    }

    #[test]
    fn cars_and_water() {
        let (mut g, mut h) = game();
        clear(&mut g);
        g.lanes[1].objs = vec![Obj {
            x: 400.0,
            y: 470.0,
            speed: 0.0,
            kind: 1,
            anim: 0.0,
            hw: 30,
        }];
        key(&mut g, &mut h, KEY_UP);
        // The lane moved at the jump's start: the car is tested at once,
        // and kills once |dy| < 1.5 × 15.
        run(&mut g, &mut h, 8);
        assert_eq!(g.lives, 2);
        assert_eq!((g.lane, g.x, g.y), (0, 400, 530));
        assert!(h.sounds.iter().any(|s| s == "frog_mort.wav"));
        // Water without a boat.
        let (mut g, mut h) = game();
        clear(&mut g);
        g.lane = 3;
        g.y = 350;
        g.step(&mut h);
        assert_eq!((g.lives, g.lane), (2, 0));
    }

    #[test]
    fn boat_carries() {
        let (mut g, mut h) = game();
        clear(&mut g);
        g.lanes[3].objs = vec![Obj {
            x: 400.0,
            y: 350.0,
            speed: -100.0,
            kind: 20,
            anim: 0.0,
            hw: 60,
        }];
        g.lane = 3;
        g.y = 350;
        g.started = true;
        g.step(&mut h);
        // The boat moved first, then the test caught the frog.
        assert_eq!(g.ride, Some((3, 0)));
        assert_eq!(g.lives, 3);
        assert_eq!(g.ride_x0, 396);
        run(&mut g, &mut h, 10);
        assert_eq!(g.x, 360);
        // Carried off the left edge: death.
        g.x = 2;
        run(&mut g, &mut h, 1);
        assert_eq!(g.lives, 2);
    }

    #[test]
    fn symbol_and_levels() {
        let (mut g, mut h) = game();
        clear(&mut g);
        g.lane = 9;
        g.y = 15;
        g.x = 200;
        g.step(&mut h);
        // The symbol not held: the top refuses.
        key(&mut g, &mut h, KEY_UP);
        assert!(!g.jumping);
        g.x = 400;
        g.step(&mut h);
        assert_eq!(h.var("FROG_PREMIER_SYMB"), 1);
        assert_eq!(g.symbols[0].found, 1);
        key(&mut g, &mut h, KEY_UP);
        assert!(g.jumping && g.level_up);
        assert_eq!((g.ty, g.lane), (-20, 0));
        run(&mut g, &mut h, 7);
        assert!(g.scrolling);
        assert_eq!(g.level, 2);
        // Two seconds of scrolling, then the jump in from the bottom.
        let mut n = 0;
        while g.scrolling {
            run(&mut g, &mut h, 1);
            n += 1;
        }
        assert!((50..=51).contains(&n), "{n}");
        assert!(g.jumping);
        assert_eq!((g.y0, g.ty), (600, 530));
        run(&mut g, &mut h, 7);
        assert_eq!(g.y, 530);
        // Level 4 wants FROG_QUATRIEME_SYMB = 3 (the script's).
        g.level = 4;
        g.lane = 9;
        g.y = 15;
        g.x = 100;
        h.set_var("FROG_QUATRIEME_SYMB", 1);
        key(&mut g, &mut h, KEY_UP);
        assert!(!g.jumping);
        h.set_var("FROG_QUATRIEME_SYMB", 3);
        key(&mut g, &mut h, KEY_UP);
        assert!(g.level_up);
        run(&mut g, &mut h, 7);
        assert_eq!(h.var("FROG_GAGNE"), 1);
        assert!(g.won);
        assert_eq!(g.level, 1);
        assert_eq!(g.step(&mut h), Step::Return);
    }

    #[test]
    fn quits() {
        // The time.
        let (mut g, mut h) = game();
        clear(&mut g);
        g.started = true;
        g.timer = 0.05;
        run(&mut g, &mut h, 1);
        assert_eq!(h.var("FROG_QUIT"), 0);
        run(&mut g, &mut h, 1);
        assert_eq!(h.var("FROG_QUIT"), 1);
        assert_eq!(g.timer, 0.0);
        // The lives.
        let (mut g, mut h) = game();
        clear(&mut g);
        g.level = 3;
        g.lives = 1;
        g.lane = 3;
        g.y = 350;
        assert_eq!(g.step(&mut h), Step::Return);
        assert_eq!((g.lives, g.level), (0, 1));
        assert_eq!(h.var("FROG_QUIT"), 1);
        // Level 1's lane 2 reached: FROG_TRAVERSE_UNE_RUE.
        let (mut g, mut h) = game();
        clear(&mut g);
        key(&mut g, &mut h, KEY_UP);
        run(&mut g, &mut h, 7);
        key(&mut g, &mut h, KEY_UP);
        run(&mut g, &mut h, 7);
        assert_eq!(h.var("FROG_TRAVERSE_UNE_RUE"), 1);
    }

    #[test]
    fn wrap() {
        let (mut g, mut h) = game();
        clear(&mut g);
        g.lanes[1].objs = vec![Obj {
            x: 831.0,
            y: 470.0,
            speed: 10.0,
            kind: 0,
            anim: 0.0,
            hw: 30,
        }];
        g.started = true;
        g.step(&mut h);
        // 831 > 800 + 30: − 860, then + 0.4.
        let dx = f64::from(0.04f32) * 10.0;
        assert_eq!(g.lanes[1].objs[0].x, (-29.0 + dx) as f32);
        g.lanes[1].objs[0].speed = -10.0;
        g.lanes[1].objs[0].x = -31.0;
        g.step(&mut h);
        assert_eq!(g.lanes[1].objs[0].x, (829.0 - dx) as f32);
    }
}

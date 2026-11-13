//! `PUZ_DEFENDER` (loop mode 0x10): a side-scrolling helicopter shooter
//! over the ground of `map\Chop_01..04.tga` (atlantis4-cd-en-3022848:
//! init FUN_004861d0, start FUN_004870b0, free FUN_00487100, frame
//! FUN_00487460). The CD starts it in chapter 22 (/004, the seat's button
//! clicked, after the film `DESCENTE_CASQUE_BLOC`); its `/puz` entries
//! /005 (`DEFENDER_GAGNE=1`: `FINPUZZLE`, the film `PASSAGE_SALLE_BLOC2`,
//! chapter 23) and /006 (`DEFENDER_PERDU=1`: `FINPUZZLE`) react to the
//! game's two end variables.
//!
//! The level (`puzzles\level_01.ini`, read by FUN_0048b700): the deviants
//! (ground turrets that shoot at the ship when it is to their left), the
//! energy items, the power-ups (none in the CD's level) and the three
//! gods `KAMA`, `ENNA`, `ZANAT`, each flying a Lorenz attractor around a
//! fixed centre. The scroll (0x0071753c, 40 px/s) stops at 2600 until
//! KAMA is destroyed, at 5590 until ENNA then ZANAT are: ZANAT destroyed
//! is the win. The ship follows the free pointer (the mouse) and the
//! arrow keys; X (code 0x2d) or the left button fires, Z (code 0x2c) or
//! the right button drops a bomb, O / P (codes 0x18 / 0x19) change the
//! scroll's speed by ∓ 10. Touching the ground, or being pushed off the
//! screen's left edge, or falling at 0 energy, costs one of three lives
//! (the level restarts); the last one sets `DEFENDER_PERDU`.
//!
//! Banks (`spr\`): `VaisseauChop.spr` (the ship), `jauge_chop.spr` (the
//! energy gauge, 15 frames), `fond_chop.spr` (the ground's collision
//! shapes, one per page), `tirdev_chop.spr` (a deviant's shot),
//! `ExpChop.spr` (explosions), `DeviantChop.spr` (#0 alive, #1 dying),
//! `ItemChop.spr` (#0 energy, #1..#3 power-ups), `Tir_Chop01.spr` (the
//! ship's shot), `BombeChop.spr`, `dieux_chop.spr` (#0 KAMA, #1 ENNA,
//! #2 ZANAT), `tirkama_chop.spr`, `tirenna_chop.spr`, `tirzanat_chop.spr`
//! (their shots), `life_chop.spr` (a life). Sounds: `cata_exp2.wav` (any
//! hit), `cata_boulet.wav` (a bomb), `cata_tir.wav` (a shot).
//!
//! Floats: x87 at precision 53 bits (0x027f): a value on the x87 stack is
//! an `f64` here, one stored to memory an `f32`.

// The x87 tests (`FCOMP` + `TEST AH, 0x41`) are kept as written: a
// negated comparison is the original's, NaN included.
#![allow(clippy::neg_cmp_op_on_partial_ord)]
// The gods are walked by index: their shots go into `self.shots`.
#![allow(clippy::needless_range_loop)]

use std::sync::Arc;

use atlantis3_data::spr::Bank;

use super::{aff, collision, ftol, ftol_d, Host, Puzzle, PuzzleId};
use crate::px::Surface;

/// `GetKeyboardState`'s keys (@ 0x00487565..0x004875c2).
pub const VK_LEFT: u8 = 0x25;
pub const VK_UP: u8 = 0x26;
pub const VK_RIGHT: u8 = 0x27;
pub const VK_DOWN: u8 = 0x28;

/// The key codes the frame reads (0x006e9e6c): X fires, Z bombs, O / P
/// change the scroll's speed.
pub const CODE_FIRE: i32 = 0x2d;
pub const CODE_BOMB: i32 = 0x2c;
pub const CODE_SLOWER: i32 = 0x18;
pub const CODE_FASTER: i32 = 0x19;

/// The ground's pages tested (0x007174a0 := 8 by FUN_0048d350): 8 × 800
/// px, the four pictures twice.
pub const PAGES: i32 = 8;

/// The gods' Lorenz centres (FUN_00489f70: x = a · 5 + cx, y = b · 5 + cy):
/// KAMA (3220, 167), ENNA and ZANAT (6170, 190).
pub const GOD_CENTRES: [(f32, f32); 3] = [(3220.0, 167.0), (6170.0, 190.0), (6170.0, 190.0)];

/// The level file (`puzzles\` FUN_00418be0 + `level_01.ini`, 0x006b2128).
pub const LEVEL_FILE: &str = "puzzles/level_01.ini";

/// A shot (the list 0x007173c8, nodes of 10 dwords).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    /// World position (+0x00).
    pub pos: (f32, f32),
    /// The velocity at the shot's creation (+0x08; read by the deviant
    /// shot's frame, FUN_0048acd0).
    pub start: (f32, f32),
    /// Velocity (+0x10).
    pub vel: (f32, f32),
    /// +0x18: 0 the ship's shot, 1 a deviant's, 2 a bomb, 3 KAMA's, 4
    /// ENNA's, 5 ZANAT's.
    pub kind: i32,
    /// +0x1c: the animation frame; a bomb's −1 while it falls, its
    /// explosion's frame after.
    pub anim: f32,
    /// +0x20 frames, +0x24 frames per second.
    pub frames: i32,
    pub speed: i32,
}

/// A deviant (the list 0x007173e0, 9 dwords).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deviant {
    pub pos: (f32, f32),
    pub frame: f32,
    pub frames: i32,
    pub speed: f32,
    /// 0 alive, 1 dying, 2 gone (removed).
    pub state: i32,
    /// `IntervalleTir` (s), `TempsDernierTir` (the timer), `Probabilite`.
    pub interval: f32,
    pub timer: f32,
    pub proba: i32,
}

/// An energy item (the list 0x007173ec, 5 dwords).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Energy {
    pub pos: (f32, f32),
    pub frame: f32,
    pub frames: i32,
    pub speed: f32,
}

/// A power-up (the list 0x007173f8, 6 dwords): 0 `Life` (+20 energy), 1
/// `Bombe` (one bomb more), 2 `MegaBombe` (the near deviants die).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerUp {
    pub pos: (f32, f32),
    pub kind: i32,
    pub frame: f32,
    pub frames: i32,
    pub speed: f32,
}

/// A god (0x00717404 KAMA, 0x00717438 ENNA, 0x0071746c ZANAT; 0x34
/// bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct God {
    pub pos: (f32, f32),
    /// +0x08 (100 at the level's start).
    pub energy: f32,
    /// +0x0c `IntervalleTir` × 0.001, +0x10 the timer, +0x14
    /// `Probabilite`.
    pub interval: f32,
    pub timer: f32,
    pub proba: i32,
    /// +0x18: on the screen (set when the scroll stops for it).
    pub active: bool,
    /// +0x1c the attractor's point (0.01, 0, 0), +0x28 its last step.
    pub lorenz: [f32; 3],
    pub step: [f32; 3],
}

/// An explosion (the list 0x00717118: x, y, frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Explosion {
    pub pos: (f32, f32),
    pub frame: f32,
}

/// The level read by FUN_0048b700.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Level {
    pub deviants: Vec<Deviant>,
    pub energies: Vec<Energy>,
    pub power_ups: Vec<PowerUp>,
    /// The `[Dieu]` sections' fields by name: KAMA, ENNA, ZANAT (`Pos`,
    /// `IntervalleTir` × 0.001, `TempsDernierTir`, `Probabilite`).
    pub gods: [Option<GodDef>; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GodDef {
    pub pos: Option<(f32, f32)>,
    pub interval: Option<f32>,
    pub timer: Option<f32>,
    pub proba: Option<i32>,
}

/// `atof` (FUN_00668e7c) stored as a float.
fn atof(s: Option<&str>) -> f32 {
    let s = s.unwrap_or("");
    // The leading number of the token (the CRT stops at the first
    // character it cannot read).
    let end = s
        .char_indices()
        .take_while(|&(i, c)| c.is_ascii_digit() || c == '.' || (i == 0 && (c == '-' || c == '+')))
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());
    s[..end].parse::<f64>().unwrap_or(0.0) as f32
}

/// `atoi` (FUN_00668e71).
fn atoi(s: Option<&str>) -> i32 {
    let s = s.unwrap_or("");
    let end = s
        .char_indices()
        .take_while(|&(i, c)| c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')))
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());
    s[..end].parse::<i32>().unwrap_or(0)
}

impl Level {
    /// FUN_0048b700: line by line (FUN_00449d70); a line starting with `'`
    /// is a comment, an empty one is skipped; a line's first word
    /// (FUN_00449180) names a section — `[Deviant]`, `[Energie]`,
    /// `[PowerUp]`, `[Dieu] <name>` — whose field lines (first word, then
    /// the values: FUN_004492a0 `atof`, FUN_00449240 `atoi`) run to the
    /// line `[End]` (FUN_00449470 compares the whole line). The words are
    /// separated by blanks (the file's tabs and spaces; FUN_0044aa70's
    /// exact set [UNKNOWN: not read]). Defaults: a deviant (0, 0), frame
    /// 0, 1 frame, 15 frames/s, alive, every 2 s, timer 0, 100 %; an
    /// energy (0, 0), frame 0, 0 frames, 15/s; a power-up (0, 0), `Bombe`
    /// (1), frame 0, 0 frames, 15/s. A god's section writes its fields
    /// directly; a god without one keeps the previous values (FUN_0048d350
    /// only resets its energy, flag and point).
    pub fn parse(data: &[u8]) -> Level {
        let text = String::from_utf8_lossy(data);
        let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
        let mut level = Level::default();
        while let Some(line) = lines.next() {
            if line.starts_with('\'') || line.is_empty() {
                continue;
            }
            let mut words = line.split_whitespace();
            let head = words.next().unwrap_or("");
            match head {
                "[Deviant]" => {
                    let mut d = Deviant {
                        pos: (0.0, 0.0),
                        frame: 0.0,
                        frames: 1,
                        speed: 15.0,
                        state: 0,
                        interval: 2.0,
                        timer: 0.0,
                        proba: 100,
                    };
                    for f in section(&mut lines) {
                        let mut w = f.split_whitespace();
                        match w.next().unwrap_or("") {
                            "Pos" => d.pos = (atof(w.next()), atof(w.next())),
                            "Frame" => d.frame = atof(w.next()),
                            "Frames" => d.frames = atoi(w.next()),
                            "VitesseAnim" => d.speed = atof(w.next()),
                            "IntervalleTir" => d.interval = atof(w.next()),
                            "TempsDernierTir" => d.timer = atof(w.next()),
                            "Probabilite" => d.proba = atoi(w.next()),
                            _ => {}
                        }
                    }
                    level.deviants.push(d);
                }
                "[Energie]" => {
                    let mut e = Energy {
                        pos: (0.0, 0.0),
                        frame: 0.0,
                        frames: 0,
                        speed: 15.0,
                    };
                    for f in section(&mut lines) {
                        let mut w = f.split_whitespace();
                        match w.next().unwrap_or("") {
                            "Pos" => e.pos = (atof(w.next()), atof(w.next())),
                            "Frame" => e.frame = atof(w.next()),
                            "Frames" => e.frames = atoi(w.next()),
                            "VitesseAnim" => e.speed = atof(w.next()),
                            _ => {}
                        }
                    }
                    level.energies.push(e);
                }
                "[PowerUp]" => {
                    let mut p = PowerUp {
                        pos: (0.0, 0.0),
                        kind: 1,
                        frame: 0.0,
                        frames: 0,
                        speed: 15.0,
                    };
                    for f in section(&mut lines) {
                        let mut w = f.split_whitespace();
                        match w.next().unwrap_or("") {
                            "Pos" => p.pos = (atof(w.next()), atof(w.next())),
                            "Type" => {
                                p.kind = match w.next().unwrap_or("") {
                                    "Life" => 0,
                                    "Bombe" => 1,
                                    "MegaBombe" => 2,
                                    _ => p.kind,
                                }
                            }
                            "Frame" => p.frame = atof(w.next()),
                            "Frames" => p.frames = atoi(w.next()),
                            "VitesseAnim" => p.speed = atof(w.next()),
                            _ => {}
                        }
                    }
                    level.power_ups.push(p);
                }
                "[Dieu]" => {
                    let k = match words.next().unwrap_or("") {
                        "KAMA" => 0,
                        "ENNA" => 1,
                        "ZANAT" => 2,
                        _ => continue,
                    };
                    let mut g = level.gods[k].unwrap_or_default();
                    for f in section(&mut lines) {
                        let mut w = f.split_whitespace();
                        match w.next().unwrap_or("") {
                            "Pos" => g.pos = Some((atof(w.next()), atof(w.next()))),
                            "IntervalleTir" => {
                                g.interval =
                                    Some((f64::from(atof(w.next())) * f64::from(0.001f32)) as f32)
                            }
                            "TempsDernierTir" => g.timer = Some(atof(w.next())),
                            "Probabilite" => g.proba = Some(atoi(w.next())),
                            _ => {}
                        }
                    }
                    level.gods[k] = Some(g);
                }
                _ => {}
            }
        }
        level
    }
}

/// A section's field lines up to `[End]` (comments and empty lines
/// skipped).
fn section<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut out = Vec::new();
    for l in lines.by_ref() {
        if l == "[End]" {
            break;
        }
        if l.starts_with('\'') || l.is_empty() {
            continue;
        }
        out.push(l);
    }
    out
}

/// The ship (0x00717340 screen, 0x00717348 world, 0x00717108 / 0x00717110
/// the previous frame's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ship {
    pub screen: (f32, f32),
    pub world: (f32, f32),
    pub prev_screen: (f32, f32),
    pub prev_world: (f32, f32),
}

/// The game's state, without its banks (the part the frame's logic
/// changes; [`Defender::state`]).
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub ship: Ship,
    /// 0x00717354 (0x00717350, its maximum, is never read again).
    pub energy: f32,
    /// 0x00717360: 999 (only a `Bombe` power-up changes it).
    pub bombs: i32,
    /// 0x006b1f28 (.data 3).
    pub lives: i32,
    /// 0x006b1f2c (.data 40.0): px/s.
    pub speed: f32,
    /// 0x0071753c, and its page / column (0x00717540, 0x00717544).
    pub scroll: f32,
    pub page: i32,
    pub column: i32,
    /// 0x0071753b: the scroll accelerates (+100 px/s²) toward a stop.
    pub accel: bool,
    pub shots: Vec<Shot>,
    pub deviants: Vec<Deviant>,
    pub energies: Vec<Energy>,
    pub power_ups: Vec<PowerUp>,
    pub gods: [God; 3],
    pub explosions: Vec<Explosion>,
}

/// The game.
pub struct Defender {
    pub st: State,
    /// The level file as read (FUN_0048d350 reads it again at each life).
    level: Level,
    banks: Banks,
    /// The ground's pictures (`map\Chop_01..04.tga`, 0x00717140 + 0x80 k).
    pages: Vec<Option<Surface>>,
}

#[derive(Default, Clone)]
struct Banks {
    ship: Option<Arc<Bank>>,
    gauge: Option<Arc<Bank>>,
    ground: Option<Arc<Bank>>,
    tir_dev: Option<Arc<Bank>>,
    exp: Option<Arc<Bank>>,
    deviant: Option<Arc<Bank>>,
    item: Option<Arc<Bank>>,
    tir: Option<Arc<Bank>>,
    bomb: Option<Arc<Bank>>,
    gods: Option<Arc<Bank>>,
    tir_kama: Option<Arc<Bank>>,
    tir_enna: Option<Arc<Bank>>,
    tir_zanat: Option<Arc<Bank>>,
    life: Option<Arc<Bank>>,
}

/// The sprites the logic tests (`SPR_COLLISION`); `None` banks touch
/// nothing.
#[derive(Default, Clone)]
pub struct Shapes {
    pub ship: Option<Arc<Bank>>,
    pub ground: Option<Arc<Bank>>,
    pub deviant: Option<Arc<Bank>>,
    pub bomb: Option<Arc<Bank>>,
    pub exp: Option<Arc<Bank>>,
    pub gods: Option<Arc<Bank>>,
}

const SFX: [&str; 3] = ["cata_exp2.wav", "cata_boulet.wav", "cata_tir.wav"];
const HIT: &str = "cata_exp2.wav";

impl State {
    /// The set-up of FUN_004861d0 and FUN_0048d350 without the loads: the
    /// ship at (50, h / 2) (h the display's height, before the stretch
    /// surface opens), energy 100, 999 bombs, no shot, the level's lists,
    /// the gods at 100 with their attractors at (0.01, 0, 0), speed 40,
    /// scroll 0, 3 lives.
    pub fn new(level: &Level, h: i32) -> State {
        let half = (h / 2) as f32;
        let mut st = State {
            ship: Ship {
                screen: (50.0, half),
                world: (50.0, half),
                prev_screen: (50.0, half),
                prev_world: (50.0, half),
            },
            energy: 100.0,
            bombs: 999,
            lives: 3,
            speed: 40.0,
            scroll: 0.0,
            page: 0,
            column: 0,
            accel: false,
            shots: Vec::new(),
            deviants: Vec::new(),
            energies: Vec::new(),
            power_ups: Vec::new(),
            gods: [God {
                pos: (0.0, 0.0),
                energy: 100.0,
                interval: 0.0,
                timer: 0.0,
                proba: 0,
                active: false,
                lorenz: [0.01, 0.0, 0.0],
                step: [0.0; 3],
            }; 3],
            explosions: Vec::new(),
        };
        st.load_level(level);
        st
    }

    /// FUN_0048d350: the lists cleared, the gods' energies 100, flags
    /// clear, attractors (0.01, 0, 0); speed 40; 8 pages; the file's
    /// contents.
    pub fn load_level(&mut self, level: &Level) {
        self.deviants = level.deviants.clone();
        self.energies = level.energies.clone();
        self.power_ups = level.power_ups.clone();
        for (g, def) in self.gods.iter_mut().zip(&level.gods) {
            g.energy = 100.0;
            g.active = false;
            g.lorenz = [0.01, 0.0, 0.0];
            if let Some(d) = def {
                if let Some(p) = d.pos {
                    g.pos = p;
                }
                if let Some(v) = d.interval {
                    g.interval = v;
                }
                if let Some(v) = d.timer {
                    g.timer = v;
                }
                if let Some(v) = d.proba {
                    g.proba = v;
                }
            }
        }
        self.speed = 40.0;
    }

    /// FUN_0048b5e0 (a life lost, more left): the ship at (50, h / 2),
    /// energy 100, 999 bombs, the pointer at (100, h / 2 − 50), the
    /// scroll at 0, the shots cleared, the level loaded again. The
    /// acceleration flag and the explosions are kept.
    pub fn restart(&mut self, level: &Level, h: i32) -> (i32, i32) {
        let half = (h / 2) as f32;
        self.ship = Ship {
            screen: (50.0, half),
            world: (50.0, half),
            prev_screen: (50.0, half),
            prev_world: (50.0, half),
        };
        self.energy = 100.0;
        self.bombs = 999;
        self.scroll = 0.0;
        self.page = 0;
        self.column = 0;
        self.shots.clear();
        self.load_level(level);
        (100, h / 2 - 50)
    }

    /// The ship's shot (@ 0x0048781b, 0x00487aa6): from (world + (30,
    /// 10)), (500, 0) px/s when the scroll stopped, else (300, 300).
    fn fire(&mut self, host: &mut dyn Host) {
        let (wx, wy) = self.ship.world;
        let y = (f64::from(wy) + 10.0) as f32;
        let x = (f64::from(wx) + 30.0) as f32;
        let vel = if self.speed == 0.0 {
            (500.0, 0.0)
        } else {
            (300.0, 300.0)
        };
        host.sfx_play("cata_tir.wav");
        self.shots.push(Shot {
            pos: (x, y),
            start: (0.0, 0.0),
            vel,
            kind: 0,
            anim: 0.0,
            frames: 7,
            speed: 15,
        });
    }

    /// A bomb (@ 0x0048796e, 0x00487c0b): from (world + (30, 10)) at (50,
    /// 300) px/s, falling.
    fn bomb(&mut self, host: &mut dyn Host) {
        let (wx, wy) = self.ship.world;
        let y = (f64::from(wy) + 10.0) as f32;
        let x = (f64::from(wx) + 30.0) as f32;
        host.sfx_play("cata_boulet.wav");
        self.shots.push(Shot {
            pos: (x, y),
            start: (0.0, 0.0),
            vel: (50.0, 300.0),
            kind: 2,
            anim: -1.0,
            frames: 9,
            speed: 8,
        });
    }

    /// The ship's input (@ 0x0048755d..0x00487de4): the arrow keys and
    /// the pointer move it at 300 px/s (x from 20 to w − 20, y from 20
    /// down), the key code and the buttons fire; at 0 energy it falls at
    /// 200 px/s instead (at least 0.011 s a frame) and the pointer follows.
    pub fn input(&mut self, host: &mut dyn Host) {
        let up = host.key(VK_UP);
        let down = host.key(VK_DOWN);
        let left = host.key(VK_LEFT);
        let right = host.key(VK_RIGHT);
        let t = f64::from(host.frame_time());
        if !(self.energy > 0.0) {
            let tt = if t < f64::from(0.01f32) {
                f64::from(0.011f32)
            } else {
                t
            };
            self.ship.world.1 = (tt * 200.0 + f64::from(self.ship.world.1)) as f32;
            self.ship.screen.1 = self.ship.world.1;
            host.set_pointer(ftol(self.ship.screen.0), ftol(self.ship.world.1));
            return;
        }
        let (px, py) = host.pointer();
        let s = &mut self.ship;
        s.screen = (px as f32, py as f32);
        s.world = (
            (f64::from(self.scroll) + f64::from(px as f32)) as f32,
            py as f32,
        );
        let iscroll = || ftol(self.scroll) as f32;
        if up {
            let y = f64::from(s.screen.1) - t * 300.0;
            s.screen.1 = y as f32;
            if y < 20.0 {
                s.screen.1 = 20.0;
            }
            s.world = (
                (f64::from(iscroll()) + f64::from(s.screen.0)) as f32,
                s.screen.1,
            );
        }
        if down {
            s.screen.1 = (t * 300.0 + f64::from(s.screen.1)) as f32;
            s.world = (
                (f64::from(iscroll()) + f64::from(s.screen.0)) as f32,
                s.screen.1,
            );
        }
        if left {
            let x = f64::from(s.screen.0) - t * 300.0;
            s.screen.0 = x as f32;
            if x < 20.0 {
                s.screen.0 = 20.0;
            }
            s.world = (
                (f64::from(iscroll()) + f64::from(s.screen.0)) as f32,
                s.screen.1,
            );
        }
        if right {
            s.screen.0 = (t * 300.0 + f64::from(s.screen.0)) as f32;
            let max = host.display_size().0 - 20;
            if (max as f32) < s.screen.0 {
                s.screen.0 = max as f32;
            }
            s.world = (
                (f64::from(iscroll()) + f64::from(s.screen.0)) as f32,
                s.screen.1,
            );
        }
        match host.key_code() {
            CODE_FIRE => {
                self.fire(host);
                host.set_key_code(0);
            }
            CODE_BOMB => {
                if self.bombs > 0 && self.speed != 0.0 {
                    self.bomb(host);
                    host.set_key_code(0);
                }
            }
            CODE_SLOWER => {
                self.speed = (f64::from(self.speed) - 10.0) as f32;
                host.set_key_code(0);
            }
            CODE_FASTER => {
                self.speed = (f64::from(self.speed) + 10.0) as f32;
                host.set_key_code(0);
            }
            _ => {}
        }
        if host.click() {
            host.set_click(false);
            self.fire(host);
        }
        if host.right_click() {
            host.set_right_click(false);
            if self.bombs > 0 && self.speed != 0.0 {
                self.bomb(host);
            }
        }
    }

    /// FUN_004888b0(prev, cur): the ship's world point against the ground
    /// (`fond_chop` frame k & 3 at (800 k, 600), k the page and the next
    /// one, at the point and (±20, +10), (±10, +10)); while touching, the
    /// point moves halfway back toward the previous frame's, until it is
    /// within a pixel of it. True when the ground was touched; `cur`
    /// becomes the last point tested (whole pixels) or the previous one.
    pub fn ground_hit(&mut self, ground: Option<&Arc<Bank>>) -> bool {
        let prev = self.ship.prev_world;
        let cur = self.ship.world;
        let mut hit = false;
        let mut ix = ftol(cur.0);
        let mut fx = ix as f32;
        let mut mx = fx;
        let mut iy = ftol(cur.1);
        let mut my = iy as f32;
        let page = self.page;
        let near = |a: f32, b: f32| ftol_d(f64::from(a) - f64::from(b)).abs() <= 1;
        let mut touching;
        loop {
            touching = false;
            for k in page..page + 2 {
                let (frame, x) = ((k % 4) as usize, 800 * k);
                let pts = [
                    (ix, iy),
                    (ix + 20, iy + 10),
                    (ix - 20, iy + 10),
                    (ix + 10, iy + 10),
                    (ix - 10, iy + 10),
                ];
                if pts
                    .iter()
                    .any(|&(px, py)| collision(ground, frame, Some(x), 600, px, py))
                {
                    touching = true;
                    hit = true;
                    break;
                }
            }
            if near(fx, prev.0) && near(iy as f32, prev.1) {
                self.ship.world = prev;
                return hit;
            }
            if touching {
                mx = ((f64::from(mx) + f64::from(prev.0)) * 0.5) as f32;
                my = ((f64::from(my) + f64::from(prev.1)) * 0.5) as f32;
                ix = ftol(mx);
                iy = ftol(my);
            }
            fx = ix as f32;
            if fx == prev.0 && iy as f32 == prev.1 {
                self.ship.world = prev;
                return hit;
            }
            if !touching {
                break;
            }
        }
        if near(ix as f32, prev.0) && near(iy as f32, prev.1) {
            self.ship.world = prev;
        } else {
            self.ship.world = (ix as f32, iy as f32);
        }
        hit
    }

    /// FUN_00488b60: the shots move (bombs and deviants' shots slow down
    /// horizontally by 50 px/s² and fall at 200 px/s²; a bomb's explosion
    /// stays and plays its frames), then hit: the ship's shots and the
    /// bombs the alive deviants, the ship's shots the active gods (−20
    /// KAMA, −10 the others), the others the ship (−10 energy); the
    /// ground stops all but an exploding bomb (a falling one explodes);
    /// below y 600 a shot is gone.
    pub fn shots(&mut self, host: &mut dyn Host, sh: &Shapes) {
        let t = f64::from(host.frame_time());
        let mut i = 0;
        'shots: while i < self.shots.len() {
            let s = &mut self.shots[i];
            if s.kind == 1 || s.kind == 2 {
                if !(s.vel.0 <= 0.0) {
                    s.vel.0 = (f64::from(s.vel.0) - t * 50.0) as f32;
                    if s.vel.0 < 0.0 {
                        s.vel.0 = 0.0;
                    }
                } else {
                    s.vel.0 = (t * 50.0 + f64::from(s.vel.0)) as f32;
                    if !(s.vel.0 <= 0.0) {
                        s.vel.0 = 0.0;
                    }
                }
                s.vel.1 = (t * 200.0 + f64::from(s.vel.1)) as f32;
            }
            if s.kind == 2 && !(s.anim <= -1.0) {
                s.anim = (t * f64::from(s.speed) + f64::from(s.anim)) as f32;
                if ((s.frames - 1) as f32) < s.anim {
                    if !unlink(&mut self.shots, &mut i) {
                        break;
                    }
                    continue;
                }
            } else {
                let dy = (t * f64::from(s.vel.1)) as f32;
                s.pos.0 = (t * f64::from(s.vel.0) + f64::from(s.pos.0)) as f32;
                s.pos.1 = (f64::from(dy) + f64::from(s.pos.1)) as f32;
            }
            let s = self.shots[i];
            let (sx, sy) = (ftol(s.pos.0), ftol(s.pos.1));
            for d in self.deviants.iter_mut().filter(|d| d.state == 0) {
                let (dx, dy) = (ftol(d.pos.0), ftol(d.pos.1));
                let kill = match s.kind {
                    0 => collision(sh.deviant.as_ref(), 0, Some(dx), dy, sx, sy),
                    2 if s.anim <= -1.0 => collision(sh.bomb.as_ref(), 0, Some(sx), sy, dx, dy),
                    2 => {
                        if collision(sh.exp.as_ref(), ftol(s.anim) as usize, Some(sx), sy, dx, dy) {
                            d.state = 1;
                            d.frame = 0.0;
                        }
                        false
                    }
                    _ => false,
                };
                if kill {
                    d.state = 1;
                    d.frame = 0.0;
                    if !unlink(&mut self.shots, &mut i) {
                        break 'shots;
                    }
                    continue 'shots;
                }
            }
            if s.kind == 0 {
                const LOSS: [f64; 3] = [20.0, 10.0, 10.0];
                for k in 0..3 {
                    let g = &mut self.gods[k];
                    if g.active
                        && collision(
                            sh.gods.as_ref(),
                            k,
                            Some(ftol(g.pos.0)),
                            ftol(g.pos.1),
                            sx,
                            sy,
                        )
                    {
                        self.explosions.push(Explosion {
                            pos: s.pos,
                            frame: 0.0,
                        });
                        host.sfx_play(HIT);
                        let g = &mut self.gods[k];
                        g.energy = (f64::from(g.energy) - LOSS[k]) as f32;
                        if !unlink(&mut self.shots, &mut i) {
                            break 'shots;
                        }
                        continue 'shots;
                    }
                }
            }
            if matches!(s.kind, 1 | 3 | 4 | 5) {
                let (wx, wy) = self.ship.world;
                if collision(sh.ship.as_ref(), 0, Some(ftol(wx)), ftol(wy), sx, sy) {
                    self.energy = (f64::from(self.energy) - 10.0) as f32;
                    if self.energy < 0.0 {
                        self.energy = 0.0;
                    }
                    self.explosions.push(Explosion {
                        pos: s.pos,
                        frame: 0.0,
                    });
                    host.sfx_play(HIT);
                    if !unlink(&mut self.shots, &mut i) {
                        break 'shots;
                    }
                    continue 'shots;
                }
            }
            if s.kind != 2 || s.anim.to_bits() == (-1.0f32).to_bits() {
                for k in 0..PAGES {
                    if collision(
                        sh.ground.as_ref(),
                        (k % 4) as usize,
                        Some(800 * k),
                        600,
                        sx,
                        sy,
                    ) {
                        if s.kind != 2 {
                            if !unlink(&mut self.shots, &mut i) {
                                break 'shots;
                            }
                            continue 'shots;
                        }
                        self.shots[i].anim = 0.0;
                        host.sfx_play(HIT);
                        break;
                    }
                }
            }
            if self.shots[i].pos.1 > 600.0 {
                if !unlink(&mut self.shots, &mut i) {
                    break;
                }
                continue;
            }
            i += 1;
        }
    }

    /// FUN_00489bf0: the deviants within 900 px of the ship animate and,
    /// every `IntervalleTir` seconds, shoot with `Probabilite` % — only at a
    /// ship on their left (the branch for a ship on their right draws its
    /// random numbers and builds no shot): toward the ship 40 px above
    /// it, with a random spread, no rightward speed, at most 600 px/s. A
    /// dying deviant plays its frames and is removed.
    pub fn deviants(&mut self, host: &mut dyn Host) {
        let t = f64::from(host.frame_time());
        let (wx, wy) = self.ship.world;
        let mut i = 0;
        while i < self.deviants.len() {
            let d = &mut self.deviants[i];
            match d.state {
                0 => {
                    if ftol_d(f64::from(wx) - f64::from(d.pos.0)).abs() < 900 {
                        let f = t * f64::from(d.speed) + f64::from(d.frame);
                        d.frame = f as f32;
                        if f > f64::from(d.frames) {
                            d.frame = 0.0;
                        }
                        let tm = t + f64::from(d.timer);
                        d.timer = tm as f32;
                        if tm > f64::from(d.interval) {
                            d.timer = 0.0;
                            if host.rand() % 100 < d.proba {
                                let d = *d;
                                let rx = host.rand() % 100 - 50;
                                let ry = host.rand() % 80 - 80;
                                if let Some(s) = deviant_shot(&d, (wx, wy), rx, ry) {
                                    self.shots.push(s);
                                }
                            }
                        }
                    }
                }
                1 => {
                    let f = t * f64::from(d.speed) + f64::from(d.frame);
                    d.frame = f as f32;
                    if f > f64::from(d.frames) {
                        d.state = 2;
                        if !unlink(&mut self.deviants, &mut i) {
                            break;
                        }
                        continue;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// FUN_0048aa00: the energy items within 900 px animate; within 100
    /// px one the ship touches gives +20 energy (at most 100) and is gone.
    pub fn energies(&mut self, host: &mut dyn Host, sh: &Shapes) {
        let t = f64::from(host.frame_time());
        let (wx, wy) = self.ship.world;
        let mut i = 0;
        while i < self.energies.len() {
            let e = &mut self.energies[i];
            let d = ftol_d(f64::from(wx) - f64::from(e.pos.0)).abs();
            if d < 900 {
                let f = t * f64::from(e.speed) + f64::from(e.frame);
                e.frame = f as f32;
                if f > f64::from(e.frames) {
                    e.frame = 0.0;
                }
            }
            if d < 100
                && collision(
                    sh.ship.as_ref(),
                    0,
                    Some(ftol(wx)),
                    ftol(wy),
                    ftol(e.pos.0),
                    ftol(e.pos.1),
                )
            {
                self.gain();
                if !unlink(&mut self.energies, &mut i) {
                    break;
                }
                continue;
            }
            i += 1;
        }
    }

    fn gain(&mut self) {
        let e = f64::from(self.energy) + 20.0;
        self.energy = e as f32;
        if e > 100.0 {
            self.energy = 100.0;
        }
    }

    /// FUN_0048ab20: the power-ups, as the energy items; touched: `Life`
    /// +20 energy, `Bombe` one bomb more, `MegaBombe` kills the alive
    /// deviants within 800 px (400 px when the ship's world x is at least
    /// half the screen's width).
    pub fn power_ups(&mut self, host: &mut dyn Host, sh: &Shapes) {
        let t = f64::from(host.frame_time());
        let (wx, wy) = self.ship.world;
        let half = host.display_size().0 / 2;
        let mut i = 0;
        while i < self.power_ups.len() {
            let p = &mut self.power_ups[i];
            let d = ftol_d(f64::from(wx) - f64::from(p.pos.0)).abs();
            if d < 900 {
                let f = t * f64::from(p.speed) + f64::from(p.frame);
                p.frame = f as f32;
                if f > f64::from(p.frames) {
                    p.frame = 0.0;
                }
            }
            if d < 100
                && collision(
                    sh.ship.as_ref(),
                    0,
                    Some(ftol(wx)),
                    ftol(wy),
                    ftol(p.pos.0),
                    ftol(p.pos.1),
                )
            {
                match p.kind {
                    0 => self.gain(),
                    1 => self.bombs += 1,
                    2 => {
                        let reach = if !(half as f32 > wx) { 400 } else { 800 };
                        for dv in self.deviants.iter_mut().filter(|d| d.state == 0) {
                            if ftol_d(f64::from(wx) - f64::from(dv.pos.0)).abs() < reach {
                                dv.state = 1;
                                dv.frame = 0.0;
                            }
                        }
                    }
                    _ => {}
                }
                if !unlink(&mut self.power_ups, &mut i) {
                    break;
                }
                continue;
            }
            i += 1;
        }
    }

    /// FUN_00489f70: each active god with energy within 700 px of the
    /// ship moves one half Euler step of the Lorenz system (σ 10, ρ 28, β
    /// 8/3) and sits at its point × 5 around its centre; every
    /// `IntervalleTir` seconds, with `Probabilite` %, it shoots leftward at
    /// 250..299 px/s (ENNA with a vertical spread of −35..34, ZANAT three
    /// shots: straight, and 10 px/s slower at ±35 px/s).
    pub fn gods(&mut self, host: &mut dyn Host) {
        let t = f64::from(host.frame_time());
        let wx = f64::from(self.ship.world.0);
        for k in 0..3 {
            let g = &mut self.gods[k];
            if !(ftol_d(wx - f64::from(g.pos.0)).abs() < 700 && g.active && g.energy > 0.0) {
                continue;
            }
            let [a, b, c] = g.lorenz.map(f64::from);
            let mut d = [
                (b * 10.0 - a * 10.0) as f32,
                ((28.0 - c) * a - b) as f32,
                (a * b - c * f64::from(8.0f32 / 3.0)) as f32,
            ];
            d[0] = (t * f64::from(d[0])) as f32;
            d[1] = (t * f64::from(d[1])) as f32;
            let dc = t * f64::from(d[2]);
            d[0] = (f64::from(d[0]) * 0.5) as f32;
            d[1] = (f64::from(d[1]) * 0.5) as f32;
            d[2] = (dc * 0.5) as f32;
            g.step = d;
            for j in 0..3 {
                g.lorenz[j] = (f64::from(d[j]) + f64::from(g.lorenz[j])) as f32;
            }
            let (cx, cy) = GOD_CENTRES[k];
            g.pos = (
                (f64::from(g.lorenz[0]) * 5.0 + f64::from(cx)) as f32,
                (f64::from(g.lorenz[1]) * 5.0 + f64::from(cy)) as f32,
            );
            let tm = t + f64::from(g.timer);
            g.timer = tm as f32;
            if !(tm > f64::from(g.interval)) {
                continue;
            }
            g.timer = 0.0;
            if host.rand() % 100 >= g.proba {
                continue;
            }
            let pos = g.pos;
            let shot = |vel: (f32, f32), kind: i32, frames: i32| Shot {
                pos,
                start: vel,
                vel,
                kind,
                anim: 0.0,
                frames,
                speed: 14,
            };
            match k {
                0 => {
                    let vx = (-250 - host.rand() % 50) as f32;
                    host.sfx_play("cata_tir.wav");
                    self.shots.push(shot((vx, 0.0), 3, 7));
                }
                1 => {
                    let vy = (host.rand() % 70 - 35) as f32;
                    let vx = (-250 - host.rand() % 50) as f32;
                    host.sfx_play("cata_tir.wav");
                    self.shots.push(shot((vx, vy), 4, 10));
                }
                _ => {
                    let vx = (-250 - host.rand() % 50) as f32;
                    host.sfx_play("cata_tir.wav");
                    let vx2 = (f64::from(vx) - 10.0) as f32;
                    self.shots.push(shot((vx, 0.0), 5, 7));
                    self.shots.push(shot((vx2, 35.0), 5, 7));
                    self.shots.push(shot((vx2, -35.0), 5, 7));
                }
            }
        }
    }

    /// The end of FUN_00487460 (@ 0x00487ed8..0x00488127): the scroll
    /// advances; from 2000 (KAMA untouched) it accelerates, at 2600 it
    /// stops and KAMA appears; KAMA destroyed before 3000, it goes on at
    /// 40 px/s; the same from 5200 / at 5590 for ENNA, then ZANAT appears
    /// when ENNA is destroyed; stopped with ZANAT destroyed: speed 40 and
    /// won (true).
    pub fn scroll(&mut self, t: f32) -> bool {
        let t = f64::from(t);
        let s = t * f64::from(self.speed) + f64::from(self.scroll);
        self.scroll = s as f32;
        let i = ftol_d(s);
        self.page = i / 800;
        self.column = i % 800;
        let [g1, g2, g3] = &mut self.gods;
        if !(s < 2000.0) && !self.accel && !g1.active && g1.energy == 100.0 {
            self.accel = true;
        }
        let stop1 = !(s < 2600.0) && g1.energy == 100.0 && !g1.active;
        if stop1 {
            self.speed = 0.0;
            self.accel = false;
            g1.active = true;
        }
        if (stop1 || self.speed == 0.0) && g1.energy == 0.0 && s < 3000.0 {
            self.speed = 40.0;
            g1.active = false;
        }
        if !(s < 5200.0)
            && !self.accel
            && !g2.active
            && !g3.active
            && g2.energy == 100.0
            && g3.energy == 100.0
        {
            self.accel = true;
        }
        let stop2 = !(s < 5590.0) && g2.energy == 100.0 && !g2.active;
        if stop2 {
            self.speed = 0.0;
            self.accel = false;
            g2.active = true;
        }
        if (stop2 || self.speed == 0.0) && g2.energy == 0.0 && g3.energy == 100.0 && !g3.active {
            g2.active = false;
            g3.active = true;
        }
        if self.accel {
            self.speed = (t * 100.0 + f64::from(self.speed)) as f32;
        }
        if self.speed == 0.0 && g3.energy == 0.0 {
            self.speed = 40.0;
            return true;
        }
        false
    }
}

/// A node unlinked while its list is walked (the shots FUN_00488b60, the
/// deviants FUN_00489bf0, the items FUN_0048aa00 / FUN_0048ab20, the
/// explosions in FUN_00488140): the walk's pointer becomes the next node,
/// then the loop's step takes *its* next (e.g. @ 0x00489b32 then
/// @ 0x00489b3d), so the node after a removed one is skipped this frame;
/// when the removed node was the last, the step goes from the sentinel to
/// the first node and the walk starts over. `i` becomes the next index;
/// false when the walk ends.
fn unlink<T>(v: &mut Vec<T>, i: &mut usize) -> bool {
    v.remove(*i);
    let n = v.len();
    if *i < n {
        *i += 1;
        *i < n
    } else if n == 0 {
        false
    } else {
        *i = 0;
        true
    }
}

/// A deviant's shot at the ship (@ 0x00489c87..0x00489efc), `None` when
/// the ship is on its right (that branch stores nothing).
pub fn deviant_shot(d: &Deviant, ship: (f32, f32), rx: i32, ry: i32) -> Option<Shot> {
    let dx = f64::from(ship.0) - f64::from(d.pos.0);
    let dy = ((f64::from(ship.1) - f64::from(d.pos.1)) - 40.0) as f32;
    if dx > 0.0 {
        return None;
    }
    let pos = (d.pos.0, (f64::from(d.pos.1) - 30.0) as f32);
    let mut vx64 = dx + f64::from(rx) + 50.0;
    if !(vx64 <= 0.0) {
        vx64 = 0.0;
    }
    let vx = vx64 as f32;
    let vy = (f64::from(ry) + f64::from(dy)) as f32;
    let len = (f64::from(vy) * f64::from(vy) + vx64 * vx64).sqrt();
    let vel = if len > 600.0 {
        let (x, y) = (f64::from(vx), f64::from(vy));
        let inv = 1.0 / (x * x + y * y).sqrt();
        let ny = (y * inv) as f32;
        let nx = (inv * x) as f32;
        (
            (f64::from(nx) * 600.0) as f32,
            (f64::from(ny) * 600.0) as f32,
        )
    } else {
        (vx, vy)
    };
    Some(Shot {
        pos,
        start: vel,
        vel,
        kind: 1,
        anim: 0.0,
        frames: 1,
        speed: 15,
    })
}

impl Defender {
    /// FUN_004861d0: the loaded byte (0x00717539); the ship at (50, h /
    /// 2); energy 100; 999 bombs; the fourteen banks; the four ground
    /// pictures; `srand(time(0))`; the shots cleared; the level
    /// (FUN_0048d350); scroll 0; 3 lives; `DEFENDER_GAGNE` /
    /// `DEFENDER_PERDU` := 0; the sounds; then (after the start, which the
    /// engine runs) the pointer at (100, h / 2 − 50) with the stretch
    /// surface's height.
    pub fn init(host: &mut dyn Host) -> Defender {
        let h = host.display_size().1;
        let banks = Banks {
            ship: host.bank("VaisseauChop.spr"),
            gauge: host.bank("jauge_chop.spr"),
            ground: host.bank("fond_chop.spr"),
            tir_dev: host.bank("tirdev_chop.spr"),
            exp: host.bank("ExpChop.spr"),
            deviant: host.bank("DeviantChop.spr"),
            item: host.bank("ItemChop.spr"),
            tir: host.bank("Tir_Chop01.spr"),
            bomb: host.bank("BombeChop.spr"),
            gods: host.bank("dieux_chop.spr"),
            tir_kama: host.bank("tirkama_chop.spr"),
            tir_enna: host.bank("tirenna_chop.spr"),
            tir_zanat: host.bank("tirzanat_chop.spr"),
            life: host.bank("life_chop.spr"),
        };
        let x888 = matches!(host.surface(), Surface::Xrgb8888(_));
        let pages = (1..=4)
            .map(|k| {
                let d = host.read(&format!("map/Chop_0{k}.tga"))?;
                if x888 {
                    atlantis2_data::tga::Tga32::parse(&d)
                        .ok()
                        .map(|t| Surface::Xrgb8888(t.pixels))
                } else {
                    atlantis2_data::tga::Tga::parse(&d)
                        .ok()
                        .map(|t| Surface::Rgb565(t.pixels))
                }
            })
            .collect();
        host.srand_time();
        let level = host
            .read(LEVEL_FILE)
            .map(|d| Level::parse(&d))
            .unwrap_or_default();
        let st = State::new(&level, h);
        host.set_var("DEFENDER_GAGNE", 0);
        host.set_var("DEFENDER_PERDU", 0);
        for s in SFX {
            host.sfx_load(s);
        }
        // @ 0x00487079 (after FUN_004870b0 opened the stretch surface).
        host.set_pointer(100, HEIGHT_AFTER_START / 2 - 50);
        Defender {
            st,
            level,
            banks,
            pages,
        }
    }

    /// A game on `level` without banks or pictures (tests: nothing is
    /// touched, nothing drawn), its state set up as [`State::new`] with a
    /// display `h` high.
    pub fn without_data(level: Level, h: i32) -> Defender {
        Defender {
            st: State::new(&level, h),
            level,
            banks: Banks::default(),
            pages: Vec::new(),
        }
    }

    fn shapes(&self) -> Shapes {
        Shapes {
            ship: self.banks.ship.clone(),
            ground: self.banks.ground.clone(),
            deviant: self.banks.deviant.clone(),
            bomb: self.banks.bomb.clone(),
            exp: self.banks.exp.clone(),
            gods: self.banks.gods.clone(),
        }
    }

    /// FUN_00487460 after the prologue and the end variables' test.
    pub fn logic(&mut self, host: &mut dyn Host, sh: &Shapes) {
        let st = &mut self.st;
        st.input(host);
        if st.ground_hit(sh.ground.as_ref()) {
            st.ship.screen = st.ship.world;
            let x = f64::from(st.ship.world.0) - f64::from(ftol(st.scroll) as f32);
            st.ship.screen.0 = x as f32;
            if x < -10.0 || !(st.energy > 0.0) {
                st.lives -= 1;
                if st.lives <= 0 {
                    host.set_var("DEFENDER_PERDU", 1);
                    return;
                }
                let h = host.display_size().1;
                let p = st.restart(&self.level, h);
                host.set_pointer(p.0, p.1);
            }
        }
        let st = &mut self.st;
        host.set_pointer(ftol(st.ship.screen.0), ftol(st.ship.screen.1));
        st.shots(host, sh);
        st.deviants(host);
        st.energies(host, sh);
        st.power_ups(host, sh);
        st.gods(host);
        self.draw(host);
        let st = &mut self.st;
        st.ship.prev_world = st.ship.world;
        st.ship.prev_screen = st.ship.screen;
        if st.scroll(host.frame_time()) {
            host.set_var("DEFENDER_GAGNE", 1);
        }
    }

    /// FUN_00488140: the ground (FUN_004883b0), the shots (FUN_0048acd0,
    /// which also advances the animations of the ship's and the gods'
    /// shots), the deviants (FUN_0048b100), the energy items
    /// (FUN_0048b3d0), the power-ups (FUN_0048b480), the gods
    /// (FUN_0048b200), the ship at the pointer, the explosions (18
    /// frames/s, gone after frame 8), the gauge (`jauge_chop` frame
    /// 14 − energy × 0.14 at (20, 20)), the lives (`life_chop` #0 at (50 +
    /// 40 i, 20)), the flip. No pointer is drawn.
    pub fn draw(&mut self, host: &mut dyn Host) {
        let t = f64::from(host.frame_time());
        self.draw_ground(host);
        let b = self.banks.clone();
        let st = &mut self.st;
        let scroll = f64::from(st.scroll);
        let screen = |p: (f32, f32)| (ftol((f64::from(p.0) - scroll) as f32), ftol(p.1));
        for s in st.shots.iter_mut() {
            let (x, y) = screen(s.pos);
            let animate = |s: &mut Shot| {
                s.anim = (t * f64::from(s.speed) + f64::from(s.anim)) as f32;
                let last = (s.frames - 1) as f32;
                if !(last > s.anim) {
                    s.anim = last;
                }
                ftol(s.anim) as usize
            };
            match s.kind {
                0 => {
                    let f = animate(s);
                    aff(host, "Tir_Chop01", b.tir.as_ref(), f, Some(x), y, 1.0);
                }
                1 => {
                    let vy = s.vel.1;
                    let f = if vy < 0.0 {
                        ((f64::from(vy) - f64::from(s.start.1)) / f64::from(s.start.1) * -5.0)
                            as f32
                    } else if vy < 50.0 {
                        4.0
                    } else if vy < 100.0 {
                        5.0
                    } else if vy < 200.0 {
                        6.0
                    } else if !(vy < 200.0) {
                        7.0
                    } else {
                        0.0
                    };
                    aff(
                        host,
                        "tirdev_chop",
                        b.tir_dev.as_ref(),
                        ftol(f) as usize,
                        Some(x),
                        y,
                        1.0,
                    );
                }
                2 => {
                    if s.anim.to_bits() == (-1.0f32).to_bits() {
                        aff(host, "BombeChop", b.bomb.as_ref(), 0, Some(x), y, 1.0);
                    } else {
                        let f = ftol(s.anim) as usize;
                        aff(host, "ExpChop", b.exp.as_ref(), f, Some(x), y, 1.0);
                    }
                }
                3 => {
                    let f = animate(s);
                    aff(
                        host,
                        "tirkama_chop",
                        b.tir_kama.as_ref(),
                        f,
                        Some(x),
                        y,
                        1.0,
                    );
                }
                4 => {
                    let f = animate(s);
                    aff(
                        host,
                        "tirenna_chop",
                        b.tir_enna.as_ref(),
                        f,
                        Some(x),
                        y,
                        1.0,
                    );
                }
                5 => {
                    let f = animate(s);
                    aff(
                        host,
                        "tirzanat_chop",
                        b.tir_zanat.as_ref(),
                        f,
                        Some(x),
                        y,
                        1.0,
                    );
                }
                _ => {}
            }
        }
        let wx = f64::from(st.ship.world.0);
        let near = |x: f32| ftol_d(wx - f64::from(x)).abs() < 800;
        for d in &st.deviants {
            if near(d.pos.0) && d.state <= 1 {
                let (x, y) = screen(d.pos);
                aff(
                    host,
                    "DeviantChop",
                    b.deviant.as_ref(),
                    d.state as usize,
                    Some(x),
                    y,
                    1.0,
                );
            }
        }
        for e in &st.energies {
            if near(e.pos.0) {
                let (x, y) = screen(e.pos);
                aff(host, "ItemChop", b.item.as_ref(), 0, Some(x), y, 1.0);
            }
        }
        for p in &st.power_ups {
            if near(p.pos.0) && (0..=2).contains(&p.kind) {
                let (x, y) = screen(p.pos);
                aff(
                    host,
                    "ItemChop",
                    b.item.as_ref(),
                    p.kind as usize + 1,
                    Some(x),
                    y,
                    1.0,
                );
            }
        }
        for (k, g) in st.gods.iter().enumerate() {
            if near(g.pos.0) && g.active && g.energy > 0.0 {
                let (x, y) = screen(g.pos);
                aff(host, "dieux_chop", b.gods.as_ref(), k, Some(x), y, 1.0);
            }
        }
        let (px, py) = host.pointer();
        aff(host, "VaisseauChop", b.ship.as_ref(), 0, Some(px), py, 1.0);
        let mut i = 0;
        while i < st.explosions.len() {
            let e = &mut st.explosions[i];
            e.frame = (t * 18.0 + f64::from(e.frame)) as f32;
            let (x, y) = screen(e.pos);
            if e.frame < 8.0 {
                let f = ftol(e.frame) as usize;
                aff(host, "ExpChop", b.exp.as_ref(), f, Some(x), y, 1.0);
                i += 1;
            } else {
                e.frame = 8.0;
                aff(host, "ExpChop", b.exp.as_ref(), 8, Some(x), y, 1.0);
                if !unlink(&mut st.explosions, &mut i) {
                    break;
                }
            }
        }
        let gauge = 14.0 - f64::from(st.energy) * f64::from(0.01f32) * 14.0;
        aff(
            host,
            "jauge_chop",
            b.gauge.as_ref(),
            ftol_d(gauge) as usize,
            Some(20),
            20,
            1.0,
        );
        for i in 0..st.lives.max(0) {
            aff(
                host,
                "life_chop",
                b.life.as_ref(),
                0,
                Some(50 + 40 * i),
                20,
                1.0,
            );
        }
        host.flip();
    }

    /// FUN_004883b0 (the byte 0x00717538 is never written: always its
    /// first branch): page `page & 3`'s columns from `column` on, then
    /// page `page + 1`'s first `column` columns, 600 rows. The terrain's
    /// sprites (`fond_chop`) are drawn only when 0x006e0518 is set, which
    /// only the crash log's path writes (0): never. A negative page (a
    /// scroll pushed below 0 with O) indexes before the pictures' array
    /// in the original [undefined]; the port wraps it.
    fn draw_ground(&mut self, host: &mut dyn Host) {
        let page = self.st.page.rem_euclid(4) as usize;
        let next = (self.st.page + 1).rem_euclid(4) as usize;
        let col = self.st.column.rem_euclid(800) as usize;
        let (w, h) = (super::WIDTH, super::HEIGHT);
        let pages = &self.pages;
        let dst = host.surface();
        fn copy<T: Copy>(d: &mut [T], a: &[T], b: Option<&[T]>, col: usize, w: usize, h: usize) {
            for y in 0..h {
                let row = &mut d[y * w..(y + 1) * w];
                if a.len() >= (y + 1) * w {
                    row[..w - col].copy_from_slice(&a[y * w + col..(y + 1) * w]);
                }
                if let Some(b) = b {
                    if col > 0 && b.len() >= (y + 1) * w {
                        row[w - col..].copy_from_slice(&b[y * w..y * w + col]);
                    }
                }
            }
        }
        match (dst, pages.get(page).and_then(|p| p.as_ref())) {
            (Surface::Rgb565(d), Some(Surface::Rgb565(a))) => {
                let b = match pages.get(next).and_then(|p| p.as_ref()) {
                    Some(Surface::Rgb565(b)) => Some(b.as_slice()),
                    _ => None,
                };
                copy(d, a, b, col, w, h);
            }
            (Surface::Xrgb8888(d), Some(Surface::Xrgb8888(a))) => {
                let b = match pages.get(next).and_then(|p| p.as_ref()) {
                    Some(Surface::Xrgb8888(b)) => Some(b.as_slice()),
                    _ => None,
                };
                copy(d, a, b, col, w, h);
            }
            _ => {}
        }
    }
}

/// The stretch surface's height (FUN_00411a50 once FUN_004870b0 opened
/// it).
const HEIGHT_AFTER_START: i32 = super::HEIGHT as i32;

impl Puzzle for Defender {
    fn id(&self) -> PuzzleId {
        PuzzleId::Defender
    }

    /// FUN_00487460: the loaded byte; the prologue; `DEFENDER_GAGNE` or
    /// `DEFENDER_PERDU` set → nothing more (the 0x00719d98 branches that
    /// would set 2 and free the game are dead); the logic and the draw.
    fn frame(&mut self, host: &mut dyn Host) {
        if !host.script_prologue() {
            return;
        }
        if host.var("DEFENDER_GAGNE") != 0 || host.var("DEFENDER_PERDU") != 0 {
            return;
        }
        let sh = self.shapes();
        self.logic(host, &sh);
    }

    /// FUN_00487100: the fourteen banks and four pictures freed, the three
    /// sounds removed (the stretch surface and mode 3 are the engine's).
    fn free(&mut self, host: &mut dyn Host) {
        self.banks = Banks::default();
        self.pages.clear();
        for s in SFX {
            host.sfx_remove(s);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INI: &str = "''''\r\n' comment\r\n[Deviant]\r\nPos\t\t681\t520\r\nFrame\t\t0\r\nFrames\t\t1\r\nVitesseAnim\t15\r\nIntervalleTir\t2\r\nTempsDernierTir\t1\r\nProbabilite\t70\r\n[End]\r\n\r\n[Energie]\r\nPos\t\t852\t179\r\n[End]\r\n[PowerUp]\r\nType\tMegaBombe\r\nPos 10 20\r\n[End]\r\n[Dieu] ENNA\r\nPos\t\t6053\t237\r\nIntervalleTir\t500\r\nProbabilite\t100\r\n[End]\r\n";

    #[test]
    fn level_parse() {
        let l = Level::parse(INI.as_bytes());
        assert_eq!(l.deviants.len(), 1);
        let d = l.deviants[0];
        assert_eq!(d.pos, (681.0, 520.0));
        assert_eq!((d.interval, d.timer, d.proba, d.frames), (2.0, 1.0, 70, 1));
        assert_eq!(l.energies[0].pos, (852.0, 179.0));
        assert_eq!(l.energies[0].frames, 0);
        assert_eq!(l.power_ups[0].kind, 2);
        assert_eq!(l.power_ups[0].pos, (10.0, 20.0));
        let g = l.gods[1].unwrap();
        assert_eq!(g.pos, Some((6053.0, 237.0)));
        assert_eq!(g.interval, Some((500.0 * f64::from(0.001f32)) as f32));
        assert!(l.gods[0].is_none() && l.gods[2].is_none());
    }

    #[test]
    fn deviant_shoots_left_only() {
        let d = Deviant {
            pos: (700.0, 520.0),
            frame: 0.0,
            frames: 1,
            speed: 15.0,
            state: 0,
            interval: 2.0,
            timer: 0.0,
            proba: 100,
        };
        assert!(deviant_shot(&d, (800.0, 300.0), 0, -40).is_none());
        let s = deviant_shot(&d, (100.0, 300.0), 0, -40).unwrap();
        assert_eq!(s.pos, (700.0, 490.0));
        assert_eq!(s.kind, 1);
        // (−550, −300): longer than 600 → scaled to 600.
        let l = (f64::from(s.vel.0).powi(2) + f64::from(s.vel.1).powi(2)).sqrt();
        assert!((l - 600.0).abs() < 0.01, "{l}");
        assert_eq!(s.start, s.vel);
        // Close: kept as it is, never rightward.
        let s = deviant_shot(&d, (690.0, 500.0), 40, -10).unwrap();
        assert_eq!(s.vel, (0.0, -70.0));
    }

    #[test]
    fn scroll_stops_for_the_gods() {
        let mut st = State::new(&Level::default(), 600);
        st.scroll = 2500.0;
        assert!(!st.scroll(0.04));
        assert!(st.accel);
        st.scroll = 2600.0;
        st.speed = 40.0;
        assert!(!st.scroll(0.04));
        assert_eq!(st.speed, 0.0);
        assert!(st.gods[0].active && !st.accel);
        // KAMA destroyed: on at 40.
        st.gods[0].energy = 0.0;
        assert!(!st.scroll(0.04));
        assert_eq!(st.speed, 40.0);
        assert!(!st.gods[0].active);
        // ENNA's stop, ENNA destroyed → ZANAT, ZANAT destroyed → won.
        st.scroll = 5600.0;
        assert!(!st.scroll(0.04));
        assert_eq!(st.speed, 0.0);
        assert!(st.gods[1].active);
        st.gods[1].energy = 0.0;
        assert!(!st.scroll(0.04));
        assert!(st.gods[2].active && !st.gods[1].active);
        st.gods[2].energy = 0.0;
        assert!(st.scroll(0.04));
        assert_eq!(st.speed, 40.0);
    }

    #[test]
    fn page_and_column() {
        let mut st = State::new(&Level::default(), 600);
        st.scroll = 1599.0;
        st.speed = 40.0;
        st.scroll(0.05);
        assert_eq!((st.page, st.column), (2, 1));
        // 0.04 (as a float) × 50 is a little under 2: the column truncates.
        st.scroll = 1599.0;
        st.speed = 50.0;
        st.accel = false;
        st.scroll(0.04);
        assert_eq!((st.page, st.column), (2, 0));
    }
}

//! The puzzles against the original `atlantis.exe` (2001 English build):
//! the dispatcher `FUN_0043a1e0`, the random generators, and whole runs of
//! the throne lock (5), the sliding puzzles (6, 7, 14), the metal pieces
//! (9..12), the hexagon (13), the crab race (15) and Muria's tablets (16),
//! replayed pass by pass: the input of each pass, the sounds started, the
//! md5 of the draws and of the state must be the original's. Then each of
//! them played in the whole engine with scripted input.
//!
//! The expected values come from the original code run under emulation
//! with scripted players (see the headers of the golden files); they hold
//! numbers and digests only. Ported from ScummVM's
//! `test/engines/cryomni3d/atlantis/` (`ui_puzzle*.h`).
//!
//! The tests that read the game (the class maps and sprite coverage of the
//! metal pieces and the hexagon, the engine runs) need `ATLANTIS_DATA`, the
//! game directory; without it they are skipped.

use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::puzzle::crab::{CrabGame, CrabIo};
use atlantis_engine::puzzle::hexa::{HexaGame, HexaIo};
use atlantis_engine::puzzle::meta::{MetaGame, MetaIo};
use atlantis_engine::puzzle::muria::{MuriaGame, MuriaIo};
use atlantis_engine::puzzle::taquin::{TaquinGame, TaquinIo};
use atlantis_engine::puzzle::throne::{ThroneGame, ThroneIo};
use atlantis_engine::puzzle::{self, read_coverage, Coverage, PuzzleCursor, MAP_SIZE};
use atlantis_engine::sys::{Input, MouseButton, Runner, Sys};
use atlantis_engine::{Event, Options, Start};
use md5::{Digest, Md5};

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// Runs a future that never waits (the replays' I/O is synchronous).
fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("a replay waited"),
    }
}

/// The golden lines without the comments.
fn golden(text: &str) -> Vec<&str> {
    text.lines().filter(|l| !l.starts_with('#')).collect()
}

/// A scenario: its header's tokens and its pass lines.
struct Scenario<'a> {
    tok: Vec<&'a str>,
    lines: &'a [&'a str],
}

impl Scenario<'_> {
    fn int(&self, i: usize) -> i32 {
        self.tok[i].parse().unwrap()
    }
}

/// The scenarios; `frames_tok` is the index of the pass count in the
/// header.
fn scenarios<'a>(lines: &'a [&'a str], frames_tok: usize) -> Vec<Scenario<'a>> {
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if !l.starts_with("scenario ") {
            continue;
        }
        let tok: Vec<&str> = l.split(' ').collect();
        let n: usize = tok[frames_tok].parse().unwrap();
        // Lines that are not passes (the stage-4 file's 'piece' lines)
        // come first.
        let first = i
            + 1
            + lines[i + 1..]
                .iter()
                .take_while(|l| !l.contains('|'))
                .count();
        out.push(Scenario {
            tok,
            lines: &lines[first..first + n],
        });
    }
    out
}

/// The pass-by-pass comparison the C++ tests make: the draws and sounds of
/// a pass collected, then compared with the pass's line at the call where
/// the original's state was dumped.
struct Replay<'a> {
    lines: &'a [&'a str],
    next: usize,
    mismatches: u32,
    first_mismatch: String,
    draws: String,
    sounds: String,
    frames: usize,
    /// The input when the lines are used up.
    idle: (i32, i32),
    /// The C++ harness stops a runaway scenario after this many passes,
    /// setting the result to 2.
    limit: usize,
}

impl<'a> Replay<'a> {
    fn new(lines: &'a [&'a str], idle: (i32, i32)) -> Replay<'a> {
        Replay {
            lines,
            next: 0,
            mismatches: 0,
            first_mismatch: String::new(),
            draws: String::new(),
            sounds: String::new(),
            frames: 0,
            idle,
            limit: usize::MAX,
        }
    }

    /// The input of the next pass: x, y, click, right button.
    fn input(&self) -> (i32, i32, bool, bool) {
        let Some(l) = self.lines.get(self.next) else {
            return (self.idle.0, self.idle.1, false, false);
        };
        let tok: Vec<&str> = l[..l.find('|').unwrap()].split_whitespace().collect();
        (
            tok[1].parse().unwrap(),
            tok[2].parse().unwrap(),
            tok[3] == "1",
            tok.get(4) == Some(&"1"),
        )
    }

    fn add(&mut self, s: &str) {
        if !self.draws.is_empty() {
            self.draws.push('\n');
        }
        self.draws.push_str(s);
    }

    fn sound(&mut self, s: &str) {
        if !self.sounds.is_empty() {
            self.sounds.push(' ');
        }
        self.sounds.push_str(s);
    }

    fn sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.add(&format!("{bank} {x} {y} {frame}"));
    }

    /// The end of a pass: the draws, sounds and state against the line.
    fn close(&mut self, state: &[u8]) {
        let Some(l) = self.lines.get(self.next) else {
            self.mismatches += 1;
            return;
        };
        self.next += 1;
        self.frames += 1;
        let parts: Vec<&str> = l.split('|').map(str::trim).collect();
        let (want_snd, want_draws, want_state) = (parts[1], parts[2], parts[3]);
        let got_draws = md5_hex(self.draws.as_bytes());
        let got_state = md5_hex(state);
        if got_draws != want_draws || got_state != want_state || self.sounds != want_snd {
            if self.mismatches == 0 {
                self.first_mismatch = format!(
                    "line '{l}': sounds '{}' draws {} state {}",
                    self.sounds,
                    if got_draws == want_draws {
                        "ok"
                    } else {
                        "differ"
                    },
                    if got_state == want_state {
                        "ok"
                    } else {
                        "differ"
                    }
                );
            }
            self.mismatches += 1;
        }
        self.draws.clear();
        self.sounds.clear();
    }

    /// The replay stops at the first mismatch, or when it runs past its
    /// lines.
    fn aborted(&self) -> bool {
        self.mismatches != 0 || self.frames > self.lines.len() + 2 || self.frames >= self.limit
    }

    /// The closing line (the sounds after the last pass, the final
    /// state), then the outcome.
    fn check(&mut self, s: &Scenario, state: &[u8], got: i32, want: i32) {
        self.close(state);
        let name = s.tok[1];
        assert_eq!(self.first_mismatch, "", "{name}");
        assert_eq!(self.mismatches, 0, "{name}");
        assert_eq!(self.frames, s.lines.len(), "{name}: passes");
        assert_eq!(got, want, "{name}: result");
    }
}

/// The state bytes, as the C++ tests lay them out.
#[derive(Default)]
struct State(Vec<u8>);

impl State {
    fn i16(&mut self, v: i16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn cursor(&mut self, c: &PuzzleCursor) -> &mut Self {
        self.i16(c.mode).i16(c.frame).i16(c.step)
    }
    fn md5(&mut self, map: &[u8]) -> &mut Self {
        self.0.extend_from_slice(&Md5::digest(map));
        self
    }
}

// The dispatcher and the generators.

#[test]
fn dispatch_vs_original() {
    let text = include_str!("golden/ui_puzzle_dispatch.txt");
    let want: Vec<&str> = golden(text)
        .into_iter()
        .filter(|l| l.starts_with("held "))
        .collect();
    let mut out = Vec::new();
    for h in [0, 7] {
        for n in 0..18 {
            let mut held = h;
            let r = puzzle::check(n, &mut held);
            let entry = match r {
                None => format!("FUN_{:08x}", puzzle::entry(n).unwrap()),
                Some(_) => "none".to_string(),
            };
            out.push(format!(
                "held {h} puzzle {n}: entry {entry} result {} objetenmain {held}",
                r.unwrap_or(0x55)
            ));
        }
    }
    assert_eq!(out, want);
}

#[test]
fn crt_rand_from_seed_1() {
    let want = [
        16838, 5758, 10113, 17515, 31051, 5627, 23010, 7419, 16212, 4086, 2749, 12767,
    ];
    let mut seed = 1;
    for w in want {
        assert_eq!(puzzle::crt_rand_next(&mut seed), w);
    }
}

#[test]
fn puzzle_lcg() {
    // FUN_00447524: masks 255, 7 and 3; the seed arithmetic is 16-bit.
    let mut g = CrabGame::default();
    g.lcg_seed = 0x1234;
    let a = g.lcg(255);
    assert!((0..=255).contains(&a));
    assert_eq!(g.lcg_seed, (0x1234i32 * -0x342f + 1) as u16);
    for _ in 0..100 {
        assert!((0..=6).contains(&g.lcg(6)));
        assert!((0..=3).contains(&g.lcg(3)));
    }
}

// The throne lock (5).

struct ThroneReplay<'a>(Replay<'a>);

fn throne_state(g: &ThroneGame) -> Vec<u8> {
    let mut s = State::default();
    for p in &g.parts {
        s.i16(p.base)
            .i16(p.count)
            .i16(p.init)
            .i16(p.cur)
            .i16(p.x)
            .i16(p.y);
    }
    s.cursor(&g.cursor)
        .i32(g.counter)
        .i32(g.active)
        .i32(g.result);
    s.0
}

impl ThroneIo for ThroneReplay<'_> {
    fn read_input(&mut self) -> (i32, i32, bool) {
        let (x, y, c, _) = self.0.input();
        (x, y, c)
    }
    fn draw_background(&mut self) {
        self.0.add("bg");
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.0.sprite(bank, x, y, frame);
    }
    async fn present(&mut self, g: &ThroneGame) {
        self.0.add("present");
        self.0.close(&throne_state(g));
    }
    fn play_sound(&mut self, slot: i32) {
        self.0.sound(&slot.to_string());
    }
    async fn wait_ticks(&mut self, _n: u32) {}
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

#[test]
fn throne_runs_vs_original() {
    let lines = golden(include_str!("golden/ui_puzzle5_throne.txt"));
    let sc = scenarios(&lines, 3);
    for s in &sc {
        let mut g = ThroneGame::default();
        let mut io = ThroneReplay(Replay::new(s.lines, (0, 0)));
        io.0.limit = 3000;
        let got = block_on(g.run(&mut io));
        if io.0.frames >= io.0.limit {
            g.result = 2;
        }
        io.0.check(s, &throne_state(&g), got, s.int(5));
    }
    assert_eq!(sc.len(), 3);
}

// The sliding puzzles (6, 7, 14).

struct TaquinReplay<'a>(Replay<'a>);

fn taquin_state(g: &TaquinGame) -> Vec<u8> {
    let mut s = State::default();
    for &b in &g.board {
        s.i16(b);
    }
    s.i16(g.moving_cell).i16(g.move_dx).i16(g.move_dy);
    s.cursor(&g.cursor).i32(g.object_in_hand).i32(g.result);
    s.0
}

impl TaquinIo for TaquinReplay<'_> {
    fn start_input(&mut self) -> (i32, i32, bool) {
        let (x, y, c, _) = self.0.input();
        (x, y, c)
    }
    fn boundary(&mut self, g: &TaquinGame) -> (i32, i32, bool) {
        self.0.close(&taquin_state(g));
        self.start_input()
    }
    fn draw_background(&mut self) {
        self.0.add("bg");
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.0.sprite(bank, x, y, frame);
    }
    fn draw_standard_cursor(&mut self, _x: i32, _y: i32) {
        self.0.add("stdcursor");
    }
    async fn present(&mut self, _g: &TaquinGame) {
        self.0.add("present");
    }
    fn play_sound(&mut self, slot: i32) {
        self.0.sound(&slot.to_string());
    }
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

#[test]
fn taquin_runs_vs_original() {
    let lines = golden(include_str!("golden/ui_puzzle6_taquin.txt"));
    let sc = scenarios(&lines, 7);
    for s in &sc {
        let mut g = TaquinGame::new(s.int(3));
        g.object_in_hand = s.int(5);
        let mut io = TaquinReplay(Replay::new(s.lines, (0, 0)));
        io.0.limit = 1500;
        let got = block_on(g.run(&mut io));
        if io.0.frames >= io.0.limit {
            g.result = 2;
        }
        io.0.check(s, &taquin_state(&g), got, s.int(9));
    }
    assert_eq!(sc.len(), 9);
}

// The metal pieces (9..12) and the hexagon (13).

/// The class map and the sprite file of a puzzle.
fn map_and_sprites(g: &Game, map: &str, spr: &str) -> (Vec<u8>, Vec<u8>) {
    let m = g.big.read(&format!("PUZZLES\\{map}")).expect("class map");
    let s = g.big.read(&format!("PUZZLES\\{spr}")).expect("sprites");
    (m[0x436..0x436 + MAP_SIZE].to_vec(), s)
}

struct MetaReplay<'a> {
    r: Replay<'a>,
    place: i32,
    first_picture: i32,
}

fn meta_state(g: &MetaGame) -> Vec<u8> {
    let mut s = State::default();
    for p in &g.pieces {
        s.i16(p.base)
            .i16(p.unk2)
            .i16(p.cls)
            .i16(p.orient)
            .i16(p.x)
            .i16(p.y)
            .i16(p.unk_c);
    }
    s.i16(g.count).cursor(&g.cursor);
    s.i32(g.grab_x)
        .i32(g.grab_y)
        .i32(g.repeat)
        .i32(g.unk_f4)
        .i32(g.held);
    s.i16(g.picture)
        .i32(g.object_in_hand)
        .i32(g.brought_flag)
        .i32(g.result);
    s.md5(&g.map);
    s.0
}

impl MetaIo for MetaReplay<'_> {
    fn start_input(&mut self) -> (i32, i32, bool, bool) {
        self.r.input()
    }
    fn boundary(&mut self, g: &MetaGame) -> (i32, i32, bool, bool) {
        self.r.close(&meta_state(g));
        self.r.input()
    }
    fn load_picture(&mut self, n: i32) {
        if self.first_picture == 0 {
            self.first_picture = n;
        }
    }
    fn draw_background(&mut self) {
        self.r.add("bg");
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.r.sprite(bank, x, y, frame);
    }
    fn draw_standard_cursor(&mut self, _x: i32, _y: i32) {
        self.r.add("stdcursor");
    }
    async fn present(&mut self, _g: &MetaGame) {
        self.r.add("present");
    }
    fn play_sound(&mut self, slot: i32) {
        self.r.sound(&slot.to_string());
    }
    fn current_place(&mut self) -> i32 {
        self.place
    }
    fn aborted(&self) -> bool {
        self.r.aborted()
    }
}

#[test]
fn meta_runs_vs_original() {
    let Some(data) = game("meta_runs_vs_original") else {
        return;
    };
    let (map, spr) = map_and_sprites(&data, "PZMETAM.BMP", "PZMETAP1.SPR");
    let cov = read_coverage(&spr).unwrap();
    let lines = golden(include_str!("golden/ui_puzzle9_meta.txt"));
    let sc = scenarios(&lines, 7);
    // win_stage1..3 and wander_stage4 share one memory; the others start
    // fresh.
    let mut g: Option<MetaGame> = None;
    for s in &sc {
        let name = s.tok[1];
        if g.is_none() || !(name.starts_with("win_stage") || name == "wander_stage4") {
            g = Some(MetaGame::new(map.clone(), cov.clone()));
        }
        let g = g.as_mut().unwrap();
        g.object_in_hand = s.int(5);
        assert_eq!((s.tok[10], s.tok[12]), ("place", "picture"));
        let mut io = MetaReplay {
            r: Replay::new(s.lines, (320, 470)),
            place: s.int(11),
            first_picture: 0,
        };
        let got = block_on(g.run(&mut io, s.int(3)));
        io.r.check(s, &meta_state(g), got, s.int(9));
        assert_eq!(io.first_picture, s.int(13), "{name}: picture");
    }
    assert_eq!(sc.len(), 10);
}

/// Stage 4 won from a board laid out by hand, without a board turn.
#[test]
fn meta_stage4_vs_original() {
    let Some(data) = game("meta_stage4_vs_original") else {
        return;
    };
    let (map, spr) = map_and_sprites(&data, "PZMETAM.BMP", "PZMETAP1.SPR");
    let cov = read_coverage(&spr).unwrap();
    let lines = golden(include_str!("golden/ui_puzzle9_stage4.txt"));
    let sc = scenarios(&lines, 7);
    assert_eq!(sc.len(), 1);
    let s = &sc[0];
    // The stage starts with the six pieces of stage 3 and the seventh
    // brought (object 320).
    let mut g = MetaGame::new(map, cov);
    g.count = 6;
    g.object_in_hand = s.int(5);
    let mut io = MetaReplay {
        r: Replay::new(s.lines, (320, 470)),
        place: 0,
        first_picture: 0,
    };
    let got = block_on(g.run(&mut io, s.int(3)));
    io.r.check(s, &meta_state(&g), got, s.int(9));
    // The pieces at the end.
    let ends: Vec<String> = lines
        .iter()
        .filter(|l| l.starts_with("piece "))
        .map(|l| l.to_string())
        .collect();
    let got: Vec<String> = g
        .pieces
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "piece {i} class {} orientation {} x {} y {}",
                p.cls, p.orient, p.x, p.y
            )
        })
        .collect();
    assert_eq!(got, ends);
}

#[test]
fn meta_coverage_vs_original() {
    let Some(data) = game("meta_coverage_vs_original") else {
        return;
    };
    let (_, spr) = map_and_sprites(&data, "PZMETAM.BMP", "PZMETAP1.SPR");
    let cov: Vec<Coverage> = read_coverage(&spr).unwrap();
    let want: Vec<&str> = golden(include_str!("golden/ui_puzzle9_coverage.txt"))
        .into_iter()
        .filter(|l| l.starts_with("frame "))
        .collect();
    assert_eq!(want.len(), 48);
    assert_eq!(cov.len(), 48);
    for (f, (c, w)) in cov.iter().zip(&want).enumerate() {
        let mut buf = vec![0u8; MAP_SIZE];
        for (&dx, &dy) in c.dx.iter().zip(&c.dy) {
            let (x, y) = (320 + i32::from(dx), 240 + i32::from(dy));
            if (0..640).contains(&x) && (0..480).contains(&y) {
                buf[(y * 640 + x) as usize] = 1;
            }
        }
        let (mut n, mut x0, mut y0, mut x1, mut y1) = (0, 640, 480, -1, -1);
        for y in 0..480 {
            for x in 0..640 {
                if buf[(y * 640 + x) as usize] != 0 {
                    n += 1;
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        let got = if n == 0 {
            format!("frame {f} empty")
        } else {
            let mut mask = Vec::new();
            for y in y0..=y1 {
                for x in x0..=x1 {
                    mask.push(buf[(y * 640 + x) as usize]);
                }
            }
            format!(
                "frame {f} count {n} box {x0} {y0} {x1} {y1} md5 {}",
                md5_hex(&mask)
            )
        };
        assert_eq!(&got, w);
    }
}

struct HexaReplay<'a>(Replay<'a>);

fn hexa_state(g: &HexaGame) -> Vec<u8> {
    let mut s = State::default();
    for p in &g.pieces {
        s.i16(p.base)
            .i16(p.cls)
            .i16(p.orient)
            .i16(p.x)
            .i16(p.y)
            .i16(p.unk_a);
    }
    s.cursor(&g.cursor);
    s.i32(g.grab_x)
        .i32(g.grab_y)
        .i32(g.repeat)
        .i32(g.unk_f4)
        .i32(g.held);
    s.i32(g.object_in_hand).i32(g.flag_temp6).i32(g.result);
    s.md5(&g.map);
    s.0
}

impl HexaIo for HexaReplay<'_> {
    fn start_input(&mut self) -> (i32, i32, bool, bool) {
        self.0.input()
    }
    fn boundary(&mut self, g: &HexaGame) -> (i32, i32, bool, bool) {
        self.0.close(&hexa_state(g));
        self.0.input()
    }
    fn draw_background(&mut self) {
        self.0.add("bg");
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.0.sprite(bank, x, y, frame);
    }
    fn draw_standard_cursor(&mut self, _x: i32, _y: i32) {
        self.0.add("stdcursor");
    }
    async fn present(&mut self, _g: &HexaGame) {
        self.0.add("present");
    }
    fn play_sound(&mut self, slot: i32) {
        self.0.sound(&slot.to_string());
    }
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

#[test]
fn hexa_runs_vs_original() {
    let Some(data) = game("hexa_runs_vs_original") else {
        return;
    };
    let (map, spr) = map_and_sprites(&data, "PUZHEXAM.BMP", "PUZHEXA.SPR");
    let cov = read_coverage(&spr).unwrap();
    let lines = golden(include_str!("golden/ui_puzzle13_hexa.txt"));
    let sc = scenarios(&lines, 5);
    for s in &sc {
        let mut g = HexaGame::new(map.clone(), cov.clone());
        g.flag_temp6 = s.int(3);
        let mut io = HexaReplay(Replay::new(s.lines, (320, 470)));
        let got = block_on(g.run(&mut io));
        io.0.check(s, &hexa_state(&g), got, s.int(7));
    }
    assert_eq!(sc.len(), 3);
}

// The crab race (15).

struct CrabReplay<'a> {
    r: Replay<'a>,
    comment_until: i64,
    rand_seed: u32,
}

/// Comments read busy for this many passes after they start.
const COMMENT_FRAMES: i64 = 10;

fn crab_state(g: &CrabGame) -> Vec<u8> {
    let mut s = State::default();
    for c in &g.crabs {
        s.i16(c.kind).i16(c.x).i16(c.y).i16(c.dir);
        s.i16(c.phase).i16(c.timer).i16(c.interval).i16(c.reload);
    }
    s.i32(g.state)
        .i32(g.hand)
        .i32(g.last_comment)
        .cursor(&g.cursor);
    s.i32(g.result)
        .i16(g.lcg_seed as i16)
        .i32(i32::from(g.click));
    s.0
}

impl CrabIo for CrabReplay<'_> {
    fn start_input(&mut self) -> (i32, i32, bool) {
        let (x, y, c, _) = self.r.input();
        (x, y, c)
    }
    fn boundary(&mut self, g: &CrabGame) -> (i32, i32, bool) {
        self.r.close(&crab_state(g));
        self.start_input()
    }
    fn load_picture(&mut self, name: &str) {
        self.r.add(&format!("picture {name}"));
    }
    fn draw_background(&mut self) {
        self.r.add("bg");
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.r.sprite(bank, x, y, frame);
    }
    async fn present(&mut self, _g: &CrabGame) {
        self.r.add("present");
    }
    fn play_sound(&mut self, slot: i32) {
        self.r.sound(&slot.to_string());
    }
    fn play_comment(&mut self, n: i32) {
        self.r.sound(&format!("c{n}"));
        self.comment_until = self.r.frames as i64 + COMMENT_FRAMES;
    }
    fn comment_busy(&mut self) -> bool {
        (self.r.frames as i64) < self.comment_until
    }
    fn rand32767(&mut self) -> i32 {
        puzzle::crt_rand_next(&mut self.rand_seed)
    }
    fn aborted(&self) -> bool {
        self.r.aborted()
    }
}

#[test]
fn crab_runs_vs_original() {
    let lines = golden(include_str!("golden/ui_puzzle15_crab.txt"));
    let sc = scenarios(&lines, 7);
    for s in &sc {
        let mut g = CrabGame::default();
        g.lcg_seed = s.int(3) as u16;
        let mut io = CrabReplay {
            r: Replay::new(s.lines, (320, 240)),
            comment_until: -1,
            rand_seed: s.int(5) as u32,
        };
        let got = block_on(g.run(&mut io));
        io.r.check(s, &crab_state(&g), got, s.int(9));
    }
    assert_eq!(sc.len(), 5);
}

// Muria's tablets (16).

struct MuriaReplay<'a>(Replay<'a>);

fn muria_state(g: &MuriaGame) -> Vec<u8> {
    let mut s = State::default();
    for p in &g.pieces {
        s.i16(p.kind).i16(p.x).i16(p.y).i16(p.slot);
    }
    for t in &g.slots {
        s.i16(t.kind)
            .i16(t.x)
            .i16(t.y)
            .i16(t.cx)
            .i16(t.cy)
            .i16(t.piece);
    }
    s.i32(g.hand).cursor(&g.cursor).i32(g.result);
    s.i32(i32::from(g.click)).i32(g.flag_temp3);
    s.0
}

impl MuriaIo for MuriaReplay<'_> {
    fn start_input(&mut self) -> (i32, i32, bool) {
        let (x, y, c, _) = self.0.input();
        (x, y, c)
    }
    fn boundary(&mut self, g: &MuriaGame) -> (i32, i32, bool) {
        self.0.close(&muria_state(g));
        self.start_input()
    }
    fn load_picture(&mut self, name: &str) {
        self.0.add(&format!("picture {name}"));
    }
    fn draw_background(&mut self) {
        self.0.add("bg");
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.0.sprite(bank, x, y, frame);
    }
    async fn present(&mut self, _g: &MuriaGame) {
        self.0.add("present");
    }
    fn play_sound(&mut self, slot: i32) {
        self.0.sound(&slot.to_string());
    }
    async fn script_hook(&mut self, _flag_temp3: i32) {
        self.0.sound("script");
    }
    fn aborted(&self) -> bool {
        self.0.aborted()
    }
}

#[test]
fn muria_slot_at() {
    // FUN_0043f750: the top row is aimed 50 pixels above its click point,
    // the others at it; the nearest within 40 wins.
    let g = MuriaGame::default();
    assert_eq!(g.slot_at(186, 134), 0);
    assert_eq!(g.slot_at(186, 184), 0); // 25 away: still slot 0
    assert_eq!(g.slot_at(186, 300), -1);
    assert_eq!(g.slot_at(175, 227), 4);
    assert_eq!(g.slot_at(478, 269), 11);
    assert_eq!(g.slot_at(320, 420), -1);
}

#[test]
fn muria_runs_vs_original() {
    let lines = golden(include_str!("golden/ui_puzzle16_muria.txt"));
    let sc = scenarios(&lines, 3);
    for s in &sc {
        let mut g = MuriaGame::default();
        let mut io = MuriaReplay(Replay::new(s.lines, (320, 240)));
        let got = block_on(g.run(&mut io));
        io.0.check(s, &muria_state(&g), got, s.int(5));
    }
    assert_eq!(sc.len(), 4);
}

// The puzzles in the whole engine.

/// The engine started on StartPuzzle `id` with `object` in hand
/// ([`Start::Puzzle`]), driven one frame a step, the sound mixed.
struct Engine {
    sys: Rc<Sys>,
    runner: Runner,
    trace: Rc<RefCell<Vec<Event>>>,
    step: u64,
    left: bool,
    right: bool,
}

impl Engine {
    fn new(g: Game, id: i32, object: i32) -> Engine {
        let sys = Sys::new(Mixer::new());
        let trace = Rc::new(RefCell::new(Vec::new()));
        let opts = Options {
            trace: Some(trace.clone()),
            start: Start::Puzzle { id, object },
            ..Options::default()
        };
        let runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
        Engine {
            sys,
            runner,
            trace,
            step: 0,
            left: false,
            right: false,
        }
    }

    fn result(&self) -> Option<i32> {
        self.trace.borrow().iter().find_map(|e| match e {
            Event::Puzzle { result, .. } => Some(*result),
            _ => None,
        })
    }

    /// One frame, after the input of a pass: the mouse at (x, y), a click
    /// (the button pressed, released first when it is down), the right
    /// button.
    fn pass(&mut self, x: i32, y: i32, click: bool, right: bool) {
        let s = &self.sys;
        s.push_input(Input::MouseMove { x, y });
        let button = |b, down| {
            if down {
                Input::MouseDown { button: b, x, y }
            } else {
                Input::MouseUp { button: b, x, y }
            }
        };
        if self.left {
            s.push_input(button(MouseButton::Left, false));
            self.left = false;
        }
        if click {
            s.push_input(button(MouseButton::Left, true));
            self.left = true;
        }
        if right != self.right {
            s.push_input(button(MouseButton::Right, right));
            self.right = right;
        }
        self.frame();
    }

    fn frame(&mut self) {
        assert!(self.runner.step(self.step * 16), "the engine ended");
        let mut sound = vec![0.0f32; 2 * (22050 * 16 / 1000)];
        self.sys.mixer.fill(&mut sound, 22050, 2);
        self.step += 1;
    }

    /// Frames with the mouse still until the puzzle ends.
    fn until_end(&mut self, max: usize) -> Option<i32> {
        for _ in 0..max {
            if let Some(r) = self.result() {
                return Some(r);
            }
            self.frame();
        }
        self.result()
    }
}

/// A scenario of the original's runs replayed through the engine's input:
/// the input of pass i is in the queue when the engine reads it. The first
/// pass's click cannot be (a button already down when a puzzle starts is
/// no click): the scenarios used have none.
fn replay_in_engine(g: Game, id: i32, object: i32, s: &Scenario) -> i32 {
    let mut e = Engine::new(g, id, object);
    let mut colours = 0;
    for (i, l) in s.lines.iter().enumerate() {
        let tok: Vec<&str> = l[..l.find('|').unwrap()].split_whitespace().collect();
        let (x, y) = (tok[1].parse().unwrap(), tok[2].parse().unwrap());
        e.pass(x, y, tok[3] == "1", tok.get(4) == Some(&"1"));
        if e.result().is_some() {
            break;
        }
        if i == 2 {
            // The puzzle's picture is on the screen.
            colours = e
                .sys
                .screen
                .borrow()
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len();
        }
    }
    assert!(colours > 100, "puzzle {id}: {colours} colours on screen");
    e.until_end(2000).expect("the puzzle did not end")
}

fn engine_scenario(file: &str, frames_tok: usize, name: &str) -> (Vec<String>, usize) {
    let lines = golden(file);
    let sc = scenarios(&lines, frames_tok);
    let s = sc.iter().find(|s| s.tok[1] == name).unwrap();
    (
        s.lines.iter().map(|l| l.to_string()).collect(),
        s.lines.len(),
    )
}

fn as_scenario<'a>(lines: &'a [&'a str]) -> Scenario<'a> {
    Scenario {
        tok: vec!["scenario", "engine"],
        lines,
    }
}

#[test]
fn engine_plays_throne() {
    let Some(g) = game("engine_plays_throne") else {
        return;
    };
    let (lines, _) = engine_scenario(include_str!("golden/ui_puzzle5_throne.txt"), 3, "win");
    // The first pass's click (on the lever, which only checks the rings)
    // is dropped.
    let mut l: Vec<&str> = lines.iter().map(String::as_str).collect();
    let first = l[0].replacen(" 1 |", " 0 |", 1);
    l[0] = &first;
    assert_eq!(replay_in_engine(g, 5, 0, &as_scenario(&l)), 255);
}

#[test]
fn engine_plays_taquin() {
    let Some(g) = game("engine_plays_taquin") else {
        return;
    };
    let (lines, _) = engine_scenario(include_str!("golden/ui_puzzle6_taquin.txt"), 7, "v6_win");
    let l: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(replay_in_engine(g, 6, 322, &as_scenario(&l)), 255);
}

#[test]
fn engine_plays_meta() {
    let Some(g) = game("engine_plays_meta") else {
        return;
    };
    let (lines, _) = engine_scenario(include_str!("golden/ui_puzzle9_meta.txt"), 7, "win_stage1");
    let l: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(replay_in_engine(g, 9, 0, &as_scenario(&l)), 255);
}

#[test]
fn engine_plays_hexa() {
    let Some(g) = game("engine_plays_hexa") else {
        return;
    };
    let (lines, _) = engine_scenario(include_str!("golden/ui_puzzle13_hexa.txt"), 5, "win");
    let l: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(replay_in_engine(g, 13, 0, &as_scenario(&l)), 255);
}

#[test]
fn engine_plays_muria() {
    let Some(g) = game("engine_plays_muria") else {
        return;
    };
    let (lines, _) = engine_scenario(include_str!("golden/ui_puzzle16_muria.txt"), 3, "solve");
    let l: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(replay_in_engine(g, 16, 0, &as_scenario(&l)), 255);
}

/// The crab race played to its end: a crab chosen and put down on the
/// start line for every round, the races run on their own (their outcome
/// depends on the clock-seeded LCG).
#[test]
fn engine_plays_crab() {
    let Some(g) = game("engine_plays_crab") else {
        return;
    };
    let mut e = Engine::new(g, 15, 0);
    let mut i = 0u64;
    while e.result().is_none() && i < 60_000 {
        match i % 60 {
            0 => e.pass(159, 133, true, false), // box 0
            30 => e.pass(80, 240, true, false), // the start line
            _ => e.pass(320, 300, false, false),
        }
        i += 1;
    }
    let r = e.result().expect("the race did not end");
    eprintln!("crab race: FinPuzzle {r} after {i} frames");
    assert!(r == 255 || r == 2, "crab race: {r}");
    // Three rounds at least, each with its two introductions and its score
    // comment.
    assert!(i > 3 * 1000, "crab race over in {i} frames");
}

/// Leaving every translated puzzle at once (a click on its frame) gives
/// 2, the crab's choice screen 1; a refused puzzle (an object in hand)
/// gives 1, a puzzle not translated yet counts as won.
#[test]
fn engine_leaves_puzzles() {
    if game("engine_leaves_puzzles").is_none() {
        return;
    }
    let cases = [
        (5, 0, (20, 240), 2),
        (6, 0, (30, 200), 2),
        (7, 0, (30, 200), 2),
        (14, 0, (30, 200), 2),
        (9, 0, (5, 5), 2),
        (13, 0, (5, 5), 2),
        (15, 0, (320, 10), 1),
        (16, 0, (320, 10), 2),
        (5, 7, (0, 0), 1),
        (0, 0, (0, 0), 1), // the eclipse: its top corners leave with 1
        (17, 0, (0, 0), 0),
    ];
    for (id, object, (x, y), want) in cases {
        let g = game("engine_leaves_puzzles").unwrap();
        let mut e = Engine::new(g, id, object);
        for k in 0..40 {
            if e.result().is_some() {
                break;
            }
            e.pass(x, y, k == 20, false);
        }
        assert_eq!(e.until_end(2000), Some(want), "puzzle {id}");
    }
}

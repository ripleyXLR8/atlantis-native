//! The rat's run (StartPuzzle 4) and the ogre game (2) against the
//! original `atlantis.exe` (2001 English build).
//!
//! The rat: its frame rules step by step, and whole runs replayed pass by
//! pass (the input of each pass, the sounds started, the md5 of the draws
//! and of the state must be the original's), then played in the whole
//! engine with the same input. The ogre game: its rules (every pixel,
//! every cell, every queen walk, the breadth-first search of every
//! reachable position), its drawing, the queen's placement, and games
//! played in the whole engine.
//!
//! The expected values come from the original code run under emulation
//! (see the headers of the golden files); they hold numbers and digests
//! only. Ported from ScummVM's `test/engines/cryomni3d/atlantis/`
//! (`ui_puzzle4_rules.h`, `ui_puzzle4_run.h`, `ui_puzzle_ogre.h`,
//! `ui_ogre_draw.h`, `ui_puzzle_random.h`).
//!
//! The tests that read the game (the rat's trajectories, the engine runs)
//! need `ATLANTIS_DATA`, the game directory; without it they are skipped.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::puzzle::ogre::{self, OgreGame, OgreIo, OgreRules, Piece};
use atlantis_engine::puzzle::rat::{self, RatGame, RatIo, RatRules, Slot};
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

/// The `key value` lines of a golden file.
fn golden_values(text: &str) -> HashMap<&str, &str> {
    golden(text)
        .into_iter()
        .filter_map(|l| l.split_once(' '))
        .collect()
}

// The rat's run (4): the frame rules.

fn rat_rules_state(r: &RatRules) -> Vec<u8> {
    let mut st = r.mem[..rat::STATE_SIZE].to_vec();
    st.extend_from_slice(&r.clock_ticked.to_le_bytes());
    st.extend_from_slice(&r.result.to_le_bytes());
    st
}

#[test]
fn rat_rules_vs_original() {
    let lines = golden(include_str!("golden/ui_puzzle4_rules.txt"));
    let mut r = RatRules::default();
    let mut init_checked = false;
    r.reset();
    // The runs activate records 0..9 and 11..28.
    for k in (0..10).chain(11..29) {
        r.set_b(rat::rec(k, rat::ACTIVE), 1);
    }
    let mut steps = 0;
    for l in lines {
        if let Some(want) = l.strip_prefix("init ") {
            let fresh = RatRules::default();
            assert_eq!(md5_hex(&fresh.mem[..rat::STATE_SIZE]), want);
            init_checked = true;
            continue;
        }
        let parts: Vec<&str> = l.split('|').map(str::trim).collect();
        let tok: Vec<&str> = parts[0].split_whitespace().collect();
        let (mx, my) = (tok[1].parse().unwrap(), tok[2].parse().unwrap());
        let (click, held) = (tok[3] == "1", tok[4] == "1");
        let ticks: u32 = tok[5].parse().unwrap();
        if tok[6] != "-" {
            r.set_b(0x0049_6ca5, tok[6].parse().unwrap());
        }
        for t in &tok[7..] {
            if let Some(v) = t.strip_prefix("cc0=") {
                r.set_i(0x0049_6cc0, v.parse().unwrap());
            } else if *t == "clear" {
                r.set_b(0x0049_69c2, 0);
                r.result = 0;
            }
        }
        let mut sounds = Vec::new();
        r.clicks(mx, my, click, held, &mut sounds);
        r.frame_updates(ticks);
        let snd: Vec<String> = sounds.iter().map(i32::to_string).collect();
        assert_eq!(snd.join(" "), parts[1], "step {}: sounds", tok[0]);
        assert_eq!(md5_hex(&rat_rules_state(&r)), parts[2], "step {}", tok[0]);
        steps += 1;
    }
    assert!(init_checked);
    assert_eq!(steps, 6000);
}

// The rat's run: whole runs.

/// The trajectories, from the game.
fn rat_slots(g: &Game) -> Vec<Slot> {
    rat::SLOT_FILES
        .iter()
        .map(|n| {
            let d = g
                .big
                .read(&format!("PUZZLES\\{n}"))
                .unwrap_or_else(|| panic!("{n}"));
            Slot::parse(&d).unwrap_or_else(|| panic!("{n}: short"))
        })
        .collect()
}

/// The compared state: 0x004969c0..0x00496cd7, the result, 0x00772d90,
/// 0x00772d94, and the animation entry 0x00496eea..0x00496eef of the rat
/// byte's data-section value 0x68.
fn rat_state(g: &RatGame) -> Vec<u8> {
    let mut st = g.rules.mem[..rat::STATE_SIZE].to_vec();
    st.extend_from_slice(&g.rules.result.to_le_bytes());
    st.extend_from_slice(&g.rules.clock_ticked.to_le_bytes());
    st.extend_from_slice(&g.wheel_y.to_le_bytes());
    let e = (0x0049_6eea - rat::BASE) as usize;
    st.extend_from_slice(&g.rules.mem[e..e + 6]);
    st
}

/// A scenario of the run file: its header's tokens and its pass lines.
struct Scenario<'a> {
    tok: Vec<&'a str>,
    lines: Vec<&'a str>,
}

/// The scenarios; `frames_tok` is the index of the pass count in the
/// header.
fn scenarios<'a>(lines: &[&'a str], frames_tok: usize) -> Vec<Scenario<'a>> {
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if let Some(rest) = l.strip_prefix("scenario ") {
            let tok: Vec<&str> = std::iter::once("scenario").chain(rest.split(' ')).collect();
            let n: usize = tok[frames_tok].parse().unwrap();
            out.push(Scenario {
                tok,
                lines: lines[i + 1..i + 1 + n].to_vec(),
            });
        }
    }
    out
}

/// The input of a pass line: x, y, click, held, the rand values.
fn pass_input(l: &str) -> (i32, i32, bool, bool, Vec<i32>) {
    let tok: Vec<&str> = l[..l.find('|').unwrap()].split_whitespace().collect();
    let rands = tok
        .get(5)
        .and_then(|t| t.strip_prefix("rand="))
        .map(|v| v.split(',').map(|r| r.parse().unwrap()).collect())
        .unwrap_or_default();
    (
        tok[1].parse().unwrap(),
        tok[2].parse().unwrap(),
        tok[3] == "1",
        tok[4] == "1",
        rands,
    )
}

/// The pass-by-pass comparison of the C++ test: a pass is closed (its
/// draws, sounds and state compared with its line) at the next input.
struct RatReplay<'a> {
    lines: &'a [&'a str],
    next: usize,
    open: bool,
    frames: usize,
    mismatches: u32,
    first_mismatch: String,
    overrun: bool,
    draws: String,
    sounds: String,
    clock: u32,
    rands: Vec<i32>,
}

impl RatReplay<'_> {
    fn add(&mut self, s: &str) {
        if !self.draws.is_empty() {
            self.draws.push('\n');
        }
        self.draws.push_str(s);
    }

    fn close(&mut self, g: &RatGame) {
        if !self.open {
            return;
        }
        self.open = false;
        self.frames += 1;
        let l = self.lines[self.next - 1];
        let parts: Vec<&str> = l.split('|').map(str::trim).collect();
        let got_draws = md5_hex(self.draws.as_bytes());
        let got_state = md5_hex(&rat_state(g));
        if got_draws != parts[2] || got_state != parts[3] || self.sounds != parts[1] {
            if self.mismatches == 0 {
                self.first_mismatch = format!(
                    "line '{l}': sounds '{}' draws {} state {}",
                    self.sounds,
                    if got_draws == parts[2] {
                        "ok"
                    } else {
                        "differ"
                    },
                    if got_state == parts[3] {
                        "ok"
                    } else {
                        "differ"
                    }
                );
            }
            self.mismatches += 1;
        }
    }
}

impl RatIo for RatReplay<'_> {
    fn read_input(&mut self, g: &RatGame) -> (i32, i32, bool, bool) {
        self.close(g);
        self.draws.clear();
        self.sounds.clear();
        if self.next >= self.lines.len() || self.mismatches != 0 {
            // Past the recorded passes: the run stops.
            self.overrun = true;
            return (0, 0, false, false);
        }
        let (x, y, click, held, rands) = pass_input(self.lines[self.next]);
        self.next += 1;
        self.open = true;
        self.rands = rands;
        self.rands.reverse();
        (x, y, click, held)
    }
    fn draw_background(&mut self, _which: i32) {
        self.add("bg");
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.add(&format!("{bank} {x} {y} {frame}"));
    }
    async fn present(&mut self, _g: &RatGame) {
        self.add("present");
    }
    fn play_sound(&mut self, slot: i32) {
        if !self.sounds.is_empty() {
            self.sounds.push(' ');
        }
        self.sounds.push_str(&slot.to_string());
    }
    fn random_value(&mut self) -> i32 {
        self.rands.pop().unwrap_or(0)
    }
    fn ticks(&mut self) -> u32 {
        self.clock
    }
    fn wait_until(&mut self, t: u32) {
        self.clock = t;
    }
    fn aborted(&self) -> bool {
        self.overrun
    }
}

/// One play replayed from `rules` with the timer at `clock`: the game as
/// the play left it, and the timer.
fn rat_replay(
    name: &str,
    rules: RatRules,
    slots: &[Slot],
    lines: &[&str],
    clock: u32,
    want: i32,
) -> (RatGame, u32) {
    let mut game = RatGame::new(rules, slots.to_vec());
    let mut io = RatReplay {
        lines,
        next: 0,
        open: false,
        frames: 0,
        mismatches: 0,
        first_mismatch: String::new(),
        overrun: false,
        draws: String::new(),
        sounds: String::new(),
        clock,
        rands: Vec::new(),
    };
    let got = block_on(game.run(&mut io));
    io.close(&game);
    assert_eq!(io.first_mismatch, "", "{name}");
    assert_eq!(io.mismatches, 0, "{name}");
    assert!(!io.overrun, "{name}: ran past its passes");
    assert_eq!(io.frames, lines.len(), "{name}: passes");
    assert_eq!(got, want, "{name}: result");
    (game, io.clock)
}

/// Every scenario; a "play 2" block replays a second play from the state
/// the first left (the original's globals last as long as the program:
/// `rat::play` keeps them in `PuzzleState::rat`), the timer going on.
#[test]
fn rat_runs_vs_original() {
    let Some(g) = game("rat_runs_vs_original") else {
        return;
    };
    let slots = rat_slots(&g);
    let lines = golden(include_str!("golden/ui_puzzle4_run.txt"));
    let sc = scenarios(&lines, 3);
    let (mut passes, mut plays) = (0, 0);
    for s in &sc {
        let name = s.tok[1];
        let want = s.tok[5].parse::<i32>().unwrap();
        let (game, clock) = rat_replay(name, RatRules::default(), &slots, &s.lines, 1000, want);
        passes += s.lines.len();
        plays += 1;
        // The line after the scenario's passes.
        let at = lines
            .iter()
            .position(|l| l.starts_with(&format!("scenario {name} ")))
            .unwrap()
            + 1
            + s.lines.len();
        if let Some(h) = lines.get(at).and_then(|l| l.strip_prefix("play 2 ")) {
            let tok: Vec<&str> = h.split(' ').collect();
            assert_eq!((tok[0], tok[2]), ("frames", "result"));
            let n: usize = tok[1].parse().unwrap();
            let p2 = &lines[at + 1..at + 1 + n];
            rat_replay(
                &format!("{name} play 2"),
                game.rules,
                &slots,
                p2,
                clock,
                tok[3].parse().unwrap(),
            );
            passes += n;
            plays += 1;
        }
    }
    assert_eq!(sc.len(), 5);
    assert_eq!(plays, 6);
    eprintln!("rat: {plays} plays, {passes} passes matched");
}

#[test]
fn rat_slot_parse() {
    // Two points, then a short file.
    let mut d = vec![2u8, 0, 7, 9];
    for v in [1.5f32, 2.5, -3.0, 4.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    let s = Slot::parse(&d).unwrap();
    assert_eq!(s.b, vec![7, 9]);
    assert_eq!(s.x, vec![1.5, 2.5]);
    assert_eq!(s.y, vec![-3.0, 4.0]);
    assert!(Slot::parse(&d[..d.len() - 1]).is_none());
}

// The whole engine.

struct Engine {
    sys: Rc<Sys>,
    runner: Runner,
    trace: Rc<RefCell<Vec<Event>>>,
    step: u64,
    /// The length of a frame.
    ms: u64,
    left: bool,
}

impl Engine {
    fn new(g: Game, id: i32, ms: u64) -> Engine {
        let sys = Sys::new(Mixer::new());
        let trace = Rc::new(RefCell::new(Vec::new()));
        let opts = Options {
            trace: Some(trace.clone()),
            start: Start::Puzzle { id, object: 0 },
            ..Options::default()
        };
        let runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
        Engine {
            sys,
            runner,
            trace,
            step: 0,
            ms,
            left: false,
        }
    }

    fn result(&self) -> Option<i32> {
        self.trace.borrow().iter().find_map(|e| match e {
            Event::Puzzle { result, .. } => Some(*result),
            _ => None,
        })
    }

    fn button(&mut self, down: bool, x: i32, y: i32) {
        let b = MouseButton::Left;
        self.sys.push_input(if down {
            Input::MouseDown { button: b, x, y }
        } else {
            Input::MouseUp { button: b, x, y }
        });
        self.left = down;
    }

    /// One frame after the input of a pass: the mouse at (x, y), a click
    /// (a press), the left button held at the end of the pass.
    fn pass(&mut self, x: i32, y: i32, click: bool, held: bool) {
        self.sys.push_input(Input::MouseMove { x, y });
        if click {
            if self.left {
                self.button(false, x, y);
            }
            self.button(true, x, y);
        }
        if self.left != held {
            self.button(held, x, y);
        }
        self.frame();
    }

    fn frame(&mut self) {
        assert!(self.runner.step(self.step * self.ms), "the engine ended");
        let mut sound = vec![0.0f32; 2 * (22050 * self.ms as usize / 1000)];
        self.sys.mixer.fill(&mut sound, 22050, 2);
        self.step += 1;
    }

    fn until_end(&mut self, max: usize) -> Option<i32> {
        for _ in 0..max {
            if let Some(r) = self.result() {
                return Some(r);
            }
            self.frame();
        }
        self.result()
    }

    fn colours(&self) -> usize {
        self.sys
            .screen
            .borrow()
            .iter()
            .collect::<HashSet<_>>()
            .len()
    }
}

/// A run of the original replayed through the engine's input, one pass a
/// frame of 25 ms (the 5 ticks every pass waits for), as the emulated
/// timer counts them. The C rand differs (it only picks the ambient
/// sounds).
///
/// The scripted players set the held flag directly, which a button cannot
/// do: the window procedure sets it from the button's messages, one a pass
/// and none on a press edge, so it goes up the pass after a press and down
/// on the pass of a release. The button is pressed the pass before each
/// rise of the held flag, released on each fall, and kept down through a
/// one-pass drop between two held passes (a button cannot be released and
/// pressed again within a pass without a click).
fn rat_in_engine(name: &str) -> (i32, usize) {
    let g = game("rat_in_engine").unwrap();
    let lines = golden(include_str!("golden/ui_puzzle4_run.txt"));
    let sc = scenarios(&lines, 3);
    let s = sc.iter().find(|s| s.tok[1] == name).unwrap();
    let input: Vec<_> = s.lines.iter().map(|l| pass_input(l)).collect();
    let held = |i: usize| -> bool {
        let Some(p) = input.get(i) else { return false };
        p.3 || (i > 0 && input[i - 1].3 && input.get(i + 1).is_some_and(|q| q.3 && !q.2))
    };
    let mut e = Engine::new(g, 4, 25);
    let mut colours = 0;
    let mut n = 0;
    for (i, &(x, y, click, _, _)) in input.iter().enumerate() {
        e.pass(x, y, click, held(i) || held(i + 1));
        n += 1;
        if e.result().is_some() {
            break;
        }
        if i == 2 {
            colours = e.colours();
        }
    }
    assert!(colours > 100, "rat: {colours} colours on screen");
    (e.until_end(200).expect("the run did not end"), n)
}

#[test]
fn engine_plays_rat() {
    if game("engine_plays_rat").is_none() {
        return;
    }
    let (r, n) = rat_in_engine("win");
    eprintln!("rat in the engine: FinPuzzle {r} after {n} passes");
    assert_eq!(r, 255);
    assert_eq!(rat_in_engine("abort").0, 1);
    assert_eq!(rat_in_engine("lose_all").0, 2);
}

// The ogre game (2): the rules.

/// The centres of the cells.
const CELL_PX: [i32; 7] = [143, 203, 263, 323, 383, 443, 503];
const CELL_PY: [i32; 7] = [59, 119, 179, 239, 299, 359, 419];

/// The drop of `FUN_00443c3c` without its drawing: -1 refused, else the
/// result (0 on, 255 the queen caught, 2 the hero caught).
fn drop(r: &mut OgreRules, mx: i32, my: i32, ogres: bool) -> i32 {
    if !r.drop(mx, my) {
        return -1;
    }
    if r.queen_caught() {
        return 255;
    }
    if ogres {
        r.move_ogres()
    } else {
        0
    }
}

fn ogre_values() -> HashMap<&'static str, &'static str> {
    golden_values(include_str!("golden/ui_puzzle2_ogre.txt"))
}

fn hero_at(x: i32, y: i32) -> OgreRules {
    let mut r = OgreRules::default();
    r.p[0].x = x;
    r.p[0].y = y;
    r
}

#[test]
fn ogre_initial_pieces() {
    let v = ogre_values();
    // The static records before the set-up, then FUN_0044203c's.
    let init: Vec<i32> = v["init.records"]
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let mut r = OgreRules::default();
    for (i, p) in r.p.iter_mut().enumerate() {
        *p = Piece {
            flag: init[3 * i],
            x: init[3 * i + 1],
            y: init[3 * i + 2],
        };
    }
    r.occ[10] = 1;
    r.reset_board();
    let p = |f, x, y| Piece { flag: f, x, y };
    assert_eq!(r.p, [p(1, 0, 0), p(0, 3, 3), p(1, 6, 6), p(1, 0, 6)]);
    // The original leaves every cell free, the ogres' included.
    assert!(r.occ.iter().all(|&c| c == 0));
    let edges: Vec<i32> = v["cell_edges"]
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(edges[..8], ogre::COL_EDGE);
    assert_eq!(edges[8..], ogre::ROW_EDGE);
}

#[test]
fn ogre_pickup_and_abort_every_pixel() {
    let v = ogre_values();
    for (hx, hy) in [(0, 0), (6, 6), (3, 4)] {
        let r = hero_at(hx, hy);
        let (mut count, mut aborts) = (0, 0);
        let (mut x0, mut x1, mut y0, mut y1) = (i32::MAX, -1, i32::MAX, -1);
        for y in 0..480 {
            for x in 0..640 {
                if r.pick_up(x, y) {
                    count += 1;
                    (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
                } else if ogre::abort_click(x) {
                    aborts += 1;
                }
            }
        }
        let got = format!("count={count} x={x0}..{x1} y={y0}..{y1} abortpixels={aborts}");
        assert_eq!(got, v[format!("pickup.{hx}.{hy}").as_str()]);
    }
}

/// `pathcol` / `pathrow`: runs of `cell:first-last`.
fn lookup_runs(f: impl Fn(i32) -> i32, n: i32) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut start = 0;
    for i in 1..=n {
        if i == n || f(i) != f(start) {
            out.push(format!("{}:{}-{}", f(start), start, i - 1));
            start = i;
        }
    }
    out.join("_")
}

#[test]
fn ogre_cell_lookup_every_pixel() {
    let v = ogre_values();
    assert_eq!(lookup_runs(ogre::cell_column, 640), v["pathcol"]);
    assert_eq!(lookup_runs(ogre::cell_row, 480), v["pathrow"]);
}

fn path_listing(non_diagonal: bool) -> (String, usize) {
    let mut lines = Vec::new();
    let mut legal = 0;
    for hy in 0..7 {
        for hx in 0..7 {
            for cy in 0..7 {
                for cx in 0..7 {
                    if non_diagonal && hx != cx && hy != cy {
                        continue;
                    }
                    let mut r = hero_at(hx, hy);
                    r.build_move_mask(CELL_PX[cx as usize], CELL_PY[cy as usize]);
                    let d = (cx - hx).abs() + (cy - hy).abs();
                    if (1..=2).contains(&d) {
                        legal += 1;
                    }
                    let cells: Vec<String> = (0..49)
                        .filter(|&i| r.mask[i] == 1)
                        .map(|i| i.to_string())
                        .collect();
                    let c = |v: i32| {
                        if v == -1 {
                            "-".to_string()
                        } else {
                            v.to_string()
                        }
                    };
                    let path: Vec<String> = (0..3)
                        .map(|i| format!("{},{}", c(r.path_x[i]), c(r.path_y[i])))
                        .collect();
                    lines.push(format!(
                        "{hx} {hy} {cx} {cy} {} {}",
                        cells.join("."),
                        path.join(",")
                    ));
                }
            }
        }
    }
    (md5_hex(lines.join("\n").as_bytes()), legal)
}

#[test]
fn ogre_path_builder_every_cell() {
    let v = ogre_values();
    let (md5, legal) = path_listing(false);
    assert_eq!(md5, v["pathbuild_md5"]);
    assert_eq!(legal.to_string(), v["pathbuild_legal_count"]);
    assert_eq!(path_listing(true).0, v["pathbuild_nondiag_md5"]);
    // Hero (6, 0), the cursor at the centre of (5, 1): the elbow (5, 0).
    let mut r = hero_at(6, 0);
    r.build_move_mask(443, 119);
    assert_eq!((r.path_x[1], r.path_y[1]), (5, 0));
}

fn catch_listing(non_diagonal: bool) -> (usize, String) {
    let mut lines = Vec::new();
    for hy in 0..7 {
        for hx in 0..7 {
            for cy in 0..7i32 {
                for cx in 0..7i32 {
                    if (cx - hx).abs() + (cy - hy).abs() > 2
                        || (non_diagonal && hx != cx && hy != cy)
                    {
                        continue;
                    }
                    for qy in 0..7 {
                        for qx in 0..7 {
                            let mut r = hero_at(hx, hy);
                            r.p[1].x = qx;
                            r.p[1].y = qy;
                            let (mx, my) = (CELL_PX[cx as usize], CELL_PY[cy as usize]);
                            r.build_move_mask(mx, my);
                            if drop(&mut r, mx, my, false) == 255 {
                                lines.push(format!("{hx} {hy} {cx} {cy} {qx} {qy}"));
                            }
                        }
                    }
                }
            }
        }
    }
    (lines.len(), md5_hex(lines.join("\n").as_bytes()))
}

#[test]
fn ogre_catch_every_move() {
    let v = ogre_values();
    let (n, md5) = catch_listing(false);
    assert_eq!(n.to_string(), v["catch_count"]);
    assert_eq!(md5, v["catch_md5"]);
    let (n, md5) = catch_listing(true);
    assert_eq!(n.to_string(), v["catch_nondiag_count"]);
    assert_eq!(md5, v["catch_nondiag_md5"]);
}

fn walk_listing(non_diagonal: bool) -> (usize, usize, String) {
    let mut lines = Vec::new();
    let mut stay = 0;
    for hy in 0..7 {
        for hx in 0..7 {
            for cy in 0..7i32 {
                for cx in 0..7i32 {
                    if (cx - hx).abs() + (cy - hy).abs() > 2
                        || (non_diagonal && hx != cx && hy != cy)
                    {
                        continue;
                    }
                    for qy in 0..7 {
                        for qx in 0..7 {
                            for first in 0..4 {
                                for cont in 0..2 {
                                    let mut r = hero_at(hx, hy);
                                    r.build_move_mask(CELL_PX[cx as usize], CELL_PY[cy as usize]);
                                    r.p[1].x = qx;
                                    r.p[1].y = qy;
                                    let pick = |b: bool| if b { 20000 } else { 100 };
                                    let mut rnd = vec![pick(first & 1 != 0), pick(first & 2 != 0)]
                                        .into_iter();
                                    let default = pick(cont != 0);
                                    r.walk_queen(|| rnd.next().unwrap_or(default));
                                    if (r.p[1].x, r.p[1].y) == (qx, qy) {
                                        stay += 1;
                                    }
                                    lines.push(format!(
                                        "{hx} {hy} {cx} {cy} {qx} {qy} {first} {cont} {} {}",
                                        r.p[1].x, r.p[1].y
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    (lines.len(), stay, md5_hex(lines.join("\n").as_bytes()))
}

#[test]
fn ogre_queen_walk_every_case() {
    let v = ogre_values();
    let (n, stay, md5) = walk_listing(false);
    assert_eq!(n.to_string(), v["walk_count"]);
    assert_eq!(stay.to_string(), v["walk_stay_count"]);
    assert_eq!(md5, v["walk_md5"]);
    let (n, _, md5) = walk_listing(true);
    assert_eq!(n.to_string(), v["walk_nondiag_count"]);
    assert_eq!(md5, v["walk_nondiag_md5"]);
}

/// The ogres' diversion (FUN_00442360's scan @ 0x00442586 tests the cell
/// (x0 + xx, yy) and moves to (x0, yy)): the hero drops on (3, 0) from
/// (2, 0), the ogres at (4, 4) and (2, 4): A goes to (3, 3), B is diverted
/// to (1, 5).
#[test]
fn ogre_diversion() {
    let mut r = hero_at(2, 0);
    r.p[1] = Piece {
        flag: 0,
        x: 99,
        y: 99,
    };
    r.p[2] = Piece {
        flag: 1,
        x: 4,
        y: 4,
    };
    r.p[3] = Piece {
        flag: 1,
        x: 2,
        y: 4,
    };
    r.occ[4 * 7 + 4] = 1;
    r.occ[4 * 7 + 2] = 1;
    let (mx, my) = (CELL_PX[3], CELL_PY[0]);
    r.build_move_mask(mx, my);
    assert_eq!(drop(&mut r, mx, my, true), 0);
    assert_eq!((r.p[2].x, r.p[2].y), (3, 3));
    assert_eq!((r.p[3].x, r.p[3].y), (1, 5));
}

fn state_key(r: &OgreRules) -> String {
    let cells: Vec<String> = (0..49)
        .filter(|&i| r.occ[i] != 0)
        .map(|i| i.to_string())
        .collect();
    format!(
        "{},{},{},{},{},{},{}",
        r.p[0].x,
        r.p[0].y,
        r.p[2].x,
        r.p[2].y,
        r.p[3].x,
        r.p[3].y,
        cells.join(".")
    )
}

/// Every position reachable from `start` by the hero's drops on the cell
/// centres (the queen off the board): the states, the sorted transitions
/// (`from cx cy result to`, the start written `start_name`) and the
/// losing ones.
fn bfs(start: &OgreRules, start_name: Option<&str>) -> (usize, Vec<String>, usize) {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    seen.insert(state_key(start));
    queue.push_back(start.clone());
    let mut lines = Vec::new();
    let mut losses = 0;
    let mut first = true;
    while let Some(cur) = queue.pop_front() {
        let from = match start_name {
            Some(n) if first => n.to_string(),
            _ => state_key(&cur),
        };
        first = false;
        for (cy, &my) in CELL_PY.iter().enumerate() {
            for (cx, &mx) in CELL_PX.iter().enumerate() {
                let mut g = cur.clone();
                g.build_move_mask(mx, my);
                let r = drop(&mut g, mx, my, true);
                if r < 0 {
                    continue;
                }
                let to = state_key(&g);
                lines.push(format!("{from} {cx} {cy} {r} {to}"));
                if r == 2 {
                    losses += 1;
                }
                if r == 0 && seen.insert(to) {
                    queue.push_back(g);
                }
            }
        }
    }
    lines.sort();
    (seen.len(), lines, losses)
}

fn bfs_start(marked: bool, padded: bool) -> OgreRules {
    let mut s = hero_at(0, 0);
    s.p[1] = Piece {
        flag: 0,
        x: 99,
        y: 99,
    };
    s.p[2] = Piece {
        flag: 1,
        x: 6,
        y: 6,
    };
    s.p[3] = Piece {
        flag: 1,
        x: 0,
        y: 6,
    };
    if marked {
        s.occ[48] = 1;
        s.occ[42] = 1;
    }
    if padded {
        s.occ[49..].fill(1);
    }
    s
}

#[test]
fn ogre_every_reachable_position() {
    let v = ogre_values();
    // From the ogres' cells marked, as the ScummVM test.
    let (states, lines, losses) = bfs(&bfs_start(true, false), None);
    assert_eq!(states.to_string(), v["bfs_states"]);
    assert_eq!(lines.len().to_string(), v["bfs_transitions"]);
    assert_eq!(losses.to_string(), v["bfs_loss_transitions"]);
    assert_eq!(md5_hex(lines.join("\n").as_bytes()), v["bfs_md5"]);
    // From the original's start (no cell marked): the same game.
    let (states, lines, _) = bfs(&bfs_start(false, false), Some("START"));
    assert_eq!(states.to_string(), v["original_start_states"]);
    assert_eq!(lines.len().to_string(), v["original_start_transitions"]);
    assert_eq!(
        md5_hex(lines.join("\n").as_bytes()),
        v["original_start_md5"]
    );
    let (_, marked, _) = bfs(&bfs_start(true, false), Some("START"));
    assert_eq!(md5_hex(marked.join("\n").as_bytes()), v["port_start_md5"]);
    // The bytes past the 49 cells matter: set to 1, another game.
    let (states, padded, _) = bfs(&bfs_start(false, true), Some("START"));
    assert_eq!(states.to_string(), v["padded_start_states"]);
    assert_eq!(v["padded_same_transitions"], "0");
    assert_ne!(padded, lines);
}

/// FUN_004422b8 on the puzzle LCG.
#[test]
fn ogre_queen_placement() {
    let cases = [
        (0, 6, 4, 43930),
        (1, 4, 6, 14011),
        (1234, 6, 4, 15577),
        (32767, 6, 4, 15477),
        (32768, 5, 5, 13800),
        (48879, 5, 5, 35852),
    ];
    for (seed, x, y, after) in cases {
        let mut s = seed as u16;
        assert_eq!(ogre::queen_cell(&mut s), (x, y), "seed {seed}");
        assert_eq!(s, after as u16, "seed {seed}");
    }
}

// The ogre game: the drawing.

/// The events of the drawing, as the golden file writes them.
#[derive(Default)]
struct OgreRecorder {
    ev: Vec<String>,
    /// The input of the scripted games: x, y, click.
    input: VecDeque<(i32, i32, bool)>,
    rand_seed: u32,
    presents: usize,
    sounds: Vec<i32>,
}

impl OgreRecorder {
    fn line(&mut self, name: &str) -> String {
        let s = if self.ev.is_empty() {
            format!("{name}:")
        } else {
            format!("{name}: {}", self.ev.join(" | "))
        };
        self.ev.clear();
        s
    }
}

impl OgreIo for OgreRecorder {
    fn read_input(&mut self, _g: &OgreGame) -> (i32, i32, bool) {
        self.input.pop_front().unwrap_or((320, 240, false))
    }
    fn draw_background(&mut self) {
        self.ev.push("bg".into());
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.ev.push(format!("{bank} {x} {y} {frame}"));
    }
    fn draw_standard_cursor(&mut self, _x: i32, _y: i32) {
        self.ev.push("stdcursor".into());
    }
    fn set_cursor_pos(&mut self, x: i32, y: i32) {
        self.ev.push(format!("setcursorpos {x} {y}"));
    }
    async fn present(&mut self, _g: &OgreGame) {
        self.ev.push("present".into());
        self.presents += 1;
    }
    async fn wait_ticks(&mut self, _n: u32) {}
    fn play_sound(&mut self, n: i32) {
        self.ev.push(format!("sound cyclo{n}"));
        self.sounds.push(n);
    }
    fn rand32767(&mut self) -> i32 {
        atlantis_engine::puzzle::crt_rand_next(&mut self.rand_seed)
    }
    fn aborted(&self) -> bool {
        self.presents > 100_000
    }
}

fn set_records(g: &mut OgreGame, pieces: [(i32, i32); 4], hero_flag: i32) {
    let p = &mut g.rules.p;
    for (i, (x, y)) in pieces.into_iter().enumerate() {
        p[i] = Piece {
            flag: if i == 0 { hero_flag } else { i32::from(i >= 2) },
            x,
            y,
        };
    }
}

#[test]
fn ogre_drawing_vs_original() {
    let want = golden(include_str!("golden/ui_puzzle2_draw.txt"));
    let mut io = OgreRecorder::default();
    let mut out = Vec::new();
    let mut g = OgreGame::default();
    g.rules.path_x = [0; 3];
    g.rules.path_y = [0; 3];
    set_records(&mut g, [(0, 0), (4, 6), (6, 6), (0, 6)], 1);
    g.draw_board(&mut io, false, 0, 0);
    out.push(io.line("board idle"));
    set_records(&mut g, [(4, 6), (4, 6), (6, 6), (0, 6)], 1);
    g.draw_board(&mut io, false, 0, 0);
    out.push(io.line("board hero on queen"));
    set_records(&mut g, [(3, 3), (4, 6), (6, 6), (0, 6)], 0);
    for (x, y) in [
        (262, 210),
        (322, 210),
        (382, 210),
        (322, 270),
        (382, 270),
        (442, 330),
        (50, 50),
        (600, 450),
    ] {
        g.draw_board(&mut io, true, x, y);
        out.push(io.line(&format!("board held mouse {x},{y}")));
    }
    (g.grab_x, g.grab_y) = (20, 30);
    for (x, y) in [
        (300, 200),
        (100, 200),
        (600, 200),
        (300, 30),
        (300, 470),
        (50, 10),
    ] {
        (g.held, g.mx, g.my) = (true, x, y);
        g.draw_cursor(&mut io);
        out.push(io.line(&format!("cursor held mouse {x},{y}")));
        out.push(format!("  mouse after {},{}", g.mx, g.my));
    }
    for x in [49, 50, 300, 590, 591] {
        (g.held, g.mx, g.my) = (false, x, 200);
        g.draw_cursor(&mut io);
        out.push(io.line(&format!("cursor free mouse {x},200")));
    }
    set_records(&mut g, [(0, 0), (4, 6), (6, 6), (0, 6)], 1);
    block_on(g.reveal(&mut io));
    out.push(io.line("reveal"));
    set_records(&mut g, [(0, 0), (6, 6), (6, 6), (0, 6)], 1);
    block_on(g.reveal(&mut io));
    out.push(io.line("reveal on ogre"));
    set_records(&mut g, [(4, 6), (4, 6), (6, 6), (0, 6)], 1);
    g.rules.path_x = [3, 4, 4];
    g.rules.path_y = [6, 6, 6];
    block_on(g.win(&mut io));
    out.push(io.line("win"));
    set_records(&mut g, [(2, 2), (4, 6), (2, 3), (0, 6)], 1);
    block_on(g.lose(&mut io));
    out.push(io.line("lose"));
    // The golden's "mouse after" lines are indented: compare trimmed.
    let out: Vec<&str> = out.iter().map(|s| s.trim()).collect();
    let want: Vec<&str> = want.iter().map(|s| s.trim()).collect();
    assert_eq!(out.len(), want.len());
    for (o, w) in out.iter().zip(&want) {
        assert_eq!(o, w);
    }
}

/// FUN_00442360's slide frame (0x00442681..0x0044295e).
#[test]
fn ogre_slide_frame() {
    let mut io = OgreRecorder::default();
    let mut g = OgreGame::default();
    // Ogre A sliding, the hero on the queen's cell (no shift).
    set_records(&mut g, [(4, 6), (4, 6), (6, 6), (0, 6)], 1);
    block_on(g.slide_frame(&mut io, 2, 0x71 + 6 * 60 - 7, 0x1d + 6 * 60));
    assert_eq!(
        io.line("slide"),
        "slide: bg | P 353 389 0 | P 466 389 2 | P 113 389 2 | bg | present"
    );
    // Ogre B sliding; the hidden hero still drawn; the queen with flag 1.
    set_records(&mut g, [(1, 1), (2, 2), (3, 3), (0, 6)], 0);
    g.rules.p[1].flag = 1;
    block_on(g.slide_frame(&mut io, 3, 120, 380));
    assert_eq!(
        io.line("slide"),
        "slide: bg | P 173 89 0 | P 233 149 1 | P 293 209 2 | P 120 380 2 | bg | present"
    );
    // An ogre with flag 0 is not drawn; the sliding one always is.
    set_records(&mut g, [(1, 1), (2, 2), (3, 3), (0, 6)], 1);
    g.rules.p[3].flag = 0;
    block_on(g.slide_frame(&mut io, 2, 300, 200));
    assert_eq!(
        io.line("slide"),
        "slide: bg | P 173 89 0 | P 300 200 2 | bg | present"
    );
}

/// A whole game played through the game's loop: the hero picked up and
/// put down one cell to the right, then back, until an ogre catches him.
/// Each move: cyclo1, cyclo2, the two ogres' slides (cyclo3, 60 frames
/// each), and every fourth move the queen shown (cyclo4, 3 frames).
#[test]
fn ogre_game_lost() {
    // Seed 1: the queen at (4, 6).
    let mut seed = 1u16;
    let mut g = OgreGame::new(&mut seed);
    assert_eq!((g.rules.p[1].x, g.rules.p[1].y), (4, 6));
    let mut io = OgreRecorder {
        rand_seed: 1,
        ..OgreRecorder::default()
    };
    let mut hero = 0;
    for _ in 0..8 {
        let to = 1 - hero;
        // The click reads the position of the previous pass.
        let (hx, hy) = (ogre::piece_x(hero) + 20, ogre::piece_y(0) + 20);
        io.input.extend([(hx, hy, false), (hx, hy, true)]);
        let (tx, ty) = (CELL_PX[to as usize], CELL_PY[0]);
        io.input.extend([(tx, ty, false), (tx, ty, true)]);
        hero = to;
    }
    let r = block_on(g.run(&mut io));
    assert_eq!(r, 2);
    assert_eq!(io.sounds.last(), Some(&6));
    let moves = io.sounds.iter().filter(|&&s| s == 2).count();
    let slides = io.sounds.iter().filter(|&&s| s == 3).count();
    let reveals = io.sounds.iter().filter(|&&s| s == 4).count();
    eprintln!(
        "ogre game lost after {moves} moves, {} presents",
        io.presents
    );
    assert!(moves >= 2);
    // Ogre B does not move when ogre A made the catch.
    assert!(slides == 2 * moves || slides == 2 * moves - 1);
    // The passes: 4 a move (the first input is read before the loop, the
    // pass after the catch reads one more), the slides, the reveals and
    // the 10 frames of the loss.
    assert_eq!(io.presents, 4 * moves + 60 * slides + 3 * reveals + 10);
}

/// The ogre game in the engine: left at once (1), then played to its end
/// (won or lost, as the clock-seeded queen walks).
#[test]
fn engine_plays_ogre() {
    let Some(g) = game("engine_plays_ogre") else {
        return;
    };
    let mut e = Engine::new(g, 2, 16);
    for k in 0..40 {
        if e.result().is_some() {
            break;
        }
        e.pass(20, 240, k == 20, false);
    }
    assert_eq!(e.until_end(2000), Some(1));

    let g = game("engine_plays_ogre").unwrap();
    let mut e = Engine::new(g, 2, 16);
    let mut colours = 0;
    let mut hero = 0;
    let mut i = 0;
    while e.result().is_none() && i < 2000 {
        let to = 1 - hero;
        let (hx, hy) = (ogre::piece_x(hero) + 20, ogre::piece_y(0) + 20);
        let (tx, ty) = (CELL_PX[to as usize], CELL_PY[0]);
        for (x, y, c) in [
            (hx, hy, false),
            (hx, hy, true),
            (tx, ty, false),
            (tx, ty, true),
        ] {
            e.pass(x, y, c, false);
        }
        // The slides of the ogres.
        for _ in 0..200 {
            e.pass(tx, ty, false, false);
        }
        if i == 0 {
            colours = e.colours();
        }
        hero = to;
        i += 1;
    }
    assert!(colours > 100, "ogre: {colours} colours on screen");
    let r = e.until_end(2000).expect("the game did not end");
    eprintln!("ogre game in the engine: FinPuzzle {r} after {i} moves");
    assert!(r == 255 || r == 2, "ogre: {r}");
}

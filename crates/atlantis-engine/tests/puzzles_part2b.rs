//! The eclipse (0), the hens (1), the rotating pieces (3) and the snakes
//! (8) against the original `atlantis.exe` (2001 English build): their
//! rules over every reachable state, the eclipse's frame pixel for pixel,
//! then each puzzle played in the whole engine with scripted input.
//!
//! The expected values come from the original code run under emulation
//! (see the headers of the golden files); they hold numbers and digests
//! only. Ported from ScummVM's `test/engines/cryomni3d/atlantis/`
//! (`ui_puzzle_eclipse.h`, `ui_puzzle_hens.h`, `ui_puzzle_pieces.h`,
//! `ui_puzzle_snakes.h`).
//!
//! The tests that read the game (the click maps, the sprites, the engine
//! runs) need `ATLANTIS_DATA`, the game directory; without it they are
//! skipped.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use atlantis_data::sprite::{limits, read_spr_file, read_tga15, SprSprite};
use atlantis_data::sprite_blend::{SPR_NO_BLEND, SPR_TRANSPARENT};
use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::puzzle::eclipse::{Click as EclipseClick, EclipseGame, EclipseIo, RECORDS};
use atlantis_engine::puzzle::hens::{self, HensGame, HensIo};
use atlantis_engine::puzzle::pieces::{self, Click as PiecesClick, PiecesGame};
use atlantis_engine::puzzle::snakes::{self, SnakesGame, SnakesIo};
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

fn puzzle_file(g: &Game, name: &str) -> Vec<u8> {
    g.big
        .read(&format!("PUZZLES\\{name}"))
        .unwrap_or_else(|| panic!("PUZZLES\\{name}"))
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

fn assert_lines(out: &[String], want: &[&str]) {
    for (i, (o, w)) in out.iter().zip(want).enumerate() {
        assert_eq!(o, w, "line {i}");
    }
    assert_eq!(out.len(), want.len(), "lines");
}

// The eclipse (0).

/// The class of a click: E exit, a/b lever 1 up/down, c/d lever 2.
fn eclipse_class(x: i32, y: i32) -> char {
    let mut g = EclipseGame::default();
    let before = g.records;
    if g.click(x, y) == EclipseClick::Exit {
        return 'E';
    }
    let r = &g.records;
    if r[2].curr != before[2].curr {
        if r[2].curr == before[2].curr + 1 {
            'a'
        } else {
            'b'
        }
    } else if r[3].curr != before[3].curr {
        if r[3].curr == before[3].curr + 1 {
            'c'
        } else {
            'd'
        }
    } else {
        '.'
    }
}

#[test]
fn eclipse_rules_vs_original() {
    let want = golden(include_str!("golden/ui_puzzle0_eclipse.txt"));
    let mut out = Vec::new();
    // The table, as in the original's data (curr = min); the pad is not
    // the port's.
    for (i, r) in RECORDS.iter().enumerate() {
        let w = want
            .iter()
            .find(|l| l.starts_with(&format!("table.{i} ")))
            .unwrap();
        let fields: Vec<&str> = w[8..].split(',').collect();
        let got = format!("{},{},{},{},{},{}", r.dir, r.x, r.y, r.min, r.max, r.curr);
        assert_eq!(got, fields[..6].join(","), "record {i}");
    }
    // The start: curr = min + 0x25 for records 2..6.
    let g = EclipseGame::default();
    for (i, r) in g.records.iter().enumerate() {
        let exp = if i >= 2 { r.min + 0x25 } else { r.min };
        assert_eq!(r.curr, exp);
    }

    // The click classes of every pixel, row by row as runs.
    let mut all = String::new();
    let mut rows: Vec<String> = Vec::new();
    for y in 0..480 {
        let row: Vec<char> = (0..640).map(|x| eclipse_class(x, y)).collect();
        if y > 0 {
            all.push('\n');
        }
        all.extend(row.iter());
        let mut runs = Vec::new();
        let mut i = 0;
        while i < row.len() {
            let j = (i..row.len())
                .find(|&j| row[j] != row[i])
                .unwrap_or(row.len());
            runs.push(format!("{}{}", row[i], j - i));
            i = j;
        }
        rows.push(runs.join("_"));
    }
    let mut y = 0;
    while y < rows.len() {
        let end = (y..rows.len())
            .find(|&j| rows[j] != rows[y])
            .unwrap_or(rows.len());
        out.push(format!("hitrows.{y}-{} {}", end - 1, rows[y]));
        y = end;
    }
    out.push(format!("hitmap_md5 {}", md5_hex(all.as_bytes())));

    // Every lever state x the four lever clicks.
    let clicks = [(180, 400), (180, 450), (490, 400), (490, 450)];
    let mut lines = Vec::new();
    let mut wins = Vec::new();
    let offset = |r: &atlantis_engine::puzzle::eclipse::Record| {
        (i32::from(r.curr - r.min) - 0x25).rem_euclid(71)
    };
    for o1 in 0..71 {
        for o2 in 0..71 {
            for (k, &(x, y)) in clicks.iter().enumerate() {
                let mut g = EclipseGame::default();
                for i in 2..7 {
                    let r = &mut g.records[i];
                    let n = r.max - r.min + 1;
                    let off = if i == 2 || i == 5 || i == 6 { o1 } else { o2 };
                    r.curr = r.min + (0x25 + off) % n;
                }
                g.click(x, y);
                let r = &g.records;
                let (n1, n2) = (offset(&r[2]), offset(&r[3]));
                assert_eq!(offset(&r[5]), n1);
                assert_eq!(offset(&r[6]), n1);
                assert_eq!(offset(&r[4]), n2);
                let w = i32::from(g.is_win());
                let c = b"abcd"[k] as char;
                lines.push(format!("{o1} {o2} {c} {n1} {n2} {w}"));
                if w == 1 {
                    let currs: Vec<String> = r.iter().map(|r| r.curr.to_string()).collect();
                    wins.push(format!("win.{o1}.{o2}.{c} {}", currs.join(",")));
                }
            }
        }
    }
    out.push(format!(
        "transitions_md5 {}",
        md5_hex(lines.join("\n").as_bytes())
    ));
    out.push(format!("transitions_count {}", lines.len()));
    out.extend(wins);

    // The winning state: lever 1 up 34 times, lever 2 up 35 times.
    let mut g = EclipseGame::default();
    for _ in 0..34 {
        g.click(180, 400);
    }
    assert!(!g.is_win());
    for _ in 0..35 {
        g.click(490, 400);
    }
    assert!(g.is_win());
    out.push("winstate 34 35".into());

    let want: Vec<&str> = want
        .into_iter()
        .filter(|l| !l.starts_with("table."))
        .collect();
    assert_lines(&out, &want);
}

#[test]
fn eclipse_exit_zones() {
    let over = EclipseGame::over_exit;
    assert!(over(0, 0));
    assert!(over(0x9f, 0xdb));
    assert!(!over(0xa0, 0));
    assert!(!over(0, 0xdc));
    assert!(!over(0x21c, 10));
    assert!(over(0x21d, 10));
    assert!(over(0x27f, 0xdb));
    assert!(!over(0x280, 10));
    assert!(!over(320, 100));
}

/// The eclipse drawn into a 640x480 buffer.
struct EclipseCanvas {
    bg: Vec<u16>,
    pz1l: Vec<SprSprite>,
    pz2l: Vec<SprSprite>,
    buf: Vec<u16>,
}

impl EclipseIo for EclipseCanvas {
    fn read_input(&mut self) -> (i32, i32, bool) {
        (320, 240, false)
    }
    fn draw_background(&mut self) {
        self.buf.copy_from_slice(&self.bg);
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        let bank = if bank == '1' { &self.pz1l } else { &self.pz2l };
        // The golden's harness ran the blitter without the blend tables
        // that FUN_0042b930 builds at start-up (all zero), so an edge
        // pixel came out as its own colour term, without the background's.
        let mut spr = bank[frame as usize].clone();
        for b in spr.blend.iter_mut().filter(|b| **b != SPR_TRANSPARENT) {
            *b = SPR_NO_BLEND;
        }
        spr.draw(&mut self.buf, 640, 480, x, y);
    }
    async fn present(&mut self, _g: &EclipseGame) {}
    fn play_sound(&mut self, _n: i32) {}
    fn stop_sounds(&mut self) {}
}

fn words_md5(w: &[u16]) -> String {
    let b: Vec<u8> = w.iter().flat_map(|v| v.to_le_bytes()).collect();
    md5_hex(&b)
}

#[test]
fn eclipse_render_vs_original() {
    let Some(data) = game("eclipse_render_vs_original") else {
        return;
    };
    let (w, h) = limits::DEFAULT;
    let mut c = EclipseCanvas {
        bg: read_tga15(&puzzle_file(&data, "PZLION.TGA"))
            .unwrap()
            .pixels,
        pz1l: read_spr_file(&puzzle_file(&data, "PZ1L.SPR"), w, h).unwrap(),
        pz2l: read_spr_file(&puzzle_file(&data, "PZ2L.SPR"), w, h).unwrap(),
        buf: vec![0; 640 * 480],
    };
    let mut out = Vec::new();
    // The .data records (curr = min).
    let mut g = EclipseGame {
        records: RECORDS,
        frame_counter: 0,
    };
    g.render(&mut c);
    out.push(format!("frame_initial_md5 {}", words_md5(&c.buf)));
    let mut g = EclipseGame {
        records: RECORDS,
        frame_counter: 2,
    };
    g.records[4].curr = g.records[4].min + 1;
    g.records[5].curr = g.records[5].min;
    g.render(&mut c);
    out.push(format!("win_highlight_md5 {}", words_md5(&c.buf)));
    for f in [0, 71, 142, 0xd5, 0xd6] {
        let mut buf = vec![0u16; 640 * 480];
        c.pz1l[f].draw(&mut buf, 640, 480, 320, 240);
        let (mut x0, mut y0, mut x1, mut y1) = (640, 480, -1, -1);
        for (i, &v) in buf.iter().enumerate() {
            if v != 0 {
                let (x, y) = ((i % 640) as i32, (i / 640) as i32);
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        out.push(format!("pz1l_{f}_box {x0},{y0},{x1},{y1}"));
        out.push(format!("pz1l_{f}_md5 {}", words_md5(&buf)));
    }
    assert_lines(
        &out,
        &golden(include_str!("golden/ui_puzzle0_eclipse_render.txt")),
    );
}

// The hens (1).

/// The I/O of the rules tests: the sounds played, nothing drawn.
#[derive(Default)]
struct HensSounds(Vec<hens::Sound>);

impl HensIo for HensSounds {
    fn read_input(&mut self) -> (i32, i32, bool) {
        (0, 0, false)
    }
    fn draw_background(&mut self) {}
    fn draw_sprite(&mut self, _bank: char, _x: i32, _y: i32, _frame: i32) {}
    async fn present(&mut self, _g: &HensGame) {}
    fn play_sound(&mut self, s: hens::Sound) {
        self.0.push(s);
    }
}

fn hens_repr(g: &HensGame) -> String {
    let mut s = g.carried.to_string();
    for n in 2..9 {
        s += &format!(" {}", g.hens[n].target);
    }
    s
}

fn hens_key(g: &HensGame) -> u32 {
    let mut k = g.carried as u32;
    for n in 2..9 {
        k = k * 16 + g.hens[n].target as u32;
    }
    k
}

fn hens_from_key(mut k: u32) -> HensGame {
    let mut g = HensGame::default();
    for n in (2..9).rev() {
        g.hens[n].target = (k & 15) as i16;
        k >>= 4;
    }
    g.carried = k as i16;
    g
}

fn hens_sound_name(s: hens::Sound) -> &'static str {
    match s {
        hens::Sound::Pick => "pick",
        hens::Sound::Home => "home",
        hens::Sound::Drop => "drop",
        hens::Sound::Even => "none",
        hens::Sound::Win => "win",
        hens::Sound::Lose => "lose",
    }
}

/// One click from a state, as the harness prints it; every click starts
/// from the start records with only the state changed.
fn hens_transition(key: u32, name: &str, x: i32, y: i32) -> (String, HensGame, Vec<hens::Sound>) {
    let mut g = hens_from_key(key);
    let from = hens_repr(&g);
    let mut io = HensSounds::default();
    let ret = block_on(g.click(&mut io, x, y));
    let mut sounds = io.0.first().map_or("", |&s| hens_sound_name(s)).to_string();
    if let Some(&s) = io.0.get(1) {
        sounds += ",";
        sounds += hens_sound_name(s);
    }
    let line = format!(
        "{from} {name} -> {} {ret} {} {} {} {sounds}",
        hens_repr(&g),
        i32::from(g.leave),
        g.won,
        g.marked
    );
    (line, g, io.0)
}

#[test]
fn hens_vs_original() {
    let want = golden(include_str!("golden/ui_puzzle1_hens.txt"));
    let mut out = Vec::new();
    let start = HensGame::default();
    out.push("start records (target, weight, x, y, tilt):".to_string());
    for (n, h) in start.hens.iter().enumerate() {
        out.push(format!(
            "{n} ({}, {}, {}, {}, {})",
            h.target, h.weight, h.home_x, h.home_y, h.tilt
        ));
    }
    let mut tl = "targets".to_string();
    let mut actions: Vec<(String, i32, i32)> = Vec::new();
    for t in 1..hens::NUM_TARGETS {
        let (x, y) = HensGame::target_pos(t);
        tl += &format!(" {t}=({x}, {y})");
        actions.push((format!("t{t}"), x, y));
    }
    out.push(tl);
    for n in 2..9 {
        let h = &start.hens[n];
        actions.push((format!("home{n}"), h.home_x.into(), h.home_y.into()));
    }
    for (name, x, y) in [("else", 450, 150), ("b1", 356, 360), ("b2", 420, 360)] {
        actions.push((name.to_string(), x, y));
    }
    let mut al = "actions".to_string();
    for (name, x, y) in &actions {
        al += &format!(" {name}=({x}, {y})");
    }
    out.push(al);

    // Breadth first over (carried, targets of hens 2..8).
    let mut queue = vec![hens_key(&start)];
    let mut seen: HashSet<u32> = queue.iter().copied().collect();
    let mut weigh = Vec::new();
    let mut transitions = 0;
    let mut qi = 0;
    while qi < queue.len() {
        let cur = queue[qi];
        qi += 1;
        for (name, x, y) in &actions {
            let (line, g, snd) = hens_transition(cur, name, *x, *y);
            transitions += 1;
            if (name == "b1" || name == "b2")
                && matches!(
                    snd.first(),
                    Some(hens::Sound::Even | hens::Sound::Win | hens::Sound::Lose)
                )
            {
                weigh.push(line);
            }
            let k = hens_key(&g);
            if seen.insert(k) {
                queue.push(k);
            }
        }
    }
    out.push(format!("states {} transitions {transitions}", queue.len()));

    // The listing: the states in the order of their reprs, each state's
    // lines sorted.
    let mut reprs: Vec<(String, u32)> = queue
        .iter()
        .map(|&k| (hens_repr(&hens_from_key(k)), k))
        .collect();
    reprs.sort();
    let mut md5 = Md5::new();
    let mut first = true;
    for (_, k) in &reprs {
        let mut lines: Vec<String> = actions
            .iter()
            .map(|(name, x, y)| hens_transition(*k, name, *x, *y).0)
            .collect();
        lines.sort();
        for l in lines {
            if !first {
                md5.update(b"\n");
            }
            first = false;
            md5.update(l.as_bytes());
        }
    }
    let digest: String = md5.finalize().iter().map(|b| format!("{b:02x}")).collect();
    out.push(format!("transitions md5 {digest}"));

    weigh.sort();
    let count = |s: &str| weigh.iter().filter(|l| l.ends_with(s)).count();
    let mut wl = "weighings".to_string();
    for (label, suffix) in [("lose", " lose"), ("none", " none"), ("win", " win")] {
        let n = count(suffix);
        if n > 0 {
            wl += &format!(" {label}={n}");
        }
    }
    out.push(wl);
    out.push(format!(
        "weighings md5 {}",
        md5_hex(weigh.join("\n").as_bytes())
    ));
    out.extend(
        weigh
            .iter()
            .filter(|l| l.ends_with(" win"))
            .take(20)
            .cloned(),
    );
    assert_lines(&out, &want);
}

/// The only solution (hen n on target n, second button) weighed four
/// times solves the puzzle; a corner click asks to leave.
#[test]
fn hens_solve_and_leave() {
    let mut g = HensGame::default();
    for n in 2..9 {
        g.hens[n].target = n as i16;
    }
    for _ in 0..4 {
        assert!(!g.solved());
        let mut io = HensSounds::default();
        block_on(g.click(&mut io, 420, 360));
        assert_eq!(io.0, [hens::Sound::Win]);
    }
    assert!(g.solved());
    assert_eq!(g.marks[3].x, 322);
    assert!(!g.leave);
    block_on(g.click(&mut HensSounds::default(), 10, 10));
    assert!(g.leave);
}

/// The drop of mark 0: its x on the frames where its y is 397..412.
#[derive(Default)]
struct HensDrop {
    sounds: Vec<hens::Sound>,
    xs: Vec<String>,
}

impl HensIo for HensDrop {
    fn read_input(&mut self) -> (i32, i32, bool) {
        (0, 0, false)
    }
    fn draw_background(&mut self) {}
    fn draw_sprite(&mut self, _bank: char, _x: i32, _y: i32, _frame: i32) {}
    async fn present(&mut self, g: &HensGame) {
        if (397..=412).contains(&g.marks[0].y) {
            self.xs.push(g.marks[0].x.to_string());
        }
    }
    fn play_sound(&mut self, s: hens::Sound) {
        self.sounds.push(s);
    }
}

/// The marks across two runs: the set-up writes only their first short,
/// so a second run keeps the last run's x / y, and the drop of the next
/// won weighing moves mark 0 from there.
#[test]
fn hens_marks_carry_over() {
    let want = golden(include_str!("golden/ui_puzzle1_hens_marks.txt"));
    let mut g = HensGame::default();
    for (i, m) in g.marks.iter_mut().enumerate() {
        *m = hens::Mark {
            shown: 1,
            x: 262 + 20 * i as i16,
            y: 457,
        };
    }
    g.reset();
    let marks: Vec<String> = g
        .marks
        .iter()
        .map(|m| format!("({},{},{})", m.shown, m.x, m.y))
        .collect();
    for n in 2..9 {
        g.hens[n].target = n as i16;
    }
    let mut io = HensDrop::default();
    block_on(g.click(&mut io, 420, 360));
    assert_eq!(io.sounds, [hens::Sound::Win]);
    let out = [
        format!("second run marks: {}", marks.join(" ")),
        "second run weighing at (420, 360): win".to_string(),
        format!("second run drop x: {}", io.xs.join(" ")),
    ];
    assert_lines(&out, &want);
}

/// The clicks that solve the hens from the start: each hen picked at
/// home and put on its target, then the second button four times.
fn hens_solution() -> Vec<(i32, i32)> {
    let start = HensGame::default();
    let mut clicks = Vec::new();
    for n in 2..9 {
        let h = &start.hens[n];
        clicks.push((i32::from(h.home_x), i32::from(h.home_y)));
        clicks.push(HensGame::target_pos(n));
    }
    clicks.extend([(420, 360); 4]);
    clicks
}

#[test]
fn hens_solution_by_clicks() {
    let mut g = HensGame::default();
    for (x, y) in hens_solution() {
        block_on(g.click(&mut HensSounds::default(), x, y));
    }
    assert!(g.solved());
    assert!(!g.leave);
}

// The rotating pieces (3).

fn tuple6(v: &[i16; 6]) -> String {
    format!(
        "({}, {}, {}, {}, {}, {})",
        v[0], v[1], v[2], v[3], v[4], v[5]
    )
}

fn pieces_repr(g: &PiecesGame) -> String {
    format!("({}, {})", tuple6(&g.pieces), tuple6(&g.counters))
}

/// The representative point of each move value 1..12, and of the reset.
/// A click point.
type Point = (i32, i32);

fn pieces_points(map: &[u8]) -> (Vec<Option<Point>>, Point) {
    let mut rep = vec![None; 13];
    let mut reset = None;
    for y in 0..480 {
        for x in 0..640 {
            let v = usize::from(pieces::map_value(map, x, y, pieces::CLICK_BASE));
            if (1..=12).contains(&v) && rep[v].is_none() {
                rep[v] = Some((x, y));
            }
            if v == 13 && reset.is_none() {
                reset = Some((x, y));
            }
        }
    }
    (rep, reset.unwrap())
}

#[test]
fn pieces_vs_original() {
    let Some(data) = game("pieces_vs_original") else {
        return;
    };
    let map = puzzle_file(&data, "PUZSRP2.BMP");
    let want = golden(include_str!("golden/ui_puzzle3_pieces.txt"));
    let mut out = Vec::new();
    let start = PiecesGame::new(map.clone());
    out.push(format!(
        "start pieces {} counters {}",
        tuple6(&start.pieces),
        tuple6(&start.counters)
    ));

    // The click map from the start state.
    let mut all = String::new();
    let mut counts = [0u32; 128];
    let mut rep = [None; 13];
    for y in 0..480 {
        if y > 0 {
            all.push('\n');
        }
        for x in 0..640 {
            let mut g = start.clone();
            let c = match g.click(x, y) {
                PiecesClick::Leave => 'L',
                PiecesClick::Reset => 'R',
                PiecesClick::Board => 'k',
                PiecesClick::Move => {
                    let v = pieces::map_value(&map, x, y, pieces::CLICK_BASE);
                    if rep[usize::from(v)].is_none() {
                        rep[usize::from(v)] = Some((x, y));
                    }
                    (b'a' + v - 1) as char
                }
                PiecesClick::None => '.',
            };
            counts[c as usize] += 1;
            all.push(c);
        }
    }
    out.push(format!("clickmap md5 {}", md5_hex(all.as_bytes())));
    let mut cl = "clickmap counts".to_string();
    for (c, &n) in counts.iter().enumerate() {
        if n > 0 {
            cl += &format!(" {}={n}", c as u8 as char);
        }
    }
    out.push(cl);
    for (v, r) in rep.iter().enumerate().skip(1) {
        if let Some((x, y)) = r {
            out.push(format!("move v={v} at ({x},{y})"));
        }
    }

    // Every state reachable by the moves and the reset.
    let (_, reset) = pieces_points(&map);
    let mut queue = vec![start.clone()];
    let mut seen: HashSet<String> = HashSet::from([pieces_repr(&start)]);
    let mut lines = Vec::new();
    let mut wins = 0;
    let mut qi = 0;
    while qi < queue.len() {
        let cur = queue[qi].clone();
        qi += 1;
        let moves = rep.iter().enumerate().skip(1);
        let clicks = moves
            .filter_map(|(a, p)| p.map(|p| (p, format!("m{a}"))))
            .chain([(reset, "reset".to_string())]);
        for ((x, y), name) in clicks {
            // The harness clears the cursor animation before each click.
            let mut g = cur.clone();
            g.cursor = Default::default();
            let c = g.click(x, y);
            let snd = i32::from(matches!(
                c,
                PiecesClick::Move | PiecesClick::Reset | PiecesClick::Board
            ));
            let win = i32::from(g.solved());
            let res = if c == PiecesClick::Leave { 2 } else { 0 };
            lines.push(format!(
                "{} {name} -> {} r={res} win={win} snd={snd}",
                pieces_repr(&cur),
                pieces_repr(&g)
            ));
            if seen.insert(pieces_repr(&g)) {
                wins += win;
                queue.push(g);
            }
        }
    }
    lines.sort();
    out.push(format!(
        "states {} transitions {} winning_states {wins}",
        queue.len(),
        lines.len()
    ));
    out.push(format!(
        "transitions md5 {}",
        md5_hex(lines.join("\n").as_bytes())
    ));
    assert_lines(&out, &want);
}

/// The shortest click sequence that solves the rotating pieces.
fn pieces_solution(map: &[u8]) -> Vec<(i32, i32)> {
    let (rep, _) = pieces_points(map);
    let start = PiecesGame::new(map.to_vec());
    type Key = ([i16; 6], [i16; 6]);
    let mut prev: HashMap<Key, (Key, Point)> = HashMap::new();
    let key = |g: &PiecesGame| (g.pieces, g.counters);
    let mut q = VecDeque::from([start.clone()]);
    let mut seen = HashSet::from([key(&start)]);
    while let Some(cur) = q.pop_front() {
        if cur.solved() {
            let mut path = Vec::new();
            let mut k = key(&cur);
            while let Some(&(p, click)) = prev.get(&k) {
                path.push(click);
                k = p;
            }
            path.reverse();
            return path;
        }
        for p in rep.iter().flatten() {
            let mut g = cur.clone();
            g.cursor = Default::default();
            if g.click(p.0, p.1) == PiecesClick::Move && seen.insert(key(&g)) {
                prev.insert(key(&g), (key(&cur), *p));
                q.push_back(g);
            }
        }
    }
    panic!("no solution");
}

// The snakes (8).

fn snake_repr(s: &snakes::Snake) -> String {
    let d: String = s.dirs[..usize::from(s.len)]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{},{},{},{d}", s.x, s.y, s.len)
}

fn snakes_repr(g: &SnakesGame) -> String {
    g.snakes
        .iter()
        .map(snake_repr)
        .collect::<Vec<_>>()
        .join(" | ")
}

fn tuple(v: &[i32]) -> String {
    format!(
        "({})",
        v.iter().map(i32::to_string).collect::<Vec<_>>().join(", ")
    )
}

fn draws_repr(g: &SnakesGame) -> String {
    g.draws()
        .iter()
        .map(|d| format!("{},{},{}", d.x, d.y, d.frame))
        .collect::<Vec<_>>()
        .join(" ")
}

fn snakes_won(g: &mut SnakesGame, x: i32, y: i32) -> i32 {
    g.result = 0;
    g.frame(x, y);
    g.result
}

const OFFSETS: [(i32, i32); 4] = [(8, 0), (-8, 0), (0, -8), (0, 8)];

fn place_all(
    k: usize,
    cur: &SnakesGame,
    chosen: &mut [i32; 4],
    out: &mut Vec<([i32; 4], SnakesGame)>,
) {
    if k == 4 {
        out.push((*chosen, cur.clone()));
        return;
    }
    for h in 0..snakes::NUM_HOLES {
        if cur.holes[h].used != 0 {
            continue;
        }
        let mut g = cur.clone();
        g.place(g.pieces[k].x, g.pieces[k].y);
        g.place(g.holes[h].x, g.holes[h].y);
        if g.pieces[k].placed == 0 {
            continue;
        }
        chosen[k] = h as i32;
        place_all(k + 1, &g, chosen, out);
    }
}

/// The solution of the golden: its placement and steering clicks.
fn snakes_solution(want: &[&str]) -> ([i32; 4], Vec<(usize, usize)>) {
    let l = want
        .iter()
        .find(|l| l.starts_with("solved placement "))
        .unwrap();
    let open = l.find('(').unwrap();
    let close = l.find(')').unwrap();
    let h: Vec<i32> = l[open + 1..close]
        .split(", ")
        .map(|v| v.parse().unwrap())
        .collect();
    let clicks = l[l.find("clicks ").unwrap() + 7..]
        .split(' ')
        .map(|c| {
            let b = c.as_bytes();
            (usize::from(b[1] - b'0'), usize::from(b[3] - b'0'))
        })
        .collect();
    ([h[0], h[1], h[2], h[3]], clicks)
}

#[test]
fn snakes_vs_original() {
    let Some(data) = game("snakes_vs_original") else {
        return;
    };
    let want = golden(include_str!("golden/ui_puzzle8_snakes.txt"));
    let map = snakes::decode_map(&puzzle_file(&data, "CHEMIN00.PCX")).unwrap();
    let mut out = Vec::new();
    out.push(format!("map md5 {}", md5_hex(&map)));
    let mut hist = [0u32; 256];
    for &v in &map {
        hist[usize::from(v)] += 1;
    }
    let mut hl = "map values".to_string();
    for (v, &n) in hist.iter().enumerate() {
        if n > 0 {
            hl += &format!(" {v}={n}");
        }
    }
    out.push(hl);
    let start = SnakesGame::new(map, Default::default());
    let mut holes = "holes (used, x, y, partner, dir)".to_string();
    for o in &start.holes {
        holes += " ";
        holes += &tuple(&[o.used.into(), o.x, o.y, o.partner.into(), o.dir.into()]);
    }
    out.push(holes);
    for (cx, cy) in [(0, 0), (5, 0), (0, 5)] {
        let mut g = start.clone();
        g.place(40, 448);
        out.push(format!(
            "carrying piece 0 at ({cx}, {cy}): result {}",
            snakes_won(&mut g, cx, cy)
        ));
    }
    for x in [29, 30, 620, 621] {
        let mut g = start.clone();
        g.place(x, 448);
        out.push(format!(
            "placement click at ({x}, 448): result {}",
            g.result
        ));
    }

    let mut placements = Vec::new();
    place_all(0, &start, &mut [0; 4], &mut placements);
    out.push(format!("placements {}", placements.len()));
    let mut total = 0;
    for (pi, (holes, state)) in placements.iter().enumerate() {
        let mut g = state.clone();
        let st0 = snakes_repr(&g);
        let mut x = pi as u32 + 1;
        let mut lines = st0.clone();
        let mut moved = 0;
        let (mut mx, mut my) = (0, 0);
        for _ in 0..400 {
            x = x.wrapping_mul(1103515245).wrapping_add(12345) & 0x7fffffff;
            let k = ((x >> 16) % 4) as usize;
            let di = ((x >> 8) % 4) as usize;
            let before = snakes_repr(&g);
            mx = g.snakes[k].x + OFFSETS[di].0;
            my = g.snakes[k].y + OFFSETS[di].1;
            let snd = g.steer(mx, my);
            let after = snakes_repr(&g);
            if after != before {
                moved += 1;
            }
            lines += &format!(
                "\ns{k}d{di} -> {after} {}",
                i32::from(snd != snakes::Sound::None)
            );
        }
        let win = snakes_won(&mut g.clone(), mx, my);
        total += moved;
        out.push(format!(
            "placement {} start {st0} end {} moves {moved} won {win} md5 {} draws {}",
            tuple(holes),
            snakes_repr(&g),
            md5_hex(lines.as_bytes()),
            md5_hex(draws_repr(&g).as_bytes())
        ));
    }
    out.push(format!("total moves {total}"));

    // The solution, replayed.
    let (h, clicks) = snakes_solution(&want);
    let mut g = placements.iter().find(|p| p.0 == h).unwrap().1.clone();
    let mut line = format!("solved placement {} clicks", tuple(&h));
    let (mut mx, mut my) = (0, 0);
    for &(k, di) in &clicks {
        mx = g.snakes[k].x + OFFSETS[di].0;
        my = g.snakes[k].y + OFFSETS[di].1;
        g.steer(mx, my);
        line += &format!(" s{k}d{di}");
    }
    out.push(line);
    out.push(format!(
        "solved end {} won {}",
        snakes_repr(&g),
        snakes_won(&mut g.clone(), mx, my)
    ));
    out.push(format!("solved draws {}", draws_repr(&g)));
    assert_lines(&out, &want);
}

/// The snakes' run with scripted clicks, one a pass, logging what it does.
struct SnakesLog {
    clicks: VecDeque<(i32, i32)>,
    log: Vec<String>,
}

impl SnakesIo for SnakesLog {
    fn read_input(&mut self) -> (i32, i32, bool) {
        self.log.push("read".to_string());
        match self.clicks.pop_front() {
            Some((x, y)) => (x, y, true),
            None => (320, 240, false),
        }
    }
    fn draw_background(&mut self) {
        self.log.push("bg".to_string());
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        self.log.push(format!("{bank} {x} {y} {frame}"));
    }
    async fn present(&mut self, _g: &SnakesGame) {
        self.log.push("present".to_string());
    }
    fn play_sound(&mut self, s: snakes::Sound) {
        self.log.push(format!("sound {s:?}"));
    }
    async fn wait_ticks(&mut self, _n: u32) {}
    fn aborted(&self) -> bool {
        self.log.len() > 100_000
    }
}

/// The exits of `FUN_0043b0f4`: a phase-1 leave goes through the phase-2
/// loop head, which steers a press taken by the last poll before the
/// result is returned; a win is followed by the cursor drawn onto the
/// win animation's last step and one more present.
#[test]
fn snakes_exits_as_original() {
    let Some(data) = game("snakes_exits_as_original") else {
        return;
    };
    let want = golden(include_str!("golden/ui_puzzle8_snakes.txt"));
    let map = snakes::decode_map(&puzzle_file(&data, "CHEMIN00.PCX")).unwrap();
    let start = SnakesGame::new(map, Default::default());

    // Phase 1: a click at the edge leaves after the frame; the poll after
    // it takes a press on snake 0, which is steered.
    let mut g = start.clone();
    let (sx, sy) = (g.snakes[0].x + OFFSETS[0].0, g.snakes[0].y + OFFSETS[0].1);
    let mut steered = start.clone();
    steered.result = 1;
    let snd = steered.steer(sx, sy);
    let mut io = SnakesLog {
        clicks: VecDeque::from([(10, 240), (sx, sy)]),
        log: Vec::new(),
    };
    assert_eq!(block_on(g.run(&mut io)), 1);
    assert_eq!(io.log.iter().filter(|l| *l == "read").count(), 2);
    assert_eq!(g.snakes, steered.snakes);
    if snd != snakes::Sound::None {
        assert_eq!(io.log.last().unwrap(), &format!("sound {snd:?}"));
    }

    // The win: after the 20 steps of the animation, the cursor alone and a
    // present.
    let (h, clicks) = snakes_solution(&want);
    let mut placements = Vec::new();
    place_all(0, &start, &mut [0; 4], &mut placements);
    let mut g = placements.iter().find(|p| p.0 == h).unwrap().1.clone();
    let mut sim = g.clone();
    let mut xy = VecDeque::new();
    for &(k, di) in &clicks {
        let (x, y) = (
            sim.snakes[k].x + OFFSETS[di].0,
            sim.snakes[k].y + OFFSETS[di].1,
        );
        sim.steer(x, y);
        xy.push_back((x, y));
    }
    let (lx, ly) = *xy.back().unwrap();
    let mut io = SnakesLog {
        clicks: xy,
        log: Vec::new(),
    };
    assert_eq!(block_on(g.run(&mut io)), 255);
    assert_eq!(io.log.iter().filter(|l| *l == "read").count(), clicks.len());
    let last_read = io.log.iter().rposition(|l| l == "read").unwrap();
    let tail = &io.log[last_read..];
    assert_eq!(tail.iter().filter(|l| *l == "present").count(), 21);
    let n = tail.len();
    let edge = if snakes::at_edge(lx) { 11 } else { 6 };
    assert_eq!(
        tail[n - 3..],
        [
            "present".to_string(),
            format!("C {lx} {ly} {edge}"),
            "present".to_string()
        ]
    );
}

// The puzzles in the whole engine.

/// The engine started on StartPuzzle `id` ([`Start::Puzzle`]), driven one
/// frame a step, the sound mixed.
struct Engine {
    sys: Rc<Sys>,
    runner: Runner,
    trace: Rc<RefCell<Vec<Event>>>,
    step: u64,
    left: bool,
}

impl Engine {
    fn new(g: Game, id: i32) -> Engine {
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
            left: false,
        }
    }

    fn result(&self) -> Option<i32> {
        self.trace.borrow().iter().find_map(|e| match e {
            Event::Puzzle { result, .. } => Some(*result),
            _ => None,
        })
    }

    /// One frame with the mouse at (x, y), and a click.
    fn pass(&mut self, x: i32, y: i32, click: bool) {
        let s = &self.sys;
        s.push_input(Input::MouseMove { x, y });
        if self.left {
            s.push_input(Input::MouseUp {
                button: MouseButton::Left,
                x,
                y,
            });
            self.left = false;
        }
        if click {
            s.push_input(Input::MouseDown {
                button: MouseButton::Left,
                x,
                y,
            });
            self.left = true;
        }
        assert!(self.runner.step(self.step * 16), "the engine ended");
        let mut sound = vec![0.0f32; 2 * (22050 * 16 / 1000)];
        self.sys.mixer.fill(&mut sound, 22050, 2);
        self.step += 1;
    }

    /// A click at (x, y), then `idle` frames with the mouse there.
    fn click(&mut self, x: i32, y: i32, idle: usize) {
        self.pass(x, y, true);
        for _ in 0..idle {
            self.pass(x, y, false);
        }
    }

    /// Frames with the mouse still until the puzzle ends.
    fn until_end(&mut self, x: i32, y: i32, max: usize) -> Option<i32> {
        for _ in 0..max {
            if let Some(r) = self.result() {
                return Some(r);
            }
            self.pass(x, y, false);
        }
        self.result()
    }

    /// The colours on the screen.
    fn colours(&self) -> usize {
        self.sys
            .screen
            .borrow()
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }
}

/// The engine on puzzle `id`, with its picture on the screen.
fn started(g: Game, id: i32) -> Engine {
    let mut e = Engine::new(g, id);
    for _ in 0..5 {
        e.pass(320, 300, false);
    }
    assert!(e.result().is_none(), "puzzle {id} ended at once");
    assert!(e.colours() > 100, "puzzle {id}: {} colours", e.colours());
    e
}

#[test]
fn engine_plays_eclipse() {
    let Some(g) = game("engine_plays_eclipse") else {
        return;
    };
    let mut e = started(g, 0);
    for _ in 0..34 {
        e.click(180, 400, 1);
    }
    for _ in 0..34 {
        e.click(490, 400, 1);
    }
    assert_eq!(e.result(), None);
    e.click(490, 400, 0);
    // The win animation's 80 passes.
    assert_eq!(e.until_end(320, 300, 200), Some(255));
}

#[test]
fn engine_plays_hens() {
    let Some(g) = game("engine_plays_hens") else {
        return;
    };
    let mut e = started(g, 1);
    for (x, y) in hens_solution() {
        // A weighing animates for about 130 frames, without reading the
        // input.
        e.click(x, y, 200);
    }
    assert_eq!(e.until_end(320, 300, 10), Some(255));
}

#[test]
fn engine_plays_pieces() {
    let Some(g) = game("engine_plays_pieces") else {
        return;
    };
    let path = pieces_solution(&puzzle_file(&g, "PUZSRP2.BMP"));
    assert!(!path.is_empty());
    let mut e = started(g, 3);
    for &(x, y) in &path {
        assert_eq!(e.result(), None);
        e.click(x, y, 2);
    }
    assert_eq!(e.until_end(320, 300, 10), Some(255));
}

#[test]
fn engine_plays_snakes() {
    let Some(g) = game("engine_plays_snakes") else {
        return;
    };
    let want = golden(include_str!("golden/ui_puzzle8_snakes.txt"));
    let (holes, clicks) = snakes_solution(&want);
    let map = snakes::decode_map(&puzzle_file(&g, "CHEMIN00.PCX")).unwrap();
    // The rules alongside, for the heads' places.
    let mut r = SnakesGame::new(map, Default::default());
    let mut e = started(g, 8);
    for (k, &h) in holes.iter().enumerate() {
        let (px, py) = (r.pieces[k].x, r.pieces[k].y);
        r.place(px, py);
        e.click(px, py, 1);
        let (hx, hy) = (r.holes[h as usize].x, r.holes[h as usize].y);
        r.place(hx, hy);
        e.click(hx, hy, 1);
    }
    assert!(r.all_placed());
    for &(k, di) in &clicks {
        let (mx, my) = (r.snakes[k].x + OFFSETS[di].0, r.snakes[k].y + OFFSETS[di].1);
        r.steer(mx, my);
        e.click(mx, my, 1);
    }
    assert!(r.frame(320, 300));
    // The win animation: 20 steps of 500 ms.
    assert_eq!(e.until_end(320, 300, 1000), Some(255));
}

/// Leaving each puzzle with a click on its frame: 1 for the eclipse and
/// the snakes (the original returns `DAT_00496d0c` there), 2 for the
/// hens and the rotating pieces.
#[test]
fn engine_leaves_part2b() {
    if game("engine_leaves_part2b").is_none() {
        return;
    }
    for (id, (x, y), want) in [
        (0, (10, 10), 1),
        (1, (10, 10), 2),
        (3, (5, 5), 2),
        (8, (5, 240), 1),
    ] {
        let mut e = started(game("engine_leaves_part2b").unwrap(), id);
        e.click(x, y, 0);
        assert_eq!(e.until_end(x, y, 100), Some(want), "puzzle {id}");
    }
}

/// The original wins a first run when piece 0 is carried to (0, 0) (the
/// snakes not yet released hold the zeroed memory); the minor bug fixes
/// require every snake released.
#[test]
fn snakes_first_run_win_fixed_by_minor_bug_fixes() {
    let map = vec![0u8; 640 * 480];
    let mut g = SnakesGame::new(map.clone(), Default::default());
    g.place(g.pieces[0].x, g.pieces[0].y);
    assert_eq!(snakes_won(&mut g, 0, 0), 255);
    let mut f = SnakesGame::new(map, Default::default());
    f.fix_first_run_win = true;
    f.place(f.pieces[0].x, f.pieces[0].y);
    assert_eq!(snakes_won(&mut f, 0, 0), 0);
}

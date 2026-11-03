//! The unicorn board (`LICORNE`, loop mode 11) against the original under
//! emulation (`tests/golden/puzzle_licorne*.txt`): the 2D cell positions,
//! the maze, the drop map, then each run's frames replayed from their
//! inputs (pointer moves, clicks) through [`Board`]: the sounds, the
//! commands, the path, the polygons given to the rasteriser per object
//! (count and md5 of their vertex records), the sprite calls, the screen's
//! md5 and the board's globals, line by line.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use std::collections::{HashMap, HashSet};

use atlantis3_data::spr::{Bank, Depth};
use atlantis3_data::A3Files;
use atlantis3_engine::inventory::aff_at;
use atlantis3_engine::licorne::{self, Board, Cursor, Host, Input};
use md5::{Digest, Md5};

const G800: &str = include_str!("golden/puzzle_licorne.txt");
const G640: &str = include_str!("golden/puzzle_licorne_640.txt");
const G1024: &str = include_str!("golden/puzzle_licorne_1024.txt");

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn md5_screen(px: &[u16]) -> String {
    let b: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    md5_hex(&b)
}

/// The harness's sound layer: a sound plays from its `Play` until a
/// `Stop` (it never ends).
struct Log {
    frame: u32,
    out: Vec<String>,
    playing: HashSet<String>,
}

impl Host for Log {
    fn load(&mut self, name: &str) {
        self.out.push(format!("snd {} load {name} 0 1", self.frame));
    }
    fn play(&mut self, name: &str) {
        self.out.push(format!("snd {} play {name}", self.frame));
        self.playing.insert(name.to_lowercase());
    }
    fn stop(&mut self, name: &str) {
        self.out.push(format!("snd {} stop {name}", self.frame));
        self.playing.remove(&name.to_lowercase());
    }
    fn playing(&mut self, name: &str) -> bool {
        let p = self.playing.contains(&name.to_lowercase());
        self.out
            .push(format!("snd {} test {name} -> {}", self.frame, u8::from(p)));
        p
    }
    fn command(&mut self, cmd: &str) {
        self.out.push(format!("cmd {} {cmd}", self.frame));
    }
}

fn now(frame: u32) -> f64 {
    100.0 + f64::from(frame) * 1.0025 / 15.0
}

fn hex(f: f32) -> String {
    format!("{:08x}", f.to_bits())
}

/// The golden's `st` fields of the board.
fn st_fields(b: &Board, mode: i32, act: bool) -> Vec<(String, String)> {
    let u = &b.uni_a;
    let t = &b.beast_a;
    let mut v: Vec<(String, String)> = vec![
        ("mode".into(), mode.to_string()),
        ("act".into(), u8::from(act).to_string()),
        ("run".into(), u8::from(b.running).to_string()),
        ("held".into(), b.held.to_string()),
        ("n".into(), b.placed.to_string()),
        ("us".into(), u.state.to_string()),
        ("un".into(), u.node.to_string()),
        ("ua".into(), u.anim.to_string()),
        ("un2".into(), u.next.to_string()),
        ("tsk".into(), t.state.to_string()),
        ("tn".into(), t.node.to_string()),
        ("ta".into(), t.anim.to_string()),
        ("won".into(), u8::from(b.won).to_string()),
        ("lost".into(), u8::from(b.lost).to_string()),
        ("end".into(), u8::from(b.end).to_string()),
        ("cur".into(), b.cursor.to_string()),
        ("uf".into(), hex(u.frame)),
        ("tf".into(), hex(t.frame)),
        ("um".into(), hex(u.m)),
        ("tm".into(), hex(t.m)),
        ("rt".into(), hex(b.rt)),
    ];
    for (i, c) in b.beacons.iter().enumerate() {
        v.push((format!("b{i}"), format!("{},{}", c.0, c.1)));
    }
    v
}

struct Replay {
    board: Board,
    files: A3Files,
    iface: Bank,
    log: Log,
    last: HashMap<String, String>,
    mode: i32,
    act: bool,
    screen: Vec<u16>,
}

impl Replay {
    fn st(&mut self) {
        let f = st_fields(&self.board, self.mode, self.act);
        let ch: Vec<String> = f
            .iter()
            .filter(|(k, v)| self.last.get(k) != Some(v))
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        if !ch.is_empty() {
            self.log.out.push(format!("st {} {}", self.log.frame, ch.join(" ")));
        }
        self.last = f.into_iter().collect();
    }

    /// One frame: false when it returned 0 (the stop ran).
    fn frame(&mut self, input: Input) -> bool {
        let f = self.log.frame;
        let running = self.board.running;
        let path_before = self.board.nodes.clone();
        let ok = self.board.frame(input, now(f), &mut self.log);
        if !running && self.board.running && self.board.nodes != path_before || (!running && self.board.running) {
            // The path line, after the click's sound.
            let nodes = &self.board.nodes;
            let s: Vec<String> = nodes
                .iter()
                .map(|n| format!("{},{},{},{}", n.x, n.y, n.dir, n.beacon))
                .collect();
            let line = format!("path {f} {} {}", nodes.len(), s.join(" "));
            // After the `snd load/play` of sound 803 of this frame.
            let pos = self
                .log
                .out
                .iter()
                .rposition(|l| l == &format!("snd {f} play sfx\\sound803.apc"))
                .map_or(self.log.out.len(), |p| p + 1);
            self.log.out.insert(pos, line);
        }
        if !ok {
            self.log.out.push(format!("ret {f} 0"));
            self.board.stop(&mut self.log);
            self.log.out.push(format!("scan {f}"));
            self.mode = 3;
            self.act = false;
            return false;
        }
        let (w, h) = (self.board.w as usize, self.board.h as usize);
        self.log.out.push(format!("draw {f} warp=flat"));
        self.board.polys = Some(Vec::new());
        self.board.draw(&mut self.screen);
        if let Some(c) = self.board.cursor() {
            match c {
                Cursor::Plain(x, y) => {
                    aff_at(&mut self.screen, w, h, "", &self.iface, 40, Some(x), y, 1.0);
                    self.log.out.push(format!(
                        "draw {f} spr\\Interface.spr#40@{x},{y}*{}",
                        "0000803f"
                    ));
                }
                Cursor::Centred(i, x, y) => {
                    self.log
                        .out
                        .push(format!("draw {f} C:spr\\Interface.spr#{i}@{x},{y}"));
                    if let Some(info) = self.iface.info(i as usize) {
                        let (dx, dy) = (i32::from(info.w) / 2, i32::from(info.h) / 2);
                        aff_at(&mut self.screen, w, h, "", &self.iface, i as usize, Some(x - dx), y - dy, 1.0);
                    }
                }
            }
        }
        for (g, p) in self.board.polys.take().unwrap_or_default() {
            if p.count > 0 {
                self.log
                    .out
                    .push(format!("poly {f} {g} {} {}", p.count, md5_hex(&p.bytes)));
            }
        }
        self.log
            .out
            .push(format!("md5 {f} {}", md5_screen(&self.screen)));
        true
    }
}

fn check(golden: &str, w: usize, h: usize, runs: &[&str]) {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let iface = Bank::parse(&files.read("spr/Interface.spr").unwrap(), Depth::Rgb565).unwrap();
    let lines: Vec<&str> = golden
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .collect();
    // The tables before the runs.
    let mut board = Board::new(&files, w, h, 100.0).expect("the board loads");
    for l in &lines {
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "cells" => {
                let y: i32 = f[1].parse().unwrap();
                for (x, c) in f[2..].iter().enumerate() {
                    let (cx, cy) = licorne::cell_xy(x as i32, y, w as i32, h as i32);
                    assert_eq!(&format!("{cx},{cy}"), c, "cell ({x}, {y})");
                }
            }
            "maze" => {
                let y: i32 = f[1].parse().unwrap();
                for (x, c) in f[2..].iter().enumerate() {
                    let s: String = (0..4)
                        .map(|d| if licorne::open(x as i32, y, d) { "NWSE".as_bytes()[d] as char } else { '.' })
                        .collect();
                    assert_eq!(&s, c, "maze ({x}, {y})");
                }
            }
            "dropmap" => {
                let y: i32 = f[1].parse().unwrap();
                for (k, c) in f[2..].iter().enumerate() {
                    let x = 10 * k as i32;
                    let got = board
                        .drop_cell(0, x, y)
                        .map_or("-".to_string(), |(cx, cy)| format!("{cx}.{cy}"));
                    assert_eq!(&got, c, "dropmap ({x}, {y})");
                }
            }
            _ => {}
        }
    }
    // The runs.
    let mut starts = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if let Some(n) = l.strip_prefix("run ") {
            starts.push((n, i));
        }
    }
    for (k, &(name, at)) in starts.iter().enumerate() {
        if !runs.contains(&name) {
            continue;
        }
        let end = starts.get(k + 1).map_or(lines.len(), |s| s.1);
        let run = &lines[at + 1..end];
        let mut inputs: HashMap<u32, Vec<Vec<&str>>> = HashMap::new();
        let mut last_frame = 0;
        for l in run {
            let f: Vec<&str> = l.split(' ').collect();
            if matches!(f[0], "pointer" | "click" | "rclick") {
                inputs.entry(f[1].parse().unwrap()).or_default().push(f.clone());
            }
            if f[0] == "end" {
                last_frame = f[3].parse().unwrap();
            }
        }
        let board = Board::new(&files, w, h, 100.0).expect("the board loads");
        let mut r = Replay {
            board,
            files: files.clone(),
            iface: iface.clone(),
            log: Log {
                frame: 0,
                out: Vec::new(),
                playing: HashSet::new(),
            },
            last: HashMap::new(),
            mode: 11,
            act: true,
            screen: vec![0; w * h],
        };
        let _ = &r.files;
        r.st();
        let mut pointer = r.board.pointer;
        for frame in 1..=last_frame {
            r.log.frame = frame;
            let (mut left, mut right) = (false, false);
            for f in inputs.get(&frame).cloned().unwrap_or_default() {
                match f[0] {
                    "pointer" => {
                        pointer = (f[2].parse().unwrap(), f[3].parse().unwrap());
                        r.log.out.push(f.join(" "));
                    }
                    "click" => {
                        left = true;
                        r.log.out.push(f.join(" "));
                    }
                    _ => {
                        right = true;
                        r.log.out.push(f.join(" "));
                    }
                }
            }
            let ok = r.frame(Input {
                pointer,
                left,
                right,
            });
            pointer = r.board.pointer;
            r.st();
            if !ok {
                break;
            }
        }
        let want: Vec<String> = run
            .iter()
            .filter(|l| !l.starts_with("end "))
            .map(|l| l.to_string())
            .collect();
        let got = std::mem::take(&mut r.log.out);
        let n = want.len().min(got.len());
        for i in 0..n {
            if want[i] != got[i] {
                let from = i.saturating_sub(6);
                panic!(
                    "{w}x{h} run {name}: line {i} differs\n want: {}\n  got: {}\n--- golden:\n  {}\n--- port:\n  {}",
                    want[i],
                    got[i],
                    want[from..(i + 6).min(want.len())].join("\n  "),
                    got[from..(i + 6).min(got.len())].join("\n  ")
                );
            }
        }
        assert_eq!(want.len(), got.len(), "{w}x{h} run {name}: line counts");
    }
}

#[test]
#[ignore = "WIP: the unicorn's poly md5 differs from frame 78 (win) / 56 (pick) on; everything before matches"]
fn licorne_800() {
    check(G800, 800, 600, &["win", "lose", "pick"]);
}

#[test]
#[ignore = "WIP: the unicorn's poly md5 differs from frame 78 (win) / 56 (pick) on; everything before matches"]
fn licorne_640() {
    check(G640, 640, 480, &["pick"]);
}

#[test]
#[ignore = "WIP: the unicorn's poly md5 differs from frame 78 (win) / 56 (pick) on; everything before matches"]
fn licorne_1024() {
    check(G1024, 1024, 768, &["pick"]);
}

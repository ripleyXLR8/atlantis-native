//! Board goldens (M4): a puzzle's C code run under emulation by the
//! place-hook harnesses, replayed through the port's board API
//! ([`Game::board_begin`], [`Game::board_enter`], [`Game::board_frame`])
//! and compared line by line with what the port's observation log
//! ([`Game::trace`]) and state give.
//!
//! # The golden format
//!
//! A golden is a list of runs, each started by `run <name>` and played
//! from a fresh process (here: a fresh [`Game`]). Lines are
//! `<kind> <frame> <rest>`; frame n is at `base + n × period` seconds
//! ([`Clock`]; the place harnesses: 100 s + n × 1.0025 / 15 s, one 15 Hz
//! place-animation step per frame). Comment lines (`#`) and `log` lines
//! (the game's printf formats) are not compared; a `rand` line's caller
//! address is dropped.
//!
//! Inputs (replayed, and echoed into the port's stream where the harness
//! writes them):
//!
//! - `setup flag <n> <v>`: a story flag, before the place is loaded;
//! - `setup wam <file> view <v> yaw 0 pitch 0`: the place loaded and the
//!   first view entered at frame 0 (no `move` line);
//! - `setup mem32 <addr> <v>`: a C global the harness wrote
//!   ([`Game::test_poke`]); `setup cmd <command>`: a command run unlogged
//!   (both have no frame: they are replayed after the frame of the
//!   golden line before them);
//! - `move <f> <v>`: after frame f, `VUE` := v and the view entry;
//! - `script <f> <command>`: after frame f, a command the chapter script
//!   would run (its shows / hides are logged, not the command itself);
//! - `click <f> <zone>` / `sprite <f> <entity> <k>`: frame f has a zone
//!   click / a click on the k-th slot of the entity in the view.
//!
//! Checks made where they stand: `slots` (the view's slots `k:spw:entity`).
//!
//! Observations (the port's stream, in order within a frame): `cmd` (a
//! command the C code ran), `show` / `hide` (@ 0x0045f520 / 0x0045f610),
//! `anim` (@ 0x0045fa90), `snd load|play|stop`, `voice`, `voicestop`,
//! `rand`, `turn`, `camlock`, `camunlock`, `scan` (`CtrlScenarIN`),
//! `draw` (the sprite calls of a frame the C code drew and flipped
//! itself, then the screen's md5), then after the frame (entry or step)
//! the state lines when they changed: `st` (the board's globals,
//! [`Game::board_st`]), `flags`, `vars` (the keys of the run's first such
//! line), `slot` (each slot of the entities the run's `slot` lines name).
//!
//! Other goldens (the full-screen modes) use [`runs`], [`md5_screen`],
//! [`Board::raw_trace`] and [`draws`] with their own frame lines.

use std::collections::HashMap;

use atlantis2_data::defnames;
use atlantis3_data::A3Files;
use atlantis3_engine::{BoardInput, Game};
use md5::{Digest, Md5};

/// The clock of a golden's frames.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    pub base: f64,
    pub period: f64,
}

impl Clock {
    /// The place harnesses' (a3_toupie's family): 100 s, then 1.0025 / 15 s
    /// a frame.
    pub const PLACE: Clock = Clock {
        base: 100.0,
        period: 1.0025 / 15.0,
    };

    pub fn at(&self, frame: u32) -> f64 {
        self.base + f64::from(frame) * self.period
    }
}

/// What a board's replay needs besides the golden.
#[derive(Clone, Copy, Debug)]
pub struct Spec<'a> {
    /// `MONDE` of the place.
    pub world: i32,
    /// The [`Game::board_st`] key of the run's `st` lines.
    pub st: Option<&'a str>,
    pub clock: Clock,
}

/// A run of a golden: its name and its lines (comments dropped).
#[derive(Clone, Debug)]
pub struct Run<'a> {
    pub name: &'a str,
    pub lines: Vec<&'a str>,
}

/// The lines before the first run (named `""`, e.g. hit-test tables),
/// then the runs.
pub fn runs(golden: &str) -> Vec<Run<'_>> {
    let mut out = vec![Run {
        name: "",
        lines: Vec::new(),
    }];
    for l in golden.lines() {
        if l.starts_with('#') || l.trim().is_empty() {
            continue;
        }
        if let Some(name) = l.strip_prefix("run ") {
            out.push(Run {
                name,
                lines: Vec::new(),
            });
            continue;
        }
        out.last_mut().unwrap().lines.push(l);
    }
    out
}

/// The md5 of an RGB565 screen as the harnesses take it (little-endian
/// pixels).
pub fn md5_screen(px: &[u16]) -> String {
    let b: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(&b).iter().map(|x| format!("{x:02x}")).collect()
}

/// A line as compared: `None` for the lines not compared (`log`), a
/// `rand` line without its caller.
pub fn normalise(line: &str) -> Option<String> {
    let w: Vec<&str> = line.split_whitespace().collect();
    match w.first().copied() {
        None | Some("log") => None,
        Some("rand") => Some(w[..3.min(w.len())].join(" ")),
        _ => Some(line.trim_end().to_string()),
    }
}

/// Compares two streams after [`normalise`]; panics at the first
/// difference with the lines around it.
pub fn assert_same(what: &str, want: &[String], got: &[String]) {
    let want: Vec<String> = want.iter().filter_map(|l| normalise(l)).collect();
    let got: Vec<String> = got.iter().filter_map(|l| normalise(l)).collect();
    let n = want.len().min(got.len());
    for i in 0..n {
        if want[i] != got[i] {
            let from = i.saturating_sub(8);
            let ctx = |v: &[String]| v[from..(i + 6).min(v.len())].join("\n  ");
            panic!(
                "{what}: line {i} differs\n want: {}\n  got: {}\n--- golden around:\n  {}\n--- port around:\n  {}",
                want[i],
                got[i],
                ctx(&want),
                ctx(&got)
            );
        }
    }
    assert_eq!(
        want.len(),
        got.len(),
        "{what}: {} golden lines, {} port lines (extra: {:?})",
        want.len(),
        got.len(),
        if want.len() > n {
            &want[n..]
        } else {
            &got[n..]
        }
    );
}

/// The sprite calls of a traced frame: `warp=flat`, `BANK#F@X,Y*ALPHA`,
/// `C:BANK#F@X,Y`, up to its flip; the flips' digests.
pub fn draws(trace: &[String]) -> (Vec<String>, Vec<String>) {
    let mut d = Vec::new();
    let mut flips = Vec::new();
    for t in trace {
        if t == "warp=flat" {
            d.push(t.clone());
        } else if let Some(x) = t.strip_prefix("aff ") {
            d.push(x.to_string());
        } else if let Some(x) = t.strip_prefix("affc ") {
            d.push(format!("C:{x}"));
        } else if let Some(m) = t.strip_prefix("flip ") {
            flips.push(m.to_string());
        }
    }
    (d, flips)
}

/// A game replaying a board golden's run.
pub struct Board {
    pub g: Game,
    pub spec: Spec<'static>,
    pub frame: u32,
    /// The port's stream, in the golden's format.
    pub out: Vec<String>,
    last: HashMap<String, String>,
    flag_keys: Vec<usize>,
    var_keys: Vec<String>,
    entities: Vec<String>,
    draws: Vec<String>,
    /// The input line to write where the frame dispatches it.
    echo: Option<String>,
}

impl Board {
    /// A fresh game logging its observations, the panorama black.
    pub fn new(files: A3Files, spec: Spec<'static>) -> Board {
        let mut g = Game::new(files).expect("the game starts");
        g.trace = Some(Vec::new());
        g.trace_digest = Some(md5_screen);
        g.flat_warp = true;
        Board {
            g,
            spec,
            frame: 0,
            out: Vec::new(),
            last: HashMap::new(),
            flag_keys: Vec::new(),
            var_keys: Vec::new(),
            entities: Vec::new(),
            draws: Vec::new(),
            echo: None,
        }
    }

    pub fn now(&self) -> f64 {
        self.spec.clock.at(self.frame)
    }

    /// The observation log since the last call, as the engine wrote it.
    pub fn raw_trace(&mut self) -> Vec<String> {
        self.g
            .trace
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// An entity's name (kind 1 a character, else an object).
    pub fn entity(&self, kind: i32, id: i32) -> String {
        let section = if kind == 1 {
            defnames::PERSONNAGES
        } else {
            defnames::OBJETS
        };
        usize::try_from(id)
            .ok()
            .and_then(|i| self.g.names().name(section, i))
            .map_or_else(
                || format!("{kind}#{id}"),
                |n| String::from_utf8_lossy(n).into_owned(),
            )
    }

    /// Slot k's entity.
    pub fn slot_entity(&self, k: usize) -> String {
        let s = &self.g.slots().slots[k];
        self.entity(s.kind, s.id)
    }

    /// The trace since the last call into the stream, at the current
    /// frame.
    fn drain(&mut self) {
        let f = self.frame;
        for t in self.raw_trace() {
            if t == "input" {
                if let Some(e) = self.echo.take() {
                    self.out.push(e);
                }
            } else if t == "warp=flat" {
                self.draws.push(t);
            } else if let Some(x) = t.strip_prefix("aff ") {
                self.draws.push(x.to_string());
            } else if let Some(x) = t.strip_prefix("affc ") {
                self.draws.push(format!("C:{x}"));
            } else if let Some(m) = t.strip_prefix("flip ") {
                let d = if self.draws.is_empty() {
                    "-".to_string()
                } else {
                    self.draws.join(" ")
                };
                self.out.push(format!("draw {f} {d} md5 {m}"));
                self.draws.clear();
            } else {
                let (kind, rest) = t.split_once(' ').unwrap_or((t.as_str(), ""));
                self.out.push(if rest.is_empty() {
                    format!("{kind} {f}")
                } else {
                    format!("{kind} {f} {rest}")
                });
            }
        }
    }

    /// The state lines that changed (after an entry or a frame).
    fn state(&mut self) {
        let f = self.frame;
        let mut lines: Vec<(String, String)> = Vec::new();
        if let Some(k) = self.spec.st {
            lines.push(("st".into(), self.g.board_st(k).expect("a board's st")));
        }
        let s = self.g.script().unwrap();
        if !self.flag_keys.is_empty() {
            let v: Vec<String> = self
                .flag_keys
                .iter()
                .map(|&n| format!("{n}={}", s.flags[n]))
                .collect();
            lines.push(("flags".into(), v.join(" ")));
        }
        if !self.var_keys.is_empty() {
            let v: Vec<String> = self
                .var_keys
                .iter()
                .map(|n| format!("{n}={}", s.var(n).unwrap_or(0)))
                .collect();
            lines.push(("vars".into(), v.join(" ")));
        }
        for (k, sl) in self.g.slots().slots.iter().enumerate() {
            let ent = self.entity(sl.kind, sl.id);
            if !self.entities.contains(&ent) {
                continue;
            }
            let next = if sl.queued {
                sl.next.map_or(-1, |n| n as i32)
            } else {
                -1
            };
            lines.push((
                format!("slot {k}"),
                format!(
                    "{ent} {} sh {} mode {} fr {} last {} ended {} next {next}",
                    sl.name,
                    u8::from(sl.shown),
                    sl.mode,
                    sl.frame,
                    sl.last,
                    u8::from(sl.ended)
                ),
            ));
        }
        for (key, v) in lines {
            if self.last.get(&key) != Some(&v) {
                self.out.push(format!("{key} {f} {v}"));
                self.last.insert(key, v);
            }
        }
    }

    /// The view's `slots` check line.
    pub fn slots_line(&self) -> String {
        let v: Vec<String> = (0..self.g.slots().slots.len())
            .map(|k| {
                format!(
                    "{k}:{}:{}",
                    self.g.slots().slots[k].name,
                    self.slot_entity(k)
                )
            })
            .collect();
        format!("slots {}", v.join(" "))
    }

    /// One frame with an input (its echo line written where the frame
    /// dispatches it).
    pub fn step(&mut self, input: BoardInput<'_>, echo: Option<String>) {
        self.frame += 1;
        self.echo = echo;
        let now = self.now();
        assert!(
            self.g.board_frame(now, input),
            "frame {}: {input:?}",
            self.frame
        );
        self.drain();
        self.state();
    }

    /// Frames up to frame `f`.
    pub fn advance_to(&mut self, f: u32) {
        while self.frame < f {
            self.step(BoardInput::None, None);
        }
    }

    /// The view entered now.
    pub fn enter(&mut self, view: u32) {
        let now = self.now();
        self.g.board_enter(view, now);
        self.drain();
        self.state();
    }

    /// Replays a run of the place harnesses' format; the port's stream is
    /// in [`Board::out`].
    pub fn replay(&mut self, run: &Run<'_>) {
        // The keys of the state lines, from the run's own lines.
        let mut last_frame = 0;
        for l in &run.lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w[0] {
                "flags" if self.flag_keys.is_empty() => {
                    self.flag_keys = w[2..]
                        .iter()
                        .map(|kv| kv.split_once('=').unwrap().0.parse().unwrap())
                        .collect();
                }
                "vars" if self.var_keys.is_empty() => {
                    self.var_keys = w[2..]
                        .iter()
                        .map(|kv| kv.split_once('=').unwrap().0.to_string())
                        .collect();
                }
                "slot" if !self.entities.iter().any(|e| e == w[3]) => {
                    self.entities.push(w[3].to_string());
                }
                _ => {}
            }
            // A `slot` line's frame follows the slot's number.
            let at = if w[0] == "slot" { 2 } else { 1 };
            if w[0] != "setup" && w[0] != "slots" {
                if let Some(f) = w.get(at).and_then(|f| f.parse::<u32>().ok()) {
                    last_frame = last_frame.max(f);
                }
            }
        }
        // The frame of the last golden line that has one: a `setup` line
        // inside the run happened then.
        let mut seen = 0;
        for l in &run.lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            let at = if w[0] == "slot" { 2 } else { 1 };
            if w[0] != "setup" && w[0] != "slots" {
                if let Some(f) = w.get(at).and_then(|f| f.parse::<u32>().ok()) {
                    seen = f;
                }
            }
            let frame = || w[1].parse::<u32>().unwrap();
            let rest = |n: usize| {
                let mut s = *l;
                for _ in 0..n {
                    s = s.trim_start().split_once(' ').map_or("", |x| x.1);
                }
                s.to_string()
            };
            match w[0] {
                "setup" => {
                    if matches!(w[1], "mem32" | "cmd") {
                        self.advance_to(seen);
                    }
                    self.out.push(l.to_string());
                    match w[1] {
                        "flag" => {
                            let n: usize = w[2].parse().unwrap();
                            self.g.script_mut().unwrap().flags[n] = w[3].parse().unwrap();
                        }
                        "wam" => {
                            assert!(self.g.board_begin(w[2], self.spec.world), "{l}");
                            self.enter(w[4].parse().unwrap());
                        }
                        "mem32" => {
                            self.advance_to(seen);
                            let a = u32::from_str_radix(w[2].trim_start_matches("0x"), 16).unwrap();
                            assert!(self.g.test_poke(a, w[3].parse().unwrap()), "{l}");
                        }
                        "cmd" => {
                            self.advance_to(seen);
                            let t = self.g.trace.take();
                            self.g.run_command(&rest(2));
                            self.g.trace = t;
                        }
                        other => panic!("{l}: setup {other}"),
                    }
                }
                "move" => {
                    self.advance_to(frame());
                    self.out.push(l.to_string());
                    self.enter(w[2].parse().unwrap());
                }
                "script" => {
                    self.advance_to(frame());
                    self.out.push(l.to_string());
                    self.g.run_command(&rest(2));
                    self.drain();
                }
                "click" => {
                    self.advance_to(frame() - 1);
                    let z = rest(2);
                    self.step(BoardInput::Zone(&z), Some(l.to_string()));
                }
                "sprite" => {
                    self.advance_to(frame() - 1);
                    let n: usize = w[3].parse().unwrap();
                    let k = (0..self.g.slots().slots.len())
                        .filter(|&k| self.slot_entity(k) == w[2])
                        .nth(n)
                        .unwrap_or_else(|| panic!("{l}: no such slot"));
                    self.step(BoardInput::Sprite(k), Some(l.to_string()));
                }
                "slots" => {
                    let s = self.slots_line();
                    self.out.push(s);
                }
                _ => {}
            }
        }
        self.advance_to(last_frame);
    }
}

/// Replays every run of a place golden with `spec` and compares; the
/// runs' names.
pub fn replay_all(files: impl Fn() -> A3Files, golden: &str, spec: Spec<'static>) -> Vec<String> {
    let mut names = Vec::new();
    for run in runs(golden).into_iter().skip(1) {
        let mut b = Board::new(files(), spec);
        b.replay(&run);
        let want: Vec<String> = run.lines.iter().map(|l| l.to_string()).collect();
        assert_same(&format!("run {}", run.name), &want, &b.out);
        names.push(run.name.to_string());
    }
    names
}

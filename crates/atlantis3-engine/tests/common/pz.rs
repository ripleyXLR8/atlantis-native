//! Place-board goldens of worlds 1 and 2 (M4): a puzzle's C code run
//! under emulation by the shared place harness of the Hoggar, the funeral
//! chamber and the Land of the Dead, replayed through the port's board
//! API and compared line by line ([`super::board::assert_same`]).
//!
//! # The format
//!
//! Runs as in [`super::board`] (`run <name>`, a fresh [`Game`] each);
//! lines `<kind> <frame> <rest>`, frame n at 100 + n × 1.0025 / 15 s (one
//! 15 Hz place-animation step per frame).
//!
//! Set-up (in this order): `setup wam <file> view <v> yaw 0 pitch 0` (the
//! place loaded), `setup flag <n> <v>` / `setup cmd <command>` (written
//! before the place's `CHANGEWAM` set-up when they come before `entry`,
//! after the view entry otherwise), `entry 0` (the set-up's commands
//! unlogged, `rand` seeded with 1, the view entered at 100 s: its own
//! calls are logged at frame 0), then the checks `slots` (the view's
//! slots `k:spw`) and `slotent` (`k:entity`), then the first state lines.
//!
//! Inputs, between frames (an input with frame f comes after frame
//! f − 1's step): `click <f> <zone>` (the zone record given to the zone
//! click hook @ 0x004c32c0; the frame hook reads it), `hand <f> <name>|0`
//! (the object put in hand, or the hand cleared). A frame: the place
//! animations' step, the frame hook, the main loop's place tail.
//!
//! Observations: `cmd`, `show`, `hide`, `addf` / `subf` (the frame-list
//! calls, the object's `#OBJETS#` name), `snd load|loaded|play|stop`,
//! `cursor set <index>` / `cursor reset`, `scan`, the Isis caption's
//! `caption`, `draw`, `textid`; after each input and frame the state lines
//! that changed: per slot `slot <f> <k> sh .. mode .. fr .. list ..
//! ended ..` and `slotck <f> <k> <click kind>`, `st` (the board's
//! globals, [`Game::board_st`]), `flags` (every flag that changed),
//! `hand <f> <id> <name>|-`, `bag` (the objects of the run's first `bag`
//! line).
//!
//! Three differences of form, normalised here: the golden's `cursor set`
//! names the bank (`''`: the harness runs no world set-up, the bank name
//! of 0x007565f0 is empty), the port logs the index only; the harness's
//! voice language (0x00789b10) is empty, its voice files are named
//! `loc\voix\<world>\...`: the install's language is put in; the port's
//! `rand` lines (the harness logs none: the draws are checked through the
//! state they give) are dropped.

use std::collections::HashMap;

use atlantis2_data::defnames;
use atlantis3_data::A3Files;
use atlantis3_engine::Game;

use super::board::{assert_same, runs, Clock, Run};

/// What a replay needs besides the golden.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    /// `MONDE` of the place.
    pub world: i32,
    /// The [`Game::board_st`] key of the `st` lines.
    pub st: &'static str,
}

/// A game replaying a run.
pub struct Pz {
    pub g: Game,
    spec: Spec,
    clock: Clock,
    pub frame: u32,
    /// The port's stream in the golden's format.
    pub out: Vec<String>,
    view: u32,
    entered: bool,
    last: HashMap<String, String>,
    flags: Vec<i32>,
    bag_watch: Vec<String>,
}

impl Pz {
    pub fn new(files: A3Files, spec: Spec) -> Pz {
        let mut g = Game::new(files).expect("the game starts");
        g.trace = Some(Vec::new());
        g.flat_warp = true;
        Pz {
            g,
            spec,
            clock: Clock::PLACE,
            frame: 0,
            out: Vec::new(),
            view: 0,
            entered: false,
            last: HashMap::new(),
            flags: Vec::new(),
            bag_watch: Vec::new(),
        }
    }

    fn now(&self) -> f64 {
        self.clock.at(self.frame)
    }

    fn object_name(&self, id: i32) -> String {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.g.names().name(defnames::OBJETS, i))
            .map_or_else(
                || format!("2#{id}"),
                |n| String::from_utf8_lossy(n).into_owned(),
            )
    }

    fn entity(&self, kind: i32, id: i32) -> String {
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

    /// The trace since the last call, at frame `f`.
    fn drain(&mut self, f: u32) {
        let t = self
            .g
            .trace
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        for l in t {
            let (kind, rest) = l.split_once(' ').unwrap_or((l.as_str(), ""));
            if kind == "rand" {
                continue;
            }
            self.out.push(if rest.is_empty() {
                format!("{kind} {f}")
            } else {
                format!("{kind} {f} {rest}")
            });
        }
    }

    /// The state lines that changed (the harness's snapshot).
    fn snapshot(&mut self, f: u32, initial: bool) {
        let n = self.g.slots().slots.len();
        for k in 0..n {
            let s = &self.g.slots().slots[k];
            let list: Vec<String> = s.frame_entries().map(|v| v.to_string()).collect();
            let line = format!(
                "slot {f} {k} sh {} mode {} fr {} list {} ended {}",
                u8::from(s.shown),
                s.mode,
                s.frame,
                if list.is_empty() {
                    "-".to_string()
                } else {
                    list.join(",")
                },
                u8::from(s.ended)
            );
            let ck = s.click;
            // The key ignores the frame number.
            let body = line.splitn(4, ' ').nth(3).unwrap().to_string();
            if self.last.get(&format!("slot{k}")) != Some(&body) {
                self.last.insert(format!("slot{k}"), body);
                self.out.push(line);
            }
            let ckb = ck.to_string();
            if self.last.get(&format!("ck{k}")) != Some(&ckb) {
                self.last.insert(format!("ck{k}"), ckb);
                self.out.push(format!("slotck {f} {k} {ck}"));
            }
        }
        let st = self.g.board_st(self.spec.st).expect("a board's st");
        if self.last.get("st") != Some(&st) {
            self.last.insert("st".into(), st.clone());
            self.out.push(format!("st {f} {st}"));
        }
        let s = self.g.script().unwrap();
        let fl = s.flags[..1100].to_vec();
        if !initial {
            let ch: Vec<String> = (0..1100)
                .filter(|&n| fl[n] != self.flags[n])
                .map(|n| format!("{n}={}", fl[n]))
                .collect();
            if !ch.is_empty() {
                self.out.push(format!("flags {f} {}", ch.join(" ")));
            }
        }
        self.flags = fl;
        let hand = self.g.object_in_hand();
        let hl = format!(
            "{hand} {}",
            if hand != 0 {
                self.object_name(hand)
            } else {
                "-".into()
            }
        );
        if self.last.get("hand") != Some(&hl) {
            self.last.insert("hand".into(), hl.clone());
            self.out.push(format!("hand {f} {hl}"));
        }
        let bag: Vec<(String, bool)> = self
            .bag_watch
            .iter()
            .map(|nm| {
                let id = self
                    .g
                    .names()
                    .id(defnames::OBJETS, nm.as_bytes())
                    .map_or(-1, |i| i as i32);
                (
                    nm.clone(),
                    self.g.script().unwrap().bag.iter().any(|e| e.id == id),
                )
            })
            .collect();
        if initial {
            if !bag.is_empty() {
                let v: Vec<String> = bag
                    .iter()
                    .map(|(nm, b)| format!("{}{nm}", if *b { '+' } else { '-' }))
                    .collect();
                self.out.push(format!("bag {f} {}", v.join(" ")));
            }
        } else {
            for (nm, b) in &bag {
                let key = format!("bag {nm}");
                let v = b.to_string();
                if self.last.get(&key) != Some(&v) {
                    self.out
                        .push(format!("bag {f} {}{nm}", if *b { '+' } else { '-' }));
                }
            }
        }
        for (nm, b) in bag {
            self.last.insert(format!("bag {nm}"), b.to_string());
        }
    }

    fn step(&mut self) {
        self.frame += 1;
        let now = self.now();
        self.g.board_place_frame(now);
        let f = self.frame;
        self.drain(f);
        self.snapshot(f, false);
    }

    fn advance_to(&mut self, f: u32) {
        while self.frame < f {
            self.step();
        }
    }

    /// The view's `slots` and `slotent` check lines.
    fn slot_lines(&self) -> (String, String) {
        let s = &self.g.slots().slots;
        let a: Vec<String> = (0..s.len()).map(|k| format!("{k}:{}", s[k].name)).collect();
        let b: Vec<String> = (0..s.len())
            .map(|k| format!("{k}:{}", self.entity(s[k].kind, s[k].id)))
            .collect();
        (
            format!("slots {}", a.join(" ")),
            format!("slotent {}", b.join(" ")),
        )
    }

    /// Replays a run; the port's stream is in [`Pz::out`].
    pub fn replay(&mut self, run: &Run<'_>) {
        let mut last_frame = 0;
        for l in &run.lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            if w[0] == "bag" && self.bag_watch.is_empty() {
                self.bag_watch = w[2..].iter().map(|x| x[1..].to_string()).collect();
            }
            if !matches!(w[0], "setup" | "slots" | "slotent") {
                if let Some(f) = w.get(1).and_then(|f| f.parse::<u32>().ok()) {
                    last_frame = last_frame.max(f);
                }
            }
        }
        for l in &run.lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            let rest = |n: usize| {
                let mut s = *l;
                for _ in 0..n {
                    s = s.split_once(' ').map_or("", |x| x.1);
                }
                s.to_string()
            };
            match w[0] {
                "setup" => {
                    self.out.push(l.to_string());
                    match w[1] {
                        "wam" => {
                            assert!(self.g.board_begin(w[2], self.spec.world), "{l}");
                            self.view = w[4].parse().unwrap();
                        }
                        "flag" => {
                            let n: usize = w[2].parse().unwrap();
                            self.g.script_mut().unwrap().flags[n] = w[3].parse().unwrap();
                        }
                        "cmd" => {
                            let t = self.g.trace.take();
                            self.g.run_command(&rest(2));
                            self.g.trace = t;
                        }
                        other => panic!("{l}: setup {other}"),
                    }
                }
                "entry" => {
                    self.out.push(l.to_string());
                    self.g.board_wam_setup();
                    self.g.seed(1);
                    let now = self.now();
                    self.g.board_enter(self.view, now);
                    self.drain(0);
                    self.entered = true;
                }
                "slots" => {
                    let (a, b) = self.slot_lines();
                    self.out.push(a);
                    self.out.push(b);
                    self.snapshot(0, true);
                }
                "click" => {
                    let f: u32 = w[1].parse().unwrap();
                    self.advance_to(f - 1);
                    self.out.push(l.to_string());
                    let now = self.now();
                    assert!(self.g.board_zone_hook(&rest(2), now), "{l}");
                    self.drain(f);
                    self.snapshot(f, false);
                }
                // An input (`hand f name`), not a state line (`hand f id
                // name`).
                "hand" if w.len() == 3 => {
                    let f: u32 = w[1].parse().unwrap();
                    self.advance_to(f - 1);
                    self.out.push(l.to_string());
                    self.g
                        .board_hand(if w[2] == "0" { None } else { Some(w[2]) });
                    self.drain(f);
                    self.snapshot(f, false);
                }
                _ => {}
            }
        }
        assert!(self.entered, "run {}: no entry", run.name);
        self.advance_to(last_frame);
    }
}

/// A golden line as compared: the bank of `cursor set` dropped; the
/// voice language (`lang`) put in a voice file's path.
fn normalise_golden(l: &str, lang: &str) -> String {
    let w: Vec<&str> = l.split_whitespace().collect();
    if w.len() == 5 && w[0] == "cursor" && w[2] == "set" {
        return format!("cursor {} set {}", w[1], w[4]);
    }
    l.replace("loc\\voix\\", &format!("loc\\voix\\{lang}\\"))
}

/// Replays every run of a place golden and compares; the runs' names.
pub fn replay_all(files: impl Fn() -> A3Files, golden: &str, spec: Spec) -> Vec<String> {
    let mut names = Vec::new();
    for run in runs(golden).into_iter().skip(1) {
        let mut p = Pz::new(files(), spec);
        p.replay(&run);
        let lang = p.g.startup.voice_language();
        let want: Vec<String> = run
            .lines
            .iter()
            .map(|l| normalise_golden(l, &lang))
            .collect();
        assert_same(&format!("run {}", run.name), &want, &p.out);
        names.push(run.name.to_string());
    }
    names
}

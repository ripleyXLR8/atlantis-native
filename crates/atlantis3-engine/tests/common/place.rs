//! Place-board goldens of Paleo and the corridor (M4): a puzzle's C code
//! run under emulation by the place-hook harness of `paleo.wam` (also
//! used for `Couloir.wam`), replayed through the port's board API and
//! compared line by line ([`super::board::assert_same`]).
//!
//! # The format
//!
//! Runs as in [`super::board`] (`run <name>`, a fresh [`Game`] each);
//! lines `<kind> <frame> <rest>`, frame n at 100 + n × 1.0025 / 15 s.
//!
//! Set-up: `setup wam <file> view <v> yaw <y> pitch <p>`, then `setup
//! flag <n> <v> [comment]` and `setup cmd <command>`. The harness loads
//! the place, writes the flags, runs the commands (unlogged), runs the
//! place's `CHANGEWAM` set-up (Paleo only), seeds `rand` with 1 and
//! enters the view at 100 s at the arrival angles (integers: the `/set`
//! handler's conversion), all unlogged. Then the header: `slots`
//! (`k:spw(entity)`), the slot lines, `st`, `bag 0` (the watched objects
//! in the bag, `-` for none), `hand 0`.
//!
//! A frame: the place animations' step, the frame's inputs, the frame
//! hook; then a `view <f> <v> (entered by the code)` line and a `slots`
//! line when `VUE` changed (the code's `(Vue=n)`), and the state lines
//! that changed: `slot <f> <k> sh .. mode .. fr .. list .. ended ..`,
//! `st`, `flags` (the watched flags that changed, in the watch order),
//! `hand <f> <name>|0`, `bag <f> ±<name>` (one per change).
//!
//! Inputs within frame f: `hand <f> <name>|-` (the object put in hand
//! unlogged, `-` the hand emptied), `click <f> <zone>` (the zone click
//! hook), `camera <f> <yaw> <pitch>` (the camera reset and turned to those
//! angles: integers as the `/set` handler converts them, else float
//! radians; then an `aim` line), `rclick <f>` (the right button's byte),
//! `flag <f> <n> <v>`. Between frames: `enter <f> view <v> yaw <y> pitch
//! <p> (walk)` (after frame f, the view entered, then its `slots` line and
//! every slot line). A `hand` line is an input when it is the first line
//! of its frame (or names `-`), else a state line.
//!
//! Observations: `cmd`, `show`, `hide`, `addf`, `subf`, `snd`, `scan`,
//! `lock <f> 1|0`, `draw <f> <bank>#<i>@<x>,<y>*<alpha bits>`,
//! `sprclose <f> <bank>`; `log` lines are not compared; the port's `rand`
//! lines are dropped (the harness logs none).

use std::collections::HashMap;

use atlantis2_data::defnames;
use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::Fpu;
use atlantis3_data::A3Files;
use atlantis3_engine::Game;

use super::board::{assert_same, runs, Clock, Run};

/// What a replay needs besides the golden (the harness's run set-up).
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    /// `MONDE` of the place.
    pub world: i32,
    /// The [`Game::board_st`] key of the `st` lines.
    pub st: &'static str,
    /// The place's `CHANGEWAM` set-up is run (Paleo: @ 0x004502b0).
    pub wam_setup: bool,
    /// The flags and the bag objects watched.
    pub flags: &'static [usize],
    pub bag: &'static [&'static str],
}

/// A game replaying a run.
pub struct Place {
    pub g: Game,
    spec: Spec,
    clock: Clock,
    pub frame: u32,
    /// The port's stream in the golden's format.
    pub out: Vec<String>,
    view: i32,
    last_slots: HashMap<usize, String>,
    last_st: Option<String>,
    last_flags: Vec<i32>,
    last_hand: Option<String>,
    last_bag: HashMap<String, bool>,
}

/// An input of a frame.
enum Input {
    Hand(String),
    Click(String),
    Camera(String, String),
    Right,
    Flag(usize, i32),
}

/// Degrees as the harness turns them into radians: integers by the
/// `/set` handler's conversion, else `math.radians` rounded to a float.
fn angle(s: &str) -> f32 {
    if s.contains('.') {
        s.parse::<f64>().unwrap().to_radians() as f32
    } else {
        deg_to_rad_a3(Fpu::Double, s.parse().unwrap())
    }
}

/// Both angles as the harness converts them: floats when either is one.
fn angles(y: &str, p: &str) -> (f32, f32) {
    if y.contains('.') || p.contains('.') {
        let f = |s: &str| s.parse::<f64>().unwrap().to_radians() as f32;
        (f(y), f(p))
    } else {
        (angle(y), angle(p))
    }
}

impl Place {
    pub fn new(files: A3Files, spec: Spec) -> Place {
        let mut g = Game::new(files).expect("the game starts");
        g.trace = Some(Vec::new());
        g.flat_warp = true;
        Place {
            g,
            spec,
            clock: Clock::PLACE,
            frame: 0,
            out: Vec::new(),
            view: 0,
            last_slots: HashMap::new(),
            last_st: None,
            last_flags: Vec::new(),
            last_hand: None,
            last_bag: HashMap::new(),
        }
    }

    fn now(&self) -> f64 {
        self.clock.at(self.frame)
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

    fn vue(&self) -> i32 {
        self.g.script().unwrap().var("VUE").unwrap_or(0)
    }

    /// The trace since the last call, at frame `f`.
    fn drain(&mut self) {
        let f = self.frame;
        let t = self
            .g
            .trace
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        for l in t {
            let (kind, rest) = l.split_once(' ').unwrap_or((l.as_str(), ""));
            let kind = match kind {
                "rand" | "warp=flat" | "flip" | "input" => continue,
                "aff" => "draw",
                k => k,
            };
            self.out.push(if rest.is_empty() {
                format!("{kind} {f}")
            } else {
                format!("{kind} {f} {rest}")
            });
        }
    }

    fn slots_line(&self) -> String {
        let s = &self.g.slots().slots;
        let v: Vec<String> = (0..s.len())
            .map(|k| format!("{k}:{}({})", s[k].name, self.entity(s[k].kind, s[k].id)))
            .collect();
        format!("slots {}", v.join(" "))
    }

    fn hand_name(&self) -> String {
        let id = self.g.object_in_hand();
        if id == 0 {
            "0".into()
        } else {
            self.entity(2, id)
        }
    }

    fn in_bag(&self, name: &str) -> bool {
        let id = self
            .g
            .names()
            .id(defnames::OBJETS, name.as_bytes())
            .map_or(-1, |i| i as i32);
        self.g.script().unwrap().bag.iter().any(|e| e.id == id)
    }

    /// The harness's `state()`: the lines that changed.
    fn state(&mut self) {
        let f = self.frame;
        for k in 0..self.g.slots().slots.len() {
            let s = &self.g.slots().slots[k];
            let list: Vec<String> = s.frame_entries().map(|v| v.to_string()).collect();
            let body = format!(
                "sh {} mode {} fr {} list {} ended {}",
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
            if self.last_slots.get(&k) != Some(&body) {
                self.out.push(format!("slot {f} {k} {body}"));
                self.last_slots.insert(k, body);
            }
        }
        let st = self.g.board_st(self.spec.st).expect("a board's st");
        if self.last_st.as_ref() != Some(&st) {
            self.out.push(format!("st {f} {st}"));
            self.last_st = Some(st);
        }
        let flags = &self.g.script().unwrap().flags;
        let now: Vec<i32> = self.spec.flags.iter().map(|&n| flags[n]).collect();
        let ch: Vec<String> = self
            .spec
            .flags
            .iter()
            .zip(&now)
            .zip(&self.last_flags)
            .filter(|((_, a), b)| a != b)
            .map(|((n, a), _)| format!("{n}={a}"))
            .collect();
        if !ch.is_empty() {
            self.out.push(format!("flags {f} {}", ch.join(" ")));
        }
        self.last_flags = now;
        let hand = self.hand_name();
        if self.last_hand.as_ref() != Some(&hand) {
            if self.last_hand.is_some() {
                self.out.push(format!("hand {f} {hand}"));
            }
            self.last_hand = Some(hand);
        }
        for nm in self.spec.bag {
            let v = self.in_bag(nm);
            match self.last_bag.get(*nm) {
                Some(&old) if old != v => {
                    self.out
                        .push(format!("bag {f} {}{nm}", if v { '+' } else { '-' }));
                }
                _ => {}
            }
            self.last_bag.insert(nm.to_string(), v);
        }
    }

    fn aim_line(&mut self) {
        let (y, p) = self.g.board_aim();
        let y = f64::from(y) * 57.29747 + 180.0;
        let p = f64::from(p) * 57.29747;
        self.out
            .push(format!("aim {} yaw={y:.3} pitch={p:.3}", self.frame));
    }

    /// One frame with its inputs.
    fn step(&mut self, inputs: Vec<(String, Input)>) {
        self.frame += 1;
        let now = self.now();
        self.g.board_anim_step(now);
        self.drain();
        for (echo, i) in inputs {
            self.out.push(echo);
            match i {
                Input::Hand(n) => {
                    self.g.board_hand(if n == "-" { None } else { Some(&n) });
                    // The harness takes the hand it put as seen.
                    self.last_hand = Some(self.hand_name());
                }
                Input::Click(z) => assert!(self.g.board_zone_hook(&z, now), "zone {z}"),
                Input::Camera(y, p) => {
                    let (y, p) = angles(&y, &p);
                    self.g.board_camera(y, p);
                    self.aim_line();
                }
                Input::Right => self.g.board_right_button(),
                Input::Flag(n, v) => self.g.script_mut().unwrap().flags[n] = v,
            }
            self.drain();
        }
        self.g.board_hook();
        self.drain();
        let v = self.vue();
        if v != self.view {
            self.view = v;
            self.out
                .push(format!("view {} {v} (entered by the code)", self.frame));
            self.out.push(self.slots_line());
            self.last_slots.clear();
        }
        self.state();
    }

    fn advance_to(&mut self, f: u32) {
        while self.frame < f {
            self.step(Vec::new());
        }
    }

    /// Replays a run; the port's stream is in [`Place::out`].
    pub fn replay(&mut self, run: &Run<'_>) {
        // The frame of each line (none for set-up and `slots` lines).
        let frame_of = |l: &str| -> Option<u32> {
            let w: Vec<&str> = l.split_whitespace().collect();
            if matches!(w[0], "setup" | "slots") {
                return None;
            }
            w.get(1)?.parse().ok()
        };
        let last_frame = run
            .lines
            .iter()
            .filter_map(|l| frame_of(l))
            .max()
            .unwrap_or(0);
        // Set-up.
        let mut wam = ("", 0, "0", "0");
        let mut cmds = Vec::new();
        let mut flags = Vec::new();
        for l in &run.lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            if w[0] != "setup" {
                continue;
            }
            self.out.push(l.to_string());
            match w[1] {
                "wam" => wam = (w[2], w[4].parse().unwrap(), w[6], w[8]),
                "flag" => {
                    flags.push((w[2].parse::<usize>().unwrap(), w[3].parse::<i32>().unwrap()))
                }
                "cmd" => cmds.push(l.splitn(3, ' ').nth(2).unwrap().to_string()),
                other => panic!("{l}: setup {other}"),
            }
        }
        assert!(self.g.board_begin(wam.0, self.spec.world), "{}", wam.0);
        for (n, v) in flags {
            self.g.script_mut().unwrap().flags[n] = v;
        }
        let t = self.g.trace.take();
        for c in &cmds {
            self.g.run_command(c);
        }
        self.g.trace = t;
        if self.spec.wam_setup {
            self.g.board_wam_setup();
        }
        self.g.seed(1);
        let (y, p) = angles(wam.2, wam.3);
        let t = self.g.trace.take();
        self.g.board_enter_at(wam.1, y, p, self.now());
        self.g.trace = t;
        self.view = wam.1 as i32;
        // The header.
        self.out.push(self.slots_line());
        self.state();
        let bag: Vec<String> = self
            .spec
            .bag
            .iter()
            .filter(|n| self.in_bag(n))
            .map(|n| format!("+{n}"))
            .collect();
        self.out.push(format!(
            "bag 0 {}",
            if bag.is_empty() {
                "-".to_string()
            } else {
                bag.join(" ")
            }
        ));
        self.out.push(format!("hand 0 {}", self.hand_name()));
        if run.lines.iter().any(|l| l.starts_with("aim 0 ")) {
            self.aim_line();
        }
        // The inputs, grouped by frame.
        let mut prev: Option<u32> = Some(0);
        let mut pending: Vec<(String, Input)> = Vec::new();
        let mut pending_frame = 0;
        let lines: Vec<&str> = run
            .lines
            .iter()
            .copied()
            .filter(|l| !l.starts_with("setup"))
            .collect();
        for l in &lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            let f = frame_of(l);
            let input = match w[0] {
                "hand" if f != Some(0) && (w[2] == "-" || prev < f) => {
                    Some(Input::Hand(w[2].to_string()))
                }
                "click" => Some(Input::Click(l.splitn(3, ' ').nth(2).unwrap().to_string())),
                "camera" => Some(Input::Camera(w[2].to_string(), w[3].to_string())),
                "rclick" => Some(Input::Right),
                "flag" => Some(Input::Flag(w[2].parse().unwrap(), w[3].parse().unwrap())),
                _ => None,
            };
            if let Some(i) = input {
                let f = f.unwrap();
                if !pending.is_empty() && pending_frame != f {
                    self.advance_to(pending_frame - 1);
                    self.step(std::mem::take(&mut pending));
                }
                pending_frame = f;
                pending.push((l.to_string(), i));
            } else if w[0] == "enter" {
                let f = f.unwrap();
                if !pending.is_empty() {
                    self.advance_to(pending_frame - 1);
                    self.step(std::mem::take(&mut pending));
                }
                self.advance_to(f);
                self.out.push(l.to_string());
                let v: u32 = w[3].parse().unwrap();
                let (y, p) = angles(w[5], w[7]);
                // `VUE` := v, then the view entry.
                self.g.board_enter_at(v, y, p, self.now());
                self.drain();
                self.view = v as i32;
                self.out.push(self.slots_line());
                self.last_slots.clear();
                self.state();
            }
            if f.is_some() {
                prev = f;
            }
        }
        if !pending.is_empty() {
            self.advance_to(pending_frame - 1);
            self.step(std::mem::take(&mut pending));
        }
        self.advance_to(last_frame);
    }
}

/// Replays every run of a golden and compares; the runs' names.
pub fn replay_all(files: impl Fn() -> A3Files, golden: &str, spec: Spec) -> Vec<String> {
    let mut names = Vec::new();
    for run in runs(golden).into_iter().skip(1) {
        let mut p = Place::new(files(), spec);
        p.replay(&run);
        let want: Vec<String> = run.lines.iter().map(|l| l.to_string()).collect();
        assert_same(&format!("run {}", run.name), &want, &p.out);
        names.push(run.name.to_string());
    }
    names
}

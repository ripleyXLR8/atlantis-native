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
use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::Fpu;
use atlantis3_data::A3Files;
use atlantis3_engine::game::Screen;
use atlantis3_engine::Game;

use super::board::{assert_same, runs, Clock, Run};

/// What a replay needs besides the golden.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    /// `MONDE` of the place.
    pub world: i32,
    /// The [`Game::board_st`] key of the `st` lines.
    pub st: &'static str,
    /// The palace's and the tower's harnesses (`a3_pz_pmagi.py`,
    /// `a3_pz_tower.py`): the set-up's commands logged at frame 0 before
    /// `entry`, `objet` inputs, a click's frame ending with the click scan,
    /// `end` lines.
    pub palace: bool,
    /// The tower's harness (`a3_pz_tower.py`, with `palace`): the first
    /// view entered at the run's angles, `view` / `cinema` lines, the code's
    /// view entries changing the slots (`slots <f>` / `slotent <f>` state
    /// lines), the `GBOY` mode's frames (`die` inputs, `gboy` /
    /// `gboystop` lines), the music players' stub (`snd stopmus`) not
    /// compared.
    pub tower: bool,
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
    /// The palace harness's inputs for the next frame: an object click,
    /// the click scan.
    object: Option<String>,
    scan: bool,
    /// The last `snd test` answer per name (the harness writes a change
    /// only).
    tests: HashMap<String, String>,
    /// The tower: the first view's angles, a die thrown in the next frame,
    /// the slots' names last written.
    angles: (f32, f32),
    die: bool,
    slot_names: String,
}

impl Pz {
    pub fn new(files: A3Files, spec: Spec) -> Pz {
        let mut g = Game::new(files).expect("the game starts");
        g.trace = Some(Vec::new());
        g.flat_warp = true;
        g.trace_views = spec.tower;
        g.stub_cinemas = spec.tower;
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
            object: None,
            scan: false,
            tests: HashMap::new(),
            angles: (0.0, 0.0),
            die: false,
            slot_names: String::new(),
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
            if self.spec.tower {
                if l == "warp=flat" || matches!(kind, "aff" | "affc" | "flip") {
                    continue;
                }
                // The mode's stop: the frame's state first.
                if l == "stop" {
                    self.gboy_line(f);
                    self.out.push(format!("gboystop {f}"));
                    continue;
                }
            }
            if let Some(t) = rest.strip_prefix("test ") {
                let (name, v) = t.rsplit_once(" -> ").unwrap_or((t, ""));
                if self.tests.get(name).map(String::as_str) == Some(v) {
                    continue;
                }
                self.tests.insert(name.to_string(), v.to_string());
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
        if self.spec.tower && !initial {
            let (a, b) = self.slot_lines();
            if a != self.slot_names {
                self.slot_names = a.clone();
                self.out
                    .push(a.replacen("slots ", &format!("slots {f} "), 1));
                self.out
                    .push(b.replacen("slotent ", &format!("slotent {f} "), 1));
                self.last
                    .retain(|k, _| !k.starts_with("slot") && !k.starts_with("ck"));
            }
        }
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
        if self.spec.tower && self.g.puzzle_screen() == Some(Screen::GBoy) {
            let left = std::mem::take(&mut self.die);
            self.g.board_screen_frame(now, (703, 568), left, false);
            self.g.board_main_tail();
            let f = self.frame;
            let stopped = self.g.puzzle_screen().is_none();
            self.drain(f);
            if !stopped {
                self.gboy_line(f);
            }
            self.snapshot(f, false);
            return;
        }
        if self.spec.palace {
            let object = self.object.take();
            let scan = std::mem::take(&mut self.scan) || object.is_some();
            assert!(
                self.g.board_place_click(now, object.as_deref(), scan),
                "frame {}: no slot of {object:?}",
                self.frame
            );
        } else {
            self.g.board_place_frame(now);
        }
        let f = self.frame;
        self.drain(f);
        self.snapshot(f, false);
    }

    fn advance_to(&mut self, f: u32) {
        while self.frame < f {
            self.step();
        }
    }

    /// The tower: the game boy's `gboy` line when it changed.
    fn gboy_line(&mut self, f: u32) {
        let st = self.g.board_st("gboy").unwrap();
        if self.last.get("gboy") != Some(&st) {
            self.out.push(format!("gboy {f} {st}"));
            self.last.insert("gboy".into(), st);
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
        // The frame of the last golden line that has one: a `setup` line
        // inside the run happened then.
        let mut seen = 0;
        for l in &run.lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            if !matches!(w[0], "setup" | "slots" | "slotent" | "table" | "note") {
                if let Some(f) = w.get(1).and_then(|f| f.parse::<u32>().ok()) {
                    seen = f;
                }
            }
            let rest = |n: usize| {
                let mut s = *l;
                for _ in 0..n {
                    s = s.split_once(' ').map_or("", |x| x.1);
                }
                s.to_string()
            };
            match w[0] {
                "setup" => {
                    if self.entered {
                        self.advance_to(seen);
                    }
                    self.out.push(l.to_string());
                    match w[1] {
                        "wam" => {
                            assert!(self.g.board_begin(w[2], self.spec.world), "{l}");
                            self.view = w[4].parse().unwrap();
                            let a = |s: &str| deg_to_rad_a3(Fpu::Double, s.parse().unwrap());
                            self.angles = (a(w[6]), a(w[8]));
                        }
                        "flag" => {
                            let n: usize = w[2].parse().unwrap();
                            self.g.script_mut().unwrap().flags[n] = w[3].parse().unwrap();
                        }
                        // The tower harness's closing commands: run logged
                        // with the next frame's number, before its step.
                        "cmd" if self.spec.tower && self.entered => {
                            let f = self.frame + 1;
                            let all = rest(2);
                            for c in all.split_inclusive(')') {
                                let c = c.trim();
                                self.out.push(format!("cmd {f} {c}"));
                                self.g.run_command(c);
                                self.drain(f);
                            }
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
                    if self.spec.palace {
                        self.g.board_wam_setup_traced();
                        self.drain(0);
                    } else {
                        self.g.board_wam_setup();
                    }
                    self.out.push(l.to_string());
                    self.g.seed(1);
                    let now = self.now();
                    let (yaw, pitch) = self.angles;
                    self.g.board_enter_at(self.view, yaw, pitch, now);
                    self.drain(0);
                    self.entered = true;
                }
                // A state line (`slots <f> ...`): the snapshot writes it.
                "slots" if w[1].parse::<u32>().is_ok() => {}
                "slots" => {
                    let (a, b) = self.slot_lines();
                    self.slot_names = a.clone();
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
                    self.scan = self.spec.palace;
                    self.drain(f);
                    self.snapshot(f, false);
                }
                // A die thrown in frame f (the tower harness).
                "die" => {
                    let f: u32 = w[1].parse().unwrap();
                    self.advance_to(f - 1);
                    self.out.push(l.to_string());
                    self.die = true;
                }
                // An object click in frame f (the palace harness).
                "objet" => {
                    let f: u32 = w[1].parse().unwrap();
                    self.advance_to(f - 1);
                    self.out.push(l.to_string());
                    self.object = Some(w[2].to_string());
                }
                "end" => {
                    self.advance_to(last_frame);
                    self.out.push(l.to_string());
                }
                // Checked by their own test.
                "table" => self.out.push(l.to_string()),
                "note" => {
                    self.advance_to(seen);
                    self.out.push(l.to_string());
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
fn normalise_golden(l: &str, lang: &str) -> Option<String> {
    let w: Vec<&str> = l.split_whitespace().collect();
    if w.len() == 4 && w[0] == "snd" && w[2] == "stopmus" {
        return None;
    }
    Some(normalise_golden_line(l, lang))
}

fn normalise_golden_line(l: &str, lang: &str) -> String {
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
            .filter_map(|l| normalise_golden(l, &lang))
            .collect();
        assert_same(&format!("run {}", run.name), &want, &p.out);
        names.push(run.name.to_string());
    }
    names
}

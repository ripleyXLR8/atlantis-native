//! The game's end against the original's interpreter
//! (`tests/golden/ending.txt`, `ending_flag33_0.txt`): a new game, then
//! chapter 8's camp (view 16) with the golden's flags, the first key taken
//! from the bag and the car zone `VOITURE` clicked. With flag 33 the
//! cinematics 19 and 92 play, 200 is missing from the install and the
//! section goes on at once, `exit=1` sets `EXIT`, switches to the menus
//! (mode 6) and starts the credits page (atlantis3-gog-fr-3473408
//! @ 0x00449cbf..0x00449cf5) without quitting; without flag 33 cinematic 20
//! plays and `Load=6` finds no quick save. Checked: every `SetVariables`
//! line, the films started, the variables and flags at the end, the mode.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use std::collections::BTreeMap;

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

struct Run {
    g: Game,
    t: f64,
    /// The films started, in order.
    films: Vec<String>,
}

impl Run {
    /// One frame, 1/30 s; the media started end at once, as the harness
    /// ends them.
    fn frame(&mut self) {
        self.t += 1.0 / 30.0;
        self.g.step(self.t);
        self.finish_media();
    }

    fn finish_media(&mut self) {
        for _ in 0..100 {
            if let Some((name, _)) = self.g.video() {
                self.films.push(name.to_ascii_lowercase());
                self.g.finish_video();
            } else if self.g.dialog_running() {
                self.g.finish_dialog();
            } else {
                break;
            }
        }
    }

    fn var(&self, name: &str) -> i32 {
        self.g.script().unwrap().var(name).unwrap()
    }

    /// The harness's settle: frames while in the warp a turn runs, a move
    /// is pending or the arrival scan has not run.
    fn settle(&mut self) {
        for _ in 0..3000 {
            let busy = self.g.mode() == Mode::Warp
                && (self.g.turning() || self.g.move_pending() || self.var("ARRIVEEVUE") != 0);
            if !busy {
                return;
            }
            self.frame();
        }
        panic!("the game did not settle");
    }

    fn id(&self, name: &str) -> i32 {
        self.g
            .names()
            .id(atlantis2_data::defnames::OBJETS, name.as_bytes())
            .unwrap_or_else(|_| panic!("{name}")) as i32
    }

    /// One step of the golden (its `STEP` line).
    fn step(&mut self, line: &str) {
        let rest = line.splitn(3, ' ').nth(2).unwrap();
        let w: Vec<&str> = rest.split_whitespace().collect();
        match w[0] {
            "START" => {
                assert!(self.g.new_game());
                self.finish_media();
                self.settle();
            }
            // `SETUP case=...: (cmd) (cmd) ... flags {f: v, ...}`: the
            // commands through `RunOneCommand`, the flags written.
            "SETUP" => {
                let (cmds, flags) = rest.split_once(" flags ").unwrap();
                for c in cmds.split(' ').filter(|c| c.starts_with('(')) {
                    self.g.run_command(c);
                }
                for kv in flags.trim_matches(|c| c == '{' || c == '}').split(", ") {
                    let (k, v) = kv.split_once(": ").unwrap();
                    let k: usize = k.parse().unwrap();
                    self.g.script_mut().unwrap().flags[k] = v.parse().unwrap();
                }
                self.finish_media();
            }
            "FRAME" => {
                self.frame();
                self.settle();
            }
            "HAND" => {
                self.g.right_click();
                assert!(self.g.inventory_open(), "{line}");
                let id = self.id(w[1]);
                let j = self
                    .g
                    .inventory_items()
                    .iter()
                    .position(|e| e.id == id)
                    .unwrap_or_else(|| panic!("{line}: not in the bag"));
                let (n, margin) = atlantis3_engine::inventory::slots(self.g.width());
                assert!((j as i32) < n, "{line}: scrolling not handled");
                self.g
                    .set_pointer(margin + 80 * j as i32 + 40, self.g.height() as i32 - 40);
                self.g.click();
                self.frame();
                assert!(!self.g.inventory_open(), "{line}");
                self.settle();
            }
            "ZONE" => {
                let name = w[1];
                assert!(
                    self.g.aim_at_zone(|z| z
                        .name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))),
                    "{line}"
                );
                self.g.click();
                self.frame();
                self.settle();
            }
            other => panic!("unknown step {other}"),
        }
    }
}

/// The `SetVariables` lines of a golden from the new game's
/// `(chapter=600)` on, with their step.
fn sets(golden: &str) -> Vec<(usize, &str)> {
    let mut started = false;
    let mut step = 0;
    let mut out = Vec::new();
    for l in golden.lines() {
        if l.starts_with("STEP ") {
            step += 1;
        }
        if let Some(s) = l.strip_prefix("LOG interp: ") {
            if s == "SET(chapter=600)" {
                started = true;
            }
            if started {
                out.push((step, s));
            }
        }
    }
    out
}

/// The variables and flags a golden's run changed, at its end.
fn changed(golden: &str) -> (BTreeMap<String, i32>, BTreeMap<usize, i32>) {
    let mut vars = BTreeMap::new();
    let mut flags = BTreeMap::new();
    for l in golden.lines() {
        let is_vars = l.starts_with("VARS ");
        if !is_vars && !l.starts_with("FLAGS ") {
            continue;
        }
        for kv in l.split_once(": ").map_or("", |x| x.1).split_whitespace() {
            let (k, v) = kv.split_once('=').unwrap();
            let v: i32 = v.parse().unwrap();
            if is_vars {
                vars.insert(k.to_string(), v);
            } else {
                flags.insert(k.parse().unwrap(), v);
            }
        }
    }
    (vars, flags)
}

fn replay(golden: &str) -> Option<Run> {
    let dir = atlantis3_data::testing::install_dir()?;
    let files = A3Files::open_dir(&dir).unwrap().0;
    // The GOG install has no cinematic 200 (its start fails, the section
    // does not wait).
    assert!(files.size("hnm/HNM640/hoggar/cine200.hnm").is_none());
    let mut g = Game::new(files).expect("the game starts");
    g.seed(1);
    g.set_harness_turns(true);
    g.script_mut().unwrap().set_log = Some(Vec::new());
    let mut r = Run {
        g,
        t: 0.0,
        films: Vec::new(),
    };
    for l in golden.lines().filter(|l| l.starts_with("STEP ")) {
        r.step(l);
    }
    // Every SetVariables line.
    let got: Vec<String> =
        r.g.script_mut()
            .unwrap()
            .set_log
            .take()
            .unwrap()
            .into_iter()
            .map(|l| format!("SET{l}"))
            .collect();
    let want = sets(golden);
    for (i, (a, (step, b))) in got.iter().zip(&want).enumerate() {
        assert_eq!(a, b, "SetVariables line {i} (golden step {step})");
    }
    assert_eq!(got.len(), want.len(), "SetVariables lines");
    // The films started (`END hnm 'name'`).
    let films: Vec<String> = golden
        .lines()
        .filter_map(|l| l.strip_prefix("END hnm '"))
        .map(|l| l.split('\'').next().unwrap().to_ascii_lowercase())
        .collect();
    assert_eq!(r.films, films, "films");
    // The variables and flags at the end (the camera's angles are the
    // harness's model).
    let (vars, flags) = changed(golden);
    let s = r.g.script().unwrap();
    for (k, v) in vars.iter().filter(|(k, _)| !k.starts_with("ANGLEWARP")) {
        let k = k.split('@').next().unwrap();
        assert_eq!(s.var(k), Some(*v), "variable {k}");
    }
    for (k, v) in &flags {
        assert_eq!(s.flags[*k], *v, "flag {k}");
    }
    // The end state.
    let end = golden
        .lines()
        .find_map(|l| l.strip_prefix("ENDSTATE "))
        .unwrap();
    let end: BTreeMap<&str, i32> = end
        .split_whitespace()
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap();
            (k, v.parse().unwrap())
        })
        .collect();
    assert_eq!(r.var("EXIT"), end["EXIT"], "EXIT");
    let mode = match r.g.mode() {
        Mode::Menu => 6,
        Mode::Warp => 3,
        m => panic!("mode {m:?}"),
    };
    assert_eq!(mode, end["MODE"], "loop mode");
    assert_eq!(r.g.quit, end["QUIT"] != 0, "quit");
    Some(r)
}

#[test]
fn the_end_shows_the_credits() {
    let Some(mut r) = replay(include_str!("golden/ending.txt")) else {
        return;
    };
    // The credits' start (@ 0x00457690): picture 1 and page 0.
    let c = r.g.credits().expect("the credits page");
    assert_eq!(c.shown_names(), ["IMGFOND1", "PAGE0"]);
    assert_eq!(c.slide, 1);
    // Mode 6 runs no warp frame: `Interp_GereFrame` never sees `EXIT`,
    // the game does not quit.
    for _ in 0..30 {
        r.frame();
    }
    assert_eq!(r.g.mode(), Mode::Menu);
    assert!(!r.g.quit);
}

#[test]
fn the_wrong_end_stays_in_the_camp() {
    let Some(r) = replay(include_str!("golden/ending_flag33_0.txt")) else {
        return;
    };
    assert!(r.g.credits().is_none());
}

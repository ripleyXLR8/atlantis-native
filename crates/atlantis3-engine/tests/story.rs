//! The story played headless, world by world, through the game's own API,
//! against the original's interpreter playing the same walks
//! (`tests/golden/story/NN_<segment>.walk` and `.txt`): every
//! `SetVariables` line in order, the walks' checks, the variables and flags
//! at the end of each segment. Each segment starts from the original's save
//! at the end of the previous one (its `CHECKPOINT`), as its golden does;
//! [`whole_story`] chains them all from the new game to the ending.
//! Cinematics, travel videos and dialogs are ended as soon as they start,
//! as the original's run ended them; puzzles are replaced by their result
//! (the walks' `SKIP` lines); `rand()` gives the original run's values.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use std::collections::BTreeMap;

use atlantis2_data::defnames;
use atlantis3_data::A3Files;
use atlantis3_engine::subjects::Layout;
use atlantis3_engine::{Game, Mode};

/// The segments: walk, golden, the checkpoint save its golden starts from.
const SEGMENTS: &[(&str, &str, &str)] = &[
    (
        "01_hoggar",
        include_str!("golden/story/01_hoggar.walk"),
        include_str!("golden/story/01_hoggar.txt"),
    ),
    (
        "02_cfuner",
        include_str!("golden/story/02_cfuner.walk"),
        include_str!("golden/story/02_cfuner.txt"),
    ),
    (
        "03_ldm",
        include_str!("golden/story/03_ldm.walk"),
        include_str!("golden/story/03_ldm.txt"),
    ),
    (
        "04_hoggar5",
        include_str!("golden/story/04_hoggar5.walk"),
        include_str!("golden/story/04_hoggar5.txt"),
    ),
    (
        "05_bagdad",
        include_str!("golden/story/05_bagdad.walk"),
        include_str!("golden/story/05_bagdad.txt"),
    ),
    (
        "06_paleo",
        include_str!("golden/story/06_paleo.walk"),
        include_str!("golden/story/06_paleo.txt"),
    ),
    (
        "07_rhea",
        include_str!("golden/story/07_rhea.walk"),
        include_str!("golden/story/07_rhea.txt"),
    ),
    (
        "08_ending",
        include_str!("golden/story/08_ending.walk"),
        include_str!("golden/story/08_ending.txt"),
    ),
];

struct Run {
    g: Game,
    t: f64,
    expects: usize,
    /// `rand()` values of the golden, by step.
    rands: BTreeMap<usize, Vec<i32>>,
    step: usize,
    /// The checkpoint saves written (name → text).
    saves: BTreeMap<String, Vec<u8>>,
}

impl Run {
    fn frame(&mut self) {
        self.t += 1.0 / 30.0;
        self.g.step(self.t);
        if self.g.video().is_some() {
            self.g.finish_video();
        }
        if self.g.dialog_running() {
            self.g.finish_dialog();
        }
    }

    /// Frames until nothing moves: no turn, no pending move, no video, no
    /// dialog, three frames in a row.
    fn settle(&mut self) {
        let mut calm = 0;
        for _ in 0..3000 {
            self.frame();
            let still = matches!(self.g.mode(), Mode::Warp | Mode::Subjects | Mode::Screen)
                && !self.g.turning()
                && !self.g.move_pending()
                && self.g.video().is_none()
                && !self.g.dialog_running();
            calm = if still { calm + 1 } else { 0 };
            if calm >= 3 {
                return;
            }
        }
        panic!("the game did not settle");
    }

    fn id(&self, section: usize, name: &str) -> i32 {
        self.g
            .names()
            .id(section, name.as_bytes())
            .unwrap_or_else(|_| panic!("{name}")) as i32
    }

    fn var(&self, name: &str) -> i32 {
        self.g.script().unwrap().var(name).unwrap()
    }

    fn action(&mut self, line: &str) {
        let w: Vec<&str> = line.split_whitespace().collect();
        self.step += 1;
        for &v in self.rands.get(&self.step).into_iter().flatten() {
            self.g.queue_rand(v);
        }
        match w[0] {
            "START" => {
                assert!(self.g.new_game());
                self.settle();
            }
            "GO" => {
                let v: i32 = w[1].parse().unwrap();
                assert!(
                    self.g.aim_at_zone(|z| z.kind == 1 && z.value == v),
                    "{line}: no zone to {v} under the centre"
                );
                self.g.click();
                self.settle();
            }
            "PERSO" | "OBJET" => {
                let (section, kind) = if w[0] == "PERSO" {
                    (defnames::PERSONNAGES, 1)
                } else {
                    (defnames::OBJETS, 2)
                };
                let id = self.id(section, w[1]);
                assert!(self.g.click_sprite(id, kind), "{line}: not shown");
                self.frame();
                self.settle();
            }
            "SUBJECT" => {
                assert!(self.g.subjects_open(), "{line}: no subject menu");
                let value = {
                    let names = self.g.names();
                    let i = names.id(defnames::SUJETS, w[1].as_bytes()).unwrap();
                    names.param_of(defnames::SUJETS, i)
                };
                let s = self.g.script().unwrap();
                let perso = s.var("CLICPERSO").unwrap();
                let lay = Layout::for_width(self.g.width());
                let cell = lay
                    .cells(&s.subjects.list(perso))
                    .into_iter()
                    .find(|c| c.value == value)
                    .unwrap_or_else(|| panic!("{line}: not offered"));
                self.g.set_pointer(cell.x, cell.y);
                self.g.click();
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
                self.settle();
            }
            "HAND" => {
                self.g.right_click();
                self.frame();
                assert!(self.g.inventory_open());
                if w[1] == "-" {
                    self.g.right_click();
                    self.settle();
                    return;
                }
                let id = self.id(defnames::OBJETS, w[1]);
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
                assert!(!self.g.inventory_open());
                self.settle();
            }
            "FRAME" => {
                let n: usize = w.get(1).map_or(1, |n| n.parse().unwrap());
                for _ in 0..n {
                    self.frame();
                }
                self.settle();
            }
            "WAIT" => {
                let s: f64 = w[1].parse().unwrap();
                let n = (s / 0.04).round() as usize;
                for _ in 0..n {
                    self.frame();
                }
            }
            "RAND" => self.g.queue_rand(w[1].parse().unwrap()),
            "CLICK" => {
                let left = w[1] == "left";
                let sprite = w
                    .get(2)
                    .and_then(|s| s.strip_prefix("SPRITE="))
                    .map(|n| n.parse().unwrap());
                self.g.screen_click(left, sprite);
                self.settle();
            }
            "MAP" => {
                self.g.screen_click(true, Some(w[1].parse().unwrap()));
                self.settle();
            }
            "SKIP" => self.skip(&w[3..]),
            "CHECKPOINT" => {
                let n: i32 = w[1].parse().unwrap();
                let name = self.g.quick_save_now(n);
                let text = self.g.saved(&name).expect("checkpoint saved").to_vec();
                self.saves.insert(w[2].to_string(), text);
            }
            "EXPECT" => self.expect(line, w[1]),
            other => panic!("unknown action {other}"),
        }
    }

    fn skip(&mut self, items: &[&str]) {
        let mut frames = 1;
        for a in items {
            if *a == "CTRL" {
                self.g.ctrl_scenar_in();
                continue;
            }
            if *a == "NOFRAME" {
                frames = 0;
                continue;
            }
            let (k, v) = a.split_once('=').unwrap();
            if let Some(f) = k.strip_prefix("FLAG") {
                let f: usize = f.parse().unwrap();
                self.g.script_mut().unwrap().flags[f] = v.parse().unwrap();
            } else if let Some(name) = k.strip_prefix("VAR:") {
                self.g.set_var(name, v.parse().unwrap());
            } else if k == "RUN" {
                self.g.run_command(&v.replace('_', " "));
            } else if k == "CALL" {
                let a = u32::from_str_radix(v.trim_start_matches("0x"), 16).unwrap();
                assert!(self.g.test_call(a), "SKIP {a:#x} not modelled");
            } else if k == "MEM8" || k == "MEM32" {
                let (a, n) = v.split_once(':').unwrap();
                let a = u32::from_str_radix(a.trim_start_matches("0x"), 16).unwrap();
                assert!(
                    self.g.test_poke(a, n.parse().unwrap()),
                    "SKIP {a:#x} not modelled"
                );
            } else if k == "FRAMES" {
                frames = v.parse().unwrap();
            } else if matches!(k, "SHOWOBJET" | "HIDEOBJET" | "SHOWPERSO" | "HIDEPERSO") {
                let kind = if k.ends_with("OBJET") { 2 } else { 1 };
                if k.starts_with("SHOW") {
                    self.g.show_entity(kind, v);
                } else {
                    self.g.hide_entity(kind, v);
                }
            } else {
                panic!("SKIP item {a} not handled");
            }
        }
        if frames == 0 {
            // The media the items started end at once, without a frame
            // (the harness ends them inside the step).
            for _ in 0..100 {
                if self.g.video().is_some() {
                    self.g.finish_video();
                } else if self.g.dialog_running() {
                    self.g.finish_dialog();
                } else {
                    break;
                }
            }
            return;
        }
        for _ in 0..frames {
            self.frame();
        }
        self.settle();
    }

    fn expect(&mut self, line: &str, kv: &str) {
        let (k, v) = kv.split_once('=').unwrap();
        let s = self.g.script().unwrap();
        let got = match k {
            "WAM" => self.g.place().0.to_string(),
            "MODE" => self
                .g
                .puzzle_screen()
                .map_or(3, |s| s.mode_number())
                .to_string(),
            "HAND" => {
                let h = s.var("OBJETENMAIN").unwrap();
                if h == 0 {
                    "-".into()
                } else {
                    String::from_utf8_lossy(
                        self.g.names().name(defnames::OBJETS, h as usize).unwrap(),
                    )
                    .into_owned()
                }
            }
            "MONDE" => s.a3.world.to_string(),
            _ => match k.strip_prefix("FLAG") {
                Some(f) => s.flags[f.parse::<usize>().unwrap()].to_string(),
                None => self.var(k).to_string(),
            },
        };
        assert_eq!(got, v, "{line}");
        self.expects += 1;
    }
}

/// The `rand()` values of each step of a golden.
fn rands(golden: &str) -> BTreeMap<usize, Vec<i32>> {
    let mut out: BTreeMap<usize, Vec<i32>> = BTreeMap::new();
    let mut step = 0;
    for l in golden.lines() {
        if l.starts_with("STEP ") {
            step += 1;
        } else if let Some(v) = l.strip_prefix("CALL rand() -> ") {
            // A value the walk forced (`RAND n`) is queued by its action.
            if !v.ends_with("(walk)") {
                let n = v.split_whitespace().next().unwrap();
                out.entry(step).or_default().push(n.parse().unwrap());
            }
        }
    }
    out
}

/// The `SetVariables` lines of a golden from its start: the new game's
/// `(chapter=600)` or the load of the segment's save.
fn sets(golden: &str) -> Vec<&str> {
    let mut started = false;
    let mut out = Vec::new();
    for l in golden.lines() {
        if l.starts_with("FRAME load") {
            started = true;
        }
        if let Some(s) = l.strip_prefix("LOG interp: ") {
            if s == "SET(chapter=600)" {
                started = true;
            }
            if started {
                out.push(s);
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
        let rest = if let Some(r) = l.strip_prefix("VARS ") {
            r
        } else if let Some(r) = l.strip_prefix("FLAGS ") {
            r
        } else {
            continue;
        };
        for kv in rest.split_once(": ").map_or("", |x| x.1).split_whitespace() {
            let (k, v) = kv.split_once('=').unwrap();
            let v: i32 = v.parse().unwrap();
            if l.starts_with("VARS") {
                vars.insert(k.to_string(), v);
            } else {
                flags.insert(k.parse().unwrap(), v);
            }
        }
    }
    (vars, flags)
}

fn new_run() -> Option<Run> {
    let dir = atlantis3_data::testing::install_dir()?;
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut g = Game::new(files).expect("the game starts");
    g.seed(1);
    Some(Run {
        g,
        t: 0.0,
        expects: 0,
        rands: BTreeMap::new(),
        step: 0,
        saves: BTreeMap::new(),
    })
}

/// Plays one segment's walk on `r` and checks it against its golden.
fn play(r: &mut Run, name: &str, walk: &str, golden: &str) {
    r.rands = rands(golden);
    r.step = 0;
    let s = r.g.script_mut().unwrap();
    if s.set_log.is_none() {
        s.set_log = Some(Vec::new());
    }
    for line in walk.lines() {
        let line = line.split('#').next().unwrap().trim();
        if !line.is_empty() {
            r.action(line);
        }
    }
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
    for (i, (a, b)) in got.iter().zip(&want).enumerate() {
        assert_eq!(a, b, "{name}: SetVariables line {i}");
    }
    assert_eq!(got.len(), want.len(), "{name}: SetVariables lines");
    let (vars, flags) = changed(golden);
    let s = r.g.script().unwrap();
    // `ANGLEWARPX` / `ANGLEWARPY` come from the harness's camera model.
    for (k, v) in vars.iter().filter(|(k, _)| !k.starts_with("ANGLEWARP")) {
        let k = k.split('@').next().unwrap();
        assert_eq!(s.var(k), Some(*v), "{name}: variable {k}");
    }
    for (k, v) in &flags {
        assert_eq!(s.flags[*k], *v, "{name}: flag {k}");
    }
}

/// The save a segment's golden starts from (its `SAVEFILE` lines at the
/// start), or none for the new game.
fn start_save(golden: &str) -> Option<Vec<u8>> {
    let mut text = String::new();
    for l in golden.lines() {
        if l.starts_with("STEP ") {
            break;
        }
        if let Some(s) = l.strip_prefix("SAVEFILE ") {
            text.push_str(s);
            text.push('\n');
        }
    }
    (!text.is_empty()).then(|| text.into_bytes())
}

fn segment(index: usize) {
    let Some(mut r) = new_run() else {
        return;
    };
    let (name, walk, golden) = SEGMENTS[index];
    if let Some(save) = start_save(golden) {
        r.g.script_mut().unwrap().set_log = Some(Vec::new());
        assert!(r.g.load_save_text("quick1_100.sav", save), "{name}: load");
    }
    play(&mut r, name, walk, golden);
}

#[test]
fn story_01_hoggar() {
    segment(0);
}

#[test]
fn story_02_cfuner() {
    segment(1);
}

#[test]
fn story_03_ldm() {
    segment(2);
}

#[test]
fn story_04_hoggar5() {
    segment(3);
}

#[test]
fn story_05_bagdad() {
    segment(4);
}

#[test]
fn story_06_paleo() {
    segment(5);
}

#[test]
fn story_07_rhea() {
    segment(6);
}

#[test]
fn story_08_ending() {
    segment(7);
}

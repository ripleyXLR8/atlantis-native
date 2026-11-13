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
//! The `played_*` tests replay the same story with every puzzle solved
//! from the player's input ([`played`]).
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
    /// Escape pressed during each travel video, after its first frame (as
    /// the original's run made Escape's two calls), then the menu left as
    /// its `BT_BACK` leaves it; the travel videos escaped.
    escape_travels: bool,
    escaped: usize,
    /// The played walks ([`played`]): the played harness's clock (1.0025 /
    /// 15 s a frame), the frame counts of the golden's steps, the `rand()`
    /// generator's state expected at each step's end.
    played: bool,
    frames: BTreeMap<usize, usize>,
    lcg: BTreeMap<usize, u32>,
    lcg_vals: BTreeMap<usize, Vec<i32>>,
    lcg_from: u32,
    /// A `CAMERA` action set the aim of the next `ZONE`.
    camera_aimed: bool,
    /// The played goldens' per-step state ([`StepRef`]) and the script's
    /// variables and flags at the previous step's end.
    refs: BTreeMap<usize, StepRef>,
    prev: Option<(Vec<i32>, Vec<i32>)>,
    /// The frames run since the step began.
    step_frames: usize,
}

/// What a played golden records of a step: the frames it ran (`FRAME`
/// lines but `FRAME load`), and after it the `STATE` line, the loop mode
/// (`MODE`, 3 when absent), the variables and flags that changed (`VARS`,
/// `FLAGS`) and a checkpoint's save (`SAVEFILE` lines).
#[derive(Default)]
struct StepRef {
    frames: usize,
    state: Option<String>,
    mode: Option<i32>,
    vars: BTreeMap<String, i32>,
    flags: BTreeMap<usize, i32>,
    save: Option<Vec<String>>,
}

/// The per-step records of a played golden.
fn step_refs(golden: &str) -> BTreeMap<usize, StepRef> {
    let mut out: BTreeMap<usize, StepRef> = BTreeMap::new();
    let mut step = 0;
    let mut in_save = false;
    let kv = |rest: &str| -> Vec<(String, i32)> {
        rest.split_once(": ")
            .map_or("", |x| x.1)
            .split_whitespace()
            .map(|kv| {
                let (k, v) = kv.split_once('=').unwrap();
                (k.to_string(), v.parse().unwrap())
            })
            .collect()
    };
    for l in golden.lines() {
        if l.starts_with("STEP ") {
            step += 1;
            in_save = false;
            continue;
        }
        let r = out.entry(step).or_default();
        if let Some(rest) = l.strip_prefix("SAVEFILE ") {
            if in_save {
                r.save.get_or_insert_with(Vec::new).push(rest.to_string());
            }
            continue;
        }
        in_save = false;
        if l.starts_with("CHECKPOINT ") {
            in_save = true;
            r.save = Some(Vec::new());
        } else if l.starts_with("FRAME ") && !l.starts_with("FRAME load") {
            r.frames += 1;
        } else if let Some(rest) = l.strip_prefix("STATE ") {
            r.state = Some(rest.split_once(": ").unwrap().1.to_string());
        } else if let Some(rest) = l.strip_prefix("MODE ") {
            let m = rest.split_once(": ").unwrap().1;
            r.mode = Some(m.split_whitespace().next().unwrap().parse().unwrap());
        } else if l.starts_with("VARS ") {
            r.vars.extend(kv(l));
        } else if l.starts_with("FLAGS ") {
            r.flags
                .extend(kv(l).into_iter().map(|(k, v)| (k.parse().unwrap(), v)));
        }
    }
    out
}

/// The played harness's frame: 1.0025 / 15 s.
const PLAYED_DT: f64 = 1.0025 / 15.0;

impl Run {
    /// One frame, 1/30 s.
    fn frame(&mut self) {
        let dt = if self.played { PLAYED_DT } else { 1.0 / 30.0 };
        self.frame_dt(dt);
    }

    fn frame_dt(&mut self, dt: f64) {
        self.step_frames += 1;
        self.t += dt;
        let before = self.g.rand_state();
        self.g.step(self.t);
        self.finish_media();
        if std::env::var_os("STORY_FRAMES").is_some() {
            let mut r = atlantis2_engine::crt::Rand::new(before);
            let mut draws = Vec::new();
            while r.state != self.g.rand_state() && draws.len() < 50 {
                draws.push(r.rand());
            }
            let slots: Vec<String> = self
                .g
                .slots()
                .slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.shown || std::env::var_os("STORY_ALL").is_some())
                .map(|(k, s)| {
                    format!(
                        "{k}:{}/{}/{} sh{} f{} l{} n{} m{} an{} fr{} w{} p{} d{} e{}",
                        s.name,
                        s.id,
                        s.kind,
                        u8::from(s.shown),
                        s.frame,
                        s.last,
                        s.count,
                        s.mode,
                        u8::from(s.animated),
                        u8::from(s.fresh),
                        u8::from(s.waiting),
                        s.pause,
                        s.state.delay,
                        u8::from(s.ended)
                    )
                })
                .collect();
            eprintln!(
                "  frame step {} t={:.4} {:?} view {:?} rand {:?} slots [{}]",
                self.step,
                self.t,
                self.g.mode(),
                self.g.place().1,
                draws,
                slots.join(", ")
            );
        }
    }

    /// The media started end at once, as the harness ends them.
    fn finish_media(&mut self) {
        for _ in 0..100 {
            if self.escape_travels && self.g.video_is_travel() {
                self.escape_travel();
            } else if self.g.video().is_some() {
                self.g.finish_video();
            } else if self.g.dialog_running() {
                self.g.finish_dialog();
            } else {
                break;
            }
        }
    }

    /// Escape during a travel video, after its first frame: the video ends,
    /// the section's tail runs again, the in-game menu opens; then back to
    /// the game.
    fn escape_travel(&mut self) {
        for _ in 0..10 {
            if self.g.video().is_none_or(|v| v.1 > 0) {
                break;
            }
            self.t += 1.0 / 15.0;
            self.g.step(self.t);
        }
        if !self.g.video_is_travel() {
            return;
        }
        self.g.escape();
        assert_eq!(self.g.mode(), Mode::Menu, "Escape opens the in-game menu");
        assert_eq!(self.g.menu_page().map(|p| p.0), Some(1));
        // The golden's run goes on in the warp as after the back to the
        // game (@ 0x00451280): the menu itself is not modelled there, so
        // neither is page 1's view re-entry (@ 0x0043fd00, whose view hook
        // would log one more `LIMITEANGLEWARPY=70`; `tests/menu_back.rs`).
        self.g.menu_resume();
        assert_eq!(self.g.mode(), Mode::Warp);
        self.escaped += 1;
    }

    /// The pointer on the running screen's sprite `k`, as the walks name
    /// it (`MAP k`, `CLICK ... SPRITE=k`), so the click that follows is a
    /// real one, found by the screen's own hit test. The points are the
    /// original's (800 x 600): the map's `clickpoint` lines
    /// (`tests/golden/map_carte.txt`, sprite k = chapter 12, 14, 13, 15,
    /// 16), the jewel case's and the remote's `hit` points
    /// (`tests/golden/puzzle_ecrin.txt`, `tests/golden/puzzle_telecommande.txt`).
    fn point_at(&mut self, k: i32) {
        use atlantis3_engine::game::Screen;
        assert_eq!((self.g.width(), self.g.height()), (800, 600));
        let p = match (self.g.puzzle_screen(), k) {
            (Some(Screen::Carte), 1) => (400, 310),
            (Some(Screen::Carte), 2) => (370, 410),
            (Some(Screen::Carte), 3) => (320, 190),
            (Some(Screen::Carte), 4) => (640, 390),
            (Some(Screen::Carte), 5) => (520, 340),
            (Some(Screen::Ecrin), 1) => (312, 136),
            (Some(Screen::Ecrin), 2) => (304, 232),
            (Some(Screen::Ecrin), 3) => (192, 224),
            (Some(Screen::Telecommande), 0) => (328, 160),
            (Some(Screen::Telecommande), 14) => (352, 384),
            (Some(Screen::Telecommande), 29) => (536, 248),
            (Some(Screen::Telecommande), 44) => (368, 248),
            (s, k) => panic!("no point for sprite {k} of {s:?}"),
        };
        self.g.set_pointer(p.0, p.1);
    }

    /// The harness's settle: frames while in the warp a turn runs, a move
    /// is pending or the arrival scan has not run (`ARRIVEEVUE` ≠ 0).
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

    /// An action's frame then the settle.
    fn act_frame(&mut self) {
        self.frame();
        self.settle();
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
        // `EXPECT` and `RAND` are no steps of the golden.
        if !matches!(w[0], "EXPECT" | "RAND") {
            self.step += 1;
            self.step_frames = 0;
            for &v in self.rands.get(&self.step).into_iter().flatten() {
                self.g.queue_rand(v);
            }
        }
        match w[0] {
            "START" if self.played => {
                // The harness's clock at the start-up: 100 s.
                self.t = 100.0;
                self.g.set_clock(self.t);
                self.g.seed(1);
                self.lcg_from = 1;
                assert!(self.g.new_game());
                self.settle();
            }
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
                self.act_frame();
            }
            "PERSO" | "OBJET" => {
                let (section, kind) = if w[0] == "PERSO" {
                    (defnames::PERSONNAGES, 1)
                } else {
                    (defnames::OBJETS, 2)
                };
                let id = self.id(section, w[1]);
                // A sprite still playing an animation that cannot be
                // clicked: the player waits for the next one (the
                // harness, without place animations, clicks at once).
                let mut found = self.g.click_sprite(id, kind);
                // The played walks wait for it themselves.
                assert!(found || !self.played, "{line}: not under any aim");
                for _ in 0..300 {
                    if found {
                        break;
                    }
                    self.frame_dt(1.0 / 15.0);
                    found = self.g.click_sprite(id, kind);
                }
                assert!(found, "{line}: not shown");
                if std::env::var_os("STORY_DEBUG").is_some() {
                    eprintln!(
                        "{line}: under centre {:?}, hand {}",
                        self.g.sprite_under_centre(),
                        self.g.object_in_hand()
                    );
                }
                self.act_frame();
                if std::env::var_os("STORY_DEBUG").is_some() {
                    let s = self.g.script().unwrap();
                    eprintln!("  after: bag {:?} hand {} sacados {:?} mode {:?} copy_waiting {} resume {}", s.bag.iter().map(|e| e.id).collect::<Vec<_>>(), self.g.object_in_hand(), s.var("SACADOS"), self.g.mode(), s.copy_waiting, s.resume_pending);
                }
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
                self.act_frame();
            }
            "ZONE" => {
                let name = w[1];
                let is = |z: &atlantis2_data::wam::Zone| {
                    z.name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                };
                // Already under the centre: no turn. After a `CAMERA` aim
                // the named zone must be the one under the centre.
                let aimed = std::mem::take(&mut self.camera_aimed);
                let (_, v) = self.g.place();
                let here = v
                    .and_then(|v| self.g.wam().view(v))
                    .and_then(|v| v.zones.get(self.g.centre_zone()?))
                    .is_some_and(is);
                if std::env::var_os("STORY_FRAMES").is_some() {
                    eprintln!(
                        "  {line}: under the centre {here} ({:?}), angles {:?}",
                        self.g.centre_zone(),
                        self.g.angles_deg()
                    );
                }
                if aimed {
                    assert!(here, "{line}: not under the aim {:?}", self.g.angles_deg());
                } else {
                    assert!(here || self.g.aim_at_zone(is), "{line}");
                }
                self.g.click();
                self.act_frame();
            }
            "HAND" => {
                self.g.right_click();
                // The media the opening's scan started end before the
                // inventory's frame, as the harness ends them.
                self.finish_media();
                assert!(self.g.inventory_open());
                if w[1] == "-" {
                    self.frame();
                    self.g.right_click();
                    self.settle();
                    return;
                }
                let id = self.id(defnames::OBJETS, w[1]);
                let j = self.g.inventory_items().iter().position(|e| e.id == id);
                let Some(j) = j.filter(|_| !self.played).or(j) else {
                    // The played harness picks an object that is not in
                    // the bag as well (the hunt's spear): the inventory
                    // closes and `SetObjetEnMain` refuses the id ("id
                    // invalide"), the hand stays empty; one frame.
                    assert!(self.played, "{line}: not in the bag");
                    self.g.close_inventory_now();
                    self.frame();
                    assert!(!self.g.inventory_open());
                    self.settle();
                    return;
                };
                let (n, margin) = atlantis3_engine::inventory::slots(self.g.width());
                assert!((j as i32) < n, "{line}: scrolling not handled");
                self.g
                    .set_pointer(margin + 80 * j as i32 + 40, self.g.height() as i32 - 40);
                self.g.click();
                self.frame();
                assert!(!self.g.inventory_open());
                self.settle();
            }
            "FRAME" | "WAIT" | "FRAMES_UNTIL" if self.played => {
                // The golden's frames of the step (`FRAME k.j`).
                let n = self.frames.get(&self.step).copied().unwrap_or(0);
                if w[0] == "FRAME" {
                    let want: usize = w.get(1).map_or(1, |n| n.parse().unwrap());
                    assert_eq!(n, want, "{line}: the golden's frames");
                }
                for i in 0..n {
                    // `FRAMES_UNTIL`: the condition is first true after the
                    // golden's frames (when the port can read it).
                    if w[0] == "FRAMES_UNTIL" {
                        assert_ne!(self.cond(w[1]), Some(true), "{line}: true after {i} frames");
                    }
                    self.frame();
                }
                if w[0] == "FRAMES_UNTIL" {
                    assert_ne!(
                        self.cond(w[1]),
                        Some(false),
                        "{line}: false after {n} frames"
                    );
                }
                self.settle();
            }
            "CAMERA" => {
                // As the `/set` handler converts: degrees × π_f32 ×
                // (1/180)_f32, rounded to f32.
                let rad = |d: &str| {
                    let d: f64 = d.parse().unwrap();
                    (d * f64::from(std::f32::consts::PI) * f64::from(1.0f32 / 180.0)) as f32
                };
                self.g.board_camera(rad(w[1]), rad(w[2]));
                self.camera_aimed = true;
            }
            "POINTER" => {
                self.g
                    .set_pointer(w[1].parse().unwrap(), w[2].parse().unwrap());
            }
            "CLICK" | "MAP" if self.played => {
                let mut left = true;
                if w[0] == "MAP" {
                    self.point_at(w[1].parse().unwrap());
                } else {
                    left = w[1] == "left";
                    for item in &w[2..] {
                        let (k, v) = item.split_once('=').unwrap();
                        match k {
                            "SPRITE" => self.point_at(v.parse().unwrap()),
                            "POINTER" => {
                                let (x, y) = v.split_once(',').unwrap();
                                self.g.set_pointer(x.parse().unwrap(), y.parse().unwrap());
                            }
                            _ => panic!("{line}: {k}"),
                        }
                    }
                }
                if left {
                    self.g.click();
                } else {
                    self.g.right_click();
                }
                self.frame();
                self.settle();
            }
            "CHECKPOINT" if self.played => {
                // The quick save, then what a run from it does after its
                // start-up: clock 100000 (n − 100) s, `rand` state n, the
                // quick load and one frame.
                let n: i32 = w[1].parse().unwrap();
                let name = self.g.quick_save_now(n);
                let text = self.g.saved(&name).expect("checkpoint saved").to_vec();
                self.saves.insert(w[2].to_string(), text.clone());
                self.reload(n, &name, text);
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
                    self.frame_dt(0.04);
                }
                self.settle();
            }
            // The golden's values are queued by step, the walk's included.
            "RAND" => {}
            "CLICK" => {
                let left = w[1] == "left";
                if let Some(n) = w.get(2).and_then(|s| s.strip_prefix("SPRITE=")) {
                    self.point_at(n.parse().unwrap());
                }
                self.g.screen_click(left);
                self.finish_media();
                self.settle();
            }
            "MAP" => {
                self.point_at(w[1].parse().unwrap());
                self.g.screen_click(true);
                self.finish_media();
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

    /// A quick load as the played harness's checkpoint and segment start
    /// make it.
    fn reload(&mut self, n: i32, name: &str, text: Vec<u8>) {
        self.t = 100000.0 * f64::from(n - 100);
        self.g.set_clock(self.t);
        self.g.seed(n as u32);
        self.lcg_from = n as u32;
        assert!(self.g.load_save_text(name, text), "load {name}");
        self.finish_media();
        self.act_frame();
    }

    /// A `FRAMES_UNTIL` condition as the played harness tests it, when the
    /// port can read all its terms: `FLAGn=v`, `MODE=n`, `|`, `&` (`None`
    /// for a puzzle's C global, `SLOTFRAME`, `SHOWN`, `CLICKABLE`).
    fn cond(&self, c: &str) -> Option<bool> {
        if c.contains('|') {
            let v: Option<Vec<bool>> = c.split('|').map(|x| self.cond(x)).collect();
            return v.map(|v| v.into_iter().any(|b| b));
        }
        if c.contains('&') {
            let v: Option<Vec<bool>> = c.split('&').map(|x| self.cond(x)).collect();
            return v.map(|v| v.into_iter().all(|b| b));
        }
        let (k, v) = c.split_once('=')?;
        if let Some(f) = k.strip_prefix("FLAG") {
            let f: usize = f.parse().ok()?;
            return Some(self.g.script()?.flags[f] == v.parse::<i32>().ok()?);
        }
        if k == "MODE" {
            return Some(self.mode_number() == v.parse::<i32>().ok()?);
        }
        None
    }

    /// The script's variables and flags now.
    fn snapshot(&self) -> (Vec<i32>, Vec<i32>) {
        let s = self.g.script().unwrap();
        (s.vars.clone(), s.flags.clone())
    }

    /// The variables' names as the played harness prints them: the
    /// `#VARIABLES#` name, `NAME@index` for a later one of equal names
    /// (the game's lookup finds the first), `#index` for an empty one.
    fn var_names(&self) -> Vec<String> {
        let names = self.g.names();
        let mut seen = std::collections::BTreeSet::new();
        (0..names.len(defnames::VARIABLES))
            .map(|i| {
                let n = String::from_utf8_lossy(names.name(defnames::VARIABLES, i).unwrap_or(b""))
                    .into_owned();
                let u = if n.is_empty() {
                    format!("#{i}")
                } else if seen.contains(&n) {
                    format!("{n}@{i}")
                } else {
                    n.clone()
                };
                seen.insert(n);
                u
            })
            .collect()
    }

    /// The loop mode's number (@ 0x00789af0).
    fn mode_number(&self) -> i32 {
        match self.g.mode() {
            Mode::Intro => 1,
            Mode::Warp => 3,
            Mode::Video => 4,
            Mode::Dialog => 5,
            Mode::Menu => 6,
            Mode::Inventory => 7,
            Mode::Subjects => 8,
            Mode::Screen => self.g.puzzle_screen().map_or(-1, |s| s.mode_number()),
            Mode::Puzzle2004 => self.g.puzzle_2004().map_or(-1, |p| p.mode() as i32),
            Mode::Carte2004 => 10,
            Mode::Idle => 0,
        }
    }

    /// The played golden's record of this step against the port: the
    /// `STATE` line, the loop mode, the variables and flags the step
    /// changed (exactly those, with their values) and a checkpoint's save.
    fn check_step(&mut self, name: &str, line: &str) {
        let Some(r) = self.refs.get(&self.step) else {
            return;
        };
        let mut bad = Vec::new();
        if self.step_frames != r.frames {
            bad.push(format!(
                "frames: golden {}, native {}",
                r.frames, self.step_frames
            ));
        }
        let s = self.g.script().unwrap();
        if let Some(want) = &r.state {
            let h = s.var("OBJETENMAIN").unwrap();
            let hand = if h == 0 {
                "-".to_string()
            } else {
                String::from_utf8_lossy(self.g.names().name(defnames::OBJETS, h as usize).unwrap())
                    .into_owned()
            };
            let (wam, view) = self.g.place();
            let got = format!(
                "chapter={} monde={} wam={} view={} hand={} waiting={} music={}",
                s.chapter,
                s.a3.world,
                wam,
                view.map_or(-1, |v| v as i64),
                hand,
                u8::from(s.resume_pending),
                String::from_utf8_lossy(&s.music)
            );
            if &got != want {
                bad.push(format!("STATE\n  golden {want}\n  native {got}"));
            }
        }
        let mode = self.mode_number();
        if mode != r.mode.unwrap_or(3) {
            bad.push(format!(
                "MODE: golden {:?}, native {mode}",
                r.mode.unwrap_or(3)
            ));
        }
        let now = self.snapshot();
        let (pv, pf) = self
            .prev
            .take()
            .unwrap_or_else(|| (vec![0; now.0.len()], vec![0; now.1.len()]));
        // Not compared: `ANGLEWARPX` / `ANGLEWARPY` (the harness's camera
        // model: `CAMERA` sets it, its re-alignment does not turn) and the
        // tick task's `RANDOM`, `TIME1`, `TIME2` (0x0043e550 is not in the
        // harness's frame).
        let skip = |n: &str| {
            matches!(
                n,
                "ANGLEWARPX" | "ANGLEWARPY" | "RANDOM" | "TIME1" | "TIME2"
            )
        };
        let names = self.var_names();
        let got: BTreeMap<String, i32> = (0..names.len())
            .filter(|&i| now.0[i] != pv[i] && !skip(&names[i]))
            .map(|i| (names[i].clone(), now.0[i]))
            .collect();
        let want: BTreeMap<String, i32> = r
            .vars
            .iter()
            .filter(|(k, _)| !skip(k))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        if got != want {
            bad.push(format!("VARS\n  golden {want:?}\n  native {got:?}"));
        }
        let got: BTreeMap<usize, i32> = (0..now.1.len())
            .filter(|&i| now.1[i] != pf[i])
            .map(|i| (i, now.1[i]))
            .collect();
        if got != r.flags {
            bad.push(format!("FLAGS\n  golden {:?}\n  native {got:?}", r.flags));
        }
        if let Some(want) = &r.save {
            let words: Vec<&str> = line.split_whitespace().collect();
            let text = self.saves.get(words[2]).expect("the checkpoint's save");
            // Not compared: the date (the golden has none) and the camera's
            // `Quaternion0..3` (the harness's camera model, as `ANGLEWARPX`).
            let cam = |l: &str| l.starts_with("Quaternion");
            let got: Vec<String> = String::from_utf8_lossy(text)
                .lines()
                .filter(|l| !l.starts_with("DateAndTime") && !cam(l))
                .map(str::to_string)
                .collect();
            let want: Vec<String> = want.iter().filter(|l| !cam(l)).cloned().collect();
            if got != want {
                let i = (0..got.len().max(want.len()))
                    .find(|&i| got.get(i) != want.get(i))
                    .unwrap();
                bad.push(format!(
                    "SAVEFILE line {i}: golden {:?}, native {:?}",
                    want.get(i),
                    got.get(i)
                ));
            }
        }
        self.prev = Some(now);
        if !bad.is_empty() {
            let msg = format!("{name}: step {} `{line}`: {}", self.step, bad.join("\n "));
            if std::env::var_os("STORY_SOFT").is_some() {
                eprintln!("{msg}");
            } else {
                panic!("{msg}");
            }
        }
    }

    /// The `rand()` generator against the golden's draws up to this step.
    fn check_lcg(&mut self, line: &str) {
        let now = self.g.rand_state();
        let from = std::mem::replace(&mut self.lcg_from, now);
        if let Some(&want) = self.lcg.get(&self.step) {
            if now != want {
                // The draws of the step, the port's and the golden's.
                let mut r = atlantis2_engine::crt::Rand::new(from);
                let mut got = Vec::new();
                while r.state != now && got.len() < 2000 {
                    got.push(r.rand());
                }
                panic!(
                    "step {} `{line}`: rand() draws differ from the golden's\n golden {:?}\n native {:?} (from state {from})",
                    self.step,
                    self.lcg_vals.get(&self.step),
                    got
                );
            }
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
                self.g.run_command(v);
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
        // The media the items started end at once (the harness ends them
        // inside the step).
        self.finish_media();
        if frames == 0 {
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
            "MODE" => match self.g.mode() {
                Mode::Menu => "6".to_string(),
                _ => self
                    .g
                    .puzzle_screen()
                    .map_or(3, |s| s.mode_number())
                    .to_string(),
            },
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
            let n = v.split_whitespace().next().unwrap();
            out.entry(step).or_default().push(n.parse().unwrap());
        }
    }
    out
}

/// The played goldens: the walk's `rand()` values by step (the `(walk)`
/// draws; the others come from the generator), the frames of each step
/// (`FRAME k.j`), and the generator's state at each step's end (from
/// `start`, reseeded at a checkpoint, one draw per `(lcg)` line).
#[allow(clippy::type_complexity)]
fn played_tables(
    golden: &str,
    start: u32,
) -> (
    BTreeMap<usize, Vec<i32>>,
    BTreeMap<usize, usize>,
    BTreeMap<usize, u32>,
    BTreeMap<usize, Vec<i32>>,
) {
    let mut walk: BTreeMap<usize, Vec<i32>> = BTreeMap::new();
    let mut frames: BTreeMap<usize, usize> = BTreeMap::new();
    let mut lcg = BTreeMap::new();
    let mut vals: BTreeMap<usize, Vec<i32>> = BTreeMap::new();
    let mut r = atlantis2_engine::crt::Rand::new(start);
    let mut step = 0;
    for l in golden.lines() {
        if l.starts_with("STEP ") {
            lcg.insert(step, r.state);
            step += 1;
        } else if let Some(v) = l.strip_prefix("CALL rand() -> ") {
            let (n, kind) = v.split_once(' ').unwrap();
            if kind == "(walk)" {
                walk.entry(step).or_default().push(n.parse().unwrap());
            } else {
                assert_eq!(kind, "(lcg)");
                let v: i32 = n.parse().unwrap();
                assert_eq!(r.rand(), v, "golden step {step}");
                vals.entry(step).or_default().push(v);
            }
        } else if let Some(rest) = l.strip_prefix("CHECKPOINT ") {
            let n = rest.split("quick save ").nth(1).unwrap();
            let n: u32 = n.split_whitespace().next().unwrap().parse().unwrap();
            r = atlantis2_engine::crt::Rand::new(n);
        } else if let Some(f) = l.strip_prefix("FRAME ") {
            // `FRAME k.j` or, in a screen's loop mode, `FRAME k.j mode n`.
            let f = f.split(" mode ").next().unwrap();
            if let Some((k, j)) = f.split_once('.') {
                if k.parse::<usize>() == Ok(step) && j.parse::<usize>().is_ok() {
                    *frames.entry(step).or_default() += 1;
                }
            }
        }
    }
    lcg.insert(step, r.state);
    (walk, frames, lcg, vals)
}

/// The `SetVariables` lines of a golden from its start (the new game's
/// `(chapter=600)` or the load of the segment's save), with their step.
fn sets(golden: &str) -> Vec<(usize, &str)> {
    let mut started = false;
    let mut step = 0;
    let mut out = Vec::new();
    for l in golden.lines() {
        if l.starts_with("STEP ") {
            step += 1;
        }
        if l.starts_with("FRAME load") {
            started = true;
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

struct Stderr;

impl log::Log for Stderr {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        eprintln!("{} {}", r.level(), r.args());
    }
    fn flush(&self) {}
}

fn new_run() -> Option<Run> {
    new_run_in(atlantis3_data::testing::install_dir()?)
}

fn new_run_in(dir: std::path::PathBuf) -> Option<Run> {
    if std::env::var_os("STORY_LOG").is_some() {
        let _ = log::set_logger(&Stderr);
        log::set_max_level(log::LevelFilter::Debug);
    }
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut g = Game::new(files).expect("the game starts");
    g.seed(1);
    g.set_harness_turns(true);
    // The harness's sound stubs: no voice plays, a sound plays for ever.
    g.story_sound_model = true;
    Some(Run {
        g,
        t: 0.0,
        expects: 0,
        rands: BTreeMap::new(),
        step: 0,
        saves: BTreeMap::new(),
        escape_travels: false,
        escaped: 0,
        played: false,
        frames: BTreeMap::new(),
        lcg: BTreeMap::new(),
        lcg_vals: BTreeMap::new(),
        lcg_from: 0,
        camera_aimed: false,
        refs: BTreeMap::new(),
        prev: None,
        step_frames: 0,
    })
}

/// Plays one segment's walk on `r` and checks it against its golden.
fn play(r: &mut Run, name: &str, walk: &str, golden: &str) {
    if !r.played {
        r.rands = rands(golden);
    }
    r.step = 0;
    let s = r.g.script_mut().unwrap();
    if s.set_log.is_none() {
        s.set_log = Some(Vec::new());
    }
    let want = sets(golden);
    for line in walk.lines() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        r.action(line);
        if r.played {
            r.check_lcg(line);
            if !line.starts_with("EXPECT") && !line.starts_with("RAND") {
                r.check_step(name, line);
            }
        }
        // The lines so far, against the golden's up to this step.
        let got: Vec<String> =
            r.g.script()
                .unwrap()
                .set_log
                .as_ref()
                .unwrap()
                .iter()
                .map(|l| format!("SET{}", l.trim_end()))
                .collect();
        let upto: Vec<&str> = want
            .iter()
            .filter(|(s, _)| *s <= r.step)
            .map(|(_, l)| *l)
            .collect();
        if let Some(i) = (0..got.len().max(upto.len()))
            .find(|&i| got.get(i).map(String::as_str) != upto.get(i).copied())
        {
            let from = i.saturating_sub(4);
            let flags: Vec<String> =
                r.g.script()
                    .unwrap()
                    .flags
                    .iter()
                    .enumerate()
                    .filter(|(k, v)| **v != 0 && *k < 1000)
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect();
            panic!(
                "{name}: step {} `{line}`: SetVariables line {i} differs\n native flags {}\n golden {:#?}\n native {:#?}",
                r.step,
                flags.join(" "),
                &upto[from..(i + 4).min(upto.len())],
                &got[from..(i + 4).min(got.len())]
            );
        }
    }
    let got: Vec<String> =
        r.g.script_mut()
            .unwrap()
            .set_log
            .take()
            .unwrap()
            .into_iter()
            .map(|l| format!("SET{}", l.trim_end()))
            .collect();
    for (i, (a, (step, b))) in got.iter().zip(&want).enumerate() {
        if a != b {
            let from = i.saturating_sub(4);
            let ctx: Vec<_> = want[from..(i + 3).min(want.len())].to_vec();
            let mine = &got[from..(i + 3).min(got.len())];
            panic!(
                "{name}: SetVariables line {i} (golden step {step}): {a} != {b}\n golden {ctx:#?}\n native {mine:#?}"
            );
        }
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

/// The whole story in one run from the new game, against the original's
/// chained run.
#[test]
fn whole_story() {
    let Some(mut r) = new_run() else {
        return;
    };
    let walk: String = SEGMENTS.iter().map(|s| s.1).collect::<Vec<_>>().join("\n");
    play(
        &mut r,
        "chain",
        &walk,
        include_str!("golden/story/chain.txt"),
    );
}

/// The whole story on GOG.com's English, German and Spanish installs,
/// against the same chained golden: the scripts are the French install's
/// but for `chapi054.con` (English and German), whose one changed line
/// the walk does not reach; the voices, texts and films only change what
/// is shown and heard.
#[test]
fn whole_story_other_languages() {
    for (var, code) in atlantis3_data::testing::GOG_LANGUAGES {
        let Some(dir) = atlantis3_data::testing::dir_of(var) else {
            continue;
        };
        let mut r = new_run_in(dir).unwrap();
        assert_eq!(r.g.startup.config.langue, code.as_bytes(), "{var}");
        let walk: String = SEGMENTS.iter().map(|s| s.1).collect::<Vec<_>>().join("\n");
        play(
            &mut r,
            &format!("chain {var}"),
            &walk,
            include_str!("golden/story/chain.txt"),
        );
    }
}

/// The one script line GOG.com's languages change (`chapi054.con`,
/// Rhea's chapter 54): with `Flag875` = 8 (the Omega's dialog done) the
/// French and Spanish scripts refuse the move to the old man (view 11:
/// `AbortDepart`, dialog 255, `Flag876` := 1), the English and German ones
/// let it go. Segment 07's walk up to `FLAG875=8`, then `GO 11`.
#[test]
fn rhea_old_man_at_875_8_by_language() {
    let mut installs = Vec::new();
    if let Some(d) = atlantis3_data::testing::install_dir() {
        installs.push(("ATLANTIS3_DATA", d, true));
    }
    for (var, code) in atlantis3_data::testing::GOG_LANGUAGES {
        if let Some(d) = atlantis3_data::testing::dir_of(var) {
            installs.push((var, d, code == "esp"));
        }
    }
    let (_, walk, golden) = SEGMENTS[6];
    let upto: Vec<&str> = walk
        .lines()
        .take_while(|l| !l.starts_with("EXPECT FLAG875=8"))
        .collect();
    for (var, dir, refused) in installs {
        let mut r = new_run_in(dir).unwrap();
        r.rands = rands(golden);
        r.g.script_mut().unwrap().set_log = Some(Vec::new());
        let save = start_save(golden).unwrap();
        assert!(r.g.load_save_text("quick1_100.sav", save), "{var}: load");
        r.act_frame();
        for line in &upto {
            let line = line.split('#').next().unwrap().trim();
            if !line.is_empty() {
                r.action(line);
            }
        }
        assert_eq!(r.g.script().unwrap().flags[875], 8, "{var}");
        assert_eq!(r.g.script().unwrap().flags[876], 0, "{var}");
        r.action("GO 11");
        r.settle();
        let view = r.g.place().1;
        if refused {
            assert_eq!(r.g.script().unwrap().flags[876], 1, "{var}: refused");
            assert_ne!(view, Some(11), "{var}: refused");
        } else {
            assert_eq!(r.g.script().unwrap().flags[876], 0, "{var}: let go");
            assert_eq!(view, Some(11), "{var}: let go");
        }
    }
}

fn segment(index: usize) {
    if let Some(r) = segment_with(index, None) {
        assert_eq!(r.escaped, 0);
    }
}

/// Segment `index`, against `golden` instead of its own when given (the
/// travel videos then escaped).
fn segment_with(index: usize, golden: Option<&str>) -> Option<Run> {
    let mut r = new_run()?;
    let (name, walk, own) = SEGMENTS[index];
    r.escape_travels = golden.is_some();
    let golden = golden.unwrap_or(own);
    // A golden without the save's dump starts from the same save as the
    // segment's own (its `FRAME load` line names the same file and md5).
    if let Some(save) = start_save(golden).or_else(|| start_save(own)) {
        r.g.script_mut().unwrap().set_log = Some(Vec::new());
        assert!(r.g.load_save_text("quick1_100.sav", save), "{name}: load");
        // The harness's first warp frame after the load, and its settle.
        r.act_frame();
    }
    play(&mut r, name, walk, golden);
    Some(r)
}

/// Segment 01 with Escape pressed during every travel video that plays
/// (the Hoggar's; the other worlds have no travel file), against the
/// original's run (`01_hoggar_escape.txt`): Escape's `0x0044c620(1)` makes
/// the video's end resume the script, which runs the tail of the last
/// interrupted section again (`Flag1000=1`, `flag426=1`, the Touareg
/// hidden again), the original's bug.
#[test]
fn story_01_hoggar_escaping_travels() {
    if let Some(r) = segment_with(0, Some(include_str!("golden/story/01_hoggar_escape.txt"))) {
        assert_eq!(r.escaped, 8);
    }
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

/// The played story (M4): the same story with every puzzle solved from
/// the player's input by the boards' code, against the original's
/// interpreter and puzzle code playing the walks
/// (`tests/golden/story_played/NN_<segment>.walk` and `.txt`): the clock
/// runs 1.0025 / 15 s a frame (one place-animation step each), the steps'
/// frames are the golden's, the sounds and voices play their files'
/// lengths, `rand()` is the generator's (seeded 1 at the start-up, n at a
/// checkpoint n's reload) but for the walks' `RAND` values. After every
/// step: the generator's state, the frames the step ran, the `STATE` line
/// (chapter, world, place, view, hand, section waiting, music), the loop
/// mode, the variables and flags the step changed (exactly those, but the
/// harness's camera model's `ANGLEWARPX` / `ANGLEWARPY` and the tick
/// task's `RANDOM`, `TIME1`, `TIME2`) and a checkpoint's save (but its
/// date and camera quaternion). New actions: `CAMERA yaw pitch` (the
/// player turned there), `POINTER x y` and `CLICK left|right [SPRITE=n]
/// [POINTER=x,y]` (the screens' free pointer; the sprite the screen's hit
/// test answers), `FRAMES_UNTIL cond` (the golden's frames of the step;
/// a `FLAGn=v` / `MODE=n` condition is checked false before the last of
/// them and true after it; a puzzle's C global, `SLOTFRAME`, `SHOWN` and
/// `CLICKABLE` are not read by the test).
mod played {
    use super::*;

    pub const SEGMENTS: &[(&str, &str, &str)] = &[
        (
            "01_hoggar",
            include_str!("golden/story_played/01_hoggar.walk"),
            include_str!("golden/story_played/01_hoggar.txt"),
        ),
        (
            "02_cfuner",
            include_str!("golden/story_played/02_cfuner.walk"),
            include_str!("golden/story_played/02_cfuner.txt"),
        ),
        (
            "03_ldm",
            include_str!("golden/story_played/03_ldm.walk"),
            include_str!("golden/story_played/03_ldm.txt"),
        ),
        (
            "04_hoggar5",
            include_str!("golden/story_played/04_hoggar5.walk"),
            include_str!("golden/story_played/04_hoggar5.txt"),
        ),
        (
            "05_bagdad",
            include_str!("golden/story_played/05_bagdad.walk"),
            include_str!("golden/story_played/05_bagdad.txt"),
        ),
        (
            "06_paleo",
            include_str!("golden/story_played/06_paleo.walk"),
            include_str!("golden/story_played/06_paleo.txt"),
        ),
        (
            "07_rhea",
            include_str!("golden/story_played/07_rhea.walk"),
            include_str!("golden/story_played/07_rhea.txt"),
        ),
        (
            "08_ending",
            include_str!("golden/story_played/08_ending.walk"),
            include_str!("golden/story_played/08_ending.txt"),
        ),
    ];

    /// The original's checkpoint saves: segment i + 1 starts from save i.
    const SAVES: &[&[u8]] = &[
        include_bytes!("golden/story_played/saves/01_hoggar_end.sav"),
        include_bytes!("golden/story_played/saves/02_cfuner_end.sav"),
        include_bytes!("golden/story_played/saves/03_ldm_end.sav"),
        include_bytes!("golden/story_played/saves/04_hoggar5_end.sav"),
        include_bytes!("golden/story_played/saves/05_bagdad_end.sav"),
        include_bytes!("golden/story_played/saves/06_paleo_end.sav"),
        include_bytes!("golden/story_played/saves/07_rhea_end.sav"),
    ];

    fn new_played() -> Option<Run> {
        let mut r = new_run()?;
        r.played = true;
        // The played harness's sounds and voices: their files' lengths.
        r.g.story_sound_model = false;
        // The harness stubs the dialogs' set-up and start (and ends each
        // dialog at once) and runs no place sound: no `rand()` drawn there.
        r.g.stub_rand = true;
        r.g.dialog_heads = false;
        Some(r)
    }

    fn run(r: &mut Run, name: &str, walk: &str, golden: &str, start: u32) {
        let (walk_rands, frames, lcg, vals) = played_tables(golden, start);
        r.rands = walk_rands;
        r.frames = frames;
        r.lcg = lcg;
        r.lcg_vals = vals;
        r.refs = step_refs(golden);
        play(r, name, walk, golden);
    }

    /// Segment `index` from the original's save at the end of the
    /// previous one (checkpoint 100 + index).
    pub fn segment(index: usize) {
        let Some(mut r) = new_played() else {
            return;
        };
        let (name, walk, golden) = SEGMENTS[index];
        let n = 100 + index as i32;
        let mut seed = 1;
        if let Some(save) = index.checked_sub(1).map(|i| SAVES[i].to_vec()) {
            r.g.script_mut().unwrap().set_log = Some(Vec::new());
            let file = atlantis3_data::save::quick_save_name(1, n);
            r.reload(n, &file, save);
            r.step = 0;
            let t = played_tables(golden, n as u32);
            r.lcg = t.2;
            r.lcg_vals = t.3;
            r.lcg_from = n as u32;
            r.check_lcg("the load");
            // The harness's reference for the first step's `VARS` /
            // `FLAGS`: the state after the load.
            r.prev = Some(r.snapshot());
            seed = n as u32;
        }
        run(&mut r, name, walk, golden, seed);
    }

    /// The whole story played in one run from the new game to `exit=1`.
    pub fn whole() {
        let Some(mut r) = new_played() else {
            return;
        };
        let walk: String = SEGMENTS.iter().map(|s| s.1).collect::<Vec<_>>().join("\n");
        run(
            &mut r,
            "played chain",
            &walk,
            include_str!("golden/story_played/chain.txt"),
            1,
        );
    }
}

#[test]
fn played_whole_story() {
    played::whole();
}

#[test]
fn played_01_hoggar() {
    played::segment(0);
}

#[test]
fn played_02_cfuner() {
    played::segment(1);
}

#[test]
fn played_03_ldm() {
    played::segment(2);
}

#[test]
fn played_04_hoggar5() {
    played::segment(3);
}

#[test]
fn played_05_bagdad() {
    played::segment(4);
}

#[test]
fn played_06_paleo() {
    played::segment(5);
}

#[test]
fn played_07_rhea() {
    played::segment(6);
}

#[test]
fn played_08_ending() {
    played::segment(7);
}

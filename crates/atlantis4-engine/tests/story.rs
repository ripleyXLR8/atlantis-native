//! The whole story replayed headless, from a new game to `FinishGame`,
//! through the engine's own input paths, chapter by chapter.
//!
//! The walk (`tests/story/walk.txt`) is a sequence of player actions found
//! by running the game's own scripts against a model of the engine; the
//! test plays each action as a player would and checks that the engine
//! events it causes are the walk's, in order:
//!
//! ```text
//! chapter <n>                 the actions of chapter n follow (n ends with SaveGame("n.xml"))
//! newgame                     Game::load, then the node change to firstWarpPath
//! <node> c <target>           a click on the warp that must hit <target> (OnWarpObjectHit's name)
//! <node> u <target> <item>    <item> selected in the inventory menu, then the click
//! <node> a <id>               a click on the answer <id>
//! <node> q                    a click on the conversation's quit button
//! <node> w <name>             wait until the animation or one-shot sound <name> ends
//! = <EVENT> <arg>             an event the action caused: WARP <node>, MOVIE <name>,
//!                             DOC <name>, TAKE / DROP / LOST <item>, SAVE <file>, FINISH
//! ```
//!
//! `<node>` is the node path without `3D/` and `.wp`; an action ending in
//! `| <animation>` is taken while that finite animation runs (the
//! spaceport's real-time passages). `tests/story/globals.txt` holds the
//! script globals the walk has when each chapter save and `FinishGame`
//! run; they are compared with the globals the save walks here.
//!
//! Input goes where the viewer sends it (`atlantis-app`'s `a4.rs`): a
//! press and release at one window pixel go to the GUI first, then to a
//! movie, then to the warp's click. To click a target the view is
//! turned by a drag (the setters' limits apply) until the target's pick
//! mesh is under a pixel whose click returns its name. Between actions
//! the engine runs on a test clock until nothing blocks the player:
//! movies are skipped with their button, dialog lines with a click on the
//! bar, documents with clicks. `FinishGame`'s screen and the saves'
//! files are not tested here: the call to `FinishGame` and the save's
//! globals are.
//!
//! Needs the game: `ATLANTIS4_DATA` (see `atlantis4_data::testing`);
//! skipped with `ATLANTIS4_SKIP_DATA_TESTS=1`. Run in release: the whole
//! story takes a few minutes. `A4_STORY_LOG=1` prints each action.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::OnceLock;

use atlantis4_data::A4Files;
use atlantis4_engine::dialog::Action;
use atlantis4_engine::warp::exit_name;
use atlantis4_engine::Game;
use atlantis4_script::tolua::Binding;
use atlantis4_script::{
    new_game, read_boot_vars, BootValue, Call, Engine, EngineRef, ErrorMode, Event, GuiKey,
    GuiValue, Options, Reply, SavedGlobal, SavedValue, Script, Value,
};

const WALK: &str = include_str!("story/walk.txt");
const GLOBALS: &str = include_str!("story/globals.txt");

const W: u32 = 1365;
const H: u32 = 768;
/// The test clock's step, µs.
const TICK: u64 = 10_000;
/// The engine clock at the new game, µs.
const T0: u64 = 5_000_000;
/// The longest a wait or a settling may take, µs of the test clock.
const LONGEST: u64 = 600_000_000;
/// The chapters: six ended by a chapter save, the last by `FinishGame`.
const CHAPTERS: usize = 7;

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Op {
    NewGame,
    Click(String),
    Use(String, String),
    Answer(String),
    Quit,
    Wait(String),
}

#[derive(Clone, Debug)]
struct Step {
    /// The line in `walk.txt`.
    line: usize,
    node: String,
    op: Op,
    /// The finite animation running when the action is taken.
    pending: Option<String>,
    events: Vec<String>,
}

fn parse_walk() -> Vec<Vec<Step>> {
    let mut chapters: Vec<Vec<Step>> = Vec::new();
    for (i, l) in WALK.lines().enumerate() {
        if l.starts_with('#') || l.trim().is_empty() {
            continue;
        }
        if let Some(n) = l.strip_prefix("chapter ") {
            assert_eq!(
                n.parse::<usize>().unwrap(),
                chapters.len(),
                "line {}",
                i + 1
            );
            chapters.push(Vec::new());
            continue;
        }
        let steps = chapters.last_mut().expect("a chapter first");
        if let Some(e) = l.strip_prefix("= ") {
            steps
                .last_mut()
                .expect("an action first")
                .events
                .push(e.to_string());
            continue;
        }
        if l == "newgame" {
            steps.push(Step {
                line: i + 1,
                node: String::new(),
                op: Op::NewGame,
                pending: None,
                events: Vec::new(),
            });
            continue;
        }
        let (act, pending) = match l.split_once(" | ") {
            Some((a, p)) => (a, Some(p.to_string())),
            None => (l, None),
        };
        let f: Vec<&str> = act.split(' ').collect();
        let op = match (f[1], f.len()) {
            ("c", 3) => Op::Click(f[2].into()),
            ("u", 4) => Op::Use(f[2].into(), f[3].into()),
            ("a", 3) => Op::Answer(f[2].into()),
            ("q", 2) => Op::Quit,
            ("w", 3) => Op::Wait(f[2].into()),
            _ => panic!("walk line {}: {l}", i + 1),
        };
        steps.push(Step {
            line: i + 1,
            node: f[0].to_string(),
            op,
            pending,
            events: Vec::new(),
        });
    }
    chapters
}

/// `globals.txt`: per chapter save and `FinishGame` ("SAVE 0.xml", …,
/// "FINISH"), the node and the globals by name (type, value).
type Expected = BTreeMap<String, (String, BTreeMap<String, (char, String)>)>;

fn parse_globals() -> Expected {
    let mut out = Expected::new();
    let mut cur: Option<String> = None;
    for l in GLOBALS.lines() {
        if l.starts_with('#') || l.trim().is_empty() {
            continue;
        }
        if let Some(rest) = l.strip_prefix("at ") {
            let (tag, node) = rest.rsplit_once(' ').unwrap();
            out.insert(tag.to_string(), (node.to_string(), BTreeMap::new()));
            cur = Some(tag.to_string());
            continue;
        }
        let mut f = l.splitn(3, ' ');
        let (name, ty, val) = (f.next().unwrap(), f.next().unwrap(), f.next().unwrap_or(""));
        let tag = cur.as_ref().expect("an `at` line first");
        out.get_mut(tag).unwrap().1.insert(
            name.to_string(),
            (ty.chars().next().unwrap(), val.to_string()),
        );
    }
    out
}

// ---------------------------------------------------------------------------
// The engine, recorded
// ---------------------------------------------------------------------------

/// What the recording engine saw.
#[derive(Default)]
struct Log {
    events: Vec<String>,
    /// The chapter saves with their globals.
    saves: Vec<(String, Vec<SavedGlobal>)>,
    /// The node entered last (`Zone/NNNN`).
    node: String,
    /// Lua errors (the original exits on a callback's).
    errors: Vec<String>,
}

/// The game, with the story's events written down as the bindings run.
struct Rec {
    game: Rc<RefCell<Game>>,
    log: Rc<RefCell<Log>>,
}

fn s(b: Option<&[u8]>) -> String {
    String::from_utf8_lossy(b.unwrap_or_default()).into_owned()
}

/// `3D/Zone/0001.wp` → `Zone/0001`.
fn short_node(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.strip_prefix("3D/").unwrap_or(&p);
    p.strip_suffix(".wp").unwrap_or(p).to_string()
}

impl Engine for Rec {
    fn call(&mut self, call: &Call) -> Reply {
        let name = s(call.str(0));
        let mut log = Vec::new();
        let before = {
            let mut g = self.game.borrow_mut();
            g.gui();
            (
                g.inventory.objects().count(),
                g.inventory.count(&name),
                g.inventory.selected_name().map(str::to_string),
            )
        };
        match call.binding {
            Binding::PlayMovie => {
                let v = name.replace('\\', "/");
                let v = v.strip_prefix("Videos/").unwrap_or(&v);
                log.push(format!("MOVIE {}", v.strip_suffix(".ogv").unwrap_or(v)));
            }
            Binding::ShowDocument => log.push(format!("DOC {name}")),
            Binding::FinishGame => log.push("FINISH".into()),
            Binding::RemoveFromBag if before.1 > 0 => log.push(format!("DROP {name}")),
            Binding::RemoveSelectedFromBag => {
                if let Some(n) = &before.2 {
                    log.push(format!("DROP {n}"));
                }
            }
            _ => {}
        }
        self.log.borrow_mut().events.extend(log);
        let reply = self.game.borrow_mut().call(call);
        if matches!(
            call.binding,
            Binding::AddToBag | Binding::AddToBagWithoutNotification
        ) {
            let after = self.game.borrow().inventory.objects().count();
            let e = if after > before.0 { "TAKE" } else { "LOST" };
            self.log.borrow_mut().events.push(format!("{e} {name}"));
        }
        reply
    }

    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        self.game.borrow_mut().read_script(path)
    }

    fn resolve(&mut self, path: &[u8]) -> Vec<u8> {
        self.game.borrow_mut().resolve(path)
    }

    fn has_scene(&self) -> bool {
        self.game.borrow().has_scene()
    }

    fn leave_scene(&mut self) {
        self.game.borrow_mut().leave_scene();
    }

    fn save_game(&mut self, file: &[u8], globals: &[SavedGlobal]) {
        if file != b"save.xml" {
            let f = s(Some(file));
            let mut l = self.log.borrow_mut();
            l.events.push(format!("SAVE {f}"));
            l.saves.push((f, globals.to_vec()));
        }
        self.game.borrow_mut().save_game(file, globals);
    }

    fn enter_scene(&mut self, path: &[u8]) {
        self.game.borrow_mut().enter_scene(path);
    }

    fn load_scene_xml(&mut self, xml: &[u8]) {
        self.game.borrow_mut().load_scene_xml(xml);
    }

    fn gui_layout(&mut self, class: &str, entries: Option<&[(GuiKey, GuiValue)]>) -> usize {
        self.game.borrow_mut().gui_layout(class, entries)
    }

    fn event(&mut self, event: &Event<'_>) {
        match event {
            Event::Warp(p) => {
                let n = short_node(&s(Some(p)));
                let mut l = self.log.borrow_mut();
                l.events.push(format!("WARP {n}"));
                l.node = n;
            }
            Event::Error(site, msg) => {
                self.log
                    .borrow_mut()
                    .errors
                    .push(format!("{site:?}: {}", s(Some(msg))));
            }
            _ => {}
        }
        self.game.borrow_mut().event(event);
    }

    fn take_callbacks(&mut self) -> Vec<(String, Vec<Value>)> {
        self.game.borrow_mut().take_callbacks()
    }
}

// ---------------------------------------------------------------------------
// The player
// ---------------------------------------------------------------------------

/// Whether the dialog bar is not all the way down (a line shows, or its
/// bar moves).
fn bar_up(g: &Game) -> bool {
    let Some(gui) = g.dialogs.gui.as_ref() else {
        return false;
    };
    let playing = |n: &str| gui.find(n).is_some_and(|a| gui.playing(a));
    gui.find("dialog")
        .is_some_and(|b| gui.node(b).anchor[1] > -0.1)
        || playing("dialogAnimationUp")
        || playing("dialogAnimationDown")
}

struct Player {
    game: Rc<RefCell<Game>>,
    log: Rc<RefCell<Log>>,
    script: Script,
    clock: Rc<Cell<u64>>,
    /// The animations and one-shot sounds that ended, with when (µs).
    ended: Vec<(String, u64)>,
    /// Where the ends that a wait may take start in `ended`.
    since: usize,
    verbose: bool,
}

fn arg_text(args: &[Value]) -> String {
    match args.first() {
        Some(Value::Str(s)) => String::from_utf8_lossy(s).into_owned(),
        _ => String::new(),
    }
}

impl Player {
    fn start(files: A4Files) -> Result<Player, String> {
        let clock = Rc::new(Cell::new(T0));
        let mut g = Game::new(files, H);
        let c = clock.clone();
        g.clock_us = Some(Box::new(move || c.get()));
        // The sounds' clock starts here (no output: mixed on this clock).
        g.update(T0 as f64 / 1000.0);
        let game = Rc::new(RefCell::new(g));
        let log = Rc::new(RefCell::new(Log::default()));
        let engine: EngineRef = Rc::new(RefCell::new(Rec {
            game: game.clone(),
            log: log.clone(),
        }));
        let options = Options {
            errors: ErrorMode::Strict,
            ..Options::default()
        };
        let boot = read_boot_vars(&engine, options).map_err(|e| e.to_string())?;
        let Some(BootValue::Str(first)) = boot.first_warp_path else {
            return Err("no firstWarpPath".into());
        };
        let script = new_game(&engine, options, &first).map_err(|e| e.to_string())?;
        Ok(Player {
            game,
            log,
            script,
            clock,
            ended: Vec::new(),
            since: 0,
            verbose: std::env::var_os("A4_STORY_LOG").is_some(),
        })
    }

    fn now(&self) -> u64 {
        self.clock.get()
    }

    fn cb(&mut self, name: &str, args: &[Value]) -> Result<(), String> {
        self.script
            .callback(name, args)
            .map_err(|e| format!("{name}({}): {e}", arg_text(args)))
    }

    fn gui_callbacks(&mut self) -> Result<(), String> {
        loop {
            let cbs = self.game.borrow_mut().gui_callbacks();
            if cbs.is_empty() {
                return Ok(());
            }
            for (n, a) in cbs {
                self.cb(&n, &a)?;
            }
        }
    }

    /// One update of the viewer (`a4.rs`'s `tick`) at the clock + `dt`.
    fn tick(&mut self, dt: u64) -> Result<(), String> {
        self.clock.set(self.clock.get() + dt);
        let now = self.clock.get() as f64 / 1000.0;
        let finished = self.game.borrow_mut().update(now);
        let sounds = self.game.borrow_mut().sound_callbacks();
        for (n, a) in sounds {
            if n == "OnSoundStopped" {
                self.ended.push((arg_text(&a), self.now()));
            }
            self.cb(&n, &a)?;
        }
        for name in finished {
            self.ended.push((name.clone(), self.now()));
            self.cb("OnAnimationFinished", &[Value::Str(name.into_bytes())])?;
        }
        let ended = self.game.borrow_mut().update_movie(now);
        if let Some(n) = ended {
            self.cb("OnMovieFinished", &[Value::Str(n.into_bytes())])?;
        }
        let dialogs = self.game.borrow_mut().dialog_callbacks();
        for (n, a) in dialogs {
            self.cb(&n, &a)?;
        }
        self.game.borrow_mut().update_gui(now);
        self.gui_callbacks()
    }

    fn run_actions(&mut self, actions: Vec<Action>) -> Result<(), String> {
        for a in actions {
            match a {
                Action::Lua(n, args) => self.cb(&n, &args)?,
                Action::HideAnswers => self.game.borrow_mut().dialogs.hide_answers(),
            }
        }
        Ok(())
    }

    /// A press and a release at window pixel (`x`, `y`), sent where the
    /// viewer sends them; the warp's hit, if the warp took the click.
    fn tap(&mut self, x: f64, y: f64) -> Result<Option<String>, String> {
        let in_movie = self.game.borrow().movie.is_some();
        let gui_down = self.game.borrow_mut().gui_press(W, H, x, y);
        if !gui_down && in_movie {
            self.game.borrow_mut().movie_press(W, H, x, y);
        }
        let gui_shown = self.game.borrow().gui_shown();
        if gui_down || gui_shown {
            let actions = self.game.borrow_mut().gui_release(W, H, x, y);
            if let Some(a) = actions {
                self.run_actions(a)?;
                self.gui_callbacks()?;
                return Ok(None);
            }
            if gui_down {
                return Ok(None);
            }
        }
        if self.game.borrow().movie.is_some() {
            let ended = self.game.borrow_mut().movie_release(W, H, x, y);
            if let Some(n) = ended {
                self.cb("OnMovieFinished", &[Value::Str(n.into_bytes())])?;
            }
            return Ok(None);
        }
        let hit = self.game.borrow().click(W, H, x as i32, y as i32);
        if let Some(name) = &hit {
            self.cb("OnWarpObjectHit", &[Value::Str(name.clone().into_bytes())])?;
            self.gui_callbacks()?;
        }
        Ok(hit)
    }

    /// What keeps the player from acting, if anything.
    fn blocked(&self) -> Option<&'static str> {
        let mut g = self.game.borrow_mut();
        if g.movie.is_some() {
            Some("movie")
        } else if g.documents.shown.is_some() {
            Some("document")
        } else if g.dialogs.shown() && !g.dialogs.in_conversation {
            // A narration shows until its last line's bar is down.
            Some("line")
        } else if g.dialogs.in_conversation
            && (g.sounds.voice.is_playing() || !g.dialogs.queue.is_empty() || bar_up(&g))
        {
            Some("line")
        } else if g.gui().menu_shown {
            Some("menu")
        } else {
            None
        }
    }

    /// Runs the engine until nothing blocks the player: movies skipped
    /// with their button, lines with a click on the bar, documents with
    /// clicks.
    fn settle(&mut self) -> Result<(), String> {
        let start = self.now();
        let mut last_tap = 0u64;
        while let Some(what) = self.blocked() {
            if self.now() - start > LONGEST {
                let g = self.game.borrow();
                let d = &g.dialogs;
                return Err(format!(
                    "still blocked by a {what} (line {:?}, queue {}, shown {}, conversation {}, voice {}, answers {})",
                    d.current.as_ref().map(|l| (&l.id, &l.voice)),
                    d.queue.len(),
                    d.shown(),
                    d.in_conversation,
                    g.sounds.voice.is_playing(),
                    d.answers.len()
                ));
            }
            match what {
                "movie" => self.skip_movie()?,
                "document" if self.now() - last_tap >= 300_000 => {
                    last_tap = self.now();
                    self.tap(f64::from(W) / 2.0, f64::from(H) / 2.0)?;
                }
                "line"
                    if self.now() - last_tap >= 100_000
                        && self.game.borrow().sounds.voice.is_playing() =>
                {
                    last_tap = self.now();
                    self.tap(f64::from(W) / 2.0, f64::from(H) - 40.0)?;
                }
                "menu" => {
                    self.game.borrow_mut().gui().close_menu();
                    return Err("the inventory menu stayed open".into());
                }
                _ => {}
            }
            self.tick(TICK)?;
        }
        Ok(())
    }

    /// The skip button: shown by a click on the movie, then clicked.
    fn skip_movie(&mut self) -> Result<(), String> {
        let (layout, shown) = {
            let g = self.game.borrow();
            let m = g.movie.as_ref().expect("a movie");
            (m.layout(W, H, (75, 75)), m.skip_shown)
        };
        let centre =
            |r: &atlantis4_engine::layout::Rect| (r.left + r.width / 2.0, r.top + r.height / 2.0);
        if !shown {
            let b = layout.background;
            self.tap(b.left + 10.0, b.top + b.height - 10.0)?;
        }
        let (x, y) = centre(&layout.skip);
        self.tap(x, y)?;
        Ok(())
    }

    /// The pixels where a click hits `target` with the current view, near
    /// `points` first.
    fn hit_pixel(&mut self, target: &str, points: &[[f32; 3]]) -> Option<(f64, f64)> {
        let (hud, cam) = {
            let mut g = self.game.borrow_mut();
            let hud = g.gui().hud_button(W, H);
            (hud, g.camera(W, H))
        };
        let ok = |this: &mut Player, x: i32, y: i32| -> bool {
            if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
                return false;
            }
            let (fx, fy) = (f64::from(x), f64::from(y));
            if hud.is_some_and(|r| r.contains(fx, fy)) {
                return false;
            }
            let mut g = this.game.borrow_mut();
            if g.gui().notify_hit(W, H, fx, fy) {
                return false;
            }
            g.click(W, H, x, y).as_deref() == Some(target)
        };
        for p in points {
            let q = cam.project(*p);
            if !(q[2] < 1.0) || !q[0].is_finite() || !q[1].is_finite() {
                continue;
            }
            let (x, y) = (q[0] as i32, H as i32 - q[1] as i32);
            for r in [0, 3, 8, 16] {
                for (dx, dy) in [(0, 0), (r, 0), (-r, 0), (0, r), (0, -r)] {
                    if ok(self, x + dx, y + dy) {
                        return Some((f64::from(x + dx), f64::from(y + dy)));
                    }
                }
            }
        }
        // The whole window, coarse.
        for y in (4..H as i32).step_by(8) {
            for x in (4..W as i32).step_by(8) {
                if ok(self, x, y) {
                    return Some((f64::from(x), f64::from(y)));
                }
            }
        }
        None
    }

    /// The pick mesh of `target`: an exit's triangles, or a clickable
    /// animation's for its current frame; the triangles' centres.
    fn target_points(&self, target: &str) -> Vec<[f32; 3]> {
        let g = self.game.borrow();
        let Some(w) = g.warp.as_ref() else {
            return Vec::new();
        };
        let tris: Vec<[[f32; 3]; 3]> = if target.starts_with("3D\\") {
            w.node
                .exits
                .iter()
                .filter(|e| exit_name(&e.linked_warp) == target)
                .flat_map(|e| e.triangles.iter().copied())
                .collect()
        } else {
            w.picks()
                .into_iter()
                .filter(|a| a.name == target && a.clickable)
                .flat_map(|a| a.triangles)
                .collect()
        };
        let mut pts: Vec<[f32; 3]> = tris
            .iter()
            .map(|t| std::array::from_fn(|i| (t[0][i] + t[1][i] + t[2][i]) / 3.0))
            .collect();
        if pts.len() > 1 {
            let n = pts.len() as f32;
            let mean: [f32; 3] = std::array::from_fn(|i| pts.iter().map(|p| p[i]).sum::<f32>() / n);
            pts.insert(0, mean);
        }
        pts
    }

    /// Whether `target` is a clickable animation playing in the node.
    fn clickable_anim(&self, target: &str) -> bool {
        let g = self.game.borrow();
        g.warp
            .iter()
            .flat_map(|w| &w.anims)
            .any(|a| a.name == target && a.clickable && a.package.is_some() && a.anim.playing)
    }

    /// The view angles that put `p` at the window's centre (2° steps).
    fn angles_to(&self, p: [f32; 3]) -> (f32, f32) {
        let fov = self.game.borrow().fov;
        let mut best = (f32::MAX, 0.0, 0.0);
        for yi in 0..180 {
            let yaw = yi as f32 * 2.0 - 180.0;
            for pi in 0..=50 {
                let pitch = -55.0 + pi as f32 * 2.0;
                let cam = atlantis4_engine::Camera::new(fov, W, H, yaw, pitch);
                let q = cam.project(p);
                if !(q[2] < 1.0) {
                    continue;
                }
                let d = (q[0] - W as f32 / 2.0).powi(2) + (q[1] - H as f32 / 2.0).powi(2);
                if d < best.0 {
                    best = (d, yaw, pitch);
                }
            }
        }
        (best.1, best.2)
    }

    /// A click on the warp that hits `target`, the view turned to it by a
    /// drag when needed.
    fn click(&mut self, target: &str) -> Result<(), String> {
        // A clickable animation whose current frame has no pick mesh (a
        // caterpillar before it shows, `Domaine_MB/0014`'s `noa1` frames 0
        // and 1) cannot be clicked yet: the player waits for it.
        let start = self.now();
        let mut pts = self.target_points(target);
        while pts.is_empty() && self.now() - start < 30_000_000 && self.clickable_anim(target) {
            self.tick(TICK)?;
            pts = self.target_points(target);
        }
        if pts.is_empty() {
            let g = self.game.borrow();
            let state: Vec<String> = g
                .warp
                .iter()
                .flat_map(|w| &w.anims)
                .filter(|a| a.name == target)
                .map(|a| {
                    let picks: Vec<u32> = a
                        .package
                        .iter()
                        .flat_map(|p| p.picks.iter().map(|m| m.first_frame))
                        .collect();
                    format!(
                        "clickable {}, loaded {}, frame {}, playing {}, pick ranges {picks:?}",
                        a.clickable,
                        a.package.is_some(),
                        a.anim.frame,
                        a.anim.playing
                    )
                })
                .collect();
            let exits: Vec<String> = g
                .warp
                .iter()
                .flat_map(|w| &w.node.exits)
                .map(|e| format!("{}:{}", exit_name(&e.linked_warp), e.triangles.len()))
                .collect();
            return Err(format!(
                "{target}: no such exit or clickable animation at {} ({state:?}, exits {exits:?})",
                self.log.borrow().node
            ));
        }
        let mut at = self.hit_pixel(target, &pts);
        if at.is_none() {
            for p in pts.iter().take(8) {
                let (yaw, pitch) = self.angles_to(*p);
                {
                    let mut g = self.game.borrow_mut();
                    let v = &mut g.view;
                    let mut dyaw = f64::from(v.yaw) - f64::from(yaw);
                    dyaw -= 360.0 * (dyaw / 360.0).round();
                    let dpitch = f64::from(pitch) - f64::from(v.pitch);
                    // Input pixels of a drag: 0.5° each.
                    v.drag((dyaw / 0.5) as f32, (dpitch / 0.5) as f32, false);
                }
                at = self.hit_pixel(target, &pts);
                if at.is_some() {
                    break;
                }
            }
        }
        let Some((x, y)) = at else {
            let v = self.game.borrow().view;
            return Err(format!(
                "{target}: no pixel's click reaches it (view {} {}, limits {} {} {} {})",
                v.yaw, v.pitch, v.yaw_plus, v.yaw_minus, v.pitch_plus, v.pitch_minus
            ));
        };
        match self.tap(x, y)? {
            Some(h) if h == target => Ok(()),
            other => Err(format!(
                "{target}: the click at ({x}, {y}) went to {other:?}"
            )),
        }
    }

    /// `item` selected through the HUD's inventory button and the menu
    /// (a second click on the object within 300 ms closes the menu).
    fn select(&mut self, item: &str) -> Result<(), String> {
        if self.game.borrow().inventory.selected_name() == Some(item) {
            return Ok(());
        }
        let hud = self
            .game
            .borrow_mut()
            .gui()
            .hud_button(W, H)
            .ok_or("no HUD button")?;
        self.tap(hud.left + hud.width / 2.0, hud.top + hud.height / 2.0)?;
        if !self.game.borrow_mut().gui().menu_shown {
            return Err("the HUD button did not open the inventory".into());
        }
        let slot = {
            let g = self.game.borrow();
            let at = g
                .inventory
                .objects()
                .find(|(_, o)| o.name() == item)
                .map(|(at, _)| at);
            at
        };
        let Some((page, slot)) = slot else {
            self.game.borrow_mut().gui().close_menu();
            return Err(format!("{item} is not in the inventory"));
        };
        let r = self.game.borrow_mut().gui().slot_rects(page, W, H)[slot];
        let (x, y) = (r.left + r.width / 2.0, r.top + r.height / 2.0);
        self.tap(x, y)?;
        self.tick(TICK)?;
        // The click timer is shared by all objects and never stopped: a
        // click within 300 ms of the last object click already closed
        // the menu (Inventory-setSelected rule 3).
        if self.game.borrow_mut().gui().menu_shown {
            self.tap(x, y)?;
            self.tick(TICK)?;
        }
        if self.game.borrow_mut().gui().menu_shown {
            return Err("a double click on the object left the menu open".into());
        }
        if self.game.borrow().inventory.selected_name() != Some(item) {
            return Err(format!("{item} not selected"));
        }
        Ok(())
    }

    /// The answer `id`'s button, clicked.
    fn answer(&mut self, id: &str) -> Result<(), String> {
        let centre = {
            let g = &mut *self.game.borrow_mut();
            g.dialogs.layout(&g.files, W, H);
            g.dialogs
                .answers
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.gui.node(a.button).world)
        };
        let Some((x, y)) = centre else {
            let ids: Vec<String> = self
                .game
                .borrow()
                .dialogs
                .answers
                .iter()
                .map(|a| a.id.clone())
                .collect();
            return Err(format!("no answer {id} (offered: {ids:?})"));
        };
        self.tap(
            f64::from(W) / 2.0 + f64::from(x),
            f64::from(H) / 2.0 + f64::from(y),
        )?;
        Ok(())
    }

    /// The conversation's quit button (top right).
    fn quit(&mut self) -> Result<(), String> {
        if !self.game.borrow().dialogs.in_conversation {
            return Err("no conversation to quit".into());
        }
        let (x, y) = (f64::from(W) * 0.92 + 30.0, f64::from(H) * 0.01 + 30.0);
        // The object notification (`Notify.lua`, a button above every
        // layer, not click-through) covers `quit` while it shows: wait.
        let start = self.now();
        while self.game.borrow_mut().gui().notify_hit(W, H, x, y) {
            if self.now() - start > 60_000_000 {
                return Err("the notification stays over the quit button".into());
            }
            self.tick(TICK)?;
        }
        self.tap(x, y)?;
        if self.game.borrow().dialogs.in_conversation {
            return Err("the quit button did not end the conversation".into());
        }
        Ok(())
    }

    /// Runs the clock until the animation or one-shot sound `name` ends.
    fn wait(&mut self, name: &str, since: usize) -> Result<(), String> {
        let start = self.now();
        loop {
            if self.ended[since..].iter().any(|(n, _)| n == name) {
                return Ok(());
            }
            if self.now() - start > LONGEST {
                return Err(format!("{name} did not end"));
            }
            self.tick(TICK)?;
        }
    }

    fn act(&mut self, step: &Step, since: usize) -> Result<(), String> {
        match &step.op {
            Op::NewGame => Ok(()),
            Op::Click(t) => self.click(t),
            Op::Use(t, item) => {
                self.select(item)?;
                self.click(t)
            }
            Op::Answer(id) => self.answer(id),
            Op::Quit => self.quit(),
            Op::Wait(n) => self.wait(n, since),
        }
    }
}

// ---------------------------------------------------------------------------
// The replay, shared by the chapter tests
// ---------------------------------------------------------------------------

/// How far a chapter went.
#[derive(Clone, Debug)]
struct ChapterRun {
    actions: usize,
    done: usize,
    /// Test-clock seconds the chapter took.
    seconds: f64,
    failure: Option<String>,
}

/// The globals of the walk at `tag` against the ones saved here.
fn compare_globals(tag: &str, got: &[SavedGlobal], want: &Expected) -> Result<(), String> {
    let (_, want) = want
        .get(tag)
        .ok_or_else(|| format!("no globals for {tag}"))?;
    let mut diffs = Vec::new();
    let mut seen = 0;
    for g in got {
        let name = String::from_utf8_lossy(&g.name).into_owned();
        if name == "_VERSION" || name == "warpName" {
            continue;
        }
        seen += 1;
        let ok = match (want.get(&name), &g.value) {
            (Some(('b', v)), SavedValue::Boolean(b)) => v == if *b { "true" } else { "false" },
            (Some(('n', v)), SavedValue::Number(x)) => v.parse::<f64>().ok() == Some(*x),
            (Some(('s', v)), SavedValue::String(t)) => v.as_bytes() == t.as_slice(),
            _ => false,
        };
        if !ok {
            diffs.push(format!("{name}: {:?}, walk {:?}", g.value, want.get(&name)));
        }
    }
    if seen != want.len() {
        for n in want.keys() {
            if !got.iter().any(|g| g.name == n.as_bytes()) {
                diffs.push(format!("{n}: missing, walk {:?}", want[n]));
            }
        }
    }
    if diffs.is_empty() {
        Ok(())
    } else {
        Err(format!("globals at {tag}: {}", diffs.join("; ")))
    }
}

fn replay() -> Option<Vec<ChapterRun>> {
    let Some(dir) = atlantis4_data::testing::install_dir() else {
        atlantis4_data::testing::missing("the installed game");
        return None;
    };
    let files = A4Files::open_dir(&dir).unwrap().0;
    let chapters = parse_walk();
    let expected = parse_globals();
    let mut out: Vec<ChapterRun> = chapters
        .iter()
        .map(|c| ChapterRun {
            actions: c.len(),
            done: 0,
            seconds: 0.0,
            failure: Some("not reached".into()),
        })
        .collect();
    let mut player: Option<Player> = None;
    for (k, steps) in chapters.iter().enumerate() {
        let t0 = player.as_ref().map_or(T0, Player::now);
        out[k].failure = None;
        let mut failure = None;
        for (i, step) in steps.iter().enumerate() {
            let r = (|| -> Result<(), String> {
                if step.op == Op::NewGame {
                    player = Some(Player::start(files.clone())?);
                }
                let p = player.as_mut().ok_or("no game")?;
                if step.op != Op::NewGame {
                    let node = p.log.borrow().node.clone();
                    if node != step.node {
                        return Err(format!("at {node}, the walk is at {}", step.node));
                    }
                }
                // A wait is met by an end since the last action that was
                // not a wait: animations started together run together,
                // where the walk waits for them one after the other.
                if !matches!(step.op, Op::Wait(_)) {
                    p.since = p.ended.len();
                }
                let since = p.since;
                if step.op != Op::NewGame {
                    p.log.borrow_mut().events.clear();
                }
                if p.verbose {
                    eprintln!(
                        "[{}] {:?} at {:.2} s",
                        step.line,
                        step.op,
                        p.now() as f64 / 1e6
                    );
                }
                p.act(step, since)?;
                p.settle()?;
                let errors = std::mem::take(&mut p.log.borrow_mut().errors);
                if !errors.is_empty() {
                    return Err(format!("Lua errors: {errors:?}"));
                }
                // One page of 8 slots: the walk never holds a 9th object
                // (it would be lost, `LOST` below).
                let held = p.game.borrow().inventory.objects().count();
                if held > 8 {
                    return Err(format!("{held} objects held"));
                }
                let got = std::mem::take(&mut p.log.borrow_mut().events);
                if got != step.events {
                    return Err(format!("events {got:?}, the walk's {:?}", step.events));
                }
                let saves = std::mem::take(&mut p.log.borrow_mut().saves);
                for (f, g) in saves {
                    compare_globals(&format!("SAVE {f}"), &g, &expected)?;
                }
                if step.events.iter().any(|e| e == "FINISH") {
                    let g = p.script.globals().map_err(|e| e.to_string())?;
                    compare_globals("FINISH", &g, &expected)?;
                }
                Ok(())
            })();
            if let Err(e) = r {
                failure = Some(format!(
                    "chapter {k}, action {} of {} (walk line {}, {:?}{}): {e}",
                    i + 1,
                    steps.len(),
                    step.line,
                    step.op,
                    step.pending
                        .as_ref()
                        .map_or(String::new(), |p| format!(" while {p} runs")),
                ));
                break;
            }
            out[k].done = i + 1;
        }
        let t1 = player.as_ref().map_or(T0, Player::now);
        out[k].seconds = (t1 - t0) as f64 / 1e6;
        if let Some(f) = failure {
            out[k].failure = Some(f);
            break;
        }
    }
    Some(out)
}

fn outcome() -> Option<&'static [ChapterRun]> {
    static RUN: OnceLock<Option<Vec<ChapterRun>>> = OnceLock::new();
    RUN.get_or_init(replay).as_deref()
}

fn chapter(k: usize) {
    let Some(runs) = outcome() else { return };
    let r = &runs[k];
    eprintln!(
        "chapter {k}: {} of {} actions, {:.1} s of game time",
        r.done, r.actions, r.seconds
    );
    if let Some(f) = &r.failure {
        let first = runs.iter().position(|r| r.failure.is_some()).unwrap();
        if first < k {
            panic!("chapter {k} not reached: chapter {first} failed");
        }
        panic!("{f}");
    }
}

#[test]
fn the_walk_has_seven_chapters_and_1076_actions() {
    let c = parse_walk();
    assert_eq!(c.len(), CHAPTERS);
    let actions: usize = c.iter().flatten().filter(|s| s.op != Op::NewGame).count();
    assert_eq!(actions, 1076);
    let saves: Vec<&String> = c
        .iter()
        .flatten()
        .flat_map(|s| &s.events)
        .filter(|e| e.starts_with("SAVE ") || *e == "FINISH")
        .collect();
    assert_eq!(
        saves,
        [
            "SAVE 0.xml",
            "SAVE 1.xml",
            "SAVE 2.xml",
            "SAVE 3.xml",
            "SAVE 4.xml",
            "SAVE 5.xml",
            "FINISH"
        ]
    );
    let g = parse_globals();
    assert_eq!(g.len(), CHAPTERS);
}

#[test]
fn chapter_0_the_ships_and_center_17() {
    chapter(0);
}

#[test]
fn chapter_1_the_village() {
    chapter(1);
}

#[test]
fn chapter_2_the_forest_and_the_grotto() {
    chapter(2);
}

#[test]
fn chapter_3_from_the_domain_to_the_gods_land() {
    chapter(3);
}

#[test]
fn chapter_4_the_gods_apartments() {
    chapter(4);
}

#[test]
fn chapter_5_the_council_and_the_temple() {
    chapter(5);
}

#[test]
fn chapter_6_the_temple_to_finish_game() {
    chapter(6);
}

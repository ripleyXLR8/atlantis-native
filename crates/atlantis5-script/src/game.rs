//! The game manager (`CGameManager`): its states and request queue, the
//! script runs and the calls that wait, the named timers and the view
//! transition of `Atl_Address_Go`.
//!
//! Addresses are atlantis5-dvd-int-3891200's.
//!
//! - `Process` (@ 0x0042f000) switches on the manager's state (+0x30):
//!   1 runs the C string `return InitChapter` with the scripts' folder and
//!   the chapter (+0x50, nil when 0) under `lua_pcall`; 4 is play (camera,
//!   events, timers, sound); 2 load, 3 a menu (game over), 7 the end of a
//!   game; 0 takes the next request. A request is an int in a queue
//!   (+0x04..+0x2c); `Atl_InitChapter(n)` stores n and queues 1 then 4,
//!   `Atl_GameOver` queues 3 (unless in 3), `Atl_Load` 2,
//!   `Atl_TerminateGame` 7. The next request is taken (@ 0x0042ef30,
//!   set @ 0x0042fb90) when state 1 starts (before `InitChapter` runs),
//!   in state 0, and in state 4 after the frame's events and timers when
//!   the queue is not empty.
//! - A new game (@ 0x0042fc50): the autosave `autoslot` is deleted, the
//!   `atl` database made (@ 0x00401010), then `atlantis.lua` and
//!   `on_new_game.lua` run.
//! - The handlers that run over frames do it in a loop of their own
//!   (`Atl_Address_Go` @ 0x00405390, `Atl_Video_Play` @ 0x0040c990): the
//!   script that called waits. Here the script is a coroutine that
//!   yields at the call and is resumed when it is over.

use std::collections::{BTreeMap, VecDeque};

use crate::engine::{Call, Event, Fx, Request, Semantic, Value};
use crate::script::{EngineRef, Outcome, Pending, Script, Task, SCRIPTS_DIR};

/// What a script run waits for.
#[derive(Clone, Debug, PartialEq)]
enum Block {
    /// A call the engine runs over frames.
    Engine,
    /// The transition at this depth of the transition stack.
    Transition(usize),
}

/// A script run on the stack of runs.
struct Run {
    task: Task,
    /// Arguments of the first resume.
    args: Option<Vec<Value>>,
    block: Option<Block>,
    /// The `on_enter_view` run of a transition (the transition goes on
    /// when it ends).
    enter_view: bool,
    /// The chapter run of state 1 (+0x50 is cleared when it ends).
    chapter: bool,
    /// An error in this run is not caught below `CAtlantis5::MainLoop`
    /// (`InitChapter`, a new game's files, the event step's
    /// `Atl_Hero_ArrangeHandledObject`): it ends the program.
    fatal: bool,
    /// An `Atl_Address_Go` this run reached where it could not stop
    /// (under `dofile`, `require` or `pcall`): its transition starts when
    /// the run ends.
    deferred: Option<Pending>,
}

/// `Atl_Address_Go`'s transition (the step @ 0x0042d900 on the object
/// set up @ 0x0042d580 / 0x0042d610).
#[derive(Clone, Debug)]
struct Transition {
    scene: Vec<u8>,
    view: i32,
    fx: Fx,
    /// +0x94: 0 before the view change, 1 the change, 2 after.
    phase: u32,
    /// +0x08: the time in the phase (seconds, single float).
    t: f32,
    /// +0x10: the camera's field of view when the transition started
    /// (radians; the camera's +0xc0).
    fov0: f32,
    /// The view change happened and its `on_enter_view` run is going.
    entering: bool,
    /// +0x98: the camera's mode when a `centring` started.
    saved_mode: u32,
}

/// A named timer (`Atl_Timer_Create`, the timer list @ 0x00435770: a map
/// by name, so the timers of a frame end in name order). The map node
/// holds the name at +0x10 and the timer at +0x20 (@ 0x00435ca0 gives
/// node + 0x10, @ 0x00435ae3 adds 0x10), so the node's +0x64 that the
/// walk @ 0x00435a70 tests is the timer's +0x44.
///
/// The clock (@ 0x00468170) is QueryPerformanceCounter / frequency:
/// seconds.
#[derive(Clone, Debug, PartialEq)]
struct Timer {
    /// +0x30: the duration (seconds).
    duration: f64,
    /// +0x28: the start (@ 0x004680c0: now − its argument).
    start: f64,
    /// +0x44: the repeats left; -1 for ever.
    repeat: i32,
}

/// A walk of the timers (@ 0x00435a70) whose `timer` handler waits (a
/// transition, a film): the original is inside the handler, so the walk
/// goes on after it, from the next name.
#[derive(Clone, Debug)]
struct TimerWalk {
    /// The timer whose handler runs.
    name: Vec<u8>,
    /// Its time run at the step: the step's +0x10 (@ 0x00468120) − its
    /// start +0x28, which the handler does not change.
    elapsed: f64,
}

/// The game.
pub struct Game {
    script: Script,
    engine: EngineRef,
    /// +0x30 and +0x34.
    state: u32,
    previous: u32,
    queue: VecDeque<u32>,
    /// +0x50: the chapter `Atl_InitChapter` gave.
    chapter: u32,
    runs: Vec<Run>,
    transitions: Vec<Transition>,
    timers: BTreeMap<Vec<u8>, Timer>,
    /// The walk of the timers that waits for a handler.
    timer_walk: Option<TimerWalk>,
    /// The game clock (seconds).
    clock: f64,
    /// The camera's field of view as the engine last set it (radians),
    /// which `zoom_and_fade` starts from.
    fov: f32,
}

impl Game {
    /// A game with its Lua state (no game started).
    pub fn new(engine: EngineRef) -> rilua::LuaResult<Self> {
        Ok(Self {
            script: Script::new(engine.clone())?,
            engine,
            state: 0,
            previous: 0,
            queue: VecDeque::new(),
            chapter: 0,
            runs: Vec::new(),
            transitions: Vec::new(),
            timers: BTreeMap::new(),
            timer_walk: None,
            clock: 0.0,
            fov: 0.0,
        })
    }

    pub fn script(&mut self) -> &mut Script {
        &mut self.script
    }

    /// The manager's state (+0x30).
    pub fn state(&self) -> u32 {
        self.state
    }

    /// Whether a script is running or waiting (the original is inside a
    /// handler's loop or a script; no event is taken then).
    pub fn busy(&self) -> bool {
        !self.runs.is_empty() || !self.transitions.is_empty()
    }

    /// The effect of the innermost `Atl_Address_Go` transition running
    /// (the loop @ 0x00405390 that draws the frames), none outside one.
    pub fn transition_fx(&self) -> Option<Fx> {
        self.transitions.last().map(|t| t.fx)
    }

    /// Whether requests wait in the manager's queue (+0x2c).
    pub fn requests_pending(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Whether the engine is running a call the scripts wait for.
    pub fn waiting_for_engine(&self) -> bool {
        self.runs.iter().any(|r| r.block == Some(Block::Engine))
    }

    /// The names of the timers, in their order.
    pub fn timer_names(&self) -> Vec<Vec<u8>> {
        self.timers.keys().cloned().collect()
    }

    /// The field of view the engine shows (radians), kept for
    /// `zoom_and_fade`.
    pub fn set_fov(&mut self, radians: f32) {
        self.fov = radians;
    }

    fn event(&self, e: &Event<'_>) {
        self.engine.borrow_mut().event(e);
    }

    fn note(&self, s: String) {
        self.event(&Event::Note(s));
    }

    fn request(&self, r: &Request) {
        self.engine.borrow_mut().request(r);
    }

    fn scripts_path(name: &[u8]) -> Vec<u8> {
        let mut p = SCRIPTS_DIR.to_vec();
        p.extend_from_slice(name);
        p
    }

    fn push_file(&mut self, path: &[u8], enter_view: bool, fatal: bool) -> bool {
        match self.script.file_task(path) {
            Some(task) => {
                self.runs.push(Run {
                    deferred: None,
                    task,
                    args: None,
                    block: None,
                    enter_view,
                    chapter: false,
                    fatal,
                });
                true
            }
            None => false,
        }
    }

    /// A new game (@ 0x0042fc50): the autosave deleted, the database made,
    /// `atlantis.lua` then `on_new_game.lua`. The manager then takes the
    /// queued requests (the chapter, then play).
    pub fn new_game(&mut self) {
        self.request(&Request::DeleteAutosave);
        if let Err(e) = self.script.create_database() {
            self.note(format!("CreateAtlantisDatabase: {e}"));
        }
        // Each file runs to its end before the next (EvaluateScriptFile).
        // An error in either is caught @ 0x0042feda / 0x0042ff5e, logged
        // and rethrown as `Exit Game` (0x0066cdcc): fatal.
        for name in [&b"atlantis.lua"[..], b"on_new_game.lua"] {
            let path = Self::scripts_path(name);
            if self.push_file(&path, false, true) {
                self.run();
            }
            if self.state == 8 {
                return;
            }
        }
        self.state = 0;
    }

    /// Runs a function of Native's own (tests, tools; `source` returns
    /// it, as the program's one-shot strings do) as a script run, the way
    /// a click's script runs: its calls may wait (a transition, a film).
    /// Ignored while a script runs.
    pub fn run_string(&mut self, source: &str) -> bool {
        if self.busy() {
            return false;
        }
        let Some((task, args)) = self.script.string_task(source, Vec::new()) else {
            return false;
        };
        self.runs.push(Run {
            deferred: None,
            task,
            args: Some(args),
            block: None,
            enter_view: false,
            chapter: false,
            fatal: false,
        });
        self.run();
        true
    }

    /// The event step's `return Atl_Hero_ArrangeHandledObject`
    /// (0x0066ce60, @ 0x004300d0 / 0x004305f0, outside state 4's try): as
    /// [`Game::run_string`], but an error in it ends the program.
    pub fn run_arrange_handled_object(&mut self) -> bool {
        if self.busy() {
            return false;
        }
        let Some((task, args)) = self
            .script
            .string_task("return Atl_Hero_ArrangeHandledObject", Vec::new())
        else {
            return false;
        };
        self.runs.push(Run {
            deferred: None,
            task,
            args: Some(args),
            block: None,
            enter_view: false,
            chapter: false,
            fatal: true,
        });
        self.run();
        true
    }

    /// A C-side string the program runs inside a menu's process
    /// (@ 0x00454220, e.g. the poker board's `Atl_Dialog_Play(275)`),
    /// as a script run: the original's call runs the dialog's or the
    /// film's loop before it returns; here the run waits and [`Game::busy`]
    /// holds until it ends. With `args`, the string returns a function
    /// called with them (`return Atl_PuzzlePoker_OnLose`, then
    /// `lua_pushnumber`, `lua_pcall`). Ignored while a script runs.
    pub fn run_one_shot(&mut self, source: &str, args: Option<Vec<Value>>) -> bool {
        if self.busy() {
            return false;
        }
        let (task, args) = match args {
            Some(a) => match self.script.string_task(source, a) {
                Some(t) => t,
                None => return false,
            },
            None => match self.script.chunk_task(source) {
                Some(t) => (t, Vec::new()),
                None => return false,
            },
        };
        self.runs.push(Run {
            deferred: None,
            task,
            args: Some(args),
            block: None,
            enter_view: false,
            chapter: false,
            fatal: false,
        });
        self.run();
        true
    }

    /// The `on_click_<scene>.lua` of the current view (@ 0x0042f890).
    fn run_on_click(&mut self) {
        let (scene, _) = self.script.address();
        let mut name = b"on_click_".to_vec();
        name.extend_from_slice(&scene);
        name.extend_from_slice(b".lua");
        let path = Self::scripts_path(&name);
        if self.push_file(&path, false, false) {
            self.run();
        }
    }

    /// A click on a zone of the view (@ 0x004300d0 → 0x004017c0,
    /// 0x0042f890): `atl.event` filled, then the scene's `on_click`
    /// script. Ignored while a script runs.
    pub fn click_zone(&mut self, id: i32, name: &[u8]) -> bool {
        if self.busy() || self.state != 4 {
            return false;
        }
        self.script.set_click_zone(id, name);
        self.run_on_click();
        true
    }

    /// Whether the sprite of `object` under the screen centre is a target
    /// (@ 0x00430b70 after its hit test): `GetSemantic` (@ 0x00402db0),
    /// then the object must be reachable from view `view`
    /// (`atl.objects.<name>.warp_reachable`, @ 0x004030f0). The cursor it
    /// sets (@ 0x0042bfd0): 2 for an item, and for a character 2 with
    /// something in the hand, 3 without. `None` when it is not a target.
    /// An error the original throws on (a sprite without an object, ...)
    /// is reported and the sprite is not a target.
    pub fn sprite_cursor(&mut self, object: &[u8], view: i32) -> Option<i32> {
        let semantic = match self.script.object_semantic(object) {
            Ok(s) => s,
            Err(e) => {
                self.lua_error(&e);
                return None;
            }
        };
        match self.script.warp_reachable(object, view) {
            Ok(true) => {}
            Ok(false) => return None,
            Err(e) => {
                self.lua_error(&e);
                return None;
            }
        }
        match semantic {
            Semantic::Item => Some(2),
            Semantic::Character => match self.script.handled_item() {
                Ok(h) if h.is_empty() => Some(3),
                Ok(_) => Some(2),
                Err(e) => {
                    self.lua_error(&e);
                    None
                }
            },
        }
    }

    /// A left click with a sprite under the centre (@ 0x004300d0, the
    /// manager's +0x3c): an item, or a character while the hand holds
    /// something, or a character without subjects
    /// (`Atl_Character_GetSubjects`, @ 0x00402540): the `click_sprite`
    /// event (@ 0x00401a30) and the scene's `on_click`. A character with
    /// subjects and an empty hand opens its subject menu instead
    /// ([`Request::SubjectMenu`]). Ignored while a script runs.
    pub fn click_sprite(&mut self, object: &[u8], animation: &[u8]) -> bool {
        if self.busy() || self.state != 4 {
            return false;
        }
        let semantic = match self.script.object_semantic(object) {
            Ok(s) => s,
            Err(e) => {
                self.lua_error(&e);
                return false;
            }
        };
        if semantic == Semantic::Character {
            let hand = match self.script.handled_item() {
                Ok(h) => h,
                Err(e) => {
                    self.lua_error(&e);
                    return false;
                }
            };
            if hand.is_empty() {
                match self.script.character_subjects(object) {
                    Ok(0) => {}
                    Ok(_) => {
                        // @ 0x00430464..: mode 9, @ 0x00410f80(name),
                        // then state 3 (@ 0x0042fb90).
                        self.request(&Request::SubjectMenu {
                            character: object.to_vec(),
                        });
                        self.open_menu();
                        return true;
                    }
                    Err(e) => {
                        self.lua_error(&e);
                        return false;
                    }
                }
            }
        }
        self.script.set_click_sprite(object, animation);
        self.run_on_click();
        true
    }

    /// A subject picked in a subject menu (@ 0x00401c40): the
    /// `click_subject` event and the scene's `on_click`.
    pub fn click_subject(&mut self, character: &[u8], subject: &[u8]) -> bool {
        // The subject menu runs in state 3 (@ 0x004129c0).
        if self.busy() || (self.state != 4 && self.state != 3) {
            return false;
        }
        self.script.set_click_subject(character, subject);
        self.run_on_click();
        true
    }

    /// A left press on slot `slot` of the inventory (@ 0x00417120, the
    /// grid's event 2), as the original: with something in the hand, a
    /// slot holding an item gives the `compound_inventory_object` event
    /// and the scene's `on_click` (@ 0x00401dc0, 0x0042f890), an empty slot
    /// takes the hand's item (@ 0x00403d30, 0x00403650); with an empty
    /// hand a slot's item goes into the hand (@ 0x00404120, 0x00403390).
    /// The inventory runs in state 3.
    pub fn click_inventory(&mut self, slot: usize) -> bool {
        if self.busy() || self.state != 3 {
            return false;
        }
        let hand = match self.script.handled_item() {
            Ok(h) => h,
            Err(e) => {
                self.lua_error(&e);
                return false;
            }
        };
        let (_, items) = self.script.inventory();
        let item = items.get(slot).cloned().flatten().unwrap_or_default();
        if hand.is_empty() {
            if !item.is_empty() {
                self.script.set_handled_item(Some(&item));
                self.script.inventory_clear(slot);
            }
        } else if item.is_empty() {
            self.script.set_handled_item(None);
            self.script.inventory_set(slot, &hand);
        } else {
            self.script.set_click_inventory(slot);
            self.run_on_click();
        }
        true
    }

    /// An error the original throws a `CException` on (the game ends);
    /// here it is reported.
    fn lua_error(&self, message: &str) {
        self.event(&Event::Error(
            crate::engine::ErrorSite::String(b"CLuaDatabaseManipulator".to_vec()),
            message.as_bytes(),
        ));
    }

    /// A timer's end as @ 0x00405ae0 handles it: the `timer` event and
    /// the scene's `on_click` (the timers call it when their time is up).
    pub fn timer_event(&mut self, name: &[u8]) -> bool {
        if self.busy() || self.state != 4 {
            return false;
        }
        self.script.set_timer_event(name);
        self.run_on_click();
        true
    }

    /// `atl.zones[name]` (type, active).
    pub fn zone(&mut self, name: &[u8]) -> Option<(i32, bool)> {
        self.script.zone(name)
    }

    /// The engine has ended a call the scripts wait for (a film, an
    /// animated look).
    pub fn finish_wait(&mut self) {
        if let Some(r) = self
            .runs
            .iter_mut()
            .rev()
            .find(|r| r.block == Some(Block::Engine))
        {
            r.block = None;
        }
        self.run();
    }

    /// The calls of the last resume that belong to the manager.
    fn manager_calls(&mut self) {
        for call in self.script.take_manager_calls() {
            match call {
                Call::TimerCreate {
                    name,
                    duration,
                    repeat,
                } => {
                    if self.timers.contains_key(&name) {
                        // 0x0066b010
                        self.note(format!(
                            "Atl_Timer_Create: a timer named {} exists already",
                            String::from_utf8_lossy(&name)
                        ));
                    } else {
                        // @ 0x0040c39c..0x0040c3a2: 0x004680c0(0.0),
                        // the start is now.
                        self.timers.insert(
                            name,
                            Timer {
                                duration,
                                start: self.clock,
                                repeat,
                            },
                        );
                    }
                }
                Call::TimerDestroy { name } => {
                    self.timers.remove(&name);
                }
                Call::TimerClearAll => self.timers.clear(),
                // @ 0x0040c5b0 → 0x004356b0, 0x004680a0: the duration only;
                // the time already run stays.
                Call::TimerSetDuration { name, duration } => match self.timers.get_mut(&name) {
                    Some(t) => t.duration = duration,
                    None => self.note(format!(
                        "Atl_Timer_SetDuration: no timer {}",
                        String::from_utf8_lossy(&name)
                    )),
                },
                Call::InitChapter(n) => {
                    self.chapter = n;
                    self.queue.push_back(1);
                    self.queue.push_back(4);
                }
                Call::GameOver(_) => {
                    if self.state != 3 {
                        self.queue.push_back(3);
                    }
                }
                Call::Load => self.queue.push_back(2),
                Call::Save => self.save(),
                Call::TerminateGame => self.queue.push_back(7),
                // @ 0x00405d70: mode 11 asked of the menus, then state 3
                // at once (@ 0x0042fb90(3)).
                Call::ExamineItem(_) => self.open_menu(),
                _ => {}
            }
        }
    }

    /// Runs the script runs until each one waits or none is left.
    fn run(&mut self) {
        loop {
            let Some(top) = self.runs.last_mut() else {
                return;
            };
            if top.block.is_some() {
                return;
            }
            let task = top.task.clone();
            let args = top.args.take().unwrap_or_default();
            let what = task.what.clone();
            self.event(&Event::Run(&what));
            let depth = self.runs.len();
            let outcome = self.script.resume(&task, &args);
            if let Some(p) = self.script.take_deferred() {
                if let Some(r) = self.runs.get_mut(depth - 1) {
                    r.deferred = Some(p);
                }
            }
            self.manager_calls();
            match outcome {
                Outcome::Finished => {
                    self.event(&Event::End(&what));
                    self.end_run();
                }
                Outcome::Error(m) => {
                    // The original throws (EvaluateScriptFile @ 0x00454410).
                    // Most runs are caught where they start (a scene's
                    // on_click @ 0x0042fa35, on_enter_view in the
                    // transition @ 0x0042dd99..., a save's read
                    // @ 0x0043b884): logged, the game goes on, as here.
                    // `InitChapter`, a new game's files and the event
                    // step's `Atl_Hero_ArrangeHandledObject` reach
                    // `CAtlantis5::MainLoop`'s catch (@ 0x0043d456), which
                    // logs and calls exit(0): the program ends.
                    let site = if what.starts_with(b"./") {
                        crate::engine::ErrorSite::File(what.clone())
                    } else {
                        crate::engine::ErrorSite::String(what.clone())
                    };
                    self.event(&Event::Error(site, &m));
                    if self.runs.last().is_some_and(|r| r.fatal) {
                        self.exit_on_error();
                        return;
                    }
                    self.end_run();
                }
                Outcome::Waiting(Pending::Engine(_)) => {
                    if let Some(r) = self.runs.last_mut() {
                        r.block = Some(Block::Engine);
                    }
                    return;
                }
                Outcome::Waiting(Pending::Transition { scene, view, fx }) => {
                    let depth = self.transitions.len();
                    if let Some(r) = self.runs.last_mut() {
                        r.block = Some(Block::Transition(depth));
                    }
                    self.start_transition(scene, view, fx);
                    return;
                }
            }
        }
    }

    fn end_run(&mut self) {
        let Some(run) = self.runs.pop() else {
            return;
        };
        if run.enter_view {
            if let Some(t) = self.transitions.last_mut() {
                t.entering = false;
            }
            // no_fx and centring end with the view change.
            self.continue_transition(0.0);
        }
        if run.chapter {
            // @ 0x0042f000 case 1: +0x50 = 0 once `InitChapter` has
            // returned without an error.
            self.chapter = 0;
        }
        if let Some(Pending::Transition { scene, view, fx }) = run.deferred {
            self.start_transition(scene, view, fx);
        }
    }

    /// The transition's set-up (@ 0x0042d580, 0x0042d8d0 → 0x0042d610):
    /// the camera's field of view kept, time and phase 0; for `centring`
    /// the camera's mode kept and its turn toward the zone under the
    /// centre started (the engine's [`crate::Engine::centring_start`]).
    /// Then the handler's loop (@ 0x00405390) steps it once with a time of
    /// 0 before its first frame.
    fn start_transition(&mut self, scene: Vec<u8>, view: i32, fx: Fx) {
        let saved_mode = if fx == Fx::Centring {
            self.engine.borrow_mut().centring_start()
        } else {
            0
        };
        self.transitions.push(Transition {
            scene,
            view,
            fx,
            phase: 0,
            t: 0.0,
            fov0: self.fov,
            entering: false,
            saved_mode,
        });
        self.continue_transition(0.0);
    }

    /// The view change of the step (@ 0x0042d900): the view loaded
    /// (@ 0x0044b590), `atl.address` set (@ 0x00401f20), then
    /// `on_enter_view_<scene>.lua` (missing: logged, nothing runs).
    fn change_view(&mut self) {
        let Some(t) = self.transitions.last_mut() else {
            return;
        };
        let (scene, view) = (t.scene.clone(), t.view);
        t.entering = true;
        self.request(&Request::LoadView {
            scene: scene.clone(),
            view,
        });
        self.script.set_entered_view(&scene, view);
        let mut name = b"on_enter_view_".to_vec();
        name.extend_from_slice(&scene);
        name.extend_from_slice(b".lua");
        let path = Self::scripts_path(&name);
        if self.push_file(&path, true, false) {
            self.run();
        } else if let Some(t) = self.transitions.last_mut() {
            t.entering = false;
            self.continue_transition(0.0);
        }
    }

    /// One step of the transition on top, `dt` seconds on.
    #[allow(clippy::cast_possible_truncation)]
    fn continue_transition(&mut self, dt: f32) {
        let Some(t) = self.transitions.last_mut() else {
            return;
        };
        if t.entering {
            return;
        }
        let pi = f32::from_bits(0x4049_0fdb);
        // 0x00655748: 1/9 (20° of π).
        let ninth = f32::from_bits(0x3de3_8e39);
        let done = match (t.fx, t.phase) {
            // no_fx (fx 0): the view change; the step ends with it.
            (Fx::NoFx, 0) => {
                t.phase = 1;
                self.change_view();
                false
            }
            (Fx::NoFx, _) => true,
            // centring (fx 1), phase 0: t += dt and the camera's step
            // (@ 0x0042cf80) until it reports its turn's end; then t = 0,
            // phase 1.
            (Fx::Centring, 0) => {
                t.t += dt;
                if self.engine.borrow_mut().camera_step(dt) {
                    let t = self.transitions.last_mut().expect("transition");
                    t.t = 0.0;
                    t.phase = 1;
                }
                false
            }
            // Phase 1: the kept mode back (@ 0x0042caa0 / 0x0042cab0), then
            // the view change as fx 0.
            (Fx::Centring, 1) => {
                t.phase = 2;
                let mode = t.saved_mode;
                self.engine.borrow_mut().camera_restore(mode);
                self.change_view();
                false
            }
            (Fx::Centring, _) => true,
            (Fx::ZoomAndFade, _) => match t.phase {
                0 => {
                    // t += dt; k = min(t · 4, 1) (0x00655740);
                    // fov = fov0 − k · π · (1/9); phase 1 after 0.25 s.
                    t.t += dt;
                    let k = (t.t * 4.0).min(1.0);
                    let fov = t.fov0 - k * pi * ninth;
                    self.engine.borrow_mut().request(&Request::TransitionFov {
                        radians: fov,
                        fade: k,
                    });
                    if t.t > 0.25 {
                        t.phase = 1;
                    }
                    false
                }
                1 => {
                    t.phase = 2;
                    t.t = 0.0;
                    self.change_view();
                    false
                }
                _ => {
                    // k = max(1 − t · 2.857143 (0x0065574c), 0);
                    // fov = (π/9 + fov0) − (1 − k) · π/9; ends after 0.35 s
                    // (0x00655744) with fov0.
                    t.t += dt;
                    let k = (1.0 - t.t * f32::from_bits(0x4036_db6e)).max(0.0);
                    let fov = (pi * ninth + t.fov0) - (1.0 - k) * pi * ninth;
                    if t.t <= f32::from_bits(0x3eb3_3333) {
                        self.engine.borrow_mut().request(&Request::TransitionFov {
                            radians: fov,
                            fade: k,
                        });
                        false
                    } else {
                        self.engine.borrow_mut().request(&Request::TransitionFov {
                            radians: t.fov0,
                            fade: 0.0,
                        });
                        true
                    }
                }
            },
        };
        if done {
            let depth = self.transitions.len() - 1;
            self.transitions.pop();
            self.request(&Request::TransitionEnd);
            if let Some(r) = self
                .runs
                .iter_mut()
                .rev()
                .find(|r| r.block == Some(Block::Transition(depth)))
            {
                r.block = None;
            }
            self.run();
        }
    }

    /// `CAtlantis5::MainLoop`'s catch (@ 0x0043d320 → 0x0043d456): `--->
    /// ATLINT::CException non attrapée...` and the message logged, then
    /// exit(0) (@ 0x004896d0), no message box: the runs dropped and the
    /// quit state 8 taken at once.
    fn exit_on_error(&mut self) {
        self.note("an uncaught script error: the program ends (MainLoop's catch)".into());
        self.drop_runs();
        self.transitions.clear();
        self.queue.clear();
        self.previous = self.state;
        self.state = 8;
    }

    /// Drops every run, waiting or not, and their tasks with them.
    fn drop_runs(&mut self) {
        for run in self.runs.drain(..) {
            self.script.drop_task(&run.task);
        }
    }

    /// The next request (@ 0x0042ef30 + 0x0042fb90).
    fn next_request(&mut self) {
        let next = self.queue.pop_front().unwrap_or(self.previous);
        if next != self.state {
            self.note(format!("game manager: state {} -> {next}", self.state));
            self.previous = self.state;
            self.state = next;
        }
    }

    /// A menu opens: state 3 (@ 0x0042fb90(3)), the state left kept for
    /// the return (@ 0x0042ef30 takes it when no request is queued).
    pub fn open_menu(&mut self) {
        if self.state != 3 {
            self.note(format!("game manager: state {} -> 3", self.state));
            self.previous = self.state;
            self.state = 3;
        }
    }

    /// A request queued (@ 0x0045f3a0 on the manager's deque): the
    /// menus' `resume` 4 (@ 0x0041ad60), the main menu's quit 8, the
    /// in-game menu's exit 3.
    pub fn queue_request(&mut self, r: u32) {
        self.queue.push_back(r);
    }

    /// The game ended (@ 0x0042ffd0, its manager's part): the requests
    /// dropped, the states, +0x38, +0x3c and the flags +0x54, +0x55
    /// cleared, the timers cleared (@ 0x004359d0). The scripts still
    /// running are dropped with it (the original runs none from a menu).
    pub fn end_game(&mut self) {
        self.queue.clear();
        self.state = 0;
        self.previous = 0;
        self.drop_runs();
        self.transitions.clear();
        self.timers.clear();
        self.timer_walk = None;
    }

    /// State 7, the end of a game after `Atl_TerminateGame` (@ 0x0042f000
    /// case 7): `saves/autoslot` (the save folder, path 0x27, and the
    /// name at 0x0066a07c) deleted when it exists (0x0042f562..0x0042f576),
    /// the game ended (@ 0x0042ffd0: the manager's part here, the
    /// engine's in [`Request::GameEnded`]), the menus asked for mode 1,
    /// the credits (@ 0x0041b5d0 with 1, 0x0042f588), the request 3
    /// queued (0x0042f6fa) and the next request taken: state 3.
    fn terminate(&mut self) {
        self.note("game manager: the end of the game".into());
        self.request(&Request::DeleteAutosave);
        self.end_game();
        self.request(&Request::GameEnded);
        self.queue.push_back(3);
        self.next_request();
    }

    /// The menu of state 3 has ended (its process returned 1,
    /// @ 0x0042f000 case 3 falling into case 0): the next request.
    pub fn menu_closed(&mut self) {
        if self.state == 3 {
            self.next_request();
        }
    }

    /// `Atl_Save` (@ 0x00408fb0 → 0x0043ad90): `saves/autoslot`, an
    /// `ATL_FILE` with `LUA_DB` (taken at the call), the engine's
    /// sections, then `CALLBACK_SHOOTER_MANAGER` (@ 0x00436f20): one
    /// `CALLBACK_SHOOTER` per timer with `name`, `duration` and
    /// `remaining_time` (floats), `repeat` (a `VT_I4`), holding one
    /// `CALLBACK` with the handler's `name` (`OnTimeOut`, 0x0066b004).
    /// The configuration's section is not written (Native keeps the
    /// options elsewhere).
    fn save(&mut self) {
        let _ = self.save_as(crate::save::AUTOSLOT);
    }

    /// The save written to `saves/<file>` (the save menu's
    /// `CSaveFileWriter` @ 0x0043ad90 on `slot_<n>`, @ 0x004272b0; the
    /// database taken now, as `Atl_Save` does it at its call).
    /// False when the save could not be made or written.
    pub fn save_now(&mut self, file: &str) -> bool {
        // An error is reported by `save_as`, which takes it.
        let _ = self.script.save_db_now();
        self.save_as(file)
    }

    fn save_as(&mut self, file: &str) -> bool {
        use crate::save;
        let db = match self.script.take_saved_db() {
            Some(Ok(db)) => db,
            Some(Err(e)) => {
                self.lua_error(&e);
                return false;
            }
            None => return false,
        };
        let mut root = save::element(save::ROOT);
        root.attributes.push(("type".into(), save::TYPE.into()));
        root.children.push(db);
        let sections = self.engine.borrow_mut().save_sections();
        root.children.extend(sections);
        let mut manager = save::element("CALLBACK_SHOOTER_MANAGER");
        for (name, t) in &self.timers {
            let mut c = save::element("CALLBACK_SHOOTER");
            c.attributes
                .push(("name".into(), atlantis4_data::xml::cp1252(name)));
            #[allow(clippy::cast_possible_truncation)]
            {
                save::attr_f32(&mut c, "duration", t.duration as f32);
                let remaining = t.duration - (self.clock - t.start);
                save::attr_f32(&mut c, "remaining_time", remaining as f32);
            }
            c.attributes.push(("repeat".into(), t.repeat.to_string()));
            let mut cb = save::element("CALLBACK");
            cb.attributes.push(("name".into(), "OnTimeOut".into()));
            c.children.push(cb);
            manager.children.push(c);
        }
        root.children.push(manager);
        let text = save::to_text(&root);
        if !self.engine.borrow_mut().write_save(file, text.as_bytes()) {
            self.note("Atl_Save: the save could not be written".into());
            return false;
        }
        true
    }

    /// State 2 (@ 0x0042f000): `saves/autoslot` (missing: `Atl_Load usage
    /// : Le fichier %s introuvable`); when it is there the game is ended
    /// first (@ 0x0042ffd0: the queue emptied, the states, timers, scene,
    /// camera controller, sounds and configuration reset; the manager's
    /// part here, the engine's in [`Request::GameReset`]), then the file
    /// read by @ 0x0043b210: `atlantis.lua` run again, `LUA_DB` made the
    /// global `atl`, the engine's sections, the timers
    /// (`CALLBACK_SHOOTER_MANAGER`, each started with its remaining time),
    /// then `return Atl_Address_GetCurrent()` and a `no_fx` transition
    /// there (@ 0x0042d8d0(0), 0x0042d8e0). The menus' loads
    /// (@ 0x00428ea0, 0x00418960) call the reader without ending the
    /// game.
    fn load(&mut self) {
        let file = crate::save::AUTOSLOT;
        let Some(data) = self.engine.borrow_mut().read_save(file) else {
            self.note(format!("Atl_Load: the file saves/{file} is missing"));
            return;
        };
        self.end_game();
        self.request(&Request::GameReset);
        self.read_save(&data);
    }

    /// `saves/<file>` read by @ 0x0043b210 (the main menu's
    /// `rescue_game` @ 0x00428ea0, the load menu's `slot_<n>`
    /// @ 0x00418960); at its end the reader queues the requests 5 (the
    /// transition, here [`Game`]'s own) and 4 (0x0043b6f4..0x0043b7c1).
    /// False when the file is missing.
    pub fn load_file(&mut self, file: &str) -> bool {
        let Some(data) = self.engine.borrow_mut().read_save(file) else {
            self.note(format!("Atl_Load: the file saves/{file} is missing"));
            return false;
        };
        self.read_save(&data)
    }

    /// The reader @ 0x0043b210 on a save's bytes.
    fn read_save(&mut self, data: &[u8]) -> bool {
        use crate::save;
        let Some(root) = save::parse(data) else {
            self.note("CSaveFileReader :: Read -> not an ATL_FILE".into());
            return false;
        };
        let path = Self::scripts_path(b"atlantis.lua");
        if self.push_file(&path, false, false) {
            self.run();
        }
        match root.child("LUA_DB") {
            Some(db) => {
                if let Err(e) = self.script.load_lua_db(db) {
                    self.lua_error(&e);
                }
            }
            None => self.note("CSerialisableLuaDB :: Deserialize: no <LUA_DB> element".into()),
        }
        self.engine.borrow_mut().load_sections(&root);
        self.timers.clear();
        if let Some(m) = root.child("CALLBACK_SHOOTER_MANAGER") {
            for c in m.children_named("CALLBACK_SHOOTER") {
                let Some(name) = c.attr("name") else { continue };
                let duration = save::get_f64(c, "duration").unwrap_or(0.0);
                let remaining = save::get_f64(c, "remaining_time").unwrap_or(duration);
                #[allow(clippy::cast_possible_truncation)]
                let repeat = save::get_f64(c, "repeat").unwrap_or(-1.0) as i32;
                self.timers.insert(
                    atlantis4_data::xml::to_cp1252(name),
                    Timer {
                        duration: f64::from(duration as f32),
                        start: self.clock - (duration - remaining),
                        repeat,
                    },
                );
            }
        }
        match self.script.address_get_current() {
            Ok((scene, view)) => {
                #[allow(clippy::cast_possible_truncation)]
                let view = view as i32;
                self.start_transition(scene, view, Fx::NoFx);
            }
            Err(e) => self.lua_error(&e),
        }
        self.queue.push_back(4);
        true
    }

    /// State 1 (@ 0x0042f000 case 1): the next request taken first
    /// (@ 0x0042ef30, 0x0042fb90: normally 4, so the chapter's script runs
    /// in play's state), then `return InitChapter` called with the
    /// scripts' folder and the chapter (+0x50, nil for 0); +0x50 is set to
    /// 0 after the call returns (so an `Atl_InitChapter(n)` made inside it
    /// loses its n: the next state 1 reloads the current chapter).
    fn init_chapter(&mut self) {
        let chapter = if self.chapter == 0 {
            Value::Nil
        } else {
            Value::Num(f64::from(self.chapter))
        };
        self.next_request();
        let path = Value::Str(SCRIPTS_DIR.to_vec());
        match self
            .script
            .string_task("return InitChapter", vec![path, chapter])
        {
            Some((task, args)) => {
                self.runs.push(Run {
                    deferred: None,
                    task,
                    args: Some(args),
                    block: None,
                    enter_view: false,
                    chapter: true,
                    // State 1's `return InitChapter` (@ 0x0042f000, whose
                    // only try is state 4's): not caught below MainLoop.
                    fatal: true,
                });
                self.run();
            }
            None => self.chapter = 0,
        }
    }

    /// The timers (@ 0x00435a70), in name order. Each step
    /// (@ 0x004353d0): the timer's +0x10 = now (@ 0x00468120); remaining =
    /// duration − (now − start) (@ 0x00468060); remaining ≤ 0 (0x00655468)
    /// runs its `OnTimeOut` handler (@ 0x00405ae0: the `timer` event, the
    /// scene's `on_click`), then the repeats (+0x44) go down when positive
    /// and, when not 0, the timer starts again (0x004354bf..0x004354cc:
    /// remaining taken again with the duration after the handler, then
    /// 0x004680c0(remaining), start = now − remaining, so the overshoot
    /// lengthens the next period). After the step the walk erases the
    /// timer when its +0x44 is 0 (0x00435aef → 0x00435ea0), whether it
    /// ran or not.
    fn timers(&mut self) {
        let mut after: Option<Vec<u8>> = None;
        if let Some(walk) = self.timer_walk.take() {
            self.end_timer_step(&walk.name, Some(walk.elapsed));
            after = Some(walk.name);
        }
        loop {
            let next = match &after {
                None => self.timers.keys().next().cloned(),
                Some(a) => self
                    .timers
                    .range::<Vec<u8>, _>((std::ops::Bound::Excluded(a), std::ops::Bound::Unbounded))
                    .next()
                    .map(|(k, _)| k.clone()),
            };
            let Some(name) = next else {
                return;
            };
            let t = &self.timers[&name];
            let elapsed = self.clock - t.start;
            if t.duration - elapsed <= 0.0 {
                self.timer_event(&name);
                if self.busy() {
                    self.timer_walk = Some(TimerWalk { name, elapsed });
                    return;
                }
                self.end_timer_step(&name, Some(elapsed));
            } else {
                self.end_timer_step(&name, None);
            }
            after = Some(name);
        }
    }

    /// The end of a timer's step: after its handler (`ran` = its time
    /// run at the step), the repeats and the restart; then the walk's
    /// erase when +0x44 is 0.
    ///
    /// The restart calls @ 0x00468060 a second time after the handler
    /// loop (0x004354bf), so the remaining time is taken with the
    /// duration as the handler left it (`Atl_Timer_SetDuration`,
    /// @ 0x004680a0, writes +0x30 only) and the step's +0x10; then
    /// @ 0x004680c0 sets start = now − remaining. A handler that changes
    /// its own timer's duration thus gets one more period of the time run
    /// at the step (about the old duration) before the new one applies.
    fn end_timer_step(&mut self, name: &[u8], ran: Option<f64>) {
        let clock = self.clock;
        let Some(t) = self.timers.get_mut(name) else {
            // Destroyed by its handler.
            return;
        };
        if let Some(elapsed) = ran {
            if t.repeat > 0 {
                t.repeat -= 1;
            }
            if t.repeat != 0 {
                t.start = clock - (t.duration - elapsed);
            }
        }
        if t.repeat == 0 {
            self.timers.remove(name);
        }
    }

    /// One frame of `dt` seconds.
    pub fn update(&mut self, dt: f64) {
        self.clock += dt;
        self.run();
        if self.waiting_for_engine() {
            return;
        }
        if !self.transitions.is_empty() {
            #[allow(clippy::cast_possible_truncation)]
            self.continue_transition(dt as f32);
            return;
        }
        if !self.runs.is_empty() {
            return;
        }
        match self.state {
            0 => self.next_request(),
            1 => self.init_chapter(),
            4 => {
                // @ 0x0042f000 case 4: the camera, the events
                // (@ 0x004300d0), the timers (@ 0x00435a70), and only then
                // a queued request (+0x2c) taken (@ 0x0042fb90), in the
                // same frame. A walk of the timers that waited for a
                // handler ends in the frame the handler ended in (the
                // original is inside the handler meanwhile), and the
                // request follows it.
                self.timers();
                if self.timer_walk.is_some() || self.busy() {
                    return;
                }
                if !self.queue.is_empty() {
                    self.next_request();
                }
            }
            2 => {
                self.load();
                self.next_request();
            }
            7 => self.terminate(),
            s => {
                // 3: a menu, driven by the engine's side (`open_menu`,
                // `menu_closed`).
                let _ = s;
            }
        }
    }
}

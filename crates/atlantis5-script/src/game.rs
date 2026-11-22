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
//!   set @ 0x0042fb90) when state 1 ends, in state 0, and in state 4 at
//!   the start of a frame when the queue is not empty.
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
    /// The chapter run of state 1 (the next request is taken when it ends).
    chapter: bool,
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
    /// Its remaining time at the step (@ 0x00468060 with the step's
    /// +0x10, which the handler does not change).
    remaining: f64,
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

    fn push_file(&mut self, path: &[u8], enter_view: bool) -> bool {
        match self.script.file_task(path) {
            Some(task) => {
                self.runs.push(Run {
                    task,
                    args: None,
                    block: None,
                    enter_view,
                    chapter: false,
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
        for name in [&b"atlantis.lua"[..], b"on_new_game.lua"] {
            let path = Self::scripts_path(name);
            if self.push_file(&path, false) {
                self.run();
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
            task,
            args: Some(args),
            block: None,
            enter_view: false,
            chapter: false,
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
        if self.push_file(&path, false) {
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
            let outcome = self.script.resume(&task, &args);
            self.manager_calls();
            match outcome {
                Outcome::Finished => {
                    self.event(&Event::End(&what));
                    self.end_run();
                }
                Outcome::Error(m) => {
                    // The original throws (EvaluateScriptFile @ 0x00454410,
                    // Process @ 0x0042f000 for the chapter) and the game
                    // stops; Native reports it and ends that run.
                    let site = if what.starts_with(b"./") {
                        crate::engine::ErrorSite::File(what.clone())
                    } else {
                        crate::engine::ErrorSite::String(what.clone())
                    };
                    self.event(&Event::Error(site, &m));
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
            self.next_request();
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
        if self.push_file(&path, true) {
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
        use crate::save;
        let db = match self.script.take_saved_db() {
            Some(Ok(db)) => db,
            Some(Err(e)) => {
                self.lua_error(&e);
                return;
            }
            None => return,
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
        if !self
            .engine
            .borrow_mut()
            .write_save(save::AUTOSLOT, text.as_bytes())
        {
            self.note("Atl_Save: the save could not be written".into());
        }
    }

    /// State 2 (@ 0x0042f000): `saves/autoslot` (missing: `Atl_Load usage
    /// : Le fichier %s introuvable`), read by @ 0x0043b210: `atlantis.lua`
    /// run again, `LUA_DB` made the global `atl`, the engine's sections,
    /// the timers (`CALLBACK_SHOOTER_MANAGER`, each started with its
    /// remaining time), then `return Atl_Address_GetCurrent()` and a
    /// `no_fx` transition there (@ 0x0042d8d0(0), 0x0042d8e0).
    fn load(&mut self) {
        use crate::save;
        let Some(data) = self.engine.borrow_mut().read_save(save::AUTOSLOT) else {
            self.note(format!(
                "Atl_Load usage : Le fichier saves/{} introuvable",
                save::AUTOSLOT
            ));
            return;
        };
        let Some(root) = save::parse(&data) else {
            self.note("CSaveFileReader :: Read -> not an ATL_FILE".into());
            return;
        };
        let path = Self::scripts_path(b"atlantis.lua");
        if self.push_file(&path, false) {
            self.run();
        }
        match root.child("LUA_DB") {
            Some(db) => {
                if let Err(e) = self.script.load_lua_db(db) {
                    self.lua_error(&e);
                }
            }
            None => self.note(
                "CSerialisableLuaDB :: Deserialize -> Grappe de donnée <LUA_DB> manquante".into(),
            ),
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
    }

    /// State 1: `return InitChapter` called with the scripts' folder and
    /// the chapter (nil for 0), then the next request.
    fn init_chapter(&mut self) {
        let chapter = if self.chapter == 0 {
            Value::Nil
        } else {
            Value::Num(f64::from(self.chapter))
        };
        self.chapter = 0;
        let path = Value::Str(SCRIPTS_DIR.to_vec());
        match self
            .script
            .string_task("return InitChapter", vec![path, chapter])
        {
            Some((task, args)) => {
                self.runs.push(Run {
                    task,
                    args: Some(args),
                    block: None,
                    enter_view: false,
                    chapter: true,
                });
                self.run();
            }
            None => self.next_request(),
        }
    }

    /// The timers (@ 0x00435a70), in name order. Each step
    /// (@ 0x004353d0): the timer's +0x10 = now (@ 0x00468120); remaining =
    /// duration − (now − start) (@ 0x00468060); remaining ≤ 0 (0x00655468)
    /// runs its `OnTimeOut` handler (@ 0x00405ae0: the `timer` event, the
    /// scene's `on_click`), then the repeats (+0x44) go down when positive
    /// and, when not 0, the timer starts again (0x004354bf..0x004354cc:
    /// 0x004680c0(remaining), start = now − remaining, so the overshoot
    /// lengthens the next period). After the step the walk erases the
    /// timer when its +0x44 is 0 (0x00435aef → 0x00435ea0), whether it
    /// ran or not.
    fn timers(&mut self) {
        let mut after: Option<Vec<u8>> = None;
        if let Some(walk) = self.timer_walk.take() {
            self.end_timer_step(&walk.name, Some(walk.remaining));
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
            let remaining = t.duration - (self.clock - t.start);
            if remaining <= 0.0 {
                self.timer_event(&name);
                if self.busy() {
                    self.timer_walk = Some(TimerWalk { name, remaining });
                    return;
                }
                self.end_timer_step(&name, Some(remaining));
            } else {
                self.end_timer_step(&name, None);
            }
            after = Some(name);
        }
    }

    /// The end of a timer's step: after its handler (`ran` = its
    /// remaining time at the step), the repeats and the restart; then the
    /// walk's erase when +0x44 is 0.
    fn end_timer_step(&mut self, name: &[u8], ran: Option<f64>) {
        let clock = self.clock;
        let Some(t) = self.timers.get_mut(name) else {
            // Destroyed by its handler.
            return;
        };
        if let Some(remaining) = ran {
            if t.repeat > 0 {
                t.repeat -= 1;
            }
            if t.repeat != 0 {
                t.start = clock - remaining;
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
                // A walk of the timers that waited for a handler ends in
                // the frame the handler ended in.
                if self.timer_walk.is_some() {
                    self.timers();
                    return;
                }
                if !self.queue.is_empty() {
                    self.next_request();
                    return;
                }
                self.timers();
            }
            2 => {
                self.load();
                self.next_request();
            }
            s => {
                // 3: a menu, driven by the engine's side (`open_menu`,
                // `menu_closed`); 7 (end of a game): not ported.
                let _ = s;
            }
        }
    }
}

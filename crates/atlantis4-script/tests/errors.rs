//! The error policy and the node change, on scripts made up here (no game
//! data).
//!
//! The original runs a file under `lua_pcall` and drops its error; it
//! calls callbacks with a bare `luaD_call`, so an error in a callback the
//! engine called reaches no handler and the game exits, while an error in
//! a callback reached from Lua (`ChangeWarp` → `OnWarpEnter`) goes up to
//! the Lua caller.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use atlantis4_script::record::RecordingEngine;
use atlantis4_script::{
    new_game, Call, Collector, Engine, EngineRef, ErrorMode, Event, Options, Reply, SavedGlobal,
    Script, ScriptError, Value,
};

/// The recording stub, with scripts from memory, where `ChangeWarp`
/// changes the node at once as the game does.
struct Warping(RecordingEngine);

impl Engine for Warping {
    fn call(&mut self, call: &Call) -> Reply {
        let r = self.0.call(call);
        if call.binding == atlantis4_script::tolua::Binding::ChangeWarp {
            Reply::Warp
        } else {
            r
        }
    }
    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        self.0.read_script(path)
    }
    fn has_scene(&self) -> bool {
        self.0.has_scene()
    }
    fn save_game(&mut self, file: &[u8], globals: &[SavedGlobal]) {
        self.0.save_game(file, globals);
    }
    fn enter_scene(&mut self, path: &[u8]) {
        self.0.enter_scene(path);
    }
    fn load_scene_xml(&mut self, xml: &[u8]) {
        self.0.load_scene_xml(xml);
    }
    fn event(&mut self, event: &Event<'_>) {
        self.0.event(event);
    }
}

fn files(list: &[(&str, &str)]) -> HashMap<Vec<u8>, Vec<u8>> {
    list.iter()
        .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
        .collect()
}

fn engine(scripts: HashMap<Vec<u8>, Vec<u8>>) -> (Rc<RefCell<Warping>>, EngineRef) {
    let read = move |p: &[u8]| scripts.get(p).cloned();
    let rec = Rc::new(RefCell::new(Warping(RecordingEngine::new(
        Box::new(read),
        true,
    ))));
    let e: EngineRef = rec.clone();
    (rec, e)
}

fn trace(rec: &Rc<RefCell<Warping>>) -> String {
    String::from_utf8_lossy(&rec.borrow().0.out).into_owned()
}

fn options(errors: ErrorMode) -> Options {
    Options {
        errors,
        collector: Collector::Stopped,
    }
}

const NODES: &[(&str, &str)] = &[
    ("3D/OnGameEnter.lua", "started = true"),
    (
        "3D/T/0001.lua",
        "function OnWarpEnter() PlayMusic('a.ogg') end\n\
        function OnWarpObjectHit(name) ChangeWarp(name) PlayMusic('after.ogg') end",
    ),
    (
        "3D/T/0002.lua",
        "function OnWarpEnter() PlayMusic('b.ogg') error('broken') end",
    ),
    (
        "3D/T/0003.lua",
        "ChangeWarp('3D\\\\T\\\\0002.wp') PlayMusic('never.ogg')",
    ),
];

#[test]
fn a_callback_error_is_fatal_in_strict_mode() {
    let (_rec, e) = engine(files(&[(
        "3D/T/0001.lua",
        "function OnWarpEnter() local x = nil + 1 end",
    )]));
    let err = new_game(&e, options(ErrorMode::Strict), b"3D/T/0001.wp")
        .err()
        .expect("an error");
    match &err {
        ScriptError::Fatal { callback, message } => {
            assert_eq!(callback, "OnWarpEnter");
            assert_eq!(
                String::from_utf8_lossy(message),
                "[string \"3D/T/0001.lua\"]:1: attempt to perform arithmetic on a nil value"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(err
        .to_string()
        .starts_with("Lua: [string \"3D/T/0001.lua\"]:1:"));
}

#[test]
fn a_callback_error_is_logged_and_the_game_goes_on() {
    let (rec, e) = engine(files(&[(
        "3D/T/0001.lua",
        "function OnWarpEnter() error('x') end",
    )]));
    let mut script = new_game(&e, options(ErrorMode::Log), b"3D/T/0001.wp").expect("goes on");
    script.callback("OnWarpEnter", &[]).expect("goes on");
    let t = trace(&rec);
    assert_eq!(
        t.matches("ERR OnWarpEnter: [string \"3D/T/0001.lua\"]:1: x")
            .count(),
        2,
        "{t}"
    );
}

/// A file's error is dropped in both modes, the rest of the file skipped.
#[test]
fn a_file_error_is_dropped() {
    let (rec, e) = engine(files(&[
        ("3D/OnGameEnter.lua", "a = 1 error('stop') b = 2"),
        ("3D/T/0001.lua", "function OnWarpEnter() end"),
    ]));
    let script = new_game(&e, options(ErrorMode::Strict), b"3D/T/0001.wp").expect("not fatal");
    let names: Vec<Vec<u8>> = script
        .globals()
        .expect("globals")
        .into_iter()
        .map(|g| g.name)
        .collect();
    assert!(names.contains(&b"a".to_vec()) && !names.contains(&b"b".to_vec()));
    assert!(trace(&rec).contains("ERR chunk: [string \"3D/OnGameEnter.lua\"]:1: stop"));
}

/// `ChangeWarp` from a callback changes the node inside the call: the old
/// node's `OnWarpLeave`, the autosave, the new node's file and
/// `OnWarpEnter`, then the rest of the calling callback.
#[test]
fn change_warp_runs_inside_the_binding() {
    let (rec, e) = engine(files(NODES));
    let mut script = new_game(&e, options(ErrorMode::Log), b"3D/T/0001.wp").expect("new game");
    rec.borrow_mut().0.out.clear();
    script
        .callback("OnWarpObjectHit", &[Value::Str(b"3D/T/0001.wp".to_vec())])
        .expect("ok");
    let t = trace(&rec);
    let want = "ENTER OnWarpObjectHit(\"3D/T/0001.wp\")\n\
        CALL ChangeWarp(\"3D/T/0001.wp\",true)\n\
        WARP 3D/T/0001.wp\n\
        MISSING OnWarpLeave\n";
    assert!(t.starts_with(want), "{t}");
    let tail = "SCENE 3D/T/0001.xml\n\
        LOAD 3D/T/0001.lua\n\
        ENTER OnWarpEnter()\n\
        CALL PlayMusic(\"a.ogg\")\n\
        CALL PlayMusic(\"after.ogg\")\n";
    assert!(t.contains(tail), "{t}");
    assert!(t.contains("SAVEGAME save.xml n="), "{t}");
}

/// An error in the new node's `OnWarpEnter` goes up through `ChangeWarp`
/// to the callback that called it: fatal there in strict mode, and the
/// rest of that callback does not run.
#[test]
fn an_error_in_a_nested_on_warp_enter_reaches_the_engines_call() {
    let (rec, e) = engine(files(NODES));
    let mut script = new_game(&e, options(ErrorMode::Strict), b"3D/T/0001.wp").expect("new game");
    let err = script
        .callback("OnWarpObjectHit", &[Value::Str(b"3D/T/0002.wp".to_vec())])
        .err();
    match err {
        Some(ScriptError::Fatal { callback, message }) => {
            assert_eq!(callback, "OnWarpObjectHit");
            assert_eq!(
                String::from_utf8_lossy(&message),
                "[string \"3D/T/0002.lua\"]:1: broken"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(!trace(&rec).contains("after.ogg"));
}

/// The same error under a file's `lua_pcall` is dropped with the rest of
/// the file. (The node change that ran that file then calls the
/// `OnWarpEnter` now defined, the failing one: logged.)
#[test]
fn an_error_in_a_nested_on_warp_enter_under_a_file_is_dropped() {
    let (rec, e) = engine(files(NODES));
    let mut script: Script =
        new_game(&e, options(ErrorMode::Log), b"3D/T/0001.wp").expect("new game");
    script.change_warp(b"3D/T/0003.wp").expect("logged");
    let t = trace(&rec);
    assert!(
        t.contains("ERR chunk: [string \"3D/T/0002.lua\"]:1: broken"),
        "{t}"
    );
    assert!(!t.contains("never.ogg"), "{t}");
}

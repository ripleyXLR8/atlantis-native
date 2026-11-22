//! The game manager on small scripts of this test (Lua source, which the
//! loader takes as well as 5.0 chunks): the chapter request, the
//! transitions, the timers, rejected calls and errors. No game data.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use atlantis5_script::engine::{Call, Engine, Event, Reply, Request};
use atlantis5_script::{Game, Value};

#[derive(Default)]
struct Probe {
    files: HashMap<String, String>,
    calls: Vec<String>,
    requests: Vec<Request>,
    errors: Vec<String>,
    rejected: Vec<String>,
    seen: Vec<Call>,
    wait: bool,
}

impl Engine for Probe {
    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        let p = String::from_utf8_lossy(path).into_owned();
        self.files.get(&p).map(|s| s.clone().into_bytes())
    }

    fn call(&mut self, call: &Call) -> Reply {
        self.seen.push(call.clone());
        if matches!(call, Call::SpriteGetFrame { .. }) {
            return Reply::Values(vec![7.0]);
        }
        if matches!(call, Call::VideoPlay { .. }) && self.wait {
            return Reply::Wait;
        }
        Reply::Done
    }

    fn request(&mut self, r: &Request) {
        self.requests.push(r.clone());
    }

    fn event(&mut self, e: &Event<'_>) {
        match e {
            Event::Call(b, a) => {
                let a: Vec<String> = a.iter().map(Value::trace).collect();
                self.calls.push(format!("{}({})", b.name(), a.join(", ")));
            }
            Event::Error(_, m) => self.errors.push(String::from_utf8_lossy(m).into_owned()),
            Event::Rejected(b, m) => self.rejected.push(format!("{}: {m}", b.name())),
            _ => {}
        }
    }
}

const DIR: &str = "./datas/scripts/";

fn game(files: &[(&str, &str)]) -> (Game, Rc<RefCell<Probe>>) {
    let probe = Rc::new(RefCell::new(Probe::default()));
    for (name, src) in files {
        probe
            .borrow_mut()
            .files
            .insert(format!("{DIR}{name}"), (*src).to_string());
    }
    let g = Game::new(probe.clone()).expect("game");
    (g, probe)
}

const BASE: &[(&str, &str)] = &[
    (
        "atlantis.lua",
        "require \"lib/util.lua\" \
         function InitChapter(Path, Chapter) CH = Chapter PATH = Path \
           Atl_Video_Play(\"intro\") Atl_Address_Go(\"deck\", 1, \"no_fx\") Atl_Save() end",
    ),
    ("lib/util.lua", "UTIL = (UTIL or 0) + 1"),
    (
        "on_new_game.lua",
        "Atl_InitChapter(2) AFTER_INIT = true",
    ),
    (
        "on_enter_view_deck.lua",
        "ENTERED = (ENTERED or 0) + 1 Atl_Sprite_Play(\"coin\", \"z1\", \"loop\")",
    ),
    (
        "on_click_deck.lua",
        "local t = atl.event.type \
         if t == \"click_zone\" then Atl_Address_Go(\"deck\", atl.event.click.zone.id, \"zoom_and_fade\") \
         elseif t == \"timer\" then TICKS = (TICKS or 0) + 1 \
           if atl.event.timer.name == \"bad\" then error(\"boom\") end end",
    ),
];

fn settle(g: &mut Game, n: usize) {
    for _ in 0..n {
        g.update(0.02);
    }
}

#[test]
fn the_chapter_runs_after_the_new_game_script() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    // Atl_InitChapter queued the chapter; on_new_game.lua went on.
    assert_eq!(g.script().global(b"AFTER_INIT"), Value::Bool(true));
    assert_eq!(g.script().global(b"CH"), Value::Nil);
    assert_eq!(g.script().global(b"UTIL"), Value::Num(1.0));
    settle(&mut g, 5);
    assert_eq!(g.script().global(b"CH"), Value::Num(2.0));
    assert_eq!(
        g.script().global(b"PATH"),
        Value::Str(DIR.as_bytes().to_vec())
    );
    assert_eq!(g.state(), 4);
    let p = probe.borrow();
    assert_eq!(
        p.calls,
        [
            "Atl_InitChapter(2)",
            "Atl_Video_Play(\"intro\")",
            "Atl_Address_Go(\"deck\", 1, \"no_fx\")",
            "Atl_Sprite_Play(\"coin\", \"z1\", \"loop\")",
            "Atl_Save()",
        ]
    );
    assert!(p.errors.is_empty(), "{:?}", p.errors);
}

#[test]
fn a_film_holds_the_script_until_it_ends() {
    let (mut g, probe) = game(BASE);
    probe.borrow_mut().wait = true;
    g.new_game();
    settle(&mut g, 5);
    assert!(g.waiting_for_engine());
    assert_eq!(probe.borrow().calls.len(), 2, "stopped at the film");
    settle(&mut g, 50);
    assert_eq!(probe.borrow().calls.len(), 2, "still at the film");
    g.finish_wait();
    settle(&mut g, 2);
    assert_eq!(probe.borrow().calls.len(), 5);
    assert_eq!(g.state(), 4);
}

#[test]
fn zoom_and_fade_turns_the_field_of_view() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    settle(&mut g, 5);
    g.set_fov(1.0);
    probe.borrow_mut().requests.clear();
    assert!(g.click_zone(7, b""));
    assert!(g.busy());
    // Phase 0: 0.25 s narrowing by up to π/9; the view changes after.
    let mut steps = 0;
    while g.busy() && steps < 100 {
        g.update(0.02);
        steps += 1;
    }
    assert!(!g.busy());
    let p = probe.borrow();
    let fovs: Vec<f32> = p
        .requests
        .iter()
        .filter_map(|r| match r {
            Request::TransitionFov { radians, .. } => Some(*radians),
            _ => None,
        })
        .collect();
    let load = p
        .requests
        .iter()
        .position(|r| matches!(r, Request::LoadView { view: 7, .. }))
        .expect("the view change");
    let ninth = std::f32::consts::PI / 9.0;
    // The handler's loop steps once with a time of 0 (@ 0x00405390), then
    // with each frame's.
    assert_eq!(fovs[0], 1.0, "{fovs:?}");
    assert!((fovs[1] - (1.0 - 0.08 * ninth)).abs() < 1e-5, "{fovs:?}");
    assert!(fovs
        .iter()
        .all(|f| *f >= 1.0 - ninth - 1e-5 && *f <= 1.0 + ninth + 1e-5));
    assert_eq!(*fovs.last().unwrap(), 1.0, "ends at the old field of view");
    assert!(load > 0);
    // 0.25 s + the change + 0.35 s at 20 ms frames.
    assert!((30..=34).contains(&steps), "{steps} frames");
    drop(p);
    assert_eq!(g.script().address(), (b"deck".to_vec(), 7));
    assert_eq!(g.script().global(b"ENTERED"), Value::Num(2.0));
}

#[test]
fn timers_repeat_and_end() {
    let (mut g, _probe) = game(BASE);
    g.new_game();
    settle(&mut g, 5);
    // Two timers made by a script: one for ever, one twice.
    let probe2 = _probe.clone();
    probe2.borrow_mut().files.insert(
        format!("{DIR}on_click_deck.lua"),
        "local t = atl.event.type \
         if t == \"timer\" then N = (N or 0) + 1 \
         if atl.event.timer.name == \"twice\" then T2 = (T2 or 0) + 1 end \
         elseif t == \"click_zone\" then Atl_Timer_Create(\"always\", 1) Atl_Timer_Create(\"twice\", 0.5, 2) \
         Atl_Timer_Create(\"always\", 3) end"
            .to_string(),
    );
    assert!(g.click_zone(0, b"make"));
    assert_eq!(g.timer_names(), [b"always".to_vec(), b"twice".to_vec()]);
    for _ in 0..(5 * 50 + 25) {
        g.update(0.02);
    }
    // 5.5 s: "always" (1 s) 5 times, "twice" (0.5 s) 2 times.
    assert_eq!(g.script().global(b"T2"), Value::Num(2.0));
    assert_eq!(g.script().global(b"N"), Value::Num(7.0));
    assert_eq!(g.timer_names(), [b"always".to_vec()]);
    // The second Create of "always" was logged, not made again: 1 s still.
}

#[test]
fn a_repeating_timer_keeps_its_overshoot() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    settle(&mut g, 5);
    probe.borrow_mut().files.insert(
        format!("{DIR}on_click_deck.lua"),
        "local t = atl.event.type \
         if t == \"timer\" then N = (N or 0) + 1 \
         elseif t == \"click_zone\" then Atl_Timer_Create(\"t\", 1) Atl_Timer_Create(\"none\", 5, 0) end"
            .to_string(),
    );
    assert!(g.click_zone(0, b"make"));
    // 0.3 s frames, a 1 s timer: up at 1.2 s (remaining −0.2), restarted
    // at now − remaining = 1.4 s (@ 0x004354bf..0x004354cc, 0x004680c0),
    // up again at 2.4 s; one with 0 repeats goes at the first walk
    // without running (@ 0x00435aef).
    g.update(0.3);
    assert_eq!(g.timer_names(), [b"t".to_vec()]);
    assert_eq!(g.script().global(b"N"), Value::Nil);
    for _ in 0..3 {
        g.update(0.3);
    }
    assert_eq!(g.script().global(b"N"), Value::Num(1.0), "at 1.2 s");
    for _ in 0..3 {
        g.update(0.3);
    }
    assert_eq!(g.script().global(b"N"), Value::Num(1.0), "not at 2.1 s");
    g.update(0.3);
    assert_eq!(g.script().global(b"N"), Value::Num(2.0), "at 2.4 s");
}

#[test]
fn rejected_calls_and_errors() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    settle(&mut g, 5);
    probe.borrow_mut().files.insert(
        format!("{DIR}on_click_deck.lua"),
        "Atl_Sprite_Play(\"coin\", \"z1\", \"spin\") Atl_Music_Play(\"x\") \
         Atl_Camera_LimitYaw(\"wide\") Atl_Sound_Play(\"s\", 3) BEFORE = true \
         assert(false, \"nope\") AFTER = true"
            .to_string(),
    );
    assert!(g.click_zone(1, b""));
    let p = probe.borrow();
    assert_eq!(
        p.rejected,
        [
            "Atl_Sprite_Play: Atl_Sprite_Play: bad play type",
            "Atl_Music_Play: Atl_Music_Play: usage",
            "Atl_Camera_LimitYaw: Atl_Camera_Limit: bad argument",
            "Atl_Sound_Play: Atl_Sound_Play: the volume must be at most 1",
        ]
    );
    assert_eq!(p.errors.len(), 1);
    assert!(p.errors[0].ends_with("nope"), "{:?}", p.errors);
    drop(p);
    assert_eq!(g.script().global(b"BEFORE"), Value::Bool(true));
    assert_eq!(g.script().global(b"AFTER"), Value::Nil);
    // The game goes on.
    assert!(!g.busy());
    assert_eq!(g.state(), 4);
}

#[test]
fn math_random_is_msvcrt_rand_from_seed_1() {
    let (mut g, _p) = game(BASE);
    g.script()
        .exec("A = math.random(25, 50) B = math.random(5, 15) C = math.random()")
        .expect("run");
    // rand() = 41, 18467, 6334: floor(41 % 32767 / 32767 · 26) + 25, ...
    assert_eq!(g.script().global(b"A"), Value::Num(25.0));
    assert_eq!(g.script().global(b"B"), Value::Num(11.0));
    assert_eq!(g.script().global(b"C"), Value::Num(6334.0 / 32767.0));
}

#[test]
fn the_sprite_handlers_check_and_convert() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    settle(&mut g, 5);
    probe.borrow_mut().seen.clear();
    g.script()
        .exec(
            "Atl_Sprite_PlayRebound(\"clock\", \"z1\", \"loop\", 2, 3.9, 5) \
             Atl_Sprite_PlayRebound(\"clock\", \"z1\", \"stop\") \
             Atl_Sprite_PlayRebound(\"clock\", \"z1\", \"stay\", 1) \
             Atl_Sprite_PlayRebound(\"clock\", \"z1\", \"loop\", 1, 2) \
             Atl_Sprite_PlayRebound(\"clock\", \"z1\", \"loop\", 1, 2, {}) \
             F = Atl_Sprite_GetFrame(\"clock\", \"z1\") \
             G = Atl_Sprite_GetFrame(\"clock\") \
             Atl_Sprite_Randomize(\"man\", 10) \
             Atl_Sprite_Randomize(\"man\", 10, 25)",
        )
        .expect("run");
    let p = probe.borrow();
    let rebound = |object: &str, looped, priority, first, last| Call::SpritePlayRebound {
        object: object.as_bytes().to_vec(),
        animation: b"z1".to_vec(),
        looped,
        priority,
        first,
        last,
    };
    assert_eq!(
        p.seen,
        [
            rebound("clock", true, 2, 3, 5),
            rebound("clock", false, 0, 0, 0),
            Call::SpriteGetFrame {
                object: b"clock".to_vec(),
                animation: b"z1".to_vec(),
            },
            Call::SpriteRandomize {
                object: b"man".to_vec(),
                tick1: 10.0,
                tick2: 0.0,
            },
            Call::SpriteRandomize {
                object: b"man".to_vec(),
                tick1: 10.0,
                tick2: 25.0,
            },
        ]
    );
    assert_eq!(
        p.rejected,
        [
            "Atl_Sprite_PlayRebound: Atl_Sprite_PlayRebound: argument 3 is not \"stop\" or \"loop\"",
            "Atl_Sprite_PlayRebound: Atl_Sprite_PlayRebound: usage",
            "Atl_Sprite_PlayRebound: Atl_Sprite_PlayRebound: argument 6 is not a number",
            "Atl_Sprite_GetFrame: Atl_Sprite_GetFrame: usage",
        ]
    );
    drop(p);
    assert_eq!(g.script().global(b"F"), Value::Num(7.0));
    assert_eq!(g.script().global(b"G"), Value::Nil);
}

#[test]
fn terminate_game_ends_the_game_on_the_credits() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    settle(&mut g, 5);
    assert_eq!(g.state(), 4);
    probe.borrow_mut().requests.clear();
    assert!(g.run_string("return function() Atl_TerminateGame() end"));
    settle(&mut g, 3);
    // State 7 (@ 0x0042f000): the autosave deleted, the game ended with
    // the credits asked, then state 3 (the request 3 queued).
    assert_eq!(g.state(), 3);
    assert!(!g.busy());
    assert!(g.timer_names().is_empty());
    let p = probe.borrow();
    assert_eq!(p.requests, [Request::DeleteAutosave, Request::GameEnded]);
}

/// The error policy (MainLoop's catch @ 0x0043d456 → exit(0)): an error in
/// `InitChapter`, in a new game's `atlantis.lua` / `on_new_game.lua` or in
/// the event step's `Atl_Hero_ArrangeHandledObject` ends the program (the
/// quit state 8); one in a scene's `on_click` is logged and the game goes
/// on.
#[test]
fn uncaught_errors_end_the_program() {
    let with = |name: &str, src: &str| -> Vec<(String, String)> {
        BASE.iter()
            .map(|(n, s)| {
                let s = if *n == name { src } else { *s };
                ((*n).to_string(), s.to_string())
            })
            .collect()
    };
    let start = |files: &[(String, String)]| {
        let f: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let (mut g, p) = game(&f);
        g.new_game();
        settle(&mut g, 5);
        (g, p)
    };
    // InitChapter.
    let (g, p) = start(&with(
        "atlantis.lua",
        "require \"lib/util.lua\" function InitChapter(Path, Chapter) error(\"chapter\") end",
    ));
    assert_eq!(p.borrow().errors.len(), 1);
    assert_eq!(g.state(), 8, "InitChapter's error ends the program");
    // on_new_game.lua.
    let (g, p) = start(&with(
        "on_new_game.lua",
        "Atl_InitChapter(2) error(\"new\")",
    ));
    assert_eq!(p.borrow().errors.len(), 1);
    assert_eq!(g.state(), 8, "a new game's error ends the program");
    // Atl_Hero_ArrangeHandledObject.
    let (mut g, p) = start(&with(
        "atlantis.lua",
        "require \"lib/util.lua\" \
         function InitChapter(Path, Chapter) Atl_Address_Go(\"deck\", 1, \"no_fx\") end \
         function Atl_Hero_ArrangeHandledObject() error(\"arrange\") end",
    ));
    assert_eq!(g.state(), 4);
    assert!(p.borrow().errors.is_empty(), "{:?}", p.borrow().errors);
    assert!(g.run_arrange_handled_object());
    assert_eq!(p.borrow().errors.len(), 1);
    assert_eq!(g.state(), 8);
    // The same error from another string: logged, the game goes on.
    let (mut g, p) = start(&with(
        "atlantis.lua",
        "require \"lib/util.lua\" \
         function InitChapter(Path, Chapter) Atl_Address_Go(\"deck\", 1, \"no_fx\") end \
         function F() error(\"other\") end",
    ));
    assert!(g.run_string("return F"));
    assert_eq!(p.borrow().errors.len(), 1);
    assert_eq!(g.state(), 4);
    assert!(!g.busy());
}

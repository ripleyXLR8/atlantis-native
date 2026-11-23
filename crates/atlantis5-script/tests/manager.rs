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
    saves: HashMap<String, Vec<u8>>,
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

    fn write_save(&mut self, name: &str, data: &[u8]) -> bool {
        self.saves.insert(name.to_string(), data.to_vec());
        true
    }

    fn read_save(&mut self, name: &str) -> Option<Vec<u8>> {
        self.saves.get(name).cloned()
    }

    /// One section that tells how many engine calls were made before it.
    fn save_sections(&mut self) -> Vec<atlantis4_data::xml::Element> {
        let mut e = atlantis5_script::save::element("PROBE");
        e.attributes
            .push(("calls".into(), self.seen.len().to_string()));
        vec![e]
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

#[test]
fn address_go_under_pcall_runs_after_the_script() {
    // The handler runs the transition inside the call (@ 0x00405390);
    // under pcall the coroutine cannot stop there, so the transition
    // starts once the script has ended instead of being lost.
    let mut files = BASE.to_vec();
    files.push((
        "on_click_deck.lua",
        "if atl.event.type == \"click_zone\" then \
           pcall(Atl_Address_Go, \"deck\", atl.event.click.zone.id, \"zoom_and_fade\") \
           AFTER_GO = true end",
    ));
    let (mut g, probe) = game(&files);
    g.new_game();
    settle(&mut g, 5);
    probe.borrow_mut().requests.clear();
    assert!(g.click_zone(9, b""));
    assert_eq!(g.script().global(b"AFTER_GO"), Value::Bool(true));
    let mut steps = 0;
    while g.busy() && steps < 100 {
        g.update(0.02);
        steps += 1;
    }
    assert!(steps > 10, "the transition ran over frames: {steps}");
    assert!(probe
        .borrow()
        .requests
        .iter()
        .any(|r| matches!(r, Request::LoadView { view: 9, .. })));
    assert_eq!(g.script().address(), (b"deck".to_vec(), 9));
    assert_eq!(g.script().global(b"ENTERED"), Value::Num(2.0));
    assert!(
        probe.borrow().errors.is_empty(),
        "{:?}",
        probe.borrow().errors
    );
}

#[test]
fn ending_the_game_frees_the_runs_that_wait() {
    let (mut g, probe) = game(BASE);
    probe.borrow_mut().wait = true;
    g.new_game();
    settle(&mut g, 5);
    assert!(g.waiting_for_engine(), "the chapter waits at its film");
    assert!(g.script().task_count() > 0);
    g.end_game();
    assert_eq!(g.script().task_count(), 0);
}

#[test]
fn a_handler_s_set_duration_applies_one_period_late() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    // Quarter seconds keep the clock exact.
    for _ in 0..5 {
        g.update(0.25);
    }
    probe.borrow_mut().files.insert(
        format!("{DIR}on_click_deck.lua"),
        "local t = atl.event.type \
         if t == \"timer\" then N = (N or 0) + 1 Atl_Timer_SetDuration(\"s\", 3) \
         elseif t == \"click_zone\" then Atl_Timer_Create(\"s\", 1) end"
            .to_string(),
    );
    assert!(g.click_zone(0, b"make"));
    // 0.25 s frames. Up at 1 s; the handler sets 3 s. The restart
    // (@ 0x004353d0's second 0x00468060, then 0x004680c0) reads the new
    // duration with the step's elapsed time: remaining = 3 − 1, start =
    // now − 2, so the next end is 1 s later (the old period); the 3 s
    // apply from then on.
    let mut ticks = Vec::new();
    for frame in 1..=40 {
        g.update(0.25);
        if let Value::Num(n) = g.script().global(b"N") {
            if ticks.len() < n as usize {
                ticks.push(frame);
            }
        }
    }
    assert_eq!(ticks, [4, 8, 20, 32]);
}

#[test]
fn the_load_state_ends_the_game_before_the_read() {
    let mut files = BASE.to_vec();
    let atlantis = format!(
        "{} function Atl_Address_GetCurrent() return \"deck\", 1 end",
        files[0].1
    );
    files[0] = ("atlantis.lua", &atlantis);
    let (mut g, probe) = game(&files);
    g.new_game();
    settle(&mut g, 5);
    assert_eq!(g.state(), 4);
    assert!(
        probe.borrow().saves.contains_key("autoslot"),
        "the chapter saved"
    );
    probe.borrow_mut().requests.clear();
    // A request queued after Atl_Load: state 2 ends the game first
    // (@ 0x0042f000 case 2 → 0x0042ffd0, which empties the queue), so it
    // is dropped; the reader then queues play.
    assert!(g.run_string(
        "return function() Atl_Timer_Create(\"gone\", 100) Atl_Load() Atl_TerminateGame() end"
    ));
    settle(&mut g, 5);
    assert_eq!(g.state(), 4);
    assert!(g.timer_names().is_empty(), "{:?}", g.timer_names());
    let p = probe.borrow();
    let reset = p
        .requests
        .iter()
        .position(|r| *r == Request::GameReset)
        .expect("the game ended before the read");
    let view = p
        .requests
        .iter()
        .position(|r| matches!(r, Request::LoadView { .. }))
        .expect("the saved view loaded");
    assert!(reset < view);
    assert!(!p.requests.contains(&Request::GameEnded));
}

#[test]
fn the_chapter_runs_in_the_next_state_and_clears_its_number_after() {
    let mut files = BASE.to_vec();
    files[0] = (
        "atlantis.lua",
        "function InitChapter(Path, Chapter) CHS = (CHS or \"\") .. tostring(Chapter) .. \",\" \
           if Chapter == 2 then Atl_InitChapter(5) end \
           Atl_Video_Play(\"intro\") end",
    );
    let (mut g, probe) = game(&files);
    probe.borrow_mut().wait = true;
    g.new_game();
    settle(&mut g, 2);
    // @ 0x0042f000 case 1: the next request (4) is taken before the call.
    assert!(g.waiting_for_engine());
    assert_eq!(g.state(), 4);
    g.finish_wait();
    probe.borrow_mut().wait = false;
    settle(&mut g, 5);
    // +0x50 = 0 after the call: the nested Atl_InitChapter(5) lost its 5.
    assert_eq!(g.script().global(b"CHS"), Value::Str(b"2,nil,".to_vec()));
    assert_eq!(g.state(), 4);
}

#[test]
fn play_walks_the_timers_before_it_takes_a_request() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    settle(&mut g, 5);
    probe.borrow_mut().files.insert(
        format!("{DIR}on_click_deck.lua"),
        "local t = atl.event.type \
         if t == \"timer\" then N = (N or 0) + 1 \
         elseif t == \"click_zone\" then Atl_Timer_Create(\"due\", 0, 1) Atl_InitChapter(3) end"
            .to_string(),
    );
    assert!(g.click_zone(0, b"go"));
    // @ 0x0042f000 case 4: the timers (@ 0x00435a70), then the request,
    // in the same frame.
    g.update(0.02);
    assert_eq!(g.script().global(b"N"), Value::Num(1.0));
    assert_eq!(g.state(), 1);
}

#[test]
fn atl_save_takes_the_engine_s_sections_at_the_call() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    settle(&mut g, 5);
    probe.borrow_mut().saves.clear();
    let before = probe.borrow().seen.len();
    // @ 0x00408fb0 → 0x0043ad90: the whole file is written inside the
    // handler, so engine calls after Atl_Save are not in it.
    assert!(g.run_string(
        "return function() Atl_Sprite_Play(\"a\", \"z1\", \"loop\") Atl_Save() \
         Atl_Sprite_Play(\"b\", \"z1\", \"loop\") end"
    ));
    let text = String::from_utf8(probe.borrow().saves["autoslot"].clone()).expect("text");
    let calls = format!("calls=\"{}\"", before + 1);
    assert!(text.contains(&calls), "{calls} in {text}");
}

/// The chapter of [`BASE`] with two timers made before its film.
fn timed_chapter() -> Vec<(&'static str, String)> {
    BASE.iter()
        .map(|(n, s)| {
            let s = if *n == "atlantis.lua" {
                "require \"lib/util.lua\" \
                 function InitChapter(Path, Chapter) Atl_Timer_Create(\"lightning\", 25) \
                   Atl_Timer_Create(\"fountains\", 19) Atl_Video_Play(\"intro\") \
                   Atl_Address_Go(\"deck\", 1, \"no_fx\") Atl_Save() end \
                 function Atl_Address_GetCurrent() return \"deck\", 1 end"
                    .to_string()
            } else {
                (*s).to_string()
            };
            (*n, s)
        })
        .collect()
}

fn saved_text(probe: &Rc<RefCell<Probe>>) -> String {
    String::from_utf8(probe.borrow().saves["autoslot"].clone()).expect("text")
}

#[test]
fn a_save_after_a_film_keeps_the_timers_whole() {
    let files = timed_chapter();
    let files: Vec<(&str, &str)> = files.iter().map(|(n, s)| (*n, s.as_str())).collect();
    let (mut g, probe) = game(&files);
    probe.borrow_mut().wait = true;
    g.new_game();
    // 10 s of film: the chapter's handler waits inside Atl_Video_Play,
    // where no walk updates the timers' +0x10.
    settle(&mut g, 500);
    assert!(g.waiting_for_engine());
    g.finish_wait();
    probe.borrow_mut().wait = false;
    settle(&mut g, 3);
    assert_eq!(g.state(), 4);
    // @ 0x00436f20 → 0x00468060 at +0x10: the remaining time of the
    // timers' creation.
    let text = saved_text(&probe);
    assert!(
        text.contains("name=\"lightning\" duration=\"25\" remaining_time=\"25\""),
        "{text}"
    );
    assert!(
        text.contains("name=\"fountains\" duration=\"19\" remaining_time=\"19\""),
        "{text}"
    );
    // Read back: 19 s from the load, not 9.
    assert!(g.run_string("return function() Atl_Load() end"));
    settle(&mut g, 3);
    assert_eq!(g.state(), 4);
    probe.borrow_mut().files.insert(
        format!("{DIR}on_click_deck.lua"),
        "if atl.event.type == \"timer\" then FIRED = atl.event.timer.name end".to_string(),
    );
    for _ in 0..(18 * 50) {
        g.update(0.02);
    }
    assert_eq!(g.script().global(b"FIRED"), Value::Nil, "not before 19 s");
    for _ in 0..(2 * 50) {
        g.update(0.02);
    }
    assert_eq!(
        g.script().global(b"FIRED"),
        Value::Str(b"fountains".to_vec())
    );
}

#[test]
fn a_menu_pauses_the_timers() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    // Quarter seconds keep the clock exact.
    for _ in 0..5 {
        g.update(0.25);
    }
    probe.borrow_mut().files.insert(
        format!("{DIR}on_click_deck.lua"),
        "local t = atl.event.type \
         if t == \"timer\" then N = (N or 0) + 1 \
         elseif t == \"click_zone\" then Atl_Timer_Create(\"t\", 2) end"
            .to_string(),
    );
    assert!(g.click_zone(0, b"make"));
    g.update(0.25);
    // A menu after 0.25 s: @ 0x0041b780 pauses the timers (@ 0x00435b70)
    // when its mode opens and resumes them (@ 0x00435bd0) at its end.
    g.open_menu();
    for _ in 0..40 {
        g.update(0.25);
    }
    assert_eq!(g.script().global(b"N"), Value::Nil);
    // A save from the menu writes the time left at the pause.
    probe.borrow_mut().saves.clear();
    assert!(g.save_now("slot_0"));
    let text = String::from_utf8(probe.borrow().saves["slot_0"].clone()).expect("text");
    assert!(text.contains("remaining_time=\"1.75\""), "{text}");
    g.menu_closed();
    assert_eq!(g.state(), 4);
    let mut frames = 0;
    while g.script().global(b"N") == Value::Nil {
        g.update(0.25);
        frames += 1;
        assert!(frames < 20);
    }
    assert_eq!(frames, 7, "1.75 s after the menu");
    // A timer made while the menus are up starts paused (@ 0x00435770).
    g.open_menu();
    assert!(g.run_string("return function() Atl_Timer_Create(\"m\", 1, 1) end"));
    for _ in 0..20 {
        g.update(0.25);
    }
    g.menu_closed();
    // One walk (+0x10 = now) after the menu: 0.25 s run.
    g.update(0.25);
    probe.borrow_mut().saves.clear();
    assert!(g.save_now("slot_1"));
    let text = String::from_utf8(probe.borrow().saves["slot_1"].clone()).expect("text");
    assert!(
        text.contains("name=\"m\" duration=\"1\" remaining_time=\"0.75\""),
        "{text}"
    );
}

#[test]
fn a_save_without_timers_has_no_timer_section() {
    let (mut g, probe) = game(BASE);
    g.new_game();
    settle(&mut g, 5);
    assert!(g.timer_names().is_empty());
    // @ 0x00436f20: CALLBACK_SHOOTER_MANAGER only with a timer.
    let text = saved_text(&probe);
    assert!(!text.contains("CALLBACK_SHOOTER_MANAGER"), "{text}");
    assert!(g.run_string("return function() Atl_Timer_Create(\"t\", 5) Atl_Save() end"));
    let text = saved_text(&probe);
    assert!(text.contains("<CALLBACK_SHOOTER_MANAGER>"), "{text}");
}

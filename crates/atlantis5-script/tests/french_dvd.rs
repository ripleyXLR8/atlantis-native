//! The French DVD's own scripts (`ATLANTIS5_DVD_FR`: the folder holding
//! `datas/`, the two cabinets extracted; skipped when unset): the 66 Lua
//! 5.0 chunks load, and a new game's calls are those the RE notes' oracle
//! (the international sources under the Lua 5.0.3 interpreter, the C side
//! modelled from the program) records.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis5_script::record::Recorder;
use atlantis5_script::{Game, Script};
use atlantis_data::FileSource;

fn dvd() -> Option<PathBuf> {
    match std::env::var_os("ATLANTIS5_DVD_FR") {
        Some(d) => Some(PathBuf::from(d)),
        None => {
            eprintln!("SKIPPED: ATLANTIS5_DVD_FR is not set");
            None
        }
    }
}

fn source(d: &std::path::Path) -> Rc<Atlantis5Source> {
    Rc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"))
}

fn recorder(src: &Rc<Atlantis5Source>) -> Rc<RefCell<Recorder>> {
    let s = src.clone();
    Rc::new(RefCell::new(Recorder::new(Box::new(move |p: &[u8]| {
        s.read(&String::from_utf8_lossy(p))
    }))))
}

#[test]
fn every_chunk_loads() {
    let Some(d) = dvd() else { return };
    let root = d.join("datas/Datas/Scripts");
    let mut n = 0;
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).expect("dir") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lua")) {
                let bytes = std::fs::read(&p).expect("read");
                assert!(bytes.starts_with(b"\x1bLua\x50"), "{p:?}: not a 5.0 chunk");
                let src = source(&d);
                let rec = recorder(&src);
                let mut s = Script::new(rec).expect("state");
                let name = format!("@{}", p.display());
                use atlantis5_script::rilua::LuaApiMut;
                s.lua()
                    .load_bytes(&bytes, &name)
                    .unwrap_or_else(|e| panic!("{p:?}: {e}"));
                n += 1;
            }
        }
    }
    assert_eq!(n, 66);
}

/// The calls of a new game up to the first view, as the oracle prints them
/// (`oracle.lua <scripts>/ new`).
const NEW_GAME: [&str; 21] = [
    "C Atl_InitChapter(1)",
    "C Atl_PersistantSprite_StopAll()",
    "C Atl_Timer_ClearAll()",
    "C Atl_Music_StopAll(0)",
    "C Atl_Camera_ActivateFx(\"NoFx\")",
    "C Atl_Video_Play(\"cine01\")",
    "C Atl_Camera_LookAt(0, 270)",
    "C Atl_Address_Go(\"zeppelin\", 3, \"no_fx\")",
    "C Atl_Sprite_Play(\"crab_breaker\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"cendar\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"glass_breaker\", \"z1\", \"stay\", 1)",
    "C Atl_Sprite_Play(\"glass_breaker_door\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"coin\", \"z1\", \"loop\")",
    "C Atl_Camera_LimitPitch(0, 120)",
    "C Atl_Camera_SetFOV(76)",
    "C Atl_Music_Play(\"zeppelin_background\", 1)",
    "C Atl_Music_Play(\"zeppelin_cracking\", 2)",
    "C Atl_Timer_Create(\"SND_Radio_Sparkle\", 30)",
    "C Atl_Save()",
    "",
    "",
];

#[test]
fn a_new_game_calls_what_the_oracle_records() {
    let Some(d) = dvd() else { return };
    let src = source(&d);
    for wait in [false, true] {
        let rec = recorder(&src);
        rec.borrow_mut().wait = wait;
        let mut game = Game::new(rec.clone()).expect("game");
        game.new_game();
        for _ in 0..10 {
            game.update(0.02);
            if game.waiting_for_engine() {
                // The film or the look ends.
                game.finish_wait();
            }
        }
        let r = rec.borrow();
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let calls = r.calls();
        let expected: Vec<&str> = NEW_GAME.iter().copied().filter(|l| !l.is_empty()).collect();
        assert_eq!(calls, expected, "wait = {wait}");
        assert_eq!(game.state(), 4, "play");
        assert!(r.lines.iter().any(|l| l == "R LoadView(zeppelin, 3)"));
        drop(r);
        assert_eq!(game.script().address(), (b"zeppelin".to_vec(), 3));
        assert_eq!(game.timer_names(), [b"SND_Radio_Sparkle".to_vec()]);
    }
}

/// A walk through the zeppelin's first views (the move zones of the WAM:
/// id = target view, no name) and two timers' ends, as the oracle records
/// it (`oracle.lua <scripts>/ new zone:2: zone:1: zone:2: zone:3: zone:4:
/// timer:SND_Radio_Sparkle timer:SND_zep_crack`), after the new game's
/// calls. `math.random` is MSVCRT's `rand` from seed 1 on both sides.
const WALK: &[&str] = &[
    "C Atl_Address_Go(\"zeppelin\", 2)",
    "C Atl_Sprite_Play(\"glass_breaker\", \"z1\", \"stay\", 1)",
    "C Atl_Sprite_Play(\"glass_breaker_door\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"coin\", \"z1\", \"loop\")",
    "C Atl_Sprite_Play(\"wallet\", \"z1\", \"loop\")",
    "C Atl_Camera_LimitPitch(0, 120)",
    "C Atl_Camera_SetFOV(76)",
    "C Atl_Address_Go(\"zeppelin\", 1)",
    "C Atl_Camera_LimitPitch(0, 120)",
    "C Atl_Camera_SetFOV(76)",
    "C Atl_Camera_LookAt(-8, -3)",
    "C Atl_Address_Go(\"zeppelin\", 2)",
    "C Atl_Sprite_Play(\"glass_breaker\", \"z1\", \"stay\", 1)",
    "C Atl_Sprite_Play(\"glass_breaker_door\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"coin\", \"z1\", \"loop\")",
    "C Atl_Sprite_Play(\"wallet\", \"z1\", \"loop\")",
    "C Atl_Camera_LimitPitch(0, 120)",
    "C Atl_Camera_SetFOV(76)",
    "C Atl_Address_Go(\"zeppelin\", 3)",
    "C Atl_Sprite_Play(\"crab_breaker\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"cendar\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"glass_breaker\", \"z1\", \"stay\", 1)",
    "C Atl_Sprite_Play(\"glass_breaker_door\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"coin\", \"z1\", \"loop\")",
    "C Atl_Camera_LimitPitch(0, 120)",
    "C Atl_Camera_SetFOV(76)",
    "C Atl_Address_Go(\"zeppelin\", 4)",
    "C Atl_Sprite_Play(\"crab_breaker\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"glass_breaker\", \"z1\", \"stay\", 1)",
    "C Atl_Sprite_Play(\"glass_breaker_door\", \"z1\", \"stay\")",
    "C Atl_Sprite_Play(\"coin\", \"z1\", \"loop\")",
    "C Atl_Camera_LimitPitch(0, 120)",
    "C Atl_Camera_SetFOV(76)",
    "C Atl_Music_Play(\"zeppelin_cracking\", 2)",
    "C Atl_Timer_SetDuration(\"SND_zep_crack\", 25)",
];

#[test]
fn a_walk_calls_what_the_oracle_records() {
    let Some(d) = dvd() else { return };
    let src = source(&d);
    let rec = recorder(&src);
    let mut game = Game::new(rec.clone()).expect("game");
    game.new_game();
    let settle = |game: &mut Game| {
        for _ in 0..10 {
            game.update(0.02);
        }
    };
    settle(&mut game);
    let start = rec.borrow().calls().len();
    for view in [2, 1, 2, 3, 4] {
        assert!(game.click_zone(view, b""), "click to {view}");
        settle(&mut game);
        assert_eq!(game.script().address().1, view);
    }
    for t in [&b"SND_Radio_Sparkle"[..], b"SND_zep_crack"] {
        assert!(game.timer_event(t));
        settle(&mut game);
    }
    let r = rec.borrow();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(&r.calls()[start..], WALK);
}

/// A timer ends on the game clock, in state 4, once per period.
#[test]
fn the_radio_timer_ends_after_its_30_seconds() {
    let Some(d) = dvd() else { return };
    let src = source(&d);
    let rec = recorder(&src);
    let mut game = Game::new(rec.clone()).expect("game");
    game.new_game();
    for _ in 0..10 {
        game.update(0.02);
    }
    let count = |rec: &std::cell::RefCell<Recorder>| {
        rec.borrow()
            .lines
            .iter()
            .filter(|l| l.contains("on_click_zeppelin"))
            .count()
    };
    assert_eq!(count(&rec), 0);
    // 29.5 s: not yet; 30.5 s: once.
    for _ in 0..1475 {
        game.update(0.02);
    }
    assert_eq!(count(&rec), 0);
    for _ in 0..50 {
        game.update(0.02);
    }
    assert_eq!(count(&rec), 1);
}

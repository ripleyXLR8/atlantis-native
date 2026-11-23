//! Tables walk in Lua 5.0.3's order (rilua's Native patch 8), the order
//! Atlantis V's save writes its LUA_DB in. `ATLANTIS5_DVD_FR` (the folder
//! holding `datas/`, the two cabinets extracted; skipped when unset): the
//! `pairs` walk of `atl` after a new game (and after events) has the MD5
//! of the same walk under PUC-Rio Lua 5.0.3 running the RE notes' oracle
//! (atlantis5-re tools/lua/oracle.lua on the international sources). Only
//! the MD5s are kept here, not the game's keys.
//!
//! The walk, depth first, keys in `next` order, a table seen once, one
//! line per key ("K atl.flags.x", " #" after a number key):
//!
//! ```lua
//! local seen = {}
//! local function walk(t, path)
//!   if seen[t] then return end
//!   seen[t] = true
//!   for k, v in pairs(t) do
//!     local p = path .. "." .. tostring(k)
//!     print("K " .. p .. (type(k) == "number" and " #" or ""))
//!     if type(v) == "table" then walk(v, p) end
//!   end
//! end
//! walk(atl, "atl")
//! ```
//!
//! `A5_TABLE_WALK_DUMP=<file>` writes the walk's text there, to diff it
//! with the oracle's.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis5_script::record::Recorder;
use atlantis5_script::{Game, Value};
use atlantis_data::FileSource;
use md5::{Digest, Md5};

fn dvd() -> Option<PathBuf> {
    match std::env::var_os("ATLANTIS5_DVD_FR") {
        Some(d) => Some(PathBuf::from(d)),
        None => {
            eprintln!("SKIPPED: ATLANTIS5_DVD_FR is not set");
            None
        }
    }
}

fn new_game(d: &std::path::Path) -> (Game, Rc<RefCell<Recorder>>) {
    let src = Rc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let s = src.clone();
    let rec = Rc::new(RefCell::new(Recorder::new(Box::new(move |p: &[u8]| {
        s.read(&String::from_utf8_lossy(p))
    }))));
    let mut game = Game::new(rec.clone()).expect("game");
    game.new_game();
    settle(&mut game);
    (game, rec)
}

fn settle(game: &mut Game) {
    for _ in 0..10 {
        game.update(0.02);
        if game.waiting_for_engine() {
            game.finish_wait();
        }
    }
}

/// The walk's text, then its MD5 in hex.
fn walk_md5(game: &mut Game) -> String {
    let script = game.script();
    script
        .exec(
            r#"
            local out = {}
            local seen = {}
            local function walk(t, path)
              if seen[t] then return end
              seen[t] = true
              for k, v in pairs(t) do
                local p = path .. "." .. tostring(k)
                out[#out + 1] = "K " .. p .. (type(k) == "number" and " #" or "") .. "\n"
                if type(v) == "table" then walk(v, p) end
              end
            end
            walk(atl, "atl")
            -- (no table library in the game's state)
            local function cat(i, j)
              if i > j then return "" end
              if i == j then return out[i] end
              local m = math.floor((i + j) / 2)
              return cat(i, m) .. cat(m + 1, j)
            end
            native_table_walk = cat(1, #out)
            "#,
        )
        .expect("the walk runs");
    let Value::Str(text) = script.global(b"native_table_walk") else {
        panic!("no walk text");
    };
    // A local copy of the walk to compare with the oracle's (not kept).
    if let Some(path) = std::env::var_os("A5_TABLE_WALK_DUMP") {
        std::fs::write(path, &text).expect("dump the walk");
    }
    Md5::digest(&text)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `oracle.lua <scripts>/ new`.
#[test]
fn a_new_game_walks_atl_in_lua_5_0_3_order() {
    let Some(d) = dvd() else { return };
    let (mut game, rec) = new_game(&d);
    assert!(rec.borrow().errors.is_empty(), "{:?}", rec.borrow().errors);
    assert_eq!(walk_md5(&mut game), "579db48b6c289b5d0ef06ad1fd78de70");
}

/// `oracle.lua <scripts>/ new zone:2: zone:1: zone:2: zone:3: zone:4:
/// timer:SND_Radio_Sparkle timer:SND_zep_crack` (french_dvd.rs's walk).
#[test]
fn a_walk_through_the_zeppelin_keeps_lua_5_0_3_order() {
    let Some(d) = dvd() else { return };
    let (mut game, rec) = new_game(&d);
    for view in [2, 1, 2, 3, 4] {
        assert!(game.click_zone(view, b""), "click to {view}");
        settle(&mut game);
    }
    for t in [&b"SND_Radio_Sparkle"[..], b"SND_zep_crack"] {
        assert!(game.timer_event(t));
        settle(&mut game);
    }
    assert!(rec.borrow().errors.is_empty(), "{:?}", rec.borrow().errors);
    assert_eq!(walk_md5(&mut game), "fc9a2b381985206228f2a9be4d2b02e8");
}

/// `oracle.lua <scripts>/ new zone:3:info zone:0:glass_breaker_door
/// sprite:cendar:z1 zone:0:glass_breaker_door sprite:glass_breaker:z1
/// zone:0:tank_room_in timer:SND_zep_crack`: zone, sprite and timer
/// events, items taken.
#[test]
fn clicks_and_a_timer_keep_lua_5_0_3_order() {
    let Some(d) = dvd() else { return };
    let (mut game, rec) = new_game(&d);
    assert!(game.click_zone(3, b"info"));
    settle(&mut game);
    assert!(game.click_zone(0, b"glass_breaker_door"));
    settle(&mut game);
    assert!(game.click_sprite(b"cendar", b"z1"));
    settle(&mut game);
    assert!(game.click_zone(0, b"glass_breaker_door"));
    settle(&mut game);
    assert!(game.click_sprite(b"glass_breaker", b"z1"));
    settle(&mut game);
    assert!(game.click_zone(0, b"tank_room_in"));
    settle(&mut game);
    assert!(game.timer_event(b"SND_zep_crack"));
    settle(&mut game);
    assert!(rec.borrow().errors.is_empty(), "{:?}", rec.borrow().errors);
    assert_eq!(walk_md5(&mut game), "8aa10a8ff54d1b0d9e3d756269b7c12c");
}

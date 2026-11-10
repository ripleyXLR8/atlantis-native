//! Table iteration order against PUC-Rio Lua 5.1.4.
//!
//! The expected strings were printed by the reference `lua` 5.1.4 (built
//! from the official source) running the same Lua code. Each test is the
//! reproducer of one of the fixes made to the vendored rilua.

use atlantis4_script::rilua::{Lua, LuaApiMut};

/// `next` order of a table, as a space-separated list of keys.
const KEYS: &str = "local function keys(t) local o = {} local k = next(t) \
    while k ~= nil do o[#o+1] = tostring(k) k = next(t, k) end return table.concat(o, ' ') end ";

fn keys_of(lua: &mut Lua, expr: &str) -> String {
    let src = format!("{KEYS} local function child() return {{}} end result = keys({expr})");
    lua.exec(&src).expect("run");
    lua.global::<String>("result").expect("result")
}

/// Fix 1: a constructor whose last item is a call ends with `SETLIST`
/// growing the array; PUC-Rio's `luaH_resizearray` rebuilds the hash part.
#[test]
fn setlist_growth_rebuilds_the_hash_part() {
    let mut lua = atlantis4_script::new_state().expect("state");
    assert_eq!(
        keys_of(&mut lua, "{ name = 1, sizeType = 2, child() }"),
        "1 sizeType name"
    );
    assert_eq!(
        keys_of(&mut lua, "{ name = 1, sizeType = 2, size = 3, ratio = 4, anchor = 5, position = 6, visible = 7, child() }"),
        "1 visible name position anchor ratio sizeType size"
    );
}

/// Fix 2: PUC-Rio's `newkey` takes a main position whose value is nil (a
/// key set to nil earlier) for the new key.
#[test]
fn a_nil_valued_main_position_is_reused() {
    let mut lua = atlantis4_script::new_state().expect("state");
    let got = keys_of(&mut lua, "(function() local t = {} for i = 1, 8 do t['g' .. i] = true end t.g5 = nil t.g9 = 1 return t end)()");
    assert_eq!(got, "g1 g7 g3 g2 g4 g6 g9 g8");
}

/// Fix 3: the globals table is made with `luaH_new(L, 0, 2)` and the
/// libraries are opened in `linit.c`'s order, each with its functions in
/// its own registration order.
#[test]
fn fresh_globals_iterate_as_lua_5_1_4() {
    let mut lua = atlantis4_script::new_state().expect("state");
    lua.exec("local o = {} for k in pairs(_G) do o[#o+1] = k end result = table.concat(o, ' ')")
        .expect("run");
    let got = lua.global::<String>("result").expect("result");
    // `result` itself is the last global set; it is not in the walk above.
    assert_eq!(
        got,
        "string xpcall package tostring print os unpack require getfenv setmetatable next assert \
         tonumber io rawequal collectgarbage getmetatable module rawset math debug pcall table \
         newproxy type coroutine _G select gcinfo pairs rawget loadstring ipairs _VERSION dofile \
         setfenv load error loadfile"
    );
}

/// With `package` opened second, the libraries opened after it still reach
/// `package.loaded` (PUC-Rio's `luaL_register` stores each in `_LOADED`).
#[test]
fn package_loaded_has_every_library() {
    let mut lua = atlantis4_script::new_state().expect("state");
    lua.exec(
        "ok = package.loaded.string == string and package.loaded.table == table \
         and package.loaded.math == math and package.loaded.io == io and package.loaded.os == os \
         and package.loaded.debug == debug and package.loaded.coroutine == coroutine \
         and package.loaded.package == package",
    )
    .expect("run");
    assert!(lua.global::<bool>("ok").expect("ok"));
}

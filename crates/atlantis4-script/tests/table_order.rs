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
    assert_eq!(keys_of(&mut lua, "{ name = 1, sizeType = 2, child() }"), "1 sizeType name");
    assert_eq!(
        keys_of(&mut lua, "{ name = 1, sizeType = 2, size = 3, ratio = 4, anchor = 5, position = 6, visible = 7, child() }"),
        "1 visible name position anchor ratio sizeType size"
    );
}

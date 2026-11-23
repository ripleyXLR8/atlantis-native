//! Lua 5.0 chunks on rilua (its Native patch 4) against the reference Lua
//! 5.0.3 interpreter: `lua50/semantics.luac` is `semantics.lua` compiled
//! by Lua 5.0.3's `luac` (64-bit: 8-byte size_t and instructions, which
//! the loader reads too), `semantics.expected` what Lua 5.0.3's `lua`
//! prints running it.

use atlantis5_script::rilua::{Lua, LuaApiMut};

const CHUNK: &[u8] = include_bytes!("lua50/semantics.luac");
const EXPECTED: &str = include_str!("lua50/semantics.expected");

/// 5.1's error messages quote names as 'x'; 5.0's as `x'.
fn quotes_50(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(" '") {
        let (a, b) = rest.split_at(i);
        out.push_str(a);
        out.push_str(" `");
        rest = &b[2..];
    }
    out.push_str(rest);
    out
}

#[test]
fn a_50_chunk_runs_as_lua_503_runs_it() {
    let mut lua = Lua::new().expect("state");
    // print collects its line (5.1 code of this test, not the chunk's).
    lua.exec(
        "OUT = {} function print(...) local p = {} for i = 1, select('#', ...) do \
         p[i] = tostring((select(i, ...))) end OUT[#OUT + 1] = table.concat(p, '\\t') end",
    )
    .expect("print");
    let f = lua
        .load_bytes(CHUNK, "=semantics.lua")
        .expect("the 5.0 chunk loads");
    lua.call_function(&f, &[]).expect("runs");
    lua.exec("RESULT = table.concat(OUT, '\\n')").expect("join");
    let got: String = lua.global("RESULT").expect("result");
    let got = quotes_50(&got);
    let expected = EXPECTED.trim_end();
    for (k, (g, e)) in got.lines().zip(expected.lines()).enumerate() {
        assert_eq!(g, e, "line {}", k + 1);
    }
    assert_eq!(got.lines().count(), expected.lines().count());
}

#[test]
fn other_chunks_are_refused() {
    let mut lua = Lua::new().expect("state");
    assert!(lua.load_bytes(b"\x1bLua\x50\x00", "=x").is_err());
    let mut cut = CHUNK.to_vec();
    cut.truncate(CHUNK.len() / 2);
    assert!(lua.load_bytes(&cut, "=x").is_err());
}

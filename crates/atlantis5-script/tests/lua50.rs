//! Lua 5.0 chunks on rilua (its Native patch 4) against the reference Lua
//! 5.0.3 interpreter: `lua50/semantics.luac` is `semantics.lua` compiled
//! by Lua 5.0.3's `luac` (64-bit: 8-byte size_t and instructions, which
//! the loader reads too), `semantics.expected` what Lua 5.0.3's `lua`
//! prints running it.

use atlantis5_script::rilua::{Lua, LuaApiMut};

const CHUNK: &[u8] = include_bytes!("lua50/semantics.luac");
const SOURCE: &[u8] = include_bytes!("lua50/semantics.lua");
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

/// What `semantics.lua` prints, run as `code` (the chunk or the source)
/// in a state set by `setup`.
fn run(code: &[u8], setup: impl FnOnce(&mut Lua)) -> Result<String, String> {
    let mut lua = Lua::new().expect("state");
    setup(&mut lua);
    // print collects its line (5.1 code of this test, not the chunk's).
    lua.exec(
        "OUT = {} function print(...) local p = {} for i = 1, select('#', ...) do \
         p[i] = tostring((select(i, ...))) end OUT[#OUT + 1] = table.concat(p, '\\t') end",
    )
    .expect("print");
    let f = lua
        .load_bytes(code, "=semantics.lua")
        .map_err(|e| e.to_string())?;
    lua.call_function(&f, &[]).map_err(|e| e.to_string())?;
    lua.exec("RESULT = table.concat(OUT, '\\n')").expect("join");
    let got: String = lua.global("RESULT").expect("result");
    Ok(quotes_50(&got))
}

fn same_as_lua_503(got: &str) {
    let expected = EXPECTED.trim_end();
    for (k, (g, e)) in got.lines().zip(expected.lines()).enumerate() {
        assert_eq!(g, e, "line {}", k + 1);
    }
    assert_eq!(got.lines().count(), expected.lines().count());
}

#[test]
fn a_50_chunk_runs_as_lua_503_runs_it() {
    same_as_lua_503(&run(CHUNK, |_| {}).expect("the 5.0 chunk runs"));
}

/// The same program as Lua 5.0 source (the international DVD ships its
/// scripts as source) in a state whose source is 5.0's (rilua Native patch
/// 11, as `Script::new` sets it, with 5.0.3's table layout): Lua 5.0.3's
/// output, its `for i, m in t` over a table included. Compiled as 5.1
/// source, that loop is an error.
#[test]
fn a_50_source_runs_as_lua_503_runs_it() {
    let got = run(SOURCE, |lua| {
        let st = lua.state_mut();
        st.gc.compile_lua50 = true;
        st.gc.table_lua50 = true;
        st.gc.table_max_bits = 24;
    })
    .expect("the 5.0 source runs");
    same_as_lua_503(&got);
    let as_51 = run(SOURCE, |_| {});
    assert!(
        as_51
            .as_ref()
            .is_err_and(|e| e.contains("attempt to call a table value")),
        "{as_51:?}"
    );
}

#[test]
fn other_chunks_are_refused() {
    let mut lua = Lua::new().expect("state");
    assert!(lua.load_bytes(b"\x1bLua\x50\x00", "=x").is_err());
    let mut cut = CHUNK.to_vec();
    cut.truncate(CHUNK.len() / 2);
    assert!(lua.load_bytes(&cut, "=x").is_err());
}

/// A local of a loop's body captured by a closure, in a state whose
/// source is 5.0's: each iteration's closure keeps its own value (Lua
/// 5.0.3's `forbody` compiles the body as a `block`, whose `CLOSE` comes
/// before `FORLOOP`/`TFORLOOP`; its `lua` prints `2 4 6` and `10 20 30`).
#[test]
fn closures_over_a_body_local_see_each_iteration() {
    let source = b"local fs = {}\n\
        for i = 1, 3 do local x = i * 2; fs[i] = function() return x end end\n\
        print(fs[1](), fs[2](), fs[3]())\n\
        local gs = {}\n\
        for k, v in {10, 20, 30} do local y = v; gs[k] = function() return y end end\n\
        print(gs[1](), gs[2](), gs[3]())\n";
    let got = run(source, |lua| {
        let st = lua.state_mut();
        st.gc.compile_lua50 = true;
        st.gc.table_lua50 = true;
        st.gc.table_max_bits = 24;
    })
    .expect("the 5.0 source runs");
    assert_eq!(got, "2\t4\t6\n10\t20\t30");
}

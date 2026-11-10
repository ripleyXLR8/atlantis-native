//! Atlantis Evolution's scripts.
//!
//! The original links Lua 5.1.4 with tolua++ bindings
//! (atlantis4-gog-en-2363910). This crate runs the 504 game scripts on
//! rilua, a pure-Rust Lua 5.1 vendored in `vendor/rilua` with fixes that
//! make its tables iterate in PUC-Rio 5.1.4's order (the engine walks Lua
//! tables in `next` order for layouts and for the save).

pub use rilua;

/// A fresh Lua 5.1 state with the standard libraries, as `luaL_newstate`
/// followed by `luaL_openlibs`.
pub fn new_state() -> rilua::LuaResult<rilua::Lua> {
    rilua::Lua::new()
}

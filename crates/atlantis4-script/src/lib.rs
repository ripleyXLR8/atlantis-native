//! Atlantis Evolution's scripts.
//!
//! The original (atlantis4-gog-en-2363910) links Lua 5.1.4 with tolua++
//! bindings. This crate runs the 504 game scripts on rilua, a pure-Rust
//! Lua 5.1 vendored in `vendor/rilua` with fixes that make its tables
//! iterate in PUC-Rio 5.1.4's order (the engine walks Lua tables in `next`
//! order for layouts and for the save). It provides:
//!
//! - the game state ([`Script`]) with tolua++'s module and the 54
//!   bindings, their argument checks, conversions and error messages as
//!   the executable has them ([`tolua`]); calls go to an [`Engine`];
//! - `TeLuaGUI` layout states ([`GuiState`]);
//! - the boot ([`read_boot_vars`], [`new_game`]) and the node change
//!   ([`Script::change_warp`]) in the original's order;
//! - the save's globals walk ([`Script::globals`]);
//! - a recording stub engine that writes call traces ([`record`]).
//!
//! Error policy: a Lua error in a callback the engine
//! calls ends the original (`Lua: <message>`, exit code 1); here it is
//! reported and the game goes on, unless [`ErrorMode::Strict`]. Errors
//! while a file runs are dropped, as in the original.

pub mod engine;
pub mod record;
mod runtime;
pub mod tolua;
pub mod trace;

pub use engine::{
    Arg, BootValue, Call, Engine, ErrorSite, Event, GuiKey, GuiValue, Reply, SavedGlobal,
    SavedValue, Value,
};
pub use rilua;
pub use runtime::{
    new_game, normalise_path, read_boot_vars, with_extension, BootVars, Collector, EngineRef,
    ErrorMode, GuiState, Options, Script, ScriptError, BOOT_FILES, GUI_CLASSES, RESET_CALLBACKS,
};

/// A fresh Lua 5.1 state with the standard libraries, as `luaL_newstate`
/// followed by `luaL_openlibs`.
pub fn new_state() -> rilua::LuaResult<rilua::Lua> {
    rilua::Lua::new()
}

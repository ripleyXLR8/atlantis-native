//! *Atlantis V*'s scripts.
//!
//! The French DVD ships its 66 scripts as Lua 5.0 chunks; the program
//! (atlantis5-dvd-int-3891200 statically: the French one is encrypted)
//! runs them on `lua50.dll` (Lua 5.0.2). This crate runs the player's own
//! chunks on rilua, the pure-Rust Lua vendored in `vendor/rilua`, whose
//! loader reads 5.0 chunks and runs them with 5.0's meaning (its patch 4).
//! It provides:
//!
//! - the Lua state as the program makes it ([`Script`]): the libraries it
//!   opens, `LUA_PATH`, 5.0's `require` and `dofile`, the `atl` database,
//!   the 39 C functions with their handlers' argument checks; a call goes
//!   to an [`Engine`] as a [`Call`];
//! - the game manager ([`Game`]): its states and requests, script runs
//!   as coroutines that wait at the calls the original runs over frames,
//!   the events (`atl.event`, `on_click_<scene>.lua`,
//!   `on_enter_view_<scene>.lua`), the named timers and `Atl_Address_Go`'s
//!   transition;
//! - a recording engine for traces ([`record`]).
//!
//! Error policy: a handler that rejects its arguments logs a line and
//! returns nothing (the script goes on), as the original does. A Lua
//! error makes the original throw, which ends the game; Native reports
//! it ([`Event::Error`]) and ends that script run.

pub mod engine;
pub mod game;
pub mod record;
pub mod script;

pub use engine::{
    number_text, Binding, Call, CameraFx, Engine, ErrorSite, Event, Fx, Limit, PlayType, Reply,
    Request, Semantic, Value, BINDINGS,
};
pub use game::Game;
pub use rilua;
pub use script::{sound_path, EngineRef, Outcome, Pending, Script, Task, SCRIPTS_DIR};

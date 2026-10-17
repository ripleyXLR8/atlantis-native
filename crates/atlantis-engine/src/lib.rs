//! The engine of *Atlantis: The Lost Tales* (Cryo, 1997).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (GPLv3, the ScummVM
//! team). Comments cite addresses in the original `atlantis.exe` so that
//! each behaviour stays traceable to the original code.

#![forbid(unsafe_code)]

pub mod ambient;
pub mod audio;
pub mod con;
pub mod cursor;
pub mod dialog;
pub mod engine;
pub mod enhancements;
pub mod hooks;
pub mod look;
pub mod music;
mod navigation;
pub mod place;
pub mod scripts;
pub mod subtitles;
pub mod sys;
pub mod turn;
pub mod video_skip;
pub mod warp;

pub use dialog::Dialogs;
pub use engine::{run, Abort, Engine, Event, Options};
pub use enhancements::Enhancements;
pub use hooks::{Departure, DirectLoader, Loader, NoScripts, ScriptHooks};
pub use scripts::{DialogPresenter, NoDialog, Scripts};
pub use warp::Warp;

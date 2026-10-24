//! The engine of *Atlantis II* (Cryo, 1999).
//!
//! Comments cite addresses in the original `atlantis2.exe` (the French CD
//! build of 1999, 2199552 bytes) so that each behaviour stays traceable to
//! the original code. It plays Tibet, Ireland, China and Yucatán
//! (without its Maya counting grids, and without Xibalba): the cube drawn as the original draws it with its place
//! animations, turned with the mouse within the original's limits, the
//! cursor at the centre, and the moves between views and places (turn,
//! zoom, transition video, arrival). A dialog ([`dialog`])
//! plays over the view as the original's: its lines and voices ([`sound`]),
//! the speaker's body video ([`hnm_stream`]) with the 3D head ([`head`],
//! [`raster3d`]) and its lip-sync ([`lipsync`]), the subtitles
//! ([`subtitle`]), the reverse shots and the music ducked.
//! The CON interpreter ([`script`]) runs the chapter scripts as the
//! original does, its effects outside the script tables going through
//! [`script::Host`], which the engine implements. The menus ([`menu`])
//! are the original's pages and state machine, with the saves of the
//! original's format (`atlantis2_data::save`) behind them. The bag bar,
//! the object in hand and the overlays some objects open are in [`bag`].

#![forbid(unsafe_code)]

pub mod anim;
pub mod bag;
pub mod blackbird;
pub mod camera;
pub mod credits;
pub mod crt;
pub mod dialog;
pub mod display;
pub mod engine;
pub mod enhancements;
pub mod fpu;
pub mod hd;
pub mod head;
pub mod hnm_stream;
pub mod lipsync;
pub mod look;
pub mod menu;
pub mod music;
pub mod options_menu;
pub mod place_sound;
pub mod png;
pub mod raster3d;
pub mod script;
pub mod settings;
pub mod sound;
pub mod subjects;
pub mod subtitle;
pub mod turn;
pub mod warp;

pub use camera::Camera;
pub use engine::{CentreDraw, CursorBank, Engine, EngineError, Event, LoopType, ScanVars};
pub use enhancements::Enhancements;
pub use fpu::Fpu;
pub use script::Script;
pub use warp::{Renderer, Variant};

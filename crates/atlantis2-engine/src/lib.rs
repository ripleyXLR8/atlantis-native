//! The engine of *Atlantis II* (Cryo, 1999).
//!
//! Comments cite addresses in the original `atlantis2.exe` (the French CD
//! build of 1999, 2199552 bytes) so that each behaviour stays traceable to
//! the original code. For now it plays Tibet's panoramas: the cube drawn as
//! the original draws it with its place animations, turned with the mouse
//! within the original's limits, the cursor at the centre, and the moves
//! between views (turn, transition video, arrival).

#![forbid(unsafe_code)]

pub mod anim;
pub mod camera;
pub mod engine;
pub mod fpu;
pub mod look;
pub mod png;
pub mod turn;
pub mod warp;

pub use camera::Camera;
pub use engine::{Engine, EngineError, Event, LoopType};
pub use fpu::Fpu;
pub use warp::{Renderer, Variant};

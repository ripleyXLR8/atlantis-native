//! The engine of *Atlantis II* (Cryo, 1999).
//!
//! Comments cite addresses in the original `atlantis2.exe` (the French CD
//! build of 1999, 2199552 bytes) so that each behaviour stays traceable to
//! the original code. For now it shows a view: the cube panorama drawn as
//! the original draws it, turned with the mouse within the original's
//! limits; a new game opens on its first view. A dialog ([`dialog`]) plays
//! over the view as the original's: its lines and voices ([`sound`]), the
//! speaker's body video ([`hnm_stream`]) with the 3D head ([`head`],
//! [`raster3d`]) and its lip-sync ([`lipsync`]), the subtitles
//! ([`subtitle`]), the reverse shots and the music ducked.

#![forbid(unsafe_code)]

pub mod camera;
pub mod crt;
pub mod dialog;
pub mod engine;
pub mod fpu;
pub mod head;
pub mod hnm_stream;
pub mod lipsync;
pub mod look;
pub mod png;
pub mod raster3d;
pub mod sound;
pub mod subtitle;
pub mod warp;

pub use camera::Camera;
pub use engine::{Engine, EngineError};
pub use fpu::Fpu;
pub use warp::{Renderer, Variant};

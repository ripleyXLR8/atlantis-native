//! The engine of *Atlantis II* (Cryo, 1999).
//!
//! Comments cite addresses in the original `atlantis2.exe` (the French CD
//! build of 1999, 2199552 bytes) so that each behaviour stays traceable to
//! the original code. For now it shows a view: the cube panorama drawn as
//! the original draws it, turned with the mouse within the original's
//! limits; a new game opens on its first view. The dialog heads
//! ([`head`], [`raster3d`]) and their lip-sync ([`lipsync`]) draw and move
//! as the original's, not yet shown in play.

#![forbid(unsafe_code)]

pub mod camera;
pub mod crt;
pub mod engine;
pub mod fpu;
pub mod head;
pub mod lipsync;
pub mod look;
pub mod png;
pub mod raster3d;
pub mod warp;

pub use camera::Camera;
pub use engine::{Engine, EngineError};
pub use fpu::Fpu;
pub use warp::Renderer;

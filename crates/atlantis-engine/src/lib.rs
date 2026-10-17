//! The engine of *Atlantis: The Lost Tales* (Cryo, 1997).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (GPLv3, the ScummVM
//! team). Comments cite addresses in the original `atlantis.exe` so that
//! each behaviour stays traceable to the original code.

pub mod warp;

pub use warp::Warp;

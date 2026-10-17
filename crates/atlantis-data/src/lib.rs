//! Data files of *Atlantis: The Lost Tales* (Cryo, 1997).
//!
//! Translated from the ScummVM CryOmni3D Atlantis engine (GPLv3). Comments
//! cite addresses in the original `atlantis.exe` (the 2001 English build of
//! 714240 bytes unless said otherwise) so that each behaviour stays
//! traceable to the original code.

pub mod big;
pub mod exe;
pub mod files;
pub mod game;

pub use big::BigArchive;
pub use exe::{ExeBuild, ExeTables};
pub use files::{FileSource, MemFiles, StdDir};
pub use game::{Game, GameError};

//! Data files of *Atlantis II* (Cryo, 1999).
//!
//! Comments cite addresses in the original `atlantis2.exe` (the French CD
//! build of 1999, 2199552 bytes; see [`exe::KNOWN_BUILDS`]) so that each
//! behaviour stays traceable to the original code.
//!
//! - [`exe`]: which build an `atlantis2.exe` is;
//! - [`files`]: the install folder and the four discs, with the disc rule
//!   of the `MONDE` script variable;
//! - [`wam`]: the `WAM V2.8` place files (views, moves, 3D click zones);
//! - [`con`]: what a new game starts with, read from `chapi000.con`;
//! - [`db`]: the `.db` 3D scenes (the dialog heads, the 3D puzzles);
//! - [`tga`]: the heads' texture maps;
//! - [`dialog`]: the dialog scripts' speakers and lines.

#![forbid(unsafe_code)]

pub mod con;
pub mod db;
pub mod dialog;
pub mod exe;
pub mod files;
pub mod tga;
pub mod wam;

pub use exe::{identify, Build, ExeError};
pub use files::{A2Files, Folder};
pub use wam::Wam;

//! The engine of *Atlantis III: The New World* (Cryo, 2001), unfinished:
//! not part of the released program, run only by the developers' example
//! (`cargo run --example atlantis3`).
//!
//! Comments cite addresses in the GOG.com release's `Game.exe` (3,473,408
//! bytes, `atlantis3_data::exe::KNOWN_BUILDS`). Where *Atlantis III* runs
//! *Atlantis II*'s code (the cube renderer, the camera, the CON
//! interpreter), `atlantis2-engine`'s is used.
//!
//! - [`startup`]: `Atlantis3.cfg`, the install's language and the screen;
//! - [`cinema`]: the cinematics and the intro, sound included;
//! - [`audio`]: the port's mix of the sound outputs.

#![forbid(unsafe_code)]

pub mod audio;
pub mod caption;
pub mod cinema;
pub mod dialog;
pub mod game;
pub mod inventory;
pub mod lipsync;
pub mod maps;
pub mod menu;
pub mod place_anim;
pub mod startup;
pub mod subjects;
pub mod text;
pub mod travel;
pub mod wam_setup;

pub use cinema::{Cinema, CinemaError};
pub use game::{BoardInput, Game, GameError, Mode};
pub use startup::Startup;

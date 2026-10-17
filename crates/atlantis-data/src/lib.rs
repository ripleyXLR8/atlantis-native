//! Data files of *Atlantis: The Lost Tales* (Cryo, 1997).
//!
//! Translated from the ScummVM CryOmni3D Atlantis engine (GPLv3). Comments
//! cite addresses in the original `atlantis.exe` (the 2001 English build of
//! 714240 bytes unless said otherwise) so that each behaviour stays
//! traceable to the original code.

pub mod big;
pub mod exe;
pub mod exe_icon;
pub mod files;
pub mod game;
pub mod head;
mod reader;
pub mod saves;
pub mod sprite;
pub mod sprite_anim;
pub mod sprite_blend;
pub mod sprite_list;
pub mod wam;

pub use big::BigArchive;
pub use exe::{ExeBuild, ExeTables};
pub use files::{FileSource, MemFiles, StdDir};
pub use game::{Edition, Game, GameError};
pub use sprite::{read_spr_file, Image565, SprSprite};
pub use sprite_list::SpritePlaceEntry;
pub use wam::{Place, Wam};

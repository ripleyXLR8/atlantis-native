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
//! - [`con`]: what a new game starts with, read from `chapi000.con`, and
//!   the C number conversions of the CON interpreter;
//! - [`defnames`]: `defnames.con`, the names the CON scripts and the places
//!   use;
//! - [`spw`]: the place animations drawn into the cube faces;
//! - [`spr`]: the `SPRCRYO` sprite banks (cursors);
//! - [`db`]: the `.db` 3D scenes (the dialog heads, the 3D puzzles);
//! - [`tga`]: the heads' texture maps and the menus' backgrounds;
//! - [`menu_txt`]: the menus' texts, `loc\Menu.txt`;
//! - [`save`]: the original's saves (`save\ATLpp_ss.SAV`, the `ZapFile`
//!   container) and `atlant2.cfg`;
//! - [`dialog`]: the dialog scripts' speakers and lines, their texts, the
//!   worlds' file sets and heroes, and `Font.txt`;
//! - [`messages`]: the object names the bag bar shows;
//! - [`canoe`]: the canoe puzzle's waypoints and depth tiles;
//! - [`testing`]: what a game-data test does when its data is missing.

#![forbid(unsafe_code)]

pub mod canoe;
// "con" is a reserved device name on Windows: a file called con.rs
// cannot be checked out there, so the module's file has another name.
#[path = "con_script.rs"]
pub mod con;
pub mod db;
pub mod defnames;
pub mod dialog;
pub mod exe;
pub mod files;
pub mod menu_txt;
pub mod messages;
pub mod save;
pub mod spr;
pub mod spw;
pub mod testing;
pub mod tga;
pub mod wam;

pub use defnames::DefNames;
pub use exe::{identify, Build, ExeError};
pub use files::{A2Files, Folder};
pub use wam::Wam;

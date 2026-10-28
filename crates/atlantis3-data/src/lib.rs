//! Data files of *Atlantis III: The New World* (Cryo, 2001).
//!
//! Comments cite addresses in the GOG.com release's `Game.exe` (3,473,408
//! bytes, see [`exe::KNOWN_BUILDS`]) and in its `spr_p5.dll` (2001), so
//! that each behaviour stays traceable to the original code. Formats the
//! game shares with *Atlantis II* are read by `atlantis2-data` and
//! `atlantis-codecs`; this crate holds what differs.
//!
//! - [`exe`]: which build a `Game.exe` is;
//! - [`files`]: the installed folder and its `Datas\`;
//! - [`spr`]: the `SPRCRYO` sprite banks and the `SPWCRYO` place sprites,
//!   with the blitters of the 2001 sprite library;
//! - [`db`]: the `.db` 3D scenes (their light records);
//! - [`hnm_header`]: what the video libraries read of an HNM6 header, and
//!   the cinematics' sound chunks;
//! - [`tokens`]: the text tokenizer the text files share;
//! - [`dialog`]: `info.txt`, `com.txt` and `Dial.txt`, the dialog script;
//! - [`objects`]: `Objects.txt` and `Errors.txt`;
//! - [`cine_txt`]: the cinematics' subtitle cues and captions;
//! - [`font_txt`]: the fonts of `font.txt`;
//! - [`atl`]: the `.atl` menu layouts and their `i_*.txt` tooltips;
//! - [`cfg`]: the settings file `Atlantis3.cfg`;
//! - [`save`]: the text saves;
//! - [`con`]: the chapter scripts, scanned (sections, conditions, keys);
//! - [`coord`]: `Coord*.txt`, the places' 3D sound positions;
//! - [`vue`], [`zbf`], [`set`]: the travel videos' cameras, the Licorne
//!   board's depth buffers and `Toymf.set`;
//! - [`testing`]: what a game-data test does when its data is missing.

#![forbid(unsafe_code)]

pub mod atl;
pub mod cfg;
pub mod cine_txt;
// "con" is a reserved device name on Windows: the module's file has
// another name.
#[path = "con_script.rs"]
pub mod con;
pub mod coord;
pub mod db;
pub mod dialog;
pub mod exe;
pub mod files;
pub mod font_txt;
pub mod hnm_header;
pub mod objects;
pub mod save;
pub mod set;
pub mod spr;
pub mod testing;
pub mod tokens;
pub mod vue;
pub mod zbf;

pub use exe::{identify, Build, ExeError};
pub use files::A3Files;

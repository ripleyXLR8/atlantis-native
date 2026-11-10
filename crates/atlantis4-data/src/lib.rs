//! Data files of *Atlantis Evolution* (Atlantis IV; Atlantis Interactive
//! Entertainment / Microïds, 2004), as released by GOG.com: a 2012
//! Tetraedge Engine build (C++, Lua 5.1, OpenGL, OpenAL) sharing no format
//! with the three earlier games.
//!
//! Comments cite addresses in that release's `Atlantis Evolution.exe`
//! (2,363,910 bytes, see [`exe::KNOWN_BUILDS`]), so that each behaviour
//! stays traceable to the original code; "file +0x..." is an offset in that
//! file.
//!
//! - [`exe`]: which build an executable is;
//! - [`files`]: the installed folder, `lang.ini` and the scene names;
//! - [`wpa`]: the `.wpa` warp packages;
//! - [`anim`]: `anim.anim`, a package's cube cells and click meshes;
//! - [`xml`]: the TinyXML-like reader of the rootless Windows-1252 XML;
//! - [`scene`]: a scene's description (objects, animations, exits);
//! - [`lua`]: the script inventory and loader (no interpreter yet);
//! - [`texts`]: the dialog lines, object and document names, combinations;
//! - [`font`]: the `TESF` fonts' metrics;
//! - [`testing`]: what a game-data test does when its data is missing.

#![forbid(unsafe_code)]

pub mod anim;
pub mod exe;
pub mod files;
pub mod font;
pub mod lua;
pub mod scene;
pub mod testing;
pub mod texts;
pub mod wpa;
pub mod xml;

pub use exe::{identify, Build, ExeError};
pub use files::{A4Files, WarpPath};

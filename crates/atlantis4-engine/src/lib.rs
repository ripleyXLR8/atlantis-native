//! The engine of *Atlantis Evolution* (Atlantis IV): for now the warp view
//! of a node, drawn as the original's `TeWarp3` draws it and driven by the
//! node's scripts.
//!
//! Comments cite addresses in the GOG.com release's `Atlantis Evolution.exe`
//! (atlantis4-gog-en-2363910, see `atlantis4_data::exe`).
//!
//! - [`camera`]: `TeCamera`'s projection, `getRay`, `project` and the
//!   view's rotation;
//! - [`view`]: `GameWarp`'s yaw and pitch, their limits and mouse look;
//! - [`cube`]: the cube's faces, the background cells and their atlas
//!   slots, the exits' click blocks;
//! - [`pick`]: clicks on animations and exits;
//! - [`markers`]: the markers' placement;
//! - [`frame_anim`]: `TeFrameAnim`'s frame for a time;
//! - [`render`]: the view drawn on the processor (GL's `GL_LINEAR` model),
//!   background and tile meshes;
//! - [`node`]: a node loaded from the install;
//! - [`package`]: an object's or animation's tiles, pictures and picks;
//! - [`warp`]: the node in play: draw list, markers, exits;
//! - [`sprite`]: the markers' and the cursor's 2-D sprites;
//! - [`layout`]: the GUI layouts' sizes and positions;
//! - [`movie`]: video sprites, `PlayMovie`'s full-screen movie and its
//!   skip button;
//! - [`gui`]: the 2-D layer every screen is built on: layout files,
//!   geometry, buttons, animations, pictures, text;
//! - [`dialog`]: the dialog bar, conversations and answers;
//! - [`inventory`], [`documents`]: the inventory and its selection, the
//!   documents browser and the shown document, the diary and the puzzle
//!   timer, with their save elements ([`savexml`]);
//! - [`screens`]: the HUD, the inventory menu, the shown document and
//!   the object notification;
//! - [`game`]: the bindings' engine side for the scripts.
//!
//! Not here yet: menus, saves on disk.

#![forbid(unsafe_code)]

pub mod camera;
pub mod cube;
pub mod dialog;
pub mod documents;
pub mod ending;
pub mod frame_anim;
pub mod game;
mod game_gui;
pub mod game_saves;
pub mod gui;
pub mod inventory;
pub mod layout;
pub mod markers;
pub mod movie;
pub mod node;
pub mod oalmix;
pub mod package;
pub mod pick;
pub mod render;
pub mod saves;
pub mod savexml;
pub mod screens;
pub mod sound;
pub mod sprite;
pub mod view;
pub mod warp;

pub use camera::Camera;
pub use game::Game;
pub use node::Node;
pub use view::View;

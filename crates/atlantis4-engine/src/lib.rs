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
//! - [`game`]: the bindings' engine side for the scripts.
//!
//! Not here yet: sounds, dialogs, the inventory and the rest of the GUI.

#![forbid(unsafe_code)]

pub mod camera;
pub mod cube;
pub mod frame_anim;
pub mod game;
pub mod layout;
pub mod markers;
pub mod movie;
pub mod node;
pub mod package;
pub mod pick;
pub mod render;
pub mod sprite;
pub mod view;
pub mod warp;

pub use camera::Camera;
pub use game::Game;
pub use node::Node;
pub use view::View;

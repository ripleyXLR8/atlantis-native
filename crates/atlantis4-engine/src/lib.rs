//! The engine of *Atlantis Evolution* (Atlantis IV): for now the warp view
//! of a node, drawn as the original's `TeWarp3` draws it.
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
//! - [`render`]: the view drawn on the processor (GL's `GL_LINEAR` model);
//! - [`node`]: a node loaded from the install.
//!
//! Not here yet: the Lua scripts, the objects and Theora animations drawn
//! over the background, sounds, the cursor and the interface.

#![forbid(unsafe_code)]

pub mod camera;
pub mod cube;
pub mod frame_anim;
pub mod markers;
pub mod node;
pub mod pick;
pub mod render;
pub mod view;

pub use camera::Camera;
pub use node::Node;
pub use view::View;

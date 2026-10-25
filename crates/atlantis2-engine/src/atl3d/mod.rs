//! `Atl3D`, the original's 3D library for the puzzles
//! (`Atl3D_LoadDatabase` @ 0x0042a920, `Atl3D_draw_DataBase`
//! @ 0x0042a320), along the two paths the game takes through it:
//!
//! - [`ortho`]: the canoe's (`pz_pirogue`, its own loop type 9): its
//!   scene nodes, databases and orthographic camera, every face sent to
//!   the generic face drawer (@ 0x0042d020);
//! - [`warp`]: the spider web's (`pz_araig`): models drawn over the
//!   panorama with the warp's camera, the Pentium II routines and the
//!   Atl3D Z buffer.
//!
//! The two were read and ported each for its own puzzle and share the
//! heads' helpers ([`crate::head`]: quaternions, tracks, node updates);
//! the parts of the library both paths go through (the `.db` loader's
//! planes, the tracks) are written twice, each checked against the
//! original on its own puzzle's frames.

pub mod ortho;
pub mod warp;

pub use ortho::{
    draw_database, Camera3, Database, DrawContext, DrawTarget, Library, Nodes, Records, Texture3,
    ROOT,
};
pub use warp::{
    MapInfo, MeshInfo, Model, ObjectInfo, ObjectStage, View, FAR, MESH_FLAGS, NEAR, SENTINEL_PLANE,
};

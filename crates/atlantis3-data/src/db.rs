//! The `.db` 3D scenes (`Datas\mesh\`, `Datas\map\`): the dialog heads and
//! the Licorne puzzle's board.
//!
//! The reader (@ 0x0050ce80, `Load, invalid .db file` @ 0x0072db04) has
//! *Atlantis II*'s sections and record layouts but one: the records of the
//! last section, the scene's lights, are 64 bytes after their strings
//! (@ 0x0050cdc0: a vec4, a vec3, nine dwords), not 68
//! ([`atlantis2_data::db::Dialect`]). Every shipped file is version 11.

pub use atlantis2_data::db::{DbError, Dialect, Light, Scene};

/// Reads a `.db` file with this game's reader; also says how many bytes it
/// consumed.
pub fn parse_len(data: &[u8]) -> Result<(Scene, usize), DbError> {
    Scene::parse_len_as(data, Dialect::Atlantis3)
}

/// Reads a `.db` file with this game's reader.
pub fn parse(data: &[u8]) -> Result<Scene, DbError> {
    parse_len(data).map(|(s, _)| s)
}

/// A light record's fields (@ 0x0050cdc0); the meanings past the strings
/// are not established by a user of them (the game reads the lights and
/// no code was found that uses them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightFields {
    /// Unit quaternion in the files [meaning not established].
    pub rotation: [f32; 4],
    pub position: [f32; 3],
    /// Three colours, `0xffRRGGBB` in the files, all equal.
    pub colours: [u32; 3],
    /// Three dwords, 0 in every file.
    pub unknown: [u32; 3],
    /// A cone angle in radians [meaning not established].
    pub cone: f32,
    /// A multiplier [meaning not established].
    pub multiplier: f32,
    /// 1 for `AMBIENT` [bits not established].
    pub kind: u32,
}

/// The fields of an *Atlantis III* light record (nine dwords).
pub fn light_fields(l: &Light) -> Option<LightFields> {
    let w: &[u32; 9] = l.words.as_slice().try_into().ok()?;
    Some(LightFields {
        rotation: l.rotation,
        position: l.position,
        colours: [w[0], w[1], w[2]],
        unknown: [w[3], w[4], w[5]],
        cone: f32::from_bits(w[6]),
        multiplier: f32::from_bits(w[7]),
        kind: w[8],
    })
}

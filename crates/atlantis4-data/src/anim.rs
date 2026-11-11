//! `anim.anim`, the geometry of a warp package: which cells of the cube a
//! texture (JPEG atlas, JPEG with alpha, or Theora with alpha) covers, frame
//! range by frame range, and the meshes the clicks are tested against.
//!
//! The file is a `TeZLibStream` (`TeZLibStream::open`,
//! atlantis4-gog-en-2363910 @ 0x004bb740):
//!
//! | Field | Type | Read at |
//! |---|---|---|
//! | version | `u8`, must be 1 (error string @ 0x005aed48) | @ 0x004bb783, test @ 0x004bb7a2 |
//! | compressed size | `u32` | @ 0x004bb7cf |
//! | raw size | `u32` | @ 0x004bb801 |
//! | data | zlib stream, inflated with zlib's `uncompress` (@ 0x00550140) | @ 0x004bb83c, @ 0x004bb867 |
//!
//! The inflated bytes are read by `TeWarp3::Anim::load`
//! (atlantis4-gog-en-2363910, the body from @ 0x004f2fc6; it opens
//! `anim.anim`, string @ 0x005b2458, in the package @ 0x004f2f60):
//!
//! | Field | Type | Read at |
//! |---|---|---|
//! | version | `u8`, must be 1 (error string @ 0x005b24a8) | @ 0x004f2fe6, test @ 0x004f2fe8 |
//! | frame count | `u32`, kept in the animation (+0x78) | @ 0x004f30d1 |
//! | tile count | `u32` | @ 0x004f30ec |
//! | tiles | each read by `TeWarp3::AnimTile::load` (@ 0x004f1870), below | @ 0x004f3164 |
//! | pick count | `u32` | @ 0x004f317d |
//! | picks | per pick: `u32` first frame (@ 0x004f321f), `u32` vertex count and 12 bytes per vertex (@ 0x004f3364, @ 0x004f33cf), `u32` index count and a `u32` per index (@ 0x004f3403, @ 0x004f3487) | loop @ 0x004f31c0..0x004f34c8 |
//!
//! A tile (`TeWarp3::AnimTile::load`, @ 0x004f1870): its texture's name
//! (`u32` length + bytes, @ 0x004f18d0), a `u32` count of frame ranges
//! (@ 0x004f19ad), then per range a `u32` first frame (@ 0x004f1a77), a
//! `u32` vertex count and 12 bytes (three `f32`) per vertex (@ 0x004f1c84,
//! @ 0x004f1cee), a `u32` UV count and 8 bytes per UV (@ 0x004f1d1f,
//! @ 0x004f1d90) and a `u32` index count and a `u32` per index (@ 0x004f1dbd,
//! @ 0x004f1e4e).
//!
//! Over the GOG.com data (2,613 files) every file parses to its last byte
//! this way, the first frames of a tile's ranges rise, the UV count equals
//! the vertex count and every index is below it. A background's 28 tiles
//! hold one range each, of 6 × 13 × 13 = 1,014 quads in all, on the cube
//! |x|, |y| or |z| = 500.

use crate::wpa::latin1;

/// The `TeZLibStream` header and its inflated data.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, AnimError> {
    let version = *data.first().ok_or(AnimError::Truncated)?;
    if version != 1 {
        return Err(AnimError::StreamVersion(version));
    }
    let mut c = Cursor { data, at: 1 };
    let compressed = c.u32()? as usize;
    let raw = c.u32()? as usize;
    let body = c.bytes(compressed)?;
    let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(body, raw)
        .map_err(|_| AnimError::Inflate)?;
    if out.len() != raw {
        return Err(AnimError::Inflate);
    }
    Ok(out)
}

/// A tile's frame range: the mesh its texture is drawn on from
/// `first_frame` on.
#[derive(Clone, Debug, PartialEq)]
pub struct TileRange {
    pub first_frame: u32,
    pub vertices: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Triangles, three indices each.
    pub indices: Vec<u32>,
}

/// One texture of the animation and where it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimTile {
    /// `000000.jpg`, `000000.ogv` ...; see [`crate::wpa::TileSource`].
    pub name: String,
    pub ranges: Vec<TileRange>,
}

/// A click mesh, from `first_frame` on.
#[derive(Clone, Debug, PartialEq)]
pub struct PickMesh {
    pub first_frame: u32,
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// An `anim.anim`.
#[derive(Clone, Debug, PartialEq)]
pub struct Anim {
    pub frame_count: u32,
    pub tiles: Vec<AnimTile>,
    pub picks: Vec<PickMesh>,
}

/// Why an `anim.anim` cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnimError {
    /// The `TeZLibStream` version is not 1.
    StreamVersion(u8),
    /// The animation version is not 1.
    Version(u8),
    Truncated,
    /// The zlib stream is broken or not of the stated size.
    Inflate,
    /// Bytes left after the picks.
    Trailing(usize),
}

impl std::fmt::Display for AnimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnimError::StreamVersion(v) => write!(f, "zlib stream version {v}, only 1 is read"),
            AnimError::Version(v) => write!(f, "animation version {v}, only 1 is read"),
            AnimError::Truncated => write!(f, "animation truncated"),
            AnimError::Inflate => write!(f, "animation's zlib stream broken"),
            AnimError::Trailing(n) => write!(f, "{n} bytes after the animation"),
        }
    }
}

impl std::error::Error for AnimError {}

struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn bytes(&mut self, len: usize) -> Result<&[u8], AnimError> {
        let end = self.at.checked_add(len).ok_or(AnimError::Truncated)?;
        let b = self.data.get(self.at..end).ok_or(AnimError::Truncated)?;
        self.at = end;
        Ok(b)
    }

    fn u32(&mut self) -> Result<u32, AnimError> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, AnimError> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// A count of items of `size` bytes each, refused when the bytes
    /// cannot hold them.
    fn count(&mut self, size: usize) -> Result<usize, AnimError> {
        let n = self.u32()? as usize;
        if n.saturating_mul(size) > self.data.len() - self.at {
            return Err(AnimError::Truncated);
        }
        Ok(n)
    }

    fn vec3s(&mut self) -> Result<Vec<[f32; 3]>, AnimError> {
        let n = self.count(12)?;
        (0..n)
            .map(|_| Ok([self.f32()?, self.f32()?, self.f32()?]))
            .collect()
    }

    fn vec2s(&mut self) -> Result<Vec<[f32; 2]>, AnimError> {
        let n = self.count(8)?;
        (0..n).map(|_| Ok([self.f32()?, self.f32()?])).collect()
    }

    fn u32s(&mut self) -> Result<Vec<u32>, AnimError> {
        let n = self.count(4)?;
        (0..n).map(|_| self.u32()).collect()
    }
}

impl Anim {
    /// The inflated bytes of an `anim.anim`.
    pub fn parse_raw(raw: &[u8]) -> Result<Anim, AnimError> {
        let version = *raw.first().ok_or(AnimError::Truncated)?;
        if version != 1 {
            return Err(AnimError::Version(version));
        }
        let mut c = Cursor { data: raw, at: 1 };
        let frame_count = c.u32()?;
        let tile_count = c.count(8)?;
        let mut tiles = Vec::with_capacity(tile_count);
        for _ in 0..tile_count {
            let len = c.u32()? as usize;
            let name = latin1(c.bytes(len)?);
            let range_count = c.count(16)?;
            let mut ranges = Vec::with_capacity(range_count);
            for _ in 0..range_count {
                let first_frame = c.u32()?;
                let vertices = c.vec3s()?;
                let uvs = c.vec2s()?;
                let indices = c.u32s()?;
                ranges.push(TileRange {
                    first_frame,
                    vertices,
                    uvs,
                    indices,
                });
            }
            tiles.push(AnimTile { name, ranges });
        }
        let pick_count = c.count(12)?;
        let mut picks = Vec::with_capacity(pick_count);
        for _ in 0..pick_count {
            let first_frame = c.u32()?;
            let vertices = c.vec3s()?;
            let indices = c.u32s()?;
            picks.push(PickMesh {
                first_frame,
                vertices,
                indices,
            });
        }
        if c.at != raw.len() {
            return Err(AnimError::Trailing(raw.len() - c.at));
        }
        Ok(Anim {
            frame_count,
            tiles,
            picks,
        })
    }

    /// An `anim.anim` entry as stored in its package.
    pub fn parse(data: &[u8]) -> Result<Anim, AnimError> {
        Anim::parse_raw(&inflate(data)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    fn sample_raw() -> Vec<u8> {
        let mut r = vec![1u8];
        r.extend(le(3)); // frames
        r.extend(le(1)); // tiles
        r.extend(le(10));
        r.extend(b"000000.ogv");
        r.extend(le(1)); // ranges
        r.extend(le(0)); // first frame
        r.extend(le(3));
        for v in [0.0f32, 0.0, 500.0, 1.0, 0.0, 500.0, 0.0, 1.0, 500.0] {
            r.extend(v.to_le_bytes());
        }
        r.extend(le(3));
        for v in [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0] {
            r.extend(v.to_le_bytes());
        }
        r.extend(le(3));
        for i in [0, 1, 2] {
            r.extend(le(i));
        }
        r.extend(le(1)); // picks
        r.extend(le(2)); // first frame
        r.extend(le(0));
        r.extend(le(0));
        r
    }

    #[test]
    fn an_animation_reads_its_tiles_and_picks() {
        let a = Anim::parse_raw(&sample_raw()).unwrap();
        assert_eq!(a.frame_count, 3);
        assert_eq!(a.tiles[0].name, "000000.ogv");
        assert_eq!(a.tiles[0].ranges[0].vertices[1], [1.0, 0.0, 500.0]);
        assert_eq!(a.tiles[0].ranges[0].indices, vec![0, 1, 2]);
        assert_eq!(a.picks[0].first_frame, 2);
        let mut extra = sample_raw();
        extra.push(0);
        assert_eq!(Anim::parse_raw(&extra), Err(AnimError::Trailing(1)));
    }

    #[test]
    fn the_stream_header_is_checked() {
        assert_eq!(inflate(&[2]), Err(AnimError::StreamVersion(2)));
        // An empty zlib stream (no compression) of zero bytes.
        let z = [0x78, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];
        let mut s = vec![1u8];
        s.extend(le(z.len() as u32));
        s.extend(le(0));
        s.extend(z);
        assert_eq!(inflate(&s), Ok(Vec::new()));
        s[5] = 1; // raw size 1: not what the stream holds
        assert_eq!(inflate(&s), Err(AnimError::Inflate));
    }
}

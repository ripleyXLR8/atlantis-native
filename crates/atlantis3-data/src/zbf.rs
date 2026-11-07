//! `map\PL_Fond<width>.zbf`: the depth buffer of the Licorne puzzle's
//! pre-rendered background (`PL_Fond<width>.tga`).
//!
//! `CPuzzle_Licorne`'s set-up (@ 0x004b9e40) picks the pair by the
//! puzzle's screen width: 640 → `PL_Fond640`, 800 → `PL_Fond800`, 1024 →
//! `PL_Fond1024` (@ 0x004ba523, @ 0x004ba604, @ 0x004ba6e0). It reads the
//! 36-byte header in one go (@ 0x0051e340), logs `Header : W:%d H:%d`,
//! creates a `width` x `height` surface of pixel format 0xa5 (@ 0x0051e320,
//! @ 0x004eb290), locks it and reads `height` rows of `width` float32 into
//! it (@ 0x0051e360), then copies that surface into the puzzle's own
//! (object +0x8c, @ 0x004ecc40): the depths become the Z buffer the 3D
//! pieces of the board are drawn against.
//!
//! Layout, little-endian:
//!
//! | Offset | Size | Field |
//! |---|---|---|
//! | 0x00 | 7 x f32 | [`ZbfHeader::unknown`]: read, never used |
//! | 0x1c | u32 | width (@ 0x0051e360: header +0x1c) |
//! | 0x20 | u32 | height (header +0x20) |
//! | 0x24 | width x height x f32 | depths, row by row, top row first |
//!
//! The three shipped files hold the same seven header floats and depths
//! of about 1e-4 (a reciprocal depth, larger is nearer `[INFERRED]`) with a
//! background value of 1e-30 where nothing was rendered.

use std::fmt;

/// Size of the header.
pub const HEADER_SIZE: usize = 0x24;

/// The value of the pixels where the render left nothing (every shipped
/// file uses it) `[CONFIRMED-DATA]`.
pub const EMPTY_DEPTH: f32 = 1.0e-30;

/// The header.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZbfHeader {
    /// +0x00..+0x1b: read with the header (@ 0x0051e340) and never read
    /// again (`CPuzzle_Licorne` @ 0x004b9e40 keeps them in locals it does
    /// not use). `[UNKNOWN]` meaning; the last one equals the field of view,
    /// in degrees, of the camera of the puzzle's scene `PL_Dum&Cam.DB`
    /// `[CONFIRMED-DATA]`, so the seven are probably the render camera
    /// `[INFERRED]`.
    pub unknown: [f32; 7],
    /// +0x1c.
    pub width: u32,
    /// +0x20.
    pub height: u32,
}

/// A depth buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct Zbf {
    pub header: ZbfHeader,
    /// `width * height` depths, row by row.
    pub depth: Vec<f32>,
}

/// Why a `.zbf` file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZbfError {
    /// Shorter than its header says.
    Truncated,
    /// A size that cannot be held in memory.
    TooLarge,
}

impl fmt::Display for ZbfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZbfError::Truncated => write!(f, "truncated .zbf file"),
            ZbfError::TooLarge => write!(f, ".zbf size too large"),
        }
    }
}

impl std::error::Error for ZbfError {}

fn le32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

impl ZbfHeader {
    /// Reads the 36-byte header.
    pub fn parse(data: &[u8]) -> Result<ZbfHeader, ZbfError> {
        let d = data.get(..HEADER_SIZE).ok_or(ZbfError::Truncated)?;
        let mut unknown = [0.0; 7];
        for (i, v) in unknown.iter_mut().enumerate() {
            *v = f32::from_bits(le32(d, 4 * i));
        }
        Ok(ZbfHeader {
            unknown,
            width: le32(d, 0x1c),
            height: le32(d, 0x20),
        })
    }
}

impl Zbf {
    /// Reads a whole file. Bytes after the last row are ignored, as the
    /// original reads exactly `height` rows.
    pub fn parse(data: &[u8]) -> Result<Zbf, ZbfError> {
        let header = ZbfHeader::parse(data)?;
        let count = (header.width as usize)
            .checked_mul(header.height as usize)
            .ok_or(ZbfError::TooLarge)?;
        let bytes = count.checked_mul(4).ok_or(ZbfError::TooLarge)?;
        let end = HEADER_SIZE.checked_add(bytes).ok_or(ZbfError::TooLarge)?;
        let body = data.get(HEADER_SIZE..end).ok_or(ZbfError::Truncated)?;
        let depth = body
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&c| f32::from_le_bytes(c))
            .collect();
        Ok(Zbf { header, depth })
    }

    /// The depth at (`x`, `y`), `None` outside the buffer.
    pub fn depth_at(&self, x: u32, y: u32) -> Option<f32> {
        if x >= self.header.width || y >= self.header.height {
            return None;
        }
        self.depth
            .get(y as usize * self.header.width as usize + x as usize)
            .copied()
    }

    /// The file `CPuzzle_Licorne` loads for a screen `width` pixels wide
    /// (640, 800 or 1024). For any other width the original sets no name
    /// and goes on to open an empty one (@ 0x004ba6e5 jumps to
    /// @ 0x004ba7bc).
    pub fn file_name(width: u32) -> Option<&'static str> {
        match width {
            640 => Some("PL_Fond640.zbf"),
            800 => Some("PL_Fond800.zbf"),
            1024 => Some("PL_Fond1024.zbf"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(w: u32, h: u32, depths: &[f32]) -> Vec<u8> {
        let mut d = Vec::new();
        for i in 0..7 {
            d.extend((i as f32).to_le_bytes());
        }
        d.extend(w.to_le_bytes());
        d.extend(h.to_le_bytes());
        for v in depths {
            d.extend(v.to_le_bytes());
        }
        d
    }

    #[test]
    fn reads_header_and_rows() {
        let z = Zbf::parse(&file(3, 2, &[1.0, 2.0, 3.0, 4.0, 5.0, EMPTY_DEPTH])).unwrap();
        assert_eq!(z.header.width, 3);
        assert_eq!(z.header.height, 2);
        assert_eq!(z.header.unknown[6], 6.0);
        assert_eq!(z.depth_at(1, 1), Some(5.0));
        assert_eq!(z.depth_at(2, 1), Some(EMPTY_DEPTH));
        assert_eq!(z.depth_at(3, 0), None);
    }

    #[test]
    fn short_file_is_refused_and_extra_bytes_ignored() {
        assert_eq!(Zbf::parse(&file(2, 2, &[0.0; 3])), Err(ZbfError::Truncated));
        let mut d = file(1, 1, &[7.0]);
        d.extend([1, 2, 3]);
        assert_eq!(Zbf::parse(&d).unwrap().depth, vec![7.0]);
        assert_eq!(ZbfHeader::parse(&[0; 35]), Err(ZbfError::Truncated));
    }

    #[test]
    fn file_by_width() {
        assert_eq!(Zbf::file_name(800), Some("PL_Fond800.zbf"));
        assert_eq!(Zbf::file_name(1280), None);
    }
}

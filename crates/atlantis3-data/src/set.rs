//! `map\Toymf.set`: a 25-byte file with no reader.
//!
//! **No reader in the executable.** `Game.exe` holds no `.set` name and no
//! `Toymf` string (string search of `Game.exe`, `spr_p5.dll`,
//! `spr_p6.dll` and `Atlantis3.exe`). Its name is that of a head texture
//! (`map\Stand1\Toymf.tga`, the half-closed-eyes variant of `To.tga`), so
//! it is probably a leftover of a texturing tool `[INFERRED]`. It is kept
//! raw: nothing in the game depends on it.
//!
//! Read as five little-endian u32 and five bytes, the shipped file holds
//! small values only (all below 0x1000) `[CONFIRMED-DATA]`; their meaning is
//! `[UNKNOWN]`.

use std::fmt;

/// The size of the only shipped `.set` file.
pub const SHIPPED_SIZE: usize = 25;

/// A `.set` file, raw.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetFile {
    /// The whole file.
    pub raw: Vec<u8>,
}

/// Why a `.set` file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetError {
    /// Not [`SHIPPED_SIZE`] bytes: no other layout is known.
    Size(usize),
}

impl fmt::Display for SetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetError::Size(n) => write!(f, ".set file of {n} bytes, expected {SHIPPED_SIZE}"),
        }
    }
}

impl std::error::Error for SetError {}

impl SetFile {
    /// Keeps the bytes of a 25-byte file.
    pub fn parse(data: &[u8]) -> Result<SetFile, SetError> {
        if data.len() != SHIPPED_SIZE {
            return Err(SetError::Size(data.len()));
        }
        Ok(SetFile { raw: data.to_vec() })
    }

    /// The first 20 bytes as five little-endian u32 `[UNKNOWN]` meaning.
    pub fn words(&self) -> [u32; 5] {
        let mut w = [0; 5];
        for (i, v) in w.iter_mut().enumerate() {
            let b = &self.raw[4 * i..4 * i + 4];
            *v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        w
    }

    /// The last five bytes `[UNKNOWN]` meaning.
    pub fn tail(&self) -> [u8; 5] {
        let mut t = [0; 5];
        t.copy_from_slice(&self.raw[20..25]);
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_25_bytes() {
        let mut d: Vec<u8> = (0..20).collect();
        d.extend([9, 8, 7, 6, 5]);
        let s = SetFile::parse(&d).unwrap();
        assert_eq!(s.words()[1], u32::from_le_bytes([4, 5, 6, 7]));
        assert_eq!(s.tail(), [9, 8, 7, 6, 5]);
        assert_eq!(SetFile::parse(&d[..24]), Err(SetError::Size(24)));
    }
}

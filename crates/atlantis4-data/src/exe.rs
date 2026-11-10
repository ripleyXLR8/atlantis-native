//! Which build of *Atlantis Evolution*'s executable a folder holds.
//!
//! As for the other games, a build is recognised by its size and PE
//! timestamp and confirmed by the MD5 of the whole file. One build is
//! known: GOG.com's `Atlantis Evolution.exe`, a 2012 Tetraedge Engine
//! program (Visual C++ 2010), the reference every address in this crate's
//! comments belongs to. The executable holds no game text: the title, the
//! first scene and every line come from the data files.

use md5::{Digest, Md5};

/// A known build of the game's executable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Build {
    /// `atlantis4-<medium>-<language>-<size>`.
    pub id: &'static str,
    /// The edition, for people.
    pub label: &'static str,
    pub size: u64,
    /// The PE header's `TimeDateStamp`.
    pub pe_timestamp: u32,
    /// MD5 of the whole file.
    pub md5: [u8; 16],
}

/// The builds this port knows.
pub const KNOWN_BUILDS: [Build; 1] = [Build {
    // "en" is the `lang.ini` of the install it came from; the one install
    // carries all five languages.
    id: "atlantis4-gog-en-2363910",
    label: "Atlantis Evolution, GOG.com",
    size: 2_363_910,
    // 2012-11-29 13:54:04 UTC.
    pe_timestamp: 0x50b7_68fc,
    md5: [
        0xf2, 0xde, 0x38, 0x2a, 0xf5, 0x2f, 0x1d, 0x89, 0xe3, 0x2e, 0xd7, 0xc2, 0x86, 0xbe, 0x8b,
        0x4f,
    ],
}];

/// The file name of the game's executable.
pub const EXE_NAME: &str = "Atlantis Evolution.exe";

/// Why an executable is not a known build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExeError {
    /// Not a PE executable.
    NotPe,
    /// A PE file that is no known build: its size, timestamp and MD5.
    Unknown {
        size: u64,
        pe_timestamp: u32,
        md5: String,
    },
}

impl std::fmt::Display for ExeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExeError::NotPe => write!(f, "the game's executable is not a Windows executable"),
            ExeError::Unknown {
                size,
                pe_timestamp,
                md5,
            } => write!(
                f,
                "the game's executable is not a known build of Atlantis Evolution ({size} bytes, PE timestamp {pe_timestamp:#010x}, MD5 {md5})"
            ),
        }
    }
}

impl std::error::Error for ExeError {}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The little-endian `u32` at `at`, if the bytes are there.
pub fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let b = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The COFF header's `TimeDateStamp` of a PE file (`MZ`, `e_lfanew` at
/// 0x3c, `PE\0\0`, then the stamp 8 bytes after the signature).
pub fn pe_timestamp(exe: &[u8]) -> Option<u32> {
    if exe.get(..2)? != b"MZ" {
        return None;
    }
    let pe = usize::try_from(u32_at(exe, 0x3c)?).ok()?;
    if exe.get(pe..pe.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    u32_at(exe, pe + 8)
}

/// Which known build `exe` is (size, PE timestamp, then MD5).
pub fn identify(exe: &[u8]) -> Result<&'static Build, ExeError> {
    let stamp = pe_timestamp(exe).ok_or(ExeError::NotPe)?;
    let size = exe.len() as u64;
    let unknown = || {
        let md5: [u8; 16] = Md5::digest(exe).into();
        ExeError::Unknown {
            size,
            pe_timestamp: stamp,
            md5: hex(&md5),
        }
    };
    let build = KNOWN_BUILDS
        .iter()
        .find(|b| b.size == size && b.pe_timestamp == stamp)
        .ok_or_else(unknown)?;
    let md5: [u8; 16] = Md5::digest(exe).into();
    if md5 == build.md5 {
        Ok(build)
    } else {
        Err(unknown())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_pe(stamp: u32, size: usize) -> Vec<u8> {
        let mut exe = vec![0u8; size];
        exe[..2].copy_from_slice(b"MZ");
        exe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        exe[0x80..0x84].copy_from_slice(b"PE\0\0");
        exe[0x88..0x8c].copy_from_slice(&stamp.to_le_bytes());
        exe
    }

    #[test]
    fn a_file_that_is_not_pe_is_refused() {
        assert_eq!(identify(b"not an executable"), Err(ExeError::NotPe));
        assert_eq!(pe_timestamp(b"MZ"), None);
    }

    #[test]
    fn the_size_and_stamp_alone_do_not_make_a_known_build() {
        let b = &KNOWN_BUILDS[0];
        let exe = fake_pe(b.pe_timestamp, b.size as usize);
        assert_eq!(pe_timestamp(&exe), Some(b.pe_timestamp));
        match identify(&exe) {
            Err(ExeError::Unknown { size, .. }) => assert_eq!(size, b.size),
            other => panic!("{other:?}"),
        }
    }
}

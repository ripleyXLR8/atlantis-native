//! Which build of *Atlantis III*'s executable a folder holds.
//!
//! As for *Atlantis II* (`atlantis2_data::exe`), a build is recognised by
//! its size and PE timestamp and confirmed by the MD5 of the whole file.
//! One build is known: GOG.com's `Game.exe`, the reference every address in
//! this crate's comments belongs to. GOG's folder also holds
//! `Atlantis3.exe`, a launcher of the re-release that starts `Game.exe`
//! ([`LAUNCHER_SIZE`]), and the two sprite libraries ([`SPR_DLLS`]).

use md5::{Digest, Md5};

pub use atlantis2_data::exe::{bytes_at, pe_timestamp};

/// A known build of the game's executable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Build {
    /// `atlantis3-<medium>-<language>-<size>`.
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
    id: "atlantis3-gog-fr-3473408",
    label: "Atlantis III, GOG.com",
    size: 3_473_408,
    // 2003-04-24 09:50:26 UTC.
    pe_timestamp: 0x3ea7_b362,
    md5: [
        0xeb, 0x06, 0xea, 0xa7, 0x18, 0x87, 0x7d, 0xfc, 0xab, 0xee, 0x6e, 0x8f, 0x89, 0xd8, 0x5b,
        0x31,
    ],
}];

/// A sprite library shipped beside the executable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SprDllBuild {
    pub name: &'static str,
    pub size: u64,
    pub pe_timestamp: u32,
    pub md5: [u8; 16],
}

/// `spr_p5.dll` (2001-09-05) and `spr_p6.dll` (2001-09-03) of GOG's folder:
/// the executable loads the first for a CPU class below 4 and the second
/// from 4 (@ 0x0069c500).
pub const SPR_DLLS: [SprDllBuild; 2] = [
    SprDllBuild {
        name: "spr_p5.dll",
        size: 118_784,
        pe_timestamp: 0x3b95_f1d7,
        md5: [
            0x72, 0x26, 0x6c, 0x25, 0xb9, 0x68, 0xfb, 0x5e, 0xaf, 0x21, 0xd4, 0x18, 0x51, 0x02,
            0x7d, 0x7b,
        ],
    },
    SprDllBuild {
        name: "spr_p6.dll",
        size: 118_784,
        pe_timestamp: 0x3b93_a211,
        md5: [
            0x94, 0xcf, 0xb7, 0xcf, 0xaf, 0x57, 0x7a, 0x3a, 0xeb, 0x94, 0x91, 0x4e, 0x77, 0x69,
            0xd5, 0x52,
        ],
    },
];

/// The size of GOG's `Atlantis3.exe`, the re-release's launcher (19,968
/// bytes, PE timestamp 0x4c19e4d8).
pub const LAUNCHER_SIZE: u64 = 19_968;

/// The file name of the game's executable.
pub const EXE_NAME: &str = "Game.exe";

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
                "the game's executable is not a known build of Atlantis III ({size} bytes, PE timestamp {pe_timestamp:#010x}, MD5 {md5})"
            ),
        }
    }
}

impl std::error::Error for ExeError {}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Which known build `exe` is (size, PE timestamp, then MD5).
pub fn identify(exe: &[u8]) -> Result<&'static Build, ExeError> {
    let stamp = pe_timestamp(exe).ok_or(ExeError::NotPe)?;
    let size = exe.len() as u64;
    let md5: [u8; 16] = Md5::digest(exe).into();
    KNOWN_BUILDS
        .iter()
        .find(|b| b.size == size && b.pe_timestamp == stamp && b.md5 == md5)
        .ok_or(ExeError::Unknown {
            size,
            pe_timestamp: stamp,
            md5: hex(&md5),
        })
}

/// Which shipped sprite library `dll` is.
pub fn identify_spr_dll(dll: &[u8]) -> Option<&'static SprDllBuild> {
    let stamp = pe_timestamp(dll)?;
    let md5: [u8; 16] = Md5::digest(dll).into();
    SPR_DLLS
        .iter()
        .find(|b| b.size == dll.len() as u64 && b.pe_timestamp == stamp && b.md5 == md5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pe(stamp: u32, len: usize) -> Vec<u8> {
        let mut d = vec![0u8; len.max(0x100)];
        d[..2].copy_from_slice(b"MZ");
        d[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        d[0x80..0x84].copy_from_slice(b"PE\0\0");
        d[0x88..0x8c].copy_from_slice(&stamp.to_le_bytes());
        d
    }

    #[test]
    fn known_builds() {
        assert_eq!(
            hex(&KNOWN_BUILDS[0].md5),
            "eb06eaa718877dfcabee6e8f89d85b31"
        );
        assert_eq!(hex(&SPR_DLLS[0].md5), "72266c25b968fb5eaf21d41851027d7b");
        assert_eq!(hex(&SPR_DLLS[1].md5), "94cfb7cfaf577a3aeb94914e7769d552");
    }

    #[test]
    fn rejections() {
        assert_eq!(identify(b"MZ"), Err(ExeError::NotPe));
        match identify(&pe(0x3ea7_b362, 3_473_408)) {
            Err(ExeError::Unknown {
                size, pe_timestamp, ..
            }) => assert_eq!((size, pe_timestamp), (3_473_408, 0x3ea7_b362)),
            other => panic!("{other:?}"),
        }
        assert_eq!(identify_spr_dll(&pe(0x3b95_f1d7, 118_784)), None);
    }
}

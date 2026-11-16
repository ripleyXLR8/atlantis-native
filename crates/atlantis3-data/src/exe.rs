//! Which build of *Atlantis III*'s executable a folder holds.
//!
//! As for *Atlantis II* (`atlantis2_data::exe`), a build is recognised by
//! its size and PE timestamp and confirmed by the MD5 of the whole file.
//! Two builds are known: GOG.com's `Game.exe` (2003), the reference every
//! address in this crate's comments belongs to, and the French retail
//! CD's `Atlantis3.exe` (2001, [`Release::Cd2001`]). GOG's folder also
//! holds `Atlantis3.exe`, a launcher of the re-release that starts
//! `Game.exe` ([`LAUNCHER_SIZE`]), and the two sprite libraries
//! ([`SPR_DLLS`], the CD's are the same files).

use md5::{Digest, Md5};

pub use atlantis2_data::exe::{bytes_at, pe_timestamp};

/// The release of the game's code a build is (atlantis3-re
/// `binaries/atlantis3-cd-fr-3379200.md`: 9241 of the CD build's 9709
/// functions are GOG's byte for byte, branch targets and addresses
/// masked; the rest is the compiler's other code, the 2003 program's
/// additions and the differences below).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
    /// The retail CD's program (2001-09-11). Compared with GOG's:
    /// `CHANGECD=n` runs the disc test `Menu_TestChangeCD(n)`
    /// (atlantis3-cd-fr-3379200 @ 0x004497c9; GOG's `Game.exe` has the
    /// compare `nop`ped out @ 0x00449f62 and stores the variable); the
    /// scarab case's bank is `Scarabée.spr` (@ 0x00431c22, string
    /// @ 0x007017c4; GOG: `Scarabee.spr`); the subject menu's hit test
    /// has `count / 5 + 1` columns (@ 0x0043b3c0; GOG: 1 to 3 columns,
    /// 0 from 13 subjects); a warp frame that started a video is still
    /// flipped (@ 0x0043f840: only a dialog stops the flip); entering the
    /// Isis puzzle's views does not reset the warp cursor (@ 0x004c7210;
    /// GOG calls @ 0x004287f0 there); the intro plays `intro.hnm`, then
    /// `intro_ger.hnm` for `ger`, `intro_gre1.hnm` and `intro_gre2.hnm`
    /// for `gre` (@ 0x00458c40; GOG's table @ 0x00459360 has more
    /// languages).
    Cd2001,
    /// GOG.com's `Game.exe` (2003-04-24), the reference.
    Gog2003,
}

/// A known build of the game's executable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Build {
    /// `atlantis3-<medium>-<language>-<size>`.
    pub id: &'static str,
    /// The edition, for people, without the language: one `Game.exe`
    /// serves every language ([`crate::install::edition_label`] adds it).
    pub label: &'static str,
    pub size: u64,
    /// The PE header's `TimeDateStamp`.
    pub pe_timestamp: u32,
    /// MD5 of the whole file.
    pub md5: [u8; 16],
    /// The executable's file name in the install.
    pub exe_name: &'static str,
    /// The release of its code.
    pub release: Release,
}

/// The builds this port knows. GOG.com's English, French, German and
/// Spanish installs ship the first (the "1.0 w10" build, 2022); the
/// French retail CD (three CDs, 2001) installs the second.
pub const KNOWN_BUILDS: [Build; 2] = [
    Build {
        id: "atlantis3-gog-fr-3473408",
        label: "Atlantis III, GOG.com",
        size: 3_473_408,
        // 2003-04-24 09:50:26 UTC.
        pe_timestamp: 0x3ea7_b362,
        md5: [
            0xeb, 0x06, 0xea, 0xa7, 0x18, 0x87, 0x7d, 0xfc, 0xab, 0xee, 0x6e, 0x8f, 0x89, 0xd8,
            0x5b, 0x31,
        ],
        exe_name: "Game.exe",
        release: Release::Gog2003,
    },
    Build {
        id: "atlantis3-cd-fr-3379200",
        label: "Atlantis III, CD",
        size: 3_379_200,
        // 2001-09-11 20:15:37 UTC.
        pe_timestamp: 0x3b9e_70e9,
        md5: [
            0xf8, 0x1d, 0xd8, 0xd2, 0xe4, 0x90, 0x03, 0xb2, 0xba, 0x53, 0xb4, 0x8b, 0x4a, 0x21,
            0xc2, 0xec,
        ],
        exe_name: "Atlantis3.exe",
        release: Release::Cd2001,
    },
];

impl Release {
    /// Whether `CHANGECD=n` runs the disc test (the CD's program) rather
    /// than storing the variable (GOG's, patched).
    pub fn disc_test(self) -> bool {
        self == Release::Cd2001
    }

    /// The scarab case's sprite bank (`AtlScarabee_MainInit`,
    /// @ 0x00432200; atlantis3-cd-fr-3379200 @ 0x00431be0). The French
    /// and Spanish data name the file `Scarabée.SPR`, the English and
    /// German `Scarabee.SPR`.
    pub fn scarab_bank(self) -> &'static str {
        match self {
            Release::Cd2001 => "Scarab\u{e9}e.spr",
            Release::Gog2003 => "Scarabee.spr",
        }
    }
}

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

/// The file name of GOG.com's executable.
pub const EXE_NAME: &str = "Game.exe";

/// The executables' names, in the order a folder is searched: GOG's
/// `Game.exe`, then the CD's `Atlantis3.exe` (in GOG's folder that name
/// is the launcher).
pub const EXE_NAMES: [&str; 2] = [EXE_NAME, "Atlantis3.exe"];

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
        assert_eq!(
            hex(&KNOWN_BUILDS[1].md5),
            "f81dd8d2e49003b2ba53b48b4a21c2ec"
        );
        for b in KNOWN_BUILDS {
            assert!(EXE_NAMES.contains(&b.exe_name), "{}", b.id);
        }
        assert!(Release::Cd2001.disc_test() && !Release::Gog2003.disc_test());
        assert_eq!(Release::Cd2001.scarab_bank(), "Scarabée.spr");
        assert_eq!(hex(&SPR_DLLS[0].md5), "72266c25b968fb5eaf21d41851027d7b");
        assert_eq!(hex(&SPR_DLLS[1].md5), "94cfb7cfaf577a3aeb94914e7769d552");
    }

    #[test]
    fn rejections() {
        assert_eq!(identify(b"MZ"), Err(ExeError::NotPe));
        match identify(&pe(0x3b9e_70e9, 3_379_200)) {
            Err(ExeError::Unknown {
                size, pe_timestamp, ..
            }) => assert_eq!((size, pe_timestamp), (3_379_200, 0x3b9e_70e9)),
            other => panic!("{other:?}"),
        }
        match identify(&pe(0x3ea7_b362, 3_473_408)) {
            Err(ExeError::Unknown {
                size, pe_timestamp, ..
            }) => assert_eq!((size, pe_timestamp), (3_473_408, 0x3ea7_b362)),
            other => panic!("{other:?}"),
        }
        assert_eq!(identify_spr_dll(&pe(0x3b95_f1d7, 118_784)), None);
    }
}

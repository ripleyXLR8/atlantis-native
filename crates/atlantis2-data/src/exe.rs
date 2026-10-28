//! Which build of the game's executable a folder holds.
//!
//! A build is recognised by its size and PE timestamp, and confirmed by the
//! MD5 of the whole file. Only the builds checked against their own code
//! are listed: another build is reported with its identifiers, so that it
//! can be added once compared.
//!
//! Three builds are known:
//!
//! - the French CD's `atlantis2.exe` (1.00, version string 3.15), the
//!   reference every address in the comments belongs to;
//! - the official 1.01 patch's `atlantis2.exe` (version 3.18, built
//!   1999-11-17, one executable for the French and the English releases),
//!   which a CD installation may hold;
//! - GOG.com's `Game.exe`: the 1.01 executable with 18 byte patches of
//!   GOG's (the install folder used as the CD, no free-space or CD
//!   check, no `WM_FONTCHANGE` broadcast, fonts without antialiasing,
//!   videos ended by their frame count only, input and window fixes for
//!   current Windows). GOG's folder also holds an `atlantis2.exe`, a
//!   splash-screen launcher that starts `Game.exe` ([`LAUNCHER_SIZE`]).
//!   GOG ships this one `Game.exe` (build "1.0 w10 hotfix", 2022-07-13)
//!   for its English, French, German and Spanish installations; its
//!   older builds' `Game.exe` are told apart ([`OLDER_GOG`]) but not
//!   played yet.
//!
//! 1.01 moved most of the code and data (the linker order changed), so
//! what the port reads from the executable is read at each build's own
//! addresses ([`Build::place_sounds`]). What 1.01 changes in the game's
//! behaviour is in [`Release`].

use md5::{Digest, Md5};

/// The release of the game's code a build is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Release {
    /// 1.00, the CD's (version string 3.15).
    V100,
    /// 1.01, the official patch's (version string 3.18). Compared with
    /// 1.00: `Font.txt`'s `[CharacterSet]` section (`SBCS`, the default,
    /// or `MBCS`), which decides whether a byte with the high bit set is
    /// a lead byte (in 1.00 it always is: an accented letter is glued to
    /// the next character in the subtitles); a `[Font]` block's `Height`
    /// is a cell height (`CreateFontA(+Height)`), no longer points; the
    /// line step is `GetTextMetricsA`'s `tmHeight` + 1, no longer
    /// `Height`; a missing font file falls back to a system font; Alt+Enter
    /// no longer switches between full screen and window.
    V101,
}

/// Where the place sounds' listener table lies in a build (virtual
/// addresses): the place names (16 bytes each, up to an empty one), the
/// rows (20 bytes: place, view, x, y, z) and the rows' end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaceSoundTable {
    pub names: u32,
    pub rows: u32,
    pub end: u32,
}

/// A known build of the game's executable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Build {
    /// `atlantis2-<medium>-<language>-<size>`.
    pub id: &'static str,
    /// The edition, for people.
    pub label: &'static str,
    pub size: u64,
    /// The PE header's `TimeDateStamp`.
    pub pe_timestamp: u32,
    /// MD5 of the whole file.
    pub md5: [u8; 16],
    /// The version string the game writes to its log at start-up
    /// (@ 0x005e3128, used by @ 0x0049d350).
    pub version: &'static str,
    /// The release of its code.
    pub release: Release,
    /// GOG.com's patches of the 1.01 executable (`Game.exe`).
    pub gog: bool,
    /// The place sounds' listener table (@ 0x005dd2c0 in the reference).
    pub place_sounds: PlaceSoundTable,
}

/// The listener table of 1.01 (and GOG): the same 5780 bytes as the
/// reference's, 0xf60 lower.
const PLACE_SOUNDS_101: PlaceSoundTable = PlaceSoundTable {
    names: 0x005d_c360,
    rows: 0x005d_c410,
    end: 0x005d_d9f4,
};

/// The builds this port knows.
pub const KNOWN_BUILDS: [Build; 3] = [
    Build {
        id: "atlantis2-cd-fr-2199552",
        label: "Atlantis II, French CD (1999)",
        size: 2_199_552,
        // 1999-10-12 10:10:46 UTC.
        pe_timestamp: 0x3803_0926,
        md5: [
            0x5e, 0x21, 0x8f, 0x7f, 0xa9, 0x75, 0x45, 0x32, 0xae, 0x1d, 0x7a, 0x42, 0x75, 0x44,
            0x3f, 0xef,
        ],
        version: "Atlantis 2 - Version 3.15 - 8/10/1999",
        release: Release::V100,
        gog: false,
        place_sounds: PlaceSoundTable {
            names: 0x005d_d2c0,
            rows: 0x005d_d370,
            end: 0x005d_e954,
        },
    },
    Build {
        id: "atlantis2-patch101-2183168",
        label: "Atlantis II, patch 1.01 (1999)",
        size: 2_183_168,
        // 1999-11-17 11:17:37 UTC.
        pe_timestamp: 0x3832_8ed1,
        md5: [
            0x25, 0x53, 0x10, 0xe7, 0xd6, 0xbd, 0x7f, 0x82, 0x3f, 0x68, 0x0e, 0x2b, 0xd3, 0xbe,
            0x8b, 0x7f,
        ],
        version: "Atlantis 2 - Version 3.18 - 8/10/1999",
        release: Release::V101,
        gog: false,
        place_sounds: PLACE_SOUNDS_101,
    },
    Build {
        id: "atlantis2-gog-fr-2183168",
        label: "Atlantis II, GOG.com (patch 1.01)",
        size: 2_183_168,
        pe_timestamp: 0x3832_8ed1,
        md5: [
            0x25, 0x4e, 0xcd, 0x84, 0x04, 0x6d, 0x75, 0xc2, 0x0a, 0x68, 0x5e, 0x7d, 0x7f, 0x91,
            0xd1, 0xd8,
        ],
        version: "Atlantis 2 - Version 3.18 - 8/10/1999",
        release: Release::V101,
        gog: true,
        place_sounds: PLACE_SOUNDS_101,
    },
];

/// GOG.com's `Game.exe` of its builds before "1.0 w10 hotfix"
/// (2022-07-13), whose `Game.exe` is [`KNOWN_BUILDS`]' last: 2,183,168
/// bytes each, the same file in every language (GOG's build manifests).
/// Their code has not been compared yet, so they are not played; the
/// player is told to update the game. MD5, GOG's name of the build, its
/// date.
pub const OLDER_GOG: [([u8; 16], &str, &str); 2] = [
    (
        [
            0xf8, 0x34, 0x0e, 0x5a, 0x01, 0x1f, 0xd2, 0x20, 0x7c, 0x5b, 0xb7, 0x8b, 0x4c, 0x2a,
            0x96, 0x36,
        ],
        "1.0",
        "2018-05-04",
    ),
    (
        [
            0x8c, 0x24, 0xad, 0x43, 0x7d, 0x9e, 0x99, 0x4f, 0x65, 0x7a, 0xd7, 0x75, 0x1b, 0x62,
            0x67, 0x18,
        ],
        "1.0 w10",
        "2022-06-21",
    ),
];

/// The size of GOG's `atlantis2.exe`, the DotEmu splash-screen launcher
/// that starts `Game.exe` (20,992 bytes, PE timestamp 0x4c19e4d8).
pub const LAUNCHER_SIZE: u64 = 20_992;

/// The file names the game's executable goes by: the CD's and the
/// patch's `atlantis2.exe`, GOG's `Game.exe`.
pub const EXE_NAMES: [&str; 2] = ["atlantis2.exe", "Game.exe"];

/// Why an executable is not a known build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExeError {
    /// Not a PE executable.
    NotPe,
    /// GOG.com's `Game.exe` of an older build ([`OLDER_GOG`]): GOG's
    /// name of the build and its date.
    OlderGog {
        version: &'static str,
        date: &'static str,
    },
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
            ExeError::OlderGog { version, date } => write!(
                f,
                "the game's executable is GOG.com's build \"{version}\" of {date}, not supported \
                 yet: update the game with GOG Galaxy, or install it again from GOG.com's \
                 current installer (build \"1.0 w10 hotfix\" of 2022-07-13)"
            ),
            ExeError::Unknown {
                size,
                pe_timestamp,
                md5,
            } => write!(
                f,
                "the game's executable is not a known build of Atlantis II ({size} bytes, PE timestamp {pe_timestamp:#010x}, MD5 {md5})"
            ),
        }
    }
}

impl std::error::Error for ExeError {}

/// The PE header's `TimeDateStamp`, `None` when `exe` is not a PE file.
pub fn pe_timestamp(exe: &[u8]) -> Option<u32> {
    let pe = u32::from_le_bytes(exe.get(0x3c..0x40)?.try_into().ok()?) as usize;
    if exe.get(..2)? != b"MZ" || exe.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    Some(u32::from_le_bytes(
        exe.get(pe + 8..pe + 12)?.try_into().ok()?,
    ))
}

/// `len` bytes of `exe` at virtual address `va` (the image base plus the
/// section's address), through the section table; `None` outside the
/// sections' file data.
pub fn bytes_at(exe: &[u8], va: u32, len: usize) -> Option<&[u8]> {
    let u16_at = |o: usize| Some(u16::from_le_bytes(exe.get(o..o + 2)?.try_into().ok()?));
    let u32_at = |o: usize| Some(u32::from_le_bytes(exe.get(o..o + 4)?.try_into().ok()?));
    let pe = u32_at(0x3c)? as usize;
    pe_timestamp(exe)?;
    let sections = usize::from(u16_at(pe + 6)?);
    let optional = usize::from(u16_at(pe + 20)?);
    let base = u32_at(pe + 24 + 28)?;
    let rva = va.checked_sub(base)?;
    let table = pe + 24 + optional;
    for i in 0..sections {
        let s = table + 40 * i;
        let (vsize, vaddr) = (u32_at(s + 8)?, u32_at(s + 12)?);
        let (rsize, raw) = (u32_at(s + 16)?, u32_at(s + 20)?);
        if rva >= vaddr && rva < vaddr + vsize.max(rsize) {
            let off = (raw + rva - vaddr) as usize;
            if (rva - vaddr) as usize + len > rsize as usize {
                return None;
            }
            return exe.get(off..off + len);
        }
    }
    None
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Which known build `exe` is (size, PE timestamp, then MD5).
pub fn identify(exe: &[u8]) -> Result<&'static Build, ExeError> {
    let stamp = pe_timestamp(exe).ok_or(ExeError::NotPe)?;
    let size = exe.len() as u64;
    let candidates: Vec<&Build> = KNOWN_BUILDS
        .iter()
        .filter(|b| b.size == size && b.pe_timestamp == stamp)
        .collect();
    let md5: [u8; 16] = Md5::digest(exe).into();
    if let Some(b) = candidates.into_iter().find(|b| b.md5 == md5) {
        return Ok(b);
    }
    if let Some(&(_, version, date)) = OLDER_GOG.iter().find(|(m, _, _)| *m == md5) {
        return Err(ExeError::OlderGog { version, date });
    }
    Err(ExeError::Unknown {
        size,
        pe_timestamp: stamp,
        md5: hex(&md5),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PE header with a timestamp.
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
        let md5s: Vec<String> = KNOWN_BUILDS.iter().map(|b| hex(&b.md5)).collect();
        assert_eq!(
            md5s,
            [
                "5e218f7fa9754532ae1d7a4275443fef",
                "255310e7d6bd7f823f680e2bd3be8b7f",
                "254ecd84046d75c20a685e7d7f91d1d8",
            ]
        );
        // The listener table is 5780 bytes in every build.
        for b in &KNOWN_BUILDS {
            assert_eq!(b.place_sounds.end - b.place_sounds.names, 5780, "{}", b.id);
            assert_eq!(b.place_sounds.rows - b.place_sounds.names, 0xb0, "{}", b.id);
        }
        assert_eq!(
            KNOWN_BUILDS
                .iter()
                .filter(|b| b.release == Release::V101)
                .count(),
            2
        );
        let older: Vec<String> = OLDER_GOG.iter().map(|(m, _, _)| hex(m)).collect();
        assert_eq!(
            older,
            [
                "f8340e5a011fd2207c5bb78b4c2a9636",
                "8c24ad437d9e994f657ad7751b626718",
            ]
        );
    }

    #[test]
    fn timestamps_and_rejections() {
        assert_eq!(pe_timestamp(&pe(0x3803_0926, 0x100)), Some(0x3803_0926));
        assert_eq!(pe_timestamp(b"not an exe"), None);
        assert_eq!(identify(b"MZ"), Err(ExeError::NotPe));
        // Right size and timestamp, wrong content: unknown, with its MD5.
        let fake = pe(0x3803_0926, 2_199_552);
        match identify(&fake) {
            Err(ExeError::Unknown {
                size, pe_timestamp, ..
            }) => {
                assert_eq!((size, pe_timestamp), (2_199_552, 0x3803_0926));
            }
            other => panic!("{other:?}"),
        }
    }
}

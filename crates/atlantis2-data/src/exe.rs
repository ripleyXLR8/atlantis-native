//! Which build of `atlantis2.exe` a folder holds.
//!
//! A build is recognised by its size and PE timestamp, and confirmed by the
//! MD5 of the whole file. Only the builds checked against their own code
//! are listed: another build is reported with its identifiers, so that it
//! can be added once compared.

use md5::{Digest, Md5};

/// A known build of `atlantis2.exe`.
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
}

/// The builds this port knows.
pub const KNOWN_BUILDS: [Build; 1] = [Build {
    id: "atlantis2-cd-fr-2199552",
    label: "Atlantis II, French CD (1999)",
    size: 2_199_552,
    // 1999-10-12 10:10:46 UTC.
    pe_timestamp: 0x3803_0926,
    md5: [
        0x5e, 0x21, 0x8f, 0x7f, 0xa9, 0x75, 0x45, 0x32, 0xae, 0x1d, 0x7a, 0x42, 0x75, 0x44, 0x3f,
        0xef,
    ],
    version: "Atlantis 2 - Version 3.15 - 8/10/1999",
}];

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
            ExeError::NotPe => write!(f, "atlantis2.exe is not a Windows executable"),
            ExeError::Unknown {
                size,
                pe_timestamp,
                md5,
            } => write!(
                f,
                "atlantis2.exe is not a known build ({size} bytes, PE timestamp {pe_timestamp:#010x}, MD5 {md5})"
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
    candidates
        .into_iter()
        .find(|b| b.md5 == md5)
        .ok_or(ExeError::Unknown {
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

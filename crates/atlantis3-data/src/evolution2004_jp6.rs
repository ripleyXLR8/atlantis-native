//! The panoramas (`warp\*.jp6`) of *Atlantis Evolution*'s 2004 program: a
//! container of six cube faces, read by the image class whose vtable is at
//! 0x00699ee0 of atlantis4-cd-en-3022848 (atlantis4-re
//! `formats/cd2004-jp6.md`).
//!
//! The open (@ 0x00664800) reads the version (the dword at 4). Version
//! 0x20000 or 0x20001 (@ 0x00664ce0): a 0x6e-byte header,
//!
//! | Offset | Field |
//! |---|---|
//! | 0 | `JP6\0` (3 bytes compared, 0x006d9228) |
//! | 4 | version, at most 0x20001 |
//! | 8 | u16 width, u16 height of a face |
//! | 0xc | u16 (1560 in every file) [UNKNOWN] |
//! | 0xe | six entries of 16 bytes: u32 kinds, u32 offset, u32 size, u32 face id |
//!
//! then the six entries are put in the order 2, 0, 5, 1, 3, 4 of the file's
//! (the swaps @ 0x00664a9f..0x00664cd2). A face (@ 0x00664d80) is a run of
//! chunks `tag[4], u32 size, data[size - 4]` from its entry's offset (with
//! version bit 0 set; @ 0x00664e60 walks them, the next chunk at
//! `4 + size` from the last): `JPG\0` (0x006d922c) a JPEG picture, `ZBF\0`
//! (0x006d9230) a depth layer, `AL\0` (0x006d9234) an alpha layer, pairs
//! `(count - 1, value)` each written to the alpha byte of `count` pixels.
//! Version 0x10000 (a 0x28-byte header with five offsets) is read too
//! (@ 0x00664870..0x00664a9a) but no file of the CD has it, nor `ZBF` or
//! `AL` chunks [CONFIRMED-DATA]; this module reads version 0x2000x only.

/// The magic.
pub const MAGIC: &[u8; 4] = b"JP6\0";

/// One chunk of a face.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    pub tag: [u8; 4],
    pub data: &'a [u8],
}

/// One face of the cube.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face<'a> {
    /// The entry's first dword (0x10 in every file) [UNKNOWN].
    pub kinds: u32,
    /// The face id, `0x40 | 1 << k` (0x41 .. 0x60).
    pub id: u32,
    pub chunks: Vec<Chunk<'a>>,
}

impl Face<'_> {
    /// The face's JPEG picture.
    pub fn jpeg(&self) -> Option<&[u8]> {
        self.chunks
            .iter()
            .find(|c| &c.tag == b"JPG\0")
            .map(|c| c.data)
    }
}

/// A panorama.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Jp6<'a> {
    pub version: u32,
    /// A face's size in pixels.
    pub width: u16,
    pub height: u16,
    /// The header's u16 at 0xc.
    pub unknown_c: u16,
    /// The faces in the program's order (file entries 2, 0, 5, 1, 3, 4).
    pub faces: Vec<Face<'a>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Jp6Error {
    NotJp6,
    /// A version the program refuses (above 0x20001) or this module does
    /// not read (0x10000).
    Version(u32),
    Truncated,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

/// The cube slot each face of [`Jp6::faces`] goes to: the `jp6` branch of
/// the cube loader (@ 0x0042d8b0, called from @ 0x0042cbe0 when the warp
/// file's extension is `jp6`) copies face i into the cube's i-th 0x80-byte
/// face object, in the open's order; the slots are *Atlantis III*'s (its
/// HNM frames `bk dn ft lf rt up` go to slots 1, 3, 0, 4, 5, 2,
/// `atlantis2_engine::warp::FRAME_TO_FACE`): front, back, up, down, left,
/// right. The CD's faces 2 are skies and 3 grounds [CONFIRMED-DATA: looked
/// at].
pub const SLOT_NAMES: [&str; 6] = ["ft", "bk", "up", "dn", "lf", "rt"];

/// The order of the entries after the open's swaps.
pub const FACE_ORDER: [usize; 6] = [2, 0, 5, 1, 3, 4];

impl<'a> Jp6<'a> {
    pub fn parse(d: &'a [u8]) -> Result<Jp6<'a>, Jp6Error> {
        if d.get(..3) != Some(&MAGIC[..3]) {
            return Err(Jp6Error::NotJp6);
        }
        let version = u32_at(d, 4).ok_or(Jp6Error::Truncated)?;
        if !(0x20000..=0x20001).contains(&version) {
            return Err(Jp6Error::Version(version));
        }
        if d.len() < 0x6e {
            return Err(Jp6Error::Truncated);
        }
        let width = u16_at(d, 8).ok_or(Jp6Error::Truncated)?;
        let height = u16_at(d, 10).ok_or(Jp6Error::Truncated)?;
        let unknown_c = u16_at(d, 12).ok_or(Jp6Error::Truncated)?;
        let mut entries = Vec::with_capacity(6);
        for i in 0..6 {
            let at = 0xe + 16 * i;
            let e = (
                u32_at(d, at).ok_or(Jp6Error::Truncated)?,
                u32_at(d, at + 4).ok_or(Jp6Error::Truncated)? as usize,
                u32_at(d, at + 8).ok_or(Jp6Error::Truncated)? as usize,
                u32_at(d, at + 12).ok_or(Jp6Error::Truncated)?,
            );
            entries.push(e);
        }
        let mut faces = Vec::with_capacity(6);
        for &i in &FACE_ORDER {
            let (kinds, offset, size, id) = entries[i];
            let mut chunks = Vec::new();
            if version & 1 != 0 {
                let mut walked = 0usize;
                while walked < size {
                    let at = offset + walked;
                    let tag: [u8; 4] = d
                        .get(at..at + 4)
                        .ok_or(Jp6Error::Truncated)?
                        .try_into()
                        .map_err(|_| Jp6Error::Truncated)?;
                    let n = u32_at(d, at + 4).ok_or(Jp6Error::Truncated)? as usize;
                    let data = d
                        .get(at + 8..(at + 4 + n).max(at + 8))
                        .ok_or(Jp6Error::Truncated)?;
                    chunks.push(Chunk { tag, data });
                    walked += 4 + n;
                    if n == 0 {
                        break;
                    }
                }
            }
            faces.push(Face { kinds, id, chunks });
        }
        Ok(Jp6 {
            version,
            width,
            height,
            unknown_c,
            faces,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut d = vec![0u8; 0x6e];
        d[..4].copy_from_slice(MAGIC);
        d[4..8].copy_from_slice(&0x20001u32.to_le_bytes());
        d[8..10].copy_from_slice(&768u16.to_le_bytes());
        d[10..12].copy_from_slice(&768u16.to_le_bytes());
        for i in 0..6 {
            let off = d.len();
            let jpg = vec![0xff, 0xd8, i as u8];
            d.extend_from_slice(b"JPG\0");
            d.extend_from_slice(&(4 + jpg.len() as u32).to_le_bytes());
            d.extend_from_slice(&jpg);
            let at = 0xe + 16 * i;
            d[at..at + 4].copy_from_slice(&0x10u32.to_le_bytes());
            d[at + 4..at + 8].copy_from_slice(&(off as u32).to_le_bytes());
            d[at + 8..at + 12].copy_from_slice(&(4 + 4 + jpg.len() as u32).to_le_bytes());
            d[at + 12..at + 16].copy_from_slice(&(0x40u32 | 1 << i).to_le_bytes());
        }
        d
    }

    #[test]
    fn faces_in_the_programs_order() {
        let d = sample();
        let p = Jp6::parse(&d).unwrap();
        let ids: Vec<u32> = p.faces.iter().map(|f| f.id).collect();
        assert_eq!(ids, [0x44, 0x41, 0x60, 0x42, 0x48, 0x50]);
        assert_eq!(p.faces[0].jpeg(), Some(&[0xff, 0xd8, 2][..]));
        let mut bad = d.clone();
        bad[4..8].copy_from_slice(&0x10000u32.to_le_bytes());
        assert_eq!(Jp6::parse(&bad), Err(Jp6Error::Version(0x10000)));
    }

    /// Every panorama of the CD (`ATLANTIS4_CD`): six faces, one JPEG each,
    /// starting with a JPEG start-of-image.
    #[test]
    fn the_cds_panoramas() {
        let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
            crate::testing::missing("ATLANTIS4_CD not set");
            return;
        };
        let mut n = 0;
        for disc in 1..=4 {
            let warp = dir.join(format!("cd{disc}/datas/warp"));
            let Ok(rd) = std::fs::read_dir(&warp) else {
                continue;
            };
            for e in rd.flatten() {
                let d = std::fs::read(e.path()).unwrap();
                let p = Jp6::parse(&d).unwrap_or_else(|err| panic!("{:?}: {err:?}", e.path()));
                assert_eq!((p.width, p.height), (768, 768));
                assert_eq!(p.faces.len(), 6);
                for f in &p.faces {
                    assert_eq!(f.chunks.len(), 1);
                    assert!(f.jpeg().unwrap().starts_with(&[0xff, 0xd8]));
                }
                n += 1;
            }
        }
        assert_eq!(n, 506);
    }
}

//! "BigFile 1.00" archives (BIGCD1..4.BIG), read as cryo.dll's
//! `_GL_OpenBigFile` / `_GL_OpenFileBigFile` do.
//!
//! The full game spans four discs; mounting is additive and the first copy
//! of a name wins (`_GL_OpenFileBigFile` takes the first match). The GOG
//! release ships its files unpacked next to BIGCD1..4.BIG stubs of 14 sounds:
//! with `loose` set, a name the archives lack is read from the game
//! directory.
//!
//! The original opens one archive at a time, the inserted disc's
//! (`_GL_CloseBigFile` then `_GL_OpenBigFile` on `bigcd1.big` + 13 x disc:
//! atlantis-cd-en-714240 FUN_0041eda8, atlantis-cd-nl-688128 FUN_0041e46c).
//! A member several discs carry is kept per disc too, so that a reader can
//! ask for the copy of a given disc ([`BigArchive::set_disc_copy`]): the
//! Dutch CD's disc 2 holds an older revision of some texts and fonts.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::files::FileSource;

#[derive(Clone, Copy, Debug)]
struct Entry {
    adr: u32, // data offset from the end of the directory
    size: u32,
    disc: usize,
}

pub struct BigArchive {
    src: Arc<dyn FileSource>,
    discs: Vec<(String, u32)>, // file name, data base (directory length)
    entries: HashMap<String, Entry>,
    /// The members found in more than one archive: the first copy of each
    /// archive, in mount order.
    copies: HashMap<String, Vec<Entry>>,
    /// The disc (1..4) whose copy reads take, 0 for the first copy.
    disc_copy: AtomicU8,
    loose: bool,
}

/// Archive key: upper case, backslash separated (`WAM\ATLAN1.WAM`).
pub fn key(name: &str) -> String {
    name.replace('/', "\\").to_ascii_uppercase()
}

impl BigArchive {
    pub fn new(src: Arc<dyn FileSource>) -> BigArchive {
        BigArchive {
            src,
            discs: Vec::new(),
            entries: HashMap::new(),
            copies: HashMap::new(),
            disc_copy: AtomicU8::new(0),
            loose: false,
        }
    }

    /// The player's files the archive reads (the game directory).
    pub fn source(&self) -> &Arc<dyn FileSource> {
        &self.src
    }

    /// Mounts one archive. False when it is missing or not a BigFile.
    pub fn mount(&mut self, file: &str) -> bool {
        let Some(size) = self.src.size(file) else {
            return false;
        };
        let Some(head) = self.src.read_at(file, 0, 32) else {
            return false;
        };
        // "BigFile 1.00" then 4 more magic bytes; the file count, data length
        // and header length fields are only printed by _GL_OpenBigFile.
        if &head[..12] != b"BigFile 1.00" {
            return false;
        }
        let disc = self.discs.len();
        // Directory entries, eight u32 fields then the name, until an entry
        // whose "next" field is 0. The data starts after the directory, whose
        // length is recomputed as 0x20 + sum(nameLen + 0x20) (FUN_00415dac).
        let mut pos: u64 = 32;
        let mut header_len: u32 = 0x20;
        while let Some(rec) = self.src.read_at(file, pos, 32) {
            let f = |i: usize| u32::from_le_bytes(rec[i * 4..i * 4 + 4].try_into().unwrap());
            let (name_len, entry_size, flags, adr, next) = (f(0), f(2), f(5), f(6), f(7));
            pos += 32;
            if u64::from(name_len) > size {
                break;
            }
            let Some(raw) = self.src.read_at(file, pos, name_len as usize) else {
                break;
            };
            pos += u64::from(name_len);
            // The name ends at its first NUL; names compare upper case
            // (FUN_0041d0d9 upper-cases a-z of both sides).
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            let name = String::from_utf8_lossy(&raw[..end]).to_ascii_uppercase();
            header_len = header_len.wrapping_add(name_len + 0x20);
            if flags & 0x1000 != 0 {
                // LZW-packed members are not used by the shipped archives.
                log::warn!("{name}: LZW-packed member not supported");
            }
            let entry = Entry {
                adr,
                size: entry_size,
                disc,
            };
            match self.entries.get(&name) {
                None => {
                    self.entries.insert(name, entry);
                }
                // A further archive's copy (the same archive's duplicates
                // are skipped: the first match wins within one archive).
                Some(first) if first.disc != disc => {
                    let list = self.copies.entry(name).or_insert_with(|| vec![*first]);
                    if list.last().is_none_or(|e| e.disc != disc) {
                        list.push(entry);
                    }
                }
                Some(_) => {}
            }
            if next == 0 {
                break;
            }
        }
        self.discs.push((file.to_string(), header_len));
        true
    }

    /// Reads names the archives lack from the game directory (GOG release).
    pub fn set_loose(&mut self, loose: bool) {
        self.loose = loose;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, name: &str) -> bool {
        let k = key(name);
        self.entries.contains_key(&k) || (self.loose && self.src.size(&k).is_some())
    }

    /// Reads of a member that several discs carry take disc `disc`'s copy
    /// (1..4, the archive BIGCD<disc>.BIG) when it has one; `None` restores
    /// the first copy (the lowest disc's).
    pub fn set_disc_copy(&self, disc: Option<u8>) {
        self.disc_copy.store(disc.unwrap_or(0), Ordering::Relaxed);
    }

    /// The archive index of disc `disc` (BIGCD<disc>.BIG).
    fn archive_of_disc(&self, disc: u8) -> Option<usize> {
        let want = format!("BIGCD{disc}.BIG");
        self.discs
            .iter()
            .position(|(file, _)| file.eq_ignore_ascii_case(&want))
    }

    /// The entry reads use: the chosen disc's copy, else the first.
    fn entry(&self, k: &str) -> Option<&Entry> {
        let first = self.entries.get(k)?;
        let disc = self.disc_copy.load(Ordering::Relaxed);
        if disc == 0 {
            return Some(first);
        }
        let copy = self.copies.get(k).and_then(|list| {
            let archive = self.archive_of_disc(disc)?;
            list.iter().find(|e| e.disc == archive)
        });
        Some(copy.unwrap_or(first))
    }

    /// The discs (1..4) that carry a member, in mount order.
    pub fn discs_of(&self, name: &str) -> Vec<u8> {
        let k = key(name);
        let disc_number = |e: &Entry| -> Option<u8> {
            let file = self.discs[e.disc].0.to_ascii_uppercase();
            let n = file.strip_prefix("BIGCD")?.strip_suffix(".BIG")?;
            n.parse().ok()
        };
        match (self.copies.get(&k), self.entries.get(&k)) {
            (Some(list), _) => list.iter().filter_map(disc_number).collect(),
            (None, Some(e)) => disc_number(e).into_iter().collect(),
            (None, None) => Vec::new(),
        }
    }

    /// The bytes of a member.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let k = key(name);
        match self.entry(&k) {
            Some(e) => {
                let (file, base) = &self.discs[e.disc];
                self.src
                    .read_at(file, u64::from(*base) + u64::from(e.adr), e.size as usize)
            }
            None if self.loose => self.src.read(&k),
            None => None,
        }
    }

    /// Where a member's bytes are: (file, offset, size). For the web build,
    /// which fetches that range before reading it.
    pub fn locate(&self, name: &str) -> Option<(String, u64, usize)> {
        let k = key(name);
        match self.entry(&k) {
            Some(e) => {
                let (file, base) = &self.discs[e.disc];
                Some((
                    file.clone(),
                    u64::from(*base) + u64::from(e.adr),
                    e.size as usize,
                ))
            }
            None if self.loose => Some((k.clone(), 0, usize::try_from(self.src.size(&k)?).ok()?)),
            None => None,
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemFiles;

    /// A BigFile of `members` (name, bytes), stored in order.
    fn bigfile(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut head = b"BigFile 1.00\0\0\0\0".to_vec();
        head.resize(32, 0);
        let mut dir = Vec::new();
        let mut data = Vec::new();
        for (i, (name, bytes)) in members.iter().enumerate() {
            let mut n = name.as_bytes().to_vec();
            n.push(0);
            let next = u32::from(i + 1 < members.len());
            let fields = [
                n.len() as u32,
                0,
                bytes.len() as u32,
                0,
                0,
                0,
                data.len() as u32,
                next,
            ];
            for f in fields {
                dir.extend_from_slice(&f.to_le_bytes());
            }
            dir.extend_from_slice(&n);
            data.extend_from_slice(bytes);
        }
        [head, dir, data].concat()
    }

    fn archive() -> BigArchive {
        let mut files = MemFiles::new();
        files.insert("BIGCD1.BIG", bigfile(&[("A.TXT", b"one"), ("B.TXT", b"b")]));
        files.insert(
            "BIGCD2.BIG",
            bigfile(&[("A.TXT", b"two"), ("A.TXT", b"dup")]),
        );
        files.insert("BIGCD3.BIG", bigfile(&[("C.TXT", b"c")]));
        let mut big = BigArchive::new(Arc::new(files));
        for n in 1..=3 {
            assert!(big.mount(&format!("BIGCD{n}.BIG")));
        }
        big
    }

    #[test]
    fn first_copy_unless_a_disc_is_chosen() {
        let big = archive();
        assert_eq!(big.read("a.txt").unwrap(), b"one");
        assert_eq!(big.discs_of("A.TXT"), [1, 2]);
        assert_eq!(big.discs_of("C.TXT"), [3]);
        big.set_disc_copy(Some(2));
        // Disc 2's first copy (not its duplicate); the members disc 2
        // lacks are read as before.
        assert_eq!(big.read("A.TXT").unwrap(), b"two");
        assert_eq!(big.read("B.TXT").unwrap(), b"b");
        assert_eq!(big.read("C.TXT").unwrap(), b"c");
        let (file, _, size) = big.locate("A.TXT").unwrap();
        assert_eq!((file.as_str(), size), ("BIGCD2.BIG", 3));
        big.set_disc_copy(None);
        assert_eq!(big.read("A.TXT").unwrap(), b"one");
    }
}

//! "BigFile 1.00" archives (BIGCD1..4.BIG), read as cryo.dll's
//! `_GL_OpenBigFile` / `_GL_OpenFileBigFile` do.
//!
//! The full game spans four discs; mounting is additive and the first copy
//! of a name wins (`_GL_OpenFileBigFile` takes the first match). The GOG
//! release ships its files unpacked next to BIGCD1..4.BIG stubs of 14 sounds:
//! with `loose` set, a name the archives lack is read from the game
//! directory.

use std::collections::HashMap;
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
            loose: false,
        }
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
            self.entries.entry(name).or_insert(Entry {
                adr,
                size: entry_size,
                disc,
            });
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

    /// The bytes of a member.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let k = key(name);
        match self.entries.get(&k) {
            Some(e) => {
                let (file, base) = &self.discs[e.disc];
                self.src
                    .read_at(file, u64::from(*base) + u64::from(e.adr), e.size as usize)
            }
            None if self.loose => self.src.read(&k),
            None => None,
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

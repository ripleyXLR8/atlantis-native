//! The characters' conversation subjects (@ 0x006c2e88), what the subject
//! menu offers when a character is clicked.
//!
//! The table is kept as the original's memory: 256 records of 0xc4 bytes,
//! each a count then 16 entries of 12 bytes {i32 value, i32 index, u8 flag,
//! 3 bytes}. The value is the subject's `#SUJETS#` parameter (what `SUJET`
//! is set to, and the subject's frame in the world's `suj*.spr`), the index
//! its `#SUJETS#` index, the flag 1 when the subject is enabled. The table
//! is saved as it is (entry 34 of the save).
//!
//! The keys change it in place, as the original's code does: `ENABLESUJ`
//! (@ 0x0047f920) re-enables the first entry with the value or appends one
//! after the count (no limit: a 17th entry is written over the next
//! character's record); `DISABLESUJ` (@ 0x0047f9f0) clears the flag of the
//! first entry with the value; `KILLSUJ` (@ 0x0047fa70) removes the first
//! one by moving the entries after it down one place, the one past the end
//! included (so the last place keeps what lay past the list); `CLEARSUJ`
//! sets the count to 0 and leaves the entries. What lies past a list is
//! read by the subject menu when a click falls on an empty cell
//! ([`SubjectTable::entry`]).

/// One subject of a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subject {
    /// The subject's `#SUJETS#` parameter, what `SUJET` is set to.
    pub value: i32,
    /// Its `#SUJETS#` index.
    pub index: i32,
    /// 1 when enabled; the menu draws any other value at 50 %.
    pub flag: u8,
}

impl Subject {
    pub fn enabled(&self) -> bool {
        self.flag == 1
    }
}

/// The subject table (@ 0x006c2e88).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubjectTable {
    bytes: Vec<u8>,
}

impl Default for SubjectTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SubjectTable {
    /// Characters in the table (the size the save keeps).
    pub const CHARACTERS: usize = 256;
    /// Bytes per character.
    pub const RECORD: usize = 0xc4;
    /// Entries per character.
    pub const ENTRIES: usize = 16;
    const ENTRY: usize = 12;

    /// An empty table (the program's zeroed data).
    pub fn new() -> SubjectTable {
        SubjectTable {
            bytes: vec![0; Self::CHARACTERS * Self::RECORD],
        }
    }

    /// *Atlantis III*'s reset (atlantis3-gog-fr-3473408 @ 0x0043bbc0, at
    /// start-up, a new game and a load): every count 0 and every entry
    /// {value −1, index −1, flag 1}, so that a click past a list answers
    /// −1.
    pub fn reset_a3(&mut self) {
        for c in 0..Self::CHARACTERS as i32 {
            let base = Self::base(c).unwrap();
            self.set_i32(base, 0);
            for i in 0..Self::ENTRIES {
                self.set_entry(
                    c,
                    i,
                    Subject {
                        value: -1,
                        index: -1,
                        flag: 1,
                    },
                );
            }
        }
    }

    /// Writes entry `i` of a character (within the table).
    pub fn set_entry(&mut self, character: i32, i: usize, s: Subject) {
        let Some(at) = Self::entry_at(character, i) else {
            return;
        };
        self.set_i32(at, s.value);
        self.set_i32(at + 4, s.index);
        if let Some(b) = self.bytes.get_mut(at + 8) {
            *b = s.flag;
        }
    }

    /// Sets a character's count.
    pub fn set_count(&mut self, character: i32, n: i32) {
        if let Some(base) = Self::base(character) {
            self.set_i32(base, n);
        }
    }

    /// The table's bytes, as the save keeps them.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The table from the save's bytes (missing bytes zero).
    pub fn from_bytes(data: &[u8]) -> SubjectTable {
        let mut t = SubjectTable::new();
        let n = data.len().min(t.bytes.len());
        t.bytes[..n].copy_from_slice(&data[..n]);
        t
    }

    fn base(character: i32) -> Option<usize> {
        usize::try_from(character)
            .ok()
            .filter(|&c| c < Self::CHARACTERS)
            .map(|c| c * Self::RECORD)
    }

    fn i32_at(&self, at: usize) -> i32 {
        self.bytes
            .get(at..at + 4)
            .map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    fn set_i32(&mut self, at: usize, v: i32) {
        if let Some(b) = self.bytes.get_mut(at..at + 4) {
            b.copy_from_slice(&v.to_le_bytes());
        }
    }

    fn entry_at(character: i32, i: usize) -> Option<usize> {
        Some(Self::base(character)? + 4 + Self::ENTRY * i)
    }

    /// The count of a character's list (@ +0).
    pub fn count(&self, character: i32) -> i32 {
        Self::base(character).map_or(0, |b| self.i32_at(b))
    }

    /// Entry `i` of a character as the memory holds it, past the count too
    /// (an index past 15 reads the next records; past the table, zeros).
    pub fn entry(&self, character: i32, i: usize) -> Subject {
        let Some(at) = Self::entry_at(character, i) else {
            return Subject {
                value: 0,
                index: 0,
                flag: 0,
            };
        };
        Subject {
            value: self.i32_at(at),
            index: self.i32_at(at + 4),
            flag: self.bytes.get(at + 8).copied().unwrap_or(0),
        }
    }

    /// The entries `0..count` of a character.
    pub fn list(&self, character: i32) -> Vec<Subject> {
        let n = self.count(character).max(0) as usize;
        (0..n).map(|i| self.entry(character, i)).collect()
    }

    /// The characters whose count is not 0.
    pub fn characters(&self) -> impl Iterator<Item = i32> + '_ {
        (0..Self::CHARACTERS as i32).filter(|&c| self.count(c) != 0)
    }

    fn find(&self, character: i32, value: i32) -> Option<usize> {
        let n = self.count(character).max(0) as usize;
        (0..n).find(|&i| self.entry(character, i).value == value)
    }

    /// `ENABLESUJ` of one subject (@ 0x0047f920): the first entry with the
    /// value gets flag 1, else an entry {value, index, 1} is written after
    /// the count and the count grows (the caller looks the index up only
    /// for a new entry, as the original).
    pub fn enable(&mut self, character: i32, value: i32, index: i32) {
        let Some(base) = Self::base(character) else {
            return;
        };
        if let Some(i) = self.find(character, value) {
            let at = Self::entry_at(character, i).unwrap() + 8;
            if let Some(b) = self.bytes.get_mut(at) {
                *b = 1;
            }
            return;
        }
        let n = self.count(character);
        if n >= Self::ENTRIES as i32 {
            log::warn!("ENABLESUJ: entry {n} of character {character}, past its record");
        }
        let at = base + 4 + Self::ENTRY * n.max(0) as usize;
        self.set_i32(at, value);
        self.set_i32(at + 4, index);
        if let Some(b) = self.bytes.get_mut(at + 8) {
            *b = 1;
        }
        self.set_i32(base, n + 1);
    }

    /// `DISABLESUJ` of one subject (@ 0x0047f9f0): the first entry with the
    /// value gets flag 0.
    pub fn disable(&mut self, character: i32, value: i32) {
        if let Some(i) = self.find(character, value) {
            let at = Self::entry_at(character, i).unwrap() + 8;
            self.bytes[at] = 0;
        }
    }

    /// `KILLSUJ` of one subject (@ 0x0047fa70): the first entry with the
    /// value removed, the entries after it (and the one past the end)
    /// moved down, the count one less.
    pub fn kill(&mut self, character: i32, value: i32) {
        let Some(i) = self.find(character, value) else {
            return;
        };
        let base = Self::base(character).unwrap();
        let n = self.count(character).max(0) as usize;
        for j in i..n {
            let to = base + 4 + Self::ENTRY * j;
            let from = to + Self::ENTRY;
            if from + Self::ENTRY <= self.bytes.len() {
                self.bytes.copy_within(from..from + Self::ENTRY, to);
            }
        }
        self.set_i32(base, n as i32 - 1);
    }

    /// `CLEARSUJ`: the count set to 0, the entries left.
    pub fn clear(&mut self, character: i32) {
        if let Some(base) = Self::base(character) {
            self.set_i32(base, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_change_the_memory_as_the_original() {
        let mut t = SubjectTable::new();
        t.enable(1, 10, 46);
        t.enable(1, 12, 52);
        t.enable(1, 4, 47);
        assert_eq!(t.count(1), 3);
        t.disable(1, 12);
        assert_eq!(t.entry(1, 1).flag, 0);
        t.enable(1, 12, 99);
        assert_eq!(
            t.entry(1, 1),
            Subject {
                value: 12,
                index: 52,
                flag: 1
            }
        );
        // KILLSUJ moves the entries down, the one past the end included.
        t.kill(1, 10);
        assert_eq!(t.count(1), 2);
        assert_eq!(t.entry(1, 0).value, 12);
        assert_eq!(t.entry(1, 1).value, 4);
        assert_eq!(t.entry(1, 2).value, 0);
        // CLEARSUJ keeps the entries: a new list starts over them, and
        // a removal moves the stale one past its end into it.
        t.clear(1);
        assert_eq!(t.count(1), 0);
        t.enable(1, 7, 3);
        assert_eq!(
            t.list(1),
            [Subject {
                value: 7,
                index: 3,
                flag: 1
            }]
        );
        assert_eq!(t.entry(1, 1).value, 4);
        t.kill(1, 7);
        assert_eq!(t.entry(1, 0).value, 4);
        // Entry 16 is the next character's count.
        t.enable(2, 9, 1);
        assert_eq!(t.entry(1, 16).value, 1);
    }
}

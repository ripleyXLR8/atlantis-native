//! The original game's save files: the player list `players.atl` and the
//! chapter checkpoints `p<letter><chapter>.gam`, and the episode names of
//! `SPRLIST\EPI.TXT` the load screen shows.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`saveload_format.cpp`;
//! GPLv3, the ScummVM team), which reads the files `atlantis.exe` writes.
//! The writers are `FUN_0042d6b8` (checkpoint and player list, on every
//! chapter change), `FUN_0041dea4` (player created) and `FUN_0041e8a8`
//! (player deleted); the readers the start-up code at
//! 0x0041916b..0x004192e0 (player list) and `FUN_0042de5c` (checkpoint).

/// The rows of the player list.
pub const MAX_PLAYERS: usize = 5;
/// A player name's bytes, its NUL included (16 characters at most).
pub const NAME_LEN: usize = 17;
/// The longest name typed on the player screen (`FUN_0041dea4`).
pub const NAME_MAX_CHARS: usize = 16;
/// `players.atl` of the English builds: 5 names, 6 chapter bytes, 6 letters.
pub const PLAYERS_SIZE: usize = 97;
/// A 95-byte `players.atl`: 5 chapter bytes and 5 letters, written by the
/// 1997 retail builds (the English one's writers FUN_0041d6d0,
/// FUN_0041e030 and FUN_0042c164 of atlantis-cd-en-688128 write 0x55 + 5
/// + 5 bytes; the French file has the same size).
pub const PLAYERS_SIZE_FR: usize = 95;
/// A 114-byte `players.atl`: a names block of 0x66 = 102 bytes (the five
/// rows and the 17 bytes after them, 0x00486608..0x00486618), 6 chapters, 6
/// letters, read and written by the Chinese builds (atlantis-cd-zh-tw-696832
/// WinMain @ 0x00419212..0x00419262; writers FUN_0041dd14 @ 0x0041e574,
/// FUN_0041e65c @ 0x0041e9b0, FUN_0042cb1c @ 0x0042d147).
pub const PLAYERS_SIZE_ZH: usize = 114;
/// A checkpoint: a raw dump of 25 globals, 2639 bytes.
pub const CHECKPOINT_SIZE: usize = 2639;
/// The inventory records a checkpoint holds (0x10e bytes of 9).
pub const CHECKPOINT_RECORDS: usize = 30;
/// The `/con` play counters (0x5dc bytes).
pub const CON_COUNTERS: usize = 0x5dc;
/// The guest records a checkpoint holds (0x14 bytes of 5).
pub const CHECKPOINT_GUESTS: usize = 4;
/// The hidden-perso ids a checkpoint holds (0x200 bytes of int16).
pub const CHECKPOINT_HIDDEN: usize = 256;

/// The player list (the globals 0x0049657c names, 0x006ce246 resume
/// chapters, 0x006ce24c save letters, 0x00496606 row count).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Players {
    /// The name rows as the original keeps them: NUL-terminated, the bytes
    /// after the NUL stale (a name is strcpy'd over the previous one).
    pub rows: [[u8; NAME_LEN]; MAX_PLAYERS],
    /// The resume chapter of each player (index 5 unused, kept).
    pub chapters: [u8; 6],
    /// The letter of each player's files (index 5 unused, kept).
    pub letters: [u8; 6],
    /// The rows of the list: the players, then the "new player" row while
    /// fewer than 5 players exist.
    pub count: usize,
    /// The row marker of the build (0x00496570, e.g. `#New Player`).
    pub marker: Vec<u8>,
    /// The 1997 builds' list: a 95-byte file, and a deletion leaves the
    /// rows after the last one as they were (atlantis-cd-en-688128
    /// FUN_0041e030).
    pub layout_1997: bool,
    /// The Chinese builds' list ([`PLAYERS_SIZE_ZH`]): the 17 bytes after
    /// the five rows, which only the file and the deletion's shift read
    /// (row 4 = strcpy of them, atlantis-cd-zh-tw-696832 FUN_0041e65c @
    /// 0x0041e6f2..0x0041e70b, 0x0041e80c..0x0041e825); `None` for the
    /// 85-byte names block.
    pub sixth_row: Option<[u8; NAME_LEN]>,
}

fn cstr(b: &[u8]) -> &[u8] {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    &b[..n]
}

fn strcpy(row: &mut [u8; NAME_LEN], s: &[u8]) {
    let n = s.len().min(NAME_LEN - 1);
    row[..n].copy_from_slice(&s[..n]);
    row[n] = 0;
}

impl Players {
    /// No `players.atl`: one row, the marker; the letters 'a'..'e'
    /// (0x004191a2, before the file is read).
    pub fn new(marker: &[u8]) -> Players {
        let mut p = Players {
            rows: [[0; NAME_LEN]; MAX_PLAYERS],
            chapters: [0; 6],
            letters: [b'a', b'b', b'c', b'd', b'e', 0],
            count: 1,
            marker: marker.to_vec(),
            layout_1997: false,
            sixth_row: None,
        };
        strcpy(&mut p.rows[0], marker);
        p
    }

    /// Reads a `players.atl` (97, 95 or 114 bytes) with the row rule of
    /// the 1999 and later builds (see [`Players::read_rows`]); `None` for
    /// another size.
    pub fn read(data: &[u8], marker: &[u8]) -> Option<Players> {
        Players::read_rows(data, marker, true)
    }

    /// Reads a `players.atl` with a build's row rule: the rows end at the
    /// first name that is the marker or, with `empty_name_ends_rows`, empty
    /// (the reference's reader @ 0x0041928f..0x004192cf, a second strcmp
    /// against ""; also the 1999 and Chinese builds). The 1997 builds'
    /// readers test the marker only (atlantis-cd-en-688128 @
    /// 0x00419194..0x004191b1, atlantis-cd-fr-687104 @
    /// 0x00418da7..0x00418dc4, the same loop in the other 1997 builds), so
    /// a row with an empty name before the marker is a player's row.
    pub fn read_rows(data: &[u8], marker: &[u8], empty_name_ends_rows: bool) -> Option<Players> {
        let mut p = Players::new(marker);
        let (n, block) = match data.len() {
            PLAYERS_SIZE => (6, 0x55),
            PLAYERS_SIZE_FR => (5, 0x55),
            PLAYERS_SIZE_ZH => (6, 0x66),
            _ => return None,
        };
        let letter_base = block + n;
        for (i, row) in p.rows.iter_mut().enumerate() {
            row.copy_from_slice(&data[NAME_LEN * i..NAME_LEN * (i + 1)]);
        }
        if block == 0x66 {
            let mut extra = [0u8; NAME_LEN];
            extra.copy_from_slice(&data[0x55..0x66]);
            p.sixth_row = Some(extra);
        }
        p.chapters = [0; 6];
        p.chapters[..n].copy_from_slice(&data[block..block + n]);
        p.letters = [0; 6];
        p.letters[..n].copy_from_slice(&data[letter_base..letter_base + n]);
        p.count = p.count_rows(empty_name_ends_rows);
        Some(p)
    }

    /// The row count rule of the reader (0x00419275..0x004192e0): the index
    /// of the first name that is the marker (or empty, with
    /// `empty_name_ends_rows`), plus the marker row when fewer than 5.
    fn count_rows(&self, empty_name_ends_rows: bool) -> usize {
        let n = (0..MAX_PLAYERS)
            .find(|&i| {
                let name = self.name(i);
                (empty_name_ends_rows && name.is_empty()) || name == self.marker.as_slice()
            })
            .unwrap_or(MAX_PLAYERS);
        if n < MAX_PLAYERS {
            n + 1
        } else {
            n
        }
    }

    /// The file the writers write: names (0x55 bytes), chapters (6),
    /// letters (6); 5 and 5 for the 1997 layout; names 0x66 bytes with the
    /// Chinese one.
    pub fn to_bytes(&self) -> Vec<u8> {
        let n = if self.layout_1997 { 5 } else { 6 };
        let mut out = Vec::with_capacity(PLAYERS_SIZE_ZH);
        for row in &self.rows {
            out.extend_from_slice(row);
        }
        if let Some(extra) = &self.sixth_row {
            out.extend_from_slice(extra);
        }
        out.extend_from_slice(&self.chapters[..n]);
        out.extend_from_slice(&self.letters[..n]);
        out
    }

    /// The name of row `i`, without its NUL.
    pub fn name(&self, i: usize) -> &[u8] {
        cstr(&self.rows[i])
    }

    /// Whether row `i` is the "new player" row.
    pub fn is_new_row(&self, i: usize) -> bool {
        self.name(i) == self.marker.as_slice()
    }

    /// The players (the rows other than the "new player" row).
    pub fn player_count(&self) -> usize {
        (0..self.count).filter(|&i| !self.is_new_row(i)).count()
    }

    /// `FUN_0041dea4`, a typed name committed: it replaces the "new player"
    /// row, and the marker row is appended when fewer than 5 rows exist.
    /// Returns the new player's row, `None` when there is no free row or
    /// the name is refused (its first byte ' ', '.' or none).
    pub fn create(&mut self, name: &[u8]) -> Option<usize> {
        if matches!(name.first(), None | Some(b' ' | b'.')) {
            return None;
        }
        let row = self.count.checked_sub(1)?;
        if !self.is_new_row(row) {
            return None;
        }
        Some(self.store_in_last_row(name))
    }

    /// `FUN_0041dea4`'s store of a typed name (the Enter commit @
    /// 0x0041e3ee..0x0041e436, the exit commit @ 0x0041e710..0x0041e758):
    /// `strcpy` into the last row, whatever it holds, then the marker
    /// appended when fewer than 5 rows exist. Returns the row.
    pub fn store_in_last_row(&mut self, name: &[u8]) -> usize {
        let row = self.count.saturating_sub(1);
        strcpy(&mut self.rows[row], name);
        if self.count < MAX_PLAYERS {
            let marker = self.marker.clone();
            strcpy(&mut self.rows[self.count], &marker);
            self.count += 1;
        }
        row
    }

    /// `FUN_0041e8a8`: the rows after `row` move up one slot (names,
    /// chapters and letters); the deleted player's letter goes to slot 4,
    /// its chapter there becomes 0. With five players the last row becomes
    /// the marker and the count stays 5; otherwise the rows from the new
    /// count on are emptied (the 1997 layout empties the new count's row
    /// only: atlantis-cd-en-688128 FUN_0041e030 has no loop over the
    /// others). The Chinese builds' list ([`Players::sixth_row`]) shifts
    /// the 17 bytes after the rows into row 4 and, without five players,
    /// writes the rows with `strcpy` (see the branch). Returns the deleted
    /// player's letter.
    pub fn delete(&mut self, row: usize) -> Option<u8> {
        if row >= self.count || self.is_new_row(row) {
            return None;
        }
        let letter = self.letters[row];
        // The five-player test reads row 4 before the shift
        // (atlantis-cd-zh-tw-696832 FUN_0041e65c @ 0x0041e6bb..0x0041e6d5);
        // the other layouts' shift leaves row 4 as it is, so the test
        // reads the same row there.
        let five = self.count == MAX_PLAYERS && !self.is_new_row(MAX_PLAYERS - 1);
        for i in row..MAX_PLAYERS {
            if i + 1 < MAX_PLAYERS {
                self.rows[i] = self.rows[i + 1];
            } else if let Some(extra) = self.sixth_row {
                strcpy(&mut self.rows[i], cstr(&extra));
            }
            self.chapters[i] = self.chapters[i + 1];
            self.letters[i] = self.letters[i + 1];
        }
        self.letters[4] = letter;
        self.chapters[4] = 0;
        if five {
            let marker = self.marker.clone();
            strcpy(&mut self.rows[MAX_PLAYERS - 1], &marker);
        } else if self.sixth_row.is_some() {
            // atlantis-cd-zh-tw-696832 FUN_0041e65c: row 4 = "" @ 0x0041e736;
            // the old count's last row = the marker when the count is 5
            // and that row (row 4, just emptied) is not the marker, else ""
            // @ 0x0041e754..0x0041e7b4; the rows from the old count on =
            // "" @ 0x0041e7b9..0x0041e7e8; the count drops @ 0x0041e7ea.
            // With four players and the marker in row 4, row 4 keeps the
            // marker past the new count.
            strcpy(&mut self.rows[MAX_PLAYERS - 1], b"");
            let last = self.count - 1;
            let marker = self.marker.clone();
            let s: &[u8] = if self.count == MAX_PLAYERS && !self.is_new_row(last) {
                &marker
            } else {
                b""
            };
            strcpy(&mut self.rows[last], s);
            for r in self.count..MAX_PLAYERS {
                strcpy(&mut self.rows[r], b"");
            }
            self.count -= 1;
        } else {
            self.count -= 1;
            let end = if self.layout_1997 {
                self.count + 1
            } else {
                MAX_PLAYERS
            };
            for r in self.count..end {
                self.rows[r] = [0; NAME_LEN];
            }
        }
        Some(letter)
    }

    /// The checkpoint file of player `row` for `chapter` (the writer's
    /// path @ 0x0042d7db..0x0042d86b).
    pub fn checkpoint_name(&self, row: usize, chapter: u8) -> String {
        checkpoint_name(self.letters[row], chapter)
    }
}

/// `p<letter><chapter>.gam`, the chapter in decimal.
pub fn checkpoint_name(letter: u8, chapter: u8) -> String {
    format!("p{}{}.gam", char::from(letter), chapter)
}

/// The chapter of a checkpoint file name `p<letter><N>.gam` of `letter`,
/// `None` for another file.
pub fn checkpoint_chapter(name: &str, letter: u8) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    let rest = lower.strip_prefix('p')?;
    let rest = rest.strip_prefix(char::from(letter.to_ascii_lowercase()))?;
    let digits = rest.strip_suffix(".gam")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// One inventory record of a checkpoint: {sprite, id, present}.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InventoryRecord {
    pub sprite: u8,
    pub id: i32,
    pub present: i32,
}

/// A chapter checkpoint `p<L><N>.gam`: the 25 globals `FUN_0042d6b8`
/// writes, in its order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    /// 0x000: the path of the current WAM (`wam\atlan4.wam`), 0x104 bytes.
    pub wam_path: Vec<u8>,
    /// 0x104: the chapter (0x00772909).
    pub chapter: u8,
    /// 0x105: the previous chapter (0x00772907).
    pub prev_chapter: u8,
    /// 0x106: the place bank (0x00496920).
    pub bank: i32,
    /// 0x10a / 0x10e: the place index and the previous one.
    pub place_index: i32,
    pub prev_place_index: i32,
    /// 0x112: the current place (0x006cd7c0).
    pub place_id: i32,
    /// 0x116: LastVue (0x0049692c).
    pub last_vue: i32,
    /// 0x11a: the requested place (0x006cd7b8).
    pub requested_vue: i32,
    /// 0x11e: the IncPerso count (0x0077290d).
    pub inc_perso: u8,
    /// 0x11f / 0x123: the inventory records (count, then 30 slots).
    pub inventory: Vec<InventoryRecord>,
    /// 0x231: the `/con` play counters.
    pub counters: Vec<u8>,
    /// 0x80d: the CON variables 0..6.
    pub vars: [i32; 7],
    /// 0x829 / 0x82a: the guests {id, sprite}, 4 records.
    pub guest_count: u8,
    pub guests: [(i32, u8); CHECKPOINT_GUESTS],
    /// 0x83e / 0x842: the hidden persos.
    pub hidden: Vec<i16>,
    /// 0xa42: SUBTITLES (0x004960a8, 1 = on).
    pub subtitles: i32,
    /// 0xa46: OMNI 3D mode (0x004960ac, 0 = MODE 1).
    pub omni3d: i32,
    /// 0xa4a..0xa4d: master, music, effects and voices volumes (0..127).
    pub volumes: [u8; 4],
    /// 0xa4e: the disc in use (0x00772913).
    pub disc: u8,
}

impl Default for Checkpoint {
    fn default() -> Self {
        Checkpoint {
            wam_path: Vec::new(),
            chapter: 0,
            prev_chapter: 0,
            bank: 0,
            place_index: 0,
            prev_place_index: 0,
            place_id: 0,
            last_vue: 0,
            requested_vue: 0,
            inc_perso: 0,
            inventory: Vec::new(),
            counters: vec![0; CON_COUNTERS],
            vars: [0; 7],
            guest_count: 0,
            guests: [(0, 0); CHECKPOINT_GUESTS],
            hidden: Vec::new(),
            subtitles: 1,
            omni3d: 0,
            volumes: [127; 4],
            disc: 1,
        }
    }
}

fn le32(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl Checkpoint {
    /// Reads a checkpoint; `None` unless it has the size of one.
    pub fn parse(b: &[u8]) -> Option<Checkpoint> {
        if b.len() != CHECKPOINT_SIZE {
            return None;
        }
        let count = le32(b, 0x11f).clamp(0, CHECKPOINT_RECORDS as i32) as usize;
        let inventory = (0..count)
            .map(|i| {
                let r = 0x123 + 9 * i;
                InventoryRecord {
                    sprite: b[r],
                    id: le32(b, r + 1),
                    present: le32(b, r + 5),
                }
            })
            .collect();
        let mut vars = [0; 7];
        for (i, v) in vars.iter_mut().enumerate() {
            *v = le32(b, 0x80d + 4 * i);
        }
        let mut guests = [(0, 0); CHECKPOINT_GUESTS];
        for (i, g) in guests.iter_mut().enumerate() {
            *g = (le32(b, 0x82a + 5 * i), b[0x82a + 5 * i + 4]);
        }
        let hidden_n = le32(b, 0x83e).clamp(0, CHECKPOINT_HIDDEN as i32) as usize;
        let hidden = (0..hidden_n)
            .map(|i| i16::from_le_bytes([b[0x842 + 2 * i], b[0x843 + 2 * i]]))
            .collect();
        Some(Checkpoint {
            wam_path: cstr(&b[..0x104]).to_vec(),
            chapter: b[0x104],
            prev_chapter: b[0x105],
            bank: le32(b, 0x106),
            place_index: le32(b, 0x10a),
            prev_place_index: le32(b, 0x10e),
            place_id: le32(b, 0x112),
            last_vue: le32(b, 0x116),
            requested_vue: le32(b, 0x11a),
            inc_perso: b[0x11e],
            inventory,
            counters: b[0x231..0x231 + CON_COUNTERS].to_vec(),
            vars,
            guest_count: b[0x829],
            guests,
            hidden,
            subtitles: le32(b, 0xa42),
            omni3d: le32(b, 0xa46),
            volumes: [b[0xa4a], b[0xa4b], b[0xa4c], b[0xa4d]],
            disc: b[0xa4e],
        })
    }

    /// The file `FUN_0042d6b8` writes (unused bytes zero).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = vec![0u8; CHECKPOINT_SIZE];
        let path = cstr(&self.wam_path);
        let n = path.len().min(0x103);
        b[..n].copy_from_slice(&path[..n]);
        b[0x104] = self.chapter;
        b[0x105] = self.prev_chapter;
        let put = |b: &mut [u8], at: usize, v: i32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut b, 0x106, self.bank);
        put(&mut b, 0x10a, self.place_index);
        put(&mut b, 0x10e, self.prev_place_index);
        put(&mut b, 0x112, self.place_id);
        put(&mut b, 0x116, self.last_vue);
        put(&mut b, 0x11a, self.requested_vue);
        b[0x11e] = self.inc_perso;
        let recs = &self.inventory[..self.inventory.len().min(CHECKPOINT_RECORDS)];
        put(&mut b, 0x11f, recs.len() as i32);
        for (i, r) in recs.iter().enumerate() {
            let at = 0x123 + 9 * i;
            b[at] = r.sprite;
            put(&mut b, at + 1, r.id);
            put(&mut b, at + 5, r.present);
        }
        let n = self.counters.len().min(CON_COUNTERS);
        b[0x231..0x231 + n].copy_from_slice(&self.counters[..n]);
        for (i, &v) in self.vars.iter().enumerate() {
            put(&mut b, 0x80d + 4 * i, v);
        }
        b[0x829] = self.guest_count;
        for (i, &(id, spr)) in self.guests.iter().enumerate() {
            put(&mut b, 0x82a + 5 * i, id);
            b[0x82a + 5 * i + 4] = spr;
        }
        let hidden = &self.hidden[..self.hidden.len().min(CHECKPOINT_HIDDEN)];
        put(&mut b, 0x83e, hidden.len() as i32);
        for (i, &h) in hidden.iter().enumerate() {
            b[0x842 + 2 * i..0x844 + 2 * i].copy_from_slice(&h.to_le_bytes());
        }
        put(&mut b, 0xa42, self.subtitles);
        put(&mut b, 0xa46, self.omni3d);
        b[0xa4a..0xa4e].copy_from_slice(&self.volumes);
        b[0xa4e] = self.disc;
        b
    }

    /// The WAM file name of the path (`wam\atlan4.wam` -> `ATLAN4.WAM`).
    pub fn wam_name(&self) -> String {
        let path: String = cstr(&self.wam_path)
            .iter()
            .map(|&c| char::from(c))
            .collect();
        path.rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_ascii_uppercase()
    }
}

/// The name of chapter `chapter`'s checkpoint as the load picker extracts
/// it from EPI.TXT (`FUN_0041cf78` @ 0x0041d3a1..0x0041d431): each CR
/// counts and skips the byte after it; the name runs to the next CR.
/// Chapter 1 has none (the original's scan would not end on EPI.TXT).
pub fn epi_episode_name(text: &[u8], chapter: u32) -> Vec<u8> {
    if chapter < 2 {
        return Vec::new();
    }
    let wanted = chapter - 1;
    let (mut count, mut i) = (0u32, 0usize);
    loop {
        if i >= text.len() {
            return Vec::new();
        }
        if text[i] == b'\r' {
            i += 1;
            count += 1;
        }
        i += 1;
        if count == wanted {
            break;
        }
    }
    let end = text[i.min(text.len())..]
        .iter()
        .position(|&c| c == b'\r')
        .map_or(text.len(), |n| i + n);
    text[i.min(text.len())..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: &[u8] = b"#New Player";

    /// players.atl written by the original `FUN_0042d6b8` (emulated) for
    /// the names Alice, Bob and the marker, resume chapters {3, 7} with
    /// player 0 saving chapter 12, letters "abcdef".
    fn original_players_atl() -> Vec<u8> {
        let mut b = vec![0u8; 97];
        for (i, n) in [&b"Alice"[..], b"Bob", MARKER].iter().enumerate() {
            b[17 * i..17 * i + n.len()].copy_from_slice(n);
        }
        b[85] = 12;
        b[86] = 7;
        b[91..97].copy_from_slice(b"abcdef");
        b
    }

    /// The row count with an empty name before the marker: names ALICE,
    /// "", BOB, the marker, chapters 3 0 7. The readers run under emulation
    /// (atlantis-cd-en-688128 and atlantis-cd-fr-687104 with their markers,
    /// 95 bytes; the reference with `#New Player`, 97 bytes): the 1997 ones
    /// count 4 rows (three players and the marker's), the reference 2
    /// (ALICE and the marker's row).
    #[test]
    fn rows_end_at_an_empty_name_in_the_later_builds_only() {
        for (marker, size, rule_2001) in [
            (&b"New Player"[..], PLAYERS_SIZE_FR, false),
            (b"NOUVEAU JOUEUR", PLAYERS_SIZE_FR, false),
            (MARKER, PLAYERS_SIZE, true),
        ] {
            let mut b = vec![0u8; size];
            b[..5].copy_from_slice(b"ALICE");
            b[34..37].copy_from_slice(b"BOB");
            b[51..51 + marker.len()].copy_from_slice(marker);
            b[85] = 3;
            b[87] = 7;
            let letters = if rule_2001 { 0x5b } else { 0x5a };
            b[letters..letters + 5].copy_from_slice(b"abcde");
            let p = Players::read_rows(&b, marker, rule_2001).unwrap();
            assert_eq!(p.count, if rule_2001 { 2 } else { 4 });
            assert_eq!(
                (p.name(0), p.name(1), p.name(2)),
                (&b"ALICE"[..], &b""[..], &b"BOB"[..])
            );
            assert_eq!((p.chapters[0], p.chapters[2]), (3, 7));
            assert_eq!(p.letters[2], b'c');
            // The 1997 file read with the later builds' rule stops at row 1.
            assert_eq!(Players::read_rows(&b, marker, true).unwrap().count, 2);
        }
    }

    #[test]
    fn players_atl_round_trip() {
        let data = original_players_atl();
        let p = Players::read(&data, MARKER).unwrap();
        assert_eq!(p.count, 3);
        assert_eq!(p.name(0), b"Alice");
        assert_eq!(p.chapters[0], 12);
        assert_eq!(p.letters[1], b'b');
        assert_eq!(p.checkpoint_name(0, p.chapters[0]), "pa12.gam");
        assert_eq!(checkpoint_name(b'c', 40), "pc40.gam");
        assert_eq!(p.to_bytes(), data);
    }

    #[test]
    fn create_and_delete() {
        let mut p = Players::new(MARKER);
        assert_eq!(p.count, 1);
        assert_eq!(p.create(b" x"), None);
        assert_eq!(p.create(b".x"), None);
        assert_eq!(p.create(b"Alice"), Some(0));
        assert_eq!(p.create(b"Bob"), Some(1));
        assert_eq!(p.create(b"Carol"), Some(2));
        assert_eq!(p.count, 4);
        assert!(p.is_new_row(3));
        // FUN_0041e8a8 on row 0 (emulated: names Bob, Carol, marker;
        // letters "bcdeaf"; 3 rows).
        p.letters = *b"abcdef";
        assert_eq!(p.delete(0), Some(b'a'));
        assert_eq!(p.name(0), b"Bob");
        assert_eq!(p.name(1), b"Carol");
        assert!(p.is_new_row(2));
        assert_eq!(p.name(3), b"");
        assert_eq!(&p.letters, b"bcdeaf");
        assert_eq!(p.count, 3);
        assert_eq!(p.delete(0), Some(b'b'));
        assert_eq!(&p.letters, b"cdeabf");
        assert_eq!(p.count, 2);
        // Five players: the last row becomes the marker, 5 rows stay.
        let mut q = Players::new(MARKER);
        for n in [&b"A"[..], b"B", b"C", b"D", b"E"] {
            q.create(n).unwrap();
        }
        assert_eq!((q.count, q.player_count()), (5, 5));
        assert_eq!(q.create(b"F"), None);
        q.delete(1);
        assert_eq!((q.count, q.player_count()), (5, 4));
        assert!(q.is_new_row(4));
    }

    /// The Chinese builds' 114-byte file: names block 0x66 bytes (the
    /// rows and 17 more), chapters 0x66..0x6c, letters 0x6c..0x72; the
    /// deletion's shift copies the 17 bytes' string into row 4.
    #[test]
    fn layout_114() {
        let marker = b"New Player";
        let mut b = vec![0u8; PLAYERS_SIZE_ZH];
        for (i, n) in [&b"Alice"[..], b"Bob", marker].iter().enumerate() {
            b[17 * i..17 * i + n.len()].copy_from_slice(n);
        }
        b[85..88].copy_from_slice(b"Zed");
        b[0x66] = 9;
        b[0x67] = 4;
        b[0x6c..0x72].copy_from_slice(b"abcdef");
        let mut p = Players::read(&b, marker).unwrap();
        assert_eq!((p.count, p.chapters[0], p.chapters[1]), (3, 9, 4));
        assert_eq!(&p.letters, b"abcdef");
        assert_eq!(p.sixth_row.map(|r| r[..3].to_vec()), Some(b"Zed".to_vec()));
        assert_eq!(p.to_bytes(), b);
        p.delete(0);
        assert_eq!(p.name(0), b"Bob");
        assert!(p.is_new_row(1));
        // Row 4 got "Zed" from the shift, then was emptied as a row past
        // the count.
        assert_eq!(p.name(4), b"");
        assert_eq!(p.to_bytes().len(), PLAYERS_SIZE_ZH);
        // Five players: row 4 = "Zed", then the marker over it.
        let mut q = Players::new(marker);
        q.sixth_row = Some(*b"Zed\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        for n in [&b"A"[..], b"B", b"C", b"D", b"E"] {
            q.create(n).unwrap();
        }
        q.delete(0);
        assert!(q.is_new_row(4));
        assert_eq!(q.name(3), b"E");
        // Four players and the marker in row 4: the test reads row 4
        // before the shift, so the count drops to 4 and row 4 keeps the
        // marker past it (FUN_0041e65c @ 0x0041e6bb, 0x0041e754..0x0041e7ea).
        let mut q = Players::new(marker);
        q.sixth_row = Some([0; NAME_LEN]);
        for n in [&b"A"[..], b"B", b"C", b"D"] {
            q.create(n).unwrap();
        }
        assert_eq!(q.count, 5);
        q.delete(0);
        assert_eq!(q.count, 4);
        assert_eq!(q.player_count(), 3);
        assert_eq!(q.name(2), b"D");
        assert!(q.is_new_row(3) && q.is_new_row(4));
        // A new name takes row 3; the marker is not left between players.
        assert_eq!(q.create(b"F"), Some(3));
        assert_eq!((q.count, q.player_count()), (5, 4));
        assert!(q.is_new_row(4));
    }

    /// The 1997 layout: 95 bytes (chapters 0x55..0x5a, letters
    /// 0x5a..0x5f), and a deletion empties the new count's row only.
    #[test]
    fn layout_1997() {
        let marker = b"New Player";
        let mut p = Players::new(marker);
        p.layout_1997 = true;
        p.create(b"ALICE").unwrap();
        p.chapters[0] = 3;
        let b = p.to_bytes();
        assert_eq!(b.len(), PLAYERS_SIZE_FR);
        assert_eq!(b[0x55], 3);
        assert_eq!(&b[0x5a..0x5f], b"abcde");
        let mut q = Players::read(&b, marker).unwrap();
        assert_eq!(
            (q.count, q.name(0), q.name(1)),
            (2, &b"ALICE"[..], &marker[..])
        );
        // Stale rows past the list stay where FUN_0041e030 leaves them.
        q.layout_1997 = true;
        q.rows[4][..3].copy_from_slice(b"OLD");
        q.delete(0).unwrap();
        assert_eq!(q.count, 1);
        assert!(q.is_new_row(0));
        assert_eq!(q.name(1), b"");
        assert_eq!((q.name(3), q.name(4)), (&b"OLD"[..], &b"OLD"[..]));
    }

    #[test]
    fn checkpoint_round_trip() {
        let mut cp = Checkpoint {
            wam_path: b"wam\\atlan4.wam".to_vec(),
            chapter: 12,
            prev_chapter: 11,
            place_id: 180,
            last_vue: 179,
            inc_perso: 2,
            inventory: vec![InventoryRecord {
                sprite: 0,
                id: 257,
                present: 1,
            }],
            vars: [12, 1, 0, 0, 0, 0, 3],
            guest_count: 1,
            hidden: vec![262, 279],
            disc: 2,
            ..Checkpoint::default()
        };
        cp.guests[0] = (587, 4);
        cp.counters[5] = 3;
        let b = cp.to_bytes();
        assert_eq!(b.len(), CHECKPOINT_SIZE);
        assert_eq!(Checkpoint::parse(&b).unwrap(), cp);
        assert_eq!(cp.wam_name(), "ATLAN4.WAM");
        assert_eq!(checkpoint_chapter("PA12.GAM", b'a'), Some(12));
        assert_eq!(checkpoint_chapter("pb12.gam", b'a'), None);
        assert_eq!(checkpoint_chapter("pa.gam", b'a'), None);
    }

    /// The layout of `FUN_0042d6b8`'s output (emulated with marker bytes in
    /// every field: field k byte j = (k + 1) * 16 + j; chapter 12; bank and
    /// place 0; the writer copies the old place index into field 6).
    #[test]
    fn checkpoint_field_layout() {
        const FIELDS: [usize; 25] = [
            0x104, 1, 1, 4, 4, 4, 4, 4, 4, 1, 4, 0x10e, 0x5dc, 0x1c, 1, 0x14, 4, 0x200, 4, 4, 1, 1,
            1, 1, 1,
        ];
        let mut offset = 0;
        let starts: Vec<usize> = FIELDS
            .iter()
            .map(|n| {
                let s = offset;
                offset += n;
                s
            })
            .collect();
        assert_eq!(offset, CHECKPOINT_SIZE);
        assert_eq!(
            starts,
            [
                0x000, 0x104, 0x105, 0x106, 0x10a, 0x10e, 0x112, 0x116, 0x11a, 0x11e, 0x11f, 0x123,
                0x231, 0x80d, 0x829, 0x82a, 0x83e, 0x842, 0xa42, 0xa46, 0xa4a, 0xa4b, 0xa4c, 0xa4d,
                0xa4e
            ]
        );
    }

    #[test]
    fn episode_names() {
        let epi = b"\r\nTwo\r\n\r\nFour \r\nFive\r\n/FIN\r\n";
        assert_eq!(epi_episode_name(epi, 1), b"");
        assert_eq!(epi_episode_name(epi, 2), b"Two");
        assert_eq!(epi_episode_name(epi, 3), b"");
        assert_eq!(epi_episode_name(epi, 4), b"Four ");
        assert_eq!(epi_episode_name(epi, 5), b"Five");
        assert_eq!(epi_episode_name(epi, 6), b"/FIN");
        let lf = b"\r\nTwo\nX\r\nThree\r\n/fin\r\n";
        assert_eq!(epi_episode_name(lf, 2), b"Two\nX");
        assert_eq!(epi_episode_name(lf, 3), b"Three");
    }
}

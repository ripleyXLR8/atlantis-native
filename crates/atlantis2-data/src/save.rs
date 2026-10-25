//! The original's saves and options: the `.SAV` files (`save\ATLpp_ss.SAV`,
//! the `ZapFile` container) and `atlant2.cfg`.
//!
//! - A save is 35 globals of the executable copied one after the other
//!   (the table @ 0x005dcf78: [`ENTRIES`]), 69288 bytes, no version and no
//!   checksum; its first 48 bytes are the header the load page reads
//!   (place, chapter, view, `SYSTEMTIME`, thumbnail). [`SaveData`] holds
//!   those bytes with an accessor per entry.
//! - The file is a `ZapFile` (@ 0x0047e600..0x0047ec1f): the 12 bytes
//!   `ZAPFILE 1.0\0`, 4 bytes nothing writes (the stack object's bytes
//!   after the copied string), the u32 size, then (literal, fill) run
//!   pairs ([`zap_encode`], [`zap_decode`]). A raw file of exactly 69288
//!   bytes is accepted too ([`read_save`]).
//! - `atlant2.cfg` is a raw dump of 11 option globals (the table
//!   @ 0x005df900), 35 bytes ([`Cfg`]).
//!
//! The encoder and decoder are transcribed from the original and checked
//! against it run under emulation (`tests/zapfile.rs`).

use std::fmt;

/// The `ZapFile` magic with its NUL (@ 0x005df5e0).
pub const ZAP_MAGIC: &[u8; 12] = b"ZAPFILE 1.0\0";

/// The size of a save (@ 0x00478dd0: the sum of the table's sizes).
pub const SAVE_SIZE: usize = 69288;

/// The header the load page reads (the first five entries).
pub const HEADER_SIZE: usize = 48;

/// The slots of a player (0..98).
pub const SLOTS: u8 = 99;

/// A fill run starts only on 10 equal bytes or more.
const MIN_RUN: usize = 10;

/// The save table (@ 0x005dcf78): each global's address in the original
/// and its size, in file order.
pub const ENTRIES: [(u32, usize); 35] = [
    (0x0067_2a1c, 20),    // 0 the place file's name (strncpy 19)
    (0x006d_1694, 4),     // 1 the chapter
    (0x0067_29d4, 4),     // 2 the view
    (0x0067_29e0, 16),    // 3 SYSTEMTIME of the save
    (0x0067_29d8, 4),     // 4 the thumbnail (`VignetteSave`)
    (0x006c_fe90, 1200),  // 5 the 300 script variables
    (0x006d_0578, 4096),  // 6 the 1024 flags
    (0x0067_1a10, 1080),  // 7 the bag, 90 entries of 12 bytes
    (0x0067_1a08, 4),     // 8 the bag's count
    (0x0067_29d0, 4),     // 9 camera yaw (f32, radians)
    (0x0067_2a30, 4),     // 10 camera pitch (f32, radians)
    (0x0067_2a44, 4),     // 11 the current music
    (0x006d_0444, 256),   // 12 the music list (`startmusik`)
    (0x006d_84fd, 1),     // 13 dowsing rod overlay
    (0x006d_8500, 4),     // 14 dowsing rod state
    (0x006d_84f4, 4),     // 15
    (0x005e_2efc, 4),     // 16
    (0x006a_fdd0, 12288), // 17 the show/hide table, 1024 x {id, shown, kind}
    (0x006c_2dfc, 4),     // 18 its count
    (0x0067_2a00, 4),     // 19 the cursor frame
    (0x006d_2684, 4),     // 20 three puzzle states (the Maya grids)
    (0x005e_1840, 17),    // 21
    (0x006d_2688, 4),     // 22
    (0x005e_1854, 17),    // 23
    (0x006d_268c, 4),     // 24
    (0x005e_1868, 17),    // 25
    (0x006d_84d0, 4),     // 26 the puzzle mode
    (0x005e_1340, 4),     // 27 the spider web's stage
    (0x006d_84e0, 4),     // 28 the turtle ring's pending turns
    (0x006d_84e4, 4),     // 29 the turtle ring's frame
    (0x005e_1a4c, 8),     // 30 the skull puzzle
    (0x005e_20dc, 15),    // 31 the ogham puzzle
    (0x006d_26b4, 4),     // 32 Brigid's cross
    (0x006c_fb54, 1),     // 33 the dialog-running flag
    (0x006c_2e88, 50176), // 34 the subjects, 256 characters x 196 bytes
];

/// The entries by role.
pub mod entry {
    pub const WAM: usize = 0;
    pub const CHAPTER: usize = 1;
    pub const VIEW: usize = 2;
    pub const TIME: usize = 3;
    pub const VIGNETTE: usize = 4;
    pub const VARS: usize = 5;
    pub const FLAGS: usize = 6;
    pub const BAG: usize = 7;
    pub const BAG_COUNT: usize = 8;
    pub const YAW: usize = 9;
    pub const PITCH: usize = 10;
    pub const MUSIC: usize = 11;
    pub const MUSIC_LIST: usize = 12;
    pub const SHOWN: usize = 17;
    pub const SHOWN_COUNT: usize = 18;
    pub const CURSOR: usize = 19;
    pub const DIALOG: usize = 33;
    pub const SUBJECTS: usize = 34;
}

/// The bag's entries in a save, and the pattern of those not used (the
/// bag's initialisation @ 0x00474810 fills the table with 0x63 bytes; a
/// removal @ 0x00475680 moves that pattern down, so the entries at and
/// above the count always hold it).
pub const BAG_ENTRIES: usize = 90;
pub const BAG_UNUSED: [u8; 12] = [0x63; 12];
/// The sprite byte of a removed or unused entry.
pub const BAG_NO_SPRITE: u8 = 99;

/// The show/hide table's rows and the subject table's characters and
/// entries per character.
pub const SHOWN_ROWS: usize = 1024;
pub const SUBJECT_CHARACTERS: usize = 256;
pub const SUBJECTS_PER_CHARACTER: usize = 16;
/// One character's record: a count, then 16 entries {value, index, flag,
/// 3 bytes}.
pub const SUBJECT_RECORD: usize = 196;

/// The offset of entry `k` in the save.
pub fn entry_offset(k: usize) -> usize {
    ENTRIES[..k].iter().map(|e| e.1).sum()
}

/// `GetLocalTime`'s `SYSTEMTIME`: year, month, day of week, day, hour,
/// minute, second, millisecond.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SystemTime {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milliseconds: u16,
}

impl SystemTime {
    pub fn from_bytes(b: &[u8]) -> SystemTime {
        let w = |i: usize| u16::from_le_bytes([b[2 * i], b[2 * i + 1]]);
        SystemTime {
            year: w(0),
            month: w(1),
            day_of_week: w(2),
            day: w(3),
            hour: w(4),
            minute: w(5),
            second: w(6),
            milliseconds: w(7),
        }
    }

    pub fn to_bytes(self) -> [u8; 16] {
        let mut out = [0u8; 16];
        let words = [
            self.year,
            self.month,
            self.day_of_week,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.milliseconds,
        ];
        for (i, w) in words.iter().enumerate() {
            out[2 * i..2 * i + 2].copy_from_slice(&w.to_le_bytes());
        }
        out
    }

    /// The time `secs` seconds after 1970-01-01 00:00 (the civil calendar,
    /// no time zone).
    pub fn from_unix(secs: i64, milliseconds: u16) -> SystemTime {
        let days = secs.div_euclid(86400);
        let rem = secs.rem_euclid(86400);
        // Howard Hinnant's civil_from_days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        SystemTime {
            year: year as u16,
            month: month as u16,
            // 1970-01-01 was a Thursday (4).
            day_of_week: (days + 4).rem_euclid(7) as u16,
            day: day as u16,
            hour: (rem / 3600) as u16,
            minute: (rem / 60 % 60) as u16,
            second: (rem % 60) as u16,
            milliseconds,
        }
    }

    /// FUN_0040cc40's order: year, month, day, hour, minute, second,
    /// millisecond (the day of the week is skipped).
    pub fn key(&self) -> [u16; 7] {
        [
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.milliseconds,
        ]
    }
}

/// The header of a save: what the load page lists (the second table
/// @ 0x005dd098 copies these five entries when a slot is scanned).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveHeader {
    /// The place file's name (up to its NUL).
    pub wam: Vec<u8>,
    pub chapter: i32,
    pub view: i32,
    pub time: SystemTime,
    /// The thumbnail: a frame of `spr\menu\save.spr`.
    pub vignette: i32,
}

impl SaveHeader {
    pub fn parse(b: &[u8]) -> Option<SaveHeader> {
        let b = b.get(..HEADER_SIZE)?;
        let i32_at = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let name = &b[..20];
        let end = name.iter().position(|&c| c == 0).unwrap_or(20);
        Some(SaveHeader {
            wam: name[..end].to_vec(),
            chapter: i32_at(20),
            view: i32_at(24),
            time: SystemTime::from_bytes(&b[28..44]),
            vignette: i32_at(44),
        })
    }
}

/// The 69288 bytes of a save, with an accessor per entry.
#[derive(Clone, PartialEq, Eq)]
pub struct SaveData {
    bytes: Vec<u8>,
}

impl fmt::Debug for SaveData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SaveData")
            .field("header", &SaveHeader::parse(&self.bytes))
            .finish()
    }
}

impl Default for SaveData {
    fn default() -> SaveData {
        SaveData {
            bytes: vec![0; SAVE_SIZE],
        }
    }
}

impl SaveData {
    /// The save of `bytes` (exactly [`SAVE_SIZE`] long).
    pub fn from_bytes(bytes: Vec<u8>) -> Option<SaveData> {
        (bytes.len() == SAVE_SIZE).then_some(SaveData { bytes })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn entry(&self, k: usize) -> &[u8] {
        let o = entry_offset(k);
        &self.bytes[o..o + ENTRIES[k].1]
    }

    pub fn entry_mut(&mut self, k: usize) -> &mut [u8] {
        let o = entry_offset(k);
        &mut self.bytes[o..o + ENTRIES[k].1]
    }

    /// A 4-byte entry as an `i32`.
    pub fn i32(&self, k: usize) -> i32 {
        i32::from_le_bytes(self.entry(k)[..4].try_into().unwrap())
    }

    pub fn set_i32(&mut self, k: usize, v: i32) {
        self.entry_mut(k)[..4].copy_from_slice(&v.to_le_bytes());
    }

    /// A 4-byte entry as an `f32`.
    pub fn f32(&self, k: usize) -> f32 {
        f32::from_bits(self.i32(k) as u32)
    }

    pub fn set_f32(&mut self, k: usize, v: f32) {
        self.set_i32(k, v.to_bits() as i32);
    }

    /// The `i`-th `i32` of an entry.
    pub fn i32_at(&self, k: usize, i: usize) -> i32 {
        i32::from_le_bytes(self.entry(k)[4 * i..4 * i + 4].try_into().unwrap())
    }

    pub fn set_i32_at(&mut self, k: usize, i: usize, v: i32) {
        self.entry_mut(k)[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn header(&self) -> SaveHeader {
        SaveHeader::parse(&self.bytes).expect("a save is longer than its header")
    }

    /// Entry 0 as FUN_00478eb0 fills it: `strncpy(entry, name, 19)` (the
    /// rest of the 19 bytes zeroed, the 20th left as it was).
    pub fn set_wam(&mut self, name: &[u8]) {
        let e = &mut self.entry_mut(entry::WAM)[..19];
        e.fill(0);
        let n = name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(name.len())
            .min(19);
        e[..n].copy_from_slice(&name[..n]);
    }

    pub fn set_time(&mut self, t: SystemTime) {
        self.entry_mut(entry::TIME).copy_from_slice(&t.to_bytes());
    }

    /// The `i`-th bag entry: sprite, the three filler bytes, id, world.
    pub fn bag_entry(&self, i: usize) -> &[u8] {
        &self.entry(entry::BAG)[12 * i..12 * i + 12]
    }

    pub fn bag_entry_mut(&mut self, i: usize) -> &mut [u8] {
        &mut self.entry_mut(entry::BAG)[12 * i..12 * i + 12]
    }

    /// Character `c`'s subject record.
    pub fn subject_record(&self, c: usize) -> &[u8] {
        &self.entry(entry::SUBJECTS)[SUBJECT_RECORD * c..SUBJECT_RECORD * (c + 1)]
    }

    pub fn subject_record_mut(&mut self, c: usize) -> &mut [u8] {
        &mut self.entry_mut(entry::SUBJECTS)[SUBJECT_RECORD * c..SUBJECT_RECORD * (c + 1)]
    }
}

/// `FUN_0047e940(p, end)`: the equal bytes from `p` (at least 1, counting
/// while `p + n < end`).
fn run_length(buf: &[u8], p: usize, end: usize) -> usize {
    let mut n = 1;
    while p + n < end && buf[p + n] == buf[p] {
        n += 1;
    }
    n
}

/// One run of a `ZapFile`: a literal (`true`) or a fill, and its length.
pub type Run = (bool, usize);

/// `ZapFile::Save` (@ 0x0047e7e0 with the literal run @ 0x0047e8d0, the
/// run length @ 0x0047e940 and the fill run @ 0x0047e970): the file the
/// original writes for `data`, and its runs.
///
/// A literal holds at least 10 bytes and ends where 10 or more equal
/// bytes start; a fill run follows every literal, even of length 0.
/// `past_end` is the byte after the save buffer in memory, which the
/// encoder writes as the last fill's byte when the data end inside a
/// literal (nothing reads it; Native writes 0); `header_tail` the 4 bytes
/// after the magic, which the original does not initialise.
pub fn zap_encode(data: &[u8], past_end: u8, header_tail: [u8; 4]) -> (Vec<u8>, Vec<Run>) {
    let size = data.len();
    let mut buf = data.to_vec();
    buf.push(past_end);
    let mut out = Vec::with_capacity(size + 64);
    out.extend_from_slice(ZAP_MAGIC);
    out.extend_from_slice(&header_tail);
    out.extend_from_slice(&(size as u32).to_le_bytes());
    let mut runs = Vec::new();
    let mut cur = 0;
    while cur < size {
        // The literal (FUN_0047e8d0).
        let mut u = cur + MIN_RUN;
        let mut n = if u <= size {
            run_length(&buf, u, size)
        } else {
            1
        };
        while n < MIN_RUN && u < size {
            u += 1;
            n = run_length(&buf, u, size);
        }
        u = u.min(size);
        out.extend_from_slice(&((u - cur) as u32).to_le_bytes());
        out.extend_from_slice(&buf[cur..u]);
        runs.push((true, u - cur));
        cur = u;
        // The fill (FUN_0047e970).
        let n = run_length(&buf, cur, size).min(size - cur);
        out.extend_from_slice(&(n as u32).to_le_bytes());
        out.push(buf[cur]);
        runs.push((false, n));
        cur += n;
    }
    (out, runs)
}

/// The stored size of a `ZapFile` (`OpenAndReadHeader` @ 0x0047e9d0,
/// FUN_0047eaa0): `None` when the file is shorter than its header or its
/// magic differs (a `strcmp`, so the bytes after the NUL are not read).
pub fn zap_size(file: &[u8]) -> Option<u32> {
    if file.len() < 20 || &file[..12] != ZAP_MAGIC {
        return None;
    }
    Some(u32::from_le_bytes(file[16..20].try_into().unwrap()))
}

/// The `ZapFile` load (@ 0x0047e660, runs @ 0x0047e700 / 0x0047e760): the
/// stored size and the data, `want` bytes of them rebuilt (0 = all).
/// Runs are cut to end at `want` as the original cuts them; where its
/// unsigned clip test wraps (a huge length read from the middle of a cut
/// literal) this clips instead. A file whose runs end before `want` bytes
/// are rebuilt is refused here (the original reads on past its end).
pub fn zap_decode(file: &[u8], want: usize) -> Option<(u32, Vec<u8>)> {
    let size = zap_size(file)?;
    let want = if want == 0 { size as usize } else { want };
    let mut data = vec![0u8; size as usize];
    let u32_at = |p: usize| {
        file.get(p..p + 4)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap())) as usize
    };
    let (mut pos, mut cur) = (20usize, 0usize);
    // A file cut short ends the runs (the rest stays 0).
    while cur < want && pos < file.len() {
        let n = u32_at(pos).min(want - cur);
        pos += 4;
        let chunk = file.get(pos..(pos + n).min(file.len())).unwrap_or(&[]);
        let end = (cur + chunk.len()).min(data.len());
        if cur < end {
            data[cur..end].copy_from_slice(&chunk[..end - cur]);
        }
        pos += n;
        cur += n;
        if cur >= want {
            break;
        }
        let n = u32_at(pos).min(want - cur);
        let b = file.get(pos + 4).copied().unwrap_or(0);
        pos += 5;
        let end = (cur + n).min(data.len());
        if cur < end {
            data[cur..end].fill(b);
        }
        cur += n;
    }
    (cur >= want).then_some((size, data))
}

/// How a save file was stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFormat {
    Zap,
    /// The 69288 bytes as they are: accepted, and saved again at once as
    /// a `ZapFile` in the same slot (FUN_00478f30).
    Raw,
}

/// Why a save file was refused (`AtlSaveGame_Load(%d) fichier
/// invalide(%d != %d)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidSave {
    pub size: usize,
}

impl fmt::Display for InvalidSave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid save ({} != {SAVE_SIZE})", self.size)
    }
}

impl std::error::Error for InvalidSave {}

/// The size check of `AtlSaveGame_Load` (@ 0x00478f30): a `ZapFile` whose
/// stored size is 69288, or a raw file of exactly 69288 bytes.
pub fn read_save(file: &[u8]) -> Result<(SaveData, SaveFormat), InvalidSave> {
    match zap_size(file) {
        Some(n) if n as usize == SAVE_SIZE => {
            let (_, data) = zap_decode(file, 0).ok_or(InvalidSave { size: 0 })?;
            Ok((SaveData { bytes: data }, SaveFormat::Zap))
        }
        Some(n) => Err(InvalidSave { size: n as usize }),
        None if file.len() == SAVE_SIZE => Ok((
            SaveData {
                bytes: file.to_vec(),
            },
            SaveFormat::Raw,
        )),
        None => Err(InvalidSave { size: file.len() }),
    }
}

/// `scan_savegame_file` (@ 0x00478b50 / 0x00479250): the header of a
/// valid save (only its first 48 bytes are decoded), `None` otherwise.
pub fn scan_save(file: &[u8]) -> Option<SaveHeader> {
    match zap_size(file) {
        Some(n) if n as usize == SAVE_SIZE => SaveHeader::parse(&zap_decode(file, HEADER_SIZE)?.1),
        Some(_) => None,
        None if file.len() == SAVE_SIZE => SaveHeader::parse(file),
        None => None,
    }
}

/// The file a save is written to: the `ZapFile` of the data, the bytes
/// the original leaves to chance written as 0.
pub fn write_save(data: &SaveData) -> Vec<u8> {
    zap_encode(&data.bytes, 0, [0; 4]).0
}

/// A slot's file name (@ 0x00403a40: `ATL%02d_%02d.SAV` in `save\`).
pub fn save_file_name(player: u8, slot: u8) -> String {
    format!("ATL{player:02}_{slot:02}.SAV")
}

/// `atlant2.cfg` (the table @ 0x005df900, read at start-up by
/// FUN_0047f590 and written at exit by FUN_0047f530): 35 bytes, no
/// header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cfg {
    /// The mouse's Y inverted (@ 0x006569cc).
    pub invert_y: i32,
    /// The divisor of the mouse turn (@ 0x005da268).
    pub sensitivity: i32,
    /// Full screen (@ 0x005e2f2c).
    pub fullscreen: u8,
    /// The subtitles (@ 0x005df970).
    pub subtitles: u8,
    /// The centre marker (@ 0x005db84c).
    pub centre_marker: u8,
    /// The general volume, 0..256 (@ 0x005dd2b8).
    pub general: i32,
    /// The effects volume, 0..127 (@ 0x005dd2bc).
    pub effects: i32,
    /// The music volume, 0..127 (@ 0x006c2e6c: a copy of @ 0x005dc9dc).
    pub music: i32,
    /// The dialog voices' volume, 0..127 (@ 0x005df974).
    pub dialogs: i32,
    /// The screen size, 640 x 480 or 800 x 600 (@ 0x005e2f24, 0x005e2f28).
    pub width: i32,
    pub height: i32,
}

/// The size of `atlant2.cfg`.
pub const CFG_SIZE: usize = 35;

impl Default for Cfg {
    /// The globals' initial values in the executable's data section.
    fn default() -> Cfg {
        Cfg {
            invert_y: 0,
            sensitivity: 2000,
            fullscreen: 1,
            subtitles: 1,
            centre_marker: 1,
            general: 256,
            effects: 127,
            music: 100,
            dialogs: 127,
            width: 640,
            height: 480,
        }
    }
}

impl Cfg {
    pub fn to_bytes(&self) -> [u8; CFG_SIZE] {
        let mut out = [0u8; CFG_SIZE];
        let mut put = |o: usize, b: &[u8]| out[o..o + b.len()].copy_from_slice(b);
        put(0, &self.invert_y.to_le_bytes());
        put(4, &self.sensitivity.to_le_bytes());
        put(8, &[self.fullscreen]);
        put(9, &[self.subtitles]);
        put(10, &[self.centre_marker]);
        put(11, &self.general.to_le_bytes());
        put(15, &self.effects.to_le_bytes());
        put(19, &self.music.to_le_bytes());
        put(23, &self.dialogs.to_le_bytes());
        put(27, &self.width.to_le_bytes());
        put(31, &self.height.to_le_bytes());
        out
    }

    /// The file read over the defaults, as far as it goes (a short file
    /// leaves the rest; the bytes of a field cut short replace only its
    /// first bytes).
    pub fn parse(file: &[u8]) -> Cfg {
        let mut b = Cfg::default().to_bytes();
        let n = file.len().min(CFG_SIZE);
        b[..n].copy_from_slice(&file[..n]);
        let i = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let mut cfg = Cfg {
            invert_y: i(0),
            sensitivity: i(4),
            fullscreen: b[8],
            subtitles: b[9],
            centre_marker: b[10],
            general: i(11),
            effects: i(15),
            music: i(19),
            dialogs: i(23),
            width: i(27),
            height: i(31),
        };
        // The reader passes the music volume to FUN_00478100, which
        // accepts 0..127 only.
        if !(0..=127).contains(&cfg.music) {
            cfg.music = Cfg::default().music;
        }
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_and_offsets() {
        assert_eq!(ENTRIES.iter().map(|e| e.1).sum::<usize>(), SAVE_SIZE);
        assert_eq!(entry_offset(entry::VARS), 0x30);
        assert_eq!(entry_offset(entry::FLAGS), 0x4e0);
        assert_eq!(entry_offset(entry::YAW), 0x191c);
        assert_eq!(entry_offset(entry::SHOWN), 0x1a35);
        assert_eq!(entry_offset(entry::CURSOR), 0x4a39);
        assert_eq!(entry_offset(entry::DIALOG), 0x4aa7);
        assert_eq!(entry_offset(entry::SUBJECTS), 0x4aa8);
        assert_eq!(SUBJECT_RECORD * SUBJECT_CHARACTERS, ENTRIES[34].1);
    }

    #[test]
    fn round_trips() {
        let mut d = SaveData::default();
        d.set_wam(b"tibet.wam");
        d.set_i32(entry::CHAPTER, 75);
        d.entry_mut(entry::SUBJECTS)[1000..1030].fill(7);
        let file = write_save(&d);
        assert_eq!(&file[..12], ZAP_MAGIC);
        let (back, f) = read_save(&file).unwrap();
        assert_eq!((back, f), (d.clone(), SaveFormat::Zap));
        let h = scan_save(&file).unwrap();
        assert_eq!((h.wam.as_slice(), h.chapter), (&b"tibet.wam"[..], 75));
        assert!(read_save(&file[..40]).is_err());
        assert_eq!(read_save(d.bytes()).unwrap().1, SaveFormat::Raw);
    }

    #[test]
    fn civil_time() {
        // 1999-10-05 08:14:30, a Tuesday.
        let t = SystemTime::from_unix(939_111_270, 250);
        assert_eq!(
            (
                t.year,
                t.month,
                t.day,
                t.day_of_week,
                t.hour,
                t.minute,
                t.second
            ),
            (1999, 10, 5, 2, 8, 14, 30)
        );
    }
}

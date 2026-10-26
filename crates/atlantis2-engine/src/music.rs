//! The music files and choices: `muzikNN.apc` decoded from its start or
//! from a `.spp` mark, the random mark (`AtlMuzik_GetRandomMark`
//! @ 0x00478090) and the `startmusik` list (`Interp_SelectMusic`
//! @ 0x004851f0). The player's states and fades are in [`crate::sound`];
//! the engine runs them.
//!
//! The player's streaming ([`Stream`]) is the original's: voice 0 loops
//! over a 0x80000-byte ring which `check_buffer_music` (@ 0x00477590)
//! refills 0x4000 bytes at a time about 5.8 s ahead of the play cursor.
//! When the decoder (not the listener) reaches the end of a track, the
//! next one is written behind the cursor and the cursor moved there: the
//! last ~5.8 s of a track followed by another are never heard. With no
//! next track (an empty `startmusik` list) or loops left, the rewind fails
//! and the ring keeps looping its last 5.9 s. The bug fixes play every
//! track to its end and an ended track again from its start instead,
//! each its own switch ([`MusicFixes`], see `docs/TECHNICAL.md`).

use std::sync::Arc;

use atlantis_codecs::{ApcDecoder, ApcHeader, APC_HEADER_SIZE};

use crate::crt::Rand;
use crate::sound::Pcm;

/// The ring's size in bytes (voice 0's buffer, @ 0x00476c90).
pub const RING_BYTES: i32 = 0x80000;
/// What one refill writes, bytes (0x1000 file bytes decoded).
const CHUNK: i32 = 0x4000;
/// How far `GotoMark` moves the write offset back (@ 0x00477f53).
const GOTO_BACK: i32 = 0x14000;

/// The music file of track `n` (@ 0x005deb38: `muzik%02d.apc`).
pub fn music_name(n: i32) -> String {
    format!("muzik{n:02}.apc")
}

/// Its marks' file (`music_name_to_spp_name` @ 0x00477d00).
pub fn spp_name(n: i32) -> String {
    format!("muzik{n:02}.spp")
}

/// One `.spp` record (0x38 bytes): where a resume starts and the decoder
/// state there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    /// The byte offset in the ADPCM data (after the 0x20-byte header).
    pub offset: u32,
    pub stereo: bool,
    /// The frames left to decode from there.
    pub frames_left: u32,
    pub predictors: [i32; 2],
    pub step_indexes: [u32; 2],
}

/// `Load_Spp` (@ 0x00477340): the records; `None` when the size is not a
/// multiple of 0x38 (`fichier SPP invalide`).
pub fn parse_spp(data: &[u8]) -> Option<Vec<Mark>> {
    if !data.len().is_multiple_of(0x38) {
        return None;
    }
    let le = |r: &[u8], o: usize| u32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    Some(
        data.as_chunks::<0x38>()
            .0
            .iter()
            .map(|r| Mark {
                offset: le(r, 0),
                stereo: le(r, 0x04) & 1 != 0,
                frames_left: le(r, 0x18),
                predictors: [le(r, 0x1c) as i32, le(r, 0x20) as i32],
                step_indexes: [le(r, 0x24), le(r, 0x28)],
            })
            .collect(),
    )
}

/// Decodes a music file from its start (`mark` `None`) or from a mark
/// (`AtlMuzik_GotoMark` @ 0x00477e80: the record's state over the
/// decoder's, the file read from `offset + 0x20`), up to the frames the
/// state has left.
pub fn decode_music(data: &[u8], mark: Option<&Mark>) -> Option<Pcm> {
    let header = ApcHeader::parse(data).ok()?;
    let (mut dec, start, frames, stereo) = match mark {
        None => (
            ApcDecoder::new(&header),
            APC_HEADER_SIZE,
            header.frames as usize,
            header.stereo,
        ),
        Some(m) => (
            ApcDecoder::resume(m.stereo, m.predictors, m.step_indexes),
            m.offset as usize + APC_HEADER_SIZE,
            m.frames_left as usize,
            m.stereo,
        ),
    };
    let channels: u16 = if stereo { 2 } else { 1 };
    let wanted = frames * usize::from(channels);
    let body = data.get(start..).unwrap_or(&[]);
    let body = &body[..body.len().min(wanted.div_ceil(2))];
    let mut samples = Vec::with_capacity(wanted);
    dec.decode(body, &mut samples);
    samples.truncate(wanted);
    Some(Pcm {
        rate: header.sample_rate,
        channels,
        samples: Arc::new(samples),
    })
}

/// `AtlMuzik_GetRandomMark` (@ 0x00478090): up to 10 draws of
/// `rand() · (count + 1) / 0x7fff` while the draw equals the current mark.
/// 0 is the file's start; `count + 1` (from `rand()` = 0x7fff, the golden's
/// `randmark.edge` rows) is the invalid mark `GotoMark` reports as fatal
/// ([`MusicFixes::mark`]).
pub fn random_mark(rand: &mut Rand, count: i32, current: i32) -> i32 {
    let mut m = current;
    for _ in 0..10 {
        if m != current {
            break;
        }
        m = rand.rand() * (count + 1) / 0x7fff;
    }
    m
}

/// One entry of a `startmusik` list: `mus[-mark][*loops]` (mark −1 = a
/// random one, loops 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub music: i32,
    pub mark: i32,
    pub loops: i32,
}

/// The list's parser in `Interp_SelectMusic` (with @ 0x00485180: skip to a
/// digit, `atoi`, skip the digits and blanks), up to `)` or the end.
pub fn parse_list(text: &[u8]) -> Vec<Entry> {
    let mut entries = Vec::new();
    if text.is_empty() {
        return entries;
    }
    let num = |i: &mut usize| -> i32 {
        while *i < text.len() && !text[*i].is_ascii_digit() && text[*i] != b'\r' {
            *i += 1;
        }
        let v = atlantis2_data::con::atoi(&text[*i..]);
        while *i < text.len() && text[*i].is_ascii_digit() {
            *i += 1;
        }
        while *i < text.len() && matches!(text[*i], b' ' | b'\t') {
            *i += 1;
        }
        v
    };
    let mut i = 0;
    loop {
        if text.get(i) == Some(&b')') {
            break;
        }
        let music = num(&mut i);
        let mut e = Entry {
            music,
            mark: -1,
            loops: 0,
        };
        if text.get(i) == Some(&b'-') {
            e.mark = num(&mut i);
        }
        if text.get(i) == Some(&b'*') {
            e.loops = num(&mut i);
        }
        entries.push(e);
        if i >= text.len() {
            break;
        }
    }
    entries
}

/// `Interp_SelectMusic` (@ 0x004851f0) on the list: the entry to play and
/// the number of entries; `None` when the list is empty. One `rand()` is
/// drawn first; with two entries naming the current music the other one
/// comes in 85 % of the draws; with more, up to 10 draws of an index while
/// it equals the current music's number (an index compared with a
/// number). The `rand()` = 0x7fff edge (an entry past the list, read from
/// the original's stack) gives `None` here.
pub fn select(
    rand: &mut Rand,
    text: &[u8],
    current_music: i32,
    current_mark: i32,
) -> Option<(Entry, usize)> {
    let e = parse_list(text);
    let r = rand.rand() * 100 / 0x7fff;
    if e.is_empty() {
        return None;
    }
    let n = e.len();
    if n == 1 {
        return Some((e[0], 1));
    }
    let idx = if n == 2 {
        if current_music == e[0].music || current_music == e[1].music {
            let (same, other) = if e[0].music == e[1].music {
                if current_mark == e[0].mark {
                    (0, 1)
                } else {
                    (1, 0)
                }
            } else if current_music == e[0].music {
                (0, 1)
            } else {
                (1, 0)
            };
            if r < 85 {
                other
            } else {
                same
            }
        } else {
            (r * 2 / 100) as usize
        }
    } else {
        let mut idx = current_music;
        for _ in 0..10 {
            if idx != current_music {
                break;
            }
            idx = (i64::from(rand.rand()) * n as i64 * 100 / 0x31_ff9c) as i32;
        }
        idx as usize
    };
    e.get(idx).map(|&x| (x, n))
}

/// Voice 0's DirectSound buffer as the music player sees it: the play
/// cursor and the bytes it locks.
pub trait Ring {
    /// `GetCurrentPosition`'s play cursor, in bytes; `None` without a
    /// buffer.
    fn cursor(&self) -> Option<i32>;
    /// `SetCurrentPosition`, in bytes.
    fn set_cursor(&mut self, at: i32);
    /// Writes `samples` (16-bit) at byte offset `at` (a lock, the copy, the
    /// unlock).
    fn write(&mut self, at: i32, samples: &[i16]);
}

/// A music file as the player opens it: its number, bytes and marks.
#[derive(Clone, Debug)]
pub struct MusicData {
    pub music: i32,
    pub data: Arc<Vec<u8>>,
    pub marks: Vec<Mark>,
}

/// What the player did, for the engine's events and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamEvent {
    /// A music file opened (by a start or at the end of a track).
    Open(i32),
    /// The play cursor moved by `GotoMark`: to, from (bytes).
    Cursor(i32, i32),
    /// The rewind's file check failed: the copy deleted and copied again
    /// (`remove`, `LOADMUSIK`), the file closed.
    Recopy(i32),
    /// A fatal error of the original's player (FUN_00480cd0: an
    /// emergency save, then the game quits), its message: `GotoMark` on a
    /// mark the file does not have.
    Fatal(String),
}

/// The open file (@ 0x00672174) and where it is read.
#[derive(Clone, Debug)]
struct OpenFile {
    file: MusicData,
    read: usize,
}

/// The music player's bug fixes, each its own switch
/// ([`crate::BugFix`]); all off is the original.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MusicFixes {
    /// The next music of the list is written right after the old track's
    /// end, which is heard to its last sample (the original writes it
    /// behind the play cursor, `GotoMark`).
    pub end: bool,
    /// The rewind of a track that ends with an empty list or with loops
    /// left checks the file's size, as the code means, and the track
    /// plays again from its start right after its end (the original's
    /// check compares the PCM size, fails and closes the file).
    pub restart: bool,
    /// A random mark drawn at the `rand()` = 0x7fff edge, one past the
    /// file's marks, is its last mark (the original's `GotoMark` reports
    /// it as a fatal error).
    pub mark: bool,
}

impl MusicFixes {
    /// Every fix `on`.
    pub fn all(on: bool) -> MusicFixes {
        MusicFixes {
            end: on,
            restart: on,
            mark: on,
        }
    }
}

/// The music streamer (`AtlMuzik`, @ 0x00672138..0x006725c4): the file,
/// the decoder state, the PCM position and size, the ring's write offset,
/// the music buffer, the mark played and the loops left.
#[derive(Clone, Debug)]
pub struct Stream {
    file: Option<OpenFile>,
    dec: Option<ApcDecoder>,
    /// The PCM bytes decoded from the file (@ 0x00672178).
    pub pos: i32,
    /// The PCM size in bytes (@ 0x0067217c); the file size during an
    /// open.
    pub total: i32,
    /// The ring's write offset (@ 0x00672180).
    pub write: i32,
    /// The voice's rate and channels (@ 0x005dc9cc, 0x005dc9d4).
    pub rate: u32,
    pub channels: u16,
    /// The music buffer (@ 0x0067216c): a start from the beginning
    /// decodes its first 0x80000 bytes there, and every new voice 0 is
    /// created from it.
    pub buffer: Vec<i16>,
    /// The mark played (@ 0x006725a4), set by `GotoMark` only.
    pub current_mark: i32,
    /// The loops left (@ 0x006725c4).
    pub loops: i32,
    pub events: Vec<StreamEvent>,
    /// The bug fixes; `GotoMark` reads [`MusicFixes::mark`], the refill
    /// ([`Stream::refill`]) sets them.
    pub fixes: MusicFixes,
}

impl Default for Stream {
    fn default() -> Self {
        Stream {
            file: None,
            dec: None,
            pos: 0,
            total: 0,
            write: 0,
            rate: 22050,
            channels: 2,
            buffer: vec![0; RING_BYTES as usize / 2],
            current_mark: 0,
            loops: 0,
            events: Vec::new(),
            fixes: MusicFixes::default(),
        }
    }
}

/// The ring offset after `w` (the write offset wraps at the ring's end).
fn wrap(w: i32) -> i32 {
    if w == RING_BYTES {
        0
    } else {
        if w > RING_BYTES {
            log::error!("check_buffer_music: music offset {w:#x} past the ring");
        }
        w
    }
}

impl Stream {
    /// The music open, if any.
    pub fn music(&self) -> Option<i32> {
        self.file.as_ref().map(|f| f.file.music)
    }

    /// Closes the file (`FUN_00403cb0`, and the marks freed by
    /// FUN_004778f0).
    pub fn close(&mut self) {
        self.file = None;
    }

    /// `FUN_00477160`: opens `file` (only `.apc` music exists, the ADPCM
    /// mode 0), checks it (`init_read_music_data`) and loads its marks;
    /// the write offset and position back to 0. False when the check
    /// fails (the file closed).
    pub fn open(&mut self, file: MusicData) -> bool {
        self.total = file.data.len() as i32;
        self.file = Some(OpenFile { file, read: 0 });
        if !self.init_read(false) {
            self.file = None;
            return false;
        }
        self.write = 0;
        self.pos = 0;
        self.events
            .push(StreamEvent::Open(self.music().unwrap_or(0)));
        true
    }

    /// `init_read_music_data` (FUN_00477b00) on the file at its read
    /// position: the header, the decoder state, the voice format, and the
    /// size check `|(total − 0x20) · 4 − frames · channels · 2| ≤ 4`, where
    /// `total` is the file size at an open but the PCM size at a rewind
    /// (`fixed`: the file size, the check the code means). A failed check
    /// closes the file and copies it again.
    fn init_read(&mut self, fixed: bool) -> bool {
        let Some(f) = self.file.as_mut() else {
            return false;
        };
        let data = f.file.data.clone();
        let header = data
            .get(f.read..f.read + APC_HEADER_SIZE)
            .and_then(|h| ApcHeader::parse(h).ok());
        f.read += APC_HEADER_SIZE.min(data.len().saturating_sub(f.read));
        let Some(header) = header else {
            log::error!("init_read_music_data: not an APC file");
            return false;
        };
        self.dec = Some(ApcDecoder::new(&header));
        self.rate = header.sample_rate;
        self.channels = header.channels();
        let checked = if fixed { data.len() as i32 } else { self.total };
        let size = (checked.wrapping_sub(0x20)).wrapping_mul(4);
        self.total = (header.frames as i32)
            .wrapping_mul(i32::from(self.channels))
            .wrapping_mul(2);
        self.pos = 0;
        if size.wrapping_sub(self.total).wrapping_abs() > 4 {
            // `Fichier musique corrompu`: remove, LOADMUSIK N.
            log::info!("init_read_music_data: file {size} / PCM {}", self.total);
            let music = f.file.music;
            self.file = None;
            self.events.push(StreamEvent::Recopy(music));
            return false;
        }
        true
    }

    /// Feeds `n` file bytes to the decoder (`FUN_00403bf0` and the stream
    /// decoder FUN_00593f60).
    fn feed(&mut self, n: usize, out: &mut Vec<i16>) {
        let (Some(f), Some(dec)) = (self.file.as_mut(), self.dec.as_mut()) else {
            return;
        };
        let end = (f.read + n).min(f.file.data.len());
        let start = f.read.min(end);
        if end - start != n {
            log::error!("read_music_data: {} of {n} bytes", end - start);
        }
        dec.decode(&f.file.data[start..end], out);
        f.read = end;
    }

    /// `read_music_data` (FUN_00477950): `len` PCM bytes, 0x1000 file
    /// bytes per 0x4000 while more than 0x4000 are left, then `len / 4`.
    /// Returns the samples and `len`.
    fn read_data(&mut self, len: i32) -> (Vec<i16>, i32) {
        let mut out = Vec::new();
        let mut ret = 0;
        let mut len = len;
        if len == 0 {
            log::error!("read_music_data: len 0");
        }
        if len > CHUNK {
            ret += (len - 1) & !(CHUNK - 1);
            while len > CHUNK {
                self.feed(0x1000, &mut out);
                len -= CHUNK;
            }
        }
        if len != 0 {
            let n = (len + ((len >> 31) & 3)) >> 2;
            self.feed(n.max(0) as usize, &mut out);
            ret += len;
        }
        (out, ret)
    }

    /// FUN_00477410 after the open: the first 0x80000 bytes decoded into
    /// the music buffer, the write offset 0.
    pub fn decode_start(&mut self) {
        let (samples, n) = self.read_data(RING_BYTES);
        let k = samples.len().min(self.buffer.len());
        self.buffer[..k].copy_from_slice(&samples[..k]);
        self.pos = n;
        self.write = 0;
    }

    /// `AtlMuzik_GotoMark` (FUN_00477e80): mark `k` (−1 a random one, 0
    /// the start, k ≥ 1 record k − 1 over the decoder state); the write
    /// offset moved back 0x14000 bytes, four 0x4000-byte chunks decoded
    /// from there and the play cursor set where they start.
    pub fn goto_mark(&mut self, k: i32, rand: &mut Rand, ring: &mut dyn Ring) -> bool {
        let Some(offset) = self.seek_mark(k, rand) else {
            return false;
        };
        let mut w = self.write - GOTO_BACK;
        if w < 0 {
            w = self.write + (RING_BYTES - GOTO_BACK);
        }
        self.write = w;
        self.file_seek(offset as usize + APC_HEADER_SIZE);
        let start = self.write;
        self.pos = offset.wrapping_mul(4);
        for _ in 0..4 {
            let (samples, _) = self.read_data(CHUNK);
            ring.write(self.write, &samples);
            self.pos += CHUNK;
            if self.total < self.pos {
                log::error!("AtlMuzik_GotoMark: too close to the end");
            }
            self.write = wrap(self.write + CHUNK);
        }
        let from = ring.cursor().unwrap_or(0);
        ring.set_cursor(start);
        self.events.push(StreamEvent::Cursor(start, from));
        true
    }

    /// The decoder side of `GotoMark`: the mark drawn (−1), the decoder
    /// state from the header (0) or from the record; the mark played set.
    /// Returns the record's offset (0 for the start).
    fn seek_mark(&mut self, k: i32, rand: &mut Rand) -> Option<i32> {
        let marks = self.file.as_ref()?.file.marks.clone();
        let mut k = k;
        if k == -1 {
            k = random_mark(rand, marks.len() as i32, self.current_mark);
            if self.fixes.mark {
                k = k.min(marks.len() as i32);
            }
        }
        let mut offset = 0;
        if k == 0 {
            self.file_seek(0);
            let data = self.file.as_ref()?.file.data.clone();
            if let Ok(h) = ApcHeader::parse(&data) {
                self.dec = Some(ApcDecoder::new(&h));
            }
        } else {
            match usize::try_from(k - 1).ok().and_then(|i| marks.get(i)) {
                Some(m) => {
                    offset = m.offset as i32;
                    self.dec = Some(ApcDecoder::resume(m.stereo, m.predictors, m.step_indexes));
                }
                None => {
                    // A fatal error in the original (@ 0x00477f2a).
                    let msg = format!("AtlMuzik_GotoMark({k}) invalide ({} marks)", marks.len());
                    log::error!("{msg}");
                    self.events.push(StreamEvent::Fatal(msg));
                    return None;
                }
            }
        }
        self.current_mark = k;
        Some(offset)
    }

    fn file_seek(&mut self, at: usize) {
        if let Some(f) = self.file.as_mut() {
            f.read = at;
        }
    }

    /// `check_buffer_music` (FUN_00477590): when the play cursor has left
    /// the chunk at the write offset, the next 0x4000 bytes decoded there.
    /// At the end of the data, with no loops left, the end-of-track
    /// callback (@ 0x00485080: [`select`] on `list`, its mark 0 when the
    /// list has one entry) gives the next music, which opens and goes to
    /// its mark (`GotoMark`: behind the cursor, what the ring still held of
    /// the old track dropped); with none, or loops left, the file is
    /// rewound, and the rewind's check closes it: the ring loops on.
    /// `load` reads a music file. `fixes`: with [`MusicFixes::end`] the
    /// next music is written right after the old track's end, which is
    /// heard to its last sample; with [`MusicFixes::restart`] the rewind
    /// succeeds and the track goes on from its start there. True when the
    /// ring was written.
    #[allow(clippy::too_many_arguments)]
    pub fn refill(
        &mut self,
        ring: &mut dyn Ring,
        list: &[u8],
        current_music: i32,
        rand: &mut Rand,
        load: &mut dyn FnMut(i32) -> Option<MusicData>,
        fixes: MusicFixes,
    ) -> bool {
        self.fixes = fixes;
        if self.file.is_none() {
            return false;
        }
        let Some(p) = ring.cursor() else {
            return false;
        };
        if self.write <= p && p <= self.write + CHUNK {
            return false;
        }
        let mut len = CHUNK;
        if RING_BYTES < self.write + CHUNK {
            len = RING_BYTES - self.write;
            if len == 0 {
                return true;
            }
        }
        let fits = self.pos + len <= self.total;
        let mut rest = 0;
        if !fits {
            rest = len - (self.total - self.pos);
            len = self.total - self.pos;
        }
        if len != 0 {
            let (samples, _) = self.read_data(len);
            ring.write(self.write, &samples);
        }
        if fits {
            self.pos += len;
            self.write = wrap(self.write + len);
            return true;
        }
        if self.loops < 2 {
            if let Some((e, count)) = select(rand, list, current_music, self.current_mark) {
                let mut mark = if count == 1 { 0 } else { e.mark };
                let old = self.music();
                self.file = None;
                // The same file at a mark other than −1 restarts at 0.
                if old == Some(e.music) && mark != -1 {
                    mark = 0;
                }
                let Some(file) = load(e.music) else {
                    log::warn!("{}: not found, no music", music_name(e.music));
                    return false;
                };
                if fixes.end {
                    if !self.continue_with(file, mark, len, rest, rand, ring) {
                        return false;
                    }
                    self.loops = e.loops;
                    return true;
                }
                if !self.open(file) {
                    return false;
                }
                self.goto_mark(mark, rand, ring);
                self.loops = e.loops;
                return true;
            }
        } else {
            self.loops -= 1;
        }
        self.file_seek(0);
        if !self.init_read(fixes.restart) {
            self.file = None;
            return false;
        }
        // Reached only with the restart fix: the rewound file goes on in
        // the chunk.
        self.write_rest(len, rest, ring);
        true
    }

    /// The fixed end of a track: `file` opened at `mark` and decoded into
    /// the rest of the chunk, right after the old track's last sample.
    fn continue_with(
        &mut self,
        file: MusicData,
        mark: i32,
        len: i32,
        rest: i32,
        rand: &mut Rand,
        ring: &mut dyn Ring,
    ) -> bool {
        let music = file.music;
        self.total = file.data.len() as i32;
        self.file = Some(OpenFile { file, read: 0 });
        if !self.init_read(true) {
            self.file = None;
            return false;
        }
        self.events.push(StreamEvent::Open(music));
        let Some(offset) = self.seek_mark(mark, rand) else {
            self.file = None;
            return false;
        };
        self.file_seek(offset as usize + APC_HEADER_SIZE);
        self.pos = offset.wrapping_mul(4);
        self.write_rest(len, rest, ring);
        true
    }

    /// The rest of the chunk after the old track's end, from the file's
    /// position (no more than it holds).
    fn write_rest(&mut self, len: i32, rest: i32, ring: &mut dyn Ring) {
        let n = rest.min(self.total - self.pos).max(0);
        if n > 0 {
            let (samples, n) = self.read_data(n);
            ring.write(self.write + len, &samples);
            self.pos += n;
            self.write = wrap(self.write + len + n);
        } else {
            self.write = wrap(self.write + len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists() {
        assert_eq!(
            parse_list(b"20-3)"),
            [Entry {
                music: 20,
                mark: 3,
                loops: 0
            }]
        );
        assert_eq!(
            parse_list(b"1-2*3,4)"),
            [
                Entry {
                    music: 1,
                    mark: 2,
                    loops: 3
                },
                Entry {
                    music: 4,
                    mark: -1,
                    loops: 0
                }
            ]
        );
        assert!(parse_list(b"").is_empty());
    }
}

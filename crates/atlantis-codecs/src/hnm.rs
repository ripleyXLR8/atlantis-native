//! The HNM6 container: header, superchunks (one per frame), video and sound
//! chunks, frame timing.
//!
//! Translated from ScummVM's `Video::HNMDecoder` (video/hnm_decoder.cpp,
//! GPLv3, the ScummVM team): `loadStream` for the header, `readNextPacket`
//! for the superchunks, `HNM6VideoTrack` for the timing and `APCAudioTrack`
//! for the sound, used the way the Atlantis engine uses it (panoramas read
//! until the first superchunk holding a video chunk; IX videos optionally
//! decoded into frame buffers kept from one video to the next).

use crate::apc::{ApcDecoder, ApcHeader, APC_HEADER_SIZE};
use crate::hnm6::{Hnm6Decoder, HnmPixel};
use crate::{DecodeError, Panorama};

/// Size of the file header; superchunks follow it.
const FILE_HEADER_SIZE: usize = 64;

/// Frame delay of HNM6 files without sound.
const REGULAR_FRAME_DELAY_MS: u64 = 66;

const fn tag16(s: &[u8; 2]) -> u16 {
    u16::from_be_bytes(*s)
}

const CHUNK_IW: u16 = tag16(b"IW");
const CHUNK_IX: u16 = tag16(b"IX");
const CHUNK_SD: u16 = tag16(b"SD");
const CHUNK_AA: u16 = tag16(b"AA");
const CHUNK_BB: u16 = tag16(b"BB");
/// Atlantis II's dialog videos: the alpha mask of the frame (run pairs),
/// drawn by the dialog video stream, not by the frame decoder.
const CHUNK_AL: u16 = tag16(b"AL");

fn le16(d: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([d[pos], d[pos + 1]])
}

fn le32(d: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([d[pos], d[pos + 1], d[pos + 2], d[pos + 3]])
}

/// The sample format of a video's sound track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// One chunk of a superchunk.
struct Chunk<'a> {
    kind: u16,
    data: &'a [u8],
}

/// Splits a superchunk's payload into its chunks (4-byte aligned in HNM6).
fn chunks(mut rest: &[u8]) -> impl Iterator<Item = Result<Chunk<'_>, DecodeError>> {
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        if rest.len() < 8 {
            rest = &[];
            return Some(Err(DecodeError::InvalidSuperchunk));
        }
        let size = le32(rest, 0) as usize;
        let kind = u16::from_be_bytes([rest[4], rest[5]]);
        if size < 8 || size > rest.len() {
            rest = &[];
            return Some(Err(DecodeError::InvalidChunk));
        }
        let data = &rest[8..size];
        let aligned = size.div_ceil(4) * 4;
        rest = rest.get(aligned..).unwrap_or(&[]);
        Some(Ok(Chunk { kind, data }))
    })
}

/// Which header rule turns a file's sound track on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoundRule {
    /// The first two games: byte +6 bit 0, the rate from its bits 4..6
    /// (`(flags >> 4) & 6` x 11025 Hz) and stereo from its bit 7.
    #[default]
    Flags,
    /// *Atlantis III*'s CryoMovie library (`Game.exe` @ 0x006b8929): sound
    /// whenever byte +6 is not zero, whatever its bits; the track is the
    /// first `AA` chunk of the first frame (else its first `BB` chunk), its
    /// rate and channels read from that chunk's APC header
    /// (@ 0x006b96fb .. 0x006b9766); no such chunk, no sound.
    ApcHeader,
}

/// The sound track: APC data in AA (with a header) and BB chunks.
struct AudioTrack {
    format: AudioFormat,
    /// *Atlantis III*'s CryoMovie rules ([`SoundRule::ApcHeader`]).
    movie: Option<MovieSound>,
    decoder: Option<ApcDecoder>,
    pcm: Vec<i16>,
}

/// How *Atlantis III*'s CryoMovie decodes a film's sound (`Game.exe`
/// @ 0x006b9ec0, the APC decoder @ 0x006b3160): only the first `AA` chunk
/// carries the APC header, every later `AA` (or `BB`) chunk is raw data
/// continuing the stream. Each frame decodes a fixed number of sample
/// frames F (the first one 32 F), F = ((first payload - 32) >> 5) x 4
/// bytes of 16-bit samples, whatever the chunks hold; the header's sample
/// count is a budget: past it, or when the data runs out, the frame's
/// buffer is filled with silence. Every frame, with or without a sound
/// chunk, has its F sample frames.
#[derive(Default)]
struct MovieSound {
    /// APC bytes received and not yet decoded.
    fifo: std::collections::VecDeque<u8>,
    /// Sample frames the header still allows.
    remaining: u64,
    /// F, once the first chunk is read.
    per_frame: u64,
    frames_out: u64,
}

/// The bytes of an HNM file for [`HnmVideo`]: the whole file in memory
/// (`Vec<u8>`, `&[u8]`, `Arc<[u8]>`...) or read by parts as the frames
/// need them ([`HnmParts`], [`HnmData`]).
pub trait HnmBytes {
    /// The file's size.
    fn size(&self) -> usize;
    /// Bytes `start..end`, read if need be; `None` past the end of the
    /// file or when they cannot be read.
    fn bytes(&mut self, start: usize, end: usize) -> Option<&[u8]>;
    /// Bytes `start..end` if they are at hand: those of the last two
    /// [`HnmBytes::bytes`] calls are.
    fn held(&self, start: usize, end: usize) -> Option<&[u8]>;
}

impl<T: AsRef<[u8]>> HnmBytes for T {
    fn size(&self) -> usize {
        self.as_ref().len()
    }

    fn bytes(&mut self, start: usize, end: usize) -> Option<&[u8]> {
        let d: &T = self;
        d.as_ref().get(start..end)
    }

    fn held(&self, start: usize, end: usize) -> Option<&[u8]> {
        self.as_ref().get(start..end)
    }
}

/// Reads `len` bytes at an offset of a file (all of them, else `None`).
pub type ReadPart = Box<dyn FnMut(u64, usize) -> Option<Vec<u8>> + Send + Sync>;

/// An HNM file read by parts: the bytes the decoder asks for, a part of
/// at least `part` bytes at a time, read in order as the frames are
/// decoded; the bytes before the last two asked for are let go. A film is
/// then never read whole at its start (the web build's synchronous reads
/// of the player's files cost about 12 ms a MB on the page's thread).
pub struct HnmParts {
    read: ReadPart,
    size: usize,
    /// The file offset of `buf[0]`.
    start: usize,
    buf: Vec<u8>,
    /// The start of the previous request, kept.
    keep: usize,
    part: usize,
}

impl HnmParts {
    /// A file of `size` bytes read through `read`, `part` bytes at least
    /// at a time.
    pub fn new(size: u64, part: usize, read: ReadPart) -> HnmParts {
        HnmParts {
            read,
            size: usize::try_from(size).unwrap_or(usize::MAX),
            start: 0,
            buf: Vec::new(),
            keep: 0,
            part: part.max(1),
        }
    }
}

impl HnmBytes for HnmParts {
    fn size(&self) -> usize {
        self.size
    }

    fn bytes(&mut self, start: usize, end: usize) -> Option<&[u8]> {
        if start > end || end > self.size {
            return None;
        }
        let keep = std::mem::replace(&mut self.keep, start);
        let held_end = self.start + self.buf.len();
        if start < self.start || end > held_end {
            let from = start.min(keep);
            if from >= self.start && from <= held_end {
                self.buf.drain(..from - self.start);
                self.start = from;
            } else {
                self.buf.clear();
                self.start = start;
            }
            let have = self.start + self.buf.len();
            let want = end.max(start.saturating_add(self.part)).min(self.size);
            if want > have {
                let more = (self.read)(have as u64, want - have)?;
                if more.len() != want - have {
                    return None;
                }
                self.buf.extend_from_slice(&more);
            }
        }
        self.buf.get(start - self.start..end - self.start)
    }

    fn held(&self, start: usize, end: usize) -> Option<&[u8]> {
        self.buf
            .get(start.checked_sub(self.start)?..end.checked_sub(self.start)?)
    }
}

/// An HNM file in memory or read by parts: the type the players keep, so
/// that one player type serves both.
pub enum HnmData {
    Vec(Vec<u8>),
    Shared(std::sync::Arc<[u8]>),
    Parts(HnmParts),
}

impl From<Vec<u8>> for HnmData {
    fn from(d: Vec<u8>) -> HnmData {
        HnmData::Vec(d)
    }
}

impl From<std::sync::Arc<[u8]>> for HnmData {
    fn from(d: std::sync::Arc<[u8]>) -> HnmData {
        HnmData::Shared(d)
    }
}

impl From<HnmParts> for HnmData {
    fn from(d: HnmParts) -> HnmData {
        HnmData::Parts(d)
    }
}

impl HnmBytes for HnmData {
    fn size(&self) -> usize {
        match self {
            HnmData::Vec(d) => d.len(),
            HnmData::Shared(d) => d.len(),
            HnmData::Parts(p) => p.size(),
        }
    }

    fn bytes(&mut self, start: usize, end: usize) -> Option<&[u8]> {
        match self {
            HnmData::Vec(d) => d.get(start..end),
            HnmData::Shared(d) => d.get(start..end),
            HnmData::Parts(p) => p.bytes(start, end),
        }
    }

    fn held(&self, start: usize, end: usize) -> Option<&[u8]> {
        match self {
            HnmData::Vec(d) => d.get(start..end),
            HnmData::Shared(d) => d.get(start..end),
            HnmData::Parts(p) => p.held(start, end),
        }
    }
}

/// An HNM6 file decoded frame by frame: the IX videos (transitions,
/// cinematics) and the IW panoramas, as RGB565 frames.
///
/// `D` holds the file's bytes (`Vec<u8>`, `&[u8]`, `Arc<[u8]>`...).
pub struct HnmVideo<D: HnmBytes = Vec<u8>, P: HnmPixel = u16> {
    data: D,
    width: usize,
    height: usize,
    frame_count: u32,
    warp: bool,
    decoder: Hnm6Decoder<P>,
    /// A caller's frame buffer block not used by a warp file.
    spare: Option<Vec<P>>,
    /// Offset of the next superchunk.
    pos: usize,
    decoded: u32,
    finished: bool,
    audio: Option<AudioTrack>,
    /// Sample rate of the timestamps (1 without sound).
    time_rate: u64,
    /// Start of the next frame, in 1 / (time_rate * 1000) s.
    next_start: u64,
    /// Sound sample frames per video frame, once known.
    frame_samples: u64,
    /// The delay of a frame without sound.
    regular_delay_ms: u64,
    /// The last `AL` payload of the last superchunk read (start, end in
    /// `data`), `None` when that superchunk has none.
    alpha: Option<(usize, usize)>,
}

impl<D: HnmBytes> HnmVideo<D> {
    /// Opens an HNM6 file, with fresh zeroed frame buffers.
    pub fn new(data: D) -> Result<Self, DecodeError> {
        Self::with_buffers(data, Vec::new())
    }

    /// Opens an HNM6 file whose IX frames are decoded into `block`, a pair
    /// of frame buffers at offsets 0 and `width * height` (grown with zeros
    /// if shorter than two frames). The original allocates them once and
    /// never clears them: passing the block of the previous video (see
    /// [`HnmVideo::into_buffers`]) reproduces what its first frames show.
    pub fn with_buffers(data: D, block: Vec<u16>) -> Result<Self, DecodeError> {
        Self::with_rule(data, block, SoundRule::Flags)
    }

    /// [`HnmVideo::with_buffers`] with the header rule `rule` for the sound
    /// track.
    pub fn with_rule(data: D, block: Vec<u16>, rule: SoundRule) -> Result<Self, DecodeError> {
        Self::open(data, block, rule)
    }
}

impl<D: HnmBytes, P: HnmPixel> HnmVideo<D, P> {
    /// [`HnmVideo::with_rule`] into frame buffers of pixels `P`: `u16` for
    /// RGB565, `u32` for the x8r8g8b8 frames of a 32-bit display
    /// ([`HnmPixel`]).
    pub fn open(mut data: D, block: Vec<P>, rule: SoundRule) -> Result<Self, DecodeError> {
        // The header and the first superchunk, all that is looked at here
        // (the first superchunk's end bounded by the file's).
        let len = data.size();
        if len < FILE_HEADER_SIZE {
            return Err(DecodeError::Truncated);
        }
        let first = data
            .bytes(FILE_HEADER_SIZE, FILE_HEADER_SIZE + 4)
            .map_or(4, |w| (le32(w, 0) & 0x00ff_ffff) as usize);
        let end = (FILE_HEADER_SIZE + first.max(4)).min(len);
        let hnm = &data.bytes(0, end).ok_or(DecodeError::Truncated)?.to_vec()[..];
        // HNM4 and UBB2 are 8-bit formats: the 16-bit decoder refuses them.
        if &hnm[0..4] != b"HNM6" {
            return Err(DecodeError::NotHnm6);
        }
        let audio_flags = hnm[6];
        let width = usize::from(le16(hnm, 8));
        let height = usize::from(le16(hnm, 10));
        let frame_count = le32(hnm, 16);
        let frame_size = le32(hnm, 28) as usize;
        // The decoder needs room for at least a frame header.
        if frame_size < 24 {
            return Err(DecodeError::InvalidFileHeader);
        }
        if width == 0 || height == 0 || width % 8 != 0 || height % 8 != 0 {
            return Err(DecodeError::InvalidFileHeader);
        }

        let format = match rule {
            SoundRule::Flags => (audio_flags & 1 != 0).then(|| AudioFormat {
                sample_rate: u32::from((audio_flags >> 4) & 6) * 11025,
                channels: if audio_flags & 0x80 != 0 { 2 } else { 1 },
            }),
            SoundRule::ApcHeader if audio_flags != 0 => first_sound_format(hnm),
            SoundRule::ApcHeader => None,
        };
        let audio = format.map(|format| AudioTrack {
            format,
            movie: (rule == SoundRule::ApcHeader).then(MovieSound::default),
            decoder: None,
            pcm: Vec::new(),
        });
        let time_rate = audio
            .as_ref()
            .map_or(1, |a| u64::from(a.format.sample_rate).max(1));

        // The first video chunk tells a warp (IW, panorama) file, decoded
        // into one buffer, from a video (IX), decoded into two.
        let warp = first_video_chunk(hnm) == Some(CHUNK_IW);
        let (block, spare) = if warp {
            (Vec::new(), Some(block))
        } else {
            (block, None)
        };
        Ok(HnmVideo {
            width,
            height,
            frame_count,
            warp,
            decoder: Hnm6Decoder::new(width, height, frame_size, !warp, block),
            spare,
            pos: FILE_HEADER_SIZE,
            decoded: 0,
            finished: false,
            audio,
            time_rate,
            next_start: 0,
            frame_samples: 0,
            regular_delay_ms: REGULAR_FRAME_DELAY_MS,
            alpha: None,
            data,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// The number of frames the file header announces.
    pub fn frame_count(&self) -> u32 {
        self.frame_count
    }

    /// The number of frames decoded so far.
    pub fn frames_decoded(&self) -> u32 {
        self.decoded
    }

    /// Whether this is a warp (panorama, IW) file rather than a video.
    pub fn is_warp(&self) -> bool {
        self.warp
    }

    /// Sets the delay of the frames of a file without sound (66 ms by
    /// default): the engine plays the moves' transition videos at 30 ms a
    /// frame and the cinematics at 65 ms (ScummVM's `setRegularFrameDelay`).
    /// Call it before the first frame.
    pub fn set_regular_frame_delay(&mut self, ms: u64) {
        self.regular_delay_ms = ms;
    }

    /// When the next frame is due, in milliseconds from the start: 66 ms a
    /// frame without sound; with sound, the first frame's sound covers 32
    /// frames and sets the pace.
    pub fn next_frame_start_ms(&self) -> u64 {
        self.next_start / self.time_rate
    }

    /// The sound track's format, if the file has one.
    pub fn audio_format(&self) -> Option<AudioFormat> {
        self.audio.as_ref().map(|a| a.format)
    }

    /// Takes the sound decoded so far (signed 16-bit, interleaved in
    /// stereo). The first frame holds the sound of the 32 first frames.
    pub fn take_audio(&mut self) -> Vec<i16> {
        self.audio
            .as_mut()
            .map(|a| std::mem::take(&mut a.pcm))
            .unwrap_or_default()
    }

    /// The alpha mask of the last frame read: the payload of the last `AL`
    /// chunk of its superchunk (Atlantis II's dialog videos), as the chunk
    /// scan atlantis2.exe @ 0x0040be20 keeps it; `None` when the superchunk
    /// has none. Draw it with [`crate::AlphaTables::blend`].
    pub fn alpha(&self) -> Option<&[u8]> {
        let (start, end) = self.alpha?;
        self.data.held(start, end)
    }

    /// The last decoded frame (before the first one: the initial buffer
    /// content), `width * height` RGB565 pixels row by row.
    pub fn frame(&self) -> &[P] {
        self.decoder.frame()
    }

    /// The last decoded frame's buffer, to draw into: the original's frame
    /// presenter draws the overlays of a video into it (atlantis.exe
    /// `FUN_0041b064` @ 0x0041b187..0x0041b192), so that the next frames'
    /// motion copies read them.
    pub fn frame_mut(&mut self) -> &mut [P] {
        self.decoder.frame_mut()
    }

    /// An IX video: the buffer of the frame decoded before the last one,
    /// which the next frame is decoded into (its in-place motion copies
    /// read it first). `None` for a warp file.
    pub fn previous_frame_mut(&mut self) -> Option<&mut [P]> {
        self.decoder.previous_frame_mut()
    }

    /// Gives back the frame buffer block, to decode the next video into.
    pub fn into_buffers(self) -> Vec<P> {
        match self.spare {
            Some(block) => block,
            None => self.decoder.into_block(),
        }
    }

    /// Decodes the next frame. `None` once the header's frame count is
    /// reached or the file ends; after an error, every later call returns
    /// `None`.
    pub fn next_frame(&mut self) -> Option<Result<&[P], DecodeError>> {
        if self.finished || (self.frame_count != 0 && self.decoded >= self.frame_count) {
            self.finished = true;
            return None;
        }
        match self.read_packet() {
            Ok(Some(_)) => Some(Ok(self.decoder.frame())),
            Ok(None) => {
                self.finished = true;
                None
            }
            Err(e) => {
                self.finished = true;
                Some(Err(e))
            }
        }
    }

    /// Reads one superchunk: `None` at the end of the file, else whether it
    /// held a video chunk.
    fn read_packet(&mut self) -> Result<Option<bool>, DecodeError> {
        // A zero header ends the file.
        let word = match self.data.bytes(self.pos, self.pos + 4) {
            Some(w) => le32(w, 0),
            None => return Ok(None),
        };
        if word == 0 {
            return Ok(None);
        }
        let size = (word & 0x00ff_ffff) as usize;
        if size < 4 {
            return Err(DecodeError::InvalidSuperchunk);
        }
        let base = self.pos + 4;
        let superchunk = self
            .data
            .bytes(base, self.pos + size)
            .ok_or(DecodeError::Truncated)?;
        self.pos += size;

        let mut got_video = false;
        // Sound sample frames of the superchunk's last sound chunk.
        let mut frame_delay = None;
        self.alpha = None;
        for chunk in chunks(superchunk) {
            let Chunk { kind, data } = chunk?;
            match kind {
                CHUNK_SD | CHUNK_AA | CHUNK_BB => {
                    if let Some(audio) = &mut self.audio {
                        frame_delay = Some(audio.decode(kind, data)?);
                    }
                }
                CHUNK_IW | CHUNK_IX => {
                    self.decoder.decode_frame(data, kind == CHUNK_IW)?;
                    got_video = true;
                }
                // Not part of the picture: kept for the caller (the first
                // game's files have none).
                CHUNK_AL => {
                    let start = base + (data.as_ptr() as usize - superchunk.as_ptr() as usize);
                    self.alpha = Some((start, start + data.len()));
                }
                other => return Err(DecodeError::UnexpectedChunk(other)),
            }
        }
        if let Some(audio) = &mut self.audio {
            if audio.movie.is_some() {
                frame_delay = audio.movie_frame();
            }
        }
        self.new_frame(frame_delay);
        Ok(Some(got_video))
    }

    /// Frame done: advances the next frame's start time.
    fn new_frame(&mut self, frame_delay: Option<u64>) {
        self.decoded += 1;
        if self.frame_samples == 0 {
            match frame_delay {
                // No sound: a regular delay
                None => {
                    self.next_start += self.regular_delay_ms * self.time_rate;
                    return;
                }
                // The first frame holds the sound of the 32 following
                // frames (pre-buffering); then each frame holds one
                // frame's worth, except the 32 last.
                Some(samples) => self.frame_samples = samples / 32,
            }
        }
        self.next_start += self.frame_samples * 1000;
    }
}

impl AudioTrack {
    /// Decodes one sound chunk; returns its number of sample frames.
    fn decode(&mut self, kind: u16, mut data: &[u8]) -> Result<u64, DecodeError> {
        if let Some(m) = &mut self.movie {
            if kind == CHUNK_SD {
                return Ok(0);
            }
            if self.decoder.is_none() {
                if kind != CHUNK_AA {
                    return Ok(0);
                }
                let header = ApcHeader::parse(data)?;
                self.decoder = Some(ApcDecoder::new(&header));
                m.remaining = u64::from(header.frames);
                // ((payload - 32) >> 5) * 4 bytes of 16-bit samples.
                let bytes = ((data.len() - APC_HEADER_SIZE) >> 5) as u64 * 4;
                m.per_frame = bytes / (2 * u64::from(self.format.channels));
                data = &data[APC_HEADER_SIZE..];
            }
            m.fifo.extend(data.iter().copied());
            return Ok(0);
        }
        if kind == CHUNK_AA {
            let header = ApcHeader::parse(data)?;
            if header.sample_rate != self.format.sample_rate
                || header.channels() != self.format.channels
            {
                return Err(DecodeError::Bitstream("sound track format mismatch"));
            }
            self.decoder = Some(ApcDecoder::new(&header));
            data = &data[APC_HEADER_SIZE..];
        }
        if kind != CHUNK_SD {
            if let Some(dec) = &mut self.decoder {
                dec.decode(data, &mut self.pcm);
            }
        }
        // Two 4-bit samples in one byte
        let bytes = data.len() as u64;
        Ok(if self.format.channels == 2 {
            bytes
        } else {
            2 * bytes
        })
    }
}

impl AudioTrack {
    /// A CryoMovie frame's sound: 32 F sample frames for the first frame,
    /// F for each later one, decoded from the bytes received within the
    /// header's budget, silence for the rest. `None` before the first
    /// chunk (no sound yet: the regular delay).
    fn movie_frame(&mut self) -> Option<u64> {
        let channels = u64::from(self.format.channels);
        let m = self.movie.as_mut()?;
        let decoder = self.decoder.as_mut()?;
        let want = if m.frames_out == 0 {
            32 * m.per_frame
        } else {
            m.per_frame
        };
        m.frames_out += 1;
        let n = want.min(m.remaining);
        // Stereo: a byte per sample frame; mono: two.
        let bytes = if channels == 2 { n } else { n.div_ceil(2) } as usize;
        let take = bytes.min(m.fifo.len());
        let chunk: Vec<u8> = m.fifo.drain(..take).collect();
        let start = self.pcm.len();
        decoder.decode(&chunk, &mut self.pcm);
        let got = ((self.pcm.len() - start) as u64 / channels).min(n);
        self.pcm.truncate(start + (got * channels) as usize);
        m.remaining -= got;
        self.pcm.resize(start + (want * channels) as usize, 0);
        Some(want)
    }
}

/// The format of the sound track by the APC header of the first `AA` chunk
/// of the first superchunk, else of its first `BB` chunk (CryoMovie's
/// search, `Game.exe` of *Atlantis III* @ 0x006b7f60).
fn first_sound_format(hnm: &[u8]) -> Option<AudioFormat> {
    let pos = FILE_HEADER_SIZE;
    let size = (le32(hnm.get(pos..pos + 4)?, 0) & 0x00ff_ffff) as usize;
    let end = (pos + size.max(4)).min(hnm.len());
    let first = |tag| {
        chunks(hnm.get(pos + 4..end)?)
            .map_while(Result::ok)
            .find(|c| c.kind == tag)
            .map(|c| c.data)
    };
    let header = ApcHeader::parse(first(CHUNK_AA).or_else(|| first(CHUNK_BB))?).ok()?;
    Some(AudioFormat {
        sample_rate: header.sample_rate,
        channels: header.channels(),
    })
}

/// The type of the first video chunk of the first superchunk, skipping
/// sound chunks.
fn first_video_chunk(hnm: &[u8]) -> Option<u16> {
    let pos = FILE_HEADER_SIZE;
    let size = (le32(hnm.get(pos..pos + 4)?, 0) & 0x00ff_ffff) as usize;
    let end = (pos + size.max(4)).min(hnm.len());
    chunks(&hnm[pos + 4..end])
        .map_while(Result::ok)
        .map(|c| c.kind)
        .find(|&k| k == CHUNK_IW || k == CHUNK_IX)
}

/// Decodes the first frame of an HNM6 file, reading superchunks until one
/// holds a video chunk.
pub(crate) fn decode_first_frame(hnm: &[u8]) -> Result<Panorama, DecodeError> {
    let mut video = HnmVideo::new(hnm)?;
    loop {
        match video.read_packet()? {
            None => return Err(DecodeError::NoVideo),
            Some(false) => {}
            Some(true) => {
                return Ok(Panorama {
                    width: video.width,
                    height: video.height,
                    pixels: video.frame().to_vec(),
                })
            }
        }
    }
}

//! The 64-byte header of *Atlantis III*'s HNM6 videos, as the game's two
//! linked HNM libraries read it, and where a cinematic's sound is.
//!
//! The pictures are decoded by `atlantis_codecs::hnm6::HnmVideo`; this
//! module only reads the header and tells what the original does with it.
//! The executable holds two readers and no check of its own (no `HNM6`
//! magic test anywhere in `Game.exe`):
//!
//! - **CryoMovie** plays the cinematics and the intro (`AtlHnm_start`
//!   @ 0x00420ee0 with its last argument 0). Its open (@ 0x006b88f0) reads
//!   the 64 bytes into its object and uses +0x06 (sound when non-zero),
//!   +0x08 width, +0x0a height, the u16 at +0x10 (frame count) and the u16
//!   at +0x18: the file's frame rate is `200 / value` (object +0x68; 15
//!   when the value is 0, constructor @ 0x006b8130).
//! - **Hnm_Framer**, wrapped by `CHnmStream` (@ 0x004430d0, library
//!   version 0x1b581 checked @ 0x006a0460), plays the warp cubes, the dialog
//!   videos and the travel videos. Its open (@ 0x006a07ec) reads +0x08,
//!   +0x0a, the u16 at +0x10 and the u16 at +0x18 (frame time
//!   `value / 200` s). It never plays sound: only `IX` and `AL` chunks are
//!   used (@ 0x006a0c31), other chunks go to a callback no caller sets.
//!
//! Neither reads +0x04, +0x05, +0x07 (bits per pixel), +0x0c (file size),
//! the high half of +0x10, +0x14 (the `V109` / `V110` tag of the files),
//! +0x1a or +0x1c (largest frame size): the buffers are sized from width x
//! height x the screen's bytes per pixel.
//!
//! **Sound.** A cinematic's sound is inside the file, in `AA` chunks (APC,
//! a `CRYO_APC` header in the first one) and `BB` chunks; CryoMovie plays
//! it through `waveOut` when header +0x06 is non-zero, whatever its bits
//! (the files hold 0xc0 or 0xb0). The sample rate and channels come from
//! the APC header of the first `AA` chunk (@ 0x006b2d30: rate at +0x10,
//! stereo = bit 0 of +0x1c), never from the header byte. The first chunk
//! holds 32 frames of sound; one `waveOut` buffer of
//! `(first payload length - 32) / 32 * 4` bytes is queued per frame and
//! each finished buffer decodes the next frame (@ 0x006b9160), so the
//! video is clocked by the sound. Without sound, a multimedia timer of
//! `1000 / fps` ms (truncated, @ 0x006b9b11) clocks it.
//!
//! **Chunk sizes.** Both libraries round a chunk's size up to 4 and take
//! the low two bits of the chunk's byte 7 (the high byte of its flags) as
//! padding at the end of the payload (@ 0x006b7f60, @ 0x006a0c31): the
//! dialog videos' `AL` chunks with flags 0x200 carry two bytes of padding.

use std::fmt;

/// Size of the header.
pub const HEADER_SIZE: usize = 0x40;

/// Frame rate of a CryoMovie file whose u16 at +0x18 is 0 (@ 0x006b8130:
/// object +0x68 = 15.0).
pub const DEFAULT_FPS: f32 = 15.0;

/// The header fields. Fields neither library reads are kept raw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HnmHeader {
    /// +0x00, `HNM6` in every shipped file; not checked by the game.
    pub magic: [u8; 4],
    /// +0x04, +0x05: 0 in every file; not read.
    pub byte4: u8,
    pub byte5: u8,
    /// +0x06: CryoMovie plays the `AA` / `BB` sound when non-zero
    /// (@ 0x006b8929). 0xc0 or 0xb0 in the files with sound, 0 elsewhere;
    /// the meaning of the bits is `[UNKNOWN]` and irrelevant to the game.
    pub sound_flags: u8,
    /// +0x07: 16 in every file; not read (the screen's depth is used).
    pub bpp: u8,
    /// +0x08, +0x0a.
    pub width: u16,
    pub height: u16,
    /// +0x0c: the file's length in every shipped file; not read.
    pub file_size: u32,
    /// +0x10 as a u32; both libraries read only its low 16 bits.
    pub frame_count_u32: u32,
    /// +0x14: ASCII `V109` or `V110` in the cinematics and dialog videos,
    /// 0 elsewhere; read by neither library. Probably the encoder's
    /// version `[INFERRED]`.
    pub tag: [u8; 4],
    /// +0x18: CryoMovie's frame rate is `200 / rate_div`; Hnm_Framer's
    /// frame time is `rate_div / 200` s.
    pub rate_div: u16,
    /// +0x1a: 2 in every file; not read.
    pub word_1a: u16,
    /// +0x1c: width x height x 2 in every file; not read.
    pub frame_size: u32,
}

/// Why a header could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HnmHeaderError {
    /// Shorter than [`HEADER_SIZE`].
    Truncated,
}

impl fmt::Display for HnmHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HnmHeaderError::Truncated => write!(f, "HNM header shorter than 64 bytes"),
        }
    }
}

impl std::error::Error for HnmHeaderError {}

fn le16(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn le32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

impl HnmHeader {
    /// Reads the first 64 bytes of `data`.
    pub fn parse(data: &[u8]) -> Result<HnmHeader, HnmHeaderError> {
        let d = data.get(..HEADER_SIZE).ok_or(HnmHeaderError::Truncated)?;
        Ok(HnmHeader {
            magic: [d[0], d[1], d[2], d[3]],
            byte4: d[4],
            byte5: d[5],
            sound_flags: d[6],
            bpp: d[7],
            width: le16(d, 8),
            height: le16(d, 0x0a),
            file_size: le32(d, 0x0c),
            frame_count_u32: le32(d, 0x10),
            tag: [d[0x14], d[0x15], d[0x16], d[0x17]],
            rate_div: le16(d, 0x18),
            word_1a: le16(d, 0x1a),
            frame_size: le32(d, 0x1c),
        })
    }

    /// The frame count both libraries use: the u16 at +0x10 (CryoMovie
    /// object +0x58 @ 0x006b88f0, Hnm_Framer +0x0c @ 0x006a07ec).
    pub fn frame_count(&self) -> u32 {
        self.frame_count_u32 & 0xffff
    }

    /// Whether CryoMovie looks for sound (`AA` / `BB` chunks): +0x06 != 0.
    pub fn has_sound(&self) -> bool {
        self.sound_flags != 0
    }

    /// The file's frame rate for CryoMovie: `200 / rate_div`, or
    /// [`DEFAULT_FPS`] when `rate_div` is 0.
    pub fn file_fps(&self) -> f32 {
        if self.rate_div == 0 {
            DEFAULT_FPS
        } else {
            200.0 / f32::from(self.rate_div)
        }
    }

    /// Hnm_Framer's frame time in seconds: `rate_div / 200` (object +0x18).
    pub fn framer_frame_time(&self) -> f32 {
        f32::from(self.rate_div) / 200.0
    }

    /// The `+0x14` tag as text when it is printable ASCII (`V109`).
    pub fn tag_text(&self) -> Option<&str> {
        if self.tag.iter().all(|c| c.is_ascii_graphic()) {
            std::str::from_utf8(&self.tag).ok()
        } else {
            None
        }
    }
}

/// The timer period of a video played without sound: `1000 / fps` ms,
/// truncated (`__ftol` @ 0x006b9b1a, and @ 0x006b90e2 when the script
/// gives a frame rate).
pub fn silent_period_ms(fps: f32) -> u32 {
    if fps > 0.0 {
        (1000.0 / fps) as u32
    } else {
        0
    }
}

/// The sound format of a cinematic, from the `CRYO_APC` header that starts
/// the payload of its first `AA` chunk (@ 0x006b2d30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundFormat {
    /// APC header +0x0c: samples per channel.
    pub samples: u32,
    /// APC header +0x10: `nSamplesPerSec` of the `waveOut` device.
    pub sample_rate: u32,
    /// APC header +0x1c bit 0.
    pub stereo: bool,
}

impl SoundFormat {
    /// Reads an APC header (32 bytes); `None` when it is not
    /// `CRYO_APC1.20`, the only version the library accepts.
    pub fn from_apc_header(apc: &[u8]) -> Option<SoundFormat> {
        let h = apc.get(..0x20)?;
        if &h[..12] != b"CRYO_APC1.20" {
            return None;
        }
        Some(SoundFormat {
            samples: le32(h, 0x0c),
            sample_rate: le32(h, 0x10),
            stereo: le32(h, 0x1c) & 1 != 0,
        })
    }
}

/// The step from one chunk to the next and its payload length, as
/// CryoMovie computes them (@ 0x006b7f60): the step is the size rounded up
/// to 4; the padding is that rounding when the size is not a multiple of
/// 4 (the routine writes it into byte 7), else the low two bits of byte 7;
/// the payload is the step minus the padding minus the 8-byte chunk
/// header. Hnm_Framer (@ 0x006a0c31) steps the same way and subtracts
/// byte 7 unmasked from the `AL` payload only; every shipped chunk size is
/// a multiple of 4 and every byte 7 is 0 or 2, so the two agree.
/// `None` when the chunk header is cut.
pub fn chunk_step(chunk: &[u8]) -> Option<(usize, usize)> {
    let h = chunk.get(..8)?;
    let size = le32(h, 0) as usize;
    let step = size.checked_add(3)? & !3;
    let pad = if step != size {
        step - size
    } else {
        usize::from(h[7] & 3)
    };
    Some((step, step.saturating_sub(pad + 8)))
}

/// The payload of the first `AA` chunk of the first superchunk, the only
/// place CryoMovie looks for the sound format (@ 0x006b96fb: `AA`, then
/// `BB`, whose payload has no APC header and so turns the sound off),
/// walked with [`chunk_step`]. `None` when that superchunk has no `AA`
/// chunk: the original then plays the video silent.
pub fn first_sound_chunk(data: &[u8]) -> Option<&[u8]> {
    let at = HEADER_SIZE;
    let len = (le32(data.get(at..at + 4)?, 0) & 0x00ff_ffff) as usize;
    let end = at.checked_add(len)?.min(data.len());
    let mut p = at + 4;
    while p + 8 <= end {
        let (step, payload) = chunk_step(&data[p..])?;
        if step == 0 {
            return None;
        }
        if &data[p + 4..p + 6] == b"AA" {
            return data.get(p + 8..p + 8 + payload);
        }
        p += step;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(flags: u8, rate_div: u16, frames: u32, tag: &[u8; 4]) -> Vec<u8> {
        let mut h = vec![0u8; HEADER_SIZE];
        h[..4].copy_from_slice(b"HNM6");
        h[6] = flags;
        h[7] = 16;
        h[8..10].copy_from_slice(&64u16.to_le_bytes());
        h[10..12].copy_from_slice(&32u16.to_le_bytes());
        h[0x10..0x14].copy_from_slice(&frames.to_le_bytes());
        h[0x14..0x18].copy_from_slice(tag);
        h[0x18..0x1a].copy_from_slice(&rate_div.to_le_bytes());
        h[0x1a..0x1c].copy_from_slice(&2u16.to_le_bytes());
        h
    }

    #[test]
    fn reads_the_fields_the_libraries_read() {
        let h = HnmHeader::parse(&header(0xb0, 8, 0x0001_0005, b"V110")).unwrap();
        assert!(h.has_sound());
        assert_eq!(h.frame_count(), 5);
        assert_eq!((h.width, h.height), (64, 32));
        assert_eq!(h.file_fps(), 25.0);
        assert_eq!(h.framer_frame_time(), 0.04);
        assert_eq!(h.tag_text(), Some("V110"));
        assert_eq!(silent_period_ms(h.file_fps()), 40);
    }

    #[test]
    fn zero_rate_and_flags() {
        let h = HnmHeader::parse(&header(0, 0, 3, &[0; 4])).unwrap();
        assert!(!h.has_sound());
        assert_eq!(h.file_fps(), DEFAULT_FPS);
        assert_eq!(silent_period_ms(h.file_fps()), 66);
        assert_eq!(h.tag_text(), None);
        assert_eq!(HnmHeader::parse(&[0; 10]), Err(HnmHeaderError::Truncated));
    }

    #[test]
    fn finds_the_sound_chunk_and_its_format() {
        let mut d = header(0xc0, 8, 1, b"V109");
        // APC header: 32 bytes.
        let mut apc = b"CRYO_APC1.20".to_vec();
        apc.extend(100u32.to_le_bytes());
        apc.extend(44_100u32.to_le_bytes());
        apc.extend(0u32.to_le_bytes());
        apc.extend(0u32.to_le_bytes());
        apc.extend(1u32.to_le_bytes());
        apc.extend([0xaa; 4]);
        // Superchunk: an IX chunk of 12 bytes, then the AA chunk.
        let ix_len = 8 + 4;
        let aa_len = 8 + apc.len();
        let total = 4 + ix_len + aa_len;
        d.extend((total as u32).to_le_bytes());
        d.extend((ix_len as u32).to_le_bytes());
        d.extend(b"IX\0\0");
        d.extend([0; 4]);
        d.extend((aa_len as u32).to_le_bytes());
        d.extend(b"AA\0\0");
        d.extend(&apc);
        d.extend([0; 4]);
        let payload = first_sound_chunk(&d).unwrap();
        assert_eq!(payload.len(), apc.len());
        let f = SoundFormat::from_apc_header(payload).unwrap();
        assert_eq!(f.sample_rate, 44_100);
        assert!(f.stereo);
        assert_eq!(f.samples, 100);
    }

    #[test]
    fn chunk_padding_comes_from_byte_seven() {
        // 0x200 flags: byte 7 = 2, two bytes of padding.
        let c = [16, 0, 0, 0, b'A', b'L', 0, 2];
        assert_eq!(chunk_step(&c), Some((16, 6)));
        let odd = [13, 0, 0, 0, b'A', b'L', 0, 0];
        assert_eq!(chunk_step(&odd), Some((16, 5)));
    }
}

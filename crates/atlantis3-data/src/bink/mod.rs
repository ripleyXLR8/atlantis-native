// Ported from ScummVM's `video/bink_decoder.cpp` and `.h` (ScummVM -
// Graphic Adventure Engine, Copyright (C) the ScummVM team, GNU General
// Public License, version 3 or later). ScummVM's decoder is based on eos'
// Bink decoder, itself based heavily on FFmpeg's Bink decoder by Kostya
// Shishkov.

//! Bink video (RAD Game Tools), the films of *Atlantis Evolution*'s 2004
//! CD program: a port of the video part of ScummVM's `BinkDecoder`.
//!
//! What the CD needs: its 300 films are all `BIKi`, without audio tracks,
//! 182 of them with an alpha plane (video flag `0x00100000`), at 15, 24
//! or 25 frames per second. The port keeps ScummVM's code for the four
//! revisions it reads (`BIKf`, `BIKg`, `BIKh`, `BIKi`), the alpha plane
//! and odd sizes; it decodes no audio (the packets of any audio tracks
//! are skipped as ScummVM reads them) and does not seek.
//!
//! - [`BinkVideo`]: the header, the frame index and the frames in order;
//! - [`Frame`]: a decoded frame's Y, U, V (and A) planes, and their
//!   conversion to x8r8g8b8, a8r8g8b8 or RGB565 by ScummVM's
//!   `YUVToRGBManager` (`kScaleITU`) as its Bink decoder calls it.
//!
//! [UNKNOWN] RAD's own `binkw32.dll`, which the 2004 program links, may
//! decode or convert differently from ScummVM's code; only ScummVM's is
//! ported.

mod bits;
mod decoder;
mod tables;
mod yuv;

use decoder::VideoTrack;

/// Why a film cannot be read or a frame decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinkError {
    /// Not `BIKf`, `BIKg`, `BIKh` or `BIKi`.
    NotBink,
    /// The header or the frame index is cut short or inconsistent.
    BadHeader(&'static str),
    /// A frame's data is out of the file.
    BadFrame(u32),
    /// The packet does not decode (ScummVM's `error()`, or a read or write
    /// outside its buffers).
    Corrupt(&'static str),
}

impl std::fmt::Display for BinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinkError::NotBink => f.write_str("not a Bink file"),
            BinkError::BadHeader(w) => write!(f, "bad Bink header: {w}"),
            BinkError::BadFrame(i) => write!(f, "frame {i} is out of the file"),
            BinkError::Corrupt(w) => write!(f, "corrupt Bink frame: {w}"),
        }
    }
}

impl std::error::Error for BinkError {}

/// The header's video flag of an alpha plane (`kVideoFlagAlpha`).
pub const VIDEO_FLAG_ALPHA: u32 = 0x0010_0000;

/// A frame's place in the file (`VideoFrame`).
#[derive(Clone, Copy, Debug)]
struct FrameEntry {
    key_frame: bool,
    offset: u32,
    size: u32,
}

/// A Bink film read from its bytes (`BinkDecoder`, video only).
pub struct BinkVideo<D: AsRef<[u8]>> {
    data: D,
    id: [u8; 4],
    width: u32,
    height: u32,
    fps_num: u32,
    fps_den: u32,
    video_flags: u32,
    audio_tracks: u32,
    frames: Vec<FrameEntry>,
    track: VideoTrack,
    /// The number of frames decoded (the next frame's index).
    next: u32,
}

fn u32_at(data: &[u8], pos: usize) -> Result<u32, BinkError> {
    data.get(pos..pos + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(BinkError::BadHeader("cut short"))
}

impl<D: AsRef<[u8]>> BinkVideo<D> {
    /// Reads the header and the frame index (`loadStream`).
    pub fn open(data: D) -> Result<BinkVideo<D>, BinkError> {
        let d = data.as_ref();
        let id: [u8; 4] = d
            .get(0..4)
            .ok_or(BinkError::NotBink)?
            .try_into()
            .map_err(|_| BinkError::NotBink)?;
        if !matches!(&id, b"BIKf" | b"BIKg" | b"BIKh" | b"BIKi") {
            return Err(BinkError::NotBink);
        }

        let file_size = u32_at(d, 4)?.wrapping_add(8);
        let frame_count = u32_at(d, 8)?;
        let largest_frame_size = u32_at(d, 12)?;
        if largest_frame_size > file_size {
            return Err(BinkError::BadHeader(
                "largest frame size greater than file size",
            ));
        }
        let width = u32_at(d, 20)?;
        let height = u32_at(d, 24)?;
        let fps_num = u32_at(d, 28)?;
        let fps_den = u32_at(d, 32)?;
        if fps_num == 0 || fps_den == 0 {
            return Err(BinkError::BadHeader("invalid frame rate"));
        }
        let video_flags = u32_at(d, 36)?;
        let audio_tracks = u32_at(d, 40)?;
        if width == 0 || height == 0 || width > 0x1_0000 || height > 0x1_0000 {
            return Err(BinkError::BadHeader("invalid size"));
        }

        // The audio tracks' headers: a dword each, then sample rate and
        // flags, then a dword each again.
        let mut pos = 44usize;
        if audio_tracks > 0 {
            pos = (audio_tracks as usize)
                .checked_mul(12)
                .and_then(|n| n.checked_add(pos))
                .ok_or(BinkError::BadHeader("too many audio tracks"))?;
        }

        if frame_count == 0 {
            return Err(BinkError::BadHeader("no frames"));
        }
        (frame_count as usize)
            .checked_mul(4)
            .and_then(|n| n.checked_add(pos))
            .filter(|&e| e <= d.len())
            .ok_or(BinkError::BadHeader("frame index cut short"))?;
        let mut frames = Vec::with_capacity(frame_count as usize);
        for i in 0..frame_count as usize {
            let offset = u32_at(d, pos + 4 * i)?;
            frames.push(FrameEntry {
                key_frame: offset & 1 != 0,
                offset: offset & !1,
                size: 0,
            });
        }
        for i in 1..frames.len() {
            frames[i - 1].size = frames[i].offset.wrapping_sub(frames[i - 1].offset);
        }
        let last = frames.len() - 1;
        frames[last].size = (d.len() as u32).wrapping_sub(frames[last].offset);

        let track = VideoTrack::new(
            width,
            height,
            u32::from_be_bytes(id),
            video_flags & VIDEO_FLAG_ALPHA != 0,
        );
        Ok(BinkVideo {
            data,
            id,
            width,
            height,
            fps_num,
            fps_den,
            video_flags,
            audio_tracks,
            frames,
            track,
            next: 0,
        })
    }

    /// The revision, `BIKf` to `BIKi`.
    pub fn id(&self) -> [u8; 4] {
        self.id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The frame rate as the header gives it: numerator and denominator.
    pub fn fps_ratio(&self) -> (u32, u32) {
        (self.fps_num, self.fps_den)
    }

    /// The frame rate (frames per second).
    pub fn fps(&self) -> f64 {
        f64::from(self.fps_num) / f64::from(self.fps_den)
    }

    /// The milliseconds from the start to frame `frame`.
    pub fn frame_time_ms(&self, frame: u32) -> u64 {
        u64::from(frame) * 1000 * u64::from(self.fps_den) / u64::from(self.fps_num)
    }

    pub fn frame_count(&self) -> u32 {
        self.frames.len() as u32
    }

    /// The header's video flags.
    pub fn video_flags(&self) -> u32 {
        self.video_flags
    }

    /// Whether the frames have an alpha plane.
    pub fn has_alpha(&self) -> bool {
        self.track.has_alpha
    }

    /// The number of audio tracks (whose packets are skipped).
    pub fn audio_tracks(&self) -> u32 {
        self.audio_tracks
    }

    /// Whether frame `frame` is a key frame.
    pub fn is_key_frame(&self, frame: u32) -> bool {
        self.frames.get(frame as usize).is_some_and(|f| f.key_frame)
    }

    /// The number of frames decoded so far: the index of the next.
    pub fn frames_decoded(&self) -> u32 {
        self.next
    }

    /// Back to before the first frame (`rewind`).
    pub fn rewind(&mut self) {
        self.track.rewind();
        self.next = 0;
    }

    /// Decodes the next frame (`readNextPacket` and `decodePacket`), or
    /// `None` after the last. After an error the film should not be read
    /// further.
    pub fn next_frame(&mut self) -> Option<Result<Frame<'_>, BinkError>> {
        let index = self.next;
        let entry = *self.frames.get(index as usize)?;
        if let Err(e) = self.decode(index, entry) {
            self.next = self.frames.len() as u32;
            return Some(Err(e));
        }
        self.next += 1;
        Some(Ok(self.frame(index, entry.key_frame)))
    }

    fn decode(&mut self, index: u32, entry: FrameEntry) -> Result<(), BinkError> {
        let d = self.data.as_ref();
        let bad = BinkError::BadFrame(index);
        let mut pos = entry.offset as usize;
        if pos > d.len() {
            return Err(bad);
        }
        let mut frame_size = entry.size;

        for _ in 0..self.audio_tracks {
            let len = u32_at(d, pos).map_err(|_| bad.clone())?;
            pos += 4;
            frame_size = frame_size
                .checked_sub(4)
                .ok_or(BinkError::Corrupt("audio packet too big for the frame"))?;
            if frame_size < len {
                return Err(BinkError::Corrupt("audio packet too big for the frame"));
            }
            if len >= 4 {
                pos += len as usize;
                frame_size -= len;
            }
        }

        let packet = d.get(pos..pos + frame_size as usize).ok_or(bad)?;
        self.track.decode_packet(packet)
    }

    fn frame(&self, index: u32, key_frame: bool) -> Frame<'_> {
        let t = &self.track;
        Frame {
            index,
            key_frame,
            width: self.width as usize,
            height: self.height as usize,
            y: &t.old[0],
            u: &t.old[1],
            v: &t.old[2],
            a: t.has_alpha.then_some(&t.old[3][..]),
            y_pitch: t.y_block_width * 8,
            uv_pitch: t.uv_block_width * 8,
        }
    }
}

/// A decoded frame: ScummVM's planes after `decodePacket`, sized in whole
/// blocks (Y and A: 8 × 8 blocks, U and V: half the size in 16 × 16
/// blocks), the video in their top-left corner.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub index: u32,
    pub key_frame: bool,
    /// The video's size.
    pub width: usize,
    pub height: usize,
    pub y: &'a [u8],
    /// The Cb plane (ScummVM's `_curPlanes[1]`).
    pub u: &'a [u8],
    /// The Cr plane (ScummVM's `_curPlanes[2]`).
    pub v: &'a [u8],
    /// The alpha plane, when the film has one.
    pub a: Option<&'a [u8]>,
    /// The bytes per row of Y and A.
    pub y_pitch: usize,
    /// The bytes per row of U and V.
    pub uv_pitch: usize,
}

impl Frame<'_> {
    fn planes(&self) -> yuv::Planes<'_> {
        yuv::Planes {
            y: self.y,
            u: self.u,
            v: self.v,
            a: self.a,
            y_pitch: self.y_pitch,
            uv_pitch: self.uv_pitch,
            even_width: (self.width + 1) & !1,
            even_height: (self.height + 1) & !1,
            width: self.width,
            height: self.height,
        }
    }

    /// The frame in x8r8g8b8 (the top byte 0), `width × height` pixels row
    /// by row.
    ///
    /// # Panics
    ///
    /// If `out` is shorter than `width × height`.
    pub fn to_xrgb8888(&self, out: &mut [u32]) {
        let mut p = self.planes();
        p.a = None;
        yuv::convert420(yuv::Lookup::xrgb8888(), &p, |i, rgb, _| out[i] = rgb);
    }

    /// The frame in a8r8g8b8: alpha from the alpha plane, or 255 without
    /// one (ScummVM's `convert420Alpha` into a 32-bit format with alpha).
    ///
    /// # Panics
    ///
    /// If `out` is shorter than `width × height`.
    pub fn to_argb8888(&self, out: &mut [u32]) {
        yuv::convert420(yuv::Lookup::xrgb8888(), &self.planes(), |i, rgb, a| {
            out[i] = rgb | (u32::from(a) << 24)
        });
    }

    /// The frame in RGB565 (alpha dropped).
    ///
    /// # Panics
    ///
    /// If `out` is shorter than `width × height`.
    pub fn to_rgb565(&self, out: &mut [u16]) {
        let mut p = self.planes();
        p.a = None;
        yuv::convert420(yuv::Lookup::rgb565(), &p, |i, rgb, _| out[i] = rgb as u16);
    }
}

#[cfg(test)]
mod tests;

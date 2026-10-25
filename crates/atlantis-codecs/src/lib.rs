//! Picture, video and sound codecs of *Atlantis: The Lost Tales* (Cryo,
//! 1997): HNM6 panoramas and videos, UBB2 dialog videos, APC sound.
//!
//! Translated from ScummVM's HNM decoders (video/hnm_decoder.cpp and
//! image/codecs/hnm.cpp) and APC decoder (audio/decoders/apc.cpp), GPLv3,
//! the ScummVM team, as used by the CryOmni3D Atlantis engine. Comments cite addresses in the original `atlantis.exe`
//! (the 2001 English build) where they explain a behaviour.

#![forbid(unsafe_code)]

mod alpha;
mod apc;
mod hnm;
mod hnm6;
mod motion_order;
mod ubb;

pub use alpha::AlphaTables;
pub use apc::{decode_apc, Apc, ApcDecoder, ApcHeader, APC_HEADER_SIZE};
pub use hnm::{AudioFormat, HnmVideo};
pub use ubb::{palette_to_rgb565, UbbFrame, UbbVideo, IA_BUFFER_SIZE};

use std::fmt;

/// A panorama (CYCLO\\*.HNM): `width` x `height` RGB565 pixels, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panorama {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u16>,
}

impl Panorama {
    /// The pixels as RGB565 little-endian bytes.
    pub fn to_le_bytes(&self) -> Vec<u8> {
        self.pixels.iter().flat_map(|p| p.to_le_bytes()).collect()
    }
}

/// Why a file could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The file ends before the data it announces.
    Truncated,
    /// Not an HNM6 file (HNM4 and UBB2 are 8-bit formats).
    NotHnm6,
    /// Bad dimensions or frame buffer size in the file header.
    InvalidFileHeader,
    /// A superchunk header is malformed.
    InvalidSuperchunk,
    /// A chunk header is malformed.
    InvalidChunk,
    /// A chunk type the HNM6 decoder does not know.
    UnexpectedChunk(u16),
    /// The file ends without a video chunk.
    NoVideo,
    /// A frame header has inconsistent offsets.
    InvalidFrameHeader,
    /// The compressed frame data is inconsistent.
    Bitstream(&'static str),
    /// A valid feature that is not translated.
    Unsupported(&'static str),
    /// Not an APC file (no `CRYO_APC1.20` header).
    NotApc,
    /// Not a UBB2 dialog video.
    NotUbb2,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Truncated => write!(f, "truncated file"),
            DecodeError::NotHnm6 => write!(f, "not an HNM6 file"),
            DecodeError::InvalidFileHeader => write!(f, "invalid HNM6 file header"),
            DecodeError::InvalidSuperchunk => write!(f, "invalid superchunk"),
            DecodeError::InvalidChunk => write!(f, "invalid chunk"),
            DecodeError::UnexpectedChunk(t) => {
                let [a, b] = t.to_be_bytes();
                write!(
                    f,
                    "unexpected chunk '{}{}'",
                    a.escape_ascii(),
                    b.escape_ascii()
                )
            }
            DecodeError::NoVideo => write!(f, "no video chunk"),
            DecodeError::InvalidFrameHeader => write!(f, "invalid frame header"),
            DecodeError::Bitstream(what) => write!(f, "corrupt frame: {what}"),
            DecodeError::Unsupported(what) => write!(f, "unsupported: {what}"),
            DecodeError::NotApc => write!(f, "not an APC file"),
            DecodeError::NotUbb2 => write!(f, "not a UBB2 file"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decodes a panorama: the first frame of a CYCLO\\*.HNM file (HNM6, IW
/// warp keyframe), bit exact with the original warp decoder FUN_0045084c.
pub fn decode_panorama(hnm: &[u8]) -> Result<Panorama, DecodeError> {
    hnm::decode_first_frame(hnm)
}

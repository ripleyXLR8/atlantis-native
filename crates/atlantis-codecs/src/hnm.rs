//! The HNM6 container, as far as the first frame of a panorama needs it.
//!
//! Translated from ScummVM's `Video::HNMDecoder` (video/hnm_decoder.cpp,
//! GPLv3, the ScummVM team): `loadStream` for the header and
//! `readNextPacket` for the superchunks, read the way the Atlantis engine's
//! `loadCycloHNM` does (until the first superchunk holding a video chunk).

use crate::hnm6;
use crate::{DecodeError, Panorama};

/// Size of the file header; superchunks follow it.
const FILE_HEADER_SIZE: usize = 64;

const fn tag16(s: &[u8; 2]) -> u16 {
    u16::from_be_bytes(*s)
}

const CHUNK_IW: u16 = tag16(b"IW");
const CHUNK_IX: u16 = tag16(b"IX");
const CHUNK_SD: u16 = tag16(b"SD");
const CHUNK_AA: u16 = tag16(b"AA");
const CHUNK_BB: u16 = tag16(b"BB");

fn le16(d: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([d[pos], d[pos + 1]])
}

fn le32(d: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([d[pos], d[pos + 1], d[pos + 2], d[pos + 3]])
}

pub(crate) fn decode_first_frame(hnm: &[u8]) -> Result<Panorama, DecodeError> {
    if hnm.len() < FILE_HEADER_SIZE {
        return Err(DecodeError::Truncated);
    }
    // HNM4 and UBB2 are 8-bit formats: the 16-bit decoder refuses them.
    if &hnm[0..4] != b"HNM6" {
        return Err(DecodeError::NotHnm6);
    }
    let width = usize::from(le16(hnm, 8));
    let height = usize::from(le16(hnm, 10));
    let frame_size = le32(hnm, 28) as usize;
    // The decoder needs room for at least a frame header.
    if frame_size < 24 {
        return Err(DecodeError::InvalidFileHeader);
    }
    if width == 0 || height == 0 || width % 8 != 0 || height % 8 != 0 {
        return Err(DecodeError::InvalidFileHeader);
    }

    let mut pixels = vec![0u16; width * height];
    let mut pos = FILE_HEADER_SIZE;
    loop {
        // One superchunk per frame; a zero header ends the file.
        if pos + 4 > hnm.len() || le32(hnm, pos) == 0 {
            return Err(DecodeError::NoVideo);
        }
        let size = (le32(hnm, pos) & 0x00ff_ffff) as usize;
        if size < 4 {
            return Err(DecodeError::InvalidSuperchunk);
        }
        let superchunk = hnm.get(pos + 4..pos + size).ok_or(DecodeError::Truncated)?;
        pos += size;

        let mut got_video = false;
        let mut rest = superchunk;
        while !rest.is_empty() {
            if rest.len() < 8 {
                return Err(DecodeError::InvalidSuperchunk);
            }
            let chunk_size = le32(rest, 0) as usize;
            let chunk_type = u16::from_be_bytes([rest[4], rest[5]]);
            if chunk_size < 8 || chunk_size > rest.len() {
                return Err(DecodeError::InvalidChunk);
            }
            let data = &rest[8..chunk_size];
            match chunk_type {
                // Audio: not needed for the picture
                CHUNK_SD | CHUNK_AA | CHUNK_BB => {}
                CHUNK_IW => {
                    hnm6::decode_iw_frame(data, width, height, frame_size, &mut pixels)?;
                    got_video = true;
                }
                // Video keyframes and interframes; no panorama uses them.
                CHUNK_IX => return Err(DecodeError::Unsupported("IX video chunk")),
                other => return Err(DecodeError::UnexpectedChunk(other)),
            }
            // HNM6 chunks are 4-byte aligned.
            let aligned = chunk_size.div_ceil(4) * 4;
            if aligned > rest.len() {
                return Err(DecodeError::InvalidChunk);
            }
            rest = &rest[aligned..];
        }
        if got_video {
            return Ok(Panorama {
                width,
                height,
                pixels,
            });
        }
    }
}

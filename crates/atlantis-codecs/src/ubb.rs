//! The dialog videos (DIALOG\\PERSO\\*.UBB): the UBB2 container, the 8-bit
//! HNM5 frame codec (IV chunks), the palette (PL) and the compositor opcode
//! stream (IA).
//!
//! Translated from ScummVM's `Video::HNMDecoder` (video/hnm_decoder.cpp,
//! GPLv3, the ScummVM team): `loadStream` for the header, `readNextPacket`
//! for the superchunks, `HNM45VideoTrack::decodePalette` for PL and
//! `HNM5VideoTrack::decodeFrame` for IV, used the way the CryOmni3D Atlantis
//! engine's dialog player uses it (loop mode, IA stream captured). Where the
//! original atlantis.exe differs from the generic decoder (superchunk size,
//! chunk walk, IA copy, rewind), this follows the original: open
//! FUN_004346f0, superchunk FUN_00434a90, chunk dispatch FUN_00434880, IV
//! decoder FUN_0041101f, dialog loop FUN_0042ee88.

use crate::DecodeError;

/// Size of the file header; superchunks follow it.
const FILE_HEADER_SIZE: usize = 64;

/// Size of the IA buffer of the original's video handle (hnm + 0x350).
pub const IA_BUFFER_SIZE: usize = 0x10000;

const fn tag16(s: &[u8; 2]) -> u16 {
    u16::from_be_bytes(*s)
}

const CHUNK_PL: u16 = tag16(b"PL");
const CHUNK_IV: u16 = tag16(b"IV");
const CHUNK_IA: u16 = tag16(b"IA");
const CHUNK_I0: u16 = tag16(b"I0");
const CHUNK_IB: u16 = tag16(b"IB");
const CHUNK_IZ: u16 = tag16(b"IZ");
const CHUNK_SD: u16 = tag16(b"SD");

fn le16(d: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([d[pos], d[pos + 1]])
}

fn le32(d: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([d[pos], d[pos + 1], d[pos + 2], d[pos + 3]])
}

/// FUN_00450e61: a palette of 8-bit components (the file's 6-bit values
/// shifted left by 2, `r g b` for each of the 256 entries) as the RGB565
/// colours the dialog compositor draws.
pub fn palette_to_rgb565(palette: &[u8; 768]) -> [u16; 256] {
    let mut lut = [0u16; 256];
    for (c, rgb) in lut.iter_mut().zip(palette.as_chunks::<3>().0) {
        let (r, g, b) = (u16::from(rgb[0]), u16::from(rgb[1]), u16::from(rgb[2]));
        *c = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
    }
    lut
}

/// One decoded frame of a [`UbbVideo`], borrowed from the decoder.
///
/// The dialog player draws it with
/// `atlantis_data::sprite_blend::compose_ubb_ia(dst, frame.pixels,
/// frame.palette, frame.ia, blend_ubb_pixel565)` (FUN_00450b08) over the
/// panorama view.
#[derive(Clone, Copy, Debug)]
pub struct UbbFrame<'a> {
    /// The frame's number in the file, 0 .. frame count (it restarts at 0
    /// when the video loops).
    pub index: u32,
    /// `width * height` palette indices, row by row.
    pub pixels: &'a [u8],
    /// The current palette as RGB565 colours ([`palette_to_rgb565`]).
    pub palette: &'a [u16; 256],
    /// The compositor opcode stream: the original's 64 KiB IA buffer, as the
    /// last IA chunk left it (see [`UbbVideo`]).
    pub ia: &'a [u8],
}

/// A UBB2 dialog video decoded frame by frame, looping as the dialog player
/// does: after the header's frame count (or at the end of the file) it
/// rewinds to the first superchunk, keeping the frame buffers, the palette
/// and the IA buffer.
///
/// The IA buffer reproduces the original's copy (FUN_00434880 @ 0x00434a19):
/// `min(chunk size, 0x10000)` bytes from the chunk's payload, the size
/// counting the 8-byte chunk header, into a 64 KiB buffer kept from frame to
/// frame. Every shipped stream ends with its end opcode, so the bytes past
/// it are never read.
///
/// `D` holds the file's bytes (`Vec<u8>`, `&[u8]`, `Arc<[u8]>`...).
pub struct UbbVideo<D: AsRef<[u8]> = Vec<u8>> {
    data: D,
    width: usize,
    height: usize,
    frame_count: u32,
    /// The frame being shown (written by the last IV chunk).
    cur: Vec<u8>,
    /// The other frame buffer.
    prev: Vec<u8>,
    /// 8-bit components (6-bit file values << 2).
    palette: [u8; 768],
    lut: [u16; 256],
    ia: Vec<u8>,
    /// Offset of the next superchunk.
    pos: usize,
    /// Frames decoded since the last rewind.
    in_loop: u32,
    /// Frames decoded in all.
    decoded: u64,
}

impl<D: AsRef<[u8]>> UbbVideo<D> {
    /// Opens a UBB2 file, with zeroed frame buffers, palette and IA buffer.
    pub fn new(data: D) -> Result<Self, DecodeError> {
        let ubb = data.as_ref();
        if ubb.len() < FILE_HEADER_SIZE {
            return Err(DecodeError::Truncated);
        }
        if &ubb[0..4] != b"UBB2" {
            return Err(DecodeError::NotUbb2);
        }
        let width = usize::from(le16(ubb, 8));
        let height = usize::from(le16(ubb, 10));
        let frame_count = le32(ubb, 16);
        // +0x1c: the size of each frame buffer (FUN_004346f0 @ 0x00434784)
        let frame_size = le32(ubb, 28) as usize;
        if width == 0 || height == 0 || width * height > frame_size || frame_size > 1 << 24 {
            return Err(DecodeError::InvalidFileHeader);
        }
        Ok(UbbVideo {
            width,
            height,
            frame_count,
            cur: vec![0; frame_size],
            prev: vec![0; frame_size],
            palette: [0; 768],
            lut: [0; 256],
            ia: vec![0; IA_BUFFER_SIZE],
            pos: FILE_HEADER_SIZE,
            in_loop: 0,
            decoded: 0,
            data,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// The number of frames of one loop (header +0x10).
    pub fn frame_count(&self) -> u32 {
        self.frame_count
    }

    /// The number of frames decoded so far, all loops together.
    pub fn frames_decoded(&self) -> u64 {
        self.decoded
    }

    /// The current palette, 8-bit components.
    pub fn palette(&self) -> &[u8; 768] {
        &self.palette
    }

    /// The last decoded frame (before the first one: zeros).
    pub fn frame(&self) -> UbbFrame<'_> {
        UbbFrame {
            index: self.in_loop.saturating_sub(1),
            pixels: &self.cur[..self.width * self.height],
            palette: &self.lut,
            ia: &self.ia,
        }
    }

    /// Decodes the next frame (one superchunk), rewinding first when the
    /// loop is done. After an error the decoder's state is unspecified.
    pub fn next_frame(&mut self) -> Result<UbbFrame<'_>, DecodeError> {
        // FUN_0042ee88 @ 0x0042f617..0x0042f651: the frame counter reaching
        // the header's count seeks back to the first superchunk. The files
        // end right after their last superchunk.
        if (self.frame_count != 0 && self.in_loop >= self.frame_count)
            || self.pos + 4 > self.data.as_ref().len()
        {
            self.pos = FILE_HEADER_SIZE;
            self.in_loop = 0;
        }
        self.read_superchunk()?;
        self.in_loop += 1;
        self.decoded += 1;
        Ok(self.frame())
    }

    fn read_superchunk(&mut self) -> Result<(), DecodeError> {
        let ubb = self.data.as_ref();
        if self.pos + 4 > ubb.len() {
            return Err(DecodeError::Truncated);
        }
        // FUN_00434a90: a 23-bit size, sign extended (bit 31 flags a palette
        // change)
        let size = ((le32(ubb, self.pos) << 9) as i32) >> 9;
        if size < 4 {
            return Err(DecodeError::InvalidSuperchunk);
        }
        let end = self.pos + size as usize;
        if end > ubb.len() {
            return Err(DecodeError::Truncated);
        }
        let mut at = self.pos + 4;
        self.pos = end;

        // FUN_00434880: chunks walked by their raw size (no alignment); an
        // unknown tag ends the superchunk (0x00434a6a).
        while at + 8 <= end {
            let size = le32(ubb, at) as usize;
            let kind = u16::from_be_bytes([ubb[at + 4], ubb[at + 5]]);
            if size < 8 || size > end - at {
                return Err(DecodeError::InvalidChunk);
            }
            let payload = &ubb[at + 8..at + size];
            match kind {
                CHUNK_PL => decode_palette(payload, &mut self.palette, &mut self.lut)?,
                CHUNK_IV => {
                    // FUN_0041101f(payload, buf[idx], buf[idx ^ 1]); idx ^= 1
                    std::mem::swap(&mut self.cur, &mut self.prev);
                    decode_iv(payload, &mut self.cur, &self.prev, self.width)?;
                }
                CHUNK_IA => {
                    // memcpy(hnm + 0x350, chunk + 8, min(chunk size, 0x10000))
                    // (0x00434a19): the size counts the header.
                    let n = size.min(IA_BUFFER_SIZE);
                    let src = &ubb[at + 8..(at + 8 + n).min(ubb.len())];
                    self.ia[..src.len()].copy_from_slice(src);
                }
                CHUNK_I0 | CHUNK_IB | CHUNK_IZ | CHUNK_SD => {}
                _ => break,
            }
            at += size;
        }
        Ok(())
    }
}

/// A PL chunk: records of (start, count (0 = 256), count * 3 components)
/// until a start of 0xff, each 6-bit component shifted left by 2.
fn decode_palette(
    mut data: &[u8],
    palette: &mut [u8; 768],
    lut: &mut [u16; 256],
) -> Result<(), DecodeError> {
    while data.len() >= 2 {
        let start = usize::from(data[0]);
        let mut count = usize::from(data[1]);
        data = &data[2..];
        if start == 0xff {
            break;
        }
        if count == 0 {
            count = 256;
        }
        if data.len() < count * 3 || start + count > 256 {
            return Err(DecodeError::Bitstream("invalid palette chunk"));
        }
        for (dst, &v) in palette[start * 3..(start + count) * 3]
            .iter_mut()
            .zip(&data[..count * 3])
        {
            *dst = v << 2;
        }
        data = &data[count * 3..];
    }
    *lut = palette_to_rgb565(palette);
    Ok(())
}

/// Widths of the block copies by block height (2, 3, 4) and opcode index.
#[rustfmt::skip]
const WIDTHS: [[u8; 32]; 3] = [
    [
         1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15, 16,
        18, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 44, 48, 52, 56,
    ],
    [
         1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15, 16,
        17, 18, 19, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 42, 44,
    ],
    [
         1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15, 16,
        17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 28, 30, 32, 34, 36, 38,
    ],
];

/// A byte reader over a chunk payload.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        let v = *self
            .data
            .get(self.pos)
            .ok_or(DecodeError::Bitstream("frame data ends early"))?;
        self.pos += 1;
        Ok(v)
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from(self.u8()?) | u16::from(self.u8()?) << 8)
    }
}

/// The source of a block copy: from the current frame (already decoded
/// part) or the previous one, at an offset from the destination.
fn source_offset(r: &mut Reader, mode: u8, pitch: isize) -> Result<(bool, isize), DecodeError> {
    // (from the current frame, x bits, x base, y base) of the one-byte
    // offsets; None: a 16-bit offset from -32768 (previous) or -65536
    // (current)
    let spec: Option<(bool, u32, isize, isize)> = match mode {
        2..=4 => None,
        5..=7 => Some((false, 4, -8, -8)),
        8..=31 => {
            const BASES: [(isize, isize); 8] = [
                (-2, -8),
                (-14, -8),
                (-8, -2),
                (-8, -14),
                (-2, -2),
                (-14, -2),
                (-2, -14),
                (-14, -14),
            ];
            let (x, y) = BASES[usize::from((mode - 8) % 8)];
            Some((false, 4, x, y))
        }
        32 | 40 => None,
        33..=47 => {
            const CUR: [(u32, isize, isize); 7] = [
                (5, -16, -8),
                (4, -8, -16),
                (4, -24, -16),
                (4, 8, -16),
                (3, -4, -32),
                (3, -12, -32),
                (3, 4, -32),
            ];
            let (bits, x, y) = CUR[usize::from((mode - 33) % 8)];
            Some((true, bits, x, y))
        }
        _ => return Err(DecodeError::Bitstream("invalid offset mode")),
    };
    Ok(match spec {
        None if mode >= 32 => (true, -65536 + r.u16()? as isize),
        None => (false, -32768 + r.u16()? as isize),
        Some((cur, bits, xbase, ybase)) => {
            let b = r.u8()?;
            let y = isize::from(b >> bits) + ybase;
            let x = isize::from(b & ((1u8 << bits) - 1)) + xbase;
            (cur, y * pitch + x)
        }
    })
}

/// An IV chunk: the HNM5 frame codec into `cur`, with `prev` the previous
/// frame (FUN_0041101f).
fn decode_iv(data: &[u8], cur: &mut [u8], prev: &[u8], pitch: usize) -> Result<(), DecodeError> {
    const OOB: DecodeError = DecodeError::Bitstream("block outside the frame");
    let len = cur.len();
    let mut r = Reader { data, pos: 0 };
    // None until the first mode opcode
    let mut height: Option<usize> = None;
    let mut mode = 0u8;
    let mut pos = 0usize;
    let need = |h: Option<usize>| h.ok_or(DecodeError::Bitstream("no block mode"));

    loop {
        if r.remaining() < 1 {
            // Not an error for ScummVM's decoder: the frame stops here.
            break;
        }
        let op = r.u8()?;
        match op {
            0x20 => {
                // Copy from the previous frame
                let h = need(height)?;
                let w = usize::from(r.u8()?) + 1;
                for row in 0..h {
                    let s = pos + row * pitch;
                    if s + w > len {
                        return Err(OOB);
                    }
                    cur[s..s + w].copy_from_slice(&prev[s..s + w]);
                }
                pos += w;
            }
            0x60 => {
                // One column of literal pixels
                let h = need(height)?;
                if h > 4 || r.remaining() < h {
                    return Err(DecodeError::Bitstream("bad literal column"));
                }
                for row in 0..h {
                    let d = pos + row * pitch;
                    *cur.get_mut(d).ok_or(OOB)? = r.u8()?;
                }
                pos += 1;
            }
            0xa0 => {
                // Literal pixels, column by column
                let h = need(height)?;
                let w = usize::from(r.u8()?) + 2;
                if r.remaining() < h * w {
                    return Err(DecodeError::Bitstream("bad literal block"));
                }
                for col in 0..w {
                    for row in 0..h {
                        let d = pos + row * pitch + col;
                        *cur.get_mut(d).ok_or(OOB)? = r.u8()?;
                    }
                }
                pos += w;
            }
            0xe0 => {
                let sub = r.u8()?;
                match sub {
                    0 => {
                        // Fill
                        let h = need(height)?;
                        let w = usize::from(r.u8()?) + 1;
                        let px = r.u8()?;
                        for row in 0..h {
                            let s = pos + row * pitch;
                            if s + w > len {
                                return Err(OOB);
                            }
                            cur[s..s + w].fill(px);
                        }
                        pos += w;
                    }
                    1 => {
                        // End of the frame
                        break;
                    }
                    _ => {
                        // A new block mode, at a line start
                        if !pos.is_multiple_of(pitch) || sub >= 48 {
                            return Err(DecodeError::Bitstream("bad block mode"));
                        }
                        if let Some(h) = height {
                            pos += (h - 1) * pitch;
                        }
                        mode = sub;
                        height = Some(match sub {
                            2 | 5 | 8..=15 | 32..=39 => 2,
                            3 | 6 | 16..=23 | 40..=47 => 3,
                            _ => 4,
                        });
                    }
                }
            }
            _ => {
                // Block copy: a width index and one of eight orientations
                let h = need(height)?;
                let w = usize::from(WIDTHS[h - 2][usize::from(op & 0x1f)]);
                if r.remaining() < 2 {
                    return Err(DecodeError::Bitstream("block copy data ends early"));
                }
                let (from_cur, offset) = source_offset(&mut r, mode, pitch as isize)?;
                let p = pitch as isize;
                // (row step, column step) of the source
                let (rs, cs) = match op >> 5 {
                    0 => (p, 1),
                    1 => (p, -1),
                    2 => (-p, 1),
                    3 => (-p, -1),
                    4 => (1, p),
                    5 => (1, -p),
                    6 => (-1, p),
                    _ => (-1, -p),
                };
                let base = pos as isize + offset;
                for row in 0..h {
                    for col in 0..w {
                        let s = base + rs * row as isize + cs * col as isize;
                        let d = pos + row * pitch + col;
                        if s < 0 || s as usize >= len || d >= len {
                            return Err(OOB);
                        }
                        cur[d] = if from_cur {
                            cur[s as usize]
                        } else {
                            prev[s as usize]
                        };
                    }
                }
                pos += w;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_lut_takes_top_bits() {
        let mut pal = [0u8; 768];
        pal[..3].copy_from_slice(&[0xfc, 0xfc, 0xfc]);
        pal[3..6].copy_from_slice(&[0x04, 0x04, 0x04]);
        let lut = palette_to_rgb565(&pal);
        assert_eq!(lut[0], 0xffff);
        assert_eq!(lut[1], 0x0020);
    }

    #[test]
    fn rejects_other_formats() {
        let mut hdr = vec![0u8; 64];
        hdr[..4].copy_from_slice(b"HNM6");
        assert_eq!(UbbVideo::new(hdr).err(), Some(DecodeError::NotUbb2));
    }
}

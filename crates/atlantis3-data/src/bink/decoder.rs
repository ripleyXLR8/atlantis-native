// Ported from ScummVM's `video/bink_decoder.cpp` (`BinkVideoTrack`)
// (ScummVM - Graphic Adventure Engine, Copyright (C) the ScummVM team,
// GNU General Public License, version 3 or later). ScummVM's decoder is
// based on eos' Bink decoder, itself based on FFmpeg's, by Kostya
// Shishkov.

//! The video track: the planes of one frame decoded from its packet
//! (`BinkDecoder::BinkVideoTrack`).
//!
//! The port keeps ScummVM's arithmetic: pixels written from the IDCT or
//! summed with a residue are truncated to 8 bits (no clamping), and the
//! 32-bit products wrap. Where ScummVM calls `error()` the frame fails
//! with [`BinkError::Corrupt`]; where it would read or write outside its
//! buffers (undefined behaviour in C++, never met on the CD's films) the
//! frame fails as well.

use std::num::Wrapping;

use super::bits::{BitReader, Trees};
use super::tables::{INTER_QUANT, INTRA_QUANT, PATTERNS, SCAN};
use super::BinkError;

/// `BIKi`: no colour sign conversion and a 32-bit skip before the planes.
pub(super) const ID_BIKI: u32 = u32::from_be_bytes(*b"BIKi");

/// Bits of the first DC value of a bundle (`kDCStartBits`).
const DC_START_BITS: u32 = 11;

// The bundles (`Source`).
const SRC_BLOCK_TYPES: usize = 0;
const SRC_SUB_BLOCK_TYPES: usize = 1;
const SRC_COLORS: usize = 2;
const SRC_PATTERN: usize = 3;
const SRC_X_OFF: usize = 4;
const SRC_Y_OFF: usize = 5;
const SRC_INTRA_DC: usize = 6;
const SRC_INTER_DC: usize = 7;
const SRC_RUN: usize = 8;
const SRC_MAX: usize = 9;

// The block types (`BlockType`).
const BLOCK_SKIP: i32 = 0;
const BLOCK_SCALED: i32 = 1;
const BLOCK_MOTION: i32 = 2;
const BLOCK_RUN: i32 = 3;
const BLOCK_RESIDUE: i32 = 4;
const BLOCK_INTRA: i32 = 5;
const BLOCK_FILL: i32 = 6;
const BLOCK_INTER: i32 = 7;
const BLOCK_PATTERN: i32 = 8;
const BLOCK_RAW: i32 = 9;

/// The run lengths of the block types' symbols 12 to 15 (`rleLens`).
const RLE_LENS: [usize; 4] = [4, 8, 12, 32];

fn corrupt(what: &'static str) -> BinkError {
    BinkError::Corrupt(what)
}

/// A tree and its symbol translation (`Huffman`).
#[derive(Clone, Copy)]
struct Huffman {
    index: usize,
    symbols: [u8; 16],
}

const IDENTITY: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

impl Huffman {
    fn new() -> Huffman {
        Huffman {
            index: 0,
            symbols: IDENTITY,
        }
    }
}

/// One data type's decoded values (`Bundle`). `cur_dec` and `cur_ptr` are
/// offsets in `data`; `cur_dec` is `None` where ScummVM's pointer is null
/// (no more values in this plane).
struct Bundle {
    count_lengths: [u32; 2],
    count_length: u32,
    huffman: Huffman,
    data: Vec<u8>,
    cur_dec: Option<usize>,
    cur_ptr: usize,
}

/// A plane's geometry (`DecodeContext`, without the pointers).
struct Ctx {
    pitch: usize,
    coord_map: [usize; 64],
    coord_scaled: [[usize; 64]; 4],
}

/// The bit reader, the trees and the bundles: what the blocks read.
struct State {
    id: u32,
    trees: Trees,
    bundles: [Bundle; SRC_MAX],
    col_high: [Huffman; 16],
    col_last_val: u8,
}

/// The video track (`BinkVideoTrack`).
pub(super) struct VideoTrack {
    pub(super) y_block_width: usize,
    pub(super) y_block_height: usize,
    pub(super) uv_block_width: usize,
    pub(super) uv_block_height: usize,
    pub(super) has_alpha: bool,
    swap_planes: bool,
    /// The planes being decoded, Y, U, V, A (`_curPlanes`).
    cur: [Vec<u8>; 4],
    /// The previous frame's planes (`_oldPlanes`); after
    /// [`VideoTrack::decode_packet`], the frame just decoded.
    pub(super) old: [Vec<u8>; 4],
    st: State,
}

fn int_log2(v: u32) -> u32 {
    31 - v.leading_zeros()
}

impl VideoTrack {
    pub(super) fn new(width: u32, height: u32, id: u32, has_alpha: bool) -> VideoTrack {
        // BIKh and BIKi swap the chroma planes.
        let swap_planes = id == u32::from_be_bytes(*b"BIKh") || id == ID_BIKI;
        let y_block_width = ((width + 7) >> 3) as usize;
        let y_block_height = ((height + 7) >> 3) as usize;
        let uv_block_width = ((width + 15) >> 4) as usize;
        let uv_block_height = ((height + 15) >> 4) as usize;
        let y_size = y_block_width * 8 * y_block_height * 8;
        let uv_size = uv_block_width * 8 * uv_block_height * 8;
        let planes = || {
            [
                vec![0; y_size],
                vec![0; uv_size],
                vec![0; uv_size],
                vec![255; y_size],
            ]
        };

        // initBundles
        let blocks = y_block_width * y_block_height;
        let cbw = [(width + 7) >> 3, (width + 15) >> 4];
        let cw = [width, width >> 1];
        let mut lengths = [[0u32; 2]; SRC_MAX];
        for i in 0..2 {
            let width = cw[i].max(8);
            let l = |v: u32| int_log2(v + 511) + 1;
            lengths[SRC_BLOCK_TYPES][i] = l(width >> 3);
            lengths[SRC_SUB_BLOCK_TYPES][i] = l((width + 7) >> 4);
            lengths[SRC_COLORS][i] = l(cbw[i] * 64);
            lengths[SRC_INTRA_DC][i] = l(width >> 3);
            lengths[SRC_INTER_DC][i] = l(width >> 3);
            lengths[SRC_X_OFF][i] = l(width >> 3);
            lengths[SRC_Y_OFF][i] = l(width >> 3);
            lengths[SRC_PATTERN][i] = l(cbw[i] << 3);
            lengths[SRC_RUN][i] = l(cbw[i] * 48);
        }
        let bundles = lengths.map(|count_lengths| Bundle {
            count_lengths,
            count_length: 0,
            huffman: Huffman::new(),
            data: vec![0; blocks * 64],
            cur_dec: None,
            cur_ptr: 0,
        });

        VideoTrack {
            y_block_width,
            y_block_height,
            uv_block_width,
            uv_block_height,
            has_alpha,
            swap_planes,
            cur: planes(),
            old: planes(),
            st: State {
                id,
                trees: Trees::new(),
                bundles,
                col_high: [Huffman::new(); 16],
                col_last_val: 0,
            },
        }
    }

    /// Back to the first frame's state (`rewind`): Y, U, V black and A
    /// opaque.
    pub(super) fn rewind(&mut self) {
        for planes in [&mut self.cur, &mut self.old] {
            for (i, p) in planes.iter_mut().enumerate() {
                p.fill(if i == 3 { 255 } else { 0 });
            }
        }
    }

    /// Decodes a video packet (`decodePacket`); the frame is then in
    /// `old`, as ScummVM swaps the planes after its conversion.
    pub(super) fn decode_packet(&mut self, packet: &[u8]) -> Result<(), BinkError> {
        let mut bits = BitReader::new(packet);

        if self.has_alpha {
            if self.st.id == ID_BIKI {
                bits.skip(32);
            }
            self.decode_plane(&mut bits, 3, false)?;
        }

        if self.st.id == ID_BIKI {
            bits.skip(32);
        }

        for i in 0..3 {
            let plane = if i == 0 || !self.swap_planes {
                i
            } else {
                i ^ 3
            };
            self.decode_plane(&mut bits, plane, i != 0)?;
            if bits.pos() >= bits.size() {
                break;
            }
        }

        std::mem::swap(&mut self.cur, &mut self.old);
        Ok(())
    }

    /// `decodePlane`.
    fn decode_plane(
        &mut self,
        bits: &mut BitReader<'_>,
        plane: usize,
        is_chroma: bool,
    ) -> Result<(), BinkError> {
        let (block_width, block_height) = if is_chroma {
            (self.uv_block_width, self.uv_block_height)
        } else {
            (self.y_block_width, self.y_block_height)
        };
        let pitch = block_width * 8;

        let mut ctx = Ctx {
            pitch,
            coord_map: [0; 64],
            coord_scaled: [[0; 64]; 4],
        };
        for i in 0..64 {
            ctx.coord_map[i] = (i & 7) + (i >> 3) * pitch;
            for (k, map) in ctx.coord_scaled.iter_mut().enumerate() {
                map[i] = ((i & 7) * 2 + (k & 1)) + ((i >> 3) * 2 + (k >> 1)) * pitch;
            }
        }

        let st = &mut self.st;
        for i in 0..SRC_MAX {
            st.bundles[i].count_length = st.bundles[i].count_lengths[usize::from(is_chroma)];
            st.read_bundle(bits, i)?;
        }

        let dest = &mut self.cur[plane][..];
        let prev = &self.old[plane][..];

        for block_y in 0..block_height {
            st.read_block_types(bits, SRC_BLOCK_TYPES)?;
            st.read_block_types(bits, SRC_SUB_BLOCK_TYPES)?;
            st.read_colors(bits)?;
            st.read_patterns(bits)?;
            st.read_motion_values(bits, SRC_X_OFF)?;
            st.read_motion_values(bits, SRC_Y_OFF)?;
            st.read_dcs(bits, SRC_INTRA_DC, false)?;
            st.read_dcs(bits, SRC_INTER_DC, true)?;
            st.read_runs(bits)?;

            let mut off = 8 * block_y * pitch;
            let mut block_x = 0;
            while block_x < block_width {
                let block_type = st.get_bundle_value(SRC_BLOCK_TYPES)?;

                // A 16x16 block type on an odd line is part of the block
                // already decoded: skip it.
                if (block_y & 1) == 1 && block_type == BLOCK_SCALED {
                    block_x += 2;
                    off += 16;
                    continue;
                }

                let extent = if block_type == BLOCK_SCALED { 16 } else { 8 };
                if off + (extent - 1) * pitch + extent > dest.len() {
                    return Err(corrupt("block outside the plane"));
                }

                match block_type {
                    BLOCK_SKIP => block_copy(dest, prev, off, off, pitch, 8),
                    BLOCK_SCALED => {
                        st.block_scaled(bits, dest, off, &ctx)?;
                        block_x += 1;
                        off += 8;
                    }
                    BLOCK_MOTION => st.block_motion(dest, prev, off, &ctx)?,
                    BLOCK_RUN => st.block_run(bits, dest, off, &ctx)?,
                    BLOCK_RESIDUE => st.block_residue(bits, dest, prev, off, &ctx)?,
                    BLOCK_INTRA => st.block_intra(bits, dest, off, &ctx)?,
                    BLOCK_FILL => st.block_fill(dest, off, &ctx)?,
                    BLOCK_INTER => st.block_inter(bits, dest, prev, off, &ctx)?,
                    BLOCK_PATTERN => st.block_pattern(dest, off, &ctx)?,
                    BLOCK_RAW => st.block_raw(dest, off, &ctx)?,
                    _ => return Err(corrupt("unknown block type")),
                }

                block_x += 1;
                off += 8;
            }
        }

        // The next plane's data starts at a 32-bit boundary.
        if bits.pos() & 0x1F != 0 {
            bits.skip(32 - (bits.pos() & 0x1F));
        }
        Ok(())
    }
}

/// Copies a `size` × `size` block from `prev` at `from` to `dest` at `to`
/// (`blockSkip`, `blockScaledSkip`, the motion copy).
fn block_copy(dest: &mut [u8], prev: &[u8], to: usize, from: usize, pitch: usize, size: usize) {
    for j in 0..size {
        let d = to + j * pitch;
        let s = from + j * pitch;
        dest[d..d + size].copy_from_slice(&prev[s..s + size]);
    }
}

impl State {
    /// `readBundle`.
    fn read_bundle(&mut self, bits: &mut BitReader<'_>, source: usize) -> Result<(), BinkError> {
        if source == SRC_COLORS {
            for h in self.col_high.iter_mut() {
                read_huffman(bits, h)?;
            }
            self.col_last_val = 0;
        }

        if source != SRC_INTRA_DC && source != SRC_INTER_DC {
            read_huffman(bits, &mut self.bundles[source].huffman)?;
        }

        self.bundles[source].cur_dec = Some(0);
        self.bundles[source].cur_ptr = 0;
        Ok(())
    }

    /// `getHuffmanSymbol` with a bundle's (or a colour high nibble's) tree.
    fn symbol(trees: &Trees, bits: &mut BitReader<'_>, h: &Huffman) -> Result<u8, BinkError> {
        let s = trees
            .symbol(h.index, bits)
            .ok_or(corrupt("unknown Huffman code"))?;
        Ok(h.symbols[usize::from(s)])
    }

    /// `getBundleValue`.
    fn get_bundle_value(&mut self, source: usize) -> Result<i32, BinkError> {
        let b = &mut self.bundles[source];
        let p = b.cur_ptr;
        if source < SRC_X_OFF || source == SRC_RUN {
            let v = *b.data.get(p).ok_or(corrupt("bundle read past its end"))?;
            b.cur_ptr += 1;
            return Ok(i32::from(v));
        }
        if source == SRC_X_OFF || source == SRC_Y_OFF {
            let v = *b.data.get(p).ok_or(corrupt("bundle read past its end"))?;
            b.cur_ptr += 1;
            return Ok(i32::from(v as i8));
        }
        let v = b
            .data
            .get(p..p + 2)
            .ok_or(corrupt("bundle read past its end"))?;
        b.cur_ptr += 2;
        Ok(i32::from(i16::from_le_bytes([v[0], v[1]])))
    }

    /// `readBundleCount`: the start and the number of the values to
    /// decode, or `None` for none.
    fn read_bundle_count(bits: &mut BitReader<'_>, b: &mut Bundle) -> Option<(usize, usize)> {
        let dec = b.cur_dec?;
        if dec > b.cur_ptr {
            return None;
        }
        let n = bits.get_bits(b.count_length) as usize;
        if n == 0 {
            b.cur_dec = None;
            return None;
        }
        Some((dec, n))
    }

    /// `readRuns`.
    fn read_runs(&mut self, bits: &mut BitReader<'_>) -> Result<(), BinkError> {
        let b = &mut self.bundles[SRC_RUN];
        let Some((dec, n)) = Self::read_bundle_count(bits, b) else {
            return Ok(());
        };
        let end = dec + n;
        if end > b.data.len() {
            return Err(corrupt("run value went out of bounds"));
        }
        if bits.get_bit() != 0 {
            let v = bits.get_bits(4) as u8;
            b.data[dec..end].fill(v);
        } else {
            for i in dec..end {
                b.data[i] = Self::symbol(&self.trees, bits, &b.huffman)?;
            }
        }
        b.cur_dec = Some(end);
        Ok(())
    }

    /// `readMotionValues`.
    fn read_motion_values(
        &mut self,
        bits: &mut BitReader<'_>,
        source: usize,
    ) -> Result<(), BinkError> {
        let b = &mut self.bundles[source];
        let Some((dec, n)) = Self::read_bundle_count(bits, b) else {
            return Ok(());
        };
        let end = dec + n;
        if end > b.data.len() {
            return Err(corrupt("too many motion values"));
        }
        let signed = |bits: &mut BitReader<'_>, v: u8| {
            if v != 0 && bits.get_bit() != 0 {
                v.wrapping_neg()
            } else {
                v
            }
        };
        if bits.get_bit() != 0 {
            let v = bits.get_bits(4) as u8;
            let v = signed(bits, v);
            b.data[dec..end].fill(v);
            b.cur_dec = Some(end);
            return Ok(());
        }
        for i in dec..end {
            let v = Self::symbol(&self.trees, bits, &b.huffman)?;
            b.data[i] = signed(bits, v);
        }
        b.cur_dec = Some(end);
        Ok(())
    }

    /// `readBlockTypes`.
    fn read_block_types(
        &mut self,
        bits: &mut BitReader<'_>,
        source: usize,
    ) -> Result<(), BinkError> {
        let b = &mut self.bundles[source];
        let Some((dec, n)) = Self::read_bundle_count(bits, b) else {
            return Ok(());
        };
        let end = dec + n;
        if end > b.data.len() {
            return Err(corrupt("too many block type values"));
        }
        if bits.get_bit() != 0 {
            let v = bits.get_bits(4) as u8;
            b.data[dec..end].fill(v);
            b.cur_dec = Some(end);
            return Ok(());
        }
        let mut last = 0;
        let mut cur = dec;
        loop {
            let v = Self::symbol(&self.trees, bits, &b.huffman)?;
            if v < 12 {
                last = v;
                b.data[cur] = v;
                cur += 1;
            } else {
                let run = RLE_LENS[usize::from(v - 12)];
                let run_end = cur + run;
                if run_end > b.data.len() {
                    return Err(corrupt("block type run out of bounds"));
                }
                b.data[cur..run_end].fill(last);
                cur = run_end;
            }
            if cur >= end {
                break;
            }
        }
        b.cur_dec = Some(cur);
        Ok(())
    }

    /// `readPatterns`.
    fn read_patterns(&mut self, bits: &mut BitReader<'_>) -> Result<(), BinkError> {
        let b = &mut self.bundles[SRC_PATTERN];
        let Some((dec, n)) = Self::read_bundle_count(bits, b) else {
            return Ok(());
        };
        let end = dec + n;
        if end > b.data.len() {
            return Err(corrupt("too many pattern values"));
        }
        for i in dec..end {
            let lo = Self::symbol(&self.trees, bits, &b.huffman)?;
            let hi = Self::symbol(&self.trees, bits, &b.huffman)?;
            b.data[i] = lo | (hi << 4);
        }
        b.cur_dec = Some(end);
        Ok(())
    }

    /// `readColors`.
    fn read_colors(&mut self, bits: &mut BitReader<'_>) -> Result<(), BinkError> {
        let b = &mut self.bundles[SRC_COLORS];
        let Some((dec, n)) = Self::read_bundle_count(bits, b) else {
            return Ok(());
        };
        let end = dec + n;
        if end > b.data.len() {
            return Err(corrupt("too many color values"));
        }
        let biki = self.id == ID_BIKI;
        let low = b.huffman;
        let (trees, col_high, col_last_val) = (&self.trees, &self.col_high, &mut self.col_last_val);
        let mut color = |bits: &mut BitReader<'_>| -> Result<u8, BinkError> {
            *col_last_val = Self::symbol(trees, bits, &col_high[usize::from(*col_last_val)])?;
            let mut v = Self::symbol(trees, bits, &low)?;
            v |= *col_last_val << 4;
            if !biki {
                let sign = i32::from(v as i8) >> 7;
                v = ((((i32::from(v) & 0x7F) ^ sign) - sign) as u8).wrapping_add(0x80);
            }
            Ok(v)
        };
        if bits.get_bit() != 0 {
            let v = color(bits)?;
            b.data[dec..end].fill(v);
        } else {
            for i in dec..end {
                b.data[i] = color(bits)?;
            }
        }
        b.cur_dec = Some(end);
        Ok(())
    }

    /// `readDCS<kDCStartBits, hasSign>`.
    fn read_dcs(
        &mut self,
        bits: &mut BitReader<'_>,
        source: usize,
        has_sign: bool,
    ) -> Result<(), BinkError> {
        let b = &mut self.bundles[source];
        let Some((dec, length)) = Self::read_bundle_count(bits, b) else {
            return Ok(());
        };
        let mut dest = dec;
        let put = |data: &mut Vec<u8>, dest: &mut usize, v: i32| -> Result<(), BinkError> {
            let slot = data
                .get_mut(*dest..*dest + 2)
                .ok_or(corrupt("too many DC values"))?;
            slot.copy_from_slice(&(v as i16).to_le_bytes());
            *dest += 2;
            Ok(())
        };

        let mut v = bits.get_bits(DC_START_BITS - u32::from(has_sign)) as i32;
        if v != 0 && has_sign && bits.get_bit() != 0 {
            v = -v;
        }
        put(&mut b.data, &mut dest, v)?;
        let length = length - 1;

        let mut i = 0;
        while i < length {
            let length2 = (length - i).min(8);
            let b_size = bits.get_bits(4);
            if b_size != 0 {
                for _ in 0..length2 {
                    let mut v2 = bits.get_bits(b_size) as i16;
                    if v2 != 0 && bits.get_bit() != 0 {
                        v2 = v2.wrapping_neg();
                    }
                    v += i32::from(v2);
                    put(&mut b.data, &mut dest, v)?;
                    if !(-32768..=32767).contains(&v) {
                        return Err(corrupt("DC value went out of bounds"));
                    }
                }
            } else {
                for _ in 0..length2 {
                    put(&mut b.data, &mut dest, v)?;
                }
            }
            i += 8;
        }

        b.cur_dec = Some(dest);
        Ok(())
    }

    /// `blockScaled`: a 16 × 16 block of a sub-block type.
    fn block_scaled(
        &mut self,
        bits: &mut BitReader<'_>,
        dest: &mut [u8],
        off: usize,
        ctx: &Ctx,
    ) -> Result<(), BinkError> {
        match self.get_bundle_value(SRC_SUB_BLOCK_TYPES)? {
            BLOCK_RUN => self.block_scaled_run(bits, dest, off, ctx),
            BLOCK_INTRA => self.block_scaled_intra(bits, dest, off, ctx),
            BLOCK_FILL => {
                // blockScaledFill
                let v = self.get_bundle_value(SRC_COLORS)? as u8;
                for i in 0..16 {
                    let d = off + i * ctx.pitch;
                    dest[d..d + 16].fill(v);
                }
                Ok(())
            }
            BLOCK_PATTERN => {
                // blockScaledPattern
                let col = [
                    self.get_bundle_value(SRC_COLORS)? as u8,
                    self.get_bundle_value(SRC_COLORS)? as u8,
                ];
                for j in 0..8 {
                    let mut v = self.get_bundle_value(SRC_PATTERN)? as u8;
                    let d1 = off + 2 * j * ctx.pitch;
                    let d2 = d1 + ctx.pitch;
                    for i in 0..8 {
                        let c = col[usize::from(v & 1)];
                        dest[d1 + 2 * i] = c;
                        dest[d1 + 2 * i + 1] = c;
                        dest[d2 + 2 * i] = c;
                        dest[d2 + 2 * i + 1] = c;
                        v >>= 1;
                    }
                }
                Ok(())
            }
            BLOCK_RAW => {
                // blockScaledRaw
                let b = &mut self.bundles[SRC_COLORS];
                for j in 0..8 {
                    let row: [u8; 8] = b
                        .data
                        .get(b.cur_ptr..b.cur_ptr + 8)
                        .ok_or(corrupt("bundle read past its end"))?
                        .try_into()
                        .unwrap_or([0; 8]);
                    let d1 = off + 2 * j * ctx.pitch;
                    let d2 = d1 + ctx.pitch;
                    for (i, &c) in row.iter().enumerate() {
                        dest[d1 + 2 * i] = c;
                        dest[d1 + 2 * i + 1] = c;
                        dest[d2 + 2 * i] = c;
                        dest[d2 + 2 * i + 1] = c;
                    }
                    b.cur_ptr += 8;
                }
                Ok(())
            }
            _ => Err(corrupt("invalid 16x16 block type")),
        }
    }

    /// `blockScaledRun`.
    fn block_scaled_run(
        &mut self,
        bits: &mut BitReader<'_>,
        dest: &mut [u8],
        off: usize,
        ctx: &Ctx,
    ) -> Result<(), BinkError> {
        let scan = &PATTERNS[bits.get_bits(4) as usize];
        let put = |dest: &mut [u8], s: u8, v: u8| {
            for map in &ctx.coord_scaled {
                dest[off + map[usize::from(s)]] = v;
            }
        };
        let mut s = 0;
        let mut i = 0;
        loop {
            let run = self.get_bundle_value(SRC_RUN)? as usize + 1;
            i += run;
            if i > 64 {
                return Err(corrupt("run went out of bounds"));
            }
            if bits.get_bit() != 0 {
                let v = self.get_bundle_value(SRC_COLORS)? as u8;
                for _ in 0..run {
                    put(dest, scan[s], v);
                    s += 1;
                }
            } else {
                for _ in 0..run {
                    let v = self.get_bundle_value(SRC_COLORS)? as u8;
                    put(dest, scan[s], v);
                    s += 1;
                }
            }
            if i >= 63 {
                break;
            }
        }
        if i == 63 {
            let v = self.get_bundle_value(SRC_COLORS)? as u8;
            put(dest, scan[s], v);
        }
        Ok(())
    }

    /// `blockScaledIntra`.
    fn block_scaled_intra(
        &mut self,
        bits: &mut BitReader<'_>,
        dest: &mut [u8],
        off: usize,
        ctx: &Ctx,
    ) -> Result<(), BinkError> {
        let mut block = [0i32; 64];
        block[0] = self.get_bundle_value(SRC_INTRA_DC)?;
        read_dct_coeffs(bits, &mut block, true)?;
        idct(&mut block);
        for j in 0..8 {
            let d1 = off + 2 * j * ctx.pitch;
            let d2 = d1 + ctx.pitch;
            for i in 0..8 {
                let c = block[j * 8 + i] as u8;
                dest[d1 + 2 * i] = c;
                dest[d1 + 2 * i + 1] = c;
                dest[d2 + 2 * i] = c;
                dest[d2 + 2 * i + 1] = c;
            }
        }
        Ok(())
    }

    /// `blockMotion`.
    fn block_motion(
        &mut self,
        dest: &mut [u8],
        prev: &[u8],
        off: usize,
        ctx: &Ctx,
    ) -> Result<(), BinkError> {
        let x_off = self.get_bundle_value(SRC_X_OFF)? as isize;
        let y_off = self.get_bundle_value(SRC_Y_OFF)? as isize;
        let from = off as isize + y_off * ctx.pitch as isize + x_off;
        if from < 0 || from as usize > prev.len() {
            return Err(corrupt("copy out of bounds"));
        }
        let from = from as usize;
        if from + 7 * ctx.pitch + 8 > prev.len() {
            return Err(corrupt("copy out of bounds"));
        }
        block_copy(dest, prev, off, from, ctx.pitch, 8);
        Ok(())
    }

    /// `blockRun`.
    fn block_run(
        &mut self,
        bits: &mut BitReader<'_>,
        dest: &mut [u8],
        off: usize,
        ctx: &Ctx,
    ) -> Result<(), BinkError> {
        let scan = &PATTERNS[bits.get_bits(4) as usize];
        let mut s = 0;
        let mut i = 0;
        loop {
            let run = self.get_bundle_value(SRC_RUN)? as usize + 1;
            i += run;
            if i > 64 {
                return Err(corrupt("run went out of bounds"));
            }
            if bits.get_bit() != 0 {
                let v = self.get_bundle_value(SRC_COLORS)? as u8;
                for _ in 0..run {
                    dest[off + ctx.coord_map[usize::from(scan[s])]] = v;
                    s += 1;
                }
            } else {
                for _ in 0..run {
                    let v = self.get_bundle_value(SRC_COLORS)? as u8;
                    dest[off + ctx.coord_map[usize::from(scan[s])]] = v;
                    s += 1;
                }
            }
            if i >= 63 {
                break;
            }
        }
        if i == 63 {
            let v = self.get_bundle_value(SRC_COLORS)? as u8;
            dest[off + ctx.coord_map[usize::from(scan[s])]] = v;
        }
        Ok(())
    }

    /// `blockResidue`.
    fn block_residue(
        &mut self,
        bits: &mut BitReader<'_>,
        dest: &mut [u8],
        prev: &[u8],
        off: usize,
        ctx: &Ctx,
    ) -> Result<(), BinkError> {
        self.block_motion(dest, prev, off, ctx)?;
        let v = bits.get_bits(7) as i32;
        let mut block = [0i16; 64];
        read_residue(bits, &mut block, v)?;
        for i in 0..8 {
            let d = off + i * ctx.pitch;
            for j in 0..8 {
                dest[d + j] = (i32::from(dest[d + j]) + i32::from(block[i * 8 + j])) as u8;
            }
        }
        Ok(())
    }

    /// `blockIntra`.
    fn block_intra(
        &mut self,
        bits: &mut BitReader<'_>,
        dest: &mut [u8],
        off: usize,
        ctx: &Ctx,
    ) -> Result<(), BinkError> {
        let mut block = [0i32; 64];
        block[0] = self.get_bundle_value(SRC_INTRA_DC)?;
        read_dct_coeffs(bits, &mut block, true)?;
        idct_put(dest, off, ctx.pitch, &block);
        Ok(())
    }

    /// `blockFill`.
    fn block_fill(&mut self, dest: &mut [u8], off: usize, ctx: &Ctx) -> Result<(), BinkError> {
        let v = self.get_bundle_value(SRC_COLORS)? as u8;
        for i in 0..8 {
            let d = off + i * ctx.pitch;
            dest[d..d + 8].fill(v);
        }
        Ok(())
    }

    /// `blockInter`.
    fn block_inter(
        &mut self,
        bits: &mut BitReader<'_>,
        dest: &mut [u8],
        prev: &[u8],
        off: usize,
        ctx: &Ctx,
    ) -> Result<(), BinkError> {
        self.block_motion(dest, prev, off, ctx)?;
        let mut block = [0i32; 64];
        block[0] = self.get_bundle_value(SRC_INTER_DC)?;
        read_dct_coeffs(bits, &mut block, false)?;
        // IDCTAdd
        idct(&mut block);
        for i in 0..8 {
            let d = off + i * ctx.pitch;
            for j in 0..8 {
                dest[d + j] = (i32::from(dest[d + j]).wrapping_add(block[i * 8 + j])) as u8;
            }
        }
        Ok(())
    }

    /// `blockPattern`.
    fn block_pattern(&mut self, dest: &mut [u8], off: usize, ctx: &Ctx) -> Result<(), BinkError> {
        let col = [
            self.get_bundle_value(SRC_COLORS)? as u8,
            self.get_bundle_value(SRC_COLORS)? as u8,
        ];
        for i in 0..8 {
            let mut v = self.get_bundle_value(SRC_PATTERN)? as u8;
            let d = off + i * ctx.pitch;
            for j in 0..8 {
                dest[d + j] = col[usize::from(v & 1)];
                v >>= 1;
            }
        }
        Ok(())
    }

    /// `blockRaw`.
    fn block_raw(&mut self, dest: &mut [u8], off: usize, ctx: &Ctx) -> Result<(), BinkError> {
        let b = &mut self.bundles[SRC_COLORS];
        let data = b
            .data
            .get(b.cur_ptr..b.cur_ptr + 64)
            .ok_or(corrupt("bundle read past its end"))?;
        for i in 0..8 {
            let d = off + i * ctx.pitch;
            dest[d..d + 8].copy_from_slice(&data[i * 8..i * 8 + 8]);
        }
        b.cur_ptr += 64;
        Ok(())
    }
}

/// `readHuffman`: a bundle's tree and symbol order.
fn read_huffman(bits: &mut BitReader<'_>, huffman: &mut Huffman) -> Result<(), BinkError> {
    huffman.index = bits.get_bits(4) as usize;

    if huffman.index == 0 {
        // The first tree always gives raw nibbles.
        huffman.symbols = IDENTITY;
        return Ok(());
    }

    if bits.get_bit() != 0 {
        // Symbol selection
        let mut has_symbol = [false; 16];
        let mut length = bits.get_bits(3) as usize;
        for i in 0..=length {
            huffman.symbols[i] = bits.get_bits(4) as u8;
            has_symbol[usize::from(huffman.symbols[i])] = true;
        }
        for (i, &has) in has_symbol.iter().enumerate() {
            if !has {
                length += 1;
                // A repeated symbol would overflow ScummVM's array.
                *huffman
                    .symbols
                    .get_mut(length)
                    .ok_or(corrupt("repeated Huffman symbol"))? = i as u8;
            }
        }
        return Ok(());
    }

    // Symbol shuffling
    let mut a = IDENTITY;
    let mut b = [0u8; 16];
    let (mut inp, mut out) = (&mut a, &mut b);
    let depth = bits.get_bits(2);
    for i in 0..=depth {
        let size = 1usize << i;
        let mut j = 0;
        while j < 16 {
            merge_huffman_symbols(bits, &mut out[j..j + 2 * size], &inp[j..j + 2 * size], size);
            j += size << 1;
        }
        std::mem::swap(&mut inp, &mut out);
    }
    huffman.symbols = *inp;
    Ok(())
}

/// `mergeHuffmanSymbols`: merges `src[..size]` and `src[size..]` into `dst`.
fn merge_huffman_symbols(bits: &mut BitReader<'_>, dst: &mut [u8], src: &[u8], size: usize) {
    let (mut s1, mut s2, mut d) = (0, size, 0);
    let (mut size1, mut size2) = (size, size);
    loop {
        if bits.get_bit() == 0 {
            dst[d] = src[s1];
            s1 += 1;
            size1 -= 1;
        } else {
            dst[d] = src[s2];
            s2 += 1;
            size2 -= 1;
        }
        d += 1;
        if size1 == 0 || size2 == 0 {
            break;
        }
    }
    for _ in 0..size1 {
        dst[d] = src[s1];
        d += 1;
        s1 += 1;
    }
    for _ in 0..size2 {
        dst[d] = src[s2];
        d += 1;
        s2 += 1;
    }
}

/// The coefficient lists of `readDCTCoeffs` and `readResidue`.
struct Lists {
    coef: [i32; 128],
    mode: [i32; 128],
    start: usize,
    end: usize,
}

impl Lists {
    fn new(init: &[(i32, i32)]) -> Lists {
        let mut l = Lists {
            coef: [0; 128],
            mode: [0; 128],
            start: 64,
            end: 64,
        };
        for (i, &(c, m)) in init.iter().enumerate() {
            l.coef[64 + i] = c;
            l.mode[64 + i] = m;
        }
        l.end = 64 + init.len();
        l
    }

    fn push_end(&mut self, c: i32, m: i32) -> Result<(), BinkError> {
        if self.end >= 128 {
            return Err(corrupt("coefficient list overflow"));
        }
        self.coef[self.end] = c;
        self.mode[self.end] = m;
        self.end += 1;
        Ok(())
    }

    fn push_start(&mut self, c: i32, m: i32) -> Result<(), BinkError> {
        if self.start == 0 {
            return Err(corrupt("coefficient list overflow"));
        }
        self.start -= 1;
        self.coef[self.start] = c;
        self.mode[self.start] = m;
        Ok(())
    }
}

fn scan_at(ccoef: i32) -> Result<usize, BinkError> {
    usize::try_from(ccoef)
        .ok()
        .and_then(|c| SCAN.get(c))
        .map(|&s| usize::from(s))
        .ok_or(corrupt("coefficient out of bounds"))
}

/// `readDCTCoeffs`: an 8 × 8 block of DCT coefficients, dequantised.
fn read_dct_coeffs(
    bits: &mut BitReader<'_>,
    block: &mut [i32; 64],
    is_intra: bool,
) -> Result<(), BinkError> {
    let mut coef_count = 0;
    let mut coef_idx = [0usize; 64];
    let mut l = Lists::new(&[(4, 0), (24, 0), (44, 0), (1, 3), (2, 3), (3, 3)]);

    let mut nbits = bits.get_bits(4) as i32 - 1;
    let mut mask = if nbits >= 0 { 1i32 << nbits } else { 0 };
    let coef = |bits: &mut BitReader<'_>, nbits: i32, mask: i32| -> i32 {
        if nbits == 0 {
            1 - ((bits.get_bit() as i32) << 1)
        } else {
            let t = bits.get_bits(nbits as u32) as i32 | mask;
            if bits.get_bit() != 0 {
                -t
            } else {
                t
            }
        }
    };
    let mut put = |block: &mut [i32; 64], ccoef: i32, t: i32| -> Result<(), BinkError> {
        block[scan_at(ccoef)?] = t;
        if coef_count >= 64 {
            return Err(corrupt("too many coefficients"));
        }
        coef_idx[coef_count] = ccoef as usize;
        coef_count += 1;
        Ok(())
    };

    while nbits >= 0 {
        let mut pos = l.start;
        while pos < l.end {
            if (l.mode[pos] | l.coef[pos]) == 0 || bits.get_bit() == 0 {
                pos += 1;
                continue;
            }
            let mut ccoef = l.coef[pos];
            let mode = l.mode[pos];
            match mode {
                0 | 2 => {
                    if mode == 0 {
                        l.coef[pos] = ccoef + 4;
                        l.mode[pos] = 1;
                    } else {
                        l.coef[pos] = 0;
                        l.mode[pos] = 0;
                        pos += 1;
                    }
                    for _ in 0..4 {
                        if bits.get_bit() != 0 {
                            l.push_start(ccoef, 3)?;
                        } else {
                            let t = coef(bits, nbits, mask);
                            put(block, ccoef, t)?;
                        }
                        ccoef += 1;
                    }
                }
                1 => {
                    l.mode[pos] = 2;
                    for _ in 0..3 {
                        ccoef += 4;
                        l.push_end(ccoef, 2)?;
                    }
                }
                3 => {
                    let t = coef(bits, nbits, mask);
                    put(block, ccoef, t)?;
                    l.coef[pos] = 0;
                    l.mode[pos] = 0;
                    pos += 1;
                }
                _ => {}
            }
        }
        mask >>= 1;
        nbits -= 1;
    }

    let quant_idx = bits.get_bits(4) as usize;
    let quant = if is_intra {
        &INTRA_QUANT[quant_idx]
    } else {
        &INTER_QUANT[quant_idx]
    };
    block[0] = block[0].wrapping_mul(quant[0]) >> 11;
    for &idx in &coef_idx[..coef_count] {
        let s = usize::from(SCAN[idx]);
        block[s] = block[s].wrapping_mul(quant[idx]) >> 11;
    }
    Ok(())
}

/// `readResidue`: an 8 × 8 block of residue after motion compensation.
fn read_residue(
    bits: &mut BitReader<'_>,
    block: &mut [i16; 64],
    mut masks_count: i32,
) -> Result<(), BinkError> {
    let mut nz_coeff = [0usize; 64];
    let mut nz_coeff_count = 0;
    let mut l = Lists::new(&[(4, 0), (24, 0), (44, 0), (0, 2)]);

    let mut mask = 1i32 << bits.get_bits(3);
    while mask != 0 {
        for &nz in &nz_coeff[..nz_coeff_count] {
            if bits.get_bit() == 0 {
                continue;
            }
            if block[nz] < 0 {
                block[nz] = block[nz].wrapping_sub(mask as i16);
            } else {
                block[nz] = block[nz].wrapping_add(mask as i16);
            }
            masks_count -= 1;
            if masks_count < 0 {
                return Ok(());
            }
        }

        let mut pos = l.start;
        while pos < l.end {
            if (l.coef[pos] | l.mode[pos]) == 0 || bits.get_bit() == 0 {
                pos += 1;
                continue;
            }
            let mut ccoef = l.coef[pos];
            let mode = l.mode[pos];
            match mode {
                0 | 2 => {
                    if mode == 0 {
                        l.coef[pos] = ccoef + 4;
                        l.mode[pos] = 1;
                    } else {
                        l.coef[pos] = 0;
                        l.mode[pos] = 0;
                        pos += 1;
                    }
                    for _ in 0..4 {
                        if bits.get_bit() != 0 {
                            l.push_start(ccoef, 3)?;
                        } else {
                            let s = scan_at(ccoef)?;
                            if nz_coeff_count >= 64 {
                                return Err(corrupt("too many coefficients"));
                            }
                            nz_coeff[nz_coeff_count] = s;
                            nz_coeff_count += 1;
                            block[s] = if bits.get_bit() != 0 { -mask } else { mask } as i16;
                            masks_count -= 1;
                            if masks_count < 0 {
                                return Ok(());
                            }
                        }
                        ccoef += 1;
                    }
                }
                1 => {
                    l.mode[pos] = 2;
                    for _ in 0..3 {
                        ccoef += 4;
                        l.push_end(ccoef, 2)?;
                    }
                }
                3 => {
                    let s = scan_at(ccoef)?;
                    if nz_coeff_count >= 64 {
                        return Err(corrupt("too many coefficients"));
                    }
                    nz_coeff[nz_coeff_count] = s;
                    nz_coeff_count += 1;
                    block[s] = if bits.get_bit() != 0 { -mask } else { mask } as i16;
                    l.coef[pos] = 0;
                    l.mode[pos] = 0;
                    pos += 1;
                    masks_count -= 1;
                    if masks_count < 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        mask >>= 1;
    }
    Ok(())
}

const A1: Wrapping<i32> = Wrapping(2896); // (1/sqrt(2))<<12
const A2: Wrapping<i32> = Wrapping(2217);
const A3: Wrapping<i32> = Wrapping(3784);
const A4: Wrapping<i32> = Wrapping(-5352);

/// `IDCT_TRANSFORM` on eight values, before the munge.
fn idct_transform(s: [i32; 8]) -> [Wrapping<i32>; 8] {
    let s = s.map(Wrapping);
    let a0 = s[0] + s[4];
    let a1 = s[0] - s[4];
    let a2 = s[2] + s[6];
    let a3 = (A1 * (s[2] - s[6])) >> 11;
    let a4 = s[5] + s[3];
    let a5 = s[5] - s[3];
    let a6 = s[1] + s[7];
    let a7 = s[1] - s[7];
    let b0 = a4 + a6;
    let b1 = (A3 * (a5 + a7)) >> 11;
    let b2 = ((A4 * a5) >> 11) - b0 + b1;
    let b3 = ((A1 * (a6 - a4)) >> 11) - b2;
    let b4 = ((A2 * a7) >> 11) + b3 - b1;
    [
        a0 + a2 + b0,
        a1 + a3 - a2 + b2,
        a1 - a3 + a2 + b3,
        a0 - a2 - b4,
        a0 - a2 + b4,
        a1 - a3 + a2 - b3,
        a1 + a3 - a2 - b2,
        a0 + a2 - b0,
    ]
}

/// The column pass (`IDCTCol`) of the eight columns into `temp`.
fn idct_cols(block: &[i32; 64]) -> [i32; 64] {
    let mut temp = [0i32; 64];
    for i in 0..8 {
        let src: [i32; 8] = std::array::from_fn(|k| block[i + 8 * k]);
        if src[1..].iter().all(|&v| v == 0) {
            for k in 0..8 {
                temp[i + 8 * k] = src[0];
            }
        } else {
            for (k, v) in idct_transform(src).into_iter().enumerate() {
                temp[i + 8 * k] = v.0;
            }
        }
    }
    temp
}

/// The row pass's munge (`MUNGE_ROW`).
fn munge_row(x: Wrapping<i32>) -> i32 {
    (x + Wrapping(0x7F)).0 >> 8
}

/// `IDCT`: the block transformed in place.
fn idct(block: &mut [i32; 64]) {
    let temp = idct_cols(block);
    for i in 0..8 {
        let row: [i32; 8] = std::array::from_fn(|k| temp[8 * i + k]);
        for (k, v) in idct_transform(row).into_iter().enumerate() {
            block[8 * i + k] = munge_row(v);
        }
    }
}

/// `IDCTPut`: the block transformed into the plane, truncated to 8 bits.
fn idct_put(dest: &mut [u8], off: usize, pitch: usize, block: &[i32; 64]) {
    let temp = idct_cols(block);
    for i in 0..8 {
        let row: [i32; 8] = std::array::from_fn(|k| temp[8 * i + k]);
        let d = off + i * pitch;
        for (k, v) in idct_transform(row).into_iter().enumerate() {
            dest[d + k] = munge_row(v) as u8;
        }
    }
}

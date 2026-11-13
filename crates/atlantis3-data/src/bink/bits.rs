// Ported from ScummVM's `common/bitstream.h` (`BitStream32LELSB`) and
// `common/compression/huffman.h` (ScummVM - Graphic Adventure Engine,
// Copyright (C) the ScummVM team, GNU General Public License, version 3
// or later).

//! The bit reader and the Huffman trees of the Bink video decoder.
//!
//! ScummVM's `BitStream32LELSB` reads the packet as little-endian 32-bit
//! words, each from its bit 0 up. Its size is the packet's length rounded
//! down to whole words; a word past that size reads as zeros (peeking past
//! the end is defined there and returns 0 bits), so a value straddling the
//! end gets zeros for its missing bits.

use super::tables::{HUFFMAN_CODES, HUFFMAN_LENGTHS};

/// A packet read bit by bit.
pub(super) struct BitReader<'a> {
    data: &'a [u8],
    /// The size in bits: whole 32-bit words only.
    size: u32,
    /// The position in bits.
    pos: u32,
}

impl<'a> BitReader<'a> {
    pub(super) fn new(data: &'a [u8]) -> BitReader<'a> {
        let words = (data.len() & !3) as u64;
        BitReader {
            data,
            size: u32::try_from(words * 8).unwrap_or(!31),
            pos: 0,
        }
    }

    /// The 32-bit word `w`, or 0 when it is not wholly within the size.
    fn word(&self, w: usize) -> u64 {
        if (w as u64 + 1) * 32 > u64::from(self.size) {
            return 0;
        }
        let b = &self.data[w * 4..w * 4 + 4];
        u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// The next `n` bits (at most 32) without moving.
    pub(super) fn peek_bits(&self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 {
            return 0;
        }
        let w = (self.pos / 32) as usize;
        let both = self.word(w) | (self.word(w + 1) << 32);
        let v = both >> (self.pos % 32);
        (v & ((1u64 << n) - 1)) as u32
    }

    /// The next `n` bits (at most 32), first bit in bit 0.
    pub(super) fn get_bits(&mut self, n: u32) -> u32 {
        let v = self.peek_bits(n);
        self.pos = self.pos.saturating_add(n);
        v
    }

    pub(super) fn get_bit(&mut self) -> u32 {
        self.get_bits(1)
    }

    pub(super) fn skip(&mut self, n: u32) {
        self.pos = self.pos.saturating_add(n);
    }

    pub(super) fn pos(&self) -> u32 {
        self.pos
    }

    pub(super) fn size(&self) -> u32 {
        self.size
    }
}

/// One entry of a tree's 8-bit prefix table: the symbol and the code's
/// length, or the length `0xFF` where no code starts.
#[derive(Clone, Copy)]
struct Prefix {
    symbol: u8,
    length: u8,
}

/// The 16 Huffman trees of Bink video (`Common::Huffman` built from
/// `binkHuffmanCodes` and `binkHuffmanLengths`). Their codes are 7 bits at
/// most, so ScummVM's 8-bit prefix table holds them all.
pub(super) struct Trees {
    prefix: Box<[[Prefix; 256]; 16]>,
}

impl Trees {
    pub(super) fn new() -> Trees {
        let mut prefix = Box::new(
            [[Prefix {
                symbol: 0,
                length: 0xFF,
            }; 256]; 16],
        );
        for (t, table) in prefix.iter_mut().enumerate() {
            for (symbol, (&code, &length)) in HUFFMAN_CODES[t]
                .iter()
                .zip(HUFFMAN_LENGTHS[t].iter())
                .enumerate()
            {
                // The code's first bit is its bit 0: every 8-bit index whose
                // low `length` bits are the code starts with it.
                for high in 0..1u32 << (8 - length) {
                    table[(code | (high << length)) as usize] = Prefix {
                        symbol: symbol as u8,
                        length,
                    };
                }
            }
        }
        Trees { prefix }
    }

    /// The next symbol of tree `tree` (`Huffman::getSymbol`), or `None` for
    /// "Unknown Huffman code".
    pub(super) fn symbol(&self, tree: usize, bits: &mut BitReader<'_>) -> Option<u8> {
        let p = self.prefix[tree][bits.peek_bits(8) as usize];
        if p.length == 0xFF {
            return None;
        }
        bits.skip(u32::from(p.length));
        Some(p.symbol)
    }
}

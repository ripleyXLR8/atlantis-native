//! The Vorbis bit reader: least significant bit first, at most 32 bits per
//! read. A read past the end of the packet returns −1 and still moves the
//! position (libogg 1.1.3's `oggpack_read`), so every later read fails too;
//! a look past the end returns −1 and moves nothing.

pub(crate) struct Bits<'a> {
    data: &'a [u8],
    /// Position in bits from the start of the packet.
    pos: u64,
}

impl<'a> Bits<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Bits { data, pos: 0 }
    }

    fn fits(&self, bits: u32) -> bool {
        self.pos + u64::from(bits) <= self.data.len() as u64 * 8
    }

    fn peek(&self, bits: u32) -> u32 {
        if bits == 0 {
            return 0;
        }
        let byte = (self.pos / 8) as usize;
        let shift = (self.pos % 8) as u32;
        let mut v = 0u64;
        for k in 0..5 {
            if let Some(&b) = self.data.get(byte + k) {
                v |= u64::from(b) << (8 * k);
            }
        }
        ((v >> shift) & ((1u64 << bits) - 1)) as u32
    }

    /// `bits` (0..=32) bits as an unsigned value, or −1 past the end.
    pub(crate) fn read(&mut self, bits: u32) -> i64 {
        let r = if self.fits(bits) {
            i64::from(self.peek(bits))
        } else {
            -1
        };
        self.pos += u64::from(bits);
        r
    }

    /// The next `bits` bits without moving, or −1 if the packet is shorter.
    pub(crate) fn look(&self, bits: u32) -> i64 {
        if self.fits(bits) {
            i64::from(self.peek(bits))
        } else {
            -1
        }
    }

    pub(crate) fn adv(&mut self, bits: u32) {
        self.pos += u64::from(bits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lsb_first_and_fails_past_the_end() {
        let mut b = Bits::new(&[0b1010_1100, 0xff]);
        assert_eq!(b.read(2), 0);
        assert_eq!(b.read(3), 0b011);
        assert_eq!(b.look(5), 0b11101);
        assert_eq!(b.read(11), 0x7fd);
        assert_eq!(b.look(1), -1);
        assert_eq!(b.read(1), -1);
        assert_eq!(b.read(0), -1, "a read after an overrun stays past the end");
        let mut b = Bits::new(&[1, 2, 3, 4]);
        assert_eq!(b.read(32), 0x0403_0201);
    }
}

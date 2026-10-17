//! A little-endian byte cursor with the end-of-stream behaviour of
//! ScummVM's `Common::MemoryReadStream`, which the translated loaders were
//! written against: a read past the end returns what is left (zero filled
//! here) and raises `eos`; a seek past the end lands on the end and clears
//! `eos`.

pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    eos: bool,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Reader<'a> {
        Reader {
            data,
            pos: 0,
            eos: false,
        }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn eos(&self) -> bool {
        self.eos
    }

    pub(crate) fn seek(&mut self, pos: usize) {
        self.pos = pos.min(self.data.len());
        self.eos = false;
    }

    pub(crate) fn skip(&mut self, n: usize) {
        let pos = self.pos.saturating_add(n);
        self.seek(pos);
    }

    /// Reads and drops `n` bytes (a run of `readByte` calls: raises `eos`
    /// when it goes past the end, unlike [`Reader::skip`]).
    pub(crate) fn discard(&mut self, n: usize) {
        let avail = (self.data.len() - self.pos).min(n);
        if avail < n {
            self.eos = true;
        }
        self.pos += avail;
    }

    fn take<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        let avail = (self.data.len() - self.pos).min(N);
        out[..avail].copy_from_slice(&self.data[self.pos..self.pos + avail]);
        if avail < N {
            self.eos = true;
        }
        self.pos += avail;
        out
    }

    pub(crate) fn u8(&mut self) -> u8 {
        self.take::<1>()[0]
    }

    pub(crate) fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.take())
    }

    pub(crate) fn i16(&mut self) -> i16 {
        i16::from_le_bytes(self.take())
    }

    pub(crate) fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take())
    }

    pub(crate) fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.take())
    }

    pub(crate) fn f32(&mut self) -> f32 {
        f32::from_le_bytes(self.take())
    }

    /// A 13-byte NUL-padded name slot, cut at its first NUL (a slot whose
    /// first byte is NUL is an empty name). Bytes are read as Latin-1.
    pub(crate) fn name13(&mut self) -> String {
        let raw: [u8; 13] = self.take();
        latin1(&raw)
    }
}

/// The bytes up to the first NUL, as Latin-1 characters.
pub(crate) fn latin1(raw: &[u8]) -> String {
    raw.iter()
        .take_while(|&&b| b != 0)
        .map(|&b| char::from(b))
        .collect()
}

/// C `atoi`: leading white space, an optional sign, then decimal digits;
/// anything else ends the number (0 when there are no digits).
pub(crate) fn atoi(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let mut neg = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        neg = s[i] == b'-';
        i += 1;
    }
    let mut v: i32 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        v = v.wrapping_mul(10).wrapping_add(i32::from(s[i] - b'0'));
        i += 1;
    }
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

/// C `strtod` on decimal input (the only form the game's text files use):
/// leading white space, then the longest `[+-]digits[.digits][e[+-]digits]`
/// prefix, correctly rounded; 0 when there is none.
pub(crate) fn strtod(s: &[u8]) -> f64 {
    let mut i = 0;
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let start = i;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        i += 1;
    }
    let digits_from = i;
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
    }
    let mut mantissa_digits = i - digits_from;
    if i < s.len() && s[i] == b'.' {
        let frac_from = i + 1;
        let mut j = frac_from;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        mantissa_digits += j - frac_from;
        if mantissa_digits > 0 {
            i = j;
        }
    }
    if mantissa_digits == 0 {
        return 0.0;
    }
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        let mut j = i + 1;
        if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
            j += 1;
        }
        let exp_from = j;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_from {
            i = j;
        }
    }
    let text = std::str::from_utf8(&s[start..i]).unwrap_or("0");
    // Rust's parser takes the same forms ("1.", ".5", "-.5e2").
    text.parse::<f64>().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_number_parsing() {
        assert_eq!(atoi(b"  -12x"), -12);
        assert_eq!(atoi(b"12-"), 12);
        assert_eq!(atoi(b"x"), 0);
        assert_eq!(strtod(b" -4500.25 1"), -4500.25);
        assert_eq!(strtod(b"1.e2"), 100.0);
        assert_eq!(strtod(b".5"), 0.5);
        assert_eq!(strtod(b"-.5e"), -0.5);
        assert_eq!(strtod(b"abc"), 0.0);
        assert_eq!(strtod(b"7e+1z"), 70.0);
    }

    #[test]
    fn reads_past_the_end() {
        let mut r = Reader::new(&[1, 2, 3]);
        assert_eq!(r.u16(), 0x0201);
        assert!(!r.eos());
        assert_eq!(r.u16(), 3);
        assert!(r.eos());
        r.seek(10);
        assert!(!r.eos());
        assert_eq!(r.pos(), 3);
    }
}

//! Codebooks: the set-up header's books, their value vectors and the
//! entry decoder, as libvorbis 1.2.0's `codebook.c` / `sharedbook.c` do
//! them (the lookup table plus bisection decoder, so that a codeword cut
//! by the end of a packet is handled the same way).

use super::bits::Bits;
use super::Error;

/// libvorbis's `_ilog`: the number of significant bits of `v`.
pub(crate) fn ilog(v: u32) -> u32 {
    32 - v.leading_zeros()
}

/// A codebook ready for decoding (`codebook` after `vorbis_book_init_decode`).
pub(crate) struct Book {
    pub(crate) dim: usize,
    pub(crate) used_entries: usize,
    /// `used_entries × dim` values, in sorted-codeword order (maptype 1/2).
    pub(crate) valuelist: Vec<f32>,
    codelist: Vec<u32>,
    dec_index: Vec<i32>,
    dec_codelengths: Vec<u8>,
    dec_firsttable: Vec<u32>,
    dec_firsttablen: u32,
    dec_maxlength: u32,
}

/// libvorbis's non-IEEE packed float (`_float32_unpack`), rounded to `f32`
/// on return as the C function's `float` result is.
fn float32_unpack(val: u32) -> f32 {
    let mut mant = f64::from(val & 0x1f_ffff);
    let exp = ((val & 0x7fe0_0000) >> 21) as i32;
    if val & 0x8000_0000 != 0 {
        mant = -mant;
    }
    (mant * 2f64.powi(exp - 20 - 768)) as f32
}

/// The number of values per dimension of a lattice book (maptype 1): the
/// greatest `v` with `v^dim <= entries`.
fn maptype1_quantvals(entries: usize, dim: usize) -> usize {
    // libvorbis starts from floor(pow(entries, 1/dim)) and corrects with
    // integers, so the start value does not change the result.
    let mut vals = (entries as f64).powf(1.0 / dim as f64).floor() as i64;
    loop {
        let (mut acc, mut acc1) = (1i64, 1i64);
        for _ in 0..dim {
            acc = acc.saturating_mul(vals);
            acc1 = acc1.saturating_mul(vals + 1);
        }
        if acc <= entries as i64 && acc1 > entries as i64 {
            return vals.max(0) as usize;
        }
        if acc > entries as i64 {
            vals -= 1;
        } else {
            vals += 1;
        }
    }
}

/// The codewords of a list of lengths (`_make_words`), only for used
/// entries, bit-reversed as the reader sees them. `None` for an
/// over-populated tree.
fn make_words(lengths: &[u8], used: usize) -> Option<Vec<u32>> {
    let mut marker = [0u32; 33];
    let mut r = Vec::with_capacity(used);
    for &len in lengths {
        let length = usize::from(len);
        if length == 0 {
            continue;
        }
        let mut entry = marker[length];
        if length < 32 && (entry >> length) != 0 {
            return None;
        }
        r.push(entry);
        for j in (1..=length).rev() {
            if marker[j] & 1 != 0 {
                if j == 1 {
                    marker[1] += 1;
                } else {
                    marker[j] = marker[j - 1] << 1;
                }
                break;
            }
            marker[j] += 1;
        }
        for j in length + 1..33 {
            if (marker[j] >> 1) == entry {
                entry = marker[j];
                marker[j] = marker[j - 1] << 1;
            } else {
                break;
            }
        }
    }
    // Bit-reverse each word within its length.
    let mut count = 0;
    for &len in lengths {
        if len == 0 {
            continue;
        }
        let mut temp = 0u32;
        for j in 0..u32::from(len) {
            temp = (temp << 1) | ((r[count] >> j) & 1);
        }
        r[count] = temp;
        count += 1;
    }
    Some(r)
}

fn bitreverse(x: u32) -> u32 {
    x.reverse_bits()
}

impl Book {
    /// Reads one codebook from the set-up header (`vorbis_staticbook_unpack`)
    /// and readies it for decoding (`vorbis_book_init_decode`).
    pub(crate) fn unpack(b: &mut Bits) -> Result<Book, Error> {
        let bad = || Error("Vorbis: bad codebook".into());
        if b.read(24) != 0x56_4342 {
            return Err(bad());
        }
        let dim = b.read(16);
        let entries = b.read(24);
        if dim < 0 || entries < 0 {
            return Err(bad());
        }
        let (dim, entries) = (dim as usize, entries as usize);
        let mut lengths = vec![0u8; entries];
        match b.read(1) {
            0 => {
                let sparse = b.read(1) == 1;
                for l in lengths.iter_mut() {
                    if !sparse || b.read(1) == 1 {
                        let num = b.read(5);
                        if num == -1 {
                            return Err(bad());
                        }
                        *l = num as u8 + 1;
                    } else {
                        *l = 0;
                    }
                }
            }
            1 => {
                let mut length = b.read(5) + 1;
                let mut i = 0;
                while i < entries {
                    let num = b.read(ilog((entries - i) as u32));
                    if num == -1 {
                        return Err(bad());
                    }
                    let mut j = 0;
                    while j < num && i < entries {
                        lengths[i] = length as u8;
                        j += 1;
                        i += 1;
                    }
                    length += 1;
                }
            }
            _ => return Err(bad()),
        }
        let maptype = b.read(4);
        let mut quant = None;
        match maptype {
            0 => {}
            1 | 2 => {
                let q_min = b.read(32) as u32;
                let q_delta = b.read(32) as u32;
                let q_quant = (b.read(4) + 1) as u32;
                let q_sequencep = b.read(1) == 1;
                let quantvals = if maptype == 1 {
                    maptype1_quantvals(entries, dim)
                } else {
                    entries * dim
                };
                let list: Vec<i64> = (0..quantvals).map(|_| b.read(q_quant)).collect();
                if list.last() == Some(&-1) {
                    return Err(bad());
                }
                quant = Some((maptype, q_min, q_delta, q_sequencep, quantvals, list));
            }
            _ => return Err(bad()),
        }

        // vorbis_book_init_decode
        let used = lengths.iter().filter(|&&l| l > 0).count();
        let mut book = Book {
            dim,
            used_entries: used,
            valuelist: Vec::new(),
            codelist: Vec::new(),
            dec_index: Vec::new(),
            dec_codelengths: Vec::new(),
            dec_firsttable: Vec::new(),
            dec_firsttablen: 0,
            dec_maxlength: 0,
        };
        if used == 0 {
            return Ok(book);
        }
        let codes: Vec<u32> = make_words(&lengths, used)
            .ok_or_else(bad)?
            .into_iter()
            .map(bitreverse)
            .collect();
        let mut order: Vec<usize> = (0..used).collect();
        order.sort_by_key(|&i| codes[i]);
        // sortindex[position in used order] = sorted rank
        let mut sortindex = vec![0usize; used];
        for (rank, &pos) in order.iter().enumerate() {
            sortindex[pos] = rank;
        }
        let mut codelist = vec![0u32; used];
        for i in 0..used {
            codelist[sortindex[i]] = codes[i];
        }
        if let Some((maptype, q_min, q_delta, seq, quantvals, list)) = quant {
            book.valuelist = unquantize(
                maptype, q_min, q_delta, seq, quantvals, &list, &lengths, dim, &sortindex, used,
            );
        }
        let mut dec_index = vec![0i32; used];
        let mut dec_codelengths = vec![0u8; used];
        let mut n = 0;
        for (i, &l) in lengths.iter().enumerate() {
            if l > 0 {
                dec_index[sortindex[n]] = i as i32;
                dec_codelengths[sortindex[n]] = l;
                n += 1;
            }
        }
        let tablen = (ilog(used as u32) as i32 - 4).clamp(5, 8) as u32;
        let tabn = 1usize << tablen;
        let mut first = vec![0u32; tabn];
        let mut maxlength = 0u32;
        for i in 0..used {
            let l = u32::from(dec_codelengths[i]);
            maxlength = maxlength.max(l);
            if l <= tablen {
                let orig = bitreverse(codelist[i]);
                for j in 0..(1u32 << (tablen - l)) {
                    first[(orig | (j << l)) as usize] = i as u32 + 1;
                }
            }
        }
        let mask = 0xffff_fffeu32 << (31 - tablen);
        let (mut lo, mut hi) = (0usize, 0usize);
        for i in 0..tabn {
            let word = (i as u32) << (32 - tablen);
            let slot = bitreverse(word) as usize;
            if first[slot] == 0 {
                while lo + 1 < used && codelist[lo + 1] <= word {
                    lo += 1;
                }
                while hi < used && word >= (codelist[hi] & mask) {
                    hi += 1;
                }
                let loval = (lo as u32).min(0x7fff);
                let hival = ((used - hi) as u32).min(0x7fff);
                first[slot] = 0x8000_0000 | (loval << 15) | hival;
            }
        }
        book.codelist = codelist;
        book.dec_index = dec_index;
        book.dec_codelengths = dec_codelengths;
        book.dec_firsttable = first;
        book.dec_firsttablen = tablen;
        book.dec_maxlength = maxlength;
        Ok(book)
    }

    /// `decode_packed_entry_number`: the sorted entry, or −1.
    fn decode_packed(&self, b: &mut Bits) -> i64 {
        let mut read = self.dec_maxlength;
        let (mut lo, mut hi);
        let lok = b.look(self.dec_firsttablen);
        if lok >= 0 {
            let entry = self.dec_firsttable[lok as usize];
            if entry & 0x8000_0000 != 0 {
                lo = ((entry >> 15) & 0x7fff) as i64;
                hi = self.used_entries as i64 - i64::from(entry & 0x7fff);
            } else {
                b.adv(u32::from(self.dec_codelengths[entry as usize - 1]));
                return i64::from(entry) - 1;
            }
        } else {
            lo = 0;
            hi = self.used_entries as i64;
        }
        let mut lok = b.look(read);
        while lok < 0 && read > 1 {
            read -= 1;
            lok = b.look(read);
        }
        if lok < 0 {
            return -1;
        }
        let testword = bitreverse(lok as u32);
        while hi - lo > 1 {
            let p = (hi - lo) >> 1;
            let test = i64::from(self.codelist[(lo + p) as usize] > testword);
            lo += p & (test - 1);
            hi -= p & (-test);
        }
        if u32::from(self.dec_codelengths[lo as usize]) <= read {
            b.adv(u32::from(self.dec_codelengths[lo as usize]));
            return lo;
        }
        b.adv(read);
        -1
    }

    /// `vorbis_book_decode`: the original entry number, or −1.
    pub(crate) fn decode(&self, b: &mut Bits) -> i64 {
        if self.used_entries > 0 {
            let p = self.decode_packed(b);
            if p >= 0 {
                return i64::from(self.dec_index[p as usize]);
            }
        }
        -1
    }

    fn vector(&self, entry: i64) -> &[f32] {
        let at = entry as usize * self.dim;
        self.valuelist.get(at..at + self.dim).unwrap_or(&[])
    }

    /// `vorbis_book_decodevs_add` (residue 0): `n / dim` entries, value `i`
    /// of entry `j` added at `i × step + j`. Nothing is added unless every
    /// entry decodes.
    pub(crate) fn decodevs_add(&self, a: &mut [f32], b: &mut Bits, n: usize) -> bool {
        if self.used_entries == 0 {
            return true;
        }
        if self.dim == 0 {
            return false;
        }
        let step = n / self.dim;
        let mut entries = Vec::with_capacity(step);
        for _ in 0..step {
            let e = self.decode_packed(b);
            if e == -1 {
                return false;
            }
            entries.push(e);
        }
        for i in 0..self.dim {
            let o = i * step;
            for (j, &e) in entries.iter().enumerate() {
                if let (Some(x), Some(&v)) = (a.get_mut(o + j), self.vector(e).get(i)) {
                    *x += v;
                }
            }
        }
        true
    }

    /// `vorbis_book_decodev_add` (residue 1): whole vectors added in turn.
    pub(crate) fn decodev_add(&self, a: &mut [f32], b: &mut Bits, n: usize) -> bool {
        if self.dim == 0 {
            return false;
        }
        if self.used_entries == 0 {
            return true;
        }
        let mut i = 0;
        while i < n {
            let e = self.decode_packed(b);
            if e == -1 {
                return false;
            }
            let t = self.vector(e);
            for k in 0..self.dim {
                if let (Some(x), Some(&v)) = (a.get_mut(i), t.get(k)) {
                    *x += v;
                }
                i += 1;
            }
        }
        true
    }

    /// `vorbis_book_decodev_set` (floor 0).
    pub(crate) fn decodev_set(&self, a: &mut [f32], b: &mut Bits, n: usize) -> bool {
        if self.dim == 0 {
            return false;
        }
        let mut i = 0;
        if self.used_entries == 0 {
            while i < n {
                for _ in 0..self.dim {
                    if let Some(x) = a.get_mut(i) {
                        *x = 0.0;
                    }
                    i += 1;
                }
            }
            return true;
        }
        while i < n {
            let e = self.decode_packed(b);
            if e == -1 {
                return false;
            }
            let t = self.vector(e);
            for k in 0..self.dim {
                if let (Some(x), Some(&v)) = (a.get_mut(i), t.get(k)) {
                    *x = v;
                }
                i += 1;
            }
        }
        true
    }

    /// `vorbis_book_decodevv_add` (residue 2): the vector interleaved over
    /// `ch` channels from `offset`.
    pub(crate) fn decodevv_add(
        &self,
        a: &mut [Vec<f32>],
        offset: usize,
        ch: usize,
        b: &mut Bits,
        n: usize,
    ) -> bool {
        if self.dim == 0 || ch == 0 {
            return false;
        }
        if self.used_entries == 0 {
            return true;
        }
        let mut chptr = 0;
        let mut i = offset / ch;
        while i < (offset + n) / ch {
            let e = self.decode_packed(b);
            if e == -1 {
                return false;
            }
            let t = self.vector(e);
            for k in 0..self.dim {
                if let (Some(x), Some(&v)) = (a[chptr].get_mut(i), t.get(k)) {
                    *x += v;
                }
                chptr += 1;
                if chptr == ch {
                    chptr = 0;
                    i += 1;
                }
            }
        }
        true
    }
}

/// `_book_unquantize` with the sparse map of the decoder: one vector per
/// used entry, at its sorted rank. Each value is `|q| × delta + min + last`
/// evaluated in double and rounded to float once, as the C expression is.
#[allow(clippy::too_many_arguments)]
fn unquantize(
    maptype: i64,
    q_min: u32,
    q_delta: u32,
    sequencep: bool,
    quantvals: usize,
    list: &[i64],
    lengths: &[u8],
    dim: usize,
    sortindex: &[usize],
    used: usize,
) -> Vec<f32> {
    let mindel = f64::from(float32_unpack(q_min));
    let delta = f64::from(float32_unpack(q_delta));
    let mut r = vec![0f32; used * dim];
    let mut count = 0;
    for (j, &l) in lengths.iter().enumerate() {
        if l == 0 {
            continue;
        }
        let mut last = 0f32;
        let mut indexdiv = 1usize;
        for k in 0..dim {
            let q = if maptype == 1 {
                list[(j / indexdiv) % quantvals]
            } else {
                list[j * dim + k]
            };
            // `float val = quantlist[...]` (an integer, exact in float)
            let val = (q as f32).abs();
            let v = (f64::from(val) * delta + mindel + f64::from(last)) as f32;
            if sequencep {
                last = v;
            }
            r[sortindex[count] * dim + k] = v;
            if maptype == 1 {
                indexdiv *= quantvals;
            }
        }
        count += 1;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codewords_follow_the_specification_example() {
        // Vorbis I specification § 3.2.1: lengths 2 4 4 4 4 2 3 3 give
        // 00 0100 0101 0110 0111 10 110 111 (written MSB first).
        let w = make_words(&[2, 4, 4, 4, 4, 2, 3, 3], 8).unwrap();
        let msb: Vec<u32> = w
            .iter()
            .zip([2u32, 4, 4, 4, 4, 2, 3, 3])
            .map(|(&c, l)| c.reverse_bits() >> (32 - l))
            .collect();
        assert_eq!(
            msb,
            [0b00, 0b0100, 0b0101, 0b0110, 0b0111, 0b10, 0b110, 0b111]
        );
        assert!(make_words(&[1, 1, 1], 3).is_none());
        assert_eq!(maptype1_quantvals(81, 4), 3);
        assert_eq!(maptype1_quantvals(80, 4), 2);
        assert_eq!(
            float32_unpack(0x6100_0000 | (1 << 20)),
            1.0 * 2f32.powi(8 - 768 + 768)
        );
    }
}

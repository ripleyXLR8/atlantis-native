//! Residue types 0, 1 and 2 (Vorbis I specification § 8; libvorbis
//! 1.2.0's `res0.c`). A truncated packet stops the decoding of the
//! residue without an error, keeping what was added so far.

use super::bits::Bits;
use super::book::{ilog, Book};
use super::Error;

pub(crate) struct Residue {
    kind: u16,
    begin: usize,
    end: usize,
    grouping: usize,
    partitions: usize,
    groupbook: usize,
    secondstages: Vec<u32>,
    /// `partbooks[partition][stage]`.
    partbooks: Vec<Vec<Option<usize>>>,
    stages: usize,
    /// `decodemap[partition word][k]`.
    decodemap: Vec<Vec<usize>>,
}

impl Residue {
    /// `res0_unpack` then `res0_look`.
    pub(crate) fn unpack(kind: u16, b: &mut Bits, books: &[Book]) -> Result<Residue, Error> {
        let bad = || Error("Vorbis: bad residue".into());
        let begin = b.read(24);
        let end = b.read(24);
        let grouping = b.read(24) + 1;
        let partitions = (b.read(6) + 1) as usize;
        let groupbook = b.read(8);
        if begin < 0 || end < 0 || grouping <= 0 || groupbook < 0 {
            return Err(bad());
        }
        let mut secondstages = Vec::with_capacity(partitions);
        let mut acc = 0;
        for _ in 0..partitions {
            let mut cascade = b.read(3) as u32;
            if b.read(1) == 1 {
                cascade |= (b.read(5) as u32) << 3;
            }
            acc += cascade.count_ones() as usize;
            secondstages.push(cascade);
        }
        let mut booklist = Vec::with_capacity(acc);
        for _ in 0..acc {
            booklist.push(b.read(8));
        }
        if groupbook as usize >= books.len()
            || booklist.iter().any(|&k| k < 0 || k as usize >= books.len())
        {
            return Err(bad());
        }
        // res0_look
        let dim = books[groupbook as usize].dim;
        let mut maxstage = 0;
        let mut acc = 0;
        let mut partbooks = Vec::with_capacity(partitions);
        for &ss in &secondstages {
            let stages = ilog(ss) as usize;
            maxstage = maxstage.max(stages);
            let mut v = vec![None; stages];
            for (k, slot) in v.iter_mut().enumerate() {
                if ss & (1 << k) != 0 {
                    *slot = Some(booklist[acc] as usize);
                    acc += 1;
                }
            }
            partbooks.push(v);
        }
        // rint(pow(parts, dim)), exact for these integers
        let partvals = (partitions as f64).powi(dim as i32).round() as usize;
        if partvals > 1 << 24 {
            return Err(bad());
        }
        let mut decodemap = Vec::with_capacity(partvals);
        for j in 0..partvals {
            let mut val = j;
            let mut mult = partvals / partitions;
            let mut m = Vec::with_capacity(dim);
            for _ in 0..dim {
                let deco = val.checked_div(mult).unwrap_or(0);
                val -= deco * mult;
                mult /= partitions;
                m.push(deco);
            }
            decodemap.push(m);
        }
        Ok(Residue {
            kind,
            begin: begin as usize,
            end: end as usize,
            grouping: grouping as usize,
            partitions,
            groupbook: groupbook as usize,
            secondstages,
            partbooks,
            stages: maxstage,
            decodemap,
        })
    }

    /// `res{0,1,2}_inverse` on the vectors of one submap. `nonzero` marks
    /// the channels whose floor is used (after coupling); `pcmend` is the
    /// block size.
    pub(crate) fn inverse(
        &self,
        b: &mut Bits,
        books: &[Book],
        vecs: &mut [Vec<f32>],
        nonzero: &[bool],
        pcmend: usize,
    ) {
        if self.kind == 2 {
            if nonzero.iter().any(|&z| z) {
                self.inverse2(b, books, vecs, pcmend);
            }
            return;
        }
        // Types 0 and 1 work on the used vectors only.
        let mut used: Vec<&mut Vec<f32>> = vecs
            .iter_mut()
            .zip(nonzero)
            .filter_map(|(v, &z)| z.then_some(v))
            .collect();
        if used.is_empty() {
            return;
        }
        let phrase = &books[self.groupbook];
        let ppw = phrase.dim;
        let max = pcmend >> 1;
        let end = self.end.min(max);
        if end <= self.begin {
            return;
        }
        let n = end - self.begin;
        let partvals = n / self.grouping;
        let partwords = partvals.div_ceil(ppw.max(1));
        let ch = used.len();
        let mut partword: Vec<Vec<usize>> = vec![vec![0; partwords]; ch];
        for s in 0..self.stages {
            let (mut i, mut l) = (0, 0);
            while i < partvals {
                if s == 0 {
                    for pw in partword.iter_mut() {
                        let temp = phrase.decode(b);
                        if temp == -1 {
                            return;
                        }
                        if temp as usize >= self.decodemap.len() {
                            return;
                        }
                        pw[l] = temp as usize;
                    }
                }
                let mut k = 0;
                while k < ppw && i < partvals {
                    for (j, v) in used.iter_mut().enumerate() {
                        let offset = self.begin + i * self.grouping;
                        let part = self.decodemap[partword[j][l]][k];
                        if self
                            .secondstages
                            .get(part)
                            .is_some_and(|ss| ss & (1 << s) != 0)
                        {
                            if let Some(Some(book)) = self.partbooks[part].get(s) {
                                let book = &books[*book];
                                let a = v.get_mut(offset..).unwrap_or(&mut []);
                                let ok = if self.kind == 0 {
                                    book.decodevs_add(a, b, self.grouping)
                                } else {
                                    book.decodev_add(a, b, self.grouping)
                                };
                                if !ok {
                                    return;
                                }
                            }
                        }
                    }
                    k += 1;
                    i += 1;
                }
                l += 1;
            }
        }
    }

    fn inverse2(&self, b: &mut Bits, books: &[Book], vecs: &mut [Vec<f32>], pcmend: usize) {
        let ch = vecs.len();
        let phrase = &books[self.groupbook];
        let ppw = phrase.dim;
        let max = (pcmend * ch) >> 1;
        let end = self.end.min(max);
        if end <= self.begin {
            return;
        }
        let n = end - self.begin;
        let partvals = n / self.grouping;
        let partwords = partvals.div_ceil(ppw.max(1));
        let mut partword = vec![0usize; partwords];
        for s in 0..self.stages {
            let (mut i, mut l) = (0, 0);
            while i < partvals {
                if s == 0 {
                    let temp = phrase.decode(b);
                    if temp == -1 || temp as usize >= self.decodemap.len() {
                        return;
                    }
                    partword[l] = temp as usize;
                }
                let mut k = 0;
                while k < ppw && i < partvals {
                    let part = self.decodemap[partword[l]][k];
                    if self
                        .secondstages
                        .get(part)
                        .is_some_and(|ss| ss & (1 << s) != 0)
                    {
                        if let Some(Some(book)) = self.partbooks[part].get(s) {
                            if !books[*book].decodevv_add(
                                vecs,
                                i * self.grouping + self.begin,
                                ch,
                                b,
                                self.grouping,
                            ) {
                                return;
                            }
                        }
                    }
                    k += 1;
                    i += 1;
                }
                l += 1;
            }
        }
        let _ = self.partitions;
    }
}

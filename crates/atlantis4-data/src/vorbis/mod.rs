//! Vorbis decoding that gives the same samples as *Atlantis Evolution*.
//!
//! The game decodes every `.ogg` with libvorbisfile from libvorbis 1.2.0,
//! linked into its executable and compiled for the x87: every float
//! expression is evaluated at 53-bit precision and rounded to float where
//! C stores it, `M_PI` is the float `3.1415926536f`, and `ov_read` turns
//! each sample into 16 bits with `fistp` (round half to even), no dither.
//! This module is a port of that decode path: the Ogg packet loop of
//! `vorbisfile.c` as the game opens a file (seekable, from offset 0), the
//! header and codebook set-up, floor 0 and floor 1, residues 0, 1 and 2,
//! channel coupling, the inverse MDCT, windowing and overlap-add, the
//! granule-position trimming, and the 16-bit conversion. Each C float
//! expression is computed in `f64` and rounded to `f32` at each
//! assignment, which gives the x87's results bit for bit (a test checks
//! the samples of every sound of the game against the game's own decoder).
//!
//! The code follows the structure of libvorbis 1.2.0 and libogg 1.1.3:
//!
//! > Copyright (c) 2002-2007 Xiph.org Foundation
//! >
//! > Redistribution and use in source and binary forms, with or without
//! > modification, are permitted provided that the following conditions
//! > are met:
//! >
//! > - Redistributions of source code must retain the above copyright
//! >   notice, this list of conditions and the following disclaimer.
//! > - Redistributions in binary form must reproduce the above copyright
//! >   notice, this list of conditions and the following disclaimer in the
//! >   documentation and/or other materials provided with the distribution.
//! > - Neither the name of the Xiph.org Foundation nor the names of its
//! >   contributors may be used to endorse or promote products derived from
//! >   this software without specific prior written permission.
//! >
//! > THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
//! > "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
//! > LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
//! > A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE FOUNDATION
//! > OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//! > SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
//! > LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
//! > DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
//! > THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
//! > (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
//! > OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

// The tables and constants are libvorbis's float literals, kept as written.
#![allow(
    clippy::excessive_precision,
    clippy::approx_constant,
    clippy::unreadable_literal
)]

mod bits;
mod book;
mod floor0;
mod floor1;
mod mdct;
mod residue;
mod window;

use std::sync::Arc;

use bits::Bits;
use book::{ilog, Book};
use floor0::Floor0;
use floor1::Floor1;
use mdct::Mdct;
use residue::Residue;

/// Why a Vorbis stream cannot be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// `ilog2` of `block.c` / `mapping0.c`: the bits needed for values below `v`.
fn ilog2(v: u32) -> u32 {
    ilog(v.saturating_sub(1))
}

enum Floor {
    Zero(Floor0),
    One(Box<Floor1>),
}

enum FloorMemo {
    Zero(Vec<f32>),
    One(Vec<i32>),
}

struct Mapping {
    coupling: Vec<(usize, usize)>,
    chmux: Vec<usize>,
    /// `(floor, residue)` of each submap.
    submaps: Vec<(usize, usize)>,
}

struct Mode {
    blockflag: bool,
    mapping: usize,
}

/// Everything the three headers set up (`vorbis_info` + `codec_setup_info`
/// + the decoder's look-ups).
struct Setup {
    channels: usize,
    rate: u32,
    blocksizes: [usize; 2],
    books: Vec<Book>,
    floors: Vec<Floor>,
    residues: Vec<Residue>,
    mappings: Vec<Mapping>,
    modes: Vec<Mode>,
    modebits: u32,
    mdct: [Mdct; 2],
}

fn check_header(b: &mut Bits, kind: i64) -> bool {
    if b.read(8) != kind {
        return false;
    }
    b"vorbis".iter().all(|&c| b.read(8) == i64::from(c))
}

impl Setup {
    fn new(ident: &[u8], comment: &[u8], setup: &[u8]) -> Result<Setup, Error> {
        let bad = |w: &str| Error(format!("Vorbis: {w}"));
        let mut b = Bits::new(ident);
        if !check_header(&mut b, 1) {
            return Err(bad("not a Vorbis stream"));
        }
        if b.read(32) != 0 {
            return Err(bad("unknown version"));
        }
        let channels = b.read(8);
        let rate = b.read(32);
        for _ in 0..3 {
            b.read(32);
        }
        let bs0 = 1usize << b.read(4).clamp(0, 15);
        let bs1 = 1usize << b.read(4).clamp(0, 15);
        if rate < 1
            || rate > i64::from(i32::MAX)
            || channels < 1
            || bs0 < 64
            || bs1 < bs0
            || bs1 > 8192
        {
            return Err(bad("bad identification header"));
        }
        if b.read(1) != 1 {
            return Err(bad("bad identification header"));
        }
        let channels = channels as usize;
        let mut b = Bits::new(comment);
        if !check_header(&mut b, 3) {
            return Err(bad("no comment header"));
        }
        let mut b = Bits::new(setup);
        if !check_header(&mut b, 5) {
            return Err(bad("no set-up header"));
        }
        let nbooks = (b.read(8) + 1) as usize;
        let mut books = Vec::with_capacity(nbooks);
        for _ in 0..nbooks {
            books.push(Book::unpack(&mut b)?);
        }
        let times = b.read(6) + 1;
        for _ in 0..times {
            let t = b.read(16);
            if t != 0 {
                return Err(bad("bad time type"));
            }
        }
        let nfloors = (b.read(6) + 1) as usize;
        let mut floors = Vec::with_capacity(nfloors);
        for _ in 0..nfloors {
            floors.push(match b.read(16) {
                0 => Floor::Zero(Floor0::unpack(&mut b, books.len())?),
                1 => Floor::One(Box::new(Floor1::unpack(&mut b, books.len())?)),
                _ => return Err(bad("bad floor type")),
            });
        }
        let nres = (b.read(6) + 1) as usize;
        let mut residues = Vec::with_capacity(nres);
        for _ in 0..nres {
            let kind = b.read(16);
            if !(0..3).contains(&kind) {
                return Err(bad("bad residue type"));
            }
            residues.push(Residue::unpack(kind as u16, &mut b, &books)?);
        }
        let nmaps = (b.read(6) + 1) as usize;
        let mut mappings = Vec::with_capacity(nmaps);
        for _ in 0..nmaps {
            if b.read(16) != 0 {
                return Err(bad("bad mapping type"));
            }
            let submaps = if b.read(1) == 1 {
                (b.read(4) + 1) as usize
            } else {
                1
            };
            let mut coupling = Vec::new();
            if b.read(1) == 1 {
                let steps = b.read(8) + 1;
                let bits = ilog2(channels as u32);
                for _ in 0..steps {
                    let m = b.read(bits);
                    let a = b.read(bits);
                    if m < 0 || a < 0 || m == a || m as usize >= channels || a as usize >= channels
                    {
                        return Err(bad("bad coupling"));
                    }
                    coupling.push((m as usize, a as usize));
                }
            }
            if b.read(2) != 0 {
                return Err(bad("bad mapping"));
            }
            let mut chmux = vec![0usize; channels];
            if submaps > 1 {
                for c in chmux.iter_mut() {
                    let v = b.read(4);
                    if v < 0 || v as usize >= submaps {
                        return Err(bad("bad mapping"));
                    }
                    *c = v as usize;
                }
            }
            let mut subs = Vec::with_capacity(submaps);
            for _ in 0..submaps {
                b.read(8);
                let f = b.read(8);
                let r = b.read(8);
                if f < 0 || f as usize >= floors.len() || r < 0 || r as usize >= residues.len() {
                    return Err(bad("bad mapping"));
                }
                subs.push((f as usize, r as usize));
            }
            mappings.push(Mapping {
                coupling,
                chmux,
                submaps: subs,
            });
        }
        let nmodes = (b.read(6) + 1) as usize;
        let mut modes = Vec::with_capacity(nmodes);
        for _ in 0..nmodes {
            let blockflag = b.read(1) == 1;
            let windowtype = b.read(16);
            let transformtype = b.read(16);
            let mapping = b.read(8);
            if windowtype != 0
                || transformtype != 0
                || mapping < 0
                || mapping as usize >= mappings.len()
            {
                return Err(bad("bad mode"));
            }
            modes.push(Mode {
                blockflag,
                mapping: mapping as usize,
            });
        }
        if b.read(1) != 1 {
            return Err(bad("bad set-up header"));
        }
        Ok(Setup {
            channels,
            rate: rate as u32,
            blocksizes: [bs0, bs1],
            books,
            floors,
            residues,
            mappings,
            modebits: ilog2(nmodes as u32),
            modes,
            mdct: [Mdct::new(bs0), Mdct::new(bs1)],
        })
    }

    /// `vorbis_packet_blocksize`: the block size of an audio packet, or
    /// `None` for any other packet. (It reads the mode with `floor(log2)`
    /// bits, unlike the decoder.)
    fn packet_blocksize(&self, p: &[u8]) -> Option<usize> {
        let mut b = Bits::new(p);
        if b.read(1) != 0 {
            return None;
        }
        let mut modebits = 0;
        let mut v = self.modes.len();
        while v > 1 {
            modebits += 1;
            v >>= 1;
        }
        let mode = b.read(modebits);
        if mode < 0 {
            return None;
        }
        let m = self.modes.get(mode as usize)?;
        Some(self.blocksizes[usize::from(m.blockflag)])
    }
}

/// One decoded block (`vorbis_block` after `vorbis_synthesis`).
struct Block {
    w: usize,
    granulepos: i64,
    sequence: i64,
    eofflag: bool,
    pcm: Vec<Vec<f32>>,
}

/// `vorbis_synthesis` + `mapping0_inverse`: `None` for a packet that is
/// not audio or cannot be read.
fn synthesis(s: &Setup, p: &Packet, data: &[u8]) -> Option<Block> {
    let mut b = Bits::new(&data[p.range.clone()]);
    if b.read(1) != 0 {
        return None;
    }
    let mode = b.read(s.modebits);
    if mode < 0 {
        return None;
    }
    let mode = s.modes.get(mode as usize)?;
    let w = usize::from(mode.blockflag);
    if w == 1 {
        b.read(1);
        if b.read(1) == -1 {
            return None;
        }
    }
    let n = s.blocksizes[w];
    let map = &s.mappings[mode.mapping];
    let ch = s.channels;
    let mut pcm = vec![vec![0f32; n]; ch];

    let mut memo: Vec<Option<FloorMemo>> = Vec::with_capacity(ch);
    let mut nonzero = vec![false; ch];
    for (i, nz) in nonzero.iter_mut().enumerate() {
        let (f, _) = map.submaps[map.chmux[i]];
        let m = match &s.floors[f] {
            Floor::Zero(fl) => fl.inverse1(&mut b, &s.books).map(FloorMemo::Zero),
            Floor::One(fl) => fl.inverse1(&mut b, &s.books).map(FloorMemo::One),
        };
        *nz = m.is_some();
        memo.push(m);
    }
    for &(m, a) in &map.coupling {
        if nonzero[m] || nonzero[a] {
            nonzero[m] = true;
            nonzero[a] = true;
        }
    }
    for (sm, &(_, r)) in map.submaps.iter().enumerate() {
        let idx: Vec<usize> = (0..ch).filter(|&j| map.chmux[j] == sm).collect();
        let mut vecs: Vec<Vec<f32>> = idx.iter().map(|&j| std::mem::take(&mut pcm[j])).collect();
        let zero: Vec<bool> = idx.iter().map(|&j| nonzero[j]).collect();
        s.residues[r].inverse(&mut b, &s.books, &mut vecs, &zero, n);
        for (v, &j) in vecs.into_iter().zip(&idx) {
            pcm[j] = v;
        }
    }
    for &(mi, ai) in map.coupling.iter().rev() {
        let (pm, pa) = if mi < ai {
            let (x, y) = pcm.split_at_mut(ai);
            (&mut x[mi], &mut y[0])
        } else {
            let (x, y) = pcm.split_at_mut(mi);
            (&mut y[0], &mut x[ai])
        };
        for j in 0..n / 2 {
            let mag = pm[j];
            let ang = pa[j];
            if mag > 0.0 {
                if ang > 0.0 {
                    pm[j] = mag;
                    pa[j] = mag - ang;
                } else {
                    pa[j] = mag;
                    pm[j] = mag + ang;
                }
            } else if ang > 0.0 {
                pm[j] = mag;
                pa[j] = mag + ang;
            } else {
                pa[j] = mag;
                pm[j] = mag - ang;
            }
        }
    }
    for (i, out) in pcm.iter_mut().enumerate() {
        let (f, _) = map.submaps[map.chmux[i]];
        match (&s.floors[f], &memo[i]) {
            (Floor::Zero(fl), Some(FloorMemo::Zero(m))) => {
                fl.inverse2(Some(m), out, n / 2, w, s.rate)
            }
            (Floor::Zero(fl), _) => fl.inverse2(None, out, n / 2, w, s.rate),
            (Floor::One(fl), Some(FloorMemo::One(m))) => fl.inverse2(Some(m), out, n / 2),
            (Floor::One(fl), _) => fl.inverse2(None, out, n / 2),
        }
    }
    for out in pcm.iter_mut() {
        s.mdct[w].backward(out);
    }
    Some(Block {
        w,
        granulepos: p.granulepos,
        sequence: p.packetno,
        eofflag: p.eos,
        pcm,
    })
}

/// The synthesis state (`vorbis_dsp_state`).
struct Dsp {
    pcm: Vec<Vec<f32>>,
    lw: usize,
    w: usize,
    center_w: usize,
    pcm_current: i64,
    pcm_returned: i64,
    granulepos: i64,
    sequence: i64,
    sample_count: i64,
}

impl Dsp {
    fn new(s: &Setup) -> Dsp {
        let c = s.blocksizes[1] / 2;
        Dsp {
            pcm: vec![vec![0f32; s.blocksizes[1]]; s.channels],
            lw: 0,
            w: 0,
            center_w: c,
            pcm_current: c as i64,
            pcm_returned: -1,
            granulepos: -1,
            sequence: -1,
            sample_count: -1,
        }
    }

    /// `vorbis_synthesis_blockin`: overlap-add with the window, then the
    /// granule-position trimming.
    fn blockin(&mut self, s: &Setup, vb: &Block) {
        self.lw = self.w;
        self.w = vb.w;
        if self.sequence == -1 || self.sequence + 1 != vb.sequence {
            self.granulepos = -1;
            self.sample_count = -1;
        }
        self.sequence = vb.sequence;
        let n = s.blocksizes[self.w] >> 1;
        let n0 = s.blocksizes[0] >> 1;
        let n1 = s.blocksizes[1] >> 1;
        let (this_center, prev_center) = if self.center_w != 0 { (n1, 0) } else { (0, n1) };
        let ola = |pcm: &mut [f32], p: &[f32], w: &[f32], len: usize| {
            for i in 0..len {
                pcm[i] = (f64::from(pcm[i]) * f64::from(w[len - i - 1])
                    + f64::from(p[i]) * f64::from(w[i])) as f32;
            }
        };
        for (pcm, p) in self.pcm.iter_mut().zip(&vb.pcm) {
            if self.lw == 1 {
                if self.w == 1 {
                    let w = window::window(s.blocksizes[1]).unwrap_or(&[]);
                    ola(&mut pcm[prev_center..], p, w, n1);
                } else {
                    let w = window::window(s.blocksizes[0]).unwrap_or(&[]);
                    ola(&mut pcm[prev_center + n1 / 2 - n0 / 2..], p, w, n0);
                }
            } else if self.w == 1 {
                let w = window::window(s.blocksizes[0]).unwrap_or(&[]);
                let off = n1 / 2 - n0 / 2;
                let dst = &mut pcm[prev_center..];
                ola(dst, &p[off..], w, n0);
                dst[n0..n1 / 2 + n0 / 2].copy_from_slice(&p[off + n0..off + n1 / 2 + n0 / 2]);
            } else {
                let w = window::window(s.blocksizes[0]).unwrap_or(&[]);
                ola(&mut pcm[prev_center..], p, w, n0);
            }
            pcm[this_center..this_center + n].copy_from_slice(&p[n..2 * n]);
        }
        self.center_w = if self.center_w != 0 { 0 } else { n1 };
        if self.pcm_returned == -1 {
            self.pcm_returned = this_center as i64;
            self.pcm_current = this_center as i64;
        } else {
            self.pcm_returned = prev_center as i64;
            self.pcm_current =
                prev_center as i64 + (s.blocksizes[self.lw] / 4 + s.blocksizes[self.w] / 4) as i64;
        }
        let step = (s.blocksizes[self.lw] / 4 + s.blocksizes[self.w] / 4) as i64;
        if self.sample_count == -1 {
            self.sample_count = 0;
        } else {
            self.sample_count += step;
        }
        if self.granulepos == -1 {
            if vb.granulepos != -1 {
                self.granulepos = vb.granulepos;
                if self.sample_count > self.granulepos {
                    if vb.eofflag {
                        self.pcm_current -= self.sample_count - self.granulepos;
                    } else {
                        self.pcm_returned += self.sample_count - self.granulepos;
                        if self.pcm_returned > self.pcm_current {
                            self.pcm_returned = self.pcm_current;
                        }
                    }
                }
            }
        } else {
            self.granulepos += step;
            if vb.granulepos != -1 && self.granulepos != vb.granulepos {
                if self.granulepos > vb.granulepos {
                    let extra = self.granulepos - vb.granulepos;
                    if extra != 0 && vb.eofflag {
                        self.pcm_current -= extra;
                    }
                }
                self.granulepos = vb.granulepos;
            }
        }
    }

    /// `vorbis_synthesis_pcmout`: the number of frames ready.
    fn pending(&self) -> usize {
        if self.pcm_returned > -1 && self.pcm_returned < self.pcm_current {
            (self.pcm_current - self.pcm_returned) as usize
        } else {
            0
        }
    }
}

/// One packet of the logical stream being decoded, as libogg 1.1.3's
/// `ogg_stream_packetout` gives it.
#[derive(Clone, Debug)]
struct Packet {
    range: std::ops::Range<usize>,
    /// The page's granule position if this is the last packet completed
    /// on its page, else −1.
    granulepos: i64,
    eos: bool,
    packetno: i64,
}

/// A page header and where its segments are.
struct Page {
    serial: u32,
    sequence: u32,
    granule: i64,
    continued: bool,
    bos: bool,
    eos: bool,
    /// Offset of the lacing values and of the body.
    lacing: std::ops::Range<usize>,
    body: usize,
    /// Offset of the next page.
    next: usize,
}

fn page_crc(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut r = (i as u32) << 24;
            for _ in 0..8 {
                r = if r & 0x8000_0000 != 0 {
                    (r << 1) ^ 0x04c1_1db7
                } else {
                    r << 1
                };
            }
            *e = r;
        }
        t
    });
    let mut c = 0u32;
    for (i, &b) in data.iter().enumerate() {
        let b = if (22..26).contains(&i) { 0 } else { b };
        c = (c << 8) ^ t[((c >> 24) as u8 ^ b) as usize];
    }
    c
}

/// The next valid page at or after `at` (libogg's `ogg_sync_pageout`:
/// bytes that do not start a page with a good CRC are skipped).
fn next_page(data: &[u8], mut at: usize) -> Option<Page> {
    while at + 27 <= data.len() {
        if &data[at..at + 4] != b"OggS" || data[at + 4] != 0 {
            at += 1;
            continue;
        }
        let h = &data[at..];
        let nseg = usize::from(h[26]);
        let lacing = at + 27..at + 27 + nseg;
        if lacing.end > data.len() {
            return None;
        }
        let body_len: usize = data[lacing.clone()].iter().map(|&l| usize::from(l)).sum();
        let next = lacing.end + body_len;
        if next > data.len() {
            return None;
        }
        let stored = u32::from_le_bytes([h[22], h[23], h[24], h[25]]);
        if page_crc(&data[at..next]) != stored {
            at += 1;
            continue;
        }
        return Some(Page {
            serial: u32::from_le_bytes([h[14], h[15], h[16], h[17]]),
            sequence: u32::from_le_bytes([h[18], h[19], h[20], h[21]]),
            granule: i64::from_le_bytes([h[6], h[7], h[8], h[9], h[10], h[11], h[12], h[13]]),
            continued: h[5] & 1 != 0,
            bos: h[5] & 2 != 0,
            eos: h[5] & 4 != 0,
            body: lacing.end,
            lacing,
            next,
        });
    }
    None
}

/// libogg's stream state: packets assembled from the pages of one serial.
struct Assembler {
    serial: u32,
    /// Expected next page sequence number (−1: any).
    pageno: i64,
    packetno: i64,
    /// Body ranges of the packet being assembled, if any.
    partial: Vec<std::ops::Range<usize>>,
}

impl Assembler {
    fn new(serial: u32) -> Self {
        Assembler {
            serial,
            pageno: -1,
            packetno: 0,
            partial: Vec::new(),
        }
    }

    /// `ogg_stream_pagein` + every `ogg_stream_packetout`. A packet split
    /// over pages is copied into `scratch` (its range then points there,
    /// marked by `split`).
    fn pagein(&mut self, data: &[u8], pg: &Page, out: &mut Vec<(Packet, Option<Vec<u8>>)>) {
        let mut seg_skip = 0;
        let lacing = &data[pg.lacing.clone()];
        if self.pageno != -1 && i64::from(pg.sequence) != self.pageno {
            // A hole: the partial packet is lost and one packet number is used.
            self.partial.clear();
            self.packetno += 1;
        }
        if pg.continued && self.partial.is_empty() {
            // A continued packet whose start was not seen: skip its rest.
            for &l in lacing {
                seg_skip += 1;
                if l < 255 {
                    break;
                }
            }
        }
        self.pageno = i64::from(pg.sequence) + 1;
        let mut pos = pg.body
            + lacing[..seg_skip]
                .iter()
                .map(|&l| usize::from(l))
                .sum::<usize>();
        let nseg = lacing.len();
        for (k, &l) in lacing.iter().enumerate().skip(seg_skip) {
            let l = usize::from(l);
            self.partial.push(pos..pos + l);
            pos += l;
            if l < 255 {
                let last = k == nseg - 1;
                let parts = std::mem::take(&mut self.partial);
                let (range, copy) = if parts.len() == 1 {
                    (parts[0].clone(), None)
                } else {
                    let mut v = Vec::new();
                    for r in parts {
                        v.extend_from_slice(&data[r]);
                    }
                    (0..v.len(), Some(v))
                };
                out.push((
                    Packet {
                        range,
                        granulepos: if last { pg.granule } else { -1 },
                        eos: last && pg.eos,
                        packetno: self.packetno,
                    },
                    copy,
                ));
                self.packetno += 1;
            }
        }
    }
}

/// A Vorbis file decoded as the game's `ov_read` gives it: interleaved
/// signed 16-bit frames.
pub struct VorbisStream {
    data: Arc<[u8]>,
    /// Offset of the next page to read.
    at: usize,
    link: Option<Link>,
    /// Packets ready for synthesis, with their bytes when they were split.
    queue: std::collections::VecDeque<(Packet, Option<Vec<u8>>)>,
    channels: usize,
    rate: u32,
    done: bool,
}

struct Link {
    setup: Setup,
    asm: Assembler,
    dsp: Dsp,
}

impl VorbisStream {
    /// Opens a file as `ov_open_callbacks` does with seekable callbacks,
    /// positioned at its start (`ov_raw_seek(vf, 0)`).
    pub fn new(data: impl Into<Arc<[u8]>>) -> Result<VorbisStream, Error> {
        let data: Arc<[u8]> = data.into();
        let mut v = VorbisStream {
            data,
            at: 0,
            link: None,
            queue: Default::default(),
            channels: 0,
            rate: 0,
            done: false,
        };
        v.open_link(true)?;
        Ok(v)
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Reads the three headers of the link that starts at the next page
    /// (`_fetch_headers`), then queues the link's packets that
    /// `ov_raw_seek` leaves to decode.
    fn open_link(&mut self, first: bool) -> Result<(), Error> {
        let data = self.data.clone();
        let pg = next_page(&data, self.at).ok_or_else(|| Error("Vorbis: no Ogg page".into()))?;
        let serial = pg.serial;
        let mut asm = Assembler::new(serial);
        let mut pkts: Vec<(Packet, Option<Vec<u8>>)> = Vec::new();
        let mut at = self.at;
        let mut pages_eos = Vec::new();
        // Collect packets until the headers are complete.
        while pkts.len() < 3 {
            let pg =
                next_page(&data, at).ok_or_else(|| Error("Vorbis: headers cut short".into()))?;
            at = pg.next;
            if pg.serial != serial {
                continue;
            }
            let before = pkts.len();
            asm.pagein(&data, &pg, &mut pkts);
            pages_eos.extend(std::iter::repeat_n(pg.eos, pkts.len() - before));
        }
        let bytes = |p: &(Packet, Option<Vec<u8>>)| -> Vec<u8> {
            match &p.1 {
                Some(v) => v.clone(),
                None => data[p.0.range.clone()].to_vec(),
            }
        };
        let setup = Setup::new(&bytes(&pkts[0]), &bytes(&pkts[1]), &bytes(&pkts[2]))?;
        self.channels = setup.channels;
        self.rate = setup.rate;
        // ov_raw_seek(vf, 0) (first link only): the packets are scanned
        // from the start until one carries a granule position (in practice
        // the identification header, whose page has granule 0); each
        // non-audio packet scanned, and each packet completed on an
        // end-of-stream page, drops one packet from the head of the queue.
        let mut drop = 0;
        if first {
            let mut k = 0;
            let mut lastblock = 0;
            let mut scan_at = at;
            'scan: loop {
                while k < pkts.len() {
                    let thisblock = match setup.packet_blocksize(&bytes(&pkts[k])) {
                        None => {
                            drop += 1;
                            0
                        }
                        Some(b) => {
                            if pages_eos[k] {
                                drop += 1;
                            }
                            b
                        }
                    };
                    let g = pkts[k].0.granulepos;
                    k += 1;
                    if g != -1 {
                        break 'scan;
                    }
                    lastblock = thisblock;
                }
                if lastblock != 0 {
                    break;
                }
                let pg = loop {
                    match next_page(&data, scan_at) {
                        None => break 'scan,
                        Some(pg) => {
                            scan_at = pg.next;
                            if pg.serial == serial {
                                break pg;
                            }
                        }
                    }
                };
                let before = pkts.len();
                asm.pagein(&data, &pg, &mut pkts);
                pages_eos.extend(std::iter::repeat_n(pg.eos, pkts.len() - before));
                at = scan_at;
            }
        }
        let skip = drop.min(pkts.len());
        self.queue = pkts.into_iter().skip(skip).collect();
        // Without the scan, the header packets are still in the queue; the
        // decoder rejects them as non-audio.
        self.at = at;
        self.link = Some(Link {
            dsp: Dsp::new(&setup),
            setup,
            asm,
        });
        Ok(())
    }

    /// Feeds packets until frames are ready; `false` at the end.
    fn fill(&mut self) -> bool {
        loop {
            let Some(link) = self.link.as_mut() else {
                return false;
            };
            if link.dsp.pending() > 0 {
                return true;
            }
            if let Some((p, copy)) = self.queue.pop_front() {
                let vb = match &copy {
                    Some(v) => synthesis(
                        &link.setup,
                        &Packet {
                            range: 0..v.len(),
                            ..p
                        },
                        v,
                    ),
                    None => synthesis(&link.setup, &p, &self.data),
                };
                if let Some(vb) = vb {
                    link.dsp.blockin(&link.setup, &vb);
                }
                continue;
            }
            if self.done {
                return false;
            }
            let Some(pg) = next_page(&self.data, self.at) else {
                self.done = true;
                return false;
            };
            if pg.serial != link.asm.serial {
                if pg.bos {
                    // A chained stream: the next link is decoded from scratch.
                    if self.open_link(false).is_err() {
                        self.done = true;
                        return false;
                    }
                } else {
                    self.at = pg.next;
                }
                continue;
            }
            self.at = pg.next;
            let mut v = Vec::new();
            link.asm.pagein(&self.data, &pg, &mut v);
            self.queue.extend(v);
        }
    }

    /// Up to `max_frames` frames appended to `out` (interleaved), as one
    /// `ov_read` call returns them; 0 at the end of the file.
    pub fn read(&mut self, out: &mut Vec<i16>, max_frames: usize) -> usize {
        if max_frames == 0 || !self.fill() {
            return 0;
        }
        let Some(link) = self.link.as_mut() else {
            return 0;
        };
        let n = link.dsp.pending().min(max_frames);
        let start = link.dsp.pcm_returned as usize;
        let ch = link.setup.channels;
        out.reserve(n * ch);
        for j in start..start + n {
            for c in 0..ch {
                out.push(to_i16(link.dsp.pcm[c][j]));
            }
        }
        link.dsp.pcm_returned += n as i64;
        n
    }
}

/// `ov_read`'s 16-bit conversion as the game's MSVC x87 build does it:
/// `fistp` of `x × 32768` (round half to even), clamped.
fn to_i16(x: f32) -> i16 {
    let v = (x * 32768.0).round_ties_even();
    v.clamp(-32768.0, 32767.0) as i16
}

/// A whole file decoded: channels, rate and interleaved samples.
pub fn decode(data: &[u8]) -> Result<(usize, u32, Vec<i16>), Error> {
    let mut s = VorbisStream::new(data.to_vec())?;
    let mut out = Vec::new();
    while s.read(&mut out, 4096) > 0 {}
    Ok((s.channels(), s.rate(), out))
}

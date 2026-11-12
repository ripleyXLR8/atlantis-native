//! A sequential (baseline or extended Huffman) JPEG decoder exact to
//! libjpeg's defaults, the settings the original decodes with: the
//! `JDCT_ISLOW` integer inverse DCT, "fancy" (triangle) upsampling and
//! libjpeg's YCbCr→RGB tables (see [`crate::image`] for the evidence).
//!
//! Written from the published algorithms (ITU-T T.81; the IJG's
//! description of its islow IDCT, a Loeffler–Ligtenberg–Moschytz
//! factorisation scaled by 2^13 with 2 extra bits between the passes; its
//! triangle upsampler and its 16-bit fixed-point colour tables). Every
//! JPEG of the game is sequential with one scan; restart intervals,
//! several sequential scans and the 2×1 upsampler are supported as well.
//! Progressive and arithmetic-coded files are refused.
//!
//! The original links libjpeg-turbo 1.x with its SIMD code (the
//! `JSIMD_FORCE*` switches, file +0x1a6aa8..+0x1a6ae0); on overflowing blocks
//! its IDCT saturates where libjpeg's C code wraps, and [`idct_islow`]
//! follows the SIMD code. Every JPEG of the game decodes to libjpeg-turbo's
//! pixels (the test `every_jpeg_decodes_to_libjpeg_turbo_pixels`).

use std::fmt;

/// Why a JPEG cannot be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JpegError(pub String);

impl fmt::Display for JpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for JpegError {}

fn err<T>(what: &str) -> Result<T, JpegError> {
    Err(JpegError(what.to_owned()))
}

/// A decoded JPEG: 1 channel (grey) or 3 (RGB), rows top first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub pixels: Vec<u8>,
}

/// Zigzag position → position in the row-major 8×8 block.
const NATURAL_ORDER: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// A Huffman table: canonical codes by length, plus a 9-bit lookahead.
#[derive(Clone)]
struct Huffman {
    /// Largest code of each length 1..=16 (-1: none), index = length.
    maxcode: [i32; 18],
    /// Symbol index of a code of length l = code + valoffset[l].
    valoffset: [i32; 17],
    values: Vec<u8>,
    /// For the next 9 bits: (length, symbol), length 0 if longer.
    lookup: Vec<(u8, u8)>,
}

const LOOKAHEAD: u32 = 9;

impl Huffman {
    fn new(counts: &[u8; 16], values: &[u8]) -> Result<Huffman, JpegError> {
        let mut maxcode = [-1i32; 18];
        let mut valoffset = [0i32; 17];
        let mut lookup = vec![(0u8, 0u8); 1 << LOOKAHEAD];
        let mut code = 0i32;
        let mut k = 0usize;
        for len in 1..=16usize {
            let n = usize::from(counts[len - 1]);
            if n > 0 {
                valoffset[len] = k as i32 - code;
                for _ in 0..n {
                    if len as u32 <= LOOKAHEAD {
                        let shift = LOOKAHEAD - len as u32;
                        let first = (code as usize) << shift;
                        for e in &mut lookup[first..first + (1 << shift)] {
                            *e = (len as u8, values[k]);
                        }
                    }
                    code += 1;
                    k += 1;
                }
                maxcode[len] = code - 1;
            }
            if code > (1 << len) {
                return err("JPEG: bad Huffman table");
            }
            code <<= 1;
        }
        // A sentinel that ends the search past 16 bits.
        maxcode[17] = i32::MAX;
        Ok(Huffman {
            maxcode,
            valoffset,
            values: values.to_vec(),
            lookup,
        })
    }
}

/// The entropy-coded bits: 0xff00 stuffing removed; at a marker or at the
/// end of the data, zero bits (as libjpeg does with a truncated scan).
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    acc: u64,
    count: u32,
    at_marker: bool,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8], pos: usize) -> Bits<'a> {
        Bits {
            data,
            pos,
            acc: 0,
            count: 0,
            at_marker: false,
        }
    }

    fn fill(&mut self) {
        while self.count <= 56 {
            let mut byte = 0u8;
            if !self.at_marker && self.pos < self.data.len() {
                byte = self.data[self.pos];
                if byte == 0xff {
                    match self.data.get(self.pos + 1) {
                        Some(0) => self.pos += 2,
                        _ => {
                            self.at_marker = true;
                            byte = 0;
                        }
                    }
                } else {
                    self.pos += 1;
                }
            }
            self.acc |= u64::from(byte) << (56 - self.count);
            self.count += 8;
        }
    }

    fn peek(&mut self, n: u32) -> u32 {
        if self.count < n {
            self.fill();
        }
        (self.acc >> (64 - n)) as u32
    }

    fn skip(&mut self, n: u32) {
        self.acc <<= n;
        self.count -= n;
    }

    fn bits(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = self.peek(n);
        self.skip(n);
        v
    }

    fn decode(&mut self, h: &Huffman) -> u8 {
        let look = self.peek(LOOKAHEAD) as usize;
        let (len, sym) = h.lookup[look];
        if len > 0 {
            self.skip(u32::from(len));
            return sym;
        }
        let mut code = self.bits(1) as i32;
        let mut len = 1;
        while code > h.maxcode[len] {
            code = (code << 1) | self.bits(1) as i32;
            len += 1;
        }
        if len > 16 {
            // A corrupt code: libjpeg warns and takes symbol 0.
            return 0;
        }
        h.values[(code + h.valoffset[len]) as usize]
    }

    /// Ends a restart interval: drops the bits left and the RSTn marker.
    fn restart(&mut self) {
        self.acc = 0;
        self.count = 0;
        self.at_marker = false;
        while self.pos + 1 < self.data.len() {
            if self.data[self.pos] == 0xff && (0xd0..=0xd7).contains(&self.data[self.pos + 1]) {
                self.pos += 2;
                return;
            }
            if self.data[self.pos] == 0xff && self.data[self.pos + 1] != 0 {
                // Another marker: left for the marker reader.
                return;
            }
            self.pos += 1;
        }
    }

    /// Where the marker reader resumes after the scan.
    fn end(&self) -> usize {
        let mut p = self.pos;
        while p + 1 < self.data.len()
            && !(self.data[p] == 0xff && !matches!(self.data[p + 1], 0 | 0xff))
        {
            p += 1;
        }
        p
    }
}

/// `v` as a signed coefficient of `s` bits (T.81's EXTEND).
fn extend(v: u32, s: u32) -> i32 {
    if s == 0 {
        0
    } else if v < 1 << (s - 1) {
        v as i32 - (1 << s) + 1
    } else {
        v as i32
    }
}

struct Component {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    /// Blocks per row and rows of blocks, padded to whole MCUs.
    bw: usize,
    bh: usize,
    /// Samples of the component inside the picture.
    width: usize,
    height: usize,
    coefs: Vec<[i16; 64]>,
    /// The quantisation table latched at the component's first scan.
    quant: Option<[u16; 64]>,
    dc_table: usize,
    ac_table: usize,
}

struct Frame {
    width: usize,
    height: usize,
    hmax: usize,
    vmax: usize,
    comps: Vec<Component>,
}

fn u16_at(d: &[u8], at: usize) -> Result<usize, JpegError> {
    match d.get(at..at + 2) {
        Some(b) => Ok(usize::from(u16::from_be_bytes([b[0], b[1]]))),
        None => err("JPEG: truncated"),
    }
}

/// Decodes a sequential JPEG with libjpeg's default decompression settings.
pub fn decode(data: &[u8]) -> Result<Decoded, JpegError> {
    if data.get(..2) != Some(&[0xff, 0xd8]) {
        return err("JPEG: no SOI marker");
    }
    let mut pos = 2;
    let mut quant: [Option<[u16; 64]>; 4] = [None; 4];
    let mut dc: [Option<Huffman>; 4] = Default::default();
    let mut ac: [Option<Huffman>; 4] = Default::default();
    let mut frame: Option<Frame> = None;
    let mut restart_interval = 0usize;
    let mut jfif = false;
    let mut adobe_transform: Option<u8> = None;
    let mut scans = 0;
    loop {
        // Next marker (fill bytes and stray data skipped).
        while pos < data.len() && data[pos] != 0xff {
            pos += 1;
        }
        while pos < data.len() && data[pos] == 0xff {
            pos += 1;
        }
        let Some(&marker) = data.get(pos) else {
            if scans > 0 {
                break;
            }
            return err("JPEG: truncated");
        };
        pos += 1;
        match marker {
            0xd9 => break,
            0xd0..=0xd7 | 0x01 => continue,
            _ => {}
        }
        let len = u16_at(data, pos)?;
        let seg = data
            .get(pos + 2..pos + len)
            .ok_or_else(|| JpegError("JPEG: truncated segment".into()))?;
        pos += len;
        match marker {
            0xc0 | 0xc1 => {
                if frame.is_some() {
                    return err("JPEG: two frames");
                }
                frame = Some(parse_frame(seg)?);
            }
            0xc2 | 0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf => {
                return err("JPEG: only sequential Huffman JPEGs are supported");
            }
            0xc4 => {
                let mut s = seg;
                while !s.is_empty() {
                    if s.len() < 17 {
                        return err("JPEG: bad DHT");
                    }
                    let (class, id) = (s[0] >> 4, usize::from(s[0] & 15));
                    let mut counts = [0u8; 16];
                    counts.copy_from_slice(&s[1..17]);
                    let n: usize = counts.iter().map(|&c| usize::from(c)).sum();
                    let vals = s
                        .get(17..17 + n)
                        .ok_or_else(|| JpegError("JPEG: bad DHT".into()))?;
                    if id > 3 || class > 1 || n > 256 {
                        return err("JPEG: bad DHT");
                    }
                    let t = Some(Huffman::new(&counts, vals)?);
                    if class == 0 {
                        dc[id] = t;
                    } else {
                        ac[id] = t;
                    }
                    s = &s[17 + n..];
                }
            }
            0xdb => {
                let mut s = seg;
                while !s.is_empty() {
                    let (prec, id) = (s[0] >> 4, usize::from(s[0] & 15));
                    let size = if prec == 0 { 64 } else { 128 };
                    let body = s
                        .get(1..1 + size)
                        .ok_or_else(|| JpegError("JPEG: bad DQT".into()))?;
                    if id > 3 {
                        return err("JPEG: bad DQT");
                    }
                    let mut q = [0u16; 64];
                    for (k, &n) in NATURAL_ORDER.iter().enumerate() {
                        q[n] = if prec == 0 {
                            u16::from(body[k])
                        } else {
                            u16::from_be_bytes([body[2 * k], body[2 * k + 1]])
                        };
                    }
                    quant[id] = Some(q);
                    s = &s[1 + size..];
                }
            }
            0xdd => restart_interval = u16_at(seg, 0)?,
            0xe0 if seg.starts_with(b"JFIF\0") => jfif = true,
            0xee if seg.starts_with(b"Adobe") && seg.len() >= 12 => adobe_transform = Some(seg[11]),
            0xda => {
                let f = frame
                    .as_mut()
                    .ok_or_else(|| JpegError("JPEG: scan before frame".into()))?;
                let end = decode_scan(f, seg, data, pos, restart_interval, &quant, &dc, &ac)?;
                pos = end;
                scans += 1;
            }
            _ => {}
        }
    }
    let f = frame.ok_or_else(|| JpegError("JPEG: no frame".into()))?;
    if scans == 0 {
        return err("JPEG: no scan");
    }
    finish(f, jfif, adobe_transform)
}

fn parse_frame(seg: &[u8]) -> Result<Frame, JpegError> {
    if seg.len() < 6 || seg[0] != 8 {
        return err("JPEG: only 8-bit samples are supported");
    }
    let height = u16_at(seg, 1)?;
    let width = u16_at(seg, 3)?;
    let n = usize::from(seg[5]);
    if width == 0 || height == 0 || !(n == 1 || n == 3) || seg.len() < 6 + 3 * n {
        return err("JPEG: unsupported frame");
    }
    let mut comps = Vec::with_capacity(n);
    for c in seg[6..6 + 3 * n].chunks(3) {
        let (h, v) = (usize::from(c[1] >> 4), usize::from(c[1] & 15));
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) || c[2] > 3 {
            return err("JPEG: bad component");
        }
        comps.push(Component {
            id: c[0],
            h,
            v,
            tq: usize::from(c[2]),
            bw: 0,
            bh: 0,
            width: 0,
            height: 0,
            coefs: Vec::new(),
            quant: None,
            dc_table: 0,
            ac_table: 0,
        });
    }
    let hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
    let vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
    let mcux = width.div_ceil(8 * hmax);
    let mcuy = height.div_ceil(8 * vmax);
    for c in &mut comps {
        c.width = (width * c.h).div_ceil(hmax);
        c.height = (height * c.v).div_ceil(vmax);
        c.bw = mcux * c.h;
        c.bh = mcuy * c.v;
        c.coefs = vec![[0i16; 64]; c.bw * c.bh];
    }
    Ok(Frame {
        width,
        height,
        hmax,
        vmax,
        comps,
    })
}

#[allow(clippy::too_many_arguments)]
fn decode_scan(
    f: &mut Frame,
    seg: &[u8],
    data: &[u8],
    start: usize,
    restart_interval: usize,
    quant: &[Option<[u16; 64]>; 4],
    dc: &[Option<Huffman>; 4],
    ac: &[Option<Huffman>; 4],
) -> Result<usize, JpegError> {
    let n = usize::from(*seg.first().unwrap_or(&0));
    if n == 0 || seg.len() < 1 + 2 * n + 3 {
        return err("JPEG: bad SOS");
    }
    let mut members = Vec::with_capacity(n);
    for s in seg[1..1 + 2 * n].chunks(2) {
        let i = f
            .comps
            .iter()
            .position(|c| c.id == s[0])
            .ok_or_else(|| JpegError("JPEG: scan of an unknown component".into()))?;
        let c = &mut f.comps[i];
        c.dc_table = usize::from(s[1] >> 4);
        c.ac_table = usize::from(s[1] & 15);
        if c.dc_table > 3 || c.ac_table > 3 {
            return err("JPEG: bad SOS");
        }
        if dc[c.dc_table].is_none() || ac[c.ac_table].is_none() {
            return err("JPEG: missing Huffman table");
        }
        if c.quant.is_none() {
            c.quant = Some(
                quant[c.tq].ok_or_else(|| JpegError("JPEG: missing quantisation table".into()))?,
            );
        }
        members.push(i);
    }
    let mut bits = Bits::new(data, start);
    let mut pred = vec![0i32; n];
    // (component, block column, block row) of each block of an MCU.
    let (mcus_x, mcus_y, layout): (usize, usize, Vec<(usize, usize, usize)>) = if n == 1 {
        let c = &f.comps[members[0]];
        let w = c.width.div_ceil(8);
        let hgt = c.height.div_ceil(8);
        (w, hgt, vec![(0, 0, 0)])
    } else {
        let mut l = Vec::new();
        for (k, &i) in members.iter().enumerate() {
            let c = &f.comps[i];
            for y in 0..c.v {
                for x in 0..c.h {
                    l.push((k, x, y));
                }
            }
        }
        if l.len() > 10 {
            return err("JPEG: too many blocks per MCU");
        }
        (
            f.width.div_ceil(8 * f.hmax),
            f.height.div_ceil(8 * f.vmax),
            l,
        )
    };
    let mut todo = restart_interval;
    for my in 0..mcus_y {
        for mx in 0..mcus_x {
            if restart_interval > 0 {
                if todo == 0 {
                    bits.restart();
                    pred.iter_mut().for_each(|p| *p = 0);
                    todo = restart_interval;
                }
                todo -= 1;
            }
            for &(k, bx, by) in &layout {
                let c = &mut f.comps[members[k]];
                let (col, row) = if n == 1 {
                    (mx, my)
                } else {
                    (mx * c.h + bx, my * c.v + by)
                };
                let dct = dc[c.dc_table].as_ref().expect("checked");
                let act = ac[c.ac_table].as_ref().expect("checked");
                let block = &mut c.coefs[row * c.bw + col];
                *block = [0; 64];
                let s = u32::from(bits.decode(dct)).min(16);
                let diff = extend(bits.bits(s), s);
                pred[k] = pred[k].wrapping_add(diff);
                block[0] = pred[k] as i16;
                let mut z = 1;
                while z < 64 {
                    let rs = bits.decode(act);
                    let (r, s) = (usize::from(rs >> 4), u32::from(rs & 15));
                    if s == 0 {
                        if r != 15 {
                            break;
                        }
                        z += 16;
                        continue;
                    }
                    z += r;
                    // Past the end, libjpeg's natural-order table keeps
                    // pointing at the last coefficient.
                    block[NATURAL_ORDER[z.min(63)]] = extend(bits.bits(s), s) as i16;
                    z += 1;
                }
            }
        }
    }
    Ok(bits.end())
}

// The islow IDCT's constants: cos-based factors scaled by 2^13.
const CONST_BITS: u32 = 13;
const PASS1_BITS: u32 = 2;
const FIX_0_298631336: i32 = 2446;
const FIX_0_390180644: i32 = 3196;
const FIX_0_541196100: i32 = 4433;
const FIX_0_765366865: i32 = 6270;
const FIX_0_899976223: i32 = 7373;
const FIX_1_175875602: i32 = 9633;
const FIX_1_501321110: i32 = 12299;
const FIX_1_847759065: i32 = 15137;
const FIX_1_961570560: i32 = 16069;
const FIX_2_053119869: i32 = 16819;
const FIX_2_562915447: i32 = 20995;
const FIX_3_072711026: i32 = 25172;

/// `x / 2^n` rounded, as an arithmetic shift.
fn descale(x: i32, n: u32) -> i32 {
    x.wrapping_add(1 << (n - 1)) >> n
}

/// `x` cut to 16 bits, as a 16-bit SIMD lane keeps it.
fn wrap16(x: i32) -> i32 {
    i32::from(x as i16)
}

/// One 1-D pass of the islow IDCT over 8 inputs (16-bit values): the
/// unscaled outputs, before the pass's rounding shift.
///
/// The arithmetic is libjpeg-turbo's SIMD version (the one the original
/// runs on any SSE2 processor): products and the rotations in 32 bits, but
/// the four plain sums `in0 ± in4`, `in7 + in3` and `in5 + in1` in 16-bit
/// lanes. On real data no sum leaves 16 bits and this equals the C code.
fn idct_1d(i: [i32; 8]) -> [i32; 8] {
    // Even part: the rotation of inputs 2 and 6, then 0 and 4.
    let (z2, z3) = (i[2], i[6]);
    let z1 = (z2 + z3) * FIX_0_541196100;
    let tmp2 = z1 - z3 * FIX_1_847759065;
    let tmp3 = z1 + z2 * FIX_0_765366865;
    let tmp0 = wrap16(i[0] + i[4]) << CONST_BITS;
    let tmp1 = wrap16(i[0] - i[4]) << CONST_BITS;
    let (t10, t13) = (tmp0.wrapping_add(tmp3), tmp0.wrapping_sub(tmp3));
    let (t11, t12) = (tmp1.wrapping_add(tmp2), tmp1.wrapping_sub(tmp2));
    // Odd part: inputs 7, 5, 3, 1.
    let (o0, o1, o2, o3) = (i[7], i[5], i[3], i[1]);
    let z1 = (o0 + o3) * -FIX_0_899976223;
    let z2 = (o1 + o2) * -FIX_2_562915447;
    let z3 = wrap16(o0 + o2);
    let z4 = wrap16(o1 + o3);
    let z5 = (z3 + z4) * FIX_1_175875602;
    let z3 = z3 * -FIX_1_961570560 + z5;
    let z4 = z4 * -FIX_0_390180644 + z5;
    let o0 = (o0 * FIX_0_298631336).wrapping_add(z1).wrapping_add(z3);
    let o1 = (o1 * FIX_2_053119869).wrapping_add(z2).wrapping_add(z4);
    let o2 = (o2 * FIX_3_072711026).wrapping_add(z2).wrapping_add(z3);
    let o3 = (o3 * FIX_1_501321110).wrapping_add(z1).wrapping_add(z4);
    [
        t10.wrapping_add(o3),
        t11.wrapping_add(o2),
        t12.wrapping_add(o1),
        t13.wrapping_add(o0),
        t13.wrapping_sub(o0),
        t12.wrapping_sub(o1),
        t11.wrapping_sub(o2),
        t10.wrapping_sub(o3),
    ]
}

/// libjpeg's `JDCT_ISLOW` inverse DCT of a quantised block (row-major),
/// into 8 rows of 8 samples at `out[r * stride..]`.
///
/// Out-of-range values follow libjpeg-turbo's SIMD code, which the
/// original uses on SSE2 processors (libjpeg-turbo 1.x links its i386
/// SSE2 IDCT; the C code differs only on blocks no encoder produces): the
/// dequantised coefficients are 16-bit products, a block whose rows 1 to 7
/// are all zero takes a 16-bit shortcut in the first pass, the first pass
/// saturates its results to 16 bits and the samples saturate to 0..255
/// (the C code wraps them through a 10-bit table instead).
pub fn idct_islow(coefs: &[i16; 64], quant: &[u16; 64], out: &mut [u8], stride: usize) {
    let dq: [i32; 64] =
        std::array::from_fn(|k| wrap16(i32::from(coefs[k]).wrapping_mul(i32::from(quant[k]))));
    let mut ws = [0i32; 64];
    // Pass 1: columns, results scaled up by 2^PASS1_BITS.
    if coefs[8..].iter().all(|&c| c == 0) {
        // Only the first row: each column is constant.
        for (k, w) in ws.iter_mut().enumerate() {
            *w = wrap16(dq[k % 8] << PASS1_BITS);
        }
    } else {
        for c in 0..8 {
            let o = idct_1d(std::array::from_fn(|r| dq[r * 8 + c]));
            for (r, v) in o.into_iter().enumerate() {
                ws[r * 8 + c] = descale(v, CONST_BITS - PASS1_BITS).clamp(-32768, 32767);
            }
        }
    }
    // Pass 2: rows, removing both scalings and the factor 8.
    for r in 0..8 {
        let o = idct_1d(std::array::from_fn(|x| ws[r * 8 + x]));
        let row = &mut out[r * stride..r * stride + 8];
        for (x, v) in row.iter_mut().zip(o) {
            let v = descale(v, CONST_BITS + PASS1_BITS + 3);
            *x = (v.clamp(-128, 127) + 128) as u8;
        }
    }
}

/// libjpeg's triangle upsampling by 2 across one row of `w` (> 2) samples
/// into `2w` (2×1 sampling): each output sample is 3/4 of its nearer input
/// and 1/4 of the other neighbour, rounded alternately down (+1) and up
/// (+2); the two end samples are copies.
pub fn upsample_h2v1(input: &[u8], out: &mut [u8]) {
    let w = input.len();
    let s = |i: usize| i32::from(input[i]);
    out[0] = input[0];
    out[1] = ((s(0) * 3 + s(1) + 2) >> 2) as u8;
    for i in 1..w - 1 {
        let v = s(i) * 3;
        out[2 * i] = ((v + s(i - 1) + 1) >> 2) as u8;
        out[2 * i + 1] = ((v + s(i + 1) + 2) >> 2) as u8;
    }
    out[2 * w - 2] = ((s(w - 1) * 3 + s(w - 2) + 1) >> 2) as u8;
    out[2 * w - 1] = input[w - 1];
}

/// 2×2: one output row from the input row it falls in (`near`, weight 3)
/// and the nearer other row (`far`, weight 1), then the same 3:1 across
/// the columns; rounding +8 then +7 alternately (sums of 16).
pub fn upsample_h2v2(near: &[u8], far: &[u8], out: &mut [u8]) {
    let w = near.len();
    let col = |i: usize| i32::from(near[i]) * 3 + i32::from(far[i]);
    out[0] = ((col(0) * 4 + 8) >> 4) as u8;
    out[1] = ((col(0) * 3 + col(1) + 7) >> 4) as u8;
    for i in 1..w - 1 {
        let this = col(i) * 3;
        out[2 * i] = ((this + col(i - 1) + 8) >> 4) as u8;
        out[2 * i + 1] = ((this + col(i + 1) + 7) >> 4) as u8;
    }
    out[2 * w - 2] = ((col(w - 1) * 3 + col(w - 2) + 8) >> 4) as u8;
    out[2 * w - 1] = ((col(w - 1) * 4 + 7) >> 4) as u8;
}

/// A component's samples after the IDCT: `bw * 8` per row.
struct Plane {
    stride: usize,
    samples: Vec<u8>,
}

impl Plane {
    fn row(&self, y: usize) -> &[u8] {
        &self.samples[y * self.stride..(y + 1) * self.stride]
    }
}

fn idct_component(c: &Component) -> Plane {
    let stride = c.bw * 8;
    let mut samples = vec![0u8; stride * c.bh * 8];
    let quant = c.quant.unwrap_or([0; 64]);
    for by in 0..c.bh {
        for bx in 0..c.bw {
            let at = by * 8 * stride + bx * 8;
            idct_islow(&c.coefs[by * c.bw + bx], &quant, &mut samples[at..], stride);
        }
    }
    Plane { stride, samples }
}

/// Output row `y` of component `c` at the full resolution, `width` samples.
fn upsampled_row(f: &Frame, c: &Component, p: &Plane, y: usize, out: &mut [u8]) {
    let (fh, fv) = (f.hmax / c.h, f.vmax / c.v);
    let fancy_ok = c.width > 2;
    let w = c.width;
    let mut tmp = vec![0u8; 2 * w];
    let src: &[u8] = match (fh, fv) {
        (1, 1) => &p.row(y)[..w],
        (2, 1) if fancy_ok => {
            upsample_h2v1(&p.row(y)[..w], &mut tmp);
            &tmp
        }
        (2, 2) if fancy_ok => {
            let inrow = y / 2;
            // Above the first row and below the last, libjpeg repeats them.
            let other = if y.is_multiple_of(2) {
                inrow.saturating_sub(1)
            } else {
                (inrow + 1).min(c.height - 1)
            };
            upsample_h2v2(&p.row(inrow)[..w], &p.row(other)[..w], &mut tmp);
            &tmp
        }
        _ => {
            // Any other ratio: each sample repeated (libjpeg's plain
            // integer upsampling).
            tmp = (0..w * fh).map(|x| p.row(y / fv)[x / fh]).collect();
            &tmp
        }
    };
    out.copy_from_slice(&src[..out.len()]);
}

/// libjpeg's YCbCr→RGB tables (16-bit fixed point).
struct ColourTables {
    cr_r: [i32; 256],
    cb_b: [i32; 256],
    cr_g: [i32; 256],
    cb_g: [i32; 256],
}

const SCALEBITS: u32 = 16;
const ONE_HALF: i32 = 1 << (SCALEBITS - 1);

impl ColourTables {
    fn new() -> ColourTables {
        // FIX(x) = round(x * 2^16) of 1.402, 1.772, 0.71414, 0.34414.
        const FIX_1_40200: i32 = 91881;
        const FIX_1_77200: i32 = 116130;
        const FIX_0_71414: i32 = 46802;
        const FIX_0_34414: i32 = 22554;
        let mut t = ColourTables {
            cr_r: [0; 256],
            cb_b: [0; 256],
            cr_g: [0; 256],
            cb_g: [0; 256],
        };
        for i in 0..256 {
            let x = i as i32 - 128;
            t.cr_r[i] = (FIX_1_40200 * x + ONE_HALF) >> SCALEBITS;
            t.cb_b[i] = (FIX_1_77200 * x + ONE_HALF) >> SCALEBITS;
            t.cr_g[i] = -FIX_0_71414 * x;
            t.cb_g[i] = -FIX_0_34414 * x + ONE_HALF;
        }
        t
    }

    fn rgb(&self, y: u8, cb: u8, cr: u8) -> [u8; 3] {
        let (y, cb, cr) = (i32::from(y), usize::from(cb), usize::from(cr));
        let c = |v: i32| v.clamp(0, 255) as u8;
        [
            c(y + self.cr_r[cr]),
            c(y + ((self.cb_g[cb] + self.cr_g[cr]) >> SCALEBITS)),
            c(y + self.cb_b[cb]),
        ]
    }
}

fn finish(f: Frame, jfif: bool, adobe_transform: Option<u8>) -> Result<Decoded, JpegError> {
    let planes: Vec<Plane> = f.comps.iter().map(idct_component).collect();
    let (w, h) = (f.width, f.height);
    let n = f.comps.len();
    // libjpeg's guess of the colour space of 3 components: JFIF → YCbCr;
    // else Adobe's transform flag; else component ids 'R','G','B' → RGB.
    let ids: Vec<u8> = f.comps.iter().map(|c| c.id).collect();
    let ycc = if jfif {
        true
    } else if let Some(t) = adobe_transform {
        t != 0
    } else {
        ids != b"RGB"
    };
    let mut pixels = vec![0u8; w * h * n];
    let mut rows: Vec<Vec<u8>> = vec![vec![0u8; w]; n];
    let tables = ColourTables::new();
    for y in 0..h {
        for (k, c) in f.comps.iter().enumerate() {
            upsampled_row(&f, c, &planes[k], y, &mut rows[k]);
        }
        let out = &mut pixels[y * w * n..(y + 1) * w * n];
        if n == 1 {
            out.copy_from_slice(&rows[0]);
        } else if ycc {
            for (x, px) in out.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                *px = tables.rgb(rows[0][x], rows[1][x], rows[2][x]);
            }
        } else {
            for (x, px) in out.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                *px = [rows[0][x], rows[1][x], rows[2][x]];
            }
        }
    }
    Ok(Decoded {
        width: w as u32,
        height: h as u32,
        channels: n as u8,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_islow_idct_gives_libjpegs_samples() {
        for (i, (coefs, quant, want)) in IDCT_VECTORS.iter().enumerate() {
            let mut out = [0u8; 64];
            idct_islow(coefs, quant, &mut out, 8);
            assert_eq!(&out, want, "vector {i}");
        }
    }

    /// `CB_PLANE` is an 8 × 8 chroma block as libjpeg decodes it at 4:4:4;
    /// the same block at 4:2:0 and 4:2:2, upsampled by libjpeg.
    #[test]
    fn the_fancy_upsamplers_give_libjpegs_samples() {
        let row = |y: usize| &CB_PLANE[y * 8..y * 8 + 8];
        let mut out = [0u8; 16];
        for y in 0..16usize {
            let inrow = y / 2;
            let far = if y.is_multiple_of(2) {
                inrow.saturating_sub(1)
            } else {
                (inrow + 1).min(7)
            };
            upsample_h2v2(row(inrow), row(far), &mut out);
            assert_eq!(out, H2V2_CB[y * 16..y * 16 + 16], "2x2, row {y}");
        }
        for y in 0..8 {
            upsample_h2v1(row(y), &mut out);
            assert_eq!(out, H2V1_CB[y * 16..y * 16 + 16], "2x1, row {y}");
        }
    }

    #[test]
    fn the_colour_tables_give_libjpegs_rgb() {
        let t = ColourTables::new();
        for (ycc, rgb) in YCC444.chunks(3).zip(RGB444.chunks(3)) {
            assert_eq!(t.rgb(ycc[0], ycc[1], ycc[2]), rgb, "{ycc:?}");
        }
    }

    #[test]
    fn broken_or_progressive_jpegs_are_refused() {
        assert!(decode(b"\xff\xd8\xff").is_err());
        assert!(decode(b"\xff\xd8\xff\xc2\x00\x0b\x08\x00\x08\x00\x08\x01\x01\x11\x00").is_err());
    }

    /// (quantised coefficients, quantisation table, libjpeg-turbo's samples),
    /// row-major; from synthetic 8 × 8 grey JPEGs decoded by libjpeg-turbo
    /// 3.1 (Pillow 12.2), `JDCT_ISLOW`. The last five overflow: clipped
    /// samples, 16-bit products and sums.
    #[allow(clippy::type_complexity)]
    const IDCT_VECTORS: [([i16; 64], [u16; 64], [u8; 64]); 8] = [
        (
            [
                -300, 0, 46, 0, -26, -57, 58, 45, 0, 0, 60, 0, 0, 0, 0, -36, 39, 0, 20, -27, -38,
                0, 0, 0, -23, 0, -27, 0, 0, 0, 0, -33, 0, 0, -39, -21, 43, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, -47, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 22, 0, 0, 0, 0,
            ],
            [
                1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7,
                1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7,
                1, 2, 3, 4, 5, 6, 7, 1,
            ],
            [
                22, 200, 165, 43, 0, 239, 0, 172, 164, 152, 223, 0, 6, 96, 116, 255, 253, 144, 92,
                0, 22, 151, 42, 232, 100, 45, 58, 80, 125, 169, 0, 168, 71, 0, 97, 15, 255, 147, 0,
                34, 52, 73, 138, 0, 88, 154, 0, 99, 47, 164, 136, 0, 1, 255, 3, 153, 28, 116, 255,
                0, 185, 137, 76, 127,
            ],
        ),
        (
            [
                200, -18, -79, -138, 0, 0, 131, 0, -83, 191, -58, 0, 11, 124, 0, 0, -166, 0, 123,
                23, -106, 0, -25, 0, 73, 144, 115, 85, -99, -169, 0, 125, 0, -100, 0, -149, -59,
                182, 0, 158, 0, -35, 0, -126, 98, 162, 0, 36, 0, 0, -97, -36, 0, 101, 0, 120, -177,
                65, 0, 25, 41, -57, 0, 0,
            ],
            [
                1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7,
                1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7,
                1, 2, 3, 4, 5, 6, 7, 1,
            ],
            [
                255, 0, 255, 0, 0, 0, 255, 0, 109, 255, 121, 255, 0, 255, 0, 0, 0, 0, 255, 255,
                129, 12, 0, 63, 0, 0, 255, 255, 163, 120, 255, 255, 255, 209, 255, 14, 255, 255, 0,
                255, 255, 255, 255, 0, 255, 228, 0, 242, 67, 255, 0, 0, 0, 255, 0, 255, 0, 0, 255,
                0, 0, 255, 255, 255,
            ],
        ),
        (
            [
                0, -263, 167, -295, -178, -280, -253, -381, 452, -240, 162, 38, -201, -308, 83,
                -233, -104, -241, 169, 292, 168, -463, 354, -11, 391, -252, -285, -62, -60, -468,
                -81, 450, -88, 262, -448, -405, 394, 290, -168, -459, 350, 6, 410, 78, 3, -294,
                485, 10, -80, 390, -69, -463, -246, -46, -355, 412, 234, 309, -276, 422, 41, -73,
                -51, -287,
            ],
            [
                1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7,
                1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7,
                1, 2, 3, 4, 5, 6, 7, 1,
            ],
            [
                255, 255, 255, 255, 0, 0, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0, 255, 88, 0, 0,
                0, 255, 255, 255, 0, 255, 0, 255, 0, 255, 0, 0, 255, 0, 255, 255, 0, 0, 0, 0, 255,
                255, 0, 255, 0, 255, 255, 255, 0, 0, 0, 255, 255, 0, 255, 0, 255, 255, 255, 0, 255,
                255, 0,
            ],
        ),
        (
            [
                -1024, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            [
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 1, 1, 1,
            ],
            [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0,
            ],
        ),
        (
            [
                0, 0, 0, 0, 0, 0, 0, 0, 700, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            [
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 1, 1, 1,
            ],
            [
                249, 249, 249, 249, 249, 249, 249, 249, 231, 231, 231, 231, 231, 231, 231, 231,
                197, 197, 197, 197, 197, 197, 197, 197, 152, 152, 152, 152, 152, 152, 152, 152,
                104, 104, 104, 104, 104, 104, 104, 104, 59, 59, 59, 59, 59, 59, 59, 59, 25, 25, 25,
                25, 25, 25, 25, 25, 7, 7, 7, 7, 7, 7, 7, 7,
            ],
        ),
        (
            [
                2000, 0, 700, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            [
                255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            ],
            [
                108, 255, 255, 255, 255, 255, 255, 108, 108, 255, 255, 255, 255, 255, 255, 108,
                108, 255, 255, 255, 255, 255, 255, 108, 108, 255, 255, 255, 255, 255, 255, 108,
                108, 255, 255, 255, 255, 255, 255, 108, 108, 255, 255, 255, 255, 255, 255, 108,
                108, 255, 255, 255, 255, 255, 255, 108, 108, 255, 255, 255, 255, 255, 255, 108,
            ],
        ),
        (
            [
                2000, 0, 0, -900, 1000, 800, 0, 0, 0, 1000, 0, 0, 0, 0, 0, 0, 0, -1000, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            [
                200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200,
                200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200,
                200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200,
                200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200,
            ],
            [
                0, 0, 0, 0, 255, 255, 255, 0, 125, 0, 0, 0, 255, 255, 255, 0, 255, 0, 0, 0, 255,
                255, 255, 0, 255, 0, 0, 0, 255, 255, 255, 0, 255, 0, 0, 0, 255, 255, 255, 0, 255,
                0, 0, 0, 0, 255, 255, 0, 255, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 255, 255, 0,
            ],
        ),
        (
            [
                1500, 0, 0, -243, -914, 635, 0, 0, 0, -568, 0, -56, 0, 580, -702, 0, 0, 0, 771,
                -197, 0, -1017, 505, 0, 0, 0, -786, -591, 0, 0, 0, -224, -213, 0, 865, 0, 0, 385,
                0, 0, 0, 104, 611, 876, 226, 0, 553, 774, 0, 0, 0, 0, -576, -997, 0, 0, 0, 0, 277,
                776, 142, -72, -496, 0,
            ],
            [
                37, 40, 43, 46, 49, 52, 55, 58, 61, 64, 67, 37, 40, 43, 46, 49, 52, 55, 58, 61, 64,
                67, 37, 40, 43, 46, 49, 52, 55, 58, 61, 64, 67, 37, 40, 43, 46, 49, 52, 55, 58, 61,
                64, 67, 37, 40, 43, 46, 49, 52, 55, 58, 61, 64, 67, 37, 40, 43, 46, 49, 52, 55, 58,
                61,
            ],
            [
                0, 0, 255, 255, 255, 0, 255, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255, 255,
                255, 0, 0, 0, 0, 255, 0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 0, 255, 255, 0, 255, 0, 0,
                0, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 255, 255, 255, 0, 255, 0, 0, 255,
            ],
        ),
    ];
    const YCC444: [u8; 192] = [
        131, 142, 131, 130, 128, 111, 120, 123, 129, 132, 106, 120, 121, 102, 129, 138, 125, 144,
        127, 133, 130, 125, 125, 146, 122, 137, 119, 129, 146, 101, 133, 135, 129, 130, 127, 104,
        133, 103, 104, 121, 101, 143, 121, 127, 108, 135, 109, 127, 132, 117, 123, 122, 115, 126,
        138, 142, 130, 120, 125, 139, 125, 124, 147, 121, 140, 126, 142, 112, 129, 124, 109, 117,
        126, 101, 151, 129, 123, 127, 126, 113, 150, 123, 135, 141, 145, 138, 131, 126, 129, 151,
        121, 143, 113, 128, 101, 140, 126, 113, 122, 129, 122, 153, 132, 152, 124, 137, 126, 116,
        110, 123, 121, 124, 134, 110, 135, 101, 148, 132, 114, 110, 121, 106, 148, 136, 134, 123,
        121, 119, 125, 135, 127, 146, 128, 127, 137, 134, 119, 126, 116, 137, 116, 132, 115, 149,
        138, 115, 128, 125, 114, 170, 120, 113, 138, 126, 97, 137, 126, 120, 141, 136, 148, 136,
        133, 133, 181, 121, 143, 142, 129, 118, 115, 125, 136, 129, 133, 122, 119, 121, 108, 148,
        136, 113, 147, 124, 120, 127, 129, 131, 144, 127, 136, 139,
    ];
    const RGB444: [u8; 192] = [
        135, 124, 156, 106, 142, 130, 121, 121, 111, 121, 145, 93, 122, 129, 75, 160, 128, 133,
        130, 124, 136, 150, 113, 120, 109, 125, 138, 91, 142, 161, 134, 130, 145, 96, 147, 128, 99,
        159, 89, 142, 120, 73, 93, 136, 119, 134, 142, 101, 125, 139, 113, 119, 128, 99, 141, 132,
        163, 135, 113, 115, 152, 113, 118, 118, 118, 142, 143, 147, 114, 109, 138, 90, 158, 119,
        78, 128, 131, 120, 157, 115, 99, 141, 111, 135, 149, 139, 163, 158, 109, 128, 100, 127,
        148, 145, 129, 80, 118, 135, 99, 164, 113, 118, 126, 127, 175, 120, 146, 133, 100, 117,
        101, 99, 135, 135, 163, 130, 87, 107, 150, 107, 149, 114, 82, 129, 138, 147, 117, 126, 105,
        160, 122, 133, 141, 122, 126, 131, 139, 118, 99, 121, 132, 161, 121, 109, 138, 142, 115,
        184, 100, 100, 134, 118, 93, 139, 130, 71, 144, 119, 112, 147, 123, 171, 207, 93, 142, 141,
        106, 148, 111, 142, 111, 126, 122, 139, 120, 141, 122, 149, 114, 86, 163, 128, 109, 123,
        127, 110, 151, 117, 134, 142, 116, 141,
    ];
    const CB_PLANE: [u8; 64] = [
        142, 128, 123, 106, 102, 125, 133, 125, 137, 146, 135, 127, 103, 101, 127, 109, 117, 115,
        142, 125, 124, 140, 112, 109, 101, 123, 113, 135, 138, 129, 143, 101, 113, 122, 152, 126,
        123, 134, 101, 114, 106, 134, 119, 127, 127, 119, 137, 115, 115, 114, 113, 97, 120, 148,
        133, 143, 118, 136, 122, 108, 113, 120, 131, 136,
    ];
    const H2V2_CB: [u8; 256] = [
        142, 138, 132, 127, 124, 119, 110, 105, 103, 108, 119, 127, 131, 131, 127, 125, 141, 139,
        135, 131, 128, 122, 115, 109, 105, 106, 115, 122, 128, 129, 124, 121, 138, 139, 141, 139,
        134, 129, 124, 117, 108, 104, 106, 112, 123, 125, 117, 113, 132, 134, 137, 138, 137, 134,
        129, 122, 113, 109, 110, 114, 120, 120, 113, 109, 122, 122, 123, 127, 136, 137, 129, 124,
        120, 122, 127, 127, 119, 114, 111, 109, 113, 114, 116, 121, 130, 133, 129, 127, 128, 130,
        135, 133, 124, 117, 110, 107, 105, 109, 117, 121, 120, 123, 129, 133, 134, 134, 132, 133,
        134, 127, 111, 103, 104, 109, 118, 123, 123, 125, 130, 133, 134, 133, 131, 131, 132, 125,
        111, 104, 110, 113, 119, 127, 137, 139, 132, 128, 127, 128, 131, 127, 117, 111, 111, 111,
        111, 115, 122, 130, 139, 139, 131, 126, 125, 126, 129, 125, 115, 111, 113, 114, 108, 114,
        125, 130, 128, 127, 127, 127, 126, 125, 124, 124, 127, 125, 118, 115, 108, 113, 124, 126,
        120, 118, 119, 121, 124, 125, 126, 129, 134, 132, 126, 122, 113, 114, 117, 118, 116, 112,
        107, 109, 117, 126, 136, 139, 136, 134, 136, 136, 116, 117, 119, 118, 116, 111, 104, 104,
        114, 124, 135, 139, 135, 135, 139, 141, 117, 121, 127, 128, 122, 116, 109, 108, 112, 118,
        124, 128, 130, 133, 136, 138, 118, 122, 132, 132, 126, 118, 112, 109, 112, 115, 118, 123,
        128, 132, 135, 136,
    ];
    const H2V1_CB: [u8; 128] = [
        142, 139, 131, 127, 124, 119, 110, 105, 103, 108, 119, 127, 131, 131, 127, 125, 137, 139,
        144, 143, 138, 133, 129, 121, 109, 103, 101, 108, 120, 123, 113, 109, 117, 117, 115, 122,
        135, 138, 129, 125, 124, 128, 136, 133, 119, 111, 110, 109, 101, 107, 117, 121, 115, 119,
        129, 136, 137, 136, 131, 133, 139, 133, 111, 101, 113, 115, 120, 130, 144, 146, 132, 125,
        124, 126, 131, 126, 109, 104, 111, 114, 106, 113, 127, 130, 123, 121, 125, 127, 127, 125,
        121, 124, 132, 132, 120, 115, 115, 115, 114, 114, 113, 109, 101, 103, 114, 127, 141, 144,
        137, 136, 140, 143, 118, 123, 131, 133, 125, 119, 111, 109, 112, 115, 118, 123, 128, 132,
        135, 136,
    ];
}

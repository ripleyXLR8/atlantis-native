// Ported from ScummVM's `math/fft.cpp`, `math/rdft.cpp`, `math/dct.cpp`,
// `math/cosinetables.cpp` and `math/sinetables.cpp` (ScummVM - Graphic
// Adventure Engine, Copyright (C) the ScummVM team, GNU General Public
// License, version 3 or later). Those are based on eos' code, in turn
// based upon FFmpeg's (I)FFT, (I)RDFT and DCT (Copyright (c) 2008 Loren
// Merritt, (c) 2002 Fabrice Bellard, (c) 2009 Alex Converse, (c) 2010
// Vitor Sessak; partly based on libdjbfft by D. J. Bernstein).

//! The transforms of Bink audio: ScummVM's split-radix FFT, the real
//! DFT built on it and the DCT built on that, in single precision as
//! ScummVM computes them (its tables are `cos`/`sin` in double precision
//! stored as floats).

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default)]
struct Complex {
    re: f32,
    im: f32,
}

/// `CosineTable(n).getTable()`: cos(2πi/n) for 0 ≤ i ≤ n/4, then the
/// mirror up to n/2 − 1 (the "EOS" table, n/2 entries).
fn cosine_table(n: usize) -> Vec<f32> {
    let rad = 2.0 * PI / n as f64;
    let mut t = vec![0f32; n / 2];
    for (i, v) in t.iter_mut().enumerate().take(n / 4 + 1) {
        *v = (i as f64 * rad).cos() as f32;
    }
    for i in 1..n / 4 {
        t[n / 2 - i] = t[i];
    }
    t
}

/// `SineTable(n).getTable()`: sin(2πi/n) for 0 ≤ i < n/4, then the same
/// negated (n/2 entries).
fn sine_table(n: usize) -> Vec<f32> {
    let rad = 2.0 * PI / n as f64;
    let mut t = vec![0f32; n / 2];
    for (i, v) in t.iter_mut().enumerate().take(n / 4) {
        *v = (i as f64 * rad).sin() as f32;
    }
    for i in 0..n / 4 {
        t[n / 4 + i] = -t[i];
    }
    t
}

/// `FFT::splitRadixPermutation`.
fn split_radix_permutation(i: i32, n: i32, inverse: bool) -> i32 {
    if n <= 2 {
        return i & 1;
    }
    let mut m = n >> 1;
    if i & m == 0 {
        return split_radix_permutation(i, m, inverse) * 2;
    }
    m >>= 1;
    if inverse == (i & m == 0) {
        return split_radix_permutation(i, m, inverse) * 4 + 1;
    }
    split_radix_permutation(i, m, inverse) * 4 - 1
}

const SQRTHALF: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// `BUTTERFLIES(a0, a1, a2, a3)` with the temporaries t1, t2, t5, t6.
#[allow(clippy::many_single_char_names)]
fn butterflies(z: &mut [Complex], a: [usize; 4], t: [f32; 4]) {
    let [a0, a1, a2, a3] = a;
    let [t1, t2, mut t5, mut t6] = t;
    let t3 = t5 - t1;
    t5 += t1;
    z[a2].re = z[a0].re - t5;
    z[a0].re += t5;
    z[a3].im = z[a1].im - t3;
    z[a1].im += t3;
    let t4 = t2 - t6;
    t6 += t2;
    z[a3].re = z[a1].re - t4;
    z[a1].re += t4;
    z[a2].im = z[a0].im - t6;
    z[a0].im += t6;
}

/// `TRANSFORM(a0, a1, a2, a3, wre, wim)`.
fn transform(z: &mut [Complex], a: [usize; 4], wre: f32, wim: f32) {
    let (p2, p3) = (z[a[2]], z[a[3]]);
    let t1 = p2.re * wre + p2.im * wim;
    let t2 = p2.im * wre - p2.re * wim;
    let t5 = p3.re * wre - p3.im * wim;
    let t6 = p3.im * wre + p3.re * wim;
    butterflies(z, a, [t1, t2, t5, t6]);
}

/// `TRANSFORM_ZERO(a0, a1, a2, a3)`.
fn transform_zero(z: &mut [Complex], a: [usize; 4]) {
    let (p2, p3) = (z[a[2]], z[a[3]]);
    butterflies(z, a, [p2.re, p2.im, p3.re, p3.im]);
}

/// `pass` (and `pass_big`, the same values) on z[0 .. 8n − 1] with the
/// cosines `wre` (`wim` = wre + 2n, read backwards).
fn pass(z: &mut [Complex], wre: &[f32], n: usize) {
    let (o1, o2, o3) = (2 * n, 4 * n, 6 * n);
    let wim = o1;
    transform_zero(z, [0, o1, o2, o3]);
    transform(z, [1, o1 + 1, o2 + 1, o3 + 1], wre[1], wre[wim - 1]);
    for k in 1..n {
        let b = 2 * k;
        transform(z, [b, o1 + b, o2 + b, o3 + b], wre[b], wre[wim - b]);
        transform(
            z,
            [b + 1, o1 + b + 1, o2 + b + 1, o3 + b + 1],
            wre[b + 1],
            wre[wim - b - 1],
        );
    }
}

fn fft4(z: &mut [Complex]) {
    let t3 = z[0].re - z[1].re;
    let t1 = z[0].re + z[1].re;
    let t8 = z[3].re - z[2].re;
    let t6 = z[3].re + z[2].re;
    z[2].re = t1 - t6;
    z[0].re = t1 + t6;
    let t4 = z[0].im - z[1].im;
    let t2 = z[0].im + z[1].im;
    let t7 = z[2].im - z[3].im;
    let t5 = z[2].im + z[3].im;
    z[3].im = t4 - t8;
    z[1].im = t4 + t8;
    z[3].re = t3 - t7;
    z[1].re = t3 + t7;
    z[2].im = t2 - t5;
    z[0].im = t2 + t5;
}

fn fft8(z: &mut [Complex]) {
    fft4(z);
    let mut t1 = z[4].re - -z[5].re;
    z[5].re = z[4].re + -z[5].re;
    let mut t2 = z[4].im - -z[5].im;
    z[5].im = z[4].im + -z[5].im;
    let t3 = z[6].re - -z[7].re;
    z[7].re = z[6].re + -z[7].re;
    let t4 = z[6].im - -z[7].im;
    z[7].im = z[6].im + -z[7].im;
    let t8 = t3 - t1;
    t1 += t3;
    let t7 = t2 - t4;
    t2 += t4;
    z[4].re = z[0].re - t1;
    z[0].re += t1;
    z[4].im = z[0].im - t2;
    z[0].im += t2;
    z[6].re = z[2].re - t7;
    z[2].re += t7;
    z[6].im = z[2].im - t8;
    z[2].im += t8;
    transform(z, [1, 3, 5, 7], SQRTHALF, SQRTHALF);
}

/// ScummVM's `FFT`: a complex FFT of 2^bits points (its input permuted
/// first), without normalisation.
struct Fft {
    bits: u32,
    rev: Vec<u16>,
    /// `_cosTables[i]` (2^(i + 4) points) for i + 4 ≤ bits.
    cos: Vec<Vec<f32>>,
}

impl Fft {
    fn new(bits: u32, inverse: bool) -> Fft {
        let n = 1usize << bits;
        let mut rev = vec![0u16; n];
        for i in 0..n {
            let p = -split_radix_permutation(i as i32, n as i32, inverse) & (n as i32 - 1);
            rev[p as usize] = i as u16;
        }
        let cos = (0..13)
            .take_while(|i| i + 4 <= bits)
            .map(|i| cosine_table(1 << (i + 4)))
            .collect();
        Fft { bits, rev, cos }
    }

    fn permute(&self, z: &mut [Complex]) {
        let mut tmp = vec![Complex::default(); z.len()];
        for (j, v) in z.iter().enumerate() {
            tmp[self.rev[j] as usize] = *v;
        }
        z.copy_from_slice(&tmp);
    }

    fn fft16(&self, z: &mut [Complex]) {
        fft8(z);
        fft4(&mut z[8..]);
        fft4(&mut z[12..]);
        let c = &self.cos[0];
        transform_zero(z, [0, 4, 8, 12]);
        transform(z, [2, 6, 10, 14], SQRTHALF, SQRTHALF);
        transform(z, [1, 5, 9, 13], c[1], c[3]);
        transform(z, [3, 7, 11, 15], c[3], c[1]);
    }

    fn fft(&self, n: usize, logn: u32, z: &mut [Complex]) {
        match logn {
            2 => fft4(z),
            3 => fft8(z),
            4 => self.fft16(z),
            _ => {
                self.fft(n / 2, logn - 1, z);
                self.fft(n / 4, logn - 2, &mut z[(n / 4) * 2..]);
                self.fft(n / 4, logn - 2, &mut z[(n / 4) * 3..]);
                pass(z, &self.cos[(logn - 4) as usize], (n / 4) / 2);
            }
        }
    }

    fn calc(&self, z: &mut [Complex]) {
        self.fft(1 << self.bits, self.bits, z);
    }
}

/// `RDFT::TransformType`.
/// ScummVM's four; Bink audio uses `DftC2r` (its RDFT codec) and
/// `IdftC2r` (under the DCT).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RdftKind {
    DftR2c,
    IdftC2r,
    IdftR2c,
    DftC2r,
}

/// ScummVM's `RDFT`: a real DFT of 2^bits points.
pub(super) struct Rdft {
    bits: u32,
    inverse: bool,
    sign: f32,
    fft: Fft,
    sin: Vec<f32>,
    sin_at: usize,
    cos: Vec<f32>,
}

impl Rdft {
    pub(super) fn new(bits: u32, kind: RdftKind) -> Rdft {
        use RdftKind::*;
        let n = 1usize << bits;
        let inverse = matches!(kind, IdftC2r | DftC2r);
        Rdft {
            bits,
            inverse,
            sign: if matches!(kind, IdftR2c | DftC2r) {
                1.0
            } else {
                -1.0
            },
            fft: Fft::new(bits - 1, matches!(kind, IdftC2r | IdftR2c)),
            sin: sine_table(n),
            sin_at: if matches!(kind, DftR2c | DftC2r) {
                n >> 2
            } else {
                0
            },
            cos: cosine_table(n),
        }
    }

    fn fft_in_place(&self, data: &mut [f32]) {
        let mut z: Vec<Complex> = data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| Complex { re: c[0], im: c[1] })
            .collect();
        self.fft.permute(&mut z);
        self.fft.calc(&mut z);
        for (c, v) in data.as_chunks_mut::<2>().0.iter_mut().zip(&z) {
            c[0] = v.re;
            c[1] = v.im;
        }
    }

    /// `RDFT::calc` on 2^bits floats.
    pub(super) fn calc(&self, data: &mut [f32]) {
        let n = 1usize << self.bits;
        let k1 = 0.5f32;
        let k2 = 0.5f32 - f32::from(u8::from(self.inverse));
        if !self.inverse {
            self.fft_in_place(&mut data[..n]);
        }
        let ev_re = data[0];
        data[0] = ev_re + data[1];
        data[1] = ev_re - data[1];
        let (tcos, tsin) = (&self.cos, &self.sin[self.sin_at..]);
        let mut i = 1;
        while i < (n >> 2) {
            let i1 = 2 * i;
            let i2 = n - i1;
            let ev_re = k1 * (data[i1] + data[i2]);
            let od_im = -k2 * (data[i1] - data[i2]);
            let ev_im = k1 * (data[i1 + 1] - data[i2 + 1]);
            let od_re = k2 * (data[i1 + 1] + data[i2 + 1]);
            data[i1] = ev_re + od_re * tcos[i] - od_im * tsin[i];
            data[i1 + 1] = ev_im + od_im * tcos[i] + od_re * tsin[i];
            data[i2] = ev_re - od_re * tcos[i] + od_im * tsin[i];
            data[i2 + 1] = -ev_im + od_im * tcos[i] + od_re * tsin[i];
            i += 1;
        }
        data[2 * i + 1] *= self.sign;
        if self.inverse {
            data[0] *= k1;
            data[1] *= k1;
            self.fft_in_place(&mut data[..n]);
        }
    }
}

/// ScummVM's `DCT` of type III (the only one Bink audio uses).
pub(super) struct Dct3 {
    bits: u32,
    cos: Vec<f32>,
    csc2: Vec<f32>,
    rdft: Rdft,
}

impl Dct3 {
    pub(super) fn new(bits: u32) -> Dct3 {
        let n = 1usize << bits;
        let csc2 = (0..n / 2)
            .map(|i| (0.5 / (PI / (2 * n) as f64 * (2 * i + 1) as f64).sin()) as f32)
            .collect();
        Dct3 {
            bits,
            cos: cosine_table(1 << (bits + 2)),
            csc2,
            rdft: Rdft::new(bits, RdftKind::IdftC2r),
        }
    }

    /// `DCT::calcDCTIII` on 2^bits floats.
    pub(super) fn calc(&self, data: &mut [f32]) {
        let n = 1usize << self.bits;
        let cos = |x: usize| self.cos[x];
        let sin = |x: usize| self.cos[n - x];
        let next = data[n - 1];
        let inv_n = 1.0f32 / n as f32;
        let mut i = n - 2;
        while i >= 2 {
            let val1 = data[i];
            let val2 = data[i - 1] - data[i + 1];
            let (c, s) = (cos(i), sin(i));
            data[i] = c * val1 + s * val2;
            data[i + 1] = s * val1 - c * val2;
            i -= 2;
        }
        data[1] = 2.0 * next;
        self.rdft.calc(data);
        for i in 0..n / 2 {
            let mut tmp1 = data[i] * inv_n;
            let tmp2 = data[n - i - 1] * inv_n;
            let csc = self.csc2[i] * (tmp1 - tmp2);
            tmp1 += tmp2;
            data[i] = tmp1 + csc;
            data[n - i - 1] = tmp1 - csc;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The FFT against a direct DFT (forward: e^(−2πi jk/n)).
    #[test]
    fn the_fft_is_a_dft() {
        for bits in 2..=8u32 {
            let n = 1usize << bits;
            let x: Vec<Complex> = (0..n)
                .map(|k| Complex {
                    re: ((k * 7 + 3) % 11) as f32 - 5.0,
                    im: ((k * 5 + 1) % 13) as f32 - 6.0,
                })
                .collect();
            let f = Fft::new(bits, false);
            let mut z = x.clone();
            f.permute(&mut z);
            f.calc(&mut z);
            for (j, out) in z.iter().enumerate() {
                let (mut re, mut im) = (0f64, 0f64);
                for (k, v) in x.iter().enumerate() {
                    let a = -2.0 * PI * (j * k) as f64 / n as f64;
                    re += f64::from(v.re) * a.cos() - f64::from(v.im) * a.sin();
                    im += f64::from(v.re) * a.sin() + f64::from(v.im) * a.cos();
                }
                assert!(
                    (f64::from(out.re) - re).abs() < 1e-3 * n as f64,
                    "{bits} {j}"
                );
                assert!(
                    (f64::from(out.im) - im).abs() < 1e-3 * n as f64,
                    "{bits} {j}"
                );
            }
        }
    }

    /// The DCT-III against its definition as ScummVM's scales it:
    /// y_k = x_0 / 2 + Σ_{j≥1} x_j cos(π j (2k + 1) / 2n), times 2/n.
    #[test]
    fn the_dct_iii() {
        for bits in 4..=8u32 {
            let n = 1usize << bits;
            let x: Vec<f32> = (0..n).map(|k| ((k * 7 + 3) % 11) as f32 - 5.0).collect();
            let d = Dct3::new(bits);
            let mut y = x.clone();
            d.calc(&mut y);
            for (k, got) in y.iter().enumerate() {
                let mut s = f64::from(x[0]) / 2.0;
                for (j, v) in x.iter().enumerate().skip(1) {
                    s += f64::from(*v) * (PI * (j * (2 * k + 1)) as f64 / (2 * n) as f64).cos();
                }
                let want = s * 2.0 / n as f64;
                assert!(
                    (f64::from(*got) - want).abs() < 1e-3,
                    "{bits} {k}: {} {want}",
                    got
                );
            }
        }
    }
}

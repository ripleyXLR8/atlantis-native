//! The inverse MDCT of libvorbis 1.2.0 (`mdct.c`, `mdct_backward`): the
//! same butterflies, in the same order, with the same roundings. Every C
//! expression is evaluated in `f64` and rounded to `f32` where C assigns
//! it to a float (array element or `REG_TYPE` local), as the game's x87
//! code at 53-bit precision does; a lone `+`, `−` or `×` of two floats is
//! the same in `f32`.

/// `cPI3_8`, `cPI2_8`, `cPI1_8`: float literals, rounded to float.
const C_PI3_8: f64 = 0.382_683_432_365_089_771_75_f32 as f64;
const C_PI2_8: f64 = 0.707_106_781_186_547_524_41_f32 as f64;
const C_PI1_8: f64 = 0.923_879_532_511_286_756_13_f32 as f64;

#[inline]
fn d(x: f32) -> f64 {
    f64::from(x)
}

pub(crate) struct Mdct {
    n: usize,
    log2n: u32,
    trig: Vec<f32>,
    bitrev: Vec<usize>,
}

impl Mdct {
    /// `mdct_init`. `M_PI` is libvorbis's float `3.1415926536f`; each
    /// argument is computed in double, `cos` / `sin` in double, the result
    /// stored as float.
    pub(crate) fn new(n: usize) -> Mdct {
        let pi = d(std::f32::consts::PI);
        let n2 = n >> 1;
        let log2n = n.trailing_zeros();
        let nf = n as f64;
        let mut t = vec![0f32; n + n / 4];
        for i in 0..n / 4 {
            let a = (pi / nf) * (4 * i) as f64;
            let b = (pi / (2.0 * nf)) * (2 * i + 1) as f64;
            t[i * 2] = libm::cos(a) as f32;
            t[i * 2 + 1] = (-libm::sin(a)) as f32;
            t[n2 + i * 2] = libm::cos(b) as f32;
            t[n2 + i * 2 + 1] = libm::sin(b) as f32;
        }
        for i in 0..n / 8 {
            let a = (pi / nf) * (4 * i + 2) as f64;
            t[n + i * 2] = (libm::cos(a) * 0.5) as f32;
            t[n + i * 2 + 1] = (-libm::sin(a) * 0.5) as f32;
        }
        let mask = (1usize << (log2n - 1)) - 1;
        let msb = 1usize << (log2n - 2);
        let mut bitrev = vec![0usize; n / 4];
        for i in 0..n / 8 {
            let mut acc = 0usize;
            let mut j = 0;
            while msb >> j != 0 {
                if (msb >> j) & i != 0 {
                    acc |= 1 << j;
                }
                j += 1;
            }
            bitrev[i * 2] = ((!acc) & mask).wrapping_sub(1);
            bitrev[i * 2 + 1] = acc;
        }
        Mdct {
            n,
            log2n,
            trig: t,
            bitrev,
        }
    }

    /// `mdct_backward(init, x, x)`: `x[..n/2]` in, `x[..n]` out.
    pub(crate) fn backward(&self, x: &mut [f32]) {
        let n = self.n;
        let n2 = n >> 1;
        let n4 = n >> 2;
        let t = &self.trig;

        // rotate
        {
            let mut ix = n2 as isize - 7;
            let mut ox = n2 + n4;
            let mut ti = n4;
            loop {
                ox -= 4;
                let i = ix as usize;
                x[ox] = (-d(x[i + 2]) * d(t[ti + 3]) - d(x[i]) * d(t[ti + 2])) as f32;
                x[ox + 1] = (d(x[i]) * d(t[ti + 3]) - d(x[i + 2]) * d(t[ti + 2])) as f32;
                x[ox + 2] = (-d(x[i + 6]) * d(t[ti + 1]) - d(x[i + 4]) * d(t[ti])) as f32;
                x[ox + 3] = (d(x[i + 4]) * d(t[ti + 1]) - d(x[i + 6]) * d(t[ti])) as f32;
                ix -= 8;
                ti += 4;
                if ix < 0 {
                    break;
                }
            }
            let mut ix = n2 as isize - 8;
            let mut ox = n2 + n4;
            let mut ti = n4;
            loop {
                ti -= 4;
                let i = ix as usize;
                x[ox] = (d(x[i + 4]) * d(t[ti + 3]) + d(x[i + 6]) * d(t[ti + 2])) as f32;
                x[ox + 1] = (d(x[i + 4]) * d(t[ti + 2]) - d(x[i + 6]) * d(t[ti + 3])) as f32;
                x[ox + 2] = (d(x[i]) * d(t[ti + 1]) + d(x[i + 2]) * d(t[ti])) as f32;
                x[ox + 3] = (d(x[i]) * d(t[ti]) - d(x[i + 2]) * d(t[ti + 1])) as f32;
                ix -= 8;
                ox += 4;
                if ix < 0 {
                    break;
                }
            }
        }

        self.butterflies(&mut x[n2..], n2);
        self.bitreverse(x);

        // rotate + window
        {
            let mut ox1 = n2 + n4;
            let mut ox2 = n2 + n4;
            let mut ix = 0;
            let mut ti = n2;
            loop {
                ox1 -= 4;
                x[ox1 + 3] = (d(x[ix]) * d(t[ti + 1]) - d(x[ix + 1]) * d(t[ti])) as f32;
                x[ox2] = (-(d(x[ix]) * d(t[ti]) + d(x[ix + 1]) * d(t[ti + 1]))) as f32;
                x[ox1 + 2] = (d(x[ix + 2]) * d(t[ti + 3]) - d(x[ix + 3]) * d(t[ti + 2])) as f32;
                x[ox2 + 1] = (-(d(x[ix + 2]) * d(t[ti + 2]) + d(x[ix + 3]) * d(t[ti + 3]))) as f32;
                x[ox1 + 1] = (d(x[ix + 4]) * d(t[ti + 5]) - d(x[ix + 5]) * d(t[ti + 4])) as f32;
                x[ox2 + 2] = (-(d(x[ix + 4]) * d(t[ti + 4]) + d(x[ix + 5]) * d(t[ti + 5]))) as f32;
                x[ox1] = (d(x[ix + 6]) * d(t[ti + 7]) - d(x[ix + 7]) * d(t[ti + 6])) as f32;
                x[ox2 + 3] = (-(d(x[ix + 6]) * d(t[ti + 6]) + d(x[ix + 7]) * d(t[ti + 7]))) as f32;
                ox2 += 4;
                ix += 8;
                ti += 8;
                if ix >= ox1 {
                    break;
                }
            }
            let mut ix = n2 + n4;
            let mut ox1 = n4;
            let mut ox2 = ox1;
            loop {
                ox1 -= 4;
                ix -= 4;
                x[ox1 + 3] = x[ix + 3];
                x[ox2] = -x[ix + 3];
                x[ox1 + 2] = x[ix + 2];
                x[ox2 + 1] = -x[ix + 2];
                x[ox1 + 1] = x[ix + 1];
                x[ox2 + 2] = -x[ix + 1];
                x[ox1] = x[ix];
                x[ox2 + 3] = -x[ix];
                ox2 += 4;
                if ox2 >= ix {
                    break;
                }
            }
            let mut ix = n2 + n4;
            let mut ox1 = n2 + n4;
            let ox2 = n2;
            loop {
                ox1 -= 4;
                x[ox1] = x[ix + 3];
                x[ox1 + 1] = x[ix + 2];
                x[ox1 + 2] = x[ix + 1];
                x[ox1 + 3] = x[ix];
                ix += 4;
                if ox1 <= ox2 {
                    break;
                }
            }
        }
    }

    fn butterflies(&self, x: &mut [f32], points: usize) {
        let t = &self.trig;
        let mut stages = self.log2n as i32 - 5;
        stages -= 1;
        if stages > 0 {
            butterfly_first(t, x, points);
        }
        let mut i = 1;
        loop {
            stages -= 1;
            if stages <= 0 {
                break;
            }
            for j in 0..(1usize << i) {
                let p = points >> i;
                butterfly_generic(t, &mut x[p * j..], p, 4 << i);
            }
            i += 1;
        }
        let mut j = 0;
        while j < points {
            butterfly_32(&mut x[j..]);
            j += 32;
        }
    }

    fn bitreverse(&self, x: &mut [f32]) {
        let n = self.n;
        let bit = &self.bitrev;
        let t = &self.trig;
        let base = n >> 1; // x = w0 + n/2
        let mut w0 = 0usize;
        let mut w1 = base;
        let mut ti = n;
        let mut bi = 0;
        loop {
            let x0 = base + bit[bi];
            let x1 = base + bit[bi + 1];
            let r0 = x[x0 + 1] - x[x1 + 1];
            let r1 = x[x0] + x[x1];
            let r2 = (d(r1) * d(t[ti]) + d(r0) * d(t[ti + 1])) as f32;
            let r3 = (d(r1) * d(t[ti + 1]) - d(r0) * d(t[ti])) as f32;
            w1 -= 4;
            let r0 = ((d(x[x0 + 1]) + d(x[x1 + 1])) * 0.5) as f32;
            let r1 = ((d(x[x0]) - d(x[x1])) * 0.5) as f32;
            x[w0] = r0 + r2;
            x[w1 + 2] = r0 - r2;
            x[w0 + 1] = r1 + r3;
            x[w1 + 3] = r3 - r1;
            let x0 = base + bit[bi + 2];
            let x1 = base + bit[bi + 3];
            let r0 = x[x0 + 1] - x[x1 + 1];
            let r1 = x[x0] + x[x1];
            let r2 = (d(r1) * d(t[ti + 2]) + d(r0) * d(t[ti + 3])) as f32;
            let r3 = (d(r1) * d(t[ti + 3]) - d(r0) * d(t[ti + 2])) as f32;
            let r0 = ((d(x[x0 + 1]) + d(x[x1 + 1])) * 0.5) as f32;
            let r1 = ((d(x[x0]) - d(x[x1])) * 0.5) as f32;
            x[w0 + 2] = r0 + r2;
            x[w1] = r0 - r2;
            x[w0 + 3] = r1 + r3;
            x[w1 + 1] = r3 - r1;
            ti += 4;
            bi += 4;
            w0 += 4;
            if w0 >= w1 {
                break;
            }
        }
    }
}

fn butterfly_8(x: &mut [f32]) {
    let r0 = x[6] + x[2];
    let r1 = x[6] - x[2];
    let r2 = x[4] + x[0];
    let r3 = x[4] - x[0];
    x[6] = r0 + r2;
    x[4] = r0 - r2;
    let r0 = x[5] - x[1];
    let r2 = x[7] - x[3];
    x[0] = r1 + r0;
    x[2] = r1 - r0;
    let r0 = x[5] + x[1];
    let r1 = x[7] + x[3];
    x[3] = r2 + r3;
    x[1] = r2 - r3;
    x[7] = r1 + r0;
    x[5] = r1 - r0;
}

fn butterfly_16(x: &mut [f32]) {
    let r0 = x[1] - x[9];
    let r1 = x[0] - x[8];
    x[8] += x[0];
    x[9] += x[1];
    x[0] = ((d(r0) + d(r1)) * C_PI2_8) as f32;
    x[1] = ((d(r0) - d(r1)) * C_PI2_8) as f32;
    let r0 = x[3] - x[11];
    let r1 = x[10] - x[2];
    x[10] += x[2];
    x[11] += x[3];
    x[2] = r0;
    x[3] = r1;
    let r0 = x[12] - x[4];
    let r1 = x[13] - x[5];
    x[12] += x[4];
    x[13] += x[5];
    x[4] = ((d(r0) - d(r1)) * C_PI2_8) as f32;
    x[5] = ((d(r0) + d(r1)) * C_PI2_8) as f32;
    let r0 = x[14] - x[6];
    let r1 = x[15] - x[7];
    x[14] += x[6];
    x[15] += x[7];
    x[6] = r0;
    x[7] = r1;
    butterfly_8(x);
    butterfly_8(&mut x[8..]);
}

fn butterfly_32(x: &mut [f32]) {
    let r0 = x[30] - x[14];
    let r1 = x[31] - x[15];
    x[30] += x[14];
    x[31] += x[15];
    x[14] = r0;
    x[15] = r1;
    let r0 = x[28] - x[12];
    let r1 = x[29] - x[13];
    x[28] += x[12];
    x[29] += x[13];
    x[12] = (d(r0) * C_PI1_8 - d(r1) * C_PI3_8) as f32;
    x[13] = (d(r0) * C_PI3_8 + d(r1) * C_PI1_8) as f32;
    let r0 = x[26] - x[10];
    let r1 = x[27] - x[11];
    x[26] += x[10];
    x[27] += x[11];
    x[10] = ((d(r0) - d(r1)) * C_PI2_8) as f32;
    x[11] = ((d(r0) + d(r1)) * C_PI2_8) as f32;
    let r0 = x[24] - x[8];
    let r1 = x[25] - x[9];
    x[24] += x[8];
    x[25] += x[9];
    x[8] = (d(r0) * C_PI3_8 - d(r1) * C_PI1_8) as f32;
    x[9] = (d(r1) * C_PI3_8 + d(r0) * C_PI1_8) as f32;
    let r0 = x[22] - x[6];
    let r1 = x[7] - x[23];
    x[22] += x[6];
    x[23] += x[7];
    x[6] = r1;
    x[7] = r0;
    let r0 = x[4] - x[20];
    let r1 = x[5] - x[21];
    x[20] += x[4];
    x[21] += x[5];
    x[4] = (d(r1) * C_PI1_8 + d(r0) * C_PI3_8) as f32;
    x[5] = (d(r1) * C_PI3_8 - d(r0) * C_PI1_8) as f32;
    let r0 = x[2] - x[18];
    let r1 = x[3] - x[19];
    x[18] += x[2];
    x[19] += x[3];
    x[2] = ((d(r1) + d(r0)) * C_PI2_8) as f32;
    x[3] = ((d(r1) - d(r0)) * C_PI2_8) as f32;
    let r0 = x[0] - x[16];
    let r1 = x[1] - x[17];
    x[16] += x[0];
    x[17] += x[1];
    x[0] = (d(r1) * C_PI3_8 + d(r0) * C_PI1_8) as f32;
    x[1] = (d(r1) * C_PI1_8 - d(r0) * C_PI3_8) as f32;
    butterfly_16(x);
    butterfly_16(&mut x[16..]);
}

/// One butterfly step on `x1[k]` / `x2[k]` with trig pair `(c0, c1)`.
#[inline]
fn step(x: &mut [f32], a: usize, b: usize, c0: f32, c1: f32) {
    let r0 = x[a] - x[b];
    let r1 = x[a + 1] - x[b + 1];
    x[a] += x[b];
    x[a + 1] += x[b + 1];
    x[b] = (d(r1) * d(c1) + d(r0) * d(c0)) as f32;
    x[b + 1] = (d(r1) * d(c0) - d(r0) * d(c1)) as f32;
}

fn butterfly_first(t: &[f32], x: &mut [f32], points: usize) {
    let mut x1 = points as isize - 8;
    let mut x2 = (points >> 1) as isize - 8;
    let mut ti = 0;
    loop {
        let (a, b) = (x1 as usize, x2 as usize);
        step(x, a + 6, b + 6, t[ti], t[ti + 1]);
        step(x, a + 4, b + 4, t[ti + 4], t[ti + 5]);
        step(x, a + 2, b + 2, t[ti + 8], t[ti + 9]);
        step(x, a, b, t[ti + 12], t[ti + 13]);
        x1 -= 8;
        x2 -= 8;
        ti += 16;
        if x2 < 0 {
            break;
        }
    }
}

fn butterfly_generic(t: &[f32], x: &mut [f32], points: usize, trigint: usize) {
    let mut x1 = points as isize - 8;
    let mut x2 = (points >> 1) as isize - 8;
    let mut ti = 0;
    loop {
        let (a, b) = (x1 as usize, x2 as usize);
        step(x, a + 6, b + 6, t[ti], t[ti + 1]);
        ti += trigint;
        step(x, a + 4, b + 4, t[ti], t[ti + 1]);
        ti += trigint;
        step(x, a + 2, b + 2, t[ti], t[ti + 1]);
        ti += trigint;
        step(x, a, b, t[ti], t[ti + 1]);
        ti += trigint;
        x1 -= 8;
        x2 -= 8;
        if x2 < 0 {
            break;
        }
    }
}

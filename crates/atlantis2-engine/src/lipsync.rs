//! The dialog lip-sync: the mouth computed from the voice itself.
//!
//! Nothing is stored per line. When a line starts, its voice (the whole
//! APC decoded to 16-bit PCM) is analysed once by blocks of 256 samples
//! (@ 0x0059f720): the sum of absolute values and a histogram of the
//! zero-crossing intervals. Every rendered frame, the evaluator
//! (@ 0x0059fa40) turns the analysis around the time t (the time since the
//! line started plus the speaker's `[Offset]`) into the weights of the 7
//! visemes `d i a o b s v` through the speaker's 10 x 7 `[Matrice]`,
//! smoothed by `[Smooth]`. [`crate::head::Head::mouth`] mixes the morph
//! targets with them.
//!
//! The operation order, the float stores and the x87 roundings are the
//! original's, at either precision ([`Fpu`]); the original's quirks are
//! kept: the mouth closes for the last `[Offset]` seconds of a line, a
//! histogram bin holding all of a block's crossings reads 0, and a click
//! after digital silence can make the weights NaN for the rest of the line
//! (the mouth then stays on viseme `a`).

use crate::fpu::{f, Fpu};

/// Run-length thresholds of the histogram bins (@ 0x0059f8b0).
const THRESH: [f64; 8] = [2.0, 4.0, 8.0, 14.0, 50.0, 80.0, 100002.0, 100003.0];

/// 1 / 22050 as a float (@ 0x005c807c): the voices' rate is assumed.
const SAMPLE_TIME: f32 = 4.535_147_3e-5;

/// 2/3 as a double (@ 0x005c8098).
const TWO_THIRDS: f64 = 0.666_666_666_666_666_6;

/// The analysis of one voice and the evaluator's state (a 0x40-byte
/// object built by @ 0x0059f720).
#[derive(Clone, Debug)]
pub struct LipSync {
    fpu: Fpu,
    matrix: [f64; 70],
    n: u32,
    blocks: usize,
    energy: Vec<i32>,
    hist: Vec<[u8; 8]>,
    emax: i32,
    duration: f64,
    smooth: f64,
    prev_t: f64,
    raw: [f64; 4],
    prev: [f64; 7],
}

fn cdiv64(x: i64) -> i64 {
    // C division, toward zero.
    x / 64
}

impl LipSync {
    /// Analyses a voice (@ 0x0059f720): `pcm` the samples, `n` the count
    /// the original passes (the PCM bytes / 2), `matrix` the speaker's
    /// `[Matrice]` (row-major, 10 x 7) and `smooth` its `[Smooth]`.
    pub fn new(fpu: Fpu, matrix: &[f32; 70], pcm: &[i16], n: u32, smooth: f32) -> LipSync {
        let nb = (n >> 8) as usize;
        let total = nb + 16;
        let mut energy = vec![0i32; total];
        let mut hist = vec![[0u8; 8]; total];
        let mut pos = false;
        let mut run: i64 = 0;
        let mut emax: i32 = 1;
        let mut p = 0usize;
        for b in 0..nb {
            let mut h = [0u8; 8];
            let mut crossings: u32 = 0;
            let mut e: i32 = 0;
            for _ in 0..256 {
                let s = i32::from(pcm.get(p).copied().unwrap_or(0));
                p += 1;
                let mut flip = false;
                if s < 0 {
                    e = e.wrapping_sub(s);
                    if pos {
                        pos = false;
                        flip = true;
                    }
                } else {
                    e = e.wrapping_add(s);
                    if !pos {
                        pos = true;
                        flip = true;
                    }
                }
                if flip {
                    let r = run as f64;
                    if let Some(i) = THRESH.iter().position(|&t| r <= t) {
                        h[i] = h[i].wrapping_add(1);
                    }
                    if r > 100003.0 {
                        h[7] = h[7].wrapping_add(1);
                    }
                    crossings += 1;
                    run = 0;
                }
                run += 1;
            }
            // Each bin as a share of the block's crossings, in 256ths,
            // truncated to a byte (all of them: 256 → 0).
            for x in &mut h {
                if let Some(v) = (u32::from(*x) << 8).checked_div(crossings) {
                    *x = v as u8;
                }
            }
            if emax < e {
                emax = e;
            }
            energy[8 + b] = e;
            hist[8 + b] = h;
        }
        LipSync {
            fpu,
            matrix: matrix.map(f64::from),
            n,
            blocks: nb,
            energy,
            hist,
            emax,
            duration: f(f64::from(n) * f64::from(SAMPLE_TIME)),
            smooth: f64::from(smooth),
            prev_t: 100.0,
            raw: [0.0; 4],
            prev: [0.0; 7],
        }
    }

    /// The number of whole 256-sample blocks.
    pub fn blocks(&self) -> usize {
        self.blocks
    }

    /// The largest block energy (at least 1).
    pub fn max_energy(&self) -> i32 {
        self.emax
    }

    /// The energies (block b at index 8 + b, 8 zero entries each side).
    pub fn energies(&self) -> &[i32] {
        &self.energy
    }

    /// The histograms, laid out as the energies.
    pub fn histograms(&self) -> &[[u8; 8]] {
        &self.hist
    }

    fn row(&self, r: usize) -> [f64; 7] {
        std::array::from_fn(|c| self.matrix[r * 7 + c])
    }

    /// `pow` as `_CIpow` (@ 0x005aeb00) computes it at the precision.
    fn pow(&self, x: f64, y: f64) -> f64 {
        match self.fpu {
            Fpu::Double => libm::pow(x, y),
            Fpu::Single => {
                // FYL2X (not rounded), FRNDINT, the fraction rounded,
                // F2XM1 + 1 rounded, FSCALE.
                let z = y * libm::log2(x);
                let i = libm::rint(z);
                let fr = -f(i - z);
                f(libm::exp2(fr)) * libm::exp2(i)
            }
        }
    }

    /// The 7 weights (`d i a o b s v`) at time `t` (@ 0x0059fa40).
    pub fn eval(&mut self, t: f32) -> [f32; 7] {
        let fpu = self.fpu;
        let r = move |x: f64| fpu.r(x);
        let t = f64::from(t);
        let x = r(r(t - self.prev_t) * 15.0);
        let k001 = f64::from(0.001_f32);
        let dt = if x < k001 { k001 } else { f(x) };
        let rr = r(t / self.duration);
        let rf = f(rr);
        if rr < 0.0 || rf > 1.0 {
            self.prev_t = t;
            return self.row(7).map(|v| v as f32);
        }
        let mut i = 8 - crate::fpu::ftol(r(r(f64::from(self.n) * rf) * -0.00390625)) as i64;
        i = i.max(8);
        i = i.min(self.blocks as i64);
        // Below block 6 the original reads before its arrays; 0 here.
        let e = |k: i64| {
            usize::try_from(k)
                .ok()
                .and_then(|k| self.energy.get(k))
                .map_or(0, |&v| i64::from(v))
        };
        let e1 = e(i + 3)
            + 15 * (e(i - 1) + e(i + 1))
            + e(i - 3)
            + 6 * (e(i - 2) + e(i + 2))
            + 20 * e(i);
        let e2 = e(i)
            + 15 * (e(i + 2) + e(i + 4))
            + e(i + 6)
            + 6 * (e(i + 5) + e(i + 1))
            + 20 * e(i + 3);
        let mx = f64::from(self.emax);
        let pow8 = |v: i64| {
            let a = r(1.0 - r(cdiv64(v) as i32 as f64 / mx));
            let a = r(a * a);
            let a = r(a * a);
            f(r(a * a))
        };
        let (a8, b8) = (pow8(e1), pow8(e2));
        let k4096 = 0.000244140625;
        let smooth_hist = |c: i64| -> [f64; 8] {
            let h = |k: i64, b: usize| {
                usize::try_from(k)
                    .ok()
                    .and_then(|k| self.hist.get(k))
                    .map_or(0, |x| i64::from(x[b]))
            };
            std::array::from_fn(|b| {
                let s = 924 * h(c, b)
                    + 792 * (h(c - 1, b) + h(c + 1, b))
                    + 495 * (h(c - 2, b) + h(c + 2, b))
                    + 220 * (h(c - 3, b) + h(c + 3, b))
                    + 66 * (h(c - 4, b) + h(c + 4, b))
                    + 12 * (h(c - 5, b) + h(c + 5, b))
                    + (h(c - 6, b) + h(c + 6, b));
                let v = f(r(s as f64 * k4096));
                if v > 255.0 {
                    255.0
                } else {
                    v
                }
            })
        };
        let mut g = smooth_hist(i);
        let a1 = self.pow(TWO_THIRDS, dt);
        let oma1 = r(1.0 - a1);
        let raw = [g[1], g[2], g[3], g[4]];
        for k in 0..4 {
            g[1 + k] = f(r(r(a1 * g[1 + k]) + r(oma1 * self.raw[k])));
        }
        self.raw = raw;
        let gp = smooth_hist(i + 3);
        let k256 = 0.00390625;
        let (k07, k05, k12, k16, k02) = (
            f64::from(0.7_f32),
            0.5,
            f64::from(1.2_f32),
            f64::from(1.6_f32),
            f64::from(0.2_f32),
        );
        let p0 = f(r(r(g[0] * k256) * k07));
        let p1 = f(r(r(r(r(r(g[4] + g[3]) + g[2]) + g[1]) * k256) * k05));
        let p2 = f(r(r(g[5] * k256) * k12));
        let p3 = f(r(r(r(g[7] + g[6]) * k256) * k16));
        let q0 = f(r(r(gp[0] * k256) * k07));
        let q1 = f(r(r(r(r(r(gp[4] + gp[3]) + gp[2]) + gp[1]) * k256) * k05));
        let q2 = r(r(gp[5] * k256) * k12); // kept on the x87 stack
        let q3 = f(r(r(r(gp[7] + gp[6]) * k256) * k16));
        let ea = (e(i + 2) + e(i + 1) + e(i)) as f64;
        let eb = (e(i + 4) + e(i + 5) + e(i + 6)) as f64;
        // 0 / 0 is the x87 default NaN.
        let rho = r(ea / r(eb + ea));
        let kp = f(r(r(r(1.0 - r(rho * rho)) * r(1.0 - b8)) * a8));
        let (mut best, mut row) = (g[1], 1);
        for k in 0..3 {
            if g[2 + k] > best {
                best = g[2 + k];
                row = 2 + k;
            }
        }
        let mut out = self.row(row);
        let blend = |w: f64, m: [f64; 7], base: [f64; 7]| -> [f64; 7] {
            let om = r(1.0 - w);
            std::array::from_fn(|c| f(r(r(w * m[c]) + r(om * base[c]))))
        };
        if p0 > p1 && p0 > p2 && p0 > p3 {
            let w = f(r(p0 * f64::from(f32::from_bits(0x3fb6_db6e)))); // 1 / 0.7
            out = blend(w, self.row(0), out);
        } else if (p2 > p1 && p2 > p3) || (p3 > p1 && p3 > p2) {
            let w = f(r(p2 / r(p3 + p2)));
            out = blend(w, self.row(5), self.row(6));
        }
        out = blend(a8, self.row(9), out);
        if r(q0 * k05) < q1 && q1 > q2 && q1 > q3 && r(q1 * k02) < q0 {
            out = blend(kp, self.row(8), out);
        }
        let base = r(self.smooth / r(self.smooth + 1.0));
        let a2 = f(self.pow(base, dt));
        let om2 = f(r(1.0 - a2));
        let out: [f64; 7] = std::array::from_fn(|c| f(r(r(a2 * self.prev[c]) + r(om2 * out[c]))));
        self.prev = out;
        self.prev_t = t;
        out.map(|v| v as f32)
    }
}

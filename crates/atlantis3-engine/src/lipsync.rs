//! The dialog lip-sync of *Atlantis III*: the mouth computed from the
//! line's voice. The analyser (@ 0x006b33d0) is *Atlantis II*'s (per 256
//! samples: the sum of absolute values and an 8-bin histogram of the
//! zero-crossing run lengths) with bin limits scaled by the speaker's
//! `[Stretch]`; the evaluator (@ 0x006b3770), run at every rendered frame,
//! turns it into nine weights: the mouth keys `-s -i -a -e -o -w -n -p`
//! and the mood key (slot 8), mixed into the head by `PlayMorphingKey`
//! (@ 0x0047d930) with [`lerp_factors`].
//!
//! The operation order, the float stores and the x87 roundings are the
//! original's (dialogs run under 0x0e7f, [`Fpu::Chop`]); its quirks are
//! kept: with the shipped speakers' weight 0 in slot 0 the `-s` key never
//! shows, slot 6 is not low-passed.

use atlantis2_engine::Fpu;

const C15: f32 = f32::from_bits(0x4170_0000);
const C0001: f32 = f32::from_bits(0x3a83_126f);
const K256: f32 = f32::from_bits(0x3b80_0000);
const K4096: f32 = f32::from_bits(0x3980_0000);
const KM256: f32 = f32::from_bits(0xbb80_0000);
const K07: f32 = f32::from_bits(0x3f33_3333);
const K0125: f32 = f32::from_bits(0x3e00_0000);
const K12: f32 = f32::from_bits(0x3f99_999a);
const K16: f32 = f32::from_bits(0x3fcc_cccd);
const E4: f32 = f32::from_bits(0x38d1_b717);
const E5: f32 = f32::from_bits(0x3727_c5ac);
/// The double 2/3 @ 0x007009d0.
const TWO_THIRDS: f64 = 0.666_666_666_666_666_6;
/// 1/22050 @ 0x007009ac.
const SAMPLE_TIME: f32 = f32::from_bits(0x383e_37c6);
const K08: f32 = f32::from_bits(0x3f4c_cccd);
const K01: f32 = f32::from_bits(0x3dcc_cccd);

fn d(x: f32) -> f64 {
    f64::from(x)
}

/// `GlobalWeight × 0.8` as the line start passes it (@ 0x0046702f).
pub fn gw08(fpu: Fpu, g: f32) -> f32 {
    fpu.f32(fpu.mul(d(g), d(K08)))
}

/// The frame's time (@ 0x004698ad): f32(((now − start) + 0.0) + 0.1f).
pub fn t_of(fpu: Fpu, elapsed: f64) -> f32 {
    fpu.f32(fpu.add(fpu.add(elapsed, 0.0), d(K01)))
}

/// The analyser of one voice.
#[derive(Clone, Debug)]
pub struct LipSync {
    fpu: Fpu,
    w: [f32; 9],
    smooth: f32,
    gw: f32,
    prevt: f32,
    raw: [f32; 4],
    emax: i64,
    ok: bool,
    n: u32,
    big_n: usize,
    dur: f32,
    prev: [f32; 9],
    e: Vec<i64>,
    h: Vec<[u8; 8]>,
}

impl LipSync {
    /// The analyser (@ 0x006b33d0) of `pcm` (n samples), the speaker's
    /// smoothing, stretch and weights, and `gw` = [`gw08`].
    pub fn new(fpu: Fpu, pcm: &[i16], smooth: f32, stretch: f32, w: [f32; 9], gw: f32) -> LipSync {
        let n = pcm.len() as u32;
        let mut l = LipSync {
            fpu,
            w,
            smooth,
            gw,
            prevt: 100.0,
            raw: [0.0; 4],
            emax: 1,
            ok: false,
            n,
            big_n: 0,
            dur: 0.0,
            prev: [0.0; 9],
            e: Vec::new(),
            h: Vec::new(),
        };
        if gw < 0.0 || n == 0 {
            return l;
        }
        let s = d(stretch.clamp(0.5, 2.0));
        let f = fpu;
        let mut t = [0f32; 8];
        t[0] = 2.0;
        t[1] = f.f32(f.add(f.add(s, 1.0), f.add(s, 1.0)));
        for (i, k) in [6.0, 12.0, 48.0, 78.0, 100_000.0, 100_001.0]
            .into_iter()
            .enumerate()
        {
            t[2 + i] = f.f32(f.add(f.mul(s, k), 2.0));
        }
        let nb = (n >> 8) as usize;
        let big_n = nb + 16;
        l.big_n = big_n;
        l.dur = f.f32(f.mul(f64::from(n), d(SAMPLE_TIME)));
        l.e = vec![0; big_n];
        l.h = vec![[0; 8]; big_n];
        let (mut pos, mut run, mut p) = (false, 0i64, 0usize);
        for b in 0..nb {
            let mut h = [0u8; 8];
            let (mut cross, mut energy) = (0i64, 0i64);
            for _ in 0..256 {
                let v = i64::from(pcm[p]);
                p += 1;
                let mut flip = false;
                if v >= 0 {
                    energy += v;
                    if !pos {
                        pos = true;
                        flip = true;
                    }
                } else {
                    energy -= v;
                    if pos {
                        pos = false;
                        flip = true;
                    }
                }
                if flip {
                    cross += 1;
                    for k in 0..8 {
                        if run as f64 <= d(t[k]) {
                            h[k] = h[k].wrapping_add(1);
                            break;
                        }
                    }
                    if run as f64 > d(t[7]) {
                        h[7] = h[7].wrapping_add(1);
                    }
                    run = 0;
                }
                run += 1;
            }
            if cross != 0 {
                h = h.map(|x| (((i64::from(x)) << 8) / cross) as u8);
            }
            if l.emax < energy {
                l.emax = energy;
            }
            l.e[8 + b] = energy;
            l.h[8 + b] = h;
        }
        l.ok = true;
        l
    }

    /// The voice's duration (f32(n × 1/22050)).
    pub fn duration(&self) -> f32 {
        self.dur
    }

    fn g(&self, c: usize) -> [f32; 8] {
        let f = self.fpu;
        let h = |i: usize, k: usize| i64::from(self.h[i][k]);
        let mut out = [0f32; 8];
        for (k, o) in out.iter_mut().enumerate() {
            let s = 924 * h(c, k)
                + 792 * (h(c - 1, k) + h(c + 1, k))
                + 495 * (h(c - 2, k) + h(c + 2, k))
                + 220 * (h(c - 3, k) + h(c + 3, k))
                + 66 * (h(c - 4, k) + h(c + 4, k))
                + 12 * (h(c - 5, k) + h(c + 5, k))
                + (h(c - 6, k) + h(c + 6, k));
            let v = f.mul(s as f64, d(K4096));
            *o = if v > 255.0 { 255.0 } else { f.f32(v) };
        }
        out
    }

    /// The evaluator (@ 0x006b3770) at time `t`: the nine weights, `None`
    /// for an empty analyser (the original leaves its buffer untouched).
    pub fn eval(&mut self, t: f32) -> Option<[f32; 9]> {
        if !self.ok {
            return None;
        }
        let f = self.fpu;
        let w = self.w.map(d);
        let x = f.mul(f.sub(d(t), d(self.prevt)), d(C15));
        let dt = if x < d(C0001) { d(C0001) } else { d(f.f32(x)) };
        let r = f.f32(f.div(d(t), d(self.dur)));
        let mut out = [0f32; 9];
        if !(0.0..=1.0).contains(&r) {
            out[8] = 1.0;
            self.prevt = t;
            return Some(out);
        }
        let q = f.mul(f.mul(f64::from(self.n), d(r)), d(KM256));
        let mut i = 8i64 - q.trunc() as i64;
        if !(8..=i64::from(u32::MAX)).contains(&i) {
            // An unsigned compare: below 8 is 8, a wrapped value is large.
            i = if i < 0 { i64::MAX } else { 8 };
        }
        let lim = self.big_n as i64 - 16;
        if i > lim {
            i = lim;
        }
        let i = i as usize;
        let e = |k: usize| self.e[k];
        let mut g = self.g(i);
        let a1 = libm::pow(TWO_THIRDS, dt);
        let oma = f.sub(1.0, a1);
        let raw = [g[1], g[2], g[3], g[4]];
        for k in 0..4 {
            g[1 + k] = f.f32(f.add(f.mul(oma, d(self.raw[k])), f.mul(a1, d(g[1 + k]))));
        }
        self.raw = raw;
        let gp = self.g(i + 4);
        let gd = g.map(d);
        let gpd = gp.map(d);
        let w0p = d(f.f32(f.mul(w[0], d(K07))));
        let wv = f.mul(f.add(f.add(f.add(w[4], w[3]), w[2]), w[1]), d(K0125));
        let w5p = d(f.f32(f.mul(w[5], d(K12))));
        let w6p = d(f.f32(f.mul(w[6], d(K16))));
        let p0 = d(f.f32(f.mul(f.mul(gd[0], d(K256)), w0p)));
        let p1 = d(f.f32(f.mul(
            f.mul(f.add(f.add(f.add(gd[4], gd[3]), gd[2]), gd[1]), d(K256)),
            wv,
        )));
        let p2 = d(f.f32(f.mul(f.mul(gd[5], d(K256)), w5p)));
        let p3 = d(f.f32(f.mul(f.mul(f.add(gd[7], gd[6]), d(K256)), w6p)));
        let q0 = d(f.f32(f.mul(f.mul(gpd[0], d(K256)), w0p)));
        let q1 = f.mul(
            f.mul(f.add(f.add(f.add(gpd[4], gpd[3]), gpd[2]), gpd[1]), d(K256)),
            wv,
        );
        let q2 = f.mul(f.mul(gpd[5], d(K256)), w5p);
        let q3 = f.mul(f.mul(f.add(gpd[7], gpd[6]), d(K256)), w6p);
        let e1 = e(i - 3)
            + 6 * e(i - 2)
            + 15 * e(i - 1)
            + 20 * e(i)
            + 15 * e(i + 1)
            + 6 * e(i + 2)
            + e(i + 3);
        // `idiv` by 64: toward zero.
        let e64 = e1 / 64;
        let mut xx = f.sub(1.0, f.div(e64 as f64, self.emax as f64));
        xx = f.mul(xx, xx);
        xx = f.mul(xx, xx);
        let a8 = d(f.f32(f.mul(xx, xx)));
        let sb = (e(i + 3) + e(i + 2) + e(i + 1)) as f64;
        let sa = d(f.f32((e(i + 5) + e(i + 6) + e(i + 7)) as f64));
        let em3 = d(f.f32(f.mul(self.emax as f64, 3.0)));
        let rb = f.div(sb, em3);
        let ra = f.div(sa, em3);
        let (mut best, mut idx) = (d(f.f32(f.mul(gd[1], w[1]))), 1usize);
        for c in 2..=4 {
            let v = f.mul(w[c], gd[c]);
            if v > best {
                best = d(f.f32(v));
                idx = c;
            }
        }
        out[idx] = 1.0;
        if p0 > p1 && p0 > p2 && p0 > p3 {
            let h = f.div(p0, f.add(w0p, d(E4)));
            let om = f.sub(1.0, h);
            for o in &mut out[1..=4] {
                *o = f.f32(f.mul(om, d(*o)));
            }
            out[0] = f.f32(h);
        } else if p2 > p1 || p3 > p1 {
            let xr = f.div(p2, f.add(f.add(p3, p2), d(E5)));
            out[..5].fill(0.0);
            out[5] = f.f32(xr);
            out[6] = f.f32(f.sub(1.0, xr));
        }
        let m = f.sub(1.0, a8);
        for o in out.iter_mut().take(8) {
            *o = f.f32(f.mul(f.mul(m, d(*o)), w[8]));
        }
        out[8] = f.f32(f.mul(a8, w[8]));
        let mut z = f.div(f.add(ra, ra), f.add(f.mul(rb, 20.0), 1.0));
        if z > 1.0 {
            z = 1.0;
        }
        let omz = f.sub(1.0, z);
        let dsum = f.add(f.add(f.add(f.add(q3, q2), q1), q0), d(E5));
        let k = f.mul(f.div(q1, dsum), f.sub(1.0, f.mul(omz, omz)));
        let p = f.mul(f.mul(f.mul(k, w[7]), k), 7.0);
        let omp = f.sub(1.0, p);
        for o in out.iter_mut() {
            let v = f.mul(omp, d(*o));
            *o = if v < 0.0 { 0.0 } else { f.f32(v) };
        }
        out[7] = if p < 0.0 { 0.0 } else { f.f32(p) };
        let mut s = d(E5);
        for o in &out {
            s = f.add(s, d(*o));
        }
        if s == 0.0 {
            s = d(C0001);
        }
        for o in out.iter_mut() {
            let v = f.div(d(*o), s);
            *o = if v < 0.0 { 0.0 } else { f.f32(v) };
        }
        let sm = d(self.smooth);
        let b1 = d(f.f32(libm::pow(f.div(sm, f.add(sm, 1.0)), dt)));
        let om1 = d(f.f32(f.sub(1.0, b1)));
        let b2 = libm::pow(f.div(sm, f.add(sm, 2.0)), dt);
        let om2 = f.sub(1.0, b2);
        let pv = &mut self.prev;
        for c in 0..6 {
            pv[c] = f.f32(f.add(f.mul(b1, d(pv[c])), f.mul(om1, d(out[c]))));
            out[c] = pv[c];
        }
        pv[7] = f.f32(f.add(f.mul(b2, d(pv[7])), f.mul(om2, d(out[7]))));
        out[7] = pv[7];
        pv[8] = f.f32(f.add(f.mul(b1, d(pv[8])), f.mul(om1, d(out[8]))));
        out[8] = pv[8];
        for o in out.iter_mut().take(8) {
            *o = f.f32(f.mul(d(*o), d(self.gw)));
        }
        self.prevt = t;
        Some(out)
    }
}

/// `PlayMorphingKey`'s factors (@ 0x0047db6a): key c (0..7) lerped by
/// w[c] / (1 − (w[8] + … + w[c+1])), the mood key by w[8].
pub fn lerp_factors(fpu: Fpu, w: &[f32; 9]) -> [f32; 9] {
    let mut out = [0f32; 9];
    for c in 0..8 {
        let mut s = 0.0;
        for j in (c + 1..=8).rev() {
            s = fpu.add(s, d(w[j]));
        }
        out[c] = fpu.f32(fpu.div(d(w[c]), fpu.sub(1.0, s)));
    }
    out[8] = w[8];
    out
}

//! Floor type 0: the LSP envelope (Vorbis I specification § 6; libvorbis
//! 1.2.0's `floor0.c`, and `lsp.c` with its default `FLOAT_LOOKUP`
//! interpolated tables from `lookup.c`). No sound of *Atlantis Evolution*
//! uses floor 0, so this path is ported with the same arithmetic rules as
//! the rest but is not checked against the game's output.

use super::bits::Bits;
use super::book::{ilog, Book};
use super::Error;

pub(crate) struct Floor0 {
    order: usize,
    rate: i64,
    barkmap: i64,
    ampbits: u32,
    amp_db: i64,
    books: Vec<usize>,
}

/// `vorbis_ftoi` of the MSVC build: `fistp`, round half to even.
fn ftoi(x: f64) -> i64 {
    x.round_ties_even() as i64
}

const fn f(x: f32) -> f64 {
    x as f64
}

/// `toBARK` of `scales.h`, its float literals rounded to float.
fn to_bark(n: f64) -> f64 {
    f(13.1) * libm::atan(f(0.00074) * n) + f(2.24) * libm::atan(n * n * f(1.85e-8)) + f(1e-4) * n
}

fn coslook(a: f32) -> f32 {
    let d = f64::from(a) * (0.318_309_89 * 128.0);
    let i = ftoi(d - 0.5).clamp(0, 127) as usize;
    (f64::from(COS_LOOKUP[i])
        + (d - i as f64) * (f64::from(COS_LOOKUP[i + 1]) - f64::from(COS_LOOKUP[i]))) as f32
}

fn invsqlook(a: f32) -> f32 {
    let d = f64::from(a) * 64.0 - 32.0;
    let i = ftoi(d - 0.5).clamp(0, 31) as usize;
    (f64::from(INVSQ_LOOKUP[i])
        + (d - i as f64) * (f64::from(INVSQ_LOOKUP[i + 1]) - f64::from(INVSQ_LOOKUP[i]))) as f32
}

fn invsq2explook(a: i32) -> f32 {
    INVSQ2EXP_LOOKUP[(a + 32).clamp(0, 64) as usize]
}

fn fromdblook(a: f32) -> f32 {
    let i = ftoi(f64::from(a) * -8.0 - 0.5);
    if i < 0 {
        1.0
    } else if i >= 35 << 5 {
        0.0
    } else {
        FROMDB_LOOKUP[(i >> 5) as usize] * FROMDB2_LOOKUP[(i & 31) as usize]
    }
}

/// `frexp` of a finite double: mantissa in [0.5, 1) and exponent.
fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || !x.is_finite() {
        return (x, 0);
    }
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32;
    if exp == 0 {
        let (m, e) = frexp(x * 2f64.powi(64));
        return (m, e - 64);
    }
    let m = f64::from_bits((bits & !(0x7ff << 52)) | (1022 << 52));
    (m, exp - 1022)
}

impl Floor0 {
    pub(crate) fn unpack(b: &mut Bits, books: usize) -> Result<Floor0, Error> {
        let bad = || Error("Vorbis: bad floor 0".into());
        let order = b.read(8);
        let rate = b.read(16);
        let barkmap = b.read(16);
        let ampbits = b.read(6);
        let amp_db = b.read(8);
        let numbooks = b.read(4) + 1;
        if order < 1 || rate < 1 || barkmap < 1 || numbooks < 1 || ampbits < 0 {
            return Err(bad());
        }
        let mut list = Vec::new();
        for _ in 0..numbooks {
            let k = b.read(8);
            if k < 0 || k as usize >= books {
                return Err(bad());
            }
            list.push(k as usize);
        }
        Ok(Floor0 {
            order: order as usize,
            rate,
            barkmap,
            ampbits: ampbits as u32,
            amp_db,
            books: list,
        })
    }

    /// `floor0_inverse1`: the LSP coefficients and the amplitude last.
    pub(crate) fn inverse1(&self, b: &mut Bits, books: &[Book]) -> Option<Vec<f32>> {
        let ampraw = b.read(self.ampbits);
        if ampraw <= 0 {
            return None;
        }
        let maxval = (1i64 << self.ampbits) - 1;
        let amp = (f64::from(ampraw as f32) / maxval as f64 * self.amp_db as f64) as f32;
        let booknum = b.read(ilog(self.books.len() as u32));
        if booknum == -1 || booknum as usize >= self.books.len() {
            return None;
        }
        let book = &books[self.books[booknum as usize]];
        let m = self.order;
        let mut lsp = vec![0f32; m + book.dim + 1];
        let mut j = 0;
        while j < m {
            if !book.decodev_set(&mut lsp[j..], b, book.dim) {
                return None;
            }
            j += book.dim.max(1);
        }
        let mut last = 0f32;
        let mut j = 0;
        while j < m {
            for _ in 0..book.dim {
                lsp[j] += last;
                j += 1;
            }
            last = lsp[j - 1];
        }
        lsp[m] = amp;
        lsp.truncate(m + 1);
        Some(lsp)
    }

    /// `floor0_inverse2` (with `floor0_map_lazy_init` and
    /// `vorbis_lsp_to_curve`) on the `n` first bins of a block of type `w`.
    pub(crate) fn inverse2(
        &self,
        memo: Option<&[f32]>,
        out: &mut [f32],
        n: usize,
        _w: usize,
        _rate: u32,
    ) {
        let Some(lsp) = memo else {
            out[..n].fill(0.0);
            return;
        };
        let ln = self.barkmap;
        let half = f64::from(self.rate as f32 / 2.0);
        let scale = (ln as f64 / to_bark(half)) as f32;
        let mut map: Vec<i64> = (0..n)
            .map(|j| {
                let v = (to_bark(half / n as f64 * j as f64) * f64::from(scale)).floor() as i64;
                v.min(ln - 1)
            })
            .collect();
        map.push(-1);
        let m = self.order;
        let amp = lsp[m];
        let ampoffset = self.amp_db as f32;
        let mut lsp: Vec<f32> = lsp[..m].iter().map(|&x| coslook(x)).collect();
        lsp.push(0.0);
        let wdel = (f64::from(std::f32::consts::PI) / ln as f64) as f32;
        let mut i = 0;
        while i < n {
            let k = map[i];
            let p0 = 0.707_106_781_2_f32;
            let (mut p, mut q) = (p0, p0);
            let w = coslook((f64::from(wdel) * k as f64) as f32);
            let wd = f64::from(w);
            let mut t = 0;
            for _ in 0..m >> 1 {
                q = (f64::from(q) * (f64::from(lsp[t]) - wd)) as f32;
                p = (f64::from(p) * (f64::from(lsp[t + 1]) - wd)) as f32;
                t += 2;
            }
            if m & 1 != 0 {
                q = (f64::from(q) * (f64::from(lsp[t]) - wd)) as f32;
                q = q * q;
                p = (f64::from(p) * (f64::from(p) * (1.0 - wd * wd))) as f32;
            } else {
                q = (f64::from(q) * (f64::from(q) * (1.0 + wd))) as f32;
                p = (f64::from(p) * (f64::from(p) * (1.0 - wd))) as f32;
            }
            let (mant, qexp) = frexp(f64::from(p) + f64::from(q));
            let q = mant as f32;
            let arg = (f64::from(amp)
                * f64::from(invsqlook(q))
                * f64::from(invsq2explook(qexp + m as i32))
                - f64::from(ampoffset)) as f32;
            let q = fromdblook(arg);
            loop {
                out[i] *= q;
                i += 1;
                if map[i] != k {
                    break;
                }
            }
        }
    }
}

#[rustfmt::skip]
static COS_LOOKUP: [f32; 129] = [
    1.0000000000000, 0.9996988186962, 0.9987954562052, 0.9972904566787,
    0.9951847266722, 0.9924795345987, 0.9891765099648, 0.9852776423889,
    0.9807852804032, 0.9757021300385, 0.9700312531945, 0.9637760657954,
    0.9569403357322, 0.9495281805930, 0.9415440651830, 0.9329927988347,
    0.9238795325113, 0.9142097557035, 0.9039892931234, 0.8932243011955,
    0.8819212643484, 0.8700869911087, 0.8577286100003, 0.8448535652497,
    0.8314696123025, 0.8175848131516, 0.8032075314806, 0.7883464276266,
    0.7730104533627, 0.7572088465065, 0.7409511253550, 0.7242470829515,
    0.7071067811865, 0.6895405447371, 0.6715589548470, 0.6531728429538,
    0.6343932841636, 0.6152315905806, 0.5956993044924, 0.5758081914178,
    0.5555702330196, 0.5349976198871, 0.5141027441932, 0.4928981922298,
    0.4713967368260, 0.4496113296546, 0.4275550934303, 0.4052413140050,
    0.3826834323651, 0.3598950365350, 0.3368898533922, 0.3136817403989,
    0.2902846772545, 0.2667127574749, 0.2429801799033, 0.2191012401569,
    0.1950903220161, 0.1709618887603, 0.1467304744554, 0.1224106751992,
    0.0980171403296, 0.0735645635997, 0.0490676743274, 0.0245412285229,
    0.0000000000000, -0.0245412285229, -0.0490676743274, -0.0735645635997,
    -0.0980171403296, -0.1224106751992, -0.1467304744554, -0.1709618887603,
    -0.1950903220161, -0.2191012401569, -0.2429801799033, -0.2667127574749,
    -0.2902846772545, -0.3136817403989, -0.3368898533922, -0.3598950365350,
    -0.3826834323651, -0.4052413140050, -0.4275550934303, -0.4496113296546,
    -0.4713967368260, -0.4928981922298, -0.5141027441932, -0.5349976198871,
    -0.5555702330196, -0.5758081914178, -0.5956993044924, -0.6152315905806,
    -0.6343932841636, -0.6531728429538, -0.6715589548470, -0.6895405447371,
    -0.7071067811865, -0.7242470829515, -0.7409511253550, -0.7572088465065,
    -0.7730104533627, -0.7883464276266, -0.8032075314806, -0.8175848131516,
    -0.8314696123025, -0.8448535652497, -0.8577286100003, -0.8700869911087,
    -0.8819212643484, -0.8932243011955, -0.9039892931234, -0.9142097557035,
    -0.9238795325113, -0.9329927988347, -0.9415440651830, -0.9495281805930,
    -0.9569403357322, -0.9637760657954, -0.9700312531945, -0.9757021300385,
    -0.9807852804032, -0.9852776423889, -0.9891765099648, -0.9924795345987,
    -0.9951847266722, -0.9972904566787, -0.9987954562052, -0.9996988186962,
    -1.0000000000000,
];
#[rustfmt::skip]
static INVSQ_LOOKUP: [f32; 33] = [
    1.414213562373, 1.392621247646, 1.371988681140, 1.352246807566,
    1.333333333333, 1.315191898443, 1.297771369046, 1.281025230441,
    1.264911064067, 1.249390095109, 1.234426799697, 1.219988562661,
    1.206045378311, 1.192569588000, 1.179535649239, 1.166919931983,
    1.154700538379, 1.142857142857, 1.131370849898, 1.120224067222,
    1.109400392450, 1.098884511590, 1.088662107904, 1.078719779941,
    1.069044967650, 1.059625885652, 1.050451462878, 1.041511287847,
    1.032795558989, 1.024295039463, 1.016001016002, 1.007905261358,
    1.000000000000,
];
#[rustfmt::skip]
static INVSQ2EXP_LOOKUP: [f32; 65] = [
    65536.0, 46340.95001, 32768.0, 23170.47501,
    16384.0, 11585.2375, 8192.0, 5792.618751,
    4096.0, 2896.309376, 2048.0, 1448.154688,
    1024.0, 724.0773439, 512.0, 362.038672,
    256.0, 181.019336, 128.0, 90.50966799,
    64.0, 45.254834, 32.0, 22.627417,
    16.0, 11.3137085, 8.0, 5.656854249,
    4.0, 2.828427125, 2.0, 1.414213562,
    1.0, 0.7071067812, 0.5, 0.3535533906,
    0.25, 0.1767766953, 0.125, 0.08838834765,
    0.0625, 0.04419417382, 0.03125, 0.02209708691,
    0.015625, 0.01104854346, 0.0078125, 0.005524271728,
    0.00390625, 0.002762135864, 0.001953125, 0.001381067932,
    0.0009765625, 0.000690533966, 0.00048828125, 0.000345266983,
    0.000244140625, 0.0001726334915, 0.0001220703125, 8.631674575e-05,
    6.103515625e-05, 4.315837288e-05, 3.051757812e-05, 2.157918644e-05,
    1.525878906e-05,
];
#[rustfmt::skip]
static FROMDB_LOOKUP: [f32; 35] = [
    1.0, 0.6309573445, 0.3981071706, 0.2511886432,
    0.1584893192, 0.1, 0.06309573445, 0.03981071706,
    0.02511886432, 0.01584893192, 0.01, 0.006309573445,
    0.003981071706, 0.002511886432, 0.001584893192, 0.001,
    0.0006309573445, 0.0003981071706, 0.0002511886432, 0.0001584893192,
    0.0001, 6.309573445e-05, 3.981071706e-05, 2.511886432e-05,
    1.584893192e-05, 1e-05, 6.309573445e-06, 3.981071706e-06,
    2.511886432e-06, 1.584893192e-06, 1e-06, 6.309573445e-07,
    3.981071706e-07, 2.511886432e-07, 1.584893192e-07,
];
#[rustfmt::skip]
static FROMDB2_LOOKUP: [f32; 32] = [
    0.9928302478, 0.9786445908, 0.9646616199, 0.9508784391,
    0.9372921937, 0.92390007, 0.9106992942, 0.8976871324,
    0.8848608897, 0.8722179097, 0.8597555737, 0.8474713009,
    0.835362547, 0.8234268041, 0.8116616003, 0.8000644989,
    0.7886330981, 0.7773650302, 0.7662579617, 0.755309592,
    0.7445176537, 0.7338799116, 0.7233941627, 0.7130582353,
    0.7028699885, 0.6928273125, 0.6829281272, 0.6731703824,
    0.6635520573, 0.6540711597, 0.6447257262, 0.6355138211,
];

//! Floor type 1: the piecewise-linear spectral envelope (Vorbis I
//! specification § 7; libvorbis 1.2.0's `floor1.c`). The curve is integer
//! work; the only float step is the multiplication of the residue by
//! `FLOOR1_fromdB_LOOKUP`, one `f32` product per bin.

use super::bits::Bits;
use super::book::{ilog, Book};
use super::Error;

pub(crate) struct Floor1 {
    partitionclass: Vec<usize>,
    class_dim: [usize; 16],
    class_subs: [u32; 16],
    class_book: [usize; 16],
    class_subbook: [[i32; 8]; 16],
    mult: i32,
    postlist: Vec<i32>,
    // look
    posts: usize,
    forward_index: Vec<usize>,
    loneighbor: Vec<usize>,
    hineighbor: Vec<usize>,
    quant_q: i32,
}

impl Floor1 {
    /// `floor1_unpack` then `floor1_look`.
    pub(crate) fn unpack(b: &mut Bits, books: usize) -> Result<Floor1, Error> {
        let bad = || Error("Vorbis: bad floor 1".into());
        let partitions = b.read(5).max(0) as usize;
        let mut partitionclass = Vec::with_capacity(partitions);
        let mut maxclass = -1i64;
        for _ in 0..partitions {
            let c = b.read(4);
            maxclass = maxclass.max(c);
            partitionclass.push(c.max(0) as usize);
        }
        let mut f = Floor1 {
            partitionclass,
            class_dim: [0; 16],
            class_subs: [0; 16],
            class_book: [0; 16],
            class_subbook: [[0; 8]; 16],
            mult: 0,
            postlist: Vec::new(),
            posts: 0,
            forward_index: Vec::new(),
            loneighbor: Vec::new(),
            hineighbor: Vec::new(),
            quant_q: 0,
        };
        for j in 0..(maxclass + 1) as usize {
            f.class_dim[j] = (b.read(3) + 1) as usize;
            let subs = b.read(2);
            if subs < 0 {
                return Err(bad());
            }
            f.class_subs[j] = subs as u32;
            if subs != 0 {
                let book = b.read(8);
                if book < 0 || book as usize >= books {
                    return Err(bad());
                }
                f.class_book[j] = book as usize;
            }
            for k in 0..(1usize << subs) {
                let sb = b.read(8) - 1;
                if sb < -1 || sb >= books as i64 {
                    return Err(bad());
                }
                f.class_subbook[j][k] = sb as i32;
            }
        }
        f.mult = (b.read(2) + 1) as i32;
        let rangebits = b.read(4);
        if rangebits < 0 {
            return Err(bad());
        }
        let rangebits = rangebits as u32;
        f.postlist = vec![0, 1 << rangebits];
        for &c in &f.partitionclass {
            for _ in 0..f.class_dim[c] {
                let t = b.read(rangebits);
                if t < 0 || t >= 1 << rangebits {
                    return Err(bad());
                }
                f.postlist.push(t as i32);
            }
        }
        if f.postlist.len() > 65 {
            return Err(bad());
        }

        // floor1_look
        let n = f.postlist.len();
        f.posts = n;
        let mut sorted: Vec<usize> = (0..n).collect();
        sorted.sort_by_key(|&i| f.postlist[i]);
        f.forward_index = sorted;
        f.quant_q = match f.mult {
            1 => 256,
            2 => 128,
            3 => 86,
            _ => 64,
        };
        for i in 0..n.saturating_sub(2) {
            let (mut lo, mut hi, mut lx, mut hx) = (0, 1, 0, f.postlist[1]);
            let currentx = f.postlist[i + 2];
            for j in 0..i + 2 {
                let x = f.postlist[j];
                if x > lx && x < currentx {
                    lo = j;
                    lx = x;
                }
                if x < hx && x > currentx {
                    hi = j;
                    hx = x;
                }
            }
            f.loneighbor.push(lo);
            f.hineighbor.push(hi);
        }
        Ok(f)
    }

    /// `floor1_inverse1`: the posts of this packet, or `None` when the
    /// floor is unused or the packet ends.
    pub(crate) fn inverse1(&self, b: &mut Bits, books: &[Book]) -> Option<Vec<i32>> {
        if b.read(1) != 1 {
            return None;
        }
        let mut fit = vec![0i32; self.posts];
        let bits = ilog((self.quant_q - 1) as u32);
        fit[0] = b.read(bits) as i32;
        fit[1] = b.read(bits) as i32;
        let mut j = 2;
        for &class in &self.partitionclass {
            let cdim = self.class_dim[class];
            let csubbits = self.class_subs[class];
            let csub = 1i64 << csubbits;
            let mut cval = 0i64;
            if csubbits != 0 {
                cval = books[self.class_book[class]].decode(b);
                if cval == -1 {
                    return None;
                }
            }
            for k in 0..cdim {
                let book = self.class_subbook[class][(cval & (csub - 1)) as usize];
                cval >>= csubbits;
                if book >= 0 {
                    let v = books[book as usize].decode(b);
                    if v == -1 {
                        return None;
                    }
                    fit[j + k] = v as i32;
                } else {
                    fit[j + k] = 0;
                }
            }
            j += cdim;
        }
        for i in 2..self.posts {
            let (ln, hn) = (self.loneighbor[i - 2], self.hineighbor[i - 2]);
            let predicted = render_point(
                self.postlist[ln],
                self.postlist[hn],
                fit[ln],
                fit[hn],
                self.postlist[i],
            );
            let hiroom = self.quant_q - predicted;
            let loroom = predicted;
            let room = hiroom.min(loroom) << 1;
            let mut val = fit[i];
            if val != 0 {
                if val >= room {
                    if hiroom > loroom {
                        val -= loroom;
                    } else {
                        val = -1 - (val - hiroom);
                    }
                } else if val & 1 != 0 {
                    val = -((val + 1) >> 1);
                } else {
                    val >>= 1;
                }
                fit[i] = val + predicted;
                fit[ln] &= 0x7fff;
                fit[hn] &= 0x7fff;
            } else {
                fit[i] = predicted | 0x8000;
            }
        }
        Some(fit)
    }

    /// `floor1_inverse2`: multiplies the first `n` bins by the curve, or
    /// zeroes them when the floor is unused.
    pub(crate) fn inverse2(&self, fit: Option<&[i32]>, out: &mut [f32], n: usize) {
        let Some(fit) = fit else {
            out[..n].fill(0.0);
            return;
        };
        let (mut hx, mut lx) = (0i32, 0i32);
        let mut ly = fit[0] * self.mult;
        for j in 1..self.posts {
            let current = self.forward_index[j];
            let mut hy = fit[current] & 0x7fff;
            if hy == fit[current] {
                hy *= self.mult;
                hx = self.postlist[current];
                render_line(n as i32, lx, hx, ly, hy, out);
                lx = hx;
                ly = hy;
            }
        }
        let v = lookup(ly);
        for x in out.iter_mut().take(n).skip(hx.max(0) as usize) {
            *x *= v;
        }
    }
}

fn lookup(y: i32) -> f32 {
    FLOOR1_FROMDB[y.clamp(0, 255) as usize]
}

fn render_point(x0: i32, x1: i32, y0: i32, y1: i32, x: i32) -> i32 {
    let (y0, y1) = (y0 & 0x7fff, y1 & 0x7fff);
    let dy = y1 - y0;
    let adx = x1 - x0;
    let ady = dy.abs();
    let err = ady * (x - x0);
    let off = if adx != 0 { err / adx } else { 0 };
    if dy < 0 {
        y0 - off
    } else {
        y0 + off
    }
}

fn render_line(n: i32, x0: i32, x1: i32, y0: i32, y1: i32, d: &mut [f32]) {
    let dy = y1 - y0;
    let adx = x1 - x0;
    if adx <= 0 {
        return;
    }
    let mut ady = dy.abs();
    let base = dy / adx;
    let sy = if dy < 0 { base - 1 } else { base + 1 };
    let mut x = x0;
    let mut y = y0;
    let mut err = 0;
    ady -= (base * adx).abs();
    let n = n.min(x1);
    if x < n {
        d[x as usize] *= lookup(y);
    }
    loop {
        x += 1;
        if x >= n {
            break;
        }
        err += ady;
        if err >= adx {
            err -= adx;
            y += sy;
        } else {
            y += base;
        }
        d[x as usize] *= lookup(y);
    }
}

/// The floor 1 inverse dB table (Vorbis I specification § 10.1), as the
/// float literals of libvorbis 1.2.0.
#[rustfmt::skip]
pub(crate) static FLOOR1_FROMDB: [f32; 256] = [
    1.0649863e-07, 1.1341951e-07, 1.2079015e-07, 1.2863978e-07,
    1.3699951e-07, 1.4590251e-07, 1.5538408e-07, 1.6548181e-07,
    1.7623575e-07, 1.8768855e-07, 1.9988561e-07, 2.128753e-07,
    2.2670913e-07, 2.4144197e-07, 2.5713223e-07, 2.7384213e-07,
    2.9163793e-07, 3.1059021e-07, 3.3077411e-07, 3.5226968e-07,
    3.7516214e-07, 3.9954229e-07, 4.2550680e-07, 4.5315863e-07,
    4.8260743e-07, 5.1396998e-07, 5.4737065e-07, 5.8294187e-07,
    6.2082472e-07, 6.6116941e-07, 7.0413592e-07, 7.4989464e-07,
    7.9862701e-07, 8.5052630e-07, 9.0579828e-07, 9.6466216e-07,
    1.0273513e-06, 1.0941144e-06, 1.1652161e-06, 1.2409384e-06,
    1.3215816e-06, 1.4074654e-06, 1.4989305e-06, 1.5963394e-06,
    1.7000785e-06, 1.8105592e-06, 1.9282195e-06, 2.0535261e-06,
    2.1869758e-06, 2.3290978e-06, 2.4804557e-06, 2.6416497e-06,
    2.8133190e-06, 2.9961443e-06, 3.1908506e-06, 3.3982101e-06,
    3.6190449e-06, 3.8542308e-06, 4.1047004e-06, 4.3714470e-06,
    4.6555282e-06, 4.9580707e-06, 5.2802740e-06, 5.6234160e-06,
    5.9888572e-06, 6.3780469e-06, 6.7925283e-06, 7.2339451e-06,
    7.7040476e-06, 8.2047000e-06, 8.7378876e-06, 9.3057248e-06,
    9.9104632e-06, 1.0554501e-05, 1.1240392e-05, 1.1970856e-05,
    1.2748789e-05, 1.3577278e-05, 1.4459606e-05, 1.5399272e-05,
    1.6400004e-05, 1.7465768e-05, 1.8600792e-05, 1.9809576e-05,
    2.1096914e-05, 2.2467911e-05, 2.3928002e-05, 2.5482978e-05,
    2.7139006e-05, 2.8902651e-05, 3.0780908e-05, 3.2781225e-05,
    3.4911534e-05, 3.7180282e-05, 3.9596466e-05, 4.2169667e-05,
    4.4910090e-05, 4.7828601e-05, 5.0936773e-05, 5.4246931e-05,
    5.7772202e-05, 6.1526565e-05, 6.5524908e-05, 6.9783085e-05,
    7.4317983e-05, 7.9147585e-05, 8.4291040e-05, 8.9768747e-05,
    9.5602426e-05, 0.00010181521, 0.00010843174, 0.00011547824,
    0.00012298267, 0.00013097477, 0.00013948625, 0.00014855085,
    0.00015820453, 0.00016848555, 0.00017943469, 0.00019109536,
    0.00020351382, 0.00021673929, 0.00023082423, 0.00024582449,
    0.00026179955, 0.00027881276, 0.00029693158, 0.00031622787,
    0.00033677814, 0.00035866388, 0.00038197188, 0.00040679456,
    0.00043323036, 0.00046138411, 0.00049136745, 0.00052329927,
    0.00055730621, 0.00059352311, 0.00063209358, 0.00067317058,
    0.00071691700, 0.00076350630, 0.00081312324, 0.00086596457,
    0.00092223983, 0.00098217216, 0.0010459992, 0.0011139742,
    0.0011863665, 0.0012634633, 0.0013455702, 0.0014330129,
    0.0015261382, 0.0016253153, 0.0017309374, 0.0018434235,
    0.0019632195, 0.0020908006, 0.0022266726, 0.0023713743,
    0.0025254795, 0.0026895994, 0.0028643847, 0.0030505286,
    0.0032487691, 0.0034598925, 0.0036847358, 0.0039241906,
    0.0041792066, 0.0044507950, 0.0047400328, 0.0050480668,
    0.0053761186, 0.0057254891, 0.0060975636, 0.0064938176,
    0.0069158225, 0.0073652516, 0.0078438871, 0.0083536271,
    0.0088964928, 0.009474637, 0.010090352, 0.010746080,
    0.011444421, 0.012188144, 0.012980198, 0.013823725,
    0.014722068, 0.015678791, 0.016697687, 0.017782797,
    0.018938423, 0.020169149, 0.021479854, 0.022875735,
    0.024362330, 0.025945531, 0.027631618, 0.029427276,
    0.031339626, 0.033376252, 0.035545228, 0.037855157,
    0.040315199, 0.042935108, 0.045725273, 0.048696758,
    0.051861348, 0.055231591, 0.058820850, 0.062643361,
    0.066714279, 0.071049749, 0.075666962, 0.080584227,
    0.085821044, 0.091398179, 0.097337747, 0.10366330,
    0.11039993, 0.11757434, 0.12521498, 0.13335215,
    0.14201813, 0.15124727, 0.16107617, 0.17154380,
    0.18269168, 0.19456402, 0.20720788, 0.22067342,
    0.23501402, 0.25028656, 0.26655159, 0.28387361,
    0.30232132, 0.32196786, 0.34289114, 0.36517414,
    0.38890521, 0.41417847, 0.44109412, 0.46975890,
    0.50028648, 0.53279791, 0.56742212, 0.60429640,
    0.64356699, 0.68538959, 0.72993007, 0.77736504,
    0.82788260, 0.88168307, 0.9389798, 1.0,
];

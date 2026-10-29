//! The save-thumbnail encoder of `Game.exe` (atlantis3-gog-fr-3473408
//! @ 0x0069c390) and its pre-pass @ 0x00410450, transcribed exactly.
//!
//! Model and golden: `atlantis3-re/tools/emu/a3_thumb_model.py`,
//! `evidence/golden/a3_thumb_encoder.txt` (real encoder under Unicorn),
//! notes `functions/saves-FUN_0069c390.md`.
//!
//! Output: the bytes the writer stores after `SPRCRYO1`: u32 flags 0x68,
//! a 256-entry palette (B, G, R, 0), u32 count 1, u32 offset 4, then one
//! sprite: u16 w, h, s16 hot spot (0, 0), u32 row offsets (relative to the
//! row data, first 0), and the rows (control bytes + palette indices).
//!
//! Steps (all integer: the x87 code of 0x006a00b0 runs under the CRT's
//! control word 0x027f, round to nearest, 53-bit, but every value it holds is
//! an integer below 2^24 and `__ftol` truncates a quotient of two such
//! integers to their integer quotient under any precision control, so
//! `S / N` in integers and exact squared distances reproduce it):
//! 1. histogram of 5-bit colours `R5 << 10 | G5 << 5 | B5` (0x0069c230);
//! 2. median cut into at most 256 boxes (0x0069fdc0): split the first box
//!    of least depth that holds more than one colour, along its widest
//!    channel (ties: red over green over blue), after sorting its colours
//!    with the VC6 CRT `qsort` (0x006d5e97) and the comparator 0x006a0380
//!    (its red case always returns 0: the order is then qsort's own
//!    shuffle), at the first colour where the running count reaches half
//!    the box's pixels;
//! 3. palette = each box's count-weighted mean (truncated), and every colour
//!    goes to its nearest palette entry (first strict minimum, an exact
//!    match stops the search) (0x006a00b0);
//! 4. rows (0x0069db30, the branch at 0x0069e3aa): the opacity probe
//!    0x0069d730 runs on the *pixels* at `y * (pitch + 1) + i` (a pointer
//!    that drifts one pixel per row, so the last rows read past the
//!    buffer: `tail`); trailing transparent pixels are dropped, runs are
//!    collected from the last opaque pixel `k` down to 1 and replayed on the
//!    index row from its start: an opaque row stores `w - 1` indices.
//!
//! Two pieces of state are not in the input:
//! - palette entries past the box count are the previous encode's (static
//!   0x007d7be0): [`ThumbEncoder`] keeps them; a fresh one holds zeros;
//! - the memory after the pixels, read by the probe for the last rows: the
//!   golden convention is zero bytes (transparent), [`encode_thumbnail`]
//!   uses it.

/// 0x00731f58: shift of the run length in the control byte, `[prev * 3 + class]`.
const SHIFT: [u8; 12] = [1, 1, 0, 2, 1, 2, 1, 2, 2, 0, 0, 0];
/// 0x00731f64: control byte base, `[prev * 3 + class]`.
const BASE: [u8; 12] = [0, 1, 0, 2, 1, 0, 1, 2, 0, 0, 0, 0];

/// 0x007d9f89 / 0x007d9f8b with the game's 16-bit (565) screens.
pub const SCREEN_565_SHIFTS: (u32, u32) = (11, 5);

/// 0x00410450: every pixel with r < 8, g < 8 and b < 8 becomes 0xff080808
/// (0x004e8280 with format 0x96 returns the alpha byte set).
pub fn darken_black(pixels: &mut [u32]) {
    for p in pixels.iter_mut() {
        if (*p & 0xff_0000) < 0x8_0000 && (*p & 0xff00) < 0x800 && (*p & 0xff) < 8 {
            *p = 0xff08_0808;
        }
    }
}

/// Encode a thumbnail as a fresh process would (zero palette history), with
/// zero bytes after the pixels; `pixels` are x8r8g8b8, `w * h`, already
/// through [`darken_black`].
pub fn encode_thumbnail(pixels: &[u32], w: usize, h: usize) -> Vec<u8> {
    ThumbEncoder::new().encode(pixels, w, h, w, &[])
}

/// The encoder with its static state (0x007d7be0) and the probe's shifts.
#[derive(Clone)]
pub struct ThumbEncoder {
    /// 0x007d7be0: B, G, R of each box, kept from one encode to the next.
    pub mcut: [u8; 0x300],
    /// 0x007d9f89, 0x007d9f8b.
    pub shifts: (u32, u32),
}

impl Default for ThumbEncoder {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn chan(c: u32, axis: u32) -> u32 {
    ((c >> (5 * axis)) << 3) & 0xff
}

#[inline]
fn key(p: u32) -> usize {
    ((((p >> 19) & 31) << 10) | (((p >> 11) & 31) << 5) | ((p >> 3) & 31)) as usize
}

/// The VC6 CRT `qsort` (0x006d5e97: CUTOFF 8, `shortsort` by
/// max-selection, middle element as pivot) on `a`, element width 4.
fn crt_qsort(a: &mut [u32], comp: &dyn Fn(u32, u32) -> i32) {
    let num = a.len();
    if num < 2 {
        return;
    }
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let (mut lo, mut hi) = (0usize, num - 1);
    loop {
        let size = hi - lo + 1;
        let mut recurse = false;
        if size <= 8 {
            let mut h = hi;
            while h > lo {
                let mut mx = lo;
                for p in lo + 1..=h {
                    if comp(a[p], a[mx]) > 0 {
                        mx = p;
                    }
                }
                a.swap(mx, h);
                h -= 1;
            }
        } else {
            let mid = lo + size / 2;
            a.swap(mid, lo);
            let mut lg = lo;
            let mut hg = hi + 1;
            loop {
                lg += 1;
                while lg <= hi && comp(a[lg], a[lo]) <= 0 {
                    lg += 1;
                }
                hg -= 1;
                while hg > lo && comp(a[hg], a[lo]) >= 0 {
                    hg -= 1;
                }
                if hg < lg {
                    break;
                }
                a.swap(lg, hg);
            }
            a.swap(lo, hg);
            // in bytes `higuy - 1 - lo >= hi - loguy`, i.e. 4 a - 1 >= 4 b: a > b
            if hg as isize - lo as isize > hi as isize - lg as isize {
                if lo + 1 < hg {
                    stack.push((lo, hg - 1));
                }
                if lg < hi {
                    lo = lg;
                    recurse = true;
                }
            } else {
                if lg < hi {
                    stack.push((lg, hi));
                }
                if lo + 1 < hg {
                    hi = hg - 1;
                    recurse = true;
                }
            }
        }
        if recurse {
            continue;
        }
        match stack.pop() {
            Some((l, h)) => {
                lo = l;
                hi = h;
            }
            None => return,
        }
    }
}

struct Box {
    lo: usize,
    hi: usize,
    sum: u32,
    depth: u32,
    min: [u32; 3],
    max: [u32; 3],
}

fn box_stats(list: &[u32], lo: usize, hi: usize, sum: u32, depth: u32) -> Box {
    let mut min = [0xff; 3];
    let mut max = [0; 3];
    for &c in &list[lo..=hi] {
        for a in 0..3 {
            let v = chan(c, a as u32);
            max[a] = max[a].max(v);
            min[a] = min[a].min(v);
        }
    }
    Box {
        lo,
        hi,
        sum,
        depth,
        min,
        max,
    }
}

impl ThumbEncoder {
    pub fn new() -> Self {
        ThumbEncoder {
            mcut: [0; 0x300],
            shifts: SCREEN_565_SHIFTS,
        }
    }

    /// 0x0069d730 (4-byte pixels): false = transparent.
    fn opaque(&self, p: u32) -> bool {
        let (s89, s8b) = self.shifts;
        let a = (p & 0xff) >> 3;
        let d = ((p >> (s89 - s8b)) & 0xff) >> 3;
        let b = ((p >> s89) & 0xff) >> 3;
        a.max(d).max(b) != 0
    }

    /// 0x0069fdc0 + 0x006a00b0: fills `self.mcut[..3 * n]`, turns each used
    /// histogram slot into its palette index.
    fn median_cut(&mut self, hist: &mut [u32]) {
        let mut list: Vec<u32> = (0..0x8000u32).filter(|&i| hist[i as usize] != 0).collect();
        let total: u32 = hist.iter().sum();
        let mut boxes = vec![box_stats(&list, 0, list.len() - 1, total, 0)];
        while boxes.len() < 256 {
            let mut sel = None;
            let mut best = 0xff;
            for (i, b) in boxes.iter().enumerate() {
                if b.lo != b.hi && b.depth < best {
                    sel = Some(i);
                    best = b.depth;
                }
            }
            let Some(sel) = sel else { break };
            let (lo, hi, sum, depth) = (
                boxes[sel].lo,
                boxes[sel].hi,
                boxes[sel].sum,
                boxes[sel].depth,
            );
            let d: [u32; 3] =
                std::array::from_fn(|a| boxes[sel].max[a].wrapping_sub(boxes[sel].min[a]) & 0xff);
            let mut axis = 0;
            if d[0] >= d[1] && d[0] >= d[2] {
                axis = 0;
            }
            if d[1] >= d[0] && d[1] >= d[2] {
                axis = 1;
            }
            if d[2] >= d[0] && d[2] >= d[1] {
                axis = 2;
            }
            // 0x006a0380: blue / green compare the two; the red case compares
            // the second element's red with itself
            let comp = move |x: u32, y: u32| -> i32 {
                if axis == 2 {
                    0
                } else {
                    chan(x, axis) as i32 - chan(y, axis) as i32
                }
            };
            crt_qsort(&mut list[lo..=hi], &comp);
            let mut acc = 0u32;
            let mut k = lo;
            while k < hi {
                if (sum >> 1) <= acc {
                    break;
                }
                acc += hist[list[k] as usize];
                k += 1;
            }
            boxes[sel] = box_stats(&list, lo, k - 1, acc, depth + 1);
            boxes.push(box_stats(&list, k, hi, sum - acc, depth + 1));
        }
        for (j, b) in boxes.iter().enumerate() {
            let mut s = [0u32; 3];
            for &c in &list[b.lo..=b.hi] {
                for (a, v) in s.iter_mut().enumerate() {
                    *v += chan(c, a as u32) * hist[c as usize];
                }
            }
            for (a, v) in s.iter().enumerate() {
                self.mcut[j * 3 + a] = (v / b.sum) as u8;
            }
        }
        let n = boxes.len();
        for b in &boxes {
            for &c in &list[b.lo..=b.hi] {
                let (cb, cg, cr) = (chan(c, 0) as i32, chan(c, 1) as i32, chan(c, 2) as i32);
                let mut best = i32::MAX; // FLT_MAX: above every distance (<= 3 * 255^2)
                let mut bj = 0u32;
                for j in 0..n {
                    let (pb, pg, pr) = (
                        self.mcut[j * 3] as i32,
                        self.mcut[j * 3 + 1] as i32,
                        self.mcut[j * 3 + 2] as i32,
                    );
                    let d = (pb - cb) * (pb - cb) + (pg - cg) * (pg - cg) + (pr - cr) * (pr - cr);
                    if d == 0 {
                        bj = j as u32;
                        break;
                    }
                    if d < best {
                        best = d;
                        bj = j as u32;
                    }
                }
                hist[c as usize] = bj;
            }
        }
    }

    /// 0x0069c390(pixels, out, w, h, pitch) after 0x00410450. `tail`: the
    /// bytes that follow the `pitch * h` pixels in memory (missing bytes read
    /// as 0). Returns the encoder's output (empty when w or h < 2: the
    /// function then returns 0).
    pub fn encode(
        &mut self,
        pixels: &[u32],
        w: usize,
        h: usize,
        pitch: usize,
        tail: &[u8],
    ) -> Vec<u8> {
        let mut hist = vec![0u32; 0x8000];
        for y in 0..h {
            for &p in &pixels[y * pitch..y * pitch + w] {
                hist[key(p)] += 1;
            }
        }
        if w * h > 0 {
            self.median_cut(&mut hist);
        }
        let mut idx = vec![0u8; pitch * h];
        for y in 0..h {
            for x in 0..w {
                idx[y * pitch + x] = hist[key(pixels[y * pitch + x])] as u8;
            }
        }
        if w < 2 || h < 2 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(0x7f18);
        out.extend_from_slice(&0x68u32.to_le_bytes());
        for j in 0..256 {
            out.extend_from_slice(&self.mcut[j * 3..j * 3 + 3]);
            out.push(0);
        }
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        let npx = pitch * h;
        let pixel = |i: usize| -> u32 {
            if i < npx {
                pixels[i]
            } else {
                let o = (i - npx) * 4;
                let mut b = [0u8; 4];
                for (t, v) in b.iter_mut().enumerate() {
                    *v = tail.get(o + t).copied().unwrap_or(0);
                }
                u32::from_le_bytes(b)
            }
        };
        let cls = |i: usize| -> u8 { self.opaque(pixel(i)) as u8 };
        out.extend_from_slice(&(w as u16).to_le_bytes());
        out.extend_from_slice(&(h as u16).to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        let tab = out.len();
        out.resize(tab + 4 * h, 0);
        let data = out.len();
        let mut runs: Vec<(usize, u8)> = Vec::with_capacity(w + 1);
        for y in 0..h {
            if y > 0 {
                let off = (out.len() - data) as u32;
                out[tab + 4 * y..tab + 4 * y + 4].copy_from_slice(&off.to_le_bytes());
            }
            let base = y * (pitch + 1);
            let mut k = w - 1;
            let mut c = cls(base + k);
            while c == 0 && k != 0 {
                k -= 1;
                c = cls(base + k);
            }
            runs.clear();
            runs.push((0, 2)); // 0x007d7f00 = 2: the bottom of the run stack
            while k != 0 {
                let rc = c;
                let mut n = 0;
                loop {
                    n += 1;
                    k -= 1;
                    c = cls(base + k);
                    if c != rc || k == 0 {
                        break;
                    }
                }
                runs.push((n, rc));
            }
            let mut src = y * pitch;
            let mut prev = 2usize;
            let mut cur = 2usize;
            let mut rem = 0usize;
            loop {
                let i;
                if rem == 0 {
                    let (n, rc) = runs.pop().expect("run stack");
                    cur = rc as usize;
                    i = prev * 3 + cur;
                    prev = cur;
                    if cur == 2 {
                        out.push(BASE[i]);
                        break;
                    }
                    rem = n;
                } else {
                    i = cur * 4;
                }
                let sh = SHIFT[i];
                let cnt = rem.min(0x100 >> sh);
                rem -= cnt;
                out.push(BASE[i] | (((cnt as u32 - 1) << sh) & 0xff) as u8);
                if cur == 1 {
                    out.extend_from_slice(&idx[src..src + cnt]);
                }
                src += cnt;
            }
        }
        out
    }
}

/// Deterministic synthetic inputs of `a3_thumb_encoder.txt`
/// (`a3_thumb_model.gen_image`), for the tests.
#[cfg(test)]
pub(crate) fn gen_image(kind: &str, w: usize, h: usize, seed: u32) -> Vec<u32> {
    let mut s = if seed == 0 { 1 } else { seed };
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let p = match kind {
                "scene" => {
                    let n = next();
                    let c: [u32; 3] = if y < h * 2 / 5 {
                        [
                            60 + y as u32 * 2 + (n & 7),
                            90 + y as u32 * 3 + ((n >> 3) & 7),
                            160 + y as u32 + ((n >> 6) & 15),
                        ]
                    } else if y < h / 2 {
                        [n & 3, (n >> 2) & 3, (n >> 4) & 7]
                    } else {
                        [
                            120 + x as u32 / 2 + (n & 31),
                            80 + (y - h / 2) as u32 * 2 + ((n >> 5) & 15),
                            40 + ((n >> 9) & 31),
                        ]
                    };
                    (c[0].min(255) << 16) | (c[1].min(255) << 8) | c[2].min(255)
                }
                "few" => {
                    [0x102030, 0xC08040, 0x40C080, 0xFFFFFF, 0x000000][((x / 7) + (y / 5) * 3) % 5]
                }
                "many" => next() & 0xFF_FFFF,
                "black" => 0,
                "dark" => next() & 0x0F0F0F,
                "redline" => {
                    if (x + y) % 3 != 0 {
                        (0xF8 + (next() & 7)) << 16
                    } else {
                        0x204060
                    }
                }
                "ramp" => {
                    (((x * 255 / (w - 1).max(1)) as u32) << 16)
                        | (((y * 255 / (h - 1).max(1)) as u32) << 8)
                        | ((x * y) as u32 & 0xFF)
                }
                _ => panic!("kind {kind}"),
            };
            let alpha = if kind == "many" {
                (next() & 0xFF) << 24
            } else {
                0
            };
            px.push(p | alpha);
        }
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;
    use md5::{Digest, Md5};

    fn md5_hex(b: &[u8]) -> String {
        let d: [u8; 16] = Md5::digest(b).into();
        d.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn le(px: &[u32]) -> Vec<u8> {
        px.iter().flat_map(|p| p.to_le_bytes()).collect()
    }

    /// kind, w, h, seed, md5 of the input, md5 after 0x00410450, length,
    /// md5 of the output.
    type Case = (
        &'static str,
        usize,
        usize,
        u32,
        &'static str,
        &'static str,
        usize,
        &'static str,
    );

    /// `case` rows of a3_thumb_encoder.txt.
    const CASES: &[Case] = &[
        (
            "scene",
            85,
            65,
            1,
            "f3a8cdf0349c8692ed11524196fe98b5",
            "14d49d91437de8f95ed796ca952aa01e",
            6894,
            "8931cb32ad917b3298705bf8f7b9159b",
        ),
        (
            "scene",
            65,
            50,
            2,
            "57189239ed939d0c3cabcf515504a9c1",
            "b01ce72c8c00e78e3376f03dec5a0ec4",
            4495,
            "3cb0e9f5347e71cb9fc86ce411055a32",
        ),
        (
            "scene",
            52,
            40,
            3,
            "8c97ec9892b4079aea45c4efae67c1bf",
            "374774fc784e52550ff56c5fc6f5226d",
            3285,
            "ff3d898ef08186c6d01c81f620767bde",
        ),
        (
            "few",
            85,
            65,
            4,
            "f0ca262a6a21249f77444147f15cc7d8",
            "158d793197373874825c1becdd819262",
            6894,
            "eb114769d2830ce1f9b16ef89c72b2b1",
        ),
        (
            "few",
            52,
            40,
            5,
            "f3ea3220c41ad911c288f7ee91d5d273",
            "5b9580be54bb906ea43b1c6636f6abac",
            3285,
            "6e8531be983b138eb104055bc8934fb2",
        ),
        (
            "many",
            85,
            65,
            6,
            "f598dbe69ad6ade6e2f6f0361fd41c1a",
            "877d3afeee29dc86ffa2bb00e2706b83",
            6894,
            "c234acce433d1ae096e5eb9db421b091",
        ),
        (
            "many",
            65,
            50,
            7,
            "9fa5d393b3f11ea6e2a6a92586d1c559",
            "9fa5d393b3f11ea6e2a6a92586d1c559",
            4495,
            "77432b818cf5016793cd2cc8a3f5c8f7",
        ),
        (
            "many",
            52,
            40,
            8,
            "c46ac8d2cc62f34ee9baa8b28125c449",
            "c46ac8d2cc62f34ee9baa8b28125c449",
            3285,
            "b2e98ba9dc978d631eb08cccf56ff521",
        ),
        (
            "black",
            85,
            65,
            9,
            "bffdea1e2d660ac7ad8e725ab65114d3",
            "57c5b7034cbfda2819661c14b23e8333",
            6894,
            "d9e39ca6d9457bbda10b1ee8785901c4",
        ),
        (
            "black",
            52,
            40,
            10,
            "71de6d465f892f0f97d7cd084c415800",
            "5d38ffe3b588a64d32068f836bab384a",
            3285,
            "45d9ae761471eb27d811fbb89df9c161",
        ),
        (
            "dark",
            65,
            50,
            11,
            "a8332233a42dfdc8d8cd91bdc2a0a730",
            "56380491d28e8dd115237ec8e3bafbc4",
            4500,
            "9f58f6a9ae5ca32d1733fbd29b5333f2",
        ),
        (
            "dark",
            52,
            40,
            12,
            "0df87cd48291d8f1db0fcd58063a2dd4",
            "f7c98ca5c321e20f1bb2a03172cc04a1",
            3288,
            "b4ecdb0fd6fa85094a81ca95fc2c4980",
        ),
        (
            "redline",
            52,
            40,
            13,
            "dc2c17ef46f7753059ff5ed61224342f",
            "dc2c17ef46f7753059ff5ed61224342f",
            3422,
            "affd6eab933ac002cccb50119fd5a38b",
        ),
        (
            "redline",
            85,
            65,
            14,
            "aa568f5af23aa973edc95a57812098f9",
            "aa568f5af23aa973edc95a57812098f9",
            7229,
            "c75148e8a1955446431e0b4f64589e42",
        ),
        (
            "ramp",
            85,
            65,
            15,
            "17708ade73da55a8fe08acffe05ffa09",
            "2bedab2c3f9083c0ac71f23ff8e5f8de",
            6904,
            "0aeb58420b36739c91abd04eb4c41b9e",
        ),
        (
            "ramp",
            65,
            50,
            16,
            "bbd15fb2e1bde420fc32a6a2a1da590b",
            "df4e6b30a78d642431cd8d69fc5ab10b",
            4495,
            "d419a86ce67c352abb723f1d0ccf7574",
        ),
        (
            "ramp",
            52,
            40,
            17,
            "1d7f8768e411901d710b6d0f18dbbb45",
            "2e79ca29e8ec44e99837955a9e039c19",
            3291,
            "6eca9e81e0492495e345e355ac719623",
        ),
        (
            "scene",
            85,
            65,
            18,
            "c2b2e76679c65b63c41931a968203516",
            "4303fb3a2f2a0366e0e2e6c7cbabf0ec",
            6894,
            "259d77e82810f5f8f762b83fe511103b",
        ),
        (
            "few",
            65,
            50,
            19,
            "f1876ad7584084263dcb9e705e6e39cb",
            "710a42ed303306862a5f4d1bc01dba0d",
            4495,
            "925261a9fc49b8a23d4ca9f1483f8f74",
        ),
        (
            "many",
            2,
            2,
            20,
            "dcb053cd6d7de44cbe6d232855151eb3",
            "dcb053cd6d7de44cbe6d232855151eb3",
            1056,
            "b48337e555ddcd203b53991aa7a3d642",
        ),
        (
            "scene",
            1,
            5,
            21,
            "c3c251a8c839e98635e19ca0af8ee1db",
            "c3c251a8c839e98635e19ca0af8ee1db",
            0,
            "d41d8cd98f00b204e9800998ecf8427e",
        ),
        (
            "scene",
            9,
            1,
            22,
            "e416f7d8418479eb76e09a595d61441e",
            "e416f7d8418479eb76e09a595d61441e",
            0,
            "d41d8cd98f00b204e9800998ecf8427e",
        ),
    ];

    /// `seq` rows: the same inputs through one encoder (palette history).
    const SEQ: &[(usize, &str)] = &[
        (6894, "8931cb32ad917b3298705bf8f7b9159b"),
        (4495, "3cb0e9f5347e71cb9fc86ce411055a32"),
        (3285, "da7e311e838c32315b7552d5e5186bb5"),
        (6894, "9ab958c66633c1aa31a163bfa81b6192"),
        (3285, "9c5514f0d134d0d178e9cdc9d8ee0a2e"),
        (6894, "c234acce433d1ae096e5eb9db421b091"),
        (4495, "77432b818cf5016793cd2cc8a3f5c8f7"),
        (3285, "b2e98ba9dc978d631eb08cccf56ff521"),
        (6894, "09b9a14a4cc8e39303653ce1b3cdf91a"),
        (3285, "02d6aaf4c44e515628c4742644bb4b2b"),
        (4500, "3843b0d67333fd7c28b73cfe97c36706"),
        (3288, "6c3bfdf54a8b2b287163544131447db7"),
        (3422, "111ff29b871d29b34a29312bb1fd31ad"),
        (7229, "5ce91f5ea1e18a154594333307ee0f02"),
        (6904, "0aeb58420b36739c91abd04eb4c41b9e"),
        (4495, "d419a86ce67c352abb723f1d0ccf7574"),
        (3291, "6eca9e81e0492495e345e355ac719623"),
        (6894, "259d77e82810f5f8f762b83fe511103b"),
        (4495, "a30d7e1e806768ca816917f475925935"),
        (1056, "b9108563f721588f6fbe309c7e4d2bd8"),
        (0, "d41d8cd98f00b204e9800998ecf8427e"),
        (0, "d41d8cd98f00b204e9800998ecf8427e"),
    ];

    #[test]
    fn fresh_encodes_match_the_original() {
        for &(kind, w, h, seed, inp, black, len, out) in CASES {
            let mut px = gen_image(kind, w, h, seed);
            assert_eq!(md5_hex(&le(&px)), inp, "{kind} {w}x{h} input");
            darken_black(&mut px);
            assert_eq!(md5_hex(&le(&px)), black, "{kind} {w}x{h} 0x00410450");
            let b = encode_thumbnail(&px, w, h);
            assert_eq!(b.len(), len, "{kind} {w}x{h} length");
            assert_eq!(md5_hex(&b), out, "{kind} {w}x{h} bytes");
        }
    }

    #[test]
    fn palette_history_matches_the_original() {
        let mut enc = ThumbEncoder::new();
        for (&(kind, w, h, seed, ..), &(len, out)) in CASES.iter().zip(SEQ) {
            let mut px = gen_image(kind, w, h, seed);
            darken_black(&mut px);
            let b = enc.encode(&px, w, h, w, &[]);
            assert_eq!(
                (b.len(), md5_hex(&b).as_str()),
                (len, out),
                "{kind} {w}x{h} seed {seed}"
            );
        }
    }

    /// a3_save_thumbs.txt `enc tiny 4 3`: whole bank, guard 00 and ff.
    #[test]
    fn tiny_bank_bytes_and_past_end_read() {
        let mut px = [0xFF0000u32, 0x00FF00, 0x0000FF, 0xFFFFFF].repeat(3);
        darken_black(&mut px);
        let b = encode_thumbnail(&px, 4, 3);
        assert_eq!(b.len(), 1069);
        assert_eq!(
            &b[..16],
            &[0x68, 0, 0, 0, 0, 0, 0xf8, 0, 0xf8, 0, 0, 0, 0, 0xf8, 0, 0]
        );
        let mut sp = b"SPRCRYO1".to_vec();
        sp.extend_from_slice(&b);
        assert_eq!(md5_hex(&sp), "a2c715c5302f5fc676bc5d5bd57c5eb0");
        let b = ThumbEncoder::new().encode(&px, 4, 3, 4, &[0xff; 1024]);
        let mut sp = b"SPRCRYO1".to_vec();
        sp.extend_from_slice(&b);
        assert_eq!(
            (b.len(), md5_hex(&sp).as_str()),
            (1071, "ceae6cdf1bb19b609f8e3c9749db6e2f")
        );
    }

    #[test]
    fn qsort_is_the_vc6_one() {
        // all-equal comparator (the red axis): VC6 qsort still moves elements
        let mut a: Vec<u32> = (0..20).collect();
        crt_qsort(&mut a, &|_, _| 0);
        assert_ne!(a, (0..20).collect::<Vec<u32>>());
        let mut b: Vec<u32> = (0..50).map(|i| (i * 37) % 50).collect();
        crt_qsort(&mut b, &|x, y| x as i32 - y as i32);
        assert_eq!(b, (0..50).collect::<Vec<u32>>());
    }
}

//! The exact OpenAL mixer against `wrap_oal.dll` 2.2.0.5's goldens
//! (`golden/a4_mix.txt`: the DLL's own mixer run in an emulator on
//! synthetic input) and against scripted cases of the reference model
//! equal to the DLL (`golden/oalmix_cases.txt`). No game data needed.

use atlantis4_engine::oalmix::{pitch_step, target_gains, Mixer, SourceId};

const MIX: &str = include_str!("golden/a4_mix.txt");
const CASES: &str = include_str!("golden/oalmix_cases.txt");

/// SHA-256 (FIPS 180-4), for the goldens' hashes.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut m = data.to_vec();
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    m.extend((data.len() as u64 * 8).to_be_bytes());
    for chunk in m.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[4 * i..4 * i + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    let mut out = [0u8; 32];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// The goldens' hash: the first 16 hex digits of SHA-256 of the int16 LE output.
fn h16(v: &[i16]) -> String {
    let bytes: Vec<u8> = v.iter().flat_map(|s| s.to_le_bytes()).collect();
    sha256(&bytes)[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The golden lines that start with `prefix`.
fn lines(prefix: &str) -> Vec<&'static str> {
    MIX.lines().filter(|l| l.starts_with(prefix)).collect()
}

/// A source with its buffers queued and its gain set, as the golden rig does.
fn source(m: &mut Mixer, bufs: &[(Vec<i16>, u8, u32)], gain: f32) -> SourceId {
    let mut ids = Vec::new();
    for (s, ch, rate) in bufs {
        let b = m.gen_buffer();
        m.buffer_data(b, s.clone(), *ch, *rate);
        ids.push(b);
    }
    let s = m.gen_source();
    m.queue(s, &ids);
    m.set_gain(s, gain);
    s
}

fn mix(m: &mut Mixer, n: usize) -> Vec<i16> {
    let mut out = Vec::new();
    m.mix(n, &mut out);
    out
}

fn left(v: &[i16]) -> Vec<i16> {
    v.iter().step_by(2).copied().collect()
}

fn changes(l: &[i16]) -> String {
    let c: Vec<String> = l
        .iter()
        .enumerate()
        .filter(|&(i, &v)| i == 0 || v != l[i - 1])
        .map(|(i, v)| format!("({i}, {v})"))
        .collect();
    format!("[{}]", c.join(", "))
}

fn nonzero(l: &[i16]) -> String {
    let c: Vec<String> = l
        .iter()
        .enumerate()
        .filter(|&(_, &v)| v != 0)
        .map(|(i, v)| format!("({i}, {v})"))
        .collect();
    format!("[{}]", c.join(", "))
}

#[test]
fn sha256_is_right() {
    assert_eq!(
        sha256(b"abc")[..8],
        [0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea]
    );
}

#[test]
fn gains_equal_the_dll() {
    let want = lines("ch=");
    assert_eq!(want.len(), 20);
    for w in want {
        let f: Vec<&str> = w.split_whitespace().collect();
        let ch: u8 = f[0][3..].parse().unwrap();
        let bits = u32::from_str_radix(&f[1][7..], 16).unwrap();
        let g = target_gains(f32::from_bits(bits), ch);
        assert_eq!(
            format!("0x{:08x} 0x{:08x}", g[0].to_bits(), g[1].to_bits()),
            format!("{} {}", f[4], f[5]),
            "{w}"
        );
    }
}

#[test]
fn ramps_equal_the_dll() {
    let want = lines("changes");
    let mut m = Mixer::new();
    let s = source(&mut m, &[(vec![16384; 2000], 1, 44_100)], 1.0);
    m.play(s);
    assert_eq!(
        format!("changes {} model=equal", changes(&left(&mix(&mut m, 600)))),
        want[0]
    );
    let mut m = Mixer::new();
    let s = source(&mut m, &[(vec![16384; 2000], 1, 44_100)], 1.0);
    m.play(s);
    let mut l = Vec::new();
    for _ in 0..4 {
        l.extend(left(&mix(&mut m, 200)));
    }
    assert_eq!(format!("changes {} model=equal", changes(&l)), want[1]);
}

#[test]
fn impulses_through_the_interpolator_equal_the_dll() {
    let want = lines("rate=");
    for (rate, w) in [44_100, 22_050, 48_000, 11_025, 32_000]
        .into_iter()
        .zip(want)
    {
        let mut s = vec![0i16; 2000];
        s[1000] = 16384;
        s[1010] = -32768;
        s[1011] = 32767;
        let mut m = Mixer::new();
        let id = source(&mut m, &[(s, 1, rate)], 1.0);
        m.play(id);
        let l = left(&mix(&mut m, 4200));
        assert_eq!(
            format!(
                "rate={rate} step={} nonzero(L)={} model=equal",
                pitch_step(rate),
                nonzero(&l)
            ),
            w
        );
    }
}

/// The golden script's sweep: 20 Hz to Nyquist, its sine from the
/// platform's mathematics library as Python's.
fn sweep(n: usize, ch: u8, rate: u32, amp: f64) -> Vec<i16> {
    let mut out = Vec::new();
    let mut ph = 0.0f64;
    for i in 0..n {
        let f = 20.0 * (f64::from(rate) / 2.0 / 20.0).powf(i as f64 / n as f64);
        ph += 2.0 * std::f64::consts::PI * f / f64::from(rate);
        out.push((amp * ph.sin()) as i16);
        if ch == 2 {
            out.push((amp * (ph * 0.5).sin()) as i16);
        }
    }
    out
}

#[test]
fn sweeps_squares_and_streams_equal_the_dll() {
    let want = lines("sweep");
    let mut k = 0;
    for rate in [22_050u32, 44_100, 48_000] {
        for ch in [1u8, 2] {
            for g in [1.0f32, 0.8] {
                let mut m = Mixer::new();
                let s = source(&mut m, &[(sweep(8820, ch, rate, 30000.0), ch, rate)], g);
                m.play(s);
                let mut out = Vec::new();
                for _ in 0..(8820 * 44_100 / rate as usize) / 1102 + 2 {
                    m.mix(1102, &mut out);
                }
                let f: Vec<&str> = want[k].split_whitespace().collect();
                assert_eq!(
                    format!("frames={} hash={}", out.len() / 2, h16(&out)),
                    format!("{} {}", f[4], f[5]),
                    "{}",
                    want[k]
                );
                k += 1;
            }
        }
    }
    assert_eq!(k, want.len());

    let mut m = Mixer::new();
    let mut ids = Vec::new();
    for ch in [1u8, 2, 1] {
        let mut s = Vec::new();
        for i in 0..4410 {
            let v = if (i / 32) % 2 == 1 { 32767 } else { -32768 };
            for _ in 0..ch {
                s.push(v);
            }
        }
        ids.push(source(&mut m, &[(s, ch, 44_100)], 1.0));
    }
    for s in ids {
        m.play(s);
    }
    let mut out = Vec::new();
    for _ in 0..5 {
        m.mix(1102, &mut out);
    }
    let hi = out.iter().filter(|&&v| v == 32767).count();
    let lo = out.iter().filter(|&&v| v == -32768).count();
    assert_eq!(
        format!(
            "squares frames={} clipped_hi={hi} clipped_lo={lo} hash={} model=equal",
            out.len() / 2,
            h16(&out)
        ),
        lines("squares")[0]
    );

    // TeMusic's pattern: 4 buffers of 11,025 frames, refilled as processed.
    let pcm = sweep(11025 * 10, 2, 44_100, 20000.0);
    let chunks: Vec<Vec<i16>> = pcm.chunks(22050).map(|c| c.to_vec()).collect();
    let mut m = Mixer::new();
    let bids: Vec<_> = (0..4).map(|_| m.gen_buffer()).collect();
    let s = m.gen_source();
    let mut next = 0;
    for &b in &bids {
        m.buffer_data(b, chunks[next].clone(), 2, 44_100);
        next += 1;
    }
    m.queue(s, &bids);
    m.set_gain(s, f32::from_bits(0x3f19_999a));
    m.play(s);
    let mut out = Vec::new();
    for _ in 0..120 {
        let p = m.processed(s);
        if p > 0 {
            for b in m.unqueue(s, p) {
                if next < chunks.len() {
                    m.buffer_data(b, chunks[next].clone(), 2, 44_100);
                    m.queue(s, &[b]);
                    next += 1;
                }
            }
        }
        m.mix(1102, &mut out);
    }
    let end = out.iter().rposition(|&v| v != 0).unwrap() / 2 + 1;
    let w = lines("stream")[0];
    assert!(
        w.starts_with(&format!(
            "stream frames={} hash={} ",
            out.len() / 2,
            h16(&out)
        )),
        "{w}"
    );
    assert!(w.contains(&format!("end_frame={end} ")), "{w}");

    // Buffer end: the stale padding of a replayed buffer.
    let mut m = Mixer::new();
    let a = m.gen_buffer();
    let b = m.gen_buffer();
    m.buffer_data(a, vec![10000i16; 600], 1, 44_100);
    m.buffer_data(b, vec![20000i16; 600], 1, 44_100);
    let s = m.gen_source();
    m.queue(s, &[a, b]);
    m.play(s);
    mix(&mut m, 1300);
    m.unqueue(s, 2);
    m.queue(s, &[a]);
    m.play(s);
    let l = left(&mix(&mut m, 1300));
    let last = l.iter().rposition(|&v| v != 0).unwrap();
    let fmt = |v: &[i16]| {
        format!(
            "[{}]",
            v.iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    assert_eq!(
        format!(
            "A alone: L[0:3]={} L[596:606]={} last_nonzero_frame={last} model=equal",
            fmt(&l[0..3]),
            fmt(&l[596..606])
        ),
        lines("A alone")[0]
    );
    m.stop(s);
    m.play(s);
    let e = mix(&mut m, 64);
    assert_eq!(
        format!(
            "after stop+play: L[0:3]={} L[31:33]={} model=equal",
            fmt(&[e[0], e[2], e[4]]),
            fmt(&[e[62], e[64]])
        ),
        lines("after stop+play")[0]
    );
}

/// The integer generators of `oalmix_cases.py`.
fn gen(kind: &str, n: usize, ch: usize, arg: u64) -> Vec<i16> {
    let tot = n * ch;
    match kind {
        "rand" => {
            let mut x = arg as u32;
            (0..tot)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    (x >> 16) as u16 as i16
                })
                .collect()
        }
        "square" => (0..tot)
            .map(|i| {
                if (i / (arg as usize * ch)) % 2 == 1 {
                    32767
                } else {
                    -32768
                }
            })
            .collect(),
        "imp" => {
            let mut v = vec![0i16; tot];
            v[(arg % tot as u64) as usize] = if arg & 1 == 1 { 32767 } else { -32768 };
            v
        }
        _ => (0..tot)
            .map(|i| {
                let t = (i as u64 * arg) % 1024;
                let v = if t < 512 { t } else { 1024 - t } as i64;
                ((v - 256) * 117) as i16
            })
            .collect(),
    }
}

#[test]
fn scripted_cases_equal_the_reference_model() {
    let mut m = Mixer::new();
    let mut srcs: Vec<SourceId> = Vec::new();
    let (mut calls, mut case) = (0, String::new());
    for l in CASES.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        let s = || srcs[f[1].parse::<usize>().unwrap()];
        match f[0] {
            "case" => {
                m = Mixer::new();
                srcs.clear();
                case = l.to_string();
            }
            "src" => srcs.push(m.gen_source()),
            "buf" => {
                let ch: u8 = f[2].parse().unwrap();
                let n: usize = f[4].parse().unwrap();
                let b = m.gen_buffer();
                m.buffer_data(
                    b,
                    gen(f[5], n, usize::from(ch), f[6].parse().unwrap()),
                    ch,
                    f[3].parse().unwrap(),
                );
                let id = s();
                m.queue(id, &[b]);
            }
            "gain" => {
                let id = s();
                m.set_gain(
                    id,
                    f32::from_bits(u32::from_str_radix(&f[2][2..], 16).unwrap()),
                );
            }
            "play" => {
                let id = s();
                m.play(id);
            }
            "stop" => {
                let id = s();
                m.stop(id);
            }
            "mix" => {
                let out = mix(&mut m, f[1].parse().unwrap());
                assert_eq!(h16(&out), f[2], "{case}, call {l}");
                calls += 1;
            }
            _ => panic!("{l}"),
        }
    }
    assert_eq!(calls, 798);
}

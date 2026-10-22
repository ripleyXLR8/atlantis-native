//! The original's saves against `tests/golden/zapfile.txt`: the save
//! table, the `ZapFile` encoder and decoder on the golden's synthetic
//! buffers (rebuilt here with the same seeded generator: Python's
//! Mersenne Twister), a slot's file for synthetic globals byte for byte,
//! the size checks, and `atlant2.cfg`.

use std::collections::HashMap;

use atlantis2_data::save::{
    entry, entry_offset, read_save, scan_save, zap_decode, zap_encode, Cfg, SaveData, SaveFormat,
    SystemTime, ENTRIES, SAVE_SIZE,
};

const GOLDEN: &str = include_str!("golden/zapfile.txt");

fn golden() -> HashMap<&'static str, &'static str> {
    GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once(' '))
        .collect()
}

/// Python's `random.Random(seed)` for a seed below 2³²: MT19937 seeded by
/// `init_by_array([seed])`, `randrange` by rejection on `getrandbits`.
struct PyRandom {
    mt: [u32; 624],
    i: usize,
}

impl PyRandom {
    fn new(seed: u32) -> PyRandom {
        let mut mt = [0u32; 624];
        mt[0] = 19_650_218;
        for i in 1..624 {
            mt[i] = 1_812_433_253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        let key = [seed];
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..624 {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1_664_525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= 624 {
                mt[0] = mt[623];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..623 {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1_566_083_941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= 624 {
                mt[0] = mt[623];
                i = 1;
            }
        }
        mt[0] = 0x8000_0000;
        PyRandom { mt, i: 624 }
    }

    fn next_u32(&mut self) -> u32 {
        if self.i >= 624 {
            for k in 0..624 {
                let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % 624] & 0x7fff_ffff);
                let mut v = self.mt[(k + 397) % 624] ^ (y >> 1);
                if y & 1 != 0 {
                    v ^= 0x9908_b0df;
                }
                self.mt[k] = v;
            }
            self.i = 0;
        }
        let mut y = self.mt[self.i];
        self.i += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// `randrange(n)` for 0 < n < 2³².
    fn below(&mut self, n: u32) -> u32 {
        let k = 32 - n.leading_zeros();
        loop {
            let r = self.next_u32() >> (32 - k);
            if r < n {
                return r;
            }
        }
    }

    fn range(&mut self, a: u32, b: u32) -> u32 {
        a + self.below(b - a)
    }
}

/// SHA-256 (FIPS 180-4), for the golden's digests.
fn sha256(data: &[u8]) -> String {
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
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for t in 0..16 {
            w[t] = u32::from_be_bytes(block[4 * t..4 * t + 4].try_into().unwrap());
        }
        for t in 16..64 {
            let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
            let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
            w[t] = w[t - 16]
                .wrapping_add(s0)
                .wrapping_add(w[t - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for t in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[t])
                .wrapping_add(w[t]);
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
    h.iter().map(|x| format!("{x:08x}")).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The golden's synthetic buffers, with the byte after each in memory.
fn cases() -> Vec<(&'static str, Vec<u8>, u8)> {
    let mut r = PyRandom::new(20_261_022);
    let seq = |a: u8, n: u8| (a..a + n).collect::<Vec<u8>>();
    let cat = |parts: &[Vec<u8>]| parts.concat();
    let mut out: Vec<(&'static str, Vec<u8>, u8)> = vec![
        ("one_byte", b"A".to_vec(), 0xa5),
        ("five", b"ABCDE".to_vec(), 0xa5),
        ("nine_equal", vec![0; 9], 0xa5),
        ("ten_equal", vec![0; 10], 0xa5),
        ("eleven_equal", vec![0; 11], 0xa5),
        ("twenty_equal", vec![0; 20], 0xa5),
        ("a_then_20_zero", cat(&[b"A".to_vec(), vec![0; 20]]), 0xa5),
        ("lit10_run10", cat(&[seq(1, 10), vec![7; 10]]), 0xa5),
        ("lit12_run9_end", cat(&[seq(1, 12), vec![7; 9]]), 0xa5),
        ("lit12_run9_end_pe00", cat(&[seq(1, 12), vec![7; 9]]), 0x00),
        ("lit12_run10_end", cat(&[seq(1, 12), vec![7; 10]]), 0xa5),
        (
            "run9_inside",
            cat(&[seq(1, 12), vec![7; 9], seq(20, 12)]),
            0xa5,
        ),
        (
            "run10_inside",
            cat(&[seq(1, 12), vec![7; 10], seq(20, 12)]),
            0xa5,
        ),
        (
            "two_runs",
            cat(&[vec![1; 15], vec![2; 15], vec![3; 15]]),
            0xa5,
        ),
        (
            "alternating_64",
            (0..64).map(|i| (i & 1) as u8 * 0xff).collect(),
            0xa5,
        ),
    ];
    out.push((
        "random_300",
        (0..300).map(|_| r.below(256) as u8).collect(),
        0xa5,
    ));
    let mut big = vec![0u8; SAVE_SIZE];
    for i in (0..SAVE_SIZE).step_by(997) {
        for j in 0..r.range(1, 40) as usize {
            if i + j < big.len() {
                big[i + j] = r.below(256) as u8;
            }
        }
    }
    out.push(("sparse_69288", big, 0xa5));
    out.push(("ff_69288", vec![0xff; SAVE_SIZE], 0xa5));
    out.push((
        "random_69288",
        (0..SAVE_SIZE).map(|_| r.below(256) as u8).collect(),
        0xa5,
    ));
    out
}

fn fmt_runs(runs: &[(bool, usize)], limit: usize) -> String {
    let mut s: Vec<String> = runs
        .iter()
        .take(limit)
        .map(|&(lit, n)| format!("{}{n}", if lit { 'L' } else { 'F' }))
        .collect();
    if runs.len() > limit {
        s.push(format!("...(+{})", runs.len() - limit));
    }
    format!("{} {}", runs.len(), s.join(" "))
}

#[test]
fn the_save_table() {
    let g = golden();
    assert_eq!(g["save_table.entries"], "35");
    assert_eq!(g["save_table.bytes"], SAVE_SIZE.to_string());
    for (k, (a, n)) in ENTRIES.iter().enumerate() {
        assert_eq!(
            g[format!("save_table.{k:02}").as_str()],
            format!("0x{a:08x}:{n}"),
            "entry {k}"
        );
    }
}

#[test]
fn the_runs_of_every_buffer() {
    let g = golden();
    let mut checked = 0;
    for (name, data, past_end) in cases() {
        let key = |f: &str| format!("rle.{name}.{f}");
        let inp = g[key("in").as_str()];
        assert_eq!(
            inp,
            format!("{} {}", data.len(), &sha256(&data)[..16]),
            "{name}: the input"
        );
        let tail: [u8; 4] = [0xcc; 4];
        assert_eq!(g[key("header_tail").as_str()], hex(&tail));
        let (file, runs) = zap_encode(&data, past_end, tail);
        assert_eq!(
            g[key("file_len").as_str()],
            file.len().to_string(),
            "{name}"
        );
        assert_eq!(g[key("file_sha256").as_str()], sha256(&file), "{name}");
        if let Some(h) = g.get(key("file_hex").as_str()) {
            assert_eq!(*h, hex(&file), "{name}");
        }
        assert_eq!(g[key("runs").as_str()], fmt_runs(&runs, 24), "{name}");
        // The full load gives the data back.
        let (size, back) = zap_decode(&file, 0).unwrap();
        assert_eq!(
            g[key("load").as_str()],
            format!("ok=1 size={size} same=True")
        );
        assert_eq!(back, data, "{name}");
        // The 48-byte load rebuilds the first 48 bytes (the rest of the
        // buffer is not written: what the golden's `rest_zero` shows is
        // the harness's allocation; where the original's clip test wraps,
        // this clips).
        if let Some(l48) = g.get(key("load48").as_str()) {
            let (_, part) = zap_decode(&file, 48).unwrap();
            assert_eq!(part[..48], data[..48], "{name}");
            assert!(
                l48.starts_with("ok=1 same=True") || l48.starts_with("overrun"),
                "{name}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 19);
}

/// fill_globals of the golden's harness (seed 7), then what the save's
/// header refresh (@ 0x00478eb0) wrote from the harness's state.
fn slot_globals() -> SaveData {
    let mut r = PyRandom::new(7);
    let mut d = SaveData::default();
    for (k, &(_, n)) in ENTRIES.iter().enumerate() {
        let b = d.entry_mut(k);
        for _ in 0..(n / 64).max(1) {
            let p = r.below(n as u32) as usize;
            for j in 0..r.range(1, 12) as usize {
                if p + j < n {
                    b[p + j] = r.below(256) as u8;
                }
            }
        }
    }
    d.set_i32_at(entry::VARS, 179, 5);
    d.set_i32(entry::CHAPTER, 75);
    d.set_wam(b"testplace.wam");
    d.set_i32(entry::VIEW, 42);
    d.set_time(SystemTime {
        year: 1999,
        month: 10,
        day_of_week: 5,
        day: 8,
        hour: 14,
        minute: 30,
        second: 15,
        milliseconds: 250,
    });
    d.set_i32(entry::VIGNETTE, 5);
    d.set_f32(entry::YAW, -90.0);
    d.set_f32(entry::PITCH, 12.5);
    d.set_i32(entry::MUSIC, 7);
    d.set_i32(entry::CURSOR, 3);
    d
}

#[test]
fn a_slot_byte_for_byte() {
    let g = golden();
    let d = slot_globals();
    let (file, runs) = zap_encode(d.bytes(), 0, [0xcc; 4]);
    assert_eq!(g["slot.file_len"], file.len().to_string());
    assert_eq!(g["slot.header"], hex(&file[..20]));
    assert_eq!(g["slot.runs"], fmt_runs(&runs, 12));
    assert_eq!(g["slot.file_sha256"], sha256(&file));
    assert_eq!(g["slot.decoded_sha256"], sha256(d.bytes()));
    for k in [0, 2, 3, 4, 9, 10, 11, 19] {
        let e = d.entry(k);
        let line = g[format!("slot.entry{k:02}").as_str()];
        assert_eq!(
            line,
            format!(
                "off={} len={} {} matches_memory=True",
                entry_offset(k),
                e.len(),
                hex(&e[..e.len().min(20)])
            )
        );
    }
    let h = scan_save(&file).unwrap();
    assert_eq!(g["slot.h.wam"], String::from_utf8_lossy(&h.wam));
    assert_eq!(g["slot.h.chapter"], h.chapter.to_string());
    assert_eq!(g["slot.h.view"], h.view.to_string());
    let t = h.time;
    assert_eq!(
        g["slot.h.systemtime"],
        format!(
            "{} {} {} {} {} {} {} {}",
            t.year, t.month, t.day_of_week, t.day, t.hour, t.minute, t.second, t.milliseconds
        )
    );
    assert_eq!(g["slot.h.vignette"], h.vignette.to_string());
    // The load gives the 35 entries back.
    assert!(g["slot.load"].starts_with("ret=1 entries_restored=35/35"));
    let (back, format) = read_save(&file).unwrap();
    assert_eq!((back, format), (d, SaveFormat::Zap));
}

#[test]
fn raw_and_refused_files() {
    let g = golden();
    // A raw file of 69288 bytes loads (and is then saved again as a
    // ZapFile, the header entries refreshed).
    assert!(g["raw.load"].starts_with("ret=1"));
    assert!(g["raw.rewritten"].starts_with("zap=True len=69317"));
    let mut r = PyRandom::new(11);
    let raw: Vec<u8> = (0..SAVE_SIZE)
        .map(|i| if i % 5 == 0 { r.below(256) as u8 } else { 0 })
        .collect();
    let (d, f) = read_save(&raw).unwrap();
    assert_eq!((d.bytes(), f), (&raw[..], SaveFormat::Raw));
    assert!(scan_save(&raw).is_some());
    // Refused: a ZapFile of another size, a raw file of another size, a
    // bad magic.
    for (name, file) in [
        ("zap_wrong_size", zap_encode(&[0; 100], 0, [0; 4]).0),
        ("raw_wrong_size", vec![0; 100]),
        ("bad_magic", {
            let mut f = zap_encode(slot_globals().bytes(), 0, [0xcc; 4]).0;
            f[..12].copy_from_slice(b"ZAPFILE 1.1\0");
            f
        }),
    ] {
        assert!(g[format!("bad.{name}").as_str()].starts_with("ret=0"));
        assert!(read_save(&file).is_err(), "{name}");
        assert!(scan_save(&file).is_none(), "{name}");
    }
}

#[test]
fn the_options_file() {
    let g = golden();
    let cfg = Cfg {
        invert_y: 1,
        sensitivity: 2000,
        fullscreen: 1,
        subtitles: 1,
        centre_marker: 0,
        general: 200,
        effects: 100,
        music: 90,
        dialogs: 110,
        width: 800,
        height: 600,
    };
    let bytes = cfg.to_bytes();
    assert_eq!(g["cfg.len"], bytes.len().to_string());
    assert_eq!(g["cfg.hex"], hex(&bytes));
    let back = Cfg::parse(&bytes);
    assert_eq!(back, cfg);
    let read = format!(
        "0x006569cc={} 0x005da268={} 0x005e2f2c={} 0x005df970={} 0x005db84c={} 0x005dd2b8={} \
         0x005dd2bc={} 0x005dc9dc={} 0x005df974={} 0x005e2f24={} 0x005e2f28={}",
        back.invert_y,
        back.sensitivity,
        back.fullscreen,
        back.subtitles,
        back.centre_marker,
        back.general,
        back.effects,
        back.music,
        back.dialogs,
        back.width,
        back.height
    );
    assert_eq!(g["cfg.read"], read);
    // No file: the defaults; a short one is read as far as it goes.
    assert_eq!(Cfg::parse(&[]), Cfg::default());
    assert_eq!(Cfg::parse(&bytes[..4]).invert_y, 1);
    assert_eq!(Cfg::parse(&bytes[..4]).general, 256);
}

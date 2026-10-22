//! The music player's ring against the original (`tests/golden/music_player.txt`):
//! the streamer run against a simulated play cursor as the original's code
//! ran under emulation, step for step: the start, the refills, the end of
//! each track (the next one written behind the cursor, the last ~5.8 s of
//! the old one never heard), the failed rewind of an empty list or of
//! loops (the ring looping), what is heard (md5) and where. Then the minor
//! bug fixes: every track heard to its end.
//!
//! Needs the game's discs: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::music::{self, MusicData, Ring, Stream, StreamEvent, RING_BYTES};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/music_player.txt");

fn files() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn load(files: &A2Files, n: i32) -> Option<MusicData> {
    let data = files.read(Folder::ApcMusic, &music::music_name(n))?;
    let marks = files
        .read(Folder::ApcMusic, &music::spp_name(n))
        .and_then(|d| music::parse_spp(&d))
        .unwrap_or_default();
    Some(MusicData {
        music: n,
        data: Arc::new(data),
        marks,
    })
}

/// Voice 0's buffer with a play cursor the test moves.
struct SimRing {
    buf: Vec<i16>,
    cursor: i32,
}

impl Ring for SimRing {
    fn cursor(&self) -> Option<i32> {
        Some(self.cursor)
    }
    fn set_cursor(&mut self, at: i32) {
        self.cursor = at;
    }
    fn write(&mut self, at: i32, samples: &[i16]) {
        let start = (at / 2) as usize;
        assert!(
            start + samples.len() <= self.buf.len(),
            "a write past the ring"
        );
        self.buf[start..start + samples.len()].copy_from_slice(samples);
    }
}

impl SimRing {
    /// The `n` bytes under the cursor, which then moves past them.
    fn hear(&mut self, n: i32) -> Vec<u8> {
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n / 2 {
            let at = ((self.cursor + 2 * i) % RING_BYTES / 2) as usize;
            out.extend_from_slice(&self.buf[at].to_le_bytes());
        }
        self.cursor = (self.cursor + n) % RING_BYTES;
        out
    }
}

fn hex(d: impl AsRef<[u8]>) -> String {
    d.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// A scenario of the golden.
struct Scenario {
    tag: &'static str,
    music: i32,
    /// 0 from the start (FUN_00477410), else a mark (−1 a random one).
    mark: i32,
    loops: i32,
    list: &'static str,
    steps: Option<u32>,
    step: i32,
}

const SCENARIOS: [Scenario; 8] = [
    Scenario {
        tag: "m30.start",
        music: 30,
        mark: 0,
        loops: 0,
        list: "30)",
        steps: None,
        step: 0x1000,
    },
    Scenario {
        tag: "m20.mark3",
        music: 20,
        mark: 3,
        loops: 0,
        list: "20-3)",
        steps: None,
        step: 0x1000,
    },
    Scenario {
        tag: "m14.loop3",
        music: 14,
        mark: 0,
        loops: 3,
        list: "14*3)",
        steps: None,
        step: 0x1000,
    },
    Scenario {
        tag: "m16.list",
        music: 16,
        mark: 0,
        loops: 0,
        list: "16,14)",
        steps: None,
        step: 0x1000,
    },
    Scenario {
        tag: "m01.random",
        music: 1,
        mark: -1,
        loops: 0,
        list: "1)",
        steps: Some(3000),
        step: 0x1000,
    },
    Scenario {
        tag: "m30.nolist",
        music: 30,
        mark: 0,
        loops: 0,
        list: "",
        steps: None,
        step: 0x1000,
    },
    Scenario {
        tag: "m16.list.s1468",
        music: 16,
        mark: 0,
        loops: 0,
        list: "16,14)",
        steps: None,
        step: 1468,
    },
    Scenario {
        tag: "m30.nolist.s1468",
        music: 30,
        mark: 0,
        loops: 0,
        list: "",
        steps: None,
        step: 1468,
    },
];

/// The player started as the harness starts it.
fn start(files: &A2Files, s: &Scenario) -> (Stream, SimRing, Rand) {
    let mut st = Stream::default();
    st.loops = s.loops;
    let mut rand = Rand::new(1);
    assert!(st.open(load(files, s.music).unwrap()));
    let mut ring = SimRing {
        buf: Vec::new(),
        cursor: 0,
    };
    if s.mark == 0 {
        st.decode_start();
        rand = Rand::new(0);
        ring.buf = st.buffer.clone();
    } else {
        ring.buf = st.buffer.clone();
        assert!(st.goto_mark(s.mark, &mut rand, &mut ring));
    }
    st.events.clear();
    (st, ring, rand)
}

/// The scenario's rows, as the harness writes them (the `init` rows left
/// out).
fn run(files: &A2Files, s: &Scenario, fixes: bool) -> Vec<String> {
    let (mut st, mut ring, mut rand) = start(files, s);
    let mut rows = Vec::new();
    let mut all = Md5::new();
    let mut seg = Md5::new();
    let (mut total, mut seg_bytes) = (0u64, 0u64);
    let (mut k, mut switches, mut after) = (0u32, 0, 0);
    let limit = s.steps.unwrap_or(200_000);
    while k < limit {
        k += 1;
        let mut ld = |n| load(files, n);
        st.refill(
            &mut ring,
            s.list.as_bytes(),
            s.music,
            &mut rand,
            &mut ld,
            fixes,
        );
        for e in std::mem::take(&mut st.events) {
            let evs = match e {
                StreamEvent::Open(n) => vec![format!("('open', '{}')", music::music_name(n))],
                StreamEvent::Cursor(to, from) => vec![format!("('setpos', {to}, {from})")],
                StreamEvent::Recopy(n) => vec![
                    format!("('remove', '{}')", music::music_name(n)),
                    format!("('loadmusik', '{n})')"),
                ],
            };
            for ev in evs {
                rows.push(format!(
                    "{}.step{k} {ev} heard_before={total} segment_md5={} segment_bytes={seg_bytes} cursor={:x}",
                    s.tag,
                    hex(seg.clone().finalize()),
                    ring.cursor
                ));
                if ev.starts_with("('setpos'") || ev.starts_with("('open'") {
                    seg = Md5::new();
                    seg_bytes = 0;
                }
                if ev.starts_with("('open'") {
                    switches += 1;
                }
            }
        }
        let chunk = ring.hear(s.step);
        all.update(&chunk);
        seg.update(&chunk);
        total += s.step as u64;
        seg_bytes += s.step as u64;
        if switches >= 2 || st.music().is_none() {
            after += 1;
            if after >= 300 {
                break;
            }
        }
    }
    rows.push(format!(
        "{}.end steps={k} heard={total} md5={} segment_md5={} segment_bytes={seg_bytes} pos={} write={:x} state=4 loopsleft={} mark={}",
        s.tag,
        hex(all.finalize()),
        hex(seg.finalize()),
        st.pos,
        st.write,
        st.loops,
        st.current_mark
    ));
    rows
}

#[test]
fn ring_matches_the_original() {
    let Some(files) = files() else { return };
    for s in &SCENARIOS {
        let want: Vec<&str> = GOLDEN
            .lines()
            .filter(|l| {
                l.strip_prefix(s.tag)
                    .is_some_and(|r| r.starts_with(".step") || r.starts_with(".end"))
            })
            .collect();
        assert!(!want.is_empty(), "{}", s.tag);
        let got = run(&files, s, false);
        assert_eq!(got, want, "{}", s.tag);
    }
}

/// The PCM bytes of music `n` from its start or from mark `k`.
fn pcm_bytes(files: &A2Files, n: i32, k: i32) -> Vec<u8> {
    let f = load(files, n).unwrap();
    let mark = (k > 0).then(|| f.marks[k as usize - 1]);
    let pcm = music::decode_music(&f.data, mark.as_ref()).unwrap();
    pcm.samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

/// Runs `s` with the fixes until `bytes` have been heard, returning them,
/// with the opens (music, mark played) in order.
fn hear_fixed(files: &A2Files, s: &Scenario, bytes: usize) -> (Vec<u8>, Vec<(i32, i32)>) {
    let (mut st, mut ring, mut rand) = start(files, s);
    let mut heard = Vec::with_capacity(bytes + s.step as usize);
    let mut opens = Vec::new();
    while heard.len() < bytes {
        let mut ld = |n| load(files, n);
        st.refill(
            &mut ring,
            s.list.as_bytes(),
            s.music,
            &mut rand,
            &mut ld,
            true,
        );
        for e in std::mem::take(&mut st.events) {
            assert!(
                !matches!(e, StreamEvent::Cursor(..) | StreamEvent::Recopy(_)),
                "{e:?}"
            );
            if let StreamEvent::Open(n) = e {
                opens.push((n, st.current_mark));
            }
        }
        heard.extend(ring.hear(s.step));
    }
    heard.truncate(bytes);
    (heard, opens)
}

#[test]
fn fixed_tracks_play_to_their_end() {
    let Some(files) = files() else { return };
    // A track followed by another: the whole of muzik16, then muzik14
    // from the mark drawn, with no sample lost or repeated.
    let s = &SCENARIOS[6];
    let first = pcm_bytes(&files, 16, 0);
    let (heard, opens) = hear_fixed(&files, s, first.len() + 0x100000);
    assert_eq!(opens.len(), 1, "{opens:?}");
    let (next, mark) = opens[0];
    assert_eq!(next, 14);
    let second = pcm_bytes(&files, next, mark);
    assert!(heard[..first.len()] == first[..], "muzik16 heard whole");
    assert!(
        heard[first.len()..] == second[..0x100000],
        "muzik14 from mark {mark} right after"
    );

    // An empty list: the track again from its start, after its last
    // sample (the original loops the ring's last 5.9 s instead).
    for s in [&SCENARIOS[5], &SCENARIOS[7]] {
        let track = pcm_bytes(&files, 30, 0);
        let (heard, opens) = hear_fixed(&files, s, track.len() + 0x100000);
        assert!(opens.is_empty(), "{opens:?}");
        assert!(heard[..track.len()] == track[..], "{}: heard whole", s.tag);
        assert!(
            heard[track.len()..] == track[..0x100000],
            "{}: again from its start",
            s.tag
        );
    }

    // Loops (`*3`): played whole three times.
    let s = &SCENARIOS[2];
    let track = pcm_bytes(&files, 14, 0);
    let (heard, _) = hear_fixed(&files, s, 3 * track.len());
    for i in 0..3 {
        assert!(
            heard[i * track.len()..(i + 1) * track.len()] == track[..],
            "loop {i}"
        );
    }
}

//! The OpenAL mix of *Atlantis Evolution*, sample for sample.
//!
//! The game calls `alcOpenDevice(NULL)`; on Windows 10/11 Creative's router
//! `OpenAL32.dll` of the game folder opens the "Generic Software" device of
//! `wrap_oal.dll` 2.2.0.5, which mixes in software at a fixed 44,100 Hz,
//! 16-bit stereo. Its mixer (`wrap_oal.dll` @ 0x10028e40) runs the x87 with
//! 53-bit precision and rounding toward zero (`_controlfp(0x300, 0x300)`),
//! so every intermediate is rounded toward zero to 53 bits and every float
//! store toward zero to 24 bits. For the sources the game makes (16-bit
//! mono or stereo, `AL_GAIN` only, at the listener, no effects):
//!
//! - the source's sample at its position, linearly interpolated with a
//!   14-bit integer fraction (the identity at 44,100 Hz), plus `1e-26f`;
//! - a mono source goes to both sides with gain `AL_GAIN × 0.70711f`, a
//!   stereo source left to left and right to right with `AL_GAIN`;
//! - each gain moves toward its target in 16 steps of 1/16, at frames
//!   31, 63, … 511 of each call (a fade-in from 0 when a source starts,
//!   a ramp when `AL_GAIN` changes);
//! - the products are summed in a float bus in the order the sources were
//!   created; the bus is rounded down (`floor`) and clipped to 16 bits;
//! - a buffer is read 4 frames past its end, from 8 frames of padding
//!   that holds the start of the next queued buffer (zeros otherwise).
//!
//! Without x87 rounding control, each step here is one IEEE double
//! operation (round to nearest) corrected toward zero with the exact error
//! from TwoSum or Dekker's product, which gives the same bits on every
//! platform, wasm32 included.
//!
//! The output equals the DLL's for the same AL operations between mix
//! calls and the same sequence of call lengths. The DLL's call lengths
//! follow Windows' timer and DirectSound cursor; this mixer is driven
//! with a fixed schedule ([`BLOCK`]), so where a ramp or a cut falls
//! depends on it, not the steady-state samples.

/// The output rate, fixed by the DLL (`wrap_oal.dll` @ 0x100276a2).
pub const RATE: u32 = 44_100;

/// The fixed mix call length used in place of the DLL's 25 ms timer
/// ticks: 25 ms rounded down, the size of a regular tick.
pub const BLOCK: usize = 1_102;

/// The centre gain of a mono source on two speakers (`0x3f350529`,
/// `wrap_oal.dll` @ 0x100518c4).
const PAN_CENTRE: f32 = f32::from_bits(0x3f35_0529);
/// Added after the dry filter (`1e-26f`, @ 0x100518f0).
const ANTI_DENORMAL: f64 = 1e-26_f32 as f64;
/// Below this a ramp step snaps to its target (qword @ 0x10051900).
const SNAP: f64 = 1.0 / 8_388_608.0;

/// The next double toward zero.
fn toward_zero(r: f64) -> f64 {
    if r == 0.0 {
        r
    } else {
        f64::from_bits(r.to_bits() - 1)
    }
}

/// `a + b` rounded toward zero to 53 bits.
fn add(a: f64, b: f64) -> f64 {
    let s = a + b;
    // TwoSum: s + e == a + b exactly.
    let bb = s - a;
    let e = (a - (s - bb)) + (b - bb);
    if e != 0.0 && (e > 0.0) != (s > 0.0) {
        toward_zero(s)
    } else {
        s
    }
}

/// Veltkamp's split of a double into two 26-bit halves.
fn split(a: f64) -> (f64, f64) {
    let c = 134_217_729.0 * a; // 2^27 + 1
    let hi = c - (c - a);
    (hi, a - hi)
}

/// `a × b` rounded toward zero to 53 bits (Dekker's exact product, no FMA).
fn mul(a: f64, b: f64) -> f64 {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    let e = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
    if e != 0.0 && (e > 0.0) != (p > 0.0) {
        toward_zero(p)
    } else {
        p
    }
}

/// `a / b` (b > 0) rounded toward zero to 53 bits.
fn div(a: f64, b: f64) -> f64 {
    let q = a / b;
    // The exact remainder a − q·b has the sign of the rounding error.
    let p = q * b;
    let (qh, ql) = split(q);
    let (bh, bl) = split(b);
    let pe = ((qh * bh - p) + qh * bl + ql * bh) + ql * bl;
    let r = (a - p) - pe;
    if r != 0.0 && (r > 0.0) != (q > 0.0) {
        toward_zero(q)
    } else {
        q
    }
}

/// A double rounded toward zero to a float (an `fst dword` under CHOP).
fn f32z(x: f64) -> f32 {
    let f = x as f32;
    if f64::from(f).abs() > x.abs() {
        f32::from_bits(f.to_bits() - 1)
    } else {
        f
    }
}

/// The dry gains `(L, R)` of a source at the listener (`wrap_oal.dll`
/// @ 0x10027ac0): distance and cone gains are 1.
pub fn target_gains(gain: f32, channels: u8) -> [f32; 2] {
    if channels == 1 {
        let t = f32z(mul(f64::from(gain), f64::from(PAN_CENTRE)));
        [t, t]
    } else {
        [gain, gain]
    }
}

/// The source's step in 1/16384 frames (@ 0x10029146..0x10029228),
/// clamped to 16..=65536: 16384 at 44,100 Hz.
pub fn pitch_step(rate: u32) -> u32 {
    let r = div(f64::from(rate), f64::from(RATE));
    let v = f32z(r * 16_384.0);
    (v.floor() as i64).clamp(16, 0x10000) as u32
}

/// An `alGenBuffers` name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferId(u32);

/// An `alGenSources` name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceId(u32);

struct Buffer {
    samples: std::sync::Arc<[i16]>,
    channels: u8,
    rate: u32,
    frames: usize,
    /// 8 frames after the data, zeroed by `alBufferData`, then holding the
    /// start of the buffer queued after this one.
    pad: [i16; 16],
}

impl Buffer {
    fn at(&self, idx: usize) -> i32 {
        match self.samples.get(idx) {
            Some(&s) => i32::from(s),
            None => {
                let k = idx - self.samples.len();
                if k < 8 * usize::from(self.channels) {
                    i32::from(self.pad[k])
                } else {
                    0
                }
            }
        }
    }
}

/// A source's state as `AL_SOURCE_STATE` reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Initial,
    Playing,
    Stopped,
}

struct Source {
    id: SourceId,
    queue: Vec<BufferId>,
    /// Index in `queue` of the buffer being played.
    cur: usize,
    pos: usize,
    frac: u32,
    gain: f32,
    /// The running dry gains.
    g: [f32; 2],
    state: State,
}

/// The software mixer with an AL-shaped interface.
#[derive(Default)]
pub struct Mixer {
    buffers: std::collections::HashMap<u32, Buffer>,
    /// In creation order: the order of the mix.
    sources: Vec<Source>,
    next: u32,
    dry: Vec<[f32; 2]>,
}

impl Mixer {
    pub fn new() -> Self {
        Self::default()
    }

    fn name(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    pub fn gen_buffer(&mut self) -> BufferId {
        BufferId(self.name())
    }

    /// `alBufferData`: 16-bit samples, interleaved when stereo. The
    /// padding is zeroed.
    pub fn buffer_data(
        &mut self,
        b: BufferId,
        samples: impl Into<std::sync::Arc<[i16]>>,
        channels: u8,
        rate: u32,
    ) {
        let samples = samples.into();
        let frames = samples.len() / usize::from(channels.max(1));
        self.buffers.insert(
            b.0,
            Buffer {
                samples,
                channels: channels.max(1),
                rate,
                frames,
                pad: [0; 16],
            },
        );
    }

    pub fn delete_buffer(&mut self, b: BufferId) {
        self.buffers.remove(&b.0);
    }

    /// `alGenSources`: a new source at the end of the mixing order.
    pub fn gen_source(&mut self) -> SourceId {
        let id = SourceId(self.name());
        self.sources.push(Source {
            id,
            queue: Vec::new(),
            cur: 0,
            pos: 0,
            frac: 0,
            gain: 1.0,
            g: [0.0; 2],
            state: State::Initial,
        });
        id
    }

    /// `alDeleteSources`: the source is no longer mixed (a cut at the
    /// call boundary).
    pub fn delete_source(&mut self, s: SourceId) {
        self.sources.retain(|x| x.id != s);
    }

    fn src(&mut self, s: SourceId) -> Option<&mut Source> {
        self.sources.iter_mut().find(|x| x.id == s)
    }

    pub fn queue(&mut self, s: SourceId, bufs: &[BufferId]) {
        if let Some(src) = self.src(s) {
            src.queue.extend_from_slice(bufs);
        }
    }

    /// `AL_BUFFERS_PROCESSED`.
    pub fn processed(&self, s: SourceId) -> usize {
        self.sources.iter().find(|x| x.id == s).map_or(0, |x| {
            if x.state == State::Playing {
                x.cur
            } else {
                x.queue.len()
            }
        })
    }

    /// `AL_BUFFERS_QUEUED`.
    pub fn queued(&self, s: SourceId) -> usize {
        self.sources
            .iter()
            .find(|x| x.id == s)
            .map_or(0, |x| x.queue.len())
    }

    /// `alSourceUnqueueBuffers` of `n` processed buffers.
    pub fn unqueue(&mut self, s: SourceId, n: usize) -> Vec<BufferId> {
        let Some(src) = self.src(s) else {
            return Vec::new();
        };
        let n = n.min(src.queue.len());
        src.cur = src.cur.saturating_sub(n);
        src.queue.drain(..n).collect()
    }

    /// `alSourcef(AL_GAIN)`: the new target, reached by the next call's ramp.
    pub fn set_gain(&mut self, s: SourceId, gain: f32) {
        if let Some(src) = self.src(s) {
            src.gain = gain;
        }
    }

    /// `alSourcePlay`: from the first buffer; the running gains are kept.
    pub fn play(&mut self, s: SourceId) {
        if let Some(src) = self.src(s) {
            src.state = State::Playing;
            src.cur = 0;
            src.pos = 0;
            src.frac = 0;
        }
    }

    /// `alSourceStop` from the application: the gains go to 0, so the next
    /// play fades in.
    pub fn stop(&mut self, s: SourceId) {
        if let Some(src) = self.src(s) {
            src.state = State::Stopped;
            src.g = [0.0; 2];
        }
    }

    /// The number of sources (generated and not deleted).
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub fn state(&self, s: SourceId) -> Option<State> {
        self.sources.iter().find(|x| x.id == s).map(|x| x.state)
    }

    /// One mixer call of `n` frames: interleaved stereo 16-bit samples
    /// appended to `out`.
    pub fn mix(&mut self, n: usize, out: &mut Vec<i16>) {
        self.dry.clear();
        self.dry.resize(n, [0.0; 2]);
        let dry = &mut self.dry;
        let buffers = &mut self.buffers;
        for src in self.sources.iter_mut() {
            if src.state != State::Playing {
                continue;
            }
            let mut done = 0;
            let mut counter = 0u32;
            while src.state == State::Playing && done < n {
                let Some(&bid) = src.queue.get(src.cur) else {
                    src.state = State::Stopped;
                    break;
                };
                // Look-ahead: the next queued buffer's first 8 frames go
                // into this buffer's padding.
                let next: Option<Vec<i16>> = src.queue.get(src.cur + 1).and_then(|nb| {
                    buffers
                        .get(&nb.0)
                        .map(|nb| nb.samples.iter().take(16).copied().collect())
                });
                let Some(buf) = buffers.get_mut(&bid.0) else {
                    src.state = State::Stopped;
                    break;
                };
                if let Some(next) = next {
                    let lim = (8 * usize::from(buf.channels)).min(next.len());
                    buf.pad[..lim].copy_from_slice(&next[..lim]);
                }
                let buf = &*buf;
                let tgt = target_gains(src.gain, buf.channels);
                let mut step = [0f32; 2];
                for c in 0..2 {
                    let x = add(f64::from(tgt[c]), -f64::from(src.g[c])) / 16.0;
                    step[c] = f32z(x);
                    if x.abs() < SNAP {
                        step[c] = 0.0;
                        src.g[c] = tgt[c];
                    }
                }
                let st = i64::from(pitch_step(buf.rate));
                let remaining =
                    (buf.frames as i64 + 4 - src.pos as i64) * 16_384 - i64::from(src.frac);
                let avail = remaining / st;
                let nseg = if avail > 0 {
                    ((n - done) as i64).min(avail) as usize
                } else {
                    0
                };
                let ch = usize::from(buf.channels);
                let mut acc = i64::from(src.frac);
                for j in 0..nseg {
                    counter += 1;
                    if counter & 31 == 0 && counter <= 512 {
                        for (g, st) in src.g.iter_mut().zip(step) {
                            *g = f32z(add(f64::from(st), f64::from(*g)));
                        }
                    }
                    let i = (acc >> 14) as usize;
                    let f = (acc & 0x3fff) as i32;
                    let base = (src.pos + i) * ch;
                    let mut ys = [0f64; 2];
                    for (c, y) in ys.iter_mut().enumerate().take(ch.min(2)) {
                        let s0 = buf.at(base + c);
                        let s1 = buf.at(base + ch + c);
                        let v = (s0 * (16_384 - f) + s1 * f) >> 14;
                        let v = i32::from(v as i16);
                        // The dry filter has coefficient 0 here, so its state
                        // drops out: y = s + 1e-26f.
                        *y = add(f64::from(v), ANTI_DENORMAL);
                    }
                    let d = &mut dry[done + j];
                    for c in 0..2 {
                        let y = if ch == 1 { ys[0] } else { ys[c] };
                        d[c] = f32z(add(mul(f64::from(src.g[c]), y), f64::from(d[c])));
                    }
                    acc += st;
                }
                done += nseg;
                src.pos += (acc >> 14) as usize;
                src.frac = (acc & 0x3fff) as u32;
                if src.pos >= buf.frames {
                    src.pos -= buf.frames;
                    if src.cur + 1 < src.queue.len() {
                        src.cur += 1;
                    } else {
                        // Natural end: stopped, the running gains kept
                        // (@ 0x1002a8b8..0x1002a8fc).
                        src.state = State::Stopped;
                    }
                }
                if nseg == 0 && src.state == State::Playing && src.pos < buf.frames {
                    break;
                }
            }
        }
        out.reserve(2 * n);
        for d in dry.iter() {
            for &v in d {
                out.push(f64::from(v).floor().clamp(-32_768.0, 32_767.0) as i16);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_toward_zero_is_exact() {
        let a = 1.0;
        let b = f64::EPSILON / 4.0;
        assert_eq!(add(a, b), 1.0);
        assert_eq!(add(-a, -b), -1.0);
        assert_eq!(add(-a, b), toward_zero(-1.0) + 0.0);
        assert_eq!(add(a, -b), 1.0 - f64::EPSILON / 2.0);
        assert_eq!(mul(1.0 / 3.0, 3.0), 1.0 - f64::EPSILON / 2.0);
        assert_eq!(div(1.0, 3.0), 1.0 / 3.0);
        assert_eq!(div(1.0, 10.0), f64::from_bits(0x3fb9_9999_9999_9999));
        assert_eq!(div(-1.0, 10.0), -f64::from_bits(0x3fb9_9999_9999_9999));
        assert_eq!(f32z(1.0 - 1e-12), 1.0 - f32::EPSILON / 2.0);
        assert_eq!(f32z(-(1.0 - 1e-12)), -(1.0 - f32::EPSILON / 2.0));
        assert_eq!(pitch_step(44_100), 16_384);
        assert_eq!(pitch_step(22_050), 8_192);
        assert_eq!(pitch_step(48_000), 17_832);
        assert_eq!(pitch_step(32_000), 11_888);
        assert_eq!(pitch_step(11_025), 4_096);
    }
}

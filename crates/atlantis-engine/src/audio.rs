//! The sound mixer: the engine starts, stops and adjusts voices; the front
//! end pulls mixed samples at its device rate.
//!
//! Volumes and balance follow ScummVM's mixer, which the C++ port's sound
//! levels were written for: volume 0..255, balance -127 (left) .. 127
//! (right), the far side attenuated linearly; rate conversion by linear
//! interpolation (ScummVM's default converter).

use std::sync::{Arc, Mutex};

/// Decoded PCM: interleaved 16-bit samples.
#[derive(Clone, Debug)]
pub struct Pcm {
    pub rate: u32,
    pub channels: u16,
    pub samples: Arc<Vec<i16>>,
}

/// A playing sound, identified by the id `play` returns.
pub type VoiceId = u64;

/// Sound produced while it plays (the music's streaming ring): the mixer
/// pulls frames from it on the audio thread.
pub trait Source: Send {
    fn rate(&self) -> u32;
    fn channels(&self) -> u16;
    /// Fills `out` (interleaved, a whole number of frames); returns the
    /// frames written, fewer than asked at the end of the sound.
    fn read(&mut self, out: &mut [i16]) -> usize;
}

/// The frames a stream voice has pulled and not played yet.
struct Stream {
    src: Box<dyn Source>,
    buf: Vec<i16>,
    /// The frame index of `buf[0]`.
    base: u64,
    ended: bool,
}

impl Stream {
    const CHUNK: usize = 1024;

    /// Makes frame `i` available unless the stream has ended before it.
    fn ensure(&mut self, i: u64, ch: usize) {
        let have = |s: &Stream| s.base + (s.buf.len() / ch) as u64;
        // Drop what is played.
        if i > self.base + 4 * Self::CHUNK as u64 {
            let drop = ((i - self.base) as usize).min(self.buf.len() / ch);
            self.buf.drain(..drop * ch);
            self.base += drop as u64;
        }
        while !self.ended && have(self) <= i {
            let at = self.buf.len();
            self.buf.resize(at + Self::CHUNK * ch, 0);
            let got = self.src.read(&mut self.buf[at..]);
            self.buf.truncate(at + got.min(Self::CHUNK) * ch);
            if got < Self::CHUNK {
                self.ended = true;
            }
        }
    }

    fn frame(&self, i: u64, ch: usize) -> Option<&[i16]> {
        let k = i.checked_sub(self.base)? as usize;
        self.buf.get(k * ch..(k + 1) * ch)
    }
}

enum Data {
    Pcm(Pcm),
    Stream(Stream),
}

struct Voice {
    id: VoiceId,
    data: Data,
    pos: f64, // position in frames
    looping: bool,
    volume: u8,
    balance: i8,
}

#[derive(Default)]
struct State {
    voices: Vec<Voice>,
    next_id: VoiceId,
    master: u8,
    /// Silence, the voices kept where they are (the port's options menu).
    paused: bool,
}

/// Shared between the engine (control) and the audio thread (`fill`).
#[derive(Clone)]
pub struct Mixer(Arc<Mutex<State>>);

impl Default for Mixer {
    fn default() -> Self {
        Mixer(Arc::new(Mutex::new(State {
            master: 255,
            ..State::default()
        })))
    }
}

impl Mixer {
    pub fn new() -> Mixer {
        Mixer::default()
    }

    pub fn play(&self, pcm: Pcm, looping: bool, volume: u8, balance: i8) -> VoiceId {
        self.add(Data::Pcm(pcm), looping, volume, balance)
    }

    /// Plays a [`Source`] until it ends (or is stopped).
    pub fn play_source(&self, src: Box<dyn Source>, volume: u8, balance: i8) -> VoiceId {
        let stream = Stream {
            src,
            buf: Vec::new(),
            base: 0,
            ended: false,
        };
        self.add(Data::Stream(stream), false, volume, balance)
    }

    fn add(&self, data: Data, looping: bool, volume: u8, balance: i8) -> VoiceId {
        let mut s = self.0.lock().unwrap();
        s.next_id += 1;
        let id = s.next_id;
        s.voices.push(Voice {
            id,
            data,
            pos: 0.0,
            looping,
            volume,
            balance,
        });
        id
    }

    pub fn stop(&self, id: VoiceId) {
        self.0.lock().unwrap().voices.retain(|v| v.id != id);
    }

    pub fn stop_all(&self) {
        self.0.lock().unwrap().voices.clear();
    }

    pub fn is_playing(&self, id: VoiceId) -> bool {
        self.0.lock().unwrap().voices.iter().any(|v| v.id == id)
    }

    /// The number of voices playing.
    pub fn voice_count(&self) -> usize {
        self.0.lock().unwrap().voices.len()
    }

    /// A voice's volume and balance, `None` once it has ended.
    pub fn voice_volume(&self, id: VoiceId) -> Option<(u8, i8)> {
        let s = self.0.lock().unwrap();
        s.voices
            .iter()
            .find(|v| v.id == id)
            .map(|v| (v.volume, v.balance))
    }

    pub fn set_volume(&self, id: VoiceId, volume: u8, balance: i8) {
        if let Some(v) = self
            .0
            .lock()
            .unwrap()
            .voices
            .iter_mut()
            .find(|v| v.id == id)
        {
            v.volume = volume;
            v.balance = balance;
        }
    }

    pub fn set_master(&self, master: u8) {
        self.0.lock().unwrap().master = master;
    }

    /// Pauses (silence, nothing advances) or resumes every voice.
    pub fn set_paused(&self, paused: bool) {
        self.0.lock().unwrap().paused = paused;
    }

    /// Mixes into `out` (interleaved f32, `channels` per frame, 1 or 2) at
    /// `rate` Hz, replacing its content.
    pub fn fill(&self, out: &mut [f32], rate: u32, channels: usize) {
        out.fill(0.0);
        let mut s = self.0.lock().unwrap();
        if s.paused {
            return;
        }
        let master = f32::from(s.master) / 255.0;
        s.voices.retain_mut(|v| {
            let vol = f32::from(v.volume) / 255.0 * master;
            let b = i32::from(v.balance);
            let (lg, rg) = if b < 0 {
                (vol, vol * (127 + b) as f32 / 127.0)
            } else {
                (vol * (127 - b) as f32 / 127.0, vol)
            };
            match &mut v.data {
                Data::Pcm(pcm) => {
                    let src_ch = usize::from(pcm.channels.max(1));
                    let frames = pcm.samples.len() / src_ch;
                    if frames == 0 {
                        return false;
                    }
                    let step = f64::from(pcm.rate) / f64::from(rate);
                    for frame in out.chunks_mut(channels) {
                        if v.pos >= frames as f64 {
                            if !v.looping {
                                return false;
                            }
                            v.pos -= frames as f64;
                        }
                        let i = v.pos as usize;
                        let t = (v.pos - i as f64) as f32;
                        let j = if i + 1 < frames {
                            i + 1
                        } else if v.looping {
                            0
                        } else {
                            i
                        };
                        let sample = |frame: usize, c: usize| {
                            f32::from(pcm.samples[frame * src_ch + c.min(src_ch - 1)]) / 32768.0
                        };
                        let l = sample(i, 0) * (1.0 - t) + sample(j, 0) * t;
                        let r = sample(i, 1) * (1.0 - t) + sample(j, 1) * t;
                        mix(frame, channels, l * lg, r * rg);
                        v.pos += step;
                    }
                    true
                }
                Data::Stream(st) => {
                    let src_ch = usize::from(st.src.channels().max(1));
                    let step = f64::from(st.src.rate()) / f64::from(rate);
                    for frame in out.chunks_mut(channels) {
                        let i = v.pos as u64;
                        let t = (v.pos - i as f64) as f32;
                        st.ensure(i + 1, src_ch);
                        let Some(a) = st.frame(i, src_ch) else {
                            return false;
                        };
                        let (al, ar) = (a[0], a[src_ch - 1]);
                        let (bl, br) = st
                            .frame(i + 1, src_ch)
                            .map_or((al, ar), |b| (b[0], b[src_ch - 1]));
                        let l = (f32::from(al) * (1.0 - t) + f32::from(bl) * t) / 32768.0;
                        let r = (f32::from(ar) * (1.0 - t) + f32::from(br) * t) / 32768.0;
                        mix(frame, channels, l * lg, r * rg);
                        v.pos += step;
                    }
                    true
                }
            }
        });
        for x in out.iter_mut() {
            *x = x.clamp(-1.0, 1.0);
        }
    }
}

/// Adds one frame (left, right) to `frame` (1 or 2 channels).
fn mix(frame: &mut [f32], channels: usize, l: f32, r: f32) {
    if channels == 1 {
        frame[0] += (l + r) * 0.5;
    } else {
        frame[0] += l;
        frame[1] += r;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_ends_and_balance_attenuates_far_side() {
        let m = Mixer::new();
        let pcm = Pcm {
            rate: 100,
            channels: 1,
            samples: Arc::new(vec![16384; 10]),
        };
        let id = m.play(pcm, false, 255, 127);
        let mut out = vec![0.0; 2 * 8];
        m.fill(&mut out, 100, 2);
        assert_eq!(out[0], 0.0); // left silent at full right
        assert_eq!(out[1], 0.5);
        m.fill(&mut out, 100, 2);
        assert!(!m.is_playing(id));
    }

    struct Ramp(i16);

    impl Source for Ramp {
        fn rate(&self) -> u32 {
            100
        }
        fn channels(&self) -> u16 {
            2
        }
        fn read(&mut self, out: &mut [i16]) -> usize {
            let n = (out.len() / 2).min(3000 - self.0 as usize);
            for f in 0..n {
                out[2 * f] = self.0;
                out[2 * f + 1] = -self.0;
                self.0 += 1;
            }
            n
        }
    }

    #[test]
    fn stream_plays_to_its_end() {
        let m = Mixer::new();
        let id = m.play_source(Box::new(Ramp(0)), 255, 0);
        let mut out = vec![0.0; 2 * 2000];
        m.fill(&mut out, 100, 2);
        assert_eq!(out[2 * 1999], 1999.0 / 32768.0);
        assert_eq!(out[2 * 1999 + 1], -1999.0 / 32768.0);
        assert!(m.is_playing(id));
        m.fill(&mut out, 100, 2);
        assert!(!m.is_playing(id));
    }
}

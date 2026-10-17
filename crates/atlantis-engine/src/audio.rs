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

struct Voice {
    id: VoiceId,
    pcm: Pcm,
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
}

/// Shared between the engine (control) and the audio thread (`fill`).
#[derive(Clone)]
pub struct Mixer(Arc<Mutex<State>>);

impl Default for Mixer {
    fn default() -> Self {
        Mixer(Arc::new(Mutex::new(State { master: 255, ..State::default() })))
    }
}

impl Mixer {
    pub fn new() -> Mixer {
        Mixer::default()
    }

    pub fn play(&self, pcm: Pcm, looping: bool, volume: u8, balance: i8) -> VoiceId {
        let mut s = self.0.lock().unwrap();
        s.next_id += 1;
        let id = s.next_id;
        s.voices.push(Voice { id, pcm, pos: 0.0, looping, volume, balance });
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

    pub fn set_volume(&self, id: VoiceId, volume: u8, balance: i8) {
        if let Some(v) = self.0.lock().unwrap().voices.iter_mut().find(|v| v.id == id) {
            v.volume = volume;
            v.balance = balance;
        }
    }

    pub fn set_master(&self, master: u8) {
        self.0.lock().unwrap().master = master;
    }

    /// Mixes into `out` (interleaved f32, `channels` per frame, 1 or 2) at
    /// `rate` Hz, replacing its content.
    pub fn fill(&self, out: &mut [f32], rate: u32, channels: usize) {
        out.fill(0.0);
        let mut s = self.0.lock().unwrap();
        let master = f32::from(s.master) / 255.0;
        s.voices.retain_mut(|v| {
            let src_ch = usize::from(v.pcm.channels.max(1));
            let frames = v.pcm.samples.len() / src_ch;
            if frames == 0 {
                return false;
            }
            let step = f64::from(v.pcm.rate) / f64::from(rate);
            let vol = f32::from(v.volume) / 255.0 * master;
            let b = i32::from(v.balance);
            let (lg, rg) = if b < 0 {
                (vol, vol * (127 + b) as f32 / 127.0)
            } else {
                (vol * (127 - b) as f32 / 127.0, vol)
            };
            for frame in out.chunks_mut(channels) {
                if v.pos >= frames as f64 {
                    if !v.looping {
                        return false;
                    }
                    v.pos -= frames as f64;
                }
                let i = v.pos as usize;
                let t = (v.pos - i as f64) as f32;
                let j = if i + 1 < frames { i + 1 } else if v.looping { 0 } else { i };
                let sample = |frame: usize, c: usize| {
                    f32::from(v.pcm.samples[frame * src_ch + c.min(src_ch - 1)]) / 32768.0
                };
                let l = sample(i, 0) * (1.0 - t) + sample(j, 0) * t;
                let r = sample(i, 1) * (1.0 - t) + sample(j, 1) * t;
                if channels == 1 {
                    frame[0] += (l * lg + r * rg) * 0.5;
                } else {
                    frame[0] += l * lg;
                    frame[1] += r * rg;
                }
                v.pos += step;
            }
            true
        });
        for x in out.iter_mut() {
            *x = x.clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_ends_and_balance_attenuates_far_side() {
        let m = Mixer::new();
        let pcm = Pcm { rate: 100, channels: 1, samples: Arc::new(vec![16384; 10]) };
        let id = m.play(pcm, false, 255, 127);
        let mut out = vec![0.0; 2 * 8];
        m.fill(&mut out, 100, 2);
        assert_eq!(out[0], 0.0); // left silent at full right
        assert_eq!(out[1], 0.5);
        m.fill(&mut out, 100, 2);
        assert!(!m.is_playing(id));
    }
}

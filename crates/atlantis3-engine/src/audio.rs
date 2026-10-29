//! The sound the player hears: the port's mix of the original's outputs.
//!
//! The original plays a cinematic's sound through `waveOut` at the file's
//! own format (44100 Hz, 16 bits, stereo: CryoMovie @ 0x006b9390), one
//! buffer per frame, and its other sounds through the Cryo Sound Library
//! (DirectSound). The port sums them here: each channel is a queue of
//! interleaved 16-bit samples at its own rate, read by the output at the
//! device's rate (linear interpolation), with a gain per side.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// The channel of the cinematics' sound.
pub const CHANNEL_CINEMA: usize = 0;
/// The channel of `playsound`.
pub const CHANNEL_SOUND: usize = 1;
/// The channel of the music.
pub const CHANNEL_MUSIC: usize = 2;
/// The first channel of the place sounds; they take the next ones up to
/// [`CHANNELS`].
pub const CHANNEL_PLACE: usize = 3;
/// The channel of the dialogs' voices (the place sounds stop before it).
pub const CHANNEL_VOICE: usize = 15;
/// The first channel of the C code's sound objects (`LoadSfx` / `Play`
/// @ 0x004329e0 / 0x00433140: the puzzles' sounds and the `PLAYVOICE`
/// voices), [`OBJECT_CHANNELS`] of them.
pub const CHANNEL_OBJECTS: usize = 16;
pub const OBJECT_CHANNELS: usize = 6;
/// How many channels the mix has.
pub const CHANNELS: usize = CHANNEL_OBJECTS + OBJECT_CHANNELS;

#[derive(Clone, Debug, Default)]
struct Channel {
    rate: u32,
    /// 1 or 2.
    channels: u16,
    /// Samples queued (a stream), or the whole sound (a looping one).
    queue: VecDeque<i16>,
    /// A looping sound keeps its samples to start again.
    looped: Option<Arc<Vec<i16>>>,
    /// Position in source frames within the queue's first frame.
    pos: f64,
    gain: [f32; 2],
    /// Source frames played since the channel started.
    played: u64,
}

#[derive(Debug, Default)]
struct Mix {
    channels: Vec<Option<Channel>>,
    paused: bool,
}

/// The mix, shared between the engine and the sound output.
#[derive(Clone, Debug, Default)]
pub struct Audio(Arc<Mutex<Mix>>);

impl Audio {
    pub fn new() -> Audio {
        let a = Audio::default();
        a.lock().channels = vec![None; CHANNELS];
        a
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Mix> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Starts a stream on `channel` (replacing what it played): samples
    /// are added with [`Audio::push`].
    pub fn start_stream(&self, channel: usize, rate: u32, channels: u16, gain: f32) {
        self.put(
            channel,
            Channel {
                rate,
                channels: channels.max(1),
                gain: [gain; 2],
                ..Channel::default()
            },
        );
    }

    /// Adds samples to a stream.
    pub fn push(&self, channel: usize, samples: &[i16]) {
        if let Some(Some(c)) = self.lock().channels.get_mut(channel) {
            c.queue.extend(samples.iter().copied());
        }
    }

    /// Plays a whole sound on `channel`, once or looping, at gains (left,
    /// right).
    pub fn play(
        &self,
        channel: usize,
        rate: u32,
        channels: u16,
        samples: Arc<Vec<i16>>,
        looping: bool,
        gain: [f32; 2],
    ) {
        self.put(
            channel,
            Channel {
                rate,
                channels: channels.max(1),
                queue: samples.iter().copied().collect(),
                looped: looping.then_some(samples),
                gain,
                ..Channel::default()
            },
        );
    }

    fn put(&self, channel: usize, c: Channel) {
        let mut m = self.lock();
        if m.channels.len() < CHANNELS {
            m.channels.resize(CHANNELS, None);
        }
        if let Some(slot) = m.channels.get_mut(channel) {
            *slot = Some(c);
        }
    }

    /// Changes a channel's gains.
    pub fn set_gain(&self, channel: usize, gain: [f32; 2]) {
        if let Some(Some(c)) = self.lock().channels.get_mut(channel) {
            c.gain = gain;
        }
    }

    pub fn stop(&self, channel: usize) {
        if let Some(slot) = self.lock().channels.get_mut(channel) {
            *slot = None;
        }
    }

    pub fn stop_all(&self) {
        for slot in self.lock().channels.iter_mut() {
            *slot = None;
        }
    }

    /// Whether a channel still has something to play.
    pub fn is_playing(&self, channel: usize) -> bool {
        matches!(self.lock().channels.get(channel), Some(Some(c)) if !c.queue.is_empty() || c.looped.is_some())
    }

    /// The source frames a channel has played.
    pub fn played(&self, channel: usize) -> Option<u64> {
        match self.lock().channels.get(channel) {
            Some(Some(c)) => Some(c.played),
            _ => None,
        }
    }

    /// Silences the output while the game is held (its channels keep
    /// their place).
    pub fn set_paused(&self, paused: bool) {
        self.lock().paused = paused;
    }

    /// Fills `out` (interleaved, `channels` per frame) at `rate` Hz.
    pub fn fill(&self, out: &mut [f32], rate: u32, channels: usize) {
        out.fill(0.0);
        let channels = channels.max(1);
        let mut m = self.lock();
        if m.paused || rate == 0 {
            return;
        }
        for slot in m.channels.iter_mut() {
            let Some(c) = slot else { continue };
            let ch = usize::from(c.channels);
            let step = f64::from(c.rate) / f64::from(rate);
            for frame in out.chunks_mut(channels) {
                if c.queue.len() < ch {
                    match &c.looped {
                        Some(all) if !all.is_empty() => c.queue.extend(all.iter().copied()),
                        _ => break,
                    }
                }
                let t = c.pos as f32;
                let get = |k: usize, s: usize| -> f32 {
                    let i = k * ch + s.min(ch - 1);
                    f32::from(
                        c.queue
                            .get(i)
                            .or_else(|| c.queue.get(s.min(ch - 1)))
                            .copied()
                            .unwrap_or(0),
                    )
                };
                let l = get(0, 0) * (1.0 - t) + get(1, 0) * t;
                let r = get(0, 1) * (1.0 - t) + get(1, 1) * t;
                let (l, r) = (l / 32768.0 * c.gain[0], r / 32768.0 * c.gain[1]);
                if channels == 1 {
                    frame[0] += (l + r) * 0.5;
                } else {
                    frame[0] += l;
                    frame[1] += r;
                }
                c.pos += step;
                while c.pos >= 1.0 && c.queue.len() >= ch {
                    c.pos -= 1.0;
                    c.queue.drain(..ch);
                    c.played += 1;
                }
            }
        }
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}

/// The Cryo Sound Manager's volume (0..100) as a gain: hundredths of dB
/// `trunc(−2000 · log10(100 / v))` (0 for 100, −10000 for 0), as
/// DirectSound applies them (`Game.exe` @ 0x006b0600 → @ 0x006b0540; the
/// object's scale +0x48 is not read yet and taken as 100).
pub fn csm_gain(v: i32) -> f32 {
    if v <= 0 {
        return 0.0;
    }
    let db = (-2000.0 * (100.0 / f64::from(v.min(100))).log10()) as i32;
    10f32.powf(db.max(-10000) as f32 / 2000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_plays_at_its_rate() {
        let a = Audio::new();
        a.start_stream(CHANNEL_CINEMA, 44100, 2, 1.0);
        a.push(CHANNEL_CINEMA, &vec![16384i16; 2 * 4410]);
        let mut out = vec![0f32; 2 * 2205];
        a.fill(&mut out, 22050, 2);
        assert!((out[0] - 0.5).abs() < 1e-6);
        assert_eq!(a.played(CHANNEL_CINEMA), Some(4410));
        assert!(!a.is_playing(CHANNEL_CINEMA));
    }

    #[test]
    fn a_loop_starts_again() {
        let a = Audio::new();
        a.play(
            CHANNEL_SOUND,
            100,
            1,
            Arc::new(vec![100; 10]),
            true,
            [1.0, 1.0],
        );
        let mut out = vec![0f32; 50];
        a.fill(&mut out, 100, 1);
        assert!(out.iter().all(|&s| s > 0.0));
        assert!(a.is_playing(CHANNEL_SOUND));
    }
}

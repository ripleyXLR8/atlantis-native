//! The sound levels and the voices: the volume → attenuation table, the
//! music's volume fades (the dialogs duck the music to 16 %), and a small
//! mixer standing for DirectSound's.
//!
//! The original does not mix: every sound is a DirectSound secondary
//! buffer on one of 12 "voices" (0 the music, 1 the dialog voice, 2 the
//! effects and cinematic tracks, 3..10 the place sounds), played at an
//! attenuation in hundredths of dB. [`Voices`] sums them as DirectSound
//! does by its documentation: gain 10^(attenuation / 2000), the pan law
//! attenuating one side, linear resampling to the device rate.

use std::sync::{Arc, Mutex};

use crate::fpu::{ftol, Fpu};

/// The attenuation table (@ 0x0047ac20, at 0x00672f48): hundredths of dB
/// for the volumes 0..127, trunc(−0.5 · e^((127 − i) / 13.6)) clamped to
/// −10000..0, entry 0 forced to −10000. The x87 code computes e^x as
/// 2^(x · log2 e) with double constants; the table is the same under every
/// control word (`sound_volume.txt`).
pub fn attenuation_table() -> [i32; 128] {
    let mut att = [0i32; 128];
    for (i, a) in att.iter_mut().enumerate() {
        // 0.07352941176470588 @ 0x005c66c0, FLDL2E, F2XM1 / FSCALE, ×−0.5.
        let x = (127 - i as i32) as f64 * 0.073_529_411_764_705_88 * std::f64::consts::LOG2_E;
        let v = ftol(-0.5 * libm::exp2(x));
        *a = v.clamp(-10000, 0);
    }
    att[0] = -10000;
    att
}

/// The pan table built with the attenuation table (@ 0x0047ac20, at
/// 0x006733b4): hundredths of dB for the pans 0..127, trunc((i − 63) ×
/// 158.0) clamped to −10000..10000 (`sound_volume.txt`).
pub fn pan_table() -> [i32; 128] {
    let mut pan = [0i32; 128];
    for (i, p) in pan.iter_mut().enumerate() {
        // FILD (i − 0x3f), FMUL double 158.0 @ 0x005c66b0, _ftol.
        *p = ftol((i as i32 - 63) as f64 * 158.0).clamp(-10000, 10000);
    }
    pan
}

/// The levels before `atlant2.cfg` is read: the master volume (@
/// 0x005dd2b8), the effects (@ 0x005dd2bc), the music (@ 0x005dc9dc) and
/// the voices (@ 0x005df974), as `sound_volume.txt` reads them.
pub const DEFAULT_MASTER: i32 = 256;
pub const DEFAULT_FX_VOLUME: i32 = 127;
pub const DEFAULT_MUSIC_VOLUME: i32 = 100;
pub const DEFAULT_VOICE_VOLUME: i32 = 127;

/// `FUN_0047acf0(v)` with the master volume `master` (0..256, @
/// 0x005dd2b8): the attenuation of volume `v`, index (master · v) / 256
/// toward zero, at most 127, a negative index giving entry 0.
pub fn volume_to_attenuation(att: &[i32; 128], master: i32, v: i32) -> i32 {
    let idx = master.wrapping_mul(v) / 256;
    if idx < 0 {
        att[0]
    } else {
        att[idx.min(127) as usize]
    }
}

/// The music's volume and fade (@ 0x00672184.. 0x006725c0;
/// `FUN_004770c0` sets a percentage, `FUN_00476e30` arms a fade,
/// `FUN_00476e80` steps it on the 15 Hz tick).
#[derive(Clone, Debug, PartialEq)]
pub struct MusicVolume {
    fpu: Fpu,
    att: [i32; 128],
    /// Master volume 0..256.
    pub master: i32,
    /// The music volume of the options, 0..127 (`GetMaxVol`, @
    /// 0x005dc9dc; 100 without `atlant2.cfg`).
    pub max: i32,
    /// The current volume (@ 0x0067218c).
    pub volume: i32,
    /// The player's state (@ 0x00672184): 0 stopped, 1 / 2 open pending,
    /// 3 fading, 4 playing, 5 stop pending.
    pub state: i32,
    /// The state after the fade (@ 0x00672188).
    pub next: i32,
    /// The fade's target (@ 0x00672190) and the copy the step reads (@
    /// 0x006725bc).
    pub target: i32,
    pub fade_target: i32,
    /// The volume a music switch fades back up to (@ 0x00672194).
    pub after: i32,
    pub start_volume: i32,
    /// The fade's start time, a float, −1 until the first step (@
    /// 0x006725b4).
    pub start_time: f32,
    /// The fade's length in seconds, a float (@ 0x006725c0).
    pub duration: f32,
    /// The dialog flag of the player (@ 0x006725c8, `FUN_00477580`): a
    /// music started during a dialog starts at volume 14.
    pub in_dialog: bool,
}

impl MusicVolume {
    /// A playing music at volume `volume` of `max`.
    pub fn new(fpu: Fpu, max: i32, volume: i32, state: i32) -> MusicVolume {
        MusicVolume {
            fpu,
            att: attenuation_table(),
            master: DEFAULT_MASTER,
            max,
            volume,
            state,
            next: 0,
            target: 0,
            fade_target: 0,
            after: 0,
            start_volume: 0,
            start_time: -1.0,
            duration: 0.0,
            in_dialog: false,
        }
    }

    /// The attenuation of the current volume.
    pub fn attenuation(&self) -> i32 {
        volume_to_attenuation(&self.att, self.master, self.volume)
    }

    /// `FUN_00476e30`: a fade from the current volume to the target at 127
    /// volume units a second.
    fn arm(&mut self) {
        let f = self.fpu;
        self.fade_target = self.target;
        self.state = 3;
        self.start_time = -1.0;
        self.start_volume = self.volume;
        // FILD |target − volume|, FDIV double 127.0, FSTP dword.
        self.duration = f.f32(f.div(f64::from((self.target - self.volume).abs()), 127.0));
    }

    /// `FUN_004770c0(pct)`: fade to `pct` % of the music volume (only
    /// while a music plays: the sound card, a music voice and a state are
    /// checked; an open still pending, states 1 and 2, has no voice yet).
    pub fn set_percent(&mut self, pct: i32) {
        if matches!(self.state, 0..=2) {
            return;
        }
        let pct = pct.clamp(0, 100);
        if self.state != 3 {
            self.next = 4;
        }
        self.target = self.max * pct / 100;
        self.arm();
    }

    /// `stopmusik` (`FUN_00477090`): fade to 0, then stop.
    pub fn stop(&mut self) {
        self.next = 5;
        self.target = 0;
        self.arm();
    }

    /// Another music while one plays (`FUN_00477dc0`): fade to 0, then
    /// state `next` (1 from the start, 2 at a mark), back up to the music
    /// volume.
    pub fn switch(&mut self, next: i32) {
        self.next = next;
        self.target = 0;
        self.after = self.max;
        self.arm();
    }

    /// `FUN_00476e80` at the scheduler's clock (seconds): one fade step.
    /// Returns the attenuation given to the music voice when the volume
    /// changed.
    pub fn step(&mut self, clock: f64) -> Option<i32> {
        let f = self.fpu;
        if self.state != 3 {
            return None;
        }
        if self.start_time < 0.0 {
            self.start_time = f.f32(clock);
        }
        let e = f.sub(clock, f64::from(self.start_time));
        let ef = f.f32(e);
        if e > 2.0 {
            // A stall: the fade starts again from where it is.
            self.arm();
            return None;
        }
        if self.volume == self.target {
            if self.next == 1 || self.next == 2 {
                // FUN_004778f0 stops the old music first.
                self.target = self.after;
            }
            self.state = self.next;
            return None;
        }
        let delta = self.fade_target - self.start_volume;
        let mut v = ftol(f.mul(
            f64::from(delta),
            f.div(f64::from(ef), f64::from(self.duration)),
        )) + self.start_volume;
        if (self.fade_target < v && self.start_volume < self.fade_target)
            || (v < self.fade_target && self.fade_target < self.start_volume)
        {
            v = self.fade_target;
        }
        if v != self.volume {
            self.volume = v;
            return Some(self.attenuation());
        }
        None
    }
}

/// DirectSound's gain for an attenuation in hundredths of dB.
pub fn gain(attenuation: i32) -> f32 {
    if attenuation <= -10000 {
        0.0
    } else {
        10f32.powf(attenuation as f32 / 2000.0)
    }
}

/// Decoded sound: interleaved 16-bit samples.
#[derive(Clone, Debug)]
pub struct Pcm {
    pub rate: u32,
    pub channels: u16,
    pub samples: Arc<Vec<i16>>,
}

#[derive(Clone, Debug)]
struct Voice {
    pcm: Pcm,
    /// Position in source frames.
    pos: f64,
    looping: bool,
    attenuation: i32,
    pan: i32,
}

/// The 12 voices, shared between the engine and the sound output.
#[derive(Clone, Debug, Default)]
pub struct Voices(Arc<Mutex<Vec<Option<Voice>>>>);

/// The original's voice numbers.
pub const VOICE_MUSIC: usize = 0;
pub const VOICE_DIALOG: usize = 1;
pub const VOICE_EFFECTS: usize = 2;

impl Voices {
    pub fn new() -> Voices {
        Voices(Arc::new(Mutex::new(vec![None; 12])))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Option<Voice>>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Plays `pcm` on `voice` (stopping what it played), at `attenuation`
    /// and `pan` (hundredths of dB).
    pub fn play(&self, voice: usize, pcm: Pcm, looping: bool, attenuation: i32, pan: i32) {
        if let Some(v) = self.lock().get_mut(voice) {
            *v = Some(Voice {
                pcm,
                pos: 0.0,
                looping,
                attenuation,
                pan,
            });
        }
    }

    pub fn stop(&self, voice: usize) {
        if let Some(v) = self.lock().get_mut(voice) {
            *v = None;
        }
    }

    pub fn set_attenuation(&self, voice: usize, attenuation: i32) {
        if let Some(Some(v)) = self.lock().get_mut(voice) {
            v.attenuation = attenuation;
        }
    }

    pub fn is_playing(&self, voice: usize) -> bool {
        matches!(self.lock().get(voice), Some(Some(_)))
    }

    /// Voice 0 as the music player's ring ([`crate::music::Ring`]).
    pub fn music_ring(&self) -> MusicRing<'_> {
        MusicRing(self)
    }

    /// Mixes every voice into `out` (interleaved f32, `channels` per
    /// frame, at `rate`), replacing its content.
    pub fn fill(&self, out: &mut [f32], rate: u32, channels: usize) {
        out.fill(0.0);
        if channels == 0 || rate == 0 {
            return;
        }
        let mut voices = self.lock();
        for slot in voices.iter_mut() {
            let Some(v) = slot else { continue };
            let g = gain(v.attenuation);
            // pan > 0 attenuates the left channel, < 0 the right one.
            let (gl, gr) = (g * gain(-v.pan.max(0)), g * gain(v.pan.min(0)));
            let ch = usize::from(v.pcm.channels.max(1));
            let frames = v.pcm.samples.len() / ch;
            let step = f64::from(v.pcm.rate) / f64::from(rate);
            let mut ended = false;
            for frame in out.chunks_exact_mut(channels) {
                if v.pos >= frames as f64 {
                    if v.looping && frames > 0 {
                        v.pos -= frames as f64;
                    } else {
                        ended = true;
                        break;
                    }
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
                let s = |k: usize, c: usize| f32::from(v.pcm.samples[k * ch + c.min(ch - 1)]);
                let l = (s(i, 0) * (1.0 - t) + s(j, 0) * t) / 32768.0;
                let r = (s(i, 1) * (1.0 - t) + s(j, 1) * t) / 32768.0;
                if channels == 1 {
                    frame[0] += 0.5 * (l * gl + r * gr);
                } else {
                    frame[0] += l * gl;
                    frame[1] += r * gr;
                }
                v.pos += step;
            }
            if ended {
                *slot = None;
            }
        }
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}

/// Voice 0's buffer as the music ring: its play cursor in bytes (the
/// frame under the position), and its samples written in place.
pub struct MusicRing<'a>(&'a Voices);

impl crate::music::Ring for MusicRing<'_> {
    fn cursor(&self) -> Option<i32> {
        let voices = self.0.lock();
        let v = voices.get(VOICE_MUSIC)?.as_ref()?;
        let ch = i32::from(v.pcm.channels.max(1));
        Some(v.pos as i32 * ch * 2)
    }

    fn set_cursor(&mut self, at: i32) {
        if let Some(Some(v)) = self.0.lock().get_mut(VOICE_MUSIC) {
            let ch = i32::from(v.pcm.channels.max(1));
            v.pos = f64::from(at / (ch * 2));
        }
    }

    fn write(&mut self, at: i32, samples: &[i16]) {
        if let Some(Some(v)) = self.0.lock().get_mut(VOICE_MUSIC) {
            let buf = Arc::make_mut(&mut v.pcm.samples);
            let start = (at.max(0) / 2) as usize;
            let end = (start + samples.len()).min(buf.len());
            if start < end {
                buf[start..end].copy_from_slice(&samples[..end - start]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_points() {
        let att = attenuation_table();
        // sound.md's table of points.
        for (v, a) in [
            (0, -10000),
            (1, -5279),
            (14, -2029),
            (16, -1752),
            (20, -1305),
            (50, -143),
            (100, -3),
            (127, 0),
        ] {
            assert_eq!(att[v], a, "volume {v}");
        }
        assert_eq!(volume_to_attenuation(&att, 256, -3), -10000);
        assert_eq!(volume_to_attenuation(&att, 256, 300), 0);
        assert_eq!(volume_to_attenuation(&att, 128, 100), att[50]);
    }

    #[test]
    fn dialog_duck_reaches_16() {
        let mut m = MusicVolume::new(Fpu::Double, 100, 100, 4);
        m.set_percent(16);
        let mut t = 10.0;
        while m.state == 3 {
            m.step(t);
            t += 1.0 / 15.0;
        }
        assert_eq!((m.volume, m.state), (16, 4));
    }

    #[test]
    fn mixer_plays_once_or_loops() {
        let v = Voices::new();
        let pcm = Pcm {
            rate: 22050,
            channels: 1,
            samples: Arc::new(vec![16384; 4]),
        };
        v.play(VOICE_DIALOG, pcm.clone(), false, 0, 0);
        let mut out = vec![0.0; 2 * 8];
        v.fill(&mut out, 22050, 2);
        assert_eq!(out[0], 0.5);
        assert_eq!(out[2 * 7], 0.0);
        assert!(!v.is_playing(VOICE_DIALOG));
        v.play(VOICE_MUSIC, pcm, true, -10000, 0);
        v.fill(&mut out, 22050, 2);
        assert!(out.iter().all(|&s| s == 0.0));
        assert!(v.is_playing(VOICE_MUSIC));
    }
}

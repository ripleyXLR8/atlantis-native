//! The music (CON StartMusik / StopMusik / LoadMusik / DelMusik), its fades,
//! and the NewSound sample.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`music.cpp`,
//! `music_ring.cpp`, `music_fade.h`, the NewSound poller of `con_exec.cpp`;
//! GPLv3, the ScummVM team).
//!
//! The original's 200 Hz timer callback `FUN_00431150` moves the music
//! volume and the NewSound fade-out; here it runs as a [`Sys`] timer, once
//! per engine step for every 5 ms elapsed, whatever loop the engine is in.
//!
//! The tracks play either looped cleanly (the default, as ScummVM with its
//! minor bug fixes; `BugFix::MusicLoop`) or through the original's streaming ring
//! ([`MusicRing`]), whose stale fragments are heard where the sound
//! driver's play position runs ahead of the refills.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_codecs::decode_apc;

use crate::audio::{Mixer, Pcm, Source, VoiceId};
use crate::con_script::{con_music_apply, con_music_list, con_music_load_all_or_nothing};
use crate::engine::Engine;
use crate::persist::Settings;
use crate::sys::Sys;

/// The music volume of the original and its fades, moved by the 200 Hz
/// timer callback `FUN_00431150`: the tick 0x006ce118 + 1, then
/// `FUN_0042f7a8` (down) and `FUN_0042f974` (up): one volume step (0..127)
/// every 5 ticks (25 ms). Field names give the original's variables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicFade {
    /// 0x00495e71: the music sample's volume.
    pub cur: u8,
    /// 0x00495eac: 2 fade down, 1 fade up, 0 idle.
    pub mode: u8,
    /// 0x00495e6c: -1 until the fade down starts.
    pub down_target: i32,
    /// 0x006ce124.
    pub up_target: i32,
    /// 0x00495ea4: the fade-down percentage (80 by default).
    pub percent: u8,
    /// 0x00495e72: the music volume (sound menu).
    pub vol: u8,
    /// 0x006ce118.
    pub ticks: u32,
    /// 0x006ce0f4.
    pub last_step: u32,
    /// 0x00495eb0: 2 when the NewSound sample 0x006ce110 fades out after a
    /// cut-short video (`FUN_0042f7a8`'s second part).
    pub sfx_mode: u8,
    /// 0x006ce128.
    pub sfx_vol: i32,
    /// Its start, 0x00495e73 (the effects volume).
    pub sfx_vol0: i32,
    /// 0x006ce104.
    pub sfx_last: u32,
    /// Set by `tick`: apply `sfx_vol`.
    pub sfx_changed: bool,
    /// Set by `tick`: stop the sample.
    pub sfx_stop: bool,
}

impl Default for MusicFade {
    fn default() -> Self {
        MusicFade {
            // The data segment's initial byte (0x00495e71, atlantis-cd-fr-688640
            // 0x00485e71: 0x6e in every build); each track's start sets 0.
            cur: 0x6e,
            mode: 0,
            down_target: -1,
            up_target: 0,
            percent: 80,
            vol: 127,
            ticks: 0,
            last_step: 0,
            sfx_mode: 0,
            sfx_vol: 0,
            sfx_vol0: 127,
            sfx_last: 0,
            sfx_changed: false,
            sfx_stop: false,
        }
    }
}

impl MusicFade {
    /// One tick of `FUN_00431150`.
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        // FUN_0042f7a8: the target is computed on the first tick; already
        // reached, the fade ends without a step and the function returns
        // (@ 0x0042f832..0x0042f843, jump to the epilogue 0x0042f96b), so
        // the NewSound fade-out below is not processed on that tick.
        // FUN_0042f974 (the fade up) is a separate call of FUN_00431150
        // (@ 0x0043117d) and still runs.
        let mut done = false;
        if self.mode == 2 {
            if self.down_target == -1 {
                self.up_target = i32::from(self.vol);
                self.down_target = (i32::from(self.vol)
                    - i32::from(self.vol) * i32::from(self.percent) / 100)
                    .max(0);
                if i32::from(self.cur) <= self.down_target {
                    self.down_target = -1;
                    self.mode = 0;
                    done = true;
                }
            }
            if !done {
                if self.ticks.wrapping_sub(self.last_step) > 4 {
                    self.last_step = self.ticks;
                    self.cur = self.cur.wrapping_sub(1);
                }
                if i32::from(self.cur) <= self.down_target {
                    self.mode = 0;
                    self.down_target = -1;
                }
            }
        }
        // The NewSound fade-out (@ 0x0042f89d..0x0042f961).
        if !done && self.sfx_mode == 2 {
            if self.ticks.wrapping_sub(self.sfx_last) > 4 {
                self.sfx_last = self.ticks;
                self.sfx_vol = (self.sfx_vol - 2).max(0);
                self.sfx_changed = true;
            }
            if self.sfx_vol < 1 {
                self.sfx_stop = true;
                self.sfx_mode = 0;
            }
        }
        // FUN_0042f974: the step comes before the test, so a fade that
        // starts at its target still goes one step above it.
        if self.mode == 1 {
            if self.ticks.wrapping_sub(self.last_step) > 4 {
                self.last_step = self.ticks;
                self.cur = self.cur.wrapping_add(1);
            }
            if self.up_target <= i32::from(self.cur) {
                self.mode = 0;
            }
        }
    }
}

/// The standard IMA tables (0x00490050, 0x004901c0).
const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const INDEX_ADJUST: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

/// The original's music streaming ring (`FUN_00432ea8` start,
/// `FUN_0043318c` refill, `FUN_00433310` release, `FUN_004101ce` decode,
/// `FUN_00417dd0` header): an 8-block ring of 0x80000 bytes that the sound
/// driver loops over, each block refilled from the file once the timer has
/// seen the play position move 0x10000 bytes past the last mark. What is
/// heard depends on the position the driver reports; the law measured
/// under Wine (the play position + 0x8000, rounded down to 0x4000) is the
/// default. Memory is the ring followed by the 0x20000-byte area of the
/// block flags, as in the original (0x0062c530, 0x006ac530), so a refill
/// that runs past the ring end writes into the flags as it does there.
pub struct MusicRing {
    file: Arc<Vec<u8>>,
    file_pos: u32,
    ahead: u32,
    gran: u32,
    /// The ring and the flag area.
    mem: Vec<u8>,
    /// [0x006ce0d4].
    buf: [u8; MusicRing::BLOCK_BYTES as usize],
    /// The decoder's data offset in `buf`.
    src: u32,
    total: u32,
    remaining: u32,
    rate: u32,
    stereo: bool,
    pred: [i32; 2],
    index: [i32; 2],
    /// 0x006ce0dc - ring.
    write: u32,
    /// 0x006ce0c8.
    refill_block: u32,
    /// 0x006ce0d8.
    release_block: u32,
    /// 0x006ce0b0.
    mark: u32,
    /// 0x00495ea8.
    on: bool,
    /// Bytes the sample has played.
    play: u64,
}

impl MusicRing {
    pub const RING_SIZE: u32 = 0x80000;
    pub const FLAGS_SIZE: u32 = 0x20000;
    /// File bytes per block.
    pub const BLOCK_BYTES: u32 = 0x4000;

    /// `ahead` / `granularity`: the reported position law
    /// (AIL_sample_position).
    pub fn new(file: Arc<Vec<u8>>, ahead: u32, granularity: u32) -> MusicRing {
        MusicRing {
            file,
            file_pos: 0,
            ahead,
            gran: granularity.max(1),
            mem: vec![0; (Self::RING_SIZE + Self::FLAGS_SIZE) as usize],
            buf: [0; Self::BLOCK_BYTES as usize],
            src: 0,
            total: 0,
            remaining: 0,
            rate: 22050,
            stereo: true,
            pred: [0; 2],
            index: [0; 2],
            write: 0,
            refill_block: 0,
            release_block: 0,
            mark: 0,
            on: false,
            play: 0,
        }
    }

    /// With the position law measured under Wine.
    pub fn wine(file: Arc<Vec<u8>>) -> MusicRing {
        MusicRing::new(file, 0x8000, 0x4000)
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn header_frames(&self) -> u32 {
        self.total
    }

    pub fn played_bytes(&self) -> u64 {
        self.play
    }

    fn flag(&self, i: u32) -> u32 {
        let o = (Self::RING_SIZE + 4 * i) as usize;
        u32::from_le_bytes([
            self.mem[o],
            self.mem[o + 1],
            self.mem[o + 2],
            self.mem[o + 3],
        ])
    }

    fn set_flag(&mut self, i: u32, v: u32) {
        let o = (Self::RING_SIZE + 4 * i) as usize;
        self.mem[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// ReadFile: what is left of the file; the rest of the buffer keeps its
    /// previous bytes.
    fn read_file(&mut self, n: u32) -> u32 {
        let size = self.file.len() as u32;
        let got = if self.file_pos < size {
            n.min(size - self.file_pos)
        } else {
            0
        };
        let at = self.file_pos as usize;
        self.buf[..got as usize].copy_from_slice(&self.file[at..at + got as usize]);
        self.file_pos += got;
        got
    }

    /// `FUN_00417dd0`: "CRYO_APC", data at +0x20, rate +0x10, frames +0x0c,
    /// predictors +0x14 / +0x18, step indexes 0, stereo flag +0x1c bit 0.
    fn header(&mut self) -> bool {
        if &self.buf[..8] != b"CRYO_APC" {
            return false;
        }
        let le = |o: usize| {
            u32::from_le_bytes([
                self.buf[o],
                self.buf[o + 1],
                self.buf[o + 2],
                self.buf[o + 3],
            ])
        };
        self.src = 0x20;
        self.total = le(0x0c);
        self.remaining = self.total;
        self.rate = le(0x10);
        self.pred = [le(0x14) as i32, le(0x18) as i32];
        self.index = [0, 0];
        self.stereo = le(0x1c) & 1 != 0;
        true
    }

    /// `FUN_004101ce`: `count` frames, at most the frames left; high nibble
    /// first (left, then right in stereo); 32-bit predictor, never clamped,
    /// stored as int16.
    fn decode(&mut self, count: u32) {
        let n = count.min(self.remaining);
        self.remaining -= n;
        let channels = if self.stereo { 2 } else { 1 };
        for _ in 0..n {
            let b = self.buf.get(self.src as usize).copied().unwrap_or(0);
            self.src += 1;
            for (k, c) in [b >> 4, b & 0xf].into_iter().enumerate() {
                let ch = if channels == 2 { k } else { 0 };
                let step = STEP_TABLE[self.index[ch] as usize];
                let mut d = step >> 3;
                if c & 4 != 0 {
                    d += step;
                }
                if c & 2 != 0 {
                    d += step >> 1;
                }
                if c & 1 != 0 {
                    d += step >> 2;
                }
                if c & 8 != 0 {
                    d = -d;
                }
                self.pred[ch] = self.pred[ch].wrapping_add(d);
                self.index[ch] = (self.index[ch] + INDEX_ADJUST[usize::from(c & 7)]).clamp(0, 88);
                let w = self.write as usize;
                if w + 2 <= self.mem.len() {
                    self.mem[w..w + 2].copy_from_slice(&(self.pred[ch] as i16).to_le_bytes());
                }
                self.write = self.write.wrapping_add(2);
            }
        }
    }

    /// `FUN_00432ea8` @ 0x00432f44..0x00433109, from its first read on;
    /// false when the file is empty or its header is not an APC header (the
    /// original returns 0), or when the track is mono (every music track is
    /// stereo; not modelled).
    pub fn start(&mut self) -> bool {
        self.write = 0;
        self.release_block = 0;
        self.refill_block = 0;
        self.mark = 0;
        self.file_pos = 0;
        if self.read_file(Self::BLOCK_BYTES) == 0 {
            return false;
        }
        self.mem.fill(0);
        if !self.header() || !self.stereo {
            return false;
        }
        self.decode(0x3fe0);
        self.set_flag(0, 1);
        for i in 1..8 {
            self.read_file(Self::BLOCK_BYTES);
            self.src = 0;
            self.decode(Self::BLOCK_BYTES);
            self.set_flag(i, 1);
        }
        self.on = true;
        self.write = 0;
        self.play = 0;
        true
    }

    /// `FUN_00433310`: the position minus 26000, unsigned; one ring added
    /// when it is below the mark.
    fn release(&mut self) {
        if !self.on {
            return;
        }
        let gran = u64::from(self.gran);
        let pos =
            ((self.play + u64::from(self.ahead)) / gran * gran % u64::from(Self::RING_SIZE)) as u32;
        let p = pos.wrapping_sub(0x6590);
        let q = if p < self.mark {
            p.wrapping_add(Self::RING_SIZE)
        } else {
            p
        };
        if q.wrapping_sub(self.mark) > 0x10000 {
            self.set_flag(self.release_block, 0);
            self.release_block = (self.release_block + 1) & 7;
            self.mark = p;
        }
    }

    /// `FUN_0043318c`.
    fn refill(&mut self) {
        if self.flag(self.refill_block) != 0 || !self.on {
            return;
        }
        self.src = 0;
        let got = self.read_file(Self::BLOCK_BYTES);
        self.decode(Self::BLOCK_BYTES);
        if got < Self::BLOCK_BYTES {
            // The loop: back to the start, the header again, the next block
            // decoded right after the tail.
            self.file_pos = 0;
            self.read_file(0x20);
            self.header();
            self.read_file(Self::BLOCK_BYTES);
            self.src = 0;
            self.decode(Self::BLOCK_BYTES);
        }
        self.set_flag(self.refill_block, 1);
        self.refill_block += 1;
        if self.refill_block == 8 {
            self.write = 0;
            self.refill_block = 0;
        }
    }

    /// The frames of the 5 ms period `t` of the 200 Hz timer.
    pub fn frames_of_tick(&self, t: u32) -> usize {
        let r = u64::from(self.rate) * 5;
        ((u64::from(t) + 1) * r / 1000 - u64::from(t) * r / 1000) as usize
    }

    /// The sample plays `frames` frames of the ring into `out` (interleaved
    /// stereo).
    pub fn play(&mut self, frames: usize, out: &mut [i16]) {
        for f in 0..frames {
            let off = (self.play % u64::from(Self::RING_SIZE)) as usize;
            out[2 * f] = i16::from_le_bytes([self.mem[off], self.mem[off + 1]]);
            out[2 * f + 1] = i16::from_le_bytes([self.mem[off + 2], self.mem[off + 3]]);
            self.play += 4;
        }
    }

    /// The end of one timer period: `FUN_00433310`, then `FUN_0043318c`, in
    /// the same period (under Wine the refill comes 3 to 6 ms after the
    /// release).
    pub fn timer(&mut self) {
        self.release();
        self.refill();
    }

    /// `play` then `timer`: one 5 ms period.
    pub fn tick(&mut self, frames: usize, out: &mut [i16]) {
        self.play(frames, out);
        self.timer();
    }
}

/// The ring as a mixer source: each 5 ms period of the 200 Hz timer plays
/// its frames, then the timer's release and refill run, whatever the size
/// of the mixer's reads.
pub struct MusicRingSource {
    ring: MusicRing,
    t: u32,
    left: usize,
}

impl MusicRingSource {
    pub fn new(ring: MusicRing) -> Self {
        MusicRingSource {
            ring,
            t: 0,
            left: 0,
        }
    }
}

impl Source for MusicRingSource {
    fn rate(&self) -> u32 {
        self.ring.rate()
    }

    fn channels(&self) -> u16 {
        2
    }

    fn read(&mut self, out: &mut [i16]) -> usize {
        let frames = out.len() / 2;
        let mut done = 0;
        while done < frames {
            if self.left == 0 {
                self.left = self.ring.frames_of_tick(self.t);
                self.t = self.t.wrapping_add(1);
            }
            let k = self.left.min(frames - done);
            self.ring.play(k, &mut out[2 * done..]);
            done += k;
            self.left -= k;
            if self.left == 0 {
                self.ring.timer();
            }
        }
        frames
    }
}

/// What the timer callback works on: the fades and the voices they move.
struct SoundTimer {
    mixer: Mixer,
    fade: MusicFade,
    last_ms: Option<u64>,
    music_voice: Option<VoiceId>,
    new_sound_voice: Option<VoiceId>,
}

impl SoundTimer {
    /// The music voice's volume: the AIL volume 0x00495e71 (clamped to 127)
    /// over the full scale; the music volume 0x00495e72 is the level the
    /// fades move between.
    fn apply_music_volume(&self) {
        if let Some(v) = self.music_voice {
            let level = if self.fade.vol == 0 {
                0
            } else {
                u32::from(self.fade.cur.min(127)) * 255 / 127
            };
            self.mixer.set_volume(v, level as u8, 0);
        }
    }

    fn run(&mut self, now: u64) {
        let last = *self.last_ms.get_or_insert(now);
        let ticks = now.saturating_sub(last) / 5;
        self.last_ms = Some(last + ticks * 5);
        // A stall of more than 2 s (a debugger) does not replay its ticks.
        for _ in 0..ticks.min(400) {
            self.fade.tick();
        }
        self.apply_music_volume();
        if std::mem::take(&mut self.fade.sfx_changed) {
            if let Some(v) = self.new_sound_voice {
                let vol0 = self.fade.sfx_vol0.max(1);
                self.mixer
                    .set_volume(v, (self.fade.sfx_vol * 255 / vol0).clamp(0, 255) as u8, 0);
            }
        }
        if std::mem::take(&mut self.fade.sfx_stop) {
            // NewSound is cleared by its poller.
            if let Some(v) = self.new_sound_voice {
                self.mixer.stop(v);
            }
        }
    }
}

/// The music and NewSound state of the game.
pub struct Music {
    timer: Rc<RefCell<SoundTimer>>,
    /// The selected track (StartMusik), 0 for none.
    pub track_id: i32,
    /// A track is open (`_musicCurrentFile`).
    pub current_file: bool,
    /// The stream handle 0x006ce0c0 is set (-1 after a failed open counts).
    pub handle_set: bool,
    /// 0x00772915: the track opened last (0 for none).
    pub open_track: i32,
    /// The tracks the original would find in its install folder (bit i =
    /// track index i). The original's copies live in its install folder;
    /// the port reads the tracks from the archives and only notes them, in
    /// the settings ([`crate::Settings::music_on_disk`]), so that the set
    /// outlives the session as the folder does.
    pub on_disk: u32,
    /// `FUN_00432a64`: 0x006cd87c, the value last started.
    pub new_sound_last: i32,
    pub new_sound_playing: bool,
}

impl Music {
    /// The music state, its timer installed on `sys`.
    pub fn new(sys: &Sys) -> Music {
        let timer = Rc::new(RefCell::new(SoundTimer {
            mixer: sys.mixer.clone(),
            fade: MusicFade::default(),
            last_ms: None,
            music_voice: None,
            new_sound_voice: None,
        }));
        let t = timer.clone();
        sys.add_timer(Box::new(move |now| t.borrow_mut().run(now)));
        Music {
            timer,
            track_id: 0,
            current_file: false,
            handle_set: false,
            open_track: 0,
            on_disk: 0,
            new_sound_last: 0,
            new_sound_playing: false,
        }
    }

    /// The fade state (for tests and tools).
    pub fn fade(&self) -> MusicFade {
        self.timer.borrow().fade
    }

    pub fn with_fade<R>(&self, f: impl FnOnce(&mut MusicFade) -> R) -> R {
        f(&mut self.timer.borrow_mut().fade)
    }

    fn mixer(&self) -> Mixer {
        self.timer.borrow().mixer.clone()
    }

    /// The music voice is playing.
    pub fn playing(&self) -> bool {
        let t = self.timer.borrow();
        t.music_voice.is_some_and(|v| t.mixer.is_playing(v))
    }

    /// The NewSound voice is playing.
    pub fn new_sound_active(&self) -> bool {
        let t = self.timer.borrow();
        t.new_sound_voice.is_some_and(|v| t.mixer.is_playing(v))
    }

    /// The music down to `percent` % (or the default) when no fade runs
    /// (`only_when_idle`) or always.
    pub fn duck(&self, only_when_idle: bool, percent: Option<u8>) {
        self.with_fade(|f| {
            if let Some(p) = percent {
                f.percent = p;
            }
            if !only_when_idle || f.mode == 0 {
                f.mode = 2;
            }
        });
    }

    /// The music back up.
    pub fn unduck(&self, percent: Option<u8>) {
        self.with_fade(|f| {
            f.mode = 1;
            if let Some(p) = percent {
                f.percent = p;
            }
        });
    }

    /// Stops the music voice at once (the fades are the original's).
    pub fn halt(&mut self) {
        let mut t = self.timer.borrow_mut();
        if let Some(v) = t.music_voice.take() {
            t.mixer.stop(v);
        }
        self.current_file = false;
    }

    /// Starts the NewSound sample (`FUN_00432a64` step 5): at the effects
    /// volume, the fade-out state 0x00495eb0 cleared, so a fade armed by a
    /// video cut short (`FUN_0041bc74`) does not carry over to the new sound.
    pub fn start_new_sound(&self, pcm: Pcm) {
        let mut t = self.timer.borrow_mut();
        t.fade.sfx_mode = 0;
        t.fade.sfx_changed = false;
        t.fade.sfx_stop = false;
        if let Some(v) = t.new_sound_voice.take() {
            t.mixer.stop(v);
        }
        let v = t.mixer.play(pcm, false, 255, 0);
        t.new_sound_voice = Some(v);
    }

    pub fn stop_new_sound(&self) {
        let mut t = self.timer.borrow_mut();
        if let Some(v) = t.new_sound_voice.take() {
            t.mixer.stop(v);
        }
    }

    /// The NewSound sample fades out: 0x006ce128 = the effects volume, 2
    /// steps every 25 ms (`FUN_0041bc74` @ 0x0041be69..0x0041be9e arms
    /// 0x00495eb0 = 2).
    pub fn fade_out_new_sound(&self, effects_volume: u8) {
        self.with_fade(|f| {
            f.sfx_vol0 = i32::from(effects_volume).max(1);
            f.sfx_vol = f.sfx_vol0;
            f.sfx_last = f.ticks;
            f.sfx_mode = 2;
        });
    }
}

/// An APC file as mixer PCM.
pub fn apc_pcm(data: &[u8]) -> Option<Pcm> {
    let apc = decode_apc(data).ok()?;
    Some(Pcm {
        rate: apc.sample_rate,
        channels: apc.channels,
        samples: Arc::new(apc.samples),
    })
}

impl Engine {
    /// The name of music track `track` (1-based, StartMusik=N).
    fn music_track_name(&self, track: i32) -> Option<String> {
        let i = usize::try_from(track).ok()?.checked_sub(1)?;
        self.game.exe.music_names.get(i).cloned()
    }

    /// The original spins on the fade (calling only the music refill
    /// `FUN_0043318c`, which reads no input) until the timer callback has
    /// finished it; the screen does not change meanwhile and input made
    /// during the spin is seen by the next poll, `FUN_0042e624`'s
    /// GetAsyncKeyState pass (`FUN_0042a100` @ 0x0042ad98..0x0042ada6,
    /// 0x0042aed0..0x0042aede). So the events stay queued for the next
    /// poll; a quit is only delayed (the fade is timer driven, at most 99
    /// steps of 25 ms).
    async fn music_wait_fade_end(&self) {
        while self.music.fade().mode != 0 && !self.sys.should_quit() {
            self.sys.frame().await;
        }
    }

    /// `FUN_00432ea8`: open track `track` and start it looping (callers:
    /// the StartMusik command, the credits).
    pub async fn music_play_track(&mut self, track: i32) {
        let Some(name) = self.music_track_name(track) else {
            log::warn!("music: invalid track {track}");
            return;
        };
        self.music.track_id = track;
        self.music.halt();
        let Some(data) = self.read(&format!("WAV\\{name}")).await else {
            log::warn!("music: cannot open track {track} '{name}'");
            return;
        };
        let mixer = self.music.mixer();
        // The music loop's fix loops the music seamlessly; off, the
        // original's streaming ring (read at each track start, so the
        // options menu applies from the next track).
        let voice = if self.music_through_ring() {
            let mut ring = MusicRing::wine(Arc::new(data.clone()));
            ring.start()
                .then(|| mixer.play_source(Box::new(MusicRingSource::new(ring)), 0, 0))
        } else {
            None
        };
        // The clean loop: the track as decoded, to its header's frame count
        // (FUN_004101ce), looped (FUN_0043318c restarts from the header).
        let voice = voice.or_else(|| apc_pcm(&data).map(|pcm| mixer.play(pcm, true, 0, 0)));
        let Some(voice) = voice else {
            log::warn!("music: track {track} '{name}' is not an APC file");
            return;
        };
        {
            let mut t = self.music.timer.borrow_mut();
            t.music_voice = Some(voice);
            t.apply_music_volume();
        }
        self.music.current_file = true;
        log::info!("music: track {track} '{name}'");
        self.emit(crate::engine::Event::Music(track));
    }

    /// The fade down to silence of the playing track, then its stop.
    async fn music_fade_out_and_halt(&mut self) -> bool {
        if !(self.music.current_file && self.music.playing()) {
            return false;
        }
        self.music.with_fade(|f| {
            f.percent = 99;
            f.mode = 2;
        });
        self.music_wait_fade_end().await;
        self.music.halt();
        true
    }

    /// Another track: the playing one fades out first.
    pub async fn music_switch_track(&mut self, track: i32) {
        if self.music_fade_out_and_halt().await {
            self.music.with_fade(|f| f.percent = 80);
        }
        self.music_play_track(track).await;
        // FUN_00432ea8 stores the CreateFileA result in 0x006ce0c0
        // (@ 0x00432f2a, -1 on failure) and the track in 0x00772915 only
        // when the open worked (@ 0x00433118).
        self.music.handle_set = true;
        if self.music.current_file {
            self.music.open_track = track;
            self.music.with_fade(|f| {
                f.mode = 1;
                f.cur = 0;
                f.up_target = i32::from(f.vol);
            });
        }
    }

    /// StartMusik=N (`FUN_0042a100` @ 0x0042ad9d..0x0042ae0f).
    /// The open track again re-arms the fade-in (0x0042ae3f..0x0042ae4d);
    /// atlantis-cd-fr-688640 does nothing then (FUN_00429184 @
    /// 0x00429d47..0x00429d51), whatever the enhancements: a volume left
    /// down by a fade stays down until the next fade-in.
    pub async fn music_start_command(&mut self, track: i32) {
        if track == self.music.track_id && self.music.current_file {
            if self.game.exe.build.music_restart_rearms_fade {
                self.music.with_fade(|f| {
                    f.mode = 1;
                    f.up_target = i32::from(f.vol);
                });
            }
            return;
        }
        // Closing the open stream for another track resets the input flags,
        // the bar request among them (0x0042add3..0x0042ae05).
        if self.music.handle_set {
            self.bar.flags_reset();
        }
        self.music_switch_track(track).await;
    }

    /// StopMusik=N.
    pub async fn music_stop_command(&mut self) {
        let saved = self.music.fade().cur;
        if self.music_fade_out_and_halt().await {
            self.music.with_fade(|f| {
                f.cur = saved;
                f.percent = 80;
            });
        }
        // StopMusik closes and clears 0x006ce0c0 and 0x00772915 only when
        // the handle is set (@ 0x0042aea9, 0x0042af0e, 0x0042af18), then
        // resets the input flags (0x0042af26..0x0042af58).
        if self.music.handle_set {
            self.music.handle_set = false;
            self.music.open_track = 0;
            self.bar.flags_reset();
        }
        self.music.track_id = 0;
    }

    /// LoadMusik=N,... / DelMusik=N,... (`FUN_0041f1d0` / `FUN_0041faa0`,
    /// without the WAIT menu a copy shows): the original copies the tracks
    /// to its install folder or deletes them; playback itself reads the
    /// archive.
    ///
    /// atlantis-cd-fr-688640 has its own LOADMUSIK (decision of the port:
    /// the build's rules by default, the per-track copy of the other builds
    /// with the LOADMUSIK fix (`BugFix::LoadMusik`), so that a list whose first track is there
    /// leaves no other listed track uncopied):
    /// - it runs only when the current music volume 0x00485e71 is 5 or more
    ///   (FUN_00429184 @ 0x00429c5e), so with the music volume option below
    ///   5 or within 125 ms of a StartMusik the tracks are not copied;
    /// - its LoadMusik copies all or nothing on the first listed track
    ///   (FUN_0041e830, [`con_music_load_all_or_nothing`]).
    ///
    /// With that fix only the copy changes: the tracks are
    /// copied one by one as atlantis-cd-en-688128 does (the first-value
    /// rule of that build kept), and the volume test stays the build's.
    /// Playback reads the archive in either case (the copies are modelled
    /// for their trace in the video buffer), so no StartMusik goes silent
    /// here.
    pub async fn music_list_command(&mut self, load: bool, text: &[u8], pos: u32) {
        let list = con_music_list(text, pos);
        let mut on_disk = self.music.on_disk;
        let build = self.game.exe.build;
        let rule = build.music_first_value_rule;
        let current = self.music.track_id;
        let fixes = self.loadmusik_track_by_track();
        // The volume test (FUN_00429184 @ 0x00429c5e), with or without the
        // LOADMUSIK fix; music_load_min_volume is 0 in every other build.
        // The executor's first-value test (0x00429c52) is also LoadMusik's
        // own (0x0041e862..0x0041e891).
        let copies = if load && self.music.fade().cur < build.music_load_min_volume {
            Vec::new()
        } else if load && build.music_load_all_or_nothing && !fixes {
            con_music_load_all_or_nothing(&list, current, &mut on_disk)
        } else {
            con_music_apply(load, &list, current, &mut on_disk, rule)
        };
        for idx in copies {
            self.music_copy_through_screen_buffer(idx).await;
        }
        self.music.on_disk = on_disk;
        self.save_music_on_disk(on_disk);
    }

    /// The tracks on disk written to the settings file when they changed;
    /// the file's other settings are kept as stored.
    fn save_music_on_disk(&mut self, bits: u32) {
        if self.settings.music_on_disk == bits {
            return;
        }
        self.settings.music_on_disk = bits;
        let mut s = Settings::load(self.saves.as_ref()).unwrap_or(self.settings);
        s.music_on_disk = bits;
        s.save(self.saves.as_ref());
    }

    /// `FUN_0041f380`: the track is copied in 0x80000-byte reads through
    /// the first IX buffer until a read returns nothing, so the buffer ends
    /// with the file's last chunk at its start, then the rest of the chunk
    /// before. The next video's first frames show it.
    async fn music_copy_through_screen_buffer(&mut self, idx: u8) {
        let Some(name) = self.music_track_name(i32::from(idx) + 1) else {
            return;
        };
        let Some(data) = self.read(&format!("WAV\\{name}")).await else {
            return;
        };
        // malloc(0x12c000) @ 0x00418f83.
        if self.ix_buffers.len() < 0x12c000 / 2 {
            self.ix_buffers.resize(0x12c000 / 2, 0);
        }
        for chunk in data.chunks(0x80000) {
            for (i, px) in chunk.chunks(2).enumerate() {
                let lo = u16::from(px[0]);
                let hi = px.get(1).map_or(self.ix_buffers[i] >> 8, |&b| u16::from(b));
                self.ix_buffers[i] = lo | hi << 8;
            }
        }
        log::info!("loadmusik: track {} '{name}' copied", idx + 1);
    }

    /// The effects volume byte 0x00495e73 (0..127).
    pub fn effects_volume(&self) -> u8 {
        self.ambient.effects_volume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fades against `FUN_00431150` (tick, then `FUN_0042f7a8` fade
    /// down, `FUN_0042f974` fade up) run unmodified under emulation: the
    /// music volume and the mode after each tick, as runs
    /// "volume,mode,count".
    #[test]
    fn fades_match_original() {
        struct Case {
            name: &'static str,
            cur: u8,
            mode: u8,
            percent: u8,
            vol: u8,
            ticks: u32,
            last: u32,
            n: usize,
            runs: String,
        }
        fn steps(from: i32, to: i32, mode: u8) -> String {
            let d = if to < from { -1 } else { 1 };
            let mut s = Vec::new();
            let mut v = from;
            while v != to {
                s.push(format!("{v},{mode},5"));
                v += d;
            }
            s.join(";")
        }
        let cases = [
            Case {
                name: "duck_80",
                cur: 100,
                mode: 2,
                percent: 80,
                vol: 100,
                ticks: 1000,
                last: 0,
                n: 450,
                runs: format!("{};20,0,55", steps(99, 20, 2)),
            },
            Case {
                name: "unduck",
                cur: 20,
                mode: 1,
                percent: 80,
                vol: 100,
                ticks: 1000,
                last: 0,
                n: 450,
                runs: format!("{};100,0,55", steps(21, 100, 1)),
            },
            Case {
                name: "stop_99",
                cur: 127,
                mode: 2,
                percent: 99,
                vol: 127,
                ticks: 50,
                last: 48,
                n: 700,
                runs: format!("127,2,2;{};2,0,78", steps(126, 2, 2)),
            },
            Case {
                name: "fade_in_from_0",
                cur: 0,
                mode: 1,
                percent: 80,
                vol: 90,
                ticks: 7,
                last: 5,
                n: 500,
                runs: format!("0,1,2;{};90,0,53", steps(1, 90, 1)),
            },
            Case {
                name: "duck_at_target",
                cur: 20,
                mode: 2,
                percent: 80,
                vol: 100,
                ticks: 1000,
                last: 0,
                n: 20,
                runs: "20,0,20".into(),
            },
            Case {
                name: "duck_from_0",
                cur: 0,
                mode: 2,
                percent: 80,
                vol: 100,
                ticks: 1000,
                last: 0,
                n: 20,
                runs: "0,0,20".into(),
            },
            Case {
                name: "up_at_target",
                cur: 100,
                mode: 1,
                percent: 80,
                vol: 100,
                ticks: 1000,
                last: 0,
                n: 20,
                runs: "101,0,20".into(),
            },
            Case {
                name: "puzzle_50",
                cur: 64,
                mode: 2,
                percent: 50,
                vol: 64,
                ticks: 10,
                last: 3,
                n: 300,
                runs: format!("{};32,0,145", steps(63, 32, 2)),
            },
        ];
        for c in &cases {
            let mut f = MusicFade {
                cur: c.cur,
                mode: c.mode,
                down_target: -1,
                up_target: i32::from(c.vol),
                percent: c.percent,
                vol: c.vol,
                ticks: c.ticks,
                last_step: c.last,
                ..MusicFade::default()
            };
            let mut runs: Vec<(u8, u8, usize)> = Vec::new();
            for _ in 0..c.n {
                f.tick();
                match runs.last_mut() {
                    Some(r) if r.0 == f.cur && r.1 == f.mode => r.2 += 1,
                    _ => runs.push((f.cur, f.mode, 1)),
                }
            }
            let got: Vec<String> = runs
                .iter()
                .map(|(a, b, n)| format!("{a},{b},{n}"))
                .collect();
            assert_eq!(got.join(";"), c.runs, "{}", c.name);
        }
    }

    /// `FUN_0042f7a8`'s second part, the NewSound fade-out
    /// (0x0042f89d..0x0042f961), armed (0x00495eb0 = 2) alone and with a
    /// music fade-down, against `FUN_00431150` run unmodified under
    /// emulation: the music volume, the mode, the dword 0x006ce128 and
    /// 0x00495eb0 after each tick, as runs "volume,mode,sfxvolume,
    /// sfxmode,count". "duck_at_target" pins the early return
    /// (@ 0x0042f832..0x0042f843): no NewSound step on that tick.
    #[test]
    fn sfx_fades_match_original() {
        // name, cur, mode, vol, sfx volume, sfx last step, ticks run, runs
        // (ticks 1000, last step 0, percent 80).
        let from_127 = std::iter::once("0,0,127,2,4".to_string())
            .chain((1..=125).rev().step_by(2).map(|v| format!("0,0,{v},2,5")))
            .chain(std::iter::once("0,0,0,0,11".to_string()))
            .collect::<Vec<_>>()
            .join(";");
        let with_duck = (0..25)
            .flat_map(|k| {
                let (m, s) = (99 - k, 50 - 2 * k);
                let mode = if s > 2 { 2 } else { 0 };
                [
                    format!("{m},2,{s},2,4"),
                    format!("{m},2,{},{mode},1", s - 2),
                ]
            })
            .chain((60..=74).rev().map(|m| format!("{m},2,0,0,5")))
            .collect::<Vec<_>>()
            .join(";");
        // (name, cur, mode, vol, sfx_vol, sfx_last, ticks, runs)
        type Case = (&'static str, u8, u8, u8, i32, u32, usize, String);
        let cases: [Case; 4] = [
            ("sfx_from_127", 0, 0, 100, 127, 1000, 330, from_127),
            (
                "sfx_odd_3",
                0,
                0,
                100,
                3,
                998,
                20,
                "0,0,3,2,2;0,0,1,2,5;0,0,0,0,13".into(),
            ),
            (
                "sfx_duck_at_target",
                20,
                2,
                100,
                10,
                996,
                30,
                "20,0,10,2,1;20,0,8,2,5;20,0,6,2,5;20,0,4,2,5;20,0,2,2,5;20,0,0,0,9".into(),
            ),
            ("sfx_with_duck", 100, 2, 100, 50, 1000, 200, with_duck),
        ];
        for (name, cur, mode, vol, sfx_vol, sfx_last, n, want) in cases {
            let mut f = MusicFade {
                cur,
                mode,
                down_target: -1,
                up_target: i32::from(vol),
                percent: 80,
                vol,
                ticks: 1000,
                last_step: 0,
                sfx_mode: 2,
                sfx_vol,
                sfx_last,
                ..MusicFade::default()
            };
            let mut runs: Vec<((u8, u8, i32, u8), usize)> = Vec::new();
            for _ in 0..n {
                f.tick();
                let k = (f.cur, f.mode, f.sfx_vol, f.sfx_mode);
                match runs.last_mut() {
                    Some(r) if r.0 == k => r.1 += 1,
                    _ => runs.push((k, 1)),
                }
            }
            let got: Vec<String> = runs
                .iter()
                .map(|((a, b, c, d), n)| format!("{a},{b},{c},{d},{n}"))
                .collect();
            assert_eq!(got.join(";"), want, "{name}");
        }
    }
}

#[cfg(test)]
mod on_disk_tests {
    use super::*;
    use crate::persist::MemStore;
    use crate::test_support::with_engine;

    /// The tracks LoadMusik copied are kept in the settings: in the next
    /// session the same LoadMusik copies nothing (each copy goes through
    /// the IX video buffer and shows at the start of the next video).
    #[test]
    fn music_on_disk_outlives_the_session() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "music_on_disk_outlives_the_session",
            move |mut e| async move {
                let store = Rc::new(MemStore::new());
                e.saves = store.clone();
                let cmd = b"/set(loadmusik=3,5)\r\n";
                e.music_list_command(true, cmd, 15).await;
                let first = (e.music.on_disk, !e.ix_buffers.is_empty());
                let stored = Settings::load(store.as_ref()).map(|s| s.music_on_disk);
                // The next session: a new engine state, the same store.
                e.music.on_disk = 0;
                e.settings = Settings::default();
                e.ix_buffers.clear();
                e.load_settings(Settings::default());
                let reloaded = e.music.on_disk;
                e.music_list_command(true, cmd, 15).await;
                *g2.borrow_mut() = Some((first, stored, reloaded, e.ix_buffers.is_empty()));
            },
            |_, _| {},
        );
        if ran {
            let got = *got.borrow();
            assert_eq!(got, Some(((0x14, true), Some(0x14), 0x14, true)));
        }
    }
}

#[cfg(test)]
mod fr688640_tests {
    use super::*;
    use crate::enhancements::{BugFix, Enhancements};
    use crate::persist::MemStore;
    use crate::test_support::with_engine_in;

    /// atlantis-cd-fr-688640's LOADMUSIK: nothing below a current music
    /// volume of 5 (FUN_00429184 @ 0x00429c5e), all or nothing on its first
    /// track (FUN_0041e830), with every fix off (the original) as with
    /// every other fix on; with its fix, alone or with the others, one by
    /// one, the volume test kept.
    #[test]
    fn may_1997_music_copies() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine_in(
            "ATLANTIS_DATA_FR1997_MAY",
            "may_1997_music_copies",
            move |mut e| async move {
                assert_eq!(e.game.exe.build.name, "atlantis-cd-fr-688640");
                e.saves = Rc::new(MemStore::new());
                let b = |l: &[u32]| l.iter().fold(0u32, |a, &i| a | 1 << i);
                let load = async |e: &mut Engine, enh: Enhancements, cur: u8, cmd: &[u8]| {
                    e.set_enhancements(enh);
                    e.music.with_fade(|f| f.cur = cur);
                    e.music_list_command(true, cmd, 15).await;
                    e.music.on_disk
                };
                let mut out = Vec::new();
                // The original (every fix off), then every fix on but
                // LOADMUSIK's: the same copies.
                for enh in [
                    Enhancements::original(),
                    Enhancements::all_but_fix(BugFix::LoadMusik),
                ] {
                    e.music.on_disk = 0;
                    // The volume test: 4 copies nothing, 5 copies the list.
                    out.push(load(&mut e, enh, 4, b"/set(loadmusik=3,5)\r\n").await);
                    out.push(load(&mut e, enh, 5, b"/set(loadmusik=3,5)\r\n").await);
                    // The first track there: the others are not copied.
                    out.push(load(&mut e, enh, 127, b"/set(loadmusik=3,7)\r\n").await);
                    // The first track missing: all copied (3 again).
                    out.push(load(&mut e, enh, 127, b"/set(loadmusik=9,3)\r\n").await);
                    // The first track the open one: nothing.
                    e.music.track_id = 11;
                    out.push(load(&mut e, enh, 127, b"/set(loadmusik=11,2)\r\n").await);
                    e.music.track_id = 0;
                }
                // The LOADMUSIK fix alone, then with every other: the
                // volume test kept (4 copies nothing), then one by one.
                for enh in [
                    Enhancements::only_fix(BugFix::LoadMusik),
                    Enhancements::default(),
                ] {
                    e.music.on_disk = b(&[2, 4, 8]);
                    out.push(load(&mut e, enh, 4, b"/set(loadmusik=3,7)\r\n").await);
                    out.push(load(&mut e, enh, 5, b"/set(loadmusik=3,7)\r\n").await);
                }
                *g2.borrow_mut() = out;
            },
            |_, _| {},
        );
        if ran {
            let b = |l: &[u32]| l.iter().fold(0u32, |a, &i| a | 1 << i);
            assert_eq!(
                *got.borrow(),
                [0, 1]
                    .map(|_| [0, b(&[2, 4]), b(&[2, 4]), b(&[2, 4, 8]), b(&[2, 4, 8]),])
                    .into_iter()
                    .flatten()
                    .chain(
                        [0, 1]
                            .map(|_| [b(&[2, 4, 8]), b(&[2, 4, 6, 8])])
                            .into_iter()
                            .flatten()
                    )
                    .collect::<Vec<u32>>()
            );
        }
    }

    /// StartMusik of the open track: the reference re-arms the fade-in
    /// (FUN_0042a100 @ 0x0042ae3f), atlantis-cd-fr-688640 does nothing
    /// (FUN_00429184 @ 0x00429d51).
    #[test]
    fn restart_of_the_open_track() {
        fn run(var: &'static str, want: u8) {
            let got = Rc::new(RefCell::new(None));
            let g2 = got.clone();
            let ran = with_engine_in(
                var,
                "restart_of_the_open_track",
                move |mut e| async move {
                    e.music.track_id = 4;
                    e.music.current_file = true;
                    e.music.with_fade(|f| f.mode = 0);
                    e.music_start_command(4).await;
                    *g2.borrow_mut() = Some(e.music.fade().mode);
                },
                |_, _| {},
            );
            if ran {
                assert_eq!(*got.borrow(), Some(want), "{var}");
            }
        }
        run("ATLANTIS_DATA", 1);
        run("ATLANTIS_DATA_FR1997_MAY", 0);
    }
}

//! *Atlantis Evolution*'s 2004 program's music (atlantis4-cd-en-3022848,
//! `AtlSound` over FMOD 3): five music slots ("couches"), each playing a
//! playlist (`music\*.mls`, [`atlantis3_data::evolution2004_mls`]) as
//! FMOD streams of the CD's Ogg Vorbis files.
//!
//! The RE note is atlantis4-re `subsystems/cd2004-music.md`. Every rule here
//! cites its address in that binary:
//!
//! - the script's `STARTMUSIK=<couche>,<list>` (@ 0x0044edc0) →
//!   [`Music2004::start_musik`] (@ 0x00452be0), `STOPMUSIK=<couche>`
//!   (@ 0x0044f068) → [`Music2004::stop_musik`] (@ 0x00452ec0),
//!   `FADEMUSIK=<couche>,<volume>,<ms>` (@ 0x0044f298) →
//!   [`Music2004::fade_musik`] (@ 0x00452e80); `LOADMUSIK` and `DELMUSIK`
//!   only skip their line (@ 0x0044ed68, @ 0x0044ed94);
//! - a slot's playlist loaded and its first track played
//!   ([`Music2004::load_music`], @ 0x00430e30), a track played or its
//!   repeat armed again ([`Music2004::play`], @ 0x00431320), the stops and
//!   fades (@ 0x00431aa0, @ 0x00431be0, @ 0x004319e0, @ 0x00431980);
//! - the stream's sync callback (@ 0x004310e0, [`Music2004::sync`]) and the
//!   update the frame routine calls (@ 0x004a15bc → @ 0x00430380,
//!   [`Music2004::tick`]) when the 15 Hz timer (@ 0x004414b0, period
//!   1/15 s at 0x0070c0d0) set its flag 0x0070c10d.
//!
//! The FMOD side (stream open, play, loop count, volume 0..255, sync points)
//! is the [`Backend`]; [`OggSlots`] is the port's: the CD's Ogg files decoded
//! by the workspace's Vorbis decoder into [`Audio`] channels. What FMOD does
//! inside `fmod.dll` is not read: its loop count, sync-point and volume
//! semantics are taken from FMOD 3's API ([INFERRED], see [`OggSlots`]).

use std::collections::VecDeque;
use std::sync::Arc;

use atlantis2_engine::crt::Rand;
use atlantis3_data::evolution2004_mls::{Playlist, Track};
use atlantis3_data::A3Files;
use atlantis4_data::vorbis::VorbisStream;

use crate::audio::{Audio, CHANNEL_MUSIC_2004};

/// The slots: the array of 5 × 0x78 bytes at 0x006ea048, FMOD channels
/// 1..5 (slot + 1).
pub const SLOTS: usize = 5;

/// The repeat count that never runs out (`999x`, @ 0x004314af, @ 0x0043079b).
pub const FOREVER: i32 = 999;

/// A slot's fade (+0x18, 0x006ea060).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fade {
    /// 0: none.
    #[default]
    None,
    /// 1: up, done when the volume reaches the target or passes it upwards
    /// (@ 0x00430583).
    Up,
    /// 2: down, done when the volume reaches the target or passes it
    /// downwards; a target of 0 then stops the stream (@ 0x004305f5).
    Down,
    /// 3: out (a stop with the track's fade-out), then the queued playlist
    /// (@ 0x00430686).
    Out,
}

/// One slot (0x78 bytes at 0x006ea048 + slot · 0x78).
#[derive(Clone, Debug, Default)]
pub struct Slot {
    /// +0x00: the playlist's path (`music\<name>.mls`), kept after a stop.
    pub name: String,
    /// +0x18.
    pub fade: Fade,
    /// +0x1c: the volume, 0..100 (the track's `vol` scale).
    pub cur: f32,
    /// +0x20: the fade's target.
    pub target: f32,
    /// +0x24: the volume when the fade started.
    pub start: f32,
    /// +0x28: the fade's length in ms.
    pub dur: i32,
    /// +0x34: the plays left of the track (999: forever).
    pub repeat: i32,
    /// +0x3c: an FMOD stream is open.
    pub stream: bool,
    /// +0x48..+0x50: the playlist's tracks (0x58 bytes each).
    pub tracks: Vec<Track>,
    /// +0x54: the track index chosen when the playlist was loaded. The
    /// track chaining reads it but never writes it (@ 0x004307c4..; the
    /// play @ 0x00431320 stores no index), so a list of two tracks or more
    /// plays the chosen track, then the next one again and again
    /// (`foret_arbre.mls`: `berimbau.ogg`, then `Foret03.ogg` for ever;
    /// `Foret04.ogg` never plays).
    pub index: usize,
    /// +0x59 (0x006ea0a1): `RND`.
    pub random: bool,
    /// +0x5c (0x006ea0a4): a playlist was read (@ 0x00432c63); cleared by a
    /// stop (@ 0x00431bd3).
    pub loaded: bool,
    /// +0x60 (0x006ea0a8): the playlist to load once the fade-out ends.
    pub queued: String,
    /// +0x74 (0x006ea0bc): the track's end was reached (the sync callback).
    pub end: bool,
    /// 0x006ea008 + slot · 4: the volume as last given to FMOD, without
    /// FMOD's 2.55 scale.
    pub volume: f32,
}

/// The names a sync point carries to the callback (@ 0x004310e0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sync {
    /// A null name: the end flag set (@ 0x004312f6).
    Null,
    /// `TRUE` (0x006ae9c0, an `mo` marker): fade down to 0 with the
    /// track's fade-out (@ 0x004311a8).
    True,
    /// `FALSE` (0x006ae9b8): stop at once (@ 0x0043124c).
    False,
    /// `END_OF_SONG` (0x006ae9ac): the end flag set (@ 0x004312ec).
    EndOfSong,
}

/// FMOD as the music code calls it, and the C library's `rand`.
pub trait Backend {
    /// `FUN_0041ab50`: whether the file exists.
    fn exists(&mut self, path: &str) -> bool;
    /// A whole file (the playlist reader @ 0x00431d80).
    fn read(&mut self, path: &str) -> Option<Vec<u8>>;
    /// `FSOUND_Stream_Open(path, 0x22002, 0, 0)` then
    /// `FSOUND_Stream_Play(slot + 1, stream)` (@ 0x00431428, @ 0x00431460):
    /// false when either fails.
    fn open(&mut self, slot: usize, path: &str) -> bool;
    /// `FSOUND_Stream_SetLoopCount` (@ 0x00431505): −1 forever, else the
    /// plays after the first.
    fn set_loop_count(&mut self, slot: usize, count: i32);
    /// `FSOUND_SetVolume(slot + 1, v)`, 0..255.
    fn set_volume(&mut self, slot: usize, v: i32);
    /// `FSOUND_Stream_Stop` and `FSOUND_Stream_Close`.
    fn close(&mut self, slot: usize);
    /// `FSOUND_IsPlaying(slot + 1)` (@ 0x00431d30).
    fn is_playing(&mut self, slot: usize) -> bool;
    /// The `END_OF_SONG` sync point added at `GetLength / 4 − 100`
    /// (`GetLength` @ 0x00431750 / @ 0x0043190f, `AddSyncPoint`
    /// @ 0x00431766 / @ 0x00431925), with the callback (@ 0x00431775 /
    /// @ 0x00431938).
    fn add_end_of_song(&mut self, slot: usize);
    /// `rand()`.
    fn rand(&mut self) -> u32;
}

/// The music state (0x006ea008..0x006ea32c and the volumes at 0x006ae750..).
#[derive(Clone, Debug)]
pub struct Music2004 {
    pub slots: [Slot; SLOTS],
    /// 0x006ea319: the sound started (@ 0x0042ed17); every call does
    /// nothing without it.
    pub enabled: bool,
    /// 0x006ae750: `GlobalVolume`, 0..99.
    pub global: f32,
    /// 0x006ae754: `MusicVolume1`, 0..99.
    pub music: f32,
    /// 0x006ae764: the music's factor (1.0), faded by the cinematics
    /// ([`Music2004::duck`]).
    pub factor: f32,
    /// 0x006ea31a: the factor's fade runs; 0x006ea31c its start,
    /// 0x006ea320 its target, 0x006ae760 its length in ms.
    pub factor_fade: bool,
    pub factor_start: f32,
    pub factor_target: f32,
    pub factor_ms: i32,
}

impl Default for Music2004 {
    fn default() -> Self {
        Music2004 {
            slots: Default::default(),
            enabled: true,
            // The data's initial values (0x006ae750 = 99.0, 0x006ae754 =
            // 99.0, 0x006ae764 = 1.0, 0x006ae760 = 1000).
            global: 99.0,
            music: 99.0,
            factor: 1.0,
            factor_fade: false,
            factor_start: 0.0,
            factor_target: 0.0,
            factor_ms: 1000,
        }
    }
}

/// `0.01f` (0x00699334).
const HUNDREDTH: f32 = 0.01;
/// FMOD's volume scale `2.55f` (0x0069942c).
const FMOD_SCALE: f32 = 2.55;
/// Ticks per ms of a fade: `0.015f` (0x00699430), the 15 Hz update.
const TICKS_PER_MS: f32 = 0.015;

/// `__ftol`: truncation.
fn ftol(x: f64) -> i32 {
    x as i32
}

/// The path builder of the music folder (FUN_004179a0: `music\` of the
/// current disc); the port's files are named with `/`.
pub fn music_path(name: &str) -> String {
    format!("music/{name}")
}

/// The words of a script value as @ 0x0044aa70 reads them: separators
/// (@ 0x0044ade0: space, tab, CR, LF, `,`, `;`, `(`, `)`) skipped, the word
/// up to the next one.
fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split([' ', '\t', '\r', '\n', ',', ';', '(', ')'])
        .filter(|w| !w.is_empty())
}

/// The couche word (@ 0x0044ee20.., @ 0x0044f0bd.., @ 0x0044f2f9..):
/// `couche1`..`couche5` (any case, @ 0x0044a430) give 1..5, any other word
/// its non-letters (@ 0x0044a7b0) read by `atoi` (@ 0x00449a50).
pub fn couche(word: &str) -> i32 {
    let w = word.to_ascii_lowercase();
    for n in 1..=5 {
        if w == format!("couche{n}") {
            return n;
        }
    }
    let digits: Vec<u8> = word.bytes().filter(|c| !c.is_ascii_alphabetic()).collect();
    atlantis3_data::atl::atoi(&digits)
}

impl Music2004 {
    fn vol_431320(&self, cur: f32) -> (f64, f32) {
        // @ 0x0043150a: G · 0.01 · (M · 0.01) · cur · factor (· 2.55).
        let v = f64::from(self.global)
            * f64::from(HUNDREDTH)
            * (f64::from(self.music) * f64::from(HUNDREDTH))
            * f64::from(cur)
            * f64::from(self.factor);
        (v * f64::from(FMOD_SCALE), v as f32)
    }

    fn vol_430380(&self, cur: f32, factor: f32) -> (f64, f32) {
        // @ 0x004305b3: M · 0.01 · (G · 0.01) · cur · factor (· 2.55).
        let v = f64::from(self.music)
            * f64::from(HUNDREDTH)
            * (f64::from(self.global) * f64::from(HUNDREDTH))
            * f64::from(cur)
            * f64::from(factor);
        (v * f64::from(FMOD_SCALE), v as f32)
    }

    fn apply_slot_volume(&mut self, b: &mut dyn Backend, ch: usize) {
        let (fmod, v) = self.vol_430380(self.slots[ch].cur, self.factor);
        b.set_volume(ch, ftol(fmod));
        self.slots[ch].volume = v;
    }

    /// The fade-out of the slot's current track (`tpsfo`, track + 0x50 at
    /// the stored index). [UNKNOWN] with no track: the original reads past
    /// the vector; 0 here.
    fn fade_out_ms(&self, ch: usize) -> i32 {
        let s = &self.slots[ch];
        s.tracks.get(s.index).map_or(0, |t| t.fade_out_ms)
    }

    /// FUN_00431d30: `FSOUND_IsPlaying(ch + 1)`.
    pub fn is_playing(&self, b: &mut dyn Backend, ch: usize) -> bool {
        b.is_playing(ch)
    }

    /// FUN_00431d40: playing, and its playlist is `path`.
    pub fn plays(&self, b: &mut dyn Backend, ch: usize, path: &str) -> bool {
        b.is_playing(ch) && self.slots[ch].name == path
    }

    /// `STARTMUSIK` (@ 0x0044edc0): the value's first word the couche, the
    /// second the playlist, then FUN_00452be0. The caller keeps the
    /// click byte 0x006e9fe8 around it (@ 0x0044efe5, @ 0x0044f00f).
    pub fn start_musik_value(&mut self, b: &mut dyn Backend, value: &str) {
        let mut w = words(value);
        let ch = couche(w.next().unwrap_or(""));
        let name = w.next().unwrap_or("");
        self.start_musik(b, ch, name);
    }

    /// FUN_00452be0(name, couche): the slot `couche − 1` (0 when outside
    /// 0..4); `.mls` (0x006b002c) added unless the name ends with it (its
    /// last four characters lower-cased), the path through the music
    /// folder; nothing when that playlist plays already in the slot, else
    /// the slot stopped (with its fade-out) and the playlist loaded.
    pub fn start_musik(&mut self, b: &mut dyn Backend, couche: i32, name: &str) {
        let mut ch = couche - 1;
        if !(0..=4).contains(&ch) {
            ch = 0;
        }
        let ch = ch as usize;
        let n = name.len();
        let has_mls = n >= 4 && name.as_bytes()[n - 4..].eq_ignore_ascii_case(b".mls");
        let path = if has_mls {
            music_path(name)
        } else {
            music_path(&format!("{name}.mls"))
        };
        // @ 0x00452dc0.
        if !self.is_playing(b, ch) {
            self.load_music(b, ch, &path);
        } else if !self.plays(b, ch, &path) {
            self.stop(b, ch, true);
            self.load_music(b, ch, &path);
        }
    }

    /// `STOPMUSIK` (@ 0x0044f068): the couche word, then FUN_00452ec0(ch,
    /// 1) (the words after it are not read).
    pub fn stop_musik_value(&mut self, b: &mut dyn Backend, value: &str) {
        let ch = couche(words(value).next().unwrap_or(""));
        self.stop_musik(b, ch, true);
    }

    /// FUN_00452ec0(couche, fade): nothing outside 1..5; the slot stopped
    /// when it plays.
    pub fn stop_musik(&mut self, b: &mut dyn Backend, couche: i32, fade: bool) -> bool {
        let ch = couche - 1;
        if !(0..5).contains(&ch) {
            return false;
        }
        if self.is_playing(b, ch as usize) {
            self.stop(b, ch as usize, fade);
        }
        true
    }

    /// `FADEMUSIK` (@ 0x0044f298): the couche word, then two numbers (each
    /// the next word's `atoi`, @ 0x00449240): FUN_00452e80(ch, volume, ms).
    pub fn fade_musik_value(&mut self, b: &mut dyn Backend, value: &str) {
        let mut w = words(value);
        let ch = couche(w.next().unwrap_or(""));
        let mut num = || atlantis3_data::atl::atoi(w.next().unwrap_or("").as_bytes());
        let volume = num();
        let ms = num();
        self.fade_musik(b, ch, volume, ms);
    }

    /// FUN_00452e80(couche, volume, ms): slot `couche − 1` (0 when outside
    /// 0..4); when it plays, FUN_00431980.
    pub fn fade_musik(&mut self, b: &mut dyn Backend, couche: i32, volume: i32, ms: i32) -> bool {
        let mut ch = couche - 1;
        if !(0..=4).contains(&ch) {
            ch = 0;
        }
        if !self.is_playing(b, ch as usize) {
            return false;
        }
        self.fade(ch as usize, volume, ms);
        true
    }

    /// FUN_00431980(ch, volume, ms): a fade from the volume to `volume`.
    /// The state is [`Fade::Up`] when `volume` ≤ the volume (`fcom`, test
    /// 0x41 @ 0x0043199a), else [`Fade::Down`]: the reverse of the update's
    /// tests, so a fade of 67 ms or more ends at its first tick (the volume
    /// jumps to `volume`), a shorter one runs on without end [sic].
    pub fn fade(&mut self, ch: usize, volume: i32, ms: i32) {
        if !self.enabled {
            return;
        }
        let s = &mut self.slots[ch];
        let v = volume as f32;
        s.fade = if v <= s.cur { Fade::Up } else { Fade::Down };
        s.dur = ms;
        s.start = s.cur;
        s.target = v;
    }

    /// FUN_00430e30(ch, path): load the playlist and play its first track
    /// (`rand() % n` for a random list, else 0). Nothing when the file is
    /// missing (logged `bool AtlSound_LoadMusic(...)`, 0x006ae95c). A slot
    /// with a stream open keeps it: the same playlist does nothing, another
    /// one is queued (+0x60) for the end of the fade-out.
    pub fn load_music(&mut self, b: &mut dyn Backend, ch: usize, path: &str) -> bool {
        if !self.enabled {
            return false;
        }
        if !b.exists(path) {
            log::warn!("bool AtlSound_LoadMusic(const CString& p_Name) : {path}");
            return false;
        }
        if self.slots[ch].stream {
            if self.slots[ch].name == path {
                return false;
            }
            self.slots[ch].queued = path.to_string();
            return true;
        }
        self.read_playlist(b, ch, path);
        let s = &mut self.slots[ch];
        s.queued.clear();
        s.name = path.to_string();
        s.index = if s.random {
            let n = s.tracks.len() as u32;
            let r = b.rand();
            // [UNKNOWN] an empty random list divides by 0 (@ 0x00431037).
            if n == 0 {
                0
            } else {
                (r % n) as usize
            }
        } else {
            0
        };
        let index = self.slots[ch].index;
        self.play(b, ch, index, None)
    }

    /// FUN_00431d80(ch, path): the playlist read into the slot (random
    /// flag, tracks), then the loaded flag set (@ 0x00432c63). A missing
    /// file is logged `PlayList introuvable : %s` (0x006aea58). The
    /// original appends to the slot's tracks; its callers reach it with
    /// the tracks cleared by a stop or never filled, here they are
    /// replaced [INFERRED].
    fn read_playlist(&mut self, b: &mut dyn Backend, ch: usize, path: &str) {
        let Some(data) = b.read(path) else {
            log::warn!("PlayList introuvable : {path}");
            return;
        };
        let list = Playlist::parse(&data);
        let s = &mut self.slots[ch];
        s.random = list.random;
        s.tracks = list.tracks;
        s.loaded = true;
    }

    /// FUN_00431320(ch, state, index, prev): `prev == Some(index)` arms the
    /// current stream's markers and `END_OF_SONG` again (its repeat);
    /// otherwise track `index` is opened and played:
    ///
    /// - the volume 0 with a fade-in of `tpsfi` ms up to `vol`
    ///   ([`Fade::Up`]), or `vol` at once when `tpsfi` is 0 (@ 0x0043137d);
    /// - the repeat from `<n>x`; FMOD's loop count −1 for 999 or a list of
    ///   one track, else `repeat − 1` (@ 0x004314af);
    /// - the volume `ftol(G · 0.01 · M · 0.01 · cur · factor · 2.55)`
    ///   (@ 0x0043150a);
    /// - `mi` / `mo` markers (a marker of the `.mrk` file drawn by `rand()`:
    ///   `mi` the stream's start position, `mo` a `TRUE` sync point
    ///   @ 0x0043173c; on a repeat a `FALSE` one @ 0x004318ee): not ported,
    ///   the CD has no `.mrk` file and no list names a marker
    ///   [CONFIRMED-DATA];
    /// - `END_OF_SONG` at the stream's length / 4 − 100, the end flag
    ///   cleared.
    ///
    /// The stored index (+0x54) is not changed.
    pub fn play(
        &mut self,
        b: &mut dyn Backend,
        ch: usize,
        index: usize,
        prev: Option<usize>,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        if prev == Some(index) {
            // @ 0x004317ba: the `mi` seek and the `mo` sync point (not
            // ported), END_OF_SONG again, the end flag cleared.
            b.add_end_of_song(ch);
            self.slots[ch].end = false;
            return true;
        }
        let Some(t) = self.slots[ch].tracks.get(index).cloned() else {
            // [UNKNOWN] the original reads past the vector.
            return false;
        };
        let s = &mut self.slots[ch];
        if t.fade_in_ms == 0 {
            s.fade = Fade::None;
            s.dur = 0;
            s.cur = t.volume as f32;
            s.start = t.volume as f32;
        } else {
            s.fade = Fade::Up;
            s.cur = 0.0;
            s.dur = t.fade_in_ms;
            s.start = 0.0;
        }
        s.target = t.volume as f32;
        s.repeat = t.repeat;
        s.stream = false;
        let file: String = t.file.iter().map(|&c| char::from(c)).collect();
        let path = music_path(&file);
        if !b.open(ch, &path) {
            log::warn!("AtlSound_PlayMusic({path}) : Stream_Open() Not OK");
            return false;
        }
        s.stream = true;
        let count = if s.repeat == FOREVER || s.tracks.len() == 1 {
            -1
        } else {
            s.repeat - 1
        };
        b.set_loop_count(ch, count);
        let (fmod, v) = self.vol_431320(self.slots[ch].cur);
        b.set_volume(ch, ftol(fmod));
        self.slots[ch].volume = v;
        b.add_end_of_song(ch);
        self.slots[ch].end = false;
        true
    }

    /// FUN_00431aa0(ch, fade): with `fade` and a track fade-out, the
    /// fade-out (FUN_00431be0: [`Fade::Out`] from the volume to 0 in
    /// `tpsfo` ms); else at once: volume 0, the stream closed, the tracks
    /// freed, the loaded flag cleared (the playlist's name kept).
    pub fn stop(&mut self, b: &mut dyn Backend, ch: usize, fade: bool) {
        if !self.enabled {
            return;
        }
        let ms = self.fade_out_ms(ch);
        if fade && ms != 0 {
            let s = &mut self.slots[ch];
            s.fade = Fade::Out;
            s.dur = ms;
            s.start = s.cur;
            s.target = 0.0;
            return;
        }
        b.set_volume(ch, 0);
        let s = &mut self.slots[ch];
        s.volume = 0.0;
        b.close(ch);
        s.stream = false;
        s.tracks.clear();
        s.loaded = false;
    }

    /// FUN_004319e0(ch, fade, target): with `fade` and a track fade-out,
    /// [`Fade::Down`] to `target` in `tpsfo` ms; else the stream closed at
    /// once and the end flag set (the tracks kept).
    pub fn fade_down(&mut self, b: &mut dyn Backend, ch: usize, fade: bool, target: i32) {
        if !self.enabled {
            return;
        }
        let ms = self.fade_out_ms(ch);
        let s = &mut self.slots[ch];
        if fade && ms != 0 {
            s.fade = Fade::Down;
            s.dur = ms;
            s.start = s.cur;
            s.target = target as f32;
            return;
        }
        b.close(ch);
        s.stream = false;
        s.end = true;
    }

    /// The sync callback (@ 0x004310e0, `userdata` the slot).
    pub fn sync(&mut self, b: &mut dyn Backend, ch: usize, name: Sync) {
        match name {
            Sync::Null | Sync::EndOfSong => self.slots[ch].end = true,
            Sync::True => self.fade_down(b, ch, true, 0),
            Sync::False => self.fade_down(b, ch, false, 0),
        }
    }

    /// One fade step: `cur + (target − start) / (dur · 0.015)`, then
    /// compared before its store to `cur` (x87 extended precision in the
    /// original, `f64` here).
    fn step(s: &Slot) -> f64 {
        let d = f64::from(s.dur) * f64::from(TICKS_PER_MS);
        f64::from(s.cur) + (f64::from(s.target) - f64::from(s.start)) / d
    }

    /// FUN_00430380, run when the 15 Hz flag 0x0070c10d is set (it is
    /// cleared): the factor's fade, then each slot with a stream: its fade
    /// step and FMOD volume, then its end flag (taken and cleared,
    /// FUN_00430350) with a playlist loaded: the repeat counted down
    /// (not 999); while plays are left the stream's repeat is armed again;
    /// else a list of one track arms it again too, a longer one closes the
    /// stream and plays track `(stored index + 1) % n`.
    ///
    /// Not ported: the global volume's fade 0x006ea324 (nothing sets it,
    /// [CONFIRMED-STATIC]), the stream 0x006e0200 closed outside mode 5
    /// (@ 0x00430524, not music: [UNKNOWN] what it is), and the sample
    /// cache's sweep (@ 0x00430853).
    pub fn tick(&mut self, b: &mut dyn Backend) {
        if !self.enabled {
            return;
        }
        if self.factor_fade {
            // @ 0x0043046b.
            let diff = self.factor_target - self.factor_start;
            let d = f64::from(self.factor_ms) * f64::from(TICKS_PER_MS);
            self.factor = (f64::from(self.factor) + f64::from(diff) / d) as f32;
            let done = if diff > 0.0 {
                self.factor >= self.factor_target
            } else {
                self.factor <= self.factor_target
            };
            if done {
                self.factor = self.factor_target;
                self.apply(b, self.factor_target);
                self.factor_fade = false;
            } else {
                self.apply(b, self.factor);
            }
        }
        for ch in 0..SLOTS {
            if !self.slots[ch].stream {
                continue;
            }
            if self.slots[ch].fade == Fade::Up {
                let s = &mut self.slots[ch];
                let v = Self::step(s);
                s.cur = v as f32;
                if v >= f64::from(s.target) {
                    s.cur = s.target;
                    s.fade = Fade::None;
                }
                self.apply_slot_volume(b, ch);
            }
            if self.slots[ch].fade == Fade::Down {
                let s = &mut self.slots[ch];
                let v = Self::step(s);
                s.cur = v as f32;
                if v <= f64::from(s.target) {
                    s.cur = s.target;
                    s.fade = Fade::None;
                    if s.target == 0.0 {
                        self.fade_down(b, ch, false, 0);
                    }
                }
                self.apply_slot_volume(b, ch);
            }
            if self.slots[ch].fade == Fade::Out {
                let s = &mut self.slots[ch];
                let v = Self::step(s);
                s.cur = v as f32;
                if v <= f64::from(s.target) {
                    s.cur = s.target;
                    s.fade = Fade::None;
                    self.stop(b, ch, false);
                    let queued = self.slots[ch].queued.clone();
                    if !queued.is_empty() {
                        self.load_music(b, ch, &queued);
                    }
                }
                self.apply_slot_volume(b, ch);
            }
            // @ 0x0043077c.
            let end = std::mem::take(&mut self.slots[ch].end);
            if !end || !self.slots[ch].loaded {
                continue;
            }
            let s = &mut self.slots[ch];
            if s.repeat != FOREVER {
                s.repeat -= 1;
            }
            let left = s.repeat;
            let index = s.index;
            if left > 0 {
                self.play(b, ch, index, Some(index));
                self.slots[ch].repeat = left;
            } else if self.slots[ch].tracks.len() == 1 {
                self.play(b, ch, index, Some(index));
            } else {
                b.close(ch);
                self.slots[ch].stream = false;
                let n = self.slots[ch].tracks.len();
                // [UNKNOWN] an empty list divides by 0 (@ 0x0043082c).
                if n != 0 {
                    self.play(b, ch, (index + 1) % n, Some(index));
                }
            }
        }
    }

    /// FUN_00430cb0(x): without the factor's fade, the music volume :=
    /// `x` clamped to 0..99, each slot with a stream given
    /// `M · 0.01 · G · 0.01 · cur · factor`; during the factor's fade `x`
    /// is the factor for that product (the music volume unchanged).
    pub fn apply(&mut self, b: &mut dyn Backend, x: f32) {
        let factor = if self.factor_fade {
            x
        } else {
            self.music = x.clamp(0.0, 99.0);
            self.factor
        };
        for ch in 0..SLOTS {
            if self.slots[ch].stream {
                // Both branches: (M · 0.01) · (G · 0.01) · cur · factor
                // (both loops of @ 0x00430cb0).
                let (fmod, v) = self.vol_430380(self.slots[ch].cur, factor);
                b.set_volume(ch, ftol(fmod));
                self.slots[ch].volume = v;
            }
        }
    }

    /// The music volume set (FUN_00430cb0 with the value).
    pub fn set_music_volume(&mut self, b: &mut dyn Backend, v: f32) {
        self.apply(b, v);
    }

    /// FUN_00430dd0(v): `GlobalVolume` := `v` clamped to 0..99, then
    /// FUN_00430cb0(music volume).
    pub fn set_global_volume(&mut self, b: &mut dyn Backend, v: f32) {
        self.global = v.clamp(0.0, 99.0);
        let m = self.music;
        self.apply(b, m);
    }

    /// FUN_00430c50(target, ms) (FUN_00430bf0 with 1000 ms): the factor
    /// faded to `target` clamped to 0..99. The Bink cinematics call it
    /// (`AtlBink_Start` FUN_00401d30 @ 0x0040269a with `(100 − x) · 0.01`
    /// over 1000 ms when the `CINEMA=` option `%x` (0x006ac080, 100 by
    /// default) is above 0; their end FUN_00403230 @ 0x0040332e with 1.0
    /// over 2000 ms): `crate::game::Game`'s films call it.
    pub fn duck(&mut self, target: f32, ms: i32) {
        self.factor_target = target.clamp(0.0, 99.0);
        self.factor_start = self.factor;
        self.factor_ms = ms;
        self.factor_fade = true;
    }
}

// ---- the port's FMOD -------------------------------------------------------

/// The decoded frames kept ahead of the output.
const AHEAD_FRAMES: u64 = 22050;

struct OggStream {
    data: Arc<[u8]>,
    dec: VorbisStream,
    /// Frames of one pass: the last Ogg page's granule position.
    len: u64,
    /// FMOD's loop count left: −1 forever.
    loops: i32,
    /// Frames given to the output.
    pushed: u64,
    /// Nothing more to give.
    done: bool,
    /// Positions (output frames since the start) of the `END_OF_SONG`
    /// sync points of the passes decoded so far.
    syncs: VecDeque<u64>,
}

/// FMOD's music streams as the port plays them: one [`Audio`] channel per
/// slot ([`CHANNEL_MUSIC_2004`] + slot), the Ogg file decoded ahead of the
/// output.
///
/// Approximations of FMOD 3 (`fmod.dll` not read):
///
/// - the loop count: −1 forever, `n` the plays after the first; a loop
///   starts the file again at once (no gap) [INFERRED from FMOD 3's API];
/// - `END_OF_SONG` (`GetLength / 4 − 100`): `GetLength` taken as the
///   decoded PCM's bytes, so 100 frames before each pass's end (every
///   music file of the CD is 16-bit stereo, 44100 Hz [CONFIRMED-DATA]); a
///   sync point fires once per pass [INFERRED];
/// - the volume 0..255 a linear gain `v / 255` [INFERRED];
/// - the callback runs on FMOD's mixer thread in the original; here the
///   positions are checked once a frame ([`OggSlots::pump`]).
#[derive(Default)]
pub struct OggSlots {
    streams: [Option<OggStream>; SLOTS],
}

impl std::fmt::Debug for OggSlots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OggSlots")
            .field(
                "open",
                &self.streams.iter().map(Option::is_some).collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// The granule position of the last Ogg page: the stream's length in
/// frames.
pub fn ogg_frames(data: &[u8]) -> u64 {
    let mut i = data.len().saturating_sub(27);
    loop {
        if data[i..].starts_with(b"OggS") && i + 14 <= data.len() {
            let g = u64::from_le_bytes(data[i + 6..i + 14].try_into().unwrap_or([0; 8]));
            if g != u64::MAX {
                return g;
            }
        }
        if i == 0 {
            return 0;
        }
        i -= 1;
    }
}

impl OggSlots {
    /// Opens `data` on slot `ch` and starts its channel.
    pub fn open(&mut self, audio: &Audio, ch: usize, data: Vec<u8>) -> bool {
        let data: Arc<[u8]> = Arc::from(data);
        let Ok(dec) = VorbisStream::new(data.clone()) else {
            return false;
        };
        audio.start_stream(
            CHANNEL_MUSIC_2004 + ch,
            dec.rate(),
            dec.channels() as u16,
            0.0,
        );
        let len = ogg_frames(&data);
        let mut syncs = VecDeque::new();
        if len >= 100 {
            syncs.push_back(len - 100);
        }
        self.streams[ch] = Some(OggStream {
            data,
            dec,
            len,
            loops: 0,
            pushed: 0,
            done: false,
            syncs,
        });
        true
    }

    pub fn set_loop_count(&mut self, ch: usize, count: i32) {
        if let Some(s) = self.streams[ch].as_mut() {
            s.loops = count;
        }
    }

    pub fn close(&mut self, audio: &Audio, ch: usize) {
        if self.streams[ch].take().is_some() {
            audio.stop(CHANNEL_MUSIC_2004 + ch);
        }
    }

    pub fn is_playing(&self, audio: &Audio, ch: usize) -> bool {
        match &self.streams[ch] {
            Some(s) => !s.done || audio.is_playing(CHANNEL_MUSIC_2004 + ch),
            None => false,
        }
    }

    /// Whether slot `ch` has a stream.
    pub fn is_open(&self, ch: usize) -> bool {
        self.streams[ch].is_some()
    }

    /// Decodes ahead of the output and returns the slots whose
    /// `END_OF_SONG` the output passed.
    pub fn pump(&mut self, audio: &Audio) -> Vec<usize> {
        let mut fired = Vec::new();
        for (ch, slot) in self.streams.iter_mut().enumerate() {
            let Some(s) = slot.as_mut() else { continue };
            let chan = CHANNEL_MUSIC_2004 + ch;
            let played = audio.played(chan).unwrap_or(0);
            let mut buf = Vec::new();
            while !s.done && s.pushed < played + AHEAD_FRAMES {
                buf.clear();
                let n = s.dec.read(&mut buf, 4096);
                if n == 0 {
                    if s.loops == 0 {
                        s.done = true;
                        break;
                    }
                    if s.loops > 0 {
                        s.loops -= 1;
                    }
                    match VorbisStream::new(s.data.clone()) {
                        Ok(d) => s.dec = d,
                        Err(_) => {
                            s.done = true;
                            break;
                        }
                    }
                    if s.len >= 100 {
                        s.syncs.push_back(s.pushed + s.len - 100);
                    }
                    continue;
                }
                audio.push(chan, &buf);
                s.pushed += n as u64;
            }
            let mut hit = false;
            while s.syncs.front().is_some_and(|&p| p <= played) {
                s.syncs.pop_front();
                hit = true;
            }
            if hit {
                fired.push(ch);
            }
        }
        fired
    }
}

/// The [`Backend`] of the game: its files, its mix, its `rand`.
pub struct GameBackend<'a> {
    pub files: &'a A3Files,
    pub audio: &'a Audio,
    pub rand: &'a mut Rand,
    pub streams: &'a mut OggSlots,
    /// The files opened, in order (for the tests and the log).
    pub opened: Option<&'a mut Vec<String>>,
}

impl Backend for GameBackend<'_> {
    fn exists(&mut self, path: &str) -> bool {
        self.files.size(path).is_some()
    }
    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        self.files.read(path)
    }
    fn open(&mut self, slot: usize, path: &str) -> bool {
        let Some(data) = self.files.read(path) else {
            return false;
        };
        if let Some(o) = self.opened.as_mut() {
            o.push(path.to_string());
        }
        self.streams.open(self.audio, slot, data)
    }
    fn set_loop_count(&mut self, slot: usize, count: i32) {
        self.streams.set_loop_count(slot, count);
    }
    fn set_volume(&mut self, slot: usize, v: i32) {
        let g = v.clamp(0, 255) as f32 / 255.0;
        self.audio.set_gain(CHANNEL_MUSIC_2004 + slot, [g, g]);
    }
    fn close(&mut self, slot: usize) {
        self.streams.close(self.audio, slot);
    }
    fn is_playing(&mut self, slot: usize) -> bool {
        self.streams.is_playing(self.audio, slot)
    }
    fn add_end_of_song(&mut self, _slot: usize) {
        // Armed per pass by [`OggSlots`].
    }
    fn rand(&mut self) -> u32 {
        self.rand.rand() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    /// FMOD and the files as a table.
    #[derive(Default)]
    struct Fake {
        files: HashMap<String, Vec<u8>>,
        playing: [bool; SLOTS],
        opened: Vec<(usize, String)>,
        loops: [i32; SLOTS],
        volume: [i32; SLOTS],
        closed: Vec<usize>,
        armed: Vec<usize>,
    }

    impl Backend for Fake {
        fn exists(&mut self, path: &str) -> bool {
            self.files.contains_key(path)
        }
        fn read(&mut self, path: &str) -> Option<Vec<u8>> {
            self.files.get(path).cloned()
        }
        fn open(&mut self, slot: usize, path: &str) -> bool {
            self.opened.push((slot, path.to_string()));
            self.playing[slot] = true;
            true
        }
        fn set_loop_count(&mut self, slot: usize, count: i32) {
            self.loops[slot] = count;
        }
        fn set_volume(&mut self, slot: usize, v: i32) {
            self.volume[slot] = v;
        }
        fn close(&mut self, slot: usize) {
            self.closed.push(slot);
            self.playing[slot] = false;
        }
        fn is_playing(&mut self, slot: usize) -> bool {
            self.playing[slot]
        }
        fn add_end_of_song(&mut self, slot: usize) {
            self.armed.push(slot);
        }
        fn rand(&mut self) -> u32 {
            41
        }
    }

    /// Ticks until `done` (at most 100), their count.
    fn ticks_until(
        m: &mut Music2004,
        b: &mut Fake,
        done: impl Fn(&Music2004, &Fake) -> bool,
    ) -> usize {
        for n in 1..=100 {
            m.tick(b);
            if done(m, b) {
                return n;
            }
        }
        panic!("not done");
    }

    fn fake(lists: &[(&str, &str)]) -> Fake {
        let mut f = Fake::default();
        for (name, text) in lists {
            f.files.insert(music_path(name), text.as_bytes().to_vec());
        }
        f
    }

    #[test]
    fn couche_words() {
        assert_eq!(couche("couche3"), 3);
        assert_eq!(couche("COUCHE5"), 5);
        assert_eq!(couche("2"), 2);
        assert_eq!(couche("x4"), 4);
        assert_eq!(couche(""), 0);
    }

    #[test]
    fn start_plays_the_first_track_with_its_fade_in() {
        let mut b = fake(&[(
            "muzik20.mls",
            "muzik20.ogg vol=100 tpsfi=3000 tpsfo=1000\r\n",
        )]);
        let mut m = Music2004::default();
        m.start_musik_value(&mut b, "1,muzik20)");
        assert_eq!(b.opened, vec![(0, "music/muzik20.ogg".to_string())]);
        let s = &m.slots[0];
        assert_eq!(s.name, "music/muzik20.mls");
        assert_eq!(
            (s.fade, s.cur, s.target, s.dur),
            (Fade::Up, 0.0, 100.0, 3000)
        );
        // One track: FMOD loops it forever.
        assert_eq!(b.loops[0], -1);
        assert_eq!(b.volume[0], 0);
        // 3000 ms = 45 ticks of 100 / 45.
        m.tick(&mut b);
        assert!((m.slots[0].cur - 100.0 / 45.0).abs() < 1e-4);
        let n = ticks_until(&mut m, &mut b, |m, _| m.slots[0].fade == Fade::None);
        assert!((44..=45).contains(&n), "{n}");
        assert_eq!(m.slots[0].cur, 100.0);
        // ftol(99 · 0.01 · 99 · 0.01 · 100 · 2.55) = 249.
        assert_eq!(b.volume[0], 249);
    }

    #[test]
    fn the_same_list_does_nothing_and_another_waits_for_the_fade_out() {
        let mut b = fake(&[
            ("a.mls", "a.ogg vol=50 tpsfi=0 tpsfo=1000\r\n"),
            ("b.mls", "b.ogg tpsfi=0\r\n"),
        ]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 2, "a");
        m.start_musik(&mut b, 2, "A.MLS");
        // `A.MLS` is another path (the comparison is exact).
        assert_eq!(m.slots[1].fade, Fade::Out);
        m.slots[1].fade = Fade::None;
        m.slots[1].queued.clear();
        m.start_musik(&mut b, 2, "a.mls");
        assert_eq!(b.opened.len(), 1);
        assert_eq!(m.slots[1].fade, Fade::None);
        m.start_musik(&mut b, 2, "b");
        assert_eq!(m.slots[1].fade, Fade::Out);
        assert_eq!(m.slots[1].queued, "music/b.mls");
        assert_eq!(b.opened.len(), 1);
        // 1000 ms = 15 ticks.
        let n = ticks_until(&mut m, &mut b, |_, b| b.opened.len() == 2);
        assert!((15..=16).contains(&n), "{n}");
        assert_eq!(b.closed, vec![1]);
        assert_eq!(b.opened[1], (1, "music/b.ogg".to_string()));
        assert_eq!(m.slots[1].name, "music/b.mls");
        assert!(m.slots[1].queued.is_empty());
        // The new list's default volume 50 at once (tpsfi=0).
        assert_eq!(m.slots[1].cur, 50.0);
    }

    #[test]
    fn stop_without_fade_out_closes_at_once() {
        let mut b = fake(&[("r.mls", "respire01.ogg vol=100 tpsfi=0 tpsfo=0\r\n")]);
        let mut m = Music2004::default();
        m.start_musik_value(&mut b, "couche4 r");
        assert!(m.slots[3].stream);
        m.stop_musik_value(&mut b, "4,r)");
        assert!(!m.slots[3].stream);
        assert!(!m.slots[3].loaded);
        assert!(m.slots[3].tracks.is_empty());
        assert_eq!(m.slots[3].name, "music/r.mls");
        assert_eq!(b.volume[3], 0);
        // Outside 1..5: nothing.
        assert!(!m.stop_musik(&mut b, 0, true));
        assert!(!m.stop_musik(&mut b, 6, true));
    }

    #[test]
    fn a_missing_list_starts_nothing() {
        let mut b = fake(&[]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "none");
        assert!(b.opened.is_empty());
        assert!(!m.slots[0].stream);
    }

    #[test]
    fn a_couche_outside_1_to_5_is_the_first() {
        let mut b = fake(&[("a.mls", "a.ogg\r\n")]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 9, "a");
        assert_eq!(b.opened[0].0, 0);
    }

    #[test]
    fn a_track_of_its_own_list_repeats_through_fmod() {
        let mut b = fake(&[("a.mls", "a.ogg tpsfi=0\r\n")]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "a");
        assert_eq!(b.loops[0], -1);
        m.sync(&mut b, 0, Sync::EndOfSong);
        m.tick(&mut b);
        // Armed again, the same stream.
        assert_eq!(b.opened.len(), 1);
        assert_eq!(b.armed, vec![0, 0]);
        assert!(!m.slots[0].end);
    }

    #[test]
    fn respire_plays_its_first_track_once_then_the_second_forever() {
        let mut b = fake(&[(
            "respire.mls",
            "respire01.ogg vol=100 tpsfi=0 tpsfo=0\r\n999xrespire02.ogg vol=100 tpsfi=0 tpsfo=0\r\n",
        )]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 4, "respire.mls");
        // Two tracks, one play: no FMOD loop.
        assert_eq!(b.loops[3], 0);
        m.sync(&mut b, 3, Sync::EndOfSong);
        m.tick(&mut b);
        assert_eq!(b.closed, vec![3]);
        assert_eq!(b.opened[1], (3, "music/respire02.ogg".to_string()));
        assert_eq!(b.loops[3], -1);
        assert_eq!(m.slots[3].repeat, FOREVER);
        for _ in 0..3 {
            m.sync(&mut b, 3, Sync::EndOfSong);
            m.tick(&mut b);
        }
        assert_eq!(b.opened.len(), 2);
        assert_eq!(m.slots[3].repeat, FOREVER);
    }

    #[test]
    fn a_longer_list_chains_from_the_stored_index() {
        let mut b = fake(&[(
            "foret_arbre.mls",
            "berimbau.ogg vol=70 tpsfi=2000 tpsfo=2000\r\nForet03.ogg vol=70\r\nForet04.ogg\r\nForet03.ogg\r\n",
        )]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "foret_arbre");
        for _ in 0..3 {
            m.sync(&mut b, 0, Sync::EndOfSong);
            m.tick(&mut b);
        }
        let files: Vec<&str> = b.opened.iter().map(|(_, f)| f.as_str()).collect();
        assert_eq!(
            files,
            [
                "music/berimbau.ogg",
                "music/Foret03.ogg",
                "music/Foret03.ogg",
                "music/Foret03.ogg"
            ]
        );
        assert_eq!(m.slots[0].index, 0);
    }

    #[test]
    fn a_repeat_count_arms_the_stream_until_it_runs_out() {
        let mut b = fake(&[("l.mls", "3xa.ogg tpsfi=0\r\nb.ogg tpsfi=0\r\n")]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "l");
        assert_eq!(b.loops[0], 2);
        for _ in 0..2 {
            m.sync(&mut b, 0, Sync::EndOfSong);
            m.tick(&mut b);
        }
        assert_eq!(b.opened.len(), 1);
        assert_eq!(m.slots[0].repeat, 1);
        m.sync(&mut b, 0, Sync::EndOfSong);
        m.tick(&mut b);
        assert_eq!(b.opened[1].1, "music/b.ogg");
    }

    #[test]
    fn the_end_flag_needs_a_loaded_list() {
        let mut b = fake(&[("a.mls", "a.ogg tpsfi=0\r\nb.ogg\r\n")]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "a");
        m.slots[0].loaded = false;
        m.sync(&mut b, 0, Sync::Null);
        m.tick(&mut b);
        assert_eq!(b.opened.len(), 1);
        // Taken all the same.
        assert!(!m.slots[0].end);
    }

    #[test]
    fn a_random_list_takes_rand_modulo_its_length() {
        let mut b = fake(&[("r.mls", "RND\r\na.ogg\r\nb.ogg\r\nc.ogg\r\n")]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "r");
        // 41 % 3 = 2.
        assert_eq!(m.slots[0].index, 2);
        assert_eq!(b.opened[0].1, "music/c.ogg");
    }

    #[test]
    fn sync_true_fades_down_then_closes_and_ends() {
        let mut b = fake(&[("a.mls", "a.ogg vol=60 tpsfi=0 tpsfo=200\r\nb.ogg\r\n")]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "a");
        m.sync(&mut b, 0, Sync::True);
        assert_eq!(m.slots[0].fade, Fade::Down);
        // 200 ms = 3 ticks; the last closes, sets the end flag, which the
        // same tick takes: the next track.
        let n = ticks_until(&mut m, &mut b, |_, b| !b.closed.is_empty());
        assert!((3..=4).contains(&n), "{n}");
        assert_eq!(b.closed, vec![0, 0]);
        assert_eq!(b.opened[1].1, "music/b.ogg");
    }

    #[test]
    fn fademusik_jumps_at_its_first_tick() {
        let mut b = fake(&[("a.mls", "a.ogg vol=80 tpsfi=0\r\n")]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "a");
        m.fade_musik_value(&mut b, "1,20,3000");
        assert_eq!(m.slots[0].fade, Fade::Up);
        m.tick(&mut b);
        assert_eq!((m.slots[0].fade, m.slots[0].cur), (Fade::None, 20.0));
        m.fade_musik(&mut b, 1, 60, 3000);
        assert_eq!(m.slots[0].fade, Fade::Down);
        m.tick(&mut b);
        assert_eq!((m.slots[0].fade, m.slots[0].cur), (Fade::None, 60.0));
    }

    #[test]
    fn the_factor_fades_and_scales_the_volume() {
        let mut b = fake(&[("a.mls", "a.ogg vol=100 tpsfi=0\r\n")]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "a");
        m.duck(0.0, 1000);
        let n = ticks_until(&mut m, &mut b, |m, _| !m.factor_fade);
        assert!((15..=16).contains(&n), "{n}");
        assert_eq!(m.factor, 0.0);
        assert_eq!(b.volume[0], 0);
        m.duck(1.0, 2000);
        let n = ticks_until(&mut m, &mut b, |m, _| !m.factor_fade);
        assert!((30..=31).contains(&n), "{n}");
        assert_eq!(m.factor, 1.0);
        assert_eq!(b.volume[0], 249);
    }

    #[test]
    fn the_volumes_clamp_and_apply() {
        let mut b = fake(&[("a.mls", "a.ogg vol=100 tpsfi=0\r\n")]);
        let mut m = Music2004::default();
        m.start_musik(&mut b, 1, "a");
        m.set_music_volume(&mut b, 50.0);
        // ftol(50 · 0.01 · 99 · 0.01 · 100 · 2.55) = 126.
        assert_eq!(b.volume[0], 126);
        m.set_global_volume(&mut b, 150.0);
        assert_eq!(m.global, 99.0);
        m.set_music_volume(&mut b, -3.0);
        assert_eq!(b.volume[0], 0);
    }

    #[test]
    fn a_track_default_is_volume_50_and_2000_ms_fades() {
        let t = Playlist::parse(b"x.ogg\r\n").tracks;
        assert_eq!(
            (t[0].volume, t[0].fade_in_ms, t[0].fade_out_ms, t[0].repeat),
            (50, 2000, 2000, 1)
        );
        assert_eq!(t[0].mi, BTreeSet::new());
    }

    #[test]
    fn ogg_length_is_the_last_granule() {
        let mut d = vec![0u8; 40];
        d.extend_from_slice(b"OggS\0\x04");
        d.extend_from_slice(&1234u64.to_le_bytes());
        d.extend_from_slice(&[0u8; 20]);
        assert_eq!(ogg_frames(&d), 1234);
    }
}

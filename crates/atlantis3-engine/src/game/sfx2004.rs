//! *Atlantis Evolution*'s 2004 sound effects: the `AtlSound` layer over FMOD
//! 3 (atlantis4-re `subsystems/cd2004-sfx.md`; addresses are
//! atlantis4-cd-en-3022848's).
//!
//! - `AtlSound_LoadSfx(path, loop, kind, 3d)` (FUN_0042ef10): the path with
//!   `ogg` in place of its last three characters (then `wav`), a sample of
//!   the map 0x006ea020 (keyed by the path) unless it is there:
//!   `FSOUND_Sample_Load(-1, file, 3d ? 0x41000 : 0x2000)` — FMOD's
//!   `FSOUND_HW3D | FSOUND_FORCEMONO` or `FSOUND_2D`;
//! - `AtlSound_PlaySfx(path)` (FUN_0042f6d0): kind 2 on FMOD channel 7, any
//!   other on channel 6, at `ftol(ftol(G · 0.01 · L) · 2.55)` (G the
//!   options' `GlobalVolume`, L `VoicesVolume` for channel 7, else
//!   `SFXVolume`): one channel for every `PLAYSOUND`, a new sound cutting
//!   the last;
//! - `AtlSound_PlaySfx3D(path, pos, k)` (FUN_0042f820): the first free of
//!   four 3D channels (0x006ea2a8 + 0x1c i: busy byte, name, FMOD channel
//!   +0x18; none: `Pas de canal 3D dispo`), min / max distance 4 / 10000,
//!   at `ftol(ftol(G · 0.01 · SFXVolume) · k)` — **no 2.55**;
//! - `IsPlaying` (FUN_0042ffd0): a 3D sound that stopped frees its channel
//!   there; `Stop` (FUN_0042fc70), `RemoveSfx` (FUN_0042fdd0: stop, then
//!   the sample freed).
//!
//! Port: the samples are the CD's Ogg files decoded whole by the
//! workspace's Vorbis decoder; FMOD's volume 0..255 is the gain `v / 255`
//! (as [`crate::music2004`], [INFERRED]); FMOD channel 6 is the mix's
//! [`CHANNEL_SOUND`], 7 [`CHANNEL_VOICE`], the 3D channels
//! [`CHANNEL_SFX3D_2004`]. A sound plays for its file's length from its
//! start (the model of [`super::sfx`]). `FSOUND_FORCEMONO` is the mean of
//! the two sides [UNKNOWN: FMOD's own mix not read].

use std::collections::HashMap;
use std::sync::Arc;

use atlantis3_data::evolution2004_sfx::{anim_sound_names, ogg_name};

use super::{Decoded, Game};
use crate::audio::{CHANNEL_SFX3D_2004, CHANNEL_SOUND, CHANNEL_VOICE, SFX3D_SLOTS};

/// A sample of `AtlSound`'s map.
#[derive(Clone, Debug)]
struct Sample {
    /// +0x38: 0 or 1 the effects, 2 the voices.
    kind: i32,
    /// +0x44: the FSOUND sample (`None`: the load failed).
    pcm: Option<Decoded>,
    /// +0x4c: the 3D channel it last played on.
    slot: Option<usize>,
}

/// The sound layer's state.
#[derive(Clone, Debug, Default)]
pub(crate) struct Sfx2004 {
    samples: HashMap<String, Sample>,
    /// FMOD channels 6 and 7: when their sound ends.
    ends: [Option<f64>; 2],
    /// The 3D channels: busy (with the end of their sound) or free.
    slots: [Option<f64>; SFX3D_SLOTS],
    /// The place animations' sounds started (the map 0x006fcc08, by path).
    anim: Vec<String>,
}

/// FMOD's volume as a gain.
fn fmod_gain(v: i32) -> f32 {
    v.clamp(0, 255) as f32 / 255.0
}

/// A decoded sound's length in seconds.
fn seconds((rate, ch, pcm): &Decoded) -> f64 {
    pcm.len() as f64 / f64::from((*ch).max(1)) / f64::from((*rate).max(1))
}

impl Game {
    /// The data path of an `AtlSound` path (`sfx\name`).
    fn sfx_data_path(path: &str) -> String {
        path.replace('\\', "/")
    }

    /// `PLAYSOUND=<name>` (@ 0x0044ea29..0x0044eb2d): the `#SONS#` file
    /// (an unknown name: `DefName_GetIdName` logs it, here nothing plays),
    /// `AtlSound_LoadSfx(sfx\file, 0, 0, 0)`, `AtlSound_PlaySfx`.
    pub(crate) fn play_sound_2004_name(&mut self, text: &str) {
        let Some(path) = self.sons_path(text) else {
            return;
        };
        self.load_sfx_2004(&path, 0, false);
        self.play_sfx_2004(&path);
    }

    /// `STOPSOUND=<name>` (@ 0x0044eb94..0x0044ec2c): the same file,
    /// `AtlSound_RemoveSfx`.
    pub(crate) fn stop_sound_2004_name(&mut self, text: &str) {
        let Some(path) = self.sons_path(text) else {
            return;
        };
        self.remove_sfx_2004(&path);
    }

    /// `sfx\<file>` of a `#SONS#` name (the name's letters, digits and
    /// `_`, as `DefName_GetIdName` FUN_00466f70 measures it).
    fn sons_path(&mut self, text: &str) -> Option<String> {
        let n = text
            .bytes()
            .take_while(|&c| c.is_ascii_alphanumeric() || c == b'_')
            .count();
        let Some(file) = self.sons.file_of(&text.as_bytes()[..n]) else {
            log::warn!("DefName_GetIdName : Nom de #SONS# inconnu:{text}");
            return None;
        };
        Some(format!("sfx\\{}", String::from_utf8_lossy(file)))
    }

    /// `AtlSound_LoadSfx(path, 0, kind, 3d)` (FUN_0042ef10): whether the
    /// file was found.
    pub(crate) fn load_sfx_2004(&mut self, path: &str, kind: i32, three_d: bool) -> bool {
        self.tr(|| format!("sfx load {path} {kind} {}", u8::from(three_d)));
        if path.is_empty() {
            log::warn!("AtlSound_LoadSfx : Nom de fichier vide");
            return false;
        }
        let ogg = ogg_name(path);
        let wav = format!("{}wav", &ogg[..ogg.len() - 3]);
        let file = if self.files.size(&Self::sfx_data_path(&ogg)).is_some() {
            ogg
        } else {
            log::warn!("AtlSound_LoadSfx : le fichier {ogg} n'existe pas.");
            if self.files.size(&Self::sfx_data_path(&wav)).is_none() {
                log::warn!("AtlSound_LoadSfx : le fichier {wav} n'existe pas.");
                return false;
            }
            wav
        };
        if self.sfx2004.samples.contains_key(path) {
            return true;
        }
        let pcm = self
            .files
            .read(&Self::sfx_data_path(&file))
            .and_then(|d| atlantis4_data::vorbis::decode(&d).ok())
            .map(|(ch, rate, pcm)| {
                if three_d && ch == 2 {
                    // `FSOUND_FORCEMONO`.
                    let mono = pcm
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|&[l, r]| ((i32::from(l) + i32::from(r)) / 2) as i16)
                        .collect();
                    (rate, 1u16, Arc::new(mono))
                } else {
                    (rate, ch as u16, Arc::new(pcm))
                }
            });
        if pcm.is_none() {
            log::warn!("void AtlSound_LoadSfx(): FSOUND_Sample_Load() failed : {file}");
        }
        self.sfx2004.samples.insert(
            path.to_string(),
            Sample {
                kind,
                pcm,
                slot: None,
            },
        );
        true
    }

    /// `AtlSound_PlaySfx(path)` (FUN_0042f6d0).
    pub(crate) fn play_sfx_2004(&mut self, path: &str) {
        self.tr(|| format!("sfx play {path}"));
        let Some(s) = self.sfx2004.samples.get(path) else {
            log::warn!(
                "void AtlSound_PlaySfx(const CString& p_Name) : le son {path} n'existe pas."
            );
            return;
        };
        let (fmod, channel, level) = if s.kind == 2 {
            (1, CHANNEL_VOICE, self.startup.config.voices_volume)
        } else {
            (0, CHANNEL_SOUND, self.startup.config.sfx_volume)
        };
        let Some(pcm) = s.pcm.clone() else {
            log::warn!("AtlSound_PlaySfx({path}) : Not OK");
            return;
        };
        // @ 0x0042f787..0x0042f7b4.
        let v = (f64::from(self.level(level)) * f64::from(2.55f32)) as i32;
        let g = fmod_gain(v);
        self.sfx2004.ends[fmod] = Some(self.now + seconds(&pcm));
        let (rate, ch, data) = pcm;
        self.audio.play(channel, rate, ch, data, false, [g, g]);
    }

    /// `FSOUND_IsPlaying` of FMOD channel 6 or 7.
    fn fmod_channel_playing(&self, fmod: usize) -> bool {
        self.sfx2004.ends[fmod].is_some_and(|e| self.now < e)
    }

    /// `IsLoaded(path)` (FUN_0042ff50).
    fn sfx_loaded_2004(&self, path: &str) -> bool {
        self.sfx2004.samples.contains_key(path)
    }

    /// `IsPlaying(path)` (FUN_0042ffd0): kind 1 a 3D channel, which a
    /// stopped sound frees.
    fn sfx_playing_2004(&mut self, path: &str) -> bool {
        let Some(s) = self.sfx2004.samples.get(path) else {
            return false;
        };
        match s.kind {
            0 => self.fmod_channel_playing(0),
            2 => self.fmod_channel_playing(1),
            1 => {
                let Some(k) = s.slot else {
                    return false;
                };
                let playing = self.sfx2004.slots[k].is_some_and(|e| self.now < e);
                if !playing {
                    self.sfx2004.slots[k] = None;
                    if let Some(s) = self.sfx2004.samples.get_mut(path) {
                        s.slot = None;
                    }
                }
                playing
            }
            _ => false,
        }
    }

    /// `Stop(path)` (FUN_0042fc70).
    fn stop_sfx_2004(&mut self, path: &str) {
        let Some((kind, slot)) = self.sfx2004.samples.get(path).map(|s| (s.kind, s.slot)) else {
            return;
        };
        match kind {
            0 => self.audio.stop(CHANNEL_SOUND),
            2 => self.audio.stop(CHANNEL_VOICE),
            1 => {
                if let Some(k) = slot {
                    self.audio.stop(CHANNEL_SFX3D_2004 + k);
                    self.sfx2004.slots[k] = None;
                    if let Some(s) = self.sfx2004.samples.get_mut(path) {
                        s.slot = None;
                    }
                }
            }
            _ => {}
        }
        match kind {
            0 => self.sfx2004.ends[0] = None,
            2 => self.sfx2004.ends[1] = None,
            _ => {}
        }
    }

    /// `AtlSound_RemoveSfx(path)` (FUN_0042fdd0): stopped, then freed.
    pub(crate) fn remove_sfx_2004(&mut self, path: &str) {
        self.tr(|| format!("sfx remove {path}"));
        if !self.sfx_loaded_2004(path) {
            return;
        }
        self.stop_sfx_2004(path);
        self.sfx2004.samples.remove(path);
    }

    /// `AtlSound_PlaySfx3D(path, pos, k)` (FUN_0042f820) with the sound at
    /// the listener (the place animations' only case on the CD, below).
    fn play_sfx3d_2004(&mut self, path: &str, k: f32) {
        self.tr(|| format!("sfx play3d {path}"));
        let Some(pcm) = self.sfx2004.samples.get(path).map(|s| s.pcm.clone()) else {
            log::warn!("void AtlSound_PlaySfx3D() : le son {path} n'existe pas.");
            return;
        };
        let Some(i) = self.sfx2004.slots.iter().position(|s| s.is_none()) else {
            log::warn!("Pas de canal 3D dispo pour le son \"{path}\"");
            return;
        };
        let end = pcm.as_ref().map_or(self.now, |p| self.now + seconds(p));
        self.sfx2004.slots[i] = Some(end);
        if let Some(s) = self.sfx2004.samples.get_mut(path) {
            s.slot = Some(i);
        }
        let Some((rate, ch, data)) = pcm else {
            log::warn!("AtlSound_PlaySfx3D({path}) : Not OK");
            return;
        };
        // @ 0x0042f9a1..0x0042f9d1: no 2.55 factor.
        let level = self.startup.config.sfx_volume;
        let v = (f64::from(self.level(level)) * f64::from(k)) as i32;
        let g = fmod_gain(v);
        self.audio
            .play(CHANNEL_SFX3D_2004 + i, rate, ch, data, false, [g, g]);
    }

    /// The place animations' sounds of a draw (FUN_00444e70 → FUN_00439fc0
    /// @ 0x0043a04d..0x0043a7db), before the sprites: each shown slot in
    /// mode 1 at frame 0 tries `sfx\<first 10 characters>.ogg`, then
    /// `sfx\<first 4>.ogg`. The first that exists is loaded (kind 1; 2D for
    /// the 10-character name, 3D for the 4-character one) unless it is,
    /// else stopped when it plays; then played through `PlaySfx3D` with
    /// k = 1.0 and remembered (0x006fcc08). The 4-character sound is placed
    /// by `AtlBruitages`' tables (FUN_00405140, FUN_00405400), which the CD
    /// leaves empty (no `[WarpList]` file in `Scenar\`): it plays at the
    /// listener, the listener at the origin. While a dialog runs (0x006e6f58,
    /// set by `DIAL` FUN_00410110, cleared at its end FUN_004111e0), every
    /// draw of a shown mode-1 slot stops both sounds instead (@ 0x0043a7db..).
    /// A slot that stays on frame 0 starts its sound again at each draw
    /// (the original's code; the step changes the frame by the clock).
    pub(crate) fn anim_sounds_2004(&mut self) {
        let dialog = self.dialog_running();
        let slots: Vec<(String, i32)> = self
            .slots
            .slots
            .iter()
            .filter(|s| s.shown && s.mode == 1)
            .map(|s| (s.name.clone(), s.frame))
            .collect();
        for (name, frame) in slots {
            let (long, short) = anim_sound_names(&name);
            let (long, short) = (format!("sfx\\{long}"), format!("sfx\\{short}"));
            if dialog {
                for p in [&short, &long] {
                    if self.sfx_playing_2004(p) {
                        self.stop_sfx_2004(p);
                    }
                }
                continue;
            }
            if frame != 0 {
                continue;
            }
            let path = if self.files.size(&Self::sfx_data_path(&long)).is_some() {
                (long, false)
            } else if self.files.size(&Self::sfx_data_path(&short)).is_some() {
                (short, true)
            } else {
                continue;
            };
            let (path, three_d) = path;
            if !self.sfx_loaded_2004(&path) {
                self.load_sfx_2004(&path, 1, three_d);
            } else if self.sfx_playing_2004(&path) {
                self.stop_sfx_2004(&path);
                self.sfx2004.anim.retain(|p| p != &path);
            }
            self.play_sfx3d_2004(&path, 1.0);
            if !self.sfx2004.anim.contains(&path) {
                self.sfx2004.anim.push(path);
            }
        }
    }

    /// FUN_00439dc0, at a film's start (`AtlBink_start` FUN_00401d30
    /// @ 0x004026a2): each remembered place animation sound that plays
    /// stopped, the list emptied.
    pub(crate) fn anim_sounds_stop_2004(&mut self) {
        for p in std::mem::take(&mut self.sfx2004.anim) {
            if self.sfx_playing_2004(&p) {
                self.stop_sfx_2004(&p);
            }
        }
    }

    /// Tests: whether an `AtlSound` path's sample is loaded and plays.
    #[doc(hidden)]
    pub fn sfx_2004_state(&mut self, path: &str) -> (bool, bool) {
        (self.sfx_loaded_2004(path), self.sfx_playing_2004(path))
    }
}

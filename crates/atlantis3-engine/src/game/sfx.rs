//! The C code's sound objects and the `PLAYVOICE` voice slots.
//!
//! The puzzles talk to the Cryo Sound Library by file name: `LoadSfx(name,
//! byte, class)` (@ 0x004329e0), `Play(name)` (@ 0x00433140),
//! `IsPlaying(name)` (@ 0x00433780) and `Stop(name)` (@ 0x004335f0). Class
//! 1 is the effects' volume, class 2 the voices' (@ 0x00433bc8). A sound
//! plays from its `Play` for its file's length (frames / rate of the APC
//! header): the model the emulation goldens use for DirectSound's own end
//! report [INFERRED: DirectSound may report the end a buffer later].
//!
//! `PLAYVOICE` (@ 0x0040da70(dialog, line, r, g, b)) takes the first free
//! of three voice slots (@ 0x0074f248, 0x38 bytes: text, voice name,
//! colour, busy; all busy: nothing happens), names the voice of the
//! dialog's line (`GetApcNameFromDial` @ 0x004658e0: `loc\voix\<Langue>\
//! <world>\<word>.apc`) and plays it with `LoadSfx(voice, 0, 2)` and
//! `Play`. The flip (@ 0x0040de60) frees a busy slot whose voice no longer
//! plays; @ 0x0040dcc0(name) stops a busy slot's voice by name and frees
//! the slot.
//!
//! Tests replaying the whole-story harness use its model instead
//! ([`Game::story_sound_model`]): `PLAYVOICE` stubbed (nothing plays), a
//! sound started by `Play` playing until a stop-all, which the walks never
//! reach.

use super::Game;
use crate::audio::{csm_gain, CHANNEL_OBJECTS, OBJECT_CHANNELS};

/// A sound loaded by `LoadSfx`.
#[derive(Clone, Debug)]
struct SfxObject {
    /// The name as the C code gives it (`sfx\AmbMMA.apc`); compared
    /// without case.
    name: String,
    /// 1: effects, 2: voices.
    class: i32,
    /// The file's length, `None` when it is missing.
    seconds: Option<f64>,
    /// The time of the last `Play` (`None`: stopped or never played).
    started: Option<f64>,
    /// The mix channel it last played on.
    channel: Option<usize>,
}

/// A `PLAYVOICE` slot (@ 0x0074f248 + 0x38 k). Its line's text is not
/// drawn by the port yet.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub(crate) struct VoiceSlot {
    pub busy: bool,
    /// The voice's name (+0x14).
    pub name: String,
    /// The dialog line (its text, +0x00, is drawn while the voice plays)
    /// and the colour (+0x28..+0x30).
    pub dialog: i32,
    pub line: i32,
    pub rgb: [i32; 3],
}

/// The sound objects and the voice slots.
#[derive(Clone, Debug, Default)]
pub(crate) struct CSounds {
    objects: Vec<SfxObject>,
    pub voices: [VoiceSlot; 3],
}

impl Game {
    fn sfx_index(&self, name: &str) -> Option<usize> {
        self.csounds
            .objects
            .iter()
            .position(|o| o.name.eq_ignore_ascii_case(name))
    }

    /// `LoadSfx(name, byte, class)` (@ 0x004329e0): false for an empty
    /// name (its first test).
    pub(crate) fn sfx_load(&mut self, name: &str, byte: i32, class: i32) -> bool {
        self.tr(|| {
            format!(
                "snd load {} {byte} {class}",
                if name.is_empty() { "-" } else { name }
            )
        });
        if name.is_empty() {
            return false;
        }
        let path = name.replace('\\', "/");
        let seconds = self.sound(&path).map(|(rate, ch, pcm)| {
            pcm.len() as f64 / f64::from(ch.max(1)) / f64::from(rate.max(1))
        });
        match self.sfx_index(name) {
            Some(i) => {
                let o = &mut self.csounds.objects[i];
                o.class = class;
                o.seconds = seconds;
            }
            None => self.csounds.objects.push(SfxObject {
                name: name.to_string(),
                class,
                seconds,
                started: None,
                channel: None,
            }),
        }
        true
    }

    /// `Play(name)` (@ 0x00433140): from the start, on a free channel.
    pub(crate) fn sfx_play(&mut self, name: &str) {
        self.tr(|| format!("snd play {}", if name.is_empty() { "-" } else { name }));
        let Some(i) = self.sfx_index(name) else {
            return;
        };
        let now = self.now;
        // The channel it had, else one no playing object holds.
        let busy: Vec<usize> = self
            .csounds
            .objects
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i && self.sfx_playing_at(j))
            .filter_map(|(_, o)| o.channel)
            .collect();
        let channel = self.csounds.objects[i]
            .channel
            .filter(|c| !busy.contains(c))
            .or_else(|| {
                (CHANNEL_OBJECTS..CHANNEL_OBJECTS + OBJECT_CHANNELS).find(|c| !busy.contains(c))
            })
            .unwrap_or(CHANNEL_OBJECTS);
        let o = &mut self.csounds.objects[i];
        o.started = Some(now);
        o.channel = Some(channel);
        let class = o.class;
        let path = o.name.replace('\\', "/");
        if let Some((rate, ch, pcm)) = self.sound(&path) {
            let level = if class == 2 {
                self.startup.config.voices_volume
            } else {
                self.startup.config.sfx_volume
            };
            let g = csm_gain(self.level(level));
            self.audio.play(channel, rate, ch, pcm, false, [g, g]);
        }
    }

    fn sfx_playing_at(&self, i: usize) -> bool {
        let o = &self.csounds.objects[i];
        if self.story_sound_model {
            return o.started.is_some();
        }
        match (o.started, o.seconds) {
            (Some(t), Some(s)) => self.now < t + s,
            _ => false,
        }
    }

    /// `IsLoaded(name)` (@ 0x00433700): a sound object of that name whose
    /// file was found.
    pub(crate) fn sfx_loaded(&mut self, name: &str) -> bool {
        self.tr(|| format!("snd loaded {}", if name.is_empty() { "-" } else { name }));
        self.sfx_index(name)
            .is_some_and(|i| self.csounds.objects[i].seconds.is_some())
    }

    /// `IsPlaying(name)` (@ 0x00433780): never for an empty or unloaded
    /// name.
    pub(crate) fn sfx_playing(&self, name: &str) -> bool {
        self.sfx_index(name).is_some_and(|i| self.sfx_playing_at(i))
    }

    /// `Stop(name)` (@ 0x004335f0).
    pub(crate) fn sfx_stop(&mut self, name: &str) {
        self.tr(|| format!("snd stop {}", if name.is_empty() { "-" } else { name }));
        if let Some(i) = self.sfx_index(name) {
            let o = &mut self.csounds.objects[i];
            if o.started.take().is_some() {
                if let Some(c) = o.channel {
                    self.audio.stop(c);
                }
            }
        }
    }

    /// The voice file of a dialog's line (`GetApcNameFromDial`
    /// @ 0x004658e0) as the C code names it (`loc\voix\fra\bagdad\
    /// GABA0457.apc`); empty when the world's dialogs have no such line.
    pub(crate) fn dialog_voice_name(&self, dialog: i32, line: i32) -> String {
        let lang = self.startup.voice_language();
        self.dialogs
            .as_ref()
            .and_then(|d| d.entry(dialog, line).map(|e| d.voice_path(e, &lang)))
            .map(|p| p.replace('/', "\\"))
            .unwrap_or_default()
    }

    /// `PLAYVOICE` (@ 0x0040da70(dialog, line, r, g, b)).
    pub(crate) fn play_voice_c(&mut self, dialog: i32, line: i32, r: i32, g: i32, b: i32) {
        self.tr(|| format!("voice {dialog} {line} {r} {g} {b}"));
        if self.story_sound_model {
            return;
        }
        let Some(k) = self.csounds.voices.iter().position(|v| !v.busy) else {
            return;
        };
        let name = self.dialog_voice_name(dialog, line);
        self.csounds.voices[k] = VoiceSlot {
            busy: true,
            name: name.clone(),
            dialog,
            line,
            rgb: [r, g, b],
        };
        if self.sfx_load(&name, 0, 2) {
            self.sfx_play(&name);
        }
    }

    /// @ 0x0040dcc0(name): the busy voice slots playing `name` stopped and
    /// freed.
    pub(crate) fn voice_stop_named(&mut self, name: &str) {
        self.tr(|| format!("voicestop {}", if name.is_empty() { "-" } else { name }));
        for k in 0..self.csounds.voices.len() {
            let v = &self.csounds.voices[k];
            if v.busy && v.name.eq_ignore_ascii_case(name) {
                let n = v.name.clone();
                self.sfx_stop(&n);
                self.csounds.voices[k] = VoiceSlot::default();
            }
        }
    }

    /// The flip's part (@ 0x0040de60): a busy slot whose voice has stopped
    /// is freed.
    pub(crate) fn voices_release(&mut self) {
        for k in 0..self.csounds.voices.len() {
            let v = &self.csounds.voices[k];
            if v.busy && !self.sfx_playing(&v.name.clone()) {
                self.csounds.voices[k] = VoiceSlot::default();
            }
        }
    }
}

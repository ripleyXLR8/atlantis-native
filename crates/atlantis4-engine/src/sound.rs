//! The game's sound objects on top of the exact mixer ([`crate::oalmix`]),
//! as atlantis4-gog-en-2363910 drives OpenAL:
//!
//! - `TeMusic` ([`Stream`]): one source and four 250 ms buffers refilled
//!   by a 50 ms thread (`0x004f7fc0`); it serves the music (channel
//!   `music`, repeating), each `PlaySound` (`sfx`), the dialog voice
//!   (`dialog`) and a cut-scene's sound (`video`). `load` of the path
//!   already loaded changes nothing; `play` while playing does nothing;
//!   `stop` cuts, deletes the source and emits the "stopped" signal if
//!   there was a source; a repeating sound re-opens its file at the end.
//! - `TeSFX` (free sounds, `TeSoundManager::playFreeSound` @ 0x00506040):
//!   the whole file in one buffer, deleted once it has played (interface
//!   sounds, the `AddToBag` notification).
//! - `TeSoundManager` ([`Sounds`]): the channel volumes (`music`, `sfx`,
//!   `dialog`, `video`, 1.0 when unset, from `Backup/options.xml`), the
//!   master volume 1.0, and the per-frame update (@ 0x00506180) that
//!   stops each finished non-repeating stream, emitting its signal.
//!
//! `AL_GAIN` = `float((double)volume × master × channel)`. Nothing fades:
//! every change is a cut, every start is the mixer's 512-frame fade-in.
//!
//! The mix itself runs in [`AudioCore`], shared with the sound output:
//! blocks of [`oalmix::BLOCK`] frames at 44,100 Hz stereo, pulled by the
//! device (or by [`Sounds::update`] on the game clock when there is no
//! device). The streams' 50 ms thread runs on the game clock.
//!
//! Cut-scenes: a movie player calls [`Sounds::start_movie`] (the music
//! stops; the movie's sound plays on the `video` channel) and
//! [`Sounds::end_movie`] (that sound stops; the music restarts from its
//! beginning), as `PlayMovie`'s worker (@ 0x005232b0) and the movie's end
//! (@ 0x00524740) do.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use atlantis4_data::vorbis::VorbisStream;
use atlantis4_data::A4Files;

use crate::oalmix::{self, BufferId, Mixer, SourceId, State};

/// The 44,100 Hz stereo mix, shared between the game and the output.
pub struct AudioCore {
    pub mixer: Mixer,
    /// Mixed samples not yet taken by the output (interleaved stereo).
    ring: VecDeque<i16>,
    /// Frames mixed since the start.
    pub frames: u64,
}

impl AudioCore {
    fn new() -> Self {
        AudioCore {
            mixer: Mixer::new(),
            ring: VecDeque::new(),
            frames: 0,
        }
    }

    fn mix_block(&mut self) {
        let mut v = Vec::with_capacity(2 * oalmix::BLOCK);
        self.mixer.mix(oalmix::BLOCK, &mut v);
        self.ring.extend(v);
        self.frames += oalmix::BLOCK as u64;
    }

    /// Fills `out` with interleaved stereo frames at 44,100 Hz, mixing
    /// whole blocks as needed.
    pub fn pull(&mut self, out: &mut [i16]) {
        while self.ring.len() < out.len() {
            self.mix_block();
        }
        let n = out.len();
        for (o, s) in out.iter_mut().zip(self.ring.drain(..n)) {
            *o = s;
        }
    }
}

/// The handle the sound output keeps.
pub type Core = Arc<Mutex<AudioCore>>;

fn lock(core: &Core) -> MutexGuard<'_, AudioCore> {
    core.lock().unwrap_or_else(|e| e.into_inner())
}

/// What a stopped sound means for the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoundEvent {
    /// A one-shot `PlaySound` stopped: Lua `OnSoundStopped(name)`
    /// (@ 0x00524de0).
    SoundStopped(String),
    /// The dialog voice stopped (`Dialog2::onVoiceStopped` @ 0x0052c220).
    VoiceStopped,
}

/// The file's samples, chunk by chunk.
enum Decoder {
    Vorbis(Box<VorbisStream>),
    Wav {
        data: Vec<u8>,
        pos: usize,
        end: usize,
        channels: u8,
        rate: u32,
    },
}

impl Decoder {
    /// `0x004c2590`: Vorbis for `.ogg`, else WAVE.
    fn open(files: &A4Files, path: &str) -> Option<Decoder> {
        let data = files.read(path)?;
        if path.to_ascii_lowercase().ends_with(".ogg") {
            return VorbisStream::new(data)
                .ok()
                .map(|v| Decoder::Vorbis(Box::new(v)));
        }
        let i = atlantis4_data::audio::wav_info(&data).ok()?;
        if i.format != 1 || i.bits != 16 || i.channels == 0 || i.channels > 2 {
            return None;
        }
        Some(Decoder::Wav {
            pos: i.data_offset,
            end: i.data_offset + i.data_len,
            channels: i.channels as u8,
            rate: i.sample_rate,
            data,
        })
    }

    fn format(&self) -> (u8, u32) {
        match self {
            Decoder::Vorbis(v) => (v.channels() as u8, v.rate()),
            Decoder::Wav { channels, rate, .. } => (*channels, *rate),
        }
    }

    /// One chunk (`0x0048e1c0`): `rate × channels × 2 / 4` bytes in whole
    /// frames, 250 ms, read until full or the end; empty at the end.
    fn chunk(&mut self) -> Vec<i16> {
        let (ch, rate) = self.format();
        let ch = usize::from(ch.max(1));
        let frames = (rate as usize * ch * 2 / 4) / (2 * ch);
        let mut out = Vec::with_capacity(frames * ch);
        match self {
            Decoder::Vorbis(v) => {
                while out.len() < frames * ch {
                    let left = frames - out.len() / ch;
                    if v.read(&mut out, left) == 0 {
                        break;
                    }
                }
            }
            Decoder::Wav { data, pos, end, .. } => {
                let take = (frames * ch * 2).min(*end - *pos) / (2 * ch) * (2 * ch);
                out.extend(
                    data[*pos..*pos + take]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| i16::from_le_bytes(*b)),
                );
                *pos += take;
            }
        }
        out
    }

    /// The whole file (`0x0048e000`: the chunks concatenated).
    fn all(&mut self) -> Vec<i16> {
        let mut v = Vec::new();
        loop {
            let c = self.chunk();
            if c.is_empty() {
                return v;
            }
            v.extend(c);
        }
    }
}

/// The file a sound path stands for (`TeMusic::load` @ 0x004f8780): the
/// file as named, else `dir/<language>/file`, else `dir/en/file`; the
/// last candidate when none exists (opening it then fails).
fn resolve(files: &A4Files, lang: &str, path: &str) -> String {
    if files.exists(path) {
        return path.to_string();
    }
    let (dir, file) = match path.rfind('/') {
        Some(i) => (&path[..i + 1], &path[i + 1..]),
        None => ("", path),
    };
    let local = format!("{dir}{lang}/{file}");
    if files.exists(&local) {
        return local;
    }
    format!("{dir}en/{file}")
}

/// `AL_GAIN` of an object: `float((double)v × m × c)` on the x87 at
/// 53-bit precision (@ 0x004f7a49).
pub fn gain(volume: f32, master: f32, channel: f32) -> f32 {
    (f64::from(volume) * f64::from(master) * f64::from(channel)) as f32
}

/// The thread's period (`Sleep(0x32)` @ 0x004f84c6).
const TICK_MS: f64 = 50.0;

/// A `TeMusic`.
pub struct Stream {
    /// The path as last loaded (`+0x38`).
    raw: String,
    /// The resolved path (`+0x48`).
    resolved: String,
    /// `+0x58`.
    pub repeat: bool,
    /// `+0x94`.
    volume: f32,
    channel: &'static str,
    /// `+0x90`.
    playing: bool,
    /// In the manager's list.
    registered: bool,
    source: Option<SourceId>,
    buffers: Vec<BufferId>,
    decoder: Option<Decoder>,
    /// `+0x88`, `+0x8c`.
    next: usize,
    queued: usize,
    /// The thread runs; its next wake on the game clock.
    thread: bool,
    next_tick: f64,
}

impl Stream {
    fn new(channel: &'static str, repeat: bool) -> Stream {
        Stream {
            raw: String::new(),
            resolved: String::new(),
            repeat,
            volume: 1.0,
            channel,
            playing: false,
            registered: false,
            source: None,
            buffers: Vec::new(),
            decoder: None,
            next: 0,
            queued: 0,
            thread: false,
            next_tick: 0.0,
        }
    }

    /// The path as last loaded.
    pub fn path(&self) -> &str {
        &self.raw
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }
}

/// A `TeSFX` of the free list.
struct Sfx {
    path: String,
    source: SourceId,
    buffer: BufferId,
}

/// Which stream, for the manager's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Music,
    Voice,
    Video,
    Play(u64),
}

/// A `PlaySound` object (the 0xbc-byte subclass, list `game+0xd34`).
struct Played {
    id: u64,
    name: String,
    stream: Stream,
    /// One-shots are connected to `OnSoundStopped` (@ 0x00524de0).
    notify: bool,
}

/// `TeSoundManager` and the game's sound objects.
pub struct Sounds {
    core: Core,
    lang: String,
    volumes: HashMap<String, f32>,
    master: f32,
    /// `Application+0`.
    pub music: Stream,
    /// `Dialog2+0x718`.
    pub voice: Stream,
    /// `game+0x3f8c`.
    pub video: Stream,
    played: Vec<Played>,
    next_played: u64,
    free: Vec<Sfx>,
    /// The manager's `TeMusic` list (+0x38), in order.
    list: Vec<Which>,
    events: Vec<SoundEvent>,
    /// No output pulls the mix: [`Sounds::update`] mixes on the game clock.
    pub headless: bool,
    now_ms: f64,
}

impl Sounds {
    /// The sound manager, every channel at 1.0 (no `options.xml`).
    pub fn new(lang: &str) -> Sounds {
        Sounds {
            core: Arc::new(Mutex::new(AudioCore::new())),
            lang: lang.to_string(),
            volumes: HashMap::new(),
            master: 1.0,
            music: Stream::new("music", true),
            voice: Stream::new("dialog", false),
            video: Stream::new("video", false),
            played: Vec::new(),
            next_played: 0,
            free: Vec::new(),
            list: Vec::new(),
            events: Vec::new(),
            headless: true,
            now_ms: 0.0,
        }
    }

    /// The mix, for the sound output (which then sets `headless` false).
    pub fn core(&self) -> Core {
        self.core.clone()
    }

    /// `channelVolume` (@ 0x00505dc0): 1.0 when unset.
    pub fn channel_volume(&self, name: &str) -> f32 {
        self.volumes.get(name).copied().unwrap_or(1.0)
    }

    /// `setChannelVolume` (@ 0x00505e00): stored, and every object with a
    /// source re-applies its gain at once.
    pub fn set_channel_volume(&mut self, name: &str, v: f32) {
        self.volumes.insert(name.to_string(), v);
        let mut gains = Vec::new();
        for s in [&self.music, &self.voice, &self.video]
            .into_iter()
            .chain(self.played.iter().map(|p| &p.stream))
        {
            if let Some(src) = s.source {
                gains.push((
                    src,
                    gain(s.volume, self.master, self.channel_volume(s.channel)),
                ));
            }
        }
        let sfx = gain(1.0, self.master, self.channel_volume("sfx"));
        for f in &self.free {
            gains.push((f.source, sfx));
        }
        let mut core = lock(&self.core);
        for (s, g) in gains {
            core.mixer.set_gain(s, g);
        }
    }

    /// The volumes of `Backup/options.xml` (@ 0x00577530): the `value`
    /// attribute of `SFXVolume`, `MusicVolume`, `DialogVolume` and
    /// `VideoVolume`, read with `atof`.
    pub fn load_options(&mut self, xml: &str) {
        for (elem, ch) in [
            ("SFXVolume", "sfx"),
            ("MusicVolume", "music"),
            ("DialogVolume", "dialog"),
            ("VideoVolume", "video"),
        ] {
            let Some(at) = xml.find(&format!("<{elem}")) else {
                continue;
            };
            let rest = &xml[at..];
            let rest = &rest[..rest.find('>').unwrap_or(rest.len())];
            let Some(v) = rest.find("value=\"").map(|i| &rest[i + 7..]) else {
                continue;
            };
            let v = &v[..v.find('"').unwrap_or(v.len())];
            self.set_channel_volume(ch, atof(v));
        }
    }

    fn stream(&mut self, w: Which) -> Option<&mut Stream> {
        match w {
            Which::Music => Some(&mut self.music),
            Which::Voice => Some(&mut self.voice),
            Which::Video => Some(&mut self.video),
            Which::Play(id) => self
                .played
                .iter_mut()
                .find(|p| p.id == id)
                .map(|p| &mut p.stream),
        }
    }

    /// `TeMusic::load`: `true` when the path is set (or already was).
    fn load(&mut self, w: Which, files: &A4Files, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        if self.stream(w).is_some_and(|s| s.raw == path) {
            return true;
        }
        self.stop(w);
        let resolved = resolve(files, &self.lang, path);
        if let Some(s) = self.stream(w) {
            s.raw = path.to_string();
            s.resolved = resolved;
        }
        true
    }

    /// `TeMusic::play` (@ 0x004f8510).
    fn play(&mut self, w: Which, files: &A4Files) -> bool {
        let master = self.master;
        let volumes = self.volumes.clone();
        let now = self.now_ms;
        let core = self.core.clone();
        let Some(s) = self.stream(w) else {
            return false;
        };
        if s.playing {
            return true;
        }
        if s.resolved.is_empty() {
            return false;
        }
        {
            let mut c = lock(&core);
            s.buffers = (0..4).map(|_| c.mixer.gen_buffer()).collect();
            s.source = Some(c.mixer.gen_source());
        }
        s.next = 0;
        s.queued = 0;
        match Decoder::open(files, &s.resolved) {
            Some(d) => s.decoder = Some(d),
            None => {
                log::warn!("TeMusic:play: can't open file \"{}\"", s.resolved);
                return false;
            }
        }
        let g = gain(
            s.volume,
            master,
            volumes.get(s.channel).copied().unwrap_or(1.0),
        );
        if let Some(src) = s.source {
            lock(&core).mixer.set_gain(src, g);
        }
        s.thread = true;
        s.next_tick = now;
        s.playing = true;
        self.list.retain(|&x| x != w);
        self.list.push(w);
        if let Some(s) = self.stream(w) {
            s.registered = true;
        }
        // The thread's first pass: four chunks, then the source plays.
        self.tick(w, files);
        true
    }

    /// One pass of the streaming thread (`TeMusic::entry` @ 0x004f7fc0).
    fn tick(&mut self, w: Which, files: &A4Files) {
        let core = self.core.clone();
        let Some(s) = self.stream(w) else { return };
        if !s.thread {
            return;
        }
        let Some(src) = s.source else { return };
        {
            let mut c = lock(&core);
            let n = c.mixer.processed(src);
            c.mixer.unqueue(src, n);
            s.queued = s.queued.saturating_sub(n);
        }
        while s.queued != 4 && s.decoder.is_some() {
            let mut data = s.decoder.as_mut().map(Decoder::chunk).unwrap_or_default();
            if data.is_empty() {
                s.decoder = None;
                if s.repeat {
                    s.decoder = Decoder::open(files, &s.resolved);
                    data = s.decoder.as_mut().map(Decoder::chunk).unwrap_or_default();
                }
            }
            if !data.is_empty() {
                let (ch, rate) = s
                    .decoder
                    .as_ref()
                    .map_or((1, oalmix::RATE), Decoder::format);
                let b = s.buffers[s.next];
                let mut c = lock(&core);
                c.mixer.buffer_data(b, data, ch, rate);
                c.mixer.queue(src, &[b]);
                s.queued += 1;
                s.next = (s.next + 1) % 4;
            }
        }
        let state = lock(&core).mixer.state(src);
        if state != Some(State::Playing) {
            if s.queued == 0 {
                s.thread = false;
                s.playing = false;
                return;
            }
            lock(&core).mixer.play(src);
        }
    }

    /// `TeMusic::stop` (@ 0x004f7480): the cut; the "stopped" signal if
    /// there was a source.
    fn stop(&mut self, w: Which) {
        let core = self.core.clone();
        let Some(s) = self.stream(w) else { return };
        s.thread = false;
        s.decoder = None;
        s.playing = false;
        let Some(src) = s.source.take() else { return };
        {
            let mut c = lock(&core);
            c.mixer.delete_source(src);
            for b in s.buffers.drain(..) {
                c.mixer.delete_buffer(b);
            }
        }
        s.queued = 0;
        self.emit(w);
    }

    fn emit(&mut self, w: Which) {
        match w {
            Which::Voice => self.events.push(SoundEvent::VoiceStopped),
            Which::Play(id) => {
                if let Some(p) = self.played.iter().find(|p| p.id == id && p.notify) {
                    self.events.push(SoundEvent::SoundStopped(p.name.clone()));
                }
            }
            _ => {}
        }
    }

    /// `PlayMusic(path)` (@ 0x0052b5b0): load, play, volume 1.0. The same
    /// track goes on; another one cuts the old and starts from its
    /// beginning; a missing file leaves silence.
    pub fn play_music(&mut self, files: &A4Files, path: &str) {
        if self.load(Which::Music, files, path) {
            self.play(Which::Music, files);
        }
        self.set_volume(Which::Music, 1.0);
    }

    fn set_volume(&mut self, w: Which, v: f32) {
        let master = self.master;
        let core = self.core.clone();
        let c = {
            let Some(s) = self.stream(w) else { return };
            s.volume = v;
            s.channel
        };
        let g = gain(v, master, self.channel_volume(c));
        if let Some(src) = self.stream(w).and_then(|s| s.source) {
            lock(&core).mixer.set_gain(src, g);
        }
    }

    /// `PlaySound(name, count)` (@ 0x00524f30): 1 plays once and reports
    /// `OnSoundStopped(name)`, −1 loops; any other count does nothing.
    /// `path` is `name` normalised.
    pub fn play_sound(&mut self, files: &A4Files, name: &str, path: &str, count: i32) {
        let repeat = match count {
            1 => false,
            -1 => true,
            _ => return,
        };
        self.next_played += 1;
        let id = self.next_played;
        self.played.push(Played {
            id,
            name: name.to_string(),
            stream: Stream::new("sfx", repeat),
            notify: !repeat,
        });
        let w = Which::Play(id);
        if self.load(w, files, path) {
            self.play(w, files);
        }
    }

    /// `StopSound(path)` (@ 0x00522b60): the first `PlaySound` object of
    /// that name is stopped and removed, then every free sound of that path.
    pub fn stop_sound(&mut self, path: &str) {
        if let Some(id) = self.played.iter().find(|p| p.name == path).map(|p| p.id) {
            self.stop(Which::Play(id));
            self.list.retain(|&x| x != Which::Play(id));
            self.played.retain(|p| p.id != id);
        }
        let mut c = lock(&self.core);
        self.free.retain(|f| {
            if f.path == path {
                c.mixer.delete_source(f.source);
                c.mixer.delete_buffer(f.buffer);
                false
            } else {
                true
            }
        });
    }

    /// The node change's step 4 (`0x00526c70` @ 0x00526e26..0x00526ff6):
    /// every `PlaySound` object stopped (signals included) and the list
    /// emptied; the music, the voice and the video sound go on.
    pub fn leave_node(&mut self) {
        let ids: Vec<u64> = self.played.iter().map(|p| p.id).collect();
        for id in ids {
            self.stop(Which::Play(id));
            self.list.retain(|&x| x != Which::Play(id));
        }
        self.played.clear();
    }

    /// `Game::leave` (@ 0x00525f50): the music stopped; the scene's
    /// sounds, the voice and a movie's sound go with the scene and the
    /// dialogs `[INFERRED]`; their signals are dropped with the Lua state
    /// (closed after the scene, @ 0x00526440). The free sounds play on.
    pub fn leave_game(&mut self) {
        self.leave_node();
        for w in [Which::Music, Which::Voice, Which::Video] {
            self.list.retain(|&x| x != w);
            self.stop(w);
        }
        self.take_events();
    }

    /// `playFreeSound(path, volume)` (@ 0x00506040): a `TeSFX` decoded
    /// whole, played once, deleted when it has played. Missing files are
    /// silent.
    pub fn play_free_sound(&mut self, files: &A4Files, path: &str, volume: f32) {
        let resolved = resolve(files, &self.lang, path);
        let Some(mut d) = Decoder::open(files, &resolved) else {
            log::info!("no sound {path}");
            return;
        };
        let (ch, rate) = d.format();
        let pcm = d.all();
        let g = gain(volume, self.master, self.channel_volume("sfx"));
        let mut c = lock(&self.core);
        let buffer = c.mixer.gen_buffer();
        let source = c.mixer.gen_source();
        c.mixer.buffer_data(buffer, pcm, ch, rate);
        c.mixer.queue(source, &[buffer]);
        c.mixer.set_gain(source, g);
        c.mixer.play(source);
        drop(c);
        self.free.push(Sfx {
            path: path.to_string(),
            source,
            buffer,
        });
    }

    /// The voice of a dialog line (`Dialog2::showNext` @ 0x0052e211):
    /// `Sounds/Dialogs/<voice>`, or `silence5s.ogg` when the voice has no
    /// file in any language; its end is [`SoundEvent::VoiceStopped`].
    pub fn play_voice(&mut self, files: &A4Files, voice: &str) {
        let mut path = format!("Sounds/Dialogs/{voice}");
        if !files.exists(&resolve(files, &self.lang, &path)) {
            path = "Sounds/Dialogs/silence5s.ogg".to_string();
        }
        if self.load(Which::Voice, files, &path) {
            self.play(Which::Voice, files);
        }
    }

    /// A click on the dialog bar cuts the voice (@ 0x0052c390).
    pub fn stop_voice(&mut self) {
        self.list.retain(|&x| x != Which::Voice);
        self.stop(Which::Voice);
    }

    /// `PlayMovie`'s sound (@ 0x00523435): the music stops (a cut), the
    /// movie's sound file plays on the `video` channel, once, at volume 1.
    pub fn start_movie(&mut self, files: &A4Files, sound: Option<&str>) {
        self.list.retain(|&x| x != Which::Music);
        self.stop(Which::Music);
        if let Some(p) = sound {
            if self.load(Which::Video, files, p) {
                self.play(Which::Video, files);
            }
            self.set_volume(Which::Video, 1.0);
        }
    }

    /// The movie's end or skip (@ 0x00524740): its sound stops, then the
    /// music restarts from its beginning.
    pub fn end_movie(&mut self, files: &A4Files) {
        self.list.retain(|&x| x != Which::Video);
        self.stop(Which::Video);
        self.play(Which::Music, files);
    }

    /// Each frame, at its start: the streams' 50 ms passes due on the game
    /// clock, the mix when no output pulls it, then `TeSoundManager::update`:
    /// played free sounds deleted, finished streams stopped (their signals
    /// become [`Sounds::take_events`]).
    pub fn update(&mut self, files: &A4Files, now_ms: f64) {
        self.now_ms = now_ms;
        let ws: Vec<Which> = self.list.clone();
        for w in ws {
            loop {
                let due = self
                    .stream(w)
                    .is_some_and(|s| s.thread && s.next_tick + TICK_MS <= now_ms);
                if !due {
                    break;
                }
                if let Some(s) = self.stream(w) {
                    s.next_tick += TICK_MS;
                }
                if self.headless {
                    let t = self.stream(w).map_or(now_ms, |s| s.next_tick);
                    self.pump(t);
                }
                self.tick(w, files);
            }
        }
        if self.headless {
            self.pump(now_ms);
        }
        {
            let mut c = lock(&self.core);
            self.free.retain(|f| {
                if c.mixer.state(f.source) == Some(State::Playing) {
                    true
                } else {
                    c.mixer.delete_source(f.source);
                    c.mixer.delete_buffer(f.buffer);
                    false
                }
            });
        }
        let ws: Vec<Which> = self.list.clone();
        for w in ws {
            let done = self.stream(w).is_some_and(|s| !s.playing && !s.repeat);
            if done {
                self.list.retain(|&x| x != w);
                if let Some(s) = self.stream(w) {
                    s.registered = false;
                }
                self.stop(w);
            }
        }
    }

    /// Mixes (and drops) the frames up to `ms` on the game clock.
    fn pump(&mut self, ms: f64) {
        let target = (ms.max(0.0) * f64::from(oalmix::RATE) / 1000.0) as u64;
        let mut c = lock(&self.core);
        while c.frames < target {
            c.mix_block();
            c.ring.clear();
        }
    }

    /// The signals raised since the last call, in order.
    pub fn take_events(&mut self) -> Vec<SoundEvent> {
        std::mem::take(&mut self.events)
    }
}

/// C's `atof` on a decimal number (leading spaces, sign, digits, point,
/// exponent; the rest ignored); 0 when nothing parses.
pub(crate) fn atof(s: &str) -> f32 {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut end = 0;
    let mut seen_digit = false;
    if end < b.len() && (b[end] == b'+' || b[end] == b'-') {
        end += 1;
    }
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
        seen_digit = true;
    }
    if end < b.len() && b[end] == b'.' {
        end += 1;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
            seen_digit = true;
        }
    }
    if seen_digit && end < b.len() && (b[end] == b'e' || b[end] == b'E') {
        let mut e = end + 1;
        if e < b.len() && (b[e] == b'+' || b[e] == b'-') {
            e += 1;
        }
        if e < b.len() && b[e].is_ascii_digit() {
            while e < b.len() && b[e].is_ascii_digit() {
                e += 1;
            }
            end = e;
        }
    }
    if !seen_digit {
        return 0.0;
    }
    t[..end].parse::<f64>().map_or(0.0, |x| x as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_data::files::MemFiles;

    #[test]
    fn gains_are_one_rounding_of_the_double_product() {
        // The emulated golden (`a4_audio.txt # gain`).
        let bits = |v: f32, m: f32, c: f32| gain(v, m, c).to_bits();
        assert_eq!(bits(1.0, 1.0, 0.8), 0x3f4c_cccd);
        assert_eq!(bits(0.5, 1.0, f32::from_bits(0x3f19_999a)), 0x3e99_999a);
        assert_eq!(bits(0.7, 0.9, 0.3), 0x3e41_8937);
        assert_eq!(bits(1.0, 1.0, 0.0), 0);
        assert_eq!(atof(" 0.600000xyz"), 0.6);
        assert_eq!(atof("abc"), 0.0);
        let mut s = Sounds::new("en");
        s.load_options(
            r#"<Options><MusicVolume value="0.400000"/><SFXVolume value="1.000000"/></Options>"#,
        );
        assert_eq!(s.channel_volume("music"), 0.4);
        assert_eq!(s.channel_volume("dialog"), 1.0);
    }

    /// A 16-bit 44,100 Hz WAVE file of `frames` frames.
    fn wav(frames: usize, channels: u16) -> Vec<u8> {
        let data: Vec<u8> = (0..frames * usize::from(channels))
            .flat_map(|i| (((i % 200) as i16 - 100) * 100).to_le_bytes())
            .collect();
        let mut w = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0".to_vec();
        w.extend([1, 0]);
        w.extend(channels.to_le_bytes());
        w.extend(44_100u32.to_le_bytes());
        w.extend((44_100u32 * 2 * u32::from(channels)).to_le_bytes());
        w.extend((2 * channels).to_le_bytes());
        w.extend(16u16.to_le_bytes());
        w.extend(b"data");
        w.extend((data.len() as u32).to_le_bytes());
        w.extend(data);
        w
    }

    fn files() -> A4Files {
        let mut m = MemFiles::new();
        m.insert("Sounds/Musiques/a.wav", wav(44_100 * 3, 2));
        m.insert("Sounds/Musiques/b.wav", wav(44_100, 2));
        m.insert("Sounds/SFX/short.wav", wav(4_410, 1));
        m.insert("Sounds/SFX/loop.wav", wav(8_820, 1));
        m.insert("Sounds/Dialogs/fr/V1.wav", wav(2_000, 1));
        m.insert("Sounds/Dialogs/silence5s.wav", wav(100, 1));
        A4Files::new(Arc::new(m))
    }

    fn sources(s: &Sounds) -> usize {
        lock(&s.core).mixer.source_count()
    }

    #[test]
    fn the_same_music_goes_on_and_another_cuts() {
        let f = files();
        let mut s = Sounds::new("fr");
        s.play_music(&f, "Sounds/Musiques/a.wav");
        let first = s.music.source;
        assert!(first.is_some() && s.music.is_playing());
        s.update(&f, 500.0);
        s.play_music(&f, "Sounds/Musiques/a.wav");
        assert_eq!(s.music.source, first, "the same track is not restarted");
        s.play_music(&f, "Sounds/Musiques/b.wav");
        assert_ne!(s.music.source, first);
        assert_eq!(sources(&s), 1);
        // The music repeats: still playing long after the file's end.
        for t in 0..80 {
            s.update(&f, 600.0 + 100.0 * f64::from(t));
        }
        assert!(s.music.is_playing());
        assert!(s.take_events().is_empty());
        // A missing file: the old track stops, nothing plays.
        s.play_music(&f, "...");
        assert!(!s.music.is_playing());
    }

    #[test]
    fn a_one_shot_reports_its_end_once_and_a_node_change_stops_the_rest() {
        let f = files();
        let mut s = Sounds::new("fr");
        s.play_sound(&f, "Sounds/SFX/short.wav", "Sounds/SFX/short.wav", 1);
        s.play_sound(&f, "Sounds/SFX/loop.wav", "Sounds/SFX/loop.wav", -1);
        s.play_sound(&f, "Sounds/SFX/nothere.ogg", "Sounds/SFX/nothere.ogg", 1);
        s.play_sound(&f, "Sounds/SFX/short.wav", "Sounds/SFX/short.wav", 2);
        assert_eq!(
            sources(&s),
            3,
            "a missing file keeps its idle source; count 2 does nothing"
        );
        let mut ended_at = None;
        for t in 1..40 {
            s.update(&f, 33.0 * f64::from(t));
            let e = s.take_events();
            if !e.is_empty() {
                assert_eq!(e, [SoundEvent::SoundStopped("Sounds/SFX/short.wav".into())]);
                assert!(ended_at.is_none());
                ended_at = Some(33.0 * f64::from(t));
            }
        }
        // 4,410 frames = 100 ms, + the next 50 ms pass, + the next frame.
        let t = ended_at.expect("the one-shot ended");
        assert!((100.0..=200.0).contains(&t), "{t}");
        s.leave_node();
        assert_eq!(
            s.take_events(),
            [SoundEvent::SoundStopped("Sounds/SFX/nothere.ogg".into())]
        );
        assert_eq!(sources(&s), 0);
    }

    #[test]
    fn stop_sound_cuts_the_first_of_that_name_and_voices_fall_back_to_silence() {
        let f = files();
        let mut s = Sounds::new("fr");
        s.play_sound(&f, "Sounds/SFX/loop.wav", "Sounds/SFX/loop.wav", 1);
        s.play_sound(&f, "Sounds/SFX/loop.wav", "Sounds/SFX/loop.wav", 1);
        s.stop_sound("Sounds/SFX/loop.wav");
        assert_eq!(
            s.take_events(),
            [SoundEvent::SoundStopped("Sounds/SFX/loop.wav".into())]
        );
        assert_eq!(sources(&s), 1);
        s.play_voice(&f, "V1.wav");
        assert_eq!(s.voice.path(), "Sounds/Dialogs/V1.wav");
        assert!(s.voice.is_playing(), "found in the language folder");
        s.stop_voice();
        assert_eq!(s.take_events(), [SoundEvent::VoiceStopped]);
        s.play_voice(&f, "V2.wav");
        assert_eq!(s.voice.path(), "Sounds/Dialogs/silence5s.ogg");
        s.play_free_sound(&f, "Sounds/SFX/short.wav", 1.0);
        s.set_channel_volume("sfx", 0.5);
        for t in 1..20 {
            s.update(&f, 33.0 * f64::from(t));
        }
        assert!(s.free.is_empty(), "a played free sound is deleted");
    }

    #[test]
    fn a_movie_cuts_the_music_and_its_end_restarts_it() {
        let f = files();
        let mut s = Sounds::new("fr");
        s.play_music(&f, "Sounds/Musiques/a.wav");
        s.start_movie(&f, Some("Sounds/Musiques/b.wav"));
        assert!(!s.music.is_playing() && s.video.is_playing());
        s.end_movie(&f);
        assert!(s.music.is_playing() && !s.video.is_playing());
        assert_eq!(s.music.path(), "Sounds/Musiques/a.wav");
    }
}

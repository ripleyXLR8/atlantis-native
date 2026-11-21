//! *Atlantis V* played: the game manager and scripts of `atlantis5-script`
//! driving the engine's side of the C functions: views and their zones,
//! the camera, the 3D sprites, sounds, music channels and films.
//!
//! Addresses are atlantis5-dvd-int-3891200's (the international DVD's
//! program; the French DVD's is encrypted and not read). The RE notes are
//! atlantis5-re `subsystems/scripts.md`, `subsystems/play.md` and
//! `functions/lua-handlers.md`.
//!
//! - Views: `Request::LoadView` (the transition's view change,
//!   @ 0x0042d900 → 0x0044b590) loads the WAM view's panorama (as the first
//!   view, [`crate::atlantis5`]), its zones, and drops the sprites; the
//!   persistent sprites come back for the new view (@ 0x004424d0).
//! - Camera: pitch (@ 0x004447a0) and yaw (@ 0x004448c0), each clamped to
//!   its limits when `Atl_Camera_Limit*` set them, else `fmod(angle, 360)`;
//!   the field of view of `Atl_Camera_SetFOV`; the mouse look's inertia
//!   (@ 0x0042cdc0, 0x0042cf80 mode 0).
//! - Sprites: `Atl_Sprite_Play` (@ 0x0040a0d0): the file of the current
//!   view, an earlier sprite of the object removed, inserted by priority
//!   (@ 0x00441830), its player (@ 0x00448c30, 0x00442920: frames 0..n−1
//!   at the bank's rate, 15 for SPW files @ 0x00440e50) stepped each frame
//!   (@ 0x00441c30 → 0x00448d20).
//! - Zones: a click on the view takes the first zone of the WAM view
//!   containing the direction whose name is empty or active in
//!   `atl.zones` (@ 0x00430740), and gives its number and name to
//!   `Game::click_zone` (@ 0x004017c0).
//! - Sounds: `fx/<name>.ogg` played on FMOD channel 6 (@ 0x00459630).
//! - Music: five channels of playlists (`music/<name>.mls`), the
//!   2004 program's `AtlSound` as A5 has it (@ 0x0045a730, 0x0045acf0,
//!   0x0045b4b0, 0x0045b320, the update @ 0x00459fd0).
//! - Films: `Atl_Video_Play(name)` (@ 0x0040c990): `videos/<name>.bik`
//!   with its voice `sounds/video/<language>/<name>.ogg`, the script
//!   waiting; Escape or Space ends it (@ 0x0042b0f0).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis2_data::wam::{Wam, Zone};
use atlantis2_engine::camera::Camera;
use atlantis2_engine::fpu::Fpu;
use atlantis2_engine::warp::{A3Raster, Renderer, Variant};
use atlantis3_data::evolution2004_mls::Playlist;
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis4_data::vorbis::VorbisStream;
use atlantis5_script::{
    Call, CameraFx, Engine, Event, Game, Limit, PlayType, Reply, Request, Value,
};
use atlantis_data::FileSource;

use crate::atlantis5::{panorama_name, sprite_path, ViewError, FACE, VIEW_HEIGHT, VIEW_WIDTH};
use crate::audio::{Audio, CHANNEL_CINEMA, CHANNEL_MUSIC_2004, CHANNEL_SOUND};
use crate::film2004::{Film, FilmSound};
use crate::px::Faces;

/// The SPW sprites' frame rate (@ 0x00440e50: 0xf for an SPW file).
pub const SPRITE_FPS: f64 = 15.0;

/// The music channels (the 5 × 0x78-byte records at DAT_0068e72c).
pub const MUSIC_CHANNELS: usize = 5;

/// The music's fade step: (target − start) / (time · 15) per update
/// (0x006559c8 = 15).
const FADE_RATE: f32 = 15.0;

/// A sprite's player (@ 0x00448c30..0x00448d20).
struct Sprite {
    object: Vec<u8>,
    animation: Vec<u8>,
    bank: Bank,
    /// +0x1c: 0 stay, 1 stop, 3 loop.
    mode: PlayType,
    /// +0x20.
    priority: i32,
    /// +0x00: seconds played.
    time: f64,
    /// +0x18: the frame shown.
    frame: usize,
    /// +0x29: a persistent sprite.
    persistent: bool,
}

impl Sprite {
    fn frames(&self) -> usize {
        self.bank.len().max(1)
    }

    /// @ 0x00448d20: the frame for the time played, and whether a `stop`
    /// sprite is over. `stay`: ftol(t · fps), held at the last frame;
    /// `stop`: the same, over once t > frames / fps; `loop`:
    /// floor(t · fps) mod frames. (The operand of the first two `__ftol`
    /// is t · fps [INFERRED: Ghidra hides the x87 operand; the loop mode
    /// multiplies t by fps explicitly].)
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn step(&mut self, dt: f64) -> bool {
        self.time += dt;
        let n1 = self.frames();
        let at = (self.time * SPRITE_FPS) as usize;
        match self.mode {
            PlayType::Stay => {
                self.frame = at.min(n1 - 1);
                false
            }
            PlayType::Stop => {
                self.frame = at.min(n1 - 1);
                (n1 as f64) / SPRITE_FPS < self.time
            }
            PlayType::Loop => {
                self.frame = ((self.time * SPRITE_FPS).floor() as usize) % n1;
                false
            }
        }
    }
}

/// `atl.zones[name]` (type, active), as the game reads it.
type ZoneLookup<'a> = dyn FnMut(&[u8]) -> Option<(i32, bool)> + 'a;

/// A view on screen.
struct ViewState {
    scene: Vec<u8>,
    view: i32,
    /// The panorama's faces without sprites.
    faces: [Vec<u32>; 6],
    zones: Vec<Zone>,
}

/// A music channel (the 0x78-byte record).
#[derive(Default)]
struct MusicChannel {
    /// +0x00: the playlist's name.
    name: Vec<u8>,
    list: Option<Playlist>,
    /// +0x54: the track played.
    track: usize,
    stream: Option<MusicStream>,
    /// The track's plays left (−1: for ever).
    plays_left: i32,
    /// +0x1c: the volume (0..1).
    volume: f32,
    /// +0x18: 0 none, 1 up, 2 down, 3 out (then stop).
    fade: u8,
    /// +0x24, +0x20, +0x28: start, target and time of the fade.
    fade_start: f32,
    fade_target: f32,
    fade_time: f32,
    /// The playlist asked for while another played (+0x60).
    queued: Vec<u8>,
}

/// A music stream: an Ogg file decoded ahead into an audio channel.
struct MusicStream {
    dec: VorbisStream,
    data: Vec<u8>,
    pushed: u64,
    done: bool,
}

/// The engine's side of the scripts' calls.
pub struct Stage {
    src: Arc<dyn FileSource>,
    language: String,
    audio: Option<Audio>,
    view: Option<ViewState>,
    /// Degrees (+0x10c pitch, +0x110 yaw of the camera).
    pitch: f32,
    yaw: f32,
    /// Radians.
    pub fov: f32,
    yaw_limit: Option<(f32, f32)>,
    pitch_limit: Option<(f32, f32)>,
    /// The mouse look's speeds (+0x08, +0x0c) and damping clock (+0x10).
    speed: [f32; 2],
    damping: f32,
    sprites: Vec<Sprite>,
    /// Persistent sprites: object, priority, seconds played.
    persistent: Vec<(Vec<u8>, i32, f64)>,
    music: [MusicChannel; MUSIC_CHANNELS],
    music_tick: f64,
    /// The program's own CRT `rand` (statically linked; the music's start
    /// and random tracks): seed 1 [INFERRED: its `srand` calls are not
    /// searched].
    music_rand: u32,
    /// The sound on FMOD channel 6.
    sound: Option<Vec<u8>>,
    film: Option<(Film, Option<FilmSound>)>,
    film_ended: bool,
    clock: f64,
    /// Every call and request, for tests and logs.
    pub trace: Vec<String>,
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

impl Stage {
    fn new(src: Arc<dyn FileSource>, language: &str, audio: Option<Audio>) -> Stage {
        Stage {
            src,
            language: language.to_string(),
            audio,
            view: None,
            pitch: 0.0,
            yaw: 0.0,
            fov: f32::from_bits(0x4049_0fdb) * 76.0 * f32::from_bits(0x3bb6_0b61),
            yaw_limit: None,
            pitch_limit: None,
            speed: [0.0; 2],
            damping: 0.0,
            sprites: Vec::new(),
            persistent: Vec::new(),
            music: Default::default(),
            music_tick: 0.0,
            music_rand: 1,
            sound: None,
            film: None,
            film_ended: false,
            clock: 0.0,
            trace: Vec::new(),
        }
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.src.read(path)
    }

    /// The pitch setter @ 0x004447a0: clamped to the limits when set
    /// (+0x125), else `fmod(angle, 360)`.
    fn set_pitch(&mut self, p: f32) {
        self.pitch = match self.pitch_limit {
            Some((lo, hi)) => p.min(hi).max(lo),
            None => libm::fmodf(p, 360.0),
        };
    }

    /// The yaw setter @ 0x004448c0 (limits +0x114/+0x118, flag +0x124).
    fn set_yaw(&mut self, y: f32) {
        self.yaw = match self.yaw_limit {
            Some((lo, hi)) => y.min(hi).max(lo),
            None => libm::fmodf(y, 360.0),
        };
    }

    /// The camera of the angles and field of view (as the first view's,
    /// [`crate::atlantis5::camera`], with the field of view in radians).
    pub fn camera(&self) -> Camera {
        let fpu = Fpu::Double;
        let (w, h) = (VIEW_WIDTH as u32, VIEW_HEIGHT as u32);
        let mut c = Camera::new(fpu, w, h);
        let pi = f64::from(f32::from_bits(0x4049_0fdb));
        let inv180 = f64::from(f32::from_bits(0x3bb6_0b61));
        c.q = [0.0, 0.0, 0.0, 1.0];
        c.yaw(fpu.f32(fpu.mul(fpu.mul(-f64::from(self.yaw), pi), inv180)));
        c.pitch(fpu.f32(fpu.mul(fpu.mul(f64::from(self.pitch), pi), inv180)));
        c.set_fov(self.fov, w, h);
        c
    }

    /// The angles (pitch, yaw) in degrees.
    pub fn angles(&self) -> (f32, f32) {
        (self.pitch, self.yaw)
    }

    /// The view shown (scene, view).
    pub fn address(&self) -> Option<(Vec<u8>, i32)> {
        self.view.as_ref().map(|v| (v.scene.clone(), v.view))
    }

    /// The sprites shown, in list order: (object, animation, frame).
    pub fn sprites(&self) -> Vec<(String, String, usize)> {
        self.sprites
            .iter()
            .map(|s| (lossy(&s.object), lossy(&s.animation), s.frame))
            .collect()
    }

    /// Whether a film plays.
    pub fn film_playing(&self) -> bool {
        self.film.is_some()
    }

    /// The music channels' playlists and tracks (name, track, volume).
    pub fn music(&self) -> Vec<(String, usize, f32)> {
        self.music
            .iter()
            .filter(|c| c.list.is_some())
            .map(|c| (lossy(&c.name), c.track, c.volume))
            .collect()
    }

    fn load_view(&mut self, scene: &[u8], view: i32) -> Result<(), ViewError> {
        let scene_s = lossy(scene);
        #[allow(clippy::cast_sign_loss)]
        let view_u = view.max(0) as u32;
        let panorama = panorama_name(self.src.as_ref(), &scene_s, view_u)?;
        let path = format!("datas/warps/{panorama}");
        let data = self.read(&path).ok_or(ViewError::Missing(path.clone()))?;
        let Faces::Xrgb8888(faces) =
            crate::game::faces_of_jp6(&data, Depth::Xrgb8888).map_err(ViewError::Bad)?
        else {
            return Err(ViewError::Bad("faces in another format".into()));
        };
        let wam_path = format!("datas/scenes/{scene_s}.wam");
        let zones = self
            .read(&wam_path)
            .and_then(|d| Wam::parse(&d).ok())
            .and_then(|w| w.view(view_u).map(|v| v.zones.clone()))
            .unwrap_or_default();
        self.view = Some(ViewState {
            scene: scene.to_vec(),
            view,
            faces,
            zones,
        });
        // The view change drops the sprites; the persistent ones come
        // back (@ 0x004424d0): each object's `z1` of the new view if the
        // file exists, with its time played, while it is not over.
        self.sprites.clear();
        let persistent = std::mem::take(&mut self.persistent);
        for (object, priority, time) in persistent {
            if self.add_sprite(&object, b"z1", PlayType::Stop, priority, true) {
                if let Some(s) = self.sprites.iter_mut().find(|s| s.object == object) {
                    s.time = time;
                }
                self.persistent.push((object, priority, time));
            }
        }
        Ok(())
    }

    /// `Atl_Sprite_Play`'s work: the file of the current view; an earlier
    /// sprite of the object removed; inserted before the first sprite of
    /// lower or equal priority (@ 0x00441830); frame 0, time 0
    /// (@ 0x00448d00). False when the file is missing.
    fn add_sprite(
        &mut self,
        object: &[u8],
        animation: &[u8],
        mode: PlayType,
        priority: i32,
        persistent: bool,
    ) -> bool {
        let Some(v) = &self.view else {
            return false;
        };
        #[allow(clippy::cast_sign_loss)]
        let path = sprite_path(
            &lossy(&v.scene),
            v.view.max(0) as u32,
            &lossy(object),
            &lossy(animation),
        );
        let Some(data) = self.read(&path) else {
            // `LUA_3DSprite_Play -> Le fichier %s n'existe pas` (0x0066a590)
            log::warn!("{path}: not found");
            return false;
        };
        let bank = match Bank::parse(&data, Depth::Xrgb8888) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("{path}: {e}");
                return false;
            }
        };
        self.sprites.retain(|s| s.object != object);
        let at = self
            .sprites
            .iter()
            .position(|s| s.priority <= priority)
            .unwrap_or(self.sprites.len());
        self.sprites.insert(
            at,
            Sprite {
                object: object.to_vec(),
                animation: animation.to_vec(),
                bank,
                mode,
                priority,
                time: 0.0,
                frame: 0,
                persistent,
            },
        );
        true
    }

    // -- sound -------------------------------------------------------------

    fn sound_play(&mut self, path: &[u8], volume: f32) {
        let file = format!("datas/sounds/{}", lossy(path));
        let Some(data) = self.read(&file) else {
            // `AtlSound_LoadSfx : le fichier %s ...` throws in the original.
            log::warn!("{file}: not found");
            return;
        };
        self.sound = Some(path.to_vec());
        let Some(audio) = &self.audio else { return };
        match atlantis4_data::audio::decode_vorbis(&data) {
            Ok(d) => audio.play(
                CHANNEL_SOUND,
                d.sample_rate,
                u16::from(d.channels),
                Arc::new(d.samples),
                false,
                [volume; 2],
            ),
            Err(e) => log::warn!("{file}: {e:?}"),
        }
    }

    // -- music ---------------------------------------------------------------

    /// `AtlSound_PlayMusicList` (@ 0x0045a730): the same list playing:
    /// nothing; another list playing: kept for when it stops; else the list
    /// read (@ 0x0045b7a0), its first track (a random one for `RND`)
    /// played.
    fn music_play(&mut self, channel: i32, name: &[u8]) {
        let Some(ch) = usize::try_from(channel)
            .ok()
            .filter(|&c| c < MUSIC_CHANNELS)
        else {
            log::warn!("music channel {channel}");
            return;
        };
        let playing = self.music[ch].stream.is_some();
        if playing && self.music[ch].name == name {
            return;
        }
        if playing {
            self.music[ch].queued = name.to_vec();
            return;
        }
        let path = format!("datas/sounds/music/{}.mls", lossy(name));
        let Some(data) = self.read(&path) else {
            // `AtlSound_PlayMusicList : le fichier %s ...` throws.
            log::warn!("{path}: not found");
            return;
        };
        let list = Playlist::parse_atlantis5(&data);
        if list.tracks.is_empty() {
            return;
        }
        let track = if list.random {
            (atlantis5_script::script::msvcrt_rand(&mut self.music_rand) as usize)
                % list.tracks.len()
        } else {
            0
        };
        let c = &mut self.music[ch];
        c.name = name.to_vec();
        c.list = Some(list);
        c.queued.clear();
        self.music_track(ch, track);
    }

    /// `AtlSound_PlayMusic` (@ 0x0045acf0): the track's file in `music/`,
    /// its plays (999: for ever), its volume at once or faded up from 0
    /// over its fade-in time.
    fn music_track(&mut self, ch: usize, track: usize) {
        let Some(list) = &self.music[ch].list else {
            return;
        };
        let Some(t) = list.tracks.get(track) else {
            return;
        };
        let file = format!("datas/sounds/music/{}", lossy(&t.file));
        #[allow(clippy::cast_precision_loss)]
        let volume = t.volume as f32 * 0.01;
        #[allow(clippy::cast_precision_loss)]
        let fade_in = t.fade_in_ms as f32 * 0.001;
        let plays = if t.repeat == 999 { -1 } else { t.repeat.max(1) };
        let c = &mut self.music[ch];
        c.track = track;
        c.plays_left = plays;
        if fade_in == 0.0 {
            c.volume = volume;
            c.fade = 0;
        } else {
            c.volume = 0.0;
            c.fade = 1;
            c.fade_start = 0.0;
            c.fade_target = volume;
            c.fade_time = fade_in;
        }
        let data = self.src.read(&file);
        let c = &mut self.music[ch];
        c.stream = None;
        let Some(data) = data else {
            log::warn!("{file}: not found");
            return;
        };
        let Ok(dec) = VorbisStream::new(data.clone()) else {
            log::warn!("{file}: not Ogg Vorbis");
            return;
        };
        if let Some(audio) = &self.audio {
            audio.start_stream(
                CHANNEL_MUSIC_2004 + ch,
                dec.rate(),
                dec.channels() as u16,
                c.volume,
            );
        }
        c.stream = Some(MusicStream {
            dec,
            data,
            pushed: 0,
            done: false,
        });
    }

    /// `AtlSound_StopMusic` (@ 0x0045b4b0): at once without a fade, else a
    /// fade out over `fade` seconds (mode 3).
    fn music_stop(&mut self, channel: usize, fade: f32) {
        let Some(c) = self.music.get_mut(channel) else {
            return;
        };
        if fade <= 0.0 {
            if let Some(audio) = &self.audio {
                audio.stop(CHANNEL_MUSIC_2004 + channel);
            }
            c.stream = None;
            c.list = None;
            c.name.clear();
            c.fade = 0;
        } else {
            c.fade = 3;
            c.fade_time = fade;
            c.fade_start = c.volume;
            c.fade_target = 0.0;
        }
    }

    /// The music part of the update @ 0x00459fd0: fades (one step per
    /// 1/15 s here [INFERRED: the original steps once per call, which is
    /// once per frame]), then the next track when one ended.
    fn music_update(&mut self, dt: f64) {
        // Streams fed ahead of the output.
        if let Some(audio) = self.audio.clone() {
            for (ch, c) in self.music.iter_mut().enumerate() {
                let Some(s) = &mut c.stream else { continue };
                let played = audio.played(CHANNEL_MUSIC_2004 + ch).unwrap_or(0);
                let mut buf = Vec::new();
                while !s.done && s.pushed < played + 44_100 {
                    buf.clear();
                    let n = s.dec.read(&mut buf, 4096);
                    if n == 0 {
                        if c.plays_left > 1 || c.plays_left < 0 {
                            if c.plays_left > 1 {
                                c.plays_left -= 1;
                            }
                            match VorbisStream::new(s.data.clone()) {
                                Ok(d) => s.dec = d,
                                Err(_) => s.done = true,
                            }
                            continue;
                        }
                        s.done = true;
                        break;
                    }
                    audio.push(CHANNEL_MUSIC_2004 + ch, &buf);
                    s.pushed += n as u64;
                }
                audio.set_gain(CHANNEL_MUSIC_2004 + ch, [c.volume; 2]);
            }
        }
        self.music_tick += dt;
        while self.music_tick >= 1.0 / 15.0 {
            self.music_tick -= 1.0 / 15.0;
            for ch in 0..MUSIC_CHANNELS {
                let c = &mut self.music[ch];
                if c.stream.is_none() || c.fade == 0 || c.fade_time <= 0.0 {
                    if c.fade != 0 && c.fade_time <= 0.0 {
                        c.volume = c.fade_target;
                        let out = c.fade == 3;
                        c.fade = 0;
                        if out {
                            self.music_out(ch);
                        }
                    }
                    continue;
                }
                let step = (c.fade_target - c.fade_start) / (c.fade_time * FADE_RATE);
                c.volume += step;
                let done = match c.fade {
                    1 => c.volume >= c.fade_target,
                    _ => c.volume <= c.fade_target,
                };
                if done {
                    c.volume = c.fade_target;
                    let mode = c.fade;
                    c.fade = 0;
                    if mode == 3 || (mode == 2 && c.fade_target == 0.0) {
                        self.music_out(ch);
                    }
                }
            }
        }
        // A track over: the next as the list's flags say (@ 0x00459fd0).
        for ch in 0..MUSIC_CHANNELS {
            let over = match (&self.music[ch].stream, &self.audio) {
                (Some(s), Some(a)) => s.done && !a.is_playing(CHANNEL_MUSIC_2004 + ch),
                (Some(s), None) => s.done,
                _ => false,
            };
            if !over {
                continue;
            }
            let c = &self.music[ch];
            let Some(list) = &c.list else { continue };
            let n = list.tracks.len();
            let next = if list.looped {
                Some(if c.track + 1 < n { c.track + 1 } else { 0 })
            } else if list.random {
                Some((atlantis5_script::script::msvcrt_rand(&mut self.music_rand) as usize) % n)
            } else if c.track + 1 < n {
                Some(c.track + 1)
            } else {
                None
            };
            match next {
                Some(t) => self.music_track(ch, t),
                None => self.music_stop(ch, 0.0),
            }
        }
    }

    /// The end of a fade out: the channel stopped, then the list asked for
    /// meanwhile played.
    fn music_out(&mut self, ch: usize) {
        let queued = std::mem::take(&mut self.music[ch].queued);
        self.music_stop(ch, 0.0);
        if !queued.is_empty() {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            self.music_play(ch as i32, &queued);
        }
    }

    // -- films ---------------------------------------------------------------

    fn video_play(&mut self, name: &[u8]) -> bool {
        let file = format!("datas/videos/{}.bik", lossy(name));
        let Some(data) = self.read(&file) else {
            log::warn!("{file}: not found");
            return false;
        };
        let film = match Film::open(&lossy(name), Arc::from(data), Depth::Xrgb8888, self.clock) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("{file}: {e:?}");
                return false;
            }
        };
        let voice = format!("datas/sounds/video/{}/{}.ogg", self.language, lossy(name));
        let sound = match (&self.audio, self.read(&voice)) {
            (Some(a), Some(d)) => FilmSound::open(a, CHANNEL_CINEMA, d, 1.0),
            _ => None,
        };
        self.film = Some((film, sound));
        self.film_ended = false;
        true
    }

    fn film_update(&mut self) {
        let now = self.clock;
        let Some((film, sound)) = &mut self.film else {
            return;
        };
        if let (Some(s), Some(a)) = (sound.as_mut(), &self.audio) {
            s.pump(a);
        }
        let mut over = false;
        while film.due(now) {
            film.do_frame();
            if !film.next_frame() {
                over = true;
                break;
            }
        }
        if over {
            self.end_film();
        }
    }

    fn end_film(&mut self) {
        if let Some((_, sound)) = self.film.take() {
            if let (Some(s), Some(a)) = (sound, &self.audio) {
                s.stop(a);
            }
            self.film_ended = true;
        }
    }

    // -- frame ---------------------------------------------------------------

    /// The camera's step in free mode (@ 0x0042cf80 mode 0): the angles
    /// turned by the speeds (rad/s · 180/π), the speeds × 0.55 20 times a
    /// second.
    fn camera_step(&mut self, dt: f32) {
        let pi = f32::from_bits(0x4049_0fdb);
        if self.speed != [0.0, 0.0] {
            let yaw = self.yaw + dt * self.speed[0] * 180.0 / pi;
            let pitch = self.pitch + dt * self.speed[1] * 180.0 / pi;
            self.set_yaw(yaw);
            self.set_pitch(pitch);
        }
        self.damping += dt * 20.0;
        while self.damping >= 1.0 {
            self.speed[0] *= 0.55;
            self.speed[1] *= 0.55;
            self.damping -= 1.0;
        }
    }

    /// The mouse look's input (@ 0x0042cdc0): a move of (dx, dy) pixels
    /// with the button down adds dx · dt · −0.3 and dy · dt · −0.3 to the
    /// speeds (a speed against the move is first cut by a third), each kept
    /// within ±6. (The button is the left one [INFERRED: the event's
    /// subtype 2 is not decoded].)
    pub fn look(&mut self, dx: f32, dy: f32, dt: f32) {
        self.damping = 0.0;
        // The sign is −1 without the camera's invert flag (+0x80).
        let mut ax = -(dx * dt) * 0.3;
        let mut ay = dy * dt * -0.3;
        let third = f32::from_bits(0x3eaa_aaab);
        if (self.speed[0] < 0.0 && ax > 0.0) || (self.speed[0] > 0.0 && ax < 0.0) {
            ax -= self.speed[0] * third;
        }
        if (self.speed[1] < 0.0 && ay > 0.0) || (self.speed[1] > 0.0 && ay < 0.0) {
            ay -= self.speed[1] * third;
        }
        self.speed[0] += ax;
        self.speed[1] += ay;
        for s in &mut self.speed {
            if s.abs() > 6.0 {
                *s *= 6.0 / s.abs();
            }
        }
    }

    /// One frame: camera, sprites (@ 0x00441c30), sound and music
    /// (@ 0x00459fd0), film.
    fn update(&mut self, dt: f64, play: bool) {
        self.clock += dt;
        #[allow(clippy::cast_possible_truncation)]
        if play {
            self.camera_step(dt as f32);
        }
        for p in &mut self.persistent {
            p.2 += dt;
        }
        let mut over = Vec::new();
        for (i, s) in self.sprites.iter_mut().enumerate() {
            if s.step(dt) {
                over.push(i);
            }
        }
        for &i in over.iter().rev() {
            let s = self.sprites.remove(i);
            if s.persistent {
                self.persistent.retain(|p| p.0 != s.object);
            }
        }
        self.music_update(dt);
        self.film_update();
    }

    /// The picture: the film while one plays, else the view with its
    /// sprites (drawn from the list's head [UNKNOWN: the drawing loop is
    /// not read]) by the cube renderer.
    pub fn render(&self, out: &mut [u32]) {
        out.fill(0);
        if let Some((film, _)) = &self.film {
            let (w, h) = film.size();
            let (ox, oy) = (
                VIEW_WIDTH.saturating_sub(w) / 2,
                VIEW_HEIGHT.saturating_sub(h) / 2,
            );
            for y in 0..h.min(VIEW_HEIGHT) {
                for x in 0..w.min(VIEW_WIDTH) {
                    out[(oy + y) * VIEW_WIDTH + ox + x] = film.xrgb[y * w + x];
                }
            }
            return;
        }
        let Some(v) = &self.view else { return };
        let mut faces = v.faces.clone();
        for s in &self.sprites {
            for (face, px) in faces.iter_mut().enumerate() {
                let mut t = Target {
                    pixels: px,
                    pitch: FACE,
                    ymin: 0,
                    ymax: FACE as i32,
                    xmin: 0,
                    xmax: FACE as i32,
                };
                s.bank.draw_face(&mut t, s.frame, face);
            }
        }
        let cam = self.camera();
        let mut r = Renderer::new(Variant::PII, cam.fpu, VIEW_WIDTH, VIEW_HEIGHT);
        r.a3 = Some(A3Raster {
            smooth: true,
            face: FACE,
        });
        r.render(&faces, &cam.matrix(), cam.projection(), out);
    }

    /// The zone under screen point (sx, sy): the first of the view's
    /// zones containing the direction whose name is empty, or whose
    /// `atl.zones` entry `zone` says is active (@ 0x00430740).
    fn zone_at(&self, sx: f32, sy: f32, zone: &mut ZoneLookup<'_>) -> Option<(i32, Vec<u8>)> {
        let v = self.view.as_ref()?;
        let d = self.camera().ray(sx, sy).map(f64::from);
        for z in &v.zones {
            if !z.contains(d) {
                continue;
            }
            let name = z.name.clone().unwrap_or_default().into_bytes();
            if name.is_empty() || matches!(zone(&name), Some((_, true))) {
                return Some((z.value, name));
            }
        }
        None
    }
}

impl Engine for Stage {
    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        self.src.read(&lossy(path))
    }

    #[allow(clippy::too_many_lines)]
    fn call(&mut self, call: &Call) -> Reply {
        match call {
            Call::VideoPlay { name, rect } => {
                if rect.is_some() {
                    // The 5-argument form draws the film over `oldpaper.tga`
                    // in the rectangle [not ported: shown like the others].
                    log::info!("film {} in a rectangle: drawn full", lossy(name));
                }
                if self.video_play(name) {
                    return Reply::Wait;
                }
            }
            Call::SpritePlay {
                object,
                animation,
                play,
                priority,
            } => {
                self.add_sprite(object, animation, *play, *priority, false);
            }
            Call::SpriteStop { object } => {
                // @ 0x0040b2e0 → 0x00441a90.
                self.sprites.retain(|s| &s.object != object);
            }
            Call::PersistantSpritePlay { object, priority } => {
                // @ 0x0040ba20: once only; the `z1` of the view played
                // with the player's default mode (1, stop, @ 0x00448c30).
                if self.persistent.iter().any(|p| &p.0 == object) {
                    log::info!("persistent sprite {} already", lossy(object));
                } else if self.add_sprite(object, b"z1", PlayType::Stop, *priority, true) {
                    self.persistent.push((object.clone(), *priority, 0.0));
                }
            }
            Call::PersistantSpriteStop { object } => {
                self.persistent.retain(|p| &p.0 != object);
                self.sprites.retain(|s| &s.object != object);
            }
            Call::PersistantSpriteStopAll => {
                // @ 0x00442460.
                let objects: Vec<Vec<u8>> = self.persistent.drain(..).map(|p| p.0).collect();
                self.sprites.retain(|s| !objects.contains(&s.object));
            }
            Call::CameraLimitYaw(l) => {
                self.yaw_limit = match l {
                    Limit::None => None,
                    Limit::Range { min, max } => Some((*min, *max)),
                };
            }
            Call::CameraLimitPitch(l) => {
                self.pitch_limit = match l {
                    Limit::None => None,
                    Limit::Range { min, max } => Some((*min, *max)),
                };
            }
            Call::CameraLookAt {
                pitch,
                yaw,
                animated,
            } => {
                if *animated {
                    // The turn (@ 0x0042cac0, 0x0042cf80 mode 2) is not
                    // ported: the angles are set at once.
                    log::info!("animated look: set at once");
                }
                self.set_pitch(*pitch);
                self.set_yaw(*yaw);
            }
            Call::CameraSetFov { radians, .. } => self.fov = *radians,
            Call::CameraActivateFx(fx) => {
                if let CameraFx::BoatFx(v) = fx {
                    log::info!("BoatFx {v:?}: the pitching is not ported");
                }
            }
            Call::DisplayGetScreenSize => {
                // The display's +0xc, +0x10: the back buffer, 1024 × 576.
                #[allow(clippy::cast_precision_loss)]
                return Reply::Values(vec![VIEW_WIDTH as f64, VIEW_HEIGHT as f64]);
            }
            Call::MusicPlay { playlist, channel } => self.music_play(*channel, playlist),
            Call::MusicStop { channel, fade } => {
                if let Ok(ch) = usize::try_from(*channel) {
                    self.music_stop(ch, *fade);
                }
            }
            Call::MusicStopAll { fade } => {
                for ch in 0..MUSIC_CHANNELS {
                    self.music_stop(ch, *fade);
                }
            }
            Call::MusicFade {
                channel,
                volume,
                time,
            } => {
                // @ 0x0045b320: down (2) when at or under the volume, else
                // up (1); from the volume now.
                if let Some(c) = usize::try_from(*channel)
                    .ok()
                    .and_then(|ch| self.music.get_mut(ch))
                {
                    c.fade = if *volume <= c.volume { 2 } else { 1 };
                    c.fade_time = *time;
                    c.fade_start = c.volume;
                    c.fade_target = *volume;
                }
            }
            Call::SoundPlay { path, volume, .. } => self.sound_play(path, *volume),
            Call::SoundStop { path, .. } => {
                if self.sound.as_deref() == Some(path.as_slice()) {
                    if let Some(a) = &self.audio {
                        a.stop(CHANNEL_SOUND);
                    }
                    self.sound = None;
                }
            }
            Call::Print(b, args) => {
                let text: Vec<String> = args.iter().map(Value::trace).collect();
                log::info!("{}: {}", b.name(), text.join(", "));
            }
            other => {
                // Dialogs, examine, poker, game over, saves: the next
                // milestones.
                log::info!("not ported yet: {other:?}");
            }
        }
        Reply::Done
    }

    fn request(&mut self, r: &Request) {
        self.trace.push(format!("{r:?}"));
        match r {
            Request::LoadView { scene, view } => {
                if let Err(e) = self.load_view(scene, *view) {
                    log::warn!("view {} {view}: {e}", lossy(scene));
                }
            }
            Request::TransitionFov { radians, .. } => self.fov = *radians,
            Request::TransitionEnd | Request::DeleteAutosave => {}
        }
    }

    fn event(&mut self, e: &Event<'_>) {
        match e {
            Event::Call(b, args) => {
                let text: Vec<String> = args.iter().map(Value::trace).collect();
                self.trace
                    .push(format!("C {}({})", b.name(), text.join(", ")));
            }
            Event::Error(site, m) => {
                log::error!("Lua error in {site:?}: {}", lossy(m));
                self.trace.push(format!("error {site:?}: {}", lossy(m)));
            }
            Event::Missing(p) => log::warn!("script missing: {}", lossy(p)),
            Event::Rejected(b, m) => log::info!("{}: {m}", b.name()),
            Event::Note(n) => log::debug!("{n}"),
            Event::Run(_) | Event::End(_) => {}
        }
    }
}

/// A game being played.
pub struct Play {
    pub game: Game,
    stage: Rc<RefCell<Stage>>,
}

impl Play {
    /// A game on `src` (an `Atlantis5Source`), films' voices in
    /// `language` (a folder name of `sounds/video/`), sound to `audio`.
    pub fn new(
        src: Arc<dyn FileSource>,
        language: &str,
        audio: Option<Audio>,
    ) -> Result<Play, String> {
        let stage = Rc::new(RefCell::new(Stage::new(src, language, audio)));
        let game = Game::new(stage.clone()).map_err(|e| e.to_string())?;
        Ok(Play { game, stage })
    }

    /// The engine's side.
    pub fn stage(&self) -> std::cell::Ref<'_, Stage> {
        self.stage.borrow()
    }

    pub fn new_game(&mut self) {
        self.game.new_game();
    }

    /// One frame of `dt` seconds.
    pub fn update(&mut self, dt: f64) {
        self.game.set_fov(self.stage.borrow().fov);
        let play = self.game.state() == 4 && !self.game.busy();
        self.stage.borrow_mut().update(dt, play);
        if std::mem::take(&mut self.stage.borrow_mut().film_ended) {
            self.game.finish_wait();
        }
        self.game.update(dt);
    }

    /// A click at (sx, sy) of the 1024 × 576 view: the zone there.
    pub fn click(&mut self, sx: f32, sy: f32) -> bool {
        let game = &mut self.game;
        let hit = self
            .stage
            .borrow()
            .zone_at(sx, sy, &mut |name| game.zone(name));
        match hit {
            Some((id, name)) => self.game.click_zone(id, &name),
            None => false,
        }
    }

    /// The zone the cursor is over, for tests and the cursor.
    pub fn zone_at(&mut self, sx: f32, sy: f32) -> Option<(i32, Vec<u8>)> {
        let game = &mut self.game;
        self.stage
            .borrow()
            .zone_at(sx, sy, &mut |name| game.zone(name))
    }

    /// A mouse move of (dx, dy) pixels with the button down.
    pub fn look(&mut self, dx: f32, dy: f32, dt: f32) {
        if self.game.state() == 4 && !self.game.busy() {
            self.stage.borrow_mut().look(dx, dy, dt);
        }
    }

    /// Escape or Space: a film ends (@ 0x0042b0f0: keys 1 and 0x39).
    pub fn skip(&mut self) {
        let mut s = self.stage.borrow_mut();
        if s.film.is_some() {
            s.end_film();
        }
    }

    /// The picture (1024 × 576 x8r8g8b8).
    pub fn render(&self, out: &mut [u32]) {
        self.stage.borrow().render(out);
    }

    /// Points of the view inside zones the player can click now (screen
    /// coordinates, a coarse grid), with each zone's number and name.
    pub fn clickable(&mut self) -> Vec<(f32, f32, i32, Vec<u8>)> {
        let mut out: Vec<(f32, f32, i32, Vec<u8>)> = Vec::new();
        for gy in 0..18 {
            for gx in 0..32 {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (gx as f32 * 32.0 + 16.0, gy as f32 * 32.0 + 16.0);
                if let Some((id, name)) = self.zone_at(x, y) {
                    if !out.iter().any(|o| o.2 == id && o.3 == name) {
                        out.push((x, y, id, name));
                    }
                }
            }
        }
        out
    }
}

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
//!   (@ 0x0042cdc0, 0x0042cf80 mode 0); the turns of mode 2 (the animated
//!   `Atl_Camera_LookAt`, `centring`; [`crate::atlantis5_camera`]);
//!   `BoatFx` (@ 0x0042ccc0: the camera under a node of its controller,
//!   rocked about X at each step).
//! - Sprites: `Atl_Sprite_Play` (@ 0x0040a0d0): the file of the current
//!   view, an earlier sprite of the object removed, inserted by priority
//!   (@ 0x00441830), its player (@ 0x00448c30, 0x00442920: frames 0..n−1
//!   at the bank's rate, 15 for SPW files @ 0x00440e50) stepped each frame
//!   (@ 0x00441c30 → 0x00448d20); `Atl_Sprite_PlayRebound`'s modes 2 and 4
//!   and frame range (@ 0x0040aa00); `Atl_Sprite_Randomize`'s records and
//!   the next animation drawn at a sprite's end (@ 0x00443160).
//! - Picks and clicks: each play frame picks what is under the screen
//!   centre, the first zone of the WAM view (type below 1000) holding the
//!   centre's direction whose name is empty or active in `atl.zones`
//!   (@ 0x00430740) or, with none, the sprite there (@ 0x00430b70,
//!   0x0044af90); a left click acts on the last pick (@ 0x004300d0):
//!   `Game::click_zone` or `Game::click_sprite`.
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
use crate::atlantis5_camera::{
    euler_quat, forward, look_at_direction, look_rotation, quat_matrix, quat_mul, Quat, Turn,
};
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

/// A sprite's player's mode (+0x1c, @ 0x00448ce0; @ 0x00448c30 makes it
/// 1): `Atl_Sprite_Play`'s `stay` 0, `stop` 1, `loop` 3;
/// `Atl_Sprite_PlayRebound`'s `stop` 2, `loop` 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Stay = 0,
    Stop = 1,
    Rebound = 2,
    Loop = 3,
    ReboundLoop = 4,
}

impl From<PlayType> for Mode {
    fn from(p: PlayType) -> Mode {
        match p {
            PlayType::Stay => Mode::Stay,
            PlayType::Stop => Mode::Stop,
            PlayType::Loop => Mode::Loop,
        }
    }
}

/// A sprite's player (@ 0x00448c30..0x00448d20).
struct Sprite {
    object: Vec<u8>,
    animation: Vec<u8>,
    bank: Bank,
    /// +0x1c.
    mode: Mode,
    /// +0x10, +0x14: the first and last frames (0 and the bank's count −
    /// 1, @ 0x00442920; `Atl_Sprite_PlayRebound` sets them).
    first: u32,
    last: u32,
    /// +0x20.
    priority: i32,
    /// +0x00: seconds played.
    time: f64,
    /// +0x18: the frame shown.
    frame: u32,
    /// +0x29: a persistent sprite.
    persistent: bool,
}

/// MSVC's `__ftol` (@ 0x0062abdc): truncation toward zero to 64 bits,
/// the low 32 kept.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn ftol(x: f64) -> u32 {
    x as i64 as u32
}

/// The frame and the end of @ 0x00448d20 at time `t` (see
/// [`Sprite::step`]), in the code's 32-bit unsigned arithmetic.
fn player_frame(mode: Mode, first: u32, last: u32, t: f64) -> (u32, bool) {
    let n1 = last.wrapping_sub(first).wrapping_add(1);
    let span = last.wrapping_sub(first).wrapping_mul(2);
    let fps = SPRITE_FPS;
    let u = ftol(t * fps);
    match mode {
        Mode::Stay | Mode::Stop => {
            let u = if u >= n1 { n1.wrapping_sub(1) } else { u };
            let over = mode == Mode::Stop && (t < 0.0 || f64::from(n1) / fps < t);
            (first.wrapping_add(u), over)
        }
        Mode::Rebound => {
            let u = if u >= n1 { span.wrapping_sub(u) } else { u };
            let period = n1.wrapping_mul(2).wrapping_sub(1);
            (first.wrapping_add(u), f64::from(period) / fps < t)
        }
        Mode::Loop => {
            let u = ftol((t * fps).floor());
            (first.wrapping_add(u.checked_rem(n1).unwrap_or(0)), false)
        }
        Mode::ReboundLoop => {
            let period = n1.wrapping_mul(2).wrapping_sub(1);
            let u = u.checked_rem(period).unwrap_or(0);
            let u = if u >= n1 { span.wrapping_sub(u) } else { u };
            (first.wrapping_add(u), false)
        }
    }
}

impl Sprite {
    /// @ 0x00448d20, as its code does it in 32-bit unsigned arithmetic
    /// (n1 = last − first + 1, `EDI`; u = `__ftol`(t · fps)): time += dt;
    /// then by mode [CONFIRMED-STATIC]
    /// - 0 `stay`: frame = first + min(u, n1 − 1);
    /// - 1 `stop`: the same; over when t < 0 or n1 / fps < t;
    /// - 2 (rebound): u, or 2(last − first) − u once u ≥ n1; over when
    ///   (2 n1 − 1) / fps < t;
    /// - 3 `loop`: first + `__ftol`(floor(t · fps)) mod n1;
    /// - 4 (rebound loop): u mod (2 n1 − 1), mirrored as in mode 2.
    ///
    /// Returns whether the sprite is over. (A divisor of 0, a bank
    /// without frames, would fault in the original; the frame is then
    /// `first`.)
    fn step(&mut self, dt: f64) -> bool {
        self.time += dt;
        let (frame, over) = player_frame(self.mode, self.first, self.last, self.time);
        self.frame = frame;
        over
    }

    /// The frame as the bank's index.
    fn index(&self) -> usize {
        self.frame as usize
    }
}

/// An `Atl_Sprite_Randomize` record (the sprite manager's map +0x20, by
/// object; @ 0x00443380: +0x24 tick1, +0x28 tick2, +0x20 = 100 − both,
/// +0x2c = 1).
struct Randomized {
    object: Vec<u8>,
    tick1: f32,
    tick2: f32,
}

impl Randomized {
    /// @ 0x00443160: r = `rand`() × 0.0030518509 (100 / 32767, the float
    /// 0x006557cc) on the x87 stack; `Tick2` when r < tick2, `Tick1` when
    /// r < tick2 + tick1, else `Stand` (0x0066e534, 0x0066e52c,
    /// 0x0066e524) [CONFIRMED-STATIC].
    fn choose(&self, rand: &mut u32) -> &'static [u8] {
        let r = f64::from(atlantis5_script::script::msvcrt_rand(rand))
            * f64::from(f32::from_bits(0x3b48_0190));
        let tick2 = f64::from(self.tick2);
        if r < tick2 {
            b"Tick2"
        } else if r < tick2 + f64::from(self.tick1) {
            b"Tick1"
        } else {
            b"Stand"
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
    /// The mouse look's speeds (+0x0c yaw, +0x08 pitch, degrees per second
    /// before the × 180/π) and damping clock (+0x10).
    speed: [f32; 2],
    damping: f32,
    /// The camera controller's mode (+0x00): 0 free (the mouse look), 1,
    /// 2 turning.
    mode: u32,
    /// The turn of mode 2.
    turn: Option<Turn>,
    /// `Atl_Camera_LookAt(..., true)` waits for its turn: the mode it
    /// kept.
    look_turn: Option<u32>,
    /// The points of the zone under the centre at the last pick (the
    /// manager's +0x38), which `centring` turns toward.
    hover_zone: Option<Vec<[f32; 3]>>,
    /// The cursor the last pick set (@ 0x0042bfd0's argument), `None` for
    /// the default one.
    pub cursor: Option<i32>,
    sprites: Vec<Sprite>,
    /// `BoatFx` (the camera controller's +0x20 = 0, @ 0x0042ccc0): its
    /// clock (+0x24, seconds, a float).
    boat: Option<f32>,
    /// The rotation of the controller's own node (+0x28, a child of the
    /// scene root: its world rotation).
    boat_q: Quat,
    /// The camera node hangs under that node (from `BoatFx` until
    /// `NoFx`), so its world rotation is boat_q ⊗ its own.
    on_boat: bool,
    /// Persistent sprites: object, priority, seconds played.
    persistent: Vec<(Vec<u8>, i32, f64)>,
    music: [MusicChannel; MUSIC_CHANNELS],
    music_tick: f64,
    /// The program's own CRT `rand` (statically linked, `_rand`
    /// @ 0x0062b352, one state for the thread: the music's start and
    /// random tracks, the randomized sprites' next animation): seed 1
    /// [INFERRED: its `srand` calls are not searched].
    crt_rand: u32,
    /// The randomized objects (`Atl_Sprite_Randomize`).
    randomized: Vec<Randomized>,
    /// The sound on FMOD channel 6.
    sound: Option<Vec<u8>>,
    film: Option<(Film, Option<FilmSound>)>,
    /// A call the scripts wait for has ended (a film, an animated look).
    wait_ended: bool,
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
            mode: 0,
            turn: None,
            look_turn: None,
            hover_zone: None,
            cursor: None,
            sprites: Vec::new(),
            boat: None,
            boat_q: [0.0, 0.0, 0.0, 1.0],
            on_boat: false,
            persistent: Vec::new(),
            music: Default::default(),
            music_tick: 0.0,
            crt_rand: 1,
            randomized: Vec::new(),
            sound: None,
            film: None,
            wait_ended: false,
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
        // Under the boat's node the world rotation (what the draw and the
        // picks read, @ 0x0048a850 relative to the root) is the parent's
        // world rotation times the node's own (@ 0x0048aff0 →
        // 0x0048a7c0 with the parent's world rotation as `this`).
        if self.on_boat {
            c.q = quat_mul(&self.boat_q, &c.q);
        }
        c.set_fov(self.fov, w, h);
        c
    }

    /// A direction of the world in the camera's parent's frame (the turns'
    /// targets, @ 0x0042d610 and @ 0x00406f00): the boat's node's
    /// rotation undone while the camera hangs under it; the root's frame
    /// otherwise.
    fn to_parent_frame(&self, v: [f32; 3]) -> [f32; 3] {
        if !self.on_boat {
            return v;
        }
        // Rᵀ · v (R rows of the rotation).
        let m = quat_matrix(&self.boat_q);
        [
            m[0] * v[0] + m[3] * v[1] + m[6] * v[2],
            m[1] * v[0] + m[4] * v[1] + m[7] * v[2],
            m[2] * v[0] + m[5] * v[1] + m[8] * v[2],
        ]
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
            .map(|s| (lossy(&s.object), lossy(&s.animation), s.index()))
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
        let scene_changed = self.view.as_ref().is_some_and(|v| v.scene != scene);
        self.view = Some(ViewState {
            scene: scene.to_vec(),
            view,
            faces,
            zones,
        });
        // Another scene drops the persistent sprites (@ 0x0044b8c8 →
        // 0x00442460, in the branch that loads the new scene). Every view
        // change drops the sprites and the randomize records
        // (@ 0x0044b8d0 → 0x004421a0 → 0x00443730); the persistent ones
        // come back (@ 0x004424d0): each object's `z1` of the new view if
        // the file exists, with its time played, while it is not over.
        if scene_changed {
            self.persistent.clear();
        }
        self.sprites.clear();
        self.randomized.clear();
        let persistent = std::mem::take(&mut self.persistent);
        for (object, priority, time) in persistent {
            if let Some(s) = self.add_sprite(&object, b"z1", Mode::Stop, priority, true) {
                s.time = time;
                self.persistent.push((object, priority, time));
            }
        }
        Ok(())
    }

    /// `Atl_Sprite_Play`'s work (@ 0x0040a0d0; `Atl_Sprite_PlayRebound`'s
    /// the same): an earlier sprite of the object removed (the list walk
    /// comparing the files' object part, @ 0x00449f50, then @ 0x00441a00)
    /// before the file of the current view is looked for; inserted before
    /// the first sprite of lower or equal priority (@ 0x00441830); its
    /// player (@ 0x00442920: frames 0..count − 1) at time 0, frame 0
    /// (@ 0x00448d00), in `mode`. `None` when the file is missing.
    fn add_sprite(
        &mut self,
        object: &[u8],
        animation: &[u8],
        mode: Mode,
        priority: i32,
        persistent: bool,
    ) -> Option<&mut Sprite> {
        let v = self.view.as_ref()?;
        #[allow(clippy::cast_sign_loss)]
        let path = sprite_path(
            &lossy(&v.scene),
            v.view.max(0) as u32,
            &lossy(object),
            &lossy(animation),
        );
        self.sprites.retain(|s| s.object != object);
        let Some(data) = self.read(&path) else {
            // `LUA_3DSprite_Play -> Le fichier %s n'existe pas` (0x0066a590)
            log::warn!("{path}: not found");
            return None;
        };
        let bank = match Bank::parse(&data, Depth::Xrgb8888) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("{path}: {e}");
                return None;
            }
        };
        let at = self
            .sprites
            .iter()
            .position(|s| s.priority <= priority)
            .unwrap_or(self.sprites.len());
        #[allow(clippy::cast_possible_truncation)]
        let last = (bank.len() as u32).wrapping_sub(1);
        self.sprites.insert(
            at,
            Sprite {
                object: object.to_vec(),
                animation: animation.to_vec(),
                bank,
                mode,
                first: 0,
                last,
                priority,
                time: 0.0,
                frame: 0,
                persistent,
            },
        );
        self.sprites.get_mut(at)
    }

    /// A randomized sprite at its end (@ 0x00441c30, 0x00441d41..
    /// 0x00441f85): the record's choice (@ 0x00443160); the same
    /// animation (the file's fourth part, @ 0x00449ff0, compared byte for
    /// byte, @ 0x00455110) plays again from its time less (last −
    /// first) / fps (@ 0x00448c70, 0x00448cf0); another replaces the
    /// sprite in its place in the list: the file with that animation
    /// (@ 0x0044a090), a new player (@ 0x00442920: mode 1, frames 0..count
    /// − 1, the old one's priority, not persistent). A missing file throws
    /// `C3DSpriteManager :: ProcessSpriteAnimation -> Impossible de trouver
    /// le fichier %s, changement de sprite annulé` (0x0066e468); its catch
    /// is not read: Native logs it and leaves the sprite.
    fn randomize_next(&mut self, i: usize) {
        let Some(r) = self
            .randomized
            .iter()
            .find(|r| r.object == self.sprites[i].object)
        else {
            return;
        };
        let next = r.choose(&mut self.crt_rand);
        let s = &mut self.sprites[i];
        if s.animation == next {
            let n = s.last.wrapping_sub(s.first);
            s.time -= f64::from(n) / SPRITE_FPS;
            return;
        }
        let Some(v) = &self.view else { return };
        #[allow(clippy::cast_sign_loss)]
        let path = sprite_path(
            &lossy(&v.scene),
            v.view.max(0) as u32,
            &lossy(&s.object),
            &lossy(next),
        );
        let Some(bank) = self
            .read(&path)
            .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok())
        else {
            log::warn!("{path}: not found, the sprite's change cancelled");
            return;
        };
        let s = &mut self.sprites[i];
        #[allow(clippy::cast_possible_truncation)]
        let last = (bank.len() as u32).wrapping_sub(1);
        *s = Sprite {
            object: std::mem::take(&mut s.object),
            animation: next.to_vec(),
            bank,
            mode: Mode::Stop,
            first: 0,
            last,
            priority: s.priority,
            time: 0.0,
            frame: 0,
            persistent: false,
        };
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
            (atlantis5_script::script::msvcrt_rand(&mut self.crt_rand) as usize) % list.tracks.len()
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
                Some((atlantis5_script::script::msvcrt_rand(&mut self.crt_rand) as usize) % n)
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
            self.wait_ended = true;
        }
    }

    // -- frame ---------------------------------------------------------------

    /// The camera's step (@ 0x0042cf80). Mode 0 (free): pitch and yaw
    /// turned by their speeds (@ 0x004444c0, 0x00444600: dt · speed ·
    /// 180/π, set through the limits every step), the damping clock +=
    /// dt · 20 (0x00655708) and, while it is above 1 (0x0042d25b /
    /// 0x0042d28c: `fcomp` 1.0, `test ah, 0x41`, so not at 1), both speeds
    /// × 0.55 (0x00655704) and the clock − 1. Mode 2: the turn's step
    /// ([`Turn::step`]), false until it ends. Other modes: nothing. True
    /// unless a turn goes on. First, with `BoatFx` on (+0x20 = 0) and
    /// outside a turn (0x0042cf8a..0x0042cfd1): the clock +0x24 = dt +
    /// clock (stored as a float), a = sin(clock · 0.5) · 0.03 (0x00655408,
    /// 0x00655734, a float), and the boat's node set to the rotation
    /// @ 0x0048f5f0(a, 0, 0) relative to the root: a about X.
    fn camera_step(&mut self, dt: f32) -> bool {
        if let Some(clock) = self.boat.as_mut() {
            if self.mode != 2 {
                let t = f64::from(dt) + f64::from(*clock);
                #[allow(clippy::cast_possible_truncation)]
                {
                    *clock = t as f32;
                    let a = ((t * f64::from(0.5f32)).sin() * f64::from(f32::from_bits(0x3cf5_c28f)))
                        as f32;
                    self.boat_q = euler_quat(a, 0.0, 0.0);
                }
            }
        }
        match self.mode {
            0 => {
                let pi = f32::from_bits(0x4049_0fdb);
                let pitch = self.pitch + dt * self.speed[1] * 180.0 / pi;
                self.set_pitch(pitch);
                let yaw = self.yaw + dt * self.speed[0] * 180.0 / pi;
                self.set_yaw(yaw);
                self.damping += dt * 20.0;
                while self.damping > 1.0 {
                    self.speed[0] *= 0.55;
                    self.speed[1] *= 0.55;
                    self.damping -= 1.0;
                }
                true
            }
            2 => {
                let Some(turn) = self.turn.as_mut() else {
                    return true;
                };
                let ((pitch, yaw), end) = turn.step(dt);
                self.set_pitch(pitch);
                self.set_yaw(yaw);
                end
            }
            _ => true,
        }
    }

    /// A turn toward `to` (@ 0x0042cac0): mode 2 from the direction now.
    fn start_turn(&mut self, to: [f32; 3]) {
        self.mode = 2;
        self.turn = Some(Turn::new(forward(self.pitch, self.yaw), to));
    }

    /// The controller's mode set back (0: @ 0x0042caa0, the free mode with
    /// the speeds and the damping clock 0; 1: @ 0x0042cab0).
    fn restore_mode(&mut self, mode: u32) {
        match mode {
            0 => {
                self.mode = 0;
                self.speed = [0.0; 2];
                self.damping = 0.0;
            }
            1 => self.mode = 1,
            _ => {}
        }
        self.turn = None;
    }

    /// The mouse look's input (@ 0x0042cdc0), for each mouse event of
    /// kind 2 (a move, @ 0x0043bde0 → 0x004546a0(2, ...); whatever the
    /// buttons) while the mode is 0: the damping clock 0; the move's
    /// DirectInput lX, lY (the event's +0x1c, +0x20) times dt and −0.3
    /// (0x006556fc; lY by +0.3 with the invert flag +0x80, 0x00655700)
    /// added to the yaw and pitch speeds (a speed against the move first
    /// loses a third of itself, 0x006556f8), each kept within ±6
    /// (0x006556f4).
    pub fn look(&mut self, dx: f32, dy: f32, dt: f32) {
        if self.mode != 0 {
            return;
        }
        self.damping = 0.0;
        // Yaw from lX × −0.3; pitch from lY × 0.3 × (−1 without the
        // invert flag +0x80, which Native does not set).
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
        // `Atl_Camera_LookAt(..., true)`'s loop (@ 0x00406f00): the turn's
        // step each frame; at its end the kept mode back, and the call
        // returns.
        if let Some(saved) = self.look_turn {
            #[allow(clippy::cast_possible_truncation)]
            if self.camera_step(dt as f32) {
                self.restore_mode(saved);
                self.look_turn = None;
                self.wait_ended = true;
            }
        }
        for p in &mut self.persistent {
            p.2 += dt;
        }
        // Each sprite in list order; one over goes on with its object's
        // randomize record (@ 0x004432c0), or is removed with its
        // persistent record (@ 0x00442350).
        let mut i = 0;
        while i < self.sprites.len() {
            if !self.sprites[i].step(dt) {
                i += 1;
                continue;
            }
            let object = &self.sprites[i].object;
            if self.randomized.iter().any(|r| &r.object == object) {
                self.randomize_next(i);
                i += 1;
            } else {
                let s = self.sprites.remove(i);
                if s.persistent {
                    self.persistent.retain(|p| p.0 != s.object);
                }
            }
        }
        self.music_update(dt);
        self.film_update();
    }

    /// The picture: the film while one plays, else the view with its
    /// sprites (drawn from the list's head to its tail, @ 0x0044bd30's
    /// second pass: @ 0x00456a90 then @ 0x00456ad0) by the cube renderer.
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
                s.bank.draw_face(&mut t, s.index(), face);
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
    /// zones of a type below 1000 (@ 0x0044acd0) containing the direction
    /// whose name is empty, or whose `atl.zones` entry `zone` says is
    /// active (@ 0x00430740). The game asks it for the screen centre.
    fn zone_at(
        &self,
        sx: f32,
        sy: f32,
        zone: &mut ZoneLookup<'_>,
    ) -> Option<(i32, Vec<u8>, usize)> {
        let v = self.view.as_ref()?;
        let d = self.camera().ray(sx, sy).map(f64::from);
        for (k, z) in v.zones.iter().enumerate() {
            if z.kind >= 1000 || !z.contains(d) {
                continue;
            }
            let name = z.name.clone().unwrap_or_default().into_bytes();
            if name.is_empty() || matches!(zone(&name), Some((_, true))) {
                return Some((z.value, name, k));
            }
        }
        None
    }

    /// The sprites under the screen centre (@ 0x0044af90): the centre's
    /// face pixel (the camera's direction × 10000 (0x00655830) through
    /// the six face cameras of the cube, 768-texel faces, the first whose
    /// projection falls in 0..767 — [`Camera::texel_face`]); then the
    /// sprite list from its tail to its head (@ 0x00456aa0 walks to the
    /// last node, @ 0x00456ae0 steps to the previous: the sprites drawn
    /// last, @ 0x0044bd30 draws from the head, come first), a sprite hit
    /// when the pixel is in its frame's `GET_SPWZ` rectangle on that face,
    /// both bounds included (@ 0x0044b312..0x0044b33b), and
    /// `SPW_COLLISION` says it is covered (the DLL's code is *Atlantis
    /// III*'s, [`Bank::face_opaque_at`]). The first hit: (object,
    /// animation).
    pub fn sprite_at_centre(&self) -> Option<(Vec<u8>, Vec<u8>)> {
        self.view.as_ref()?;
        let c = self.camera();
        let (face, u, v) = c.texel_face(c.cx, c.cy, FACE as u32)?;
        for s in self.sprites.iter().rev() {
            let Some(info) = s.bank.face_info(s.index(), face) else {
                continue;
            };
            let (x, y) = (-i32::from(info.xoff), -i32::from(info.yoff));
            let (w, h) = (i32::from(info.w), i32::from(info.h));
            if !(x <= u && u <= x + w && y <= v && v <= y + h) {
                continue;
            }
            if s.bank.face_opaque_at(s.index(), face, u, v) {
                return Some((s.object.clone(), s.animation.clone()));
            }
        }
        None
    }

    /// The view's number.
    pub fn view_number(&self) -> Option<i32> {
        self.view.as_ref().map(|v| v.view)
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
                self.add_sprite(object, animation, (*play).into(), *priority, false);
            }
            Call::SpritePlayRebound {
                object,
                animation,
                looped,
                priority,
                first,
                last,
            } => {
                // @ 0x0040aa00: as `Atl_Sprite_Play` (the earlier sprite
                // of the object removed, the file's player inserted by
                // priority, time and frame 0 @ 0x00448d00), then mode 2 or
                // 4 (@ 0x00448ce0) and the frames first..=last
                // (@ 0x00448cc0, from `__ftol`, kept as unsigned).
                let mode = if *looped {
                    Mode::ReboundLoop
                } else {
                    Mode::Rebound
                };
                if let Some(s) = self.add_sprite(object, animation, mode, *priority, false) {
                    #[allow(clippy::cast_sign_loss)]
                    {
                        s.first = *first as u32;
                        s.last = *last as u32;
                    }
                }
            }
            Call::SpriteGetFrame { object, animation } => {
                // @ 0x0040b400: the sprite of the file (scene, view,
                // object, animation) in the list (@ 0x00441480, the bank's
                // file name compared), its +0x18 pushed as an unsigned
                // number; none: `Le fichier de sprite %s n'existe pas`
                // (0x0066aac4), no result.
                if let Some(s) = self
                    .sprites
                    .iter()
                    .find(|s| &s.object == object && &s.animation == animation)
                {
                    return Reply::Values(vec![f64::from(s.frame)]);
                }
                log::warn!(
                    "Atl_Sprite_GetFrame: no sprite {} {}",
                    lossy(object),
                    lossy(animation)
                );
            }
            Call::SpriteRandomize {
                object,
                tick1,
                tick2,
            } => {
                // @ 0x0040b720: an earlier record of the object erased
                // (@ 0x004432c0, 0x00443480), then @ 0x00443380.
                self.randomized.retain(|r| &r.object != object);
                self.randomized.push(Randomized {
                    object: object.clone(),
                    tick1: *tick1,
                    tick2: *tick2,
                });
            }
            Call::SpriteStop { object } => {
                // @ 0x0040b2e0 → 0x00441a90; the object's randomize record
                // erased too (@ 0x0040b3a3 → 0x00443480).
                self.sprites.retain(|s| &s.object != object);
                self.randomized.retain(|r| &r.object != object);
            }
            Call::PersistantSpritePlay { object, priority } => {
                // @ 0x0040ba20: once only; the `z1` of the view played
                // with the player's default mode (1, stop, @ 0x00448c30).
                if self.persistent.iter().any(|p| &p.0 == object) {
                    log::info!("persistent sprite {} already", lossy(object));
                } else if self
                    .add_sprite(object, b"z1", Mode::Stop, *priority, true)
                    .is_some()
                {
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
                    // @ 0x00406f00: the mode kept, a turn toward the
                    // direction of the angles (@ 0x0048bb70 × (0, 0, 1)),
                    // then a loop on the camera's step, the first with a
                    // time of 0; the angles are the last step's.
                    let saved = self.mode;
                    self.start_turn(self.to_parent_frame(look_at_direction(*pitch, *yaw)));
                    if self.camera_step(0.0) {
                        self.restore_mode(saved);
                    } else {
                        self.look_turn = Some(saved);
                        return Reply::Wait;
                    }
                } else {
                    self.set_pitch(*pitch);
                    self.set_yaw(*yaw);
                }
            }
            Call::CameraSetFov { radians, .. } => self.fov = *radians,
            Call::CameraActivateFx(fx) => match fx {
                CameraFx::BoatFx(v) => {
                    // @ 0x0042ccc0: +0x20 = 0; the camera node moved under
                    // the root, then under the controller's node +0x28,
                    // both keeping its world transform (@ 0x0048ac60,
                    // 0x0048aca0 with 1); that node's rotation relative to
                    // its parent (+0x68 = +0x28 + 0x40: the root, from its
                    // constructor @ 0x0048a3c0) set to the look rotation of
                    // the vector with up (0, −1, 0) (@ 0x0048f7e0); the
                    // clock +0x24 = 0. (The kept world transform lasts until
                    // the setters rebuild the camera node's own rotation
                    // from the angles, which Native always does.)
                    self.boat = Some(0.0);
                    self.boat_q = look_rotation(v, &[0.0, -1.0, 0.0]);
                    self.on_boat = true;
                }
                CameraFx::NoFx => {
                    // @ 0x0042cd50: +0x20 = 1, +0x24 = 0, the camera node
                    // back under the root (@ 0x0048ac60 with 1).
                    self.boat = None;
                    self.on_boat = false;
                }
            },
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
            Request::SubjectMenu { character } => {
                // The subject menu is the dialogs' milestone.
                log::info!("subject menu of {}: not ported yet", lossy(character));
            }
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

    /// @ 0x0042d610, fx 1: the mode kept; no zone under the centre: the
    /// free mode (@ 0x0042caa0); else the zone's direction (the sum of its
    /// points, @ 0x004586c0, through the scene root's and the camera
    /// parent's frames, normalised) against the camera's direction now:
    /// a turn when their dot product is below cos(π · 0.016667)
    /// (0x00655738: 3°).
    fn centring_start(&mut self) -> u32 {
        let saved = self.mode;
        let Some(points) = self.hover_zone.clone() else {
            self.restore_mode(0);
            return saved;
        };
        let mut sum = [0.0f32; 3];
        for p in &points {
            for i in 0..3 {
                sum[i] += p[i];
            }
        }
        // Into the camera's parent's frame first, then normalised.
        let sum = self.to_parent_frame(sum);
        let l2 = f64::from(sum[0]) * f64::from(sum[0])
            + f64::from(sum[1]) * f64::from(sum[1])
            + f64::from(sum[2]) * f64::from(sum[2]);
        #[allow(clippy::cast_possible_truncation)]
        let inv = (1.0 / l2.sqrt()) as f32;
        let b = sum.map(|x| x * inv);
        let a = forward(self.pitch, self.yaw);
        let dot = f64::from(a[2]) * f64::from(b[2])
            + f64::from(a[1]) * f64::from(b[1])
            + f64::from(a[0]) * f64::from(b[0]);
        let pi = f64::from(f32::from_bits(0x4049_0fdb));
        let limit = (pi * f64::from(f32::from_bits(0x3c88_8889))).cos();
        if dot < limit {
            self.start_turn(b);
        }
        saved
    }

    fn camera_step(&mut self, dt: f32) -> bool {
        Stage::camera_step(self, dt)
    }

    fn camera_restore(&mut self, mode: u32) {
        self.restore_mode(mode);
    }
}

/// What the last pick found under the screen centre (the manager's +0x38
/// zone or +0x3c sprite).
#[derive(Clone, Debug, PartialEq)]
pub enum Hover {
    /// A zone: its number and name.
    Zone(i32, Vec<u8>),
    /// A sprite: its object and animation.
    Sprite(Vec<u8>, Vec<u8>),
}

/// A game being played.
pub struct Play {
    pub game: Game,
    stage: Rc<RefCell<Stage>>,
    /// The mouse's moves (DirectInput lX, lY) since the last frame.
    moves: Vec<(f32, f32)>,
    /// Left clicks since the last frame.
    clicks: usize,
    hover: Option<Hover>,
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
        Ok(Play {
            game,
            stage,
            moves: Vec::new(),
            clicks: 0,
            hover: None,
        })
    }

    /// The engine's side.
    pub fn stage(&self) -> std::cell::Ref<'_, Stage> {
        self.stage.borrow()
    }

    pub fn new_game(&mut self) {
        self.game.new_game();
    }

    /// One frame of `dt` seconds. In play (the manager's state 4, no
    /// script running), as `CGameManager::Process` (@ 0x0042f000): the
    /// mouse's moves into the camera's speeds (@ 0x0042cdc0), the
    /// camera's step (@ 0x0042cf80), the clicks on what the last frame's
    /// pick found (@ 0x004300d0), the timers (@ 0x00435a70); then the pick
    /// for the next frame: the zone under the centre (@ 0x00430740) or,
    /// with none, the sprite (@ 0x00430b70). Moves and clicks outside play
    /// are dropped.
    pub fn update(&mut self, dt: f64) {
        self.game.set_fov(self.stage.borrow().fov);
        let play = self.game.state() == 4 && !self.game.busy();
        let moves = std::mem::take(&mut self.moves);
        let clicks = std::mem::take(&mut self.clicks);
        if play {
            #[allow(clippy::cast_possible_truncation)]
            let dtf = dt as f32;
            let mut st = self.stage.borrow_mut();
            for (dx, dy) in moves {
                st.look(dx, dy, dtf);
            }
        }
        self.stage.borrow_mut().update(dt, play);
        if std::mem::take(&mut self.stage.borrow_mut().wait_ended) {
            self.game.finish_wait();
        }
        if play {
            for _ in 0..clicks {
                match self.hover.clone() {
                    Some(Hover::Zone(id, name)) => {
                        self.game.click_zone(id, &name);
                    }
                    Some(Hover::Sprite(object, animation)) => {
                        self.game.click_sprite(&object, &animation);
                    }
                    None => {}
                }
            }
        }
        self.game.update(dt);
        if self.game.state() == 4 && !self.game.busy() {
            self.pick();
        }
    }

    /// The pick (@ 0x0042f000 state 4's end): +0x38 = +0x3c = 0; the zone
    /// under the centre and its cursor (type 1 → 0, 2 → 2, 3 → 1); with no
    /// zone the sprite and its cursor ([`Game::sprite_cursor`]); with
    /// neither the default cursor.
    fn pick(&mut self) {
        let (cx, cy) = {
            let st = self.stage.borrow();
            let c = st.camera();
            (c.cx, c.cy)
        };
        let game = &mut self.game;
        let zone = self
            .stage
            .borrow()
            .zone_at(cx, cy, &mut |name| game.zone(name));
        let mut st = self.stage.borrow_mut();
        st.hover_zone = None;
        if let Some((id, name, k)) = zone {
            let kind = if name.is_empty() {
                1
            } else {
                self.game.zone(&name).map_or(0, |z| z.0)
            };
            st.cursor = match kind {
                1 => Some(0),
                2 => Some(2),
                3 => Some(1),
                _ => st.cursor,
            };
            st.hover_zone = st.view.as_ref().map(|v| v.zones[k].points.clone());
            self.hover = Some(Hover::Zone(id, name));
            return;
        }
        let hit = st.sprite_at_centre();
        let view = st.view_number().unwrap_or(0);
        drop(st);
        self.hover = None;
        let mut cursor = None;
        if let Some((object, animation)) = hit {
            if let Some(c) = self.game.sprite_cursor(&object, view) {
                cursor = Some(c);
                self.hover = Some(Hover::Sprite(object, animation));
            }
        }
        self.stage.borrow_mut().cursor = cursor;
    }

    /// What the last pick found under the centre.
    pub fn hover(&self) -> Option<&Hover> {
        self.hover.as_ref()
    }

    /// A mouse move of (dx, dy) (DirectInput's relative lX, lY), taken at
    /// the next frame.
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.moves.push((dx, dy));
    }

    /// A press of the left button (the mouse event of kind 0 with the
    /// buttons 1, @ 0x0043bbc0), taken at the next frame: a click on what
    /// is under the screen centre.
    pub fn click(&mut self) {
        self.clicks += 1;
    }

    /// Turns the camera so that the screen centre is on the direction of
    /// zone `id` of the view (the sum of its points), at once (tests and
    /// tools; the player turns with the mouse). False without such a zone.
    #[doc(hidden)]
    pub fn aim_at_zone(&mut self, id: i32) -> bool {
        let mut st = self.stage.borrow_mut();
        let Some(points) = st
            .view
            .as_ref()
            .and_then(|v| v.zones.iter().find(|z| z.value == id && z.kind < 1000))
            .map(|z| z.points.clone())
        else {
            return false;
        };
        let mut d = [0.0f32; 3];
        for p in &points {
            for i in 0..3 {
                d[i] += p[i];
            }
        }
        let (pitch, yaw) = crate::atlantis5_camera::angles_of(&normalize3(d));
        st.set_pitch(pitch);
        st.set_yaw(yaw);
        drop(st);
        self.pick();
        true
    }

    /// Sets the camera's angles at once and picks (tests and tools).
    #[doc(hidden)]
    pub fn look_angles(&mut self, pitch: f32, yaw: f32) {
        {
            let mut st = self.stage.borrow_mut();
            st.set_pitch(pitch);
            st.set_yaw(yaw);
        }
        self.pick();
    }

    /// Turns the camera, at once, so that the centre is on a covered pixel
    /// of the sprite of `object` and the pick takes it (tests and tools):
    /// each covered pixel of its frame's faces is tried, its direction
    /// from the cube's face cameras (@ 0x00490ad0: the look rotations of
    /// (0, 0, 1), (0, 0, −1), (0, −1, 0), (0, 1, 0), (−1, 0, 0),
    /// (1, 0, 0) with the up vectors (0, −1, 0), (0, 0, −1) for face 2 and
    /// (0, 0, 1) for face 3; field of view π · 0.5012459 (0x00656258) over
    /// 768 texels). False when none works.
    #[doc(hidden)]
    pub fn aim_at_sprite(&mut self, object: &[u8]) -> bool {
        use crate::atlantis5_camera::{angles_of, look_rotation, mat_vec, quat_matrix};
        let pixels: Vec<(usize, i32, i32)> = {
            let st = self.stage.borrow();
            let Some(s) = st.sprites.iter().find(|s| s.object == object) else {
                return false;
            };
            let mut out = Vec::new();
            for face in 0..6 {
                let Some(info) = s.bank.face_info(s.index(), face) else {
                    continue;
                };
                let (x0, y0) = (-i32::from(info.xoff), -i32::from(info.yoff));
                for v in y0..y0 + i32::from(info.h) {
                    for u in x0..x0 + i32::from(info.w) {
                        if s.bank.face_opaque_at(s.index(), face, u, v) {
                            out.push((face, u, v));
                        }
                    }
                }
            }
            out
        };
        let faces: [([f32; 3], [f32; 3]); 6] = [
            ([0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),
            ([0.0, 0.0, -1.0], [0.0, -1.0, 0.0]),
            ([0.0, -1.0, 0.0], [0.0, 0.0, -1.0]),
            ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
            ([1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
        ];
        #[allow(clippy::cast_precision_loss)]
        let half = FACE as f32 * 0.5;
        let fov = std::f32::consts::PI * f32::from_bits(0x3f00_51a6);
        let focal = half / (fov * 0.5).tan();
        // The middle of the sprite's pixels first.
        let mut order: Vec<usize> = (0..pixels.len()).collect();
        let mid = pixels.len() / 2;
        order.sort_by_key(|&i| i.abs_diff(mid));
        for i in order.into_iter().take(400) {
            let (face, u, v) = pixels[i];
            let q = look_rotation(&faces[face].0, &faces[face].1);
            #[allow(clippy::cast_precision_loss)]
            let local = [
                (u as f32 + 0.5 - half) / focal,
                (v as f32 + 0.5 - half) / focal,
                1.0,
            ];
            let d = normalize3(mat_vec(&quat_matrix(&q), &local));
            let (pitch, yaw) = angles_of(&d);
            self.look_angles(pitch, yaw);
            if matches!(&self.hover, Some(Hover::Sprite(o, _)) if o == object) {
                return true;
            }
        }
        false
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

    /// The zones the player can click in this view (those the pick would
    /// take with the centre on their direction): number and name.
    pub fn zones(&mut self) -> Vec<(i32, Vec<u8>)> {
        let zones: Vec<(i32, Option<String>, i32)> = self
            .stage
            .borrow()
            .view
            .as_ref()
            .map(|v| {
                v.zones
                    .iter()
                    .map(|z| (z.value, z.name.clone(), z.kind))
                    .collect()
            })
            .unwrap_or_default();
        let mut out = Vec::new();
        for (id, name, kind) in zones {
            if kind >= 1000 {
                continue;
            }
            let name = name.unwrap_or_default().into_bytes();
            if name.is_empty() || matches!(self.game.zone(&name), Some((_, true))) {
                out.push((id, name));
            }
        }
        out
    }
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v.map(|x| x / l)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoFiles;

    impl FileSource for NoFiles {
        fn size(&self, _: &str) -> Option<u64> {
            None
        }
        fn read_at(&self, _: &str, _: u64, _: usize) -> Option<Vec<u8>> {
            None
        }
        fn has_dir(&self, _: &str) -> bool {
            false
        }
    }

    fn stage() -> Stage {
        Stage::new(Arc::new(NoFiles), "french", None)
    }

    #[test]
    fn an_animated_look_turns_over_frames() {
        let mut s = stage();
        s.set_yaw(270.0);
        let r = s.call(&Call::CameraLookAt {
            pitch: 10.0,
            yaw: 300.0,
            animated: true,
        });
        assert_eq!(r, Reply::Wait);
        assert_eq!(s.mode, 2);
        let mut n = 0;
        let mut last = s.yaw;
        while !s.wait_ended {
            s.update(1.0 / 30.0, false);
            n += 1;
            // The yaw goes from 270 toward 300 (atan2 gives it in
            // (−180, 180], fmod keeps it there: −90 .. −60).
            assert!(s.yaw >= last - 1e-3, "{} {last}", s.yaw);
            last = s.yaw;
            assert!(n < 100);
        }
        // 0.75 s at 30 frames a second.
        assert_eq!(n, 23);
        assert_eq!(s.mode, 0, "the kept mode back");
        assert!((s.yaw - -60.0).abs() < 0.05, "{}", s.yaw);
        assert!((s.pitch - 10.0).abs() < 0.05, "{}", s.pitch);
    }

    #[test]
    fn the_turns_direction_is_the_drawn_centre() {
        // The renderer's camera and the turn's frame agree: the screen
        // centre's ray is the direction the setters' rotation gives.
        let mut s = stage();
        for &(p, y) in &[(0.0, 270.0), (15.0, 40.0), (-25.0, 181.0)] {
            s.set_pitch(p);
            s.set_yaw(y);
            let c = s.camera();
            let r = c.ray(c.cx, c.cy);
            let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
            let f = crate::atlantis5_camera::forward(p, y);
            for i in 0..3 {
                assert!((r[i] / l - f[i]).abs() < 1e-4, "{p} {y}: {r:?} {f:?}");
            }
        }
    }

    #[test]
    fn the_inertia_damps_only_above_one() {
        let mut s = stage();
        s.speed = [1.0, 0.0];
        // dt · 20 = 1 exactly: no damping yet (@ 0x0042d25b: not at 1).
        s.camera_step(0.05);
        assert_eq!(s.speed[0], 1.0);
        s.camera_step(0.05);
        assert!((s.speed[0] - 0.55).abs() < 1e-6);
    }

    #[test]
    fn centring_turns_only_beyond_three_degrees() {
        let mut s = stage();
        // No zone under the centre: free mode, speeds 0.
        s.speed = [2.0, 1.0];
        assert_eq!(s.centring_start(), 0);
        assert_eq!((s.mode, s.speed), (0, [0.0, 0.0]));
        // A zone 2° away: no turn.
        let z = crate::atlantis5_camera::forward(0.0, 2.0);
        s.hover_zone = Some(vec![z]);
        s.centring_start();
        assert_eq!(s.mode, 0);
        assert!(s.camera_step(0.0));
        // 5° away: a turn of 0.75 s.
        let z = crate::atlantis5_camera::forward(0.0, 5.0);
        s.hover_zone = Some(vec![z, z]);
        s.centring_start();
        assert_eq!(s.mode, 2);
        let mut n = 0;
        while !s.camera_step(0.25) {
            n += 1;
        }
        assert_eq!(n, 2);
        s.camera_restore(0);
        assert_eq!(s.mode, 0);
        assert!((s.yaw - 5.0).abs() < 0.01, "{}", s.yaw);
    }

    fn frames(mode: Mode, first: u32, last: u32, k: u32) -> Vec<(u32, bool)> {
        // Mid-frame times, as a 15 fps step would see them.
        (0..k)
            .map(|i| player_frame(mode, first, last, (f64::from(i) + 0.5) / SPRITE_FPS))
            .collect()
    }

    #[test]
    fn rebound_sprites_go_and_come_back() {
        // Mode 4, frames 3..=5: a period of 2 · 3 − 1 frames, mirrored.
        let f: Vec<u32> = frames(Mode::ReboundLoop, 3, 5, 10)
            .iter()
            .map(|x| x.0)
            .collect();
        assert_eq!(f, [3, 4, 5, 4, 3, 3, 4, 5, 4, 3]);
        // Mode 2, frames 0..=2: once, over after 5 frames.
        let f = frames(Mode::Rebound, 0, 2, 6);
        assert_eq!(
            f,
            [
                (0, false),
                (1, false),
                (2, false),
                (1, false),
                (0, false),
                (u32::MAX, true)
            ]
        );
        // The zeppelin's clocks: "loop", 0, 0 shows frame 0 for ever.
        assert!(frames(Mode::ReboundLoop, 0, 0, 40)
            .iter()
            .all(|&x| x == (0, false)));
        // A stop sprite of 3 frames: over once 3 / 15 s have passed.
        let f = frames(Mode::Stop, 0, 2, 4);
        assert_eq!(f, [(0, false), (1, false), (2, false), (2, true)]);
        // A puzzle's frame chosen by first = last.
        assert_eq!(player_frame(Mode::ReboundLoop, 7, 7, 3.0), (7, false));
    }

    #[test]
    fn randomized_sprites_draw_their_next_animation() {
        // rand() from seed 1: 41, 18467, 6334, 26500 → r = 0.125, 56.4,
        // 19.3, 80.9.
        let r = Randomized {
            object: b"barman".to_vec(),
            tick1: 10.0,
            tick2: 25.0,
        };
        let mut seed = 1;
        let next: Vec<&[u8]> = (0..4).map(|_| r.choose(&mut seed)).collect();
        assert_eq!(next, [&b"Tick2"[..], b"Stand", b"Tick2", b"Stand"]);
        let r = Randomized {
            object: b"gorilla".to_vec(),
            tick1: 20.0,
            tick2: 0.0,
        };
        let mut seed = 1;
        let next: Vec<&[u8]> = (0..3).map(|_| r.choose(&mut seed)).collect();
        assert_eq!(next, [&b"Tick1"[..], b"Stand", b"Tick1"]);
    }

    #[test]
    fn boat_fx_rocks_the_camera_about_x() {
        let mut s = stage();
        s.set_yaw(30.0);
        let still = s.camera().q;
        s.call(&Call::CameraActivateFx(CameraFx::BoatFx([0.0, 0.0, 1.0])));
        assert!(s.on_boat);
        // The look rotation of (0, 0, 1) with up (0, −1, 0) is no turn.
        let q = s.camera().q;
        assert!(q.iter().zip(still).all(|(a, b)| (a - b).abs() < 1e-6));
        // One second: a = sin(0.5) · 0.03 about X.
        assert!(s.camera_step(1.0));
        let a = (0.5f64.sin() * 0.03) as f32;
        let want = [(a * 0.5).sin(), 0.0, 0.0, (a * 0.5).cos()];
        assert!(s.boat_q.iter().zip(want).all(|(x, y)| (x - y).abs() < 1e-6));
        assert_eq!(s.camera().q, quat_mul(&s.boat_q, &still));
        // A direction of the world back in the boat's frame.
        let f = forward(s.pitch, s.yaw);
        let m = quat_matrix(&s.boat_q);
        let world = crate::atlantis5_camera::mat_vec(&m, &f);
        let back = s.to_parent_frame(world);
        assert!(back.iter().zip(f).all(|(x, y)| (x - y).abs() < 1e-6));
        // No rocking during a turn.
        s.start_turn(forward(0.0, 40.0));
        let held = s.boat_q;
        s.camera_step(0.1);
        assert_eq!(s.boat_q, held);
        s.restore_mode(0);
        s.set_pitch(0.0);
        s.set_yaw(30.0);
        // NoFx: the camera back under the root.
        s.call(&Call::CameraActivateFx(CameraFx::NoFx));
        assert!(!s.on_boat && s.boat.is_none());
        assert_eq!(s.camera().q, still);
    }
}

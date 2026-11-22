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
use atlantis3_data::atlantis5_texts::{Dialogs, KinematicSubtitles};
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
use crate::atlantis5_dialog::{self as dlg, Act, FilmSubtitles, Fonts, Kind, MenuState, Player};
use crate::atlantis5_menu::{self as menus, Config, Effect, Menus};
use crate::atlantis5_poker::{self as poker, Poker};
use crate::audio::{Audio, CHANNEL_CINEMA, CHANNEL_MUSIC_2004, CHANNEL_SOUND, CHANNEL_VOICE};
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
    /// Held while the object's character faces the player in a dialog
    /// or its subject menu (@ 0x00443650; back @ 0x00443570)
    /// [INFERRED: the two calls are not read; held means not drawn from].
    held: bool,
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

/// The voice of a tirade: a type-2 sample (`FSOUND` channel 7,
/// @ 0x004593d0(path, 0, 2, 0), played at 1.0 @ 0x00459630).
struct Voice {
    /// Its length (seconds), for a stage without sound.
    length: f64,
    started: f64,
}

/// `Atl_Dialog_Play`'s run (@ 0x00407ac0).
struct DialogRun {
    player: Player,
    /// The dialog's talkers and their picturing.
    talkers: Vec<(String, atlantis3_data::atlantis5_texts::Picturing)>,
    /// The tirades: talker, text.
    tirades: Vec<(String, String)>,
    /// The first loop: the 3D talkers turn to face (`facing`), until each
    /// one's time reaches its length; then `listen` in a loop.
    facing: bool,
    /// The objects that were given `facing`.
    faced: Vec<Vec<u8>>,
    voice: Option<Voice>,
    /// The bubble (+0x34) and the 2D talker's film.
    bubble: Option<Bank>,
    portrait: Option<Film>,
    /// The text's height (+0x4c, @ 0x00461800 with `AtlFont_Dialog` and
    /// 760) and the box.
    text_h: i32,
    text: String,
}

/// The subject menu (`CMenu_Dialog`, mode 9).
struct SubjectMenu {
    character: Vec<u8>,
    state: MenuState,
    /// The subjects shown: name, enabled.
    subjects: Vec<(Vec<u8>, bool)>,
    bubble: Option<Bank>,
    images: Vec<Option<Bank>>,
    /// A subject was picked: the game runs its click.
    picked: Option<Vec<u8>>,
    /// The menu has ended (its process returned 1).
    closed: bool,
}

/// The inventory (`CMenu_Inventory`, mode 8).
struct InventoryMenu {
    /// +0x14: the first slot shown.
    offset: usize,
    size: usize,
    items: Vec<Option<Vec<u8>>>,
    background: Option<Bank>,
    /// Left and right buttons: normal, focused.
    left: [Option<Bank>; 2],
    right: [Option<Bank>; 2],
    icons: Vec<Option<Bank>>,
    hand_icon: Option<Bank>,
    closed: bool,
}

/// What a left press in the inventory hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryHit {
    /// A slot (the grid's cell + the offset).
    Slot(usize),
    Left,
    Right,
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
    /// The film's subtitles (`kinematic_subtitles.xml`).
    film_subtitles: Option<FilmSubtitles>,
    /// The text tables of the language.
    dialogs: Option<Dialogs>,
    kinematic: Option<KinematicSubtitles>,
    pub fonts: Fonts,
    /// `Atl_Dialog_Play`'s run.
    dialog: Option<DialogRun>,
    /// The subject menu (mode 9).
    menu: Option<SubjectMenu>,
    /// The inventory (mode 8).
    inventory: Option<InventoryMenu>,
    /// Left presses and Space presses for the dialog and the menu this
    /// frame.
    clicks: usize,
    space: bool,
    /// The mouse's position in menus (the screen's pixels).
    pub mouse: (i32, i32),
    /// The folder of the player's saves.
    save_dir: Option<std::path::PathBuf>,
    /// `CConfiguration` (the option pages, the save's `CONFIGURATION`).
    pub config: Config,
    /// Menus the scripts asked for: `Atl_GameOver`'s death panel with its
    /// text's key (@ 0x00405b70: @ 0x00410d00, mode 10), `Atl_ExamineItem`'s
    /// examine view with its item (@ 0x00405d70: @ 0x004137b0, mode 11).
    menu_requests: Vec<(u32, Vec<u8>)>,
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
        let mut st = Stage::bare(src, language, audio);
        // @ 0x0043d910: `texts/dialog/`, `texts/menu/`, `texts/video/` +
        // the language + the file.
        st.dialogs = st
            .read(&format!("datas/texts/dialog/{language}/dialogs.xml"))
            .and_then(|d| Dialogs::parse(&d).ok());
        st.kinematic = st
            .read(&format!(
                "datas/texts/video/{language}/kinematic_subtitles.xml"
            ))
            .and_then(|d| KinematicSubtitles::parse(&d).ok());
        st
    }

    fn bare(src: Arc<dyn FileSource>, language: &str, audio: Option<Audio>) -> Stage {
        let fonts = Fonts::load(&src, language);
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
            film_subtitles: None,
            dialogs: None,
            kinematic: None,
            fonts,
            dialog: None,
            menu: None,
            inventory: None,
            clicks: 0,
            space: false,
            mouse: (VIEW_WIDTH as i32 / 2, VIEW_HEIGHT as i32 / 2),
            save_dir: None,
            config: Config::default(),
            menu_requests: Vec::new(),
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
            // @ 0x00459630: a sound at its volume × fx × master.
            Ok(d) => audio.play(
                CHANNEL_SOUND,
                d.sample_rate,
                u16::from(d.channels),
                Arc::new(d.samples),
                false,
                [volume * self.config.fx * self.config.master; 2],
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
                c.volume * self.config.master * self.config.music,
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
        let master_music = self.config.master * self.config.music;
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
                // @ 0x0045a640: each channel at master × its volume ×
                // music.
                let g = c.volume * master_music;
                audio.set_gain(CHANNEL_MUSIC_2004 + ch, [g; 2]);
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
        // The film's subtitles (`kinematic_subtitles.xml`, by the film's
        // name; compared without case [INFERRED]), read only when the
        // game manager's +0x55 (`display_subtitle`) is set: it is given to
        // the player (@ 0x0040ca6e → @ 0x0042a950, +0x41), which reads
        // the list only then (@ 0x0042aa90).
        let subtitles = self.config.subtitles;
        self.film_subtitles = self.kinematic.as_ref().filter(|_| subtitles).and_then(|k| {
            k.films
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(&lossy(name)))
                .map(|f| FilmSubtitles::new(f.subtitles.clone()))
        });
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
        // @ 0x0042b0f0: after the film's step, its subtitles.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (frame, rate) = (film.frame_num, film.fps().round() as u32);
        if let Some(fs) = &mut self.film_subtitles {
            fs.step(frame, rate);
        }
        if over {
            self.end_film();
        }
    }

    fn end_film(&mut self) {
        self.film_subtitles = None;
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
        // invert flag +0x80, `invert_mouse` of the configuration through
        // @ 0x00429a30; +1 with it).
        let mut ax = -(dx * dt) * 0.3;
        let sign = if self.config.invert_mouse { 1.0 } else { -1.0 };
        let mut ay = dy * dt * 0.3 * sign;
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
        // `Atl_Dialog_Play`'s loop: its checks, then the scene's update.
        self.dialog_update(dt);
        self.menu_close_step();
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
            if self
                .randomized
                .iter()
                .any(|r| &r.object == object && !r.held)
            {
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
            self.draw_film_subtitle(out);
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
        self.draw_menu(out);
        self.draw_inventory(out);
        self.draw_dialog(out);
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

// -- dialogs, the subject menu, film subtitles, saves ------------------------

impl Stage {
    /// The sprite of `object` given `animation` in `mode` with the same
    /// priority (@ 0x00441a90 then @ 0x00441830(file, priority, 0)).
    fn replace_anim(&mut self, object: &[u8], animation: &[u8], mode: Mode) -> bool {
        let Some(p) = self
            .sprites
            .iter()
            .find(|s| s.object == object)
            .map(|s| s.priority)
        else {
            return false;
        };
        self.add_sprite(object, animation, mode, p, false).is_some()
    }

    /// Holds or releases the object's randomize record.
    fn hold_randomized(&mut self, object: &[u8], held: bool) {
        for r in &mut self.randomized {
            if r.object == object {
                r.held = held;
            }
        }
    }

    /// Whether the `facing` sprite of `object` has run its length
    /// (@ 0x00448c60 the time, @ 0x00448c70 (last − first) / fps); a
    /// sprite that is no longer there counts as done.
    fn facing_done(&self, object: &[u8]) -> bool {
        match self.sprites.iter().find(|s| s.object == object) {
            Some(s) if s.animation == b"facing" => {
                let len = f64::from(s.last.wrapping_sub(s.first)) / SPRITE_FPS;
                s.time >= len
            }
            _ => true,
        }
    }

    /// `Atl_Dialog_Play(id)` (@ 0x00407ac0): the dialog must exist
    /// (@ 0x00465fb0: not 0 and in the table). Unless the subject menu is
    /// open (the manager in state 3 with mode 9), each 3D talker whose
    /// object has a sprite in the view turns to face (`facing`, the
    /// player's default mode, the same priority, its randomize record
    /// held). The call then waits.
    fn dialog_start(&mut self, id: i32) -> bool {
        let Some(set) = &self.dialogs else {
            log::warn!("LUA_Dialog_Play : no dialogs.xml");
            return false;
        };
        let d = u32::try_from(id)
            .ok()
            .filter(|&i| i != 0)
            .and_then(|i| set.dialog(i));
        let Some(d) = d else {
            log::info!("LUA_Dialog_Play : Le dialogue {id} n'existe pas.");
            return false;
        };
        let talkers = dlg::talkers(set, d);
        let tirades: Vec<(String, String)> = d
            .tirades
            .iter()
            .map(|t| (t.talker.clone(), t.text.clone()))
            .collect();
        let mut run = DialogRun {
            player: Player::new(d.id, tirades.len()),
            talkers,
            tirades,
            facing: false,
            faced: Vec::new(),
            voice: None,
            bubble: None,
            portrait: None,
            text_h: 0,
            text: String::new(),
        };
        let in_menu = self.menu.is_some();
        if !in_menu {
            run.facing = true;
            let threed: Vec<Vec<u8>> = run
                .talkers
                .iter()
                .filter(|(_, p)| *p == atlantis3_data::atlantis5_texts::Picturing::ThreeD)
                .map(|(n, _)| n.as_bytes().to_vec())
                .collect();
            for o in threed {
                if self.sprites.iter().any(|s| s.object == o) {
                    self.hold_randomized(&o, true);
                    if self.replace_anim(&o, b"facing", Mode::Stop) {
                        run.faced.push(o);
                    }
                }
            }
        }
        self.dialog = Some(run);
        true
    }

    /// Whether the tirade's voice still plays (@ 0x00459be0).
    fn voice_playing(&self) -> bool {
        let Some(v) = self.dialog.as_ref().and_then(|d| d.voice.as_ref()) else {
            return false;
        };
        match &self.audio {
            Some(a) => a.is_playing(CHANNEL_VOICE),
            None => self.clock - v.started < v.length,
        }
    }

    /// One frame of the dialog (@ 0x00407ac0's loops): first the facing
    /// loop (each faced talker done gets `listen` in a loop, mode 3, its
    /// priority kept; when all are done the player starts), then the
    /// player's step with the frame's clicks; at its end the faced
    /// talkers get `unfacing` (their records released) and the call
    /// returns.
    fn dialog_update(&mut self, dt: f64) {
        let Some(run) = &self.dialog else { return };
        let clicks = std::mem::take(&mut self.clicks) > 0;
        let space = std::mem::take(&mut self.space);
        if run.facing {
            let faced = run.faced.clone();
            let mut all = true;
            for o in &faced {
                let is_facing = self
                    .sprites
                    .iter()
                    .any(|s| &s.object == o && s.animation == b"facing");
                if !is_facing {
                    continue;
                }
                if self.facing_done(o) {
                    self.replace_anim(o, b"listen", Mode::Loop);
                } else {
                    all = false;
                }
            }
            if all {
                if let Some(r) = &mut self.dialog {
                    r.facing = false;
                }
            }
            return;
        }
        let playing = self.voice_playing();
        let Some(run) = &mut self.dialog else { return };
        let act = run.player.step(dt, clicks, space, playing);
        match act {
            Some(Act::Start(k, _)) => self.tirade_start(k),
            Some(Act::Next(_)) => {
                let Some(run) = &mut self.dialog else { return };
                if run.voice.take().is_some() {
                    if let Some(a) = &self.audio {
                        a.stop(CHANNEL_VOICE);
                    }
                }
                run.portrait = None;
                run.bubble = None;
            }
            Some(Act::Over) => self.dialog_end(),
            None => {}
        }
        if let Some(run) = &mut self.dialog {
            if let Some(f) = &mut run.portrait {
                // The 2D talker's film (the player's +0x14 step, case 2).
                while f.due(self.clock) {
                    f.do_frame();
                    if !f.next_frame() {
                        break;
                    }
                }
            }
        }
    }

    /// The player's state 0 (@ 0x00432a5c): the voice (`GetFileAttributes`
    /// gives +0x38), loaded as a type-2 sample and played at 1.0; the
    /// bubble and the box of the talker's picturing; a 2D talker's film.
    fn tirade_start(&mut self, k: usize) {
        let language = self.language.clone();
        let Some(run) = &self.dialog else { return };
        let (talker, text) = run.tirades[k].clone();
        let kind = Kind::of(
            run.talkers
                .iter()
                .find(|(n, _)| *n == talker)
                .map_or(atlantis3_data::atlantis5_texts::Picturing::None, |t| t.1),
        );
        let path = dlg::voice_path(&language, run.player.id, k);
        let voice = self.read(&path);
        let has_voice = voice.is_some();
        let mut v = None;
        if let Some(data) = voice {
            match atlantis4_data::audio::decode_vorbis(&data) {
                Ok(d) => {
                    #[allow(clippy::cast_precision_loss)]
                    let length = d.samples.len() as f64
                        / f64::from(d.channels.max(1))
                        / f64::from(d.sample_rate.max(1));
                    if let Some(a) = &self.audio {
                        // @ 0x00459630, type 2: 1.0 × voice × master.
                        let g = self.config.voice * self.config.master;
                        a.play(
                            CHANNEL_VOICE,
                            d.sample_rate,
                            u16::from(d.channels),
                            Arc::new(d.samples),
                            false,
                            [g; 2],
                        );
                    }
                    v = Some(Voice {
                        length,
                        started: self.clock,
                    });
                }
                Err(e) => log::warn!("{path}: {e:?}"),
            }
        }
        let bubble = self
            .read(&format!("datas/sprites2d/dialog/{}", kind.bubble()))
            .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok());
        let portrait = if kind == Kind::TwoD {
            let file = format!("datas/videos/{talker}.bik");
            self.read(&file)
                .and_then(|d| Film::open(&talker, Arc::from(d), Depth::Xrgb8888, self.clock).ok())
        } else {
            None
        };
        let text_h = self.fonts.text_height("AtlFont_Dialog", &text, 0x2f8);
        let Some(run) = &mut self.dialog else { return };
        run.voice = v;
        run.bubble = bubble;
        run.portrait = portrait;
        run.text_h = text_h;
        run.text = text;
        run.player.started(kind, has_voice);
    }

    /// The end of `Atl_Dialog_Play`: the faced talkers' `unfacing` (the
    /// player's default mode, their records released, @ 0x00443570), and
    /// the script goes on.
    fn dialog_end(&mut self) {
        let Some(run) = self.dialog.take() else {
            return;
        };
        if let (Some(_), Some(a)) = (&run.voice, &self.audio) {
            a.stop(CHANNEL_VOICE);
        }
        for o in &run.faced {
            if self.sprites.iter().any(|s| &s.object == o) {
                self.replace_anim(o, b"unfacing", Mode::Stop);
            }
            self.hold_randomized(o, false);
        }
        self.wait_ended = true;
    }

    /// Whether a dialog runs.
    pub fn dialog_running(&self) -> bool {
        self.dialog.is_some()
    }

    /// The tirade shown (dialog id, tirade, state), for tests.
    pub fn dialog_state(&self) -> Option<(u32, usize, u32)> {
        self.dialog
            .as_ref()
            .map(|d| (d.player.id, d.player.tirade, d.player.state))
    }

    /// The dialog over the view (@ 0x00433910, states 1..3): the 2D
    /// talker's film in its rectangle with a white frame
    /// (@ 0x0048e8d0(…, 0xffffff)), the bubble at its place, the text in
    /// black (`AtlFont_Dialog`, layout 0, 0, 0, 0, 0, 0) on a 760-wide
    /// surface blitted with its colour key into the 760 × 100 box, moved
    /// up by the scroll.
    fn draw_dialog(&self, out: &mut [u32]) {
        let Some(run) = &self.dialog else { return };
        if run.facing || !matches!(run.player.state, 1..=3) {
            return;
        }
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let (w, h) = (VIEW_WIDTH as i32, VIEW_HEIGHT as i32);
        let kind = run.player.kind;
        if let Some(f) = &run.portrait {
            let (l, t, r, b) = dlg::portrait_rect(h);
            let (fw, fh) = f.size();
            for y in t..b {
                for x in l..r {
                    #[allow(clippy::cast_sign_loss)]
                    let (sx, sy) = ((x - l) as usize, (y - t) as usize);
                    if sx < fw && sy < fh && (0..w).contains(&x) && (0..h).contains(&y) {
                        #[allow(clippy::cast_sign_loss)]
                        let i = (y * w + x) as usize;
                        out[i] = f.xrgb[sy * fw + sx];
                    }
                }
            }
            for x in l..=r {
                for y in [t, b] {
                    if (0..w).contains(&x) && (0..h).contains(&y) {
                        #[allow(clippy::cast_sign_loss)]
                        let i = (y * w + x) as usize;
                        out[i] = 0x00ff_ffff;
                    }
                }
            }
            for y in t..=b {
                for x in [l, r] {
                    if (0..w).contains(&x) && (0..h).contains(&y) {
                        #[allow(clippy::cast_sign_loss)]
                        let i = (y * w + x) as usize;
                        out[i] = 0x00ff_ffff;
                    }
                }
            }
        }
        if let Some(b) = &run.bubble {
            let (x, y) = dlg::bubble_at(kind, w, h);
            let mut t = Target::screen(out, VIEW_WIDTH, VIEW_HEIGHT);
            b.draw(&mut t, Some(x), y, 0);
        }
        let (bx, by, bw, bh) = dlg::text_box(kind, w, h);
        let scroll = dlg::scroll_px(run.player.time, run.text_h, bh);
        let lines = self.fonts.lines(
            "AtlFont_Dialog",
            &run.text,
            bw,
            run.text_h.max(bh) + 1,
            crate::text::Zone::new(0, 0, 0, 0, 0, 0),
        );
        // The text surface (+0x3c): filled with its colour key, white
        // (@ 0x0047c2d0(surface, 0xffffffff); the key set with
        // DDCKEY_SRCBLT, the vtable's +0x74), the text drawn in black on
        // it, then blitted with the key (0x8000, DDBLT_KEYSRC): every
        // pixel but pure white is copied.
        #[allow(clippy::cast_sign_loss)]
        let (uw, uh) = (bw as usize, bh as usize);
        let mut surf = vec![0x00ff_ffffu32; uw * uh];
        self.fonts
            .draw("AtlFont_Dialog", &lines, 0, &mut surf, uw, uh, 0, -scroll);
        for y in 0..uh {
            for x in 0..uw {
                let p = surf[y * uw + x];
                if p & 0x00ff_ffff == 0x00ff_ffff {
                    continue;
                }
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                let (dx, dy) = (bx + x as i32, by + y as i32);
                if (0..w).contains(&dx) && (0..h).contains(&dy) {
                    #[allow(clippy::cast_sign_loss)]
                    let i = (dy * w + dx) as usize;
                    out[i] = p & 0x00ff_ffff;
                }
            }
        }
    }

    /// `Atl_TerminateDialog` (@ 0x00405d30 → 0x0041b770 → the menu's
    /// +0xc): the subject menu closes (@ 0x00411a70: state 5).
    fn terminate_dialog(&mut self) {
        if let Some(m) = &mut self.menu {
            m.state = MenuState::Closing;
        }
    }

    /// The subject menu opens (mode 9, @ 0x00410f80(name), the menu's
    /// Initialize @ 0x004117f0): the character's sprite turns to face
    /// (`facing`, its priority kept, its record held), state 0.
    fn menu_open(&mut self, character: &[u8]) {
        self.hold_randomized(character, true);
        self.replace_anim(character, b"facing", Mode::Stop);
        self.menu = Some(SubjectMenu {
            character: character.to_vec(),
            state: MenuState::Facing,
            subjects: Vec::new(),
            bubble: None,
            images: Vec::new(),
            picked: None,
            closed: false,
        });
    }

    /// The subject menu's character whose subjects it needs now (its
    /// elements made: after the facing, or again after a subject's
    /// script, @ 0x00410f90 states 0 and 2).
    pub fn menu_wants_subjects(&mut self) -> Option<Vec<u8>> {
        let m = self.menu.as_ref()?;
        match m.state {
            MenuState::Facing => {
                let c = m.character.clone();
                let facing = self
                    .sprites
                    .iter()
                    .any(|s| s.object == c && s.animation == b"facing");
                if !facing {
                    // `CMenu_Dialog :: Process -> Le sprite %s n'existe pas`
                    log::info!(
                        "CMenu_Dialog :: Process -> no facing sprite of {}",
                        lossy(&c)
                    );
                    return Some(c);
                }
                if self.facing_done(&c) {
                    self.replace_anim(&c, b"listen", Mode::Loop);
                    return Some(c);
                }
                None
            }
            MenuState::Rebuild => Some(m.character.clone()),
            _ => None,
        }
    }

    /// The subject menu's elements (@ 0x00411a90): the subjects of
    /// `Atl_Character_GetSubjects`, in its order (no sort); the bubble
    /// `subject_bubble_<n>.spr` at (W · 3 / 5, H / 3); each subject's
    /// image `sprites2d/dialog/subjects/<name>.spr` (`proxy_subject.spr`
    /// when missing), drawn at half its alpha when disabled
    /// (@ 0x00412b60: 0.5), and its 64 × 64 area. No subject: none, and
    /// the menu stays.
    pub fn menu_set_subjects(&mut self, subjects: Vec<(Vec<u8>, bool)>) {
        let Some(m) = &self.menu else { return };
        let _ = m;
        let bubble = self
            .read(&format!(
                "datas/sprites2d/dialog/{}",
                dlg::subject_bubble(subjects.len())
            ))
            .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok());
        let images: Vec<Option<Bank>> = subjects
            .iter()
            .map(|(name, _)| {
                self.read(&format!(
                    "datas/sprites2d/dialog/subjects/{}.spr",
                    lossy(name)
                ))
                .or_else(|| self.read("datas/sprites2d/dialog/proxy_subject.spr"))
                .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok())
            })
            .collect();
        if let Some(m) = &mut self.menu {
            m.subjects = subjects;
            m.bubble = bubble;
            m.images = images;
            m.state = MenuState::Open;
        }
        self.mouse = (
            i32::try_from(VIEW_WIDTH).unwrap_or(0) / 2,
            i32::try_from(VIEW_HEIGHT).unwrap_or(0) / 2,
        );
    }

    /// The subject menu's bubble hot spot.
    fn menu_origin() -> (i32, i32) {
        #[allow(clippy::cast_possible_truncation)]
        (
            (VIEW_WIDTH as u32 * 3 / 5) as i32,
            (VIEW_HEIGHT as u32 / 3) as i32,
        )
    }

    /// The subject under screen point (x, y).
    pub fn menu_subject_at(&self, x: i32, y: i32) -> Option<usize> {
        let m = self.menu.as_ref()?;
        if m.state != MenuState::Open {
            return None;
        }
        let (ox, oy) = Self::menu_origin();
        dlg::subject_layout(m.subjects.len())
            .iter()
            .position(|&(dx, dy)| {
                let (l, t) = (ox + dx, oy + dy);
                (l..l + 0x40).contains(&x) && (t..t + 0x40).contains(&y)
            })
    }

    /// The menu's position of subject `i` (tests and tools).
    #[doc(hidden)]
    pub fn menu_subject_point(&self, i: usize) -> Option<(i32, i32)> {
        let m = self.menu.as_ref()?;
        let (ox, oy) = Self::menu_origin();
        dlg::subject_layout(m.subjects.len())
            .get(i)
            .map(|&(dx, dy)| (ox + dx + 32, oy + dy + 32))
    }

    /// The subjects shown.
    pub fn menu_subjects(&self) -> Vec<Vec<u8>> {
        self.menu
            .as_ref()
            .map(|m| m.subjects.iter().map(|s| s.0.clone()).collect())
            .unwrap_or_default()
    }

    /// A left press in the open menu at the mouse (@ 0x0040d920 then
    /// @ 0x00410f90 state 1): on a subject, its click (@ 0x00412ba0 →
    /// 0x004129c0: `click_subject`, state 2, `on_click`); elsewhere the
    /// menu closes (state 5).
    pub(crate) fn menu_click(&mut self) {
        let (x, y) = self.mouse;
        let hit = self.menu_subject_at(x, y);
        let Some(m) = &mut self.menu else { return };
        if m.state != MenuState::Open {
            return;
        }
        match hit {
            Some(i) => {
                m.picked = Some(m.subjects[i].0.clone());
                m.state = MenuState::Rebuild;
            }
            None => m.state = MenuState::Closing,
        }
    }

    /// A subject picked: (character, subject), taken once.
    pub fn menu_take_pick(&mut self) -> Option<(Vec<u8>, Vec<u8>)> {
        let m = self.menu.as_mut()?;
        let s = m.picked.take()?;
        Some((m.character.clone(), s))
    }

    /// The closing (@ 0x00410f90 state 5): the elements gone, the
    /// character's `unfacing` when its sprite is there (@ 0x004414d0), its
    /// record released; the menu is over.
    fn menu_close_step(&mut self) {
        let Some(m) = &self.menu else { return };
        if m.state != MenuState::Closing {
            return;
        }
        let c = m.character.clone();
        if self.sprites.iter().any(|s| s.object == c) {
            self.replace_anim(&c, b"unfacing", Mode::Stop);
        }
        self.hold_randomized(&c, false);
        if let Some(m) = &mut self.menu {
            m.closed = true;
        }
    }

    /// Whether the subject menu has ended; it is dropped then.
    pub fn menu_take_closed(&mut self) -> bool {
        if self.menu.as_ref().is_some_and(|m| m.closed) {
            self.menu = None;
            return true;
        }
        false
    }

    /// Whether the subject menu is open.
    pub fn menu_open_now(&self) -> bool {
        self.menu.is_some()
    }

    /// The subject menu over the view (@ 0x00411790: not in states 2
    /// and 5): the bubble, the images (half alpha when disabled).
    fn draw_menu(&self, out: &mut [u32]) {
        let Some(m) = &self.menu else { return };
        if m.state != MenuState::Open {
            return;
        }
        let (ox, oy) = Self::menu_origin();
        let mut t = Target::screen(out, VIEW_WIDTH, VIEW_HEIGHT);
        if let Some(b) = &m.bubble {
            b.draw(&mut t, Some(ox), oy, 0);
        }
        for (i, (dx, dy)) in dlg::subject_layout(m.subjects.len())
            .into_iter()
            .enumerate()
        {
            if let Some(Some(img)) = m.images.get(i) {
                if m.subjects[i].1 {
                    img.draw(&mut t, Some(ox + dx), oy + dy, 0);
                } else {
                    // `ftol(0.5 × 31)` = 15.
                    img.draw_trs(&mut t, Some(ox + dx), oy + dy, 0, 15);
                }
            }
        }
    }

    /// The inventory's sprites (@ 0x00414db0): `background.spr`,
    /// `button_left[_focus].spr`, `button_right[_focus].spr` of
    /// `sprites2d/menu/inventory/` (path 25); each item's icon
    /// `sprites2d/objects_icon/<item>.spr` (path 18), `proxy_object.spr`
    /// when missing; the hand's icon likewise.
    fn inventory_build(
        &self,
        size: usize,
        items: Vec<Option<Vec<u8>>>,
        hand: Vec<u8>,
        offset: usize,
    ) -> InventoryMenu {
        let spr = |path: &str| {
            self.read(path)
                .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok())
        };
        let dir = "datas/sprites2d/menu/inventory";
        let icon = |name: &[u8]| {
            spr(&format!("datas/sprites2d/objects_icon/{}.spr", lossy(name)))
                .or_else(|| spr("datas/sprites2d/objects_icon/proxy_object.spr"))
        };
        let icons = items
            .iter()
            .map(|i| i.as_ref().and_then(|n| icon(n)))
            .collect();
        InventoryMenu {
            offset,
            size,
            hand_icon: (!hand.is_empty()).then(|| icon(&hand)).flatten(),
            items,
            background: spr(&format!("{dir}/background.spr")),
            left: [
                spr(&format!("{dir}/button_left.spr")),
                spr(&format!("{dir}/button_left_focus.spr")),
            ],
            right: [
                spr(&format!("{dir}/button_right.spr")),
                spr(&format!("{dir}/button_right_focus.spr")),
            ],
            icons,
            closed: false,
        }
    }

    /// The inventory opens (mode 8, its Initialize @ 0x00414bb0: the
    /// elements, alpha 1, state 2).
    pub fn inventory_open(&mut self, size: usize, items: Vec<Option<Vec<u8>>>, hand: Vec<u8>) {
        self.inventory = Some(self.inventory_build(size, items, hand, 0));
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        {
            self.mouse = (VIEW_WIDTH as i32 / 2, VIEW_HEIGHT as i32 / 2);
        }
    }

    /// The elements made again (@ 0x00415fe0) after a click.
    pub fn inventory_refresh(&mut self, size: usize, items: Vec<Option<Vec<u8>>>, hand: Vec<u8>) {
        let offset = self.inventory.as_ref().map_or(0, |i| i.offset);
        if self.inventory.is_some() {
            self.inventory = Some(self.inventory_build(size, items, hand, offset));
        }
    }

    /// Whether the inventory is open.
    pub fn inventory_open_now(&self) -> bool {
        self.inventory.is_some()
    }

    /// The items shown, from the offset (tests).
    pub fn inventory_items(&self) -> Vec<Option<Vec<u8>>> {
        self.inventory
            .as_ref()
            .map(|i| i.items.clone())
            .unwrap_or_default()
    }

    /// The layout (@ 0x00414bb0, the grid's size its background's): the
    /// grid at ((W − w) / 2, H − h − 10), the left button left of it, the
    /// right one right of it, both 10 above the bottom; the grid's 10
    /// cells at (3 + 80 i, 4), 64 × 64 (@ 0x00415397..0x004153c0)
    /// [INFERRED: positions as the elements' top-left hot spots].
    fn inventory_layout(inv: &InventoryMenu) -> ((i32, i32), (i32, i32), (i32, i32)) {
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let (w, h) = (VIEW_WIDTH as i32, VIEW_HEIGHT as i32);
        let size = |b: &Option<Bank>| {
            b.as_ref()
                .and_then(|b| b.info(0))
                .map_or((0, 0), |i| (i32::from(i.w), i32::from(i.h)))
        };
        let (gw, gh) = size(&inv.background);
        let (lw, lh) = size(&inv.left[0]);
        let (_, rh) = size(&inv.right[0]);
        let gx = (w - gw) / 2;
        (
            (gx, h - gh - 10),
            (gx - lw, h - lh - 10),
            ((w + gw) / 2, h - rh - 10),
        )
    }

    /// What a left press at the mouse hits.
    pub fn inventory_hit(&self) -> Option<InventoryHit> {
        let inv = self.inventory.as_ref()?;
        let (x, y) = self.mouse;
        let ((gx, gy), (lx, ly), (rx, ry)) = Self::inventory_layout(inv);
        for c in 0..10usize {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            let (cx, cy) = (gx + 3 + 80 * c as i32, gy + 4);
            if (cx..cx + 64).contains(&x) && (cy..cy + 64).contains(&y) {
                return Some(InventoryHit::Slot(c + inv.offset));
            }
        }
        let inside = |b: &Option<Bank>, bx: i32, by: i32| {
            b.as_ref().and_then(|b| b.info(0)).is_some_and(|i| {
                (bx..bx + i32::from(i.w)).contains(&x) && (by..by + i32::from(i.h)).contains(&y)
            })
        };
        if inside(&inv.left[0], lx, ly) {
            return Some(InventoryHit::Left);
        }
        if inside(&inv.right[0], rx, ry) {
            return Some(InventoryHit::Right);
        }
        None
    }

    /// The buttons (@ 0x00417670 → 0x00414d50: the offset − 1 when above
    /// 0; @ 0x004176a0 → 0x00414d60: + 1 while offset + 10 < size).
    pub fn inventory_page(&mut self, right: bool) {
        let Some(inv) = &mut self.inventory else {
            return;
        };
        if right {
            if inv.offset + 10 < inv.size {
                inv.offset += 1;
            }
        } else if inv.offset > 0 {
            inv.offset -= 1;
        }
    }

    /// A right press (@ 0x00414a30 state 2): state 4, the inventory ends.
    pub fn inventory_close(&mut self) {
        if let Some(inv) = &mut self.inventory {
            inv.closed = true;
        }
    }

    /// Whether the inventory has ended; it is dropped then.
    pub fn inventory_take_closed(&mut self) -> bool {
        if self.inventory.as_ref().is_some_and(|i| i.closed) {
            self.inventory = None;
            return true;
        }
        false
    }

    /// The inventory over the view (@ 0x00414b70): the grid and buttons
    /// (a button's focus picture under the mouse [INFERRED]), the icons
    /// in their cells, the hand's icon at (5, 30) (@ 0x00415a8f..).
    fn draw_inventory(&self, out: &mut [u32]) {
        let Some(inv) = &self.inventory else { return };
        let ((gx, gy), (lx, ly), (rx, ry)) = Self::inventory_layout(inv);
        let hit = self.inventory_hit();
        let mut t = Target::screen(out, VIEW_WIDTH, VIEW_HEIGHT);
        if let Some(b) = &inv.background {
            b.draw(&mut t, Some(gx), gy, 0);
        }
        let lb = usize::from(hit == Some(InventoryHit::Left));
        if let Some(b) = inv.left[lb].as_ref().or(inv.left[0].as_ref()) {
            b.draw(&mut t, Some(lx), ly, 0);
        }
        let rb = usize::from(hit == Some(InventoryHit::Right));
        if let Some(b) = inv.right[rb].as_ref().or(inv.right[0].as_ref()) {
            b.draw(&mut t, Some(rx), ry, 0);
        }
        for c in 0..10usize {
            if let Some(Some(icon)) = inv.icons.get(c + inv.offset) {
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                icon.draw(&mut t, Some(gx + 3 + 80 * c as i32), gy + 4, 0);
            }
        }
        if let Some(h) = &inv.hand_icon {
            h.draw(&mut t, Some(5), 0x1e, 0);
        }
    }

    /// The film's subtitle (@ 0x0042b220): black at the bottom centre,
    /// then white 2 pixels up and left.
    fn draw_film_subtitle(&self, out: &mut [u32]) {
        let Some(text) = self.film_subtitles.as_ref().and_then(|f| f.shown.as_ref()) else {
            return;
        };
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let (w, h) = (VIEW_WIDTH as i32, VIEW_HEIGHT as i32);
        let lines = self
            .fonts
            .lines("AtlFont_Subtitle", text, w, h, dlg::SUBTITLE_ZONE);
        self.fonts.draw(
            "AtlFont_Subtitle",
            &lines,
            0,
            out,
            VIEW_WIDTH,
            VIEW_HEIGHT,
            0,
            0,
        );
        self.fonts.draw(
            "AtlFont_Subtitle",
            &lines,
            0x00ff_ffff,
            out,
            VIEW_WIDTH,
            VIEW_HEIGHT,
            -2,
            -2,
        );
    }

    /// A menu that pauses the sound opens (@ 0x0041b780: the music
    /// channels 0..3 paused, @ 0x0045b430; the sounds stopped,
    /// @ 0x00459740: FMOD channels 6 and 7) or its end lets the music go
    /// on (@ 0x0045b440).
    pub fn hold_sound(&mut self, hold: bool) {
        let Some(audio) = &self.audio else { return };
        for ch in 0..4 {
            audio.set_held(CHANNEL_MUSIC_2004 + ch, hold);
        }
        if hold {
            audio.stop(CHANNEL_SOUND);
            audio.stop(CHANNEL_VOICE);
            self.sound = None;
        }
    }

    /// The end of a game (@ 0x0042ffd0, the engine's part): the scene
    /// unloaded (@ 0x0044bb10), the camera controller reset
    /// (@ 0x0042cd90), the sounds stopped (@ 0x00459360), the music
    /// stopped; the sprites, the films and the dialogs with the scene
    /// [INFERRED: the parts of @ 0x0044bb10 and @ 0x0042cd90 are not
    /// read in detail].
    pub fn end_game(&mut self) {
        self.view = None;
        self.sprites.clear();
        self.persistent.clear();
        self.randomized.clear();
        self.film = None;
        self.film_subtitles = None;
        self.dialog = None;
        self.menu = None;
        self.inventory = None;
        self.mode = 0;
        self.turn = None;
        self.look_turn = None;
        self.boat = None;
        self.on_boat = false;
        self.speed = [0.0; 2];
        for ch in 0..MUSIC_CHANNELS {
            self.music_stop(ch, 0.0);
        }
        if let Some(audio) = &self.audio {
            audio.stop(CHANNEL_SOUND);
            audio.stop(CHANNEL_VOICE);
        }
        self.sound = None;
    }

    /// `Atl_Music_StopAll` and `Atl_Music_Play` for the menus.
    fn menu_music(&mut self, e: &Effect) {
        match e {
            Effect::MusicStopAll(fade) => {
                for ch in 0..MUSIC_CHANNELS {
                    self.music_stop(ch, *fade);
                }
            }
            #[allow(clippy::cast_possible_wrap)]
            Effect::MusicPlay(ch, name) => self.music_play(*ch as i32, name),
            _ => {}
        }
    }

    /// The film subtitle shown (tests).
    pub fn film_subtitle(&self) -> Option<String> {
        self.film_subtitles.as_ref().and_then(|f| f.shown.clone())
    }

    /// The folder of the player's saves (`saves/`).
    pub fn set_save_dir(&mut self, dir: Option<std::path::PathBuf>) {
        self.save_dir = dir;
    }
}

impl Engine for Stage {
    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        self.src.read(&lossy(path))
    }

    #[allow(clippy::too_many_lines)]
    fn call(&mut self, call: &Call) -> Reply {
        match call {
            Call::GameOver(key) => self.menu_requests.push((menus::DEATH, key.clone())),
            Call::ExamineItem(item) => self.menu_requests.push((menus::EXAMINE, item.clone())),
            // @ 0x00405d10: mode 12 asked of the menus (@ 0x0041b5d0).
            Call::PuzzlePokerStart => self.menu_requests.push((menus::POKER, Vec::new())),
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
                    held: false,
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
            Call::DialogPlay(id) => {
                if self.dialog_start(*id) {
                    return Reply::Wait;
                }
            }
            Call::TerminateDialog => self.terminate_dialog(),
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
            Request::TransitionEnd => {}
            Request::DeleteAutosave => {
                // @ 0x0042fc50: `saves/autoslot` deleted.
                if let Some(d) = &self.save_dir {
                    let _ = std::fs::remove_file(d.join(atlantis5_script::save::AUTOSLOT));
                }
            }
            Request::SubjectMenu { character } => self.menu_open(character),
        }
    }

    /// `CAMERA` (@ 0x00437920: `pitch`, `pitch_min`, `pitch_max`,
    /// `pitch_limited`, `yaw`, `yaw_min`, `yaw_max`, `yaw_limited`),
    /// `SOUND` (@ 0x0043a4f0: a `PLAYLIST` with `channel` and `name` for
    /// each of the 5 channels playing a list) and `PERSISTANT_SPRITES`
    /// (@ 0x00439f10: a `PS` with `name` and `time` each). The camera
    /// mover's section (`CAMERA_MOVER`: its speeds and `BoatFx`) is not
    /// written.
    fn save_sections(&mut self) -> Vec<atlantis4_data::xml::Element> {
        use atlantis5_script::save::{attr_bool, attr_f32, element};
        // `CConfiguration::Serialize` (@ 0x00429a60): `CONFIGURATION`
        // (0x0066c940) with `res_path` (a `VT_BSTR`, +0x08), `screen_size`
        // and `graphic_quality` (`VT_I4`), `master_vol`, `music_vol`,
        // `voice_vol`, `fx_vol` (floats, @ 0x00465390),
        // `display_crosshair`, `invert_mouse`, `display_subtitle`
        // (`VT_BOOL`). `res_path` is `./datas/` (0x0066c75c) in a player's
        // install (@ 0x00429620).
        let c = &self.config;
        let mut conf = element("CONFIGURATION");
        conf.attributes.push(("res_path".into(), "./datas/".into()));
        conf.attributes
            .push(("screen_size".into(), c.screen_size.to_string()));
        conf.attributes
            .push(("graphic_quality".into(), c.graphic_quality.to_string()));
        attr_f32(&mut conf, "master_vol", c.master);
        attr_f32(&mut conf, "music_vol", c.music);
        attr_f32(&mut conf, "voice_vol", c.voice);
        attr_f32(&mut conf, "fx_vol", c.fx);
        attr_bool(&mut conf, "display_crosshair", c.crosshair);
        attr_bool(&mut conf, "invert_mouse", c.invert_mouse);
        attr_bool(&mut conf, "display_subtitle", c.subtitles);
        let mut cam = element("CAMERA");
        let (pmin, pmax) = self.pitch_limit.unwrap_or((0.0, 0.0));
        let (ymin, ymax) = self.yaw_limit.unwrap_or((0.0, 0.0));
        attr_f32(&mut cam, "pitch", self.pitch);
        attr_f32(&mut cam, "pitch_min", pmin);
        attr_f32(&mut cam, "pitch_max", pmax);
        attr_bool(&mut cam, "pitch_limited", self.pitch_limit.is_some());
        attr_f32(&mut cam, "yaw", self.yaw);
        attr_f32(&mut cam, "yaw_min", ymin);
        attr_f32(&mut cam, "yaw_max", ymax);
        attr_bool(&mut cam, "yaw_limited", self.yaw_limit.is_some());
        let mut sound = element("SOUND");
        for (ch, m) in self.music.iter().enumerate() {
            if m.list.is_none() {
                continue;
            }
            let mut p = element("PLAYLIST");
            p.attributes.push(("channel".into(), ch.to_string()));
            p.attributes
                .push(("name".into(), atlantis4_data::xml::cp1252(&m.name)));
            sound.children.push(p);
        }
        let mut ps = element("PERSISTANT_SPRITES");
        for (object, _, time) in &self.persistent {
            let mut e = element("PS");
            e.attributes
                .push(("name".into(), atlantis4_data::xml::cp1252(object)));
            #[allow(clippy::cast_possible_truncation)]
            attr_f32(&mut e, "time", *time as f32);
            ps.children.push(e);
        }
        vec![conf, cam, sound, ps]
    }

    fn load_sections(&mut self, root: &atlantis4_data::xml::Element) {
        use atlantis5_script::save::{get_bool, get_f64};
        // `CConfiguration::Deserialize` (@ 0x00429e60, `./CONFIGURATION`):
        // each value through its setter; the reader runs it last, which
        // changes nothing here.
        #[allow(clippy::cast_possible_truncation)]
        if let Some(e) = root.child("CONFIGURATION") {
            let c = &mut self.config;
            let f = |n: &str, d: f32| get_f64(e, n).map_or(d, |v| v as f32);
            c.screen_size = get_f64(e, "screen_size").map_or(c.screen_size, |v| v as i32);
            c.graphic_quality =
                get_f64(e, "graphic_quality").map_or(c.graphic_quality, |v| v as i32);
            c.master = f("master_vol", c.master).clamp(0.0, 1.0);
            c.music = f("music_vol", c.music).clamp(0.0, 1.0);
            c.voice = f("voice_vol", c.voice).clamp(0.0, 1.0);
            c.fx = f("fx_vol", c.fx).clamp(0.0, 1.0);
            c.crosshair = get_bool(e, "display_crosshair").unwrap_or(c.crosshair);
            c.invert_mouse = get_bool(e, "invert_mouse").unwrap_or(c.invert_mouse);
            c.subtitles = get_bool(e, "display_subtitle").unwrap_or(c.subtitles);
        }
        #[allow(clippy::cast_possible_truncation)]
        if let Some(c) = root.child("CAMERA") {
            let f = |n: &str| get_f64(c, n).unwrap_or(0.0) as f32;
            self.pitch_limit = get_bool(c, "pitch_limited")
                .unwrap_or(false)
                .then(|| (f("pitch_min"), f("pitch_max")));
            self.yaw_limit = get_bool(c, "yaw_limited")
                .unwrap_or(false)
                .then(|| (f("yaw_min"), f("yaw_max")));
            self.pitch = f("pitch");
            self.yaw = f("yaw");
        }
        if let Some(ps) = root.child("PERSISTANT_SPRITES") {
            self.persistent = ps
                .children_named("PS")
                .filter_map(|e| {
                    let name = atlantis4_data::xml::to_cp1252(e.attr("name")?);
                    Some((name, 0, get_f64(e, "time").unwrap_or(0.0)))
                })
                .collect();
        }
        if let Some(snd) = root.child("SOUND") {
            for ch in 0..MUSIC_CHANNELS {
                self.music_stop(ch, 0.0);
            }
            for p in snd.children_named("PLAYLIST") {
                #[allow(clippy::cast_possible_truncation)]
                let ch = get_f64(p, "channel").unwrap_or(-1.0) as i32;
                if let Some(name) = p.attr("name") {
                    self.music_play(ch, &atlantis4_data::xml::to_cp1252(name));
                }
            }
        }
    }

    fn write_save(&mut self, name: &str, data: &[u8]) -> bool {
        let Some(d) = &self.save_dir else {
            return false;
        };
        std::fs::create_dir_all(d).is_ok() && std::fs::write(d.join(name), data).is_ok()
    }

    fn read_save(&mut self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.save_dir.as_ref()?.join(name)).ok()
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
    /// The menus (the manager's other modes).
    pub menus: Menus,
    /// The music held for a menu.
    sound_held: bool,
    /// The poker board (mode 12) when it is up.
    poker: Option<Poker>,
    src: Arc<dyn FileSource>,
}

/// The poker board's side of the game: the scripts' one-shot strings
/// (@ 0x00454220), the program's `rand`, the texts.
struct PokerHost<'a> {
    game: &'a mut Game,
    stage: &'a Rc<RefCell<Stage>>,
    menus: &'a Menus,
}

impl PokerHost<'_> {
    fn report(&self, e: &str) {
        log::warn!("poker: {e}");
    }
}

impl poker::Host for PokerHost<'_> {
    fn call(&mut self, source: &str, args: Option<&[Value]>, nresults: usize) -> Vec<Value> {
        match self.game.script().one_shot(source, args, nresults) {
            Ok(v) => v,
            Err(e) => {
                self.report(&e);
                vec![Value::Nil; nresults]
            }
        }
    }

    fn run(&mut self, source: &str, args: Option<Vec<Value>>) {
        if !self.game.run_one_shot(source, args) {
            self.report(&format!("{source}: not run"));
        }
    }

    fn busy(&self) -> bool {
        self.game.busy()
    }

    fn rand(&mut self) -> u32 {
        atlantis5_script::script::msvcrt_rand(&mut self.stage.borrow_mut().crt_rand)
    }

    fn string(&self, key: &str) -> String {
        self.menus.string(key)
    }
}

impl Play {
    /// A game on `src` (an `Atlantis5Source`), films' voices in
    /// `language` (a folder name of `sounds/video/`), sound to `audio`.
    pub fn new(
        src: Arc<dyn FileSource>,
        language: &str,
        audio: Option<Audio>,
    ) -> Result<Play, String> {
        let menus = Menus::new(src.clone(), language);
        let stage = Rc::new(RefCell::new(Stage::new(src.clone(), language, audio)));
        let game = Game::new(stage.clone()).map_err(|e| e.to_string())?;
        Ok(Play {
            game,
            stage,
            moves: Vec::new(),
            clicks: 0,
            hover: None,
            menus,
            sound_held: false,
            poker: None,
            src,
        })
    }

    /// The engine's side.
    pub fn stage(&self) -> std::cell::Ref<'_, Stage> {
        self.stage.borrow()
    }

    /// The engine's side, to change (tests and tools).
    #[doc(hidden)]
    pub fn stage_mut(&self) -> std::cell::RefMut<'_, Stage> {
        self.stage.borrow_mut()
    }

    /// Moves the mouse to the middle of the shown menu element `name` and
    /// presses the left button (tests and tools). False when no such
    /// element is shown.
    #[doc(hidden)]
    pub fn click_menu(&mut self, name: &str) -> bool {
        let p = {
            let st = self.stage.borrow();
            self.menus.point_of(name, &st.fonts)
        };
        let Some((x, y)) = p else { return false };
        self.mouse_at(x, y);
        // The move first (the hover), then the press.
        self.update(0.0);
        self.click();
        true
    }

    pub fn new_game(&mut self) {
        self.game.new_game();
    }

    /// The poker board, when it is up.
    pub fn poker(&self) -> Option<&Poker> {
        self.poker.as_ref()
    }

    /// Moves the mouse to the middle of the poker board's shown button
    /// `name` and presses the left button (tests and tools).
    #[doc(hidden)]
    pub fn click_poker(&mut self, name: &str) -> bool {
        let p = {
            let st = self.stage.borrow();
            self.poker
                .as_ref()
                .and_then(|p| p.point_of(name, &st.fonts))
        };
        let Some((x, y)) = p else { return false };
        self.mouse_at(x, y);
        self.update(0.0);
        self.click();
        true
    }

    /// The program's start (`CGameManager` ctor @ 0x0042eac0): the main
    /// menu asked (@ 0x0041b5d0(0)) and the request 3 queued: the game
    /// starts in the main menu.
    pub fn boot(&mut self) {
        self.menus.request(menus::MAIN);
        self.game.open_menu();
    }

    /// Whether the game asked to quit (the request 8 of the main menu's
    /// `quit_game`, @ 0x00428ea0).
    pub fn quit_requested(&self) -> bool {
        self.game.state() == 8
    }

    /// Escape (a keyboard event, key 1, pressed: @ 0x004300d0): in play,
    /// the in-game menu (mode 2) and state 3 (@ 0x0042fb90(3)); a film
    /// ends (@ 0x0042b0f0).
    pub fn escape(&mut self) {
        if self.stage.borrow().film_playing() {
            self.skip();
            return;
        }
        if self.game.state() == 4 && !self.game.busy() {
            self.menus.request(menus::IN_GAME);
            self.game.open_menu();
        }
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
        // A dialog takes the frame's clicks (`Atl_Dialog_Play`'s loop).
        if self.stage.borrow().dialog_running() {
            self.stage.borrow_mut().clicks += clicks;
        }
        self.stage.borrow_mut().update(dt, play);
        if std::mem::take(&mut self.stage.borrow_mut().wait_ended) {
            self.game.finish_wait();
        }
        let requests = std::mem::take(&mut self.stage.borrow_mut().menu_requests);
        for (mode, arg) in requests {
            if mode == menus::DEATH {
                self.menus.set_death_text(&lossy(&arg));
            } else if mode == menus::EXAMINE {
                self.menus.set_examine_item(&arg);
            }
            self.menus.request(mode);
        }
        if self.game.state() == 3 && !self.game.busy() {
            self.menu_frame(clicks, dt);
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

    /// State 3 (@ 0x0042f000 → @ 0x0041b780): the menu's process. The
    /// subject menu (mode 9) is ported; another mode (the inventory 8,
    /// the death panel 10 of `Atl_GameOver`, the examine view 11, poker
    /// 12, the game's menus) is not, and ends at once (logged), so the
    /// manager takes its next request.
    fn menu_frame(&mut self, clicks: usize, dt: f64) {
        if self.stage.borrow().inventory_open_now() {
            self.inventory_frame(clicks);
            return;
        }
        if !self.stage.borrow().menu_open_now() {
            self.menus_frame(clicks, dt);
            return;
        }
        let wants = self.stage.borrow_mut().menu_wants_subjects();
        if let Some(c) = wants {
            let list = match self.game.script().character_subject_list(&c) {
                Ok(l) => l.into_iter().map(|(n, on, _)| (n, on)).collect(),
                Err(e) => {
                    log::warn!("{e}");
                    Vec::new()
                }
            };
            self.stage.borrow_mut().menu_set_subjects(list);
        }
        for _ in 0..clicks {
            self.stage.borrow_mut().menu_click();
            let pick = self.stage.borrow_mut().menu_take_pick();
            if let Some((c, s)) = pick {
                self.game.click_subject(&c, &s);
                break;
            }
        }
        if self.stage.borrow_mut().menu_take_closed() {
            // A mode queued meanwhile (`Atl_PuzzlePoker_Start` from a
            // subject's script): the manager stays in state 3 and opens it
            // at the next frame (@ 0x0041b780).
            if !self.menus.active() {
                self.game.menu_closed();
            }
        }
    }

    /// The menus' frame (@ 0x0041b780): the next mode made when none is
    /// up (a mode that pauses the sound holds the music); its process;
    /// what it asks done; at its end with nothing queued, the music goes
    /// on and the manager takes its next request. With no menu up nor
    /// queued, state 3 ends at once (logged).
    fn menus_frame(&mut self, clicks: usize, dt: f64) {
        if self.poker.is_none() && self.menus.take_next(menus::POKER) {
            // The mode opens (@ 0x0041b780): the mouse at the screen's
            // centre, the board's init (state 0, @ 0x00425890).
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            self.menus
                .mouse_at(VIEW_WIDTH as i32 / 2, VIEW_HEIGHT as i32 / 2);
            let host = PokerHost {
                game: &mut self.game,
                stage: &self.stage,
                menus: &self.menus,
            };
            self.poker = Some(Poker::new(&self.src, &host));
        }
        if let Some(mut poker) = self.poker.take() {
            let mouse = self.menus.mouse;
            let closed = {
                let stage = self.stage.clone();
                let measure = |f: &str, t: &str, w: i32| stage.borrow().fonts.text_height(f, t, w);
                let mut host = PokerHost {
                    game: &mut self.game,
                    stage: &self.stage,
                    menus: &self.menus,
                };
                poker.frame(&mut host, mouse, clicks, &measure)
            };
            if closed {
                // Its process returned 1: mode 0xd; with a mode queued
                // (the death panel of `Atl_GameOver`) the manager stays in
                // state 3, else it takes its next request.
                if !self.menus.active() {
                    self.game.menu_closed();
                }
            } else {
                self.poker = Some(poker);
            }
            return;
        }
        // Mode 8 queued (after the examine view of an inventory item): the
        // inventory again.
        if self.menus.take_next(menus::INVENTORY) {
            let (size, items) = self.game.script().inventory();
            let hand = self.game.script().handled_item().unwrap_or_default();
            self.stage.borrow_mut().inventory_open(size, items, hand);
            return;
        }
        if !self.menus.active() {
            log::info!("menu of state 3: none asked, closed");
            self.game.menu_closed();
            return;
        }
        for _ in 0..clicks {
            self.menus.click();
        }
        let ended = {
            let mut st = self.stage.borrow_mut();
            let st = &mut *st;
            self.menus.step(dt, &st.fonts, &mut st.config)
        };
        if let Some(m) = self.menus.current() {
            if menus::pauses_sound(m) && !self.sound_held {
                self.sound_held = true;
                self.stage.borrow_mut().hold_sound(true);
            }
        }
        for e in self.menus.take_effects() {
            self.menu_effect(e);
        }
        if ended.is_some() {
            if self.sound_held {
                self.sound_held = false;
                self.stage.borrow_mut().hold_sound(false);
            }
            self.game.menu_closed();
        }
    }

    /// What a menu asked.
    fn menu_effect(&mut self, e: Effect) {
        let dir = self.stage.borrow().save_dir.clone();
        match e {
            Effect::NewGame => {
                self.poker = None;
                self.stage.borrow_mut().end_game();
                self.game.new_game();
            }
            Effect::Load(file) => {
                self.game.load_file(&file);
            }
            Effect::Save(file) => self.game.save_now(&file),
            Effect::Request(r) => self.game.queue_request(r),
            Effect::Delete(file) => {
                if let Some(d) = dir {
                    let p = d.join(file);
                    if p.is_file() {
                        let _ = std::fs::remove_file(p);
                    }
                }
            }
            Effect::Copy { from, to } => {
                if let Some(d) = dir {
                    let (f, t) = (d.join(from), d.join(to));
                    if f.is_file() {
                        let _ = std::fs::copy(f, t);
                    }
                }
            }
            Effect::EndGame => {
                self.poker = None;
                self.game.end_game();
                let mut st = self.stage.borrow_mut();
                st.end_game();
                st.config.reset_for_new_session();
            }
            Effect::Config => {}
            Effect::MusicStopAll(_) | Effect::MusicPlay(..) => {
                self.stage.borrow_mut().menu_music(&e);
            }
        }
    }

    /// A press of the right button (the mouse event of kind 0 with the
    /// buttons 4, @ 0x004300d0), in play: with something in the hand,
    /// `return Atl_Hero_ArrangeHandledObject` (0x0066ce60, run as the
    /// one-shot strings are) puts it back in the inventory; with an empty
    /// hand the original opens the inventory (mode 8, state 3), which is
    /// not ported (logged).
    pub fn right_click(&mut self) {
        if self.game.state() != 4 || self.game.busy() {
            return;
        }
        match self.game.script().handled_item() {
            Ok(h) if !h.is_empty() => {
                self.game.run_string("return Atl_Hero_ArrangeHandledObject");
            }
            Ok(_) => {
                // Mode 8, state 3 (@ 0x004305d0).
                let (size, items) = self.game.script().inventory();
                self.game.open_menu();
                self.stage
                    .borrow_mut()
                    .inventory_open(size, items, Vec::new());
            }
            Err(e) => log::warn!("{e}"),
        }
    }

    /// A right press while the inventory is open: on a cell the grid
    /// takes it (@ 0x004505c0: buttons & 4 → its event 3) and its handler
    /// (@ 0x004173a0) shows the cell's item in the examine view when the
    /// item has a detailed picture (`objects_detailed/<item>.spr`): the
    /// item given (@ 0x004137b0), modes 11 then 8 asked, the inventory
    /// ended; elsewhere the inventory ends (@ 0x00414a30 state 2: a mouse
    /// event with the buttons 4 that no element took).
    pub fn right_press(&mut self) {
        if !(self.game.state() == 3 && self.stage.borrow().inventory_open_now()) {
            self.right_click();
            return;
        }
        let hit = self.stage.borrow().inventory_hit();
        match hit {
            Some(InventoryHit::Slot(s)) => {
                let item = self
                    .stage
                    .borrow()
                    .inventory_items()
                    .get(s)
                    .cloned()
                    .flatten();
                let Some(item) = item else { return };
                let path = format!("datas/sprites2d/objects_detailed/{}.spr", lossy(&item));
                if self.stage.borrow().read(&path).is_some() {
                    self.menus.set_examine_item(&item);
                    self.menus.request(menus::EXAMINE);
                    self.menus.request(menus::INVENTORY);
                    self.stage.borrow_mut().inventory_close();
                }
            }
            Some(_) => {}
            None => self.stage.borrow_mut().inventory_close(),
        }
    }

    /// The inventory's frame: each left press on a slot (@ 0x00417120)
    /// or a button; then the elements made again (@ 0x00415fe0) from the
    /// database; at its end the manager's next request.
    fn inventory_frame(&mut self, clicks: usize) {
        for _ in 0..clicks {
            let hit = self.stage.borrow().inventory_hit();
            match hit {
                Some(InventoryHit::Slot(s)) => {
                    self.game.click_inventory(s);
                }
                Some(InventoryHit::Left) => self.stage.borrow_mut().inventory_page(false),
                Some(InventoryHit::Right) => self.stage.borrow_mut().inventory_page(true),
                None => continue,
            }
            let (size, items) = self.game.script().inventory();
            let hand = self.game.script().handled_item().unwrap_or_default();
            self.stage.borrow_mut().inventory_refresh(size, items, hand);
        }
        // Its end: the manager's next mode (the examine view), or the
        // game manager's next request.
        if self.stage.borrow_mut().inventory_take_closed() && !self.menus.active() {
            self.game.menu_closed();
        }
    }

    /// Whether a menu is up (the manager's state 3): the mouse moves a
    /// cursor then.
    pub fn in_menu(&self) -> bool {
        self.game.state() == 3
    }

    /// The mouse at (x, y) on the 1024 × 576 picture (menus).
    pub fn mouse_at(&mut self, x: i32, y: i32) {
        self.stage.borrow_mut().mouse = (x, y);
        self.menus.mouse_at(x, y);
    }

    /// The folder of the player's saves.
    pub fn set_save_dir(&mut self, dir: Option<std::path::PathBuf>) {
        self.menus.save_dir.clone_from(&dir);
        self.stage.borrow_mut().set_save_dir(dir);
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
        self.aim_at(id, None)
    }

    /// As [`Play::aim_at_zone`], for the zone numbered `id` named `name`
    /// (several zones of a view may share a number).
    #[doc(hidden)]
    pub fn aim_at_zone_named(&mut self, id: i32, name: &[u8]) -> bool {
        self.aim_at(id, Some(name))
    }

    fn aim_at(&mut self, id: i32, name: Option<&[u8]>) -> bool {
        let mut st = self.stage.borrow_mut();
        let Some(points) = st
            .view
            .as_ref()
            .and_then(|v| {
                v.zones.iter().find(|z| {
                    z.value == id
                        && z.kind < 1000
                        && name.is_none_or(|n| z.name.as_deref().unwrap_or("").as_bytes() == n)
                })
            })
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
        let c = normalize3(d);
        let (pitch, yaw) = crate::atlantis5_camera::angles_of(&c);
        st.set_pitch(pitch);
        st.set_yaw(yaw);
        drop(st);
        self.pick();
        // The sum's direction can fall outside a zone that is not convex:
        // then directions between it and each point, until the pick takes
        // the zone.
        let taken = |h: &Option<Hover>| matches!(h, Some(Hover::Zone(i, n)) if *i == id && name.is_none_or(|w| n == w));
        if taken(&self.hover) {
            return true;
        }
        for k in [0.25f32, 0.5, 0.75] {
            for p in &points {
                let q = normalize3(*p);
                let v = normalize3([
                    c[0] * (1.0 - k) + q[0] * k,
                    c[1] * (1.0 - k) + q[1] * k,
                    c[2] * (1.0 - k) + q[2] * k,
                ]);
                let (pitch, yaw) = crate::atlantis5_camera::angles_of(&v);
                {
                    let mut st = self.stage.borrow_mut();
                    st.set_pitch(pitch);
                    st.set_yaw(yaw);
                }
                self.pick();
                if taken(&self.hover) {
                    return true;
                }
            }
        }
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

    /// Space (DirectInput 0x39): a dialog ends (@ 0x00432a0f: the player's
    /// +0x39), a film too.
    pub fn space(&mut self) {
        let dialog = self.stage.borrow().dialog_running();
        if dialog {
            self.stage.borrow_mut().space = true;
        } else {
            self.skip();
        }
    }

    /// The picture (1024 × 576 x8r8g8b8): the view, then the menu up
    /// with the cursor (the subject menu and the inventory draw theirs
    /// over the view, with the menus' cursor).
    pub fn render(&self, out: &mut [u32]) {
        let st = self.stage.borrow();
        st.render(out);
        if self.game.state() != 3 {
            return;
        }
        if self.menus.current().is_some() {
            self.menus.render(out, &st.fonts);
        } else if let Some(p) = &self.poker {
            // Drawn by the menus' frame; a dialog or a film of its scripts
            // draws in its own loop.
            if !self.game.busy() {
                p.draw(out, &st.fonts);
                if p.cursor {
                    self.menus.draw_cursor(out);
                }
            }
        } else if st.inventory_open_now() || st.menu_open_now() {
            self.menus.draw_cursor(out);
        }
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
            held: false,
        };
        let mut seed = 1;
        let next: Vec<&[u8]> = (0..4).map(|_| r.choose(&mut seed)).collect();
        assert_eq!(next, [&b"Tick2"[..], b"Stand", b"Tick2", b"Stand"]);
        let r = Randomized {
            object: b"gorilla".to_vec(),
            tick1: 20.0,
            tick2: 0.0,
            held: false,
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

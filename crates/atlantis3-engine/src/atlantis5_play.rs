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

/// The mix's channel of a film's own Bink audio track (Bink's DirectSound
/// buffer beside FMOD's channels in the original): one *Atlantis V*
/// leaves free.
pub const CHANNEL_BINK: usize = crate::audio::CHANNEL_MUSIC;

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

/// The cursor types (+0x88 of the game's cursor, @ 0x0042bfd0): the
/// pictures `cursor_move`, `cursor_info`, `cursor_object`,
/// `cursor_character`, `cursor_aim` (0x0066cad0..0x0066cae0); 5 is none.
pub const CURSOR_FILES: [&str; 5] = [
    "cursor_move",
    "cursor_info",
    "cursor_object",
    "cursor_character",
    "cursor_aim",
];
/// The aim, the cursor with nothing under the screen centre (@ 0x0042f390).
pub const CURSOR_AIM: i32 = 4;
/// The followers' pictures (0x0066cae4..0x0066caf0) and the names given
/// to `Atl_Hero_HasFollower` (@ 0x0042c780: 0x0066cb84, 0x0066cb8c,
/// 0x0066cba0).
pub const FOLLOWERS: [&str; 3] = ["ishtar", "ishtar_mesopotamy", "sing_like_rain"];

/// The game's cursor in play (the game's +0x44; ctor @ 0x0042ba10, its
/// pictures loaded @ 0x0042bb10 on entering state 4 or 6 and freed
/// @ 0x0042bf30 on leaving it, @ 0x0042fb90) [CONFIRMED-STATIC]:
/// - +0x38..+0x48 the five cursors, `sprites2d/gui/<name>.spr` (path
///   16); +0x4c..+0x54 the three followers' pictures, same folder;
/// - +0x64 the hand's object, `sprites2d/objects_icon/<item>.spr`
///   (path 18), from `atl.hero.handled_item` at each update
///   (@ 0x0042c030);
/// - +0x68 its player (@ 0x00448c30: 15 frames a second, +0x08 the
///   double 15.0); a new type (@ 0x0042bfd0, only while loaded, only when
///   it differs) restarts it on the frames 0..count−1 in mode 3, a loop;
/// - +0x88 the type (5 at first: none), +0x8c loaded, +0x8d the
///   crosshair option (the options page writes it, @ 0x0041d0b0).
pub struct PlayCursor {
    cursors: [Option<Bank>; 5],
    followers: [Option<Bank>; 3],
    /// +0x64: the hand's object picture and its file name in its folder.
    hand: Option<(String, Bank)>,
    /// +0x88.
    kind: i32,
    /// The player's time (+0x00, seconds) and frame (+0x18).
    time: f64,
    frame: u32,
    /// +0x8c.
    loaded: bool,
    /// `Atl_Hero_HasFollower` for each follower, as last asked.
    shown_followers: [bool; 3],
    /// An item whose picture is missing (logged once).
    missing: Option<String>,
}

impl PlayCursor {
    fn new(src: &dyn FileSource) -> PlayCursor {
        let spr = |name: &str| {
            let path = format!("datas/sprites2d/gui/{name}.spr");
            let bank = src
                .read(&path)
                .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok());
            if bank.is_none() {
                log::warn!("{path}: missing, the cursor not drawn");
            }
            bank
        };
        PlayCursor {
            cursors: CURSOR_FILES.map(spr),
            followers: FOLLOWERS.map(spr),
            hand: None,
            kind: 5,
            time: 0.0,
            frame: 0,
            loaded: false,
            shown_followers: [false; 3],
            missing: None,
        }
    }

    /// The type (+0x88): 0..=4, or 5 (none).
    pub fn kind(&self) -> i32 {
        self.kind
    }

    /// The frame of the cursor's picture (+0x80, the player's +0x18).
    pub fn frame(&self) -> u32 {
        self.frame
    }

    /// The hand's object picture's file name (in its folder), if any.
    pub fn hand(&self) -> Option<&str> {
        self.hand.as_ref().map(|h| h.0.as_str())
    }

    /// @ 0x0042bfd0(kind): only while loaded and only a new type; a type
    /// below 5 restarts the player (@ 0x00448d00: time 0, the frame the
    /// first) on 0..count−1 (@ 0x00448cc0, the count @ 0x00463370) in
    /// mode 3 (@ 0x00448ce0) [CONFIRMED-STATIC].
    fn set_kind(&mut self, kind: i32) {
        if !self.loaded || self.kind == kind {
            return;
        }
        self.kind = kind;
        if kind < 5 {
            self.time = 0.0;
            self.frame = 0;
        }
    }

    /// The frame count of the current type's picture.
    fn count(&self) -> u32 {
        usize::try_from(self.kind)
            .ok()
            .and_then(|k| self.cursors.get(k)?.as_ref())
            .map_or(0, |b| u32::try_from(b.len()).unwrap_or(u32::MAX))
    }

    /// @ 0x0042c030(dt), while loaded: the hand's object first — with
    /// `hand` (`atl.hero.handled_item`, @ 0x00403ee0) empty its picture is
    /// freed; else, with none, `objects_icon/` + `hand` + `.spr` is
    /// loaded; with one whose name (the path less the folder's length,
    /// @ 0x00455dc0) is neither `hand` + `.spr` nor `proxy_object.spr`
    /// (0x0066bb60), it is freed and the new one loaded. A missing file
    /// throws in the original (@ 0x00463ac0: `C2DSpriteManager ::
    /// AddSprite`; its catch is not read): Native logs it and shows no
    /// object. Then, with a type below 5, the player's step
    /// (@ 0x00448d20, mode 3: frame = ⌊t · 15⌋ mod count).
    fn update(&mut self, dt: f64, hand: &[u8], src: &dyn FileSource) {
        if !self.loaded {
            return;
        }
        if hand.is_empty() {
            self.hand = None;
        } else {
            let want = format!("{}.spr", lossy(hand));
            let keep = self
                .hand
                .as_ref()
                .is_some_and(|(name, _)| *name == want || name == "proxy_object.spr");
            if !keep {
                self.hand = None;
                let path = format!("datas/sprites2d/objects_icon/{want}");
                match src
                    .read(&path)
                    .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok())
                {
                    Some(b) => self.hand = Some((want, b)),
                    None => {
                        if self.missing.as_deref() != Some(&want) {
                            log::warn!(
                                "{path}: missing (the original throws), no hand object drawn"
                            );
                            self.missing = Some(want);
                        }
                    }
                }
            }
        }
        if self.kind < 5 {
            self.time += dt;
            let last = self.count().wrapping_sub(1);
            self.frame = player_frame(Mode::Loop, 0, last, self.time).0;
        }
    }

    /// @ 0x0042c660, while loaded [CONFIRMED-STATIC]: with a type below
    /// 5, the centre of the display (its width and height halved,
    /// @ 0x0044d1b0 +0x0c / +0x10); the cursor's picture at its frame with
    /// its hot spot there (@ 0x00463d20, scale 1: `AFF_SPR`), except the
    /// aim (type 4) when both the crosshair option is off and the hand is
    /// empty; the hand's object, frame 0, at (x + 5, y + 30). Then, from
    /// x = 10, each follower `Atl_Hero_HasFollower` says is there: its
    /// picture, frame 0, at (x, 10), and x += 10 + its width (`GET_SPRT`
    /// @ 0x00463300).
    pub fn draw(&self, out: &mut [u32], crosshair: bool) {
        if !self.loaded {
            return;
        }
        let mut t = Target::screen(out, VIEW_WIDTH, VIEW_HEIGHT);
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let (cx, cy) = ((VIEW_WIDTH >> 1) as i32, (VIEW_HEIGHT >> 1) as i32);
        if self.kind < 5 {
            if self.kind != CURSOR_AIM || crosshair || self.hand.is_some() {
                if let Some(b) = usize::try_from(self.kind)
                    .ok()
                    .and_then(|k| self.cursors[k].as_ref())
                {
                    b.draw(&mut t, Some(cx), cy, self.frame as usize);
                }
            }
            if let Some((_, b)) = &self.hand {
                b.draw(&mut t, Some(cx + 5), cy + 30, 0);
            }
        }
        let mut x = 10;
        for (shown, b) in self.shown_followers.iter().zip(&self.followers) {
            if !*shown {
                continue;
            }
            if let Some(b) = b {
                b.draw(&mut t, Some(x), 10, 0);
                x += 10 + b.info(0).map_or(0, |i| i32::from(i.w));
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

/// A source of a music stream's frames (interleaved, as `ov_read`).
trait MusicDecoder {
    /// Up to `max_frames` frames appended to `out`; 0 at the end.
    fn read_frames(&mut self, out: &mut Vec<i16>, max_frames: usize) -> usize;
}

impl MusicDecoder for VorbisStream {
    fn read_frames(&mut self, out: &mut Vec<i16>, max_frames: usize) -> usize {
        self.read(out, max_frames)
    }
}

/// A music stream fed until `until` frames are pushed: at the file's end
/// it starts again while plays are left (`plays_left` < 0: for ever),
/// with a new decoder from `reopen`. A pass that gives no frame at all
/// (a file with its headers and no sound) ends the stream instead of
/// starting it again for ever.
fn feed_music<D: MusicDecoder>(
    dec: &mut D,
    pushed: &mut u64,
    done: &mut bool,
    plays_left: &mut i32,
    until: u64,
    reopen: impl Fn() -> Option<D>,
    mut push: impl FnMut(&[i16]),
) {
    let mut buf = Vec::new();
    let mut empty_pass = false;
    while !*done && *pushed < until {
        buf.clear();
        let n = dec.read_frames(&mut buf, 4096);
        if n == 0 {
            if !empty_pass && (*plays_left > 1 || *plays_left < 0) {
                if *plays_left > 1 {
                    *plays_left -= 1;
                }
                match reopen() {
                    Some(d) => *dec = d,
                    None => *done = true,
                }
                empty_pass = true;
                continue;
            }
            *done = true;
            break;
        }
        empty_pass = false;
        push(&buf);
        *pushed += n as u64;
    }
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
    /// The lip-sync analyser of a 3D talker's tirade (+0x60, made at the
    /// tirade's start, deleted at its end, 0x0043373d).
    lips: Option<dlg::LipSync>,
    /// The `talk` and `listen` pictures read during the dialog (Native
    /// reads a file once, the original's insert reads it each time).
    lip_banks: Vec<(String, Bank)>,
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
    /// +0x44: the slot last under the mouse (−1: none), its item's
    /// description in the bubble (`info_bulle` and its shadow, +0x34 and
    /// +0x38).
    hover: Option<usize>,
    info: String,
    /// The hand's item's description (`handled_object_name` and its
    /// shadow, +0x3c and +0x40).
    hand_name: String,
}

/// The inventory's texts' font (@ 0x004144a0: every one of its four
/// `CIE_Text`s).
const INVENTORY_FONT: &str = "AtlFont_Inventory";

/// The inventory's texts' width (@ 0x004144a0: 300 for each).
const INVENTORY_TEXT_WIDTH: i32 = 300;

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
    /// The `zoom_and_fade` transition's fade amount k (the transition's
    /// +0x0c, 0..1), drawn over the view (@ 0x0042e640); 0 outside it.
    pub fade: f32,
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
    /// The camera node's own rotation as a re-parenting left it (`BoatFx`
    /// @ 0x0042ccc0, `NoFx` @ 0x0042cd50 keep its world rotation), until
    /// the next step that rebuilds it from the angles (the mode-0 step
    /// @ 0x0042cf80 or a turn's step, through the setters @ 0x004444c0 →
    /// 0x0048a5a0 / 0x0048aa90). `None`: the angles' rotation.
    cam_override: Option<Quat>,
    /// Persistent sprites' records (@ 0x00442290: the object and its
    /// seconds played; no priority, they come back at 0).
    persistent: Vec<(Vec<u8>, f64)>,
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
    /// The film's own Bink audio track plays on [`CHANNEL_BINK`].
    film_bink_sound: bool,
    /// The film's step reported its end (@ 0x00456cb0: `FrameNum` =
    /// `Frames` − 1): the loop draws that frame once more, then ends at
    /// the next update.
    film_ending: bool,
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
    saves: Option<Rc<dyn crate::atlantis5_saves::SaveFiles>>,
    /// `CConfiguration` (the option pages, the save's `CONFIGURATION`).
    pub config: Config,
    /// The sound's music level b (0x0066efc4, @ 0x0045a640): set from the
    /// configuration's music (the sound page @ 0x00421370, the
    /// configuration applied @ 0x004299e0), faded by the holders below.
    music_level: f32,
    /// The configuration's music last given to the level.
    music_applied: f32,
    /// The music level's fade (flag 0x0068e746, @ 0x0045a5e0): start
    /// (0x0068e750), target (0x0068e754), elapsed (0x0068e748), time
    /// (0x0068e74c).
    music_fade: Option<LevelFade>,
    /// The holders of the half music level (0x0068e710, @ 0x0045caf0 /
    /// 0x0045cb20).
    duck_count: u32,
    /// The film playing is `Atl_Video_Play`'s 5-argument form (it holds
    /// the level).
    film_ducks: bool,
    /// The film playing is `Atl_Video_Play`'s 1-argument form: the music
    /// channels 0..4 muted (`FSOUND_SetMute(1..5, 1)` @ 0x0045b470 before
    /// its loop, unmuted @ 0x0045b490 after).
    music_muted: bool,
    /// The 5-argument form's picture under the film (`back`, the view
    /// blended with `oldpaper.tga`) and the film's rectangle (left, top,
    /// right, bottom).
    film_paper: Option<(Vec<u32>, [i32; 4])>,
    /// The rectangle (left, top, right, bottom; right and bottom
    /// excluded, a `RECT`) a film without paper is stretched into: the
    /// display's (0, 0, 1024, 576) for `Atl_Video_Play`'s 1-argument form
    /// (0x0040d24d..0x0040d2f3), one less each way for the start's logos
    /// (@ 0x0043d6a0: width − 1, height − 1).
    film_dest: [i32; 4],
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
            fade: 0.0,
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
            cam_override: None,
            persistent: Vec::new(),
            music: Default::default(),
            music_tick: 0.0,
            crt_rand: 1,
            randomized: Vec::new(),
            sound: None,
            film: None,
            film_bink_sound: false,
            film_ending: false,
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
            saves: None,
            config: Config::default(),
            music_level: Config::default().music,
            music_applied: Config::default().music,
            music_fade: None,
            duck_count: 0,
            film_ducks: false,
            music_muted: false,
            film_paper: None,
            film_dest: FULL_RECT,
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
    /// (+0x125), else `fmod(angle, 360)`. Both setters then rebuild the
    /// camera node's rotation from the angles (0x00444806..0x004448b2),
    /// which ends a rotation kept by `Atl_Camera_ActivateFx`'s
    /// re-parenting (play.md § 2).
    fn set_pitch(&mut self, p: f32) {
        self.cam_override = None;
        self.pitch = match self.pitch_limit {
            Some((lo, hi)) => p.min(hi).max(lo),
            None => libm::fmodf(p, 360.0),
        };
    }

    /// The yaw setter @ 0x004448c0 (limits +0x114/+0x118, flag +0x124).
    fn set_yaw(&mut self, y: f32) {
        self.cam_override = None;
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
        if let Some(q) = self.cam_override {
            c.q = q;
        }
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

    /// The film playing, by its name.
    pub fn film_name(&self) -> Option<String> {
        self.film.as_ref().map(|(f, _)| f.name().to_string())
    }

    /// Whether a film plays.
    /// The film's `FrameNum` (the frame its next step decodes, 1-based)
    /// and `Frames`, for tests.
    pub fn film_frame(&self) -> Option<(u32, u32)> {
        self.film.as_ref().map(|(f, _)| (f.frame_num, f.frames()))
    }

    pub fn film_playing(&self) -> bool {
        self.film.is_some()
    }

    /// Whether the music channels are muted (a 1-argument film).
    pub fn music_muted(&self) -> bool {
        self.music_muted
    }

    /// The 5-argument film's rectangle (left, top, right, bottom).
    pub fn film_rect(&self) -> Option<[i32; 4]> {
        self.film_paper.as_ref().map(|p| p.1)
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
        // the file exists, inserted with priority 0 and the persistent
        // flag (0x00441830(file, 0, 1)), with its time played, while it is
        // not over. A record whose file the view has not is kept
        // (GetFileAttributesA = −1 goes to the next record, LAB_004426cf),
        // its time still growing, for a later view that has it.
        if scene_changed {
            self.persistent.clear();
        }
        self.sprites.clear();
        self.randomized.clear();
        let persistent = self.persistent.clone();
        for (object, time) in persistent {
            if let Some(s) = self.add_sprite(&object, b"z1", Mode::Stop, 0, true) {
                s.time = time;
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
        self.insert_sprite(object, animation, bank, mode, priority, persistent)
    }

    /// A sprite of `bank` put in the list before the first of a priority
    /// not above its own (frames 0..count − 1, time 0).
    fn insert_sprite(
        &mut self,
        object: &[u8],
        animation: &[u8],
        bank: Bank,
        mode: Mode,
        priority: i32,
        persistent: bool,
    ) -> Option<&mut Sprite> {
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
    /// le fichier %s, changement de sprite annulé` (0x0066e468, @ 0x0044218d,
    /// before the old player is touched): false then (the sprite left as
    /// it is).
    fn randomize_next(&mut self, i: usize) -> bool {
        let Some(r) = self
            .randomized
            .iter()
            .find(|r| r.object == self.sprites[i].object)
        else {
            return true;
        };
        let next = r.choose(&mut self.crt_rand);
        let s = &mut self.sprites[i];
        if s.animation == next {
            let n = s.last.wrapping_sub(s.first);
            s.time -= f64::from(n) / SPRITE_FPS;
            return true;
        }
        let Some(v) = &self.view else { return true };
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
            return false;
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
                if self.music_muted {
                    0.0
                } else {
                    c.volume * self.config.master * self.music_level
                },
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

    /// The music level b now (tests).
    pub fn music_level(&self) -> f32 {
        self.music_level
    }

    /// The music level's fade to `target` (clamped to [0, 1]) over `time`
    /// seconds from the level now (@ 0x0045a5e0).
    fn music_level_fade(&mut self, target: f32, time: f32) {
        self.music_fade = Some(LevelFade {
            start: self.music_level,
            target: target.clamp(0.0, 1.0),
            elapsed: 0.0,
            time,
        });
    }

    /// A holder of the half music level (@ 0x0045caf0(0.5)): the first
    /// fades the level to level · f over 1 s.
    fn music_duck(&mut self, f: f32) {
        if self.duck_count == 0 {
            self.music_level_fade(self.music_level * f, 1.0);
        }
        self.duck_count += 1;
    }

    /// A holder's release (@ 0x0045cb20 with the configuration's music,
    /// @ 0x004295e0): the last fades the level back over 1 s. (The
    /// original's counter is not kept from going below 0; Native's is.)
    fn music_unduck(&mut self) {
        if self.duck_count == 0 {
            return;
        }
        self.duck_count -= 1;
        if self.duck_count == 0 {
            self.music_level_fade(self.config.music, 1.0);
        }
    }

    /// The music part of the update @ 0x00459fd0: the music level's fade
    /// (step 2: elapsed += dt, level = start + (target − start) ·
    /// min(elapsed / time, 1)), the channels' fades (step 3: one step per
    /// call, which is once per frame: the frame ends with
    /// `Flip(NULL, DDFLIP_WAIT)` @ 0x0047dd11, the screen's refresh; here
    /// one step per 1/60 s, `FRAME_HZ`), then the next track when one ended
    /// (step 4).
    fn music_update(&mut self, dt: f64) {
        // The configuration's music given to the level when it changes
        // (the sound page, a configuration applied: @ 0x0045a640).
        if self.config.music != self.music_applied {
            self.music_applied = self.config.music;
            self.music_level = self.config.music;
        }
        if let Some(f) = &mut self.music_fade {
            #[allow(clippy::cast_possible_truncation)]
            {
                f.elapsed += dt as f32;
            }
            let k = f.elapsed / f.time;
            if k < 1.0 {
                self.music_level = f.start + k * (f.target - f.start);
            } else {
                self.music_level = f.start + (f.target - f.start);
                self.music_fade = None;
            }
        }
        // Streams fed ahead of the output.
        let master_music = self.config.master * self.music_level;
        if let Some(audio) = self.audio.clone() {
            for (ch, c) in self.music.iter_mut().enumerate() {
                let Some(s) = &mut c.stream else { continue };
                let played = audio.played(CHANNEL_MUSIC_2004 + ch).unwrap_or(0);
                let data = &s.data;
                feed_music(
                    &mut s.dec,
                    &mut s.pushed,
                    &mut s.done,
                    &mut c.plays_left,
                    played + 44_100,
                    || VorbisStream::new(data.clone()).ok(),
                    |buf| audio.push(CHANNEL_MUSIC_2004 + ch, buf),
                );
                // @ 0x0045a640: each channel at master × its volume ×
                // the music level.
                let g = if self.music_muted {
                    0.0
                } else {
                    c.volume * master_music
                };
                audio.set_gain(CHANNEL_MUSIC_2004 + ch, [g; 2]);
            }
        }
        // A 60 Hz refresh [INFERRED: DDFLIP_WAIT waits for the vertical
        // blank, DirectDraw's documentation; the screen's rate is the
        // player's].
        const FRAME_HZ: f64 = 60.0;
        self.music_tick += dt;
        while self.music_tick >= 1.0 / FRAME_HZ {
            self.music_tick -= 1.0 / FRAME_HZ;
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
        let mut film = match Film::open(&lossy(name), Arc::from(data), Depth::Xrgb8888, self.clock)
        {
            Ok(f) => f,
            Err(e) => {
                log::warn!("{file}: {e:?}");
                return false;
            }
        };
        // The film player's Bink handle (@ 0x00456b50, made by the player
        // @ 0x0042a950 with FMOD's output handle) routes Bink's sound to
        // FMOD's DirectSound object (`BinkSetSoundSystem(BinkOpenDirectSound,
        // FSOUND_GetOutputHandle())`, 0x00456b72; @ 0x0043d6a0 for the
        // logos) and opens the file with `BinkOpen(name, 0)` (0x00456bbe):
        // RAD's default, the first audio track played [INFERRED: RAD's
        // API, binkw32 not read]. The program imports no `BinkSetVolume`,
        // so the track plays at Bink's own volume, whatever the options'
        // volumes. Only the two logos have a track.
        self.film_bink_sound = false;
        if let Some(a) = &self.audio {
            if let Some((rate, channels)) = film.enable_audio(0) {
                a.start_stream(CHANNEL_BINK, rate, channels, 1.0);
                self.film_bink_sound = true;
            }
        }
        self.film_dest = FULL_RECT;
        let voice = format!("datas/sounds/video/{}/{}.ogg", self.language, lossy(name));
        let sound = match (&self.audio, self.read(&voice)) {
            // @ 0x00459630(name, 1.0): FMOD channel 7 at `ftol`(voice ×
            // 1.0 × master × 1.0 × 255.0) (0x00459708..0x00459729), set
            // once at the start: FMOD's 0..255 volume.
            (Some(a), Some(d)) => {
                #[allow(clippy::cast_possible_truncation)]
                let v =
                    (f64::from(self.config.voice) * f64::from(self.config.master) * 255.0) as i32;
                #[allow(clippy::cast_precision_loss)]
                FilmSound::open(a, CHANNEL_CINEMA, d, v as f32 / 255.0)
            }
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
        self.film_ending = false;
        // The loop's first iteration steps the film before its first draw
        // (@ 0x0040c990, @ 0x0043d6a0: the step @ 0x0042b0f0, then the draw
        // @ 0x0042b220): the first picture is the film's first frame.
        self.film_step();
        true
    }

    /// One update of the film: its step, unless the last one reported
    /// the end; the loop is then left at the next update, after the frame
    /// decoded last was drawn once (`evidence/m5/video-play-rect.c`: step,
    /// sound, draw, flip, then the loop's test).
    fn film_update(&mut self) {
        if !self.film_ending {
            self.film_step();
        }
    }

    /// The loop's step (@ 0x0042b0f0): the film's own (@ 0x00456cb0) and
    /// its subtitles; the end it reports is taken at the next update.
    fn film_step(&mut self) {
        let now = self.clock;
        let Some((film, sound)) = &mut self.film else {
            return;
        };
        if let (Some(s), Some(a)) = (sound.as_mut(), &self.audio) {
            s.pump(a);
        }
        // The step @ 0x00456cb0, once per update: `BinkWait`, and only
        // when it returns 0 `BinkDoFrame` then `BinkNextFrame`.
        if film.due(now) {
            film.do_frame();
            if self.film_bink_sound {
                if let Some(a) = &self.audio {
                    a.push(CHANNEL_BINK, &film.take_audio(0));
                }
            }
            film.next_frame();
        }
        // Over when `FrameNum` = `Frames` − 1 (the last two frames are
        // never shown); a film of at most one frame at once.
        let over = film.frames() <= 1 || film.frame_num + 1 >= film.frames();
        // @ 0x0042b0f0: after the film's step, its subtitles.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (frame, rate) = (film.frame_num, film.fps().round() as u32);
        if let Some(fs) = &mut self.film_subtitles {
            fs.step(frame, rate);
        }
        if over {
            self.film_ending = true;
        }
    }

    fn end_film(&mut self) {
        self.film_ending = false;
        self.film_subtitles = None;
        if std::mem::take(&mut self.film_ducks) {
            self.music_unduck();
        }
        self.music_muted = false;
        self.film_paper = None;
        // `BinkClose` (@ 0x00456c77, from the player's close) ends its
        // sound.
        if std::mem::take(&mut self.film_bink_sound) {
            if let Some(a) = &self.audio {
                a.stop(CHANNEL_BINK);
            }
        }
        if let Some((_, sound)) = self.film.take() {
            if let (Some(s), Some(a)) = (sound, &self.audio) {
                s.stop(a);
            }
            self.wait_ended = true;
        }
    }

    // -- frame ---------------------------------------------------------------

    /// `CAMERA_MOVER` (`CCameraMover :: Serialize` @ 0x00437f40, on a copy
    /// of the camera controller): `moving_type` (`VT_I4`, the mode),
    /// `moving_time` (a float, @ 0x00465390: the turn's time), `fx_type`
    /// (`VT_I4`: 0 `BoatFx`, 1 `NoFx`), `fx_time` (the boat's clock); a
    /// child `MOVING_DATAS` only in mode 0 (`camera_speed_x` the pitch
    /// speed +0x08, `camera_speed_y` the yaw speed +0x0c, `decel_time`
    /// the damping clock +0x10) or mode 2 (`sx`, `sy`, `sz` the turn's
    /// start direction, `dx`, `dy`, `dz` its target), none in mode 1; a
    /// child `FX_DATAS` only with `fx_type` 0: `qx`, `qy`, `qz`, `qw`, the
    /// boat node's rotation relative to its parent (@ 0x0048a850).
    fn camera_mover_section(&self) -> atlantis4_data::xml::Element {
        use atlantis5_script::save::{attr_f32, element};
        let mut e = element("CAMERA_MOVER");
        e.attributes
            .push(("moving_type".into(), self.mode.to_string()));
        attr_f32(
            &mut e,
            "moving_time",
            self.turn.as_ref().map_or(0.0, |t| t.t),
        );
        e.attributes.push((
            "fx_type".into(),
            if self.boat.is_some() { "0" } else { "1" }.into(),
        ));
        attr_f32(&mut e, "fx_time", self.boat.unwrap_or(0.0));
        match self.mode {
            0 => {
                let mut m = element("MOVING_DATAS");
                attr_f32(&mut m, "camera_speed_x", self.speed[1]);
                attr_f32(&mut m, "camera_speed_y", self.speed[0]);
                attr_f32(&mut m, "decel_time", self.damping);
                e.children.push(m);
            }
            2 => {
                let mut m = element("MOVING_DATAS");
                let (from, to) = self
                    .turn
                    .as_ref()
                    .map_or(([0.0; 3], [0.0; 3]), |t| (t.from, t.to));
                for (n, v) in ["sx", "sy", "sz"].iter().zip(from) {
                    attr_f32(&mut m, n, v);
                }
                for (n, v) in ["dx", "dy", "dz"].iter().zip(to) {
                    attr_f32(&mut m, n, v);
                }
                e.children.push(m);
            }
            _ => {}
        }
        if self.boat.is_some() {
            let mut f = element("FX_DATAS");
            for (n, v) in ["qx", "qy", "qz", "qw"].iter().zip(self.boat_q) {
                attr_f32(&mut f, n, v);
            }
            e.children.push(f);
        }
        e
    }

    /// `CSerializableCameraMover :: Deserialize` (@ 0x004383b0) and the
    /// copy back into the controller (@ 0x0042d2b0, 0x0043b390): the mode,
    /// the time, the speeds and the damping clock (mode 0) or the turn's
    /// two vectors (mode 2), the boat's flag and clock, its node's
    /// rotation when `fx_type` is 0 (the quaternion (0, 0, 0, 1) before
    /// its four reads). The camera node is not re-parented (only `BoatFx`
    /// does that): `on_boat` stays as it was. A missing element or
    /// attribute throws in the original; Native keeps what it has.
    fn read_camera_mover(&mut self, e: &atlantis4_data::xml::Element) {
        use atlantis5_script::save::get_f64;
        #[allow(clippy::cast_possible_truncation)]
        let f = |e: &atlantis4_data::xml::Element, n: &str| get_f64(e, n).unwrap_or(0.0) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mode = get_f64(e, "moving_type").unwrap_or(0.0) as u32;
        let time = f(e, "moving_time");
        self.mode = mode;
        self.turn = None;
        match (mode, e.child("MOVING_DATAS")) {
            (0, Some(m)) => {
                self.speed = [f(m, "camera_speed_y"), f(m, "camera_speed_x")];
                self.damping = f(m, "decel_time");
            }
            (2, Some(m)) => {
                self.turn = Some(Turn {
                    t: time,
                    from: [f(m, "sx"), f(m, "sy"), f(m, "sz")],
                    to: [f(m, "dx"), f(m, "dy"), f(m, "dz")],
                });
            }
            (0 | 2, None) => log::warn!("CAMERA_MOVER: no MOVING_DATAS"),
            _ => {}
        }
        #[allow(clippy::cast_possible_truncation)]
        let fx = get_f64(e, "fx_type").unwrap_or(1.0) as i32;
        if fx == 0 {
            self.boat = Some(f(e, "fx_time"));
            match e.child("FX_DATAS") {
                Some(q) => {
                    #[allow(clippy::cast_possible_truncation)]
                    let qw = get_f64(q, "qw").unwrap_or(1.0) as f32;
                    self.boat_q = [f(q, "qx"), f(q, "qy"), f(q, "qz"), qw];
                }
                None => log::warn!("CAMERA_MOVER: no FX_DATAS"),
            }
        } else {
            self.boat = None;
        }
    }

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
                // The setters rebuild the node's rotation every step.
                self.cam_override = None;
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
                self.cam_override = None;
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

    /// One frame: camera, sprites (@ 0x00441c30) when `scene`, sound and
    /// music (@ 0x00459fd0), film.
    fn update(&mut self, dt: f64, play: bool, scene: bool) {
        self.update_start(dt, play);
        self.update_scene(dt, scene);
    }

    /// The frame's first part: the film's end, the camera, an animated
    /// look's and a dialog's steps, the film's step.
    fn update_start(&mut self, dt: f64, play: bool) {
        self.clock += dt;
        // A film whose last step reported its end was drawn once: its loop
        // is left, and the script goes on in this frame.
        if self.film_ending {
            self.end_film();
        }
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
        self.film_update();
    }

    /// The scene's update (@ 0x0044c380: the sprites' step) and the sound
    /// (@ 0x00459fd0), after the frame's scripts: state 4 (@ 0x0042f000
    /// case 4) runs the events and the timers, then the pick and the
    /// cursor, and only then the scene's update, so a sprite a click or a
    /// timer made is stepped once before it is first drawn.
    fn update_scene(&mut self, dt: f64, scene: bool) {
        // A randomized sprite's missing file throws out of the sprites'
        // step; `CGameManager::Process`'s catch (@ 0x0042f3c6) logs it
        // (`CGameManager :: Process -> %s`, 0x0066cc4c) and goes on after
        // its try (@ 0x0042f464): that frame the rest of the sprite list
        // and the sound update (@ 0x00459fd0) are skipped.
        //
        // `Atl_Video_Play`'s two loops (@ 0x0040c990: events, the film's
        // step @ 0x0042b0f0, the sound @ 0x00459fd0, the draw
        // @ 0x0042b220, the flip) never call the scene's update
        // @ 0x0044c380: the sprites, the persistent records' times and the
        // randomize draws are frozen for the film's length.
        let thrown = scene && self.film.is_none() && !self.sprites_step(dt);
        if !thrown {
            self.music_update(dt);
        }
    }

    /// The sprites' step (@ 0x00441c30, from the scene's update
    /// @ 0x0044c380). False when a randomized sprite's file is missing:
    /// the throw ends the step there (the sprites after it not stepped).
    fn sprites_step(&mut self, dt: f64) -> bool {
        for p in &mut self.persistent {
            p.1 += dt;
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
                if !self.randomize_next(i) {
                    return false;
                }
                i += 1;
            } else if self.facing_awaited(i) {
                // Kept on its last frame for the next frame's check.
                i += 1;
            } else {
                let s = self.sprites.remove(i);
                if s.persistent {
                    self.persistent.retain(|p| p.0 != s.object);
                }
            }
        }
        true
    }

    /// The picture: the film while one plays, else the view with its
    /// sprites (drawn from the list's head to its tail, @ 0x0044bd30's
    /// second pass: @ 0x00456a90 then @ 0x00456ad0) by the cube renderer,
    /// the subject menu, the inventory and the dialog.
    pub fn render(&self, out: &mut [u32]) {
        self.render_with(out, true);
    }

    /// [`Stage::render`]; without `menus`, no subject menu nor inventory:
    /// a script's handler loop (a dialog, @ 0x00407ac0's loop drawing only
    /// @ 0x0044bd30 and @ 0x00433910; a film, a transition) draws its own
    /// frame, not the menus' (@ 0x0041baa0 is state 3's display only).
    pub fn render_with(&self, out: &mut [u32], menus: bool) {
        out.fill(0);
        if let Some((film, _)) = &self.film {
            let (w, h) = film.size();
            // The film's draw (@ 0x00456da0): its frame `Blt` from a
            // surface of its own size into the player's rectangle (+0x08):
            // stretched (1:1 for the scripts' films, which have their
            // rectangle's size; the logos, 720 × 405 and 800 × 450, fill
            // the picture). Nearest here, the filter [UNKNOWN:
            // DirectDraw's].
            if let Some((back, rect)) = &self.film_paper {
                // The 5-argument form's frame: `back` blitted whole first.
                out.copy_from_slice(back);
                stretch_into(out, &film.xrgb, w, h, *rect);
            } else {
                stretch_into(out, &film.xrgb, w, h, self.film_dest);
            }
            self.draw_film_subtitle(out);
            return;
        }
        if !self.render_view(out) {
            return;
        }
        if self.fade > 0.0 {
            darken(out, self.fade);
        }
        if menus {
            self.draw_menu(out);
            self.draw_inventory(out);
        }
        self.draw_dialog(out);
    }

    /// The start's logos (@ 0x0043d6a0): the film's rectangle is
    /// (0, 0, width − 1, height − 1) of the display.
    fn logo_rect(&mut self) {
        self.film_dest = [0, 0, FULL_RECT[2] - 1, FULL_RECT[3] - 1];
    }

    /// The film's rectangle without paper (tests).
    pub fn film_dest(&self) -> [i32; 4] {
        self.film_dest
    }

    /// The 5-argument film's `back` (@ 0x0040c990, its first loop frame):
    /// the view drawn alone (@ 0x0044bd30: the cube and the sprites, no
    /// cursor, inventory or dialog), `sprites2d/fx/oldpaper.tga` (path
    /// entry 0xf + 0x0066b310; 1024 × 768) stretched to the display
    /// (`Blt` with no rectangles, 0x0040cd3d: nearest here, the filter
    /// [UNKNOWN: DirectDraw's or the driver's]) and blended into it per
    /// byte, back = (paper · 150 + back · 106) >> 8 (@ 0x0047a8a4). Made at
    /// the call: the sprites are not stepped before the film's first
    /// frame, so it is the same picture (the original shows the previous
    /// picture during that first frame). `None` when the paper is missing
    /// (logged; the original throws `Impossible d'ouvrir le fichier %s`).
    fn film_paper_back(&self) -> Option<Vec<u32>> {
        let file = "datas/sprites2d/fx/oldpaper.tga";
        let img = self
            .read(file)
            .and_then(|d| atlantis4_data::image::decode_tga(&d).ok());
        let Some(img) = img else {
            log::warn!("Atl_Video_Play: {file} cannot be opened: the film is not played");
            return None;
        };
        let mut back = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
        self.render_view(&mut back);
        let (pw, ph) = (img.width as usize, img.height as usize);
        if pw == 0 || ph == 0 {
            return Some(back);
        }
        for y in 0..VIEW_HEIGHT {
            let sy = y * ph / VIEW_HEIGHT;
            for x in 0..VIEW_WIDTH {
                let sx = x * pw / VIEW_WIDTH;
                let p = &img.pixels[(sy * pw + sx) * 4..][..4];
                let paper = (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]);
                let v = &mut back[y * VIEW_WIDTH + x];
                let mut o = 0u32;
                for sh in [0, 8, 16, 24] {
                    let (a, b) = ((paper >> sh) & 0xff, (*v >> sh) & 0xff);
                    o |= ((a * 150 + b * 106) >> 8) << sh;
                }
                *v = o;
            }
        }
        Some(back)
    }

    /// The view with its sprites (@ 0x0044bd30): false without a view.
    fn render_view(&self, out: &mut [u32]) -> bool {
        let Some(v) = &self.view else { return false };
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
        true
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

    /// Whether sprite `i` is a `facing` sprite that the dialog's facing
    /// loop or the subject menu's state 0 still waits on. The original
    /// checks the sprite's time against (last − first) / fps before the
    /// sprites' step of each frame, and the step removes it once the time
    /// passes (last − first + 1) / fps: at the screen's refresh (a frame
    /// every 1/60 s or so, play.md § 6) a check always lands in that
    /// 1/15 s window and swaps in `listen`. A frame longer than 1/15 s
    /// could step over the window and remove the talker for the whole
    /// dialog; such a sprite is kept, ended, on its last frame, so the
    /// next frame's check finds it done, as at the full frame rate.
    fn facing_awaited(&self, i: usize) -> bool {
        let s = &self.sprites[i];
        if s.animation != b"facing" {
            return false;
        }
        let dialog = self
            .dialog
            .as_ref()
            .is_some_and(|d| d.facing && d.faced.contains(&s.object));
        let menu = self
            .menu
            .as_ref()
            .is_some_and(|m| m.state == MenuState::Facing && m.character == s.object);
        dialog || menu
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
            log::info!("LUA_Dialog_Play: no dialog {id}");
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
            lips: None,
            lip_banks: Vec::new(),
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
        // The state and time the step starts from (the lip-sync reads the
        // time before it grows).
        let (state, time) = (run.player.state, run.player.time);
        let talker = run
            .tirades
            .get(run.player.tirade)
            .map(|t| t.0.as_bytes().to_vec());
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
                if state == 3 {
                    // @ 0x004336c0: `listen` (whatever the talker plays,
                    // its time kept), then the analyser deleted
                    // (0x004336e6..0x0043373d).
                    if let Some(t) = &talker {
                        self.lip_swap(t, b"listen");
                    }
                    if let Some(run) = &mut self.dialog {
                        run.lips = None;
                    }
                }
            }
            Some(Act::Over) => self.dialog_end(),
            None => {
                if state == 3 {
                    if let Some(t) = &talker {
                        self.lip_sync(t, time);
                    }
                }
            }
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
        // A 3D talker's analyser (@ 0x00432b3c..: when the sample is not
        // loaded yet, which it never is at a tirade's start: the previous
        // tirade's stop freed it, @ 0x00459a00); an unreadable voice
        // leaves its list empty.
        let mut lips = (kind == Kind::ThreeD).then(dlg::LipSync::default);
        if let Some(data) = voice {
            match atlantis4_data::audio::decode_vorbis(&data) {
                Ok(d) => {
                    if kind == Kind::ThreeD {
                        lips = Some(dlg::LipSync::analyse(&d.samples, d.channels, d.sample_rate));
                    }
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
        run.lips = lips;
        run.player.started(kind, has_voice);
        if kind == Kind::ThreeD {
            // @ 0x004346f0 at the time 0, before the bubble.
            self.lip_sync(talker.as_bytes(), 0.0);
        }
    }

    /// The 3D talker's mouth (@ 0x004346f0, the player's state 0 and each
    /// frame of state 3 before its time grows): with an analyser, the
    /// talker's sprite (the sprite list's first whose object is the
    /// talker; its animation, the file's fourth part, or nothing) gets
    /// `talk` (0x0066d2ac, @ 0x00433fd0) when the analyser says the voice
    /// is loud at the tirade's time and it is not `talk` already, else
    /// `listen` (0x00669f4c, @ 0x00434360) when it is not `listen`.
    fn lip_sync(&mut self, talker: &[u8], t: f32) {
        let Some(lips) = self.dialog.as_ref().and_then(|r| r.lips.as_ref()) else {
            return;
        };
        let talking = lips.talking(t);
        let anim = self
            .sprites
            .iter()
            .find(|s| s.object == talker)
            .map(|s| s.animation.clone())
            .unwrap_or_default();
        let want: &[u8] = if talking { b"talk" } else { b"listen" };
        if anim != want {
            self.lip_swap(talker, want);
        }
    }

    /// @ 0x00433fd0 / @ 0x00434360: the talker's sprite (when it has one)
    /// removed (@ 0x00441a00) and its file with `animation` inserted with
    /// its priority (@ 0x00441830(file, priority, 0)), in a loop (mode 3,
    /// @ 0x00448ce0) from the old sprite's time (@ 0x00448c60 kept,
    /// @ 0x00448cf0 set): the mouth moves on the same clock.
    fn lip_swap(&mut self, object: &[u8], animation: &[u8]) {
        let Some(old) = self.sprites.iter().find(|s| s.object == object) else {
            return;
        };
        let (priority, time) = (old.priority, old.time);
        let Some(v) = &self.view else { return };
        #[allow(clippy::cast_sign_loss)]
        let path = sprite_path(
            &lossy(&v.scene),
            v.view.max(0) as u32,
            &lossy(object),
            &lossy(animation),
        );
        let cached = self.dialog.as_ref().and_then(|r| {
            r.lip_banks
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, b)| b.clone())
        });
        let bank = cached.or_else(|| {
            let b = self
                .read(&path)
                .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok())?;
            if let Some(r) = &mut self.dialog {
                r.lip_banks.push((path.clone(), b.clone()));
            }
            Some(b)
        });
        self.sprites.retain(|s| s.object != object);
        let Some(bank) = bank else {
            log::warn!("{path}: not found");
            return;
        };
        if let Some(s) = self.insert_sprite(object, animation, bank, Mode::Loop, priority, false) {
            s.time = time;
        }
    }

    /// The end of `Atl_Dialog_Play`: the faced talkers' `unfacing` (the
    /// player's default mode, their records released, @ 0x00443570), and
    /// the script goes on.
    fn dialog_end(&mut self) {
        let Some(run) = self.dialog.take() else {
            return;
        };
        self.music_unduck();
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
    /// The camera's look speeds (yaw +0x0c, pitch +0x08; radians a
    /// second), for tests.
    pub fn look_speed(&self) -> [f32; 2] {
        self.speed
    }

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
        // @ 0x00433910: the zone (0, 0, +0x48 − 1, …), the text
        // partitioned at draw (@ 0x00461bf0) at 759 for the 760-wide box,
        // while its height was measured at 760 (state 0).
        let lines = self.fonts.lines_partitioned(
            "AtlFont_Dialog",
            &run.text,
            bw - 1,
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
        // The menu's start holds the half music level (@ 0x0041197e).
        self.music_duck(0.5);
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
    /// record released; the menu is over. Run by the menu's process, so
    /// once, at the first menu frame after `Atl_TerminateDialog` or the
    /// click elsewhere (a script that goes on after `Atl_TerminateDialog`
    /// holds the menus' frame until it ends, as its calls run inside the
    /// original's handler).
    pub(crate) fn menu_close_step(&mut self) {
        let Some(m) = &self.menu else { return };
        if m.state != MenuState::Closing || m.closed {
            return;
        }
        let c = m.character.clone();
        if self.sprites.iter().any(|s| s.object == c) {
            self.replace_anim(&c, b"unfacing", Mode::Stop);
        }
        self.hold_randomized(&c, false);
        if let Some(m) = &mut self.menu {
            if !m.closed {
                // Released by the menu's process (@ 0x004114c5).
                self.music_unduck();
            }
        }
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
            hover: None,
            info: String::new(),
            hand_name: String::new(),
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

    /// The inventory over the view (@ 0x00414b70): its background +4 with
    /// its children, the grid and buttons (a button's focus picture under
    /// the mouse [INFERRED]) and the icons in their cells. The cursor's
    /// children come after the cursor
    /// ([`Stage::draw_inventory_over_cursor`]).
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
    }

    /// The bubble over the slot under the mouse (@ 0x00416c50, each frame
    /// of the inventory's state 2 after the events): the grid's cell
    /// under the mouse (@ 0x00450520) + the page is the slot; when it
    /// changes and holds an item, both bubble texts become its
    /// description (`describe`); off the cells the texts are emptied
    /// once (+0x44 back to −1). A slot without an item leaves the texts
    /// as they were.
    pub fn inventory_hover(&mut self, describe: impl Fn(&[u8]) -> String) {
        let hit = self.inventory_hit();
        let Some(inv) = &mut self.inventory else {
            return;
        };
        match hit {
            Some(InventoryHit::Slot(slot)) => {
                if inv.hover != Some(slot) {
                    inv.hover = Some(slot);
                    if let Some(Some(item)) = inv.items.get(slot) {
                        if !item.is_empty() {
                            inv.info = describe(item);
                        }
                    }
                }
            }
            _ => {
                if inv.hover.is_some() {
                    inv.hover = None;
                    inv.info.clear();
                }
            }
        }
    }

    /// The hand's item's description (@ 0x00414db0, @ 0x00415fe0: the
    /// elements made; empty with an empty hand).
    pub fn inventory_set_hand_name(&mut self, text: String) {
        if let Some(inv) = &mut self.inventory {
            inv.hand_name = text;
        }
    }

    /// The bubble's and the hand name's texts (tests).
    pub fn inventory_texts(&self) -> Option<(String, String)> {
        self.inventory
            .as_ref()
            .map(|i| (i.info.clone(), i.hand_name.clone()))
    }

    /// The inventory's elements that are the cursor's children, drawn
    /// after the cursor (@ 0x00414b70: the cursor +8 at alpha 1 with its
    /// children, then the hand's icon +0x30 again at the menu's alpha
    /// +0x1c, which is always 1: Initialize @ 0x00414bb0 sets it and state
    /// 2, and no code sets the fading states 1 or 3). A parent draws
    /// itself, then its children from the last added (@ 0x004524e0; a
    /// child is put first, @ 0x00452140 with index 0): the hand's icon
    /// (made by the elements' builder, @ 0x00414db0, at (5, 0x1e)), the
    /// hand name's shadow (black, (0, 0x5f)) with the hand name (white,
    /// (−1, −1) from it), the bubble's shadow (white, (0x20, 0)) with the
    /// bubble (black, (−1, −1)); all relative to the cursor, which is at
    /// the mouse (@ 0x0040d920).
    pub fn draw_inventory_over_cursor(&self, out: &mut [u32]) {
        let Some(inv) = &self.inventory else { return };
        let (mx, my) = self.mouse;
        let icon = |out: &mut [u32]| {
            if let Some(h) = &inv.hand_icon {
                let mut t = Target::screen(out, VIEW_WIDTH, VIEW_HEIGHT);
                h.draw(&mut t, Some(mx + 5), my + 0x1e, 0);
            }
        };
        icon(out);
        for (text, colour, x, y) in [
            (&inv.hand_name, 0x0000_0000, mx, my + 0x5f),
            (&inv.hand_name, 0x00ff_ffff, mx - 1, my + 0x5f - 1),
            (&inv.info, 0x00ff_ffff, mx + 0x20, my),
            (&inv.info, 0x0000_0000, mx + 0x20 - 1, my - 1),
        ] {
            self.draw_inventory_text(out, text, colour, x, y);
        }
        icon(out);
    }

    /// A `CIE_Text` of the inventory (@ 0x00451600): `AtlFont_Inventory`,
    /// 300 wide, its height the text's at that width (@ 0x00461800), left
    /// aligned (+0x80 0), top, in `colour` (a `COLORREF`).
    fn draw_inventory_text(&self, out: &mut [u32], text: &str, colour: u32, x: i32, y: i32) {
        if text.is_empty() {
            return;
        }
        let w = INVENTORY_TEXT_WIDTH;
        let h = self.fonts.text_height(INVENTORY_FONT, text, w);
        let zone = crate::text::Zone::new(0, 0, 0, 0, menus::halign(0), 0);
        let lines = self.fonts.lines(INVENTORY_FONT, text, w, h.max(1), zone);
        self.fonts.draw(
            INVENTORY_FONT,
            &lines,
            menus::rgb(colour),
            out,
            VIEW_WIDTH,
            VIEW_HEIGHT,
            x,
            y,
        );
    }

    /// The film's subtitle (@ 0x0042b220): partitioned once at the
    /// screen's width W (@ 0x00461e80), its height h at W (@ 0x00461800);
    /// drawn black in (0, H − h, W, H), centred (`DrawTextW`, no
    /// partition), then white in (−2, H − h − 2, W, H): only the left and
    /// top move, so the white copy is centred in W + 2 from −2, 1 pixel
    /// left and 2 up of the black.
    fn draw_film_subtitle(&self, out: &mut [u32]) {
        let Some(text) = self.film_subtitles.as_ref().and_then(|f| f.shown.as_ref()) else {
            return;
        };
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let (w, h) = (VIEW_WIDTH as i32, VIEW_HEIGHT as i32);
        let font = "AtlFont_Subtitle";
        let lines = self.fonts.lines(font, text, w, h, dlg::SUBTITLE_ZONE);
        let top = h - self.fonts.text_height(font, text, w);
        self.fonts
            .draw(font, &lines, 0, out, VIEW_WIDTH, VIEW_HEIGHT, 0, top);
        let white: Vec<crate::text::Line> = lines
            .iter()
            .map(|l| {
                let cx: i32 = l.dx.iter().sum();
                crate::text::Line {
                    x: -2 + (w + 2 - cx) / 2,
                    ..l.clone()
                }
            })
            .collect();
        self.fonts.draw(
            font,
            &white,
            0x00ff_ffff,
            out,
            VIEW_WIDTH,
            VIEW_HEIGHT,
            0,
            top - 2,
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
        self.film_ending = false;
        self.film_subtitles = None;
        self.dialog = None;
        self.menu = None;
        self.inventory = None;
        self.mode = 0;
        self.turn = None;
        self.look_turn = None;
        self.boat = None;
        self.on_boat = false;
        self.cam_override = None;
        self.fade = 0.0;
        // The holders gone with the scene: the level back to the
        // configuration's at once.
        self.duck_count = 0;
        self.film_ducks = false;
        self.music_muted = false;
        self.film_paper = None;
        self.music_fade = None;
        self.music_level = self.config.music;
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

    /// The player's saves (`saves/`).
    pub fn set_saves(&mut self, saves: Option<Rc<dyn crate::atlantis5_saves::SaveFiles>>) {
        self.saves = saves;
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
                // The 5-argument form (@ 0x0040c990's second branch): the
                // paper made first (`oldpaper.tga` missing throws out of
                // the handler, so the film does not play; Native logs it
                // and goes on with the script), the rectangle RECT (left,
                // top, right, bottom) = arguments (3, 2, 5, 4) after
                // `ftol` (@ 0x00456b10).
                let paper = match rect {
                    Some(r) => match self.film_paper_back() {
                        Some(back) => Some((back, [r[1], r[0], r[3], r[2]])),
                        None => return Reply::Done,
                    },
                    None => None,
                };
                if self.video_play(name) {
                    if paper.is_some() {
                        // It holds the half music level (@ 0x0040ce21)
                        // until the film's end (@ 0x0040d0bd).
                        self.film_paper = paper;
                        self.film_ducks = true;
                        self.music_duck(0.5);
                    } else {
                        // The 1-argument form mutes the music channels
                        // 0..4 for the film (@ 0x0045b470 / 0x0045b490).
                        // (The start's logos, another loop, do neither.)
                        self.music_muted = true;
                    }
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
                // @ 0x0040ba20: once only; the record made first
                // (@ 0x00442290(name, 0), whether the view has the file or
                // not), then the `z1` of the view inserted with the call's
                // priority (@ 0x00441830) and the player's default mode
                // (1, stop, @ 0x00448c30).
                if self.persistent.iter().any(|p| &p.0 == object) {
                    log::info!("persistent sprite {} already", lossy(object));
                } else {
                    self.persistent.push((object.clone(), 0.0));
                    let _ = self.add_sprite(object, b"z1", Mode::Stop, *priority, true);
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
                    // clock +0x24 = 0. The re-parenting comes first: the
                    // camera's own rotation becomes N⁻¹ ⊗ W (N the node's
                    // rotation at the call, W the camera's world one), and
                    // then N = L, so until the setters rebuild the camera
                    // from the angles (the rest of a transition's frames)
                    // the view is L ⊗ N⁻¹ ⊗ W (play.md § 2). The inverse
                    // of the unit quaternion N: its conjugate.
                    let world = self.camera().q;
                    let n = self.boat_q;
                    let n_inv = [-n[0], -n[1], -n[2], n[3]];
                    self.cam_override = Some(quat_mul(&n_inv, &world));
                    self.boat = Some(0.0);
                    self.boat_q = look_rotation(v, &[0.0, -1.0, 0.0]);
                    self.on_boat = true;
                }
                CameraFx::NoFx => {
                    // @ 0x0042cd50: +0x20 = 1, +0x24 = 0, the camera node
                    // back under the root (@ 0x0048ac60 with 1), keeping
                    // its world rotation until the next rebuild; the
                    // controller's node N keeps its last rotation.
                    self.cam_override = Some(self.camera().q);
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
                    // `Atl_Dialog_Play` holds the half music level
                    // (@ 0x00407c51) until its end (@ 0x00408586).
                    self.music_duck(0.5);
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
            Request::TransitionFov { radians, fade } => {
                self.fov = *radians;
                self.fade = *fade;
            }
            Request::TransitionEnd => self.fade = 0.0,
            Request::DeleteAutosave => {
                // @ 0x0042fc50: `saves/autoslot` deleted.
                if let Some(d) = &self.saves {
                    d.remove(atlantis5_script::save::AUTOSLOT);
                }
            }
            Request::SubjectMenu { character } => self.menu_open(character),
            Request::GameEnded => {
                // @ 0x0042ffd0's engine part, then the credits (mode 1).
                self.end_game();
                self.config.reset_for_new_session();
                self.menu_requests.push((menus::CREDITS, Vec::new()));
            }
            Request::GameReset => {
                // @ 0x0042ffd0's engine part before state 2's read.
                self.end_game();
                self.config.reset_for_new_session();
            }
        }
    }

    /// `CAMERA` (@ 0x00437920: `pitch`, `pitch_min`, `pitch_max`,
    /// `pitch_limited`, `yaw`, `yaw_min`, `yaw_max`, `yaw_limited`),
    /// `SOUND` (@ 0x0043a4f0: a `PLAYLIST` with `channel` and `name` for
    /// each of the 5 channels playing a list) and `PERSISTANT_SPRITES`
    /// (@ 0x00439f10: a `PS` with `name` and `time` each); `CAMERA_MOVER`
    /// after `CONFIGURATION` ([`Stage::camera_mover_section`]).
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
        for (object, time) in &self.persistent {
            let mut e = element("PS");
            e.attributes
                .push(("name".into(), atlantis4_data::xml::cp1252(object)));
            #[allow(clippy::cast_possible_truncation)]
            attr_f32(&mut e, "time", *time as f32);
            ps.children.push(e);
        }
        vec![conf, self.camera_mover_section(), cam, sound, ps]
    }

    fn load_sections(&mut self, root: &atlantis4_data::xml::Element) {
        use atlantis5_script::save::{get_bool, get_f64};
        if let Some(e) = root.child("CAMERA_MOVER") {
            self.read_camera_mover(e);
        }
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
                    Some((name, get_f64(e, "time").unwrap_or(0.0)))
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
        self.saves.as_ref().is_some_and(|d| d.write(name, data))
    }

    fn read_save(&mut self, name: &str) -> Option<Vec<u8>> {
        self.saves.as_ref()?.read(name)
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

/// The `zoom_and_fade` transition's fade (@ 0x0042e640, fx 2): a surface
/// of the display's size filled with 0x7f000000 (@ 0x0048de80) blended
/// over the view with α = `__ftol`(k · 255.0) (0x00655750) by @ 0x0047a8a4
/// (32 bits per pixel; MMX, c′ = (c · 256 + (s − c) · α) >> 8 with s = 0
/// for the colour bytes): c′ = (c · (256 − α)) >> 8 on R, G and B.
/// The display's rectangle (left, top, right, bottom).
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
const FULL_RECT: [i32; 4] = [0, 0, VIEW_WIDTH as i32, VIEW_HEIGHT as i32];

/// A `w` × `h` picture stretched into `rect` (left, top, right, bottom;
/// right and bottom excluded) of the 1024 × 576 picture, nearest pixel,
/// clipped to the picture.
fn stretch_into(out: &mut [u32], src: &[u32], w: usize, h: usize, [l, t, r, b]: [i32; 4]) {
    let (rw, rh) = (r - l, b - t);
    if rw <= 0 || rh <= 0 || w == 0 || h == 0 {
        return;
    }
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    for y in t.max(0)..b.min(VIEW_HEIGHT as i32) {
        let sy = ((y - t) as usize * h) / rh as usize;
        for x in l.max(0)..r.min(VIEW_WIDTH as i32) {
            let sx = ((x - l) as usize * w) / rw as usize;
            out[y as usize * VIEW_WIDTH + x as usize] = src[sy * w + sx];
        }
    }
}

fn darken(out: &mut [u32], k: f32) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let a = (f64::from(k) * 255.0) as u32;
    let keep = 256 - a.min(256);
    for p in out.iter_mut() {
        let c = |sh: u32| ((((*p >> sh) & 0xff) * keep) >> 8) << sh;
        *p = (*p & 0xff00_0000) | c(16) | c(8) | c(0);
    }
}

/// The music level's fade (@ 0x0045a5e0, stepped @ 0x00459fd0).
#[derive(Clone, Copy, Debug)]
struct LevelFade {
    start: f32,
    target: f32,
    elapsed: f32,
    time: f32,
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
    /// The mouse's move (DirectInput lX, lY) since the last frame: the
    /// sum of the system's moves, as the original reads the device's
    /// state once per frame.
    moves: (f32, f32),
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
    /// The logos still to play before the main menu ([`Play::boot`]).
    logos: Vec<&'static str>,
    /// Whether the start's logos play (the application's state 0).
    in_logos: bool,
    /// The game's cursor in play.
    pub cursor: PlayCursor,
}

/// The logos the program plays at its start, in order (@ 0x0043d320
/// state 0: `Logo_Nobilis` 0x0066e15c, then `Logo_AIE` 0x0066e150).
pub const LOGOS: [&str; 2] = ["Logo_Nobilis", "Logo_AIE"];

/// The poker board's side of the game: the scripts' one-shot strings
/// (@ 0x00454220), the program's `rand`, the texts.
struct PokerHost<'a> {
    game: &'a mut Game,
    stage: &'a Rc<RefCell<Stage>>,
    menus: &'a Menus,
}

impl PokerHost<'_> {
    /// A one-shot string's error (@ 0x00454220 throws an
    /// `ATLINT::CException`): nothing between the board and the
    /// application's frame catches it; the original's
    /// `CAtlantis5::MainLoop` catch (@ 0x0043d456) logs it and ends the
    /// program (@ 0x004896d0(0)). Native logs it and goes on (a getter's
    /// results read as Nil).
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
        let cursor = PlayCursor::new(&*src);
        Ok(Play {
            game,
            stage,
            moves: (0.0, 0.0),
            clicks: 0,
            hover: None,
            menus,
            sound_held: false,
            poker: None,
            src,
            logos: Vec::new(),
            in_logos: false,
            cursor,
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
        // A move onto the button first (the hover is tested on moves
        // only): from beside it, then onto it; then the press.
        self.mouse_at(x + 1, y);
        self.update(0.0);
        self.mouse_at(x, y);
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
        // A move onto the button (a button's hover is tested on moves
        // only, and a show resets it): from beside it, then onto it.
        self.mouse_at(x + 1, y);
        self.update(0.0);
        self.mouse_at(x, y);
        self.update(0.0);
        self.click();
        true
    }

    /// The program's start (`CGameManager` ctor @ 0x0042eac0): the main
    /// menu asked (@ 0x0041b5d0(0)) and the request 3 queued: the game
    /// starts in the main menu.
    ///
    /// First the logos (the application's state 0, @ 0x0043d320): each
    /// played to its end by the film player's own loop (@ 0x0043d6a0, the
    /// full display as its rectangle, the step @ 0x0042b0f0 ending it on
    /// its last frame or a key), the game manager not run meanwhile; then
    /// state 1, the manager's frames. Their sound is the Bink files' own
    /// audio track (DCT, stereo, 44100 Hz), played as the films' player
    /// routes it ([`CHANNEL_BINK`]).
    pub fn boot(&mut self) {
        self.logos = LOGOS.iter().rev().copied().collect();
        self.in_logos = true;
        self.next_logo();
    }

    /// The next logo, or the main menu once they have played.
    fn next_logo(&mut self) {
        while let Some(name) = self.logos.pop() {
            let mut st = self.stage.borrow_mut();
            if st.video_play(name.as_bytes()) {
                st.logo_rect();
                return;
            }
        }
        self.in_logos = false;
        self.menus.request(menus::MAIN);
        self.game.open_menu();
    }

    /// Whether the start's logos are playing.
    pub fn logos_playing(&self) -> bool {
        self.in_logos
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
    /// with none, the sprite (@ 0x00430b70), the cursor (@ 0x0042c030);
    /// last the scene's update (@ 0x0044c380, the sprites' step) and the
    /// sound (@ 0x00459fd0). Moves and clicks outside play are dropped.
    pub fn update(&mut self, dt: f64) {
        if self.logos_playing() {
            // The logos' loop (@ 0x0043d6a0): the film's steps only.
            self.clicks = 0;
            self.moves = (0.0, 0.0);
            self.stage.borrow_mut().update(dt, false, true);
            let ended = std::mem::take(&mut self.stage.borrow_mut().wait_ended);
            if ended && !self.stage().film_playing() {
                self.next_logo();
            }
            return;
        }
        self.game.set_fov(self.stage.borrow().fov);
        let play = self.game.state() == 4 && !self.game.busy();
        let (dx, dy) = std::mem::take(&mut self.moves);
        let clicks = std::mem::take(&mut self.clicks);
        // One move event a frame at most (@ 0x0047c8a0: one
        // `GetDeviceState` of the mouse, the move handler called once when
        // lX, lY or lZ is not 0), so the look's rule (a third of a speed
        // against the move taken off, the ±6 clamp, @ 0x0042cdc0) runs
        // once on the frame's whole move, whatever the mouse's report rate.
        if play && (dx != 0.0 || dy != 0.0) {
            #[allow(clippy::cast_possible_truncation)]
            self.stage.borrow_mut().look(dx, dy, dt as f32);
        }
        // The modes asked by the last frame's scripts.
        self.take_menu_requests();
        // State 3 (the menus' frame @ 0x0041b780): the mode's process
        // first, then the scene's update (@ 0x0044c380 → the sprites'
        // step @ 0x00441c30) for modes 8, 9 and 0xc only (the inventory,
        // the subject menu, poker: the 3D view stays under them), then the
        // sound (@ 0x00459fd0); the other modes freeze the sprites.
        let mut scene = true;
        if self.game.state() == 3 && !self.game.busy() {
            let before = self.scene_under_menu();
            self.menu_frame(clicks, dt);
            scene = before || self.scene_under_menu();
        }
        // A dialog takes the frame's clicks (`Atl_Dialog_Play`'s loop).
        if self.stage.borrow().dialog_running() {
            self.stage.borrow_mut().clicks += clicks;
        }
        self.stage.borrow_mut().update_start(dt, play);
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
        self.cursor_frame(dt);
        // The scene's update and the sound last (@ 0x0042f000 case 4:
        // after the events, the timers, the pick and the cursor).
        self.stage.borrow_mut().update_scene(dt, scene);
    }

    /// The modes the scripts asked (`Atl_GameOver`, `Atl_ExamineItem`,
    /// `Atl_PuzzlePoker_Start`), queued in the menus. The original's
    /// handlers queue them in the menus at the call.
    fn take_menu_requests(&mut self) {
        let requests = std::mem::take(&mut self.stage.borrow_mut().menu_requests);
        for (mode, arg) in requests {
            if mode == menus::DEATH {
                self.menus.set_death_text(&lossy(&arg));
            } else if mode == menus::EXAMINE {
                self.menus.set_examine_item(&arg);
            }
            self.menus.request(mode);
        }
    }

    /// The game's cursor this frame. Loaded in state 4 (@ 0x0042fb90:
    /// @ 0x0042bb10 on entering it, @ 0x0042bf30 on leaving it). A play
    /// frame (@ 0x0042f000) gives it the pick's type (@ 0x0042bfd0, the
    /// aim when nothing is under the centre, 0x0042f390) and its update
    /// (@ 0x0042c030, 0x0042f3a3); so does each frame of an animated
    /// `Atl_Camera_LookAt` (@ 0x00406f00: the update only); a transition's
    /// loop (@ 0x00405390) does not update it. `Atl_Hero_HasFollower` is
    /// asked for the three followers where the cursor is drawn
    /// (@ 0x0042c780 from @ 0x0042c660): here, after the frame's scripts.
    fn cursor_frame(&mut self, dt: f64) {
        let in_play = self.game.state() == 4;
        if in_play != self.cursor.loaded {
            self.cursor.loaded = in_play;
            if !in_play {
                self.cursor.hand = None;
            }
        }
        if !in_play {
            return;
        }
        let looking = self.stage.borrow().look_turn.is_some();
        let playing = !self.game.busy();
        if playing || looking {
            if playing {
                let kind = self.stage.borrow().cursor.unwrap_or(CURSOR_AIM);
                self.cursor.set_kind(kind);
            }
            let hand = match self.game.script().handled_item() {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("{e}");
                    Vec::new()
                }
            };
            self.cursor.update(dt, &hand, &*self.src);
        }
        if self.cursor_drawn() {
            for (i, name) in FOLLOWERS.iter().enumerate() {
                // `lua_pcall`'s error message is a string: true for
                // `lua_toboolean` (@ 0x0042c7d1).
                self.cursor.shown_followers[i] = match self.game.script().one_shot(
                    "return Atl_Hero_HasFollower",
                    Some(&[Value::Str(name.as_bytes().to_vec())]),
                    1,
                ) {
                    Ok(v) => !matches!(v.first(), None | Some(Value::Nil | Value::Bool(false))),
                    Err(e) => {
                        log::warn!("{e}");
                        true
                    }
                };
            }
        }
    }

    /// Whether this frame's picture has the game's cursor (@ 0x0042c660's
    /// callers): state 4's display (@ 0x0042f780, the view then the
    /// cursor); an animated `Atl_Camera_LookAt`'s loop (@ 0x00406f00);
    /// a transition's loop (@ 0x0042e640) with `no_fx` or `centring`
    /// (`zoom_and_fade` draws its fade instead). Not during a film or a
    /// dialog (their loops draw their own picture) nor in a menu
    /// (state 3).
    pub fn cursor_drawn(&self) -> bool {
        if !self.cursor.loaded || self.game.state() != 4 {
            return false;
        }
        let st = self.stage.borrow();
        if st.film_playing() || st.dialog_running() {
            return false;
        }
        match self.game.transition_fx() {
            Some(fx) => fx != atlantis5_script::Fx::ZoomAndFade,
            None => !self.game.busy() || st.look_turn.is_some(),
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
    /// Whether the menus' mode is 8, 9 or 0xc (the inventory, the subject
    /// menu, poker), the modes whose frame steps the scene (@ 0x0041b780).
    fn scene_under_menu(&self) -> bool {
        let st = self.stage.borrow();
        st.inventory_open_now() || st.menu_open_now() || self.poker.is_some()
    }

    fn menu_frame(&mut self, clicks: usize, dt: f64) {
        if self.stage.borrow().inventory_open_now() {
            self.inventory_frame(clicks);
            return;
        }
        if !self.stage.borrow().menu_open_now() {
            self.menus_frame(clicks, dt);
            return;
        }
        // The menu's process (@ 0x00410f90): state 5 (asked by a click
        // elsewhere or `Atl_TerminateDialog` since its last frame) ends it.
        self.stage.borrow_mut().menu_close_step();
        if self.stage.borrow_mut().menu_take_closed() {
            // A mode queued meanwhile (`Atl_PuzzlePoker_Start` from a
            // subject's script): the manager stays in state 3 and opens it
            // at the next frame (@ 0x0041b780).
            self.take_menu_requests();
            if !self.menus.active() {
                self.game.menu_closed();
            }
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
            self.cursor_to_stage_mouse();
        }
        for _ in 0..clicks {
            self.stage.borrow_mut().menu_click();
            let pick = self.stage.borrow_mut().menu_take_pick();
            if let Some((c, s)) = pick {
                self.game.click_subject(&c, &s);
                break;
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
                // (the death panel of `Atl_GameOver`, which the board's
                // `OnLose` may have asked in this very frame) the manager
                // stays in state 3, else it takes its next request.
                self.take_menu_requests();
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
            self.cursor_to_stage_mouse();
            self.inventory_hand_name();
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
        // A save page's slot not written: its autosave copy is left as it
        // was too, so the slot and its `autoslot_<n>` stay a pair.
        let mut save_failed = false;
        let mut load_failed = false;
        for e in self.menus.take_effects() {
            if save_failed && matches!(e, Effect::Delete(_) | Effect::Copy { .. }) {
                continue;
            }
            if let Effect::Save(file) = &e {
                save_failed = !self.game.save_now(file);
                continue;
            }
            if let Effect::Load(file) = &e {
                load_failed = !self.game.load_file(file);
                continue;
            }
            self.menu_effect(e);
        }
        if let (true, Some(mode)) = (load_failed, ended) {
            // A save that exists but cannot be read (empty, cut short,
            // not an `ATL_FILE`): nothing is queued, and with the menu
            // closed the manager would go round states 0 and 3 with no
            // menu. The original closes the page after its reader
            // whatever the read did (0x00418c29) and leaves the menus with
            // no mode; Native opens the page again.
            log::warn!("the save could not be read: its page again");
            self.menus.request(mode);
            return;
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
        let dir = self.stage.borrow().saves.clone();
        match e {
            Effect::NewGame => {
                self.poker = None;
                self.stage.borrow_mut().end_game();
                self.game.new_game();
            }
            Effect::Load(file) => {
                self.game.load_file(&file);
            }
            Effect::Save(file) => {
                let _ = self.game.save_now(&file);
            }
            Effect::Request(r) => self.game.queue_request(r),
            Effect::Delete(file) => {
                if let Some(d) = dir {
                    d.remove(&file);
                }
            }
            Effect::Copy { from, to } => {
                if let Some(d) = dir {
                    d.copy(&from, &to);
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
                self.game.run_arrange_handled_object();
            }
            Ok(_) => {
                // Mode 8, state 3 (@ 0x004305d0).
                let (size, items) = self.game.script().inventory();
                self.game.open_menu();
                self.stage
                    .borrow_mut()
                    .inventory_open(size, items, Vec::new());
                self.cursor_to_stage_mouse();
                self.inventory_hand_name();
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
            self.inventory_hand_name();
        }
        // The bubble (@ 0x00416c50, after the events).
        if self.stage.borrow().inventory_open_now() {
            let mut st = self.stage.borrow_mut();
            st.inventory_hover(|item| self.item_description(item));
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

    /// A relative move of the mouse in a menu (DirectInput's lX, lY,
    /// @ 0x0047c8a0): the menus' mouse moved by it and kept within
    /// 0..=1024, 0..=576 (@ 0x0040d890).
    pub fn mouse_by(&mut self, dx: i32, dy: i32) {
        let (x, y) = self.menus.mouse;
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let (w, h) = (VIEW_WIDTH as i32, VIEW_HEIGHT as i32);
        self.mouse_at(
            x.saturating_add(dx).clamp(0, w),
            y.saturating_add(dy).clamp(0, h),
        );
    }

    /// The drawn cursor moved where the stage put the mouse: a mode's
    /// initialize puts it at the screen's centre (menus.md: the subject
    /// menu's state 2, the inventory's open), where both the hit tests
    /// and the cursor then are.
    fn cursor_to_stage_mouse(&mut self) {
        let (x, y) = self.stage.borrow().mouse;
        self.menus.mouse_at(x, y);
    }

    /// The folder of the player's saves.
    pub fn set_save_dir(&mut self, dir: Option<std::path::PathBuf>) {
        self.set_saves(dir.map(|d| {
            Rc::new(crate::atlantis5_saves::DirSaves(d))
                as Rc<dyn crate::atlantis5_saves::SaveFiles>
        }));
    }

    /// The player's saves wherever they are kept (the web build's
    /// storage).
    pub fn set_saves(&mut self, saves: Option<Rc<dyn crate::atlantis5_saves::SaveFiles>>) {
        self.menus.saves.clone_from(&saves);
        self.stage.borrow_mut().set_saves(saves);
    }

    /// What the last pick found under the centre.
    pub fn hover(&self) -> Option<&Hover> {
        self.hover.as_ref()
    }

    /// A mouse move of (dx, dy) (DirectInput's relative lX, lY), taken at
    /// the next frame: the moves of a frame add up into one, as the
    /// device's state the original reads once per frame (@ 0x0047c8a0).
    /// Every front end (mouse, pointer lock, touch drag) may call it once
    /// per system event.
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.moves.0 += dx;
        self.moves.1 += dy;
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
        // A script waiting in a handler's loop (a subject's or an
        // inventory click's dialog, film or transition) draws that loop's
        // frame: no menu, no menus' cursor.
        let busy = self.game.busy();
        st.render_with(out, !busy);
        if self.cursor_drawn() {
            self.cursor.draw(out, st.config.crosshair);
        }
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
        } else if !busy && (st.inventory_open_now() || st.menu_open_now()) {
            self.menus.draw_cursor(out);
            st.draw_inventory_over_cursor(out);
        }
    }

    /// An item's description in the inventory (@ 0x00416c50,
    /// @ 0x00414db0): the string `_item_<item>` of the texts, or the
    /// program's `Description introuvable : _item_<item>` (0x0066bb34)
    /// when there is none.
    fn item_description(&self, item: &[u8]) -> String {
        let name = String::from_utf8_lossy(item);
        self.menus
            .string_opt(&format!("_item_{name}"))
            .unwrap_or_else(|| format!("Description introuvable : _item_{name}"))
    }

    /// The hand name's text after the inventory's elements are made
    /// (@ 0x00414db0 when it opens, @ 0x00415fe0 after a click): the
    /// hand's item's description, empty with an empty hand.
    fn inventory_hand_name(&mut self) {
        let hand = self.game.script().handled_item().unwrap_or_default();
        let text = if hand.is_empty() {
            String::new()
        } else {
            self.item_description(&hand)
        };
        self.stage.borrow_mut().inventory_set_hand_name(text);
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
    fn a_film_is_stretched_into_its_rectangle() {
        // A 2 × 2 film into the logos' rectangle: each source pixel fills
        // a quarter of (0, 0, 1023, 575); the last column and row stay.
        let src = [1, 2, 3, 4];
        let mut out = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
        stretch_into(&mut out, &src, 2, 2, [0, 0, 1023, 575]);
        let at = |x: usize, y: usize| out[y * VIEW_WIDTH + x];
        assert_eq!(
            (
                at(0, 0),
                at(510, 286),
                at(512, 0),
                at(0, 288),
                at(1022, 574)
            ),
            (1, 1, 2, 3, 4)
        );
        assert_eq!((at(1023, 0), at(0, 575)), (0, 0));
        // 1:1 into the display's rectangle.
        let src: Vec<u32> = (0..VIEW_WIDTH * VIEW_HEIGHT).map(|i| i as u32).collect();
        let mut out = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
        stretch_into(&mut out, &src, VIEW_WIDTH, VIEW_HEIGHT, FULL_RECT);
        assert_eq!(out, src);
    }

    #[test]
    fn a_look_at_after_no_fx_shows_its_angles_at_once() {
        let mut s = stage();
        s.set_yaw(0.0);
        // `Atl_Camera_ActivateFx("NoFx")` keeps the camera's rotation
        // (@ 0x0042cd50) until the setters rebuild it: the 2-argument
        // `Atl_Camera_LookAt` (@ 0x00406f00) calls them (@ 0x004447a0,
        // @ 0x004448c0), so the next picture has its angles.
        s.call(&Call::CameraActivateFx(CameraFx::NoFx));
        s.call(&Call::CameraLookAt {
            pitch: 0.0,
            yaw: 270.0,
            animated: false,
        });
        let mut want = stage();
        want.set_pitch(0.0);
        want.set_yaw(270.0);
        assert_eq!(s.camera().q, want.camera().q);
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
            s.update(1.0 / 30.0, false, true);
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

    /// `zoom_and_fade`'s darkening (@ 0x0047a8a4): c′ = (c · (256 −
    /// ftol(k · 255))) >> 8, black at k = 1; kept from the transition's
    /// requests until its end.
    #[test]
    fn the_transition_fade_darkens_the_view() {
        let mut px = [0x00ff_8040u32, 0x0001_0203];
        darken(&mut px, 0.5);
        // α = ftol(127.5) = 127: 256 − 127 = 129.
        assert_eq!(
            px[0],
            (((255 * 129) >> 8) << 16) | (((0x80 * 129) >> 8) << 8) | ((0x40 * 129) >> 8)
        );
        assert_eq!(px[1], 0x0000_0101);
        let mut px = [0x00ff_ffffu32];
        darken(&mut px, 1.0);
        assert_eq!(px[0], 0);
        let mut s = stage();
        s.request(&Request::TransitionFov {
            radians: 1.0,
            fade: 0.25,
        });
        assert_eq!((s.fov, s.fade), (1.0, 0.25));
        s.request(&Request::TransitionEnd);
        assert_eq!(s.fade, 0.0);
    }

    /// A decoder that gives `left` frames, then none.
    struct Frames(usize);

    impl MusicDecoder for Frames {
        fn read_frames(&mut self, out: &mut Vec<i16>, max: usize) -> usize {
            let n = self.0.min(max);
            self.0 -= n;
            out.extend(std::iter::repeat_n(0, 2 * n));
            n
        }
    }

    #[test]
    fn a_music_file_without_sound_ends_even_played_for_ever() {
        // 999 plays (for ever) of a file that gives no frame: the feed
        // ends instead of opening it again without end.
        let (mut pushed, mut done, mut plays) = (0, false, -1);
        feed_music(
            &mut Frames(0),
            &mut pushed,
            &mut done,
            &mut plays,
            44_100,
            || Some(Frames(0)),
            |_| {},
        );
        assert!(done);
        assert_eq!(pushed, 0);
        // A file with sound starts again for ever, and 3 plays end.
        let (mut pushed, mut done) = (0, false);
        feed_music(
            &mut Frames(1000),
            &mut pushed,
            &mut done,
            &mut plays,
            44_100,
            || Some(Frames(1000)),
            |_| {},
        );
        assert!(!done);
        assert!(pushed >= 44_100);
        let (mut pushed, mut done, mut plays) = (0, false, 3);
        feed_music(
            &mut Frames(1000),
            &mut pushed,
            &mut done,
            &mut plays,
            44_100,
            || Some(Frames(1000)),
            |_| {},
        );
        assert!(done);
        assert_eq!((pushed, plays), (3000, 1));
    }

    /// The half music level (@ 0x0045caf0 / 0x0045cb20): the first holder
    /// fades it to half over 1 s, the last release back to the
    /// configuration's music over 1 s; nested holders change nothing.
    #[test]
    fn the_music_level_ducks_under_its_holders() {
        let mut s = stage();
        s.config.music = 0.8;
        s.music_update(0.0);
        assert!((s.music_level() - 0.8).abs() < 1e-6);
        s.music_duck(0.5);
        s.music_update(0.5);
        assert!((s.music_level() - 0.6).abs() < 1e-6);
        s.music_duck(0.5);
        s.music_update(0.5);
        assert!((s.music_level() - 0.4).abs() < 1e-6);
        s.music_update(0.5);
        assert!((s.music_level() - 0.4).abs() < 1e-6, "the fade over");
        s.music_unduck();
        s.music_update(1.0);
        assert!((s.music_level() - 0.4).abs() < 1e-6, "one holder left");
        s.music_unduck();
        s.music_update(0.25);
        assert!((s.music_level() - 0.5).abs() < 1e-6);
        s.music_update(2.0);
        assert!((s.music_level() - 0.8).abs() < 1e-6);
        s.music_unduck();
        assert_eq!(s.duck_count, 0);
        // The sound page's change reaches the level.
        s.config.music = 0.3;
        s.music_update(0.0);
        assert!((s.music_level() - 0.3).abs() < 1e-6);
    }

    /// `CAMERA_MOVER` written (@ 0x00437f40) and read back (@ 0x004383b0):
    /// the mode with its speeds or turn, the boat's clock and node; the
    /// camera's parent (`on_boat`) left as it was; a save without the
    /// section (Native's older ones) changes nothing.
    #[test]
    fn the_camera_mover_section_round_trips() {
        let mut s = stage();
        s.call(&Call::CameraActivateFx(CameraFx::BoatFx([0.0, 0.0, 1.0])));
        s.camera_step(0.7);
        s.speed = [1.5, -0.25];
        s.damping = 0.125;
        let e = s.camera_mover_section();
        let text = atlantis5_script::save::to_text(&e);
        assert!(text.contains("moving_type=\"0\""), "{text}");
        assert!(text.contains("fx_type=\"0\""), "{text}");
        assert!(text.contains("camera_speed_x=\"-0.25\""), "{text}");
        let mut t = stage();
        t.read_camera_mover(&e);
        assert_eq!((t.mode, t.speed, t.damping), (0, s.speed, s.damping));
        assert_eq!(t.boat, s.boat);
        assert!(t
            .boat_q
            .iter()
            .zip(s.boat_q)
            .all(|(a, b)| (a - b).abs() < 1e-6));
        assert!(!t.on_boat, "not re-parented");
        // A turn (mode 2), no boat: the two vectors, no FX_DATAS.
        let mut s = stage();
        s.start_turn(forward(10.0, 40.0));
        s.camera_step(0.1);
        let e = s.camera_mover_section();
        assert!(e.child("FX_DATAS").is_none());
        let mut t = stage();
        t.read_camera_mover(&e);
        assert_eq!(t.mode, 2);
        let (a, b) = (t.turn.unwrap(), s.turn.clone().unwrap());
        assert!((a.t - b.t).abs() < 1e-6);
        assert!(a.to.iter().zip(b.to).all(|(x, y)| (x - y).abs() < 1e-6));
        assert!(t.boat.is_none());
        // No section: nothing changes.
        let root = atlantis5_script::save::element("ATL_FILE");
        let mut u = stage();
        u.speed = [2.0, 3.0];
        u.load_sections(&root);
        assert_eq!(u.speed, [2.0, 3.0]);
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
        // NoFx: the camera back under the root, its world rotation kept
        // until the next free step rebuilds it from the angles.
        let world = s.camera().q;
        let n = s.boat_q;
        s.call(&Call::CameraActivateFx(CameraFx::NoFx));
        assert!(!s.on_boat && s.boat.is_none());
        assert_eq!(s.camera().q, world);
        assert_eq!(s.boat_q, n, "the node keeps its rotation");
        assert!(s.camera_step(0.0));
        assert_eq!(s.camera().q, still);
        // BoatFx again: the view L ⊗ N⁻¹ ⊗ W = R(−a) ⊗ q until the next
        // free step (play.md § 2).
        s.call(&Call::CameraActivateFx(CameraFx::BoatFx([0.0, 0.0, 1.0])));
        let n_inv = [-n[0], -n[1], -n[2], n[3]];
        let want = quat_mul(&n_inv, &still);
        assert!(s
            .camera()
            .q
            .iter()
            .zip(want)
            .all(|(x, y)| (x - y).abs() < 1e-6));
        assert!(s.camera_step(0.0));
        let q = s.camera().q;
        assert!(q.iter().zip(still).all(|(x, y)| (x - y).abs() < 1e-6));
    }
}

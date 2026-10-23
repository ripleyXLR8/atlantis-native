//! The game, as far as it goes: a new game opens on its first view; the
//! player looks around, the place animations play, and a click on a move
//! zone walks to the next view as the original does (turn, transition
//! video, arrival); a dialog can be played over the view.
//!
//! Time follows the original: the frame dispatcher (@ 0x0049ce20) runs as
//! often as frames are drawn ([`Engine::pass`]), and a 15 Hz task
//! ([`Engine::tick`], @ 0x0040caa0) paces the videos, the place animations
//! and the cursor. The turn of a move is counted in frames.

use atlantis2_data::con::NewGame;
use atlantis2_data::defnames::{self, DefNames};
use atlantis2_data::dialog::{FontTxt, World};
use atlantis2_data::messages::ObjectNames;
use atlantis2_data::spr::{SprBank, Target};
use atlantis2_data::spw::{FaceBox, Spw};
use atlantis2_data::wam::{self, View, WamError, Zone};
use atlantis2_data::{A2Files, Folder, Wam};
use atlantis_codecs::{DecodeError, HnmVideo};

use crate::anim::{restore, Anim, Texel};
use crate::bag::{Bar, Overlays, Rod, UiDraw};
use crate::camera::{deg_to_rad, zone_aim, Camera, WARP_FOV};
use crate::crt::Rand;
use crate::dialog::{Dialog, DialogError, DialogScript, FrameContext, TickContext};
use crate::enhancements::Enhancements;
use crate::fpu::Fpu;
use crate::look::{Look, DEFAULT_LIMIT_Y, NO_LIMIT};
use crate::menu::Menu;
use crate::script::{BagEntry, Script};
use crate::sound::{MusicVolume, Pcm, Voices};
use crate::subjects::SubjectMenu;
use crate::subtitle::SubtitleFont;
use crate::turn::{Turn, Zoom};
use crate::warp::{faces_from_hnm, Renderer, Variant};

mod blackbird;
mod chest;
mod cross;
mod figurines;
mod hd;
pub use figurines::FIGURINE_TARGETS;
mod display;
mod dragon;
mod host;
mod labyrinth;
pub use labyrinth::{
    soul_cell, soul_drops, soul_open, soul_random, soul_visible, Hood, SOULS_MODE,
};
mod menus;
mod ogham;
pub use ogham::{ogham_click, OghamClick, OGHAM_START, OGHAM_TARGET};
mod places;
mod puzzles;
pub use puzzles::{
    chest_mode_cleared, chinese_click_hooks, chinese_entry_hooks, irish_click_hook,
    irish_entry_hook, mandala_hook_applies, ported_place, MANDALAS, SKULL_OBJECTS, SKULL_START,
};
mod saves;
mod subjects;
mod turtle;
pub use turtle::RING_MODE;
mod ui;
mod wheels;
pub use wheels::{DISC_WHEEL, WHEELS_FIRST, WHEELS_SECOND, WHEEL_DISC};

pub use blackbird::Blackbird;
pub use dragon::Dragon;
pub use saves::SaveError;
pub use subjects::subjects_bank_name;

/// The scheduler's tick: 15 a second (the dialogs' sequencer, the music
/// fades, the animations, the videos).
pub const TICK: f64 = 1.0 / 15.0;

/// Why the engine cannot show a view.
#[derive(Debug)]
pub enum EngineError {
    /// `scenar\chapi000.con` is missing or does not say where to start.
    NoStart,
    NoWam(String),
    BadWam(String, WamError),
    NoView(String, u32),
    /// The view's warp file is on no disc found.
    NoWarp(String),
    BadWarp(String, DecodeError),
    /// A fatal error of the chapter scripts (the original stops there).
    Script(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::NoStart => write!(f, "scenar\\chapi000.con missing or without a start"),
            EngineError::NoWam(w) => write!(f, "wam\\{w} not found"),
            EngineError::BadWam(w, e) => write!(f, "wam\\{w}: {e}"),
            EngineError::NoView(w, v) => write!(f, "{w} has no view {v}"),
            EngineError::NoWarp(n) => write!(f, "datas_a2\\warp\\{n} not found on the discs"),
            EngineError::BadWarp(n, e) => write!(f, "datas_a2\\warp\\{n}: {e}"),
            EngineError::Script(e) => write!(f, "script: {e}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// What the engine did, for tests and logs ([`Engine::trace`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A left click reached the game (and whether it was a double click).
    Click { double: bool },
    /// `CtrlScenarOUT`, the departure scan (`DEPARTVUE` = the target).
    DepartScan(ScanVars),
    /// The click scan (@ 0x0045a870).
    ClickScan(ScanVars),
    /// The arrival scan of `Interp_GereFrame` (`ARRIVEEVUE` = the view).
    ArrivalScan(ScanVars),
    /// The turn set-up towards (yaw, pitch), radians.
    TurnSetup { yaw: f32, pitch: f32, turning: bool },
    /// A view entered (@ 0x00459fe0).
    Enter { view: u32 },
    /// A view made current whose entry was skipped by `NextWarpInvalide`
    /// = 1 (@ 0x0045a01a): no panorama, camera, animations nor sounds.
    EnterSkipped { view: u32 },
    /// An SPW slot loaded, or a file not found.
    SpwLoad { name: String, found: bool },
    /// `AtlHnm_start` on a transition: the file, and its frame count when
    /// it opened.
    VideoStart { name: String, frames: Option<u32> },
    /// A video frame shown.
    VideoFrame(u32),
    /// The video loop ended.
    VideoEnd,
    /// An object taken from the panorama (`CLICOBJET`).
    Take { id: i32 },
    /// A click on a zone the puzzle hooks take (@ 0x0049b910): the zone's
    /// index in the view, and `CLICZONE` as the hook finds it.
    PuzzleHook { zone: usize, cliczone: i32 },
    /// The zone click FUN_0049ca80 done on the zone of index `zone`:
    /// `CLICZONE` as it leaves it (the click scan may not follow: a film a
    /// hook's scan started drops the click).
    ZoneClick { zone: usize, cliczone: i32 },
    /// A puzzle hook the port does not have (China's puzzles): the
    /// original's function, which would have run here.
    PuzzleNotPorted { function: u32 },
    /// A puzzle function the port has, run: the original's function (a
    /// hook, or the wheels' check FUN_0048d310).
    PuzzleRan { function: u32 },
    /// A labyrinth soul's passage started (`(NewSound=220)`) or ended
    /// (`(NewSound=221)`, a sound on no disc) in slot `slot`
    /// (FUN_0048eb30).
    SoulPassage { slot: usize, start: bool },
    /// Brigid's cross laid on the zone of value `value` (FUN_00494190).
    CrossLaid { value: i32 },
    /// Brigid's cross taken back from the zone of value `value`.
    CrossTaken { value: i32 },
    /// A move to another place (a type-2 zone, @ 0x0040fc60): its file,
    /// the view, and whether it waits for the turn (deferred) or is made
    /// at once.
    LinkMove {
        wam: String,
        view: u32,
        deferred: bool,
    },
    /// A transition sprite bank loaded (`datas_a2\spf\`, @ 0x0047c100), or
    /// not found.
    SpfLoad { name: String, found: bool },
    /// A place sound started (@ 0x0047b4d0): its slot (voice 3 + slot) and
    /// file.
    PlaceSound { slot: usize, name: String },
    /// The scripts asked for a place of a world the port does not have
    /// yet (`changewam`): refused, the game stays where it is.
    WorldNotPorted(String),
    /// A `/set` line of the chapter scripts ran: the chapter and the line
    /// number in its file.
    Set { chapter: i32, line: usize },
    /// `AtlWam_Load` (`changewam`).
    Wam(String),
    /// `Interp_SetMonde` (`monde`).
    Monde(i32),
    /// A dialog started (`AtlDial_start`), or could not.
    DialogStart { number: u32, started: bool },
    /// A dialog line began.
    DialogLine(String),
    /// The dialog loop ended (@ 0x004808a0).
    DialogEnd,
    /// A music opened (`muzik%02d.apc`) at a mark (0 its start).
    Music { music: i32, mark: i32 },
    /// The music stopped (`stopmusik`'s fade done).
    MusicStop,
    /// A sound started on voice 2 (a film's track, `NEWSOUND`,
    /// `TrajSound`, the blackbird game's songs and answers).
    Sound(String),
    /// A sound decoded into the sound table (@ 0x0047a530 then
    /// @ 0x0047aad0, a name not loaded yet): its entry, −1 when the file
    /// could not be read.
    SoundLoaded { name: String, entry: i32 },
    /// `ViewBigSpr` (@ 0x0048c9d0): the picture viewer opened on that
    /// bank of `spr\`, found or not.
    Picture { name: String, found: bool },
    /// A click closed the picture viewer (@ 0x0048ca50).
    PictureEnd,
    /// The right button opened or closed the bag bar.
    Bag { open: bool },
    /// An object picked from the bag bar.
    BagPick { id: i32 },
    /// A click on a character opened its subject menu (@ 0x0045a7c0).
    SubjectMenu { character: i32, count: i32 },
    /// The subject menu answered: a subject's value, or none (closed).
    Subject { character: i32, value: Option<i32> },
}

/// The script variables a scan sees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanVars {
    pub departvue: i32,
    pub arriveevue: u32,
    pub cliczone: i32,
    pub clicperso: i32,
    pub clicobjet: i32,
}

/// A draw at the screen centre (FUN_0045af10), in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CentreDraw {
    /// `AtlSpr_Aff` (@ 0x0047bc40) of a sprite with its hot spot at
    /// (x, y).
    Sprite {
        bank: CursorBank,
        frame: i32,
        x: i32,
        y: i32,
    },
    /// A line (@ 0x004c4620) from one point to another, ARGB.
    Line {
        from: (i32, i32),
        to: (i32, i32),
        argb: u32,
    },
}

/// The loop types of the dispatcher (@ 0x006d8520) the engine has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopType {
    /// 0: the panorama.
    Warp,
    /// 1: a video (`AtlMain_loop_HNM` @ 0x0040c060).
    Video,
    /// 2: a dialog (@ 0x004616f0).
    Dialog,
    /// 5: the menus (@ 0x00486eb0).
    Menu,
    /// 11: a full-screen picture (`ViewBigSpr`, @ 0x0048ca50) until a
    /// click.
    Picture,
}

/// The script variables the navigation sets (`defnames.con` names).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vars {
    pub vue: u32,
    pub last_vue: u32,
    pub arrivee_vue: u32,
    pub cliczone: i32,
    pub clicobjet: i32,
    pub clicperso: i32,
    pub objet_en_main: i32,
}

/// The bank of the cursor's (bank, frame) pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorBank {
    /// `spr\interfac.spr` (@ 0x006719fc).
    Interface,
    /// The world's objects bank (@ 0x00671ef4): set by a take
    /// (@ 0x0045a650) until the next cursor choice.
    Objects,
}

/// The cursor (@ 0x00474500): a range of `interfac.spr` frames played on
/// the 15 Hz flag @ 0x00656a54, and the (bank, frame) pair drawn at the
/// screen centre (@ 0x00673fa0 / 0x005def58).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// @ 0x00671a00: the range's first frame, −1 none.
    pub start: i32,
    /// @ 0x006719e8: one past its last frame.
    pub end: i32,
    /// @ 0x006719f0: the frame shown next.
    pub frame: i32,
    /// The frame drawn, `None` when hidden.
    pub shown: Option<i32>,
    /// The bank of the frame drawn.
    pub bank: CursorBank,
}

impl Default for Cursor {
    fn default() -> Cursor {
        Cursor {
            start: -1,
            end: 0,
            frame: 0,
            shown: None,
            bank: CursorBank::Interface,
        }
    }
}

impl Cursor {
    /// FUN_00474500(zone type, sprite state): moves (types 1, 2) frames
    /// 1..12, sprite state 1 frames 13..24, state 2 and zones 3.. frames
    /// 25..39; a range restarts only when it changes; types ≥ 1000 show
    /// nothing; one frame on per 15 Hz flag.
    fn choose(&mut self, kind: i32, state: i32, flag: &mut bool) {
        if (kind == 1 || kind == 2) && self.start != 1 {
            (self.start, self.end, self.frame) = (1, 13, 1);
        } else {
            let wide = if state == 1 {
                if self.start != 13 {
                    (self.start, self.end, self.frame) = (13, 25, 13);
                    false
                } else {
                    !(kind < 3 || self.start == 25)
                }
            } else if state != 2 || self.start == 25 {
                !(kind < 3 || self.start == 25)
            } else {
                true
            };
            if wide {
                (self.start, self.end, self.frame) = (25, 40, 25);
            }
        }
        self.shown = (kind <= 999).then_some(self.frame);
        self.bank = CursorBank::Interface;
        if *flag {
            *flag = false;
            self.frame += 1;
        }
        if self.frame == self.end {
            self.frame = self.start;
        }
    }

    /// FUN_0047bf80: no cursor (and the range forgotten).
    fn hide(&mut self) {
        self.shown = None;
        self.start = -1;
    }
}

/// A video playing (a transition, a film, the intro).
struct Video {
    hnm: HnmVideo,
    name: String,
    /// The loop type it returns to (@ 0x00616890).
    prev: LoopType,
    /// The tick counter when the last frame was shown (@ 0x006569dc).
    tick: u32,
    end: bool,
    /// The frame counter of the frame shown (@ 0x00616884 before its
    /// increment): the transition sprites' frame.
    shown: u32,
}

/// The engine.
pub struct Engine {
    files: A2Files,
    wam: Wam,
    wam_name: String,
    names: DefNames,
    view: u32,
    /// The view's faces as loaded, and the faces drawn on (with the
    /// sprites).
    faces: [Vec<u16>; 6],
    work: [Vec<u16>; 6],
    renderer: Renderer,
    /// The screen's size and, at 800 x 600, the surface the frame is
    /// drawn on (`crate::display`, `engine/display.rs`).
    display: crate::display::Display,
    /// The HD mode's view and state (`engine/hd.rs`).
    hd: hd::HdState,
    pub camera: Camera,
    pub look: Look,
    /// The place animations.
    pub anim: Anim,
    pub loop_type: LoopType,
    turn: Option<Turn>,
    /// The zoom of a move (`ZoomStart`, @ 0x005db850..0x005db858).
    pub zoom: Zoom,
    /// The next warp frame skips its draw (@ 0x005db84d = 0, set by the
    /// end of a zoom, FUN_0045b680; the frame sets it back).
    draw_skip: bool,
    move_pending: bool,
    next_view: Option<u32>,
    /// The arrival angles (@ 0x005e2f30 / 0x005e2f34), radians.
    arrival: (f32, f32),
    /// The zone under the centre (@ 0x006d85e0) as (view, index): the
    /// pointer stays on the old view's zone after a move until the next
    /// pick.
    centre_zone: Option<(u32, usize)>,
    sprite_hit: Option<usize>,
    /// Whether the draw finds the sprites under the centre. False
    /// reproduces the harness of the moves golden, which stubs the draw.
    pub sprite_picks: bool,
    /// The texel under the screen centre the sprite pick uses, instead of
    /// the camera's: the tests against the original set it as its harness
    /// does.
    pub forced_centre: Option<Texel>,
    /// What the last pass drew at the screen centre.
    pub centre_draws: Vec<CentreDraw>,
    /// The sprite cursor state of the last draw (@ 0x006c2df4).
    sprite_state: i32,
    /// Input flags: a click (@ 0x006569c9) and a double click
    /// (@ 0x006569cb), and the time of the last click (@ 0x006569a8).
    click: bool,
    double: bool,
    last_click: Option<f64>,
    /// Escape held: the key state (@ 0x006e2b6c, written by the input
    /// poll @ 0x0058ea8e) of the key that the key table (@ 0x006e3174)
    /// names for code 1, which the credits read (@ 0x00472378).
    escape_down: bool,
    /// The 15 Hz counter (@ 0x006569d8) and the flags it sets for the
    /// SPW step (@ 0x00656a4c) and the cursor (@ 0x00656a54).
    pub ticks: u32,
    spw_flag: bool,
    cursor_flag: bool,
    /// The frame dispatcher's 15 Hz flag (@ 0x00656a50): set by every
    /// tick (@ 0x0040cace), cleared after every pass of the dispatcher
    /// (@ 0x0049ce56 .. 0x0049cf29); the turtle ring's tick reads and
    /// clears it (@ 0x0049c41f..0x0049c433).
    pub frame_flag: bool,
    /// Whether any SPW was ever loaded (@ 0x006c2df8).
    spw_loaded: bool,
    /// The labyrinth entry's counts: the candidate states (@ 0x006d18d8)
    /// and the souls loaded (@ 0x006d18e0), as the last entry left them.
    pub souls_candidates: i32,
    pub souls_loaded: i32,
    video: Option<Video>,
    /// The start-up intro: playing (@ 0x006d85f1) and the number of its
    /// next file (@ 0x006d85f4).
    intro_playing: bool,
    intro_next: u32,
    pub vars: Vars,
    pub cursor: Cursor,
    cursor_bank: Option<SprBank>,
    /// The world's objects bank (`spr\obj*.spr`, @ 0x00671ef4).
    objects_bank: Option<SprBank>,
    /// The world's subjects bank (`spr\suj*.spr`, @ 0x006cfb5c), loaded by
    /// `/set(monde=N)` only.
    subjects_bank: Option<SprBank>,
    /// The subject menu of a character.
    pub subject_menu: SubjectMenu,
    /// The centre marker option (@ 0x005db84c, on by default).
    pub centre_marker: bool,
    /// The ids of the objects taken by a click, in order.
    pub bag: Vec<i32>,
    /// Without the scripts, the bag's entries those takes make (with the
    /// scripts the bag is theirs).
    own_bag: Vec<BagEntry>,
    /// `MONDE` when the scripts do not run.
    monde: i32,
    /// The bag bar, the overlays of the special objects, the dowsing rod.
    pub bar: Bar,
    pub overlays: Overlays,
    pub rod: Rod,
    /// The mouse pointer (@ 0x006569c0 / 0x006569bc), in screen pixels:
    /// drawn and read while the bar is open.
    pub pointer: (i32, i32),
    /// The click's companion flag (@ 0x006569a0): set by the right button,
    /// cleared with the click at the end of each warp frame.
    pub click_aux: i32,
    /// What the last pass drew over the panorama and the centre: the
    /// rod, the overlays, the bar and the pointer.
    pub ui_draws: Vec<UiDraw>,
    /// `Messages.txt`'s object names, the popup font of the bar.
    object_names: ObjectNames,
    popup_font: Option<SubtitleFont>,
    /// The object in hand: its id and sprite in the world's objects bank
    /// (the pair @ 0x00673f68 / 0x00673f94, drawn under the cursor).
    pub hand: Option<(i32, i32)>,
    /// Record [`Event`]s into [`Engine::events`].
    pub trace: bool,
    pub events: Vec<Event>,
    /// The world's dialog scripts (loaded with the world, @ 0x0047fe50).
    dial_script: Option<DialogScript>,
    dialog: Option<Dialog>,
    /// The loop type a dialog returns to (@ 0x006cf2b4).
    dialog_prev_loop: LoopType,
    /// The music's volume and fades.
    pub music: MusicVolume,
    /// Brigid's cross slot's id (@ 0x006d26b8), kept by the view 2
    /// entry hook (`DefName_GetIdName(1, "CROIXBRIGIT_SPRITE")`).
    cross_sprite: i32,
    /// The blackbird songs of Ireland (`engine/blackbird.rs`), never reset
    /// nor saved.
    pub blackbird: Blackbird,
    /// The slots of the six figurines of the miniature garden as their
    /// entry last found them (@ 0x006d18b8..0x006d18cc; 0 before any),
    /// used by their click in view 6 (`engine/figurines.rs`).
    pub figurine_slots: [usize; 6],
    /// The script variables while the interpreter runs a `vue=` line (it
    /// is out of the engine then), for the view entry's puzzle set-up.
    lent_vars: Option<Vec<i32>>,
    /// The skull puzzle's state (@ 0x005e1a4c, save entry 30): seven
    /// digits and a 0, one per frame of `CRANE_SPRITE`'s list; only a load
    /// changes it besides the puzzle's click (no new game resets it).
    pub skull: [u8; 8],
    /// `CRANE_SPRITE`'s id as the skull's view entry last looked it up
    /// (@ 0x006d26a8; 0 before any).
    pub skull_sprite: i32,
    /// The animation records of `RoueK1..5` as the wheels' view entry
    /// last found them (@ 0x006d189c..0x006d18ac), for their clicks.
    pub wheel_slots: [usize; 5],
    /// The Ogham stones' 15 bytes (@ 0x005e20dc, the save's entry 31):
    /// set at start-up, written by the puzzle's clicks and by a load, not
    /// by a new game.
    pub ogham: [u8; 15],
    /// The Ogham sprite's id as the view entry found it (@ 0x006d2768),
    /// for the clicks.
    ogham_sprite: i32,
    /// The pearl and dragon puzzle's state (`engine/dragon.rs`), never
    /// reset nor saved.
    pub dragon: Dragon,
    /// Where the port may differ from the original (default: the minor
    /// bug fixes on).
    pub enhancements: Enhancements,
    voices: Voices,
    /// The C runtime's `rand` (seeded by the dialog object's `srand`).
    pub rand: Rand,
    font: Option<SubtitleFont>,
    /// The subtitles option (@ 0x005df970, on by default).
    pub subtitles: bool,
    /// The dialog voice volume, 0..127 (@ 0x005df974).
    pub voice_volume: i32,
    /// The effects volume, 0..127 (@ 0x005dd2bc).
    pub fx_volume: i32,
    /// The scheduler's clock (seconds), its next tick and the time of the
    /// last one (the 15 Hz task's previous run, @ 0x00656a10).
    clock: f64,
    next_tick: f64,
    last_tick: f64,
    zbuf: Vec<u32>,
    /// The CON interpreter when the scripts run the game
    /// ([`Engine::boot`]); taken out while it runs, so that the engine is
    /// its [`crate::script::Host`].
    con: Option<Box<Script>>,
    /// The music player's files and choices (the fades are in `music`).
    player: Player,
    /// The picture of `ViewBigSpr` (@ 0x006d1874).
    picture: Option<SprBank>,
    /// A sound loaded for the next video (`cine%03d.apc` @ 0x0047a320, or
    /// the `TrajSound` of a move), played on voice 2 when it starts.
    pending_sound: Option<(String, Pcm)>,
    /// `TrajSound`'s file name (@ 0x00673198), until the next move with a
    /// video.
    traj_sound: Option<String>,
    /// The transition sprites (@ 0x00673f40, count @ 0x00673fa4), drawn
    /// over the video's frames.
    spf: Vec<SprBank>,
    /// A deferred change of place (@ 0x00659180 / 0x00659184): its file
    /// and view, made when the move completes; and the view it leaves
    /// (@ 0x00659188).
    deferred_wam: Option<(String, u32)>,
    deferred_old: u32,
    /// The place sounds and their decoded files.
    pub place: crate::place_sound::PlaceSounds,
    place_pcm: places::PlacePcm,
    /// The scripts asked for a place the port does not have
    /// ([`Engine::refuse_place`]), handled once they are back.
    refused_place: Option<String>,
    /// The video end asks the next warp draw for the music at 100 %
    /// (@ 0x00656999, FUN_0040bb90).
    music_after_video: bool,
    /// The place animation hidden while its character speaks
    /// (@ 0x006c2e78): the slot and the names it was found for.
    hidden: Option<(Option<usize>, Vec<String>)>,
    /// A dialog played over a film (`cinema=N,M`, FUN_004801a0).
    film_dialog: Option<Dialog>,
    /// The menus (`AtlMenu_MainInit` @ 0x00485c00), taken out while a
    /// menu frame runs so that the engine can be its host.
    menu: Option<Box<Menu>>,
    /// The loop type a menu returns to (@ 0x006d16e0).
    menu_prev_loop: LoopType,
    /// The player number (@ 0x005dcf70): the saves' file names.
    pub player_number: u8,
    /// The folder of the saves (`save\` in it) and `atlant2.cfg`; none:
    /// nothing is read or written.
    data_dir: Option<std::path::PathBuf>,
    /// The time a save is stamped with (`GetLocalTime`).
    pub wall_clock: fn() -> atlantis2_data::save::SystemTime,
    /// The saved entries Native keeps as bytes (the dowsing rod, the
    /// puzzles, the dialog flag): those of the last load, written back by
    /// the next save.
    save_extra: atlantis2_data::save::SaveData,
    /// The options of `atlant2.cfg` the engine does not use itself (the
    /// screen size of the options page, full screen).
    pub cfg_extra: atlantis2_data::save::Cfg,
    /// The menus asked the program to end.
    quit_requested: bool,
}

/// The music player's own state (@ 0x00673f30, 0x006725a0..0x006725c4):
/// what plays and what the next open does.
#[derive(Clone, Debug, Default)]
struct Player {
    /// `current_music` (@ 0x00673f30), 0 none.
    current: i32,
    /// The mark the next open goes to (−1 a random one, @ 0x006725a0).
    mark: i32,
    /// The streamer: the file, the ring's write offset, the mark played,
    /// the loops left.
    stream: crate::music::Stream,
    /// The `startmusik` list (`Interp_MusikScript` @ 0x006d0444).
    list: Vec<u8>,
}

/// The texel of the cube under a direction: the face (cube order) and the
/// texel (column, row) the warp samples there, a + (b − a)·t of the face's
/// texture coordinates with a = 1/512, b = 511/512 (the cube's corners,
/// @ 0x0045c790).
fn texel_of(dir: [f32; 3]) -> Option<Texel> {
    crate::hd::cube_point(dir.map(f64::from))
        .map(|(face, u, v)| (face, u.floor() as i32, v.floor() as i32))
}

/// The world's objects bank `/set(monde=N)` loads (@ 0x00484df0).
pub fn objects_bank_name(monde: i32) -> Option<&'static str> {
    Some(match monde {
        1 => "objeire.spr",
        2 | 6 => "objyuc.spr",
        3 => "objchine.spr",
        4 => "objshamb.spr",
        5 => "objtibet.spr",
        _ => return None,
    })
}

/// The world's objects bank, logged when missing.
fn load_objects_bank(files: &A2Files, monde: i32) -> Option<SprBank> {
    objects_bank_name(monde).and_then(|n| {
        let bank = files
            .read(Folder::Spr, n)
            .and_then(|d| SprBank::parse(&d).ok());
        if bank.is_none() {
            log::warn!("spr\\{n} missing or unreadable: no object in hand");
        }
        bank
    })
}

/// ARGB to RGB565.
fn rgb565(argb: u32) -> u16 {
    let (r, g, b) = ((argb >> 16) & 0xff, (argb >> 8) & 0xff, argb & 0xff);
    (((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)) as u16
}

impl Engine {
    /// A new game (menu action 10 for a player without a save, @ 0x00486eb0):
    /// chapter 0's `/INIT` chooses the world's disc, the place, the view
    /// and its angles (Tibet, `tibet.wam`, view 2, yaw −90°, pitch 0). The
    /// opening cinematic it then plays, and the rest of the script, are not
    /// run yet: the view is shown at once.
    ///
    /// `variant` is the CPU's cube renderer, `fpu` the x87 control word the
    /// process starts with. With `warpPII` the first frame changes it to
    /// 0x0e7f for good: the first view is entered under `fpu` and every
    /// later turn, entry and frame runs under 0x0e7f, as in the original if
    /// no assembly renderer drew before the first view (not known).
    pub fn new_game(
        mut files: A2Files,
        variant: Variant,
        fpu: Fpu,
        width: u32,
        height: u32,
    ) -> Result<Engine, EngineError> {
        let text = files
            .read(Folder::Scenar, "chapi000.con")
            .ok_or(EngineError::NoStart)?;
        let text: String = text.iter().map(|&b| char::from(b)).collect();
        let start = NewGame::from_chapter0(&text).ok_or(EngineError::NoStart)?;
        log::info!(
            "new game: MONDE {}, {} view {} at ({}, {})",
            start.monde,
            start.wam,
            start.view,
            start.yaw_deg,
            start.pitch_deg
        );
        files.set_monde(start.monde);
        let mut engine = Engine::blank(files, variant, fpu, width, height, start.monde);
        engine.load_world_texts(start.monde);
        let mut wam = load_wam(&engine.files, &start.wam)?;
        wam.apply_defnames(&engine.names);
        engine.wam = wam;
        engine.wam_name = start.wam.clone();
        engine.place_sounds_reset();
        engine.arrival = (
            deg_to_rad(engine.camera.fpu, start.yaw_deg),
            deg_to_rad(engine.camera.fpu, start.pitch_deg),
        );
        engine.enter_view(start.view, engine.arrival.0, engine.arrival.1)?;
        // `/set(vue=N)` ends with the per-view reset.
        engine.per_view_reset();
        Ok(engine)
    }

    /// The game run by the chapter scripts, as the original's start-up
    /// (@ 0x0049d680): the script tables, chapter 0 loaded. Nothing is
    /// shown until [`Engine::start_new_game`] (the menu's new game); the
    /// original's start-up ends with the player menu
    /// ([`Engine::open_player_menu`]) and the intro
    /// ([`Engine::start_intro`]), which the caller starts. The arguments
    /// are [`Engine::new_game`]'s.
    pub fn boot(
        files: A2Files,
        variant: Variant,
        fpu: Fpu,
        width: u32,
        height: u32,
    ) -> Result<Engine, EngineError> {
        let defnames = files
            .read(Folder::Scenar, "defnames.con")
            .ok_or(EngineError::NoStart)?;
        let script = Script::new(&defnames).map_err(EngineError::Script)?;
        // The bag's start-up (@ 0x00474810) loads the Irish objects bank
        // before any world is set.
        let mut engine = Engine::blank(files, variant, fpu, width, height, 1);
        // The start angles of the next view entry are one state with the
        // script's (−9° and 26° in the data section).
        engine.arrival = (script.start_x, script.start_y);
        engine.con = Some(Box::new(script));
        if engine.with_script(|s, h| s.boot(h)) != Some(true) {
            return Err(engine.script_error().unwrap_or(EngineError::NoStart));
        }
        Ok(engine)
    }

    /// The menu's new game (@ 0x00488f12): chapter 0's `/INIT`, which
    /// chooses the world, the place and the first view, then plays the
    /// opening cinematic; its last lines (the music) run when the film
    /// ends, and from then on the warp frames run the scans
    /// ([`crate::script`]).
    pub fn start_new_game(&mut self) -> Result<(), EngineError> {
        self.with_script(|s, h| s.new_game(h));
        match self.script_error() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// The scripts' fatal error, if one stopped them.
    fn script_error(&self) -> Option<EngineError> {
        self.con
            .as_ref()
            .and_then(|s| s.fatal.clone())
            .map(EngineError::Script)
    }

    /// The engine before any place is loaded: the sprite banks of the
    /// interface and of world `monde`'s objects, the sound, no view.
    fn blank(
        files: A2Files,
        variant: Variant,
        fpu: Fpu,
        width: u32,
        height: u32,
        monde: i32,
    ) -> Engine {
        let names = match files.read(Folder::Scenar, "defnames.con") {
            Some(d) => DefNames::parse(&d).unwrap_or_else(|e| {
                log::warn!("{e}");
                DefNames::default()
            }),
            None => {
                log::warn!("scenar\\defnames.con not found");
                DefNames::default()
            }
        };
        let cursor_bank = files
            .read(Folder::Spr, "interfac.spr")
            .and_then(|d| SprBank::parse(&d).ok());
        if cursor_bank.is_none() {
            log::warn!("spr\\interfac.spr missing or unreadable: no cursor");
        }
        let objects_bank = load_objects_bank(&files, monde);
        // The bag's start-up (@ 0x00474810) reads the object names (a
        // missing file is fatal there).
        let object_names = match files.read(Folder::Loc, "Messages.txt") {
            Some(d) => ObjectNames::parse(&d),
            None => {
                log::warn!("loc\\Messages.txt not found: no object names");
                ObjectNames::default()
            }
        };
        let camera = Camera::new(fpu, width, height);
        // The listener table of the place sounds, in the executable.
        let place = crate::place_sound::PlaceSounds::new(
            files
                .install()
                .read("atlantis2.exe")
                .map(|exe| crate::place_sound::Listeners::from_exe(&exe))
                .unwrap_or_default(),
        );
        let mut anim = Anim::new(0);
        anim.cheval = names.id(defnames::OBJETS, b"CHEVAL").ok().map(|i| i as i32);
        Engine {
            files,
            wam: Wam::default(),
            wam_name: String::new(),
            names,
            view: 0,
            faces: Default::default(),
            work: Default::default(),
            renderer: Renderer::new(variant, fpu, width as usize, height as usize),
            display: crate::display::Display::new(width as usize, height as usize),
            hd: Default::default(),
            camera,
            look: Look::default(),
            // GetTickCount() in the original: a port seeds with its clock
            // (`anim.rand`).
            anim,
            loop_type: LoopType::Warp,
            turn: None,
            zoom: Zoom::default(),
            draw_skip: false,
            move_pending: false,
            next_view: None,
            arrival: (0.0, 0.0),
            centre_zone: None,
            sprite_hit: None,
            sprite_picks: true,
            forced_centre: None,
            centre_draws: Vec::new(),
            sprite_state: 0,
            click: false,
            double: false,
            last_click: None,
            escape_down: false,
            ticks: 0,
            spw_flag: false,
            frame_flag: false,
            cursor_flag: false,
            spw_loaded: false,
            souls_candidates: 0,
            souls_loaded: 0,
            video: None,
            intro_playing: false,
            intro_next: 0,
            vars: Vars::default(),
            cursor: Cursor::default(),
            cursor_bank,
            objects_bank,
            subjects_bank: None,
            subject_menu: SubjectMenu::default(),
            centre_marker: true,
            bag: Vec::new(),
            own_bag: Vec::new(),
            monde,
            bar: Bar::default(),
            overlays: Overlays::new(),
            rod: Rod::default(),
            pointer: (0, 0),
            click_aux: 0,
            ui_draws: Vec::new(),
            object_names,
            popup_font: None,
            hand: None,
            trace: false,
            events: Vec::new(),
            dial_script: None,
            dialog: None,
            dialog_prev_loop: LoopType::Warp,
            // No music plays until a script starts one: the fades only run
            // then, as `FUN_004770c0` is refused while stopped.
            music: MusicVolume::new(fpu, crate::sound::DEFAULT_MUSIC_VOLUME, 0, 0),
            cross_sprite: 0,
            blackbird: Blackbird::default(),
            figurine_slots: [0; 6],
            lent_vars: None,
            skull: puzzles::SKULL_START,
            skull_sprite: 0,
            wheel_slots: [0; 5],
            ogham: OGHAM_START,
            ogham_sprite: 0,
            dragon: Dragon::default(),
            enhancements: Enhancements::default(),
            voices: Voices::new(),
            rand: Rand::new(0),
            font: None,
            subtitles: true,
            voice_volume: crate::sound::DEFAULT_VOICE_VOLUME,
            fx_volume: crate::sound::DEFAULT_FX_VOLUME,
            clock: 0.0,
            next_tick: 0.0,
            last_tick: 0.0,
            zbuf: vec![0; (width * height) as usize],
            con: None,
            player: Player::default(),
            picture: None,
            pending_sound: None,
            traj_sound: None,
            spf: Vec::new(),
            deferred_wam: None,
            deferred_old: 0,
            place,
            place_pcm: places::PlacePcm::new(),
            refused_place: None,
            music_after_video: false,
            hidden: None,
            film_dialog: None,
            menu: None,
            menu_prev_loop: LoopType::Warp,
            player_number: 0,
            data_dir: None,
            wall_clock: atlantis2_data::save::SystemTime::default,
            save_extra: atlantis2_data::save::SaveData::default(),
            cfg_extra: atlantis2_data::save::Cfg::default(),
            quit_requested: false,
        }
    }

    /// The per-view reset (@ 0x00483ce0), run by `/set(vue=N)` and in the
    /// arrival frame: `CLICPERSO`, `CLICZONE`, `CLICOBJET` = 0, `VUE` = the
    /// current view, the limits back to ±70° in pitch and none in yaw,
    /// `OBJETENMAIN` = 0 and the hand emptied (@ 0x0047c0c0(0, 0)), the
    /// click flag cleared. `CHAPTER`'s copy, `ABORTDEPART`, `SACADOS` and
    /// `flag` are script state not ported.
    fn per_view_reset(&mut self) {
        self.vars.clicperso = 0;
        self.vars.cliczone = 0;
        self.vars.clicobjet = 0;
        self.vars.vue = self.view;
        self.look.limit_y = DEFAULT_LIMIT_Y;
        self.look.limit_x = NO_LIMIT;
        self.vars.objet_en_main = 0;
        self.hand = None;
        self.click = false;
    }

    fn event(&mut self, e: Event) {
        if self.trace {
            self.events.push(e);
        }
    }

    /// Enters a view of the current place (@ 0x00459fe0): loads its cube,
    /// orients the camera to the arrival angles (radians), stops the turn,
    /// sets the 76° field of view and loads the view's place animations.
    /// The view's limits are reset later, by the per-view reset
    /// ([`Engine::per_view_reset`]).
    pub fn enter_view(&mut self, id: u32, yaw: f32, pitch: f32) -> Result<(), EngineError> {
        let view = self
            .wam
            .view(id)
            .ok_or_else(|| EngineError::NoView(self.wam_name.clone(), id))?;
        let name = view.name.clone();
        // `NextWarpInvalide` = 1 (@ 0x0045a01a): the entry is skipped and
        // the variable becomes 2, under which the warp frames draw nothing
        // (the screen frozen) until a `vue=` or a move to another place
        // clears it. The view is current all the same (its caller made it
        // so), with the old panorama, camera, animations and sounds.
        let invalid = self
            .con
            .as_ref()
            .is_some_and(|s| s.vars[s.keys.next_warp_invalide] == 1);
        if invalid {
            self.set_script_var(|k| k.next_warp_invalide, 2);
            self.view = id;
            self.vars.vue = id;
            self.event(Event::EnterSkipped { view: id });
            return Ok(());
        }
        let (hnm, disc) = self
            .files
            .read_with_disc(Folder::Warp, &name)
            .ok_or_else(|| EngineError::NoWarp(name.clone()))?;
        self.faces = faces_from_hnm(&hnm).map_err(|e| EngineError::BadWarp(name.clone(), e))?;
        self.work = self.faces.clone();
        log::info!("view {id}: warp {name} (disc {disc})");
        self.view = id;
        self.vars.vue = id;
        self.event(Event::Enter { view: id });
        self.camera.enter(yaw, pitch);
        let (w, h) = (self.renderer.width() as u32, self.renderer.height() as u32);
        self.camera.set_fov(WARP_FOV, w, h);
        self.look.stop();
        self.load_animations(id);
        // FUN_0049bd50: the puzzles' set-up of the view.
        self.entry_hooks();
        // FUN_0047ad30: the place sounds of the view.
        self.place_sounds_enter();
        // A slot hidden for a speaker is gone with the old view's slots.
        if let Some((slot, _)) = self.hidden.as_mut() {
            *slot = None;
        }
        self.turn = None;
        Ok(())
    }

    /// The SPW slots of the view (@ 0x00459fe0's loop): every state of
    /// every entity, in file order, shown in this view.
    fn load_animations(&mut self, view: u32) {
        self.anim.clear();
        let mut loads = Vec::new();
        for (ei, e) in self.wam.entities.iter().enumerate() {
            for (si, s) in e
                .state_records()
                .enumerate()
                .filter(|(_, s)| s.view == view)
            {
                let sprite = if e.kind == 2 {
                    self.names.param(defnames::OBJETS, e.con_name.as_bytes())
                } else {
                    0
                };
                loads.push((
                    e.spw_name(&s),
                    e.kind as i32,
                    e.index as i32,
                    sprite,
                    s,
                    (ei, si),
                ));
            }
        }
        for (name, kind, id, sprite, s, record) in loads {
            let spw = self
                .files
                .read(Folder::Spw, &name)
                .and_then(|d| Spw::parse(d).ok());
            self.event(Event::SpwLoad {
                name: name.clone(),
                found: spw.is_some(),
            });
            match spw {
                Some(spw) => {
                    self.spw_loaded = true;
                    let n = self.anim.slots.len();
                    self.anim.load(&name, spw, kind, id, sprite, s);
                    if let Some(slot) = self.anim.slots.get_mut(n) {
                        slot.record = Some(record);
                    }
                    // The horse's frame kept across views (@ 0x0047c764):
                    // a slot whose id is `CHEVAL`'s object index, whatever
                    // its kind, starts at `FRAMECHEVAL`.
                    if Some(id) == self.anim.cheval {
                        let frame = self
                            .con
                            .as_deref()
                            .and_then(|c| c.var("FRAMECHEVAL"))
                            .unwrap_or(0);
                        if let Some(slot) = self.anim.slots.last_mut().filter(|s| s.id == id) {
                            slot.frame = frame;
                        }
                    }
                }
                None => log::warn!("Can't load SPW {name}"),
            }
        }
    }

    /// A mouse move, in mickeys (relative pointer motion): the pointer
    /// and the turn speeds.
    pub fn mouse_motion(&mut self, dx: i32, dy: i32) {
        self.move_pointer(dx, dy);
        self.look.mouse(self.camera.fpu, dx, dy);
    }

    /// A left button press at `now` seconds (the input callback
    /// @ 0x0040c510): a click, and a double click when it follows the
    /// previous one by less than 0.2 s, in every loop (the callback tests
    /// the loop type only for the right button).
    /// During a dialog the click goes to it: the next line (the dialog's
    /// tick reads and clears the flag, @ 0x004801d0).
    pub fn click(&mut self, now: f64) {
        self.double = self.last_click.is_some_and(|t| now - t < 0.2);
        self.last_click = Some(now);
        let double = self.double;
        self.event(Event::Click { double });
        if self.loop_type == LoopType::Menu {
            if let Some(m) = self.menu.as_mut() {
                m.input.click = true;
                m.input.held = true;
            }
            return;
        }
        if let Some(d) = self.dialog.as_mut() {
            d.click();
            return;
        }
        self.click = true;
    }

    /// The 15 Hz task (@ 0x0040caa0) at the current clock: the counter and
    /// the flags of the animations and the cursor, the music fade step, in
    /// the dialog loop the dialog's tick and end, then the script clocks.
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        self.anim.counter = self.ticks;
        self.spw_flag = true;
        self.cursor_flag = true;
        self.frame_flag = true;
        if let Some(m) = self.menu.as_mut() {
            m.tick_flag = true;
        }
        let t = self.clock;
        self.fade_step();
        // The dialog's step and end come before the script clocks
        // (@ 0x0040caeb, 0x0040caf4, then 0x0040caf9..0x0040cb4e): a
        // section resumed by the dialog's end sees the clocks of the
        // previous tick.
        self.dialog_tick(t);
        let last = self.last_tick;
        self.last_tick = t;
        if let Some(s) = self.con.as_mut() {
            s.tick(t, last);
        }
    }

    /// In the dialog loop, the dialog's tick (@ 0x004801d0) and its end
    /// (@ 0x004808a0) when it has no line left.
    fn dialog_tick(&mut self, t: f64) {
        if self.loop_type != LoopType::Dialog {
            return;
        }
        // The field of view takes the screen's size, not the logical one
        // (@ 0x004803e5): see `crate::display`.
        let screen = self.fov_screen();
        let Some(d) = self.dialog.as_mut() else {
            return;
        };
        let line = d.current();
        let mut ctx = TickContext {
            files: &self.files,
            camera: &mut self.camera,
            voices: &self.voices,
            voice_volume: self.voice_volume,
            master: self.music.master,
            clock: t,
            screen,
        };
        let going = d.tick(&mut ctx);
        if going {
            if d.current() != line {
                if let Some(l) = d.current_line() {
                    let id = l.id.clone();
                    self.event(Event::DialogLine(id));
                }
            }
            self.sync_hidden();
        } else {
            self.end_dialog();
        }
    }

    /// The music fade step (@ 0x00476e80) at the clock, its volume given to
    /// the music voice.
    fn fade_step(&mut self) {
        if let Some(att) = self.music.step(self.clock) {
            self.voices.set_attenuation(crate::sound::VOICE_MUSIC, att);
        }
    }

    /// The scheduler's clock, seconds.
    pub fn clock(&self) -> f64 {
        self.clock
    }

    /// Moves the clock to `now` (seconds), running the 15 Hz ticks due
    /// (a stall of more than a second skips the ticks missed).
    pub fn set_clock(&mut self, now: f64) {
        if now - self.next_tick > 1.0 {
            self.next_tick = now;
        }
        while self.next_tick <= now {
            self.clock = self.next_tick;
            self.tick();
            self.next_tick += TICK;
        }
        self.clock = now;
    }

    /// The world's dialog texts (`MONDE`'s world change, @ 0x0047fe50) and
    /// the subtitle font (`AtlFont_MainInit` @ 0x00404140).
    fn load_world_texts(&mut self, monde: i32) {
        self.dial_script = World::of_monde(monde).and_then(|w| {
            DialogScript::load(&self.files, w, self.camera.fpu)
                .map_err(|e| log::warn!("dialog scripts: {e}"))
                .ok()
        });
        if self.font.is_none() {
            let font_txt = self
                .files
                .read(Folder::Loc, "Font.txt")
                .map(|d| FontTxt::parse(&d));
            let load = |id: &str| {
                let f = font_txt.as_ref()?;
                let file = f.font(id)?.file.clone();
                SubtitleFont::named(self.files.read(Folder::Loc, &file)?, f, id)
            };
            self.font = load("AtlFT_Dialog");
            self.popup_font = load("AtlFT_POPUP");
            if self.font.is_none() {
                log::warn!("loc\\Font.txt or its font missing: no subtitles");
            }
        }
    }

    /// `/set(dial=N)`: `AtlDial_start` (@ 0x0047ff60), a running dialog
    /// stopped first; the loop type 2 (the previous one kept); the views'
    /// angles are saved; then the music fades to 16 % (`SetVariables`
    /// @ 0x004818f0, at 0x004826ce: after the start, whether it worked or
    /// not; a failed start first resets the dialog flag and asks for
    /// 100 %, @ 0x00480098).
    pub fn start_dialog(&mut self, number: u32) -> Result<(), DialogError> {
        if self.dialog.is_some() {
            log::warn!("SetVariables : AtlDial_start : a dialog is running");
            self.stop_dialog();
        }
        let out = self.dial_open(number);
        self.music.set_percent(16);
        out
    }

    /// `AtlDial_start` (@ 0x0047ff60) itself: the dialog loop, or the
    /// failure's reset (the music asked back to 100 %).
    pub(crate) fn dial_open(&mut self, number: u32) -> Result<(), DialogError> {
        self.music.in_dialog = true;
        // Without the world's dialog scripts the initialisation fails as a
        // missing dialog does (@ 0x00480098): the same reset follows.
        let result = match self.dial_script.as_ref() {
            Some(script) => Dialog::initialise(
                number,
                script,
                &self.files,
                &mut self.rand,
                self.renderer.variant,
                self.camera.fpu,
            ),
            None => Err(DialogError::NoFile("the world's dialog scripts".into())),
        };
        let out = match result {
            Ok(mut d) => {
                let (y, p) = self.camera.angles();
                d.saved_yaw = y;
                d.saved_pitch = p;
                self.dialog = Some(d);
                if self.loop_type != LoopType::Dialog {
                    self.dialog_prev_loop = self.loop_type;
                }
                self.set_loop_type(LoopType::Dialog);
                self.dialog_mode(number);
                Ok(())
            }
            Err(e) => {
                self.music.in_dialog = false;
                self.music.set_percent(100);
                // AtlDial_start set the loop type 2, then the previous one
                // back (@ 0x0047ff7a, 0x0047ffd6): the camera set again
                // for it (nothing changes at 640 x 480).
                self.set_loop_type(self.loop_type);
                Err(e)
            }
        };
        self.event(Event::DialogStart {
            number,
            started: out.is_ok(),
        });
        out
    }

    /// The end of the dialog (@ 0x004808a0): the music back to 100 %, the
    /// previous loop type back, then the script section the dialog
    /// interrupted goes on (`CLICPERSO` = 0, `SUJET` = −2 first).
    pub fn end_dialog(&mut self) {
        let running = self.dialog.is_some();
        self.stop_dialog();
        if running {
            self.with_script(|s, h| s.dialog_ended(h));
        }
    }

    /// The end of the dialog without the script's part (a dialog stopped
    /// by another `dial`, whose section is the one running).
    pub(crate) fn stop_dialog(&mut self) {
        // The screen's size (@ 0x0048096e).
        let screen = self.fov_screen();
        if let Some(mut d) = self.dialog.take() {
            d.end(&mut self.camera, &self.voices, screen);
            self.set_loop_type(self.dialog_prev_loop);
            self.event(Event::DialogEnd);
        }
        self.sync_hidden();
        self.music.in_dialog = false;
        self.music.set_percent(100);
    }

    /// The running dialog.
    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }

    pub fn dialog_mut(&mut self) -> Option<&mut Dialog> {
        self.dialog.as_mut()
    }

    /// The world's dialog scripts.
    pub fn script(&self) -> Option<&DialogScript> {
        self.dial_script.as_ref()
    }

    /// The subtitle font.
    pub fn subtitle_font(&self) -> Option<&SubtitleFont> {
        self.font.as_ref()
    }

    /// The sound voices, for the sound output to mix.
    pub fn voices(&self) -> &Voices {
        &self.voices
    }

    /// [`Engine::pass`] on the surface the frame is drawn on (the logical
    /// size, [`Engine::width`] x [`Engine::height`]).
    pub(crate) fn pass_surface(&mut self, mut out: Option<&mut [u16]>) {
        self.centre_draws.clear();
        self.ui_draws.clear();
        self.hd_pass_start();
        // The music state machine (@ 0x00476fc0, called @ 0x0049ce32).
        self.music_frame();
        match self.loop_type {
            LoopType::Warp => self.warp_pass(out.as_deref_mut()),
            LoopType::Video => self.video_pass(out.as_deref_mut()),
            LoopType::Dialog => {
                self.frame();
                if let Some(out) = out.as_deref_mut() {
                    self.render_surface(out);
                }
            }
            LoopType::Menu => self.menu_pass(out.as_deref_mut()),
            LoopType::Picture => self.picture_pass(out.as_deref_mut()),
        }
        self.hd_pass_end(out.as_deref());
        // Every case of the dispatcher ends with the 15 Hz flag cleared
        // (@ 0x0049ce56 .. 0x0049cf29).
        self.frame_flag = false;
    }

    /// `ViewBigSpr` (FUN_0048c9d0): loop type 11 (the previous one is not
    /// kept: the end goes back to the panorama); a bank still loaded is
    /// freed with the original's warning; the bank named by the value up
    /// to `)`, a CR or 63 characters, loaded from `spr\` (FUN_0047ba80).
    /// The section goes on at once.
    pub(crate) fn open_picture(&mut self, value: &str) {
        self.set_loop_type(LoopType::Picture);
        if self.picture.take().is_some() {
            log::warn!("ViewBigSpr_Start() : bank non NULL");
        }
        let name: String = value
            .chars()
            .take_while(|&c| !matches!(c, ')' | '\r' | '\0'))
            .take(63)
            .collect();
        self.picture = self.load_bank(name.as_bytes());
        let found = self.picture.is_some();
        self.event(Event::Picture { name, found });
    }

    /// The picture loop (FUN_0048ca50): the panorama as FUN_0045a930
    /// draws it (the cube and the place animations, their step), frame 0
    /// of the picture at the screen centre (`AtlSpr_Aff` mode 0, its hot
    /// spot there), the flip; a click goes back to the panorama (loop type
    /// 0), the bank freed, the click cleared, the turn speeds stopped
    /// (FUN_0040c680).
    fn picture_pass(&mut self, out: Option<&mut [u16]>) {
        if let Some(out) = out {
            let (w, h) = (self.renderer.width(), self.renderer.height());
            self.draw_scene(out);
            if let Some(bank) = &self.picture {
                bank.draw(
                    &mut Target::screen(out, w, h),
                    Some(w as i32 / 2),
                    h as i32 / 2,
                    0,
                );
            }
        } else if self.renderer.variant == Variant::PII && !self.faces[0].is_empty() {
            self.camera.fpu = Fpu::Chop;
            self.renderer.fpu = Fpu::Chop;
        }
        self.anim_tick();
        if self.click {
            self.set_loop_type(LoopType::Warp);
            self.picture = None;
            self.click = false;
            self.look.stop();
            self.event(Event::PictureEnd);
        }
    }

    /// The picture of `ViewBigSpr` shown, if any.
    pub fn picture(&self) -> Option<&SprBank> {
        self.picture.as_ref()
    }

    /// The cube with the place animations drawn on its faces, nothing
    /// picked (FUN_0045a930's drawing).
    fn draw_scene(&mut self, out: &mut [u16]) {
        if self.faces[0].is_empty() {
            out.fill(0);
            return;
        }
        let centre = texel_of(self.camera.ray(self.camera.cx, self.camera.cy));
        let (dirty, _) = self.anim.draw(&mut self.work, centre, false);
        let (m, p) = (self.camera.matrix(), self.camera.projection());
        self.renderer.fpu = self.camera.fpu;
        self.renderer.render(&self.work, &m, p, out);
        self.camera.fpu = self.renderer.fpu;
        self.hd_scene(out, &m, p);
        restore(&mut self.work, &self.faces, &dirty);
    }

    /// The place animations' 15 Hz step (FUN_0047d7f0, on its flag), the
    /// horse's frame kept in `FRAMECHEVAL`.
    fn anim_tick(&mut self) {
        if self.spw_flag && self.spw_loaded {
            self.spw_flag = false;
            if let Some(frame) = self.anim.step() {
                if let Some(s) = self.con.as_mut() {
                    s.set_var("FRAMECHEVAL", frame);
                }
            }
        }
    }

    /// The dialog loop's turn (@ 0x00480840): only a `voixoff` line turns;
    /// outside a dialog, the mouse look.
    pub fn frame(&mut self) {
        if self.dialog.as_ref().is_none_or(|d| d.kind() == 1) {
            self.look.frame(&mut self.camera);
        }
    }

    /// `AtlWarp_continue` (@ 0x0045a3b0).
    fn warp_pass(&mut self, mut out: Option<&mut [u16]>) {
        // 1. The turn step, or the mouse inertia and the limits
        // (@ 0x0040c6a0); then the zone under the centre.
        if let Some(turn) = &mut self.turn {
            let screen = (self.renderer.width() as u32, self.renderer.height() as u32);
            let step = turn.step(&mut self.camera, &mut self.zoom, screen);
            if step.skip_draw {
                self.draw_skip = true;
            }
            if !step.going {
                self.turn = None;
                self.move_pending = true;
            }
        } else {
            // The limits are the script's `LIMITEANGLEWARPX` / `Y`, read
            // every frame. Not while the bag bar, the subject menu or a
            // full-screen overlay is open (nor a dialog, the parchment or
            // the pause, which the port does not have here).
            if let Some(s) = &self.con {
                self.look.limit_x = s.vars[s.keys.limiteanglewarpx];
                self.look.limit_y = s.vars[s.keys.limiteanglewarpy];
            }
            if !self.bar.open && !self.subject_menu.open && !self.overlays.cover {
                self.look.frame(&mut self.camera);
            }
        }
        self.centre_zone = self.pick_centre().map(|i| (self.view, i));
        // The overlays' clicks (@ 0x0047efd0).
        if self.overlays.any() {
            self.overlay_clicks();
        }
        // 2. The draw (@ 0x0045a9f0); after a video, the music back to
        // 100 % first (FUN_0040bb90).
        if std::mem::take(&mut self.music_after_video) {
            self.music.set_percent(100);
        }
        // Not while `NextWarpInvalide` is 2 (the screen frozen) nor in the
        // frame after a zoom's end (@ 0x0045a3fd..0x0045a41a); the flag is
        // set back either way.
        let frozen = self
            .con
            .as_ref()
            .is_some_and(|s| s.vars[s.keys.next_warp_invalide] == 2);
        if !frozen && !self.draw_skip {
            self.draw(out.as_deref_mut());
        }
        self.draw_skip = false;
        // 3. The clicks and the scans, unless turning.
        if self.turn.is_none() {
            self.sprite_click();
            self.zone_click();
            if self.con.is_some() {
                self.gere_frame();
            } else if self.vars.arrivee_vue != 0 {
                // Interp_GereFrame (@ 0x00484b70): the per-view reset
                // (@ 0x00483ce0, which also drops this frame's click),
                // the hand gets `Objetenmain_NextWarp` (a script key, 0
                // without the scripts), the arrival scan, then
                // `ARRIVEEVUE` = `CLICZONE` = 0.
                self.per_view_reset();
                let vars = self.scan_vars(0);
                self.event(Event::ArrivalScan(vars));
                self.vars.arrivee_vue = 0;
                self.vars.cliczone = 0;
            }
            self.click_scan();
            // FUN_0047b390: the place sounds.
            self.place_sounds_frame();
        }
        // 4. The click flags.
        self.click = false;
        self.click_aux = 0;
        // 5. The move completes.
        if self.move_pending {
            self.complete_move();
        }
        // AtlHnm_start shows the video's first frame at once (a transition,
        // or a film a scan started).
        if let (Some(_), Some(out)) = (&self.video, out) {
            self.draw_video(out);
            self.hd_video();
        }
        // The puzzles' per-frame step (FUN_0049bfb0, @ 0x0045a5e8): every
        // warp frame while the puzzle mode is not 0, turning or not.
        if self.puzzle_mode() != 0 {
            let _ = self.puzzle_tick();
        }
        // 6.
        self.vars.cliczone = 0;
        if let Some(s) = self.con.as_mut() {
            s.vars[s.keys.cliczone] = 0;
        }
    }

    /// The zone under the screen centre (@ 0x00458fe0).
    fn pick_centre(&self) -> Option<usize> {
        let view = self.wam.view(self.view)?;
        let d = self
            .camera
            .ray(self.camera.cx, self.camera.cy)
            .map(f64::from);
        wam::pick(&view.zones, d)
    }

    /// The warp draw (@ 0x0045a9f0): sprites into the faces, the cube, the
    /// faces restored, the animation step; the cursor and the centre
    /// marker; the cursor chosen for the next frame.
    fn draw(&mut self, mut out: Option<&mut [u16]>) {
        if self.faces[0].is_empty() {
            // No view entered yet (a script that has not reached `vue=`).
            if let Some(out) = out {
                out.fill(0);
            }
            return;
        }
        let centre = self
            .forced_centre
            .or_else(|| texel_of(self.camera.ray(self.camera.cx, self.camera.cy)));
        let (dirty, mut hit): (Vec<(usize, FaceBox)>, _) =
            self.anim.draw(&mut self.work, centre, self.click);
        if !self.sprite_picks {
            hit = Default::default();
        }
        self.sprite_state = hit.cursor;
        self.sprite_hit = hit.slot;
        let (w, h) = (self.renderer.width(), self.renderer.height());
        // FUN_0045af10 draws nothing while the bar is open, only the
        // marker while the subject menu is.
        self.centre_draws = if self.bar.open {
            Vec::new()
        } else {
            self.centre_plan(w, h)
        };
        if let Some(out) = out.as_deref_mut() {
            let (m, p) = (self.camera.matrix(), self.camera.projection());
            self.renderer.fpu = self.camera.fpu;
            self.renderer.render(&self.work, &m, p, out);
            self.camera.fpu = self.renderer.fpu;
            self.hd_scene(out, &m, p);
            self.draw_centre(out, w, h);
        } else if self.renderer.variant == Variant::PII {
            // warpPII leaves 0x0e7f whether or not the pixels are wanted.
            self.camera.fpu = Fpu::Chop;
            self.renderer.fpu = Fpu::Chop;
        }
        restore(&mut self.work, &self.faces, &dirty);
        self.anim_tick();
        // The cursor for the next frame (after FUN_0045af10 drew this one),
        // unless the bar, the subject menu (or a dialog) is open.
        let zone_kind = self.centre().map(|(_, z)| z.kind);
        if !self.bar.open
            && !self.subject_menu.open
            && self.dialog.is_none()
            && !self.blackbird.flag
        {
            if self.sprite_state != 0 {
                self.cursor
                    .choose(0, self.sprite_state, &mut self.cursor_flag);
            } else if let Some(kind) = zone_kind {
                self.cursor.choose(kind, 0, &mut self.cursor_flag);
            } else {
                self.cursor.hide();
            }
        }
        // FUN_00475320: the dowsing rod, the overlays (the parchment never
        // opens); then the bar, the blackbird game's menu in its mode 2
        // (@ 0x0045aac4) or the subject menu, and the pointer.
        self.rod_frame();
        if self.overlays.any() {
            self.overlay_draw();
        }
        if self.bar.open && !self.subject_menu.open && self.dialog.is_none() {
            self.bag_frame();
        } else if self.blackbird.mode == 2 {
            self.blackbird_menu_draw();
            self.pointer_draw();
        } else if self.subject_menu.open {
            self.subjects_draw();
        }
        if let Some(out) = out {
            self.paint_ui(out, w, h);
        }
        // After the picture, with the menu open, the blackbird's song under
        // the pointer, then the subject menu answers this frame's click
        // (@ 0x0045ab69..0x0045ac72).
        if self.subject_menu.open {
            self.blackbird_hover();
            self.subjects_click();
        }
    }

    /// FUN_0045af10: at the screen centre (FUN_0047bfd0) the object in
    /// hand, then the cursor over it; then the 5-pixel cross of the
    /// centre marker in the colour of what is under the centre (the zone
    /// type, or a sprite).
    fn centre_plan(&self, w: usize, h: usize) -> Vec<CentreDraw> {
        let (cx, cy) = (w as i32 / 2, h as i32 / 2);
        let mut plan = Vec::new();
        // FUN_0047bfd0, not while the subject menu is open.
        if let Some((_, sprite)) = self.hand.filter(|_| !self.subject_menu.open) {
            if sprite >= 0 {
                plan.push(CentreDraw::Sprite {
                    bank: CursorBank::Objects,
                    frame: sprite,
                    x: cx,
                    y: cy,
                });
            }
        }
        if let Some(frame) = self.cursor.shown.filter(|_| !self.subject_menu.open) {
            plan.push(CentreDraw::Sprite {
                bank: self.cursor.bank,
                frame,
                x: cx,
                y: cy,
            });
        }
        if self.centre_marker {
            let mut argb = match self.centre().map(|(_, z)| z.kind) {
                Some(1) => 0xff00_00ff,
                Some(2) => 0xffff_0000,
                Some(k) if k > 3 && k < 1000 => 0xffff_00ff,
                _ => 0xffff_ffff,
            };
            if self.sprite_state != 0 {
                argb = 0xff00_ff00;
            }
            plan.push(CentreDraw::Line {
                from: (cx, cy - 2),
                to: (cx, cy + 2),
                argb,
            });
            plan.push(CentreDraw::Line {
                from: (cx - 2, cy),
                to: (cx + 2, cy),
                argb,
            });
        }
        plan
    }

    /// Draws [`Engine::centre_draws`].
    fn draw_centre(&self, out: &mut [u16], w: usize, h: usize) {
        for d in &self.centre_draws {
            match *d {
                CentreDraw::Sprite { bank, frame, x, y } => {
                    let bank = match bank {
                        CursorBank::Interface => &self.cursor_bank,
                        CursorBank::Objects => &self.objects_bank,
                    };
                    if let (Some(bank), Ok(frame)) = (bank, usize::try_from(frame)) {
                        bank.draw(&mut Target::screen(out, w, h), Some(x), y, frame);
                    }
                }
                CentreDraw::Line { from, to, argb } => {
                    // A horizontal or vertical line, end points included
                    // [INFERRED].
                    let c = rgb565(argb);
                    for x in from.0.min(to.0)..=from.0.max(to.0) {
                        for y in from.1.min(to.1)..=from.1.max(to.1) {
                            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                                out[y as usize * w + x as usize] = c;
                            }
                        }
                    }
                }
            }
        }
    }

    /// The zone the centre pointer designates in the current view.
    fn centre(&self) -> Option<(usize, &Zone)> {
        let (v, i) = self.centre_zone?;
        if v != self.view {
            return None;
        }
        Some((i, self.wam.view(v)?.zones.get(i)?))
    }

    /// The index of the zone the centre pointer designates in the current
    /// view's list, −1 when none (or a zone of another view).
    pub fn centre_index(&self) -> i32 {
        self.centre().map_or(-1, |(i, _)| i as i32)
    }

    fn current_view(&self) -> Option<&View> {
        self.wam.view(self.view)
    }

    /// FUN_0045a650 and FUN_0045a7c0: a click on a sprite.
    fn sprite_click(&mut self) {
        let Some(n) = self.sprite_hit else { return };
        let Some(slot) = self.anim.slots.get(n) else {
            return;
        };
        let (id, click, kind, sprite) = (slot.id, slot.click, slot.kind, slot.object_sprite);
        match click {
            // Not while the bag bar is open.
            1 if self.bar.open => {}
            1 => {
                // An object to take: hidden for good, in the bag, as the
                // cursor (@ 0x0047bef0, until the next cursor choice) and
                // in the hand (@ 0x0047c0c0), its sprite in the world's
                // objects bank.
                self.vars.clicobjet = id;
                self.anim.take(n);
                if !self.bag.contains(&id) {
                    self.bag.push(id);
                }
                if self.con.is_none() && !self.own_bag.iter().any(|e| e.id == id) {
                    let world = if self.monde == 6 { 2 } else { self.monde };
                    self.own_bag.push(BagEntry {
                        sprite: sprite as u8,
                        id,
                        world,
                    });
                    let visible = crate::bag::visible_count(&self.own_bag, self.monde);
                    self.bar.taken(visible);
                }
                self.set_cursor_pair(CursorBank::Objects, sprite);
                self.vars.objet_en_main = id;
                self.hand = Some((id, sprite));
                self.event(Event::Take { id });
                log::info!("Objet pris en spw : {}", self.name_of(1, id));
                // The script's side: `CLICOBJET`, the show / hide table,
                // the bag, `OBJETENMAIN` and the hand.
                self.with_script(|s, h| s.take_object(h, id, sprite as u8));
            }
            3 => {
                self.vars.clicobjet = id;
                self.set_script_var(|k| k.clicobjet, id);
            }
            4 => {
                self.vars.clicperso = id;
                self.set_script_var(|k| k.clicperso, id);
            }
            _ => {}
        }
        if click == 2 {
            self.open_subjects();
            if !self.subject_menu.open {
                let section = if kind == 1 { 0 } else { 1 };
                log::debug!("{}: no subject menu", self.name_of(section, id));
            }
        }
    }

    /// FUN_0045a870, on a click with no subject menu and no bag open:
    /// `CLICPERSO` = the character under the centre (click kind 2), the
    /// click scan, then `CLICPERSO` = `CLICOBJET` = 0, `SUJET` = −2 and the
    /// click cleared.
    fn click_scan(&mut self) {
        if !self.click || self.bar.open || self.subject_menu.open {
            return;
        }
        let mut character = None;
        if let Some(slot) = self.sprite_hit.and_then(|n| self.anim.slots.get(n)) {
            if slot.click == 2 {
                self.vars.clicperso = slot.id;
                character = Some(slot.id);
            }
        }
        let vars = self.scan_vars(0);
        self.event(Event::ClickScan(vars));
        self.with_script(|s, h| s.click_scan(h, character));
        self.vars.clicperso = 0;
        self.vars.clicobjet = 0;
        self.click = false;
    }

    /// The variables a scan sees, with `DEPARTVUE` = `departvue` (set
    /// only around the departure scan).
    fn scan_vars(&self, departvue: i32) -> ScanVars {
        ScanVars {
            departvue,
            arriveevue: self.vars.arrivee_vue,
            cliczone: self.vars.cliczone,
            clicperso: self.vars.clicperso,
            clicobjet: self.vars.clicobjet,
        }
    }

    fn name_of(&self, section: u8, id: i32) -> String {
        let s = if section == 0 {
            defnames::PERSONNAGES
        } else {
            defnames::OBJETS
        };
        self.names.name(s, id as usize).map_or_else(
            || id.to_string(),
            |n| String::from_utf8_lossy(n).into_owned(),
        )
    }

    /// FUN_0049ca80: a click on the zone under the centre. When a zone
    /// was clicked, [`Event::ZoneClick`] notes `CLICZONE` as the handler
    /// leaves it.
    fn zone_click(&mut self) {
        if let Some(zone) = self.zone_click_handler() {
            self.event(Event::ZoneClick {
                zone,
                cliczone: self.vars.cliczone,
            });
        }
    }

    /// FUN_0049ca80's body: the zone clicked, if any.
    fn zone_click_handler(&mut self) -> Option<usize> {
        // In Ireland's view 73 the blackbird game's click (FUN_00495ae0)
        // comes first (@ 0x0049ca8e..0x0049cab3), whatever is open.
        if self.click && self.in_blackbird_view() {
            self.blackbird_click();
        }
        // Not while the subject menu or the bag bar is open or a dialog
        // runs (@ 0x006617b0, 0x00671ef0, 0x006cfb54).
        if !self.click || self.subject_menu.open || self.bar.open || self.dialog.is_some() {
            return None;
        }
        let (zi, zone) = self.centre().map(|(i, z)| (i, z.clone()))?;
        if self.sprite_hit.is_some() {
            self.vars.cliczone = zone.value;
            self.set_script_var(|k| k.cliczone, zone.value);
            self.puzzle_hooks(zi, &zone);
            return Some(zi);
        }
        let target = if zone.kind == 1 || zone.kind == 2 {
            zone.value
        } else {
            0
        };
        let vars = self.scan_vars(target);
        self.event(Event::DepartScan(vars));
        // `CtrlScenarOUT` (@ 0x00483cc0) with `DEPARTVUE` = the target;
        // `ABORTDEPART` (or a section that dropped the click) ends the
        // click there.
        let target = (zone.kind == 1 || zone.kind == 2).then_some(zone.value);
        if self.with_script(|s, h| s.departure_scan(h, target)) == Some(false) {
            return Some(zi);
        }
        if self.puzzle_hooks(zi, &zone) {
            return Some(zi);
        }
        if zone.kind > 2 && zone.kind < 88888 {
            self.vars.cliczone = zone.value;
            self.set_script_var(|k| k.cliczone, zone.value);
        }
        match zone.kind {
            1 => self.move_setup(&zone),
            2 => self.link_setup(&zone),
            _ => {}
        }
        Some(zi)
    }

    /// FUN_0049cbf0: a type-2 zone: the first link of the current place
    /// file's root place from the current view to the zone's value
    /// (@ 0x0049cc0b..0x0049cc18) moves to that view of the link's file
    /// (FUN_0040fc60); none: nothing.
    fn link_setup(&mut self, zone: &Zone) {
        let link = self.wam.places.first().and_then(|p| {
            p.links
                .iter()
                .find(|l| l.from_view == self.view && l.to_view as i32 == zone.value)
                .cloned()
        });
        if let Some(l) = link {
            self.change_place(&l.wam, l.to_view);
        }
    }

    /// FUN_0040fc60(file, N): with a destination from the current view to
    /// N, a deferred change (the file and N kept, the transition sprites
    /// of that pair when the current file has a view N, then the turn as
    /// the destination says, made pending at once when none is needed);
    /// without one, the file is loaded and view N entered at once
    /// (`LASTVUE` = 0, `VUE` = N, no arrival scan, no video).
    fn change_place(&mut self, name: &str, n: u32) {
        if ported_place(name).is_none() {
            log::warn!("{name}: this world is not ported yet, the game stays here");
            self.event(Event::WorldNotPorted(name.to_string()));
            return;
        }
        self.deferred_old = 0;
        let dest = self
            .current_view()
            .and_then(|v| v.destinations.iter().find(|d| d.target == n).cloned());
        self.event(Event::LinkMove {
            wam: name.to_string(),
            view: n,
            deferred: dest.is_some(),
        });
        let Some(d) = dest else {
            // @ 0x0040fc9f..0x0040fcd5.
            self.load_place(name);
            if self.wam.view(n).is_some() {
                if let Some(s) = self.con.as_mut() {
                    s.current_view = None;
                    s.set_current_view(n as i32);
                }
                self.vars.last_vue = 0;
                if let Err(e) = self.enter_view(n, self.arrival.0, self.arrival.1) {
                    log::error!("{e}");
                }
            }
            return;
        };
        // @ 0x0040fce0..0x0040fe43.
        self.deferred_old = self.view;
        if self.wam.view(n).is_some() {
            self.load_transition_sprites(self.view, n);
        }
        self.deferred_wam = Some((name.to_string(), n));
        let none = f32::from_bits(0x4479_c000);
        let pending = if d.angles_as_given != 0 {
            self.arrival = (d.end_yaw, d.end_pitch);
            !self.start_turn(d.start_yaw, d.start_pitch)
        } else {
            let mut start = (none, none);
            if d.start_yaw <= 998.0 {
                start = (d.start_yaw, d.start_pitch);
            } else if let Some((_, z)) = self.centre() {
                start = zone_aim(self.camera.fpu, &z.points);
            }
            self.arrival = if d.end_yaw <= 998.0 {
                (d.end_yaw, d.end_pitch)
            } else {
                start
            };
            if start.0 < 998.0 {
                !self.start_turn(start.0, start.1)
            } else {
                self.turn = None;
                true
            }
        };
        if pending {
            self.move_pending = true;
        }
    }

    /// `AtlWam_Load` (@ 0x0040fee0): the place file read, the entities'
    /// and zones' names resolved; the zone under the centre and the move's
    /// next view forgotten. No view is entered (the scripts' `vue=` or a
    /// move does it; tests use [`Engine::enter_at`]).
    pub fn load_place(&mut self, name: &str) {
        self.event(Event::Wam(name.to_string()));
        match load_wam(&self.files, name) {
            Ok(mut wam) => {
                wam.apply_defnames(&self.names);
                self.wam = wam;
                self.wam_name = name.to_string();
            }
            // Fatal in the original (`Can't load WAM`).
            Err(e) => log::error!("{e}"),
        }
        self.centre_zone = None;
        self.next_view = None;
        // FUN_0047afc0: the place sounds freed.
        self.place_sounds_reset();
    }

    /// FUN_0047c100(old, new): the transition sprites of the move old →
    /// new (a leftover set freed first, with the original's warning): the
    /// first destination of view `old` (of the current place file) to
    /// `new` with a sprite list; for each name not hidden
    /// (FUN_00484d90), the bank `<prefix><old><new>.spr` of the entity of
    /// that name (FUN_00412410), at most 10.
    fn load_transition_sprites(&mut self, old: u32, new: u32) {
        if !self.spf.is_empty() {
            log::warn!("AtlSpr_init_spf() {} spf not cleared", self.spf.len());
            self.spf.clear();
        }
        let names: Vec<String> = self
            .wam
            .view(old)
            .and_then(|v| {
                v.destinations
                    .iter()
                    .find(|d| d.target == new && !d.sprites.is_empty())
            })
            .map(|d| d.sprites.clone())
            .unwrap_or_default();
        for name in names {
            if self.spf_hidden(&name) {
                continue;
            }
            let prefix = self
                .wam
                .entities
                .iter()
                .find(|e| e.con_name == name)
                .map(|e| e.prefix.clone())
                .unwrap_or_default();
            let file = format!("{prefix}{old:03}{new:03}.spr");
            let bank = self
                .files
                .read(Folder::Spf, &file)
                .and_then(|d| SprBank::parse(&d).ok());
            self.event(Event::SpfLoad {
                name: file.clone(),
                found: bank.is_some(),
            });
            let Some(bank) = bank else {
                log::warn!("{file}: no transition sprite");
                continue;
            };
            if self.spf.len() >= 10 {
                // Fatal in the original (`nb_spf>=MAX_SPF`).
                log::error!("AtlSpr_init_spf() : nb_spf>=MAX_SPF!!");
                return;
            }
            self.spf.push(bank);
        }
    }

    /// `Interp_is_perso_hidden` (FUN_00484d90): the name as an object (or
    /// else as a character) hidden by the show / hide table (a state
    /// below 1; no row gives 2).
    fn spf_hidden(&self, name: &str) -> bool {
        let perso = self
            .names
            .id(defnames::PERSONNAGES, name.as_bytes())
            .unwrap_or(0) as i32;
        let objet = self
            .names
            .id(defnames::OBJETS, name.as_bytes())
            .unwrap_or(0) as i32;
        if (perso == 0) == (objet == 0) {
            log::warn!("Interp_is_perso_hidden(): PB avec SPF {name}");
        }
        let (id, kind) = if objet != 0 { (objet, 2) } else { (perso, 1) };
        self.anim.table.get(id, kind) < 1
    }

    /// The video's frame centred on the screen, then frame k of each
    /// transition sprite bank at its default position (FUN_0040bbb0
    /// @ 0x0040bc58 → FUN_0047c200): k the frame counter; a bank shorter
    /// than k, or a sprite larger than 640 x 480, is not drawn
    /// (FUN_0047bc40's checks).
    fn draw_video(&self, out: &mut [u16]) {
        let Some(v) = &self.video else {
            return;
        };
        let (w, h) = (self.renderer.width(), self.renderer.height());
        blit_centred(v.hnm.frame(), v.hnm.width(), v.hnm.height(), out, w, h);
        let k = v.shown as usize;
        let mut t = Target::screen(out, w, h);
        for bank in &self.spf {
            if k >= bank.len() {
                continue;
            }
            match bank.info(k) {
                Some(i) if i.w <= 640 && i.h <= 480 => {
                    bank.draw(&mut t, None, 0, k);
                }
                _ => {}
            }
        }
    }

    /// FUN_0049cc40: the move set-up of a type-1 zone.
    fn move_setup(&mut self, zone: &Zone) {
        let fpu = self.camera.fpu;
        let target = zone.value as u32;
        // `NEXTVUE` ≠ 0: it is reset and the view its `/set` looked up is
        // kept; else the zone's view.
        let next = self.con.as_mut().and_then(|s| {
            let k = s.keys.nextvue;
            (s.vars[k] != 0).then(|| {
                s.vars[k] = 0;
                s.next_view
            })
        });
        self.next_view = match next {
            Some(v) => v.and_then(|v| self.wam.view(v as u32)).map(|v| v.id),
            None => self.wam.view(target).map(|v| v.id),
        };
        let aim = || zone_aim(fpu, &zone.points);
        if self.next_view.is_none() {
            let (y, p) = aim();
            self.start_turn(y, p);
            return;
        }
        let mut start = (999.0f32, f32::from_bits(0x4479_c000));
        let dests: Vec<_> = self
            .current_view()
            .map(|v| {
                v.destinations
                    .iter()
                    .filter(|d| d.target == target)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if !dests.is_empty() {
            for d in &dests {
                if d.angles_as_given == 0 {
                    start = if d.start_yaw <= 998.0 {
                        (d.start_yaw, d.start_pitch)
                    } else {
                        aim()
                    };
                    self.arrival = if d.end_yaw <= 998.0 {
                        (d.end_yaw, d.end_pitch)
                    } else {
                        start
                    };
                    if !self.start_turn(start.0, start.1) {
                        self.move_pending = true;
                    }
                } else {
                    start = (d.start_yaw, d.start_pitch);
                    self.arrival = (d.end_yaw, d.end_pitch);
                    // No turn needed: nothing makes the move pending (a
                    // click that does nothing, kept from the original).
                    self.start_turn(start.0, start.1);
                }
            }
            if start.0 != 999.0 {
                return;
            }
        }
        let (y, p) = aim();
        self.start_turn(y, p);
        self.arrival = (y, p);
    }

    /// FUN_0045b690: sets up a turn; false when none is needed.
    fn start_turn(&mut self, yaw: f32, pitch: f32) -> bool {
        let (turn, turning) = Turn::new(&self.camera, yaw, pitch, &self.zoom);
        self.event(Event::TurnSetup {
            yaw,
            pitch,
            turning,
        });
        self.turn = turning.then_some(turn);
        turning
    }

    /// The end of a move (@ 0x0045a3b0, move pending): the new view
    /// entered at the arrival angles, then its transition video.
    fn complete_move(&mut self) {
        self.move_pending = false;
        // A deferred change of place (@ 0x0045a487..0x0045a4e6): the file
        // loaded, its view N the next one, `NextWarpInvalide` = 0; the old
        // view is then the one kept (@ 0x00659188), the current view
        // being none after the load.
        let mut old_record = true;
        if let Some((name, n)) = self.deferred_wam.take() {
            old_record = false;
            self.load_place(&name);
            self.next_view = self.wam.view(n).map(|v| v.id);
            if self.next_view.is_none() {
                // Fatal in the original.
                log::error!("AtlWarp_continue(): load wam {name} : vue {n} introuvable!");
            }
            self.set_script_var(|k| k.next_warp_invalide, 0);
            if let Some(s) = self.con.as_mut() {
                s.current_view = None;
            }
        }
        let Some(next) = self.next_view.take() else {
            // FUN_0045a630: only `voiles.wam` (Shamballa's fireflies, not
            // ported) does something there.
            log::debug!("move without a next view");
            return;
        };
        let old = if old_record {
            self.view
        } else {
            self.deferred_old
        };
        self.vars.last_vue = if old_record { old } else { 0 };
        self.vars.arrivee_vue = next;
        // `LASTVUE`, `VUE` and `ARRIVEEVUE` for the arrival scan of the next
        // frame.
        if let Some(s) = self.con.as_mut() {
            s.arrive(next as i32);
        }
        if let Err(e) = self.enter_view(next, self.arrival.0, self.arrival.1) {
            log::error!("{e}");
            return;
        }
        // FUN_0047c100 (@ 0x0045a54f), when the old view record is there.
        if old_record {
            self.load_transition_sprites(old, next);
        }
        let dest = self
            .wam
            .places
            .iter()
            .flat_map(|p| p.views.iter())
            .filter(|v| v.id == old)
            .find_map(|v| v.destinations.iter().find(|d| d.target == next))
            .cloned();
        // `UDADA` ≠ 0 skips the video, as a double click does.
        let udada = self.con.as_ref().and_then(|s| s.var("UDADA")).unwrap_or(0);
        if let (Some(d), false, 0) = (&dest, self.double, udada) {
            // FUN_0047a350: a `TrajSound` set before the move plays with
            // the video, the music at 20 %.
            if let Some(name) = self.traj_sound.take() {
                self.music.set_percent(20);
                self.pending_sound = self.load_sound(&name);
            }
            // The name is used as a printf format by the original; when
            // the video cannot start, FUN_0047b280 (@ 0x0045a5b2).
            if !self.start_video(&d.video.clone()) {
                self.place_sounds_after_video();
            }
        }
        if self.sprite_hit.is_none() {
            // FUN_0047bf40: the hand emptied.
            self.drop_hand();
        }
    }

    /// `AtlHnm_start` (@ 0x0040bf60): opens the video, shows its first
    /// frame at once and switches to the video loop; false (a cut) when the
    /// file is on no disc or is not a video.
    fn start_video(&mut self, name: &str) -> bool {
        if self.loop_type == LoopType::Video {
            log::warn!("AtlHnm_start({name}) : deja en AtlMain_loop_HNM");
            return false;
        }
        let opened = self
            .files
            .read(Folder::Hnm, name)
            .and_then(|d| HnmVideo::new(d).ok());
        let Some(mut hnm) = opened else {
            self.event(Event::VideoStart {
                name: name.to_string(),
                frames: None,
            });
            log::info!("{name}: no video, a cut");
            // The music back to 100 % (outside a dialog), the sound loaded
            // for the video dropped.
            if self.dialog.is_none() {
                self.music.set_percent(100);
            }
            self.pending_sound = None;
            return false;
        };
        let frames = hnm.frame_count();
        self.event(Event::VideoStart {
            name: name.to_string(),
            frames: Some(frames),
        });
        // FUN_0047b030: the place sounds' listener steps.
        self.place_sounds_video_start(frames);
        // The sound loaded for it (a film's track, a `TrajSound`) starts
        // on voice 2 (FUN_0047a650(id, 2)) at the effects volume.
        if let Some((sound, pcm)) = self.pending_sound.take() {
            let att = self.fx_attenuation();
            self.voices
                .play(crate::sound::VOICE_EFFECTS, pcm, false, att, 0);
            self.event(Event::Sound(sound));
        }
        let end = !matches!(hnm.next_frame(), Some(Ok(_))) || hnm.frames_decoded() >= frames;
        self.event(Event::VideoFrame(0));
        self.video = Some(Video {
            hnm,
            name: name.to_string(),
            prev: self.loop_type,
            tick: self.ticks,
            end,
            shown: 0,
        });
        self.set_loop_type(LoopType::Video);
        true
    }

    /// `AtlMain_loop_HNM` (@ 0x0040c060): a new frame at each 15 Hz tick;
    /// the end, a click or a double click ends it.
    fn video_pass(&mut self, mut out: Option<&mut [u16]>) {
        let Some(v) = &mut self.video else {
            self.set_loop_type(LoopType::Warp);
            return;
        };
        let new_tick = v.tick != self.ticks;
        let mut shown = None;
        if new_tick {
            v.tick = self.ticks;
            if !v.end {
                match v.hnm.next_frame() {
                    Some(Ok(_)) => {
                        v.shown = v.hnm.frames_decoded() - 1;
                        shown = Some(v.shown);
                    }
                    _ => v.end = true,
                }
                if v.hnm.frames_decoded() >= v.hnm.frame_count() {
                    v.end = true;
                }
            }
        }
        // The original's loop draws on its new ticks only (@ 0x0040c068);
        // the port also redraws the frame between them, the same pixels,
        // except under a film's dialog, which is drawn once per tick.
        let end = v.end;
        if new_tick || self.film_dialog.is_none() {
            if let Some(out) = out.as_deref_mut() {
                self.draw_video(out);
                self.hd_video();
            }
        }
        if let Some(k) = shown {
            self.event(Event::VideoFrame(k));
        }
        // `cinema=N,M`: the dialog's step after each frame (FUN_00480830,
        // FUN_00480840): its lines and subtitles over the film.
        self.film_dialog_step(new_tick, out);
        if !new_tick {
            return;
        }
        if !end && !self.click && !self.double {
            // FUN_0047b090: the place sounds along the video.
            self.place_sounds_video_frame();
            return;
        }
        self.finish_video();
    }

    /// The start-up intro (FUN_0049d9c0), the last step of the original's
    /// start-up after the player menu is built ([`Engine::boot`] then
    /// [`Engine::open_player_menu`]): `intro%02d.hnm` from 00, each with
    /// its `intro%02d.apc` on voice 2, the next one at the end of each
    /// (a click ends one, a double click all: its flag stays set), until a
    /// file the current disc does not have; then the loop type the first
    /// started from (the menu) is back. Played at every start.
    pub fn start_intro(&mut self) {
        self.intro_playing = true;
        let n = self.intro_next;
        // One pass of the music player (the thunk @ 0x0047a280), the
        // sound loaded (FUN_0047a320), `AtlHnm_start`.
        self.music_frame();
        self.pending_sound = self.load_sound(&format!("intro{n:02}.apc"));
        if self.start_video(&format!("intro{n:02}.hnm")) {
            self.intro_next += 1;
        } else {
            self.intro_playing = false;
            self.intro_next = 1;
        }
    }

    /// Whether the start-up intro plays.
    pub fn intro_playing(&self) -> bool {
        self.intro_playing
    }

    /// The video playing: its file name and the frames shown.
    pub fn video_playing(&self) -> Option<(&str, u32)> {
        self.video
            .as_ref()
            .map(|v| (v.name.as_str(), v.hnm.frames_decoded()))
    }

    /// Ends the video at once, as a click does (a testing aid).
    pub fn end_video(&mut self) {
        if self.video.is_some() {
            self.finish_video();
        }
    }

    /// The end of the video loop (@ 0x0040c060, FUN_0040bae0): the click
    /// flag cleared (not the double click's, @ 0x0040c0c7), the loop type
    /// the video started from back.
    fn finish_video(&mut self) {
        self.click = false;
        // FUN_0047c240 (@ 0x0040c0e1): the transition sprites freed.
        self.spf.clear();
        let prev = self.video.take().map_or(LoopType::Warp, |v| v.prev);
        self.set_loop_type(prev);
        self.event(Event::VideoEnd);
        // FUN_0040bae0: the video's sound stops; the next warp draw asks
        // for the music at 100 % unless a dialog runs; a dialog played
        // over the film ends with it (FUN_00480a40).
        self.voices.stop(crate::sound::VOICE_EFFECTS);
        if self.dialog.is_none() {
            self.music_after_video = true;
        }
        // The intro's next file (@ 0x0040c0ee), in place of the place
        // sounds after a film (FUN_0047b280, not ported).
        if self.intro_playing {
            self.start_intro();
        } else {
            // FUN_0047b280: the old view's place sounds dropped.
            self.place_sounds_after_video();
        }
        if let Some(mut d) = self.film_dialog.take() {
            d.remove(&self.voices);
        }
        // The section the film interrupted goes on.
        self.with_script(|s, h| s.video_ended(h));
    }

    /// [`Engine::render`] on the surface the frame is drawn on (the
    /// logical size, [`Engine::width`] x [`Engine::height`]): the
    /// cube with the place animations as FUN_0045a930 draws it (no cursor,
    /// no centre marker), or the video's frame. In a dialog
    /// (@ 0x00480840): nothing before its first line; the panorama for a
    /// head or `voixoff` line, then the dialog frame over it; for
    /// `voixoff0` and `cinema` lines the subtitles over what `out` holds.
    pub fn render_surface(&mut self, out: &mut [u16]) {
        self.hd_pass_start();
        self.render_frame(out);
        self.hd_pass_end(Some(out));
    }

    /// [`Engine::render`]'s drawing.
    fn render_frame(&mut self, out: &mut [u16]) {
        let (w, h) = (self.renderer.width(), self.renderer.height());
        let kind = self.dialog.as_ref().map(Dialog::kind);
        if kind == Some(-1) {
            return;
        }
        if let (Some(_), None) = (&self.video, kind) {
            self.draw_video(out);
            self.hd_video();
            return;
        }
        if self.faces[0].is_empty() {
            out.fill(0);
        } else if matches!(kind, None | Some(0) | Some(1)) {
            self.draw_scene(out);
        }
        let active = self.hd_active();
        if let Some(d) = self.dialog.as_mut() {
            let (hd, texts) = self.hd.frame_parts(active);
            let mut frame = FrameContext {
                clock: self.clock,
                rand: &mut self.rand,
                font: self.font.as_ref(),
                subtitles: self.subtitles,
                screen: (w, h),
                fpu: self.camera.fpu,
                hd,
                texts: Some(texts),
            };
            d.draw(out, &mut self.zbuf, &mut frame);
            let fpu = d.fpu();
            self.set_fpu(fpu);
            self.hd_dialog();
        }
    }

    /// The x87 control word the game's code left (the camera's and the
    /// renderer's copies).
    pub(crate) fn set_fpu(&mut self, fpu: Fpu) {
        self.camera.fpu = fpu;
        self.renderer.fpu = fpu;
    }

    /// The warp frame's click flag (@ 0x006569c9): set by a click, cleared
    /// at the end of the frame that sees it.
    pub fn click_flag(&self) -> bool {
        self.click
    }

    /// The sprite under the centre a click hit (its slot, @ 0x00661640)
    /// and the cursor state of the last draw (@ 0x006c2df4).
    pub fn sprite_under_centre(&self) -> (Option<usize>, i32) {
        (self.sprite_hit, self.sprite_state)
    }

    /// The x87 control word the game's float code runs under now.
    pub fn fpu(&self) -> Fpu {
        self.camera.fpu
    }

    /// The cube renderer.
    pub fn variant(&self) -> Variant {
        self.renderer.variant
    }

    pub fn width(&self) -> usize {
        self.renderer.width()
    }

    pub fn height(&self) -> usize {
        self.renderer.height()
    }

    /// The current place file and view id.
    pub fn place(&self) -> (&str, u32) {
        (&self.wam_name, self.view)
    }

    /// The place file, to change for tests (a synthetic destination).
    #[doc(hidden)]
    pub fn wam_mut(&mut self) -> &mut Wam {
        &mut self.wam
    }

    /// The number of transition sprite banks loaded (@ 0x00673fa4).
    pub fn transition_sprites(&self) -> usize {
        self.spf.len()
    }

    /// The transition sprite banks taken out (to draw a frame without
    /// them) and put back: for tests.
    #[doc(hidden)]
    pub fn take_transition_sprites(&mut self) -> Vec<SprBank> {
        std::mem::take(&mut self.spf)
    }

    #[doc(hidden)]
    pub fn put_transition_sprites(&mut self, banks: Vec<SprBank>) {
        self.spf = banks;
    }

    pub fn wam(&self) -> &Wam {
        &self.wam
    }

    pub fn files(&self) -> &A2Files {
        &self.files
    }

    /// The view's faces in cube order, as loaded (without the sprites).
    pub fn faces(&self) -> &[Vec<u16>; 6] {
        &self.faces
    }

    /// Whether a move's turn is running.
    pub fn turning(&self) -> bool {
        self.turn.is_some()
    }

    /// The angles read back from the camera, in degrees.
    pub fn angles_deg(&self) -> (f32, f32) {
        let (y, p) = self.camera.angles();
        (y.to_degrees(), p.to_degrees())
    }

    /// The click zone under the screen centre (@ 0x0040c6a0 picks it every
    /// frame, @ 0x00458fe0): its index in the view's list, and the zone.
    pub fn centre_zone(&self) -> Option<(usize, &Zone)> {
        let i = self.pick_centre()?;
        Some((i, &self.current_view()?.zones[i]))
    }

    /// A zone's aim (FUN_00459300), radians.
    pub fn zone_aim(&self, view: u32, zone: usize) -> Option<(f32, f32)> {
        let z = self.wam.view(view)?.zones.get(zone)?;
        Some(zone_aim(self.camera.fpu, &z.points))
    }

    /// Enters `view` as a move leaves it (the arrival angles, radians,
    /// then the view entry), outside any move: for tests and tools.
    pub fn enter_at(&mut self, view: u32, yaw: f32, pitch: f32) -> Result<(), EngineError> {
        self.arrival = (yaw, pitch);
        self.vars.last_vue = self.view;
        self.enter_view(view, yaw, pitch)
    }

    /// `RunOneCommand` (@ 0x00481800): a `(KEY=value)` command run by the
    /// scripts' `/set` handler, as the engine's own code runs some (for
    /// tests and tools: an object put in the bag or the hand). Nothing
    /// without the scripts.
    pub fn run_command(&mut self, command: &[u8]) {
        self.with_script(|s, h| s.run_command(h, command));
    }

    /// A click on the sprite of slot `n` of the view, as the warp frame
    /// handles it (@ 0x0045a650): for tests and tools.
    pub fn take_slot(&mut self, n: usize) {
        self.sprite_hit = Some(n);
        self.sprite_click();
    }

    /// Loads the current view's place animations again, as its entry
    /// does before the puzzles' set-up, without that set-up: for tests
    /// and tools (a puzzle hook run alone on the slots as loaded).
    pub fn reload_animations(&mut self) {
        self.load_animations(self.view);
    }

    /// The per-view reset of an arrival frame (@ 0x00483ce0): for tests
    /// and tools.
    pub fn reset_view(&mut self) {
        self.per_view_reset();
        if let Some(s) = self.con.as_mut() {
            s.view_reset();
        }
    }

    /// What the centre routine (FUN_0045af10) would draw now: the hand,
    /// the cursor and the marker.
    pub fn plan_centre(&self) -> Vec<CentreDraw> {
        self.centre_plan(self.width(), self.height())
    }
}

/// A video frame onto the screen, centred (the original's screen and
/// videos are both 640 x 480).
fn blit_centred(src: &[u16], sw: usize, sh: usize, out: &mut [u16], w: usize, h: usize) {
    if sw > w || sh > h {
        for y in 0..h.min(sh) {
            let n = w.min(sw);
            out[y * w..y * w + n].copy_from_slice(&src[y * sw..y * sw + n]);
        }
        return;
    }
    let (ox, oy) = ((w - sw) / 2, (h - sh) / 2);
    for y in 0..sh {
        out[(oy + y) * w + ox..(oy + y) * w + ox + sw].copy_from_slice(&src[y * sw..(y + 1) * sw]);
    }
}

fn load_wam(files: &A2Files, name: &str) -> Result<Wam, EngineError> {
    let data = files
        .read(Folder::Wam, name)
        .ok_or_else(|| EngineError::NoWam(name.to_string()))?;
    Wam::parse(&data).map_err(|e| EngineError::BadWam(name.to_string(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_ranges() {
        let mut c = Cursor::default();
        let mut flag = false;
        // A move zone: frames 1..12; the frame shown, then one on per
        // flag, wrapping.
        c.choose(1, 0, &mut flag);
        assert_eq!(c.shown, Some(1));
        for k in 0..13 {
            flag = true;
            c.choose(1, 0, &mut flag);
            assert_eq!(c.shown, Some(1 + k % 12));
        }
        // A character sprite: 25..39; an object: 13..24.
        c.choose(0, 2, &mut flag);
        assert_eq!((c.start, c.shown), (25, Some(25)));
        c.choose(0, 1, &mut flag);
        assert_eq!((c.start, c.shown), (13, Some(13)));
        // A named zone, then a type-1000 zone: no cursor.
        c.choose(3, 0, &mut flag);
        assert_eq!(c.shown, Some(25));
        c.choose(1000, 0, &mut flag);
        assert_eq!(c.shown, None);
        c.hide();
        assert_eq!((c.shown, c.start), (None, -1));
    }

    #[test]
    fn texels_under_the_centre() {
        assert_eq!(texel_of([0.0, 0.0, 1.0]), Some((0, 256, 256)));
        assert_eq!(texel_of([1.0, 0.0, 0.0]), Some((5, 256, 256)));
        assert_eq!(texel_of([0.0, -1.0, 0.0]), Some((2, 256, 256)));
        // Top-left corner of ft: texel 1.
        assert_eq!(
            texel_of([-1.0, -1.0, 1.0]).map(|t| (t.1, t.2)),
            Some((1, 1))
        );
        assert_eq!(rgb565(0xff00_00ff), 0x001f);
    }
}

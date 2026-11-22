//! What the scripts call out to: the program's C functions as the engine
//! side sees them, after their argument checks.
//!
//! Addresses are atlantis5-dvd-int-3891200's (the international DVD's
//! program; the French DVD's is encrypted and not read).

/// The C functions, in the program's registration order (one
/// `lua_pushstring` / `lua_pushcclosure` / `lua_settable(LUA_GLOBALSINDEX)`
/// per name, @ 0x00404bfc..0x0040522d, then the debug group @ 0x004540b4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Binding {
    VideoPlay,
    AddressGo,
    SpritePlay,
    SpritePlayRebound,
    SpriteStop,
    SpriteGetFrame,
    SpriteRandomize,
    SpriteSetPriority,
    PersistantSpritePlay,
    PersistantSpriteStop,
    PersistantSpriteStopAll,
    CameraLimitYaw,
    CameraLimitPitch,
    CameraLookAt,
    CameraSetFov,
    CameraActivateFx,
    DialogPlay,
    DisplayGetScreenSize,
    MusicPlay,
    MusicStop,
    MusicStopAll,
    MusicFade,
    SoundPlay,
    SoundStop,
    TimerCreate,
    TimerDestroy,
    TimerClearAll,
    TimerSetDuration,
    InitChapter,
    PuzzlePokerStart,
    TerminateDialog,
    ExamineItem,
    GameOver,
    Save,
    Load,
    TerminateGame,
    PrintStack,
    PrintVars,
    PrintMessage,
}

/// Every binding with its global name and handler address, in
/// registration order.
pub const BINDINGS: [(Binding, &str, u32); 39] = [
    (Binding::VideoPlay, "Atl_Video_Play", 0x0040_c990),
    (Binding::AddressGo, "Atl_Address_Go", 0x0040_5390),
    (Binding::SpritePlay, "Atl_Sprite_Play", 0x0040_a0d0),
    (
        Binding::SpritePlayRebound,
        "Atl_Sprite_PlayRebound",
        0x0040_aa00,
    ),
    (Binding::SpriteStop, "Atl_Sprite_Stop", 0x0040_b2e0),
    (Binding::SpriteGetFrame, "Atl_Sprite_GetFrame", 0x0040_b400),
    (
        Binding::SpriteRandomize,
        "Atl_Sprite_Randomize",
        0x0040_b720,
    ),
    (
        Binding::SpriteSetPriority,
        "Atl_Sprite_SetPriority",
        0x0040_bc80,
    ),
    (
        Binding::PersistantSpritePlay,
        "Atl_PersistantSprite_Play",
        0x0040_ba20,
    ),
    (
        Binding::PersistantSpriteStop,
        "Atl_PersistantSprite_Stop",
        0x0040_bdc0,
    ),
    (
        Binding::PersistantSpriteStopAll,
        "Atl_PersistantSprite_StopAll",
        0x0040_bef0,
    ),
    (Binding::CameraLimitYaw, "Atl_Camera_LimitYaw", 0x0040_6b40),
    (
        Binding::CameraLimitPitch,
        "Atl_Camera_LimitPitch",
        0x0040_6d20,
    ),
    (Binding::CameraLookAt, "Atl_Camera_LookAt", 0x0040_6f00),
    (Binding::CameraSetFov, "Atl_Camera_SetFOV", 0x0040_7340),
    (
        Binding::CameraActivateFx,
        "Atl_Camera_ActivateFx",
        0x0040_7450,
    ),
    (Binding::DialogPlay, "Atl_Dialog_Play", 0x0040_7ac0),
    (
        Binding::DisplayGetScreenSize,
        "Atl_Display_GetScreenSize",
        0x0040_8d40,
    ),
    (Binding::MusicPlay, "Atl_Music_Play", 0x0040_9490),
    (Binding::MusicStop, "Atl_Music_Stop", 0x0040_96c0),
    (Binding::MusicStopAll, "Atl_Music_StopAll", 0x0040_9780),
    (Binding::MusicFade, "Atl_Music_Fade", 0x0040_9800),
    (Binding::SoundPlay, "Atl_Sound_Play", 0x0040_9940),
    (Binding::SoundStop, "Atl_Sound_Stop", 0x0040_9cd0),
    (Binding::TimerCreate, "Atl_Timer_Create", 0x0040_c110),
    (Binding::TimerDestroy, "Atl_Timer_Destroy", 0x0040_c3e0),
    (Binding::TimerClearAll, "Atl_Timer_ClearAll", 0x0040_c570),
    (
        Binding::TimerSetDuration,
        "Atl_Timer_SetDuration",
        0x0040_c5b0,
    ),
    (Binding::InitChapter, "Atl_InitChapter", 0x0040_5e60),
    (
        Binding::PuzzlePokerStart,
        "Atl_PuzzlePoker_Start",
        0x0040_5d10,
    ),
    (Binding::TerminateDialog, "Atl_TerminateDialog", 0x0040_5d30),
    (Binding::ExamineItem, "Atl_ExamineItem", 0x0040_5d70),
    (Binding::GameOver, "Atl_GameOver", 0x0040_5b70),
    (Binding::Save, "Atl_Save", 0x0040_8fb0),
    (Binding::Load, "Atl_Load", 0x0040_90e0),
    (Binding::TerminateGame, "Atl_TerminateGame", 0x0040_5fc0),
    (Binding::PrintStack, "Atl_PrintStack", 0x004a_0620),
    (Binding::PrintVars, "Atl_PrintVars", 0x004a_0620),
    (Binding::PrintMessage, "Atl_PrintMessage", 0x004a_0620),
];

impl Binding {
    /// The global's name.
    pub fn name(self) -> &'static str {
        BINDINGS.iter().find(|b| b.0 == self).map_or("?", |b| b.1)
    }

    /// The handler's address.
    pub fn handler(self) -> u32 {
        BINDINGS.iter().find(|b| b.0 == self).map_or(0, |b| b.2)
    }
}

/// An argument as the script passed it (for traces).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Vec<u8>),
    /// A table, function, userdata or thread (by type name).
    Other(&'static str),
}

/// Lua 5.0's `tostring` of a number (`LUA_NUMBER_FMT` "%.14g").
pub fn number_text(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == x.trunc() && x.abs() < 1e15 {
        #[allow(clippy::cast_possible_truncation)]
        return format!("{}", x as i64);
    }
    // %.14g: 14 significant digits, shortest of %e / %f forms.
    let e = format!("{x:.13e}");
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    if !(-4..14).contains(&exp) {
        let mant = mant.trim_end_matches('0').trim_end_matches('.');
        let sign = if exp < 0 { '-' } else { '+' };
        return format!("{mant}e{sign}{:02}", exp.abs());
    }
    #[allow(clippy::cast_sign_loss)]
    let decimals = (13 - exp).max(0) as usize;
    let f = format!("{x:.decimals$}");
    if f.contains('.') {
        f.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        f
    }
}

impl Value {
    /// As the trace prints it: `%q` for strings, `{...}` for tables,
    /// `tostring` otherwise (the recorder of the RE notes' oracle).
    pub fn trace(&self) -> String {
        match self {
            Value::Nil => "nil".into(),
            Value::Bool(b) => b.to_string(),
            Value::Num(x) => number_text(*x),
            Value::Str(s) => {
                let mut o = String::from("\"");
                for &c in s {
                    match c {
                        b'"' => o.push_str("\\\""),
                        b'\\' => o.push_str("\\\\"),
                        b'\n' => o.push_str("\\\n"),
                        b'\r' => o.push_str("\\r"),
                        0 => o.push_str("\\000"),
                        c => o.push(char::from(c)),
                    }
                }
                o.push('"');
                o
            }
            Value::Other("table") => "{...}".into(),
            Value::Other(t) => format!("{t}: ?"),
        }
    }
}

/// What an object is (`atl.objects.<name>.semantic`, @ 0x00402db0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Semantic {
    /// `item` (0).
    Item,
    /// `character` (1).
    Character,
}

/// `Atl_Address_Go`'s transition (@ 0x00405390: the third argument
/// compared with these names; two arguments give `centring`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fx {
    NoFx = 0,
    Centring = 1,
    ZoomAndFade = 2,
}

/// `Atl_Sprite_Play`'s third argument (@ 0x0040a0d0 → @ 0x00448ce0):
/// `"stay"` 0, `"stop"` 1, `"loop"` 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayType {
    Stay = 0,
    Stop = 1,
    Loop = 3,
}

/// `Atl_Camera_ActivateFx`'s effect (@ 0x00407450).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraFx {
    /// `"NoFx"` (one argument).
    NoFx,
    /// `"BoatFx", x, y, z` (four arguments; the vector is not null).
    BoatFx([f32; 3]),
}

/// A limit of `Atl_Camera_LimitYaw` / `LimitPitch`: `(centre, range)`
/// becomes `[centre − range · 0.5, centre + range · 0.5]` (0x00655408 is
/// 0.5), as single floats; `"NO_LIMIT"` clears it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Limit {
    None,
    Range { min: f32, max: f32 },
}

/// A call that passed its handler's checks, with its converted arguments.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    /// `Atl_Video_Play(name)` or `(name, a, b, c, d)` (the four numbers as
    /// `__ftol` gives them: arguments 2..5; the handler keeps 3, 2, 5, 4 in
    /// its rectangle in that order). Blocks the script while it plays.
    VideoPlay {
        name: Vec<u8>,
        rect: Option<[i32; 4]>,
    },
    /// `Atl_Address_Go(scene, view[, fx])`: handled by the game manager
    /// (the transition), then reaches the engine as `LoadView`.
    AddressGo {
        scene: Vec<u8>,
        view: i32,
        fx: Fx,
    },
    /// `Atl_Sprite_Play(object, animation, playtype[, priority])`.
    SpritePlay {
        object: Vec<u8>,
        animation: Vec<u8>,
        play: PlayType,
        priority: i32,
    },
    /// `Atl_Sprite_PlayRebound(object, animation, playtype[, priority[,
    /// first, last]])` (@ 0x0040aa00): the player's mode 2 (`"stop"`) or
    /// 4 (`"loop"`), its frames `first..=last` (@ 0x00448cc0: +0x10,
    /// +0x14; `lua_tonumber` of absent arguments 5 and 6 gives 0).
    SpritePlayRebound {
        object: Vec<u8>,
        animation: Vec<u8>,
        /// `"loop"` (mode 4) rather than `"stop"` (mode 2).
        looped: bool,
        priority: i32,
        first: i32,
        last: i32,
    },
    /// `Atl_Sprite_Stop(object)`.
    SpriteStop {
        object: Vec<u8>,
    },
    /// `Atl_Sprite_GetFrame(object, animation)` (@ 0x0040b400; unused by
    /// the scripts): the frame (+0x18) of the sprite of that file in the
    /// current view, as a number; none (logged) when no sprite plays it.
    SpriteGetFrame {
        object: Vec<u8>,
        animation: Vec<u8>,
    },
    /// `Atl_Sprite_Randomize(object, tick1[, tick2])` (@ 0x0040b720 →
    /// 0x00443380; single floats, `tick2` 0 when absent): the percents of
    /// the `Tick1` and `Tick2` animations the object's sprite takes when
    /// it ends.
    SpriteRandomize {
        object: Vec<u8>,
        tick1: f32,
        tick2: f32,
    },
    /// `Atl_Sprite_SetPriority(object, priority)` (unused by the scripts).
    SpriteSetPriority {
        object: Vec<u8>,
        priority: i32,
    },
    /// `Atl_PersistantSprite_Play(object[, priority])`: the `z1`
    /// animation (0x0066ad34) of the current view.
    PersistantSpritePlay {
        object: Vec<u8>,
        priority: i32,
    },
    PersistantSpriteStop {
        object: Vec<u8>,
    },
    PersistantSpriteStopAll,
    CameraLimitYaw(Limit),
    CameraLimitPitch(Limit),
    /// `Atl_Camera_LookAt(pitch, yaw[, animated])` (single floats).
    /// Animated, it blocks the script while the camera turns.
    CameraLookAt {
        pitch: f32,
        yaw: f32,
        animated: bool,
    },
    /// `Atl_Camera_SetFOV(degrees)`: the engine gets π · deg · (1/180) as
    /// a single float (0x00656248, 0x00655410).
    CameraSetFov {
        degrees: f32,
        radians: f32,
    },
    CameraActivateFx(CameraFx),
    /// `Atl_Dialog_Play(id)` (@ 0x00407ac0; `__ftol` of the number). The
    /// engine checks that the dialog exists (@ 0x00465fb0: not 0, and in
    /// the table; else `LUA_Dialog_Play : Le dialogue %d n'existe pas`,
    /// 0x00669f5c, and nothing) and plays it, the script waiting.
    DialogPlay(i32),
    DisplayGetScreenSize,
    /// `Atl_Music_Play(playlist, channel)`.
    MusicPlay {
        playlist: Vec<u8>,
        channel: i32,
    },
    /// `Atl_Music_Stop(channel, fade)`.
    MusicStop {
        channel: i32,
        fade: f32,
    },
    /// `Atl_Music_StopAll(fade)`.
    MusicStopAll {
        fade: f32,
    },
    /// `Atl_Music_Fade(channel, volume[, time])` (time 0 without a third
    /// argument; a volume outside [0, 1] is logged and still passed).
    MusicFade {
        channel: i32,
        volume: f32,
        time: f32,
    },
    /// `Atl_Sound_Play(name[, volume])`: the file `fx/<name>.ogg`
    /// (0x0066a41c, 0x00669618, 0x0066a420); volume 1 by default, clamped
    /// to [0, 1] with a log line.
    SoundPlay {
        name: Vec<u8>,
        path: Vec<u8>,
        volume: f32,
    },
    /// `Atl_Sound_Stop(name)` (same file name).
    SoundStop {
        name: Vec<u8>,
        path: Vec<u8>,
    },
    /// The game manager handles these itself; the engine sees them for
    /// traces.
    TimerCreate {
        name: Vec<u8>,
        duration: f64,
        repeat: i32,
    },
    TimerDestroy {
        name: Vec<u8>,
    },
    TimerClearAll,
    TimerSetDuration {
        name: Vec<u8>,
        duration: f64,
    },
    InitChapter(u32),
    PuzzlePokerStart,
    TerminateDialog,
    ExamineItem(Vec<u8>),
    GameOver(Vec<u8>),
    Save,
    Load,
    TerminateGame,
    /// `Atl_PrintStack` / `PrintVars` / `PrintMessage` (debug output).
    Print(Binding, Vec<Value>),
}

/// The engine's answer to a call.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// Done; the call returns nothing to the script.
    Done,
    /// The call runs over frames (a film, an animated look): the script
    /// waits until the engine calls [`crate::Game::finish_wait`].
    Wait,
    /// Values returned to the script (`Atl_Display_GetScreenSize`).
    Values(Vec<f64>),
}

/// What the game asks of the engine besides the scripts' calls.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Load and show a view (the transition's middle, @ 0x0042d900 →
    /// 0x0044b590). The engine drops the view's sprites.
    LoadView { scene: Vec<u8>, view: i32 },
    /// The view's field of view during a `zoom_and_fade` (radians, single
    /// float), and the fade amount the step keeps (0..1).
    TransitionFov { radians: f32, fade: f32 },
    /// The transition is over.
    TransitionEnd,
    /// Delete the autosave (`autoslot`, @ 0x0042fc50) before a new game.
    DeleteAutosave,
    /// A click on a character with subjects and an empty hand: the
    /// original opens its subject menu (@ 0x004300d0: mode 9,
    /// @ 0x00410f80(name), state 3). The menu is the dialogs' milestone.
    SubjectMenu { character: Vec<u8> },
}

/// Where a Lua error happened.
#[derive(Clone, Debug, PartialEq)]
pub enum ErrorSite {
    /// `luaL_loadfile` of a script file failed.
    Load(Vec<u8>),
    /// Running a script file (`EvaluateScriptFile` @ 0x00454410).
    File(Vec<u8>),
    /// Running a C-side string (`one_shot_func`, @ 0x00454220).
    String(Vec<u8>),
}

/// Something to log or trace.
#[derive(Clone, Debug, PartialEq)]
pub enum Event<'a> {
    /// A script file starts (its path).
    Run(&'a [u8]),
    /// A script file ends.
    End(&'a [u8]),
    /// A script file is missing: `Impossible de trouver le fichier %s`
    /// (0x0066cc10) in the original's log.
    Missing(&'a [u8]),
    /// A C function is called, with the script's arguments.
    Call(Binding, &'a [Value]),
    /// A handler rejected its arguments (the original logs a usage
    /// message and returns nothing).
    Rejected(Binding, &'static str),
    /// A Lua error. The original throws an exception that ends the game
    /// (`Erreur lors de l'evaluation du script %s.\nErreur LUA : %s`).
    Error(ErrorSite, &'a [u8]),
    /// A note of the game manager (state changes, timers).
    Note(String),
}

/// The engine side.
pub trait Engine {
    /// The bytes of a file the scripts load (`luaL_loadfile`), by the path
    /// the program builds (`./datas/scripts/...`); `None` if missing.
    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>>;

    /// A call that passed its checks.
    fn call(&mut self, call: &Call) -> Reply;

    /// A request of the game manager.
    fn request(&mut self, _request: &Request) {}

    /// Something to log or trace.
    fn event(&mut self, _event: &Event<'_>) {}

    /// The engine's sections of a save, in the writer's order after
    /// `LUA_DB` (@ 0x0043ad90: `CAMERA_MOVER`, `CAMERA`, `SOUND`,
    /// `PERSISTANT_SPRITES`).
    fn save_sections(&mut self) -> Vec<atlantis4_data::xml::Element> {
        Vec::new()
    }

    /// Restores the engine's sections of a save (@ 0x0043b210: camera
    /// mover, camera, then after the timers the persistent sprites and
    /// the sound).
    fn load_sections(&mut self, _root: &atlantis4_data::xml::Element) {}

    /// Writes the save file `saves/<name>` of the player; false when it
    /// could not be written.
    fn write_save(&mut self, _name: &str, _data: &[u8]) -> bool {
        false
    }

    /// Reads the save file `saves/<name>`.
    fn read_save(&mut self, _name: &str) -> Option<Vec<u8>> {
        None
    }

    /// `centring`'s set-up (@ 0x0042d610, fx 1): the camera's mode kept
    /// (returned; the transition's +0x98); with a zone under the centre
    /// (the manager's +0x38) a turn toward it when it is 3° or more away,
    /// with none the free mode (@ 0x0042caa0: speeds 0).
    fn centring_start(&mut self) -> u32 {
        0
    }

    /// One step of the camera (@ 0x0042cf80): true unless a turn goes on.
    fn camera_step(&mut self, _dt: f32) -> bool {
        true
    }

    /// The camera's mode a transition kept, back (@ 0x0042d900's phase 1:
    /// 0 → @ 0x0042caa0, the free mode with its speeds 0; 1 →
    /// @ 0x0042cab0).
    fn camera_restore(&mut self, _mode: u32) {}
}

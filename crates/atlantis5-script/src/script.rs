//! The Lua state: libraries as the program opens them, its `LUA_PATH`,
//! 5.0's `require` and `dofile`, the `atl` database, the 39 C functions
//! with their argument checks, and scripts run as coroutines so that a
//! call that runs over frames (a film, a transition) can wait.
//!
//! Addresses are atlantis5-dvd-int-3891200's.

use std::cell::RefCell;
use std::rc::Rc;

use rilua::stdlib::StdLib;
use rilua::vm::closure::{Closure, RustClosure};
use rilua::vm::gc::arena::GcRef;
use rilua::vm::state::{LuaState, LuaThread, ThreadStatus, MAXCALLS};
use rilua::vm::table::Table;
use rilua::vm::value::{Userdata, Val};
use rilua::{Lua, LuaApiMut, LuaError, LuaResult, RuntimeError};

use crate::engine::{
    Binding, Call, CameraFx, Engine, ErrorSite, Event, Fx, Limit, PlayType, Reply, Semantic, Value,
    BINDINGS,
};

/// The engine as the bindings reach it.
pub type EngineRef = Rc<RefCell<dyn Engine>>;

/// The scripts' folder as the program names it: the path table's entry 6
/// (`FUN_00432190(6)`); `LUA_PATH` is this folder followed by `?;`
/// (@ 0x00405250), and `InitChapter` gets it as `Path`.
pub const SCRIPTS_DIR: &[u8] = b"./datas/scripts/";

/// What a call asked the game manager to do once the running script has
/// stopped at it.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    /// The engine runs the call over frames ([`Reply::Wait`]).
    Engine(Binding),
    /// `Atl_Address_Go`: the transition loop of its handler.
    Transition { scene: Vec<u8>, view: i32, fx: Fx },
}

/// State the bindings and the game manager share.
pub(crate) struct Shared {
    pub engine: EngineRef,
    /// What the last call that waits asked for.
    pub pending: Option<Pending>,
    /// Calls for the game manager (timers, chapter, state requests), in
    /// call order.
    pub manager_calls: Vec<Call>,
    /// The `LUA_DB` of each `Atl_Save` (taken at the call, as the
    /// original writes it inside the handler), in call order.
    pub saved_dbs: Vec<Result<atlantis4_data::xml::Element, String>>,
    /// MSVCRT's `rand` state (`holdrand`), which `lua50.dll`'s
    /// `math.random` uses; 1 until `srand`.
    pub holdrand: u32,
}

pub(crate) type SharedRef = Rc<RefCell<Shared>>;

const SHARED_KEY: &[u8] = b"atlantis5-script.shared";

fn runtime_error(message: String) -> LuaError {
    LuaError::Runtime(RuntimeError::new(message))
}

fn str_val(state: &mut LuaState, s: &[u8]) -> Val {
    Val::Str(state.gc.intern_string(s))
}

fn raw_set(state: &mut LuaState, table: GcRef<Table>, key: Val, value: Val) -> LuaResult<()> {
    let strings = &state.gc.string_arena;
    let t = state
        .gc
        .tables
        .get_mut(table)
        .ok_or_else(|| runtime_error("table not found".into()))?;
    t.raw_set(key, value, strings)
}

fn raw_get(state: &LuaState, table: GcRef<Table>, key: Val) -> Val {
    state
        .gc
        .tables
        .get(table)
        .map_or(Val::Nil, |t| t.get(key, &state.gc.string_arena))
}

fn new_table(state: &mut LuaState) -> GcRef<Table> {
    state.gc.alloc_table(Table::new())
}

fn bytes_of(state: &LuaState, v: Val) -> Option<Vec<u8>> {
    match v {
        Val::Str(r) => state.gc.string_arena.get(r).map(|s| s.data().to_vec()),
        Val::Num(x) => Some(crate::engine::number_text(x).into_bytes()),
        _ => None,
    }
}

fn shared_of(state: &mut LuaState) -> LuaResult<SharedRef> {
    let key = str_val(state, SHARED_KEY);
    if let Val::Userdata(u) = raw_get(state, state.registry, key) {
        if let Some(s) = state
            .gc
            .userdata
            .get(u)
            .and_then(|u| u.downcast_ref::<SharedRef>())
        {
            return Ok(s.clone());
        }
    }
    Err(runtime_error("no game in this state".into()))
}

fn upvalue(state: &LuaState) -> Val {
    let func = state.stack_get(state.call_stack[state.ci].func);
    if let Val::Function(r) = func {
        if let Some(Closure::Rust(rc)) = state.gc.closures.get(r) {
            return rc.upvalues.first().copied().unwrap_or(Val::Nil);
        }
    }
    Val::Nil
}

fn rust_closure(state: &mut LuaState, func: rilua::RustFn, name: &str, upvalue: Val) -> Val {
    let closure = Closure::Rust(RustClosure {
        func,
        upvalues: vec![upvalue],
        name: name.to_string(),
        env: None,
    });
    Val::Function(state.gc.alloc_closure(closure))
}

// ---------------------------------------------------------------------------
// Arguments as lua50.dll's API sees them
// ---------------------------------------------------------------------------

fn nargs(state: &LuaState) -> usize {
    state.top.saturating_sub(state.base)
}

/// Argument `i` (1-based); `None` past `lua_gettop`.
fn arg(state: &LuaState, i: usize) -> Option<Val> {
    let idx = state.base + i - 1;
    (i >= 1 && idx < state.top).then(|| state.stack_get(idx))
}

/// `lua_isstring`: a string or a number.
fn is_string(state: &LuaState, i: usize) -> bool {
    matches!(arg(state, i), Some(Val::Str(_) | Val::Num(_)))
}

/// `lua_isnumber`: a number or a string that converts.
fn is_number(state: &LuaState, i: usize) -> bool {
    match arg(state, i) {
        Some(Val::Num(_)) => true,
        Some(Val::Str(_)) => state.check_number(i).is_ok(),
        _ => false,
    }
}

/// `lua_tostring` (bytes; empty for a non-string).
fn to_string(state: &LuaState, i: usize) -> Vec<u8> {
    arg(state, i)
        .and_then(|v| bytes_of(state, v))
        .unwrap_or_default()
}

/// `lua_tonumber` (0 for a value that does not convert).
fn to_number(state: &LuaState, i: usize) -> f64 {
    match arg(state, i) {
        Some(Val::Num(x)) => x,
        Some(Val::Str(_)) => state.check_number(i).unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `__ftol` of `lua_tonumber` (truncation toward zero).
#[allow(clippy::cast_possible_truncation)]
fn to_int(state: &LuaState, i: usize) -> i32 {
    let x = to_number(state, i);
    if x.is_nan() {
        i32::MIN
    } else {
        x.trunc().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
    }
}

#[allow(clippy::cast_possible_truncation)]
fn to_f32(state: &LuaState, i: usize) -> f32 {
    to_number(state, i) as f32
}

fn value_of(state: &LuaState, v: Val) -> Value {
    match v {
        Val::Nil => Value::Nil,
        Val::Bool(b) => Value::Bool(b),
        Val::Num(x) => Value::Num(x),
        Val::Str(_) => Value::Str(bytes_of(state, v).unwrap_or_default()),
        Val::Table(_) => Value::Other("table"),
        Val::Function(_) => Value::Other("function"),
        Val::Userdata(_) | Val::LightUserdata(_) => Value::Other("userdata"),
        Val::Thread(_) => Value::Other("thread"),
    }
}

fn args_of(state: &LuaState) -> Vec<Value> {
    (1..=nargs(state))
        .map(|i| value_of(state, arg(state, i).unwrap_or(Val::Nil)))
        .collect()
}

// ---------------------------------------------------------------------------
// The 39 C functions
// ---------------------------------------------------------------------------

/// What a handler does with its arguments: a call, or the usage /
/// argument log line it writes instead (the handler then returns 0
/// results).
enum Checked {
    Call(Call),
    Rejected(&'static str),
}

use Checked::{Call as Ok_, Rejected};

/// The checks and conversions of each handler, in the handler's order.
#[allow(clippy::too_many_lines)]
fn check(state: &LuaState, b: Binding) -> (Checked, Vec<&'static str>) {
    let top = nargs(state);
    // Log lines a handler writes before going on anyway.
    let mut notes: Vec<&'static str> = Vec::new();
    let checked = match b {
        // @ 0x0040c990
        Binding::VideoPlay => {
            if top == 1 && is_string(state, 1) {
                Ok_(Call::VideoPlay {
                    name: to_string(state, 1),
                    rect: None,
                })
            } else if top == 5 && is_string(state, 1) {
                // A non-number among 2..5 is logged ("L'argument %d") and
                // the loop stops; the film plays anyway.
                if let Some(_bad) = (2..=5).find(|&i| !is_number(state, i)) {
                    notes.push("Atl_Video_Play: argument is not a number");
                }
                Ok_(Call::VideoPlay {
                    name: to_string(state, 1),
                    rect: Some([
                        to_int(state, 2),
                        to_int(state, 3),
                        to_int(state, 4),
                        to_int(state, 5),
                    ]),
                })
            } else if top == 1 || top == 5 {
                Rejected("Atl_Video_Play: argument 1 is not a string")
            } else {
                Rejected("Atl_Video_Play: usage")
            }
        }
        // @ 0x00405390: a wrong count is logged, then the call goes on
        // with the default transition.
        Binding::AddressGo => {
            let mut fx = Fx::Centring;
            let mut bad = None;
            if top == 3 {
                if !is_string(state, 3) {
                    bad = Some("Atl_Address_Go: argument 3 is not a string");
                } else {
                    fx = match to_string(state, 3).as_slice() {
                        b"no_fx" => Fx::NoFx,
                        b"centring" => Fx::Centring,
                        b"zoom_and_fade" => Fx::ZoomAndFade,
                        _ => {
                            bad = Some("Atl_Address_Go: unknown transition");
                            Fx::NoFx
                        }
                    };
                }
            } else if top != 2 {
                notes.push("Atl_Address_Go: usage");
            }
            if let Some(m) = bad {
                Rejected(m)
            } else if (top == 2 || top == 3) && !is_number(state, 2) {
                Rejected("Atl_Address_Go: argument 2 is not a number")
            } else if (top == 2 || top == 3) && !is_string(state, 1) {
                Rejected("Atl_Address_Go: argument 1 is not a string")
            } else {
                Ok_(Call::AddressGo {
                    scene: to_string(state, 1),
                    view: to_int(state, 2),
                    fx,
                })
            }
        }
        // @ 0x0040a0d0
        Binding::SpritePlay => {
            if top != 3 && top != 4 {
                Rejected("Atl_Sprite_Play: usage")
            } else if top == 4 && !is_number(state, 4) {
                Rejected("Atl_Sprite_Play: argument 4 is not a number")
            } else if !is_string(state, 1) {
                Rejected("Atl_Sprite_Play: argument 1 is not a string")
            } else if !is_string(state, 2) {
                Rejected("Atl_Sprite_Play: argument 2 is not a string")
            } else if !is_string(state, 3) {
                Rejected("Atl_Sprite_Play: argument 3 is not a string")
            } else {
                let play = match to_string(state, 3).as_slice() {
                    b"stop" => Some(PlayType::Stop),
                    b"stay" => Some(PlayType::Stay),
                    b"loop" => Some(PlayType::Loop),
                    _ => None,
                };
                match play {
                    // The handler logs PlayRebound's message here.
                    None => Rejected("Atl_Sprite_Play: bad play type"),
                    Some(play) => Ok_(Call::SpritePlay {
                        object: to_string(state, 1),
                        animation: to_string(state, 2),
                        play,
                        priority: if top == 3 { 0 } else { to_int(state, 4) },
                    }),
                }
            }
        }
        // @ 0x0040aa00: 3, 4 or 6 arguments; checked from the last one
        // down (6, 5, then 4, then 3 and its value, 2, 1).
        Binding::SpritePlayRebound => {
            let looped = to_string(state, 3);
            if top != 3 && top != 4 && top != 6 {
                Rejected("Atl_Sprite_PlayRebound: usage")
            } else if top == 6 && !is_number(state, 6) {
                Rejected("Atl_Sprite_PlayRebound: argument 6 is not a number")
            } else if top == 6 && !is_number(state, 5) {
                Rejected("Atl_Sprite_PlayRebound: argument 5 is not a number")
            } else if top >= 4 && !is_number(state, 4) {
                Rejected("Atl_Sprite_PlayRebound: argument 4 is not a number")
            } else if !is_string(state, 3) {
                Rejected("Atl_Sprite_PlayRebound: argument 3 is not a string")
            } else if looped != b"stop" && looped != b"loop" {
                // 0x0066a5e0: neither "stop" nor "loop".
                Rejected("Atl_Sprite_PlayRebound: argument 3 is not \"stop\" or \"loop\"")
            } else if !is_string(state, 2) {
                Rejected("Atl_Sprite_PlayRebound: argument 2 is not a string")
            } else if !is_string(state, 1) {
                Rejected("Atl_Sprite_PlayRebound: argument 1 is not a string")
            } else {
                Ok_(Call::SpritePlayRebound {
                    object: to_string(state, 1),
                    animation: to_string(state, 2),
                    looped: looped == b"loop",
                    priority: if top == 3 { 0 } else { to_int(state, 4) },
                    // 0x0040afb8..0x0040afdc: `__ftol` of arguments 6 and
                    // 5 (0 when absent) to @ 0x00448cc0(first, last).
                    first: to_int(state, 5),
                    last: to_int(state, 6),
                })
            }
        }
        // @ 0x0040b2e0
        Binding::SpriteStop => {
            if top != 1 {
                Rejected("Atl_Sprite_Stop: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_Sprite_Stop: argument 1 is not a string")
            } else {
                Ok_(Call::SpriteStop {
                    object: to_string(state, 1),
                })
            }
        }
        // @ 0x0040b400
        Binding::SpriteGetFrame => {
            if top != 2 {
                Rejected("Atl_Sprite_GetFrame: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_Sprite_GetFrame: argument 1 is not a string")
            } else if !is_string(state, 2) {
                Rejected("Atl_Sprite_GetFrame: argument 2 is not a string")
            } else {
                Ok_(Call::SpriteGetFrame {
                    object: to_string(state, 1),
                    animation: to_string(state, 2),
                })
            }
        }
        // @ 0x0040b720
        Binding::SpriteRandomize => {
            if top != 2 && top != 3 {
                Rejected("Atl_Sprite_Randomize: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_Sprite_Randomize: argument 1 is not a string")
            } else if !is_number(state, 2) {
                Rejected("Atl_Sprite_Randomize: argument 2 is not a number")
            } else if top == 3 && !is_number(state, 3) {
                Rejected("Atl_Sprite_Randomize: argument 3 is not a number")
            } else {
                // Two arguments: @ 0x00443380(object, tick1, 0).
                Ok_(Call::SpriteRandomize {
                    object: to_string(state, 1),
                    tick1: to_f32(state, 2),
                    tick2: if top == 3 { to_f32(state, 3) } else { 0.0 },
                })
            }
        }
        // @ 0x0040bc80
        Binding::SpriteSetPriority => {
            if top != 2 {
                Rejected("Atl_Sprite_SetPriority: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_Sprite_SetPriority: argument 1 is not a string")
            } else if !is_number(state, 2) {
                Rejected("Atl_Sprite_SetPriority: argument 2 is not a number")
            } else {
                Ok_(Call::SpriteSetPriority {
                    object: to_string(state, 1),
                    priority: to_int(state, 2),
                })
            }
        }
        // @ 0x0040ba20
        Binding::PersistantSpritePlay => {
            if top != 1 && top != 2 {
                Rejected("Atl_PersistantSprite_Play: usage")
            } else if top == 2 && !is_number(state, 2) {
                Rejected("Atl_PersistantSprite_Play: argument 2 is not a number")
            } else if !is_string(state, 1) {
                Rejected("Atl_PersistantSprite_Play: argument 1 is not a string")
            } else {
                Ok_(Call::PersistantSpritePlay {
                    object: to_string(state, 1),
                    priority: if top == 2 { to_int(state, 2) } else { 0 },
                })
            }
        }
        // @ 0x0040bdc0
        Binding::PersistantSpriteStop => {
            if top != 1 {
                Rejected("Atl_PersistantSprite_Stop: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_PersistantSprite_Stop: argument 1 is not a string")
            } else {
                Ok_(Call::PersistantSpriteStop {
                    object: to_string(state, 1),
                })
            }
        }
        // @ 0x0040bef0
        Binding::PersistantSpriteStopAll => {
            if top == 0 {
                Ok_(Call::PersistantSpriteStopAll)
            } else {
                Rejected("Atl_PersistantSprite_StopAll: usage")
            }
        }
        // @ 0x00406b40 / 0x00406d20
        Binding::CameraLimitYaw | Binding::CameraLimitPitch => {
            let limit = if top == 2 {
                if !is_number(state, 1) {
                    Err("Atl_Camera_Limit: argument 1 is not a number")
                } else if !is_number(state, 2) {
                    Err("Atl_Camera_Limit: argument 2 is not a number")
                } else {
                    // (float)(b * 0.5 + (float)a), (float)((float)a - b * 0.5)
                    let a = f64::from(to_f32(state, 1));
                    let r = to_number(state, 2);
                    #[allow(clippy::cast_possible_truncation)]
                    Ok(Limit::Range {
                        min: (a - r * 0.5) as f32,
                        max: (r * 0.5 + a) as f32,
                    })
                }
            } else if top == 1 {
                if !is_string(state, 1) {
                    Err("Atl_Camera_Limit: argument 1 is not a string")
                } else if to_string(state, 1) == b"NO_LIMIT" {
                    Ok(Limit::None)
                } else {
                    Err("Atl_Camera_Limit: bad argument")
                }
            } else {
                Err("Atl_Camera_Limit: bad number of arguments")
            };
            match limit {
                Err(m) => Rejected(m),
                Ok(l) if b == Binding::CameraLimitYaw => Ok_(Call::CameraLimitYaw(l)),
                Ok(l) => Ok_(Call::CameraLimitPitch(l)),
            }
        }
        // @ 0x00406f00
        Binding::CameraLookAt => {
            if top != 2 && top != 3 {
                Rejected("Atl_Camera_LookAt: bad number of arguments")
            } else if top == 3 && !matches!(arg(state, 3), Some(Val::Bool(_))) {
                Rejected("Atl_Camera_LookAt: argument 3 is not a boolean")
            } else if !is_number(state, 1) {
                Rejected("Atl_Camera_LookAt: argument 1 is not a number")
            } else if !is_number(state, 2) {
                Rejected("Atl_Camera_LookAt: argument 2 is not a number")
            } else {
                Ok_(Call::CameraLookAt {
                    pitch: to_f32(state, 1),
                    yaw: to_f32(state, 2),
                    animated: top == 3 && arg(state, 3).is_some_and(Val::is_truthy),
                })
            }
        }
        // @ 0x00407340
        Binding::CameraSetFov => {
            if top != 1 {
                Rejected("Atl_Camera_SetFOV: bad number of arguments")
            } else if !is_number(state, 1) {
                Rejected("Atl_Camera_SetFOV: argument 1 is not a number")
            } else {
                let degrees = to_f32(state, 1);
                let pi = f32::from_bits(0x4049_0fdb);
                let inv180 = f32::from_bits(0x3bb6_0b61);
                Ok_(Call::CameraSetFov {
                    degrees,
                    radians: pi * degrees * inv180,
                })
            }
        }
        // @ 0x00407450
        Binding::CameraActivateFx => {
            if top == 1 {
                if !is_string(state, 1) {
                    Rejected("Atl_Camera_ActivateFx: argument 1 is not a string")
                } else if to_string(state, 1) == b"NoFx" {
                    Ok_(Call::CameraActivateFx(CameraFx::NoFx))
                } else {
                    Rejected("Atl_Camera_ActivateFx: bad type")
                }
            } else if top == 4 {
                if !is_string(state, 1) {
                    Rejected("Atl_Camera_ActivateFx: argument 1 is not a string")
                } else if to_string(state, 1) != b"BoatFx" {
                    Rejected("Atl_Camera_ActivateFx: bad type")
                } else if (2..=4).any(|i| !is_number(state, i)) {
                    Rejected("Atl_Camera_ActivateFx: argument is not a number")
                } else {
                    let v = [to_f32(state, 2), to_f32(state, 3), to_f32(state, 4)];
                    if (v[2] * v[2] + v[1] * v[1] + v[0] * v[0]).sqrt() == 0.0 {
                        Rejected("Atl_Camera_ActivateFx: null vector")
                    } else {
                        Ok_(Call::CameraActivateFx(CameraFx::BoatFx(v)))
                    }
                }
            } else {
                Rejected("Atl_Camera_ActivateFx: bad number of arguments")
            }
        }
        // @ 0x00407ac0: one argument (usage 0x00669fc0), a number
        // (0x00669f8c), `__ftol` of it; whether the dialog exists
        // (@ 0x00465fb0) is the engine's.
        Binding::DialogPlay => {
            if top != 1 {
                Rejected("LUA_Dialog_Play: bad number of arguments")
            } else if !is_number(state, 1) {
                Rejected("LUA_Dialog_Play: argument 1 is not a NUMBER")
            } else {
                Ok_(Call::DialogPlay(to_int(state, 1)))
            }
        }
        // @ 0x00408d40
        Binding::DisplayGetScreenSize => {
            if top == 0 {
                Ok_(Call::DisplayGetScreenSize)
            } else {
                Rejected("Atl_Display_GetScreenSize: usage")
            }
        }
        // @ 0x00409490
        Binding::MusicPlay => {
            if top != 2 {
                Rejected("Atl_Music_Play: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_Music_Play: argument 1 is not a string")
            } else if !is_number(state, 2) {
                Rejected("Atl_Music_Play: argument 2 is not a number")
            } else {
                Ok_(Call::MusicPlay {
                    playlist: to_string(state, 1),
                    channel: to_int(state, 2),
                })
            }
        }
        // @ 0x004096c0
        Binding::MusicStop => {
            if top != 2 {
                Rejected("Atl_Music_Stop: usage")
            } else if !is_number(state, 1) {
                Rejected("Atl_Music_Stop: argument 1 is not a number")
            } else if !is_number(state, 2) {
                Rejected("Atl_Music_Stop: argument 2 is not a number")
            } else {
                Ok_(Call::MusicStop {
                    channel: to_int(state, 1),
                    fade: to_f32(state, 2),
                })
            }
        }
        // @ 0x00409780: checks argument 1, then reads argument 2 (none:
        // 0), so the fade is always 0 with the one argument it accepts.
        Binding::MusicStopAll => {
            if top != 1 {
                Rejected("Atl_Music_StopAll: usage")
            } else if !is_number(state, 1) {
                Rejected("Atl_Music_StopAll: argument 1 is not a number")
            } else {
                Ok_(Call::MusicStopAll {
                    fade: to_f32(state, 2),
                })
            }
        }
        // @ 0x00409800
        Binding::MusicFade => {
            if top != 2 && top != 3 {
                Rejected("Atl_Music_Fade: usage")
            } else if top == 3 && !is_number(state, 3) {
                Rejected("Atl_Music_Fade: argument 3 is not a number")
            } else if !is_number(state, 2) {
                Rejected("Atl_Music_Fade: argument 2 is not a number")
            } else {
                let volume = to_f32(state, 2);
                if !(0.0..=1.0).contains(&volume) {
                    notes.push("Atl_Music_Fade: the volume must be within 0..1");
                }
                if !is_number(state, 1) {
                    Rejected("Atl_Music_Fade: argument 1 is not a number")
                } else {
                    Ok_(Call::MusicFade {
                        channel: to_int(state, 1),
                        volume,
                        time: if top == 3 { to_f32(state, 3) } else { 0.0 },
                    })
                }
            }
        }
        // @ 0x00409940
        Binding::SoundPlay => {
            let mut volume = 1.0f32;
            let mut bad = None;
            if top != 1 && top != 2 {
                bad = Some("Atl_Sound_Play: usage");
            } else if top == 2 {
                if !is_number(state, 2) {
                    bad = Some("Atl_Sound_Play: argument 2 is not a number");
                } else {
                    volume = to_f32(state, 2);
                    if volume >= 0.0 {
                        if volume > 1.0 {
                            notes.push("Atl_Sound_Play: the volume must be at most 1");
                            volume = 1.0;
                        }
                    } else {
                        notes.push("Atl_Sound_Play: the volume must be at least 0");
                        volume = 0.0;
                    }
                }
            }
            if let Some(m) = bad {
                Rejected(m)
            } else if !is_string(state, 1) {
                Rejected("Atl_Sound_Play: argument 1 is not a string")
            } else {
                let name = to_string(state, 1);
                Ok_(Call::SoundPlay {
                    path: sound_path(&name),
                    name,
                    volume,
                })
            }
        }
        // @ 0x00409cd0
        Binding::SoundStop => {
            if top != 1 {
                Rejected("Atl_Sound_Stop: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_Sound_Stop: argument 1 is not a string")
            } else {
                let name = to_string(state, 1);
                Ok_(Call::SoundStop {
                    path: sound_path(&name),
                    name,
                })
            }
        }
        // @ 0x0040c110
        Binding::TimerCreate => {
            if top != 2 && top != 3 {
                Rejected("Atl_Timer_Create: usage")
            } else if top == 3 && !is_number(state, 3) {
                Rejected("Atl_Timer_Create: argument 3 is not a number")
            } else if top == 3 && to_number(state, 3) > 2_147_483_647.0 {
                Rejected("Atl_Timer_Create: argument 3 is too big")
            } else if !is_number(state, 2) {
                Rejected("Atl_Timer_Create: argument 2 is not a number")
            } else if !is_string(state, 1) {
                Rejected("Atl_Timer_Create: argument 1 is not a string")
            } else {
                Ok_(Call::TimerCreate {
                    name: to_string(state, 1),
                    duration: to_number(state, 2),
                    repeat: if top == 3 { to_int(state, 3) } else { -1 },
                })
            }
        }
        // @ 0x0040c3e0
        Binding::TimerDestroy => {
            if top != 1 {
                Rejected("Atl_Timer_Destroy: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_Timer_Destroy: argument 1 is not a string")
            } else {
                Ok_(Call::TimerDestroy {
                    name: to_string(state, 1),
                })
            }
        }
        // @ 0x0040c570
        Binding::TimerClearAll => {
            if top == 0 {
                Ok_(Call::TimerClearAll)
            } else {
                Rejected("Atl_Timer_ClearAll: usage")
            }
        }
        // @ 0x0040c5b0
        Binding::TimerSetDuration => {
            if top != 2 {
                Rejected("Atl_Timer_SetDuration: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_Timer_SetDuration: argument 1 is not a string")
            } else if !is_number(state, 2) {
                Rejected("Atl_Timer_SetDuration: argument 2 is not a number")
            } else {
                Ok_(Call::TimerSetDuration {
                    name: to_string(state, 1),
                    duration: to_number(state, 2),
                })
            }
        }
        // @ 0x00405e60
        Binding::InitChapter => {
            if top != 1 {
                Rejected("Atl_InitChapter: usage")
            } else if !is_number(state, 1) {
                Rejected("Atl_InitChapter: argument 1 is not a number")
            } else {
                #[allow(clippy::cast_sign_loss)]
                Ok_(Call::InitChapter(to_int(state, 1) as u32))
            }
        }
        // @ 0x00405d10 (no check)
        Binding::PuzzlePokerStart => Ok_(Call::PuzzlePokerStart),
        // @ 0x00405d30
        Binding::TerminateDialog => {
            if top == 0 {
                Ok_(Call::TerminateDialog)
            } else {
                Rejected("Atl_TerminateDialog: bad number of arguments")
            }
        }
        // @ 0x00405d70: checks argument 1, converts the top one (-1).
        Binding::ExamineItem => {
            if top != 1 {
                Rejected("Atl_ExamineItem: bad number of arguments")
            } else if !is_string(state, 1) {
                Rejected("Atl_ExamineItem: argument 1 is not a string")
            } else {
                Ok_(Call::ExamineItem(to_string(state, top)))
            }
        }
        // @ 0x00405b70
        Binding::GameOver => {
            if top != 1 {
                Rejected("Atl_GameOver: usage")
            } else if !is_string(state, 1) {
                Rejected("Atl_GameOver: argument 1 is not a string")
            } else {
                Ok_(Call::GameOver(to_string(state, 1)))
            }
        }
        // @ 0x00408fb0, 0x004090e0, 0x00405fc0
        Binding::Save | Binding::Load | Binding::TerminateGame => {
            if top != 0 {
                Rejected("usage: no argument")
            } else {
                Ok_(match b {
                    Binding::Save => Call::Save,
                    Binding::Load => Call::Load,
                    _ => Call::TerminateGame,
                })
            }
        }
        // @ 0x004a0620 [not read: an unreliable address]
        Binding::PrintStack | Binding::PrintVars | Binding::PrintMessage => {
            Ok_(Call::Print(b, args_of(state)))
        }
    };
    (checked, notes)
}

/// `fx/` + name + `.` + `ogg` (0x0066a41c, 0x00669618, 0x0066a420).
pub fn sound_path(name: &[u8]) -> Vec<u8> {
    let mut p = b"fx/".to_vec();
    p.extend_from_slice(name);
    p.extend_from_slice(b".ogg");
    p
}

/// Whether the running code may stop here: inside a script's coroutine,
/// with no Rust frame between (a `dofile` or `require` is one).
fn can_wait(state: &LuaState) -> bool {
    state.current_thread.is_some() && state.n_ccalls == 0
}

/// Every C function: the binding is the closure's upvalue (its index).
fn binding_entry(state: &mut LuaState) -> LuaResult<u32> {
    let index = match upvalue(state) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Val::Num(n) => n as usize,
        _ => return Err(runtime_error("binding without its index".into())),
    };
    let binding = BINDINGS[index].0;
    let shared = shared_of(state)?;
    let engine = shared.borrow().engine.clone();
    let raw = args_of(state);
    engine.borrow_mut().event(&Event::Call(binding, &raw));
    let (checked, notes) = check(state, binding);
    for n in notes {
        engine.borrow_mut().event(&Event::Rejected(binding, n));
    }
    let call = match checked {
        Rejected(m) => {
            engine.borrow_mut().event(&Event::Rejected(binding, m));
            return Ok(0);
        }
        Ok_(c) => c,
    };
    // The game manager's own calls.
    match &call {
        Call::TimerCreate { .. }
        | Call::TimerDestroy { .. }
        | Call::TimerClearAll
        | Call::TimerSetDuration { .. }
        | Call::InitChapter(_)
        | Call::GameOver(_)
        | Call::ExamineItem(_)
        | Call::Load
        | Call::TerminateGame => {
            shared.borrow_mut().manager_calls.push(call.clone());
        }
        Call::Save => {
            // @ 0x00408fb0 → 0x0043ad90: the save is written inside the
            // handler; its `LUA_DB` is the table now.
            let k = str_val(state, b"atl");
            let g = state.global;
            let db = match raw_get(state, g, k) {
                Val::Table(t) => {
                    let mut root = crate::save::element("LUA_DB");
                    write_table(state, t, &mut root, 0).map(|()| root)
                }
                _ => Err(
                    "CSerialisableLuaDB :: Serialize -> impossible de recuperer la table : atl"
                        .into(),
                ),
            };
            let mut sh = shared.borrow_mut();
            sh.saved_dbs.push(db);
            sh.manager_calls.push(call.clone());
        }
        _ => {}
    }
    if let Call::AddressGo { scene, view, fx } = &call {
        shared.borrow_mut().pending = Some(Pending::Transition {
            scene: scene.clone(),
            view: *view,
            fx: *fx,
        });
        return wait(state, &engine, binding);
    }
    let reply = engine.borrow_mut().call(&call);
    match reply {
        Reply::Done => Ok(0),
        Reply::Values(v) => {
            let n = v.len();
            for x in v {
                state.push(Val::Num(x));
            }
            #[allow(clippy::cast_possible_truncation)]
            Ok(n as u32)
        }
        Reply::Wait => {
            shared.borrow_mut().pending = Some(Pending::Engine(binding));
            wait(state, &engine, binding)
        }
    }
}

/// Stops the running script at this call (a yield of no value); the game
/// manager resumes it once the call is over.
fn wait(state: &mut LuaState, engine: &EngineRef, binding: Binding) -> LuaResult<u32> {
    if can_wait(state) {
        Err(LuaError::Yield(0))
    } else {
        // A call reached through a Rust frame (`dofile`, `require`): the
        // game manager runs it once the script has ended.
        engine.borrow_mut().event(&Event::Note(format!(
            "{}: called where the script cannot wait; it runs after the script",
            binding.name()
        )));
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// 5.0's require and dofile (lbaselib.c of Lua 5.0.2, which lua50.dll is)
// ---------------------------------------------------------------------------

/// `luaL_loadfile`: the file through the engine, chunk name `@path`.
fn load_file(state: &mut LuaState, path: &[u8]) -> Result<Val, (bool, Vec<u8>)> {
    let shared = shared_of(state).map_err(|e| (false, e.to_string().into_bytes()))?;
    let engine = shared.borrow().engine.clone();
    let source = engine.borrow_mut().read_script(path);
    let Some(source) = source else {
        let mut m = b"cannot read ".to_vec();
        m.extend_from_slice(path);
        return Err((true, m));
    };
    let name = format!("@{}", String::from_utf8_lossy(path));
    match state.load_bytes(&source, &name) {
        Ok(f) => Ok(Val::Function(f.gc_ref())),
        Err(e) => Err((false, e.to_string().into_bytes())),
    }
}

/// `luaB_dofile`: load, then call with all results.
fn lua_dofile(state: &mut LuaState) -> LuaResult<u32> {
    let path = to_string(state, 1);
    let n = nargs(state);
    let f = load_file(state, &path).map_err(|(_, m)| {
        let v = Val::Str(state.gc.intern_string(&m));
        state.error_object = Some(v);
        runtime_error(String::from_utf8_lossy(&m).into_owned())
    })?;
    let func_pos = state.top;
    state.ensure_stack(1);
    state.stack_set(func_pos, f);
    state.top = func_pos + 1;
    state.call_function(func_pos, -1)?;
    #[allow(clippy::cast_possible_truncation)]
    Ok((state.top - state.base - n) as u32)
}

/// `luaB_require` of Lua 5.0: `_LOADED[name]`, else each `;` part of the
/// global `LUA_PATH` with `?` replaced by the name, the first file that
/// loads is run with `_REQUIREDNAME` set, and its result (true for nil)
/// is kept in `_LOADED`.
fn lua_require(state: &mut LuaState) -> LuaResult<u32> {
    if !is_string(state, 1) {
        return Err(runtime_error(
            "bad argument #1 to `require' (string expected)".into(),
        ));
    }
    let name = to_string(state, 1);
    let name_v = str_val(state, &name);
    let loaded_key = str_val(state, b"_LOADED");
    let Val::Table(loaded) = raw_get(state, state.global, loaded_key) else {
        return Err(runtime_error("`_LOADED' is not a table".into()));
    };
    let done = raw_get(state, loaded, name_v);
    if done.is_truthy() {
        state.push(done);
        return Ok(1);
    }
    let path_key = str_val(state, b"LUA_PATH");
    let path = bytes_of(state, raw_get(state, state.global, path_key))
        .unwrap_or_else(|| b"?;?.lua".to_vec());
    let mut found = None;
    for part in path.split(|&c| c == b';').filter(|p| !p.is_empty()) {
        let mut file = Vec::new();
        for (k, piece) in part.split(|&c| c == b'?').enumerate() {
            if k > 0 {
                file.extend_from_slice(&name);
            }
            file.extend_from_slice(piece);
        }
        match load_file(state, &file) {
            Ok(f) => {
                found = Some(f);
                break;
            }
            Err((true, _)) => {}
            Err((false, m)) => {
                return Err(runtime_error(format!(
                    "error loading package `{}' ({})",
                    String::from_utf8_lossy(&name),
                    String::from_utf8_lossy(&m)
                )));
            }
        }
    }
    let Some(f) = found else {
        return Err(runtime_error(format!(
            "could not load package `{}' from path `{}'",
            String::from_utf8_lossy(&name),
            String::from_utf8_lossy(&path)
        )));
    };
    let req_key = str_val(state, b"_REQUIREDNAME");
    let previous = raw_get(state, state.global, req_key);
    state.settable(Val::Table(state.global), req_key, name_v)?;
    let func_pos = state.top;
    state.ensure_stack(1);
    state.stack_set(func_pos, f);
    state.top = func_pos + 1;
    state.call_function(func_pos, 1)?;
    let mut result = state.stack_get(func_pos);
    state.top = func_pos;
    state.settable(Val::Table(state.global), req_key, previous)?;
    if result.is_nil() {
        result = Val::Bool(true);
    }
    raw_set(state, loaded, name_v, result)?;
    state.push(result);
    Ok(1)
}

// ---------------------------------------------------------------------------
// math.random as lua50.dll has it
// ---------------------------------------------------------------------------

/// MSVCRT's `rand`: `holdrand = holdrand * 214013 + 2531011`, the result
/// `(holdrand >> 16) & 0x7fff` (`RAND_MAX` 0x7fff).
pub fn msvcrt_rand(holdrand: &mut u32) -> u32 {
    *holdrand = holdrand.wrapping_mul(214_013).wrapping_add(2_531_011);
    (*holdrand >> 16) & 0x7fff
}

/// `luaL_checkint`: the number cast to `int`.
#[allow(clippy::cast_possible_truncation)]
fn check_int(state: &LuaState, i: usize, f: &str) -> LuaResult<i32> {
    if !is_number(state, i) {
        return Err(runtime_error(format!(
            "bad argument #{i} to `{f}' (number expected)"
        )));
    }
    Ok(to_number(state, i) as i32)
}

/// Lua 5.0.2's `math_random` on MSVCRT's `rand` (`lua50.dll` imports
/// `rand` and `srand` from MSVCRT.dll): `r = (rand() % RAND_MAX) /
/// RAND_MAX`, then `r`, `floor(r·u)+1` or `floor(r·(u−l+1))+l`.
fn lua_random(state: &mut LuaState) -> LuaResult<u32> {
    let shared = shared_of(state)?;
    let x = msvcrt_rand(&mut shared.borrow_mut().holdrand);
    let r = f64::from(x % 0x7fff) / f64::from(0x7fff);
    let v = match nargs(state) {
        0 => r,
        1 => {
            let u = check_int(state, 1, "random")?;
            if u < 1 {
                return Err(runtime_error(
                    "bad argument #1 to `random' (interval is empty)".into(),
                ));
            }
            (r * f64::from(u)).floor() + 1.0
        }
        2 => {
            let l = check_int(state, 1, "random")?;
            let u = check_int(state, 2, "random")?;
            if l > u {
                return Err(runtime_error(
                    "bad argument #2 to `random' (interval is empty)".into(),
                ));
            }
            (r * (f64::from(u) - f64::from(l) + 1.0)).floor() + f64::from(l)
        }
        _ => return Err(runtime_error("wrong number of arguments".into())),
    };
    state.push(Val::Num(v));
    Ok(1)
}

/// `math_randomseed`: `srand(luaL_checkint(L, 1))`.
fn lua_randomseed(state: &mut LuaState) -> LuaResult<u32> {
    let seed = check_int(state, 1, "randomseed")?;
    #[allow(clippy::cast_sign_loss)]
    {
        shared_of(state)?.borrow_mut().holdrand = seed as u32;
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// The state
// ---------------------------------------------------------------------------

/// How a script run ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// It returned.
    Finished,
    /// It stopped at a call that waits.
    Waiting(Pending),
    /// A Lua error (its message).
    Error(Vec<u8>),
}

/// The Lua state of a game.
pub struct Script {
    lua: Lua,
    pub(crate) shared: SharedRef,
    /// Threads kept from the collector: registry table of running tasks.
    tasks: GcRef<Table>,
    next_task: f64,
}

/// A script run: its coroutine.
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    key: f64,
    thread: GcRef<LuaThread>,
    /// What it runs (for logs and errors).
    pub what: Vec<u8>,
}

impl Script {
    /// `lua_open`, the libraries the program opens (`luaopen_base`,
    /// `luaopen_io`, `luaopen_string`, `luaopen_math`, `luaopen_debug`:
    /// no `table` library), `LUA_PATH` (@ 0x00405250) and the 39 C
    /// functions (@ 0x00404bfc..).
    pub fn new(engine: EngineRef) -> LuaResult<Self> {
        let mut lua = Lua::new_empty();
        let shared = Rc::new(RefCell::new(Shared {
            engine,
            pending: None,
            manager_calls: Vec::new(),
            saved_dbs: Vec::new(),
            holdrand: 1,
        }));
        let tasks;
        {
            let state = lua.state_mut();
            rilua::stdlib::open_libs_selective(
                state,
                StdLib::BASE
                    | StdLib::COROUTINE
                    | StdLib::IO
                    | StdLib::OS
                    | StdLib::STRING
                    | StdLib::MATH
                    | StdLib::DEBUG,
            )?;
            let data: Box<dyn std::any::Any> = Box::new(shared.clone());
            let ud = state.gc.alloc_userdata(Userdata::new(data));
            let key = str_val(state, SHARED_KEY);
            raw_set(state, state.registry, key, Val::Userdata(ud))?;
            tasks = new_table(state);
            let key = str_val(state, b"atlantis5-script.tasks");
            raw_set(state, state.registry, key, Val::Table(tasks))?;
            // 5.0's base library: `_LOADED`, `require`, `dofile`.
            let loaded = new_table(state);
            let key = str_val(state, b"_LOADED");
            raw_set(state, state.global, key, Val::Table(loaded))?;
            for (name, f) in [
                ("require", lua_require as rilua::RustFn),
                ("dofile", lua_dofile as rilua::RustFn),
            ] {
                let v = rust_closure(state, f, name, Val::Nil);
                let key = str_val(state, name.as_bytes());
                raw_set(state, state.global, key, v)?;
            }
            let math_key = str_val(state, b"math");
            if let Val::Table(math) = raw_get(state, state.global, math_key) {
                for (name, f) in [
                    ("random", lua_random as rilua::RustFn),
                    ("randomseed", lua_randomseed as rilua::RustFn),
                ] {
                    let v = rust_closure(state, f, name, Val::Nil);
                    let key = str_val(state, name.as_bytes());
                    raw_set(state, math, key, v)?;
                }
            }
            let mut path = SCRIPTS_DIR.to_vec();
            path.extend_from_slice(b"?;");
            let key = str_val(state, b"LUA_PATH");
            let v = str_val(state, &path);
            state.settable(Val::Table(state.global), key, v)?;
            for (i, (_, name, _)) in BINDINGS.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let f = rust_closure(state, binding_entry, name, Val::Num(i as f64));
                let key = str_val(state, name.as_bytes());
                state.settable(Val::Table(state.global), key, f)?;
            }
        }
        Ok(Self {
            lua,
            shared,
            tasks,
            next_task: 1.0,
        })
    }

    fn engine(&self) -> EngineRef {
        self.shared.borrow().engine.clone()
    }

    fn event(&self, e: &Event<'_>) {
        self.engine().borrow_mut().event(e);
    }

    /// The raw state (tests, tools).
    pub fn lua(&mut self) -> &mut Lua {
        &mut self.lua
    }

    /// Calls the game manager has to handle, taken in call order.
    pub(crate) fn take_manager_calls(&self) -> Vec<Call> {
        std::mem::take(&mut self.shared.borrow_mut().manager_calls)
    }

    /// Takes the `LUA_DB` now, as the next one [`Script::take_saved_db`]
    /// gives (the save menu's save, outside any script).
    pub(crate) fn save_db_now(&mut self) -> Result<(), String> {
        let db = self.lua_db();
        let ok = db.as_ref().map(|_| ()).map_err(Clone::clone);
        self.shared.borrow_mut().saved_dbs.push(db);
        ok
    }

    /// The `LUA_DB` of the oldest `Atl_Save` not yet written.
    pub(crate) fn take_saved_db(&self) -> Option<Result<atlantis4_data::xml::Element, String>> {
        let mut sh = self.shared.borrow_mut();
        (!sh.saved_dbs.is_empty()).then(|| sh.saved_dbs.remove(0))
    }

    fn new_task(&mut self, func: Val, what: &[u8]) -> LuaResult<Task> {
        let state = self.lua.state_mut();
        let thread = LuaThread::new(func, state.global);
        let thread = state.gc.alloc_thread(thread);
        let key = self.next_task;
        self.next_task += 1.0;
        raw_set(state, self.tasks, Val::Num(key), Val::Thread(thread))?;
        Ok(Task {
            key,
            thread,
            what: what.to_vec(),
        })
    }

    /// A task for a script file (`EvaluateScriptFile` @ 0x00454410:
    /// `luaL_loadfile` then `lua_pcall`). `None` when the file is missing
    /// or does not load (reported).
    pub fn file_task(&mut self, path: &[u8]) -> Option<Task> {
        let source = self.engine().borrow_mut().read_script(path);
        let Some(source) = source else {
            self.event(&Event::Missing(path));
            return None;
        };
        let name = format!("@{}", String::from_utf8_lossy(path));
        let f = match self.lua.state_mut().load_bytes(&source, &name) {
            Ok(f) => f,
            Err(e) => {
                self.event(&Event::Error(
                    ErrorSite::Load(path.to_vec()),
                    e.to_string().as_bytes(),
                ));
                return None;
            }
        };
        self.new_task(Val::Function(f.gc_ref()), path).ok()
    }

    /// A task for a C-side string the program runs with `luaL_loadbuffer`
    /// (chunk name `one_shot_func`, @ 0x00454220) that returns a function,
    /// called with `args` (`return InitChapter`).
    pub fn string_task(&mut self, source: &str, args: Vec<Value>) -> Option<(Task, Vec<Value>)> {
        let state = self.lua.state_mut();
        let f = match state.load_bytes(source.as_bytes(), "=one_shot_func") {
            Ok(f) => f,
            Err(e) => {
                self.event(&Event::Error(
                    ErrorSite::String(source.as_bytes().to_vec()),
                    e.to_string().as_bytes(),
                ));
                return None;
            }
        };
        let func_pos = state.top;
        state.ensure_stack(1);
        state.stack_set(func_pos, Val::Function(f.gc_ref()));
        state.top = func_pos + 1;
        let r = state.call_function(func_pos, 1);
        let v = state.stack_get(func_pos);
        state.top = func_pos;
        if r.is_err() || !matches!(v, Val::Function(_)) {
            self.event(&Event::Error(
                ErrorSite::String(source.as_bytes().to_vec()),
                b"not a function",
            ));
            return None;
        }
        let task = self.new_task(v, source.as_bytes()).ok()?;
        Some((task, args))
    }

    fn lua_val(&mut self, v: &Value) -> Val {
        let state = self.lua.state_mut();
        match v {
            Value::Nil | Value::Other(_) => Val::Nil,
            Value::Bool(b) => Val::Bool(*b),
            Value::Num(x) => Val::Num(*x),
            Value::Str(s) => str_val(state, s),
        }
    }

    /// Runs a task until it returns, stops at a call that waits, or fails.
    pub fn resume(&mut self, task: &Task, args: &[Value]) -> Outcome {
        let args: Vec<Val> = args.iter().map(|a| self.lua_val(a)).collect();
        self.shared.borrow_mut().pending = None;
        let state = self.lua.state_mut();
        let saved_top = state.top;
        let saved_base = state.base;
        let saved_ci = state.ci;
        let r = rilua::stdlib::coroutine::auxresume(state, task.thread, &args);
        state.top = saved_top;
        state.base = saved_base;
        state.ci = saved_ci;
        if state.ci < MAXCALLS {
            state.ci_overflow = false;
        }
        let status = state
            .gc
            .threads
            .get(task.thread)
            .map_or(ThreadStatus::Dead, |t| t.status);
        let outcome = match r {
            Err(v) => {
                let msg = bytes_of(state, v).unwrap_or_else(|| b"(non-string error)".to_vec());
                Outcome::Error(msg)
            }
            Ok(_) if status == ThreadStatus::Suspended => {
                match self.shared.borrow_mut().pending.take() {
                    Some(p) => Outcome::Waiting(p),
                    // A script's own coroutine.yield: nothing to wait for.
                    None => Outcome::Waiting(Pending::Engine(Binding::PrintMessage)),
                }
            }
            Ok(_) => Outcome::Finished,
        };
        if !matches!(outcome, Outcome::Waiting(_)) {
            let state = self.lua.state_mut();
            let _ = raw_set(state, self.tasks, Val::Num(task.key), Val::Nil);
        }
        outcome
    }

    // -- the atl table -------------------------------------------------------

    fn global_table(&mut self, path: &[&[u8]]) -> Option<GcRef<Table>> {
        let state = self.lua.state_mut();
        let mut t = state.global;
        for name in path {
            let k = str_val(state, name);
            match raw_get(state, t, k) {
                Val::Table(n) => t = n,
                _ => return None,
            }
        }
        Some(t)
    }

    /// `lua_settable` of a field of an `atl` sub-table.
    fn set_field(&mut self, path: &[&[u8]], key: &[u8], value: &Value) -> bool {
        let Some(t) = self.global_table(path) else {
            return false;
        };
        let v = self.lua_val(value);
        let state = self.lua.state_mut();
        let k = str_val(state, key);
        state.settable(Val::Table(t), k, v).is_ok()
    }

    fn get_field(&mut self, path: &[&[u8]], key: &[u8]) -> Value {
        let Some(t) = self.global_table(path) else {
            return Value::Nil;
        };
        let state = self.lua.state_mut();
        let k = str_val(state, key);
        let v = raw_get(state, t, k);
        value_of(state, v)
    }

    /// `CLuaDatabaseManipulator::CreateAtlantisDatabase` (@ 0x00401010):
    /// the globals `atl_args` and `atl` and `atl`'s tables, in its order.
    pub fn create_database(&mut self) -> LuaResult<()> {
        let state = self.lua.state_mut();
        let g = state.global;
        for name in [&b"atl_args"[..], b"atl"] {
            let t = new_table(state);
            let k = str_val(state, name);
            state.settable(Val::Table(g), k, Val::Table(t))?;
        }
        let mut sub = |path: &[&[u8]], names: &[&[u8]]| -> LuaResult<()> {
            let Some(t) = self.global_table(path) else {
                return Err(runtime_error("atl: missing table".into()));
            };
            let state = self.lua.state_mut();
            for name in names {
                let n = new_table(state);
                let k = str_val(state, name);
                state.settable(Val::Table(t), k, Val::Table(n))?;
            }
            Ok(())
        };
        sub(
            &[b"atl"],
            &[
                b"flags",
                b"puzzle",
                b"address",
                b"previous_address",
                b"zones",
                b"objects",
                b"hero",
                b"event",
            ],
        )?;
        sub(&[b"atl", b"event"], &[b"timer", b"click"])?;
        sub(
            &[b"atl", b"event", b"click"],
            &[b"zone", b"sprite", b"subject", b"inventory"],
        )?;
        sub(&[b"atl", b"hero"], &[b"inventory", b"follower"])?;
        let inv: &[&[u8]] = &[b"atl", b"hero", b"inventory"];
        self.set_field(inv, b"size", &Value::Num(20.0));
        let Some(t) = self.global_table(inv) else {
            return Err(runtime_error("atl: missing table".into()));
        };
        let state = self.lua.state_mut();
        let items = new_table(state);
        let k = str_val(state, b"items");
        state.settable(Val::Table(t), k, Val::Table(items))?;
        self.set_field(inv, b"nb_items", &Value::Num(0.0));
        Ok(())
    }

    /// `CSubjectManager::SetEnteredWarp` (@ 0x00401f20): `atl.address`
    /// takes the new view, `atl.previous_address` the old one (a scene that
    /// is not a string reads as "", a view that is not a number as 0).
    pub fn set_entered_view(&mut self, scene: &[u8], view: i32) {
        let addr: &[&[u8]] = &[b"atl", b"address"];
        let old_scene = match self.get_field(addr, b"scene") {
            Value::Str(s) => s,
            Value::Num(x) => crate::engine::number_text(x).into_bytes(),
            _ => Vec::new(),
        };
        let old_view = match self.get_field(addr, b"view") {
            Value::Num(x) => x,
            _ => 0.0,
        };
        self.set_field(addr, b"scene", &Value::Str(scene.to_vec()));
        self.set_field(addr, b"view", &Value::Num(f64::from(view)));
        let prev: &[&[u8]] = &[b"atl", b"previous_address"];
        self.set_field(prev, b"scene", &Value::Str(old_scene));
        self.set_field(prev, b"view", &Value::Num(old_view));
    }

    /// The event's type (@ 0x004014d0: `atl.event.type`), then the clear
    /// of every event field (@ 0x00401550).
    fn begin_event(&mut self, kind: &[u8]) {
        self.set_field(&[b"atl", b"event"], b"type", &Value::Str(kind.to_vec()));
        let fields: [(&[&[u8]], &[u8]); 8] = [
            (&[b"atl", b"event", b"click", b"zone"], b"id"),
            (&[b"atl", b"event", b"click", b"zone"], b"name"),
            (&[b"atl", b"event", b"click", b"sprite"], b"name"),
            (&[b"atl", b"event", b"click", b"sprite"], b"anim_name"),
            (&[b"atl", b"event", b"click", b"subject"], b"character"),
            (&[b"atl", b"event", b"click", b"subject"], b"name"),
            (&[b"atl", b"event", b"click", b"inventory"], b"num_slot"),
            (&[b"atl", b"event", b"timer"], b"name"),
        ];
        for (path, key) in fields {
            self.set_field(path, key, &Value::Nil);
        }
    }

    /// A zone click (@ 0x004017c0): type `click_zone`, `click.zone.id`,
    /// `click.zone.name` (nil for an empty name).
    pub fn set_click_zone(&mut self, id: i32, name: &[u8]) {
        self.begin_event(b"click_zone");
        let z: &[&[u8]] = &[b"atl", b"event", b"click", b"zone"];
        self.set_field(z, b"id", &Value::Num(f64::from(id)));
        let v = if name.is_empty() {
            Value::Nil
        } else {
            Value::Str(name.to_vec())
        };
        self.set_field(z, b"name", &v);
    }

    /// A sprite click (@ 0x00401a30): type `click_sprite`, the clear,
    /// then `atl.event.click.sprite.name` = the object (part 3 of the
    /// sprite's file name, @ 0x00449f50) and `.anim_name` = the animation
    /// (part 4, @ 0x00449ff0).
    pub fn set_click_sprite(&mut self, name: &[u8], animation: &[u8]) {
        self.begin_event(b"click_sprite");
        let s: &[&[u8]] = &[b"atl", b"event", b"click", b"sprite"];
        self.set_field(s, b"name", &Value::Str(name.to_vec()));
        self.set_field(s, b"anim_name", &Value::Str(animation.to_vec()));
    }

    /// A subject picked in a character's subject menu (@ 0x00401c40):
    /// type `click_subject`, the clear, then
    /// `atl.event.click.subject.character` and `.name`.
    pub fn set_click_subject(&mut self, character: &[u8], subject: &[u8]) {
        self.begin_event(b"click_subject");
        let s: &[&[u8]] = &[b"atl", b"event", b"click", b"subject"];
        self.set_field(s, b"character", &Value::Str(character.to_vec()));
        self.set_field(s, b"name", &Value::Str(subject.to_vec()));
    }

    /// `atl.objects.<name>` (the path walk of @ 0x00405910).
    fn object_field(&mut self, name: &[u8], key: &[u8]) -> Option<Val> {
        let objects = self.global_table(&[b"atl", b"objects"])?;
        let state = self.lua.state_mut();
        let k = str_val(state, name);
        let Val::Table(o) = raw_get(state, objects, k) else {
            return None;
        };
        let k = str_val(state, key);
        match raw_get(state, o, k) {
            Val::Nil => None,
            v => Some(v),
        }
    }

    /// `GetSemantic` (@ 0x00402db0): `atl.objects.<name>.semantic`,
    /// `item` (0x00668900) or `character` (0x0066814c). A missing field, a
    /// value that is not a string or another word throws in the original
    /// (`GetSemantic -> ...`, 0x00668930, 0x00668908, 0x006688dc): the
    /// error's text here.
    pub fn object_semantic(&mut self, name: &[u8]) -> Result<Semantic, String> {
        let lossy = String::from_utf8_lossy(name).into_owned();
        let Some(v) = self.object_field(name, b"semantic") else {
            return Err(format!("GetSemantic: no atl.objects.{lossy}.semantic"));
        };
        let state = self.lua.state_mut();
        let text = match v {
            Val::Str(_) => bytes_of(state, v).unwrap_or_default(),
            Val::Num(x) => crate::engine::number_text(x).into_bytes(),
            _ => return Err(format!("GetSemantic: {lossy} has no string semantic")),
        };
        match text.as_slice() {
            b"item" => Ok(Semantic::Item),
            b"character" => Ok(Semantic::Character),
            _ => Err(format!("GetSemantic: unknown semantic of {lossy}")),
        }
    }

    /// `IsReachable` (@ 0x004030f0): whether `atl.objects.<name>.
    /// warp_reachable` (a table, else the original throws) holds the view
    /// number, its entries walked from 1 to the first nil (one that is not
    /// a number throws).
    pub fn warp_reachable(&mut self, name: &[u8], view: i32) -> Result<bool, String> {
        let lossy = String::from_utf8_lossy(name).into_owned();
        let Some(Val::Table(t)) = self.object_field(name, b"warp_reachable") else {
            return Err(format!(
                "IsReachable: atl.objects.{lossy}.warp_reachable is not a table"
            ));
        };
        let state = self.lua.state_mut();
        let mut found = false;
        let mut i = 1.0;
        loop {
            match raw_get(state, t, Val::Num(i)) {
                Val::Nil => return Ok(found),
                #[allow(clippy::cast_possible_truncation)]
                Val::Num(x) => {
                    if x as i32 == view {
                        found = true;
                    }
                }
                Val::Str(_) => {
                    // lua_isnumber accepts a numeric string.
                    let n = bytes_of(state, raw_get(state, t, Val::Num(i)))
                        .and_then(|b| String::from_utf8(b).ok())
                        .and_then(|s| s.trim().parse::<f64>().ok());
                    match n {
                        #[allow(clippy::cast_possible_truncation)]
                        Some(x) => found |= x as i32 == view,
                        None => return Err(format!("IsReachable: entry {i} of {lossy}")),
                    }
                }
                _ => return Err(format!("IsReachable: entry {i} of {lossy}")),
            }
            i += 1.0;
        }
    }

    /// `GetHandledItem` (@ 0x00403ee0): `atl.hero.handled_item`, empty
    /// when nil; a value that is not a string throws in the original.
    pub fn handled_item(&mut self) -> Result<Vec<u8>, String> {
        match self.get_field(&[b"atl", b"hero"], b"handled_item") {
            Value::Nil => Ok(Vec::new()),
            Value::Str(s) => Ok(s),
            Value::Num(x) => Ok(crate::engine::number_text(x).into_bytes()),
            _ => Err("GetHandledItem: atl.hero.handled_item is not a string".into()),
        }
    }

    /// `GetSubjects` (@ 0x00402540): the scripts' global
    /// `Atl_Character_GetSubjects(name)` under `lua_pcall` (an error, or a
    /// result that is not a table, throws in the original); its entries
    /// from 1 to the first nil, each `{name, state, priority}`: a string,
    /// a boolean and a number, else logged and skipped (0x00668520,
    /// 0x006684ac, 0x00668458). The kept ones' count.
    pub fn character_subjects(&mut self, name: &[u8]) -> Result<usize, String> {
        let state = self.lua.state_mut();
        let g = state.global;
        let k = str_val(state, b"Atl_Character_GetSubjects");
        let f = raw_get(state, g, k);
        let arg = str_val(state, name);
        let func_pos = state.top;
        state.ensure_stack(2);
        state.stack_set(func_pos, f);
        state.stack_set(func_pos + 1, arg);
        state.top = func_pos + 2;
        let r = state.call_function(func_pos, 1);
        let v = state.stack_get(func_pos);
        state.top = func_pos;
        if let Err(e) = r {
            return Err(format!("GetSubjects: {e}"));
        }
        let Val::Table(t) = v else {
            return Err("GetSubjects: the result is not a table".into());
        };
        let mut n = 0;
        let mut i = 1.0;
        loop {
            let e = raw_get(state, t, Val::Num(i));
            let t_e = match e {
                Val::Nil => return Ok(n),
                Val::Table(e) => e,
                // lua_rawgeti on it is outside Lua 5.0's API contract.
                _ => return Err(format!("GetSubjects: entry {i} is not a table")),
            };
            let s = raw_get(state, t_e, Val::Num(1.0));
            let b = raw_get(state, t_e, Val::Num(2.0));
            let p = raw_get(state, t_e, Val::Num(3.0));
            // lua_isnumber: a number or a string that reads as one.
            let numeric = match p {
                Val::Num(_) => true,
                Val::Str(_) => bytes_of(state, p)
                    .and_then(|b| String::from_utf8(b).ok())
                    .is_some_and(|s| s.trim().parse::<f64>().is_ok()),
                _ => false,
            };
            if matches!(s, Val::Str(_) | Val::Num(_)) && matches!(b, Val::Bool(_)) && numeric {
                n += 1;
            }
            i += 1.0;
        }
    }

    /// A timer's end (@ 0x004023d0): type `timer`, `atl.event.timer.name`.
    pub fn set_timer_event(&mut self, name: &[u8]) {
        self.begin_event(b"timer");
        self.set_field(
            &[b"atl", b"event", b"timer"],
            b"name",
            &Value::Str(name.to_vec()),
        );
    }

    /// `atl.zones[name]`: its type (`[1]`: 1 move, 2 action, 3 info) and
    /// whether it is active (`[2]`), as `IsZoneActive` (@ 0x00402990) and
    /// `GetZone` (@ 0x00402ba0) read them. `None` for an unknown zone.
    pub fn zone(&mut self, name: &[u8]) -> Option<(i32, bool)> {
        let zones = self.global_table(&[b"atl", b"zones"])?;
        let state = self.lua.state_mut();
        let k = str_val(state, name);
        let Val::Table(z) = raw_get(state, zones, k) else {
            return None;
        };
        let kind = match raw_get(state, z, Val::Num(1.0)) {
            #[allow(clippy::cast_possible_truncation)]
            Val::Num(x) => x as i32,
            _ => 0,
        };
        let active = matches!(raw_get(state, z, Val::Num(2.0)), Val::Bool(true));
        Some((kind, active))
    }

    /// `atl.address` (scene, view).
    pub fn address(&mut self) -> (Vec<u8>, i32) {
        let a: &[&[u8]] = &[b"atl", b"address"];
        let scene = match self.get_field(a, b"scene") {
            Value::Str(s) => s,
            _ => Vec::new(),
        };
        #[allow(clippy::cast_possible_truncation)]
        let view = match self.get_field(a, b"view") {
            Value::Num(x) => x as i32,
            _ => 0,
        };
        (scene, view)
    }

    /// A global's value (tests).
    pub fn global(&mut self, name: &[u8]) -> Value {
        let state = self.lua.state_mut();
        let k = str_val(state, name);
        let v = raw_get(state, state.global, k);
        value_of(state, v)
    }

    /// Runs a Lua expression or statement of Native's own (tests, tools),
    /// outside any task.
    pub fn exec(&mut self, source: &str) -> LuaResult<()> {
        self.lua.exec(source)
    }

    /// The entries `{name, state, priority}` of
    /// `Atl_Character_GetSubjects(name)` (@ 0x00402540) that it keeps (a
    /// string, a boolean and a number), in the result's order: what the
    /// subject menu shows (@ 0x00411a90).
    pub fn character_subject_list(
        &mut self,
        name: &[u8],
    ) -> Result<Vec<(Vec<u8>, bool, f64)>, String> {
        let r = self.call_global(
            b"Atl_Character_GetSubjects",
            &[Value::Str(name.to_vec())],
            1,
        )?;
        let state = self.lua.state_mut();
        let Some(Val::Table(t)) = r.first().copied() else {
            return Err("GetSubjects: the result is not a table".into());
        };
        let mut out = Vec::new();
        let mut i = 1.0;
        loop {
            let t_e = match raw_get(state, t, Val::Num(i)) {
                Val::Nil => return Ok(out),
                Val::Table(e) => e,
                _ => return Err(format!("GetSubjects: entry {i} is not a table")),
            };
            let s = raw_get(state, t_e, Val::Num(1.0));
            let b = raw_get(state, t_e, Val::Num(2.0));
            let p = raw_get(state, t_e, Val::Num(3.0));
            let priority = match p {
                Val::Num(x) => Some(x),
                Val::Str(_) => bytes_of(state, p)
                    .and_then(|b| String::from_utf8(b).ok())
                    .and_then(|s| s.trim().parse::<f64>().ok()),
                _ => None,
            };
            if let (true, Val::Bool(on), Some(pr)) =
                (matches!(s, Val::Str(_) | Val::Num(_)), b, priority)
            {
                out.push((bytes_of(state, s).unwrap_or_default(), on, pr));
            }
            i += 1.0;
        }
    }

    /// Calls the global function `name` with `args` (`lua_pcall`),
    /// keeping `nresults` results.
    fn call_global(
        &mut self,
        name: &[u8],
        args: &[Value],
        nresults: usize,
    ) -> Result<Vec<Val>, String> {
        let vals: Vec<Val> = args.iter().map(|a| self.lua_val(a)).collect();
        let state = self.lua.state_mut();
        let g = state.global;
        let k = str_val(state, name);
        let f = raw_get(state, g, k);
        let func_pos = state.top;
        state.ensure_stack(1 + vals.len());
        state.stack_set(func_pos, f);
        for (i, v) in vals.iter().enumerate() {
            state.stack_set(func_pos + 1 + i, *v);
        }
        state.top = func_pos + 1 + vals.len();
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let r = state.call_function(func_pos, nresults as i32);
        let out: Vec<Val> = (0..nresults)
            .map(|i| state.stack_get(func_pos + i))
            .collect();
        state.top = func_pos;
        r.map_err(|e| format!("{}: {e}", String::from_utf8_lossy(name)))?;
        Ok(out)
    }

    /// `atl.hero.inventory`: its `size` (@ 0x00403bb0) and the items of
    /// slots 0..size (`items[slot + 1]`, @ 0x00403920: a string, or
    /// nothing).
    pub fn inventory(&mut self) -> (usize, Vec<Option<Vec<u8>>>) {
        let inv: &[&[u8]] = &[b"atl", b"hero", b"inventory"];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let size = match self.get_field(inv, b"size") {
            Value::Num(x) if x > 0.0 => x as usize,
            _ => 0,
        };
        let Some(items) = self.global_table(&[b"atl", b"hero", b"inventory", b"items"]) else {
            return (size, vec![None; size]);
        };
        let state = self.lua.state_mut();
        let list = (0..size)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let v = raw_get(state, items, Val::Num((i + 1) as f64));
                match v {
                    Val::Str(_) => bytes_of(state, v),
                    _ => None,
                }
            })
            .collect();
        (size, list)
    }

    /// `ClearInventorySlot` (@ 0x00403390): `items[slot + 1]` = nil,
    /// `nb_items` − 1 (as a number).
    pub fn inventory_clear(&mut self, slot: usize) {
        self.inventory_write(slot, None);
    }

    /// `SetInventoryItem` (@ 0x00403650): `items[slot + 1]` = the name,
    /// `nb_items` + 1.
    pub fn inventory_set(&mut self, slot: usize, name: &[u8]) {
        self.inventory_write(slot, Some(name));
    }

    fn inventory_write(&mut self, slot: usize, name: Option<&[u8]>) {
        let inv: &[&[u8]] = &[b"atl", b"hero", b"inventory"];
        let Some(items) = self.global_table(&[b"atl", b"hero", b"inventory", b"items"]) else {
            return;
        };
        let n = match self.get_field(inv, b"nb_items") {
            Value::Num(x) => x,
            _ => return,
        };
        let state = self.lua.state_mut();
        let v = match name {
            Some(b) => Val::Str(state.gc.intern_string(b)),
            None => Val::Nil,
        };
        #[allow(clippy::cast_precision_loss)]
        let _ = raw_set(state, items, Val::Num((slot + 1) as f64), v);
        let d = if name.is_some() { 1.0 } else { -1.0 };
        self.set_field(inv, b"nb_items", &Value::Num(n + d));
    }

    /// `atl.hero.handled_item` set (@ 0x00404120) or cleared
    /// (@ 0x00403d30: nil).
    pub fn set_handled_item(&mut self, name: Option<&[u8]>) {
        let v = name.map_or(Value::Nil, |b| Value::Str(b.to_vec()));
        self.set_field(&[b"atl", b"hero"], b"handled_item", &v);
    }

    /// `SetClickedInventoryInfo` (@ 0x00401dc0): the event
    /// `compound_inventory_object` (0x0066835c), the click fields
    /// cleared, `atl.event.click.inventory.num_slot` = slot + 1.
    pub fn set_click_inventory(&mut self, slot: usize) {
        self.begin_event(b"compound_inventory_object");
        #[allow(clippy::cast_precision_loss)]
        self.set_field(
            &[b"atl", b"event", b"click", b"inventory"],
            b"num_slot",
            &Value::Num((slot + 1) as f64),
        );
    }

    /// `return Atl_Address_GetCurrent()` as the load runs it
    /// (@ 0x0043b210: `lua_tostring(-2)`, `lua_tonumber(-1)`).
    pub fn address_get_current(&mut self) -> Result<(Vec<u8>, f64), String> {
        let r = self.call_global(b"Atl_Address_GetCurrent", &[], 2)?;
        let state = self.lua.state_mut();
        let scene = bytes_of(state, r[0]).unwrap_or_default();
        let view = match r[1] {
            Val::Num(x) => x,
            v @ Val::Str(_) => bytes_of(state, v)
                .and_then(|b| String::from_utf8(b).ok())
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0.0),
            _ => 0.0,
        };
        Ok((scene, view))
    }

    /// `CSerialisableLuaDB::Serialize` (@ 0x00438920): the global `atl`
    /// (0x006680e4) written into a `LUA_DB` element (0x0066d950) by
    /// `WriteTable` (@ 0x00438be0). Each `lua_next` pair is one element
    /// named after its value (in the handler's order of tests): a table
    /// `T` (its pairs inside), `lua_isnumber` `N`, `lua_isstring` `S`,
    /// a boolean `B`, with the value in the attribute `v`; then the key in
    /// `ik` (`lua_isnumber`: a number) or `sk` (a string). Any other type
    /// throws (`type de valeur inconnue`, `type de clé inconnue`).
    /// Strings go through the ANSI code page (`_com_util`'s
    /// `ConvertStringToBSTR`, @ 0x0062a562): Windows-1252.
    pub fn lua_db(&mut self) -> Result<atlantis4_data::xml::Element, String> {
        let Some(atl) = self.global_table(&[b"atl"]) else {
            return Err(
                "CSerialisableLuaDB :: Serialize -> impossible de recuperer la table : atl".into(),
            );
        };
        let mut root = crate::save::element("LUA_DB");
        let state = self.lua.state_mut();
        write_table(state, atl, &mut root, 0)?;
        Ok(root)
    }

    /// `CSerialisableLuaDB::Deserialize` (@ 0x00439270): a new table set
    /// as the global `atl`, then `ReadTable` (@ 0x004393e0) on the
    /// children of `LUA_DB` (`./*`): each child's key from `sk` (a string)
    /// or else `ik` (a number); `T` a new table read the same way, `N` a
    /// number, `S` a string, `B` a boolean, from `v`.
    pub fn load_lua_db(&mut self, db: &atlantis4_data::xml::Element) -> Result<(), String> {
        let state = self.lua.state_mut();
        let t = new_table(state);
        let k = str_val(state, b"atl");
        let g = state.global;
        raw_set(state, g, k, Val::Table(t)).map_err(|e| e.to_string())?;
        read_table(state, t, db, 0)
    }
}

/// A number as the save writes it (a `VT_R8` attribute: MSXML's text
/// of it is not read; Lua's `%.14g`, which reads back the same).
fn save_number(x: f64) -> String {
    crate::engine::number_text(x)
}

/// `lua_isnumber` / `lua_tonumber` of a string (5.0's `strtod` in the C
/// locale, spaces around allowed).
fn lua_str_number(b: &[u8]) -> Option<f64> {
    std::str::from_utf8(b).ok()?.trim().parse().ok()
}

fn read_number(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse().ok()
}

fn write_table(
    state: &mut LuaState,
    t: GcRef<Table>,
    out: &mut atlantis4_data::xml::Element,
    depth: usize,
) -> Result<(), String> {
    use atlantis4_data::xml::cp1252;
    if depth > 64 {
        return Err("CSerialisableLuaDB :: WriteTable -> table too deep".into());
    }
    let mut key = Val::Nil;
    loop {
        let next = state
            .gc
            .tables
            .get(t)
            .ok_or("CSerialisableLuaDB :: WriteTable -> no table")?
            .next(key, &state.gc.string_arena)
            .map_err(|e| e.to_string())?;
        let Some((k, v)) = next else {
            return Ok(());
        };
        key = k;
        let mut e = match v {
            Val::Table(inner) => {
                let mut e = crate::save::element("T");
                write_table(state, inner, &mut e, depth + 1)?;
                e
            }
            Val::Num(x) => crate::save::element_v("N", save_number(x)),
            Val::Str(_) => {
                let b = bytes_of(state, v).unwrap_or_default();
                match lua_str_number(&b) {
                    Some(x) => crate::save::element_v("N", save_number(x)),
                    None => crate::save::element_v("S", cp1252(&b)),
                }
            }
            Val::Bool(b) => {
                crate::save::element_v("B", if b { "True" } else { "False" }.to_string())
            }
            _ => return Err("CSerialisableLuaDB :: WriteTable -> type de valeur inconnue".into()),
        };
        match k {
            Val::Num(x) => e.attributes.push(("ik".into(), save_number(x))),
            Val::Str(_) => {
                let b = bytes_of(state, k).unwrap_or_default();
                match lua_str_number(&b) {
                    Some(x) => e.attributes.push(("ik".into(), save_number(x))),
                    None => e.attributes.push(("sk".into(), cp1252(&b))),
                }
            }
            _ => return Err("CSerialisableLuaDB :: WriteTable -> type de clé inconnue".into()),
        }
        out.children.push(e);
    }
}

fn read_table(
    state: &mut LuaState,
    t: GcRef<Table>,
    from: &atlantis4_data::xml::Element,
    depth: usize,
) -> Result<(), String> {
    use atlantis4_data::xml::to_cp1252;
    if depth > 64 {
        return Err("CSerialisableLuaDB :: ReadTable -> table too deep".into());
    }
    for e in &from.children {
        let key = if let Some(sk) = e.attr("sk") {
            Val::Str(state.gc.intern_string(&to_cp1252(sk)))
        } else if let Some(ik) = e.attr("ik") {
            Val::Num(read_number(ik).ok_or("CSerialisableLuaDB :: ReadTable -> bad ik")?)
        } else {
            return Err("CSerialisableLuaDB :: ReadTable -> no key".into());
        };
        let v = e.attr("v");
        let value = match (e.name.as_str(), v) {
            ("T", _) => {
                let inner = new_table(state);
                read_table(state, inner, e, depth + 1)?;
                Val::Table(inner)
            }
            ("N", Some(v)) => Val::Num(read_number(v).ok_or("CSerialisableLuaDB :: ReadTable -> bad N")?),
            ("S", Some(v)) => Val::Str(state.gc.intern_string(&to_cp1252(v))),
            ("B", Some(v)) => Val::Bool(matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "true" | "-1" | "1" | "vrai"
            )),
            (name, _) => {
                return Err(format!(
                    "CSerialisableLuaDB :: ReadTable -> impossible de recuperer l'attribut \"v\" d'un Element <{name}>"
                ))
            }
        };
        raw_set(state, t, key, value).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msvcrt_sequence() {
        // MSVCRT's first values from seed 1.
        let mut h = 1;
        let v: Vec<u32> = (0..4).map(|_| msvcrt_rand(&mut h)).collect();
        assert_eq!(v, [41, 18467, 6334, 26500]);
    }

    #[test]
    fn sound_names() {
        assert_eq!(sound_path(b"zep_crack"), b"fx/zep_crack.ogg");
    }
}

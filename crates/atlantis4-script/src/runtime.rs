//! Lua states, bindings, callbacks and the boot and node-change sequences.
//!
//! Addresses are atlantis4-gog-en-2363910's.

use std::cell::RefCell;
use std::rc::Rc;

use rilua::vm::closure::{Closure, RustClosure};
use rilua::vm::gc::arena::GcRef;
use rilua::vm::state::{LuaState, MAXCALLS};
use rilua::vm::table::Table;
use rilua::vm::value::{Userdata, Val};
use rilua::{Lua, LuaApi, LuaApiMut, LuaError, LuaResult, RuntimeError};

use crate::engine::{
    Arg, BootValue, Call, Engine, ErrorSite, Event, GuiKey, GuiValue, Reply, SavedGlobal,
    SavedValue, Value,
};
use crate::tolua::{Binding, Check, Conversion, Signature, SIGNATURES};

/// The engine as the bindings reach it.
pub type EngineRef = Rc<RefCell<dyn Engine>>;

/// What a Lua error in a callback the engine called does. The original
/// calls callbacks with a bare `luaD_call`, so an error there reaches its
/// panic function, which prints `Lua: <message>`, and the process exits.
/// Native logs and continues by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorMode {
    /// Report the error as an event and go on.
    Log,
    /// Stop: the call returns [`ScriptError::Fatal`], as the original exits.
    Strict,
}

/// Whether the collector runs. The order of the globals (hence of the
/// save's elements) depends on when collections happen, so exact traces
/// are compared with the collector stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collector {
    Running,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub errors: ErrorMode,
    pub collector: Collector,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            errors: ErrorMode::Log,
            collector: Collector::Running,
        }
    }
}

/// An error that stops the game in strict mode.
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptError {
    /// A Lua error in a callback the engine called (the original exits
    /// with code 1 after printing `Lua: <message>`).
    Fatal { callback: String, message: Vec<u8> },
    /// The Lua state could not be set up.
    State(String),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fatal { message, .. } => write!(f, "Lua: {}", String::from_utf8_lossy(message)),
            Self::State(m) => write!(f, "Lua state: {m}"),
        }
    }
}

impl std::error::Error for ScriptError {}

impl From<LuaError> for ScriptError {
    fn from(e: LuaError) -> Self {
        Self::State(e.to_string())
    }
}

/// The eight callbacks `ChangeWarp`'s worker sets to `nil` before loading
/// a node's script (@ `0x005273a7`..`0x00527b8f`), in that order.
pub const RESET_CALLBACKS: [&str; 8] = [
    "OnWarpEnter",
    "OnWarpLeave",
    "OnWarpObjectHit",
    "OnMovieFinished",
    "OnAnimationFinished",
    "OnDialogFinished",
    "OnDocumentClosed",
    "OnPuzzleWon",
];

/// The 13 `TeLuaGUI` constructors (@ `0x0054ba1b`..`0x0054c26f`).
pub const GUI_CLASSES: [&str; 13] = [
    "TeLayout",
    "TeListLayout",
    "TeSpriteLayout",
    "TeButtonLayout",
    "TeCheckboxLayout",
    "TeLayoutPositionLinearAnimation",
    "TeLayoutAnchorLinearAnimation",
    "TeTextLayout",
    "TeClipLayout",
    "TeColorLinearAnimation",
    "TeRotationLinearAnimation",
    "TeScrollingLayout",
    "TeExtendedTextLayout",
];

/// The layout enums each `TeLuaGUI` state gets (@ `0x0054b5aa`).
const GUI_ENUMS: [(&str, f64); 7] = [
    ("Pixel", 0.0),
    ("Percent", 1.0),
    ("None", 0.0),
    ("LetterBox", 1.0),
    ("PanScan", 2.0),
    ("MultiLine", 0.0),
    ("SingleLine", 1.0),
];

/// tolua++'s own module (`tolua_open` @ `0x004db5e0`): the global table
/// `tolua` with these functions, in this order.
const TOLUA_FUNCTIONS: [&str; 7] = [
    "type",
    "takeownership",
    "releaseownership",
    "cast",
    "inherit",
    "setpeer",
    "getpeer",
];

/// The registry key under which a state keeps its engine.
const ENGINE_KEY: &[u8] = b"atlantis4-script.engine";

fn runtime_error(message: String) -> LuaError {
    LuaError::Runtime(RuntimeError::new(message))
}

// ---------------------------------------------------------------------------
// States
// ---------------------------------------------------------------------------

/// `luaL_newstate` (with the collector stopped first when asked) and
/// `luaL_openlibs`, with the engine kept in the registry.
fn new_state(engine: &EngineRef, options: Options) -> LuaResult<Lua> {
    let mut lua = Lua::new_empty();
    if options.collector == Collector::Stopped {
        lua.gc_stop();
    }
    rilua::stdlib::open_libs(lua.state_mut())?;
    let state = lua.state_mut();
    let data: Box<dyn std::any::Any> = Box::new(engine.clone());
    let ud = state.gc.alloc_userdata(Userdata::new(data));
    let key = Val::Str(state.gc.intern_string(ENGINE_KEY));
    raw_set(state, state.registry, key, Val::Userdata(ud))?;
    Ok(lua)
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

fn str_val(state: &mut LuaState, s: &[u8]) -> Val {
    Val::Str(state.gc.intern_string(s))
}

/// `lua_setfield(L, LUA_GLOBALSINDEX, name)` (`luaV_settable`).
fn set_global(state: &mut LuaState, name: &[u8], value: Val) -> LuaResult<()> {
    let key = str_val(state, name);
    let globals = Val::Table(state.global);
    state.settable(globals, key, value)
}

/// `lua_getfield(L, LUA_GLOBALSINDEX, name)` (`luaV_gettable`).
fn get_global(state: &mut LuaState, name: &[u8]) -> LuaResult<Val> {
    let key = str_val(state, name);
    let globals = Val::Table(state.global);
    state.gettable(globals, key)
}

fn engine_of(state: &mut LuaState) -> LuaResult<EngineRef> {
    let key = str_val(state, ENGINE_KEY);
    if let Val::Userdata(u) = raw_get(state, state.registry, key) {
        if let Some(e) = state
            .gc
            .userdata
            .get(u)
            .and_then(|u| u.downcast_ref::<EngineRef>())
        {
            return Ok(e.clone());
        }
    }
    Err(runtime_error("no engine in this state".into()))
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

/// The first upvalue of the running Rust closure.
fn upvalue(state: &LuaState) -> Val {
    let func = state.stack_get(state.call_stack[state.ci].func);
    if let Val::Function(r) = func {
        if let Some(Closure::Rust(rc)) = state.gc.closures.get(r) {
            return rc.upvalues.first().copied().unwrap_or(Val::Nil);
        }
    }
    Val::Nil
}

fn bytes_of(state: &LuaState, v: Val) -> Option<Vec<u8>> {
    match v {
        Val::Str(r) => state.gc.string_arena.get(r).map(|s| s.data().to_vec()),
        Val::Num(_) => Some(format!("{v}").into_bytes()),
        _ => None,
    }
}

/// `lua_tostring` of an error value (`None` for a value that is neither a
/// string nor a number).
fn error_text(state: &LuaState, v: Val) -> Vec<u8> {
    bytes_of(state, v).unwrap_or_else(|| b"(non-string error)".to_vec())
}

// ---------------------------------------------------------------------------
// tolua++
// ---------------------------------------------------------------------------

fn nargs(state: &LuaState) -> usize {
    state.top.saturating_sub(state.base)
}

/// Argument `i` (1-based), `None` past `lua_gettop`.
fn arg(state: &LuaState, i: usize) -> Option<Val> {
    let idx = state.base + i - 1;
    (idx < state.top).then(|| state.stack_get(idx))
}

/// `tolua_typename` (@ `0x004dc210`).
fn typename(state: &LuaState, i: usize) -> String {
    let class_name = |key: Val| match raw_get(state, state.registry, key) {
        v @ (Val::Str(_) | Val::Num(_)) => {
            bytes_of(state, v).map(|b| String::from_utf8_lossy(&b).into_owned())
        }
        _ => None,
    };
    match arg(state, i) {
        None => "[no object]".into(),
        Some(Val::Userdata(u)) => match state.gc.userdata.get(u).and_then(Userdata::metatable) {
            None => "userdata".into(),
            Some(mt) => class_name(Val::Table(mt)).unwrap_or_else(|| "[undefined]".into()),
        },
        Some(v @ Val::Table(_)) => {
            class_name(v).map_or_else(|| "table".into(), |name| format!("class {name}"))
        }
        Some(v) => v.type_name().into(),
    }
}

/// `tolua_isusertype(L, i, "const TeString", 0)` after `push_table_instance`.
fn is_te_string(state: &mut LuaState, v: Val) -> LuaResult<bool> {
    let v = if let Val::Table(_) = v {
        // push_table_instance: t[".c_instance"] (lua_gettable, metamethods
        // included) stands for the table when it is a userdata.
        let key = str_val(state, b".c_instance");
        match state.gettable(v, key)? {
            u @ Val::Userdata(_) => u,
            _ => return Ok(false),
        }
    } else {
        v
    };
    let Val::Userdata(u) = v else {
        // A light userdata's metatable is the shared one, which is unset.
        return Ok(false);
    };
    let Some(mt) = state.gc.userdata.get(u).and_then(Userdata::metatable) else {
        return Ok(false);
    };
    // The registry maps a class metatable to its name. (The original's
    // registry also holds tolua++'s `tolua_super` map, which nothing here
    // creates.)
    let name = raw_get(state, state.registry, Val::Table(mt));
    Ok(bytes_of(state, name).as_deref() == Some(b"const TeString".as_slice()))
}

/// The checks of one signature: `None` when they pass, else the failing
/// index and the expected type.
fn check(state: &mut LuaState, sig: &Signature) -> LuaResult<Option<(usize, &'static str)>> {
    let top = nargs(state);
    for (k, c) in sig.checks.iter().enumerate() {
        let i = k + 1;
        let optional = matches!(c, Check::OptStr | Check::OptNum | Check::OptBool);
        if optional && top < i {
            continue;
        }
        let v = arg(state, i);
        let (ok, expected) = match c {
            Check::Str | Check::OptStr => (
                matches!(v, Some(Val::Nil | Val::Str(_) | Val::Num(_))),
                "string",
            ),
            Check::Num | Check::OptNum => {
                let ok = match v {
                    Some(Val::Num(_)) => true,
                    // luaO_str2d through rilua's coercion.
                    Some(Val::Str(_)) => state.check_number(i).is_ok(),
                    _ => false,
                };
                (ok, "number")
            }
            Check::Bool | Check::OptBool => {
                (matches!(v, None | Some(Val::Nil | Val::Bool(_))), "boolean")
            }
            Check::ValueTeString => {
                // tolua_isvaluenil: an explicit nil is reported as 'value'.
                if matches!(v, Some(Val::Nil)) {
                    return Ok(Some((i, "value")));
                }
                let ok = match v {
                    Some(v) => is_te_string(state, v)?,
                    None => false,
                };
                (ok, "const TeString")
            }
        };
        if !ok {
            return Ok(Some((i, expected)));
        }
    }
    let past = sig.checks.len() + 1;
    if top >= past {
        return Ok(Some((past, "[no object]")));
    }
    Ok(None)
}

fn convert(state: &LuaState, c: &Conversion, top: usize) -> Arg {
    match *c {
        Conversion::Str(i, default) => {
            if top < i {
                Arg::Str(default.map(|d| d.as_bytes().to_vec()))
            } else {
                Arg::Str(state.check_string(i).ok())
            }
        }
        Conversion::Num(i, default) => {
            if top < i {
                Arg::Num(default)
            } else {
                Arg::Num(state.check_number(i).unwrap_or(0.0))
            }
        }
        Conversion::Bool(i, default) => {
            if top < i {
                Arg::Bool(default)
            } else {
                Arg::Bool(arg(state, i).is_some_and(Val::is_truthy))
            }
        }
        Conversion::Object(_) => Arg::Object,
    }
}

/// Every binding: the signature index is the closure's upvalue.
fn binding_entry(state: &mut LuaState) -> LuaResult<u32> {
    let index = match upvalue(state) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Val::Num(n) => n as usize,
        _ => return Err(runtime_error("binding without its index".into())),
    };
    let mut sig = &SIGNATURES[index];
    // tolua++ overloads: a failed check runs the earlier entry instead.
    let failure = loop {
        match check(state, sig)? {
            None => break None,
            Some(f) => match sig.fallback {
                Some(fb) => sig = fb.signature(),
                None => break Some(f),
            },
        }
    };
    if let Some((i, expected)) = failure {
        // tolua_error (@ 0x004dc5d0) with "#ferror in function '<name>'.":
        // luaL_error, which prefixes the caller's position.
        let got = typename(state, i);
        return Err(runtime_error(format!(
            "error in function '{}'.\n     argument #{i} is '{got}'; '{expected}' expected.\n",
            sig.name
        )));
    }
    let top = nargs(state);
    let args = sig
        .conversions
        .iter()
        .map(|c| convert(state, c, top))
        .collect();
    let call = Call {
        binding: sig.binding,
        args,
    };
    let engine = engine_of(state)?;
    let reply = engine.borrow_mut().call(&call);
    match sig.binding {
        // SetCondition / UnsetCondition (@ 0x0052ba10 / 0x0052b970): the
        // global `name` = true / false through luaV_settable. A NULL name
        // (nil argument) is taken as "" here.
        Binding::SetCondition | Binding::UnsetCondition => {
            let name = call.str(0).unwrap_or_default().to_vec();
            set_global(
                state,
                &name,
                Val::Bool(sig.binding == Binding::SetCondition),
            )?;
        }
        Binding::SaveGame => {
            let globals = save_walk(state)?;
            let file = call.str(0).unwrap_or_default().to_vec();
            engine.borrow_mut().save_game(&file, &globals);
        }
        Binding::ChangeWarp if reply == Reply::Warp => {
            let path = call.str(0).unwrap_or_default().to_vec();
            change_warp(state, &engine, &path, None).map_err(|e| match e {
                Stop::Lua(e) => e,
                Stop::Fatal(f) => runtime_error(f.to_string()),
            })?;
        }
        _ => {}
    }
    match sig.binding {
        Binding::Selected | Binding::TestObjectLimit => {
            state.push(Val::Bool(reply == Reply::Bool(true)));
            Ok(1)
        }
        Binding::ObjectCount => {
            state.push(Val::Num(if let Reply::Num(x) = reply { x } else { 0.0 }));
            Ok(1)
        }
        _ => Ok(0),
    }
}

fn tolua_stub(state: &mut LuaState) -> LuaResult<u32> {
    let name = bytes_of(state, upvalue(state)).unwrap_or_default();
    Err(runtime_error(format!(
        "tolua.{} is not available",
        String::from_utf8_lossy(&name)
    )))
}

/// `tolua_open` (its global module `tolua`) and the 54 bindings in
/// registration order, all raw sets on the globals (@ `0x00581950`).
fn register_bindings(state: &mut LuaState) -> LuaResult<()> {
    let tolua = state.gc.alloc_table(Table::new());
    for name in TOLUA_FUNCTIONS {
        let up = str_val(state, name.as_bytes());
        let f = rust_closure(state, tolua_stub, name, up);
        let key = str_val(state, name.as_bytes());
        raw_set(state, tolua, key, f)?;
    }
    let key = str_val(state, b"tolua");
    raw_set(state, state.global, key, Val::Table(tolua))?;
    for (i, sig) in SIGNATURES.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let f = rust_closure(state, binding_entry, sig.name, Val::Num(i as f64));
        let key = str_val(state, sig.name.as_bytes());
        raw_set(state, state.global, key, f)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Calls into Lua
// ---------------------------------------------------------------------------

/// `lua_pcall` from Rust: the error value on failure, the state restored.
fn protected_call(state: &mut LuaState, func: Val, args: &[Val]) -> Result<(), Val> {
    let func_pos = state.top;
    state.ensure_stack(args.len() + 1);
    state.stack_set(func_pos, func);
    for (k, a) in args.iter().enumerate() {
        state.stack_set(func_pos + 1 + k, *a);
    }
    state.top = func_pos + 1 + args.len();
    let saved_ci = state.ci;
    let saved_base = state.base;
    let saved_n_ccalls = state.n_ccalls;
    let saved_depth = state.call_depth;
    state.error_object = None;
    match state.call_function(func_pos, 0) {
        Ok(()) => {
            state.top = func_pos;
            state.base = saved_base;
            Ok(())
        }
        Err(err) => {
            state.ci = saved_ci;
            state.base = state.call_stack[state.ci].base;
            state.n_ccalls = saved_n_ccalls;
            state.call_depth = saved_depth;
            if state.ci < MAXCALLS {
                state.ci_overflow = false;
            }
            state.close_upvalues(func_pos);
            let v = state
                .error_object
                .take()
                .unwrap_or_else(|| Val::Str(state.gc.intern_string(err.to_string().as_bytes())));
            state.top = func_pos;
            Err(v)
        }
    }
}

/// An unprotected call (`luaD_call`): errors go to the caller.
fn plain_call(state: &mut LuaState, func: Val, args: &[Val]) -> LuaResult<()> {
    let func_pos = state.top;
    state.ensure_stack(args.len() + 1);
    state.stack_set(func_pos, func);
    for (k, a) in args.iter().enumerate() {
        state.stack_set(func_pos + 1 + k, *a);
    }
    state.top = func_pos + 1 + args.len();
    state.call_function(func_pos, 0)?;
    state.top = func_pos;
    Ok(())
}

fn event(engine: &EngineRef, e: &Event<'_>) {
    engine.borrow_mut().event(e);
}

/// `TeLuaScript::load` + `TeLuaScript::execute` (@ `0x004f8e40`,
/// `0x004f98c0`): `luaL_loadbuffer` with the path as chunk name, then
/// `lua_pcall`; an error is dropped by the original (reported here as an
/// event).
fn run_file(state: &mut LuaState, engine: &EngineRef, path: &[u8]) {
    event(engine, &Event::Load(path));
    let Some(source) = engine.borrow_mut().read_script(path) else {
        event(engine, &Event::MissingFile(path));
        return;
    };
    run_chunk(state, engine, &source, path);
}

/// Compiles `source` with `name` as chunk name and runs it under
/// `lua_pcall`; errors are reported, not returned.
fn run_chunk(state: &mut LuaState, engine: &EngineRef, source: &[u8], name: &[u8]) {
    let chunk_name = String::from_utf8_lossy(name).into_owned();
    let func = match state.load_bytes(source, &chunk_name) {
        Ok(f) => f,
        Err(e) => {
            event(
                engine,
                &Event::Error(ErrorSite::Load(name.to_vec()), e.to_string().as_bytes()),
            );
            return;
        }
    };
    if let Err(v) = protected_call(state, Val::Function(func.gc_ref()), &[]) {
        let msg = error_text(state, v);
        event(engine, &Event::Error(ErrorSite::Chunk(name.to_vec()), &msg));
    }
}

/// Why a sequence stopped.
enum Stop {
    /// A Lua error going up to the enclosing protected call.
    Lua(LuaError),
    /// Strict mode at the top level.
    Fatal(ScriptError),
}

impl From<LuaError> for Stop {
    fn from(e: LuaError) -> Self {
        Self::Lua(e)
    }
}

fn to_vals(state: &mut LuaState, args: &[Value]) -> Vec<Val> {
    args.iter()
        .map(|a| match a {
            Value::Str(s) => str_val(state, s),
            Value::Num(x) => Val::Num(*x),
            Value::Bool(b) => Val::Bool(*b),
        })
        .collect()
}

/// `TeLuaScript::Execute(name, …)` (@ `0x004f90a0`, `0x004f95e0`,
/// `0x004f9140`): the global if it is a function, else the "n'existe pas"
/// log. `top` is the error mode when the engine itself calls (no Lua
/// frame below: the original's error is fatal), `None` when called from a
/// binding (the error goes up to the Lua caller).
fn execute(
    state: &mut LuaState,
    engine: &EngineRef,
    name: &str,
    args: &[Value],
    top: Option<ErrorMode>,
) -> Result<(), Stop> {
    let f = get_global(state, name.as_bytes())?;
    if !matches!(f, Val::Function(_)) {
        event(engine, &Event::Missing(name));
        return Ok(());
    }
    event(engine, &Event::Enter(name, args));
    let vals = to_vals(state, args);
    let Some(mode) = top else {
        return plain_call(state, f, &vals).map_err(Stop::Lua);
    };
    if let Err(v) = protected_call(state, f, &vals) {
        let msg = error_text(state, v);
        event(
            engine,
            &Event::Error(ErrorSite::Callback(name.to_string()), &msg),
        );
        if mode == ErrorMode::Strict {
            return Err(Stop::Fatal(ScriptError::Fatal {
                callback: name.to_string(),
                message: msg,
            }));
        }
    }
    Ok(())
}

/// The path helper @ `0x00552bd0`: `\` becomes `/`, then `//` becomes `/`.
pub fn normalise_path(path: &[u8]) -> Vec<u8> {
    let mut p: Vec<u8> = path
        .iter()
        .map(|&c| if c == b'\\' { b'/' } else { c })
        .collect();
    while let Some(i) = p.windows(2).position(|w| w == b"//") {
        p.remove(i);
    }
    p
}

/// @ `0x00552e60`: the path with another extension.
pub fn with_extension(path: &[u8], ext: &[u8]) -> Vec<u8> {
    let slash = path.iter().rposition(|&c| c == b'/');
    let mut out = path.to_vec();
    match path.iter().rposition(|&c| c == b'.') {
        Some(dot) if slash.is_none_or(|s| dot > s) => out.truncate(dot + 1),
        _ => out.push(b'.'),
    }
    out.extend_from_slice(ext);
    out
}

/// `ChangeWarp`'s worker (@ `0x00526c70`).
fn change_warp(
    state: &mut LuaState,
    engine: &EngineRef,
    path: &[u8],
    top: Option<ErrorMode>,
) -> Result<(), Stop> {
    let path = normalise_path(path);
    event(engine, &Event::Warp(&path));
    let has_scene = engine.borrow().has_scene();
    if has_scene {
        execute(state, engine, "OnWarpLeave", &[], top)?;
        engine.borrow_mut().leave_scene();
        let globals = save_walk(state)?;
        engine.borrow_mut().save_game(b"save.xml", &globals);
    }
    engine.borrow_mut().enter_scene(&path);
    for name in RESET_CALLBACKS {
        set_global(state, name.as_bytes(), Val::Nil)?;
    }
    engine
        .borrow_mut()
        .load_scene_xml(&with_extension(&path, b"xml"));
    let v = str_val(state, &path);
    set_global(state, b"warpName", v)?;
    run_file(state, engine, &with_extension(&path, b"lua"));
    execute(state, engine, "OnWarpEnter", &[], top)
}

// ---------------------------------------------------------------------------
// The save's walk
// ---------------------------------------------------------------------------

/// The globals the save writes, in `lua_next` order (@ `0x004c1670`):
/// boolean, number and string values with a string key. (The original
/// converts a non-string key in place with `lua_tostring`, which would
/// break its walk; no script makes one.)
fn save_walk(state: &LuaState) -> LuaResult<Vec<SavedGlobal>> {
    let strings = &state.gc.string_arena;
    let t = state
        .gc
        .tables
        .get(state.global)
        .ok_or_else(|| runtime_error("globals not found".into()))?;
    let mut out = Vec::new();
    let mut key = Val::Nil;
    while let Some((k, v)) = t.next(key, strings)? {
        key = k;
        let Val::Str(kr) = k else { continue };
        let value = match v {
            Val::Bool(b) => SavedValue::Boolean(b),
            Val::Num(x) => SavedValue::Number(x),
            Val::Str(r) => SavedValue::String(
                strings
                    .get(r)
                    .map(|s| s.data().to_vec())
                    .unwrap_or_default(),
            ),
            _ => continue,
        };
        let name = strings
            .get(kr)
            .map(|s| s.data().to_vec())
            .unwrap_or_default();
        out.push(SavedGlobal { name, value });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// TeLuaGUI states
// ---------------------------------------------------------------------------

fn gui_key(state: &LuaState, k: Val) -> GuiKey {
    match k {
        Val::Str(_) => GuiKey::Name(bytes_of(state, k).unwrap_or_default()),
        Val::Num(x) => GuiKey::Index(x),
        other => GuiKey::Other(other.type_name()),
    }
}

fn gui_value(state: &LuaState, v: Val, depth: usize) -> LuaResult<GuiValue> {
    Ok(match v {
        Val::Nil => GuiValue::Nil,
        Val::Bool(b) => GuiValue::Bool(b),
        Val::Num(x) => GuiValue::Num(x),
        Val::Str(_) => GuiValue::Str(bytes_of(state, v).unwrap_or_default()),
        Val::LightUserdata(id) => GuiValue::Child(id),
        Val::Table(t) if depth < 8 => GuiValue::Table(gui_entries(state, t, depth + 1)?),
        other => GuiValue::Other(other.type_name()),
    })
}

/// A table's entries in `next` order: the array part, then the hash part
/// in node order (`luaH_next` @ `0x004aad30`).
fn gui_entries(
    state: &LuaState,
    t: GcRef<Table>,
    depth: usize,
) -> LuaResult<Vec<(GuiKey, GuiValue)>> {
    let table = state
        .gc
        .tables
        .get(t)
        .ok_or_else(|| runtime_error("table not found".into()))?;
    let mut out = Vec::new();
    let mut key = Val::Nil;
    while let Some((k, v)) = table.next(key, &state.gc.string_arena)? {
        key = k;
        out.push((gui_key(state, k), gui_value(state, v, depth)?));
    }
    Ok(out)
}

/// A `TeLuaGUI` constructor: its table (one argument) to the engine, the
/// new layout back as a light userdata.
fn gui_constructor(state: &mut LuaState) -> LuaResult<u32> {
    let class = bytes_of(state, upvalue(state)).unwrap_or_default();
    let class = String::from_utf8_lossy(&class).into_owned();
    let engine = engine_of(state)?;
    let entries = match arg(state, 1) {
        Some(Val::Table(t)) => Some(gui_entries(state, t, 0)?),
        _ => None,
    };
    let is_table = entries.is_some();
    let id = engine.borrow_mut().gui_layout(&class, entries.as_deref());
    if !is_table {
        return Ok(0);
    }
    state.push(Val::LightUserdata(id));
    Ok(1)
}

/// A `TeLuaGUI` state (`TeLuaGUI::load` @ `0x0054b1d0`): a fresh state
/// with the libraries, the seven layout enums, the 13 constructors and
/// `__TeLuaGUIThis`, without the game bindings.
pub struct GuiState {
    lua: Lua,
    engine: EngineRef,
}

impl GuiState {
    pub fn new(engine: &EngineRef, options: Options) -> Result<Self, ScriptError> {
        let mut lua = new_state(engine, options)?;
        let state = lua.state_mut();
        for (name, value) in GUI_ENUMS {
            set_global(state, name.as_bytes(), Val::Num(value))?;
        }
        for class in GUI_CLASSES {
            let up = str_val(state, class.as_bytes());
            let f = rust_closure(state, gui_constructor, class, up);
            set_global(state, class.as_bytes(), f)?;
        }
        set_global(state, b"__TeLuaGUIThis", Val::LightUserdata(0))?;
        Ok(Self {
            lua,
            engine: engine.clone(),
        })
    }

    /// Runs a layout file (errors dropped, as the original's loader).
    pub fn run_file(&mut self, path: &[u8]) {
        run_file(self.lua.state_mut(), &self.engine, path);
    }

    /// A global of the state, as the engine reads it back.
    pub fn read(&mut self, name: &str) -> Result<Option<BootValue>, ScriptError> {
        let state = self.lua.state_mut();
        let v = get_global(state, name.as_bytes())?;
        Ok(match v {
            Val::Nil => None,
            Val::Bool(b) => Some(BootValue::Bool(b)),
            Val::Num(x) => Some(BootValue::Num(x)),
            Val::Str(_) => Some(BootValue::Str(bytes_of(state, v).unwrap_or_default())),
            other => Some(BootValue::Other(other.type_name())),
        })
    }

    /// The Lua state, for tests and tools.
    pub fn lua(&mut self) -> &mut Lua {
        &mut self.lua
    }
}

// ---------------------------------------------------------------------------
// The game state
// ---------------------------------------------------------------------------

/// The game's Lua state (`Game::load` @ `0x00529e30`): the libraries,
/// tolua++'s module and the 54 bindings; node scripts,
/// `3D/OnGameEnter.lua`, `Diary/Diary.lua` and
/// `Inventory/InventoryScripts.lua` all run in it.
pub struct Script {
    lua: Lua,
    engine: EngineRef,
    options: Options,
}

impl Script {
    /// A fresh game state, no file run yet.
    pub fn new(engine: &EngineRef, options: Options) -> Result<Self, ScriptError> {
        let mut lua = new_state(engine, options)?;
        register_bindings(lua.state_mut())?;
        Ok(Self {
            lua,
            engine: engine.clone(),
            options,
        })
    }

    /// Runs a script file in the game state (errors dropped).
    pub fn run_file(&mut self, path: &[u8]) {
        run_file(self.lua.state_mut(), &self.engine, path);
    }

    /// Runs Lua source as a chunk named `name` (errors reported, not
    /// returned), as a file is run but without reading it.
    pub fn run_chunk(&mut self, source: &[u8], name: &[u8]) {
        run_chunk(self.lua.state_mut(), &self.engine, source, name);
    }

    /// Calls a callback as the engine does (`OnWarpObjectHit(name)`,
    /// `OnAnswered(n)`, …); a missing one is only reported.
    pub fn callback(&mut self, name: &str, args: &[Value]) -> Result<(), ScriptError> {
        let mode = self.options.errors;
        stop_to_result(
            execute(self.lua.state_mut(), &self.engine, name, args, Some(mode)),
            mode,
        )
    }

    /// The node change as `ChangeWarp`'s worker does it, called by the
    /// engine (the first node of a new game; a loaded save).
    pub fn change_warp(&mut self, path: &[u8]) -> Result<(), ScriptError> {
        let mode = self.options.errors;
        stop_to_result(
            change_warp(self.lua.state_mut(), &self.engine, path, Some(mode)),
            mode,
        )
    }

    /// The globals the save writes, in `lua_next` order.
    pub fn globals(&self) -> Result<Vec<SavedGlobal>, ScriptError> {
        Ok(save_walk(self.lua.state())?)
    }

    /// The Lua state, for tests and tools.
    pub fn lua(&mut self) -> &mut Lua {
        &mut self.lua
    }
}

fn stop_to_result(r: Result<(), Stop>, mode: ErrorMode) -> Result<(), ScriptError> {
    match r {
        Ok(()) => Ok(()),
        Err(Stop::Fatal(e)) => Err(e),
        // A Lua error outside a callback call (a metamethod of the globals
        // table): fatal in strict mode, else dropped.
        Err(Stop::Lua(e)) if mode == ErrorMode::Strict => Err(ScriptError::Fatal {
            callback: String::new(),
            message: e.to_string().into_bytes(),
        }),
        Err(Stop::Lua(_)) => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

/// The boot variables the application reads at start, each from its own
/// `Texts` file run in a `TeLuaGUI` state (@ `0x00578d90`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BootVars {
    pub application_title: Option<BootValue>,
    pub backup_path_template: Option<BootValue>,
    pub version_string: Option<BootValue>,
    pub first_warp_path: Option<BootValue>,
}

/// The four files in the order the application loads them, with the
/// variable read from each. The paths are the logical ones the engine
/// resolves (`Texts/Full/…`, `Texts/PC/…`).
pub const BOOT_FILES: [(&str, &str); 4] = [
    ("Texts/ApplicationTitle.lua", "applicationTitle"),
    ("texts/BackupPathTemplate.lua", "backupPathTemplate"),
    ("Texts/Version.lua", "versionString"),
    ("Texts/FirstWarpPath.lua", "firstWarpPath"),
];

/// Reads the four boot variables.
pub fn read_boot_vars(engine: &EngineRef, options: Options) -> Result<BootVars, ScriptError> {
    let mut values = Vec::new();
    for (logical, var) in BOOT_FILES {
        let path = engine.borrow_mut().resolve(logical.as_bytes());
        let mut gui = GuiState::new(engine, options)?;
        gui.run_file(&path);
        let v = gui.read(var)?;
        event(engine, &Event::Read(var, v.as_ref()));
        values.push(v);
    }
    let mut it = values.into_iter();
    Ok(BootVars {
        application_title: it.next().flatten(),
        backup_path_template: it.next().flatten(),
        version_string: it.next().flatten(),
        first_warp_path: it.next().flatten(),
    })
}

/// A new game (`Game::load` without a save, @ `0x00529e30`): the game
/// state, `3D/OnGameEnter.lua`, `Diary/Diary.lua`,
/// `Inventory/InventoryScripts.lua`, then the node change to
/// `first_warp_path`.
pub fn new_game(
    engine: &EngineRef,
    options: Options,
    first_warp_path: &[u8],
) -> Result<Script, ScriptError> {
    event(engine, &Event::Game);
    let mut script = Script::new(engine, options)?;
    script.run_file(b"3D/OnGameEnter.lua");
    script.run_file(b"Diary/Diary.lua");
    script.run_file(b"Inventory/InventoryScripts.lua");
    script.change_warp(first_warp_path)?;
    Ok(script)
}

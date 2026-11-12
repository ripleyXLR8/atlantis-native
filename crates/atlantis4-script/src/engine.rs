//! What the scripts call out to: the engine side of the bindings, and the
//! hooks of the boot and node-change sequences.

use crate::tolua::Binding;

/// A converted argument, as the binding's `tolua_to*` call returned it.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// `tolua_tostring`: `None` is the NULL a `nil` argument gives.
    /// Numbers arrive converted (`%.14g`). Bytes as in the script (cp1252).
    Str(Option<Vec<u8>>),
    /// `tolua_tonumber` (the engine truncates most to `int` itself).
    Num(f64),
    /// `tolua_toboolean`.
    Bool(bool),
    /// `tolua_tousertype` (`PushAnswer`'s first entry; unreachable from
    /// scripts).
    Object,
}

/// One binding call: the registration entry and its converted arguments
/// in conversion order.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub binding: Binding,
    pub args: Vec<Arg>,
}

impl Call {
    /// String argument `i` (0-based), `None` for NULL or another kind.
    pub fn str(&self, i: usize) -> Option<&[u8]> {
        match self.args.get(i) {
            Some(Arg::Str(Some(s))) => Some(s),
            _ => None,
        }
    }

    /// Number argument `i` (0-based).
    pub fn num(&self, i: usize) -> f64 {
        match self.args.get(i) {
            Some(Arg::Num(x)) => *x,
            _ => 0.0,
        }
    }

    /// Boolean argument `i` (0-based).
    pub fn flag(&self, i: usize) -> bool {
        matches!(self.args.get(i), Some(Arg::Bool(true)))
    }
}

/// The engine's answer to a binding call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reply {
    None,
    /// `Selected`, `TestObjectLimit`.
    Bool(bool),
    /// `ObjectCount`.
    Num(f64),
    /// `ChangeWarp`: run the node change now, inside the binding, as the
    /// original does (its binding calls the worker directly).
    Warp,
}

/// A global the save writes (TeLuaContext save, atlantis4-gog-en-2363910
/// @ `0x004c1670`): booleans, numbers and strings, in `lua_next` order.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedGlobal {
    pub name: Vec<u8>,
    pub value: SavedValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SavedValue {
    /// `<Boolean name=… value="%d">`.
    Boolean(bool),
    /// `<Number name=… value="%f">`.
    Number(f64),
    /// `<String name=… …>`.
    String(Vec<u8>),
}

impl SavedValue {
    /// The value as the save prints it (`%d`, `%f`, the bytes).
    pub fn text(&self) -> Vec<u8> {
        match self {
            Self::Boolean(b) => vec![if *b { b'1' } else { b'0' }],
            Self::Number(x) => format!("{x:.6}").into_bytes(),
            Self::String(s) => s.clone(),
        }
    }
}

/// A global read back from a save (`TeLuaContext::load`,
/// atlantis4-gog-en-2363910 @ `0x004c1aa0`): the value pushed for one
/// top-level `Boolean` / `Number` / `String` element.
#[derive(Clone, Debug, PartialEq)]
pub enum LoadedValue {
    Boolean(bool),
    Number(f64),
    String(Vec<u8>),
    /// A `String` element without `value` (`lua_pushstring(NULL)`).
    Nil,
}

/// What `Game::load` (@ `0x00529e30`) found at `Backup/<name>`.
#[derive(Clone, Debug, PartialEq)]
pub enum SaveOpen {
    /// No file: a new game (the node change goes to `firstWarpPath`).
    Missing,
    /// The file does not parse: `Failed to load file "%s"` and `Game::load`
    /// returns at once, after `3D/OnGameEnter.lua` only.
    Broken,
    /// The file's globals, in file order (the engine keeps the rest for
    /// [`Engine::restore_save`]).
    Globals(Vec<(Vec<u8>, LoadedValue)>),
}

/// A callback argument (`TeLuaScript::Execute` pushes the variant's type).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(Vec<u8>),
    Num(f64),
    Bool(bool),
}

/// Where a Lua error happened.
#[derive(Clone, Debug, PartialEq)]
pub enum ErrorSite {
    /// Compiling a file (`luaL_loadbuffer`); dropped by the original.
    Load(Vec<u8>),
    /// Running a file's chunk under `lua_pcall`; dropped by the original.
    Chunk(Vec<u8>),
    /// A callback the engine called (unprotected in the original: it
    /// prints `Lua: <message>` and exits).
    Callback(String),
}

/// What happened, for logs and call traces.
#[derive(Clone, Debug, PartialEq)]
pub enum Event<'a> {
    /// A file is about to run (its chunk name).
    Load(&'a [u8]),
    /// The file could not be read.
    MissingFile(&'a [u8]),
    /// A Lua error and its message (`lua_tostring` of the error value).
    Error(ErrorSite, &'a [u8]),
    /// A callback is called.
    Enter(&'a str, &'a [Value]),
    /// A callback is not a function: `[TeLuaScript::Execute] La fonction :
    /// "%s" n'existe pas.` in the original's log.
    Missing(&'a str),
    /// `Game::load` makes the game state.
    Game,
    /// The node change starts (path normalised).
    Warp(&'a [u8]),
    /// A boot variable read from a `Texts` file.
    Read(&'a str, Option<&'a BootValue>),
}

/// A value read back from a `TeLuaGUI` state.
#[derive(Clone, Debug, PartialEq)]
pub enum BootValue {
    Str(Vec<u8>),
    Num(f64),
    Bool(bool),
    /// Another type, by name.
    Other(&'static str),
}

/// A key of a layout table.
#[derive(Clone, Debug, PartialEq)]
pub enum GuiKey {
    Name(Vec<u8>),
    Index(f64),
    /// Another kind of key, by type name.
    Other(&'static str),
}

/// A value of a layout table.
#[derive(Clone, Debug, PartialEq)]
pub enum GuiValue {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Vec<u8>),
    /// A layout made by an earlier constructor call (its id).
    Child(usize),
    /// A table such as `size = { 0.5, 0.5 }`, walked in `next` order.
    Table(Vec<(GuiKey, GuiValue)>),
    /// Another type (function, userdata, thread), by name.
    Other(&'static str),
}

/// The engine side. Every method but `call` and `read_script` has a
/// default that does nothing.
pub trait Engine {
    /// A binding call, after the argument checks and conversions.
    fn call(&mut self, call: &Call) -> Reply;

    /// The bytes of a script (`TeLuaScript::load`); `None` if missing.
    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>>;

    /// The file a logical path stands for (the engine's resolver: `HD`,
    /// `PC`, `Full`, language folders).
    fn resolve(&mut self, path: &[u8]) -> Vec<u8> {
        path.to_vec()
    }

    /// Whether a node is shown (`game+0x2c`): the node change then runs
    /// `OnWarpLeave` and autosaves first.
    fn has_scene(&self) -> bool {
        false
    }

    /// After `OnWarpLeave`: unhook the old node.
    fn leave_scene(&mut self) {}

    /// The save (the `SaveGame` worker): the file name and the globals.
    fn save_game(&mut self, _file: &[u8], _globals: &[SavedGlobal]) {}

    /// `Game::load`'s test and parse of `Backup/<file>` (@ `0x00529fd3`):
    /// the default has no saves.
    fn open_save(&mut self, _file: &[u8]) -> SaveOpen {
        SaveOpen::Missing
    }

    /// The rest of the save branch after the globals (@ `0x0052a4d2`..
    /// `0x0052a7ba`): inventory, documents, diary, puzzle timer, warp
    /// name, orientation, music, played time. Returns the warp path the
    /// node change goes to (empty without a `WarpName`).
    fn restore_save(&mut self) -> Vec<u8> {
        Vec::new()
    }

    /// Before the callback reset: stop sounds, build the new node and its
    /// camera (`path` normalised, still `.wp`).
    fn enter_scene(&mut self, _path: &[u8]) {}

    /// After the callback reset: the node's XML.
    fn load_scene_xml(&mut self, _xml: &[u8]) {}

    /// A `TeLuaGUI` constructor (`TeLayout { … }` …) in a layout state:
    /// returns the new layout's id, which scripts see as a light
    /// userdata. `None` entries: the argument was not a table.
    fn gui_layout(&mut self, _class: &str, _entries: Option<&[(GuiKey, GuiValue)]>) -> usize {
        0
    }

    /// Something to log or trace.
    fn event(&mut self, _event: &Event<'_>) {}

    /// Callbacks raised by the engine's own signals during its last call
    /// (a sound's "stopped" signal calls `OnSoundStopped(name)` from
    /// inside `StopSound` or the node change, @ 0x00524de0), run at once
    /// in this order.
    fn take_callbacks(&mut self) -> Vec<(String, Vec<Value>)> {
        Vec::new()
    }
}

//! A recording stub engine: every binding call and event becomes a line of
//! a call trace, in the format of the golden traces made with the
//! reference Lua 5.1.4 (see `tests/golden.rs`).

use crate::engine::{
    Arg, BootValue, Call, Engine, ErrorSite, Event, GuiKey, GuiValue, Reply, SavedGlobal,
    SavedValue, Value,
};
use crate::tolua::Binding;
use crate::trace::{fmt_num, fmt_str, fnv1a, push_escaped};

/// Reads a script by its path; `None` if there is no such file.
pub type Reader = Box<dyn FnMut(&[u8]) -> Option<Vec<u8>>>;

/// The stub. `Selected` answers false, true, false… by call count,
/// `TestObjectLimit` false, `ObjectCount` 0; `ChangeWarp` only records.
pub struct RecordingEngine {
    /// The trace so far, one line per entry.
    pub out: Vec<u8>,
    /// First arguments of `SetMarker` and `PutObject` since the last
    /// clear (the names a walk sends to `OnWarpObjectHit`).
    pub markers: Vec<Vec<u8>>,
    read: Reader,
    /// Print the order hash of saves (meaningful with the collector
    /// stopped only).
    save_order: bool,
    selected: u32,
    scene: bool,
    gui_id: usize,
}

impl RecordingEngine {
    pub fn new(read: Reader, save_order: bool) -> Self {
        Self {
            out: Vec::new(),
            markers: Vec::new(),
            read,
            save_order,
            selected: 0,
            scene: false,
            gui_id: 0,
        }
    }

    /// Appends a line.
    pub fn line(&mut self, s: &[u8]) {
        self.out.extend_from_slice(s);
        self.out.push(b'\n');
    }

    /// The save's lines (`<Kind> <name> <value>`) in walk order.
    pub fn save_lines(globals: &[SavedGlobal]) -> Vec<String> {
        globals
            .iter()
            .map(|g| {
                let (kind, value) = match &g.value {
                    SavedValue::Boolean(_) | SavedValue::Number(_) => {
                        let kind = if matches!(g.value, SavedValue::Boolean(_)) {
                            "Boolean"
                        } else {
                            "Number"
                        };
                        (kind, String::from_utf8_lossy(&g.value.text()).into_owned())
                    }
                    SavedValue::String(s) => {
                        let mut v = String::new();
                        fmt_str(&mut v, s);
                        ("String", v)
                    }
                };
                let mut name = String::new();
                fmt_str(&mut name, &g.name);
                format!("{kind} {name} {value}")
            })
            .collect()
    }

    /// `SAVE` lines and the count, sorted when `as_set`.
    pub fn dump_save(&mut self, globals: &[SavedGlobal], as_set: bool) {
        let mut lines = Self::save_lines(globals);
        if as_set {
            lines.sort();
        }
        for l in &lines {
            self.line(format!("SAVE {l}").as_bytes());
        }
        self.line(format!("SAVE count {}", lines.len()).as_bytes());
    }
}

fn fmt_values(args: &[Value]) -> String {
    let mut s = String::new();
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        match a {
            Value::Str(v) => fmt_str(&mut s, v),
            Value::Num(x) => fmt_num(&mut s, *x),
            Value::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
        }
    }
    s
}

fn fmt_gui_value(s: &mut String, v: &GuiValue) {
    match v {
        GuiValue::Nil => s.push_str("nil"),
        GuiValue::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
        GuiValue::Num(x) => fmt_num(s, *x),
        GuiValue::Str(b) => fmt_str(s, b),
        GuiValue::Child(id) => s.push_str(&format!("gui#{id}")),
        GuiValue::Table(_) => s.push_str("table"),
        GuiValue::Other(t) => s.push_str(t),
    }
}

impl Engine for RecordingEngine {
    fn call(&mut self, call: &Call) -> Reply {
        let mut s = format!("CALL {}(", call.binding.name());
        for (i, a) in call.args.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            match a {
                Arg::Str(Some(v)) => fmt_str(&mut s, v),
                Arg::Str(None) => s.push_str("NULL"),
                Arg::Num(x) => fmt_num(&mut s, *x),
                Arg::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
                Arg::Object => s.push_str("usertype"),
            }
        }
        s.push(')');
        self.line(s.as_bytes());
        if matches!(call.binding, Binding::SetMarker | Binding::PutObject) {
            if let Some(name) = call.str(0) {
                self.markers.push(name.to_vec());
            }
        }
        match call.binding {
            Binding::Selected => {
                self.selected += 1;
                Reply::Bool(self.selected.is_multiple_of(2))
            }
            Binding::TestObjectLimit => Reply::Bool(false),
            Binding::ObjectCount => Reply::Num(0.0),
            _ => Reply::None,
        }
    }

    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        (self.read)(path)
    }

    /// A stand-in for the engine's resolver, enough for the boot files:
    /// `Texts/<file>` is looked for in `Texts/Full/`, `Texts/PC/`, then
    /// `Texts/` (the `part` and `platform` values the application sets).
    fn resolve(&mut self, path: &[u8]) -> Vec<u8> {
        let Some(slash) = path.iter().position(|&c| c == b'/') else {
            return path.to_vec();
        };
        let (dir, file) = (&path[..slash], &path[slash + 1..]);
        if !dir.eq_ignore_ascii_case(b"texts") {
            return path.to_vec();
        }
        for sub in [&b"Texts/Full/"[..], b"Texts/PC/", b"Texts/"] {
            let p = [sub, file].concat();
            if (self.read)(&p).is_some() {
                return p;
            }
        }
        path.to_vec()
    }

    fn has_scene(&self) -> bool {
        self.scene
    }

    fn save_game(&mut self, file: &[u8], globals: &[SavedGlobal]) {
        let lines = Self::save_lines(globals);
        let mut set: u64 = 0;
        let mut order: u64 = 0xcbf2_9ce4_8422_2325;
        for l in &lines {
            let h = fnv1a(l.as_bytes());
            set = set.wrapping_add(h);
            order ^= h;
            order = order.wrapping_mul(0x0000_0100_0000_01b3);
        }
        let mut s = b"SAVEGAME ".to_vec();
        s.extend_from_slice(file);
        s.extend_from_slice(format!(" n={} set={set:016x}", lines.len()).as_bytes());
        if self.save_order {
            s.extend_from_slice(format!(" order={order:016x}").as_bytes());
        }
        self.line(&s);
    }

    fn enter_scene(&mut self, _path: &[u8]) {
        self.scene = true;
    }

    fn load_scene_xml(&mut self, xml: &[u8]) {
        self.line(&[b"SCENE ", xml].concat());
    }

    fn gui_layout(&mut self, class: &str, entries: Option<&[(GuiKey, GuiValue)]>) -> usize {
        let Some(entries) = entries else {
            self.line(format!("GUI {class} not a table").as_bytes());
            return 0;
        };
        self.gui_id += 1;
        let mut s = format!("GUI {class}#{} {{", self.gui_id);
        for (i, (k, v)) in entries.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            match k {
                GuiKey::Name(n) => fmt_str(&mut s, n),
                GuiKey::Index(x) => fmt_num(&mut s, *x),
                GuiKey::Other(t) => s.push_str(t),
            }
            s.push('=');
            fmt_gui_value(&mut s, v);
        }
        s.push('}');
        self.line(s.as_bytes());
        self.gui_id
    }

    fn event(&mut self, event: &Event<'_>) {
        match event {
            Event::Load(p) => self.line(&[b"LOAD ", *p].concat()),
            Event::MissingFile(_) => self.line(b"MISSINGFILE"),
            Event::Error(site, msg) => {
                let mut s = b"ERR ".to_vec();
                match site {
                    ErrorSite::Load(_) => s.extend_from_slice(b"load"),
                    ErrorSite::Chunk(_) => s.extend_from_slice(b"chunk"),
                    ErrorSite::Callback(name) => s.extend_from_slice(name.as_bytes()),
                }
                s.extend_from_slice(b": ");
                push_escaped(&mut s, msg);
                self.line(&s);
            }
            Event::Enter(name, args) => {
                self.line(format!("ENTER {name}({})", fmt_values(args)).as_bytes())
            }
            Event::Missing(name) => self.line(format!("MISSING {name}").as_bytes()),
            Event::Game => self.line(b"GAME"),
            Event::Warp(p) => self.line(&[b"WARP ", *p].concat()),
            Event::Read(var, v) => {
                let mut s = format!("READ {var} ");
                match v {
                    None => s.push_str("nil"),
                    Some(BootValue::Str(b)) => fmt_str(&mut s, b),
                    Some(BootValue::Num(x)) => fmt_num(&mut s, *x),
                    Some(BootValue::Bool(b)) => s.push_str(if *b { "true" } else { "false" }),
                    Some(BootValue::Other(t)) => s.push_str(t),
                }
                self.line(s.as_bytes());
            }
        }
    }
}

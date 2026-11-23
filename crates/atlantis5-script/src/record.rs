//! A recording engine: every call and event as a line of text, in the
//! format of the RE notes' oracle (`C Atl_Name(args)`, `-- <script>`),
//! with the scripts read from a closure.

use crate::engine::{Call, Engine, Event, Reply, Request, Value};

/// Reads a script by the program's path.
pub type Reader = Box<dyn FnMut(&[u8]) -> Option<Vec<u8>>>;

/// Records calls; every call is done at once (films and looks do not
/// wait), `Atl_Display_GetScreenSize` answers 1024 × 576.
pub struct Recorder {
    read: Reader,
    /// The trace, one line per call or note.
    pub lines: Vec<String>,
    /// Errors reported by the game.
    pub errors: Vec<String>,
    /// Calls that wait answer `Wait` when set.
    pub wait: bool,
}

impl Recorder {
    pub fn new(read: Reader) -> Self {
        Self {
            read,
            lines: Vec::new(),
            errors: Vec::new(),
            wait: false,
        }
    }

    /// The call lines only (`C ...`).
    pub fn calls(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| l.starts_with("C "))
            .map(String::as_str)
            .collect()
    }
}

fn args(a: &[Value]) -> String {
    a.iter().map(Value::trace).collect::<Vec<_>>().join(", ")
}

impl Engine for Recorder {
    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        (self.read)(path)
    }

    fn call(&mut self, call: &Call) -> Reply {
        match call {
            Call::DisplayGetScreenSize => Reply::Values(vec![1024.0, 576.0]),
            Call::VideoPlay { .. } | Call::CameraLookAt { animated: true, .. } if self.wait => {
                Reply::Wait
            }
            _ => Reply::Done,
        }
    }

    fn request(&mut self, r: &Request) {
        if let Request::LoadView { scene, view } = r {
            self.lines.push(format!(
                "R LoadView({}, {view})",
                String::from_utf8_lossy(scene)
            ));
        }
    }

    fn event(&mut self, e: &Event<'_>) {
        match e {
            Event::Call(b, a) => self.lines.push(format!("C {}({})", b.name(), args(a))),
            Event::Run(p) => self
                .lines
                .push(format!("-- {}", String::from_utf8_lossy(p))),
            Event::Missing(p) => self
                .lines
                .push(format!("missing {}", String::from_utf8_lossy(p))),
            Event::Rejected(b, m) => self.lines.push(format!("rejected {}: {m}", b.name())),
            Event::Error(site, m) => {
                let l = format!("error {site:?}: {}", String::from_utf8_lossy(m));
                self.errors.push(l.clone());
                self.lines.push(l);
            }
            Event::End(_) | Event::Note(_) => {}
        }
    }
}

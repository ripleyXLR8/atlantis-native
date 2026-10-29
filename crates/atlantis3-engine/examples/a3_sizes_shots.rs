//! Developers' aid: screenshots (PNG) of the screens that follow the
//! screen size, headless, with the install's `Atlantis3.cfg` given another
//! `Resolution`: the Land of the Dead's map at 640 x 480 and 1024 x 768,
//! the credits at 1024 x 768.
//!
//! `cargo run --release -p atlantis3-engine --example a3_sizes_shots -- <game folder> <out folder>`

use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis3_data::A3Files;
use atlantis3_engine::Game;
use atlantis_data::files::{FileSource, StdDir};

/// The install with its `Atlantis3.cfg` followed by a `Resolution` line.
struct Resized {
    dir: StdDir,
    cfg: Vec<u8>,
}

fn is_cfg(name: &str) -> bool {
    name.replace('\\', "/")
        .eq_ignore_ascii_case("Datas/Atlantis3.cfg")
}

impl FileSource for Resized {
    fn size(&self, name: &str) -> Option<u64> {
        if is_cfg(name) {
            return Some(self.cfg.len() as u64);
        }
        self.dir.size(name)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        if is_cfg(name) {
            let start = usize::try_from(offset).ok()?;
            return self
                .cfg
                .get(start..start.checked_add(len)?)
                .map(<[u8]>::to_vec);
        }
        self.dir.read_at(name, offset, len)
    }

    fn has_dir(&self, name: &str) -> bool {
        self.dir.has_dir(name)
    }

    fn list(&self, dir: &str) -> Vec<String> {
        self.dir.list(dir)
    }
}

struct Run {
    g: Game,
    t: f64,
}

impl Run {
    fn new(dir: &Path, w: usize, h: usize) -> Run {
        let std_dir = StdDir::new(dir);
        let mut cfg = std_dir.read("Datas/Atlantis3.cfg").unwrap_or_default();
        cfg.extend_from_slice(format!("\r\nResolution\t\t= {w} {h}\r\n").as_bytes());
        let files = A3Files::new(Arc::new(Resized { dir: std_dir, cfg }));
        let mut g = Game::new(files).expect("the game starts");
        g.seed(1);
        g.new_game();
        let mut r = Run { g, t: 0.0 };
        r.frames(200);
        r
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.t += 1.0 / 30.0;
            self.g.step(self.t);
            if self.g.video().is_some() {
                self.g.finish_video();
            }
            if self.g.dialog_running() {
                self.g.finish_dialog();
            }
        }
    }

    fn shot(&self, out: &Path, name: &str) {
        let png =
            atlantis2_engine::png::encode_rgb565(self.g.screen(), self.g.width(), self.g.height());
        std::fs::write(out.join(format!("{name}.png")), png).unwrap();
        println!("{name} {}x{}", self.g.width(), self.g.height());
    }

    /// The Land of the Dead's map (flag 80: the priest seen), the pointer
    /// on a label.
    fn map(&mut self) {
        for c in [
            "(monde=2)",
            "(changewam=LDMorts.wam)",
            "(vue=5)",
            "(chapter=12)",
        ] {
            self.g.run_command(c);
        }
        self.frames(30);
        self.g.script_mut().unwrap().flags[80] = 1;
        self.g.run_command("(ObjetEnMain=LDM_CARTELIEUX)");
        self.frames(10);
        let (w, h) = (self.g.width() as i32, self.g.height() as i32);
        self.g.set_pointer(w / 2 - 60, h / 2 + 100);
        self.frames(10);
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("game folder"));
    let out = PathBuf::from(args.next().expect("output folder"));
    std::fs::create_dir_all(&out).unwrap();
    for (w, h) in [(640, 480), (1024, 768)] {
        let mut r = Run::new(&dir, w, h);
        r.map();
        r.shot(&out, &format!("map_{w}"));
    }
    let mut r = Run::new(&dir, 1024, 768);
    r.g.run_command("(exit=1)");
    r.frames(30 * 5);
    r.shot(&out, "credits_1024");
}

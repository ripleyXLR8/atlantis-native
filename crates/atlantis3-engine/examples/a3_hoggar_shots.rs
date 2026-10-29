//! Developers' aid: plays the start of the Hoggar headless (the
//! cinematics skipped) and writes screenshots (PNG): the subject menu, a
//! dialog with its head and subtitle, the inventory, the save page.
//!
//! `cargo run --release -p atlantis3-engine --example a3_hoggar_shots -- <game folder> <out folder>`

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis3_data::A3Files;
use atlantis3_engine::subjects::Layout;
use atlantis3_engine::{Game, Mode};

struct Run {
    g: Game,
    t: f64,
    out: PathBuf,
}

impl Run {
    fn frame(&mut self) {
        self.t += 1.0 / 30.0;
        self.g.step(self.t);
    }

    /// Frames until the warp is still, videos skipped, dialogs played.
    fn settle(&mut self) {
        for _ in 0..30_000 {
            self.frame();
            if self.g.video().is_some() {
                self.g.finish_video();
            }
            if self.g.mode() == Mode::Warp && !self.g.turning() && !self.g.move_pending() {
                for _ in 0..3 {
                    self.frame();
                }
                if self.g.mode() == Mode::Warp {
                    return;
                }
            }
        }
    }

    fn go(&mut self, view: i32) {
        assert!(
            self.g.aim_at_zone(|z| z.kind == 1 && z.value == view),
            "zone to {view}"
        );
        self.g.click();
        self.settle();
    }

    fn save(&self, name: &str) {
        let png =
            atlantis2_engine::png::encode_rgb565(self.g.screen(), self.g.width(), self.g.height());
        std::fs::write(self.out.join(format!("{name}.png")), png).unwrap();
        println!("{name}");
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("game folder"));
    let out = PathBuf::from(args.next().expect("output folder"));
    std::fs::create_dir_all(&out).unwrap();
    let files = A3Files::open_dir(&dir).expect("an Atlantis III install").0;
    let g = Game::new(files).expect("the game starts");
    let mut r = Run { g, t: 0.0, out };
    r.g.seed(1);
    if r.g.show_start_page() {
        for _ in 0..10 {
            r.frame();
        }
        r.save("start-page");
    }
    r.g.new_game();
    r.settle();
    r.go(5);
    r.go(6);
    r.go(7);
    // The guard: the subject menu, then the dialog.
    let guard =
        r.g.names()
            .id(defnames::PERSONNAGES, b"HOG_GARDIEN01")
            .unwrap() as i32;
    assert!(r.g.click_sprite(guard, 1));
    r.frame();
    for _ in 0..5 {
        r.frame();
    }
    r.save("subject-menu");
    let lay = Layout::for_width(r.g.width());
    let s = r.g.script().unwrap();
    let cell = lay.cells(&s.subjects.list(guard))[0];
    r.g.set_pointer(cell.x, cell.y);
    r.g.click();
    let mut shots = 0;
    while r.g.dialog_running() || shots == 0 {
        r.frame();
        if let Some((d, l)) = r.g.dialog_line() {
            let name = format!("dialog-{d:03}-{l}");
            if shots < 4 && !std::path::Path::new(&r.out.join(format!("{name}.png"))).exists() {
                for _ in 0..40 {
                    r.frame();
                }
                r.save(&name);
                shots += 1;
            }
        }
        if r.t > 600.0 {
            break;
        }
    }
    r.settle();
    // The inventory.
    r.g.right_click();
    for _ in 0..10 {
        r.frame();
    }
    r.g.set_pointer(r.g.width() as i32 / 2, r.g.height() as i32 - 120);
    r.frame();
    r.save("inventory");
    r.g.right_click();
    r.settle();
    if r.g.show_save_page() {
        for _ in 0..10 {
            r.frame();
        }
        r.save("save-page");
    }
}

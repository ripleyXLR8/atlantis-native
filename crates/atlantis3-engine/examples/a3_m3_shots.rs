//! Developers' aid: screenshots (PNG) of the M3 screens, headless: the
//! in-game menu, the save confirmation, the load page with its thumbnails,
//! the Land of the Dead's map, the tower's board, the credits.
//!
//! `cargo run --release -p atlantis3-engine --example a3_m3_shots -- <game folder> <out folder>`

use std::path::PathBuf;

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

struct Run {
    g: Game,
    t: f64,
    out: PathBuf,
}

impl Run {
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

    fn shot(&self, name: &str) {
        let png =
            atlantis2_engine::png::encode_rgb565(self.g.screen(), self.g.width(), self.g.height());
        std::fs::write(self.out.join(format!("{name}.png")), png).unwrap();
        println!("{name}");
    }

    /// A click (press, a frame, release) on the menu page's element `name`.
    fn click_element(&mut self, name: &[u8]) -> bool {
        let Some((page, _)) = self.g.menu_page() else {
            return false;
        };
        let (w, h) = (self.g.width() as i32, self.g.height() as i32);
        let at = (0..h).step_by(4).find_map(|y| {
            (0..w)
                .step_by(4)
                .find(|&x| {
                    self.g
                        .menus()
                        .get(page)
                        .and_then(|p| p.clicked((x, y)))
                        .is_some_and(|n| n == name)
                })
                .map(|x| (x, y))
        });
        let Some((x, y)) = at else {
            eprintln!("{}: not found", String::from_utf8_lossy(name));
            return false;
        };
        self.g.set_pointer(x, y);
        self.frames(2);
        self.g.click();
        self.frames(1);
        self.g.release();
        self.frames(2);
        true
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("game folder"));
    let out = PathBuf::from(args.next().expect("output folder"));
    std::fs::create_dir_all(&out).unwrap();
    let files = || A3Files::open_dir(&dir).expect("an Atlantis III install").0;
    let mut r = Run {
        g: Game::new(files()).expect("the game starts"),
        t: 0.0,
        out: out.clone(),
    };
    r.g.seed(1);
    r.g.new_game();
    r.frames(200);
    // The in-game menu, three saves, the load page.
    r.g.escape();
    r.frames(60);
    r.shot("menu-ingame");
    for k in 0..3 {
        if !r.click_element(b"BT_SAVE") {
            break;
        }
        r.frames(45);
        if k == 0 {
            r.shot("menu-save");
        }
        r.click_element(b"BT_YES");
        r.frames(30);
    }
    if r.click_element(b"BT_LOAD") {
        r.frames(45);
        r.shot("menu-load");
    }
    r.click_element(b"BT_BACK");
    r.frames(30);
    if r.g.menu_page().is_some() {
        r.click_element(b"BT_BACK");
        r.frames(10);
    }
    // The options page.
    r.g.escape();
    r.frames(40);
    if r.click_element(b"BT_OPTION") {
        r.frames(45);
        r.shot("menu-options");
    }

    // The Land of the Dead's map (flag 80: the priest seen).
    let mut r = Run {
        g: Game::new(files()).expect("the game starts"),
        t: 0.0,
        out: out.clone(),
    };
    r.g.seed(1);
    r.g.new_game();
    r.frames(200);
    for c in [
        "(monde=2)",
        "(changewam=LDMorts.wam)",
        "(vue=5)",
        "(chapter=12)",
    ] {
        r.g.run_command(c);
    }
    r.frames(30);
    r.g.script_mut().unwrap().flags[80] = 1;
    r.g.run_command("(ObjetEnMain=LDM_CARTELIEUX)");
    r.frames(10);
    r.g.set_pointer(r.g.width() as i32 / 2 - 60, r.g.height() as i32 / 2 + 100);
    r.frames(10);
    println!("mode {:?} screen {:?}", r.g.mode(), r.g.puzzle_screen());
    r.shot("map-ldm");
    r.g.right_click();
    r.frames(10);

    // The tower's board.
    for c in [
        "(monde=3)",
        "(changewam=TourMagi.wam)",
        "(vue=39)",
        "(chapter=47)",
    ] {
        r.g.run_command(c);
    }
    r.frames(30);
    r.g.run_command("(ObjetEnMain=BAG_MAP_TOUR)");
    r.frames(20);
    println!("mode {:?} screen {:?}", r.g.mode(), r.g.puzzle_screen());
    r.shot("map-tower");
    r.g.click();
    r.frames(5);

    // The end: the credits page (EXIT=1).
    r.g.run_command("(exit=1)");
    r.frames(30);
    println!("mode {:?}", r.g.mode());
    if r.g.mode() == Mode::Menu {
        r.shot("credits-1");
        r.frames(30 * 7);
        r.shot("credits-2");
    }
}

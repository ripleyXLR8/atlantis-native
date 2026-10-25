//! A headless trip to Yucatán: Yucatán's boat laid in the shaman's hut,
//! the departure film, the arrival in `yucatan.wam` view 10, the link to
//! the king's hall (its arrival turned by `AngleWarpXNow`, dialog 101),
//! a character's subject menu and answer, the priestess, the pyramid's first
//! counting grid (empty, then five marbles laid with four in hand), the Tezcatlipoca statue's
//! menu (in Xibalba's `xibalba.wam`, loaded directly for the picture;
//! `tests/flow.rs` reaches it as a player does), and the brazier's way
//! back to Tibet. Some
//! steps put the game where they need it (a place, a view, a chapter, an
//! object in hand) instead of walking there.
//!
//! Usage: `yucatan_tour <install dir> <discs dir> <out dir>`; writes
//! `yucatan-*.png`. 59.94 passes a second, a 15 Hz tick every fourth.

use std::path::{Path, PathBuf};

use atlantis2_data::defnames;
use atlantis2_data::A2Files;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::subjects::cell;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const W: usize = 640;
const H: usize = 480;

struct Tour {
    e: Engine,
    screen: Vec<u16>,
    pass: u64,
    out: PathBuf,
}

impl Tour {
    fn now(&self) -> f64 {
        self.pass as f64 * PASS
    }

    fn step(&mut self) {
        if self.pass.is_multiple_of(4) {
            self.e.tick();
        }
        self.e.set_clock(self.now());
        self.e.pass(Some(&mut self.screen));
        self.pass += 1;
    }

    fn wait(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    fn click(&mut self) {
        let t = self.now() + 100.0 * self.pass as f64;
        self.e.click(t);
    }

    fn save(&self, name: &str) {
        let path = self.out.join(name);
        std::fs::write(&path, encode_rgb565(&self.screen, W, H)).unwrap();
        println!(
            "{} ({:?}, view {}, chapter {})",
            path.display(),
            self.e.place().0,
            self.e.place().1,
            self.e.script_state().map_or(-1, |s| s.chapter)
        );
    }

    fn id(&self, section: usize, name: &str) -> i32 {
        let s = self.e.script_state().unwrap();
        s.names.id(section, name.as_bytes()).unwrap() as i32
    }

    /// The centre on the first zone `pick` accepts.
    fn aim(&mut self, pick: impl Fn(&atlantis2_data::wam::Zone) -> bool) {
        let view = self.e.place().1;
        let zi = self
            .e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(pick)
            .expect("the zone");
        let (yaw, pitch) = self.e.zone_aim(view, zi).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
    }

    /// Plays until the loop type is `want` (at most `n` passes).
    fn until(&mut self, want: LoopType, n: u64) {
        for _ in 0..n {
            if self.e.loop_type == want {
                return;
            }
            self.step();
        }
    }

    /// A dialog played to its end, a click every half second.
    fn listen(&mut self) {
        for i in 0..20_000 {
            if self.e.loop_type != LoopType::Dialog {
                break;
            }
            if i % 30 == 29 {
                self.click();
            }
            self.step();
        }
    }

    /// Films and dialogs played out until the panorama is back.
    fn settle(&mut self) {
        for _ in 0..20_000 {
            match self.e.loop_type {
                LoopType::Video => {
                    self.wait(30);
                    self.click();
                    self.step();
                }
                LoopType::Dialog => self.listen(),
                _ => break,
            }
        }
        self.wait(20);
    }

    /// The game moved to a place, view and chapter, as a section does.
    fn goto(&mut self, place: &str, view: u32, chapter: i32) {
        for c in [
            format!("(changewam={place})"),
            format!("(vue={view})"),
            format!("(chapter={chapter})"),
        ] {
            self.e.run_command(c.as_bytes());
        }
        self.settle();
    }

    /// Turns the view until a character's sprite is under the centre.
    fn find_sprite(&mut self) -> bool {
        for pitch in (-8..=8).map(|p| p * 5) {
            for yaw in (0..72).map(|y| y * 5 - 180) {
                let (y, p) = ((yaw as f32).to_radians(), (pitch as f32).to_radians());
                self.e.camera.enter(y, p);
                self.step();
                if self.e.sprite_under_centre().1 == 2 {
                    return true;
                }
            }
        }
        false
    }
}

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [install, discs, out] = &args[..] else {
        eprintln!("usage: yucatan_tour <install dir> <discs dir> <out dir>");
        std::process::exit(2);
    };
    std::fs::create_dir_all(out).unwrap();
    let (files, _) =
        A2Files::open_dir(Path::new(install), std::slice::from_ref(discs)).expect("game folder");
    let mut e = Engine::boot(
        files,
        Variant::default(),
        Fpu::default(),
        W as u32,
        H as u32,
    )
    .unwrap();
    e.trace = true;
    e.start_new_game().unwrap();
    e.end_video();
    let mut t = Tour {
        e,
        screen: vec![0; W * H],
        pass: 0,
        out: out.clone(),
    };
    t.wait(10);

    // The hut, chapter 75 (a click on the crystal's zone), Yucatán's boat
    // in hand, laid on its zone: the departure film.
    t.e.enter_at(10, 0.0, 0.0).unwrap();
    t.e.script_state_mut().unwrap().set_current_view(10);
    t.e.reset_view();
    t.aim(|z| {
        z.name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case("ZONEPoseCristal"))
    });
    t.click();
    t.wait(5);
    t.e.run_command(b"(sacados=MANDALA_BATEAU_YU)");
    t.e.run_command(b"(objetenmain=MANDALA_BATEAU_YU)");
    t.aim(|z| z.name.as_deref() == Some("ZONE_YUB"));
    t.click();
    t.wait(200);
    t.save("yucatan-01-departure-film.png");
    t.click();
    t.until(LoopType::Warp, 600);
    t.wait(60);
    t.save("yucatan-02-arrival-view10.png");

    // View 39's link to the king's hall (view 81): the arrival turned
    // (`AngleWarpXNow=270`), dialog 101.
    t.goto("yucatan.wam", 39, 25);
    t.save("yucatan-03-view39.png");
    t.aim(|z| z.kind == 2 && z.value == 81);
    t.click();
    t.until(LoopType::Dialog, 900);
    t.wait(150);
    t.save("yucatan-04-kings-hall-dialog.png");
    t.listen();
    t.wait(20);
    t.save("yucatan-05-kings-hall-turned.png");

    // The prince and the high priest (view 82): the menu of the first
    // found, his answer.
    t.goto("salleroi.wam", 82, 26);
    if t.find_sprite() {
        t.click();
        t.wait(2);
        let s = t.e.script_state().unwrap();
        let who = s.var("CLICPERSO").unwrap();
        let count = s.subjects.list(who).len() as i32;
        let (x, y) = cell(count, 0);
        let (px, py) = t.e.pointer;
        t.e.mouse_motion(x + 31 - px, y + 31 - py);
        t.wait(2);
        t.save("yucatan-06-kings-hall-subjects.png");
        t.click();
        t.until(LoopType::Dialog, 30);
        t.wait(200);
        t.save("yucatan-07-kings-hall-answer.png");
        t.listen();
    }

    // The priestess (view 84, chapter 27: its `/INIT` dialog first).
    t.goto("salleroi.wam", 84, 27);
    t.settle();
    t.save("yucatan-08-priestess.png");

    // The pyramid's first counting grid (view 55, chapter 31): empty,
    // then five marbles laid on the first cell and four taken in hand
    // (the hand of marbles drawn at the centre).
    t.goto("pyratez.wam", 55, 31);
    t.save("yucatan-09-grid.png");
    for v in [18, 1, 17, 17, 17, 17] {
        t.aim(|z| z.kind == 50 && z.value == v);
        t.click();
        t.step();
    }
    t.save("yucatan-09b-grid-marbles.png");

    // The Tezcatlipoca statue (Xibalba's view 122, chapter 39 once the
    // star map is done): a click on its zone opens its subject menu.
    t.e.load_place("xibalba.wam");
    t.e.enter_at(122, 0.0, 0.0).unwrap();
    t.e.run_command(b"(etoiles_fini=1)");
    t.e.run_command(b"(chapter=39)");
    t.settle();
    t.aim(|z| {
        z.name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case("ClicTezStatue"))
    });
    t.click();
    t.wait(2);
    let statue = t.id(defnames::PERSONNAGES, "TEZ_STATUE");
    let count = t.e.script_state().unwrap().subjects.list(statue).len() as i32;
    let (x, y) = cell(count, 0);
    let (px, py) = t.e.pointer;
    t.e.mouse_motion(x + 31 - px, y + 31 - py);
    t.wait(2);
    t.save("yucatan-10-statue-subjects.png");
    t.click();
    t.until(LoopType::Dialog, 30);
    t.wait(200);
    t.save("yucatan-11-statue-dialog.png");
    t.listen();

    // Back in Yucatán (chapter 30): the brazier of view 35 clicked with an
    // empty hand, the way back to Tibet.
    t.e.run_command(b"(monde=2)");
    t.goto("yucatan.wam", 35, 30);
    t.save("yucatan-12-brazier.png");
    t.e.run_command(b"(objetenmain=0)");
    t.aim(|z| z.name.as_deref() == Some("BRAZERO_ZONE"));
    t.click();
    t.wait(30);
    t.settle();
    t.save("yucatan-13-back-in-tibet.png");
    let refused: Vec<String> =
        t.e.events
            .iter()
            .filter_map(|e| match e {
                Event::WorldNotPorted(w) => Some(w.clone()),
                _ => None,
            })
            .collect();
    let noted: Vec<String> =
        t.e.events
            .iter()
            .filter_map(|e| match e {
                Event::PuzzleNotPorted { function } => Some(format!("{function:08x}")),
                _ => None,
            })
            .collect();
    println!("refused places: {refused:?}");
    println!("puzzle hooks noted: {noted:?}");
}

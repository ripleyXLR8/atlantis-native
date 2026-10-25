//! A headless trip to China: China's boat laid in the shaman's hut, the
//! departure film, the arrival in `chine.wam` view 10, the gardener master
//! and his subject menu, his answer, the turtle room with the picture
//! viewer (`ViewBigSpr`), the miniature garden, a move with its zoom
//! (`ZoomStart`), the labyrinth and a demon's special event, chapter 59's
//! door (`NextWarpInvalide`), and the gong's way back to Tibet. Some steps
//! put the game where they need it (a place, a view, a chapter, an object
//! in hand) instead of walking there.
//!
//! Usage: `china_tour <install dir> <discs dir> <out dir>`; writes
//! `china-*.png`. 60 passes a second, a 15 Hz tick every fourth.

use std::path::{Path, PathBuf};

use atlantis2_data::defnames;
use atlantis2_data::A2Files;
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
        self.pass as f64 / 60.0
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
        eprintln!("usage: china_tour <install dir> <discs dir> <out dir>");
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

    // The hut, chapter 75 (a click on the crystal's zone), China's boat in
    // hand, laid on its zone: the departure film.
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
    t.e.run_command(b"(sacados=MANDALA_BATEAU_CH)");
    t.e.run_command(b"(objetenmain=MANDALA_BATEAU_CH)");
    t.aim(|z| z.name.as_deref() == Some("ZONE_CHB"));
    t.click();
    t.wait(200);
    t.save("china-01-departure-film.png");
    t.click();
    t.until(LoopType::Warp, 600);
    t.wait(60);
    t.save("china-02-arrival-view10.png");

    // The gardener master (view 12), his subject menu, his answer.
    t.e.run_command(b"(vue=12)");
    t.wait(20);
    if t.find_sprite() {
        t.click();
        t.wait(2);
        let gardener = t.id(defnames::PERSONNAGES, "MAITRE_JARDIN");
        let count = t.e.script_state().unwrap().subjects.list(gardener).len() as i32;
        let (x, y) = cell(count, 0);
        let (px, py) = t.e.pointer;
        t.e.mouse_motion(x + 31 - px, y + 31 - py);
        t.wait(2);
        t.save("china-03-gardener-subjects.png");
        t.click();
        t.until(LoopType::Dialog, 30);
        t.wait(200);
        t.save("china-04-gardener-dialog.png");
        t.listen();
    }

    // The turtle room (chapter 46): the picture of the turtle until a
    // click.
    t.goto("salletortue.wam", 56, 46);
    t.save("china-05-turtle-room.png");
    t.aim(|z| z.name.as_deref() == Some("TGA_TORTUE"));
    t.click();
    t.wait(10);
    t.save("china-06-picture-zoomtor.png");
    t.click();
    t.wait(10);

    // The miniature garden (chapter 47) and its map.
    t.goto("miniature.wam", 1, 47);
    t.save("china-07-miniature.png");
    t.e.run_command(b"(ViewBigSpr=CarteChi.spr)");
    t.wait(10);
    t.save("china-08-picture-map.png");
    t.click();
    t.wait(10);

    // View 7 of chapter 48: the move to view 10 zooms in (`ZoomStart`).
    t.goto("miniature.wam", 7, 48);
    t.aim(|z| z.kind == 1 && z.value == 10);
    t.click();
    let mut zoom_shot = false;
    for _ in 0..600 {
        t.step();
        if !zoom_shot && t.e.zoom.current < 55.0 && t.e.zoom.current > 45.0 {
            t.save("china-09-zoom.png");
            zoom_shot = true;
        }
        if t.e.loop_type == LoopType::Video {
            break;
        }
    }
    t.settle();
    t.save("china-10-after-zoom.png");

    // Chapter 59's door: miniature view 22 to 122, the view entry skipped,
    // back to view 22.
    t.goto("miniature.wam", 22, 59);
    t.aim(|z| z.kind == 1 && z.value == 122);
    t.click();
    for _ in 0..600 {
        t.step();
        if t.e.loop_type == LoopType::Video {
            t.wait(30);
            t.save("china-11-door-video.png");
            break;
        }
    }
    t.settle();
    t.save("china-12-door-back-in-22.png");

    // The labyrinth (chapter 60): the rat demon given an empty form, its
    // special event to its end, the stamped form back.
    t.goto("labysol.wam", 25, 60);
    t.save("china-13-labyrinth-rat.png");
    for c in [
        &b"(MemoNumDial=333)"[..],
        b"(tampondemon=1)",
        b"(ContinueAnim=DEMON_RAT)",
        b"(TestSpecialEvent=1)",
    ] {
        t.e.run_command(c);
    }
    for _ in 0..4000 {
        t.step();
        if t.e.loop_type == LoopType::Dialog {
            t.wait(150);
            t.save("china-14-special-event-dialog.png");
            break;
        }
    }
    t.listen();
    t.wait(10);

    // Back in chine.wam (chapter 42): the gong in hand, struck in view 5.
    t.goto("chine.wam", 5, 42);
    t.e.run_command(b"(sacados=GONG)");
    t.e.run_command(b"(objetenmain=GONG)");
    t.aim(|z| z.name.as_deref() == Some("GONG_ZONE"));
    t.click();
    t.wait(30);
    t.settle();
    t.save("china-15-back-in-tibet.png");
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

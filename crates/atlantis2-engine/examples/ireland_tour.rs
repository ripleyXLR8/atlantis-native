//! A headless trip to Ireland: Ireland's boat laid in the shaman's hut,
//! the departure film, the arrival in `bigile1.wam` view 5 with its
//! dialog, Liam and his subject menu, a transition video with its
//! sprites, the place of Brigid's cross, the goddess's film and dialog
//! once the cross is solved by the testing aid, and the way back to
//! Tibet. Some steps put the game where they need it (a view entered, an
//! object in hand, a story flag) instead of walking there.
//!
//! Usage: `ireland_tour <install dir> <discs dir> <out dir>`; writes
//! `ireland-*.png`. 60 passes a second, a 15 Hz tick every fourth.

use std::path::{Path, PathBuf};

use atlantis2_data::defnames;
use atlantis2_data::A2Files;
use atlantis2_engine::engine::IrishPuzzle;
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

    /// The centre on the zone named `name` (or of value `value`).
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
        eprintln!("usage: ireland_tour <install dir> <discs dir> <out dir>");
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

    // The hut, chapter 75 (a click on the crystal's zone), Ireland's boat
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
    t.e.run_command(b"(sacados=MANDALA_BATEAU_IR)");
    t.e.run_command(b"(objetenmain=MANDALA_BATEAU_IR)");
    t.aim(|z| z.name.as_deref() == Some("ZONE_IRB"));
    t.click();
    t.wait(2);
    t.wait(200);
    t.save("ireland-1-departure-film.png");
    t.click();
    // Ireland: view 5, the first arrival's dialog.
    t.until(LoopType::Dialog, 300);
    t.wait(150);
    t.save("ireland-2-arrival-dialog.png");
    for i in 0..20_000 {
        if t.e.loop_type != LoopType::Dialog {
            break;
        }
        if i % 30 == 29 {
            t.click();
        }
        t.step();
    }
    t.wait(30);
    // Liam: turned to, clicked: his subject menu, the pointer on it.
    if t.find_sprite() {
        t.save("ireland-3-liam.png");
        t.click();
        t.wait(2);
        let liam = t.id(defnames::PERSONNAGES, "LIAM_SCRIPT");
        let count = t.e.script_state().unwrap().subjects.list(liam).len() as i32;
        let (x, y) = cell(count, 0);
        let (px, py) = t.e.pointer;
        t.e.mouse_motion(x + 31 - px, y + 31 - py);
        t.wait(2);
        t.save("ireland-4-subject-menu.png");
        t.click();
        t.until(LoopType::Dialog, 30);
        t.wait(200);
        t.save("ireland-5-liam-dialog.png");
        for i in 0..20_000 {
            if t.e.loop_type != LoopType::Dialog {
                break;
            }
            if i % 30 == 29 {
                t.click();
            }
            t.step();
        }
    }
    // A transition with its sprites: view 7 to view 8 (Liam and a hen
    // drawn over the video).
    t.e.enter_at(7, 0.0, 0.0).unwrap();
    t.wait(10);
    t.aim(|z| z.kind == 1 && z.value == 8);
    t.click();
    t.until(LoopType::Video, 300);
    t.wait(60);
    let sprites = t.e.transition_sprites();
    t.save("ireland-6-transition-sprites.png");
    println!("transition 7 -> 8: {sprites} sprite banks over the video");
    t.until(LoopType::Warp, 2000);
    t.wait(30);
    t.save("ireland-7-view8.png");
    // The place of Brigid's cross (view 2), chapter 16.
    t.e.run_command(b"(chapter=16)");
    t.e.enter_at(2, 0.0, 0.0).unwrap();
    t.e.script_state_mut().unwrap().set_current_view(2);
    t.e.reset_view();
    t.wait(30);
    t.save("ireland-8-cross-view.png");
    // The cross solved by the testing aid: the goddess's film and dialog.
    t.e.solve_irish_puzzle(IrishPuzzle::Cross);
    t.wait(150);
    t.save("ireland-9-goddess-film.png");
    t.click();
    t.until(LoopType::Dialog, 300);
    t.wait(150);
    t.save("ireland-10-goddess-dialog.png");
    for i in 0..20_000 {
        if t.e.loop_type != LoopType::Dialog {
            break;
        }
        if i % 30 == 29 {
            t.click();
        }
        t.step();
    }
    t.wait(20);
    // The way back (chapter 16, once the story's flag is set): view 2 to
    // view 1, film 19 with dialog 71 over it, Tibet's view 7.
    t.e.run_command(b"(flag71=1)");
    t.wait(5);
    t.aim(|z| z.kind == 1 && z.value == 1);
    t.click();
    t.until(LoopType::Video, 300);
    t.wait(200);
    t.save("ireland-11-return-film.png");
    t.click();
    t.until(LoopType::Warp, 2000);
    t.wait(60);
    t.save("ireland-12-back-in-tibet.png");
    let names: Vec<String> =
        t.e.events
            .iter()
            .filter_map(|e| match e {
                Event::WorldNotPorted(w) => Some(w.clone()),
                _ => None,
            })
            .collect();
    println!("refused places: {names:?}");
}

//! The Traditional and Simplified Chinese CDs (atlantis-cd-zh-tw-696832,
//! atlantis-cd-zh-cn-692736) played from their disc images: the start-up,
//! the player screen, a new game up to the in-game menu. The Chinese
//! subtitles, menus and captions are not drawn yet; the game runs.
//!
//! Needs the folders of the discs' images: set `ATLANTIS_ISO_ZH_TW` (the
//! Traditional CD's ISO images) and `ATLANTIS_BIN_ZH_CN` (the Simplified
//! CD's raw images); without them the tests are skipped.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::Game;
use atlantis_engine::audio::Mixer;
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::{Event, MemStore, Options, Start};

fn game(var: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(var) else {
        eprintln!("{var} not set: skipped");
        return None;
    };
    let set = DiscSet::open_folder(dir.as_ref()).expect("CD images");
    Some(Game::open(Arc::new(set)).expect("the game from its images"))
}

struct Headless {
    sys: Rc<Sys>,
    runner: Runner,
    now: u64,
    sound: Vec<f32>,
    alive: bool,
}

impl Headless {
    fn step(&mut self) -> bool {
        self.alive = self.runner.step(self.now);
        self.sys.mixer.fill(&mut self.sound, 22050, 2);
        self.now += 16;
        self.alive
    }

    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            if !self.step() {
                break;
            }
        }
    }

    fn click(&mut self, x: i32, y: i32) {
        let button = MouseButton::Left;
        self.sys.push_input(Input::MouseMove { x, y });
        self.sys.push_input(Input::MouseDown { button, x, y });
        self.step();
        self.sys.push_input(Input::MouseUp { button, x, y });
        self.steps(3);
    }

    fn key(&mut self, k: Key) {
        self.sys.push_input(Input::KeyDown(k));
        self.sys.push_input(Input::KeyUp(k));
        self.steps(2);
    }

    fn until(&mut self, n: usize, mut f: impl FnMut(&Headless) -> bool) -> bool {
        for _ in 0..n {
            if f(self) || !self.step() {
                return f(self);
            }
        }
        false
    }
}

/// From the start-up to the in-game menu: the logos cut short, the player
/// screen, a new player, the first chapter, Escape.
fn boot_to_menu(var: &str, build: &str) {
    let Some(g) = game(var) else { return };
    assert_eq!(g.exe.build.name, build);
    let sys = Sys::new(Mixer::new());
    let trace = Rc::new(RefCell::new(Vec::new()));
    let opts = Options {
        trace: Some(trace.clone()),
        saves: Rc::new(MemStore::new()),
        start: Start::Boot,
        ..Options::default()
    };
    let mut h = Headless {
        runner: Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts)),
        sys: sys.clone(),
        now: 0,
        sound: vec![0.0; 2 * (22050 * 16 / 1000)],
        alive: true,
    };
    let menus = |t: &Rc<RefCell<Vec<Event>>>, m: &str| {
        t.borrow()
            .iter()
            .filter(|e| matches!(e, Event::Menu(n) if n == m))
            .count()
    };
    // LOGO15N and PREINTRO cut short, up to the player screen.
    let mut player_screen = false;
    for _ in 0..100 {
        h.steps(30);
        if menus(&trace, "SELEMENU.TXT") > 0 {
            player_screen = true;
            break;
        }
        h.click(320, 240);
    }
    assert!(player_screen, "{build}: no player screen");
    // The "new player" row, a name, Enter: the first chapter.
    h.steps(10);
    h.click(200, 259);
    for c in "Tester".chars() {
        h.key(Key::Char(c));
    }
    h.steps(2);
    h.key(Key::Enter);
    assert!(
        h.until(2000, |_| trace.borrow().contains(&Event::ChapterLoaded(1))),
        "{build}: chapter 1 not loaded"
    );
    // Past the opening (a click ends each line), Escape: the in-game menu.
    let mut menu_up = false;
    for _ in 0..300 {
        h.steps(30);
        if h.sys.mouse_locked() {
            h.key(Key::Escape);
            if h.until(10, |_| menus(&trace, "MAINMENU.TXT") > 0) {
                menu_up = true;
                break;
            }
        } else {
            h.click(320, 240);
        }
    }
    assert!(menu_up, "{build}: the in-game menu did not open");
    h.sys.push_input(Input::Quit);
    assert!(
        h.until(600, |hh| !hh.alive),
        "{build}: the game did not end"
    );
    let videos: Vec<String> = trace
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::Video { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    eprintln!("{build}: {} frames, videos {videos:?}", h.now / 16);
}

#[test]
fn traditional_chinese_cd_boots_to_the_menu() {
    boot_to_menu("ATLANTIS_ISO_ZH_TW", "atlantis-cd-zh-tw-696832");
}

#[test]
fn simplified_chinese_cd_boots_to_the_menu() {
    boot_to_menu("ATLANTIS_BIN_ZH_CN", "atlantis-cd-zh-cn-692736");
}

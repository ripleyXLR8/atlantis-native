//! A game played through the features together, as a player starts it:
//! the player menu, a new game and its opening film, the first click that
//! sends the game to chapter 75, an object taken and the bag bar opened,
//! Escape and a save through the menus over the open bar, the save loaded
//! again through the load page, then continued from the player menu of a
//! new run (the music started again from the save, at a drawn mark).
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the test is skipped. The saves go to a
//! scratch folder of the build's.

use std::path::{Path, PathBuf};

use atlantis2_data::defnames::{self, DefNames};
use atlantis2_data::save::{entry, read_save, SystemTime};
use atlantis2_data::A2Files;
use atlantis2_engine::bag::{UiBank, UiDraw};
use atlantis2_engine::menu::{state, PageKind};
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn time() -> SystemTime {
    SystemTime {
        year: 2026,
        month: 10,
        day_of_week: 4,
        day: 22,
        hour: 12,
        minute: 0,
        second: 0,
        milliseconds: 0,
    }
}

/// The engine at 60 passes a second, every pass drawn.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
}

impl Run {
    fn new(dir: &Path) -> Run {
        let mut e = Engine::boot(
            game().unwrap(),
            Variant::default(),
            Fpu::default(),
            640,
            480,
        )
        .unwrap();
        e.set_data_dir(Some(dir.to_path_buf()));
        e.wall_clock = time;
        e.trace = true;
        Run {
            e,
            pass: 0,
            screen: vec![0; 640 * 480],
        }
    }

    fn now(&self) -> f64 {
        self.pass as f64 / 60.0
    }

    fn step(&mut self) {
        self.e.set_clock(self.now());
        self.e.pass(Some(&mut self.screen));
        self.pass += 1;
        let fatal = self.e.script_state().and_then(|s| s.fatal.clone());
        assert!(fatal.is_none(), "{fatal:?}");
    }

    fn wait(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    /// A click at (x, y) of the menu, the button released after a pass.
    fn press(&mut self, x: i32, y: i32) {
        self.e.pointer(x, y);
        self.e.click(self.now());
        self.step();
        self.e.release();
        self.step();
    }

    fn press_widget(&mut self, name: &str) {
        let m = self.e.menu().unwrap();
        let w = m.page.as_ref().unwrap().widget(name).unwrap();
        let (x, y) = (
            (w.hit.left + w.hit.right) / 2,
            (w.hit.top + w.hit.bottom) / 2,
        );
        self.press(x, y);
    }

    fn page(&self) -> Option<PageKind> {
        self.e.menu().and_then(|m| m.kind())
    }

    /// Escape, the disc button: the load / save panel slid in.
    fn disc_panel(&mut self) {
        self.e.escape();
        assert_eq!(self.page(), Some(PageKind::Main));
        self.press_widget("a_disc.spr");
        self.wait(30);
        assert_eq!(self.e.menu().unwrap().state, state::LOAD_OR_SAVE);
    }

    /// The sprites the last pass drew over the panorama.
    fn ui_sprites(&self) -> Vec<(UiBank, i32, i32, i32)> {
        self.e
            .ui_draws
            .iter()
            .filter_map(|d| match d {
                UiDraw::Sprite { bank, frame, x, y } => Some((*bank, *frame, *x, *y)),
                UiDraw::Text { .. } => None,
            })
            .collect()
    }

    /// What a load gives back, as the scripts and the navigation see it
    /// (the angles apart, `AngleWarpX`.. `AngleWarpYNow`: the view entry
    /// turns the camera back to the saved ones with float residues).
    fn state(&self) -> String {
        let s = self.e.script_state().unwrap();
        let mut vars = s.vars.clone();
        vars[41..45].fill(0);
        format!(
            "chapter {} place {:?} vars {:?} bag {:?} music {:?}",
            s.chapter,
            self.e.place(),
            vars,
            s.bag,
            String::from_utf8_lossy(&s.music),
        )
    }
}

fn scratch() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("a2-flow");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn player_menu_new_game_bag_save_and_load() {
    if game().is_none() {
        return;
    }
    let dir = scratch();
    let mut r = Run::new(&dir);

    // The player menu; player 4 has no save: a new game, its opening film.
    r.e.open_player_menu();
    r.wait(5);
    assert_eq!(r.page(), Some(PageKind::Player));
    r.press_widget("b_ter.spr");
    assert_eq!((r.e.player_number, r.e.loop_type), (4, LoopType::Video));
    r.wait(120);
    assert_eq!(r.e.loop_type, LoopType::Video);
    // A click ends the film; the first view.
    r.e.click(r.now());
    for _ in 0..600 {
        r.step();
        if r.e.loop_type == LoopType::Warp {
            break;
        }
    }
    r.wait(30);
    assert_eq!(r.e.loop_type, LoopType::Warp);
    assert_eq!(r.e.place(), ("tibet.wam", 2));

    // The first click sends the game to chapter 75.
    r.e.click(r.now());
    r.wait(30);
    assert_eq!(r.e.script_state().unwrap().chapter, 75);

    // An object in the bag, the bar opened with the right button: the bar,
    // its one cell, the pointer at the screen centre.
    let names = DefNames::parse(
        &r.e.files()
            .read(atlantis2_data::Folder::Scenar, "defnames.con")
            .unwrap(),
    )
    .unwrap();
    let name = String::from_utf8_lossy(names.name(defnames::OBJETS, 61).unwrap()).into_owned();
    r.e.run_command(format!("(sacados={name})").as_bytes());
    r.e.right_click();
    assert!(r.e.bar.open);
    r.step();
    let sprites = r.ui_sprites();
    assert_eq!(sprites[0], (UiBank::Interface, 0, 320, 441));
    assert_eq!(sprites[1].0, UiBank::Objects);
    assert_eq!(*sprites.last().unwrap(), (UiBank::Interface, 40, 320, 240));
    assert_eq!(r.e.bar.shown.len(), 1);

    // Escape over the open bar: the main menu, the save.
    let before = r.state();
    let want = r.e.save_data();
    r.disc_panel();
    r.press_widget("a_save.spr");
    r.press_widget("b_oui.spr");
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert!(r.e.bar.open, "the menus leave the bar as it was");
    let file = std::fs::read(dir.join("save").join("ATL04_00.SAV")).unwrap();
    let (got, _) = read_save(&file).unwrap();
    assert_eq!(got.entry(entry::BAG_COUNT), want.entry(entry::BAG_COUNT));
    assert_eq!(got.i32(entry::CHAPTER), 75);
    let music = got.i32(entry::MUSIC);
    // Chapter 75 plays music 30.
    assert_ne!(music, 0);

    // The bar closed, the game goes on, then the load page loads the save.
    r.e.right_click();
    assert!(!r.e.bar.open);
    r.e.mouse_motion(300, 0);
    r.wait(30);
    r.disc_panel();
    r.press_widget("a_load.spr");
    assert_eq!(r.page(), Some(PageKind::Load));
    r.press(108 + 30, 129 + 30);
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert_eq!(r.state(), before);
    r.wait(60);

    // A new run: the player menu continues player 4's save, the music
    // started again at a drawn mark.
    let mut r = Run::new(&dir);
    r.e.open_player_menu();
    r.wait(5);
    r.press_widget("b_ter.spr");
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert_eq!(r.state(), before);
    r.wait(120);
    assert_eq!(r.e.music_playing(), Some(music));
    assert!(r
        .e
        .events
        .iter()
        .any(|e| matches!(e, Event::Music { music: m, .. } if *m == music)));
}

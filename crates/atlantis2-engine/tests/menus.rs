//! The menus with the game's own files: `Menu.txt`, the pages' widgets
//! (positions, hit boxes from the sprite banks, actions), the state
//! machine driven by the mouse (the slides of the main page, the options,
//! the volumes, the questions), the saves made and loaded through the
//! menus in the original's format, the player menu's choice of the newest
//! save, and the credits.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the test is skipped. The saves go to a
//! scratch folder of the build's.

use std::path::PathBuf;

use atlantis2_data::menu_txt::{MenuTxt, Section, SECTIONS};
use atlantis2_data::save::{
    entry, read_save, scan_save, SaveData, SaveFormat, SystemTime, ENTRIES, SAVE_SIZE,
};
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::anim::MsvcRand;
use atlantis2_engine::menu::{action, state, Menu, PageKind, Rect, WidgetKind};
use atlantis2_engine::{Engine, Fpu, LoopType, Variant};

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

/// `loc\Menu.txt` read on its own: the widgets' texts are checked
/// against its lines (the line each widget takes, not the words).
fn texts() -> MenuTxt {
    let d = game().unwrap().read(Folder::Loc, "Menu.txt").unwrap();
    let t = MenuTxt::parse(&d);
    assert!(t.padded.is_empty());
    t
}

/// Line `i` of section `s`, never empty.
fn line(t: &MenuTxt, s: Section, i: usize) -> &[u8] {
    let l = t.line(s, i);
    assert!(!l.is_empty(), "{s:?} {i}");
    l
}

/// An empty scratch folder for the saves.
fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("a2-menus-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn time(minute: u16) -> SystemTime {
    SystemTime {
        year: 2026,
        month: 10,
        day_of_week: 4,
        day: 22,
        hour: 9,
        minute,
        second: 0,
        milliseconds: 0,
    }
}

/// The engine at 60 passes a second.
struct Run {
    e: Engine,
    pass: u64,
}

impl Run {
    fn new(files: A2Files, data: Option<PathBuf>) -> Run {
        let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        e.anim.rand = MsvcRand(1000);
        e.set_data_dir(data);
        Run { e, pass: 0 }
    }

    fn step(&mut self) {
        self.e.set_clock(self.pass as f64 / 60.0);
        self.e.pass(None);
        self.pass += 1;
    }

    fn wait(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    /// A click at (x, y) of the menu, the button released after a pass.
    fn press(&mut self, x: i32, y: i32) {
        self.e.pointer(x, y);
        self.e.click(self.pass as f64 / 60.0);
        self.step();
        self.e.release();
        self.step();
    }

    /// A new game, its film ended.
    fn new_game(&mut self) {
        self.e.start_new_game().unwrap();
        self.e.end_video();
        self.wait(30);
        assert_eq!(self.e.loop_type, LoopType::Warp);
    }

    fn menu(&self) -> &Menu {
        self.e.menu().unwrap()
    }

    fn page(&self) -> Option<PageKind> {
        self.e.menu().and_then(|m| m.kind())
    }

    /// The centre of a widget's box.
    fn at(&self, name: &str) -> (i32, i32) {
        let w = self.menu().page.as_ref().unwrap().widget(name).unwrap();
        (
            (w.hit.left + w.hit.right) / 2,
            (w.hit.top + w.hit.bottom) / 2,
        )
    }

    fn press_widget(&mut self, name: &str) {
        let (x, y) = self.at(name);
        self.press(x, y);
    }

    /// The main menu's load / save panel slid in (state 8).
    fn disc_panel(&mut self) {
        self.e.escape();
        assert_eq!(self.page(), Some(PageKind::Main));
        self.press_widget("a_disc.spr");
        self.wait(30);
        assert_eq!(self.menu().state, state::LOAD_OR_SAVE);
    }
}

#[test]
fn the_texts_and_the_pages() {
    let Some(files) = game() else { return };
    let m = Menu::new(&files);
    let counts: Vec<usize> = m.texts.sections.iter().map(Vec::len).collect();
    assert_eq!(counts, [11, 7, 10, 6, 5, 4, 4]);
    assert!(m.texts.padded.is_empty());
    assert!(counts.iter().zip(SECTIONS).all(|(n, (_, min))| *n >= min));

    let mut r = Run::new(files, None);
    r.e.open_player_menu();
    assert_eq!(r.e.loop_type, LoopType::Menu);
    let p = r.menu().page.as_ref().unwrap();
    let names: Vec<(&str, i32, i32, i32, bool)> = p
        .widgets
        .iter()
        .map(|w| (w.name.as_str(), w.x, w.y, w.action, w.visible))
        .collect();
    assert_eq!(
        names,
        [
            ("c_player.spr", 320, 240, 0, true),
            ("a_exit.spr", 600, 440, 23, true),
            ("b_gala.spr", 277, 146, 5, true),
            ("b_lune.spr", 335, 133, 6, true),
            ("b_sol.spr", 395, 129, 7, true),
            ("b_star.spr", 453, 138, 8, true),
            ("b_ter.spr", 504, 171, 9, true),
            ("c_yn.spr", 320, 240, 0, false),
            ("b_oui.spr", 290, 230, 25, false),
            ("b_non.spr", 340, 240, 24, false),
        ]
    );
    // Every widget's bank was found: its box is frame 0's size centred on
    // its position.
    for w in &p.widgets {
        assert!(
            w.hit.right > w.hit.left && w.hit.bottom > w.hit.top,
            "{}",
            w.name
        );
        assert_eq!(w.hit.left + w.hit.right, 2 * w.x, "{}", w.name);
    }
    let tx = texts();
    assert_eq!(
        p.widget("b_gala.spr").unwrap().text,
        line(&tx, Section::Player, 2)
    );

    for (kind, want) in [
        (
            PageKind::Main,
            &[
                "a_load.spr",
                "a_save.spr",
                "c_opt.spr",
                "c_son.spr",
                "c_load.spr",
                "a_exit.spr",
                "a_toolbx.spr",
                "a_baffle.spr",
                "a_disc.spr",
                "c_yn.spr",
                "b_oui.spr",
                "b_non.spr",
            ][..],
        ),
        (
            PageKind::Options,
            &[
                "b_back.spr",
                "s_sub.spr",
                "s_cross.spr",
                "s_xyinv.spr",
                "b_rezo.spr",
            ][..],
        ),
        (
            PageKind::Sound,
            &[
                "b_back.spr",
                "MasterVolume",
                "a_baffle.spr",
                "MusicVolume",
                "a_harpe.spr",
                "DialogVolume",
                "a_hp.spr",
                "FxVolume",
                "a_cloche.spr",
            ][..],
        ),
        (
            PageKind::Load,
            &[
                "b_back.spr",
                "Listbox",
                "b_erase.spr",
                "c_yn.spr",
                "b_oui.spr",
                "b_non.spr",
            ][..],
        ),
    ] {
        r.e.open_menu_page(kind);
        let p = r.menu().page.as_ref().unwrap();
        let names: Vec<&str> = p.widgets.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, want, "{kind:?}");
    }
    r.e.open_menu_page(PageKind::Main);
    let p = r.menu().page.as_ref().unwrap();
    // The disc's box is set by hand; the panels start off the page.
    assert_eq!(
        p.widget("a_disc.spr").unwrap().hit,
        Rect {
            left: 70,
            top: 250,
            right: 135,
            bottom: 310
        }
    );
    assert!(!p.widget("a_load.spr").unwrap().visible);
    r.e.open_menu_page(PageKind::Sound);
    let p = r.menu().page.as_ref().unwrap();
    assert_eq!(
        p.widget("a_harpe.spr").unwrap().hit,
        Rect {
            left: 210,
            top: 175,
            right: 450,
            bottom: 235
        }
    );
    assert_eq!(
        p.widget("a_harpe.spr").unwrap().text,
        line(&texts(), Section::Sound, 3)
    );
}

#[test]
fn the_main_menu_its_slides_and_the_options() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, None);
    r.new_game();
    // A video goes on with Escape; the panorama opens the main menu.
    r.e.escape();
    assert_eq!(r.e.loop_type, LoopType::Menu);
    assert!(r.menu().from_game && r.menu().cursor_shown);
    // A click on nothing: 0x13, nothing happens in state 0.
    r.press(5, 470);
    assert_eq!((r.page(), r.menu().state), (Some(PageKind::Main), 0));
    // The tool box: every widget disabled, the panel slides 30 frames
    // (1 + 2 + ... + 30 = 465 pixels), then the options page.
    let x0 = r
        .menu()
        .page
        .as_ref()
        .unwrap()
        .widget("c_opt.spr")
        .unwrap()
        .x;
    r.press_widget("a_toolbx.spr");
    assert_eq!(r.menu().state, state::OPTIONS_SLIDE);
    let p = r.menu().page.as_ref().unwrap();
    assert!(p.widgets.iter().all(|w| !w.enabled));
    let c = r.menu().counter;
    let moved = p.widget("c_opt.spr").unwrap().x - x0;
    assert_eq!(moved, c * (c + 1) / 2);
    r.wait(30 - c as u64 - 1);
    assert_eq!(r.page(), Some(PageKind::Main));
    r.step();
    assert_eq!(r.page(), Some(PageKind::Options));
    // The options: the toggles flip the engine's options.
    assert!(r.e.subtitles && r.e.centre_marker && !r.e.look.invert_y);
    r.press_widget("s_sub.spr");
    r.press_widget("s_cross.spr");
    r.press_widget("s_xyinv.spr");
    assert!(!r.e.subtitles && !r.e.centre_marker && r.e.look.invert_y);
    let w = r.menu().page.as_ref().unwrap().widget("s_sub.spr").unwrap();
    assert!(matches!(w.kind, WidgetKind::Toggle { state: false, .. }));
    let tx = texts();
    assert_eq!(w.text, line(&tx, Section::Options, 2));
    // The resolution: 800 x 600 chosen, the button now offers 640 x 480.
    r.press_widget("b_rezo.spr");
    assert_eq!((r.e.cfg().width, r.e.cfg().height), (800, 600));
    let w = r
        .menu()
        .page
        .as_ref()
        .unwrap()
        .widget("b_rezo.spr")
        .unwrap();
    assert_eq!(w.text, line(&tx, Section::Options, 8));
    // Back to the main page; Escape closes the menu opened from the game.
    r.press_widget("b_back.spr");
    assert_eq!(r.page(), Some(PageKind::Main));
    r.e.escape();
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    // The options are in atlant2.cfg's form.
    let cfg = r.e.cfg();
    assert_eq!((cfg.subtitles, cfg.centre_marker, cfg.invert_y), (0, 0, 1));
}

#[test]
fn the_volumes() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, None);
    r.new_game();
    r.e.escape();
    r.press_widget("a_baffle.spr");
    r.wait(40);
    assert_eq!(r.page(), Some(PageKind::Sound));
    // The sliders start at general / 256, music / 127 / general, dialogs
    // and effects / 127.
    let v = r.menu().values;
    assert_eq!(v[0], 1.0);
    assert_eq!(v[1], (100.0f64 / 127.0) as f32);
    assert_eq!((v[2], v[3]), (1.0, 1.0));
    // The general volume grabbed at 57 pixels from the track's origin
    // (425 − 65): 57 / 115, × 256 truncated; the music then follows:
    // ftol(music × general × 127 + 0.5).
    r.e.pointer(360 + 57, 139);
    r.e.click(r.pass as f64 / 60.0);
    r.step();
    let g = (57.0f64 / 115.0) as f32;
    assert_eq!(r.menu().values[0], g);
    assert_eq!(r.e.music.master, (f64::from(g) * 256.0) as i32);
    let music = (f64::from((100.0f64 / 127.0) as f32) * f64::from(g) * 127.0 + 0.5) as i32;
    assert_eq!((r.e.music.max, r.e.music.volume), (music, music));
    // Dragged on, past the track's end: 1, every frame while held.
    r.e.pointer(600, 300);
    r.step();
    assert_eq!(r.e.music.master, 256);
    r.e.release();
    r.step();
    r.e.pointer(360, 139);
    r.step();
    assert_eq!(
        r.e.music.master, 256,
        "released: the slider no longer follows"
    );
    // The effects at 0.
    r.e.pointer(350 - 65 - 5, 364);
    r.e.click(r.pass as f64 / 60.0);
    r.step();
    r.e.release();
    r.step();
    assert_eq!(r.e.fx_volume, 0);
    assert_eq!(r.e.cfg().effects, 0);
}

/// The state a load gives back, as the scripts and the navigation see it
/// (the angles apart: the view entry turns the camera back by its current
/// angles, then to the saved ones, which leaves float residues).
fn state_of(e: &Engine) -> String {
    let s = e.script_state().unwrap();
    format!(
        "chapter {} place {:?} vue {} vars {:?} flags {:?} bag {:?} shown {:?} subjects {:?} music {:?} {}",
        s.chapter,
        e.place(),
        e.vars.vue,
        s.vars,
        s.flags.iter().enumerate().filter(|f| *f.1 != 0).collect::<Vec<_>>(),
        s.bag,
        s.shown,
        s.subjects
            .characters()
            .map(|c| (c, s.subjects.list(c)))
            .collect::<Vec<_>>(),
        String::from_utf8_lossy(&s.music),
        e.music_playing().unwrap_or(0),
    )
}

#[test]
fn saving_and_loading_through_the_menus() {
    let Some(files) = game() else { return };
    let dir = scratch("saves");
    let mut r = Run::new(files, Some(dir.clone()));
    r.e.wall_clock = || time(10);
    r.new_game();
    // Play a little: the first click sends the game to chapter 75; look
    // elsewhere.
    r.e.click(r.pass as f64 / 60.0);
    r.wait(20);
    r.e.mouse_motion(400, -100);
    r.wait(30);
    assert_eq!(r.e.script_state().unwrap().chapter, 75);
    let before = state_of(&r.e);
    let angles = r.e.angles_deg();
    let want = r.e.save_data();

    // Save: the disc, the save button, yes.
    r.disc_panel();
    r.press_widget("a_save.spr");
    assert_eq!(r.menu().state, state::ASK_SAVE);
    let p = r.menu().page.as_ref().unwrap();
    assert!(p.widget("c_yn.spr").unwrap().visible);
    let tx = texts();
    assert_eq!(
        p.widget("b_oui.spr").unwrap().text,
        line(&tx, Section::Main, 7)
    );
    assert_eq!(
        p.widget("b_non.spr").unwrap().text,
        line(&tx, Section::Main, 8)
    );
    r.press_widget("b_oui.spr");
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    let file = std::fs::read(dir.join("save").join("ATL00_00.SAV")).unwrap();
    assert_eq!(&file[..12], b"ZAPFILE 1.0\0");
    let (got, format) = read_save(&file).unwrap();
    assert_eq!(format, SaveFormat::Zap);
    assert_eq!(got, want, "the file holds the globals");
    let h = scan_save(&file).unwrap();
    assert_eq!(
        (h.wam.as_slice(), h.chapter, h.view, h.time),
        (&b"tibet.wam"[..], 75, 2, time(10))
    );
    // The bag's unused entries keep the initial pattern.
    assert_eq!(got.bag_entry(89), [0x63; 12]);

    // A second save goes to the first free slot.
    r.e.wall_clock = || time(11);
    r.disc_panel();
    r.press_widget("a_save.spr");
    r.press_widget("b_oui.spr");
    assert!(dir.join("save").join("ATL00_01.SAV").exists());

    // The game goes on elsewhere: a move to view 3.
    let (yaw, pitch) = r.e.zone_aim(2, 0).unwrap();
    r.e.camera.enter(yaw, pitch);
    r.wait(15);
    r.e.click(r.pass as f64 / 60.0);
    for _ in 0..600 {
        r.step();
        if r.e.place().1 == 3 && r.e.loop_type == LoopType::Warp {
            break;
        }
        if r.e.loop_type == LoopType::Video {
            r.e.end_video();
        }
    }
    r.wait(10);
    assert_eq!(r.e.place().1, 3);
    assert_ne!(state_of(&r.e), before);

    // Load: the load page lists the two saves, newest first.
    r.disc_panel();
    r.press_widget("a_load.spr");
    assert_eq!(r.page(), Some(PageKind::Load));
    let p = r.menu().page.as_ref().unwrap();
    let WidgetKind::List(l) = &p.widget("Listbox").unwrap().kind else {
        panic!()
    };
    let slots: Vec<u8> = l.entries.iter().map(|e| e.slot).collect();
    assert_eq!(slots, [1, 0]);
    // The second row's thumbnail: slot 0.
    r.press(108 + 30, 129 + 60 + 30);
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert_eq!(state_of(&r.e), before);
    let (yaw, pitch) = r.e.angles_deg();
    assert!((yaw - angles.0).abs() < 0.01 && (pitch - angles.1).abs() < 0.01);
    // Saving again gives the same file but for the time and the angles'
    // residues.
    let mut again = r.e.save_data();
    again.set_time(time(10));
    again.set_f32(entry::YAW, want.f32(entry::YAW));
    again.set_f32(entry::PITCH, want.f32(entry::PITCH));
    assert_eq!(again, want);

    // Erase: the toggle, the row, yes; the page is built again.
    r.disc_panel();
    r.press_widget("a_load.spr");
    r.press_widget("b_erase.spr");
    assert!(r.menu().erase_mode);
    r.press(108 + 30, 129 + 30);
    assert_eq!(r.menu().state, state::ASK_ERASE);
    r.press_widget("b_oui.spr");
    assert!(!dir.join("save").join("ATL00_01.SAV").exists());
    assert!(dir.join("save").join("ATL00_00.SAV").exists());
    assert_eq!((r.page(), r.menu().state), (Some(PageKind::Load), 0));
    assert!(!r.menu().erase_mode);
    r.e.escape();
    assert_eq!(r.e.loop_type, LoopType::Warp);

    // atlant2.cfg is written and read back.
    r.e.write_cfg();
    let cfg = std::fs::read(dir.join("atlant2.cfg")).unwrap();
    assert_eq!(cfg.len(), 35);
}

#[test]
fn the_player_menu_continues_the_newest_save() {
    if game().is_none() {
        return;
    }
    let dir = scratch("players");
    // Player 2 has two saves: slot 5 in view 2, slot 7 (newer) in view 3.
    let d: SaveData = {
        let mut r = Run::new(game().unwrap(), Some(dir.clone()));
        r.new_game();
        r.e.player_number = 2;
        r.e.wall_clock = || time(20);
        r.e.save_slot(5);
        let d = r.e.save_data();
        let mut later = d.clone();
        later.set_i32(entry::VIEW, 3);
        later.set_time(time(21));
        let file = atlantis2_data::save::write_save(&later);
        std::fs::write(dir.join("save").join("ATL02_07.SAV"), file).unwrap();
        // An invalid file is not a save.
        std::fs::write(dir.join("save").join("ATL02_08.SAV"), b"nothing").unwrap();
        d
    };
    assert_eq!(d.bytes().len(), SAVE_SIZE);
    let mut r = Run::new(game().unwrap(), Some(dir.clone()));
    r.e.open_player_menu();
    assert!(!r.menu().from_game);
    // Escape does not close the player menu.
    r.e.escape();
    assert_eq!(r.page(), Some(PageKind::Player));
    r.press_widget("b_sol.spr");
    assert_eq!(r.e.player_number, 2);
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert_eq!(r.e.place(), ("tibet.wam", 3));
    assert_eq!(r.e.script_state().unwrap().chapter, 0);

    // Player 4 has none: a new game (the opening film).
    let mut r = Run::new(game().unwrap(), Some(dir));
    r.e.open_player_menu();
    r.press_widget("b_ter.spr");
    assert_eq!((r.e.player_number, r.e.loop_type), (4, LoopType::Video));
}

#[test]
fn quitting_shows_the_credits() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, None);
    r.new_game();
    r.e.escape();
    r.press_widget("a_exit.spr");
    assert_eq!(r.menu().state, state::ASK_QUIT);
    let p = r.menu().page.as_ref().unwrap();
    assert_eq!(
        p.widget("b_oui.spr").unwrap().text,
        line(&texts(), Section::Main, 9)
    );
    // No: back to the page, every widget enabled.
    r.press_widget("b_non.spr");
    assert_eq!(r.menu().state, state::IDLE);
    assert!(
        r.menu()
            .page
            .as_ref()
            .unwrap()
            .widget("a_exit.spr")
            .unwrap()
            .enabled
    );
    r.press_widget("a_exit.spr");
    r.press_widget("b_oui.spr");
    assert_eq!(
        (r.page(), r.menu().state),
        (Some(PageKind::Credits), state::CREDITS)
    );
    assert!(!r.menu().cursor_shown);
    assert!(!r.e.quit());
    // The pages show 5 s each and fade 2 s into the next.
    r.wait(60 * 8);
    assert!(!r.e.quit());
    // Escape held ends the program (a key state, not an event: a key
    // pressed and released before has no effect, nor another key).
    r.e.escape_key(true);
    r.step();
    assert!(r.e.quit());
}

#[test]
fn the_animated_buttons() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, None);
    r.new_game();
    r.e.escape();
    fn frame(r: &Run) -> i32 {
        let p = r.menu().page.as_ref().unwrap();
        match p.widget("a_toolbx.spr").unwrap().kind {
            WidgetKind::Animated { frame, .. } => frame,
            _ => panic!("not an animated button"),
        }
    }
    // The mouse in the box, the button up: a frame per tick flag.
    let (x, y) = r.at("a_toolbx.spr");
    r.e.pointer(x, y);
    r.wait(30);
    assert!(frame(&r) > 0);
    // The button pressed outside the box and held, then the mouse in the
    // box: the button is not pressed, frame 0 (FUN_0046db00).
    r.e.pointer(5, 470);
    r.e.click(r.pass as f64 / 60.0);
    r.step();
    assert_eq!(frame(&r), 0);
    r.e.pointer(x, y);
    r.wait(30);
    assert_eq!(frame(&r), 0);
    // Released: it animates again.
    r.e.release();
    r.wait(30);
    assert!(frame(&r) > 0);
    assert_eq!((r.page(), r.menu().state), (Some(PageKind::Main), 0));
}

#[test]
fn script_credits_wait_for_escape_held() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, None);
    r.new_game();
    // Escape pressed and released in the game (the main menu opened and
    // closed), then the credits started as by the script key `credits`.
    for _ in 0..2 {
        r.e.escape_key(true);
        r.e.escape();
        r.step();
        r.e.escape_key(false);
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);
    r.e.open_credits();
    r.wait(60);
    assert_eq!(r.page(), Some(PageKind::Credits));
    assert!(!r.e.quit());
    r.e.escape_key(true);
    r.step();
    assert!(r.e.quit());
}

#[test]
fn the_table_is_the_originals() {
    // The save is the 35 globals of the original's table, back to back.
    assert_eq!(ENTRIES.len(), 35);
    assert_eq!(ENTRIES.iter().map(|e| e.1).sum::<usize>(), SAVE_SIZE);
    assert_eq!(action::ELSEWHERE, 0x13);
}

#[test]
fn the_credits_end_after_the_last_page() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, None);
    r.e.open_player_menu();
    r.e.open_credits();
    let start = r.pass;
    // 21 pages: 21 cycles of 7 s and a frame (FUN_00470d00), the last
    // one a fade to black; the frame after it the action 0x1a, then the
    // program ends (no section waits).
    while !r.e.quit() {
        assert!(r.pass - start < 60 * 200, "the credits do not end");
        r.step();
    }
    // (A float clock may land a page change a frame later.)
    let n = r.pass - start;
    assert!((21 * 421 + 1..=21 * 422 + 1).contains(&n), "{n} frames");
}

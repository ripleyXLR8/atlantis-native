//! The Chinese CDs' interface (atlantis-cd-zh-tw-696832,
//! atlantis-cd-zh-cn-692736): the menus drawn from the SPRCHI1 / SPRCHI2
//! sprites, the options toggles as sprites, the LOAD list from the EPICHI
//! sprites by row, the CINEMA 20 captions, SUBTITLES on at start-up and the
//! 114-byte players.atl.
//!
//! Needs the folders of the discs' images: set `ATLANTIS_ISO_ZH_TW` (the
//! Traditional CD's ISO images) and `ATLANTIS_BIN_ZH_CN` (the Simplified
//! CD's raw images); without them the tests are skipped. Nothing of the
//! discs is kept here: the expected blits come from the discs' own menu
//! files and sprites.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::saves::PLAYERS_SIZE_ZH;
use atlantis_data::sprite::{limits, read_spr_file, read_tga15, SprSprite};
use atlantis_data::sprite_blend::SPR_NO_BLEND;
use atlantis_data::{Game, LoadList};
use atlantis_engine::audio::Mixer;
use atlantis_engine::checkpoint::PLAYERS_FILE;
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::ui::layout::{menu_number, walk_menu_text, MenuRules, MenuWalk};
use atlantis_engine::{BugFixes, Engine, Enhancements, Event, MemStore, Options, SaveStore, Start};

const W: usize = 640;
const H: usize = 480;

fn game(var: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(var) else {
        eprintln!("{var} not set: skipped");
        return None;
    };
    let set = DiscSet::open_folder(dir.as_ref()).expect("CD images");
    Some(Game::open(Arc::new(set)).expect("the game from its images"))
}

fn bank(g: &Game, name: &str) -> Vec<SprSprite> {
    let d = g
        .big
        .read(&format!("SPRITE\\2D\\{name}"))
        .unwrap_or_else(|| panic!("{name}"));
    read_spr_file(&d, limits::DEFAULT.0, limits::DEFAULT.1).unwrap()
}

/// The share of a sprite's opaque pixels on screen that show it, drawn
/// with its hotspot at (x, y) (as `SprSprite::draw` places it).
fn shown(screen: &[u16], spr: &SprSprite, x: i32, y: i32) -> f64 {
    let (bx, by) = (x - i32::from(spr.xoff), y - i32::from(spr.yoff));
    let (mut opaque, mut same) = (0usize, 0usize);
    for j in 0..i32::from(spr.h) {
        for i in 0..i32::from(spr.w) {
            let k = (j * i32::from(spr.w) + i) as usize;
            let (sx, sy) = (bx + i, by + j);
            if spr.blend[k] != SPR_NO_BLEND
                || !(0..W as i32).contains(&sx)
                || !(0..H as i32).contains(&sy)
            {
                continue;
            }
            opaque += 1;
            if screen[sy as usize * W + sx as usize] == spr.pixels[k] {
                same += 1;
            }
        }
    }
    assert!(opaque > 0, "a sprite with no opaque pixel on screen");
    same as f64 / opaque as f64
}

/// The interface's banks: SPRCHI1 / SPRCHI2 instead of FONTMAX, EPICHI1 /
/// EPICHI2 of 74 frames whose widths are the executable's width table,
/// SUBCHINE of 11 captions; every menu file walks as sprite records of
/// the banks.
fn banks_and_menus(var: &str) {
    let Some(g) = game(var) else { return };
    let build = g.exe.build;
    assert!(build.menu_sprite_banks.is_some());
    let subchine = bank(&g, build.cinema_captions.unwrap().bank);
    assert_eq!(subchine.len(), 11);
    let LoadList::Sprites { banks, rows, .. } = build.load_list else {
        panic!("{}: not a sprite LOAD list", build.name);
    };
    for b in banks {
        let frames = bank(&g, b);
        assert_eq!(frames.len(), usize::from(rows), "{b}");
    }
    // The title is centred by the width table: the widths of the EPICHI1
    // frames, but for the one frame too wide for a byte (280 px), which
    // the table gives less (the title is then drawn 30 px right of
    // centre).
    let frames = bank(&g, banks[0]);
    for (i, (f, &w)) in frames.iter().zip(&g.exe.load_widths).enumerate() {
        if f.w < 0x100 {
            assert_eq!(f.w, u16::from(w), "{}: frame {i}", build.name);
        } else {
            assert!(u16::from(w) < f.w, "{}: frame {i}", build.name);
        }
    }
    assert_eq!(g.exe.load_chapters.len(), usize::from(rows));
    // END.TGA, the end picture, is the disc's own.
    assert!(g
        .big
        .read("IMAGES\\END.TGA")
        .and_then(|d| read_tga15(&d))
        .is_some());

    let sys = Sys::new(Mixer::new());
    let s2 = sys.clone();
    let done = Rc::new(RefCell::new(false));
    let done2 = done.clone();
    let mut r = Runner::new(sys.clone(), async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.load_interface().await;
        let ui = e.ui.clone();
        assert!(ui.fontmax.is_empty() && ui.fontmax2.is_empty());
        let [normal, hovered] = &ui.menu_banks;
        assert!(normal.len() >= 31 && normal.len() == hovered.len());
        assert_eq!(ui.load_banks[0].len(), 74);
        let rules = MenuRules::of_build(e.game.exe.build, &BugFixes::NONE);
        let mut walked = 0;
        for name in [
            "MAINMENU", "LOADMENU", "OPTMENU", "SELEMENU", "SONMENU", "SUREMENU", "CD1MENU",
            "CD2MENU", "CD3MENU", "CD4MENU", "WAIT", "ERROR1", "ERROR2", "ERROR3",
        ] {
            let file = format!("{name}.TXT");
            let text = e.game.big.read(&format!("SPRLIST\\{file}")).unwrap();
            let number = menu_number(&file).unwrap() as i32;
            let mut walk = MenuWalk::default();
            walk_menu_text(&text, number, -1, 1, 0, rules, &mut walk);
            assert!(!walk.ops.is_empty(), "{name}");
            assert!(walk.ops.iter().all(|o| o.sprite), "{name}: a text line");
            let words: Vec<_> = walk.ops.iter().filter(|o| o.alt_bank).collect();
            assert!(!words.is_empty(), "{name}");
            let mut s = vec![0u16; W * H];
            ui.draw_menu(&mut s, &walk);
            for o in words {
                let spr = &normal[o.index as usize];
                let at = shown(&s, spr, i32::from(o.x), i32::from(o.y));
                assert!(at > 0.5, "{name}: word {} at {at}", o.index);
            }
            walked += 1;
        }
        assert_eq!(walked, 14);
        *done2.borrow_mut() = true;
    });
    let mut t = 0;
    while r.step(t) {
        t += 16;
    }
    assert!(*done.borrow());
}

#[test]
fn traditional_chinese_banks_and_menus() {
    banks_and_menus("ATLANTIS_ISO_ZH_TW");
}

#[test]
fn simplified_chinese_banks_and_menus() {
    banks_and_menus("ATLANTIS_BIN_ZH_CN");
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

    fn move_to(&mut self, x: i32, y: i32) {
        self.sys.push_input(Input::MouseMove { x, y });
        self.steps(3);
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

    fn shown(&self, spr: &SprSprite, x: i32, y: i32) -> f64 {
        shown(&self.sys.screen.borrow(), spr, x, y)
    }
}

/// The record `n` of a menu as the build walks it: its centre.
fn record_centre(g: &Game, file: &str, n: usize, toggles: (i32, i32)) -> (i32, i32) {
    let text = g.big.read(&format!("SPRLIST\\{file}")).unwrap();
    let mut walk = MenuWalk::default();
    let rules = MenuRules::of_build(g.exe.build, &BugFixes::NONE);
    let number = menu_number(file).unwrap() as i32;
    walk_menu_text(&text, number, -1, toggles.0, toggles.1, rules, &mut walk);
    let r = walk.records[n];
    (
        i32::from(r.x) + i32::from(r.w) / 2,
        i32::from(r.y) + i32::from(r.h) / 2,
    )
}

/// The word record `n` of a menu (index, x, y) as walked with `toggles`.
fn word(g: &Game, file: &str, n: usize, toggles: (i32, i32)) -> (usize, i32, i32) {
    let text = g.big.read(&format!("SPRLIST\\{file}")).unwrap();
    let mut walk = MenuWalk::default();
    let rules = MenuRules::of_build(g.exe.build, &BugFixes::NONE);
    let number = menu_number(file).unwrap() as i32;
    walk_menu_text(&text, number, -1, toggles.0, toggles.1, rules, &mut walk);
    let o = walk.ops.iter().find(|o| o.record == n as i32).unwrap();
    (o.index as usize, i32::from(o.x), i32::from(o.y))
}

/// A run from the start-up with SUBTITLES off in the settings, without the
/// port's enhancements: the player
/// screen's words, a 114-byte players.atl, the CINEMA 20 captions,
/// subtitles drawn, the options toggles, the LOAD rows and title.
fn play_the_interface(var: &str) {
    let Some(g) = game(var) else { return };
    let Some(probe) = game(var) else { return };
    let words = bank(&probe, "SPRCHI1.SPR");
    let words_hovered = bank(&probe, "SPRCHI2.SPR");
    let rows = bank(&probe, "EPICHI1.SPR");
    let rows_hovered = bank(&probe, "EPICHI2.SPR");
    let captions = bank(&probe, "SUBCHINE.SPR");
    let build = g.exe.build.name;
    let sys = Sys::new(Mixer::new());
    let trace = Rc::new(RefCell::new(Vec::new()));
    let store = Rc::new(MemStore::new());
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        subtitles: false,
        // The original's rules (the SUBTITLES record's fix reads the options'
        // id 9 record in step).
        enhancements: Enhancements::original(),
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
    // The start-up videos cut short, up to the player screen.
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
    h.move_to(639, 0);
    h.steps(10);
    // Its title (record 0) and OK (record 5) as SPRCHI1 words.
    for n in [0, 5] {
        let (i, x, y) = word(&probe, "SELEMENU.TXT", n, (0, 0));
        let at = h.shown(&words[i], x, y);
        assert!(at > 0.9, "{build}: SELEMENU record {n}: {at}");
    }
    // Hovered: OK from SPRCHI2.
    let ok = record_centre(&probe, "SELEMENU.TXT", 5, (0, 0));
    h.move_to(ok.0, ok.1);
    let (i, x, y) = word(&probe, "SELEMENU.TXT", 5, (0, 0));
    assert!(
        h.shown(&words_hovered[i], x, y) > 0.8,
        "{build}: OK not hovered"
    );
    // A new player: players.atl of 114 bytes.
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
    assert_eq!(
        store.read(PLAYERS_FILE).map(|d| d.len()),
        Some(PLAYERS_SIZE_ZH),
        "{build}"
    );
    // CINEMA 20 (CHAPI001's first section); no click until the captions
    // are checked.
    let cinema = |t: &Rc<RefCell<Vec<Event>>>| {
        t.borrow()
            .iter()
            .any(|e| matches!(e, Event::Set(c) if c.eq_ignore_ascii_case("cinema=20")))
    };
    assert!(h.until(3000, |_| cinema(&trace)), "{build}: no CINEMA 20");
    let start = h.now;
    h.move_to(639, 0);
    // Ticks 1200..2200: caption 0 at (10, 430); 2200..4400: caption 1.
    let at_tick = |h: &mut Headless, tick: u64| {
        while h.now < start + tick * 5 {
            h.step();
        }
    };
    at_tick(&mut h, 1600);
    assert!(h.shown(&captions[0], 10, 430) > 0.9, "{build}: caption 0");
    at_tick(&mut h, 3000);
    assert!(h.shown(&captions[1], 10, 430) > 0.9, "{build}: caption 1");
    assert!(
        h.shown(&captions[0], 10, 430) < 0.9,
        "{build}: caption 0 left"
    );
    // Past the opening (a click ends each line), Escape: the in-game menu.
    let open_menu = |h: &mut Headless| {
        let before = menus(&trace, "MAINMENU.TXT");
        for _ in 0..300 {
            h.steps(30);
            if h.sys.mouse_locked() {
                h.key(Key::Escape);
                if h.until(10, |_| menus(&trace, "MAINMENU.TXT") > before) {
                    return true;
                }
            } else {
                h.click(320, 240);
            }
        }
        false
    };
    assert!(open_menu(&mut h), "{build}: the in-game menu did not open");
    // SUBTITLES were on whatever the settings said.
    let subtitled = trace
        .borrow()
        .iter()
        .any(|e| matches!(e, Event::LinePlayed { subtitles, .. } if *subtitles > 0));
    assert!(subtitled, "{build}: no subtitles drawn");

    // The options: the SUBTITLES toggle record 3 shows word 10 (on); a
    // click turns it off: word 9 drawn where word 10 was.
    let options = record_centre(&probe, "MAINMENU.TXT", 1, (0, 0));
    h.click(options.0, options.1);
    assert!(h.until(10, |_| menus(&trace, "OPTMENU.TXT") > 0));
    h.move_to(639, 0);
    let (on_word, x, y) = word(&probe, "OPTMENU.TXT", 3, (1, 0));
    assert_eq!(on_word, 10);
    assert!(h.shown(&words[10], x, y) > 0.9, "{build}: toggle on");
    let toggle = record_centre(&probe, "OPTMENU.TXT", 3, (1, 0));
    h.click(toggle.0, toggle.1);
    h.move_to(639, 0);
    assert!(h.shown(&words[9], x, y) > 0.9, "{build}: toggle off");
    // The label of record 2 is hovered under the pointer (SPRCHI2).
    let (label, lx, ly) = word(&probe, "OPTMENU.TXT", 2, (1, 0));
    let c = record_centre(&probe, "OPTMENU.TXT", 2, (1, 0));
    h.move_to(c.0, c.1);
    assert!(
        h.shown(&words_hovered[label], lx, ly) > 0.8,
        "{build}: label"
    );
    // OK, then the options again: the word still where it was.
    let ok = record_centre(&probe, "OPTMENU.TXT", 6, (1, 0));
    h.click(ok.0, ok.1);
    h.steps(10);
    assert!(open_menu(&mut h), "{build}: the in-game menu again");
    h.click(options.0, options.1);
    assert!(h.until(10, |_| menus(&trace, "OPTMENU.TXT") > 1));
    h.move_to(639, 0);
    assert!(h.shown(&words[9], x, y) > 0.9, "{build}: toggle kept");
    h.click(ok.0, ok.1);
    h.steps(10);

    // The LOAD list: three checkpoints of the player (the list reads
    // their names only); rows 0..2 drawn as EPICHI frames 0..2 whatever
    // their chapters, the hovered row from EPICHI2; a click shows the
    // row's title at y 60.
    for n in ["pa2.gam", "pa5.gam", "pa9.gam"] {
        store.write(n, b"");
    }
    assert!(open_menu(&mut h), "{build}: the in-game menu for LOAD");
    let load = record_centre(&probe, "MAINMENU.TXT", 0, (0, 0));
    h.click(load.0, load.1);
    assert!(h.until(10, |_| menus(&trace, "LOADMENU.TXT") > 0));
    h.move_to(639, 0);
    for (k, row) in rows.iter().enumerate().take(3) {
        let at = h.shown(row, 0x37, 0x6e + 22 * k as i32);
        assert!(at > 0.9, "{build}: row {k}: {at}");
    }
    assert!(h.shown(&rows[3], 0x37, 0x6e + 66) < 0.5, "{build}: row 3");
    let row2 = (200, 108 + 22 * 2 + 11);
    h.move_to(row2.0, row2.1);
    assert!(h.shown(&rows_hovered[2], 0x37, 0x6e + 44) > 0.8, "{build}");
    h.click(row2.0, row2.1);
    h.move_to(639, 0);
    let w = i32::from(rows[2].w);
    let at = h.shown(&rows_hovered[2], (640 - w) / 2, 0x3c);
    assert!(at > 0.9, "{build}: title {at}");
    h.key(Key::Escape);
    h.sys.push_input(Input::Quit);
    assert!(
        h.until(600, |hh| !hh.alive),
        "{build}: the game did not end"
    );
}

#[test]
fn traditional_chinese_interface() {
    play_the_interface("ATLANTIS_ISO_ZH_TW");
}

#[test]
fn simplified_chinese_interface() {
    play_the_interface("ATLANTIS_BIN_ZH_CN");
}

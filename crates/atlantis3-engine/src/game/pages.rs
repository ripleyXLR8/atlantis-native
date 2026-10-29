//! The menus' game side: the Escape handler (@ 0x0042ac50) that captures
//! the screen for the thumbnails and opens the in-game menu, the menu frame
//! (loop mode 6, [`crate::menu::Menus`]) and what its events do to the
//! game: loads, saves with their thumbnails, back to the game, the start
//! page, the options and volumes, `Atlantis3.cfg`.
//!
//! Comments cite atlantis3-gog-fr-3473408 (`Game.exe`).

use std::collections::HashMap;
use std::path::Path;

use atlantis2_engine::warp::A3Raster;

use super::{Game, Mode};
use crate::audio::{csm_gain, CHANNEL_MUSIC, CHANNEL_PLACE};
use crate::menu::{Env, Event, Host, Input, Menus, Page, PAGE_MAIN, PAGE_START};

/// Where a thumbnail is kept among the saves: `spr_<W>/<name>.spr` (the
/// original's `menu\spr_<W>\32bits\save\<name>.spr`).
pub fn thumbnail_key(menu_width: usize, name: &str) -> String {
    format!("spr_{menu_width}/{name}.spr")
}

/// The saves as the menus see them: the ones written this session, then the
/// save folder.
struct Saves<'a> {
    saves: &'a HashMap<String, Vec<u8>>,
    dir: Option<&'a Path>,
}

impl Saves<'_> {
    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.saves
            .get(name)
            .cloned()
            .or_else(|| self.dir.and_then(|d| std::fs::read(d.join(name)).ok()))
    }
}

impl Host for Saves<'_> {
    fn exists(&mut self, name: &str) -> bool {
        self.saves.contains_key(name) || self.dir.is_some_and(|d| d.join(name).is_file())
    }

    fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        self.get(name)
    }

    fn thumbnail(&mut self, width: usize, name: &str) -> Option<Vec<u8>> {
        self.get(&thumbnail_key(width, name))
    }
}

/// `_strdate` + `_strtime` (`MM/DD/YY HH:MM:SS`) of the system clock in
/// UTC (the original's is the local time; the port has no time zone).
fn date_and_time_now() -> Vec<u8> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days to the civil date (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    atlantis3_data::save::date_and_time(
        year as u32,
        month as u32,
        day as u32,
        (rem / 3600) as u32,
        (rem / 60 % 60) as u32,
        (rem % 60) as u32,
    )
}

impl Game {
    /// The menus loaded for the screen (once: the original loads the nine
    /// interfaces at start-up, @ 0x00451570, and keeps them).
    fn menus_ready(&mut self) -> bool {
        let (w, h) = (self.width, self.height);
        if !self.menus.loaded(w, h) {
            self.menus.load(&self.files, w, h, &self.startup.config);
        }
        self.menus.get(PAGE_MAIN).is_some()
    }

    /// The menus (tests: pages, state, the last frame's calls).
    pub fn menus(&self) -> &Menus {
        &self.menus
    }

    /// The menus, mutable (tests; the app sets `cfg_path`).
    pub fn menus_mut(&mut self) -> &mut Menus {
        &mut self.menus
    }

    /// The menu page shown and its sub-state, in mode 6.
    pub fn menu_page(&self) -> Option<(u8, u8)> {
        (self.mode == Mode::Menu).then_some((self.menus.page, self.menus.sub))
    }

    /// The menu page shown, if any.
    pub fn page(&self) -> Option<&Page> {
        (self.mode == Mode::Menu)
            .then(|| self.menus.get(self.menus.page))
            .flatten()
    }

    fn enter_menu(&mut self, page: u8) -> bool {
        if !self.menus_ready() {
            return false;
        }
        if self.mode != Mode::Menu {
            self.prev_mode = self.mode;
        }
        self.mode = Mode::Menu;
        self.menus.page = page;
        self.click = false;
        true
    }

    /// Shows the start page (`i_debut`, after the intro: loop mode 6, page
    /// 0); false when it cannot.
    pub fn show_start_page(&mut self) -> bool {
        let ok = self.enter_menu(PAGE_START);
        self.menus.sub = 0;
        ok
    }

    /// Opens the in-game menu as Escape does ([`Game::open_menu`]).
    pub fn show_save_page(&mut self) -> bool {
        self.open_menu()
    }

    /// Opens the load page as the in-game menu's `BT_LOAD` does (the save
    /// list built, page 2 in sub-state 0).
    pub fn show_load_page(&mut self) -> bool {
        if !self.open_menu() {
            return false;
        }
        let mut host = Saves {
            saves: &self.saves,
            dir: self.save_dir.as_deref(),
        };
        let mut env = Env {
            files: &self.files,
            config: &mut self.startup.config,
            texts: self.texts.as_ref(),
            host: &mut host,
            player: self.player,
            now: self.now,
            pointer: self.pointer,
        };
        self.menus.open_load(&mut env);
        true
    }

    /// The in-game menu opened from the game (@ 0x0042ac50, loop mode 3):
    /// the place sounds stopped (@ 0x00433580), the subtitle off
    /// (@ 0x0040e330), loop mode 6, the screen captured for the save
    /// thumbnails (@ 0x00410400: a same-size copy of the last frame),
    /// page 1. The sub-state is left as it is.
    pub fn open_menu(&mut self) -> bool {
        let capture = (self.screen.clone(), self.width, self.height);
        if !self.enter_menu(PAGE_MAIN) {
            return false;
        }
        self.audio.stop(CHANNEL_PLACE);
        self.subtitle = None;
        self.menus.capture = Some(capture);
        true
    }

    /// Escape (@ 0x0042ac50 by loop mode): the panorama opens the in-game
    /// menu (or, during the start page's demo, the start page); the
    /// inventory captures the screen, closes (@ 0x00427b20) and opens the
    /// menu; a dialog takes it as a click (the line ends, @ 0x0042ae60);
    /// a cinematic stops (its own wait loop's key, [`Game::skip`]); on the
    /// start page with no player the in-game menu opens with `BT_LOAD`,
    /// `BT_SAVE` and `BT_BACK` off. Mode 4's own case (a travel video
    /// stopped, @ 0x00422090, then the menu) is not ported: travel videos
    /// ignore Escape.
    pub fn escape(&mut self) {
        match self.mode {
            Mode::Warp if self.menus.demo => {
                self.menus.demo = false;
                self.audio.stop(CHANNEL_PLACE);
                self.subtitle = None;
                if self.enter_menu(PAGE_START) {
                    self.menus.sub = 0;
                }
            }
            Mode::Warp => {
                self.open_menu();
            }
            Mode::Inventory => {
                let capture = (self.screen.clone(), self.width, self.height);
                self.close_inventory();
                if self.open_menu() {
                    self.menus.capture = Some(capture);
                }
            }
            Mode::Dialog => self.click = true,
            Mode::Intro | Mode::Video => self.skip(),
            Mode::Menu if self.menus.page == PAGE_START && self.player == 0 => {
                // @ 0x0042af4a..0x0042b1b0
                self.menus.page = PAGE_MAIN;
                if let Some(p) = self.menus.get_mut(PAGE_MAIN) {
                    p.clear_pressed();
                    for name in [&b"BT_LOAD"[..], b"BT_SAVE", b"BT_BACK"] {
                        if let Some(k) = p.find(name) {
                            p.items[k].el.selectable = false;
                            p.items[k].el.clickable = false;
                        }
                    }
                }
                self.menus.input = Input::default();
            }
            _ => {}
        }
    }

    /// The left button released (@ 0x004316e9): in the menus, the click
    /// flag (the click tests act on the release).
    pub fn release(&mut self) {
        if self.mode == Mode::Menu {
            self.menus.release();
        }
    }

    /// A menu frame (loop mode 6, @ 0x004591a0): the menus' frame on the
    /// screen, the pointer (@ 0x004281e0), then the events.
    pub(super) fn menu_frame(&mut self) {
        if self.credits.is_some() {
            self.credits_frame();
            return;
        }
        if !self.menus_ready() {
            self.mode = Mode::Idle;
            return;
        }
        if self.texts.is_none() {
            self.texts();
        }
        let mut host = Saves {
            saves: &self.saves,
            dir: self.save_dir.as_deref(),
        };
        let mut env = Env {
            files: &self.files,
            config: &mut self.startup.config,
            texts: self.texts.as_ref(),
            host: &mut host,
            player: self.player,
            now: self.now,
            pointer: self.pointer,
        };
        let events = self.menus.frame(&mut env, &mut self.screen);
        self.draw_pointer();
        self.menu_events(events);
    }

    /// The menus' events, in order.
    fn menu_events(&mut self, events: Vec<Event>) {
        for e in events {
            match e {
                Event::Command(c) => self.run_command(&String::from_utf8_lossy(&c)),
                Event::WriteCfg => self.write_cfg(),
                Event::Exists(..) => {}
                Event::Player(n) => self.player = n,
                Event::NewGame => {
                    self.page_left();
                    self.new_game();
                }
                Event::Load(name) => {
                    self.page_left();
                    self.video = None;
                    self.mode = Mode::Warp;
                    self.load_named(&name);
                }
                Event::Save(name) => self.write_menu_save(&name),
                Event::Resume => self.page_left(),
                Event::StartPage => {}
                // `BT_QUIT` of the start page: the credits (@ 0x00457690),
                // whose end quits.
                Event::Quit => self.start_credits(),
                Event::Volumes => {
                    // The music setter applies master × music at once
                    // (@ 0x00434100); the other levels are read when a
                    // sound starts.
                    let g = csm_gain(self.level(self.startup.config.music_volume));
                    self.audio.set_gain(CHANNEL_MUSIC, [g, g]);
                }
                Event::Options => {
                    let c = &self.startup.config;
                    self.renderer.a3 = Some(A3Raster { smooth: c.smooth });
                    self.dialog_renderer.a3 = Some(A3Raster { smooth: c.smooth });
                    self.look.invert_y = c.invert_mouse_y;
                }
                // The page surface and the banks follow the chosen depth
                // (the menus); the screen stays RGB565 and its size.
                Event::DisplayChange(w, h)
                    if (w as usize, h as usize) != (self.width, self.height) =>
                {
                    self.not_ported(&format!("the options' screen size switch to {w} x {h}"));
                }
                Event::DisplayChange(..) => {}
                Event::ReloadTexts => self.texts = None,
            }
        }
    }

    /// Back to the game (@ 0x00451280): loop mode 3.
    fn page_left(&mut self) {
        if self.mode == Mode::Menu {
            self.mode = Mode::Warp;
        }
        self.look_last = None;
    }

    /// `Atlantis3.cfg` written (@ 0x00408f80) where the app asked.
    fn write_cfg(&mut self) {
        let data = self.startup.config.write(&self.startup.install_language);
        if let Some(p) = &self.menus.cfg_path {
            if let Err(e) = std::fs::write(p, &data) {
                log::warn!("{}: {e}", p.display());
            }
        }
        self.menus.cfg_written = Some(data);
    }

    /// The save page's `BT_YES` (@ 0x00459ee0): the save `name` (its
    /// `DateAndTime` the clock's) and, for a `game*` save, its three
    /// thumbnails made from the screen captured at Escape
    /// ([`crate::menu::thumbnails`]).
    fn write_menu_save(&mut self, name: &str) {
        let Some(s) = self.script.take() else {
            return;
        };
        let text = self.save_text(&s);
        self.script = Some(s);
        // The writer's second line (`DateAndTime = ...`).
        let mut data = Vec::new();
        let mut lines = text.splitn(3, |&b| b == b'\n');
        if let (Some(a), Some(_), Some(rest)) = (lines.next(), lines.next(), lines.next()) {
            data.extend_from_slice(a);
            data.extend_from_slice(b"\nDateAndTime = ");
            data.extend(date_and_time_now());
            data.push(b'\n');
            data.extend_from_slice(rest);
        } else {
            data = text;
        }
        self.store_save(name, data);
        if !atlantis3_data::save::has_thumbnail(name) {
            return;
        }
        let Menus {
            capture, encoder, ..
        } = &mut self.menus;
        let Some((pixels, w, h)) = capture.as_ref() else {
            return;
        };
        let banks = crate::menu::thumbnails(pixels, *w, *h, encoder);
        for (menu_width, bank) in banks {
            let key = thumbnail_key(menu_width, name);
            if let Some(dir) = &self.save_dir {
                let path = dir.join(&key);
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::write(&path, &bank) {
                    log::warn!("{key}: {e}");
                }
            }
            self.saves.insert(key, bank);
        }
    }
}

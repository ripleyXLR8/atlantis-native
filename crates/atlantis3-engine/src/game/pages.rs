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

/// The system clock in seconds since 1970 (the default of
/// [`Game::unix_time`]; the browser's build gives its own, the standard
/// library has no clock there).
pub fn system_unix_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64
}

/// `_strdate` + `_strtime` (`MM/DD/YY HH:MM:SS`): the local date and
/// time at `secs`, seconds since 1970 already shifted to the local time
/// zone ([`Game::local_time`]).
pub(super) fn date_and_time_now(secs: i64) -> Vec<u8> {
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

    pub(super) fn enter_menu(&mut self, page: u8) -> bool {
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
    /// inventory clears the subtitle line and closes (@ 0x00427b20), back
    /// to the panorama with no menu; a dialog takes it as a click (the line ends, @ 0x0042ae60);
    /// a cinematic stops (its own wait loop's key, [`Game::skip`]); on the
    /// start page with no player the in-game menu opens with `BT_LOAD`,
    /// `BT_SAVE` and `BT_BACK` off; a travel video ends and the in-game
    /// menu opens ([`Game::escape_travel`]); the tower map, the unicorn,
    /// the game boy, the jewel case and the skull stop
    /// ([`Game::escape_screen`]).
    pub fn escape(&mut self) {
        // The keyboard callback handles no key while a puzzle freezes the
        // input (@ 0x0042ab20: the byte 0x00789d56 set → `ret`): no aim
        // stored, no Escape handler, so no menu opens during the spinning
        // top, a corridor door, the merchant's stone or capture, the
        // tiger's stone, the fisherman's scale or the tower's demon turn.
        if self.keys_frozen() {
            return;
        }
        // The keyboard callback stores the camera's angles at every Escape
        // press, whatever the mode (@ 0x0042ac35: 0x004408e0 into
        // 0x0077e648 / 0x0078111c), then runs the Escape handler.
        self.escape_angles = self.camera.angles();
        match self.mode {
            Mode::Video if self.video.as_ref().is_some_and(|v| v.travel) => self.escape_travel(),
            Mode::Warp if self.menus.demo => {
                self.menus.demo = false;
                self.audio.stop(CHANNEL_PLACE);
                self.subtitle = None;
                self.voices_clear();
                if self.enter_menu(PAGE_START) {
                    self.menus.sub = 0;
                }
            }
            Mode::Warp => {
                // The voice slots stopped and freed (@ 0x0042ad09), not
                // from the inventory (@ 0x0042af2b).
                self.voices_clear();
                self.open_menu();
            }
            Mode::Inventory => {
                // @ 0x0042af2b: the subtitle line cleared (the voices and
                // the place sounds play on), the menu's loop mode 6 and
                // page 1 set and the screen captured, then the inventory's
                // close (@ 0x00427b20) puts back the mode it saved at its
                // opening: the panorama. No menu shows; the capture is
                // taken again by the next Escape that opens it.
                self.subtitle = None;
                self.close_inventory();
            }
            Mode::Dialog => self.click = true,
            Mode::Screen => self.escape_screen(),
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

    /// Whether the keyboard callback ignores every key (@ 0x0042ab20:
    /// the input freeze byte 0x00789d56, set by a puzzle's animation):
    /// Escape ([`Game::escape`]) and Alt+Enter do nothing then.
    pub fn keys_frozen(&self) -> bool {
        self.puzzles.freeze
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
        if let Some(&Event::DisplayChange(w, h)) = events
            .iter()
            .find(|e| matches!(e, Event::DisplayChange(..)))
        {
            self.display_change(w as usize, h as usize);
        }
        self.draw_pointer();
        // The flip (@ 0x00459268): the subtitle line or the voice slots
        // (an Escape from the warp cleared both; from the inventory the
        // slots go on, @ 0x0042af2b).
        self.flip_texts();
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
                    // Back to the game follows (`Event::Resume`).
                    self.load_named(&name);
                }
                Event::Save(name) => self.write_menu_save(&name),
                Event::Resume => self.back_to_game(),
                Event::ReEnter => self.reenter_view(),
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
                // Done in the frame ([`Game::display_change`]).
                Event::DisplayChange(..) => {}
                Event::ReloadTexts => self.texts = None,
            }
        }
    }

    /// The options' `BT_BACK` with a size or depth pending
    /// (@ 0x00454f44..0x004551b1), the game's part, in the original's
    /// order after the page handler: the display re-created at `w` x `h`
    /// (@ 0x0040fa60; the window follows [`Game::width`] /
    /// [`Game::height`]) and the menus' page surface with it (black this
    /// frame, the menus did it); `Interface.spr` and the cursor (the
    /// port's are not size-bound); the inventory's and the screens' banks
    /// of the new width (@ 0x0043b060, @ 0x00429420, @ 0x004c34e0: here
    /// the subject menu's bank and the tower map's `CarteTour<W>.spr`,
    /// opened again at their next use); the warp's pipeline and camera
    /// (@ 0x0043f6f0, @ 0x0043f420: centre (W/2, H/2), the 76° field of
    /// view at the new size, the viewport; the orientation kept); the
    /// fonts made again for the new size (@ 0x0041e030, @ 0x0041d960).
    /// `Atlantis3.cfg`'s writer reads the size and depth of the display in
    /// use (@ 0x00408f80), so the settings take them. The warp init also
    /// re-allocates the six cube faces (@ 0x0043f420 → 0x004eb290: new
    /// buffers, not filled by the chain); page 1's `BT_BACK` reads the
    /// cube into them again ([`Game::reenter_view`]).
    ///
    /// The display's format follows the page surface's (`BT_16BITS` /
    /// `BT_32BITS`): the screen, the faces, the videos, the textures and
    /// every bank the chain loads again are in it from now on. The faces:
    /// the warp init's 0x004eb290 frees each buffer and takes a new one
    /// from `operator new` (CRT `malloc` → `HeapAlloc(_crtheap, 0, n)`,
    /// @ 0x006d42d3), 0x80000 + 0x3f bytes or more: above the Windows NT
    /// heap's virtual-block threshold, so fresh committed pages, zero
    /// filled, while the menu shows. Leaving the menu by page 1's
    /// `BT_BACK` enters the view again (@ 0x0043fd00): the cube read into
    /// the faces in the new format and the slots loaded again, so the
    /// panorama is back at once. Until then the slots keep their banks
    /// prepared for the old format, drawn by the new format's blitters
    /// ([`atlantis3_data::spr::Bank::as_drawn_at`]).
    pub(super) fn display_change(&mut self, w: usize, h: usize) {
        let (w, h) = (w.max(1), h.max(1));
        let depth = self.menus.depth;
        self.width = w;
        self.height = h;
        self.screen = crate::px::Surface::new(depth, w * h);
        match &mut self.screen {
            crate::px::Surface::Rgb565(s) if self.menus.surface.len() == s.len() => {
                s.copy_from_slice(&self.menus.surface)
            }
            crate::px::Surface::Xrgb8888(s) if self.menus.surface32.len() == s.len() => {
                s.copy_from_slice(&self.menus.surface32)
            }
            _ => {}
        }
        self.place.faces = match depth {
            atlantis3_data::spr::Depth::Rgb565 => {
                crate::px::Faces::Rgb565(std::array::from_fn(|_| vec![0; 512 * 512]))
            }
            atlantis3_data::spr::Depth::Xrgb8888 => {
                crate::px::Faces::Xrgb8888(std::array::from_fn(|_| vec![0; 512 * 512]))
            }
        };
        self.place.cube.clear();
        for slot in &mut self.slots.slots {
            if slot.bank.depth != depth {
                slot.bank = std::sync::Arc::new(slot.bank.as_drawn_at(depth));
            }
        }
        // The banks the chain loads again in the new format (Interface.spr,
        // the world's banks, the screens').
        if self.interface.as_ref().is_some_and(|b| b.depth != depth) {
            self.interface = self
                .files
                .read("spr/Interface.spr")
                .and_then(|d| atlantis3_data::spr::Bank::parse(&d, depth).ok());
        }
        self.arrows = None;
        if self.puzzles.screens.carte_bank.is_some() {
            self.carte_reload_banks();
        }
        if self.puzzles.screens.crane_bank.is_some() {
            self.crane_reload_banks();
        }
        if self.puzzles.screens.tele_bank.is_some() {
            self.tele_reload_banks();
        }
        let c = &mut self.startup.config;
        c.resolution = (w as i32, h as i32);
        c.pixel_format = match self.menus.depth {
            atlantis3_data::spr::Depth::Xrgb8888 => atlantis3_data::cfg::PixelFormat::X8R8G8B8,
            atlantis3_data::spr::Depth::Rgb565 => atlantis3_data::cfg::PixelFormat::R5G6B5,
        };
        // The banks sized by the screen's width, opened again at their
        // next use.
        self.banks_world = 0;
        self.puzzles.screens.tour_board = None;
        self.puzzles.screens.tour_pawns = None;
        // The warp at the new size.
        let a3 = self.renderer.a3;
        let fpu = self.renderer.fpu;
        self.renderer = atlantis2_engine::Renderer::new(atlantis2_engine::Variant::PII, fpu, w, h);
        self.renderer.a3 = a3;
        (self.camera.cx, self.camera.cy) = ((w / 2) as f32, (h / 2) as f32);
        self.camera
            .set_fov(atlantis2_engine::camera::WARP_FOV, w as u32, h as u32);
        // The fonts (and the texts with them) made again.
        self.texts = None;
        self.pointer = (
            self.pointer.0.clamp(0, w as i32 - 1),
            self.pointer.1.clamp(0, h as i32 - 1),
        );
    }

    /// Leaves the menu as the in-game menu's `BT_BACK` does (tests): back
    /// to the game, then the view entered again (@ 0x00454744..0x00454772).
    #[doc(hidden)]
    pub fn menu_back(&mut self) {
        self.back_to_game();
        self.reenter_view();
    }

    /// Leaves the menu by the back to the game alone (@ 0x00451280),
    /// without page 1's view re-entry (tests replaying a run whose menu
    /// was not modelled).
    #[doc(hidden)]
    pub fn menu_resume(&mut self) {
        self.back_to_game();
    }

    /// Back to the game (@ 0x00451280): loop mode 3.
    /// Back to the game (@ 0x00451280, the in-game menu's `BT_BACK`
    /// after its own `Interp_CloseInterrupt(1)`): `Interp_CloseInterrupt(1)`
    /// (a section waiting when the menu opened goes on: the lock cleared),
    /// then loop mode 3.
    fn back_to_game(&mut self) {
        self.with_script(|s, h| s.close_interrupt(h));
        self.page_left();
    }

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
        // The writer's text, its `DateAndTime` the local clock's.
        let text = self.save_text(&s);
        self.script = Some(s);
        self.store_save(name, text);
        if !atlantis3_data::save::has_thumbnail(name) {
            return;
        }
        let Menus {
            capture, encoder, ..
        } = &mut self.menus;
        let Some((pixels, w, h)) = capture.as_ref() else {
            return;
        };
        let banks = crate::menu::thumbnails_of(pixels, *w, *h, encoder);
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
            self.saves_written.push(key.clone());
            self.saves.insert(key, bank);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Option<Game> {
        let dir = atlantis3_data::testing::install_dir()?;
        Game::new(atlantis3_data::A3Files::open_dir(&dir).ok()?.0).ok()
    }

    /// The saves' `DateAndTime` (@ 0x00459ee0, `_strdate` / `_strtime`):
    /// the local time, for the script's quick saves as for the menu's.
    #[test]
    fn saves_are_stamped_with_the_local_time() {
        let Some(mut g) = game() else {
            return;
        };
        assert!(g.board_begin("hoggar.wam", 1));
        g.board_enter(1, 100.0);
        // 2023-11-14 22:13:20 UTC, one hour east of it.
        g.unix_time = || 1_700_000_000;
        g.utc_offset = || 3600;
        let want = "DateAndTime = 11/14/23 23:13:20";
        let name = g.quick_save_now(1);
        let text = String::from_utf8_lossy(g.saved(&name).unwrap()).into_owned();
        assert!(text.contains(want), "{text}");
        g.write_menu_save("game1_1.sav");
        let text = String::from_utf8_lossy(g.saved("game1_1.sav").unwrap()).into_owned();
        assert!(text.contains(want), "{text}");
    }
}

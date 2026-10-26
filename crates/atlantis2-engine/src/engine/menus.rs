//! The engine as the menus' [`MenuHost`], and the menu loop (loop type
//! 5): opening the pages, the frame, Escape, the pointer.

use atlantis2_data::save::SaveHeader;
use atlantis2_data::A2Files;

use super::{Engine, LoopType};
use crate::fpu::Fpu;
use crate::menu::{Menu, MenuHost, PageKind, Settings, HEIGHT, WIDTH};

impl Engine {
    /// Runs `f` on the menus with the engine as their host (the menus are
    /// loaded at the first use, as `AtlMenu_MainInit` at start-up).
    pub(super) fn with_menu<R>(&mut self, f: impl FnOnce(&mut Menu, &mut Engine) -> R) -> R {
        let mut m = self
            .menu
            .take()
            .unwrap_or_else(|| Box::new(Menu::new(&self.files)));
        // The menus read the game's one pointer (@ 0x006569c0,
        // 0x006569bc), the one the bag bar reads too.
        m.input.x = self.pointer.0.clamp(0, WIDTH as i32 - 1);
        m.input.y = self.pointer.1.clamp(0, HEIGHT as i32 - 1);
        m.input.held = self.button_held;
        m.date_text = self.date_text;
        let r = f(&mut m, self);
        self.menu = Some(m);
        r
    }

    /// The menus, when loaded.
    pub fn menu(&self) -> Option<&Menu> {
        self.menu.as_deref()
    }

    pub fn menu_mut(&mut self) -> Option<&mut Menu> {
        self.menu.as_deref_mut()
    }

    /// The player menu, shown at start-up (FUN_00489080 from
    /// FUN_0049d680): a symbol continues that player's newest save or
    /// starts a new game.
    pub fn open_player_menu(&mut self) {
        self.with_menu(|m, e| m.start_player(e));
    }

    /// The main menu (`AtlMenu_start` @ 0x00486930).
    pub fn open_main_menu(&mut self) {
        self.with_menu(|m, e| m.start_main(e));
    }

    /// A menu frame whose action is `act` (a test hook, see
    /// [`Menu::step_action`]).
    pub fn menu_step_action(&mut self, act: i32) {
        self.with_menu(|m, e| m.step_action(e, act));
    }

    /// The credits (`AtlMenu_start_Credits` @ 0x00486e00).
    pub fn open_credits(&mut self) {
        self.with_menu(|m, e| m.start_credits(e));
    }

    /// The main menu, then page `page` built in it (a testing aid: the
    /// pages the menus reach by their slides).
    pub fn open_menu_page(&mut self, page: PageKind) {
        self.with_menu(|m, e| {
            m.start_main(e);
            if page != PageKind::Main {
                m.show_page(e, page);
            }
        });
    }

    /// Escape (the key callback @ 0x004762f0), by loop type: the
    /// panorama opens the main menu, a dialog ends, a video goes on, the
    /// menu closes when it is the main menu opened from the game.
    pub fn escape(&mut self) {
        if self.hold_input(super::media::Input::Escape) {
            return;
        }
        match self.loop_type {
            LoopType::Warp | LoopType::Picture => self.open_main_menu(),
            LoopType::Dialog => self.end_dialog(),
            LoopType::Video => {}
            LoopType::Menu => self.with_menu(|m, e| m.escape(e)),
            // The key callback writes the key's code (@ 0x004762f9), which
            // the canoe's frame reads: Escape (1) quits it.
            LoopType::Canoe => self.canoe_escape = true,
        }
    }

    /// Escape went down or up: a key state, not an event (the credits
    /// end while it is held, @ 0x00472378).
    pub fn escape_key(&mut self, down: bool) {
        self.escape_down = down;
    }

    /// The pointer at (x, y) of the screen (FUN_0040c420 clamps it to the
    /// screen): the game's one pointer (@ 0x006569c0, 0x006569bc), which
    /// the menus and the bag bar both read.
    pub fn pointer(&mut self, x: i32, y: i32) {
        if self.hold_input(super::media::Input::Pointer(x, y)) {
            return;
        }
        let (w, h) = (self.width() as i32, self.height() as i32);
        self.pointer = (x.clamp(0, w - 1), y.clamp(0, h - 1));
        if let Some(m) = self.menu.as_mut() {
            m.input.x = x.clamp(0, WIDTH as i32 - 1);
            m.input.y = y.clamp(0, HEIGHT as i32 - 1);
        }
    }

    /// The left button released: the held flag (@ 0x006569ca) cleared
    /// in every loop type (FUN_0040c660 @ 0x0040c669), so a press that
    /// closed a menu does not leave the next menu's buttons still.
    pub fn release(&mut self) {
        if self.hold_input(super::media::Input::Release) {
            return;
        }
        self.button_held = false;
        if let Some(m) = self.menu.as_mut() {
            m.input.held = false;
        }
    }

    /// Whether the left button is held (@ 0x006569ca).
    pub fn button_held(&self) -> bool {
        self.button_held
    }

    /// Whether the menus want the program to end.
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// The menu loop's frame (@ 0x00486eb0).
    pub(super) fn menu_pass(&mut self, out: Option<&mut [u16]>) {
        if let Some(m) = self.menu.as_mut() {
            m.input.escape = self.escape_down;
        }
        let (w, h) = (self.renderer.width(), self.renderer.height());
        if w != WIDTH || h != HEIGHT {
            // The pages are laid out for 640 x 480; the menu loop draws
            // on the 640 x 480 surface (`crate::display`), so this is only
            // a surface of another size given by a caller.
            let mut page = vec![0u16; WIDTH * HEIGHT];
            self.with_menu(|m, e| m.frame(e, Some(&mut page)));
            if let Some(out) = out {
                out.fill(0);
                let (ox, oy) = ((w - WIDTH) / 2, (h - HEIGHT) / 2);
                for y in 0..HEIGHT.min(h) {
                    let d = (oy + y) * w + ox;
                    out[d..d + WIDTH.min(w)]
                        .copy_from_slice(&page[y * WIDTH..y * WIDTH + WIDTH.min(w)]);
                }
            }
            return;
        }
        self.with_menu(|m, e| {
            // The page's texts recorded for the HD mode.
            m.hd_texts = Some(std::mem::take(e.hd.texts_mut()));
            m.frame(e, out);
            if let Some(t) = m.hd_texts.take() {
                *e.hd.texts_mut() = t;
            }
        });
    }
}

impl MenuHost for Engine {
    fn files(&self) -> &A2Files {
        &self.files
    }

    fn fpu(&self) -> Fpu {
        self.camera.fpu
    }

    fn ticks(&self) -> u32 {
        self.ticks
    }

    fn clock(&self) -> f64 {
        self.clock
    }

    fn enter_menu_loop(&mut self) {
        if self.loop_type != LoopType::Menu {
            self.menu_prev_loop = self.loop_type;
        }
        // The turn speeds are kept (only FUN_0040c680 zeroes them, and no
        // menu code calls it): the menu frame runs no decay, and the
        // pointer's moves add to them ([`Engine::pointer_moved`]).
        self.set_loop_type(LoopType::Menu);
    }

    fn leave_menu_loop(&mut self) {
        self.set_loop_type(self.menu_prev_loop);
        self.click = false;
    }

    fn game_screen(&mut self) -> Vec<u16> {
        // At 800 wide the backdrop is black (FUN_00470430 tests the
        // screen's width, FUN_00402dd0).
        if self.display_large() {
            return vec![0; WIDTH * HEIGHT];
        }
        let (w, h) = (self.renderer.width(), self.renderer.height());
        let mut out = vec![0u16; w * h];
        self.render_surface(&mut out);
        if w == WIDTH && h == HEIGHT {
            out
        } else {
            vec![0; WIDTH * HEIGHT]
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            subtitles: self.subtitles,
            centre_marker: self.centre_marker,
            invert_y: self.look.invert_y,
            width: self.cfg_extra.width,
            height: self.cfg_extra.height,
            general: self.music.master,
            music: self.music.max,
            effects: self.fx_volume,
            dialogs: self.voice_volume,
        }
    }

    fn set_settings(&mut self, s: Settings) {
        self.subtitles = s.subtitles;
        self.centre_marker = s.centre_marker;
        self.look.invert_y = s.invert_y;
        self.cfg_extra.width = s.width;
        self.cfg_extra.height = s.height;
        if self.music.master != s.general {
            self.music.master = s.general;
            self.voices
                .set_attenuation(crate::sound::VOICE_MUSIC, self.music.attenuation());
        }
        self.fx_volume = s.effects;
        self.voice_volume = s.dialogs;
    }

    fn set_music_volume(&mut self, v: i32) {
        // FUN_00478100 accepts 0..127; FUN_00476f80 sets the current
        // music's volume to it.
        if (0..=127).contains(&v) {
            self.music.max = v;
            self.music.volume = v;
            self.voices
                .set_attenuation(crate::sound::VOICE_MUSIC, self.music.attenuation());
        }
    }

    fn set_player(&mut self, player: u8) {
        self.player_number = player;
    }

    fn player(&self) -> u8 {
        self.player_number
    }

    fn saves(&mut self) -> Vec<(u8, SaveHeader)> {
        Engine::saves(self)
    }

    fn save(&mut self, slot: u8) {
        self.save_slot(slot);
    }

    fn load(&mut self, slot: u8) {
        if let Err(e) = self.load_slot(slot) {
            log::warn!("AtlSaveGame_Load({slot}): {e}");
        }
    }

    fn delete(&mut self, slot: u8) {
        self.delete_slot(slot);
    }

    fn new_game(&mut self) {
        if let Err(e) = self.start_new_game() {
            log::error!("new game: {e}");
        }
    }

    fn quit(&mut self) {
        self.quit_requested = true;
    }

    fn disc_present(&mut self, n: u8) -> bool {
        // FUN_0040e090: the drives looked at again; here the folders the
        // discs were found in.
        self.files.refresh_discs();
        self.files.disc_numbers().contains(&n)
    }

    fn disc_inserted(&mut self) {
        self.with_script(|s, h| s.disc_inserted(h));
    }

    fn menu_cursor(&mut self) {
        self.set_cursor_pair(super::CursorBank::Menu, 0);
    }

    fn resume_section(&mut self) -> bool {
        self.with_script(|s, h| {
            let pending = s.resume_pending;
            s.disc_inserted(h);
            pending
        })
        .unwrap_or(false)
    }
}

//! The menus: the player screen (SELEMENU), the in-game menu (MAINMENU),
//! the options (OPTMENU) and sound (SONMENU) menus, the load picker
//! (LOADMENU), the confirmation (SUREMENU), the credits and the end.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`menus.cpp`,
//! `engine.cpp` `playQuitOutro`; GPLv3, the ScummVM team), which follows
//! `atlantis.exe`: `FUN_0041dea4` (player screen), `FUN_0041e8a8` (delete),
//! `FUN_0041c838` (in-game menu), `FUN_0041d5f4` (options), `FUN_0041d940`
//! (sound), `FUN_0041cf78` (load picker), `FUN_0041ec7c` (confirmation),
//! `FUN_0041a72c` (credits), `FUN_0041a66c` (the end).
//!
//! Every menu draws itself by walking its file ([`walk_menu_text`]) over a
//! backdrop each frame, and hit-tests the records of that walk; the buttons
//! are found by record number, as the original does, so that the menus of
//! every language work.

use std::collections::VecDeque;

use atlantis_data::saves::{checkpoint_chapter, epi_episode_name, MAX_PLAYERS};
use atlantis_data::sprite::read_tga15;
use atlantis_data::LoadList;

use super::credits::{credits_events, CreditsEvent};
use super::layout::{
    font_min_load_list_width, load_picker_knob_record, menu_grid_hit, menu_number,
    menu_volume_from_knob, menu_walk_hit, name_key_code, spell_sharp_s, subtitle_toggle_click,
    walk_menu_text_in, FontMin, LoadPickerScroll, MenuRecord, MenuRecordTable, MenuRules, MenuWalk,
};
use super::player_screen::{
    player_screen_exit, MenuDoubleClick, PlayerScreenExit, PlayerScreenState,
};
use super::{dim_screen, draw_sprite, Screen};
use crate::engine::Engine;
use crate::music::apc_pcm;
use crate::sys::{Input, Key, MouseButton};
use crate::warp::{SCREEN_H, SCREEN_W};

const N: usize = SCREEN_W * SCREEN_H;

/// The input of one frame of a menu loop.
#[derive(Default)]
pub(crate) struct MenuInput {
    pub left_press: bool,
    pub right_press: bool,
    pub escape: bool,
    pub enter: bool,
    pub up: bool,
    pub down: bool,
    /// The WM_CHAR codes of the keys, in order (the code the original's
    /// window stores, `name_key_code`; 8 Backspace, 0x0d Enter, 0x1b
    /// Escape).
    pub codes: Vec<u8>,
}

/// A loaded menu file and its walk state.
pub(crate) struct Menu {
    pub text: Vec<u8>,
    pub number: i32,
    pub walk: MenuWalk,
    /// The build's walk rules, with the port's fixes from the minor bug
    /// fixes when the menu was opened ([`MenuRules::of_build`]).
    pub rules: MenuRules,
}

impl Menu {
    /// The menu's block in the record table.
    fn table_menu(&self) -> Option<usize> {
        usize::try_from(self.number)
            .ok()
            .filter(|&m| m < MenuRecordTable::MENUS)
    }

    /// Walks the file (`FUN_0041fb6c`) with the hovered line and the
    /// options toggles, its records in the session's `table`
    /// ([`Engine::menu_table`]) or, without one, its own.
    pub fn walk(
        &mut self,
        hover: Option<usize>,
        toggle_a: i32,
        toggle_b: i32,
        table: Option<&mut MenuRecordTable>,
    ) {
        let hover = hover.map_or(-1, |h| h as i32);
        walk_menu_text_in(
            &self.text,
            self.number,
            hover,
            toggle_a,
            toggle_b,
            self.rules,
            &mut self.walk,
            table,
        );
    }

    /// The record under the point (`FUN_004210c0`), read from the table as
    /// it stands (what the walks and the pickers left there).
    pub fn hit(&self, x: i32, y: i32, table: Option<&MenuRecordTable>) -> Option<usize> {
        match (table, self.table_menu()) {
            (Some(t), Some(m)) => menu_walk_hit(&t.records(m), x, y),
            _ => menu_walk_hit(&self.walk.records, x, y),
        }
    }

    /// Record `k` as the next walk will find it.
    pub fn stored_record(&self, k: usize, table: Option<&MenuRecordTable>) -> MenuRecord {
        match (table, self.table_menu()) {
            (Some(t), Some(m)) => t.record(m, k),
            _ => self.walk.records.get(k).copied().unwrap_or_default(),
        }
    }

    /// Writes record `k` for the next walk (the LOAD knob, the sliders).
    pub fn set_stored_record(
        &mut self,
        k: usize,
        r: MenuRecord,
        table: Option<&mut MenuRecordTable>,
    ) {
        match (table, self.table_menu()) {
            (Some(t), Some(m)) => t.set_record(m, k, r),
            _ => {
                if self.walk.records.len() <= k {
                    self.walk.records.resize(k + 1, MenuRecord::default());
                }
                self.walk.records[k] = r;
            }
        }
    }
}

/// What the in-game menu leads to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuOutcome {
    /// Back to the game.
    Resume,
    /// The player's checkpoint is reloaded (the load picker's OK, RESTART
    /// EPISODE or a double click).
    Reload,
    /// The game ends (the credits have played).
    Quit,
}

/// `FUN_0041c838` step 4: the entry of the record clicked, 0 load, 1
/// options, 2 quit; the picker's exit (5 RESTART, 6 OK, 7 Cancel) and the
/// confirmation's (3 OK, 4 Cancel) decide the outcome; any other record
/// resumes.
pub fn ingame_menu_outcome(record: Option<usize>, load_exit: u8, quit_exit: u8) -> MenuOutcome {
    match record {
        Some(0) if load_exit != 7 => MenuOutcome::Reload,
        Some(2) if quit_exit == 3 => MenuOutcome::Quit,
        _ => MenuOutcome::Resume,
    }
}

impl Engine {
    pub(crate) fn poll_menu_input(&self) -> MenuInput {
        self.sys.mark_menu_input();
        let mut mi = MenuInput::default();
        while let Some(ev) = self.sys.poll_event() {
            match ev {
                Input::MouseDown {
                    button: MouseButton::Left,
                    ..
                } => mi.left_press = true,
                Input::MouseDown {
                    button: MouseButton::Right,
                    ..
                } => mi.right_press = true,
                Input::KeyDown(Key::Escape) => {
                    mi.escape = true;
                    mi.codes.push(0x1b);
                }
                Input::KeyDown(Key::Enter) => {
                    mi.enter = true;
                    mi.codes.push(0x0d);
                }
                Input::KeyDown(Key::Backspace) => mi.codes.push(8),
                Input::KeyDown(Key::Up) => mi.up = true,
                Input::KeyDown(Key::Down) => mi.down = true,
                Input::KeyDown(Key::Space) => mi.codes.push(b' '),
                Input::KeyDown(Key::Char(c)) => {
                    let code = name_key_code(c);
                    if code != 0 {
                        mi.codes.push(code);
                    }
                }
                _ => {}
            }
        }
        mi
    }

    /// `SPRLIST\<name>`, `None` when the archive lacks it. Not walked
    /// yet: each menu walks it before its first hit test, with its toggles.
    pub(crate) async fn load_menu(&self, name: &str) -> Option<Menu> {
        let text = self.read(&format!("SPRLIST\\{name}")).await?;
        self.emit(crate::engine::Event::Menu(name.to_string()));
        let number = menu_number(name).map_or(-1, |n| n as i32);
        Some(Menu {
            text,
            number,
            walk: MenuWalk::default(),
            rules: MenuRules::of_build(self.game.exe.build, self.enhancements().minor_bug_fixes),
        })
    }

    /// `images\<name>` full screen, black around it.
    pub(crate) async fn load_picture(&self, name: &str) -> Screen {
        let mut s = vec![0u16; N];
        if let Some(img) = self
            .read(&format!("IMAGES\\{name}"))
            .await
            .and_then(|d| read_tga15(&d))
        {
            for y in 0..img.height.min(SCREEN_H) {
                let n = img.width.min(SCREEN_W);
                s[y * SCREEN_W..y * SCREEN_W + n]
                    .copy_from_slice(&img.pixels[y * img.width..y * img.width + n]);
            }
        }
        s
    }

    /// The view as the warp loop last drew it, without the cursor (the
    /// in-game menu's backdrop), dimmed (`FUN_0041cc28`).
    pub(crate) fn dimmed_view(&mut self) -> Screen {
        if self.warp_loaded {
            self.render_view();
            self.sys.hd_unmark(); // dimmed: no HD view
        }
        let mut bg = self.sys.screen.borrow().clone();
        if !self.warp_loaded {
            bg.fill(0);
        }
        dim_screen(&mut bg);
        bg
    }

    /// Shows a frame: `screen` with the arrow pointer.
    pub(crate) async fn present(&self, screen: &[u16], pointer: bool) {
        {
            let mut s = self.sys.screen.borrow_mut();
            s.copy_from_slice(screen);
            if pointer {
                self.draw_pointer(&mut s);
            }
        }
        self.sys.frame().await;
    }

    /// The player screen `FUN_0041dea4`: pick a player, type a new one on
    /// the "new player" row, delete one (the blocks of
    /// [`PlayerScreenState`], in the original's order). Returns the
    /// player's row, `None` when the window is closed: the loop reads no
    /// Escape (it ends only on OK, Enter or a double click with a valid
    /// name; WM_CHAR 0x1b is no name character).
    pub async fn player_screen(&mut self) -> Option<usize> {
        let bg = self.load_picture("PREINTRO.TGA").await;
        let mut menu = self.load_menu("SELEMENU.TXT").await?;
        self.load_fontmin_from_current_disc().await;
        // The rows: FUN_004211f0 at (175, 248), cells 320 x 22, one column,
        // 0x00496606 rows (0x0041e08e); its inclusive far edges give an
        // index past the list, which the original copies from the names
        // array (what it holds there is not known): the port takes it as a
        // click off the rows.
        let (grid_x, grid_y, grid_w, grid_h) = (175, 248, 320, 22);
        let build = self.game.exe.build;
        let (upper_case, dos_letters) = (build.upper_case_names, build.name_entry_dos_letters);
        let list_font = if build.players_1997 {
            FontMin::PlayerList1997
        } else {
            FontMin::PlayerList
        };
        let mut st = PlayerScreenState::default();
        let mut dbl = MenuDoubleClick::default();
        // The window procedure's WM_CHAR codes, one a frame.
        let mut keys: VecDeque<u8> = VecDeque::new();
        menu.walk(None, 0, 0, self.menu_table());
        self.wait_buttons_up().await;
        while self.sys.poll_event().is_some() {}
        loop {
            if self.sys.should_quit() {
                return None;
            }
            let mi = self.poll_menu_input();
            keys.extend(&mi.codes);
            if let Some(k) = keys.pop_front() {
                st.key = k;
            }
            let (mx, my) = self.sys.mouse();
            let rows = self.players.count as i32;
            let hover_row = menu_grid_hit(grid_x, grid_y, grid_w, grid_h, 1, rows, true, mx, my)
                .filter(|&r| r < rows);
            let record = menu.hit(mx, my, self.menu_table().as_deref());
            // WM_LBUTTONDBLCLK sets the flag wherever the press lands; the
            // port arms it before the frame's click block (which of the two
            // the original runs first on the second press is not known).
            if mi.left_press && dbl.press(self.sys.millis(), (mx, my)) {
                st.dbl_flag = true;
            }
            if st.click(
                mi.left_press,
                record,
                hover_row.unwrap_or(-1),
                &self.players,
            ) {
                // FUN_0041e8a8: ARE YOU SURE ? over the bare title backdrop,
                // then the delete.
                let row = st.row as usize;
                let confirmed = self.confirm(&bg).await;
                if confirmed {
                    self.delete_player(row);
                }
                self.wait_buttons_up().await;
                st.delete_done(confirmed);
            }
            // FUN_00420f60 draws the field before the name entry
            // (0x0041e20d): what is shown lags the blink by one frame.
            let shown = st.field.clone();
            let mut exit = st.exit_before_draw(&self.players, upper_case, dos_letters);
            if exit == PlayerScreenExit::None {
                // The frame: the backdrop, the menu, the rows (FUN_00420d8c:
                // x 185, y 270 + 22 j, FONTMIN2 under the mouse), the name
                // field (FUN_00420f60 at (160, 190)) when not empty.
                menu.walk(record, 0, 0, self.menu_table());
                let mut s = bg.clone();
                self.ui.draw_menu(&mut s, &menu.walk);
                let sharp_s_as_ss = self.enhancements().minor_bug_fixes;
                for j in 0..self.players.count {
                    let name = spell_sharp_s(self.players.name(j), sharp_s_as_ss).into_owned();
                    self.ui.draw_font_min(
                        &mut s,
                        list_font,
                        &name,
                        185,
                        grid_y + (j as i32 + 1) * grid_h,
                        hover_row == Some(j as i32),
                    );
                }
                if !shown.is_empty() {
                    self.ui
                        .draw_font_min(&mut s, FontMin::NameField, &shown, 160, 190, true);
                }
                self.present(&s, true).await;
                exit = st.exit_after_draw(mi.left_press, record);
                if exit == PlayerScreenExit::None {
                    continue;
                }
            }
            // The exit and its commit; players.atl written on record 5.
            let keep_fifth_name = self.enhancements().minor_bug_fixes;
            let end = player_screen_exit(&mut st, exit, record, &mut self.players, keep_fifth_name);
            if end.write {
                self.save_players();
            }
            if let Some(row) = end.chosen {
                return Some(row);
            }
        }
    }

    /// `FUN_0041e8a8`: the player of `row` leaves the list. The port deletes
    /// the player's own checkpoints (the original deletes those of the
    /// letter of the row, which after an earlier deletion are another
    /// player's); the later players move up with their letters, so their
    /// checkpoints follow them.
    pub fn delete_player(&mut self, row: usize) {
        let Some(letter) = self.players.delete(row) else {
            return;
        };
        for name in self.saves.list() {
            if checkpoint_chapter(&name, letter).is_some() {
                self.saves.remove(&name);
            }
        }
        self.save_players();
    }

    /// The confirmation `FUN_0041ec7c` (SUREMENU) over `bg`: true for OK
    /// (record 3), false for Cancel (record 4); it reads no key.
    pub(crate) async fn confirm(&mut self, bg: &[u16]) -> bool {
        let Some(mut menu) = self.load_menu("SUREMENU.TXT").await else {
            return true;
        };
        menu.walk(None, 0, 0, self.menu_table());
        self.wait_buttons_up().await;
        loop {
            if self.should_abort() {
                return false;
            }
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let hit = menu
                .hit(mx, my, self.menu_table().as_deref())
                .filter(|&r| r == 3 || r == 4);
            if mi.left_press {
                if let Some(r) = hit {
                    self.wait_buttons_up().await;
                    return r == 3;
                }
            }
            menu.walk(hit, 0, 0, self.menu_table());
            let mut s = bg.to_vec();
            self.ui.draw_menu(&mut s, &menu.walk);
            self.present(&s, true).await;
        }
    }

    /// The in-game menu `FUN_0041c838` over the dimmed view: the loop runs
    /// until a left click, whatever is under the pointer; then the record
    /// clicked runs its entry once (0 load, 1 options, 2 quit) and the menu
    /// ends whatever the entry did. No key is read.
    pub async fn ingame_menu(&mut self) -> MenuOutcome {
        let bg = self.dimmed_view();
        let Some(mut menu) = self.load_menu("MAINMENU.TXT").await else {
            return MenuOutcome::Resume;
        };
        menu.walk(None, 0, 0, self.menu_table());
        self.wait_buttons_up().await;
        while self.sys.poll_event().is_some() {}
        let mut outcome = MenuOutcome::Resume;
        loop {
            if self.should_abort() {
                break;
            }
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let hit = menu.hit(mx, my, self.menu_table().as_deref());
            if mi.left_press {
                self.wait_buttons_up().await;
                match hit {
                    Some(0) => {
                        let exit = self.load_picker(&bg).await;
                        outcome = ingame_menu_outcome(hit, exit, 0);
                    }
                    Some(1) => self.options_menu(&bg).await,
                    Some(2) => {
                        let ok = self.confirm(&bg).await;
                        outcome = ingame_menu_outcome(hit, 0, if ok { 3 } else { 4 });
                        if outcome == MenuOutcome::Quit {
                            self.play_credits().await;
                        }
                    }
                    _ => {}
                }
                break;
            }
            menu.walk(hit, 0, 0, self.menu_table());
            let mut s = bg.clone();
            self.ui.draw_menu(&mut s, &menu.walk);
            self.present(&s, true).await;
        }
        // The timers resume from their values (0x0041cb84) when the play
        // goes on: the caller rebases them on a resume.
        outcome
    }

    /// The options `FUN_0041d5f4` (OPTMENU): records 1 SOUND OPTIONS, 3
    /// the SUBTITLES toggle, 5 the OMNI 3D toggle, 6 OK. It ends on SOUND
    /// OPTIONS (after the sound menu) or OK; no key is read.
    pub(crate) async fn options_menu(&mut self, bg: &[u16]) {
        let Some(mut menu) = self.load_menu("OPTMENU.TXT").await else {
            return;
        };
        // The first walk is made with the toggles (atlantis-cd-zh-tw-696832
        // FUN_0041d5c0 @ 0x0041d5e7..0x0041d5ec): with the toggles as
        // sprite records (the Chinese builds), the id 9 record of a walk
        // with SUBTITLES off keeps the x / y the session's table holds from
        // an earlier visit (see MenuRules::toggle_in_step).
        menu.walk(
            None,
            i32::from(self.subtitles),
            i32::from(self.settings.omni3d_mode2),
            self.menu_table(),
        );
        // The hovered record is the one under the pointer, whatever it is
        // (@ 0x0041d5f1..0x0041d600): with the sprite menus the labels
        // SUBTITLES and OMNI 3D (records 2 and 4) are drawn hovered too
        // (the text menus never draw lines 2 and 4 of this menu hovered).
        let sprite_menus = self.game.exe.build.menu_sprite_banks.is_some();
        let mut changed = false;
        loop {
            if self.should_abort() {
                break;
            }
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let under = menu.hit(mx, my, self.menu_table().as_deref());
            let hit = under.filter(|r| [1, 3, 5, 6].contains(r));
            if mi.left_press {
                self.wait_buttons_up().await;
                match hit {
                    Some(1) => {
                        self.sound_menu(bg).await;
                        break;
                    }
                    Some(3) => {
                        let mut on = self.subtitles;
                        let cycle = self.game.exe.build.subtitle_click_cycle;
                        subtitle_toggle_click(&mut on, &mut self.subtitle_clicks, cycle);
                        self.subtitles = on;
                        self.settings.subtitles = on;
                        changed = true;
                    }
                    Some(5) => {
                        // 0x0041d706: 0x004960ac = (0x004960ac == 0).
                        self.settings.omni3d_mode2 = !self.settings.omni3d_mode2;
                        changed = true;
                    }
                    Some(6) => break,
                    _ => {}
                }
            }
            menu.walk(
                if sprite_menus { under } else { hit },
                i32::from(self.subtitles),
                i32::from(self.settings.omni3d_mode2),
                self.menu_table(),
            );
            let mut s = bg.to_vec();
            self.ui.draw_menu(&mut s, &menu.walk);
            self.present(&s, true).await;
        }
        if changed {
            self.save_settings();
        }
    }

    /// The sound menu `FUN_0041d940` (SONMENU): records 1..4 the slider
    /// tracks, 5..8 their knobs, 14 / 15 the left / right channel tests, 16
    /// OK, 17 Cancel. A press latches the record under the pointer; with a
    /// track latched its knob follows mouse x - 10 while 315 < x < 528. The
    /// knobs keep their place for the session (never recomputed from the
    /// volumes). OK sets the volumes (x - 305) / 213 * 127 in the order
    /// master, music, effects, voices; Cancel sets nothing.
    pub(crate) async fn sound_menu(&mut self, bg: &[u16]) {
        let Some(mut menu) = self.load_menu("SONMENU.TXT").await else {
            return;
        };
        // The walk writes its 18 records into the session's table (the
        // reference FUN_0041d940, atlantis-cd-en-688128 FUN_0041d218 @
        // 0x0041d2b9): in a 16-record build its records 16 and 17 land on
        // SUREMENU's count byte and records 0-1. The knobs are its records
        // 5..8, whose x the sliders write (atlantis-cd-en-688128 @
        // 0x0041d2b2) and the walk keeps (son_knob_x mirrors them).
        menu.walk(None, 0, 0, self.menu_table());
        for t in 0..4 {
            if self.son_knob_x[t] == 0 {
                let r = menu.stored_record(5 + t, self.menu_table().as_deref());
                self.son_knob_x[t] = i32::from(r.x);
            }
        }
        let set_knobs = |e: &mut Engine, menu: &mut Menu| {
            for t in 0..4 {
                let mut r = menu.stored_record(5 + t, e.menu_table().as_deref());
                r.x = e.son_knob_x[t] as i16;
                menu.set_stored_record(5 + t, r, e.menu_table());
            }
        };
        let mut grabbed: Option<usize> = None;
        let mut commit = false;
        loop {
            if self.should_abort() {
                break;
            }
            set_knobs(self, &mut menu);
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let hit = menu.hit(mx, my, self.menu_table().as_deref());
            let down = self.sys.buttons().0;
            if mi.left_press {
                grabbed = hit;
            } else if !down {
                grabbed = None;
            }
            if let Some(g) = grabbed.filter(|g| (1..=4).contains(g)) {
                if down && mx > 315 && mx < 528 {
                    self.son_knob_x[g - 1] = mx - 10;
                }
            }
            if mi.left_press {
                match hit {
                    Some(14) | Some(15) => self.channel_test(hit == Some(15)).await,
                    Some(16) => {
                        commit = true;
                        break;
                    }
                    Some(17) => break,
                    _ => {}
                }
            }
            set_knobs(self, &mut menu);
            menu.walk(hit, 0, 0, self.menu_table());
            let mut s = bg.to_vec();
            self.ui.draw_menu(&mut s, &menu.walk);
            self.present(&s, true).await;
        }
        self.wait_buttons_up().await;
        if commit {
            for t in 0..4 {
                self.settings.volumes[t] = menu_volume_from_knob(self.son_knob_x[t]);
            }
            self.apply_volumes();
            self.save_settings();
        }
    }

    /// The channel test (`FUN_0041db4c` / `FUN_0041dcf8`): `WAV\VAGUE_10.APC`
    /// hard left or right at the effects volume; the menu runs on while it
    /// plays (the original's click wait ends at once).
    async fn channel_test(&mut self, right: bool) {
        if let Some(v) = self.channel_test_voice.take() {
            self.sys.mixer.stop(v);
        }
        if let Some(pcm) = self
            .read("WAV\\VAGUE_10.APC")
            .await
            .and_then(|d| apc_pcm(&d))
        {
            let vol = (u32::from(self.ambient.effects_volume.min(127)) * 255 / 127) as u8;
            self.channel_test_voice = Some(self.sys.mixer.play(
                pcm,
                false,
                vol,
                if right { 127 } else { -127 },
            ));
        }
    }

    /// The current player's checkpoints, ascending chapters (the picker's
    /// `p<letter>*.gam`, sorted).
    /// A chapter-0 file is left out: the original lists it but its rows
    /// drawer skips it (`atoi(name + 2) - 1 == -1`, FUN_00420b48 @
    /// 0x00420be0..0x00420bed), and its click would walk EPI.TXT without
    /// an end; no build writes one.
    pub(crate) fn player_checkpoints(&self) -> Vec<u8> {
        let letter = self.players.letters[self.player.min(MAX_PLAYERS - 1)];
        let mut v: Vec<u8> = self
            .saves
            .list()
            .iter()
            .filter_map(|n| checkpoint_chapter(n, letter))
            .filter(|&c| c != 0)
            .collect();
        v.sort_unstable();
        v.dedup();
        v.truncate(0x80);
        v
    }

    /// The load picker `FUN_0041cf78` (LOADMENU): the player's checkpoints
    /// by their EPI.TXT names, 11 rows and a scrollbar. A row click stores
    /// its chapter in the player's chapter byte (memory only); records 5
    /// RESTART EPISODE (the byte = the current chapter, once there is a
    /// file) and 6 OK end it for a reload, 7 Cancel; so does a double click
    /// anywhere once a title is shown, as the record under the pointer.
    /// Returns the record it ended on (255 for a double click off them).
    pub(crate) async fn load_picker(&mut self, bg: &[u16]) -> u8 {
        let Some(mut menu) = self.load_menu("LOADMENU.TXT").await else {
            return 7;
        };
        self.load_fontmin_from_current_disc().await;
        let chapters = self.player_checkpoints();
        // The rows, their widths and the title, with the port's `ß` fix
        // (the original draws no glyph for 0xe1 and advances 7).
        let sharp_s_as_ss = self.enhancements().minor_bug_fixes;
        let names: Vec<Vec<u8>> = chapters
            .iter()
            .map(|&c| {
                let name = epi_episode_name(&self.epi_text, u32::from(c));
                spell_sharp_s(&name, sharp_s_as_ss).into_owned()
            })
            .collect();
        let count = chapters.len();
        let build = self.game.exe.build;
        let clamp = build.load_picker_clamp;
        let mut sb = LoadPickerScroll::new(count as i32, build.load_knob_starts_at_bar, clamp);
        // The entry's stores of the knob (0x0041d1f3..0x0041d24b), then the
        // first walk.
        let knob =
            load_picker_knob_record(menu.stored_record(4, self.menu_table().as_deref()), &sb);
        menu.set_stored_record(4, knob, self.menu_table());
        menu.walk(None, 0, 0, self.menu_table());
        let sprite_rows = matches!(build.load_list, LoadList::Sprites { .. });
        let mut title: Option<(Vec<u8>, i32)> = None;
        // The Chinese builds' title: EPICHI2 frame `row` at x.
        let mut title_frame: Option<(usize, i32)> = None;
        let mut selected: Option<usize> = None;
        // The double-click flag 0x00496118: set by any double click,
        // wherever it lands, and cleared only when the picker ends
        // (0x0041d50f).
        let mut dbl = MenuDoubleClick::default();
        let mut dbl_flag = false;
        let mut track_latched = false;
        let player = self.player.min(MAX_PLAYERS - 1);
        let choose = |e: &mut Engine,
                      i: usize,
                      title: &mut Option<(Vec<u8>, i32)>,
                      title_frame: &mut Option<(usize, i32)>| {
            if sprite_rows {
                // atlantis-cd-zh-tw-696832 FUN_0041ce00: the chapter of the
                // row's position in the table, not the file's
                // (@ 0x0041d413..0x0041d44d), the title frame centred by
                // the width table (@ 0x0041d3d0..0x0041d3ee); the title is
                // drawn while the file's EPI.TXT line copied to 0x00602080
                // is not empty (@ 0x0041d37c..0x0041d3aa, 0x0041d452..0x0041d476).
                let exe = &e.game.exe;
                e.players.chapters[player] =
                    load_list_chapter(&exe.load_chapters, &exe.load_widths, i);
                let w = e.game.exe.load_widths.get(i).copied().unwrap_or(0);
                *title_frame = (!names[i].is_empty()).then_some((i, (640 - i32::from(w)) / 2));
                return;
            }
            e.players.chapters[player] = chapters[i];
            let w = font_min_load_list_width(&names[i], &e.ui.fontmin_advance);
            *title = Some((names[i].clone(), (640 - w) / 2));
        };
        loop {
            if self.should_abort() {
                return 7;
            }
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let down = self.sys.buttons().0;
            // Port keys (the original reads none here; Escape does
            // nothing, as in the original): Enter is OK, Up / Down a row
            // click.
            if mi.enter {
                return 6;
            }
            if count > 0 && (mi.up || mi.down) {
                let i = match selected {
                    None => 0,
                    Some(i) if mi.up => i.saturating_sub(1),
                    Some(i) => (i + 1).min(count - 1),
                };
                selected = Some(i);
                choose(self, i, &mut title, &mut title_frame);
            }
            if mi.left_press {
                // WM_LBUTTONDOWN latches the record under the pointer
                // (0x00419c47): the track when strictly inside it.
                track_latched = menu.hit(mx, my, self.menu_table().as_deref()) == Some(1);
            }
            if !down {
                track_latched = false;
            }
            sb.frame(track_latched, down, my);
            let scroll = usize::from(sb.offset);
            // A row counts when row + offset < count, the only bound
            // (0x0041d37b; atlantis-cd-fr-687104 FUN_0041c708 @
            // 0x0041cb0b..0x0041cb1b): the grid's inclusive far edges (x
            // 445, y 350), which give 11..22, select the entry below the
            // visible rows (its title shown, its chapter stored, no visible
            // row lit). The English 1997 build tests the row alone
            // (atlantis-cd-en-688128 FUN_0041c9a8 @ 0x0041cd87) and takes
            // entry row + offset (0x0041cdaa): past the list it would read
            // an empty record (its chapter -1 then walks EPI.TXT without an
            // end): the port takes no such row.
            let row_hover = menu_grid_hit(45, 108, 400, 22, 1, 11, true, mx, my)
                .map(|g| scroll + g as usize)
                .filter(|&r| r < count);
            let button = menu
                .hit(mx, my, self.menu_table().as_deref())
                .filter(|r| [5, 6, 7].contains(r));
            if mi.left_press && dbl.press(self.sys.millis(), (mx, my)) {
                dbl_flag = true;
            }
            if mi.left_press && !track_latched {
                if let Some(r) = row_hover {
                    selected = Some(r);
                    choose(self, r, &mut title, &mut title_frame);
                }
            }
            // 0x0041d4ad: once a title is shown, the double-click flag ends
            // the picker in that frame, whatever the pointer is over (a
            // double click before the row click counts too), as a press on
            // the hovered record would: RESTART EPISODE stores the chapter,
            // Cancel reloads nothing (0x0041d519, FUN_0041c838).
            let title_shown =
                title.as_ref().is_some_and(|(t, _)| !t.is_empty()) || title_frame.is_some();
            if dbl_flag && title_shown {
                self.wait_buttons_up().await;
                if button == Some(5) && count > 0 {
                    self.players.chapters[player] = self.chapter as u8;
                }
                return button.map_or(255, |b| b as u8);
            }
            if mi.left_press && !track_latched {
                if let Some(b) = button {
                    self.wait_buttons_up().await;
                    if b == 5 && count > 0 {
                        self.players.chapters[player] = self.chapter as u8;
                    }
                    return b as u8;
                }
            }
            // The knob, record 4: the picker writes it, the walk draws it
            // at its stored x / y (in the session's table, an x 0xffce
            // stays for the later visits).
            let knob =
                load_picker_knob_record(menu.stored_record(4, self.menu_table().as_deref()), &sb);
            menu.set_stored_record(4, knob, self.menu_table());
            menu.walk(button, 0, 0, self.menu_table());
            let mut s = bg.to_vec();
            self.ui.draw_menu(&mut s, &menu.walk);
            if sprite_rows {
                self.draw_load_sprite_rows(&mut s, count, scroll, (mx, my), title_frame);
                self.present(&s, true).await;
                continue;
            }
            // FUN_00420b48: rows at x 55, baseline 130 + 22 i.
            for i in 0..11 {
                let Some(name) = names.get(scroll + i) else {
                    break;
                };
                self.ui.draw_font_min(
                    &mut s,
                    FontMin::LoadList,
                    name,
                    55,
                    130 + 22 * i as i32,
                    row_hover == Some(scroll + i),
                );
            }
            if let Some((t, x)) = &title {
                self.ui
                    .draw_font_min(&mut s, FontMin::NameField, t, *x, 0x50, true);
            }
            self.present(&s, true).await;
        }
    }

    /// The Chinese builds' LOAD rows and title (atlantis-cd-zh-tw-696832):
    /// FUN_004205a4 draws EPICHI1 frames `offset` .. `offset` + min(count,
    /// 11) at (0x37, 0x6e + 22 row), whatever files the rows hold (a list
    /// scrolled one row past its end shows the next frame), EPICHI2 for
    /// the grid row under the pointer (@ 0x004205c1..0x00420691; the grid
    /// of FUN_00420c08 @ 0x0041d277..0x0041d2a0); FUN_004208bc then draws
    /// the title, EPICHI2 frame `row` at (x, 0x3c) (@ 0x00420a22..0x00420a8d).
    /// A frame past the banks' 74 is not drawn (FUN_00410430).
    fn draw_load_sprite_rows(
        &self,
        s: &mut [u16],
        count: usize,
        scroll: usize,
        (mx, my): (i32, i32),
        title_frame: Option<(usize, i32)>,
    ) {
        let grid = menu_grid_hit(45, 108, 400, 22, 1, 11, true, mx, my);
        for row in 0..count.min(11) {
            let hovered = grid == Some(row as i32);
            let bank = &self.ui.load_banks[usize::from(hovered)];
            if let Some(spr) = bank.get(scroll + row) {
                draw_sprite(s, spr, 0x37, 0x6e + 22 * row as i32);
            }
        }
        if let Some((frame, x)) = title_frame {
            if let Some(spr) = self.ui.load_banks[1].get(frame) {
                draw_sprite(s, spr, x, 0x3c);
            }
        }
    }

    /// The credits `FUN_0041a72c`: OBJETENMAIN = 0, the ambient sounds
    /// stop; with a music handle and a track other than 22 the music
    /// switches to track 26; the pages of CREDITS.TXT, each presented after
    /// its time; Escape ends them. Then the music fades out and the screen
    /// is cleared.
    pub async fn play_credits(&mut self) {
        self.vars.set("objetenmain", 0);
        self.stop_ambient();
        if self.music.handle_set && self.music.open_track != 22 {
            self.music_switch_track(26).await;
        }
        // Read at each run from the disc in the drive by the 1997 builds of
        // the English May code; loaded with the menus at start-up, from the
        // boot disc (disc 1 here), by the others (ExeBuild::credits_read_per_run).
        const CREDITS: &str = "SPRLIST\\CREDITS.TXT";
        let text = if self.game.exe.build.credits_read_per_run {
            self.read_from_current_disc(CREDITS).await
        } else {
            self.read(CREDITS).await
        }
        .unwrap_or_default();
        // The port's `ß` fix, for consistency (no known CREDITS.TXT holds 0xe1).
        let text = spell_sharp_s(&text, self.enhancements().minor_bug_fixes);
        let events = credits_events(&text, &self.game.exe.credit_widths);
        let start = self.sys.millis();
        let mut buffer = vec![0u16; N];
        while self.sys.poll_event().is_some() {}
        'pages: for ev in events {
            match ev {
                CreditsEvent::Image(n) => {
                    buffer = self.load_picture(&format!("CREDIT{n:02}.TGA")).await;
                }
                CreditsEvent::Glyph { blue, x, y, frame } => {
                    let bank = if blue {
                        &self.ui.credbleu
                    } else {
                        &self.ui.credblan
                    };
                    if let Some(spr) = usize::try_from(frame).ok().and_then(|f| bank.get(f)) {
                        draw_sprite(&mut buffer, spr, x, y);
                    }
                }
                CreditsEvent::Present(tick) | CreditsEvent::Clear(tick) => {
                    while self.sys.millis() - start < u64::from(tick) * 5 {
                        if self.sys.should_quit() || self.poll_menu_input().escape {
                            break 'pages;
                        }
                        self.sys.frame().await;
                    }
                    if matches!(ev, CreditsEvent::Present(_)) {
                        self.present(&buffer, false).await;
                    }
                }
            }
        }
        self.music_stop_command().await;
        self.present(&vec![0u16; N], false).await;
    }

    /// The end of the game (`FUN_0041a66c`, Exit = 86): `images\end.tga`
    /// for up to 0x578 ticks (7 s) or a left click, then the credits.
    pub async fn end_of_game(&mut self) {
        self.stop_ambient();
        let end = self.load_picture("END.TGA").await;
        let start = self.sys.millis();
        while self.sys.poll_event().is_some() {}
        while !self.sys.should_quit() && self.sys.millis() - start < 0x578 * 5 {
            if self.poll_menu_input().left_press {
                break;
            }
            self.present(&end, false).await;
        }
        self.play_credits().await;
    }
}

/// The chapter a LOAD row of the Chinese builds loads: entry `row` of the
/// chapter table (atlantis-cd-zh-tw-696832 0x00486619, read @ 0x0041d432
/// with no bound), which past its 74 entries reads on into the width table
/// that follows it (0x00486663): a 75th checkpoint (`p<L>99.gam`) loads
/// chapter 80. The picker lists at most 0x80 files, so the read stays in
/// the two tables.
pub fn load_list_chapter(chapters: &[u8], widths: &[u8], row: usize) -> u8 {
    chapters.iter().chain(widths).nth(row).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes() {
        assert_eq!(ingame_menu_outcome(Some(0), 6, 3), MenuOutcome::Reload);
        assert_eq!(ingame_menu_outcome(Some(0), 7, 3), MenuOutcome::Resume);
        assert_eq!(ingame_menu_outcome(Some(1), 6, 3), MenuOutcome::Resume);
        assert_eq!(ingame_menu_outcome(Some(2), 6, 3), MenuOutcome::Quit);
        assert_eq!(ingame_menu_outcome(Some(2), 6, 4), MenuOutcome::Resume);
        assert_eq!(ingame_menu_outcome(None, 6, 3), MenuOutcome::Resume);
    }

    /// A LOAD row of the Chinese builds loads its position's entry of the
    /// chapter table, past it the width table's.
    #[test]
    fn load_list_chapters_by_row() {
        let chapters = [2, 3, 4];
        let widths = [80, 98];
        let got: Vec<u8> = (0..6)
            .map(|r| load_list_chapter(&chapters, &widths, r))
            .collect();
        assert_eq!(got, [2, 3, 4, 80, 98, 0]);
    }
}

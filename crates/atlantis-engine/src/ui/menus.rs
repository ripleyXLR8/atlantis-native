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

use atlantis_data::saves::{checkpoint_chapter, epi_episode_name, MAX_PLAYERS, NAME_MAX_CHARS};
use atlantis_data::sprite::read_tga15;

use super::credits::{credits_events, CreditsEvent};
use super::layout::{
    font_min_load_list_width, menu_grid_hit, menu_number, menu_volume_from_knob, menu_walk_hit,
    name_key_code, subtitle_toggle_click, walk_menu_text, FontMin, LoadPickerScroll, MenuWalk,
};
use super::{dim_screen, draw_sprite, Screen};
use crate::engine::Engine;
use crate::music::apc_pcm;
use crate::sys::{Input, Key, MouseButton};
use crate::warp::{SCREEN_H, SCREEN_W};

const N: usize = SCREEN_W * SCREEN_H;

/// Windows' default double-click interval (the window class has
/// CS_DBLCLKS; the original reads WM_LBUTTONDBLCLK).
const DOUBLE_CLICK_MS: u64 = 500;

/// The input of one frame of a menu loop.
#[derive(Default)]
pub(crate) struct MenuInput {
    pub left_press: bool,
    pub right_press: bool,
    pub escape: bool,
    pub enter: bool,
    pub backspace: bool,
    pub chars: Vec<char>,
    pub up: bool,
    pub down: bool,
}

/// A loaded menu file and its walk state.
pub(crate) struct Menu {
    pub text: Vec<u8>,
    pub number: i32,
    pub walk: MenuWalk,
}

impl Menu {
    /// Walks the file (`FUN_0041fb6c`) with the hovered line and the
    /// options toggles.
    pub fn walk(&mut self, hover: Option<usize>, toggle_a: i32, toggle_b: i32) {
        let hover = hover.map_or(-1, |h| h as i32);
        walk_menu_text(
            &self.text,
            self.number,
            hover,
            toggle_a,
            toggle_b,
            &mut self.walk,
        );
    }

    /// The record under the point (`FUN_004210c0`).
    pub fn hit(&self, x: i32, y: i32) -> Option<usize> {
        menu_walk_hit(&self.walk.records, x, y)
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
                Input::KeyDown(Key::Escape) => mi.escape = true,
                Input::KeyDown(Key::Enter) => mi.enter = true,
                Input::KeyDown(Key::Backspace) => mi.backspace = true,
                Input::KeyDown(Key::Up) => mi.up = true,
                Input::KeyDown(Key::Down) => mi.down = true,
                Input::KeyDown(Key::Space) => mi.chars.push(' '),
                Input::KeyDown(Key::Char(c)) => mi.chars.push(c),
                _ => {}
            }
        }
        mi
    }

    /// `SPRLIST\<name>`, `None` when the archive lacks it.
    pub(crate) async fn load_menu(&self, name: &str) -> Option<Menu> {
        let text = self.read(&format!("SPRLIST\\{name}")).await?;
        self.emit(crate::engine::Event::Menu(name.to_string()));
        let number = menu_number(name).map_or(-1, |n| n as i32);
        let mut m = Menu {
            text,
            number,
            walk: MenuWalk::default(),
        };
        m.walk(None, 0, 0);
        Some(m)
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
    /// the "new player" row, delete one. Returns the player's row, `None`
    /// when the game is quit (window closed, or Escape: the port's way out).
    pub async fn player_screen(&mut self) -> Option<usize> {
        let bg = self.load_picture("PREINTRO.TGA").await;
        let mut menu = self.load_menu("SELEMENU.TXT").await?;
        // The records OK (5) and DELETE (6): the title, four sprites, then
        // the two labels (FUN_0041dea4 tests them by number).
        const OK: usize = 5;
        const DELETE: usize = 6;
        // The rows: FUN_004211f0 at (175, 248), cells 320 x 22, one column.
        let (grid_x, grid_y, grid_w, grid_h) = (175, 248, 320, 22);
        let mut selected: Option<usize> = None;
        let mut typed: Option<Vec<u8>> = None; // on the "new player" row
        let names_1997 = self.game.exe.build.players_1997;
        let list_font = if names_1997 {
            FontMin::PlayerList1997
        } else {
            FontMin::PlayerList
        };
        let mut last_click: Option<(usize, u64)> = None;
        self.wait_buttons_up().await;
        while self.sys.poll_event().is_some() {}
        loop {
            if self.sys.should_quit() {
                return None;
            }
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let rows = self.players.count as i32;
            let hover_row = menu_grid_hit(grid_x, grid_y, grid_w, grid_h, 1, rows, true, mx, my)
                .filter(|&r| r < rows)
                .map(|r| r as usize);
            let hover_rec = menu.hit(mx, my);
            let mut commit = false;
            let mut leave = false;
            if mi.escape {
                if typed.is_some() {
                    typed = None;
                    selected = None;
                } else {
                    return None;
                }
            }
            // Name entry, only on the "new player" row (@ 0x0041e283):
            // codes 0x20..0xa7, a leading space refused; at 16 characters
            // the last one is replaced; backspace restores a dot. The 1997
            // build takes 0x20..0x7a and stores a..z as A..Z
            // (atlantis-cd-en-688128 FUN_0041d6d0 @ 0x0041dab1..0x0041daf8).
            if let Some(buf) = typed.as_mut() {
                for &c in &mi.chars {
                    let mut code = name_key_code(c);
                    let last = if names_1997 { 0x7a } else { 0xa7 };
                    if !(0x20..=last).contains(&code) || (buf.is_empty() && code == b' ') {
                        continue;
                    }
                    if names_1997 {
                        code = code.to_ascii_uppercase();
                    }
                    if buf.len() >= NAME_MAX_CHARS {
                        buf.pop();
                    }
                    buf.push(code);
                }
                if mi.backspace {
                    buf.pop();
                }
                if mi.enter {
                    commit = true;
                }
            }
            if mi.left_press {
                let now = self.sys.millis();
                if hover_rec == Some(OK) {
                    if typed.is_some() {
                        commit = true;
                    } else if selected.is_some() {
                        leave = true;
                    }
                } else if hover_rec == Some(DELETE) {
                    if let (Some(row), None) = (selected, &typed) {
                        if self.confirm(&bg).await {
                            self.delete_player(row);
                            selected = None;
                        }
                        self.wait_buttons_up().await;
                    }
                } else if let Some(row) = hover_row {
                    let double =
                        last_click.is_some_and(|(r, t)| r == row && now - t <= DOUBLE_CLICK_MS);
                    last_click = Some((row, now));
                    if self.players.is_new_row(row) {
                        if typed.is_none() {
                            typed = Some(Vec::new());
                        }
                        selected = None;
                        if double {
                            commit = true;
                        }
                    } else {
                        typed = None;
                        selected = Some(row);
                        if double {
                            leave = true;
                        }
                    }
                }
            }
            if commit {
                if let Some(buf) = &typed {
                    if let Some(row) = self.players.create(buf) {
                        self.save_players();
                        return Some(row);
                    }
                }
            }
            if leave {
                if let Some(row) = selected {
                    self.save_players();
                    return Some(row);
                }
            }

            // The frame: the backdrop, the menu, the rows (FUN_00420d8c:
            // x 185, y 270 + 22 j, FONTMIN2 under the mouse), the name field
            // (FUN_00420f60 at (160, 190)).
            menu.walk(hover_rec, 0, 0);
            let mut s = bg.clone();
            self.ui.draw_menu(&mut s, &menu.walk);
            for j in 0..self.players.count {
                let name = self.players.name(j).to_vec();
                self.ui.draw_font_min(
                    &mut s,
                    list_font,
                    &name,
                    185,
                    grid_y + (j as i32 + 1) * grid_h,
                    hover_row == Some(j),
                );
            }
            let field = match (&typed, selected) {
                (Some(buf), _) => {
                    let mut f = buf.clone();
                    f.resize(NAME_MAX_CHARS, b'.');
                    Some(f)
                }
                (None, Some(row)) => Some(self.players.name(row).to_vec()),
                _ => None,
            };
            if let Some(f) = field {
                self.ui
                    .draw_font_min(&mut s, FontMin::NameField, &f, 160, 190, true);
            }
            self.present(&s, true).await;
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
    /// (record 3), false for Cancel (record 4) or Escape (the port's).
    pub(crate) async fn confirm(&mut self, bg: &[u16]) -> bool {
        let Some(mut menu) = self.load_menu("SUREMENU.TXT").await else {
            return true;
        };
        self.wait_buttons_up().await;
        loop {
            if self.should_abort() {
                return false;
            }
            let mi = self.poll_menu_input();
            if mi.escape {
                return false;
            }
            let (mx, my) = self.sys.mouse();
            let hit = menu.hit(mx, my).filter(|&r| r == 3 || r == 4);
            if mi.left_press {
                if let Some(r) = hit {
                    self.wait_buttons_up().await;
                    return r == 3;
                }
            }
            menu.walk(hit, 0, 0);
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
        self.wait_buttons_up().await;
        while self.sys.poll_event().is_some() {}
        let mut outcome = MenuOutcome::Resume;
        loop {
            if self.should_abort() {
                break;
            }
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let hit = menu.hit(mx, my);
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
            menu.walk(hit, 0, 0);
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
        let mut changed = false;
        loop {
            if self.should_abort() {
                break;
            }
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let hit = menu.hit(mx, my).filter(|r| [1, 3, 5, 6].contains(r));
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
                hit,
                i32::from(self.subtitles),
                i32::from(self.settings.omni3d_mode2),
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
        for t in 0..4 {
            match menu.walk.records.get(5 + t) {
                Some(r) if self.son_knob_x[t] == 0 => self.son_knob_x[t] = i32::from(r.x),
                _ => {}
            }
        }
        let mut grabbed: Option<usize> = None;
        let mut commit = false;
        loop {
            if self.should_abort() {
                break;
            }
            for t in 0..4 {
                if let Some(r) = menu.walk.records.get_mut(5 + t) {
                    r.x = self.son_knob_x[t] as i16;
                }
            }
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let hit = menu.hit(mx, my);
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
            for t in 0..4 {
                if let Some(r) = menu.walk.records.get_mut(5 + t) {
                    r.x = self.son_knob_x[t] as i16;
                }
            }
            menu.walk(hit, 0, 0);
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
    pub(crate) fn player_checkpoints(&self) -> Vec<u8> {
        let letter = self.players.letters[self.player.min(MAX_PLAYERS - 1)];
        let mut v: Vec<u8> = self
            .saves
            .list()
            .iter()
            .filter_map(|n| checkpoint_chapter(n, letter))
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
    /// file), 6 OK and a double click on a row end it for a reload; 7
    /// Cancel. Returns the record it ended on (255 for a double click).
    pub(crate) async fn load_picker(&mut self, bg: &[u16]) -> u8 {
        let Some(mut menu) = self.load_menu("LOADMENU.TXT").await else {
            return 7;
        };
        let chapters = self.player_checkpoints();
        let names: Vec<Vec<u8>> = chapters
            .iter()
            .map(|&c| epi_episode_name(&self.epi_text, u32::from(c)))
            .collect();
        let count = chapters.len();
        let clamp = self.game.exe.build.load_picker_clamp;
        let mut sb = LoadPickerScroll::new(count as i32, clamp);
        let mut title: Option<(Vec<u8>, i32)> = None;
        let mut selected: Option<usize> = None;
        let mut last_click: Option<(usize, u64)> = None;
        let mut track_latched = false;
        let player = self.player.min(MAX_PLAYERS - 1);
        let choose = |e: &mut Engine, i: usize, title: &mut Option<(Vec<u8>, i32)>| {
            e.players.chapters[player] = chapters[i];
            let w = font_min_load_list_width(&names[i]);
            *title = Some((names[i].clone(), (640 - w) / 2));
        };
        loop {
            if self.should_abort() {
                return 7;
            }
            let mi = self.poll_menu_input();
            let (mx, my) = self.sys.mouse();
            let down = self.sys.buttons().0;
            // Port keys (the original reads none here): Escape cancels,
            // Enter is OK, Up / Down a row click.
            if mi.escape {
                return 7;
            }
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
                choose(self, i, &mut title);
            }
            if mi.left_press {
                // WM_LBUTTONDOWN latches the record under the pointer
                // (0x00419c47): the track when strictly inside it.
                track_latched = menu.hit(mx, my) == Some(1);
            }
            if !down {
                track_latched = false;
            }
            sb.frame(track_latched, down, my);
            let scroll = usize::from(sb.offset);
            // A row counts when row + offset < count (0x0041d37b); the
            // 1997 build tests the row alone (atlantis-cd-en-688128
            // FUN_0041c9a8 @ 0x0041cd87) and takes entry row + offset
            // (0x0041cdaa). Past the list it would read an empty record
            // (its chapter -1 then walks EPI.TXT without an end): the port
            // takes no such row.
            let row_hover = menu_grid_hit(45, 108, 400, 22, 1, 11, true, mx, my)
                .filter(|&g| g < 11 && (clamp || (g as usize) < count))
                .map(|g| scroll + g as usize)
                .filter(|&r| r < count);
            let button = menu.hit(mx, my).filter(|r| [5, 6, 7].contains(r));
            if mi.left_press && !track_latched {
                if let Some(r) = row_hover {
                    selected = Some(r);
                    choose(self, r, &mut title);
                    let now = self.sys.millis();
                    if last_click.is_some_and(|(l, t)| l == r && now - t <= DOUBLE_CLICK_MS) {
                        self.wait_buttons_up().await;
                        return 255;
                    }
                    last_click = Some((r, now));
                } else if let Some(b) = button {
                    self.wait_buttons_up().await;
                    if b == 5 && count > 0 {
                        self.players.chapters[player] = self.chapter as u8;
                    }
                    return b as u8;
                }
            }
            // The knob: drawn at its stored record position.
            if let Some(r) = menu.walk.records.get_mut(4) {
                if sb.knob_x != 0 {
                    r.x = sb.knob_x as i16;
                }
                r.y = sb.knob_y as i16;
            }
            menu.walk(button, 0, 0);
            let mut s = bg.to_vec();
            self.ui.draw_menu(&mut s, &menu.walk);
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
        let text = self.read("SPRLIST\\CREDITS.TXT").await.unwrap_or_default();
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
}

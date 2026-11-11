//! The game's saves and its start and end (atlantis4-gog-en-2363910):
//! the `SaveGame` worker writing to the backup folder, `Game::load`'s save
//! branch, `Game::leave` / `Game::enter`, the main menu's *new game* and
//! *continue*, the loading menu's chapter slots, `FinishGame`.
//!
//! The menus call, in the original:
//! - *continue* (@ 0x00565e20): `Game::enter`, `Game::load("save.xml")`;
//! - *new game* (@ 0x00565af0 / 0x005655c0): the twelve saves removed
//!   ([`saves::NEW_GAME_REMOVES`]), then the game;
//! - a chapter slot (@ 0x00518460, confirmed @ 0x00519f40): `Game::enter`,
//!   `Game::load("<N>.xml")`;
//! - the in-game menu's *main menu* and `FinishGame`'s end
//!   (@ 0x00526910): `Game::leave` (the autosave when a node is shown).
//!
//! Here [`Game::load_from`], [`Game::start_new_game`], [`Game::leave`]
//! and [`Game::chapter_slots`] are those entries; the screens are the
//! menus' own.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use atlantis4_script::{
    load_game, BootValue, BootVars, EngineRef, Options, SaveOpen, SavedGlobal, Script, ScriptError,
};

use crate::ending::Planks;
use crate::game::Game;
use crate::saves::{
    self, BackupFolder, SaveSlot, SaveStore, AUTOSAVE, CHAPTER_SLOTS, NEW_GAME_REMOVES,
};
use crate::savexml::save_text;

fn text(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

fn boot_str(v: &Option<BootValue>) -> Option<String> {
    match v {
        Some(BootValue::Str(s)) => Some(text(s)),
        _ => None,
    }
}

impl Game {
    /// The boot variables the saves need: `firstWarpPath`, and the backup
    /// folder from `backupPathTemplate` and `applicationTitle` under
    /// Native's data folder `root` (`None`: the saves stay in memory).
    pub fn configure_saves(&mut self, boot: &BootVars, root: Option<&Path>) {
        if let Some(BootValue::Str(s)) = &boot.first_warp_path {
            self.first_warp_path = s.clone();
        }
        let template = boot_str(&boot.backup_path_template);
        let app = boot_str(&boot.application_title);
        self.backup = match (root, template, app) {
            (Some(r), Some(t), Some(a)) => {
                let dir = saves::native_backup_dir(r, &t, &a);
                log::info!("saves in {}", dir.display());
                BackupFolder::Dir(dir)
            }
            (Some(_), t, a) => {
                // "TeGetBackupPath: no backupPathTemplate set" / "no appName
                // set": the original then uses the path as it is.
                log::warn!("no backup folder (template {t:?}, name {a:?}): saves kept in memory");
                BackupFolder::default()
            }
            (None, ..) => BackupFolder::default(),
        };
    }

    /// The current node's path as the node change set it (`WarpName`).
    pub fn warp_name(&self) -> &[u8] {
        &self.warp_name
    }

    /// The seconds played: `game+0x4608` + the timer since `Game::enter`
    /// (whole µs) / 1,000,000 (@ 0x00524535..0x00524558).
    pub fn played_time(&self) -> f64 {
        let us = ((self.clock_ms() - self.played_start_ms) * 1000.0).round();
        us / 1_000_000.0 + self.played_base
    }

    /// The `SaveGame` worker (@ 0x00524100) for `file`: nothing while no
    /// node was entered (the warp name is empty); else the text (kept in
    /// [`Game::last_save`]) written to the backup folder. True when
    /// written.
    pub fn save_to(&mut self, file: &str, globals: &[SavedGlobal]) -> bool {
        if self.warp_name.is_empty() {
            log::info!("SaveGame({file}): no node yet, nothing written");
            return false;
        }
        let parts = self.save_parts();
        let text = save_text(&parts, globals);
        let written = self.backup.write(file, &saves::to_disk(&text));
        log::info!(
            "SaveGame({file}): {} bytes, {} globals{}",
            text.len(),
            globals.len(),
            if written { "" } else { ", not written" }
        );
        self.last_save = Some((file.to_string(), text));
        written
    }

    /// `Game::load`'s test and parse of `Backup/<file>` (@ 0x00529fd3..
    /// 0x0052a4c1).
    pub(crate) fn open_backup(&mut self, file: &str) -> SaveOpen {
        let logical = format!("Backup/{file}");
        let Some(bytes) = self.backup.read(&logical) else {
            return SaveOpen::Missing;
        };
        match saves::parse(&bytes) {
            Ok(els) => {
                let g = saves::globals(&els);
                self.pending_save = Some(els);
                SaveOpen::Globals(g)
            }
            Err(e) => {
                log::warn!("Failed to load file \"{logical}\" ({e})");
                SaveOpen::Broken
            }
        }
    }

    /// The save's parts after the globals, in `Game::load`'s order
    /// (@ 0x0052a4d2..0x0052a7ba); the warp path.
    pub(crate) fn restore_backup(&mut self) -> Vec<u8> {
        let els = self.pending_save.take().unwrap_or_default();
        // The slots of the layouts `Game::enter` loaded.
        self.gui();
        if let Some(e) = saves::top(&els, "Inventory") {
            self.inventory.load_xml(e);
        }
        if let Some(e) = saves::top(&els, "DocumentsBrowser") {
            self.documents.load_xml(e);
        }
        if let Some(e) = saves::top(&els, "Diary") {
            self.diary.load_xml(e);
        }
        if let Some(e) = saves::top(&els, "PuzzleTimer") {
            self.puzzle_timer.load_xml(e);
        }
        // Through the logical resolver (0x00553000), which leaves a warp's
        // path as it is.
        let warp = saves::top(&els, "WarpName")
            .and_then(|e| saves::attr_bytes(e, "name"))
            .unwrap_or_default();
        if let Some(o) = saves::top(&els, "Orientation") {
            // Before the node change, which keeps them. The limits in force
            // are the previous game's (`[UNKNOWN]`): free here.
            self.view.reset_limits();
            self.view.set_yaw(saves::angle(Some(o), "x"));
            self.view.set_pitch(saves::angle(Some(o), "y"));
        }
        if let Some(path) = saves::top(&els, "Music").and_then(|e| saves::attr_bytes(e, "path")) {
            // `load`, `TeMusic::play`, `volume(1.0)`: `PlayMusic`'s
            // sequence; the music was stopped by `Game::leave`, so the
            // track starts from its beginning.
            self.sounds.play_music(&self.files, &text(&path));
        }
        if let Some(v) = saves::top(&els, "PlayedTime")
            .and_then(|e| e.attr("value"))
            .and_then(|v| saves::scan_double(v.as_bytes()))
        {
            self.played_base = v;
        }
        warp
    }

    /// The engine's part of `Game::leave` (@ 0x00525f50, after the
    /// autosave): the music stopped, the scene deleted, the in-game
    /// screens, dialogs, inventory, documents and diary dropped (their
    /// layouts are loaded again by `Game::enter`), the movie gone.
    pub fn leave_game(&mut self) {
        self.sounds.leave_game();
        // `game+0xda0` is not reset by `Game::leave` or `Game::enter`
        // `[UNKNOWN]`; dormant in this game, reset with the rest.
        self.drop_game_state();
        self.pending_save = None;
    }

    /// `Game::enter` (@ 0x00528c10): the played time 0 and its timer
    /// started (@ 0x00529d3c..0x00529d58); the in-game layouts are loaded
    /// at their first use here.
    pub fn enter_game(&mut self) {
        self.played_base = 0.0;
        self.played_start_ms = self.clock_ms();
        // A new game's view (@ 0x005296bf): yaw = pitch = 0, the limits
        // reset; a save's `Orientation` follows.
        self.view = crate::view::View::default();
        self.ended = false;
        self.planks = None;
    }

    /// `FinishGame` (@ 0x0052bc30): the planks close over the game; their
    /// end is [`Game::take_ended`]. With [`Game::app_planks`], the
    /// application's planks instead ([`Game::take_finish`]): the portrait
    /// close (0x0051f3c0) whose turn fires `App+0xdb0` (`RotationAnim`
    /// `+0x700`), `Game::onFinished`'s signal.
    pub fn finish_game(&mut self) {
        if self.app_planks {
            self.finish_requested = true;
            return;
        }
        let now = self.clock_ms();
        let (planks, sound) = Planks::close(&self.files, now);
        self.planks = Some(planks);
        if let Some(s) = sound {
            // 0x0051f3c0 reads `portraitCloseSound` and plays it (0x004f70b0;
            // volume 1 `[INFERRED]`).
            self.sounds.play_free_sound(&self.files, &s, 1.0);
        }
    }

    /// True once after `FinishGame`'s planks closed: the application then
    /// runs [`Game::leave`] and shows its main menu.
    pub fn take_ended(&mut self) -> bool {
        std::mem::take(&mut self.ended)
    }

    /// True once after `FinishGame` when the application runs the planks:
    /// it closes them (portrait) and, at their turn, runs [`Game::leave`]
    /// and enters its main menu, without the credits.
    pub fn take_finish(&mut self) -> bool {
        std::mem::take(&mut self.finish_requested)
    }

    /// The main menu's *continue* is offered (`Backup/save.xml` exists,
    /// 0x00566000 tests it).
    pub fn has_autosave(&self) -> bool {
        self.backup.exists(&format!("Backup/{AUTOSAVE}"))
    }

    /// The loading menu's slots `0`..`5`.
    pub fn chapter_slots(&self) -> Vec<SaveSlot> {
        (0..CHAPTER_SLOTS)
            .map(|i| saves::slot(&self.backup, i))
            .collect()
    }

    /// `Game::leave` (@ 0x00525f50) of the game in `game` run by
    /// `script`: the autosave `save.xml` when a node is shown, then the
    /// engine's state dropped. The caller drops `script` after (the
    /// original closes the Lua state; no script defines `Unload`).
    pub fn leave(game: &Rc<RefCell<Game>>, script: Option<&Script>) {
        if let Some(s) = script {
            if game.borrow().warp.is_some() {
                match s.globals() {
                    Ok(g) => {
                        game.borrow_mut().save_to(AUTOSAVE, &g);
                    }
                    Err(e) => log::warn!("autosave: {e}"),
                }
            }
        }
        game.borrow_mut().leave_game();
    }

    /// `Game::enter` then `Game::load(file)`: the save `Backup/<file>`
    /// loaded (a missing file starts a new game at `firstWarpPath`; a
    /// broken one leaves a game without node, as the original). `old`,
    /// the game running, is left first ([`Game::leave`]).
    pub fn load_from(
        game: &Rc<RefCell<Game>>,
        old: Option<&Script>,
        options: Options,
        file: &str,
    ) -> Result<Script, ScriptError> {
        Game::leave(game, old);
        game.borrow_mut().enter_game();
        let first = game.borrow().first_warp_path.clone();
        let engine: EngineRef = game.clone();
        load_game(&engine, options, file.as_bytes(), &first)
    }

    /// The main menu's new game (@ 0x005655c0): the autosave and the
    /// chapter saves removed, then a game from `firstWarpPath`.
    pub fn start_new_game(
        game: &Rc<RefCell<Game>>,
        old: Option<&Script>,
        options: Options,
    ) -> Result<Script, ScriptError> {
        Game::leave(game, old);
        {
            let mut g = game.borrow_mut();
            for f in NEW_GAME_REMOVES {
                g.backup.remove(&format!("Backup/{f}"));
            }
        }
        // The name `Game::load` then gets is the menu's (`[UNKNOWN]`);
        // every save is gone, so it is a new game whatever it is.
        Game::load_from(game, None, options, AUTOSAVE)
    }
}

/// The game's backup folder as the menus see it ([`SaveStore`]): the
/// names are the folder's files (`save.xml`, `3.xml`), saving is the
/// `SaveGame` worker ([`Game::save_to`]) and loading `Game::enter` +
/// `Game::load` ([`Game::load_from`]; a missing file is a new game).
pub struct GameSaves {
    pub game: Rc<RefCell<Game>>,
    /// The game scripts' options.
    pub options: Options,
}

impl SaveStore for GameSaves {
    fn exists(&self, name: &str) -> bool {
        self.game.borrow().backup.exists(&format!("Backup/{name}"))
    }

    fn played_time(&self, name: &str) -> Option<f64> {
        let bytes = self.game.borrow().backup.read(name)?;
        let els = saves::parse(&bytes).ok()?;
        saves::top(&els, "PlayedTime")
            .and_then(|e| e.attr("value"))
            .and_then(|v| saves::scan_double(v.as_bytes()))
    }

    fn delete(&mut self, name: &str) {
        self.game.borrow_mut().backup.remove(&format!("Backup/{name}"));
    }

    fn save(
        &mut self,
        game: &Rc<RefCell<Game>>,
        script: &mut Script,
        name: &str,
    ) -> Result<(), String> {
        let globals = script.globals().map_err(|e| format!("SaveGame({name}): {e}"))?;
        if game.borrow_mut().save_to(name, &globals) {
            Ok(())
        } else {
            Err(format!("SaveGame({name}): nothing written"))
        }
    }

    fn load(&mut self, game: &Rc<RefCell<Game>>, name: &str) -> Result<Script, String> {
        Game::load_from(game, None, self.options, name).map_err(|e| format!("load {name}: {e}"))
    }
}

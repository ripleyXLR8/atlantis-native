//! What the menus need from the saves, as a trait: the save format and
//! its loading live elsewhere (the `a4/saves` work); the menus only ask
//! whether a file exists, its played time, to delete, to save and to load.
//!
//! The original's files (atlantis4-gog-en-2363910, `STATIC`), all in the
//! backup folder (`Backup/`):
//!
//! - `save.xml`: the "continue" save. The main menu's `continueGameButton`
//!   is enabled only when it exists (0x005666e3); the in-game menu's
//!   `mainMenuButton` writes it before going back (0x0057cc50 →
//!   `Game::save("save.xml")` 0x00524100); "continue" loads it
//!   (0x00565e20 → `Game::enter` then `Game::load("save.xml")`).
//! - `0.xml` .. `5.xml`: the chapter saves the story writes (`SaveGame`),
//!   the six slots of the load screen (`LoadingMenu::SaveButton`
//!   0x005185b0: `Backup/<i>.xml`, enabled when it exists, sorted by its
//!   `PlayedTime` element's `value`, 1e43 when absent).
//! - "New game" (0x005655c0) deletes `save.xml` and `0.xml` .. `10.xml`
//!   before starting (then loads the missing `save.xml`: a new game).

use std::cell::RefCell;
use std::rc::Rc;

use atlantis4_script::Script;

use crate::Game;

/// The files "new game" deletes, in order (0x005655f5..0x00565a73).
pub const NEW_GAME_DELETES: [&str; 12] = [
    "save.xml", "0.xml", "1.xml", "2.xml", "3.xml", "4.xml", "5.xml", "6.xml", "7.xml", "8.xml",
    "9.xml", "10.xml",
];

/// The "continue" save's name.
pub const CONTINUE: &str = "save.xml";

/// The played time of a save without one (0x0051877b).
pub const NO_PLAYED_TIME: f64 = 1e43;

/// The saves as the menus see them. Names are the backup folder's file
/// names (`save.xml`, `3.xml`). The menus call only the first three; the
/// application calls [`SaveStore::save`] and [`SaveStore::load`] where the
/// original calls `Game::save` (0x00524100) and `Game::load` (0x00529e30).
pub trait SaveStore {
    /// Whether the file exists.
    fn exists(&self, name: &str) -> bool;
    /// Its `PlayedTime` element's `value`, if it has one.
    fn played_time(&self, name: &str) -> Option<f64>;
    /// Deletes the file (nothing if it is missing).
    fn delete(&mut self, name: &str);
    /// Writes the game in play (its state and `script`'s globals) to `name`.
    fn save(
        &mut self,
        game: &Rc<RefCell<Game>>,
        script: &mut Script,
        name: &str,
    ) -> Result<(), String>;
    /// `Game::load(name)` of an existing file: the game's state read back
    /// and its script state made; `Err` leaves the caller to decide.
    fn load(&mut self, game: &Rc<RefCell<Game>>, name: &str) -> Result<Script, String>;
}

/// A stand-in until the save format is wired: files are only names in a
/// set (an existing folder's `*.xml` can seed it); saving records the
/// name, loading fails (`[UNKNOWN]` to the menus: the stub's caller starts
/// a new game instead and logs it).
#[derive(Clone, Debug, Default)]
pub struct StubSaves {
    pub names: Vec<String>,
    pub played: Vec<(String, f64)>,
}

impl StubSaves {
    /// The `.xml` files of a folder (none when it cannot be read).
    pub fn from_dir(dir: &std::path::Path) -> StubSaves {
        let mut s = StubSaves::default();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.ends_with(".xml") && n != crate::options::FILE {
                    s.names.push(n);
                }
            }
        }
        s.names.sort();
        s
    }

    /// Records a save of `name` (the stub writes nothing).
    pub fn record(&mut self, name: &str) {
        if !self.exists(name) {
            self.names.push(name.to_string());
        }
    }
}

impl SaveStore for StubSaves {
    fn exists(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    fn played_time(&self, name: &str) -> Option<f64> {
        self.played.iter().find(|(n, _)| n == name).map(|(_, t)| *t)
    }

    fn delete(&mut self, name: &str) {
        self.names.retain(|n| n != name);
    }

    fn save(&mut self, _: &Rc<RefCell<Game>>, _: &mut Script, name: &str) -> Result<(), String> {
        log::info!("save {name}: recorded (the save format is not wired)");
        self.record(name);
        Ok(())
    }

    fn load(&mut self, _: &Rc<RefCell<Game>>, name: &str) -> Result<Script, String> {
        Err(format!("load {name}: the save format is not wired"))
    }
}

/// The load screen's order of the six chapter saves (0x0051a8d3..
/// 0x0051a9e1): each inserted before the first already placed whose
/// played time is greater, so ascending and stable; missing ones count
/// as [`NO_PLAYED_TIME`].
pub fn chapter_order(store: &dyn SaveStore, count: usize) -> Vec<usize> {
    let time = |i: usize| {
        let n = format!("{i}.xml");
        if store.exists(&n) {
            store.played_time(&n).unwrap_or(NO_PLAYED_TIME)
        } else {
            NO_PLAYED_TIME
        }
    };
    let mut out: Vec<usize> = Vec::new();
    for i in 0..count {
        let t = time(i);
        let at = out.iter().position(|&j| time(j) > t).unwrap_or(out.len());
        out.insert(at, i);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapters_sort_by_played_time_missing_last() {
        let mut s = StubSaves::default();
        for n in ["0.xml", "2.xml", "3.xml"] {
            s.record(n);
        }
        s.played = vec![("0.xml".into(), 500.0), ("2.xml".into(), 100.0)];
        // 3.xml exists without a time: 1e43, as the missing ones, after
        // them in index order.
        assert_eq!(chapter_order(&s, 6), [2, 0, 1, 3, 4, 5]);
        s.delete("2.xml");
        assert!(!s.exists("2.xml"));
    }
}

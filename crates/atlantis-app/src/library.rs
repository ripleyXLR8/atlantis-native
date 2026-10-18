//! The game library (desktop): the game folders the player has used, one
//! per line in `games.txt` in the per-user data folder, the last played
//! first. Earlier versions remembered one folder in `game-folder.txt`; it
//! is read when there is no `games.txt` yet.

use std::path::{Path, PathBuf};

/// The library file, in the per-user data folder.
pub const LIBRARY_FILE: &str = "games.txt";
/// The file of earlier versions: the one game folder chosen last.
pub const OLD_FOLDER_FILE: &str = "game-folder.txt";

/// The game folders, the last played first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Library {
    folders: Vec<PathBuf>,
}

/// Whether two paths name the same folder: equal once trailing separators
/// are dropped (ignoring case on Windows), or the same folder on disk.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy();
        let t = s.trim_end_matches(['/', '\\']);
        let t = if t.is_empty() {
            &s[..1.min(s.len())]
        } else {
            t
        };
        if cfg!(windows) {
            t.to_lowercase()
        } else {
            t.to_string()
        }
    };
    if norm(a) == norm(b) {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

impl Library {
    /// A library of `folders` (duplicates dropped), for tests and callers
    /// that build one.
    pub fn from_folders(folders: impl IntoIterator<Item = PathBuf>) -> Library {
        let mut lib = Library::default();
        for f in folders {
            lib.add(&f);
        }
        lib
    }

    /// The library kept in `dir` (the per-user data folder): `games.txt`,
    /// else the folder of `game-folder.txt`, else empty.
    pub fn load(dir: &Path) -> Library {
        let text = std::fs::read_to_string(dir.join(LIBRARY_FILE))
            .or_else(|_| std::fs::read_to_string(dir.join(OLD_FOLDER_FILE)))
            .unwrap_or_default();
        Library::from_folders(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(PathBuf::from),
        )
    }

    /// Writes the library to `games.txt` in `dir` (created if needed). A
    /// folder whose path is not UTF-8 is left out.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let mut text = String::new();
        for f in &self.folders {
            match f.to_str() {
                Some(s) => {
                    text.push_str(s);
                    text.push('\n');
                }
                None => log::warn!("{}: not UTF-8, not kept in the library", f.display()),
            }
        }
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(LIBRARY_FILE), text)
    }

    /// [`Library::save`], a failure logged.
    pub fn save_logged(&self, dir: &Path) {
        if let Err(e) = self.save(dir) {
            log::warn!("{}: {e}", dir.join(LIBRARY_FILE).display());
        }
    }

    pub fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    pub fn len(&self) -> usize {
        self.folders.len()
    }

    pub fn is_empty(&self) -> bool {
        self.folders.is_empty()
    }

    /// Where `dir` is in the list.
    pub fn position(&self, dir: &Path) -> Option<usize> {
        self.folders.iter().position(|f| same_folder(f, dir))
    }

    /// Adds `dir` at the end when it is not listed yet; true when added.
    pub fn add(&mut self, dir: &Path) -> bool {
        if self.position(dir).is_some() {
            return false;
        }
        self.folders.push(dir.to_path_buf());
        true
    }

    /// `dir` was played: it moves to the top (added when not listed).
    pub fn played(&mut self, dir: &Path) {
        if let Some(i) = self.position(dir) {
            self.folders.remove(i);
        }
        self.folders.insert(0, dir.to_path_buf());
    }

    /// Takes entry `i` off the list (no file is touched).
    pub fn remove(&mut self, i: usize) -> Option<PathBuf> {
        (i < self.folders.len()).then(|| self.folders.remove(i))
    }

    /// Entry `i` now points to `dir` (the game moved); another entry for
    /// `dir` is dropped. Returns the index of `dir` afterwards.
    pub fn relocate(&mut self, i: usize, dir: &Path) -> usize {
        if i >= self.folders.len() {
            self.add(dir);
            return self.position(dir).unwrap_or(0);
        }
        self.folders[i] = dir.to_path_buf();
        let mut j = 0;
        let mut at = i;
        self.folders.retain(|f| {
            let keep = j == i || !same_folder(f, dir);
            if !keep && j < i {
                at -= 1;
            }
            j += 1;
            keep
        });
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("atlantis-library-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn loads_saves_and_migrates() {
        let d = tmp("migrate");
        assert!(Library::load(&d).is_empty());
        // The old file: its folder.
        std::fs::write(d.join(OLD_FOLDER_FILE), "/games/atlantis-gog\n").unwrap();
        let mut lib = Library::load(&d);
        assert_eq!(lib.folders(), [PathBuf::from("/games/atlantis-gog")]);
        assert!(lib.add(Path::new("/games/cd")));
        lib.save(&d).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.join(LIBRARY_FILE)).unwrap(),
            "/games/atlantis-gog\n/games/cd\n"
        );
        // games.txt wins over the old file once it exists.
        std::fs::write(d.join(OLD_FOLDER_FILE), "/elsewhere\n").unwrap();
        assert_eq!(Library::load(&d), lib);
        // Blank lines, spaces and duplicates are dropped.
        std::fs::write(
            d.join(LIBRARY_FILE),
            "\n  /games/cd  \n/games/cd/\n\n/games/iso\n",
        )
        .unwrap();
        assert_eq!(
            Library::load(&d).folders(),
            [PathBuf::from("/games/cd"), PathBuf::from("/games/iso")]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn played_moves_to_the_top() {
        let mut lib = Library::from_folders(["/a", "/b", "/c"].map(PathBuf::from));
        lib.played(Path::new("/c"));
        assert_eq!(lib.folders(), ["/c", "/a", "/b"].map(PathBuf::from));
        lib.played(Path::new("/d"));
        assert_eq!(lib.folders(), ["/d", "/c", "/a", "/b"].map(PathBuf::from));
        lib.played(Path::new("/d/"));
        assert_eq!(lib.len(), 4);
        assert!(!lib.add(Path::new("/a")));
        assert!(lib.add(Path::new("/e")));
        assert_eq!(lib.position(Path::new("/e")), Some(4));
    }

    #[test]
    fn removes_and_relocates() {
        let mut lib = Library::from_folders(["/a", "/b", "/c"].map(PathBuf::from));
        assert_eq!(lib.remove(1), Some(PathBuf::from("/b")));
        assert_eq!(lib.remove(5), None);
        assert_eq!(lib.folders(), ["/a", "/c"].map(PathBuf::from));
        // Moved to a new place.
        assert_eq!(lib.relocate(1, Path::new("/z")), 1);
        assert_eq!(lib.folders(), ["/a", "/z"].map(PathBuf::from));
        // Moved onto a folder listed already: one entry left.
        assert_eq!(lib.relocate(1, Path::new("/a")), 0);
        assert_eq!(lib.folders(), [PathBuf::from("/a")]);
    }

    #[test]
    fn missing_folders_stay_listed() {
        let d = tmp("missing");
        let gone = d.join("gone");
        let mut lib = Library::default();
        lib.add(&gone);
        lib.add(&d);
        lib.save(&d).unwrap();
        // The library keeps what it was given: the launcher shows the
        // folders that no longer open as not found.
        assert_eq!(Library::load(&d).folders(), [gone, d.clone()]);
        assert!(same_folder(&d, &d.join(".")));
        let _ = std::fs::remove_dir_all(&d);
    }
}
